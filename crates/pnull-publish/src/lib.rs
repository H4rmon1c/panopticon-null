//! Deterministic, JavaScript-free publication with sensitive-data gates.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use pnull_core::{
    Alert, Citation, CoreError, EvidenceRecord, Matter, PageCitation, ReviewBinding, ReviewState,
    Store, sha256_hex, stable_id,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PublishError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("site filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("public output rejected by privacy gate: {0}")]
    Sensitive(String),
    #[error("public output contains a broken evidence reference: {0}")]
    BrokenReference(String),
    #[error("public claim depends on a citation without a current Approved review: {0}")]
    Review(String),
}

pub struct SiteConfig<'a> {
    pub canonical_base_url: &'a str,
    pub rules_yaml: &'a str,
}

pub fn build_site(
    store: &Store,
    output_dir: impl AsRef<Path>,
    config: &SiteConfig<'_>,
) -> Result<Vec<PathBuf>, PublishError> {
    let output_dir = output_dir.as_ref();
    let temporary = output_dir.with_extension("pnull-building");
    if temporary.exists() {
        fs::remove_dir_all(&temporary)?;
    }
    let temporary_files = build_site_in(store, &temporary, config)?;
    if output_dir.exists() {
        fs::remove_dir_all(output_dir)?;
    }
    fs::rename(&temporary, output_dir)?;
    Ok(temporary_files
        .into_iter()
        .map(|path| output_dir.join(path.strip_prefix(&temporary).unwrap_or(&path)))
        .collect())
}

fn build_site_in(
    store: &Store,
    output_dir: &Path,
    config: &SiteConfig<'_>,
) -> Result<Vec<PathBuf>, PublishError> {
    fs::create_dir_all(output_dir.join("alerts"))?;
    fs::create_dir_all(output_dir.join("evidence"))?;
    fs::create_dir_all(output_dir.join("diffs"))?;
    let alerts = store.alerts()?;
    let evidence = store.all_evidence()?;
    validate_references(&alerts, &evidence)?;
    // Fail closed: only alerts whose line citations are all currently
    // approved may be published. Unapproved alerts are skipped, not fatal.
    let approved_alerts: Vec<&Alert> = alerts
        .iter()
        .filter(|alert| assert_citations_approved(store, &alert.citations).is_ok())
        .collect();
    let mut written = Vec::new();
    write_public(
        output_dir.join("style.css"),
        include_str!("style.css"),
        &mut written,
    )?;
    write_public(
        output_dir.join("index.html"),
        &page("Current alerts", &alerts_index(&approved_alerts, false), ""),
        &mut written,
    )?;
    write_public(
        output_dir.join("history.html"),
        &page("Alert history", &alerts_index(&approved_alerts, true), ""),
        &mut written,
    )?;
    for alert in &approved_alerts {
        write_public(
            output_dir
                .join("alerts")
                .join(format!("{}.html", safe_id(&alert.id))),
            &page(&alert.title, &alert_page(alert, store), "../"),
            &mut written,
        )?;
        if let Some(diff) = &alert.diff {
            let mut body = String::new();
            writeln!(body, "<p><a href=\"../evidence/{}.html\">Earlier evidence</a> → <a href=\"../evidence/{}.html\">Newer evidence</a></p>", safe_id(&diff.old_evidence_id), safe_id(&diff.new_evidence_id)).expect("string write");
            body.push_str("<ul class=\"changes\">");
            for change in &diff.changes {
                writeln!(
                    body,
                    "<li><strong>{}</strong>: {}</li>",
                    escape(&change.kind.replace('_', " ")),
                    escape(&change.summary)
                )
                .expect("string write");
            }
            body.push_str("</ul><h2>Textual differences</h2><pre>");
            body.push_str(&escape(&diff.unified_text));
            body.push_str("</pre><p>This records textual differences. It does not assert a legal violation.</p>");
            write_public(
                output_dir
                    .join("diffs")
                    .join(format!("{}.html", safe_id(&alert.id))),
                &page("Evidence diff", &body, "../"),
                &mut written,
            )?;
        }
    }
    for (record, _) in &evidence {
        write_public(
            output_dir
                .join("evidence")
                .join(format!("{}.html", safe_id(&record.id))),
            &page(&record.document_title, &evidence_page(record, store), "../"),
            &mut written,
        )?;
    }
    write_root_documents(output_dir, config, &approved_alerts, &mut written)?;
    written.sort();
    Ok(written)
}

/// Stable id for a line-based [`Citation`]. Matches the ingest scheme of
/// keying reviews by a content-derived citation id so the same physical
/// citation can be reviewed and gated consistently across crates.
pub fn citation_id(citation: &Citation) -> String {
    stable_id(
        "citation",
        &[
            &citation.evidence_id,
            &citation.quote,
            &citation.locator.label,
        ],
    )
}

/// The `ReviewBinding` an approval must bind for a line-based [`Citation`].
/// Fields a `Citation` cannot express (rule digest, processing artifact,
/// public fields) take stable empty/default values so the binding is a pure,
/// reproducible function of the citation; any change to the cited content
/// (evidence, source URL, locator, or quote) changes the digest and voids a
/// stale approval.
pub fn citation_review_binding(citation: &Citation) -> ReviewBinding {
    ReviewBinding {
        evidence_id: citation.evidence_id.clone(),
        source_digest: sha256_hex(citation.source_url.as_bytes()),
        locator_or_geometry: citation.locator.label.clone(),
        quote: citation.quote.clone(),
        quote_digest: sha256_hex(citation.quote.as_bytes()),
        rule_digest: String::new(),
        processing_artifact_digest: String::new(),
        proposed_public_fields: "quote,locator".to_owned(),
    }
}

/// Fail closed: every citation must have a current `Approved` review whose
/// `bound_digest` matches a freshly computed binding. A missing review, a
/// non-Approved state, or a stale binding (content changed after approval)
/// all refuse publication.
pub fn assert_citations_approved(
    store: &Store,
    citations: &[Citation],
) -> Result<(), PublishError> {
    for citation in citations {
        let id = citation_id(citation);
        let decision = store.current_review(&id)?;
        let Some(decision) = decision else {
            return Err(PublishError::Review(format!(
                "citation {id} has no review decision"
            )));
        };
        if decision.state != ReviewState::Approved {
            return Err(PublishError::Review(format!(
                "citation {id} review state is {:?}, not Approved",
                decision.state
            )));
        }
        let binding = citation_review_binding(citation);
        if decision.bound_digest != binding.digest() {
            return Err(PublishError::Review(format!(
                "citation {id} approval is stale: bound digest no longer matches content"
            )));
        }
    }
    Ok(())
}

/// Whether any publication allowlist actively permits the given field
/// category. An allowlist is not automatic approval (that is handled by the
/// citation review gate); with no matching allowlist the category is refused.
pub fn publication_allowlist_allows(store: &Store, category: &str) -> Result<bool, PublishError> {
    Ok(store.publication_allowlists()?.iter().any(|allowlist| {
        allowlist
            .field_categories
            .iter()
            .any(|item| item == category)
    }))
}

/// Whether the current review for a citation id is `Approved`.
fn citation_id_approved(store: &Store, id: &str) -> Result<bool, PublishError> {
    Ok(match store.current_review(id)? {
        Some(decision) => decision.state == ReviewState::Approved,
        None => false,
    })
}

/// Page citations for an evidence id whose current review is `Approved`.
fn approved_page_citations(
    store: &Store,
    evidence_id: &str,
) -> Result<Vec<PageCitation>, PublishError> {
    let mut approved = Vec::new();
    for citation in store.page_citations(evidence_id)? {
        if citation_id_approved(store, &citation.id)? {
            approved.push(citation);
        }
    }
    Ok(approved)
}

/// Matters whose attachments reference the given evidence id.
fn matters_for_evidence(store: &Store, evidence_id: &str) -> Result<Vec<Matter>, PublishError> {
    let mut matters = Vec::new();
    for matter in store.matters()? {
        for attachment in store.attachments(&matter.id)? {
            if attachment.evidence_id.as_deref() == Some(evidence_id) {
                matters.push(matter);
                break;
            }
        }
    }
    Ok(matters)
}

fn validate_references(
    alerts: &[Alert],
    evidence: &[(EvidenceRecord, String)],
) -> Result<(), PublishError> {
    let ids: BTreeSet<&str> = evidence
        .iter()
        .map(|(record, _)| record.id.as_str())
        .collect();
    for (record, _) in evidence {
        if let Some(supersedes) = record.supersedes.as_deref()
            && !ids.contains(supersedes)
        {
            return Err(PublishError::BrokenReference(supersedes.to_owned()));
        }
    }
    for alert in alerts {
        if !ids.contains(alert.evidence_id.as_str()) {
            return Err(PublishError::BrokenReference(alert.evidence_id.clone()));
        }
        for citation in &alert.citations {
            if !ids.contains(citation.evidence_id.as_str()) {
                return Err(PublishError::BrokenReference(citation.evidence_id.clone()));
            }
        }
    }
    Ok(())
}

fn write_root_documents(
    output_dir: &Path,
    config: &SiteConfig<'_>,
    alerts: &[&Alert],
    written: &mut Vec<PathBuf>,
) -> Result<(), PublishError> {
    for (filename, title, body) in [
        ("methodology.html", "Methodology", METHODOLOGY),
        ("privacy.html", "Data and privacy policy", PRIVACY),
        ("manifesto.html", "Manifesto", MANIFESTO),
        ("legal.html", "Legal and ethical boundaries", LEGAL),
    ] {
        write_public(output_dir.join(filename), &page(title, body, ""), written)?;
    }
    let rules = format!(
        "<p>Rules are deterministic and reviewable. A keyword alone does not establish a purchase.</p><pre>{}</pre>",
        escape(config.rules_yaml)
    );
    write_public(
        output_dir.join("rules.html"),
        &page("Rule taxonomy", &rules, ""),
        written,
    )?;
    write_public(
        output_dir.join("atom.xml"),
        &atom_feed(alerts, config.canonical_base_url),
        written,
    )?;
    Ok(())
}

fn write_public(
    path: PathBuf,
    content: &str,
    written: &mut Vec<PathBuf>,
) -> Result<(), PublishError> {
    validate_public_text(content)?;
    fs::write(&path, content.as_bytes())?;
    written.push(path);
    Ok(())
}

pub fn validate_public_text(text: &str) -> Result<(), PublishError> {
    let uppercase = text.to_uppercase();
    for marker in ["PLATE:", "PLATE NUMBER:", "LICENSE PLATE NUMBER:", "SSN:"] {
        if uppercase.contains(marker) {
            return Err(PublishError::Sensitive(format!(
                "disallowed identifier label {marker}"
            )));
        }
    }
    if contains_email(text) {
        return Err(PublishError::Sensitive(
            "email-like private identifier".to_owned(),
        ));
    }
    if contains_street_address(text) {
        return Err(PublishError::Sensitive(
            "street-address-like value".to_owned(),
        ));
    }
    if contains_person_identifier(text) {
        return Err(PublishError::Sensitive(
            "person-level identifier or movement record".to_owned(),
        ));
    }
    Ok(())
}

fn contains_email(text: &str) -> bool {
    text.match_indices('@').any(|(index, _)| {
        let local: String = text[..index]
            .chars()
            .rev()
            .take_while(|character| {
                character.is_ascii_alphanumeric()
                    || matches!(character, '.' | '_' | '%' | '+' | '-')
            })
            .collect();
        let domain: String = text[index + 1..]
            .chars()
            .take_while(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '.' | '-')
            })
            .collect();
        !local.is_empty()
            && domain.contains('.')
            && !domain.ends_with("example.invalid")
            && !domain.starts_with('.')
    })
}

fn contains_street_address(text: &str) -> bool {
    let words: Vec<&str> = text.split_whitespace().collect();
    words.windows(3).any(|window| {
        window[0]
            .trim_matches(|character: char| !character.is_ascii_digit())
            .parse::<u32>()
            .is_ok()
            && matches!(
                window[2]
                    .trim_matches(|character: char| !character.is_ascii_alphabetic())
                    .to_ascii_lowercase()
                    .as_str(),
                "street" | "st" | "avenue" | "ave" | "road" | "rd" | "boulevard" | "blvd"
            )
    })
}

fn contains_person_identifier(text: &str) -> bool {
    let uppercase = text.to_uppercase();
    if [
        "SOCIAL SECURITY NUMBER:",
        "PHONE NUMBER:",
        "HOME ADDRESS:",
        "MOVEMENT LOG:",
        "LOCATION HISTORY:",
        "LATITUDE:",
        "LONGITUDE:",
    ]
    .iter()
    .any(|marker| uppercase.contains(marker))
    {
        return true;
    }
    text.split_whitespace().any(|token| {
        let token = token
            .trim_matches(|character: char| !character.is_ascii_alphanumeric() && character != '-');
        let groups: Vec<&str> = token.split('-').collect();
        groups.len() == 3
            && groups[0].len() == 3
            && groups[1].len() == 2
            && groups[2].len() == 4
            && groups
                .iter()
                .all(|group| group.bytes().all(|byte| byte.is_ascii_digit()))
    })
}

fn page(title: &str, body: &str, prefix: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{} — Panopticon Null</title><link rel=\"stylesheet\" href=\"{}style.css\"><link rel=\"alternate\" type=\"application/atom+xml\" href=\"{}atom.xml\"></head><body><header><a class=\"brand\" href=\"{}index.html\">PANOPTICON NULL</a><p>No human being is born to be indexed.</p><nav><a href=\"{}index.html\">Alerts</a><a href=\"{}history.html\">History</a><a href=\"{}methodology.html\">Method</a><a href=\"{}rules.html\">Rules</a><a href=\"{}privacy.html\">Privacy</a><a href=\"{}manifesto.html\">Manifesto</a><a href=\"{}legal.html\">Boundaries</a></nav></header><main><h1>{}</h1>{}</main><footer><p>We think, therefore we are free. Evidence before rhetoric; institutions, not private people.</p></footer></body></html>\n",
        escape(title),
        prefix,
        prefix,
        prefix,
        prefix,
        prefix,
        prefix,
        prefix,
        prefix,
        prefix,
        prefix,
        escape(title),
        body
    )
}

fn alerts_index(alerts: &[&Alert], history: bool) -> String {
    let mut body = String::from(
        "<p class=\"declaration\">The machinery of mass surveillance depends on invisibility. This project records what is purchased, what is promised, what changes, and who authorized it.</p>",
    );
    if alerts.is_empty() {
        body.push_str("<p>No evidence-backed alerts have been generated.</p>");
        return body;
    }
    body.push_str("<ol class=\"alerts\">");
    for (index, alert) in alerts.iter().enumerate() {
        if !history && index >= 10 {
            break;
        }
        writeln!(body, "<li><p class=\"state\">{}</p><h2><a href=\"alerts/{}.html\">{}</a></h2><p>{}</p><p><time>{}</time> · {}</p></li>", escape(alert.state.label()), safe_id(&alert.id), escape(&alert.title), escape(&alert.summary), escape(&alert.publication_date), escape(&alert.jurisdiction)).expect("string write");
    }
    body.push_str("</ol>");
    body
}

fn alert_page(alert: &Alert, store: &Store) -> String {
    let mut body = format!(
        "<p class=\"state\">{}</p><p>{}</p><dl><dt>Jurisdiction</dt><dd>{}</dd><dt>Date</dt><dd>{}</dd><dt>Rules</dt><dd>{}</dd><dt>Rule-set provenance</dt><dd>version {} · <code>{}</code></dd></dl><h2>Exact citations</h2>{}",
        escape(alert.state.label()),
        escape(&alert.summary),
        escape(&alert.jurisdiction),
        escape(&alert.publication_date),
        escape(&alert.rule_ids.join(", ")),
        alert.rules_version,
        escape(&alert.rules_digest),
        citations_html(&alert.citations)
    );
    writeln!(
        body,
        "<p><a href=\"../evidence/{}.html\">Evidence metadata and archived digest</a></p>",
        safe_id(&alert.evidence_id)
    )
    .expect("string write");
    if alert.diff.is_some() {
        writeln!(
            body,
            "<p><a href=\"../diffs/{}.html\">Human-readable evidence diff</a></p>",
            safe_id(&alert.id)
        )
        .expect("string write");
    }
    body.push_str(&page_citations_section(store, alert));
    body.push_str(&subjects_actions_section(store, alert));
    body.push_str("<p><strong>Constraint:</strong> this alert reports what the cited source says. It does not establish legality, implementation beyond the record, or effects not documented by the source.</p>");
    body
}

/// "Page-accurate citations": approved page citations for the alert's own
/// evidence and for every evidence its line citations reference.
fn page_citations_section(store: &Store, alert: &Alert) -> String {
    let mut evidence_ids: Vec<String> = alert
        .citations
        .iter()
        .map(|citation| citation.evidence_id.clone())
        .collect();
    if !evidence_ids.contains(&alert.evidence_id) {
        evidence_ids.push(alert.evidence_id.clone());
    }
    evidence_ids.sort();
    evidence_ids.dedup();
    let mut sections = String::new();
    for evidence_id in evidence_ids {
        let approved = match approved_page_citations(store, &evidence_id) {
            Ok(list) => list,
            Err(error) => {
                write!(
                    sections,
                    "<h2>Page-accurate citations</h2><p>Not rendered: {}.</p>",
                    escape(&error.to_string())
                )
                .expect("string write");
                continue;
            }
        };
        if approved.is_empty() {
            continue;
        }
        sections.push_str("<h2>Page-accurate citations</h2><ol class=\"page-citations\">");
        for citation in approved {
            let rects = citation
                .rects
                .iter()
                .map(|rect| {
                    format!(
                        "[{:.1},{:.1} → {:.1},{:.1}]",
                        rect.x_min, rect.y_min, rect.x_max, rect.y_max
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(
                sections,
                "<li><blockquote>{}</blockquote><p>Page {}</p><p>Bounding rects: {}</p><p>Text-map digest <code>{}</code> · evidence digest <code>{}</code></p></li>",
                escape(&citation.quote),
                citation.page_number,
                escape(&rects),
                escape(&citation.text_map_digest),
                escape(&citation.evidence_digest)
            )
            .expect("string write");
        }
        sections.push_str("</ol>");
    }
    sections
}

/// "Subjects and actions" for matters tied to the alert's evidence, gated by
/// the `subject_action` allowlist and by approval of the underlying citations.
fn subjects_actions_section(store: &Store, alert: &Alert) -> String {
    let allowed = match publication_allowlist_allows(store, "subject_action") {
        Ok(allowed) => allowed,
        Err(error) => {
            return format!(
                "<h2>Subjects and actions</h2><p>Not rendered: {}.</p>",
                escape(&error.to_string())
            );
        }
    };
    if !allowed {
        return String::new();
    }
    let matters = match matters_for_evidence(store, &alert.evidence_id) {
        Ok(list) => list,
        Err(error) => {
            return format!(
                "<h2>Subjects and actions</h2><p>Not rendered: {}.</p>",
                escape(&error.to_string())
            );
        }
    };
    if matters.is_empty() {
        return String::new();
    }
    let mut body = String::from("<h2>Subjects and actions</h2>");
    for matter in matters {
        writeln!(body, "<h3>{}</h3>", escape(&matter.title)).expect("string write");
        let subjects = match store.subjects(&matter.id) {
            Ok(list) => list,
            Err(error) => {
                write!(
                    body,
                    "<p>Subjects not rendered: {}.</p>",
                    escape(&error.to_string())
                )
                .expect("string write");
                continue;
            }
        };
        for subject in subjects {
            if !citations_approved(store, &subject.citations) {
                continue;
            }
            let known = if subject.known { "known" } else { "unknown" };
            writeln!(
                body,
                "<p><strong>{}</strong> {} — {} ({})</p>",
                subject.kind.label(),
                escape(&subject.name),
                escape(&subject.detail),
                known
            )
            .expect("string write");
        }
        let actions = match store.actions(&matter.id) {
            Ok(list) => list,
            Err(error) => {
                write!(
                    body,
                    "<p>Actions not rendered: {}.</p>",
                    escape(&error.to_string())
                )
                .expect("string write");
                continue;
            }
        };
        for action in actions {
            if !citations_approved(store, &action.citations) {
                continue;
            }
            let known = if action.known { "known" } else { "unknown" };
            writeln!(
                body,
                "<p><strong>{}</strong> {} ({})</p>",
                action.kind.label(),
                escape(&action.summary),
                known
            )
            .expect("string write");
        }
    }
    body.push_str("<p><em>This reports the subject and action stated in the preserved source; it does not assert legality or an unrecorded relationship.</em></p>");
    body
}

/// Whether every referenced citation id has a current `Approved` review.
fn citations_approved(store: &Store, citation_ids: &[String]) -> bool {
    citation_ids
        .iter()
        .all(|id| citation_id_approved(store, id).unwrap_or(false))
}

fn citations_html(citations: &[Citation]) -> String {
    let mut body = String::from("<ol class=\"citations\">");
    for citation in citations {
        writeln!(body, "<li><blockquote>{}</blockquote><p><a rel=\"external nofollow\" href=\"{}\">Official source</a> · <a href=\"../evidence/{}.html\">local hash and provenance</a> · {}</p></li>", escape(&citation.quote), escape_attr(&citation.source_url), safe_id(&citation.evidence_id), escape(&citation.locator.label)).expect("string write");
    }
    body.push_str("</ol>");
    body
}

fn evidence_page(record: &EvidenceRecord, store: &Store) -> String {
    let supersedes = record.supersedes.as_ref().map_or_else(
        || "None".to_owned(),
        |id| format!("<a href=\"{}.html\">{}</a>", safe_id(id), escape(id)),
    );
    let page_citations = match approved_page_citations(store, &record.id) {
        Ok(list) => list,
        Err(error) => {
            return format!(
                "<dl><dt>Stable evidence identifier</dt><dd><code>{}</code></dd></dl><h2>Page citations</h2><p>Not rendered: {}.</p>",
                escape(&record.id),
                escape(&error.to_string())
            );
        }
    };
    let mut body = format!(
        "<dl><dt>Stable evidence identifier</dt><dd><code>{}</code></dd><dt>Jurisdiction</dt><dd>{}</dd><dt>Official source</dt><dd><a rel=\"external nofollow\" href=\"{}\">{}</a></dd><dt>Source type</dt><dd>{:?}</dd><dt>Publication date</dt><dd>{}</dd><dt>Retrieval timestamp</dt><dd>{}</dd><dt>MIME type</dt><dd>{}</dd><dt>SHA-256 of original bytes</dt><dd><code>{}</code></dd><dt>Original filename</dt><dd>{}</dd><dt>Extraction</dt><dd>{} ({:?})</dd><dt>Supersedes</dt><dd>{}</dd><dt>Processing version</dt><dd>{}</dd></dl><p>The original is retained locally by content digest. Public output omits raw documents when republication would expose unnecessary personal data.</p>",
        escape(&record.id),
        escape(&record.jurisdiction),
        escape_attr(&record.source_url),
        escape(&record.source_url),
        record.source_type,
        escape(record.publication_date.as_deref().unwrap_or("Not stated")),
        escape(&record.retrieval_timestamp),
        escape(&record.mime_type),
        escape(&record.sha256),
        escape(&record.original_filename),
        escape(&record.extraction_method),
        record.extraction_status,
        supersedes,
        escape(&record.processing_version)
    );
    if page_citations.is_empty() {
        return body;
    }
    body.push_str("<h2>Page citations</h2><ol class=\"page-citations\">");
    for citation in page_citations {
        writeln!(
            body,
            "<li><blockquote>{}</blockquote><p>Page {}</p><p>Text-map digest <code>{}</code> · evidence digest <code>{}</code> · review status: approved</p></li>",
            escape(&citation.quote),
            citation.page_number,
            escape(&citation.text_map_digest),
            escape(&citation.evidence_digest)
        )
        .expect("string write");
    }
    body.push_str("</ol>");
    body
}

fn atom_feed(alerts: &[&Alert], base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    let updated = alerts
        .first()
        .map_or("1970-01-01", |alert| alert.publication_date.as_str());
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<feed xmlns=\"http://www.w3.org/2005/Atom\"><id>{}</id><title>Panopticon Null — Colorado alerts</title><updated>{}T00:00:00Z</updated><link rel=\"self\" href=\"{}/atom.xml\"/><subtitle>No human being is born to be indexed.</subtitle>",
        escape_xml(base),
        escape_xml(updated),
        escape_xml(base)
    );
    for alert in alerts {
        let path = format!("{base}/alerts/{}.html", safe_id(&alert.id));
        write!(xml, "<entry><id>{}</id><title>{}</title><updated>{}T00:00:00Z</updated><link href=\"{}\"/><summary>{}</summary><category term=\"{}\"/></entry>", escape_xml(&alert.id), escape_xml(&alert.title), escape_xml(&alert.publication_date), escape_xml(&path), escape_xml(&alert.summary), escape_xml(alert.state.label())).expect("string write");
    }
    xml.push_str("</feed>\n");
    xml
}

fn safe_id(id: &str) -> String {
    id.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn escape_attr(input: &str) -> String {
    escape(input)
}

fn escape_xml(input: &str) -> String {
    escape(input)
}

const METHODOLOGY: &str = r"<p>Panopticon Null preserves the exact downloaded bytes, hashes them with SHA-256, extracts text under resource limits, applies published rules, and stores exact line citations. Classification is deterministic. A term match creates at most a mention unless a separate cited phrase establishes a stronger state.</p><h2>Observed, inferred, unknown</h2><ul><li><strong>Observed:</strong> exact text in a linked, hashed public source.</li><li><strong>Inferred:</strong> a deterministic classification whose reason and source span are shown.</li><li><strong>Unknown:</strong> facts the source does not establish, including legality, implementation, effectiveness, or unstated terms.</li></ul><p>Differences are textual findings, not legal conclusions. Possible policy discrepancies require human review.</p>";
const PRIVACY: &str = r"<p>Public availability does not automatically justify republishing personal data. Raw sensitive logs remain local. Public output contains only what is necessary to establish institutional conduct. Plate numbers, movement histories, home addresses, private identities, and sensitive free text are rejected or omitted.</p><p>This project monitors institutions and contracts, not private citizens. It creates no dossiers on activists, officers, employees, or residents. It performs no facial recognition and no person-level movement analysis.</p>";
const MANIFESTO: &str = r"<blockquote><p>We think, therefore we are free. No corporation or state acquires moral title to the inner life, movements, associations, or ordinary existence of a human person merely because technology makes collection cheap.</p></blockquote><p>Panopticon Null opposes systems that convert ordinary life into permanent dossiers. Surveillance thrives through secrecy, complexity, institutional inertia, and the assumption that resistance is futile. We make it visible, legible, contestable, and politically expensive.</p><p>We do this through lawful public records, reproducible analysis, source-linked evidence, privacy-preserving publication, and free software available to everyone.</p>";
const LEGAL: &str = r"<p>Lawfully dismantle the global surveillance panopticon by making every acquisition visible, every promise permanent, every abuse provable, and every community capable of saying no.</p><ul><li>No unauthorized access, bypass of controls, scraping of private systems, harassment, or doxxing.</li><li>No damaging, disabling, evading, or interfering with equipment.</li><li>No surveillance of private people, facial recognition, or movement analysis.</li><li>No claim that a textual difference is a legal violation.</li><li>No legal conclusion; potential rule or policy discrepancies are for human review.</li></ul><p>The purpose is to constrain the panopticon, not recreate it with different operators.</p>";

#[cfg(test)]
mod tests {
    use super::*;
    use pnull_core::{
        FindingState, Locator, PublicationAllowlist, ReviewDecision, ReviewState, SourceType,
    };
    use quick_xml::Reader;
    use tempfile::tempdir;

    /// A valid evidence id (`evidence:<sha256>`), as required by the store.
    fn evid(seed: &str) -> String {
        format!("evidence:{}", sha256_hex(seed.as_bytes()))
    }

    fn test_citation(evidence_id: &str, quote: &str, label: &str) -> Citation {
        Citation {
            evidence_id: evidence_id.to_owned(),
            source_url: format!("https://example.test/{evidence_id}"),
            locator: Locator {
                kind: "line".to_owned(),
                start: 1,
                end: 2,
                label: label.to_owned(),
            },
            quote: quote.to_owned(),
        }
    }

    fn test_evidence(id: &str) -> EvidenceRecord {
        EvidenceRecord {
            id: id.to_owned(),
            jurisdiction: "Colorado Springs, Colorado".to_owned(),
            source_url: format!("https://example.test/{id}"),
            source_type: SourceType::Agenda,
            document_title: "Agenda".to_owned(),
            publication_date: Some("2025-01-01".to_owned()),
            retrieval_timestamp: "2025-01-01T00:00:00Z".to_owned(),
            mime_type: "text/plain".to_owned(),
            sha256: "00".repeat(32),
            original_filename: "agenda.txt".to_owned(),
            extraction_method: "test".to_owned(),
            extraction_status: pnull_core::ExtractionStatus::Complete,
            extraction_error: None,
            locators: Vec::new(),
            matched_rule_ids: Vec::new(),
            quoted_source_spans: Vec::new(),
            supersedes: None,
            processing_version: "test".to_owned(),
        }
    }

    fn test_alert(id: &str, evidence_id: &str, citations: Vec<Citation>) -> Alert {
        Alert {
            id: id.to_owned(),
            jurisdiction: "Colorado Springs, Colorado".to_owned(),
            evidence_id: evidence_id.to_owned(),
            previous_evidence_id: None,
            title: format!("Alert {id}"),
            state: FindingState::MentionDetected,
            summary: "A purchase was mentioned.".to_owned(),
            publication_date: "2025-01-01".to_owned(),
            rule_ids: vec!["vendor.axon".to_owned()],
            rules_version: 1,
            rules_digest: "rules".to_owned(),
            citations,
            diff: None,
        }
    }

    /// Seeds a current `Approved` review for a citation using the same binding
    /// `assert_citations_approved` computes.
    fn approve(store: &Store, citation: &Citation) {
        let id = citation_id(citation);
        let decision = ReviewDecision {
            id: ReviewDecision::id_for(&id, "2025-01-01T00:00:00Z"),
            citation_id: id.clone(),
            state: ReviewState::Approved,
            reviewer: "test-reviewer".to_owned(),
            note: String::new(),
            bound_digest: citation_review_binding(citation).digest(),
            decision_digest: String::new(),
            decided_at: "2025-01-01T00:00:00Z".to_owned(),
            supersedes: None,
        };
        assert!(store.insert_review(&decision).expect("insert review"));
    }

    fn config() -> SiteConfig<'static> {
        SiteConfig {
            canonical_base_url: "https://example.invalid/pnull",
            rules_yaml: "version: 1\nrules: []\n",
        }
    }

    fn seed_site() -> (tempfile::TempDir, Store) {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("store");
        let a = evid("a");
        let b = evid("b");
        store
            .insert_evidence(&test_evidence(&a), "text a")
            .expect("evidence a");
        store
            .insert_evidence(&test_evidence(&b), "text b")
            .expect("evidence b");
        let approved = test_alert(
            "alert:approved",
            &a,
            vec![test_citation(&a, "quote a", "L1")],
        );
        let unapproved = test_alert(
            "alert:unapproved",
            &b,
            vec![test_citation(&b, "quote b", "L1")],
        );
        approve(&store, &approved.citations[0]);
        store
            .insert_alert(&approved)
            .expect("insert approved alert");
        store
            .insert_alert(&unapproved)
            .expect("insert unapproved alert");
        (dir, store)
    }

    #[test]
    fn sensitive_identifiers_are_rejected() {
        assert!(validate_public_text("Plate: ABC123").is_err());
        assert!(validate_public_text("resident@example.com").is_err());
        assert!(validate_public_text("123 Main Street").is_err());
        assert!(validate_public_text("Social Security Number: 123-45-6789").is_err());
        assert!(validate_public_text("Movement log: 39.7,-104.9").is_err());
    }

    #[test]
    fn institutional_terms_are_allowed() {
        assert!(validate_public_text("Automated license plate reader contract").is_ok());
    }

    #[test]
    fn atom_is_well_formed_xml() {
        let xml = atom_feed(&[], "https://example.invalid/pnull");
        let mut reader = Reader::from_str(&xml);
        let mut buffer = Vec::new();
        loop {
            match reader.read_event_into(&mut buffer) {
                Ok(quick_xml::events::Event::Eof) => break,
                Ok(_) => {}
                Err(error) => panic!("invalid Atom: {error}"),
            }
            buffer.clear();
        }
    }

    #[test]
    fn unreviewed_citation_is_refused_for_publication() {
        let (_dir, store) = seed_site();
        // A citation with no review decision must fail the gate.
        let citation = test_citation(&evid("unreviewed"), "never reviewed quote", "L1");
        assert!(assert_citations_approved(&store, &[citation]).is_err());
    }

    #[test]
    fn approved_citation_passes_gate() {
        let (_dir, store) = seed_site();
        // seed_site already approved "quote a"; the gate must pass for it.
        let citation = test_citation(&evid("a"), "quote a", "L1");
        assert!(assert_citations_approved(&store, &[citation]).is_ok());
    }

    #[test]
    fn changed_content_invalidates_approval() {
        let (_dir, store) = seed_site();
        let original = test_citation(&evid("a"), "original quote", "L1");
        approve(&store, &original);
        let changed = test_citation(&evid("a"), "changed quote", "L1");
        assert!(assert_citations_approved(&store, &[changed]).is_err());
    }

    #[test]
    fn build_site_skips_alerts_without_approved_citations() {
        let (_dir, store) = seed_site();
        let out = tempdir().expect("tempdir");
        let paths = build_site(&store, out.path(), &config()).expect("build succeeds");
        assert!(out.path().join("alerts/alert_approved.html").exists());
        assert!(!out.path().join("alerts/alert_unapproved.html").exists());
        assert!(
            paths
                .iter()
                .any(|path| path.ends_with("alerts/alert_approved.html"))
        );
        assert!(
            !paths
                .iter()
                .any(|path| path.ends_with("alerts/alert_unapproved.html"))
        );
    }

    #[test]
    fn atom_feed_omits_unapproved_alerts() {
        let (_dir, store) = seed_site();
        let out = tempdir().expect("tempdir");
        build_site(&store, out.path(), &config()).expect("build succeeds");
        let xml = fs::read_to_string(out.path().join("atom.xml")).expect("read atom");
        assert!(xml.contains("Alert alert:approved"));
        assert!(!xml.contains("Alert alert:unapproved"));
    }

    #[test]
    fn no_script_in_public_output() {
        let (_dir, store) = seed_site();
        let out = tempdir().expect("tempdir");
        build_site(&store, out.path(), &config()).expect("build succeeds");
        for entry in walkdir_files(out.path()) {
            let content = fs::read_to_string(&entry).expect("read file");
            assert!(
                !content.contains("<script"),
                "file {} contains a script element",
                entry.display()
            );
        }
    }

    fn walkdir_files(root: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).expect("read dir") {
                let entry = entry.expect("dir entry");
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    files.push(path);
                }
            }
        }
        files
    }

    #[test]
    fn publication_allowlist_gates_categories() {
        let (_dir, store) = seed_site();
        assert!(!publication_allowlist_allows(&store, "subject_action").unwrap());
        let allowlist = PublicationAllowlist {
            id: "allowlist:1".to_owned(),
            field_categories: vec!["subject_action".to_owned()],
            created_at: "2025-01-01T00:00:00Z".to_owned(),
            note: String::new(),
        };
        store
            .insert_publication_allowlist(&allowlist)
            .expect("insert allowlist");
        assert!(publication_allowlist_allows(&store, "subject_action").unwrap());
        assert!(!publication_allowlist_allows(&store, "page_citation").unwrap());
    }
}
