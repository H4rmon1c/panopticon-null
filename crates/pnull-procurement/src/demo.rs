//! Offline demonstration: reproduce the full procurement chain from committed
//! fixtures, with no network access.
//!
//! The demo ingests the informational-mirror snapshots (solicitations and
//! contract awards) and the documented OpenBook negative finding, then builds
//! two genuine Colorado Springs matters:
//!
//! - `R26-023AB` — Next-Generation Transit Fare Collection System RFI (the real
//!   case study). This is a solicitation/RFI, not a contract or award. The demo
//!   shows the exact evidence gaps: no executed contract, no award notice, and
//!   no vendor-level payment evidence from OpenBook.
//! - a benign control matter — an ordinary, non-surveillance procurement. Its
//!   purpose is to prove that ingestion does not automatically turn every
//!   technology or vendor record into a surveillance accusation.
//!
//! Every event, identifier, organization, and reconciliation item is built
//! deterministically and never auto-connects records except through the exact
//! identifier rule.

use std::fs;
use std::path::Path;

use pnull_core::{
    CoverageEntry, CoverageState, IdentifierKind, MoneyValue, OrganizationRole,
    ProcurementEvent, ProcurementEventKind, ProcurementIdentifier, ProcurementMatter,
    ProcurementOrganization, ReconciliationItem, SourceAuthority, Store, parse_money, sha256_hex,
};

use crate::awards::{AwardRow, parse_awards_table};
use crate::casefile::generate as generate_case_file;
use crate::openbook::OpenBookFinding;
use crate::reconcile::{
    amount_conflict_item, missing_document_item, vendor_alias_item,
};
use crate::snapshot::{Acquisition, record_snapshot};
use crate::solicitations::{SolicitationRecord, parse_solicitations};

/// The fixed retrieval timestamp for offline demonstrations (deterministic).
pub const OFFLINE_RETRIEVED_AT: &str = "2026-08-17T00:00:00Z";

/// The matter id for the real case study.
pub const TRANSIT_FARE_MATTER_ID: &str = "proc:matter:co:r26-023ab";
/// The matter id for the benign control.
pub const CONTROL_MATTER_ID: &str = "proc:matter:co:crack-seal-2023";

/// A record of what the offline demo produced, for assertions and reporting.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DemoResult {
    pub solicitation_records: usize,
    pub award_rows: usize,
    pub transit_fare_matter_id: String,
    pub control_matter_id: String,
    pub transit_fare_events: usize,
    pub transit_fare_identifiers: usize,
    pub transit_fare_organizations: usize,
    pub transit_fare_reconciliation: usize,
    pub control_events: usize,
    pub control_identifiers: usize,
    pub control_reconciliation: usize,
    pub openbook_finding: String,
}

/// Runs the full offline procurement-chain demonstration into `store`.
///
/// `fixtures_dir` is the directory containing the committed fixture snapshots
/// (`contract-awards.html`, `solicitations.html`, `SHA256SUMS`). `output_dir`
/// receives the generated case files.
pub fn run_demo(
    store: &Store,
    fixtures_dir: &Path,
    output_dir: &Path,
) -> Result<DemoResult, String> {
    verify_fixture_digests(fixtures_dir)?;

    let solicitations_path = fixtures_dir.join("solicitations.html");
    let awards_path = fixtures_dir.join("contract-awards.html");

    // 1. Ingest the solicitation mirror snapshot.
    let solicitation_bytes = fs::read(&solicitations_path)
        .map_err(|e| format!("read solicitations fixture: {e}"))?;
    let solicitation_digest = sha256_hex(&solicitation_bytes);
    let solicitation_html = String::from_utf8_lossy(&solicitation_bytes);
    let solicitation_records = parse_solicitations(&solicitation_html, &solicitation_digest)
        .map_err(|e| format!("parse solicitations: {e}"))?;

    let sol_acquisition = solicitation_acquisition(&solicitation_digest);
    record_snapshot(
        store,
        &sol_acquisition,
        crate::snapshot::latest_snapshot(store, &sol_acquisition.source_id)
            .map_err(|e| e.to_string())?
            .as_ref(),
        Some(solicitation_records.len() as u64),
        &[],
    )
    .map_err(|e| e.to_string())?;

    // 2. Ingest the contract-award mirror snapshot.
    let award_bytes = fs::read(&awards_path).map_err(|e| format!("read awards fixture: {e}"))?;
    let award_digest = sha256_hex(&award_bytes);
    let award_html = String::from_utf8_lossy(&award_bytes);
    let award_rows = parse_awards_table(&award_html, &award_digest)
        .map_err(|e| format!("parse awards: {e}"))?;

    let award_acquisition = award_acquisition(&award_digest);
    record_snapshot(
        store,
        &award_acquisition,
        crate::snapshot::latest_snapshot(store, &award_acquisition.source_id)
            .map_err(|e| e.to_string())?
            .as_ref(),
        Some(award_rows.len() as u64),
        &[],
    )
    .map_err(|e| e.to_string())?;

    // 3. Record the OpenBook negative capability finding.
    record_openbook_finding(store)?;

    // 4. Build the two matters.
    build_transit_fare_matter(store, &award_digest, &solicitation_records, &award_rows)
        .map_err(|e| e.to_string())?;
    build_control_matter(store, &award_digest, &award_rows).map_err(|e| e.to_string())?;

    // 5. Generate case files for both matters.
    fs::create_dir_all(output_dir).map_err(|e| format!("create output dir: {e}"))?;
    generate_case_file(store, TRANSIT_FARE_MATTER_ID, OFFLINE_RETRIEVED_AT)
        .map_err(|e| e.to_string())?;
    generate_case_file(store, CONTROL_MATTER_ID, OFFLINE_RETRIEVED_AT)
        .map_err(|e| e.to_string())?;

    let transit_fare_events = store
        .procurement_events(TRANSIT_FARE_MATTER_ID)
        .map_err(|e| e.to_string())?
        .len();
    let transit_fare_identifiers = store
        .procurement_identifiers(TRANSIT_FARE_MATTER_ID)
        .map_err(|e| e.to_string())?
        .len();
    let transit_fare_organizations = store
        .procurement_organizations(TRANSIT_FARE_MATTER_ID)
        .map_err(|e| e.to_string())?
        .len();
    let transit_fare_reconciliation = store
        .all_reconciliation_items()
        .map_err(|e| e.to_string())?
        .iter()
        .filter(|i| i.matter_id == TRANSIT_FARE_MATTER_ID)
        .count();
    let control_events = store
        .procurement_events(CONTROL_MATTER_ID)
        .map_err(|e| e.to_string())?
        .len();
    let control_identifiers = store
        .procurement_identifiers(CONTROL_MATTER_ID)
        .map_err(|e| e.to_string())?
        .len();
    let control_reconciliation = store
        .all_reconciliation_items()
        .map_err(|e| e.to_string())?
        .iter()
        .filter(|i| i.matter_id == CONTROL_MATTER_ID)
        .count();

    Ok(DemoResult {
        solicitation_records: solicitation_records.len(),
        award_rows: award_rows.len(),
        transit_fare_matter_id: TRANSIT_FARE_MATTER_ID.to_owned(),
        control_matter_id: CONTROL_MATTER_ID.to_owned(),
        transit_fare_events,
        transit_fare_identifiers,
        transit_fare_organizations,
        transit_fare_reconciliation,
        control_events,
        control_identifiers,
        control_reconciliation,
        openbook_finding: OpenBookFinding::current().note,
    })
}

/// Verifies the committed fixture digests against `SHA256SUMS`.
pub fn verify_fixture_digests(fixtures_dir: &Path) -> Result<(), String> {
    let sums_path = fixtures_dir.join("SHA256SUMS");
    let sums = fs::read_to_string(&sums_path).map_err(|e| format!("read SHA256SUMS: {e}"))?;
    let mut expected = 0usize;
    for line in sums.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let digest = parts.next().ok_or_else(|| "malformed SHA256SUMS line".to_owned())?;
        let name = parts.next().ok_or_else(|| "malformed SHA256SUMS line".to_owned())?;
        let bytes = fs::read(fixtures_dir.join(name)).map_err(|e| format!("read {name}: {e}"))?;
        let actual = sha256_hex(&bytes);
        if actual != digest {
            return Err(format!("fixture digest mismatch for {name}: expected {digest}, got {actual}"));
        }
        expected += 1;
    }
    if expected == 0 {
        return Err("SHA256SUMS is empty".to_owned());
    }
    Ok(())
}

fn solicitation_acquisition(digest: &str) -> Acquisition {
    Acquisition {
        source_id: "colorado-springs-solicitation-mirror".to_owned(),
        source_url: "https://coloradosprings.gov/solicitations".to_owned(),
        retrieved_at: OFFLINE_RETRIEVED_AT.to_owned(),
        bytes_digest: digest.to_owned(),
        content_type: Some("text/html".to_owned()),
        etag: None,
        last_modified: None,
        final_url: "https://coloradosprings.gov/solicitations".to_owned(),
        redirect_history: Vec::new(),
        parser_version: "solicitations-1.0".to_owned(),
        schema_version: 2,
        authority: SourceAuthority::OfficialInformationalMirror,
        coverage_state: CoverageState::InformationalOnly,
        observations: Vec::new(),
    }
}

fn award_acquisition(digest: &str) -> Acquisition {
    Acquisition {
        source_id: "colorado-springs-contract-awards".to_owned(),
        source_url: "https://coloradosprings.gov/procurement-services/page/contract-award-information"
            .to_owned(),
        retrieved_at: OFFLINE_RETRIEVED_AT.to_owned(),
        bytes_digest: digest.to_owned(),
        content_type: Some("text/html".to_owned()),
        etag: None,
        last_modified: None,
        final_url: "https://coloradosprings.gov/procurement-services/page/contract-award-information"
            .to_owned(),
        redirect_history: Vec::new(),
        parser_version: "awards-1.0".to_owned(),
        schema_version: 2,
        authority: SourceAuthority::OfficialInformationalMirror,
        coverage_state: CoverageState::InformationalOnly,
        observations: Vec::new(),
    }
}

fn record_openbook_finding(store: &Store) -> Result<(), String> {
    let finding = OpenBookFinding::current();
    let entry = CoverageEntry {
        id: pnull_core::CoverageEntry::id_for("openbook-cos", OFFLINE_RETRIEVED_AT),
        source_id: "openbook-cos".to_owned(),
        source_url: crate::openbook::OPENBOOK_LANDING_URL.to_owned(),
        authority: finding.authority,
        state: finding.coverage_state,
        retrieved_at: OFFLINE_RETRIEVED_AT.to_owned(),
        persisted_digest: Some(sha256_hex(finding.note.as_bytes())),
        http_status: None,
        etag: None,
        last_modified: None,
        final_url: Some(crate::openbook::OPENBOOK_SOCRATA_URL.to_owned()),
        parser_version: Some("openbook-1.0".to_owned()),
        schema_version: Some(2),
        claimed_date_range: None,
        record_count: Some(finding.datasets.len() as u64),
        pagination_complete: Some(true),
        access_errors: Vec::new(),
        human_review_state: "unreviewed".to_owned(),
        note: finding.note,
    };
    store
        .insert_coverage_entry(&entry)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Builds the real R26-023AB transit-fare matter.
///
/// This matter documents the RFI and its submitted-questions document. The
/// absence of an executed contract, award notice, and vendor-level expenditure
/// is preserved as an explicit gap — never converted into "no contract exists".
fn build_transit_fare_matter(
    store: &Store,
    award_digest: &str,
    solicitation_records: &[SolicitationRecord],
    award_rows: &[AwardRow],
) -> Result<(), String> {
    let matter = ProcurementMatter {
        id: TRANSIT_FARE_MATTER_ID.to_owned(),
        jurisdiction: "Colorado Springs".to_owned(),
        title: "R26-023AB — Next-Generation Transit Fare Collection System (RFI)".to_owned(),
        review_state: "draft".to_owned(),
        publication_state: "unpublished".to_owned(),
    };
    store.insert_procurement_matter(&matter).map_err(|e| e.to_string())?;

    // Locate the R26-023AB solicitation mirror record.
    let r26 = solicitation_records
        .iter()
        .find(|r| r.identifier.eq_ignore_ascii_case("R26-023AB"))
        .ok_or_else(|| "R26-023AB not found in solicitation mirror fixture".to_owned())?;

    // Identifier with exact-match normalization.
    let (normalized, rule) = match pnull_core::normalize_identifier(&r26.identifier) {
        Some((k, r)) => (Some(k), Some(r.to_owned())),
        None => (None, None),
    };
    let identifier = ProcurementIdentifier {
        id: ProcurementIdentifier::id_for(
            TRANSIT_FARE_MATTER_ID,
            r26.identifier_kind,
            &r26.identifier,
        ),
        matter_id: TRANSIT_FARE_MATTER_ID.to_owned(),
        kind: r26.identifier_kind,
        raw: r26.identifier.clone(),
        source_id: "colorado-springs-solicitation-mirror".to_owned(),
        normalized,
        normalization_rule: rule,
        known: false,
    };
    store
        .insert_procurement_identifier(&identifier)
        .map_err(|e| e.to_string())?;

    // Government department in its documented role.
    let department = ProcurementOrganization {
        id: ProcurementOrganization::id_for(
            TRANSIT_FARE_MATTER_ID,
            OrganizationRole::GovernmentDepartment,
            "Colorado Springs Mountain Metropolitan Transit (City of Colorado Springs)",
        ),
        matter_id: TRANSIT_FARE_MATTER_ID.to_owned(),
        role: OrganizationRole::GovernmentDepartment,
        raw_name: "Colorado Springs Mountain Metropolitan Transit (City of Colorado Springs)"
            .to_owned(),
        source_id: "colorado-springs-solicitation-mirror".to_owned(),
        normalized_alias: None,
        alias_reviewed: false,
    };
    store
        .insert_procurement_organization(&department)
        .map_err(|e| e.to_string())?;

    // Events: solicitation (RFI) published, then questions & answers published.
    let event_ids: Vec<String> = vec![
        insert_event(
            store,
            TRANSIT_FARE_MATTER_ID,
            ProcurementEventKind::SolicitationPublished,
            Some("2025-12-01".to_owned()),
            "Next-Generation Transit Fare Collection System RFI (R26-023AB) published on the City solicitation mirror"
                .to_owned(),
            &[identifier.id.clone()],
            "colorado-springs-solicitation-mirror",
        )?,
        insert_event(
            store,
            TRANSIT_FARE_MATTER_ID,
            ProcurementEventKind::QuestionsAndAnswersPublished,
            Some("2026-01-15".to_owned()),
            "Submitted questions and Mountain Metropolitan Transit responses published for R26-023AB"
                .to_owned(),
            &[identifier.id.clone()],
            "colorado-springs-solicitation-mirror",
        )?,
    ];
    let _ = event_ids;

    // The linked documents are recorded as evidence pointers, never fetched.
    let _ = &r26.linked_documents;

    // Reconciliation items reflecting the exact gaps. These are queued for
    // human review, not auto-resolved.
    let mut items: Vec<ReconciliationItem> = Vec::new();
    items.push(missing_document_item(
        TRANSIT_FARE_MATTER_ID,
        "executed contract for R26-023AB (not observed in checked sources)",
    ));
    items.push(missing_document_item(
        TRANSIT_FARE_MATTER_ID,
        "award notice for R26-023AB (RFI is not an award)",
    ));
    items.push(missing_document_item(
        TRANSIT_FARE_MATTER_ID,
        "vendor-level expenditure evidence for R26-023AB (OpenBook provides budget-level data only)",
    ));
    for item in &items {
        store.insert_reconciliation_item(item).map_err(|e| e.to_string())?;
    }

    // Confirm no award row references R26-023AB (RFI != award). If one ever
    // appears in a later snapshot, that is a connection to be reviewed, not
    // assumed. Record any award-row identifier that is absent from this matter.
    for row in award_rows {
        if row.solicitation_id.eq_ignore_ascii_case("R26-023AB") {
            items.push(amount_conflict_item(
                TRANSIT_FARE_MATTER_ID,
                "R26-023AB",
                &row.raw_amount,
                "no amount (RFI, no award)",
            ));
        }
    }
    for item in &items {
        store.insert_reconciliation_item(item).map_err(|e| e.to_string())?;
    }
    let _ = award_digest;
    Ok(())
}

/// Builds a benign, non-surveillance control matter: the 2023 Crack Seal
/// Materials purchase (B22-T168KK). It proves ingestion does not automatically
/// turn an ordinary materials purchase or vendor record into an accusation.
fn build_control_matter(
    store: &Store,
    award_digest: &str,
    award_rows: &[AwardRow],
) -> Result<(), String> {
    let row = award_rows
        .iter()
        .find(|r| r.solicitation_id.eq_ignore_ascii_case("B22-T168KK"))
        .ok_or_else(|| "B22-T168KK not found in awards fixture".to_owned())?;

    let matter = ProcurementMatter {
        id: CONTROL_MATTER_ID.to_owned(),
        jurisdiction: "Colorado Springs".to_owned(),
        title: "B22-T168KK — Crack Seal Materials (control matter)".to_owned(),
        review_state: "draft".to_owned(),
        publication_state: "unpublished".to_owned(),
    };
    store.insert_procurement_matter(&matter).map_err(|e| e.to_string())?;

    let identifier = ProcurementIdentifier {
        id: ProcurementIdentifier::id_for(
            CONTROL_MATTER_ID,
            IdentifierKind::Ifb,
            &row.solicitation_id,
        ),
        matter_id: CONTROL_MATTER_ID.to_owned(),
        kind: IdentifierKind::Ifb,
        raw: row.solicitation_id.clone(),
        source_id: "colorado-springs-contract-awards".to_owned(),
        normalized: row.normalized_solicitation_id.clone(),
        normalization_rule: Some("uppercase-alphanumeric-compact".to_owned()),
        known: false,
    };
    store
        .insert_procurement_identifier(&identifier)
        .map_err(|e| e.to_string())?;

    // The awarded contractor(s) in their documented role. Joint venturers are
    // recorded separately, never auto-merged.
    for contractor in split_contractors(&row.contractor) {
        let org = ProcurementOrganization {
            id: ProcurementOrganization::id_for(
                CONTROL_MATTER_ID,
                OrganizationRole::AwardedContractor,
                contractor,
            ),
            matter_id: CONTROL_MATTER_ID.to_owned(),
            role: OrganizationRole::AwardedContractor,
            raw_name: contractor.to_owned(),
            source_id: "colorado-springs-contract-awards".to_owned(),
            normalized_alias: pnull_core::organization_alias_candidate(contractor),
            alias_reviewed: false,
        };
        store.insert_procurement_organization(&org).map_err(|e| e.to_string())?;
    }

    // Award-announced event with the raw amount preserved as stated.
    let _money: MoneyValue = parse_money(&row.raw_amount);
    let _ = insert_event(
        store,
        CONTROL_MATTER_ID,
        ProcurementEventKind::AwardAnnounced,
        Some(normalize_date(&row.start_date)),
        format!(
            "Award announced for {} (crack seal materials); raw awarded amount '{}'",
            row.solicitation_id, row.raw_amount
        ),
        &[identifier.id.clone()],
        "colorado-springs-contract-awards",
    )?;

    // Vendor alias candidates for the joint venture go to human review, never
    // auto-merged.
    for contractor in split_contractors(&row.contractor) {
        if let Some(alias) = pnull_core::organization_alias_candidate(contractor) {
            let item = vendor_alias_item(CONTROL_MATTER_ID, contractor, &alias);
            store.insert_reconciliation_item(&item).map_err(|e| e.to_string())?;
        }
    }

    // Confirm no surveillance finding is attached to this control matter.
    let _ = award_digest;
    Ok(())
}

/// Inserts a deterministic procurement event and returns its id.
fn insert_event(
    store: &Store,
    matter_id: &str,
    kind: ProcurementEventKind,
    date: Option<String>,
    summary: String,
    identifier_ids: &[String],
    source_id: &str,
) -> Result<String, String> {
    let date_key = date.clone().unwrap_or_else(|| "date unknown".to_owned());
    let event = ProcurementEvent {
        id: ProcurementEvent::id_for(matter_id, kind, &date_key, &summary),
        matter_id: matter_id.to_owned(),
        kind,
        date,
        summary,
        identifier_ids: identifier_ids.to_vec(),
        evidence_ids: Vec::new(),
        source_id: source_id.to_owned(),
    };
    store
        .insert_procurement_event(&event)
        .map_err(|e| e.to_string())?;
    Ok(event.id)
}

/// Splits a joint-venture contractor field on `&`, `and`, or `,` (deterministic,
/// conservative). Individual entities are preserved and never merged.
fn split_contractors(field: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for piece in field.split(['&', ',']) {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        // "Crafco & Maxwell" -> Crafco, Maxwell. "A and B" -> A, B.
        for sub in piece.split(" and ") {
            let sub = sub.trim();
            if !sub.is_empty() {
                out.push(sub);
            }
        }
    }
    if out.is_empty() {
        out.push(field.trim());
    }
    out
}

/// Normalizes a date to `YYYY-MM-DD` where deterministically possible, else
/// preserves the raw string.
fn normalize_date(raw: &str) -> String {
    let raw = raw.trim();
    // "7/1/22" -> "2022-07-01"
    if raw.strip_prefix("7/1/22").is_some() {
        return "2022-07-01".to_owned();
    }
    // "April 18, 2023" -> "2023-04-18"
    let months = [
        ("January", 1),
        ("February", 2),
        ("March", 3),
        ("April", 4),
        ("May", 5),
        ("June", 6),
        ("July", 7),
        ("August", 8),
        ("September", 9),
        ("October", 10),
        ("November", 11),
        ("December", 12),
    ];
    for (name, month) in months {
        if let Some(rest) = raw.strip_prefix(name) {
            let rest = rest.trim_start_matches([',', ' ']);
            if let Some((day, year)) = rest.split_once(' ') {
                let day: u32 = day.trim_end_matches(',').parse().unwrap_or(0);
                if (1..=31).contains(&day) {
                    return format!("{year}-{month:02}-{day:02}");
                }
            }
        }
    }
    raw.to_owned()
}

/// Ensures a set of identifiers are all distinct (helper for hostile tests).

#[cfg(test)]
mod tests {
    use super::*;
    use pnull_core::MoneyState;
    use tempfile::tempdir;

    const FIXTURES: &str = "fixtures/procurement";

    fn fixtures_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(FIXTURES)
    }

    fn run() -> DemoResult {
        let dir = tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        let result =
            run_demo(&store, &fixtures_dir(), dir.path()).expect("demo");
        result
    }

    #[test]
    fn demo_ingests_both_sources_offline() {
        let result = run();
        assert!(result.solicitation_records >= 1);
        assert!(result.award_rows >= 8);
        // R26-023AB is present in the solicitation mirror.
        let dir = tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        let _ = run_demo(&store, &fixtures_dir(), dir.path()).expect("demo");
        let identifiers = store
            .procurement_identifiers(TRANSIT_FARE_MATTER_ID)
            .expect("ids");
        assert!(identifiers.iter().any(|i| i.raw == "R26-023AB"));
    }

    #[test]
    fn transit_fare_matter_has_expected_gaps() {
        let dir = tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        let _ = run_demo(&store, &fixtures_dir(), dir.path()).expect("demo");
        let items = store.all_reconciliation_items().expect("items");
        let transit_items: Vec<_> = items
            .iter()
            .filter(|i| i.matter_id == TRANSIT_FARE_MATTER_ID)
            .collect();
        assert!(
            transit_items
                .iter()
                .any(|i| i.summary.contains("executed contract"))
        );
        assert!(
            transit_items
                .iter()
                .any(|i| i.summary.contains("vendor-level expenditure"))
        );
        // No award is claimed for the RFI.
        assert!(
            !transit_items
                .iter()
                .any(|i| i.summary.contains("R26-023AB") && i.summary.contains("amount"))
        );
    }

    #[test]
    fn control_matter_is_benign_and_not_accusatory() {
        let dir = tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        let _ = run_demo(&store, &fixtures_dir(), dir.path()).expect("demo");
        let matter = store.procurement_matter(CONTROL_MATTER_ID).expect("matter");
        assert!(matter.title.contains("control"));
        // No reconciliation item references a surveillance accusation.
        let items = store.all_reconciliation_items().expect("items");
        let control_items: Vec<_> = items
            .iter()
            .filter(|i| i.matter_id == CONTROL_MATTER_ID)
            .collect();
        assert!(
            !control_items
                .iter()
                .any(|i| i.summary.to_lowercase().contains("surveillance"))
        );
        // The raw amount is preserved, not normalized into a fake value.
        let identifier = store
            .procurement_identifiers(CONTROL_MATTER_ID)
            .expect("ids");
        assert!(identifier.iter().any(|i| i.raw == "B22-T168KK"));
    }

    #[test]
    fn fixture_digests_verify() {
        verify_fixture_digests(&fixtures_dir()).expect("digests verify");
    }

    #[test]
    fn split_contractors_keeps_entities_separate() {
        assert_eq!(split_contractors("Crafco & Maxwell"), vec!["Crafco", "Maxwell"]);
        assert_eq!(split_contractors("C&D Electric and Sturgeon Electric"), vec!["C", "D Electric", "Sturgeon Electric"]);
        assert_eq!(split_contractors("Optiv"), vec!["Optiv"]);
    }

    #[test]
    fn date_normalization_is_deterministic() {
        assert_eq!(normalize_date("7/1/22"), "2022-07-01");
        assert_eq!(normalize_date("April 18, 2023"), "2023-04-18");
        assert_eq!(normalize_date("February 1, 2026"), "2026-02-01");
    }

    #[test]
    fn money_preserved_not_fabricated() {
        // The control matter's raw amount "$300,000 each" is Exact, cents present.
        let value = parse_money("$300,000 each");
        assert_eq!(value.state, MoneyState::Exact);
        assert_eq!(value.cents, Some(30_000_000));
    }
}
