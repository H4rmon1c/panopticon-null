//! CLI handlers for the v0.0.3 procurement chain.
//!
//! These commands operate offline on committed immutable fixture snapshots by
//! default. Network retrieval is never attempted by this module: the connectors
//! are deterministic parsers over preserved snapshots, and the coverage ledger
//! records the informational-mirror status of each source. Live mode is an
//! explicit opt-in that is refused unless a persistent source review exists; no
//! live procurement fetch path is wired in this milestone.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use pnull_core::{CoverageEntry, CoverageState, SourceAuthority, Store, sha256_hex};
use pnull_procurement::{
    Acquisition, OpenBookFinding, build_cora_draft, generate_case_file, import_supplied_record,
    parse_awards_table, parse_solicitations, record_snapshot,
};

/// Default fixture paths (committed, offline).
const DEFAULT_AWARDS_FIXTURE: &str = "fixtures/procurement/contract-awards.html";
const DEFAULT_SOLICITATIONS_FIXTURE: &str = "fixtures/procurement/solicitations.html";

/// The fixed retrieval timestamp for offline demonstrations (deterministic).
const OFFLINE_RETRIEVED_AT: &str = "2026-08-17T00:00:00Z";

/// Run `procurement ingest solicitations` (offline from a fixture snapshot).
pub fn ingest_solicitations(store: &Store, source_path: &str, live: bool) -> Result<()> {
    if live {
        require_review_for_live(store, "colorado-springs-solicitation-mirror")?;
    }
    let bytes = read_fixture(source_path, DEFAULT_SOLICITATIONS_FIXTURE)?;
    let digest = sha256_hex(&bytes);
    let html = String::from_utf8_lossy(&bytes);
    let records =
        parse_solicitations(&html, &digest).context("parse solicitation mirror snapshot")?;

    let acquisition = Acquisition {
        source_id: "colorado-springs-solicitation-mirror".to_owned(),
        source_url: "https://coloradosprings.gov/solicitations".to_owned(),
        retrieved_at: OFFLINE_RETRIEVED_AT.to_owned(),
        bytes_digest: digest.clone(),
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
    };
    let (sol_snapshot, _) = record_snapshot(
        store,
        &acquisition,
        pnull_procurement::latest_snapshot(store, &acquisition.source_id)?.as_ref(),
        Some(records.len() as u64),
        &records
            .iter()
            .map(pnull_procurement::SolicitationRecord::to_record_row)
            .collect::<Vec<_>>(),
    )?;
    let sol_evidence = vec![sol_snapshot.id.clone()];

    for record in &records {
        if let Some(matter_id) =
            pnull_procurement::attach_solicitation_record(store, record, &sol_evidence)?
        {
            println!("  -> attached to reachable matter {matter_id}");
        }
    }
    println!(
        "ingested {} solicitation record(s) from {} (digest {})",
        records.len(),
        source_path,
        digest
    );
    println!(
        "note: this is an informational mirror; it does not represent every solicitation. BidNet and Bonfire remain authoritative."
    );
    Ok(())
}

/// Run `procurement ingest awards` (offline from a fixture snapshot).
pub fn ingest_awards(store: &Store, source_path: &str, live: bool) -> Result<()> {
    if live {
        require_review_for_live(store, "colorado-springs-contract-awards")?;
    }
    let bytes = read_fixture(source_path, DEFAULT_AWARDS_FIXTURE)?;
    let digest = sha256_hex(&bytes);
    let html = String::from_utf8_lossy(&bytes);
    let rows = parse_awards_table(&html, &digest).context("parse contract-award snapshot")?;

    let acquisition = Acquisition {
        source_id: "colorado-springs-contract-awards".to_owned(),
        source_url:
            "https://coloradosprings.gov/procurement-services/page/contract-award-information"
                .to_owned(),
        retrieved_at: OFFLINE_RETRIEVED_AT.to_owned(),
        bytes_digest: digest.clone(),
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
    };
    let (award_snapshot, _) = record_snapshot(
        store,
        &acquisition,
        pnull_procurement::latest_snapshot(store, &acquisition.source_id)?.as_ref(),
        Some(rows.len() as u64),
        &rows
            .iter()
            .map(pnull_procurement::AwardRow::to_record_row)
            .collect::<Vec<_>>(),
    )?;
    let award_evidence = vec![award_snapshot.id.clone()];

    for row in &rows {
        let matter_id = pnull_procurement::attach_award_row(store, row, &award_evidence)?;
        println!("  -> attached to reachable matter {matter_id}");
    }
    println!(
        "ingested {} award row(s) from {} (digest {})",
        rows.len(),
        source_path,
        digest
    );
    Ok(())
}

/// Run `procurement export-awards` — write a formula-neutralized CSV of the
/// parsed contract-award rows for human review.
pub fn export_awards(store: &Store, source_path: &str, output: &Path) -> Result<()> {
    let bytes = read_fixture(source_path, DEFAULT_AWARDS_FIXTURE)?;
    let html = String::from_utf8_lossy(&bytes);
    let rows =
        parse_awards_table(&html, &sha256_hex(&bytes)).context("parse contract-award snapshot")?;
    let header = [
        "RFP/IFB Number",
        "Project Name",
        "Awarded Contractor",
        "Awarded Amount",
        "Contract Start Date",
        "Notes",
    ];
    let mut data = Vec::new();
    for row in &rows {
        data.push(vec![
            row.solicitation_id.clone(),
            row.project_name.clone(),
            row.contractor.clone(),
            row.raw_amount.clone(),
            row.start_date.clone(),
            row.notes.clone(),
        ]);
    }
    let csv = pnull_procurement::rows_to_csv(&header, &data).map_err(|e| anyhow!(e.to_string()))?;
    fs::write(output, csv)?;
    println!("wrote {} award row(s) to {}", rows.len(), output.display());
    println!("note: spreadsheet-formula injection is neutralized in the export.");
    let _ = store;
    Ok(())
}

/// Run `procurement ingest openbook` (offline documented negative finding).
pub fn ingest_openbook(store: &Store) -> Result<()> {
    let finding = OpenBookFinding::current();
    let entry = CoverageEntry {
        id: pnull_core::CoverageEntry::id_for("openbook-cos", OFFLINE_RETRIEVED_AT),
        source_id: "openbook-cos".to_owned(),
        source_url: pnull_procurement::OPENBOOK_LANDING_URL.to_owned(),
        authority: finding.authority,
        state: finding.coverage_state,
        retrieved_at: OFFLINE_RETRIEVED_AT.to_owned(),
        persisted_digest: Some(sha256_hex(finding.note.as_bytes())),
        http_status: None,
        etag: None,
        last_modified: None,
        final_url: Some(pnull_procurement::OPENBOOK_SOCRATA_URL.to_owned()),
        parser_version: Some("openbook-1.0".to_owned()),
        schema_version: Some(2),
        claimed_date_range: None,
        record_count: Some(finding.datasets.len() as u64),
        pagination_complete: Some(true),
        access_errors: Vec::new(),
        human_review_state: "unreviewed".to_owned(),
        note: finding.note.clone(),
    };
    store.insert_coverage_entry(&entry)?;
    println!("recorded OpenBook negative capability finding:");
    println!("{}", finding.note);
    Ok(())
}

/// Run `procurement import <path>` (operator-supplied public record).
pub fn import_record(
    data_dir: &Path,
    file_path: &str,
    source_or_request_id: &str,
    acquisition_date: &str,
    document_role: &str,
    operator: &str,
    declared_digest: &str,
) -> Result<()> {
    let declaration = pnull_procurement::SuppliedRecordDeclaration {
        source_or_request_id: source_or_request_id.to_owned(),
        acquisition_date: acquisition_date.to_owned(),
        document_role: document_role.to_owned(),
        lawful_possession: true,
        declared_digest: declared_digest.to_owned(),
        operator: operator.to_owned(),
    };
    let record = import_supplied_record(
        data_dir,
        Path::new(file_path),
        &declaration,
        50 * 1024 * 1024,
    )
    .map_err(|e| anyhow!(e.to_string()))?;
    println!(
        "imported supplied record {} ({} bytes, digest {})",
        record.id, record.byte_count, record.observed_digest
    );
    println!("status: unreviewed; human review required before publication.");
    Ok(())
}

/// Run `procurement show <matter>`.
pub fn show_matter(store: &Store, matter_id: &str) -> Result<()> {
    let matter = store.procurement_matter(matter_id)?;
    println!("matter {} ({})", matter.id, matter.title);
    println!("  jurisdiction: {}", matter.jurisdiction);
    println!(
        "  review: {} publication: {}",
        matter.review_state, matter.publication_state
    );
    let identifiers = store.procurement_identifiers(matter_id)?;
    for id in &identifiers {
        println!(
            "  identifier {} [{}] source {} normalized {:?}",
            id.raw,
            id.kind.label(),
            id.source_id,
            id.normalized
        );
    }
    let orgs = store.procurement_organizations(matter_id)?;
    for org in &orgs {
        println!(
            "  organization {} [{}] source {}",
            org.raw_name,
            org.role.label(),
            org.source_id
        );
    }
    let events = store.procurement_events(matter_id)?;
    for event in &events {
        println!(
            "  event {} [{}] {}",
            event.date.as_deref().unwrap_or("date unknown"),
            event.kind.label(),
            event.summary
        );
    }
    Ok(())
}

/// Run `procurement gaps <matter>` — print unresolved evidence gaps.
pub fn gaps(store: &Store, matter_id: &str) -> Result<()> {
    let _ = store.procurement_matter(matter_id)?;
    let items = store.all_reconciliation_items()?;
    let matter_items: Vec<_> = items.iter().filter(|i| i.matter_id == matter_id).collect();
    println!("reconciliation-review queue for {matter_id}:");
    if matter_items.is_empty() {
        println!("  no pending reconciliation items");
    }
    for item in &matter_items {
        let decision = store.current_reconciliation_decision(&item.id)?;
        println!(
            "  [{}] {} | {} | decided: {}",
            item.kind.label(),
            item.summary,
            item.state,
            decision.map_or("none".to_owned(), |d| d.decision.clone())
        );
    }
    println!("Gap phrasing: Not observed in the checked sources.");
    Ok(())
}

/// Run `procurement chain <matter>` — print the ordered chain, evidence-backed
/// links, and every unresolved gap.
pub fn chain(store: &Store, matter_id: &str) -> Result<()> {
    let view =
        pnull_procurement::build_chain(store, matter_id).map_err(|e| anyhow!(e.to_string()))?;
    print!("{}", pnull_procurement::render_chain(&view));
    Ok(())
}

/// Run `procurement reconcile <matter>` — manage the reconciliation-review queue.
///
/// With no decision flags, lists every pending item and its decision state. With
/// `--item`, `--decision`, `--operator`, and `--note`, records an immutable
/// auditable human decision on that item.
pub fn reconcile_matter(
    store: &Store,
    matter_id: &str,
    item_id: Option<&str>,
    decision: Option<&str>,
    operator: Option<&str>,
    note: Option<&str>,
    decided_at: &str,
) -> Result<()> {
    let _ = store.procurement_matter(matter_id)?;
    match (item_id, decision, operator, note) {
        (Some(item), Some(decision), Some(operator), Some(note)) => {
            pnull_procurement::record_decision(store, item, decision, operator, note, decided_at)
                .map_err(|e| anyhow!(e.to_string()))?;
            println!("recorded decision '{decision}' on {item} (operator {operator})");
            Ok(())
        }
        (None, None, None, None) => {
            let items = store.all_reconciliation_items()?;
            let matter_items: Vec<_> = items.iter().filter(|i| i.matter_id == matter_id).collect();
            println!("reconciliation-review queue for {matter_id}:");
            if matter_items.is_empty() {
                println!("  no pending reconciliation items");
            }
            for item in &matter_items {
                let current = store.current_reconciliation_decision(&item.id)?;
                println!(
                    "  {} | {} | state {} | decision {}",
                    item.id,
                    item.summary,
                    item.state,
                    current.map_or("none".to_owned(), |d| d.decision.clone())
                );
            }
            Ok(())
        }
        _ => bail!(
            "reconcile requires either no flags (to list the queue) or all of \
             --item --decision --operator --note (to record a decision)"
        ),
    }
}

/// Run `coverage show`.
pub fn coverage_show(store: &Store) -> Result<()> {
    let entries = store.all_coverage_entries()?;
    let mut by_source: std::collections::BTreeMap<String, Vec<CoverageEntry>> =
        std::collections::BTreeMap::new();
    for entry in &entries {
        by_source
            .entry(entry.source_id.clone())
            .or_default()
            .push(entry.clone());
    }
    for (source_id, source_entries) in &by_source {
        let summary = pnull_procurement::summarize(source_entries);
        println!(
            "{} | state {} | entries {} | latest {} | digest {}",
            source_id,
            summary.latest_state.label(),
            summary.entry_count,
            summary.latest_retrieved_at.as_deref().unwrap_or("none"),
            summary.latest_digest.as_deref().unwrap_or("none")
        );
    }
    Ok(())
}

/// Run `coverage diff <old> <new>` — print a snapshot record-level diff.
pub fn coverage_diff(store: &Store, old_snapshot: &str, new_snapshot: &str) -> Result<()> {
    let old = store.source_snapshot(old_snapshot)?;
    let new = store.source_snapshot(new_snapshot)?;
    if old.source_id != new.source_id {
        bail!(
            "snapshots are from different sources: {} vs {}",
            old.source_id,
            new.source_id
        );
    }
    // Load each snapshot's persisted deterministic record rows. A legacy
    // snapshot with no stored rows fails honestly rather than being diffed from
    // counts or digests as fake records.
    let old_rows = pnull_procurement::snapshot_rows(store, old_snapshot)?;
    let new_rows = pnull_procurement::snapshot_rows(store, new_snapshot)?;
    let diff = pnull_procurement::record_diff(
        old_snapshot,
        new_snapshot,
        &old.source_id,
        &old_rows,
        &new_rows,
    );
    if diff.changes.is_empty() {
        println!("no record-level changes between {old_snapshot} and {new_snapshot}");
    } else {
        for change in &diff.changes {
            println!("{}: {}", change.kind, change.summary);
        }
    }
    Ok(())
}

/// Run `case build <matter>`.
pub fn case_build(store: &Store, matter_id: &str, output_dir: &Path) -> Result<()> {
    let case_file = generate_case_file(store, matter_id, OFFLINE_RETRIEVED_AT)
        .map_err(|e| anyhow!(e.to_string()))?;
    let content =
        pnull_procurement::build_content(store, matter_id).map_err(|e| anyhow!(e.to_string()))?;
    fs::create_dir_all(output_dir)?;
    let json_path = output_dir.join("case-file.json");
    let md_path = output_dir.join("case-file.md");
    fs::write(&json_path, pnull_procurement::render_case_json(&content))?;
    fs::write(&md_path, pnull_procurement::render_case_markdown(&content))?;
    println!(
        "built case file {} (state {})",
        case_file.id,
        case_file.state.label()
    );
    println!("  json: {}", json_path.display());
    println!("  markdown: {}", md_path.display());
    println!("  manifest: {:?}", case_file.sha256_manifest);
    println!("note: remains a draft until citation review and publication allowlists pass.");
    Ok(())
}

/// Run `cora draft <matter>`.
pub fn cora_draft(store: &Store, matter_id: &str) -> Result<()> {
    let matter = store.procurement_matter(matter_id)?;
    let identifiers = store.procurement_identifiers(matter_id)?;
    let missing = vec![
        "executed contract",
        "award notice",
        "vendor-level expenditure evidence",
    ];
    let sources_checked = vec![
        "colorado-springs-contract-awards",
        "colorado-springs-solicitation-mirror",
        "openbook-cos",
    ];
    let draft = build_cora_draft(
        &matter,
        &identifiers,
        &missing,
        Some((Some("2026-01-01".to_owned()), Some("2026-08-17".to_owned()))),
        Some(&matter.title),
        &sources_checked,
    );
    store.insert_cora_draft(&draft)?;
    println!("{}", draft.markdown);
    println!("STATUS: local draft only; not submitted. Operator/legal review required.");
    Ok(())
}

fn read_fixture(source_path: &str, default: &str) -> Result<Vec<u8>> {
    let path = if source_path.trim().is_empty() {
        default
    } else {
        source_path
    };
    fs::read(path).with_context(|| format!("read fixture {path}"))
}

fn require_review_for_live(store: &Store, source_id: &str) -> Result<()> {
    match store.current_source_review(source_id)? {
        Some(review) if review.expires_at.as_str() >= OFFLINE_RETRIEVED_AT => {
            println!("live mode: approved source review present for {source_id}");
            Ok(())
        }
        _ => bail!(
            "live retrieval refused: no current persistent source review for {source_id}; \
             use the offline fixture path or record a source review first"
        ),
    }
}
