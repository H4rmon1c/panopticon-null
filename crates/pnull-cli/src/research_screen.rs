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
    use super::valid_jurisdiction_ref;

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
