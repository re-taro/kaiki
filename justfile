#!/usr/bin/env -S just --justfile

set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]
set shell := ["bash", "-cu"]

_default:
  @just --list -u

alias r := ready

init:
  cargo binstall watchexec-cli cargo-insta typos-cli cargo-shear dprint -y

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
  wasm-pack build wasm/kaiki_diff_wasm --target nodejs

wasm-build-web:
  wasm-pack build wasm/kaiki_diff_wasm --target web

wasm-test:
  wasm-pack test --node wasm/kaiki_diff_wasm

[unix]
doc:
  RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --document-private-items

[windows]
doc:
  $Env:RUSTDOCFLAGS='-D warnings'; cargo doc --no-deps --document-private-items
