//! The stdio connection factory (`docs/architecture.md` §6.1): spawn the agent
//! as a direct child, frame newline-delimited JSON on its stdin/stdout, and pipe
//! its stderr into the diagnostic channel line by line.
//!
//! Two processes, no bridge, no port, no launch token — the MCP Inspector's
//! CLI/TUI path, which is the one its architecture proves works without the
//! remote layer (§4).

use std::ffi::{OsStr, OsString};
use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, oneshot};
use tokio_util::codec::{Framed, FramedParts, FramedWrite, LinesCodec, LinesCodecError};
use tokio_util::sync::CancellationToken;

use crate::connection::{
    Connection, ConnectionFactory, Diagnostic, DiagnosticKind, TransportEnds, connection,
};
use crate::frame::Frame;

/// The longest line the transport will read.
///
/// The MCP TypeScript SDK's number, by way of `app/bridge` (§4.3 there): neither
/// Zed nor the ACP SDK caps it at all, which makes an unterminated stream a slow
/// OOM. It is a reading limit and nothing more — a line over it is dropped and
/// announced, the connection carries on, and an agent that emits one has told
/// you something about itself.
const MAX_FRAME_BYTES: usize = 10 * 1024 * 1024;

/// Spawns an agent and speaks newline-delimited JSON to it.
///
/// The four fields are the MCP Inspector's, unchanged (§6.1) — command, args,
/// env, cwd — because they are what a user actually needs to launch an agent
/// (`npx`-wrapped ones included) and no more.
#[derive(Clone, Debug, Default)]
pub struct StdioSpawn {
    command: OsString,
    args: Vec<OsString>,
    env: Vec<(OsString, OsString)>,
    cwd: Option<PathBuf>,
}

impl StdioSpawn {
    pub fn new(command: impl Into<OsString>) -> Self {
        Self {
            command: command.into(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }

    #[must_use]
    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<OsString>>) -> Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Adds one variable to the child's environment.
    ///
    /// **Added to the inherited environment, not replacing it**: the credentials
    /// in the terminal the inspector was launched from are the credentials the
    /// agent runs with, which is how every practical agent login already works
    /// (§7.1 — the inspector never runs a login itself).
    #[must_use]
    pub fn env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    #[must_use]
    pub fn envs(
        mut self,
        vars: impl IntoIterator<Item = (impl Into<OsString>, impl Into<OsString>)>,
    ) -> Self {
        self.env.extend(
            vars.into_iter()
                .map(|(key, value)| (key.into(), value.into())),
        );
        self
    }

    /// The child's working directory. Distinct from ACP's per-session `cwd`,
    /// which is a parameter on the wire (§7.1) — this one is the process's.
    #[must_use]
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// The command line as typed, for the spawn-failure evidence.
    ///
    /// Arguments included: this string exists to be read by the person who
    /// wrote it, next to the error it caused, and a failure report that hides
    /// the argument that caused it is not evidence.
    pub fn command_line(&self) -> String {
        std::iter::once(self.command.as_os_str())
            .chain(self.args.iter().map(OsString::as_os_str))
            .map(OsStr::to_string_lossy)
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.command);
        command
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Piped, unlike `app/bridge`, which inherits it: the bridge has
            // nothing to show stderr *to*, while for the inspector the agent's
            // diagnostics are a screen (§9) and the spawn-failure evidence.
            .stderr(Stdio::piped())
            // A backstop for a task that unwound before teardown ran; the
            // ordinary path is the shutdown token below.
            .kill_on_drop(true);
        for (key, value) in &self.env {
            command.env(key, value);
        }
        if let Some(cwd) = &self.cwd {
            command.current_dir(cwd);
        }
        command
    }
}

impl ConnectionFactory for StdioSpawn {
    /// Spawns the agent and starts the pumps.
    ///
    /// A failed spawn is not an error return: the connection comes back with the
    /// failure already in its diagnostic channel and its frame stream ended
    /// (§6.1). Callers therefore have one shape to handle, whether the agent
    /// died before the first frame or after the thousandth.
    fn connect(&self) -> Connection {
        let (connection, ends) = connection();

        match self.command().spawn() {
            Ok(child) => pump(child, ends),
            Err(error) => {
                // `try_send` on a channel nobody has had the chance to fill: the
                // failure is in the buffer before this returns, so a caller that
                // reads its diagnostics finds it there, not eventually.
                let _ = ends
                    .diagnostics
                    .try_send(Diagnostic::now(DiagnosticKind::SpawnFailed {
                        command: self.command_line(),
                        error: Arc::new(error),
                    }));
                // Dropping `ends` closes the frame stream: nothing will arrive,
                // and the client learns that by being told rather than by
                // waiting.
            }
        }

        connection
    }
}

/// Wires a spawned child to the transport ends: one task per pipe, plus one
/// waiting on the process itself.
fn pump(mut child: Child, ends: TransportEnds) {
    let stdin = child.stdin.take().expect("stdin is piped");
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");

    let TransportEnds {
        outgoing,
        incoming,
        diagnostics,
        shutdown,
    } = ends;

    let (stderr_drained, stderr_done) = oneshot::channel();

    tokio::spawn(write_frames(stdin, outgoing, diagnostics.clone()));
    tokio::spawn(read_frames(stdout, incoming, diagnostics.clone()));
    tokio::spawn(read_stderr(stderr, diagnostics.clone(), stderr_drained));
    tokio::spawn(watch(child, diagnostics, shutdown, stderr_done));
}

/// Client frames onto the agent's stdin, one per line.
///
/// Ends when the client has dropped every sink, and closing stdin is how that
/// ending reaches the agent: an stdio agent stops at EOF, which is the first
/// step of `app/bridge`'s teardown sequence and the polite half of this one.
async fn write_frames(
    stdin: ChildStdin,
    mut outgoing: mpsc::Receiver<Frame>,
    diagnostics: mpsc::Sender<Diagnostic>,
) {
    // No cap on the encoder: what goes out was composed here, and refusing to
    // write a frame the client asked for would be the transport editing the
    // conversation.
    let mut lines = FramedWrite::new(stdin, LinesCodec::new());
    while let Some(frame) = outgoing.recv().await {
        if let Err(LinesCodecError::Io(error)) = lines.send(frame.as_str()).await {
            let _ = diagnostics.send(broken_pipe(error)).await;
            return;
        }
    }
}

/// The agent's stdout, one JSON-RPC message per line.
///
/// The codec is what makes framing the transport's job: a read carrying three
/// messages becomes three frames, and a message split across reads is
/// reassembled — both of them bugs a hand-rolled reader has to earn its way out
/// of.
async fn read_frames(
    stdout: ChildStdout,
    incoming: mpsc::Sender<Frame>,
    diagnostics: mpsc::Sender<Diagnostic>,
) {
    let mut lines = reader(stdout);
    loop {
        match lines.next().await {
            Some(Ok(line)) => {
                if incoming.send(Frame::new(line)).await.is_err() {
                    return;
                }
            }
            // One frame is lost, the connection is not, and the loss is
            // announced rather than left as a gap nobody can see: an agent that
            // emits a line this long is misbehaving, and misbehaviour is what
            // this tool is for (§7.3, §8).
            Some(Err(LinesCodecError::MaxLineLengthExceeded)) => {
                let _ = diagnostics
                    .send(Diagnostic::now(DiagnosticKind::FrameDropped {
                        limit: MAX_FRAME_BYTES,
                    }))
                    .await;
                lines = resume(lines);
            }
            Some(Err(LinesCodecError::Io(error))) => {
                let _ = diagnostics.send(broken_pipe(error)).await;
                return;
            }
            None => return,
        }
    }
}

/// The agent's stderr, line by line, stamped as each line is read
/// (`CONTEXT.md`, *Diagnostic channel*).
async fn read_stderr(
    stderr: ChildStderr,
    diagnostics: mpsc::Sender<Diagnostic>,
    drained: oneshot::Sender<()>,
) {
    let mut lines = reader(stderr);
    loop {
        let diagnostic = match lines.next().await {
            Some(Ok(line)) => Diagnostic::now(DiagnosticKind::Stderr(line)),
            Some(Err(LinesCodecError::MaxLineLengthExceeded)) => {
                lines = resume(lines);
                Diagnostic::now(DiagnosticKind::StderrLineDropped {
                    limit: MAX_FRAME_BYTES,
                })
            }
            // The wire is elsewhere: a broken stderr ends the console, not the
            // connection.
            Some(Err(LinesCodecError::Io(_))) | None => break,
        };
        if diagnostics.send(diagnostic).await.is_err() {
            break;
        }
    }
    let _ = drained.send(());
}

/// Owns the child: reports its exit, and ends it when the client lets go.
///
/// Someone has to hold the `Child`, and it may as well be the one task that has
/// something to do with it. Without this, a dropped connection would leave the
/// agent running with nobody reading it.
async fn watch(
    mut child: Child,
    diagnostics: mpsc::Sender<Diagnostic>,
    shutdown: CancellationToken,
    stderr_drained: oneshot::Receiver<()>,
) {
    tokio::select! {
        exit = child.wait() => {
            // Stderr first: an agent that explains itself and then dies should
            // have the explanation appear above the death, not after it.
            let _ = stderr_drained.await;
            if let Ok(status) = exit {
                let _ = diagnostics
                    .send(Diagnostic::now(DiagnosticKind::AgentExited(status)))
                    .await;
            }
        }
        () = shutdown.cancelled() => {
            // Nobody is left to tell, so this is teardown and not reporting.
            // `kill` reaches the child alone; a launcher's grandchildren are the
            // process-group problem `app/bridge` solved at length (§3.4 there),
            // and lifting that solution waits for an inspector that outlives its
            // connections — the desktop shell's ticket, not this one.
            let _ = child.kill().await;
        }
    }
}

/// A line reader that refuses to buffer forever.
///
/// One byte of headroom over the cap, following `app/bridge/src/frames.rs`:
/// `LinesCodec` counts a CR before stripping a CRLF terminator, so without it a
/// line exactly at the limit would be refused for its own line ending.
///
/// `Framed` rather than `FramedRead`, though nothing is ever written through it:
/// it is the one that can be taken apart and put back together, which is what
/// [`resume`] needs.
fn reader<T>(io: T) -> Framed<T, LinesCodec> {
    Framed::from_parts(FramedParts::new::<String>(
        io,
        LinesCodec::new_with_max_length(MAX_FRAME_BYTES + 1),
    ))
}

/// Reads on past an over-long line.
///
/// A framed reader fuses itself after a decode error, and the length limit is a
/// decode error — so continuing means rebuilding the reader around the same
/// buffer and the same codec, which is still mid-discard and will resume at the
/// next line ending. Rebuilding from scratch would instead drop whatever of the
/// *next* frames had already been read.
fn resume<T>(lines: Framed<T, LinesCodec>) -> Framed<T, LinesCodec> {
    Framed::from_parts(lines.into_parts())
}

/// A write or read that failed for the pipe's own reasons. Nothing more will
/// cross in that direction.
fn broken_pipe(error: io::Error) -> Diagnostic {
    Diagnostic::now(DiagnosticKind::TransportFailed(Arc::new(error)))
}
