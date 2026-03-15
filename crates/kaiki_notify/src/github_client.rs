use std::future::Future;

use crate::NotifyError;

/// Minimal representation of a GitHub issue comment.
#[derive(Debug)]
pub struct IssueComment {
    pub id: u64,
    pub body: String,
}

/// Trait abstracting GitHub REST API operations.
/// Production: `HttpGitHubClient` (reqwest). Tests: mock implementation.
pub trait GitHubClient: Send + Sync {
    fn create_commit_status(
        &self,
        owner: &str,
        repo: &str,
        sha: &str,
        payload: &serde_json::Value,
    ) -> impl Future<Output = Result<(), NotifyError>> + Send;

    fn list_issue_comments(
        &self,
        owner: &str,
        repo: &str,
        issue_number: u64,
    ) -> impl Future<Output = Result<Vec<IssueComment>, NotifyError>> + Send;

    fn create_issue_comment(
        &self,
        owner: &str,
        repo: &str,
        issue_number: u64,
        body: &str,
    ) -> impl Future<Output = Result<(), NotifyError>> + Send;

    fn update_issue_comment(
        &self,
        owner: &str,
        repo: &str,
        comment_id: u64,
        body: &str,
    ) -> impl Future<Output = Result<(), NotifyError>> + Send;
}

/// reqwest-based GitHub REST API client.
pub struct HttpGitHubClient {
    client: reqwest::Client,
    token: String,
    base_url: String,
}

impl HttpGitHubClient {
    /// Create a new client targeting `https://api.github.com`.
    pub fn new(token: String) -> Result<Self, NotifyError> {
        let client = reqwest::Client::builder()
            .user_agent("kaiki")
            .build()
            .map_err(|e| NotifyError::Http(e.to_string()))?;
        Ok(Self {
            client,
            token,
            base_url: "https://api.github.com".to_string(),
        })
    }

    /// Create a new client with a custom base URL (for testing / GitHub Enterprise).
    pub fn with_base_url(token: String, base_url: String) -> Result<Self, NotifyError> {
        let client = reqwest::Client::builder()
            .user_agent("kaiki")
            .build()
            .map_err(|e| NotifyError::Http(e.to_string()))?;
        Ok(Self { client, token, base_url })
    }
}

impl GitHubClient for HttpGitHubClient {
    async fn create_commit_status(
        &self,
        owner: &str,
        repo: &str,
        sha: &str,
        payload: &serde_json::Value,
    ) -> Result<(), NotifyError> {
        let url = format!("{}/repos/{owner}/{repo}/statuses/{sha}", self.base_url);

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github.v3+json")
            .json(payload)
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

    async fn list_issue_comments(
        &self,
        owner: &str,
        repo: &str,
        issue_number: u64,
    ) -> Result<Vec<IssueComment>, NotifyError> {
        let url =
            format!("{}/repos/{owner}/{repo}/issues/{issue_number}/comments", self.base_url);

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .map_err(|e| NotifyError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(NotifyError::Failed(format!(
                "list comments failed ({status}): {text}"
            )));
        }

        let raw: Vec<serde_json::Value> =
            resp.json().await.map_err(|e| NotifyError::Http(e.to_string()))?;

        let comments = raw
            .into_iter()
            .filter_map(|v| {
                let id = v["id"].as_u64()?;
                let body = v["body"].as_str()?.to_string();
                Some(IssueComment { id, body })
            })
            .collect();

        Ok(comments)
    }

    async fn create_issue_comment(
        &self,
        owner: &str,
        repo: &str,
        issue_number: u64,
        body: &str,
    ) -> Result<(), NotifyError> {
        let url =
            format!("{}/repos/{owner}/{repo}/issues/{issue_number}/comments", self.base_url);

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
            return Err(NotifyError::Failed(format!(
                "create comment failed ({status}): {text}"
            )));
        }

        Ok(())
    }

    async fn update_issue_comment(
        &self,
        owner: &str,
        repo: &str,
        comment_id: u64,
        body: &str,
    ) -> Result<(), NotifyError> {
        let url = format!("{}/repos/{owner}/{repo}/issues/comments/{comment_id}", self.base_url);

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
            return Err(NotifyError::Failed(format!(
                "update comment failed ({status}): {text}"
            )));
        }

        Ok(())
    }
}
