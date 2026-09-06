#!/usr/bin/env bash
# Builds Testy — the ACP rust-sdk's deterministic stdio test agent — from a
# rust-sdk checkout (docs/architecture.md §12).
#
# `agent-client-protocol-test` is `publish = false`, so there is no crates.io
# release to depend on: the only way to get the binary is to build it from the
# SDK repository. This script is that build, and it is the one place the
# checkout, the revision and the output path are decided — `just testy` and the
# CI step both call it, so a green CI run and a local `cargo test` are running
# the same agent.
#
# The revision defaults to `main`: the spec accepts rust-sdk `main` as a
# dev-time moving part rather than pinning a rev that would silently rot.
# Override any of the three environment variables to test against a fixed rev.
set -euo pipefail

repo="${ACP_RUST_SDK_REPO:-https://github.com/agentclientprotocol/rust-sdk.git}"
rev="${ACP_RUST_SDK_REV:-main}"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
checkout="${ACP_RUST_SDK_DIR:-$root/.testy/rust-sdk}"
bin_dir="$root/.testy/bin"

if [ ! -d "$checkout/.git" ]; then
    # Blobless: the SDK's history is not the subject, the current tree is.
    git clone --filter=blob:none "$repo" "$checkout"
fi

git -C "$checkout" fetch --filter=blob:none origin "$rev"
# Detached, because this checkout is a build input and never a place to work.
git -C "$checkout" checkout --detach FETCH_HEAD

cargo build --manifest-path "$checkout/Cargo.toml" -p agent-client-protocol-test --bin testy

mkdir -p "$bin_dir"
cp "$checkout/target/debug/testy" "$bin_dir/testy"

echo "testy built from $(git -C "$checkout" rev-parse --short HEAD) -> $bin_dir/testy"
