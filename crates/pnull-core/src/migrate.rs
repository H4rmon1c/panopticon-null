//! `SQLite` schema versioning and transactional migrations.
//!
//! v0.0.1 databases carry no `user_version` (treated as 0). v0.0.2 introduces
//! `user_version = 1` and adds supplemental tables without rewriting canonical
//! v0.0.1 evidence, findings, alerts, approvals, posts, or source-fetch history.

use rusqlite::{Connection, Transaction};
use thiserror::Error;

pub const SCHEMA_VERSION: u32 = 1;
/// The highest schema version this build understands.
pub const MAX_SUPPORTED_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("SQLite migration failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error(
        "data directory uses unsupported schema version {0}; this build supports up to {MAX_SUPPORTED_SCHEMA_VERSION}"
    )]
    UnsupportedVersion(u32),
}

/// Returns the current schema version of the database.
pub fn current_version(connection: &Connection) -> Result<u32, MigrationError> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    Ok(u32::try_from(version).unwrap_or(u32::MAX))
}

/// Runs the v0.0.1 -> v0.0.2 migration inside a transaction.
///
/// This is a no-op for a fresh database (which is created at the target schema).
/// For an existing v0.0.1 database it only adds supplemental tables; it never
/// rewrites canonical v0.0.1 records.
pub fn migrate(connection: &mut Connection) -> Result<(), MigrationError> {
    let version = current_version(connection)?;
    if version == SCHEMA_VERSION {
        return Ok(());
    }
    if version > MAX_SUPPORTED_SCHEMA_VERSION {
        return Err(MigrationError::UnsupportedVersion(version));
    }
    // version is 0 (v0.0.1) here.
    let transaction = connection.transaction()?;
    apply_v1(&transaction)?;
    transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    transaction.commit()?;
    Ok(())
}

fn apply_v1(transaction: &Transaction<'_>) -> Result<(), rusqlite::Error> {
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS matters (
           id TEXT PRIMARY KEY,
           source_id TEXT NOT NULL,
           official_matter_id TEXT NOT NULL,
           matter_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS matter_attachments (
           id TEXT PRIMARY KEY,
           matter_id TEXT NOT NULL REFERENCES matters(id),
           attachment_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS subjects (
           id TEXT PRIMARY KEY,
           matter_id TEXT NOT NULL,
           subject_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS actions (
           id TEXT PRIMARY KEY,
           matter_id TEXT NOT NULL,
           subject_id TEXT NOT NULL,
           action_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS text_maps (
           id TEXT PRIMARY KEY,
           evidence_id TEXT NOT NULL REFERENCES evidence(id),
           text_map_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS page_citations (
           id TEXT PRIMARY KEY,
           evidence_id TEXT NOT NULL REFERENCES evidence(id),
           page_citation_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS review_decisions (
           id TEXT PRIMARY KEY,
           citation_id TEXT NOT NULL,
           decision_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS processing_runs (
           id TEXT PRIMARY KEY,
           run_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS source_reviews (
           id TEXT PRIMARY KEY,
           source_id TEXT NOT NULL,
           review_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS fetch_observations (
           id TEXT PRIMARY KEY,
           source_id TEXT,
           observation_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS x_attempts (
           id TEXT PRIMARY KEY,
           alert_id TEXT NOT NULL REFERENCES alerts(id),
           attempt_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS x_reconciliations (
           id TEXT PRIMARY KEY,
           attempt_id TEXT NOT NULL REFERENCES x_attempts(id),
           reconciliation_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS publication_allowlists (
           id TEXT PRIMARY KEY,
           allowlist_json TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_matters_source ON matters(source_id);
         CREATE INDEX IF NOT EXISTS idx_actions_matter ON actions(matter_id);
         CREATE INDEX IF NOT EXISTS idx_reviews_citation ON review_decisions(citation_id);
         CREATE INDEX IF NOT EXISTS idx_sr_source ON source_reviews(source_id);
         CREATE INDEX IF NOT EXISTS idx_fo_source ON fetch_observations(source_id);
         CREATE INDEX IF NOT EXISTS idx_xa_alert ON x_attempts(alert_id);",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::path::Path;
    use tempfile::tempdir;

    type EvidenceRow = (String, String, String, String, String);

    fn read_evidence(connection: &Connection) -> Vec<EvidenceRow> {
        let mut statement = connection
            .prepare("SELECT id, sha256, source_url, record_json, extracted_text FROM evidence")
            .expect("prepare");
        statement
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .expect("read evidence")
            .collect::<Result<_, _>>()
            .expect("collect evidence")
    }

    #[test]
    fn fresh_database_is_at_target_schema() {
        let dir = tempdir().expect("temp dir");
        let mut connection = Connection::open(dir.path().join("pnull.db")).expect("open");
        migrate(&mut connection).expect("migrate fresh");
        assert_eq!(
            current_version(&connection).expect("version"),
            SCHEMA_VERSION
        );
    }

    #[test]
    fn v01_database_upgrades_without_reinterpreting_records() {
        // Load the committed minimal v0.0.1 fixture database.
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/migration/v0.0.1-minimal.sql");
        let sql = std::fs::read_to_string(&fixture).expect("read v0.0.1 fixture");

        let dir = tempdir().expect("temp dir");
        let mut connection = Connection::open(dir.path().join("pnull.db")).expect("open");
        connection.execute_batch(&sql).expect("load v0.0.1 fixture");

        let before = read_evidence(&connection);

        migrate(&mut connection).expect("migrate v0.0.1");
        assert_eq!(
            current_version(&connection).expect("version"),
            SCHEMA_VERSION
        );

        // Every canonical record survives byte-for-byte unchanged.
        let after = read_evidence(&connection);
        assert_eq!(
            before, after,
            "evidence records must survive migration unchanged"
        );

        let findings: i64 = connection
            .query_row("SELECT COUNT(*) FROM findings", [], |row| row.get(0))
            .expect("findings count");
        assert_eq!(findings, 1);
        let alerts: i64 = connection
            .query_row("SELECT COUNT(*) FROM alerts", [], |row| row.get(0))
            .expect("alerts count");
        assert_eq!(alerts, 1);
        let approvals: i64 = connection
            .query_row("SELECT COUNT(*) FROM approvals", [], |row| row.get(0))
            .expect("approvals count");
        assert_eq!(approvals, 1);
        let posts: i64 = connection
            .query_row("SELECT COUNT(*) FROM posts", [], |row| row.get(0))
            .expect("posts count");
        assert_eq!(posts, 1);
        let segments: i64 = connection
            .query_row("SELECT COUNT(*) FROM post_segments", [], |row| row.get(0))
            .expect("segments count");
        assert_eq!(segments, 2);
        let fetches: i64 = connection
            .query_row("SELECT COUNT(*) FROM source_fetches", [], |row| row.get(0))
            .expect("fetches count");
        assert_eq!(fetches, 1);

        // Original content digest and evidence ID remain stable.
        let original: String = connection
            .query_row(
                "SELECT sha256 FROM evidence WHERE id='evidence:0136f043bcf653166033290ffa1522d406360e7b6345b4852af92e1739c584c3'",
                [],
                |row| row.get(0),
            )
            .expect("digest");
        assert_eq!(
            original,
            "badda12921d29bf2fc2d86b274efc9544fa339db82de830ba460eaa9c6bbd2e4"
        );
    }

    #[test]
    fn migration_is_idempotent() {
        let dir = tempdir().expect("temp dir");
        let mut connection = Connection::open(dir.path().join("pnull.db")).expect("open");
        migrate(&mut connection).expect("first migrate");
        migrate(&mut connection).expect("second migrate");
        assert_eq!(
            current_version(&connection).expect("version"),
            SCHEMA_VERSION
        );
    }

    #[test]
    fn newer_unsupported_schema_is_rejected() {
        let dir = tempdir().expect("temp dir");
        let mut connection = Connection::open(dir.path().join("pnull.db")).expect("open");
        connection
            .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .expect("set future version");
        assert!(matches!(
            migrate(&mut connection),
            Err(MigrationError::UnsupportedVersion(_))
        ));
    }
}
