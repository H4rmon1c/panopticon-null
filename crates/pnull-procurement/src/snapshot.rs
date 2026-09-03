//! Immutable source snapshots, revision/supersession links, and deterministic
//! record-level change detection.
//!
//! Every fetched page, export, and document becomes an immutable snapshot keyed
//! by the SHA-256 of its exact persisted bytes. If an official URL later serves
//! different bytes, both the old and new snapshots are preserved and linked
//! through a revision relationship; a deterministic record-level diff is
//! produced. Old artifacts and their derived observations are never rewritten.

use pnull_core::{
    CoverageEntry, CoverageState, FetchObservation, RecordChange, SnapshotDiff, SnapshotRevision,
    SnapshotRow, SnapshotRowSet, SourceAuthority, SourceSnapshot, row_set_digest, sha256_hex,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SnapshotError {
    #[error("snapshot with id {0} does not exist")]
    NotFound(String),
    #[error("snapshots are from different sources: {old} vs {new}")]
    DifferentSources { old: String, new: String },
    #[error(
        "snapshot {0} has no stored rows: its rows were never preserved (legacy or incomplete capture)"
    )]
    LegacyOrIncomplete(String),
    #[error("stored snapshot rows for {0} violate integrity: {1}")]
    RowSetIntegrity(String, String),
    #[error("store operation failed: {0}")]
    Store(#[from] pnull_core::CoreError),
}

/// Metadata about an acquisition, distinct from the artifact bytes.
#[derive(Clone, Debug)]
pub struct Acquisition {
    pub source_id: String,
    pub source_url: String,
    pub retrieved_at: String,
    pub bytes_digest: String,
    pub content_type: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub final_url: String,
    pub redirect_history: Vec<String>,
    pub parser_version: String,
    pub schema_version: u32,
    pub authority: SourceAuthority,
    pub coverage_state: CoverageState,
    pub observations: Vec<FetchObservation>,
}

impl Acquisition {
    /// Builds a coverage-ledger entry from this acquisition.
    pub fn coverage_entry(&self, record_count: Option<u64>, note: &str) -> CoverageEntry {
        CoverageEntry {
            id: CoverageEntry::id_for(&self.source_id, &self.retrieved_at),
            source_id: self.source_id.clone(),
            source_url: self.source_url.clone(),
            authority: self.authority,
            state: self.coverage_state,
            retrieved_at: self.retrieved_at.clone(),
            persisted_digest: Some(self.bytes_digest.clone()),
            http_status: None,
            etag: self.etag.clone(),
            last_modified: self.last_modified.clone(),
            final_url: Some(self.final_url.clone()),
            parser_version: Some(self.parser_version.clone()),
            schema_version: Some(self.schema_version),
            claimed_date_range: None,
            record_count,
            pagination_complete: None,
            access_errors: Vec::new(),
            human_review_state: "unreviewed".to_owned(),
            note: note.to_owned(),
        }
    }

    /// A source snapshot referencing the exact persisted bytes.
    pub fn snapshot(
        &self,
        record_count: Option<u64>,
        supersedes: Option<String>,
    ) -> SourceSnapshot {
        SourceSnapshot {
            id: SourceSnapshot::id_for(&self.source_id, &self.bytes_digest),
            source_id: self.source_id.clone(),
            source_url: self.source_url.clone(),
            retrieved_at: self.retrieved_at.clone(),
            persisted_digest: self.bytes_digest.clone(),
            content_type: self.content_type.clone(),
            etag: self.etag.clone(),
            last_modified: self.last_modified.clone(),
            final_url: self.final_url.clone(),
            redirect_history: self.redirect_history.clone(),
            parser_version: self.parser_version.clone(),
            schema_version: self.schema_version,
            record_count,
            pagination_complete: None,
            coverage_state: self.coverage_state,
            supersedes,
        }
    }
}

/// A row-level record keyed by a deterministic identifier for diffing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordRow {
    /// Deterministic key (e.g., a normalized identifier or a stable row hash).
    pub key: String,
    /// A canonical text form of the row used for the diff.
    pub canonical: String,
    /// The raw original values of the row as JSON, retained for evidence and
    /// field-level diffs. Empty when the row has no raw-value source.
    pub raw_json: String,
}

impl RecordRow {
    /// Converts this comparison row into a persisted snapshot row, carrying the
    /// canonical form, its deterministic digest, and the raw values (when the
    /// row was produced from parsed original values) for evidence and diffs.
    pub fn to_snapshot_row(&self) -> SnapshotRow {
        SnapshotRow {
            key: self.key.clone(),
            canonical: self.canonical.clone(),
            row_digest: sha256_hex(self.canonical.as_bytes()),
            raw_json: self.raw_json.clone(),
        }
    }
}

/// Produces a deterministic record-level diff between two snapshots' rows.
///
/// Rows are matched by `key`. A row present only in the old snapshot is a
/// removal; only in the new snapshot is an addition; present in both with a
/// different `canonical` is a change. Ordering is ignored. Because a source can
/// legitimately contain two rows sharing one stable key (e.g. a joint award
/// with separate contractor lines), rows are compared as a *multiset*: equal
/// key+canonical pairs cancel, and the surplus determines added/changed/removed.
fn group_by_key(rows: &[RecordRow]) -> std::collections::BTreeMap<&str, Vec<&str>> {
    let mut map: std::collections::BTreeMap<&str, Vec<&str>> = std::collections::BTreeMap::new();
    for row in rows {
        map.entry(row.key.as_str())
            .or_default()
            .push(row.canonical.as_str());
    }
    for values in map.values_mut() {
        values.sort_unstable();
    }
    map
}

/// Multiset difference `a - b`: values in `a` not matched by a value in `b`.
fn multiset_difference<'a>(a: &[&'a str], b: &[&'a str]) -> Vec<&'a str> {
    let mut b_counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for &value in b {
        *b_counts.entry(value).or_insert(0) += 1;
    }
    let mut used: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    let mut result = Vec::new();
    for &value in a {
        let matched = used.entry(value).or_insert(0);
        let available = b_counts.get(value).copied().unwrap_or(0);
        if *matched < available {
            *matched += 1;
        } else {
            result.push(value);
        }
    }
    result
}

pub fn record_diff(
    old_snapshot_id: &str,
    new_snapshot_id: &str,
    source_id: &str,
    old_rows: &[RecordRow],
    new_rows: &[RecordRow],
) -> SnapshotDiff {
    let old_map = group_by_key(old_rows);
    let new_map = group_by_key(new_rows);
    let mut changes = Vec::new();

    for (key, old_values) in &old_map {
        let new_values = new_map.get(key).map_or(&[] as &[&str], Vec::as_slice);
        // Identical (key, canonical) pairs cancel; the surplus is the change.
        let removed = multiset_difference(old_values, new_values);
        let added = multiset_difference(new_values, old_values);
        let overlap = removed.len().min(added.len());
        for _ in 0..overlap {
            changes.push(RecordChange {
                kind: "changed".to_owned(),
                row_key: (*key).to_owned(),
                summary: format!("record {key} changed between snapshots"),
            });
        }
        for _ in overlap..removed.len() {
            changes.push(RecordChange {
                kind: "removed".to_owned(),
                row_key: (*key).to_owned(),
                summary: format!("record {key} disappeared from the later snapshot"),
            });
        }
        for _ in overlap..added.len() {
            changes.push(RecordChange {
                kind: "added".to_owned(),
                row_key: (*key).to_owned(),
                summary: format!("record {key} appeared in the later snapshot"),
            });
        }
    }
    // Keys present only in the new snapshot are additions.
    for key in new_map.keys() {
        if !old_map.contains_key(key) {
            for _ in &new_map[key] {
                changes.push(RecordChange {
                    kind: "added".to_owned(),
                    row_key: (*key).to_owned(),
                    summary: format!("record {key} appeared in the later snapshot"),
                });
            }
        }
    }
    changes.sort_by(|a, b| a.row_key.cmp(&b.row_key).then_with(|| a.kind.cmp(&b.kind)));
    SnapshotDiff {
        id: SnapshotDiff::id_for(old_snapshot_id, new_snapshot_id),
        old_snapshot_id: old_snapshot_id.to_owned(),
        new_snapshot_id: new_snapshot_id.to_owned(),
        source_id: source_id.to_owned(),
        changes,
        produced_at: "deterministic".to_owned(),
    }
}

/// Records a snapshot, its revision link to a prior snapshot, its stored rows,
/// and coverage entry.
///
/// `prior_snapshot` is the most recent snapshot of the same source, if any.
/// `new_rows` are the rows parsed from the snapshot being recorded; they are
/// persisted verbatim, bound to the snapshot, so the exact prior row set is
/// always reproducible from the database without reading a fixture or file.
/// The record-level diff against the prior snapshot is computed from the
/// **prior snapshot's own stored rows** (loaded from the database), never from
/// a caller-supplied `old_rows` argument or any file on disk. When the prior
/// snapshot's rows were never preserved (a legacy capture), no diff is
/// fabricated. The old snapshot is never modified; only a new revision link and
/// the new rows are added.
pub fn record_snapshot(
    store: &pnull_core::Store,
    acquisition: &Acquisition,
    prior_snapshot: Option<&SourceSnapshot>,
    record_count: Option<u64>,
    old_rows: &[RecordRow],
    new_rows: &[RecordRow],
) -> Result<(SourceSnapshot, Option<SnapshotDiff>), SnapshotError> {
    let snapshot = acquisition.snapshot(record_count, prior_snapshot.map(|s| s.id.clone()));

    // Insert the immutable snapshot first so the row set can reference it.
    // This is idempotent: re-inserting the same snapshot id inserts nothing.
    let inserted = store.insert_source_snapshot(&snapshot)?;
    if inserted && snapshot.supersedes.is_some() {
        let revision = SnapshotRevision {
            id: SnapshotRevision::id_for(&snapshot.id, &acquisition.retrieved_at),
            snapshot_id: snapshot.id.clone(),
            supersedes: snapshot.supersedes.clone(),
            superseded_by: None,
            reason: "new bytes observed for the same official URL".to_owned(),
            recorded_at: acquisition.retrieved_at.clone(),
        };
        store.insert_snapshot_revision(&revision)?;
    }

    // Persist the exact parsed row set, bound to the snapshot. A conflicting
    // re-persist (identical snapshot id, different rows) fails closed rather
    // than overwriting historical rows.
    let persisted_rows: Vec<SnapshotRow> =
        new_rows.iter().map(RecordRow::to_snapshot_row).collect();
    let meta = SnapshotRowSet {
        snapshot_id: snapshot.id.clone(),
        expected_count: new_rows.len() as u64,
        row_set_digest: row_set_digest(&persisted_rows),
        parser_version: acquisition.parser_version.clone(),
        schema_version: acquisition.schema_version,
    };
    store.insert_snapshot_row_set_with_rows(&meta, &persisted_rows)?;

    let coverage = acquisition.coverage_entry(record_count, "snapshot captured");
    store.insert_coverage_entry(&coverage)?;

    // The diff is computed from the exact prior snapshot's *stored* rows, never
    // from the caller-supplied `old_rows` or any file on disk. `old_rows` is
    // kept for backward compatibility but is not authoritative.
    let _ = old_rows;
    let diff = if let Some(prior) = prior_snapshot {
        if prior.persisted_digest == snapshot.persisted_digest {
            None
        } else {
            // Load the prior snapshot's stored rows with integrity verification.
            // A legacy capture with no preserved rows yields no diff (no
            // fabricated history); a corrupt capture fails closed.
            match snapshot_rows(store, &prior.id) {
                Ok(prior_rows) if prior_rows.is_empty() => None,
                Ok(prior_rows) => Some(record_diff(
                    &prior.id,
                    &snapshot.id,
                    &snapshot.source_id,
                    &prior_rows,
                    new_rows,
                )),
                // Rows were never preserved (legacy capture): degrade to no
                // diff rather than fabricating history.
                Err(SnapshotError::LegacyOrIncomplete(_)) => None,
                // Corrupt or ambiguous stored evidence: fail closed.
                Err(e) => return Err(e),
            }
        }
    } else {
        None
    };
    if let Some(diff) = &diff {
        store.insert_snapshot_diff(diff)?;
    }
    Ok((snapshot, diff))
}

/// Handles a 304 Not Modified response: records an acquisition/provenance event
/// without duplicating the artifact.
///
/// Returns `true` when the artifact is unchanged (a 304), so callers can avoid
/// re-parsing and re-persisting the bytes.
pub fn record_unchanged(
    store: &pnull_core::Store,
    acquisition: &Acquisition,
    existing_snapshot: &SourceSnapshot,
) -> Result<bool, SnapshotError> {
    // A 304 means the previously persisted bytes remain authoritative.
    let entry = CoverageEntry {
        id: CoverageEntry::id_for(&acquisition.source_id, &acquisition.retrieved_at),
        source_id: acquisition.source_id.clone(),
        source_url: acquisition.source_url.clone(),
        authority: acquisition.authority,
        state: CoverageState::Complete,
        retrieved_at: acquisition.retrieved_at.clone(),
        persisted_digest: Some(existing_snapshot.persisted_digest.clone()),
        http_status: Some(304),
        etag: acquisition.etag.clone(),
        last_modified: acquisition.last_modified.clone(),
        final_url: Some(acquisition.final_url.clone()),
        parser_version: Some(acquisition.parser_version.clone()),
        schema_version: Some(acquisition.schema_version),
        claimed_date_range: None,
        record_count: existing_snapshot.record_count,
        pagination_complete: None,
        access_errors: Vec::new(),
        human_review_state: "unreviewed".to_owned(),
        note: "304 Not Modified: artifact unchanged; no duplicate snapshot created".to_owned(),
    };
    store.insert_coverage_entry(&entry)?;
    Ok(true)
}

/// Reads the most recent snapshot for a source, if any.
pub fn latest_snapshot(
    store: &pnull_core::Store,
    source_id: &str,
) -> Result<Option<SourceSnapshot>, SnapshotError> {
    Ok(store.source_snapshots(source_id)?.into_iter().last())
}

/// Loads the exact stored rows of a snapshot with integrity verification
/// (v0.0.4c).
///
/// Fails closed when the stored evidence is missing, corrupt, or ambiguous:
/// - No completion metadata -> `LegacyOrIncomplete` (rows were never preserved).
/// - Declared count disagrees with the number of stored rows -> `RowSetIntegrity`.
/// - Row-set digest disagrees with completion metadata -> `RowSetIntegrity`.
/// - A stored row's per-row digest disagrees with its canonical form ->
///   `RowSetIntegrity`.
///
/// A valid capture that genuinely contained zero rows returns an empty vec.
pub fn snapshot_rows(
    store: &pnull_core::Store,
    snapshot_id: &str,
) -> Result<Vec<RecordRow>, SnapshotError> {
    let meta = store
        .snapshot_row_set(snapshot_id)?
        .ok_or_else(|| SnapshotError::LegacyOrIncomplete(snapshot_id.to_owned()))?;
    let raw: Vec<SnapshotRow> = store.snapshot_rows(snapshot_id)?;

    for row in &raw {
        let expected = sha256_hex(row.canonical.as_bytes());
        if row.row_digest != expected {
            return Err(SnapshotError::RowSetIntegrity(
                snapshot_id.to_owned(),
                format!("row key {} has a corrupted per-row digest", row.key),
            ));
        }
    }

    if meta.expected_count == 0 {
        if raw.is_empty() {
            return Ok(Vec::new());
        }
        return Err(SnapshotError::RowSetIntegrity(
            snapshot_id.to_owned(),
            "completion metadata declares 0 rows but stored rows exist".to_owned(),
        ));
    }
    if raw.len() as u64 != meta.expected_count {
        return Err(SnapshotError::RowSetIntegrity(
            snapshot_id.to_owned(),
            format!(
                "completion metadata declares {} rows but {} rows are stored",
                meta.expected_count,
                raw.len()
            ),
        ));
    }
    if row_set_digest(&raw) != meta.row_set_digest {
        return Err(SnapshotError::RowSetIntegrity(
            snapshot_id.to_owned(),
            "stored row-set digest disagrees with completion metadata".to_owned(),
        ));
    }
    Ok(raw
        .into_iter()
        .map(|row| RecordRow {
            key: row.key,
            canonical: row.canonical,
            raw_json: row.raw_json,
        })
        .collect())
}

/// Convenience for hashing a record row's canonical form.
pub fn row_key(normalized_identifier: &str, row_hash: &str) -> String {
    sha256_hex(format!("{normalized_identifier}\0{row_hash}").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn acquisition_for(source_id: &str, url: &str, digest: &str, at: &str) -> Acquisition {
        Acquisition {
            source_id: source_id.to_owned(),
            source_url: url.to_owned(),
            retrieved_at: at.to_owned(),
            bytes_digest: digest.to_owned(),
            content_type: Some("text/html".to_owned()),
            etag: None,
            last_modified: None,
            final_url: url.to_owned(),
            redirect_history: Vec::new(),
            parser_version: "awards-1.0".to_owned(),
            schema_version: 2,
            authority: SourceAuthority::OfficialInformationalMirror,
            coverage_state: CoverageState::InformationalOnly,
            observations: Vec::new(),
        }
    }

    #[test]
    fn record_diff_detects_added_changed_removed() {
        let old = vec![
            RecordRow {
                key: "A".into(),
                canonical: "A1".into(),
                raw_json: String::new(),
            },
            RecordRow {
                key: "B".into(),
                canonical: "B1".into(),
                raw_json: String::new(),
            },
        ];
        let new = vec![
            RecordRow {
                key: "A".into(),
                canonical: "A2".into(),
                raw_json: String::new(),
            },
            RecordRow {
                key: "C".into(),
                canonical: "C1".into(),
                raw_json: String::new(),
            },
        ];
        let diff = record_diff("old", "new", "src", &old, &new);
        let kinds: Vec<&str> = diff.changes.iter().map(|c| c.kind.as_str()).collect();
        assert!(kinds.contains(&"changed"));
        assert!(kinds.contains(&"added"));
        assert!(kinds.contains(&"removed"));
        // A changed, added, and removed => 3 changes, no duplicates.
        assert_eq!(diff.changes.len(), 3);
    }

    #[test]
    fn record_diff_ignores_reordering() {
        let old = vec![
            RecordRow {
                key: "A".into(),
                canonical: "A1".into(),
                raw_json: String::new(),
            },
            RecordRow {
                key: "B".into(),
                canonical: "B1".into(),
                raw_json: String::new(),
            },
        ];
        let new = vec![
            RecordRow {
                key: "B".into(),
                canonical: "B1".into(),
                raw_json: String::new(),
            },
            RecordRow {
                key: "A".into(),
                canonical: "A1".into(),
                raw_json: String::new(),
            },
        ];
        let diff = record_diff("old", "new", "src", &old, &new);
        assert!(diff.changes.is_empty(), "reordering is not a change");
    }

    #[test]
    fn snapshot_preserves_old_and_links_new() {
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let first = acquisition_for("src", "https://x/a", "digest-old", "2026-08-17T00:00:00Z");
        let first_rows = vec![RecordRow {
            key: "A".into(),
            canonical: "A1".into(),
            raw_json: String::new(),
        }];
        let (snap1, diff1) =
            record_snapshot(&store, &first, None, Some(1), &[], &first_rows).expect("first");
        assert!(diff1.is_none());
        let second = acquisition_for("src", "https://x/a", "digest-new", "2026-08-17T01:00:00Z");
        let second_rows = vec![RecordRow {
            key: "A".into(),
            canonical: "A2".into(),
            raw_json: String::new(),
        }];
        let (snap2, diff2) =
            record_snapshot(&store, &second, Some(&snap1), Some(1), &[], &second_rows)
                .expect("second");
        // The diff is computed from the prior snapshot's *stored* rows.
        assert!(diff2.is_some());
        assert_eq!(snap2.supersedes.as_deref(), Some(snap1.id.as_str()));
        // Both snapshots persisted immutably.
        assert_eq!(store.source_snapshots("src").expect("list").len(), 2);
        // Coverage ledger has two entries.
        assert_eq!(store.coverage_entries("src").expect("coverage").len(), 2);
        // The prior snapshot's rows were persisted and are loadable.
        let loaded = snapshot_rows(&store, &snap1.id).expect("stored rows");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].key, "A");
        assert_eq!(loaded[0].canonical, "A1");
    }

    #[test]
    fn identical_bytes_produce_no_diff() {
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let first = acquisition_for("src", "https://x/a", "digest-same", "2026-08-17T00:00:00Z");
        let (snap1, _) = record_snapshot(&store, &first, None, Some(1), &[], &[]).expect("first");
        let second = acquisition_for("src", "https://x/a", "digest-same", "2026-08-17T01:00:00Z");
        let (snap2, diff2) =
            record_snapshot(&store, &second, Some(&snap1), Some(1), &[], &[]).expect("second");
        // Same digest -> snapshot is a duplicate; no diff.
        assert_eq!(snap2.persisted_digest, snap1.persisted_digest);
        assert!(diff2.is_none());
    }

    #[test]
    fn unchanged_304_records_provenance_without_duplicate() {
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let first = acquisition_for("src", "https://x/a", "digest-x", "2026-08-17T00:00:00Z");
        let (snap1, _) = record_snapshot(&store, &first, None, Some(1), &[], &[]).expect("first");
        let unchanged = acquisition_for("src", "https://x/a", "digest-x", "2026-08-17T01:00:00Z");
        let unchanged_flag = record_unchanged(&store, &unchanged, &snap1).expect("unchanged");
        assert!(unchanged_flag);
        // Still only one snapshot; coverage ledger gained a 304 entry.
        assert_eq!(store.source_snapshots("src").expect("list").len(), 1);
        let coverage = store.coverage_entries("src").expect("coverage");
        assert!(coverage.iter().any(|e| e.http_status == Some(304)));
    }

    fn row(key: &str, canonical: &str) -> RecordRow {
        RecordRow {
            key: key.to_owned(),
            canonical: canonical.to_owned(),
            raw_json: String::new(),
        }
    }

    /// Reopens the store from the same data directory, simulating a process
    /// restart. The prior `Store` is dropped first so WAL is checkpointed.
    fn restart(store: pnull_core::Store, dir: &std::path::Path) -> pnull_core::Store {
        drop(store);
        pnull_core::Store::open(dir).expect("reopen store after restart")
    }

    #[test]
    fn rows_survive_restart_and_fixture_is_not_needed() {
        // Snapshot A ingestion, then a process/store restart, then fixture
        // deletion, then snapshot B compared using database state alone.
        let dir = tempdir().expect("temp");
        let data_dir = dir.path().join("data");
        let store = pnull_core::Store::open(&data_dir).expect("store");
        let source = "src";
        let a = acquisition_for(source, "https://x/a", "digest-a", "2026-08-17T00:00:00Z");
        let a_rows = vec![row("k1", "v1"), row("k2", "v2")];
        let (snap_a, diff_a) =
            record_snapshot(&store, &a, None, Some(2), &[], &a_rows).expect("snapshot A");
        assert!(diff_a.is_none());

        // Simulate a process restart by reopening the database.
        let store = restart(store, &data_dir);
        // The fixture file is gone — nothing should need it.
        let fixture_path = dir.path().join("gone.html");
        std::fs::remove_file(&fixture_path).ok();

        // Snapshot B ingestion, prior from the database's stored rows.
        let b = acquisition_for(source, "https://x/a", "digest-b", "2026-08-17T01:00:00Z");
        let b_rows = vec![row("k1", "v1-changed"), row("k3", "v3")];
        let (snap_b, diff_b) =
            record_snapshot(&store, &b, Some(&snap_a), Some(2), &[], &b_rows).expect("snapshot B");
        let diff = diff_b.expect("diff must be computed from stored rows");
        // k2 removed, k3 added, k1 changed.
        let kinds: Vec<&str> = diff.changes.iter().map(|c| c.kind.as_str()).collect();
        assert!(kinds.contains(&"removed"));
        assert!(kinds.contains(&"added"));
        assert!(kinds.contains(&"changed"));
        // Both snapshots' rows are loadable from the database.
        assert_eq!(snapshot_rows(&store, &snap_a.id).expect("a rows").len(), 2);
        assert_eq!(snapshot_rows(&store, &snap_b.id).expect("b rows").len(), 2);
    }

    #[test]
    fn identical_reingestion_produces_no_duplicate_snapshots_or_alerts() {
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let source = "src";
        let rows = vec![row("k1", "v1"), row("k2", "v2")];
        let a = acquisition_for(source, "https://x/a", "digest-dup", "2026-08-17T00:00:00Z");
        let (snap_a, _) = record_snapshot(&store, &a, None, Some(2), &[], &rows).expect("first");
        // Re-ingest the identical snapshot; no duplicate snapshot, no error.
        let (snap_a2, _) =
            record_snapshot(&store, &a, None, Some(2), &[], &rows).expect("re-ingest");
        assert_eq!(snap_a.id, snap_a2.id);
        assert_eq!(store.source_snapshots(source).expect("snap").len(), 1);
        assert_eq!(snapshot_rows(&store, &snap_a.id).expect("rows").len(), 2);
    }

    #[test]
    fn reordering_rows_is_stable() {
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let source = "src";
        let a = acquisition_for(source, "https://x/a", "digest-r1", "2026-08-17T00:00:00Z");
        let (snap_a, _) = record_snapshot(
            &store,
            &a,
            None,
            Some(2),
            &[],
            &[row("k1", "v1"), row("k2", "v2")],
        )
        .expect("snapshot A");
        let b = acquisition_for(source, "https://x/a", "digest-r2", "2026-08-17T01:00:00Z");
        let (_, diff) = record_snapshot(
            &store,
            &b,
            Some(&snap_a),
            Some(2),
            &[],
            &[row("k2", "v2"), row("k1", "v1")],
        )
        .expect("snapshot B");
        // Reordering the same rows is not a change.
        assert!(diff.is_none() || diff.expect("diff").changes.is_empty());
    }

    #[test]
    fn duplicate_stable_row_keys_are_supported() {
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let source = "src";
        let a = acquisition_for(source, "https://x/a", "digest-d1", "2026-08-17T00:00:00Z");
        let (snap_a, _) = record_snapshot(
            &store,
            &a,
            None,
            Some(2),
            &[],
            &[row("k", "v1"), row("k", "v1")],
        )
        .expect("snapshot A");
        let b = acquisition_for(source, "https://x/a", "digest-d2", "2026-08-17T01:00:00Z");
        let (_, diff) = record_snapshot(&store, &b, Some(&snap_a), Some(2), &[], &[row("k", "v1")])
            .expect("snapshot B");
        // One of the two duplicate rows is removed -> a single removal.
        let diff = diff.expect("diff");
        let removals = diff.changes.iter().filter(|c| c.kind == "removed").count();
        assert_eq!(removals, 1);
    }

    #[test]
    fn parser_version_changes_are_recorded() {
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let source = "src";
        let mut a = acquisition_for(source, "https://x/a", "digest-p1", "2026-08-17T00:00:00Z");
        a.parser_version = "awards-1.0".to_owned();
        let (snap_a, _) =
            record_snapshot(&store, &a, None, Some(1), &[], &[row("k", "v1")]).expect("snapshot A");
        let mut b = acquisition_for(source, "https://x/a", "digest-p2", "2026-08-17T01:00:00Z");
        b.parser_version = "awards-2.0".to_owned();
        let (snap_b, _) =
            record_snapshot(&store, &b, Some(&snap_a), Some(1), &[], &[row("k", "v1")])
                .expect("snapshot B");
        // Each snapshot retains its own parser version in its stored row set.
        let meta_a = store
            .snapshot_row_set(&snap_a.id)
            .expect("meta a")
            .expect("some");
        let meta_b = store
            .snapshot_row_set(&snap_b.id)
            .expect("meta b")
            .expect("some");
        assert_eq!(meta_a.parser_version, "awards-1.0");
        assert_eq!(meta_b.parser_version, "awards-2.0");
    }

    #[test]
    fn malformed_stored_rows_fail_closed() {
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let source = "src";
        let a = acquisition_for(source, "https://x/a", "digest-m1", "2026-08-17T00:00:00Z");
        let (snap_a, _) =
            record_snapshot(&store, &a, None, Some(1), &[], &[row("k", "v1")]).expect("snapshot A");
        // Corrupt the completion metadata so the declared count disagrees.
        store
            .connection()
            .execute(
                "UPDATE snapshot_row_sets SET expected_count = 99 WHERE snapshot_id = ?1",
                [&snap_a.id],
            )
            .expect("corrupt metadata");
        let err = snapshot_rows(&store, &snap_a.id).expect_err("must fail closed");
        assert!(matches!(err, SnapshotError::RowSetIntegrity(_, _)));
    }

    #[test]
    fn missing_legacy_row_data_is_a_limitation_not_a_fabrication() {
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        // A legacy snapshot captured before row persistence has a source
        // snapshot but no stored rows and no completion metadata.
        let legacy = acquisition_for(
            "src",
            "https://x/a",
            "digest-legacy",
            "2026-08-17T00:00:00Z",
        );
        let snap_legacy = legacy.snapshot(Some(1), None);
        store
            .insert_source_snapshot(&snap_legacy)
            .expect("insert legacy snapshot without rows");
        assert!(
            store
                .snapshot_row_set(&snap_legacy.id)
                .expect("meta")
                .is_none()
        );

        let new = acquisition_for("src", "https://x/a", "digest-new2", "2026-08-17T01:00:00Z");
        let (_, diff) = record_snapshot(
            &store,
            &new,
            Some(&snap_legacy),
            Some(1),
            &[],
            &[row("k", "v-new")],
        )
        .expect("snapshot B");
        // No diff is fabricated from missing legacy rows.
        assert!(diff.is_none());
    }

    #[test]
    fn corrupted_row_digest_fails_closed() {
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let a = acquisition_for("src", "https://x/a", "digest-c1", "2026-08-17T00:00:00Z");
        let (snap_a, _) =
            record_snapshot(&store, &a, None, Some(1), &[], &[row("k", "v1")]).expect("snapshot A");
        // Corrupt a stored row's digest.
        store
            .connection()
            .execute(
                "UPDATE snapshot_rows SET row_digest = 'bogus' WHERE snapshot_id = ?1",
                [&snap_a.id],
            )
            .expect("corrupt digest");
        let err = snapshot_rows(&store, &snap_a.id).expect_err("must fail closed");
        assert!(matches!(err, SnapshotError::RowSetIntegrity(_, _)));
    }

    #[test]
    fn conflicting_repersist_fails_closed_without_overwriting() {
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let a = acquisition_for("src", "https://x/a", "digest-conf", "2026-08-17T00:00:00Z");
        record_snapshot(&store, &a, None, Some(1), &[], &[row("k", "v1")]).expect("snapshot A");
        // Attempt to re-persist the same snapshot with different rows.
        let rows = vec![SnapshotRow {
            key: "k".to_owned(),
            canonical: "different".to_owned(),
            row_digest: sha256_hex(b"different"),
            raw_json: String::new(),
        }];
        let meta = SnapshotRowSet {
            snapshot_id: a.snapshot(Some(1), None).id,
            expected_count: 1,
            row_set_digest: row_set_digest(&rows),
            parser_version: "awards-1.0".to_owned(),
            schema_version: 2,
        };
        let err = store
            .insert_snapshot_row_set_with_rows(&meta, &rows)
            .expect_err("conflict must fail");
        assert!(matches!(
            err,
            pnull_core::CoreError::SnapshotRowSetConflict { .. }
        ));
        // Historical rows are untouched.
        let loaded = snapshot_rows(&store, &a.snapshot(Some(1), None).id).expect("rows");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].canonical, "v1");
    }
}
