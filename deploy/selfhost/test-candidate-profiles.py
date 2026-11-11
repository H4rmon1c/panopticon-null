#!/usr/bin/env python3
"""Verify calendar candidate path policies stay narrow and unscheduled."""

from pathlib import Path
import tomllib
from urllib.parse import urlparse


ROOT = Path(__file__).resolve().parents[2]
PROFILES = (
    "co.toml",
    "co-commerce-city.toml",
    "co-denver.toml",
    "co-castle-rock.toml",
    "co-arapahoe-county.toml",
    "co-douglas-county.toml",
    "co-littleton.toml",
)
MATRIX = (ROOT / "deploy/selfhost/collection-sources.tsv").read_text()


def main():
    ids = set()
    for filename in PROFILES:
        profile = tomllib.loads((ROOT / "configs/states" / filename).read_text())
        sources = {source["id"]: source for source in profile["sources"]}
        calendars = [source for source in sources.values() if source["id"].endswith("-calendar")]
        assert len(calendars) == 1, filename
        calendar = calendars[0]
        calendar_url = urlparse(calendar["url"])
        assert calendar_url.scheme == "https" and calendar_url.path == "/Calendar.aspx"
        assert not calendar_url.query and not calendar_url.fragment
        prefix = calendar["id"].removesuffix("-calendar")
        for suffix, path, kind, interval, max_bytes in (
            ("meeting-documents", "/MeetingDetail.aspx", "candidate_meeting_html", 60, 4_194_304),
            ("pdf-documents", "/View.ashx", "candidate_document_pdf", 120, 20_971_520),
        ):
            source_id = f"{prefix}-{suffix}"
            source = sources[source_id]
            assert source_id not in ids, source_id
            ids.add(source_id)
            target = urlparse(source["url"])
            assert target.scheme == "https" and target.netloc == calendar_url.netloc
            assert target.path == path and not target.query and not target.fragment
            assert source["source_type"] == kind
            assert source["official_discovery_url"] == calendar["url"]
            assert source["documented_at"] == calendar["url"]
            assert source["minimum_interval_seconds"] >= interval
            assert 0 < source["maximum_bytes"] <= max_bytes
            assert source_id not in MATRIX, source_id
    assert len(ids) == 14
    print("14 configured candidate path policies are bounded and unscheduled")


if __name__ == "__main__":
    main()
