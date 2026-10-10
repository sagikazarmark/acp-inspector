//! Reading an export back in (F5): a JSONL trace this inspector wrote, opened
//! again as a trace and — by replaying its frames through the typed layer — as
//! a timeline.
//!
//! **The file is the contract** (`docs/trace-export.md`), so this reads exactly
//! what that document pins and refuses what it does not: a header first, of
//! this format and a version it knows; then `frame`, `dropped` and `stderr`
//! records, with records of a type it does not know skipped and counted.
//!
//! **Nothing here is live.** The trace that comes back has no connection
//! behind it, and the replay drives a client whose pipe goes nowhere: what the
//! typed layer would answer on its own is written into a channel nobody reads,
//! and a request the file shows being answered is resolved with the answer the
//! file shows. What the agent was asked by the inspector's own calls is read off
//! the frames rather than re-asked — a prompt opens a turn and its answer closes
//! it — so no frame in the rebuilt timeline is one the file does not hold.

use std::collections::HashMap;
use std::fmt;
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use agent_client_protocol_schema::v1;
use futures_util::FutureExt;
use serde_json::Value;

use crate::call::CallError;
use crate::client::{Client, Stores};
use crate::connection::{Diagnostic, DiagnosticKind, Received};
use crate::frame::{Direction, Frame, FrameKind, Omission};
use crate::timeline::Timeline;
use crate::trace::{Trace, TracedFrame};
use crate::turn::{TurnOutcome, TurnState};

/// What the format calls itself, which a file must say before anything else.
const FORMAT: &str = "acp-inspector-trace";

/// The one schema version this reads.
const VERSION: u64 = 1;

/// A trace read back out of an export, and what the typed layer made of it.
pub struct Imported {
    trace: Trace,
    timeline: Timeline,
    stderr: Vec<Diagnostic>,
    agent: Option<v1::InitializeResponse>,
    taken: SystemTime,
    skipped: usize,
    warnings: Vec<String>,
}

/// Why a file could not be opened as a trace.
#[derive(Debug)]
#[non_exhaustive]
pub enum ImportError {
    /// The file could not be read.
    Io(Arc<io::Error>),
    /// The first line is not this format's header.
    NotATrace(String),
    /// The header names a version this does not read.
    Version(u64),
    /// A record that is not what the format says it is, and on which line.
    Record { line: usize, problem: String },
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "could not read the file: {error}"),
            Self::NotATrace(problem) => write!(f, "not an ACP Inspector trace: {problem}"),
            Self::Version(version) => write!(
                f,
                "the trace is format version {version}; this inspector reads version {VERSION}"
            ),
            Self::Record { line, problem } => write!(f, "line {line}: {problem}"),
        }
    }
}

impl std::error::Error for ImportError {}

impl Imported {
    /// Reads and replays the export at `path`.
    ///
    /// Blocking: it reads the whole file and rebuilds everything from it, so a
    /// window calls it off its UI thread.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ImportError> {
        let bytes = std::fs::read(path).map_err(|error| ImportError::Io(Arc::new(error)))?;
        let text = String::from_utf8(bytes).map_err(|_| ImportError::Record {
            line: 0,
            problem: "the file is not UTF-8, and a trace export always is".to_owned(),
        })?;
        Self::parse(&text)
    }

    /// Reads and replays an export's text.
    pub fn parse(text: &str) -> Result<Self, ImportError> {
        let mut lines = text
            .split('\n')
            .enumerate()
            .map(|(index, line)| (index + 1, line.strip_suffix('\r').unwrap_or(line)))
            .filter(|(_, line)| !line.trim().is_empty());

        let (_, first) = lines
            .next()
            .ok_or_else(|| ImportError::NotATrace("the file is empty".to_owned()))?;
        let header: Value = serde_json::from_str(first).map_err(|error| {
            ImportError::NotATrace(format!("the first line is not JSON: {error}"))
        })?;
        if header.get("type").and_then(Value::as_str) != Some("header")
            || header.get("format").and_then(Value::as_str) != Some(FORMAT)
        {
            return Err(ImportError::NotATrace(format!(
                "the first line is not an `{FORMAT}` header"
            )));
        }
        let version = header.get("version").and_then(Value::as_u64).unwrap_or(0);
        if version != VERSION {
            return Err(ImportError::Version(version));
        }
        let taken = header
            .get("at")
            .and_then(Value::as_i64)
            .map_or(UNIX_EPOCH, moment);
        let claimed_frames = header.get("frames").and_then(Value::as_u64);
        let claimed_dropped = header.get("dropped").and_then(Value::as_u64).unwrap_or(0) as usize;

        let mut frames = Vec::new();
        let mut stderr = Vec::new();
        let mut skipped = 0;
        let mut placeholders = 0;
        for (number, line) in lines {
            let bad = |problem: String| ImportError::Record {
                line: number,
                problem,
            };
            let record: Value =
                serde_json::from_str(line).map_err(|error| bad(format!("not JSON: {error}")))?;
            let field = |name: &str| {
                record
                    .get(name)
                    .ok_or_else(|| bad(format!("the record has no `{name}`")))
            };
            let number_of = |name: &str| {
                field(name)?
                    .as_u64()
                    .ok_or_else(|| bad(format!("`{name}` is not a whole number")))
            };
            let at = || {
                field("at")?
                    .as_i64()
                    .map(moment)
                    .ok_or_else(|| bad("`at` is not a whole number".to_owned()))
            };
            let direction = || match field("direction")?.as_str() {
                Some("client -> agent") => Ok(Direction::ToAgent),
                Some("agent -> client") => Ok(Direction::FromAgent),
                _ => Err(bad(
                    "`direction` is neither of the two the format names".to_owned()
                )),
            };
            match record.get("type").and_then(Value::as_str) {
                Some("frame") => {
                    let text = field("frame")?
                        .as_str()
                        .ok_or_else(|| bad("`frame` is not a string".to_owned()))?;
                    let hand = record.get("origin").and_then(Value::as_str) == Some("hand");
                    let bytes = match record.get("encoding").and_then(Value::as_str) {
                        None | Some("utf8") => text.as_bytes().to_vec(),
                        Some("base64") => unbase64(text)
                            .ok_or_else(|| bad("`frame` is not valid base64".to_owned()))?,
                        Some(other) => return Err(bad(format!("unknown encoding `{other}`"))),
                    };
                    frames.push(TracedFrame {
                        at: at()?,
                        connection: number_of("connection")?,
                        direction: direction()?,
                        frame: Frame::rebuilt(bytes, hand),
                    });
                }
                Some("dropped") => {
                    placeholders += 1;
                    frames.push(TracedFrame {
                        at: at()?,
                        connection: number_of("connection")?,
                        direction: direction()?,
                        frame: Frame::placeholder(Omission {
                            length: number_of("length")? as usize,
                            limit: number_of("limit")? as usize,
                        }),
                    });
                }
                Some("stderr") => {
                    let said = field("line")?
                        .as_str()
                        .ok_or_else(|| bad("`line` is not a string".to_owned()))?;
                    stderr.push(Diagnostic {
                        at: at()?,
                        kind: DiagnosticKind::Stderr(said.to_owned()),
                    });
                }
                Some("header") => return Err(bad("a second header".to_owned())),
                Some(_) => skipped += 1,
                None => return Err(bad("the record has no `type`".to_owned())),
            }
        }

        let mut warnings = Vec::new();
        let carried = frames.len() - placeholders;
        if claimed_frames.is_some_and(|claimed| claimed != carried as u64) {
            warnings.push(format!(
                "The header says {} frames and the file carries {carried}: it may have been cut short or edited.",
                claimed_frames.unwrap_or_default()
            ));
        }
        if claimed_dropped < placeholders {
            warnings.push(format!(
                "The header counts {claimed_dropped} dropped frames and the file has {placeholders} placeholders."
            ));
        }
        if skipped > 0 {
            warnings.push(format!(
                "{skipped} records of a type this inspector does not know were skipped."
            ));
        }

        let dropped = claimed_dropped.saturating_sub(placeholders);
        let (timeline, agent) = replay(&frames, dropped);
        Ok(Self {
            trace: Trace::rebuilt(frames, dropped),
            timeline,
            stderr,
            agent,
            taken,
            skipped,
            warnings,
        })
    }

    /// The frames, numbered as they were when they were captured.
    pub fn trace(&self) -> &Trace {
        &self.trace
    }

    /// What the typed layer made of the frames, replayed.
    pub fn timeline(&self) -> &Timeline {
        &self.timeline
    }

    /// The agent's stderr lines, where the export carried them.
    pub fn stderr(&self) -> &[Diagnostic] {
        &self.stderr
    }

    /// What the agent said about itself, where the file holds its answer to
    /// `initialize`.
    pub fn agent(&self) -> Option<&v1::InitializeResponse> {
        self.agent.as_ref()
    }

    /// When the export was taken.
    pub fn taken(&self) -> SystemTime {
        self.taken
    }

    /// How many records were of a type this does not read.
    pub fn skipped(&self) -> usize {
        self.skipped
    }

    /// What about the file did not add up, in sentences.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }
}

/// Replays the frames through a client whose pipe goes nowhere, and answers
/// with the timeline it wrote and the agent's `initialize` answer.
///
/// Every send the client makes completes at once (the channel is drained after
/// each frame), so each step is polled once rather than run on an executor.
fn replay(frames: &[TracedFrame], dropped: usize) -> (Timeline, Option<v1::InitializeResponse>) {
    let (connection, mut ends) = crate::connection::connection();
    let (sink, _incoming, _diagnostics) = connection.into_parts();
    let stores = Stores::default();
    let client = Client::new(sink, stores.clone());
    // The calls the inspector made, by connection and id, and what each was.
    let mut asked: HashMap<(u64, String), String> = HashMap::new();
    let mut agent = None;

    for (position, traced) in frames.iter().enumerate() {
        let ordinal = (dropped + position) as u64;
        let summary = traced.frame.summary();
        let answer = matches!(summary.kind, FrameKind::Result | FrameKind::Error)
            && summary.method.is_none();
        let key = summary.id.clone().map(|id| (traced.connection, id));
        match traced.direction {
            Direction::ToAgent => match summary.kind {
                FrameKind::Call => {
                    if let (Some(key), Some(method)) = (key, summary.method.clone()) {
                        if method == v1::AGENT_METHOD_NAMES.session_prompt {
                            stores.turn.set(TurnState::InFlight);
                            stores.timeline.open_turn();
                        }
                        asked.insert(key, method);
                    }
                }
                // The inspector's answer to something the agent asked: the
                // request it answers is resolved with what the file says.
                _ if answer => {
                    let _ = resolve(&stores, &traced.frame).now_or_never();
                }
                _ => {}
            },
            Direction::FromAgent => {
                let ours = if answer {
                    key.and_then(|key| asked.remove(&key))
                } else {
                    None
                };
                match ours {
                    Some(method) => {
                        let outcome = outcome(&traced.frame);
                        if method == v1::AGENT_METHOD_NAMES.session_prompt {
                            let ended = match outcome {
                                Ok(result) => {
                                    match crate::decode::from_value::<v1::PromptResponse>(result) {
                                        Ok(response) => TurnOutcome::Ended(response.stop_reason),
                                        Err(error) => TurnOutcome::Failed(CallError::Undecodable(
                                            error.to_string(),
                                        )),
                                    }
                                }
                                Err(error) => TurnOutcome::Failed(CallError::Rejected(error)),
                            };
                            stores.timeline.close_turn(ended);
                        } else if method == v1::AGENT_METHOD_NAMES.initialize
                            && let Ok(result) = outcome
                        {
                            agent = crate::decode::from_value(result).ok();
                        }
                    }
                    None => {
                        let received = Received {
                            frame: traced.frame.clone(),
                            captured: Some(ordinal),
                        };
                        let _ = client.receive(received).now_or_never();
                    }
                }
            }
        }
        while ends.outgoing.try_recv().is_ok() {}
    }

    // Read-only: whatever the file leaves unanswered can no longer be answered.
    stores.permissions.abandon();
    stores.elicitations.abandon();
    (stores.timeline.clone(), agent)
}

/// The result or the error an answer carries.
fn outcome(frame: &Frame) -> Result<Value, v1::Error> {
    let message = crate::decode::frame_value(frame.as_str()).unwrap_or(Value::Null);
    match message.get("error") {
        Some(error) => Err(crate::decode::from_value::<v1::Error>(error.clone())
            .unwrap_or_else(|_| v1::Error::internal_error())),
        None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
    }
}

/// Resolves the permission request or elicitation an inspector's answer was
/// for, the way the answer resolved it.
async fn resolve(stores: &Stores, frame: &Frame) {
    let Ok(message) = crate::decode::frame_value(frame.as_str()) else {
        return;
    };
    let Some(id) = message.get("id") else {
        return;
    };
    let result = message.get("result").cloned();
    if let Some(request) = stores
        .permissions
        .waiting()
        .into_iter()
        .find(|request| request.id().value() == id)
    {
        let answered = result.and_then(|result| {
            crate::decode::from_value::<v1::RequestPermissionResponse>(result).ok()
        });
        let _ = match answered.map(|response| response.outcome) {
            Some(v1::RequestPermissionOutcome::Selected(selected)) => {
                request.select(&selected.option_id).await
            }
            Some(v1::RequestPermissionOutcome::Cancelled) => request.cancelled().await,
            _ => Ok(request.abandoned()),
        };
        return;
    }
    if let Some(request) = stores
        .elicitations
        .waiting()
        .into_iter()
        .find(|request| request.id().value() == id)
    {
        let action = result
            .as_ref()
            .and_then(|result| result.get("action"))
            .and_then(Value::as_str);
        let _ = match action {
            Some("accept") => {
                let content = result
                    .as_ref()
                    .and_then(|result| result.get("content"))
                    .and_then(Value::as_object)
                    .cloned();
                request.accept(content).await
            }
            Some("decline") => request.decline().await,
            Some("cancel") => request.cancel().await,
            _ => Ok(request.abandoned()),
        };
    }
}

/// Milliseconds since the Unix epoch, as the file has them, as a moment.
fn moment(millis: i64) -> SystemTime {
    let span = Duration::from_millis(millis.unsigned_abs());
    if millis >= 0 {
        UNIX_EPOCH + span
    } else {
        UNIX_EPOCH - span
    }
}

/// Standard base64 with padding (RFC 4648 §4), the export's other encoding.
fn unbase64(text: &str) -> Option<Vec<u8>> {
    fn value(byte: u8) -> Option<u32> {
        Some(match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    }
    let bytes = text.trim().as_bytes();
    if bytes.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let padding = chunk.iter().rev().take_while(|&&byte| byte == b'=').count();
        if padding > 2 {
            return None;
        }
        let mut n = 0u32;
        for (i, &byte) in chunk.iter().enumerate() {
            let digit = if i >= 4 - padding { 0 } else { value(byte)? };
            n |= digit << (18 - 6 * i);
        }
        let decoded = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&decoded[..3 - padding]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_reads_back_what_the_export_writes() {
        for (text, bytes) in [
            ("", &b""[..]),
            ("Zg==", b"f"),
            ("Zm8=", b"fo"),
            ("Zm9v", b"foo"),
            ("/w==", &[0xff][..]),
        ] {
            assert_eq!(unbase64(text).as_deref(), Some(bytes), "{text}");
        }
        assert_eq!(unbase64("Zm9"), None);
        assert_eq!(unbase64("Zm9*"), None);
    }

    #[test]
    fn a_file_that_is_not_a_trace_says_so() {
        assert!(matches!(
            Imported::parse(""),
            Err(ImportError::NotATrace(_))
        ));
        assert!(matches!(
            Imported::parse(r#"{"type":"header","format":"something-else","version":1}"#),
            Err(ImportError::NotATrace(_))
        ));
        assert!(matches!(
            Imported::parse(r#"{"type":"header","format":"acp-inspector-trace","version":2}"#),
            Err(ImportError::Version(2))
        ));
        let broken = "{\"type\":\"header\",\"format\":\"acp-inspector-trace\",\"version\":1,\"frames\":1,\"dropped\":0}\n{\"type\":\"frame\",\"at\":1}";
        assert!(matches!(
            Imported::parse(broken),
            Err(ImportError::Record { line: 2, .. })
        ));
    }
}
