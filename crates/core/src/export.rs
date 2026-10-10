//! The JSONL trace export (`docs/architecture.md` §10).
//!
//! The record schema is a versioned, self-describing contract pinned in
//! `docs/trace-export.md`; change it only there first. Nothing is redacted, and
//! the header says so.

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::connection::{Diagnostic, DiagnosticKind};
use crate::frame::Direction;
use crate::trace::TracedFrame;

/// What the format calls itself, in every header it writes.
const FORMAT: &str = "acp-inspector-trace";

/// The schema version. Bumped only when an existing reader would misread the
/// file, not for added fields.
const VERSION: u32 = 1;

/// The header's warning: the bridge's `--trace-frames` wording, verbatim
/// (`docs/trace-export.md`).
const WARNING: &str =
    "ACP frames may contain prompts, file contents, and tool output; nothing is redacted";

/// What an export is called on disk, after the moment it was taken.
const EXTENSION: &str = "jsonl";

/// How many names one export may try before giving up on a directory.
const NAMES: u32 = 64;

/// A snapshot of a trace: the frames it held at one moment and the count it
/// had dropped. Later frames or clears do not change an export already taken.
pub struct Export {
    at: SystemTime,
    frames: Vec<TracedFrame>,
    dropped: usize,
    /// The agent's stderr lines, when asked for ([`with_stderr`](Self::with_stderr)).
    stderr: Option<Vec<(SystemTime, String)>>,
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
    /// How many stderr records follow, when the export carries any at all.
    #[serde(skip_serializing_if = "Option::is_none")]
    stderr: Option<usize>,
    warning: &'a str,
}

/// One frame, as the file has it.
///
/// `connection`, `direction`, `encoding` and `frame` match the bridge's
/// `--trace-frames` records so one reader handles both; `type` and `at` are
/// added. Non-UTF-8 frames are carried as `encoding: "base64"`.
#[derive(Serialize)]
struct Record<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    at: i64,
    connection: u64,
    direction: &'static str,
    encoding: &'static str,
    frame: std::borrow::Cow<'a, str>,
    /// `"hand"` for a frame a person sent from the raw request composer;
    /// absent for everything the inspector composed itself or read.
    #[serde(skip_serializing_if = "Option::is_none")]
    origin: Option<&'static str>,
}

/// One line of the agent's stderr, carried only when the export was asked to
/// carry them: not a frame, so its own record type.
#[derive(Serialize)]
struct Stderr<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    at: i64,
    line: &'a str,
}

/// Where the transport dropped a frame too long to carry: not a frame, so its
/// own record type, counted in the header's `dropped`.
#[derive(Serialize)]
struct Dropped {
    #[serde(rename = "type")]
    kind: &'static str,
    at: i64,
    connection: u64,
    direction: &'static str,
    /// The dropped line's length in bytes.
    length: usize,
    /// The transport's limit it was over.
    limit: usize,
}

impl Export {
    pub(crate) fn new(at: SystemTime, frames: Vec<TracedFrame>, dropped: usize) -> Self {
        Self {
            at,
            frames,
            dropped,
            stderr: None,
        }
    }

    /// The same export, carrying the agent's stderr lines from `diagnostics`
    /// as `stderr` records interleaved with the frames by when each was read.
    /// Other diagnostics (the transport's own remarks) are left out.
    #[must_use]
    pub fn with_stderr(mut self, diagnostics: &[Diagnostic]) -> Self {
        self.stderr = Some(
            diagnostics
                .iter()
                .filter_map(|diagnostic| match &diagnostic.kind {
                    DiagnosticKind::Stderr(line) => Some((diagnostic.at, line.clone())),
                    _ => None,
                })
                .collect(),
        );
        self
    }

    /// Writes the export to exactly `path`, replacing what is there (a save
    /// dialog already confirmed that).
    ///
    /// Written to a sibling `.partial` file and renamed into place, so a failed
    /// write leaves `path` untouched and nothing half-written. Created `0600` on
    /// Unix.
    pub fn save_to(&self, path: impl AsRef<Path>) -> io::Result<PathBuf> {
        let path = path.as_ref();
        let name = path.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "an export needs a file name")
        })?;
        let mut partial = name.to_os_string();
        partial.push(".partial");
        let partial = path.with_file_name(partial);
        let _ = std::fs::remove_file(&partial);
        let written = create(&partial)
            .and_then(|file| self.write(file))
            .and_then(|()| std::fs::rename(&partial, path));
        match written {
            Ok(()) => Ok(path.to_path_buf()),
            Err(error) => {
                let _ = std::fs::remove_file(&partial);
                Err(error)
            }
        }
    }

    /// When the export was taken.
    pub fn at(&self) -> SystemTime {
        self.at
    }

    /// How many frames it carries.
    pub fn frames(&self) -> usize {
        self.frames.len() - self.placeholders()
    }

    /// How many frames crossed that this export does not carry: cleared, aged
    /// out of the trace's cap, or dropped by the transport for their length.
    /// Also written in the header.
    pub fn dropped(&self) -> usize {
        self.dropped + self.placeholders()
    }

    fn placeholders(&self) -> usize {
        self.frames
            .iter()
            .filter(|traced| traced.frame.omission().is_some())
            .count()
    }

    /// Writes the export into `directory` under a fresh name, and returns the
    /// path written.
    ///
    /// Never overwrites: a taken name gets a `-N` suffix (up to 64 tries). The
    /// file is `0600` on Unix because nothing in it is redacted. A failed write
    /// removes the file, since a truncated export would still parse.
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

    /// Writes the whole document into an open file and flushes it.
    fn write(&self, mut file: File) -> io::Result<()> {
        write!(file, "{self}")?;
        file.flush()
    }

    /// Writes the export into the system temp directory (always writable, and
    /// never the agent's project), and returns the path written.
    pub fn save(&self) -> io::Result<PathBuf> {
        self.save_in(std::env::temp_dir())
    }

    /// The default file name, stamped with when the export was taken so a
    /// directory of them sorts chronologically.
    pub fn filename(&self) -> String {
        format!("{}.{EXTENSION}", self.stem())
    }

    /// The name without its extension, which a taken name is suffixed from.
    fn stem(&self) -> String {
        format!("acp-trace-{}", stamp(self.at))
    }
}

/// Creates a new file (`0600` on Unix); an existing name is an error, never an
/// overwrite.
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
    /// The whole document: the header line, then one record per line.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let header = Header {
            kind: "header",
            format: FORMAT,
            version: VERSION,
            at: stamp(self.at),
            frames: self.frames(),
            dropped: self.dropped(),
            stderr: self.stderr.as_ref().map(Vec::len),
            warning: WARNING,
        };
        writeln!(f, "{}", line(&header)?)?;

        // Stderr lines go in where they were read: before the first frame read
        // after them. The frames keep the trace's order, which is authoritative.
        let mut lines = self.stderr.as_deref().unwrap_or_default().iter().peekable();
        let mut stderr_until = |f: &mut fmt::Formatter<'_>, until: Option<SystemTime>| {
            while let Some((at, text)) =
                lines.next_if(|(at, _)| until.is_none_or(|until| *at < until))
            {
                let record = Stderr {
                    kind: "stderr",
                    at: stamp(*at),
                    line: text,
                };
                writeln!(f, "{}", line(&record)?)?;
            }
            Ok::<(), fmt::Error>(())
        };

        for frame in &self.frames {
            stderr_until(f, Some(frame.at))?;
            if let Some(omission) = frame.frame.omission() {
                let record = Dropped {
                    kind: "dropped",
                    at: stamp(frame.at),
                    connection: frame.connection,
                    direction: way(frame.direction),
                    length: omission.length,
                    limit: omission.limit,
                };
                writeln!(f, "{}", line(&record)?)?;
                continue;
            }
            let (encoding, text) = if frame.frame.is_lossy() {
                ("base64", base64(frame.frame.bytes()).into())
            } else {
                ("utf8", frame.frame.as_str().into())
            };
            let record = Record {
                kind: "frame",
                at: stamp(frame.at),
                connection: frame.connection,
                direction: way(frame.direction),
                encoding,
                frame: text,
                origin: frame.frame.is_hand_sent().then_some("hand"),
            };
            writeln!(f, "{}", line(&record)?)?;
        }
        stderr_until(f, None)?;

        Ok(())
    }
}

/// Standard base64 with padding (RFC 4648 §4).
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &byte)| n | u32::from(byte) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// One record, as compact JSON on one line. Cannot fail for these plain
/// record types; the error path is `serde_json`'s.
fn line(record: &impl Serialize) -> Result<String, fmt::Error> {
    serde_json::to_string(record).map_err(|_| fmt::Error)
}

/// How a direction reads in the file: the bridge's strings, unchanged.
fn way(direction: Direction) -> &'static str {
    match direction {
        Direction::ToAgent => "client -> agent",
        Direction::FromAgent => "agent -> client",
    }
}

/// A moment, as milliseconds since the Unix epoch (a number, so tools need no
/// timezone or date parser).
fn stamp(at: SystemTime) -> i64 {
    match at.duration_since(UNIX_EPOCH) {
        // Saturates rather than panics on an absurd clock, keeping the sign.
        Ok(since) => i64::try_from(since.as_millis()).unwrap_or(i64::MAX),
        // A clock before 1970 exports as a negative stamp, not a lost frame.
        Err(before) => {
            i64::try_from(before.duration().as_millis()).map_or(i64::MIN, |millis| -millis)
        }
    }
}
