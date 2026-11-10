//! Archive a configured official HTML page, PDF, or JSON record as private bytes.
//!
//! This deliberately stops before extraction, claim creation, or publication.
//! A source must have a current review bound to a preserved robots response,
//! its exact configuration, host, and path. The shared fetcher enforces DNS and
//! response-size bounds and retains conditional-fetch provenance.

use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::net::IpAddr;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Component;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use pnull_core::{
    CandidateQueueItem, CandidateQueueOutcome, CandidateQueueSeed, FetchObservation, Store,
    sha256_hex,
};
use pnull_http::{FetchConfig, PriorEvidence};
use pnull_ingest::fetch_source;
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::{SourceConfig, StateConfig, source_config_digest, verify_source_review};

const WORDPRESS_PDF_PREFIX: &str = "/wp-content/uploads/";
const TOWNCLOUD_PDF_PREFIX: &str = "/uploads/";
const MAX_QUEUE_CANDIDATES: usize = 10_000;
const MAX_QUEUE_APPEARANCES: usize = 20_000;
const MAX_QUEUE_FETCHES: usize = 8;
const MAX_QUEUE_INDEX_WINDOW: usize = 256;

fn town_index_path_allowed(path: &str) -> bool {
    matches!(path, "/agendas-and-minutes/" | "/documents")
}

fn town_pdf_prefix_allowed(path: &str) -> bool {
    matches!(path, WORDPRESS_PDF_PREFIX | TOWNCLOUD_PDF_PREFIX)
}

fn town_index_pdf_policy<'a>(
    config: &'a StateConfig,
    index_url: &url::Url,
) -> Result<&'a SourceConfig> {
    let mut policies = config.sources.iter().filter(|source| {
        source.source_type == "candidate_index_pdf"
            && source.official_discovery_url == index_url.as_str()
            && source.documented_at == index_url.as_str()
    });
    let policy = policies
        .next()
        .context("document index lacks a matching PDF path policy")?;
    if policies.next().is_some() {
        bail!("document index has multiple PDF path policies");
    }
    let policy_url = url::Url::parse(&policy.url)?;
    if !town_pdf_prefix_allowed(policy_url.path())
        || policy_url.scheme() != "https"
        || policy_url.host_str() != index_url.host_str()
        || policy_url.port_or_known_default() != Some(443)
        || policy_url.query().is_some()
        || policy_url.fragment().is_some()
        || !policy_url.username().is_empty()
        || policy_url.password().is_some()
    {
        bail!("document index PDF policy is outside the approved same-host paths");
    }
    Ok(policy)
}

trait ArchiveFetcher {
    fn fetch(
        &mut self,
        store: &Store,
        source_id: &str,
        config: &FetchConfig,
        url: &str,
        retrieved_at: &str,
        prior: Option<&PriorEvidence>,
    ) -> Result<(Vec<u8>, Vec<FetchObservation>)>;
}

struct LiveFetcher;

impl ArchiveFetcher for LiveFetcher {
    fn fetch(
        &mut self,
        store: &Store,
        source_id: &str,
        config: &FetchConfig,
        url: &str,
        retrieved_at: &str,
        prior: Option<&PriorEvidence>,
    ) -> Result<(Vec<u8>, Vec<FetchObservation>)> {
        fetch_source(store, Some(source_id), config, url, retrieved_at, prior).map_err(Into::into)
    }
}

#[derive(Debug)]
struct ArchiveOutcome {
    digest: String,
    bytes: usize,
    unchanged: bool,
    status: u16,
    final_url: String,
    observations: usize,
    receipt_id: String,
    retrieved_at: String,
    review_id: String,
    changed_from: Option<String>,
}

pub(super) fn archive_source(data_dir: &Path, config: &StateConfig, source_id: &str) -> Result<()> {
    let store = Store::open(data_dir)?;
    let outcome = archive_with(&store, config, source_id, &mut LiveFetcher)?;
    println!(
        "{} source {} (SHA-256 {}, {} bytes; HTTP {}; {} fetch observations)\n  URL: {}\n  private blob: {}",
        if outcome.unchanged {
            "unchanged"
        } else {
            "archived"
        },
        source_id,
        outcome.digest,
        outcome.bytes,
        outcome.status,
        outcome.observations,
        outcome.final_url,
        store.content_path(&outcome.digest)?.display(),
    );
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct CalendarCandidate {
    kind: String,
    url: String,
    /// Zero-based position of this anchor among all anchors in the exact HTML snapshot.
    anchor_index: usize,
    label: String,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
struct CalendarCandidates {
    schema_version: u8,
    source_id: String,
    source_url: String,
    snapshot_sha256: String,
    candidates: Vec<CalendarCandidate>,
}

/// A calendar is a discovery surface, not evidence that any listed item passed.
/// The command retains the raw page plus an immutable private candidate list;
/// following a candidate requires its own reviewed source and bounded fetch.
pub(super) fn discover_calendar(
    data_dir: &Path,
    config: &StateConfig,
    source_id: &str,
) -> Result<()> {
    if source_id.is_empty()
        || !source_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        bail!("invalid calendar source ID");
    }
    let source = config
        .sources
        .iter()
        .find(|source| source.id == source_id)
        .ok_or_else(|| anyhow::anyhow!("unknown source: {source_id}"))?;
    let source_url = url::Url::parse(&source.url)?;
    if source.source_type != "html_page"
        || !source_url.path().eq_ignore_ascii_case("/Calendar.aspx")
        || source_url.query().is_some()
    {
        bail!("calendar discovery requires a configured official Calendar.aspx HTML source");
    }
    let store = Store::open(data_dir)?;
    let outcome = archive_with(&store, config, source_id, &mut LiveFetcher)?;
    let mut raw = Vec::new();
    fs::File::open(store.content_path(&outcome.digest)?)?
        .take((source.maximum_bytes as u64).saturating_add(1))
        .read_to_end(&mut raw)?;
    if raw.len() > source.maximum_bytes || sha256_hex(&raw) != outcome.digest {
        bail!("archived calendar bytes do not match their digest");
    }
    let candidates = parse_calendar_candidates(&source_url, &raw)?;
    let manifest = CalendarCandidates {
        schema_version: 1,
        source_id: source_id.to_owned(),
        source_url: source.url.clone(),
        snapshot_sha256: outcome.digest.clone(),
        candidates,
    };
    let path = persist_calendar_candidates(data_dir, &manifest)?;
    println!(
        "discovered {} private meeting/document candidates from {} (SHA-256 {})\n  artifact: {}",
        manifest.candidates.len(),
        source_id,
        outcome.digest,
        path.display()
    );
    Ok(())
}

/// Small-town HTML indexes can expose official PDF records without an API.
/// Only links on the same host and under a separately reviewed upload path
/// become private candidates. Nothing here implies a vote or a public case.
pub(super) fn discover_index(data_dir: &Path, config: &StateConfig, source_id: &str) -> Result<()> {
    if source_id.is_empty()
        || !source_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        bail!("invalid document index source ID");
    }
    let source = config
        .sources
        .iter()
        .find(|source| source.id == source_id)
        .ok_or_else(|| anyhow::anyhow!("unknown source: {source_id}"))?;
    let source_url = url::Url::parse(&source.url)?;
    if source.source_type != "html_page"
        || source_url.scheme() != "https"
        || source_url.port_or_known_default() != Some(443)
        || !town_index_path_allowed(source_url.path())
        || source_url.query().is_some()
        || source_url.fragment().is_some()
        || !source_url.username().is_empty()
        || source_url.password().is_some()
    {
        bail!(
            "document index discovery requires a configured official agendas-and-minutes HTML source"
        );
    }
    let policy_url = url::Url::parse(&town_index_pdf_policy(config, &source_url)?.url)?;
    let store = Store::open(data_dir)?;
    let outcome = archive_with(&store, config, source_id, &mut LiveFetcher)?;
    let mut raw = Vec::new();
    fs::File::open(store.content_path(&outcome.digest)?)?
        .take((source.maximum_bytes as u64).saturating_add(1))
        .read_to_end(&mut raw)?;
    if raw.is_empty() || raw.len() > source.maximum_bytes || sha256_hex(&raw) != outcome.digest {
        bail!("archived document index bytes do not match their digest");
    }
    let manifest = CalendarCandidates {
        schema_version: 1,
        source_id: source_id.to_owned(),
        source_url: source.url.clone(),
        snapshot_sha256: outcome.digest,
        candidates: parse_town_pdf_candidates(&source_url, &policy_url, &raw)?,
    };
    let path = persist_calendar_candidates(data_dir, &manifest)?;
    println!(
        "discovered {} private PDF candidates from {} (SHA-256 {})\n  artifact: {}",
        manifest.candidates.len(),
        source_id,
        manifest.snapshot_sha256,
        path.display()
    );
    Ok(())
}

fn town_pdf_url_allowed(policy_url: &url::Url, candidate_url: &url::Url) -> bool {
    candidate_url.scheme() == "https"
        && candidate_url.host_str() == policy_url.host_str()
        && candidate_url.port_or_known_default() == Some(443)
        && candidate_url.username().is_empty()
        && candidate_url.password().is_none()
        && candidate_url.query().is_none()
        && candidate_url.fragment().is_none()
        && town_pdf_prefix_allowed(policy_url.path())
        && candidate_url.path().starts_with(policy_url.path())
        && candidate_url.path().len() > policy_url.path().len()
        && !candidate_url.path().contains(['%', '\\'])
        && !candidate_url.path().contains("//")
        && candidate_url.path().to_ascii_lowercase().ends_with(".pdf")
        && candidate_url.as_str().len() <= 2_048
}

fn parse_town_pdf_candidates(
    source_url: &url::Url,
    policy_url: &url::Url,
    raw: &[u8],
) -> Result<Vec<CalendarCandidate>> {
    let html = std::str::from_utf8(raw).context("document index HTML is not UTF-8")?;
    let document = Html::parse_document(html);
    let anchors = Selector::parse("a[href]").expect("static selector is valid");
    let mut candidates = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (anchor_index, anchor) in document.select(&anchors).enumerate() {
        let Some(href) = anchor.value().attr("href") else {
            continue;
        };
        let Ok(url) = source_url.join(href) else {
            continue;
        };
        if !town_pdf_url_allowed(policy_url, &url) || !seen.insert(url.to_string()) {
            continue;
        }
        if candidates.len() >= 2_000 {
            bail!("document index has more than 2,000 eligible PDF links");
        }
        let label = anchor
            .parent()
            .and_then(ElementRef::wrap)
            .unwrap_or(anchor)
            .text()
            .flat_map(str::split_whitespace)
            .take(24)
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(160)
            .collect();
        candidates.push(CalendarCandidate {
            kind: "document".to_owned(),
            url: url.to_string(),
            anchor_index,
            label,
        });
    }
    if candidates.is_empty() {
        bail!("document index yielded no same-host upload PDF links");
    }
    Ok(candidates)
}

fn parse_reviewed_index_candidates(
    config: &StateConfig,
    index_url: &url::Url,
    raw: &[u8],
) -> Result<Vec<CalendarCandidate>> {
    let policy_url = url::Url::parse(&town_index_pdf_policy(config, index_url)?.url)?;
    parse_town_pdf_candidates(index_url, &policy_url, raw)
}

fn persist_calendar_candidates(data_dir: &Path, manifest: &CalendarCandidates) -> Result<PathBuf> {
    let bytes = serde_json::to_vec_pretty(manifest)?;
    let directory = data_dir
        .join("discovery-candidates")
        .join(&manifest.source_id);
    private_directory(data_dir, &directory)?;
    let path = directory.join(format!("{}.json", manifest.snapshot_sha256));
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
    {
        Ok(mut file) => {
            file.write_all(&bytes)?;
            file.sync_all()?;
        }
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            if !existing_regular_file_matches(&path, &bytes)? {
                bail!("existing calendar candidate artifact differs from archived bytes");
            }
        }
        Err(error) => return Err(error.into()),
    }
    // The SQLite index is scheduling metadata only. Every queued URL is
    // rederived from the immutable artifact and raw response before a fetch.
    // Recording it after the artifact is durable makes a crash safe to retry.
    let store = Store::open(data_dir)?;
    let seeds = manifest
        .candidates
        .iter()
        .map(|candidate| CandidateQueueSeed {
            url: candidate.url.clone(),
            kind: candidate.kind.clone(),
            anchor_index: candidate.anchor_index,
        })
        .collect::<Vec<_>>();
    store.index_candidate_snapshot(
        &manifest.source_id,
        &manifest.snapshot_sha256,
        OffsetDateTime::now_utc().unix_timestamp(),
        &seeds,
    )?;
    Ok(path)
}

fn parse_calendar_candidates(source_url: &url::Url, raw: &[u8]) -> Result<Vec<CalendarCandidate>> {
    let html = std::str::from_utf8(raw).context("calendar HTML is not UTF-8")?;
    let document = Html::parse_document(html);
    let anchors = Selector::parse("a[href]").expect("static selector is valid");
    let mut candidates = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (anchor_index, anchor) in document.select(&anchors).enumerate() {
        let Some(href) = anchor.value().attr("href") else {
            continue;
        };
        let Ok(url) = source_url.join(href) else {
            continue;
        };
        if url.scheme() != "https"
            || url.host_str() != source_url.host_str()
            || url.port_or_known_default() != Some(443)
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || url.as_str().len() > 2_048
        {
            continue;
        }
        let kind = if url.path().eq_ignore_ascii_case("/MeetingDetail.aspx") {
            "meeting"
        } else if url.path().eq_ignore_ascii_case("/View.ashx") {
            let mut media_parameters = url
                .query_pairs()
                .filter(|(key, _)| key.eq_ignore_ascii_case("M"));
            let Some((_, media)) = media_parameters.next() else {
                continue;
            };
            // A query with two M values is ambiguous even when they agree.
            // Never assign a document kind by silently accepting the first.
            if media_parameters.next().is_some() {
                continue;
            }
            match media.to_ascii_uppercase().as_str() {
                "A" | "AADA" | "AP" => "agenda",
                "PA" => "packet",
                "M" | "MADA" | "MP" => "minutes",
                _ => continue,
            }
        } else {
            continue;
        };
        let url = url.to_string();
        if !seen.insert(url.clone()) {
            continue;
        }
        if candidates.len() >= 2_000 {
            bail!("calendar has more than 2,000 eligible links");
        }
        let label = anchor
            .text()
            .flat_map(str::split_whitespace)
            .take(24)
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(160)
            .collect();
        candidates.push(CalendarCandidate {
            kind: kind.to_owned(),
            url,
            anchor_index,
            label,
        });
    }
    if candidates.is_empty() {
        bail!("calendar HTML yielded no meeting, agenda, or minutes links");
    }
    Ok(candidates)
}

#[derive(Debug, Serialize)]
struct CandidateArchive<'a> {
    schema_version: u8,
    calendar_source_id: &'a str,
    calendar_snapshot_sha256: &'a str,
    anchor_index: usize,
    candidate_kind: &'a str,
    candidate_url: &'a str,
    document_source_id: &'a str,
    document_source_config_digest: String,
    document_source_review_id: &'a str,
    document_sha256: &'a str,
    changed_from_sha256: Option<&'a str>,
    fetch_receipt_id: &'a str,
    retrieved_at: &'a str,
}

/// A candidate is an archived link, not a public claim. The selected URL is
/// independently rederived from preserved calendar bytes before any request.
pub(super) fn archive_candidate(
    data_dir: &Path,
    config: &StateConfig,
    calendar_source_id: &str,
    calendar_snapshot: &str,
    anchor_index: usize,
    document_source_id: &str,
) -> Result<()> {
    let store = Store::open(data_dir)?;
    let (outcome, path) = archive_candidate_with(
        &store,
        config,
        calendar_source_id,
        calendar_snapshot,
        anchor_index,
        document_source_id,
        &mut LiveFetcher,
    )?;
    println!(
        "archived calendar candidate {} (SHA-256 {}, {} bytes; HTTP {})\n  URL: {}\n  private record: {}",
        anchor_index,
        outcome.digest,
        outcome.bytes,
        outcome.status,
        outcome.final_url,
        path.display(),
    );
    if let Some(previous) = &outcome.changed_from {
        println!(
            "document bytes changed at this exact URL: previous SHA-256 {previous}; new SHA-256 {}. Both are preserved; no public claim changed.",
            outcome.digest
        );
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct QueueEntry {
    snapshot: String,
    candidate: CalendarCandidate,
    policy_id: String,
    previously_archived: bool,
    last_attempt_unix: Option<i64>,
}

#[derive(Debug)]
struct QueuePlan {
    selected: Vec<QueueEntry>,
    unique_candidates: usize,
    not_selected_candidates: usize,
    indexed: bool,
    planned_at_unix: i64,
    state_updates: Vec<(String, CandidateQueueOutcome, i64)>,
}

/// Read every bounded private candidate artifact before any network request.
/// Prior accepted bytes and receipts are the checkpoint; an already archived
/// URL becomes eligible again only after its configured refresh interval.
pub(super) fn archive_queue(
    data_dir: &Path,
    config: &StateConfig,
    origin_source_id: &str,
    snapshot: Option<&str>,
    max_fetches: usize,
    refresh_after_seconds: u64,
    dry_run: bool,
) -> Result<()> {
    let store = Store::open(data_dir)?;
    if snapshot.is_none() && !dry_run {
        // Do not mutate scheduling state when the origin itself is no longer
        // reviewed; the planner also checks this before selecting a fetch.
        verify_source_review(&store, config, origin_source_id)?;
        let context = queue_context_token(&store, config, origin_source_id)?;
        let reopened = store.requeue_blocked_candidate_context(
            origin_source_id,
            &context,
            MAX_QUEUE_INDEX_WINDOW,
        )?;
        if reopened > 0 {
            println!("reconsidering {reopened} private candidates after source policy changed");
        }
    }
    let now = OffsetDateTime::now_utc();
    let plan = plan_candidate_queue(
        &store,
        config,
        origin_source_id,
        snapshot,
        max_fetches,
        refresh_after_seconds,
        now,
    )?;
    println!(
        "{} indexed private candidates; {} selected in this bounded run; {} not selected; {} checkpoint/blocked/deferred",
        plan.unique_candidates,
        plan.selected.len(),
        plan.not_selected_candidates,
        plan.state_updates.len(),
    );
    for (url, outcome, _) in &plan.state_updates {
        if matches!(outcome, CandidateQueueOutcome::Blocked { .. }) {
            println!("  blocked {url}: {outcome:?}");
        }
    }
    if dry_run {
        for entry in &plan.selected {
            println!("  {} {}", entry.policy_id, entry.candidate.url);
        }
        return Ok(());
    }
    let run = archive_queue_with(&store, config, origin_source_id, plan, &mut LiveFetcher)?;
    for (url, outcome, path) in &run.successes {
        println!(
            "{} {} (SHA-256 {}, HTTP {}; private record {})",
            if outcome.unchanged {
                "unchanged"
            } else {
                "archived"
            },
            url,
            outcome.digest,
            outcome.status,
            path.display(),
        );
    }
    if !run.failures.is_empty() {
        bail!(
            "{} candidate fetch(es) failed; successful archives remain private:\n{}",
            run.failures.len(),
            run.failures.join("\n")
        );
    }
    Ok(())
}

#[derive(Debug)]
struct QueueRun {
    successes: Vec<(String, ArchiveOutcome, PathBuf)>,
    failures: Vec<String>,
}

fn archive_queue_with(
    store: &Store,
    config: &StateConfig,
    origin_source_id: &str,
    plan: QueuePlan,
    fetcher: &mut dyn ArchiveFetcher,
) -> Result<QueueRun> {
    let mut run = QueueRun {
        successes: Vec::new(),
        failures: Vec::new(),
    };
    if plan.indexed {
        for (url, outcome, at) in &plan.state_updates {
            store.mark_candidate_queue_outcome(origin_source_id, url, *at, outcome.clone())?;
        }
        if !plan.selected.is_empty() {
            store.advance_candidate_queue_turn(origin_source_id)?;
        }
    }
    for entry in plan.selected {
        let result = archive_candidate_with(
            store,
            config,
            origin_source_id,
            &entry.snapshot,
            entry.candidate.anchor_index,
            &entry.policy_id,
            fetcher,
        );
        match result {
            Ok((outcome, path)) => {
                if plan.indexed {
                    store.mark_candidate_queue_outcome(
                        origin_source_id,
                        &entry.candidate.url,
                        plan.planned_at_unix,
                        CandidateQueueOutcome::Archived,
                    )?;
                }
                run.successes.push((entry.candidate.url, outcome, path));
            }
            Err(error) => {
                if plan.indexed {
                    let interval = config
                        .sources
                        .iter()
                        .find(|source| source.id == entry.policy_id)
                        .context("candidate policy disappeared after planning")?
                        .minimum_interval_seconds;
                    store.mark_candidate_queue_outcome(
                        origin_source_id,
                        &entry.candidate.url,
                        plan.planned_at_unix,
                        CandidateQueueOutcome::RetryAfter(
                            plan.planned_at_unix
                                .saturating_add(i64::try_from(interval.max(60))?),
                        ),
                    )?;
                }
                run.failures
                    .push(format!("{}: {error:#}", entry.candidate.url));
            }
        }
    }
    Ok(run)
}

fn queue_snapshot_digests(
    store: &Store,
    origin_source_id: &str,
    selected_snapshot: Option<&str>,
) -> Result<Vec<String>> {
    let directory = store
        .data_dir()
        .join("discovery-candidates")
        .join(origin_source_id);
    walk_private_directories(store.data_dir(), &directory, false)?;
    let digest = selected_snapshot.context("explicit historical queue needs --snapshot")?;
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("selected candidate snapshot has an invalid digest");
    }
    let metadata = fs::symlink_metadata(directory.join(format!("{digest}.json")))?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > 8_388_608
    {
        bail!("candidate artifact is not a bounded regular file");
    }
    Ok(vec![digest.to_owned()])
}

fn policy_for_candidate<'a>(
    config: &'a StateConfig,
    origin_url: &str,
    candidate: &CalendarCandidate,
) -> Result<&'a SourceConfig> {
    let source_type = match candidate.kind.as_str() {
        "meeting" => "candidate_meeting_html",
        "agenda" | "packet" | "minutes" => "candidate_document_pdf",
        "document" => "candidate_index_pdf",
        _ => bail!("unsupported candidate kind"),
    };
    let mut matching = config.sources.iter().filter(|source| {
        source.source_type == source_type
            && source.official_discovery_url == origin_url
            && source.documented_at == origin_url
    });
    let policy = matching
        .next()
        .context("candidate lacks a matching reviewed path policy")?;
    if matching.next().is_some() {
        bail!("candidate has multiple matching path policies");
    }
    checked_archive_url(
        policy,
        &policy.id,
        &candidate.url,
        if source_type == "candidate_meeting_html" {
            "html_page"
        } else {
            "pdf"
        },
    )?;
    Ok(policy)
}

type QueueCandidates = HashMap<String, (String, CalendarCandidate, String)>;
type QueuePolicies<'a> =
    HashMap<String, (&'a SourceConfig, HashMap<String, Vec<FetchObservation>>)>;

fn last_confirmed_fetch_unix(
    history: Option<&Vec<FetchObservation>>,
    prior: Option<&PriorEvidence>,
    source_id: &str,
    url: &str,
    document_type: &str,
) -> Option<i64> {
    let digest = prior?.evidence_id.as_str();
    history?
        .iter()
        .filter(|item| {
            item.source_id.as_deref() == Some(source_id)
                && item.requested_url == url
                && item.final_url == url
                && item.method == "GET"
                && item.error.is_none()
                && item.redirect_target.is_none()
                && receipt_has_public_network(item, url)
        })
        .filter(|item| match item.status_code {
            200 => {
                let mime = item
                    .content_type
                    .as_deref()
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                item.body_digest.as_deref() == Some(digest)
                    && match document_type {
                        "pdf" => mime == "application/pdf" || mime.starts_with("application/pdf;"),
                        "html_page" => {
                            mime.starts_with("text/html")
                                || mime.starts_with("application/xhtml+xml")
                        }
                        _ => false,
                    }
            }
            304 => true,
            _ => false,
        })
        .filter_map(|item| OffsetDateTime::parse(&item.retrieved_at, &Rfc3339).ok())
        .map(OffsetDateTime::unix_timestamp)
        .max()
}

fn load_queue_state<'a>(
    store: &Store,
    config: &'a StateConfig,
    origin_source_id: &str,
    snapshot: Option<&str>,
) -> Result<(QueueCandidates, QueuePolicies<'a>, HashSet<String>)> {
    let origin = config
        .sources
        .iter()
        .find(|source| source.id == origin_source_id)
        .context("unknown candidate origin source")?;
    let snapshots = queue_snapshot_digests(store, origin_source_id, snapshot)?;
    let mut receipt_times = HashMap::<String, i64>::new();
    for observation in store.fetch_observations(origin_source_id)? {
        if observation.source_id.as_deref() != Some(origin_source_id)
            || observation.requested_url != origin.url
            || observation.final_url != origin.url
            || observation.status_code != 200
            || observation.error.is_some()
            || observation.redirect_target.is_some()
            || !receipt_has_public_network(&observation, &origin.url)
        {
            continue;
        }
        if let (Some(digest), Ok(at)) = (
            observation.body_digest,
            OffsetDateTime::parse(&observation.retrieved_at, &Rfc3339),
        ) {
            receipt_times
                .entry(digest)
                .and_modify(|old| *old = (*old).max(at.unix_timestamp()))
                .or_insert(at.unix_timestamp());
        }
    }
    let newest_snapshot = snapshots
        .iter()
        .max_by_key(|digest| receipt_times.get(*digest).copied().unwrap_or(i64::MIN))
        .context("candidate queue has no snapshot")?
        .clone();
    let mut current_urls = HashSet::new();
    let mut unique = QueueCandidates::new();
    let mut policy_ids = HashSet::new();
    let mut appearances = 0_usize;
    for digest in snapshots {
        for candidate in load_verified_candidates(store, config, origin_source_id, &digest)? {
            appearances += 1;
            if appearances > MAX_QUEUE_APPEARANCES {
                bail!(
                    "candidate queue exceeds its total link bound; select one snapshot explicitly"
                );
            }
            if digest == newest_snapshot {
                current_urls.insert(candidate.url.clone());
            }
            let policy = policy_for_candidate(config, &origin.url, &candidate)?;
            policy_ids.insert(policy.id.clone());
            unique.entry(candidate.url.clone()).or_insert((
                digest.clone(),
                candidate,
                policy.id.clone(),
            ));
            if unique.len() > MAX_QUEUE_CANDIDATES {
                bail!(
                    "candidate queue exceeds its unique URL bound; select one snapshot explicitly"
                );
            }
        }
    }
    let mut policies = QueuePolicies::new();
    for id in policy_ids {
        let policy = config
            .sources
            .iter()
            .find(|source| source.id == id)
            .context("path policy disappeared")?;
        verify_source_review(store, config, &id)?;
        let mut by_url = HashMap::<String, Vec<FetchObservation>>::new();
        for observation in store.fetch_observations(&id)? {
            by_url
                .entry(observation.requested_url.clone())
                .or_default()
                .push(observation);
        }
        policies.insert(id, (policy, by_url));
    }
    Ok((unique, policies, current_urls))
}

// Explicit historical selection retains the original source-checked planner;
// the indexed branch below handles the default long-running queue.
#[allow(clippy::too_many_lines)]
fn plan_candidate_queue(
    store: &Store,
    config: &StateConfig,
    origin_source_id: &str,
    snapshot: Option<&str>,
    max_fetches: usize,
    refresh_after_seconds: u64,
    now: OffsetDateTime,
) -> Result<QueuePlan> {
    if max_fetches == 0
        || max_fetches > MAX_QUEUE_FETCHES
        || refresh_after_seconds == 0
        || refresh_after_seconds > 31_536_000
        || origin_source_id.is_empty()
        || !origin_source_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        bail!("invalid bounded candidate queue parameters");
    }
    if snapshot.is_none() {
        return plan_indexed_candidate_queue(
            store,
            config,
            origin_source_id,
            max_fetches,
            refresh_after_seconds,
            now,
        );
    }
    let refresh_after_seconds = i64::try_from(refresh_after_seconds)?;
    let (unique, policies, current_urls) =
        load_queue_state(store, config, origin_source_id, snapshot)?;
    let mut eligible = Vec::new();
    for (digest, candidate, policy_id) in unique.values() {
        let (policy, observations) = policies
            .get(policy_id)
            .context("path policy observations missing")?;
        let history = observations.get(&candidate.url);
        let last_attempt_unix = history.and_then(|items| {
            items
                .iter()
                .filter(|item| item.source_id.as_deref() == Some(policy_id.as_str()))
                .filter_map(|item| OffsetDateTime::parse(&item.retrieved_at, &Rfc3339).ok())
                .map(OffsetDateTime::unix_timestamp)
                .max()
        });
        let document_type = if policy.source_type == "candidate_meeting_html" {
            "html_page"
        } else {
            "pdf"
        };
        let prior = latest_valid_archive_prior_in(
            store,
            policy_id,
            &candidate.url,
            document_type,
            policy.maximum_bytes,
            history.into_iter().flat_map(|items| items.iter().rev()),
        );
        if prior.is_some() && !current_urls.contains(&candidate.url) {
            continue;
        }
        let last_confirmed_unix = last_confirmed_fetch_unix(
            history,
            prior.as_ref(),
            policy_id,
            &candidate.url,
            document_type,
        );
        if last_confirmed_unix
            .is_some_and(|then| now.unix_timestamp().saturating_sub(then) < refresh_after_seconds)
        {
            continue;
        }
        eligible.push(QueueEntry {
            snapshot: digest.clone(),
            candidate: candidate.clone(),
            policy_id: policy_id.clone(),
            previously_archived: prior.is_some(),
            last_attempt_unix,
        });
    }
    eligible.sort_by(|a, b| {
        a.previously_archived
            .cmp(&b.previously_archived)
            .then_with(|| a.last_attempt_unix.cmp(&b.last_attempt_unix))
            .then_with(|| a.candidate.url.cmp(&b.candidate.url))
    });
    let mut selected = Vec::new();
    let mut selected_policies = HashSet::new();
    for entry in eligible {
        if selected.len() >= max_fetches {
            break;
        }
        if selected_policies.contains(&entry.policy_id) {
            continue;
        }
        let (policy, _) = policies
            .get(&entry.policy_id)
            .context("path policy missing")?;
        if !store.source_fetch_allowed(
            &entry.policy_id,
            policy.minimum_interval_seconds,
            now.unix_timestamp(),
        )? {
            continue;
        }
        selected_policies.insert(entry.policy_id.clone());
        selected.push(entry);
    }
    Ok(QueuePlan {
        not_selected_candidates: unique.len().saturating_sub(selected.len()),
        selected,
        unique_candidates: unique.len(),
        indexed: false,
        planned_at_unix: now.unix_timestamp(),
        state_updates: vec![],
    })
}

/// The queue index is only a bounded scheduler. Selected entries still pass
/// the original raw-byte, receipt, anchor, path, review, robots, and fetch
/// gates before any network request. Blocked entries are visible and cannot
/// occupy the same due window forever.
#[allow(clippy::too_many_lines)]
fn plan_indexed_candidate_queue(
    store: &Store,
    config: &StateConfig,
    origin_source_id: &str,
    max_fetches: usize,
    refresh_after_seconds: u64,
    now: OffsetDateTime,
) -> Result<QueuePlan> {
    let origin = config
        .sources
        .iter()
        .find(|source| source.id == origin_source_id)
        .context("unknown candidate origin source")?;
    verify_source_review(store, config, origin_source_id)?;
    let context_token = queue_context_token(store, config, origin_source_id)?;
    let now_unix = now.unix_timestamp();
    let due: Vec<CandidateQueueItem> = store.candidate_queue_due(
        origin_source_id,
        now_unix,
        i64::try_from(refresh_after_seconds)?,
        MAX_QUEUE_INDEX_WINDOW,
    )?;
    let total = store.candidate_queue_total(origin_source_id)?;
    if total == 0 {
        bail!(
            "no indexed candidate snapshots match this source; discover its official index first"
        );
    }
    let mut checked_snapshots = HashMap::<String, Vec<CalendarCandidate>>::new();
    let mut selected = Vec::new();
    let mut selected_policies = HashSet::new();
    let mut state_updates = Vec::new();
    for item in due {
        let candidates = if let Some(candidates) = checked_snapshots.get(&item.snapshot_digest) {
            candidates
        } else {
            match load_verified_candidates(store, config, origin_source_id, &item.snapshot_digest) {
                Ok(candidates) => {
                    checked_snapshots.insert(item.snapshot_digest.clone(), candidates);
                    checked_snapshots
                        .get(&item.snapshot_digest)
                        .context("validated snapshot vanished")?
                }
                Err(error) => {
                    state_updates.push((
                        item.url,
                        queue_blocked(
                            &context_token,
                            &format!("snapshot verification failed: {error:#}"),
                        ),
                        now_unix,
                    ));
                    continue;
                }
            }
        };
        let candidate = candidates
            .iter()
            .find(|candidate| candidate.anchor_index == item.anchor_index)
            .filter(|candidate| candidate.url == item.url && candidate.kind == item.kind)
            .cloned();
        let Some(candidate) = candidate else {
            state_updates.push((
                item.url,
                queue_blocked(
                    &context_token,
                    "indexed anchor differs from preserved index",
                ),
                now_unix,
            ));
            continue;
        };
        let policy = match policy_for_candidate(config, &origin.url, &candidate) {
            Ok(policy) => policy,
            Err(error) => {
                state_updates.push((
                    item.url,
                    queue_blocked(
                        &context_token,
                        &format!("candidate is outside current reviewed path: {error:#}"),
                    ),
                    now_unix,
                ));
                continue;
            }
        };
        let review = match verify_source_review(store, config, &policy.id) {
            Ok(review) => review,
            Err(error) => {
                state_updates.push((
                    item.url,
                    queue_blocked(
                        &context_token,
                        &format!("candidate path policy review unavailable: {error:#}"),
                    ),
                    now_unix,
                ));
                continue;
            }
        };
        let candidate_path = url::Url::parse(&candidate.url)?.path().to_owned();
        if let Err(error) = crate::require_preserved_robots_candidate_path(
            store,
            &policy.id,
            &review,
            &candidate_path,
        ) {
            if error.to_string().contains("robots.txt disallows") {
                state_updates.push((
                    item.url,
                    queue_blocked(
                        &context_token,
                        &format!("candidate path disallowed by robots review {}", review.id),
                    ),
                    now_unix,
                ));
                continue;
            }
            return Err(error);
        }
        let observations = store.fetch_observations_for_url(&policy.id, &candidate.url)?;
        let document_type = if policy.source_type == "candidate_meeting_html" {
            "html_page"
        } else {
            "pdf"
        };
        let prior = latest_valid_archive_prior_in(
            store,
            &policy.id,
            &candidate.url,
            document_type,
            policy.maximum_bytes,
            observations.iter().rev(),
        );
        if item.last_success_unix.is_some() && prior.is_none() {
            bail!("candidate queue success metadata lacks preserved accepted bytes and receipt");
        }
        if prior.is_some() && item.last_success_unix.is_none() {
            let confirmed = last_confirmed_fetch_unix(
                Some(&observations),
                prior.as_ref(),
                &policy.id,
                &candidate.url,
                document_type,
            )
            .context("accepted candidate has no confirmed receipt time")?;
            if !item.is_current
                || now_unix.saturating_sub(confirmed) < i64::try_from(refresh_after_seconds)?
            {
                state_updates.push((candidate.url, CandidateQueueOutcome::Archived, confirmed));
                continue;
            }
        }
        if selected.len() >= max_fetches {
            state_updates.push((
                candidate.url,
                CandidateQueueOutcome::RetryAfter(now_unix.saturating_add(60)),
                now_unix,
            ));
            continue;
        }
        if selected_policies.contains(&policy.id)
            || !store.source_fetch_allowed(&policy.id, policy.minimum_interval_seconds, now_unix)?
        {
            state_updates.push((
                candidate.url,
                CandidateQueueOutcome::RetryAfter(
                    now_unix
                        .saturating_add(i64::try_from(policy.minimum_interval_seconds.max(60))?),
                ),
                now_unix,
            ));
            continue;
        }
        selected_policies.insert(policy.id.clone());
        selected.push(QueueEntry {
            snapshot: item.snapshot_digest,
            candidate,
            policy_id: policy.id.clone(),
            previously_archived: prior.is_some(),
            last_attempt_unix: item.last_attempt_unix,
        });
    }
    Ok(QueuePlan {
        not_selected_candidates: total.saturating_sub(selected.len()),
        unique_candidates: total,
        selected,
        indexed: true,
        planned_at_unix: now_unix,
        state_updates,
    })
}

fn queue_context_token(
    store: &Store,
    config: &StateConfig,
    origin_source_id: &str,
) -> Result<String> {
    let origin = config
        .sources
        .iter()
        .find(|source| source.id == origin_source_id)
        .context("unknown candidate origin source")?;
    let mut parts = Vec::new();
    for source in &config.sources {
        if source.id == origin_source_id || source.official_discovery_url == origin.url {
            let review_id = store
                .current_source_review(&source.id)?
                .map(|review| review.id)
                .unwrap_or_default();
            parts.push(format!(
                "{}:{}:{}",
                source.id,
                source_config_digest(source),
                review_id
            ));
        }
    }
    parts.sort();
    Ok(sha256_hex(parts.join("\n").as_bytes()))
}

fn queue_blocked(context_token: &str, reason: &str) -> CandidateQueueOutcome {
    let brief = reason.chars().take(100).collect::<String>();
    CandidateQueueOutcome::Blocked {
        context: format!("{context_token}:{brief}"),
    }
}

fn archive_candidate_with(
    store: &Store,
    config: &StateConfig,
    calendar_source_id: &str,
    calendar_snapshot: &str,
    anchor_index: usize,
    document_source_id: &str,
    fetcher: &mut dyn ArchiveFetcher,
) -> Result<(ArchiveOutcome, PathBuf)> {
    if document_source_id.is_empty()
        || !document_source_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        bail!("invalid document path policy ID");
    }
    let candidate = load_verified_candidate(
        store,
        config,
        calendar_source_id,
        calendar_snapshot,
        anchor_index,
    )?;
    let policy = config
        .sources
        .iter()
        .find(|source| source.id == document_source_id)
        .ok_or_else(|| anyhow::anyhow!("unknown document policy: {document_source_id}"))?;
    let (expected_kind, document_type, path) = match policy.source_type.as_str() {
        "candidate_meeting_html" => ("meeting", "html_page", "/MeetingDetail.aspx"),
        "candidate_document_pdf" => ("document", "pdf", "/View.ashx"),
        "candidate_index_pdf" => ("index_document", "pdf", ""),
        _ => bail!("source is not a reviewed calendar document path policy"),
    };
    if (expected_kind == "meeting" && candidate.kind != "meeting")
        || (expected_kind == "document"
            && !matches!(candidate.kind.as_str(), "agenda" | "packet" | "minutes"))
        || (expected_kind == "index_document" && candidate.kind != "document")
    {
        bail!("candidate kind does not match the reviewed document path policy");
    }
    let policy_url = url::Url::parse(&policy.url)?;
    let candidate_url = url::Url::parse(&candidate.url)?;
    let path_allowed = if expected_kind == "index_document" {
        policy.official_discovery_url
            == config
                .sources
                .iter()
                .find(|source| source.id == calendar_source_id)
                .context("document index source disappeared")?
                .url
            && policy.documented_at == policy.official_discovery_url
            && town_pdf_prefix_allowed(policy_url.path())
            && town_pdf_url_allowed(&policy_url, &candidate_url)
    } else {
        policy_url.path() == path && candidate_url.path() == path && candidate_url.query().is_some()
    };
    if policy_url.scheme() != "https"
        || policy_url.query().is_some()
        || policy_url.fragment().is_some()
        || !path_allowed
        || candidate_url.scheme() != "https"
        || candidate_url.host_str() != policy_url.host_str()
        || candidate_url.port_or_known_default() != Some(443)
        || candidate_url.fragment().is_some()
        || !candidate_url.username().is_empty()
        || candidate_url.password().is_some()
    {
        bail!("candidate URL is outside the reviewed official document path");
    }
    let outcome = archive_url_with(
        store,
        config,
        document_source_id,
        &candidate.url,
        document_type,
        fetcher,
    )?;
    let record = CandidateArchive {
        schema_version: 1,
        calendar_source_id,
        calendar_snapshot_sha256: calendar_snapshot,
        anchor_index,
        candidate_kind: &candidate.kind,
        candidate_url: &candidate.url,
        document_source_id,
        document_source_config_digest: source_config_digest(policy),
        document_source_review_id: &outcome.review_id,
        document_sha256: &outcome.digest,
        changed_from_sha256: outcome.changed_from.as_deref(),
        fetch_receipt_id: &outcome.receipt_id,
        retrieved_at: &outcome.retrieved_at,
    };
    let path = persist_candidate_archive(store.data_dir(), &record)?;
    Ok((outcome, path))
}

fn load_verified_candidate(
    store: &Store,
    config: &StateConfig,
    calendar_source_id: &str,
    calendar_snapshot: &str,
    anchor_index: usize,
) -> Result<CalendarCandidate> {
    load_verified_candidates(store, config, calendar_source_id, calendar_snapshot)?
        .into_iter()
        .find(|candidate| candidate.anchor_index == anchor_index)
        .context("anchor index is absent from archived calendar")
}

fn load_verified_candidates(
    store: &Store,
    config: &StateConfig,
    calendar_source_id: &str,
    calendar_snapshot: &str,
) -> Result<Vec<CalendarCandidate>> {
    if calendar_source_id.is_empty()
        || !calendar_source_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || calendar_snapshot.len() != 64
        || !calendar_snapshot
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("invalid calendar source ID or snapshot digest");
    }
    let calendar = config
        .sources
        .iter()
        .find(|source| source.id == calendar_source_id)
        .ok_or_else(|| anyhow::anyhow!("unknown calendar source: {calendar_source_id}"))?;
    let calendar_url = url::Url::parse(&calendar.url)?;
    let is_legistar_calendar = calendar.source_type == "html_page"
        && calendar_url.path() == "/Calendar.aspx"
        && calendar_url.query().is_none();
    let is_town_index = calendar.source_type == "html_page"
        && town_index_path_allowed(calendar_url.path())
        && calendar_url.query().is_none();
    if (!is_legistar_calendar && !is_town_index)
        || calendar_url.scheme() != "https"
        || calendar_url.port_or_known_default() != Some(443)
        || calendar_url.fragment().is_some()
        || !calendar_url.username().is_empty()
        || calendar_url.password().is_some()
    {
        bail!("candidate origin is not a configured official HTML calendar or document index");
    }
    verify_source_review(store, config, calendar_source_id)?;
    let mut raw = Vec::new();
    fs::File::open(store.content_path(calendar_snapshot)?)?
        .take((calendar.maximum_bytes as u64).saturating_add(1))
        .read_to_end(&mut raw)?;
    if raw.is_empty() || raw.len() > calendar.maximum_bytes || sha256_hex(&raw) != calendar_snapshot
    {
        bail!("calendar snapshot bytes are missing or altered");
    }
    if !store
        .fetch_observations(calendar_source_id)?
        .iter()
        .any(|observation| {
            calendar_receipt_matches(
                observation,
                calendar_source_id,
                &calendar.url,
                calendar_snapshot,
                &raw,
            )
        })
    {
        bail!("calendar snapshot lacks an exact official fetch receipt");
    }
    let expected = CalendarCandidates {
        schema_version: 1,
        source_id: calendar_source_id.to_owned(),
        source_url: calendar.url.clone(),
        snapshot_sha256: calendar_snapshot.to_owned(),
        candidates: if is_legistar_calendar {
            parse_calendar_candidates(&calendar_url, &raw)?
        } else {
            parse_reviewed_index_candidates(config, &calendar_url, &raw)?
        },
    };
    let path = store
        .data_dir()
        .join("discovery-candidates")
        .join(calendar_source_id)
        .join(format!("{calendar_snapshot}.json"));
    walk_private_directories(
        store.data_dir(),
        path.parent()
            .context("calendar artifact has no directory")?,
        false,
    )?;
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > 8_388_608
    {
        bail!("calendar candidate artifact is not a bounded regular file");
    }
    let mut artifact = Vec::new();
    fs::File::open(&path)?
        .take(8_388_609)
        .read_to_end(&mut artifact)?;
    if artifact.len() > 8_388_608 {
        bail!("calendar candidate artifact exceeds its byte bound");
    }
    if artifact != serde_json::to_vec_pretty(&expected)? {
        bail!("calendar candidate artifact differs from preserved HTML");
    }
    Ok(expected.candidates)
}

fn calendar_receipt_matches(
    observation: &FetchObservation,
    source_id: &str,
    source_url: &str,
    snapshot_sha256: &str,
    raw: &[u8],
) -> bool {
    observation.source_id.as_deref() == Some(source_id)
        && observation.requested_url == source_url
        && observation.final_url == source_url
        && observation.method == "GET"
        && observation.status_code == 200
        && observation.error.is_none()
        && observation.redirect_target.is_none()
        && observation.body_digest.as_deref() == Some(snapshot_sha256)
        && validate_document("html_page", raw, observation.content_type.as_deref(), false).is_ok()
        && receipt_has_public_network(observation, source_url)
}

fn receipt_has_public_network(observation: &FetchObservation, source_url: &str) -> bool {
    let resolved = observation
        .resolved_ips
        .iter()
        .map(|address| address.parse::<IpAddr>())
        .collect::<Result<Vec<_>, _>>();
    OffsetDateTime::parse(&observation.retrieved_at, &Rfc3339).is_ok()
        && resolved.is_ok_and(|addresses| {
            url::Url::parse(source_url)
                .ok()
                .and_then(|url| url.host_str().map(str::to_owned))
                .is_some_and(|host| pnull_http::validate_resolved(&host, &addresses).is_ok())
        })
}

fn persist_candidate_archive(data_dir: &Path, record: &CandidateArchive<'_>) -> Result<PathBuf> {
    let directory = data_dir
        .join("discovery-archives")
        .join(record.document_source_id);
    private_directory(data_dir, &directory)?;
    let key = sha256_hex(
        format!(
            "{}\0{}\0{}",
            record.candidate_url, record.document_sha256, record.fetch_receipt_id,
        )
        .as_bytes(),
    );
    let path = directory.join(format!("{key}.json"));
    let bytes = serde_json::to_vec_pretty(record)?;
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
    {
        Ok(mut file) => {
            file.write_all(&bytes)?;
            file.sync_all()?;
        }
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            if !existing_regular_file_matches(&path, &bytes)? {
                bail!("existing calendar archive record differs from fetched bytes");
            }
        }
        Err(error) => return Err(error.into()),
    }
    Ok(path)
}

fn private_directory(data_dir: &Path, path: &Path) -> Result<()> {
    walk_private_directories(data_dir, path, true)
}

fn walk_private_directories(data_dir: &Path, path: &Path, create: bool) -> Result<()> {
    let root = fs::symlink_metadata(data_dir)?;
    if !root.file_type().is_dir() || root.file_type().is_symlink() {
        bail!("private archive root is not a regular directory");
    }
    let relative = path
        .strip_prefix(data_dir)
        .context("private archive path escapes the data directory")?;
    let mut current = data_dir.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            bail!("private archive path has an invalid component");
        };
        current.push(name);
        if create {
            match fs::create_dir(&current) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
        let metadata = fs::symlink_metadata(&current)?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            bail!("private archive path contains a non-directory or symlink");
        }
        if create {
            fs::set_permissions(&current, fs::Permissions::from_mode(0o700))?;
        }
    }
    Ok(())
}

fn existing_regular_file_matches(path: &Path, expected: &[u8]) -> Result<bool> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() != expected.len() as u64
    {
        return Ok(false);
    }
    let mut actual = Vec::new();
    fs::File::open(path)?
        .take((expected.len() as u64).saturating_add(1))
        .read_to_end(&mut actual)?;
    Ok(actual == expected)
}

fn archive_with(
    store: &Store,
    config: &StateConfig,
    source_id: &str,
    fetcher: &mut dyn ArchiveFetcher,
) -> Result<ArchiveOutcome> {
    let source = config
        .sources
        .iter()
        .find(|source| source.id == source_id)
        .ok_or_else(|| anyhow::anyhow!("unknown source: {source_id}"))?;
    if !matches!(
        source.source_type.as_str(),
        "html_page" | "pdf" | "official_api"
    ) {
        bail!("source archive requires a configured HTML, PDF, or JSON source: {source_id}");
    }
    archive_url_with(
        store,
        config,
        source_id,
        &source.url,
        &source.source_type,
        fetcher,
    )
}

fn checked_archive_url(
    source: &SourceConfig,
    source_id: &str,
    url: &str,
    document_type: &str,
) -> Result<url::Url> {
    let expected_type = match source.source_type.as_str() {
        "candidate_meeting_html" => "html_page",
        "candidate_document_pdf" | "candidate_index_pdf" => "pdf",
        ordinary => ordinary,
    };
    if expected_type != document_type {
        bail!("archive document type differs from reviewed source policy: {source_id}");
    }
    let reviewed_url = url::Url::parse(&source.url)?;
    let requested_url = url::Url::parse(url)?;
    let path_allowed = if source.source_type == "candidate_index_pdf" {
        town_pdf_prefix_allowed(reviewed_url.path())
            && town_pdf_url_allowed(&reviewed_url, &requested_url)
    } else {
        reviewed_url.path() == requested_url.path()
    };
    if reviewed_url.scheme() != "https"
        || reviewed_url.port_or_known_default() != Some(443)
        || !reviewed_url.username().is_empty()
        || reviewed_url.password().is_some()
        || (source.source_type.starts_with("candidate_")
            && (reviewed_url.query().is_some() || reviewed_url.fragment().is_some()))
        || reviewed_url.host_str() != requested_url.host_str()
        || reviewed_url.port_or_known_default() != requested_url.port_or_known_default()
        || !path_allowed
        || requested_url.fragment().is_some()
        || !requested_url.username().is_empty()
        || requested_url.password().is_some()
        || (!source.source_type.starts_with("candidate_") && source.url != url)
    {
        bail!("archive URL is outside the reviewed source path: {source_id}");
    }
    if source.maximum_bytes == 0 {
        bail!("source archive requires a positive maximum_bytes for {source_id}");
    }
    Ok(requested_url)
}

fn checked_archive_observation<'a>(
    source_id: &str,
    url: &str,
    retrieved_at: &str,
    observations: &'a [FetchObservation],
) -> Result<&'a FetchObservation> {
    let last = observations
        .first()
        .ok_or_else(|| anyhow::anyhow!("archive fetch returned no provenance for {source_id}"))?;
    if observations.len() != 1
        || last.source_id.as_deref() != Some(source_id)
        || last.requested_url != url
        || last.final_url != url
        || last.retrieved_at != retrieved_at
        || last.method != "GET"
        || last.error.is_some()
        || last.redirect_target.is_some()
        || !receipt_has_public_network(last, url)
    {
        bail!("archive fetch provenance does not match the reviewed source {source_id}");
    }
    if !matches!(last.status_code, 200 | 304) {
        bail!(
            "archive fetch returned unexpected HTTP {} for {source_id}",
            last.status_code
        );
    }
    Ok(last)
}

/// Ignore HTTP receipts whose bytes were never accepted as this document type.
/// A failed archive attempt may still leave raw bytes and a receipt in the
/// private store; it must not become the baseline for a 304 or revision report.
fn latest_valid_archive_prior(
    store: &Store,
    source_id: &str,
    url: &str,
    document_type: &str,
    maximum_bytes: usize,
) -> Result<Option<PriorEvidence>> {
    let observations = store.fetch_observations(source_id)?;
    Ok(latest_valid_archive_prior_in(
        store,
        source_id,
        url,
        document_type,
        maximum_bytes,
        observations.iter().rev(),
    ))
}

fn latest_valid_archive_prior_in<'a>(
    store: &Store,
    source_id: &str,
    url: &str,
    document_type: &str,
    maximum_bytes: usize,
    observations: impl IntoIterator<Item = &'a FetchObservation>,
) -> Option<PriorEvidence> {
    for observation in observations {
        if observation.source_id.as_deref() != Some(source_id)
            || observation.requested_url != url
            || observation.final_url != url
            || observation.status_code != 200
            || observation.method != "GET"
            || observation.error.is_some()
            || observation.redirect_target.is_some()
            || !receipt_has_public_network(observation, url)
        {
            continue;
        }
        let Some(digest) = observation.body_digest.as_ref() else {
            continue;
        };
        let Ok(path) = store.content_path(digest) else {
            continue;
        };
        let Ok(file) = fs::File::open(path) else {
            continue;
        };
        let mut raw = Vec::new();
        if file
            .take((maximum_bytes as u64).saturating_add(1))
            .read_to_end(&mut raw)
            .is_err()
            || raw.is_empty()
            || raw.len() > maximum_bytes
            || sha256_hex(&raw) != *digest
            || validate_document(
                document_type,
                &raw,
                observation.content_type.as_deref(),
                false,
            )
            .is_err()
        {
            continue;
        }
        return Some(PriorEvidence {
            evidence_id: digest.clone(),
            etag: observation.etag.clone(),
            last_modified: observation.last_modified.clone(),
        });
    }
    None
}

fn archive_url_with(
    store: &Store,
    config: &StateConfig,
    source_id: &str,
    url: &str,
    document_type: &str,
    fetcher: &mut dyn ArchiveFetcher,
) -> Result<ArchiveOutcome> {
    let source = config
        .sources
        .iter()
        .find(|source| source.id == source_id)
        .ok_or_else(|| anyhow::anyhow!("unknown source: {source_id}"))?;
    let requested_url = checked_archive_url(source, source_id, url, document_type)?;

    // This gate verifies expiration, preserved robots bytes and receipt, exact
    // config digest, host, endpoint path, and absence of response restrictions.
    let review = verify_source_review(store, config, source_id)?;
    if source.source_type.starts_with("candidate_") {
        crate::require_preserved_robots_candidate_path(
            store,
            source_id,
            &review,
            requested_url.path(),
        )?;
    }
    let now = OffsetDateTime::now_utc();
    let retrieved_at = now.format(&Rfc3339)?;
    if !store.try_record_source_fetch(
        source_id,
        source.minimum_interval_seconds,
        now.unix_timestamp(),
    )? {
        bail!(
            "rate limit: source {source_id} was fetched within the configured {}-second interval",
            source.minimum_interval_seconds
        );
    }

    let prior =
        latest_valid_archive_prior(store, source_id, url, document_type, source.maximum_bytes)?;
    let fetch_config = FetchConfig {
        reviewed_hosts: vec![
            requested_url
                .host_str()
                .context("archive URL lacks an official host")?
                .to_owned(),
        ],
        reviewed_paths: Some(vec![requested_url.path().to_owned()]),
        max_bytes: source.maximum_bytes,
    };
    let (body, observations) = fetcher
        .fetch(
            store,
            source_id,
            &fetch_config,
            url,
            &retrieved_at,
            prior.as_ref(),
        )
        .with_context(|| format!("archive fetch failed for {source_id}"))?;
    let last = checked_archive_observation(source_id, url, &retrieved_at, &observations)?;
    let unchanged = last.status_code == 304;
    if body.is_empty() || body.len() > source.maximum_bytes {
        bail!("archive fetch returned empty or oversized content for {source_id}");
    }
    validate_document(
        document_type,
        &body,
        last.content_type.as_deref(),
        unchanged,
    )?;

    let digest = sha256_hex(&body);
    let changed_from = prior
        .as_ref()
        .filter(|item| item.evidence_id != digest)
        .map(|item| item.evidence_id.clone());
    if unchanged {
        if prior.as_ref().is_none_or(|item| item.evidence_id != digest) {
            bail!("archive 304 response lacked matching preserved prior bytes for {source_id}");
        }
    } else if last.body_digest.as_deref() != Some(digest.as_str()) {
        bail!("archive fetch receipt digest does not match raw response for {source_id}");
    }
    // The real fetcher already persists both. Repeating the writes here also
    // makes the archive's retention invariant explicit and testable.
    store.persist_content(&digest, &body)?;
    for observation in &observations {
        store.insert_fetch_observation(observation)?;
    }
    Ok(ArchiveOutcome {
        digest,
        bytes: body.len(),
        unchanged,
        status: last.status_code,
        final_url: last.final_url.clone(),
        observations: observations.len(),
        receipt_id: last.id.clone(),
        retrieved_at,
        review_id: review.id,
        changed_from,
    })
}

fn validate_document(
    source_type: &str,
    bytes: &[u8],
    content_type: Option<&str>,
    unchanged: bool,
) -> Result<()> {
    match source_type {
        "pdf" => {
            if !bytes.starts_with(b"%PDF-") {
                bail!("archive PDF source returned non-PDF bytes");
            }
            if !unchanged
                && !content_type.is_some_and(|value| {
                    value.eq_ignore_ascii_case("application/pdf")
                        || value.to_ascii_lowercase().starts_with("application/pdf;")
                })
            {
                bail!("archive PDF source returned a non-PDF content type");
            }
        }
        "html_page" => {
            let prefix =
                String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).to_ascii_lowercase();
            if !prefix.contains("<html") && !prefix.contains("<!doctype html") {
                bail!("archive HTML source returned non-HTML bytes");
            }
            if !unchanged
                && !content_type.is_some_and(|value| {
                    value.to_ascii_lowercase().starts_with("text/html")
                        || value
                            .to_ascii_lowercase()
                            .starts_with("application/xhtml+xml")
                })
            {
                bail!("archive HTML source returned a non-HTML content type");
            }
        }
        "official_api" => {
            serde_json::from_slice::<serde_json::Value>(bytes)
                .context("archive JSON source returned invalid JSON")?;
            if !unchanged
                && !content_type.is_some_and(|value| {
                    value.to_ascii_lowercase().starts_with("application/json")
                        || value.to_ascii_lowercase().starts_with("text/json")
                })
            {
                bail!("archive JSON source returned a non-JSON content type");
            }
        }
        _ => bail!("archive source type is not HTML, PDF, or JSON"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SourceConfig, SourceReview, StateConfig, source_config_digest};
    use pnull_core::stable_id;
    use std::os::unix::fs::symlink;
    use tempfile::tempdir;

    const SOURCE_ID: &str = "test-direct-document";
    const SOURCE_URL: &str = "https://official.example.test/decision.html?id=1";

    #[test]
    fn calendar_discovers_same_host_meetings_and_documents_only() {
        let url = url::Url::parse("https://council.example.test/Calendar.aspx").unwrap();
        let html = br#"<!doctype html><html><body>
          <a href="MeetingDetail.aspx?ID=25&amp;GUID=abc">Meeting details</a>
          <a href="View.ashx?M=A&amp;ID=25">Agenda</a>
          <a href="View.ashx?M=M&amp;ID=25">Minutes</a>
          <a href="View.ashx?M=IC&amp;ID=25">Calendar invite</a>
          <a href="https://evil.example/MeetingDetail.aspx?ID=25">External</a>
          <a href="javascript:alert(1)">Script</a>
          <a href="MeetingDetail.aspx?ID=25&amp;GUID=abc">Duplicate</a>
        </body></html>"#;
        let links = parse_calendar_candidates(&url, html).unwrap();
        assert_eq!(links.len(), 3);
        assert_eq!(links[0].kind, "meeting");
        assert_eq!(links[0].anchor_index, 0);
        assert_eq!(
            links[0].url,
            "https://council.example.test/MeetingDetail.aspx?ID=25&GUID=abc"
        );
        assert_eq!(links[1].kind, "agenda");
        assert_eq!(links[2].kind, "minutes");
    }

    #[test]
    fn calendar_selector_change_fails_closed() {
        let url = url::Url::parse("https://council.example.test/Calendar.aspx").unwrap();
        assert!(parse_calendar_candidates(&url, b"<html><body>no links</body></html>").is_err());
        assert!(parse_calendar_candidates(&url, b"not utf8 \xff").is_err());
    }

    #[test]
    fn calendar_skips_oversized_candidate_urls() {
        let url = url::Url::parse("https://council.example.test/Calendar.aspx").unwrap();
        let html = format!(
            "<a href=\"MeetingDetail.aspx?ID={}\">too long</a><a href=\"MeetingDetail.aspx?ID=2\">valid</a>",
            "a".repeat(2_048)
        );
        let candidates = parse_calendar_candidates(&url, html.as_bytes()).unwrap();
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].url.ends_with("?ID=2"));
    }

    #[test]
    fn calendar_skips_view_links_with_duplicate_media_parameters() {
        let url = url::Url::parse("https://council.example.test/Calendar.aspx").unwrap();
        let html = br#"<html><body>
          <a href="View.ashx?ID=1&amp;M=A&amp;M=M">conflicting M</a>
          <a href="View.ashx?ID=2&amp;M=A&amp;m=A">repeated M</a>
          <a href="View.ashx?ID=3&amp;%4D=A&amp;M=A">encoded repeated M</a>
          <a href="View.ashx?ID=4&amp;M=A">unambiguous agenda</a>
        </body></html>"#;
        let candidates = parse_calendar_candidates(&url, html).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].kind, "agenda");
        assert!(candidates[0].url.contains("ID=4"));
    }

    #[test]
    fn silver_plume_official_excerpt_yields_private_pdf_candidates() {
        let raw = include_bytes!("../../../fixtures/co-silver-plume/index-excerpt.html");
        assert_eq!(
            sha256_hex(raw),
            "8302b7bc8119c963ccf11f687aabc60aaacce93186ced8faf97c012dc6539ab4"
        );
        let url = url::Url::parse("https://silverplumetown.com/agendas-and-minutes/").unwrap();
        let policy = url::Url::parse("https://silverplumetown.com/wp-content/uploads/").unwrap();
        let candidates = parse_town_pdf_candidates(&url, &policy, raw).unwrap();
        assert_eq!(candidates.len(), 2);
        assert!(candidates.iter().all(|item| item.kind == "document"));
        assert!(candidates[0].label.contains("BOT Agenda 11.9.2026"));
        assert!(candidates[1].label.contains("February 12, 2024"));
        assert!(candidates.iter().all(|item| {
            item.url
                .starts_with("https://silverplumetown.com/wp-content/uploads/")
        }));
    }

    #[test]
    fn moffat_and_simla_document_indexes_yield_only_reviewed_upload_pdfs() {
        for (source, policy, raw, digest) in [
            (
                "https://www.townofmoffatco.gov/documents",
                "https://www.townofmoffatco.gov/uploads/",
                include_bytes!("../../../fixtures/co-moffat/index-excerpt.html").as_slice(),
                "d27ebab69581b050e9bb882e7238e59d6e7de3cc6462313bafc26beb4c0db58d",
            ),
            (
                "https://townofsimla.com/documents",
                "https://townofsimla.com/uploads/",
                include_bytes!("../../../fixtures/co-simla/index-excerpt.html").as_slice(),
                "cbdb51960b9879a9702de50b3e72232c8daa8cea4a559ef0feb720cb837d3297",
            ),
        ] {
            assert_eq!(sha256_hex(raw), digest);
            let source_url = url::Url::parse(source).unwrap();
            let policy_url = url::Url::parse(policy).unwrap();
            let candidates = parse_town_pdf_candidates(&source_url, &policy_url, raw).unwrap();
            assert_eq!(candidates.len(), 2);
            assert!(candidates.iter().all(|item| item.url.starts_with(policy)));
            assert!(candidates.iter().all(|item| item.kind == "document"));
            assert!(!town_pdf_url_allowed(
                &policy_url,
                &url::Url::parse(&format!(
                    "{}uploads-evil/minutes.pdf",
                    source_url.origin().ascii_serialization()
                ))
                .unwrap()
            ));
        }
    }

    #[test]
    fn town_pdf_index_rejects_cross_host_paths_queries_and_long_urls() {
        let url = url::Url::parse("https://silverplumetown.com/agendas-and-minutes/").unwrap();
        let policy = url::Url::parse("https://silverplumetown.com/wp-content/uploads/").unwrap();
        let long_url = format!("/wp-content/uploads/{}.pdf", "x".repeat(2_100));
        let html = format!(
            "<html><body>\
            <a href='/wp-content/uploads/2026/11/minutes.pdf'>valid</a>\
            <a href='https://evil.example/wp-content/uploads/2026/11/minutes.pdf'>cross host</a>\
            <a href='https://silverplumetown.com.evil.example/wp-content/uploads/a.pdf'>lookalike</a>\
            <a href='/other/minutes.pdf'>wrong path</a>\
            <a href='/wp-content/uploads/minutes.pdf.exe'>wrong extension</a>\
            <a href='/wp-content/uploads/minutes.pdf?download=1'>query</a>\
            <a href='/wp-content/uploads/minutes.pdf#x'>fragment</a>\
            <a href='{long_url}'>long URL</a>\
            <a href='/wp-content/uploads/2026/11/minutes.pdf'>duplicate</a>\
            </body></html>"
        );
        let candidates = parse_town_pdf_candidates(&url, &policy, html.as_bytes()).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].anchor_index, 0);
        assert_eq!(
            candidates[0].url,
            "https://silverplumetown.com/wp-content/uploads/2026/11/minutes.pdf"
        );
        assert!(parse_town_pdf_candidates(&url, &policy, b"<html>no documents</html>").is_err());
        assert!(parse_town_pdf_candidates(&url, &policy, b"not utf8 \xff").is_err());
    }

    #[test]
    fn calendar_origin_requires_public_nonredirected_parseable_receipt() {
        let url = "https://official.example.test/Calendar.aspx";
        let raw = b"<html>calendar bytes</html>";
        let digest = sha256_hex(raw);
        let receipt = FetchObservation {
            id: "receipt".to_owned(),
            source_id: Some("test-calendar".to_owned()),
            requested_url: url.to_owned(),
            resolved_ips: vec!["1.1.1.1".to_owned()],
            retrieved_at: "2026-11-10T00:00:00Z".to_owned(),
            method: "GET".to_owned(),
            status_code: 200,
            redirect_target: None,
            final_url: url.to_owned(),
            allowlisted_headers: Vec::new(),
            content_type: Some("text/html".to_owned()),
            content_length: None,
            etag: None,
            last_modified: None,
            body_digest: Some(digest.clone()),
            error: None,
        };
        let matches = |receipt: &FetchObservation| {
            calendar_receipt_matches(receipt, "test-calendar", url, &digest, raw)
        };
        assert!(matches(&receipt));
        for addresses in [
            Vec::new(),
            vec!["127.0.0.1".to_owned()],
            vec!["1.1.1.1".to_owned(), "10.0.0.1".to_owned()],
            vec!["not-an-ip".to_owned()],
        ] {
            let mut invalid = receipt.clone();
            invalid.resolved_ips = addresses;
            assert!(!matches(&invalid));
        }
        let mut redirected = receipt.clone();
        redirected.redirect_target = Some(url.to_owned());
        assert!(!matches(&redirected));
        let mut invalid_time = receipt;
        invalid_time.retrieved_at = "yesterday".to_owned();
        assert!(!matches(&invalid_time));
        let mut wrong_mime = invalid_time.clone();
        wrong_mime.retrieved_at = "2026-11-10T00:00:00Z".to_owned();
        wrong_mime.content_type = Some("text/plain".to_owned());
        assert!(!matches(&wrong_mime));
    }

    #[test]
    fn calendar_candidate_artifact_is_private_and_immutable() {
        let temporary = tempdir().unwrap();
        let digest = sha256_hex(b"calendar bytes");
        let manifest = CalendarCandidates {
            schema_version: 1,
            source_id: "test-calendar".to_owned(),
            source_url: "https://council.example.test/Calendar.aspx".to_owned(),
            snapshot_sha256: digest,
            candidates: vec![CalendarCandidate {
                kind: "meeting".to_owned(),
                url: "https://council.example.test/MeetingDetail.aspx?ID=1".to_owned(),
                anchor_index: 3,
                label: "Meeting details".to_owned(),
            }],
        };
        let path = persist_calendar_candidates(temporary.path(), &manifest).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            persist_calendar_candidates(temporary.path(), &manifest).unwrap(),
            path
        );
        fs::write(&path, b"tampered").unwrap();
        assert!(persist_calendar_candidates(temporary.path(), &manifest).is_err());
    }

    #[test]
    fn private_artifact_paths_reject_parent_and_file_symlinks() {
        let data = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let manifest = CalendarCandidates {
            schema_version: 1,
            source_id: "test-calendar".to_owned(),
            source_url: "https://official.example.test/Calendar.aspx".to_owned(),
            snapshot_sha256: sha256_hex(b"calendar"),
            candidates: Vec::new(),
        };
        let record = CandidateArchive {
            schema_version: 1,
            calendar_source_id: "test-calendar",
            calendar_snapshot_sha256: &manifest.snapshot_sha256,
            anchor_index: 1,
            candidate_kind: "meeting",
            candidate_url: "https://official.example.test/MeetingDetail.aspx?ID=1",
            document_source_id: "test-meeting-documents",
            document_source_config_digest: sha256_hex(b"policy"),
            document_source_review_id: "review-1",
            document_sha256: "document-digest",
            changed_from_sha256: None,
            fetch_receipt_id: "receipt-1",
            retrieved_at: "2026-11-10T00:00:00Z",
        };
        for (base, attempt) in [
            (
                "discovery-candidates",
                persist_calendar_candidates(data.path(), &manifest).map(|_| ()),
            ),
            (
                "discovery-archives",
                persist_candidate_archive(data.path(), &record).map(|_| ()),
            ),
        ] {
            attempt.unwrap();
            let path = data.path().join(base);
            fs::remove_dir_all(&path).unwrap();
            symlink(outside.path(), &path).unwrap();
            let result = if base == "discovery-candidates" {
                persist_calendar_candidates(data.path(), &manifest)
            } else {
                persist_candidate_archive(data.path(), &record)
            };
            assert!(result.unwrap_err().to_string().contains("symlink"));
            assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
            fs::remove_file(&path).unwrap();
        }

        let candidate_path = persist_calendar_candidates(data.path(), &manifest).unwrap();
        let candidate_bytes = fs::read(&candidate_path).unwrap();
        let external_candidate = outside.path().join("candidate.json");
        fs::write(&external_candidate, &candidate_bytes).unwrap();
        fs::remove_file(&candidate_path).unwrap();
        symlink(&external_candidate, &candidate_path).unwrap();
        assert!(
            persist_calendar_candidates(data.path(), &manifest)
                .unwrap_err()
                .to_string()
                .contains("differs")
        );
        assert_eq!(fs::read(&external_candidate).unwrap(), candidate_bytes);

        let archive_path = persist_candidate_archive(data.path(), &record).unwrap();
        let archive_bytes = fs::read(&archive_path).unwrap();
        let external_archive = outside.path().join("archive.json");
        fs::write(&external_archive, &archive_bytes).unwrap();
        fs::remove_file(&archive_path).unwrap();
        symlink(&external_archive, &archive_path).unwrap();
        assert!(
            persist_candidate_archive(data.path(), &record)
                .unwrap_err()
                .to_string()
                .contains("differs")
        );
        assert_eq!(fs::read(&external_archive).unwrap(), archive_bytes);
    }

    fn test_config(source_type: &str, interval: u64) -> StateConfig {
        StateConfig {
            state_code: "CO".to_owned(),
            state_name: "Colorado".to_owned(),
            jurisdiction: "Test".to_owned(),
            canonical_base_url: "https://example.test".to_owned(),
            x_feed_label: "Test".to_owned(),
            default_dry_run: true,
            sources: vec![SourceConfig {
                id: SOURCE_ID.to_owned(),
                name: "Official decision".to_owned(),
                url: SOURCE_URL.to_owned(),
                source_type: source_type.to_owned(),
                documented_at: SOURCE_URL.to_owned(),
                official_discovery_url: SOURCE_URL.to_owned(),
                minimum_interval_seconds: interval,
                maximum_bytes: 1024,
                robots_status: "reviewed".to_owned(),
                terms_note: "reviewed".to_owned(),
            }],
            demo_documents: vec![],
        }
    }

    fn install_review(store: &Store, config: &StateConfig, expired: bool) {
        let captured_at = OffsetDateTime::now_utc().format(&Rfc3339).expect("time");
        let robots_url = "https://official.example.test/robots.txt";
        let robots = b"User-agent: *\nAllow: /\n";
        let digest = sha256_hex(robots);
        let observation = FetchObservation {
            id: FetchObservation::id_for(robots_url, &captured_at, 200),
            source_id: Some(SOURCE_ID.to_owned()),
            requested_url: robots_url.to_owned(),
            resolved_ips: vec!["1.1.1.1".to_owned()],
            retrieved_at: captured_at.clone(),
            method: "GET".to_owned(),
            status_code: 200,
            redirect_target: None,
            final_url: robots_url.to_owned(),
            allowlisted_headers: vec![],
            content_type: Some("text/plain".to_owned()),
            content_length: Some(robots.len() as u64),
            etag: None,
            last_modified: None,
            body_digest: Some(digest.clone()),
            error: None,
        };
        store
            .persist_content(&digest, robots)
            .expect("robots bytes");
        store
            .insert_fetch_observation(&observation)
            .expect("robots receipt");
        let review = SourceReview {
            id: SourceReview::id_for(SOURCE_ID, &captured_at),
            source_id: SOURCE_ID.to_owned(),
            source_config_digest: source_config_digest(&config.sources[0]),
            reviewed_hosts: vec!["official.example.test".to_owned()],
            endpoint_patterns: vec!["/decision.html".to_owned()],
            robots_url: robots_url.to_owned(),
            robots_snapshot_digest: digest,
            robots_provenance: Some(observation.id),
            terms_urls: vec![],
            terms_snapshot_digests: vec![],
            reviewer: "test".to_owned(),
            note: "reviewed robots and terms".to_owned(),
            reviewed_at: captured_at,
            expires_at: if expired { "2020-01-01" } else { "2099-01-01" }.to_owned(),
            minimum_interval_seconds: config.sources[0].minimum_interval_seconds,
            restrictions: vec![],
            supersedes: None,
        };
        store.insert_source_review(&review).expect("source review");
    }

    fn candidate_config(meeting_interval: u64, pdf_interval: u64) -> StateConfig {
        let mut config = test_config("html_page", 0);
        let make = |id: &str, path: &str, kind: &str, interval: u64| SourceConfig {
            id: id.to_owned(),
            name: format!("Official {kind}"),
            url: format!("https://official.example.test/{path}"),
            source_type: kind.to_owned(),
            documented_at: "https://official.example.test/Calendar.aspx".to_owned(),
            official_discovery_url: "https://official.example.test/Calendar.aspx".to_owned(),
            minimum_interval_seconds: interval,
            maximum_bytes: 4096,
            robots_status: "reviewed".to_owned(),
            terms_note: "reviewed".to_owned(),
        };
        config.sources = vec![
            make("test-calendar", "Calendar.aspx", "html_page", 0),
            make(
                "test-meeting-documents",
                "MeetingDetail.aspx",
                "candidate_meeting_html",
                meeting_interval,
            ),
            make(
                "test-pdf-documents",
                "View.ashx",
                "candidate_document_pdf",
                pdf_interval,
            ),
        ];
        config
    }

    fn town_index_config() -> StateConfig {
        let mut config = candidate_config(0, 0);
        config.sources.truncate(2);
        config.sources[0].id = "test-index".to_owned();
        config.sources[0].url = "https://official.example.test/agendas-and-minutes/".to_owned();
        config.sources[1].id = "test-index-pdf".to_owned();
        config.sources[1].url = "https://official.example.test/wp-content/uploads/".to_owned();
        config.sources[1].source_type = "candidate_index_pdf".to_owned();
        config.sources[1].official_discovery_url = config.sources[0].url.clone();
        config.sources[1].documented_at = config.sources[0].url.clone();
        config
    }

    #[test]
    fn town_index_pdf_policy_accepts_only_reviewed_upload_prefix() {
        let config = town_index_config();
        let policy = &config.sources[1];
        let valid = "https://official.example.test/wp-content/uploads/2026/11/board-minutes.pdf";
        assert!(checked_archive_url(policy, &policy.id, valid, "pdf").is_ok());
        for invalid in [
            "https://evil.example/wp-content/uploads/2026/11/board-minutes.pdf",
            "https://official.example.test/wp-content/uploads-evil/minutes.pdf",
            "https://official.example.test/wp-content/uploads/minutes.pdf?x=1",
            "https://official.example.test/wp-content/uploads/minutes.pdf#x",
            "https://official.example.test/wp-content/uploads/%2e%2e/minutes.pdf",
            "https://official.example.test/wp-content/uploads/%252e%252e/minutes.pdf",
            "https://official.example.test/wp-content/uploads//minutes.pdf",
            "https://official.example.test/wp-content/uploads/minutes.pdf.exe",
        ] {
            assert!(checked_archive_url(policy, &policy.id, invalid, "pdf").is_err());
        }
    }

    #[test]
    fn town_candidate_checks_preserved_robots_for_exact_pdf_path_before_fetch() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let config = town_index_config();
        install_candidate_review(&store, &config, "test-index");
        install_candidate_review_with_robots(
            &store,
            &config,
            "test-index-pdf",
            b"User-agent: *\nDisallow: /wp-content/uploads/private/\n",
        );
        let (digest, candidates) = seed_calendar(
            &store,
            &config,
            br"<html><a href='/wp-content/uploads/private/secret.pdf'>private</a><a href='/wp-content/uploads/public.pdf'>public</a></html>",
        );
        let mut fetcher = FakeFetcher {
            body: b"%PDF-1.7\nPublic meeting\n".to_vec(),
            content_type: "application/pdf".to_owned(),
            ..FakeFetcher::default()
        };
        let denied = archive_candidate_with(
            &store,
            &config,
            "test-index",
            &digest,
            candidates[0].anchor_index,
            "test-index-pdf",
            &mut fetcher,
        )
        .unwrap_err();
        assert!(denied.to_string().contains("robots.txt disallows"));
        assert_eq!(fetcher.calls, 0);
        archive_candidate_with(
            &store,
            &config,
            "test-index",
            &digest,
            candidates[1].anchor_index,
            "test-index-pdf",
            &mut fetcher,
        )
        .unwrap();
        assert_eq!(fetcher.calls, 1);
    }

    #[test]
    fn towncloud_document_index_policy_requires_exact_index_and_upload_path() {
        let mut config = town_index_config();
        config.sources[0].url = "https://official.example.test/documents".to_owned();
        config.sources[1].url = "https://official.example.test/uploads/".to_owned();
        config.sources[1].official_discovery_url = config.sources[0].url.clone();
        config.sources[1].documented_at = config.sources[0].url.clone();
        let index_url = url::Url::parse(&config.sources[0].url).unwrap();
        let policy = town_index_pdf_policy(&config, &index_url).unwrap();
        assert!(
            checked_archive_url(
                policy,
                &policy.id,
                "https://official.example.test/uploads/meeting-minutes.pdf",
                "pdf"
            )
            .is_ok()
        );
        for invalid in [
            "https://official.example.test/wp-content/uploads/minutes.pdf",
            "https://official.example.test/uploads-evil/minutes.pdf",
            "https://official.example.test/uploads/minutes.pdf?download=1",
            "https://evil.example/uploads/minutes.pdf",
        ] {
            assert!(checked_archive_url(policy, &policy.id, invalid, "pdf").is_err());
        }
        config.sources[1].official_discovery_url =
            "https://official.example.test/unrelated".to_owned();
        assert!(town_index_pdf_policy(&config, &index_url).is_err());
    }

    fn install_candidate_review(store: &Store, config: &StateConfig, source_id: &str) {
        install_candidate_review_with_robots(
            store,
            config,
            source_id,
            b"User-agent: *\nAllow: /\n",
        );
    }

    fn install_candidate_review_with_robots(
        store: &Store,
        config: &StateConfig,
        source_id: &str,
        robots: &[u8],
    ) {
        let source = config
            .sources
            .iter()
            .find(|item| item.id == source_id)
            .expect("source");
        let captured_at = OffsetDateTime::now_utc().format(&Rfc3339).expect("time");
        let robots_url = "https://official.example.test/robots.txt";
        let robots_digest = sha256_hex(robots);
        let receipt = FetchObservation {
            id: stable_id("test-candidate-robots", &[source_id, &captured_at]),
            source_id: Some(source_id.to_owned()),
            requested_url: robots_url.to_owned(),
            resolved_ips: vec!["1.1.1.1".to_owned()],
            retrieved_at: captured_at.clone(),
            method: "GET".to_owned(),
            status_code: 200,
            redirect_target: None,
            final_url: robots_url.to_owned(),
            allowlisted_headers: vec![],
            content_type: Some("text/plain".to_owned()),
            content_length: Some(robots.len() as u64),
            etag: None,
            last_modified: None,
            body_digest: Some(robots_digest.clone()),
            error: None,
        };
        store
            .persist_content(&robots_digest, robots)
            .expect("robots");
        store.insert_fetch_observation(&receipt).expect("receipt");
        let review = SourceReview {
            id: SourceReview::id_for(source_id, &captured_at),
            source_id: source_id.to_owned(),
            source_config_digest: source_config_digest(source),
            reviewed_hosts: vec!["official.example.test".to_owned()],
            endpoint_patterns: vec![url::Url::parse(&source.url).expect("URL").path().to_owned()],
            robots_url: robots_url.to_owned(),
            robots_snapshot_digest: robots_digest,
            robots_provenance: Some(receipt.id),
            terms_urls: vec![],
            terms_snapshot_digests: vec![],
            reviewer: "Policy Reviewer".to_owned(),
            note: "reviewed robots and terms".to_owned(),
            reviewed_at: captured_at,
            expires_at: "2099-01-01".to_owned(),
            minimum_interval_seconds: source.minimum_interval_seconds,
            restrictions: vec![],
            supersedes: None,
        };
        store.insert_source_review(&review).expect("review");
    }

    fn seed_calendar(
        store: &Store,
        config: &StateConfig,
        raw: &[u8],
    ) -> (String, Vec<CalendarCandidate>) {
        let calendar = &config.sources[0];
        let digest = sha256_hex(raw);
        store.persist_content(&digest, raw).expect("calendar blob");
        let retrieved_at = OffsetDateTime::now_utc().format(&Rfc3339).expect("time");
        store
            .insert_fetch_observation(&FetchObservation {
                id: stable_id("test-calendar-receipt", &[&calendar.id, &digest]),
                source_id: Some(calendar.id.clone()),
                requested_url: calendar.url.clone(),
                resolved_ips: vec!["1.1.1.1".to_owned()],
                retrieved_at,
                method: "GET".to_owned(),
                status_code: 200,
                redirect_target: None,
                final_url: calendar.url.clone(),
                allowlisted_headers: vec![],
                content_type: Some("text/html".to_owned()),
                content_length: Some(raw.len() as u64),
                etag: None,
                last_modified: None,
                body_digest: Some(digest.clone()),
                error: None,
            })
            .expect("calendar receipt");
        let source_url = url::Url::parse(&calendar.url).unwrap();
        let candidates = if source_url.path() == "/Calendar.aspx" {
            parse_calendar_candidates(&source_url, raw)
        } else {
            let policy_url =
                url::Url::parse(&town_index_pdf_policy(config, &source_url).unwrap().url).unwrap();
            parse_town_pdf_candidates(&source_url, &policy_url, raw)
        }
        .expect("candidates");
        let artifact = CalendarCandidates {
            schema_version: 1,
            source_id: calendar.id.clone(),
            source_url: calendar.url.clone(),
            snapshot_sha256: digest.clone(),
            candidates: candidates.clone(),
        };
        persist_calendar_candidates(store.data_dir(), &artifact).expect("artifact");
        (digest, candidates)
    }

    #[test]
    #[allow(clippy::too_many_lines)] // One lifecycle verifies dedupe, checkpoint, and revision behavior.
    fn queue_deduplicates_snapshots_fetches_one_per_policy_and_resumes_from_receipts() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let config = candidate_config(0, 0);
        for source in [
            "test-calendar",
            "test-meeting-documents",
            "test-pdf-documents",
        ] {
            install_candidate_review(&store, &config, source);
        }
        let (first_digest, _) = seed_calendar(&store, &config,
            br"<html><a href='MeetingDetail.aspx?ID=1'>one</a><a href='MeetingDetail.aspx?ID=2'>two</a></html>");
        seed_calendar(&store, &config,
            br"<html><a href='MeetingDetail.aspx?ID=1'>repeat</a><a href='MeetingDetail.aspx?ID=2'>two</a><a href='View.ashx?M=A&amp;ID=3'>agenda</a></html>");
        let now = OffsetDateTime::now_utc();
        let plan =
            plan_candidate_queue(&store, &config, "test-calendar", None, 8, 86_400, now).unwrap();
        assert_eq!(plan.unique_candidates, 3);
        assert_eq!(plan.selected.len(), 2); // one per reviewed path policy
        assert!(
            plan.selected
                .iter()
                .any(|entry| entry.policy_id == "test-meeting-documents")
        );
        assert!(
            plan.selected
                .iter()
                .any(|entry| entry.policy_id == "test-pdf-documents")
        );

        let first = plan_candidate_queue(
            &store,
            &config,
            "test-calendar",
            Some(&first_digest),
            1,
            86_400,
            now,
        )
        .unwrap();
        assert_eq!(first.selected.len(), 1);
        let first_url = first.selected[0].candidate.url.clone();
        let mut fetcher = FakeFetcher {
            body: b"<html>meeting version one</html>".to_vec(),
            content_type: "text/html".to_owned(),
            ..FakeFetcher::default()
        };
        let run =
            archive_queue_with(&store, &config, "test-calendar", first, &mut fetcher).unwrap();
        assert_eq!(run.successes.len(), 1);
        assert!(run.failures.is_empty());
        assert_eq!(fetcher.calls, 1);
        let next = plan_candidate_queue(
            &store,
            &config,
            "test-calendar",
            Some(&first_digest),
            1,
            86_400,
            now + time::Duration::minutes(1),
        )
        .unwrap();
        assert_eq!(next.selected.len(), 1);
        assert_ne!(next.selected[0].candidate.url, first_url);

        let mut changed = FakeFetcher {
            body: b"<html>meeting version two</html>".to_vec(),
            content_type: "text/html".to_owned(),
            ..FakeFetcher::default()
        };
        let refresh = plan_candidate_queue(
            &store,
            &config,
            "test-calendar",
            Some(&first_digest),
            1,
            86_400,
            now + time::Duration::days(2),
        )
        .unwrap();
        // New URLs take priority over refreshes. After archiving the second URL,
        // the older one can be rechecked and a changed response is retained.
        let second =
            archive_queue_with(&store, &config, "test-calendar", refresh, &mut changed).unwrap();
        assert_eq!(second.successes.len(), 1);
        let refresh = plan_candidate_queue(
            &store,
            &config,
            "test-calendar",
            Some(&first_digest),
            1,
            86_400,
            now + time::Duration::days(2),
        )
        .unwrap();
        assert_eq!(refresh.selected[0].candidate.url, first_url);
        let mut changed = FakeFetcher {
            body: b"<html>meeting version two</html>".to_vec(),
            content_type: "text/html".to_owned(),
            ..FakeFetcher::default()
        };
        let refreshed =
            archive_queue_with(&store, &config, "test-calendar", refresh, &mut changed).unwrap();
        assert_eq!(refreshed.successes.len(), 1);
        assert!(refreshed.successes[0].1.changed_from.is_some());
        assert_eq!(
            store
                .fetch_observations("test-meeting-documents")
                .unwrap()
                .into_iter()
                .filter(|item| item.requested_url.contains("/MeetingDetail.aspx"))
                .count(),
            3
        );
    }

    #[test]
    fn queue_rejects_drift_and_tampering_before_fetch() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let config = candidate_config(0, 0);
        install_candidate_review(&store, &config, "test-calendar");
        let (digest, _) = seed_calendar(
            &store,
            &config,
            br"<html><a href='MeetingDetail.aspx?ID=1'>one</a></html>",
        );
        let now = OffsetDateTime::now_utc();
        let unavailable =
            plan_candidate_queue(&store, &config, "test-calendar", None, 1, 86_400, now).unwrap();
        assert!(unavailable.selected.is_empty());
        assert!(matches!(
            unavailable.state_updates.first().map(|item| &item.1),
            Some(CandidateQueueOutcome::Blocked { .. })
        ));
        install_candidate_review(&store, &config, "test-meeting-documents");
        assert_eq!(
            plan_candidate_queue(&store, &config, "test-calendar", None, 1, 86_400, now)
                .unwrap()
                .selected
                .len(),
            1
        );
        let mut changed = config.clone();
        changed.sources[1].maximum_bytes += 1;
        let drift =
            plan_candidate_queue(&store, &changed, "test-calendar", None, 1, 86_400, now).unwrap();
        assert!(drift.selected.is_empty());
        assert!(matches!(
            drift.state_updates.first().map(|item| &item.1),
            Some(CandidateQueueOutcome::Blocked { .. })
        ));
        let artifact = store
            .data_dir()
            .join("discovery-candidates/test-calendar")
            .join(format!("{digest}.json"));
        fs::write(&artifact, b"{}").unwrap();
        let tampered =
            plan_candidate_queue(&store, &config, "test-calendar", None, 1, 86_400, now).unwrap();
        assert!(tampered.selected.is_empty());
        assert!(matches!(
            tampered.state_updates.first().map(|item| &item.1),
            Some(CandidateQueueOutcome::Blocked { .. })
        ));
    }

    #[test]
    fn explicit_queue_snapshot_skips_unbounded_directory_scan() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let path = store.data_dir().join("discovery-candidates/test-calendar");
        private_directory(store.data_dir(), &path).unwrap();
        for index in 0..=130 {
            fs::write(path.join(format!("{index:064x}.json")), b"{}").unwrap();
        }
        assert!(queue_snapshot_digests(&store, "test-calendar", None).is_err());
        let digest = format!("{:064x}", 1);
        assert_eq!(
            queue_snapshot_digests(&store, "test-calendar", Some(&digest)).unwrap(),
            vec![digest]
        );
        assert!(queue_snapshot_digests(&store, "test-calendar", Some("../outside")).is_err());
    }

    #[test]
    fn indexed_queue_progresses_after_more_than_128_immutable_snapshots() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let config = candidate_config(0, 0);
        install_candidate_review(&store, &config, "test-calendar");
        install_candidate_review(&store, &config, "test-meeting-documents");
        for index in 0..130 {
            let html =
                format!("<html><a href='MeetingDetail.aspx?ID={index}'>meeting {index}</a></html>");
            seed_calendar(&store, &config, html.as_bytes());
        }
        let now = OffsetDateTime::now_utc();
        let first =
            plan_candidate_queue(&store, &config, "test-calendar", None, 1, 86_400, now).unwrap();
        assert_eq!(first.unique_candidates, 130);
        assert_eq!(first.selected.len(), 1);
        let first_url = first.selected[0].candidate.url.clone();
        let mut fetcher = FakeFetcher {
            body: b"<html>official meeting</html>".to_vec(),
            content_type: "text/html".to_owned(),
            ..FakeFetcher::default()
        };
        archive_queue_with(&store, &config, "test-calendar", first, &mut fetcher).unwrap();
        let next = plan_candidate_queue(
            &store,
            &config,
            "test-calendar",
            None,
            1,
            86_400,
            now + time::Duration::seconds(61),
        )
        .unwrap();
        assert_eq!(next.unique_candidates, 130);
        assert_eq!(next.selected.len(), 1);
        assert_ne!(next.selected[0].candidate.url, first_url);
    }

    #[test]
    fn indexed_queue_marks_disallowed_first_url_and_fetches_allowed_url() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let config = town_index_config();
        install_candidate_review(&store, &config, "test-index");
        install_candidate_review_with_robots(
            &store,
            &config,
            "test-index-pdf",
            b"User-agent: *\nDisallow: /wp-content/uploads/a-private.pdf\n",
        );
        seed_calendar(
            &store,
            &config,
            br"<html><a href='/wp-content/uploads/a-private.pdf'>private</a><a href='/wp-content/uploads/z-public.pdf'>public</a></html>",
        );
        let now = OffsetDateTime::now_utc();
        let plan =
            plan_candidate_queue(&store, &config, "test-index", None, 1, 86_400, now).unwrap();
        assert_eq!(plan.selected.len(), 1);
        assert!(plan.selected[0].candidate.url.ends_with("/z-public.pdf"));
        assert!(matches!(
            plan.state_updates.first().map(|item| &item.1),
            Some(CandidateQueueOutcome::Blocked { .. })
        ));
        let mut fetcher = FakeFetcher {
            body: b"%PDF-1.7\npublic minutes\n".to_vec(),
            content_type: "application/pdf".to_owned(),
            ..FakeFetcher::default()
        };
        let run = archive_queue_with(&store, &config, "test-index", plan, &mut fetcher).unwrap();
        assert_eq!(run.successes.len(), 1);
        assert_eq!(fetcher.calls, 1);
        assert!(fetcher.urls[0].ends_with("/z-public.pdf"));
    }

    #[test]
    fn unavailable_pdf_review_does_not_stop_reviewed_meeting_candidate() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let config = candidate_config(0, 0);
        install_candidate_review(&store, &config, "test-calendar");
        install_candidate_review(&store, &config, "test-meeting-documents");
        seed_calendar(
            &store,
            &config,
            br"<html><a href='View.ashx?M=A&amp;ID=1'>agenda</a><a href='MeetingDetail.aspx?ID=2'>meeting</a></html>",
        );
        let now = OffsetDateTime::now_utc();
        let plan =
            plan_candidate_queue(&store, &config, "test-calendar", None, 2, 86_400, now).unwrap();
        assert_eq!(plan.selected.len(), 1);
        assert_eq!(plan.selected[0].policy_id, "test-meeting-documents");
        assert_eq!(plan.state_updates.len(), 1);
        assert!(matches!(
            &plan.state_updates[0].1,
            CandidateQueueOutcome::Blocked { context }
                if context.contains("policy review unavailable")
        ));
        let mut fetcher = FakeFetcher {
            body: b"<html><body>official meeting</body></html>".to_vec(),
            content_type: "text/html".to_owned(),
            ..FakeFetcher::default()
        };
        let run = archive_queue_with(&store, &config, "test-calendar", plan, &mut fetcher).unwrap();
        assert_eq!(run.successes.len(), 1);
        assert_eq!(fetcher.calls, 1);
        assert!(fetcher.urls[0].contains("MeetingDetail.aspx"));
    }

    #[test]
    fn missing_origin_review_cannot_requeue_blocked_private_state() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let config = candidate_config(0, 0);
        let (_, candidates) = seed_calendar(
            &store,
            &config,
            br"<html><a href='MeetingDetail.aspx?ID=2'>meeting</a></html>",
        );
        let url = &candidates[0].url;
        store
            .mark_candidate_queue_outcome(
                "test-calendar",
                url,
                1,
                CandidateQueueOutcome::Blocked {
                    context: "outdated policy".to_owned(),
                },
            )
            .unwrap();
        assert!(archive_queue(dir.path(), &config, "test-calendar", None, 1, 60, false).is_err());
        assert!(
            store
                .candidate_queue_due("test-calendar", 10, 60, 1)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn indexed_queue_skips_historical_out_of_scope_url_without_stopping_other_policy() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let config = candidate_config(0, 0);
        for id in [
            "test-calendar",
            "test-meeting-documents",
            "test-pdf-documents",
        ] {
            install_candidate_review(&store, &config, id);
        }
        seed_calendar(
            &store,
            &config,
            br"<html><a href='View.ashx?M=A&amp;ID=1'>old agenda</a></html>",
        );
        seed_calendar(
            &store,
            &config,
            br"<html><a href='MeetingDetail.aspx?ID=2'>current meeting</a></html>",
        );
        let mut changed = config.clone();
        changed.sources[2].official_discovery_url =
            "https://official.example.test/another-index".to_owned();
        let plan = plan_candidate_queue(
            &store,
            &changed,
            "test-calendar",
            None,
            1,
            86_400,
            OffsetDateTime::now_utc(),
        )
        .unwrap();
        assert_eq!(plan.selected.len(), 1);
        assert!(
            plan.selected[0]
                .candidate
                .url
                .contains("MeetingDetail.aspx")
        );
        assert!(plan.state_updates.iter().any(|(url, outcome, _)| {
            url.contains("View.ashx") && matches!(outcome, CandidateQueueOutcome::Blocked { .. })
        }));
    }

    #[test]
    fn indexed_queue_limits_each_planning_window() {
        use std::fmt::Write as _;

        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let mut config = candidate_config(0, 0);
        config.sources[0].maximum_bytes = 65_536;
        install_candidate_review(&store, &config, "test-calendar");
        install_candidate_review(&store, &config, "test-meeting-documents");
        let mut html = "<html>".to_owned();
        for index in 0..300 {
            write!(
                &mut html,
                "<a href='MeetingDetail.aspx?ID={index}'>meeting {index}</a>"
            )
            .unwrap();
        }
        html.push_str("</html>");
        seed_calendar(&store, &config, html.as_bytes());
        let plan = plan_candidate_queue(
            &store,
            &config,
            "test-calendar",
            None,
            1,
            86_400,
            OffsetDateTime::now_utc(),
        )
        .unwrap();
        assert_eq!(plan.unique_candidates, 300);
        assert_eq!(plan.selected.len(), 1);
        assert_eq!(plan.state_updates.len(), MAX_QUEUE_INDEX_WINDOW - 1);
    }

    #[test]
    fn queue_run_refreshes_current_link_despite_large_historical_backlog() {
        use std::fmt::Write as _;

        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let mut config = candidate_config(0, 0);
        config.sources[0].maximum_bytes = 65_536;
        install_candidate_review(&store, &config, "test-calendar");
        install_candidate_review(&store, &config, "test-meeting-documents");
        let (first_digest, first) = seed_calendar(
            &store,
            &config,
            br"<html><a href='MeetingDetail.aspx?ID=999'>current</a></html>",
        );
        let mut fetcher = FakeFetcher {
            body: b"<html>meeting</html>".to_vec(),
            content_type: "text/html".to_owned(),
            ..FakeFetcher::default()
        };
        archive_candidate_with(
            &store,
            &config,
            "test-calendar",
            &first_digest,
            first[0].anchor_index,
            "test-meeting-documents",
            &mut fetcher,
        )
        .unwrap();
        let now = OffsetDateTime::now_utc();
        store
            .mark_candidate_queue_outcome(
                "test-calendar",
                &first[0].url,
                now.unix_timestamp(),
                CandidateQueueOutcome::Archived,
            )
            .unwrap();

        let mut html = "<html>".to_owned();
        for index in 0..300 {
            write!(
                &mut html,
                "<a href='MeetingDetail.aspx?ID={index}'>historical {index}</a>"
            )
            .unwrap();
        }
        html.push_str("<a href='MeetingDetail.aspx?ID=999'>current</a></html>");
        seed_calendar(&store, &config, html.as_bytes());

        let first_plan = plan_candidate_queue(
            &store,
            &config,
            "test-calendar",
            None,
            1,
            1,
            now + time::Duration::seconds(2),
        )
        .unwrap();
        assert_ne!(first_plan.selected[0].candidate.url, first[0].url);
        let mut run_fetcher = FakeFetcher {
            body: b"<html>historical meeting</html>".to_vec(),
            content_type: "text/html".to_owned(),
            ..FakeFetcher::default()
        };
        let run = archive_queue_with(
            &store,
            &config,
            "test-calendar",
            first_plan,
            &mut run_fetcher,
        )
        .unwrap();
        assert_eq!(run.successes.len(), 1);

        let refresh_plan = plan_candidate_queue(
            &store,
            &config,
            "test-calendar",
            None,
            1,
            1,
            now + time::Duration::seconds(3),
        )
        .unwrap();
        assert_eq!(refresh_plan.selected[0].candidate.url, first[0].url);
    }

    #[test]
    fn blocked_candidate_context_changes_with_reviewed_policy() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let config = candidate_config(0, 0);
        install_candidate_review(&store, &config, "test-calendar");
        install_candidate_review(&store, &config, "test-meeting-documents");
        seed_calendar(
            &store,
            &config,
            br"<html><a href='MeetingDetail.aspx?ID=1'>meeting</a></html>",
        );
        let original = queue_context_token(&store, &config, "test-calendar").unwrap();
        let url = "https://official.example.test/MeetingDetail.aspx?ID=1";
        store
            .mark_candidate_queue_outcome(
                "test-calendar",
                url,
                10,
                queue_blocked(&original, "old path policy"),
            )
            .unwrap();
        assert!(
            store
                .candidate_queue_due("test-calendar", 11, 86_400, 1)
                .unwrap()
                .is_empty()
        );
        let mut changed = config;
        changed.sources[1].maximum_bytes += 1;
        let changed_token = queue_context_token(&store, &changed, "test-calendar").unwrap();
        assert_ne!(original, changed_token);
        assert_eq!(
            store
                .requeue_blocked_candidate_context("test-calendar", &changed_token, 1)
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .candidate_queue_due("test-calendar", 11, 86_400, 1)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn queue_reports_failed_document_without_losing_successful_archive() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let config = candidate_config(0, 0);
        for source in [
            "test-calendar",
            "test-meeting-documents",
            "test-pdf-documents",
        ] {
            install_candidate_review(&store, &config, source);
        }
        seed_calendar(&store, &config,
            br"<html><a href='MeetingDetail.aspx?ID=1'>meeting</a><a href='View.ashx?M=A&amp;ID=2'>agenda</a></html>");
        let plan = plan_candidate_queue(
            &store,
            &config,
            "test-calendar",
            None,
            8,
            86_400,
            OffsetDateTime::now_utc(),
        )
        .unwrap();
        assert_eq!(plan.selected.len(), 2);
        let mut fetcher = FakeFetcher {
            body: b"<html>meeting</html>".to_vec(),
            content_type: "text/html".to_owned(),
            ..FakeFetcher::default()
        };
        let run = archive_queue_with(&store, &config, "test-calendar", plan, &mut fetcher).unwrap();
        assert_eq!(fetcher.calls, 2);
        assert_eq!(run.successes.len(), 1);
        assert_eq!(run.failures.len(), 1);
        assert!(run.failures[0].contains("View.ashx"));
        assert!(
            store
                .content_path(&run.successes[0].1.digest)
                .unwrap()
                .is_file()
        );
    }

    #[test]
    fn queue_retries_failed_receipts_after_policy_cooldown() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let config = candidate_config(60, 0);
        install_candidate_review(&store, &config, "test-calendar");
        install_candidate_review(&store, &config, "test-meeting-documents");
        let (digest, candidates) = seed_calendar(
            &store,
            &config,
            br"<html><a href='MeetingDetail.aspx?ID=1'>meeting</a></html>",
        );
        let url = &candidates[0].url;
        let now = OffsetDateTime::now_utc();
        let failed_receipt = |at: OffsetDateTime| {
            let retrieved_at = at.format(&Rfc3339).unwrap();
            FetchObservation {
                id: FetchObservation::id_for(url, &retrieved_at, 503),
                source_id: Some("test-meeting-documents".to_owned()),
                requested_url: url.clone(),
                resolved_ips: vec!["1.1.1.1".to_owned()],
                retrieved_at,
                method: "GET".to_owned(),
                status_code: 503,
                redirect_target: None,
                final_url: url.clone(),
                allowlisted_headers: vec![],
                content_type: Some("text/html".to_owned()),
                content_length: None,
                etag: None,
                last_modified: None,
                body_digest: None,
                error: Some(pnull_core::StructuredError {
                    code: "fetch_failed".to_owned(),
                    message: "temporary failure".to_owned(),
                }),
            }
        };
        let first_failure = now - time::Duration::seconds(61);
        store
            .insert_fetch_observation(&failed_receipt(first_failure))
            .unwrap();
        store
            .record_source_fetch("test-meeting-documents", first_failure.unix_timestamp())
            .unwrap();
        assert_eq!(
            plan_candidate_queue(&store, &config, "test-calendar", None, 1, 604_800, now)
                .unwrap()
                .selected
                .len(),
            1
        );
        let mut fetcher = FakeFetcher {
            body: b"<html>meeting</html>".to_vec(),
            content_type: "text/html".to_owned(),
            ..FakeFetcher::default()
        };
        archive_candidate_with(
            &store,
            &config,
            "test-calendar",
            &digest,
            candidates[0].anchor_index,
            "test-meeting-documents",
            &mut fetcher,
        )
        .unwrap();
        let later_failure = now + time::Duration::days(8);
        store
            .insert_fetch_observation(&failed_receipt(later_failure))
            .unwrap();
        store
            .record_source_fetch("test-meeting-documents", later_failure.unix_timestamp())
            .unwrap();
        assert_eq!(
            plan_candidate_queue(
                &store,
                &config,
                "test-calendar",
                None,
                1,
                604_800,
                later_failure + time::Duration::seconds(61),
            )
            .unwrap()
            .selected
            .len(),
            1
        );
    }

    #[test]
    fn queue_backfills_removed_links_once_but_refreshes_only_current_links() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let config = candidate_config(0, 0);
        for source in ["test-calendar", "test-meeting-documents"] {
            install_candidate_review(&store, &config, source);
        }
        let (old_digest, old) = seed_calendar(
            &store,
            &config,
            br"<html><a href='MeetingDetail.aspx?ID=1'>old</a></html>",
        );
        let (new_digest, newer) = seed_calendar(
            &store,
            &config,
            br"<html><a href='MeetingDetail.aspx?ID=2'>current</a></html>",
        );
        let mut current_receipt = store
            .fetch_observations("test-calendar")
            .unwrap()
            .into_iter()
            .find(|item| item.body_digest.as_deref() == Some(&new_digest))
            .unwrap();
        current_receipt.retrieved_at = (OffsetDateTime::now_utc() + time::Duration::minutes(1))
            .format(&Rfc3339)
            .unwrap();
        current_receipt.id = stable_id("newest-calendar", &[&new_digest]);
        store.insert_fetch_observation(&current_receipt).unwrap();

        for (digest, candidate) in [(&old_digest, &old[0]), (&new_digest, &newer[0])] {
            let mut fetcher = FakeFetcher {
                body: b"<html>meeting</html>".to_vec(),
                content_type: "text/html".to_owned(),
                ..FakeFetcher::default()
            };
            archive_candidate_with(
                &store,
                &config,
                "test-calendar",
                digest,
                candidate.anchor_index,
                "test-meeting-documents",
                &mut fetcher,
            )
            .unwrap();
        }
        let plan = plan_candidate_queue(
            &store,
            &config,
            "test-calendar",
            None,
            8,
            86_400,
            OffsetDateTime::now_utc() + time::Duration::days(2),
        )
        .unwrap();
        assert_eq!(plan.unique_candidates, 2);
        assert_eq!(plan.selected.len(), 1);
        assert_eq!(plan.selected[0].candidate.url, newer[0].url);
    }

    #[derive(Default)]
    struct FakeFetcher {
        calls: usize,
        priors: Vec<Option<PriorEvidence>>,
        urls: Vec<String>,
        reviewed_paths: Vec<Option<Vec<String>>>,
        body: Vec<u8>,
        content_type: String,
        redirect_to: Option<String>,
    }

    impl ArchiveFetcher for FakeFetcher {
        fn fetch(
            &mut self,
            _store: &Store,
            source_id: &str,
            config: &FetchConfig,
            url: &str,
            retrieved_at: &str,
            prior: Option<&PriorEvidence>,
        ) -> Result<(Vec<u8>, Vec<FetchObservation>)> {
            self.calls += 1;
            self.priors.push(prior.cloned());
            self.urls.push(url.to_owned());
            self.reviewed_paths.push(config.reviewed_paths.clone());
            let unchanged = self.calls > 1;
            let status = if unchanged { 304 } else { 200 };
            let observation = FetchObservation {
                id: FetchObservation::id_for(url, retrieved_at, status),
                source_id: Some(source_id.to_owned()),
                requested_url: url.to_owned(),
                resolved_ips: vec!["1.1.1.1".to_owned()],
                retrieved_at: retrieved_at.to_owned(),
                method: "GET".to_owned(),
                status_code: status,
                redirect_target: self.redirect_to.clone(),
                final_url: self.redirect_to.clone().unwrap_or_else(|| url.to_owned()),
                allowlisted_headers: vec![],
                content_type: (!unchanged).then(|| self.content_type.clone()),
                content_length: (!unchanged).then_some(self.body.len() as u64),
                etag: (!unchanged).then(|| "\"v1\"".to_owned()),
                last_modified: None,
                body_digest: (!unchanged).then(|| sha256_hex(&self.body)),
                error: None,
            };
            Ok((self.body.clone(), vec![observation]))
        }
    }

    #[test]
    fn missing_and_expired_reviews_refuse_before_fetch() {
        let config = test_config("html_page", 0);
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("store");
        let mut fetcher = FakeFetcher {
            body: b"<!doctype html><html><body>Official record</body></html>".to_vec(),
            content_type: "text/html".to_owned(),
            ..FakeFetcher::default()
        };
        let err = archive_with(&store, &config, SOURCE_ID, &mut fetcher).unwrap_err();
        assert!(
            err.to_string()
                .contains("no current persistent source review")
        );
        assert_eq!(fetcher.calls, 0);

        install_review(&store, &config, true);
        let err = archive_with(&store, &config, SOURCE_ID, &mut fetcher).unwrap_err();
        assert!(err.to_string().contains("expired"));
        assert_eq!(fetcher.calls, 0);
        assert_eq!(
            store.fetch_observations(SOURCE_ID).expect("receipts").len(),
            1
        );
    }

    #[test]
    fn repeated_archive_uses_preserved_conditional_prior_and_keeps_exact_bytes() {
        let config = test_config("html_page", 0);
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("store");
        install_review(&store, &config, false);
        let body = b"<!doctype html><html><body>Official record</body></html>".to_vec();
        let digest = sha256_hex(&body);
        let mut fetcher = FakeFetcher {
            body: body.clone(),
            content_type: "text/html; charset=utf-8".to_owned(),
            ..FakeFetcher::default()
        };
        let first = archive_with(&store, &config, SOURCE_ID, &mut fetcher).expect("first archive");
        let second =
            archive_with(&store, &config, SOURCE_ID, &mut fetcher).expect("conditional archive");
        assert_eq!(first.digest, digest);
        assert_eq!(second.digest, digest);
        assert!(!first.unchanged);
        assert!(second.unchanged);
        assert!(fetcher.priors[0].is_none());
        assert_eq!(
            fetcher.priors[1].as_ref().map(|p| p.evidence_id.as_str()),
            Some(digest.as_str())
        );
        assert_eq!(
            fetcher.priors[1].as_ref().and_then(|p| p.etag.as_deref()),
            Some("\"v1\"")
        );
        assert_eq!(
            std::fs::read(store.content_path(&digest).expect("path")).expect("blob"),
            body
        );
        assert_eq!(
            store.fetch_observations(SOURCE_ID).expect("receipts").len(),
            3
        );
    }

    #[test]
    fn pdf_archive_rejects_html_response_and_type_confusion() {
        let config = test_config("pdf", 0);
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("store");
        install_review(&store, &config, false);
        let mut fetcher = FakeFetcher {
            body: b"<!doctype html><html>login</html>".to_vec(),
            content_type: "text/html".to_owned(),
            ..FakeFetcher::default()
        };
        let err = archive_with(&store, &config, SOURCE_ID, &mut fetcher).unwrap_err();
        assert!(err.to_string().contains("non-PDF bytes"));
    }

    #[test]
    fn archive_rejects_oversized_and_mismatched_receipts() {
        struct MissingReceipt;
        impl ArchiveFetcher for MissingReceipt {
            fn fetch(
                &mut self,
                _store: &Store,
                _source_id: &str,
                _config: &FetchConfig,
                _url: &str,
                _retrieved_at: &str,
                _prior: Option<&PriorEvidence>,
            ) -> Result<(Vec<u8>, Vec<FetchObservation>)> {
                Ok((b"<html>record</html>".to_vec(), vec![]))
            }
        }

        let config = test_config("html_page", 0);
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("store");
        install_review(&store, &config, false);
        let mut oversized = FakeFetcher {
            body: format!("<html>{}</html>", "x".repeat(1024)).into_bytes(),
            content_type: "text/html".to_owned(),
            ..FakeFetcher::default()
        };
        let err = archive_with(&store, &config, SOURCE_ID, &mut oversized).unwrap_err();
        assert!(err.to_string().contains("oversized"));
        assert_eq!(
            store.fetch_observations(SOURCE_ID).expect("receipts").len(),
            1
        );

        let err = archive_with(&store, &config, SOURCE_ID, &mut MissingReceipt).unwrap_err();
        assert!(err.to_string().contains("no provenance"));
        assert_eq!(
            store.fetch_observations(SOURCE_ID).expect("receipts").len(),
            1
        );
    }

    #[test]
    fn rejected_raw_response_is_not_a_conditional_or_revision_baseline() {
        let config = test_config("html_page", 0);
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        install_review(&store, &config, false);
        let rejected = b"not HTML";
        let rejected_digest = sha256_hex(rejected);
        store.persist_content(&rejected_digest, rejected).unwrap();
        store
            .insert_fetch_observation(&FetchObservation {
                id: FetchObservation::id_for(SOURCE_URL, "2025-01-01T00:00:00Z", 200),
                source_id: Some(SOURCE_ID.to_owned()),
                requested_url: SOURCE_URL.to_owned(),
                resolved_ips: vec!["1.1.1.1".to_owned()],
                retrieved_at: "2025-01-01T00:00:00Z".to_owned(),
                method: "GET".to_owned(),
                status_code: 200,
                redirect_target: None,
                final_url: SOURCE_URL.to_owned(),
                allowlisted_headers: Vec::new(),
                content_type: Some("text/plain".to_owned()),
                content_length: Some(rejected.len() as u64),
                etag: Some("\"bad\"".to_owned()),
                last_modified: None,
                body_digest: Some(rejected_digest),
                error: None,
            })
            .unwrap();
        let mut fetcher = FakeFetcher {
            body: b"<html>Valid official record</html>".to_vec(),
            content_type: "text/html".to_owned(),
            ..FakeFetcher::default()
        };
        let result = archive_with(&store, &config, SOURCE_ID, &mut fetcher).unwrap();
        assert!(fetcher.priors[0].is_none());
        assert!(result.changed_from.is_none());
    }

    #[test]
    fn nonpublic_receipt_is_not_a_conditional_or_revision_baseline() {
        let config = test_config("html_page", 0);
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        install_review(&store, &config, false);
        let prior_bytes = b"<html>Previous valid-looking record</html>";
        let prior_digest = sha256_hex(prior_bytes);
        store.persist_content(&prior_digest, prior_bytes).unwrap();
        store
            .insert_fetch_observation(&FetchObservation {
                id: FetchObservation::id_for(SOURCE_URL, "2025-01-01T00:00:00Z", 200),
                source_id: Some(SOURCE_ID.to_owned()),
                requested_url: SOURCE_URL.to_owned(),
                resolved_ips: vec!["127.0.0.1".to_owned()],
                retrieved_at: "2025-01-01T00:00:00Z".to_owned(),
                method: "GET".to_owned(),
                status_code: 200,
                redirect_target: None,
                final_url: SOURCE_URL.to_owned(),
                allowlisted_headers: Vec::new(),
                content_type: Some("text/html".to_owned()),
                content_length: Some(prior_bytes.len() as u64),
                etag: Some("\"old\"".to_owned()),
                last_modified: None,
                body_digest: Some(prior_digest),
                error: None,
            })
            .unwrap();
        let mut fetcher = FakeFetcher {
            body: b"<html>Current official record</html>".to_vec(),
            content_type: "text/html".to_owned(),
            ..FakeFetcher::default()
        };
        let result = archive_with(&store, &config, SOURCE_ID, &mut fetcher).unwrap();
        assert!(fetcher.priors[0].is_none());
        assert!(result.changed_from.is_none());
    }

    #[test]
    fn candidate_loader_rejects_symlinked_artifact_parent_before_fetch() {
        let config = candidate_config(0, 0);
        let dir = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        install_candidate_review(&store, &config, "test-calendar");
        let raw = br#"<html><a href="MeetingDetail.aspx?ID=25">Meeting</a></html>"#;
        let (digest, candidates) = seed_calendar(&store, &config, raw);
        let base = dir.path().join("discovery-candidates");
        let moved = outside.path().join("candidates");
        fs::rename(&base, &moved).unwrap();
        symlink(&moved, &base).unwrap();
        let mut fetcher = FakeFetcher {
            body: b"<html>meeting</html>".to_vec(),
            content_type: "text/html".to_owned(),
            ..FakeFetcher::default()
        };
        let error = archive_candidate_with(
            &store,
            &config,
            "test-calendar",
            &digest,
            candidates[0].anchor_index,
            "test-meeting-documents",
            &mut fetcher,
        )
        .unwrap_err();
        assert!(error.to_string().contains("symlink"));
        assert_eq!(fetcher.calls, 0);
    }

    #[test]
    fn candidate_archive_refuses_non_html_calendar_receipt_before_fetch() {
        let config = candidate_config(0, 0);
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        install_candidate_review(&store, &config, "test-calendar");
        install_candidate_review(&store, &config, "test-meeting-documents");
        let raw = br#"<html><a href="MeetingDetail.aspx?ID=25">Meeting</a></html>"#;
        let digest = sha256_hex(raw);
        store.persist_content(&digest, raw).unwrap();
        let calendar = &config.sources[0];
        let candidates =
            parse_calendar_candidates(&url::Url::parse(&calendar.url).unwrap(), raw).unwrap();
        persist_calendar_candidates(
            store.data_dir(),
            &CalendarCandidates {
                schema_version: 1,
                source_id: calendar.id.clone(),
                source_url: calendar.url.clone(),
                snapshot_sha256: digest.clone(),
                candidates: candidates.clone(),
            },
        )
        .unwrap();
        store
            .insert_fetch_observation(&FetchObservation {
                id: FetchObservation::id_for(&calendar.url, "2026-11-10T00:00:00Z", 200),
                source_id: Some(calendar.id.clone()),
                requested_url: calendar.url.clone(),
                resolved_ips: vec!["1.1.1.1".to_owned()],
                retrieved_at: "2026-11-10T00:00:00Z".to_owned(),
                method: "GET".to_owned(),
                status_code: 200,
                redirect_target: None,
                final_url: calendar.url.clone(),
                allowlisted_headers: Vec::new(),
                content_type: Some("text/plain".to_owned()),
                content_length: Some(raw.len() as u64),
                etag: None,
                last_modified: None,
                body_digest: Some(digest.clone()),
                error: None,
            })
            .unwrap();
        let mut fetcher = FakeFetcher {
            body: b"<html>meeting</html>".to_vec(),
            content_type: "text/html".to_owned(),
            ..FakeFetcher::default()
        };
        let error = archive_candidate_with(
            &store,
            &config,
            "test-calendar",
            &digest,
            candidates[0].anchor_index,
            "test-meeting-documents",
            &mut fetcher,
        )
        .unwrap_err();
        assert!(error.to_string().contains("exact official fetch receipt"));
        assert_eq!(fetcher.calls, 0);
    }

    #[test]
    fn calendar_candidate_uses_reviewed_query_path_and_preserves_304_chain() {
        let config = candidate_config(0, 0);
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("store");
        install_candidate_review(&store, &config, "test-calendar");
        install_candidate_review(&store, &config, "test-meeting-documents");
        let raw = br#"<html><a href="MeetingDetail.aspx?ID=25&amp;GUID=abc">Meeting</a></html>"#;
        let (calendar_digest, candidates) = seed_calendar(&store, &config, raw);
        let body = b"<!doctype html><html>Official meeting</html>".to_vec();
        let expected_digest = sha256_hex(&body);
        let mut fetcher = FakeFetcher {
            body: body.clone(),
            content_type: "text/html".to_owned(),
            ..FakeFetcher::default()
        };
        let (first, record_path) = archive_candidate_with(
            &store,
            &config,
            "test-calendar",
            &calendar_digest,
            candidates[0].anchor_index,
            "test-meeting-documents",
            &mut fetcher,
        )
        .expect("first candidate archive");
        let (second, second_path) = archive_candidate_with(
            &store,
            &config,
            "test-calendar",
            &calendar_digest,
            candidates[0].anchor_index,
            "test-meeting-documents",
            &mut fetcher,
        )
        .expect("conditional candidate archive");
        assert_eq!(first.digest, expected_digest);
        assert_eq!(second.digest, expected_digest);
        assert!(!first.unchanged);
        assert!(second.unchanged);
        assert_ne!(record_path, second_path);
        assert_eq!(
            fetcher.urls,
            vec![candidates[0].url.clone(), candidates[0].url.clone()]
        );
        assert_eq!(
            fetcher.reviewed_paths,
            vec![Some(vec!["/MeetingDetail.aspx".to_owned()]); 2]
        );
        assert!(fetcher.priors[0].is_none());
        assert_eq!(
            fetcher.priors[1]
                .as_ref()
                .map(|item| item.evidence_id.as_str()),
            Some(expected_digest.as_str())
        );
        assert_eq!(
            fs::read(store.content_path(&expected_digest).unwrap()).unwrap(),
            body
        );
        let private_record: serde_json::Value =
            serde_json::from_slice(&fs::read(&record_path).unwrap()).unwrap();
        assert_eq!(private_record["calendar_snapshot_sha256"], calendar_digest);
        assert_eq!(private_record["candidate_url"], candidates[0].url);
        assert_eq!(private_record["document_sha256"], expected_digest);
        assert_eq!(
            fs::metadata(&record_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn town_index_candidate_archives_reviewed_pdf_without_public_claim() {
        let config = town_index_config();
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        install_candidate_review(&store, &config, "test-index");
        install_candidate_review(&store, &config, "test-index-pdf");
        let raw = br#"<html><li>Board minutes<a href="/wp-content/uploads/2026/11/minutes.pdf">Download PDF</a></li></html>"#;
        let (index_digest, candidates) = seed_calendar(&store, &config, raw);
        let pdf = b"%PDF-1.7\nprivate test document\n".to_vec();
        let mut fetcher = FakeFetcher {
            body: pdf.clone(),
            content_type: "application/pdf".to_owned(),
            ..FakeFetcher::default()
        };
        let (outcome, record_path) = archive_candidate_with(
            &store,
            &config,
            "test-index",
            &index_digest,
            candidates[0].anchor_index,
            "test-index-pdf",
            &mut fetcher,
        )
        .unwrap();
        assert_eq!(outcome.digest, sha256_hex(&pdf));
        assert_eq!(outcome.status, 200);
        assert_eq!(fetcher.calls, 1);
        assert_eq!(fetcher.urls, vec![candidates[0].url.clone()]);
        assert_eq!(
            fetcher.reviewed_paths,
            vec![Some(vec![
                "/wp-content/uploads/2026/11/minutes.pdf".to_owned()
            ])]
        );
        let record = fs::read_to_string(record_path).unwrap();
        assert!(record.contains("\"candidate_kind\": \"document\""));
        assert!(record.contains("\"document_source_review_id\""));
        assert_eq!(
            fs::read(store.content_path(&outcome.digest).unwrap()).unwrap(),
            pdf
        );
    }

    #[test]
    fn candidate_rejects_tampered_artifact_wrong_kind_and_missing_path_review_before_fetch() {
        let config = candidate_config(0, 0);
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("store");
        install_candidate_review(&store, &config, "test-calendar");
        let raw = br#"<html><a href="MeetingDetail.aspx?ID=25">Meeting</a></html>"#;
        let (digest, candidates) = seed_calendar(&store, &config, raw);
        let mut fetcher = FakeFetcher {
            body: b"<html>meeting</html>".to_vec(),
            content_type: "text/html".to_owned(),
            ..FakeFetcher::default()
        };
        let err = archive_candidate_with(
            &store,
            &config,
            "test-calendar",
            &digest,
            candidates[0].anchor_index,
            "test-meeting-documents",
            &mut fetcher,
        )
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("no current persistent source review")
        );
        assert_eq!(fetcher.calls, 0);
        install_candidate_review(&store, &config, "test-meeting-documents");
        let err = archive_candidate_with(
            &store,
            &config,
            "test-calendar",
            &digest,
            candidates[0].anchor_index,
            "test-pdf-documents",
            &mut fetcher,
        )
        .unwrap_err();
        assert!(err.to_string().contains("candidate kind"));
        assert_eq!(fetcher.calls, 0);
        let path = dir
            .path()
            .join("discovery-candidates/test-calendar")
            .join(format!("{digest}.json"));
        let original = fs::read(&path).unwrap();
        let forged = String::from_utf8(original.clone()).unwrap().replace(
            "official.example.test/MeetingDetail",
            "evil.example.test/MeetingDetail",
        );
        fs::write(&path, forged).unwrap();
        let err = archive_candidate_with(
            &store,
            &config,
            "test-calendar",
            &digest,
            candidates[0].anchor_index,
            "test-meeting-documents",
            &mut fetcher,
        )
        .unwrap_err();
        assert!(err.to_string().contains("artifact differs"));
        assert_eq!(fetcher.calls, 0);
        fs::write(path, original).unwrap();
        let mut changed_config = config.clone();
        changed_config.sources[1].maximum_bytes += 1;
        let err = archive_candidate_with(
            &store,
            &changed_config,
            "test-calendar",
            &digest,
            candidates[0].anchor_index,
            "test-meeting-documents",
            &mut fetcher,
        )
        .unwrap_err();
        assert!(err.to_string().contains("configuration changed"));
        assert_eq!(fetcher.calls, 0);
    }

    #[test]
    fn candidate_pdf_rate_limit_applies_across_query_documents() {
        let config = candidate_config(0, 120);
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("store");
        install_candidate_review(&store, &config, "test-calendar");
        install_candidate_review(&store, &config, "test-pdf-documents");
        let raw = br#"<html><a href="View.ashx?M=M&amp;ID=25">Minutes</a></html>"#;
        let (digest, candidates) = seed_calendar(&store, &config, raw);
        let mut first = FakeFetcher {
            body: b"%PDF-1.4\nfirst\n".to_vec(),
            content_type: "application/pdf".to_owned(),
            ..FakeFetcher::default()
        };
        let (archived, _) = archive_candidate_with(
            &store,
            &config,
            "test-calendar",
            &digest,
            candidates[0].anchor_index,
            "test-pdf-documents",
            &mut first,
        )
        .unwrap();
        assert_eq!(archived.changed_from, None);
        let mut second = FakeFetcher {
            body: b"%PDF-1.4\nsecond\n".to_vec(),
            content_type: "application/pdf".to_owned(),
            ..FakeFetcher::default()
        };
        let err = archive_candidate_with(
            &store,
            &config,
            "test-calendar",
            &digest,
            candidates[0].anchor_index,
            "test-pdf-documents",
            &mut second,
        )
        .unwrap_err();
        assert!(err.to_string().contains("rate limit"));
        assert_eq!(second.calls, 0);
    }

    #[test]
    fn candidate_pdf_redirect_fails_and_revisions_preserve_both() {
        let no_interval = candidate_config(0, 0);
        let revision_dir = tempdir().expect("revision tempdir");
        let revision_store = Store::open(revision_dir.path()).expect("revision store");
        install_candidate_review(&revision_store, &no_interval, "test-calendar");
        install_candidate_review(&revision_store, &no_interval, "test-pdf-documents");
        let raw = br#"<html><a href="View.ashx?M=M&amp;ID=25">Minutes</a></html>"#;
        let (revision_calendar, revision_candidates) =
            seed_calendar(&revision_store, &no_interval, raw);
        let mut first_revision = FakeFetcher {
            body: b"%PDF-1.4\nfirst\n".to_vec(),
            content_type: "application/pdf".to_owned(),
            ..FakeFetcher::default()
        };
        let (original, _) = archive_candidate_with(
            &revision_store,
            &no_interval,
            "test-calendar",
            &revision_calendar,
            revision_candidates[0].anchor_index,
            "test-pdf-documents",
            &mut first_revision,
        )
        .unwrap();
        let mut second = FakeFetcher {
            body: b"%PDF-1.4\nsecond\n".to_vec(),
            content_type: "application/pdf".to_owned(),
            ..FakeFetcher::default()
        };
        let (changed, record_path) = archive_candidate_with(
            &revision_store,
            &no_interval,
            "test-calendar",
            &revision_calendar,
            revision_candidates[0].anchor_index,
            "test-pdf-documents",
            &mut second,
        )
        .unwrap();
        assert_eq!(changed.changed_from, Some(original.digest.clone()));
        assert_ne!(changed.digest, original.digest);
        assert!(
            revision_store
                .content_path(&original.digest)
                .unwrap()
                .exists()
        );
        assert!(
            revision_store
                .content_path(&changed.digest)
                .unwrap()
                .exists()
        );
        let record: serde_json::Value =
            serde_json::from_slice(&fs::read(record_path).unwrap()).unwrap();
        assert_eq!(record["changed_from_sha256"], original.digest);
        let mut redirect = FakeFetcher {
            body: b"%PDF-1.4\nthird\n".to_vec(),
            content_type: "application/pdf".to_owned(),
            redirect_to: Some("https://official.example.test/View.ashx?M=M&ID=26".to_owned()),
            ..FakeFetcher::default()
        };
        let err = archive_candidate_with(
            &revision_store,
            &no_interval,
            "test-calendar",
            &revision_calendar,
            revision_candidates[0].anchor_index,
            "test-pdf-documents",
            &mut redirect,
        )
        .unwrap_err();
        assert!(err.to_string().contains("provenance"));
        assert_eq!(redirect.calls, 1);
    }
}
