use super::*;
use serde_json::json;
use std::os::unix::fs::symlink;

fn source(text: &str) -> CheckedSource {
    CheckedSource {
        proof: SourceProof {
            source_ref: "minutes".to_owned(),
            source_id: "test-pdf-documents".to_owned(),
            source_url: "https://example.test/minutes.pdf".to_owned(),
            sha256: "a".repeat(64),
            index_source_id: "test-index".to_owned(),
            index_sha256: "b".repeat(64),
            index_fetch_receipt_id: "index-receipt".to_owned(),
            index_review_id: "index-review".to_owned(),
            anchor_index: 0,
            evidence_id: "evidence:test".to_owned(),
            source_review_id: "document-review".to_owned(),
            fetch_receipt_id: "document-receipt".to_owned(),
            retrieved_at: "2026-11-10T00:00:00Z".to_owned(),
            extraction_method: "test".to_owned(),
        },
        text: text.to_owned(),
        maps: vec![],
    }
}

fn proof(line: usize, quote: &str) -> serde_json::Value {
    json!({"source_ref":"minutes","locator":format!("line:{line}"),"quote":quote})
}

#[test]
fn official_moffat_index_anchor_stays_on_bounded_pdf_path() {
    let index = Url::parse("https://www.townofmoffatco.gov/documents").unwrap();
    let policy = Url::parse("https://www.townofmoffatco.gov/uploads/").unwrap();
    let fixture = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/co-moffat/index-excerpt.html"),
    )
    .unwrap();
    let good = "https://www.townofmoffatco.gov/uploads/12.12.2023-meeting.pdf";
    assert!(anchor_matches(&index, &fixture, 0, good).unwrap());
    assert!(!anchor_matches(&index, &fixture, 1, good).unwrap());
    assert!(town_pdf_shape(&index, &policy, &Url::parse(good).unwrap()));
    for bad in [
        "https://www.townofmoffatco.gov/uploads/12.12.2023-meeting.pdf?download=1",
        "https://www.townofmoffatco.gov/uploads/%2e%2e/secret.pdf",
        "https://www.townofmoffatco.gov/uploads//secret.pdf",
    ] {
        assert!(!town_pdf_shape(&index, &policy, &Url::parse(bad).unwrap()));
    }
}

#[test]
fn mapped_index_is_bound_to_its_exact_config_path() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let moffat = root.join("configs/states/co-moffat.toml");
    let simla = root.join("configs/states/co-simla.toml");
    assert_eq!(
        geography_ref_for_index(&root, &moffat, "moffat-document-index").unwrap(),
        "us-co-place-0851250"
    );
    assert!(geography_ref_for_index(&root, &simla, "moffat-document-index").is_err());
}

#[test]
fn exact_line_proof_rejects_other_lines_and_invented_values() {
    let mut sources = BTreeMap::new();
    sources.insert(
        "minutes".to_owned(),
        source("Item 5\nMotion carried\nAlice Smith Aye"),
    );
    let claim: ClaimInput =
        serde_json::from_value(json!({"value":"Motion carried","proof":proof(2,"Motion carried")}))
            .unwrap();
    assert!(matches!(
        checked_claim(Some(&claim), &sources, "action").unwrap(),
        FactState::Established { .. }
    ));
    let invented: ClaimInput =
        serde_json::from_value(json!({"value":"Vendor award","proof":proof(2,"Motion carried")}))
            .unwrap();
    assert!(checked_claim(Some(&invented), &sources, "award").is_err());
    let wrong_line: ClaimInput =
        serde_json::from_value(json!({"value":"Motion carried","proof":proof(3,"Motion carried")}))
            .unwrap();
    assert!(checked_claim(Some(&wrong_line), &sources, "action").is_err());
}

#[test]
fn negated_outcome_cannot_be_reversed_by_a_substring_quote() {
    let mut sources = BTreeMap::new();
    sources.insert("minutes".to_owned(), source("Motion not approved"));
    for quote in ["approved", "not approved", "Motion not approved"] {
        let claim: ClaimInput = serde_json::from_value(json!({
            "value":"approved","proof":proof(1,quote)
        }))
        .unwrap();
        assert!(checked_claim(Some(&claim), &sources, "outcome").is_err());
    }
}

#[test]
fn aggregate_vote_language_cannot_create_named_rows() {
    let mut sources = BTreeMap::new();
    sources.insert(
        "minutes".to_owned(),
        source("Item 5\nMotion Item 5\nAll in favor"),
    );
    let matter: ClaimInput =
        serde_json::from_value(json!({"value":"Item 5","proof":proof(1,"Item 5")})).unwrap();
    let matter = checked_claim(Some(&matter), &sources, "matter").unwrap();
    let vote: VoteInput = serde_json::from_value(json!({
        "scope":"direct_item","motion":{"value":"Item 5","proof":proof(2,"Motion Item 5")},
        "rows":[{"name":"Alice Smith","position":"Aye","proof":proof(3,"All in favor")}]
    }))
    .unwrap();
    assert!(checked_votes(&vote, &matter, &sources, "test-place").is_err());
    let gap: VoteInput =
        serde_json::from_value(json!({"scope":"not_established","rows":[]})).unwrap();
    let result = checked_votes(&gap, &matter, &sources, "test-place").unwrap();
    assert!(!result.item_votes_established && result.rows.is_empty());
}

#[test]
fn consent_motion_rows_never_become_item_votes() {
    let mut sources = BTreeMap::new();
    sources.insert(
        "minutes".to_owned(),
        source("Item 5\nMotion to approve Item 5 consent agenda\nAlice Smith Aye"),
    );
    let matter: ClaimInput =
        serde_json::from_value(json!({"value":"Item 5","proof":proof(1,"Item 5")})).unwrap();
    let matter = checked_claim(Some(&matter), &sources, "matter").unwrap();
    let vote: VoteInput = serde_json::from_value(json!({
        "scope":"consent_motion","motion":{"value":"Motion to approve Item 5 consent agenda","proof":proof(2,"Motion to approve Item 5 consent agenda")},
        "rows":[{"name":"Alice Smith","position":"Aye","proof":proof(3,"Alice Smith Aye")}]
    }))
    .unwrap();
    let result = checked_votes(&vote, &matter, &sources, "test-place").unwrap();
    assert_eq!(result.rows.len(), 1);
    assert!(!result.item_votes_established);
    assert!(matches!(
        result.rows[0].role,
        FactState::NotEstablished { .. }
    ));
    assert!(matches!(
        result.rows[0].term,
        FactState::NotEstablished { .. }
    ));
    assert!(matches!(
        result.rows[0].portrait,
        FactState::NotEstablished { .. }
    ));
}

#[test]
fn consent_motion_cannot_be_relabelled_as_a_direct_item_vote() {
    let mut sources = BTreeMap::new();
    sources.insert(
        "minutes".to_owned(),
        source("Item 5\nMotion to approve Item 5 as part of consent agenda\nAlice Smith Yes"),
    );
    let matter: ClaimInput = serde_json::from_value(json!({
        "value":"Item 5","proof":proof(1,"Item 5")
    }))
    .unwrap();
    let matter = checked_claim(Some(&matter), &sources, "matter").unwrap();
    let direct: VoteInput = serde_json::from_value(json!({
        "scope":"direct_item",
        "motion":{"value":"Motion to approve Item 5 as part of consent agenda","proof":proof(2,"Motion to approve Item 5 as part of consent agenda")},
        "rows":[{"name":"Alice Smith","position":"Yes","proof":proof(3,"Alice Smith Yes")}]
    })).unwrap();
    assert!(checked_votes(&direct, &matter, &sources, "test-place").is_err());
    let consent: VoteInput = serde_json::from_value(json!({
        "scope":"consent_motion",
        "motion":{"value":"Motion to approve Item 5 as part of consent agenda","proof":proof(2,"Motion to approve Item 5 as part of consent agenda")},
        "rows":[{"name":"Alice Smith","position":"Yes","proof":proof(3,"Alice Smith Yes")}]
    })).unwrap();
    assert!(
        !checked_votes(&consent, &matter, &sources, "test-place")
            .unwrap()
            .item_votes_established
    );
}

#[test]
fn named_position_must_be_atomic_for_that_person() {
    let mut sources = BTreeMap::new();
    sources.insert(
        "minutes".to_owned(),
        source("Item 5\nMotion to approve Item 5\nAlice Smith No; Bob Jones Yes"),
    );
    let matter: ClaimInput = serde_json::from_value(json!({
        "value":"Item 5","proof":proof(1,"Item 5")
    }))
    .unwrap();
    let matter = checked_claim(Some(&matter), &sources, "matter").unwrap();
    for (position, quote) in [
        ("Yes", "Alice Smith No; Bob Jones Yes"),
        ("Yes", "Alice Smith No"),
    ] {
        let vote: VoteInput = serde_json::from_value(json!({
            "scope":"direct_item",
            "motion":{"value":"Motion to approve Item 5","proof":proof(2,"Motion to approve Item 5")},
            "rows":[{"name":"Alice Smith","position":position,"proof":proof(3,quote)}]
        })).unwrap();
        assert!(checked_votes(&vote, &matter, &sources, "test-place").is_err());
    }
    let valid: VoteInput = serde_json::from_value(json!({
        "scope":"direct_item",
        "motion":{"value":"Motion to approve Item 5","proof":proof(2,"Motion to approve Item 5")},
        "rows":[{"name":"Alice Smith","position":"No","proof":proof(3,"Alice Smith No")}]
    }))
    .unwrap();
    let result = checked_votes(&valid, &matter, &sources, "test-place").unwrap();
    assert!(result.item_votes_established);
}

#[test]
fn direct_item_motion_cannot_borrow_a_different_item_on_the_line() {
    let mut sources = BTreeMap::new();
    sources.insert(
        "minutes".to_owned(),
        source("Item 5\nItem 5; Motion to approve Item 6\nAlice Smith Yes"),
    );
    let matter: ClaimInput = serde_json::from_value(json!({
        "value":"Item 5","proof":proof(1,"Item 5")
    }))
    .unwrap();
    let matter = checked_claim(Some(&matter), &sources, "matter").unwrap();
    for motion in [
        "Motion to approve Item 6",
        "Item 5; Motion to approve Item 6",
    ] {
        let vote: VoteInput = serde_json::from_value(json!({
            "scope":"direct_item",
            "motion":{"value":motion,"proof":proof(2,"Item 5; Motion to approve Item 6")},
            "rows":[{"name":"Alice Smith","position":"Yes","proof":proof(3,"Alice Smith Yes")}]
        }))
        .unwrap();
        assert!(checked_votes(&vote, &matter, &sources, "test-place").is_err());
    }
}

#[test]
fn direct_item_vote_cannot_cross_an_intervening_motion() {
    let mut sources = BTreeMap::new();
    sources.insert(
        "minutes".to_owned(),
        source("Item 5\nMotion to approve Item 5\nMotion to approve Item 6\nAlice Smith Yes"),
    );
    let matter: ClaimInput = serde_json::from_value(json!({
        "value":"Item 5","proof":proof(1,"Item 5")
    }))
    .unwrap();
    let matter = checked_claim(Some(&matter), &sources, "matter").unwrap();
    let vote: VoteInput = serde_json::from_value(json!({
        "scope":"direct_item",
        "motion":{"value":"Motion to approve Item 5","proof":proof(2,"Motion to approve Item 5")},
        "rows":[{"name":"Alice Smith","position":"Yes","proof":proof(4,"Alice Smith Yes")}]
    }))
    .unwrap();
    assert!(checked_votes(&vote, &matter, &sources, "test-place").is_err());
}

#[test]
fn repeated_item_number_in_one_document_cannot_join_different_meetings() {
    let mut sources = BTreeMap::new();
    sources.insert(
        "minutes".to_owned(),
        source("Item 5\nMotion to approve Item 5\nAlice Smith Yes\nMeeting 2026-12-01\nItem 5\nItem 5: Flock Safety"),
    );
    let matter: ClaimInput = serde_json::from_value(json!({
        "value":"Item 5","proof":proof(1,"Item 5")
    }))
    .unwrap();
    let matter = checked_claim(Some(&matter), &sources, "matter").unwrap();
    let vote: VoteInput = serde_json::from_value(json!({
        "scope":"direct_item",
        "motion":{"value":"Motion to approve Item 5","proof":proof(2,"Motion to approve Item 5")},
        "rows":[{"name":"Alice Smith","position":"Yes","proof":proof(3,"Alice Smith Yes")}]
    }))
    .unwrap();
    assert!(checked_votes(&vote, &matter, &sources, "test-place").is_err());
    let source = sources.get("minutes").unwrap();
    assert!(!single_item_context(
        source,
        "Item 5",
        "line:1",
        &["line:6"]
    ));
}

#[test]
fn linked_item_line_can_precede_outcome_within_one_section() {
    let source = source("Item 5\nItem 5: Flock Safety\nApproved");
    assert!(single_item_context(
        &source,
        "Item 5",
        "line:1",
        &["line:2", "line:3"]
    ));
}

#[test]
fn person_attribute_must_name_the_same_person_on_an_atomic_line() {
    let mut sources = BTreeMap::new();
    sources.insert(
        "minutes".to_owned(),
        source("Bob Jones: Mayor; Alice Smith: Councilmember\nAlice Smith: Councilmember"),
    );
    let wrong: ClaimInput = serde_json::from_value(json!({
        "value":"Mayor","proof":proof(1,"Bob Jones: Mayor; Alice Smith: Councilmember")
    }))
    .unwrap();
    assert!(checked_person_claim(Some(&wrong), &sources, "role", "Alice Smith").is_err());
    let right: ClaimInput = serde_json::from_value(json!({
        "value":"Councilmember","proof":proof(2,"Alice Smith: Councilmember")
    }))
    .unwrap();
    assert!(matches!(
        checked_person_claim(Some(&right), &sources, "role", "Alice Smith").unwrap(),
        FactState::Established { .. }
    ));
}

#[test]
fn technology_needs_an_explicit_same_matter_relationship_for_case_stage() {
    let mut sources = BTreeMap::new();
    sources.insert("minutes".to_owned(), source(
        "Item 5\n2026-11-10\nTown Council\nMotion carried\nApproved\nFlock Safety\nItem 5: Flock Safety\nItem 6: Flock Safety"
    ));
    let base = json!({
        "schema_version":1,"jurisdiction_ref":"test-place",
        "sources":[{"source_ref":"minutes","archive_record":"discovery-archives/test-pdf-documents/record.json"}],
        "decision":{
            "matter":{"value":"Item 5","proof":proof(1,"Item 5")},
            "date":{"value":"2026-11-10","proof":proof(2,"2026-11-10")},
            "body":{"value":"Town Council","proof":proof(3,"Town Council")},
            "action":{"value":"Motion carried","proof":proof(4,"Motion carried")},
            "outcome":{"value":"Approved","proof":proof(5,"Approved")}
        },
        "technology":{"value":"Flock Safety","proof":proof(6,"Flock Safety")},
        "votes":{"scope":"not_established"},
        "unresolved_question":"Was a contract awarded?"
    });
    let root = tempfile::tempdir().unwrap();
    let unlinked: ProposalInput = serde_json::from_value(base.clone()).unwrap();
    assert_eq!(
        compose_proposal(root.path(), &unlinked, &sources)
            .unwrap()
            .stage,
        "source_checked_screening"
    );
    let mut wrong = base.clone();
    wrong["technology_relationship"] = json!({
        "value":"Item 6: Flock Safety","proof":proof(8,"Item 6: Flock Safety")
    });
    let wrong: ProposalInput = serde_json::from_value(wrong).unwrap();
    assert!(compose_proposal(root.path(), &wrong, &sources).is_err());
    let mut other = source("Item 5: Flock Safety");
    other.proof.source_ref = "other".to_owned();
    sources.insert("other".to_owned(), other);
    let mut cross_source = base.clone();
    cross_source["sources"].as_array_mut().unwrap().push(json!({
        "source_ref":"other",
        "archive_record":"discovery-archives/test-pdf-documents/other.json"
    }));
    cross_source["technology_relationship"] = json!({
        "value":"Item 5: Flock Safety",
        "proof":{"source_ref":"other","locator":"line:1","quote":"Item 5: Flock Safety"}
    });
    let cross_source: ProposalInput = serde_json::from_value(cross_source).unwrap();
    assert!(compose_proposal(root.path(), &cross_source, &sources).is_err());
    let mut linked = base;
    linked["technology_relationship"] = json!({
        "value":"Item 5: Flock Safety","proof":proof(7,"Item 5: Flock Safety")
    });
    let linked: ProposalInput = serde_json::from_value(linked).unwrap();
    let proposal = compose_proposal(root.path(), &linked, &sources).unwrap();
    assert_eq!(proposal.stage, "source_checked_case_proposal");
    assert!(matches!(
        proposal.contract_award_link,
        FactState::NotEstablished { .. }
    ));
}

#[test]
fn contract_authorization_needs_an_atomic_approved_item_link() {
    let mut sources = BTreeMap::new();
    sources.insert("minutes".to_owned(), source(
        "Item 5\n2026-11-10\nTown Council\nMotion to approve Item 5\nApproved\nFlock Safety\nItem 5: Flock Safety\nAgreement 123\nItem 5 | Agreement 123 | Flock Safety | award authorized"
    ));
    let mut input = json!({
        "schema_version":1,"jurisdiction_ref":"test-place",
        "sources":[{"source_ref":"minutes","archive_record":"discovery-archives/test-pdf-documents/record.json"}],
        "decision":{
            "matter":{"value":"Item 5","proof":proof(1,"Item 5")},
            "date":{"value":"2026-11-10","proof":proof(2,"2026-11-10")},
            "body":{"value":"Town Council","proof":proof(3,"Town Council")},
            "action":{"value":"Motion to approve Item 5","proof":proof(4,"Motion to approve Item 5")},
            "outcome":{"value":"Approved","proof":proof(5,"Approved")}
        },
        "technology":{"value":"Flock Safety","proof":proof(6,"Flock Safety")},
        "technology_relationship":{"value":"Item 5: Flock Safety","proof":proof(7,"Item 5: Flock Safety")},
        "contract_reference":{"value":"Agreement 123","proof":proof(8,"Agreement 123")},
        "contract_award_link":{"value":"Item 5 | Agreement 123 | Flock Safety | award authorized",
            "proof":proof(9,"Item 5 | Agreement 123 | Flock Safety | award authorized")},
        "votes":{"scope":"not_established"},"unresolved_question":"Was the agreement signed?"
    });
    let root = tempfile::tempdir().unwrap();
    let checked: ProposalInput = serde_json::from_value(input.clone()).unwrap();
    let result = compose_proposal(root.path(), &checked, &sources).unwrap();
    assert!(matches!(
        result.contract_award_link,
        FactState::Established { .. }
    ));
    input["decision"]["outcome"] = json!({"value":"Failed","proof":proof(5,"Approved")});
    let rejected: ProposalInput = serde_json::from_value(input.clone()).unwrap();
    assert!(compose_proposal(root.path(), &rejected, &sources).is_err());
    input["decision"]["outcome"] = json!({"value":"Approved","proof":proof(5,"Approved")});
    input["contract_award_link"]["value"] =
        json!("Item 5 | Agreement 123 | Flock Safety | not award authorized");
    let negated: ProposalInput = serde_json::from_value(input).unwrap();
    assert!(compose_proposal(root.path(), &negated, &sources).is_err());
}

#[test]
fn direct_item_votes_require_matter_and_nearby_named_rows() {
    let mut sources = BTreeMap::new();
    let mut lines = vec!["Item 5".to_owned(), "Motion to approve Item 5".to_owned()];
    lines.extend((0..30).map(|_| "other public matter".to_owned()));
    lines.push("Alice Smith Aye".to_owned());
    sources.insert("minutes".to_owned(), source(&lines.join("\n")));
    let matter: ClaimInput =
        serde_json::from_value(json!({"value":"Item 5","proof":proof(1,"Item 5")})).unwrap();
    let matter = checked_claim(Some(&matter), &sources, "matter").unwrap();
    let distant: VoteInput = serde_json::from_value(json!({
        "scope":"direct_item",
        "motion":{"value":"Motion to approve Item 5","proof":proof(2,"Motion to approve Item 5")},
        "rows":[{"name":"Alice Smith","position":"Aye","proof":proof(33,"Alice Smith Aye")}]
    }))
    .unwrap();
    assert!(checked_votes(&distant, &matter, &sources, "test-place").is_err());
    let unrelated: VoteInput = serde_json::from_value(json!({
        "scope":"direct_item",
        "motion":{"value":"other public matter","proof":proof(3,"other public matter")},
        "rows":[{"name":"Alice Smith","position":"Aye","proof":proof(33,"Alice Smith Aye")}]
    }))
    .unwrap();
    assert!(checked_votes(&unrelated, &matter, &sources, "test-place").is_err());
}

#[test]
fn empty_source_claims_are_private_screening_with_explicit_gaps() {
    let input: ProposalInput = serde_json::from_value(json!({
        "schema_version":1,"jurisdiction_ref":"test-place",
        "sources":[{"source_ref":"minutes","archive_record":"discovery-archives/test-pdf-documents/record.json"}],
        "votes":{"scope":"not_established"},
        "unresolved_question":"Does this source contain a relevant public decision?"
    })).unwrap();
    let mut sources = BTreeMap::new();
    sources.insert(
        "minutes".to_owned(),
        source("An ordinary public meeting index"),
    );
    let root = tempfile::tempdir().unwrap();
    let first = compose_proposal(root.path(), &input, &sources).unwrap();
    let second = compose_proposal(root.path(), &input, &sources).unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(first.stage, "source_checked_screening");
    assert_eq!(
        first.publication_state,
        "private_pending_publication_checks"
    );
    assert!(matches!(first.technology, FactState::NotEstablished { .. }));
    assert!(matches!(
        first.contract_award_link,
        FactState::NotEstablished { .. }
    ));
    assert!(first.correction_history.is_empty());
    persist(root.path(), &first).unwrap();
    let correction: ProposalInput = serde_json::from_value(json!({
        "schema_version":1,"jurisdiction_ref":"test-place",
        "sources":[{"source_ref":"minutes","archive_record":"discovery-archives/test-pdf-documents/record.json"}],
        "votes":{"scope":"not_established"},
        "unresolved_question":"Is there a relevant decision in the record?",
        "supersedes_proposal_id":first.id,
        "correction_note":"Corrected the open question after checking the page."
    }))
    .unwrap();
    let corrected = compose_proposal(root.path(), &correction, &sources).unwrap();
    assert_ne!(corrected.id, first.id);
    assert_eq!(corrected.correction_history.len(), 1);
    assert_eq!(
        corrected.correction_history[0].superseded_proposal_id,
        first.id
    );
    let wrong_jurisdiction: ProposalInput = serde_json::from_value(json!({
        "schema_version":1,"jurisdiction_ref":"another-town",
        "sources":[{"source_ref":"minutes","archive_record":"discovery-archives/test-pdf-documents/record.json"}],
        "votes":{"scope":"not_established"},
        "unresolved_question":"Does this source contain a relevant public decision?",
        "supersedes_proposal_id":first.id,
        "correction_note":"Claiming another town's correction history"
    })).unwrap();
    assert!(compose_proposal(root.path(), &wrong_jurisdiction, &sources).is_err());
}

#[test]
fn private_artifact_symlinks_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), root.path().join("discovery-archives")).unwrap();
    assert!(
        private_path(
            root.path(),
            Path::new("discovery-archives/x/file.json"),
            "discovery-archives"
        )
        .is_err()
    );
}

#[test]
fn conditional_receipt_needs_prior_exact_bytes_and_public_network() {
    let data = tempfile::tempdir().unwrap();
    let store = Store::open(data.path()).unwrap();
    let source_id = "test-documents";
    let url = "https://example.test/uploads/minutes.pdf";
    let digest = "a".repeat(64);
    let captured = FetchObservation {
        id: FetchObservation::id_for(url, "2026-11-09T00:00:00Z", 200),
        source_id: Some(source_id.to_owned()),
        requested_url: url.to_owned(),
        resolved_ips: vec!["93.184.216.34".to_owned()],
        retrieved_at: "2026-11-09T00:00:00Z".to_owned(),
        method: "GET".to_owned(),
        status_code: 200,
        redirect_target: None,
        final_url: url.to_owned(),
        allowlisted_headers: vec![],
        content_type: Some("application/pdf".to_owned()),
        content_length: Some(100),
        etag: Some("tag".to_owned()),
        last_modified: None,
        body_digest: Some(digest.clone()),
        error: None,
    };
    let unchanged = FetchObservation {
        id: FetchObservation::id_for(url, "2026-11-10T00:00:00Z", 304),
        retrieved_at: "2026-11-10T00:00:00Z".to_owned(),
        status_code: 304,
        content_type: None,
        content_length: None,
        body_digest: None,
        ..captured.clone()
    };
    store.insert_fetch_observation(&unchanged).unwrap();
    assert!(matching_receipt(&store, source_id, url, &digest, Some(&unchanged.id), None).is_err());
    store.insert_fetch_observation(&captured).unwrap();
    let (matched, mime) = matching_receipt(
        &store,
        source_id,
        url,
        &digest,
        Some(&unchanged.id),
        Some(&unchanged.retrieved_at),
    )
    .unwrap();
    assert_eq!(matched.id, unchanged.id);
    assert_eq!(mime, "application/pdf");
    let replaced = FetchObservation {
        id: FetchObservation::id_for(url, "2026-11-09T12:00:00Z", 200),
        retrieved_at: "2026-11-09T12:00:00Z".to_owned(),
        body_digest: Some("b".repeat(64)),
        ..captured
    };
    store.insert_fetch_observation(&replaced).unwrap();
    assert!(matching_receipt(&store, source_id, url, &digest, Some(&unchanged.id), None).is_err());
}
