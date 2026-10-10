//! The CLI's dependency boundary, verified against the resolved graph the way
//! core's is (`crates/core/tests/headless.rs`): it is a surface over core with
//! no renderer under it, so it runs on a CI machine with no display, no
//! WebView and no Dioxus — and an argument parser is not a reason to take a
//! dependency either.

use std::collections::{HashSet, VecDeque};
use std::process::Command;

use serde_json::Value;

#[test]
fn the_cli_depends_on_core_and_on_no_ui_crate() {
    let names = dependency_tree();

    let ui: Vec<_> = names
        .iter()
        .filter(|name| *name == "dioxus" || name.starts_with("dioxus-"))
        .collect();
    assert!(
        ui.is_empty(),
        "acp-inspector-cli must have no UI dependency, found: {ui:?}"
    );
    assert!(
        !names.iter().any(|name| name == "acp-inspector"),
        "the CLI is a surface over core, not over the app crate"
    );
    assert!(
        !names.iter().any(|name| name == "agent-client-protocol"),
        "the inspector speaks ACP below the SDK's client layer, not through it"
    );
    assert!(
        !names.iter().any(|name| name == "clap"),
        "the command line is read by hand (`src/args.rs`)"
    );

    // Guard the guard: an empty walk would pass every assertion above.
    assert!(
        names.contains(&"acp-inspector-core".to_owned()) && names.contains(&"tokio".to_owned()),
        "dependency walk looks broken; saw: {names:?}"
    );
}

/// Every crate this one resolves to, all dependency kinds and features.
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

    let packages = metadata["packages"].as_array().unwrap();
    let package = |id: &str| packages.iter().find(|package| package["id"] == id).unwrap();
    // In a workspace the resolve root is null, so start from this package.
    let root = packages
        .iter()
        .find(|package| package["name"] == env!("CARGO_PKG_NAME"))
        .expect("this package is in its own metadata")["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let nodes = metadata["resolve"]["nodes"].as_array().unwrap();
    let node = |id: &str| nodes.iter().find(|node| node["id"] == id).unwrap();

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
