# Architecture

## Vertical slice

```text
official HTTPS source / committed fixture
  → bounded original-byte ingestion
  → SHA-256 content store + canonical JSON + SQLite
  → static extraction (HTML/text/JSON/PDF/OCR)
  → deterministic YAML scan + exact citations
  → cautious state classification + version diff
  → privacy gate
  → static HTML + Atom
  → local X draft → exact-digest approval → optional confirmed transport
```

All durable state is local. No hosted service, JavaScript runtime, telemetry, analytics, advertising, or tracking is required.

## Crates

- `pnull-core` owns the evidence, citation, finding, diff, and alert schemas. IDs are domain-separated SHA-256 values. SQLite enforces durable uniqueness. Original bytes are stored under `evidence/sha256/<prefix>/<digest>`; canonical records are deterministic JSON.
- `pnull-ingest` validates metadata, restricts live retrieval to same-host public HTTPS redirects, enforces input limits, and extracts hostile content. It never executes scripts, macros, document attachments, or source content. PDF and OCR tools are allowlisted subprocesses with address-space, CPU, file, process, page, image, output, and wall-time bounds.
- `pnull-detect` parses the reviewed YAML taxonomy. It records exact lines, limits strong states to cited context, rejects common negation/conditional forms, resolves conflicting states to `Unknown`, and records rule version and digest.
- `pnull-publish` validates all internal evidence references, applies publication gates, writes a complete temporary tree, then atomically replaces the previous site. Core reading and navigation require no JavaScript.
- `pnull-x` creates one post or a short thread, applies the same publication gate, binds approval to a canonical draft digest, and hides the network transport behind a trait.
- `pnull-cli` composes these units without duplicating state-specific application logic.

## Determinism

Fixture retrieval timestamps are fixed in Colorado configuration. Evidence identifiers depend on jurisdiction, source URL, and original-byte digest. Finding and alert identifiers additionally include state, matched rules, and rule digest. Site generation has no current-time input. Tests compare canonical records and all site bytes across two clean output directories.

Presentation metadata that is inherently live, such as an operator's approval time, is stored in SQLite and excluded from canonical evidence JSON.

## Failure model

An extraction failure creates a preserved evidence record with a structured error and empty extracted text. Batch-oriented library callers can continue. The live CLI reports that extraction as failure rather than claiming success. Static publication writes to a sibling temporary directory and leaves the previous site untouched if validation fails.

An X attempt is reserved before network access. Each successful thread segment is persisted immediately. An interrupted attempt is fail-closed: it cannot be blindly replayed.
