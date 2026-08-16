//! Aggregate ingestion-job budgets.
//!
//! Per-file limits are insufficient on their own: a hostile matter containing
//! many individually valid attachments must not multiply the per-file limit
//! into unlimited aggregate work. These budgets span an entire ingestion job.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum BudgetError {
    #[error("aggregate downloaded-bytes budget exceeded (limit {limit}, observed {observed})")]
    DownloadedBytes { limit: u64, observed: u64 },
    #[error("aggregate attachment budget exceeded (limit {limit})")]
    Attachments { limit: u32 },
    #[error("aggregate PDF-page budget exceeded (limit {limit})")]
    PdfPages { limit: u32 },
    #[error("aggregate OCR-page budget exceeded (limit {limit})")]
    OcrPages { limit: u32 },
    #[error("aggregate extracted-bytes budget exceeded (limit {limit}, observed {observed})")]
    ExtractedBytes { limit: u64, observed: u64 },
    #[error("aggregate child-process budget exceeded (limit {limit})")]
    ChildProcesses { limit: u32 },
    #[error("aggregate CPU-seconds budget exceeded (limit {limit})")]
    CpuSeconds { limit: u64 },
    #[error("aggregate wall-seconds budget exceeded (limit {limit})")]
    WallSeconds { limit: u64 },
}

/// Aggregate limits spanning one ingestion job.
#[derive(Clone, Copy, Debug)]
pub struct JobBudgets {
    pub max_total_downloaded_bytes: u64,
    pub max_total_attachments: u32,
    pub max_total_pdf_pages: u32,
    pub max_total_ocr_pages: u32,
    pub max_total_extracted_bytes: u64,
    pub max_total_child_processes: u32,
    pub max_total_cpu_seconds: u64,
    pub max_total_wall_seconds: u64,
}

impl JobBudgets {
    pub const fn defaults() -> Self {
        Self {
            max_total_downloaded_bytes: 200 * 1024 * 1024,
            max_total_attachments: 200,
            max_total_pdf_pages: 2000,
            max_total_ocr_pages: 50,
            max_total_extracted_bytes: 50 * 1024 * 1024,
            max_total_child_processes: 2000,
            max_total_cpu_seconds: 600,
            max_total_wall_seconds: 600,
        }
    }
}

/// Tracks cumulative resource use across an ingestion job and enforces the
/// aggregate budgets.
#[derive(Clone, Debug)]
pub struct JobBudgetTracker {
    budgets: JobBudgets,
    downloaded_bytes: u64,
    attachments: u32,
    pdf_pages: u32,
    ocr_pages: u32,
    extracted_bytes: u64,
    child_processes: u32,
    cpu_seconds: u64,
    wall_seconds: u64,
}

impl JobBudgetTracker {
    pub fn new(budgets: JobBudgets) -> Self {
        Self {
            budgets,
            downloaded_bytes: 0,
            attachments: 0,
            pdf_pages: 0,
            ocr_pages: 0,
            extracted_bytes: 0,
            child_processes: 0,
            cpu_seconds: 0,
            wall_seconds: 0,
        }
    }

    pub fn add_downloaded_bytes(&mut self, bytes: u64) -> Result<(), BudgetError> {
        self.downloaded_bytes = self.downloaded_bytes.saturating_add(bytes);
        if self.downloaded_bytes > self.budgets.max_total_downloaded_bytes {
            return Err(BudgetError::DownloadedBytes {
                limit: self.budgets.max_total_downloaded_bytes,
                observed: self.downloaded_bytes,
            });
        }
        Ok(())
    }

    pub fn add_attachment(&mut self) -> Result<(), BudgetError> {
        self.attachments = self.attachments.saturating_add(1);
        if self.attachments > self.budgets.max_total_attachments {
            return Err(BudgetError::Attachments {
                limit: self.budgets.max_total_attachments,
            });
        }
        Ok(())
    }

    pub fn add_pdf_pages(&mut self, pages: u32) -> Result<(), BudgetError> {
        self.pdf_pages = self.pdf_pages.saturating_add(pages);
        if self.pdf_pages > self.budgets.max_total_pdf_pages {
            return Err(BudgetError::PdfPages {
                limit: self.budgets.max_total_pdf_pages,
            });
        }
        Ok(())
    }

    pub fn add_ocr_pages(&mut self, pages: u32) -> Result<(), BudgetError> {
        self.ocr_pages = self.ocr_pages.saturating_add(pages);
        if self.ocr_pages > self.budgets.max_total_ocr_pages {
            return Err(BudgetError::OcrPages {
                limit: self.budgets.max_total_ocr_pages,
            });
        }
        Ok(())
    }

    pub fn add_extracted_bytes(&mut self, bytes: u64) -> Result<(), BudgetError> {
        self.extracted_bytes = self.extracted_bytes.saturating_add(bytes);
        if self.extracted_bytes > self.budgets.max_total_extracted_bytes {
            return Err(BudgetError::ExtractedBytes {
                limit: self.budgets.max_total_extracted_bytes,
                observed: self.extracted_bytes,
            });
        }
        Ok(())
    }

    pub fn add_child_process(&mut self) -> Result<(), BudgetError> {
        self.child_processes = self.child_processes.saturating_add(1);
        if self.child_processes > self.budgets.max_total_child_processes {
            return Err(BudgetError::ChildProcesses {
                limit: self.budgets.max_total_child_processes,
            });
        }
        Ok(())
    }

    pub fn add_cpu_seconds(&mut self, seconds: u64) -> Result<(), BudgetError> {
        self.cpu_seconds = self.cpu_seconds.saturating_add(seconds);
        if self.cpu_seconds > self.budgets.max_total_cpu_seconds {
            return Err(BudgetError::CpuSeconds {
                limit: self.budgets.max_total_cpu_seconds,
            });
        }
        Ok(())
    }

    pub fn add_wall_seconds(&mut self, seconds: u64) -> Result<(), BudgetError> {
        self.wall_seconds = self.wall_seconds.saturating_add(seconds);
        if self.wall_seconds > self.budgets.max_total_wall_seconds {
            return Err(BudgetError::WallSeconds {
                limit: self.budgets.max_total_wall_seconds,
            });
        }
        Ok(())
    }

    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "downloaded_bytes": self.downloaded_bytes,
            "attachments": self.attachments,
            "pdf_pages": self.pdf_pages,
            "ocr_pages": self.ocr_pages,
            "extracted_bytes": self.extracted_bytes,
            "child_processes": self.child_processes,
            "cpu_seconds": self.cpu_seconds,
            "wall_seconds": self.wall_seconds,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_attachment_limit_applies_across_many_attachments() {
        let budgets = JobBudgets {
            max_total_attachments: 3,
            ..JobBudgets::defaults()
        };
        let mut tracker = JobBudgetTracker::new(budgets);
        tracker.add_attachment().expect("1");
        tracker.add_attachment().expect("2");
        tracker.add_attachment().expect("3");
        assert!(matches!(
            tracker.add_attachment(),
            Err(BudgetError::Attachments { limit: 3 })
        ));
    }

    #[test]
    fn aggregate_pdf_page_limit_cannot_be_multiplied() {
        let budgets = JobBudgets {
            max_total_pdf_pages: 5,
            ..JobBudgets::defaults()
        };
        let mut tracker = JobBudgetTracker::new(budgets);
        tracker.add_pdf_pages(3).expect("first");
        assert!(matches!(
            tracker.add_pdf_pages(3),
            Err(BudgetError::PdfPages { limit: 5 })
        ));
    }

    #[test]
    fn aggregate_downloaded_bytes_limit_applies() {
        let budgets = JobBudgets {
            max_total_downloaded_bytes: 100,
            ..JobBudgets::defaults()
        };
        let mut tracker = JobBudgetTracker::new(budgets);
        tracker.add_downloaded_bytes(60).expect("first");
        assert!(matches!(
            tracker.add_downloaded_bytes(60),
            Err(BudgetError::DownloadedBytes { .. })
        ));
    }
}
