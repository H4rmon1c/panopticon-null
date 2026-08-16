# Migration v0.0.1 → v0.0.2

## Schema versioning

SQLite schema versioning uses `PRAGMA user_version`. `SCHEMA_VERSION = 1` is the current target; `MAX_SUPPORTED_SCHEMA_VERSION = 1` is the highest this build understands.

- A v0.0.1 database has no `user_version` set, which `PRAGMA user_version` reports as `0`.
- A fresh database is created directly at the target schema (`1`); migration is a no-op for it.
- A database with `user_version` greater than the supported maximum is rejected with `UnsupportedVersion` and is never touched.

## What the migration does

For an existing v0.0.1 database (version `0`), the migration runs inside a single SQLite transaction and only adds supplemental tables and indexes:

- `matters`, `matter_attachments`, `subjects`, `actions`
- `text_maps`, `page_citations`
- `review_decisions`
- `processing_runs`
- `source_reviews`
- `fetch_observations`
- `x_attempts`, `x_reconciliations`
- `publication_allowlists`
- supporting indexes on `matters(source_id)`, `actions(matter_id)`, `review_decisions(citation_id)`, `source_reviews(source_id)`, `fetch_observations(source_id)`, and `x_attempts(alert_id)`

It never rewrites canonical v0.0.1 records. Evidence, findings, alerts, approvals, posts, post segments, and source-fetch history are preserved byte-for-byte. No migration reinterprets old findings as new subject/action assertions; the new subject/action tables are populated only by new v0.0.2 processing.

## What is preserved

- Existing evidence IDs stay stable.
- Content-addressed blobs (`evidence/sha256/<prefix>/<digest>`) are unchanged.
- v0.0.1 records continue to verify against their stored digests.
- The migration test proves every canonical record survives byte-for-byte and that the original content digest and evidence ID remain stable.

## Committed fixture

The migration test loads the committed fixture at `fixtures/migration/v0.0.1-minimal.sql`, a minimal v0.0.1 database containing one evidence record, one finding, one alert, one approval, one post with two segments, and one source fetch. It verifies that after migration:

- the database is at the target schema version;
- every evidence row is unchanged byte-for-byte;
- findings, alerts, approvals, posts, post segments, and source fetches are all preserved;
- the original content digest and evidence ID are unchanged.

## Rollback and rejection behavior

- Migration failure rolls back cleanly: the transaction is not committed, so the database is left untouched.
- A newer unsupported schema is rejected rather than migrated; nothing is modified.
- Migration is idempotent: running it again on an already-current database is a no-op.

## Impact on operators

Upgrading from v0.0.1 is automatic and transactional when the store opens. No data export/import is required, and no v0.0.1 record is rewritten or reinterpreted. This matches the project's evidence-integrity guarantee: preserved bytes and their digests are never altered by a schema upgrade.
