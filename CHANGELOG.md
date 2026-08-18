# Changelog

All notable changes to Panopticon Null are documented here. This project adheres to semantic versioning.

## 0.0.3 — "The Procurement Chain"

Turns isolated evidence receipts into a verifiable institutional money trail
(solicitation → amendment → award → contract → expenditure). Governed by the rule
**"follow the money without inventing the links"**: records are connected only when the
evidence supports the connection.

1. **Source-authority model and persistent coverage ledger.** Every procurement source
   carries an authority classification (authoritative, official informational mirror,
   official financial export, official meeting/legislative record, operator-supplied,
   unreviewed, restricted/inaccessible). Every acquisition records source identity,
   retrieval timestamp, persisted-byte SHA-256, HTTP provenance, parser/schema version,
   date range, record count, completion state, authority, failures, completeness, and
   review state. Coverage states include `complete`, `partial`, `informational_only`,
   `access_blocked`, `terms_unreviewed`, `schema_changed`, and `unknown` (default
   `unknown`/`partial`). Absence from a partial source is phrased "Not observed in the
   checked sources," never "No contract exists."
2. **Immutable source snapshots + change detection.** Every fetched page/export/document is
   an immutable snapshot. Changed official bytes produce a second snapshot linked by
   revision/supersession with a deterministic record-level diff; old artifacts and derived
   observations are never rewritten. A `304` records provenance without duplicating the
   artifact. Embedded links are never auto-followed.
3. **Procurement domain model.** Matters, events (solicitation published, amendment,
   award announced, contract executed, expenditure reported, record corrected/removed),
   identifiers (never merged across differing formats without a deterministic rule +
   tests), money (never floating point; raw strings preserved; exact/zero/N/A/various/
   IDIQ/unknown/unparseable kept distinct), and organizations (source spelling preserved;
   non-exact matches to human review; no auto-merging of subsidiaries/joint ventures/
   similarly named firms).
4. **Bounded ingestion adapters.** Contract-award table with row-level provenance;
   solicitation mirror carrying its incompleteness warning; documented negative capability
   finding for OpenBook COS (budget-level only, no vendor-level expenditure linkage); safe
   operator-supplied public-record import path treating supplied files as hostile.
5. **Reconciliation.** Automatic connections only via exact normalized identifiers,
   explicit official relationships, or existing evidence-backed relationships. A
   reconciliation-review queue holds candidate matches, vendor aliases, conflicting
   amounts/dates, duplicate/revised rows, missing documents, and vanished records; every
   decision is immutable and auditable.
6. **Case files.** Deterministic JSON + Markdown case files with a chronological timeline,
   organizations in documented roles, raw and parsed money, exact citations, source-authority
   labels, contradictions, missing documents, coverage, provenance, a SHA-256 manifest, and
   a limitations section. Files stay drafts until the citation-review and publication
   allowlists pass.
7. **Gap-driven CORA drafts.** A command generates a local, unsent Colorado Open Records Act
   draft from unresolved gaps. It never sends, never guesses an email recipient, and states
   that operator/legal review is required.
8. **Real case study + benign control.** The Next-Generation Transit Fare Collection System
   RFI (R26-023AB) is ingested as an RFI (not an award or contract) with no mass-surveillance
   labeling and an explicit gap where no executed contract or payment was located. A benign
   control matter (Crack Seal Materials award) proves ingestion does not automatically turn
   every purchase into a surveillance accusation.
9. **Hostile-input tests + CSV safety.** Malformed/deeply nested HTML, unexpected columns,
   duplicate/reordered rows, Unicode and hostile vendor names, huge numbers, currency
   ambiguity, broken CSV quoting, and source schema drift are exercised. CSV exports
   neutralize spreadsheet-formula injection.
10. **Schema v2 + real 0.0.2 upgrade fixture.** `SCHEMA_VERSION = 2`; transactional
   migration preserves all 0.0.1/0.0.2 rows byte-for-byte; upgrade test loads a real 0.0.2
   database fixture; failure-injection tests prove atomic rollback.

Also in 0.0.3: the nine-crate layout (added `pnull-procurement`); expanded documentation
(`docs/procurement-methodology.md`, `docs/migration-v0.0.3.md`, `docs/validation-0.0.3.md`,
`docs/0.0.3-source-survey.md`).

## 0.0.2 — "The Verifiable Receipt"

Deliverables added in this release:

1. **Page-accurate PDF citation geometry.** Immutable text maps (evidence ID, page, page width/height, rotation, coordinate system `pdf_user_space_points_bottom_left_y_up`, extracted words, word bounding boxes, extractor + version, text-map digest, relation to source digest). Bounding rectangles are validated (negative coordinates, inverted rects, out-of-bounds, missing pages, quote mismatch all rejected). New commands: `pnull citation show <id>`, `pnull citation render <id> --output review.png`. OCR uses deterministic Tesseract TSV with a pixel-to-page transform; OCR confidence is metadata, never proof.
2. **Immutable processing-run provenance.** Every ingestion/reprocessing job records schema version, Panopticon Null version, source revision, rules digest, state-config digest, input evidence IDs, native tool names + versions (Poppler, Tesseract, bubblewrap, prlimit), sandbox backend + version, configured budgets, actual aggregate resource consumption, start/end timestamps, outcome, structured errors, and output artifact IDs + digests. Stored as supplemental records; v0.0.1 records are not rewritten. Offline tests inject clocks and build metadata.
3. **Real extraction sandbox + aggregate job budgets.** Linux bubblewrap sandbox (no network namespace, no inherited secrets, no writable access outside a dedicated temp output dir, read-only exact inputs, new process/session boundaries, CPU/memory/file-size/output-size/wall-time limits via prlimit, cleanup on success/failure/timeout/interrupt). Live PDF/OCR ingestion fails closed when the sandbox cannot be established. Aggregate budgets bound downloaded bytes, attachments, PDF pages, OCR pages, extracted bytes, child processes, CPU, and wall clock.
4. **DNS-safe HTTP provenance + conditional retrieval.** Per-request/redirect provenance; never persists cookies/authorization headers/bearer tokens; rejects non-public addresses; fails closed on mixed public + prohibited DNS answers; HTTPS required with no cert-validation disabling; conditional requests via `If-None-Match`/`If-Modified-Since` (a 304 references prior evidence, never a new blob); resolver/transport abstractions keep CI offline.
5. **Persistent robots/terms review.** `pnull source review capture/record/show/verify`. Immutable review artifacts; live retrieval refuses on no review, expiration, config change, host change, out-of-scope endpoint, or prior restriction. The ephemeral `--robots-reviewed` flag is deprecated.
6. **Bounded Legistar pagination + attachment discovery.** One request at a time, configurable page size, hard max pages/events/matters/attachments, aggregate budgets, dedup by official identifiers, repeated-page detection, deterministic ordering, conditional requests, fail-closed on malformed identifiers/unknown hosts. Discovery only through documented official fields + reviewed hosts. Commands: `pnull ingest`, `pnull matter list/show/attachments`.
7. **Explicit subjects/actions/document roles.** Versioned domain types (Subject, Action, DocumentRole). Every action identifies its exact subject + citations; no action transfer between subjects in the same matter. A regression test proves Ordinance 25-93 approval cannot become an Axon/Flock purchase assertion.
8. **Human citation-review queue.** Append-only Pending/Approved/Rejected/NeedsContext/Superseded; each decision binds exact digests; changing any bound value invalidates approval; structured publication allowlists. Commands: `pnull review list/show/approve/reject/supersede`. Site/Atom/X fail closed on pending/rejected/stale/mismatched decisions.
9. **Safe reconciliation of uncertain X threads.** Dry-run default; `pnull x attempts/status/reconcile`; append-only operator decisions; no blind retry; no audit-history deletion; no live X transport in tests/demos.
10. **Second genuine Colorado Springs matter.** Ordinance No. 15-84 (2015), matter 15-00663, preserved at `fixtures/co2/`. The surveillance-technology link is documented via the preserved 2025 presentation as supporting evidence, not asserted by the 2015 action itself.

Also in 0.0.2: SQLite schema versioning (`PRAGMA user_version`, `SCHEMA_VERSION = 1`) with transactional v0.0.1 upgrade, committed migration fixture, and rollback on failure; the eight-crate layout (added `pnull-geometry`, `pnull-http`); expanded documentation.

## 0.0.1

Initial release: one Colorado Springs vertical slice with official Legistar ingestion, preserved evidence, deterministic taxonomy, cautious classifications, meaningful version changes, SQLite, static publication, Atom feed, and citation-bound dry-run X drafts.
