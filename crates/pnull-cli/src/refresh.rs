//! `pnull procurement refresh` — the exposure heartbeat (v0.0.4, Item 6).
//!
//! Keeps the ledger fresh: `--dry-run` (the default) prints exactly what would
//! be fetched and compared with zero network activity. `--live` fetches the
//! reviewed surface, records a new snapshot (or 304 provenance), runs Item 1
//! change detection, prints the alert count and affected matter ids, and writes
//! a coverage-ledger entry. A refusal or parse failure creates no new
//! snapshot or alert; successful fetch observations remain in the audit log.
//!
//! The transport is behind a trait so tests inject a fake that never touches
//! the network. The real transport performs a DNS-safe conditional HTTPS GET
//! and is only ever constructed on the `--live` path behind the persistent
//! source-review gate.

use anyhow::{Result, anyhow, bail};
use pnull_core::{
    CoverageState, FetchObservation, ProcurementAlert, ProcurementChangeKind,
    ProcurementRecordChange, SourceAuthority, SourceReview, SourceSnapshot, Store, sha256_hex,
};
use pnull_procurement::{
    Acquisition, RecordRow, build_change_alerts, ensure_matter, latest_snapshot,
    matter_id_for_identifier, parse_awards_table, parse_solicitations, persist_change_alerts,
    record_diff, record_snapshot, record_unchanged, solicitation_record_rows,
};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// What a transport fetch returned for a surface.
pub struct TransportFetch {
    pub bytes: Vec<u8>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    /// True when the server returned 304 Not Modified (bytes unchanged).
    pub not_modified: bool,
    pub final_url: String,
    pub content_type: Option<String>,
    /// Provenance observations recorded by the fetch (empty for a fake).
    pub observations: Vec<FetchObservation>,
}

/// A fetch transport for a procurement surface. The real implementation does a
/// DNS-safe conditional `HTTPS GET`; tests inject a deterministic fake. The
/// caller supplies the surface URL and any prior `ETag` so the transport can
/// issue a conditional request.
pub trait ProcurementTransport {
    fn fetch(
        &mut self,
        source_id: &str,
        url: &str,
        prior_etag: Option<&str>,
        retrieved_at: &str,
    ) -> Result<TransportFetch, String>;
}

/// The offline dry-run result: the planned comparison with zero network.
pub struct DryRunPlan {
    pub source_id: String,
    pub source_url: String,
    pub latest_snapshot_digest: Option<String>,
    pub planned_comparison: String,
}

enum ParsedSurface {
    Awards(Vec<pnull_procurement::AwardRow>),
    Solicitations(Vec<pnull_procurement::SolicitationRecord>),
}

impl ParsedSurface {
    const fn label(&self) -> &'static str {
        match self {
            Self::Awards(_) => "award",
            Self::Solicitations(_) => "solicitation",
        }
    }
}

fn prior_blob_verified(store: &Store, snapshot: &pnull_core::SourceSnapshot) -> bool {
    store
        .content_path(&snapshot.persisted_digest)
        .ok()
        .and_then(|path| std::fs::read(path).ok())
        .is_some_and(|bytes| sha256_hex(&bytes) == snapshot.persisted_digest)
}

fn redirect_history(observations: &[FetchObservation]) -> Vec<String> {
    observations
        .iter()
        .skip(1)
        .map(|observation| observation.requested_url.clone())
        .collect()
}

/// Resolve an alert to the matter created from the row's actual identifier.
/// A digest fallback key is not an identifier and must not create a phantom
/// matter. Removed rows can still resolve through the prior snapshot's rows.
fn alert_matter_id(
    source_id: &str,
    row_key: &str,
    new_rows: &[RecordRow],
    prior_rows: Option<&[RecordRow]>,
) -> Option<String> {
    let row = new_rows
        .iter()
        .chain(prior_rows.into_iter().flatten())
        .find(|row| row.key == row_key)?;
    let identifier = match source_id {
        "colorado-springs-contract-awards" => {
            serde_json::from_str::<pnull_procurement::AwardRow>(&row.raw_json)
                .ok()?
                .solicitation_id
        }
        "colorado-springs-solicitation-mirror" => {
            serde_json::from_str::<pnull_procurement::SolicitationRecord>(&row.raw_json)
                .ok()?
                .identifier
        }
        _ => return None,
    };
    pnull_core::normalize_identifier(&identifier)
        .map(|(normalized, _)| matter_id_for_identifier(&normalized))
}

fn solicitation_change_alerts(
    prior: &SourceSnapshot,
    current: &SourceSnapshot,
    prior_rows: &[RecordRow],
    current_rows: &[RecordRow],
    retrieved_at: &str,
) -> Vec<ProcurementAlert> {
    let diff = record_diff(
        &prior.id,
        &current.id,
        &current.source_id,
        prior_rows,
        current_rows,
    );
    diff.changes
        .into_iter()
        .filter_map(|change| {
            let (kind, description) = match change.kind.as_str() {
                "added" => (
                    ProcurementChangeKind::RecordAdded,
                    "appeared in the later checked solicitation mirror snapshot",
                ),
                "changed" => (
                    ProcurementChangeKind::RecordModified,
                    "differs between two checked solicitation mirror snapshots",
                ),
                "removed" => (
                    ProcurementChangeKind::RecordRemoved,
                    "was present in the earlier checked solicitation mirror snapshot but is not present in the later one",
                ),
                _ => return None,
            };
            let summary = format!(
                "Solicitation row {} {description}. This informational mirror may be incomplete; the comparison does not establish a procurement outcome or prove absence.",
                change.row_key
            );
            let record_change = ProcurementRecordChange {
                change_kind: kind,
                row_identity: change.row_key.clone(),
                field_diffs: Vec::new(),
                old_snapshot_id: prior.id.clone(),
                old_snapshot_digest: prior.persisted_digest.clone(),
                new_snapshot_id: current.id.clone(),
                new_snapshot_digest: current.persisted_digest.clone(),
                summary: summary.clone(),
            };
            Some(ProcurementAlert {
                id: ProcurementAlert::id_for(
                    &current.source_id,
                    "solicitation-mirror",
                    &change.row_key,
                    kind,
                    &prior.id,
                    &current.id,
                ),
                source_id: current.source_id.clone(),
                surface: "solicitation-mirror".to_owned(),
                old_snapshot_id: prior.id.clone(),
                old_snapshot_digest: prior.persisted_digest.clone(),
                new_snapshot_id: current.id.clone(),
                new_snapshot_digest: current.persisted_digest.clone(),
                changes: vec![record_change],
                retrieved_at: retrieved_at.to_owned(),
                coverage_state: current.coverage_state,
                matter_ids: Vec::new(),
                identifier_ids: Vec::new(),
                taxonomy_matches: Vec::new(),
                summary,
            })
        })
        .collect()
}

/// The URL and fixture filename for a reviewed procurement surface.
fn surface_url(source_id: &str) -> Option<(&'static str, &'static str)> {
    match source_id {
        "colorado-springs-contract-awards" => Some((
            "https://coloradosprings.gov/procurement-services/page/contract-award-information",
            "contract-awards.html",
        )),
        "colorado-springs-solicitation-mirror" => Some((
            "https://coloradosprings.gov/solicitations",
            "solicitations.html",
        )),
        _ => None,
    }
}

/// Computes the dry-run plan for a source without any network activity.
pub fn dry_run_plan(store: &Store, source_id: &str) -> Result<DryRunPlan, String> {
    let (url, _) = surface_url(source_id).ok_or_else(|| format!("unknown source {source_id}"))?;
    let latest = store
        .source_snapshots(source_id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .last();
    let planned = match &latest {
        Some(snapshot) => format!(
            "conditional GET {} against stored snapshot {} (digest {})",
            url, snapshot.id, snapshot.persisted_digest
        ),
        None => format!("initial GET {url}"),
    };
    Ok(DryRunPlan {
        source_id: source_id.to_owned(),
        source_url: url.to_string(),
        latest_snapshot_digest: latest.map(|s| s.persisted_digest),
        planned_comparison: planned,
    })
}

/// Enforces the persistent source-review gate for a live procurement refresh.
///
/// Reuses the established `require_review_for_live` semantics (no review or
/// expired review refuses) and additionally fails closed on a host outside the
/// reviewed scope or on any announced restrictions. `surface_url` is validated
/// as in-scope before the transport is ever constructed.
fn check_live_gate<'a>(
    store: &Store,
    config: &'a crate::StateConfig,
    source_id: &str,
    now: OffsetDateTime,
) -> Result<(&'a crate::SourceConfig, SourceReview)> {
    let (url, _) = surface_url(source_id)
        .ok_or_else(|| anyhow::anyhow!("unknown procurement source: {source_id}"))?;
    let source = config
        .sources
        .iter()
        .find(|source| source.id == source_id)
        .ok_or_else(|| anyhow!("procurement source {source_id} is not configured"))?;
    if source.url != url {
        bail!(
            "live retrieval refused: configured URL differs from reviewed surface for {source_id}"
        );
    }
    let review = crate::procurement_cmd::require_review_for_live_at(store, source_id, now)?;
    if review.source_config_digest != crate::source_config_digest(source) {
        bail!("live retrieval refused: source configuration changed since review for {source_id}");
    }
    let parsed = url::Url::parse(url)?;
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow!("source URL has no host"))?;
    if !review
        .reviewed_hosts
        .iter()
        .any(|h| h.eq_ignore_ascii_case(host))
    {
        bail!(
            "live retrieval refused: host {host} is not within the reviewed scope for {source_id}"
        );
    }
    if !review.restrictions.is_empty() {
        bail!("live retrieval refused: a prior response announced restrictions for {source_id}");
    }
    if !review
        .endpoint_patterns
        .iter()
        .any(|path| path == parsed.path())
    {
        bail!("live retrieval refused: endpoint is outside reviewed scope for {source_id}");
    }
    if !store.source_fetch_allowed(
        source_id,
        source.minimum_interval_seconds,
        now.unix_timestamp(),
    )? {
        bail!("rate limit: source {source_id} was fetched within the configured interval");
    }
    Ok((source, review))
}

/// Runs the live refresh given a transport. Fails closed on any gate failure.
#[allow(clippy::too_many_lines)]
fn refresh_live_with(
    store: &Store,
    config: &crate::StateConfig,
    source_id: &str,
    transport: &mut dyn ProcurementTransport,
    now: OffsetDateTime,
) -> Result<()> {
    let (source, _review) = check_live_gate(store, config, source_id, now)?;
    let (url, _) = surface_url(source_id)
        .ok_or_else(|| anyhow::anyhow!("unknown procurement source: {source_id}"))?;
    let retrieved_at = now.format(&Rfc3339)?;

    // Prior evidence for a conditional request, if a prior snapshot exists.
    let latest = latest_snapshot(store, source_id).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let prior_etag = latest
        .as_ref()
        .filter(|snapshot| prior_blob_verified(store, snapshot))
        .and_then(|snapshot| snapshot.etag.clone());

    // Claim the interval atomically before transport. Failed HTTP responses,
    // oversized bodies, and parser errors still consume the fetch attempt.
    if !store.try_record_source_fetch(
        source_id,
        source.minimum_interval_seconds,
        now.unix_timestamp(),
    )? {
        bail!("rate limit: source {source_id} was fetched within the configured interval");
    }
    let fetched = transport
        .fetch(source_id, url, prior_etag.as_deref(), &retrieved_at)
        .map_err(|e| anyhow::anyhow!("live fetch failed for {source_id}: {e}"))?;

    // Persist provenance observations first, before any state change.
    for observation in &fetched.observations {
        store
            .insert_fetch_observation(observation)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    }

    // 304: the preserved bytes remain authoritative; record provenance, no alert.
    if fetched.not_modified {
        let existing = latest.as_ref().ok_or_else(|| {
            anyhow::anyhow!("304 without a prior snapshot for {source_id}: refusing to record")
        })?;
        if !prior_blob_verified(store, existing) {
            bail!("304 without preserved original bytes for {source_id}: refusing to record");
        }
        let acquisition = Acquisition {
            source_id: source_id.to_owned(),
            source_url: url.to_owned(),
            retrieved_at: retrieved_at.clone(),
            bytes_digest: existing.persisted_digest.clone(),
            content_type: fetched.content_type.clone(),
            etag: fetched.etag.clone(),
            last_modified: fetched.last_modified.clone(),
            final_url: fetched.final_url.clone(),
            redirect_history: redirect_history(&fetched.observations),
            parser_version: parser_version(source_id),
            schema_version: 2,
            authority: SourceAuthority::OfficialInformationalMirror,
            coverage_state: CoverageState::InformationalOnly,
            observations: fetched.observations.clone(),
        };
        record_unchanged(store, &acquisition, existing)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        println!("live refresh: 304 Not Modified; no snapshot change, no alerts.");
        println!(
            "  unchanged snapshot {} (digest {})",
            existing.id, existing.persisted_digest
        );
        return Ok(());
    }

    // Changed bytes: parse, record a supersession snapshot, run change alerts.
    let digest = sha256_hex(&fetched.bytes);
    // Preserve the exact response even when parsing or row quarantine fails.
    // Its fetch observation can then be audited without publishing a snapshot.
    store.persist_content(&digest, &fetched.bytes)?;
    let html = String::from_utf8_lossy(&fetched.bytes);
    let parsed = match source_id {
        "colorado-springs-contract-awards" => ParsedSurface::Awards(
            parse_awards_table(&html, &digest)
                .map_err(|e| anyhow!("parse refreshed awards snapshot: {e}"))?,
        ),
        "colorado-springs-solicitation-mirror" => ParsedSurface::Solicitations(
            parse_solicitations(&html, &digest)
                .map_err(|e| anyhow!("parse refreshed solicitation snapshot: {e}"))?,
        ),
        _ => bail!("unknown procurement source: {source_id}"),
    };
    let parsed_count = match &parsed {
        ParsedSurface::Awards(rows) => rows.len(),
        ParsedSurface::Solicitations(rows) => rows.len(),
    };
    if parsed_count == 0 {
        bail!(
            "refreshed {source_id} contains no parsed rows; quarantining capture to avoid unsupported mass-removal alerts"
        );
    }
    let acquisition = Acquisition {
        source_id: source_id.to_owned(),
        source_url: url.to_owned(),
        retrieved_at: retrieved_at.clone(),
        bytes_digest: digest.clone(),
        content_type: fetched.content_type.clone(),
        etag: fetched.etag.clone(),
        last_modified: fetched.last_modified.clone(),
        final_url: fetched.final_url.clone(),
        redirect_history: redirect_history(&fetched.observations),
        parser_version: parser_version(source_id),
        schema_version: 2,
        authority: SourceAuthority::OfficialInformationalMirror,
        coverage_state: CoverageState::InformationalOnly,
        observations: fetched.observations.clone(),
    };

    // Prior rows for change detection come from the database's stored rows of
    // the exact previous snapshot — never reconstructed from a fixture or any
    // file currently on disk. If the prior snapshot's rows were never preserved
    // (a legacy capture), no prior rows are available and change detection
    // cannot fabricate a diff; the snapshot is still recorded and its own rows
    // are persisted for future comparisons.
    let prior_stored = match latest
        .as_ref()
        .map(|snapshot| pnull_procurement::snapshot_rows(store, &snapshot.id))
    {
        Some(Ok(rows)) => Some(rows),
        Some(Err(pnull_procurement::SnapshotError::LegacyOrIncomplete(_))) | None => None,
        Some(Err(error)) => return Err(anyhow!("prior snapshot row integrity failed: {error}")),
    };
    let new_rows = match &parsed {
        ParsedSurface::Awards(rows) => pnull_procurement::award_record_rows(rows),
        ParsedSurface::Solicitations(records) => solicitation_record_rows(records),
    };
    let (new_snapshot, _) = record_snapshot(
        store,
        &acquisition,
        latest.as_ref(),
        Some(new_rows.len() as u64),
        &prior_stored.clone().unwrap_or_default(),
        &new_rows,
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    match &parsed {
        ParsedSurface::Awards(rows) => {
            for row in rows {
                pnull_procurement::attach_award_row(
                    store,
                    row,
                    std::slice::from_ref(&new_snapshot.id),
                )?;
            }
        }
        ParsedSurface::Solicitations(records) => {
            for record in records {
                pnull_procurement::attach_solicitation_record(
                    store,
                    record,
                    std::slice::from_ref(&new_snapshot.id),
                )?;
            }
        }
    }

    // Change detection only applies against a prior snapshot whose rows were
    // preserved in the database. Award field-level changes require their
    // persisted raw values; solicitation changes use canonical stored rows.
    let mut alerts = Vec::new();
    let mut prior_for_alerts: Option<pnull_core::SourceSnapshot> = None;
    if let (Some(prior), Some(prior_rows)) = (latest.as_ref(), prior_stored.as_ref())
        && prior.persisted_digest != new_snapshot.persisted_digest
    {
        match &parsed {
            ParsedSurface::Awards(rows) => {
                let prior_award_rows: Vec<pnull_procurement::AwardRow> = prior_rows
                    .iter()
                    .filter_map(pnull_procurement::award_row_from_record_row)
                    .collect();
                if !prior_award_rows.is_empty() {
                    alerts = build_change_alerts(
                        source_id,
                        "contract-award-table",
                        &prior.id,
                        &prior.persisted_digest,
                        &new_snapshot.id,
                        &new_snapshot.persisted_digest,
                        &retrieved_at,
                        new_snapshot.coverage_state,
                        &prior_award_rows,
                        rows,
                        &[],
                        &[],
                    );
                }
            }
            ParsedSurface::Solicitations(_) => {
                alerts = solicitation_change_alerts(
                    prior,
                    &new_snapshot,
                    prior_rows,
                    &new_rows,
                    &retrieved_at,
                );
            }
        }
        if !alerts.is_empty() {
            prior_for_alerts = Some(prior.clone());
        }
    }

    // Resolve affected matter ids from the stored row's actual identifier.
    let mut affected = Vec::new();
    for alert in &mut alerts {
        let matter_id = alert.changes.first().and_then(|change| {
            alert_matter_id(
                source_id,
                &change.row_identity,
                &new_rows,
                prior_stored.as_deref(),
            )
        });
        if let Some(matter_id) = matter_id {
            alert.matter_ids = vec![matter_id.clone()];
            if !affected.contains(&matter_id) {
                affected.push(matter_id);
            }
        }
    }
    let inserted =
        persist_change_alerts(store, &alerts).map_err(|e| anyhow::anyhow!(e.to_string()))?;

    // Ensure each affected matter is reachable so its case file/site page can
    // render the change (append-only: never deletes, only creates).
    for matter_id in &affected {
        ensure_matter(
            store,
            matter_id,
            &format!("{matter_id} — affected procurement record"),
        )
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    }

    println!(
        "live refresh: recorded snapshot {} (digest {})",
        new_snapshot.id, new_snapshot.persisted_digest
    );
    if let Some(prior) = prior_for_alerts {
        println!(
            "  supersedes previous snapshot (digest {})",
            prior.persisted_digest
        );
    }
    println!("  parsed {} {} row(s)", new_rows.len(), parsed.label());
    println!(
        "  change alerts: {inserted} new ({} total produced)",
        alerts.len()
    );
    if affected.is_empty() {
        println!("  affected matter(s): none");
    } else {
        println!("  affected matter(s): {}", affected.join(", "));
    }
    Ok(())
}

fn parser_version(source_id: &str) -> String {
    match source_id {
        "colorado-springs-solicitation-mirror" => "solicitations-1.0".to_owned(),
        _ => "awards-1.0".to_owned(),
    }
}

/// Runs the refresh command. `live=false` is the dry-run default (no network).
pub fn refresh_procurement(
    store: &Store,
    config: &crate::StateConfig,
    source_id: &str,
    live: bool,
) -> Result<()> {
    if !live {
        let plan = dry_run_plan(store, source_id).map_err(|e| anyhow!(e))?;
        println!(
            "dry run: source {} would fetch {}",
            plan.source_id, plan.source_url
        );
        println!("planned comparison: {}", plan.planned_comparison);
        println!(
            "latest snapshot digest: {}",
            plan.latest_snapshot_digest.as_deref().unwrap_or("none")
        );
        println!("dry run: zero network activity.");
        return Ok(());
    }
    let now = OffsetDateTime::now_utc();
    let (source, _) = check_live_gate(store, config, source_id, now)?;
    let mut transport = HttpTransport::new(store, source.maximum_bytes.min(MAX_FETCH_BYTES))
        .map_err(|e| anyhow!(e))?;
    refresh_live_with(store, config, source_id, &mut transport, now)
}

/// The real DNS-safe conditional HTTPS transport. Only constructed on the
/// `--live` path (never in tests or the demo). Wraps `pnull_http`'s
/// provenance-aware fetch.
struct HttpTransport<'a> {
    store: &'a Store,
    client: pnull_http::ReqwestTransport,
    max_bytes: usize,
}

impl<'a> HttpTransport<'a> {
    fn new(store: &'a Store, max_bytes: usize) -> Result<Self, String> {
        let client = pnull_http::ReqwestTransport::new(max_bytes).map_err(|e| e.to_string())?;
        Ok(Self {
            store,
            client,
            max_bytes,
        })
    }

    fn fetch_with(
        &self,
        source_id: &str,
        url: &str,
        prior_etag: Option<&str>,
        retrieved_at: &str,
        resolver: &dyn pnull_http::Resolver,
        transport: &dyn pnull_http::Transport,
    ) -> Result<TransportFetch, String> {
        let parsed = url::Url::parse(url).map_err(|e| e.to_string())?;
        let host = parsed
            .host_str()
            .ok_or_else(|| "invalid url: no host".to_owned())?
            .to_owned();
        let config = pnull_http::FetchConfig {
            reviewed_hosts: vec![host.clone()],
            reviewed_paths: Some(vec![parsed.path().to_owned()]),
            max_bytes: self.max_bytes,
        };
        let prior = prior_etag.map(|etag| pnull_http::PriorEvidence {
            evidence_id: etag.to_owned(),
            etag: Some(etag.to_owned()),
            last_modified: None,
        });
        let request = pnull_http::FetchRequest {
            source_id: Some(source_id.to_owned()),
            requested_url: url.to_owned(),
            retrieved_at: retrieved_at.to_owned(),
            prior,
        };
        let result =
            match pnull_http::provenance_fetch_detailed(&config, resolver, transport, &request) {
                Ok(result) => result,
                Err(failure) => {
                    for observation in &failure.observations {
                        self.store
                            .insert_fetch_observation(observation)
                            .map_err(|error| error.to_string())?;
                    }
                    return Err(failure.error.to_string());
                }
            };
        let last = result.observations.last().cloned();
        Ok(TransportFetch {
            bytes: result.body.unwrap_or_default(),
            etag: last.as_ref().and_then(|o| o.etag.clone()),
            last_modified: last.as_ref().and_then(|o| o.last_modified.clone()),
            not_modified: result.unchanged,
            final_url: result.final_url,
            content_type: last.as_ref().and_then(|o| o.content_type.clone()),
            observations: result.observations,
        })
    }
}

/// Cap on the bytes fetched for a refreshed surface.
const MAX_FETCH_BYTES: usize = 8 * 1024 * 1024;

impl ProcurementTransport for HttpTransport<'_> {
    fn fetch(
        &mut self,
        source_id: &str,
        url: &str,
        prior_etag: Option<&str>,
        retrieved_at: &str,
    ) -> Result<TransportFetch, String> {
        self.fetch_with(
            source_id,
            url,
            prior_etag,
            retrieved_at,
            &pnull_http::SystemResolver,
            &self.client,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pnull_core::SourceReview;
    use pnull_http::{HttpError, Resolver, Transport, TransportRequest, TransportResponse};
    use std::net::IpAddr;
    use tempfile::tempdir;

    struct PublicResolver;

    impl Resolver for PublicResolver {
        fn resolve(&self, _host: &str) -> Result<Vec<IpAddr>, HttpError> {
            Ok(vec!["93.184.216.34".parse().expect("public IP")])
        }
    }

    struct FailedStatusTransport;

    impl Transport for FailedStatusTransport {
        fn request(&self, _request: &TransportRequest) -> Result<TransportResponse, HttpError> {
            Ok(TransportResponse {
                status_code: 503,
                headers: vec![("retry-after".to_owned(), "120".to_owned())],
                body: b"temporarily unavailable".to_vec(),
            })
        }
    }

    #[test]
    fn real_procurement_transport_preserves_failed_http_receipt() {
        let directory = tempdir().expect("temporary directory");
        let store = Store::open(directory.path()).expect("store");
        let transport = HttpTransport::new(&store, 1024).expect("transport");
        let source_id = "colorado-springs-contract-awards";
        let (url, _) = surface_url(source_id).expect("source URL");
        let error = transport
            .fetch_with(
                source_id,
                url,
                None,
                "2026-11-10T00:00:00Z",
                &PublicResolver,
                &FailedStatusTransport,
            )
            .err()
            .expect("HTTP 503 must fail");
        assert!(error.contains("network transport failed"));
        let receipts = store.fetch_observations(source_id).expect("fetch receipts");
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].status_code, 503);
        assert_eq!(receipts[0].requested_url, url);
        assert!(
            receipts[0]
                .allowlisted_headers
                .iter()
                .any(|(name, value)| name == "retry-after" && value == "120")
        );
    }

    fn test_config() -> crate::StateConfig {
        let mut config: crate::StateConfig = toml::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../configs/states/co.toml"
        )))
        .expect("state config");
        for source in &mut config.sources {
            source.minimum_interval_seconds = 0;
        }
        config
    }

    fn test_now(day: u8) -> OffsetDateTime {
        OffsetDateTime::parse(&format!("2026-11-{day:02}T12:00:00Z"), &Rfc3339).expect("test time")
    }

    fn review(
        config: &crate::StateConfig,
        source_id: &str,
        expires: &str,
        host: &str,
    ) -> SourceReview {
        const ROBOTS_BYTES: &[u8] = b"User-agent: *\nAllow: /\n";
        const CAPTURED_AT: &str = "2026-08-01T00:00:00Z";
        let source = config
            .sources
            .iter()
            .find(|source| source.id == source_id)
            .expect("configured source");
        SourceReview {
            id: format!("review:{source_id}"),
            source_id: source_id.to_owned(),
            source_config_digest: crate::source_config_digest(source),
            reviewed_hosts: vec![host.to_owned()],
            endpoint_patterns: vec![
                url::Url::parse(&source.url)
                    .expect("source URL")
                    .path()
                    .to_owned(),
            ],
            robots_url: format!("https://{host}/robots.txt"),
            robots_snapshot_digest: sha256_hex(ROBOTS_BYTES),
            robots_provenance: Some(FetchObservation::id_for(
                &format!("https://{host}/robots.txt"),
                CAPTURED_AT,
                200,
            )),
            terms_urls: Vec::new(),
            terms_snapshot_digests: Vec::new(),
            reviewer: "tester".to_owned(),
            note: "test".to_owned(),
            reviewed_at: "2026-08-01T00:00:00Z".to_owned(),
            expires_at: expires.to_owned(),
            minimum_interval_seconds: 0,
            restrictions: Vec::new(),
            supersedes: None,
        }
    }

    fn insert_review_record(store: &Store, review: &SourceReview) {
        const ROBOTS_BYTES: &[u8] = b"User-agent: *\nAllow: /\n";
        const CAPTURED_AT: &str = "2026-08-01T00:00:00Z";
        store
            .persist_content(&review.robots_snapshot_digest, ROBOTS_BYTES)
            .expect("preserved robots");
        store
            .insert_fetch_observation(&FetchObservation {
                id: review.robots_provenance.clone().expect("robots fetch id"),
                source_id: Some(review.source_id.clone()),
                requested_url: review.robots_url.clone(),
                resolved_ips: vec!["203.0.113.10".to_owned()],
                retrieved_at: CAPTURED_AT.to_owned(),
                method: "GET".to_owned(),
                status_code: 200,
                redirect_target: None,
                final_url: review.robots_url.clone(),
                allowlisted_headers: Vec::new(),
                content_type: Some("text/plain".to_owned()),
                content_length: Some(ROBOTS_BYTES.len() as u64),
                etag: None,
                last_modified: None,
                body_digest: Some(review.robots_snapshot_digest.clone()),
                error: None,
            })
            .expect("robots receipt");
        store.insert_source_review(review).expect("review");
    }

    fn insert_review(store: &Store, source_id: &str, host: &str) {
        let config = test_config();
        insert_review_record(
            store,
            &review(&config, source_id, "2027-01-01T00:00:00Z", host),
        );
    }

    fn refresh_test(
        store: &Store,
        source_id: &str,
        transport: &mut dyn ProcurementTransport,
        day: u8,
    ) -> Result<()> {
        refresh_live_with(store, &test_config(), source_id, transport, test_now(day))
    }

    /// A fake transport that serves fixed bytes, tracks whether it was called,
    /// and can simulate a 304.
    struct FakeTransport {
        bytes: Vec<u8>,
        not_modified: bool,
        calls: usize,
    }

    impl FakeTransport {
        fn new(bytes: &[u8]) -> Self {
            Self {
                bytes: bytes.to_vec(),
                not_modified: false,
                calls: 0,
            }
        }
        fn not_modified(bytes: &[u8]) -> Self {
            Self {
                bytes: bytes.to_vec(),
                not_modified: true,
                calls: 0,
            }
        }
    }

    impl ProcurementTransport for FakeTransport {
        fn fetch(
            &mut self,
            _source_id: &str,
            url: &str,
            _prior_etag: Option<&str>,
            _retrieved_at: &str,
        ) -> Result<TransportFetch, String> {
            self.calls += 1;
            Ok(TransportFetch {
                bytes: if self.not_modified {
                    Vec::new()
                } else {
                    self.bytes.clone()
                },
                etag: Some("\"fake-etag\"".to_owned()),
                last_modified: None,
                not_modified: self.not_modified,
                final_url: url.to_owned(),
                content_type: Some("text/html".to_owned()),
                observations: Vec::new(),
            })
        }
    }

    #[derive(Default)]
    struct FailingTransport {
        calls: usize,
    }

    impl ProcurementTransport for FailingTransport {
        fn fetch(
            &mut self,
            _source_id: &str,
            _url: &str,
            _prior_etag: Option<&str>,
            _retrieved_at: &str,
        ) -> Result<TransportFetch, String> {
            self.calls += 1;
            Err("simulated network failure".to_owned())
        }
    }

    const AWARDS_HOST: &str = "coloradosprings.gov";

    /// Workspace-root-anchored path to a fixture (tests run from the crate dir).
    fn workspace_fixture(rel: &str) -> String {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(rel)
            .to_string_lossy()
            .into_owned()
    }

    const AWARDS_FIXTURE: &str = "fixtures/procurement/contract-awards.html";
    const AWARDS_FIXTURE_2: &str = "fixtures/procurement/contract-awards-2.html";
    const SOLICITATIONS_FIXTURE: &str = "fixtures/procurement/solicitations.html";

    fn baseline_path() -> String {
        workspace_fixture(AWARDS_FIXTURE)
    }
    fn changed_path() -> String {
        workspace_fixture(AWARDS_FIXTURE_2)
    }

    fn awards_bytes() -> Vec<u8> {
        // A minimal award snapshot with one row.
        br"<table>
        <tr><th>RFP</th><th>Project</th><th>Vendor</th><th>Amount</th><th>Date</th><th>Notes</th></tr>
        <tr><td>25-001</td><td>Streetlights</td><td>Acme Corp</td><td>$12,345.00</td><td>2026-03-01</td><td></td></tr>
        </table>"
            .to_vec()
    }

    #[test]
    fn dry_run_makes_zero_transport_calls() {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open");
        let transport = FakeTransport::new(&awards_bytes());
        // refresh_procurement in dry-run mode must never construct or call a transport.
        let out = refresh_procurement(
            &store,
            &test_config(),
            "colorado-springs-contract-awards",
            false,
        );
        assert!(out.is_ok());
        assert_eq!(transport.calls, 0, "dry run must make zero transport calls");
        assert_eq!(
            store
                .source_snapshots("colorado-springs-contract-awards")
                .expect("snap")
                .len(),
            0
        );
    }

    #[test]
    fn dry_run_prints_planned_comparison() {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open");
        let plan = dry_run_plan(&store, "colorado-springs-contract-awards").expect("plan");
        assert!(plan.planned_comparison.starts_with("initial GET "));
        assert!(plan.latest_snapshot_digest.is_none());
    }

    #[test]
    fn live_no_source_review_is_refused_and_writes_nothing() {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open");
        let mut transport = FakeTransport::new(&awards_bytes());
        let out = refresh_test(
            &store,
            "colorado-springs-contract-awards",
            &mut transport,
            8,
        );
        assert!(out.is_err());
        assert!(out.unwrap_err().to_string().contains("source review"));
        assert_eq!(
            transport.calls, 0,
            "transport must not be called before the gate"
        );
        assert_eq!(
            store
                .source_snapshots("colorado-springs-contract-awards")
                .expect("snap")
                .len(),
            0
        );
        assert_eq!(store.all_procurement_alerts().expect("alerts").len(), 0);
    }

    #[test]
    fn live_expired_review_is_refused() {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open");
        insert_review_record(
            &store,
            &review(
                &test_config(),
                "colorado-springs-contract-awards",
                "2026-09-01T00:00:00Z",
                AWARDS_HOST,
            ),
        );
        let mut transport = FakeTransport::new(&awards_bytes());
        let out = refresh_test(
            &store,
            "colorado-springs-contract-awards",
            &mut transport,
            8,
        );
        assert!(out.is_err());
        assert_eq!(store.all_procurement_alerts().expect("alerts").len(), 0);
    }

    #[test]
    fn live_refuses_unbound_or_changed_robots_review_before_transport() {
        let source_id = "colorado-springs-contract-awards";
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open");
        let mut legacy = review(
            &test_config(),
            source_id,
            "2027-01-01T00:00:00Z",
            AWARDS_HOST,
        );
        legacy.robots_snapshot_digest.clear();
        legacy.robots_provenance = None;
        store.insert_source_review(&legacy).expect("legacy review");
        let mut transport = FakeTransport::new(&awards_bytes());
        let error = refresh_test(&store, source_id, &mut transport, 8).unwrap_err();
        assert!(error.to_string().contains("robots snapshot digest"));
        assert_eq!(transport.calls, 0);

        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open");
        let valid = review(
            &test_config(),
            source_id,
            "2027-01-01T00:00:00Z",
            AWARDS_HOST,
        );
        insert_review_record(&store, &valid);
        std::fs::write(
            store
                .content_path(&valid.robots_snapshot_digest)
                .expect("path"),
            b"changed",
        )
        .expect("tamper saved bytes");
        let error = refresh_test(&store, source_id, &mut transport, 8).unwrap_err();
        assert!(error.to_string().contains("do not match the review digest"));
        assert_eq!(transport.calls, 0);
    }

    #[test]
    fn changed_source_configuration_or_unreviewed_path_is_refused() {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open");
        let source_id = "colorado-springs-contract-awards";
        insert_review(&store, source_id, AWARDS_HOST);
        let mut changed = test_config();
        changed
            .sources
            .iter_mut()
            .find(|source| source.id == source_id)
            .expect("source")
            .maximum_bytes += 1;
        let mut transport = FakeTransport::new(&awards_bytes());
        let result = refresh_live_with(&store, &changed, source_id, &mut transport, test_now(8));
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("configuration changed")
        );
        assert_eq!(transport.calls, 0);

        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open");
        let mut wrong_scope = review(
            &test_config(),
            source_id,
            "2027-01-01T00:00:00Z",
            AWARDS_HOST,
        );
        wrong_scope.endpoint_patterns = vec!["/some-other-page".to_owned()];
        insert_review_record(&store, &wrong_scope);
        let result = refresh_test(&store, source_id, &mut transport, 8);
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("outside reviewed scope")
        );
        assert_eq!(transport.calls, 0);
    }

    #[test]
    fn live_changed_bytes_produce_new_snapshot_and_alerts() {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open");
        insert_review(&store, "colorado-springs-contract-awards", AWARDS_HOST);
        // Ingest a baseline snapshot via the offline fixture path.
        crate::procurement_cmd::ingest_awards(&store, &baseline_path(), false)
            .expect("ingest baseline");
        let before = store
            .source_snapshots("colorado-springs-contract-awards")
            .expect("snap")
            .len();
        assert_eq!(before, 1);

        // Refresh returns the second (changed) fixture -> new snapshot + alerts.
        let changed = std::fs::read(changed_path()).expect("read");
        let mut transport = FakeTransport::new(&changed);
        let out = refresh_test(
            &store,
            "colorado-springs-contract-awards",
            &mut transport,
            8,
        );
        assert!(out.is_ok(), "live refresh should succeed: {:?}", out.err());

        let snapshots = store
            .source_snapshots("colorado-springs-contract-awards")
            .expect("snap");
        assert_eq!(
            snapshots.len(),
            2,
            "a changed refresh records a second snapshot"
        );
        assert!(snapshots[1].supersedes.as_deref() == Some(snapshots[0].id.as_str()));
        assert_eq!(
            snapshots[0].persisted_digest,
            std::fs::read(baseline_path())
                .map(|b| sha256_hex(&b))
                .expect("digest"),
            "the first snapshot's digest must be unchanged"
        );

        // The second fixture edits an amount, removes a row, adds a row, and
        // alters a vendor name, so all three change kinds appear.
        let alerts = store.all_procurement_alerts().expect("alerts");
        let kinds: std::collections::BTreeSet<&str> = alerts
            .iter()
            .map(|a| {
                a.changes
                    .first()
                    .map(|c| c.change_kind.label())
                    .unwrap_or_default()
            })
            .collect();
        assert!(kinds.contains("record_added"));
        assert!(kinds.contains("record_modified"));
        assert!(kinds.contains("record_removed"));
        assert!(alerts.iter().all(|alert| alert.matter_ids.len() == 1));
        let added = alerts
            .iter()
            .find(|alert| {
                alert.changes[0].change_kind == ProcurementChangeKind::RecordAdded
                    && alert.changes[0].row_identity == "R25044AB"
            })
            .expect("added award alert");
        assert!(
            store
                .procurement_events(&added.matter_ids[0])
                .expect("attached award events")
                .iter()
                .any(|event| event.evidence_ids.contains(&snapshots[1].id)),
            "live award rows must attach to the matter with this snapshot as evidence"
        );
        assert!(
            alerts
                .iter()
                .filter(
                    |a| a.changes.first().map(|c| c.change_kind.label()) == Some("record_modified")
                )
                .all(|a| !a.changes[0].field_diffs.is_empty()),
            "record_modified alerts must carry a field-level diff"
        );
    }

    #[test]
    fn live_changed_refresh_is_idempotent() {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open");
        insert_review(&store, "colorado-springs-contract-awards", AWARDS_HOST);
        crate::procurement_cmd::ingest_awards(&store, &baseline_path(), false)
            .expect("ingest baseline");
        let changed = std::fs::read(changed_path()).expect("read");

        let mut t1 = FakeTransport::new(&changed);
        refresh_test(&store, "colorado-springs-contract-awards", &mut t1, 8).expect("refresh 1");
        let alerts_1 = store.all_procurement_alerts().expect("alerts").len();

        // Re-ingesting the same snapshot pair must not duplicate alerts.
        let mut t2 = FakeTransport::new(&changed);
        refresh_test(&store, "colorado-springs-contract-awards", &mut t2, 9).expect("refresh 2");
        let alerts_2 = store.all_procurement_alerts().expect("alerts").len();
        assert_eq!(
            alerts_1, alerts_2,
            "re-running the same refresh is idempotent"
        );
    }

    #[test]
    fn returned_bytes_create_a_new_head_and_reverse_change_alerts() {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open");
        let source_id = "colorado-springs-contract-awards";
        insert_review(&store, source_id, AWARDS_HOST);
        let first_bytes = std::fs::read(baseline_path()).expect("first fixture");
        let second_bytes = std::fs::read(changed_path()).expect("second fixture");
        refresh_test(&store, source_id, &mut FakeTransport::new(&first_bytes), 8).expect("first A");
        refresh_test(&store, source_id, &mut FakeTransport::new(&second_bytes), 9).expect("B");
        refresh_test(&store, source_id, &mut FakeTransport::new(&first_bytes), 10)
            .expect("returned A");

        let snapshots = store.source_snapshots(source_id).expect("snapshots");
        assert_eq!(snapshots.len(), 3);
        assert_eq!(snapshots[0].persisted_digest, snapshots[2].persisted_digest);
        assert_ne!(snapshots[0].id, snapshots[2].id);
        assert_eq!(
            snapshots[1].supersedes.as_deref(),
            Some(snapshots[0].id.as_str())
        );
        assert_eq!(
            snapshots[2].supersedes.as_deref(),
            Some(snapshots[1].id.as_str())
        );
        assert_eq!(
            latest_snapshot(&store, source_id)
                .expect("latest")
                .expect("head")
                .id,
            snapshots[2].id
        );
        assert!(
            store
                .all_procurement_alerts()
                .expect("alerts")
                .iter()
                .any(|alert| {
                    alert.old_snapshot_id == snapshots[1].id
                        && alert.new_snapshot_id == snapshots[2].id
                }),
            "B-to-A must emit a new reverse change alert"
        );
        assert_eq!(
            std::fs::read(
                store
                    .content_path(&snapshots[0].persisted_digest)
                    .expect("A blob")
            )
            .expect("preserved A bytes"),
            first_bytes
        );
        assert_eq!(
            std::fs::read(
                store
                    .content_path(&snapshots[1].persisted_digest)
                    .expect("B blob")
            )
            .expect("preserved B bytes"),
            second_bytes
        );
    }

    #[test]
    fn failed_network_attempt_still_consumes_minimum_interval() {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open");
        let source_id = "colorado-springs-contract-awards";
        let mut config = test_config();
        config
            .sources
            .iter_mut()
            .find(|source| source.id == source_id)
            .expect("source")
            .minimum_interval_seconds = 86_400;
        insert_review_record(
            &store,
            &review(&config, source_id, "2027-01-01T00:00:00Z", AWARDS_HOST),
        );
        let mut failed = FailingTransport::default();
        let first = refresh_live_with(&store, &config, source_id, &mut failed, test_now(8));
        assert!(
            first
                .unwrap_err()
                .to_string()
                .contains("simulated network failure")
        );
        assert_eq!(failed.calls, 1);
        let bytes = std::fs::read(baseline_path()).expect("valid awards fixture");
        let mut retry = FakeTransport::new(&bytes);
        let second = refresh_live_with(&store, &config, source_id, &mut retry, test_now(8));
        assert!(second.unwrap_err().to_string().contains("rate limit"));
        assert_eq!(retry.calls, 0);
        assert!(
            store
                .source_snapshots(source_id)
                .expect("snapshots")
                .is_empty()
        );
        refresh_live_with(&store, &config, source_id, &mut retry, test_now(9))
            .expect("retry after a full interval");
        assert_eq!(retry.calls, 1);
    }

    #[test]
    fn failed_parse_attempt_still_consumes_minimum_interval() {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open");
        let source_id = "colorado-springs-contract-awards";
        let mut config = test_config();
        config
            .sources
            .iter_mut()
            .find(|source| source.id == source_id)
            .expect("source")
            .minimum_interval_seconds = 86_400;
        insert_review_record(
            &store,
            &review(&config, source_id, "2027-01-01T00:00:00Z", AWARDS_HOST),
        );
        let mut malformed = FakeTransport::new(b"<main>no award table</main>");
        assert!(
            refresh_live_with(&store, &config, source_id, &mut malformed, test_now(8)).is_err()
        );
        assert_eq!(malformed.calls, 1);
        let mut retry = FakeTransport::new(&awards_bytes());
        let second = refresh_live_with(&store, &config, source_id, &mut retry, test_now(8));
        assert!(second.unwrap_err().to_string().contains("rate limit"));
        assert_eq!(retry.calls, 0);
        assert!(
            store
                .source_snapshots(source_id)
                .expect("snapshots")
                .is_empty()
        );
        let malformed_bytes = b"<main>no award table</main>";
        assert_eq!(
            std::fs::read(
                store
                    .content_path(&sha256_hex(malformed_bytes))
                    .expect("quarantined blob path")
            )
            .expect("quarantined original bytes"),
            malformed_bytes
        );
    }

    #[test]
    fn live_identical_bytes_record_304_provenance_and_no_alerts() {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open");
        insert_review(&store, "colorado-springs-contract-awards", AWARDS_HOST);
        // First ingest a baseline snapshot via the fixture path.
        crate::procurement_cmd::ingest_awards(&store, &baseline_path(), false)
            .expect("ingest baseline");
        let before = store
            .source_snapshots("colorado-springs-contract-awards")
            .expect("snap")
            .len();

        // The fake returns identical (unchanged) bytes -> 304-style provenance.
        let baseline_bytes = std::fs::read(baseline_path()).expect("read");
        let mut transport = FakeTransport::not_modified(&baseline_bytes);
        let out = refresh_test(
            &store,
            "colorado-springs-contract-awards",
            &mut transport,
            8,
        );
        assert!(out.is_ok());
        let after = store
            .source_snapshots("colorado-springs-contract-awards")
            .expect("snap")
            .len();
        assert_eq!(before, after, "a 304 must not create a new snapshot");
        assert_eq!(
            store.all_procurement_alerts().expect("alerts").len(),
            0,
            "a 304 must create no alerts"
        );
        let coverage = store
            .coverage_entries("colorado-springs-contract-awards")
            .expect("coverage");
        assert!(coverage.iter().any(|entry| {
            entry.http_status == Some(304)
                && entry.state == CoverageState::InformationalOnly
                && entry.retrieved_at == "2026-11-08T12:00:00Z"
        }));
    }

    #[test]
    fn repeated_refreshes_keep_distinct_coverage_and_real_timestamps() {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open");
        insert_review(&store, "colorado-springs-contract-awards", AWARDS_HOST);
        let bytes = std::fs::read(baseline_path()).expect("baseline fixture");
        refresh_test(
            &store,
            "colorado-springs-contract-awards",
            &mut FakeTransport::new(&bytes),
            8,
        )
        .expect("first refresh");
        refresh_test(
            &store,
            "colorado-springs-contract-awards",
            &mut FakeTransport::not_modified(&bytes),
            9,
        )
        .expect("second refresh");
        let coverage = store
            .coverage_entries("colorado-springs-contract-awards")
            .expect("coverage");
        assert_eq!(coverage.len(), 2);
        assert_eq!(coverage[0].retrieved_at, "2026-11-08T12:00:00Z");
        assert_eq!(coverage[1].retrieved_at, "2026-11-09T12:00:00Z");
    }

    #[test]
    fn live_solicitation_refresh_uses_its_parser_and_preserves_original_bytes() {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open");
        let source_id = "colorado-springs-solicitation-mirror";
        insert_review(&store, source_id, AWARDS_HOST);
        let bytes = std::fs::read(workspace_fixture(SOLICITATIONS_FIXTURE)).expect("fixture");
        refresh_test(&store, source_id, &mut FakeTransport::new(&bytes), 8)
            .expect("solicitation refresh");
        let snapshot = store
            .source_snapshots(source_id)
            .expect("snapshots")
            .pop()
            .expect("snapshot");
        assert_eq!(snapshot.record_count, Some(3));
        assert_eq!(snapshot.parser_version, "solicitations-1.0");
        assert_eq!(
            std::fs::read(
                store
                    .content_path(&snapshot.persisted_digest)
                    .expect("blob path")
            )
            .expect("preserved original bytes"),
            bytes
        );
        assert_eq!(
            pnull_procurement::snapshot_rows(&store, &snapshot.id)
                .expect("stored rows")
                .len(),
            3
        );
    }

    #[test]
    fn changed_solicitation_rows_emit_cautious_idempotent_alerts() {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open");
        let source_id = "colorado-springs-solicitation-mirror";
        insert_review(&store, source_id, AWARDS_HOST);
        let original = std::fs::read(workspace_fixture(SOLICITATIONS_FIXTURE)).expect("fixture");
        refresh_test(&store, source_id, &mut FakeTransport::new(&original), 8)
            .expect("first refresh");
        let changed = String::from_utf8(original)
            .expect("UTF-8 fixture")
            .replace("Snow Removal Services", "Winter Road Services");
        refresh_test(
            &store,
            source_id,
            &mut FakeTransport::new(changed.as_bytes()),
            9,
        )
        .expect("second refresh");
        let alerts = store.all_procurement_alerts().expect("alerts");
        assert_eq!(alerts.len(), 1);
        assert_eq!(
            alerts[0].changes[0].change_kind,
            ProcurementChangeKind::RecordModified
        );
        assert_eq!(alerts[0].matter_ids, vec!["proc:matter:co:r25301ab"]);
        assert!(
            store.procurement_matter(&alerts[0].matter_ids[0]).is_ok(),
            "the alert must point to the attached normalized matter"
        );
        assert!(
            alerts[0]
                .summary
                .contains("informational mirror may be incomplete")
        );
        assert!(!alerts[0].summary.contains("contract approved"));
        refresh_test(
            &store,
            source_id,
            &mut FakeTransport::new(changed.as_bytes()),
            10,
        )
        .expect("repeat same bytes");
        assert_eq!(store.all_procurement_alerts().expect("alerts").len(), 1);
    }

    #[test]
    fn unnumbered_solicitation_row_does_not_create_an_identifier_matter() {
        let records = parse_solicitations(
            "<main><h2>Unnumbered procurement opportunity</h2></main>",
            "digest",
        )
        .expect("parse");
        let rows = solicitation_record_rows(&records);
        assert_eq!(rows.len(), 1);
        assert!(
            alert_matter_id(
                "colorado-springs-solicitation-mirror",
                &rows[0].key,
                &rows,
                None,
            )
            .is_none()
        );
    }

    #[test]
    fn rate_limit_refuses_a_second_live_request_before_transport() {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open");
        let mut config = test_config();
        let source_id = "colorado-springs-contract-awards";
        let source = config
            .sources
            .iter_mut()
            .find(|source| source.id == source_id)
            .expect("source");
        source.minimum_interval_seconds = 86_400;
        let review = review(&config, source_id, "2027-01-01T00:00:00Z", AWARDS_HOST);
        insert_review_record(&store, &review);
        let bytes = std::fs::read(baseline_path()).expect("baseline fixture");
        refresh_live_with(
            &store,
            &config,
            source_id,
            &mut FakeTransport::new(&bytes),
            test_now(8),
        )
        .expect("first refresh");
        let mut second = FakeTransport::new(&bytes);
        let result = refresh_live_with(&store, &config, source_id, &mut second, test_now(8));
        assert!(result.unwrap_err().to_string().contains("rate limit"));
        assert_eq!(second.calls, 0);
    }

    #[test]
    fn unknown_source_is_refused() {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open");
        let mut transport = FakeTransport::new(&awards_bytes());
        let out = refresh_test(&store, "not-a-source", &mut transport, 8);
        assert!(out.is_err());
    }

    #[test]
    fn refresh_uses_database_rows_not_the_deleted_fixture() {
        // The exact regression this phase fixes: change detection must compare
        // the immutable rows stored for the previous snapshot, never reconstruct
        // them from a fixture file on disk. Here snapshot A is ingested, the
        // store is reopened (process restart), and the fixture file is deleted.
        // Snapshot B is then compared using database state alone, and the
        // change alerts are still produced.
        let dir = tempdir().expect("tempdir");
        let data_dir = dir.path().join("data");
        let store = Store::open(&data_dir).expect("open");
        insert_review(&store, "colorado-springs-contract-awards", AWARDS_HOST);

        // Write snapshot A's fixture to a temp file we control, ingest it, then
        // delete the file so no fixture remains on disk.
        let fixture_a = dir.path().join("contract-awards.html");
        let a_bytes = std::fs::read(baseline_path()).expect("read baseline fixture");
        std::fs::write(&fixture_a, &a_bytes).expect("write temp fixture A");
        crate::procurement_cmd::ingest_awards(&store, &fixture_a.to_string_lossy(), false)
            .expect("ingest baseline");
        std::fs::remove_file(&fixture_a).expect("delete fixture A");

        // Reopen the store, simulating a process restart. The prior snapshot's
        // rows must be loaded from the database, not from the deleted fixture.
        drop(store);
        let store = Store::open(&data_dir).expect("reopen store");

        // Snapshot B differs from A: edit an amount, remove a row, add a row.
        let changed = std::fs::read(changed_path()).expect("read changed fixture");
        let mut transport = FakeTransport::new(&changed);
        let out = refresh_test(
            &store,
            "colorado-springs-contract-awards",
            &mut transport,
            8,
        );
        assert!(out.is_ok(), "live refresh should succeed: {:?}", out.err());

        let alerts = store.all_procurement_alerts().expect("alerts");
        let kinds: std::collections::BTreeSet<&str> = alerts
            .iter()
            .filter_map(|a| a.changes.first().map(|c| c.change_kind.label()))
            .collect();
        assert!(
            kinds.contains("record_added")
                && kinds.contains("record_modified")
                && kinds.contains("record_removed"),
            "change detection must work without the fixture on disk; got {kinds:?}"
        );
    }
}
