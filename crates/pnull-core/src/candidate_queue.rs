//! Durable, private scheduling metadata for source-bound document candidates.
//! Queue rows never substitute for immutable source artifacts or fetch receipts.

use rusqlite::{OptionalExtension, params};

use crate::{CoreError, Store};

const MAX_SEEDS: usize = 2_000;
const MAX_DUE_ROWS: usize = 2_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateQueueSeed {
    pub url: String,
    pub kind: String,
    pub anchor_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateQueueItem {
    pub url: String,
    pub kind: String,
    pub anchor_index: usize,
    pub snapshot_digest: String,
    pub is_current: bool,
    pub last_attempt_unix: Option<i64>,
    pub last_success_unix: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CandidateQueueOutcome {
    Archived,
    RetryAfter(i64),
    Blocked { context: String },
}

fn valid_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn invalid(detail: &str) -> CoreError {
    CoreError::InvalidCandidateQueue(detail.to_owned())
}

impl Store {
    /// Index an already preserved snapshot. This operation is atomic and
    /// idempotent for the same digest; a later sighting can make a URL current
    /// again without erasing its prior fetch history.
    pub fn index_candidate_snapshot(
        &self,
        origin_source_id: &str,
        digest: &str,
        observed_unix: i64,
        seeds: &[CandidateQueueSeed],
    ) -> Result<(), CoreError> {
        if !valid_key(origin_source_id) || !valid_digest(digest) || seeds.len() > MAX_SEEDS {
            return Err(invalid("invalid origin, snapshot digest, or seed count"));
        }
        let mut seen = std::collections::HashSet::new();
        for seed in seeds {
            if seed.url.is_empty()
                || seed.url.len() > 2_048
                || !matches!(
                    seed.kind.as_str(),
                    "meeting" | "agenda" | "packet" | "minutes" | "document"
                )
                || i64::try_from(seed.anchor_index).is_err()
                || !seen.insert(seed.url.as_str())
            {
                return Err(invalid("invalid or duplicate candidate seed"));
            }
        }
        self.transaction(|tx| {
            tx.execute(
                "INSERT INTO candidate_queue_snapshots(origin_source_id, digest, observed_unix)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(origin_source_id, digest) DO UPDATE SET
                   observed_unix = MAX(observed_unix, excluded.observed_unix)",
                params![origin_source_id, digest, observed_unix],
            )?;
            tx.execute(
                "INSERT INTO candidate_queue_origins(origin_source_id, latest_digest, latest_observed_unix)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(origin_source_id) DO UPDATE SET
                   latest_digest = excluded.latest_digest,
                   latest_observed_unix = excluded.latest_observed_unix
                 WHERE excluded.latest_observed_unix >= candidate_queue_origins.latest_observed_unix",
                params![origin_source_id, digest, observed_unix],
            )?;
            let mut statement = tx.prepare(
                "INSERT INTO candidate_queue_items(
                   origin_source_id, url, kind, snapshot_digest, anchor_index,
                   last_seen_digest, last_seen_unix, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?4, ?6, 'pending')
                 ON CONFLICT(origin_source_id, url) DO UPDATE SET
                   kind = excluded.kind,
                   snapshot_digest = excluded.snapshot_digest,
                   anchor_index = excluded.anchor_index,
                   last_seen_digest = excluded.last_seen_digest,
                   last_seen_unix = excluded.last_seen_unix,
                   status = CASE
                     WHEN candidate_queue_items.status = 'blocked'
                       AND candidate_queue_items.last_seen_digest != excluded.last_seen_digest
                     THEN 'pending' ELSE candidate_queue_items.status END,
                   blocked_context = CASE
                     WHEN candidate_queue_items.last_seen_digest != excluded.last_seen_digest
                     THEN NULL ELSE candidate_queue_items.blocked_context END,
                   retry_after_unix = CASE
                     WHEN candidate_queue_items.last_seen_digest != excluded.last_seen_digest
                     THEN 0 ELSE candidate_queue_items.retry_after_unix END
                 WHERE excluded.last_seen_unix >= candidate_queue_items.last_seen_unix",
            )?;
            for seed in seeds {
                statement.execute(params![
                    origin_source_id,
                    seed.url,
                    seed.kind,
                    digest,
                    i64::try_from(seed.anchor_index)
                        .map_err(|_| invalid("candidate anchor index is too large"))?,
                    observed_unix,
                ])?;
            }
            Ok(())
        })
    }

    /// Return a bounded page, alternating the preferred class after each
    /// attempted queue run. Within each class, interleave meeting HTML and
    /// document PDF candidates so one path policy cannot fill the whole page.
    /// Blocked rows never occupy a due page.
    pub fn candidate_queue_due(
        &self,
        origin_source_id: &str,
        now_unix: i64,
        refresh_after_seconds: i64,
        limit: usize,
    ) -> Result<Vec<CandidateQueueItem>, CoreError> {
        if !valid_key(origin_source_id)
            || !(1..=31_536_000).contains(&refresh_after_seconds)
            || !(1..=MAX_DUE_ROWS).contains(&limit)
        {
            return Err(invalid("invalid bounded due query"));
        }
        let cutoff = now_unix.saturating_sub(refresh_after_seconds);
        let limit = i64::try_from(limit).map_err(|_| invalid("due query limit overflow"))?;
        let mut statement = self.connection().prepare(
            "WITH due AS (
               SELECT i.url, i.kind, i.anchor_index, i.snapshot_digest,
                      i.last_seen_digest = o.latest_digest AS is_current,
                      i.last_attempt_unix, i.last_success_unix,
                      CASE WHEN i.status = o.next_class THEN 0 ELSE 1 END AS class_rank,
                      CASE WHEN i.kind = 'meeting' THEN 0 ELSE 1 END AS policy_rank,
                      ROW_NUMBER() OVER (
                        PARTITION BY
                          CASE WHEN i.status = o.next_class THEN 0 ELSE 1 END,
                          CASE WHEN i.kind = 'meeting' THEN 0 ELSE 1 END
                        ORDER BY COALESCE(i.last_attempt_unix, -9223372036854775808), i.url
                      ) AS policy_row
               FROM candidate_queue_items i
               JOIN candidate_queue_origins o ON o.origin_source_id = i.origin_source_id
               WHERE i.origin_source_id = ?1 AND i.retry_after_unix <= ?2
                 AND (
                   i.status = 'pending'
                   OR (i.status = 'archived'
                       AND i.last_seen_digest = o.latest_digest
                       AND COALESCE(i.last_success_unix, -9223372036854775808) <= ?3)
                 )
             )
             SELECT url, kind, anchor_index, snapshot_digest, is_current,
                    last_attempt_unix, last_success_unix
             FROM due
             ORDER BY class_rank, policy_row, policy_rank, url
             LIMIT ?4",
        )?;
        let rows =
            statement.query_map(params![origin_source_id, now_unix, cutoff, limit], |row| {
                let anchor: i64 = row.get(2)?;
                Ok(CandidateQueueItem {
                    url: row.get(0)?,
                    kind: row.get(1)?,
                    anchor_index: usize::try_from(anchor)
                        .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(2, anchor))?,
                    snapshot_digest: row.get(3)?,
                    is_current: row.get(4)?,
                    last_attempt_unix: row.get(5)?,
                    last_success_unix: row.get(6)?,
                })
            })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(CoreError::from)
    }

    /// Advance once for an actual queue run with at least one selected fetch.
    /// The turn is scheduling metadata, not a source or fetch assertion.
    pub fn advance_candidate_queue_turn(&self, origin_source_id: &str) -> Result<(), CoreError> {
        if !valid_key(origin_source_id) {
            return Err(invalid("invalid candidate origin"));
        }
        let changed = self.connection().execute(
            "UPDATE candidate_queue_origins
             SET next_class = CASE next_class
               WHEN 'pending' THEN 'archived' ELSE 'pending' END
             WHERE origin_source_id = ?1",
            [origin_source_id],
        )?;
        if changed != 1 {
            return Err(CoreError::NotFound(format!(
                "candidate queue origin {origin_source_id}"
            )));
        }
        Ok(())
    }

    pub fn candidate_queue_total(&self, origin_source_id: &str) -> Result<usize, CoreError> {
        if !valid_key(origin_source_id) {
            return Err(invalid("invalid candidate origin"));
        }
        let count: i64 = self.connection().query_row(
            "SELECT COUNT(*) FROM candidate_queue_items WHERE origin_source_id = ?1",
            [origin_source_id],
            |row| row.get(0),
        )?;
        usize::try_from(count).map_err(|_| invalid("candidate count overflow"))
    }

    pub fn candidate_queue_latest_snapshot(
        &self,
        origin_source_id: &str,
    ) -> Result<Option<String>, CoreError> {
        if !valid_key(origin_source_id) {
            return Err(invalid("invalid candidate origin"));
        }
        self.connection()
            .query_row(
                "SELECT latest_digest FROM candidate_queue_origins WHERE origin_source_id = ?1",
                [origin_source_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(CoreError::from)
    }

    /// Reconsider blocked scheduling rows when the source configuration or
    /// recorded review binding changes. This changes no evidence or receipt.
    /// The bounded limit lets a large backlog recover over repeated runs.
    pub fn requeue_blocked_candidate_context(
        &self,
        origin_source_id: &str,
        context_token: &str,
        limit: usize,
    ) -> Result<usize, CoreError> {
        if !valid_key(origin_source_id)
            || !valid_digest(context_token)
            || !(1..=MAX_DUE_ROWS).contains(&limit)
        {
            return Err(invalid("invalid blocked candidate requeue parameters"));
        }
        let prefix = format!("{context_token}:");
        let changed = self.connection().execute(
            "UPDATE candidate_queue_items
             SET status = 'pending', blocked_context = NULL, retry_after_unix = 0
             WHERE rowid IN (
               SELECT rowid FROM candidate_queue_items
               WHERE origin_source_id = ?1 AND status = 'blocked'
                 AND COALESCE(substr(blocked_context, 1, 65), '') != ?2
               ORDER BY url LIMIT ?3
             )",
            params![
                origin_source_id,
                prefix,
                i64::try_from(limit).map_err(|_| invalid("requeue limit overflow"))?
            ],
        )?;
        Ok(changed)
    }

    /// Record scheduler state only. A caller must still preserve a real fetch
    /// receipt and original bytes before marking a candidate archived.
    pub fn mark_candidate_queue_outcome(
        &self,
        origin_source_id: &str,
        url: &str,
        attempt_unix: i64,
        outcome: CandidateQueueOutcome,
    ) -> Result<(), CoreError> {
        if !valid_key(origin_source_id) || url.is_empty() || url.len() > 2_048 {
            return Err(invalid("invalid candidate outcome key"));
        }
        let (status, success, retry, blocked): (&str, Option<i64>, i64, Option<String>) =
            match outcome {
                CandidateQueueOutcome::Archived => ("archived", Some(attempt_unix), 0, None),
                CandidateQueueOutcome::RetryAfter(after) if after > attempt_unix => {
                    ("retry", None, after, None)
                }
                CandidateQueueOutcome::Blocked { context }
                    if !context.trim().is_empty() && context.len() <= 500 =>
                {
                    ("blocked", None, 0, Some(context))
                }
                _ => return Err(invalid("invalid candidate outcome")),
            };
        let changed = if status == "retry" {
            self.connection().execute(
                "UPDATE candidate_queue_items
                 SET last_attempt_unix = ?3, retry_after_unix = ?4
                 WHERE origin_source_id = ?1 AND url = ?2",
                params![origin_source_id, url, attempt_unix, retry],
            )?
        } else {
            self.connection().execute(
                "UPDATE candidate_queue_items
                 SET status = ?3, last_attempt_unix = ?4,
                     last_success_unix = COALESCE(?5, last_success_unix),
                     retry_after_unix = ?6, blocked_context = ?7
                 WHERE origin_source_id = ?1 AND url = ?2",
                params![
                    origin_source_id,
                    url,
                    status,
                    attempt_unix,
                    success,
                    retry,
                    blocked
                ],
            )?
        };
        if changed != 1 {
            return Err(CoreError::NotFound(format!(
                "candidate queue row {origin_source_id} {url}"
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn seed(index: usize) -> CandidateQueueSeed {
        CandidateQueueSeed {
            url: format!("https://official.example.test/View.ashx?M=A&ID={index}"),
            kind: "agenda".to_owned(),
            anchor_index: index,
        }
    }

    #[test]
    fn large_meeting_backlog_cannot_hide_document_policy_from_due_window() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let mut seeds = (0..300)
            .map(|index| CandidateQueueSeed {
                url: format!("https://official.example.test/MeetingDetail.aspx?ID={index}"),
                kind: "meeting".to_owned(),
                anchor_index: index,
            })
            .collect::<Vec<_>>();
        let pdf = seed(999);
        seeds.push(pdf.clone());
        store
            .index_candidate_snapshot("town-index", &"a".repeat(64), 10, &seeds)
            .unwrap();

        let due = store
            .candidate_queue_due("town-index", 11, 60, 256)
            .unwrap();
        assert_eq!(due.len(), 256);
        assert_eq!(due[0].kind, "meeting");
        assert_eq!(due[1].url, pdf.url);
        assert_eq!(due.iter().filter(|item| item.kind == "agenda").count(), 1);
    }

    #[test]
    fn more_than_128_snapshots_progress_through_indexed_due_rows() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        for index in 0..=128 {
            store
                .index_candidate_snapshot(
                    "town-index",
                    &format!("{index:064x}"),
                    i64::try_from(index).unwrap(),
                    &[seed(index)],
                )
                .unwrap();
        }
        assert_eq!(store.candidate_queue_total("town-index").unwrap(), 129);
        assert_eq!(
            store.candidate_queue_latest_snapshot("town-index").unwrap(),
            Some(format!("{:064x}", 128))
        );
        let first = store
            .candidate_queue_due("town-index", 1_000, 60, 8)
            .unwrap();
        assert_eq!(first.len(), 8);
        assert_eq!(first[0].url, seed(0).url);
        assert!(!first[0].is_current);
        for item in &first {
            store
                .mark_candidate_queue_outcome(
                    "town-index",
                    &item.url,
                    1_000,
                    CandidateQueueOutcome::Archived,
                )
                .unwrap();
        }
        let second = store
            .candidate_queue_due("town-index", 1_001, 60, 8)
            .unwrap();
        assert_eq!(second.len(), 8);
        assert!(
            second
                .iter()
                .all(|item| !first.iter().any(|old| old.url == item.url))
        );
    }

    #[test]
    fn blocked_and_retry_rows_do_not_starve_later_pending_or_current_refresh() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let first_digest = "a".repeat(64);
        store
            .index_candidate_snapshot(
                "town-index",
                &first_digest,
                10,
                &[seed(1), seed(2), seed(3)],
            )
            .unwrap();
        store
            .mark_candidate_queue_outcome(
                "town-index",
                &seed(1).url,
                11,
                CandidateQueueOutcome::Blocked {
                    context: "robots disallows this exact path".to_owned(),
                },
            )
            .unwrap();
        store
            .mark_candidate_queue_outcome(
                "town-index",
                &seed(2).url,
                11,
                CandidateQueueOutcome::RetryAfter(100),
            )
            .unwrap();
        let due = store.candidate_queue_due("town-index", 12, 60, 8).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].url, seed(3).url);
        store
            .mark_candidate_queue_outcome(
                "town-index",
                &seed(3).url,
                12,
                CandidateQueueOutcome::Archived,
            )
            .unwrap();
        assert!(
            store
                .candidate_queue_due("town-index", 13, 60, 8)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            store.candidate_queue_due("town-index", 100, 60, 8).unwrap()[0].url,
            seed(2).url
        );
        let next_digest = "b".repeat(64);
        store
            .index_candidate_snapshot("town-index", &next_digest, 101, &[seed(1), seed(3)])
            .unwrap();
        let due = store.candidate_queue_due("town-index", 102, 60, 8).unwrap();
        assert_eq!(due[0].url, seed(1).url); // a new sighting rechecks a block
        assert!(
            due.iter()
                .any(|item| item.url == seed(3).url && item.is_current)
        );
    }

    #[test]
    fn large_pending_history_and_current_refresh_take_durable_turns() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let current = seed(999);
        store
            .index_candidate_snapshot(
                "town-index",
                &"a".repeat(64),
                1,
                std::slice::from_ref(&current),
            )
            .unwrap();
        store
            .mark_candidate_queue_outcome(
                "town-index",
                &current.url,
                2,
                CandidateQueueOutcome::Archived,
            )
            .unwrap();
        let mut next = (0..300).map(seed).collect::<Vec<_>>();
        next.push(current.clone());
        store
            .index_candidate_snapshot("town-index", &"b".repeat(64), 3, &next)
            .unwrap();

        let first = store.candidate_queue_due("town-index", 100, 60, 8).unwrap();
        assert_eq!(first.len(), 8);
        assert!(first.iter().all(|item| item.last_success_unix.is_none()));

        store.advance_candidate_queue_turn("town-index").unwrap();
        let refresh = store.candidate_queue_due("town-index", 100, 60, 8).unwrap();
        assert_eq!(refresh[0].url, current.url);
        assert!(refresh[0].is_current);

        store.advance_candidate_queue_turn("town-index").unwrap();
        drop(store);
        let reopened = Store::open(dir.path()).unwrap();
        let historical = reopened
            .candidate_queue_due("town-index", 100, 60, 8)
            .unwrap();
        assert!(
            historical
                .iter()
                .all(|item| item.last_success_unix.is_none())
        );
    }

    #[test]
    fn malformed_or_duplicate_seed_cannot_partially_change_index() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        assert!(
            store
                .index_candidate_snapshot("town-index", &"c".repeat(64), 1, &[seed(1), seed(1)])
                .is_err()
        );
        assert_eq!(store.candidate_queue_total("town-index").unwrap(), 0);
        assert!(
            store
                .candidate_queue_latest_snapshot("town-index")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn changed_review_context_requeues_blocked_rows_in_bounded_batches() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        store
            .index_candidate_snapshot(
                "town-index",
                &"a".repeat(64),
                10,
                &[seed(1), seed(2), seed(3)],
            )
            .unwrap();
        let old = "b".repeat(64);
        for candidate in [seed(1), seed(2), seed(3)] {
            store
                .mark_candidate_queue_outcome(
                    "town-index",
                    &candidate.url,
                    11,
                    CandidateQueueOutcome::Blocked {
                        context: format!("{old}:robots disallowed"),
                    },
                )
                .unwrap();
        }
        assert_eq!(
            store
                .requeue_blocked_candidate_context("town-index", &old, 2)
                .unwrap(),
            0
        );
        let renewed = "c".repeat(64);
        assert_eq!(
            store
                .requeue_blocked_candidate_context("town-index", &renewed, 2)
                .unwrap(),
            2
        );
        assert_eq!(
            store
                .candidate_queue_due("town-index", 12, 60, 8)
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            store
                .requeue_blocked_candidate_context("town-index", &renewed, 2)
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .candidate_queue_due("town-index", 12, 60, 8)
                .unwrap()
                .len(),
            3
        );
    }
}
