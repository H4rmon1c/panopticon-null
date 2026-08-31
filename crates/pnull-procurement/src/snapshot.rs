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
    SourceAuthority, SourceSnapshot, sha256_hex,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SnapshotError {
    #[error("snapshot with id {0} does not exist")]
    NotFound(String),
    #[error("snapshots are from different sources: {old} vs {new}")]
    DifferentSources { old: String, new: String },
    #[error(
        "snapshot {0} has no row-set completion metadata; it is either a true legacy snapshot \
         (captured before row-level diffing) or an incomplete/interrupted capture. It cannot be \
         loaded as a complete row set. Re-ingest the source so a complete row set is captured."
    )]
    LegacyOrIncomplete(String),
    #[error(
        "snapshot {snapshot_id} row set is internally inconsistent: {detail}. Stored rows are \
         never overwritten or silently reinterpreted."
    )]
    RowSetIntegrity { snapshot_id: String, detail: String },
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
}

impl RecordRow {
    /// Converts to the core persisted form.
    pub fn to_snapshot_row(&self) -> pnull_core::SnapshotRow {
        pnull_core::SnapshotRow {
            key: self.key.clone(),
            canonical: self.canonical.clone(),
        }
    }
}

/// Produces a deterministic record-level diff between two snapshots' rows.
///
/// Rows are matched by `key`. A row present only in the old snapshot is a
/// removal; only in the new snapshot is an addition; present in both with a
/// different `canonical` is a change. Ordering is ignored. Duplicate rows with
/// the same key are compared as multisets, so a reordered or repeated identical
/// row is never a spurious change.
pub fn record_diff(
    old_snapshot_id: &str,
    new_snapshot_id: &str,
    source_id: &str,
    old_rows: &[RecordRow],
    new_rows: &[RecordRow],
) -> SnapshotDiff {
    let mut changes = Vec::new();
    let old_map = group_by_key(old_rows);
    let new_map = group_by_key(new_rows);

    for key in old_map.keys() {
        match new_map.get(key) {
            None => {
                for _ in &old_map[key] {
                    changes.push(RecordChange {
                        kind: "removed".to_owned(),
                        row_key: (*key).to_owned(),
                        summary: format!("record {key} disappeared from the later snapshot"),
                    });
                }
            }
            Some(new_vals) => {
                let old_vals = &old_map[key];
                let removed = multiset_difference(old_vals, new_vals);
                let added = multiset_difference(new_vals, old_vals);
                if removed.is_empty() && added.is_empty() {
                    continue;
                }
                let paired = removed.len().min(added.len());
                for _ in 0..paired {
                    changes.push(RecordChange {
                        kind: "changed".to_owned(),
                        row_key: (*key).to_owned(),
                        summary: format!("record {key} changed between snapshots"),
                    });
                }
                for _ in paired..removed.len() {
                    changes.push(RecordChange {
                        kind: "removed".to_owned(),
                        row_key: (*key).to_owned(),
                        summary: format!("record {key} disappeared from the later snapshot"),
                    });
                }
                for _ in paired..added.len() {
                    changes.push(RecordChange {
                        kind: "added".to_owned(),
                        row_key: (*key).to_owned(),
                        summary: format!("record {key} appeared in the later snapshot"),
                    });
                }
            }
        }
    }
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
    SnapshotDiff {
        id: SnapshotDiff::id_for(old_snapshot_id, new_snapshot_id),
        old_snapshot_id: old_snapshot_id.to_owned(),
        new_snapshot_id: new_snapshot_id.to_owned(),
        source_id: source_id.to_owned(),
        changes,
        produced_at: "deterministic".to_owned(),
    }
}

/// Groups rows by key with each key's sorted canonical values (a multiset).
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

/// Multiset difference: returns the values in `a` not fully matched in `b`
/// (each value matched up to its multiplicity in `b`).
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

/// Records a snapshot, its revision link to a prior snapshot, and coverage entry.
///
/// `prior_snapshot` is the most recent snapshot of the same source, if any. The
/// old snapshot is never modified; only a new revision link is added. This
/// snapshot's deterministic parsed `rows` are persisted for later record-level
/// diffing. When a prior snapshot exists with different bytes, the new rows are
/// compared against the prior snapshot's *stored* rows — never against
/// themselves. A prior snapshot with no stored rows (a legacy snapshot) cannot
/// be diffed honestly, so no diff is produced and none is fabricated.
pub fn record_snapshot(
    store: &pnull_core::Store,
    acquisition: &Acquisition,
    prior_snapshot: Option<&SourceSnapshot>,
    record_count: Option<u64>,
    rows: &[RecordRow],
) -> Result<(SourceSnapshot, Option<SnapshotDiff>), SnapshotError> {
    let snapshot = acquisition.snapshot(record_count, prior_snapshot.map(|s| s.id.clone()));
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
    let coverage = acquisition.coverage_entry(record_count, "snapshot captured");
    store.insert_coverage_entry(&coverage)?;

    // Persist this snapshot's row-set completion metadata and every row in a
    // single transaction. The metadata row is the completion marker and is
    // written last, so a failure rolls back leaving neither partial rows nor a
    // completion marker. The digest is deterministic and ordering-independent,
    // so an identical logical row set retried later is an idempotent success.
    let persisted_rows: Vec<pnull_core::SnapshotRow> =
        rows.iter().map(RecordRow::to_snapshot_row).collect();
    let meta = pnull_core::SnapshotRowSet {
        snapshot_id: snapshot.id.clone(),
        expected_count: rows.len() as u64,
        row_set_digest: pnull_core::row_set_digest(&persisted_rows),
        parser_version: acquisition.parser_version.clone(),
        schema_version: acquisition.schema_version,
    };
    store.insert_snapshot_row_set_with_rows(&meta, &persisted_rows)?;

    let diff = if let Some(prior) = prior_snapshot {
        if prior.persisted_digest == snapshot.persisted_digest {
            None
        } else {
            // Load the prior snapshot's stored rows and compare them against the
            // new rows. Never compare the new rows against themselves. A prior
            // snapshot with no completion metadata is a true legacy or incomplete
            // capture and cannot be diffed honestly, so no diff is produced.
            if store.snapshot_row_set(&prior.id)?.is_some() {
                let prior_rows = snapshot_rows(store, &prior.id)?;
                Some(record_diff(
                    &prior.id,
                    &snapshot.id,
                    &snapshot.source_id,
                    &prior_rows,
                    rows,
                ))
            } else {
                None
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

/// Loads a snapshot's persisted deterministic record rows, verified against its
/// row-set completion metadata.
///
/// Loading semantics:
/// - No completion metadata means the snapshot is a true legacy capture or an
///   incomplete/interrupted capture; it fails honestly with
///   [`SnapshotError::LegacyOrIncomplete`] rather than fabricating rows.
/// - Completion metadata declaring zero rows returns `Ok([])`.
/// - Any disagreement between the metadata (expected count, digest) and the
///   stored rows returns a [`SnapshotError::RowSetIntegrity`] error. Stored
///   rows are never overwritten or silently reinterpreted.
pub fn snapshot_rows(
    store: &pnull_core::Store,
    snapshot_id: &str,
) -> Result<Vec<RecordRow>, SnapshotError> {
    let meta = store
        .snapshot_row_set(snapshot_id)?
        .ok_or_else(|| SnapshotError::LegacyOrIncomplete(snapshot_id.to_owned()))?;
    let raw: Vec<pnull_core::SnapshotRow> = store.snapshot_rows(snapshot_id)?;
    let rows: Vec<RecordRow> = raw
        .iter()
        .map(|row| RecordRow {
            key: row.key.clone(),
            canonical: row.canonical.clone(),
        })
        .collect();

    if meta.expected_count == 0 {
        if raw.is_empty() {
            // A valid captured zero-record snapshot.
            return Ok(Vec::new());
        }
        return Err(SnapshotError::RowSetIntegrity {
            snapshot_id: snapshot_id.to_owned(),
            detail: "completion metadata declares 0 rows but stored rows exist".to_owned(),
        });
    }
    if raw.len() as u64 != meta.expected_count {
        return Err(SnapshotError::RowSetIntegrity {
            snapshot_id: snapshot_id.to_owned(),
            detail: format!(
                "completion metadata declares {} rows but {} rows are stored",
                meta.expected_count,
                raw.len()
            ),
        });
    }
    if pnull_core::row_set_digest(&raw) != meta.row_set_digest {
        return Err(SnapshotError::RowSetIntegrity {
            snapshot_id: snapshot_id.to_owned(),
            detail: "stored row-set digest disagrees with completion metadata".to_owned(),
        });
    }
    Ok(rows)
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
            },
            RecordRow {
                key: "B".into(),
                canonical: "B1".into(),
            },
        ];
        let new = vec![
            RecordRow {
                key: "A".into(),
                canonical: "A2".into(),
            },
            RecordRow {
                key: "C".into(),
                canonical: "C1".into(),
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
            },
            RecordRow {
                key: "B".into(),
                canonical: "B1".into(),
            },
        ];
        let new = vec![
            RecordRow {
                key: "B".into(),
                canonical: "B1".into(),
            },
            RecordRow {
                key: "A".into(),
                canonical: "A1".into(),
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
        let old_rows = vec![RecordRow {
            key: "A".into(),
            canonical: "A1".into(),
        }];
        let (snap1, diff1) =
            record_snapshot(&store, &first, None, Some(1), &old_rows).expect("first");
        assert!(diff1.is_none());
        let second = acquisition_for("src", "https://x/a", "digest-new", "2026-08-17T01:00:00Z");
        let new_rows = vec![RecordRow {
            key: "A".into(),
            canonical: "A2".into(),
        }];
        let (snap2, diff2) =
            record_snapshot(&store, &second, Some(&snap1), Some(1), &new_rows).expect("second");
        assert!(diff2.is_some());
        assert_eq!(snap2.supersedes.as_deref(), Some(snap1.id.as_str()));
        // Both snapshots persisted immutably.
        assert_eq!(store.source_snapshots("src").expect("list").len(), 2);
        // Coverage ledger has two entries.
        assert_eq!(store.coverage_entries("src").expect("coverage").len(), 2);
    }

    #[test]
    fn identical_bytes_produce_no_diff() {
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let first = acquisition_for("src", "https://x/a", "digest-same", "2026-08-17T00:00:00Z");
        let (snap1, _) = record_snapshot(&store, &first, None, Some(1), &[]).expect("first");
        let second = acquisition_for("src", "https://x/a", "digest-same", "2026-08-17T01:00:00Z");
        let (snap2, diff2) =
            record_snapshot(&store, &second, Some(&snap1), Some(1), &[]).expect("second");
        // Same digest -> snapshot is a duplicate; no diff.
        assert_eq!(snap2.persisted_digest, snap1.persisted_digest);
        assert!(diff2.is_none());
    }

    #[test]
    fn unchanged_304_records_provenance_without_duplicate() {
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let first = acquisition_for("src", "https://x/a", "digest-x", "2026-08-17T00:00:00Z");
        let (snap1, _) = record_snapshot(&store, &first, None, Some(1), &[]).expect("first");
        let unchanged = acquisition_for("src", "https://x/a", "digest-x", "2026-08-17T01:00:00Z");
        let unchanged_flag = record_unchanged(&store, &unchanged, &snap1).expect("unchanged");
        assert!(unchanged_flag);
        // Still only one snapshot; coverage ledger gained a 304 entry.
        assert_eq!(store.source_snapshots("src").expect("list").len(), 1);
        let coverage = store.coverage_entries("src").expect("coverage");
        assert!(coverage.iter().any(|e| e.http_status == Some(304)));
    }

    #[test]
    fn record_diff_handles_duplicate_row_keys_as_multiset() {
        // Two rows share the key "R21-T107KK" (a joint award). Reordering the
        // duplicates must not register as a change.
        let old = vec![
            RecordRow {
                key: "R21-T107KK".into(),
                canonical: "United Rentals".into(),
            },
            RecordRow {
                key: "R21-T107KK".into(),
                canonical: "Herc Rentals Inc.".into(),
            },
        ];
        let reordered = vec![
            RecordRow {
                key: "R21-T107KK".into(),
                canonical: "Herc Rentals Inc.".into(),
            },
            RecordRow {
                key: "R21-T107KK".into(),
                canonical: "United Rentals".into(),
            },
        ];
        let diff = record_diff("old", "new", "src", &old, &reordered);
        assert!(
            diff.changes.is_empty(),
            "reordered duplicates are not a change"
        );

        // A changed duplicate: one of the two rows' content changes.
        let changed = vec![
            RecordRow {
                key: "R21-T107KK".into(),
                canonical: "Herc Rentals Inc.".into(),
            },
            RecordRow {
                key: "R21-T107KK".into(),
                canonical: "United Rentals of CO".into(),
            },
        ];
        let diff2 = record_diff("old", "new", "src", &old, &changed);
        assert_eq!(diff2.changes.len(), 1);
        assert_eq!(diff2.changes[0].kind, "changed");

        // A duplicate removed: only one row remains.
        let reduced = vec![RecordRow {
            key: "R21-T107KK".into(),
            canonical: "United Rentals".into(),
        }];
        let diff3 = record_diff("old", "new", "src", &old, &reduced);
        assert_eq!(diff3.changes.len(), 1);
        assert_eq!(diff3.changes[0].kind, "removed");
    }

    #[test]
    fn record_diff_output_is_deterministic_regardless_of_input_order() {
        // The same logical row sets produce byte-identical diffs no matter the
        // order the rows are supplied in.
        let old_order_a = vec![
            RecordRow {
                key: "B".into(),
                canonical: "B1".into(),
            },
            RecordRow {
                key: "A".into(),
                canonical: "A1".into(),
            },
        ];
        let old_order_b = vec![
            RecordRow {
                key: "A".into(),
                canonical: "A1".into(),
            },
            RecordRow {
                key: "B".into(),
                canonical: "B1".into(),
            },
        ];
        let new = vec![
            RecordRow {
                key: "A".into(),
                canonical: "A2".into(),
            },
            RecordRow {
                key: "C".into(),
                canonical: "C1".into(),
            },
        ];
        let diff1 = record_diff("old", "new", "src", &old_order_a, &new);
        let diff2 = record_diff("old", "new", "src", &old_order_b, &new);
        assert_eq!(
            diff1.changes, diff2.changes,
            "output must not depend on input order"
        );
        // And the same inputs always produce identical output.
        let diff3 = record_diff("old", "new", "src", &old_order_a, &new);
        assert_eq!(diff1.changes, diff3.changes);
    }

    #[test]
    fn record_snapshot_persists_rows_and_diffs_against_prior_not_self() {
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let first = acquisition_for("src", "https://x/a", "digest-old", "2026-08-17T00:00:00Z");
        let old_rows = vec![
            RecordRow {
                key: "A".into(),
                canonical: "A1".into(),
            },
            RecordRow {
                key: "B".into(),
                canonical: "B1".into(),
            },
        ];
        let (snap1, _) = record_snapshot(&store, &first, None, Some(2), &old_rows).expect("first");
        // The first snapshot's rows are persisted.
        assert_eq!(snapshot_rows(&store, &snap1.id).expect("stored"), old_rows);

        let second = acquisition_for("src", "https://x/a", "digest-new", "2026-08-17T01:00:00Z");
        let new_rows = vec![
            RecordRow {
                key: "A".into(),
                canonical: "A2".into(),
            },
            RecordRow {
                key: "C".into(),
                canonical: "C1".into(),
            },
        ];
        let (snap2, diff2) =
            record_snapshot(&store, &second, Some(&snap1), Some(2), &new_rows).expect("second");
        // The diff reflects real differences between the prior stored rows and
        // the new rows (never the new rows against themselves).
        let diff = diff2.expect("diff present");
        let kinds: Vec<&str> = diff.changes.iter().map(|c| c.kind.as_str()).collect();
        assert!(kinds.contains(&"changed"));
        assert!(kinds.contains(&"added"));
        assert!(kinds.contains(&"removed"));
        // The second snapshot's rows are also persisted.
        assert_eq!(snapshot_rows(&store, &snap2.id).expect("stored"), new_rows);
    }

    #[test]
    fn valid_captured_zero_record_snapshot_loads_as_empty() {
        // A snapshot captured through the row-diffing path with zero parsed
        // records is a valid empty snapshot, not a legacy one.
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let first = acquisition_for("src", "https://x/a", "digest-empty", "2026-08-17T00:00:00Z");
        let (snap1, _) = record_snapshot(&store, &first, None, Some(0), &[]).expect("first");
        assert_eq!(
            snapshot_rows(&store, &snap1.id).expect("stored"),
            Vec::<RecordRow>::new()
        );
    }

    #[test]
    fn true_legacy_snapshot_without_metadata_fails_honestly() {
        // A snapshot with no completion metadata is a true legacy or incomplete
        // capture. Loading it must fail honestly rather than fabricate rows.
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let snap_id = "snapshot:legacy";
        {
            let connection =
                rusqlite::Connection::open(store.data_dir().join("pnull.db")).expect("open db");
            connection
                .execute(
                    "INSERT INTO source_snapshots(id, source_id, snapshot_json)
                     VALUES (?1, 'src', '{\"id\":\"snapshot:legacy\",\"source_id\":\"src\"}')",
                    [snap_id],
                )
                .expect("insert legacy snapshot");
        }
        assert!(matches!(
            snapshot_rows(&store, snap_id),
            Err(SnapshotError::LegacyOrIncomplete(_))
        ));
    }

    #[test]
    fn empty_to_nonempty_snapshot_is_added_records() {
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let first = acquisition_for("src", "https://x/a", "digest-old", "2026-08-17T00:00:00Z");
        let (snap1, _) = record_snapshot(&store, &first, None, Some(0), &[]).expect("empty");
        let second = acquisition_for("src", "https://x/a", "digest-new", "2026-08-17T01:00:00Z");
        let new_rows = vec![RecordRow {
            key: "A".into(),
            canonical: "A1".into(),
        }];
        let (_, diff2) =
            record_snapshot(&store, &second, Some(&snap1), Some(1), &new_rows).expect("second");
        let diff = diff2.expect("empty->nonempty diff present");
        assert_eq!(diff.changes.len(), 1);
        assert_eq!(diff.changes[0].kind, "added");
    }

    #[test]
    fn nonempty_to_empty_snapshot_is_removed_records() {
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let first = acquisition_for("src", "https://x/a", "digest-old", "2026-08-17T00:00:00Z");
        let old_rows = vec![RecordRow {
            key: "A".into(),
            canonical: "A1".into(),
        }];
        let (snap1, _) =
            record_snapshot(&store, &first, None, Some(1), &old_rows).expect("nonempty");
        let second = acquisition_for("src", "https://x/a", "digest-new", "2026-08-17T01:00:00Z");
        let (_, diff2) =
            record_snapshot(&store, &second, Some(&snap1), Some(0), &[]).expect("second");
        let diff = diff2.expect("nonempty->empty diff present");
        assert_eq!(diff.changes.len(), 1);
        assert_eq!(diff.changes[0].kind, "removed");
    }

    #[test]
    fn empty_to_empty_snapshot_is_no_changes() {
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let first = acquisition_for("src", "https://x/a", "digest-old", "2026-08-17T00:00:00Z");
        let (snap1, _) = record_snapshot(&store, &first, None, Some(0), &[]).expect("empty");
        let second = acquisition_for("src", "https://x/a", "digest-new", "2026-08-17T01:00:00Z");
        let (_, diff2) =
            record_snapshot(&store, &second, Some(&snap1), Some(0), &[]).expect("second");
        // Both snapshots are empty; the diff must report no record changes.
        match diff2 {
            None => {}
            Some(diff) => assert!(diff.changes.is_empty()),
        }
    }

    #[test]
    fn identical_retry_is_idempotent_success() {
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let first = acquisition_for("src", "https://x/a", "digest-same", "2026-08-17T00:00:00Z");
        let rows = vec![
            RecordRow {
                key: "A".into(),
                canonical: "A1".into(),
            },
            RecordRow {
                key: "B".into(),
                canonical: "B1".into(),
            },
        ];
        let (snap1, _) = record_snapshot(&store, &first, None, Some(2), &rows).expect("first");
        // Retry with an identical logical row set for the same snapshot.
        record_snapshot(&store, &first, None, Some(2), &rows).expect("idempotent retry");
        assert_eq!(snapshot_rows(&store, &snap1.id).expect("stored"), rows);
    }

    #[test]
    fn reordered_but_logically_identical_retry_is_idempotent_success() {
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let first = acquisition_for("src", "https://x/a", "digest-same", "2026-08-17T00:00:00Z");
        let rows = vec![
            RecordRow {
                key: "A".into(),
                canonical: "A1".into(),
            },
            RecordRow {
                key: "B".into(),
                canonical: "B1".into(),
            },
        ];
        let (snap1, _) = record_snapshot(&store, &first, None, Some(2), &rows).expect("first");
        // Reordered but logically identical row set: same multiset.
        let reordered = vec![
            RecordRow {
                key: "B".into(),
                canonical: "B1".into(),
            },
            RecordRow {
                key: "A".into(),
                canonical: "A1".into(),
            },
        ];
        record_snapshot(&store, &first, None, Some(2), &reordered).expect("reordered retry");
        assert_eq!(snapshot_rows(&store, &snap1.id).expect("stored"), rows);
    }

    #[test]
    fn conflicting_retry_rejection() {
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let first = acquisition_for("src", "https://x/a", "digest-same", "2026-08-17T00:00:00Z");
        let rows = vec![RecordRow {
            key: "A".into(),
            canonical: "A1".into(),
        }];
        let (snap1, _) = record_snapshot(&store, &first, None, Some(1), &rows).expect("first");
        // A retry with a different logical row set for the same snapshot fails
        // loudly and never overwrites the stored rows.
        let conflicting = vec![RecordRow {
            key: "A".into(),
            canonical: "A2".into(),
        }];
        assert!(matches!(
            record_snapshot(&store, &first, None, Some(1), &conflicting),
            Err(SnapshotError::Store(
                pnull_core::CoreError::SnapshotRowSetConflict { .. }
            ))
        ));
        // Stored rows are unchanged.
        assert_eq!(snapshot_rows(&store, &snap1.id).expect("stored"), rows);
    }

    #[test]
    fn injected_mid_write_failure_rolls_back_fully() {
        // A failed write must leave neither a completion marker nor partial rows.
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let snap_id = "snapshot:injected";
        let meta = pnull_core::SnapshotRowSet {
            snapshot_id: snap_id.to_owned(),
            expected_count: 2,
            row_set_digest: "x".to_owned(),
            parser_version: "p".to_owned(),
            schema_version: 4,
        };
        let rows = vec![
            pnull_core::SnapshotRow {
                key: "A".into(),
                canonical: "A1".into(),
            },
            pnull_core::SnapshotRow {
                key: "B".into(),
                canonical: "B1".into(),
            },
        ];
        // Sabotage: make the completion-marker insert fail after the rows would
        // have been written, to prove the entire transaction rolls back.
        {
            let connection =
                rusqlite::Connection::open(store.data_dir().join("pnull.db")).expect("open db");
            connection
                .execute_batch(
                    "CREATE TRIGGER sabotage_meta BEFORE INSERT ON snapshot_row_sets
                     BEGIN SELECT RAISE(ABORT, 'injected failure'); END;",
                )
                .expect("create sabotage trigger");
        }
        let result = store.insert_snapshot_row_set_with_rows(&meta, &rows);
        // Drop the trigger so it does not affect later statements.
        {
            let connection =
                rusqlite::Connection::open(store.data_dir().join("pnull.db")).expect("open db");
            connection
                .execute_batch("DROP TRIGGER IF EXISTS sabotage_meta;")
                .expect("drop trigger");
        }
        assert!(result.is_err(), "injected failure must propagate");
        // Neither a completion marker nor any partial rows may persist.
        assert!(store.snapshot_row_set(snap_id).expect("meta").is_none());
        assert!(
            store.snapshot_rows(snap_id).expect("rows").is_empty(),
            "no partial rows may persist after a failed write"
        );
    }

    #[test]
    fn marker_count_or_digest_corruption_is_detected() {
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let first = acquisition_for("src", "https://x/a", "digest-x", "2026-08-17T00:00:00Z");
        let rows = vec![
            RecordRow {
                key: "A".into(),
                canonical: "A1".into(),
            },
            RecordRow {
                key: "B".into(),
                canonical: "B1".into(),
            },
        ];
        let (snap1, _) = record_snapshot(&store, &first, None, Some(2), &rows).expect("first");
        {
            let connection =
                rusqlite::Connection::open(store.data_dir().join("pnull.db")).expect("open db");
            // Corrupt the count.
            connection
                .execute(
                    "UPDATE snapshot_row_sets SET expected_count = 99 WHERE snapshot_id = ?1",
                    [&snap1.id],
                )
                .expect("corrupt count");
        }
        assert!(matches!(
            snapshot_rows(&store, &snap1.id),
            Err(SnapshotError::RowSetIntegrity { .. })
        ));
        // Restore count and corrupt the digest.
        {
            let connection =
                rusqlite::Connection::open(store.data_dir().join("pnull.db")).expect("open db");
            connection
                .execute(
                    "UPDATE snapshot_row_sets SET expected_count = 2,
                            row_set_digest = 'deadbeef' WHERE snapshot_id = ?1",
                    [&snap1.id],
                )
                .expect("corrupt digest");
        }
        assert!(matches!(
            snapshot_rows(&store, &snap1.id),
            Err(SnapshotError::RowSetIntegrity { .. })
        ));
        // Corrupt a stored row directly (row count unchanged, digest differs).
        {
            let connection =
                rusqlite::Connection::open(store.data_dir().join("pnull.db")).expect("open db");
            connection
                .execute(
                    "UPDATE snapshot_rows SET canonical = 'ZZZ' WHERE snapshot_id = ?1 AND seq = 0",
                    [&snap1.id],
                )
                .expect("corrupt row");
        }
        assert!(matches!(
            snapshot_rows(&store, &snap1.id),
            Err(SnapshotError::RowSetIntegrity { .. })
        ));
    }

    #[test]
    fn row_set_digest_is_deterministic_and_order_independent() {
        let rows = vec![
            pnull_core::SnapshotRow {
                key: "A".into(),
                canonical: "A1".into(),
            },
            pnull_core::SnapshotRow {
                key: "B".into(),
                canonical: "B1".into(),
            },
        ];
        let reordered = vec![
            pnull_core::SnapshotRow {
                key: "B".into(),
                canonical: "B1".into(),
            },
            pnull_core::SnapshotRow {
                key: "A".into(),
                canonical: "A1".into(),
            },
        ];
        let d1 = pnull_core::row_set_digest(&rows);
        let d2 = pnull_core::row_set_digest(&reordered);
        assert_eq!(d1, d2, "order must not change the digest");
        let d3 = pnull_core::row_set_digest(&rows);
        assert_eq!(d1, d3, "digest must be stable");

        // Duplicates are preserved: removing one duplicate changes the digest.
        let fewer = vec![pnull_core::SnapshotRow {
            key: "A".into(),
            canonical: "A1".into(),
        }];
        assert_ne!(d1, pnull_core::row_set_digest(&fewer));
    }
}
