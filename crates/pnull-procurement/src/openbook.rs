//! `OpenBook` COS investigation and documented negative capability finding.
//!
//! `OpenBook` COS is a Socrata single-page application. Its exposed schema is
//! budget-level (department, fund, account, service, budget) with no vendor-level
//! expenditure field and no contract or purchase-order identifier. This module
//! documents that limitation so payment evidence is never invented to satisfy the
//! desired procurement chain.

use pnull_core::{CoverageState, SourceAuthority};

/// The `OpenBook` landing page (informational).
pub const OPENBOOK_LANDING_URL: &str = "https://coloradosprings.gov/budget/page/openbook-cos";
/// The Socrata SPA that hosts the budget datasets.
pub const OPENBOOK_SOCRATA_URL: &str = "https://coloradosprings.budget.socrata.com/";
/// The view-data surface that claims monthly export.
pub const OPENBOOK_VIEW_DATA_URL: &str = "https://coloradosprings.budget.socrata.com/#!/view-data";

/// The four real dataset IDs discovered from the SPA configuration.
pub const OPENBOOK_DATASETS: &[&str] = &[
    "3mjf-cycw", // revenue
    "utpe-gz6w", // operating budget (expenses)
    "9x87-g8nk", // capital budget
    "bxi2-uqix", // capital projects
];

/// The exposed budget-level fields (from the SPA config). No vendor or contract
/// identifier appears among them.
pub const OPENBOOK_FIELDS: &[&str] = &[
    "department_description",
    "fund_description",
    "account_description",
    "service",
    "budget",
];

/// A structured negative capability finding. This is a *finding*, not a claim
/// that no expenditure exists — only that `OpenBook` cannot supply vendor-level
/// payment evidence for a procurement matter.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct OpenBookFinding {
    pub authority: SourceAuthority,
    pub coverage_state: CoverageState,
    /// Whether the exposed schema contains a vendor-level expenditure field.
    pub has_vendor_level_expenditures: bool,
    /// Whether the exposed schema contains a contract/PO/invoice identifier.
    pub has_procurement_identifier: bool,
    pub fields: Vec<String>,
    pub datasets: Vec<String>,
    pub note: String,
}

impl OpenBookFinding {
    pub fn current() -> Self {
        Self {
            authority: SourceAuthority::OfficialFinancialExport,
            coverage_state: CoverageState::InformationalOnly,
            has_vendor_level_expenditures: false,
            has_procurement_identifier: false,
            fields: OPENBOOK_FIELDS.iter().map(|s| (*s).to_owned()).collect(),
            datasets: OPENBOOK_DATASETS.iter().map(|s| (*s).to_owned()).collect(),
            note: "OpenBook COS exposes budget-level data (department, fund, account, \
                   service, budget) only. It provides no vendor-level expenditure field and \
                   no contract, purchase-order, or invoice identifier, so it cannot connect \
                   expenditures to a procurement matter at the vendor level. Payment evidence \
                   is therefore unavailable from this source in this milestone."
                .to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openbook_is_documented_as_insufficient_for_vendor_linkage() {
        let finding = OpenBookFinding::current();
        assert!(!finding.has_vendor_level_expenditures);
        assert!(!finding.has_procurement_identifier);
        assert_eq!(finding.coverage_state, CoverageState::InformationalOnly);
        assert!(finding.fields.contains(&"budget".to_owned()));
        // The note is explicit about the limitation.
        assert!(finding.note.contains("no vendor-level expenditure field"));
    }

    #[test]
    fn openbook_never_claims_no_expenditure_exists() {
        let finding = OpenBookFinding::current();
        // The finding must not assert absence of payments, only that the source
        // cannot provide the evidence.
        assert!(!finding.note.contains("no expenditure exists"));
        assert!(finding.note.contains("unavailable from this source"));
    }
}
