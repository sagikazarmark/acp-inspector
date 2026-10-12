//! The stdio connection factory (`docs/architecture.md` §6.1): spawn the agent
//! as a direct child, frame newline-delimited JSON on its stdin/stdout, and pipe
//! its stderr into the diagnostic channel line by line.

use std::ffi::{OsStr, OsString};
use std::future::Future;
use std::io;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use futures_util::SinkExt;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{ChildStdin, Command};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::codec::{FramedWrite, LinesCodec, LinesCodecError};
use tokio_util::sync::CancellationToken;

use crate::connection::{
    Connection, ConnectionFactory, Diagnostic, DiagnosticKind, Stop, TransportEnds, connection,
};
use crate::frame::{Frame, Omission};

mod process;
use process::{Process, Spawned, WatchOwner};

/// The longest line the transport will read (the MCP TypeScript SDK's limit),
/// so an unterminated stream cannot exhaust memory. A longer line is replaced
/// by a placeholder and announced, and the connection carries on.
const MAX_FRAME_BYTES: usize = 10 * 1024 * 1024;

/// How much of a dropped frame's start the placeholder keeps, which is where
/// the id of an answer usually is.
const KEPT_PREFIX_BYTES: usize = 4 * 1024;

/// The longest stderr line kept whole, well under the console's 8 MiB budget
/// so one line cannot evict the rest of it.
const MAX_STDERR_LINE_BYTES: usize = 1024 * 1024;

/// How long each polite step of a graceful stop gets before the next.
const STOP_GRACE: Duration = Duration::from_millis(1500);

/// How long both readers may sit idle on pipes after the agent exited before
/// they are abandoned to whatever escaped its process group.
const DRAIN_GRACE: Duration = Duration::from_secs(1);

/// Spawns an agent and speaks newline-delimited JSON to it: command, args,
/// env and cwd, as in the MCP Inspector (§6.1).
#[derive(Clone, Debug, Default)]
pub struct StdioSpawn {
    command: OsString,
    args: Vec<OsString>,
    env: Vec<(OsString, OsString)>,
    removed: Vec<OsString>,
    clear_env: bool,
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

    /// Adds one variable to the child's environment, on top of the inherited
    /// one, so the agent keeps the credentials of the launching terminal.
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

    /// Keeps one inherited variable from the child. Variables set with
    /// [`env`](Self::env) are still set.
    #[must_use]
    pub fn env_remove(mut self, key: impl Into<OsString>) -> Self {
        self.removed.push(key.into());
        self
    }

    /// Starts the child with only the variables set with [`env`](Self::env).
    #[must_use]
    pub fn env_clear(mut self) -> Self {
        self.clear_env = true;
        self
    }

    /// The child process's working directory; distinct from ACP's per-session
    /// `cwd` on the wire.
    #[must_use]
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// The command line as typed, arguments included, for the spawn-failure
    /// evidence.
    pub fn command_line(&self) -> String {
        std::iter::once(self.command.as_os_str())
            .chain(self.args.iter().map(OsString::as_os_str))
            .map(OsStr::to_string_lossy)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The program to start: the command, or on Windows the file a bare name
    /// resolves to the way a shell resolves it ([`on_path`]), against the
    /// `PATH` the child will have.
    fn program(&self) -> OsString {
        #[cfg(windows)]
        {
            let set = |key: &str| {
                self.env
                    .iter()
                    .rev()
                    .find(|(name, _)| name.eq_ignore_ascii_case(key))
                    .map(|(_, value)| value.clone())
            };
            let path = set("PATH").or_else(|| std::env::var_os("PATH"));
            let pathext = set("PATHEXT").or_else(|| std::env::var_os("PATHEXT"));
            if let Some(found) = on_path(
                &self.command,
                path.as_deref(),
                pathext.as_deref(),
                std::path::Path::is_file,
            ) {
                return found.into_os_string();
            }
        }
        self.command.clone()
    }

    fn command(&self) -> Command {
        let mut command = Command::new(self.program());
        command
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Piped into the diagnostic channel.
            .stderr(Stdio::piped());
        if self.clear_env {
            command.env_clear();
        }
        for key in &self.removed {
            command.env_remove(key);
        }
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
    /// (§6.1).
    fn connect(&self) -> Connection {
        let (mut connection, ends) = connection();

        match Process::spawn(self.command()) {
            Ok(child) => {
                let owner = child.process.clone();
                connection.on_shutdown(move || owner.terminate());
                pump(child, ends);
            }
            Err(error) => {
                // The channel is empty, so `try_send` puts the failure in the
                // buffer before this returns.
                let _ = ends
                    .diagnostics
                    .try_send(Diagnostic::now(DiagnosticKind::SpawnFailed {
                        command: self.command_line(),
                        error: Arc::new(error),
                    }));
                // Dropping `ends` closes the frame stream, so the client is told
                // rather than left waiting.
            }
        }

        connection
    }
}

/// Wires a spawned child to the transport ends: one task per pipe, plus one
/// waiting on the process itself.
fn pump(child: Spawned, ends: TransportEnds) {
    let Spawned {
        process,
        stdin,
        stdout,
        stderr,
    } = child;

    let TransportEnds {
        outgoing,
        incoming,
        diagnostics,
        shutdown,
        graceful,
    } = ends;

    let stop_writing = CancellationToken::new();
    let stdout_reading = Arc::new(Reading::default());
    let stderr_reading = Arc::new(Reading::default());

    let stdin_done = tokio::spawn(guarded(
        diagnostics.clone(),
        write_frames(stdin, outgoing, diagnostics.clone(), stop_writing.clone()),
    ));
    let stdout_done = tokio::spawn(guarded(
        diagnostics.clone(),
        read_frames(
            Lines::new(stdout, MAX_FRAME_BYTES + 1, stdout_reading.clone()),
            incoming,
            diagnostics.clone(),
        ),
    ));
    let stderr_done = tokio::spawn(guarded(
        diagnostics.clone(),
        read_stderr(
            Lines::new(stderr, MAX_STDERR_LINE_BYTES, stderr_reading.clone()),
            diagnostics.clone(),
        ),
    ));
    tokio::spawn(guarded(
        diagnostics.clone(),
        watch(Watch {
            child: WatchOwner(process),
            diagnostics,
            shutdown,
            graceful,
            stop_writing,
            stdin_done,
            readers: [
                (Some(stdout_done), stdout_reading),
                (Some(stderr_done), stderr_reading),
            ],
        }),
    ));
}

/// A pump that reports its own panic as a terminal transport failure,
/// so the connection is Lost and its calls fail rather than wait on a task
/// that is gone. The panic hook, where the app installs one, logs the rest.
async fn guarded(diagnostics: mpsc::Sender<Diagnostic>, task: impl Future<Output = ()>) {
    if let Err(what) = crate::unwind::caught(task).await {
        let _ = diagnostics
            .send(Diagnostic::now(DiagnosticKind::TransportFailed(Arc::new(
                crate::unwind::internal_error(&what),
            ))))
            .await;
    }
}

/// Client frames onto the agent's stdin, one per line.
///
/// Ends when the client has dropped every sink, and closing stdin is how that
/// ending reaches the agent: an stdio agent stops at EOF, which is the first
/// step of a graceful stop.
async fn write_frames(
    stdin: ChildStdin,
    mut outgoing: mpsc::Receiver<Frame>,
    diagnostics: mpsc::Sender<Diagnostic>,
    stop: CancellationToken,
) {
    // No cap on the encoder: outgoing frames were composed here, and the
    // transport must not drop what the client asked to send.
    let mut lines = FramedWrite::new(stdin, LinesCodec::new());
    loop {
        let frame = tokio::select! {
            biased;
            () = stop.cancelled() => {
                // No new sends after exit. A queued Frame still gets a write
                // attempt so its failure is diagnosed before AgentExited.
                outgoing.close();
                outgoing.recv().await
            },
            frame = outgoing.recv() => frame,
        };
        let Some(frame) = frame else { return };
        if let Err(LinesCodecError::Io(error)) = lines.send(frame.as_str()).await {
            // Release blocked senders BEFORE waiting for diagnostic capacity.
            // The Inspector may itself be awaiting an automatic reply send.
            drop(outgoing);
            drop(lines);
            let _ = diagnostics
                .send(Diagnostic::now(DiagnosticKind::WriteFailed(Arc::new(
                    error,
                ))))
                .await;
            return;
        }
    }
}

/// The agent's stdout, one JSON-RPC message per line.
///
/// Bytes rather than text: a line that is not UTF-8 is still a frame, recorded
/// lossily and flagged, and a line over the limit leaves a placeholder.
async fn read_frames<R: AsyncRead + Unpin>(
    mut lines: Lines<R>,
    incoming: mpsc::Sender<Frame>,
    diagnostics: mpsc::Sender<Diagnostic>,
) {
    loop {
        let frame = match lines.next().await {
            Ok(Some(Line::Whole(bytes))) => Frame::read(bytes),
            Ok(Some(Line::Cut { mut kept, length })) => {
                let _ = diagnostics
                    .send(Diagnostic::now(DiagnosticKind::FrameDropped {
                        limit: MAX_FRAME_BYTES,
                        length,
                    }))
                    .await;
                kept.truncate(KEPT_PREFIX_BYTES);
                Frame::omitted(
                    &kept,
                    Omission {
                        length,
                        limit: MAX_FRAME_BYTES,
                    },
                )
            }
            Ok(None) => return,
            Err(error) => {
                let _ = diagnostics.send(broken_pipe(error)).await;
                return;
            }
        };
        if incoming.send(frame).await.is_err() {
            return;
        }
    }
}

/// The agent's stderr, line by line, stamped as each line is read.
///
/// Drains to EOF even when nobody is listening, so the agent never blocks on
/// (or dies of) a full stderr pipe.
async fn read_stderr<R: AsyncRead + Unpin>(
    mut lines: Lines<R>,
    diagnostics: mpsc::Sender<Diagnostic>,
) {
    let mut listening = true;
    // The wire is elsewhere: a broken stderr ends the console, not the
    // connection.
    while let Ok(Some(line)) = lines.next().await {
        if !listening {
            continue;
        }
        let (text, lossy, cut) = stderr_line(line);
        let mut said = vec![Diagnostic::now(DiagnosticKind::Stderr(text))];
        if lossy {
            said.push(Diagnostic::now(DiagnosticKind::StderrLineLossy));
        }
        if let Some(length) = cut {
            said.push(Diagnostic::now(DiagnosticKind::StderrLineCut {
                limit: MAX_STDERR_LINE_BYTES,
                length,
            }));
        }
        for diagnostic in said {
            if diagnostics.send(diagnostic).await.is_err() {
                listening = false;
                break;
            }
        }
    }
}

/// A stderr line as console text: CR stripped, decoded lossily, and cut on a
/// character boundary.
fn stderr_line(line: Line) -> (String, bool, Option<usize>) {
    let (mut bytes, cut) = match line {
        Line::Whole(mut bytes) => {
            if bytes.last() == Some(&b'\r') {
                bytes.pop();
            }
            (bytes, None)
        }
        Line::Cut { kept, length } => (kept, Some(length)),
    };
    if cut.is_some()
        && let Err(error) = std::str::from_utf8(&bytes)
        && error.error_len().is_none()
    {
        // The cut split a character; that is not the agent's doing.
        bytes.truncate(error.valid_up_to());
    }
    match String::from_utf8(bytes) {
        Ok(text) => (text, false, cut),
        Err(error) => (
            String::from_utf8_lossy(error.as_bytes()).into_owned(),
            true,
            cut,
        ),
    }
}

/// What the watch task owns.
struct Watch {
    child: WatchOwner,
    diagnostics: mpsc::Sender<Diagnostic>,
    shutdown: CancellationToken,
    graceful: CancellationToken,
    stop_writing: CancellationToken,
    stdin_done: JoinHandle<()>,
    readers: [(Option<JoinHandle<()>>, Arc<Reading>); 2],
}

/// Owns the child: reports its exit, and ends it when the client lets go, so a
/// dropped connection never leaves the agent running unread.
async fn watch(watch: Watch) {
    let Watch {
        child,
        diagnostics,
        shutdown,
        graceful,
        stop_writing,
        stdin_done,
        mut readers,
    } = watch;

    tokio::select! {
        () = async {
            let exit = child.0.wait().await;
            child.0.terminate();
            // Wake an idle writer too. An in-progress write finishes against
            // the closed pipe and records its failure before the exit notice.
            stop_writing.cancel();
            // Exit is not EOF: both pipes can still hold final evidence. The
            // Inspector must also consume the queued Frames before teardown.
            let abandoned = drain(&mut readers).await;
            // The writer gets the same grace. Against a closed pipe its write
            // fails at once; it blocks only when something outside the
            // process group still holds the agent's stdin without reading it,
            // and then the exit would never be reported.
            let mut stdin_done = stdin_done;
            if tokio::time::timeout(DRAIN_GRACE, &mut stdin_done).await.is_err() {
                stdin_done.abort();
                let _ = diagnostics
                    .send(Diagnostic::now(DiagnosticKind::WriteFailed(Arc::new(
                        io::Error::new(
                            io::ErrorKind::TimedOut,
                            "the agent exited mid-write and another process holds its input \
                             open without reading; the write was abandoned",
                        ),
                    ))))
                    .await;
            }
            if abandoned {
                let _ = diagnostics
                    .send(Diagnostic::now(DiagnosticKind::DrainAbandoned))
                    .await;
            }
            let diagnostic = match exit {
                Ok(status) => Diagnostic::now(DiagnosticKind::AgentExited(status)),
                Err(error) => broken_pipe(error),
            };
            let _ = diagnostics.send(diagnostic).await;
        } => {}
        () = shutdown.cancelled() => {
            // Closes stdin once whatever is queued has been written.
            stop_writing.cancel();
            if graceful.is_cancelled() {
                let (by, status) = stop(&child.0).await;
                let stopped = Diagnostic::now(DiagnosticKind::Stopped { by, status });
                let _ = tokio::time::timeout(STOP_GRACE, diagnostics.send(stopped)).await;
            } else {
                // Nobody is left to tell, so this is teardown and not reporting.
                child.0.terminate();
            }
        }
    }
}

/// Asks the agent to stop, politely first: stdin is already closing, then
/// SIGTERM to its group, then SIGKILL. Says which step ended it.
async fn stop(process: &Process) -> (Stop, Option<ExitStatus>) {
    if let Ok(Some(status)) = process.try_wait() {
        return (Stop::AlreadyExited, Some(status));
    }
    if let Ok(Ok(status)) = tokio::time::timeout(STOP_GRACE, process.wait()).await {
        return (Stop::StdinClosed, Some(status));
    }
    if process.interrupt()
        && let Ok(Ok(status)) = tokio::time::timeout(STOP_GRACE, process.wait()).await
    {
        return (Stop::Terminated, Some(status));
    }
    process.terminate();
    let status = tokio::time::timeout(STOP_GRACE, process.wait())
        .await
        .ok()
        .and_then(Result::ok);
    (Stop::Killed, status)
}

/// Waits for both readers to reach EOF after the agent exited, and says
/// whether it gave up instead.
///
/// It gives up only when every reader still running has sat idle on its pipe
/// for a whole grace period: something outside the process group holds the
/// pipe open, and EOF may never come. A reader waiting on a slow consumer is
/// making progress and is waited for.
async fn drain(readers: &mut [(Option<JoinHandle<()>>, Arc<Reading>); 2]) -> bool {
    loop {
        let before = readers
            .each_ref()
            .map(|(_, reading)| reading.reads.load(Ordering::Acquire));
        let joined = tokio::time::timeout(DRAIN_GRACE, async {
            for (task, _) in readers.iter_mut() {
                if let Some(running) = task {
                    let _ = running.await;
                    *task = None;
                }
            }
        })
        .await;
        if joined.is_ok() {
            return false;
        }
        let stalled = readers
            .iter()
            .zip(before)
            .all(|((task, reading), reads)| task.is_none() || reading.idle_since(reads));
        if stalled {
            for (task, _) in readers.iter_mut() {
                if let Some(running) = task.take() {
                    running.abort();
                }
            }
            return true;
        }
    }
}

/// Whether a reader is waiting on its pipe, and how many reads it has done.
#[derive(Default)]
struct Reading {
    waiting: AtomicBool,
    reads: AtomicU64,
}

impl Reading {
    fn idle_since(&self, reads: u64) -> bool {
        self.waiting.load(Ordering::Acquire) && self.reads.load(Ordering::Acquire) == reads
    }
}

/// One line off a pipe, `\n` excluded and anything before it — `\r` included —
/// kept verbatim.
enum Line {
    Whole(Vec<u8>),
    /// Longer than the limit: its first `limit` bytes, and its whole length.
    Cut {
        kept: Vec<u8>,
        length: usize,
    },
}

/// A line reader that refuses to buffer forever.
struct Lines<R> {
    pipe: BufReader<R>,
    limit: usize,
    reading: Arc<Reading>,
}

impl<R: AsyncRead + Unpin> Lines<R> {
    fn new(pipe: R, limit: usize, reading: Arc<Reading>) -> Self {
        Self {
            pipe: BufReader::with_capacity(64 * 1024, pipe),
            limit,
            reading,
        }
    }

    /// The next line, or `None` at EOF. A last line without a line ending is
    /// still a line.
    async fn next(&mut self) -> io::Result<Option<Line>> {
        let mut kept = Vec::new();
        let mut length = 0;
        loop {
            self.reading.waiting.store(true, Ordering::Release);
            let filled = self.pipe.fill_buf().await;
            self.reading.waiting.store(false, Ordering::Release);
            self.reading.reads.fetch_add(1, Ordering::AcqRel);
            let buffer = match filled {
                Ok(buffer) => buffer,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            };
            if buffer.is_empty() {
                return Ok((length > 0).then(|| self.line(kept, length)));
            }
            let end = buffer.iter().position(|&byte| byte == b'\n');
            let chunk = &buffer[..end.unwrap_or(buffer.len())];
            let room = self.limit.saturating_sub(kept.len());
            kept.extend(&chunk[..chunk.len().min(room)]);
            length += chunk.len();
            let consumed = end.map_or(buffer.len(), |end| end + 1);
            self.pipe.consume(consumed);
            if end.is_some() {
                return Ok(Some(self.line(kept, length)));
            }
        }
    }

    fn line(&self, mut kept: Vec<u8>, length: usize) -> Line {
        if length <= self.limit {
            Line::Whole(kept)
        } else {
            kept.truncate(self.limit);
            Line::Cut { kept, length }
        }
    }
}

/// A terminal read or process-wait failure. Write failure leaves evidence to drain.
fn broken_pipe(error: io::Error) -> Diagnostic {
    Diagnostic::now(DiagnosticKind::TransportFailed(Arc::new(error)))
}

/// The file a bare program name resolves to, as Windows' own shell finds it.
///
/// Rust's `Command` searches `PATH` for a bare name with `.exe` appended and
/// nothing else, but most agents installed with npm, `npx` among them, are
/// `.cmd` shims, and typing `npx` would fail with "not found". So a name with no
/// directory and no extension is looked for here: directory by directory along
/// `path`, each extension `pathext` lists in its order (`;`-separated, as on
/// Windows). Anything else, or a name found nowhere, is left to `Command`.
///
/// Compiled everywhere so its tests run everywhere; used only on Windows.
#[cfg_attr(not(windows), allow(dead_code))]
fn on_path(
    program: &OsStr,
    path: Option<&OsStr>,
    pathext: Option<&OsStr>,
    exists: impl Fn(&std::path::Path) -> bool,
) -> Option<PathBuf> {
    let name = program.to_string_lossy();
    if name.is_empty()
        || name.contains(['/', '\\', ':'])
        || std::path::Path::new(program).extension().is_some()
    {
        return None;
    }
    let pathext = pathext.map_or(".COM;.EXE;.BAT;.CMD".into(), OsStr::to_string_lossy);
    let path = path?.to_string_lossy();
    let directories = path
        .split(';')
        .map(|directory| directory.trim().trim_matches('"'))
        .filter(|directory| !directory.is_empty());
    for directory in directories {
        for extension in pathext.split(';').filter(|extension| !extension.is_empty()) {
            let mut file = program.to_owned();
            file.push(extension);
            let candidate = std::path::Path::new(directory).join(file);
            if exists(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_name_resolves_to_the_first_pathext_match_along_path() {
        // Windows matches file names without regard to case; this stand-in
        // does not, so the files carry PATHEXT's spelling.
        let files = ["/nodejs/npx.CMD", "/later/npx.EXE", "/tools/agent.EXE"];
        let exists =
            |path: &std::path::Path| files.iter().any(|file| path == std::path::Path::new(file));
        let path = OsStr::new("/empty;\"/nodejs\";;/later;/tools");
        let pathext = OsStr::new(".COM;.EXE;.BAT;.CMD");
        let find = |program: &str| on_path(OsStr::new(program), Some(path), Some(pathext), exists);

        // The shim in the earlier directory, as cmd.exe would run it.
        assert_eq!(find("npx"), Some(PathBuf::from("/nodejs/npx.CMD")));
        assert_eq!(find("agent"), Some(PathBuf::from("/tools/agent.EXE")));
        // Left to `Command`: found nowhere, an extension given, or a path.
        assert_eq!(find("missing"), None);
        assert_eq!(find("npx.cmd"), None);
        assert_eq!(find("./npx"), None);
        assert_eq!(find("C:\\nodejs\\npx"), None);
        assert_eq!(find(""), None);
        // No PATHEXT: the extensions Windows has by default.
        assert_eq!(
            on_path(OsStr::new("npx"), Some(path), None, exists),
            Some(PathBuf::from("/nodejs/npx.CMD"))
        );
        // No PATH: nothing to search.
        assert_eq!(
            on_path(OsStr::new("npx"), None, Some(pathext), exists),
            None
        );
    }

    #[tokio::test]
    async fn a_pump_that_panics_reports_a_terminal_transport_failure() {
        let (sender, mut said) = mpsc::channel(1);
        guarded(sender, async { panic!("stdout pump") }).await;
        let Some(Diagnostic {
            kind: DiagnosticKind::TransportFailed(error),
            ..
        }) = said.recv().await
        else {
            panic!("a transport failure")
        };
        assert_eq!(
            error.to_string(),
            "internal error: stdout pump (see crash log)"
        );

        // A pump that ends normally says nothing.
        let (sender, mut said) = mpsc::channel(1);
        guarded(sender, async {}).await;
        assert!(said.recv().await.is_none());
    }
}
