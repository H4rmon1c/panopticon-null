//! Gap-driven Colorado Open Records Act (CORA) draft generation.
//!
//! A CORA draft is produced only from unresolved evidence gaps: known
//! institution, solicitation/contract identifiers, the specific missing record
//! types, a narrow date range, and the public sources already checked. The
//! output is Markdown or plain text only. It is never sent, never guesses an
//! email recipient, never claims a legal deadline or entitlement unless
//! supported by reviewed project documentation, and always states that
//! operator/legal review is required. It avoids requesting person-level data
//! unless directly necessary and lawfully justified.

use pnull_core::{CoraDraft, ProcurementMatter, ProcurementIdentifier};
use std::fmt::Write as _;
use thiserror::Error;

use crate::coverage::NOT_OBSERVED_PHRASING;

#[derive(Debug, Error)]
pub enum CoraError {
    #[error("procurement matter {0} not found")]
    MatterNotFound(String),
}

/// The mandatory review statement appended to every draft.
pub const REVIEW_REQUIRED: &str =
    "This is a locally generated draft for operator and legal review. It has not been \
     submitted, and no recipient has been selected. Verify the current law, the correct \
     recipient, and the sufficiency of the request before any use.";

/// The institutions known to hold Colorado Springs procurement records.
pub const CITY_DEPARTMENT: &str = "City of Colorado Springs";

/// Builds the content of a CORA draft from unresolved gaps in a matter.
///
/// `missing_record_types` are the specific record kinds that are absent from the
/// checked sources. `date_range` is a narrow (start, end) window. Only record
/// kinds supplied here are requested; nothing is invented.
pub fn build_draft(
    matter: &ProcurementMatter,
    identifiers: &[ProcurementIdentifier],
    missing_record_types: &[&str],
    date_range: Option<(Option<String>, Option<String>)>,
    vendor_or_project: Option<&str>,
    sources_checked: &[&str],
) -> CoraDraft {
    let created_at = "2026-08-17T00:00:00Z";
    let markdown = render_markdown(
        matter,
        identifiers,
        missing_record_types,
        &date_range,
        vendor_or_project,
        sources_checked,
    );
    // Deduplicate raw identifiers while preserving first-seen order.
    let mut seen = std::collections::BTreeSet::new();
    let mut identifier_list = Vec::new();
    for id in identifiers {
        if seen.insert(id.raw.clone()) {
            identifier_list.push(id.raw.clone());
        }
    }
    CoraDraft {
        id: CoraDraft::id_for(&matter.id, created_at),
        matter_id: matter.id.clone(),
        institution: CITY_DEPARTMENT.to_owned(),
        identifiers: identifier_list,
        missing_record_types: missing_record_types.iter().map(|s| (*s).to_owned()).collect(),
        date_range,
        vendor_or_project: vendor_or_project.map(str::to_owned),
        sources_checked: sources_checked.iter().map(|s| (*s).to_owned()).collect(),
        markdown,
        created_at: created_at.to_owned(),
    }
}

/// Renders the draft as deterministic Markdown (also valid plain text).
pub fn render_markdown(
    matter: &ProcurementMatter,
    identifiers: &[ProcurementIdentifier],
    missing_record_types: &[&str],
    date_range: &Option<(Option<String>, Option<String>)>,
    vendor_or_project: Option<&str>,
    sources_checked: &[&str],
) -> String {
    let mut out = String::new();
    out.push_str("# Draft Colorado Open Records Act Request\n\n");
    out.push_str("**Status:** local draft only; not submitted.\n\n");

    out.push_str("## Requesting institution / department\n\n");
    let _ = writeln!(out, "- {CITY_DEPARTMENT}\n");

    out.push_str("## Known solicitation or contract identifiers\n\n");
    if identifiers.is_empty() {
        let _ = writeln!(out, "- {NOT_OBSERVED_PHRASING}\n");
    } else {
        for id in identifiers {
            let _ = writeln!(out, "- `{}` ({})", id.raw, id.kind.label());
        }
        out.push('\n');
    }

    out.push_str("## Specific records requested\n\n");
    if missing_record_types.is_empty() {
        let _ = writeln!(out, "- {NOT_OBSERVED_PHRASING}\n");
    } else {
        for record in missing_record_types {
            let _ = writeln!(out, "- {record}");
        }
        out.push('\n');
    }

    out.push_str("## Narrow date range\n\n");
    match date_range {
        Some((Some(start), Some(end))) => {
            let _ = writeln!(out, "- From {start} to {end}\n");
        }
        Some((Some(start), None)) => {
            let _ = writeln!(out, "- From {start} onward\n");
        }
        Some((None, Some(end))) => {
            let _ = writeln!(out, "- Through {end}\n");
        }
        _ => {
            let _ = writeln!(out, "- {NOT_OBSERVED_PHRASING}\n");
        }
    }

    out.push_str("## Known vendor or project name\n\n");
    match vendor_or_project {
        Some(name) => {
            let _ = writeln!(out, "- {name}\n");
        }
        None => {
            let _ = writeln!(out, "- {NOT_OBSERVED_PHRASING}\n");
        }
    }

    out.push_str("## Existing public sources already checked\n\n");
    if sources_checked.is_empty() {
        let _ = writeln!(out, "- {NOT_OBSERVED_PHRASING}\n");
    } else {
        for source in sources_checked {
            let _ = writeln!(out, "- {source}");
        }
        out.push('\n');
    }

    out.push_str("## Review notice\n\n");
    let _ = writeln!(out, "{REVIEW_REQUIRED}\n");

    let _ = writeln!(out, "*Matter: {} ({})*", matter.title, matter.jurisdiction);
    out
}

/// Loads identifiers for a matter from the store for draft building.
pub fn matter_identifiers(
    store: &pnull_core::Store,
    matter_id: &str,
) -> Result<Vec<ProcurementIdentifier>, CoraError> {
    store
        .procurement_identifiers(matter_id)
        .map_err(|_| CoraError::MatterNotFound(matter_id.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pnull_core::{IdentifierKind, ProcurementIdentifier};

    fn matter() -> ProcurementMatter {
        ProcurementMatter {
            id: "matter:1".to_owned(),
            jurisdiction: "Colorado Springs".to_owned(),
            title: "R26-023AB Transit Fare".to_owned(),
            review_state: "draft".to_owned(),
            publication_state: "unpublished".to_owned(),
        }
    }

    fn identifier() -> ProcurementIdentifier {
        ProcurementIdentifier {
            id: ProcurementIdentifier::id_for(
                "matter:1",
                IdentifierKind::SolicitationNumber,
                "R26-023AB",
            ),
            matter_id: "matter:1".to_owned(),
            kind: IdentifierKind::SolicitationNumber,
            raw: "R26-023AB".to_owned(),
            source_id: "src".to_owned(),
            normalized: Some("R26023AB".to_owned()),
            normalization_rule: Some("uppercase-alphanumeric-compact".to_owned()),
            known: false,
        }
    }

    #[test]
    fn draft_identifies_gap_not_fact() {
        let draft = build_draft(
            &matter(),
            &[identifier()],
            &["executed contract", "award notice"],
            Some((Some("2025-06-01".to_owned()), Some("2026-01-31".to_owned()))),
            Some("Next-Generation Transit Fare Collection System"),
            &["colorado-springs-contract-awards", "colorado-springs-solicitation-mirror"],
        );
        assert!(draft.markdown.contains("not submitted"));
        assert!(draft.markdown.contains("R26-023AB"));
        assert!(draft.markdown.contains("executed contract"));
        assert!(draft.markdown.contains("2025-06-01 to 2026-01-31"));
        assert!(draft.markdown.contains("operator and legal review"));
    }

    #[test]
    fn draft_never_sends_or_guesses_recipient() {
        let draft = build_draft(&matter(), &[], &[], None, None, &[]);
        assert!(!draft.markdown.contains("mailto:"));
        assert!(!draft.markdown.contains('@')); // no email address
        assert!(!draft.markdown.contains("deadline"));
        assert!(draft.markdown.contains("no recipient has been selected"));
    }

    #[test]
    fn empty_gaps_are_stated_not_invented() {
        let draft = build_draft(&matter(), &[], &[], None, None, &[]);
        assert!(draft.markdown.contains("Not observed in the checked sources."));
    }

    #[test]
    fn draft_is_deterministic() {
        let a = build_draft(&matter(), &[identifier()], &["award"], None, Some("X"), &["s1"]);
        let b = build_draft(&matter(), &[identifier()], &["award"], None, Some("X"), &["s1"]);
        assert_eq!(a.markdown, b.markdown);
        assert_eq!(a.id, b.id);
    }

    #[test]
    fn identifiers_are_deduplicated_preserving_order() {
        let a = identifier();
        let b = identifier(); // same raw
        let draft = build_draft(&matter(), &[a, b], &[], None, None, &[]);
        assert_eq!(draft.identifiers.len(), 1);
        assert_eq!(draft.identifiers[0], "R26-023AB");
    }
}
