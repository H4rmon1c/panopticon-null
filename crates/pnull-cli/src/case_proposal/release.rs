//! Conservative bridge from a freshly rechecked private proposal to a
//! single-case static atlas snapshot. The input is rebuilt from candidate
//! archives on every release; a persisted proposal JSON is never trusted.

use super::{
    FactProof, FactState, MAX_INPUT_BYTES, Proposal, ProposalInput, SourceProof, build,
    read_regular_bounded,
};
use crate::StateConfig;
use anyhow::{Context, Result, bail};
use pnull_core::{ReviewState, Store, sha256_hex, stable_id};
use pnull_publish::{publication_allowlist_allows, validate_public_text};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::Path;
use url::Url;

fn established<'a>(fact: &'a FactState, label: &str) -> Result<(&'a str, &'a FactProof)> {
    match fact {
        FactState::Established { value, proof } => Ok((value, proof)),
        FactState::NotEstablished { .. } => bail!("release needs an established {label}"),
    }
}

fn source_id(proof: &FactProof) -> String {
    stable_id("atlas-proposal-source", &[&proof.source_url, &proof.sha256])
}

fn evidence_id(case_id: &str, key: &str) -> String {
    stable_id("atlas-proposal-evidence", &[case_id, key])
}

fn add_evidence(
    output: &mut Vec<Value>,
    case_id: &str,
    key: &str,
    fact: &FactState,
) -> Result<Option<String>> {
    let FactState::Established { value, proof } = fact else {
        return Ok(None);
    };
    let id = evidence_id(case_id, key);
    let digest = sha256_hex(
        serde_json::to_vec(&json!([
            case_id,
            key,
            value,
            proof.source_url,
            proof.sha256,
            proof.locator,
            proof.quote,
            proof.fetch_receipt_id
        ]))?
        .as_slice(),
    );
    output.push(json!({
        "id":id,"source_id":source_id(proof),"sha256":proof.sha256,
        "locator":proof.locator,"quote":proof.quote,"claim":value,
        "review_state":"SOURCE_VERIFIED","verification_digest":format!("sha256:{digest}"),
        "corrections":[]
    }));
    Ok(Some(id))
}

fn add_attribute(
    attributes: &mut Vec<Value>,
    evidence: &mut Vec<Value>,
    case_id: &str,
    key: &str,
    label: &str,
    fact: &FactState,
) -> Result<()> {
    if let Some(id) = add_evidence(evidence, case_id, key, fact)? {
        let (value, _) = established(fact, label)?;
        attributes.push(json!({"label":label,"value":value,"evidence_id":id,
            "review_state":"SOURCE_VERIFIED","corrections":[]}));
    }
    Ok(())
}

fn check_current_bytes(store: &Store, source: &SourceProof) -> Result<()> {
    let observations = store.fetch_observations(&source.source_id)?;
    let latest_200 = observations
        .iter()
        .filter(|item| {
            item.requested_url == source.source_url
                && item.final_url == source.source_url
                && item.error.is_none()
                && item.status_code == 200
        })
        .max_by(|a, b| a.retrieved_at.cmp(&b.retrieved_at))
        .context("release source has no successful raw-byte fetch receipt")?;
    if latest_200.body_digest.as_deref() != Some(&source.sha256) {
        bail!(
            "release source changed after its proposal: {}",
            source.source_url
        );
    }
    Ok(())
}

fn public_text(value: &Value) -> Result<()> {
    match value {
        Value::String(text) => validate_public_text(text)?,
        Value::Array(items) => {
            for item in items {
                public_text(item)?;
            }
        }
        Value::Object(fields) => {
            for item in fields.values() {
                public_text(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// This is a geography join, not evidence that a government made a decision.
/// The proposal builder separately checks the official index/config mapping.
fn jurisdiction_entity(repository_root: &Path, jurisdiction_ref: &str) -> Result<Value> {
    let roster_bytes = read_regular_bounded(
        &repository_root.join("web/panopticon-terminal/geography/colorado-jurisdictions.json"),
        16 * 1024 * 1024,
    )?;
    let roster: Value = serde_json::from_slice(&roster_bytes)?;
    let rows = roster["jurisdictions"]
        .as_array()
        .context("geography roster lacks jurisdictions")?;
    if roster["schema_version"] != 1 || rows.len() > 10_000 {
        bail!("invalid bounded geography roster");
    }
    let matches = rows
        .iter()
        .filter(|row| row["id"] == jurisdiction_ref)
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        bail!("proposal jurisdiction must match exactly one geography roster entry");
    }
    let row = matches[0];
    let name = row["name"].as_str().context("geography entry lacks name")?;
    if name.trim().is_empty() || name.len() > 300 {
        bail!("geography entry lacks a bounded name");
    }
    Ok(json!({
        "id":stable_id("atlas-proposal-jurisdiction", &[jurisdiction_ref]),
        "type":"JURISDICTION","name":name,"jurisdiction_ref":jurisdiction_ref,
        "status":"geography indexed","subtitle":"Colorado geography index",
        "description":"Geography roster identity, joined through the configured official document index. This entry alone establishes no decision or technology use.",
        "geo":null,"jurisdiction_ids":[],"source_count":0,"relation_count":0,
        "source_ids":[],"attributes":[],"relationships":[],"timeline":[]
    }))
}

#[allow(clippy::too_many_lines)]
fn snapshot(
    store: &Store,
    repository_root: &Path,
    proposal: &Proposal,
    site_url: &str,
) -> Result<Value> {
    if proposal.stage != "source_checked_case_proposal"
        || proposal.publication_state != "private_pending_publication_checks"
    {
        bail!("screening proposals cannot be released");
    }
    let origin = Url::parse(site_url)?;
    if origin.scheme() != "https"
        || origin.host_str().is_none()
        || !origin.username().is_empty()
        || origin.password().is_some()
        || origin.path() != "/"
        || origin.query().is_some()
        || origin.fragment().is_some()
        || origin.host_str().is_some_and(|host| {
            host == "localhost" || host.ends_with(".invalid") || host.contains("example")
        })
    {
        bail!("case release needs a real HTTPS site origin");
    }
    if !publication_allowlist_allows(store, "atlas_release")? {
        bail!("atlas_release allowlist is absent");
    }
    let (matter, _) = established(&proposal.decision.matter, "matter")?;
    let (date, _) = established(&proposal.decision.date, "date")?;
    let (_body, _) = established(&proposal.decision.body, "governing body")?;
    let (action, _) = established(&proposal.decision.action, "action")?;
    let (outcome, _) = established(&proposal.decision.outcome, "outcome")?;
    let (technology, _) = established(&proposal.technology, "technology")?;
    established(&proposal.technology_relationship, "technology relationship")?;
    if time::Date::parse(date, &time::format_description::well_known::Iso8601::DATE).is_err() {
        bail!("release date must be a source-backed ISO calendar date");
    }
    let lower_outcome = outcome.to_ascii_lowercase();
    let outcome_class = match lower_outcome.as_str() {
        "pass" | "passed" | "approved" | "motion carried" => "approved",
        "fail" | "failed" | "denied" | "motion failed" => "failed",
        _ => bail!("outcome needs an unambiguous source-backed result"),
    };
    if proposal.votes.scope == "consent_motion" {
        bail!("consent motions need a separate block-vote release adapter");
    }
    if !matches!(
        proposal.votes.scope.as_str(),
        "direct_item" | "not_established"
    ) {
        bail!("unsupported release vote scope");
    }
    for source in &proposal.source_proofs {
        check_current_bytes(store, source)?;
    }
    let suffix = proposal
        .id
        .strip_prefix("case-proposal:")
        .context("proposal has an invalid stable ID")?;
    let case_id = format!("co-record-{}", &suffix[..16]);
    let binding = format!("sha256:{}", sha256_hex(&serde_json::to_vec(proposal)?));
    let review_id = stable_id("atlas-release", &[&case_id]);
    let history = store.reviews_for_citation(&review_id)?;
    if let Some(current) = store.current_review(&review_id)?
        && (current.state != ReviewState::Approved
            || current.bound_digest != binding
            || current.reviewer.trim().is_empty()
            || ["demo", "test", "seed", "bot"]
                .iter()
                .any(|word| current.reviewer.to_ascii_lowercase().contains(word)))
    {
        bail!("current case review vetoes proposal release");
    }
    let corrections = proposal
        .correction_history
        .iter()
        .map(|entry| {
            json!({
                "state":"superseded_proposal","prior_proposal_id":entry.superseded_proposal_id,
                "prior_digest":entry.superseded_artifact_sha256
            })
        })
        .chain(
            history
                .iter()
                .filter(|item| item.state != ReviewState::Approved)
                .map(|item| json!({"state":item.state.label(),"at":item.decided_at})),
        )
        .collect::<Vec<_>>();
    let release = json!({"state":"SOURCE_VERIFIED","bound_digest":binding,
        "reviewed_at":null,"verification_method":"preserved_source_bytes_and_exact_locators",
        "corrections":corrections});
    let sources = proposal.source_proofs.iter().map(|source| {
        let id = stable_id("atlas-proposal-source", &[&source.source_url,&source.sha256]);
        (id.clone(), json!({
        "id":id,
        "title":"Official decision document","authority":"Official public record",
        "source_type":"official_record","canonical_url":source.source_url,
        "retrieved_at":source.retrieved_at,"document_date":null,
        "sha256":source.sha256,
        "description":"Preserved raw bytes, exact locator, reviewed source policy, and public-network receipt"
    }))}).collect::<BTreeMap<_, _>>().into_values().collect::<Vec<_>>();
    let mut evidence = Vec::new();
    let mut attributes = Vec::new();
    for (key, label, fact) in [
        ("matter", "Matter", &proposal.decision.matter),
        ("date", "Decision date", &proposal.decision.date),
        ("body", "Governing body", &proposal.decision.body),
        ("action", "Recorded action", &proposal.decision.action),
        ("outcome", "Outcome", &proposal.decision.outcome),
        ("vote_motion", "Recorded motion", &proposal.votes.motion),
        ("technology", "Technology", &proposal.technology),
        (
            "technology_relationship",
            "Technology relationship",
            &proposal.technology_relationship,
        ),
        (
            "contract_reference",
            "Contract reference",
            &proposal.contract_reference,
        ),
        (
            "contract_award_link",
            "Award authorization",
            &proposal.contract_award_link,
        ),
    ] {
        add_attribute(&mut attributes, &mut evidence, &case_id, key, label, fact)?;
    }
    let technology_id = evidence_id(&case_id, "technology_relationship");
    let outcome_id = evidence_id(&case_id, "outcome");
    let source_ids = proposal
        .source_proofs
        .iter()
        .map(|source| {
            stable_id(
                "atlas-proposal-source",
                &[&source.source_url, &source.sha256],
            )
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let jurisdiction = jurisdiction_entity(repository_root, &proposal.jurisdiction_ref)?;
    let jurisdiction_id = jurisdiction["id"]
        .as_str()
        .context("jurisdiction ID")?
        .to_owned();
    let mut entities = vec![jurisdiction];
    let mut votes = Vec::new();
    let mut relationships = Vec::new();
    if let FactState::Established {
        value: contract,
        proof,
    } = &proposal.contract_reference
    {
        let authorized = matches!(proposal.contract_award_link, FactState::Established { .. });
        if authorized {
            let contract_id = stable_id("atlas-proposal-contract", &[&case_id, contract]);
            entities.push(json!({"id":contract_id,"type":"CONTRACT","name":contract,
                "jurisdiction_ref":proposal.jurisdiction_ref,"jurisdiction_ids":[jurisdiction_id],"status":"award authorized",
                "source_count":1,"relation_count":0,"source_ids":[source_id(proof)],
                "attributes":[{"label":"Contract reference","value":contract,
                    "evidence_id":evidence_id(&case_id,"contract_reference"),
                    "review_state":"SOURCE_VERIFIED","corrections":[]}],
                "relationships":[],"timeline":[],"geo":null,
                "unknowns":[{"kind":"execution","state":"not_established",
                    "detail":"An authorized award does not establish a signed agreement or payment"}]}));
            relationships.push(json!({"id":stable_id("atlas-proposal-award", &[&case_id]),
                "type":"AUTHORIZED_AWARD","label":"award authorized",
                "target_entity_id":contract_id,"source_count":1,
                "evidence_id":evidence_id(&case_id,"contract_award_link"),
                "review_state":"SOURCE_VERIFIED","corrections":[]}));
        }
    }
    for (index, row) in proposal.votes.rows.iter().enumerate() {
        let key = format!("vote_{index}");
        let fact = FactState::Established {
            value: row.position.clone(),
            proof: row.proof.clone(),
        };
        let vote_evidence = add_evidence(&mut evidence, &case_id, &key, &fact)?
            .context("named vote lacks proof")?;
        evidence.last_mut().context("named vote evidence")?["verification_method"] =
            json!("exact_document_line_named_vote_v1");
        let voter_id = stable_id(
            "atlas-proposal-voter",
            &[
                &proposal.jurisdiction_ref,
                &case_id,
                &row.name,
                &row.proof.locator,
            ],
        );
        let mut voter_attributes = Vec::new();
        for (label, claim) in [
            ("Public role", &row.role),
            ("Office term", &row.term),
            ("Official portrait", &row.portrait),
        ] {
            let field = format!("voter_{index}_{}", label.replace(' ', "_").to_lowercase());
            add_attribute(
                &mut voter_attributes,
                &mut evidence,
                &case_id,
                &field,
                label,
                claim,
            )?;
        }
        entities.push(json!({"id":voter_id,"type":"RECORDED_VOTER","name":row.name,
            "identity_scope":format!("decision:{case_id}"),"jurisdiction_ref":proposal.jurisdiction_ref,
            "jurisdiction_ids":[jurisdiction_id],
            "source_count":1,"relation_count":0,"source_ids":[source_id(&row.proof)],
            "attributes":voter_attributes,"relationships":[],"timeline":[],"geo":null,
            "unknowns":[
                {"kind":"public_role","state":if matches!(&row.role,FactState::Established { .. }) {"established"} else {"not_established"}},
                {"kind":"office_term","state":if matches!(&row.term,FactState::Established { .. }) {"established"} else {"not_established"}},
                {"kind":"official_portrait","state":if matches!(&row.portrait,FactState::Established { .. }) {"established"} else {"not_established"}}
            ],
            "status":"recorded","tags":["named public vote"]}));
        votes.push(
            json!({"name":row.name,"vote":row.position,"voter_entity_id":voter_id,
            "evidence_id":vote_evidence,"review_state":"SOURCE_VERIFIED","corrections":[]}),
        );
    }
    if proposal.votes.scope == "direct_item" && votes.is_empty() {
        bail!("direct vote has no rows");
    }
    let award_established = !relationships.is_empty();
    let unknowns = json!([
        {"kind":"contract_award_link","state":if award_established {"established"} else {"not_established"},
            "detail":if award_established {"Award authorization is cited; execution and payment remain unestablished"}
                else {"A contract reference does not establish an award or execution"}},
        {"kind":"individual_votes","state":if votes.is_empty() {"not_established"} else {"established"},
            "detail":if votes.is_empty() {"No named direct-item vote was established"} else {"Named direct-item positions are cited"}}
    ]);
    let title = format!("{matter}: {technology} ({outcome_class})");
    let award_summary = if award_established {
        "Award authorization is cited; execution and payment remain unestablished."
    } else {
        "Contract award link not established."
    };
    let summary = format!("{action}. Outcome: {outcome}. {award_summary}");
    entities.push(
        json!({"id":stable_id("atlas-proposal-decision", &[&case_id]),
        "type":"DECISION","slug":case_id,"name":title,"social_title":title,
        "social_summary":summary,"summary":summary,"description":summary,
        "jurisdiction_ref":proposal.jurisdiction_ref,"jurisdiction_ids":[jurisdiction_id],"status":"recorded","geo":null,
        "tags":["official decision",outcome_class],"updated_at":date,
        "source_count":source_ids.len(),"relation_count":relationships.len(),"source_ids":source_ids,
        "attributes":attributes,"relationships":relationships,
        "timeline":[{"id":format!("{case_id}_decision"),"date":date,
            "type":"RECORDED_DECISION","title":title,"evidence_id":outcome_id,
            "review_state":"SOURCE_VERIFIED","corrections":[]}],
        "votes":votes,"vote_record_state":if votes.is_empty() {Value::Null} else {json!("direct")},
        "technology_context":[{"name":technology,"relationship":"recorded context",
            "evidence_id":technology_id,"review_state":"SOURCE_VERIFIED","corrections":[]}],
        "unknowns":unknowns,"unresolved_question":proposal.unresolved_question,
        "share_url":format!("{}decisions/{case_id}.html",origin.as_str())}),
    );
    if proposal.votes.scope == "direct_item" {
        let (_, motion_proof) = established(&proposal.votes.motion, "direct-item motion")?;
        entities.last_mut().context("decision entity")?["document_vote_context"] = json!({
            "verification_method":"bounded_exact_document_lines_v1","scope":"direct_item",
            "source_id":source_id(motion_proof),
            "matter_evidence_id":evidence_id(&case_id,"matter"),
            "motion_evidence_id":evidence_id(&case_id,"vote_motion")
        });
    }
    // A missing vote means that the official material has not established
    // individual positions; omit the stricter empty-API vote-record state.
    if proposal.votes.scope == "not_established" {
        entities
            .last_mut()
            .context("decision entity")?
            .as_object_mut()
            .context("decision object")?
            .remove("vote_record_state");
    }
    let relationship_count = usize::from(award_established);
    let output = json!({"schema_version":"pnull-public-snapshot-v1","demo":false,"preview":false,
        "status":{"state":"released","dataset_version":binding,"last_ingest":null,
            "last_publish":null,"manifest_digest":binding,
            "counts":{"entities":entities.len(),"records":evidence.len(),
                "sources":sources.len(),"relationships":relationship_count}},
        "release_review":release,"case_releases":[{"case_id":case_id,"release":release}],
        "activity":[],"entities":entities,"evidence":evidence,"sources":sources});
    public_text(&output)?;
    Ok(output)
}

pub(crate) fn release(
    store: &Store,
    config_path: &Path,
    config: &StateConfig,
    site_url: Option<&str>,
    input_path: &Path,
    output_path: &Path,
) -> Result<()> {
    let output = checked_snapshot(store, config_path, config, site_url, input_path)?;
    let parent = output_path.parent().context("output has no parent")?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".pnull-case-release-")
        .tempfile_in(parent)?;
    serde_json::to_writer_pretty(&mut temporary, &output)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(output_path)
        .map_err(|error| error.error)?;
    println!(
        "wrote source-verified proposal case to {}",
        output_path.display()
    );
    Ok(())
}

pub(crate) fn checked_snapshot(
    store: &Store,
    config_path: &Path,
    config: &StateConfig,
    site_url: Option<&str>,
    input_path: &Path,
) -> Result<Value> {
    let site_url = site_url.context("case release needs explicit --site-url")?;
    let raw = read_regular_bounded(input_path, MAX_INPUT_BYTES)?;
    let input: ProposalInput = serde_json::from_slice(&raw)?;
    let config_path = config_path.canonicalize()?;
    let root = config_path
        .ancestors()
        .nth(3)
        .context("config is not under a repository states directory")?;
    let proposal = build(store, root, &config_path, config, &input)?;
    snapshot(store, root, &proposal, site_url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::case_proposal::{CheckedSource, DecisionFacts, VoteFacts, compose_proposal};
    use std::path::PathBuf;

    fn repository_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }
    use pnull_core::{FetchObservation, PublicationAllowlist, ReviewDecision};

    fn fact(value: &str, locator: &str) -> FactState {
        FactState::Established {
            value: value.to_owned(),
            proof: FactProof {
                source_ref: "minutes".to_owned(),
                source_id: "test-documents".to_owned(),
                source_url: "https://official.example.org/minutes.html".to_owned(),
                sha256: "a".repeat(64),
                evidence_id: "evidence:test".to_owned(),
                fetch_receipt_id: "fetch:test".to_owned(),
                locator: locator.to_owned(),
                quote: value.to_owned(),
            },
        }
    }

    fn proposed() -> Proposal {
        Proposal {
            schema_version: 1,
            id: format!("case-proposal:{}", "b".repeat(64)),
            stage: "source_checked_case_proposal".to_owned(),
            publication_state: "private_pending_publication_checks".to_owned(),
            jurisdiction_ref: "us-co-place-0816495".to_owned(),
            source_proofs: vec![SourceProof {
                source_ref: "minutes".to_owned(),
                source_id: "test-documents".to_owned(),
                source_url: "https://official.example.org/minutes.html".to_owned(),
                sha256: "a".repeat(64),
                index_source_id: "test-index".to_owned(),
                index_sha256: "c".repeat(64),
                index_fetch_receipt_id: "fetch:index".to_owned(),
                index_review_id: "review:index".to_owned(),
                anchor_index: 0,
                evidence_id: "evidence:test".to_owned(),
                source_review_id: "review:document".to_owned(),
                fetch_receipt_id: "fetch:test".to_owned(),
                retrieved_at: "2026-11-10T12:00:00Z".to_owned(),
                extraction_method: "html".to_owned(),
            }],
            decision: DecisionFacts {
                matter: fact("Item 5", "line:1"),
                date: fact("2026-11-10", "line:2"),
                body: fact("City Council", "line:3"),
                action: fact("Motion to approve Item 5", "line:4"),
                outcome: fact("Fail", "line:5"),
            },
            technology: fact("Flock", "line:6"),
            technology_relationship: fact("Item 5: Flock", "line:7"),
            contract_reference: super::super::missing("no contract reference"),
            contract_award_link: super::super::missing("no award"),
            official_role: super::super::missing("no person"),
            office_term: super::super::missing("no person"),
            portrait_reference: super::super::missing("no person"),
            votes: VoteFacts {
                scope: "not_established".to_owned(),
                motion: super::super::missing("no motion"),
                rows: vec![],
                item_votes_established: false,
            },
            unresolved_question: "Was a contract later awarded?".to_owned(),
            correction_history: vec![],
            supersedes_proposal_id: None,
        }
    }

    /// Cross-language fixture producer: the actual Rust claim/context checker,
    /// release exporter, and bundle join run before JavaScript sees any data.
    /// The source is synthetic extracted text; live/archive ingestion is not
    /// claimed by this test and is covered separately by candidate tests.
    #[test]
    #[allow(clippy::too_many_lines)]
    fn checked_document_proposals_export_for_browser() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        store
            .insert_publication_allowlist(&PublicationAllowlist {
                id: "allowlist:proposal-bridge-test".to_owned(),
                field_categories: vec!["atlas_release".to_owned()],
                created_at: "2026-11-10T12:00:00Z".to_owned(),
                note: "Synthetic offline bridge regression only".to_owned(),
            })
            .unwrap();
        let text = [5, 6].iter().map(|number| format!(
            "Item {number}\n2026-11-10\nTown Council\nMotion to approve Item {number}\nAlice Smith Aye\nNo — Bob Jones\nApproved\nFlock Safety\nItem {number}: Flock Safety\nAgreement {number}\nItem {number} | Agreement {number} | Flock Safety | award authorized"
        )).collect::<Vec<_>>().join("\n");
        let mut source = proposed().source_proofs.remove(0);
        source.sha256 = sha256_hex(text.as_bytes());
        source.index_source_id = "moffat-document-index".to_owned();
        let receipt = FetchObservation {
            id: FetchObservation::id_for(&source.source_url, &source.retrieved_at, 200),
            source_id: Some(source.source_id.clone()),
            requested_url: source.source_url.clone(),
            final_url: source.source_url.clone(),
            method: "GET".to_owned(),
            status_code: 200,
            retrieved_at: source.retrieved_at.clone(),
            resolved_ips: vec!["93.184.216.34".to_owned()],
            redirect_target: None,
            content_type: Some("text/html".to_owned()),
            content_length: Some(text.len() as u64),
            etag: None,
            last_modified: None,
            allowlisted_headers: vec![],
            body_digest: Some(source.sha256.clone()),
            error: None,
        };
        source.fetch_receipt_id.clone_from(&receipt.id);
        store.insert_fetch_observation(&receipt).unwrap();
        let checked = BTreeMap::from([(
            "minutes".to_owned(),
            CheckedSource {
                proof: source,
                text,
                maps: vec![],
            },
        )]);
        let root = repository_root();
        let jurisdiction_ref = super::super::geography_ref_for_index(
            &root,
            &root.join("configs/states/co-moffat.toml"),
            "moffat-document-index",
        )
        .unwrap();
        let mut generated = Vec::new();
        for (index, number) in [5, 6].iter().enumerate() {
            let proof = |line: usize, quote: &str| {
                json!({
                    "source_ref":"minutes","locator":format!("line:{}",line + index * 11),"quote":quote
                })
            };
            let claim = |line: usize, value: &str| json!({"value":value,"proof":proof(line,value)});
            let matter = format!("Item {number}");
            let motion = format!("Motion to approve Item {number}");
            let input: ProposalInput = serde_json::from_value(json!({
                "schema_version":1,"jurisdiction_ref":jurisdiction_ref,
                "sources":[{"source_ref":"minutes","archive_record":"discovery-archives/test-documents/synthetic.json"}],
                "decision":{"matter":claim(1,&matter),"date":claim(2,"2026-11-10"),
                    "body":claim(3,"Town Council"),"action":claim(4,&motion),"outcome":claim(7,"Approved")},
                "technology":claim(8,"Flock Safety"),
                "technology_relationship":claim(9,&format!("Item {number}: Flock Safety")),
                "contract_reference":claim(10,&format!("Agreement {number}")),
                "contract_award_link":claim(11,&format!("Item {number} | Agreement {number} | Flock Safety | award authorized")),
                "votes":{"scope":"direct_item","motion":claim(4,&motion),"rows":[
                    {"name":"Alice Smith","position":"Aye","proof":proof(5,"Alice Smith Aye")},
                    {"name":"Bob Jones","position":"No","proof":proof(6,"No — Bob Jones")}
                ]},"unresolved_question":"Was the authorized agreement executed?"
            })).unwrap();
            let proposal = compose_proposal(store.data_dir(), &input, &checked).unwrap();
            assert!(proposal.votes.item_votes_established);
            let output = snapshot(&store, &root, &proposal, "https://panopticon.fail").unwrap();
            let decision = output["entities"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item["type"] == "DECISION")
                .unwrap();
            assert_eq!(decision["votes"].as_array().unwrap().len(), 2);
            assert_eq!(decision["jurisdiction_ids"].as_array().unwrap().len(), 1);
            assert_eq!(decision["document_vote_context"]["scope"], "direct_item");
            generated.push(output);
        }
        let mut combined: Value = serde_json::from_slice(
            &fs::read(root.join("web/panopticon-terminal/public/public-snapshot.json")).unwrap(),
        )
        .unwrap();
        let previous_sources = combined["sources"].as_array().unwrap().len();
        let previous_decisions = combined["case_releases"].as_array().unwrap().len();
        for proposal in &generated {
            pnull_publish::atlas::append_verified_proposal_snapshot(&mut combined, proposal)
                .unwrap();
        }
        assert_eq!(
            combined["case_releases"].as_array().unwrap().len(),
            previous_decisions + 2
        );
        assert_eq!(
            combined["sources"].as_array().unwrap().len(),
            previous_sources + 1
        );
        assert_eq!(
            combined["entities"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|entity| entity["type"] == "JURISDICTION"
                    && entity["jurisdiction_ref"] == jurisdiction_ref)
                .count(),
            1
        );
        assert!(
            pnull_publish::atlas::append_verified_proposal_snapshot(
                &mut combined.clone(),
                &generated[0]
            )
            .is_err()
        );
        let mut conflicting = generated[1].clone();
        conflicting["sources"][0]["title"] = json!("conflicting title for one stable ID");
        let mut first = generated[0].clone();
        assert!(
            pnull_publish::atlas::append_verified_proposal_snapshot(&mut first, &conflicting)
                .is_err()
        );
        if let Ok(path) = std::env::var("PNUL_DOCUMENT_BRIDGE_TEST_OUTPUT") {
            fs::write(
                path,
                serde_json::to_vec(&json!({"proposals":generated,"combined":combined})).unwrap(),
            )
            .unwrap();
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn failed_decision_keeps_vote_and_award_gaps() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("store");
        store
            .insert_publication_allowlist(&PublicationAllowlist {
                id: "allowlist:test".to_owned(),
                field_categories: vec!["atlas_release".to_owned()],
                created_at: "2026-11-10T12:00:00Z".to_owned(),
                note: "test only".to_owned(),
            })
            .expect("allowlist");
        let receipt = FetchObservation {
            id: FetchObservation::id_for(
                "https://official.example.org/minutes.html",
                "2026-11-10T12:00:00Z",
                200,
            ),
            source_id: Some("test-documents".to_owned()),
            requested_url: "https://official.example.org/minutes.html".to_owned(),
            final_url: "https://official.example.org/minutes.html".to_owned(),
            method: "GET".to_owned(),
            status_code: 200,
            retrieved_at: "2026-11-10T12:00:00Z".to_owned(),
            resolved_ips: vec!["93.184.216.34".to_owned()],
            redirect_target: None,
            content_type: Some("text/html".to_owned()),
            content_length: Some(8),
            etag: None,
            last_modified: None,
            allowlisted_headers: vec![],
            body_digest: Some("a".repeat(64)),
            error: None,
        };
        store.insert_fetch_observation(&receipt).expect("receipt");
        let proposal = proposed();
        let output = snapshot(
            &store,
            &repository_root(),
            &proposal,
            "https://panopticon.fail",
        )
        .expect("snapshot");
        let decision = output["entities"]
            .as_array()
            .expect("entities")
            .iter()
            .find(|entity| entity["type"] == "DECISION")
            .expect("decision");
        assert_eq!(decision["tags"][1], "failed");
        assert!(decision["votes"].as_array().expect("votes").is_empty());
        assert!(decision.get("vote_record_state").is_none());
        assert_eq!(decision["unknowns"][0]["state"], "not_established");
        assert_eq!(
            output["case_releases"][0]["release"]["state"],
            "SOURCE_VERIFIED"
        );
        if let Ok(path) = std::env::var("PNUL_PROPOSAL_RELEASE_TEST_OUTPUT") {
            fs::write(path, serde_json::to_vec_pretty(&output).expect("JSON"))
                .expect("QA snapshot");
        }
        let mut authorized = proposed();
        authorized.decision.outcome = fact("Approved", "line:5");
        authorized.contract_reference = fact("Agreement 123", "line:8");
        authorized.contract_award_link = fact(
            "Item 5 | Agreement 123 | Flock | award authorized",
            "line:9",
        );
        let linked = snapshot(
            &store,
            &repository_root(),
            &authorized,
            "https://panopticon.fail",
        )
        .expect("authorized snapshot");
        let linked_decision = linked["entities"]
            .as_array()
            .expect("entities")
            .iter()
            .find(|entity| entity["type"] == "DECISION")
            .expect("decision");
        assert_eq!(
            linked_decision["relationships"]
                .as_array()
                .expect("links")
                .len(),
            1
        );
        assert_eq!(linked_decision["unknowns"][0]["state"], "established");
        assert!(
            linked_decision["summary"]
                .as_str()
                .unwrap()
                .contains("Award authorization is cited")
        );
        assert!(
            !linked_decision["summary"]
                .as_str()
                .unwrap()
                .contains("Contract award link not established")
        );
        assert!(
            linked["entities"]
                .as_array()
                .expect("entities")
                .iter()
                .any(|entity| entity["type"] == "CONTRACT")
        );

        let req = stable_id("atlas-release", &["co-record-bbbbbbbbbbbbbbbb"]);
        store
            .insert_review(&ReviewDecision {
                id: ReviewDecision::id_for(&req, "2026-11-10T13:00:00Z"),
                citation_id: req,
                state: ReviewState::Rejected,
                reviewer: "Editor".to_owned(),
                note: "withdraw".to_owned(),
                bound_digest: output["release_review"]["bound_digest"]
                    .as_str()
                    .expect("binding")
                    .to_owned(),
                decision_digest: String::new(),
                decided_at: "2026-11-10T13:00:00Z".to_owned(),
                supersedes: None,
            })
            .expect("withdrawal");
        assert!(
            snapshot(
                &store,
                &repository_root(),
                &proposal,
                "https://panopticon.fail"
            )
            .is_err()
        );
    }
}
