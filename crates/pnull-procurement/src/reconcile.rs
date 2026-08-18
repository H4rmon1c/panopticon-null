//! Reconciliation rules and the human review queue.
//!
//! Connections between records may be created automatically only through:
//! - exact normalized identifiers (a deterministic rule + test),
//! - a relationship explicitly stated by an official source,
//! - an existing immutable relationship already supported by evidence.
//!
//! Connections are never created automatically through similar names, similar
//! titles, equal dollar amounts, close dates, keyword overlap, or LLM judgment.

use pnull_core::{
    ProcurementIdentifier, ReconciliationDecision, ReconciliationItem, ReconciliationKind,
    sha256_hex,
};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReconcileError {
    #[error("not an exact identifier match under any deterministic rule")]
    NotExactMatch,
    #[error("automatic connection requires a deterministic rule and its test")]
    NoRule,
}

/// Compares two procurement identifiers under the exact-match rule.
///
/// Returns `Ok(true)` only when a deterministic rule (with a test) proves the
/// two identifiers are the same. Any non-exact result returns `Err`, which the
/// caller must route to the reconciliation-review queue rather than auto-connect.
pub fn exact_identifier_match(
    left: &ProcurementIdentifier,
    right: &ProcurementIdentifier,
) -> Result<bool, ReconcileError> {
    match (&left.normalized, &right.normalized) {
        (Some(l), Some(r)) if l == r => Ok(true),
        _ => Err(ReconcileError::NotExactMatch),
    }
}

/// Builds a reconciliation-review item for a candidate identifier match.
///
/// The match is *not* created automatically; it is queued for human review.
pub fn candidate_identifier_item(
    matter_id: &str,
    left_raw: &str,
    right_raw: &str,
) -> ReconciliationItem {
    let summary = format!("candidate identifier match: {left_raw} vs {right_raw}");
    ReconciliationItem {
        id: ReconciliationItem::id_for(
            matter_id,
            ReconciliationKind::CandidateIdentifierMatch,
            &summary,
        ),
        matter_id: matter_id.to_owned(),
        kind: ReconciliationKind::CandidateIdentifierMatch,
        summary,
        record_refs: vec![left_raw.to_owned(), right_raw.to_owned()],
        state: "pending".to_owned(),
        created_at: "deterministic".to_owned(),
    }
}

/// Builds a reconciliation-review item for a vendor alias candidate.
pub fn vendor_alias_item(
    matter_id: &str,
    name_a: &str,
    name_b: &str,
) -> ReconciliationItem {
    let summary = format!("vendor alias candidate: {name_a} vs {name_b}");
    ReconciliationItem {
        id: ReconciliationItem::id_for(matter_id, ReconciliationKind::VendorAlias, &summary),
        matter_id: matter_id.to_owned(),
        kind: ReconciliationKind::VendorAlias,
        summary,
        record_refs: vec![name_a.to_owned(), name_b.to_owned()],
        state: "pending".to_owned(),
        created_at: "deterministic".to_owned(),
    }
}

/// Builds a reconciliation-review item for conflicting award amounts.
pub fn amount_conflict_item(
    matter_id: &str,
    record_key: &str,
    left_raw: &str,
    right_raw: &str,
) -> ReconciliationItem {
    let summary = format!("conflicting award amount for {record_key}: {left_raw} vs {right_raw}");
    ReconciliationItem {
        id: ReconciliationItem::id_for(
            matter_id,
            ReconciliationKind::ConflictingAwardAmount,
            &summary,
        ),
        matter_id: matter_id.to_owned(),
        kind: ReconciliationKind::ConflictingAwardAmount,
        summary,
        record_refs: vec![record_key.to_owned()],
        state: "pending".to_owned(),
        created_at: "deterministic".to_owned(),
    }
}

/// Builds a reconciliation-review item for a record that disappeared from a
/// later snapshot.
pub fn vanished_record_item(matter_id: &str, record_key: &str) -> ReconciliationItem {
    let summary = format!("record {record_key} vanished from a later snapshot");
    ReconciliationItem {
        id: ReconciliationItem::id_for(
            matter_id,
            ReconciliationKind::VanishedRecord,
            &summary,
        ),
        matter_id: matter_id.to_owned(),
        kind: ReconciliationKind::VanishedRecord,
        summary,
        record_refs: vec![record_key.to_owned()],
        state: "pending".to_owned(),
        created_at: "deterministic".to_owned(),
    }
}

/// Builds a reconciliation-review item for a missing expected document.
pub fn missing_document_item(matter_id: &str, description: &str) -> ReconciliationItem {
    let summary = format!("missing document: {description}");
    ReconciliationItem {
        id: ReconciliationItem::id_for(
            matter_id,
            ReconciliationKind::MissingDocument,
            &summary,
        ),
        matter_id: matter_id.to_owned(),
        kind: ReconciliationKind::MissingDocument,
        summary,
        record_refs: vec![description.to_owned()],
        state: "pending".to_owned(),
        created_at: "deterministic".to_owned(),
    }
}

/// Records an immutable, auditable human reconciliation decision.
pub fn record_decision(
    store: &pnull_core::Store,
    item_id: &str,
    decision: &str,
    operator: &str,
    note: &str,
    decided_at: &str,
) -> Result<ReconciliationDecision, pnull_core::CoreError> {
    let item = ReconciliationDecision {
        id: ReconciliationDecision::id_for(item_id, decided_at),
        item_id: item_id.to_owned(),
        decision: decision.to_owned(),
        operator: operator.to_owned(),
        note: note.to_owned(),
        decided_at: decided_at.to_owned(),
    };
    store.insert_reconciliation_decision(&item)?;
    Ok(item)
}

/// A stable digest binding a reconciliation item to its decision history.
pub fn reconciliation_binding_digest(item: &ReconciliationItem) -> String {
    sha256_hex(
        format!("{}\0{}\0{}", item.id, item.kind.label(), item.summary).as_bytes(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use pnull_core::{IdentifierKind, ProcurementIdentifier};

    fn id(matter_id: &str, raw: &str) -> ProcurementIdentifier {
        let (normalized, rule) = match pnull_core::normalize_identifier(raw) {
            Some((key, rule)) => (Some(key), Some(rule.to_owned())),
            None => (None, None),
        };
        ProcurementIdentifier {
            id: ProcurementIdentifier::id_for(matter_id, IdentifierKind::SolicitationNumber, raw),
            matter_id: matter_id.to_owned(),
            kind: IdentifierKind::SolicitationNumber,
            raw: raw.to_owned(),
            source_id: "src".to_owned(),
            normalized,
            normalization_rule: rule,
            known: false,
        }
    }

    #[test]
    fn exact_identifier_match_only_via_deterministic_rule() {
        let a = id("m", "R26-023AB");
        let b = id("m", "r26-023ab");
        assert_eq!(exact_identifier_match(&a, &b), Ok(true));
        // Differently formatted identifiers never auto-match.
        let c = id("m", "R26-023AC");
        assert_eq!(exact_identifier_match(&a, &c), Err(ReconcileError::NotExactMatch));
    }

    #[test]
    fn no_rule_means_no_auto_connection() {
        // An identifier without a normalized form cannot be auto-connected.
        let a = ProcurementIdentifier {
            normalized: None,
            normalization_rule: None,
            ..id("m", "R26-023AB")
        };
        let b = ProcurementIdentifier {
            normalized: None,
            normalization_rule: None,
            ..id("m", "R26-023AB")
        };
        assert_eq!(exact_identifier_match(&a, &b), Err(ReconcileError::NotExactMatch));
    }

    #[test]
    fn non_exact_matches_enter_the_review_queue() {
        let item = vendor_alias_item("m", "Crafco", "Crafco & Maxwell");
        assert_eq!(item.kind, ReconciliationKind::VendorAlias);
        assert_eq!(item.state, "pending");
        assert!(item.summary.contains("Crafco"));
    }

    #[test]
    fn decisions_are_recorded_immutably() {
        let dir = tempfile::tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        let item = vendor_alias_item("m", "A", "B");
        store.insert_reconciliation_item(&item).expect("insert");
        record_decision(&store, &item.id, "accept", "op", "confirmed alias", "2026-08-17T00:00:00Z")
            .expect("decision");
        let decisions = store.reconciliation_decisions(&item.id).expect("list");
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].decision, "accept");
    }

    #[test]
    fn amount_and_vanished_conflicts_are_queued_not_resolved() {
        let amount = amount_conflict_item("m", "R26-023AB", "$0.00 IDIQ", "$1,000.00");
        assert_eq!(amount.kind, ReconciliationKind::ConflictingAwardAmount);
        let vanished = vanished_record_item("m", "R21-T107KK");
        assert_eq!(vanished.kind, ReconciliationKind::VanishedRecord);
    }
}
