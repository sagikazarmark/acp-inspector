//! The one task reading a connection: frames into the typed client,
//! diagnostics into the log, and the status and teardown they decide.

use std::sync::{Arc, MutexGuard};

use tokio_util::sync::CancellationToken;

use crate::client::Client;
use crate::connection::{Diagnostic, DiagnosticKind, DiagnosticStream, FrameSink, FrameStream};
use crate::store::locked;
use crate::store::{Changes, Log};

use super::{ConnectionStatus, Live, State};

/// Everything the transport said that was not a frame, oldest first
/// (`CONTEXT.md`, *Diagnostic channel*).
///
/// The Console's Diagnostics tab is a rendering of this, and so is the evidence
/// behind a failed spawn: one channel, so an agent's own diagnostics and the
/// inspector's remarks about the connection sit in one timeline instead of two.
#[derive(Clone)]
pub struct DiagnosticLog {
    entries: Log<Diagnostic>,
}

impl Default for DiagnosticLog {
    fn default() -> Self {
        Self {
            entries: Log::budgeted(Self::CAPACITY, Self::BYTE_CAPACITY, |line| {
                std::mem::size_of::<Diagnostic>()
                    + match &line.kind {
                        DiagnosticKind::Stderr(text) => text.len(),
                        DiagnosticKind::SpawnFailed { command, error } => {
                            command.len() + error.to_string().len()
                        }
                        DiagnosticKind::TransportFailed(error)
                        | DiagnosticKind::WriteFailed(error) => error.to_string().len(),
                        _ => 0,
                    }
            }),
        }
    }
}

impl DiagnosticLog {
    pub const CAPACITY: usize = 10_000;
    pub const BYTE_CAPACITY: usize = 8 * 1024 * 1024;
    pub fn entries(&self) -> Vec<Diagnostic> {
        self.entries.entries()
    }

    /// The lines it holds and the count of the ones it does not, from one
    /// moment — [`Trace::snapshot`](crate::Trace::snapshot)'s reason, for the
    /// other log a window renders.
    ///
    /// The second number counts entries aged out under either budget. A line's
    /// identity remains `dropped + position`, including transport remarks that
    /// reported holes in stdout or stderr and have themselves aged out.
    pub fn snapshot(&self) -> (Vec<Diagnostic>, usize) {
        self.entries.snapshot()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Watches the log: how many lines there are now, and a wait for the next.
    pub fn changes(&self) -> Changes<usize> {
        self.entries.changes()
    }
}

impl State {
    /// Records one diagnostic, and lets it move the status.
    ///
    /// **The connection's whole lifecycle is on this channel** (§6.1): a start
    /// that failed, an agent that exited, a pipe that broke. Reading the status
    /// off the diagnostics rather than off "the streams went quiet" is what
    /// keeps the two agreeing — an agent that closes its stdout and keeps
    /// running has not disconnected, and the transport says so by not saying so.
    ///
    /// The log first, then the status, then the teardown, always: the shell
    /// opens the console *on the captured output* (§9), so the output has to be
    /// there by the time the status tells it to look — and the teardown is what
    /// wakes everyone waiting on an answer, so the status has to be there by
    /// the time they ask why.
    pub(super) fn observe(&self, diagnostic: Diagnostic, shutdown: &CancellationToken) {
        let mut connection = self.connection();
        // A reader can wake after reconnect. Its evidence and teardown belong
        // only to the Connection whose token it holds.
        if shutdown.is_cancelled() {
            return;
        }
        let status = match diagnostic.kind {
            DiagnosticKind::SpawnFailed { .. } => Some(ConnectionStatus::FailedToStart),
            // The agent is gone either way; which way is what the console says.
            DiagnosticKind::AgentExited(_) | DiagnosticKind::TransportFailed(_) => {
                Some(ConnectionStatus::Lost)
            }
            // Stderr, record holes and a failed outgoing pipe do not end the
            // incoming evidence. Only a terminal diagnostic releases it.
            _ => None,
        };

        self.diagnostics.entries.append(diagnostic);
        if let Some(status) = status {
            // Nothing is on the other end any more, so nothing here should be
            // holding a connection to it: "connected" and "holding a
            // connection" are the same fact, and this is where they stay one.
            self.release_locked(&mut connection, status, false);
        }
    }

    /// Lets go of the connection, if there is one, and says so — in the status,
    /// and to everyone waiting on an answer. A connection that is already gone
    /// leaves nothing to do and nothing to say over what said it went.
    ///
    /// Both ends of the teardown: the reader lets go of the stream and the
    /// channel, the [`Live`] lets go of the client that holds the sink, and the
    /// last of them going away is what tells the transport to end the agent
    /// (§6.1).
    ///
    /// **The status moves under the connection's own lock**, before anything is
    /// woken. That is what keeps the two agreeing from any angle: a caller told
    /// its call will not be answered can read a status that already says why,
    /// and a `disconnect` racing an agent's last breath either finds the
    /// connection gone — leaving the news standing, because it has to survive a
    /// click — or gets there first and this reports nothing over it.
    ///
    /// Everything still waiting on an answer is then told there will not be
    /// one. A call left pending across a teardown is the hang this design is
    /// against, and a turn is the one that would be visible: it would sit on
    /// the screen looking like it was still running.
    ///
    /// `graceful` asks the transport to stop the agent politely rather than at
    /// once ([`Inspector::disconnect_gracefully`]).
    pub(super) fn release(&self, status: ConnectionStatus, graceful: bool) {
        self.release_locked(&mut self.connection(), status, graceful);
    }

    pub(super) fn release_locked(
        &self,
        live: &mut Option<Live>,
        status: ConnectionStatus,
        graceful: bool,
    ) {
        if let Some(connection) = live.take() {
            self.status.set(status);
            if graceful {
                let sink = connection.client.sink();
                sink.stop_gracefully();
                locked(&self.stopping).push(sink.clone());
            }
            connection.client.disconnected();
            connection.shutdown.cancel();
        }
    }

    /// Poll only while this reader owns the stores. The lock is held for each
    /// synchronous poll, never over an await: reconnect cannot reset the stores
    /// halfway through an old Frame being recorded or decoded.
    pub(super) async fn while_connected<F: std::future::Future>(
        &self,
        shutdown: &CancellationToken,
        future: F,
    ) -> Option<F::Output> {
        tokio::pin!(future);
        std::future::poll_fn(|cx| {
            let _connection = self.connection();
            if shutdown.is_cancelled() {
                return std::task::Poll::Ready(None);
            }
            future.as_mut().poll(cx).map(Some)
        })
        .await
    }

    pub(super) fn connection(&self) -> MutexGuard<'_, Option<Live>> {
        self.connection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Drains a connection into the stores until the inspector lets go of it.
///
/// One task for both ends rather than one each: a diagnostic is what moves the
/// status, and a frame is what the trace and the timeline are made of, so
/// keeping them in one place is what makes "the log has the evidence before the
/// status points at it" a fact about this function rather than a hope about
/// two.
///
/// **This is the only reader**, which is what makes the stores trustworthy: a
/// frame reaches the timeline, settles a turn and answers a call from here and
/// nowhere else, in the order the wire had them. Whoever awaits a driven method
/// is told after the stores already know.
///
/// It outlives the agent on purpose. An exited agent still has stderr to drain
/// and an exit status to report, and a diagnostic channel that stays open as
/// long as the client holds a sink is the transport keeping that promise —
/// which is why nothing here concludes anything from a stream going quiet.
pub(super) async fn read(
    state: Arc<State>,
    client: Client,
    incoming: FrameStream,
    mut diagnostics: DiagnosticStream,
    shutdown: CancellationToken,
) {
    let sink = client.sink().clone();
    let released = drain(&state, client, incoming, &mut diagnostics, &shutdown).await;
    if released && sink.stopping_gracefully() {
        stopped(&state, &sink, diagnostics).await;
    } else {
        // The transport ended before the release was seen (or there was none):
        // whatever stop was asked for is over, so nothing is still stopping.
        locked(&state.stopping).retain(|stopping| !stopping.same(&sink));
    }
}

/// Runs the reader, and reports its panic as a terminal transport failure
/// (C8): the connection moves to Lost and its pending calls fail, instead of
/// the status staying Connected over a reader that is gone. A reader that
/// panics after its connection was released has nothing left to report.
pub(super) async fn watched(
    state: Arc<State>,
    shutdown: CancellationToken,
    reader: impl std::future::Future<Output = ()>,
) {
    if let Err(what) = crate::unwind::caught(reader).await {
        state.observe(
            Diagnostic::now(DiagnosticKind::TransportFailed(Arc::new(
                crate::unwind::internal_error(&what),
            ))),
            &shutdown,
        );
    }
}

/// How long a graceful stop may take to report how it ended.
const STOPPING: std::time::Duration = std::time::Duration::from_secs(6);

/// After a graceful disconnect, logs the one line saying how the agent
/// stopped. The frames are no longer read; the rest of its stderr is drained
/// and dropped.
async fn stopped(state: &State, sink: &FrameSink, mut diagnostics: DiagnosticStream) {
    let _ = tokio::time::timeout(STOPPING, async {
        while let Some(diagnostic) = diagnostics.recv().await {
            if matches!(diagnostic.kind, DiagnosticKind::Stopped { .. }) {
                state.diagnostics.entries.append(diagnostic);
                return;
            }
        }
    })
    .await;
    locked(&state.stopping).retain(|stopping| !stopping.same(sink));
}

/// Reads frames and diagnostics into the stores. True when it stopped because
/// the inspector released the connection.
async fn drain(
    state: &State,
    client: Client,
    mut incoming: FrameStream,
    diagnostics: &mut DiagnosticStream,
    shutdown: &CancellationToken,
) -> bool {
    // Keep ordered Frame handling (including bounded automatic reply sends)
    // concurrent with diagnostics. A stalled reply must not prevent observing
    // a terminal read failure or draining stderr that the Agent is waiting on.
    let receive = async {
        while let Some(Some(frame)) = state.while_connected(shutdown, incoming.recv()).await {
            if state
                .while_connected(shutdown, client.receive(frame))
                .await
                .is_none()
            {
                return;
            }
        }
    };
    tokio::pin!(receive);
    let mut frames = true;
    let mut lines = true;
    let mut exit = None;

    while frames || lines {
        tokio::select! {
            () = shutdown.cancelled() => return true,
            () = &mut receive, if frames => frames = false,
            diagnostic = diagnostics.recv(), if lines && exit.is_none() => match diagnostic {
                Some(diagnostic) if matches!(diagnostic.kind, DiagnosticKind::AgentExited(_)) => {
                    // The transport finished producing Frames, but a separate
                    // channel means its exit can overtake the queued tail.
                    exit = Some(diagnostic);
                }
                Some(diagnostic) => state.observe(diagnostic, shutdown),
                None => lines = false,
            },
        }
        if !frames && let Some(diagnostic) = exit.take() {
            state.observe(diagnostic, shutdown);
            return false;
        }
    }
    false
}

#[cfg(test)]
mod background_tests {
    use super::*;
    use std::sync::Mutex;

    use crate::connection::{Connection, ConnectionFactory, TransportEnds, connection};
    use crate::inspector::Inspector;

    /// A connection whose other ends the test holds.
    struct Held(Mutex<Option<Connection>>);

    impl ConnectionFactory for Held {
        fn connect(&self) -> Connection {
            self.0.lock().unwrap().take().expect("one connection")
        }
    }

    #[tokio::test]
    async fn a_reader_that_panics_loses_the_connection_with_a_diagnostic() {
        let (made, _ends): (Connection, TransportEnds) = connection();
        let inspector = Inspector::new();
        inspector.connect(&Held(Mutex::new(Some(made))));
        assert_eq!(inspector.status(), ConnectionStatus::Connected);

        let shutdown = inspector
            .0
            .connection()
            .as_ref()
            .map(|live| live.shutdown.clone())
            .expect("a live connection");
        watched(Arc::clone(&inspector.0.0), shutdown, async {
            panic!("the reader fell over")
        })
        .await;

        assert_eq!(inspector.status(), ConnectionStatus::Lost);
        let said = inspector.diagnostics().entries();
        assert!(
            said.iter().any(|line| matches!(
                &line.kind,
                DiagnosticKind::TransportFailed(error)
                    if error.to_string() == "internal error: the reader fell over (see crash log)"
            )),
            "{said:?}"
        );
        assert!(
            inspector.initialize().await.is_err(),
            "calls fail rather than wait"
        );
    }

    #[tokio::test]
    async fn a_graceful_disconnect_is_stopping_until_the_transport_lets_go() {
        let (made, ends): (Connection, TransportEnds) = connection();
        let inspector = Inspector::new();
        inspector.connect(&Held(Mutex::new(Some(made))));
        assert!(!inspector.is_stopping());

        inspector.disconnect_gracefully();
        assert_eq!(inspector.status(), ConnectionStatus::Disconnected);
        assert!(inspector.is_stopping(), "the agent has not said it stopped");

        // The transport going away is the last it will say.
        drop(ends);
        tokio::time::timeout(std::time::Duration::from_secs(2), inspector.stopped())
            .await
            .expect("the stop finishes");
        assert!(!inspector.is_stopping());
    }
}
