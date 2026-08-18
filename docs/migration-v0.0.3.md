# Migration v0.0.2 → v0.0.3

## Schema versioning

SQLite schema versioning uses `PRAGMA user_version`. `SCHEMA_VERSION = 2` is the current
target; `MAX_SUPPORTED_SCHEMA_VERSION = 2` is the highest this build understands.

- A fresh database is created directly at the target schema (`2`); migration is a no-op.
- A v0.0.1 database (`user_version = 0`) upgrades through the same supplemental-table path
  as 0.0.2, then to v2.
- A database with `user_version` greater than the supported maximum is rejected with
  `UnsupportedVersion` and is never touched.

## What the migration does

For an existing v0.0.2 database (`user_version = 1`), the migration runs inside a single
SQLite transaction and only adds procurement supplemental tables:

- `procurement_matters` — procurement matters with title, identifiers, and state.
- `procurement_events` — chronological procurement events (solicitation published,
  amendment, award announced, contract executed, expenditure reported, and so on).
- `procurement_identifiers` — raw identifiers and their source.
- `procurement_organizations` — organizations in their documented roles.
- `procurement_money` — raw and parsed money values.
- `source_snapshots` — immutable source snapshots with persisted-byte digests.
- `snapshot_revisions` — revision/supersession links between snapshots.
- `snapshot_diffs` — deterministic record-level diffs.
- `coverage_ledger` — the persistent coverage ledger.
- `reconciliation_items` — the reconciliation-review queue.
- `reconciliation_decisions` — immutable, auditable accept/reject decisions.
- `supplied_records` — operator-supplied public records with declared origin.
- `case_files` — generated case files.
- `cora_drafts` — local, unsent CORA drafts.
- supporting indexes on matter, source, snapshot, and coverage identifiers.

It never rewrites canonical v0.0.1 or v0.0.2 records. Evidence, findings, alerts,
approvals, posts, post segments, source-fetch history, matters, subjects, actions, text
maps, page citations, review decisions, processing runs, source reviews, fetch
observations, X attempts/reconciliations, and publication allowlists are preserved
byte-for-byte. No migration reinterprets old findings as new procurement assertions; the
new procurement tables are populated only by new v0.0.3 processing.

## What is preserved

- Existing evidence IDs stay stable.
- Content-addressed blobs (`evidence/sha256/<prefix>/<digest>`) are unchanged.
- All 0.0.1 and 0.0.2 records continue to verify against their stored digests.
- The migration test proves every canonical record survives byte-for-byte.

## Committed fixtures

The migration test loads the committed fixtures:

- `fixtures/migration/v0.0.1-minimal.sql` — a minimal v0.0.1 database.
- `fixtures/migration/v0.0.2-minimal.sql` — a real 0.0.2 database.

The v0.0.2 fixture is used to prove the upgrade from a real 0.0.2 database preserves every
canonical row and reaches schema v2.

## Rollback and rejection behavior

- Migration failure rolls back atomically: the transaction is not committed, so the
  database is left untouched. A failure-injection test proves this.
- A newer unsupported schema is rejected rather than migrated; nothing is modified.
- Migration is idempotent: running it again on an already-current database is a no-op.

## Impact on operators

Upgrading from v0.0.2 is automatic and transactional when the store opens. No data
export/import is required, and no v0.0.1 or v0.0.2 record is rewritten or reinterpreted.
This matches the project's evidence-integrity guarantee: preserved bytes and their digests
are never altered by a schema upgrade.
