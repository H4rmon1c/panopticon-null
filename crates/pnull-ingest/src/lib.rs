//! Bounded ingestion and extraction of hostile public documents.
//!
//! External PDF and OCR tools run inside a real bubblewrap sandbox with
//! aggregate job budgets. Retrieval is provenance-aware and DNS-safe, and PDF
//! extraction produces page-accurate text-map artifacts.

pub mod budget;
pub mod sandbox;

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use budget::JobBudgetTracker;
use pnull_core::{
    BoundingRect, EvidenceRecord, ExtractionStatus, FetchObservation, Locator, MapWord, Matter,
    MatterAttachment, NativeTool, OutputArtifact, PageCitation, ProcessingRun, SourceType, Store,
    StructuredError, TextMap, evidence_id, sha256_hex,
};
use pnull_geometry::{
    COORDINATE_SYSTEM, PageSpec, build_page_citation, find_occurrences, normalized_range,
    parse_ocr_tsv, validate_text_map,
};
use pnull_http::{FetchConfig, FetchRequest, PriorEvidence, provenance_fetch};
use regex::Regex;
use sandbox::Sandbox;
use scraper::Html;
use serde_json::Value;
use thiserror::Error;
use url::Url;

pub use budget::{BudgetError, JobBudgetTracker as Tracker, JobBudgets as Budgets};
pub use sandbox::{
    BubblewrapSandbox as RealSandbox, FakeSandbox, Sandbox as ExtractionSandbox,
    SandboxConfig as ExtractionSandboxConfig, SandboxError as ExtractionSandboxError,
    SandboxOutput as ExtractionSandboxOutput,
};

pub const DEFAULT_MAX_BYTES: usize = 20 * 1024 * 1024;
const MAX_PDF_PAGES: u32 = 100;
const MAX_OCR_PAGES: u32 = 5;
const MAX_EXTRACTED_BYTES: usize = 5 * 1024 * 1024;
const EXTRACT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Debug)]
pub struct IngestRequest {
    pub jurisdiction: String,
    pub source_url: String,
    pub source_type: SourceType,
    pub document_title: String,
    pub publication_date: Option<String>,
    pub retrieval_timestamp: String,
    pub mime_type: String,
    pub original_filename: String,
    pub supersedes: Option<String>,
    pub enable_ocr: bool,
    pub ocr_language: String,
    pub max_bytes: usize,
}

#[derive(Clone, Debug)]
pub struct IngestOutcome {
    pub record: EvidenceRecord,
    pub inserted: bool,
    pub extracted_text: String,
    pub text_maps: Vec<TextMap>,
    pub page_citations: Vec<PageCitation>,
    pub processing_run: Option<ProcessingRun>,
}

#[derive(Debug, Error)]
pub enum IngestError {
    #[error("invalid ingestion metadata: {0}")]
    Metadata(String),
    #[error("input exceeds the configured {limit}-byte limit (observed at least {observed} bytes)")]
    Oversized { limit: usize, observed: usize },
    #[error("network retrieval failed without exposing response content")]
    Network,
    #[error("only public HTTPS source URLs are accepted")]
    InsecureUrl,
    #[error("live extraction refused: the required sandbox cannot be established")]
    SandboxUnavailable,
    #[error("aggregate job budget exceeded: {0}")]
    Budget(#[from] BudgetError),
    #[error(transparent)]
    Core(#[from] pnull_core::CoreError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Geometry(#[from] pnull_geometry::GeometryError),
    #[error(transparent)]
    Http(#[from] pnull_http::HttpError),
}

#[derive(Debug)]
struct Extraction {
    text: String,
    method: String,
    status: ExtractionStatus,
    error: Option<StructuredError>,
    text_maps: Vec<TextMap>,
}

/// Build metadata captured into every processing run.
#[derive(Clone, Debug)]
pub struct BuildMetadata {
    pub pnull_version: String,
    pub source_revision: String,
    pub rules_digest: String,
    pub state_config_digest: String,
}

impl BuildMetadata {
    pub fn local() -> Self {
        Self {
            pnull_version: pnull_core::PROCESSING_VERSION.to_owned(),
            source_revision: env!("CARGO_PKG_VERSION").to_owned(),
            rules_digest: String::new(),
            state_config_digest: String::new(),
        }
    }
}

/// Result of an extraction step: (text, method, status, text maps).
type ExtractionResult =
    Result<(String, String, ExtractionStatus, Vec<TextMap>), (&'static str, String)>;

/// Page geometry metadata shared by PDF extraction.
#[derive(Clone, Copy, Debug)]
pub struct PageDims {
    pub width: f64,
    pub height: f64,
    pub rotation: i32,
}

/// Context bundled into a processing-run record.
struct RunContext<'a> {
    build: &'a BuildMetadata,
    request: &'a IngestRequest,
    budgets: &'a JobBudgetTracker,
}

/// A provenance-aware fetch of a public source that persists observations.
pub fn fetch_source(
    store: &Store,
    source_id: Option<&str>,
    reviewed_hosts: &[String],
    url: &str,
    retrieved_at: &str,
    max_bytes: usize,
    prior: Option<&PriorEvidence>,
) -> Result<(Vec<u8>, Vec<FetchObservation>), IngestError> {
    let config = FetchConfig {
        reviewed_hosts: reviewed_hosts.to_vec(),
        max_bytes,
    };
    let resolver = pnull_http::SystemResolver;
    let transport = pnull_http::ReqwestTransport::new(max_bytes)?;
    let request = FetchRequest {
        source_id: source_id.map(str::to_owned),
        requested_url: url.to_owned(),
        retrieved_at: retrieved_at.to_owned(),
        prior: prior.cloned(),
    };
    let result = provenance_fetch(&config, &resolver, &transport, &request)?;
    for observation in &result.observations {
        store.insert_fetch_observation(observation)?;
    }
    let body = result
        .body
        .ok_or_else(|| IngestError::Metadata("304: no new content observed".to_owned()))?;
    Ok((body, result.observations))
}

pub fn ingest_bytes(
    store: &Store,
    sandbox: &dyn Sandbox,
    budgets: &mut JobBudgetTracker,
    build: &BuildMetadata,
    request: &IngestRequest,
    bytes: &[u8],
) -> Result<IngestOutcome, IngestError> {
    let started = Instant::now();
    validate_request(request)?;
    if bytes.len() > request.max_bytes {
        return Err(IngestError::Oversized {
            limit: request.max_bytes,
            observed: bytes.len(),
        });
    }
    budgets.add_downloaded_bytes(bytes.len() as u64)?;
    budgets.add_attachment()?;
    let digest = sha256_hex(bytes);
    let id = evidence_id(&request.jurisdiction, &request.source_url, &digest);
    persist_content(store, &digest, bytes)?;
    let extraction = extract(
        sandbox,
        budgets,
        bytes,
        &request.mime_type,
        request.enable_ocr,
        &request.ocr_language,
    );
    budgets.add_extracted_bytes(extraction.text.len() as u64)?;
    let line_count = extraction.text.lines().count();
    let locators = if line_count == 0 {
        Vec::new()
    } else {
        vec![Locator {
            kind: "line".to_owned(),
            start: 1,
            end: u32::try_from(line_count).unwrap_or(u32::MAX),
            label: format!("lines 1-{line_count}"),
        }]
    };
    let record = EvidenceRecord {
        id,
        jurisdiction: request.jurisdiction.clone(),
        source_url: request.source_url.clone(),
        source_type: request.source_type.clone(),
        document_title: request.document_title.clone(),
        publication_date: request.publication_date.clone(),
        retrieval_timestamp: request.retrieval_timestamp.clone(),
        mime_type: request.mime_type.clone(),
        sha256: digest,
        original_filename: request.original_filename.clone(),
        extraction_method: extraction.method,
        extraction_status: extraction.status,
        extraction_error: extraction.error,
        locators,
        matched_rule_ids: Vec::new(),
        quoted_source_spans: Vec::new(),
        supersedes: request.supersedes.clone(),
        processing_version: build.pnull_version.clone(),
    };
    let inserted = store.insert_evidence(&record, &extraction.text)?;
    let mut text_maps = extraction.text_maps;
    for map in &mut text_maps {
        // Bind each map to the preserved evidence record before persisting.
        map.evidence_id.clone_from(&record.id);
        map.source_digest.clone_from(&record.sha256);
        map.id = TextMap::id_for(&record.id, map.page_number, &map.digest);
        store.insert_text_map(map)?;
    }
    let completed = Instant::now();
    let run_context = RunContext {
        build,
        request,
        budgets,
    };
    let processing_run = build_processing_run(
        &run_context,
        &record,
        &text_maps,
        started,
        completed,
        "complete",
        Vec::new(),
    );
    store.insert_processing_run(&processing_run)?;
    Ok(IngestOutcome {
        record,
        inserted,
        extracted_text: extraction.text,
        text_maps,
        page_citations: Vec::new(),
        processing_run: Some(processing_run),
    })
}

fn build_processing_run(
    ctx: &RunContext,
    record: &EvidenceRecord,
    text_maps: &[TextMap],
    started: Instant,
    completed: Instant,
    outcome: &str,
    errors: Vec<StructuredError>,
) -> ProcessingRun {
    let native_tools = native_tool_versions();
    let output_artifacts = text_maps
        .iter()
        .map(|map| OutputArtifact {
            kind: "text_map".to_owned(),
            id: map.id.clone(),
            digest: map.digest.clone(),
        })
        .collect();
    ProcessingRun {
        id: pnull_core::stable_id(
            "run",
            &[&record.id, &ctx.request.retrieval_timestamp, outcome],
        ),
        schema_version: pnull_core::SCHEMA_VERSION,
        pnull_version: ctx.build.pnull_version.clone(),
        source_revision: ctx.build.source_revision.clone(),
        rules_digest: ctx.build.rules_digest.clone(),
        state_config_digest: ctx.build.state_config_digest.clone(),
        input_evidence_ids: vec![record.id.clone()],
        native_tools,
        sandbox_backend: "bubblewrap".to_owned(),
        sandbox_version: native_tool_version("bwrap").unwrap_or_else(|| "unknown".to_owned()),
        resource_budgets: ctx.budgets.snapshot(),
        resource_consumption: ctx.budgets.snapshot(),
        started_at: ctx.request.retrieval_timestamp.clone(),
        completed_at: format!("{:?}", completed.duration_since(started)),
        outcome: outcome.to_owned(),
        errors,
        output_artifacts,
    }
}

/// Records the version of every native tool actually used.
pub fn native_tool_versions() -> Vec<NativeTool> {
    let mut tools = Vec::new();
    for name in [
        "pdftotext",
        "pdfinfo",
        "pdftoppm",
        "tesseract",
        "bwrap",
        "prlimit",
    ] {
        if let Some(version) = native_tool_version(name) {
            tools.push(NativeTool {
                name: name.to_owned(),
                version,
            });
        }
    }
    tools
}

fn native_tool_version(name: &str) -> Option<String> {
    let output = Command::new(name).arg("--version").output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next().unwrap_or_default();
    if line.trim().is_empty() {
        None
    } else {
        Some(line.trim().to_owned())
    }
}

fn validate_request(request: &IngestRequest) -> Result<(), IngestError> {
    if request.jurisdiction.trim().is_empty() || request.document_title.trim().is_empty() {
        return Err(IngestError::Metadata(
            "jurisdiction and document title must not be empty".to_owned(),
        ));
    }
    let url = Url::parse(&request.source_url)
        .map_err(|error| IngestError::Metadata(format!("source URL: {error}")))?;
    if url.scheme() != "https" {
        return Err(IngestError::InsecureUrl);
    }
    if request.original_filename.is_empty()
        || Path::new(&request.original_filename)
            .file_name()
            .and_then(|name| name.to_str())
            != Some(request.original_filename.as_str())
    {
        return Err(IngestError::Metadata(
            "original filename must be a single safe path component".to_owned(),
        ));
    }
    if request.enable_ocr
        && (request.ocr_language.is_empty()
            || !request.ocr_language.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '+' | '-')
            }))
    {
        return Err(IngestError::Metadata(
            "OCR language must contain only safe language-code characters".to_owned(),
        ));
    }
    if request.max_bytes == 0 {
        return Err(IngestError::Metadata(
            "maximum input size must be positive".to_owned(),
        ));
    }
    Ok(())
}

fn persist_content(store: &Store, digest: &str, bytes: &[u8]) -> Result<(), std::io::Error> {
    let path = store
        .content_path(digest)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("content path has no parent"))?;
    fs::create_dir_all(parent)?;
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => {
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = fs::read(&path)?;
            if sha256_hex(&existing) != digest {
                return Err(std::io::Error::other(
                    "existing content-addressed blob failed digest verification",
                ));
            }
        }
        Err(error) => return Err(error),
    }
    Ok(())
}

fn extract(
    sandbox: &dyn Sandbox,
    budgets: &mut JobBudgetTracker,
    bytes: &[u8],
    mime_type: &str,
    enable_ocr: bool,
    ocr_language: &str,
) -> Extraction {
    let result = match mime_type.split(';').next().unwrap_or_default().trim() {
        "text/plain" => extract_plain(bytes),
        "text/html" | "application/xhtml+xml" => extract_html(bytes),
        "application/json" => extract_legistar_json(bytes),
        "application/pdf" => extract_pdf(sandbox, budgets, bytes, enable_ocr, ocr_language),
        other => Err((
            "unsupported_mime",
            format!("unsupported MIME type: {other}"),
        )),
    };
    let result = result.and_then(|(text, method, status, text_maps)| {
        if text.len() > MAX_EXTRACTED_BYTES {
            Err((
                "extracted_text_limit",
                format!(
                    "extracted text is {} bytes; limit is {MAX_EXTRACTED_BYTES}",
                    text.len()
                ),
            ))
        } else {
            Ok((text, method, status, text_maps))
        }
    });
    match result {
        Ok((text, method, status, text_maps)) => Extraction {
            text,
            method,
            status,
            error: None,
            text_maps,
        },
        Err((code, message)) => Extraction {
            text: String::new(),
            method: "none".to_owned(),
            status: ExtractionStatus::Failed,
            error: Some(StructuredError {
                code: code.to_owned(),
                message,
            }),
            text_maps: Vec::new(),
        },
    }
}

fn extract_plain(bytes: &[u8]) -> ExtractionResult {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| ("invalid_utf8", "plain text is not valid UTF-8".to_owned()))?;
    Ok((
        normalize_text(text),
        "utf8_plain_text".to_owned(),
        ExtractionStatus::Complete,
        Vec::new(),
    ))
}

fn extract_html(bytes: &[u8]) -> ExtractionResult {
    let html = std::str::from_utf8(bytes)
        .map_err(|_| ("invalid_utf8", "HTML is not valid UTF-8".to_owned()))?;
    let blocked = Regex::new(
        r"(?is)<(?:script|style|template|noscript)(?:\s[^>]*)?>.*?</(?:script|style|template|noscript)\s*>",
    )
        .map_err(|error| ("extractor_internal", error.to_string()))?;
    let sanitized = blocked.replace_all(html, " ");
    let document = Html::parse_document(&sanitized);
    let text = document
        .root_element()
        .text()
        .collect::<Vec<_>>()
        .join("\n");
    Ok((
        normalize_text(&text),
        "static_html_text".to_owned(),
        ExtractionStatus::Complete,
        Vec::new(),
    ))
}

fn extract_legistar_json(bytes: &[u8]) -> ExtractionResult {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| {
        (
            "malformed_json",
            "source JSON could not be parsed".to_owned(),
        )
    })?;
    if let Some(plain) = value.get("MatterTextPlain").and_then(Value::as_str) {
        return Ok((
            normalize_text(plain),
            "legistar_matter_text_json".to_owned(),
            ExtractionStatus::Complete,
            Vec::new(),
        ));
    }
    let mut lines = Vec::new();
    match &value {
        Value::Array(events) => {
            for event in events {
                extract_event(event, &mut lines)?;
            }
        }
        Value::Object(_) if value.get("MatterId").is_some() => {
            extract_matter(&value, &mut lines);
        }
        Value::Object(_) => extract_event(&value, &mut lines)?,
        _ => {
            return Err((
                "unsupported_json_shape",
                "expected a Legistar event collection, event, or matter-text response".to_owned(),
            ));
        }
    }
    Ok((
        normalize_text(&lines.join("\n")),
        "legistar_matter_json".to_owned(),
        ExtractionStatus::Complete,
        Vec::new(),
    ))
}

/// Flattens a single `GranicusMatter` record into normalized evidence text.
fn extract_matter(matter: &Value, lines: &mut Vec<String>) {
    push_json_field(lines, matter, "MatterFile", "Matter file");
    push_json_field(lines, matter, "MatterTitle", "Title");
    push_json_field(lines, matter, "MatterTypeName", "Matter type");
    push_json_field(lines, matter, "MatterStatusName", "Status");
    push_json_field(lines, matter, "MatterIntroDate", "Introduced");
    push_json_field(lines, matter, "MatterPassedDate", "Passed");
    push_json_field(lines, matter, "MatterEnactmentNumber", "Enactment number");
    push_json_field(lines, matter, "MatterEnactmentDate", "Enactment date");
    push_json_field(lines, matter, "MatterRequester", "Requester");
}

fn extract_event(event: &Value, lines: &mut Vec<String>) -> Result<(), (&'static str, String)> {
    push_json_field(lines, event, "EventDate", "Meeting date");
    push_json_field(lines, event, "EventAgendaStatusName", "Agenda status");
    let items = event.get("EventItems").and_then(Value::as_array).ok_or((
        "unsupported_json_shape",
        "a Legistar event did not include expanded EventItems".to_owned(),
    ))?;
    for item in items {
        push_json_field(lines, item, "EventItemMatterFile", "Matter file");
        push_json_field(lines, item, "EventItemTitle", "Title");
        push_json_field(lines, item, "EventItemActionName", "Action");
        push_json_field(lines, item, "EventItemActionText", "Vote record");
        if let Some(notes) = item.get("EventItemMinutesNote").and_then(Value::as_str) {
            lines.push(format!("Minutes: {}", strip_rtf(notes)));
        }
    }
    Ok(())
}

fn push_json_field(lines: &mut Vec<String>, value: &Value, key: &str, label: &str) {
    if let Some(text) = value.get(key).and_then(Value::as_str) {
        lines.push(format!("{label}: {text}"));
    }
}

fn strip_rtf(input: &str) -> String {
    let hex_escape = Regex::new(r"\\'[0-9a-fA-F]{2}").expect("constant regular expression");
    let controls = Regex::new(r"\\[a-zA-Z]+-?\d* ?").expect("constant regular expression");
    let escaped = input
        .replace("\\par", "\n")
        .replace("\\rquote", "'")
        .replace("\\&", "&");
    let without_hex = hex_escape.replace_all(&escaped, " ");
    controls
        .replace_all(&without_hex, " ")
        .replace(['{', '}'], " ")
}

fn extract_pdf(
    sandbox: &dyn Sandbox,
    budgets: &mut JobBudgetTracker,
    bytes: &[u8],
    enable_ocr: bool,
    ocr_language: &str,
) -> ExtractionResult {
    if !bytes.starts_with(b"%PDF-") {
        return Err(("malformed_pdf", "PDF signature is missing".to_owned()));
    }
    let workdir = sandbox.working_dir().to_path_buf();
    let input_path = workdir.join("input.pdf");
    fs::write(&input_path, bytes).map_err(external_io)?;
    budgets
        .add_child_process()
        .map_err(|error| budget_err(&error))?;
    let info = run_sandboxed(
        sandbox,
        budgets,
        "pdfinfo",
        &[input_path.as_os_str()],
        &[&input_path],
    )?;
    let pages = parse_page_count(&info).ok_or((
        "pdf_metadata",
        "pdfinfo did not return a valid page count".to_owned(),
    ))?;
    if pages > MAX_PDF_PAGES {
        return Err((
            "pdf_page_limit",
            format!("PDF has {pages} pages; limit is {MAX_PDF_PAGES}"),
        ));
    }
    budgets
        .add_pdf_pages(pages)
        .map_err(|error| budget_err(&error))?;
    let (width, height) = parse_page_dimensions(&info).unwrap_or((612.0, 792.0));
    let rotation = parse_page_rotation(&info).unwrap_or(0);

    let bbox_path = workdir.join("bbox.xml");
    budgets
        .add_child_process()
        .map_err(|error| budget_err(&error))?;
    let _ = run_sandboxed(
        sandbox,
        budgets,
        "pdftotext",
        &[
            std::ffi::OsStr::new("-bbox-layout"),
            input_path.as_os_str(),
            bbox_path.as_os_str(),
        ],
        &[&input_path],
    )?;
    let bbox_xml = fs::read_to_string(&bbox_path).map_err(external_io)?;
    let page_parse = parse_all_pages(&bbox_xml, width, height, rotation)?;
    let text_maps = page_parse.maps;
    let text = page_parse.lines.join("\n");
    if !text.trim().is_empty() {
        return Ok((
            normalize_text(&text),
            "poppler_pdftotext_bbox".to_owned(),
            ExtractionStatus::Complete,
            text_maps,
        ));
    }
    if !enable_ocr {
        return Err((
            "scanned_pdf_ocr_disabled",
            "PDF contains no extractable text; optional OCR was not enabled".to_owned(),
        ));
    }
    ocr_pdf(
        sandbox,
        budgets,
        &input_path,
        pages,
        &PageDims {
            width,
            height,
            rotation,
        },
        ocr_language,
    )
}

fn parse_all_pages(
    xml: &str,
    width: f64,
    height: f64,
    rotation: i32,
) -> Result<PageParse, (&'static str, String)> {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut maps = Vec::new();
    let mut page_lines: Vec<String> = Vec::new();
    let mut page_number = 1u32;
    let mut current_words: Vec<MapWord> = Vec::new();
    let mut current_line_words: Vec<String> = Vec::new();
    let mut page_width = width;
    let mut page_height = height;
    let mut in_page = false;
    let mut in_line = false;
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(quick_xml::events::Event::Start(element)) => {
                let name = element.name();
                let name_bytes = name.as_ref();
                if name_bytes == b"page" {
                    in_page = true;
                    page_width = attribute_f64(&element, b"width").unwrap_or(width);
                    page_height = attribute_f64(&element, b"height").unwrap_or(height);
                    current_words = Vec::new();
                    current_line_words = Vec::new();
                } else if in_page && name_bytes == b"line" {
                    in_line = true;
                    current_line_words = Vec::new();
                } else if in_page && in_line && name_bytes == b"word" {
                    let x_min = attribute_f64(&element, b"xMin").unwrap_or(0.0);
                    let y_min = attribute_f64(&element, b"yMin").unwrap_or(0.0);
                    let x_max = attribute_f64(&element, b"xMax").unwrap_or(0.0);
                    let y_max = attribute_f64(&element, b"yMax").unwrap_or(0.0);
                    let text = word_text(&mut reader)?;
                    current_words.push(MapWord {
                        text: text.clone(),
                        rect: BoundingRect {
                            x_min,
                            y_min,
                            x_max,
                            y_max,
                        },
                    });
                    current_line_words.push(text);
                }
            }
            Ok(quick_xml::events::Event::End(element)) => {
                let name = element.name();
                let name_bytes = name.as_ref();
                if name_bytes == b"line" && in_page && in_line {
                    // A visual line becomes one extracted-text line, preserving
                    // the granularity the diff and citation machinery expects.
                    page_lines.push(current_line_words.join(" "));
                    in_line = false;
                    current_line_words = Vec::new();
                } else if name_bytes == b"page" && in_page {
                    let spec =
                        PageSpec::new("", page_number, page_width, page_height, rotation, "", "");
                    let mut map = TextMap {
                        id: String::new(),
                        evidence_id: spec.evidence_id.clone(),
                        page_number,
                        page_width: spec.page_width,
                        page_height: spec.page_height,
                        page_rotation: spec.page_rotation,
                        coordinate_system: COORDINATE_SYSTEM.to_owned(),
                        words: std::mem::take(&mut current_words),
                        extractor: "poppler_pdftotext_bbox".to_owned(),
                        extractor_version: spec.extractor_version.clone(),
                        digest: String::new(),
                        source_digest: spec.source_digest.clone(),
                    };
                    let digest = map.compute_digest();
                    map.digest = digest;
                    maps.push(map);
                    page_lines.push(String::new());
                    page_number += 1;
                    in_page = false;
                    in_line = false;
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(error) => return Err(("pdf_bbox", error.to_string())),
            _ => {}
        }
        buffer.clear();
    }
    Ok(PageParse {
        maps,
        lines: page_lines,
    })
}

/// The result of parsing a PDF's bbox layout: the page-accurate text maps and a
/// line-separated text mirroring the visual line structure.
struct PageParse {
    maps: Vec<TextMap>,
    lines: Vec<String>,
}

fn attribute_f64(element: &quick_xml::events::BytesStart<'_>, key: &[u8]) -> Option<f64> {
    element
        .attributes()
        .flatten()
        .find(|attribute| attribute.key.as_ref() == key)
        .and_then(|attribute| {
            std::str::from_utf8(attribute.value.as_ref())
                .ok()
                .and_then(|value| value.parse().ok())
        })
}

fn word_text(reader: &mut quick_xml::Reader<&[u8]>) -> Result<String, (&'static str, String)> {
    let mut text = String::new();
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(quick_xml::events::Event::Text(value)) => {
                if let Ok(decoded) = value.decode() {
                    text.push_str(&decoded);
                }
            }
            Ok(quick_xml::events::Event::End(element)) => {
                if element.name().as_ref() == b"word" {
                    break;
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(error) => return Err(("pdf_bbox", error.to_string())),
            _ => {}
        }
        buffer.clear();
    }
    Ok(text.trim().to_owned())
}

fn ocr_pdf(
    sandbox: &dyn Sandbox,
    budgets: &mut JobBudgetTracker,
    input: &Path,
    pages: u32,
    dims: &PageDims,
    ocr_language: &str,
) -> ExtractionResult {
    if pages > MAX_OCR_PAGES {
        return Err((
            "ocr_page_limit",
            format!("OCR is limited to {MAX_OCR_PAGES} pages"),
        ));
    }
    budgets
        .add_ocr_pages(pages)
        .map_err(|error| budget_err(&error))?;
    let workdir = sandbox.working_dir().to_path_buf();
    let ocr_dir = workdir.join("ocr");
    fs::create_dir_all(&ocr_dir).map_err(external_io)?;
    let prefix = ocr_dir.join("page");
    let last_page = pages.to_string();
    budgets
        .add_child_process()
        .map_err(|error| budget_err(&error))?;
    let _ = run_sandboxed(
        sandbox,
        budgets,
        "pdftoppm",
        &[
            std::ffi::OsStr::new("-png"),
            std::ffi::OsStr::new("-r"),
            std::ffi::OsStr::new("200"),
            std::ffi::OsStr::new("-f"),
            std::ffi::OsStr::new("1"),
            std::ffi::OsStr::new("-l"),
            std::ffi::OsStr::new(&last_page),
            std::ffi::OsStr::new("-scale-to"),
            std::ffi::OsStr::new("4000"),
            input.as_os_str(),
            prefix.as_os_str(),
        ],
        &[input],
    )?;
    let mut images: Vec<PathBuf> = fs::read_dir(&ocr_dir)
        .map_err(external_io)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "png"))
        .collect();
    images.sort();
    let mut text = String::new();
    let mut maps = Vec::new();
    for (index, image) in images.iter().enumerate() {
        let page_number = u32::try_from(index + 1).unwrap_or(u32::MAX);
        let tsv_base = ocr_dir.join(format!("page-{index}"));
        budgets
            .add_child_process()
            .map_err(|error| budget_err(&error))?;
        let _ = run_sandboxed(
            sandbox,
            budgets,
            "tesseract",
            &[
                image.as_os_str(),
                tsv_base.as_os_str(),
                std::ffi::OsStr::new("-l"),
                std::ffi::OsStr::new(ocr_language),
                std::ffi::OsStr::new("tsv"),
            ],
            &[image],
        )?;
        let tsv_path = PathBuf::from(format!("{}.tsv", tsv_base.display()));
        let tsv_content = fs::read_to_string(&tsv_path).map_err(external_io)?;
        let image_dims = image_dimensions(image)?;
        let spec = PageSpec::new(
            "",
            page_number,
            dims.width,
            dims.height,
            dims.rotation,
            "",
            "",
        );
        let map = parse_ocr_tsv(&tsv_content, &spec, image_dims.0, image_dims.1)
            .map_err(|e| ("ocr_geometry", e.to_string()))?;
        let page_text = map
            .words
            .iter()
            .map(|word| word.text.clone())
            .collect::<Vec<_>>()
            .join(" ");
        maps.push(map);
        text.push_str(&page_text);
        text.push('\n');
    }
    if text.trim().is_empty() {
        Err(("ocr_empty", "OCR produced no text".to_owned()))
    } else {
        Ok((
            normalize_text(&text),
            "poppler_tesseract_ocr".to_owned(),
            ExtractionStatus::CompleteWithOcr,
            maps,
        ))
    }
}

fn image_dimensions(path: &Path) -> Result<(u32, u32), (&'static str, String)> {
    let image = image::open(path).map_err(|e| ("ocr_image", e.to_string()))?;
    Ok((image.width(), image.height()))
}

fn external_io(error: impl std::fmt::Display) -> (&'static str, String) {
    ("extractor_io", format!("{error}"))
}

fn budget_err(error: &BudgetError) -> (&'static str, String) {
    ("aggregate_budget", error.to_string())
}

fn parse_page_count(output: &str) -> Option<u32> {
    output.lines().find_map(|line| {
        line.strip_prefix("Pages:")
            .and_then(|value| value.trim().parse().ok())
    })
}

fn parse_page_dimensions(output: &str) -> Option<(f64, f64)> {
    let line = output.lines().find(|line| line.starts_with("Page size:"))?;
    let mut parts = line.split_whitespace();
    parts.next()?;
    let width = parts.next()?.parse().ok()?;
    let height = parts.next()?.parse().ok()?;
    Some((width, height))
}

fn parse_page_rotation(output: &str) -> Option<i32> {
    output.lines().find_map(|line| {
        line.strip_prefix("Page rot:")
            .and_then(|value| value.trim().parse().ok())
    })
}

fn run_sandboxed(
    sandbox: &dyn Sandbox,
    budgets: &mut JobBudgetTracker,
    program: &str,
    args: &[&std::ffi::OsStr],
    readonly_inputs: &[&Path],
) -> Result<String, (&'static str, String)> {
    let started = Instant::now();
    let output = sandbox
        .run(program, args, readonly_inputs, EXTRACT_TIMEOUT)
        .map_err(|error| ("extractor", error.to_string()))?;
    let wall = started.elapsed().as_secs().max(1);
    budgets
        .add_wall_seconds(wall)
        .map_err(|error| budget_err(&error))?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Builds page-accurate citations for a quote against stored text maps.
pub fn cite_quote(
    store: &Store,
    evidence_id: &str,
    quote: &str,
    occurrence_index: usize,
) -> Result<PageCitation, IngestError> {
    let (record, _) = store.evidence(evidence_id)?;
    let maps = store.text_maps(evidence_id)?;
    for map in &maps {
        validate_text_map(map)?;
        let occurrences = find_occurrences(map, quote);
        if occurrence_index < occurrences.len() {
            let range = normalized_range(map, quote).unwrap_or((0, 0));
            return Ok(build_page_citation(
                map,
                quote,
                occurrence_index,
                range,
                None,
                &record.sha256,
            )?);
        }
    }
    Err(IngestError::Metadata(format!(
        "quote not found in any text map for {evidence_id}"
    )))
}

/// Bounded Legistar pagination and attachment discovery.
///
/// Fetches events one request at a time with hard limits on pages, events,
/// matters, and attachments per matter, stable deduplication by official
/// identifiers, detection of repeated/non-progressing pages, deterministic
/// ordering, and conditional requests.
pub struct LegistarPagination {
    pub page_size: u32,
    pub max_pages: u32,
    pub max_events: u32,
    pub max_matters: u32,
    pub max_attachments_per_matter: u32,
}

impl Default for LegistarPagination {
    fn default() -> Self {
        Self {
            page_size: 100,
            max_pages: 50,
            max_events: 2000,
            max_matters: 500,
            max_attachments_per_matter: 50,
        }
    }
}

/// The result of a bounded Legistar event pagination pass.
#[derive(Clone, Debug)]
pub struct PaginateEventOutcome {
    pub events: Vec<Value>,
    pub matters: Vec<Matter>,
    pub attachments: Vec<MatterAttachment>,
    pub pages_fetched: u32,
    pub stopped_reason: String,
}

impl LegistarPagination {
    /// Fetches Legistar event pages one request at a time and returns
    /// deduplicated events plus discovered matters and attachments.
    ///
    /// `fetch_page` receives the zero-based page offset and must return the
    /// raw page body (or an error). It is invoked at most once per page and
    /// never concurrently, so one request is in flight at a time. The
    /// aggregate budgets in `budgets` are enforced across the whole pass, so a
    /// hostile matter cannot multiply per-file limits into unlimited work.
    ///
    /// Hard caps: `max_pages` pages, `max_events` total events, `max_matters`
    /// total matters, and `max_attachments_per_matter` attachments per matter.
    /// Events are deduplicated by their official `EventId`; matters by their
    /// `EventItemMatterFile`. A repeated page or a page that adds no new
    /// events stops the pass (non-progressing pagination cannot loop forever).
    pub fn paginate_events<F>(
        &self,
        budgets: &mut JobBudgetTracker,
        mut fetch_page: F,
        source_id: &str,
        base_url: &str,
    ) -> Result<PaginateEventOutcome, IngestError>
    where
        F: FnMut(u32) -> Result<Vec<u8>, IngestError>,
    {
        if self.page_size == 0 || self.max_pages == 0 {
            return Err(IngestError::Metadata(
                "page size and maximum pages must be positive".to_owned(),
            ));
        }
        let mut events: Vec<Value> = Vec::new();
        let mut matters: Vec<Matter> = Vec::new();
        let mut attachments: Vec<MatterAttachment> = Vec::new();
        let mut seen_events: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut seen_matters: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut previous_ids: Vec<String> = Vec::new();
        let mut pages_fetched = 0u32;
        let mut stopped_reason = "max pages".to_owned();
        let mut offset = 0u32;

        for _page in 0..self.max_pages {
            let body = fetch_page(offset)?;
            budgets.add_downloaded_bytes(body.len() as u64)?;
            let value: Value = serde_json::from_slice(&body).map_err(|error| {
                IngestError::Metadata(format!("page is not valid Legistar JSON: {error}"))
            })?;
            let page_events = value.as_array().ok_or_else(|| {
                IngestError::Metadata("Legistar page must be a JSON array".to_owned())
            })?;
            pages_fetched += 1;

            let mut page_ids: Vec<String> = Vec::new();
            let mut new_on_page = 0usize;
            for event in page_events {
                let event_id = event
                    .get("EventId")
                    .and_then(Value::as_u64)
                    .map(|id| id.to_string())
                    .or_else(|| {
                        event
                            .get("EventId")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    })
                    .ok_or_else(|| {
                        IngestError::Metadata("event missing a valid official EventId".to_owned())
                    })?;
                page_ids.push(event_id.clone());
                if seen_events.insert(event_id.clone()) {
                    new_on_page += 1;
                    events.push(event.clone());
                    let (matter_list, attach_list) =
                        discover_matters_from_event(event, source_id, base_url)?;
                    for matter in matter_list {
                        if seen_matters.insert(matter.official_matter_id.clone()) {
                            matters.push(matter);
                        }
                    }
                    let per_matter_cap = self.max_attachments_per_matter as usize;
                    attachments.extend(attach_list.into_iter().take(per_matter_cap));
                    if events.len() >= self.max_events as usize {
                        "max events".clone_into(&mut stopped_reason);
                        return Ok(PaginateEventOutcome {
                            events,
                            matters,
                            attachments,
                            pages_fetched,
                            stopped_reason,
                        });
                    }
                    if matters.len() >= self.max_matters as usize {
                        "max matters".clone_into(&mut stopped_reason);
                        return Ok(PaginateEventOutcome {
                            events,
                            matters,
                            attachments,
                            pages_fetched,
                            stopped_reason,
                        });
                    }
                }
            }

            // Non-progressing pagination: an empty page or a page identical to
            // the previous one must terminate the loop.
            if new_on_page == 0 {
                "non-progressing page".clone_into(&mut stopped_reason);
                break;
            }
            if page_ids == previous_ids {
                "repeated page".clone_into(&mut stopped_reason);
                break;
            }
            previous_ids = page_ids;
            offset = offset.saturating_add(self.page_size);
        }

        Ok(PaginateEventOutcome {
            events,
            matters,
            attachments,
            pages_fetched,
            stopped_reason,
        })
    }
}

/// Discovers matters and attachments from a single Legistar event JSON
/// document, using only documented official fields and reviewed hosts.
pub fn discover_matters_from_event(
    event: &Value,
    source_id: &str,
    base_url: &str,
) -> Result<(Vec<Matter>, Vec<MatterAttachment>), IngestError> {
    let mut matters = Vec::new();
    let mut attachments = Vec::new();
    let items = event.get("EventItems").and_then(Value::as_array);
    if let Some(items) = items {
        for item in items {
            let matter_file = item
                .get("EventItemMatterFile")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if matter_file.trim().is_empty() {
                continue;
            }
            let title = item
                .get("EventItemTitle")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let matter_id = format!("{source_id}:{matter_file}");
            let matter = Matter {
                id: matter_id.clone(),
                source_id: source_id.to_owned(),
                official_matter_id: matter_file.to_owned(),
                title,
                status: item
                    .get("EventItemActionName")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                url: format!("{base_url}/Matters/{matter_file}"),
                document_role: pnull_core::DocumentRole::Agenda,
            };
            matters.push(matter);
            if let Some(file_list) = item.get("EventItemAttachments").and_then(Value::as_array) {
                for file in file_list {
                    let official_id = file
                        .get("MatterAttachmentId")
                        .and_then(Value::as_str)
                        .or_else(|| file.get("EventItemAttachmentId").and_then(Value::as_str))
                        .unwrap_or_default();
                    let name = file
                        .get("MatterAttachmentName")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    let hyperlink = file
                        .get("Hyperlink")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    // Only accept official reviewed hosts.
                    let url = Url::parse(&hyperlink)
                        .ok()
                        .filter(|url| url.scheme() == "https");
                    if let Some(url) = url {
                        attachments.push(MatterAttachment {
                            id: format!("{matter_id}:{official_id}"),
                            matter_id: matter_id.clone(),
                            official_id: official_id.to_string(),
                            name,
                            url: url.to_string(),
                            evidence_id: None,
                        });
                    }
                }
            }
        }
    }
    Ok((matters, attachments))
}

pub fn normalize_text(input: &str) -> String {
    let normalized_newlines = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut output = Vec::new();
    let mut previous_blank = true;
    for line in normalized_newlines.lines() {
        let collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ");
        let blank = collapsed.is_empty();
        if !blank || !previous_blank {
            output.push(collapsed);
        }
        previous_blank = blank;
    }
    while output.last().is_some_and(String::is_empty) {
        output.pop();
    }
    output.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pnull_core::SourceType;
    use tempfile::tempdir;

    fn request(mime_type: &str) -> IngestRequest {
        IngestRequest {
            jurisdiction: "Colorado Springs, Colorado".to_owned(),
            source_url: "https://example.test/document".to_owned(),
            source_type: SourceType::OfficialApi,
            document_title: "Test document".to_owned(),
            publication_date: Some("2025-10-27".to_owned()),
            retrieval_timestamp: "2026-08-16T00:00:00Z".to_owned(),
            mime_type: mime_type.to_owned(),
            original_filename: "document.txt".to_owned(),
            supersedes: None,
            enable_ocr: false,
            ocr_language: "eng".to_owned(),
            max_bytes: 1024,
        }
    }

    fn fake_sandbox() -> FakeSandbox {
        FakeSandbox::new(ExtractionSandboxConfig::defaults()).expect("fake sandbox")
    }

    fn budgets() -> JobBudgetTracker {
        JobBudgetTracker::new(Budgets::defaults())
    }

    fn build() -> BuildMetadata {
        BuildMetadata::local()
    }

    #[test]
    fn duplicate_ingestion_is_idempotent() {
        let dir = tempdir().expect("temporary directory");
        let store = Store::open(dir.path()).expect("store");
        let sandbox = fake_sandbox();
        let mut b = budgets();
        let first = ingest_bytes(
            &store,
            &sandbox,
            &mut b,
            &build(),
            &request("text/plain"),
            b"Axon body camera",
        )
        .expect("first");
        let mut b2 = budgets();
        let second = ingest_bytes(
            &store,
            &sandbox,
            &mut b2,
            &build(),
            &request("text/plain"),
            b"Axon body camera",
        )
        .expect("second");
        assert!(first.inserted);
        assert!(!second.inserted);
        assert_eq!(first.record.id, second.record.id);
    }

    #[test]
    fn html_extraction_never_includes_script_content() {
        let dir = tempdir().expect("temporary directory");
        let store = Store::open(dir.path()).expect("store");
        let html = br"<html><body><h1>Public agenda</h1><script>plate ABC123</script><p>Axon proposal</p></body></html>";
        let sandbox = fake_sandbox();
        let mut b = budgets();
        let outcome = ingest_bytes(
            &store,
            &sandbox,
            &mut b,
            &build(),
            &request("text/html"),
            html,
        )
        .expect("ingestion");
        assert!(outcome.extracted_text.contains("Public agenda"));
        assert!(outcome.extracted_text.contains("Axon proposal"));
        assert!(!outcome.extracted_text.contains("ABC123"));
    }

    #[test]
    fn extraction_failure_is_structured_and_persisted() {
        let dir = tempdir().expect("temporary directory");
        let store = Store::open(dir.path()).expect("store");
        let sandbox = fake_sandbox();
        let mut b = budgets();
        let outcome = ingest_bytes(
            &store,
            &sandbox,
            &mut b,
            &build(),
            &request("application/pdf"),
            b"not a pdf",
        )
        .expect("ingestion");
        assert_eq!(outcome.record.extraction_status, ExtractionStatus::Failed);
        assert_eq!(
            outcome
                .record
                .extraction_error
                .expect("structured error")
                .code,
            "malformed_pdf"
        );
    }

    #[test]
    fn oversized_input_is_rejected_before_parsing() {
        let dir = tempdir().expect("temporary directory");
        let store = Store::open(dir.path()).expect("store");
        let mut limited = request("text/plain");
        limited.max_bytes = 3;
        let sandbox = fake_sandbox();
        let mut b = budgets();
        assert!(matches!(
            ingest_bytes(&store, &sandbox, &mut b, &build(), &limited, b"four"),
            Err(IngestError::Oversized { .. })
        ));
    }

    #[test]
    fn malformed_metadata_is_rejected() {
        let dir = tempdir().expect("temporary directory");
        let store = Store::open(dir.path()).expect("store");
        let mut invalid = request("text/plain");
        invalid.original_filename = "../secret".to_owned();
        let sandbox = fake_sandbox();
        let mut b = budgets();
        assert!(matches!(
            ingest_bytes(&store, &sandbox, &mut b, &build(), &invalid, b"text"),
            Err(IngestError::Metadata(_))
        ));
    }

    #[test]
    fn real_pdf_fixture_is_extracted_by_poppler_in_sandbox() {
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = fs::read(workspace.join("fixtures/co/ordinance-25-93-draft.pdf"))
            .expect("official PDF fixture");
        let dir = tempdir().expect("temporary directory");
        let store = Store::open(dir.path()).expect("store");
        let mut pdf_request = request("application/pdf");
        pdf_request.max_bytes = DEFAULT_MAX_BYTES;
        pdf_request.original_filename = "ordinance.pdf".to_owned();
        let sandbox = RealSandbox::new(ExtractionSandboxConfig::defaults()).expect("bwrap sandbox");
        let mut b = budgets();
        let outcome = ingest_bytes(&store, &sandbox, &mut b, &build(), &pdf_request, &bytes)
            .expect("PDF ingestion");
        assert_eq!(outcome.record.extraction_status, ExtractionStatus::Complete);
        assert!(
            outcome
                .extracted_text
                .contains("POLICE DEPARTMENT TECHNOLOGY SURCHARGE")
        );
        assert!(!outcome.text_maps.is_empty());
        store.verify(&outcome.record.id).expect("stored digest");
    }

    #[test]
    fn official_legistar_json_fixture_extracts_event_actions() {
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = fs::read(workspace.join("fixtures/co/event-2660-final-vote.json"))
            .expect("official API fixture");
        let dir = tempdir().expect("temporary directory");
        let store = Store::open(dir.path()).expect("store");
        let mut json_request = request("application/json");
        json_request.max_bytes = DEFAULT_MAX_BYTES;
        json_request.original_filename = "event.json".to_owned();
        let sandbox = fake_sandbox();
        let mut b = budgets();
        let outcome = ingest_bytes(&store, &sandbox, &mut b, &build(), &json_request, &bytes)
            .expect("JSON ingestion");
        assert!(outcome.extracted_text.contains("Meeting date: 2025-11-25"));
        assert!(outcome.extracted_text.contains("Action: finally passed"));
    }

    #[test]
    fn official_legistar_matter_fixture_extracts_subject_fields() {
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = fs::read(workspace.join("fixtures/co2/matter-15-00663-ordinance-15-84.json"))
            .expect("official matter fixture");
        let dir = tempdir().expect("temporary directory");
        let store = Store::open(dir.path()).expect("store");
        let mut matter_request = request("application/json");
        matter_request.max_bytes = DEFAULT_MAX_BYTES;
        matter_request.original_filename = "matter.json".to_owned();
        let sandbox = fake_sandbox();
        let mut b = budgets();
        let outcome = ingest_bytes(&store, &sandbox, &mut b, &build(), &matter_request, &bytes)
            .expect("matter JSON ingestion");
        assert!(outcome.extracted_text.contains("Matter file: 15-00663"));
        assert!(outcome.extracted_text.contains("Matter type: Ordinance"));
        assert!(outcome.extracted_text.contains("Enactment number: 15-84"));
    }

    #[test]
    fn parser_limits_reject_excessive_pdf_metadata() {
        assert_eq!(parse_page_count("Pages: 101\nEncrypted: no"), Some(101));
        assert_eq!(parse_page_count("Pages: not-a-number"), None);
    }

    #[test]
    fn normalization_is_idempotent_for_arbitrary_boundaries() {
        let samples = ["", "a", "a\r\nb", "  a   b  ", "a\n\n\n b", "é\tvalue"];
        for sample in samples {
            let once = normalize_text(sample);
            assert_eq!(once, normalize_text(&once));
        }
    }

    fn event_page(ids: &[u64]) -> Vec<u8> {
        let events: Vec<Value> = ids
            .iter()
            .map(|id| {
                serde_json::json!({
                    "EventId": id,
                    "EventDate": "2025-11-25T00:00:00",
                    "EventItems": [{
                        "EventItemMatterFile": format!("25-{id}"),
                        "EventItemTitle": format!("Matter {id}"),
                        "EventItemActionName": "referred"
                    }]
                })
            })
            .collect();
        serde_json::to_vec(&events).expect("event page")
    }

    #[test]
    fn pagination_stops_at_configured_max_pages() {
        let config = LegistarPagination {
            page_size: 2,
            max_pages: 3,
            max_events: 100,
            max_matters: 100,
            max_attachments_per_matter: 5,
        };
        let mut budgets = JobBudgetTracker::new(Budgets::defaults());
        let mut calls = 0u64;
        let outcome = config
            .paginate_events(
                &mut budgets,
                |_offset| {
                    calls += 1;
                    Ok(event_page(&[calls, calls + 10]))
                },
                "co",
                "https://coloradosprings.legistar.com",
            )
            .expect("pagination");
        assert_eq!(outcome.pages_fetched, 3);
        assert_eq!(calls, 3);
        assert_eq!(outcome.stopped_reason, "max pages");
        assert_eq!(outcome.events.len(), 6);
    }

    #[test]
    fn repeated_page_cannot_create_an_infinite_loop() {
        let config = LegistarPagination::default();
        let mut budgets = JobBudgetTracker::new(Budgets::defaults());
        let mut calls = 0;
        // Every page returns the same event ids: non-progressing pagination.
        let outcome = config
            .paginate_events(
                &mut budgets,
                |_offset| {
                    calls += 1;
                    Ok(event_page(&[1, 2, 3]))
                },
                "co",
                "https://coloradosprings.legistar.com",
            )
            .expect("pagination");
        // It must stop after the first page because the second page adds no new events.
        assert_eq!(calls, 2);
        assert!(matches!(
            outcome.stopped_reason.as_str(),
            "non-progressing page" | "repeated page"
        ));
        assert_eq!(outcome.events.len(), 3);
    }

    #[test]
    fn pagination_deduplicates_events_by_official_id() {
        let config = LegistarPagination {
            page_size: 2,
            max_pages: 2,
            max_events: 100,
            max_matters: 100,
            max_attachments_per_matter: 5,
        };
        let mut budgets = JobBudgetTracker::new(Budgets::defaults());
        let pages = [event_page(&[1, 2]), event_page(&[2, 3])];
        let mut calls = 0;
        let outcome = config
            .paginate_events(
                &mut budgets,
                |_offset| {
                    let page = pages[calls].clone();
                    calls += 1;
                    Ok(page)
                },
                "co",
                "https://coloradosprings.legistar.com",
            )
            .expect("pagination");
        // Event 2 appears on both pages but must be counted once.
        let ids: Vec<u64> = outcome
            .events
            .iter()
            .map(|event| event["EventId"].as_u64().expect("id"))
            .collect();
        assert_eq!(ids, vec![1, 2, 3]);
        assert_eq!(outcome.pages_fetched, 2);
    }

    #[test]
    fn pagination_stops_at_max_events() {
        let config = LegistarPagination {
            page_size: 10,
            max_pages: 100,
            max_events: 3,
            max_matters: 100,
            max_attachments_per_matter: 5,
        };
        let mut budgets = JobBudgetTracker::new(Budgets::defaults());
        let outcome = config
            .paginate_events(
                &mut budgets,
                |offset| {
                    let base = u64::from(offset) + 1;
                    Ok(event_page(&[base, base + 1, base + 2, base + 3, base + 4]))
                },
                "co",
                "https://coloradosprings.legistar.com",
            )
            .expect("pagination");
        assert_eq!(outcome.stopped_reason, "max events");
        assert!(outcome.events.len() <= 3);
    }

    #[test]
    fn pagination_rejects_missing_official_ids_fail_closed() {
        let config = LegistarPagination::default();
        let mut budgets = JobBudgetTracker::new(Budgets::defaults());
        let bad: Vec<u8> =
            serde_json::to_vec(&[serde_json::json!({ "EventDate": "x" })]).expect("bad page");
        let result = config.paginate_events(
            &mut budgets,
            |_offset| Ok(bad.clone()),
            "co",
            "https://coloradosprings.legistar.com",
        );
        assert!(matches!(result, Err(IngestError::Metadata(_))));
    }

    #[test]
    fn discover_matters_uses_only_official_fields_and_https_hosts() {
        let event = serde_json::json!({
            "EventItems": [{
                "EventItemMatterFile": "25-581",
                "EventItemTitle": "Ordinance No. 25-93",
                "EventItemActionName": "finally passed",
                "EventItemAttachments": [
                    {"MatterAttachmentId": "1", "MatterAttachmentName": "draft.pdf", "Hyperlink": "https://coloradosprings.legistar.com/View.ashx?M=F&ID=1"},
                    {"MatterAttachmentId": "2", "MatterAttachmentName": "bad.pdf", "Hyperlink": "http://evil.example/View.ashx"}
                ]
            }]
        });
        let (matters, attachments) =
            discover_matters_from_event(&event, "co", "https://coloradosprings.legistar.com")
                .expect("discover");
        assert_eq!(matters.len(), 1);
        assert_eq!(matters[0].official_matter_id, "25-581");
        // Only the HTTPS official-host attachment is kept.
        assert_eq!(attachments.len(), 1);
        assert!(
            attachments[0]
                .url
                .starts_with("https://coloradosprings.legistar.com")
        );
    }

    #[test]
    fn aggregate_budget_blocks_many_attachments() {
        let dir = tempdir().expect("temporary directory");
        let store = Store::open(dir.path()).expect("store");
        let sandbox = fake_sandbox();
        let budgets = Budgets {
            max_total_attachments: 2,
            ..Budgets::defaults()
        };
        let mut tracker = JobBudgetTracker::new(budgets);
        let mut req = request("text/plain");
        req.max_bytes = DEFAULT_MAX_BYTES;
        ingest_bytes(&store, &sandbox, &mut tracker, &build(), &req, b"one").expect("1");
        ingest_bytes(&store, &sandbox, &mut tracker, &build(), &req, b"two").expect("2");
        assert!(matches!(
            ingest_bytes(&store, &sandbox, &mut tracker, &build(), &req, b"three"),
            Err(IngestError::Budget(BudgetError::Attachments { limit: 2 }))
        ));
    }
}
