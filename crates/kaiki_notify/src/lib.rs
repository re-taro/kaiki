pub mod github;
pub mod github_client;
pub mod slack;

use kaiki_report::ComparisonResult;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NotifyError {
    #[error("HTTP request failed: {0}")]
    Http(String),
    #[error("notification failed: {0}")]
    Failed(String),
    #[error("configuration error: {0}")]
    Config(String),
}

/// Parameters for notification.
#[derive(Debug, Clone)]
pub struct NotifyParams {
    pub comparison: ComparisonResult,
    pub report_url: Option<String>,
    pub current_sha: String,
    pub pr_number: Option<u64>,
}

/// Trait for notification backends.
pub trait Notifier: Send + Sync {
    fn notify(
        &self,
        params: &NotifyParams,
    ) -> impl std::future::Future<Output = Result<(), NotifyError>> + Send;
}
