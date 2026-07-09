use std::fs;
use std::path::Path;

const WORKSPACE_CRATE_NAMES: &[&str] = &[
    "kaiki_config",
    "kaiki_core",
    "kaiki_diff",
    "kaiki_git",
    "kaiki_notify",
    "kaiki_report",
    "kaiki_storage",
];

const PACKAGE_JSON_FILES: &[&str] =
    &["npm/kaiki/package.json", "napi/kaiki/package.json", "wasm/kaiki_diff_wasm/package.json"];

/// Update `[workspace.package] version` and `[workspace.dependencies] kaiki_* version`
/// in the root `Cargo.toml`.
pub fn update_workspace_cargo_toml(root: &Path, new_version: &str) {
    let cargo_toml_path = root.join("Cargo.toml");
    let content = fs::read_to_string(&cargo_toml_path).expect("failed to read root Cargo.toml");
    let mut doc: toml_edit::DocumentMut = content.parse().expect("failed to parse root Cargo.toml");

    // [workspace.package] version
    doc["workspace"]["package"]["version"] = toml_edit::value(new_version);

    // [workspace.dependencies] kaiki_* version
    for name in WORKSPACE_CRATE_NAMES {
        doc["workspace"]["dependencies"][name]["version"] = toml_edit::value(new_version);
    }

    fs::write(&cargo_toml_path, doc.to_string()).expect("failed to write root Cargo.toml");
}

/// Update `version` (and `optionalDependencies` if present) in each `package.json`.
pub fn update_package_json_files(root: &Path, new_version: &str) {
    for rel_path in PACKAGE_JSON_FILES {
        let path = root.join(rel_path);
        if !path.exists() {
            continue;
        }
        update_single_package_json(&path, new_version);
    }
}

fn update_single_package_json(path: &Path, new_version: &str) {
    let content = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    let mut pkg: serde_json::Value = serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()));

    pkg["version"] = serde_json::Value::String(new_version.to_string());

    // Update optionalDependencies versions if present
    if let Some(deps) = pkg.get_mut("optionalDependencies")
        && let Some(obj) = deps.as_object_mut()
    {
        for value in obj.values_mut() {
            *value = serde_json::Value::String(new_version.to_string());
        }
    }

    let output = serde_json::to_string_pretty(&pkg)
        .unwrap_or_else(|e| panic!("failed to serialize {}: {e}", path.display()));
    fs::write(path, output + "\n")
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", path.display()));
}

/// Read the current workspace version from root `Cargo.toml`.
pub fn read_workspace_version(root: &Path) -> String {
    let cargo_toml_path = root.join("Cargo.toml");
    let content = fs::read_to_string(&cargo_toml_path).expect("failed to read root Cargo.toml");
    let doc: toml_edit::DocumentMut = content.parse().expect("failed to parse root Cargo.toml");
    doc["workspace"]["package"]["version"]
        .as_str()
        .expect("workspace.package.version not found")
        .to_string()
}

/// Compute the next version given the current version and bump type.
pub fn bump_version(current: &str, bump: BumpType) -> String {
    let parts: Vec<u64> =
        current.split('.').map(|s| s.parse().expect("invalid version part")).collect();
    assert!(parts.len() == 3, "expected semver x.y.z, got {current}");

    let (major, minor, patch) = (parts[0], parts[1], parts[2]);
    match bump {
        BumpType::Major => format!("{}.0.0", major + 1),
        BumpType::Minor => format!("{major}.{}.0", minor + 1),
        BumpType::Patch => format!("{major}.{minor}.{}", patch + 1),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BumpType {
    Major,
    Minor,
    Patch,
}
