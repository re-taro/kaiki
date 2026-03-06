use kaiki_config::GitHubNotifyConfig;

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

/// GitHub notifier using direct REST API calls.
pub struct GitHubNotifier {
    client: reqwest::Client,
    token: String,
    config: GitHubNotifyConfig,
}

impl GitHubNotifier {
    pub fn new(config: GitHubNotifyConfig) -> Result<Self, NotifyError> {
        let token = std::env::var("GITHUB_TOKEN")
            .map_err(|_| NotifyError::Config("GITHUB_TOKEN environment variable not set".into()))?;

        let client = reqwest::Client::builder()
            .user_agent("kaiki")
            .build()
            .map_err(|e| NotifyError::Http(e.to_string()))?;

        Ok(Self { client, token, config })
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

    async fn set_commit_status(
        &self,
        sha: &str,
        state: &str,
        description: &str,
        target_url: Option<&str>,
    ) -> Result<(), NotifyError> {
        let owner = self.owner()?;
        let repo = self.repo()?;
        let url = format!("https://api.github.com/repos/{owner}/{repo}/statuses/{sha}");

        let body = build_commit_status_payload(state, description, target_url);

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github.v3+json")
            .json(&body)
            .send()
            .await
            .map_err(|e| NotifyError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(NotifyError::Failed(format!("commit status failed ({status}): {text}")));
        }

        Ok(())
    }

    async fn post_or_update_comment(&self, pr_number: u64, body: &str) -> Result<(), NotifyError> {
        let owner = self.owner()?;
        let repo = self.repo()?;

        match self.config.pr_comment_behavior.as_str() {
            "once" => {
                // Only post if no existing comment
                if self.find_existing_comment(pr_number).await?.is_none() {
                    self.create_comment(owner, repo, pr_number, body).await?;
                }
            }
            "new" => {
                // Always create new comment
                self.create_comment(owner, repo, pr_number, body).await?;
            }
            _ => {
                // "default" - update existing or create new
                if let Some(comment_id) = self.find_existing_comment(pr_number).await? {
                    self.update_comment(owner, repo, comment_id, body).await?;
                } else {
                    self.create_comment(owner, repo, pr_number, body).await?;
                }
            }
        }

        Ok(())
    }

    async fn find_existing_comment(&self, pr_number: u64) -> Result<Option<u64>, NotifyError> {
        let owner = self.owner()?;
        let repo = self.repo()?;
        let url =
            format!("https://api.github.com/repos/{owner}/{repo}/issues/{pr_number}/comments");

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .map_err(|e| NotifyError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(None);
        }

        let comments: Vec<serde_json::Value> =
            resp.json().await.map_err(|e| NotifyError::Http(e.to_string()))?;

        for comment in comments {
            if let Some(body) = comment["body"].as_str()
                && body.contains(COMMENT_MARKER)
                && let Some(id) = comment["id"].as_u64()
            {
                return Ok(Some(id));
            }
        }

        Ok(None)
    }

    async fn create_comment(
        &self,
        owner: &str,
        repo: &str,
        pr_number: u64,
        body: &str,
    ) -> Result<(), NotifyError> {
        let url =
            format!("https://api.github.com/repos/{owner}/{repo}/issues/{pr_number}/comments");

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github.v3+json")
            .json(&serde_json::json!({ "body": body }))
            .send()
            .await
            .map_err(|e| NotifyError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(NotifyError::Failed(format!("create comment failed ({status}): {text}")));
        }

        Ok(())
    }

    async fn update_comment(
        &self,
        owner: &str,
        repo: &str,
        comment_id: u64,
        body: &str,
    ) -> Result<(), NotifyError> {
        let url =
            format!("https://api.github.com/repos/{owner}/{repo}/issues/comments/{comment_id}");

        let resp = self
            .client
            .patch(&url)
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github.v3+json")
            .json(&serde_json::json!({ "body": body }))
            .send()
            .await
            .map_err(|e| NotifyError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(NotifyError::Failed(format!("update comment failed ({status}): {text}")));
        }

        Ok(())
    }

    fn build_comment_body(&self, params: &NotifyParams) -> String {
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
}

impl Notifier for GitHubNotifier {
    async fn notify(&self, params: &NotifyParams) -> Result<(), NotifyError> {
        let has_changes = params.comparison.has_changes();

        // Set commit status
        if self.config.set_commit_status {
            let state = if params.comparison.has_failures() { "failure" } else { "success" };
            let description = format!(
                "changed: {}, new: {}, deleted: {}",
                params.comparison.failed_items.len(),
                params.comparison.new_items.len(),
                params.comparison.deleted_items.len(),
            );
            self.set_commit_status(
                &params.current_sha,
                state,
                &description,
                params.report_url.as_deref(),
            )
            .await?;
        }

        // Post PR comment
        if self.config.pr_comment
            && has_changes
            && let Some(pr_number) = params.pr_number
        {
            let body = self.build_comment_body(params);
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
        let config = GitHubNotifyConfig::default();
        let notifier =
            GitHubNotifier { client: reqwest::Client::new(), token: "fake".to_string(), config };
        let params = sample_params(None);
        let body = notifier.build_comment_body(&params);
        assert!(body.contains(COMMENT_MARKER));
        assert!(body.contains("## reg report"));
        assert!(body.contains("Changed | 1"));
        assert!(body.contains("New | 1"));
        assert!(body.contains("Deleted | 0"));
        assert!(body.contains("Passed | 1"));
    }

    #[test]
    fn test_build_comment_body_with_report_url() {
        let config = GitHubNotifyConfig::default();
        let notifier =
            GitHubNotifier { client: reqwest::Client::new(), token: "fake".to_string(), config };
        let params = sample_params(Some("https://example.com/report".to_string()));
        let body = notifier.build_comment_body(&params);
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
