//! Evidence-bound public decisions, participation, contracts, and technology.
//!
//! These records describe official conduct in a public role. A vote on a
//! funding measure does not become authorization of a contract without a
//! separately reviewed, exact-source link.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{CoreError, Store, sha256_hex, stable_id, types::ReviewState};

/// Exact source metadata attached to each public fact and relationship.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceProof {
    pub source_url: String,
    pub snapshot_digest: String,
    pub locator: String,
    pub quote: String,
}

impl SourceProof {
    pub fn digest(&self) -> String {
        stable_id(
            "source-proof",
            &[
                &self.source_url,
                &self.snapshot_digest,
                &self.locator,
                &self.quote,
            ],
        )
    }
}

/// A person in a documented public office for a bounded term.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OfficeTerm {
    pub id: String,
    pub person_name: String,
    pub official_person_id: Option<String>,
    pub office_title: String,
    pub body: String,
    pub jurisdiction: String,
    pub starts_on: Option<String>,
    pub ends_on: Option<String>,
    /// Officially published year range when exact term dates are unavailable.
    pub term_span_raw: Option<String>,
    /// Independent locator for the published term span, which may differ from
    /// the page location that establishes the person's public role.
    #[serde(default)]
    pub term_proof: Option<SourceProof>,
    pub proof: SourceProof,
}

impl OfficeTerm {
    pub fn id_for(
        jurisdiction: &str,
        body: &str,
        official_person_id: &str,
        starts_on: &str,
    ) -> String {
        stable_id(
            "office-term",
            &[jurisdiction, body, official_person_id, starts_on],
        )
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    LegislativeVote,
    ContractAward,
    ContractSignature,
    Other,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionOutcome {
    Approved,
    Rejected,
    Deferred,
    Unknown,
}

/// A recorded decision on one exact official matter or agenda item.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CivicDecision {
    pub id: String,
    pub jurisdiction: String,
    pub body: String,
    pub official_matter_id: String,
    pub official_event_item_id: String,
    pub decided_on: String,
    pub subject: String,
    pub kind: DecisionKind,
    pub outcome: DecisionOutcome,
    pub proof: SourceProof,
}

impl CivicDecision {
    pub fn id_for(
        jurisdiction: &str,
        official_matter_id: &str,
        official_event_item_id: &str,
    ) -> String {
        stable_id(
            "civic-decision",
            &[jurisdiction, official_matter_id, official_event_item_id],
        )
    }
}

/// The recorded action by one officeholder; moving and seconding are distinct
/// from voting yes, and none implies signing an executed contract.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipationKind {
    VoteYes,
    VoteNo,
    Abstained,
    Absent,
    Moved,
    Seconded,
    Signed,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DecisionParticipation {
    pub id: String,
    pub decision_id: String,
    pub office_term_id: String,
    pub kind: ParticipationKind,
    pub official_vote_id: Option<String>,
    pub raw_vote_value: Option<String>,
    pub proof: SourceProof,
}

impl DecisionParticipation {
    pub fn id_for(decision_id: &str, office_term_id: &str, kind: ParticipationKind) -> String {
        stable_id(
            "decision-participation",
            &[decision_id, office_term_id, kind.label()],
        )
    }
}

impl ParticipationKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::VoteYes => "vote_yes",
            Self::VoteNo => "vote_no",
            Self::Abstained => "abstained",
            Self::Absent => "absent",
            Self::Moved => "moved",
            Self::Seconded => "seconded",
            Self::Signed => "signed",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractStatus {
    Proposed,
    Authorized,
    Awarded,
    Executed,
    Unknown,
}

/// A contract or award as described in an official record. "Authorized" is
/// deliberately distinct from "Executed".
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContractRecord {
    pub id: String,
    pub procurement_matter_id: Option<String>,
    pub official_identifier: String,
    pub title: String,
    pub vendor: Option<String>,
    pub amount_raw: Option<String>,
    pub term_raw: Option<String>,
    pub status: ContractStatus,
    pub proof: SourceProof,
}

impl ContractRecord {
    pub fn id_for(jurisdiction: &str, official_identifier: &str) -> String {
        stable_id("contract-record", &[jurisdiction, official_identifier])
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TechnologyRecord {
    pub id: String,
    pub name: String,
    pub category: String,
    pub vendor: Option<String>,
    pub proof: SourceProof,
}

impl TechnologyRecord {
    pub fn id_for(name: &str, category: &str) -> String {
        stable_id("technology-record", &[name, category])
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalLinkKind {
    DecisionAuthorizesContract,
    ContractIncludesTechnology,
    ParticipationRecordedForDecision,
    ParticipationByOfficeholder,
}

impl ApprovalLinkKind {
    const fn label(self) -> &'static str {
        match self {
            Self::DecisionAuthorizesContract => "decision_authorizes_contract",
            Self::ContractIncludesTechnology => "contract_includes_technology",
            Self::ParticipationRecordedForDecision => "participation_recorded_for_decision",
            Self::ParticipationByOfficeholder => "participation_by_officeholder",
        }
    }
}

/// One immutable claim connecting exact public records. Review decisions and
/// corrections are separate append-only records.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ApprovalLink {
    pub id: String,
    pub kind: ApprovalLinkKind,
    pub from_id: String,
    pub to_id: String,
    pub proof: SourceProof,
    /// Additional exact sources needed to establish a multi-record relationship.
    #[serde(default)]
    pub supporting_proofs: Vec<SourceProof>,
    pub note: String,
}

impl ApprovalLink {
    pub fn id_for(
        kind: ApprovalLinkKind,
        from_id: &str,
        to_id: &str,
        proof: &SourceProof,
        supporting_proofs: &[SourceProof],
    ) -> String {
        let primary_digest = proof.digest();
        let mut supporting_digests: Vec<String> =
            supporting_proofs.iter().map(SourceProof::digest).collect();
        supporting_digests.sort();
        let mut parts = vec![kind.label(), from_id, to_id, primary_digest.as_str()];
        parts.extend(supporting_digests.iter().map(String::as_str));
        stable_id("approval-link", &parts)
    }

    pub fn binding_digest(&self) -> String {
        let primary_digest = self.proof.digest();
        let mut supporting_digests: Vec<String> = self
            .supporting_proofs
            .iter()
            .map(SourceProof::digest)
            .collect();
        supporting_digests.sort();
        let mut parts = vec![
            self.id.as_str(),
            self.kind.label(),
            self.from_id.as_str(),
            self.to_id.as_str(),
            primary_digest.as_str(),
            self.note.as_str(),
        ];
        parts.extend(supporting_digests.iter().map(String::as_str));
        stable_id("approval-binding", &parts)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ApprovalReview {
    pub id: String,
    pub link_id: String,
    pub state: ReviewState,
    pub bound_digest: String,
    pub reviewer: String,
    pub note: String,
    pub decided_at: String,
    pub supersedes: Option<String>,
}

impl ApprovalReview {
    pub fn id_for(link_id: &str, decided_at: &str) -> String {
        stable_id("approval-review", &[link_id, decided_at])
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalCorrectionKind {
    Retracted,
    Replaced,
    Clarified,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ApprovalCorrection {
    pub id: String,
    pub link_id: String,
    pub kind: ApprovalCorrectionKind,
    pub replacement_link_id: Option<String>,
    pub operator: String,
    pub reason: String,
    pub recorded_at: String,
    pub proof: SourceProof,
}

impl ApprovalCorrection {
    pub fn id_for(link_id: &str, kind: ApprovalCorrectionKind, recorded_at: &str) -> String {
        stable_id("approval-correction", &[link_id, kind.label(), recorded_at])
    }
}

impl ApprovalCorrectionKind {
    const fn label(self) -> &'static str {
        match self {
            Self::Retracted => "retracted",
            Self::Replaced => "replaced",
            Self::Clarified => "clarified",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalGapKind {
    IndividualVoteNotEstablished,
    ContractAuthorizationNotEstablished,
    ContractExecutionNotEstablished,
    PaymentNotEstablished,
    DeploymentNotEstablished,
    TechnologyUseNotEstablished,
    OfficeTermDatesNotEstablished,
}

/// An explicit unknown. It cannot be mistaken for an established link.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ApprovalGap {
    pub id: String,
    pub subject_id: String,
    pub kind: ApprovalGapKind,
    pub checked_source_digests: Vec<String>,
    pub detail: String,
    pub recorded_at: String,
}

impl ApprovalGap {
    pub fn id_for(subject_id: &str, kind: ApprovalGapKind, recorded_at: &str) -> String {
        stable_id("approval-gap", &[subject_id, kind.label(), recorded_at])
    }
}

impl ApprovalGapKind {
    const fn label(self) -> &'static str {
        match self {
            Self::IndividualVoteNotEstablished => "individual_vote_not_established",
            Self::ContractAuthorizationNotEstablished => "contract_authorization_not_established",
            Self::ContractExecutionNotEstablished => "contract_execution_not_established",
            Self::PaymentNotEstablished => "payment_not_established",
            Self::DeploymentNotEstablished => "deployment_not_established",
            Self::TechnologyUseNotEstablished => "technology_use_not_established",
            Self::OfficeTermDatesNotEstablished => "office_term_dates_not_established",
        }
    }
}

/// A view of currently approved, unretracted connections. The publisher must
/// also review the public fields of the individual records it renders.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ApprovalChain {
    pub contract: ContractRecord,
    pub authorizing_decisions: Vec<CivicDecision>,
    pub participations: Vec<DecisionParticipation>,
    pub office_terms: Vec<OfficeTerm>,
    pub technologies: Vec<TechnologyRecord>,
    pub approved_links: Vec<ApprovalLink>,
    pub gaps: Vec<ApprovalGap>,
}

fn validate_text(value: &str, field: &str) -> Result<(), CoreError> {
    if value.trim().is_empty() {
        Err(CoreError::InvalidApproval(format!("{field} is empty")))
    } else {
        Ok(())
    }
}

/// Resolves the small, unambiguous `JSONPath` subset used by source proofs:
/// object fields and zero-based array indexes, such as `$.MatterName` or
/// `$[2].VoteValueName`. An unsupported path fails closed.
fn json_value_at_locator<'a>(root: &'a Value, locator: &str) -> Option<&'a Value> {
    let mut remaining = locator.strip_prefix('$')?;
    let mut value = root;
    if remaining.is_empty() {
        return None;
    }
    while !remaining.is_empty() {
        if let Some(rest) = remaining.strip_prefix('.') {
            let end = rest.find(['.', '[']).unwrap_or(rest.len());
            let field = rest.get(..end)?;
            if field.is_empty() {
                return None;
            }
            value = value.get(field)?;
            remaining = &rest[end..];
        } else if let Some(rest) = remaining.strip_prefix('[') {
            let (index, after) = rest.split_once(']')?;
            if index.is_empty() || !index.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            value = value.get(index.parse::<usize>().ok()?)?;
            remaining = after;
        } else {
            return None;
        }
    }
    Some(value)
}

fn scalar_contains_quote(value: &Value, quote: &str) -> bool {
    match value {
        Value::String(text) => text.contains(quote),
        Value::Number(_) | Value::Bool(_) => value.to_string().contains(quote),
        Value::Null | Value::Array(_) | Value::Object(_) => false,
    }
}

fn json_contains_quote(value: &Value, quote: &str) -> bool {
    match value {
        Value::Array(items) => items.iter().any(|item| json_contains_quote(item, quote)),
        Value::Object(fields) => fields.values().any(|item| json_contains_quote(item, quote)),
        _ => scalar_contains_quote(value, quote),
    }
}

impl Store {
    fn validate_office_term_proofs(&self, term: &OfficeTerm) -> Result<(), CoreError> {
        self.validate_source_proof(&term.proof)?;
        match (&term.term_span_raw, &term.term_proof) {
            (None, None) => Ok(()),
            (Some(span), Some(proof)) if span == &proof.quote => self.validate_source_proof(proof),
            _ => Err(CoreError::InvalidApproval(
                "published term span must have a proof quoting that exact span".to_owned(),
            )),
        }
    }

    fn preserved_content(&self, digest: &str) -> Result<Vec<u8>, CoreError> {
        let path = self.content_path(digest)?;
        let bytes = std::fs::read(path)?;
        let observed = sha256_hex(&bytes);
        if observed != digest {
            return Err(CoreError::DigestMismatch {
                evidence_id: digest.to_owned(),
                expected: digest.to_owned(),
                observed,
            });
        }
        Ok(bytes)
    }

    fn validate_source_proof(&self, proof: &SourceProof) -> Result<(), CoreError> {
        if !proof.source_url.starts_with("https://") {
            return Err(CoreError::InvalidApproval(
                "proof URL must use HTTPS".to_owned(),
            ));
        }
        validate_text(&proof.locator, "proof locator")?;
        validate_text(&proof.quote, "proof quote")?;
        let bytes = self.preserved_content(&proof.snapshot_digest)?;
        let found = if proof.locator.starts_with('$') {
            let source: Value = serde_json::from_slice(&bytes).map_err(|_| {
                CoreError::InvalidApproval("JSONPath proof source is not valid JSON".to_owned())
            })?;
            json_value_at_locator(&source, &proof.locator)
                .is_some_and(|value| scalar_contains_quote(value, &proof.quote))
        } else if let Ok(source) = serde_json::from_slice::<Value>(&bytes) {
            json_contains_quote(&source, &proof.quote)
        } else {
            std::str::from_utf8(&bytes).is_ok_and(|source| source.contains(&proof.quote))
        };
        if !found {
            return Err(CoreError::InvalidApproval(format!(
                "proof quote not found at locator {} in preserved source",
                proof.locator
            )));
        }
        Ok(())
    }

    fn insert_approval_json<T: Serialize>(
        &self,
        table: &str,
        json_column: &str,
        id: &str,
        extra_column: &str,
        extra_value: &str,
        item: &T,
    ) -> Result<bool, CoreError> {
        let serialized = serde_json::to_string(item)?;
        let existing: Option<String> = self
            .connection
            .query_row(
                &format!("SELECT {json_column} FROM {table} WHERE id = ?1"),
                [id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(existing) = existing {
            if existing == serialized {
                return Ok(false);
            }
            return Err(CoreError::ApprovalConflict(id.to_owned()));
        }
        Ok(self.connection.execute(
            &format!("INSERT INTO {table}(id, {extra_column}, {json_column}) VALUES (?1, ?2, ?3)"),
            params![id, extra_value, serialized],
        )? == 1)
    }

    pub fn insert_office_term(&self, term: &OfficeTerm) -> Result<bool, CoreError> {
        validate_text(&term.person_name, "person name")?;
        validate_text(&term.office_title, "office title")?;
        validate_text(&term.body, "body")?;
        validate_text(&term.jurisdiction, "jurisdiction")?;
        self.validate_office_term_proofs(term)?;
        self.insert_approval_json(
            "office_terms",
            "term_json",
            &term.id,
            "jurisdiction",
            &term.jurisdiction,
            term,
        )
    }

    pub fn office_term(&self, id: &str) -> Result<OfficeTerm, CoreError> {
        let term: OfficeTerm =
            self.read_json_row("SELECT term_json FROM office_terms WHERE id = ?1", &[id])?;
        self.validate_office_term_proofs(&term)?;
        Ok(term)
    }

    pub fn insert_civic_decision(&self, decision: &CivicDecision) -> Result<bool, CoreError> {
        validate_text(&decision.official_matter_id, "official matter id")?;
        validate_text(&decision.official_event_item_id, "official event item id")?;
        validate_text(&decision.subject, "decision subject")?;
        self.validate_source_proof(&decision.proof)?;
        self.insert_approval_json(
            "civic_decisions",
            "decision_json",
            &decision.id,
            "jurisdiction",
            &decision.jurisdiction,
            decision,
        )
    }

    pub fn civic_decision(&self, id: &str) -> Result<CivicDecision, CoreError> {
        self.read_json_row(
            "SELECT decision_json FROM civic_decisions WHERE id = ?1",
            &[id],
        )
    }

    pub fn insert_decision_participation(
        &self,
        participation: &DecisionParticipation,
    ) -> Result<bool, CoreError> {
        self.civic_decision(&participation.decision_id)?;
        self.office_term(&participation.office_term_id)?;
        self.validate_source_proof(&participation.proof)?;
        self.insert_approval_json(
            "decision_participations",
            "participation_json",
            &participation.id,
            "decision_id",
            &participation.decision_id,
            participation,
        )
    }

    pub fn decision_participations(
        &self,
        decision_id: &str,
    ) -> Result<Vec<DecisionParticipation>, CoreError> {
        self.read_json_rows("SELECT participation_json FROM decision_participations WHERE decision_id = ?1 ORDER BY id", &[decision_id])
    }

    pub fn insert_contract_record(&self, contract: &ContractRecord) -> Result<bool, CoreError> {
        validate_text(&contract.official_identifier, "contract identifier")?;
        validate_text(&contract.title, "contract title")?;
        self.validate_source_proof(&contract.proof)?;
        self.insert_approval_json(
            "contract_records",
            "contract_json",
            &contract.id,
            "official_identifier",
            &contract.official_identifier,
            contract,
        )
    }

    pub fn contract_record(&self, id: &str) -> Result<ContractRecord, CoreError> {
        self.read_json_row(
            "SELECT contract_json FROM contract_records WHERE id = ?1",
            &[id],
        )
    }

    pub fn insert_technology_record(
        &self,
        technology: &TechnologyRecord,
    ) -> Result<bool, CoreError> {
        validate_text(&technology.name, "technology name")?;
        validate_text(&technology.category, "technology category")?;
        self.validate_source_proof(&technology.proof)?;
        self.insert_approval_json(
            "technology_records",
            "technology_json",
            &technology.id,
            "name",
            &technology.name,
            technology,
        )
    }

    pub fn technology_record(&self, id: &str) -> Result<TechnologyRecord, CoreError> {
        self.read_json_row(
            "SELECT technology_json FROM technology_records WHERE id = ?1",
            &[id],
        )
    }

    pub fn insert_approval_link(&self, link: &ApprovalLink) -> Result<bool, CoreError> {
        if link.id
            != ApprovalLink::id_for(
                link.kind,
                &link.from_id,
                &link.to_id,
                &link.proof,
                &link.supporting_proofs,
            )
        {
            return Err(CoreError::InvalidApproval(
                "link id does not match its contents".to_owned(),
            ));
        }
        match link.kind {
            ApprovalLinkKind::DecisionAuthorizesContract => {
                self.civic_decision(&link.from_id)?;
                self.contract_record(&link.to_id)?;
            }
            ApprovalLinkKind::ContractIncludesTechnology => {
                self.contract_record(&link.from_id)?;
                self.technology_record(&link.to_id)?;
            }
            ApprovalLinkKind::ParticipationRecordedForDecision => {
                let participation: DecisionParticipation = self.read_json_row(
                    "SELECT participation_json FROM decision_participations WHERE id = ?1",
                    &[&link.from_id],
                )?;
                if participation.decision_id != link.to_id {
                    return Err(CoreError::InvalidApproval(
                        "participation link points to a different decision".to_owned(),
                    ));
                }
                self.civic_decision(&link.to_id)?;
            }
            ApprovalLinkKind::ParticipationByOfficeholder => {
                let participation: DecisionParticipation = self.read_json_row(
                    "SELECT participation_json FROM decision_participations WHERE id = ?1",
                    &[&link.from_id],
                )?;
                if participation.office_term_id != link.to_id {
                    return Err(CoreError::InvalidApproval(
                        "participation link points to a different office term".to_owned(),
                    ));
                }
                self.office_term(&link.to_id)?;
            }
        }
        self.validate_source_proof(&link.proof)?;
        for proof in &link.supporting_proofs {
            self.validate_source_proof(proof)?;
        }
        self.insert_approval_json(
            "approval_links",
            "link_json",
            &link.id,
            "from_id",
            &link.from_id,
            link,
        )
    }

    pub fn approval_link(&self, id: &str) -> Result<ApprovalLink, CoreError> {
        self.read_json_row("SELECT link_json FROM approval_links WHERE id = ?1", &[id])
    }

    /// Lists candidate links for operator review, including pending or
    /// subsequently corrected claims. Callers must inspect review state.
    pub fn approval_links_for_contract(
        &self,
        contract_id: &str,
    ) -> Result<Vec<ApprovalLink>, CoreError> {
        self.contract_record(contract_id)?;
        self.read_json_rows(
            "SELECT link_json FROM approval_links WHERE from_id = ?1 OR json_extract(link_json, '$.to_id') = ?1 ORDER BY id",
            &[contract_id],
        )
    }

    pub fn approval_links_for_decision(
        &self,
        decision_id: &str,
    ) -> Result<Vec<ApprovalLink>, CoreError> {
        self.civic_decision(decision_id)?;
        self.read_json_rows(
            "SELECT link_json FROM approval_links WHERE from_id = ?1 OR json_extract(link_json, '$.to_id') = ?1 ORDER BY id",
            &[decision_id],
        )
    }

    pub fn approval_links_for_participation(
        &self,
        participation_id: &str,
    ) -> Result<Vec<ApprovalLink>, CoreError> {
        let _: DecisionParticipation = self.read_json_row(
            "SELECT participation_json FROM decision_participations WHERE id = ?1",
            &[participation_id],
        )?;
        self.read_json_rows(
            "SELECT link_json FROM approval_links WHERE from_id = ?1 ORDER BY id",
            &[participation_id],
        )
    }

    /// Returns the current human decision on a link; corrected links are
    /// superseded even if they were previously approved.
    pub fn approval_link_review_state(
        &self,
        link_id: &str,
    ) -> Result<Option<ReviewState>, CoreError> {
        let link = self.approval_link(link_id)?;
        self.validate_source_proof(&link.proof)?;
        for proof in &link.supporting_proofs {
            self.validate_source_proof(proof)?;
        }
        let corrections = self.approval_corrections(link_id)?;
        if corrections.iter().any(|correction| {
            matches!(
                correction.kind,
                ApprovalCorrectionKind::Retracted | ApprovalCorrectionKind::Replaced
            )
        }) {
            return Ok(Some(ReviewState::Superseded));
        }
        Ok(self.approval_reviews(link_id)?.last().map(|review| {
            if review.bound_digest == link.binding_digest() {
                review.state
            } else {
                ReviewState::NeedsContext
            }
        }))
    }

    pub fn insert_approval_review(&self, review: &ApprovalReview) -> Result<bool, CoreError> {
        let link = self.approval_link(&review.link_id)?;
        if review.bound_digest != link.binding_digest() {
            return Err(CoreError::InvalidApproval(
                "review binding does not match link".to_owned(),
            ));
        }
        let existing = self.approval_reviews(&review.link_id)?;
        if !existing.iter().any(|item| item.id == review.id) {
            match existing.last() {
                None if review.supersedes.is_some() => {
                    return Err(CoreError::InvalidApproval(
                        "first review cannot supersede another decision".to_owned(),
                    ));
                }
                Some(previous) if review.supersedes.as_deref() != Some(previous.id.as_str()) => {
                    return Err(CoreError::InvalidApproval(
                        "new review must supersede the latest decision".to_owned(),
                    ));
                }
                _ => {}
            }
            if let Some(previous) = existing.last()
                && review.decided_at <= previous.decided_at
            {
                return Err(CoreError::InvalidApproval(
                    "new review must have a later timestamp".to_owned(),
                ));
            }
        }
        self.insert_approval_json(
            "approval_reviews",
            "review_json",
            &review.id,
            "link_id",
            &review.link_id,
            review,
        )
    }

    pub fn approval_reviews(&self, link_id: &str) -> Result<Vec<ApprovalReview>, CoreError> {
        self.read_json_rows(
            "SELECT review_json FROM approval_reviews WHERE link_id = ?1 ORDER BY json_extract(review_json, '$.decided_at'), id",
            &[link_id],
        )
    }

    pub fn insert_approval_correction(
        &self,
        correction: &ApprovalCorrection,
    ) -> Result<bool, CoreError> {
        self.approval_link(&correction.link_id)?;
        if correction.kind == ApprovalCorrectionKind::Replaced {
            let replacement = correction.replacement_link_id.as_ref().ok_or_else(|| {
                CoreError::InvalidApproval("replacement link id required".to_owned())
            })?;
            self.approval_link(replacement)?;
        } else if correction.replacement_link_id.is_some() {
            return Err(CoreError::InvalidApproval(
                "replacement link only applies to a replacement correction".to_owned(),
            ));
        }
        validate_text(&correction.operator, "correction operator")?;
        validate_text(&correction.reason, "correction reason")?;
        self.validate_source_proof(&correction.proof)?;
        self.insert_approval_json(
            "approval_corrections",
            "correction_json",
            &correction.id,
            "link_id",
            &correction.link_id,
            correction,
        )
    }

    pub fn approval_corrections(
        &self,
        link_id: &str,
    ) -> Result<Vec<ApprovalCorrection>, CoreError> {
        let corrections: Vec<ApprovalCorrection> = self.read_json_rows("SELECT correction_json FROM approval_corrections WHERE link_id = ?1 ORDER BY json_extract(correction_json, '$.recorded_at'), id", &[link_id])?;
        for correction in &corrections {
            self.validate_source_proof(&correction.proof)?;
        }
        Ok(corrections)
    }

    pub fn insert_approval_gap(&self, gap: &ApprovalGap) -> Result<bool, CoreError> {
        validate_text(&gap.subject_id, "gap subject")?;
        validate_text(&gap.detail, "gap detail")?;
        for digest in &gap.checked_source_digests {
            self.preserved_content(digest)?;
        }
        self.insert_approval_json(
            "approval_gaps",
            "gap_json",
            &gap.id,
            "subject_id",
            &gap.subject_id,
            gap,
        )
    }

    pub fn approval_gaps(&self, subject_id: &str) -> Result<Vec<ApprovalGap>, CoreError> {
        let gaps: Vec<ApprovalGap> = self.read_json_rows(
            "SELECT gap_json FROM approval_gaps WHERE subject_id = ?1 ORDER BY id",
            &[subject_id],
        )?;
        for gap in &gaps {
            for digest in &gap.checked_source_digests {
                self.preserved_content(digest)?;
            }
        }
        Ok(gaps)
    }

    fn approval_link_is_current(&self, link: &ApprovalLink) -> Result<bool, CoreError> {
        Ok(self.approval_link_review_state(&link.id)? == Some(ReviewState::Approved))
    }

    fn reviewed_link_to(
        &self,
        candidates: &[ApprovalLink],
        kind: ApprovalLinkKind,
        to_id: &str,
    ) -> Result<Option<ApprovalLink>, CoreError> {
        for candidate in candidates {
            if candidate.kind == kind
                && candidate.to_id == to_id
                && self.approval_link_is_current(candidate)?
            {
                return Ok(Some(candidate.clone()));
            }
        }
        Ok(None)
    }

    pub fn approval_chain(&self, contract_id: &str) -> Result<ApprovalChain, CoreError> {
        let contract = self.contract_record(contract_id)?;
        self.validate_source_proof(&contract.proof)?;
        let links = self.approval_links_for_contract(contract_id)?;
        let mut chain = ApprovalChain {
            contract,
            authorizing_decisions: Vec::new(),
            participations: Vec::new(),
            office_terms: Vec::new(),
            technologies: Vec::new(),
            approved_links: Vec::new(),
            gaps: self.approval_gaps(contract_id)?,
        };
        for link in links {
            if !self.approval_link_is_current(&link)? {
                continue;
            }
            match link.kind {
                ApprovalLinkKind::DecisionAuthorizesContract if link.to_id == contract_id => {
                    let decision = self.civic_decision(&link.from_id)?;
                    self.validate_source_proof(&decision.proof)?;
                    chain.gaps.extend(self.approval_gaps(&decision.id)?);
                    for participation in self.decision_participations(&decision.id)? {
                        let participant_links =
                            self.approval_links_for_participation(&participation.id)?;
                        let Some(decision_link) = self.reviewed_link_to(
                            &participant_links,
                            ApprovalLinkKind::ParticipationRecordedForDecision,
                            &decision.id,
                        )?
                        else {
                            continue;
                        };
                        let Some(office_link) = self.reviewed_link_to(
                            &participant_links,
                            ApprovalLinkKind::ParticipationByOfficeholder,
                            &participation.office_term_id,
                        )?
                        else {
                            continue;
                        };
                        let term = self.office_term(&participation.office_term_id)?;
                        self.validate_source_proof(&participation.proof)?;
                        chain.gaps.extend(self.approval_gaps(&term.id)?);
                        if !chain
                            .office_terms
                            .iter()
                            .any(|existing| existing.id == term.id)
                        {
                            chain.office_terms.push(term);
                        }
                        chain.participations.push(participation);
                        chain.approved_links.push(decision_link);
                        chain.approved_links.push(office_link);
                    }
                    chain.authorizing_decisions.push(decision);
                }
                ApprovalLinkKind::ContractIncludesTechnology if link.from_id == contract_id => {
                    let technology = self.technology_record(&link.to_id)?;
                    self.validate_source_proof(&technology.proof)?;
                    chain.technologies.push(technology);
                }
                _ => {}
            }
            chain.approved_links.push(link);
        }
        chain.authorizing_decisions.sort_by(|a, b| a.id.cmp(&b.id));
        chain.authorizing_decisions.dedup_by(|a, b| a.id == b.id);
        chain.participations.sort_by(|a, b| a.id.cmp(&b.id));
        chain.participations.dedup_by(|a, b| a.id == b.id);
        chain.office_terms.sort_by(|a, b| a.id.cmp(&b.id));
        chain.office_terms.dedup_by(|a, b| a.id == b.id);
        chain.technologies.sort_by(|a, b| a.id.cmp(&b.id));
        chain.technologies.dedup_by(|a, b| a.id == b.id);
        chain.approved_links.sort_by(|a, b| a.id.cmp(&b.id));
        chain.approved_links.dedup_by(|a, b| a.id == b.id);
        if !chain.authorizing_decisions.is_empty() {
            chain
                .gaps
                .retain(|gap| gap.kind != ApprovalGapKind::ContractAuthorizationNotEstablished);
        }
        chain.gaps.sort_by(|a, b| a.id.cmp(&b.id));
        chain.gaps.dedup_by(|a, b| a.id == b.id);
        Ok(chain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn proof(store: &Store) -> SourceProof {
        let bytes = b"official record: Chino City Council approved Axon agreement; five Aye votes";
        let digest = sha256_hex(bytes);
        store
            .persist_content(&digest, bytes)
            .expect("persist source");
        SourceProof {
            source_url: "https://chino.legistar.com/LegislationDetail.aspx?ID=1".to_owned(),
            snapshot_digest: digest,
            locator: "matter 26-308, agenda item 52243".to_owned(),
            quote: "approved Axon agreement".to_owned(),
        }
    }

    fn approve(store: &Store, link: &ApprovalLink, at: &str) {
        store
            .insert_approval_review(&ApprovalReview {
                id: ApprovalReview::id_for(&link.id, at),
                link_id: link.id.clone(),
                state: ReviewState::Approved,
                bound_digest: link.binding_digest(),
                reviewer: "reviewer".to_owned(),
                note: "checked exact official source".to_owned(),
                decided_at: at.to_owned(),
                supersedes: None,
            })
            .expect("approve link");
    }

    #[test]
    #[allow(clippy::too_many_lines)] // End-to-end review, publication, and correction lifecycle.
    fn approval_chain_requires_reviewed_links_and_preserves_corrections() {
        let dir = tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        let source = proof(&store);
        let term_bytes = b"<h2>Term of Office</h2><p>2026-2030</p>";
        let term_digest = sha256_hex(term_bytes);
        store
            .persist_content(&term_digest, term_bytes)
            .expect("persist term source");
        let term = OfficeTerm {
            id: OfficeTerm::id_for("Chino, California", "City Council", "100", "2026-01-01"),
            person_name: "Example Councilmember".to_owned(),
            official_person_id: Some("100".to_owned()),
            office_title: "Councilmember".to_owned(),
            body: "City Council".to_owned(),
            jurisdiction: "Chino, California".to_owned(),
            starts_on: Some("2026-01-01".to_owned()),
            ends_on: None,
            term_span_raw: Some("2026-2030".to_owned()),
            term_proof: Some(SourceProof {
                source_url: "https://example.test/office".to_owned(),
                snapshot_digest: term_digest.clone(),
                locator: "h2[Term of Office]+p".to_owned(),
                quote: "2026-2030".to_owned(),
            }),
            proof: source.clone(),
        };
        let decision = CivicDecision {
            id: CivicDecision::id_for("Chino, California", "26-308", "52243"),
            jurisdiction: "Chino, California".to_owned(),
            body: "City Council".to_owned(),
            official_matter_id: "26-308".to_owned(),
            official_event_item_id: "52243".to_owned(),
            decided_on: "2026-06-16".to_owned(),
            subject: "Axon Basic with Full AI agreement".to_owned(),
            kind: DecisionKind::LegislativeVote,
            outcome: DecisionOutcome::Approved,
            proof: source.clone(),
        };
        let participation = DecisionParticipation {
            id: DecisionParticipation::id_for(&decision.id, &term.id, ParticipationKind::VoteYes),
            decision_id: decision.id.clone(),
            office_term_id: term.id.clone(),
            kind: ParticipationKind::VoteYes,
            official_vote_id: Some("200".to_owned()),
            raw_vote_value: Some("Aye".to_owned()),
            proof: source.clone(),
        };
        let contract = ContractRecord {
            id: ContractRecord::id_for("Chino, California", "26-308"),
            procurement_matter_id: None,
            official_identifier: "26-308".to_owned(),
            title: "Axon Basic with Full AI agreement".to_owned(),
            vendor: Some("Axon Enterprise".to_owned()),
            amount_raw: None,
            term_raw: Some("five years".to_owned()),
            status: ContractStatus::Authorized,
            proof: source.clone(),
        };
        let technology = TechnologyRecord {
            id: TechnologyRecord::id_for("Axon Basic with Full AI", "surveillance technology"),
            name: "Axon Basic with Full AI".to_owned(),
            category: "surveillance technology".to_owned(),
            vendor: Some("Axon Enterprise".to_owned()),
            proof: source.clone(),
        };
        let mut unproven_term = term.clone();
        unproven_term.term_proof = None;
        assert!(store.insert_office_term(&unproven_term).is_err());
        assert!(store.insert_office_term(&term).expect("term"));
        assert!(!store.insert_office_term(&term).expect("idempotent term"));
        assert!(store.insert_civic_decision(&decision).expect("decision"));
        assert!(
            store
                .insert_decision_participation(&participation)
                .expect("participation")
        );
        assert!(store.insert_contract_record(&contract).expect("contract"));
        assert!(
            store
                .insert_technology_record(&technology)
                .expect("technology")
        );

        let consent_bytes = b"consent motion: five Aye votes";
        let consent_digest = sha256_hex(consent_bytes);
        store
            .persist_content(&consent_digest, consent_bytes)
            .expect("persist consent source");
        let consent_proof = SourceProof {
            snapshot_digest: consent_digest.clone(),
            locator: "consent motion".to_owned(),
            quote: "five Aye votes".to_owned(),
            ..source.clone()
        };
        let decision_link = ApprovalLink {
            id: ApprovalLink::id_for(
                ApprovalLinkKind::DecisionAuthorizesContract,
                &decision.id,
                &contract.id,
                &source,
                std::slice::from_ref(&consent_proof),
            ),
            kind: ApprovalLinkKind::DecisionAuthorizesContract,
            from_id: decision.id.clone(),
            to_id: contract.id.clone(),
            proof: source.clone(),
            supporting_proofs: vec![consent_proof],
            note: "the exact agenda item authorizes the agreement".to_owned(),
        };
        assert_ne!(
            decision_link.id,
            ApprovalLink::id_for(
                ApprovalLinkKind::DecisionAuthorizesContract,
                &decision.id,
                &contract.id,
                &source,
                &[],
            ),
            "additional evidence must change link identity"
        );
        let technology_link = ApprovalLink {
            id: ApprovalLink::id_for(
                ApprovalLinkKind::ContractIncludesTechnology,
                &contract.id,
                &technology.id,
                &source,
                &[],
            ),
            kind: ApprovalLinkKind::ContractIncludesTechnology,
            from_id: contract.id.clone(),
            to_id: technology.id.clone(),
            proof: source.clone(),
            supporting_proofs: Vec::new(),
            note: "the agreement names the technology".to_owned(),
        };
        let vote_link = ApprovalLink {
            id: ApprovalLink::id_for(
                ApprovalLinkKind::ParticipationRecordedForDecision,
                &participation.id,
                &decision.id,
                &source,
                &[],
            ),
            kind: ApprovalLinkKind::ParticipationRecordedForDecision,
            from_id: participation.id.clone(),
            to_id: decision.id.clone(),
            proof: source.clone(),
            supporting_proofs: Vec::new(),
            note: "official vote row records Aye".to_owned(),
        };
        let office_link = ApprovalLink {
            id: ApprovalLink::id_for(
                ApprovalLinkKind::ParticipationByOfficeholder,
                &participation.id,
                &term.id,
                &source,
                &[],
            ),
            kind: ApprovalLinkKind::ParticipationByOfficeholder,
            from_id: participation.id.clone(),
            to_id: term.id.clone(),
            proof: source.clone(),
            supporting_proofs: Vec::new(),
            note: "the official vote identity matches the officeholder record".to_owned(),
        };
        store
            .insert_approval_link(&decision_link)
            .expect("decision link");
        store
            .insert_approval_link(&technology_link)
            .expect("technology link");
        store.insert_approval_link(&vote_link).expect("vote link");
        store
            .insert_approval_link(&office_link)
            .expect("office link");

        let gap = ApprovalGap {
            id: ApprovalGap::id_for(
                &contract.id,
                ApprovalGapKind::TechnologyUseNotEstablished,
                "2026-06-17T00:00:00Z",
            ),
            subject_id: contract.id.clone(),
            kind: ApprovalGapKind::TechnologyUseNotEstablished,
            checked_source_digests: vec![source.snapshot_digest.clone()],
            detail: "No executed deployment record checked".to_owned(),
            recorded_at: "2026-06-17T00:00:00Z".to_owned(),
        };
        store.insert_approval_gap(&gap).expect("gap");

        let unreviewed = store
            .approval_chain(&contract.id)
            .expect("unreviewed chain");
        assert!(unreviewed.approved_links.is_empty());
        assert!(unreviewed.office_terms.is_empty());
        assert_eq!(unreviewed.gaps, vec![gap]);

        approve(&store, &decision_link, "2026-06-18T00:00:00Z");
        approve(&store, &technology_link, "2026-06-18T00:00:01Z");
        let without_vote = store
            .approval_chain(&contract.id)
            .expect("chain without vote review");
        assert_eq!(without_vote.authorizing_decisions.len(), 1);
        assert_eq!(without_vote.technologies.len(), 1);
        assert!(without_vote.participations.is_empty());

        approve(&store, &vote_link, "2026-06-18T00:00:02Z");
        let without_office = store
            .approval_chain(&contract.id)
            .expect("office link pending");
        assert!(without_office.participations.is_empty());
        approve(&store, &office_link, "2026-06-18T00:00:03Z");
        let reviewed = store.approval_chain(&contract.id).expect("reviewed chain");
        assert_eq!(reviewed.participations, vec![participation]);
        assert_eq!(reviewed.office_terms, vec![term.clone()]);
        assert_eq!(reviewed.approved_links.len(), 4);

        let consent_path = store.content_path(&consent_digest).expect("consent path");
        std::fs::write(&consent_path, b"tampered source").expect("tamper supporting proof");
        assert!(store.approval_link_review_state(&decision_link.id).is_err());
        assert!(store.approval_chain(&contract.id).is_err());
        std::fs::write(&consent_path, consent_bytes).expect("restore supporting proof");
        std::fs::remove_file(&consent_path).expect("remove supporting proof");
        assert!(store.approval_link_review_state(&decision_link.id).is_err());
        std::fs::write(&consent_path, consent_bytes).expect("restore missing proof");
        assert_eq!(
            store
                .approval_link_review_state(&decision_link.id)
                .expect("verified proof"),
            Some(ReviewState::Approved)
        );
        let term_path = store.content_path(&term_digest).expect("term path");
        std::fs::write(&term_path, b"tampered term").expect("tamper term proof");
        assert!(store.office_term(&term.id).is_err());
        assert!(store.approval_chain(&contract.id).is_err());
        std::fs::write(&term_path, term_bytes).expect("restore term proof");

        let correction = ApprovalCorrection {
            id: ApprovalCorrection::id_for(
                &decision_link.id,
                ApprovalCorrectionKind::Retracted,
                "2026-06-19T00:00:00Z",
            ),
            link_id: decision_link.id.clone(),
            kind: ApprovalCorrectionKind::Retracted,
            replacement_link_id: None,
            operator: "reviewer".to_owned(),
            reason: "official source corrected".to_owned(),
            recorded_at: "2026-06-19T00:00:00Z".to_owned(),
            proof: source,
        };
        store
            .insert_approval_correction(&correction)
            .expect("correction");
        assert_eq!(
            store
                .approval_corrections(&decision_link.id)
                .expect("history"),
            vec![correction]
        );
        let corrected = store.approval_chain(&contract.id).expect("corrected chain");
        assert!(corrected.authorizing_decisions.is_empty());
        assert!(corrected.participations.is_empty());
        assert_eq!(corrected.technologies.len(), 1);
    }

    #[test]
    fn proof_requires_preserved_exact_bytes() {
        let dir = tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        let bytes = b"official";
        let digest = sha256_hex(bytes);
        assert!(store.persist_content(&digest, b"altered").is_err());
        let term = OfficeTerm {
            id: "term:missing-proof".to_owned(),
            person_name: "Example".to_owned(),
            official_person_id: None,
            office_title: "Councilmember".to_owned(),
            body: "Council".to_owned(),
            jurisdiction: "Chino".to_owned(),
            starts_on: None,
            ends_on: None,
            term_span_raw: None,
            term_proof: None,
            proof: SourceProof {
                source_url: "https://example.test".to_owned(),
                snapshot_digest: digest,
                locator: "row 1".to_owned(),
                quote: "Example".to_owned(),
            },
        };
        assert!(store.insert_office_term(&term).is_err());
    }

    #[test]
    fn proof_quote_must_appear_at_exact_json_field_or_in_html() {
        let dir = tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        let json = br#"[{"VoteValueName":"NO","Name":"Other"},{"VoteValueName":"AYES","Name":"Eunice M. Ulloa","Agenda":"Consent\u0020Calendar"}]"#;
        let digest = sha256_hex(json);
        store.persist_content(&digest, json).expect("persist JSON");
        let mut proof = SourceProof {
            source_url: "https://example.test/votes.json".to_owned(),
            snapshot_digest: digest,
            locator: "$[1].VoteValueName".to_owned(),
            quote: "AYES".to_owned(),
        };
        assert!(store.validate_source_proof(&proof).is_ok());
        proof.locator = "$[0].VoteValueName".to_owned();
        assert!(store.validate_source_proof(&proof).is_err());
        proof.locator = "$[1].Name".to_owned();
        assert!(store.validate_source_proof(&proof).is_err());
        proof.locator = "$[bogus].VoteValueName".to_owned();
        assert!(store.validate_source_proof(&proof).is_err());
        proof.locator = "$[1].Agenda".to_owned();
        proof.quote = "Consent Calendar".to_owned();
        assert!(
            store.validate_source_proof(&proof).is_ok(),
            "JSON escapes are decoded"
        );
        proof.locator = "vote response".to_owned();
        proof.quote = "VoteValueName".to_owned();
        assert!(
            store.validate_source_proof(&proof).is_err(),
            "JSON key names are not evidence values"
        );

        let html = b"<h3>Mayor Pro Tem</h3><p>2022-2026</p>";
        let digest = sha256_hex(html);
        store.persist_content(&digest, html).expect("persist HTML");
        proof.snapshot_digest = digest;
        proof.locator = "h3".to_owned();
        proof.quote = "Mayor Pro Tem".to_owned();
        assert!(store.validate_source_proof(&proof).is_ok());
        proof.quote = "Council Member".to_owned();
        assert!(store.validate_source_proof(&proof).is_err());
    }

    #[test]
    fn chain_rechecks_contract_proof_after_insert() {
        let dir = tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        let bytes = b"official Axon agreement";
        let digest = sha256_hex(bytes);
        store
            .persist_content(&digest, bytes)
            .expect("persist source");
        let contract = ContractRecord {
            id: ContractRecord::id_for("Chino", "26-308"),
            procurement_matter_id: None,
            official_identifier: "26-308".to_owned(),
            title: "Axon agreement".to_owned(),
            vendor: Some("Axon".to_owned()),
            amount_raw: None,
            term_raw: None,
            status: ContractStatus::Authorized,
            proof: SourceProof {
                source_url: "https://example.test/matter".to_owned(),
                snapshot_digest: digest.clone(),
                locator: "matter title".to_owned(),
                quote: "Axon agreement".to_owned(),
            },
        };
        store
            .insert_contract_record(&contract)
            .expect("insert contract");
        assert!(store.approval_chain(&contract.id).is_ok());
        std::fs::write(store.content_path(&digest).expect("path"), b"tampered")
            .expect("tamper source");
        assert!(store.approval_chain(&contract.id).is_err());
    }
}
