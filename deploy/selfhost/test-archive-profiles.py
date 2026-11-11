#!/usr/bin/env python3
"""Check fixed direct-document profiles against preserved official-source bytes."""

import hashlib
import json
from pathlib import Path
import tomllib
from urllib.parse import urlparse

from archive_profile_digest import config_digest


ROOT = Path(__file__).resolve().parents[2]
REGISTRY = {
    source["fixture"]: source
    for source in json.loads((ROOT / "fixtures/atlas/sources.co.v1.json").read_text())["sources"]
}
PROFILE_FIXTURES = {
    "co-commerce-archive.toml": {
        "commerce-ord2755-detail": "fixtures/co-commerce/ord-2755-detail.html",
        "commerce-ord2755-minutes": "fixtures/co-commerce/minutes-2026-04-06.pdf",
        "commerce-ord2755-event-json": "fixtures/co-commerce/event-4626-2026-04-06.json",
        "commerce-ord2755-votes-json": "fixtures/co-commerce/votes-70384-ord-2755.json",
        "commerce-ord2756-votes-json": "fixtures/co-commerce/votes-70385-ord-2756.json",
        "commerce-ord2755-ordinance": "fixtures/co-commerce/ord-2755-ordinance.pdf",
        "commerce-ord2755-agreement": "fixtures/co-commerce/ord-2755-unsigned-agreement.pdf",
    },
    "co-castle-rock-archive.toml": {
        "castle-rock-res2025-071-event-json": "fixtures/co-castlerock/event-4406-2025-06-03.json",
        "castle-rock-res2025-071-votes-json": "fixtures/co-castlerock/votes-66424-res-2025-071.json",
        "castle-rock-res2025-071-detail": "fixtures/co-castlerock/res-2025-071-detail-captured-2026-11-10.html",
        "castle-rock-res2025-071-agreement": "fixtures/co-castlerock/res-2025-071-vendor-signed-agreement.pdf",
        "castle-rock-res2025-071-matter-json": "fixtures/co-castlerock/matter-10968-res-2025-071.json",
        "castle-rock-res2025-071-sole-source": "fixtures/co-castlerock/res-2025-071-sole-source-request.pdf",
    },
    "co-denver-archive.toml": {
        "denver-25-0413-event-json": "fixtures/co-denver/event-9502-2025-05-05.json",
        "denver-25-0413-votes-json": "fixtures/co-denver/votes-187514-flock-failed.json",
        "denver-25-0413-minutes": "fixtures/co-denver/minutes-2025-05-05.pdf",
        "denver-25-0413-proposed-amendment": "fixtures/co-denver/25-0413-proposed-failed-amendment.pdf",
        "denver-25-1967-1968-event-json": "fixtures/co-denver/event-9898-2025-12-08.json",
        "denver-25-1967-1968-consent-votes-json": "fixtures/co-denver/votes-199484-consent.json",
        "denver-25-1967-1968-minutes": "fixtures/co-denver/minutes-2025-12-08.pdf",
        "denver-25-1968-framework": "fixtures/co-denver/25-1968-proposed-framework.pdf",
        "denver-25-0413-matter-json": "fixtures/co-denver/matter-27055-flock-failed.json",
        "denver-25-1967-matter-json": "fixtures/co-denver/matter-28523-axon-amendment.json",
        "denver-25-1968-matter-json": "fixtures/co-denver/matter-28524-axon-framework.json",
        "denver-25-1967-vendor-amendment": "fixtures/co-denver/25-1967-vendor-signed-amendment.pdf",
    },
    "co-arapahoe-archive.toml": {
        "arapahoe-24-633-event-json": "fixtures/co-arapahoe/event-2335-2024-11-26.json",
        "arapahoe-24-633-consent-votes-json": "fixtures/co-arapahoe/votes-38020-consent-2024-11-26.json",
        "arapahoe-24-633-detail": "fixtures/co-arapahoe/24-633-detail.html",
        "arapahoe-24-633-minutes": "fixtures/co-arapahoe/minutes-2024-11-26.pdf",
        "arapahoe-25-378-event-json": "fixtures/co-arapahoe/event-2604-2025-07-22.json",
        "arapahoe-25-378-consent-votes-json": "fixtures/co-arapahoe/votes-40833-consent-2025-07-22.json",
        "arapahoe-25-378-detail": "fixtures/co-arapahoe/25-378-detail.html",
        "arapahoe-25-378-minutes": "fixtures/co-arapahoe/minutes-2025-07-22.pdf",
        "arapahoe-24-633-matter-json": "fixtures/co-arapahoe/matter-4198-24-633.json",
        "arapahoe-24-633-board-report": "fixtures/co-arapahoe/24-633-board-report.pdf",
        "arapahoe-24-633-unsigned-agreement": "fixtures/co-arapahoe/24-633-unsigned-agreement.pdf",
        "arapahoe-25-378-matter-json": "fixtures/co-arapahoe/matter-4672-25-378.json",
        "arapahoe-25-378-board-report": "fixtures/co-arapahoe/25-378-board-report.pdf",
        "arapahoe-25-378-unsigned-expansion": "fixtures/co-arapahoe/25-378-unsigned-expansion.pdf",
    },
}
EXPECTED_TYPES = {"json": "official_api", "html": "html_page", "pdf": "pdf"}


def main():
    ids = set()
    for profile_name, expected in PROFILE_FIXTURES.items():
        profile = tomllib.loads((ROOT / "configs/states" / profile_name).read_text())
        assert profile["state_code"] == "CO"
        actual = {source["id"]: source for source in profile["sources"]}
        assert set(actual) == set(expected), profile_name
        for source_id, fixture in expected.items():
            assert source_id not in ids, source_id
            ids.add(source_id)
            source = actual[source_id]
            registered = REGISTRY[fixture]
            assert registered["acquisition_source_id"] == source_id, source_id
            assert registered["acquisition_config_digest"] == config_digest(source), source_id
            raw = (ROOT / fixture).read_bytes()
            assert hashlib.sha256(raw).hexdigest() == registered["sha256"], source_id
            assert source["url"] == registered["url"], source_id
            assert source["source_type"] == EXPECTED_TYPES[registered["format"]], source_id
            assert source["maximum_bytes"] >= len(raw) > 0, source_id
            assert source["minimum_interval_seconds"] >= 86400, source_id
            assert urlparse(source["url"]).scheme == "https", source_id
            if "consent-votes" in source_id:
                assert "consent" in source["name"].lower(), source_id
                assert "not" in source["name"].lower(), source_id
    print(f"{len(ids)} fixed archive endpoints matched preserved source URLs and SHA-256 bytes")


if __name__ == "__main__":
    main()
