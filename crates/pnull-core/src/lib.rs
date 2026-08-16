//! Canonical evidence, finding, alert, and durable-state primitives.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

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
    #[error("digest mismatch for evidence {evidence_id}: expected {expected}, observed {observed}")]
    DigestMismatch {
        evidence_id: String,
        expected: String,
        observed: String,
    },
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
        let connection = Connection::open(data_dir.join("pnull.db"))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS evidence (
               id TEXT PRIMARY KEY,
               sha256 TEXT NOT NULL,
               source_url TEXT NOT NULL,
               record_json TEXT NOT NULL,
               extracted_text TEXT NOT NULL,
               UNIQUE(source_url, sha256)
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
               approved_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS posts (
               alert_id TEXT PRIMARY KEY REFERENCES alerts(id),
               remote_ids_json TEXT NOT NULL,
               posted_at TEXT NOT NULL
             );",
        )?;
        Ok(Self {
            connection,
            data_dir,
        })
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn content_path(&self, digest: &str) -> PathBuf {
        self.data_dir
            .join("evidence/sha256")
            .join(&digest[..2])
            .join(digest)
    }

    pub fn insert_evidence(
        &self,
        record: &EvidenceRecord,
        extracted_text: &str,
    ) -> Result<bool, CoreError> {
        let record_json = String::from_utf8(record.canonical_json()?)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        let inserted = self.connection.execute(
            "INSERT OR IGNORE INTO evidence(id, sha256, source_url, record_json, extracted_text)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                record.id,
                record.sha256,
                record.source_url,
                record_json,
                extracted_text
            ],
        )? == 1;
        if inserted {
            fs::write(
                self.data_dir
                    .join("records")
                    .join(format!("{}.json", record.id.replace(':', "_"))),
                record.canonical_json()?,
            )?;
        }
        Ok(inserted)
    }

    pub fn update_evidence_annotations(
        &self,
        record: &EvidenceRecord,
        extracted_text: &str,
    ) -> Result<(), CoreError> {
        let record_json = String::from_utf8(record.canonical_json()?)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        self.connection.execute(
            "UPDATE evidence SET record_json = ?2, extracted_text = ?3 WHERE id = ?1",
            params![record.id, record_json, extracted_text],
        )?;
        fs::write(
            self.data_dir
                .join("records")
                .join(format!("{}.json", record.id.replace(':', "_"))),
            record.canonical_json()?,
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
        self.read_json_rows("SELECT finding_json FROM findings ORDER BY id")
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
        )
    }

    fn read_json_rows<T: for<'de> Deserialize<'de>>(
        &self,
        query: &str,
    ) -> Result<Vec<T>, CoreError> {
        let mut statement = self.connection.prepare(query)?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }

    pub fn approve(&self, alert_id: &str, approved_at: &str) -> Result<bool, CoreError> {
        self.alert(alert_id)?;
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO approvals(alert_id, approved_at) VALUES (?1, ?2)",
            params![alert_id, approved_at],
        )? == 1)
    }

    pub fn is_approved(&self, alert_id: &str) -> Result<bool, CoreError> {
        Ok(self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM approvals WHERE alert_id = ?1)",
            [alert_id],
            |row| row.get(0),
        )?)
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
        let bytes = fs::read(self.content_path(&record.sha256))?;
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
}
