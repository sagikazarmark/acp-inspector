//! The Agent screen's and the Sessions dialog's inputs: core's gates, and the
//! handlers that drive the session lifecycle and authentication.

use acp_inspector_core::{AgentCommand, CallError, Cursor, Inspector, Restore, Roots, v1};
use dioxus::prelude::*;

use super::stores::Stores;
use crate::agent::{self, AuthenticationCall, Failed, Opening};
use crate::{client_services, session};

/// Authentication calls in flight, scoped to the connection. Presentation only:
/// core's `AuthState` is the account of what the agent answered.
#[derive(Clone, Default)]
pub(crate) struct AuthenticationWork {
    connection: u64,
    pub pending: Vec<AuthenticationCall>,
}

impl AuthenticationWork {
    pub fn reset(&mut self) {
        self.connection += 1;
        self.pending.clear();
    }

    fn begin(&mut self, call: AuthenticationCall) -> u64 {
        self.pending.push(call);
        self.connection
    }

    fn finish(&mut self, connection: u64, call: &AuthenticationCall) {
        if self.connection == connection
            && let Some(index) = self.pending.iter().position(|pending| pending == call)
        {
            self.pending.remove(index);
        }
    }
}

/// What the agent advertised, as the gates the affordances are drawn behind.
#[derive(Clone, Copy)]
pub(crate) struct Gates {
    pub lists_sessions: Memo<bool>,
    pub restores: Memo<Option<Restore>>,
    pub closes: Memo<bool>,
    pub deletes: Memo<bool>,
    pub opens_with_roots: Memo<bool>,
    pub logs_out: Memo<bool>,
}

pub(crate) fn use_gates(
    inspector: &Inspector,
    described: Signal<Option<v1::InitializeResponse>>,
) -> Gates {
    // Reading the description subscribes each gate to the next agent; the
    // protocol answer itself stays core's.
    let gated = inspector.clone();
    let lists_sessions = use_memo(move || described.read().is_some() && gated.lists_sessions());

    // Core also picks between `session/load` and `session/resume`.
    let restoring = inspector.clone();
    let restores = use_memo(move || {
        described
            .read()
            .is_some()
            .then(|| restoring.advertised_restore())
            .flatten()
    });

    let closing = inspector.clone();
    let closes = use_memo(move || described.read().is_some() && closing.closes_sessions());
    let deleting = inspector.clone();
    let deletes = use_memo(move || described.read().is_some() && deleting.deletes_sessions());

    // Gates the roots control only: a reopen sends the reported roots whatever
    // this says.
    let rooting = inspector.clone();
    let opens_with_roots =
        use_memo(move || described.read().is_some() && rooting.opens_with_roots());

    // Never gated on where authentication stands: logout with nobody logged in
    // is worth driving.
    let advertising_logout = inspector.clone();
    let logs_out = use_memo(move || described.read().is_some() && advertising_logout.logs_out());

    Gates {
        lists_sessions,
        restores,
        closes,
        deletes,
        opens_with_roots,
        logs_out,
    }
}

/// Opens a new session on the running agent and records why not, if not.
pub(crate) fn open_session(
    inspector: Inspector,
    command: AgentCommand,
    roots: Option<Roots>,
    mut failed: Signal<Option<Failed>>,
    mut problem: Signal<Option<CallError>>,
) {
    failed.set(None);
    spawn(async move {
        let outcome = inspector.open_session(&command, roots).await;
        if outcome.is_ok() {
            problem.set(None);
            session::close();
        }
        failed.set(
            outcome
                .err()
                .map(Failed::of(v1::AGENT_METHOD_NAMES.session_new)),
        );
    });
}

/// The signals the session and authentication handlers write, and a launch
/// resets.
#[derive(Clone, Copy)]
pub(crate) struct Lifecycle {
    pub auth_work: Signal<AuthenticationWork>,
    /// Connection generation and listing asks in flight; the notice stays up
    /// until every ask on this connection has answered.
    pub listing_work: Signal<(u64, usize)>,
    /// The last lifecycle call that failed, with its method. Separate from
    /// `problem`, which is the handshake's: a refusal here leaves the live
    /// session live.
    pub failed: Signal<Option<Failed>>,
    /// Why the last launch never reached a session, if it did not.
    pub problem: Signal<Option<CallError>>,
}

pub(crate) fn use_lifecycle() -> Lifecycle {
    Lifecycle {
        auth_work: use_signal(AuthenticationWork::default),
        listing_work: use_signal(|| (0u64, 0usize)),
        failed: use_signal(|| None::<Failed>),
        problem: use_signal(|| None::<CallError>),
    }
}

/// The agent's claims and the handlers acting on them, shared by the Agent
/// screen and the Sessions dialog.
pub(crate) fn claims(
    inspector: &Inspector,
    stores: Stores,
    gates: Gates,
    lifecycle: Lifecycle,
    launched: Signal<Option<AgentCommand>>,
) -> agent::Claims {
    let Lifecycle {
        mut auth_work,
        mut listing_work,
        mut failed,
        mut problem,
    } = lifecycle;
    let restores = gates.restores;
    let authenticating = inspector.clone();
    let logging_out = inspector.clone();
    let opening = inspector.clone();
    let listing_agent = inspector.clone();
    let restoring_session = inspector.clone();
    let closing_session = inspector.clone();
    let deleting_session = inspector.clone();

    agent::Claims {
        agent: (stores.described)(),
        // What this connection's `initialize` claimed, or what the next launch will.
        client: client_services::claim(inspector),
        client_info: acp_inspector_core::client_info(),
        driven: (stores.driven)(),
        auth: (stores.auth)(),
        lists_sessions: (gates.lists_sessions)(),

        session: (stores.session)(),
        auth_pending: auth_work().pending,
        listing: (stores.listing)(),
        reports: (stores.reports)(),
        listing_pending: listing_work().1 > 0,
        restores: restores(),
        closes: (gates.closes)(),
        deletes: (gates.deletes)(),
        opens_with_roots: (gates.opens_with_roots)(),
        connected: stores.connected(),
        failed: failed(),
        logs_out: (gates.logs_out)(),
        // Not waited for: the answers land in stores.
        on_authenticate: EventHandler::new(move |method: v1::AuthMethodId| {
            let inspector = authenticating.clone();
            let call = AuthenticationCall::Authenticate(method.clone());
            let connection = auth_work.with_mut(|work| work.begin(call.clone()));
            spawn(async move {
                let _ = inspector.authenticate(&method).await;
                auth_work.with_mut(|work| work.finish(connection, &call));
            });
        }),
        // Nothing else is sent: what the agent does to the live session after a
        // logout is the finding.
        on_logout: EventHandler::new(move |()| {
            let inspector = logging_out.clone();
            let call = AuthenticationCall::Logout;
            let connection = auth_work.with_mut(|work| work.begin(call.clone()));
            spawn(async move {
                let _ = inspector.logout().await;
                auth_work.with_mut(|work| work.finish(connection, &call));
            });
        }),
        // The switch's costs (cancelled turn and requests, rebuilt timeline) are
        // core's, in one call. `None` roots means the agent advertised no control.
        on_new_session: EventHandler::new(move |roots: Option<Roots>| {
            if !crate::mcp::ready_to_open() {
                return;
            }
            let inspector = opening.clone();
            // The button is only drawn once a launched agent described itself.
            let Some(command) = launched() else {
                return;
            };
            open_session(inspector, command, roots, failed, problem);
        }),
        on_list: EventHandler::new(move |from: Option<Cursor>| {
            let inspector = listing_agent.clone();
            let connection = listing_work().0;
            listing_work.with_mut(|(_, pending)| *pending += 1);
            spawn(async move {
                let _ = inspector.list_sessions(from).await;
                listing_work.with_mut(|(current, pending)| {
                    if *current == connection {
                        *pending = pending.saturating_sub(1);
                    }
                });
            });
        }),
        // Which call reopens it is the same memo the rows were drawn from.
        on_open: EventHandler::new(move |opening: Opening| {
            if !crate::mcp::ready_to_open() {
                return;
            }
            let inspector = restoring_session.clone();
            let Some(how) = restores() else {
                return;
            };
            failed.set(None);
            spawn(async move {
                let outcome = inspector
                    .restore_session(&opening.session, how, opening.roots)
                    .await;
                if outcome.is_ok() {
                    problem.set(None);
                    session::close();
                }
                failed.set(outcome.err().map(Failed::of(how.method())));
            });
        }),
        // Core asks for the listing again afterwards.
        on_close: EventHandler::new(move |session: v1::SessionId| {
            let inspector = closing_session.clone();
            failed.set(None);
            spawn(async move {
                let outcome = inspector.close_session(&session).await;
                failed.set(
                    outcome
                        .err()
                        .map(Failed::of(v1::AGENT_METHOD_NAMES.session_close)),
                );
            });
        }),
        on_delete: EventHandler::new(move |session: v1::SessionId| {
            let inspector = deleting_session.clone();
            failed.set(None);
            spawn(async move {
                let outcome = inspector.delete_session(&session).await;
                failed.set(
                    outcome
                        .err()
                        .map(Failed::of(v1::AGENT_METHOD_NAMES.session_delete)),
                );
            });
        }),
    }
}
