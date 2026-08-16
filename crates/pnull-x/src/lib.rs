//! State-aware X drafting with explicit approval, confirmation, and idempotent transport.

use std::fmt;
use std::fs;
use std::path::Path;

use pnull_core::{Alert, CoreError, Store, stable_id};
use reqwest::blocking::Client;
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
    if !confirmed {
        return Err(XError::NotConfirmed);
    }
    if store.approved_draft_digest(&draft.alert_id)?.as_deref() != Some(draft.digest().as_str()) {
        return Err(XError::NotApproved);
    }
    if store.is_posted(&draft.alert_id)? || !store.reserve_post(&draft.alert_id)? {
        return Err(XError::AlreadyPosted);
    }
    let mut remote_ids = Vec::new();
    for (index, post) in draft.posts.iter().enumerate() {
        let reply_to = remote_ids.last().map(String::as_str);
        let remote_id = transport.submit(post, reply_to)?;
        store.record_post_segment(&draft.alert_id, index, &remote_id)?;
        remote_ids.push(remote_id);
    }
    store.mark_posted(&draft.alert_id, &remote_ids, posted_at)?;
    Ok(remote_ids)
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
        Citation, EvidenceRecord, ExtractionStatus, FindingState, Locator, SourceType,
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
}
