# Colorado jurisdiction roster

The public site serves [colorado-jurisdictions.json](colorado-jurisdictions.json) as a **geographic discovery index**. It never calls Census or Colorado DOLA APIs at runtime. A roster row does not assert that a jurisdiction approved, bought, deployed, or even considered surveillance technology. The browser shows “Records not indexed” unless a released decision in the separately validated public snapshot is linked through a jurisdiction entity.

## Current extract

| Kind | Rows | Authority and scope |
| --- | ---: | --- |
| State | 1 | Colorado (Census state code 08) |
| Counties | 64 | U.S. Census 2025 cartographic county file |
| Incorporated cities and towns | 273 | U.S. Census 2025 Colorado place file, LSAD 25 and 43 only |
| Unified school districts | 178 | U.S. Census 2025 Colorado unified school district file |
| Other district discovery entries | 3,595 | Colorado DOLA “All Special Districts” layer, by nonblank unique LGID, excluding its 178 school district rows |

The Census legal boundaries and names are **as of January 1, 2025**. All four source files were archived on November 10, 2026. The DOLA dataset description calls its layer a first attempt toward a statewide special district dataset and explicitly disclaims boundary accuracy. We preserve DOLA's source type and status codes but **do not interpret those codes as a verified legal status**. DOLA entries are labeled provisional in the interface.

The Census place file also contains 209 census-designated places. Those are statistical geographies, not incorporated municipalities, and are excluded from the jurisdiction total. DOLA's district layer includes 40 rows without a usable LGID/name and 178 school district rows; those are also excluded from the 3,595 other district entries. The school districts come from the Census file instead.

This roster is not a legal claim to include every Colorado public authority. Tribal governments, state and local departments, public authorities, institutions of higher education, and certain intergovernmental bodies need separate sourced registries. District formation and dissolution can also occur after this extract. The index can accept those as additional kinds without changing the published-case boundary.

## Source preservation and controlled refresh

The exact downloaded source bytes are stored in [source-archives](source-archives). Its [manifest](source-archives/manifest.json) records each archive's SHA-256 digest, download date, and official URL. The builder verifies those digests. Only the derived JSON is in the web release allowlist; the archives are not served to visitors. An official URL can change after this extraction, so its current response should not be assumed to match the recorded digest.

To rebuild offline:

```sh
python3 -m pip install -r web/panopticon-terminal/scripts/requirements-geography.txt
python3 web/panopticon-terminal/scripts/build-colorado-geography.py
node web/panopticon-terminal/scripts/test-colorado-geography.mjs
```

For a refresh, download all four files from the exact official URLs in `source-archives/manifest.json` into `source-archives` using the existing filenames. Keep a copy of the previous archives outside the release tree. Update the manifest's digests and archive date only after inspecting the downloads, then rebuild and test. The builder stops if the expected counts change, requiring a deliberate review before a new geography release. Do not treat a geography refresh as a case publication.

The case join is deliberately narrow. The legacy Colorado Springs record uses its published jurisdiction entity ID. New releases should set a stable `jurisdiction_ref` on each `JURISDICTION` entity, matching this registry's `id`, and attach decision `jurisdiction_ids` to that entity. The browser checks the validated public snapshot before displaying a decision link; preview and synthetic snapshots never create public case links.
