//! The core's dependency boundary, verified against the resolved graph rather
//! than promised in prose — the host repo's `no_dioxus.rs` habit
//! (`crates/dioxus-agent-ui-core/tests/no_dioxus.rs`, ADR 0006), applied to the
//! two rules `docs/architecture.md` §3 and §5 set for this crate:
//!
//! - **zero UI dependencies**, which is what keeps a web surface additive
//!   instead of a rewrite, and what makes "if it needs a window to test, it
//!   belongs in core" a boundary rather than a preference;
//! - **no `dioxus-agent-ui*`**: patterns are copied from the host repo, never
//!   linked (§13), because the library is still waiting for a genuinely
//!   independent second consumer and the inspector is not it;
//! - **no full ACP SDK**: the schema crate is types, while the SDK's client
//!   machinery sits above exactly the seam the trace decorator owns.

use std::collections::{HashSet, VecDeque};
use std::process::Command;

use serde_json::Value;

#[test]
fn the_core_depends_on_no_ui_crate_and_no_acp_client() {
    let names = dependency_tree();

    let ui: Vec<_> = names
        .iter()
        .filter(|name| *name == "dioxus" || name.starts_with("dioxus-"))
        .collect();
    assert!(
        ui.is_empty(),
        "acp-inspector-core must have no UI dependency, found: {ui:?}"
    );

    assert!(
        !names.iter().any(|name| name == "agent-client-protocol"),
        "the inspector speaks ACP below the SDK's client layer, not through it"
    );

    // Guard the guard: an empty walk would pass every assertion above.
    assert!(
        names.contains(&"tokio".to_owned()),
        "dependency walk looks broken; saw: {names:?}"
    );
}

/// Every crate this one resolves to, all dependency kinds and features — which
/// includes dev-dependencies, so it is the widest form of the question.
fn dependency_tree() -> Vec<String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let output = Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--manifest-path",
            concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"),
        ])
        .output()
        .expect("cargo metadata runs");
    assert!(output.status.success(), "cargo metadata failed");
    let metadata: Value = serde_json::from_slice(&output.stdout).expect("valid metadata JSON");

    let root = metadata["resolve"]["root"]
        .as_str()
        .expect("resolve graph has a root")
        .to_owned();
    let nodes = metadata["resolve"]["nodes"].as_array().unwrap();
    let node = |id: &str| nodes.iter().find(|node| node["id"] == id).unwrap();

    let packages = metadata["packages"].as_array().unwrap();
    let package = |id: &str| packages.iter().find(|package| package["id"] == id).unwrap();

    let mut seen = HashSet::from([root.clone()]);
    let mut queue = VecDeque::from([root.clone()]);
    let mut names = Vec::new();
    while let Some(id) = queue.pop_front() {
        if id != root {
            names.push(package(&id)["name"].as_str().unwrap().to_owned());
        }
        for dependency in node(&id)["dependencies"].as_array().unwrap() {
            let dependency = dependency.as_str().unwrap().to_owned();
            if seen.insert(dependency.clone()) {
                queue.push_back(dependency);
            }
        }
    }
    names
}
