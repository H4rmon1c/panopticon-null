#!/usr/bin/env python3
"""Check that API backups are narrow, separately reviewed source profiles."""

from pathlib import Path
import tomllib
from urllib.parse import parse_qs, urlsplit


ROOT = Path(__file__).resolve().parents[2]
EXPECTED_CLIENTS = {
    "colorado-springs-calendar": "coloradosprings",
    "denver-calendar": "denver",
    "commerce-city-calendar": "commerce",
    "castle-rock-calendar": "castlerock-co",
    "arapahoe-county-calendar": "arapahoe",
}
EXPECTED_START = {
    "colorado-springs-calendar": "2025-01-01",
    "denver-calendar": "2025-01-01",
    "commerce-city-calendar": "2025-01-01",
    "castle-rock-calendar": "2025-01-01",
    "arapahoe-county-calendar": "2024-01-01",
}


def rows(path):
    result = []
    for line in path.read_text().splitlines():
        if not line or line.startswith("#"):
            continue
        parts = line.split("\t")
        assert len(parts) == 3, path
        result.append(parts)
    return result


scheduled = {
    (config, source): adapter
    for config, source, adapter in rows(ROOT / "deploy/selfhost/collection-sources.tsv")
}
fallbacks = rows(ROOT / "deploy/selfhost/collection-fallbacks.tsv")
assert len(fallbacks) == 5
assert {primary for _, primary, _ in fallbacks} == set(EXPECTED_CLIENTS)
assert len({(config, primary) for config, primary, _ in fallbacks}) == len(fallbacks)
assert len({fallback for _, _, fallback in fallbacks}) == len(fallbacks)

for config_path, primary_id, fallback_id in fallbacks:
    assert scheduled[(config_path, primary_id)] == "discover-calendar"
    assert (config_path, fallback_id) not in scheduled
    profile = tomllib.loads((ROOT / config_path).read_text())
    sources = {source["id"]: source for source in profile["sources"]}
    assert len(sources) == len(profile["sources"])
    calendar = sources[primary_id]
    api = sources[fallback_id]
    assert calendar["source_type"] == "html_page"
    assert api["source_type"] == "official_api"
    url = urlsplit(api["url"])
    assert url.scheme == "https" and url.hostname == "webapi.legistar.com"
    assert url.port is None and not url.username and not url.password and not url.fragment
    assert url.path == f"/v1/{EXPECTED_CLIENTS[primary_id]}/events"
    query = parse_qs(url.query, keep_blank_values=True)
    assert query == {
        "$filter": [f"EventDate ge datetime'{EXPECTED_START[primary_id]}'"],
        "$orderby": ["EventDate desc"],
        "$top": ["100"],
        "EventItems": ["1"],
        "AgendaNote": ["1"],
        "MinutesNote": ["1"],
        "EventItemAttachments": ["1"],
    }
    assert api["minimum_interval_seconds"] >= 86_400
    assert 0 < api["maximum_bytes"] <= 20 * 1024 * 1024
    assert api["documented_at"].startswith("https://webapi.legistar.com/")
    assert api["official_discovery_url"].startswith("https://")

print("5 official API event feeds are gated fallbacks for HTML calendars")
