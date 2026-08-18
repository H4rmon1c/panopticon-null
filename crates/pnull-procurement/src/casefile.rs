//! Deterministic case-file generation for a procurement matter, in both
//! machine-readable JSON and human-readable Markdown.
//!
//! Every public factual statement in the case file must resolve to a reviewed
//! citation, and the file remains a `draft` until the human citation-review and
//! publication-allowlist controls pass.

use pnull_core::{
    CaseFile, CaseFileState, CoverageEntry, CoverageState, MoneyValue, ProcurementEvent,
    ProcurementEventKind, ProcurementIdentifier, ProcurementMatter, ProcurementOrganization,
    ReconciliationKind, SourceAuthority, sha256_hex,
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
    pub coverage_summary: BTreeMap<String, String>,
    pub provenance: Vec<String>,
    pub limitations: Vec<String>,
}

/// Builds the deterministic case-file content from the store for a matter.
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

    Ok(CaseFileContent {
        matter,
        events,
        identifiers,
        organizations,
        coverage,
        contradictions,
        missing_documents,
        coverage_summary,
        provenance,
        limitations,
    })
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
    use pnull_core::{IdentifierKind, OrganizationRole, ProcurementEventKind};

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
}
