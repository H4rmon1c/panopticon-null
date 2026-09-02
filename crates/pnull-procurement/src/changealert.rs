//! Deterministic procurement change alerts (v0.0.4, Item 1).
//!
//! When a reviewed procurement surface (the contract-award table, the
//! solicitation mirror, or a future adapter) is re-ingested and the new
//! snapshot differs from the latest snapshot, this module produces immutable
//! change alerts. Change kinds: `record_added`, `record_modified`,
//! `record_removed`. For award rows, `record_modified` carries a field-level
//! diff (field name, old raw value, new raw value).
//!
//! Row identity is a stable key: the official identifier where one is present;
//! otherwise a digest over the row's normalized field values (published rule
//! in `docs/procurement-methodology.md`). A digest key stays stable across
//! reordering because it is a function of the row's own normalized values, not
//! its position.
//!
//! Alerts are idempotent: re-ingesting the same snapshot pair never creates a
//! second alert, and a byte-identical snapshot (the 304 path) creates no
//! alerts at all.
//!
//! Phrasing discipline: a removal reports a comparison, not a legal
//! conclusion — "The row observed in snapshot N (digest …) is not present in
//! snapshot M (digest …)." No alert declares conduct unlawful, corrupt,
//! abusive, or malicious, and no alert labels a purchase as surveillance.

use pnull_core::{
    CoverageState, FieldDiff, ProcurementAlert, ProcurementChangeKind, ProcurementRecordChange,
    sha256_hex,
};
use thiserror::Error;

use crate::awards::AwardRow;
use crate::snapshot::RecordRow;

/// The retrieval timestamp is supplied by the caller (offline demos use the
/// fixed demonstration timestamp); it is never a wall-clock surprise in tests.

#[derive(Debug, Error)]
pub enum ChangeAlertError {
    #[error("store operation failed: {0}")]
    Store(#[from] pnull_core::CoreError),
    #[error("snapshots are from different sources: {old} vs {new}")]
    DifferentSources { old: String, new: String },
}

/// Normalized field values of an award row, used to build the stable digest key
/// when no official identifier is present. The value list is fixed and ordered
/// so the digest is a pure function of the row content, independent of ordering.
fn row_normalized_fields(row: &AwardRow) -> Vec<&str> {
    vec![
        &row.project_name,
        &row.contractor,
        &row.raw_amount,
        &row.start_date,
        &row.notes,
    ]
}

/// The stable row-identity key for an award row.
///
/// When the official solicitation identifier is present and non-empty, the key
/// is the normalized identifier (so the same official row keeps its identity
/// across snapshots even if edited). Otherwise the key is a digest over the
/// row's normalized field values, so it is stable across reordering but changes
/// when the row's own content changes.
pub fn row_identity(row: &AwardRow) -> String {
    if row.solicitation_id.trim().is_empty() {
        digest_identity(row)
    } else {
        pnull_core::identifier_match_key(&row.solicitation_id)
            .unwrap_or_else(|| digest_identity(row))
    }
}

/// A digest over the row's normalized field values (stable across reordering).
fn digest_identity(row: &AwardRow) -> String {
    sha256_hex(row_normalized_fields(row).join("\u{1f}").as_bytes())
}

/// Converts award rows to snapshot `RecordRow`s for a record-level diff.
///
/// The row key is the same stable row identity used by change alerts, so the
/// snapshot-level diff and the change alerts share one notion of a row.
pub fn award_record_rows(rows: &[AwardRow]) -> Vec<RecordRow> {
    rows.iter()
        .map(|row| RecordRow {
            key: row_identity(row),
            canonical: format!(
                "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
                row.project_name, row.contractor, row.raw_amount, row.start_date, row.notes
            ),
        })
        .collect()
}

/// Produces the deterministic field-level diff between two award rows for the
/// same identity. Only changed fields are reported, preserving raw strings.
pub fn field_diffs(old: &AwardRow, new: &AwardRow) -> Vec<FieldDiff> {
    let mut diffs = Vec::new();
    for (field, old_raw, new_raw) in [
        ("project_name", &old.project_name, &new.project_name),
        ("contractor", &old.contractor, &new.contractor),
        ("raw_amount", &old.raw_amount, &new.raw_amount),
        ("start_date", &old.start_date, &new.start_date),
        ("notes", &old.notes, &new.notes),
    ] {
        if old_raw != new_raw {
            diffs.push(FieldDiff {
                field: field.to_owned(),
                old_raw: old_raw.clone(),
                new_raw: new_raw.clone(),
            });
        }
    }
    diffs
}

/// Builds deterministic procurement change alerts between two snapshots.
///
/// `old_rows`/`new_rows` are the award rows parsed from the respective
/// snapshots. Rows are matched by stable identity. A row present only in the
/// old snapshot is a removal; only in the new snapshot is an addition; present
/// in both with different content is a modification carrying a field-level
/// diff. Ordering is ignored.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn build_change_alerts(
    source_id: &str,
    surface: &str,
    old_snapshot_id: &str,
    old_snapshot_digest: &str,
    new_snapshot_id: &str,
    new_snapshot_digest: &str,
    retrieved_at: &str,
    coverage_state: CoverageState,
    old_rows: &[AwardRow],
    new_rows: &[AwardRow],
    matter_ids: &[String],
    identifier_ids: &[String],
) -> Vec<ProcurementAlert> {
    let old_map: std::collections::BTreeMap<String, &AwardRow> = old_rows
        .iter()
        .map(|row| (row_identity(row), row))
        .collect();
    let new_map: std::collections::BTreeMap<String, &AwardRow> = new_rows
        .iter()
        .map(|row| (row_identity(row), row))
        .collect();

    let mut alerts: Vec<ProcurementAlert> = Vec::new();

    for (identity, old_row) in &old_map {
        match new_map.get(identity) {
            None => {
                let summary = format!(
                    "The row observed in snapshot {old_snapshot_id} (digest {old_snapshot_digest}) is not present in snapshot {new_snapshot_id} (digest {new_snapshot_digest})."
                );
                let change = ProcurementRecordChange {
                    change_kind: ProcurementChangeKind::RecordRemoved,
                    row_identity: identity.clone(),
                    field_diffs: Vec::new(),
                    old_snapshot_id: old_snapshot_id.to_owned(),
                    old_snapshot_digest: old_snapshot_digest.to_owned(),
                    new_snapshot_id: new_snapshot_id.to_owned(),
                    new_snapshot_digest: new_snapshot_digest.to_owned(),
                    summary: summary.clone(),
                };
                alerts.push(alert_for(
                    source_id,
                    surface,
                    old_snapshot_id,
                    old_snapshot_digest,
                    new_snapshot_id,
                    new_snapshot_digest,
                    retrieved_at,
                    coverage_state,
                    matter_ids,
                    identifier_ids,
                    change,
                    summary,
                ));
            }
            Some(new_row) => {
                let diffs = field_diffs(old_row, new_row);
                if diffs.is_empty() {
                    continue; // identical row; no alert
                }
                let summary = format!(
                    "The row in snapshot {new_snapshot_id} (digest {new_snapshot_digest}) differs from the row observed in snapshot {old_snapshot_id} (digest {old_snapshot_digest})."
                );
                let change = ProcurementRecordChange {
                    change_kind: ProcurementChangeKind::RecordModified,
                    row_identity: identity.clone(),
                    field_diffs: diffs.clone(),
                    old_snapshot_id: old_snapshot_id.to_owned(),
                    old_snapshot_digest: old_snapshot_digest.to_owned(),
                    new_snapshot_id: new_snapshot_id.to_owned(),
                    new_snapshot_digest: new_snapshot_digest.to_owned(),
                    summary: summary.clone(),
                };
                alerts.push(alert_for(
                    source_id,
                    surface,
                    old_snapshot_id,
                    old_snapshot_digest,
                    new_snapshot_id,
                    new_snapshot_digest,
                    retrieved_at,
                    coverage_state,
                    matter_ids,
                    identifier_ids,
                    change,
                    summary,
                ));
            }
        }
    }
    for identity in new_map.keys() {
        if !old_map.contains_key(identity) {
            let summary = format!(
                "A row not present in snapshot {old_snapshot_id} (digest {old_snapshot_digest}) now appears in snapshot {new_snapshot_id} (digest {new_snapshot_digest})."
            );
            let change = ProcurementRecordChange {
                change_kind: ProcurementChangeKind::RecordAdded,
                row_identity: identity.clone(),
                field_diffs: Vec::new(),
                old_snapshot_id: old_snapshot_id.to_owned(),
                old_snapshot_digest: old_snapshot_digest.to_owned(),
                new_snapshot_id: new_snapshot_id.to_owned(),
                new_snapshot_digest: new_snapshot_digest.to_owned(),
                summary: summary.clone(),
            };
            alerts.push(alert_for(
                source_id,
                surface,
                old_snapshot_id,
                old_snapshot_digest,
                new_snapshot_id,
                new_snapshot_digest,
                retrieved_at,
                coverage_state,
                matter_ids,
                identifier_ids,
                change,
                summary,
            ));
        }
    }

    alerts.sort_by(|a, b| a.id.cmp(&b.id));
    alerts
}

/// Builds a single immutable alert with the phrasing-disciplined summary.
#[allow(clippy::too_many_arguments)]
fn alert_for(
    source_id: &str,
    surface: &str,
    old_snapshot_id: &str,
    old_snapshot_digest: &str,
    new_snapshot_id: &str,
    new_snapshot_digest: &str,
    retrieved_at: &str,
    coverage_state: CoverageState,
    matter_ids: &[String],
    identifier_ids: &[String],
    change: ProcurementRecordChange,
    summary: String,
) -> ProcurementAlert {
    ProcurementAlert {
        id: ProcurementAlert::id_for(
            source_id,
            surface,
            &change.row_identity,
            change.change_kind,
            old_snapshot_id,
            new_snapshot_id,
        ),
        source_id: source_id.to_owned(),
        surface: surface.to_owned(),
        old_snapshot_id: old_snapshot_id.to_owned(),
        old_snapshot_digest: old_snapshot_digest.to_owned(),
        new_snapshot_id: new_snapshot_id.to_owned(),
        new_snapshot_digest: new_snapshot_digest.to_owned(),
        changes: vec![change],
        retrieved_at: retrieved_at.to_owned(),
        coverage_state,
        matter_ids: matter_ids.to_vec(),
        identifier_ids: identifier_ids.to_vec(),
        taxonomy_matches: Vec::new(),
        summary,
    }
}

/// Writes change alerts for a new snapshot into the store, returning how many
/// were newly inserted. Idempotent: re-inserting the same alerts inserts
/// nothing new.
pub fn persist_change_alerts(
    store: &pnull_core::Store,
    alerts: &[ProcurementAlert],
) -> Result<usize, ChangeAlertError> {
    let mut inserted = 0usize;
    for alert in alerts {
        if store.insert_procurement_alert(alert)? {
            inserted += 1;
        }
    }
    Ok(inserted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pnull_core::{CoverageState, MoneyState, parse_money};
    use tempfile::tempdir;

    fn row(solicitation_id: &str, contractor: &str, amount: &str, project: &str) -> AwardRow {
        AwardRow {
            row_index: 0,
            solicitation_id: solicitation_id.to_owned(),
            project_name: project.to_owned(),
            contractor: contractor.to_owned(),
            raw_amount: amount.to_owned(),
            amount: parse_money(amount),
            start_date: "2026-01-01".to_owned(),
            notes: String::new(),
            authority: pnull_core::SourceAuthority::OfficialInformationalMirror,
            coverage_state: CoverageState::InformationalOnly,
            snapshot_digest: "d".to_owned(),
            normalized_solicitation_id: pnull_core::normalize_identifier(solicitation_id)
                .map(|(k, _)| k),
        }
    }

    const SRC: &str = "colorado-springs-contract-awards";
    const SURFACE: &str = "contract-award-table";

    fn alerts(old: &[AwardRow], new: &[AwardRow]) -> Vec<ProcurementAlert> {
        build_change_alerts(
            SRC,
            SURFACE,
            "snap:old",
            "olddigest",
            "snap:new",
            "newdigest",
            "2026-08-17T00:00:00Z",
            CoverageState::InformationalOnly,
            old,
            new,
            &[],
            &[],
        )
    }

    #[test]
    fn detects_added_row() {
        let old = vec![row("R26-023AB", "Acme", "$1.00", "Project A")];
        let new = vec![
            row("R26-023AB", "Acme", "$1.00", "Project A"),
            row("R24-T114JD", "Adarand", "$2.00", "Project B"),
        ];
        let alerts = alerts(&old, &new);
        assert_eq!(alerts.len(), 1);
        assert_eq!(
            alerts[0].changes[0].change_kind,
            ProcurementChangeKind::RecordAdded
        );
    }

    #[test]
    fn detects_edited_amount_with_field_diff() {
        let old = vec![row("R26-023AB", "Acme", "$1,000.00", "Project A")];
        let new = vec![row("R26-023AB", "Acme", "$2,500.00", "Project A")];
        let alerts = alerts(&old, &new);
        assert_eq!(alerts.len(), 1);
        let change = &alerts[0].changes[0];
        assert_eq!(change.change_kind, ProcurementChangeKind::RecordModified);
        assert_eq!(change.field_diffs.len(), 1);
        assert_eq!(change.field_diffs[0].field, "raw_amount");
        assert_eq!(change.field_diffs[0].old_raw, "$1,000.00");
        assert_eq!(change.field_diffs[0].new_raw, "$2,500.00");
    }

    #[test]
    fn detects_vendor_change_and_date_drift() {
        let old = vec![row("R26-023AB", "Acme", "$1,000.00", "Project A")];
        let new = vec![row("R26-023AB", "Optiv", "$1,000.00", "Project A")];
        let vendor_alerts = alerts(&old, &new);
        let diffs = &vendor_alerts[0].changes[0].field_diffs;
        assert!(diffs.iter().any(|d| d.field == "contractor"));
        // Date-format drift: change start date.
        let mut changed = row("R26-023AB", "Optiv", "$1,000.00", "Project A");
        changed.start_date = "February 1, 2026".to_owned();
        let date_alerts = alerts(&old, &[changed]);
        assert!(
            date_alerts[0].changes[0]
                .field_diffs
                .iter()
                .any(|d| d.field == "start_date")
        );
    }

    #[test]
    fn idiq_various_money_preserved_raw() {
        let old = vec![row(
            "R23-T119KK",
            "C&D Electric",
            "$0.00 IDIQ",
            "Traffic Signal",
        )];
        let new = vec![row(
            "R23-T119KK",
            "C&D Electric",
            "various",
            "Traffic Signal",
        )];
        let alerts = alerts(&old, &new);
        assert_eq!(alerts.len(), 1);
        let diff = &alerts[0].changes[0].field_diffs[0];
        assert_eq!(diff.old_raw, "$0.00 IDIQ");
        assert_eq!(diff.new_raw, "various");
        assert_eq!(diff.field, "raw_amount");
    }

    #[test]
    fn detects_removed_row_with_comparison_phrasing() {
        let old = vec![row("R26-023AB", "Acme", "$1.00", "Project A")];
        let new: Vec<AwardRow> = Vec::new();
        let alerts = alerts(&old, &new);
        assert_eq!(alerts.len(), 1);
        let change = &alerts[0].changes[0];
        assert_eq!(change.change_kind, ProcurementChangeKind::RecordRemoved);
        assert!(
            change
                .summary
                .contains("is not present in snapshot snap:new (digest newdigest)")
        );
    }

    #[test]
    fn byte_identical_rows_produce_no_alert() {
        let rows = vec![row("R26-023AB", "Acme", "$1.00", "Project A")];
        assert!(alerts(&rows, &rows).is_empty());
    }

    #[test]
    fn reordering_is_not_a_change() {
        let a = row("R26-023AB", "Acme", "$1.00", "Project A");
        let b = row("R24-T114JD", "Adarand", "$2.00", "Project B");
        // Same set, different order.
        assert!(alerts(&[a.clone(), b.clone()], &[b, a]).is_empty());
    }

    #[test]
    fn row_without_identifier_uses_stable_digest_key() {
        let mut a = row("", "Acme", "$1.00", "Project A");
        let mut b = row("", "Acme", "$1.00", "Project A");
        // Same content in a different position -> same digest identity.
        a.row_index = 0;
        b.row_index = 5;
        assert_eq!(row_identity(&a), row_identity(&b));
        // A different value -> different identity (treated as remove+add).
        let mut c = row("", "Acme", "$9.00", "Project A");
        c.row_index = 5;
        assert_ne!(row_identity(&a), row_identity(&c));
    }

    #[test]
    fn stable_alert_id_is_idempotent_for_same_pair() {
        let old = vec![row("R26-023AB", "Acme", "$1.00", "Project A")];
        let new = vec![row("R26-023AB", "Optiv", "$1.00", "Project A")];
        let first = alerts(&old, &new);
        let second = alerts(&old, &new);
        assert_eq!(first[0].id, second[0].id);
        assert_eq!(first.len(), 1);
    }

    #[test]
    fn hostile_values_do_not_break_alerts() {
        let mut old = row("R26-023AB", "Acme", "$1.00", "Project A");
        let mut new = row("R26-023AB", "Acme", "$1.00", "Project A");
        new.contractor = "ÅB̊Ĉ \u{202e} =SUM(A1:A2) 123-45-6789".to_owned();
        new.notes = "\"quoted\" & <tag> 999999999999999999999999".to_owned();
        let hostile = alerts(&[old.clone()], &[new.clone()]);
        assert_eq!(hostile.len(), 1);
        assert!(
            hostile[0].changes[0]
                .field_diffs
                .iter()
                .any(|d| d.field == "contractor")
        );
        old.notes = String::new();
        let _ = old;
    }

    #[test]
    fn persist_is_idempotent() {
        let dir = tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let old = vec![row("R26-023AB", "Acme", "$1.00", "Project A")];
        let new = vec![row("R26-023AB", "Optiv", "$1.00", "Project A")];
        let alerts = alerts(&old, &new);
        let first = persist_change_alerts(&store, &alerts).expect("first");
        let second = persist_change_alerts(&store, &alerts).expect("second");
        assert_eq!(first, alerts.len());
        assert_eq!(second, 0);
        assert_eq!(store.all_procurement_alerts().expect("list").len(), 1);
    }

    #[test]
    fn money_states_are_preserved_in_diff() {
        // $0.00 IDIQ is IdiqCeiling, not Zero; both raw forms are preserved.
        let old = vec![row("R23-T119KK", "C&D", "$0.00 IDIQ", "Signal")];
        let new = vec![row("R23-T119KK", "C&D", "$0.00 IDIQ", "Signal")];
        assert!(alerts(&old, &new).is_empty());
        // MoneyState remains parseable and non-float.
        assert_eq!(old[0].amount.state, MoneyState::IdiqCeiling);
    }
}
