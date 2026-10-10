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
# The revision is pinned to rust-sdk v3.3.0, by the commit the tag points at so
# a moved tag cannot change it: the suite asserts on Testy's exact behaviour,
# and tracking `main` lets an upstream change turn a green checkout red with no
# commit here. Bump it deliberately, in the same commit as whatever the new
# Testy needs. The same rev is the default in `dagger.dang`. Override any of the
# three environment variables to build another checkout or rev
# (`ACP_RUST_SDK_REV=main` for the moving one).
set -euo pipefail

repo="${ACP_RUST_SDK_REPO:-https://github.com/agentclientprotocol/rust-sdk.git}"
rev="${ACP_RUST_SDK_REV:-199532c7433f1aec207dd0cada299901a5621ece}" # v3.3.0

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
