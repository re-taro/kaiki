use kaiki_config::SlackNotifyConfig;
use kaiki_notify::slack::SlackNotifier;
use kaiki_notify::{Notifier, NotifyParams};
use kaiki_report::ComparisonResult;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_params(has_failures: bool, report_url: Option<&str>) -> NotifyParams {
    NotifyParams {
        comparison: ComparisonResult {
            failed_items: if has_failures { vec!["a.png".into()] } else { vec![] },
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
        report_url: report_url.map(|s| s.to_string()),
        current_sha: "abc123".to_string(),
        pr_number: Some(42),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_slack_webhook_post() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let config = SlackNotifyConfig { webhook_url: server.uri() };
    let notifier = SlackNotifier::new(config).unwrap();
    let params = make_params(false, None);

    notifier.notify(&params).await.unwrap();
}

#[tokio::test]
async fn test_slack_webhook_failure_is_ok() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(500).set_body_string("server error"))
        .mount(&server)
        .await;

    let config = SlackNotifyConfig { webhook_url: server.uri() };
    let notifier = SlackNotifier::new(config).unwrap();
    let params = make_params(false, None);

    // Should return Ok even when webhook returns 500 (warn-only behavior)
    notifier.notify(&params).await.unwrap();
}

#[tokio::test]
async fn test_slack_payload_contains_report_url() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let config = SlackNotifyConfig { webhook_url: server.uri() };
    let notifier = SlackNotifier::new(config).unwrap();
    let params = make_params(false, Some("https://example.com/report"));

    notifier.notify(&params).await.unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    let fields = body["attachments"][0]["fields"].as_array().unwrap();
    let report_field = fields.iter().find(|f| f["title"] == "Report");
    assert!(report_field.is_some());
    assert_eq!(report_field.unwrap()["value"], "https://example.com/report");
}

#[tokio::test]
async fn test_slack_payload_failure_color() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let config = SlackNotifyConfig { webhook_url: server.uri() };
    let notifier = SlackNotifier::new(config).unwrap();
    let params = make_params(true, None);

    notifier.notify(&params).await.unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["attachments"][0]["color"], "danger");
}
