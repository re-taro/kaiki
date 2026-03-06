mod env;

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("environment variable not found: {0}")]
    EnvVar(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegSuitConfiguration {
    pub core: CoreConfig,
    #[serde(default)]
    pub plugins: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreConfig {
    #[serde(default = "default_actual_dir")]
    pub actual_dir: String,
    #[serde(default = "default_working_dir")]
    pub working_dir: String,
    pub threshold: Option<f64>,
    pub threshold_rate: Option<f64>,
    pub threshold_pixel: Option<u64>,
    #[serde(default)]
    pub matching_threshold: Option<f64>,
    #[serde(default)]
    pub enable_antialias: Option<bool>,
    pub ximgdiff: Option<XimgdiffConfig>,
    pub concurrency: Option<u32>,
    #[serde(default)]
    pub diff_color: Option<[u8; 3]>,
    #[serde(default)]
    pub diff_color_alt: Option<[u8; 3]>,
    #[serde(default)]
    pub aa_color: Option<[u8; 3]>,
    #[serde(default)]
    pub alpha: Option<f64>,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            actual_dir: default_actual_dir(),
            working_dir: default_working_dir(),
            threshold: None,
            threshold_rate: None,
            threshold_pixel: None,
            matching_threshold: None,
            enable_antialias: None,
            ximgdiff: None,
            concurrency: None,
            diff_color: None,
            diff_color_alt: None,
            aa_color: None,
            alpha: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XimgdiffConfig {
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct S3PluginConfig {
    pub bucket_name: String,
    #[serde(default)]
    pub acl: Option<String>,
    #[serde(default)]
    pub sse: Option<bool>,
    #[serde(default, rename = "sseKMSKeyId", alias = "sseKmsKeyId")]
    pub sse_kms_key_id: Option<String>,
    #[serde(default)]
    pub path_prefix: Option<String>,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GcsPluginConfig {
    pub bucket_name: String,
    #[serde(default)]
    pub path_prefix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubNotifyConfig {
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default = "default_pr_comment")]
    pub pr_comment: bool,
    #[serde(default = "default_pr_comment_behavior")]
    pub pr_comment_behavior: String,
    #[serde(default = "default_set_commit_status")]
    pub set_commit_status: bool,
}

impl Default for GitHubNotifyConfig {
    fn default() -> Self {
        Self {
            client_id: None,
            owner: None,
            repository: None,
            pr_comment: default_pr_comment(),
            pr_comment_behavior: default_pr_comment_behavior(),
            set_commit_status: default_set_commit_status(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlackNotifyConfig {
    pub webhook_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimpleKeygenConfig {
    pub expected_key: String,
}

fn default_actual_dir() -> String {
    "directory_contains_actual_images".to_string()
}

fn default_working_dir() -> String {
    ".reg".to_string()
}

fn default_pr_comment() -> bool {
    true
}

fn default_pr_comment_behavior() -> String {
    "default".to_string()
}

fn default_set_commit_status() -> bool {
    true
}

/// Load and parse a regconfig.json file.
pub fn load_config(path: &Path) -> Result<RegSuitConfiguration, ConfigError> {
    let content = std::fs::read_to_string(path)?;
    let expanded = env::expand_env_vars(&content)?;
    let config: RegSuitConfiguration = serde_json::from_str(&expanded)?;
    Ok(config)
}

/// Resolve the effective matching threshold from config.
pub fn effective_matching_threshold(core: &CoreConfig) -> f64 {
    core.matching_threshold.unwrap_or(0.0)
}

/// Resolve the effective concurrency from config.
pub fn effective_concurrency(core: &CoreConfig) -> u32 {
    core.concurrency.unwrap_or(4)
}

/// Resolve the effective threshold rate, considering the legacy `threshold` alias.
pub fn effective_threshold_rate(core: &CoreConfig) -> Option<f64> {
    core.threshold_rate.or(core.threshold)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_minimal_config() {
        let json = r#"{ "core": {} }"#;
        let config: RegSuitConfiguration = serde_json::from_str(json).unwrap();
        assert_eq!(config.core.actual_dir, "directory_contains_actual_images");
        assert_eq!(config.core.working_dir, ".reg");
    }

    #[test]
    fn test_load_full_config() {
        let json = r#"{
            "core": {
                "actualDir": "screenshots",
                "workingDir": ".regwork",
                "threshold": 0.05,
                "matchingThreshold": 0.1,
                "enableAntialias": true,
                "concurrency": 8
            },
            "plugins": {
                "reg-keygen-git-hash-plugin": {}
            }
        }"#;
        let config: RegSuitConfiguration = serde_json::from_str(json).unwrap();
        assert_eq!(config.core.actual_dir, "screenshots");
        assert_eq!(config.core.working_dir, ".regwork");
        assert_eq!(config.core.threshold, Some(0.05));
        assert_eq!(config.core.matching_threshold, Some(0.1));
        assert_eq!(config.core.enable_antialias, Some(true));
        assert_eq!(config.core.concurrency, Some(8));
        assert!(config.plugins.contains_key("reg-keygen-git-hash-plugin"));
    }

    #[test]
    fn test_default_values() {
        let core = CoreConfig::default();
        assert_eq!(core.actual_dir, "directory_contains_actual_images");
        assert_eq!(core.working_dir, ".reg");
        assert!(core.threshold.is_none());
        assert!(core.matching_threshold.is_none());
        assert!(core.enable_antialias.is_none());
    }

    #[test]
    fn test_effective_matching_threshold() {
        let core = CoreConfig::default();
        assert_eq!(effective_matching_threshold(&core), 0.0);

        let core = CoreConfig { matching_threshold: Some(0.05), ..CoreConfig::default() };
        assert_eq!(effective_matching_threshold(&core), 0.05);
    }

    #[test]
    fn test_effective_threshold_rate() {
        let core = CoreConfig::default();
        assert_eq!(effective_threshold_rate(&core), None);

        let core = CoreConfig { threshold_rate: Some(0.1), ..CoreConfig::default() };
        assert_eq!(effective_threshold_rate(&core), Some(0.1));

        // Legacy threshold as fallback
        let core = CoreConfig { threshold: Some(0.2), ..CoreConfig::default() };
        assert_eq!(effective_threshold_rate(&core), Some(0.2));

        // threshold_rate takes precedence
        let core =
            CoreConfig { threshold_rate: Some(0.1), threshold: Some(0.2), ..CoreConfig::default() };
        assert_eq!(effective_threshold_rate(&core), Some(0.1));
    }

    #[test]
    fn test_effective_concurrency() {
        let core = CoreConfig::default();
        assert_eq!(effective_concurrency(&core), 4);

        let core = CoreConfig { concurrency: Some(16), ..CoreConfig::default() };
        assert_eq!(effective_concurrency(&core), 16);
    }

    #[test]
    fn test_plugin_config_deserialization() {
        // S3
        let json = r#"{ "bucketName": "my-bucket", "region": "us-east-1" }"#;
        let config: S3PluginConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.bucket_name, "my-bucket");
        assert_eq!(config.region, Some("us-east-1".to_string()));

        // GCS
        let json = r#"{ "bucketName": "gcs-bucket" }"#;
        let config: GcsPluginConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.bucket_name, "gcs-bucket");

        // GitHub
        let config = GitHubNotifyConfig::default();
        assert!(config.pr_comment);
        assert_eq!(config.pr_comment_behavior, "default");
        assert!(config.set_commit_status);

        // Slack
        let json = r#"{ "webhookUrl": "https://hooks.slack.com/services/xxx" }"#;
        let config: SlackNotifyConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.webhook_url, "https://hooks.slack.com/services/xxx");
    }

    #[test]
    fn test_env_expansion_in_config() {
        // SAFETY: test-only, single-threaded test runner
        unsafe { std::env::set_var("KAIKI_TEST_BUCKET", "test-bucket") };
        let json =
            r#"{ "core": {}, "plugins": { "s3": { "bucketName": "${KAIKI_TEST_BUCKET}" } } }"#;
        let expanded = env::expand_env_vars(json).unwrap();
        assert!(expanded.contains("test-bucket"));
    }

    #[test]
    fn test_s3_bucket_name_required() {
        let json = r#"{ "region": "us-east-1" }"#;
        let result = serde_json::from_str::<S3PluginConfig>(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_plugins_object() {
        let json = r#"{ "core": {}, "plugins": {} }"#;
        let config: RegSuitConfiguration = serde_json::from_str(json).unwrap();
        assert!(config.plugins.is_empty());
        assert_eq!(config.core.actual_dir, "directory_contains_actual_images");
        assert_eq!(config.core.working_dir, ".reg");
    }

    #[test]
    fn test_s3_sse_kms_deserialization() {
        // sseKMSKeyId (reg-suit compatible uppercase KMS)
        let json = r#"{ "bucketName": "b", "sseKMSKeyId": "arn:aws:kms:us-east-1:123:key/abc" }"#;
        let config: S3PluginConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.sse_kms_key_id, Some("arn:aws:kms:us-east-1:123:key/abc".to_string()));

        // sseKmsKeyId (alias camelCase)
        let json = r#"{ "bucketName": "b", "sseKmsKeyId": "arn:aws:kms:us-east-1:123:key/def" }"#;
        let config: S3PluginConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.sse_kms_key_id, Some("arn:aws:kms:us-east-1:123:key/def".to_string()));

        // Without KMS key — backward compatible
        let json = r#"{ "bucketName": "b", "sse": true }"#;
        let config: S3PluginConfig = serde_json::from_str(json).unwrap();
        assert!(config.sse_kms_key_id.is_none());
        assert_eq!(config.sse, Some(true));
    }
}
