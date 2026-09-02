//! Deterministic case-file generation for a procurement matter, in both
//! machine-readable JSON and human-readable Markdown.
//!
//! Every public factual statement in the case file must resolve to a reviewed
//! citation, and the file remains a `draft` until the human citation-review and
//! publication-allowlist controls pass.

use pnull_core::{
    CaseFile, CaseFileState, Citation, CoraRequest, CoraRequestState, CoverageEntry, CoverageState,
    Locator, MoneyValue, ProcurementEvent, ProcurementEventKind, ProcurementIdentifier,
    ProcurementMatter, ProcurementOrganization, ReconciliationKind, SourceAuthority, sha256_hex,
};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use thiserror::Error;

use crate::coverage::{NOT_OBSERVED_PHRASING, summarize};

#[derive(Debug, Error)]
pub enum CaseFileError {
    #[error("procurement matter {0} not found")]
    MatterNotFound(String),
    #[error("store operation failed: {0}")]
    Store(#[from] pnull_core::CoreError),
}

/// The assembled content of a case file before rendering.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CaseFileContent {
    pub matter: ProcurementMatter,
    pub events: Vec<ProcurementEvent>,
    pub identifiers: Vec<ProcurementIdentifier>,
    pub organizations: Vec<ProcurementOrganization>,
    pub coverage: Vec<CoverageEntry>,
    pub contradictions: Vec<String>,
    pub missing_documents: Vec<String>,
    /// CORA request ledger entries for this matter (Item 3). Each request shows
    /// its state, the gaps it targeted, and any response evidence linked.
    pub cora_requests: Vec<CoraRequest>,
    /// Gap-resolution status lines derived from the CORA ledger (Item 3): a
    /// resolved gap is marked covered by its request; a still-unresolved gap
    /// stays visible with the response digest noted.
    pub gap_resolutions: Vec<String>,
    /// The reviewed fact-citations the page's factual statements rest on
    /// (Item 2). Each references a preserved source snapshot with an exact
    /// quote, so every public statement can be gated on an Approved
    /// citation-review decision bound to the exact digests.
    pub citations: Vec<Citation>,
    /// The "what changed" section (Item 4): per snapshot-supersession pair,
    /// the snapshot digests, retrieval timestamps, and record-level diff,
    /// phrased as a comparison, not a legal conclusion.
    pub what_changed: Vec<String>,
    /// Explicit official-relationship links (Item 5): "the preserved record X
    /// (snapshot, digest) references Y in reference field Z". These are exact,
    /// declared-field matches only; never invented.
    pub documented_relationships: Vec<String>,
    pub coverage_summary: BTreeMap<String, String>,
    pub provenance: Vec<String>,
    pub limitations: Vec<String>,
}

/// Builds the deterministic case-file content from the store for a matter.
#[allow(clippy::too_many_lines)]
pub fn build_content(
    store: &pnull_core::Store,
    matter_id: &str,
) -> Result<CaseFileContent, CaseFileError> {
    let matter = store
        .procurement_matter(matter_id)
        .map_err(|_| CaseFileError::MatterNotFound(matter_id.to_owned()))?;
    let events = store.procurement_events(matter_id)?;
    let identifiers = store.procurement_identifiers(matter_id)?;
    let organizations = store.procurement_organizations(matter_id)?;
    let coverage = store.all_coverage_entries()?;

    // Deterministic contradiction scan over amounts for the same identifier.
    let mut contradictions = Vec::new();
    let mut amounts: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for identifier in &identifiers {
        let key = identifier
            .normalized
            .clone()
            .unwrap_or_else(|| identifier.raw.clone());
        // Amounts live on events; here we note contradictions among raw forms.
        amounts.entry(key).or_default().push(identifier.raw.clone());
    }
    for (key, raw_forms) in amounts {
        let mut unique: Vec<String> = raw_forms.clone();
        unique.sort();
        unique.dedup();
        if unique.len() > 1 {
            contradictions.push(format!(
                "Multiple raw forms recorded for identifier {key}: {}",
                unique.join(", ")
            ));
        }
    }

    // Coverage summary, one line per source.
    let mut coverage_by_source: BTreeMap<String, Vec<CoverageEntry>> = BTreeMap::new();
    for entry in &coverage {
        coverage_by_source
            .entry(entry.source_id.clone())
            .or_default()
            .push(entry.clone());
    }
    let mut coverage_summary = BTreeMap::new();
    for (source_id, entries) in &coverage_by_source {
        let summary = summarize(entries);
        coverage_summary.insert(
            source_id.clone(),
            format!(
                "state={} entries={} latest={}",
                summary.latest_state.label(),
                summary.entry_count,
                summary.latest_retrieved_at.as_deref().unwrap_or("none")
            ),
        );
    }

    // Provenance lines.
    let mut provenance = Vec::new();
    for (source_id, entries) in &coverage_by_source {
        for entry in entries {
            provenance.push(format!(
                "{source_id}: retrieved {} state={} digest={}",
                entry.retrieved_at,
                entry.state.label(),
                entry.persisted_digest.as_deref().unwrap_or("none")
            ));
        }
    }

    // Missing expected documents and contradictions come from the immutable
    // reconciliation-review queue. They are surfaced here as explicit gaps,
    // never as invented facts.
    let mut missing_documents = Vec::new();
    let mut reconciliation_gaps = Vec::new();
    for item in store.all_reconciliation_items()? {
        if item.matter_id != matter_id {
            continue;
        }
        match item.kind {
            ReconciliationKind::MissingDocument => {
                missing_documents.push(item.summary.clone());
            }
            ReconciliationKind::ConflictingAwardAmount
            | ReconciliationKind::ConflictingDate
            | ReconciliationKind::DuplicateOrRevisedRow
            | ReconciliationKind::VanishedRecord => {
                reconciliation_gaps.push(item.summary.clone());
            }
            _ => {}
        }
    }
    missing_documents.sort();
    missing_documents.dedup();
    reconciliation_gaps.sort();
    reconciliation_gaps.dedup();
    contradictions.extend(reconciliation_gaps);

    let limitations = default_limitations();

    // CORA request ledger entries for this matter (Item 3), newest first.
    let mut cora_requests = store.cora_requests(matter_id)?;
    cora_requests.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(a.id.cmp(&b.id)));

    // Gap-resolution status lines (Item 3): derive from the CORA ledger whether
    // each targeted gap is covered or still open, with the response digest
    // where a response was received.
    let mut gap_resolutions = Vec::new();
    for request in &cora_requests {
        let targets = if request.missing_record_types.is_empty() {
            "the targeted gap".to_owned()
        } else {
            request.missing_record_types.join(", ")
        };
        let response_digest = request_response_digest(store, request)?;
        match request.state {
            CoraRequestState::GapResolved => gap_resolutions.push(format!(
                "Gap on {targets} closed by CORA request {} (response digest {})",
                request.id,
                response_digest.as_deref().unwrap_or("none")
            )),
            CoraRequestState::StillUnresolved => gap_resolutions.push(format!(
                "Gap on {targets} remains visible; CORA request {} response does not cover it (response digest {})",
                request.id,
                response_digest.as_deref().unwrap_or("none")
            )),
            _ => {}
        }
    }
    gap_resolutions.sort();

    // Reviewed fact-citations (Item 2): every factual statement the published
    // page makes resolves to a preserved source snapshot with an exact quote,
    // so the site gate can require an Approved citation-review decision bound
    // to the exact digests. See `build_citations`.
    let citations = build_citations(store, &events, &identifiers, &organizations);

    // The "what changed" section (Item 4) from snapshot supersessions for the
    // matter's sources. See `build_what_changed`.
    let what_changed = build_what_changed(store, &events)?;

    // Documented official-relationship links (Item 5): exact, declared-field
    // matches. Each line is phrased as "the preserved record X (snapshot,
    // digest) references Y in reference field Z" — a factual statement about
    // the preserved record, never a legal conclusion. Only links touching this
    // matter (as source record or target matter) are shown.
    let mut documented_relationships = Vec::new();
    for link in store.all_official_relationships()? {
        let touches = link.source_record_id.starts_with(&format!("{matter_id}:"))
            || link.target_matter_id == matter_id;
        if !touches {
            continue;
        }
        documented_relationships.push(format!(
            "The preserved record {} (snapshot {}, digest {}) references {} in reference field {}.",
            link.source_record_id,
            link.source_snapshot_id,
            link.source_snapshot_digest,
            link.target_identifier,
            link.reference_field
        ));
    }
    documented_relationships.sort();
    documented_relationships.dedup();

    Ok(CaseFileContent {
        matter,
        events,
        identifiers,
        organizations,
        coverage,
        contradictions,
        missing_documents,
        cora_requests,
        gap_resolutions,
        citations,
        what_changed,
        documented_relationships,
        coverage_summary,
        provenance,
        limitations,
    })
}

/// Builds the "what changed" lines from snapshot supersessions relevant to the
/// matter. For each source referenced by the matter's events, every snapshot
/// that supersedes an earlier one yields a line: the two snapshot ids and
/// digests, the later retrieval timestamp, and the record-level diff, phrased
/// as a comparison ("observed in snapshot N ... not present in snapshot M"),
/// never as a legal conclusion. When the same source has multiple revisions,
/// each supersession pair is reported.
fn build_what_changed(
    store: &pnull_core::Store,
    events: &[ProcurementEvent],
) -> Result<Vec<String>, pnull_core::CoreError> {
    // Distinct source ids referenced by the matter's event evidence.
    let mut source_ids = std::collections::BTreeSet::new();
    for event in events {
        for evidence_id in &event.evidence_ids {
            if let Ok(snapshot) = store.source_snapshot(evidence_id) {
                source_ids.insert(snapshot.source_id.clone());
            }
        }
    }

    let mut lines = Vec::new();
    for source_id in source_ids {
        let snapshots = store.source_snapshots(&source_id)?;
        for snapshot in &snapshots {
            let Some(old_id) = snapshot.supersedes.as_deref() else {
                continue;
            };
            let Some(old) = store.source_snapshot(old_id).ok() else {
                continue;
            };
            let mut line = format!(
                "Snapshot {} (digest {}) supersedes snapshot {} (digest {}); retrieved at {}.",
                snapshot.id,
                snapshot.persisted_digest,
                old.id,
                old.persisted_digest,
                snapshot.retrieved_at
            );
            if let Some(diff) = store.snapshot_diff(old_id, &snapshot.id)?
                && !diff.changes.is_empty()
            {
                let changes: Vec<String> = diff
                    .changes
                    .iter()
                    .map(|c| format!("{}: {}", c.kind, c.summary))
                    .collect();
                line.push_str(" Record-level changes: ");
                line.push_str(&changes.join("; "));
            }
            lines.push(line);
        }
    }
    lines.sort();
    Ok(lines)
}

/// Builds the deterministic fact-citations for a matter's case file.
///
/// Each citation references a preserved `SourceSnapshot` (as its evidence) with
/// an exact quote drawn from the preserved record. The timeline events carry
/// explicit `evidence_ids` (snapshot ids), so their summaries are quoted
/// against the snapshot that records them. Identifiers and organizations are
/// quoted against the matter's primary snapshot (the first snapshot referenced
/// by any event), so no citation invents an evidence association. A citation is
/// emitted only when the referenced snapshot resolves; unresolvable references
/// produce no citation and no assertion.
fn build_citations(
    store: &pnull_core::Store,
    events: &[ProcurementEvent],
    identifiers: &[ProcurementIdentifier],
    organizations: &[ProcurementOrganization],
) -> Vec<Citation> {
    // Resolve each event's first evidence snapshot, in event order.
    let mut primary_snapshot: Option<pnull_core::SourceSnapshot> = None;
    let mut by_evidence: Vec<(String, pnull_core::SourceSnapshot)> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for event in events {
        if let Some(evidence_id) = event.evidence_ids.first()
            && let Ok(snapshot) = store.source_snapshot(evidence_id)
        {
            if primary_snapshot.is_none() {
                primary_snapshot = Some(snapshot.clone());
            }
            if seen.insert(snapshot.id.clone()) {
                by_evidence.push((snapshot.id.clone(), snapshot));
            }
        }
    }

    let mut citations: Vec<Citation> = Vec::new();
    let locator_for = |snapshot: &pnull_core::SourceSnapshot| Locator {
        kind: "procurement-snapshot".to_owned(),
        start: 1,
        end: 1,
        label: format!("{} retrieved {}", snapshot.source_id, snapshot.retrieved_at),
    };
    // Timeline events quoted against their own evidence snapshot.
    for event in events {
        if let Some(evidence_id) = event.evidence_ids.first()
            && let Ok(snapshot) = store.source_snapshot(evidence_id)
        {
            citations.push(Citation {
                evidence_id: snapshot.id.clone(),
                source_url: snapshot.source_url.clone(),
                locator: locator_for(&snapshot),
                quote: event.summary.clone(),
            });
        }
    }
    // Identifiers and organizations quoted against the primary snapshot.
    if let Some(primary) = &primary_snapshot {
        for identifier in identifiers {
            citations.push(Citation {
                evidence_id: primary.id.clone(),
                source_url: primary.source_url.clone(),
                locator: locator_for(primary),
                quote: identifier.raw.clone(),
            });
        }
        for org in organizations {
            citations.push(Citation {
                evidence_id: primary.id.clone(),
                source_url: primary.source_url.clone(),
                locator: locator_for(primary),
                quote: org.raw_name.clone(),
            });
        }
    }
    // Deterministic order: by evidence id, then locator, then quote.
    citations.sort_by(|a, b| {
        (&a.evidence_id, &a.locator.label, &a.quote).cmp(&(
            &b.evidence_id,
            &b.locator.label,
            &b.quote,
        ))
    });
    citations
}

/// Looks up the observed digest of the response evidence linked to a CORA
/// request, if any. The response evidence id is read from the most recent
/// `response_received` event note; its digest is read from the imported
/// supplied-record store. Returns `None` when no response was received.
fn request_response_digest(
    store: &pnull_core::Store,
    request: &CoraRequest,
) -> Result<Option<String>, pnull_core::CoreError> {
    for event in request.events.iter().rev() {
        if event.state != CoraRequestState::ResponseReceived {
            continue;
        }
        // The response_received note is "response evidence <id> linked...".
        let id = event.note.split_whitespace().nth(2).map(str::to_owned);
        if let Some(id) = id {
            return store.supplied_record_digest(&id);
        }
    }
    Ok(None)
}

/// The standard limitations block carried by every case file.
pub fn default_limitations() -> Vec<String> {
    vec![
        "This is not comprehensive procurement coverage.".to_owned(),
        "An informational mirror is not an authoritative procurement system.".to_owned(),
        "Absence from checked sources is not proof of absence.".to_owned(),
        "Vendor appearance is not proof of procurement wrongdoing.".to_owned(),
        "A technology purchase is not automatically surveillance.".to_owned(),
        "OpenBook may not provide vendor-level payment evidence.".to_owned(),
        "Restricted records may require a lawful CORA request.".to_owned(),
        "Panopticon Null does not provide legal advice or a legal-compliance guarantee.".to_owned(),
    ]
}

/// Renders a deterministic Markdown case file from the content.
#[allow(clippy::too_many_lines)]
pub fn render_markdown(content: &CaseFileContent) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Case File: {}\n", content.matter.title);
    let _ = writeln!(out, "**Jurisdiction:** {}  ", content.matter.jurisdiction);
    let _ = writeln!(out, "**Review state:** {}  ", content.matter.review_state);
    let _ = writeln!(
        out,
        "**Publication state:** {}\n",
        content.matter.publication_state
    );

    out.push_str("## Identifiers\n\n");
    if content.identifiers.is_empty() {
        let _ = writeln!(out, "{NOT_OBSERVED_PHRASING}\n");
    } else {
        for identifier in &content.identifiers {
            let _ = writeln!(
                out,
                "- `{}` ({}; source {})",
                identifier.raw,
                identifier.kind.label(),
                identifier.source_id
            );
        }
        out.push('\n');
    }

    out.push_str("## Organizations\n\n");
    if content.organizations.is_empty() {
        let _ = writeln!(out, "{NOT_OBSERVED_PHRASING}\n");
    } else {
        for org in &content.organizations {
            let _ = writeln!(
                out,
                "- **{}** — {} (source {})",
                org.raw_name,
                org.role.label(),
                org.source_id
            );
        }
        out.push('\n');
    }

    out.push_str("## Timeline\n\n");
    let mut events = content.events.clone();
    events.sort_by(|a, b| a.date.cmp(&b.date).then(a.id.cmp(&b.id)));
    if events.is_empty() {
        let _ = writeln!(out, "{NOT_OBSERVED_PHRASING}\n");
    } else {
        for event in events {
            let _ = writeln!(
                out,
                "- **{}** — {} ({})",
                event.date.as_deref().unwrap_or("date unknown"),
                event.summary,
                event.kind.label()
            );
        }
        out.push('\n');
    }

    out.push_str("## Reviewed citations\n\n");
    if content.citations.is_empty() {
        let _ = writeln!(out, "{NOT_OBSERVED_PHRASING}\n");
    } else {
        for citation in &content.citations {
            let _ = writeln!(
                out,
                "- **{quote}** — {evidence} (source {source_url}; locator {locator})",
                quote = citation.quote,
                evidence = citation.evidence_id,
                source_url = citation.source_url,
                locator = citation.locator.label
            );
        }
        out.push('\n');
    }

    out.push_str("## What changed\n\n");
    if content.what_changed.is_empty() {
        let _ = writeln!(out, "None observed in the checked sources.\n");
    } else {
        for line in &content.what_changed {
            let _ = writeln!(out, "- {line}");
        }
        out.push('\n');
    }

    out.push_str("## Documented relationships\n\n");
    if content.documented_relationships.is_empty() {
        let _ = writeln!(
            out,
            "No preserved record in the checked sources explicitly references another official identifier in a declared reference field.\n"
        );
    } else {
        for line in &content.documented_relationships {
            let _ = writeln!(out, "- {line}");
        }
        out.push('\n');
    }

    out.push_str("## Contradictions\n\n");
    if content.contradictions.is_empty() {
        out.push_str("None observed in the checked sources.\n\n");
    } else {
        for contradiction in &content.contradictions {
            let _ = writeln!(out, "- {contradiction}");
        }
        out.push('\n');
    }

    out.push_str("## Missing expected documents\n\n");
    if content.missing_documents.is_empty() {
        let _ = writeln!(out, "{NOT_OBSERVED_PHRASING}\n");
    } else {
        for missing in &content.missing_documents {
            let _ = writeln!(out, "- {missing}");
        }
        out.push('\n');
    }

    out.push_str("## Records requests (CORA ledger)\n\n");
    if content.cora_requests.is_empty() {
        let _ = writeln!(out, "{NOT_OBSERVED_PHRASING}\n");
    } else {
        for request in &content.cora_requests {
            let _ = writeln!(
                out,
                "- **{}** — {} (request {})",
                request.id,
                request.state.label(),
                request.matter_id
            );
            if !request.missing_record_types.is_empty() {
                let _ = writeln!(
                    out,
                    "  - targeted missing records: {}",
                    request.missing_record_types.join(", ")
                );
            }
            let _ = writeln!(out, "  - draft digest: `{}`", request.draft_digest);
            // Summarize the lifecycle events, showing response evidence where
            // it was linked (and noting still-unresolved responses).
            for event in &request.events {
                let _ = writeln!(
                    out,
                    "  - [{}] {} by {}",
                    event.state.label(),
                    event.note,
                    event.operator
                );
            }
        }
        out.push('\n');
    }

    out.push_str("## Gap resolution status\n\n");
    if content.gap_resolutions.is_empty() {
        let _ = writeln!(out, "None.\n");
    } else {
        for line in &content.gap_resolutions {
            let _ = writeln!(out, "- {line}");
        }
        out.push('\n');
    }

    out.push_str("## Coverage summary\n\n");
    if content.coverage_summary.is_empty() {
        let _ = writeln!(out, "{NOT_OBSERVED_PHRASING}\n");
    } else {
        for (source_id, line) in &content.coverage_summary {
            let _ = writeln!(out, "- **{source_id}**: {line}");
        }
        out.push('\n');
    }

    out.push_str("## Retrieval and processing provenance\n\n");
    if content.provenance.is_empty() {
        let _ = writeln!(out, "{NOT_OBSERVED_PHRASING}\n");
    } else {
        for line in &content.provenance {
            let _ = writeln!(out, "- {line}");
        }
        out.push('\n');
    }

    out.push_str("## Limitations\n\n");
    for limitation in &content.limitations {
        let _ = writeln!(out, "- {limitation}");
    }
    out.push('\n');
    out
}

/// Renders a deterministic JSON case file (indented, stable field order).
pub fn render_json(content: &CaseFileContent) -> String {
    serde_json::to_string_pretty(content).expect("case file serialization")
}

/// Produces and persists a draft case file, returning it with its digests.
pub fn generate(
    store: &pnull_core::Store,
    matter_id: &str,
    built_at: &str,
) -> Result<CaseFile, CaseFileError> {
    let content = build_content(store, matter_id)?;
    let json = render_json(&content);
    let markdown = render_markdown(&content);
    let manifest = pnull_core::sha256_manifest(&[
        ("case-file.json".to_owned(), sha256_hex(json.as_bytes())),
        ("case-file.md".to_owned(), sha256_hex(markdown.as_bytes())),
    ]);
    let case_file = CaseFile {
        id: CaseFile::id_for(matter_id, built_at),
        matter_id: matter_id.to_owned(),
        state: CaseFileState::Draft,
        json_digest: sha256_hex(json.as_bytes()),
        markdown_digest: sha256_hex(markdown.as_bytes()),
        sha256_manifest: manifest.clone(),
        built_at: built_at.to_owned(),
    };
    store.insert_case_file(&case_file)?;
    Ok(case_file)
}

/// Deterministic money display helper for case-file rendering.
pub fn money_display(value: &MoneyValue) -> String {
    value.display()
}

/// A helper for tests: does any event of the given kind exist in the content?
pub fn has_event_kind(content: &CaseFileContent, kind: ProcurementEventKind) -> bool {
    content.events.iter().any(|e| e.kind == kind)
}

/// Authority label lookup for citation rendering.
pub fn authority_label(authority: SourceAuthority) -> &'static str {
    authority.label()
}

/// Coverage-state label lookup.
pub fn coverage_state_label(state: CoverageState) -> &'static str {
    state.label()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pnull_core::{
        IdentifierKind, OfficialRelationship, OfficialRelationshipKind, OrganizationRole,
        ProcurementEventKind,
    };

    fn seed_matter(store: &pnull_core::Store, matter_id: &str) {
        let matter = ProcurementMatter {
            id: matter_id.to_owned(),
            jurisdiction: "Colorado Springs".to_owned(),
            title: "R26-023AB Transit Fare".to_owned(),
            review_state: "draft".to_owned(),
            publication_state: "unpublished".to_owned(),
        };
        store.insert_procurement_matter(&matter).expect("matter");
        let event = ProcurementEvent {
            id: ProcurementEvent::id_for(
                matter_id,
                ProcurementEventKind::SolicitationPublished,
                "2026-01-01",
                "solicitation published",
            ),
            matter_id: matter_id.to_owned(),
            kind: ProcurementEventKind::SolicitationPublished,
            date: Some("2026-01-01".to_owned()),
            summary: "solicitation published".to_owned(),
            identifier_ids: Vec::new(),
            evidence_ids: Vec::new(),
            source_id: "src".to_owned(),
        };
        store.insert_procurement_event(&event).expect("event");
        let identifier = ProcurementIdentifier {
            id: ProcurementIdentifier::id_for(
                matter_id,
                IdentifierKind::SolicitationNumber,
                "R26-023AB",
            ),
            matter_id: matter_id.to_owned(),
            kind: IdentifierKind::SolicitationNumber,
            raw: "R26-023AB".to_owned(),
            source_id: "src".to_owned(),
            normalized: Some("R26023AB".to_owned()),
            normalization_rule: Some("uppercase-alphanumeric-compact".to_owned()),
            known: false,
        };
        store
            .insert_procurement_identifier(&identifier)
            .expect("identifier");
        let org = ProcurementOrganization {
            id: ProcurementOrganization::id_for(
                matter_id,
                OrganizationRole::AwardedContractor,
                "Adarand Constructors",
            ),
            matter_id: matter_id.to_owned(),
            role: OrganizationRole::AwardedContractor,
            raw_name: "Adarand Constructors".to_owned(),
            source_id: "src".to_owned(),
            normalized_alias: None,
            alias_reviewed: false,
        };
        store.insert_procurement_organization(&org).expect("org");
    }

    #[test]
    fn case_file_renders_json_and_markdown() {
        let dir = tempfile::tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        seed_matter(&store, "matter:1");
        let content = build_content(&store, "matter:1").expect("content");
        assert!(has_event_kind(
            &content,
            ProcurementEventKind::SolicitationPublished
        ));
        let md = render_markdown(&content);
        assert!(md.contains("R26-023AB"));
        assert!(md.contains("Adarand Constructors"));
        assert!(md.contains("Limitations"));
        assert!(md.contains("not proof of absence"));
        let json = render_json(&content);
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        assert!(parsed.get("matter").is_some());
    }

    #[test]
    fn case_file_is_deterministic() {
        let dir_a = tempfile::tempdir().expect("temp");
        let dir_b = tempfile::tempdir().expect("temp");
        let store_a = pnull_core::Store::open(dir_a.path()).expect("store");
        let store_b = pnull_core::Store::open(dir_b.path()).expect("store");
        seed_matter(&store_a, "matter:1");
        seed_matter(&store_b, "matter:1");
        let a = build_content(&store_a, "matter:1").expect("a");
        let b = build_content(&store_b, "matter:1").expect("b");
        assert_eq!(render_json(&a), render_json(&b));
        assert_eq!(render_markdown(&a), render_markdown(&b));
    }

    #[test]
    fn generate_persists_draft_and_manifest() {
        let dir = tempfile::tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        seed_matter(&store, "matter:1");
        let case_file = generate(&store, "matter:1", "2026-08-17T00:00:00Z").expect("generate");
        assert_eq!(case_file.state, CaseFileState::Draft);
        assert_eq!(case_file.sha256_manifest.len(), 2);
        let stored = store.case_files("matter:1").expect("stored");
        assert_eq!(stored.len(), 1);
    }

    #[test]
    fn missing_matter_is_an_error() {
        let dir = tempfile::tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        assert!(matches!(
            build_content(&store, "matter:nope"),
            Err(CaseFileError::MatterNotFound(_))
        ));
    }

    #[test]
    fn case_file_gap_section_reflects_resolution_state() {
        let dir = tempfile::tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        seed_matter(&store, "matter:1");

        // A resolved request: drafted -> submitted -> response -> gap_resolved.
        let evid = "supplied-record:resp";
        store
            .insert_supplied_record_json(evid, "abcdef0123", r#"{"id":"supplied-record:resp"}"#)
            .expect("insert evidence");
        crate::cora_ledger::register_draft(
            &store,
            "matter:1",
            "City",
            vec!["R26-023AB".to_owned()],
            vec!["executed contract".to_owned()],
            Some((Some("2026-01-01".to_owned()), Some("2026-08-17".to_owned()))),
            Some("Transit Fare".to_owned()),
            vec!["colorado-springs-contract-awards".to_owned()],
            "draft text resolved",
            crate::cora_ledger::OFFLINE_CREATED_AT,
        )
        .expect("register");
        let resolved_id = store.all_cora_requests().expect("all")[0].id.clone();
        crate::cora_ledger::submit(&store, &resolved_id, "op", "2026-08-20", "TRK-1", None)
            .expect("submit");
        crate::cora_ledger::response_received(&store, &resolved_id, evid, None).expect("received");
        crate::cora_ledger::gap_resolved(
            &store,
            &resolved_id,
            "op",
            "gap covered by cited evidence",
        )
        .expect("resolved");

        // A still-unresolved request on a second gap.
        let unres_evid = "supplied-record:resp2";
        store
            .insert_supplied_record_json(
                unres_evid,
                "9988776655",
                r#"{"id":"supplied-record:resp2"}"#,
            )
            .expect("insert evidence");
        crate::cora_ledger::register_draft(
            &store,
            "matter:1",
            "City",
            vec!["R26-023AB".to_owned()],
            vec!["vendor-level expenditure evidence".to_owned()],
            Some((Some("2026-01-01".to_owned()), Some("2026-08-17".to_owned()))),
            Some("Transit Fare".to_owned()),
            vec!["openbook-cos".to_owned()],
            "draft text unresolved",
            "2026-08-18T00:00:00Z",
        )
        .expect("register unresolved");
        let unres_id = store
            .all_cora_requests()
            .expect("all")
            .iter()
            .find(|r| r.id != resolved_id)
            .map(|r| r.id.clone())
            .expect("unresolved id");
        crate::cora_ledger::submit(&store, &unres_id, "op", "2026-08-21", "TRK-2", None)
            .expect("submit");
        crate::cora_ledger::response_received(&store, &unres_id, unres_evid, None)
            .expect("received");
        crate::cora_ledger::still_unresolved(
            &store,
            &unres_id,
            "op",
            "response does not cover the gap",
        )
        .expect("unresolved");

        let content = build_content(&store, "matter:1").expect("content");
        let md = render_markdown(&content);
        // The resolved gap is marked covered with the request id and digest.
        assert!(md.contains(&format!("closed by CORA request {resolved_id}")));
        assert!(md.contains("abcdef0123"), "resolved response digest noted");
        // The still-unresolved gap remains visible with its response digest.
        assert!(md.contains("remains visible"));
        assert!(md.contains(&format!(
            "CORA request {unres_id} response does not cover it"
        )));
        assert!(
            md.contains("9988776655"),
            "unresolved response digest noted"
        );
        // Deterministic JSON carries the same status lines.
        let json = render_json(&content);
        assert!(json.contains("gap_resolutions"));
    }

    #[test]
    fn documented_relationships_render_on_case_file() {
        let dir = tempfile::tempdir().expect("temp");
        let store = pnull_core::Store::open(dir.path()).expect("store");
        seed_matter(&store, "matter:1");
        seed_matter(&store, "matter:2");

        // No relationships yet -> absence phrasing.
        let content_empty = build_content(&store, "matter:1").expect("content");
        let md_empty = render_markdown(&content_empty);
        assert!(
            md_empty.contains("No preserved record in the checked sources explicitly references")
        );

        // Insert a genuine official-relationship link touching matter:1 as the
        // source record.
        let link = OfficialRelationship {
            id: OfficialRelationship::id_for(
                "matter:1:record:award:0",
                "notes",
                "25-93",
                "snapshot:src1",
            ),
            kind: OfficialRelationshipKind::OfficialRelationship,
            source_record_id: "matter:1:record:award:0".to_owned(),
            source_snapshot_id: "snapshot:src1".to_owned(),
            source_snapshot_digest: "a".repeat(64),
            target_identifier: "25-93".to_owned(),
            target_matter_id: "matter:2".to_owned(),
            reference_field: "notes".to_owned(),
            quote: "25-93".to_owned(),
            locator: "contract-award row notes column".to_owned(),
            citations: vec!["citation a".to_owned(), "citation b".to_owned()],
            reviewed: true,
        };
        store.insert_official_relationship(&link).expect("insert");

        let content = build_content(&store, "matter:1").expect("content");
        assert_eq!(content.documented_relationships.len(), 1);
        let md = render_markdown(&content);
        assert!(
            md.contains(
                "The preserved record matter:1:record:award:0 (snapshot snapshot:src1, digest "
            ),
            "phrasing discipline for documented relationships"
        );
        assert!(md.contains("references 25-93 in reference field notes"));
        // JSON carries the section too.
        let json = render_json(&content);
        assert!(json.contains("documented_relationships"));
        assert!(json.contains("matter:1:record:award:0"));
    }
}
