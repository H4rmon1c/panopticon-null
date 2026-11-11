# Four-case Colorado static release

Historical release record. The [current production release](colorado-production-release.md)
adds Castle Rock after its preserved source revision was reconciled.

The checked-in public snapshot contains Colorado Springs Ordinance 25-93,
Commerce City Ordinances 2755 and 2756, and Denver Resolution 25-0413. Denver's
motion **failed**; its 12 Nay and one Absent positions are not contract
approvals. No consent-block vote is attributed to a specific item. Castle Rock
is excluded because its current detail-page bytes differ from the fixture
digest. The four-case snapshot has 44 entities, 200 evidence entries, and 12
unique cited sources. It is a static artifact, not a deployed website.

The existing Colorado Springs release decision and the 12 newly captured
Colorado endpoint reviews live in separate private stores. Prepare a new
private release store without changing either original:

```sh
python3 deploy/selfhost/prepare-four-case-store.py \
  --capture-data-dir /absolute/path/to/colorado-capture/.pnull \
  --approval-data-dir /absolute/path/to/colorado-springs-approval/.pnull \
  --output-data-dir /absolute/path/to/new-private-release-store
```

The preparation script requires the exact single Colorado Springs approval
and atlas allowlist already recorded, copies the preserved raw bytes and
SQLite state, and refuses an existing output directory. The exporter rechecks
all source reviews, fetch receipts, raw bytes, locators, case release bindings,
and the configured canonical HTTPS origin. From the repository root:

```sh
pnull --data-dir /absolute/path/to/new-private-release-store \
  --config configs/states/co.toml --site-url https://panopticon.fail \
  atlas export \
  --manifest fixtures/atlas/colorado-springs-25-93.v1.json \
  --manifest fixtures/atlas/cases/commerce-city-ord-2755-2026-04-06.v2.json \
  --manifest fixtures/atlas/cases/commerce-city-ord-2756-2026-04-06.v2.json \
  --manifest fixtures/atlas/cases/denver-25-0413-2025-05-05.v2.json \
  --output web/panopticon-terminal/public/public-snapshot.json
node web/panopticon-terminal/scripts/validate.mjs
node --test web/panopticon-terminal/scripts/test-client.mjs
```

The checked-in snapshot SHA-256 is
`eaf091963b9558193ba5b3924f73b96a041eb53e786793dd5190e74a1483c8bf`.
An independent preparation and export produced that exact digest.

## Corrections and withdrawal

`atlas review --decision reject` on a case's current release binding vetoes
that case. A production export listing it fails before replacing the output.
To withdraw it from the next release, remove its manifest from
`PNUL_MANIFEST_PATHS`, add it to `PNUL_WITHDRAWN_MANIFESTS`, and rebuild. The
exporter requires a current human rejection bound to that exact manifest and
emits a sanitized withdrawal record; the static page generator writes a
`noindex` correction notice at the former share URL. The case's entities and
citations disappear together. Reviewer notes stay private. Keep the previous
published artifact and append-only review history privately for audit. A
corrected manifest can return to the active list only after all source and
release gates pass. The publisher and browser regressions cover rejection,
atomic failure, the old URL notice, exclusion, and reviewed rerelease.

For a proposal-backed case, record an exact rejection against its prior
`atlas-release` binding, remove its `PNUL_PROPOSAL_INPUTS` entry, and put its
case ID in `PNUL_WITHDRAWN_CASE_IDS`. The publisher reads the current public
snapshot as `--previous-snapshot` and verifies that the rejected binding was
actually released before writing a noindex notice. Keep the case ID configured
on later runs to carry that notice forward. A corrected proposal can join as a
new case after its source checks pass; keep the old case's withdrawal notice.

The public site has not been deployed.
