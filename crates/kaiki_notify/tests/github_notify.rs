use std::sync::{Arc, Mutex};

use kaiki_config::GitHubNotifyConfig;
use kaiki_notify::github::GitHubNotifier;
use kaiki_notify::github_client::{GitHubClient, IssueComment};
use kaiki_notify::{Notifier, NotifyError, NotifyParams};
use kaiki_report::ComparisonResult;

// ---------------------------------------------------------------------------
// Mock GitHubClient (shared via Arc so tests can inspect calls after notify)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct MockState {
    /// Captures (owner, repo, sha, payload) for each `create_commit_status` call.
    status_calls: Vec<(String, String, String, serde_json::Value)>,
    /// Captures (owner, repo, issue_number, body) for each `create_issue_comment` call.
    create_calls: Vec<(String, String, u64, String)>,
    /// Captures (owner, repo, comment_id, body) for each `update_issue_comment` call.
    update_calls: Vec<(String, String, u64, String)>,
}

struct MockGitHubClient {
    state: Arc<Mutex<MockState>>,
    /// Comments returned by `list_issue_comments`.
    existing_comments: Vec<IssueComment>,
    /// When true, `create_commit_status` returns an error.
    fail_status: bool,
}

impl MockGitHubClient {
    fn new(state: Arc<Mutex<MockState>>) -> Self {
        Self {
            state,
            existing_comments: Vec::new(),
            fail_status: false,
        }
    }

    fn with_existing_comments(mut self, comments: Vec<IssueComment>) -> Self {
        self.existing_comments = comments;
        self
    }

    fn with_fail_status(mut self) -> Self {
        self.fail_status = true;
        self
    }
}

impl GitHubClient for MockGitHubClient {
    async fn create_commit_status(
        &self,
        owner: &str,
        repo: &str,
        sha: &str,
        payload: &serde_json::Value,
    ) -> Result<(), NotifyError> {
        if self.fail_status {
            return Err(NotifyError::Failed("mock error".into()));
        }
        self.state.lock().unwrap().status_calls.push((
            owner.to_string(),
            repo.to_string(),
            sha.to_string(),
            payload.clone(),
        ));
        Ok(())
    }

    async fn list_issue_comments(
        &self,
        _owner: &str,
        _repo: &str,
        _issue_number: u64,
    ) -> Result<Vec<IssueComment>, NotifyError> {
        Ok(self
            .existing_comments
            .iter()
            .map(|c| IssueComment { id: c.id, body: c.body.clone() })
            .collect())
    }

    async fn create_issue_comment(
        &self,
        owner: &str,
        repo: &str,
        issue_number: u64,
        body: &str,
    ) -> Result<(), NotifyError> {
        self.state.lock().unwrap().create_calls.push((
            owner.to_string(),
            repo.to_string(),
            issue_number,
            body.to_string(),
        ));
        Ok(())
    }

    async fn update_issue_comment(
        &self,
        owner: &str,
        repo: &str,
        comment_id: u64,
        body: &str,
    ) -> Result<(), NotifyError> {
        self.state.lock().unwrap().update_calls.push((
            owner.to_string(),
            repo.to_string(),
            comment_id,
            body.to_string(),
        ));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_config(pr_comment_behavior: &str, set_commit_status: bool) -> GitHubNotifyConfig {
    GitHubNotifyConfig {
        client_id: None,
        owner: Some("test-owner".to_string()),
        repository: Some("test-repo".to_string()),
        pr_comment: true,
        pr_comment_behavior: pr_comment_behavior.to_string(),
        set_commit_status,
    }
}

fn make_params(has_failures: bool, has_changes: bool, report_url: Option<&str>) -> NotifyParams {
    NotifyParams {
        comparison: ComparisonResult {
            failed_items: if has_failures { vec!["a.png".into()] } else { vec![] },
            new_items: if has_changes && !has_failures { vec!["b.png".into()] } else { vec![] },
            deleted_items: vec![],
            passed_items: vec!["c.png".into()],
            expected_items: vec![],
            actual_items: vec![],
            diff_items: vec![],
            actual_dir: "actual".into(),
            expected_dir: "expected".into(),
            diff_dir: "diff".into(),
        },
        report_url: report_url.map(|s| s.to_string()),
        current_sha: "abc123".to_string(),
        pr_number: Some(42),
    }
}

// ---------------------------------------------------------------------------
// Group A: Commit status tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_commit_status_success() {
    let state = Arc::new(Mutex::new(MockState::default()));
    let client = MockGitHubClient::new(Arc::clone(&state));
    let notifier = GitHubNotifier::with_client(make_config("default", true), client);
    let params = make_params(false, false, None);

    notifier.notify(&params).await.unwrap();

    let s = state.lock().unwrap();
    assert_eq!(s.status_calls.len(), 1);
    let (owner, repo, sha, payload) = &s.status_calls[0];
    assert_eq!(owner, "test-owner");
    assert_eq!(repo, "test-repo");
    assert_eq!(sha, "abc123");
    assert_eq!(payload["state"], "success");
}

#[tokio::test]
async fn test_commit_status_failure() {
    let state = Arc::new(Mutex::new(MockState::default()));
    let client = MockGitHubClient::new(Arc::clone(&state));
    let notifier = GitHubNotifier::with_client(make_config("default", true), client);
    let params = make_params(true, true, None);

    notifier.notify(&params).await.unwrap();

    let s = state.lock().unwrap();
    assert_eq!(s.status_calls.len(), 1);
    assert_eq!(s.status_calls[0].3["state"], "failure");
}

#[tokio::test]
async fn test_commit_status_with_target_url() {
    let state = Arc::new(Mutex::new(MockState::default()));
    let client = MockGitHubClient::new(Arc::clone(&state));
    let notifier = GitHubNotifier::with_client(make_config("default", true), client);
    let params = make_params(false, false, Some("https://example.com/report"));

    notifier.notify(&params).await.unwrap();

    let s = state.lock().unwrap();
    assert_eq!(s.status_calls.len(), 1);
    assert_eq!(s.status_calls[0].3["target_url"], "https://example.com/report");
}

// ---------------------------------------------------------------------------
// Group B: PR comment behavior tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_pr_comment_default_creates_new() {
    let state = Arc::new(Mutex::new(MockState::default()));
    let client = MockGitHubClient::new(Arc::clone(&state));
    let notifier = GitHubNotifier::with_client(make_config("default", false), client);
    let params = make_params(true, true, None);

    notifier.notify(&params).await.unwrap();

    let s = state.lock().unwrap();
    assert_eq!(s.create_calls.len(), 1);
    assert_eq!(s.create_calls[0].2, 42); // pr_number
    assert!(s.create_calls[0].3.contains("reg report"));
    assert!(s.update_calls.is_empty());
}

#[tokio::test]
async fn test_pr_comment_default_updates_existing() {
    let state = Arc::new(Mutex::new(MockState::default()));
    let existing = vec![IssueComment {
        id: 999,
        body: "<!-- reg-suit kaiki -->\nold report".to_string(),
    }];
    let client = MockGitHubClient::new(Arc::clone(&state)).with_existing_comments(existing);
    let notifier = GitHubNotifier::with_client(make_config("default", false), client);
    let params = make_params(true, true, None);

    notifier.notify(&params).await.unwrap();

    let s = state.lock().unwrap();
    assert!(s.create_calls.is_empty());
    assert_eq!(s.update_calls.len(), 1);
    assert_eq!(s.update_calls[0].2, 999); // comment_id
}

#[tokio::test]
async fn test_pr_comment_once_skips_if_exists() {
    let state = Arc::new(Mutex::new(MockState::default()));
    let existing = vec![IssueComment {
        id: 123,
        body: "<!-- reg-suit kaiki -->\nprevious".to_string(),
    }];
    let client = MockGitHubClient::new(Arc::clone(&state)).with_existing_comments(existing);
    let notifier = GitHubNotifier::with_client(make_config("once", false), client);
    let params = make_params(true, true, None);

    notifier.notify(&params).await.unwrap();

    let s = state.lock().unwrap();
    assert!(s.create_calls.is_empty());
    assert!(s.update_calls.is_empty());
}

#[tokio::test]
async fn test_pr_comment_new_always_creates() {
    let state = Arc::new(Mutex::new(MockState::default()));
    let existing = vec![IssueComment {
        id: 123,
        body: "<!-- reg-suit kaiki -->\nprevious".to_string(),
    }];
    let client = MockGitHubClient::new(Arc::clone(&state)).with_existing_comments(existing);
    let notifier = GitHubNotifier::with_client(make_config("new", false), client);
    let params = make_params(true, true, None);

    notifier.notify(&params).await.unwrap();

    let s = state.lock().unwrap();
    assert_eq!(s.create_calls.len(), 1);
    assert!(s.update_calls.is_empty());
}

#[tokio::test]
async fn test_pr_comment_skipped_without_changes() {
    let state = Arc::new(Mutex::new(MockState::default()));
    let client = MockGitHubClient::new(Arc::clone(&state));
    let notifier = GitHubNotifier::with_client(make_config("default", false), client);
    // no failures, no changes
    let params = make_params(false, false, None);

    notifier.notify(&params).await.unwrap();

    let s = state.lock().unwrap();
    assert!(s.create_calls.is_empty());
    assert!(s.update_calls.is_empty());
}

// ---------------------------------------------------------------------------
// Group C: Error and skip tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_commit_status_api_error() {
    let state = Arc::new(Mutex::new(MockState::default()));
    let client = MockGitHubClient::new(Arc::clone(&state)).with_fail_status();
    let notifier = GitHubNotifier::with_client(make_config("default", true), client);
    let params = make_params(false, false, None);

    let result = notifier.notify(&params).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("mock error"));

    // No status call was recorded (error before recording)
    let s = state.lock().unwrap();
    assert!(s.status_calls.is_empty());
}

#[tokio::test]
async fn test_commit_status_disabled() {
    let state = Arc::new(Mutex::new(MockState::default()));
    let client = MockGitHubClient::new(Arc::clone(&state));
    let notifier = GitHubNotifier::with_client(make_config("default", false), client);
    let params = make_params(false, false, None);

    notifier.notify(&params).await.unwrap();

    let s = state.lock().unwrap();
    assert!(s.status_calls.is_empty());
}
