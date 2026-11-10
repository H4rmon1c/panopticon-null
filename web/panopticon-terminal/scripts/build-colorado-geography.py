#!/usr/bin/env python3
"""Build a neutral Colorado jurisdiction roster, separate from published cases.

Install the pinned pyshp dependency in requirements-geography.txt before running.
Use --source-dir to rebuild from previously downloaded source bytes.
"""

import argparse
from collections import Counter
from datetime import date
import hashlib
import io
import json
from pathlib import Path
import zipfile

import shapefile


ROOT = Path(__file__).resolve().parent.parent
SOURCE_KEYS = ("census_counties", "census_places", "census_schools", "dola_districts")


def shapefile_rows(data):
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        shp = next(name for name in archive.namelist() if name.endswith(".shp"))
        reader = shapefile.Reader(
            shp=io.BytesIO(archive.read(shp)),
            shx=io.BytesIO(archive.read(shp[:-4] + ".shx")),
            dbf=io.BytesIO(archive.read(shp[:-4] + ".dbf")),
        )
        for item in reader.iterShapeRecords():
            yield item.record.as_dict(), item.shape.__geo_interface__


def rounded_coordinates(value):
    if isinstance(value, (tuple, list)):
        return [rounded_coordinates(part) for part in value]
    return round(value, 5)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-dir", type=Path, default=ROOT / "geography" / "source-archives")
    parser.add_argument("--output", type=Path, default=ROOT / "geography" / "colorado-jurisdictions.json")
    args = parser.parse_args()
    manifest = json.loads((args.source_dir / "manifest.json").read_text())
    archived_on = date.fromisoformat(manifest["archived_on"]).isoformat()
    if set(manifest["files"]) != set(SOURCE_KEYS):
        raise ValueError("Source archive manifest must name all four expected sources")
    raw = {
        key: (args.source_dir / manifest["files"][key]["file"]).read_bytes()
        for key in SOURCE_KEYS
    }
    digests = {key: hashlib.sha256(value).hexdigest() for key, value in raw.items()}
    for key in SOURCE_KEYS:
        if digests[key] != manifest["files"][key]["sha256"]:
            raise ValueError(f"Source archive digest mismatch: {key}")

    records = [{
        "id": "us-co",
        "kind": "state",
        "name": "Colorado",
        "code": "08",
        "source_id": "census_counties",
        "source_locator": "STATEFP 08",
    }]
    for row, geometry in shapefile_rows(raw["census_counties"]):
        if row["STATEFP"] != "08":
            continue
        records.append({
            "id": f"us-co-county-{row['GEOID']}",
            "kind": "county",
            "name": row["NAMELSAD"],
            "code": row["GEOID"],
            "source_id": "census_counties",
            "source_locator": f"GEOID {row['GEOID']}",
            "geometry": {
                "type": geometry["type"],
                "coordinates": rounded_coordinates(geometry["coordinates"]),
            },
        })
    for row, _ in shapefile_rows(raw["census_places"]):
        # LSAD 25 = city and 43 = town. LSAD 57 = statistical CDP, not a municipality.
        if row["STATEFP"] != "08" or row["LSAD"] not in {"25", "43"}:
            continue
        record = {
            "id": f"us-co-place-{row['GEOID']}",
            "kind": "municipality",
            "name": row["NAMELSAD"],
            "code": row["GEOID"],
            "source_id": "census_places",
            "source_locator": f"GEOID {row['GEOID']}; LSAD {row['LSAD']}",
        }
        if row["NAME"] == "Colorado Springs":
            # This only becomes a case link if the released snapshot contains this ID.
            record["published_entity_id"] = "jurisdiction_co_colorado_springs"
        records.append(record)
    for row, _ in shapefile_rows(raw["census_schools"]):
        if row["STATEFP"] != "08":
            continue
        records.append({
            "id": f"us-co-school-{row['GEOID']}",
            "kind": "school_district",
            "name": row["NAME"],
            "code": row["GEOID"],
            "source_id": "census_schools",
            "source_locator": f"GEOID {row['GEOID']}",
        })

    district_rows = json.loads(raw["dola_districts"])
    seen_lgid = set()
    for row in district_rows:
        lgid = row.get("lgid", "").strip()
        name = row.get("name", "").strip()
        if not lgid or not name or row.get("lgtype_id") == "99.0":
            # The state layer contains 178 school districts; Census schools are
            # listed above to avoid duplicate entries under incompatible IDs.
            continue
        if lgid in seen_lgid:
            raise ValueError(f"Duplicate DOLA LGID {lgid}")
        seen_lgid.add(lgid)
        records.append({
            "id": f"dola-district-{lgid}",
            "kind": "district_provisional",
            "name": name,
            "code": lgid,
            "source_id": "dola_districts",
            "source_locator": f"lgId {lgid}",
            "source_status_code": row.get("lgstatus_i"),
            "source_type_code": row.get("lgtype_id"),
            "source_last_update": row.get("lastupdate"),
        })

    records.sort(key=lambda r: ({"state": 0, "county": 1, "municipality": 2, "school_district": 3, "district_provisional": 4}[r["kind"]], r["name"].casefold(), r["id"]))
    ids = [record["id"] for record in records]
    if len(ids) != len(set(ids)):
        raise ValueError("Duplicate jurisdiction ID")
    counts = dict(Counter(record["kind"] for record in records))
    if counts != {"state": 1, "county": 64, "municipality": 273, "school_district": 178, "district_provisional": 3595}:
        raise ValueError(f"Unexpected source counts: {counts}; inspect the source revision before release")
    registry = {
        "schema_version": 1,
        "source_archived_on": archived_on,
        "purpose": "Neutral jurisdiction discovery roster; no approval, contract, or case assertion is implied.",
        "counts": counts,
        "sources": {
            key: {
                "url": manifest["files"][key]["url"],
                "sha256": digests[key],
                "archived_on": archived_on,
                "as_of": "2025-01-01" if key.startswith("census") else None,
                "caveat": "First version of a statewide district boundary layer; source disclaims boundary accuracy and legal completeness."
                if key == "dola_districts" else None,
            }
            for key in SOURCE_KEYS
        },
        "jurisdictions": records,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(registry, ensure_ascii=False, separators=(",", ":")) + "\n")
    print(f"Wrote {len(records)} jurisdictions to {args.output}: {counts}")


if __name__ == "__main__":
    main()
