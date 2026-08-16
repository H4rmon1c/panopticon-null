//! Deterministic, JavaScript-free publication with sensitive-data gates.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use pnull_core::{Alert, Citation, CoreError, EvidenceRecord, Store};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PublishError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("site filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("public output rejected by privacy gate: {0}")]
    Sensitive(String),
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
    if output_dir.exists() {
        fs::remove_dir_all(output_dir)?;
    }
    fs::create_dir_all(output_dir.join("alerts"))?;
    fs::create_dir_all(output_dir.join("evidence"))?;
    fs::create_dir_all(output_dir.join("diffs"))?;
    let alerts = store.alerts()?;
    let evidence = store.all_evidence()?;
    let mut written = Vec::new();
    write_public(
        output_dir.join("style.css"),
        include_str!("style.css"),
        &mut written,
    )?;
    write_public(
        output_dir.join("index.html"),
        &page("Current alerts", &alerts_index(&alerts, false), ""),
        &mut written,
    )?;
    write_public(
        output_dir.join("history.html"),
        &page("Alert history", &alerts_index(&alerts, true), ""),
        &mut written,
    )?;
    for alert in &alerts {
        write_public(
            output_dir
                .join("alerts")
                .join(format!("{}.html", safe_id(&alert.id))),
            &page(&alert.title, &alert_page(alert), "../"),
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
            &page(&record.document_title, &evidence_page(record), "../"),
            &mut written,
        )?;
    }
    write_root_documents(output_dir, config, &alerts, &mut written)?;
    written.sort();
    Ok(written)
}

fn write_root_documents(
    output_dir: &Path,
    config: &SiteConfig<'_>,
    alerts: &[Alert],
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

fn alerts_index(alerts: &[Alert], history: bool) -> String {
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

fn alert_page(alert: &Alert) -> String {
    let mut body = format!(
        "<p class=\"state\">{}</p><p>{}</p><dl><dt>Jurisdiction</dt><dd>{}</dd><dt>Date</dt><dd>{}</dd><dt>Rules</dt><dd>{}</dd></dl><h2>Exact citations</h2>{}",
        escape(alert.state.label()),
        escape(&alert.summary),
        escape(&alert.jurisdiction),
        escape(&alert.publication_date),
        escape(&alert.rule_ids.join(", ")),
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
    body.push_str("<p><strong>Constraint:</strong> this alert reports what the cited source says. It does not establish legality, implementation beyond the record, or effects not documented by the source.</p>");
    body
}

fn citations_html(citations: &[Citation]) -> String {
    let mut body = String::from("<ol class=\"citations\">");
    for citation in citations {
        writeln!(body, "<li><blockquote>{}</blockquote><p><a rel=\"external nofollow\" href=\"{}\">Official source</a>, {}</p></li>", escape(&citation.quote), escape_attr(&citation.source_url), escape(&citation.locator.label)).expect("string write");
    }
    body.push_str("</ol>");
    body
}

fn evidence_page(record: &EvidenceRecord) -> String {
    let supersedes = record.supersedes.as_ref().map_or_else(
        || "None".to_owned(),
        |id| format!("<a href=\"{}.html\">{}</a>", safe_id(id), escape(id)),
    );
    format!(
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
    )
}

fn atom_feed(alerts: &[Alert], base_url: &str) -> String {
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
    use quick_xml::Reader;

    #[test]
    fn sensitive_identifiers_are_rejected() {
        assert!(validate_public_text("Plate: ABC123").is_err());
        assert!(validate_public_text("resident@example.com").is_err());
        assert!(validate_public_text("123 Main Street").is_err());
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
}
