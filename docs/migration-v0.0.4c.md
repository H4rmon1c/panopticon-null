# Migration v0.0.4 → v0.0.4c

## Schema versioning

SQLite schema versioning uses `PRAGMA user_version`. `SCHEMA_VERSION = 4` is the current
target; `MAX_SUPPORTED_SCHEMA_VERSION = 4` is the highest this build understands.

- A fresh database is created directly at the target schema (`4`); migration is a no-op.
- An existing v0.0.4 database (`user_version = 3`) upgrades through the additive path
  below.
- A database with `user_version` greater than the supported maximum is rejected with
  `UnsupportedVersion` and is never touched.

## What the migration does

For an existing v0.0.4 database (`user_version = 3`), the migration runs inside a single
SQLite transaction and only adds two snapshot-row storage tables:

- `snapshot_rows` — every immutable procurement snapshot's exact parsed row set,
  persisted as `(snapshot_id, seq, row_key, canonical, row_digest, raw_json)`. Each row is
  bound to its snapshot via a foreign key to `source_snapshots(id)`, and carries a
  deterministic per-row digest over the normalized canonical fields.
- `snapshot_row_sets` — completion metadata per snapshot (`expected_count`,
  `row_set_digest`, `parser_version`, `schema_version`) that distinguishes a valid
  zero-row capture from a legacy snapshot whose rows were never preserved.

It never rewrites canonical v0.0.1–v0.0.4 records. Every row of every pre-existing table —
including the v0.0.4 public-ledger tables (`procurement_alerts`, `cora_requests`,
`official_relationships`, `supplied_records`) — survives byte-for-byte. The migration is
additive only.

## Coverage and evidence limitation

The migration does **not** fabricate rows for snapshots that were ingested before v0.0.4c
and whose rows were never persisted. Those legacy snapshots load with a documented
evidence limitation: change detection between a legacy snapshot and a new one degrades to
*no diff reported* rather than reconstructing history from fixtures or files on disk.
This is deliberate — the integrity guarantee is that change detection compares only exact
immutable data stored for the exact previous snapshot, never a reconstruction. The empty
`snapshot_rows`/`snapshot_row_sets` for legacy snapshots are the stored record of that
limitation.

## What is preserved

- Existing evidence IDs stay stable.
- Content-addressed blobs (`evidence/sha256/<prefix>/<digest>`) are unchanged.
- All 0.0.1, 0.0.2, 0.0.3, and 0.0.4 records continue to verify against their stored
  digests.
- The migration test proves every canonical record survives byte-for-byte, including the
  v0.0.4 public-ledger tables.

## Committed fixtures

The migration test loads the committed fixture:

- `fixtures/migration/v0.0.4-minimal.sql` — a real 0.0.4 database (schema version 3) with
  all prior tables plus the v0.0.4 public-ledger tables and representative rows.

The v0.0.4 fixture is used to prove the upgrade preserves every canonical row and reaches
schema v4.

## Rollback and rejection behavior

- Migration failure rolls back atomically: the transaction is not committed, so the
  database is left untouched. A failure-injection test proves this (sabotaging the v4
  index creation leaves `user_version = 3` with no partial snapshot-row tables).
- A newer unsupported schema is rejected rather than migrated; nothing is modified.
- Migration is idempotent: running it again on an already-current database is a no-op.

## Impact on operators

Upgrading from v0.0.4 is automatic and transactional when the store opens. No data
export/import is required, and no prior record is rewritten or reinterpreted. After the
upgrade, new snapshots persist their exact rows, so change detection can compare the exact
previous snapshot's rows from the database. Legacy snapshots (pre-v0.0.4c) retain the
documented coverage limitation above until their rows are next captured.
