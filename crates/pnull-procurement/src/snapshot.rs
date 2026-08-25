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
        "snapshot {0} has no stored record rows; it predates record-level diffing \
         (legacy snapshot). Re-ingest the source to capture deterministic rows before diffing."
    )]
    NoStoredRows(String),
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

    // Persist this snapshot's deterministic parsed rows (additive).
    let snapshot_rows: Vec<pnull_core::SnapshotRow> =
        rows.iter().map(RecordRow::to_snapshot_row).collect();
    store.insert_snapshot_rows(&snapshot.id, &snapshot_rows)?;

    let diff = if let Some(prior) = prior_snapshot {
        if prior.persisted_digest == snapshot.persisted_digest {
            None
        } else {
            // Load the prior snapshot's stored rows and compare them against the
            // new rows. Never compare the new rows against themselves.
            let prior_rows: Vec<RecordRow> = store
                .snapshot_rows(&prior.id)?
                .into_iter()
                .map(|row| RecordRow {
                    key: row.key,
                    canonical: row.canonical,
                })
                .collect();
            if prior_rows.is_empty() {
                // Legacy prior snapshot with no stored rows: cannot diff honestly.
                None
            } else {
                Some(record_diff(
                    &prior.id,
                    &snapshot.id,
                    &snapshot.source_id,
                    &prior_rows,
                    rows,
                ))
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

/// Loads a snapshot's persisted deterministic record rows.
///
/// Fails honestly with [`SnapshotError::NoStoredRows`] when the snapshot has no
/// stored rows — i.e., it is a legacy snapshot recorded before record-level
/// diffing existed. Callers must not fall back to counts or digests as fake
/// records.
pub fn snapshot_rows(
    store: &pnull_core::Store,
    snapshot_id: &str,
) -> Result<Vec<RecordRow>, SnapshotError> {
    let rows: Vec<RecordRow> = store
        .snapshot_rows(snapshot_id)?
        .into_iter()
        .map(|row| RecordRow {
            key: row.key,
            canonical: row.canonical,
        })
        .collect();
    if rows.is_empty() {
        return Err(SnapshotError::NoStoredRows(snapshot_id.to_owned()));
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
    fn snapshot_rows_fails_honestly_when_no_rows_stored() {
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let first = acquisition_for(
            "src",
            "https://x/a",
            "digest-legacy",
            "2026-08-17T00:00:00Z",
        );
        let (snap1, _) = record_snapshot(&store, &first, None, Some(0), &[]).expect("first");
        // No rows were stored; reading them must fail honestly rather than
        // fabricating a record from counts or digests.
        assert!(matches!(
            snapshot_rows(&store, &snap1.id),
            Err(SnapshotError::NoStoredRows(_))
        ));
    }

    #[test]
    fn record_snapshot_does_not_fabricate_diff_for_legacy_prior() {
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        // A legacy prior snapshot recorded with no rows.
        let first = acquisition_for("src", "https://x/a", "digest-old", "2026-08-17T00:00:00Z");
        let (snap1, _) = record_snapshot(&store, &first, None, Some(0), &[]).expect("first");
        // A new snapshot that supersedes it, with real rows.
        let second = acquisition_for("src", "https://x/a", "digest-new", "2026-08-17T01:00:00Z");
        let new_rows = vec![RecordRow {
            key: "A".into(),
            canonical: "A1".into(),
        }];
        let (_, diff2) =
            record_snapshot(&store, &second, Some(&snap1), Some(1), &new_rows).expect("second");
        // Because the prior has no stored rows, no diff is fabricated.
        assert!(diff2.is_none());
    }
}
