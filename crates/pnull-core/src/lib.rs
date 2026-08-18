//! Canonical evidence, finding, alert, and durable-state primitives.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, Transaction, params, params_from_iter};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub mod migrate;
pub mod procurement;
pub mod types;

pub use migrate::{SCHEMA_VERSION, migrate};
pub use procurement::{
    CaseFile, CaseFileState, CoraDraft, CoverageEntry, CoverageState, IdentifierKind, MoneyState,
    MoneyValue, OrganizationRole, ProcurementEvent, ProcurementEventKind, ProcurementIdentifier,
    ProcurementMatter, ProcurementOrganization, RecordChange, ReconciliationDecision,
    ReconciliationItem, ReconciliationKind, SnapshotDiff, SnapshotRevision, SourceAuthority,
    SourceSnapshot, identifier_match_key, normalize_identifier, organization_alias_candidate,
    organization_exact_match, parse_money, sha256_manifest,
};
pub use types::{
    Action, ActionKind, BoundingRect, ConditionalResult, DocumentRole, FetchObservation,
    LocatorRange, MapWord, Matter, MatterAttachment, NativeTool, OutputArtifact, PageCitation,
    ProcessingRun, PublicationAllowlist, ReviewBinding, ReviewDecision, ReviewState, SourceReview,
    Subject, SubjectKind, TextMap, XAttempt, XReconciliation, XSegment,
};

pub const PROCESSING_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("state serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("record not found: {0}")]
    NotFound(String),
    #[error("invalid SHA-256 digest: {0}")]
    InvalidDigest(String),
    #[error("digest mismatch for evidence {evidence_id}: expected {expected}, observed {observed}")]
    DigestMismatch {
        evidence_id: String,
        expected: String,
        observed: String,
    },
    #[error("schema migration failed: {0}")]
    Migration(#[from] migrate::MigrationError),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    OfficialApi,
    Agenda,
    Contract,
    Amendment,
    HtmlPage,
    PlainText,
    Pdf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionStatus {
    Complete,
    CompleteWithOcr,
    Failed,
    Rejected,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Locator {
    pub kind: String,
    pub start: u32,
    pub end: u32,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Citation {
    pub evidence_id: String,
    pub source_url: String,
    pub locator: Locator,
    pub quote: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StructuredError {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EvidenceRecord {
    pub id: String,
    pub jurisdiction: String,
    pub source_url: String,
    pub source_type: SourceType,
    pub document_title: String,
    pub publication_date: Option<String>,
    pub retrieval_timestamp: String,
    pub mime_type: String,
    pub sha256: String,
    pub original_filename: String,
    pub extraction_method: String,
    pub extraction_status: ExtractionStatus,
    pub extraction_error: Option<StructuredError>,
    pub locators: Vec<Locator>,
    pub matched_rule_ids: Vec<String>,
    pub quoted_source_spans: Vec<Citation>,
    pub supersedes: Option<String>,
    pub processing_version: String,
}

impl EvidenceRecord {
    pub fn canonical_json(&self) -> Result<Vec<u8>, CoreError> {
        let mut bytes = serde_json::to_vec_pretty(self)?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingState {
    MentionDetected,
    Proposal,
    PublicHearingScheduled,
    VoteScheduled,
    Approved,
    Rejected,
    ContractExecuted,
    RenewalOrExpansion,
    DeploymentReported,
    PolicyChange,
    Unknown,
}

impl FindingState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::MentionDetected => "Mention detected",
            Self::Proposal => "Proposal",
            Self::PublicHearingScheduled => "Public hearing scheduled",
            Self::VoteScheduled => "Vote scheduled",
            Self::Approved => "Approved",
            Self::Rejected => "Rejected",
            Self::ContractExecuted => "Contract executed",
            Self::RenewalOrExpansion => "Renewal or expansion",
            Self::DeploymentReported => "Deployment reported",
            Self::PolicyChange => "Policy change",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Finding {
    pub id: String,
    pub evidence_id: String,
    pub jurisdiction: String,
    pub state: FindingState,
    pub classification_reason: String,
    pub rules_version: u32,
    pub rules_digest: String,
    pub matched_rule_ids: Vec<String>,
    pub citations: Vec<Citation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DiffChange {
    pub kind: String,
    pub summary: String,
    pub before: Option<Citation>,
    pub after: Option<Citation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EvidenceDiff {
    pub old_evidence_id: String,
    pub new_evidence_id: String,
    pub old_source_url: String,
    pub new_source_url: String,
    pub changes: Vec<DiffChange>,
    pub unified_text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Alert {
    pub id: String,
    pub jurisdiction: String,
    pub evidence_id: String,
    pub previous_evidence_id: Option<String>,
    pub title: String,
    pub state: FindingState,
    pub summary: String,
    pub publication_date: String,
    pub rule_ids: Vec<String>,
    pub rules_version: u32,
    pub rules_digest: String,
    pub citations: Vec<Citation>,
    pub diff: Option<EvidenceDiff>,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn stable_id(namespace: &str, parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(namespace.as_bytes());
    for part in parts {
        hasher.update([0]);
        hasher.update(part.as_bytes());
    }
    format!("{namespace}:{}", hex::encode(hasher.finalize()))
}

pub fn evidence_id(jurisdiction: &str, source_url: &str, digest: &str) -> String {
    stable_id("evidence", &[jurisdiction, source_url, digest])
}

pub struct Store {
    connection: Connection,
    data_dir: PathBuf,
}

impl Store {
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self, CoreError> {
        let data_dir = data_dir.as_ref().to_path_buf();
        fs::create_dir_all(&data_dir)?;
        fs::create_dir_all(data_dir.join("evidence/sha256"))?;
        fs::create_dir_all(data_dir.join("records"))?;
        set_private_directory(&data_dir)?;
        set_private_directory(&data_dir.join("evidence"))?;
        set_private_directory(&data_dir.join("records"))?;
        let database_path = data_dir.join("pnull.db");
        let mut connection = Connection::open(&database_path)?;
        set_private_file(&database_path)?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS evidence (
               id TEXT PRIMARY KEY,
               sha256 TEXT NOT NULL,
               source_url TEXT NOT NULL,
               record_json TEXT NOT NULL,
               extracted_text TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS findings (
               id TEXT PRIMARY KEY,
               evidence_id TEXT NOT NULL REFERENCES evidence(id),
               finding_json TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS alerts (
               id TEXT PRIMARY KEY,
               evidence_id TEXT NOT NULL REFERENCES evidence(id),
               alert_json TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS approvals (
               alert_id TEXT PRIMARY KEY REFERENCES alerts(id),
               draft_digest TEXT NOT NULL,
               approved_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS posts (
               alert_id TEXT PRIMARY KEY REFERENCES alerts(id),
               remote_ids_json TEXT NOT NULL,
               posted_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS source_fetches (
               source_id TEXT PRIMARY KEY,
               fetched_at_unix INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS post_segments (
               alert_id TEXT NOT NULL REFERENCES posts(alert_id),
               segment_index INTEGER NOT NULL,
               remote_id TEXT NOT NULL,
               PRIMARY KEY(alert_id, segment_index)
             );",
        )?;
        migrate(&mut connection)?;
        Ok(Self {
            connection,
            data_dir,
        })
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn content_path(&self, digest: &str) -> Result<PathBuf, CoreError> {
        if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(CoreError::InvalidDigest(digest.to_owned()));
        }
        Ok(self
            .data_dir
            .join("evidence/sha256")
            .join(&digest[..2])
            .join(digest))
    }

    fn record_path(&self, id: &str) -> Result<PathBuf, CoreError> {
        if !id.starts_with("evidence:")
            || !id.strip_prefix("evidence:").is_some_and(|digest| {
                digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        {
            return Err(CoreError::NotFound(id.to_owned()));
        }
        Ok(self
            .data_dir
            .join("records")
            .join(format!("{}.json", id.replace(':', "_"))))
    }

    pub fn insert_evidence(
        &self,
        record: &EvidenceRecord,
        extracted_text: &str,
    ) -> Result<bool, CoreError> {
        let exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM evidence WHERE id = ?1)",
            [&record.id],
            |row| row.get(0),
        )?;
        if exists {
            return Ok(false);
        }
        let canonical = record.canonical_json()?;
        let record_json = String::from_utf8(canonical.clone())
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        write_atomic(&self.record_path(&record.id)?, &canonical)?;
        self.connection.execute(
            "INSERT INTO evidence(id, sha256, source_url, record_json, extracted_text)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                record.id,
                record.sha256,
                record.source_url,
                record_json,
                extracted_text
            ],
        )?;
        Ok(true)
    }

    pub fn update_evidence_annotations(
        &self,
        record: &EvidenceRecord,
        extracted_text: &str,
    ) -> Result<(), CoreError> {
        self.evidence(&record.id)?;
        let canonical = record.canonical_json()?;
        let record_json = String::from_utf8(canonical.clone())
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        write_atomic(&self.record_path(&record.id)?, &canonical)?;
        self.connection.execute(
            "UPDATE evidence SET record_json = ?2, extracted_text = ?3 WHERE id = ?1",
            params![record.id, record_json, extracted_text],
        )?;
        Ok(())
    }

    pub fn evidence(&self, id: &str) -> Result<(EvidenceRecord, String), CoreError> {
        let row: Option<(String, String)> = self
            .connection
            .query_row(
                "SELECT record_json, extracted_text FROM evidence WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let (record_json, text) = row.ok_or_else(|| CoreError::NotFound(id.to_owned()))?;
        Ok((serde_json::from_str(&record_json)?, text))
    }

    pub fn all_evidence(&self) -> Result<Vec<(EvidenceRecord, String)>, CoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT record_json, extracted_text FROM evidence ORDER BY id")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.map(|row| {
            let (json, text) = row?;
            Ok((serde_json::from_str(&json)?, text))
        })
        .collect()
    }

    pub fn insert_finding(&self, finding: &Finding) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO findings(id, evidence_id, finding_json) VALUES (?1, ?2, ?3)",
            params![
                finding.id,
                finding.evidence_id,
                serde_json::to_string(finding)?
            ],
        )? == 1)
    }

    pub fn findings(&self) -> Result<Vec<Finding>, CoreError> {
        self.read_json_rows("SELECT finding_json FROM findings ORDER BY id", &[])
    }

    pub fn insert_alert(&self, alert: &Alert) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO alerts(id, evidence_id, alert_json) VALUES (?1, ?2, ?3)",
            params![alert.id, alert.evidence_id, serde_json::to_string(alert)?],
        )? == 1)
    }

    pub fn alert(&self, id: &str) -> Result<Alert, CoreError> {
        let json: Option<String> = self
            .connection
            .query_row("SELECT alert_json FROM alerts WHERE id = ?1", [id], |row| {
                row.get(0)
            })
            .optional()?;
        Ok(serde_json::from_str(
            &json.ok_or_else(|| CoreError::NotFound(id.to_owned()))?,
        )?)
    }

    pub fn alerts(&self) -> Result<Vec<Alert>, CoreError> {
        self.read_json_rows(
            "SELECT alert_json FROM alerts ORDER BY json_extract(alert_json, '$.publication_date') DESC, id",
            &[],
        )
    }

    fn read_json_rows<T: for<'de> Deserialize<'de>>(
        &self,
        query: &str,
        params: &[&str],
    ) -> Result<Vec<T>, CoreError> {
        let mut statement = self.connection.prepare(query)?;
        let rows = statement.query_map(params_from_iter(params.iter().copied()), |row| {
            row.get::<_, String>(0)
        })?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }

    pub fn source_fetch_allowed(
        &self,
        source_id: &str,
        minimum_interval_seconds: u64,
        now_unix: i64,
    ) -> Result<bool, CoreError> {
        let last: Option<i64> = self
            .connection
            .query_row(
                "SELECT fetched_at_unix FROM source_fetches WHERE source_id = ?1",
                [source_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(last.is_none_or(|last| {
            now_unix.saturating_sub(last)
                >= i64::try_from(minimum_interval_seconds).unwrap_or(i64::MAX)
        }))
    }

    pub fn record_source_fetch(&self, source_id: &str, now_unix: i64) -> Result<(), CoreError> {
        self.connection.execute(
            "INSERT INTO source_fetches(source_id, fetched_at_unix) VALUES (?1, ?2)
             ON CONFLICT(source_id) DO UPDATE SET fetched_at_unix = excluded.fetched_at_unix",
            params![source_id, now_unix],
        )?;
        Ok(())
    }

    pub fn approve(
        &self,
        alert_id: &str,
        draft_digest: &str,
        approved_at: &str,
    ) -> Result<bool, CoreError> {
        self.alert(alert_id)?;
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO approvals(alert_id, draft_digest, approved_at) VALUES (?1, ?2, ?3)",
            params![alert_id, draft_digest, approved_at],
        )? == 1)
    }

    pub fn approved_draft_digest(&self, alert_id: &str) -> Result<Option<String>, CoreError> {
        Ok(self
            .connection
            .query_row(
                "SELECT draft_digest FROM approvals WHERE alert_id = ?1",
                [alert_id],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn is_posted(&self, alert_id: &str) -> Result<bool, CoreError> {
        Ok(self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM posts WHERE alert_id = ?1)",
            [alert_id],
            |row| row.get(0),
        )?)
    }

    pub fn reserve_post(&self, alert_id: &str) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO posts(alert_id, remote_ids_json, posted_at) VALUES (?1, '[]', 'IN_PROGRESS')",
            [alert_id],
        )? == 1)
    }

    pub fn record_post_segment(
        &self,
        alert_id: &str,
        segment_index: usize,
        remote_id: &str,
    ) -> Result<(), CoreError> {
        self.connection.execute(
            "INSERT INTO post_segments(alert_id, segment_index, remote_id) VALUES (?1, ?2, ?3)",
            params![
                alert_id,
                i64::try_from(segment_index).unwrap_or(i64::MAX),
                remote_id
            ],
        )?;
        Ok(())
    }

    pub fn post_segments(&self, alert_id: &str) -> Result<Vec<String>, CoreError> {
        let mut statement = self.connection.prepare(
            "SELECT remote_id FROM post_segments WHERE alert_id = ?1 ORDER BY segment_index",
        )?;
        let rows = statement.query_map([alert_id], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(CoreError::from)
    }

    pub fn mark_posted(
        &self,
        alert_id: &str,
        remote_ids: &[String],
        posted_at: &str,
    ) -> Result<(), CoreError> {
        self.connection.execute(
            "UPDATE posts SET remote_ids_json = ?2, posted_at = ?3 WHERE alert_id = ?1",
            params![alert_id, serde_json::to_string(remote_ids)?, posted_at],
        )?;
        Ok(())
    }

    pub fn verify(&self, evidence_id: &str) -> Result<(), CoreError> {
        let (record, _) = self.evidence(evidence_id)?;
        let bytes = fs::read(self.content_path(&record.sha256)?)?;
        let observed = sha256_hex(&bytes);
        if observed == record.sha256 {
            Ok(())
        } else {
            Err(CoreError::DigestMismatch {
                evidence_id: evidence_id.to_owned(),
                expected: record.sha256,
                observed,
            })
        }
    }

    pub fn schema_version(&self) -> Result<u32, CoreError> {
        Ok(migrate::current_version(&self.connection)?)
    }

    pub fn insert_matter(&self, matter: &Matter) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO matters(id, source_id, official_matter_id, matter_json)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                matter.id,
                matter.source_id,
                matter.official_matter_id,
                serde_json::to_string(matter)?
            ],
        )? == 1)
    }

    pub fn matter(&self, id: &str) -> Result<Matter, CoreError> {
        self.read_json_row("SELECT matter_json FROM matters WHERE id = ?1", &[id])
    }

    pub fn matters(&self) -> Result<Vec<Matter>, CoreError> {
        self.read_json_rows(
            "SELECT matter_json FROM matters ORDER BY official_matter_id",
            &[],
        )
    }

    pub fn insert_attachment(&self, attachment: &MatterAttachment) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO matter_attachments(id, matter_id, attachment_json)
             VALUES (?1, ?2, ?3)",
            params![
                attachment.id,
                attachment.matter_id,
                serde_json::to_string(attachment)?
            ],
        )? == 1)
    }

    pub fn attachments(&self, matter_id: &str) -> Result<Vec<MatterAttachment>, CoreError> {
        self.read_json_rows(
            "SELECT attachment_json FROM matter_attachments WHERE matter_id = ?1 ORDER BY json_extract(attachment_json, '$.name')",
            &[matter_id],
        )
    }

    pub fn insert_subject(&self, subject: &Subject) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO subjects(id, matter_id, subject_json) VALUES (?1, ?2, ?3)",
            params![
                subject.id,
                subject.matter_id,
                serde_json::to_string(subject)?
            ],
        )? == 1)
    }

    pub fn subjects(&self, matter_id: &str) -> Result<Vec<Subject>, CoreError> {
        self.read_json_rows(
            "SELECT subject_json FROM subjects WHERE matter_id = ?1 ORDER BY json_extract(subject_json, '$.kind'), json_extract(subject_json, '$.name')",
            &[matter_id],
        )
    }

    pub fn insert_action(&self, action: &Action) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO actions(id, matter_id, subject_id, action_json)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                action.id,
                action.matter_id,
                action.subject_id,
                serde_json::to_string(action)?
            ],
        )? == 1)
    }

    pub fn actions(&self, matter_id: &str) -> Result<Vec<Action>, CoreError> {
        self.read_json_rows(
            "SELECT action_json FROM actions WHERE matter_id = ?1 ORDER BY id",
            &[matter_id],
        )
    }

    pub fn insert_text_map(&self, map: &TextMap) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO text_maps(id, evidence_id, text_map_json) VALUES (?1, ?2, ?3)",
            params![map.id, map.evidence_id, serde_json::to_string(map)?],
        )? == 1)
    }

    pub fn text_maps(&self, evidence_id: &str) -> Result<Vec<TextMap>, CoreError> {
        self.read_json_rows(
            "SELECT text_map_json FROM text_maps WHERE evidence_id = ?1 ORDER BY json_extract(text_map_json, '$.page_number')",
            &[evidence_id],
        )
    }

    pub fn text_map(&self, id: &str) -> Result<TextMap, CoreError> {
        self.read_json_row("SELECT text_map_json FROM text_maps WHERE id = ?1", &[id])
    }

    pub fn insert_page_citation(&self, citation: &PageCitation) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO page_citations(id, evidence_id, page_citation_json)
             VALUES (?1, ?2, ?3)",
            params![
                citation.id,
                citation.evidence_id,
                serde_json::to_string(citation)?
            ],
        )? == 1)
    }

    pub fn page_citations(&self, evidence_id: &str) -> Result<Vec<PageCitation>, CoreError> {
        self.read_json_rows(
            "SELECT page_citation_json FROM page_citations WHERE evidence_id = ?1 ORDER BY json_extract(page_citation_json, '$.page_number'), id",
            &[evidence_id],
        )
    }

    pub fn page_citation(&self, id: &str) -> Result<PageCitation, CoreError> {
        self.read_json_row(
            "SELECT page_citation_json FROM page_citations WHERE id = ?1",
            &[id],
        )
    }

    pub fn insert_review(&self, decision: &ReviewDecision) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO review_decisions(id, citation_id, decision_json)
             VALUES (?1, ?2, ?3)",
            params![
                decision.id,
                decision.citation_id,
                serde_json::to_string(decision)?
            ],
        )? == 1)
    }

    pub fn reviews_for_citation(
        &self,
        citation_id: &str,
    ) -> Result<Vec<ReviewDecision>, CoreError> {
        self.read_json_rows(
            "SELECT decision_json FROM review_decisions WHERE citation_id = ?1 ORDER BY id",
            &[citation_id],
        )
    }

    pub fn all_reviews(&self) -> Result<Vec<ReviewDecision>, CoreError> {
        self.read_json_rows(
            "SELECT decision_json FROM review_decisions ORDER BY id",
            &[],
        )
    }

    pub fn current_review(&self, citation_id: &str) -> Result<Option<ReviewDecision>, CoreError> {
        let reviews = self.reviews_for_citation(citation_id)?;
        Ok(reviews.into_iter().last())
    }

    pub fn insert_processing_run(&self, run: &ProcessingRun) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO processing_runs(id, run_json) VALUES (?1, ?2)",
            params![run.id, serde_json::to_string(run)?],
        )? == 1)
    }

    pub fn processing_runs(&self) -> Result<Vec<ProcessingRun>, CoreError> {
        self.read_json_rows(
            "SELECT run_json FROM processing_runs ORDER BY json_extract(run_json, '$.started_at')",
            &[],
        )
    }

    pub fn insert_source_review(&self, review: &SourceReview) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO source_reviews(id, source_id, review_json) VALUES (?1, ?2, ?3)",
            params![review.id, review.source_id, serde_json::to_string(review)?],
        )? == 1)
    }

    pub fn source_reviews(&self, source_id: &str) -> Result<Vec<SourceReview>, CoreError> {
        self.read_json_rows(
            "SELECT review_json FROM source_reviews WHERE source_id = ?1 ORDER BY json_extract(review_json, '$.reviewed_at')",
            &[source_id],
        )
    }

    pub fn current_source_review(
        &self,
        source_id: &str,
    ) -> Result<Option<SourceReview>, CoreError> {
        Ok(self.source_reviews(source_id)?.into_iter().last())
    }

    pub fn insert_fetch_observation(
        &self,
        observation: &FetchObservation,
    ) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO fetch_observations(id, source_id, observation_json)
             VALUES (?1, ?2, ?3)",
            params![
                observation.id,
                observation.source_id,
                serde_json::to_string(observation)?
            ],
        )? == 1)
    }

    pub fn fetch_observations(&self, source_id: &str) -> Result<Vec<FetchObservation>, CoreError> {
        self.read_json_rows(
            "SELECT observation_json FROM fetch_observations WHERE source_id = ?1 ORDER BY json_extract(observation_json, '$.retrieved_at')",
            &[source_id],
        )
    }

    pub fn insert_x_attempt(&self, attempt: &XAttempt) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO x_attempts(id, alert_id, attempt_json) VALUES (?1, ?2, ?3)",
            params![
                attempt.id,
                attempt.alert_id,
                serde_json::to_string(attempt)?
            ],
        )? == 1)
    }

    pub fn x_attempt(&self, id: &str) -> Result<XAttempt, CoreError> {
        self.read_json_row("SELECT attempt_json FROM x_attempts WHERE id = ?1", &[id])
    }

    pub fn x_attempts(&self) -> Result<Vec<XAttempt>, CoreError> {
        self.read_json_rows(
            "SELECT attempt_json FROM x_attempts ORDER BY json_extract(attempt_json, '$.started_at')",
            &[],
        )
    }

    pub fn x_attempts_for_alert(&self, alert_id: &str) -> Result<Vec<XAttempt>, CoreError> {
        self.read_json_rows(
            "SELECT attempt_json FROM x_attempts WHERE alert_id = ?1 ORDER BY json_extract(attempt_json, '$.started_at')",
            &[alert_id],
        )
    }

    pub fn insert_x_reconciliation(&self, item: &XReconciliation) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO x_reconciliations(id, attempt_id, reconciliation_json)
             VALUES (?1, ?2, ?3)",
            params![item.id, item.attempt_id, serde_json::to_string(item)?],
        )? == 1)
    }

    pub fn x_reconciliations(&self, attempt_id: &str) -> Result<Vec<XReconciliation>, CoreError> {
        self.read_json_rows(
            "SELECT reconciliation_json FROM x_reconciliations WHERE attempt_id = ?1 ORDER BY json_extract(reconciliation_json, '$.decided_at')",
            &[attempt_id],
        )
    }

    pub fn insert_publication_allowlist(
        &self,
        item: &PublicationAllowlist,
    ) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO publication_allowlists(id, allowlist_json) VALUES (?1, ?2)",
            params![item.id, serde_json::to_string(item)?],
        )? == 1)
    }

    pub fn publication_allowlists(&self) -> Result<Vec<PublicationAllowlist>, CoreError> {
        self.read_json_rows(
            "SELECT allowlist_json FROM publication_allowlists ORDER BY id",
            &[],
        )
    }

    fn read_json_row<T: for<'de> Deserialize<'de>>(
        &self,
        query: &str,
        params: &[&str],
    ) -> Result<T, CoreError> {
        let json: Option<String> = self
            .connection
            .query_row(query, params_from_iter(params.iter().copied()), |row| {
                row.get(0)
            })
            .optional()?;
        Ok(serde_json::from_str(&json.ok_or_else(|| {
            CoreError::NotFound("record".to_owned())
        })?)?)
    }
}

impl Store {
    /// Inserts a procurement matter, ignoring duplicates.
    pub fn insert_procurement_matter(&self, matter: &ProcurementMatter) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO procurement_matters(id, official_title, matter_json) VALUES (?1, ?2, ?3)",
            params![matter.id, matter.title, serde_json::to_string(matter)?],
        )? == 1)
    }

    pub fn procurement_matter(&self, id: &str) -> Result<ProcurementMatter, CoreError> {
        self.read_json_row(
            "SELECT matter_json FROM procurement_matters WHERE id = ?1",
            &[id],
        )
    }

    pub fn procurement_matters(&self) -> Result<Vec<ProcurementMatter>, CoreError> {
        self.read_json_rows(
            "SELECT matter_json FROM procurement_matters ORDER BY title",
            &[],
        )
    }

    pub fn insert_procurement_event(&self, event: &ProcurementEvent) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO procurement_events(id, matter_id, event_json) VALUES (?1, ?2, ?3)",
            params![event.id, event.matter_id, serde_json::to_string(event)?],
        )? == 1)
    }

    pub fn procurement_events(&self, matter_id: &str) -> Result<Vec<ProcurementEvent>, CoreError> {
        self.read_json_rows(
            "SELECT event_json FROM procurement_events WHERE matter_id = ?1 ORDER BY json_extract(event_json, '$.date'), id",
            &[matter_id],
        )
    }

    pub fn insert_procurement_identifier(
        &self,
        identifier: &ProcurementIdentifier,
    ) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO procurement_identifiers(id, matter_id, identifier_json) VALUES (?1, ?2, ?3)",
            params![identifier.id, identifier.matter_id, serde_json::to_string(identifier)?],
        )? == 1)
    }

    pub fn procurement_identifiers(
        &self,
        matter_id: &str,
    ) -> Result<Vec<ProcurementIdentifier>, CoreError> {
        self.read_json_rows(
            "SELECT identifier_json FROM procurement_identifiers WHERE matter_id = ?1 ORDER BY json_extract(identifier_json, '$.kind'), id",
            &[matter_id],
        )
    }

    pub fn insert_procurement_organization(
        &self,
        org: &ProcurementOrganization,
    ) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO procurement_organizations(id, matter_id, organization_json) VALUES (?1, ?2, ?3)",
            params![org.id, org.matter_id, serde_json::to_string(org)?],
        )? == 1)
    }

    pub fn procurement_organizations(
        &self,
        matter_id: &str,
    ) -> Result<Vec<ProcurementOrganization>, CoreError> {
        self.read_json_rows(
            "SELECT organization_json FROM procurement_organizations WHERE matter_id = ?1 ORDER BY json_extract(organization_json, '$.role'), id",
            &[matter_id],
        )
    }

    pub fn insert_coverage_entry(&self, entry: &CoverageEntry) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO coverage_ledger(id, source_id, entry_json) VALUES (?1, ?2, ?3)",
            params![entry.id, entry.source_id, serde_json::to_string(entry)?],
        )? == 1)
    }

    pub fn coverage_entries(&self, source_id: &str) -> Result<Vec<CoverageEntry>, CoreError> {
        self.read_json_rows(
            "SELECT entry_json FROM coverage_ledger WHERE source_id = ?1 ORDER BY json_extract(entry_json, '$.retrieved_at')",
            &[source_id],
        )
    }

    pub fn all_coverage_entries(&self) -> Result<Vec<CoverageEntry>, CoreError> {
        self.read_json_rows(
            "SELECT entry_json FROM coverage_ledger ORDER BY json_extract(entry_json, '$.retrieved_at'), source_id",
            &[],
        )
    }

    pub fn insert_source_snapshot(&self, snapshot: &SourceSnapshot) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO source_snapshots(id, source_id, snapshot_json) VALUES (?1, ?2, ?3)",
            params![snapshot.id, snapshot.source_id, serde_json::to_string(snapshot)?],
        )? == 1)
    }

    pub fn source_snapshot(&self, id: &str) -> Result<SourceSnapshot, CoreError> {
        self.read_json_row(
            "SELECT snapshot_json FROM source_snapshots WHERE id = ?1",
            &[id],
        )
    }

    pub fn source_snapshots(&self, source_id: &str) -> Result<Vec<SourceSnapshot>, CoreError> {
        self.read_json_rows(
            "SELECT snapshot_json FROM source_snapshots WHERE source_id = ?1 ORDER BY json_extract(snapshot_json, '$.retrieved_at')",
            &[source_id],
        )
    }

    pub fn insert_snapshot_revision(&self, revision: &SnapshotRevision) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO snapshot_revisions(id, snapshot_id, revision_json) VALUES (?1, ?2, ?3)",
            params![revision.id, revision.snapshot_id, serde_json::to_string(revision)?],
        )? == 1)
    }

    pub fn snapshot_revisions(&self, snapshot_id: &str) -> Result<Vec<SnapshotRevision>, CoreError> {
        self.read_json_rows(
            "SELECT revision_json FROM snapshot_revisions WHERE snapshot_id = ?1 ORDER BY json_extract(revision_json, '$.recorded_at')",
            &[snapshot_id],
        )
    }

    pub fn insert_snapshot_diff(&self, diff: &SnapshotDiff) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO snapshot_diffs(id, old_snapshot_id, new_snapshot_id, diff_json) VALUES (?1, ?2, ?3, ?4)",
            params![diff.id, diff.old_snapshot_id, diff.new_snapshot_id, serde_json::to_string(diff)?],
        )? == 1)
    }

    pub fn snapshot_diff(&self, old: &str, new: &str) -> Result<Option<SnapshotDiff>, CoreError> {
        let json: Option<String> = self
            .connection
            .query_row(
                "SELECT diff_json FROM snapshot_diffs WHERE old_snapshot_id = ?1 AND new_snapshot_id = ?2",
                params![old, new],
                |row| row.get(0),
            )
            .optional()?;
        json.map(|j| serde_json::from_str(&j)).transpose().map_err(CoreError::from)
    }

    pub fn insert_reconciliation_item(&self, item: &ReconciliationItem) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO reconciliation_items(id, matter_id, item_json) VALUES (?1, ?2, ?3)",
            params![item.id, item.matter_id, serde_json::to_string(item)?],
        )? == 1)
    }

    pub fn reconciliation_items(
        &self,
        matter_id: &str,
    ) -> Result<Vec<ReconciliationItem>, CoreError> {
        self.read_json_rows(
            "SELECT item_json FROM reconciliation_items WHERE matter_id = ?1 ORDER BY json_extract(item_json, '$.created_at'), id",
            &[matter_id],
        )
    }

    pub fn all_reconciliation_items(&self) -> Result<Vec<ReconciliationItem>, CoreError> {
        self.read_json_rows(
            "SELECT item_json FROM reconciliation_items ORDER BY json_extract(item_json, '$.created_at'), matter_id",
            &[],
        )
    }

    pub fn insert_reconciliation_decision(
        &self,
        decision: &ReconciliationDecision,
    ) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO reconciliation_decisions(id, item_id, decision_json) VALUES (?1, ?2, ?3)",
            params![decision.id, decision.item_id, serde_json::to_string(decision)?],
        )? == 1)
    }

    pub fn reconciliation_decisions(
        &self,
        item_id: &str,
    ) -> Result<Vec<ReconciliationDecision>, CoreError> {
        self.read_json_rows(
            "SELECT decision_json FROM reconciliation_decisions WHERE item_id = ?1 ORDER BY json_extract(decision_json, '$.decided_at')",
            &[item_id],
        )
    }

    pub fn current_reconciliation_decision(
        &self,
        item_id: &str,
    ) -> Result<Option<ReconciliationDecision>, CoreError> {
        Ok(self.reconciliation_decisions(item_id)?.into_iter().last())
    }

    pub fn insert_case_file(&self, case_file: &CaseFile) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO case_files(id, matter_id, case_file_json) VALUES (?1, ?2, ?3)",
            params![case_file.id, case_file.matter_id, serde_json::to_string(case_file)?],
        )? == 1)
    }

    pub fn case_files(&self, matter_id: &str) -> Result<Vec<CaseFile>, CoreError> {
        self.read_json_rows(
            "SELECT case_file_json FROM case_files WHERE matter_id = ?1 ORDER BY json_extract(case_file_json, '$.built_at')",
            &[matter_id],
        )
    }

    pub fn insert_cora_draft(&self, draft: &CoraDraft) -> Result<bool, CoreError> {
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO cora_drafts(id, matter_id, draft_json) VALUES (?1, ?2, ?3)",
            params![draft.id, draft.matter_id, serde_json::to_string(draft)?],
        )? == 1)
    }

    pub fn cora_drafts(&self, matter_id: &str) -> Result<Vec<CoraDraft>, CoreError> {
        self.read_json_rows(
            "SELECT draft_json FROM cora_drafts WHERE matter_id = ?1 ORDER BY json_extract(draft_json, '$.created_at')",
            &[matter_id],
        )
    }
}

impl Store {
    /// Runs a transactional closure over the underlying connection.
    pub fn transaction<T>(
        &self,
        f: impl FnOnce(&Transaction<'_>) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        let result = f(&transaction)?;
        transaction.commit()?;
        Ok(result)
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("record path has no parent"))?;
    fs::create_dir_all(parent)?;
    let mut temporary = path.to_path_buf();
    temporary.set_extension(format!("tmp-{}", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    set_private_file(&temporary)?;
    fs::rename(&temporary, path)?;
    set_private_file(path)?;
    Ok(())
}

#[cfg(unix)]
fn set_private_directory(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn set_private_directory(_path: &Path) -> Result<(), std::io::Error> {
    Ok(())
}

#[cfg(unix)]
fn set_private_file(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private_file(_path: &Path) -> Result<(), std::io::Error> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_deterministic_and_domain_separated() {
        let first = evidence_id("Colorado Springs", "https://example.test/a", "abc");
        let second = evidence_id("Colorado Springs", "https://example.test/a", "abc");
        assert_eq!(first, second);
        assert_ne!(
            first,
            stable_id(
                "alert",
                &["Colorado Springs", "https://example.test/a", "abc"]
            )
        );
    }

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn json_backed_list_queries_round_trip_and_are_ordered() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = Store::open(dir.path()).expect("store");

        // Subjects and attachments are stored as JSON; the read-back queries
        // order by fields embedded in that JSON, so a bare-column ORDER BY
        // would fail. This test guards those queries.
        store
            .insert_matter(&Matter {
                id: "matter:1".to_owned(),
                source_id: "co".to_owned(),
                official_matter_id: "25-1".to_owned(),
                title: "Test matter".to_owned(),
                status: "passed".to_owned(),
                url: "https://example.test/m".to_owned(),
                document_role: DocumentRole::Ordinance,
            })
            .expect("insert matter");
        store
            .insert_subject(&Subject {
                id: Subject::id_for("matter:1", "b"),
                matter_id: "matter:1".to_owned(),
                kind: SubjectKind::Ordinance,
                name: "b".to_owned(),
                detail: String::new(),
                citations: Vec::new(),
                known: true,
            })
            .expect("insert subject b");
        store
            .insert_subject(&Subject {
                id: Subject::id_for("matter:1", "a"),
                matter_id: "matter:1".to_owned(),
                kind: SubjectKind::Policy,
                name: "a".to_owned(),
                detail: String::new(),
                citations: Vec::new(),
                known: true,
            })
            .expect("insert subject a");
        let subjects = store.subjects("matter:1").expect("subjects query");
        assert_eq!(subjects.len(), 2);
        // Ordered by kind ("Ordinance" < "Policy") then name.
        assert_eq!(subjects[0].name, "b");
        assert_eq!(subjects[1].name, "a");

        store
            .insert_attachment(&MatterAttachment {
                id: "attachment:1".to_owned(),
                matter_id: "matter:1".to_owned(),
                official_id: "o1".to_owned(),
                name: "b.pdf".to_owned(),
                url: "https://example.test/b.pdf".to_owned(),
                evidence_id: None,
            })
            .expect("insert attachment");
        let attachments = store.attachments("matter:1").expect("attachments query");
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].name, "b.pdf");

        store
            .insert_processing_run(&ProcessingRun {
                id: "run:1".to_owned(),
                schema_version: 1,
                pnull_version: "0.0.2".to_owned(),
                source_revision: "rev".to_owned(),
                rules_digest: "rules".to_owned(),
                state_config_digest: "cfg".to_owned(),
                input_evidence_ids: Vec::new(),
                native_tools: Vec::new(),
                sandbox_backend: "bubblewrap".to_owned(),
                sandbox_version: "0.9.0".to_owned(),
                resource_budgets: serde_json::json!({}),
                resource_consumption: serde_json::json!({}),
                started_at: "2026-08-16T00:00:00Z".to_owned(),
                completed_at: "2026-08-16T00:00:01Z".to_owned(),
                outcome: "complete".to_owned(),
                errors: Vec::new(),
                output_artifacts: Vec::new(),
            })
            .expect("insert processing run");
        let runs = store.processing_runs().expect("processing runs query");
        assert_eq!(runs.len(), 1);

        store
            .insert_source_review(&SourceReview {
                id: "source-review:1".to_owned(),
                source_id: "co".to_owned(),
                source_config_digest: "cfg".to_owned(),
                reviewed_hosts: vec!["example.test".to_owned()],
                endpoint_patterns: Vec::new(),
                robots_url: "https://example.test/robots.txt".to_owned(),
                robots_snapshot_digest: String::new(),
                robots_provenance: None,
                terms_urls: Vec::new(),
                terms_snapshot_digests: Vec::new(),
                reviewer: "operator".to_owned(),
                note: "demo".to_owned(),
                reviewed_at: "2026-08-16T00:00:00Z".to_owned(),
                expires_at: "2026-08-17T00:00:00Z".to_owned(),
                minimum_interval_seconds: 86400,
                restrictions: Vec::new(),
                supersedes: None,
            })
            .expect("insert source review");
        let reviews = store.source_reviews("co").expect("source reviews query");
        assert_eq!(reviews.len(), 1);

        store
            .insert_fetch_observation(&FetchObservation {
                id: "fetch:1".to_owned(),
                source_id: Some("co".to_owned()),
                requested_url: "https://example.test/robots.txt".to_owned(),
                resolved_ips: Vec::new(),
                retrieved_at: "2026-08-16T00:00:00Z".to_owned(),
                method: "GET".to_owned(),
                status_code: 200,
                redirect_target: None,
                final_url: "https://example.test/robots.txt".to_owned(),
                allowlisted_headers: Vec::new(),
                content_type: None,
                content_length: None,
                etag: None,
                last_modified: None,
                body_digest: None,
                error: None,
            })
            .expect("insert fetch observation");
        let observations = store
            .fetch_observations("co")
            .expect("fetch observations query");
        assert_eq!(observations.len(), 1);
    }
}
