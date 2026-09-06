//! The JSONL trace export (`docs/architecture.md` §10): the trace as a file
//! somebody else can read.
//!
//! **The record schema is a contract** (§15 q2, pinned in
//! `docs/trace-export.md`). The export is the seed of the event log ACP Wiretap
//! and replay tooling are waiting on, which means its shape stops being this
//! crate's business the moment anything consumes it — so it is written down,
//! versioned, and self-describing rather than grown.
//!
//! Nothing is redacted, and the header says so. An ACP trace carries prompts,
//! file contents and tool output; an export that quietly dropped some of it
//! would be evidence you cannot trust, which is worse than evidence you have to
//! handle carefully.

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::frame::Direction;
use crate::trace::TracedFrame;

/// What the format calls itself, in every header it writes.
const FORMAT: &str = "acp-inspector-trace";

/// The schema version. It changes when a consumer that reads version 1 would
/// read a version 2 file wrongly — never for a field that was added beside the
/// ones already pinned.
const VERSION: u32 = 1;

/// What the header warns whoever opens the file.
///
/// The bridge's word for word (`app/bridge/src/shell/trace.rs`): the two files
/// carry the same risk, so they say the same thing about it.
const WARNING: &str =
    "ACP frames may contain prompts, file contents, and tool output; nothing is redacted";

/// What an export is called on disk, after the moment it was taken.
const EXTENSION: &str = "jsonl";

/// How many names one export may try before giving up on a directory.
const NAMES: u32 = 64;

/// A trace, taken as evidence: the frames it held at one moment, and what it
/// says about itself.
///
/// **A snapshot, not a view.** Taking one is the whole of "export": the frames
/// are copied out from under the trace's lock, so what is written is what had
/// crossed by the time the button was pressed — the trace going on, or being
/// cleared, changes nothing about an export already taken (§10, and
/// `docs/trace-export.md` on what clear means).
pub struct Export {
    at: SystemTime,
    frames: Vec<TracedFrame>,
    dropped: usize,
}

/// The first line of every export: what the file is, and what is in it.
#[derive(Serialize)]
struct Header<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    format: &'static str,
    version: u32,
    at: i64,
    frames: usize,
    dropped: usize,
    warning: &'a str,
}

/// One frame, as the file has it.
///
/// Aligned with the bridge's `--trace-frames` record field for field where the
/// fields mean the same thing — `connection`, `direction` (down to the strings),
/// `encoding`, `frame` — so one reader handles both captures, and extended with
/// the two the inspector knows and the bridge did not record: which kind of
/// record this is, and when the frame crossed.
#[derive(Serialize)]
struct Record<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    at: i64,
    connection: u64,
    direction: &'static str,
    encoding: &'static str,
    frame: &'a str,
}

impl Export {
    pub(crate) fn new(at: SystemTime, frames: Vec<TracedFrame>, dropped: usize) -> Self {
        Self {
            at,
            frames,
            dropped,
        }
    }

    /// When the export was taken.
    pub fn at(&self) -> SystemTime {
        self.at
    }

    /// How many frames it carries.
    pub fn frames(&self) -> usize {
        self.frames.len()
    }

    /// How many frames the trace captured that this export does not carry —
    /// cleared, or aged out of the trace's cap.
    ///
    /// In the header as well, because a consumer reading the file has to be
    /// able to tell an export of a whole session from an export of what was
    /// left of one.
    pub fn dropped(&self) -> usize {
        self.dropped
    }

    /// Writes the export into `directory`, under a name of its own, and answers
    /// with the file it wrote.
    ///
    /// **It never writes over anything.** The file is created rather than
    /// opened, and a name already taken — two exports inside one millisecond,
    /// which is one impatient click — is answered with the next name along, not
    /// with the earlier evidence gone.
    ///
    /// On Unix the file is the user's own to read (`0600`), because nothing in
    /// it is redacted: an ACP trace carries prompts, file contents and tool
    /// output, and a world-readable copy of it in a shared temp directory is not
    /// what anybody asked for by pressing Export.
    ///
    /// **A write that fails leaves nothing behind.** A half-written export is
    /// the one outcome worse than none: its header would claim frames the file
    /// does not have, and it would parse — evidence that lies quietly, which is
    /// what this format exists not to be. So the file goes when the writing
    /// does, and the error is the whole answer.
    pub fn save_in(&self, directory: impl AsRef<Path>) -> io::Result<PathBuf> {
        let directory = directory.as_ref();
        let stem = self.stem();

        for attempt in 1..=NAMES {
            let path = directory.join(match attempt {
                1 => format!("{stem}.{EXTENSION}"),
                nth => format!("{stem}-{nth}.{EXTENSION}"),
            });
            match create(&path) {
                Ok(file) => {
                    return self.write(file).map(|()| path.clone()).inspect_err(|_| {
                        let _ = std::fs::remove_file(&path);
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }

        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{NAMES} exports named `{stem}` are already there"),
        ))
    }

    /// Writes the whole document into an open file, and makes sure it is really
    /// there before saying so.
    fn write(&self, mut file: File) -> io::Result<()> {
        write!(file, "{self}")?;
        file.flush()
    }

    /// Writes the export where an export goes when nobody says: the system's
    /// temp directory.
    ///
    /// **Somewhere always writable, and never the user's project.** A tool that
    /// scattered `.jsonl` files through the working directory of whatever the
    /// agent was pointed at would be leaving litter in someone's repository; a
    /// tool that wrote to the home directory would be doing it more politely.
    /// The path comes back so that whoever pressed the button can be told
    /// exactly where the evidence is — which is the whole of what this owes
    /// them until there is a save dialog to ask with.
    pub fn save(&self) -> io::Result<PathBuf> {
        self.save_in(std::env::temp_dir())
    }

    /// What the file is called: when it was taken, and what it is.
    ///
    /// The timestamp is the export's, so a directory of them sorts into the
    /// order they were taken — and two traces from one session never collide by
    /// being from one session.
    pub fn filename(&self) -> String {
        format!("{}.{EXTENSION}", self.stem())
    }

    /// The name without its extension, which is what a name already taken is
    /// extended from.
    fn stem(&self) -> String {
        format!("acp-trace-{}", stamp(self.at))
    }
}

/// Creates the file, and no other: an existing name is an error rather than an
/// overwrite, which is what makes a second export a second file.
fn create(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

impl fmt::Display for Export {
    /// The whole document: the header, then one record per frame, each a whole
    /// line — which is what JSONL is, and what lets a consumer read a huge trace
    /// a record at a time.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let header = Header {
            kind: "header",
            format: FORMAT,
            version: VERSION,
            at: stamp(self.at),
            frames: self.frames.len(),
            dropped: self.dropped,
            warning: WARNING,
        };
        writeln!(f, "{}", line(&header)?)?;

        for frame in &self.frames {
            let record = Record {
                kind: "frame",
                at: stamp(frame.at),
                connection: frame.connection,
                direction: way(frame.direction),
                // Always, and pinned anyway: a `Frame` is text by construction,
                // so the inspector has nothing to base64. The field is here so
                // that a reader written for the bridge's records — which can
                // carry either — reads these without a special case.
                encoding: "utf8",
                frame: frame.frame.as_str(),
            };
            writeln!(f, "{}", line(&record)?)?;
        }

        Ok(())
    }
}

/// One record, as compact JSON on one line.
///
/// Serializing cannot fail for either record type: both are plain fields, and
/// the frame is a string whatever it contains. The error path exists because
/// `serde_json` has one, not because there is a way to reach it.
fn line(record: &impl Serialize) -> Result<String, fmt::Error> {
    serde_json::to_string(record).map_err(|_| fmt::Error)
}

/// How a direction reads in the file.
///
/// The bridge's strings, unchanged: the inspector *is* the client, so its two
/// directions and the bridge's are the same two facts about the same two ends,
/// and spelling them differently would cost a consumer a translation table for
/// nothing.
fn way(direction: Direction) -> &'static str {
    match direction {
        Direction::ToAgent => "client -> agent",
        Direction::FromAgent => "agent -> client",
    }
}

/// A moment, as milliseconds since the Unix epoch.
///
/// A number rather than a formatted date, because this file is read by tools:
/// no timezone to agree on, no parser to write, and the ordering is the
/// comparison. Rendering it for a human is presentation, and belongs where the
/// reader's own clock is known.
fn stamp(at: SystemTime) -> i64 {
    match at.duration_since(UNIX_EPOCH) {
        // The saturations are unreachable on any clock: `i64` milliseconds run
        // to the year 292 million. They are here because a wrong clock must not
        // be able to panic a stamp — and they saturate the way the clock was
        // wrong, so a nonsense timestamp at least keeps its sign.
        Ok(since) => i64::try_from(since.as_millis()).unwrap_or(i64::MAX),
        // A clock set before 1970 is a machine with a wrong clock, not a reason
        // to lose a frame: it exports as the negative it is.
        Err(before) => {
            i64::try_from(before.duration().as_millis()).map_or(i64::MIN, |millis| -millis)
        }
    }
}
