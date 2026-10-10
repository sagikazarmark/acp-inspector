//! The session lifecycle (§7.1, §7.5): `initialize`, opening, reopening,
//! leaving, closing and listing sessions, and authentication.

use std::path::PathBuf;

use agent_client_protocol_schema::v1;
use serde_json::Value;

use crate::auth::AuthState;
use crate::call::CallError;
use crate::driven::AgentCapability;
use crate::listing::{Cursor, SessionListing};
use crate::permission::PermissionPolicy;
use crate::restore::Restore;
use crate::roots::{self, Roots};
use crate::services::ClientClaim;
use crate::session_settings::SessionSettings;
use crate::store::locked;
use crate::turn::TurnState;

use super::driving::{drives, drives_setup};
use super::settings::Setup;
use super::{Client, Loading, client_info, decode, encode};

/// Which blocking requests a `cancel` is owed to (§7.2, §7.8).
///
/// The difference is not what is being cancelled but who can still answer:
/// cancelling a turn leaves the timeline where it is, and discarding a session
/// takes with it the only screen an elicitation could have been answered from.
#[derive(Clone, Copy)]
pub(super) enum Debt {
    /// What the turn is responsible for: every permission request, and every
    /// elicitation that named a session. One scoped to a JSON-RPC call is not
    /// the turn's, and outlives it.
    TheTurns,
    /// All of it, whatever it was scoped to.
    Everything,
}

impl Client {
    /// `initialize`: a protocol version, honest client capabilities, and who we
    /// are (§7.1).
    ///
    /// **The capabilities are what they are because they are true.** By
    /// default the inspector claims no filesystem and no terminal — the agent's
    /// behaviour gets tested against a minimal conformant client, and
    /// everything it calls anyway is a finding (§7.3). What it *does* claim, it
    /// honours: boolean config options, compaction and notices, which it
    /// renders, and both elicitation modes, which it services (§7.8).
    /// [`client_capabilities`] is that default claim and the argument for each
    /// part.
    ///
    /// The capabilities and the version are the launch's ([`ClientServices`]):
    /// version 1 and no services unless the reader chose otherwise. What went
    /// out is kept ([`Stores::claim`]) for the display to read.
    pub(crate) async fn initialize(&self) -> Result<v1::InitializeResponse, CallError> {
        let claim = ClientClaim {
            capabilities: self.services.capabilities(),
            protocol_version: self.services.protocol(),
        };
        let request = v1::InitializeRequest::new(claim.protocol_version)
            .client_capabilities(claim.capabilities.clone())
            .client_info(client_info());
        self.stores.claim.set(Some(claim));

        let response: v1::InitializeResponse = self
            .call(v1::AGENT_METHOD_NAMES.initialize, &request)
            .await?;
        self.stores.agent.set(Some(response.clone()));
        Ok(response)
    }

    /// `session/new`: the spawn form's working directory and supplied MCP definitions
    /// (§7.1).
    ///
    /// **Creating a session while one is live is a switch** (§7.5), and the
    /// switch is the substance of this: the live session is left before the
    /// call goes out, and the session that answers becomes the live one. The
    /// launch handshake's own call is not a switch — it opens the first session
    /// on a fresh connection, where there is nothing to leave and the last
    /// agent's timeline is left exactly where a connection left it.
    ///
    /// **Two things are read out of the answer**: the session's id, here, and
    /// the settings it published, by the reader that sees the answer cross
    /// ([`opened`](Self::opened)). The second is the session's own, so it
    /// replaces what the session being left published rather than merging with
    /// it — one live session, one surface describing it.
    ///
    /// **The roots it opens with are the caller's**, resolved against the `cwd`
    /// the request carries (§7.7): a control supplies them where the agent
    /// advertised `additionalDirectories`, and an empty list puts no field on
    /// the wire — which is why the call drives that advertisement exactly when
    /// the frame carries it ([`drives_roots`]).
    pub(crate) async fn new_session(
        &self,
        cwd: PathBuf,
        roots: Option<&Roots>,
        mcp: Vec<v1::McpServer>,
    ) -> Result<v1::SessionId, CallError> {
        let switching = self.stores.session.get().is_some();
        if switching {
            self.leave().await;
        }

        let request = v1::NewSessionRequest::new(cwd.clone())
            .mcp_servers(mcp)
            .additional_directories(roots::creating(roots, &cwd));
        let params = encode(&request);
        // `session/new` is the one lifecycle call no capability gates (§7.5), so
        // whatever it drives it drives by what it carries.
        //
        // Prepared and registered by hand rather than through [`drive`](Self::drive),
        // for [`restore`](Self::restore)'s reason: this needs the id before the
        // frame goes out, because the settings the answer will publish are armed
        // under it (§7.6).
        let driving = drives_setup(&params);
        let call = self
            .rpc
            .prepare(v1::AGENT_METHOD_NAMES.session_new, params)
            .inspect_err(|error| self.drive_failed(None, &driving, error))?;
        let sent = call.id();
        self.registers(sent, &driving);
        // With no session id to watch for: `session/new` mints one in its own
        // answer, so nothing crossing before it can be shown to be about the
        // session that is opening ([`stated_while_opening`](Self::stated_while_opening)).
        self.opening(sent, Setup::New, None);

        let outcome = self.rpc.send(call).await;
        if let Err(error) = &outcome {
            // A call that never became a frame the agent could answer has no
            // reader event, so the record is written by whoever awaited it —
            // under the id guard the turn and the two setters already use.
            self.drive_failed(Some(sent), &driving, error);
        }
        let response: v1::NewSessionResponse = decode(outcome?)?;

        // **Discarded after the answer, not before it**, which is what keeps
        // the timeline the new session's: the session being left has a
        // cancelled turn still resolving, and whatever it says on the way out
        // belongs to the view it is leaving. (A `session/load` replays *ahead*
        // of its answer and will have to discard before it asks — that is its
        // own ticket's problem, and this is why it is a different one.)
        // And only where there was a session to leave. A relaunch leaves one
        // too, and empties the view where it does — at the connection, which is
        // where it abandons it (`Inspector::connect`).
        if switching {
            self.stores.timeline.discard();
        }
        // A new session is a new conversation: nothing from the last one has a
        // turn state to be part of. The view first and the session last, so a
        // screen woken by the session changing reads a timeline, a turn and a
        // set of settings that already belong to it — the settings by the time
        // this call returned at all, because the reader wrote them from the
        // frame.
        self.stores.turn.set(TurnState::Idle);
        self.stores.session.set(Some(response.session_id.clone()));
        Ok(response.session_id)
    }

    /// `session/load` or `session/resume`: opens a session that already exists,
    /// and makes it the live one (§7.5).
    ///
    /// **One operation with a property, not two calls.** The two requests
    /// carry the same inputs (resume omits an empty MCP list), and their one
    /// behavioural difference is [`Restore::replays`] — the ordering the specification states, and the
    /// thing that decides whether the timeline can be rebuilt at all. Which one
    /// is sent is the caller's to say, from what the agent advertised
    /// ([`Restore::preferred`]); this sends it and never second-guesses it, the
    /// way `session/list` is sent to an agent that never advertised listing.
    ///
    /// **The view is discarded before the ask, not after it**, which is where
    /// this parts company with [`new_session`](Self::new_session): a load
    /// replays *ahead* of its answer, so a timeline emptied afterwards would be
    /// emptied of the replay it was waiting for. The cost is stated rather than
    /// hidden — whatever the session being left says on its way out lands in
    /// the view being rebuilt.
    ///
    /// **Resume is discarded before the ask too**, though nothing it does
    /// requires it. A resume that replays anyway has broken a MUST NOT, which
    /// is this tool's subject matter, and a view emptied after the answer would
    /// throw those updates away — showing less than the agent sent, which is
    /// the one thing this layer may not do (§8). One rule for both is also what
    /// makes them one operation on screen.
    ///
    /// Everything else the switch costs is [`leave`](Self::leave)'s, the same
    /// as creating one: the in-flight turn cancelled, every waiting request
    /// answered `cancelled`, and the trace untouched by any of it. And it costs
    /// nothing at all where there was no session to leave, which is
    /// [`new_session`](Self::new_session)'s rule unchanged: a connection that
    /// changed keeps what the last agent said ([`Timeline::reopen`]).
    ///
    /// **A session that could not be opened is a session that was not opened.**
    /// The failure is the caller's answer and the live session stays the one it
    /// could not replace — with whatever the agent did replay before refusing
    /// still on the timeline, because that is evidence about the agent and the
    /// view is where it can be read. A call that could never be *asked* — a
    /// connection already gone — costs the view nothing either: the frame is
    /// prepared before anything is given up, so the one failure that happens
    /// before the wire happens before the switch as well.
    ///
    /// [`Timeline::reopen`]: crate::timeline::Timeline::reopen
    ///
    /// The session's own working directory is what is asked for, because the
    /// request's `cwd` must be the session's. **The roots are the caller's**,
    /// and default to the ones the listing reported because a reopen should
    /// reopen — not because a different list would be forbidden. The schema
    /// permits one "as long as the request `cwd` matches the session's `cwd`",
    /// so a control supplies them where the agent advertised
    /// `additionalDirectories` and a reopen the user changed asks for what they
    /// changed it to (§7.5, §7.7). Whichever list it is, it crosses resolved
    /// against that `cwd` and never as typed.
    ///
    /// **The default crosses whether or not the capability was advertised.**
    /// Handing an agent its own words back is fidelity to the session being
    /// reopened rather than a claim about a capability, and gating it would
    /// make this reopen a different session from the one it named.
    ///
    /// **What became of it is in the record either way** (§7.7), on the row of
    /// whichever of the two calls the property selected ([`drives`]): an answer,
    /// a refusal, or a connection that went away holding the call is what the
    /// capability panel then says about that advertisement and not the other.
    ///
    /// **One thing is read out of the answer**, and it is the same thing
    /// `session/new`'s answer carries: the settings this session was published
    /// with (§7.6), read by the reader that sees it cross
    /// ([`opened`](Self::opened)). Both responses carry those two optional
    /// fields and nothing else that names the session, so the rest of what the
    /// answer is here is the agent's word that the session is open — and the
    /// frame is in the trace either way (§8).
    pub(crate) async fn restore(
        &self,
        session: &v1::SessionInfo,
        how: Restore,
        roots: Option<&Roots>,
        mcp: Vec<v1::McpServer>,
    ) -> Result<v1::SessionId, CallError> {
        let id = session.session_id.clone();
        let asked = roots::reopening(roots, session);
        // Two arms because the schema gives two types, not because there are two
        // operations: every field either request has, the other has, and the one
        // thing that differs is which method name they go out under. It is the
        // place where "one operation with a property" is least visible and most
        // true.
        let params = match how {
            Restore::Load => encode(
                &v1::LoadSessionRequest::new(id.clone(), session.cwd.clone())
                    .mcp_servers(mcp)
                    .additional_directories(asked.clone()),
            ),
            Restore::Resume => encode(
                &v1::ResumeSessionRequest::new(id.clone(), session.cwd.clone())
                    .mcp_servers(mcp)
                    .additional_directories(asked),
            ),
        };

        // The call's own Advertisement, and the field's where the frame carries
        // it: one call, two things driven, answered by the one frame (§7.7).
        let mut driving = vec![drives(how)];
        driving.extend(drives_setup(&params));

        // Prepared first, because this is where a connection that has gone away
        // is found out — and a switch that could never be asked for must not
        // spend the view on the way to saying so.
        let call = self
            .rpc
            .prepare(how.method(), params)
            .inspect_err(|error| self.drive_failed(None, &driving, error))?;
        let sent = call.id();
        // And registered here, with the id, rather than beside the two things
        // armed below (§7.7).
        //
        // **Because the teardown is what refuses it, and the teardown can
        // happen during the switch this is about to make.** A registration made
        // *after* `leave` would be one the teardown had already swept past: the
        // caller would then find its own entry, and write a refusal into a
        // record another agent's connection had already claimed. Registered
        // before anything is awaited, the entry is either swept — and refused —
        // or it is this connection's to answer.
        self.registers(sent, &driving);

        if self.stores.session.get().is_some() {
            self.leave().await;
            self.stores.timeline.discard();
        }

        // Armed before the frame goes out, for the reason the turn registers its
        // id before the prompt does: the replay is allowed to arrive before
        // anyone here is looking for it.
        if how.replays() {
            *locked(&self.loading) = Some(Loading {
                call: call.id(),
                session: id.clone(),
                request: call.frame().clone(),
                replayed: false,
            });
        }
        // And the settings this answer will publish, for the same reason and at
        // the same moment (§7.6). A load replays *ahead* of its answer, so an
        // agent that announced a mode change on the way announced it before the
        // answer that says whether it offered any modes at all — and the answer
        // is written when it crosses, which is after. With the session it is
        // asking for, which is what tells that replay from whatever the session
        // being left is still saying.
        self.opening(call.id(), Setup::Restore(how), Some(id.clone()));

        let outcome = self.rpc.send(call).await;
        if let Err(error) = &outcome {
            // A call that never became a frame the agent could answer has no
            // reader event, so the record is written by whoever awaited it —
            // under the id guard the turn and the two setters already use.
            self.drive_failed(Some(sent), &driving, error);
        }
        outcome?;

        self.stores.turn.set(TurnState::Idle);
        self.stores.session.set(Some(id.clone()));
        Ok(id)
    }

    /// `session/close`: asks the agent to close a session it holds (§7.5).
    ///
    /// **Nothing is asked to stop first.** A `session/close` against an
    /// in-flight turn is the observation this affordance exists to make — what
    /// the agent does with a turn it was never told to cancel — and a client
    /// that sent `session/cancel` ahead of it would be answering that question
    /// on the agent's behalf.
    pub(crate) async fn close_session(&self, session: &v1::SessionId) -> Result<(), CallError> {
        self.end_session(
            v1::AGENT_METHOD_NAMES.session_close,
            encode(&v1::CloseSessionRequest::new(session.clone())),
            session,
            AgentCapability::Close,
        )
        .await
    }

    /// `session/delete`: asks the agent to delete a session it holds (§7.5).
    ///
    /// Whether the session then leaves a subsequent listing is the agent's
    /// answer and not this client's guess, which is why nothing here touches
    /// the listing store: what an agent claims to hold is asked for again
    /// rather than edited from here.
    pub(crate) async fn delete_session(&self, session: &v1::SessionId) -> Result<(), CallError> {
        self.end_session(
            v1::AGENT_METHOD_NAMES.session_delete,
            encode(&v1::DeleteSessionRequest::new(session.clone())),
            session,
            AgentCapability::Delete,
        )
        .await
    }

    /// The two calls that end a session, which are one thing to *this* client
    /// however different they are to an agent: it asks, and if the session it
    /// asked about was the live one and the agent said it did it, the inspector
    /// has no live session any more.
    ///
    /// **They are two methods above this and not one operation with a
    /// property.** Close frees a session the agent still has and delete takes it
    /// away, which is a difference in what the agent does rather than in how it
    /// answers — unlike `session/load` and `session/resume`, whose one
    /// difference *is* a property ([`Restore`]). What they share is what happens
    /// on this side afterwards, and that is what is shared here.
    ///
    /// **A session the agent would not end is a session that did not end**, the
    /// same rule a refused open follows ([`restore`](Self::restore)): the
    /// refusal is the caller's answer and the live session stays live, because
    /// the only thing this knows about what the agent holds is what the agent
    /// said.
    ///
    /// Nothing is read out of a successful answer. Both responses carry `_meta`
    /// and nothing else, so what the answer is here is the agent's word that it
    /// did what it was asked, and the frame is in the trace either way (§8).
    ///
    /// **Both of them drive an Advertisement**, so both go out through
    /// [`drive`](Self::drive) rather than being asked outright (§7.7).
    pub(super) async fn end_session(
        &self,
        method: &str,
        params: Value,
        session: &v1::SessionId,
        driving: AgentCapability,
    ) -> Result<(), CallError> {
        let outcome = self.drive(method, params, Some(driving)).await;

        if outcome.is_ok() && self.stores.session.get().as_ref() == Some(session) {
            self.ended().await;
        }
        self.refresh_listing().await;
        outcome.map(|_| ())
    }

    /// Asks the agent for its sessions again, where it said it could be asked
    /// (§7.5).
    ///
    /// **The refresh is the point of the two calls above**, because whether a
    /// closed session still appears in a listing is exactly the kind of thing
    /// agents differ on and no document will settle: the only way to know what
    /// this agent now claims to hold is to ask it again. From the start rather
    /// than from a cursor, because it is a fresh question and the pages that
    /// answered the last one are the agent's old news.
    ///
    /// **Asked whatever the answer was**, including a refusal: an agent that
    /// declined to close a session and dropped it anyway is a finding, and one
    /// this could only hide by not looking. What it cannot do is change the
    /// answer the caller gets — a listing that fails says so in the trace and
    /// nowhere else.
    ///
    /// This is the one call this client makes that nobody clicked, so it is the
    /// one place the advertisement gates the *sending* rather than the
    /// affordance: an agent that never claimed `session/list` is not asked a
    /// question of the inspector's own invention. A user who asks for one anyway
    /// still gets it sent ([`list_sessions`](Self::list_sessions)), which is the
    /// rule unchanged — what is gated here is the inspector's initiative, not
    /// the user's.
    pub(super) async fn refresh_listing(&self) {
        let lists = self
            .stores
            .agent
            .get()
            .is_some_and(|agent| agent.agent_capabilities.session_capabilities.list.is_some());

        if lists {
            // **And it drives nothing** (§7.7). Nobody asked for this one, so
            // nothing it does reaches the record: a `session/list` row marked by
            // the tool's own housekeeping would report a call the user never
            // made and could not clear by listing again themselves.
            let _ = self.list(None, None).await;
        }
    }

    /// The live session is over, and there is not another one (§7.5).
    ///
    /// **What [`leave`](Self::leave) does, minus the asking.** A switch cancels
    /// the turn it walks away from because it is walking away; this *is* the
    /// ask, and a `session/cancel` sent ahead of it would answer the question
    /// the close was sent to ask. So the turn is let go of rather than stopped,
    /// and what the agent does with it lands in the trace like everything else.
    ///
    /// The waiting requests are still answered `cancelled`, because that debt is
    /// to *them* and it comes due when this client stops being the one that
    /// answers them (§7.2) — after the answer, so it cannot be mistaken for a
    /// client cancelling ahead of the call.
    ///
    /// **The view goes with the session.** The timeline is the view of the live
    /// session and there is no longer one, so an emptied timeline beside no
    /// session reads as what it is rather than as a session with nothing in it.
    /// The trace keeps every frame, which is where what was said stays readable.
    /// The view first and the session last, so a screen woken by the session
    /// changing reads a timeline that already agrees with it.
    ///
    /// **What the agent says next is still shown.** An update about a session it
    /// agreed to close is exactly the sort of thing this tool is pointed at, and
    /// showing less than the agent sent is the one thing this layer may not do
    /// (§8) — so nothing filters the timeline by session, and *no live session*
    /// is what the session store says rather than what an empty timeline
    /// implies. What crosses in the moment between the answer and this discard
    /// goes with the view, which is the cost a switch already states.
    pub(super) async fn ended(&self) {
        // Everything, request-scoped elicitations included (§7.8): the timeline
        // that would have answered them is about to be discarded, and a debt
        // whose screen is gone cannot be paid.
        self.answer_cancelled(Debt::Everything).await;
        self.forget();
        self.stores.policy.set(PermissionPolicy::Ask);
        self.stores.timeline.discard();
        // The settings go the way the view does, and for the same reason: they
        // are the *live session's* account of itself (§7.6), and a surface still
        // offering the modes of a session the agent no longer has would be
        // describing something that is not there.
        self.stores.settings.set(SessionSettings::default());
        self.stores.session.set(None);
    }

    /// Stops being the client of the session that was live: nothing is awaited
    /// on its behalf, no conformance rule is armed on it, and no turn is running
    /// in it.
    ///
    /// Both ways of leaving a session go through here. An answer that arrives
    /// afterwards still reaches whoever asked for it and reaches nothing on
    /// screen, because every store above this is the *live* session's — and the
    /// rules that were armed are decided from frames a client that stopped
    /// listening is no longer reading (§15 q7).
    pub(super) fn forget(&self) {
        *locked(&self.awaiting) = None;
        *locked(&self.cancelled) = None;
        *locked(&self.loading) = None;
        // A mode or an option asked for in the session being left is not this
        // client's question any more: the settings store is the *live*
        // session's, and an answer that arrives afterwards would report the
        // last session's ask beside the new one's settings — or rebuild the new
        // session's options out of the old session's answer.
        *locked(&self.changing_mode) = None;
        *locked(&self.setting_option) = None;
        self.stores.turn.set(TurnState::Idle);
    }

    /// Walks away from the live session, leaving nothing of it running (§7.5).
    ///
    /// **The sequence is [`cancel`](Self::cancel)'s, reused rather than
    /// restated**: a turn this client stops is owed a `session/cancel`, and
    /// every request waiting on the user is owed a `cancelled` — which is what
    /// a client owes them and the only way an agent blocked on one gets to end
    /// its turn (§7.2). Walking away without either would leave the agent
    /// believing a turn is running with a client that stopped listening.
    ///
    /// The requests are answered whether or not there was a turn to cancel,
    /// because the debt is to *them*: an agent that asked outside a turn is
    /// misbehaving, which is this tool's subject matter rather than a reason to
    /// tell it nothing (§8). Only the notification is a turn's — nothing is
    /// asked to stop when nothing is running.
    ///
    /// **It does not wait for the turn to end.** The specification says the
    /// agent MUST resolve a cancelled prompt and an agent that never does is
    /// exactly the sort this tool is pointed at, so a switch that waited would
    /// hang on the agents worth inspecting. The turn is left behind instead:
    /// what answers it afterwards is an answer about the session that was left,
    /// and [`TurnState`] is the live session's — so the turn stops being
    /// awaited here, and the frame the conformance rule was armed with is
    /// dropped with it. That rule is decided from captured frames (§15 q7), and
    /// a client that walked away has none to decide it with; the frames
    /// themselves are in the trace either way, which is the only place a switch
    /// promises anything.
    pub(super) async fn leave(&self) {
        if self.stores.turn.get().is_running() {
            let _ = self.cancel().await;
        }
        // Whatever is still waiting: one that arrived beside the cancel, or one
        // from an agent that never started a turn to ask inside. Everything,
        // for [`ended`](Self::ended)'s reason — the view they are answered from
        // goes with the session.
        self.answer_cancelled(Debt::Everything).await;
        // The policy was the session's being left.
        self.stores.policy.set(PermissionPolicy::Ask);

        // And the turn is over here, whatever the agent does with it next. Not
        // a stop reason of this client's invention — there is no turn on the
        // session being opened, and a switch that failed must not leave a
        // composer waiting on an answer nobody is listening for.
        self.forget();
    }

    /// Answers the requests the agent is waiting on `cancelled` (§7.2, §7.8).
    ///
    /// The two callers are the two ways this client stops being the one that
    /// answers them: cancelling the turn they blocked, and walking away from
    /// the session they were asked in. Both owe them the same answer, so both
    /// send it from here — and they differ in *which* requests they owe it to,
    /// which is [`Debt`]'s whole job.
    pub(super) async fn answer_cancelled(&self, debt: Debt) {
        for request in self.stores.permissions.take() {
            // One that was answered in the moment before this — the user
            // clicking as the stop button was pressed — refuses the second
            // answer itself, which is the whole of what happens about it.
            let _ = request.cancelled().await;
        }
        let elicitations = match debt {
            Debt::Everything => self.stores.elicitations.take(),
            Debt::TheTurns => self
                .stores
                .elicitations
                .take_unless(|request| request.session_id().is_none()),
        };
        for request in elicitations {
            let _ = request.cancel().await;
        }
        // Client-service requests, like elicitations, belong to the turn when
        // they name a session.
        let services = match debt {
            Debt::Everything => self.stores.services.take(),
            Debt::TheTurns => self
                .stores
                .services
                .take_unless(|request| request.session_id().is_none()),
        };
        for request in services {
            let _ = request.cancelled().await;
        }
    }

    /// `session/list`: what the agent says it has open (§7.1).
    ///
    /// `from` is a [`Cursor`] a previous listing carried, handed back exactly
    /// as it arrived, or `None` to ask from the start. **Neither this layer nor
    /// anything above it can read one** ([`Cursor`] says why): the
    /// protocol calls it opaque, no surveyed agent emits one, and a client that
    /// took a guess at the inside of a token nobody has seen would be inventing
    /// the one part of this call it has no evidence for.
    ///
    /// A continuation adds to the listing it continues; asking from the start
    /// replaces it, because that is a fresh question and the last answer is the
    /// agent's old news.
    ///
    /// **Somebody asked for this one, which is what makes it a drive** (§7.7).
    /// The same call goes out on the tool's own initiative after a close or a
    /// delete ([`refresh_listing`](Self::refresh_listing)) and records nothing;
    /// the record is about what an agent did when it was *asked*.
    pub(crate) async fn list_sessions(
        &self,
        from: Option<Cursor>,
    ) -> Result<SessionListing, CallError> {
        self.list(from, Some(AgentCapability::List)).await
    }

    /// The listing itself, whoever wanted it: `driving` is the capability the
    /// ask goes on the record as, or `None` for the one nobody asked for.
    pub(super) async fn list(
        &self,
        from: Option<Cursor>,
        driving: Option<AgentCapability>,
    ) -> Result<SessionListing, CallError> {
        let request = v1::ListSessionsRequest::new().cursor(from.as_ref().map(Cursor::token));

        let answer = self
            .drive(
                v1::AGENT_METHOD_NAMES.session_list,
                encode(&request),
                driving,
            )
            .await?;
        let response: v1::ListSessionsResponse = decode(answer)?;
        let page = SessionListing {
            sessions: response.sessions,
            next_page: response.next_cursor.map(Cursor::new),
        };

        // Folded under the field's own lock rather than read-then-written, so
        // two listings in flight at once cannot lose a page between them.
        let listed = self.stores.listing.change(|held| {
            let mut folded = held.take().filter(|_| from.is_some()).unwrap_or_default();
            folded.extend(page);
            *held = Some(folded);
        });
        Ok(listed.unwrap_or_default())
    }

    /// `authenticate`: the method the user picked, and nothing invented around
    /// it (§7.1).
    ///
    /// **Display-only authentication** (§1.1): this sends the call and records
    /// what came back. It cannot log anyone in — the flows real agents run are
    /// theirs, in the user's own terminal — so what it produces is a state, and
    /// the screen showing that state is the whole of the feature.
    ///
    /// The outcome lands in the store whether or not anyone awaited the call,
    /// like every other thing a screen renders here.
    pub(crate) async fn authenticate(&self, method: &v1::AuthMethodId) -> Result<(), CallError> {
        let request = v1::AuthenticateRequest::new(method.clone());

        let outcome: Result<v1::AuthenticateResponse, CallError> = self
            .call(v1::AGENT_METHOD_NAMES.authenticate, &request)
            .await;

        self.stores.auth.set(match &outcome {
            Ok(_) => AuthState::Authenticated(method.clone()),
            Err(error) => AuthState::Refused {
                method: method.clone(),
                error: error.clone(),
            },
        });
        outcome.map(|_| ())
    }

    /// `logout`: the agent is asked to give back what it accepted, and nothing
    /// else is asked of it (§7.1, §7.7).
    ///
    /// **Connection-scoped, like the call.** The request carries no session id
    /// and the method name is the bare `logout`, which is why the button that
    /// sends it sits with the auth methods rather than among the session
    /// controls.
    ///
    /// **The live session is left strictly alone.** Nothing is cancelled and
    /// nothing is closed ahead of it: what the agent does to a session it has
    /// logged out of is the finding — the specification guarantees nothing there
    /// — and pre-empting it would answer the question on the agent's behalf.
    ///
    /// **It answers `{}`, so it licenses no claim about where authentication
    /// stands.** What a success says is that the agent has taken back what it
    /// accepted, so the state goes back to [`AuthState::Unasked`]: going on
    /// saying *the agent accepted `authenticate` with X* would be a screen
    /// stating something the agent has retracted. It does **not** raise the
    /// login screen — the schema predicts that new sessions will require
    /// authentication, and a prediction is not the agent asking for one. What
    /// the agent refuses next settles that honestly, wherever every other
    /// `auth_required` is read ([`note_login`](Self::note_login)).
    ///
    /// **And a refusal claims nothing either way.** The agent retracted
    /// nothing, so the state stands as it was, and what became of the call is on
    /// the record with every other driven advertisement's outcome (§7.7).
    pub(crate) async fn logout(&self) -> Result<(), CallError> {
        let request = v1::LogoutRequest::new();

        let answer = self
            .drive(
                v1::AGENT_METHOD_NAMES.logout,
                encode(&request),
                Some(AgentCapability::Logout),
            )
            .await?;
        // Read as v1 for the reason every other answer this client acts on is:
        // an answer it could not take is not one to act on. Whether the
        // *advertisement* was answered is a different question and is already
        // settled, off the answer's shape rather than off this decode (§7.7).
        decode::<v1::LogoutResponse>(answer)?;

        self.stores.auth.set(AuthState::Unasked);
        Ok(())
    }

    /// Reads a login out of a refusal, wherever the refusal came from.
    ///
    /// **`auth_required` is an answer about the agent, not about the call**
    /// (§7.1): whichever method it refuses — `session/new` is the usual one, a
    /// prompt on an agent whose credentials expired is the other — what it says
    /// is that this agent will not proceed until somebody logs in. So it is
    /// read here, where every answer crosses, rather than at each call site
    /// that might provoke one; a screen that only learned about it from
    /// whichever call happened to be awaited would be showing a state that
    /// depended on who was listening.
    pub(super) fn note_login(&self, outcome: &Result<Value, v1::Error>) {
        if let Err(error) = outcome
            && error.code == v1::ErrorCode::AuthRequired
        {
            self.stores.auth.set(AuthState::Required(error.clone()));
        }
    }
}
