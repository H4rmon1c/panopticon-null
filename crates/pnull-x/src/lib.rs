//! State-aware X drafting with explicit approval, confirmation, and idempotent transport.

use std::fmt;
use std::fs;
use std::path::Path;

use pnull_core::{
    Alert, CoreError, ProcurementAlert, Store, XAttempt, XReconciliation, XSegment, stable_id,
};
use reqwest::blocking::Client;
use rusqlite::{Transaction, params};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const X_CHARACTER_LIMIT: usize = 280;
const CHUNK_TARGET: usize = 270;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Draft {
    pub alert_id: String,
    pub posts: Vec<String>,
}

impl Draft {
    pub fn digest(&self) -> String {
        let mut parts = vec![self.alert_id.as_str()];
        parts.extend(self.posts.iter().map(String::as_str));
        stable_id("x-draft", &parts)
    }
}

#[derive(Debug, Error)]
pub enum XError {
    #[error("draft refused: the finding has no exact source citation")]
    MissingCitation,
    #[error("draft refused by sensitive-data policy")]
    SensitiveData,
    #[error("procurement alert is not publishable: an affected matter or alert is withheld")]
    ProcurementNotPublished,
    #[error("draft cannot fit within X character limits")]
    CharacterLimit,
    #[error("alert has not received explicit approval for this exact draft")]
    NotApproved,
    #[error("live posting requires --confirm")]
    NotConfirmed,
    #[error("this alert already has a post or post attempt recorded")]
    AlreadyPosted,
    #[error(
        "cannot start a new posting attempt while the most recent attempt for this alert is unresolved (uncertain or in progress); reconcile it first"
    )]
    UncertainAttempt,
    #[error("unsupported reconciliation decision: {0}")]
    UnsupportedDecision(String),
    #[error("unsafe reconciliation: {0}")]
    UnsafeReconciliation(String),
    #[error("X credentials are not configured")]
    CredentialsMissing,
    #[error("the runtime secret file is not protected with mode 0600")]
    SecretPermissions,
    #[error("X transport failed; credentials and response content were redacted")]
    Transport,
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error(transparent)]
    Publish(#[from] pnull_publish::PublishError),
    #[error("could not read protected runtime secret")]
    SecretRead,
}

pub fn draft(alert: &Alert, canonical_base_url: &str) -> Result<Draft, XError> {
    if alert.citations.is_empty()
        || alert
            .citations
            .iter()
            .any(|citation| citation.quote.trim().is_empty())
    {
        return Err(XError::MissingCitation);
    }
    pnull_publish::validate_public_text(&alert.summary).map_err(|_| XError::SensitiveData)?;
    pnull_publish::validate_public_text(&alert.title).map_err(|_| XError::SensitiveData)?;
    let alert_url = format!(
        "{}/alerts/{}.html",
        canonical_base_url.trim_end_matches('/'),
        safe_id(&alert.id)
    );
    let text = format!(
        "COLORADO SURVEILLANCE ALERT · AUTOMATED\n\n{} — monitored matter: {}\n\nCurrent public-record state: {}. This state does not by itself establish a vendor purchase.\n\nDetected change: {}\n\nSource documents, archived hashes, and exact citations:\n{}\n\nPublic power must remain publicly visible.",
        alert.jurisdiction,
        alert.title,
        alert.state.label(),
        alert.summary,
        alert_url
    );
    pnull_publish::validate_public_text(&text).map_err(|_| XError::SensitiveData)?;
    let posts = split_thread(&text)?;
    if posts
        .iter()
        .any(|post| post.chars().count() > X_CHARACTER_LIMIT)
    {
        return Err(XError::CharacterLimit);
    }
    Ok(Draft {
        alert_id: alert.id.clone(),
        posts,
    })
}

/// Drafts an X thread for a procurement change alert (v0.0.4, Item 1).
///
/// Reuses the same pipeline machinery as the general alert draft: dry-run
/// default, exact-digest approval, canonical-URL check, credentials gate, and
/// reconciliation for uncertain attempts. The body identifies the feed as
/// automated, names the jurisdiction and matter/identifier, states the observed
/// change in one sentence with phrasing discipline, links the published
/// case-file page under `canonical_base_url`, and carries the "public record;
/// not proof of absence" caveat. The page must pass the current publication
/// gates before a draft is produced.
pub fn draft_procurement(
    store: &Store,
    alert: &ProcurementAlert,
    canonical_base_url: &str,
    jurisdiction: &str,
    matter_label: &str,
) -> Result<Draft, XError> {
    assert_procurement_published(store, alert)?;
    pnull_publish::validate_public_text(&alert.summary).map_err(|_| XError::SensitiveData)?;
    let first = alert.changes.first().ok_or(XError::MissingCitation)?;
    let kind = first.change_kind.label();
    // The published case-file page for a procurement change is the affected
    // matter's page (deterministic slug from the matter id). The canonical base
    // URL already carries the site path prefix (e.g. `.../co`), so the
    // `procurement/` segment is appended directly, matching how the general
    // alert draft appends `/alerts/...`. When no matter id is resolvable,
    // the alert itself is withheld by the publisher and cannot be drafted.
    let matter_id = alert
        .matter_ids
        .first()
        .ok_or(XError::ProcurementNotPublished)?;
    let case_page = format!(
        "{}/procurement/{}/index.html",
        canonical_base_url.trim_end_matches('/'),
        safe_id(matter_id)
    );
    let text = format!(
        "AUTOMATED PUBLIC-RECORD CHANGE NOTICE · procurement\n\n{jurisdiction} — matter/identifier: {matter_label}\n\nObserved change ({kind}): {}. This reports a change in the public record; it is not proof of absence or of wrongdoing.\n\nCase-file page: {case_page}\n\nPublic record; not proof of absence.",
        alert.summary
    );
    pnull_publish::validate_public_text(&text).map_err(|_| XError::SensitiveData)?;
    let posts = split_thread(&text)?;
    if posts
        .iter()
        .any(|post| post.chars().count() > X_CHARACTER_LIMIT)
    {
        return Err(XError::CharacterLimit);
    }
    Ok(Draft {
        alert_id: alert.id.clone(),
        posts,
    })
}

fn assert_procurement_published(store: &Store, alert: &ProcurementAlert) -> Result<(), XError> {
    if pnull_publish::procurement::approved_procurement_alert_page(store, alert)?.is_none() {
        return Err(XError::ProcurementNotPublished);
    }
    Ok(())
}

fn split_thread(text: &str) -> Result<Vec<String>, XError> {
    if text.chars().count() <= X_CHARACTER_LIMIT {
        return Ok(vec![text.to_owned()]);
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    for paragraph in text.split("\n\n") {
        let additional = paragraph.chars().count() + usize::from(!current.is_empty()) * 2;
        if current.chars().count() + additional <= CHUNK_TARGET {
            if !current.is_empty() {
                current.push_str("\n\n");
            }
            current.push_str(paragraph);
        } else {
            if !current.is_empty() {
                chunks.push(current);
                current = String::new();
            }
            if paragraph.chars().count() <= CHUNK_TARGET {
                current.push_str(paragraph);
            } else {
                split_words(paragraph, CHUNK_TARGET, &mut chunks)?;
            }
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    if chunks.len() > 8 || chunks.is_empty() {
        return Err(XError::CharacterLimit);
    }
    let total = chunks.len();
    let numbered: Vec<String> = chunks
        .into_iter()
        .enumerate()
        .map(|(index, chunk)| format!("{chunk}\n\n({}/{total})", index + 1))
        .collect();
    if numbered
        .iter()
        .any(|post| post.chars().count() > X_CHARACTER_LIMIT)
    {
        return Err(XError::CharacterLimit);
    }
    Ok(numbered)
}

fn split_words(paragraph: &str, target: usize, output: &mut Vec<String>) -> Result<(), XError> {
    let mut current = String::new();
    for word in paragraph.split_whitespace() {
        if word.chars().count() > target {
            return Err(XError::CharacterLimit);
        }
        let extra = word.chars().count() + usize::from(!current.is_empty());
        if current.chars().count() + extra > target {
            output.push(std::mem::take(&mut current));
            current.push_str(word);
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        output.push(current);
    }
    Ok(())
}

fn safe_id(id: &str) -> String {
    id.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

pub trait XTransport {
    fn submit(&mut self, text: &str, reply_to: Option<&str>) -> Result<String, XError>;
}

pub fn post_approved<T: XTransport>(
    store: &Store,
    draft: &Draft,
    confirmed: bool,
    posted_at: &str,
    transport: &mut T,
) -> Result<Vec<String>, XError> {
    post_with_attempt(store, draft, confirmed, posted_at, transport)
}

/// Posts a draft while recording an [`XAttempt`] for operator visibility and
/// reconciliation. Every posting run records an attempt first (status
/// `"in_progress"`, all segments `"pending"`). As each segment is submitted the
/// segment state is advanced to `"posted"` with its remote id. On a transport
/// failure the attempt is left `"uncertain"` with its partially-posted segments
/// in `"posted"` state; an uncertain attempt is never blindly retried.
pub fn post_with_attempt<T: XTransport>(
    store: &Store,
    draft: &Draft,
    confirmed: bool,
    posted_at: &str,
    transport: &mut T,
) -> Result<Vec<String>, XError> {
    if !confirmed {
        return Err(XError::NotConfirmed);
    }
    if draft.posts.is_empty()
        || draft.posts.len() > 8
        || draft
            .posts
            .iter()
            .any(|post| post.trim().is_empty() || post.chars().count() > X_CHARACTER_LIMIT)
    {
        return Err(XError::CharacterLimit);
    }
    for post in &draft.posts {
        pnull_publish::validate_public_text(post).map_err(|_| XError::SensitiveData)?;
    }
    let procurement_alert = store
        .all_procurement_alerts()?
        .into_iter()
        .find(|alert| alert.id == draft.alert_id);
    if let Some(alert) = procurement_alert.as_ref() {
        assert_procurement_published(store, alert)?;
    } else {
        if draft
            .posts
            .iter()
            .any(|post| post.contains("/procurement/"))
        {
            return Err(XError::ProcurementNotPublished);
        }
        let alert = store.alert(&draft.alert_id)?;
        pnull_publish::assert_citations_approved(store, &alert.citations)?;
    }
    if store.approved_draft_digest(&draft.alert_id)?.as_deref() != Some(draft.digest().as_str()) {
        return Err(XError::NotApproved);
    }
    guard_against_blind_retry(store, &draft.alert_id)?;
    if store.is_posted(&draft.alert_id)? {
        return Err(XError::AlreadyPosted);
    }

    let attempt_id = stable_id("x-attempt", &[&draft.alert_id, posted_at]);
    let segments: Vec<XSegment> = draft
        .posts
        .iter()
        .enumerate()
        .map(|(index, _)| XSegment {
            index: u32::try_from(index).unwrap_or(u32::MAX),
            remote_id: None,
            state: "pending".to_owned(),
        })
        .collect();
    let mut attempt = XAttempt {
        id: attempt_id,
        alert_id: draft.alert_id.clone(),
        draft_digest: draft.digest(),
        started_at: posted_at.to_owned(),
        status: "in_progress".to_owned(),
        segments,
    };
    if !store.reserve_x_post_attempt(&attempt)? {
        return Err(XError::AlreadyPosted);
    }

    let mut remote_ids = Vec::new();
    let mut failure: Option<XError> = None;
    for (index, post) in draft.posts.iter().enumerate() {
        let reply_to = remote_ids.last().map(String::as_str);
        match transport.submit(post, reply_to) {
            Ok(remote_id) => {
                if remote_id.trim().is_empty() {
                    failure = Some(XError::Transport);
                    break;
                }
                if let Some(segment) = attempt
                    .segments
                    .iter_mut()
                    .find(|segment| segment.index as usize == index)
                {
                    segment.remote_id = Some(remote_id.clone());
                    "posted".clone_into(&mut segment.state);
                }
                persist_post_segment(store, &attempt, index, &remote_id)?;
                remote_ids.push(remote_id);
            }
            Err(error) => {
                failure = Some(error);
                break;
            }
        }
    }
    if let Some(error) = failure {
        "uncertain".clone_into(&mut attempt.status);
        persist_attempt(store, &attempt)?;
        return Err(error);
    }
    "complete".clone_into(&mut attempt.status);
    complete_attempt(store, &attempt, &remote_ids, posted_at)?;
    Ok(remote_ids)
}

fn persist_post_segment(
    store: &Store,
    attempt: &XAttempt,
    index: usize,
    remote_id: &str,
) -> Result<(), XError> {
    let json = serde_json::to_string(attempt).map_err(CoreError::from)?;
    store.transaction(|transaction| {
        transaction.execute(
            "INSERT INTO post_segments(alert_id, segment_index, remote_id) VALUES (?1, ?2, ?3)",
            params![
                attempt.alert_id,
                i64::try_from(index).unwrap_or(i64::MAX),
                remote_id
            ],
        )?;
        transaction.execute(
            "UPDATE x_attempts SET attempt_json = ?1 WHERE id = ?2",
            params![json, attempt.id],
        )?;
        Ok(())
    })?;
    Ok(())
}

fn complete_attempt(
    store: &Store,
    attempt: &XAttempt,
    remote_ids: &[String],
    posted_at: &str,
) -> Result<(), XError> {
    let attempt_json = serde_json::to_string(attempt).map_err(CoreError::from)?;
    let remote_ids_json = serde_json::to_string(remote_ids).map_err(CoreError::from)?;
    store.transaction(|transaction| {
        if transaction.execute(
            "UPDATE posts SET remote_ids_json = ?2, posted_at = ?3 WHERE alert_id = ?1 AND posted_at = 'IN_PROGRESS'",
            params![attempt.alert_id, remote_ids_json, posted_at],
        )? != 1 {
            return Err(CoreError::PostStateConflict(attempt.alert_id.clone()));
        }
        transaction.execute(
            "UPDATE x_attempts SET attempt_json = ?1 WHERE id = ?2",
            params![attempt_json, attempt.id],
        )?;
        Ok(())
    })?;
    Ok(())
}

/// Refuses a new posting attempt while the most recent attempt for the alert
/// is unresolved (`"uncertain"` or `"in_progress"`), so an uncertain attempt is
/// never blindly retried before an operator reconciles it.
fn guard_against_blind_retry(store: &Store, alert_id: &str) -> Result<(), XError> {
    let attempts = load_attempts_for_alert(store, alert_id)?;
    if let Some(latest) = attempts.last()
        && matches!(
            latest.status.as_str(),
            "uncertain" | "in_progress" | "partial" | "abandoned"
        )
    {
        return Err(XError::UncertainAttempt);
    }
    Ok(())
}

/// Loads attempts for an alert, oldest first, ordered by their JSON `started_at`.
fn load_attempts_for_alert(store: &Store, alert_id: &str) -> Result<Vec<XAttempt>, XError> {
    let attempts = store.transaction(|transaction| {
        let mut statement = transaction.prepare(
            "SELECT attempt_json FROM x_attempts WHERE alert_id = ?1 \
                 ORDER BY json_extract(attempt_json, '$.started_at')",
        )?;
        let rows = statement.query_map(params![alert_id], |row| row.get::<_, String>(0))?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    })?;
    Ok(attempts)
}

/// Loads reconciliation history for an attempt, oldest first, ordered by JSON
/// `decided_at`. Appends, never deletes.
#[cfg(test)]
fn load_reconciliations_for_attempt(
    store: &Store,
    attempt_id: &str,
) -> Result<Vec<XReconciliation>, XError> {
    let items = store.transaction(|transaction| {
        let mut statement = transaction.prepare(
            "SELECT reconciliation_json FROM x_reconciliations WHERE attempt_id = ?1 \
                 ORDER BY json_extract(reconciliation_json, '$.decided_at')",
        )?;
        let rows = statement.query_map(params![attempt_id], |row| row.get::<_, String>(0))?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    })?;
    Ok(items)
}

/// Re-persists an attempt after its segments or status change.
fn persist_attempt(store: &Store, attempt: &XAttempt) -> Result<(), XError> {
    let json = serde_json::to_string(attempt).map_err(CoreError::from)?;
    store.transaction(|transaction| {
        transaction.execute(
            "UPDATE x_attempts SET attempt_json = ?1 WHERE id = ?2",
            params![json, attempt.id],
        )?;
        Ok(())
    })?;
    Ok(())
}

/// Records an append-only operator reconciliation decision for an attempt and
/// updates the attempt's status accordingly. Never deletes audit history.
pub fn reconcile(
    store: &Store,
    attempt_id: &str,
    decision: &str,
    remote_id: Option<&str>,
    operator: &str,
    note: &str,
    decided_at: &str,
) -> Result<XReconciliation, XError> {
    if !matches!(
        decision,
        "confirm_posted" | "confirm_none_posted" | "partial" | "abandon"
    ) {
        return Err(XError::UnsupportedDecision(decision.to_owned()));
    }
    if operator.trim().is_empty() || note.trim().is_empty() {
        return Err(XError::UnsafeReconciliation(
            "operator and note are required".to_owned(),
        ));
    }
    if remote_id.is_some_and(|id| id.trim().is_empty()) {
        return Err(XError::UnsafeReconciliation(
            "remote ID cannot be blank".to_owned(),
        ));
    }
    if decision != "confirm_posted" && remote_id.is_some() {
        return Err(XError::UnsafeReconciliation(
            "remote ID is only valid with confirm_posted".to_owned(),
        ));
    }
    let transaction = store
        .connection()
        .unchecked_transaction()
        .map_err(CoreError::from)?;
    let attempt_json: String = transaction
        .query_row(
            "SELECT attempt_json FROM x_attempts WHERE id = ?1",
            [attempt_id],
            |row| row.get(0),
        )
        .map_err(CoreError::from)?;
    let mut attempt: XAttempt = serde_json::from_str(&attempt_json).map_err(CoreError::from)?;
    if !matches!(attempt.status.as_str(), "uncertain" | "in_progress") {
        return Err(XError::UnsafeReconciliation(format!(
            "attempt is already resolved ({})",
            attempt.status
        )));
    }
    let recorded: Vec<(u32, String)> = {
        let mut statement = transaction
            .prepare(
                "SELECT segment_index, remote_id FROM post_segments WHERE alert_id = ?1 ORDER BY segment_index",
            )
            .map_err(CoreError::from)?;
        let rows = statement
            .query_map([&attempt.alert_id], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(CoreError::from)?;
        rows.collect::<Result<_, _>>().map_err(CoreError::from)?
    };
    let known: Vec<(u32, String)> = attempt
        .segments
        .iter()
        .filter_map(|segment| {
            segment
                .remote_id
                .as_ref()
                .map(|id| (segment.index, id.clone()))
        })
        .collect();
    if recorded != known {
        return Err(XError::UnsafeReconciliation(
            "stored segment IDs disagree with the attempt; investigate before reconciling"
                .to_owned(),
        ));
    }
    let item = XReconciliation {
        id: stable_id("x-reconcile", &[attempt_id, decided_at]),
        attempt_id: attempt_id.to_owned(),
        decision: decision.to_owned(),
        remote_id: remote_id.map(str::to_owned),
        note: note.to_owned(),
        operator: operator.to_owned(),
        decided_at: decided_at.to_owned(),
    };
    apply_reconciliation_decision(
        &transaction,
        &mut attempt,
        decision,
        remote_id,
        &known,
        decided_at,
    )?;
    transaction
        .execute(
            "INSERT INTO x_reconciliations(id, attempt_id, reconciliation_json) VALUES (?1, ?2, ?3)",
            params![item.id, item.attempt_id, serde_json::to_string(&item).map_err(CoreError::from)?],
        )
        .map_err(CoreError::from)?;
    transaction
        .execute(
            "UPDATE x_attempts SET attempt_json = ?1 WHERE id = ?2",
            params![
                serde_json::to_string(&attempt).map_err(CoreError::from)?,
                attempt.id
            ],
        )
        .map_err(CoreError::from)?;
    transaction.commit().map_err(CoreError::from)?;
    Ok(item)
}

fn apply_reconciliation_decision(
    transaction: &Transaction<'_>,
    attempt: &mut XAttempt,
    decision: &str,
    remote_id: Option<&str>,
    known: &[(u32, String)],
    decided_at: &str,
) -> Result<(), XError> {
    match decision {
        "confirm_posted" => {
            let missing = attempt
                .segments
                .iter()
                .filter(|segment| segment.remote_id.is_none())
                .count();
            if missing != usize::from(remote_id.is_some()) {
                return Err(XError::UnsafeReconciliation(
                    "confirm_posted requires one remote ID for the sole missing segment, or none when all IDs are already recorded"
                        .to_owned(),
                ));
            }
            if let Some(remote_id) = remote_id {
                if known.iter().any(|(_, id)| id == remote_id) {
                    return Err(XError::UnsafeReconciliation(
                        "remote ID is already assigned to another segment".to_owned(),
                    ));
                }
                let segment = attempt
                    .segments
                    .iter_mut()
                    .find(|segment| segment.remote_id.is_none())
                    .expect("validated exactly one missing segment");
                segment.remote_id = Some(remote_id.to_owned());
                transaction
                    .execute(
                        "INSERT INTO post_segments(alert_id, segment_index, remote_id) VALUES (?1, ?2, ?3)",
                        params![attempt.alert_id, segment.index, remote_id],
                    )
                    .map_err(CoreError::from)?;
            }
            for segment in &mut attempt.segments {
                "posted".clone_into(&mut segment.state);
            }
            "reconciled".clone_into(&mut attempt.status);
            let remote_ids: Vec<String> = attempt
                .segments
                .iter()
                .filter_map(|segment| segment.remote_id.clone())
                .collect();
            if transaction
                .execute(
                    "UPDATE posts SET remote_ids_json = ?2, posted_at = ?3 WHERE alert_id = ?1 AND posted_at = 'IN_PROGRESS'",
                    params![attempt.alert_id, serde_json::to_string(&remote_ids).map_err(CoreError::from)?, decided_at],
                )
                .map_err(CoreError::from)?
                != 1
            {
                return Err(XError::UnsafeReconciliation(
                    "posting reservation is missing or already finalized".to_owned(),
                ));
            }
        }
        "confirm_none_posted" => {
            if !known.is_empty() {
                return Err(XError::UnsafeReconciliation(
                    "one or more segments have recorded remote IDs".to_owned(),
                ));
            }
            if transaction
                .execute(
                    "DELETE FROM posts WHERE alert_id = ?1 AND posted_at = 'IN_PROGRESS'",
                    [&attempt.alert_id],
                )
                .map_err(CoreError::from)?
                != 1
            {
                return Err(XError::UnsafeReconciliation(
                    "posting reservation is missing or already finalized".to_owned(),
                ));
            }
            "reconciled".clone_into(&mut attempt.status);
        }
        "partial" => {
            "partial".clone_into(&mut attempt.status);
        }
        "abandon" => {
            "abandoned".clone_into(&mut attempt.status);
        }
        _ => unreachable!("decision was validated above"),
    }
    Ok(())
}

/// Returns all posting attempts recorded for an alert, oldest first.
pub fn attempts_for_alert(store: &Store, alert_id: &str) -> Result<Vec<XAttempt>, XError> {
    load_attempts_for_alert(store, alert_id)
}

/// Summarizes an attempt and its per-segment state.
pub fn attempt_summary(attempt: &XAttempt) -> String {
    let mut lines = vec![format!(
        "attempt {} · alert {} · status {}",
        attempt.id, attempt.alert_id, attempt.status
    )];
    for segment in &attempt.segments {
        let remote = segment.remote_id.as_deref().unwrap_or("-");
        lines.push(format!(
            "  segment {} · {} · remote {}",
            segment.index, segment.state, remote
        ));
    }
    lines.join("\n")
}

/// Returns a human-readable status for an attempt, including per-segment state.
pub fn attempt_status(store: &Store, attempt_id: &str) -> Result<String, XError> {
    let attempt = store.x_attempt(attempt_id)?;
    Ok(attempt_summary(&attempt))
}

pub struct Credentials(String);

impl fmt::Debug for Credentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Credentials([REDACTED])")
    }
}

impl Credentials {
    pub fn from_runtime() -> Result<Self, XError> {
        if let Ok(value) = std::env::var("X_BEARER_TOKEN")
            && !value.trim().is_empty()
        {
            return Ok(Self(value));
        }
        let path = std::env::var_os("PNUL_X_SECRET_FILE").ok_or(XError::CredentialsMissing)?;
        Self::from_protected_file(&path)
    }

    pub fn from_protected_file(path: impl AsRef<Path>) -> Result<Self, XError> {
        let path = path.as_ref();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(path)
                .map_err(|_| XError::SecretRead)?
                .permissions()
                .mode()
                & 0o777;
            if mode != 0o600 {
                return Err(XError::SecretPermissions);
            }
        }
        let value = fs::read_to_string(path).map_err(|_| XError::SecretRead)?;
        if value.trim().is_empty() {
            Err(XError::CredentialsMissing)
        } else {
            Ok(Self(value.trim().to_owned()))
        }
    }
}

pub struct ReqwestXTransport {
    client: Client,
    credentials: Credentials,
}

impl ReqwestXTransport {
    pub fn new(credentials: Credentials) -> Result<Self, XError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .user_agent(concat!("PanopticonNull/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| XError::Transport)?;
        Ok(Self {
            client,
            credentials,
        })
    }
}

#[derive(Serialize)]
struct TweetRequest<'a> {
    text: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    reply: Option<TweetReply<'a>>,
}

#[derive(Serialize)]
struct TweetReply<'a> {
    in_reply_to_tweet_id: &'a str,
}

#[derive(Deserialize)]
struct TweetResponse {
    data: TweetData,
}

#[derive(Deserialize)]
struct TweetData {
    id: String,
}

impl XTransport for ReqwestXTransport {
    fn submit(&mut self, text: &str, reply_to: Option<&str>) -> Result<String, XError> {
        if text.chars().count() > X_CHARACTER_LIMIT {
            return Err(XError::CharacterLimit);
        }
        let request = TweetRequest {
            text,
            reply: reply_to.map(|id| TweetReply {
                in_reply_to_tweet_id: id,
            }),
        };
        let response = self
            .client
            .post("https://api.x.com/2/tweets")
            .bearer_auth(&self.credentials.0)
            .json(&request)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|_| XError::Transport)?;
        let body: TweetResponse = response.json().map_err(|_| XError::Transport)?;
        if body.data.id.trim().is_empty() {
            return Err(XError::Transport);
        }
        Ok(body.data.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pnull_core::{
        Citation, CoverageState, EvidenceRecord, ExtractionStatus, FindingState, Locator,
        ProcurementAlert, ProcurementChangeKind, ProcurementEvent, ProcurementEventKind,
        ProcurementMatter, ProcurementRecordChange, PublicationAllowlist, ReviewDecision,
        ReviewState, SourceSnapshot, SourceType,
    };
    use tempfile::tempdir;

    struct FakeTransport {
        calls: usize,
    }

    impl XTransport for FakeTransport {
        fn submit(&mut self, text: &str, _reply_to: Option<&str>) -> Result<String, XError> {
            assert!(text.chars().count() <= X_CHARACTER_LIMIT);
            self.calls += 1;
            Ok(format!("fake-{}", self.calls))
        }
    }

    /// Succeeds for the first `succeed_for` submissions, then fails.
    struct FakeFailingTransport {
        calls: usize,
        succeed_for: usize,
    }

    struct BlankIdTransport;

    impl XTransport for BlankIdTransport {
        fn submit(&mut self, _text: &str, _reply_to: Option<&str>) -> Result<String, XError> {
            Ok(String::new())
        }
    }

    impl XTransport for FakeFailingTransport {
        fn submit(&mut self, text: &str, _reply_to: Option<&str>) -> Result<String, XError> {
            assert!(text.chars().count() <= X_CHARACTER_LIMIT);
            if self.calls < self.succeed_for {
                self.calls += 1;
                Ok(format!("fake-{}", self.calls))
            } else {
                Err(XError::Transport)
            }
        }
    }

    fn long_alert() -> Alert {
        alert(&("Evidence-backed change ".repeat(35)))
    }

    fn alert(summary: &str) -> Alert {
        Alert {
            id: "alert:test".to_owned(),
            jurisdiction: "Colorado Springs, Colorado".to_owned(),
            evidence_id:
                "evidence:0000000000000000000000000000000000000000000000000000000000000000"
                    .to_owned(),
            previous_evidence_id: None,
            title: "Police technology agenda item".to_owned(),
            state: FindingState::Approved,
            summary: summary.to_owned(),
            publication_date: "2025-11-25".to_owned(),
            rule_ids: vec!["vendor.axon".to_owned()],
            rules_version: 1,
            rules_digest: "test-rules-digest".to_owned(),
            citations: vec![Citation {
                evidence_id:
                    "evidence:0000000000000000000000000000000000000000000000000000000000000000"
                        .to_owned(),
                source_url: "https://example.invalid/source".to_owned(),
                locator: Locator {
                    kind: "line".to_owned(),
                    start: 1,
                    end: 1,
                    label: "line 1".to_owned(),
                },
                quote: "Action: finally passed".to_owned(),
            }],
            diff: None,
        }
    }

    fn seed(store: &Store, alert: &Alert) {
        let record = EvidenceRecord {
            id: alert.evidence_id.clone(),
            jurisdiction: alert.jurisdiction.clone(),
            source_url: "https://example.invalid/source".to_owned(),
            source_type: SourceType::Agenda,
            document_title: alert.title.clone(),
            publication_date: Some(alert.publication_date.clone()),
            retrieval_timestamp: "2025-11-26T00:00:00Z".to_owned(),
            mime_type: "text/plain".to_owned(),
            sha256: "00".repeat(32),
            original_filename: "source.txt".to_owned(),
            extraction_method: "test".to_owned(),
            extraction_status: ExtractionStatus::Complete,
            extraction_error: None,
            locators: Vec::new(),
            matched_rule_ids: alert.rule_ids.clone(),
            quoted_source_spans: alert.citations.clone(),
            supersedes: None,
            processing_version: "test".to_owned(),
        };
        store.insert_evidence(&record, "Axon").expect("evidence");
        store.insert_alert(alert).expect("alert");
        for citation in &alert.citations {
            let citation_id = pnull_publish::citation_id(citation);
            let decided_at = "2025-11-26T00:00:00Z".to_owned();
            store
                .insert_review(&ReviewDecision {
                    id: ReviewDecision::id_for(&citation_id, &decided_at),
                    citation_id,
                    state: ReviewState::Approved,
                    reviewer: "test".to_owned(),
                    note: "test review".to_owned(),
                    bound_digest: pnull_publish::citation_review_binding(citation).digest(),
                    decision_digest: String::new(),
                    decided_at,
                    supersedes: None,
                })
                .expect("citation review");
        }
    }

    #[test]
    fn drafts_never_exceed_character_limit_and_can_thread() {
        let long = "Evidence-backed change ".repeat(35);
        let draft = draft(&alert(&long), "https://example.invalid/pnull").expect("draft");
        assert!(draft.posts.len() > 1);
        assert!(
            draft
                .posts
                .iter()
                .all(|post| post.chars().count() <= X_CHARACTER_LIMIT)
        );
    }

    #[test]
    fn citation_is_mandatory() {
        let mut without = alert("change");
        without.citations.clear();
        assert!(matches!(
            draft(&without, "https://example.invalid"),
            Err(XError::MissingCitation)
        ));
    }

    #[test]
    fn posting_requires_approval_confirmation_and_is_idempotent() {
        let directory = tempdir().expect("temporary directory");
        let store = Store::open(directory.path()).expect("store");
        let alert = alert("The action changed from referred to finally passed.");
        seed(&store, &alert);
        let draft = draft(&alert, "https://example.invalid/pnull").expect("draft");
        let mut fake = FakeTransport { calls: 0 };
        assert!(matches!(
            post_approved(&store, &draft, true, "2025-11-26T00:00:00Z", &mut fake),
            Err(XError::NotApproved)
        ));
        store
            .approve(&alert.id, &draft.digest(), "2025-11-26T00:00:00Z")
            .expect("approval");
        let mut tampered = draft.clone();
        tampered.posts[0].push_str(" unsupported addition");
        assert!(matches!(
            post_approved(&store, &tampered, true, "2025-11-26T00:00:00Z", &mut fake),
            Err(XError::NotApproved)
        ));
        assert!(matches!(
            post_approved(&store, &draft, false, "2025-11-26T00:00:00Z", &mut fake),
            Err(XError::NotConfirmed)
        ));
        let ids = post_approved(&store, &draft, true, "2025-11-26T00:00:00Z", &mut fake)
            .expect("post through fake transport");
        assert_eq!(ids.len(), draft.posts.len());
        let calls = fake.calls;
        assert!(matches!(
            post_approved(&store, &draft, true, "2025-11-26T00:00:00Z", &mut fake),
            Err(XError::AlreadyPosted)
        ));
        assert_eq!(fake.calls, calls);
    }

    #[test]
    fn attempt_is_recorded_before_posting() {
        let directory = tempdir().expect("temporary directory");
        let store = Store::open(directory.path()).expect("store");
        let alert = long_alert();
        seed(&store, &alert);
        let draft = draft(&alert, "https://example.invalid/pnull").expect("draft");
        assert!(draft.posts.len() > 1);
        store
            .approve(&alert.id, &draft.digest(), "2025-11-26T00:00:00Z")
            .expect("approval");
        let mut fake = FakeTransport { calls: 0 };
        post_with_attempt(&store, &draft, true, "2025-11-26T00:00:00Z", &mut fake).expect("post");
        let attempts = attempts_for_alert(&store, &alert.id).expect("attempts");
        assert_eq!(attempts.len(), 1);
        let attempt = &attempts[0];
        assert_eq!(attempt.status, "complete");
        assert_eq!(attempt.segments.len(), draft.posts.len());
        assert!(
            attempt
                .segments
                .iter()
                .all(|segment| segment.state == "posted" && segment.remote_id.is_some())
        );
    }

    #[test]
    fn partial_post_leaves_uncertain_attempt() {
        let directory = tempdir().expect("temporary directory");
        let store = Store::open(directory.path()).expect("store");
        let alert = long_alert();
        seed(&store, &alert);
        let draft = draft(&alert, "https://example.invalid/pnull").expect("draft");
        assert!(draft.posts.len() > 1);
        store
            .approve(&alert.id, &draft.digest(), "2025-11-26T00:00:00Z")
            .expect("approval");
        let mut failing = FakeFailingTransport {
            calls: 0,
            succeed_for: 1,
        };
        assert!(matches!(
            post_with_attempt(&store, &draft, true, "2025-11-26T00:00:01Z", &mut failing),
            Err(XError::Transport)
        ));
        let attempts = attempts_for_alert(&store, &alert.id).expect("attempts");
        assert_eq!(attempts.len(), 1);
        let attempt = &attempts[0];
        assert_eq!(attempt.status, "uncertain");
        assert_eq!(attempt.segments[0].state, "posted");
        assert!(attempt.segments[0].remote_id.is_some());
        assert_eq!(attempt.segments[1].state, "pending");
        assert!(attempt.segments[1].remote_id.is_none());
        // An uncertain attempt must not be blindly retried.
        let mut fake = FakeTransport { calls: 0 };
        assert!(matches!(
            post_with_attempt(&store, &draft, true, "2025-11-26T00:00:02Z", &mut fake),
            Err(XError::UncertainAttempt)
        ));
        assert_eq!(
            attempts_for_alert(&store, &alert.id)
                .expect("attempts")
                .len(),
            1
        );
    }

    #[test]
    fn reconcile_confirm_none_posted_allows_new_attempt() {
        let directory = tempdir().expect("temporary directory");
        let store = Store::open(directory.path()).expect("store");
        let alert = long_alert();
        seed(&store, &alert);
        let draft = draft(&alert, "https://example.invalid/pnull").expect("draft");
        store
            .approve(&alert.id, &draft.digest(), "2025-11-26T00:00:00Z")
            .expect("approval");
        let mut failing = FakeFailingTransport {
            calls: 0,
            succeed_for: 0,
        };
        assert!(matches!(
            post_with_attempt(&store, &draft, true, "2025-11-26T00:00:01Z", &mut failing),
            Err(XError::Transport)
        ));
        let attempts = attempts_for_alert(&store, &alert.id).expect("attempts");
        assert_eq!(attempts[0].status, "uncertain");
        reconcile(
            &store,
            &attempts[0].id,
            "confirm_none_posted",
            None,
            "op",
            "nothing posted",
            "2025-11-26T00:10:00Z",
        )
        .expect("reconcile");
        let mut fake = FakeTransport { calls: 0 };
        let ids = post_with_attempt(&store, &draft, true, "2025-11-26T00:20:00Z", &mut fake)
            .expect("new attempt after reconcile");
        assert_eq!(ids.len(), draft.posts.len());
        assert_eq!(
            attempts_for_alert(&store, &alert.id)
                .expect("attempts")
                .len(),
            2
        );
    }

    #[test]
    fn reconcile_confirm_posted_records_remote_ids() {
        let directory = tempdir().expect("temporary directory");
        let store = Store::open(directory.path()).expect("store");
        let alert = long_alert();
        seed(&store, &alert);
        let draft = draft(&alert, "https://example.invalid/pnull").expect("draft");
        store
            .approve(&alert.id, &draft.digest(), "2025-11-26T00:00:00Z")
            .expect("approval");
        let mut failing = FakeFailingTransport {
            calls: 0,
            succeed_for: draft.posts.len() - 1,
        };
        assert!(matches!(
            post_with_attempt(&store, &draft, true, "2025-11-26T00:00:01Z", &mut failing),
            Err(XError::Transport)
        ));
        let attempts = attempts_for_alert(&store, &alert.id).expect("attempts");
        let attempt_id = attempts[0].id.clone();
        reconcile(
            &store,
            &attempt_id,
            "confirm_posted",
            Some("manual-remote-2"),
            "op",
            "confirmed posted",
            "2025-11-26T00:10:00Z",
        )
        .expect("reconcile");
        let attempt = store.x_attempt(&attempt_id).expect("attempt");
        assert_eq!(attempt.status, "reconciled");
        assert!(
            attempt
                .segments
                .iter()
                .all(|segment| segment.state == "posted")
        );
        assert_eq!(
            attempt
                .segments
                .last()
                .and_then(|segment| segment.remote_id.as_deref()),
            Some("manual-remote-2")
        );
        assert!(store.is_posted(&alert.id).expect("is posted"));
        let recorded = store.post_segments(&alert.id).expect("segments");
        assert!(recorded.contains(&"manual-remote-2".to_owned()));
    }

    #[test]
    fn reconciliation_cannot_be_rewritten_after_resolution() {
        let directory = tempdir().expect("temporary directory");
        let store = Store::open(directory.path()).expect("store");
        let alert = long_alert();
        seed(&store, &alert);
        let draft = draft(&alert, "https://example.invalid/pnull").expect("draft");
        store
            .approve(&alert.id, &draft.digest(), "2025-11-26T00:00:00Z")
            .expect("approval");
        let mut failing = FakeFailingTransport {
            calls: 0,
            succeed_for: 0,
        };
        assert!(matches!(
            post_with_attempt(&store, &draft, true, "2025-11-26T00:00:01Z", &mut failing),
            Err(XError::Transport)
        ));
        let attempt_id = attempts_for_alert(&store, &alert.id).expect("attempts")[0]
            .id
            .clone();
        reconcile(
            &store,
            &attempt_id,
            "confirm_none_posted",
            None,
            "op",
            "nothing posted",
            "2025-11-26T00:10:00Z",
        )
        .expect("reconcile");
        for (decision, decided_at) in [
            ("confirm_posted", "2025-11-26T00:20:00Z"),
            ("partial", "2025-11-26T00:30:00Z"),
            ("abandon", "2025-11-26T00:40:00Z"),
        ] {
            assert!(matches!(
                reconcile(
                    &store,
                    &attempt_id,
                    decision,
                    None,
                    "op",
                    "note",
                    decided_at
                ),
                Err(XError::UnsafeReconciliation(_))
            ));
            let history = load_reconciliations_for_attempt(&store, &attempt_id).expect("history");
            assert_eq!(history.len(), 1);
        }
        let final_status = store.x_attempt(&attempt_id).expect("attempt").status;
        assert_eq!(final_status, "reconciled");
    }

    #[test]
    fn partial_reconciliation_preserves_posted_segments_and_blocks_retry() {
        let directory = tempdir().expect("temporary directory");
        let store = Store::open(directory.path()).expect("store");
        let alert = long_alert();
        seed(&store, &alert);
        let draft = draft(&alert, "https://example.invalid/pnull").expect("draft");
        assert!(draft.posts.len() > 1);
        store
            .approve(&alert.id, &draft.digest(), "2025-11-26T00:00:00Z")
            .expect("approval");
        let mut failing = FakeFailingTransport {
            calls: 0,
            succeed_for: 1,
        };
        assert!(matches!(
            post_with_attempt(&store, &draft, true, "2025-11-26T00:00:01Z", &mut failing),
            Err(XError::Transport)
        ));
        let attempt_id = attempts_for_alert(&store, &alert.id).expect("attempts")[0]
            .id
            .clone();
        assert!(matches!(
            reconcile(
                &store,
                &attempt_id,
                "confirm_none_posted",
                None,
                "op",
                "none",
                "2025-11-26T00:10:00Z"
            ),
            Err(XError::UnsafeReconciliation(_))
        ));
        assert!(matches!(
            reconcile(
                &store,
                &attempt_id,
                "confirm_posted",
                Some("one-id"),
                "op",
                "all",
                "2025-11-26T00:11:00Z"
            ),
            Err(XError::UnsafeReconciliation(_))
        ));
        assert!(
            load_reconciliations_for_attempt(&store, &attempt_id)
                .expect("history")
                .is_empty()
        );
        reconcile(
            &store,
            &attempt_id,
            "partial",
            None,
            "op",
            "first segment posted, later segments unresolved",
            "2025-11-26T00:12:00Z",
        )
        .expect("partial reconciliation");
        assert_eq!(
            store.post_segments(&alert.id).expect("segments"),
            vec!["fake-1"]
        );
        let mut fake = FakeTransport { calls: 0 };
        assert!(matches!(
            post_with_attempt(&store, &draft, true, "2025-11-26T00:20:00Z", &mut fake),
            Err(XError::UncertainAttempt)
        ));
        assert_eq!(fake.calls, 0);
    }

    #[test]
    fn invalid_reconciliation_never_writes_audit_history() {
        let directory = tempdir().expect("temporary directory");
        let store = Store::open(directory.path()).expect("store");
        let alert = alert("short");
        seed(&store, &alert);
        let draft = draft(&alert, "https://example.invalid/pnull").expect("draft");
        store
            .approve(&alert.id, &draft.digest(), "2025-11-26T00:00:00Z")
            .expect("approval");
        let mut failing = FakeFailingTransport {
            calls: 0,
            succeed_for: 0,
        };
        assert!(matches!(
            post_with_attempt(&store, &draft, true, "2025-11-26T00:00:01Z", &mut failing),
            Err(XError::Transport)
        ));
        let attempt_id = attempts_for_alert(&store, &alert.id).expect("attempts")[0]
            .id
            .clone();
        assert!(matches!(
            reconcile(
                &store,
                &attempt_id,
                "typo",
                None,
                "op",
                "note",
                "2025-11-26T00:10:00Z"
            ),
            Err(XError::UnsupportedDecision(_))
        ));
        assert!(
            load_reconciliations_for_attempt(&store, &attempt_id)
                .expect("history")
                .is_empty()
        );
        assert_eq!(
            store.x_attempt(&attempt_id).expect("attempt").status,
            "uncertain"
        );
    }

    #[test]
    fn contradictory_remote_id_cannot_clear_a_posting_reservation() {
        let directory = tempdir().expect("temporary directory");
        let store = Store::open(directory.path()).expect("store");
        let alert = alert("short");
        seed(&store, &alert);
        let draft = draft(&alert, "https://example.invalid/pnull").expect("draft");
        store
            .approve(&alert.id, &draft.digest(), "2025-11-26T00:00:00Z")
            .expect("approval");
        let mut failing = FakeFailingTransport {
            calls: 0,
            succeed_for: 0,
        };
        assert!(matches!(
            post_with_attempt(&store, &draft, true, "2025-11-26T00:00:01Z", &mut failing),
            Err(XError::Transport)
        ));
        let attempt_id = attempts_for_alert(&store, &alert.id).expect("attempts")[0]
            .id
            .clone();
        for decision in ["confirm_none_posted", "partial", "abandon"] {
            assert!(matches!(
                reconcile(
                    &store,
                    &attempt_id,
                    decision,
                    Some("known-remote-id"),
                    "op",
                    "found a remote ID",
                    "2025-11-26T00:10:00Z"
                ),
                Err(XError::UnsafeReconciliation(_))
            ));
        }
        assert!(
            load_reconciliations_for_attempt(&store, &attempt_id)
                .expect("history")
                .is_empty()
        );
        let mut fake = FakeTransport { calls: 0 };
        assert!(matches!(
            post_with_attempt(&store, &draft, true, "2025-11-26T00:20:00Z", &mut fake),
            Err(XError::UncertainAttempt)
        ));
    }

    #[test]
    fn blank_transport_id_remains_uncertain_and_cannot_be_retried() {
        let directory = tempdir().expect("temporary directory");
        let store = Store::open(directory.path()).expect("store");
        let alert = alert("short");
        seed(&store, &alert);
        let draft = draft(&alert, "https://example.invalid/pnull").expect("draft");
        store
            .approve(&alert.id, &draft.digest(), "2025-11-26T00:00:00Z")
            .expect("approval");
        assert!(matches!(
            post_with_attempt(
                &store,
                &draft,
                true,
                "2025-11-26T00:00:01Z",
                &mut BlankIdTransport
            ),
            Err(XError::Transport)
        ));
        assert_eq!(
            attempts_for_alert(&store, &alert.id).expect("attempts")[0].status,
            "uncertain"
        );
        assert!(store.post_segments(&alert.id).expect("segments").is_empty());
        let mut fake = FakeTransport { calls: 0 };
        assert!(matches!(
            post_with_attempt(&store, &draft, true, "2025-11-26T00:20:00Z", &mut fake),
            Err(XError::UncertainAttempt)
        ));
    }

    #[test]
    fn empty_or_unsafe_approved_thread_cannot_be_marked_posted() {
        let directory = tempdir().expect("temporary directory");
        let store = Store::open(directory.path()).expect("store");
        let alert = alert("short");
        seed(&store, &alert);
        let mut fake = FakeTransport { calls: 0 };
        for posts in [
            vec![],
            vec![" ".to_owned()],
            vec!["x".repeat(X_CHARACTER_LIMIT + 1)],
            vec!["Plate: ABC123".to_owned()],
        ] {
            let draft = Draft {
                alert_id: alert.id.clone(),
                posts,
            };
            store
                .connection()
                .execute(
                    "INSERT INTO approvals(alert_id, draft_digest, approved_at) VALUES (?1, ?2, ?3)
                     ON CONFLICT(alert_id) DO UPDATE SET draft_digest=excluded.draft_digest",
                    params![alert.id, draft.digest(), "2025-11-26T00:00:00Z"],
                )
                .expect("approval");
            assert!(
                post_with_attempt(&store, &draft, true, "2025-11-26T00:00:01Z", &mut fake).is_err()
            );
        }
        assert_eq!(fake.calls, 0);
        assert!(!store.is_posted(&alert.id).expect("post state"));
    }

    #[test]
    fn revoked_ordinary_citation_blocks_post_before_transport() {
        let directory = tempdir().expect("temporary directory");
        let store = Store::open(directory.path()).expect("store");
        let alert = alert("short");
        seed(&store, &alert);
        let draft = draft(&alert, "https://example.invalid/pnull").expect("draft");
        store
            .approve(&alert.id, &draft.digest(), "2025-11-26T00:00:00Z")
            .expect("approval");
        let citation = &alert.citations[0];
        let citation_id = pnull_publish::citation_id(citation);
        let decided_at = "2025-11-26T01:00:00Z".to_owned();
        store
            .insert_review(&ReviewDecision {
                id: ReviewDecision::id_for(&citation_id, &decided_at),
                citation_id,
                state: ReviewState::Rejected,
                reviewer: "test".to_owned(),
                note: "revoked".to_owned(),
                bound_digest: pnull_publish::citation_review_binding(citation).digest(),
                decision_digest: String::new(),
                decided_at,
                supersedes: None,
            })
            .expect("revocation");
        let mut fake = FakeTransport { calls: 0 };
        assert!(matches!(
            post_with_attempt(&store, &draft, true, "2025-11-26T02:00:00Z", &mut fake),
            Err(XError::Publish(_))
        ));
        assert_eq!(fake.calls, 0);
    }

    #[test]
    fn segment_record_and_attempt_state_commit_together() {
        let directory = tempdir().expect("temporary directory");
        let store = Store::open(directory.path()).expect("store");
        let alert = alert("short");
        seed(&store, &alert);
        let draft = draft(&alert, "https://example.invalid/pnull").expect("draft");
        store
            .approve(&alert.id, &draft.digest(), "2025-11-26T00:00:00Z")
            .expect("approval");
        store
            .connection()
            .execute_batch(
                "CREATE TRIGGER fail_attempt_update BEFORE UPDATE ON x_attempts
                 BEGIN SELECT RAISE(ABORT, 'injected attempt write failure'); END;",
            )
            .expect("trigger");
        let mut transport = FakeTransport { calls: 0 };
        assert!(
            post_with_attempt(&store, &draft, true, "2025-11-26T00:00:01Z", &mut transport)
                .is_err()
        );
        assert_eq!(transport.calls, 1);
        assert!(store.post_segments(&alert.id).expect("segments").is_empty());
        assert_eq!(
            attempts_for_alert(&store, &alert.id).expect("attempts")[0].status,
            "in_progress"
        );
    }

    #[test]
    fn final_attempt_and_post_status_commit_together() {
        let directory = tempdir().expect("temporary directory");
        let store = Store::open(directory.path()).expect("store");
        let alert = alert("short");
        seed(&store, &alert);
        let draft = draft(&alert, "https://example.invalid/pnull").expect("draft");
        store
            .approve(&alert.id, &draft.digest(), "2025-11-26T00:00:00Z")
            .expect("approval");
        store
            .connection()
            .execute_batch(
                "CREATE TRIGGER fail_post_update BEFORE UPDATE ON posts
                 BEGIN SELECT RAISE(ABORT, 'injected post write failure'); END;",
            )
            .expect("trigger");
        let mut transport = FakeTransport { calls: 0 };
        assert!(
            post_with_attempt(&store, &draft, true, "2025-11-26T00:00:01Z", &mut transport)
                .is_err()
        );
        assert_eq!(transport.calls, draft.posts.len());
        assert_eq!(
            attempts_for_alert(&store, &alert.id).expect("attempts")[0].status,
            "in_progress"
        );
        assert_eq!(
            store.post_segments(&alert.id).expect("segments").len(),
            draft.posts.len()
        );
    }

    #[test]
    fn attempt_status_reports_segment_states() {
        let directory = tempdir().expect("temporary directory");
        let store = Store::open(directory.path()).expect("store");
        let alert = long_alert();
        seed(&store, &alert);
        let draft = draft(&alert, "https://example.invalid/pnull").expect("draft");
        store
            .approve(&alert.id, &draft.digest(), "2025-11-26T00:00:00Z")
            .expect("approval");
        let mut failing = FakeFailingTransport {
            calls: 0,
            succeed_for: 1,
        };
        assert!(matches!(
            post_with_attempt(&store, &draft, true, "2025-11-26T00:00:01Z", &mut failing),
            Err(XError::Transport)
        ));
        let attempt_id = attempts_for_alert(&store, &alert.id).expect("attempts")[0]
            .id
            .clone();
        let status = attempt_status(&store, &attempt_id).expect("status");
        assert!(status.contains("uncertain"));
        assert!(status.contains("segment 0"));
        assert!(status.contains("posted"));
        assert!(status.contains("segment 1"));
        assert!(status.contains("pending"));
    }

    #[test]
    fn credentials_are_always_redacted() {
        let credentials = Credentials("do-not-print".to_owned());
        assert_eq!(format!("{credentials:?}"), "Credentials([REDACTED])");
        assert!(!XError::Transport.to_string().contains("do-not-print"));
    }

    #[test]
    fn sensitive_summary_is_rejected() {
        assert!(matches!(
            draft(&alert("Plate: ABC123"), "https://example.invalid"),
            Err(XError::SensitiveData)
        ));
    }

    fn procurement_alert() -> ProcurementAlert {
        let source_id = "colorado-springs-contract-awards";
        let old_digest = pnull_core::sha256_hex(b"<main><p>Previous record</p></main>");
        let new_digest = pnull_core::sha256_hex(b"<main><p>Test row changed</p></main>");
        let old_snapshot_id = SourceSnapshot::id_for(source_id, &old_digest);
        let new_snapshot_id = SourceSnapshot::id_for(source_id, &new_digest);
        ProcurementAlert {
            id: "proc-alert:test".to_owned(),
            source_id: source_id.to_owned(),
            surface: "contract-award-table".to_owned(),
            old_snapshot_id: old_snapshot_id.clone(),
            old_snapshot_digest: old_digest.clone(),
            new_snapshot_id: new_snapshot_id.clone(),
            new_snapshot_digest: new_digest.clone(),
            changes: vec![ProcurementRecordChange {
                change_kind: ProcurementChangeKind::RecordAdded,
                row_identity: "proc:matter:co:abc".to_owned(),
                field_diffs: Vec::new(),
                old_snapshot_id,
                old_snapshot_digest: old_digest,
                new_snapshot_id,
                new_snapshot_digest: new_digest,
                summary: "The row observed in snapshot snapshot:a (digest aaaaa...) is not present in snapshot snapshot:b (digest bbbbb...).".to_owned(),
            }],
            retrieved_at: "2026-08-31T00:00:00Z".to_owned(),
            coverage_state: CoverageState::InformationalOnly,
            matter_ids: vec!["proc:matter:co:abc".to_owned()],
            identifier_ids: Vec::new(),
            taxonomy_matches: Vec::new(),
            summary: "The row observed in snapshot snapshot:a (digest aaaaa...) is not present in snapshot snapshot:b (digest bbbbb...).".to_owned(),
        }
    }

    fn procurement_citation(alert: &ProcurementAlert) -> Citation {
        let event_id = ProcurementEvent::id_for(
            &alert.matter_ids[0],
            ProcurementEventKind::RecordCorrected,
            "2026-08-31",
            "Test row changed",
        );
        Citation {
            evidence_id: alert.new_snapshot_id.clone(),
            source_url: "https://example.test/awards".to_owned(),
            locator: Locator {
                kind: "html-element".to_owned(),
                start: 1,
                end: 1,
                label: format!("event:{event_id};visible-element[1]/p"),
            },
            quote: "Test row changed".to_owned(),
        }
    }

    fn published_procurement_alert(store: &Store) -> ProcurementAlert {
        let mut alert = procurement_alert();
        let matter_id = alert.matter_ids[0].clone();
        alert.id = ProcurementAlert::id_for(
            &alert.source_id,
            &alert.surface,
            &alert.changes[0].row_identity,
            alert.changes[0].change_kind,
            &alert.old_snapshot_id,
            &alert.new_snapshot_id,
        );
        store
            .insert_procurement_matter(&ProcurementMatter {
                id: matter_id.clone(),
                jurisdiction: "Colorado Springs".to_owned(),
                title: "Test Matter".to_owned(),
                review_state: "draft".to_owned(),
                publication_state: "unpublished".to_owned(),
            })
            .expect("matter");
        for (id, digest) in [
            (&alert.old_snapshot_id, &alert.old_snapshot_digest),
            (&alert.new_snapshot_id, &alert.new_snapshot_digest),
        ] {
            let bytes: &[u8] = if id == &alert.old_snapshot_id {
                b"<main><p>Previous record</p></main>"
            } else {
                b"<main><p>Test row changed</p></main>"
            };
            store.persist_content(digest, bytes).expect("source bytes");
            store
                .insert_source_snapshot(&SourceSnapshot {
                    id: id.clone(),
                    source_id: alert.source_id.clone(),
                    source_url: "https://example.test/awards".to_owned(),
                    retrieved_at: alert.retrieved_at.clone(),
                    persisted_digest: digest.clone(),
                    content_type: Some("text/html".to_owned()),
                    etag: None,
                    last_modified: None,
                    final_url: "https://example.test/awards".to_owned(),
                    redirect_history: Vec::new(),
                    parser_version: "awards-1.0".to_owned(),
                    schema_version: 2,
                    record_count: Some(1),
                    pagination_complete: Some(true),
                    coverage_state: CoverageState::InformationalOnly,
                    supersedes: None,
                })
                .expect("snapshot");
        }
        store
            .insert_procurement_event(&ProcurementEvent {
                id: ProcurementEvent::id_for(
                    &matter_id,
                    ProcurementEventKind::RecordCorrected,
                    "2026-08-31",
                    "Test row changed",
                ),
                matter_id: matter_id.clone(),
                kind: ProcurementEventKind::RecordCorrected,
                date: Some("2026-08-31".to_owned()),
                summary: "Test row changed".to_owned(),
                identifier_ids: Vec::new(),
                evidence_ids: vec![alert.new_snapshot_id.clone()],
                source_id: alert.source_id.clone(),
            })
            .expect("event");
        store
            .insert_publication_allowlist(&PublicationAllowlist {
                id: "allowlist:x-test".to_owned(),
                field_categories: vec!["procurement_casefile".to_owned()],
                created_at: "2026-08-30T00:00:00Z".to_owned(),
                note: "test".to_owned(),
            })
            .expect("allowlist");
        let citation = procurement_citation(&alert);
        let citation_id = pnull_publish::citation_id(&citation);
        let decided_at = "2026-08-30T00:00:00Z".to_owned();
        store
            .insert_review(&ReviewDecision {
                id: ReviewDecision::id_for(&citation_id, &decided_at),
                citation_id,
                state: ReviewState::Approved,
                reviewer: "test".to_owned(),
                note: String::new(),
                bound_digest: pnull_publish::citation_review_binding(&citation).digest(),
                decision_digest: String::new(),
                decided_at,
                supersedes: None,
            })
            .expect("review");
        store.insert_procurement_alert(&alert).expect("alert");
        alert
    }

    #[test]
    fn procurement_draft_links_the_matter_case_file_page() {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("store");
        let alert = published_procurement_alert(&store);
        let draft = draft_procurement(
            &store,
            &alert,
            "https://example.invalid/base",
            "Colorado Springs, Colorado",
            "proc:matter:co:abc",
        )
        .expect("draft");
        let text = draft.posts.join("\n");
        assert!(
            text.contains("AUTOMATED PUBLIC-RECORD CHANGE NOTICE · procurement"),
            "the draft must identify the feed as automated"
        );
        assert!(
            text.contains("/procurement/proc_matter_co_abc/index.html"),
            "the draft must link the matter case-file page (slug from the matter id), got: {text}"
        );
        assert!(
            text.contains("not proof of absence"),
            "the draft must carry the absence caveat"
        );
    }

    #[test]
    fn procurement_draft_refuses_withheld_matter() {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("store");
        let alert = published_procurement_alert(&store);
        let matter = alert.matter_ids[0].clone();
        store
            .insert_procurement_event(&ProcurementEvent {
                id: ProcurementEvent::id_for(
                    &matter,
                    ProcurementEventKind::RecordCorrected,
                    "2026-09-01",
                    "New unreviewed change",
                ),
                matter_id: matter.clone(),
                kind: ProcurementEventKind::RecordCorrected,
                date: Some("2026-09-01".to_owned()),
                summary: "New unreviewed change".to_owned(),
                identifier_ids: Vec::new(),
                evidence_ids: vec![alert.new_snapshot_id.clone()],
                source_id: alert.source_id.clone(),
            })
            .expect("new event");
        let result = draft_procurement(
            &store,
            &alert,
            "https://example.invalid/base",
            "Colorado Springs, Colorado",
            &matter,
        );
        assert!(matches!(result, Err(XError::ProcurementNotPublished)));
    }

    #[test]
    fn procurement_post_refuses_revoked_review_before_transport() {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("store");
        let alert = published_procurement_alert(&store);
        let draft = draft_procurement(
            &store,
            &alert,
            "https://example.invalid/base",
            "Colorado Springs, Colorado",
            &alert.matter_ids[0],
        )
        .expect("draft");
        let citation = procurement_citation(&alert);
        let citation_id = pnull_publish::citation_id(&citation);
        let decided_at = "2026-09-02T00:00:00Z".to_owned();
        store
            .insert_review(&ReviewDecision {
                id: ReviewDecision::id_for(&citation_id, &decided_at),
                citation_id,
                state: ReviewState::Rejected,
                reviewer: "test".to_owned(),
                note: "revoked".to_owned(),
                bound_digest: pnull_publish::citation_review_binding(&citation).digest(),
                decision_digest: String::new(),
                decided_at,
                supersedes: None,
            })
            .expect("revoke");
        let mut fake = FakeTransport { calls: 0 };
        assert!(matches!(
            post_approved(&store, &draft, true, "2026-09-03T00:00:00Z", &mut fake),
            Err(XError::ProcurementNotPublished)
        ));
        assert_eq!(fake.calls, 0);
    }
}
