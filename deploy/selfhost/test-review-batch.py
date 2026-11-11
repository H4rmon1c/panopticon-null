#!/usr/bin/env python3
"""Offline adversarial checks for the exact Colorado review packet gate."""

import copy
import hashlib
import importlib.util
import json
import sqlite3
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
from datetime import datetime, timedelta, timezone
from pathlib import Path

SCRIPT = Path(__file__).with_name("record-source-review-batch.py")
sys.path.insert(0, str(SCRIPT.parent))
spec = importlib.util.spec_from_file_location("review_batch", SCRIPT)
batch = importlib.util.module_from_spec(spec)
spec.loader.exec_module(batch)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def utc(value):
    return value.isoformat(timespec="microseconds").replace("+00:00", "Z")


class SyntheticPacket:
    def __init__(self, base):
        self.base = Path(base)
        self.private = self.base / ".pnull"
        self.private.mkdir(mode=0o700)
        self.private.chmod(0o700)
        self.packet_path = self.base / "packet.json"
        self.attestation_path = self.base / "attestation.json"
        self.packet = copy.deepcopy(batch.json_file(batch.PACKET))
        self.now = datetime.now(timezone.utc).replace(microsecond=0)
        self.connection = sqlite3.connect(self.private / "pnull.db")
        self.connection.executescript("""
            CREATE TABLE fetch_observations (
                id TEXT PRIMARY KEY, source_id TEXT NOT NULL,
                observation_json TEXT NOT NULL
            );
            CREATE TABLE source_reviews (
                id TEXT PRIMARY KEY, source_id TEXT NOT NULL,
                review_json TEXT NOT NULL
            );
        """)
        for index, row in enumerate(self.packet["sources"]):
            blocked = not row["release_cited"]
            body = b"User-agent: *\nDisallow: /\n" if blocked else b"robots.txt not found\n"
            body_sha = sha(body)
            path = self.private / "evidence" / "sha256" / body_sha[:2] / body_sha
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(body)
            receipt_id = f"fetch:offline-{index:02d}"
            captured_at = utc(self.now - timedelta(minutes=2))
            row.update({
                "robots_body_sha256": body_sha,
                "robots_body_bytes": len(body),
                "robots_fetch_receipt_id": receipt_id,
                "robots_captured_at_utc": captured_at,
            })
            observation = {
                "id": receipt_id,
                "source_id": row["archive_source_id"],
                "requested_url": row["robots_url"],
                "final_url": row["robots_url"],
                "method": "GET",
                "status_code": row["robots_http_status"],
                "body_digest": body_sha,
                "retrieved_at": captured_at,
                "resolved_ips": ["8.8.8.8"],
                "error": None,
            }
            self.connection.execute(
                "INSERT INTO fetch_observations VALUES (?, ?, ?)",
                (receipt_id, row["archive_source_id"], json.dumps(observation)),
            )
        self.connection.commit()
        self.attestation = {
            "schema_version": 1,
            "reviewer": "h4rmon1c",
            "expires_at": utc(self.now + timedelta(days=29)),
            "approved_source_ids": sorted(
                row["registry_source_id"] for row in self.packet["sources"]
                if row["release_cited"]
            ),
            "host_notes": {
                host: f"Reviewed {host}: HTTP 404 robots response is not permission; "
                      "terms scope is uncertain for this exact source and limited collection."
                for host in batch.REVIEW_HOSTS
            },
        }
        self.save()

    def save(self, bind=True):
        packet_bytes = json.dumps(self.packet, indent=2).encode()
        self.packet_path.write_bytes(packet_bytes)
        if bind:
            self.attestation["packet_sha256"] = sha(packet_bytes)
        self.attestation_path.write_text(json.dumps(self.attestation), encoding="utf-8")

    def preflight(self):
        return batch.preflight(
            self.packet_path, self.attestation_path, self.private, self.now,
        )

    def add_review(self, row, note=None):
        review = {
            "source_id": row["archive_source_id"],
            "reviewed_at": utc(self.now),
            "source_config_digest": row["source_config_sha256"],
            "reviewed_hosts": [batch.urlsplit(row["endpoint_url"]).hostname],
            "endpoint_patterns": [batch.urlsplit(row["endpoint_url"]).path],
            "robots_url": row["robots_url"],
            "robots_snapshot_digest": row["robots_body_sha256"],
            "robots_provenance": row["robots_fetch_receipt_id"],
            "reviewer": self.attestation["reviewer"],
            "note": note or batch.note_for(
                row,
                self.attestation["host_notes"][
                    batch.urlsplit(row["endpoint_url"]).hostname
                ],
                self.attestation["packet_sha256"],
            ),
            "expires_at": self.attestation["expires_at"],
            "minimum_interval_seconds": row["minimum_interval_seconds"],
            "restrictions": [],
            "terms_urls": [],
            "terms_snapshot_digests": [],
        }
        self.connection.execute(
            "INSERT INTO source_reviews VALUES (?, ?, ?)",
            (f"review:{row['archive_source_id']}", row["archive_source_id"], json.dumps(review)),
        )
        self.connection.commit()

    def close(self):
        self.connection.close()


class ReviewBatchTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.state = SyntheticPacket(self.temp.name)

    def tearDown(self):
        self.state.close()
        self.temp.cleanup()

    def row(self, release=True):
        return next(row for row in self.state.packet["sources"]
                    if row["release_cited"] is release)

    def refuses(self, fragment):
        self.state.save()
        with self.assertRaisesRegex(batch.ReviewBatchError, fragment):
            self.state.preflight()

    def test_valid_packet_only_plans_twelve_release_sources(self):
        plan = self.state.preflight()
        self.assertEqual(len(plan), 12)
        self.assertEqual({state for _, _, state in plan}, {"ready"})
        self.assertTrue(all(row["release_cited"] for row, _, _ in plan))
        self.assertTrue(all("404" in note and "terms" in note for _, note, _ in plan))

    def test_packet_digest_binds_attestation(self):
        self.row()["endpoint_url"] += "&other=1"
        self.state.save(bind=False)
        with self.assertRaisesRegex(batch.ReviewBatchError, "does not bind"):
            self.state.preflight()

    def test_changed_endpoint_refused_even_with_new_attestation_digest(self):
        self.row()["endpoint_url"] += "&other=1"
        self.refuses("URL or source policy changed")

    def test_changed_config_digest_refused(self):
        self.row()["source_config_sha256"] = "0" * 64
        self.refuses("URL or source policy changed")

    def test_changed_fixture_digest_refused(self):
        self.row()["preserved_fixture_sha256"] = "0" * 64
        self.refuses("fixture bytes changed")

    def test_changed_robots_receipt_refused(self):
        self.row()["robots_fetch_receipt_id"] = "fetch:invented"
        self.refuses("missing exact robots receipt")

    def test_changed_robots_body_refused(self):
        self.row()["robots_body_sha256"] = "0" * 64
        self.refuses("robots receipt differs")

    def test_stale_robots_404_refused(self):
        row = self.row()
        stale = utc(self.state.now - timedelta(hours=25))
        row["robots_captured_at_utc"] = stale
        self.state.connection.execute(
            "UPDATE fetch_observations SET observation_json = json_set(" 
            "observation_json, '$.retrieved_at', ?) WHERE id = ?",
            (stale, row["robots_fetch_receipt_id"]),
        )
        self.state.connection.commit()
        self.refuses("stale")

    def test_blocked_source_cannot_be_reclassified(self):
        self.row(False)["release_cited"] = True
        self.refuses("packet release citations differ")

    def test_blocked_source_with_prior_review_refused(self):
        self.state.add_review(self.row(False), note="incorrect blocked review")
        self.refuses("blocked attachment has a source review")

    def test_expiry_later_than_thirty_days_refused(self):
        self.state.attestation["expires_at"] = utc(self.state.now + timedelta(days=31))
        self.refuses("within 30 days")

    def test_terms_or_404_scope_note_is_required(self):
        host = next(iter(batch.REVIEW_HOSTS))
        self.state.attestation["host_notes"][host] = "Approved automatically, no further review required."
        self.refuses("note must acknowledge")

    def test_existing_exact_review_is_idempotent_and_conflicts_refused(self):
        row = self.row()
        self.state.add_review(row)
        plan = self.state.preflight()
        self.assertEqual(sum(state == "already_recorded" for _, _, state in plan), 1)
        self.state.connection.execute(
            "UPDATE source_reviews SET review_json = json_set(review_json, '$.note', 'changed') "
            "WHERE source_id = ?", (row["archive_source_id"],),
        )
        self.state.connection.commit()
        self.refuses("conflicting existing review")

    def test_existing_review_with_broader_endpoint_scope_is_not_complete(self):
        row = self.row()
        self.state.add_review(row)
        self.state.connection.execute(
            "UPDATE source_reviews SET review_json = json_set("
            "review_json, '$.endpoint_patterns', json('[\"/\"]')) "
            "WHERE source_id = ?", (row["archive_source_id"],),
        )
        self.state.connection.commit()
        self.refuses("conflicting existing review")

    def test_bad_packet_makes_zero_writer_calls(self):
        self.row()["endpoint_url"] += "&other=1"
        self.state.save()
        marker = Path(self.temp.name) / "writer-called"
        fake_bin = Path(self.temp.name) / "pnull-fake"
        fake_bin.write_text(f"#!/bin/sh\ntouch '{marker}'\n", encoding="utf-8")
        fake_bin.chmod(0o700)
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "--data-dir", str(self.state.private),
             "--attestation", str(self.state.attestation_path), "--packet",
             str(self.state.packet_path), "--pnull-bin", str(fake_bin), "--record"],
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertFalse(marker.exists())
        self.assertEqual(self.state.connection.execute(
            "SELECT count(*) FROM source_reviews"
        ).fetchone()[0], 0)

    def test_attestation_swap_after_preflight_cannot_change_writer_identity_or_expiry(self):
        plan = self.state.preflight()
        original_reviewer = plan.reviewer
        original_expiry = plan.expires_at
        self.state.attestation["reviewer"] = "different_reviewer"
        self.state.attestation["expires_at"] = utc(self.state.now + timedelta(days=365))
        self.state.save()
        fake_bin = Path(self.temp.name) / "pnull-fake"
        fake_bin.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        fake_bin.chmod(0o700)
        with patch.object(batch.subprocess, "run") as writer:
            batch.record_plan(plan, self.state.private, fake_bin)
        self.assertEqual(writer.call_count, 12)
        for call in writer.call_args_list:
            command = call.args[0]
            self.assertEqual(command[command.index("--reviewer") + 1], original_reviewer)
            self.assertEqual(command[command.index("--expires") + 1], original_expiry)


if __name__ == "__main__":
    unittest.main()
