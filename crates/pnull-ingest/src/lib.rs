//! Bounded ingestion and extraction of hostile public documents.

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use pnull_core::{
    EvidenceRecord, ExtractionStatus, Locator, PROCESSING_VERSION, SourceType, Store,
    StructuredError, evidence_id, sha256_hex,
};
use regex::Regex;
use reqwest::blocking::Client;
use reqwest::header::CONTENT_LENGTH;
use scraper::Html;
use serde_json::Value;
use tempfile::{NamedTempFile, TempDir};
use thiserror::Error;
use url::Url;
use wait_timeout::ChildExt;

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
    #[error(transparent)]
    Core(#[from] pnull_core::CoreError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug)]
struct Extraction {
    text: String,
    method: String,
    status: ExtractionStatus,
    error: Option<StructuredError>,
}

fn is_forbidden_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".localhost") {
        return true;
    }
    host.parse::<std::net::IpAddr>()
        .is_ok_and(|address| match address {
            std::net::IpAddr::V4(value) => {
                value.is_private()
                    || value.is_loopback()
                    || value.is_link_local()
                    || value.is_broadcast()
                    || value.is_unspecified()
            }
            std::net::IpAddr::V6(value) => {
                value.is_loopback() || value.is_unspecified() || value.is_unique_local()
            }
        })
}

pub fn fetch_public_source(source_url: &str, max_bytes: usize) -> Result<Vec<u8>, IngestError> {
    let url = Url::parse(source_url).map_err(|error| IngestError::Metadata(error.to_string()))?;
    let source_host = url.host_str().ok_or(IngestError::InsecureUrl)?.to_owned();
    if url.scheme() != "https" || is_forbidden_host(&source_host) {
        return Err(IngestError::InsecureUrl);
    }
    let redirect_host = source_host.clone();
    let redirect_policy = reqwest::redirect::Policy::custom(move |attempt| {
        let next = attempt.url();
        if attempt.previous().len() >= 5
            || next.scheme() != "https"
            || next.host_str() != Some(redirect_host.as_str())
            || next.host_str().is_some_and(is_forbidden_host)
        {
            attempt.error("redirect target violates the public same-host HTTPS policy")
        } else {
            attempt.follow()
        }
    });
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .redirect(redirect_policy)
        .user_agent(concat!(
            "PanopticonNull/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/panopticon-null/panopticon-null)"
        ))
        .build()
        .map_err(|_| IngestError::Network)?;
    let response = client
        .get(url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|_| IngestError::Network)?;
    if response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|length| length > max_bytes)
    {
        return Err(IngestError::Oversized {
            limit: max_bytes,
            observed: max_bytes.saturating_add(1),
        });
    }
    let mut bytes = Vec::with_capacity(max_bytes.min(1024 * 1024));
    response
        .take(u64::try_from(max_bytes).unwrap_or(u64::MAX) + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| IngestError::Network)?;
    if bytes.len() > max_bytes {
        return Err(IngestError::Oversized {
            limit: max_bytes,
            observed: bytes.len(),
        });
    }
    Ok(bytes)
}

pub fn ingest_bytes(
    store: &Store,
    request: &IngestRequest,
    bytes: &[u8],
) -> Result<IngestOutcome, IngestError> {
    validate_request(request)?;
    if bytes.len() > request.max_bytes {
        return Err(IngestError::Oversized {
            limit: request.max_bytes,
            observed: bytes.len(),
        });
    }
    let digest = sha256_hex(bytes);
    let id = evidence_id(&request.jurisdiction, &request.source_url, &digest);
    persist_content(store, &digest, bytes)?;
    let extraction = extract(
        bytes,
        &request.mime_type,
        request.enable_ocr,
        &request.ocr_language,
    );
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
        processing_version: PROCESSING_VERSION.to_owned(),
    };
    let inserted = store.insert_evidence(&record, &extraction.text)?;
    Ok(IngestOutcome {
        record,
        inserted,
        extracted_text: extraction.text,
    })
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

fn extract(bytes: &[u8], mime_type: &str, enable_ocr: bool, ocr_language: &str) -> Extraction {
    let result = match mime_type.split(';').next().unwrap_or_default().trim() {
        "text/plain" => extract_plain(bytes),
        "text/html" | "application/xhtml+xml" => extract_html(bytes),
        "application/json" => extract_legistar_json(bytes),
        "application/pdf" => extract_pdf(bytes, enable_ocr, ocr_language),
        other => Err((
            "unsupported_mime",
            format!("unsupported MIME type: {other}"),
        )),
    };
    let result = result.and_then(|(text, method, status)| {
        if text.len() > MAX_EXTRACTED_BYTES {
            Err((
                "extracted_text_limit",
                format!(
                    "extracted text is {} bytes; limit is {MAX_EXTRACTED_BYTES}",
                    text.len()
                ),
            ))
        } else {
            Ok((text, method, status))
        }
    });
    match result {
        Ok((text, method, status)) => Extraction {
            text,
            method,
            status,
            error: None,
        },
        Err((code, message)) => Extraction {
            text: String::new(),
            method: "none".to_owned(),
            status: ExtractionStatus::Failed,
            error: Some(StructuredError {
                code: code.to_owned(),
                message,
            }),
        },
    }
}

fn extract_plain(
    bytes: &[u8],
) -> Result<(String, String, ExtractionStatus), (&'static str, String)> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| ("invalid_utf8", "plain text is not valid UTF-8".to_owned()))?;
    Ok((
        normalize_text(text),
        "utf8_plain_text".to_owned(),
        ExtractionStatus::Complete,
    ))
}

fn extract_html(
    bytes: &[u8],
) -> Result<(String, String, ExtractionStatus), (&'static str, String)> {
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
    ))
}

fn extract_legistar_json(
    bytes: &[u8],
) -> Result<(String, String, ExtractionStatus), (&'static str, String)> {
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
        ));
    }
    let mut lines = Vec::new();
    match &value {
        Value::Array(events) => {
            for event in events {
                extract_event(event, &mut lines)?;
            }
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
        "legistar_event_json".to_owned(),
        ExtractionStatus::Complete,
    ))
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
    bytes: &[u8],
    enable_ocr: bool,
    ocr_language: &str,
) -> Result<(String, String, ExtractionStatus), (&'static str, String)> {
    if !bytes.starts_with(b"%PDF-") {
        return Err(("malformed_pdf", "PDF signature is missing".to_owned()));
    }
    let input = NamedTempFile::new().map_err(external_io)?;
    fs::write(input.path(), bytes).map_err(external_io)?;
    let info = run_limited("pdfinfo", &[input.path().as_os_str()], EXTRACT_TIMEOUT)?;
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
    let output = NamedTempFile::new().map_err(external_io)?;
    run_limited(
        "pdftotext",
        &[
            std::ffi::OsStr::new("-raw"),
            input.path().as_os_str(),
            output.path().as_os_str(),
        ],
        EXTRACT_TIMEOUT,
    )?;
    let text = fs::read_to_string(output.path()).map_err(external_io)?;
    if !text.trim().is_empty() {
        return Ok((
            normalize_text(&text),
            "poppler_pdftotext".to_owned(),
            ExtractionStatus::Complete,
        ));
    }
    if !enable_ocr {
        return Err((
            "scanned_pdf_ocr_disabled",
            "PDF contains no extractable text; optional OCR was not enabled".to_owned(),
        ));
    }
    ocr_pdf(input.path(), pages, ocr_language)
}

fn ocr_pdf(
    input: &Path,
    pages: u32,
    ocr_language: &str,
) -> Result<(String, String, ExtractionStatus), (&'static str, String)> {
    if pages > MAX_OCR_PAGES {
        return Err((
            "ocr_page_limit",
            format!("OCR is limited to {MAX_OCR_PAGES} pages"),
        ));
    }
    let directory = TempDir::new().map_err(external_io)?;
    let prefix = directory.path().join("page");
    let last_page = pages.to_string();
    run_limited(
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
        EXTRACT_TIMEOUT,
    )?;
    let mut images: Vec<PathBuf> = fs::read_dir(directory.path())
        .map_err(external_io)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "png"))
        .collect();
    images.sort();
    let mut text = String::new();
    for image in images {
        let output = NamedTempFile::new().map_err(external_io)?;
        run_limited(
            "tesseract",
            &[
                image.as_os_str(),
                output.path().as_os_str(),
                std::ffi::OsStr::new("-l"),
                std::ffi::OsStr::new(ocr_language),
            ],
            EXTRACT_TIMEOUT,
        )?;
        let txt_path = PathBuf::from(format!("{}.txt", output.path().display()));
        text.push_str(&fs::read_to_string(txt_path).map_err(external_io)?);
        text.push('\n');
    }
    if text.trim().is_empty() {
        Err(("ocr_empty", "OCR produced no text".to_owned()))
    } else {
        Ok((
            normalize_text(&text),
            "poppler_tesseract_ocr".to_owned(),
            ExtractionStatus::CompleteWithOcr,
        ))
    }
}

fn external_io(error: impl std::fmt::Display) -> (&'static str, String) {
    ("extractor_io", format!("{error}"))
}

fn parse_page_count(output: &str) -> Option<u32> {
    output.lines().find_map(|line| {
        line.strip_prefix("Pages:")
            .and_then(|value| value.trim().parse().ok())
    })
}

fn resolve_executable(name: &str) -> Result<PathBuf, (&'static str, String)> {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|directory| directory.join(name))
        .find(|path| path.is_file())
        .ok_or((
            "extractor_unavailable",
            format!("required allowlisted extractor is unavailable: {name}"),
        ))
}

fn run_limited(
    program: &str,
    args: &[&std::ffi::OsStr],
    timeout: Duration,
) -> Result<String, (&'static str, String)> {
    let prlimit = resolve_executable("prlimit")?;
    let executable = resolve_executable(program)?;
    let output = NamedTempFile::new().map_err(external_io)?;
    let error = NamedTempFile::new().map_err(external_io)?;
    let stdout = output.reopen().map_err(external_io)?;
    let stderr = error.reopen().map_err(external_io)?;
    let mut command = Command::new(prlimit);
    command
        .args([
            "--as=536870912",
            "--cpu=12",
            "--fsize=52428800",
            "--nproc=16",
            "--",
        ])
        .arg(executable)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .env_clear()
        .env("LANG", "C.UTF-8")
        .env("LC_ALL", "C.UTF-8")
        .env("OMP_THREAD_LIMIT", "1");
    let mut child = command.spawn().map_err(external_io)?;
    match child.wait_timeout(timeout).map_err(external_io)? {
        Some(status) if status.success() => fs::read_to_string(output.path()).map_err(external_io),
        Some(_) => Err((
            "extractor_failed",
            format!("allowlisted extractor {program} returned an error"),
        )),
        None => {
            child.kill().map_err(external_io)?;
            let _ = child.wait();
            Err((
                "extractor_timeout",
                format!("allowlisted extractor {program} exceeded its time limit"),
            ))
        }
    }
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

    #[test]
    fn live_fetch_rejects_private_and_insecure_targets() {
        assert!(matches!(
            fetch_public_source("http://example.test", 1024),
            Err(IngestError::InsecureUrl)
        ));
        assert!(matches!(
            fetch_public_source("https://127.0.0.1/private", 1024),
            Err(IngestError::InsecureUrl)
        ));
        assert!(matches!(
            fetch_public_source("https://localhost/private", 1024),
            Err(IngestError::InsecureUrl)
        ));
    }

    #[test]
    fn duplicate_ingestion_is_idempotent() {
        let dir = tempdir().expect("temporary directory");
        let store = Store::open(dir.path()).expect("store");
        let first = ingest_bytes(&store, &request("text/plain"), b"Axon body camera")
            .expect("first ingestion");
        let second = ingest_bytes(&store, &request("text/plain"), b"Axon body camera")
            .expect("second ingestion");
        assert!(first.inserted);
        assert!(!second.inserted);
        assert_eq!(first.record.id, second.record.id);
    }

    #[test]
    fn html_extraction_never_includes_script_content() {
        let dir = tempdir().expect("temporary directory");
        let store = Store::open(dir.path()).expect("store");
        let html = br"<html><body><h1>Public agenda</h1><script>plate ABC123</script><p>Axon proposal</p></body></html>";
        let outcome = ingest_bytes(&store, &request("text/html"), html).expect("ingestion");
        assert!(outcome.extracted_text.contains("Public agenda"));
        assert!(outcome.extracted_text.contains("Axon proposal"));
        assert!(!outcome.extracted_text.contains("ABC123"));
    }

    #[test]
    fn extraction_failure_is_structured_and_persisted() {
        let dir = tempdir().expect("temporary directory");
        let store = Store::open(dir.path()).expect("store");
        let outcome = ingest_bytes(&store, &request("application/pdf"), b"not a pdf")
            .expect("ingestion itself succeeds");
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
        assert!(matches!(
            ingest_bytes(&store, &limited, b"four"),
            Err(IngestError::Oversized { .. })
        ));
    }

    #[test]
    fn malformed_metadata_is_rejected() {
        let dir = tempdir().expect("temporary directory");
        let store = Store::open(dir.path()).expect("store");
        let mut invalid = request("text/plain");
        invalid.original_filename = "../secret".to_owned();
        assert!(matches!(
            ingest_bytes(&store, &invalid, b"text"),
            Err(IngestError::Metadata(_))
        ));
    }

    #[test]
    fn real_pdf_fixture_is_extracted_by_poppler() {
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = fs::read(workspace.join("fixtures/co/ordinance-25-93-draft.pdf"))
            .expect("official PDF fixture");
        let dir = tempdir().expect("temporary directory");
        let store = Store::open(dir.path()).expect("store");
        let mut pdf_request = request("application/pdf");
        pdf_request.max_bytes = DEFAULT_MAX_BYTES;
        pdf_request.original_filename = "ordinance.pdf".to_owned();
        let outcome = ingest_bytes(&store, &pdf_request, &bytes).expect("PDF ingestion");
        assert_eq!(outcome.record.extraction_status, ExtractionStatus::Complete);
        assert!(
            outcome
                .extracted_text
                .contains("POLICE DEPARTMENT TECHNOLOGY SURCHARGE")
        );
        store.verify(&outcome.record.id).expect("stored digest");
    }

    #[test]
    fn optional_ocr_extracts_an_image_only_pdf() {
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = fs::read(workspace.join("fixtures/pdf/scanned-surveillance-text.pdf"))
            .expect("scanned PDF fixture");
        let dir = tempdir().expect("temporary directory");
        let store = Store::open(dir.path()).expect("store");
        let mut pdf_request = request("application/pdf");
        pdf_request.max_bytes = DEFAULT_MAX_BYTES;
        pdf_request.original_filename = "scanned.pdf".to_owned();
        pdf_request.enable_ocr = true;
        let languages = Command::new("tesseract")
            .arg("--list-langs")
            .output()
            .expect("list OCR languages");
        pdf_request.ocr_language = String::from_utf8_lossy(&languages.stdout)
            .lines()
            .find(|line| !line.starts_with("List of") && *line != "osd" && !line.is_empty())
            .expect("at least one recognition language")
            .to_owned();
        let outcome = ingest_bytes(&store, &pdf_request, &bytes).expect("OCR ingestion");
        assert_eq!(
            outcome.record.extraction_status,
            ExtractionStatus::CompleteWithOcr,
            "{:?}",
            outcome.record.extraction_error
        );
        assert!(outcome.extracted_text.to_lowercase().contains("technology"));
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
        let outcome = ingest_bytes(&store, &json_request, &bytes).expect("JSON ingestion");
        assert!(outcome.extracted_text.contains("Meeting date: 2025-11-25"));
        assert!(outcome.extracted_text.contains("Action: finally passed"));

        let collection = [b"[".as_slice(), bytes.as_slice(), b"]".as_slice()].concat();
        let mut collection_request = json_request;
        collection_request.source_url = "https://example.test/events".to_owned();
        let collection_outcome =
            ingest_bytes(&store, &collection_request, &collection).expect("collection ingestion");
        assert!(
            collection_outcome
                .extracted_text
                .contains("Action: finally passed")
        );
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
}
