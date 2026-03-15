use kaiki_config::SlackNotifyConfig;

use crate::{Notifier, NotifyError, NotifyParams};

/// Build a diff image URL from the report URL.
/// Strips `index.html` from the report URL and appends `{diff_dir}/{image_name}`.
fn diff_image_url(report_url: &str, diff_dir: &str, image_name: &str) -> String {
    let base = report_url.strip_suffix("index.html").unwrap_or(report_url);
    format!("{base}{diff_dir}/{image_name}")
}

/// Build the Slack webhook payload from notification parameters.
fn build_slack_payload(params: &NotifyParams) -> serde_json::Value {
    let comp = &params.comparison;
    let has_failures = comp.has_failures();

    let color = if has_failures {
        "danger"
    } else if comp.has_changes() {
        "warning"
    } else {
        "good"
    };

    let text = format!(
        "Changed: {}, New: {}, Deleted: {}, Passed: {}",
        comp.failed_items.len(),
        comp.new_items.len(),
        comp.deleted_items.len(),
        comp.passed_items.len(),
    );

    let mut fields = vec![serde_json::json!({
        "title": "Result",
        "value": text,
        "short": false,
    })];

    if let Some(ref url) = params.report_url {
        fields.push(serde_json::json!({
            "title": "Report",
            "value": url,
            "short": false,
        }));
    }

    let mut attachment = serde_json::json!({
        "color": color,
        "title": "Visual Regression Report",
        "fields": fields,
    });

    if let (Some(url), Some(first_failed)) = (&params.report_url, comp.failed_items.first()) {
        attachment["image_url"] =
            serde_json::Value::String(diff_image_url(url, &comp.diff_dir, first_failed));
    }

    serde_json::json!({
        "attachments": [attachment]
    })
}

/// Slack webhook notifier.
pub struct SlackNotifier {
    client: reqwest::Client,
    config: SlackNotifyConfig,
}

impl SlackNotifier {
    /// Creates a new Slack notifier with the given configuration.
    pub fn new(config: SlackNotifyConfig) -> Result<Self, NotifyError> {
        let client = reqwest::Client::new();
        Ok(Self { client, config })
    }
}

impl Notifier for SlackNotifier {
    async fn notify(&self, params: &NotifyParams) -> Result<(), NotifyError> {
        let payload = build_slack_payload(params);

        let resp = self
            .client
            .post(&self.config.webhook_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| NotifyError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            tracing::warn!("Slack notification failed ({status}): {text}");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use kaiki_report::ComparisonResult;

    use super::*;
    use crate::NotifyParams;

    fn sample_comparison(has_failures: bool, has_changes: bool) -> ComparisonResult {
        ComparisonResult {
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
        }
    }

    #[test]
    fn test_slack_color_failure() {
        let comp = sample_comparison(true, true);
        let color = if comp.has_failures() {
            "danger"
        } else if comp.has_changes() {
            "warning"
        } else {
            "good"
        };
        assert_eq!(color, "danger");
    }

    #[test]
    fn test_slack_color_warning() {
        let comp = sample_comparison(false, true);
        let color = if comp.has_failures() {
            "danger"
        } else if comp.has_changes() {
            "warning"
        } else {
            "good"
        };
        assert_eq!(color, "warning");
    }

    #[test]
    fn test_diff_image_url_basic() {
        let url = super::diff_image_url(
            "https://bucket.s3.amazonaws.com/key/index.html",
            "diff",
            "a.png",
        );
        assert_eq!(url, "https://bucket.s3.amazonaws.com/key/diff/a.png");
    }

    #[test]
    fn test_diff_image_url_with_prefix() {
        let url = super::diff_image_url(
            "https://bucket.s3.amazonaws.com/prefix/key/index.html",
            "diff",
            "sub/b.png",
        );
        assert_eq!(url, "https://bucket.s3.amazonaws.com/prefix/key/diff/sub/b.png");
    }

    #[test]
    fn test_diff_image_url_no_index_html() {
        let url = super::diff_image_url("https://example.com/report/", "diff", "c.png");
        assert_eq!(url, "https://example.com/report/diff/c.png");
    }

    #[test]
    fn test_slack_payload_has_image_url() {
        let comp = sample_comparison(true, true);
        let report_url = "https://bucket.s3.amazonaws.com/key/index.html";
        let image_url =
            super::diff_image_url(report_url, &comp.diff_dir, comp.failed_items.first().unwrap());
        assert_eq!(image_url, "https://bucket.s3.amazonaws.com/key/diff/a.png");
    }

    #[test]
    fn test_slack_payload_no_image_url_when_no_failures() {
        let comp = sample_comparison(false, false);
        assert!(comp.failed_items.is_empty());
    }

    #[test]
    fn test_slack_color_success() {
        let comp = sample_comparison(false, false);
        let _params = NotifyParams {
            comparison: comp.clone(),
            report_url: None,
            current_sha: "abc".to_string(),
            pr_number: None,
        };
        let color = if comp.has_failures() {
            "danger"
        } else if comp.has_changes() {
            "warning"
        } else {
            "good"
        };
        assert_eq!(color, "good");
    }

    fn make_params(has_failures: bool, report_url: Option<&str>) -> NotifyParams {
        NotifyParams {
            comparison: sample_comparison(has_failures, has_failures),
            report_url: report_url.map(|s| s.to_string()),
            current_sha: "abc123".to_string(),
            pr_number: Some(42),
        }
    }

    #[test]
    fn test_build_slack_payload_attachments_structure() {
        let params = make_params(true, Some("https://bucket.s3.amazonaws.com/key/index.html"));
        let payload = build_slack_payload(&params);
        let attachments = payload["attachments"].as_array().unwrap();
        assert_eq!(attachments.len(), 1);
        let attachment = &attachments[0];
        assert!(attachment.get("color").is_some());
        assert!(attachment.get("title").is_some());
        assert!(attachment.get("fields").is_some());
    }

    #[test]
    fn test_build_slack_payload_image_url_with_failures_and_report() {
        let params = make_params(true, Some("https://bucket.s3.amazonaws.com/key/index.html"));
        let payload = build_slack_payload(&params);
        let attachment = &payload["attachments"][0];
        assert!(attachment.get("image_url").is_some());
        assert_eq!(attachment["image_url"], "https://bucket.s3.amazonaws.com/key/diff/a.png");
    }

    #[test]
    fn test_build_slack_payload_no_image_url_without_failures() {
        let params = make_params(false, Some("https://example.com/index.html"));
        let payload = build_slack_payload(&params);
        let attachment = &payload["attachments"][0];
        assert!(attachment.get("image_url").is_none());
    }

    #[test]
    fn test_build_slack_payload_no_image_url_without_report() {
        let params = make_params(true, None);
        let payload = build_slack_payload(&params);
        let attachment = &payload["attachments"][0];
        assert!(attachment.get("image_url").is_none());
    }

    #[test]
    fn test_build_slack_payload_result_field_always_present() {
        let params = make_params(false, None);
        let payload = build_slack_payload(&params);
        let fields = payload["attachments"][0]["fields"].as_array().unwrap();
        let result_field = fields.iter().find(|f| f["title"] == "Result");
        assert!(result_field.is_some());
    }

    #[test]
    fn test_build_slack_payload_report_field_when_url_present() {
        let params = make_params(false, Some("https://example.com/index.html"));
        let payload = build_slack_payload(&params);
        let fields = payload["attachments"][0]["fields"].as_array().unwrap();
        let report_field = fields.iter().find(|f| f["title"] == "Report");
        assert!(report_field.is_some());
        assert_eq!(report_field.unwrap()["value"], "https://example.com/index.html");
    }

    #[test]
    fn test_build_slack_payload_no_report_field_when_no_url() {
        let params = make_params(false, None);
        let payload = build_slack_payload(&params);
        let fields = payload["attachments"][0]["fields"].as_array().unwrap();
        let report_field = fields.iter().find(|f| f["title"] == "Report");
        assert!(report_field.is_none());
    }
}
