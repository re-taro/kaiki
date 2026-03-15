pub mod ci;
pub mod image_finder;
pub mod processor;

use thiserror::Error;

/// Errors that can occur in the core pipeline.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("diff error: {0}")]
    Diff(#[from] kaiki_diff::DiffError),
    #[error("config error: {0}")]
    Config(#[from] kaiki_config::ConfigError),
    #[error("report error: {0}")]
    Report(#[from] kaiki_report::ReportError),
    #[error("git error: {0}")]
    Git(#[from] kaiki_git::GitError),
    #[error("storage error: {0}")]
    Storage(#[from] kaiki_storage::StorageError),
    #[error("notify error: {0}")]
    Notify(#[from] kaiki_notify::NotifyError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// The result of a full pipeline run.
#[derive(Debug)]
pub struct PipelineResult {
    pub comparison: kaiki_report::ComparisonResult,
    pub report_url: Option<String>,
    pub has_failures: bool,
}
