//! The connection contract (`docs/architecture.md` §6.1): a factory yields an
//! outgoing frame sink, an incoming frame stream, and a diagnostic side-channel.
//!
//! Framing is the transport's job; everything above this seam deals in whole
//! JSON messages. [`ConnectionFactory::connect`] cannot fail: a transport that
//! cannot start reports it on the diagnostic channel and ends the frame stream.

use std::io;
use std::process::ExitStatus;
use std::sync::{Arc, OnceLock};
use std::time::SystemTime;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::frame::{Direction, Frame};
use crate::trace::Tap;

/// In-flight frames per direction before the slow end pushes back. Bounded so
/// a consumer that stops draining slows the agent instead of growing memory.
const FRAME_CAPACITY: usize = 256;

/// Buffered diagnostic lines, bounded for the same reason.
const DIAGNOSTIC_CAPACITY: usize = 256;

/// Makes connections. [`StdioSpawn`] is the one implementation; the seam keeps
/// a second one additive (§6.1).
///
/// [`StdioSpawn`]: crate::StdioSpawn
pub trait ConnectionFactory {
    /// Starts a connection, and returns it started.
    ///
    /// Never fails: see the module docs. Implementations spawn their pumps onto
    /// the ambient async runtime, so this is called from within one.
    fn connect(&self) -> Connection;
}

/// One agent plus the framed message pipe to it (`CONTEXT.md`, *Connection*).
pub struct Connection {
    outgoing: FrameSink,
    incoming: FrameStream,
    diagnostics: DiagnosticStream,
}

/// Frames out to the agent.
#[derive(Clone)]
pub struct FrameSink {
    frames: mpsc::Sender<Frame>,
    tap: Option<Tap>,
    _guard: Arc<Guard>,
}

/// Frames in from the agent. Ends when the agent's output does — including
/// immediately, when there was no agent to start.
pub struct FrameStream {
    frames: mpsc::Receiver<Frame>,
    tap: Option<Tap>,
    _guard: Arc<Guard>,
}

/// One frame in from the agent, and the ordinal the trace captured it under
/// (the same number [`Trace::snapshot`](crate::Trace::snapshot) reports).
///
/// `captured` is `None` only for an untapped connection, which only tests make.
pub struct Received {
    pub frame: Frame,
    pub captured: Option<u64>,
}

/// The transport's out-of-band output (`CONTEXT.md`, *Diagnostic channel*): for
/// stdio, the child's stderr, plus what the transport itself has to say about
/// starting and ending.
pub struct DiagnosticStream {
    lines: mpsc::Receiver<Diagnostic>,
    _guard: Arc<Guard>,
}

/// Something the transport observed that is not a frame, and when.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub at: SystemTime,
    pub kind: DiagnosticKind,
}

/// What the transport saw. Typed so a failed spawn can be recognized without
/// reading text, and the agent's stderr stays distinct from the inspector's
/// remarks.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum DiagnosticKind {
    /// One line the agent wrote to stderr, stamped when the line was read.
    Stderr(String),
    /// The transport never started the agent. The evidence, and the reason the
    /// frame stream is about to end.
    SpawnFailed {
        command: String,
        error: Arc<io::Error>,
    },
    /// The agent process ended, on its own or because the connection was
    /// dropped.
    ///
    /// Stdio emits this after all pipe pumps finish. Frames may still be
    /// queued: consumers must drain the incoming stream before using this
    /// diagnostic to tear down the Connection.
    AgentExited(ExitStatus),
    /// A frame past the transport's length limit was dropped and replaced by a
    /// placeholder ([`Frame::omission`](crate::Frame::omission)). The
    /// connection continues.
    FrameDropped { limit: usize, length: usize },
    /// The stderr line before this one was cut to `limit` of its `length`
    /// bytes.
    StderrLineCut { limit: usize, length: usize },
    /// The stderr line before this one was not UTF-8 and was decoded lossily.
    StderrLineLossy,
    /// The agent stopped after a graceful disconnect, and which step did it.
    Stopped {
        by: Stop,
        status: Option<ExitStatus>,
    },
    /// The agent exited but something outside its process group kept its
    /// output pipes open, so reading them was abandoned.
    DrainAbandoned,
    /// The outgoing pipe broke. Further sends fail, but incoming evidence can
    /// still arrive. This is not a terminal Connection diagnostic: consumers
    /// keep draining until AgentExited or TransportFailed.
    WriteFailed(Arc<io::Error>),
    /// A terminal transport failure (including an unreadable incoming pipe).
    /// Consumers interrupt the reader and fail pending calls immediately.
    TransportFailed(Arc<io::Error>),
}

/// Which step of a graceful stop ended the agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Stop {
    /// It had exited before it was asked.
    AlreadyExited,
    /// It exited when its stdin closed.
    StdinClosed,
    /// It exited on SIGTERM to its process group.
    Terminated,
    /// It was killed after the polite steps' grace ran out.
    Killed,
}

/// Why a frame did not go out.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SendError {
    /// The frame contains a newline, so a message-per-line transport would put
    /// two messages on the wire where the trace records one
    /// ([`Frame::is_one_message`](crate::Frame)).
    NotOneMessage,
    /// The transport is gone: the agent exited, never started, or the pipe broke.
    Disconnected,
}

/// The transport's half of a connection — what a factory writes into.
///
/// A transport's whole obligation: put whole frames in, take whole frames out,
/// report what else it saw, and stop when `shutdown` fires.
pub struct TransportEnds {
    /// Frames the client wants sent, in order.
    pub outgoing: mpsc::Receiver<Frame>,
    /// Frames arriving from the agent.
    pub incoming: mpsc::Sender<Frame>,
    /// Everything that is not a frame.
    pub diagnostics: mpsc::Sender<Diagnostic>,
    /// Fires when the client has dropped every part of the connection. A
    /// transport that owns a process ends it here; nothing else will.
    pub shutdown: CancellationToken,
    /// Fires before `shutdown` when the client asked for a graceful stop
    /// rather than an immediate one.
    pub graceful: CancellationToken,
}

/// Cancels the transport when the last of the three client-side ends is
/// dropped (counted by the `Arc`), or on an explicit RPC close.
///
/// Stdio installs a synchronous stop hook so shutdown works without a running
/// executor.
struct Guard {
    shutdown: CancellationToken,
    graceful: CancellationToken,
    stop: OnceLock<Box<dyn Fn() + Send + Sync>>,
}

/// Builds the two halves of a connection: the client's, and the transport's.
pub fn connection() -> (Connection, TransportEnds) {
    let (outgoing_tx, outgoing_rx) = mpsc::channel(FRAME_CAPACITY);
    let (incoming_tx, incoming_rx) = mpsc::channel(FRAME_CAPACITY);
    let (diagnostics_tx, diagnostics_rx) = mpsc::channel(DIAGNOSTIC_CAPACITY);

    let shutdown = CancellationToken::new();
    let graceful = CancellationToken::new();
    let guard = Arc::new(Guard {
        shutdown: shutdown.clone(),
        graceful: graceful.clone(),
        stop: OnceLock::new(),
    });

    let connection = Connection {
        outgoing: FrameSink {
            frames: outgoing_tx,
            tap: None,
            _guard: Arc::clone(&guard),
        },
        incoming: FrameStream {
            frames: incoming_rx,
            tap: None,
            _guard: Arc::clone(&guard),
        },
        diagnostics: DiagnosticStream {
            lines: diagnostics_rx,
            _guard: guard,
        },
    };

    let ends = TransportEnds {
        outgoing: outgoing_rx,
        incoming: incoming_tx,
        diagnostics: diagnostics_tx,
        shutdown,
        graceful,
    };

    (connection, ends)
}

impl Connection {
    pub(crate) fn on_shutdown(&mut self, stop: impl Fn() + Send + Sync + 'static) {
        assert!(self.outgoing._guard.stop.set(Box::new(stop)).is_ok());
    }

    pub fn outgoing(&self) -> &FrameSink {
        &self.outgoing
    }

    pub fn incoming(&mut self) -> &mut FrameStream {
        &mut self.incoming
    }

    pub fn diagnostics(&mut self) -> &mut DiagnosticStream {
        &mut self.diagnostics
    }

    /// The three ends, separately movable — a client reads frames in one task
    /// and writes them from another.
    pub fn into_parts(self) -> (FrameSink, FrameStream, DiagnosticStream) {
        (self.outgoing, self.incoming, self.diagnostics)
    }

    /// Installs the trace on both frame ends; reached only through the trace.
    pub(crate) fn tapped(mut self, tap: Tap) -> Self {
        self.outgoing.tap = Some(tap.clone());
        self.incoming.tap = Some(tap);
        self
    }
}

impl FrameSink {
    pub(crate) fn shutdown(&self) {
        self._guard.shutdown();
    }

    /// Makes the coming shutdown a graceful stop rather than an immediate one.
    pub(crate) fn stop_gracefully(&self) {
        self._guard.graceful.cancel();
    }

    pub(crate) fn stopping_gracefully(&self) -> bool {
        self._guard.graceful.is_cancelled()
    }

    /// Ends the agent now, whatever stop was asked for before.
    pub(crate) fn kill(&self) {
        if let Some(stop) = self._guard.stop.get() {
            stop();
        }
        self._guard.shutdown.cancel();
    }

    pub(crate) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self._guard, &other._guard)
    }

    /// Enqueues without waiting, and answers with where the trace put it.
    pub(crate) fn try_send(
        &self,
        frame: Frame,
    ) -> Result<Option<u64>, tokio::sync::mpsc::error::TrySendError<()>> {
        let permit = self.frames.try_reserve()?;
        Ok(match &self.tap {
            Some(tap) => {
                Some(tap.record_then(Direction::ToAgent, frame, |frame| permit.send(frame)))
            }
            None => {
                permit.send(frame);
                None
            }
        })
    }

    /// [`send`](Self::send), answering with where the trace put the frame.
    pub(crate) async fn send_captured(&self, frame: Frame) -> Result<Option<u64>, SendError> {
        if !frame.is_one_message() {
            return Err(SendError::NotOneMessage);
        }
        let permit = self
            .frames
            .reserve()
            .await
            .map_err(|_| SendError::Disconnected)?;
        Ok(match &self.tap {
            Some(tap) => {
                Some(tap.record_then(Direction::ToAgent, frame, |frame| permit.send(frame)))
            }
            None => {
                permit.send(frame);
                None
            }
        })
    }
    /// Sends one frame, recording it as it goes.
    ///
    /// Waits for room first, then records and enqueues under the trace's lock,
    /// so concurrent senders cannot record in one order and enqueue in another.
    /// "Sent" means handed to the transport.
    pub async fn send(&self, frame: Frame) -> Result<(), SendError> {
        self.send_captured(frame).await.map(|_| ())
    }
}

impl FrameStream {
    /// The next frame from the agent, or `None` once no more will come.
    ///
    /// The trace records the frame before this returns it (§6.2), and the
    /// ordinal it was captured under comes with it, so identical frames are
    /// never confused by matching on text.
    pub async fn recv(&mut self) -> Option<Received> {
        let frame = self.frames.recv().await?;
        let captured = self
            .tap
            .as_ref()
            .map(|tap| tap.record(Direction::FromAgent, &frame));
        Some(Received { frame, captured })
    }
}

impl DiagnosticStream {
    pub async fn recv(&mut self) -> Option<Diagnostic> {
        self.lines.recv().await
    }
}

impl Diagnostic {
    /// Stamps an observation now; for a pipe line, when it was read.
    pub fn now(kind: DiagnosticKind) -> Self {
        Self {
            at: SystemTime::now(),
            kind,
        }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Guard {
    fn shutdown(&self) {
        // A graceful stop is the transport's to run; killing here would end it.
        if !self.graceful.is_cancelled()
            && let Some(stop) = self.stop.get()
        {
            stop();
        }
        self.shutdown.cancel();
    }
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotOneMessage => {
                f.write_str("a frame containing a newline would cross as two messages")
            }
            Self::Disconnected => f.write_str("the connection to the agent is gone"),
        }
    }
}

impl std::error::Error for SendError {}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.kind, f)
    }
}

impl std::fmt::Display for DiagnosticKind {
    /// One console line. Stderr is the agent's text, undecorated.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stderr(line) => f.write_str(line),
            Self::SpawnFailed { command, error } => {
                write!(f, "could not start `{command}`: {error}")
            }
            Self::AgentExited(status) => write!(f, "the agent exited: {status}"),
            Self::FrameDropped { limit, length } => write!(
                f,
                "dropped a {length}-byte frame, over the {limit}-byte limit; a placeholder is in the trace"
            ),
            Self::StderrLineCut { limit, length } => write!(
                f,
                "the stderr line above was cut to its first {limit} of {length} bytes"
            ),
            Self::StderrLineLossy => f.write_str(
                "the stderr line above was not valid UTF-8; invalid bytes are shown as \u{FFFD}",
            ),
            Self::Stopped { by, status } => {
                match by {
                    Stop::AlreadyExited => f.write_str("the agent had already exited")?,
                    Stop::StdinClosed => f.write_str("the agent stopped when its stdin closed")?,
                    Stop::Terminated => f.write_str("the agent stopped on SIGTERM")?,
                    Stop::Killed => f.write_str("the agent did not stop when asked and was killed")?,
                }
                match status {
                    Some(status) => write!(f, ": {status}"),
                    None => Ok(()),
                }
            }
            Self::DrainAbandoned => f.write_str(
                "stopped reading the agent's output: it exited, but another process still holds its pipes",
            ),
            Self::TransportFailed(error) => write!(f, "the connection broke: {error}"),
            Self::WriteFailed(error) => write!(f, "could not write to the agent: {error}"),
        }
    }
}
