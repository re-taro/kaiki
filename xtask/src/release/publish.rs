use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

use super::version;

/// Publish all workspace crates to crates.io in topological order.
#[expect(clippy::print_stdout)]
pub fn run(dry_run: bool) {
    let root = workspace_root();

    // Verify clean working directory (skip in dry-run)
    if !dry_run {
        verify_clean_working_dir();
    }

    let current_version = version::read_workspace_version(&root);
    println!("Publishing version: {current_version}");

    // Get workspace metadata
    let metadata = cargo_metadata::MetadataCommand::new()
        .no_deps()
        .exec()
        .expect("failed to run cargo metadata");

    // Filter publishable workspace crates
    let workspace_members: HashSet<&str> =
        metadata.workspace_members.iter().map(|id| id.repr.as_str()).collect();

    let publishable: Vec<&cargo_metadata::Package> = metadata
        .packages
        .iter()
        .filter(|pkg| workspace_members.contains(pkg.id.repr.as_str()))
        .filter(|pkg| pkg.publish.is_none())
        .collect();

    if publishable.is_empty() {
        println!("No publishable crates found.");
        return;
    }

    // Topological sort
    let ordered = topological_sort(&publishable, &metadata);

    println!("\nPublish order:");
    for (i, name) in ordered.iter().enumerate() {
        println!("  {}: {name}", i + 1);
    }
    println!();

    let client = reqwest::blocking::Client::builder()
        .user_agent("kaiki-xtask-release")
        .build()
        .expect("failed to build HTTP client");

    for name in &ordered {
        // Check if already published
        if is_already_published(&client, name, &current_version) {
            println!("[skip] {name}@{current_version} already published");
            continue;
        }

        println!("[publish] {name}@{current_version}");

        let mut cmd = Command::new("cargo");
        cmd.args(["publish", "-p", name]);
        if dry_run {
            cmd.arg("--dry-run");
        }
        cmd.current_dir(&root);

        let status =
            cmd.status().unwrap_or_else(|e| panic!("failed to run cargo publish for {name}: {e}"));
        assert!(status.success(), "cargo publish failed for {name}");

        // Wait for crates.io index to update (skip for last crate or dry-run)
        if !dry_run {
            println!("  waiting 10s for crates.io index update...");
            thread::sleep(Duration::from_secs(10));
        }
    }

    println!("\nAll crates published successfully.");
}

fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

#[expect(clippy::print_stdout)]
fn verify_clean_working_dir() {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .expect("failed to run git status");

    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.trim().is_empty() {
        println!("Working directory is not clean:");
        println!("{stdout}");
        panic!("clean working directory required for publish");
    }
}

fn topological_sort(
    packages: &[&cargo_metadata::Package],
    metadata: &cargo_metadata::Metadata,
) -> Vec<String> {
    let pkg_names: HashSet<String> = packages.iter().map(|p| p.name.to_string()).collect();

    // Build adjacency: name -> set of internal dependencies
    let mut deps: HashMap<String, HashSet<String>> = HashMap::new();
    for pkg in packages {
        let internal_deps: HashSet<String> = pkg
            .dependencies
            .iter()
            .filter(|d| pkg_names.contains(d.name.as_str()))
            .map(|d| d.name.clone())
            .collect();
        deps.insert(pkg.name.to_string(), internal_deps);
    }

    // Also check resolve for more accurate dependency info
    if let Some(resolve) = &metadata.resolve {
        for node in &resolve.nodes {
            if let Some(pkg) = packages.iter().find(|p| p.id == node.id) {
                let entry = deps.entry(pkg.name.to_string()).or_default();
                for dep_id in &node.deps {
                    let name = dep_id.name.clone();
                    if pkg_names.contains(&name) {
                        entry.insert(name);
                    }
                }
            }
        }
    }

    // Kahn's algorithm
    let mut remaining_deps: HashMap<String, HashSet<String>> = deps;
    let mut queue: VecDeque<String> = VecDeque::new();
    let mut result: Vec<String> = Vec::new();

    // Start with packages that have no internal dependencies
    for name in &pkg_names {
        if remaining_deps.get(name).is_none_or(HashSet::is_empty) {
            queue.push_back(name.clone());
        }
    }

    // Sort queue for deterministic order
    let mut sorted_queue: Vec<String> = queue.drain(..).collect();
    sorted_queue.sort();
    queue.extend(sorted_queue);

    while let Some(name) = queue.pop_front() {
        result.push(name.clone());

        // Remove this package from all dependency sets
        let mut newly_ready: Vec<String> = Vec::new();
        for (pkg_name, dep_set) in &mut remaining_deps {
            if dep_set.remove(&name) && dep_set.is_empty() && !result.contains(pkg_name) {
                newly_ready.push(pkg_name.clone());
            }
        }
        newly_ready.sort();
        queue.extend(newly_ready);
    }

    assert_eq!(
        result.len(),
        pkg_names.len(),
        "topological sort failed: circular dependency detected"
    );

    result
}

fn is_already_published(client: &reqwest::blocking::Client, name: &str, version: &str) -> bool {
    let url = format!("https://crates.io/api/v1/crates/{name}");
    let Ok(resp) = client.get(&url).send() else {
        return false;
    };

    if !resp.status().is_success() {
        return false;
    }

    let Ok(body) = resp.json::<serde_json::Value>() else {
        return false;
    };

    // Check if the version exists in the versions array
    body["versions"]
        .as_array()
        .is_some_and(|versions| versions.iter().any(|v| v["num"].as_str() == Some(version)))
}
