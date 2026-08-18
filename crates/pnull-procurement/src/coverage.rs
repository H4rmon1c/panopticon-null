//! Coverage ledger helpers: rendering, and the strict user-facing phrasing for
//! absences. Absence from a partial source is never proof of absence.

use pnull_core::{CoverageEntry, CoverageState};

/// The only user-facing phrasing permitted for an unobserved record.
pub const NOT_OBSERVED_PHRASING: &str = "Not observed in the checked sources.";

/// Returns the user-facing phrasing for an absence. This never asserts the
/// record does not exist; it states only that it was not observed.
pub fn absence_phrasing() -> &'static str {
    NOT_OBSERVED_PHRASING
}

/// Whether a coverage state can support a negative claim about a population.
///
/// Only `Complete` (with affirmative, reproducible enumeration evidence) can
/// support a negative claim; everything else defaults to "not observed".
pub fn can_support_negative_claim(state: CoverageState) -> bool {
    state == CoverageState::Complete
}

/// A rendered coverage summary for one source.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CoverageSummary {
    pub source_id: String,
    pub latest_state: CoverageState,
    pub entry_count: usize,
    pub latest_retrieved_at: Option<String>,
    pub latest_digest: Option<String>,
}

/// Summarizes the coverage ledger for one source from its entries.
pub fn summarize(entries: &[CoverageEntry]) -> CoverageSummary {
    let latest = entries.last();
    CoverageSummary {
        source_id: latest.map(|e| e.source_id.clone()).unwrap_or_default(),
        latest_state: latest.map_or(CoverageState::Unknown, |e| e.state),
        entry_count: entries.len(),
        latest_retrieved_at: latest.map(|e| e.retrieved_at.clone()),
        latest_digest: latest.and_then(|e| e.persisted_digest.clone()),
    }
}

/// The default coverage state when no affirmative evidence exists.
pub fn default_state() -> CoverageState {
    CoverageState::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;
    use pnull_core::SourceAuthority;

    fn entry(source_id: &str, state: CoverageState, at: &str) -> CoverageEntry {
        CoverageEntry {
            id: CoverageEntry::id_for(source_id, at),
            source_id: source_id.to_owned(),
            source_url: "https://x".to_owned(),
            authority: SourceAuthority::OfficialInformationalMirror,
            state,
            retrieved_at: at.to_owned(),
            persisted_digest: Some("digest".to_owned()),
            http_status: Some(200),
            etag: None,
            last_modified: None,
            final_url: Some("https://x".to_owned()),
            parser_version: Some("1.0".to_owned()),
            schema_version: Some(2),
            claimed_date_range: None,
            record_count: Some(3),
            pagination_complete: Some(true),
            access_errors: Vec::new(),
            human_review_state: "unreviewed".to_owned(),
            note: String::new(),
        }
    }

    #[test]
    fn absence_phrasing_is_never_negative() {
        assert_eq!(absence_phrasing(), "Not observed in the checked sources.");
        assert!(!absence_phrasing().contains("no contract exists"));
        assert!(!absence_phrasing().contains("No contract exists"));
    }

    #[test]
    fn only_complete_supports_negative_claim() {
        assert!(can_support_negative_claim(CoverageState::Complete));
        assert!(!can_support_negative_claim(CoverageState::Partial));
        assert!(!can_support_negative_claim(
            CoverageState::InformationalOnly
        ));
        assert!(!can_support_negative_claim(CoverageState::Unknown));
        assert!(!can_support_negative_claim(CoverageState::AccessBlocked));
    }

    #[test]
    fn default_state_is_unknown() {
        assert_eq!(default_state(), CoverageState::Unknown);
    }

    #[test]
    fn summary_uses_latest_entry() {
        let entries = vec![
            entry("src", CoverageState::Partial, "2026-08-01T00:00:00Z"),
            entry(
                "src",
                CoverageState::InformationalOnly,
                "2026-08-17T00:00:00Z",
            ),
        ];
        let summary = summarize(&entries);
        assert_eq!(summary.entry_count, 2);
        assert_eq!(summary.latest_state, CoverageState::InformationalOnly);
        assert_eq!(
            summary.latest_retrieved_at.as_deref(),
            Some("2026-08-17T00:00:00Z")
        );
    }
}
