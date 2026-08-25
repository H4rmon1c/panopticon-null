//! Real Bubblewrap + Poppler extraction integration test.
//!
//! Nested Bubblewrap cannot be established inside the Nix build sandbox, so
//! this test is marked `#[ignore]`: it is NOT run by `cargo test --workspace`
//! inside the Nix `build-and-test` derivation. It IS run explicitly, with
//! `--ignored`, by a dedicated required CI step outside the Nix derivation
//! (see `.github/workflows/ci.yml`), where real Bubblewrap and Poppler are
//! available. That step executes real Bubblewrap plus Poppler and fails if
//! extraction does not work. This test is never silently skipped.

use std::fs;
use std::path::PathBuf;

use pnull_core::{ExtractionStatus, SourceType};
use pnull_ingest::{
    Budgets, BuildMetadata, DEFAULT_MAX_BYTES, ExtractionSandboxConfig, IngestRequest, RealSandbox,
    Tracker, ingest_bytes,
};
use tempfile::tempdir;

#[test]
#[ignore = "requires real Bubblewrap + Poppler outside the Nix derivation; run via --ignored in the dedicated CI step"]
fn real_pdf_fixture_is_extracted_by_poppler_in_sandbox() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bytes = fs::read(workspace.join("fixtures/co/ordinance-25-93-draft.pdf"))
        .expect("official PDF fixture");

    let dir = tempdir().expect("temporary directory");
    let store = pnull_core::Store::open(dir.path()).expect("store");

    let request = IngestRequest {
        jurisdiction: "Colorado Springs, Colorado".to_owned(),
        source_url: "https://example.test/document".to_owned(),
        source_type: SourceType::OfficialApi,
        document_title: "Test document".to_owned(),
        publication_date: Some("2025-10-27".to_owned()),
        retrieval_timestamp: "2026-08-16T00:00:00Z".to_owned(),
        mime_type: "application/pdf".to_owned(),
        original_filename: "ordinance.pdf".to_owned(),
        supersedes: None,
        enable_ocr: false,
        ocr_language: "eng".to_owned(),
        max_bytes: DEFAULT_MAX_BYTES,
    };

    let sandbox = RealSandbox::new(ExtractionSandboxConfig::defaults()).expect("bwrap sandbox");
    let mut budgets = Tracker::new(Budgets::defaults());
    let outcome = ingest_bytes(
        &store,
        &sandbox,
        &mut budgets,
        &BuildMetadata::local(),
        &request,
        &bytes,
    )
    .expect("PDF ingestion");

    // On failure, surface the full structured extraction error (failing layer,
    // exit status or signal, and sanitized stderr) so the CI log explains why.
    if outcome.record.extraction_status != ExtractionStatus::Complete {
        eprintln!("extraction_status: {:?}", outcome.record.extraction_status);
        match &outcome.record.extraction_error {
            Some(error) => {
                eprintln!("extraction_error.code:    {}", error.code);
                eprintln!("extraction_error.message: {}", error.message);
            }
            None => eprintln!("extraction_error: <none>"),
        }
    }

    assert_eq!(outcome.record.extraction_status, ExtractionStatus::Complete);
    assert!(
        outcome
            .extracted_text
            .contains("POLICE DEPARTMENT TECHNOLOGY SURCHARGE")
    );
    assert!(!outcome.text_maps.is_empty());
    store.verify(&outcome.record.id).expect("stored digest");
}
