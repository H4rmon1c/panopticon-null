//! Parser for the City of Colorado Springs informational solicitation list.
//!
//! This is an informational mirror. The City's own page states the list "may or
//! may not be up to date" and that `BidNet` and Bonfire remain the authoritative
//! versions. Every record produced here carries that warning and never claims to
//! represent every solicitation.

use pnull_core::{
    CoverageState, IdentifierKind, ProcurementIdentifier, SourceAuthority,
};
use regex::Regex;
use scraper::{Html, Selector};
use thiserror::Error;

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
pub fn parse_solicitations(html: &str, snapshot_digest: &str) -> Result<Vec<SolicitationRecord>, SolicitationError> {
    let document = Html::parse_document(html);
    let identifier_re = Regex::new(r"(?i)\b(rfp|rfq|ifb|r|q|b)?-?\s?[0-9]{2}-[0-9]{3}[a-z]{0,3}\b")
        .expect("constant identifier regex");
    let mut records = Vec::new();

    // Collect document links and headings, pairing each heading's text with the
    // links it contains. The identifier may appear in the heading's own text
    // (e.g., "(R26-023AB)") while the linked document lives on a child anchor.
    // This is intentionally bounded and tolerant of layout drift without
    // claiming completeness.
    let heading_selector = Selector::parse("h2, h3, h4").expect("valid");
    let link_selector = Selector::parse("a[href]").expect("valid");
    let mut items: Vec<(String, Vec<String>)> = Vec::new();

    // Standalone anchors outside any heading (e.g., table cells).
    for element in document.select(&link_selector) {
        let href = absolutize(element.value().attr("href").unwrap_or_default());
        if href.is_empty() {
            continue;
        }
        let in_heading = element
            .ancestors()
            .any(|node| matches!(node.value(), scraper::Node::Element(el) if matches!(el.name(), "h2" | "h3" | "h4")));
        if in_heading {
            continue; // handled with its heading below
        }
        let text = normalize(&element.text().collect::<Vec<_>>().join(" "));
        if text.is_empty() {
            continue;
        }
        items.push((text, vec![href]));
    }

    // Headings with their descendant links.
    for element in document.select(&heading_selector) {
        let text = normalize(&element.text().collect::<Vec<_>>().join(" "));
        if text.is_empty() {
            continue;
        }
        let links: Vec<String> = element
            .select(&link_selector)
            .map(|a| absolutize(a.value().attr("href").unwrap_or_default()))
            .filter(|u| !u.is_empty())
            .collect();
        items.push((text, links));
    }

    for (text, links) in items {
        if text.is_empty() && links.is_empty() {
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
        id: ProcurementIdentifier::id_for(
            matter_id,
            record.identifier_kind,
            &record.identifier,
        ),
        matter_id: matter_id.to_owned(),
        kind: record.identifier_kind,
        raw: record.identifier.clone(),
        source_id: "colorado-springs-solicitation-mirror".to_owned(),
        normalized,
        normalization_rule: rule,
        known: false,
    })
}

fn absolutize(href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        href.to_owned()
    } else {
        format!("https://coloradosprings.gov{href}")
    }
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
        assert!(r26
            .linked_documents
            .iter()
            .any(|u| u.contains("r26-023ab")));
        assert_eq!(r26.authority, SourceAuthority::OfficialInformationalMirror);
        assert_eq!(r26.coverage_state, CoverageState::InformationalOnly);
        assert!(r26.incompleteness_warning.contains("may or may not be up to date"));
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
        assert!(r26
            .linked_documents
            .iter()
            .all(|u| u.starts_with("https://coloradosprings.gov")));
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
