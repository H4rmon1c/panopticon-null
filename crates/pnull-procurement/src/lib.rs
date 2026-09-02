//! Panopticon Null procurement: "The Procurement Chain".
//!
//! Turns isolated evidence receipts into a verifiable institutional money trail
//! (solicitation -> amendment -> award -> contract -> expenditure), connecting
//! official records only when the evidence supports the connection. The
//! governing rule is: *follow the money without inventing the links*.

pub mod awards;
pub mod casefile;
pub mod chain;
pub mod changealert;
pub mod cora;
pub mod cora_ledger;
pub mod coverage;
pub mod csv;
pub mod demo;
pub mod import;
pub mod matters;
pub mod openbook;
pub mod reconcile;
pub mod relationships;
pub mod snapshot;
pub mod solicitations;

pub use awards::{AwardRow, award_identifier, award_organization, parse_awards_table};
pub use casefile::{
    CaseFileContent, build_content, default_limitations, generate as generate_case_file,
    money_display, render_json as render_case_json, render_markdown as render_case_markdown,
};
pub use chain::{
    ChainError, ChainEvidence, ChainLink, ChainStage, ChainStageObservation, ChainView,
    EvidenceGap, LinkKind, build_chain, linked_by_exact_identifier, render as render_chain,
};
pub use changealert::{
    ChangeAlertError, build_change_alerts, field_diffs, persist_change_alerts, row_identity,
};
pub use cora::{
    CITY_DEPARTMENT, REVIEW_REQUIRED, build_draft as build_cora_draft,
    matter_identifiers as cora_matter_identifiers, render_markdown as render_cora_markdown,
};
pub use cora_ledger::{
    CoraLedgerError, OFFLINE_CREATED_AT, gap_resolved, gap_set_digest, list as list_cora_requests,
    register_draft as register_cora_draft, response_received, show as show_cora_request,
    still_unresolved, submit as submit_cora_request,
};
pub use coverage::{
    NOT_OBSERVED_PHRASING, absence_phrasing, can_support_negative_claim, default_state, summarize,
};
pub use csv::{CsvError, neutralize_cell, rows_to_csv};
pub use demo::{
    CONTROL_MATTER_ID, DemoResult, TRANSIT_FARE_MATTER_ID, run_demo, verify_fixture_digests,
};
pub use import::{
    SuppliedRecord, SuppliedRecordDeclaration, import_supplied_record, persist_supplied_record,
};
pub use matters::{
    attach_award_row, attach_solicitation_record, ensure_matter, matter_id_for_identifier,
};
pub use openbook::{
    OPENBOOK_DATASETS, OPENBOOK_FIELDS, OPENBOOK_LANDING_URL, OPENBOOK_SOCRATA_URL, OpenBookFinding,
};
pub use reconcile::{
    ReconcileError, amount_conflict_item, candidate_identifier_item, exact_identifier_match,
    missing_document_item, reconciliation_binding_digest, record_decision, vanished_record_item,
    vendor_alias_item,
};
pub use relationships::{
    LinkDetectionOutcome, RecordReference, RelationshipError, detect_official_relationships,
    reference_fields,
};
pub use snapshot::{
    Acquisition, RecordRow, SnapshotError, latest_snapshot, record_diff, record_snapshot,
    record_unchanged, row_key,
};
pub use solicitations::{
    ABSENCE_PHRASING, INCOMPLETENESS_WARNING, SolicitationRecord, parse_solicitations,
    solicitation_identifier,
};
