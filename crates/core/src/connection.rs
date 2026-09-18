//! The connection contract (`docs/architecture.md` §6.1): a factory yields an
//! outgoing frame sink, an incoming frame stream, and a diagnostic side-channel.
//!
//! **Framing is the transport's job.** Everything above this seam deals in whole
//! JSON messages, which is why the WebSocket factory the spec defers is a second
//! factory rather than a redesign: WS is natively message-framed, and stdio's
//! newline framing stays stdio's business.
//!
//! **A connection is a running thing, not a result.** [`ConnectionFactory::connect`]
//! cannot fail, because a transport that cannot start is not an error to return
//! to a caller who may not be waiting for one — it is evidence, and evidence
//! goes in the diagnostic channel while the frame stream ends. That is the
//! difference between an inspector that says "no such command" and one that
//! hangs on an agent that was never there.

use std::io;
use std::process::ExitStatus;
use std::sync::Arc;
use std::time::SystemTime;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::frame::{Direction, Frame};
use crate::trace::Tap;

/// In-flight frames per direction before the slow end pushes back.
///
/// Bounded, so a UI that stops draining slows the agent down instead of growing
/// until something dies. 256 frames is roughly a chatty turn's worth — enough
/// that ordinary bursts never touch the limit.
const FRAME_CAPACITY: usize = 256;

/// Buffered diagnostic lines. A chatty agent's stderr is the usual producer, and
/// it is bounded for the same reason — the console is a view, not an archive.
const DIAGNOSTIC_CAPACITY: usize = 256;

/// Makes connections. The MVP ships one implementation, [`StdioSpawn`]; the seam
/// exists so the second one is additive (§6.1).
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

/// One frame in from the agent, and where the trace put it.
///
/// **Two things a reader pairs, so they are paired here.** The frame is what
/// crossed; the ordinal is the row it crossed as, and a Timeline entry keeps it
/// so that what this window decoded can be read against what it decoded it from
/// ([`Trace::snapshot`](crate::Trace::snapshot) is where the same number is read
/// back off the log).
///
/// `captured` is `None` for a connection nobody tapped — no trace, so no row.
/// That is a shape tests make and the inspector never does: every connection it
/// opens is recorded (§6.2).
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
///
/// Everything on this channel is timestamped, so the time is the struct's and
/// only the *what* varies — which is also what lets a console render a line
/// without asking each kind for its clock.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub at: SystemTime,
    pub kind: DiagnosticKind,
}

/// What the transport saw.
///
/// Typed rather than a stream of strings: a failed spawn is the flow the desktop
/// shell auto-surfaces (§9), so it has to be recognizable as one without reading
/// the text — and the agent's own stderr must stay distinguishable from the
/// inspector's remarks about it.
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
    AgentExited(ExitStatus),
    /// A frame past the transport's length limit was dropped — the one hole the
    /// trace can have, so it is announced rather than silently left.
    ///
    /// The connection continues: an agent that emits a 10 MB line is the
    /// misbehaviour this tool exists to show (§7.3, evidence over silence), not
    /// a reason to stop showing the rest of what it does.
    FrameDropped { limit: usize },
    /// The same, for a stderr line. The console loses one line and keeps
    /// running.
    StderrLineDropped { limit: usize },
    /// The pipe to a started agent broke.
    TransportFailed(Arc<io::Error>),
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
/// Public because writing a second transport (the deferred WebSocket factory) is
/// meant to be possible from outside this module without touching anything
/// above the seam. A transport's whole obligation is here: put whole frames in,
/// take whole frames out, say what else it saw, and stop when `shutdown` fires.
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
}

/// Cancels the transport when the last client-side part goes away.
///
/// The client end is three separately-movable values (the typed layer parks the
/// stream in one task and keeps the sink in another), so "the connection was
/// dropped" is the drop of the last of them — an `Arc` counts that, and nothing
/// else has to remember to hang up.
struct Guard(CancellationToken);

/// Builds the two halves of a connection: the client's, and the transport's.
pub fn connection() -> (Connection, TransportEnds) {
    let (outgoing_tx, outgoing_rx) = mpsc::channel(FRAME_CAPACITY);
    let (incoming_tx, incoming_rx) = mpsc::channel(FRAME_CAPACITY);
    let (diagnostics_tx, diagnostics_rx) = mpsc::channel(DIAGNOSTIC_CAPACITY);

    let shutdown = CancellationToken::new();
    let guard = Arc::new(Guard(shutdown.clone()));

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
    };

    (connection, ends)
}

impl Connection {
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

    /// Installs the trace on both frame ends. Private, and reachable only
    /// through [`Trace::decorate`], so a tapped connection is the only kind
    /// anyone can build a traced one out of.
    pub(crate) fn tapped(mut self, tap: Tap) -> Self {
        self.outgoing.tap = Some(tap.clone());
        self.incoming.tap = Some(tap);
        self
    }
}

impl FrameSink {
    pub(crate) fn try_send(
        &self,
        frame: Frame,
    ) -> Result<(), tokio::sync::mpsc::error::TrySendError<()>> {
        let permit = self.frames.try_reserve()?;
        match &self.tap {
            Some(tap) => {
                tap.record_then(Direction::ToAgent, frame, |frame| permit.send(frame));
            }
            None => permit.send(frame),
        }
        Ok(())
    }
    /// Sends one frame, recording it as it goes.
    ///
    /// The wait for room happens first, then the record and the handoff happen
    /// together: the trace holds the log while the frame goes into the pipe, so
    /// two tasks sharing a sink cannot record in one order and enqueue in the
    /// other. The trace's claim to be *the* order of the wire is only worth
    /// making if concurrent senders cannot break it.
    ///
    /// What "sent" therefore means is "handed to the transport" — the honest
    /// claim, since nothing above the seam can know the wire took it.
    pub async fn send(&self, frame: Frame) -> Result<(), SendError> {
        if !frame.is_one_message() {
            return Err(SendError::NotOneMessage);
        }
        let permit = self
            .frames
            .reserve()
            .await
            .map_err(|_| SendError::Disconnected)?;
        match &self.tap {
            // The ordinal it was recorded under is dropped here rather than
            // carried: what reads one is a Timeline entry, and nothing this
            // client sends becomes one (`timeline::Recorded`).
            Some(tap) => {
                tap.record_then(Direction::ToAgent, frame, |frame| permit.send(frame));
            }
            None => permit.send(frame),
        }
        Ok(())
    }
}

impl FrameStream {
    /// The next frame from the agent, or `None` once no more will come.
    ///
    /// The trace has the frame before this returns it, which is the whole of
    /// "below any typed layer" (§6.2): there is no way to observe a frame here
    /// that the trace has not already recorded.
    ///
    /// And it says where the trace put it, which is the same sentence read
    /// forwards: what the layer below knows about a frame, the layer above is
    /// told rather than left to work out. A typed layer that had to find its
    /// own frame in the log again would be matching on text — two identical
    /// notifications are two frames and one string — and a screen built on that
    /// would point at the wrong row exactly when an agent repeats itself.
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
    /// Stamps an observation now — which for a line read from a pipe is when it
    /// was read, the closest the client end can get to when it was written.
    pub fn now(kind: DiagnosticKind) -> Self {
        Self {
            at: SystemTime::now(),
            kind,
        }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.0.cancel();
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
    /// One console line's worth. The stderr case is the agent's own text and
    /// nothing else — decoration belongs to whatever renders it.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stderr(line) => f.write_str(line),
            Self::SpawnFailed { command, error } => {
                write!(f, "could not start `{command}`: {error}")
            }
            Self::AgentExited(status) => write!(f, "the agent exited: {status}"),
            Self::FrameDropped { limit } => {
                write!(f, "dropped a frame longer than {limit} bytes")
            }
            Self::StderrLineDropped { limit } => {
                write!(f, "dropped a stderr line longer than {limit} bytes")
            }
            Self::TransportFailed(error) => write!(f, "the connection broke: {error}"),
        }
    }
}
