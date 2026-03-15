use std::path::Path;

use kaiki_config::{
    GcsPluginConfig, RegSuitConfiguration, S3PluginConfig, SimpleKeygenConfig, SlackNotifyConfig,
};

use crate::error::CliError;

/// Validates the configuration and prepares the working directory.
pub fn run_prepare(config_path: &Path) -> Result<bool, CliError> {
    let config = kaiki_config::load_config(config_path)?;
    validate_config(&config)?;

    let working = Path::new(&config.core.working_dir);
    std::fs::create_dir_all(working)?;
    tracing::info!(path = %working.display(), "working directory ready");

    let plugin_count = config.plugins.len();
    tracing::info!(plugins = plugin_count, "configuration validated");
    Ok(false)
}

fn ensure(condition: bool, msg: impl Into<String>) -> Result<(), CliError> {
    if condition { Ok(()) } else { Err(CliError::Validation(msg.into())) }
}

fn validate_config(config: &RegSuitConfiguration) -> Result<(), CliError> {
    ensure(!config.core.actual_dir.is_empty(), "actualDir must not be empty")?;
    ensure(!config.core.working_dir.is_empty(), "workingDir must not be empty")?;

    if let Some(mt) = config.core.matching_threshold {
        ensure(
            (0.0..=1.0).contains(&mt),
            format!("matchingThreshold must be between 0.0 and 1.0, got {mt}"),
        )?;
    }
    if let Some(tr) = kaiki_config::effective_threshold_rate(&config.core) {
        ensure(
            (0.0..=1.0).contains(&tr),
            format!("thresholdRate must be between 0.0 and 1.0, got {tr}"),
        )?;
    }
    if let Some(a) = config.core.alpha {
        ensure((0.0..=1.0).contains(&a), format!("alpha must be between 0.0 and 1.0, got {a}"))?;
    }

    let mut keygen_count = 0u32;
    let mut storage_count = 0u32;

    for (name, value) in &config.plugins {
        match name.as_str() {
            "reg-keygen-git-hash-plugin" => {
                keygen_count += 1;
            }
            "reg-simple-keygen-plugin" => {
                keygen_count += 1;
                serde_json::from_value::<SimpleKeygenConfig>(value.clone()).map_err(|e| {
                    CliError::Validation(format!("invalid reg-simple-keygen-plugin config: {e}"))
                })?;
            }
            "reg-publish-s3-plugin" => {
                storage_count += 1;
                serde_json::from_value::<S3PluginConfig>(value.clone()).map_err(|e| {
                    CliError::Validation(format!("invalid reg-publish-s3-plugin config: {e}"))
                })?;
            }
            "reg-publish-gcs-plugin" => {
                storage_count += 1;
                serde_json::from_value::<GcsPluginConfig>(value.clone()).map_err(|e| {
                    CliError::Validation(format!("invalid reg-publish-gcs-plugin config: {e}"))
                })?;
            }
            "reg-notify-github-plugin" => {}
            "reg-notify-slack-plugin" => {
                serde_json::from_value::<SlackNotifyConfig>(value.clone()).map_err(|e| {
                    CliError::Validation(format!("invalid reg-notify-slack-plugin config: {e}"))
                })?;
            }
            unknown => {
                tracing::warn!(plugin = unknown, "unknown plugin, skipping validation");
            }
        }
    }

    ensure(keygen_count <= 1, format!("at most one keygen plugin allowed, found {keygen_count}"))?;
    ensure(
        storage_count <= 1,
        format!("at most one storage plugin allowed, found {storage_count}"),
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_config() -> RegSuitConfiguration {
        serde_json::from_value(serde_json::json!({
            "core": {
                "actualDir": "actual",
                "workingDir": ".reg"
            },
            "plugins": {}
        }))
        .unwrap()
    }

    #[test]
    fn test_valid_minimal() {
        assert!(validate_config(&minimal_config()).is_ok());
    }

    #[test]
    fn test_empty_actual_dir() {
        let mut cfg = minimal_config();
        cfg.core.actual_dir = String::new();
        let err = validate_config(&cfg).unwrap_err();
        assert!(err.to_string().contains("actualDir"));
    }

    #[test]
    fn test_empty_working_dir() {
        let mut cfg = minimal_config();
        cfg.core.working_dir = String::new();
        let err = validate_config(&cfg).unwrap_err();
        assert!(err.to_string().contains("workingDir"));
    }

    #[test]
    fn test_matching_threshold_out_of_range() {
        let mut cfg = minimal_config();
        cfg.core.matching_threshold = Some(1.5);
        let err = validate_config(&cfg).unwrap_err();
        assert!(err.to_string().contains("matchingThreshold"));
    }

    #[test]
    fn test_threshold_rate_out_of_range() {
        let mut cfg = minimal_config();
        cfg.core.threshold_rate = Some(-0.1);
        let err = validate_config(&cfg).unwrap_err();
        assert!(err.to_string().contains("thresholdRate"));
    }

    #[test]
    fn test_alpha_out_of_range() {
        let mut cfg = minimal_config();
        cfg.core.alpha = Some(2.0);
        let err = validate_config(&cfg).unwrap_err();
        assert!(err.to_string().contains("alpha"));
    }

    #[test]
    fn test_valid_with_all_plugins() {
        let cfg: RegSuitConfiguration = serde_json::from_value(serde_json::json!({
            "core": { "actualDir": "a", "workingDir": ".reg" },
            "plugins": {
                "reg-keygen-git-hash-plugin": {},
                "reg-publish-s3-plugin": { "bucketName": "b" },
                "reg-notify-github-plugin": {},
                "reg-notify-slack-plugin": { "webhookUrl": "https://x" }
            }
        }))
        .unwrap();
        assert!(validate_config(&cfg).is_ok());
    }

    #[test]
    fn test_multiple_keygen_rejected() {
        let cfg: RegSuitConfiguration = serde_json::from_value(serde_json::json!({
            "core": { "actualDir": "a", "workingDir": ".reg" },
            "plugins": {
                "reg-keygen-git-hash-plugin": {},
                "reg-simple-keygen-plugin": { "expectedKey": "main" }
            }
        }))
        .unwrap();
        let err = validate_config(&cfg).unwrap_err();
        assert!(err.to_string().contains("keygen"));
    }

    #[test]
    fn test_multiple_storage_rejected() {
        let cfg: RegSuitConfiguration = serde_json::from_value(serde_json::json!({
            "core": { "actualDir": "a", "workingDir": ".reg" },
            "plugins": {
                "reg-publish-s3-plugin": { "bucketName": "b1" },
                "reg-publish-gcs-plugin": { "bucketName": "b2" }
            }
        }))
        .unwrap();
        let err = validate_config(&cfg).unwrap_err();
        assert!(err.to_string().contains("storage"));
    }

    #[test]
    fn test_invalid_s3_config() {
        let cfg: RegSuitConfiguration = serde_json::from_value(serde_json::json!({
            "core": { "actualDir": "a", "workingDir": ".reg" },
            "plugins": {
                "reg-publish-s3-plugin": { "wrong": "field" }
            }
        }))
        .unwrap();
        let err = validate_config(&cfg).unwrap_err();
        assert!(err.to_string().contains("reg-publish-s3-plugin"));
    }

    #[test]
    fn test_invalid_slack_config() {
        let cfg: RegSuitConfiguration = serde_json::from_value(serde_json::json!({
            "core": { "actualDir": "a", "workingDir": ".reg" },
            "plugins": {
                "reg-notify-slack-plugin": {}
            }
        }))
        .unwrap();
        let err = validate_config(&cfg).unwrap_err();
        assert!(err.to_string().contains("reg-notify-slack-plugin"));
    }

    #[test]
    fn test_unknown_plugin_allowed() {
        let cfg: RegSuitConfiguration = serde_json::from_value(serde_json::json!({
            "core": { "actualDir": "a", "workingDir": ".reg" },
            "plugins": {
                "some-custom-plugin": { "x": 1 }
            }
        }))
        .unwrap();
        assert!(validate_config(&cfg).is_ok());
    }
}
