#!/usr/bin/env python3
"""Preflight one human attestation, then record 12 exact source reviews.

This never fetches case endpoints. The existing pnull CLI remains the only
writer of source reviews and independently enforces robots and review scope.
"""

import argparse
import hashlib
import ipaddress
import json
import re
import sqlite3
import stat
import subprocess
import sys
import tomllib
from datetime import datetime, timedelta, timezone
from pathlib import Path, PurePosixPath
from urllib.parse import urlsplit

from archive_profile_digest import config_digest


ROOT = Path(__file__).resolve().parents[2]
PACKET = ROOT / "docs/colorado-source-review-packet.json"
REGISTRY = ROOT / "fixtures/atlas/sources.co.v1.json"
CASE_FILES = (
    "commerce-city-ord-2755-2026-04-06.v2.json",
    "commerce-city-ord-2756-2026-04-06.v2.json",
    "castle-rock-res-2025-071.v2.json",
    "denver-25-0413-2025-05-05.v2.json",
)
CONFIGS = frozenset({
    "configs/states/co-commerce-archive.toml",
    "configs/states/co-castle-rock-archive.toml",
    "configs/states/co-denver-archive.toml",
})
REVIEW_HOSTS = frozenset({
    "webapi.legistar.com", "commerce.legistar.com",
    "castlerock-co.legistar.com", "denver.legistar.com",
})
BLOCKED_HOSTS = frozenset({
    "legistar.granicus.com", "legistar2.granicus.com", "denver.legistar1.com",
})
BLOCKED_IDS = frozenset({
    "co-commerce-2026-ordinance", "co-commerce-2026-unsigned-agreement",
    "co-castle-rock-2025-sole-source", "co-castle-rock-2025-vendor-signed-agreement",
    "co-denver-2025-flock-proposed-amendment",
})


class ReviewBatchError(ValueError):
    pass


class ReviewPlan(list):
    """Validated attestation fields retained with the checked source rows."""

    def __init__(self, rows, reviewer, expires_at):
        super().__init__(rows)
        self.reviewer = reviewer
        self.expires_at = expires_at


def check(condition, message):
    if not condition:
        raise ReviewBatchError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def timestamp(value):
    check(isinstance(value, str) and value.endswith("Z"), "timestamp must be UTC RFC 3339")
    try:
        result = datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError as error:
        raise ReviewBatchError(f"invalid timestamp: {value}") from error
    check(result.tzinfo is not None, "timestamp is missing timezone")
    return result


def repository_file(relative):
    check(isinstance(relative, str), "repository path must be text")
    posix = PurePosixPath(relative)
    check(not posix.is_absolute() and posix.parts and
          all(part not in ("", ".", "..") for part in posix.parts),
          f"unsafe repository path: {relative}")
    path = ROOT.joinpath(*posix.parts)
    check(path.is_file() and not path.is_symlink() and
          path.resolve() == path.absolute() and path.resolve().is_relative_to(ROOT.resolve()),
          f"repository file missing or redirected: {relative}")
    return path


def json_file(path):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ReviewBatchError(f"cannot read JSON: {path}") from error


def case_source_ids(registry_ids):
    found = set()

    def walk(value):
        if isinstance(value, dict):
            for child in value.values():
                walk(child)
        elif isinstance(value, list):
            for child in value:
                walk(child)
        elif isinstance(value, str) and value in registry_ids:
            found.add(value)

    for name in CASE_FILES:
        path = repository_file(f"fixtures/atlas/cases/{name}")
        walk(json_file(path))
    return found


def read_private_store(data_dir):
    check(data_dir.is_absolute() and data_dir.is_dir() and not data_dir.is_symlink(),
          "private data directory must be an existing absolute directory")
    check(stat.S_IMODE(data_dir.stat().st_mode) & 0o077 == 0,
          "private data directory must not grant group or other access")
    db = data_dir / "pnull.db"
    check(db.is_file() and not db.is_symlink(), "private pnull.db is missing or symlinked")
    connection = sqlite3.connect(db.as_uri() + "?mode=ro", uri=True)
    connection.execute("PRAGMA query_only=ON")
    return connection


def private_blob(data_dir, sha256):
    check(isinstance(sha256, str) and re.fullmatch(r"[a-f0-9]{64}", sha256),
          "invalid robots body digest")
    path = data_dir / "evidence" / "sha256" / sha256[:2] / sha256
    check(path.is_file() and not path.is_symlink() and
          path.resolve().is_relative_to(data_dir.resolve()),
          f"preserved robots body missing: {sha256}")
    data = path.read_bytes()
    check(digest(data) == sha256, f"preserved robots body changed: {sha256}")
    return data


def receipt_for(connection, source_id, receipt_id):
    rows = connection.execute(
        "SELECT observation_json FROM fetch_observations WHERE id = ? AND source_id = ?",
        (receipt_id, source_id),
    ).fetchall()
    check(len(rows) == 1, f"missing exact robots receipt: {source_id}")
    return json.loads(rows[0][0])


def existing_review(connection, source_id):
    rows = connection.execute(
        "SELECT review_json FROM source_reviews WHERE source_id = ? "
        "ORDER BY json_extract(review_json, '$.reviewed_at') DESC LIMIT 1",
        (source_id,),
    ).fetchall()
    return json.loads(rows[0][0]) if rows else None


def note_for(row, host_note, packet_sha256):
    return (f"One-batch source-policy attestation {packet_sha256}; "
            f"exact endpoint {row['archive_source_id']}; {host_note}")


def preflight(packet_path, attestation_path, data_dir, now=None):
    now = now or datetime.now(timezone.utc)
    check(now.tzinfo is not None and now.utcoffset() == timedelta(0),
          "preflight clock must be UTC")
    try:
        packet_bytes = packet_path.read_bytes()
        packet = json.loads(packet_bytes)
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ReviewBatchError(f"cannot read packet: {packet_path}") from error
    packet_sha256 = digest(packet_bytes)
    attestation = json_file(attestation_path)
    check(packet.get("schema_version") == 1 and attestation.get("schema_version") == 1,
          "unsupported packet or attestation schema")
    check(attestation.get("packet_sha256") == packet_sha256,
          "attestation does not bind the exact review packet")
    check(digest(REGISTRY.read_bytes()) == packet.get("source_registry_sha256"),
          "source registry changed since packet creation")
    registry_rows = json_file(REGISTRY).get("sources")
    check(isinstance(registry_rows, list), "invalid source registry")
    registry = {row["id"]: row for row in registry_rows}
    check(len(registry) == len(registry_rows), "duplicate registry source ID")
    expected_release = case_source_ids(registry)
    check(len(expected_release) == 12, "release case source set changed")
    rows = packet.get("sources")
    check(isinstance(rows, list) and len(rows) == 17, "packet must enumerate 17 exact sources")
    check(len({row.get("registry_source_id") for row in rows}) == 17 and
          len({row.get("archive_source_id") for row in rows}) == 17,
          "duplicate source in review packet")
    release_rows = [row for row in rows if row.get("release_cited") is True]
    blocked_rows = [row for row in rows if row.get("release_cited") is False]
    check({row["registry_source_id"] for row in release_rows} == expected_release,
          "packet release citations differ from current case manifests")
    check({row["registry_source_id"] for row in blocked_rows} == BLOCKED_IDS,
          "packet omitted or reclassified a blocked attachment")
    check(packet.get("counts") == {
        "fixture_digest_matches_registry": 17,
        "robots_responses_preserved": 17,
        "release_cited_pending_policy_review": 12,
        "blocked_by_robots_excluded_from_release": 5,
        "source_policy_reviews_recorded": 0,
        "case_endpoint_fetches_by_this_packet": 0,
    }, "packet count claims changed")
    reviewer = attestation.get("reviewer")
    check(isinstance(reviewer, str) and re.fullmatch(r"[A-Za-z0-9._-]{2,64}", reviewer) and
          reviewer not in ("REPLACE", "example"), "explicit reviewer name is required")
    expiry_text = attestation.get("expires_at")
    expiry = timestamp(expiry_text)
    check(now < expiry <= now + timedelta(days=30), "review expiry must be within 30 days")
    host_notes = attestation.get("host_notes")
    check(isinstance(host_notes, dict) and set(host_notes) == REVIEW_HOSTS,
          "attestation must have one note for each release host")
    for host, note in host_notes.items():
        check(isinstance(note, str) and len(note.strip()) >= 40 and
              "404" in note and "terms" in note.lower() and
              all(ord(char) >= 32 for char in note),
              f"{host}: note must acknowledge HTTP 404 and the reviewed terms scope")
    check(attestation.get("approved_source_ids") == sorted(expected_release),
          "attestation does not name the exact 12 release sources")
    configs = {}
    for config in CONFIGS:
        profile = tomllib.loads(repository_file(config).read_text(encoding="utf-8"))
        sources = profile.get("sources")
        check(isinstance(sources, list), f"invalid config: {config}")
        configs[config] = {source["id"]: source for source in sources}
        check(len(configs[config]) == len(sources), f"duplicate config source: {config}")
    connection = read_private_store(data_dir)
    plan = []
    try:
        for row in rows:
            source_id = row["registry_source_id"]
            archive_id = row["archive_source_id"]
            config_path = row["archive_config"]
            check(config_path in CONFIGS and archive_id in configs[config_path],
                  f"unknown archive config/source: {source_id}")
            source = configs[config_path][archive_id]
            registered = registry.get(source_id)
            check(registered is not None and registered.get("acquisition_source_id") == archive_id,
                  f"registry acquisition mismatch: {source_id}")
            url = urlsplit(source["url"])
            check(url.scheme == "https" and url.hostname and url.port in (None, 443) and
                  not url.username and not url.password and not url.fragment,
                  f"unsafe archive URL: {source_id}")
            check(row["endpoint_url"] == source["url"] == registered["url"] and
                  row["source_config_sha256"] == config_digest(source) ==
                  registered["acquisition_config_digest"],
                  f"URL or source policy changed: {source_id}")
            fixture = repository_file(row["preserved_fixture"])
            fixture_bytes = fixture.read_bytes()
            check(row["preserved_fixture"] == registered["fixture"] and
                  row["preserved_fixture_sha256"] == registered["sha256"] == digest(fixture_bytes) and
                  row["preserved_fixture_bytes"] == len(fixture_bytes) and
                  row["fixture_matches_registry"] is True,
                  f"fixture bytes changed: {source_id}")
            check(row["minimum_interval_seconds"] == source["minimum_interval_seconds"] and
                  row["maximum_bytes"] == source["maximum_bytes"] and
                  0 < len(fixture_bytes) <= source["maximum_bytes"],
                  f"archive bounds changed: {source_id}")
            robots_url = f"https://{url.hostname}/robots.txt"
            check(row["robots_url"] == robots_url,
                  f"robots host does not match endpoint: {source_id}")
            receipt = receipt_for(connection, archive_id, row["robots_fetch_receipt_id"])
            captured_at = timestamp(row["robots_captured_at_utc"])
            check(receipt.get("id") == row["robots_fetch_receipt_id"] and
                  receipt.get("source_id") == archive_id and
                  receipt.get("requested_url") == robots_url and
                  receipt.get("final_url") == robots_url and
                  receipt.get("method") == "GET" and receipt.get("error") is None and
                  receipt.get("status_code") == row["robots_http_status"] and
                  receipt.get("body_digest") == row["robots_body_sha256"] and
                  receipt.get("retrieved_at") == row["robots_captured_at_utc"],
                  f"robots receipt differs from packet: {source_id}")
            ips = receipt.get("resolved_ips")
            check(isinstance(ips, list) and ips and
                  all(isinstance(ip, str) and ipaddress.ip_address(ip).is_global for ip in ips),
                  f"robots receipt lacks public IP: {source_id}")
            body = private_blob(data_dir, row["robots_body_sha256"])
            check(len(body) == row["robots_body_bytes"],
                  f"robots body length changed: {source_id}")
            check(row["source_policy_review"] == "not_recorded", 
                  f"packet review claim changed: {source_id}")
            if row["release_cited"]:
                check(url.hostname in REVIEW_HOSTS and row["robots_http_status"] == 404 and
                      row["live_endpoint_fetch"] == "pending_human_source_policy_review" and
                      row["robots_observation"] == "HTTP 404; absence is not permission" and
                      timedelta(0) <= now - captured_at < timedelta(hours=24),
                      f"release source robots review is missing or stale: {source_id}")
                note = note_for(row, host_notes[url.hostname], packet_sha256)
                previous = existing_review(connection, archive_id)
                if previous is None:
                    state = "ready"
                else:
                    matching = (previous.get("source_config_digest") == row["source_config_sha256"] and
                                previous.get("source_id") == archive_id and
                                previous.get("reviewed_hosts") == [url.hostname] and
                                previous.get("endpoint_patterns") == [url.path] and
                                previous.get("robots_url") == robots_url and
                                previous.get("robots_snapshot_digest") == row["robots_body_sha256"] and
                                previous.get("robots_provenance") == row["robots_fetch_receipt_id"] and
                                previous.get("reviewer") == reviewer and
                                previous.get("note") == note and
                                previous.get("expires_at") == expiry_text and
                                previous.get("minimum_interval_seconds") ==
                                source["minimum_interval_seconds"] and
                                previous.get("restrictions") == [] and
                                previous.get("terms_urls") == [] and
                                previous.get("terms_snapshot_digests") == [] and
                                captured_at <= timestamp(previous.get("reviewed_at")) < expiry)
                    check(matching, f"conflicting existing review: {source_id}")
                    state = "already_recorded"
                plan.append((row, note, state))
            else:
                check(url.hostname in BLOCKED_HOSTS and row["robots_http_status"] == 200 and
                      row["live_endpoint_fetch"] == "blocked_by_robots" and
                      row["robots_observation"] == "Disallow: / for User-agent: *" and
                      b"user-agent: *" in body.lower() and b"disallow: /" in body.lower(),
                      f"blocked attachment policy changed: {source_id}")
                check(existing_review(connection, archive_id) is None,
                      f"blocked attachment has a source review: {source_id}")
    finally:
        connection.close()
    return ReviewPlan(
        sorted(plan, key=lambda item: item[0]["archive_source_id"]),
        reviewer,
        expiry_text,
    )


def record_plan(plan, data_dir, pnull_bin):
    check(pnull_bin.is_file() and not pnull_bin.is_symlink(),
          "pnull binary is missing or symlinked")
    for row, note, state in plan:
        if state == "already_recorded":
            continue
        command = [str(pnull_bin), "--data-dir", str(data_dir),
                   "--config", row["archive_config"], "source", "review", "record",
                   row["archive_source_id"], "--reviewer", plan.reviewer,
                   "--note", note, "--expires", plan.expires_at]
        subprocess.run(command, cwd=ROOT, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data-dir", type=Path, required=True)
    parser.add_argument("--attestation", type=Path, required=True)
    parser.add_argument("--packet", type=Path, default=PACKET)
    parser.add_argument("--pnull-bin", type=Path, default=ROOT / "target/debug/pnull")
    parser.add_argument("--record", action="store_true", help="write reviews after full preflight")
    args = parser.parse_args()
    try:
        plan = preflight(args.packet, args.attestation, args.data_dir)
        if args.record:
            record_plan(plan, args.data_dir, args.pnull_bin)
        print(json.dumps({"release_sources": len(plan),
                          "ready": sum(state == "ready" for _, _, state in plan),
                          "already_recorded": sum(state == "already_recorded" for _, _, state in plan),
                          "record_requested": args.record}, sort_keys=True))
    except (ReviewBatchError, OSError, sqlite3.Error, ValueError, TypeError,
            KeyError, subprocess.CalledProcessError) as error:
        print(f"source review batch refused: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
