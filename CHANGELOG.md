# Changelog

All notable changes to Panopticon Null are documented here. This project adheres to semantic versioning.

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
