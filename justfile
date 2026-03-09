#!/usr/bin/env -S just --justfile

set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]
set shell := ["bash", "-cu"]

_default:
  @just --list -u

alias r := ready

init:
  cargo binstall watchexec-cli cargo-insta typos-cli cargo-shear dprint wasm-bindgen-cli -y

ready:
  git diff --exit-code --quiet
  typos
  just fmt
  just check
  just test
  just lint
  just doc

watch *args='':
  watchexec --no-vcs-ignore {{args}}

download-fixtures:
  cargo run -p xtask -- download-fixtures

fmt:
  cargo shear --fix
  cargo fmt --all
  dprint fmt

check:
  just wasm-build-web
  cargo check --workspace --all-features --all-targets --exclude kaiki_napi --locked
  cargo check -p kaiki_napi --locked

watch-check:
  just watch "'cargo check; cargo clippy'"

test:
  cargo test --workspace --exclude kaiki_napi

bench:
  cargo bench -p kaiki_diff

lint:
  just wasm-build-web
  cargo clippy --workspace --all-targets --all-features --exclude kaiki_napi -- --deny warnings
  cargo clippy -p kaiki_napi -- --deny warnings

wasm-build:
  cargo build --target wasm32-unknown-unknown --release -p kaiki_diff_wasm
  wasm-bindgen --target nodejs --out-dir wasm/kaiki_diff_wasm/pkg target/wasm32-unknown-unknown/release/kaiki_diff_wasm.wasm

wasm-build-web:
  cargo build --target wasm32-unknown-unknown --release -p kaiki_diff_wasm
  wasm-bindgen --target web --out-dir wasm/kaiki_diff_wasm/pkg target/wasm32-unknown-unknown/release/kaiki_diff_wasm.wasm

wasm-test:
  cargo test --target wasm32-unknown-unknown -p kaiki_diff_wasm

napi-test:
  cd napi/kaiki && pnpm run build:debug && pnpm test

napi-bench:
  cd napi/kaiki && pnpm run build:debug && pnpm run bench

[unix]
doc:
  RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --document-private-items

[windows]
doc:
  $Env:RUSTDOCFLAGS='-D warnings'; cargo doc --no-deps --document-private-items
