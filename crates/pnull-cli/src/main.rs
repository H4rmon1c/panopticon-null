use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Parser, Subcommand};
use pnull_core::{
    Alert, EvidenceDiff, ExtractionStatus, Finding, SourceType, Store, sha256_hex, stable_id,
};
use pnull_detect::{build_alert, classify_document, compare, load_rules, scan};
use pnull_ingest::{DEFAULT_MAX_BYTES, IngestRequest, fetch_public_source, ingest_bytes};
use pnull_publish::{SiteConfig, build_site};
use pnull_x::{Credentials, ReqwestXTransport, draft, post_approved};
use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

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
}

#[derive(Subcommand)]
enum SourceCommand {
    List,
}

#[derive(Args)]
struct IngestArgs {
    #[arg(long, default_value = "colorado-springs-legistar-events")]
    source_id: String,
    #[arg(long)]
    robots_reviewed: bool,
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
        Command::X { command } => x_command(&cli.data_dir, &config, command),
        Command::Verify { evidence_id } => verify(&cli.data_dir, &evidence_id),
        Command::Demo { output } => run_demo(&output, &config),
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
            "{}\n  {}\n  API: {}\n  documentation: {}\n  official discovery: {}\n  minimum interval: {} seconds\n  robots: {}\n  terms: {}",
            source.id,
            source.name,
            source.url,
            source.documented_at,
            source.official_discovery_url,
            source.minimum_interval_seconds,
            source.robots_status,
            source.terms_note
        );
    }
}

fn live_ingest(data_dir: &Path, config: &StateConfig, args: &IngestArgs) -> Result<()> {
    let source = config
        .sources
        .iter()
        .find(|source| source.id == args.source_id)
        .ok_or_else(|| anyhow!("unknown source: {}", args.source_id))?;
    if !args.robots_reviewed {
        bail!(
            "live retrieval refused: current robots directives must be reviewed first; rerun only after review with --robots-reviewed"
        );
    }
    let store = Store::open(data_dir)?;
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
    let bytes = fetch_public_source(&source.url, source.maximum_bytes)?;
    store.record_source_fetch(&source.id, now.unix_timestamp())?;
    let timestamp = now.format(&Rfc3339)?;
    let request = IngestRequest {
        jurisdiction: config.jurisdiction.clone(),
        source_url: source.url.clone(),
        source_type: parse_source_type(&source.source_type)?,
        document_title: source.name.clone(),
        publication_date: None,
        retrieval_timestamp: timestamp,
        mime_type: "application/json".to_owned(),
        original_filename: format!("{}.json", source.id),
        supersedes: None,
        enable_ocr: false,
        ocr_language: "eng".to_owned(),
        max_bytes: source.maximum_bytes,
    };
    let outcome = ingest_bytes(&store, &request, &bytes)?;
    if !matches!(
        outcome.record.extraction_status,
        ExtractionStatus::Complete | ExtractionStatus::CompleteWithOcr
    ) {
        let detail = outcome
            .record
            .extraction_error
            .as_ref()
            .map_or("unknown extraction failure", |error| error.code.as_str());
        bail!("source was preserved but extraction failed: {detail}");
    }
    println!(
        "{} evidence {} ({})",
        if outcome.inserted {
            "ingested"
        } else {
            "duplicate"
        },
        outcome.record.id,
        outcome.record.sha256
    );
    Ok(())
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
    let files = build_site(
        &store,
        output,
        &SiteConfig {
            canonical_base_url: &config.canonical_base_url,
            rules_yaml: &rules_yaml,
        },
    )?;
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

fn x_command(data_dir: &Path, config: &StateConfig, command: XCommand) -> Result<()> {
    let store = Store::open(data_dir)?;
    match command {
        XCommand::Draft { alert_id } => {
            let alert = store.alert(&alert_id)?;
            let generated = draft(&alert, &config.canonical_base_url)?;
            print_draft(&generated);
            println!("DRY RUN: no network transport was created.");
        }
        XCommand::Approve { alert_id } => {
            let alert = store.alert(&alert_id)?;
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
            let generated = draft(&alert, &config.canonical_base_url)?;
            let credentials = Credentials::from_runtime()?;
            let mut transport = ReqwestXTransport::new(credentials)?;
            let timestamp = OffsetDateTime::now_utc().format(&Rfc3339)?;
            let ids = post_approved(&store, &generated, true, &timestamp, &mut transport)?;
            println!("posted {} item(s) for {}", ids.len(), alert_id);
        }
    }
    Ok(())
}

fn verify(data_dir: &Path, evidence_id: &str) -> Result<()> {
    let store = Store::open(data_dir)?;
    store.verify(evidence_id)?;
    let (record, _) = store.evidence(evidence_id)?;
    println!("verified {} sha256:{}", evidence_id, record.sha256);
    Ok(())
}

fn run_demo(output: &Path, config: &StateConfig) -> Result<()> {
    let (store, site_dir) = prepare_demo_output(output)?;
    let rules = load_rules(RULES_PATH)?;

    let draft_doc = demo_document(config, "ordinance-draft")?;
    let draft_outcome = ingest_demo_document(&store, config, draft_doc, None)?;
    println!("1. Ingested original draft: {}", draft_outcome.record.id);
    store.verify(&draft_outcome.record.id)?;
    println!("2. Verified SHA-256: {}", draft_outcome.record.sha256);

    let support_doc = demo_document(config, "supporting-presentation")?;
    let mut support = ingest_demo_document(&store, config, support_doc, None)?;
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

    let alert = persist_demo_alert(&store, config, &support_finding, &mut signed, diff)?;

    let rules_yaml = fs::read_to_string(RULES_PATH)?;
    let files = build_site(
        &store,
        &site_dir,
        &SiteConfig {
            canonical_base_url: &config.canonical_base_url,
            rules_yaml: &rules_yaml,
        },
    )?;
    println!(
        "6. Generated {} static site files at {}",
        files.len(),
        site_dir.display()
    );
    let generated = draft(&alert, &config.canonical_base_url)?;
    println!("7. Generated local X draft:");
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
