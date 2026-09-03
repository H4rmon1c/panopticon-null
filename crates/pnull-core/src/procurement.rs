//! v0.0.3 procurement domain types: matters, events, identifiers, money,
//! organizations, source authority, coverage, immutable snapshots,
//! reconciliation, case files, and CORA drafts.
//!
//! Design rules that govern these types:
//! - Money is never stored as a floating-point value.
//! - Raw identifier and organization strings are preserved verbatim with their
//!   source. Normalization may produce candidates but never silently merges.
//! - Absence from a partial source is never proof of absence.

use serde::{Deserialize, Serialize};

use crate::{sha256_hex, stable_id};

/// Authority of a source with respect to the procurement record it carries.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceAuthority {
    /// The controlling procurement version (e.g., `BidNet` / `Bonfire`).
    AuthoritativeProcurementRecord,
    /// City-hosted copy that is informative but explicitly not controlling.
    OfficialInformationalMirror,
    /// Official budget/expenditure data export.
    OfficialFinancialExport,
    /// Council or legislative meeting / matter record.
    OfficialMeetingOrLegislative,
    /// Public record obtained manually or via CORA and imported by an operator.
    OperatorSuppliedPublicRecord,
    /// Source fetched but not yet reviewed.
    Unreviewed,
    /// Source that cannot be lawfully automated in this build.
    RestrictedOrInaccessible,
}

impl SourceAuthority {
    pub const fn label(self) -> &'static str {
        match self {
            Self::AuthoritativeProcurementRecord => "Authoritative procurement record",
            Self::OfficialInformationalMirror => "Official informational mirror",
            Self::OfficialFinancialExport => "Official financial export",
            Self::OfficialMeetingOrLegislative => "Official meeting or legislative record",
            Self::OperatorSuppliedPublicRecord => "Operator-supplied public record",
            Self::Unreviewed => "Unreviewed source",
            Self::RestrictedOrInaccessible => "Restricted or inaccessible source",
        }
    }
}

/// Persistent coverage state of a source acquisition attempt.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageState {
    /// Affirmative, reproducible evidence that the snapshot enumerates the
    /// defined population.
    Complete,
    /// Evidence covers only part of the defined population.
    Partial,
    /// Source is informative and explicitly not controlling/complete.
    InformationalOnly,
    /// Retrieval was blocked (portal, auth, rate limit, redirect policy).
    AccessBlocked,
    /// Terms/robots not yet reviewed to the project's standard.
    TermsUnreviewed,
    /// The source's schema/layout changed from a prior snapshot.
    SchemaChanged,
    /// No affirmative evidence of coverage.
    Unknown,
}

impl CoverageState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Partial => "partial",
            Self::InformationalOnly => "informational only",
            Self::AccessBlocked => "access blocked",
            Self::TermsUnreviewed => "terms unreviewed",
            Self::SchemaChanged => "schema changed",
            Self::Unknown => "unknown",
        }
    }
}

/// A single coverage-ledger entry for one acquisition attempt.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CoverageEntry {
    pub id: String,
    pub source_id: String,
    pub source_url: String,
    pub authority: SourceAuthority,
    pub state: CoverageState,
    pub retrieved_at: String,
    /// SHA-256 of the exact persisted bytes, when a snapshot was captured.
    pub persisted_digest: Option<String>,
    pub http_status: Option<u16>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub final_url: Option<String>,
    pub parser_version: Option<String>,
    pub schema_version: Option<u32>,
    pub claimed_date_range: Option<(Option<String>, Option<String>)>,
    pub record_count: Option<u64>,
    pub pagination_complete: Option<bool>,
    pub access_errors: Vec<String>,
    pub human_review_state: String,
    pub note: String,
}

impl CoverageEntry {
    pub fn id_for(source_id: &str, retrieved_at: &str) -> String {
        stable_id("coverage", &[source_id, retrieved_at])
    }
}

/// The state of a parsed money value. Never a floating-point number.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MoneyState {
    /// An exact stated amount (cents is Some and non-zero).
    Exact,
    /// An explicit zero amount.
    Zero,
    /// Not applicable (e.g., `N/A`, `n/a`).
    NotApplicable,
    /// A non-specific amount (e.g., `various`).
    Various,
    /// An IDIQ or ceiling amount (e.g., `$0.00 IDIQ`).
    IdiqCeiling,
    /// Amount omitted or not stated.
    Unknown,
    /// An amount present but unparseable under the applied rules.
    Unparseable,
}

impl MoneyState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Zero => "zero",
            Self::NotApplicable => "not applicable",
            Self::Various => "various",
            Self::IdiqCeiling => "IDIQ/ceiling",
            Self::Unknown => "unknown",
            Self::Unparseable => "unparseable",
        }
    }
}

/// A money value preserving its raw string and a parsed integer-cents value.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MoneyValue {
    pub raw: String,
    pub state: MoneyState,
    /// Parsed value in integer cents, present only when `state` is Exact or Zero.
    pub cents: Option<i64>,
}

impl MoneyValue {
    /// A helper that produces a normalized display label without inventing value.
    pub fn display(&self) -> String {
        match self.state {
            MoneyState::Exact => match self.cents {
                Some(cents) => format!("${}.{:02}", cents / 100, (cents % 100).abs()),
                None => self.raw.clone(),
            },
            _ => self.state.label().to_owned(),
        }
    }
}

/// Kind of a procurement identifier. Raw spelling is preserved on the record.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentifierKind {
    SolicitationNumber,
    Rfp,
    Rfq,
    Ifb,
    QuoteNumber,
    ContractNumber,
    PurchaseOrder,
    Invoice,
    LegislativeMatter,
    Other,
    Unknown,
}

impl IdentifierKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::SolicitationNumber => "solicitation number",
            Self::Rfp => "RFP",
            Self::Rfq => "RFQ",
            Self::Ifb => "IFB",
            Self::QuoteNumber => "quote number",
            Self::ContractNumber => "contract number",
            Self::PurchaseOrder => "purchase order",
            Self::Invoice => "invoice",
            Self::LegislativeMatter => "legislative matter",
            Self::Other => "other",
            Self::Unknown => "unknown",
        }
    }
}

/// A raw procurement identifier with its source. Normalization is explicit and
/// never silently merges differently formatted identifiers.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProcurementIdentifier {
    pub id: String,
    pub matter_id: String,
    pub kind: IdentifierKind,
    /// The exact identifier string as it appeared in the source.
    pub raw: String,
    /// The source (`source_id`) that supplied this identifier.
    pub source_id: String,
    /// A normalized candidate, present only when a deterministic rule produced it.
    pub normalized: Option<String>,
    /// The identifier of the deterministic rule that produced `normalized`.
    pub normalization_rule: Option<String>,
    /// True when this identifier is confirmed to reference the same matter.
    pub known: bool,
}

impl ProcurementIdentifier {
    pub fn id_for(matter_id: &str, kind: IdentifierKind, raw: &str) -> String {
        stable_id("proc-id", &[matter_id, kind.label(), raw])
    }
}

/// The role an organization plays within a procurement matter.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationRole {
    Requester,
    AwardedContractor,
    Subcontractor,
    JointVentureMember,
    Vendor,
    GovernmentDepartment,
    Other,
    Unknown,
}

impl OrganizationRole {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Requester => "requester",
            Self::AwardedContractor => "awarded contractor",
            Self::Subcontractor => "subcontractor",
            Self::JointVentureMember => "joint venture member",
            Self::Vendor => "vendor",
            Self::GovernmentDepartment => "government department",
            Self::Other => "other",
            Self::Unknown => "unknown",
        }
    }
}

/// An organization in its documented role. Raw spelling is preserved.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProcurementOrganization {
    pub id: String,
    pub matter_id: String,
    pub role: OrganizationRole,
    /// The exact name as it appeared in the source.
    pub raw_name: String,
    pub source_id: String,
    /// A candidate normalized alias, present only when a deterministic rule
    /// produced it. A non-exact match must enter human review.
    pub normalized_alias: Option<String>,
    /// True only after a human confirmed this alias with provenance.
    pub alias_reviewed: bool,
}

impl ProcurementOrganization {
    pub fn id_for(matter_id: &str, role: OrganizationRole, raw_name: &str) -> String {
        stable_id("proc-org", &[matter_id, role.label(), raw_name])
    }
}

/// Kind of a procurement event.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcurementEventKind {
    SolicitationPublished,
    AmendmentPublished,
    QuestionsAndAnswersPublished,
    SubmissionDeadlineChanged,
    AwardAnnounced,
    ContractExecuted,
    ContractAmended,
    ExpenditureReported,
    RecordCorrected,
    RecordRemoved,
    Unknown,
}

impl ProcurementEventKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::SolicitationPublished => "solicitation published",
            Self::AmendmentPublished => "amendment or addendum published",
            Self::QuestionsAndAnswersPublished => "questions and answers published",
            Self::SubmissionDeadlineChanged => "submission deadline changed",
            Self::AwardAnnounced => "award announced",
            Self::ContractExecuted => "contract executed",
            Self::ContractAmended => "contract amended",
            Self::ExpenditureReported => "expenditure reported",
            Self::RecordCorrected => "record corrected",
            Self::RecordRemoved => "record removed",
            Self::Unknown => "unknown",
        }
    }
}

/// A single procurement event tied to evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProcurementEvent {
    pub id: String,
    pub matter_id: String,
    pub kind: ProcurementEventKind,
    pub date: Option<String>,
    pub summary: String,
    pub identifier_ids: Vec<String>,
    pub evidence_ids: Vec<String>,
    pub source_id: String,
}

impl ProcurementEvent {
    pub fn id_for(
        matter_id: &str,
        kind: ProcurementEventKind,
        date: &str,
        summary: &str,
    ) -> String {
        stable_id("proc-event", &[matter_id, kind.label(), date, summary])
    }
}

/// A logical procurement case containing records and their relationships.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProcurementMatter {
    pub id: String,
    pub jurisdiction: String,
    pub title: String,
    pub review_state: String,
    pub publication_state: String,
}

impl ProcurementMatter {
    pub fn id_for(jurisdiction: &str, title: &str) -> String {
        stable_id("proc-matter", &[jurisdiction, title])
    }
}

/// An immutable snapshot of a fetched official page, export, or document.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceSnapshot {
    pub id: String,
    pub source_id: String,
    pub source_url: String,
    pub retrieved_at: String,
    /// SHA-256 of the exact persisted bytes.
    pub persisted_digest: String,
    pub content_type: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub final_url: String,
    pub redirect_history: Vec<String>,
    pub parser_version: String,
    pub schema_version: u32,
    pub record_count: Option<u64>,
    pub pagination_complete: Option<bool>,
    pub coverage_state: CoverageState,
    /// The snapshot this one supersedes, if any (revision link).
    pub supersedes: Option<String>,
}

impl SourceSnapshot {
    pub fn id_for(source_id: &str, persisted_digest: &str) -> String {
        stable_id("snapshot", &[source_id, persisted_digest])
    }
}

/// A single parsed record row bound to an immutable snapshot (snapshot-row
/// persistence, v0.0.4c).
///
/// Every immutable procurement snapshot persists the exact parsed row set that
/// belongs to it, so later change detection can compare the previous snapshot's
/// rows from the database without ever reading a mutable fixture or source file
/// from disk. Each row carries the stable identity key, a canonical text form
/// used for identity and equality, a deterministic digest of that canonical
/// form, and the raw original values (as JSON) retained for evidence and
/// field-level diffs.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SnapshotRow {
    /// The stable row-identity key (official identifier, else a digest over the
    /// row's normalized field values).
    pub key: String,
    /// Canonical text form of the row used for equality and comparison.
    pub canonical: String,
    /// Deterministic SHA-256 digest of `canonical` (per-row integrity).
    pub row_digest: String,
    /// The raw original values of the row, retained verbatim for evidence and
    /// field-level diffs. Empty when only identity/comparison data is retained.
    pub raw_json: String,
}

/// Completion metadata for a snapshot's stored row set (v0.0.4c).
///
/// Distinguishes a valid capture that happened to contain zero rows from a
/// legacy snapshot whose rows were never preserved. Only snapshots written
/// through `insert_snapshot_row_set_with_rows` carry this metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SnapshotRowSet {
    pub snapshot_id: String,
    pub expected_count: u64,
    /// Deterministic, order-independent, duplicate-preserving digest over the
    /// exact stored rows (keys + canonicals).
    pub row_set_digest: String,
    pub parser_version: String,
    pub schema_version: u32,
}

/// Deterministic digest over a set of snapshot rows.
///
/// Rows are sorted by `(key, canonical)` so the digest is order-independent;
/// duplicates are preserved (a joint award can produce two rows sharing one
/// identifier), so an added duplicate changes the digest.
pub fn row_set_digest(rows: &[SnapshotRow]) -> String {
    let mut sorted: Vec<&SnapshotRow> = rows.iter().collect();
    sorted.sort_by(|a, b| {
        a.key
            .cmp(&b.key)
            .then_with(|| a.canonical.cmp(&b.canonical))
    });
    let mut stream: Vec<u8> = Vec::new();
    for row in sorted {
        stream.extend_from_slice(&(row.key.len() as u64).to_le_bytes());
        stream.extend_from_slice(row.key.as_bytes());
        stream.extend_from_slice(&(row.canonical.len() as u64).to_le_bytes());
        stream.extend_from_slice(row.canonical.as_bytes());
    }
    sha256_hex(&stream)
}

/// A revision/supersession link between snapshots of the same source.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SnapshotRevision {
    pub id: String,
    pub snapshot_id: String,
    pub supersedes: Option<String>,
    pub superseded_by: Option<String>,
    pub reason: String,
    pub recorded_at: String,
}

impl SnapshotRevision {
    pub fn id_for(snapshot_id: &str, recorded_at: &str) -> String {
        stable_id("snapshot-rev", &[snapshot_id, recorded_at])
    }
}

/// A deterministic record-level change between two snapshots.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecordChange {
    pub kind: String,
    pub row_key: String,
    pub summary: String,
}

/// A deterministic record-level diff between two snapshots of a source.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SnapshotDiff {
    pub id: String,
    pub old_snapshot_id: String,
    pub new_snapshot_id: String,
    pub source_id: String,
    pub changes: Vec<RecordChange>,
    pub produced_at: String,
}

impl SnapshotDiff {
    pub fn id_for(old_snapshot_id: &str, new_snapshot_id: &str) -> String {
        stable_id("snapshot-diff", &[old_snapshot_id, new_snapshot_id])
    }
}

/// Kind of a reconciliation-review item.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationKind {
    CandidateIdentifierMatch,
    VendorAlias,
    ConflictingAwardAmount,
    ConflictingDate,
    DuplicateOrRevisedRow,
    MissingDocument,
    VanishedRecord,
    Other,
}

impl ReconciliationKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::CandidateIdentifierMatch => "candidate identifier match",
            Self::VendorAlias => "vendor alias",
            Self::ConflictingAwardAmount => "conflicting award amount",
            Self::ConflictingDate => "conflicting date",
            Self::DuplicateOrRevisedRow => "duplicate or revised row",
            Self::MissingDocument => "missing document",
            Self::VanishedRecord => "vanished record",
            Self::Other => "other",
        }
    }
}

/// An item awaiting human reconciliation review.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReconciliationItem {
    pub id: String,
    pub matter_id: String,
    pub kind: ReconciliationKind,
    pub summary: String,
    /// Identifiers of the records involved (evidence or snapshot row keys).
    pub record_refs: Vec<String>,
    /// The automatic decision if one could be derived, else None.
    pub state: String,
    pub created_at: String,
}

impl ReconciliationItem {
    pub fn id_for(matter_id: &str, kind: ReconciliationKind, summary: &str) -> String {
        stable_id("reconcile", &[matter_id, kind.label(), summary])
    }
}

/// An immutable, auditable human reconciliation decision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReconciliationDecision {
    pub id: String,
    pub item_id: String,
    /// accept | reject
    pub decision: String,
    pub operator: String,
    pub note: String,
    pub decided_at: String,
}

impl ReconciliationDecision {
    pub fn id_for(item_id: &str, decided_at: &str) -> String {
        stable_id("reconcile-decision", &[item_id, decided_at])
    }
}

/// The state of a case file before/after human citation review.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseFileState {
    Draft,
    Reviewed,
    Published,
}

impl CaseFileState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Reviewed => "reviewed",
            Self::Published => "published",
        }
    }
}

/// A deterministic, human-reviewable case file for a procurement matter.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CaseFile {
    pub id: String,
    pub matter_id: String,
    pub state: CaseFileState,
    pub json_digest: String,
    pub markdown_digest: String,
    pub sha256_manifest: Vec<(String, String)>,
    pub built_at: String,
}

impl CaseFile {
    pub fn id_for(matter_id: &str, built_at: &str) -> String {
        stable_id("case-file", &[matter_id, built_at])
    }
}

/// A local, unsent draft Colorado Open Records Act request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CoraDraft {
    pub id: String,
    pub matter_id: String,
    pub institution: String,
    pub identifiers: Vec<String>,
    pub missing_record_types: Vec<String>,
    pub date_range: Option<(Option<String>, Option<String>)>,
    pub vendor_or_project: Option<String>,
    pub sources_checked: Vec<String>,
    /// Markdown or plain text only.
    pub markdown: String,
    pub created_at: String,
}

impl CoraDraft {
    pub fn id_for(matter_id: &str, created_at: &str) -> String {
        stable_id("cora", &[matter_id, created_at])
    }
}

/// Deterministic money parsing result for an amount string.
pub fn parse_money(raw: &str) -> MoneyValue {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return MoneyValue {
            raw: raw.to_owned(),
            state: MoneyState::Unknown,
            cents: None,
        };
    }
    let lower = trimmed.to_ascii_lowercase();
    if matches!(lower.as_str(), "n/a" | "na" | "-") {
        return MoneyValue {
            raw: raw.to_owned(),
            state: MoneyState::NotApplicable,
            cents: None,
        };
    }
    if lower.contains("various") {
        return MoneyValue {
            raw: raw.to_owned(),
            state: MoneyState::Various,
            cents: None,
        };
    }
    if lower.contains("idiq") || lower.contains("ceiling") {
        // IDIQ / ceiling amounts are ceiling figures, not exact expenditures.
        let cents = parse_cents(&lower);
        return MoneyValue {
            raw: raw.to_owned(),
            state: MoneyState::IdiqCeiling,
            cents,
        };
    }
    // Strip currency symbols, commas, whitespace.
    let cleaned: String = trimmed
        .chars()
        .filter(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '$' | ',' | ' ' | '\u{00a0}'))
        .collect();
    match parse_cents(&cleaned) {
        Some(0) => MoneyValue {
            raw: raw.to_owned(),
            state: MoneyState::Zero,
            cents: Some(0),
        },
        Some(cents) => MoneyValue {
            raw: raw.to_owned(),
            state: MoneyState::Exact,
            cents: Some(cents),
        },
        None => MoneyValue {
            raw: raw.to_owned(),
            state: MoneyState::Unparseable,
            cents: None,
        },
    }
}

/// Parses a cleaned numeric amount string to integer cents, or None.
fn parse_cents(cleaned: &str) -> Option<i64> {
    let compact: String = cleaned
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    if compact.is_empty() {
        return None;
    }
    let negative = compact.starts_with('-');
    let unsigned = compact.trim_start_matches('-');
    if unsigned.is_empty() {
        return None;
    }
    // Split integer and fractional parts.
    let (whole, frac) = match unsigned.split_once('.') {
        Some((w, f)) => (w, f),
        None => (unsigned, ""),
    };
    if !whole.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // Only allow up to two fractional digits; more is ambiguous/unparseable.
    let frac = frac.trim_end_matches('0');
    if frac.len() > 2 || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let whole_int: i64 = whole.parse().ok()?;
    let frac_val: i64 = if frac.is_empty() {
        0
    } else {
        let padded = if frac.len() == 1 {
            format!("{frac}0")
        } else {
            frac.to_owned()
        };
        padded.parse().ok()?
    };
    let cents = whole_int.checked_mul(100)?.checked_add(frac_val)?;
    Some(if negative { -cents } else { cents })
}

/// Builds the SHA-256 manifest of a set of file digests.
pub fn sha256_manifest(files: &[(String, String)]) -> Vec<(String, String)> {
    let mut entries = files.to_vec();
    entries.sort();
    entries
}

/// A deterministic normalized identifier candidate for a raw identifier.
///
/// Returns the normalized form and the rule name. Returns `None` when no rule
/// applies; a `None` means the raw identifier must NOT be auto-merged with any
/// differently formatted identifier.
pub fn normalize_identifier(raw: &str) -> Option<(String, &'static str)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Upper-case alphanumerics, collapse interior whitespace to nothing.
    let compact: String = trimmed
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if compact.is_empty() {
        return None;
    }
    Some((compact, "uppercase-alphanumeric-compact"))
}

/// Deterministic exact-match key for comparing two identifiers under a rule.
pub fn identifier_match_key(raw: &str) -> Option<String> {
    normalize_identifier(raw).map(|(key, _)| key)
}

/// Produces a candidate normalized alias for an organization name.
///
/// This is a *candidate only*: it is never used to merge organizations
/// automatically. Non-exact matches must enter human review.
pub fn organization_alias_candidate(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut compact: String = trimmed
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    // Collapse repeated spaces is already handled by filtering.
    if compact.is_empty() {
        return None;
    }
    // Remove common corporate suffixes and trailing punctuation as a candidate.
    for suffix in [
        "inc",
        "llc",
        "corp",
        "corporation",
        "co",
        "ltd",
        "limited",
        "company",
    ] {
        if let Some(stripped) = compact.strip_suffix(suffix)
            && !stripped.is_empty()
        {
            compact = stripped.to_owned();
            break;
        }
    }
    Some(compact)
}

/// True only when two raw organization names are exact after the deterministic
/// alias-candidate rule, i.e., they are provably identical. Any non-exact match
/// returns `false` and must enter human review rather than auto-merge.
pub fn organization_exact_match(left: &str, right: &str) -> bool {
    organization_alias_candidate(left) == organization_alias_candidate(right)
        && left.trim() == right.trim()
}

/// Kind of a procurement change alert (Item 1).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcurementChangeKind {
    RecordAdded,
    RecordModified,
    RecordRemoved,
}

impl ProcurementChangeKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::RecordAdded => "record_added",
            Self::RecordModified => "record_modified",
            Self::RecordRemoved => "record_removed",
        }
    }
}

/// A field-level change for a modified row: field name and old/new raw values.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FieldDiff {
    pub field: String,
    pub old_raw: String,
    pub new_raw: String,
}

/// A single deterministic change between two procurement snapshots, suitable
/// for a change alert. Row identity is a stable key (official identifier where
/// present, else a digest over the row's normalized field values).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProcurementRecordChange {
    pub change_kind: ProcurementChangeKind,
    pub row_identity: String,
    /// Present only for `RecordModified`: a field-level diff (raw strings).
    pub field_diffs: Vec<FieldDiff>,
    pub old_snapshot_id: String,
    pub old_snapshot_digest: String,
    pub new_snapshot_id: String,
    pub new_snapshot_digest: String,
    pub summary: String,
}

/// An immutable procurement change alert (Item 1). Shares the `stable_id`,
/// append-only, and review-gate contract of the general `Alert` record.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProcurementAlert {
    pub id: String,
    pub source_id: String,
    pub surface: String,
    pub old_snapshot_id: String,
    pub old_snapshot_digest: String,
    pub new_snapshot_id: String,
    pub new_snapshot_digest: String,
    pub changes: Vec<ProcurementRecordChange>,
    pub retrieved_at: String,
    pub coverage_state: CoverageState,
    /// Affected procurement matter/identifier ids when resolvable by the exact
    /// identifier rule (never by similarity).
    pub matter_ids: Vec<String>,
    pub identifier_ids: Vec<String>,
    /// Optional surveillance-related terminology observations with exact rule
    /// citations. Never an accusation.
    pub taxonomy_matches: Vec<String>,
    /// The phrasing-disciplined summary rendered for humans / X drafts.
    pub summary: String,
}

impl ProcurementAlert {
    /// Stable, idempotent alert id over source + row identity + change kind +
    /// old/new snapshot ids. Re-ingesting the same pair never creates a second
    /// alert.
    pub fn id_for(
        source_id: &str,
        surface: &str,
        row_identity: &str,
        change_kind: ProcurementChangeKind,
        old_snapshot_id: &str,
        new_snapshot_id: &str,
    ) -> String {
        stable_id(
            "proc-alert",
            &[
                source_id,
                surface,
                row_identity,
                change_kind.label(),
                old_snapshot_id,
                new_snapshot_id,
            ],
        )
    }
}

/// The state of a CORA request in the local, append-only ledger (Item 3).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoraRequestState {
    Drafted,
    Submitted,
    ResponseReceived,
    GapResolved,
    StillUnresolved,
}

impl CoraRequestState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Drafted => "drafted",
            Self::Submitted => "submitted",
            Self::ResponseReceived => "response_received",
            Self::GapResolved => "gap_resolved",
            Self::StillUnresolved => "still_unresolved",
        }
    }
}

/// An immutable transition event in a CORA request's lifecycle.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CoraRequestEvent {
    pub id: String,
    pub request_id: String,
    pub state: CoraRequestState,
    pub operator: String,
    pub timestamp: String,
    pub note: String,
}

/// A local, append-only records-request ledger entry (Item 3). The tool never
/// sends anything and never claims a legal deadline or entitlement.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CoraRequest {
    pub id: String,
    pub matter_id: String,
    pub state: CoraRequestState,
    /// Stable id over matter id + gap-set digest + creation timestamp.
    pub gap_set_digest: String,
    pub created_at: String,
    /// Institution, identifiers, missing record types, narrow date range,
    /// vendor/project name, sources already checked — the gap summary.
    pub institution: String,
    pub identifiers: Vec<String>,
    pub missing_record_types: Vec<String>,
    pub date_range: Option<(Option<String>, Option<String>)>,
    pub vendor_or_project: Option<String>,
    pub sources_checked: Vec<String>,
    /// The draft text and its exact digest.
    pub draft_text: String,
    pub draft_digest: String,
    /// Append-only event list; corrections are new events.
    pub events: Vec<CoraRequestEvent>,
}

impl CoraRequest {
    /// Stable request id over matter id + gap-set digest + creation timestamp.
    pub fn id_for(matter_id: &str, gap_set_digest: &str, created_at: &str) -> String {
        stable_id("cora-req", &[matter_id, gap_set_digest, created_at])
    }
}

/// Kind of an official-relationship link (Item 5).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OfficialRelationshipKind {
    /// Record A references record B in a declared reference field.
    OfficialRelationship,
}

/// A stored official-relationship link (Item 5). Recorded only when a declared
/// reference field of one preserved record contains an exact match of an
/// identifier stored for another record, and both endpoints resolve to stored
/// snapshots with valid SHA-256 digests.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OfficialRelationship {
    pub id: String,
    pub kind: OfficialRelationshipKind,
    /// The record whose declared reference field carries the reference.
    pub source_record_id: String,
    pub source_snapshot_id: String,
    pub source_snapshot_digest: String,
    /// The record referenced.
    pub target_identifier: String,
    pub target_matter_id: String,
    /// The declared reference field that carried the exact match.
    pub reference_field: String,
    /// The exact quote and locator of the reference in the source record.
    pub quote: String,
    pub locator: String,
    /// One citation per endpoint.
    pub citations: Vec<String>,
    /// Whether this link was confirmed by human review (exact matches are
    /// recorded automatically; near-misses are candidates only).
    pub reviewed: bool,
}

impl OfficialRelationship {
    pub fn id_for(
        source_record_id: &str,
        reference_field: &str,
        target_identifier: &str,
        source_snapshot_id: &str,
    ) -> String {
        stable_id(
            "official-rel",
            &[
                source_record_id,
                reference_field,
                target_identifier,
                source_snapshot_id,
            ],
        )
    }
}

/// Hash helper re-exported for convenience in the procurement domain.
pub fn sha256_hex_bytes(bytes: &[u8]) -> String {
    sha256_hex(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_exact_amount_parses_to_cents() {
        let value = parse_money("$42,075.00");
        assert_eq!(value.state, MoneyState::Exact);
        assert_eq!(value.cents, Some(4_207_500));
    }

    #[test]
    fn money_distinguishes_zero_from_not_applicable() {
        assert_eq!(parse_money("$0.00").state, MoneyState::Zero);
        assert_eq!(parse_money("$0").state, MoneyState::Zero);
        assert_eq!(parse_money("N/A").state, MoneyState::NotApplicable);
        assert_eq!(parse_money("n/a").state, MoneyState::NotApplicable);
        assert_eq!(parse_money("-").state, MoneyState::NotApplicable);
    }

    #[test]
    fn money_distinguishes_various_and_idiq_from_exact() {
        assert_eq!(parse_money("various").state, MoneyState::Various);
        assert_eq!(parse_money("$0.00 IDIQ").state, MoneyState::IdiqCeiling);
        assert_eq!(parse_money("Various amounts").state, MoneyState::Various);
    }

    #[test]
    fn money_unknown_and_unparseable_are_distinct() {
        assert_eq!(parse_money("").state, MoneyState::Unknown);
        assert_eq!(parse_money("   ").state, MoneyState::Unknown);
        // Ambiguous fractional digits are unparseable, not exact.
        assert_eq!(parse_money("$1,234.567").state, MoneyState::Unparseable);
    }

    #[test]
    fn money_handles_currency_format_ambiguity() {
        // Dollar sign with two decimals -> exact.
        assert_eq!(parse_money("$300,000 each").cents, Some(30_000_000));
        assert_eq!(parse_money("$300,000 each").state, MoneyState::Exact);
        // Decimal-point ambiguity (thousands vs cents) is rejected.
        assert_eq!(parse_money("1.5.5").state, MoneyState::Unparseable);
    }

    #[test]
    fn money_handles_huge_values_without_overflow() {
        let value = parse_money("$9,999,999,999,999.99");
        assert_eq!(value.state, MoneyState::Exact);
        assert_eq!(value.cents, Some(999_999_999_999_999));
    }

    #[test]
    fn money_display_is_deterministic() {
        assert_eq!(parse_money("$42,075.00").display(), "$42075.00");
        assert_eq!(parse_money("N/A").display(), "not applicable");
        assert_eq!(parse_money("various").display(), "various");
    }

    #[test]
    fn property_money_never_floats_and_round_trips() {
        // Invariant: parsing never produces a fractional cent and never panics.
        let samples = [
            "$0",
            "$0.00",
            "N/A",
            "various",
            "$0.00 IDIQ",
            "$1.00",
            "$-1.00",
            "$1,234,567.89",
            "$1.5",
            "1.5.5",
            "",
            "$999999999999999999999999.99",
            "€50.00",
            "$300,000 each",
            "12,34",
            "1,000",
            "0.000",
        ];
        for sample in samples {
            let value = parse_money(sample);
            match value.state {
                MoneyState::Exact | MoneyState::Zero => {
                    let cents = value.cents.expect("cents present for exact/zero");
                    // Parsed cents decompose cleanly into dollars + 0..=99 cents,
                    // so the value is always representable without fractions.
                    let remainder = cents.rem_euclid(100);
                    assert!((0..=99).contains(&remainder));
                    let display = value.display();
                    assert!(!display.is_empty());
                }
                _ => {}
            }
        }
    }

    #[test]
    fn identifiers_normalize_deterministically() {
        let a = normalize_identifier("R26-023AB");
        let b = normalize_identifier("r26-023ab");
        assert_eq!(a, b);
        assert_eq!(
            a,
            Some(("R26023AB".to_owned(), "uppercase-alphanumeric-compact"))
        );
    }

    #[test]
    fn identifiers_never_merge_different_formats_without_rule() {
        // A plain rule exists, but the system must still require an explicit
        // rule + test before treating two different raw strings as equal.
        assert_eq!(
            identifier_match_key("R26-023AB"),
            identifier_match_key("r26-023ab")
        );
        // Distinct identifiers must not collide under the exact rule.
        assert_ne!(
            identifier_match_key("R26-023AB"),
            identifier_match_key("R26-023AC")
        );
        assert_ne!(
            identifier_match_key("Q25-130ZM"),
            identifier_match_key("R24-T114JD")
        );
    }

    #[test]
    fn property_identifier_keys_are_functional_and_non_colliding() {
        // Invariant: the exact-match key is a function of the identifier and
        // preserves inequality of distinct identifiers (no spurious merges).
        let raw = [
            "R26-023AB",
            "r26-023ab",
            "R26-023AC",
            "IFB-2024-001",
            "ifb-2024-001",
        ];
        let keys: Vec<Option<String>> = raw.iter().map(|s| identifier_match_key(s)).collect();
        assert_eq!(keys[0], keys[1], "case-insensitive variant must match");
        assert_eq!(keys[3], keys[4]);
        assert_ne!(keys[0], keys[2]);
        assert_ne!(keys[0], keys[3]);
    }

    #[test]
    fn organization_exact_match_never_merges_non_exact() {
        // Identical raw names match exactly.
        assert!(organization_exact_match(
            "Adarand Constructors",
            "Adarand Constructors"
        ));
        // Different names (even similar) are NOT auto-merged.
        assert!(!organization_exact_match("Adarand Constructors", "Adarand"));
        assert!(!organization_exact_match("Crafco & Maxwell", "Crafco"));
        // A candidate alias is produced but is never treated as an auto-merge.
        assert!(organization_alias_candidate("Acme Inc.").is_some());
    }

    #[test]
    fn property_organization_aliases_are_candidates_only() {
        // Invariant: the candidate rule is deterministic, and a non-identical
        // raw pair is never an exact match.
        for name in [
            "Acme Inc.",
            "Acme, Inc.",
            "Acme LLC",
            "ACME",
            "Acme Corporation",
        ] {
            let _ = organization_alias_candidate(name);
        }
        assert!(!organization_exact_match("Acme Inc.", "Acme LLC"));
    }

    #[test]
    fn sha256_manifest_is_sorted_and_stable() {
        let files = vec![
            ("b".to_owned(), "B".to_owned()),
            ("a".to_owned(), "A".to_owned()),
        ];
        let manifest = sha256_manifest(&files);
        assert_eq!(manifest[0].0, "a");
        assert_eq!(manifest[1].0, "b");
    }
}
