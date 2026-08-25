//! Real, reachable matters for standalone ingestion.
//!
//! Standalone `procurement ingest` commands must never write records into a
//! hidden magic matter id that no operator can reach. Every ingested record is
//! attached to a real `ProcurementMatter` derived deterministically from its
//! identifier, so the record is always reachable through its matter.

use pnull_core::{
    IdentifierKind, MoneyValue, ProcurementEvent, ProcurementEventKind, ProcurementIdentifier,
    ProcurementMatter, ProcurementOrganization, Store, parse_money,
};
use thiserror::Error;

use crate::awards::AwardRow;
use crate::solicitations::SolicitationRecord;

/// The jurisdiction label used for derived ingestion matters.
pub const JURISDICTION: &str = "Colorado Springs";

#[derive(Debug, Error)]
pub enum MatterError {
    #[error("store operation failed: {0}")]
    Store(#[from] pnull_core::CoreError),
}

/// A deterministic, real matter id derived from a normalized identifier.
///
/// This is the matter a standalone ingest attaches its records to; it is not a
/// hidden id, and `procurement show <this-id>` resolves it like any other.
pub fn matter_id_for_identifier(normalized: &str) -> String {
    format!("proc:matter:co:{}", normalized.to_ascii_lowercase())
}

/// Builds the identifier attached to a derived ingestion matter.
pub fn identifier_for(
    matter_id: &str,
    kind: IdentifierKind,
    raw: &str,
    source_id: &str,
) -> ProcurementIdentifier {
    let (normalized, rule) = match pnull_core::normalize_identifier(raw) {
        Some((k, r)) => (Some(k), Some(r.to_owned())),
        None => (None, None),
    };
    ProcurementIdentifier {
        id: ProcurementIdentifier::id_for(matter_id, kind, raw),
        matter_id: matter_id.to_owned(),
        kind,
        raw: raw.to_owned(),
        source_id: source_id.to_owned(),
        normalized,
        normalization_rule: rule,
        known: false,
    }
}

/// Ensures a real, reachable matter exists, returning its id.
pub fn ensure_matter(store: &Store, matter_id: &str, title: &str) -> Result<(), MatterError> {
    let matter = ProcurementMatter {
        id: matter_id.to_owned(),
        jurisdiction: JURISDICTION.to_owned(),
        title: title.to_owned(),
        review_state: "draft".to_owned(),
        publication_state: "unpublished".to_owned(),
    };
    store.insert_procurement_matter(&matter)?;
    Ok(())
}

/// Inserts an award-stage event bound to an identifier.
pub fn insert_award_event(
    store: &Store,
    matter_id: &str,
    identifier_id: &str,
    source_id: &str,
    start_date: &str,
    summary: &str,
) -> Result<(), MatterError> {
    let date_key = if start_date.trim().is_empty() {
        "date unknown".to_owned()
    } else {
        start_date.to_owned()
    };
    let event = ProcurementEvent {
        id: ProcurementEvent::id_for(
            matter_id,
            ProcurementEventKind::AwardAnnounced,
            &date_key,
            summary,
        ),
        matter_id: matter_id.to_owned(),
        kind: ProcurementEventKind::AwardAnnounced,
        date: Some(start_date.to_owned()),
        summary: summary.to_owned(),
        identifier_ids: vec![identifier_id.to_owned()],
        evidence_ids: Vec::new(),
        source_id: source_id.to_owned(),
    };
    store.insert_procurement_event(&event)?;
    Ok(())
}

/// Inserts a solicitation-stage event bound to an identifier.
pub fn insert_solicitation_event(
    store: &Store,
    matter_id: &str,
    identifier_id: &str,
    source_id: &str,
    summary: &str,
) -> Result<(), MatterError> {
    let date_key = "date unknown".to_owned();
    let event = ProcurementEvent {
        id: ProcurementEvent::id_for(
            matter_id,
            ProcurementEventKind::SolicitationPublished,
            &date_key,
            summary,
        ),
        matter_id: matter_id.to_owned(),
        kind: ProcurementEventKind::SolicitationPublished,
        date: None,
        summary: summary.to_owned(),
        identifier_ids: vec![identifier_id.to_owned()],
        evidence_ids: Vec::new(),
        source_id: source_id.to_owned(),
    };
    store.insert_procurement_event(&event)?;
    Ok(())
}

/// Inserts an awarded-contractor organization bound to a matter.
pub fn insert_awarded_contractor(
    store: &Store,
    matter_id: &str,
    contractor: &str,
    source_id: &str,
) -> Result<(), MatterError> {
    if contractor.trim().is_empty() {
        return Ok(());
    }
    let org = ProcurementOrganization {
        id: ProcurementOrganization::id_for(
            matter_id,
            pnull_core::OrganizationRole::AwardedContractor,
            contractor,
        ),
        matter_id: matter_id.to_owned(),
        role: pnull_core::OrganizationRole::AwardedContractor,
        raw_name: contractor.to_owned(),
        source_id: source_id.to_owned(),
        normalized_alias: pnull_core::organization_alias_candidate(contractor),
        alias_reviewed: false,
    };
    store.insert_procurement_organization(&org)?;
    Ok(())
}

/// Parses an award row's raw amount (for deterministic summaries).
pub fn award_amount(raw: &str) -> MoneyValue {
    parse_money(raw)
}

/// Attaches an award row to a real, reachable matter derived from its
/// solicitation identifier. Returns the matter id used.
pub fn attach_award_row(store: &Store, row: &AwardRow) -> Result<String, MatterError> {
    let normalized = row.normalized_solicitation_id.clone().unwrap_or_else(|| {
        pnull_core::normalize_identifier(&row.solicitation_id)
            .map_or_else(|| row.solicitation_id.clone(), |(k, _)| k)
    });
    let matter_id = matter_id_for_identifier(&normalized);
    let title = if row.project_name.trim().is_empty() {
        format!("{} — ingested award record", row.solicitation_id)
    } else {
        format!(
            "{} — {} (ingested award record)",
            row.solicitation_id, row.project_name
        )
    };
    ensure_matter(store, &matter_id, &title)?;

    let identifier = identifier_for(
        &matter_id,
        classify_award_identifier(&row.solicitation_id),
        &row.solicitation_id,
        "colorado-springs-contract-awards",
    );
    store.insert_procurement_identifier(&identifier)?;

    if !row.contractor.trim().is_empty() {
        insert_awarded_contractor(
            store,
            &matter_id,
            &row.contractor,
            "colorado-springs-contract-awards",
        )?;
    }

    insert_award_event(
        store,
        &matter_id,
        &identifier.id,
        "colorado-springs-contract-awards",
        &row.start_date,
        &format!(
            "Award announced for {} ({}); raw awarded amount '{}'",
            row.solicitation_id, row.project_name, row.raw_amount
        ),
    )?;
    Ok(matter_id)
}

/// Attaches a solicitation record to a real, reachable matter. Returns the
/// matter id used, or `None` when the record has no identifier to key on.
pub fn attach_solicitation_record(
    store: &Store,
    record: &SolicitationRecord,
) -> Result<Option<String>, MatterError> {
    if record.identifier.trim().is_empty() {
        return Ok(None);
    }
    let (normalized, _) = match pnull_core::normalize_identifier(&record.identifier) {
        Some((k, r)) => (Some(k), Some(r.to_owned())),
        None => (None, None),
    };
    let Some(normalized) = normalized else {
        return Ok(None);
    };
    let matter_id = matter_id_for_identifier(&normalized);
    let title = if record.title.trim().is_empty() {
        format!("{} — ingested solicitation record", record.identifier)
    } else {
        format!(
            "{} — {} (ingested solicitation record)",
            record.identifier, record.title
        )
    };
    ensure_matter(store, &matter_id, &title)?;

    let identifier = identifier_for(
        &matter_id,
        record.identifier_kind,
        &record.identifier,
        "colorado-springs-solicitation-mirror",
    );
    store.insert_procurement_identifier(&identifier)?;

    insert_solicitation_event(
        store,
        &matter_id,
        &identifier.id,
        "colorado-springs-solicitation-mirror",
        &format!("Solicitation published: {}", record.title),
    )?;
    Ok(Some(matter_id))
}

/// Classifies a raw award solicitation id by prefix.
fn classify_award_identifier(raw: &str) -> IdentifierKind {
    let upper = raw.to_ascii_uppercase();
    if upper.starts_with("RFP") {
        IdentifierKind::Rfp
    } else if upper.starts_with("RFQ") {
        IdentifierKind::Rfq
    } else if upper.starts_with("IFB") || upper.starts_with('B') {
        IdentifierKind::Ifb
    } else if upper.starts_with('R') || upper.starts_with('Q') {
        IdentifierKind::SolicitationNumber
    } else {
        IdentifierKind::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn award_row() -> AwardRow {
        AwardRow {
            row_index: 0,
            solicitation_id: "B22-T168KK".to_owned(),
            project_name: "Crack Seal Materials".to_owned(),
            contractor: "Crafco & Maxwell".to_owned(),
            raw_amount: "$300,000 each".to_owned(),
            amount: parse_money("$300,000 each"),
            start_date: "April 18, 2023".to_owned(),
            notes: String::new(),
            authority: pnull_core::SourceAuthority::OfficialInformationalMirror,
            coverage_state: pnull_core::CoverageState::InformationalOnly,
            snapshot_digest: "d41d8cd98f00b204e9800998ecf8427e".to_owned(),
            normalized_solicitation_id: Some("B22T168KK".to_owned()),
        }
    }

    #[test]
    fn derived_matter_is_real_and_reachable() {
        let dir = tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        let row = award_row();
        let matter_id = attach_award_row(&store, &row).expect("attach");
        assert_eq!(matter_id, "proc:matter:co:b22t168kk");
        // The matter resolves like any other (no hidden id).
        let matter = store.procurement_matter(&matter_id).expect("matter");
        assert!(matter.title.contains("Crack Seal"));
        // The identifier and award event are reachable through the matter.
        let ids = store.procurement_identifiers(&matter_id).expect("ids");
        assert!(ids.iter().any(|i| i.raw == "B22-T168KK"));
        let events = store.procurement_events(&matter_id).expect("events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, ProcurementEventKind::AwardAnnounced);
    }

    #[test]
    fn solicitation_record_attaches_to_reachable_matter() {
        let dir = tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        let record = SolicitationRecord {
            title: "Snow Removal Services".to_owned(),
            identifier: "R25-301AB".to_owned(),
            identifier_kind: IdentifierKind::SolicitationNumber,
            linked_documents: Vec::new(),
            authority: pnull_core::SourceAuthority::OfficialInformationalMirror,
            coverage_state: pnull_core::CoverageState::InformationalOnly,
            incompleteness_warning: "warn".to_owned(),
            snapshot_digest: "d41d8cd98f00b204e9800998ecf8427e".to_owned(),
        };
        let matter_id = attach_solicitation_record(&store, &record)
            .expect("attach")
            .expect("matter");
        assert_eq!(matter_id, "proc:matter:co:r25301ab");
        let events = store.procurement_events(&matter_id).expect("events");
        assert_eq!(events[0].kind, ProcurementEventKind::SolicitationPublished);
    }
}
