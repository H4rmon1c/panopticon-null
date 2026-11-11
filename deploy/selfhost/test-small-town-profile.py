#!/usr/bin/env python3
"""Keep small-town document indexes narrow, private, and tied to geography."""

import hashlib
import json
from pathlib import Path
import tomllib
from urllib.parse import urlsplit


ROOT = Path(__file__).resolve().parents[2]
PROFILES = (
    ("Silver Plume town", "us-co-place-0870360", "silver-plume", "https://silverplumetown.com/agendas-and-minutes/", "/wp-content/uploads/", True),
    ("Moffat town", "us-co-place-0851250", "moffat", "https://www.townofmoffatco.gov/documents", "/uploads/", True),
    ("Simla town", "us-co-place-0870635", "simla", "https://townofsimla.com/documents", "/uploads/", True),
    ("Del Norte town", "us-co-place-0819795", "del-norte", "https://www.delnortecolorado.com/minutes-agendas/", "/wp-content/uploads/", False),
    ("Saguache town", "us-co-place-0867005", "saguache", "https://www.townofsaguache.org/documents", "/uploads/", False),
)


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8"))


geography = read_json(ROOT / "web/panopticon-terminal/geography/colorado-jurisdictions.json")
by_ref = {row["id"]: row for row in geography["jurisdictions"]}
mapping = read_json(ROOT / "configs/states/co-document-index-geography.v1.json")
assert mapping["schema_version"] == 1
assert len(mapping["document_index_geography"]) == len(PROFILES)
mapped = {row["source_id"]: row for row in mapping["document_index_geography"]}

scheduled = {
    line.split("\t")[1] for line in
    (ROOT / "deploy/selfhost/collection-sources.tsv").read_text().splitlines()
    if line and not line.startswith("#")
}
snapshot = read_json(ROOT / "web/panopticon-terminal/public/public-snapshot.json")

for name, ref, slug, index_url, pdf_prefix, has_excerpt in PROFILES:
    assert by_ref[ref]["name"] == name and by_ref[ref]["kind"] == "municipality"
    index_id, pdf_id = f"{slug}-document-index", f"{slug}-pdf-documents"
    assert mapped[index_id] == {
        "source_id": index_id,
        "config_path": f"configs/states/co-{slug}.toml",
        "jurisdiction_ref": ref,
    }
    config = tomllib.loads((ROOT / f"configs/states/co-{slug}.toml").read_text())
    assert config["default_dry_run"] is True
    assert config["jurisdiction"] == f"{name.removesuffix(' town')}, Colorado"
    sources = {source["id"]: source for source in config["sources"]}
    assert set(sources) == {index_id, pdf_id}
    index, pdf = sources[index_id], sources[pdf_id]
    assert index["source_type"] == "html_page" and index["url"] == index_url
    assert index["minimum_interval_seconds"] >= 86_400
    assert 0 < index["maximum_bytes"] <= 2_097_152
    assert pdf["source_type"] == "candidate_index_pdf"
    assert pdf["url"] == f"{urlsplit(index_url).scheme}://{urlsplit(index_url).netloc}{pdf_prefix}"
    assert pdf["minimum_interval_seconds"] >= 120
    assert 0 < pdf["maximum_bytes"] <= 20_971_520
    assert pdf["official_discovery_url"] == index_url and pdf["documented_at"] == index_url
    assert index_id not in scheduled and pdf_id not in scheduled

    excerpt = ROOT / f"fixtures/co-{slug}/index-excerpt.html"
    manifest_path = ROOT / f"fixtures/co-{slug}/index-excerpt.json"
    assert excerpt.exists() == has_excerpt and manifest_path.exists() == has_excerpt
    if has_excerpt:
        manifest = read_json(manifest_path)
        assert manifest["source_url"] == index_url
        assert hashlib.sha256(excerpt.read_bytes()).hexdigest() == manifest["excerpt_sha256"]
        assert len(manifest["full_page_sha256"]) == 64
    assert ref not in json.dumps(snapshot)
    for directory in (ROOT / "fixtures/atlas/cases", ROOT / "fixtures/atlas/preview-cases"):
        for path in directory.glob("*.v2.json"):
            assert read_json(path)["jurisdiction"]["jurisdiction_ref"] != ref

print(f"{len(PROFILES)} towns map to unscheduled, bounded PDF document indexes and no public case")
