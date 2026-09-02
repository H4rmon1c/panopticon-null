//! State-aware X drafting with explicit approval, confirmation, and idempotent transport.

use std::fmt;
use std::fs;
use std::path::Path;

use pnull_core::{
    Alert, CoreError, ProcurementAlert, Store, XAttempt, XReconciliation, XSegment, stable_id,
};
use reqwest::blocking::Client;
use rusqlite::params;
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
    #[error("X credentials are not configured")]
    CredentialsMissing,
    #[error("the runtime secret file is not protected with mode 0600")]
    SecretPermissions,
    #[error("X transport failed; credentials and response content were redacted")]
    Transport,
    #[error(transparent)]
    Core(#[from] CoreError),
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
/// not proof of absence" caveat.
pub fn draft_procurement(
    alert: &ProcurementAlert,
    canonical_base_url: &str,
    jurisdiction: &str,
    matter_label: &str,
) -> Result<Draft, XError> {
    pnull_publish::validate_public_text(&alert.summary).map_err(|_| XError::SensitiveData)?;
    let first = alert.changes.first().ok_or(XError::MissingCitation)?;
    let kind = first.change_kind.label();
    // The published case-file page for a procurement change is the affected
    // matter's page (deterministic slug from the matter id). The canonical base
    // URL already carries the site path prefix (e.g. `.../co`), so the
    // `procurement/` segment is appended directly, matching how the general
    // alert draft appends `/alerts/...`. When no matter id is resolvable, fall
    // back to the change-alert page.
    let case_page = match alert.matter_ids.first() {
        Some(matter_id) if !matter_id.is_empty() => format!(
            "{}/procurement/{}/index.html",
            canonical_base_url.trim_end_matches('/'),
            safe_id(matter_id)
        ),
        _ => format!(
            "{}/procurement/change-alerts/{}.html",
            canonical_base_url.trim_end_matches('/'),
            safe_id(&alert.id)
        ),
    };
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
    store.insert_x_attempt(&attempt)?;

    if !store.reserve_post(&draft.alert_id)? {
        return Err(XError::AlreadyPosted);
    }

    let mut remote_ids = Vec::new();
    let mut failure: Option<XError> = None;
    for (index, post) in draft.posts.iter().enumerate() {
        let reply_to = remote_ids.last().map(String::as_str);
        match transport.submit(post, reply_to) {
            Ok(remote_id) => {
                store.record_post_segment(&draft.alert_id, index, &remote_id)?;
                if let Some(segment) = attempt
                    .segments
                    .iter_mut()
                    .find(|segment| segment.index as usize == index)
                {
                    segment.remote_id = Some(remote_id.clone());
                    "posted".clone_into(&mut segment.state);
                }
                persist_attempt(store, &attempt)?;
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
    persist_attempt(store, &attempt)?;
    store.mark_posted(&draft.alert_id, &remote_ids, posted_at)?;
    Ok(remote_ids)
}

/// Refuses a new posting attempt while the most recent attempt for the alert
/// is unresolved (`"uncertain"` or `"in_progress"`), so an uncertain attempt is
/// never blindly retried before an operator reconciles it.
fn guard_against_blind_retry(store: &Store, alert_id: &str) -> Result<(), XError> {
    let attempts = load_attempts_for_alert(store, alert_id)?;
    if let Some(latest) = attempts.last()
        && matches!(latest.status.as_str(), "uncertain" | "in_progress")
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
    let mut attempt = store.x_attempt(attempt_id)?;
    let item = XReconciliation {
        id: stable_id("x-reconcile", &[attempt_id, decided_at]),
        attempt_id: attempt_id.to_owned(),
        decision: decision.to_owned(),
        remote_id: remote_id.map(str::to_owned),
        note: note.to_owned(),
        operator: operator.to_owned(),
        decided_at: decided_at.to_owned(),
    };
    store.insert_x_reconciliation(&item)?;

    match decision {
        "confirm_posted" => {
            if let Some(remote_id) = remote_id
                && let Some(segment) = attempt
                    .segments
                    .iter_mut()
                    .find(|segment| segment.remote_id.is_none())
            {
                segment.remote_id = Some(remote_id.to_owned());
                store.record_post_segment(
                    &attempt.alert_id,
                    usize::try_from(segment.index).unwrap_or(usize::MAX),
                    remote_id,
                )?;
            }
            for segment in &mut attempt.segments {
                "posted".clone_into(&mut segment.state);
            }
            "reconciled".clone_into(&mut attempt.status);
            if !store.is_posted(&attempt.alert_id)? {
                let remote_ids: Vec<String> = attempt
                    .segments
                    .iter()
                    .filter_map(|segment| segment.remote_id.clone())
                    .collect();
                store.mark_posted(&attempt.alert_id, &remote_ids, decided_at)?;
            }
        }
        "confirm_none_posted" => {
            clear_in_progress_reservation(store, &attempt.alert_id)?;
            "reconciled".clone_into(&mut attempt.status);
        }
        "partial" => {
            clear_in_progress_reservation(store, &attempt.alert_id)?;
            "partial".clone_into(&mut attempt.status);
        }
        "abandon" => {
            "abandoned".clone_into(&mut attempt.status);
        }
        other => return Err(XError::UnsupportedDecision(other.to_owned())),
    }
    persist_attempt(store, &attempt)?;
    Ok(item)
}

/// Removes an un-finalized posting reservation (a `posts` row still marked
/// `IN_PROGRESS`) and its segment rows so a fresh attempt may begin after a
/// reconciliation that concluded nothing was (or should be) permanently posted.
fn clear_in_progress_reservation(store: &Store, alert_id: &str) -> Result<(), XError> {
    store.transaction(|transaction| {
        transaction.execute(
            "DELETE FROM post_segments WHERE alert_id = ?1",
            params![alert_id],
        )?;
        transaction.execute(
            "DELETE FROM posts WHERE alert_id = ?1 AND posted_at = 'IN_PROGRESS'",
            params![alert_id],
        )?;
        Ok(())
    })?;
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
        Ok(body.data.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pnull_core::{
        Citation, CoverageState, EvidenceRecord, ExtractionStatus, FindingState, Locator,
        ProcurementAlert, ProcurementChangeKind, ProcurementRecordChange, SourceType,
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
            succeed_for: 1,
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
            attempt.segments[1].remote_id.as_deref(),
            Some("manual-remote-2")
        );
        assert!(store.is_posted(&alert.id).expect("is posted"));
        let recorded = store.post_segments(&alert.id).expect("segments");
        assert!(recorded.contains(&"manual-remote-2".to_owned()));
    }

    #[test]
    fn reconcile_is_append_only_and_does_not_delete_history() {
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
        for (index, (decision, decided_at)) in [
            ("confirm_none_posted", "2025-11-26T00:10:00Z"),
            ("confirm_posted", "2025-11-26T00:20:00Z"),
            ("partial", "2025-11-26T00:30:00Z"),
            ("abandon", "2025-11-26T00:40:00Z"),
        ]
        .iter()
        .enumerate()
        {
            reconcile(
                &store,
                &attempt_id,
                decision,
                None,
                "op",
                "note",
                decided_at,
            )
            .expect("reconcile");
            let history = load_reconciliations_for_attempt(&store, &attempt_id).expect("history");
            assert_eq!(history.len(), index + 1);
        }
        let final_status = store.x_attempt(&attempt_id).expect("attempt").status;
        assert_eq!(final_status, "abandoned");
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
        ProcurementAlert {
            id: "proc-alert:test".to_owned(),
            source_id: "colorado-springs-contract-awards".to_owned(),
            surface: "contract-award-table".to_owned(),
            old_snapshot_id: "snapshot:a".to_owned(),
            old_snapshot_digest: "a".repeat(64),
            new_snapshot_id: "snapshot:b".to_owned(),
            new_snapshot_digest: "b".repeat(64),
            changes: vec![ProcurementRecordChange {
                change_kind: ProcurementChangeKind::RecordAdded,
                row_identity: "proc:matter:co:abc".to_owned(),
                field_diffs: Vec::new(),
                old_snapshot_id: "snapshot:a".to_owned(),
                old_snapshot_digest: "a".repeat(64),
                new_snapshot_id: "snapshot:b".to_owned(),
                new_snapshot_digest: "b".repeat(64),
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

    #[test]
    fn procurement_draft_links_the_matter_case_file_page() {
        let draft = draft_procurement(
            &procurement_alert(),
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
    fn procurement_draft_links_change_alert_page_when_no_matter() {
        let mut alert = procurement_alert();
        alert.matter_ids.clear();
        let draft = draft_procurement(
            &alert,
            "https://example.invalid/base",
            "Colorado Springs, Colorado",
            "proc:matter:co:abc",
        )
        .expect("draft");
        assert!(
            draft
                .posts
                .join("\n")
                .contains("/procurement/change-alerts/proc-alert_test.html")
        );
    }
}
