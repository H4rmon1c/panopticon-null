//! Safe import path for operator-supplied public records (manual or via CORA).
//!
//! All supplied files are treated as hostile input. The importer requires a
//! declared source URL or records-request identifier, acquisition date, document
//! role, an operator declaration of lawful possession, and an exact file digest,
//! and it never publishes without human review.

use pnull_core::{sha256_hex, stable_id};
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("import refused: {0}")]
    Refused(String),
    #[error("input exceeds the configured {limit}-byte limit (observed {observed})")]
    Oversized { limit: usize, observed: usize },
    #[error("digest mismatch: declared {declared}, observed {observed}")]
    DigestMismatch { declared: String, observed: String },
    #[error("file read failed: {0}")]
    Io(#[from] std::io::Error),
}

/// Metadata an operator must declare before a record is accepted.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SuppliedRecordDeclaration {
    /// A declared source URL, or a records-request (CORA) identifier.
    pub source_or_request_id: String,
    pub acquisition_date: String,
    /// The document role (e.g., contract, award, solicitation, expenditure).
    pub document_role: String,
    /// Operator declaration of lawful possession.
    pub lawful_possession: bool,
    /// The declared SHA-256 digest of the exact file bytes.
    pub declared_digest: String,
    pub operator: String,
}

/// A validated operator-supplied record ready for ingestion and review.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SuppliedRecord {
    pub id: String,
    pub declaration: SuppliedRecordDeclaration,
    /// SHA-256 of the exact bytes supplied.
    pub observed_digest: String,
    pub byte_count: usize,
    /// The public-address source this record was lawfully obtained from, when
    /// the source is a URL.
    pub source_url: Option<String>,
    /// True only after human review permits the record to be published.
    pub human_reviewed: bool,
    pub processing_provenance: String,
}

/// Validates an operator-supplied file against its declaration and stores the
/// bytes into the content-addressed store path, returning a provenance record.
///
/// `max_bytes` bounds hostile oversized inputs. The returned `SuppliedRecord` is
/// never published until a human review decision marks it reviewed.
pub fn import_supplied_record(
    data_dir: &Path,
    file_path: &Path,
    declaration: &SuppliedRecordDeclaration,
    max_bytes: usize,
) -> Result<SuppliedRecord, ImportError> {
    if !declaration.lawful_possession {
        return Err(ImportError::Refused(
            "operator must declare lawful possession".to_owned(),
        ));
    }
    if declaration.source_or_request_id.trim().is_empty() {
        return Err(ImportError::Refused(
            "a declared source URL or records-request identifier is required".to_owned(),
        ));
    }
    if declaration.document_role.trim().is_empty() {
        return Err(ImportError::Refused("document role is required".to_owned()));
    }
    let bytes = fs::read(file_path)?;
    if bytes.len() > max_bytes {
        return Err(ImportError::Oversized {
            limit: max_bytes,
            observed: bytes.len(),
        });
    }
    let observed_digest = sha256_hex(&bytes);
    if observed_digest != declaration.declared_digest {
        return Err(ImportError::DigestMismatch {
            declared: declaration.declared_digest.clone(),
            observed: observed_digest.clone(),
        });
    }

    // Persist into the content-addressed store path (immutable by digest).
    let content_path = data_dir
        .join("evidence/sha256")
        .join(&observed_digest[..2])
        .join(&observed_digest);
    if let Some(parent) = content_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if !content_path.exists() {
        fs::write(&content_path, &bytes)?;
    }

    let source_url = if declaration.source_or_request_id.starts_with("http") {
        Some(declaration.source_or_request_id.clone())
    } else {
        None
    };
    let id = stable_id(
        "supplied-record",
        &[&declaration.source_or_request_id, &observed_digest],
    );
    Ok(SuppliedRecord {
        id,
        declaration: declaration.clone(),
        observed_digest,
        byte_count: bytes.len(),
        source_url,
        human_reviewed: false,
        processing_provenance: format!(
            "imported by {} on {}",
            declaration.operator, declaration.acquisition_date
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn digest(bytes: &[u8]) -> String {
        sha256_hex(bytes)
    }

    #[test]
    fn refuses_without_lawful_possession_declaration() {
        let dir = tempdir().expect("temp");
        let f = dir.path().join("r.pdf");
        fs::write(&f, b"data").unwrap();
        let mut decl = valid_declaration(&f);
        decl.lawful_possession = false;
        assert!(matches!(
            import_supplied_record(dir.path(), &f, &decl, 1000),
            Err(ImportError::Refused(_))
        ));
    }

    #[test]
    fn refuses_without_source_or_request_id() {
        let dir = tempdir().expect("temp");
        let f = dir.path().join("r.pdf");
        fs::write(&f, b"data").unwrap();
        let mut decl = valid_declaration(&f);
        decl.source_or_request_id = String::new();
        assert!(matches!(
            import_supplied_record(dir.path(), &f, &decl, 1000),
            Err(ImportError::Refused(_))
        ));
    }

    #[test]
    fn refuses_oversized_input() {
        let dir = tempdir().expect("temp");
        let f = dir.path().join("r.pdf");
        fs::write(&f, vec![b'x'; 500]).unwrap();
        let mut decl = valid_declaration(&f);
        decl.declared_digest = digest(&[b'x'; 500]);
        assert!(matches!(
            import_supplied_record(dir.path(), &f, &decl, 100),
            Err(ImportError::Oversized { .. })
        ));
    }

    #[test]
    fn rejects_digest_mismatch() {
        let dir = tempdir().expect("temp");
        let f = dir.path().join("r.pdf");
        fs::write(&f, b"data").unwrap();
        let mut decl = valid_declaration(&f);
        decl.declared_digest = "0".repeat(64);
        assert!(matches!(
            import_supplied_record(dir.path(), &f, &decl, 1000),
            Err(ImportError::DigestMismatch { .. })
        ));
    }

    #[test]
    fn imports_valid_record_as_unreviewed() {
        let dir = tempdir().expect("temp");
        let f = dir.path().join("r.pdf");
        fs::write(&f, b"official record bytes").unwrap();
        let decl = valid_declaration(&f);
        let record = import_supplied_record(dir.path(), &f, &decl, 1000).expect("import");
        assert_eq!(record.observed_digest, digest(b"official record bytes"));
        assert!(!record.human_reviewed);
        assert!(record.source_url.is_some());
        // Bytes persisted into the content-addressed path.
        let blob = dir
            .path()
            .join("evidence/sha256")
            .join(&record.observed_digest[..2])
            .join(&record.observed_digest);
        assert!(blob.exists());
    }

    fn valid_declaration(path: &Path) -> SuppliedRecordDeclaration {
        let bytes = fs::read(path).unwrap();
        SuppliedRecordDeclaration {
            source_or_request_id: "https://coloradosprings.gov/document/example.pdf".to_owned(),
            acquisition_date: "2026-08-17".to_owned(),
            document_role: "contract".to_owned(),
            lawful_possession: true,
            declared_digest: sha256_hex(&bytes),
            operator: "operator".to_owned(),
        }
    }
}
