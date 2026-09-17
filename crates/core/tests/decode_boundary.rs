//! A source-level tripwire for §8, like `headless.rs` for the dependency graph.
//! All production deserialization is accounted for: protocol types have one
//! home, and the few non-protocol readers are named explicitly below.

use std::fs;
use std::path::Path;

#[test]
fn protocol_types_are_decoded_only_at_the_canonicalizing_seam() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut readers = Vec::new();
    for source in ["crates/core/src", "crates/app/src"] {
        collect(&root.join(source), &root, &mut readers);
    }
    readers.sort();
    // Production readers read JSON values, stored preferences or syntax. The
    // component's test-only schema fixture is the sole protocol exception.
    // Exact lines rather than whole-file exemptions: a new reader in rpc.rs
    // is just as capable of bypassing the seam as one in a new module.
    let mut allowed: Vec<_> = [
        (
            "crates/app/src/elicitation.rs",
            "Surface::Raw => match serde_json::from_str::<Value>(&self.written) {",
        ),
        (
            "crates/app/src/elicitation.rs",
            "serde_json::from_str::<Value>(written)",
        ),
        (
            "crates/core/src/frame.rs",
            "serde_json::from_str::<serde_json::Value>(&self.0)",
        ),
        (
            "crates/core/src/indentation.rs",
            "serde_json::from_str::<serde::de::IgnoredAny>(text).ok()?;",
        ),
        (
            "crates/core/src/recent.rs",
            "let file: Stored = serde_json::from_str(&document).ok()?;",
        ),
        (
            "crates/core/src/settings.rs",
            "let file: Stored = serde_json::from_str(&document).ok()?;",
        ),
        (
            "crates/app/src/elicitation.rs",
            "serde_json::from_value(json).expect(\"a schema the crate can read\")",
        ),
        (
            "crates/app/src/style.rs",
            "u8::from_str_radix(&hex[at..at + 2], 16).expect(\"a hex pair\") as f64 / 255.0;",
        ),
    ]
    .into_iter()
    .map(|(path, line)| (path.to_owned(), line.to_owned()))
    .collect();
    allowed.sort();
    assert_eq!(
        readers, allowed,
        "new deserialization must go through decode.rs; only non-protocol readers belong in this list"
    );
    assert!(
        root.join("crates/core/src/decode.rs").is_file(),
        "the seam exists"
    );
}

fn collect(directory: &Path, root: &Path, readers: &mut Vec<(String, String)>) {
    for entry in fs::read_dir(directory).expect("source directory is readable") {
        let path = entry.expect("source entry is readable").path();
        if path.is_dir() {
            collect(&path, root, readers);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let relative = path.strip_prefix(root).unwrap().to_str().unwrap();
            if relative == "crates/core/src/decode.rs" {
                continue;
            }
            let source = fs::read_to_string(&path).expect("Rust source is UTF-8");
            // Scan test-gated items too: production items can follow them.
            // The one component fixture decode is explicitly accounted for.
            for line in source.lines().map(str::trim) {
                if line.starts_with("//") {
                    continue;
                }
                // Include imports/aliases and direct Deserialize calls, not only
                // fully-qualified serde_json calls. False positives demand a
                // look at the boundary rather than silently widening it.
                if [
                    "from_value",
                    "from_str",
                    "from_slice",
                    "from_reader",
                    "Deserializer",
                    "deserialize(",
                    "deserialize::",
                ]
                .iter()
                .any(|needle| line.contains(needle))
                    && !line.contains("crate::decode::from_value")
                {
                    readers.push((relative.to_owned(), line.to_owned()));
                }
            }
        }
    }
}
