use kaiki_notify::github_client::{GitHubClient, HttpGitHubClient};
use wiremock::matchers::{bearer_token, body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn setup() -> (MockServer, HttpGitHubClient) {
    let server = MockServer::start().await;
    let client =
        HttpGitHubClient::with_base_url("test-token".to_string(), server.uri()).unwrap();
    (server, client)
}

// ---------------------------------------------------------------------------
// create_commit_status
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_commit_status_request() {
    let (server, client) = setup().await;

    Mock::given(method("POST"))
        .and(path("/repos/owner/repo/statuses/abc123"))
        .and(bearer_token("test-token"))
        .and(header("Accept", "application/vnd.github.v3+json"))
        .and(body_json(serde_json::json!({
            "state": "success",
            "description": "all good",
            "context": "reg"
        })))
        .respond_with(ResponseTemplate::new(201))
        .expect(1)
        .mount(&server)
        .await;

    let payload = serde_json::json!({
        "state": "success",
        "description": "all good",
        "context": "reg"
    });
    client
        .create_commit_status("owner", "repo", "abc123", &payload)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_create_commit_status_error_response() {
    let (server, client) = setup().await;

    Mock::given(method("POST"))
        .and(path("/repos/owner/repo/statuses/abc123"))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
        .mount(&server)
        .await;

    let payload = serde_json::json!({"state": "success", "description": "x", "context": "reg"});
    let result = client.create_commit_status("owner", "repo", "abc123", &payload).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("commit status failed"));
    assert!(err.contains("500"));
}

// ---------------------------------------------------------------------------
// list_issue_comments
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_issue_comments_parses_response() {
    let (server, client) = setup().await;

    let response_body = serde_json::json!([
        {"id": 1, "body": "first comment", "user": {"login": "bot"}},
        {"id": 2, "body": "second comment", "user": {"login": "human"}},
    ]);

    Mock::given(method("GET"))
        .and(path("/repos/owner/repo/issues/42/comments"))
        .and(bearer_token("test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .mount(&server)
        .await;

    let comments = client.list_issue_comments("owner", "repo", 42).await.unwrap();
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0].id, 1);
    assert_eq!(comments[0].body, "first comment");
    assert_eq!(comments[1].id, 2);
    assert_eq!(comments[1].body, "second comment");
}

// ---------------------------------------------------------------------------
// create_issue_comment
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_issue_comment_request() {
    let (server, client) = setup().await;

    Mock::given(method("POST"))
        .and(path("/repos/owner/repo/issues/42/comments"))
        .and(bearer_token("test-token"))
        .and(body_json(serde_json::json!({"body": "hello world"})))
        .respond_with(ResponseTemplate::new(201))
        .expect(1)
        .mount(&server)
        .await;

    client
        .create_issue_comment("owner", "repo", 42, "hello world")
        .await
        .unwrap();
}

// ---------------------------------------------------------------------------
// update_issue_comment
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_update_issue_comment_request() {
    let (server, client) = setup().await;

    Mock::given(method("PATCH"))
        .and(path("/repos/owner/repo/issues/comments/999"))
        .and(bearer_token("test-token"))
        .and(body_json(serde_json::json!({"body": "updated body"})))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    client
        .update_issue_comment("owner", "repo", 999, "updated body")
        .await
        .unwrap();
}
