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
    #[error("data directory uses unsupported schema version {0}; this build supports up to {MAX_SUPPORTED_SCHEMA_VERSION}")]
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
    use tempfile::tempdir;

    fn v01_schema(connection: &Connection) {
        connection
            .execute_batch(
                "CREATE TABLE evidence (
                   id TEXT PRIMARY KEY,
                   sha256 TEXT NOT NULL,
                   source_url TEXT NOT NULL,
                   record_json TEXT NOT NULL,
                   extracted_text TEXT NOT NULL
                 );
                 CREATE TABLE findings (
                   id TEXT PRIMARY KEY,
                   evidence_id TEXT NOT NULL REFERENCES evidence(id),
                   finding_json TEXT NOT NULL
                 );
                 CREATE TABLE alerts (
                   id TEXT PRIMARY KEY,
                   evidence_id TEXT NOT NULL REFERENCES evidence(id),
                   alert_json TEXT NOT NULL
                 );
                 CREATE TABLE approvals (
                   alert_id TEXT PRIMARY KEY REFERENCES alerts(id),
                   draft_digest TEXT NOT NULL,
                   approved_at TEXT NOT NULL
                 );
                 CREATE TABLE posts (
                   alert_id TEXT PRIMARY KEY REFERENCES alerts(id),
                   remote_ids_json TEXT NOT NULL,
                   posted_at TEXT NOT NULL
                 );
                 CREATE TABLE source_fetches (
                   source_id TEXT PRIMARY KEY,
                   fetched_at_unix INTEGER NOT NULL
                 );
                 CREATE TABLE post_segments (
                   alert_id TEXT NOT NULL REFERENCES posts(alert_id),
                   segment_index INTEGER NOT NULL,
                   remote_id TEXT NOT NULL,
                   PRIMARY KEY(alert_id, segment_index)
                 );",
            )
            .expect("v0.0.1 schema");
    }

    #[test]
    fn fresh_database_is_at_target_schema() {
        let dir = tempdir().expect("temp dir");
        let mut connection = Connection::open(dir.path().join("pnull.db")).expect("open");
        migrate(&mut connection).expect("migrate fresh");
        assert_eq!(current_version(&connection).expect("version"), SCHEMA_VERSION);
    }

    #[test]
    fn v01_database_upgrades_without_reinterpreting_records() {
        let dir = tempdir().expect("temp dir");
        let mut connection = Connection::open(dir.path().join("pnull.db")).expect("open");
        v01_schema(&connection);
        connection
            .execute(
                "INSERT INTO evidence(id, sha256, source_url, record_json, extracted_text)
                 VALUES ('evidence:abc', 'aa', 'https://example.test', '{}', 'text')",
                [],
            )
            .expect("seed evidence");
        migrate(&mut connection).expect("migrate v0.0.1");
        assert_eq!(current_version(&connection).expect("version"), SCHEMA_VERSION);
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM evidence", [], |row| row.get(0))
            .expect("count");
        assert_eq!(count, 1);
        let row: String = connection
            .query_row("SELECT record_json FROM evidence WHERE id='evidence:abc'", [], |row| {
                row.get(0)
            })
            .expect("record");
        assert_eq!(row, "{}");
    }

    #[test]
    fn migration_is_idempotent() {
        let dir = tempdir().expect("temp dir");
        let mut connection = Connection::open(dir.path().join("pnull.db")).expect("open");
        migrate(&mut connection).expect("first migrate");
        migrate(&mut connection).expect("second migrate");
        assert_eq!(current_version(&connection).expect("version"), SCHEMA_VERSION);
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
