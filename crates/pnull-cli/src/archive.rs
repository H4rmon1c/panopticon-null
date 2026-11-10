//! Archive a configured official HTML page, PDF, or JSON record as private bytes.
//!
//! This deliberately stops before extraction, claim creation, or publication.
//! A source must have a current review bound to a preserved robots response,
//! its exact configuration, host, and path. The shared fetcher enforces DNS and
//! response-size bounds and retains conditional-fetch provenance.

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::net::IpAddr;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Component;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use pnull_core::{FetchObservation, Store, sha256_hex};
use pnull_http::{FetchConfig, PriorEvidence};
use pnull_ingest::fetch_source;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::{SourceConfig, StateConfig, source_config_digest, verify_source_review};

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
        _ => bail!("source is not a reviewed calendar document path policy"),
    };
    if (expected_kind == "meeting" && candidate.kind != "meeting")
        || (expected_kind == "document"
            && !matches!(candidate.kind.as_str(), "agenda" | "packet" | "minutes"))
    {
        bail!("candidate kind does not match the reviewed document path policy");
    }
    let policy_url = url::Url::parse(&policy.url)?;
    let candidate_url = url::Url::parse(&candidate.url)?;
    if policy_url.scheme() != "https"
        || policy_url.query().is_some()
        || policy_url.fragment().is_some()
        || policy_url.path() != path
        || candidate_url.scheme() != "https"
        || candidate_url.host_str() != policy_url.host_str()
        || candidate_url.port_or_known_default() != Some(443)
        || candidate_url.path() != path
        || candidate_url.query().is_none()
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
    if calendar.source_type != "html_page"
        || calendar_url.path() != "/Calendar.aspx"
        || calendar_url.query().is_some()
    {
        bail!("candidate origin is not a configured official HTML calendar");
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
        candidates: parse_calendar_candidates(&calendar_url, &raw)?,
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
    expected
        .candidates
        .into_iter()
        .find(|candidate| candidate.anchor_index == anchor_index)
        .context("anchor index is absent from archived calendar")
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
        "candidate_document_pdf" => "pdf",
        ordinary => ordinary,
    };
    if expected_type != document_type {
        bail!("archive document type differs from reviewed source policy: {source_id}");
    }
    let reviewed_url = url::Url::parse(&source.url)?;
    let requested_url = url::Url::parse(url)?;
    if reviewed_url.scheme() != "https"
        || reviewed_url.port_or_known_default() != Some(443)
        || !reviewed_url.username().is_empty()
        || reviewed_url.password().is_some()
        || reviewed_url.host_str() != requested_url.host_str()
        || reviewed_url.port_or_known_default() != requested_url.port_or_known_default()
        || reviewed_url.path() != requested_url.path()
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
    for observation in store.fetch_observations(source_id)?.into_iter().rev() {
        if observation.requested_url != url
            || observation.final_url != url
            || observation.status_code != 200
            || observation.method != "GET"
            || observation.error.is_some()
            || observation.redirect_target.is_some()
            || !receipt_has_public_network(&observation, url)
        {
            continue;
        }
        let Some(digest) = observation.body_digest else {
            continue;
        };
        let Ok(path) = store.content_path(&digest) else {
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
            || sha256_hex(&raw) != digest
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
        return Ok(Some(PriorEvidence {
            evidence_id: digest,
            etag: observation.etag,
            last_modified: observation.last_modified,
        }));
    }
    Ok(None)
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

    fn install_candidate_review(store: &Store, config: &StateConfig, source_id: &str) {
        let source = config
            .sources
            .iter()
            .find(|item| item.id == source_id)
            .expect("source");
        let captured_at = OffsetDateTime::now_utc().format(&Rfc3339).expect("time");
        let robots_url = "https://official.example.test/robots.txt";
        let robots = b"User-agent: *\nAllow: /\n";
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
        let candidates = parse_calendar_candidates(&url::Url::parse(&calendar.url).unwrap(), raw)
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
