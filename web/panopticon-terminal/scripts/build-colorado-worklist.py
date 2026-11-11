#!/usr/bin/env python3
"""Build an offline Colorado geography worklist from configured sources and cases.

The output describes repository coverage, not current source-review state or
whether a jurisdiction uses surveillance technology.
"""

import argparse
import csv
import json
import importlib.util
import re
import tomllib
from collections import Counter, defaultdict
from pathlib import Path
from urllib.parse import urlsplit


ROOT = Path(__file__).resolve().parents[3]
GEOGRAPHY = ROOT / "web/panopticon-terminal/geography/colorado-jurisdictions.json"
MAPPING = ROOT / "configs/states/co-calendar-geography.v1.json"
INDEX_MAPPING = ROOT / "configs/states/co-document-index-geography.v1.json"
MUNICIPAL_DIRECTORY = ROOT / "configs/states/co-municipal-websites.v1.json"
MATRIX = ROOT / "deploy/selfhost/collection-sources.tsv"
DEFAULT_MATRIX = ROOT / "deploy/selfhost/collection-release-sources.tsv"
ATLAS = ROOT / "fixtures/atlas"
PUBLIC = ROOT / "web/panopticon-terminal/public/public-snapshot.json"


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8"))


def fixed_official_path(path):
    return (path.startswith("/") and path != "/" and len(path) <= 160
            and "%" not in path and "\\" not in path and "//" not in path
            and all(part not in (".", "..") for part in path.split("/")))


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


def document_index_bindings(valid_refs):
    mapping = read_json(INDEX_MAPPING)
    if mapping.get("schema_version") != 1:
        raise ValueError("unsupported document-index mapping")
    indexes = defaultdict(list)
    seen = set()
    seen_pdf = set()
    for item in mapping["document_index_geography"]:
        source_id, ref, config_path = item["source_id"], item["jurisdiction_ref"], item["config_path"]
        if (source_id in seen or ref not in valid_refs
                or valid_refs[ref]["kind"] != "municipality"
                or not re.fullmatch(r"configs/states/co-[a-z0-9-]+\.toml", config_path)):
            raise ValueError(f"invalid town index binding: {item}")
        seen.add(source_id)
        config_file = ROOT / config_path
        if config_file.is_symlink() or not config_file.is_file():
            raise ValueError(f"missing or unsafe document index config: {config_path}")
        config = tomllib.loads(config_file.read_text(encoding="utf-8"))
        sources = [source for source in config["sources"] if source["id"] == source_id]
        if len(sources) != 1 or sources[0]["source_type"] != "html_page":
            raise ValueError(f"document index source is missing: {source_id}")
        url = urlsplit(sources[0]["url"])
        if (url.scheme != "https" or not url.hostname or url.port not in (None, 443)
                or url.username or url.password or not fixed_official_path(url.path)
                or url.path.lower() == "/calendar.aspx"
                or url.query or url.fragment
                or sources[0]["minimum_interval_seconds"] < 3600
                or not 0 < sources[0]["maximum_bytes"] <= 2_097_152):
            raise ValueError(f"document index policy is outside its reviewed shape: {source_id}")
        pdf_policies = [source for source in config["sources"]
                        if source["source_type"] == "candidate_index_pdf"
                        and source["official_discovery_url"] == sources[0]["url"]
                        and source["documented_at"] == sources[0]["url"]]
        if len(pdf_policies) != 1:
            raise ValueError(f"document index needs exactly one PDF path policy: {source_id}")
        pdf = pdf_policies[0]
        pdf_url = urlsplit(pdf["url"])
        if (pdf["id"] in seen_pdf or pdf_url.scheme != "https"
                or pdf_url.netloc != url.netloc or not fixed_official_path(pdf_url.path)
                or not pdf_url.path.endswith("/")
                or pdf_url.query or pdf_url.fragment or pdf["minimum_interval_seconds"] < 120
                or not 0 < pdf["maximum_bytes"] <= 20_971_520):
            raise ValueError(f"document PDF policy is outside its reviewed shape: {source_id}")
        seen_pdf.add(pdf["id"])
        indexes[ref].append(source_id)
    return indexes


def municipal_website_leads(valid_refs):
    directory = read_json(MUNICIPAL_DIRECTORY)
    source = directory.get("source", {})
    if (directory.get("schema_version") != 1
            or source.get("url") != "https://www.cml.org/home/networking-events/membership/membership"
            or not re.fullmatch(r"[0-9a-f]{64}", source.get("sha256", ""))):
        raise ValueError("invalid municipal directory provenance")
    entries = directory.get("municipalities", [])
    leads = {}
    for item in entries:
        ref = item["jurisdiction_ref"]
        url = item["website_url"]
        name = item["listing_name"]
        if (ref in leads or ref not in valid_refs or valid_refs[ref]["kind"] != "municipality"
                or not name or item["source_locator"] != f"Links to Colorado cities and towns > {name}"):
            raise ValueError(f"invalid municipal directory binding: {item}")
        if url is not None:
            parts = urlsplit(url)
            if (parts.scheme not in ("http", "https") or not parts.hostname
                    or parts.username or parts.password or len(url) > 2_048):
                raise ValueError(f"invalid municipal website lead: {item}")
        leads[ref] = item
    counts = directory["counts"]
    municipal_count = sum(row["kind"] == "municipality" for row in valid_refs.values())
    linked = sum(item["website_url"] is not None for item in entries)
    http = sum(item["website_url"] is not None and item["website_url"].startswith("http:")
               for item in entries)
    expected = {"census_municipalities": municipal_count, "listed": len(entries),
                "with_website": linked, "without_website": len(entries) - linked,
                "http_leads": http, "https_leads": linked - http,
                "not_listed": municipal_count - len(entries)}
    if counts != expected:
        raise ValueError("municipal directory counts do not match its bindings")
    return leads


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


def selected_discovery(matrix):
    matrix = Path(matrix)
    if not matrix.is_absolute():
        matrix = ROOT / matrix
    relative = matrix.relative_to(ROOT).as_posix()
    spec = importlib.util.spec_from_file_location("collection_matrix", ROOT / "deploy/selfhost/validate-collection-matrix.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    entries = module.validate(ROOT, relative, "deploy/selfhost/collection-release-fallbacks.tsv")
    return relative, {identity: adapter for _, identity, adapter in entries
                      if adapter in ("discover-calendar", "discover-index")}


def build_rows(matrix=DEFAULT_MATRIX):
    selected_matrix, selected = selected_discovery(matrix)
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
        raise ValueError("calendar mapping does not match configured discovery sources")
    indexes = document_index_bindings(by_ref)
    known_calendar = {identity for values in calendar_by_ref.values() for identity in values}
    known_indexes = {identity for values in indexes.values() for identity in values}
    for identity, adapter in selected.items():
        if identity not in (known_calendar if adapter == "discover-calendar" else known_indexes):
            raise ValueError(f"scheduled discovery source lacks geography mapping: {identity}")
    website_leads = municipal_website_leads(by_ref)
    cases = case_inventory(by_ref)
    published = published_inventory(records)
    rows = []
    for record in records:
        ref = record["id"]
        source_verified = [case_id for case_id, state in cases[ref] if state == "source_verified"]
        pending = [case_id for case_id, state in cases[ref] if state == "pending"]
        public = published[ref]
        directory = website_leads.get(ref)
        if record["kind"] != "municipality":
            directory_status = "not_applicable"
        elif directory is None:
            directory_status = "not_listed"
        elif directory["website_url"]:
            directory_status = "listed_with_url"
        else:
            directory_status = "listed_without_url"
        stage = ("published_case" if public else "source_verified_manifest" if source_verified
                 else "pending_manifest" if pending else "calendar_configured" if calendar_by_ref[ref]
                 else "document_index_configured" if indexes[ref] else "not_started")
        rows.append({
            "geography_id": ref,
            "kind": record["kind"],
            "name": record["name"],
            "stage": stage,
            "municipal_directory_status": directory_status,
            "municipal_website_lead": directory["website_url"] if directory else "",
            "municipal_directory_locator": directory["source_locator"] if directory else "",
            "configured_calendar_ids": ";".join(sorted(calendar_by_ref[ref])),
            "selected_collection_matrix": selected_matrix,
            "scheduled_calendar_ids": ";".join(sorted(identity for identity in calendar_by_ref[ref] if identity in selected)),
            "scheduled_document_index_ids": ";".join(sorted(identity for identity in indexes[ref] if identity in selected)),
            "discovery_schedule_state": "selected_in_matrix" if any(identity in selected for identity in calendar_by_ref[ref] + indexes[ref]) else "not_selected",
            "source_review_state": "not_checked",
            "search_execution_state": "not_measured",
            "document_index_ids": ";".join(sorted(indexes[ref])),
            "source_verified_case_ids": ";".join(sorted(source_verified)),
            "pending_case_ids": ";".join(sorted(pending)),
            "published_case_ids": ";".join(sorted(public)),
        })
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--collection-matrix", type=Path, default=DEFAULT_MATRIX, help="selected schedule to report; does not enable collection")
    args = parser.parse_args()
    rows = build_rows(args.collection_matrix)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", newline="", encoding="utf-8") as out:
        writer = csv.DictWriter(out, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)
    print(f"Wrote {len(rows)} geography worklist rows: {dict(sorted(Counter(row['stage'] for row in rows).items()))}")


if __name__ == "__main__":
    main()
