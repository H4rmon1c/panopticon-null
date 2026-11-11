#!/usr/bin/env python3
"""Check scheduled Colorado HTML calendars against the single geography map."""

import json
from pathlib import Path
import tomllib
from urllib.parse import urlsplit


ROOT = Path(__file__).resolve().parents[2]


def rows(path: Path):
    return [line.split("\t") for line in path.read_text().splitlines()
            if line and not line.startswith("#")]


geographies = {
    item["id"]: item for item in json.loads(
        (ROOT / "web/panopticon-terminal/geography/colorado-jurisdictions.json").read_text()
    )["jurisdictions"]
}
mapping = json.loads((ROOT / "configs/states/co-calendar-geography.v1.json").read_text())
assert mapping["schema_version"] == 1
bindings = mapping["calendar_geography"]
assert len(bindings) == 7
assert len(bindings) == len({item["source_id"] for item in bindings}), "duplicate calendar source"
schedule = {
    source: (config, adapter)
    for config, source, adapter in rows(ROOT / "deploy/selfhost/collection-sources.tsv")
    if adapter == "discover-calendar"
}
assert set(schedule) == {item["source_id"] for item in bindings}, "unmapped scheduled calendar"

for item in bindings:
    source_id = item["source_id"]
    geo = geographies[item["jurisdiction_ref"]]
    assert geo["kind"] in {"county", "municipality"}
    config_path, adapter = schedule[source_id]
    assert adapter == "discover-calendar"
    assert config_path.startswith("configs/states/co") and config_path.endswith(".toml")
    config = tomllib.loads((ROOT / config_path).read_text())
    assert config["jurisdiction"].split(",")[0] == geo["name"].removesuffix(" city").removesuffix(" town")
    sources = [source for source in config["sources"] if source["id"] == source_id]
    assert len(sources) == 1
    source = sources[0]
    url = urlsplit(source["url"])
    assert source["source_type"] == "html_page"
    assert url.scheme == "https" and url.hostname and not url.username and not url.password
    assert url.path == "/Calendar.aspx" and not url.query and not url.fragment
    assert 0 < source["maximum_bytes"] <= 4 * 1024 * 1024
    assert source["minimum_interval_seconds"] >= 3600
    assert source["documented_at"].startswith("https://")
    assert source["official_discovery_url"].startswith("https://")

print("7 official HTML calendars are mapped to Colorado geography")
