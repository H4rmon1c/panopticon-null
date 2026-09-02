# Validation report — v0.0.4 ("The Public Ledger")

This report states exactly what was proven by the v0.0.4 validation suite and what remains
unknown. It is an honest account, not a claim of perfection or legal compliance.

## What 0.0.4 adds

v0.0.4 ("The Public Ledger") closes four gaps left at the end of v0.0.3, when the verifiable
procurement chain ended in the local state directory:

1. **Procurement change alerts (Item 1).** Re-ingesting a reviewed procurement surface and
   finding the new snapshot differs from the latest produces deterministic, idempotent
   change alerts (`record_added`, `record_modified`, `record_removed`). Award-row
   `record_modified` carries a field-level diff (field name, old raw value, new raw value).
   Row identity is a stable key (official identifier where present, otherwise a digest over
   the row's normalized field values). Alerts flow into the existing `Alert` store contract,
   so `pnull alerts` lists both kinds, and the X pipeline reuses its existing dry-run /
   exact-digest / canonical-URL / credentials / reconciliation gates verbatim.
2. **Publish the procurement chain (Item 2).** `pnull build-site` renders the procurement
   chain from the same deterministic case-file JSON that `case build` produces — one source
   of truth, no second renderer. `/co/procurement/index.html` and per-matter pages render a
   chronological timeline, roles, raw/parsed money, citations, contradictions, coverage
   gaps, a "what changed" section built from snapshot supersessions, provenance, a SHA-256
   manifest, and a limitations block. Every citation requires an Approved citation-review
   decision bound to the exact digests; a `procurement_casefile` publication-allowlist
   category is required; the privacy backstop runs over all rendered procurement text
   including vendor names and raw money strings. Published matters and change alerts appear
   in the Atom feed under the identical gates. `pnull procurement publish-ready <matter-id>`
   reports gate state without publishing.
3. **CORA request ledger (Item 3).** An append-only, fully local request ledger connects
   `procurement gaps` → CORA draft → human submission → response import → case-file gap
   update. States: `drafted`, `submitted`, `response_received`, `gap_resolved`,
   `still_unresolved`. The tool never sends anything, never guesses a recipient, and never
   claims a legal deadline or entitlement. Commands: `pnull cora list/show/submit/response`.
   No transition may be reversed or edited; corrections are new events.
4. **In-place edit demonstration (Item 4).** A second contract-award snapshot
   (`fixtures/procurement/contract-awards-2.html`, a labeled synthetic demonstration fixture
   derived from the preserved official snapshot) proves the supersession + diff + alert
   pipeline catches the exact in-place edits the source survey documented: one amount
   edited, one vendor name altered, one row removed, one row added.
5. **Explicit official-relationship links (Item 5).** A conservative deterministic link
   class: a source adapter may declare reference fields, and a link (`kind
   official_relationship`) is recorded only when a declared reference field of one preserved
   record contains an exact match of an identifier stored for another record and both
   endpoints resolve to stored snapshots with valid SHA-256 digests. Near-miss references
   become candidates in the reconciliation review queue, never auto-links.
6. **`pnull procurement refresh` (Item 6).** The exposure heartbeat: `--dry-run` (default)
   prints the planned fetch with zero network activity; `--live` requires the persistent
   source-review gate, fetches one surface at a time, records a new snapshot (or 304
   provenance), runs change detection, and writes a coverage-ledger entry. On any refusal or
   failure it fails closed and changes nothing.

## Validation commands

The following validation commands pass for this release:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- `cargo deny check`
- `nix --extra-experimental-features 'nix-command flakes' flake check --print-build-logs`
- `sha256sum -c fixtures/co/SHA256SUMS`
- `sha256sum -c fixtures/co2/SHA256SUMS`
- `sha256sum -c fixtures/procurement/SHA256SUMS`
- `cargo run --locked -p pnull-cli -- demo`

`cargo deny check` is provided by the pinned Nix environment (`cargo-deny`); it is exercised
inside `nix flake check` as the `dependency-policy` check, which runs `cargo deny --offline
check` against the pinned RustSec advisory database. In a plain non-Nix shell the command is
not on PATH; the authoritative path is the Nix flake check.

## The demo

The offline demo (`cargo run --locked -p pnull-cli -- demo`) is proven to:

- run entirely offline using preserved official fixtures and the labeled synthetic second
  snapshot (no network access, zero network posts);
- verify the procurement fixture SHA-256 digests before ingestion;
- ingest the solicitation mirror and the first contract-award snapshot, then **re-ingest
  the second contract-award snapshot** and demonstrate end to end: the supersession
  relationship, the record-level diff, the resulting change alerts (record added /
  modified / removed), the `RecordCorrected`/`RecordRemoved` events on the affected matter,
  and the "what changed" section in the case file and site;
- build the transit-fare RFI matter, the benign control matter, and a derived matter for the
  in-place-edit demonstration (carrying the synthetic label);
- generate deterministic case files (JSON + Markdown) with citations, coverage, gaps,
  relationships, a "what changed" section, and a limitations section;
- generate a local, unsent CORA draft and register the transit-fare request in `drafted`
  state with a deterministic timestamp (no request state is published beyond that);
- generate a dry-run X draft for a procurement change alert using deterministic, clearly
  labeled demonstration review decisions (no transport is constructed);
- publish procurement pages and Atom entries under the full review/privacy gates;
- export the award rows as a formula-neutralized CSV;
- generate a JavaScript-free static site and an Atom feed;
- be reproducible byte-for-byte: the test runs the demo in two clean directories and asserts
  that every generated output tree (`site/`, `state/records/`, the procurement chain
  output, and the new publication output) is identical, and that `network-posts.txt`
  contains `0`.

## Real sources and fixtures

The 0.0.3 procurement fixtures preserve the following official surfaces, retrieved one
request at a time with no authentication, access-control bypass, or browser automation.
Exact URLs and hashes are in `fixtures/README.md` and `fixtures/procurement/SHA256SUMS`.

| Surface | Authority classification | Coverage state | Note |
|---|---|---|---|
| City contract-award table | Official informational mirror | `partial` | Parsed to award rows with row-level provenance |
| City solicitation list | Official informational mirror | `informational_only` | Carries the source's own incompleteness warning |
| OpenBook COS / Socrata export | Official financial export | `partial` | Budget-level only; no vendor-level expenditure linkage (documented negative finding) |
| BidNet / Bonfire | Authoritative procurement portal (per City) | `access_blocked` | Registration/terms-restricted; not automated |
| Legistar meeting records (0.0.1/0.0.2) | Official meeting/legislative record | — | Reused, unchanged |

### The synthetic second snapshot (Item 4)

`fixtures/procurement/contract-awards-2.html` is a **synthetic demonstration fixture derived
from the preserved official snapshot; not an official record**. It was not retrieved from any
live endpoint. Relative to the preserved official `contract-awards.html` it: edits one amount
and one vendor name and adds notes on the `Q25-130ZM` LogRhythm renewal row (a
`record_modified` field-level diff); removes one row (`R24-T114JD`) to demonstrate
`record_removed`; and adds one row (`R25-044AB`) to demonstrate `record_added`. It is labeled
as synthetic in `fixtures/README.md`, in its filename context, and in every demo output that
references it, and is never presented as official bytes. Its SHA-256 is committed in
`fixtures/procurement/SHA256SUMS`.

The survey documented that the City edits award rows in place; the demo proves the
supersession + diff + alert pipeline catches exactly that class of change. Digests of the
first snapshot are unchanged; the supersession links both snapshots; a re-run is idempotent.

## The real case study

The real case study is the **Next-Generation Transit Fare Collection System RFI
(R26-023AB)** for Mountain Metropolitan Transit. It is a **Request for Information (RFI)**,
not an RFP, award, contract, or purchase. The preserved City-hosted documents state this
explicitly.

- The RFI is revalidated against the live official identifier and documents.
- It is not labeled as mass surveillance merely because it handles data; only documented
  capabilities, data practices, requirements, and institutional actions are extracted.
- No award or payment record for the fare system was located in the checked sources, so the
  case file shows that exact gap and the CORA request targets it. No contract or payment
  relationship is invented.
- The transit-fare matter's page is the restraint demonstration: no surveillance labeling
  appears anywhere on the page.

A **benign control matter** (Crack Seal Materials, award under `B22-T168KK`) is also
ingested and its page published. It proves that ingestion and publication do not
automatically turn every materials purchase into a surveillance accusation: the control
matter's page contains no surveillance-category text.

## Migration

The schema advances to `SCHEMA_VERSION = 3` (`MAX_SUPPORTED_SCHEMA_VERSION = 3`) through a
transactional migration that preserves all 0.0.1, 0.0.2, and 0.0.3 rows byte-for-byte and
never rewrites old evidence or processing history. The migration is additive: it creates the
v0.0.4 tables `procurement_alerts`, `cora_requests`, `official_relationships`, and
`supplied_records`.

- The upgrade test loads the committed fixture `fixtures/migration/v0.0.3-minimal.sql` (a
  real 0.0.3 database) and proves every canonical row is preserved byte-for-byte.
- Ledger entries (CORA requests, change alerts, official-relationship links, supplied
  records) survive migration byte-for-byte.
- Failure-injection tests prove migration failure rolls back atomically.
- Migration is idempotent; a newer unsupported schema is rejected.

## Security and epistemic controls added or strengthened

- **Change alerts are comparisons, not conclusions.** A `record_removed` alert is phrased
  "The row observed in snapshot N (digest …) is not present in snapshot M (digest …)." No
  alert declares conduct unlawful, corrupt, abusive, or malicious. Alert ids are stable and
  idempotent: re-ingesting the same snapshot pair never creates a second alert, and a
  byte-identical re-ingest creates no alerts.
- **No surveillance labeling.** If a row title or vendor name matches the published
  surveillance taxonomy, it appears only as optional metadata — "surveillance-related
  terminology observed, rule `<rule-id>`" — never "surveillance purchase" or "surveillance
  award."
- **Publication gates fail closed.** Every procurement citation requires an Approved
  citation-review decision bound to the exact digests. Pending, rejected, stale, or
  mismatched review state, or a missing `procurement_casefile` publication-allowlist
  category, removes the page/entry from the build with a visible "publication withheld
  pending review" note — never a partial page. The privacy backstop (plates, personal
  contact fields, SSNs, home-address patterns, coordinates, movement logs) runs over all
  rendered procurement text, including vendor names and raw money strings.
- **CORA ledger is append-only and never sends.** Transitions are immutable events with
  operator, timestamp, and note; duplicate transitions and unknown evidence ids are
  refused; the tool stores operator-supplied submission facts, it does not perform them.
- **Official-relationship links are never invented.** A link is recorded only on an exact
  identifier match in a declared reference field with both endpoints digest-bound;
  free-text co-occurrence and near-miss identifiers produce no link (candidates enter the
  reconciliation review queue).
- **Refresh fails closed.** `--live` refuses on no review, expired review, config change,
  host change, or out-of-scope endpoint, one request at a time under aggregate budgets.
  `--dry-run` (the default) makes zero transport calls.

## Test results

All tests across the nine-crate workspace pass. New test coverage includes:

- **Item 1:** added row; edited row (amount change, vendor change, date-format drift,
  IDIQ/`various` money preserved raw); removed row; byte-identical re-ingest (no alert);
  same pair re-ingested (no duplicate alert); hostile rows (Unicode, embedded quotes,
  formula-injection strings, huge numbers); row with no official identifier (stable digest
  key works and stays stable across reorders).
- **Item 2:** pending citation → page withheld; rejected → withheld; superseded → withheld;
  allowlist missing category → withheld; hostile vendor name containing plate-like and
  SSN-like strings → privacy backstop withholds; control-matter page contains no
  surveillance-category text; two builds in clean directories are byte-identical.
- **Item 3:** full lifecycle happy path against a fixture response; duplicate transition
  refused; response evidence id not present refused; ledger entries survive migration
  byte-for-byte; case-file gap section updates for both `gap_resolved` and
  `still_unresolved`.
- **Item 4:** the demo's second-snapshot step produces the expected alert kinds and event
  kinds; digests of the first snapshot are unchanged; supersession links both snapshots; a
  re-run is idempotent.
- **Item 5:** positive case → link recorded with citations; free-text co-occurrence → no
  link; identifier present but endpoint snapshot missing or digest invalid → no link; same
  field with near-miss identifier → no link, candidate queued; link survives migration
  byte-for-byte; the demo records zero links for the preserved ordinance matter and the
  test proves that absence rather than fabricating one (no preserved record carries an
  explicit procurement-identifier reference in a declared reference field).
- **Item 6:** dry-run makes zero transport calls and prints the planned comparison; live
  path with a fake transport returning changed bytes → new snapshot + alerts; live path
  with identical bytes → 304-style provenance, no alerts; live path with no source review →
  refused, nothing written; expired review refused; idempotent re-ingest; unknown source
  refused.
- **pnull-x:** procurement change-alert drafts link the affected matter's case-file page
  (and fall back to the change-alert page when no matter id resolves) while reusing the
  existing pipeline gates.

A second execution over the same fixtures produces the same normalized records, citations,
case-file JSON, Markdown, site pages, Atom entries, and manifest digests, excluding
explicitly documented runtime metadata (for example, operator approval timestamps stored in
SQLite rather than canonical evidence JSON).

## What was proven

- A second snapshot demonstrating in-place edits produces a supersession, a record-level
  diff, change alerts of all three kinds, the `RecordCorrected`/`RecordRemoved` events, and
  a "what changed" section — idempotently and with the first snapshot's digests unchanged.
- Change alerts are deterministic, idempotent, comparison-only, and flow through the
  existing `Alert` store and X pipeline gates.
- The procurement chain publishes to a static site and Atom feed from the same deterministic
  case-file JSON, failing closed on every review/privacy gate.
- The CORA request ledger tracks a request end to end, append-only, with a case-file gap
  section that updates for both `gap_resolved` and `still_unresolved`.
- Official-relationship links are recorded only on exact, digest-bound, declared-field
  matches, and near-misses enter the review queue rather than auto-linking.
- `pnull procurement refresh` keeps the ledger fresh in real operation while dry-run makes
  zero transport calls and live mode fails closed on the persistent source-review gate.
- All prior 0.0.1/0.0.2/0.0.3 behavior and evidence survive migration and still pass.

## What remains unknown

- **Procurement coverage is not comprehensive.** Only the checked surfaces were reviewed;
  other contracts and amendments may exist only in sources this project has not reviewed.
- **BidNet / Bonfire are not automated.** They are the City's authoritative portals but are
  registration/terms-restricted; records there were not scraped.
- **No vendor-level payment linkage from OpenBook.** OpenBook may not provide vendor-level
  payment evidence; the negative finding is documented.
- **No executed fare-system contract or payment is asserted.** None was located in the
  checked sources; the gap is shown and targeted by a CORA request.
- **The second snapshot is synthetic, not live.** No live re-fetch was recorded for the
  in-place-edit demonstration; `contract-awards-2.html` is a labeled synthetic fixture. A
  real live re-fetch under the persistent source-review process, if the live page ever
  changes, would exercise the same pipeline against official bytes.
- **Zero official-relationship links in the demo.** The preserved records reviewed do not
  carry an explicit procurement-identifier reference in a declared reference field, so the
  demo records zero `official_relationship` links and proves that absence rather than
  fabricating a link. The mechanism is tested, not yet demonstrated against preserved
  official bytes.
- **Refresh change detection for removed/modified rows reads prior rows from the preserved
  fixture on disk.** Source snapshots store metadata and a persisted-byte digest, not the
  raw row bytes, so a live refresh compares the newly fetched surface against the preserved
  prior fixture (offline) rather than a byte re-parse of the prior snapshot. Without the
  prior fixture on disk, only added rows are reported. This is an honest limitation of the
  live refresh path, not a claim of full historical diffing from snapshot metadata.
- **Legal compliance.** No legal conclusions are made, and this report offers no guarantee
  that any republication or data-handling practice is lawful in every jurisdiction.
- **Boundary robustness.** Sandbox, HTTP, privacy-backstop, and review-gate tests prove the
  specified behaviors under the tested conditions but do not prove absence of latent
  weaknesses. Pattern checks cannot reliably catch every sensitive value; human review
  remains a required boundary.
- **Live behavior.** Validation is offline by design; live retrieval under reviewed terms
  is an operator responsibility and is not exercised by this report.

## Deliberate gaps

Section 6 of the build prompt allows a documented gap rather than a broken invariant. The
deliberate gaps this release accepts are:

- **No live second-snapshot demonstration (Item 4).** The preferred real live re-fetch was
  not possible to record offline and no review could be performed here, so the second
  snapshot is a labeled synthetic fixture. A live re-fetch under the persistent source-review
  process would close this gap.
- **Zero demonstrated official-relationship link (Item 5).** The mechanism is implemented
  and tested, but no preserved record in the fixtures carries an explicit
  procurement-identifier reference in a declared reference field, so the demo records zero
  links and proves absence. A future preserved record containing such a reference would
  close this gap.
- **Refresh change detection reads prior rows from the preserved fixture on disk (Item 6).**
  Source snapshots do not store raw row bytes, so removed/modified-row detection in a live
  refresh depends on the prior fixture being present offline. Storing raw persisted bytes in
  snapshots (an additive schema change) would close this gap.

These gaps are recorded here so they are not mistaken for claims of completeness.
