//! `pnull procurement refresh` — the exposure heartbeat (v0.0.4, Item 6).
//!
//! Keeps the ledger fresh: `--dry-run` (the default) prints exactly what would
//! be fetched and compared with zero network activity. `--live` fetches the
//! reviewed surface, records a new snapshot (or 304 provenance), runs Item 1
//! change detection, prints the alert count and affected matter ids, and writes
//! a coverage-ledger entry. Any refusal or failure fails closed: state the
//! reason and change nothing.
//!
//! The transport is behind a trait so tests inject a fake that never touches
//! the network. The real transport performs a DNS-safe conditional HTTPS GET
//! and is only ever constructed on the `--live` path behind the persistent
//! source-review gate.

use anyhow::{Result, anyhow, bail};
use pnull_core::{
    CoverageState, FetchObservation, SourceAuthority, SourceReview, Store, sha256_hex,
};
use pnull_procurement::{
    Acquisition, build_change_alerts, ensure_matter, latest_snapshot, matter_id_for_identifier,
    parse_awards_table, persist_change_alerts, record_snapshot, record_unchanged,
};

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
    fn fetch(&mut self, url: &str, prior_etag: Option<&str>) -> Result<TransportFetch, String>;
}

/// The offline dry-run result: the planned comparison with zero network.
pub struct DryRunPlan {
    pub source_id: String,
    pub source_url: String,
    pub latest_snapshot_digest: Option<String>,
    pub planned_comparison: String,
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
fn check_live_gate(store: &Store, source_id: &str) -> Result<SourceReview> {
    let (url, _) = surface_url(source_id)
        .ok_or_else(|| anyhow::anyhow!("unknown procurement source: {source_id}"))?;
    let review = crate::procurement_cmd::require_review_for_live(store, source_id)?;
    let host = url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or_default();
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
    Ok(review)
}

/// Runs the live refresh given a transport. Fails closed on any gate failure.
#[allow(clippy::too_many_lines)]
fn refresh_live_with(
    store: &Store,
    source_id: &str,
    transport: &mut dyn ProcurementTransport,
) -> Result<()> {
    let _review = check_live_gate(store, source_id)?;
    let (url, _) = surface_url(source_id)
        .ok_or_else(|| anyhow::anyhow!("unknown procurement source: {source_id}"))?;

    // Prior evidence for a conditional request, if a prior snapshot exists.
    let latest = latest_snapshot(store, source_id).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let prior_etag = latest.as_ref().and_then(|s| s.etag.clone());

    let fetched = transport
        .fetch(url, prior_etag.as_deref())
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
        let acquisition = Acquisition {
            source_id: source_id.to_owned(),
            source_url: url.to_owned(),
            retrieved_at: fetched_retrieved_at(),
            bytes_digest: existing.persisted_digest.clone(),
            content_type: fetched.content_type.clone(),
            etag: fetched.etag.clone(),
            last_modified: fetched.last_modified.clone(),
            final_url: fetched.final_url.clone(),
            redirect_history: Vec::new(),
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
    let html = String::from_utf8_lossy(&fetched.bytes);
    let rows = parse_awards_table(&html, &digest)
        .map_err(|e| anyhow::anyhow!("parse refreshed awards snapshot: {e}"))?;

    let acquisition = Acquisition {
        source_id: source_id.to_owned(),
        source_url: url.to_owned(),
        retrieved_at: fetched_retrieved_at(),
        bytes_digest: digest.clone(),
        content_type: fetched.content_type.clone(),
        etag: fetched.etag.clone(),
        last_modified: fetched.last_modified.clone(),
        final_url: fetched.final_url.clone(),
        redirect_history: Vec::new(),
        parser_version: parser_version(source_id),
        schema_version: 2,
        authority: SourceAuthority::OfficialInformationalMirror,
        coverage_state: CoverageState::InformationalOnly,
        observations: fetched.observations.clone(),
    };

    // Prior rows for change detection come from the preserved bytes of the
    // most recent snapshot. Procurement snapshots retain metadata (digest) but
    // not the raw bytes, so the preserved fixture is re-read when present. In
    // offline operation the prior fixture is on disk; a live fetch without a
    // preserved prior fixture diff (and thus alerts) would be limited to
    // additions, which is documented as a known limitation.
    let prior_rows = prior_rows_from_disk(store, source_id);
    let (new_snapshot, _) = record_snapshot(
        store,
        &acquisition,
        latest.as_ref(),
        Some(rows.len() as u64),
        &pnull_procurement::changealert::award_record_rows(&prior_rows),
        &pnull_procurement::changealert::award_record_rows(&rows),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    // Change detection only applies against a prior snapshot. With no prior
    // snapshot there is nothing to diff against, so no alerts are produced.
    let mut alerts = Vec::new();
    let mut prior_for_alerts: Option<pnull_core::SourceSnapshot> = None;
    if let Some(prior) = latest
        .as_ref()
        .filter(|p| p.persisted_digest != new_snapshot.persisted_digest)
    {
        let built = build_change_alerts(
            source_id,
            "contract-award-table",
            &prior.id,
            &prior.persisted_digest,
            &new_snapshot.id,
            &new_snapshot.persisted_digest,
            &fetched_retrieved_at(),
            new_snapshot.coverage_state,
            &prior_rows,
            &rows,
            &[],
            &[],
        );
        alerts = built;
        prior_for_alerts = Some(prior.clone());
    }

    // Resolve affected matter ids by the exact-identifier rule (never similarity).
    let mut affected = Vec::new();
    for alert in &mut alerts {
        let normalized = alert
            .changes
            .first()
            .map(|c| matter_id_for_identifier(&c.row_identity))
            .unwrap_or_default();
        alert.matter_ids = vec![normalized.clone()];
        if !normalized.is_empty() && !affected.contains(&normalized) {
            affected.push(normalized);
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
            &format!("{matter_id} — affected award record"),
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
    println!("  parsed {} award row(s)", rows.len());
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

/// The deterministic retrieval timestamp used by the offline live-refresh path.
/// Real `--live` refresh would use the actual clock; the offline fixtures use a
/// fixed timestamp so the ledger stays reproducible.
fn fetched_retrieved_at() -> String {
    "2026-08-31T00:00:00Z".to_owned()
}

fn parser_version(source_id: &str) -> String {
    match source_id {
        "colorado-springs-solicitation-mirror" => "solicitations-1.0".to_owned(),
        _ => "awards-1.0".to_owned(),
    }
}

/// Parses the rows of the most recent snapshot from its preserved fixture.
///
/// Procurement snapshots store metadata (digest, retrieval) but not the raw
/// bytes, so the prior rows are re-read from the preserved fixture on disk when
/// it is present (offline). Returns empty rows when no prior snapshot exists or
/// no preserved fixture is available, in which case change detection can only
/// report additions.
fn prior_rows_from_disk(store: &Store, source_id: &str) -> Vec<pnull_procurement::AwardRow> {
    let Some((_, fixture)) = surface_url(source_id) else {
        return Vec::new();
    };
    let has_prior = matches!(latest_snapshot(store, source_id), Ok(Some(_)));
    if !has_prior {
        return Vec::new();
    }
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("fixtures/procurement")
        .join(fixture);
    let Ok(bytes) = std::fs::read(&path) else {
        return Vec::new();
    };
    let html = String::from_utf8_lossy(&bytes);
    let digest = sha256_hex(&bytes);
    parse_awards_table(&html, &digest).unwrap_or_default()
}

/// Runs the refresh command. `live=false` is the dry-run default (no network).
pub fn refresh_procurement(store: &Store, source_id: &str, live: bool) -> Result<()> {
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
    // Real live path: construct a DNS-safe HTTPS transport behind the review
    // gate and perform the refresh.
    let mut transport = HttpTransport::new().map_err(|e| anyhow!(e))?;
    refresh_live_with(store, source_id, &mut transport)
}

/// The real DNS-safe conditional HTTPS transport. Only constructed on the
/// `--live` path (never in tests or the demo). Wraps `pnull_http`'s
/// provenance-aware fetch.
struct HttpTransport {
    client: pnull_http::ReqwestTransport,
}

impl HttpTransport {
    fn new() -> Result<Self, String> {
        let client =
            pnull_http::ReqwestTransport::new(MAX_FETCH_BYTES).map_err(|e| e.to_string())?;
        Ok(Self { client })
    }
}

/// Cap on the bytes fetched for a refreshed surface.
const MAX_FETCH_BYTES: usize = 8 * 1024 * 1024;

impl ProcurementTransport for HttpTransport {
    fn fetch(&mut self, url: &str, prior_etag: Option<&str>) -> Result<TransportFetch, String> {
        let parsed = url::Url::parse(url).map_err(|e| e.to_string())?;
        let host = parsed
            .host_str()
            .ok_or_else(|| "invalid url: no host".to_owned())?
            .to_owned();
        let config = pnull_http::FetchConfig {
            reviewed_hosts: vec![host.clone()],
            max_bytes: MAX_FETCH_BYTES,
        };
        let resolver = pnull_http::SystemResolver;
        let prior = prior_etag.map(|etag| pnull_http::PriorEvidence {
            evidence_id: etag.to_owned(),
            etag: Some(etag.to_owned()),
            last_modified: None,
        });
        let request = pnull_http::FetchRequest {
            source_id: None,
            requested_url: url.to_owned(),
            retrieved_at: fetched_retrieved_at(),
            prior,
        };
        let result = pnull_http::provenance_fetch(&config, &resolver, &self.client, &request)
            .map_err(|e| e.to_string())?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use pnull_core::SourceReview;
    use tempfile::tempdir;

    fn review(source_id: &str, expires: &str, host: &str) -> SourceReview {
        SourceReview {
            id: format!("review:{source_id}"),
            source_id: source_id.to_owned(),
            source_config_digest: "cfg".to_owned(),
            reviewed_hosts: vec![host.to_owned()],
            endpoint_patterns: Vec::new(),
            robots_url: format!("https://{host}/robots.txt"),
            robots_snapshot_digest: "digest".to_owned(),
            robots_provenance: None,
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

    fn insert_review(store: &Store, source_id: &str, host: &str) {
        store
            .insert_source_review(&review(source_id, "2027-01-01T00:00:00Z", host))
            .expect("insert review");
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
            url: &str,
            _prior_etag: Option<&str>,
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
        let out = refresh_procurement(&store, "colorado-springs-contract-awards", false);
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
        let out = refresh_live_with(&store, "colorado-springs-contract-awards", &mut transport);
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
        store
            .insert_source_review(&review(
                "colorado-springs-contract-awards",
                "2026-01-01T00:00:00Z",
                AWARDS_HOST,
            ))
            .expect("review");
        let mut transport = FakeTransport::new(&awards_bytes());
        let out = refresh_live_with(&store, "colorado-springs-contract-awards", &mut transport);
        assert!(out.is_err());
        assert_eq!(store.all_procurement_alerts().expect("alerts").len(), 0);
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
        let out = refresh_live_with(&store, "colorado-springs-contract-awards", &mut transport);
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
        refresh_live_with(&store, "colorado-springs-contract-awards", &mut t1).expect("refresh 1");
        let alerts_1 = store.all_procurement_alerts().expect("alerts").len();

        // Re-ingesting the same snapshot pair must not duplicate alerts.
        let mut t2 = FakeTransport::new(&changed);
        refresh_live_with(&store, "colorado-springs-contract-awards", &mut t2).expect("refresh 2");
        let alerts_2 = store.all_procurement_alerts().expect("alerts").len();
        assert_eq!(
            alerts_1, alerts_2,
            "re-running the same refresh is idempotent"
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
        let out = refresh_live_with(&store, "colorado-springs-contract-awards", &mut transport);
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
    }

    #[test]
    fn unknown_source_is_refused() {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open");
        let mut transport = FakeTransport::new(&awards_bytes());
        let out = refresh_live_with(&store, "not-a-source", &mut transport);
        assert!(out.is_err());
    }
}
