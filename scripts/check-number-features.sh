#!/usr/bin/env bash
# Exercise the typed seam under each caller-selected serde_json feature set.
# Core reads RawValue fragments, so these exercise four distinct feature sets.
set -euo pipefail
for features in '' serde_json/arbitrary_precision serde_json/float_roundtrip serde_json/arbitrary_precision,serde_json/float_roundtrip; do
    args=()
    if [[ -n "$features" ]]; then args=(--features "$features"); fi
    cargo test --locked -p acp-inspector-core --lib decode::tests "${args[@]}"
    cargo test --locked -p acp-inspector-core --test numbers "${args[@]}"
done
