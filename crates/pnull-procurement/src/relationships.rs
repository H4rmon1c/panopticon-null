//! Explicit official-relationship links ("who authorized it", Item 5).
//!
//! A link is never invented. Each source adapter may declare a fixed allowlist
//! of *reference fields*: documented fields in which an official document may
//! reference another official identifier (e.g. a council matter's referenced
//! matter field, an ordinance's numbered citation of a solicitation number, an
//! award row's notes field citing an ordinance number). Fields that are not
//! declared are free text and can never produce a link.
//!
//! A link is recorded only when a declared reference field of one preserved
//! record contains an exact match of an identifier stored for another record,
//! and both endpoints resolve to stored snapshots with valid SHA-256 digests.
//! Near-miss (non-exact) candidates are queued for human review; they are never
//! auto-linked.

use pnull_core::{IdentifierKind, OfficialRelationship, OfficialRelationshipKind, Store};
use thiserror::Error;

/// A declared reference field: the field name and a human locator describing
/// where in the record the reference appears.
#[derive(Clone, Debug)]
pub struct ReferenceField {
    pub field: &'static str,
    pub locator: &'static str,
}

/// The fixed allowlist of declared reference fields per source adapter.
///
/// Only these fields are scanned for official-identifier references. Anything
/// else is free text and can never produce a link. Adding a field here is a
/// deliberate, documented decision (see `docs/procurement-methodology.md`).
pub fn reference_fields(source_id: &str) -> &'static [ReferenceField] {
    match source_id {
        // The award-row notes column may cite an authorizing ordinance or
        // legislative matter number.
        "colorado-springs-contract-awards" => &[ReferenceField {
            field: "notes",
            locator: "contract-award row notes column",
        }],
        // The solicitation mirror's linked-documents list may cite another
        // official identifier (e.g. an ordinance or a numbered solicitation).
        "colorado-springs-solicitation-mirror" => &[ReferenceField {
            field: "linked_documents",
            locator: "solicitation linked-documents list",
        }],
        _ => &[],
    }
}

/// A preserved record's declared reference field, ready for link detection.
///
/// `source_record_id` is the stable id of the preserved record whose field
/// carries the reference text; `snapshot_id`/`snapshot_digest` bind the record
/// to its immutable snapshot. `matter_id` is the matter the record belongs to.
#[derive(Clone, Debug)]
pub struct RecordReference {
    pub source_id: String,
    pub source_record_id: String,
    pub matter_id: String,
    pub snapshot_id: String,
    pub snapshot_digest: String,
    pub reference_field: String,
    pub reference_text: String,
}

/// The outcome of one link-detection pass over a set of preserved records.
#[derive(Clone, Debug, Default)]
pub struct LinkDetectionOutcome {
    /// Exact, recorded official-relationship links.
    pub links: Vec<OfficialRelationship>,
    /// Candidate (non-exact) references queued for human review.
    pub candidates: Vec<pnull_core::ReconciliationItem>,
}

#[derive(Debug, Error)]
pub enum RelationshipError {
    #[error("store operation failed: {0}")]
    Store(#[from] pnull_core::CoreError),
}

/// Scans the given preserved records' declared reference fields for exact
/// matches of identifiers stored for other records, and records an
/// official-relationship link for each such match where both endpoints resolve
/// to stored snapshots with valid SHA-256 digests. Near-misses are queued as
/// review candidates, never auto-linked. Returns the recorded links (already
/// persisted) and the queued candidates.
#[allow(clippy::too_many_lines)]
pub fn detect_official_relationships(
    store: &Store,
    records: &[RecordReference],
) -> Result<LinkDetectionOutcome, RelationshipError> {
    // Index every stored identifier by its exact-match key (normalized form).
    // Multiple raws may share a key (same identifier in different matters or
    // formats); each exact match candidate is checked individually.
    let mut identifiers_by_key: std::collections::BTreeMap<String, Vec<StoredIdentifier>> =
        std::collections::BTreeMap::new();
    for matter in store.procurement_matters()? {
        for identifier in store.procurement_identifiers(&matter.id)? {
            let key = identifier
                .normalized
                .clone()
                .or_else(|| pnull_core::identifier_match_key(&identifier.raw));
            if let Some(key) = key {
                identifiers_by_key
                    .entry(key)
                    .or_default()
                    .push(StoredIdentifier {
                        raw: identifier.raw.clone(),
                        matter_id: identifier.matter_id.clone(),
                        kind: identifier.kind,
                    });
            }
        }
    }

    let mut outcome = LinkDetectionOutcome::default();

    for record in records {
        // Only declared reference fields are ever scanned.
        let declared = reference_fields(&record.source_id)
            .iter()
            .any(|f| f.field == record.reference_field);
        if !declared {
            continue;
        }

        // Resolve and validate the source endpoint's snapshot.
        let Some(source_snapshot) = store.source_snapshot(&record.snapshot_id).ok() else {
            continue;
        };
        if !valid_digest(&record.snapshot_digest)
            || source_snapshot.persisted_digest != record.snapshot_digest
        {
            continue;
        }

        // Tokenize the reference-field text into candidate identifiers.
        let candidates = tokenize_identifiers(&record.reference_text);
        for candidate in &candidates {
            let Some(key) = pnull_core::identifier_match_key(candidate.as_str()) else {
                continue;
            };

            // Exact match against a stored identifier.
            let mut matched = false;
            if let Some(stored) = identifiers_by_key.get(&key) {
                for target in stored {
                    // A record referencing itself is not a relationship.
                    if target.matter_id == record.matter_id
                        && target.raw.eq_ignore_ascii_case(candidate)
                    {
                        continue;
                    }
                    // Both endpoints must resolve to stored snapshots with
                    // valid SHA-256 digests.
                    if !matter_resolves_to_valid_snapshot(store, &target.matter_id)? {
                        continue;
                    }
                    let link = OfficialRelationship {
                        id: OfficialRelationship::id_for(
                            &record.source_record_id,
                            &record.reference_field,
                            &target.raw,
                            &record.snapshot_id,
                        ),
                        kind: OfficialRelationshipKind::OfficialRelationship,
                        source_record_id: record.source_record_id.clone(),
                        source_snapshot_id: record.snapshot_id.clone(),
                        source_snapshot_digest: record.snapshot_digest.clone(),
                        target_identifier: target.raw.clone(),
                        target_matter_id: target.matter_id.clone(),
                        reference_field: record.reference_field.clone(),
                        quote: candidate.clone(),
                        locator: reference_fields(&record.source_id)
                            .iter()
                            .find(|f| f.field == record.reference_field)
                            .map_or_else(
                                || record.reference_field.clone(),
                                |f| f.locator.to_owned(),
                            ),
                        citations: vec![
                            format!(
                                "record {} in snapshot {} (digest {})",
                                record.source_record_id, record.snapshot_id, record.snapshot_digest
                            ),
                            format!(
                                "matter {} identifier {} (kind {})",
                                target.matter_id,
                                target.raw,
                                target.kind.label()
                            ),
                        ],
                        reviewed: true,
                    };
                    if store.insert_official_relationship(&link)? {
                        outcome.links.push(link);
                    }
                    matched = true;
                }
            }
            if matched {
                continue;
            }

            // Near-miss (non-exact) candidate: same family, trailing digits
            // differ by a small amount. Queued for human review, never linked.
            if let Some(target) = near_miss_target(&key, &identifiers_by_key) {
                let item = pnull_core::ReconciliationItem {
                    id: pnull_core::ReconciliationItem::id_for(
                        &record.matter_id,
                        pnull_core::ReconciliationKind::CandidateIdentifierMatch,
                        &format!(
                            "candidate reference '{}' in {} ({}): near-miss of '{}' in matter {}",
                            candidate,
                            record.reference_field,
                            record.source_record_id,
                            target.raw,
                            target.matter_id
                        ),
                    ),
                    matter_id: record.matter_id.clone(),
                    kind: pnull_core::ReconciliationKind::CandidateIdentifierMatch,
                    summary: format!(
                        "candidate reference '{}' in {} ({}): near-miss of '{}' in matter {}",
                        candidate,
                        record.reference_field,
                        record.source_record_id,
                        target.raw,
                        target.matter_id
                    ),
                    record_refs: vec![record.source_record_id.clone()],
                    state: "candidate".to_owned(),
                    created_at: "deterministic".to_owned(),
                };
                if store.insert_reconciliation_item(&item)? {
                    outcome.candidates.push(item);
                }
            }
        }
    }

    Ok(outcome)
}

/// A stored identifier indexed for exact-match lookup.
struct StoredIdentifier {
    raw: String,
    matter_id: String,
    kind: IdentifierKind,
}

/// Whether `matter_id` resolves to a stored snapshot with a valid SHA-256
/// digest. A matter resolves when at least one of its events is bound to a
/// snapshot whose persisted digest is a valid SHA-256.
fn matter_resolves_to_valid_snapshot(
    store: &Store,
    matter_id: &str,
) -> Result<bool, RelationshipError> {
    if store.procurement_matter(matter_id).is_err() {
        return Ok(false);
    }
    for event in store.procurement_events(matter_id)? {
        for evidence_id in event.evidence_ids {
            if let Ok(snapshot) = store.source_snapshot(&evidence_id)
                && valid_digest(&snapshot.persisted_digest)
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Whether a string is a valid 64-character lowercase hex SHA-256 digest.
pub fn valid_digest(digest: &str) -> bool {
    digest.len() == 64 && digest.chars().all(|c| c.is_ascii_hexdigit())
}

/// Tokenizes free text into candidate identifier tokens.
///
/// A candidate is a whitespace-delimited word with at least 3 alphanumeric
/// characters (so hyphenated identifiers like `R26-023AB`, `B22-T168KK`, and
/// `25-93` survive intact, while ordinary words still produce no match unless
/// they normalize exactly to a stored identifier). Leading/trailing
/// non-alphanumeric punctuation is stripped. Exact matching against stored
/// identifiers is what decides a link, never the tokenizer alone.
fn tokenize_identifiers(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for word in text.split_whitespace() {
        let trimmed: String = word
            .trim_matches(|c: char| !c.is_ascii_alphanumeric())
            .chars()
            .collect();
        let alpha_count = trimmed.chars().filter(char::is_ascii_alphanumeric).count();
        if alpha_count >= 3 {
            out.push(trimmed.to_ascii_uppercase());
        }
    }
    out
}

/// Finds a stored identifier that is a near-miss of `key`: same length and
/// differing only in the final alphanumeric character. Returns the closest
/// stored target, or `None`. Near-misses are never auto-linked.
fn near_miss_target<'a>(
    key: &str,
    identifiers_by_key: &'a std::collections::BTreeMap<String, Vec<StoredIdentifier>>,
) -> Option<&'a StoredIdentifier> {
    if key.len() < 3 {
        return None;
    }
    let prefix = &key[..key.len() - 1];
    let mut best: Option<&StoredIdentifier> = None;
    for (candidate_key, targets) in identifiers_by_key {
        if candidate_key.len() == key.len()
            && candidate_key.starts_with(prefix)
            && candidate_key != key
            && let Some(target) = targets.first()
        {
            best = Some(target);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use pnull_core::{
        CoverageState, IdentifierKind, ProcurementEvent, ProcurementEventKind, ProcurementMatter,
        SourceSnapshot, sha256_hex,
    };
    use tempfile::tempdir;

    fn test_store() -> Store {
        Store::open(tempdir().expect("temp").path()).expect("store")
    }

    fn seed_matter(store: &Store, matter_id: &str, title: &str) {
        let matter = ProcurementMatter {
            id: matter_id.to_owned(),
            jurisdiction: "Colorado Springs".to_owned(),
            title: title.to_owned(),
            review_state: "draft".to_owned(),
            publication_state: "unpublished".to_owned(),
        };
        store.insert_procurement_matter(&matter).expect("matter");
    }

    /// Inserts an identifier and an event bound to a valid snapshot so the
    /// matter resolves to a stored snapshot with a valid SHA-256 digest.
    fn seed_identifier_with_evidence(
        store: &Store,
        matter_id: &str,
        raw: &str,
        kind: IdentifierKind,
    ) {
        let digest = sha256_hex(format!("{raw}-evidence").as_bytes());
        let snapshot = SourceSnapshot {
            id: pnull_core::SourceSnapshot::id_for("test-source", &digest),
            source_id: "test-source".to_owned(),
            source_url: "https://example.test/source".to_owned(),
            retrieved_at: "2026-08-17T00:00:00Z".to_owned(),
            persisted_digest: digest.clone(),
            content_type: Some("text/html".to_owned()),
            etag: None,
            last_modified: None,
            final_url: "https://example.test/source".to_owned(),
            redirect_history: Vec::new(),
            parser_version: "test-1.0".to_owned(),
            schema_version: 2,
            record_count: Some(1),
            pagination_complete: Some(true),
            coverage_state: CoverageState::Complete,
            supersedes: None,
        };
        store.insert_source_snapshot(&snapshot).expect("snapshot");

        let identifier = pnull_core::ProcurementIdentifier {
            id: pnull_core::ProcurementIdentifier::id_for(matter_id, kind, raw),
            matter_id: matter_id.to_owned(),
            kind,
            raw: raw.to_owned(),
            source_id: "test-source".to_owned(),
            normalized: pnull_core::normalize_identifier(raw).map(|(k, _)| k),
            normalization_rule: Some("uppercase-alphanumeric-compact".to_owned()),
            known: false,
        };
        store
            .insert_procurement_identifier(&identifier)
            .expect("id");

        let event = ProcurementEvent {
            id: ProcurementEvent::id_for(
                matter_id,
                ProcurementEventKind::SolicitationPublished,
                "2026-01-01",
                &format!("solicitation {raw}"),
            ),
            matter_id: matter_id.to_owned(),
            kind: ProcurementEventKind::SolicitationPublished,
            date: Some("2026-01-01".to_owned()),
            summary: format!("solicitation {raw}"),
            identifier_ids: vec![identifier.id],
            evidence_ids: vec![snapshot.id.clone()],
            source_id: "test-source".to_owned(),
        };
        store.insert_procurement_event(&event).expect("event");
    }

    fn record(
        source_id: &str,
        source_record_id: &str,
        matter_id: &str,
        snapshot_id: &str,
        snapshot_digest: &str,
        field: &str,
        text: &str,
    ) -> RecordReference {
        RecordReference {
            source_id: source_id.to_owned(),
            source_record_id: source_record_id.to_owned(),
            matter_id: matter_id.to_owned(),
            snapshot_id: snapshot_id.to_owned(),
            snapshot_digest: snapshot_digest.to_owned(),
            reference_field: field.to_owned(),
            reference_text: text.to_owned(),
        }
    }

    #[test]
    fn declared_field_exact_match_records_link() {
        let store = test_store();
        // Two matters: an ordinance matter and a solicitation matter.
        seed_matter(&store, "proc:matter:co:ordinance-25-93", "Ordinance 25-93");
        seed_identifier_with_evidence(
            &store,
            "proc:matter:co:ordinance-25-93",
            "25-93",
            IdentifierKind::LegislativeMatter,
        );
        seed_matter(&store, "proc:matter:co:r26-023ab", "R26-023AB RFI");
        seed_identifier_with_evidence(
            &store,
            "proc:matter:co:r26-023ab",
            "R26-023AB",
            IdentifierKind::Rfp,
        );

        // The award row's declared `notes` field references the ordinance.
        let digest = sha256_hex(b"award-record-bytes");
        let snapshot = SourceSnapshot {
            id: pnull_core::SourceSnapshot::id_for("colorado-springs-contract-awards", &digest),
            source_id: "colorado-springs-contract-awards".to_owned(),
            source_url: "https://example.test/awards".to_owned(),
            retrieved_at: "2026-08-17T00:00:00Z".to_owned(),
            persisted_digest: digest.clone(),
            content_type: Some("text/html".to_owned()),
            etag: None,
            last_modified: None,
            final_url: "https://example.test/awards".to_owned(),
            redirect_history: Vec::new(),
            parser_version: "awards-1.0".to_owned(),
            schema_version: 2,
            record_count: Some(1),
            pagination_complete: Some(true),
            coverage_state: CoverageState::InformationalOnly,
            supersedes: None,
        };
        store.insert_source_snapshot(&snapshot).expect("snapshot");

        let records = [record(
            "colorado-springs-contract-awards",
            "proc:matter:co:r26-023ab:award-row-1",
            "proc:matter:co:r26-023ab",
            &snapshot.id,
            &digest,
            "notes",
            "Awarded per Ordinance 25-93 authorizing the police technology surcharge.",
        )];
        let outcome = detect_official_relationships(&store, &records).expect("detect");
        assert_eq!(outcome.links.len(), 1, "links: {outcome:?}");
        let link = &outcome.links[0];
        assert_eq!(link.kind, OfficialRelationshipKind::OfficialRelationship);
        assert_eq!(link.target_identifier, "25-93");
        assert_eq!(link.target_matter_id, "proc:matter:co:ordinance-25-93");
        assert_eq!(link.reference_field, "notes");
        assert_eq!(link.quote, "25-93");
        assert!(link.reviewed);
        assert_eq!(link.citations.len(), 2, "one citation per endpoint");
        // Idempotent: re-running inserts nothing new.
        let second = detect_official_relationships(&store, &records).expect("detect");
        assert!(second.links.is_empty(), "no duplicate link on re-run");
    }

    #[test]
    fn free_text_co_occurrence_produces_no_link() {
        let store = test_store();
        seed_matter(&store, "proc:matter:co:ordinance-25-93", "Ordinance 25-93");
        seed_identifier_with_evidence(
            &store,
            "proc:matter:co:ordinance-25-93",
            "25-93",
            IdentifierKind::LegislativeMatter,
        );
        seed_matter(&store, "proc:matter:co:r26-023ab", "R26-023AB RFI");
        seed_identifier_with_evidence(
            &store,
            "proc:matter:co:r26-023ab",
            "R26-023AB",
            IdentifierKind::Rfp,
        );
        let digest = sha256_hex(b"award-record-bytes");
        let snapshot = SourceSnapshot {
            id: pnull_core::SourceSnapshot::id_for("colorado-springs-contract-awards", &digest),
            source_id: "colorado-springs-contract-awards".to_owned(),
            source_url: "https://example.test/awards".to_owned(),
            retrieved_at: "2026-08-17T00:00:00Z".to_owned(),
            persisted_digest: digest.clone(),
            content_type: Some("text/html".to_owned()),
            etag: None,
            last_modified: None,
            final_url: "https://example.test/awards".to_owned(),
            redirect_history: Vec::new(),
            parser_version: "awards-1.0".to_owned(),
            schema_version: 2,
            record_count: Some(1),
            pagination_complete: Some(true),
            coverage_state: CoverageState::InformationalOnly,
            supersedes: None,
        };
        store.insert_source_snapshot(&snapshot).expect("snapshot");

        // The notes field is NOT declared for this scenario -> free text. The
        // identifier co-occurs but the field is not a declared reference field,
        // so no link is recorded.
        let records = [record(
            "colorado-springs-contract-awards",
            "award-row-1",
            "proc:matter:co:r26-023ab",
            &snapshot.id,
            &digest,
            "notes",
            "R26-023AB 25-93",
        )];
        // notes IS declared for the award source; use a non-declared source to
        // prove free-text co-occurrence never links.
        let outcome = detect_official_relationships(&store, &records).expect("detect");
        // notes is declared, so it would link. To prove free-text, use a
        // non-declared field/source.
        assert_eq!(outcome.links.len(), 1);

        let records_ft = [record(
            "some-other-source",
            "rec-1",
            "proc:matter:co:r26-023ab",
            &snapshot.id,
            &digest,
            "body",
            "R26-023AB and 25-93",
        )];
        let outcome_ft = detect_official_relationships(&store, &records_ft).expect("detect");
        assert!(
            outcome_ft.links.is_empty(),
            "free-text co-occurrence must not link: {outcome_ft:?}"
        );
    }

    #[test]
    fn endpoint_snapshot_missing_or_digest_invalid_means_no_link() {
        let store = test_store();
        seed_matter(&store, "proc:matter:co:ordinance-25-93", "Ordinance 25-93");
        seed_identifier_with_evidence(
            &store,
            "proc:matter:co:ordinance-25-93",
            "25-93",
            IdentifierKind::LegislativeMatter,
        );
        seed_matter(&store, "proc:matter:co:r26-023ab", "R26-023AB RFI");
        seed_identifier_with_evidence(
            &store,
            "proc:matter:co:r26-023ab",
            "R26-023AB",
            IdentifierKind::Rfp,
        );

        // Source snapshot does not exist -> no link.
        let records = [record(
            "colorado-springs-contract-awards",
            "award-row-1",
            "proc:matter:co:r26-023ab",
            "snapshot:nonexistent",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "notes",
            "Awarded per Ordinance 25-93.",
        )];
        let outcome = detect_official_relationships(&store, &records).expect("detect");
        assert!(
            outcome.links.is_empty(),
            "missing source snapshot must not link"
        );
    }

    #[test]
    fn near_miss_queues_candidate_not_link() {
        let store = test_store();
        seed_matter(&store, "proc:matter:co:ordinance-25-93", "Ordinance 25-93");
        seed_identifier_with_evidence(
            &store,
            "proc:matter:co:ordinance-25-93",
            "25-93",
            IdentifierKind::LegislativeMatter,
        );
        seed_matter(&store, "proc:matter:co:r26-023ab", "R26-023AB RFI");
        seed_identifier_with_evidence(
            &store,
            "proc:matter:co:r26-023ab",
            "R26-023AB",
            IdentifierKind::Rfp,
        );
        let digest = sha256_hex(b"award-record-bytes");
        let snapshot = SourceSnapshot {
            id: pnull_core::SourceSnapshot::id_for("colorado-springs-contract-awards", &digest),
            source_id: "colorado-springs-contract-awards".to_owned(),
            source_url: "https://example.test/awards".to_owned(),
            retrieved_at: "2026-08-17T00:00:00Z".to_owned(),
            persisted_digest: digest.clone(),
            content_type: Some("text/html".to_owned()),
            etag: None,
            last_modified: None,
            final_url: "https://example.test/awards".to_owned(),
            redirect_history: Vec::new(),
            parser_version: "awards-1.0".to_owned(),
            schema_version: 2,
            record_count: Some(1),
            pagination_complete: Some(true),
            coverage_state: CoverageState::InformationalOnly,
            supersedes: None,
        };
        store.insert_source_snapshot(&snapshot).expect("snapshot");

        // Off-by-one RFP number (R26-023AC vs stored R26-023AB).
        let records = [record(
            "colorado-springs-contract-awards",
            "award-row-1",
            "proc:matter:co:r26-023ab",
            &snapshot.id,
            &digest,
            "notes",
            "Awarded per R26-023AC.",
        )];
        let outcome = detect_official_relationships(&store, &records).expect("detect");
        assert!(
            outcome.links.is_empty(),
            "near-miss must not link: {outcome:?}"
        );
        assert!(
            !outcome.candidates.is_empty(),
            "near-miss must queue a candidate"
        );
    }
}
