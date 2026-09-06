#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

mkdir "$scratch/checkout" "$scratch/blocked"

# Copy the working versions of tracked files only. node_modules, Testy, build
# output, runtime assets and any other untracked contributor state cannot enter
# this checkout.
git -C "$repository_root" ls-files --full-name -z \
  | tar -C "$repository_root" --null -T - -cf - \
  | tar -xf - -C "$scratch/checkout"

git -C "$repository_root" ls-files --error-unmatch \
  crates/app/style/sheet.css >/dev/null

for command in node nodejs npm npx yarn pnpm bun deno dx; do
  printf '#!/usr/bin/env bash\nprintf "unexpected %s invocation\\n" "$0" >&2\nexit 1\n' \
    >"$scratch/blocked/$command"
  chmod +x "$scratch/blocked/$command"
done

export PATH="$scratch/blocked:$PATH"
export CARGO_NET_OFFLINE=true
export CARGO_TARGET_DIR="$repository_root/target/node-free"

cargo build --locked --offline -p acp-inspector \
  --manifest-path "$scratch/checkout/Cargo.toml"
cargo test --locked --offline -p acp-inspector \
  --manifest-path "$scratch/checkout/Cargo.toml"
