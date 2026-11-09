# Implementation Plan — Phase 1: Make Main Trustworthy (0.0.4c)

Branch: `fix/0.0.4c-snapshot-row-persistence`
Base: main `d31e51a` (Panopticon Null 0.0.4)

## 1. Diagnosed defects on current main

1. **Filesystem-derived history (`prior_rows_from_disk`) in `crates/pnull-cli/src/refresh.rs`.**
   `refresh_live_with` reconstructs the previous snapshot's award rows by re-reading
   the fixture HTML file from `fixtures/procurement/<name>` on disk
   (`prior_rows_from_disk`, lines 315+). This means:
   - Change detection depends on a mutable file currently present on disk.
   - If the fixture is deleted or a different file is present, comparison breaks or
     silently compares against the wrong data.
   - It does not survive restarts that move/remove fixtures, and it cannot reproduce
     the exact prior snapshot after re-ingestion.
   This is the primary goal's core defect.

2. **`record_snapshot` never persists the parsed rows.** It computes a diff from the
   two caller-supplied row sets and stores only the `SnapshotDiff`, but the rows
   themselves are never persisted per-snapshot. So there is no durable record of
   *what* each snapshot contained — only the derived diff.

3. **Offline ingestion paths pass empty rows to `record_snapshot`.**
   `ingest_awards`/`ingest_solicitations` (procurement_cmd.rs) and the first
   ingestion in `demo.rs` pass `&[]`/`&[]` for old/new rows, so the snapshot-level
   diff is empty and no row history is recorded even at the diff level.

4. **`coverage_diff` synthesizes placeholder rows** from record count + digest rather
   than reading actual stored rows.

5. **The `record_diff` single-key map drops duplicate stable row keys.**
   A joint award can produce two rows sharing one solicitation id; main's
   `BTreeMap<key, row>` collapses them. The old `fix/0.0.4b` branch fixes this with a
   multiset comparison.

6. **No migration fixture / test for the current pre-change 0.0.4 (v3) schema**, and
   no snapshot-row tables exist at all.

## 2. Design

Persist the exact parsed row set belonging to every immutable snapshot, bound to:
snapshot id, source id, stable row key, parser/schema version, normalized identity
fields, raw original values, and a deterministic per-row digest. Change detection
reads the exact previous snapshot's stored rows from the database — never a fixture.

### New core types (`pnull-core/src/procurement.rs`)
- `SnapshotRow { key, canonical, row_digest, raw_json }` — a stored row bound to a
  snapshot, carrying the canonical form used for identity/equality, a deterministic
  digest of that canonical form, and the raw original values as JSON for evidence
  and field-level diffs.
- `SnapshotRowSet { snapshot_id, expected_count, row_set_digest, parser_version,
  schema_version }` — completion metadata so an empty-but-valid capture is
  distinguishable from a legacy/incomplete one.
- `fn row_set_digest(&[SnapshotRow]) -> String` — deterministic, order-independent,
  duplicate-preserving digest over key+canonical.

### New schema (migration `v4`), transactional and atomic
- `snapshot_rows(snapshot_id, seq, row_key, canonical, row_digest, raw_json)` with
  `PRIMARY KEY (snapshot_id, seq)` and an index on `(snapshot_id, row_key)`. `seq`
  preserves duplicate row keys.
- `snapshot_row_sets(snapshot_id PRIMARY KEY, expected_count, row_set_digest,
  parser_version, schema_version)`.
- Migration runs inside the existing single transaction (as `apply_v1..v3` do),
  increments `user_version` to `4`, and rolls back entirely on failure. It never
  fabricates rows; legacy snapshots simply have no row metadata and are represented
  as a coverage/evidence limitation (`LegacyOrIncomplete`).
- Preserves all existing v1..v3 rows byte-for-byte.

### Store methods (`pnull-core/src/lib.rs`)
- `insert_snapshot_row_set_with_rows(meta, rows)` — transactional: writes rows, then
  the completion marker last; idempotent identical retry; any conflicting retry
  returns a `CoreError::SnapshotRowSetConflict` without overwriting.
- `snapshot_row_set(snapshot_id) -> Option<SnapshotRowSet>`.
- `snapshot_rows(snapshot_id) -> Vec<SnapshotRow>` (ordered by seq).
- `snapshot_rows_for_source_snapshot(...)` or reuse `source_snapshot(id)` +
  `snapshot_rows(id)`.

### `pnull-procurement/src/snapshot.rs`
- `record_snapshot` persists the new snapshot's rows (via
  `insert_snapshot_row_set_with_rows`) when it inserts a genuinely new snapshot, and
  computes the diff from the **exact stored prior rows** (loaded from the DB) when a
  prior snapshot exists, rather than trusting caller-passed `old_rows`. Keep the
  existing `(old_rows, new_rows)` signature for compatibility but make the persisted
  prior rows authoritative.
- `snapshot_rows(store, snapshot_id) -> Result<Vec<RecordRow>, SnapshotError>` — the
  metadata-verified loader returning `LegacyOrIncomplete` when no row metadata
  exists and `RowSetIntegrity` on count/digest mismatch. Fail closed.
- Upgrade `record_diff` to multiset semantics (port from 0.0.4b) so duplicate stable
  row keys are handled correctly.

### `pnull-cli/src/refresh.rs`
- Delete `prior_rows_from_disk` and its filesystem reads.
- In `refresh_live_with`, load the exact prior rows from the previous snapshot's
  stored rows; pass the new parsed rows so `record_snapshot` persists them and
  computes the diff; build change alerts from stored prior rows vs new rows.
- 304 path unchanged. Idempotency unchanged.

### `pnull-cli/src/procurement_cmd.rs` and `pnull-procurement/src/demo.rs`
- `ingest_awards`/`ingest_solicitations` and the demo's first ingest pass the real
  parsed rows (new_rows = the parsed set, old_rows = [] when first).
- `coverage_diff` reads stored rows via `snapshot_rows` instead of synthesizing
  placeholders; surfaces `LegacyOrIncomplete` honestly.

## 3. CI repair

Root cause: `nix flake check`'s `buildRustPackage` computes `cargoDeps` with
`import-cargo-lock`, which downloads each crate `.crate` file from the crates.io CDN.
The upstream CDN intermittently returns HTTP 403 for some crates (e.g. zerovec 0.11.7).
This is a transient upstream failure, not a source-code failure.

Fix: pin and retry the crates.io source. Concretely, wrap the cargo invocation used by
`buildRustPackage` with a bounded retry so a transient CDN 403 is retried before the
build is declared failed, and keep `Cargo.lock` / flake.lock fully pinned. A source-code
failure will still fail (retry only retries download/transport failures), so a transient
upstream failure remains distinguishable. (Determined during implementation; see section
9 verification.)

## 4. Tests to add (see Required tests)
Coverage for: fresh DB, v0.0.3→v4 migration, pre-change 0.0.4(v3)→v4 migration,
byte-for-byte preservation, snapshot A, restart, snapshot B, fixture deletion,
added/modified(field diffs)/removed, idempotent re-ingest, reordering stability,
duplicate keys, parser-version changes, malformed rows, missing legacy data, corrupt
digests, transaction rollback, alert binding after snapshot C, deterministic demo.

## 5. Documentation to update
README.md, CHANGELOG.md, docs/architecture.md, docs/operator-guide-procurement.md,
docs/procurement-methodology.md, docs/roadmap.md, docs/validation-0.0.4.md, and
migration documentation.

## 6. Verification
- nix flake check
- cargo fmt --check
- cargo clippy --workspace --all-features --locked -- -D warnings
- cargo test --workspace --all-features --locked
- real Bubblewrap + Poppler extraction integration test
- dependency-policy checks
- offline demo twice, compare outputs for determinism
- restart test: ingest A, exit, remove fixture, ingest B, compare using DB only
