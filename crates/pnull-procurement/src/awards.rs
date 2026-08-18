//! Parser for the City of Colorado Springs contract-award table.
//!
//! The source is an informational mirror (`official_informational_mirror`), not
//! an authoritative procurement system. The parser preserves every raw value and
//! a per-row provenance, tolerating historical formatting irregularities without
//! silently shifting columns.

use pnull_core::{
    CoverageState, IdentifierKind, MoneyValue, OrganizationRole, ProcurementIdentifier,
    ProcurementOrganization, SourceAuthority, parse_money,
};
use scraper::{Html, Selector};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AwardsError {
    #[error("award table not found in source document")]
    TableNotFound,
    #[error("header row did not match the expected contract-award columns: {0}")]
    UnexpectedColumns(String),
    #[error("row {row} has {observed} cells; expected 6")]
    UnexpectedCellCount { row: usize, observed: usize },
}

/// One parsed award row with row-level provenance.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct AwardRow {
    pub row_index: usize,
    /// RFP/IFB number as written in the source (may be blank).
    pub solicitation_id: String,
    pub project_name: String,
    pub contractor: String,
    pub raw_amount: String,
    pub amount: MoneyValue,
    pub start_date: String,
    pub notes: String,
    /// The coverage/source authority labels that travel with the record.
    pub authority: SourceAuthority,
    pub coverage_state: CoverageState,
    /// Row-level provenance: the snapshot digest this row was parsed from.
    pub snapshot_digest: String,
    /// The normalized identifier, if a deterministic rule produced one.
    pub normalized_solicitation_id: Option<String>,
}

/// Parses the 6-column contract-award table from the source HTML.
///
/// The `snapshot_digest` is the SHA-256 of the exact persisted bytes this table
/// was parsed from, binding every row to its immutable snapshot.
pub fn parse_awards_table(html: &str, snapshot_digest: &str) -> Result<Vec<AwardRow>, AwardsError> {
    let document = Html::parse_document(html);
    let table_selector = Selector::parse("table.table").expect("valid table selector");
    let row_selector = Selector::parse("tr").expect("valid row selector");
    let cell_selector = Selector::parse("td, th").expect("valid cell selector");

    let table = document
        .select(&table_selector)
        .next()
        .ok_or(AwardsError::TableNotFound)?;

    let mut rows = Vec::new();
    let mut row_index = 0usize;
    let mut header_seen = false;
    for row in table.select(&row_selector) {
        let cells: Vec<String> = row
            .select(&cell_selector)
            .map(|cell| normalize_cell_text(&cell.text().collect::<Vec<_>>().join(" ")))
            .collect();
        if cells.is_empty() {
            continue;
        }
        if !header_seen {
            // The first non-empty row must be the header and must name the
            // expected columns; anything else is a schema/layout drift we
            // refuse rather than silently shifting columns.
            header_seen = true;
            let is_header = cells[0].eq_ignore_ascii_case("RFP/IFB Number")
                || cells[0].contains("RFP/IFB");
            if !is_header || cells.len() != 6 {
                return Err(AwardsError::UnexpectedColumns(cells.join(" | ")));
            }
            continue;
        }
        if cells.len() < 6 {
            return Err(AwardsError::UnexpectedCellCount {
                row: row_index,
                observed: cells.len(),
            });
        }
        let mut padded = cells.clone();
        padded.resize(6, String::new());
        let raw_amount = padded[3].clone();
        let solicitation_id = padded[0].clone();
        let normalized = pnull_core::normalize_identifier(&solicitation_id);
        rows.push(AwardRow {
            row_index,
            solicitation_id,
            project_name: padded[1].clone(),
            contractor: padded[2].clone(),
            raw_amount: raw_amount.clone(),
            amount: parse_money(&raw_amount),
            start_date: padded[4].clone(),
            notes: padded[5].clone(),
            authority: SourceAuthority::OfficialInformationalMirror,
            coverage_state: CoverageState::InformationalOnly,
            snapshot_digest: snapshot_digest.to_owned(),
            normalized_solicitation_id: normalized.map(|(k, _)| k),
        });
        row_index += 1;
    }
    Ok(rows)
}

/// Normalizes a cell's raw text without altering meaningful content.
fn normalize_cell_text(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.trim().to_owned()
}

/// Builds a procurement organization from an award row's contractor.
pub fn award_organization(
    matter_id: &str,
    contractor: &str,
    _snapshot_digest: &str,
) -> ProcurementOrganization {
    ProcurementOrganization {
        id: ProcurementOrganization::id_for(
            matter_id,
            OrganizationRole::AwardedContractor,
            contractor,
        ),
        matter_id: matter_id.to_owned(),
        role: OrganizationRole::AwardedContractor,
        raw_name: contractor.to_owned(),
        source_id: "colorado-springs-contract-awards".to_owned(),
        normalized_alias: pnull_core::organization_alias_candidate(contractor),
        alias_reviewed: false,
    }
}

/// Builds a procurement identifier from an award row's solicitation id.
pub fn award_identifier(
    matter_id: &str,
    solicitation_id: &str,
) -> Option<ProcurementIdentifier> {
    if solicitation_id.trim().is_empty() {
        return None;
    }
    let kind = classify_identifier(solicitation_id);
    let (normalized, rule) = match pnull_core::normalize_identifier(solicitation_id) {
        Some((key, rule)) => (Some(key), Some(rule.to_owned())),
        None => (None, None),
    };
    Some(ProcurementIdentifier {
        id: ProcurementIdentifier::id_for(matter_id, kind, solicitation_id),
        matter_id: matter_id.to_owned(),
        kind,
        raw: solicitation_id.to_owned(),
        source_id: "colorado-springs-contract-awards".to_owned(),
        normalized,
        normalization_rule: rule,
        known: false,
    })
}

/// Classifies a raw solicitation identifier by its prefix, when one is clear.
fn classify_identifier(raw: &str) -> IdentifierKind {
    let upper = raw.to_ascii_uppercase();
    if upper.starts_with("RFP") {
        IdentifierKind::Rfp
    } else if upper.starts_with("RFQ") {
        IdentifierKind::Rfq
    } else if upper.starts_with("IFB") || upper.starts_with('B') {
        IdentifierKind::Ifb
    } else if upper.starts_with('R') || upper.starts_with('Q') {
        IdentifierKind::SolicitationNumber
    } else {
        IdentifierKind::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pnull_core::MoneyState;

    const DIGEST: &str = "d41d8cd98f00b204e9800998ecf8427e";

    fn html_for(rows: &[&str]) -> String {
        let mut out = String::from(
            "<html><body><table class=\"table\"><tr><th>RFP/IFB Number</th>\
             <th>Project Name</th><th>Awarded Contractor</th><th>Awarded Amount</th>\
             <th>Contract Start Date</th><th>Notes</th></tr>",
        );
        for row in rows {
            let cells: Vec<&str> = row.split('|').collect();
            out.push_str("<tr>");
            for cell in cells {
                out.push_str("<td>");
                out.push_str(cell);
                out.push_str("</td>");
            }
            out.push_str("</tr>");
        }
        out.push_str("</table></body></html>");
        out
    }

    #[test]
    fn parses_well_formed_rows() {
        let html = html_for(&[
            "Q25-130ZM|LogRhythm Renewal|Optiv|$42,075.00|February 1, 2026|",
            "R24-T114JD|On-Call Guardrail Construction Services|Adarand Constructors|N/A|January 1, 2025|",
        ]);
        let rows = parse_awards_table(&html, DIGEST).expect("parse");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].solicitation_id, "Q25-130ZM");
        assert_eq!(rows[0].amount.state, MoneyState::Exact);
        assert_eq!(rows[0].amount.cents, Some(4_207_500));
        assert_eq!(rows[1].amount.state, MoneyState::NotApplicable);
        assert_eq!(rows[1].row_index, 1);
    }

    #[test]
    fn tolerates_irregular_amounts_without_shifting_columns() {
        let html = html_for(&[
            "R23-T119KK|Traffic Signal On-Call|C&D Electric and Sturgeon Electric|$0.00 IDIQ|January 1, 2024|",
            "B22-T168KK|Crack Seal Materials|Crafco & Maxwell|$300,000 each|April 18, 2023|",
            "R22-005NS|Temporary Staffing Services|System Soft, Apex|various|7/1/22|Multiple contracts awarded",
        ]);
        let rows = parse_awards_table(&html, DIGEST).expect("parse");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].amount.state, MoneyState::IdiqCeiling);
        assert_eq!(rows[1].amount.state, MoneyState::Exact);
        assert_eq!(rows[1].amount.cents, Some(30_000_000));
        assert_eq!(rows[2].amount.state, MoneyState::Various);
        // Columns did not shift: contractor is still column 3.
        assert_eq!(rows[2].contractor, "System Soft, Apex");
        assert_eq!(rows[2].notes, "Multiple contracts awarded");
    }

    #[test]
    fn rejects_row_with_wrong_column_count() {
        let html = html_for(&["Q25-130ZM|LogRhythm Renewal|Optiv|$42,075.00|February 1, 2026"]);
        assert!(matches!(
            parse_awards_table(&html, DIGEST),
            Err(AwardsError::UnexpectedCellCount { .. })
        ));
    }

    #[test]
    fn rejects_table_with_unexpected_columns() {
        let html = "<html><body><table class=\"table\"><tr><th>X</th><th>Y</th></tr>\
                    <tr><td>a</td><td>b</td></tr></table></body></html>";
        assert!(matches!(
            parse_awards_table(html, DIGEST),
            Err(AwardsError::UnexpectedColumns(_))
        ));
    }

    #[test]
    fn rejects_missing_table() {
        let html = "<html><body>no table</body></html>";
        assert!(matches!(
            parse_awards_table(html, DIGEST),
            Err(AwardsError::TableNotFound)
        ));
    }

    #[test]
    fn hostile_malformed_html_does_not_panic() {
        let cases = [
            "<table", "<table class=table>", "<table><tr>", "<html",
            "<td><td>", "broken</table></table>",
            "<table class=\"table\"><tr><td colspan=\"99\">x</td></tr></table>",
        ];
        for case in cases {
            let _ = parse_awards_table(case, DIGEST);
        }
    }

    #[test]
    fn identifiers_are_classified() {
        assert_eq!(classify_identifier("RFP-2024-01"), IdentifierKind::Rfp);
        assert_eq!(classify_identifier("RFQ-2024-01"), IdentifierKind::Rfq);
        assert_eq!(classify_identifier("IFB-2024-01"), IdentifierKind::Ifb);
        assert_eq!(classify_identifier("R26-023AB"), IdentifierKind::SolicitationNumber);
        assert_eq!(classify_identifier("unknown"), IdentifierKind::Unknown);
    }
}
