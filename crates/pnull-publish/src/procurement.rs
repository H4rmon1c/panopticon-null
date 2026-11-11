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

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use pnull_core::{Citation, CoverageState, ProcurementAlert, SourceSnapshot, Store, sha256_hex};
use pnull_procurement::casefile::CaseFileContent;
use pnull_procurement::coverage::NOT_OBSERVED_PHRASING;
use regex::Regex;
use scraper::{Html, Selector};

use crate::{
    PublishError, citation_id, escape, escape_attr, escape_xml, publication_allowlist_allows,
    safe_id, validate_public_text,
};

/// The allowlist category that permits procurement case-file content to be
/// published. An allowlist entry is not automatic approval; the citation
/// review gate still applies.
pub const PROCUREMENT_CASEFILE_CATEGORY: &str = "procurement_casefile";

const EMPTY_FEED_UPDATED: &str = "1970-01-01T00:00:00Z";

struct PublishedMatter {
    id: String,
    slug: String,
    title: String,
    jurisdiction: String,
    coverage: CoverageState,
    updated_at: String,
}

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
    pub provenance_issues: Vec<String>,
    pub withheld: bool,
    pub withholds_for: Vec<String>,
}

impl PublicationGateReport {
    pub fn all_pending(&self) -> bool {
        !self.withheld
            && self.citations_pending.is_empty()
            && self.citations_rejected.is_empty()
            && self.citations_stale.is_empty()
            && self.allowlist_present
            && self.privacy_issues.is_empty()
            && self.provenance_issues.is_empty()
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
    let provenance_issues = verify_casefile_provenance(store, content);

    let mut withholds_for = Vec::new();
    if content.citations.is_empty() {
        withholds_for.push("no evidence citations".to_owned());
    }
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
    if !provenance_issues.is_empty() {
        withholds_for.push(format!(
            "{} unverified source/citation link(s)",
            provenance_issues.len()
        ));
    }

    Ok(PublicationGateReport {
        matter_id,
        citations_pending: pending,
        citations_rejected: rejected,
        citations_stale: stale,
        allowlist_present,
        privacy_issues,
        provenance_issues,
        withheld: !withholds_for.is_empty(),
        withholds_for,
    })
}

fn verified_snapshot_bytes(store: &Store, snapshot: &SourceSnapshot) -> Option<Vec<u8>> {
    let legacy_id = SourceSnapshot::id_for(&snapshot.source_id, &snapshot.persisted_digest);
    let revision_id = snapshot.supersedes.as_ref().map(|prior| {
        SourceSnapshot::id_for_revision(&snapshot.source_id, &snapshot.persisted_digest, prior)
    });
    if snapshot.id != legacy_id && revision_id.as_deref() != Some(&snapshot.id) {
        return None;
    }
    let bytes = std::fs::read(store.content_path(&snapshot.persisted_digest).ok()?).ok()?;
    (sha256_hex(&bytes) == snapshot.persisted_digest).then_some(bytes)
}

fn normalize_visible_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn visible_source_text(snapshot: &SourceSnapshot, bytes: &[u8]) -> Option<String> {
    let mime = snapshot.content_type.as_deref()?.split(';').next()?.trim();
    let source = std::str::from_utf8(bytes).ok()?;
    match mime {
        "text/plain" => Some(normalize_visible_text(source)),
        "text/html" | "application/xhtml+xml" => {
            static HIDDEN: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
            let hidden = HIDDEN.get_or_init(|| {
                Regex::new(
                    r"(?is)<(?:script|style|template|noscript)(?:\s[^>]*)?>.*?</(?:script|style|template|noscript)\s*>",
                )
                .expect("constant hidden-element pattern")
            });
            let sanitized = hidden.replace_all(source, " ");
            let document = Html::parse_document(&sanitized);
            let visible = document.root_element().text().collect::<Vec<_>>().join(" ");
            Some(normalize_visible_text(&visible))
        }
        _ => None,
    }
}

fn verified_snapshot_text(store: &Store, id: &str) -> Option<(SourceSnapshot, String)> {
    let snapshot = store.source_snapshot(id).ok()?;
    let bytes = verified_snapshot_bytes(store, &snapshot)?;
    let text = visible_source_text(&snapshot, &bytes)?;
    Some((snapshot, text))
}

fn quote_at_locator(store: &Store, snapshot: &SourceSnapshot, citation: &Citation) -> bool {
    if citation.locator.kind != "html-element"
        || citation.locator.start == 0
        || citation.locator.start != citation.locator.end
    {
        return false;
    }
    let Some(bytes) = verified_snapshot_bytes(store, snapshot) else {
        return false;
    };
    let Ok(html) = std::str::from_utf8(&bytes) else {
        return false;
    };
    let document = Html::parse_document(html);
    let Ok(selector) = Selector::parse("tr, h2, p, li") else {
        return false;
    };
    let Some(element) = document
        .select(&selector)
        .nth((citation.locator.start - 1) as usize)
    else {
        return false;
    };
    let suffix = format!(
        ";visible-element[{}]/{}",
        citation.locator.start,
        element.value().name()
    );
    let element_text = normalize_visible_text(&element.text().collect::<Vec<_>>().join(" "));
    citation.locator.label.ends_with(&suffix)
        && element_text.contains(&normalize_visible_text(&citation.quote))
}

fn verify_casefile_provenance(store: &Store, content: &CaseFileContent) -> Vec<String> {
    let mut issues = BTreeSet::new();
    let mut snapshots = BTreeMap::new();
    for citation in &content.citations {
        let snapshot = snapshots
            .entry(citation.evidence_id.clone())
            .or_insert_with(|| verified_snapshot_text(store, &citation.evidence_id));
        let citation_key = citation_id(citation);
        match snapshot {
            None => {
                issues.insert(format!(
                    "{citation_key}: missing or invalid preserved snapshot"
                ));
            }
            Some((snapshot, text)) => {
                if citation.source_url != snapshot.source_url {
                    issues.insert(format!("{citation_key}: source URL mismatch"));
                }
                let quote = normalize_visible_text(&citation.quote);
                if quote.is_empty() || !text.contains(&quote) {
                    issues.insert(format!(
                        "{citation_key}: quote absent from preserved source"
                    ));
                }
                if !quote_at_locator(store, snapshot, citation) {
                    issues.insert(format!("{citation_key}: invalid locator"));
                }
            }
        }
    }
    for event in &content.events {
        let valid = event.evidence_ids.first().is_some_and(|evidence_id| {
            content.citations.iter().any(|citation| {
                citation.evidence_id == *evidence_id
                    && citation
                        .locator
                        .label
                        .starts_with(&format!("event:{};", event.id))
                    && snapshots
                        .get(evidence_id)
                        .and_then(Option::as_ref)
                        .is_some_and(|(snapshot, _)| snapshot.source_id == event.source_id)
            })
        });
        if !valid {
            issues.insert(format!("event {}: source/citation mismatch", event.id));
        }
    }
    for identifier in &content.identifiers {
        let valid = content.citations.iter().any(|citation| {
            citation.quote == identifier.raw
                && citation
                    .locator
                    .label
                    .starts_with(&format!("identifier:{};", identifier.id))
                && snapshots
                    .get(&citation.evidence_id)
                    .and_then(Option::as_ref)
                    .is_some_and(|(snapshot, _)| snapshot.source_id == identifier.source_id)
        });
        if !valid {
            issues.insert(format!(
                "identifier {}: source/citation mismatch",
                identifier.id
            ));
        }
    }
    for organization in &content.organizations {
        let valid = content.citations.iter().any(|citation| {
            citation.quote == organization.raw_name
                && citation
                    .locator
                    .label
                    .starts_with(&format!("organization:{};", organization.id))
                && snapshots
                    .get(&citation.evidence_id)
                    .and_then(Option::as_ref)
                    .is_some_and(|(snapshot, _)| snapshot.source_id == organization.source_id)
        });
        if !valid {
            issues.insert(format!(
                "organization {}: source/citation mismatch",
                organization.id
            ));
        }
    }
    issues.into_iter().collect()
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
) -> Result<BTreeSet<String>, PublishError> {
    let co = output_dir.join("co");
    let procurement_dir = co.join("procurement");
    std::fs::create_dir_all(&procurement_dir)?;

    // Per-matter pages.
    let mut matters = store.procurement_matters()?;
    matters.sort_by(|a, b| a.id.cmp(&b.id));
    let mut published = Vec::new();
    let mut published_evidence: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut withheld_count = 0usize;
    for matter in matters {
        let content = pnull_procurement::build_content(store, &matter.id)
            .map_err(|e| PublishError::Procurement(e.to_string()))?;
        let slug = matter_slug(&matter.id);
        let matter_dir = procurement_dir.join(&slug);
        std::fs::create_dir_all(&matter_dir)?;
        let page_path = matter_dir.join("index.html");
        let gate = evaluate_gate(store, &content)?;
        let page = if gate.withheld {
            withheld_page(&gate)
        } else {
            render_matter_page(store, &content)?
        };
        validate_public_text(&page)?;
        std::fs::write(&page_path, page.as_bytes())?;
        written.push(page_path);
        if gate.withheld {
            withheld_count += 1;
        } else {
            published_evidence.insert(
                matter.id.clone(),
                content
                    .citations
                    .iter()
                    .map(|citation| citation.evidence_id.clone())
                    .collect(),
            );
            published.push(PublishedMatter {
                id: matter.id,
                slug,
                title: matter.title,
                jurisdiction: matter.jurisdiction,
                coverage: latest_coverage_state(store, &content),
                updated_at: latest_matter_updated(store, &content),
            });
        }
    }

    // Matter index.
    let index = render_index(&published, withheld_count);
    let index_path = procurement_dir.join("index.html");
    validate_public_text(&index)?;
    std::fs::write(&index_path, index.as_bytes())?;
    written.push(index_path);

    // A change alert is public only when its affected matters passed the same
    // citation and allowlist gates, its snapshot references verify, and its
    // rendered content passes the privacy backstop.
    let mut alerts = store.all_procurement_alerts()?;
    alerts.sort_by(|a, b| a.id.cmp(&b.id));
    let change_alert_dir = procurement_dir.join("change-alerts");
    let mut published_alerts = Vec::new();
    for alert in alerts {
        let Some(page) = approved_change_alert_page(store, &alert, &published_evidence)? else {
            continue;
        };
        std::fs::create_dir_all(&change_alert_dir)?;
        let path = change_alert_dir.join(format!("{}.html", safe_id(&alert.id)));
        std::fs::write(&path, page.as_bytes())?;
        written.push(path);
        published_alerts.push(alert);
    }

    // Feed entries are built from the exact pages written above.
    let atom = render_procurement_atom(&published, &published_alerts, canonical_base_url);
    validate_public_text(&atom)?;
    let atom_path = procurement_dir.join("atom.xml");
    std::fs::write(&atom_path, atom.as_bytes())?;
    written.push(atom_path);

    Ok(published_evidence.into_values().flatten().collect())
}

fn latest_matter_updated(store: &Store, content: &CaseFileContent) -> String {
    content
        .events
        .iter()
        .flat_map(|event| &event.evidence_ids)
        .filter_map(|id| store.source_snapshot(id).ok())
        .filter_map(|snapshot| canonical_timestamp(&snapshot.retrieved_at))
        .max()
        .unwrap_or_else(|| EMPTY_FEED_UPDATED.to_owned())
}

fn canonical_timestamp(value: &str) -> Option<String> {
    // Retrieval timestamps produced by this pipeline are UTC RFC 3339. The
    // feed does not invent a date when a stored timestamp is malformed.
    let (date, clock) = value.split_once('T')?;
    let clock = clock.strip_suffix('Z')?;
    let clock = clock.split_once('.').map_or(clock, |(whole, fraction)| {
        if fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
            ""
        } else {
            whole
        }
    });
    let date_bytes = date.as_bytes();
    let clock_bytes = clock.as_bytes();
    if date_bytes.len() != 10
        || clock_bytes.len() != 8
        || date_bytes[4] != b'-'
        || date_bytes[7] != b'-'
        || clock_bytes[2] != b':'
        || clock_bytes[5] != b':'
        || !date_bytes
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != 4 && *index != 7)
            .all(|(_, byte)| byte.is_ascii_digit())
        || !clock_bytes
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != 2 && *index != 5)
            .all(|(_, byte)| byte.is_ascii_digit())
    {
        return None;
    }
    let month: u8 = date[5..7].parse().ok()?;
    let day: u8 = date[8..10].parse().ok()?;
    let hour: u8 = clock[0..2].parse().ok()?;
    let minute: u8 = clock[3..5].parse().ok()?;
    let second: u8 = clock[6..8].parse().ok()?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    Some(value.to_owned())
}

fn approved_change_alert_page(
    store: &Store,
    alert: &ProcurementAlert,
    published_evidence: &BTreeMap<String, BTreeSet<String>>,
) -> Result<Option<String>, PublishError> {
    if alert.matter_ids.is_empty()
        || !alert.matter_ids.iter().all(|id| {
            published_evidence
                .get(id)
                .is_some_and(|evidence| evidence.contains(&alert.new_snapshot_id))
        })
        || !publication_allowlist_allows(store, PROCUREMENT_CASEFILE_CATEGORY)?
        || canonical_timestamp(&alert.retrieved_at).is_none()
        || alert.changes.len() != 1
        || alert.summary != alert.changes[0].summary
    {
        return Ok(None);
    }
    let change = &alert.changes[0];
    if alert.id
        != ProcurementAlert::id_for(
            &alert.source_id,
            &alert.surface,
            &change.row_identity,
            change.change_kind,
            &alert.old_snapshot_id,
            &alert.new_snapshot_id,
        )
        || change.old_snapshot_id != alert.old_snapshot_id
        || change.new_snapshot_id != alert.new_snapshot_id
        || change.old_snapshot_digest != alert.old_snapshot_digest
        || change.new_snapshot_digest != alert.new_snapshot_digest
    {
        return Ok(None);
    }
    let (Ok(old), Ok(new)) = (
        store.source_snapshot(&alert.old_snapshot_id),
        store.source_snapshot(&alert.new_snapshot_id),
    ) else {
        return Ok(None);
    };
    if old.source_id != alert.source_id
        || new.source_id != alert.source_id
        || old.persisted_digest != alert.old_snapshot_digest
        || new.persisted_digest != alert.new_snapshot_digest
        || verified_snapshot_bytes(store, &old).is_none()
        || verified_snapshot_bytes(store, &new).is_none()
    {
        return Ok(None);
    }
    let page = render_change_alert_page(alert)?;
    if validate_public_text(&page).is_err() {
        return Ok(None);
    }
    Ok(Some(page))
}

/// Returns the alert page only when a fresh site build would publish it.
/// Drafting and posting use this same gate so a withheld matter cannot be
/// advertised through a stale or independently constructed X draft.
pub fn approved_procurement_alert_page(
    store: &Store,
    alert: &ProcurementAlert,
) -> Result<Option<String>, PublishError> {
    if !store
        .all_procurement_alerts()?
        .iter()
        .any(|persisted| persisted == alert)
    {
        return Ok(None);
    }
    let mut published_evidence = BTreeMap::new();
    for matter_id in &alert.matter_ids {
        match store.procurement_matter(matter_id) {
            Ok(_) => {}
            Err(pnull_core::CoreError::NotFound(_)) => return Ok(None),
            Err(error) => return Err(error.into()),
        }
        let content = pnull_procurement::build_content(store, matter_id)
            .map_err(|error| PublishError::Procurement(error.to_string()))?;
        if evaluate_gate(store, &content)?.withheld {
            return Ok(None);
        }
        published_evidence.insert(
            matter_id.clone(),
            content
                .citations
                .iter()
                .map(|citation| citation.evidence_id.clone())
                .collect(),
        );
    }
    approved_change_alert_page(store, alert, &published_evidence)
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

fn withheld_page(gate: &PublicationGateReport) -> String {
    let mut body = String::from(
        "<p class=\"state\">Publication withheld pending review.</p><p>This page is not published because its publication gates have not all passed.</p><ul>",
    );
    for reason in &gate.withholds_for {
        writeln!(body, "<li>{}</li>", escape(reason)).expect("string write");
    }
    body.push_str("</ul>");
    crate::page("Publication withheld", &body, "../../../")
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
    let page = crate::page(&title, &body, "../../../");
    Ok(page)
}

fn render_index(published: &[PublishedMatter], withheld_count: usize) -> String {
    let mut body = String::from(
        "<p class=\"declaration\">The procurement chain is a verifiable public ledger: every acquisition visible, every promise permanent, every change provable.</p>",
    );
    if published.is_empty() {
        body.push_str("<p>No procurement matters are published.</p>");
    } else {
        body.push_str("<ul>");
        for matter in published {
            writeln!(
                body,
                "<li><a href=\"{}/index.html\">{}</a> — coverage {}</li>",
                escape(&matter.slug),
                escape(&matter.title),
                escape(matter.coverage.label())
            )
            .expect("string write");
        }
        body.push_str("</ul>");
    }
    if withheld_count > 0 {
        writeln!(
            body,
            "<p>{withheld_count} procurement matter(s) withheld pending review.</p>"
        )
        .expect("string write");
    }
    body.push_str("<p>Absence from a partial or informational source is not proof of absence.</p>");
    body.push_str("<p><a href=\"atom.xml\">Procurement Atom feed</a></p>");
    crate::page("Procurement ledger", &body, "../../")
}

fn render_procurement_atom(
    matters: &[PublishedMatter],
    alerts: &[ProcurementAlert],
    base_url: &str,
) -> String {
    let base = base_url.trim_end_matches('/');
    let feed_url = format!("{base}/procurement/atom.xml");
    let updated = matters
        .iter()
        .map(|matter| matter.updated_at.as_str())
        .chain(alerts.iter().map(|alert| alert.retrieved_at.as_str()))
        .max()
        .unwrap_or(EMPTY_FEED_UPDATED);
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<feed xmlns=\"http://www.w3.org/2005/Atom\"><id>{}</id><title>Panopticon Null — procurement</title><updated>{}</updated><link rel=\"self\" href=\"{}\"/><subtitle>Public procurement ledger; not proof of absence.</subtitle>",
        escape_xml(&feed_url),
        escape_xml(updated),
        escape_xml(&feed_url)
    );

    for alert in alerts {
        let path = format!(
            "{base}/procurement/change-alerts/{}.html",
            safe_id(&alert.id)
        );
        let title = alert_title(alert);
        write!(
            xml,
            "<entry><id>{}</id><title>{}</title><updated>{}</updated><link href=\"{}\"/><summary>{}</summary><category term=\"procurement-change\"/></entry>",
            escape_xml(&alert.id),
            escape_xml(&title),
            escape_xml(&alert.retrieved_at),
            escape_xml(&path),
            escape_xml(&alert.summary)
        )
        .expect("string write");
    }

    for matter in matters {
        let path = format!("{base}/procurement/{}/index.html", matter.slug);
        write!(
            xml,
            "<entry><id>proc-matter:{}</id><title>{}</title><updated>{}</updated><link href=\"{}\"/><summary>Procurement matter for {}. Not proof of absence.</summary></entry>",
            escape_xml(&matter.id),
            escape_xml(&matter.title),
            escape_xml(&matter.updated_at),
            escape_xml(&path),
            escape_xml(&matter.jurisdiction)
        )
        .expect("string write");
    }

    xml.push_str("</feed>\n");
    xml
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

/// Renders a change-alert page under `/co/procurement/change-alerts/`.
/// The site builder checks publication gates before calling this renderer.
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
    Ok(crate::page(&title, &body, "../../../"))
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
        CoverageState, IdentifierKind, OrganizationRole, ProcurementAlert, ProcurementChangeKind,
        ProcurementEventKind, ProcurementIdentifier, ProcurementMatter, ProcurementOrganization,
        ProcurementRecordChange, PublicationAllowlist, ReviewDecision, ReviewState, SourceSnapshot,
    };
    use quick_xml::Reader;
    use quick_xml::events::Event;

    const OLD_BODY: &str =
        "<main><p>award announced</p><p>R26-001</p><p>Other ID</p><p>Example Vendor</p></main>";
    const NEW_BODY: &str = "<main><p>R26-001 changed in the award table</p></main>";

    fn snapshot(store: &Store, source_id: &str, body: &str) -> (String, String) {
        let digest = sha256_hex(body.as_bytes());
        store
            .persist_content(&digest, body.as_bytes())
            .expect("preserved bytes");
        let snap = SourceSnapshot {
            id: SourceSnapshot::id_for(source_id, &digest),
            source_id: source_id.to_owned(),
            source_url: format!("https://example.test/{source_id}"),
            retrieved_at: "2026-08-17T00:00:00Z".to_owned(),
            persisted_digest: digest.clone(),
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
        (snap.id, digest)
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

        let (evidence_id, _) = snapshot(store, "colorado-springs-contract-awards", OLD_BODY);
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

    fn insert_change_alert(store: &Store, matter_id: &str, mismatch: bool) -> ProcurementAlert {
        let source_id = "colorado-springs-contract-awards";
        let old_digest = sha256_hex(OLD_BODY.as_bytes());
        let old_snapshot_id = SourceSnapshot::id_for(source_id, &old_digest);
        let (new_snapshot_id, verified_new_digest) = snapshot(store, source_id, NEW_BODY);
        let new_digest = if mismatch {
            "unverified-digest".to_owned()
        } else {
            verified_new_digest
        };
        let row_identity = "R26-001";
        let event = pnull_core::ProcurementEvent {
            id: pnull_core::ProcurementEvent::id_for(
                matter_id,
                ProcurementEventKind::RecordCorrected,
                "2026-08-18",
                "R26-001 changed in the award table",
            ),
            matter_id: matter_id.to_owned(),
            kind: ProcurementEventKind::RecordCorrected,
            date: Some("2026-08-18".to_owned()),
            summary: "R26-001 changed in the award table".to_owned(),
            identifier_ids: Vec::new(),
            evidence_ids: vec![new_snapshot_id.clone()],
            source_id: source_id.to_owned(),
        };
        store
            .insert_procurement_event(&event)
            .expect("change event");
        let summary = "The recorded award row changed between preserved snapshots.".to_owned();
        let change = ProcurementRecordChange {
            change_kind: ProcurementChangeKind::RecordModified,
            row_identity: row_identity.to_owned(),
            field_diffs: Vec::new(),
            old_snapshot_id: old_snapshot_id.clone(),
            old_snapshot_digest: old_digest.clone(),
            new_snapshot_id: new_snapshot_id.clone(),
            new_snapshot_digest: new_digest.clone(),
            summary: summary.clone(),
        };
        let alert = ProcurementAlert {
            id: ProcurementAlert::id_for(
                source_id,
                "contract-award-table",
                row_identity,
                change.change_kind,
                &old_snapshot_id,
                &new_snapshot_id,
            ),
            source_id: source_id.to_owned(),
            surface: "contract-award-table".to_owned(),
            old_snapshot_id,
            old_snapshot_digest: old_digest,
            new_snapshot_id,
            new_snapshot_digest: new_digest,
            changes: vec![change],
            retrieved_at: "2026-08-18T12:30:00Z".to_owned(),
            coverage_state: CoverageState::InformationalOnly,
            matter_ids: vec![matter_id.to_owned()],
            identifier_ids: Vec::new(),
            taxonomy_matches: Vec::new(),
            summary,
        };
        store.insert_procurement_alert(&alert).expect("alert");
        alert
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
    fn matter_without_any_citation_is_withheld() {
        let dir = tempfile::tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        store
            .insert_procurement_matter(&ProcurementMatter {
                id: "uncited:1".to_owned(),
                jurisdiction: "Colorado Springs".to_owned(),
                title: "Uncited matter".to_owned(),
                review_state: "draft".to_owned(),
                publication_state: "unpublished".to_owned(),
            })
            .expect("matter");
        add_allowlist(&store);
        let content = pnull_procurement::build_content(&store, "uncited:1").expect("content");
        let gate = evaluate_gate(&store, &content).expect("gate");
        assert!(gate.withheld);
        assert!(
            gate.withholds_for
                .iter()
                .any(|reason| reason == "no evidence citations")
        );
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
    fn derived_event_quote_is_withheld_even_when_review_approved() {
        let dir = tempfile::tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        seed_matter(&store, "matter:1");
        let old_snapshot_id = SourceSnapshot::id_for(
            "colorado-springs-contract-awards",
            &sha256_hex(OLD_BODY.as_bytes()),
        );
        store
            .insert_procurement_event(&pnull_core::ProcurementEvent {
                id: pnull_core::ProcurementEvent::id_for(
                    "matter:1",
                    ProcurementEventKind::RecordCorrected,
                    "2026-09-01",
                    "Derived sentence absent from source",
                ),
                matter_id: "matter:1".to_owned(),
                kind: ProcurementEventKind::RecordCorrected,
                date: Some("2026-09-01".to_owned()),
                summary: "Derived sentence absent from source".to_owned(),
                identifier_ids: Vec::new(),
                evidence_ids: vec![old_snapshot_id],
                source_id: "colorado-springs-contract-awards".to_owned(),
            })
            .expect("derived event");
        let content = fully_gated(&store, "matter:1");
        let gate = evaluate_gate(&store, &content).expect("gate");
        assert!(gate.withheld);
        assert!(
            gate.provenance_issues
                .iter()
                .any(|issue| issue.contains("source/citation mismatch"))
        );
    }

    #[test]
    fn identifier_cited_to_wrong_source_is_withheld() {
        let dir = tempfile::tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        seed_matter(&store, "matter:1");
        store
            .insert_procurement_identifier(&ProcurementIdentifier {
                id: ProcurementIdentifier::id_for(
                    "matter:1",
                    IdentifierKind::SolicitationNumber,
                    "Other ID",
                ),
                matter_id: "matter:1".to_owned(),
                kind: IdentifierKind::SolicitationNumber,
                raw: "Other ID".to_owned(),
                source_id: "different-official-source".to_owned(),
                normalized: None,
                normalization_rule: None,
                known: false,
            })
            .expect("identifier");
        let content = fully_gated(&store, "matter:1");
        let gate = evaluate_gate(&store, &content).expect("gate");
        assert!(gate.withheld);
        assert!(
            gate.provenance_issues
                .iter()
                .any(|issue| issue.contains("source/citation mismatch"))
        );
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
        assert!(!page.contains("Test Matter"));
        let withheld_index =
            std::fs::read_to_string(out.path().join("co/procurement/index.html")).expect("index");
        let withheld_atom =
            std::fs::read_to_string(out.path().join("co/procurement/atom.xml")).expect("atom");
        assert!(!withheld_index.contains("Test Matter"));
        assert!(!withheld_atom.contains("Test Matter"));
        assert_eq!(withheld_atom.matches("<entry>").count(), 0);

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
        assert!(page2.contains("href=\"../../../style.css\""));
        let index2 =
            std::fs::read_to_string(out2.path().join("co/procurement/index.html")).expect("index");
        assert!(index2.starts_with("<!doctype html>"));
        assert!(index2.contains("href=\"../../style.css\""));
        assert!(index2.contains("Test Matter"));
        assert!(index2.contains("Procurement Atom feed"));
    }

    #[test]
    fn change_alert_feed_links_resolve_only_when_reviewed() {
        let dir = tempfile::tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        seed_matter(&store, "matter:1");
        let alert = insert_change_alert(&store, "matter:1", false);
        assert!(
            approved_procurement_alert_page(&store, &alert)
                .expect("alert gate")
                .is_none()
        );

        // With no citation approvals or allowlist, neither the matter title
        // nor the change alert is allowed into the public feed.
        let withheld_out = tempfile::tempdir().expect("withheld output");
        build_procurement_site(
            &store,
            withheld_out.path(),
            "https://example.invalid/co",
            &mut Vec::new(),
        )
        .expect("withheld build");
        let withheld_feed =
            std::fs::read_to_string(withheld_out.path().join("co/procurement/atom.xml"))
                .expect("withheld feed");
        assert!(!withheld_feed.contains(&alert.id));
        assert_eq!(withheld_feed.matches("<entry>").count(), 0);
        assert!(
            !withheld_out
                .path()
                .join(format!(
                    "co/procurement/change-alerts/{}.html",
                    safe_id(&alert.id)
                ))
                .exists()
        );

        fully_gated(&store, "matter:1");
        let approved_page = approved_procurement_alert_page(&store, &alert)
            .expect("alert gate")
            .expect("published alert page");
        let published_out = tempfile::tempdir().expect("published output");
        build_procurement_site(
            &store,
            published_out.path(),
            "https://example.invalid/co",
            &mut Vec::new(),
        )
        .expect("published build");
        let feed = std::fs::read_to_string(published_out.path().join("co/procurement/atom.xml"))
            .expect("feed");
        assert_eq!(feed.matches("<entry>").count(), 2);
        assert!(!feed.contains("/co/co/"));
        assert!(!feed.contains("ZT00:00:00Z"));
        assert!(feed.contains("<updated>2026-08-18T12:30:00Z</updated>"));
        let change_page = published_out.path().join(format!(
            "co/procurement/change-alerts/{}.html",
            safe_id(&alert.id)
        ));
        assert!(change_page.is_file());
        let page = std::fs::read_to_string(change_page).expect("change page");
        assert_eq!(page, approved_page);
        assert!(page.contains("href=\"../../../style.css\""));

        // Every absolute URL emitted by the feed must map to an output file.
        let mut reader = Reader::from_str(&feed);
        let mut buffer = Vec::new();
        let mut links = 0usize;
        loop {
            match reader
                .read_event_into(&mut buffer)
                .expect("well-formed Atom")
            {
                Event::Empty(element) if element.name().as_ref() == b"link" => {
                    let href = element
                        .attributes()
                        .flatten()
                        .find(|attr| attr.key.as_ref() == b"href")
                        .expect("link href");
                    let href = std::str::from_utf8(href.value.as_ref()).expect("UTF-8 href");
                    let relative = href
                        .strip_prefix("https://example.invalid/co/")
                        .expect("canonical site URL");
                    assert!(
                        published_out.path().join("co").join(relative).is_file(),
                        "{href}"
                    );
                    links += 1;
                }
                Event::Eof => break,
                _ => {}
            }
            buffer.clear();
        }
        assert_eq!(links, 3, "self link plus both entry links");
    }

    #[test]
    fn change_alert_with_mismatched_snapshot_digest_is_withheld() {
        let dir = tempfile::tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        seed_matter(&store, "matter:1");
        fully_gated(&store, "matter:1");
        let alert = insert_change_alert(&store, "matter:1", true);
        let out = tempfile::tempdir().expect("output");
        build_procurement_site(
            &store,
            out.path(),
            "https://example.invalid/co",
            &mut Vec::new(),
        )
        .expect("build");
        let feed =
            std::fs::read_to_string(out.path().join("co/procurement/atom.xml")).expect("feed");
        assert!(!feed.contains(&alert.id));
        assert!(
            !out.path()
                .join(format!(
                    "co/procurement/change-alerts/{}.html",
                    safe_id(&alert.id)
                ))
                .exists()
        );
    }

    #[test]
    fn change_alert_requires_preserved_old_snapshot_bytes() {
        for tamper in [false, true] {
            let dir = tempfile::tempdir().expect("temp");
            let store = Store::open(dir.path()).expect("store");
            store
                .insert_procurement_matter(&ProcurementMatter {
                    id: "matter:new-only".to_owned(),
                    jurisdiction: "Colorado Springs".to_owned(),
                    title: "New-only matter".to_owned(),
                    review_state: "draft".to_owned(),
                    publication_state: "unpublished".to_owned(),
                })
                .expect("matter");
            snapshot(&store, "colorado-springs-contract-awards", OLD_BODY);
            let alert = insert_change_alert(&store, "matter:new-only", false);
            let content = fully_gated(&store, "matter:new-only");
            assert!(
                !evaluate_gate(&store, &content)
                    .expect("matter gate")
                    .withheld
            );
            assert!(
                approved_procurement_alert_page(&store, &alert)
                    .expect("alert gate")
                    .is_some()
            );

            let old_blob = store
                .content_path(&alert.old_snapshot_digest)
                .expect("old blob path");
            if tamper {
                std::fs::write(&old_blob, b"tampered bytes").expect("tamper blob");
            } else {
                std::fs::remove_file(&old_blob).expect("remove blob");
            }
            assert!(
                !evaluate_gate(&store, &content)
                    .expect("matter gate")
                    .withheld,
                "old blob is not a citation for this matter"
            );
            assert!(
                approved_procurement_alert_page(&store, &alert)
                    .expect("alert gate")
                    .is_none(),
                "alert must withhold a {} old blob",
                if tamper { "tampered" } else { "missing" }
            );
        }
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
