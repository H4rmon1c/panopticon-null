#!/usr/bin/env python3
"""Build an offline Colorado geography worklist from configured sources and cases.

The output describes repository coverage, not current source-review state or
whether a jurisdiction uses surveillance technology.
"""

import argparse
import csv
import json
from collections import Counter, defaultdict
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
GEOGRAPHY = ROOT / "web/panopticon-terminal/geography/colorado-jurisdictions.json"
MAPPING = ROOT / "configs/states/co-calendar-geography.v1.json"
MATRIX = ROOT / "deploy/selfhost/collection-sources.tsv"
ATLAS = ROOT / "fixtures/atlas"
PUBLIC = ROOT / "web/panopticon-terminal/public/public-snapshot.json"


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8"))


def calendar_source_ids():
    ids = set()
    for line in MATRIX.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        parts = line.split("\t")
        if len(parts) != 3:
            raise ValueError(f"invalid collection matrix row: {line!r}")
        if parts[2] == "discover-calendar":
            if parts[1] in ids:
                raise ValueError(f"duplicate calendar source: {parts[1]}")
            ids.add(parts[1])
    return ids


def case_inventory(valid_refs):
    cases = defaultdict(list)
    seen = set()
    for directory in (ATLAS / "cases", ATLAS / "preview-cases"):
        for path in sorted(directory.glob("*.v2.json")):
            case = read_json(path)
            case_id = case["case_id"]
            ref = case["jurisdiction"]["jurisdiction_ref"]
            state = case["review_state"]
            if case_id in seen or ref not in valid_refs or state not in ("source_verified", "pending"):
                raise ValueError(f"invalid case identity or state: {path}")
            if directory.name == "preview-cases" and state != "pending":
                raise ValueError(f"preview case has release-ready state: {path}")
            if directory.name == "cases" and state != "source_verified":
                raise ValueError(f"source-verified case is pending: {path}")
            seen.add(case_id)
            cases[ref].append((case_id, state))
    return cases


def published_inventory(records):
    snapshot = read_json(PUBLIC)
    if snapshot.get("demo") or snapshot.get("preview") or snapshot["status"]["state"] != "released":
        raise ValueError("public snapshot is not a release")
    if snapshot["release_review"]["state"] not in ("APPROVED", "SOURCE_VERIFIED"):
        raise ValueError("public snapshot lacks release state")
    by_entity = {row.get("published_entity_id"): row["id"] for row in records if row.get("published_entity_id")}
    refs = {row["id"] for row in records}
    jurisdiction_to_ref = {}
    for entity in snapshot["entities"]:
        if entity["type"] != "JURISDICTION":
            continue
        ref = entity.get("jurisdiction_ref")
        if ref in refs:
            jurisdiction_to_ref[entity["id"]] = ref
        elif entity["id"] in by_entity:
            jurisdiction_to_ref[entity["id"]] = by_entity[entity["id"]]
    published = defaultdict(list)
    release_by_slug = {item["case_id"]: item["release"] for item in snapshot.get("case_releases", [])}
    decision_slugs = set()
    for entity in snapshot["entities"]:
        if entity["type"] != "DECISION":
            continue
        decision_slugs.add(entity["slug"])
        if release_by_slug and release_by_slug.get(entity["slug"], {}).get("state") not in ("APPROVED", "SOURCE_VERIFIED"):
            raise ValueError(f"published decision lacks a case release: {entity['slug']}")
        for ref in {jurisdiction_to_ref.get(jid) for jid in entity.get("jurisdiction_ids", [])} - {None}:
            published[ref].append(entity["slug"])
    if release_by_slug and set(release_by_slug) != decision_slugs:
        raise ValueError("public case releases do not match decisions")
    return published


def build_rows():
    geography = read_json(GEOGRAPHY)
    records = geography["jurisdictions"]
    if len(records) != 4111 or geography["counts"]["county"] != 64:
        raise ValueError("geography roster changed; inspect the source archives first")
    by_ref = {row["id"]: row for row in records}
    if len(by_ref) != len(records):
        raise ValueError("duplicate geography ID")
    mapping = read_json(MAPPING)
    if mapping.get("schema_version") != 1:
        raise ValueError("unsupported calendar mapping")
    calendar_by_ref = defaultdict(list)
    mapped_ids = set()
    for item in mapping["calendar_geography"]:
        source_id, ref = item["source_id"], item["jurisdiction_ref"]
        if source_id in mapped_ids or ref not in by_ref or by_ref[ref]["kind"] not in ("municipality", "county"):
            raise ValueError(f"invalid calendar geography binding: {item}")
        mapped_ids.add(source_id)
        calendar_by_ref[ref].append(source_id)
    if mapped_ids != calendar_source_ids():
        raise ValueError("calendar mapping does not match scheduled discovery sources")
    cases = case_inventory(by_ref)
    published = published_inventory(records)
    rows = []
    for record in records:
        ref = record["id"]
        source_verified = [case_id for case_id, state in cases[ref] if state == "source_verified"]
        pending = [case_id for case_id, state in cases[ref] if state == "pending"]
        public = published[ref]
        stage = ("published_case" if public else "source_verified_manifest" if source_verified
                 else "pending_manifest" if pending else "calendar_configured" if calendar_by_ref[ref]
                 else "not_started")
        rows.append({
            "geography_id": ref,
            "kind": record["kind"],
            "name": record["name"],
            "stage": stage,
            "scheduled_calendar_ids": ";".join(sorted(calendar_by_ref[ref])),
            "source_verified_case_ids": ";".join(sorted(source_verified)),
            "pending_case_ids": ";".join(sorted(pending)),
            "published_case_ids": ";".join(sorted(public)),
        })
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    rows = build_rows()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", newline="", encoding="utf-8") as out:
        writer = csv.DictWriter(out, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)
    print(f"Wrote {len(rows)} geography worklist rows: {dict(sorted(Counter(row['stage'] for row in rows).items()))}")


if __name__ == "__main__":
    main()
