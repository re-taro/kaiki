use std::path::Path;

use dialoguer::{Confirm, Input, MultiSelect, Select};

use crate::error::CliError;

/// Runs the interactive configuration wizard to create regconfig.json.
pub fn run_init_wizard() -> Result<bool, CliError> {
    if Path::new("regconfig.json").exists() {
        let overwrite = Confirm::new()
            .with_prompt("regconfig.json already exists. Overwrite?")
            .default(false)
            .interact()?;
        if !overwrite {
            tracing::info!("aborted");
            return Ok(false);
        }
    }

    let actual_dir: String = Input::new()
        .with_prompt("Directory containing actual images")
        .default("directory_contains_actual_images".into())
        .interact_text()?;

    let working_dir: String =
        Input::new().with_prompt("Working directory").default(".reg".into()).interact_text()?;

    let keygen_items =
        &["reg-keygen-git-hash-plugin (recommended)", "reg-simple-keygen-plugin", "None"];
    let keygen_idx = Select::new()
        .with_prompt("Key generator plugin")
        .items(keygen_items)
        .default(0)
        .interact()?;

    let expected_key = if keygen_idx == 1 {
        let key: String = Input::new()
            .with_prompt("Expected key (branch name or commit hash)")
            .interact_text()?;
        Some(key)
    } else {
        None
    };

    let storage_items = &["reg-publish-s3-plugin", "reg-publish-gcs-plugin", "None"];
    let storage_idx =
        Select::new().with_prompt("Storage plugin").items(storage_items).default(0).interact()?;

    let s3_bucket = if storage_idx == 0 {
        let bucket: String = Input::new().with_prompt("S3 bucket name").interact_text()?;
        Some(bucket)
    } else {
        None
    };

    let s3_region = if storage_idx == 0 {
        let region: String = Input::new()
            .with_prompt("S3 region (leave empty to skip)")
            .default(String::new())
            .interact_text()?;
        if region.is_empty() { None } else { Some(region) }
    } else {
        None
    };

    let s3_path_prefix = if storage_idx == 0 {
        let prefix: String = Input::new()
            .with_prompt("S3 path prefix (leave empty to skip)")
            .default(String::new())
            .interact_text()?;
        if prefix.is_empty() { None } else { Some(prefix) }
    } else {
        None
    };

    let s3_endpoint = if storage_idx == 0 {
        let ep: String = Input::new()
            .with_prompt("S3 custom endpoint (leave empty to skip)")
            .default(String::new())
            .interact_text()?;
        if ep.is_empty() { None } else { Some(ep) }
    } else {
        None
    };

    let s3_acl = if storage_idx == 0 {
        let acl: String = Input::new()
            .with_prompt("S3 ACL (leave empty to skip)")
            .default(String::new())
            .interact_text()?;
        if acl.is_empty() { None } else { Some(acl) }
    } else {
        None
    };

    let gcs_bucket = if storage_idx == 1 {
        let bucket: String = Input::new().with_prompt("GCS bucket name").interact_text()?;
        Some(bucket)
    } else {
        None
    };

    let gcs_path_prefix = if storage_idx == 1 {
        let prefix: String = Input::new()
            .with_prompt("GCS path prefix (leave empty to skip)")
            .default(String::new())
            .interact_text()?;
        if prefix.is_empty() { None } else { Some(prefix) }
    } else {
        None
    };

    let notifier_items = &["reg-notify-github-plugin", "reg-notify-slack-plugin"];
    let notifier_idxs = MultiSelect::new()
        .with_prompt("Notifier plugins (Space to select, Enter to confirm)")
        .items(notifier_items)
        .interact()?;

    let slack_webhook = if notifier_idxs.contains(&1) {
        let url: String = Input::new().with_prompt("Slack webhook URL").interact_text()?;
        Some(url)
    } else {
        None
    };

    let config = build_config_json(BuildConfigParams {
        actual_dir,
        working_dir,
        keygen_idx,
        expected_key,
        storage_idx,
        s3_bucket,
        s3_region,
        s3_path_prefix,
        s3_endpoint,
        s3_acl,
        gcs_bucket,
        gcs_path_prefix,
        notifier_idxs,
        slack_webhook,
    });

    std::fs::write("regconfig.json", serde_json::to_string_pretty(&config)?)?;
    tracing::info!("created regconfig.json");
    Ok(false)
}

#[derive(Debug)]
struct BuildConfigParams {
    actual_dir: String,
    working_dir: String,
    keygen_idx: usize,
    expected_key: Option<String>,
    storage_idx: usize,
    s3_bucket: Option<String>,
    s3_region: Option<String>,
    s3_path_prefix: Option<String>,
    s3_endpoint: Option<String>,
    s3_acl: Option<String>,
    gcs_bucket: Option<String>,
    gcs_path_prefix: Option<String>,
    notifier_idxs: Vec<usize>,
    slack_webhook: Option<String>,
}

fn build_config_json(p: BuildConfigParams) -> serde_json::Value {
    let mut plugins = serde_json::Map::new();

    match p.keygen_idx {
        0 => {
            plugins.insert("reg-keygen-git-hash-plugin".into(), serde_json::json!({}));
        }
        1 => {
            if let Some(key) = &p.expected_key {
                plugins.insert(
                    "reg-simple-keygen-plugin".into(),
                    serde_json::json!({ "expectedKey": key }),
                );
            }
        }
        _ => {}
    }

    match p.storage_idx {
        0 => {
            if let Some(bucket) = &p.s3_bucket {
                let mut cfg = serde_json::Map::new();
                cfg.insert("bucketName".into(), serde_json::Value::String(bucket.clone()));
                if let Some(r) = &p.s3_region {
                    cfg.insert("region".into(), serde_json::Value::String(r.clone()));
                }
                if let Some(pp) = &p.s3_path_prefix {
                    cfg.insert("pathPrefix".into(), serde_json::Value::String(pp.clone()));
                }
                if let Some(ep) = &p.s3_endpoint {
                    cfg.insert("endpoint".into(), serde_json::Value::String(ep.clone()));
                }
                if let Some(acl) = &p.s3_acl {
                    cfg.insert("acl".into(), serde_json::Value::String(acl.clone()));
                }
                plugins.insert("reg-publish-s3-plugin".into(), serde_json::Value::Object(cfg));
            }
        }
        1 => {
            if let Some(bucket) = &p.gcs_bucket {
                let mut cfg = serde_json::Map::new();
                cfg.insert("bucketName".into(), serde_json::Value::String(bucket.clone()));
                if let Some(pp) = &p.gcs_path_prefix {
                    cfg.insert("pathPrefix".into(), serde_json::Value::String(pp.clone()));
                }
                plugins.insert("reg-publish-gcs-plugin".into(), serde_json::Value::Object(cfg));
            }
        }
        _ => {}
    }

    for &idx in &p.notifier_idxs {
        match idx {
            0 => {
                plugins.insert("reg-notify-github-plugin".into(), serde_json::json!({}));
            }
            1 => {
                if let Some(url) = &p.slack_webhook {
                    plugins.insert(
                        "reg-notify-slack-plugin".into(),
                        serde_json::json!({ "webhookUrl": url }),
                    );
                }
            }
            _ => {}
        }
    }

    serde_json::json!({
        "core": {
            "actualDir": p.actual_dir,
            "workingDir": p.working_dir,
        },
        "plugins": serde_json::Value::Object(plugins),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_minimal_config() {
        let config = build_config_json(BuildConfigParams {
            actual_dir: "actual".into(),
            working_dir: ".reg".into(),
            keygen_idx: 2,
            expected_key: None,
            storage_idx: 2,
            s3_bucket: None,
            s3_region: None,
            s3_path_prefix: None,
            s3_endpoint: None,
            s3_acl: None,
            gcs_bucket: None,
            gcs_path_prefix: None,
            notifier_idxs: vec![],
            slack_webhook: None,
        });

        assert_eq!(config["core"]["actualDir"], "actual");
        assert_eq!(config["core"]["workingDir"], ".reg");
        assert!(config["plugins"].as_object().unwrap().is_empty());
    }

    #[test]
    fn test_build_git_keygen_s3_github() {
        let config = build_config_json(BuildConfigParams {
            actual_dir: "screenshots".into(),
            working_dir: ".reg".into(),
            keygen_idx: 0,
            expected_key: None,
            storage_idx: 0,
            s3_bucket: Some("my-bucket".into()),
            s3_region: Some("us-east-1".into()),
            s3_path_prefix: None,
            s3_endpoint: None,
            s3_acl: None,
            gcs_bucket: None,
            gcs_path_prefix: None,
            notifier_idxs: vec![0],
            slack_webhook: None,
        });

        let plugins = config["plugins"].as_object().unwrap();
        assert!(plugins.contains_key("reg-keygen-git-hash-plugin"));
        assert_eq!(plugins["reg-publish-s3-plugin"]["bucketName"], "my-bucket");
        assert_eq!(plugins["reg-publish-s3-plugin"]["region"], "us-east-1");
        assert!(!plugins["reg-publish-s3-plugin"].as_object().unwrap().contains_key("pathPrefix"));
        assert!(plugins.contains_key("reg-notify-github-plugin"));
    }

    #[test]
    fn test_build_simple_keygen_gcs_slack() {
        let config = build_config_json(BuildConfigParams {
            actual_dir: "imgs".into(),
            working_dir: ".reg".into(),
            keygen_idx: 1,
            expected_key: Some("main".into()),
            storage_idx: 1,
            s3_bucket: None,
            s3_region: None,
            s3_path_prefix: None,
            s3_endpoint: None,
            s3_acl: None,
            gcs_bucket: Some("gcs-bucket".into()),
            gcs_path_prefix: Some("prefix/".into()),
            notifier_idxs: vec![1],
            slack_webhook: Some("https://hooks.slack.com/test".into()),
        });

        let plugins = config["plugins"].as_object().unwrap();
        assert_eq!(plugins["reg-simple-keygen-plugin"]["expectedKey"], "main");
        assert_eq!(plugins["reg-publish-gcs-plugin"]["bucketName"], "gcs-bucket");
        assert_eq!(plugins["reg-publish-gcs-plugin"]["pathPrefix"], "prefix/");
        assert_eq!(
            plugins["reg-notify-slack-plugin"]["webhookUrl"],
            "https://hooks.slack.com/test"
        );
    }

    #[test]
    fn test_build_s3_all_optional_fields() {
        let config = build_config_json(BuildConfigParams {
            actual_dir: "actual".into(),
            working_dir: ".reg".into(),
            keygen_idx: 0,
            expected_key: None,
            storage_idx: 0,
            s3_bucket: Some("bucket".into()),
            s3_region: Some("ap-northeast-1".into()),
            s3_path_prefix: Some("p/".into()),
            s3_endpoint: Some("http://localhost:9000".into()),
            s3_acl: Some("private".into()),
            gcs_bucket: None,
            gcs_path_prefix: None,
            notifier_idxs: vec![0, 1],
            slack_webhook: Some("https://hooks.slack.com/x".into()),
        });

        let s3 = &config["plugins"]["reg-publish-s3-plugin"];
        assert_eq!(s3["bucketName"], "bucket");
        assert_eq!(s3["region"], "ap-northeast-1");
        assert_eq!(s3["pathPrefix"], "p/");
        assert_eq!(s3["endpoint"], "http://localhost:9000");
        assert_eq!(s3["acl"], "private");

        let plugins = config["plugins"].as_object().unwrap();
        assert!(plugins.contains_key("reg-notify-github-plugin"));
        assert!(plugins.contains_key("reg-notify-slack-plugin"));
    }
}
