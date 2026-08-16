//! Versioned domain types for subjects, actions, document roles, reviews,
//! processing runs, source reviews, fetch observations, and citation geometry.

use serde::{Deserialize, Serialize};

use crate::{sha256_hex, stable_id};

/// The subject of an institutional action: the thing acted upon.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubjectKind {
    Ordinance,
    Policy,
    Solicitation,
    Contract,
    Amendment,
    Vendor,
    SurveillanceTechnology,
    Program,
    BudgetItem,
    Other,
    Unknown,
}

impl SubjectKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ordinance => "Ordinance",
            Self::Policy => "Policy",
            Self::Solicitation => "Solicitation",
            Self::Contract => "Contract",
            Self::Amendment => "Amendment",
            Self::Vendor => "Vendor",
            Self::SurveillanceTechnology => "Surveillance technology",
            Self::Program => "Program",
            Self::BudgetItem => "Budget item",
            Self::Other => "Other",
            Self::Unknown => "Unknown",
        }
    }
}

/// An institutional action applied to exactly one subject.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Mentioned,
    Proposed,
    HearingScheduled,
    VoteScheduled,
    Approved,
    Rejected,
    Awarded,
    Executed,
    Amended,
    Renewed,
    Expanded,
    DeploymentReported,
    PolicyChanged,
    Unknown,
}

impl ActionKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Mentioned => "Mentioned",
            Self::Proposed => "Proposed",
            Self::HearingScheduled => "Hearing scheduled",
            Self::VoteScheduled => "Vote scheduled",
            Self::Approved => "Approved",
            Self::Rejected => "Rejected",
            Self::Awarded => "Awarded",
            Self::Executed => "Executed",
            Self::Amended => "Amended",
            Self::Renewed => "Renewed",
            Self::Expanded => "Expanded",
            Self::DeploymentReported => "Deployment reported",
            Self::PolicyChanged => "Policy changed",
            Self::Unknown => "Unknown",
        }
    }
}

/// The role a document plays within a matter.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentRole {
    Agenda,
    Minutes,
    Ordinance,
    Policy,
    Solicitation,
    Award,
    Contract,
    Amendment,
    StaffReport,
    Presentation,
    Other,
}

impl DocumentRole {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Agenda => "Agenda",
            Self::Minutes => "Minutes",
            Self::Ordinance => "Ordinance",
            Self::Policy => "Policy",
            Self::Solicitation => "Solicitation",
            Self::Award => "Award",
            Self::Contract => "Contract",
            Self::Amendment => "Amendment",
            Self::StaffReport => "Staff report",
            Self::Presentation => "Presentation",
            Self::Other => "Other",
        }
    }
}

/// A subject of an institutional action, bound to its supporting citations.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Subject {
    pub id: String,
    pub matter_id: String,
    pub kind: SubjectKind,
    pub name: String,
    pub detail: String,
    pub citations: Vec<String>,
    pub known: bool,
}

impl Subject {
    pub fn id_for(matter_id: &str, name: &str) -> String {
        stable_id("subject", &[matter_id, name])
    }
}

/// An institutional action applied to exactly one subject.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Action {
    pub id: String,
    pub matter_id: String,
    pub subject_id: String,
    pub kind: ActionKind,
    pub summary: String,
    pub citations: Vec<String>,
    pub known: bool,
}

impl Action {
    pub fn id_for(matter_id: &str, subject_id: &str, kind: ActionKind, summary: &str) -> String {
        stable_id("action", &[matter_id, subject_id, kind.label(), summary])
    }
}

/// A Legistar matter and its discovered attachments.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Matter {
    pub id: String,
    pub source_id: String,
    pub official_matter_id: String,
    pub title: String,
    pub status: String,
    pub url: String,
    pub document_role: DocumentRole,
}

/// An attachment discovered through documented official Legistar fields.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MatterAttachment {
    pub id: String,
    pub matter_id: String,
    pub official_id: String,
    pub name: String,
    pub url: String,
    pub evidence_id: Option<String>,
}

/// A bounding rectangle in PDF user-space coordinates.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct BoundingRect {
    pub x_min: f64,
    pub y_min: f64,
    pub x_max: f64,
    pub y_max: f64,
}

impl BoundingRect {
    pub fn digest(self) -> String {
        sha256_hex(
            format!(
                "{:.6},{:.6},{:.6},{:.6}",
                self.x_min, self.y_min, self.x_max, self.y_max
            )
            .as_bytes(),
        )
    }
}

/// An immutable PDF text-map artifact covering a single page.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TextMap {
    pub id: String,
    pub evidence_id: String,
    pub page_number: u32,
    pub page_width: f64,
    pub page_height: f64,
    pub page_rotation: i32,
    pub coordinate_system: String,
    pub words: Vec<MapWord>,
    pub extractor: String,
    pub extractor_version: String,
    pub digest: String,
    pub source_digest: String,
}

impl TextMap {
    pub fn id_for(evidence_id: &str, page_number: u32, digest: &str) -> String {
        stable_id("textmap", &[evidence_id, &page_number.to_string(), digest])
    }

    pub fn compute_digest(&self) -> String {
        let mut bytes = Vec::new();
        for word in &self.words {
            bytes.extend_from_slice(word.text.as_bytes());
            bytes.extend_from_slice(b"\x00");
            bytes.extend_from_slice(word.rect.digest().as_bytes());
            bytes.extend_from_slice(b"\x00");
        }
        sha256_hex(&bytes)
    }
}

/// A single extracted word with its bounding box.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MapWord {
    pub text: String,
    pub rect: BoundingRect,
}

/// A page-accurate citation with validated geometry.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PageCitation {
    pub id: String,
    pub evidence_id: String,
    pub quote: String,
    pub quote_digest: String,
    pub page_number: u32,
    pub rects: Vec<BoundingRect>,
    pub normalized_range: LocatorRange,
    pub text_map_digest: String,
    pub evidence_digest: String,
    pub ocr_confidence: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LocatorRange {
    pub start: u32,
    pub end: u32,
}

impl PageCitation {
    pub fn id_for(evidence_id: &str, quote_digest: &str, page: u32) -> String {
        stable_id(
            "page_citation",
            &[evidence_id, quote_digest, &page.to_string()],
        )
    }
}

/// The state of a human citation-review decision.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewState {
    Pending,
    Approved,
    Rejected,
    NeedsContext,
    Superseded,
}

impl ReviewState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Approved => "Approved",
            Self::Rejected => "Rejected",
            Self::NeedsContext => "Needs context",
            Self::Superseded => "Superseded",
        }
    }
}

/// An append-only human review decision bound to exact content digests.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReviewDecision {
    pub id: String,
    pub citation_id: String,
    pub state: ReviewState,
    pub reviewer: String,
    pub note: String,
    pub bound_digest: String,
    pub decision_digest: String,
    pub decided_at: String,
    pub supersedes: Option<String>,
}

impl ReviewDecision {
    pub fn id_for(citation_id: &str, decided_at: &str) -> String {
        stable_id("review", &[citation_id, decided_at])
    }
}

/// The digest of every value a review decision binds to.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReviewBinding {
    pub evidence_id: String,
    pub source_digest: String,
    pub locator_or_geometry: String,
    pub quote: String,
    pub quote_digest: String,
    pub rule_digest: String,
    pub processing_artifact_digest: String,
    pub proposed_public_fields: String,
}

impl ReviewBinding {
    pub fn digest(&self) -> String {
        let mut parts = vec![
            self.evidence_id.as_str(),
            self.source_digest.as_str(),
            self.locator_or_geometry.as_str(),
            self.quote.as_str(),
            self.quote_digest.as_str(),
            self.rule_digest.as_str(),
            self.processing_artifact_digest.as_str(),
            self.proposed_public_fields.as_str(),
        ];
        parts.sort_unstable();
        stable_id("review-binding", &parts)
    }
}

/// An immutable processing-run provenance record.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProcessingRun {
    pub id: String,
    pub schema_version: u32,
    pub pnull_version: String,
    pub source_revision: String,
    pub rules_digest: String,
    pub state_config_digest: String,
    pub input_evidence_ids: Vec<String>,
    pub native_tools: Vec<NativeTool>,
    pub sandbox_backend: String,
    pub sandbox_version: String,
    pub resource_budgets: serde_json::Value,
    pub resource_consumption: serde_json::Value,
    pub started_at: String,
    pub completed_at: String,
    pub outcome: String,
    pub errors: Vec<crate::StructuredError>,
    pub output_artifacts: Vec<OutputArtifact>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NativeTool {
    pub name: String,
    pub version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OutputArtifact {
    pub kind: String,
    pub id: String,
    pub digest: String,
}

/// A persistent, expiring human review of a source's robots and terms.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceReview {
    pub id: String,
    pub source_id: String,
    pub source_config_digest: String,
    pub reviewed_hosts: Vec<String>,
    pub endpoint_patterns: Vec<String>,
    pub robots_url: String,
    pub robots_snapshot_digest: String,
    pub robots_provenance: Option<String>,
    pub terms_urls: Vec<String>,
    pub terms_snapshot_digests: Vec<String>,
    pub reviewer: String,
    pub note: String,
    pub reviewed_at: String,
    pub expires_at: String,
    pub minimum_interval_seconds: u64,
    pub restrictions: Vec<String>,
    pub supersedes: Option<String>,
}

impl SourceReview {
    pub fn id_for(source_id: &str, reviewed_at: &str) -> String {
        stable_id("source-review", &[source_id, reviewed_at])
    }
}

/// A provenance-aware HTTP fetch observation for one request or redirect.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FetchObservation {
    pub id: String,
    pub source_id: Option<String>,
    pub requested_url: String,
    pub resolved_ips: Vec<String>,
    pub retrieved_at: String,
    pub method: String,
    pub status_code: u16,
    pub redirect_target: Option<String>,
    pub final_url: String,
    pub allowlisted_headers: Vec<(String, String)>,
    pub content_type: Option<String>,
    pub content_length: Option<u64>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub body_digest: Option<String>,
    pub error: Option<crate::StructuredError>,
}

impl FetchObservation {
    pub fn id_for(requested_url: &str, retrieved_at: &str, status: u16) -> String {
        stable_id("fetch", &[requested_url, retrieved_at, &status.to_string()])
    }
}

/// A conditional-retrieval result referencing prior evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConditionalResult {
    pub observation: FetchObservation,
    pub unchanged: bool,
    pub prior_evidence_id: Option<String>,
    pub new_evidence_id: Option<String>,
}

/// An X posting attempt with operator-visible segment state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct XAttempt {
    pub id: String,
    pub alert_id: String,
    pub draft_digest: String,
    pub started_at: String,
    pub status: String,
    pub segments: Vec<XSegment>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct XSegment {
    pub index: u32,
    pub remote_id: Option<String>,
    pub state: String,
}

/// An append-only operator reconciliation decision for an uncertain X attempt.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct XReconciliation {
    pub id: String,
    pub attempt_id: String,
    pub decision: String,
    pub remote_id: Option<String>,
    pub note: String,
    pub operator: String,
    pub decided_at: String,
}

/// A structured publication allowlist entry: which field categories may go public.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PublicationAllowlist {
    pub id: String,
    pub field_categories: Vec<String>,
    pub created_at: String,
    pub note: String,
}
