//! `inspect`: what an exported trace holds, counted.
//!
//! A reader of the pinned format (`docs/trace-export.md`) and nothing more: the
//! header, then one record per line. Frames are counted by direction and by
//! what they were — the method a request or notification named, or the method
//! of the request an answer answers — so a CI log can say "the agent sent 40
//! `session/update`s and answered `initialize` with an error" without anybody
//! opening the file. Unknown fields are ignored and unknown record types are
//! counted, as the format's versioning rules ask of a consumer.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

use serde_json::{Value, json};

use crate::args::Inspect;

const FORMAT: &str = "acp-inspector-trace";
const VERSION: u64 = 1;
const TO_AGENT: &str = "client -> agent";
const FROM_AGENT: &str = "agent -> client";

/// What a trace holds.
#[derive(Default)]
struct Counts {
    /// The header's own counts.
    header_frames: u64,
    header_dropped: u64,
    /// Frame records actually in the file.
    frames: u64,
    /// `dropped` placeholder records: frames the transport refused for length.
    dropped_records: u64,
    /// Frames carried as base64 because they were not UTF-8.
    lossy: u64,
    /// Records of a type this reader does not know.
    unknown_records: u64,
    connections: BTreeMap<u64, u64>,
    first: Option<i64>,
    last: Option<i64>,
    /// Direction → what the frame was → how many.
    methods: BTreeMap<String, BTreeMap<String, u64>>,
}

/// Runs `inspect`, printing to stdout, and answers with the exit code.
pub fn inspect(inspect: &Inspect) -> u8 {
    let counts = match read(&inspect.path) {
        Ok(counts) => counts,
        Err(error) => {
            eprintln!(
                "acp-inspector-cli: cannot read {}: {error}",
                inspect.path.display()
            );
            return 1;
        }
    };
    let truncated = counts.frames != counts.header_frames;
    if inspect.json {
        println!("{}", as_json(&counts));
    } else {
        print!("{}", human(&inspect.path, &counts));
    }
    if truncated {
        eprintln!(
            "acp-inspector-cli: the header promises {} frames and the file carries {}",
            counts.header_frames, counts.frames
        );
        return 1;
    }
    0
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn read(path: &Path) -> io::Result<Counts> {
    let mut lines = BufReader::new(File::open(path)?).lines();
    let header = lines.next().ok_or_else(|| invalid("the file is empty"))??;
    let header: Value = serde_json::from_str(&header)
        .map_err(|error| invalid(format!("the header is not JSON: {error}")))?;
    if header["type"] != "header" || header["format"] != FORMAT {
        return Err(invalid(format!("not an {FORMAT} export")));
    }
    match header["version"].as_u64() {
        Some(VERSION) => {}
        other => {
            return Err(invalid(format!(
                "format version {} is not one this reader knows ({VERSION})",
                other.map_or_else(|| "?".to_owned(), |v| v.to_string())
            )));
        }
    }

    let mut counts = Counts {
        header_frames: header["frames"].as_u64().unwrap_or(0),
        header_dropped: header["dropped"].as_u64().unwrap_or(0),
        ..Counts::default()
    };
    // (connection, direction the request went, id) → the request's method.
    let mut requests: HashMap<(u64, String, String), String> = HashMap::new();

    for (index, line) in lines.enumerate() {
        let line = line?;
        if line.is_empty() {
            continue;
        }
        let record: Value = serde_json::from_str(&line)
            .map_err(|error| invalid(format!("record {} is not JSON: {error}", index + 1)))?;
        if let Some(at) = record["at"].as_i64() {
            counts.first = Some(counts.first.map_or(at, |first| first.min(at)));
            counts.last = Some(counts.last.map_or(at, |last| last.max(at)));
        }
        let direction = record["direction"].as_str().unwrap_or("?").to_owned();
        match record["type"].as_str() {
            Some("frame") => {
                counts.frames += 1;
                let connection = record["connection"].as_u64().unwrap_or(0);
                *counts.connections.entry(connection).or_default() += 1;
                let label = if record["encoding"] == "utf8" {
                    label(
                        record["frame"].as_str().unwrap_or_default(),
                        connection,
                        &direction,
                        &mut requests,
                    )
                } else {
                    counts.lossy += 1;
                    "(not UTF-8)".to_owned()
                };
                *counts
                    .methods
                    .entry(direction)
                    .or_default()
                    .entry(label)
                    .or_default() += 1;
            }
            Some("dropped") => {
                counts.dropped_records += 1;
                *counts
                    .methods
                    .entry(direction)
                    .or_default()
                    .entry("(dropped: over the length limit)".to_owned())
                    .or_default() += 1;
            }
            _ => counts.unknown_records += 1,
        }
    }
    Ok(counts)
}

/// What one frame was: the method it named, or what it answered.
fn label(
    frame: &str,
    connection: u64,
    direction: &str,
    requests: &mut HashMap<(u64, String, String), String>,
) -> String {
    let Ok(message) = serde_json::from_str::<Value>(frame) else {
        return "(not JSON)".to_owned();
    };
    let id = message.get("id").map(Value::to_string);
    if let Some(method) = message.get("method").and_then(Value::as_str) {
        if let Some(id) = id {
            requests.insert((connection, direction.to_owned(), id), method.to_owned());
        }
        return method.to_owned();
    }
    let answered = if message.get("error").is_some() {
        "error"
    } else if message.get("result").is_some() {
        "result"
    } else {
        return "(not a JSON-RPC message)".to_owned();
    };
    let asked = match direction {
        TO_AGENT => FROM_AGENT,
        _ => TO_AGENT,
    };
    match id.and_then(|id| requests.remove(&(connection, asked.to_owned(), id))) {
        Some(method) => format!("{method} → {answered}"),
        None => format!("(unpaired {answered})"),
    }
}

fn span_seconds(counts: &Counts) -> Option<f64> {
    Some((counts.last? - counts.first?) as f64 / 1000.0)
}

fn human(path: &Path, counts: &Counts) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "trace        {}", path.display());
    let _ = writeln!(
        out,
        "frames       {} (header: {}), {} dropped (header), {} dropped-frame records, {} not UTF-8",
        counts.frames,
        counts.header_frames,
        counts.header_dropped,
        counts.dropped_records,
        counts.lossy
    );
    let _ = writeln!(out, "connections  {}", counts.connections.len());
    if let Some(span) = span_seconds(counts) {
        let _ = writeln!(out, "span         {span:.3} s");
    }
    if counts.unknown_records > 0 {
        let _ = writeln!(
            out,
            "unknown      {} records of a type this reader does not know",
            counts.unknown_records
        );
    }
    for (direction, methods) in &counts.methods {
        let _ = writeln!(out, "{direction}");
        for (method, count) in methods {
            let _ = writeln!(out, "  {count:>6}  {method}");
        }
    }
    out
}

fn as_json(counts: &Counts) -> Value {
    json!({
        "format": "acp-inspector-cli-inspect",
        "version": 1,
        "frames": counts.frames,
        "headerFrames": counts.header_frames,
        "dropped": counts.header_dropped,
        "droppedRecords": counts.dropped_records,
        "lossy": counts.lossy,
        "unknownRecords": counts.unknown_records,
        "connections": counts.connections.len(),
        "spanSeconds": span_seconds(counts),
        "directions": counts.methods,
    })
}
