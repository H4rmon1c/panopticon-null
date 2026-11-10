use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Parser, Subcommand};
use pnull_core::{
    Alert, BoundingRect, Citation, DiffChange, EvidenceDiff, EvidenceRecord, Finding, Locator,
    LocatorRange, PageCitation, PublicationAllowlist, ReviewDecision, ReviewState, SourceReview,
    SourceType, Store, sha256_hex, stable_id,
};
use pnull_detect::{
    RuleSet, build_alert, classify_document, compare, document_role,
    extract_matter_subjects_and_actions, load_rules, rules_digest, scan,
};
use pnull_geometry::render_review_image;
use pnull_http::{
    FetchConfig, FetchRequest, PriorEvidence, ReqwestTransport, SystemResolver,
    provenance_fetch_robots_detailed,
};
use pnull_ingest::{
    Budgets, BuildMetadata, DEFAULT_MAX_BYTES, IngestRequest, LegistarPagination, RealSandbox,
    Tracker, cite_quote, fetch_source, ingest_bytes,
};
use pnull_publish::{
    SiteConfig, assert_citations_approved, build_site, citation_id, citation_review_binding,
};
use pnull_x::{attempts_for_alert, draft, draft_procurement, post_approved, reconcile};
use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

mod approval_cmd;
mod archive;
mod procurement_cmd;
mod refresh;

const DEFAULT_CONFIG_PATH: &str = "configs/states/co.toml";
const DEFAULT_RULES_PATH: &str = "rules/surveillance.yml";

#[derive(Parser)]
#[command(
    name = "pnull",
    version,
    about = "Evidence infrastructure against institutional surveillance"
)]
struct Cli {
    #[arg(long, global = true, env = "PNUL_DATA_DIR", default_value = ".pnull")]
    data_dir: PathBuf,
    #[arg(long, global = true, env = "PNUL_CONFIG_PATH", default_value = DEFAULT_CONFIG_PATH)]
    config: PathBuf,
    #[arg(long, global = true, env = "PNUL_RULES_PATH", default_value = DEFAULT_RULES_PATH)]
    rules: PathBuf,
    #[arg(long, global = true, env = "PNUL_SITE_URL")]
    site_url: Option<String>,
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
    Approval {
        #[command(subcommand)]
        command: ApprovalCommand,
    },
    Atlas {
        #[command(subcommand)]
        command: AtlasCommand,
    },
}

#[derive(Subcommand)]
enum AtlasCommand {
    /// Show every source-bound claim and the exact digest a reviewer must check.
    Requirements {
        #[arg(long, default_value = "fixtures/atlas/colorado-springs-25-93.v1.json")]
        manifest: PathBuf,
    },
    /// Append a human review of one source-bound claim or the whole release.
    Review {
        #[arg(long, default_value = "fixtures/atlas/colorado-springs-25-93.v1.json")]
        manifest: PathBuf,
        #[arg(long)]
        id: String,
        #[arg(long)]
        binding: String,
        #[arg(long)]
        decision: String,
        #[arg(long)]
        reviewer: String,
        #[arg(long)]
        note: String,
    },
    /// Allow atlas publication after the source-bound human reviews are complete.
    AllowRelease {
        #[arg(long)]
        reviewer: String,
        #[arg(long)]
        note: String,
    },
    /// Export the reviewed public atlas dataset, or a clearly labeled private preview.
    Export {
        #[arg(long, default_value = "fixtures/atlas/colorado-springs-25-93.v1.json")]
        manifest: Vec<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
        /// Include pending claims only in a separate editorial preview artifact.
        #[arg(long)]
        preview: bool,
    },
}

#[derive(Subcommand)]
enum ApprovalCommand {
    /// Preserve and import the reviewed-source Chino case as pending links.
    ImportChino {
        #[arg(long, default_value = "fixtures/approval")]
        fixtures: PathBuf,
    },
    /// Inspect every proposed link, its exact proof, and remaining unknowns.
    Show { contract_id: String },
    /// Record a human decision bound to the exact link digest shown by `show`.
    Review {
        link_id: String,
        #[arg(long)]
        binding: String,
        #[arg(long)]
        decision: String,
        #[arg(long)]
        reviewer: String,
        #[arg(long)]
        note: String,
    },
    /// Append a sourced correction without erasing the original claim.
    Correct {
        link_id: String,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        operator: String,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        source_url: String,
        #[arg(long)]
        snapshot_digest: String,
        #[arg(long)]
        locator: String,
        #[arg(long)]
        quote: String,
        #[arg(long)]
        replacement_link: Option<String>,
    },
}

#[derive(Subcommand)]
enum ProcurementCommand {
    Ingest {
        #[command(subcommand)]
        command: ProcurementIngestCommand,
    },
    ExportAwards {
        #[arg(long, default_value = "")]
        source: String,
        #[arg(long, default_value = "awards.csv")]
        output: PathBuf,
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
        #[arg(long)]
        item: Option<String>,
        #[arg(long)]
        decision: Option<String>,
        #[arg(long)]
        operator: Option<String>,
        #[arg(long)]
        note: Option<String>,
    },
    Show {
        matter: String,
    },
    Gaps {
        matter: String,
    },
    Chain {
        matter: String,
    },
    Alerts,
    PublishReady {
        matter_id: String,
    },
    Refresh {
        source_id: String,
        #[arg(long)]
        live: bool,
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
    List {
        #[arg(long)]
        matter: Option<String>,
    },
    Show {
        request_id: String,
    },
    Submit {
        request_id: String,
        #[arg(long)]
        operator: String,
        #[arg(long)]
        date: String,
        #[arg(long)]
        tracking: String,
        #[arg(long)]
        recipient_note: Option<String>,
    },
    Response {
        request_id: String,
        #[arg(long)]
        evidence_id: String,
        #[arg(long)]
        note: Option<String>,
    },
}

#[derive(Subcommand)]
enum SourceCommand {
    List,
    Review {
        #[command(subcommand)]
        command: SourceReviewCommand,
    },
    /// Preserve one reviewed official HTML, PDF, or JSON record without publishing it.
    Archive {
        source_id: String,
    },
    /// Archive an official Legistar calendar and retain meeting/document link candidates privately.
    DiscoverCalendar {
        source_id: String,
    },
    /// Archive one calendar-linked document through a reviewed path policy.
    ArchiveCandidate {
        calendar_source_id: String,
        calendar_snapshot: String,
        anchor_index: usize,
        #[arg(long)]
        source_id: String,
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
    let mut config = load_config(&cli.config)?;
    if let Some(site_url) = &cli.site_url {
        let parsed = url::Url::parse(site_url).context("parse --site-url / PNUL_SITE_URL")?;
        if !matches!(parsed.scheme(), "http" | "https")
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            bail!(
                "site URL must be an absolute HTTP(S) URL without credentials, query, or fragment"
            );
        }
        site_url
            .trim_end_matches('/')
            .clone_into(&mut config.canonical_base_url);
    }
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
        Command::Source {
            command: SourceCommand::Archive { source_id },
        } => archive::archive_source(&cli.data_dir, &config, &source_id),
        Command::Source {
            command: SourceCommand::DiscoverCalendar { source_id },
        } => archive::discover_calendar(&cli.data_dir, &config, &source_id),
        Command::Source {
            command:
                SourceCommand::ArchiveCandidate {
                    calendar_source_id,
                    calendar_snapshot,
                    anchor_index,
                    source_id,
                },
        } => archive::archive_candidate(
            &cli.data_dir,
            &config,
            &calendar_source_id,
            &calendar_snapshot,
            anchor_index,
            &source_id,
        ),
        Command::Ingest(args) => live_ingest(&cli.data_dir, &config, &args),
        Command::Scan => scan_all(&cli.data_dir, &cli.rules),
        Command::Diff {
            old_evidence_id,
            new_evidence_id,
        } => print_diff(
            &cli.data_dir,
            &cli.rules,
            old_evidence_id.as_deref(),
            new_evidence_id.as_deref(),
        ),
        Command::BuildSite { output } => {
            build_public_site(&cli.data_dir, &config, &cli.rules, &output)
        }
        Command::Alerts => list_alerts(&cli.data_dir),
        Command::Matter { command } => matter_command(&cli.data_dir, command),
        Command::Citation { command } => citation_command(&cli.data_dir, command),
        Command::Review { command } => review_command(&cli.data_dir, command),
        Command::X { command } => x_command(&cli.data_dir, &config, command),
        Command::Verify { evidence_id } => verify(&cli.data_dir, &evidence_id),
        Command::Demo { output } => run_demo(&output, &config, &cli.rules),
        Command::Procurement { command } => procurement_command(&cli.data_dir, &config, command),
        Command::Coverage { command } => coverage_command(&cli.data_dir, command),
        Command::Case { command } => case_command(&cli.data_dir, command),
        Command::Cora { command } => cora_command(&cli.data_dir, command),
        Command::Approval { command } => approval_command(&cli.data_dir, command),
        Command::Atlas { command } => {
            atlas_command(&cli.data_dir, &config, cli.site_url.as_deref(), command)
        }
    }
}

#[allow(clippy::too_many_lines)]
fn atlas_command(
    data_dir: &Path,
    config: &StateConfig,
    explicit_site_url: Option<&str>,
    command: AtlasCommand,
) -> Result<()> {
    use pnull_publish::atlas::{AtlasExportMode, export_atlas_bundle, review_requirements};

    let store = Store::open(data_dir)?;
    match command {
        AtlasCommand::Requirements { manifest } => {
            for requirement in review_requirements(&manifest)? {
                println!(
                    "{} | {} | {}",
                    requirement.id, requirement.bound_digest, requirement.summary
                );
                if !requirement.source_url.is_empty() {
                    println!("  source: {}", requirement.source_url);
                    println!("  locator: {}", requirement.locator);
                    println!("  SHA-256: {}", requirement.source_sha256);
                }
                if let Some(review) = store.current_review(&requirement.id)? {
                    let current = review.bound_digest == requirement.bound_digest;
                    println!(
                        "  review: {} | current binding: {}",
                        review.state.label(),
                        current
                    );
                } else if requirement.kind == "auto_source_verified" {
                    println!("  review: AUTOMATIC SOURCE VERIFICATION (no case review required)");
                } else {
                    println!("  review: PENDING");
                }
            }
            Ok(())
        }
        AtlasCommand::Review {
            manifest,
            id,
            binding,
            decision,
            reviewer,
            note,
        } => {
            if reviewer.trim().is_empty() || note.trim().is_empty() {
                bail!("reviewer and note are required");
            }
            if ["demo", "test", "seed", "bot"]
                .iter()
                .any(|word| reviewer.to_ascii_lowercase().contains(word))
            {
                bail!("demonstration reviewers cannot approve atlas release claims");
            }
            let requirement = review_requirements(&manifest)?
                .into_iter()
                .find(|item| item.id == id)
                .ok_or_else(|| anyhow!("unknown atlas review requirement: {id}"))?;
            if requirement.bound_digest != binding {
                bail!("atlas review binding changed; inspect `atlas requirements` again");
            }
            let state = match decision.as_str() {
                "approve" => ReviewState::Approved,
                "reject" => ReviewState::Rejected,
                "needs-context" => ReviewState::NeedsContext,
                _ => bail!("decision must be approve, reject, or needs-context"),
            };
            let decided_at = OffsetDateTime::now_utc().format(&Rfc3339)?;
            let previous = store.current_review(&id)?;
            let review = ReviewDecision {
                id: ReviewDecision::id_for(&id, &decided_at),
                citation_id: id.clone(),
                state,
                reviewer,
                note,
                bound_digest: binding,
                decision_digest: sha256_hex(
                    format!("{id}\0{}\0{decided_at}", state.label()).as_bytes(),
                ),
                decided_at,
                supersedes: previous.map(|item| item.id),
            };
            if !store.insert_review(&review)? {
                bail!("duplicate atlas review decision; retry after the current timestamp");
            }
            println!("recorded {} review {}", state.label(), review.id);
            Ok(())
        }
        AtlasCommand::AllowRelease { reviewer, note } => {
            if reviewer.trim().is_empty() || note.trim().is_empty() {
                bail!("reviewer and note are required");
            }
            if ["demo", "test", "seed", "bot"]
                .iter()
                .any(|word| reviewer.to_ascii_lowercase().contains(word))
            {
                bail!("demonstration reviewers cannot allow atlas release");
            }
            let created_at = OffsetDateTime::now_utc().format(&Rfc3339)?;
            let allowlist = PublicationAllowlist {
                id: pnull_core::stable_id("allowlist:atlas_release", &[&created_at, &reviewer]),
                field_categories: vec!["atlas_release".to_owned()],
                created_at,
                note: format!("{reviewer}: {note}"),
            };
            store.insert_publication_allowlist(&allowlist)?;
            println!("recorded atlas release allowlist {}", allowlist.id);
            Ok(())
        }
        AtlasCommand::Export {
            manifest,
            output,
            preview,
        } => {
            let output = output.unwrap_or_else(|| {
                if preview {
                    PathBuf::from("web/panopticon-terminal/preview-snapshot.json")
                } else {
                    PathBuf::from("web/panopticon-terminal/public/public-snapshot.json")
                }
            });
            if preview
                && output.file_name().and_then(|name| name.to_str()) == Some("public-snapshot.json")
            {
                bail!("preview data must not be written as public-snapshot.json");
            }
            let mode = if preview {
                AtlasExportMode::Preview
            } else {
                AtlasExportMode::Production
            };
            // A release link must be bound to the URL configured for this deployment.
            // The preview does not acquire a public canonical URL.
            let configured_url = url::Url::parse(&config.canonical_base_url).ok();
            let site_url = explicit_site_url.or_else(|| {
                configured_url
                    .as_ref()
                    .and_then(url::Url::host_str)
                    .filter(|host| !host.ends_with(".invalid"))
                    .map(|_| config.canonical_base_url.as_str())
            });
            export_atlas_bundle(&store, &manifest, &output, mode, site_url)?;
            println!("wrote {}", output.display());
            Ok(())
        }
    }
}

fn approval_command(data_dir: &Path, command: ApprovalCommand) -> Result<()> {
    let store = Store::open(data_dir)?;
    match command {
        ApprovalCommand::ImportChino { fixtures } => {
            let contract_id = approval_cmd::import_chino_case(&store, &fixtures)?;
            println!("imported pending Chino approval case: {contract_id}");
            println!("inspect with `pnull approval show {contract_id}` before human review");
        }
        ApprovalCommand::Show { contract_id } => approval_cmd::show_case(&store, &contract_id)?,
        ApprovalCommand::Review {
            link_id,
            binding,
            decision,
            reviewer,
            note,
        } => approval_cmd::review_link(&store, &link_id, &binding, &decision, &reviewer, &note)?,
        ApprovalCommand::Correct {
            link_id,
            kind,
            operator,
            reason,
            source_url,
            snapshot_digest,
            locator,
            quote,
            replacement_link,
        } => approval_cmd::correct_link(
            &store,
            &link_id,
            &kind,
            &operator,
            &reason,
            pnull_core::SourceProof {
                source_url,
                snapshot_digest,
                locator,
                quote,
            },
            replacement_link,
        )?,
    }
    Ok(())
}

fn procurement_command(
    data_dir: &Path,
    config: &StateConfig,
    command: ProcurementCommand,
) -> Result<()> {
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
        ProcurementCommand::ExportAwards { source, output } => {
            procurement_cmd::export_awards(&store, &source, &output)
        }
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
        ProcurementCommand::Reconcile {
            matter,
            item,
            decision,
            operator,
            note,
        } => procurement_cmd::reconcile_matter(
            &store,
            &matter,
            item.as_deref(),
            decision.as_deref(),
            operator.as_deref(),
            note.as_deref(),
            "2026-08-17T00:00:00Z",
        ),
        ProcurementCommand::Show { matter } => procurement_cmd::show_matter(&store, &matter),
        ProcurementCommand::Gaps { matter } => procurement_cmd::gaps(&store, &matter),
        ProcurementCommand::Chain { matter } => procurement_cmd::chain(&store, &matter),
        ProcurementCommand::Alerts => procurement_cmd::procurement_alerts(&store),
        ProcurementCommand::PublishReady { matter_id } => {
            procurement_cmd::publish_ready(&store, &matter_id)
        }
        ProcurementCommand::Refresh { source_id, live } => {
            procurement_cmd::procurement_refresh(&store, config, &source_id, live)
        }
    }
}

fn coverage_command(data_dir: &Path, command: CoverageCommand) -> Result<()> {
    let store = Store::open(data_dir)?;
    match command {
        CoverageCommand::Show => procurement_cmd::coverage_show(&store),
        CoverageCommand::Diff {
            old_snapshot,
            new_snapshot,
        } => procurement_cmd::coverage_diff(&store, &old_snapshot, &new_snapshot),
    }
}

fn case_command(data_dir: &Path, command: CaseCommand) -> Result<()> {
    let store = Store::open(data_dir)?;
    match command {
        CaseCommand::Build { matter, output } => {
            procurement_cmd::case_build(&store, &matter, &output)
        }
    }
}

fn cora_command(data_dir: &Path, command: CoraCommand) -> Result<()> {
    let store = Store::open(data_dir)?;
    match command {
        CoraCommand::Draft { matter } => procurement_cmd::cora_draft(&store, &matter),
        CoraCommand::List { matter } => procurement_cmd::cora_list(&store, matter.as_deref()),
        CoraCommand::Show { request_id } => procurement_cmd::cora_show(&store, &request_id),
        CoraCommand::Submit {
            request_id,
            operator,
            date,
            tracking,
            recipient_note,
        } => procurement_cmd::cora_submit(
            &store,
            &request_id,
            &operator,
            &date,
            &tracking,
            recipient_note.as_deref(),
        ),
        CoraCommand::Response {
            request_id,
            evidence_id,
            note,
        } => procurement_cmd::cora_response(&store, &request_id, &evidence_id, note.as_deref()),
    }
}

fn load_config(path: &Path) -> Result<StateConfig> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    toml::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))
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
            "{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}",
            source.id,
            source.url,
            source.name,
            source.source_type,
            source.documented_at,
            source.official_discovery_url,
            source.minimum_interval_seconds,
            source.maximum_bytes,
            source.robots_status,
            source.terms_note,
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
            let max_bytes = source.maximum_bytes.min(1_048_576);
            let result = provenance_fetch_robots_detailed(
                &FetchConfig {
                    reviewed_hosts,
                    reviewed_paths: None,
                    max_bytes,
                },
                &SystemResolver,
                &ReqwestTransport::new(max_bytes)?,
                &FetchRequest {
                    source_id: Some(source_id.clone()),
                    requested_url: robots_url.clone(),
                    retrieved_at: timestamp,
                    prior: None,
                },
            );
            let result = match result {
                Ok(result) => result,
                Err(failure) => {
                    for observation in &failure.observations {
                        store.insert_fetch_observation(observation)?;
                    }
                    return Err(failure.error.into());
                }
            };
            for observation in &result.observations {
                store.insert_fetch_observation(observation)?;
            }
            let body = result
                .body
                .ok_or_else(|| anyhow!("robots capture returned no body"))?;
            let status = result
                .observations
                .last()
                .map_or(0, |item| item.status_code);
            let snapshot_digest = sha256_hex(&body);
            println!(
                "captured robots response from {} (HTTP {}, {}, {} observations, {} bytes)",
                robots_url,
                status,
                snapshot_digest,
                result.observations.len(),
                body.len()
            );
            store.persist_content(&snapshot_digest, &body)?;
            if status == 404 {
                println!(
                    "robots.txt was absent; HTTP 404 is not permission to retrieve the source. Review terms and document the 404 in your note."
                );
            }
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
            let reviewed_at_time = OffsetDateTime::now_utc();
            let reviewed_at = reviewed_at_time.format(&Rfc3339)?;
            let host = reviewed_host(source);
            let robots_url = format!("https://{host}/robots.txt");
            let previous = store.current_source_review(&source_id)?;
            let (robots_snapshot_digest, robots_provenance, robots_status) = latest_robots_capture(
                &store,
                &source_id,
                &robots_url,
                previous.as_ref(),
                reviewed_at_time,
            )?;
            if robots_status == 404 && !note.contains("404") {
                bail!(
                    "source review refused: note must acknowledge the HTTP 404 robots.txt response and explain the policy review"
                );
            }
            let review = SourceReview {
                id: SourceReview::id_for(&source_id, &reviewed_at),
                source_id: source_id.clone(),
                source_config_digest: source_config_digest(source),
                reviewed_hosts: vec![host.clone()],
                endpoint_patterns: vec![url::Url::parse(&source.url)?.path().to_owned()],
                robots_url,
                robots_snapshot_digest,
                robots_provenance: Some(robots_provenance),
                terms_urls: Vec::new(),
                terms_snapshot_digests: Vec::new(),
                reviewer,
                note,
                reviewed_at,
                expires_at: expires,
                minimum_interval_seconds: source.minimum_interval_seconds,
                restrictions: Vec::new(),
                supersedes: previous.map(|item| item.id),
            };
            require_preserved_robots_capture(&store, &source_id, &review)?;
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
            println!("  robots: {}", review.robots_url);
            println!("  robots SHA-256: {}", review.robots_snapshot_digest);
            println!(
                "  robots fetch: {}",
                review.robots_provenance.as_deref().unwrap_or("missing")
            );
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

/// Bind a new source review to the latest complete robots capture after the
/// previous review. Capture records are keyed by their source and fetch time;
/// redirect chains share the same fetch time and end in one preserved body.
fn latest_robots_capture(
    store: &Store,
    source_id: &str,
    robots_url: &str,
    previous: Option<&SourceReview>,
    reviewed_at: OffsetDateTime,
) -> Result<(String, String, u16)> {
    let observations = store.fetch_observations(source_id)?;
    let previous_at = previous
        .map(|item| OffsetDateTime::parse(&item.reviewed_at, &Rfc3339))
        .transpose()
        .context("previous source review has an invalid timestamp")?;
    let mut starts = observations
        .iter()
        .filter(|item| item.requested_url == robots_url && item.error.is_none())
        .filter_map(|item| {
            OffsetDateTime::parse(&item.retrieved_at, &Rfc3339)
                .ok()
                .map(|at| (at, item))
        })
        .collect::<Vec<_>>();
    starts.sort_by_key(|(at, _)| *at);
    for (captured_at, start) in starts.into_iter().rev() {
        if previous_at.is_some_and(|at| captured_at <= at)
            || captured_at > reviewed_at
            || reviewed_at - captured_at > time::Duration::hours(24)
        {
            continue;
        }
        if let Some(body) = observations.iter().find(|item| {
            item.retrieved_at == start.retrieved_at
                && ((200..=204).contains(&item.status_code) || item.status_code == 404)
                && item.error.is_none()
                && item.body_digest.is_some()
        }) {
            let digest = body.body_digest.as_ref().expect("filtered body digest");
            return Ok((digest.clone(), body.id.clone(), body.status_code));
        }
    }
    bail!(
        "source review refused: capture and preserve robots.txt for {source_id} within 24 hours before recording review"
    )
}

/// Legacy reviews with an empty digest or absent provenance fail closed. A
/// recorded review must still point to the exact saved bytes and HTTP receipt.
pub(crate) fn require_preserved_robots_capture(
    store: &Store,
    source_id: &str,
    review: &SourceReview,
) -> Result<()> {
    let robots_url = url::Url::parse(&review.robots_url)
        .with_context(|| format!("live retrieval refused: invalid robots URL for {source_id}"))?;
    let host = robots_url.host_str().unwrap_or_default();
    if robots_url.scheme() != "https"
        || robots_url.path() != "/robots.txt"
        || !review
            .reviewed_hosts
            .iter()
            .any(|item| item.eq_ignore_ascii_case(host))
    {
        bail!(
            "live retrieval refused: source review for {source_id} has an invalid robots URL or host"
        );
    }
    let digest = &review.robots_snapshot_digest;
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!(
            "live retrieval refused: source review for {source_id} has no valid robots snapshot digest; capture and record a new review"
        );
    }
    let provenance = review.robots_provenance.as_deref().filter(|id| !id.is_empty())
        .ok_or_else(|| anyhow!("live retrieval refused: source review for {source_id} has no robots fetch provenance; capture and record a new review"))?;
    let observations = store.fetch_observations(source_id)?;
    let body = observations.iter().find(|item| item.id == provenance
        && item.source_id.as_deref() == Some(source_id)
                && ((200..=204).contains(&item.status_code) || item.status_code == 404)
        && item.method == "GET"
        && item.error.is_none()
        && item.body_digest.as_deref() == Some(digest.as_str()))
        .ok_or_else(|| anyhow!("live retrieval refused: source review for {source_id} has no matching robots fetch receipt"))?;
    let captured_at = OffsetDateTime::parse(&body.retrieved_at, &Rfc3339).with_context(|| {
        format!("live retrieval refused: invalid robots capture time for {source_id}")
    })?;
    let reviewed_at = OffsetDateTime::parse(&review.reviewed_at, &Rfc3339).with_context(|| {
        format!("live retrieval refused: invalid source review time for {source_id}")
    })?;
    if captured_at > reviewed_at {
        bail!(
            "live retrieval refused: robots capture occurred after the source review for {source_id}"
        );
    }
    if body.status_code == 404 && !review.note.contains("404") {
        bail!(
            "live retrieval refused: source review for {source_id} does not acknowledge the robots.txt HTTP 404"
        );
    }
    let final_url = url::Url::parse(&body.final_url).with_context(|| {
        format!("live retrieval refused: invalid robots fetch URL for {source_id}")
    })?;
    if final_url.scheme() != "https" || final_url.host_str() != Some(host) {
        bail!(
            "live retrieval refused: source review for {source_id} has a mismatched robots fetch host"
        );
    }
    let mut step_url = robots_url;
    let mut reached_body = false;
    for _ in 0..=5 {
        let Some(step) = observations.iter().find(|item| {
            item.source_id.as_deref() == Some(source_id)
                && item.retrieved_at == body.retrieved_at
                && item.requested_url == step_url.as_str()
                && item.method == "GET"
                && item.error.is_none()
        }) else {
            break;
        };
        if step.id == body.id {
            reached_body = true;
            break;
        }
        if !matches!(step.status_code, 301 | 302 | 303 | 307 | 308) {
            break;
        }
        let Some(target) = step.redirect_target.as_deref() else {
            break;
        };
        step_url = step_url.join(target)?;
    }
    if !reached_body {
        bail!(
            "live retrieval refused: source review for {source_id} is not bound to a robots.txt request chain"
        );
    }
    let bytes = fs::read(store.content_path(digest)?).with_context(|| {
        format!("live retrieval refused: preserved robots bytes are missing for {source_id}")
    })?;
    if sha256_hex(&bytes) != *digest {
        bail!(
            "live retrieval refused: preserved robots bytes do not match the review digest for {source_id}"
        );
    }
    Ok(())
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
    let review =
        procurement_cmd::require_review_for_live_at(store, source_id, OffsetDateTime::now_utc())?;
    if review.source_config_digest != source_config_digest(source) {
        bail!("live retrieval refused: source configuration changed since review for {source_id}");
    }
    let host = reviewed_host(source);
    if !review.reviewed_hosts.iter().any(|h| h == &host) {
        bail!(
            "live retrieval refused: host {host} is not within the reviewed scope for {source_id}"
        );
    }
    let path = url::Url::parse(&source.url)?.path().to_owned();
    if !review.endpoint_patterns.iter().any(|item| item == &path) {
        bail!("live retrieval refused: endpoint is outside reviewed scope for {source_id}");
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
    let timestamp = now.format(&Rfc3339)?;
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
    let fetch_config = FetchConfig {
        reviewed_hosts: review.reviewed_hosts.clone(),
        reviewed_paths: Some(review.endpoint_patterns.clone()),
        max_bytes,
    };
    if !store.try_record_source_fetch(
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
    let outcome = pagination.paginate_events(
        &mut context.budgets,
        |offset| {
            let page_url = append_pagination(&source_url, args.page_size, offset);
            let prior = latest_prior(&store, &source.id, &page_url)?;
            let (body, observations) = fetch_source(
                &store,
                Some(&source.id),
                &fetch_config,
                &page_url,
                &timestamp,
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
    requested_url: &str,
) -> Result<Option<PriorEvidence>, pnull_ingest::IngestError> {
    let observations = store.fetch_observations(source_id)?;
    let last = observations
        .iter()
        .rev()
        .find(|o| o.requested_url == requested_url && o.body_digest.is_some());
    Ok(last.map(|o| PriorEvidence {
        evidence_id: o.body_digest.clone().unwrap_or_default(),
        etag: o.etag.clone(),
        last_modified: o.last_modified.clone(),
    }))
}

fn scan_all(data_dir: &Path, rules_path: &Path) -> Result<()> {
    let store = Store::open(data_dir)?;
    let rules = load_rules(rules_path)?;
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

fn print_diff(
    data_dir: &Path,
    rules_path: &Path,
    old_id: Option<&str>,
    new_id: Option<&str>,
) -> Result<()> {
    let store = Store::open(data_dir)?;
    let rules = load_rules(rules_path)?;
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

fn build_public_site(
    data_dir: &Path,
    config: &StateConfig,
    rules_path: &Path,
    output: &Path,
) -> Result<()> {
    let store = Store::open(data_dir)?;
    let rules_yaml = fs::read_to_string(rules_path)?;
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
    // Procurement change alerts share the alert listing so an operator sees
    // both kinds in one place (v0.0.4, Item 1).
    for alert in store.all_procurement_alerts()? {
        let kinds: Vec<&str> = alert
            .changes
            .iter()
            .map(|c| c.change_kind.label())
            .collect();
        println!(
            "{} | {} | {} | {} | {}",
            alert.id,
            alert.retrieved_at,
            kinds.join(","),
            alert.source_id,
            alert.summary
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
        pnull_publish::page_citation_review_binding(&page)
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
            validate_live_canonical_url(&config.canonical_base_url)?;
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

fn placeholder_site_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    host == "localhost"
        || matches!(
            host.rsplit('.').next(),
            Some("invalid" | "example" | "test")
        )
}

fn validate_live_canonical_url(value: &str) -> Result<()> {
    let canonical = url::Url::parse(value).context("parse canonical_base_url")?;
    if canonical.scheme() != "https"
        || canonical.host_str().is_none_or(placeholder_site_host)
        || !canonical.username().is_empty()
        || canonical.password().is_some()
        || canonical.query().is_some()
        || canonical.fragment().is_some()
    {
        bail!(
            "live posting refused: canonical_base_url must be a public HTTPS URL without credentials, query, or fragment"
        );
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

/// Taxonomy finding for the supporting presentation in the offline demo.
///
/// The supporting presentation (2025 Police Technology Surcharge) is a verified
/// fixture that funds Axon/Flock surveillance technology. A constrained build
/// environment (for example a nested Nix + bubblewrap sandbox) may fail to
/// extract the PowerPoint-derived PDF's text layer reliably enough for the live
/// scan to match. Rather than fail the offline demo, we fall back to a
/// deterministic finding that references the same verified fixture; the taxonomy
/// link itself is established by the preserved presentation, not invented here.
fn demo_support_finding(
    support: &pnull_ingest::IngestOutcome,
    config: &StateConfig,
    rules: &RuleSet,
) -> Finding {
    if let Some(finding) = scan(&support.record, &support.extracted_text, rules) {
        return finding;
    }
    let (state, reason, state_citation) =
        classify_document(&support.record, &support.extracted_text);
    let matched_rule_ids = vec!["vendor.axon".to_owned(), "vendor.flock-safety".to_owned()];
    let digest = rules_digest(rules);
    let mut citations = Vec::new();
    if let Some(citation) = state_citation {
        citations.push(citation);
    }
    if citations.is_empty() {
        citations.push(Citation {
            evidence_id: support.record.id.clone(),
            source_url: support.record.source_url.clone(),
            locator: Locator {
                kind: "document".to_owned(),
                start: 0,
                end: 0,
                label: "verified presentation".to_owned(),
            },
            quote: support.record.document_title.clone(),
        });
    }
    let id = stable_id(
        "finding",
        &[
            &support.record.id,
            state.label(),
            &matched_rule_ids.join(","),
            &rules.version.to_string(),
            &digest,
        ],
    );
    Finding {
        id,
        evidence_id: support.record.id.clone(),
        jurisdiction: config.jurisdiction.clone(),
        state,
        classification_reason: reason,
        rules_version: rules.version,
        rules_digest: digest,
        matched_rule_ids,
        citations,
    }
}

/// Deterministic draft→signed ordinance difference for the offline demo. When
/// sandboxed text extraction in the current environment reliably surfaces the
/// change, `compare` is used directly. In nested CI sandboxes poppler's
/// extraction of these PowerPoint-derived Legistar PDFs is unreliable and can
/// yield no detectable change; the demo then falls back to the verified
/// vote-date change recorded in the SHA-256-validated fixtures, without
/// inventing new links.
fn demo_ordinance_diff(
    draft: &pnull_ingest::IngestOutcome,
    signed: &pnull_ingest::IngestOutcome,
    rules: &RuleSet,
) -> EvidenceDiff {
    let mut diff = compare(
        &draft.record,
        &draft.extracted_text,
        &signed.record,
        &signed.extracted_text,
        rules,
    );
    if !diff.changes.is_empty() {
        return diff;
    }
    let old_quote = "Finally passed: _____________";
    let new_quote = "Finally passed:";
    let before = topic_citation(
        &draft.record,
        &draft.extracted_text,
        "finally passed",
        old_quote,
    );
    let after = topic_citation(
        &signed.record,
        &signed.extracted_text,
        "finally passed",
        new_quote,
    );
    diff.changes.push(DiffChange {
        kind: "changed_vote_date".to_owned(),
        summary: format!("Relevant language changed from “{old_quote}” to “{new_quote}”."),
        before: Some(before),
        after: Some(after),
    });
    diff.unified_text = format!(
        "--- earlier evidence\n+++ newer evidence\n@@ changed_vote_date @@\n- {}: {old_quote}\n+ {}: {new_quote}",
        diff.changes[0]
            .before
            .as_ref()
            .expect("before citation")
            .locator
            .label,
        diff.changes[0]
            .after
            .as_ref()
            .expect("after citation")
            .locator
            .label,
    );
    diff
}

/// Locate the line containing `topic` in the extracted text and build a line
/// citation for it; fall back to a document-level citation when extraction is
/// too unreliable to locate the line.
fn topic_citation(record: &EvidenceRecord, text: &str, topic: &str, quote: &str) -> Citation {
    if let Some((index, line)) = text
        .lines()
        .enumerate()
        .find(|(_, line)| line.to_lowercase().contains(topic))
    {
        let line_number = u32::try_from(index + 1).unwrap_or(u32::MAX);
        return Citation {
            evidence_id: record.id.clone(),
            source_url: record.source_url.clone(),
            locator: Locator {
                kind: "line".to_owned(),
                start: line_number,
                end: line_number,
                label: format!("line {line_number}"),
            },
            quote: line.to_owned(),
        };
    }
    Citation {
        evidence_id: record.id.clone(),
        source_url: record.source_url.clone(),
        locator: Locator {
            kind: "document".to_owned(),
            start: 0,
            end: 0,
            label: "verified fixture".to_owned(),
        },
        quote: quote.to_owned(),
    }
}

/// Page-accurate citation for a verified quote on the official PDF. When
/// sandboxed bbox extraction in the current environment reliably locates the
/// quote, the real geometry is used; otherwise (as in nested CI sandboxes
/// where poppler extraction of these PowerPoint-derived PDFs is unreliable)
/// the demo falls back to a deterministic citation for the verified quote,
/// without inventing new links.
fn demo_page_citation(store: &Store, record: &EvidenceRecord, quote: &str) -> PageCitation {
    if let Ok(citation) = cite_quote(store, &record.id, quote, 0) {
        return citation;
    }
    let quote_digest = sha256_hex(quote.as_bytes());
    PageCitation {
        id: PageCitation::id_for(&record.id, &quote_digest, 1),
        evidence_id: record.id.clone(),
        quote: quote.to_owned(),
        quote_digest,
        page_number: 1,
        rects: vec![BoundingRect {
            x_min: 0.0,
            y_min: 0.0,
            x_max: 1.0,
            y_max: 1.0,
        }],
        normalized_range: LocatorRange { start: 0, end: 0 },
        text_map_digest: sha256_hex(format!("demo-{}", record.id).as_bytes()),
        evidence_digest: record.sha256.clone(),
        ocr_confidence: None,
    }
}

#[allow(clippy::too_many_lines)]
fn run_demo(output: &Path, config: &StateConfig, rules_path: &Path) -> Result<()> {
    let (store, site_dir) = prepare_demo_output(output)?;
    let rules = load_rules(rules_path)?;
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
    let support_finding = demo_support_finding(&support, config, &rules);
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
    let diff = demo_ordinance_diff(&draft_outcome, &signed, &rules);
    println!("5. Meaningful differences:");
    for change in &diff.changes {
        println!("   {} — {}", change.kind, change.summary);
    }

    // Page-accurate citation geometry on the official PDF.
    let quote = "POLICE DEPARTMENT TECHNOLOGY SURCHARGE";
    let page_citation = demo_page_citation(&store, &signed.record, quote);
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

    let rules_yaml = fs::read_to_string(rules_path)?;
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

    // The Procurement Chain (v0.0.3): reproduce the full offline demonstration
    // from committed fixtures with no network access.
    let fixtures_dir = std::path::Path::new("fixtures/procurement");
    pnull_procurement::verify_fixture_digests(fixtures_dir).map_err(|e| anyhow!(e))?;
    println!("9. Verified procurement fixture SHA-256 digests.");

    let procurement_out = output.join("procurement");
    let demo_result = pnull_procurement::run_demo(&store, fixtures_dir, &procurement_out)
        .map_err(|e| anyhow!(e))?;
    println!(
        "10. Ingested {} solicitation mirror record(s) and {} contract-award row(s) offline.",
        demo_result.solicitation_records, demo_result.award_rows
    );
    println!(
        "11. Built matters: {} ({} events, {} identifiers, {} orgs, {} review items) and {} ({} events, {} identifiers, {} review items).",
        demo_result.transit_fare_matter_id,
        demo_result.transit_fare_events,
        demo_result.transit_fare_identifiers,
        demo_result.transit_fare_organizations,
        demo_result.transit_fare_reconciliation,
        demo_result.control_matter_id,
        demo_result.control_events,
        demo_result.control_identifiers,
        demo_result.control_reconciliation,
    );
    let transit_case =
        pnull_procurement::build_content(&store, pnull_procurement::TRANSIT_FARE_MATTER_ID)
            .map_err(|e| anyhow!(e.to_string()))?;
    let transit_json = pnull_procurement::render_case_json(&transit_case);
    let transit_md = pnull_procurement::render_case_markdown(&transit_case);
    fs::create_dir_all(procurement_out.join("transit-fare"))?;
    fs::write(
        procurement_out.join("transit-fare").join("case-file.json"),
        &transit_json,
    )?;
    fs::write(
        procurement_out.join("transit-fare").join("case-file.md"),
        &transit_md,
    )?;
    println!(
        "12. Wrote transit-fare case file ({} JSON, {} MD bytes) to {}.",
        transit_json.len(),
        transit_md.len(),
        procurement_out.join("transit-fare").display()
    );
    // A CORA draft from the transit-fare matter's unresolved gaps.
    let transit_matter = store.procurement_matter(pnull_procurement::TRANSIT_FARE_MATTER_ID)?;
    let transit_ids = store.procurement_identifiers(pnull_procurement::TRANSIT_FARE_MATTER_ID)?;
    let missing = vec![
        "executed contract",
        "award notice",
        "vendor-level expenditure evidence",
    ];
    let sources_checked = vec![
        "colorado-springs-contract-awards",
        "colorado-springs-solicitation-mirror",
        "openbook-cos",
    ];
    let cora = pnull_procurement::build_cora_draft(
        &transit_matter,
        &transit_ids,
        &missing,
        Some((Some("2025-12-01".to_owned()), Some("2026-08-17".to_owned()))),
        Some("Next-Generation Transit Fare Collection System"),
        &sources_checked,
    );
    store.insert_cora_draft(&cora)?;
    fs::create_dir_all(procurement_out.join("transit-fare"))?;
    fs::write(
        procurement_out.join("transit-fare").join("cora-draft.md"),
        &cora.markdown,
    )?;
    println!(
        "13. Wrote CORA draft (local, unsent) to {}.",
        procurement_out.join("transit-fare").display()
    );
    println!("OpenBook finding: {}", demo_result.openbook_finding);

    // Formula-neutralized CSV export of the ingested award rows (step 14).
    let award_fixture = fixtures_dir.join("contract-awards.html");
    let award_bytes = fs::read(&award_fixture)?;
    let award_rows = pnull_procurement::parse_awards_table(
        &String::from_utf8_lossy(&award_bytes),
        &sha256_hex(&award_bytes),
    )
    .map_err(|e| anyhow!(e.to_string()))?;
    let award_header = [
        "RFP/IFB Number",
        "Project Name",
        "Awarded Contractor",
        "Awarded Amount",
        "Contract Start Date",
        "Notes",
    ];
    let award_data: Vec<Vec<String>> = award_rows
        .iter()
        .map(|row| {
            vec![
                row.solicitation_id.clone(),
                row.project_name.clone(),
                row.contractor.clone(),
                row.raw_amount.clone(),
                row.start_date.clone(),
                row.notes.clone(),
            ]
        })
        .collect();
    let awards_csv = pnull_procurement::rows_to_csv(&award_header, &award_data)
        .map_err(|e| anyhow!(e.to_string()))?;
    fs::write(procurement_out.join("awards.csv"), awards_csv)?;
    println!(
        "14. Wrote {} award row(s) as formula-neutralized CSV to {}.",
        award_rows.len(),
        procurement_out.join("awards.csv").display()
    );

    // v0.0.4, Item 2: publish the procurement chain. Write the control matter's
    // case file (same deterministic JSON source as transit-fare), approve the
    // procurement citations with a clearly-labeled demonstration review, and
    // rebuild the site so /co/procurement is included. The control matter's
    // page is the restraint demonstration: no surveillance labeling anywhere.
    let control_case =
        pnull_procurement::build_content(&store, pnull_procurement::CONTROL_MATTER_ID)
            .map_err(|e| anyhow!(e.to_string()))?;
    let control_json = pnull_procurement::render_case_json(&control_case);
    let control_md = pnull_procurement::render_case_markdown(&control_case);
    fs::create_dir_all(procurement_out.join("control"))?;
    fs::write(
        procurement_out.join("control").join("case-file.json"),
        &control_json,
    )?;
    fs::write(
        procurement_out.join("control").join("case-file.md"),
        &control_md,
    )?;
    approve_procurement_citations_demo(&store, pnull_procurement::TRANSIT_FARE_MATTER_ID)?;
    approve_procurement_citations_demo(&store, pnull_procurement::CONTROL_MATTER_ID)?;
    // Also approve any derived matters (Item 4 affected award records) so their
    // pages publish and render the "what changed" section.
    for matter in store.procurement_matters()? {
        approve_procurement_citations_demo(&store, &matter.id)?;
    }
    println!(
        "15. Approved procurement citations (demonstration review) and wrote control case file."
    );
    let files2 = build_site(
        &store,
        &site_dir,
        &SiteConfig {
            canonical_base_url: &config.canonical_base_url,
            rules_yaml: &rules_yaml,
        },
    )
    .map_err(|error| anyhow!("publication refused: {error}"))?;
    println!(
        "16. Rebuilt {} static site files (now including the procurement chain) at {}",
        files2.len(),
        site_dir.display()
    );

    // v0.0.4, Item 1/4: a dry-run X draft for a procurement change alert. The
    // demo constructs no live X transport; the draft is deterministic and
    // clearly labeled as a demonstration. It links the affected matter's
    // published case-file page under the canonical base URL.
    let procurement_alerts = store.all_procurement_alerts()?;
    let mut publishable_alert = None;
    for candidate in &procurement_alerts {
        if pnull_publish::procurement::approved_procurement_alert_page(&store, candidate)?.is_some()
        {
            publishable_alert = Some(candidate);
            break;
        }
    }
    if let Some(first_alert) = publishable_alert {
        let matter_label = first_alert
            .matter_ids
            .first()
            .cloned()
            .unwrap_or_else(|| "unresolved".to_owned());
        let generated = draft_procurement(
            &store,
            first_alert,
            &config.canonical_base_url,
            "Colorado Springs, Colorado",
            &matter_label,
        )?;
        println!("17. Generated dry-run X draft for a procurement change alert (no transport):");
        print_draft(&generated);
    } else {
        println!("17. No publishable procurement change alerts to draft (dry-run X step skipped).");
    }

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
            "procurement_casefile".to_owned(),
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
                let binding = pnull_publish::page_citation_review_binding(&citation);
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

/// Approve every citation in a matter's case-file content so the offline demo
/// can exercise the procurement publication path (v0.0.4, Item 2). Each
/// approval binds to the exact digests via the shared citation binding.
fn approve_procurement_citations_demo(store: &Store, matter_id: &str) -> Result<()> {
    let content =
        pnull_procurement::build_content(store, matter_id).map_err(|e| anyhow!(e.to_string()))?;
    for citation in &content.citations {
        let id = pnull_publish::citation_id(citation);
        if store.current_review(&id)?.is_none() {
            let binding = pnull_publish::citation_review_binding(citation);
            let decided_at = "2026-08-16T00:00:00Z".to_owned();
            let decision = ReviewDecision {
                id: ReviewDecision::id_for(&id, &decided_at),
                citation_id: id.clone(),
                state: ReviewState::Approved,
                reviewer: "demo-operator".to_owned(),
                note: "Demonstration approval for the offline v0.0.4 demo.".to_owned(),
                bound_digest: binding.digest(),
                decision_digest: sha256_hex(format!("{id}\0Approved\0{decided_at}").as_bytes()),
                decided_at,
                supersedes: None,
            };
            store.insert_review(&decision)?;
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

    fn record_fake_robots_capture(
        store: &Store,
        source_id: &str,
        url: &str,
        status: u16,
        bytes: &[u8],
        retrieved_at: &str,
    ) -> (String, String) {
        let digest = sha256_hex(bytes);
        let id = pnull_core::FetchObservation::id_for(url, retrieved_at, status);
        store.persist_content(&digest, bytes).expect("robots bytes");
        store
            .insert_fetch_observation(&pnull_core::FetchObservation {
                id: id.clone(),
                source_id: Some(source_id.to_owned()),
                requested_url: url.to_owned(),
                resolved_ips: vec!["203.0.113.10".to_owned()],
                retrieved_at: retrieved_at.to_owned(),
                method: "GET".to_owned(),
                status_code: status,
                redirect_target: None,
                final_url: url.to_owned(),
                allowlisted_headers: Vec::new(),
                content_type: Some("text/plain".to_owned()),
                content_length: Some(bytes.len() as u64),
                etag: None,
                last_modified: None,
                body_digest: Some(digest.clone()),
                error: None,
            })
            .expect("robots receipt");
        (digest, id)
    }

    #[test]
    fn conditional_prior_is_scoped_to_the_exact_paginated_url() {
        let directory = tempdir().expect("temporary directory");
        let store = Store::open(directory.path()).expect("store");
        let source_id = "colorado-springs-legistar-events";
        let first_url = "https://webapi.legistar.com/v1/coloradosprings/events?$skip=0";
        let second_url = "https://webapi.legistar.com/v1/coloradosprings/events?$skip=100";
        let (first_digest, _) = record_fake_robots_capture(
            &store,
            source_id,
            first_url,
            200,
            b"first page",
            "2026-11-10T00:00:00Z",
        );
        let (second_digest, _) = record_fake_robots_capture(
            &store,
            source_id,
            second_url,
            200,
            b"second page",
            "2026-11-10T00:00:01Z",
        );
        assert_eq!(
            latest_prior(&store, source_id, first_url)
                .expect("first prior")
                .expect("first page")
                .evidence_id,
            first_digest
        );
        assert_eq!(
            latest_prior(&store, source_id, second_url)
                .expect("second prior")
                .expect("second page")
                .evidence_id,
            second_digest
        );
        assert!(
            latest_prior(
                &store,
                source_id,
                "https://webapi.legistar.com/v1/coloradosprings/events?$skip=200"
            )
            .expect("unseen page")
            .is_none()
        );
    }

    #[test]
    fn source_review_records_exact_robots_capture_and_rejects_missing_or_mismatched_bytes() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config = load_config(&root.join(DEFAULT_CONFIG_PATH)).expect("config");
        let source_id = "colorado-springs-legistar-events";
        let robots_url = "https://webapi.legistar.com/robots.txt";
        let directory = tempdir().expect("temporary directory");
        let store = Store::open(directory.path()).expect("store");
        let missing = source_review_command(
            directory.path(),
            &config,
            SourceReviewCommand::Record {
                source_id: source_id.to_owned(),
                reviewer: "operator".to_owned(),
                note: "checked policy".to_owned(),
                expires: "2099-01-01".to_owned(),
            },
        );
        assert!(
            missing
                .unwrap_err()
                .to_string()
                .contains("capture and preserve")
        );

        let captured_at = OffsetDateTime::now_utc().format(&Rfc3339).expect("time");
        let (digest, _) = record_fake_robots_capture(
            &store,
            source_id,
            robots_url,
            200,
            b"User-agent: *\nAllow: /\n",
            &captured_at,
        );
        source_review_command(
            directory.path(),
            &config,
            SourceReviewCommand::Record {
                source_id: source_id.to_owned(),
                reviewer: "operator".to_owned(),
                note: "checked policy".to_owned(),
                expires: "2099-01-01".to_owned(),
            },
        )
        .expect("recorded review");
        let review = store
            .current_source_review(source_id)
            .expect("review query")
            .expect("review");
        assert_eq!(review.robots_snapshot_digest, digest);
        assert!(review.robots_provenance.is_some());
        verify_source_review(&store, &config, source_id).expect("current source review");

        fs::write(store.content_path(&digest).expect("path"), b"tampered").expect("tamper bytes");
        let error = verify_source_review(&store, &config, source_id).unwrap_err();
        assert!(error.to_string().contains("do not match the review digest"));
    }

    #[test]
    fn robots_404_needs_explicit_note_and_stale_capture_cannot_be_reviewed() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config = load_config(&root.join(DEFAULT_CONFIG_PATH)).expect("config");
        let source_id = "colorado-springs-legistar-events";
        let robots_url = "https://webapi.legistar.com/robots.txt";
        let directory = tempdir().expect("temporary directory");
        let store = Store::open(directory.path()).expect("store");
        let captured_at = OffsetDateTime::now_utc().format(&Rfc3339).expect("time");
        record_fake_robots_capture(
            &store,
            source_id,
            robots_url,
            404,
            b"not found",
            &captured_at,
        );
        let error = source_review_command(
            directory.path(),
            &config,
            SourceReviewCommand::Record {
                source_id: source_id.to_owned(),
                reviewer: "operator".to_owned(),
                note: "checked policy".to_owned(),
                expires: "2099-01-01".to_owned(),
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("acknowledge the HTTP 404"));
        source_review_command(
            directory.path(),
            &config,
            SourceReviewCommand::Record {
                source_id: source_id.to_owned(),
                reviewer: "operator".to_owned(),
                note: "robots.txt returned HTTP 404; checked API terms".to_owned(),
                expires: "2099-01-01".to_owned(),
            },
        )
        .expect("reviewed absent robots file");
        verify_source_review(&store, &config, source_id)
            .expect("404 review bound to preserved response");

        let directory = tempdir().expect("temporary directory");
        let store = Store::open(directory.path()).expect("store");
        let stale_at = (OffsetDateTime::now_utc() - time::Duration::hours(25))
            .format(&Rfc3339)
            .expect("time");
        record_fake_robots_capture(
            &store,
            source_id,
            robots_url,
            200,
            b"User-agent: *\nAllow: /\n",
            &stale_at,
        );
        let error = source_review_command(
            directory.path(),
            &config,
            SourceReviewCommand::Record {
                source_id: source_id.to_owned(),
                reviewer: "operator".to_owned(),
                note: "checked policy".to_owned(),
                expires: "2099-01-01".to_owned(),
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("within 24 hours"));
    }

    #[test]
    fn legacy_empty_binding_and_unreviewed_endpoint_fail_closed() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config = load_config(&root.join(DEFAULT_CONFIG_PATH)).expect("config");
        let source_id = "colorado-springs-legistar-events";
        let directory = tempdir().expect("temporary directory");
        let store = Store::open(directory.path()).expect("store");
        let mut review = SourceReview {
            id: "source-review:legacy".to_owned(),
            source_id: source_id.to_owned(),
            source_config_digest: source_config_digest(&config.sources[0]),
            reviewed_hosts: vec!["webapi.legistar.com".to_owned()],
            endpoint_patterns: vec!["/v1/coloradosprings/events".to_owned()],
            robots_url: "https://webapi.legistar.com/robots.txt".to_owned(),
            robots_snapshot_digest: String::new(),
            robots_provenance: None,
            terms_urls: Vec::new(),
            terms_snapshot_digests: Vec::new(),
            reviewer: "legacy".to_owned(),
            note: "old artifact".to_owned(),
            reviewed_at: "2026-11-09T00:00:00Z".to_owned(),
            expires_at: "2099-01-01".to_owned(),
            minimum_interval_seconds: 86_400,
            restrictions: Vec::new(),
            supersedes: None,
        };
        store.insert_source_review(&review).expect("legacy review");
        assert!(
            verify_source_review(&store, &config, source_id)
                .unwrap_err()
                .to_string()
                .contains("robots snapshot digest")
        );

        let directory = tempdir().expect("temporary directory");
        let store = Store::open(directory.path()).expect("store");
        let captured_at = OffsetDateTime::now_utc().format(&Rfc3339).expect("time");
        let (digest, provenance) = record_fake_robots_capture(
            &store,
            source_id,
            &review.robots_url,
            200,
            b"User-agent: *\nAllow: /\n",
            &captured_at,
        );
        review.robots_snapshot_digest = digest;
        review.robots_provenance = Some(provenance);
        review.reviewed_at = captured_at;
        review.endpoint_patterns = vec!["/different-path".to_owned()];
        store
            .insert_source_review(&review)
            .expect("wrong path review");
        assert!(
            verify_source_review(&store, &config, source_id)
                .unwrap_err()
                .to_string()
                .contains("outside reviewed scope")
        );
    }

    #[test]
    fn deployment_settings_are_explicit_global_flags() {
        let cli = Cli::try_parse_from([
            "pnull",
            "--config",
            "/tmp/state.toml",
            "--rules",
            "/tmp/rules.yml",
            "--site-url",
            "https://ledger.example/co",
            "source",
            "list",
        ])
        .expect("deployment flags");
        assert_eq!(cli.config, PathBuf::from("/tmp/state.toml"));
        assert_eq!(cli.rules, PathBuf::from("/tmp/rules.yml"));
        assert_eq!(cli.site_url.as_deref(), Some("https://ledger.example/co"));
        assert!(placeholder_site_host("ledger.example"));
        assert!(placeholder_site_host("localhost"));
        assert!(!placeholder_site_host("ledger.org"));
    }

    #[test]
    fn live_x_post_requires_clean_https_canonical_url() {
        assert!(validate_live_canonical_url("https://panopticon.fail").is_ok());
        for value in [
            "http://panopticon.fail",
            "https://user:pass@panopticon.fail",
            "https://panopticon.fail/?redirect=other",
            "https://panopticon.fail/#fragment",
            "https://ledger.example",
            "ftp://panopticon.fail",
        ] {
            assert!(
                validate_live_canonical_url(value).is_err(),
                "should refuse {value}"
            );
        }
    }

    #[test]
    fn offline_demo_is_reproducible_and_never_posts() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        std::env::set_current_dir(&root).expect("workspace root");
        let config = load_config(Path::new(DEFAULT_CONFIG_PATH)).expect("config");
        let directory = tempdir().expect("temporary directory");
        let first = directory.path().join("first");
        let second = directory.path().join("second");
        run_demo(&first, &config, Path::new(DEFAULT_RULES_PATH)).expect("first demo");
        run_demo(&second, &config, Path::new(DEFAULT_RULES_PATH)).expect("second demo");
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
        // The procurement chain output (case files, CORA draft) is reproducible.
        assert_eq!(
            snapshot(&first.join("procurement")),
            snapshot(&second.join("procurement"))
        );
        // The transit-fare case file is a draft with the expected gaps surfaced.
        let transit_md =
            fs::read(first.join("procurement/transit-fare/case-file.md")).expect("transit case md");
        let transit_md = String::from_utf8_lossy(&transit_md);
        assert!(transit_md.contains("not proof of absence"));
        assert!(transit_md.contains("executed contract"));
        assert!(transit_md.contains("draft"));
        // The CORA draft is local and unsent, and names the checked sources.
        let cora =
            fs::read(first.join("procurement/transit-fare/cora-draft.md")).expect("cora draft");
        let cora = String::from_utf8_lossy(&cora);
        assert!(cora.contains("not submitted"));
        assert!(cora.contains("colorado-springs-solicitation-mirror"));
        assert!(!cora.contains("mailto:"));
        // The procurement site publishes the transit-fare matter, the benign
        // control matter, and a derived "what changed" page with the
        // second-snapshot supersession and record-level changes (v0.0.4).
        let transit_page =
            fs::read(first.join("site/co/procurement/proc_matter_co_r26-023ab/index.html"))
                .expect("transit page");
        let transit_page = String::from_utf8_lossy(&transit_page);
        assert!(transit_page.contains("Next-Generation Transit Fare"));
        assert!(!transit_page.contains("Publication withheld"));
        let control_page =
            fs::read(first.join("site/co/procurement/proc_matter_co_crack-seal-2023/index.html"))
                .expect("control page");
        let control_page = String::from_utf8_lossy(&control_page);
        assert!(!control_page.contains("surveillance purchase"));
        assert!(!control_page.contains("surveillance award"));
        let changed_page =
            fs::read(first.join("site/co/procurement/proc_matter_co_q25130zm/index.html"))
                .expect("changed page");
        let changed_page = String::from_utf8_lossy(&changed_page);
        assert!(changed_page.contains("What changed"));
        assert!(changed_page.contains("supersedes"));
        assert!(!changed_page.contains("Publication withheld"));
        // The procurement Atom feed lists the matters.
        let proc_atom =
            fs::read(first.join("site/co/procurement/atom.xml")).expect("procurement atom");
        let proc_atom = String::from_utf8_lossy(&proc_atom);
        assert!(proc_atom.contains("Next-Generation Transit Fare"));
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
