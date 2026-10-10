//! The Agent screen's and the Sessions dialog's inputs: core's gates, and the
//! handlers that drive the session lifecycle and authentication.

use acp_inspector_core::{AgentCommand, CallError, Cursor, Inspector, Restore, Roots, v1};
use dioxus::prelude::*;

use super::stores::Stores;
use crate::agent::{self, AuthenticationCall, Failed, Opening};
use crate::{client_services, session};

// Authentication has no protocol-level loading state. This is only the
// Connection-scoped presentation fact; core's AuthState remains the one
// account of whether the Agent accepted or refused the request.
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
    // One core-owned answer drives both the listing claim and its Affordance in
    // the Agent panel. Reading the description subscribes this memo to the next
    // agent even though the protocol interpretation remains core's (§5).
    let gated = inspector.clone();
    let lists_sessions = use_memo(move || described.read().is_some() && gated.lists_sessions());

    // And which way into a session it already has, if any — the same question
    // asked of the same store, with the preference between the two calls core's
    // as well (§7.5): a window that picked between `session/load` and
    // `session/resume` would be a window deciding protocol.
    let restoring = inspector.clone();
    let restores = use_memo(move || {
        described
            .read()
            .is_some()
            .then(|| restoring.advertised_restore())
            .flatten()
    });

    // And the two ways out of one, each its own claim in its own field (§7.5).
    // Same shape as the listing gate for the same reason: reading the signal is
    // the subscription and the condition at once.
    let closing = inspector.clone();
    let closes = use_memo(move || described.read().is_some() && closing.closes_sessions());
    let deleting = inspector.clone();
    let deletes = use_memo(move || described.read().is_some() && deleting.deletes_sessions());

    // And whether a session can be opened with roots the user supplies (§7.7),
    // which is the same question again about the one advertisement here that
    // gates a field on a request rather than a method. It gates the control and
    // nothing else: a reopen hands the agent back the roots it reported whatever
    // this says, which is core's rule and not a thing this window decides.
    let rooting = inspector.clone();
    let opens_with_roots =
        use_memo(move || described.read().is_some() && rooting.opens_with_roots());

    // And the way out of a login, claimed in the same shape under `auth` rather
    // than under `sessionCapabilities` (§7.7). Gated on the advertisement and on
    // nothing else — never on where authentication stands, because a logout with
    // nobody logged in is the corner worth driving.
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
    // Presentation state for listing requests, scoped to the Connection that
    // started them. Multiple asks remain possible because the Affordance is
    // gated only by the Advertisement; the count keeps the notice visible until
    // every ask on this Connection has answered.
    pub listing_work: Signal<(u64, usize)>,
    // Which lifecycle call last failed, and what it said — `session/new`, a
    // listed session reopened, closed or deleted. Its own signal rather than
    // `problem`: that one is the handshake's, and a refusal here leaves whatever
    // was live still live. One signal for all four because it is one sentence in
    // one place: the affordances sit together and the latest answer is the one
    // worth reading — and it carries the method, because which of the four
    // refused is the finding (§7.5).
    pub failed: Signal<Option<Failed>>,
    // Why the last launch never got as far as a session, if it did not. Core
    // answers the handshake once; the stores carry everything else.
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

// Everything about the agent that two surfaces draw: the Agent screen,
// which says what it claimed, and the Sessions dialog, which acts on what
// it has. One value built once — two literals would be two places for the
// same fact to be wrong.
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
        // What this connection's `initialize` claimed, or what the next
        // launch will (F3, F13).
        client: client_services::claim(inspector),
        client_info: acp_inspector_core::client_info(),
        driven: (stores.driven)(),
        auth: (stores.auth)(),
        lists_sessions: (gates.lists_sessions)(),

        session: (stores.session)(),
        auth_pending: auth_work().pending,
        listing: (stores.listing)(),
        listing_pending: listing_work().1 > 0,
        restores: restores(),
        closes: (gates.closes)(),
        deletes: (gates.deletes)(),
        opens_with_roots: (gates.opens_with_roots)(),
        connected: stores.connected(),
        failed: failed(),
        logs_out: (gates.logs_out)(),
        // Neither answer is waited for here either: both land in a
        // store on their way past, and this window renders stores.
        on_authenticate: EventHandler::new(move |method: v1::AuthMethodId| {
            let inspector = authenticating.clone();
            let call = AuthenticationCall::Authenticate(method.clone());
            let connection = auth_work.with_mut(|work| work.begin(call.clone()));
            spawn(async move {
                let _ = inspector.authenticate(&method).await;
                auth_work.with_mut(|work| work.finish(connection, &call));
            });
        }),
        // And the way back out of one. Nothing is waited for here
        // either, and nothing else is sent: the live session is left
        // strictly alone, because what the agent does to a session it
        // has logged out of is the finding (§7.7). What came of the
        // call is on the `logout` row above, where the record draws
        // it.
        on_logout: EventHandler::new(move |()| {
            let inspector = logging_out.clone();
            let call = AuthenticationCall::Logout;
            let connection = auth_work.with_mut(|work| work.begin(call.clone()));
            spawn(async move {
                let _ = inspector.logout().await;
                auth_work.with_mut(|work| work.finish(connection, &call));
            });
        }),
        // A second session on the agent that is already running.
        // Everything the switch costs — the turn cancelled, the
        // requests answered `cancelled`, the timeline discarded and
        // rebuilt — is core's, in the one call, because a sequence
        // in an event handler is a sequence each surface would have
        // to get right again (§5). What comes back is why there is
        // no new session, if there is none.
        // The roots ride with it, because they are part of the ask
        // rather than a setting: what the control beside the button
        // holds is what this session is opened with, and `None` is
        // an agent that advertised no control at all (§7.7).
        on_new_session: EventHandler::new(move |roots: Option<Roots>| {
            if !crate::mcp::ready_to_open() {
                return;
            }
            let inspector = opening.clone();
            // The invocation, not a directory: which command is
            // running is this window's to remember, and what
            // session a command opens is core's to decide (§7.1).
            // There is no agent here that was not launched from the
            // form, and the button is only drawn once one has
            // described itself, so the empty case is a shape rather
            // than a path.
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
        // A session the agent listed, opened. Which call that takes
        // is core's answer read back out of the same memo the rows
        // were drawn from, so the button and the call cannot say
        // different things; the switch it costs is core's too, in
        // the one call (§5). What comes back is why there is no new
        // live session, if there is none.
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
        // The two ways a session ends, and the same shape as every
        // other affordance here: one call into core, nothing waited
        // for, and what comes back is why it did not happen. The
        // listing is asked for again by core rather than from here,
        // because a call and the question it makes worth asking are
        // one sequence and sequences are core's (§5).
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
