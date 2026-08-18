# Architecture

Panopticon Null is a lawful, nonviolent evidence infrastructure for dismantling institutional mass surveillance, focused on Colorado Springs. Version 0.0.3 ("The Procurement Chain") adds a source-authority/coverage ledger, immutable source snapshots with change detection, bounded procurement ingestion adapters, reconciliation rules, case-file generation, and gap-driven CORA drafts — on top of the 0.0.2 page-accurate citations, processing-run provenance, sandbox, DNS-safe HTTP, source review, explicit subjects/actions, citation-review queue, and X reconciliation.

## Vertical slice

```text
official HTTPS source / committed fixture
  → persistent robots/terms review gate
  → DNS-safe HTTPS retrieval (public addresses only, conditional requests)
  → bounded pagination + attachment discovery (Legistar)
  → SHA-256 content store + canonical JSON + SQLite
  → static extraction (HTML/text/JSON/PDF/OCR) inside a bubblewrap sandbox
  → deterministic YAML scan + exact page-accurate citations (pnull-geometry)
  → explicit subjects/actions + cautious state classification
  → human citation-review queue + publication allowlists
  → privacy gate
  → static HTML + Atom (JS-free)
  → local X draft → dry-run → exact-digest approval → optional confirmed transport
```

All durable state is local. No hosted service, JavaScript runtime, telemetry, analytics, advertising, or tracking is required.

## Crates (9)

- `pnull-core` owns the evidence, finding, alert, matter, subject, action, citation, review, processing-run, source-review, fetch-observation, X-attempt, publication-allowlist, and (v0.0.3) procurement schemas. IDs are domain-separated SHA-256 values. SQLite enforces durable uniqueness. Original bytes are stored under `evidence/sha256/<prefix>/<digest>`; canonical records are deterministic JSON.
- `pnull-ingest` validates metadata, restricts live retrieval to reviewed same-host public HTTPS redirects, enforces input limits, runs the bubblewrap extraction sandbox with per-job resource budgets, and extracts hostile content. It never executes scripts, macros, document attachments, or source content. PDF and OCR tools are allowlisted subprocesses with address-space, CPU, file, process, page, image, output, and wall-time bounds.
- `pnull-geometry` produces and validates page-accurate PDF citation geometry: immutable text maps, word bounding boxes, coordinate transforms, bounding-rectangle validation, and OCR confidence handling.
- `pnull-http` is the DNS-safe HTTP layer: public-address validation, allowlisted headers, conditional retrieval, and provenance-aware fetch observations.
- `pnull-detect` parses the reviewed YAML taxonomy. It records exact lines, limits strong states to cited context, rejects common negation/conditional forms, resolves conflicting states to `Unknown`, and records rule version and digest.
- `pnull-publish` validates all internal evidence references, applies publication gates and the human review queue, writes a complete temporary tree, then atomically replaces the previous site. Core reading and navigation require no JavaScript.
- `pnull-x` creates one post or a short thread, applies the same publication gate, binds approval to a canonical draft digest, records attempts and reconciliation, and hides the network transport behind a trait. The demo never constructs a live transport.
- `pnull-procurement` implements the procurement chain: the source-authority and coverage ledger, immutable source snapshots with revision/supersession and record-level diffing, bounded ingestion adapters (contract awards, solicitation mirror, OpenBook negative finding, operator-supplied import), reconciliation rules and review queue, case-file generation, gap-driven CORA drafts, and the offline procurement demo.
- `pnull-cli` composes these units without duplicating state-specific application logic.

## SQLite schema and migration

SQLite stores canonical JSON records and enforces durable uniqueness. Schema versioning uses `PRAGMA user_version` with `SCHEMA_VERSION = 2` (as of v0.0.3).

- v0.0.1 databases have no `user_version` (treated as `0`). They upgrade transactionally by adding supplemental tables without rewriting canonical records.
- v0.0.2 databases (`user_version = 1`) upgrade transactionally to v2 by adding procurement supplemental tables.
- Existing evidence IDs stay stable, content-addressed blobs are unchanged, and older records still verify.
- Migration failure rolls back cleanly; a newer unsupported schema is rejected.
- No migration reinterprets old findings as new subject/action or procurement assertions.
- The migration tests use the committed fixtures at `fixtures/migration/v0.0.1-minimal.sql` and `fixtures/migration/v0.0.2-minimal.sql`.

Tables: `evidence`, `findings`, `alerts`, `approvals`, `posts`, `post_segments`, `source_fetches` (v0.0.1), plus the v0.0.2 supplemental tables:

- `matters` — Legistar matters with official identifiers and document roles.
- `matter_attachments` — attachments discovered through documented official fields.
- `subjects` — the explicit subject of an action (e.g. Ordinance 15-84).
- `actions` — an institutional action applied to exactly one subject.
- `text_maps` — immutable per-page PDF text maps.
- `page_citations` — page-accurate citations with validated geometry.
- `review_decisions` — append-only human citation-review decisions.
- `processing_runs` — immutable processing-run provenance records.
- `source_reviews` — persistent, expiring robots/terms reviews.
- `fetch_observations` — DNS-safe HTTP provenance per request/redirect.
- `x_attempts`, `x_reconciliations` — X posting attempts and operator reconciliation.
- `publication_allowlists` — structured allowlist entries for public field categories.
- `procurement_matters`, `procurement_events`, `procurement_identifiers`, `procurement_organizations`, `procurement_money` — the v0.0.3 procurement domain model.
- `source_snapshots`, `snapshot_revisions`, `snapshot_diffs` — immutable source snapshots and revision/supersession/record-diff relationships.
- `coverage_ledger` — the persistent coverage ledger.
- `reconciliation_items`, `reconciliation_decisions` — the reconciliation-review queue and immutable decisions.
- `supplied_records` — operator-supplied public records with declared origin.
- `case_files`, `cora_drafts` — generated case files and local unsent CORA drafts.

## Procurement chain

v0.0.3 connects official procurement records only when the evidence supports the connection
("follow the money without inventing the links"). See `docs/procurement-methodology.md` for
the full model. In summary:

- Every procurement source carries an authority classification, and every acquisition
  writes a coverage-ledger entry with digest, date range, record count, completion state,
  failures, and review state. Coverage defaults to `unknown`/`partial`; absence from a
  partial source is never proof of absence.
- Fetched pages/exports/documents are immutable snapshots; changed official bytes produce a
  second snapshot linked by revision/supersession with a deterministic record-level diff.
- Money is never floating point; raw strings are preserved with distinct parsed states.
- Connections are automatic only for exact normalized identifiers, explicit official
  relationships, or existing evidence-backed relationships; everything else enters the
  reconciliation-review queue and requires an immutable human decision.
- Case files (JSON + Markdown) stay drafts until the citation-review and publication
  allowlists pass. CORA drafts are local and unsent.
- Live retrieval still requires an explicit live mode and an approved persistent source
  review; default demonstrations are fully offline.

## Processing-run provenance

Every ingestion or reprocessing job records an immutable `ProcessingRun`:

- schema version and Panopticon Null version;
- source revision, rules digest, and state-config digest;
- input evidence IDs;
- native tool names and versions (Poppler, Tesseract, bubblewrap, prlimit);
- sandbox backend and version;
- configured budgets and actual aggregate resource consumption;
- start/end timestamps, outcome, and structured errors;
- output artifact IDs and digests.

Processing runs are supplemental records linked to existing evidence IDs; they never rewrite v0.0.1 records. Offline tests inject clocks and build metadata for reproducibility.

## Text maps, page citations, and the coordinate system

`pnull-geometry` extracts an immutable text map for each PDF page using Poppler's `pdftotext -bbox-layout`. Each map records the evidence ID, page number, page width/height, rotation, the coordinate system (`pdf_user_space_points_bottom_left_y_up`), extracted words with bounding boxes, the extractor and its version, a text-map digest, and its relation to the source digest.

Bounding rectangles are validated: negative coordinates, inverted rectangles, out-of-bounds geometry, missing pages, and quote mismatches are rejected.

A page citation binds a quote to exact geometry (page number and bounding rectangles), a normalized character range, the text-map digest, the evidence digest, and optional OCR confidence. OCR uses deterministic Tesseract TSV with a pixel-to-page transform; OCR confidence is metadata, never proof.

The CLI renders a highlighted image of the quoted region (`pnull citation render <id> --output review.png`).

## Bubblewrap sandbox

Live PDF/OCR ingestion runs in a Linux bubblewrap sandbox:

- no network namespace access;
- no inherited secrets;
- no writable access outside a dedicated temporary output directory;
- read-only exact inputs;
- new process and session boundaries;
- CPU, memory, file-size, output-size, and wall-time limits via `prlimit`;
- cleanup on success, failure, timeout, or interrupt.

Live ingestion fails closed when the sandbox cannot be established. Aggregate job budgets bound total downloaded bytes, attachments, PDF pages, OCR pages, extracted bytes, child processes, CPU allowance, and wall-clock allowance. Tests prove sandboxed tools cannot reach the network, read unrelated files, write outside the output directory, spawn unbounded process trees, or exceed aggregate budgets.

## DNS-safe HTTP layer

`pnull-http` persists provenance for every request and redirect: requested URL, resolved public IPs, retrieval timestamp, method, status code, redirect target, final URL, allowlisted headers, content type, content length, ETag, Last-Modified, response-body digest, and structured errors. Cookies, authorization headers, and bearer tokens are never persisted.

- Rejects loopback, private, link-local, multicast, unspecified, documentation, and non-public addresses.
- Fails closed on mixed public + prohibited DNS answers.
- Requires HTTPS; certificate validation cannot be disabled.
- Conditional requests use `If-None-Match` / `If-Modified-Since`; a 304 creates a fetch observation referencing previous preserved evidence, never a new blob.
- Resolver and transport are abstractions so CI tests stay offline.

## Review queue

Human citation review is an append-only state machine: Pending, Approved, Rejected, NeedsContext, Superseded. Each decision binds to the exact digests of the evidence ID, source digest, locator/geometry, quote, quote digest, rule digest, processing artifact digest, and proposed public fields. Changing any bound value invalidates approval. Structured publication allowlists state which field categories may appear publicly; an allowlist is not auto-approval.

The site, Atom feed, and X pipeline fail closed on pending, rejected, stale, or mismatched decisions. The demo uses clearly labeled deterministic demonstration reviews.

## X reconciliation

X thread safety is built around append-only attempts and operator decisions. `pnull x attempts`, `pnull x status`, and `pnull x reconcile` operate with a dry-run default. A new attempt is authorized only after the previous attempt is resolved; uncertain attempts are never blindly retried. No audit history is deleted, and no test, fixture, or demo constructs a live X transport.

## Determinism

Fixture retrieval timestamps are fixed in Colorado configuration. Evidence identifiers depend on jurisdiction, source URL, and original-byte digest. Finding, alert, subject, and action identifiers additionally include their governing inputs and digests. Site generation has no current-time input. Tests compare canonical records and all site bytes across two clean output directories.

Presentation metadata that is inherently live, such as an operator's approval time, is stored in SQLite and excluded from canonical evidence JSON.

## Failure model

An extraction failure creates a preserved evidence record with a structured error and empty extracted text. Batch-oriented library callers can continue. The live CLI reports that extraction as failure rather than claiming success. Static publication writes to a sibling temporary directory and leaves the previous site untouched if validation fails.

An X attempt is reserved before network access. Each successful thread segment is persisted immediately. An interrupted attempt is fail-closed: it cannot be blindly replayed without operator reconciliation.
