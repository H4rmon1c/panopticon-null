//! Sandboxed screening of privately captured research documents.
//!
//! This command emits a preliminary keyword finding, not an approval, vote, or
//! publication authorization. Network capture and its receipt are maintained
//! by the discovery queue; this command never fetches a URL.

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use anyhow::{Context, Result, bail};
use pnull_core::{ExtractionStatus, SourceType, Store, sha256_hex};
use pnull_detect::{load_rules, rules_digest, scan};
use pnull_ingest::{Budgets, BuildMetadata, IngestRequest, RealSandbox, Tracker, ingest_bytes};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

const MAX_PDF: u64 = 20 * 1024 * 1024;
// Linux O_NOFOLLOW: the document is untrusted and must not redirect reads.
const NOFOLLOW: i32 = 0o400_000;

fn valid_jurisdiction_ref(value: &str) -> bool {
    let colorado = value.starts_with("us-co-")
        && value.len() <= 100
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    let district = value
        .strip_prefix("dola-district-")
        .is_some_and(|id| id.len() == 5 && id.bytes().all(|byte| byte.is_ascii_digit()));
    colorado || district
}

fn read_input(
    data_dir: &Path,
    digest: &str,
    source_url: &str,
    jurisdiction_ref: &str,
    retrieved_at: &str,
) -> Result<Vec<u8>> {
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        bail!("invalid document digest");
    }
    if !valid_jurisdiction_ref(jurisdiction_ref) {
        bail!("invalid Colorado jurisdiction reference");
    }
    let url = url::Url::parse(source_url)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || source_url.len() > 2048
    {
        bail!("research source must be bounded public HTTPS");
    }
    OffsetDateTime::parse(retrieved_at, &Rfc3339).context("invalid capture timestamp")?;
    for path in [data_dir.to_path_buf(), data_dir.join("content")] {
        let meta = fs::symlink_metadata(&path)?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            bail!("unsafe research directory");
        }
    }
    let path = data_dir.join("content").join(digest);
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(NOFOLLOW)
        .open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > MAX_PDF {
        bail!("PDF exceeds its byte bound");
    }
    let mut bytes = Vec::new();
    file.take(MAX_PDF + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_PDF || !bytes.starts_with(b"%PDF-") || sha256_hex(&bytes) != digest
    {
        bail!("PDF bytes do not match the preserved digest");
    }
    Ok(bytes)
}

pub(super) fn screen_pdf(
    data_dir: &Path,
    rules_path: &Path,
    digest: &str,
    source_url: &str,
    jurisdiction_ref: &str,
    retrieved_at: &str,
    ocr: bool,
) -> Result<()> {
    let bytes = read_input(data_dir, digest, source_url, jurisdiction_ref, retrieved_at)?;
    let rules = load_rules(rules_path)?;
    let rule_digest = rules_digest(&rules);
    let sandbox = RealSandbox::new(pnull_ingest::ExtractionSandboxConfig::defaults())
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    // Separate temporary evidence stores keep failed extraction attempts from
    // poisoning the first-write-only evidence row for a later successful retry.
    let temporary = tempfile::Builder::new()
        .prefix("research-extract-")
        .tempdir()?;
    let store = Store::open(temporary.path())?;
    let mut tracker = Tracker::new(Budgets::defaults());
    let mut build = BuildMetadata::local();
    build.rules_digest.clone_from(&rule_digest);
    "preliminary-machine-discovery".clone_into(&mut build.state_config_digest);
    let result = ingest_bytes(
        &store,
        &sandbox,
        &mut tracker,
        &build,
        &IngestRequest {
            jurisdiction: jurisdiction_ref.to_owned(),
            source_url: source_url.to_owned(),
            source_type: SourceType::Pdf,
            document_title: "Public meeting document".to_owned(),
            publication_date: None,
            retrieval_timestamp: retrieved_at.to_owned(),
            mime_type: "application/pdf".to_owned(),
            original_filename: "meeting.pdf".to_owned(),
            supersedes: None,
            enable_ocr: ocr,
            ocr_language: "eng".to_owned(),
            max_bytes: usize::try_from(MAX_PDF)?,
        },
        &bytes,
    )?;
    if !matches!(
        result.record.extraction_status,
        ExtractionStatus::Complete | ExtractionStatus::CompleteWithOcr
    ) || result.extracted_text.trim().is_empty()
    {
        let code = result
            .record
            .extraction_error
            .as_ref()
            .map_or("unknown", |error| error.code.as_str());
        let message = result
            .record
            .extraction_error
            .as_ref()
            .map_or("", |error| error.message.as_str());
        bail!(
            "PDF text extraction incomplete ({code}): {message}; no no-match conclusion is available"
        );
    }
    store.verify(&result.record.id)?;
    let text_digest = preserve_extraction(data_dir, &result.extracted_text)?;
    let finding = scan(&result.record, &result.extracted_text, &rules);
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "state": if finding.is_some() { "matched" } else { "no_match" },
            "evidence_id": result.record.id, "document_sha256": digest,
            "extracted_sha256": text_digest, "rules_digest": rule_digest,
            "extraction_status": result.record.extraction_status,
            "ocr_page_set_validated": result.record.extraction_status == ExtractionStatus::CompleteWithOcr,
            "ocr_page_text_mapping_version": if result.record.extraction_status == ExtractionStatus::CompleteWithOcr { 2 } else { 0 },
            "finding": finding,
        }))?
    );
    Ok(())
}

fn validate_cgroup_limits(memory: &str, swap: &str, pids: &str, cpu: &str) -> Result<()> {
    let memory: u64 = memory
        .trim()
        .parse()
        .context("bounded cgroup memory.max required")?;
    let pids: u64 = pids
        .trim()
        .parse()
        .context("bounded cgroup pids.max required")?;
    let values = cpu
        .split_whitespace()
        .map(str::parse::<u64>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("bounded cgroup cpu.max required")?;
    if memory == 0
        || memory > 512 * 1024 * 1024
        || swap.trim() != "0"
        || pids == 0
        || pids > 16
        || values.len() != 2
        || values[0] == 0
        || values[1] == 0
        || values[0] > values[1]
    {
        bail!("private page OCR requires memory <=512 MiB, zero swap, <=16 tasks, and <=1 CPU");
    }
    Ok(())
}

fn require_bounded_cgroup() -> Result<()> {
    use std::path::Component;
    let membership = fs::read_to_string("/proc/self/cgroup")?;
    let relative = membership
        .lines()
        .find_map(|line| line.strip_prefix("0::/"))
        .context("cgroup v2 membership required")?;
    let relative = Path::new(relative);
    if !relative
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
    {
        bail!("invalid cgroup path");
    }
    let dir = Path::new("/sys/fs/cgroup").join(relative);
    validate_cgroup_limits(
        &fs::read_to_string(dir.join("memory.max"))?,
        &fs::read_to_string(dir.join("memory.swap.max"))?,
        &fs::read_to_string(dir.join("pids.max"))?,
        &fs::read_to_string(dir.join("cpu.max"))?,
    )
}

fn require_bounded_scratch() -> Result<()> {
    if std::env::temp_dir() != Path::new("/tmp") {
        bail!("private page OCR requires /tmp as its isolated scratch directory");
    }
    let info = fs::read_to_string("/proc/self/mountinfo")?;
    let entry = info
        .lines()
        .filter_map(|line| {
            let (mount, fsinfo) = line.split_once(" - ")?;
            if mount.split_whitespace().nth(4)? != "/tmp" {
                return None;
            }
            let mut parts = fsinfo.split_whitespace();
            if parts.next()? != "tmpfs" {
                return Some(None);
            }
            let _source = parts.next()?;
            let options = parts.next()?;
            Some(
                options
                    .split(',')
                    .find_map(|option| option.strip_prefix("size=")),
            )
        })
        .next_back()
        .flatten()
        .context("bounded tmpfs mounted at /tmp required")?;
    let (digits, multiplier) = if let Some(value) = entry.strip_suffix('k') {
        (value, 1024u64)
    } else if let Some(value) = entry.strip_suffix('m') {
        (value, 1024u64 * 1024)
    } else {
        (entry, 1u64)
    };
    let size = digits
        .parse::<u64>()?
        .checked_mul(multiplier)
        .context("tmpfs size overflow")?;
    if size == 0 || size > 128 * 1024 * 1024 {
        bail!("private page OCR tmpfs must be at most 128 MiB");
    }
    Ok(())
}

/// One isolated page; output is private and cannot be consumed as a release.
pub(super) fn screen_page(
    data_dir: &Path,
    digest: &str,
    source_url: &str,
    jurisdiction_ref: &str,
    retrieved_at: &str,
    page: u32,
) -> Result<()> {
    require_bounded_cgroup()?;
    require_bounded_scratch()?;
    let bytes = read_input(data_dir, digest, source_url, jurisdiction_ref, retrieved_at)?;
    let mut sandbox_config = pnull_ingest::ExtractionSandboxConfig::defaults();
    sandbox_config.page_mode = true;
    let sandbox =
        RealSandbox::new(sandbox_config).map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let mut tracker = Tracker::new(Budgets::defaults());
    let (pages, native_text, ocr_text) =
        pnull_ingest::private_research_page(&sandbox, &mut tracker, &bytes, page)
            .map_err(|error| anyhow::anyhow!(error))?;
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "state": "private_provisional_page",
            "notice": "Native text and OCR attempted; OCR is imperfect and this is not publication or a no-match conclusion.",
            "document_sha256": digest, "source_url": source_url, "jurisdiction_ref": jurisdiction_ref,
            "retrieved_at": retrieved_at, "page": page, "page_count": pages,
            "native_text": native_text, "ocr_text": ocr_text,
            "method": "pnull-research-page-v1; poppler-pdftotext-layout; pdftoppm-200dpi-4000max; tesseract-eng-tsv"
        }))?
    );
    Ok(())
}

fn preserve_extraction(data_dir: &Path, text: &str) -> Result<String> {
    let text_digest = sha256_hex(text.as_bytes());
    let extracted = data_dir.join("extracted");
    fs::create_dir_all(&extracted)?;
    if fs::symlink_metadata(&extracted)?.file_type().is_symlink() {
        bail!("unsafe extracted directory");
    }
    let output = extracted.join(&text_digest);
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(NOFOLLOW)
        .mode(0o600)
        .open(&output)
    {
        Ok(mut file) => {
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let mut existing = Vec::new();
            OpenOptions::new()
                .read(true)
                .custom_flags(NOFOLLOW)
                .open(&output)?
                .take(5 * 1024 * 1024 + 1)
                .read_to_end(&mut existing)?;
            if existing != text.as_bytes() {
                bail!("stored extraction differs");
            }
        }
        Err(error) => return Err(error.into()),
    }
    Ok(text_digest)
}

#[cfg(test)]
mod tests {
    use super::{valid_jurisdiction_ref, validate_cgroup_limits};

    #[test]
    fn private_page_rejects_unbounded_cgroup_settings() {
        assert!(validate_cgroup_limits("536870912", "0", "16", "100000 100000").is_ok());
        for (memory, swap, pids, cpu) in [
            ("max", "0", "16", "100000 100000"),
            ("536870913", "0", "16", "100000 100000"),
            ("536870912", "max", "16", "100000 100000"),
            ("536870912", "0", "max", "100000 100000"),
            ("536870912", "0", "17", "100000 100000"),
            ("536870912", "0", "16", "max 100000"),
            ("536870912", "0", "16", "200000 100000"),
        ] {
            assert!(validate_cgroup_limits(memory, swap, pids, cpu).is_err());
        }
    }

    #[test]
    fn accepts_only_five_digit_dola_district_identifiers() {
        assert!(valid_jurisdiction_ref("dola-district-01006"));
        assert!(valid_jurisdiction_ref("us-co-county-08001"));
        for invalid in [
            "dola-district-1006",
            "dola-district-010060",
            "dola-district-01a06",
            "dola-district-01006/../other",
            "DOLA-district-01006",
        ] {
            assert!(!valid_jurisdiction_ref(invalid), "{invalid}");
        }
    }
}
