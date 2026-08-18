//! `SQLite` schema versioning and transactional migrations.
//!
//! v0.0.1 databases carry no `user_version` (treated as 0). v0.0.2 introduces
//! `user_version = 1` and adds supplemental tables without rewriting canonical
//! v0.0.1 evidence, findings, alerts, approvals, posts, or source-fetch history.
//! v0.0.3 introduces `user_version = 2` and adds the procurement domain tables
//! (matters, events, identifiers, organizations, coverage ledger, immutable
//! snapshots and revisions, coverage diffs, reconciliation, case files, and CORA
//! drafts) without rewriting any prior canonical rows.

use rusqlite::{Connection, Transaction};
use thiserror::Error;

pub const SCHEMA_VERSION: u32 = 2;
/// The highest schema version this build understands.
pub const MAX_SUPPORTED_SCHEMA_VERSION: u32 = 2;

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
    // version is 0 (v0.0.1) or 1 (v0.0.2) here.
    let transaction = connection.transaction()?;
    if version < 1 {
        apply_v1(&transaction)?;
    }
    if version < 2 {
        apply_v2(&transaction)?;
    }
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

/// Creates the v0.0.3 procurement-domain tables. This is additive only: it never
/// rewrites canonical v0.0.1/v0.0.2 evidence, findings, alerts, or reviews.
fn apply_v2(transaction: &Transaction<'_>) -> Result<(), rusqlite::Error> {
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS procurement_matters (
           id TEXT PRIMARY KEY,
           official_title TEXT NOT NULL,
           matter_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS procurement_events (
           id TEXT PRIMARY KEY,
           matter_id TEXT NOT NULL,
           event_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS procurement_identifiers (
           id TEXT PRIMARY KEY,
           matter_id TEXT NOT NULL,
           identifier_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS procurement_organizations (
           id TEXT PRIMARY KEY,
           matter_id TEXT NOT NULL,
           organization_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS coverage_ledger (
           id TEXT PRIMARY KEY,
           source_id TEXT NOT NULL,
           entry_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS source_snapshots (
           id TEXT PRIMARY KEY,
           source_id TEXT NOT NULL,
           snapshot_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS snapshot_revisions (
           id TEXT PRIMARY KEY,
           snapshot_id TEXT NOT NULL,
           revision_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS snapshot_diffs (
           id TEXT PRIMARY KEY,
           old_snapshot_id TEXT NOT NULL,
           new_snapshot_id TEXT NOT NULL,
           diff_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS reconciliation_items (
           id TEXT PRIMARY KEY,
           matter_id TEXT NOT NULL,
           item_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS reconciliation_decisions (
           id TEXT PRIMARY KEY,
           item_id TEXT NOT NULL,
           decision_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS case_files (
           id TEXT PRIMARY KEY,
           matter_id TEXT NOT NULL,
           case_file_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS cora_drafts (
           id TEXT PRIMARY KEY,
           matter_id TEXT NOT NULL,
           draft_json TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_pe_matter ON procurement_events(matter_id);
         CREATE INDEX IF NOT EXISTS idx_pid_matter ON procurement_identifiers(matter_id);
         CREATE INDEX IF NOT EXISTS idx_porg_matter ON procurement_organizations(matter_id);
         CREATE INDEX IF NOT EXISTS idx_cl_source ON coverage_ledger(source_id);
         CREATE INDEX IF NOT EXISTS idx_ss_source ON source_snapshots(source_id);
         CREATE INDEX IF NOT EXISTS idx_sr_snapshot ON snapshot_revisions(snapshot_id);
         CREATE INDEX IF NOT EXISTS idx_sd_old ON snapshot_diffs(old_snapshot_id);
         CREATE INDEX IF NOT EXISTS idx_sd_new ON snapshot_diffs(new_snapshot_id);
         CREATE INDEX IF NOT EXISTS idx_ri_matter ON reconciliation_items(matter_id);
         CREATE INDEX IF NOT EXISTS idx_rd_item ON reconciliation_decisions(item_id);
         CREATE INDEX IF NOT EXISTS idx_cf_matter ON case_files(matter_id);
         CREATE INDEX IF NOT EXISTS idx_cora_matter ON cora_drafts(matter_id);",
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

    /// Counts rows in a set of v0.0.2 tables as a byte-for-byte preservation check.
    fn count_table(connection: &Connection, table: &str) -> i64 {
        connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))
            .expect("count")
    }

    /// Reads every row of a table as opaque JSON-ish strings for equality checks.
    fn rows_of(connection: &Connection, table: &str, columns: usize) -> Vec<Vec<String>> {
        let mut statement = connection
            .prepare(&format!("SELECT * FROM {table}"))
            .expect("prepare");
        let rows = statement
            .query_map([], |row| {
                let mut values = Vec::with_capacity(columns);
                for i in 0..columns {
                    values.push(row.get::<_, String>(i).unwrap_or_default());
                }
                Ok(values)
            })
            .expect("query");
        rows.collect::<Result<_, _>>().expect("collect")
    }

    #[test]
    fn v02_database_upgrades_to_v03_preserving_all_rows() {
        // Load the committed minimal v0.0.2 (schema version 1) fixture database.
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/migration/v0.0.2-minimal.sql");
        let sql = std::fs::read_to_string(&fixture).expect("read v0.0.2 fixture");

        let dir = tempdir().expect("temp dir");
        let mut connection = Connection::open(dir.path().join("pnull.db")).expect("open");
        connection.execute_batch(&sql).expect("load v0.0.2 fixture");

        assert_eq!(current_version(&connection).expect("version"), 1);
        let evidence_before = read_evidence(&connection);
        let matters_before = rows_of(&connection, "matters", 4);
        let reviews_before = rows_of(&connection, "review_decisions", 3);
        let runs_before = rows_of(&connection, "processing_runs", 2);
        let xrec_before = rows_of(&connection, "x_reconciliations", 3);

        migrate(&mut connection).expect("migrate v0.0.2 -> v0.0.3");
        assert_eq!(
            current_version(&connection).expect("version"),
            SCHEMA_VERSION
        );
        assert_eq!(SCHEMA_VERSION, 2);

        // Every canonical v0.0.1 and v0.0.2 row survives byte-for-byte.
        assert_eq!(read_evidence(&connection), evidence_before);
        assert_eq!(rows_of(&connection, "matters", 4), matters_before);
        assert_eq!(rows_of(&connection, "review_decisions", 3), reviews_before);
        assert_eq!(rows_of(&connection, "processing_runs", 2), runs_before);
        assert_eq!(rows_of(&connection, "x_reconciliations", 3), xrec_before);

        // The v0.0.3 procurement tables now exist and are empty.
        for table in [
            "procurement_matters",
            "procurement_events",
            "procurement_identifiers",
            "procurement_organizations",
            "coverage_ledger",
            "source_snapshots",
            "snapshot_revisions",
            "snapshot_diffs",
            "reconciliation_items",
            "reconciliation_decisions",
            "case_files",
            "cora_drafts",
        ] {
            assert_eq!(count_table(&connection, table), 0, "table {table} is empty");
        }
    }

    #[test]
    fn migration_failure_rolls_back_atomically() {
        // Load a real v0.0.2 fixture, then sabotage the migration by occupying an
        // index name with a table. The CREATE INDEX in apply_v2 will fail, which
        // must roll back the entire transaction: user_version stays at 1 and no
        // partial v0.0.3 table survives.
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/migration/v0.0.2-minimal.sql");
        let sql = std::fs::read_to_string(&fixture).expect("read v0.0.2 fixture");

        let dir = tempdir().expect("temp dir");
        let mut connection = Connection::open(dir.path().join("pnull.db")).expect("open");
        connection.execute_batch(&sql).expect("load v0.0.2 fixture");
        // Sabotage: a table now claims the index name idx_pe_matter.
        connection
            .execute_batch(
                "CREATE TABLE idx_pe_matter (x TEXT);
                 INSERT INTO idx_pe_matter VALUES ('sabotage');",
            )
            .expect("sabotage schema");

        assert!(migrate(&mut connection).is_err(), "migration must fail");
        assert_eq!(
            current_version(&connection).expect("version"),
            1,
            "user_version must roll back to 1 on failure"
        );
        // No v0.0.3 table may persist.
        let exists: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name IN
                 ('procurement_matters','coverage_ledger','source_snapshots','case_files')",
                [],
                |row| row.get(0),
            )
            .expect("check partial tables");
        assert_eq!(exists, 0, "no partial v0.0.3 tables may remain");
        // Canonical rows untouched.
        assert_eq!(count_table(&connection, "matters"), 1);
        assert_eq!(count_table(&connection, "evidence"), 1);
        // Sabotage object still present (so the failure was genuinely injected).
        assert_eq!(count_table(&connection, "idx_pe_matter"), 1);
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
