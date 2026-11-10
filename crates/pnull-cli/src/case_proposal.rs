//! Private, document-first case proposals. A candidate archive is a source,
//! never a vote, contract award, or publishable decision by itself.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::IpAddr;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use pnull_core::{
    ExtractionStatus, FetchObservation, SourceType, Store, TextMap, sha256_hex, stable_id,
};
use pnull_http::validate_resolved;
use pnull_ingest::{Budgets, BuildMetadata, IngestRequest, RealSandbox, Tracker, ingest_bytes};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use url::Url;

use crate::{StateConfig, source_config_digest, verify_source_review};

const MAX_INPUT_BYTES: u64 = 1_048_576;
const MAX_PROPOSAL_BYTES: u64 = 4_194_304;
const MAX_ARCHIVE_BYTES: u64 = 16_384;
const MAX_ARCHIVES: usize = 8;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SourceInput {
    source_ref: String,
    archive_record: PathBuf,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProofInput {
    source_ref: String,
    locator: String,
    quote: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ClaimInput {
    value: String,
    proof: ProofInput,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DecisionInput {
    matter: Option<ClaimInput>,
    date: Option<ClaimInput>,
    body: Option<ClaimInput>,
    action: Option<ClaimInput>,
    outcome: Option<ClaimInput>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct VoteRowInput {
    name: String,
    position: String,
    proof: ProofInput,
    role: Option<ClaimInput>,
    term: Option<ClaimInput>,
    portrait: Option<ClaimInput>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct VoteInput {
    scope: String,
    motion: Option<ClaimInput>,
    #[serde(default)]
    rows: Vec<VoteRowInput>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProposalInput {
    schema_version: u8,
    jurisdiction_ref: String,
    sources: Vec<SourceInput>,
    #[serde(default)]
    decision: DecisionInput,
    technology: Option<ClaimInput>,
    technology_relationship: Option<ClaimInput>,
    contract_reference: Option<ClaimInput>,
    official_role: Option<ClaimInput>,
    office_term: Option<ClaimInput>,
    portrait_reference: Option<ClaimInput>,
    votes: VoteInput,
    unresolved_question: String,
    #[serde(default)]
    supersedes_proposal_id: Option<String>,
    #[serde(default)]
    correction_note: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CandidateArchive {
    schema_version: u8,
    calendar_source_id: String,
    calendar_snapshot_sha256: String,
    anchor_index: usize,
    candidate_kind: String,
    candidate_url: String,
    document_source_id: String,
    document_source_config_digest: String,
    document_source_review_id: String,
    document_sha256: String,
    changed_from_sha256: Option<String>,
    fetch_receipt_id: String,
    retrieved_at: String,
}

#[derive(Clone, Serialize)]
struct SourceProof {
    source_ref: String,
    source_id: String,
    source_url: String,
    sha256: String,
    index_source_id: String,
    index_sha256: String,
    index_fetch_receipt_id: String,
    index_review_id: String,
    anchor_index: usize,
    evidence_id: String,
    source_review_id: String,
    fetch_receipt_id: String,
    retrieved_at: String,
    extraction_method: String,
}

struct CheckedSource {
    proof: SourceProof,
    text: String,
    maps: Vec<TextMap>,
}

#[derive(Clone, Serialize)]
struct FactProof {
    source_ref: String,
    source_id: String,
    source_url: String,
    sha256: String,
    evidence_id: String,
    fetch_receipt_id: String,
    locator: String,
    quote: String,
}

#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum FactState {
    Established { value: String, proof: FactProof },
    NotEstablished { detail: String },
}

#[derive(Serialize)]
struct DecisionFacts {
    matter: FactState,
    date: FactState,
    body: FactState,
    action: FactState,
    outcome: FactState,
}

#[derive(Serialize)]
struct VoteRow {
    id: String,
    name: String,
    position: String,
    proof: FactProof,
    role: FactState,
    term: FactState,
    portrait: FactState,
}

#[derive(Serialize)]
struct VoteFacts {
    scope: String,
    motion: FactState,
    rows: Vec<VoteRow>,
    item_votes_established: bool,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CorrectionEntry {
    superseded_proposal_id: String,
    superseded_artifact_sha256: String,
    note: String,
}

#[derive(Serialize)]
struct Proposal {
    schema_version: u8,
    id: String,
    stage: String,
    publication_state: String,
    jurisdiction_ref: String,
    source_proofs: Vec<SourceProof>,
    decision: DecisionFacts,
    technology: FactState,
    technology_relationship: FactState,
    contract_reference: FactState,
    contract_award_link: FactState,
    official_role: FactState,
    office_term: FactState,
    portrait_reference: FactState,
    votes: VoteFacts,
    unresolved_question: String,
    correction_history: Vec<CorrectionEntry>,
    supersedes_proposal_id: Option<String>,
}

pub(super) fn propose(
    store: &Store,
    config_path: &Path,
    config: &StateConfig,
    input_path: &Path,
) -> Result<()> {
    let input_raw = read_regular_bounded(input_path, MAX_INPUT_BYTES)?;
    let input: ProposalInput = serde_json::from_slice(&input_raw)?;
    let config_path = config_path.canonicalize()?;
    let root = config_path
        .ancestors()
        .nth(3)
        .context("config is not under a repository states directory")?;
    if root.join("configs/states") != config_path.parent().context("config has no parent")? {
        bail!("case proposals need a config under configs/states");
    }
    let proposal = build(store, root, &config_path, config, &input)?;
    let path = persist(store.data_dir(), &proposal)?;
    println!(
        "private {} {}\n  artifact: {}",
        proposal.stage,
        proposal.id,
        path.display()
    );
    Ok(())
}

fn build(
    store: &Store,
    repository_root: &Path,
    config_path: &Path,
    config: &StateConfig,
    input: &ProposalInput,
) -> Result<Proposal> {
    if input.schema_version != 1 || input.sources.is_empty() || input.sources.len() > MAX_ARCHIVES {
        bail!("proposal needs 1 to {MAX_ARCHIVES} candidate archives under schema version 1");
    }
    if input.unresolved_question.trim().is_empty() || input.unresolved_question.len() > 1_000 {
        bail!("proposal needs a bounded unresolved question");
    }
    let mut checked = BTreeMap::new();
    let mut mapped_ref = None;
    for source in &input.sources {
        check_key(&source.source_ref)?;
        if checked.contains_key(&source.source_ref) {
            bail!("duplicate proposal source reference: {}", source.source_ref);
        }
        let result = check_candidate_archive(store, config, source)?;
        let ref_id = geography_ref_for_index(repository_root, config_path, &result.1)?;
        if ref_id != input.jurisdiction_ref || mapped_ref.as_ref().is_some_and(|old| old != &ref_id)
        {
            bail!("candidate source is mapped to a different Colorado jurisdiction");
        }
        mapped_ref = Some(ref_id);
        checked.insert(source.source_ref.clone(), result.0);
    }
    compose_proposal(store.data_dir(), input, &checked)
}

#[allow(clippy::too_many_lines)]
fn compose_proposal(
    data_dir: &Path,
    input: &ProposalInput,
    checked: &BTreeMap<String, CheckedSource>,
) -> Result<Proposal> {
    if input.official_role.is_some()
        || input.office_term.is_some()
        || input.portrait_reference.is_some()
    {
        bail!(
            "global official claims need a named, source-bound subject; use voter-specific fields"
        );
    }
    let decision = DecisionFacts {
        matter: checked_claim(input.decision.matter.as_ref(), checked, "matter")?,
        date: checked_claim(input.decision.date.as_ref(), checked, "date")?,
        body: checked_claim(input.decision.body.as_ref(), checked, "body")?,
        action: checked_claim(input.decision.action.as_ref(), checked, "action")?,
        outcome: checked_claim(input.decision.outcome.as_ref(), checked, "outcome")?,
    };
    let technology = checked_claim(input.technology.as_ref(), checked, "technology")?;
    let technology_relationship = checked_claim(
        input.technology_relationship.as_ref(),
        checked,
        "technology relationship",
    )?;
    if let FactState::Established {
        value: relationship,
        proof: relationship_proof,
    } = &technology_relationship
    {
        let (
            FactState::Established {
                value: matter_value,
                proof: matter_proof,
            },
            FactState::Established {
                value: technology_value,
                ..
            },
        ) = (&decision.matter, &technology)
        else {
            bail!("technology relationship requires established matter and technology");
        };
        if !atomic_relationship(relationship, matter_value, technology_value) {
            bail!("technology relationship does not atomically bind this matter and technology");
        }
        let same_decision_source = [&decision.action, &decision.outcome].iter().all(|fact| {
            matches!(
                fact,
                FactState::Established { proof, .. }
                    if proof.source_ref == relationship_proof.source_ref
            )
        });
        if relationship_proof.source_ref != matter_proof.source_ref || !same_decision_source {
            bail!("technology relationship must be in the same archived decision source");
        }
        let source = checked
            .get(&relationship_proof.source_ref)
            .context("technology relationship source is missing")?;
        if !single_item_context(
            source,
            matter_value,
            &matter_proof.locator,
            &[
                &relationship_proof.locator,
                match &decision.action {
                    FactState::Established { proof, .. } => &proof.locator,
                    FactState::NotEstablished { .. } => unreachable!("action checked above"),
                },
                match &decision.outcome {
                    FactState::Established { proof, .. } => &proof.locator,
                    FactState::NotEstablished { .. } => unreachable!("outcome checked above"),
                },
            ],
        ) {
            bail!("technology relationship lacks one bounded item context");
        }
    }
    let contract_reference = checked_claim(
        input.contract_reference.as_ref(),
        checked,
        "contract reference",
    )?;
    let official_role = missing("No named subject is bound to a global public role");
    let office_term = missing("No named subject is bound to a global office term");
    let portrait_reference = missing("No named subject is bound to a global portrait");
    let votes = checked_votes(
        &input.votes,
        &decision.matter,
        checked,
        &input.jurisdiction_ref,
    )?;
    let complete_decision = matches!(decision.matter, FactState::Established { .. })
        && matches!(decision.date, FactState::Established { .. })
        && matches!(decision.body, FactState::Established { .. })
        && matches!(decision.action, FactState::Established { .. })
        && matches!(decision.outcome, FactState::Established { .. });
    let stage = if complete_decision
        && matches!(technology, FactState::Established { .. })
        && matches!(technology_relationship, FactState::Established { .. })
    {
        "source_checked_case_proposal"
    } else {
        "source_checked_screening"
    };
    let source_proofs = checked
        .values()
        .map(|source| source.proof.clone())
        .collect::<Vec<_>>();
    let source_binding = serde_json::to_vec(&source_proofs)?;
    let claims_binding = serde_json::to_vec(&serde_json::json!({
        "decision": &input.decision,
        "technology": &input.technology,
        "technology_relationship": &input.technology_relationship,
        "contract_reference": &input.contract_reference,
        "official_role": &input.official_role,
        "office_term": &input.office_term,
        "portrait_reference": &input.portrait_reference,
        "votes": &input.votes,
        "unresolved_question": &input.unresolved_question,
        "supersedes": &input.supersedes_proposal_id,
        "correction_note": &input.correction_note,
    }))?;
    let id = stable_id(
        "case-proposal",
        &[
            &input.jurisdiction_ref,
            &sha256_hex(&source_binding),
            &sha256_hex(&claims_binding),
        ],
    );
    let correction_history = match (&input.supersedes_proposal_id, &input.correction_note) {
        (None, None) => Vec::new(),
        (Some(previous), Some(note)) if !note.trim().is_empty() && note.len() <= 1_000 => {
            if previous == &id {
                bail!("a proposal cannot supersede itself");
            }
            let (previous_digest, mut history) =
                check_previous(data_dir, previous, &input.jurisdiction_ref)?;
            if history.len() >= 100 {
                bail!("proposal correction history is too long");
            }
            history.push(CorrectionEntry {
                superseded_proposal_id: previous.clone(),
                superseded_artifact_sha256: previous_digest,
                note: note.clone(),
            });
            history
        }
        _ => bail!("a correction requires both the prior proposal ID and a bounded note"),
    };
    Ok(Proposal {
        schema_version: 1,
        id,
        stage: stage.to_owned(),
        publication_state: "private_pending_publication_checks".to_owned(),
        jurisdiction_ref: input.jurisdiction_ref.clone(),
        source_proofs,
        decision,
        technology,
        technology_relationship,
        contract_reference,
        contract_award_link: missing(
            "A document mention alone does not establish a contract award or approval link",
        ),
        official_role,
        office_term,
        portrait_reference,
        votes,
        unresolved_question: input.unresolved_question.clone(),
        correction_history,
        supersedes_proposal_id: input.supersedes_proposal_id.clone(),
    })
}

fn missing(detail: &str) -> FactState {
    FactState::NotEstablished {
        detail: detail.to_owned(),
    }
}

fn checked_claim(
    input: Option<&ClaimInput>,
    sources: &BTreeMap<String, CheckedSource>,
    label: &str,
) -> Result<FactState> {
    let Some(input) = input else {
        return Ok(missing(&format!(
            "{label} is not established from the cited archived sources"
        )));
    };
    if input.value.trim().is_empty() || input.value.len() > 2_000 {
        bail!("empty or oversized {label} claim");
    }
    let proof = checked_proof(&input.proof, sources)?;
    // A substring can invert the meaning of a line ("not approved" ->
    // "approved"). Scalar claims remain verbatim, complete extracted lines
    // until a source-specific field parser can establish stronger semantics.
    let source = sources
        .get(&input.proof.source_ref)
        .context("claim references an unarchived source")?;
    let line = input
        .proof
        .locator
        .strip_prefix("line:")
        .and_then(|value| value.parse::<usize>().ok())
        .and_then(|number| number.checked_sub(1))
        .and_then(|index| source.text.lines().nth(index))
        .context("scalar claim requires an exact extracted line locator")?;
    if input.value.trim() != proof.quote.trim() || line.trim() != proof.quote.trim() {
        bail!("{label} value must equal its complete cited line");
    }
    Ok(FactState::Established {
        value: input.value.clone(),
        proof,
    })
}

// Keep motion, row, and scope checks together so one path cannot skip a gate.
#[allow(clippy::too_many_lines)]
fn checked_votes(
    input: &VoteInput,
    matter: &FactState,
    sources: &BTreeMap<String, CheckedSource>,
    jurisdiction_ref: &str,
) -> Result<VoteFacts> {
    match input.scope.as_str() {
        "not_established" => {
            if input.motion.is_some() || !input.rows.is_empty() {
                bail!("vote gap cannot carry inferred motion or named rows");
            }
            Ok(VoteFacts {
                scope: input.scope.clone(),
                motion: missing(
                    "individual named votes are not established from the cited sources",
                ),
                rows: vec![],
                item_votes_established: false,
            })
        }
        "direct_item" | "consent_motion" => {
            if input.rows.is_empty() || input.rows.len() > 100 {
                bail!("named vote scope requires 1 to 100 explicit rows");
            }
            if !matches!(matter, FactState::Established { .. }) {
                bail!("named vote rows require a source-backed matter reference");
            }
            let motion_input = input
                .motion
                .as_ref()
                .context("named vote rows require a cited motion context")?;
            let motion = checked_claim(Some(motion_input), sources, "motion")?;
            let motion_ref = &motion_input.proof.source_ref;
            let FactState::Established {
                value: matter_value,
                proof: matter_proof,
            } = matter
            else {
                unreachable!("matter was checked above");
            };
            if input.scope == "direct_item" {
                let motion_text = motion_input.value.as_str();
                let item_marker_count = motion_text
                    .to_ascii_lowercase()
                    .match_indices("item ")
                    .count();
                let motion_lower = motion_text.to_ascii_lowercase();
                if matter_proof.source_ref != *motion_ref
                    || !contains_complete_phrase(motion_text, matter_value)
                    || motion_text.contains(';')
                    || (!motion_lower.contains("motion") && !motion_lower.contains("moved"))
                    || ["consent", "agenda", "en bloc", "all items", "calendar"]
                        .iter()
                        .any(|marker| motion_lower.contains(marker))
                    || (matter_value.to_ascii_lowercase().starts_with("item ")
                        && item_marker_count != 1)
                    || !sources.get(motion_ref).is_some_and(|source| {
                        single_item_context(
                            source,
                            matter_value,
                            &matter_proof.locator,
                            &[&motion_input.proof.locator],
                        )
                    })
                {
                    bail!(
                        "direct item motion must unambiguously name the matter in the same source"
                    );
                }
            } else {
                let motion_lower = motion_input.value.to_ascii_lowercase();
                if matter_proof.source_ref != *motion_ref
                    || !contains_complete_phrase(&motion_input.value, matter_value)
                    || !["consent", "agenda"]
                        .iter()
                        .any(|marker| motion_lower.contains(marker))
                    || !sources.get(motion_ref).is_some_and(|source| {
                        single_item_context(
                            source,
                            matter_value,
                            &matter_proof.locator,
                            &[&motion_input.proof.locator],
                        )
                    })
                {
                    bail!("consent motion must explicitly name the matter in the same source");
                }
            }
            let mut names = BTreeSet::new();
            let mut rows = Vec::new();
            for row in &input.rows {
                if row.name.trim().is_empty()
                    || row.name.len() > 160
                    || !names.insert(row.name.to_lowercase())
                {
                    bail!("blank or duplicate named voter");
                }
                if !matches!(
                    row.position.as_str(),
                    "Yes" | "No" | "Aye" | "Nay" | "Abstain" | "Absent" | "Present"
                ) {
                    bail!("unsupported named vote position");
                }
                if &row.proof.source_ref != motion_ref {
                    bail!("named vote row and motion must use the same source");
                }
                if input.scope == "direct_item" {
                    let source = sources
                        .get(motion_ref)
                        .context("motion source is missing")?;
                    if !direct_item_row_in_context(
                        source,
                        &motion_input.proof.locator,
                        &row.proof.locator,
                    ) {
                        bail!("named direct-item vote row is outside its cited motion context");
                    }
                }
                let proof = checked_proof(&row.proof, sources)?;
                if !atomic_named_vote(&proof.quote, &row.name, &row.position) {
                    bail!("named vote row quote must bind one exact name to one position");
                }
                rows.push(VoteRow {
                    id: stable_id(
                        "proposal-vote",
                        &[
                            jurisdiction_ref,
                            &motion_input.value,
                            &row.name,
                            &proof.sha256,
                        ],
                    ),
                    name: row.name.clone(),
                    position: row.position.clone(),
                    proof,
                    role: checked_person_claim(
                        row.role.as_ref(),
                        sources,
                        "voter public role",
                        &row.name,
                    )?,
                    term: checked_person_claim(
                        row.term.as_ref(),
                        sources,
                        "voter office term",
                        &row.name,
                    )?,
                    portrait: checked_person_claim(
                        row.portrait.as_ref(),
                        sources,
                        "voter portrait",
                        &row.name,
                    )?,
                });
            }
            Ok(VoteFacts {
                scope: input.scope.clone(),
                motion,
                rows,
                item_votes_established: input.scope == "direct_item",
            })
        }
        _ => bail!("unsupported vote scope"),
    }
}

fn contains_complete_phrase(text: &str, phrase: &str) -> bool {
    text.match_indices(phrase).any(|(start, found)| {
        let before = text[..start].chars().last();
        let after = text[start + found.len()..].chars().next();
        before.is_none_or(|ch| !ch.is_alphanumeric())
            && after.is_none_or(|ch| !ch.is_alphanumeric())
    })
}

fn line_index(locator: &str) -> Option<usize> {
    locator
        .strip_prefix("line:")?
        .parse::<usize>()
        .ok()?
        .checked_sub(1)
}

fn single_item_context(
    source: &CheckedSource,
    matter: &str,
    matter_locator: &str,
    related_locators: &[&str],
) -> bool {
    let lines = source.text.lines().collect::<Vec<_>>();
    let Some(start) = line_index(matter_locator) else {
        return false;
    };
    if lines.get(start).is_none_or(|line| line.trim() != matter)
        || lines.iter().filter(|line| line.trim() == matter).count() != 1
    {
        return false;
    }
    let related = related_locators
        .iter()
        .map(|locator| line_index(locator))
        .collect::<Option<Vec<_>>>();
    let Some(related) = related else {
        return false;
    };
    if related
        .iter()
        .any(|&index| index <= start || index >= lines.len() || index - start > 25)
    {
        return false;
    }
    let end = related.iter().copied().max().unwrap_or(start);
    !lines
        .iter()
        .enumerate()
        .take(end)
        .skip(start + 1)
        .any(|(index, line)| {
            if related.contains(&index) {
                return false;
            }
            let lower = line.trim().to_ascii_lowercase();
            lower.starts_with("item ") || lower.starts_with("meeting ")
        })
}

fn atomic_named_vote(quote: &str, name: &str, position: &str) -> bool {
    let quote = quote.trim();
    for (first, second) in [(name, position), (position, name)] {
        if let Some(rest) = quote.strip_prefix(first) {
            if !rest
                .chars()
                .next()
                .is_some_and(|ch| ch.is_whitespace() || ":—–-".contains(ch))
            {
                continue;
            }
            let rest =
                rest.trim_start_matches(|ch: char| ch.is_whitespace() || ":—–-".contains(ch));
            if rest == second {
                return true;
            }
        }
    }
    false
}

fn atomic_relationship(line: &str, matter: &str, technology: &str) -> bool {
    [": ", " — ", " - ", " | "]
        .iter()
        .any(|separator| line.trim() == format!("{matter}{separator}{technology}"))
}

fn checked_person_claim(
    input: Option<&ClaimInput>,
    sources: &BTreeMap<String, CheckedSource>,
    label: &str,
    name: &str,
) -> Result<FactState> {
    let Some(input) = input else {
        return Ok(missing(&format!("{label} is not established for {name}")));
    };
    if input.value.trim().is_empty() || input.value.len() > 2_000 {
        bail!("empty or oversized {label} claim");
    }
    let proof = checked_proof(&input.proof, sources)?;
    let source = sources
        .get(&input.proof.source_ref)
        .context("person claim references an unarchived source")?;
    let line = input
        .proof
        .locator
        .strip_prefix("line:")
        .and_then(|value| value.parse::<usize>().ok())
        .and_then(|number| number.checked_sub(1))
        .and_then(|index| source.text.lines().nth(index))
        .context("person claim requires an exact extracted line")?;
    let Some(rest) = proof.quote.trim().strip_prefix(name) else {
        bail!("{label} must name the same voter");
    };
    if !rest
        .chars()
        .next()
        .is_some_and(|ch| ch.is_whitespace() || ":,—–-".contains(ch))
    {
        bail!("{label} must separate the voter name from the asserted value");
    }
    let rest = rest.trim_start_matches(|ch: char| ch.is_whitespace() || ":,—–-".contains(ch));
    if line.trim() != proof.quote.trim() || rest != input.value.trim() {
        bail!("{label} must be an atomic named line");
    }
    Ok(FactState::Established {
        value: input.value.clone(),
        proof,
    })
}

fn direct_item_row_in_context(source: &CheckedSource, motion: &str, row: &str) -> bool {
    let lines = source.text.lines().collect::<Vec<_>>();
    let Some(motion_index) = motion
        .strip_prefix("line:")
        .and_then(|text| text.parse::<usize>().ok())
        .and_then(|number| number.checked_sub(1))
    else {
        return false;
    };
    let Some(row_index) = row
        .strip_prefix("line:")
        .and_then(|text| text.parse::<usize>().ok())
        .and_then(|number| number.checked_sub(1))
    else {
        return false;
    };
    if motion_index >= lines.len()
        || row_index >= lines.len()
        || row_index <= motion_index
        || row_index - motion_index > 25
    {
        return false;
    }
    !lines[motion_index + 1..row_index].iter().any(|line| {
        let line = line.trim().to_ascii_lowercase();
        [
            "motion",
            "moved",
            "item ",
            "resolution",
            "ordinance",
            "agenda",
        ]
        .iter()
        .any(|marker| line.contains(marker))
    })
}

fn checked_proof(
    input: &ProofInput,
    sources: &BTreeMap<String, CheckedSource>,
) -> Result<FactProof> {
    if input.quote.trim().is_empty() || input.quote.len() > 2_000 {
        bail!("empty or oversized source quote");
    }
    let source = sources
        .get(&input.source_ref)
        .context("claim references an unarchived source")?;
    if let Some(value) = input.locator.strip_prefix("line:") {
        let line = value.parse::<usize>().context("invalid line locator")?;
        if line == 0
            || !source
                .text
                .lines()
                .nth(line - 1)
                .is_some_and(|text| text.contains(&input.quote))
        {
            bail!("quote is absent from the exact extracted line");
        }
    } else if let Some(value) = input.locator.strip_prefix("page:") {
        let page = value.parse::<u32>().context("invalid page locator")?;
        let map = source
            .maps
            .iter()
            .find(|map| map.page_number == page)
            .context("PDF page locator is absent")?;
        if map.source_digest != source.proof.sha256 || map.compute_digest() != map.digest {
            bail!("PDF text map does not match preserved source bytes");
        }
        let text = map
            .words
            .iter()
            .map(|word| word.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        if !text.contains(&input.quote.split_whitespace().collect::<Vec<_>>().join(" ")) {
            bail!("quote is absent from the exact extracted PDF page");
        }
    } else {
        bail!("locator must be an exact line:N or page:N");
    }
    Ok(FactProof {
        source_ref: input.source_ref.clone(),
        source_id: source.proof.source_id.clone(),
        source_url: source.proof.source_url.clone(),
        sha256: source.proof.sha256.clone(),
        evidence_id: source.proof.evidence_id.clone(),
        fetch_receipt_id: source.proof.fetch_receipt_id.clone(),
        locator: input.locator.clone(),
        quote: input.quote.clone(),
    })
}

#[allow(clippy::too_many_lines)]
fn check_candidate_archive(
    store: &Store,
    config: &StateConfig,
    input: &SourceInput,
) -> Result<(CheckedSource, String)> {
    let path = private_path(
        store.data_dir(),
        &input.archive_record,
        "discovery-archives",
    )?;
    let raw = read_regular_bounded(&path, MAX_ARCHIVE_BYTES)?;
    let record: CandidateArchive = serde_json::from_slice(&raw)?;
    if record.schema_version != 1
        || !valid_digest(&record.calendar_snapshot_sha256)
        || !valid_digest(&record.document_sha256)
        || record
            .changed_from_sha256
            .as_ref()
            .is_some_and(|digest| !valid_digest(digest))
    {
        bail!("invalid candidate archive record");
    }
    check_key(&record.calendar_source_id)?;
    check_key(&record.document_source_id)?;
    let key = sha256_hex(
        format!(
            "{}\0{}\0{}",
            record.candidate_url, record.document_sha256, record.fetch_receipt_id
        )
        .as_bytes(),
    );
    if path.file_name().and_then(|name| name.to_str()) != Some(&format!("{key}.json"))
        || path
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            != Some(record.document_source_id.as_str())
    {
        bail!("candidate archive path is not bound to its record");
    }
    let index = config
        .sources
        .iter()
        .find(|source| source.id == record.calendar_source_id)
        .context("candidate index is missing from config")?;
    let policy = config
        .sources
        .iter()
        .find(|source| source.id == record.document_source_id)
        .context("candidate document policy is missing from config")?;
    if index.source_type != "html_page"
        || record.document_source_config_digest != source_config_digest(policy)
        || policy.official_discovery_url != index.url
        || policy.documented_at != index.url
    {
        bail!("candidate archive differs from exact reviewed source configuration");
    }
    let index_review = verify_source_review(store, config, &index.id)?;
    let policy_review = verify_source_review(store, config, &policy.id)?;
    if policy_review.id != record.document_source_review_id {
        bail!("candidate archive refers to an outdated document source review");
    }
    let index_url = Url::parse(&index.url)?;
    let candidate_url = Url::parse(&record.candidate_url)?;
    let policy_url = Url::parse(&policy.url)?;
    let is_town = policy.source_type == "candidate_index_pdf"
        && record.candidate_kind == "document"
        && town_pdf_shape(&index_url, &policy_url, &candidate_url);
    let is_legistar_pdf = policy.source_type == "candidate_document_pdf"
        && matches!(
            record.candidate_kind.as_str(),
            "agenda" | "packet" | "minutes"
        )
        && index_url.path() == "/Calendar.aspx"
        && candidate_url.path() == "/View.ashx"
        && policy_url.path() == "/View.ashx"
        && candidate_url.query().is_some();
    let is_legistar_meeting = policy.source_type == "candidate_meeting_html"
        && record.candidate_kind == "meeting"
        && index_url.path() == "/Calendar.aspx"
        && candidate_url.path() == "/MeetingDetail.aspx"
        && policy_url.path() == "/MeetingDetail.aspx"
        && candidate_url.query().is_some();
    if !(is_town || is_legistar_pdf || is_legistar_meeting)
        || index_url.scheme() != "https"
        || policy_url.scheme() != "https"
        || index_url.port_or_known_default() != Some(443)
        || policy_url.port_or_known_default() != Some(443)
        || !index_url.username().is_empty()
        || !policy_url.username().is_empty()
        || index_url.password().is_some()
        || policy_url.password().is_some()
        || index_url.query().is_some()
        || policy_url.query().is_some()
        || index_url.fragment().is_some()
        || policy_url.fragment().is_some()
        || candidate_url.scheme() != "https"
        || candidate_url.host_str() != policy_url.host_str()
        || candidate_url.host_str() != index_url.host_str()
        || candidate_url.port_or_known_default() != Some(443)
        || candidate_url.username() != ""
        || candidate_url.password().is_some()
        || candidate_url.fragment().is_some()
        || candidate_url.as_str().len() > 2_048
    {
        bail!("candidate URL is outside the reviewed official document path");
    }
    let index_raw = checked_blob(store, &record.calendar_snapshot_sha256, index.maximum_bytes)?;
    let (index_receipt, index_mime) = matching_receipt(
        store,
        &index.id,
        &index.url,
        &record.calendar_snapshot_sha256,
        None,
        None,
    )?;
    let html_prefix =
        String::from_utf8_lossy(&index_raw[..index_raw.len().min(4_096)]).to_ascii_lowercase();
    if !matches!(
        index_mime.split(';').next().map(str::trim),
        Some("text/html" | "application/xhtml+xml")
    ) || (!html_prefix.contains("<html") && !html_prefix.contains("<!doctype html"))
    {
        bail!("archived index bytes or MIME are not HTML");
    }
    if !anchor_matches(
        &index_url,
        &index_raw,
        record.anchor_index,
        &record.candidate_url,
    )? {
        bail!("candidate URL differs from preserved index anchor");
    }
    let doc_raw = checked_blob(store, &record.document_sha256, policy.maximum_bytes)?;
    let (_receipt, mime) = matching_receipt(
        store,
        &policy.id,
        &record.candidate_url,
        &record.document_sha256,
        Some(&record.fetch_receipt_id),
        Some(&record.retrieved_at),
    )?;
    let (source_type, expected_mime) = if is_legistar_meeting {
        (SourceType::HtmlPage, "text/html")
    } else {
        (SourceType::Pdf, "application/pdf")
    };
    if !mime
        .split(';')
        .next()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case(expected_mime))
        || (expected_mime == "application/pdf" && !doc_raw.starts_with(b"%PDF-"))
    {
        bail!("candidate bytes or MIME differ from reviewed document type");
    }
    let sandbox = RealSandbox::new(pnull_ingest::ExtractionSandboxConfig::defaults())
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let mut budgets = Tracker::new(Budgets::defaults());
    let outcome = ingest_bytes(
        store,
        &sandbox,
        &mut budgets,
        &BuildMetadata::local(),
        &IngestRequest {
            jurisdiction: config.jurisdiction.clone(),
            source_url: record.candidate_url.clone(),
            source_type,
            document_title: format!("Official {} candidate", record.candidate_kind),
            publication_date: None,
            retrieval_timestamp: record.retrieved_at.clone(),
            mime_type: mime,
            original_filename: candidate_url
                .path_segments()
                .and_then(Iterator::last)
                .unwrap_or("document")
                .to_owned(),
            supersedes: None,
            enable_ocr: false,
            ocr_language: "eng".to_owned(),
            max_bytes: policy.maximum_bytes,
        },
        &doc_raw,
    )?;
    if outcome.record.extraction_status != ExtractionStatus::Complete
        && outcome.record.extraction_status != ExtractionStatus::CompleteWithOcr
    {
        bail!("candidate extraction failed; no case claims can be checked");
    }
    store.verify(&outcome.record.id)?;
    let proof = SourceProof {
        source_ref: input.source_ref.clone(),
        source_id: policy.id.clone(),
        source_url: record.candidate_url,
        sha256: record.document_sha256,
        index_source_id: index.id.clone(),
        index_sha256: record.calendar_snapshot_sha256,
        index_fetch_receipt_id: index_receipt.id,
        index_review_id: index_review.id,
        anchor_index: record.anchor_index,
        evidence_id: outcome.record.id,
        source_review_id: policy_review.id,
        fetch_receipt_id: record.fetch_receipt_id,
        retrieved_at: record.retrieved_at,
        extraction_method: outcome.record.extraction_method,
    };
    Ok((
        CheckedSource {
            proof,
            text: outcome.extracted_text,
            maps: outcome.text_maps,
        },
        index.id.clone(),
    ))
}

fn town_pdf_shape(index_url: &Url, policy_url: &Url, candidate_url: &Url) -> bool {
    matches!(index_url.path(), "/documents" | "/agendas-and-minutes/")
        && matches!(policy_url.path(), "/uploads/" | "/wp-content/uploads/")
        && candidate_url.path().starts_with(policy_url.path())
        && candidate_url.path().len() > policy_url.path().len()
        && candidate_url.path().to_ascii_lowercase().ends_with(".pdf")
        && candidate_url.query().is_none()
        && !candidate_url.path().contains(['%', '\\'])
        && !candidate_url.path().contains("//")
}

fn anchor_matches(
    index_url: &Url,
    raw: &[u8],
    anchor_index: usize,
    candidate_url: &str,
) -> Result<bool> {
    let html = std::str::from_utf8(raw).context("archived index is not UTF-8")?;
    let document = Html::parse_document(html);
    let selector = Selector::parse("a[href]").expect("fixed selector");
    let anchor = document
        .select(&selector)
        .nth(anchor_index)
        .context("candidate anchor is absent from preserved index")?;
    let href = anchor
        .value()
        .attr("href")
        .context("candidate anchor has no URL")?;
    Ok(index_url.join(href)?.as_str() == candidate_url)
}

fn matching_receipt(
    store: &Store,
    source_id: &str,
    url: &str,
    digest: &str,
    exact_id: Option<&str>,
    exact_time: Option<&str>,
) -> Result<(FetchObservation, String)> {
    let host = Url::parse(url)?
        .host_str()
        .context("receipt URL lacks host")?
        .to_owned();
    let observations = store.fetch_observations(source_id)?;
    let valid = |item: &FetchObservation| {
        item.source_id.as_deref() == Some(source_id)
            && item.id
                == FetchObservation::id_for(
                    &item.requested_url,
                    &item.retrieved_at,
                    item.status_code,
                )
            && item.requested_url == url
            && item.final_url == url
            && item.method == "GET"
            && matches!(item.status_code, 200 | 304)
            && item.error.is_none()
            && item.redirect_target.is_none()
            && OffsetDateTime::parse(&item.retrieved_at, &Rfc3339).is_ok()
            && item
                .resolved_ips
                .iter()
                .map(|ip| ip.parse::<IpAddr>())
                .collect::<Result<Vec<_>, _>>()
                .is_ok_and(|ips| validate_resolved(&host, &ips).is_ok())
    };
    let selected = observations
        .iter()
        .rev()
        .find(|item| {
            valid(item)
                && exact_id.is_none_or(|id| id == item.id)
                && exact_time.is_none_or(|time| time == item.retrieved_at)
                && (item.status_code == 304 || item.body_digest.as_deref() == Some(digest))
        })
        .context("archived source lacks an exact public-network fetch receipt")?;
    let mime_receipt = if selected.status_code == 304 {
        observations
            .iter()
            .rev()
            .find(|item| {
                valid(item) && item.status_code == 200 && item.retrieved_at < selected.retrieved_at
            })
            .filter(|item| item.body_digest.as_deref() == Some(digest))
            .context("304 receipt lacks a prior latest exact 200-byte capture")?
    } else {
        selected
    };
    let mime = mime_receipt
        .content_type
        .as_ref()
        .context("source receipt lacks MIME type")?;
    Ok((selected.clone(), mime.clone()))
}

fn checked_blob(store: &Store, digest: &str, maximum_bytes: usize) -> Result<Vec<u8>> {
    if !valid_digest(digest) || maximum_bytes == 0 {
        bail!("invalid source digest or byte bound");
    }
    let content_path = store.content_path(digest)?;
    let relative = content_path.strip_prefix(store.data_dir())?;
    let path = private_path(store.data_dir(), relative, "evidence")?;
    let raw = read_regular_bounded(&path, maximum_bytes as u64)?;
    if raw.is_empty() || sha256_hex(&raw) != digest {
        bail!("private source bytes differ from their digest");
    }
    Ok(raw)
}

fn geography_ref_for_index(
    repository_root: &Path,
    config_path: &Path,
    index_source_id: &str,
) -> Result<String> {
    let mut refs = Vec::new();
    for path in [
        "configs/states/co-calendar-geography.v1.json",
        "configs/states/co-document-index-geography.v1.json",
    ] {
        let raw = read_regular_bounded(&repository_root.join(path), 100_000)?;
        let parsed: serde_json::Value = serde_json::from_slice(&raw)?;
        let rows = parsed
            .get("calendar_geography")
            .or_else(|| parsed.get("document_index_geography"))
            .and_then(serde_json::Value::as_array)
            .context("invalid geography source mapping")?;
        for row in rows {
            if row.get("source_id").and_then(serde_json::Value::as_str) == Some(index_source_id) {
                let mapped_config = row
                    .get("config_path")
                    .and_then(serde_json::Value::as_str)
                    .context("mapped source lacks config_path")?;
                if repository_root.join(mapped_config) != config_path {
                    bail!("mapped source belongs to a different Colorado config");
                }
                refs.push(
                    row.get("jurisdiction_ref")
                        .and_then(serde_json::Value::as_str)
                        .context("mapped source lacks jurisdiction_ref")?
                        .to_owned(),
                );
            }
        }
    }
    if refs.len() != 1 {
        bail!("candidate index must map to exactly one Colorado geography entry");
    }
    Ok(refs.remove(0))
}

fn read_regular_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.file_type().is_file()
        || meta.file_type().is_symlink()
        || meta.len() == 0
        || meta.len() > limit
    {
        bail!(
            "source artifact is not a bounded regular file: {}",
            path.display()
        );
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        bail!("source artifact exceeds its bound");
    }
    Ok(bytes)
}

fn private_path(data_dir: &Path, relative: &Path, expected_root: &str) -> Result<PathBuf> {
    let root = fs::symlink_metadata(data_dir)?;
    if !root.file_type().is_dir() || root.file_type().is_symlink() {
        bail!("private store root is unsafe");
    }
    let mut components = relative.components();
    if components.next() != Some(Component::Normal(expected_root.as_ref()))
        || !components.all(|component| matches!(component, Component::Normal(_)))
    {
        bail!("private artifact path must be beneath {expected_root}");
    }
    let mut current = data_dir.to_path_buf();
    for component in relative.components() {
        current.push(component);
        let metadata = fs::symlink_metadata(&current)?;
        if metadata.file_type().is_symlink() {
            bail!("private artifact path contains a symlink");
        }
    }
    Ok(current)
}

fn check_previous(
    data_dir: &Path,
    id: &str,
    jurisdiction_ref: &str,
) -> Result<(String, Vec<CorrectionEntry>)> {
    let digest = id
        .strip_prefix("case-proposal:")
        .context("invalid previous proposal ID")?;
    if !valid_digest(digest) {
        bail!("invalid previous proposal digest");
    }
    let relative = PathBuf::from("case-proposals").join(format!("{digest}.json"));
    let path = private_path(data_dir, &relative, "case-proposals")?;
    let raw = read_regular_bounded(&path, MAX_PROPOSAL_BYTES)?;
    let previous: serde_json::Value = serde_json::from_slice(&raw)?;
    if previous.get("id").and_then(serde_json::Value::as_str) != Some(id)
        || previous
            .get("jurisdiction_ref")
            .and_then(serde_json::Value::as_str)
            != Some(jurisdiction_ref)
        || previous
            .get("publication_state")
            .and_then(serde_json::Value::as_str)
            != Some("private_pending_publication_checks")
    {
        bail!("previous proposal artifact does not match its ID");
    }
    let history: Vec<CorrectionEntry> = serde_json::from_value(
        previous
            .get("correction_history")
            .cloned()
            .context("previous proposal lacks correction history")?,
    )?;
    Ok((sha256_hex(&raw), history))
}

fn persist(data_dir: &Path, proposal: &Proposal) -> Result<PathBuf> {
    let metadata = fs::symlink_metadata(data_dir)?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        bail!("private store root is unsafe");
    }
    let dir = data_dir.join("case-proposals");
    if let Err(error) = fs::create_dir(&dir)
        && error.kind() != std::io::ErrorKind::AlreadyExists
    {
        return Err(error.into());
    }
    let metadata = fs::symlink_metadata(&dir)?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        bail!("proposal directory is unsafe");
    }
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    let digest = proposal
        .id
        .strip_prefix("case-proposal:")
        .context("invalid proposal ID")?;
    let path = dir.join(format!("{digest}.json"));
    let bytes = serde_json::to_vec_pretty(proposal)?;
    if bytes.len() as u64 > MAX_PROPOSAL_BYTES {
        bail!("private proposal exceeds its byte bound");
    }
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
    {
        Ok(mut file) => {
            file.write_all(&bytes)?;
            file.sync_all()?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if read_regular_bounded(&path, MAX_PROPOSAL_BYTES)? != bytes {
                bail!("existing proposal differs from source-checked content");
            }
        }
        Err(error) => return Err(error.into()),
    }
    Ok(path)
}

fn check_key(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 100
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        bail!("invalid source reference or ID");
    }
    Ok(())
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests;
