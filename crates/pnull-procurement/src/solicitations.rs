//! Parser for the City of Colorado Springs informational solicitation list.
//!
//! This is an informational mirror. The City's own page states the list "may or
//! may not be up to date" and that `BidNet` and Bonfire remain the authoritative
//! versions. Every record produced here carries that warning and never claims to
//! represent every solicitation.

use pnull_core::{CoverageState, IdentifierKind, ProcurementIdentifier, SourceAuthority};
use regex::Regex;
use scraper::{Html, Selector};
use thiserror::Error;

use crate::RecordRow;

/// The source's own incompleteness warning, preserved verbatim and carried on
/// every record and in every case-file coverage summary.
pub const INCOMPLETENESS_WARNING: &str = "Solicitations are provided on this page for information \
purposes only. RFP's and IFB's listed here may or may not be up to date. BidNet (Rocky Mountain \
E-Purchasing System) and Bonfire Interactive Procurement Portal remain the authoritative and valid \
version for procurement purposes. Not all open RFP's and IFB's are listed yet.";

/// The user-facing phrasing for an absence in this mirror.
pub const ABSENCE_PHRASING: &str = "Not observed in the checked sources.";

#[derive(Debug, Error)]
pub enum SolicitationError {
    #[error("solicitation list not found in source document")]
    ListNotFound,
}

/// One solicitation mirror record with its linked City-hosted documents.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SolicitationRecord {
    pub title: String,
    /// The identifier as written (e.g., `R26-023AB`), if present.
    pub identifier: String,
    pub identifier_kind: IdentifierKind,
    /// Linked City-hosted document URLs (PDFs, etc.).
    pub linked_documents: Vec<String>,
    pub authority: SourceAuthority,
    pub coverage_state: CoverageState,
    /// The mirror's own warning that the list may be incomplete or outdated.
    pub incompleteness_warning: String,
    pub snapshot_digest: String,
}

/// Extracts solicitation records and their linked City-hosted documents.
///
/// The `snapshot_digest` binds each record to the immutable snapshot it came
/// from. Embedded links are recorded but never automatically followed.
pub fn parse_solicitations(
    html: &str,
    snapshot_digest: &str,
) -> Result<Vec<SolicitationRecord>, SolicitationError> {
    let document = Html::parse_document(html);
    let identifier_re =
        Regex::new(r"(?i)\b(?:rfp|rfq|ifb|rfi|r|q|b)?-?[0-9]{2,4}-[0-9]{3}[a-z]{0,3}\b")
            .expect("constant identifier regex");
    let mut records: Vec<SolicitationRecord> = Vec::new();

    // Bound parsing to the main content where available. A link to a related
    // document is not itself a solicitation, and page navigation must never
    // become one merely because it is an anchor.
    let main_selector = Selector::parse("main").expect("valid");
    let container = document
        .select(&main_selector)
        .next()
        .unwrap_or_else(|| document.root_element());
    let link_selector = Selector::parse("a[href]").expect("valid");
    let candidate_selector = Selector::parse("h2, h3, h4, a[href]").expect("valid");
    let mut current_record: Option<usize> = None;
    for element in container.select(&candidate_selector) {
        let is_heading = matches!(element.value().name(), "h2" | "h3" | "h4");
        if !is_heading {
            let nested_under_heading = element.ancestors().any(|node| {
                matches!(node.value(), scraper::Node::Element(el) if matches!(el.name(), "h2" | "h3" | "h4"))
            });
            if nested_under_heading {
                continue;
            }
        }
        let text = normalize(&element.text().collect::<Vec<_>>().join(" "));
        let links: Vec<String> = if is_heading {
            element
                .select(&link_selector)
                .map(|a| absolutize(a.value().attr("href").unwrap_or_default()))
                .filter(|u| !u.is_empty())
                .collect()
        } else {
            let href = absolutize(element.value().attr("href").unwrap_or_default());
            if href.is_empty() {
                continue;
            }
            vec![href]
        };
        let standalone_record = !is_heading
            && identifier_re.is_match(&text)
            && element.ancestors().any(|node| {
                matches!(node.value(), scraper::Node::Element(el) if matches!(el.name(), "li" | "td" | "tr"))
            });
        if !is_heading && !standalone_record {
            if let Some(index) = current_record {
                records[index].linked_documents.extend(links);
            }
            continue;
        }
        if text.is_empty() {
            continue;
        }
        let identifier = identifier_re
            .find(&text)
            .map(|m| normalize(m.as_str()))
            .unwrap_or_default();
        let title = if identifier.is_empty() {
            text.clone()
        } else {
            // Prefer the surrounding title text without the bare identifier.
            text.replace(&identifier, "").trim().to_owned()
        };
        if title.is_empty() && identifier.is_empty() && links.is_empty() {
            continue;
        }
        let kind = if identifier.to_ascii_uppercase().starts_with("RFP") {
            IdentifierKind::Rfp
        } else if identifier.to_ascii_uppercase().starts_with("RFQ") {
            IdentifierKind::Rfq
        } else if identifier.to_ascii_uppercase().starts_with("IFB") {
            IdentifierKind::Ifb
        } else if !identifier.is_empty() {
            IdentifierKind::SolicitationNumber
        } else {
            IdentifierKind::Unknown
        };
        records.push(SolicitationRecord {
            title,
            identifier,
            identifier_kind: kind,
            linked_documents: links,
            authority: SourceAuthority::OfficialInformationalMirror,
            coverage_state: CoverageState::InformationalOnly,
            incompleteness_warning: INCOMPLETENESS_WARNING.to_owned(),
            snapshot_digest: snapshot_digest.to_owned(),
        });
        current_record = Some(records.len() - 1);
    }

    for record in &mut records {
        record.linked_documents.sort();
        record.linked_documents.dedup();
    }

    if records.is_empty() {
        return Err(SolicitationError::ListNotFound);
    }
    Ok(records)
}

/// Builds a procurement identifier for a solicitation mirror record.
pub fn solicitation_identifier(
    matter_id: &str,
    record: &SolicitationRecord,
) -> Option<ProcurementIdentifier> {
    if record.identifier.is_empty() {
        return None;
    }
    let (normalized, rule) = match pnull_core::normalize_identifier(&record.identifier) {
        Some((key, rule)) => (Some(key), Some(rule.to_owned())),
        None => (None, None),
    };
    Some(ProcurementIdentifier {
        id: ProcurementIdentifier::id_for(matter_id, record.identifier_kind, &record.identifier),
        matter_id: matter_id.to_owned(),
        kind: record.identifier_kind,
        raw: record.identifier.clone(),
        source_id: "colorado-springs-solicitation-mirror".to_owned(),
        normalized,
        normalization_rule: rule,
        known: false,
    })
}

/// Converts solicitation records to snapshot `RecordRow`s for a record-level
/// diff and persistence. The key is the official identifier when present,
/// otherwise a digest over the title and linked documents.
pub fn solicitation_record_rows(records: &[SolicitationRecord]) -> Vec<RecordRow> {
    records
        .iter()
        .map(|record| {
            let canonical = format!(
                "{}\u{1f}{}\u{1f}{}",
                record.identifier,
                record.title,
                record.linked_documents.join("\u{1f}")
            );
            let key = if record.identifier.trim().is_empty() {
                pnull_core::sha256_hex(canonical.as_bytes())
            } else {
                record.identifier.clone()
            };
            RecordRow {
                key,
                canonical,
                raw_json: serde_json::to_string(record).unwrap_or_default(),
            }
        })
        .collect()
}

fn absolutize(href: &str) -> String {
    let href = href.trim();
    if href.is_empty() || href.starts_with('#') {
        return String::new();
    }
    let base =
        url::Url::parse("https://coloradosprings.gov/solicitations").expect("constant City URL");
    let Ok(link) = base.join(href) else {
        return String::new();
    };
    if !matches!(link.scheme(), "http" | "https")
        || link.host_str() != Some("coloradosprings.gov")
        || !link.username().is_empty()
        || link.password().is_some()
    {
        return String::new();
    }
    link.to_string()
}

fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIGEST: &str = "d41d8cd98f00b204e9800998ecf8427e";

    fn page() -> String {
        String::from(
            "<html><body><h1>Solicitations</h1>\
             <h2><a href=\"/document/r26-023ab-next-generation-transit-fare-collection-system.pdf\">\
             Next-Generation Transit Fare Collection System RFI</a> (R26-023AB)</h2>\
             <p>info</p>\
             <h2>Roadway Improvements IFB (IFB-2024-001)</h2>\
             </body></html>",
        )
    }

    #[test]
    fn extracts_identifier_and_linked_documents() {
        let records = parse_solicitations(&page(), DIGEST).expect("parse");
        assert!(!records.is_empty());
        let r26 = records
            .iter()
            .find(|r| r.identifier.eq_ignore_ascii_case("R26-023AB"))
            .expect("r26 record");
        assert!(r26.title.contains("Transit Fare Collection"));
        assert!(r26.linked_documents.iter().any(|u| u.contains("r26-023ab")));
        assert_eq!(r26.authority, SourceAuthority::OfficialInformationalMirror);
        assert_eq!(r26.coverage_state, CoverageState::InformationalOnly);
        assert!(
            r26.incompleteness_warning
                .contains("may or may not be up to date")
        );
    }

    #[test]
    fn every_record_carries_the_incompleteness_warning() {
        let records = parse_solicitations(&page(), DIGEST).expect("parse");
        for record in records {
            assert!(record.incompleteness_warning.contains("authoritative"));
            assert!(record.incompleteness_warning.contains("Not all open"));
        }
    }

    #[test]
    fn linked_urls_are_recorded_but_not_followed() {
        let records = parse_solicitations(&page(), DIGEST).expect("parse");
        let r26 = records
            .iter()
            .find(|r| r.identifier.eq_ignore_ascii_case("R26-023AB"))
            .expect("r26 record");
        assert!(
            r26.linked_documents
                .iter()
                .all(|u| u.starts_with("https://coloradosprings.gov"))
        );
    }

    #[test]
    fn only_city_document_links_are_retained() {
        let html = "<main><h2>Street Repair IFB (IFB-2024-001)</h2>\
                    <p><a href=\"document/specification.pdf\">Specification</a>\
                    <a href=\"https://outside.example/bid.pdf\">Other portal</a>\
                    <a href=\"mailto:buyer@example.com\">Email buyer</a>\
                    <a href=\"#details\">Details</a></p></main>";
        let records = parse_solicitations(html, DIGEST).expect("parse");
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].linked_documents,
            ["https://coloradosprings.gov/document/specification.pdf"]
        );
    }

    #[test]
    fn related_documents_do_not_become_solicitations() {
        let fixture = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/procurement/solicitations.html"
        ));
        let records = parse_solicitations(fixture, DIGEST).expect("parse fixture");
        assert_eq!(records.len(), 3);
        let transit = records
            .iter()
            .find(|record| record.identifier == "R26-023AB")
            .expect("transit record");
        assert_eq!(transit.linked_documents.len(), 3);
        assert!(
            transit
                .linked_documents
                .iter()
                .any(|link| link.contains("sample-services-contract"))
        );
        assert!(
            records
                .iter()
                .any(|record| record.identifier == "IFB-2024-001")
        );
    }

    #[test]
    fn missing_list_is_an_error_not_a_claim() {
        assert!(matches!(
            parse_solicitations("<html><body>nothing</body></html>", DIGEST),
            Err(SolicitationError::ListNotFound)
        ));
    }

    #[test]
    fn hostile_malformed_html_does_not_panic() {
        for case in ["<html", "<a href", "<h2>", "", "<table>", "<p><a>"] {
            let _ = parse_solicitations(case, DIGEST);
        }
    }
}
