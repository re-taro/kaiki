use kaiki_config::GitHubNotifyConfig;

use crate::github_client::{GitHubClient, HttpGitHubClient, IssueComment};
use crate::{Notifier, NotifyError, NotifyParams};

const COMMENT_MARKER: &str = "<!-- reg-suit kaiki -->";

/// Build the JSON payload for a GitHub commit status API call.
fn build_commit_status_payload(
    state: &str,
    description: &str,
    target_url: Option<&str>,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "state": state,
        "description": description,
        "context": "reg"
    });

    if let Some(target) = target_url {
        body["target_url"] = serde_json::Value::String(target.to_string());
    }

    body
}

/// Build the PR comment body from notification parameters.
fn build_comment_body(params: &NotifyParams) -> String {
    let comp = &params.comparison;
    let mut body = format!("{COMMENT_MARKER}\n## reg report\n\n");

    body.push_str(&format!(
        "| | Count |\n|---|---|\n| Changed | {} |\n| New | {} |\n| Deleted | {} |\n| Passed | {} |\n\n",
        comp.failed_items.len(),
        comp.new_items.len(),
        comp.deleted_items.len(),
        comp.passed_items.len(),
    ));

    if let Some(ref url) = params.report_url {
        body.push_str(&format!("[Report]({url})\n"));
    }

    body
}

/// GitHub notifier, generic over the HTTP client layer.
pub struct GitHubNotifier<C: GitHubClient> {
    client: C,
    config: GitHubNotifyConfig,
}

/// Production constructor using `HttpGitHubClient`.
impl GitHubNotifier<HttpGitHubClient> {
    /// Creates a new GitHub notifier using `GITHUB_TOKEN` from the environment.
    pub fn new(config: GitHubNotifyConfig) -> Result<Self, NotifyError> {
        let token = std::env::var("GITHUB_TOKEN")
            .map_err(|_| NotifyError::Config("GITHUB_TOKEN environment variable not set".into()))?;
        let client = HttpGitHubClient::new(token)?;
        Ok(Self { client, config })
    }
}

/// Generic constructor for testing with any `GitHubClient` implementation.
impl<C: GitHubClient> GitHubNotifier<C> {
    pub fn with_client(config: GitHubNotifyConfig, client: C) -> Self {
        Self { client, config }
    }

    fn owner(&self) -> Result<&str, NotifyError> {
        self.config
            .owner
            .as_deref()
            .ok_or_else(|| NotifyError::Config("owner not configured".into()))
    }

    fn repo(&self) -> Result<&str, NotifyError> {
        self.config
            .repository
            .as_deref()
            .ok_or_else(|| NotifyError::Config("repository not configured".into()))
    }

    async fn post_or_update_comment(&self, pr_number: u64, body: &str) -> Result<(), NotifyError> {
        let owner = self.owner()?;
        let repo = self.repo()?;

        match self.config.pr_comment_behavior.as_str() {
            "once" => {
                if self.find_existing_comment(pr_number).await?.is_none() {
                    self.client.create_issue_comment(owner, repo, pr_number, body).await?;
                }
            }
            "new" => {
                self.client.create_issue_comment(owner, repo, pr_number, body).await?;
            }
            _ => {
                if let Some(comment_id) = self.find_existing_comment(pr_number).await? {
                    self.client.update_issue_comment(owner, repo, comment_id, body).await?;
                } else {
                    self.client.create_issue_comment(owner, repo, pr_number, body).await?;
                }
            }
        }

        Ok(())
    }

    async fn find_existing_comment(&self, pr_number: u64) -> Result<Option<u64>, NotifyError> {
        let owner = self.owner()?;
        let repo = self.repo()?;

        let comments = match self.client.list_issue_comments(owner, repo, pr_number).await {
            Ok(c) => c,
            Err(_) => return Ok(None), // best-effort: treat HTTP errors as "no existing comment"
        };

        for IssueComment { id, body } in &comments {
            if body.contains(COMMENT_MARKER) {
                return Ok(Some(*id));
            }
        }

        Ok(None)
    }
}

impl<C: GitHubClient> Notifier for GitHubNotifier<C> {
    async fn notify(&self, params: &NotifyParams) -> Result<(), NotifyError> {
        let has_changes = params.comparison.has_changes();

        if self.config.set_commit_status {
            let owner = self.owner()?;
            let repo = self.repo()?;
            let state = if params.comparison.has_failures() { "failure" } else { "success" };
            let description = format!(
                "changed: {}, new: {}, deleted: {}",
                params.comparison.failed_items.len(),
                params.comparison.new_items.len(),
                params.comparison.deleted_items.len(),
            );
            let payload =
                build_commit_status_payload(state, &description, params.report_url.as_deref());
            self.client.create_commit_status(owner, repo, &params.current_sha, &payload).await?;
        }

        if self.config.pr_comment
            && has_changes
            && let Some(pr_number) = params.pr_number
        {
            let body = build_comment_body(params);
            self.post_or_update_comment(pr_number, &body).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use kaiki_report::ComparisonResult;

    use super::*;

    fn sample_params(report_url: Option<String>) -> NotifyParams {
        NotifyParams {
            comparison: ComparisonResult {
                failed_items: vec!["a.png".into()],
                new_items: vec!["b.png".into()],
                deleted_items: vec![],
                passed_items: vec!["c.png".into()],
                expected_items: vec![],
                actual_items: vec![],
                diff_items: vec![],
                actual_dir: "actual".into(),
                expected_dir: "expected".into(),
                diff_dir: "diff".into(),
            },
            report_url,
            current_sha: "abc123".to_string(),
            pr_number: Some(42),
        }
    }

    #[test]
    fn test_comment_marker() {
        assert!(COMMENT_MARKER.starts_with("<!--"));
        assert!(COMMENT_MARKER.ends_with("-->"));
    }

    #[test]
    fn test_build_comment_body_format() {
        let params = sample_params(None);
        let body = build_comment_body(&params);
        assert!(body.contains(COMMENT_MARKER));
        assert!(body.contains("## reg report"));
        assert!(body.contains("Changed | 1"));
        assert!(body.contains("New | 1"));
        assert!(body.contains("Deleted | 0"));
        assert!(body.contains("Passed | 1"));
    }

    #[test]
    fn test_build_comment_body_with_report_url() {
        let params = sample_params(Some("https://example.com/report".to_string()));
        let body = build_comment_body(&params);
        assert!(body.contains("[Report](https://example.com/report)"));
    }

    #[test]
    fn test_commit_status_payload_structure() {
        let payload = build_commit_status_payload("success", "all good", None);
        assert_eq!(payload["state"], "success");
        assert_eq!(payload["description"], "all good");
        assert_eq!(payload["context"], "reg");
        assert!(payload.get("target_url").is_none());
    }

    #[test]
    fn test_commit_status_payload_with_target_url() {
        let payload = build_commit_status_payload(
            "failure",
            "changed: 1, new: 0, deleted: 0",
            Some("https://example.com/report"),
        );
        assert_eq!(payload["state"], "failure");
        assert_eq!(payload["target_url"], "https://example.com/report");
    }

    #[test]
    fn test_commit_status_payload_without_target_url() {
        let payload = build_commit_status_payload("success", "no changes", None);
        assert!(payload.get("target_url").is_none());
    }

    #[test]
    fn test_commit_status_failure_when_has_failures() {
        let params = sample_params(Some("https://example.com/report".to_string()));
        let state = if params.comparison.has_failures() { "failure" } else { "success" };
        assert_eq!(state, "failure");
    }

    #[test]
    fn test_commit_status_success_when_no_failures() {
        let params = NotifyParams {
            comparison: ComparisonResult {
                failed_items: vec![],
                new_items: vec![],
                deleted_items: vec![],
                passed_items: vec!["c.png".into()],
                expected_items: vec![],
                actual_items: vec![],
                diff_items: vec![],
                actual_dir: "actual".into(),
                expected_dir: "expected".into(),
                diff_dir: "diff".into(),
            },
            report_url: None,
            current_sha: "abc123".to_string(),
            pr_number: None,
        };
        let state = if params.comparison.has_failures() { "failure" } else { "success" };
        assert_eq!(state, "success");
    }

    #[test]
    fn test_commit_status_description_format() {
        let params = sample_params(None);
        let description = format!(
            "changed: {}, new: {}, deleted: {}",
            params.comparison.failed_items.len(),
            params.comparison.new_items.len(),
            params.comparison.deleted_items.len(),
        );
        assert_eq!(description, "changed: 1, new: 1, deleted: 0");
    }
}
