//! Panopticon Null procurement: "The Procurement Chain".
//!
//! Turns isolated evidence receipts into a verifiable institutional money trail
//! (solicitation -> amendment -> award -> contract -> expenditure), connecting
//! official records only when the evidence supports the connection. The
//! governing rule is: *follow the money without inventing the links*.

pub mod awards;
pub mod casefile;
pub mod cora;
pub mod coverage;
pub mod csv;
pub mod demo;
pub mod import;
pub mod openbook;
pub mod reconcile;
pub mod snapshot;
pub mod solicitations;

pub use awards::{AwardRow, award_identifier, award_organization, parse_awards_table};
pub use casefile::{
    CaseFileContent, build_content, default_limitations, generate as generate_case_file,
    money_display, render_json as render_case_json, render_markdown as render_case_markdown,
};
pub use cora::{
    CITY_DEPARTMENT, REVIEW_REQUIRED, build_draft as build_cora_draft,
    matter_identifiers as cora_matter_identifiers, render_markdown as render_cora_markdown,
};
pub use coverage::{
    NOT_OBSERVED_PHRASING, absence_phrasing, can_support_negative_claim, default_state, summarize,
};
pub use csv::{CsvError, neutralize_cell, rows_to_csv};
pub use demo::{
    CONTROL_MATTER_ID, DemoResult, TRANSIT_FARE_MATTER_ID, run_demo, verify_fixture_digests,
};
pub use import::{SuppliedRecord, SuppliedRecordDeclaration, import_supplied_record};
pub use openbook::{
    OPENBOOK_DATASETS, OPENBOOK_FIELDS, OPENBOOK_LANDING_URL, OPENBOOK_SOCRATA_URL, OpenBookFinding,
};
pub use reconcile::{
    ReconcileError, amount_conflict_item, candidate_identifier_item, exact_identifier_match,
    missing_document_item, reconciliation_binding_digest, record_decision, vanished_record_item,
    vendor_alias_item,
};
pub use snapshot::{
    Acquisition, RecordRow, SnapshotError, latest_snapshot, record_diff, record_snapshot,
    record_unchanged, row_key,
};
pub use solicitations::{
    ABSENCE_PHRASING, INCOMPLETENESS_WARNING, SolicitationRecord, parse_solicitations,
    solicitation_identifier,
};
