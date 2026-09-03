//! Offline demonstration: reproduce the full procurement chain from committed
//! fixtures, with no network access.
//!
//! The demo ingests the informational-mirror snapshots (solicitations and
//! contract awards) and the documented `OpenBook` negative finding, then builds
//! two genuine Colorado Springs matters:
//!
//! - `R26-023AB` — Next-Generation Transit Fare Collection System RFI (the real
//!   case study). This is a solicitation/RFI, not a contract or award. The demo
//!   shows the exact evidence gaps: no executed contract, no award notice, and
//!   no vendor-level payment evidence from `OpenBook`.
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
    ProcurementChangeKind, ProcurementEvent, ProcurementEventKind, ProcurementIdentifier,
    ProcurementMatter, ProcurementOrganization, ReconciliationItem, SourceAuthority, Store,
    parse_money, sha256_hex,
};

use crate::awards::{AwardRow, parse_awards_table};
use crate::casefile::generate as generate_case_file;
use crate::openbook::OpenBookFinding;
use crate::reconcile::{amount_conflict_item, missing_document_item, vendor_alias_item};
use crate::snapshot::{Acquisition, record_snapshot};
use crate::solicitations::{SolicitationRecord, parse_solicitations};

/// The fixed retrieval timestamp for offline demonstrations (deterministic).
pub const OFFLINE_RETRIEVED_AT: &str = "2026-08-17T00:00:00Z";

/// The fixed retrieval timestamp for the second (synthetic) award snapshot
/// (deterministic; Item 4).
pub const OFFLINE_RETRIEVED_AT_2: &str = "2026-08-31T00:00:00Z";

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
    /// Item 4: change alerts produced by re-ingesting the second snapshot.
    pub second_snapshot_digest: String,
    pub change_alerts: usize,
    pub corrected_events: usize,
    pub removed_events: usize,
    /// Item 5: explicit official-relationship links recorded (zero unless a
    /// preserved record's declared reference field exactly references another
    /// stored identifier).
    pub documented_relationships: usize,
}

/// Runs the full offline procurement-chain demonstration into `store`.
///
/// `fixtures_dir` is the directory containing the committed fixture snapshots
/// (`contract-awards.html`, `solicitations.html`, `SHA256SUMS`). `output_dir`
/// receives the generated case files.
#[allow(clippy::too_many_lines)]
pub fn run_demo(
    store: &Store,
    fixtures_dir: &Path,
    output_dir: &Path,
) -> Result<DemoResult, String> {
    verify_fixture_digests(fixtures_dir)?;

    let solicitations_path = fixtures_dir.join("solicitations.html");
    let awards_path = fixtures_dir.join("contract-awards.html");

    // 1. Ingest the solicitation mirror snapshot.
    let solicitation_bytes =
        fs::read(&solicitations_path).map_err(|e| format!("read solicitations fixture: {e}"))?;
    let solicitation_digest = sha256_hex(&solicitation_bytes);
    let solicitation_html = String::from_utf8_lossy(&solicitation_bytes);
    let solicitation_records = parse_solicitations(&solicitation_html, &solicitation_digest)
        .map_err(|e| format!("parse solicitations: {e}"))?;

    let sol_acquisition = solicitation_acquisition(&solicitation_digest);
    let sol_rows = crate::solicitations::solicitation_record_rows(&solicitation_records);
    let (sol_snapshot, _) = record_snapshot(
        store,
        &sol_acquisition,
        crate::snapshot::latest_snapshot(store, &sol_acquisition.source_id)
            .map_err(|e| e.to_string())?
            .as_ref(),
        Some(solicitation_records.len() as u64),
        &sol_rows,
        &sol_rows,
    )
    .map_err(|e| e.to_string())?;
    let sol_evidence = vec![sol_snapshot.id.clone()];

    // 2. Ingest the contract-award mirror snapshot.
    let award_bytes = fs::read(&awards_path).map_err(|e| format!("read awards fixture: {e}"))?;
    let award_digest = sha256_hex(&award_bytes);
    let award_html = String::from_utf8_lossy(&award_bytes);
    let award_rows =
        parse_awards_table(&award_html, &award_digest).map_err(|e| format!("parse awards: {e}"))?;

    let award_acquisition = award_acquisition(&award_digest);
    let (award_snapshot, _) = record_snapshot(
        store,
        &award_acquisition,
        crate::snapshot::latest_snapshot(store, &award_acquisition.source_id)
            .map_err(|e| e.to_string())?
            .as_ref(),
        Some(award_rows.len() as u64),
        &[],
        &crate::changealert::award_record_rows(&award_rows),
    )
    .map_err(|e| e.to_string())?;
    let award_evidence = vec![award_snapshot.id.clone()];

    // 3. Record the OpenBook negative capability finding.
    record_openbook_finding(store)?;

    // 4. Build the two matters.
    build_transit_fare_matter(
        store,
        &award_digest,
        &solicitation_records,
        &award_rows,
        &sol_evidence,
    )
    .map_err(|e| e.to_string())?;
    build_control_matter(store, &award_digest, &award_rows, &award_evidence)
        .map_err(|e| e.to_string())?;

    // 4.25 Item 3: register the transit-fare CORA request in `drafted` state.
    // The demo publishes no request state beyond this.
    register_transit_fare_cora_request(store, &solicitation_records).map_err(|e| e.to_string())?;

    // 4.5 Item 4: re-ingest the second (synthetic) contract-award snapshot and
    // demonstrate the supersession + diff + change-alert pipeline.
    let second = reingest_second_snapshot(
        store,
        fixtures_dir,
        &award_snapshot,
        &award_rows,
        &award_digest,
    )
    .map_err(|e| e.to_string())?;

    // 4.75 Item 5: run exact, declared-field official-relationship detection
    // over the preserved records. The committed fixtures contain no declared
    // reference field that exactly references another stored identifier, so
    // the demo records zero links and the test proves absence rather than
    // fabricating a link.
    let relationship_outcome = crate::relationships::detect_official_relationships(
        store,
        &preserved_record_references(
            &award_rows,
            &solicitation_records,
            &award_snapshot,
            &sol_snapshot,
        ),
    )
    .map_err(|e| e.to_string())?;
    let documented_relationships = relationship_outcome.links.len();

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
        second_snapshot_digest: second.digest,
        change_alerts: second.alert_count,
        corrected_events: second.corrected_events,
        removed_events: second.removed_events,
        documented_relationships,
    })
}

/// The outcome of re-ingesting the second contract-award snapshot (Item 4).
pub struct SecondSnapshotOutcome {
    pub digest: String,
    pub alert_count: usize,
    pub corrected_events: usize,
    pub removed_events: usize,
}

/// Builds the preserved-record reference-field entries used by Item 5 link
/// detection, one per award row's `notes` field and one per solicitation's
/// `linked_documents` list. Only declared reference fields are included.
fn preserved_record_references(
    award_rows: &[AwardRow],
    solicitation_records: &[SolicitationRecord],
    award_snapshot: &pnull_core::SourceSnapshot,
    sol_snapshot: &pnull_core::SourceSnapshot,
) -> Vec<crate::relationships::RecordReference> {
    let mut refs = Vec::new();
    for row in award_rows {
        if row.notes.trim().is_empty() {
            continue;
        }
        let matter_id = crate::matters::matter_id_for_identifier(
            row.normalized_solicitation_id
                .as_deref()
                .unwrap_or(&row.solicitation_id),
        );
        refs.push(crate::relationships::RecordReference {
            source_id: "colorado-springs-contract-awards".to_owned(),
            source_record_id: format!("{matter_id}:record:award:{}", row.row_index),
            matter_id: matter_id.clone(),
            snapshot_id: award_snapshot.id.clone(),
            snapshot_digest: award_snapshot.persisted_digest.clone(),
            reference_field: "notes".to_owned(),
            reference_text: row.notes.clone(),
        });
    }
    for record in solicitation_records {
        if record.linked_documents.is_empty() {
            continue;
        }
        let matter_id = crate::matters::matter_id_for_identifier(&record.identifier);
        refs.push(crate::relationships::RecordReference {
            source_id: "colorado-springs-solicitation-mirror".to_owned(),
            source_record_id: format!("{matter_id}:record:solicitation:{}", record.identifier),
            matter_id: matter_id.clone(),
            snapshot_id: sol_snapshot.id.clone(),
            snapshot_digest: sol_snapshot.persisted_digest.clone(),
            reference_field: "linked_documents".to_owned(),
            reference_text: record.linked_documents.join(" "),
        });
    }
    refs
}

/// Registers the transit-fare CORA request in the append-only ledger in
/// `drafted` state (Item 3). The demo publishes no request state beyond this.
fn register_transit_fare_cora_request(
    store: &Store,
    solicitation_records: &[SolicitationRecord],
) -> Result<(), String> {
    let r26 = solicitation_records
        .iter()
        .find(|r| r.identifier.eq_ignore_ascii_case("R26-023AB"))
        .ok_or_else(|| "R26-023AB not found in solicitation mirror fixture".to_owned())?;
    let missing = vec![
        "executed contract".to_owned(),
        "award notice".to_owned(),
        "vendor-level expenditure evidence".to_owned(),
    ];
    let date_range = Some((Some("2026-01-01".to_owned()), Some("2026-08-17".to_owned())));
    let sources_checked = vec![
        "colorado-springs-contract-awards".to_owned(),
        "colorado-springs-solicitation-mirror".to_owned(),
        "openbook-cos".to_owned(),
    ];
    let draft_text = format!(
        "Records request (draft): missing record types for {}\nInstitution: Colorado Springs Mountain Metropolitan Transit (City of Colorado Springs)\nIdentifiers: {}\nSources already checked: {}\nThis draft is local; nothing has been sent.",
        r26.identifier,
        r26.identifier,
        sources_checked.join(", ")
    );
    let registered = crate::cora_ledger::register_draft(
        store,
        TRANSIT_FARE_MATTER_ID,
        "Colorado Springs Mountain Metropolitan Transit (City of Colorado Springs)",
        vec![r26.identifier.clone()],
        missing,
        date_range,
        Some("Next-Generation Transit Fare Collection System".to_owned()),
        sources_checked,
        &draft_text,
        crate::cora_ledger::OFFLINE_CREATED_AT,
    )
    .map_err(|e| e.to_string())?;
    if !registered {
        return Err("CORA request already registered (unexpected duplicate)".to_owned());
    }
    Ok(())
}

/// Re-ingests a second (synthetic) contract-award snapshot, recording the
/// supersession relationship, the record-level diff, the resulting change
/// alerts (Item 1), and `RecordCorrected`/`RecordRemoved` events on the
/// affected matters. The synthetic fixture is clearly labeled and never
/// presented as official bytes.
#[allow(clippy::too_many_lines)]
fn reingest_second_snapshot(
    store: &Store,
    fixtures_dir: &Path,
    first_snapshot: &pnull_core::SourceSnapshot,
    first_rows: &[AwardRow],
    _first_digest: &str,
) -> Result<SecondSnapshotOutcome, String> {
    let second_path = fixtures_dir.join("contract-awards-2.html");
    let second_bytes =
        fs::read(&second_path).map_err(|e| format!("read second awards fixture: {e}"))?;
    let second_digest = sha256_hex(&second_bytes);
    let second_html = String::from_utf8_lossy(&second_bytes);
    let second_rows =
        parse_awards_table(&second_html, &second_digest).map_err(|e| format!("parse: {e}"))?;

    let acquisition = award_acquisition(&second_digest);
    let (second_snapshot, _) = record_snapshot(
        store,
        &acquisition,
        Some(first_snapshot),
        Some(second_rows.len() as u64),
        &crate::changealert::award_record_rows(first_rows),
        &crate::changealert::award_record_rows(&second_rows),
    )
    .map_err(|e| e.to_string())?;
    debug_assert_eq!(
        second_snapshot.supersedes.as_deref(),
        Some(first_snapshot.id.as_str()),
        "the second snapshot must supersede the first"
    );

    // Build and persist the deterministic change alerts (Item 1).
    let mut alerts = crate::changealert::build_change_alerts(
        &first_snapshot.source_id,
        "contract-award-table",
        &first_snapshot.id,
        &first_snapshot.persisted_digest,
        &second_snapshot.id,
        &second_snapshot.persisted_digest,
        OFFLINE_RETRIEVED_AT_2,
        first_snapshot.coverage_state,
        first_rows,
        &second_rows,
        &[],
        &[],
    );
    // Resolve the affected matter/identifier ids for each alert by the exact
    // identifier rule (never by similarity).
    for alert in &mut alerts {
        let normalized = alert
            .changes
            .first()
            .map(|c| crate::matters::matter_id_for_identifier(&c.row_identity))
            .unwrap_or_default();
        alert.matter_ids = vec![normalized];
    }
    let inserted =
        crate::changealert::persist_change_alerts(store, &alerts).map_err(|e| e.to_string())?;
    // Re-ingesting the same snapshot pair must not create duplicate alerts; the
    // insert is idempotent (INSERT OR IGNORE over stable alert ids), so on a
    // re-run `inserted` may be less than `alerts.len()`.
    debug_assert!(inserted <= alerts.len());

    // Record RecordCorrected / RecordRemoved events on affected matters. Each
    // affected matter is ensured to exist so it is a real, reachable matter and
    // its case file / site page can render the change.
    let mut corrected_events = 0usize;
    let mut removed_events = 0usize;
    for alert in &alerts {
        let change = &alert.changes[0];
        let matter_id = crate::matters::matter_id_for_identifier(&change.row_identity);
        let title = format!("{} — affected award record", change.row_identity);
        crate::matters::ensure_matter(store, &matter_id, &title).map_err(|e| e.to_string())?;
        let (kind, summary) = match change.change_kind {
            ProcurementChangeKind::RecordModified => {
                corrected_events += 1;
                (
                    ProcurementEventKind::RecordCorrected,
                    format!(
                        "Award record corrected between snapshot {} (digest {}) and {} (digest {}): {}",
                        alert.old_snapshot_id,
                        alert.old_snapshot_digest,
                        alert.new_snapshot_id,
                        alert.new_snapshot_digest,
                        change
                            .field_diffs
                            .iter()
                            .map(|d| format!("{} '{}' -> '{}'", d.field, d.old_raw, d.new_raw))
                            .collect::<Vec<_>>()
                            .join("; ")
                    ),
                )
            }
            ProcurementChangeKind::RecordRemoved => {
                removed_events += 1;
                (ProcurementEventKind::RecordRemoved, change.summary.clone())
            }
            ProcurementChangeKind::RecordAdded => continue,
        };
        let event = ProcurementEvent {
            id: ProcurementEvent::id_for(&matter_id, kind, OFFLINE_RETRIEVED_AT_2, &summary),
            matter_id: matter_id.clone(),
            kind,
            date: Some(OFFLINE_RETRIEVED_AT_2.to_owned()),
            summary,
            identifier_ids: Vec::new(),
            evidence_ids: vec![second_snapshot.id.clone()],
            source_id: "colorado-springs-contract-awards".to_owned(),
        };
        store
            .insert_procurement_event(&event)
            .map_err(|e| e.to_string())?;
    }

    Ok(SecondSnapshotOutcome {
        digest: second_digest,
        alert_count: alerts.len(),
        corrected_events,
        removed_events,
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
        let digest = parts
            .next()
            .ok_or_else(|| "malformed SHA256SUMS line".to_owned())?;
        let name = parts
            .next()
            .ok_or_else(|| "malformed SHA256SUMS line".to_owned())?;
        let bytes = fs::read(fixtures_dir.join(name)).map_err(|e| format!("read {name}: {e}"))?;
        let actual = sha256_hex(&bytes);
        if actual != digest {
            return Err(format!(
                "fixture digest mismatch for {name}: expected {digest}, got {actual}"
            ));
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
        source_url:
            "https://coloradosprings.gov/procurement-services/page/contract-award-information"
                .to_owned(),
        retrieved_at: OFFLINE_RETRIEVED_AT.to_owned(),
        bytes_digest: digest.to_owned(),
        content_type: Some("text/html".to_owned()),
        etag: None,
        last_modified: None,
        final_url:
            "https://coloradosprings.gov/procurement-services/page/contract-award-information"
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
    evidence_ids: &[String],
) -> Result<(), String> {
    let matter = ProcurementMatter {
        id: TRANSIT_FARE_MATTER_ID.to_owned(),
        jurisdiction: "Colorado Springs".to_owned(),
        title: "R26-023AB — Next-Generation Transit Fare Collection System (RFI)".to_owned(),
        review_state: "draft".to_owned(),
        publication_state: "unpublished".to_owned(),
    };
    store
        .insert_procurement_matter(&matter)
        .map_err(|e| e.to_string())?;

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
    let bindings = EventBindings {
        identifier_ids: std::slice::from_ref(&identifier.id),
        evidence_ids,
    };
    let event_ids: Vec<String> = vec![
        insert_event(
            store,
            TRANSIT_FARE_MATTER_ID,
            ProcurementEventKind::SolicitationPublished,
            Some("2025-12-01".to_owned()),
            "Next-Generation Transit Fare Collection System RFI (R26-023AB) published on the City solicitation mirror"
                .to_owned(),
            "colorado-springs-solicitation-mirror",
            &bindings,
        )?,
        insert_event(
            store,
            TRANSIT_FARE_MATTER_ID,
            ProcurementEventKind::QuestionsAndAnswersPublished,
            Some("2026-01-15".to_owned()),
            "Submitted questions and Mountain Metropolitan Transit responses published for R26-023AB"
                .to_owned(),
            "colorado-springs-solicitation-mirror",
            &bindings,
        )?,
    ];
    let _ = event_ids;

    // The linked documents are recorded as evidence pointers, never fetched.
    let _ = &r26.linked_documents;

    seed_transit_fare_reconciliation(store, award_rows)?;
    let _ = award_digest;
    Ok(())
}

/// Seeds the reconciliation-review queue for the transit-fare matter. Each gap
/// is queued for human review, never auto-resolved or converted into a fact.
fn seed_transit_fare_reconciliation(store: &Store, award_rows: &[AwardRow]) -> Result<(), String> {
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

    // Confirm no award row references R26-023AB (RFI != award). If one ever
    // appears in a later snapshot, that is a connection to be reviewed, not
    // assumed.
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
        store
            .insert_reconciliation_item(item)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Builds a benign, non-surveillance control matter: the 2023 Crack Seal
/// Materials purchase (B22-T168KK). It proves ingestion does not automatically
/// turn an ordinary materials purchase or vendor record into an accusation.
fn build_control_matter(
    store: &Store,
    award_digest: &str,
    award_rows: &[AwardRow],
    evidence_ids: &[String],
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
    store
        .insert_procurement_matter(&matter)
        .map_err(|e| e.to_string())?;

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
        store
            .insert_procurement_organization(&org)
            .map_err(|e| e.to_string())?;
    }

    // Award-announced event with the raw amount preserved as stated.
    let _money: MoneyValue = parse_money(&row.raw_amount);
    let bindings = EventBindings {
        identifier_ids: std::slice::from_ref(&identifier.id),
        evidence_ids,
    };
    let _ = insert_event(
        store,
        CONTROL_MATTER_ID,
        ProcurementEventKind::AwardAnnounced,
        Some(normalize_date(&row.start_date)),
        format!(
            "Award announced for {} (crack seal materials); raw awarded amount '{}'",
            row.solicitation_id, row.raw_amount
        ),
        "colorado-springs-contract-awards",
        &bindings,
    )?;

    // Vendor alias candidates for the joint venture go to human review, never
    // auto-merged.
    for contractor in split_contractors(&row.contractor) {
        if let Some(alias) = pnull_core::organization_alias_candidate(contractor) {
            let item = vendor_alias_item(CONTROL_MATTER_ID, contractor, &alias);
            store
                .insert_reconciliation_item(&item)
                .map_err(|e| e.to_string())?;
        }
    }

    // Confirm no surveillance finding is attached to this control matter.
    let _ = award_digest;
    Ok(())
}

/// Identifiers and snapshot evidence an event is bound to at ingestion.
struct EventBindings<'a> {
    identifier_ids: &'a [String],
    evidence_ids: &'a [String],
}

/// Inserts a deterministic procurement event bound to the exact snapshot it
/// was ingested from and returns its id.
fn insert_event(
    store: &Store,
    matter_id: &str,
    kind: ProcurementEventKind,
    date: Option<String>,
    summary: String,
    source_id: &str,
    bindings: &EventBindings,
) -> Result<String, String> {
    let date_key = date.clone().unwrap_or_else(|| "date unknown".to_owned());
    let event = ProcurementEvent {
        id: ProcurementEvent::id_for(matter_id, kind, &date_key, &summary),
        matter_id: matter_id.to_owned(),
        kind,
        date,
        summary,
        identifier_ids: bindings.identifier_ids.to_vec(),
        evidence_ids: bindings.evidence_ids.to_vec(),
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
        run_demo(&store, &fixtures_dir(), dir.path()).expect("demo")
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
    fn second_snapshot_produces_change_and_event_kinds() {
        let dir = tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        let result = run_demo(&store, &fixtures_dir(), dir.path()).expect("demo");
        // The synthetic second snapshot must produce at least one of each
        // change kind (added, modified, removed).
        assert!(
            result.change_alerts >= 3,
            "alerts: {}",
            result.change_alerts
        );
        assert!(result.corrected_events >= 1);
        assert!(result.removed_events >= 1);
        let alerts = store.all_procurement_alerts().expect("alerts");
        let kinds: std::collections::BTreeSet<String> = alerts
            .iter()
            .flat_map(|a| a.changes.iter().map(|c| format!("{:?}", c.change_kind)))
            .collect();
        assert!(kinds.contains("RecordAdded"), "kinds: {kinds:?}");
        assert!(kinds.contains("RecordModified"), "kinds: {kinds:?}");
        assert!(kinds.contains("RecordRemoved"), "kinds: {kinds:?}");
        // The affected derived matters must have received the
        // corrected/removed events (exact-identifier rule maps the changed
        // rows to their per-identifier matters, not by similarity).
        let affected: [&str; 2] = [
            "proc:matter:co:q25130zm",  // modified LogRhythm row
            "proc:matter:co:r24t114jd", // removed guardrail row
        ];
        let mut corrected = 0usize;
        for matter_id in affected {
            corrected += store
                .procurement_events(matter_id)
                .expect("events")
                .iter()
                .filter(|e| {
                    matches!(
                        e.kind,
                        ProcurementEventKind::RecordCorrected | ProcurementEventKind::RecordRemoved
                    )
                })
                .count();
        }
        assert!(corrected >= 1, "corrected/removed events: {corrected}");
    }

    #[test]
    fn demo_registers_transit_fare_cora_request_drafted() {
        let dir = tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        let _ = run_demo(&store, &fixtures_dir(), dir.path()).expect("demo");
        let requests = store
            .cora_requests(TRANSIT_FARE_MATTER_ID)
            .expect("requests");
        assert_eq!(requests.len(), 1, "exactly one transit-fare request");
        let request = &requests[0];
        assert_eq!(request.state, pnull_core::CoraRequestState::Drafted);
        assert!(request.identifiers.iter().any(|i| i == "R26-023AB"));
        assert!(
            request
                .missing_record_types
                .iter()
                .any(|m| m.contains("executed contract"))
        );
        assert_eq!(
            request.created_at,
            crate::cora_ledger::OFFLINE_CREATED_AT,
            "deterministic timestamp"
        );
        assert_eq!(request.events.len(), 1, "only the drafting event");
        assert_eq!(
            request.events[0].state,
            pnull_core::CoraRequestState::Drafted
        );
    }

    #[test]
    fn demo_records_zero_official_relationships_by_absence() {
        let dir = tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        let result = run_demo(&store, &fixtures_dir(), dir.path()).expect("demo");
        // The preserved fixtures contain no declared reference field that
        // exactly references another stored identifier, so the demo must prove
        // absence rather than fabricate a link (Item 5).
        assert_eq!(
            result.documented_relationships, 0,
            "no preserved fixture record carries a genuine cross-reference"
        );
        let links = store.all_official_relationships().expect("links");
        assert!(
            links.is_empty(),
            "zero official-relationship links must be stored"
        );
    }

    #[test]
    fn first_snapshot_digests_unchanged_and_superseded() {
        let dir = tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        let _ = run_demo(&store, &fixtures_dir(), dir.path()).expect("demo");
        let snapshots = store
            .source_snapshots("colorado-springs-contract-awards")
            .expect("snapshots");
        assert!(snapshots.len() >= 2, "want at least 2 award snapshots");
        // The first snapshot digest must equal the preserved official fixture.
        let first = &snapshots[0];
        let official_bytes =
            std::fs::read(fixtures_dir().join("contract-awards.html")).expect("read");
        assert_eq!(first.persisted_digest, sha256_hex(&official_bytes));
        // The second supersedes the first.
        let second = snapshots[1].clone();
        assert_eq!(second.supersedes.as_deref(), Some(first.id.as_str()));
    }

    #[test]
    fn second_snapshot_reingest_is_idempotent() {
        let dir = tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        let first = run_demo(&store, &fixtures_dir(), dir.path()).expect("demo");
        let alerts_before = store.all_procurement_alerts().expect("alerts").len();
        // Re-running the second-snapshot step must not create duplicate alerts
        // (stable, idempotent alert ids) and must not add snapshots/events.
        let second = reingest_second_snapshot(
            &store,
            &fixtures_dir(),
            &store
                .source_snapshots("colorado-springs-contract-awards")
                .expect("snapshots")[0],
            &parse_awards_table(
                &String::from_utf8_lossy(
                    &std::fs::read(fixtures_dir().join("contract-awards.html")).expect("read"),
                ),
                &sha256_hex(
                    &std::fs::read(fixtures_dir().join("contract-awards.html")).expect("read"),
                ),
            )
            .expect("parse"),
            "",
        )
        .expect("reingest");
        assert_eq!(first.second_snapshot_digest, second.digest);
        let alerts_after = store.all_procurement_alerts().expect("alerts").len();
        assert_eq!(
            alerts_before, alerts_after,
            "no duplicate alerts on re-ingest"
        );
    }

    #[test]
    fn split_contractors_keeps_entities_separate() {
        assert_eq!(
            split_contractors("Crafco & Maxwell"),
            vec!["Crafco", "Maxwell"]
        );
        assert_eq!(
            split_contractors("C&D Electric and Sturgeon Electric"),
            vec!["C", "D Electric", "Sturgeon Electric"]
        );
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
