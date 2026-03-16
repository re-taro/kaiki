use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn kaiki() -> Command {
    Command::cargo_bin("kaiki").unwrap()
}

fn write_config(dir: &Path, config: &serde_json::Value) -> PathBuf {
    let path = dir.join("regconfig.json");
    std::fs::write(&path, serde_json::to_string_pretty(config).unwrap()).unwrap();
    path
}

fn minimal_config() -> serde_json::Value {
    serde_json::json!({
        "core": { "actualDir": "actual", "workingDir": ".reg" },
        "plugins": {}
    })
}

/// Config using simple-keygen so we don't need a real git repository.
fn dry_run_config() -> serde_json::Value {
    serde_json::json!({
        "core": { "actualDir": "actual", "workingDir": ".reg" },
        "plugins": {
            "reg-simple-keygen-plugin": { "expectedKey": "test-key" }
        }
    })
}

// =========================================================================
// Group A: Basic CLI behaviour
// =========================================================================

#[test]
fn test_help_flag() {
    kaiki()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Visual regression testing tool"));
}

#[test]
fn test_run_subcommand_help() {
    kaiki()
        .args(["run", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Run full pipeline"));
}

#[test]
fn test_missing_config_file() {
    // -c is a global flag and must come before the subcommand
    kaiki()
        .args(["-c", "nonexistent_config_123.json", "run"])
        .assert()
        .failure()
        // tracing logs go to stdout in this binary
        .stdout(predicate::str::contains("failed to read config file"));
}

// =========================================================================
// Group B: prepare subcommand E2E
// =========================================================================

#[test]
fn test_prepare_valid_config() {
    let dir = TempDir::new().unwrap();
    let config_path = write_config(dir.path(), &minimal_config());

    kaiki().arg("-c").arg(&config_path).arg("prepare").assert().success();
}

#[test]
fn test_prepare_invalid_json() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bad.json");
    std::fs::write(&path, "{ not valid json !!!").unwrap();

    kaiki()
        .arg("-c")
        .arg(&path)
        .arg("prepare")
        .assert()
        .failure()
        .stdout(predicate::str::contains("failed to parse config"));
}

#[test]
fn test_prepare_validation_error_threshold() {
    let dir = TempDir::new().unwrap();
    let config = serde_json::json!({
        "core": {
            "actualDir": "actual",
            "workingDir": ".reg",
            "matchingThreshold": 5.0
        },
        "plugins": {}
    });
    let config_path = write_config(dir.path(), &config);

    kaiki()
        .arg("-c")
        .arg(&config_path)
        .arg("prepare")
        .assert()
        .failure()
        .stdout(predicate::str::contains("matchingThreshold"));
}

#[test]
fn test_prepare_custom_config_path() {
    let dir = TempDir::new().unwrap();
    let custom = dir.path().join("custom").join("my-config.json");
    std::fs::create_dir_all(custom.parent().unwrap()).unwrap();
    std::fs::write(&custom, serde_json::to_string_pretty(&minimal_config()).unwrap()).unwrap();

    kaiki().arg("-c").arg(&custom).arg("prepare").assert().success();
}

// =========================================================================
// Group C: dry-run mode
// =========================================================================

#[test]
fn test_run_dry_run_exits_success() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("actual")).unwrap();
    let config_path = write_config(dir.path(), &dry_run_config());

    kaiki()
        .arg("-t")
        .arg("-c")
        .arg(&config_path)
        .arg("run")
        .current_dir(dir.path())
        .assert()
        .success();
}

#[test]
fn test_compare_dry_run() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("actual")).unwrap();
    let config_path = write_config(dir.path(), &dry_run_config());

    kaiki()
        .arg("-t")
        .arg("-c")
        .arg(&config_path)
        .arg("compare")
        .current_dir(dir.path())
        .assert()
        .success();
}

#[test]
fn test_verbose_still_shows_info_logs() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("actual")).unwrap();
    let config_path = write_config(dir.path(), &dry_run_config());

    // --verbose sets log level to DEBUG, which is more permissive than INFO.
    // Verify that INFO-level messages still appear (ANSI codes surround the level).
    kaiki()
        .arg("-t")
        .arg("--verbose")
        .arg("-c")
        .arg(&config_path)
        .arg("run")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("INFO"));
}

// =========================================================================
// Group D: Error cases
// =========================================================================

#[test]
fn test_unknown_subcommand() {
    kaiki()
        .arg("foobar")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("unrecognized subcommand"));
}

#[test]
fn test_invalid_config_empty_actual_dir() {
    let dir = TempDir::new().unwrap();
    let config = serde_json::json!({
        "core": { "actualDir": "", "workingDir": ".reg" },
        "plugins": {}
    });
    let config_path = write_config(dir.path(), &config);

    kaiki()
        .arg("-c")
        .arg(&config_path)
        .arg("prepare")
        .assert()
        .failure()
        .stdout(predicate::str::contains("actualDir"));
}

#[test]
fn test_quiet_suppresses_info_output() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("actual")).unwrap();
    let config_path = write_config(dir.path(), &dry_run_config());

    kaiki()
        .arg("-t")
        .arg("--quiet")
        .arg("-c")
        .arg(&config_path)
        .arg("run")
        .current_dir(dir.path())
        .assert()
        .success()
        // In quiet mode, INFO-level logs should not appear
        .stdout(predicate::str::contains("INFO").not());
}
