use std::fs;
use std::path::Path;
use std::process::Command;

use git_cliff_core::changelog::Changelog;
use git_cliff_core::commit::Commit as CliffCommit;
use git_cliff_core::config::Config as CliffConfig;
use git_cliff_core::release::Release;
use git_cliff_core::repo::Repository;

use super::version::{self, BumpType};

/// Run the full release update: detect bump, generate changelog, update versions.
#[expect(clippy::print_stdout)]
pub fn run(dry_run: bool) {
    let root = workspace_root();

    let current_version = version::read_workspace_version(&root);
    println!("Current version: {current_version}");

    let latest_tag = find_latest_tag();
    println!("Latest tag: {}", latest_tag.as_deref().unwrap_or("(none)"));

    let range = latest_tag.as_deref().map(|tag| format!("{tag}..HEAD"));

    // Load cliff config
    let cliff_toml_path = root.join("cliff.toml");
    let cliff_config = CliffConfig::load(&cliff_toml_path).expect("failed to load cliff.toml");

    // Open repository and collect commits
    let repo = Repository::init(root.clone()).expect("failed to init git-cliff repository");
    let git_commits = repo
        .commits(range.as_deref(), None, None, false)
        .expect("failed to collect commits");

    if git_commits.is_empty() {
        println!("No commits found since last tag. Nothing to release.");
        return;
    }

    // Convert git2::Commit to git_cliff_core::commit::Commit
    let cliff_commits: Vec<CliffCommit> =
        git_commits.iter().map(CliffCommit::from).collect();

    // Detect bump type from commits
    let bump = detect_bump_from_commits(&cliff_commits);
    let new_version = version::bump_version(&current_version, bump);
    println!("Detected bump: {bump:?} -> {new_version}");

    // Generate changelog entry
    let changelog_entry = generate_changelog(cliff_config, &cliff_commits, &new_version);

    if dry_run {
        println!("\n--- DRY RUN ---");
        println!("\nNew version: {new_version}");
        println!("\nChangelog entry:\n{changelog_entry}");
        println!("\nFiles that would be updated:");
        println!("  - Cargo.toml (workspace version + deps)");
        println!("  - npm/kaiki/package.json");
        println!("  - napi/kaiki/package.json");
        println!("  - wasm/kaiki_diff_wasm/package.json");
        println!("  - CHANGELOG.md");
        println!("  - Cargo.lock (via cargo update)");
        return;
    }

    // Update Cargo.toml
    version::update_workspace_cargo_toml(&root, &new_version);
    println!("Updated Cargo.toml");

    // Update package.json files
    version::update_package_json_files(&root, &new_version);
    println!("Updated package.json files");

    // Update CHANGELOG.md
    update_changelog(&root, &changelog_entry);
    println!("Updated CHANGELOG.md");

    // Update Cargo.lock
    let status = Command::new("cargo")
        .args(["update", "--workspace"])
        .current_dir(&root)
        .status()
        .expect("failed to run cargo update");
    assert!(status.success(), "cargo update --workspace failed");
    println!("Updated Cargo.lock");

    println!("\nRelease v{new_version} prepared successfully.");
    println!("Review the changes and commit when ready.");
}

/// Generate changelog only (no version bump or file updates).
#[expect(clippy::print_stdout)]
pub fn changelog_only() {
    let root = workspace_root();
    let latest_tag = find_latest_tag();
    let range = latest_tag.as_deref().map(|tag| format!("{tag}..HEAD"));

    let cliff_toml_path = root.join("cliff.toml");
    let cliff_config = CliffConfig::load(&cliff_toml_path).expect("failed to load cliff.toml");

    let repo = Repository::init(root).expect("failed to init git-cliff repository");
    let git_commits = repo
        .commits(range.as_deref(), None, None, false)
        .expect("failed to collect commits");

    if git_commits.is_empty() {
        println!("No commits found since last tag.");
        return;
    }

    let cliff_commits: Vec<CliffCommit> =
        git_commits.iter().map(CliffCommit::from).collect();

    let changelog = generate_changelog(cliff_config, &cliff_commits, "Unreleased");
    println!("{changelog}");
}

fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn find_latest_tag() -> Option<String> {
    let output = Command::new("git")
        .args(["describe", "--tags", "--abbrev=0", "--match", "v*"])
        .output()
        .ok()?;

    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

fn detect_bump_from_commits(commits: &[CliffCommit]) -> BumpType {
    let mut has_feat = false;

    for commit in commits {
        let msg = &commit.message;

        // Check for breaking changes
        if msg.contains("BREAKING CHANGE") || msg.contains("BREAKING-CHANGE") {
            return BumpType::Major;
        }

        // Check for ! in conventional commit type (e.g. "feat!:" or "fix!:")
        if let Some(colon_pos) = msg.find(':') {
            let prefix = &msg[..colon_pos];
            if prefix.contains('!') {
                return BumpType::Major;
            }
        }

        if msg.starts_with("feat") {
            has_feat = true;
        }
    }

    if has_feat { BumpType::Minor } else { BumpType::Patch }
}

fn generate_changelog(
    config: CliffConfig,
    commits: &[CliffCommit],
    version: &str,
) -> String {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_secs()
        .cast_signed();

    let release = Release {
        version: Some(version.to_string()),
        commits: commits.to_vec(),
        timestamp: Some(timestamp),
        ..Release::default()
    };

    let changelog =
        Changelog::new(vec![release], config, None).expect("failed to create changelog");
    let mut output = Vec::new();
    changelog.generate(&mut output).expect("failed to generate changelog");
    String::from_utf8(output).expect("changelog is not valid UTF-8")
}

fn update_changelog(root: &Path, new_entry: &str) {
    let changelog_path = root.join("CHANGELOG.md");

    let existing = if changelog_path.exists() {
        fs::read_to_string(&changelog_path).expect("failed to read CHANGELOG.md")
    } else {
        String::new()
    };

    // Find the position after the header to insert the new entry
    let content = if existing.is_empty() {
        format!(
            "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n{new_entry}"
        )
    } else if let Some(pos) = existing.find("\n## ") {
        // Insert before the first ## section
        let (before, after) = existing.split_at(pos.saturating_add(1));
        format!("{before}{new_entry}\n{after}")
    } else {
        // No existing sections, append after content
        format!("{existing}\n{new_entry}")
    };

    fs::write(&changelog_path, content).expect("failed to write CHANGELOG.md");
}
