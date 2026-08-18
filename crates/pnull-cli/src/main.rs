use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Parser, Subcommand};
use pnull_core::{
    Alert, Citation, EvidenceDiff, Finding, PublicationAllowlist, ReviewBinding, ReviewDecision,
    ReviewState, SourceReview, SourceType, Store, sha256_hex, stable_id,
};
use pnull_detect::{
    build_alert, classify_document, compare, document_role, extract_matter_subjects_and_actions,
    load_rules, scan,
};
use pnull_geometry::render_review_image;
use pnull_http::PriorEvidence;
use pnull_ingest::{
    Budgets, BuildMetadata, DEFAULT_MAX_BYTES, IngestRequest, LegistarPagination, RealSandbox,
    Tracker, cite_quote, fetch_source, ingest_bytes,
};
use pnull_publish::{
    SiteConfig, assert_citations_approved, build_site, citation_id, citation_review_binding,
};
use pnull_x::{attempts_for_alert, draft, post_approved, reconcile};
use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

mod procurement_cmd;

const CONFIG_PATH: &str = "configs/states/co.toml";
const RULES_PATH: &str = "rules/surveillance.yml";

#[derive(Parser)]
#[command(
    name = "pnull",
    version,
    about = "Evidence infrastructure against institutional surveillance"
)]
struct Cli {
    #[arg(long, global = true, env = "PNUL_DATA_DIR", default_value = ".pnull")]
    data_dir: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Source {
        #[command(subcommand)]
        command: SourceCommand,
    },
    Ingest(IngestArgs),
    Scan,
    Diff {
        old_evidence_id: Option<String>,
        new_evidence_id: Option<String>,
    },
    BuildSite {
        #[arg(long, default_value = "site")]
        output: PathBuf,
    },
    Alerts,
    Matter {
        #[command(subcommand)]
        command: MatterCommand,
    },
    Citation {
        #[command(subcommand)]
        command: CitationCommand,
    },
    Review {
        #[command(subcommand)]
        command: ReviewCommand,
    },
    X {
        #[command(subcommand)]
        command: XCommand,
    },
    Verify {
        evidence_id: String,
    },
    Demo {
        #[arg(long, default_value = "demo-output")]
        output: PathBuf,
    },
    Procurement {
        #[command(subcommand)]
        command: ProcurementCommand,
    },
    Coverage {
        #[command(subcommand)]
        command: CoverageCommand,
    },
    Case {
        #[command(subcommand)]
        command: CaseCommand,
    },
    Cora {
        #[command(subcommand)]
        command: CoraCommand,
    },
}

#[derive(Subcommand)]
enum ProcurementCommand {
    Ingest {
        #[command(subcommand)]
        command: ProcurementIngestCommand,
    },
    Import {
        path: String,
        #[arg(long)]
        source_or_request_id: String,
        #[arg(long)]
        acquisition_date: String,
        #[arg(long)]
        role: String,
        #[arg(long)]
        operator: String,
        #[arg(long)]
        digest: String,
    },
    Reconcile {
        matter: String,
    },
    Show {
        matter: String,
    },
    Gaps {
        matter: String,
    },
}

#[derive(Subcommand)]
enum ProcurementIngestCommand {
    Solicitations {
        #[arg(long, default_value = "")]
        source: String,
        #[arg(long)]
        live: bool,
    },
    Awards {
        #[arg(long, default_value = "")]
        source: String,
        #[arg(long)]
        live: bool,
    },
    Openbook,
}

#[derive(Subcommand)]
enum CoverageCommand {
    Show,
    Diff {
        old_snapshot: String,
        new_snapshot: String,
    },
}

#[derive(Subcommand)]
enum CaseCommand {
    Build {
        matter: String,
        #[arg(long, default_value = "case-output")]
        output: PathBuf,
    },
}

#[derive(Subcommand)]
enum CoraCommand {
    Draft {
        matter: String,
    },
}

#[derive(Subcommand)]
enum SourceCommand {
    List,
    Review {
        #[command(subcommand)]
        command: SourceReviewCommand,
    },
}

#[derive(Subcommand)]
enum SourceReviewCommand {
    Capture {
        source_id: String,
    },
    Record {
        source_id: String,
        #[arg(long)]
        reviewer: String,
        #[arg(long)]
        note: String,
        #[arg(long)]
        expires: String,
    },
    Show {
        source_id: String,
    },
    Verify {
        source_id: String,
    },
}

#[derive(Args)]
struct IngestArgs {
    #[arg(long, default_value = "colorado-springs-legistar-events")]
    source_id: String,
    #[arg(long)]
    robots_reviewed: bool,
    #[arg(long, default_value_t = 100)]
    page_size: u32,
    #[arg(long, default_value_t = 5)]
    max_pages: u32,
}

#[derive(Subcommand)]
enum MatterCommand {
    List,
    Show { matter_id: String },
    Attachments { matter_id: String },
}

#[derive(Subcommand)]
enum CitationCommand {
    Show {
        citation_id: String,
    },
    Render {
        citation_id: String,
        #[arg(long)]
        output: PathBuf,
    },
}

#[derive(Subcommand)]
enum ReviewCommand {
    List,
    Show {
        citation_id: String,
    },
    Approve {
        citation_id: String,
        #[arg(long)]
        reviewer: String,
        #[arg(long)]
        note: String,
    },
    Reject {
        citation_id: String,
        #[arg(long)]
        reviewer: String,
        #[arg(long)]
        reason: String,
    },
    Supersede {
        decision_id: String,
        #[arg(long)]
        reviewer: String,
        #[arg(long)]
        reason: String,
    },
}

#[derive(Subcommand)]
enum XCommand {
    Draft {
        alert_id: String,
    },
    Approve {
        alert_id: String,
    },
    Post {
        alert_id: String,
        #[arg(long)]
        confirm: bool,
    },
    Attempts,
    Status {
        alert_id: String,
    },
    Reconcile {
        attempt_id: String,
        #[arg(long)]
        decision: String,
        #[arg(long)]
        operator: String,
        #[arg(long)]
        note: String,
        #[arg(long)]
        remote_id: Option<String>,
    },
}

#[derive(Clone, Debug, Deserialize)]
struct StateConfig {
    state_code: String,
    state_name: String,
    jurisdiction: String,
    canonical_base_url: String,
    x_feed_label: String,
    default_dry_run: bool,
    sources: Vec<SourceConfig>,
    demo_documents: Vec<DemoDocument>,
}

#[derive(Clone, Debug, Deserialize)]
struct SourceConfig {
    id: String,
    name: String,
    url: String,
    source_type: String,
    documented_at: String,
    official_discovery_url: String,
    minimum_interval_seconds: u64,
    maximum_bytes: usize,
    robots_status: String,
    terms_note: String,
}

#[derive(Clone, Debug, Deserialize)]
struct DemoDocument {
    id: String,
    path: PathBuf,
    url: String,
    title: String,
    publication_date: String,
    retrieval_timestamp: String,
    source_type: String,
    mime_type: String,
    sha256: String,
}

/// Bundle of the sandbox, budgets, and build metadata needed for ingestion.
struct ExtractContext {
    sandbox: RealSandbox,
    budgets: Tracker,
    build: BuildMetadata,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    run(cli)
}

fn run(cli: Cli) -> Result<()> {
    let config = load_config()?;
    match cli.command {
        Command::Source {
            command: SourceCommand::List,
        } => {
            source_list(&config);
            Ok(())
        }
        Command::Source {
            command: SourceCommand::Review { command },
        } => source_review_command(&cli.data_dir, &config, command),
        Command::Ingest(args) => live_ingest(&cli.data_dir, &config, &args),
        Command::Scan => scan_all(&cli.data_dir),
        Command::Diff {
            old_evidence_id,
            new_evidence_id,
        } => print_diff(
            &cli.data_dir,
            old_evidence_id.as_deref(),
            new_evidence_id.as_deref(),
        ),
        Command::BuildSite { output } => build_public_site(&cli.data_dir, &config, &output),
        Command::Alerts => list_alerts(&cli.data_dir),
        Command::Matter { command } => matter_command(&cli.data_dir, command),
        Command::Citation { command } => citation_command(&cli.data_dir, command),
        Command::Review { command } => review_command(&cli.data_dir, command),
        Command::X { command } => x_command(&cli.data_dir, &config, command),
        Command::Verify { evidence_id } => verify(&cli.data_dir, &evidence_id),
        Command::Demo { output } => run_demo(&output, &config),
        Command::Procurement { command } => procurement_command(&cli.data_dir, command),
        Command::Coverage { command } => coverage_command(&cli.data_dir, command),
        Command::Case { command } => case_command(&cli.data_dir, command),
        Command::Cora { command } => cora_command(&cli.data_dir, command),
    }
}

fn procurement_command(data_dir: &Path, command: ProcurementCommand) -> Result<()> {
    let store = Store::open(data_dir)?;
    match command {
        ProcurementCommand::Ingest {
            command: ProcurementIngestCommand::Solicitations { source, live },
        } => procurement_cmd::ingest_solicitations(&store, &source, live),
        ProcurementCommand::Ingest {
            command: ProcurementIngestCommand::Awards { source, live },
        } => procurement_cmd::ingest_awards(&store, &source, live),
        ProcurementCommand::Ingest {
            command: ProcurementIngestCommand::Openbook,
        } => procurement_cmd::ingest_openbook(&store),
        ProcurementCommand::Import {
            path,
            source_or_request_id,
            acquisition_date,
            role,
            operator,
            digest,
        } => procurement_cmd::import_record(
            data_dir,
            &path,
            &source_or_request_id,
            &acquisition_date,
            &role,
            &operator,
            &digest,
        ),
        ProcurementCommand::Reconcile { matter } => procurement_cmd::gaps(&store, &matter),
        ProcurementCommand::Show { matter } => procurement_cmd::show_matter(&store, &matter),
        ProcurementCommand::Gaps { matter } => procurement_cmd::gaps(&store, &matter),
    }
}

fn coverage_command(data_dir: &Path, command: CoverageCommand) -> Result<()> {
    let store = Store::open(data_dir)?;
    match command {
        CoverageCommand::Show => procurement_cmd::coverage_show(&store),
        CoverageCommand::Diff { old_snapshot, new_snapshot } => {
            procurement_cmd::coverage_diff(&store, &old_snapshot, &new_snapshot)
        }
    }
}

fn case_command(data_dir: &Path, command: CaseCommand) -> Result<()> {
    let store = Store::open(data_dir)?;
    match command {
        CaseCommand::Build { matter, output } => procurement_cmd::case_build(&store, &matter, &output),
    }
}

fn cora_command(data_dir: &Path, command: CoraCommand) -> Result<()> {
    let store = Store::open(data_dir)?;
    match command {
        CoraCommand::Draft { matter } => procurement_cmd::cora_draft(&store, &matter),
    }
}

fn load_config() -> Result<StateConfig> {
    let bytes = fs::read(CONFIG_PATH).with_context(|| format!("read {CONFIG_PATH}"))?;
    toml::from_slice(&bytes).with_context(|| format!("parse {CONFIG_PATH}"))
}

fn source_list(config: &StateConfig) {
    println!(
        "{} ({}) — {} | feed: {} | dry-run default: {}",
        config.state_name,
        config.state_code,
        config.jurisdiction,
        config.x_feed_label,
        config.default_dry_run
    );
    for source in &config.sources {
        println!(
            "{}\n  {}\n  API: {}\n  source type: {}\n  documentation: {}\n  official discovery: {}\n  minimum interval: {} seconds\n  robots: {}\n  terms: {}",
            source.id,
            source.name,
            source.url,
            source.source_type,
            source.documented_at,
            source.official_discovery_url,
            source.minimum_interval_seconds,
            source.robots_status,
            source.terms_note
        );
    }
}

fn source_config_digest(source: &SourceConfig) -> String {
    sha256_hex(
        format!(
            "{}\0{}\0{}\0{}\0{}",
            source.id,
            source.url,
            source.name,
            source.minimum_interval_seconds,
            source.maximum_bytes
        )
        .as_bytes(),
    )
}

#[allow(clippy::too_many_lines)]
fn source_review_command(
    data_dir: &Path,
    config: &StateConfig,
    command: SourceReviewCommand,
) -> Result<()> {
    let store = Store::open(data_dir)?;
    match command {
        SourceReviewCommand::Capture { source_id } => {
            let source = config
                .sources
                .iter()
                .find(|source| source.id == source_id)
                .ok_or_else(|| anyhow!("unknown source: {source_id}"))?;
            // Robots snapshot capture uses the provenance-aware fetch path.
            let timestamp = OffsetDateTime::now_utc().format(&Rfc3339)?;
            let robots_url = format!("https://{}/robots.txt", reviewed_host(source));
            let reviewed_hosts = vec![reviewed_host(source).clone()];
            let (body, observations) = fetch_source(
                &store,
                Some(&source_id),
                &reviewed_hosts,
                &robots_url,
                &timestamp,
                source.maximum_bytes,
                None,
            )?;
            let snapshot_digest = sha256_hex(&body);
            println!(
                "captured robots snapshot from {} ({}, {} observations, {} bytes)",
                robots_url,
                snapshot_digest,
                observations.len(),
                body.len()
            );
            // Persist the snapshot as local evidence bytes via a dedicated blob.
            let blob_path = store
                .content_path(&snapshot_digest)
                .map_err(|error| anyhow!(error.to_string()))?;
            if let Some(parent) = blob_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&blob_path, &body)?;
            println!(
                "snapshot preserved locally; run `source review record` to record the human review"
            );
            Ok(())
        }
        SourceReviewCommand::Record {
            source_id,
            reviewer,
            note,
            expires,
        } => {
            let source = config
                .sources
                .iter()
                .find(|source| source.id == source_id)
                .ok_or_else(|| anyhow!("unknown source: {source_id}"))?;
            let reviewed_at = OffsetDateTime::now_utc().format(&Rfc3339)?;
            let host = reviewed_host(source);
            let review = SourceReview {
                id: SourceReview::id_for(&source_id, &reviewed_at),
                source_id: source_id.clone(),
                source_config_digest: source_config_digest(source),
                reviewed_hosts: vec![host.clone()],
                endpoint_patterns: vec!["/api/v2/Events".to_owned(), "/Events".to_owned()],
                robots_url: format!("https://{host}/robots.txt"),
                robots_snapshot_digest: String::new(),
                robots_provenance: None,
                terms_urls: Vec::new(),
                terms_snapshot_digests: Vec::new(),
                reviewer,
                note,
                reviewed_at,
                expires_at: expires,
                minimum_interval_seconds: source.minimum_interval_seconds,
                restrictions: Vec::new(),
                supersedes: None,
            };
            let inserted = store.insert_source_review(&review)?;
            println!(
                "{} source review {} for {} (expires {})",
                if inserted { "recorded" } else { "duplicate" },
                review.id,
                source_id,
                review.expires_at
            );
            Ok(())
        }
        SourceReviewCommand::Show { source_id } => {
            let review = store
                .current_source_review(&source_id)?
                .ok_or_else(|| anyhow!("no source review recorded for {source_id}"))?;
            println!("source review {}", review.id);
            println!("  reviewer: {}", review.reviewer);
            println!("  note: {}", review.note);
            println!(
                "  reviewed: {} expires: {}",
                review.reviewed_at, review.expires_at
            );
            println!("  config digest: {}", review.source_config_digest);
            println!("  hosts: {}", review.reviewed_hosts.join(", "));
            println!("  endpoints: {}", review.endpoint_patterns.join(", "));
            println!("  min interval: {}s", review.minimum_interval_seconds);
            println!("  restrictions: {}", review.restrictions.join(", "));
            Ok(())
        }
        SourceReviewCommand::Verify { source_id } => {
            verify_source_review(&store, config, &source_id)?;
            println!("source {source_id} is reviewable and in scope");
            Ok(())
        }
    }
}

/// The reviewed host for a source, derived from its documented public API URL.
fn reviewed_host(source: &SourceConfig) -> String {
    url::Url::parse(&source.url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| "coloradosprings.legistar.com".to_owned())
}

fn verify_source_review(
    store: &Store,
    config: &StateConfig,
    source_id: &str,
) -> Result<SourceReview> {
    let source = config
        .sources
        .iter()
        .find(|source| source.id == source_id)
        .ok_or_else(|| anyhow!("unknown source: {source_id}"))?;
    let review = store.current_source_review(source_id)?.ok_or_else(|| {
        anyhow!("live retrieval refused: no source review exists for {source_id}")
    })?;
    let now = OffsetDateTime::now_utc();
    let now_ts = now.format(&Rfc3339)?;
    if review.expires_at < now_ts {
        bail!(
            "live retrieval refused: source review for {source_id} expired {}",
            review.expires_at
        );
    }
    if review.source_config_digest != source_config_digest(source) {
        bail!("live retrieval refused: source configuration changed since review for {source_id}");
    }
    let host = reviewed_host(source);
    if !review.reviewed_hosts.iter().any(|h| h == &host) {
        bail!(
            "live retrieval refused: host {host} is not within the reviewed scope for {source_id}"
        );
    }
    if !review.restrictions.is_empty() {
        bail!("live retrieval refused: a prior response announced restrictions for {source_id}");
    }
    Ok(review)
}

fn live_ingest(data_dir: &Path, config: &StateConfig, args: &IngestArgs) -> Result<()> {
    let source = config
        .sources
        .iter()
        .find(|source| source.id == args.source_id)
        .ok_or_else(|| anyhow!("unknown source: {}", args.source_id))?;
    // The ephemeral --robots-reviewed flag is no longer the primary
    // authorization; persistent human source review is required.
    if !args.robots_reviewed {
        println!("note: --robots-reviewed is deprecated; persistent source review is required");
    }
    let store = Store::open(data_dir)?;
    let review = verify_source_review(&store, config, &args.source_id)?;
    let now = OffsetDateTime::now_utc();
    if !store.source_fetch_allowed(
        &source.id,
        source.minimum_interval_seconds,
        now.unix_timestamp(),
    )? {
        bail!(
            "rate limit: source {} was fetched within the configured {}-second interval",
            source.id,
            source.minimum_interval_seconds
        );
    }
    let timestamp = now.format(&Rfc3339)?;
    let reviewed_hosts = review.reviewed_hosts.clone();
    let budgets = Tracker::new(Budgets::defaults());
    let mut context = ExtractContext {
        sandbox: RealSandbox::new(pnull_ingest::ExtractionSandboxConfig::defaults())
            .map_err(|error| anyhow!(error.to_string()))?,
        budgets,
        build: BuildMetadata::local(),
    };

    let pagination = LegistarPagination {
        page_size: args.page_size,
        max_pages: args.max_pages,
        ..LegistarPagination::default()
    };
    let source_url = source.url.clone();
    let max_bytes = source.maximum_bytes;
    let outcome = pagination.paginate_events(
        &mut context.budgets,
        |offset| {
            let page_url = append_pagination(&source_url, args.page_size, offset);
            let prior = latest_prior(&store, &source.id)?;
            let (body, observations) = fetch_source(
                &store,
                Some(&source.id),
                &reviewed_hosts,
                &page_url,
                &timestamp,
                max_bytes,
                prior.as_ref(),
            )?;
            println!(
                "  fetched page offset {offset}: {} observations",
                observations.len()
            );
            Ok(body)
        },
        &source.id,
        &source_url,
    )?;
    store.record_source_fetch(&source.id, now.unix_timestamp())?;
    println!(
        "paginated {} events across {} pages (stopped: {})",
        outcome.events.len(),
        outcome.pages_fetched,
        outcome.stopped_reason
    );
    for matter in &outcome.matters {
        store.insert_matter(matter)?;
    }
    for attachment in &outcome.attachments {
        store.insert_attachment(attachment)?;
    }
    println!("  matters: {}", outcome.matters.len());
    println!("  attachments: {}", outcome.attachments.len());
    Ok(())
}

fn append_pagination(base: &str, page_size: u32, offset: u32) -> String {
    let mut url =
        url::Url::parse(base).unwrap_or_else(|_| url::Url::parse("https://invalid/").unwrap());
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    pairs.retain(|(k, _)| k != "$top" && k != "$skip");
    pairs.push(("$top".to_owned(), page_size.to_string()));
    pairs.push(("$skip".to_owned(), offset.to_string()));
    let query: Vec<String> = pairs
        .iter()
        .map(|(k, v)| {
            format!(
                "{}={}",
                url::form_urlencoded::byte_serialize(k.as_bytes()).collect::<String>(),
                url::form_urlencoded::byte_serialize(v.as_bytes()).collect::<String>()
            )
        })
        .collect();
    url.set_query(Some(&query.join("&")));
    url.to_string()
}

fn latest_prior(
    store: &Store,
    source_id: &str,
) -> Result<Option<PriorEvidence>, pnull_ingest::IngestError> {
    let observations = store.fetch_observations(source_id)?;
    let last = observations.iter().rev().find(|o| o.body_digest.is_some());
    Ok(last.map(|o| PriorEvidence {
        evidence_id: o.body_digest.clone().unwrap_or_default(),
        etag: o.etag.clone(),
        last_modified: o.last_modified.clone(),
    }))
}

fn scan_all(data_dir: &Path) -> Result<()> {
    let store = Store::open(data_dir)?;
    let rules = load_rules(RULES_PATH)?;
    let mut count = 0;
    for (mut record, text) in store.all_evidence()? {
        if let Some(finding) = scan(&record, &text, &rules) {
            record
                .matched_rule_ids
                .clone_from(&finding.matched_rule_ids);
            record.quoted_source_spans.clone_from(&finding.citations);
            store.update_evidence_annotations(&record, &text)?;
            if store.insert_finding(&finding)? {
                count += 1;
            }
            if let Some(alert) = build_alert(&record, &finding, None) {
                store.insert_alert(&alert)?;
            }
            println!(
                "{} — {} — {}",
                finding.id,
                finding.state.label(),
                finding.classification_reason
            );
        }
    }
    println!("{count} new deterministic finding(s)");
    Ok(())
}

fn print_diff(data_dir: &Path, old_id: Option<&str>, new_id: Option<&str>) -> Result<()> {
    let store = Store::open(data_dir)?;
    let rules = load_rules(RULES_PATH)?;
    let (old_record, old_text, new_record, new_text) = match (old_id, new_id) {
        (Some(old), Some(new)) => {
            let (old_record, old_text) = store.evidence(old)?;
            let (new_record, new_text) = store.evidence(new)?;
            (old_record, old_text, new_record, new_text)
        }
        (None, None) => {
            let evidence = store.all_evidence()?;
            let (new_record, new_text) = evidence
                .iter()
                .find(|(record, _)| record.supersedes.is_some())
                .cloned()
                .ok_or_else(|| anyhow!("no superseding evidence pair is stored"))?;
            let old_id = new_record.supersedes.as_deref().expect("checked above");
            let (old_record, old_text) = store.evidence(old_id)?;
            (old_record, old_text, new_record, new_text)
        }
        _ => bail!("provide both evidence identifiers or neither"),
    };
    let diff = compare(&old_record, &old_text, &new_record, &new_text, &rules);
    if diff.changes.is_empty() {
        println!("No configured meaningful changes detected.");
    } else {
        for change in &diff.changes {
            println!("{}: {}", change.kind, change.summary);
        }
    }
    println!("\n{}", diff.unified_text);
    Ok(())
}

fn build_public_site(data_dir: &Path, config: &StateConfig, output: &Path) -> Result<()> {
    let store = Store::open(data_dir)?;
    let rules_yaml = fs::read_to_string(RULES_PATH)?;
    // build_site now gates publication on approved citations and allowlists.
    let files = build_site(
        &store,
        output,
        &SiteConfig {
            canonical_base_url: &config.canonical_base_url,
            rules_yaml: &rules_yaml,
        },
    )
    .map_err(|error| anyhow!("publication refused: {error}"))?;
    println!("wrote {} static files to {}", files.len(), output.display());
    Ok(())
}

fn list_alerts(data_dir: &Path) -> Result<()> {
    let store = Store::open(data_dir)?;
    for alert in store.alerts()? {
        println!(
            "{} | {} | {} | {}",
            alert.id,
            alert.publication_date,
            alert.state.label(),
            alert.title
        );
    }
    Ok(())
}

fn matter_command(data_dir: &Path, command: MatterCommand) -> Result<()> {
    let store = Store::open(data_dir)?;
    match command {
        MatterCommand::List => {
            let matters = store.matters()?;
            for matter in &matters {
                let attachments = store.attachments(&matter.id)?.len();
                println!(
                    "{} | {} | {} | {} | {} attachments",
                    matter.id,
                    matter.official_matter_id,
                    matter.document_role.clone().label(),
                    matter.title,
                    attachments
                );
            }
            println!("{} matter(s)", matters.len());
        }
        MatterCommand::Show { matter_id } => {
            let matter = store.matter(&matter_id)?;
            println!("matter {} ({})", matter.id, matter.official_matter_id);
            println!("  title: {}", matter.title);
            println!("  status: {}", matter.status);
            println!("  role: {}", matter.document_role.clone().label());
            println!("  url: {}", matter.url);
            let subjects = store.subjects(&matter_id)?;
            let actions = store.actions(&matter_id)?;
            for subject in &subjects {
                println!(
                    "  subject {} [{}] known={} :: {}",
                    subject.name,
                    subject.kind.clone().label(),
                    subject.known,
                    subject.detail
                );
            }
            for action in &actions {
                let subject = subjects.iter().find(|s| s.id == action.subject_id);
                let name = subject.map_or("?", |s| s.name.as_str());
                println!(
                    "  action: {} -> {} [{}] known={} :: {}",
                    action.kind.clone().label(),
                    name,
                    action.subject_id,
                    action.known,
                    action.summary
                );
            }
        }
        MatterCommand::Attachments { matter_id } => {
            let attachments = store.attachments(&matter_id)?;
            for attachment in &attachments {
                println!(
                    "{} | {} | {} | {} | evidence: {}",
                    attachment.id,
                    attachment.official_id,
                    attachment.name,
                    attachment.url,
                    attachment.evidence_id.as_deref().unwrap_or("none")
                );
            }
            println!("{} attachment(s)", attachments.len());
        }
    }
    Ok(())
}

/// Locate a line citation by its review citation id across all alerts.
fn find_line_citation(store: &Store, id: &str) -> Result<Option<Citation>> {
    for alert in store.alerts()? {
        for citation in &alert.citations {
            if citation_id(citation) == id {
                return Ok(Some(citation.clone()));
            }
        }
    }
    Ok(None)
}

fn citation_command(data_dir: &Path, command: CitationCommand) -> Result<()> {
    let store = Store::open(data_dir)?;
    match command {
        CitationCommand::Show { citation_id } => {
            let citation = store.page_citation(&citation_id)?;
            println!("page citation {}", citation.id);
            println!("  evidence: {}", citation.evidence_id);
            println!("  page: {}", citation.page_number);
            println!("  quote: {}", citation.quote);
            println!("  quote digest: {}", citation.quote_digest);
            println!("  text-map digest: {}", citation.text_map_digest);
            println!("  evidence digest: {}", citation.evidence_digest);
            println!(
                "  normalized range: {}-{}",
                citation.normalized_range.start, citation.normalized_range.end
            );
            for (index, rect) in citation.rects.iter().enumerate() {
                println!(
                    "  rect {}: x[{:.2},{:.2}] y[{:.2},{:.2}]",
                    index + 1,
                    rect.x_min,
                    rect.x_max,
                    rect.y_min,
                    rect.y_max
                );
            }
            println!("  ocr confidence: {:?}", citation.ocr_confidence);
        }
        CitationCommand::Render {
            citation_id,
            output,
        } => {
            let citation = store.page_citation(&citation_id)?;
            let (record, _) = store.evidence(&citation.evidence_id)?;
            let pdf_path = store
                .content_path(&record.sha256)
                .map_err(|e| anyhow!(e.to_string()))?;
            if !pdf_path.exists() {
                bail!(
                    "preserved PDF bytes not present locally for {}",
                    citation.evidence_id
                );
            }
            let map = store.text_map(&citation.text_map_digest).ok();
            let (page_width, page_height) = map
                .as_ref()
                .map_or((612.0, 792.0), |m| (m.page_width, m.page_height));
            render_review_image(
                &pdf_path,
                citation.page_number,
                &citation.rects,
                page_width,
                page_height,
                &output,
                150,
            )
            .map_err(|error| anyhow!(error.to_string()))?;
            println!("wrote reviewer image to {}", output.display());
        }
    }
    Ok(())
}

fn review_command(data_dir: &Path, command: ReviewCommand) -> Result<()> {
    let store = Store::open(data_dir)?;
    match command {
        ReviewCommand::List => {
            let mut seen: BTreeSet<String> = BTreeSet::new();
            for alert in store.alerts()? {
                for citation in &alert.citations {
                    let id = citation_id(citation);
                    let decision = store.current_review(&id)?;
                    let state = decision.map_or("PENDING", |d| d.state.label());
                    println!("{} | {} | {}", id, state, citation.quote);
                    seen.insert(id);
                }
            }
            // Also list page citations that lack an approved review.
            for record in store.all_evidence()? {
                for citation in store.page_citations(&record.0.id)? {
                    let decision = store.current_review(&citation.id)?;
                    let state = decision.map_or("PENDING", |d| d.state.label());
                    println!(
                        "{} | {} | page {}: {}",
                        citation.id, state, citation.page_number, citation.quote
                    );
                }
            }
            println!("{} line citation(s) listed", seen.len());
        }
        ReviewCommand::Show { citation_id } => {
            let decisions = store.reviews_for_citation(&citation_id)?;
            if decisions.is_empty() {
                println!("no review decisions for {citation_id}");
            } else {
                for decision in &decisions {
                    println!(
                        "{} | {} | reviewer {} | note: {} | decided {} | supersedes {}",
                        decision.id,
                        decision.state.label(),
                        decision.reviewer,
                        decision.note,
                        decision.decided_at,
                        decision.supersedes.as_deref().unwrap_or("none")
                    );
                }
            }
        }
        ReviewCommand::Approve {
            citation_id,
            reviewer,
            note,
        } => record_review(
            &store,
            &citation_id,
            ReviewState::Approved,
            &reviewer,
            &note,
        )?,
        ReviewCommand::Reject {
            citation_id,
            reviewer,
            reason,
        } => record_review(
            &store,
            &citation_id,
            ReviewState::Rejected,
            &reviewer,
            &reason,
        )?,
        ReviewCommand::Supersede {
            decision_id,
            reviewer,
            reason,
        } => {
            // Find the decision and mark it superseded with a follow-up decision.
            let mut found = false;
            for decision in store.all_reviews()? {
                if decision.id == decision_id {
                    let decided_at = OffsetDateTime::now_utc().format(&Rfc3339)?;
                    let superseded = ReviewDecision {
                        id: ReviewDecision::id_for(&decision.citation_id, &decided_at),
                        citation_id: decision.citation_id.clone(),
                        state: ReviewState::Superseded,
                        reviewer,
                        note: reason,
                        bound_digest: decision.bound_digest.clone(),
                        decision_digest: decision.decision_digest.clone(),
                        decided_at,
                        supersedes: Some(decision.id.clone()),
                    };
                    store.insert_review(&superseded)?;
                    println!("superseded decision {decision_id} with {}", superseded.id);
                    found = true;
                    break;
                }
            }
            if !found {
                bail!("no review decision with id {decision_id}");
            }
        }
    }
    Ok(())
}

/// Compute a review binding for either a line citation or a page citation and
/// store a review decision.
fn record_review(
    store: &Store,
    citation_id_value: &str,
    state: ReviewState,
    reviewer: &str,
    note: &str,
) -> Result<()> {
    let binding = if let Some(citation) = find_line_citation(store, citation_id_value)? {
        citation_review_binding(&citation)
    } else if let Ok(page) = store.page_citation(citation_id_value) {
        ReviewBinding {
            evidence_id: page.evidence_id,
            source_digest: page.evidence_digest,
            locator_or_geometry: format!("page {}", page.page_number),
            quote: page.quote,
            quote_digest: page.quote_digest,
            rule_digest: String::new(),
            processing_artifact_digest: page.text_map_digest,
            proposed_public_fields: "quote,locator,geometry".to_owned(),
        }
    } else {
        bail!("citation {citation_id_value} not found among line or page citations");
    };
    let bound_digest = binding.digest();
    let decided_at = OffsetDateTime::now_utc().format(&Rfc3339)?;
    let decision = ReviewDecision {
        id: ReviewDecision::id_for(citation_id_value, &decided_at),
        citation_id: citation_id_value.to_owned(),
        state,
        reviewer: reviewer.to_owned(),
        note: note.to_owned(),
        bound_digest,
        decision_digest: sha256_hex(
            format!("{citation_id_value}\0{}\0{}", state.label(), decided_at).as_bytes(),
        ),
        decided_at,
        supersedes: None,
    };
    let inserted = store.insert_review(&decision)?;
    println!(
        "{} review {} for citation {citation_id_value} ({})",
        if inserted { "recorded" } else { "duplicate" },
        decision.id,
        state.label()
    );
    Ok(())
}

fn x_command(data_dir: &Path, config: &StateConfig, command: XCommand) -> Result<()> {
    let store = Store::open(data_dir)?;
    match command {
        XCommand::Draft { alert_id } => {
            let alert = store.alert(&alert_id)?;
            // X drafting fails closed if the alert's citations are not approved.
            assert_citations_approved(&store, &alert.citations)
                .map_err(|error| anyhow!("draft refused: {error}"))?;
            let generated = draft(&alert, &config.canonical_base_url)?;
            print_draft(&generated);
            println!("DRY RUN: no network transport was created.");
        }
        XCommand::Approve { alert_id } => {
            let alert = store.alert(&alert_id)?;
            assert_citations_approved(&store, &alert.citations)
                .map_err(|error| anyhow!("approval refused: {error}"))?;
            let generated = draft(&alert, &config.canonical_base_url)?;
            let timestamp = OffsetDateTime::now_utc().format(&Rfc3339)?;
            let inserted = store.approve(&alert_id, &generated.digest(), &timestamp)?;
            println!(
                "{} {}",
                if inserted {
                    "approved"
                } else {
                    "already approved"
                },
                alert_id
            );
        }
        XCommand::Post { alert_id, confirm } => {
            if !confirm {
                bail!("live posting requires --confirm");
            }
            let canonical = url::Url::parse(&config.canonical_base_url)?;
            if canonical
                .host_str()
                .is_none_or(|host| host.ends_with(".invalid"))
            {
                bail!(
                    "live posting refused: configure a real public canonical_base_url before approval and posting"
                );
            }
            let alert = store.alert(&alert_id)?;
            assert_citations_approved(&store, &alert.citations)
                .map_err(|error| anyhow!("posting refused: {error}"))?;
            let generated = draft(&alert, &config.canonical_base_url)?;
            let credentials = pnull_x::Credentials::from_runtime()?;
            let mut transport = pnull_x::ReqwestXTransport::new(credentials)?;
            let timestamp = OffsetDateTime::now_utc().format(&Rfc3339)?;
            let ids = post_approved(&store, &generated, true, &timestamp, &mut transport)?;
            println!("posted {} item(s) for {}", ids.len(), alert_id);
        }
        XCommand::Attempts => {
            let attempts = store.x_attempts()?;
            for attempt in &attempts {
                println!(
                    "{} | alert {} | {} | {} segments",
                    attempt.id,
                    attempt.alert_id,
                    attempt.status,
                    attempt.segments.len()
                );
            }
            println!("{} attempt(s)", attempts.len());
        }
        XCommand::Status { alert_id } => {
            let attempts = attempts_for_alert(&store, &alert_id)?;
            if attempts.is_empty() {
                println!("no posting attempts recorded for {alert_id}");
            } else {
                for attempt in &attempts {
                    println!("{}", attempt_summary(attempt));
                }
            }
        }
        XCommand::Reconcile {
            attempt_id,
            decision,
            operator,
            note,
            remote_id,
        } => {
            let decided_at = OffsetDateTime::now_utc().format(&Rfc3339)?;
            let item = reconcile(
                &store,
                &attempt_id,
                &decision,
                remote_id.as_deref(),
                &operator,
                &note,
                &decided_at,
            )
            .map_err(|error| anyhow!(error.to_string()))?;
            println!(
                "recorded reconciliation {} ({}: {}) for attempt {attempt_id}",
                item.id, item.decision, item.note
            );
        }
    }
    Ok(())
}

fn attempt_summary(attempt: &pnull_core::XAttempt) -> String {
    let segments: Vec<String> = attempt
        .segments
        .iter()
        .map(|segment| format!("#{}={}", segment.index, segment.state))
        .collect();
    format!(
        "{} | alert {} | {} | segments [{}]",
        attempt.id,
        attempt.alert_id,
        attempt.status,
        segments.join(", ")
    )
}

fn verify(data_dir: &Path, evidence_id: &str) -> Result<()> {
    let store = Store::open(data_dir)?;
    store.verify(evidence_id)?;
    let (record, _) = store.evidence(evidence_id)?;
    println!("verified {} sha256:{}", evidence_id, record.sha256);
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn run_demo(output: &Path, config: &StateConfig) -> Result<()> {
    let (store, site_dir) = prepare_demo_output(output)?;
    let rules = load_rules(RULES_PATH)?;
    let mut context = ExtractContext {
        sandbox: RealSandbox::new(pnull_ingest::ExtractionSandboxConfig::defaults())
            .map_err(|error| anyhow!(error.to_string()))?,
        budgets: Tracker::new(Budgets::defaults()),
        build: BuildMetadata::local(),
    };

    // Seed deterministic demonstration reviews and a source review.
    seed_demo_reviews(&store)?;
    println!("0. Seeded deterministic demonstration reviews and source review.");

    let draft_doc = demo_document(config, "ordinance-draft")?;
    let draft_outcome = ingest_demo_document(&store, config, &mut context, draft_doc, None)?;
    println!("1. Ingested original draft: {}", draft_outcome.record.id);
    store.verify(&draft_outcome.record.id)?;
    println!("2. Verified SHA-256: {}", draft_outcome.record.sha256);

    let support_doc = demo_document(config, "supporting-presentation")?;
    let mut support = ingest_demo_document(&store, config, &mut context, support_doc, None)?;
    let support_finding = scan(&support.record, &support.extracted_text, &rules)
        .ok_or_else(|| anyhow!("supporting official presentation produced no taxonomy finding"))?;
    support
        .record
        .matched_rule_ids
        .clone_from(&support_finding.matched_rule_ids);
    support
        .record
        .quoted_source_spans
        .clone_from(&support_finding.citations);
    store.update_evidence_annotations(&support.record, &support.extracted_text)?;
    println!(
        "3. Detected surveillance terms {} at {}",
        support_finding.matched_rule_ids.join(", "),
        support_finding
            .citations
            .iter()
            .map(|citation| citation.locator.label.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );

    let signed_doc = demo_document(config, "ordinance-signed")?;
    let mut signed = ingest_demo_document(
        &store,
        config,
        &mut context,
        signed_doc,
        Some(draft_outcome.record.id.clone()),
    )?;
    println!("4. Ingested signed version: {}", signed.record.id);
    let diff = compare(
        &draft_outcome.record,
        &draft_outcome.extracted_text,
        &signed.record,
        &signed.extracted_text,
        &rules,
    );
    if diff.changes.is_empty() {
        bail!("demo expected a meaningful ordinance change");
    }
    println!("5. Meaningful differences:");
    for change in &diff.changes {
        println!("   {} — {}", change.kind, change.summary);
    }

    // Page-accurate citation geometry on the official PDF.
    let quote = "POLICE DEPARTMENT TECHNOLOGY SURCHARGE";
    let page_citation = cite_quote(&store, &signed.record.id, quote, 0)
        .map_err(|error| anyhow!(error.to_string()))?;
    store.insert_page_citation(&page_citation)?;
    println!(
        "5b. Built page citation {} (page {}, {} rects)",
        page_citation.id,
        page_citation.page_number,
        page_citation.rects.len()
    );

    // Explicit subjects and actions (the ordinance approval is the dispositive action).
    let matter_id = "co:25-581";
    let (subjects, actions) = extract_matter_subjects_and_actions(
        matter_id,
        &signed.record,
        &signed.extracted_text,
        &rules,
    );
    store.insert_matter(&pnull_core::Matter {
        id: matter_id.to_owned(),
        source_id: "colorado-springs-legistar-events".to_owned(),
        official_matter_id: "25-581".to_owned(),
        title: signed.record.document_title.clone(),
        status: "finally passed".to_owned(),
        url: signed.record.source_url.clone(),
        document_role: document_role(&signed.record, &signed.extracted_text),
    })?;
    for subject in &subjects {
        store.insert_subject(subject)?;
    }
    for action in &actions {
        store.insert_action(action)?;
    }
    println!(
        "5c. Modeled {} subject(s) and {} action(s); ordinance approval is separate from any vendor mention",
        subjects.len(),
        actions.len()
    );

    // Second genuine matter: Ordinance 15-84 (2015) established the surcharge
    // that funds surveillance technology. The matter record identifies the
    // subject; the event record carries the explicit "finally passed" action.
    // The Axon/Flock surveillance-technology link is established by the preserved
    // 2025 presentation (supporting evidence), not by the 2015 action itself.
    let second_matter_doc = demo_document(config, "matter-ordinance-15-84")?;
    let second_matter_ev =
        ingest_demo_document(&store, config, &mut context, second_matter_doc, None)?;
    let second_event_doc = demo_document(config, "event-ordinance-15-84-final-vote")?;
    let second_event = ingest_demo_document(&store, config, &mut context, second_event_doc, None)?;
    let second_matter_id = "co:15-00663";
    let (second_subjects, second_actions) = extract_matter_subjects_and_actions(
        second_matter_id,
        &second_event.record,
        &second_event.extracted_text,
        &rules,
    );
    store.insert_matter(&pnull_core::Matter {
        id: second_matter_id.to_owned(),
        source_id: "colorado-springs-legistar-events".to_owned(),
        official_matter_id: "15-00663".to_owned(),
        title: second_matter_ev.record.document_title.clone(),
        status: "finally passed".to_owned(),
        url: second_matter_ev.record.source_url.clone(),
        document_role: document_role(&second_matter_ev.record, &second_matter_ev.extracted_text),
    })?;
    for subject in &second_subjects {
        store.insert_subject(subject)?;
    }
    for action in &second_actions {
        store.insert_action(action)?;
    }
    println!(
        "5e. Modeled second genuine matter (Ordinance 15-84, 2015): {} subject(s), {} action(s); the surcharge (object) funds Axon/Flock surveillance technology per the preserved presentation (supporting evidence)",
        second_subjects.len(),
        second_actions.len()
    );

    let alert = persist_demo_alert(&store, config, &support_finding, &mut signed, diff)?;

    // Approve the alert's line citations (demonstration review).
    approve_alert_citations_demo(&store, &alert)?;
    println!("5d. Recorded demonstration approvals for the alert's citations.");

    let rules_yaml = fs::read_to_string(RULES_PATH)?;
    let files = build_site(
        &store,
        &site_dir,
        &SiteConfig {
            canonical_base_url: &config.canonical_base_url,
            rules_yaml: &rules_yaml,
        },
    )
    .map_err(|error| anyhow!("publication refused: {error}"))?;
    println!(
        "6. Generated {} static site files at {}",
        files.len(),
        site_dir.display()
    );
    let generated = draft(&alert, &config.canonical_base_url)?;
    println!("7. Generated local X draft (dry run, no transport):");
    print_draft(&generated);

    let network_posts = 0_u8;
    fs::write(
        output.join("network-posts.txt"),
        format!("{network_posts}\n"),
    )?;
    println!("8. Network posts performed: {network_posts}. No X transport was constructed.");
    println!("Alert ID: {}", alert.id);
    Ok(())
}

/// Seed deterministic demonstration reviews (clearly labeled) and a source
/// review so the offline demo can exercise the fail-closed publication gates.
fn seed_demo_reviews(store: &Store) -> Result<()> {
    // Publication allowlist for page citations and subject/action rendering.
    let allowlist = PublicationAllowlist {
        id: "allowlist:demo".to_owned(),
        field_categories: vec![
            "page_citation".to_owned(),
            "subject_action".to_owned(),
            "evidence_metadata".to_owned(),
        ],
        created_at: "2026-08-16T00:00:00Z".to_owned(),
        note: "Demonstration allowlist for the offline v0.0.2 demo.".to_owned(),
    };
    store.insert_publication_allowlist(&allowlist)?;

    // A persistent source review (demonstration, fixed timestamp).
    let review = SourceReview {
        id: "source-review:demo:co".to_owned(),
        source_id: "colorado-springs-legistar-events".to_owned(),
        source_config_digest: "demo-source-config".to_owned(),
        reviewed_hosts: vec![
            "webapi.legistar.com".to_owned(),
            "coloradosprings.legistar.com".to_owned(),
        ],
        endpoint_patterns: vec!["/api/v2/Events".to_owned()],
        robots_url: "https://coloradosprings.legistar.com/robots.txt".to_owned(),
        robots_snapshot_digest: "demo-robots-snapshot".to_owned(),
        robots_provenance: Some("demonstration snapshot, offline".to_owned()),
        terms_urls: Vec::new(),
        terms_snapshot_digests: Vec::new(),
        reviewer: "demo-operator".to_owned(),
        note: "Demonstration source review for the offline v0.0.2 demo.".to_owned(),
        reviewed_at: "2026-08-16T00:00:00Z".to_owned(),
        expires_at: "2027-08-16T00:00:00Z".to_owned(),
        minimum_interval_seconds: 86400,
        restrictions: Vec::new(),
        supersedes: None,
    };
    store.insert_source_review(&review)?;
    Ok(())
}

fn approve_alert_citations_demo(store: &Store, alert: &Alert) -> Result<()> {
    for citation in &alert.citations {
        let id = citation_id(citation);
        if store.current_review(&id)?.is_none() {
            let binding = citation_review_binding(citation);
            let decided_at = "2026-08-16T00:00:00Z".to_owned();
            let decision = ReviewDecision {
                id: ReviewDecision::id_for(&id, &decided_at),
                citation_id: id.clone(),
                state: ReviewState::Approved,
                reviewer: "demo-operator".to_owned(),
                note: "Demonstration approval for the offline v0.0.2 demo.".to_owned(),
                bound_digest: binding.digest(),
                decision_digest: sha256_hex(format!("{id}\0Approved\0{decided_at}").as_bytes()),
                decided_at,
                supersedes: None,
            };
            store.insert_review(&decision)?;
        }
    }
    // Also approve the page citation so it can be published.
    for record in store.all_evidence()? {
        for citation in store.page_citations(&record.0.id)? {
            if store.current_review(&citation.id)?.is_none() {
                let binding = ReviewBinding {
                    evidence_id: citation.evidence_id,
                    source_digest: citation.evidence_digest,
                    locator_or_geometry: format!("page {}", citation.page_number),
                    quote: citation.quote.clone(),
                    quote_digest: citation.quote_digest,
                    rule_digest: String::new(),
                    processing_artifact_digest: citation.text_map_digest.clone(),
                    proposed_public_fields: "quote,locator,geometry".to_owned(),
                };
                let decided_at = "2026-08-16T00:00:00Z".to_owned();
                let decision = ReviewDecision {
                    id: ReviewDecision::id_for(&citation.id, &decided_at),
                    citation_id: citation.id.clone(),
                    state: ReviewState::Approved,
                    reviewer: "demo-operator".to_owned(),
                    note: "Demonstration approval for the offline v0.0.2 demo.".to_owned(),
                    bound_digest: binding.digest(),
                    decision_digest: sha256_hex(
                        format!("{}\0Approved\0{decided_at}", citation.id).as_bytes(),
                    ),
                    decided_at,
                    supersedes: None,
                };
                store.insert_review(&decision)?;
            }
        }
    }
    Ok(())
}

fn persist_demo_alert(
    store: &Store,
    config: &StateConfig,
    support_finding: &Finding,
    signed: &mut pnull_ingest::IngestOutcome,
    diff: EvidenceDiff,
) -> Result<Alert> {
    let (state, reason, state_citation) = classify_document(&signed.record, &signed.extracted_text);
    let mut citations = support_finding.citations.clone();
    if let Some(citation) = state_citation {
        citations.push(citation);
    }
    let finding = Finding {
        id: stable_id(
            "finding",
            &[
                &signed.record.id,
                state.label(),
                &support_finding.matched_rule_ids.join(","),
                &support_finding.rules_digest,
            ],
        ),
        evidence_id: signed.record.id.clone(),
        jurisdiction: config.jurisdiction.clone(),
        state,
        classification_reason: format!(
            "{reason} Surveillance relevance is established separately by the linked official presentation citations."
        ),
        rules_version: support_finding.rules_version,
        rules_digest: support_finding.rules_digest.clone(),
        matched_rule_ids: support_finding.matched_rule_ids.clone(),
        citations,
    };
    signed
        .record
        .matched_rule_ids
        .clone_from(&finding.matched_rule_ids);
    signed
        .record
        .quoted_source_spans
        .clone_from(&finding.citations);
    store.update_evidence_annotations(&signed.record, &signed.extracted_text)?;
    store.insert_finding(&finding)?;
    let alert = build_alert(&signed.record, &finding, Some(diff))
        .ok_or_else(|| anyhow!("citation-constrained alert construction failed"))?;
    store.insert_alert(&alert)?;
    Ok(alert)
}

fn prepare_demo_output(output: &Path) -> Result<(Store, PathBuf)> {
    if output.exists() {
        fs::remove_dir_all(output)?;
    }
    let store = Store::open(output.join("state"))?;
    Ok((store, output.join("site")))
}

fn ingest_demo_document(
    store: &Store,
    config: &StateConfig,
    context: &mut ExtractContext,
    document: &DemoDocument,
    supersedes: Option<String>,
) -> Result<pnull_ingest::IngestOutcome> {
    let bytes = fs::read(&document.path)
        .with_context(|| format!("read fixture {}", document.path.display()))?;
    let observed_digest = sha256_hex(&bytes);
    if observed_digest != document.sha256 {
        bail!(
            "fixture digest mismatch for {}: expected {}, observed {}",
            document.id,
            document.sha256,
            observed_digest
        );
    }
    let original_filename = document
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("invalid fixture filename"))?
        .to_owned();
    ingest_bytes(
        store,
        &context.sandbox,
        &mut context.budgets,
        &context.build,
        &IngestRequest {
            jurisdiction: config.jurisdiction.clone(),
            source_url: document.url.clone(),
            source_type: parse_source_type(&document.source_type)?,
            document_title: document.title.clone(),
            publication_date: Some(document.publication_date.clone()),
            retrieval_timestamp: document.retrieval_timestamp.clone(),
            mime_type: document.mime_type.clone(),
            original_filename,
            supersedes,
            enable_ocr: false,
            ocr_language: "eng".to_owned(),
            max_bytes: DEFAULT_MAX_BYTES,
        },
        &bytes,
    )
    .map_err(Into::into)
}

fn demo_document<'a>(config: &'a StateConfig, id: &str) -> Result<&'a DemoDocument> {
    config
        .demo_documents
        .iter()
        .find(|document| document.id == id)
        .ok_or_else(|| anyhow!("missing demo document configuration: {id}"))
}

fn parse_source_type(value: &str) -> Result<SourceType> {
    match value {
        "official_api" => Ok(SourceType::OfficialApi),
        "agenda" => Ok(SourceType::Agenda),
        "contract" => Ok(SourceType::Contract),
        "amendment" => Ok(SourceType::Amendment),
        "html_page" => Ok(SourceType::HtmlPage),
        "plain_text" => Ok(SourceType::PlainText),
        "pdf" => Ok(SourceType::Pdf),
        _ => bail!("unsupported source type: {value}"),
    }
}

fn print_draft(generated: &pnull_x::Draft) {
    for (index, post) in generated.posts.iter().enumerate() {
        println!(
            "--- post {}/{} ({} characters) ---\n{}",
            index + 1,
            generated.posts.len(),
            post.chars().count(),
            post
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use tempfile::tempdir;

    #[test]
    fn offline_demo_is_reproducible_and_never_posts() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        std::env::set_current_dir(&root).expect("workspace root");
        let config = load_config().expect("config");
        let directory = tempdir().expect("temporary directory");
        let first = directory.path().join("first");
        let second = directory.path().join("second");
        run_demo(&first, &config).expect("first demo");
        run_demo(&second, &config).expect("second demo");
        assert_eq!(
            fs::read(first.join("network-posts.txt")).expect("proof"),
            b"0\n"
        );
        assert_eq!(
            snapshot(&first.join("site")),
            snapshot(&second.join("site"))
        );
        assert_eq!(
            snapshot(&first.join("state/records")),
            snapshot(&second.join("state/records"))
        );
        // No script element may occur in public output.
        for (path, bytes) in snapshot(&first.join("site")) {
            let content = String::from_utf8_lossy(&bytes);
            assert!(
                !content.contains("<script"),
                "public file {} contains <script",
                path.display()
            );
        }
    }

    fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut files = BTreeMap::new();
        collect(root, root, &mut files);
        files
    }

    fn collect(root: &Path, current: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        let mut entries: Vec<_> = fs::read_dir(current)
            .expect("read directory")
            .map(|entry| entry.expect("entry"))
            .collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                collect(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root).expect("relative path").to_owned(),
                    fs::read(path).expect("file bytes"),
                );
            }
        }
    }
}
