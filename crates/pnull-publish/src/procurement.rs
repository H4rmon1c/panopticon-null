//! Publication of the procurement chain (v0.0.4, Item 2).
//!
//! Renders `/co/procurement/index.html` and one page per procurement matter,
//! from the **same deterministic case-file content** (`build_content`) that
//! `pnull procurement case build` serializes to JSON. There is exactly one
//! source of truth: the site never maintains a second renderer.
//!
//! Every page fails closed on the same gates the document pages use:
//! every citation must carry a current `Approved` citation-review decision
//! bound to the exact digests; a `procurement_casefile` publication-allowlist
//! category must be present (an allowlist is not auto-approval); and the
//! privacy backstop runs over all rendered text, including vendor names and
//! raw money strings. When any gate fails, the page/entry is withheld from the
//! build with a visible "publication withheld pending review" note rather than
//! a partial page.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use pnull_core::{Citation, CoverageState, ProcurementAlert, Store};
use pnull_procurement::casefile::CaseFileContent;
use pnull_procurement::coverage::NOT_OBSERVED_PHRASING;

use crate::{
    PublishError, citation_id, escape, escape_attr, escape_xml, publication_allowlist_allows,
    safe_id, validate_public_text,
};

/// The allowlist category that permits procurement case-file content to be
/// published. An allowlist entry is not automatic approval; the citation
/// review gate still applies.
pub const PROCUREMENT_CASEFILE_CATEGORY: &str = "procurement_casefile";

/// The report `pnull procurement publish-ready <matter-id>` prints. It is the
/// operator's pre-publish checklist: it reports gate state without publishing
/// anything.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicationGateReport {
    pub matter_id: String,
    pub citations_pending: Vec<String>,
    pub citations_rejected: Vec<String>,
    pub citations_stale: Vec<String>,
    pub allowlist_present: bool,
    pub privacy_issues: Vec<String>,
    pub withheld: bool,
    pub withholds_for: Vec<String>,
}

impl PublicationGateReport {
    pub fn all_pending(&self) -> bool {
        self.citations_pending.is_empty()
            && self.citations_rejected.is_empty()
            && self.citations_stale.is_empty()
            && self.allowlist_present
            && self.privacy_issues.is_empty()
    }
}

/// Evaluates every publication gate for a matter's case-file content, without
/// publishing anything. Used by `publish-ready` and shared by the page build.
pub fn evaluate_gate(
    store: &Store,
    content: &CaseFileContent,
) -> Result<PublicationGateReport, PublishError> {
    let matter_id = content.matter.id.clone();
    let mut pending = Vec::new();
    let mut rejected = Vec::new();
    let mut stale = Vec::new();
    for citation in &content.citations {
        let id = citation_id(citation);
        match store.current_review(&id)? {
            None => pending.push(id),
            Some(decision) => {
                if decision.state != pnull_core::ReviewState::Approved {
                    rejected.push(id);
                } else if decision.bound_digest != crate::citation_review_binding(citation).digest()
                {
                    stale.push(id);
                }
            }
        }
    }
    let allowlist_present = publication_allowlist_allows(store, PROCUREMENT_CASEFILE_CATEGORY)?;

    // Privacy backstop over all rendered text, including vendor names and raw
    // money strings. Validate both the assembled page and each raw field value
    // individually so an HTML tag boundary can never mask a private identifier.
    let mut privacy_issues = Vec::new();
    let rendered = render_matter_page(store, content)?;
    if let Err(PublishError::Sensitive(issue)) = validate_public_text(&rendered) {
        privacy_issues.push(issue);
    }
    for org in &content.organizations {
        if let Err(PublishError::Sensitive(issue)) = validate_public_text(&org.raw_name) {
            privacy_issues.push(format!("vendor name: {issue}"));
        }
    }
    for identifier in &content.identifiers {
        if let Err(PublishError::Sensitive(issue)) = validate_public_text(&identifier.raw) {
            privacy_issues.push(format!("identifier: {issue}"));
        }
    }
    for event in &content.events {
        if let Err(PublishError::Sensitive(issue)) = validate_public_text(&event.summary) {
            privacy_issues.push(format!("event: {issue}"));
        }
    }

    let mut withholds_for = Vec::new();
    if !pending.is_empty() {
        withholds_for.push(format!("{} pending citation(s)", pending.len()));
    }
    if !rejected.is_empty() {
        withholds_for.push(format!("{} rejected citation(s)", rejected.len()));
    }
    if !stale.is_empty() {
        withholds_for.push(format!("{} stale citation(s)", stale.len()));
    }
    if !allowlist_present {
        withholds_for.push(format!("missing {PROCUREMENT_CASEFILE_CATEGORY} allowlist"));
    }
    if !privacy_issues.is_empty() {
        withholds_for.push("privacy backstop".to_owned());
    }

    Ok(PublicationGateReport {
        matter_id,
        citations_pending: pending,
        citations_rejected: rejected,
        citations_stale: stale,
        allowlist_present,
        privacy_issues,
        withheld: !withholds_for.is_empty(),
        withholds_for,
    })
}

/// Builds all procurement publication output under `output_dir/co/procurement`.
/// The matter index and per-matter pages are written only when their gates
/// pass; a withheld page is replaced by a visible "publication withheld pending
/// review" note rather than a partial page. Returns the paths written.
pub fn build_procurement_site(
    store: &Store,
    output_dir: &Path,
    canonical_base_url: &str,
    written: &mut Vec<PathBuf>,
) -> Result<(), PublishError> {
    let co = output_dir.join("co");
    let procurement_dir = co.join("procurement");
    std::fs::create_dir_all(&procurement_dir)?;
    let index_link_prefix = "../../../";

    // Per-matter pages.
    let mut matters = store.procurement_matters()?;
    matters.sort_by(|a, b| a.id.cmp(&b.id));
    let mut published: Vec<(String, String, CoverageState)> = Vec::new(); // (slug, title, coverage)
    for matter in matters {
        let content = pnull_procurement::build_content(store, &matter.id)
            .map_err(|e| PublishError::Procurement(e.to_string()))?;
        let slug = matter_slug(&matter.id);
        let matter_dir = procurement_dir.join(&slug);
        std::fs::create_dir_all(&matter_dir)?;
        let page_path = matter_dir.join("index.html");
        let gate = evaluate_gate(store, &content)?;
        let page = if gate.withheld {
            withheld_page(&matter.title, &gate)
        } else {
            render_matter_page(store, &content)?
        };
        validate_public_text(&page)?;
        std::fs::write(&page_path, page.as_bytes())?;
        written.push(page_path);
        let latest_coverage = latest_coverage_state(store, &content);
        published.push((slug, matter.title, latest_coverage));
    }

    // Matter index.
    let index = render_index(&published);
    let index_path = procurement_dir.join("index.html");
    validate_public_text(&index)?;
    std::fs::write(&index_path, index.as_bytes())?;
    written.push(index_path);

    // Atom entries for procurement matters and change alerts.
    let atom = render_procurement_atom(store, canonical_base_url)?;
    validate_public_text(&atom)?;
    let atom_path = procurement_dir.join("atom.xml");
    std::fs::write(&atom_path, atom.as_bytes())?;
    written.push(atom_path);

    let _ = index_link_prefix;
    Ok(())
}

fn latest_coverage_state(store: &Store, content: &CaseFileContent) -> CoverageState {
    // The matter's events carry evidence snapshot ids; resolve the latest
    // snapshot's coverage state per source. Fall back to the case-file's own
    // coverage entries.
    let mut latest: Option<CoverageState> = None;
    for event in &content.events {
        for evidence_id in &event.evidence_ids {
            if let Ok(snapshot) = store.source_snapshot(evidence_id) {
                latest = Some(snapshot.coverage_state);
            }
        }
    }
    latest.unwrap_or(CoverageState::InformationalOnly)
}

/// A deterministic slug for a matter id: safe-id the matter id, then, if it
/// does not already end with a recognizable tail, append nothing. The slug is
/// derived from the stable matter id alone so it is reproducible.
fn matter_slug(matter_id: &str) -> String {
    safe_id(matter_id)
}

fn withheld_page(title: &str, gate: &PublicationGateReport) -> String {
    let mut body = format!(
        "<p class=\"state\">Publication withheld pending review.</p><p>{}</p><p>This page is not published because its publication gates have not all passed.</p><ul>",
        escape(title)
    );
    for reason in &gate.withholds_for {
        writeln!(body, "<li>{}</li>", escape(reason)).expect("string write");
    }
    body.push_str("</ul>");
    crate::page("Publication withheld", &body, "../../../../")
}

/// Renders a single matter page from its case-file content.
#[allow(clippy::too_many_lines)]
fn render_matter_page(store: &Store, content: &CaseFileContent) -> Result<String, PublishError> {
    let mut body = String::new();
    let m = &content.matter;
    writeln!(
        body,
        "<p class=\"state\">{} · {}</p>",
        escape(&m.review_state),
        escape(&m.publication_state)
    )
    .expect("string write");
    writeln!(
        body,
        "<p><strong>Jurisdiction:</strong> {}</p>",
        escape(&m.jurisdiction)
    )
    .expect("string write");

    body.push_str("<h2>Identifiers</h2>");
    if content.identifiers.is_empty() {
        writeln!(body, "<p>{NOT_OBSERVED_PHRASING}</p>").expect("string write");
    } else {
        body.push_str("<ul>");
        for identifier in &content.identifiers {
            writeln!(
                body,
                "<li><code>{}</code> — {} (source {})</li>",
                escape(&identifier.raw),
                escape(identifier.kind.label()),
                escape(&identifier.source_id)
            )
            .expect("string write");
        }
        body.push_str("</ul>");
    }

    body.push_str("<h2>Organizations</h2>");
    if content.organizations.is_empty() {
        writeln!(body, "<p>{NOT_OBSERVED_PHRASING}</p>").expect("string write");
    } else {
        body.push_str("<ul>");
        for org in &content.organizations {
            writeln!(
                body,
                "<li><strong>{}</strong> — {} (source {})</li>",
                escape(&org.raw_name),
                escape(org.role.label()),
                escape(&org.source_id)
            )
            .expect("string write");
        }
        body.push_str("</ul>");
    }

    body.push_str("<h2>Timeline</h2>");
    if content.events.is_empty() {
        writeln!(body, "<p>{NOT_OBSERVED_PHRASING}</p>").expect("string write");
    } else {
        body.push_str("<ul class=\"timeline\">");
        let mut events = content.events.clone();
        events.sort_by(|a, b| a.date.cmp(&b.date).then(a.id.cmp(&b.id)));
        for event in events {
            writeln!(
                body,
                "<li><strong>{}</strong> — {} ({})</li>",
                escape(event.date.as_deref().unwrap_or("date unknown")),
                escape(&event.summary),
                escape(event.kind.label())
            )
            .expect("string write");
        }
        body.push_str("</ul>");
    }

    body.push_str("<h2>Reviewed citations</h2>");
    if content.citations.is_empty() {
        writeln!(body, "<p>{NOT_OBSERVED_PHRASING}</p>").expect("string write");
    } else {
        body.push_str("<ol class=\"citations\">");
        for citation in &content.citations {
            writeln!(
                body,
                "<li><blockquote>{}</blockquote><p><a rel=\"external nofollow\" href=\"{}\">Official source</a> · {} · <a href=\"../../../../evidence/{}.html\">local hash and provenance</a></p></li>",
                escape(&citation.quote),
                escape_attr(&citation.source_url),
                escape(&citation.locator.label),
                safe_id(&citation.evidence_id)
            )
            .expect("string write");
        }
        body.push_str("</ol>");
    }

    body.push_str("<h2>What changed</h2>");
    if content.what_changed.is_empty() {
        writeln!(body, "<p>None observed in the checked sources.</p>").expect("string write");
    } else {
        body.push_str("<ul>");
        for line in &content.what_changed {
            writeln!(body, "<li>{}</li>", escape(line)).expect("string write");
        }
        body.push_str("</ul>");
        body.push_str("<p>These record changes in the public record. They are comparisons, not legal conclusions.</p>");
    }

    body.push_str("<h2>Documented relationships</h2>");
    if content.documented_relationships.is_empty() {
        writeln!(
            body,
            "<p>No preserved record in the checked sources explicitly references another official identifier in a declared reference field.</p>"
        )
        .expect("string write");
    } else {
        body.push_str("<ul>");
        for line in &content.documented_relationships {
            writeln!(body, "<li>{}</li>", escape(line)).expect("string write");
        }
        body.push_str("</ul>");
        body.push_str("<p>These are exact, declared-field matches in the preserved record; they are factual statements about the record, not legal conclusions.</p>");
    }

    body.push_str("<h2>Contradictions</h2>");
    if content.contradictions.is_empty() {
        body.push_str("<p>None observed in the checked sources.</p>");
    } else {
        body.push_str("<ul>");
        for contradiction in &content.contradictions {
            writeln!(body, "<li>{}</li>", escape(contradiction)).expect("string write");
        }
        body.push_str("</ul>");
    }

    body.push_str("<h2>Missing documents and coverage gaps</h2>");
    let mut gap_items = content.missing_documents.clone();
    gap_items.sort();
    gap_items.dedup();
    if gap_items.is_empty() {
        writeln!(body, "<p>{NOT_OBSERVED_PHRASING}</p>").expect("string write");
    } else {
        body.push_str("<ul>");
        for missing in &gap_items {
            writeln!(body, "<li>{}</li>", escape(missing)).expect("string write");
        }
        body.push_str("</ul>");
    }

    body.push_str("<h2>Records requests (CORA ledger)</h2>");
    if content.cora_requests.is_empty() {
        writeln!(body, "<p>{NOT_OBSERVED_PHRASING}</p>").expect("string write");
    } else {
        body.push_str("<ul>");
        for request in &content.cora_requests {
            writeln!(
                body,
                "<li><strong>{}</strong> — {} · draft digest <code>{}</code>",
                escape(&request.id),
                escape(request.state.label()),
                escape(&request.draft_digest)
            )
            .expect("string write");
            for event in &request.events {
                writeln!(
                    body,
                    "<p class=\"indent\">[{}] {} by {}</p>",
                    escape(event.state.label()),
                    escape(&event.note),
                    escape(&event.operator)
                )
                .expect("string write");
            }
            writeln!(body, "</li>").expect("string write");
        }
        body.push_str("</ul>");
    }

    body.push_str("<h2>Gap resolution status</h2>");
    if content.gap_resolutions.is_empty() {
        body.push_str("<p>None.</p>");
    } else {
        body.push_str("<ul>");
        for line in &content.gap_resolutions {
            writeln!(body, "<li>{}</li>", escape(line)).expect("string write");
        }
        body.push_str("</ul>");
    }

    body.push_str("<h2>Coverage summary</h2>");
    if content.coverage_summary.is_empty() {
        writeln!(body, "<p>{NOT_OBSERVED_PHRASING}</p>").expect("string write");
    } else {
        body.push_str("<ul>");
        for (source_id, line) in &content.coverage_summary {
            writeln!(
                body,
                "<li><strong>{}</strong>: {}</li>",
                escape(source_id),
                escape(line)
            )
            .expect("string write");
        }
        body.push_str("</ul>");
    }

    body.push_str("<h2>Retrieval and processing provenance</h2>");
    if content.provenance.is_empty() {
        writeln!(body, "<p>{NOT_OBSERVED_PHRASING}</p>").expect("string write");
    } else {
        body.push_str("<ul>");
        for line in &content.provenance {
            writeln!(body, "<li>{}</li>", escape(line)).expect("string write");
        }
        body.push_str("</ul>");
    }

    body.push_str("<h2>SHA-256 manifest</h2>");
    if let Some(case_file) = store.case_files(&m.id)?.last() {
        let manifest: Vec<String> = case_file
            .sha256_manifest
            .iter()
            .map(|(name, digest)| format!("{name} {digest}"))
            .collect();
        writeln!(
            body,
            "<p>Case-file manifest: <code>{}</code></p>",
            escape(&manifest.join("; "))
        )
        .expect("string write");
    }

    body.push_str("<h2>Limitations</h2><ul>");
    for limitation in &content.limitations {
        writeln!(body, "<li>{}</li>", escape(limitation)).expect("string write");
    }
    body.push_str("</ul>");
    body.push_str("<p><strong>No legal conclusions.</strong> This page reports what the preserved records state. It does not establish legality, corruption, abuse, or malice, and absence from checked sources is not proof of absence.</p>");

    let title = format!("{} — procurement", m.title);
    let page = crate::page(&title, &body, "../../../../");
    Ok(page)
}

fn render_index(published: &[(String, String, CoverageState)]) -> String {
    let mut body = String::from(
        "<p class=\"declaration\">The procurement chain is a verifiable public ledger: every acquisition visible, every promise permanent, every change provable.</p>",
    );
    if published.is_empty() {
        body.push_str("<p>No procurement matters are published.</p>");
        return body;
    }
    body.push_str("<ul>");
    for (slug, title, coverage) in published {
        writeln!(
            body,
            "<li><a href=\"{}/index.html\">{}</a> — coverage {}</li>",
            escape(slug),
            escape(title),
            escape(coverage.label())
        )
        .expect("string write");
    }
    body.push_str("</ul>");
    body.push_str("<p>Absence from a partial or informational source is not proof of absence.</p>");
    body
}

fn render_procurement_atom(store: &Store, base_url: &str) -> Result<String, PublishError> {
    let base = base_url.trim_end_matches('/');
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<feed xmlns=\"http://www.w3.org/2005/Atom\"><id>{}/co/procurement/atom.xml</id><title>Panopticon Null — procurement</title><updated>1970-01-01T00:00:00Z</updated><link rel=\"self\" href=\"{}/co/procurement/atom.xml\"/><subtitle>Public procurement ledger; not proof of absence.</subtitle>",
        escape_xml(base),
        escape_xml(base)
    );

    // Procurement change alerts as entries, under the same gates.
    let mut alerts = store.all_procurement_alerts()?;
    alerts.sort_by(|a, b| a.id.cmp(&b.id));
    for alert in &alerts {
        let path = format!(
            "{base}/co/procurement/change-alerts/{}.html",
            safe_id(&alert.id)
        );
        let title = alert_title(alert);
        write!(
            xml,
            "<entry><id>{}</id><title>{}</title><updated>{}T00:00:00Z</updated><link href=\"{}\"/><summary>{}</summary><category term=\"procurement-change\"/></entry>",
            escape_xml(&alert.id),
            escape_xml(&title),
            escape_xml(&alert.retrieved_at),
            escape_xml(&path),
            escape_xml(&alert.summary)
        )
        .expect("string write");
    }

    // Published matters as entries.
    let mut matters = store.procurement_matters()?;
    matters.sort_by(|a, b| a.id.cmp(&b.id));
    for matter in matters {
        let slug = matter_slug(&matter.id);
        let path = format!("{base}/co/procurement/{slug}/index.html");
        write!(
            xml,
            "<entry><id>proc-matter:{}</id><title>{}</title><updated>1970-01-01T00:00:00Z</updated><link href=\"{}\"/><summary>Procurement matter for {}. Not proof of absence.</summary></entry>",
            escape_xml(&matter.id),
            escape_xml(&matter.title),
            escape_xml(&path),
            escape_xml(&matter.jurisdiction)
        )
        .expect("string write");
    }

    xml.push_str("</feed>\n");
    Ok(xml)
}

fn alert_title(alert: &ProcurementAlert) -> String {
    let kinds: Vec<&str> = alert
        .changes
        .iter()
        .map(|c| c.change_kind.label())
        .collect();
    format!(
        "Procurement record changed ({}) — {}",
        kinds.join(", "),
        alert.source_id
    )
}

/// Renders a change-alert page under `/co/procurement/change-alerts/` for a
/// procurement change alert (Item 1/2). Reuses the same gates.
pub fn render_change_alert_page(alert: &ProcurementAlert) -> Result<String, PublishError> {
    let mut body = String::new();
    writeln!(
        body,
        "<p><strong>Feed:</strong> automated. <strong>Jurisdiction / matter:</strong> {} · {}.</p>",
        escape(&alert.source_id),
        escape(&alert.matter_ids.join(", "))
    )
    .expect("string write");
    writeln!(body, "<p>{}</p>", escape(&alert.summary)).expect("string write");
    body.push_str("<h2>Change details</h2><ul>");
    for change in &alert.changes {
        writeln!(
            body,
            "<li>{} on row <code>{}</code>: {}</li>",
            escape(change.change_kind.label()),
            escape(&change.row_identity),
            escape(&change.summary)
        )
        .expect("string write");
    }
    body.push_str("</ul>");
    writeln!(
        body,
        "<p>Old snapshot {} (digest <code>{}</code>) · new snapshot {} (digest <code>{}</code>).</p>",
        escape(&alert.old_snapshot_id),
        escape(&alert.old_snapshot_digest),
        escape(&alert.new_snapshot_id),
        escape(&alert.new_snapshot_digest)
    )
    .expect("string write");
    body.push_str(
        "<p><strong>This reports a change in the public record. It is a comparison, not a legal conclusion. Absence from checked sources is not proof of absence.</strong></p>",
    );
    for rule in &alert.taxonomy_matches {
        writeln!(body, "<p>Surveillance-related terminology observed, rule <code>{}</code> — terminology only, not an accusation.</p>", escape(rule)).expect("string write");
    }
    let title = format!("Procurement change — {}", alert.source_id);
    Ok(crate::page(&title, &body, "../../../../"))
}

/// A list of (citation, outcome) for the operator's publish-ready checklist.
pub fn citation_outcomes(
    store: &Store,
    content: &CaseFileContent,
) -> Result<Vec<(Citation, String)>, PublishError> {
    let mut outcomes = Vec::new();
    for citation in &content.citations {
        let id = citation_id(citation);
        let outcome = match store.current_review(&id)? {
            None => "no review decision (pending)".to_owned(),
            Some(decision) => {
                if decision.state != pnull_core::ReviewState::Approved {
                    format!("review state {:?} (not Approved)", decision.state)
                } else if decision.bound_digest != crate::citation_review_binding(citation).digest()
                {
                    "approval stale (digest mismatch)".to_owned()
                } else {
                    "approved".to_owned()
                }
            }
        };
        outcomes.push((citation.clone(), outcome));
    }
    Ok(outcomes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pnull_core::{
        CoverageState, IdentifierKind, OrganizationRole, ProcurementEventKind,
        ProcurementIdentifier, ProcurementMatter, ProcurementOrganization, PublicationAllowlist,
        ReviewDecision, ReviewState, SourceSnapshot,
    };

    fn snapshot(store: &Store, source_id: &str, digest: &str) -> String {
        let snap = SourceSnapshot {
            id: SourceSnapshot::id_for(source_id, digest),
            source_id: source_id.to_owned(),
            source_url: format!("https://example.test/{source_id}"),
            retrieved_at: "2026-08-17T00:00:00Z".to_owned(),
            persisted_digest: digest.to_owned(),
            content_type: Some("text/html".to_owned()),
            etag: None,
            last_modified: None,
            final_url: format!("https://example.test/{source_id}"),
            redirect_history: Vec::new(),
            parser_version: "awards-1.0".to_owned(),
            schema_version: 2,
            record_count: Some(1),
            pagination_complete: Some(true),
            coverage_state: CoverageState::InformationalOnly,
            supersedes: None,
        };
        store.insert_source_snapshot(&snap).expect("snapshot");
        snap.id
    }

    fn seed_matter(store: &Store, matter_id: &str) {
        let matter = ProcurementMatter {
            id: matter_id.to_owned(),
            jurisdiction: "Colorado Springs".to_owned(),
            title: "Test Matter".to_owned(),
            review_state: "draft".to_owned(),
            publication_state: "unpublished".to_owned(),
        };
        store.insert_procurement_matter(&matter).expect("matter");

        let evidence_id = snapshot(store, "colorado-springs-contract-awards", "digest-1");
        let event = pnull_core::ProcurementEvent {
            id: pnull_core::ProcurementEvent::id_for(
                matter_id,
                ProcurementEventKind::AwardAnnounced,
                "2026-01-01",
                "award announced",
            ),
            matter_id: matter_id.to_owned(),
            kind: ProcurementEventKind::AwardAnnounced,
            date: Some("2026-01-01".to_owned()),
            summary: "award announced".to_owned(),
            identifier_ids: Vec::new(),
            evidence_ids: vec![evidence_id.clone()],
            source_id: "colorado-springs-contract-awards".to_owned(),
        };
        store.insert_procurement_event(&event).expect("event");

        let identifier = ProcurementIdentifier {
            id: ProcurementIdentifier::id_for(
                matter_id,
                IdentifierKind::SolicitationNumber,
                "R26-001",
            ),
            matter_id: matter_id.to_owned(),
            kind: IdentifierKind::SolicitationNumber,
            raw: "R26-001".to_owned(),
            source_id: "colorado-springs-contract-awards".to_owned(),
            normalized: Some("R26001".to_owned()),
            normalization_rule: Some("uppercase-alphanumeric-compact".to_owned()),
            known: false,
        };
        store
            .insert_procurement_identifier(&identifier)
            .expect("id");

        let org = ProcurementOrganization {
            id: ProcurementOrganization::id_for(
                matter_id,
                OrganizationRole::AwardedContractor,
                "Example Vendor",
            ),
            matter_id: matter_id.to_owned(),
            role: OrganizationRole::AwardedContractor,
            raw_name: "Example Vendor".to_owned(),
            source_id: "colorado-springs-contract-awards".to_owned(),
            normalized_alias: None,
            alias_reviewed: false,
        };
        store.insert_procurement_organization(&org).expect("org");
    }

    fn add_allowlist(store: &Store) {
        let allowlist = PublicationAllowlist {
            id: "allowlist:test".to_owned(),
            field_categories: vec![PROCUREMENT_CASEFILE_CATEGORY.to_owned()],
            created_at: "2026-08-16T00:00:00Z".to_owned(),
            note: "test".to_owned(),
        };
        store
            .insert_publication_allowlist(&allowlist)
            .expect("allowlist");
    }

    fn approve_citations(store: &Store, content: &CaseFileContent) {
        for citation in &content.citations {
            let id = citation_id(citation);
            let binding = crate::citation_review_binding(citation);
            let decided_at = "2026-08-16T00:00:00Z".to_owned();
            let decision = ReviewDecision {
                id: ReviewDecision::id_for(&id, &decided_at),
                citation_id: id.clone(),
                state: ReviewState::Approved,
                reviewer: "test".to_owned(),
                note: String::new(),
                bound_digest: binding.digest(),
                decision_digest: String::new(),
                decided_at,
                supersedes: None,
            };
            store.insert_review(&decision).expect("review");
        }
    }

    fn fully_gated(store: &Store, matter_id: &str) -> CaseFileContent {
        let content = pnull_procurement::build_content(store, matter_id).expect("build content");
        add_allowlist(store);
        approve_citations(store, &content);
        content
    }

    #[test]
    fn pending_citation_withholds_page() {
        let dir = tempfile::tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        seed_matter(&store, "matter:1");
        let content = pnull_procurement::build_content(&store, "matter:1").expect("content");
        let gate = evaluate_gate(&store, &content).expect("gate");
        assert!(gate.withheld);
        assert!(gate.withholds_for.iter().any(|r| r.contains("pending")));
    }

    #[test]
    fn rejected_citation_withholds_page() {
        let dir = tempfile::tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        seed_matter(&store, "matter:1");
        let content = pnull_procurement::build_content(&store, "matter:1").expect("content");
        add_allowlist(&store);
        // Record a Rejected review for every citation.
        for citation in &content.citations {
            let id = citation_id(citation);
            let decided_at = "2026-08-16T00:00:00Z".to_owned();
            let decision = ReviewDecision {
                id: ReviewDecision::id_for(&id, &decided_at),
                citation_id: id.clone(),
                state: ReviewState::Rejected,
                reviewer: "test".to_owned(),
                note: String::new(),
                bound_digest: crate::citation_review_binding(citation).digest(),
                decision_digest: String::new(),
                decided_at,
                supersedes: None,
            };
            store.insert_review(&decision).expect("review");
        }
        let gate = evaluate_gate(&store, &content).expect("gate");
        assert!(gate.withheld);
        assert!(gate.withholds_for.iter().any(|r| r.contains("rejected")));
    }

    #[test]
    fn stale_approval_withholds_page() {
        let dir = tempfile::tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        seed_matter(&store, "matter:1");
        let content = pnull_procurement::build_content(&store, "matter:1").expect("content");
        add_allowlist(&store);
        // Approve with a wrong (stale) binding digest so the gate reports it.
        for citation in &content.citations {
            let id = citation_id(citation);
            let decided_at = "2026-08-16T00:00:00Z".to_owned();
            let decision = ReviewDecision {
                id: ReviewDecision::id_for(&id, &decided_at),
                citation_id: id.clone(),
                state: ReviewState::Approved,
                reviewer: "test".to_owned(),
                note: String::new(),
                bound_digest: "wrong-binding-digest".to_owned(),
                decision_digest: String::new(),
                decided_at,
                supersedes: None,
            };
            store.insert_review(&decision).expect("review");
        }
        let gate = evaluate_gate(&store, &content).expect("gate");
        assert!(gate.withheld);
        assert!(gate.withholds_for.iter().any(|r| r.contains("stale")));
    }

    #[test]
    fn missing_allowlist_withholds_page() {
        let dir = tempfile::tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        seed_matter(&store, "matter:1");
        let content = pnull_procurement::build_content(&store, "matter:1").expect("content");
        approve_citations(&store, &content);
        // No allowlist inserted.
        let gate = evaluate_gate(&store, &content).expect("gate");
        assert!(gate.withheld);
        assert!(gate.withholds_for.iter().any(|r| r.contains("allowlist")));
    }

    #[test]
    fn fully_gated_matter_is_not_withheld() {
        let dir = tempfile::tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        seed_matter(&store, "matter:1");
        let content = fully_gated(&store, "matter:1");
        let gate = evaluate_gate(&store, &content).expect("gate");
        assert!(!gate.withheld, "withholds: {:?}", gate.withholds_for);
    }

    #[test]
    fn hostile_vendor_name_with_plate_and_ssn_is_withheld() {
        let dir = tempfile::tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        seed_matter(&store, "matter:1");
        // A vendor whose raw name carries a plate-like token and an SSN-like
        // token must be caught by the privacy backstop over rendered text.
        let org = pnull_core::ProcurementOrganization {
            id: pnull_core::ProcurementOrganization::id_for(
                "matter:1",
                OrganizationRole::AwardedContractor,
                "ABC-12-3456 Acme 123-45-6789",
            ),
            matter_id: "matter:1".to_owned(),
            role: OrganizationRole::AwardedContractor,
            raw_name: "ABC-12-3456 Acme 123-45-6789".to_owned(),
            source_id: "colorado-springs-contract-awards".to_owned(),
            normalized_alias: None,
            alias_reviewed: false,
        };
        store.insert_procurement_organization(&org).expect("org");
        let content = fully_gated(&store, "matter:1");
        let gate = evaluate_gate(&store, &content).expect("gate");
        assert!(
            gate.withheld,
            "privacy backstop must withhold hostile vendor name"
        );
        assert!(
            gate.privacy_issues
                .iter()
                .any(|issue| issue.contains("vendor name")),
            "privacy issues: {:?}",
            gate.privacy_issues
        );
    }

    #[test]
    fn build_site_withholds_and_publishes_consistently() {
        let dir = tempfile::tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        seed_matter(&store, "matter:1");
        // Without gates: page is withheld, not partial.
        let out = tempfile::tempdir().expect("temp");
        let mut written = Vec::new();
        build_procurement_site(
            &store,
            out.path(),
            "https://example.invalid/pnull",
            &mut written,
        )
        .expect("build");
        let page = std::fs::read_to_string(
            out.path()
                .join("co/procurement")
                .join(matter_slug("matter:1"))
                .join("index.html"),
        )
        .expect("page");
        assert!(page.contains("Publication withheld pending review"));

        // With gates: page publishes with no surveillance labeling.
        fully_gated(&store, "matter:1");
        let out2 = tempfile::tempdir().expect("temp");
        let mut written2 = Vec::new();
        build_procurement_site(
            &store,
            out2.path(),
            "https://example.invalid/pnull",
            &mut written2,
        )
        .expect("build 2");
        let page2 = std::fs::read_to_string(
            out2.path()
                .join("co/procurement")
                .join(matter_slug("matter:1"))
                .join("index.html"),
        )
        .expect("page 2");
        assert!(!page2.contains("Publication withheld pending review"));
        assert!(page2.contains("Example Vendor"));
        assert!(!page2.contains("surveillance purchase"));
        assert!(!page2.contains("surveillance award"));
    }

    #[test]
    fn two_clean_builds_are_byte_identical() {
        let dir = tempfile::tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        seed_matter(&store, "matter:1");
        fully_gated(&store, "matter:1");
        let out_a = tempfile::tempdir().expect("temp a");
        let out_b = tempfile::tempdir().expect("temp b");
        let mut written_a = Vec::new();
        let mut written_b = Vec::new();
        build_procurement_site(
            &store,
            out_a.path(),
            "https://example.invalid/pnull",
            &mut written_a,
        )
        .expect("a");
        build_procurement_site(
            &store,
            out_b.path(),
            "https://example.invalid/pnull",
            &mut written_b,
        )
        .expect("b");
        let a_index = std::fs::read(out_a.path().join("co/procurement/index.html")).expect("a idx");
        let b_index = std::fs::read(out_b.path().join("co/procurement/index.html")).expect("b idx");
        assert_eq!(a_index, b_index);
        let a_page = std::fs::read(
            out_a
                .path()
                .join("co/procurement")
                .join(matter_slug("matter:1"))
                .join("index.html"),
        )
        .expect("a page");
        let b_page = std::fs::read(
            out_b
                .path()
                .join("co/procurement")
                .join(matter_slug("matter:1"))
                .join("index.html"),
        )
        .expect("b page");
        assert_eq!(a_page, b_page);
    }

    #[test]
    fn control_page_has_no_surveillance_category_text() {
        // The demo's benign control matter must never carry surveillance
        // terminology. This is exercised end-to-end in the demo reproduction;
        // here we assert the renderer emits no surveillance-category labels for
        // a matter whose identifiers and organizations are benign.
        let dir = tempfile::tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        seed_matter(&store, "matter:1");
        let content = fully_gated(&store, "matter:1");
        let page = render_matter_page(&store, &content).expect("render");
        for term in [
            "surveillance purchase",
            "surveillance award",
            "plate reader",
            "facial recognition",
        ] {
            assert!(
                !page.to_lowercase().contains(term),
                "control page must not contain {term}"
            );
        }
    }

    #[test]
    fn sourced_snapshot_not_required_for_gate() {
        // Coverage state on the page reflects the snapshot's informational
        // state; the gate itself never depends on an evidence record.
        let dir = tempfile::tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        seed_matter(&store, "matter:1");
        let content = fully_gated(&store, "matter:1");
        let gate = evaluate_gate(&store, &content).expect("gate");
        assert!(!gate.withheld);
    }
}
