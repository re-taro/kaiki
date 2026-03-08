# kaiki - Visual Regression Testing Tool (Rust)

## Architecture

Cargo workspace with 10 crates. Re-implementation of reg-suit (Node.js) in Rust.

## Crate Layout

- `crates/kaiki_diff` - Pixelmatch pixel comparison (wasm-compatible)
- `crates/kaiki_config` - regconfig.json parsing
- `crates/kaiki_report` - HTML/JSON report generation
- `crates/kaiki_git` - Git commit key generation (gix)
- `crates/kaiki_storage` - S3/GCS storage backends
- `crates/kaiki_notify` - GitHub/Slack notifications
- `crates/kaiki_core` - Orchestration layer
- `apps/kaiki_cli` - CLI binary
- `napi/kaiki` - Node.js napi-rs bindings
- `wasm/kaiki_diff_wasm` - WASM bindings

## Key Dependencies

image, gix, aws-sdk-s3, reqwest, rayon, tokio, clap, serde, wasm-bindgen, napi

## Build Notes

- napi crate must be excluded from --all-targets builds
- Edition 2024 (resolver = 3)
- Workspace lints propagated via [lints] workspace = true
