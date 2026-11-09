//! CORA request ledger — the local, append-only gap-closing loop (Item 3).
//!
//! Connects `procurement gaps` -> CORA draft -> human submission -> response
//! import -> case-file gap update. The tool never sends anything, never guesses
//! a recipient, and never claims a legal deadline or entitlement. Every state
//! transition is an immutable event appended to the request's event list;
//! corrections are new events, never edits to prior ones.

use pnull_core::{CoraRequest, CoraRequestEvent, CoraRequestState, Store, sha256_hex, stable_id};
use thiserror::Error;

/// The fixed creation timestamp for offline demonstrations (deterministic).
pub const OFFLINE_CREATED_AT: &str = "2026-08-17T00:00:00Z";

#[derive(Debug, Error)]
pub enum CoraLedgerError {
    #[error("request {0} not found")]
    RequestNotFound(String),
    #[error("transition refused: request {0} is already {1:?}")]
    AlreadyInState(String, CoraRequestState),
    #[error("transition refused: response evidence id {0} does not match an imported record")]
    EvidenceNotFound(String),
    #[error("store operation failed: {0}")]
    Store(#[from] pnull_core::CoreError),
}

/// Computes a stable gap-set digest over the gap summary of a request.
///
/// The digest is over the institution, identifiers, missing record types,
/// date range, vendor/project name, and sources already checked — the full set
/// of facts that define a gap. Two requests describing the same gap share the
/// same digest; the request id further keys on matter and creation timestamp.
pub fn gap_set_digest(
    institution: &str,
    identifiers: &[String],
    missing_record_types: &[String],
    date_range: &Option<(Option<String>, Option<String>)>,
    vendor_or_project: &Option<String>,
    sources_checked: &[String],
) -> String {
    let mut parts = Vec::new();
    parts.push(institution.to_owned());
    parts.extend(identifiers.iter().cloned());
    parts.extend(missing_record_types.iter().cloned());
    match date_range {
        Some((Some(start), Some(end))) => {
            parts.push(start.clone());
            parts.push(end.clone());
        }
        Some((Some(start), None)) => parts.push(start.clone()),
        Some((None, Some(end))) => parts.push(end.clone()),
        _ => parts.push("<no-range>".to_owned()),
    }
    parts.push(vendor_or_project.clone().unwrap_or_default());
    parts.extend(sources_checked.iter().cloned());
    sha256_hex(parts.join("\u{1f}").as_bytes())
}

/// Registers a newly drafted CORA request in the ledger (state `drafted`).
///
/// Returns `false` when a request with the same id already exists (the draft
/// path is idempotent for the same matter + gap set + creation timestamp).
#[allow(clippy::too_many_arguments)]
pub fn register_draft(
    store: &Store,
    matter_id: &str,
    institution: &str,
    identifiers: Vec<String>,
    missing_record_types: Vec<String>,
    date_range: Option<(Option<String>, Option<String>)>,
    vendor_or_project: Option<String>,
    sources_checked: Vec<String>,
    draft_text: &str,
    created_at: &str,
) -> Result<bool, CoraLedgerError> {
    let gap_digest = gap_set_digest(
        institution,
        &identifiers,
        &missing_record_types,
        &date_range,
        &vendor_or_project,
        &sources_checked,
    );
    let id = CoraRequest::id_for(matter_id, &gap_digest, created_at);
    let event = CoraRequestEvent {
        id: stable_id(
            "cora-event",
            &[&id, CoraRequestState::Drafted.label(), created_at],
        ),
        request_id: id.clone(),
        state: CoraRequestState::Drafted,
        operator: "draft".to_owned(),
        timestamp: created_at.to_owned(),
        note: "draft generated".to_owned(),
    };
    let request = CoraRequest {
        id,
        matter_id: matter_id.to_owned(),
        state: CoraRequestState::Drafted,
        gap_set_digest: gap_digest,
        created_at: created_at.to_owned(),
        institution: institution.to_owned(),
        identifiers,
        missing_record_types,
        date_range,
        vendor_or_project,
        sources_checked,
        draft_text: draft_text.to_owned(),
        draft_digest: sha256_hex(draft_text.as_bytes()),
        events: vec![event],
    };
    Ok(store.insert_cora_request(&request)?)
}

/// Applies a state transition to a request by appending an immutable event.
///
/// This is append-only: prior events are never modified, reordered, or removed;
/// only a new event is appended and the derived `state` field advances. The
/// prior events remain byte-for-byte in order. Returns the updated request.
fn transition(
    store: &Store,
    request_id: &str,
    new_state: CoraRequestState,
    operator: &str,
    timestamp: &str,
    note: &str,
) -> Result<CoraRequest, CoraLedgerError> {
    let mut request = store.cora_request(request_id)?;
    if request.state == new_state {
        return Err(CoraLedgerError::AlreadyInState(
            request_id.to_owned(),
            new_state,
        ));
    }
    let event = CoraRequestEvent {
        id: stable_id("cora-event", &[request_id, new_state.label(), timestamp]),
        request_id: request_id.to_owned(),
        state: new_state,
        operator: operator.to_owned(),
        timestamp: timestamp.to_owned(),
        note: note.to_owned(),
    };
    request.state = new_state;
    request.events.push(event);
    store.update_cora_request(&request)?;
    Ok(request)
}

/// Marks a request as submitted by the human operator (state `submitted`).
///
/// The recipient, date, and tracking reference are operator-supplied facts
/// about an action the human performed; the tool stores them, it does not
/// perform them. `recipient_note` is optional and recorded in the event note.
pub fn submit(
    store: &Store,
    request_id: &str,
    operator: &str,
    date: &str,
    tracking: &str,
    recipient_note: Option<&str>,
) -> Result<CoraRequest, CoraLedgerError> {
    let note = format!(
        "submitted on {date} by operator {operator}; tracking ref '{tracking}'{}",
        recipient_note.map_or(String::new(), |n| format!("; recipient note: {n}"))
    );
    transition(
        store,
        request_id,
        CoraRequestState::Submitted,
        operator,
        date,
        &note,
    )
}

/// Links an imported response evidence to a request (state `response_received`).
///
/// `evidence_id` must match an imported record (`supplied-record:<id>`) that was
/// persisted through the hostile-file import path. The evidence digest was
/// already validated at import time; here we only confirm the record exists.
pub fn response_received(
    store: &Store,
    request_id: &str,
    evidence_id: &str,
    note: Option<&str>,
) -> Result<CoraRequest, CoraLedgerError> {
    // The evidence must be an imported supplied record persisted via the
    // hostile-file import path. A fabricated or missing id is refused.
    if store.supplied_record_json(evidence_id)?.is_none() {
        return Err(CoraLedgerError::EvidenceNotFound(evidence_id.to_owned()));
    }
    let note = format!(
        "response evidence {evidence_id} linked{}",
        note.map_or(String::new(), |n| format!("; note: {n}"))
    );
    transition(
        store,
        request_id,
        CoraRequestState::ResponseReceived,
        "response-import",
        OFFLINE_CREATED_AT,
        &note,
    )
}

/// Marks a request's gap as resolved by cited evidence (state `gap_resolved`).
pub fn gap_resolved(
    store: &Store,
    request_id: &str,
    operator: &str,
    note: &str,
) -> Result<CoraRequest, CoraLedgerError> {
    transition(
        store,
        request_id,
        CoraRequestState::GapResolved,
        operator,
        OFFLINE_CREATED_AT,
        note,
    )
}

/// Marks a request's gap as still unresolved (state `still_unresolved`).
pub fn still_unresolved(
    store: &Store,
    request_id: &str,
    operator: &str,
    note: &str,
) -> Result<CoraRequest, CoraLedgerError> {
    transition(
        store,
        request_id,
        CoraRequestState::StillUnresolved,
        operator,
        OFFLINE_CREATED_AT,
        note,
    )
}

/// Lists all requests, optionally filtered by matter.
pub fn list(store: &Store, matter_id: Option<&str>) -> Result<Vec<CoraRequest>, CoraLedgerError> {
    let all = match matter_id {
        Some(id) => store.cora_requests(id)?,
        None => store.all_cora_requests()?,
    };
    Ok(all)
}

/// Reads a single request by id.
pub fn show(store: &Store, request_id: &str) -> Result<CoraRequest, CoraLedgerError> {
    Ok(store.cora_request(request_id)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pnull_core::Store;
    use tempfile::tempdir;

    fn seed() -> (tempfile::TempDir, Store, String) {
        let dir = tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        let inserted = register_draft(
            &store,
            "matter:1",
            "City of Colorado Springs",
            vec!["R26-023AB".to_owned()],
            vec!["executed contract".to_owned()],
            Some((Some("2026-01-01".to_owned()), Some("2026-08-17".to_owned()))),
            Some("Transit Fare".to_owned()),
            vec!["colorado-springs-contract-awards".to_owned()],
            "draft text",
            OFFLINE_CREATED_AT,
        )
        .expect("register");
        assert!(inserted);
        let id = store.all_cora_requests().expect("all")[0].id.clone();
        (dir, store, id)
    }

    #[test]
    fn draft_is_registered_in_drafted_state() {
        let (_dir, store, id) = seed();
        let request = store.cora_request(&id).expect("request");
        assert_eq!(request.state, CoraRequestState::Drafted);
        assert_eq!(request.events.len(), 1);
        assert_eq!(request.events[0].state, CoraRequestState::Drafted);
    }

    #[test]
    fn duplicate_draft_is_refused_idempotently() {
        let dir = tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        let first = register_draft(
            &store,
            "matter:1",
            "City",
            vec![],
            vec!["award".to_owned()],
            None,
            None,
            vec![],
            "text",
            OFFLINE_CREATED_AT,
        )
        .expect("first");
        let second = register_draft(
            &store,
            "matter:1",
            "City",
            vec![],
            vec!["award".to_owned()],
            None,
            None,
            vec![],
            "text",
            OFFLINE_CREATED_AT,
        )
        .expect("second");
        assert!(first);
        assert!(!second);
        assert_eq!(store.all_cora_requests().expect("all").len(), 1);
    }

    #[test]
    fn full_lifecycle_happy_path() {
        let (_dir, store, id) = seed();
        store
            .insert_supplied_record_json(
                "supplied-record:abc",
                "digest",
                "{\"id\":\"supplied-record:abc\"}",
            )
            .expect("insert evidence");
        let submitted =
            submit(&store, &id, "operator-a", "2026-08-20", "TRK-001", None).expect("submit");
        assert_eq!(submitted.state, CoraRequestState::Submitted);
        assert_eq!(submitted.events.len(), 2);

        let received =
            response_received(&store, &id, "supplied-record:abc", None).expect("received");
        assert_eq!(received.state, CoraRequestState::ResponseReceived);
        assert_eq!(received.events.len(), 3);

        let resolved = gap_resolved(&store, &id, "operator-a", "gap covered by cited evidence")
            .expect("resolved");
        assert_eq!(resolved.state, CoraRequestState::GapResolved);
        assert_eq!(resolved.events.len(), 4);
        // Prior events are preserved in order (append-only).
        let states: Vec<&str> = resolved.events.iter().map(|e| e.state.label()).collect();
        assert_eq!(
            states,
            ["drafted", "submitted", "response_received", "gap_resolved"]
        );
    }

    #[test]
    fn duplicate_transition_is_refused() {
        let (_dir, store, id) = seed();
        submit(&store, &id, "op", "2026-08-20", "TRK", None).expect("submit");
        assert!(matches!(
            submit(&store, &id, "op", "2026-08-21", "TRK2", None),
            Err(CoraLedgerError::AlreadyInState(
                _,
                CoraRequestState::Submitted
            ))
        ));
    }

    #[test]
    fn response_without_matching_evidence_is_refused() {
        let (_dir, store, id) = seed();
        submit(&store, &id, "op", "2026-08-20", "TRK", None).expect("submit");
        assert!(matches!(
            response_received(&store, &id, "supplied-record:missing", None),
            Err(CoraLedgerError::EvidenceNotFound(_))
        ));
    }

    #[test]
    fn still_unresolved_keeps_gap_visible() {
        let (_dir, store, id) = seed();
        store
            .insert_supplied_record_json(
                "supplied-record:resp1",
                "digest",
                "{\"id\":\"supplied-record:resp1\"}",
            )
            .expect("insert evidence");
        submit(&store, &id, "op", "2026-08-20", "TRK", None).expect("submit");
        let unresolved = still_unresolved(&store, &id, "op", "response does not cover the gap")
            .expect("unresolved");
        assert_eq!(unresolved.state, CoraRequestState::StillUnresolved);
        // The response digest is recorded in the event list.
        assert!(
            unresolved
                .events
                .iter()
                .any(|e| e.note.contains("does not cover"))
        );
    }

    #[test]
    fn gap_set_digest_is_stable_and_distinguishes_gaps() {
        let a = gap_set_digest(
            "City",
            &["R1".to_owned()],
            &["award".to_owned()],
            &Some((Some("2026".to_owned()), None)),
            &Some("X".to_owned()),
            &["s1".to_owned()],
        );
        let b = gap_set_digest(
            "City",
            &["R1".to_owned()],
            &["award".to_owned()],
            &Some((Some("2026".to_owned()), None)),
            &Some("X".to_owned()),
            &["s1".to_owned()],
        );
        let c = gap_set_digest(
            "City",
            &["R2".to_owned()],
            &["award".to_owned()],
            &Some((Some("2026".to_owned()), None)),
            &Some("X".to_owned()),
            &["s1".to_owned()],
        );
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
