//! Archive a configured official HTML page, PDF, or JSON record as private bytes.
//!
//! This deliberately stops before extraction, claim creation, or publication.
//! A source must have a current review bound to a preserved robots response,
//! its exact configuration, host, and path. The shared fetcher enforces DNS and
//! response-size bounds and retains conditional-fetch provenance.

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use pnull_core::{FetchObservation, Store, sha256_hex};
use pnull_http::{FetchConfig, PriorEvidence};
use pnull_ingest::fetch_source;
use scraper::{Html, Selector};
use serde::Serialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::{StateConfig, latest_prior, verify_source_review};

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

#[derive(Debug, Serialize)]
struct CalendarCandidate {
    kind: &'static str,
    url: String,
    /// Zero-based position of this anchor among all anchors in the exact HTML snapshot.
    anchor_index: usize,
    label: String,
}

#[derive(Debug, Serialize)]
struct CalendarCandidates<'a> {
    schema_version: u8,
    source_id: &'a str,
    source_url: &'a str,
    snapshot_sha256: &'a str,
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
    let raw = fs::read(store.content_path(&outcome.digest)?)?;
    if sha256_hex(&raw) != outcome.digest {
        bail!("archived calendar bytes do not match their digest");
    }
    let candidates = parse_calendar_candidates(&source_url, &raw)?;
    let manifest = CalendarCandidates {
        schema_version: 1,
        source_id,
        source_url: &source.url,
        snapshot_sha256: &outcome.digest,
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

fn persist_calendar_candidates(
    data_dir: &Path,
    manifest: &CalendarCandidates<'_>,
) -> Result<PathBuf> {
    let bytes = serde_json::to_vec_pretty(manifest)?;
    let directory = data_dir
        .join("discovery-candidates")
        .join(manifest.source_id);
    fs::create_dir_all(&directory)?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
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
            if fs::read(&path)? != bytes {
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
            let media = url
                .query_pairs()
                .find_map(|(key, value)| key.eq_ignore_ascii_case("M").then(|| value.into_owned()));
            match media.as_deref().map(str::to_ascii_uppercase).as_deref() {
                Some("A" | "AADA" | "AP") => "agenda",
                Some("PA") => "packet",
                Some("M" | "MADA" | "MP") => "minutes",
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
            kind,
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
    if source.maximum_bytes == 0 {
        bail!("source archive requires a positive maximum_bytes for {source_id}");
    }

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

    let prior = latest_prior(store, source_id, &source.url)?;
    let fetch_config = FetchConfig {
        reviewed_hosts: review.reviewed_hosts,
        reviewed_paths: Some(review.endpoint_patterns),
        max_bytes: source.maximum_bytes,
    };
    let (body, observations) = fetcher
        .fetch(
            store,
            source_id,
            &fetch_config,
            &source.url,
            &retrieved_at,
            prior.as_ref(),
        )
        .with_context(|| format!("archive fetch failed for {source_id}"))?;
    let first = observations
        .first()
        .ok_or_else(|| anyhow::anyhow!("archive fetch returned no provenance for {source_id}"))?;
    let last = observations.last().expect("nonempty observations");
    if first.requested_url != source.url
        || observations.iter().any(|item| {
            item.source_id.as_deref() != Some(source_id)
                || item.retrieved_at != retrieved_at
                || item.method != "GET"
                || item.error.is_some()
        })
    {
        bail!("archive fetch provenance does not match the reviewed source {source_id}");
    }
    let requested_url = url::Url::parse(&source.url)?;
    let final_url = url::Url::parse(&last.final_url)?;
    if final_url != requested_url {
        bail!("archive fetch redirected away from the exact configured document {source_id}");
    }
    let unchanged = last.status_code == 304;
    if !unchanged && last.status_code != 200 {
        bail!(
            "archive fetch returned unexpected HTTP {} for {source_id}",
            last.status_code
        );
    }
    if body.is_empty() || body.len() > source.maximum_bytes {
        bail!("archive fetch returned empty or oversized content for {source_id}");
    }
    validate_document(
        &source.source_type,
        &body,
        last.content_type.as_deref(),
        unchanged,
    )?;

    let digest = sha256_hex(&body);
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
    fn calendar_candidate_artifact_is_private_and_immutable() {
        let temporary = tempdir().unwrap();
        let digest = sha256_hex(b"calendar bytes");
        let manifest = CalendarCandidates {
            schema_version: 1,
            source_id: "test-calendar",
            source_url: "https://council.example.test/Calendar.aspx",
            snapshot_sha256: &digest,
            candidates: vec![CalendarCandidate {
                kind: "meeting",
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
            resolved_ips: vec!["203.0.113.10".to_owned()],
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

    #[derive(Default)]
    struct FakeFetcher {
        calls: usize,
        priors: Vec<Option<PriorEvidence>>,
        body: Vec<u8>,
        content_type: String,
    }

    impl ArchiveFetcher for FakeFetcher {
        fn fetch(
            &mut self,
            _store: &Store,
            source_id: &str,
            _config: &FetchConfig,
            url: &str,
            retrieved_at: &str,
            prior: Option<&PriorEvidence>,
        ) -> Result<(Vec<u8>, Vec<FetchObservation>)> {
            self.calls += 1;
            self.priors.push(prior.cloned());
            let unchanged = self.calls > 1;
            let status = if unchanged { 304 } else { 200 };
            let observation = FetchObservation {
                id: FetchObservation::id_for(url, retrieved_at, status),
                source_id: Some(source_id.to_owned()),
                requested_url: url.to_owned(),
                resolved_ips: vec!["203.0.113.10".to_owned()],
                retrieved_at: retrieved_at.to_owned(),
                method: "GET".to_owned(),
                status_code: status,
                redirect_target: None,
                final_url: url.to_owned(),
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
}
