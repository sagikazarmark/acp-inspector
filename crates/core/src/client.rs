//! The typed ACP client (`docs/architecture.md` §5, §14 step 3): the driven
//! methods on the way out, and one place that reads everything on the way in.
//!
//! **It decorates; it does not replace.** By the time a frame reaches this
//! layer the trace already has it (§6.2), so nothing here can lose a frame,
//! misreport one, or turn one into an error — the worst it can do is fail to
//! recognize one, and a frame it fails to recognize becomes a first-class
//! unrecognized entry carrying its own JSON (§8). That is the raw-first rule as
//! a program: the types are a decoration over a record that is already
//! complete.
//!
//! **It decodes as v1, explicitly** (§11 seam 1). Every type it names comes out
//! of the schema crate's `v1` module, so a v2 decoder is a second one of these
//! rather than an excavation of this one, and v2 traffic arriving today degrades
//! to something visible instead of breaking anything.
//!
//! **The conformant path only** (§7.1, §7.5, §7.6): `initialize`,
//! `session/new`, `session/prompt`, `session/cancel`, `session/list`,
//! `session/load`, `session/resume`, `session/close`, `session/delete`,
//! `session/set_mode`, `session/set_config_option`, `authenticate` and
//! `logout` — the six that open, close or list a session driven from what the
//! agent advertised about *itself*, which is the same `initialize` result the
//! capability display renders, `logout` from the auth block of that same
//! result, and the two setters from what the live session's setup response
//! published, which is an advertisement of a different shape and not a
//! capability at all. What the inspector does not offer, it declines by
//! advertising no capabilities (§7.3) — and answers method-not-found to,
//! loudly, when an agent calls it anyway.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use agent_client_protocol_schema::{ProtocolVersion, v1};
use serde::Serialize;
use serde_json::Value;

use crate::auth::AuthState;
use crate::blocking::{Pending, RequestId, Resolver};
use crate::call::CallError;
use crate::conformance::{Annotation, Ending, OptionSet, Replay, Reuse};
use crate::connection::{FrameSink, Received};
use crate::driven::{AgentCapability, DrivenRecord};
use crate::elicitation::{ElicitationRequest, Outstanding};
use crate::frame::Frame;
use crate::listing::{Cursor, SessionListing};
use crate::permission::PermissionRequest;
use crate::restore::Restore;
use crate::roots::{self, Roots};
use crate::rpc::{Incoming, Rpc, classify, ours};
use crate::session_settings::SessionSettings;
use crate::store::{Field, locked};
use crate::timeline::{Recorded, Timeline, Unrecognized};
use crate::turn::{TurnOutcome, TurnState};

/// What the agent is told this client is.
///
/// The crate's name and version, which are the provisional ones the whole
/// project carries (§3) — an agent that logs its clients should log something
/// true, and "ACP Inspector" is what this is until the name is settled.
const CLIENT_NAME: &str = "acp-inspector";
const CLIENT_TITLE: &str = "ACP Inspector";

/// Who this client says it is in every `initialize` request.
///
/// Public for the same reason as [`client_capabilities`]: a surface that shows
/// the claim must read the value the wire gets rather than restating it.
#[must_use]
pub fn client_info() -> v1::Implementation {
    v1::Implementation::new(CLIENT_NAME, env!("CARGO_PKG_VERSION")).title(CLIENT_TITLE)
}

/// What this client claims about *itself*, on every connection (§7.3, §7.6,
/// §7.8).
///
/// **The boolean config option is a data shape rather than a service.** It is
/// gated by a client capability, so an agent that never hears this must not send
/// one — and a surveyed agent honours that precisely, shipping an option as a
/// boolean to clients that claimed the capability and degrading it to a
/// two-value select for everyone else. Declining it did not spare this tool any
/// work; it changed what the subject offered, by a claim nobody had examined.
/// Claiming it is what makes the inspector observe an agent's real behaviour
/// instead of a lesser one shaped for it.
///
/// **Elicitation is a service, and it is claimed because this tool performs
/// it** (§7.8). Both modes: a form is built from the agent's own schema, and a
/// URL is shown in full and handed to the reader's own browser. What is claimed
/// is what will be honoured, so a mode outside these two is refused on the wire
/// rather than answered on the reader's behalf.
///
/// **The MVP's empty client capabilities are narrowed here, not abandoned.**
/// That stance was about *services* the inspector would have to perform on the
/// agent's behalf and genuinely cannot: `fs/*` has no filesystem mediation to
/// offer and `terminal/*` no terminal to embed. Both are still declined in this
/// very value, and a call that arrives anyway is still recorded and still
/// answered method-not-found (§8). The rule every half shares is unchanged:
/// **this client claims exactly what it will honour.**
///
/// **A function of nothing, which is the whole of "not configurable"** (§7.6).
/// There is no argument to vary it by, no store behind it and no spawn-form
/// switch: watching an agent degrade its offer is real behaviour worth seeing,
/// and the first configuration knob on this tool is a decision of its own that
/// this ring does not smuggle in. It is public because a screen has to state
/// what was claimed ([§9](../../../docs/architecture.md)) and reading it off the same
/// value the wire gets is what keeps the screen from making a claim of its own.
#[must_use]
pub fn client_capabilities() -> v1::ClientCapabilities {
    v1::ClientCapabilities::new()
        .session(
            v1::ClientSessionCapabilities::new().config_options(
                v1::SessionConfigOptionsCapabilities::new()
                    .boolean(v1::BooleanConfigOptionCapabilities::new()),
            ),
        )
        .elicitation(
            v1::ElicitationCapabilities::new()
                .form(v1::ElicitationFormCapabilities::new())
                .url(v1::ElicitationUrlCapabilities::new()),
        )
}

/// Whether this client advertised the mode an elicitation asks in (§7.8).
///
/// The two [`client_capabilities`] claims, and the escape hatch the schema keeps
/// for modes it has no name for is deliberately not one of them: an extension
/// mode is a shape nobody here claimed, and the specification's own answer to a
/// mode the client did not advertise is an error rather than a rendering.
fn advertised(mode: &v1::ElicitationMode) -> bool {
    matches!(
        mode,
        v1::ElicitationMode::Form(_) | v1::ElicitationMode::Url(_)
    )
}

/// A mode as the agent spelled it on the wire.
///
/// Read back out of the serialization rather than matched arm by arm, for the
/// reason a stop reason is (`conformance::spelt`) and one more: the only mode
/// this is ever asked about is one this client has no name for, so a name of its
/// own invention is the one thing it must not print.
fn spelt_mode(mode: &v1::ElicitationMode) -> String {
    serde_json::to_value(mode)
        .ok()
        .and_then(|value| {
            value
                .get("mode")
                .and_then(Value::as_str)
                .map(|mode| format!("mode `{mode}`"))
        })
        .unwrap_or_else(|| "an elicitation mode with no name on the wire".to_owned())
}

/// Which blocking requests a `cancel` is owed to (§7.2, §7.8).
///
/// The difference is not what is being cancelled but who can still answer:
/// cancelling a turn leaves the timeline where it is, and discarding a session
/// takes with it the only screen an elicitation could have been answered from.
#[derive(Clone, Copy)]
enum Debt {
    /// What the turn is responsible for: every permission request, and every
    /// elicitation that named a session. One scoped to a JSON-RPC call is not
    /// the turn's, and outlives it.
    TheTurns,
    /// All of it, whatever it was scoped to.
    Everything,
}

/// The stores the typed layer writes and the screens read.
///
/// Held by the inspector and shared into each connection's client, because they
/// outlive any one connection: what an agent claimed and what it said stay
/// readable after it is gone, which is the rule the trace and the diagnostic log
/// already follow.
#[derive(Clone, Default)]
pub(crate) struct Stores {
    pub(crate) agent: Field<Option<v1::InitializeResponse>>,
    pub(crate) session: Field<Option<v1::SessionId>>,
    pub(crate) listing: Field<Option<SessionListing>>,
    pub(crate) auth: Field<AuthState>,
    pub(crate) turn: Field<TurnState>,
    /// What the live session is configured as (§7.6): the modes and the config
    /// options the agent published for it, and whatever it has said about them
    /// since. The session's own, unlike everything above it that is the
    /// connection's — which is why opening another session replaces it whole.
    pub(crate) settings: Field<SessionSettings>,
    /// What became of driving each of the agent's advertisements on this
    /// connection (§7.7): not driven, answered, or refused with the agent's own
    /// error. The connection's rather than the live session's — a capability is
    /// claimed once per connection and is true of every session on it, so
    /// opening another session leaves this alone.
    pub(crate) driven: Field<DrivenRecord>,
    pub(crate) timeline: Timeline,
    /// The permission requests the agent is still waiting on (§7.2). Screens
    /// *render* them off the timeline, where the turn stopped for each of them;
    /// this is the list, for the things that are true of all of them at once —
    /// who a cancel owes an answer to, what a turn's end gives up on, and
    /// whether the agent is waiting on anybody at all.
    pub(crate) permissions: Pending<PermissionRequest>,
    /// The elicitations it is still waiting on (§7.8) — the same list for the
    /// same three questions, kept apart because the answers are different
    /// sentences and one of them outlives the turn it arrived in.
    pub(crate) elicitations: Pending<ElicitationRequest>,
    /// The URL elicitations nobody has said are finished (§7.8): what an
    /// `elicitation/complete` is matched against, and what decides whether an
    /// `elicitationId` was reused while still outstanding.
    pub(crate) outstanding: Outstanding,
}

/// One connection's client: the conversation, and what it writes into.
///
/// A handle, not the client: cloning shares the pending calls and the stores,
/// so the task reading the agent and the task driving it are one client.
#[derive(Clone)]
pub(crate) struct Client {
    rpc: Rpc,
    stores: Stores,
    /// The id of the answer the open turn is waiting for.
    ///
    /// This is what makes the turn's state stream-driven rather than
    /// call-bracketed (§11 seam 2): the reader recognizes the turn's answer
    /// when it crosses, and settles the state from the frame. Whoever is
    /// awaiting the call is told afterwards, and could be nobody.
    awaiting: Arc<Mutex<Option<i64>>>,
    /// The `session/cancel` that cancelled the open turn, held until the turn
    /// resolves — and `None` whenever no rule is watching (§15 q7).
    ///
    /// **It is the arming of the rule and the evidence for it at once.** A
    /// frame here means this client sent a cancel for a turn that was still
    /// running, which is the only condition under which the specification's
    /// MUST applies; and it is the frame the annotation is read beside, because
    /// a notification is the one thing sent here that nothing will ever refer
    /// back to by id.
    cancelled: Arc<Mutex<Option<Frame>>>,
    /// The `session/load` this client is waiting on, and whether anything has
    /// been replayed into it — `None` whenever no load is in flight (§15 q7).
    ///
    /// The second rule's arming, and the same shape as the first's: a value
    /// here means this client asked an agent to load a session, which is the
    /// only condition under which the ordering MUST applies.
    loading: Arc<Mutex<Option<Loading>>>,
    /// The session setup call on the wire, and which of the three shapes its
    /// answer will arrive in — `None` whenever this client is not opening a
    /// session (§7.6).
    ///
    /// **Why the settings are read here rather than where the call is
    /// awaited.** An agent is free to announce a mode change in the breath
    /// after it answers, and the reader has that notification in hand before
    /// whoever asked for the session has woken up — so a caller writing the
    /// answer's settings afterwards would paint over an announcement that came
    /// later on the wire. Registered before the frame goes out, for the reason
    /// a turn registers its id before the prompt does: the answer is allowed to
    /// arrive before anyone here is looking for it.
    opening: Arc<Mutex<Option<Opening>>>,
    /// The `session/set_mode` on the wire, and what has crossed since it went
    /// out — `None` whenever this client is not setting a mode (§7.6).
    ///
    /// **Read where the frame is read, for [`opening`](Self::opening)'s
    /// reason.** An agent is free to announce the mode change in the breath
    /// before or after it answers, and the one task reading the connection has
    /// both in wire order; a caller writing the answer's outcome after it woke
    /// up would say *acknowledged and not restated* over an announcement that
    /// restated it.
    changing_mode: Arc<Mutex<Option<ChangingMode>>>,
    /// The `session/set_config_option` on the wire — `None` whenever this
    /// client is not setting one (§7.6).
    ///
    /// **Read where the frame is read**, for the reason
    /// [`changing_mode`](Self::changing_mode) is: the answer carries the whole
    /// option set, and an agent that restates its options in the breath before
    /// answering has said something the reader has already taken. Writing the
    /// answer from whoever awaited the call would put the two frames in the
    /// order the caller woke up in rather than the order they crossed.
    setting_option: Arc<Mutex<Option<SettingOption>>>,
    /// Which Advertisements each call on the wire is driving — empty whenever
    /// this client is driving none (§7.7).
    ///
    /// **The sixth id-keyed registration, and the one that keeps ACP out of the
    /// envelope.** `rpc` knows JSON-RPC and not ACP and must stay that way, so
    /// the map from an id to the capabilities its answer is about lives here,
    /// with the layer that knows what a method means. Registered *before* the
    /// frame goes out, the rule five things here already follow: the answer is
    /// allowed to arrive before anyone is looking for it.
    ///
    /// **A list rather than one capability**, because one call can drive two: a
    /// `session/load` carrying `additionalDirectories` drives the method it is
    /// and the field it carries, and both are answered by the same frame (§7.7).
    ///
    /// **Emptied by the teardown**, which is what stops a dying connection's
    /// late answer landing in the record — the mechanism the five slots above
    /// already rely on, rather than a connection ordinal on one store and not
    /// the others.
    driving: Arc<Mutex<HashMap<i64, Vec<AgentCapability>>>>,
    /// The first thing the agent said in each session on this connection.
    ///
    /// **The record that there was a conversation to replay**, which is the
    /// other half of what the load rule is decided from — and one frame per
    /// session rather than all of them, because the claim it supports is that
    /// the session was not empty, and one message is the whole of that. Sessions
    /// rather than the trace's whole record: a session id is one agent's, and the
    /// next connection's identical id is a different session.
    conversed: Arc<Mutex<HashMap<v1::SessionId, Frame>>>,
}

/// A `session/load` on the wire, and what has crossed since it went out.
struct Loading {
    /// The id its answer will arrive under, which is what says the answer is
    /// this load's.
    call: i64,
    /// The session it is loading. Only that session's updates are its replay —
    /// an agent talking about another one has replayed nothing here.
    session: v1::SessionId,
    /// The request frame, kept because the annotation is read beside it.
    request: Frame,
    /// Whether any `session/update` for that session has crossed since.
    ///
    /// Anything that crossed counts, whatever it was for: nothing on the wire
    /// tells a replayed message from one the agent would have sent anyway —
    /// the last words of a turn the switch just cancelled, say. Which can only
    /// make the rule quieter than it might have been, never wrong about an
    /// agent, and quieter is the right way for a rule decided from frames to
    /// fail.
    replayed: bool,
}

/// A `session/set_mode` on the wire, and what has crossed since it went out.
///
/// Named for the call rather than for the store it writes into, the way
/// [`Opening`] and [`Loading`] are — `session/set_config_option` has one of
/// these of its own ([`SettingOption`]), and *changing* on its own would be
/// ambiguous between them.
struct ChangingMode {
    /// The id its answer will arrive under, which is what says the answer is
    /// this set's.
    call: i64,
    /// The mode that was asked for. Kept because what a surface reports is the
    /// *ask* — the mode itself is only ever the agent's word.
    mode: v1::SessionModeId,
    /// Whether the agent has stated a mode since this went out.
    ///
    /// An agent that announced before it answered has confirmed the change,
    /// and an answer that then reported the set as unrestated would be
    /// contradicting a frame already read. Anything that crossed counts,
    /// whatever mode it named ([`SessionSettings::now_in`] says why).
    restated: bool,
}

/// A `session/set_config_option` on the wire.
///
/// **Nothing is watched for while it is out**, which is where this parts
/// company with [`ChangingMode`]: the answer carries the complete option set,
/// so there is nothing an announcement could settle and nothing left
/// unconfirmed to report. What is kept is the ask, because a refusal is about
/// the ask and the answer to a refused set says nothing about what was asked —
/// and because the third rule is decided from it (§15 q7).
struct SettingOption {
    /// The id its answer will arrive under, which is what says the answer is
    /// this set's.
    call: i64,
    /// The option that was asked about.
    option: v1::SessionConfigId,
    /// The value it was asked for, in the shape the write carried it.
    value: v1::SessionConfigOptionValue,
    /// The request frame, kept for [`Loading`]'s reason: the annotation is read
    /// beside the frames that decide it, and this is the half that says which
    /// option the answer owed.
    request: Frame,
}

/// A call that opens a session, on the wire.
struct Opening {
    /// The id its answer will arrive under, which is what says the answer is
    /// this call's.
    call: i64,
    how: Setup,
    /// The session it is opening, where that is known before the answer — which
    /// is every way of opening one but `session/new`, whose id the agent mints
    /// in the answer itself.
    ///
    /// What it is for is telling a *replay* from the traffic beside it: a call
    /// that reopens a session says which session it is asking for, so an
    /// announcement naming that session while the call is out is that session's
    /// own account of itself, and one naming the session being left is the last
    /// word of a conversation this client walked away from. Attributing the
    /// second to the session that is opening would be reporting an asymmetry
    /// the agent never produced.
    session: Option<v1::SessionId>,
    /// The mode the agent has stated for that session since the call went out,
    /// where it has stated one.
    ///
    /// The last one, because that is what the session's mode would be: an agent
    /// replaying several mode changes replayed a conversation that ended in the
    /// last of them.
    stated: Option<v1::SessionModeId>,
}

/// Which call is opening the session, and therefore how its answer reads.
///
/// **One thing to a reader and three types in the schema.** `session/new`,
/// `session/load` and `session/resume` answer with three different structs
/// carrying the same two optional fields, which is why the inspector's own
/// vocabulary has a word for what they have in common and the protocol does not
/// (`CONTEXT.md`, *Session setup response*).
#[derive(Clone, Copy)]
enum Setup {
    New,
    Restore(Restore),
}

impl Client {
    pub(crate) fn new(outgoing: FrameSink, stores: Stores) -> Self {
        Self {
            rpc: Rpc::new(outgoing),
            stores,
            awaiting: Arc::default(),
            cancelled: Arc::default(),
            loading: Arc::default(),
            opening: Arc::default(),
            changing_mode: Arc::default(),
            setting_option: Arc::default(),
            driving: Arc::default(),
            conversed: Arc::default(),
        }
    }

    /// `initialize`: protocol version 1, honest client capabilities, and who we
    /// are (§7.1).
    ///
    /// **The capabilities are what they are because they are true.** The
    /// inspector mediates no filesystem and embeds no terminal, so it
    /// advertises neither — which is the point of driving an agent with this
    /// tool at all: the agent's behaviour gets tested against what a minimal
    /// conformant client actually offers, and everything it calls anyway is a
    /// finding (§7.3). What it *does* claim, it honours: boolean config
    /// options, which it renders, and both elicitation modes, which it services
    /// (§7.8). [`client_capabilities`] is those claims and the argument for
    /// each of them.
    pub(crate) async fn initialize(&self) -> Result<v1::InitializeResponse, CallError> {
        let request = v1::InitializeRequest::new(ProtocolVersion::V1)
            .client_capabilities(client_capabilities())
            .client_info(client_info());

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
        let driving = drives_roots(&params);
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
    /// **One operation with a property, not two calls.** The two requests are
    /// carry the same inputs (resume omits an empty MCP list); their behavioral difference is
    /// [`Restore::replays`] — the ordering the specification states, and the
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
        driving.extend(drives_roots(&params));

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
    async fn end_session(
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
    async fn refresh_listing(&self) {
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
    async fn ended(&self) {
        // Everything, request-scoped elicitations included (§7.8): the timeline
        // that would have answered them is about to be discarded, and a debt
        // whose screen is gone cannot be paid.
        self.answer_cancelled(Debt::Everything).await;
        self.forget();
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
    fn forget(&self) {
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
    async fn leave(&self) {
        if self.stores.turn.get().is_running() {
            let _ = self.cancel().await;
        }
        // Whatever is still waiting: one that arrived beside the cancel, or one
        // from an agent that never started a turn to ask inside. Everything,
        // for [`ended`](Self::ended)'s reason — the view they are answered from
        // goes with the session.
        self.answer_cancelled(Debt::Everything).await;

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
    async fn answer_cancelled(&self, debt: Debt) {
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
    }

    /// `session/set_mode`: asks the agent to put the live session in a mode
    /// (§7.6).
    ///
    /// **Nothing is applied optimistically, so nothing is rolled back.** The
    /// mode on the surface is the agent's own last statement of it, and stays
    /// that whatever this call answers: an inspector that moved the row when it
    /// asked would be stating a configuration its agent never claimed, which is
    /// the one thing this tool must never do. What the answer produces is a
    /// [`ModeChange`](crate::ModeChange) beside it — an acknowledgement the
    /// agent never restated, or a refusal — written by the reader that sees the
    /// answer cross ([`changed_mode`](Self::changed_mode)).
    ///
    /// **Sent whenever there is a session to send it about**, including one
    /// with a turn in flight: the specification is silent on what an agent
    /// should do with a change it was not expecting, and an inspector that
    /// withheld the call would be deciding on the agent's behalf what may be
    /// observed (§7.6). Gating on the advertisement is the *surface's*, and it
    /// is the presence of published modes and nothing else.
    ///
    /// Registered before the frame goes out, for the reason a turn registers
    /// its id before the prompt does: the answer is allowed to arrive before
    /// anyone here is looking for it.
    pub(crate) async fn set_mode(&self, mode: &v1::SessionModeId) -> Result<(), CallError> {
        let session = self
            .session()
            .inspect_err(|error| self.mode_change_failed(None, mode, error))?;

        let call = self
            .rpc
            .prepare(
                v1::AGENT_METHOD_NAMES.session_set_mode,
                encode(&v1::SetSessionModeRequest::new(session, mode.clone())),
            )
            .inspect_err(|error| self.mode_change_failed(None, mode, error))?;
        let id = call.id();
        *locked(&self.changing_mode) = Some(ChangingMode {
            call: id,
            mode: mode.clone(),
            restated: false,
        });

        let outcome = self.rpc.send(call).await;
        if let Err(error) = &outcome {
            self.mode_change_failed(Some(id), mode, error);
        }
        // Nothing is read out of a successful answer: `session/set_mode`
        // answers `_meta` and nothing else, so what the answer is here is the
        // agent's word that it did what it was asked — and the whole point of
        // the ring is that its word is all there is. The frame is in the trace
        // either way (§8).
        outcome.map(|_| ())
    }

    /// `session/set_config_option`: asks the agent to set one of the live
    /// session's config options (§7.6).
    ///
    /// **The answer is the complete replacement option set, and the store takes
    /// it.** There is nothing to infer and nothing to compute here: the option
    /// set is rebuilt from the list the agent returned, whatever is in it —
    /// nothing merges, patches or assumes the option that was set is the only
    /// one that moved. Written by the reader that sees the answer cross
    /// ([`set_answered`](Self::set_answered)), for the reason the mode setter's
    /// outcome is.
    ///
    /// **The value carries its own shape, and the schema serializes each of
    /// them.** A select's goes out as a bare value id — no `type` discriminator
    /// over it — and a boolean's goes out as `"type":"boolean"` beside an
    /// unquoted `value`, because the discriminator describes the *shape of a
    /// value* rather than the kind of an option. Handing this call a
    /// [`v1::SessionConfigOptionValue`] rather than a value id is what makes
    /// that the schema's business instead of this crate's: a client that sent
    /// the one shape where the other was required is a mistake a real client
    /// made against a real agent, and the type is where it stops being possible.
    ///
    /// **The boolean shape exists on the wire only because this client claimed
    /// it** ([`client_capabilities`]): a conformant agent must not publish a
    /// boolean config option to a client that never advertised
    /// `clientCapabilities.session.configOptions.boolean`, so nothing here has
    /// a kind to write until that claim goes out.
    ///
    /// **Sent whenever there is a session to send it about**, including one
    /// with a turn in flight, and gated at the surface on the advertisement and
    /// nothing else — [`set_mode`](Self::set_mode) argues both, and the
    /// argument is the same one.
    pub(crate) async fn set_config_option(
        &self,
        option: &v1::SessionConfigId,
        value: &v1::SessionConfigOptionValue,
    ) -> Result<(), CallError> {
        let session = self
            .session()
            .inspect_err(|error| self.option_set_failed(None, option, value, error))?;

        let call = self
            .rpc
            .prepare(
                v1::AGENT_METHOD_NAMES.session_set_config_option,
                encode(&v1::SetSessionConfigOptionRequest::new(
                    session,
                    option.clone(),
                    value.clone(),
                )),
            )
            .inspect_err(|error| self.option_set_failed(None, option, value, error))?;
        let id = call.id();
        *locked(&self.setting_option) = Some(SettingOption {
            call: id,
            option: option.clone(),
            value: value.clone(),
            request: call.frame().clone(),
        });

        let outcome = self
            .rpc
            .send(call)
            .await
            // Read a second time by whoever asked, so that what this answers is
            // what the surface says: an answer the reader could not take is a
            // set that did not happen, and a caller told otherwise would be
            // told something the store disagrees with.
            .and_then(decode::<v1::SetSessionConfigOptionResponse>);
        if let Err(error) = &outcome {
            self.option_set_failed(Some(id), option, value, error);
        }
        outcome.map(|_| ())
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
    async fn list(
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

    /// `session/prompt`: ordered content blocks; each media kind is advertised.
    ///
    /// The turn is registered before the frame goes out, so the answer cannot
    /// arrive before the reader knows what it is an answer to.
    ///
    /// **A turn that fails says so in the store**, whether it failed on the way
    /// out, on the way back, or before it was a frame at all. The screen reads
    /// the turn rather than the call (§11 seam 2), so a prompt that quietly
    /// answered an error to whoever asked would be a composer whose button does
    /// nothing visible.
    ///
    /// **But only while the turn that failed is still the one on screen.** A
    /// prompt that was walked away from — the session switched, closed or
    /// deleted under it (§7.5) — answers its own caller long afterwards, and a
    /// failure written then would paint one session's turn over another's. So
    /// the guard is the id: `awaiting` is emptied by every way of leaving a
    /// session ([`forget`](Self::forget)), by the connection ending, and by the
    /// answer that settled the turn, and a failure whose id is no longer the
    /// awaited one says nothing here. The frame that carried it is in the trace
    /// either way (§8).
    ///
    /// A failure that happens *before* the frame goes out is nobody else's:
    /// nothing was registered, so nothing can have replaced it, and a composer
    /// whose button left no trace anywhere would be the screen keeping a secret
    /// the store is there to tell.
    pub(crate) fn submit_prompt(
        &self,
        content: Vec<v1::ContentBlock>,
    ) -> Result<tokio::task::JoinHandle<Result<v1::StopReason, CallError>>, CallError> {
        let session = self
            .session()
            .inspect_err(|error| self.turn_failed(None, error))?;
        let has_image = content
            .iter()
            .any(|block| matches!(block, v1::ContentBlock::Image(_)));
        let has_audio = content
            .iter()
            .any(|block| matches!(block, v1::ContentBlock::Audio(_)));
        let has_embedded = content
            .iter()
            .any(|block| matches!(block, v1::ContentBlock::Resource(_)));
        if content.iter().any(|block| {
            !(matches!(
                block,
                v1::ContentBlock::Text(_) | v1::ContentBlock::Image(_) | v1::ContentBlock::Audio(_)
            ) || matches!(block, v1::ContentBlock::Resource(resource) if match &resource.resource {
                v1::EmbeddedResourceResource::TextResourceContents(_) => true,
                v1::EmbeddedResourceResource::BlobResourceContents(blob) => blob.mime_type.as_deref() == Some("application/pdf"),
                _ => false,
            }))
        }) {
            return Err(CallError::UnsupportedPromptContent);
        }
        if has_image
            && !self
                .stores
                .agent
                .get()
                .is_some_and(|agent| agent.agent_capabilities.prompt_capabilities.image)
        {
            let error = CallError::ImageNotAdvertised;
            self.turn_failed(None, &error);
            return Err(error);
        }
        let request = v1::PromptRequest::new(session, content);
        if has_audio
            && !self
                .stores
                .agent
                .get()
                .is_some_and(|agent| agent.agent_capabilities.prompt_capabilities.audio)
        {
            let error = CallError::AudioNotAdvertised;
            self.turn_failed(None, &error);
            return Err(error);
        }
        let driven: Vec<_> = [
            (has_image, AgentCapability::Image),
            (has_audio, AgentCapability::Audio),
            (has_embedded, AgentCapability::EmbeddedContext),
        ]
        .into_iter()
        .filter_map(|(present, kind)| present.then_some(kind))
        .collect();

        if has_embedded
            && !self.stores.agent.get().is_some_and(|agent| {
                agent
                    .agent_capabilities
                    .prompt_capabilities
                    .embedded_context
            })
        {
            let error = CallError::EmbeddedContextNotAdvertised;
            self.turn_failed(None, &error);
            return Err(error);
        }

        let call = self
            .rpc
            .prepare_bounded(
                v1::AGENT_METHOD_NAMES.session_prompt,
                encode(&request),
                10 * 1024 * 1024,
            )
            .inspect_err(|error| self.turn_failed(None, error))?;
        let id = call.id();
        *locked(&self.awaiting) = Some(id);
        self.registers(id, &driven);
        // Nothing has been asked to stop *this* turn, so nothing is watching
        // it. A cancel still armed here belongs to a turn the user prompted
        // over — and that turn draws no annotation, because the connection is
        // still open and its answer may yet cross. If it does it lands as an
        // unsolicited entry (§8), which is the visible thing to have happened
        // and better than a claim that "never" had arrived.
        *locked(&self.cancelled) = None;
        self.stores.turn.set(TurnState::InFlight);
        // And the span the timeline stamps its entries with. Opened here rather
        // than where the state is read, because what belongs to a turn is
        // everything after the prompt went out — including the traffic that
        // arrives before this call returns.
        self.stores.timeline.open_turn();

        self.rpc.enqueue(&call).inspect_err(|error| {
            self.turn_failed(Some(id), error);
            self.drive_failed(Some(id), &driven, error);
        })?;
        let client = self.clone();
        Ok(tokio::spawn(async move {
            client.finish_prompt(call, driven).await
        }))
    }

    async fn finish_prompt(
        &self,
        call: crate::rpc::Call,
        driven: Vec<AgentCapability>,
    ) -> Result<v1::StopReason, CallError> {
        let id = call.id();
        let outcome = match self.rpc.wait(call).await {
            Ok(result) => decode::<v1::PromptResponse>(result).map(|response| response.stop_reason),
            Err(error) => {
                self.drive_failed(Some(id), &driven, &error);
                Err(error)
            }
        };
        if let Err(error) = &outcome {
            self.turn_failed(Some(id), error);
        }
        outcome
    }

    /// Says on screen that a turn failed, where the turn that failed is still
    /// the one the screen is about.
    ///
    /// `registered` is the id the turn went out under, or `None` for a failure
    /// that happened before it was a frame at all. The two are judged
    /// differently for the reason [`prompt`](Self::prompt) states: an unregistered
    /// failure is this call's alone, and a registered one belongs to whichever
    /// turn the store is about now.
    ///
    /// Where the answer *did* cross, the reader has already settled the turn
    /// from the frame ([`settle`](Self::settle)) and emptied `awaiting` doing
    /// it, so this says nothing over it — which is the same conclusion by the
    /// same rule, and why the turn state stays the stream's (§11 seam 2).
    fn turn_failed(&self, registered: Option<i64>, error: &CallError) {
        if let Some(id) = registered {
            let mut awaiting = locked(&self.awaiting);
            if *awaiting != Some(id) {
                return;
            }
            *awaiting = None;
        }
        self.stores.turn.set(TurnState::Failed(error.clone()));
        self.stores
            .timeline
            .close_turn(TurnOutcome::Failed(error.clone()));
    }

    /// `session/cancel`: a notification, then the answers it owes, and then a
    /// wait (§7.1).
    ///
    /// Cancelling does not end the turn — the agent does, by resolving the
    /// prompt with `cancelled`. What this changes is what the turn is *doing*,
    /// which is now waiting for an agent that has been asked to stop.
    ///
    /// **Every request the turn was blocked on is answered `cancelled`**, which
    /// the protocol requires of a client that cancels (§7.2) and which is also
    /// the only way the agent gets to stop: an agent waiting on a permission it
    /// will never be told about cannot end the turn it was asked to end. They
    /// are answered *after* the notification, so the agent knows why they are
    /// being cancelled by the time they are.
    pub(crate) async fn cancel(&self) -> Result<(), CallError> {
        let session = self.session()?;
        let sent = self
            .rpc
            .notify(
                v1::AGENT_METHOD_NAMES.session_cancel,
                encode(&v1::CancelNotification::new(session)),
            )
            .await?;

        // The turn's own: a request-scoped elicitation is tied to a call rather
        // than to a conversation (§7.8), so cancelling this turn is not a
        // sentence about it and its row is still there to answer.
        self.answer_cancelled(Debt::TheTurns).await;

        // Only if the turn is still open: an agent that answered while the
        // cancel was on its way has already ended the turn, and this is not
        // news that outranks it.
        //
        // **Arming the conformance rule is that same decision, so it is made
        // without letting go in between.** The reader task is the other
        // producer here (`settle`), and it takes these two in this same order,
        // so a cancel going out as its answer arrives resolves one way or the
        // other and never into both halves of neither: either the cancel arms
        // before the answer is judged, or it finds a turn already ended and
        // arms nothing. Two locks taken apart is exactly the gap
        // `Field::update` exists to close.
        let mut cancelled = locked(&self.cancelled);
        let open = self.stores.turn.update(|state| {
            let open = *state == TurnState::InFlight;
            if open {
                *state = TurnState::Cancelling;
            }
            open
        });
        if open {
            // A second cancel for a turn already cancelling leaves the first
            // one's frame in place: that is the one the agent was answering.
            *cancelled = Some(sent);
        }
        Ok(())
    }

    /// Everything the agent says, in one place.
    ///
    /// The four shapes a frame can arrive in, and what this client does with
    /// each: decode a `session/update` as v1, refuse a method it does not
    /// service, hand an answer to whoever asked, and *show* whatever is left
    /// (§8). No arm drops a frame.
    pub(crate) async fn receive(&self, received: Received) {
        let Received { frame, captured } = received;
        // The frame and the row it crossed as, paired for whichever store keeps
        // it: an entry is made of frames, and where the trace put each of them
        // is half of what makes the two screens one thing (§6.2).
        let recorded = |frame: Frame| Recorded { frame, captured };
        match classify(&frame) {
            Incoming::Notification { method, params } => {
                if method == v1::CLIENT_METHOD_NAMES.session_update {
                    // Before the decoding, and off the envelope rather than out
                    // of it: an update variant v1 has no name for is still the
                    // agent talking in that session, and the load rule is about
                    // what crossed rather than about what this client could read
                    // (§8).
                    self.heard(&params, &frame);
                    match crate::decode::from_value::<v1::SessionNotification>(params) {
                        Ok(notification) => {
                            // The settings before the timeline, for the reason
                            // the pending list goes before it: a screen woken
                            // by the timeline moving reads a surface that
                            // already accounts for what woke it.
                            self.reconfigured(&notification);
                            self.stores.timeline.update(notification, recorded(frame));
                        }
                        // An update variant v1 has no name for: an unstable-v1
                        // one, an extension, or v2 arriving early. It is shown,
                        // and the decoder's complaint is shown with it.
                        Err(problem) => self.stores.timeline.unrecognized(
                            Unrecognized::Undecodable {
                                method: Some(method),
                                problem: problem.to_string(),
                            },
                            recorded(frame),
                        ),
                    }
                } else if method == v1::CLIENT_METHOD_NAMES.elicitation_complete {
                    // The one notification that names something rather than
                    // reporting it (§7.8): a URL elicitation's out-of-band half
                    // finished, and the entry it names says so. A completion for
                    // an id nobody is holding is *ignored*, which the
                    // specification asks for and this tool spells as unsolicited
                    // traffic — shown, and acted on by nothing.
                    match crate::decode::from_value::<v1::CompleteElicitationNotification>(params) {
                        Ok(notification) => {
                            let completed = self
                                .stores
                                .outstanding
                                .completed(&notification.elicitation_id);
                            if completed.is_empty() {
                                self.stores
                                    .timeline
                                    .unrecognized(Unrecognized::Unsolicited, recorded(frame));
                            } else {
                                let named: Vec<RequestId> = completed
                                    .iter()
                                    .map(|request| request.id().clone())
                                    .collect();
                                self.stores.timeline.completed(&named, recorded(frame));
                            }
                        }
                        Err(problem) => self.stores.timeline.unrecognized(
                            Unrecognized::Undecodable {
                                method: Some(method),
                                problem: problem.to_string(),
                            },
                            recorded(frame),
                        ),
                    }
                } else {
                    self.stores
                        .timeline
                        .unrecognized(Unrecognized::NotServiced { method }, recorded(frame));
                }
            }

            // Two requests are serviced, and the first is the one an ACP turn is
            // made of (§7.2): `session/request_permission` becomes a pending
            // request where the turn stopped for it, and stays pending until
            // somebody answers it. Nothing is answered here — that is the point
            // of it.
            //
            // `fs/*` and `terminal/*` are declined, which is the capability
            // story exactly (§7.3): never advertised, so an agent calling one is
            // behaving badly at a client that says so out loud.
            Incoming::Request { id, method, params }
                if method == v1::CLIENT_METHOD_NAMES.session_request_permission =>
            {
                match crate::decode::from_value::<v1::RequestPermissionRequest>(params) {
                    Ok(request) => {
                        let request = PermissionRequest::new(
                            RequestId::new(id),
                            request,
                            Resolver::new(self.rpc.clone(), self.stores.timeline.ticker()),
                        );
                        // The list before the timeline: a screen woken by the
                        // timeline moving reads a pending list that already
                        // accounts for what woke it.
                        self.stores.permissions.arrived(request.clone());
                        self.stores.timeline.blocked(request, recorded(frame));
                    }
                    // A request for a method this client *does* service,
                    // carrying something v1 cannot read. It is shown like any
                    // other unrecognized frame (§8) — and refused as invalid
                    // params rather than method-not-found, because the method
                    // exists here and saying otherwise would be a lie about
                    // this client rather than a finding about the agent.
                    Err(problem) => {
                        self.stores.timeline.unrecognized(
                            Unrecognized::Undecodable {
                                method: Some(method),
                                problem: problem.to_string(),
                            },
                            recorded(frame),
                        );
                        let _ = self
                            .rpc
                            .respond(&id, Err(v1::Error::invalid_params()))
                            .await;
                    }
                }
            }

            // And the second is the one this ring made serviced (§7.8): an
            // elicitation becomes a pending request where the agent asked it,
            // in whichever of the two modes this client advertised. A mode it
            // did not advertise is refused with the error the specification
            // names for exactly that — and refused rather than *declined*,
            // because decline and cancel are answers about a reader and nothing
            // drew this one for one.
            Incoming::Request { id, method, params }
                if method == v1::CLIENT_METHOD_NAMES.elicitation_create =>
            {
                match crate::decode::from_value::<v1::CreateElicitationRequest>(params) {
                    Ok(request) => {
                        if advertised(&request.mode) {
                            let request = ElicitationRequest::new(
                                RequestId::new(id),
                                request,
                                &frame,
                                Resolver::new(self.rpc.clone(), self.stores.timeline.ticker()),
                            );
                            // A URL elicitation's id is registered before the
                            // entry is made, so that the annotation the reuse
                            // draws is read *below* the traffic it is about
                            // rather than above it.
                            let reused = request.elicitation_id().cloned().and_then(|elicited| {
                                self.stores
                                    .outstanding
                                    .arrived(elicited.clone(), request.clone(), frame.clone())
                                    .map(|first| (elicited, first))
                            });
                            // The list before the timeline, for the permission
                            // request's reason: a screen woken by the timeline
                            // moving reads a pending list that already accounts
                            // for what woke it.
                            self.stores.elicitations.arrived(request.clone());
                            self.stores
                                .timeline
                                .elicited(request, recorded(frame.clone()));
                            if let Some((elicited, first)) = reused {
                                self.stores.timeline.annotate(
                                    Annotation::ElicitationId(Reuse::Outstanding(elicited)),
                                    vec![first, frame],
                                );
                            }
                        } else {
                            // A mode outside the two this client claims. It
                            // decoded — the schema keeps an escape hatch for
                            // modes it has no name for — so this is a decision
                            // rather than a decode failure, and the frame is
                            // evidence like every refusal §7.3 already makes.
                            self.stores.timeline.unrecognized(
                                Unrecognized::Unadvertised {
                                    method,
                                    detail: spelt_mode(&request.mode),
                                },
                                recorded(frame),
                            );
                            let _ = self
                                .rpc
                                .respond(&id, Err(v1::Error::invalid_params()))
                                .await;
                        }
                    }
                    // Params v1 cannot read at all: shown, and refused as
                    // invalid params for the reason the permission request is —
                    // the method exists here.
                    Err(problem) => {
                        self.stores.timeline.unrecognized(
                            Unrecognized::Undecodable {
                                method: Some(method),
                                problem: problem.to_string(),
                            },
                            recorded(frame),
                        );
                        let _ = self
                            .rpc
                            .respond(&id, Err(v1::Error::invalid_params()))
                            .await;
                    }
                }
            }

            Incoming::Request { id, method, .. } => {
                self.stores
                    .timeline
                    .unrecognized(Unrecognized::NotServiced { method }, recorded(frame));
                let _ = self
                    .rpc
                    .respond(&id, Err(v1::Error::method_not_found()))
                    .await;
            }

            // Whether the id could be one of ours is the envelope's question,
            // asked once (`rpc::ours`) and answered for both of the things that
            // happen to an answer.
            Incoming::Answer { id, outcome } => match ours(&id) {
                // The stores first, the caller second: what the screen shows
                // must not depend on anyone being awake to await the call.
                Some(id) => {
                    self.note_login(&outcome);
                    self.drove(id, &outcome);
                    self.settle(id, &outcome, &frame);
                    self.loaded(id, &outcome, &frame);
                    self.opened(id, &outcome);
                    self.changed_mode(id, &outcome);
                    self.set_answered(id, &outcome, &frame);
                    if !self.rpc.answer(id, outcome) {
                        self.stores
                            .timeline
                            .unrecognized(Unrecognized::Unsolicited, recorded(frame));
                    }
                }
                None => self
                    .stores
                    .timeline
                    .unrecognized(Unrecognized::Unsolicited, recorded(frame)),
            },

            Incoming::Unreadable(problem) => self.stores.timeline.unrecognized(
                Unrecognized::Undecodable {
                    method: None,
                    problem,
                },
                recorded(frame),
            ),
        }
    }

    /// The connection is gone: nothing more will be answered, and a turn that
    /// was open is over however it looked a moment ago.
    ///
    /// Without this, an agent that dies mid-turn leaves a prompt awaiting an
    /// answer from a process that no longer exists, and a screen that still
    /// says the turn is running (§6.1: a transport that cannot deliver is
    /// evidence, never a hang).
    pub(crate) fn disconnected(&self) {
        self.rpc.close();
        *locked(&self.awaiting) = None;
        // A load that will never be answered decides nothing: the rule is about
        // what a load that *answers* did first, and this one did not answer.
        *locked(&self.loading) = None;
        // And a session that will never be opened publishes nothing.
        *locked(&self.opening) = None;
        // A set that will never be answered is refused by the teardown instead,
        // the way the turn below is ended by it: an answer nobody will ever
        // give is news, and a control left waiting on one would be the window
        // showing a question that has stopped being one.
        if let Some(changing) = locked(&self.changing_mode).take() {
            self.stores
                .settings
                .update(|settings| settings.mode_refused(changing.mode, CallError::Disconnected));
        }
        if let Some(setting) = locked(&self.setting_option).take() {
            self.stores.settings.update(|settings| {
                settings.option_refused(setting.option, setting.value, CallError::Disconnected)
            });
        }
        // And every advertisement still being driven is refused by the teardown
        // too (§7.7), for the reason the two setters above are: a connection
        // that died under a call in flight is where `AuthState` and both setters
        // already put a refusal, and a row left reading *not driven* about a
        // capability somebody drove would be the panel keeping a secret this
        // record exists to tell.
        //
        // **Emptying it is what closes the late-answer race**, which is why the
        // whole map goes rather than the ones a caller happens to be awaiting:
        // an answer that arrives after this has no registration to land in.
        for capability in std::mem::take(&mut *locked(&self.driving))
            .into_values()
            .flatten()
        {
            self.stores
                .driven
                .update(|record| record.refused(capability, CallError::Disconnected));
        }
        // Every request the agent left blocking is given up on: there is
        // nobody to answer to any more, and a panel that went on offering
        // buttons would be offering to send a frame into a closed pipe. Every
        // elicitation with it, whatever it was scoped to — a request-scoped one
        // outlives its turn and nothing outlives the connection it was asked on
        // (§7.8), which is the same sentence the outstanding ids answer to.
        self.stores.permissions.abandon();
        self.stores.elicitations.abandon();
        self.stores.outstanding.clear();
        self.end_turn(TurnState::Failed(CallError::Disconnected));
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
    fn note_login(&self, outcome: &Result<Value, v1::Error>) {
        if let Err(error) = outcome
            && error.code == v1::ErrorCode::AuthRequired
        {
            self.stores.auth.set(AuthState::Required(error.clone()));
        }
    }

    /// Settles the turn from the frame that ended it, when this answer is the
    /// one the turn was waiting for.
    fn settle(&self, id: i64, outcome: &Result<Value, v1::Error>, frame: &Frame) {
        {
            let mut awaiting = locked(&self.awaiting);
            if *awaiting != Some(id) {
                return;
            }
            *awaiting = None;
        }

        let resolved = match outcome {
            Ok(result) => match decode::<v1::PromptResponse>(result.clone()) {
                Ok(response) => TurnState::Ended(response.stop_reason),
                Err(error) => TurnState::Failed(error),
            },
            Err(error) => TurnState::Failed(CallError::Rejected(error.clone())),
        };

        // Whether a cancel was watching this turn, and the end of the turn
        // itself, under one hold and in the order `cancel` takes them: taken
        // exactly once, so an answer arriving as a cancel goes out is judged by
        // that cancel or by none, and never by the next turn's.
        let cancelled = {
            let mut cancelled = locked(&self.cancelled);
            let armed = cancelled.take();
            self.stores.turn.set(resolved.clone());
            armed
        };
        if let Some(cancel) = cancelled {
            self.judge(&resolved, cancel, Some(frame.clone()));
        }
        // After the annotation and not before it: a conformance annotation is a
        // finding *about* this turn, so it belongs inside the span rather than
        // in whatever arrives next.
        if let Some(outcome) = TurnOutcome::of(&resolved) {
            self.stores.timeline.close_turn(outcome);
        }

        // **A blocking request belongs to the turn it blocked** (§7.2): an
        // agent that ended its turn is not waiting on anything any more,
        // whatever it left unanswered. Given up on rather than quietly dropped,
        // so that the panel says nobody answered it instead of going on
        // offering buttons for a turn that is over.
        //
        // **Unless nothing tied it to the turn** (§7.8). An elicitation scoped
        // to a JSON-RPC call rather than to a session is not this turn's to give
        // up on — the agent may well still be waiting on it, its row survives
        // the turn boundary like every row, and the connection ending is what
        // ends it.
        self.stores.permissions.abandon();
        self.stores
            .elicitations
            .abandon_unless(|request| request.session_id().is_none());
    }

    /// Ends an open turn, and leaves a settled one alone: a turn the agent
    /// already ended keeps the reason the agent gave it.
    fn end_turn(&self, ended: TurnState) {
        let cancelled = {
            let mut cancelled = locked(&self.cancelled);
            let open = self.stores.turn.update(|state| {
                let open = state.is_running();
                if open {
                    *state = ended.clone();
                }
                open
            });
            cancelled.take().filter(|_| open)
        };

        if let Some(cancel) = cancelled {
            self.judge(&ended, cancel, None);
        }
        if let Some(outcome) = TurnOutcome::of(&ended) {
            self.stores.timeline.close_turn(outcome);
        }
    }

    /// Says so when a turn that was cancelled did not end the one way the
    /// specification allows — and says nothing at all when it did (§15 q7).
    ///
    /// **Only reached with a cancel in hand**, which is the admission rule made
    /// mechanical: the frame is proof this client asked an agent to stop a turn
    /// that was running, and the resolution is what the trace saw of the
    /// answer. Nothing here models what the agent was doing, and nothing here
    /// counts, scores or aggregates what it finds.
    ///
    /// Reading the resolution is [`Ending::of`]'s, beside the annotation it
    /// makes; what is left here is the client's own half — the frames the claim
    /// is read beside, and the store it goes in.
    fn judge(&self, resolved: &TurnState, cancel: Frame, answer: Option<Frame>) {
        let Some(ending) = Ending::of(resolved) else {
            return;
        };

        self.stores.timeline.annotate(
            Annotation::Cancellation(ending),
            std::iter::once(cancel).chain(answer).collect(),
        );
    }

    /// Says that a call which opens a session is on the wire (§7.6).
    ///
    /// A second one registered while the first is still out replaces it, and
    /// the first then publishes nothing: two sessions opening at once is one
    /// user pressing two buttons, and the surface belongs to whichever session
    /// ends up live. The same simplification the load rule makes about a second
    /// load.
    fn opening(&self, call: i64, how: Setup, session: Option<v1::SessionId>) {
        *locked(&self.opening) = Some(Opening {
            call,
            how,
            session,
            stated: None,
        });
    }

    /// Fills the settings from the session setup response that just crossed
    /// (§7.6).
    ///
    /// **The store's first source, written where the frame is read.** What the
    /// answer published is the whole of what the session that is opening is
    /// configured as, so it replaces rather than merges — and an announcement
    /// the agent makes after it is a later fact about the same session, which
    /// is why this is written in wire order rather than by whoever awaited the
    /// call.
    ///
    /// **A refused open published nothing**, and changes nothing: the session
    /// was not opened, so the surface still describes whichever one is live.
    ///
    /// **And the answer outranks whatever was replayed ahead of it**, which is
    /// the one case the ring's two rules collide in (§7.6). A `session/load`
    /// MUST replay before it answers, so a mode the agent stated on the way is
    /// a frame this reader has already taken — and an answer publishing no
    /// `modes` is the agent saying it offers none, which wins absolutely. What
    /// the replay is kept for is saying so
    /// ([`SessionSettings::unoffered_mode`]): the agent stated a mode change it
    /// did not offer, and that asymmetry is worth seeing.
    fn opened(&self, id: i64, outcome: &Result<Value, v1::Error>) {
        let Some(opening) = locked(&self.opening).take_if(|open| open.call == id) else {
            return;
        };

        let Ok(result) = outcome else {
            return;
        };
        self.stores
            .settings
            .set(opening.how.published(result.clone(), opening.stated));
    }

    /// Notes a mode the agent stated for the session that is opening (§7.6).
    ///
    /// **Only for the session the open names**, which is what makes this a
    /// reading of a replay rather than of whatever else is crossing: a
    /// `session/load` says which session it wants back, and the session being
    /// left is entitled to go on talking while it does — the last words of a
    /// turn the switch just cancelled, say. Attributing those to the session
    /// that is opening would report an asymmetry the agent never produced.
    ///
    /// **So a `session/new` records nothing**, because its session has no id
    /// until the answer mints one and nothing crossing before that can be shown
    /// to be about it. That is under-reporting rather than mis-reporting, which
    /// is the direction a rule decided from frames is allowed to fail in.
    ///
    /// Answers with whether the update was that session's, which is also the
    /// answer to whether the settings store should take it
    /// ([`reconfigured`](Self::reconfigured)).
    fn stated_while_opening(&self, session: &v1::SessionId, mode: &v1::SessionModeId) -> bool {
        let mut opening = locked(&self.opening);
        let Some(opening) = opening
            .as_mut()
            .filter(|opening| opening.session.as_ref() == Some(session))
        else {
            return false;
        };
        opening.stated = Some(mode.clone());
        true
    }

    /// Whether an update names the session a call still on the wire is opening.
    ///
    /// [`stated_while_opening`](Self::stated_while_opening) without the mode to
    /// remember: the other settings notification carries a whole option set,
    /// which the setup response is about to replace whatever it says, so there
    /// is nothing to keep and only the question of whose account it is.
    fn opening_now(&self, session: &v1::SessionId) -> bool {
        locked(&self.opening)
            .as_ref()
            .is_some_and(|opening| opening.session.as_ref() == Some(session))
    }

    /// Says what became of the `session/set_mode` this answer is the answer to
    /// (§7.6).
    ///
    /// **A success the agent never restated is the case the ring is about.**
    /// The answer carries nothing, and no rule obliges the agent to say
    /// anything afterwards, so what is recorded is the ask and its
    /// acknowledgement — and the mode on the surface stays the agent's own last
    /// statement of it. An agent that *did* announce, before answering, has
    /// already settled it: `restated` is that frame, read where it crossed, and
    /// nothing is recorded over it.
    fn changed_mode(&self, id: i64, outcome: &Result<Value, v1::Error>) {
        let Some(changing) = locked(&self.changing_mode).take_if(|change| change.call == id) else {
            return;
        };

        match outcome {
            // Already settled by a frame this reader saw first: the agent
            // announced the change before answering the set, so there is
            // nothing left unconfirmed to report.
            Ok(_) if changing.restated => {}
            Ok(_) => {
                self.stores
                    .settings
                    .update(|settings| settings.acknowledged(changing.mode));
            }
            // The refusal is written here rather than by whoever awaited the
            // call, for the reason the acknowledgement is: this is where the
            // frame is, and the store must not depend on anybody being awake.
            Err(error) => {
                self.stores.settings.update(|settings| {
                    settings.mode_refused(changing.mode, CallError::Rejected(error.clone()))
                });
            }
        }
    }

    /// Rebuilds the option set from the answer to the `session/set_config_option`
    /// this answer is the answer to (§7.6).
    ///
    /// **The agent's word, taken entirely.** The answer is the complete
    /// replacement set, so the store is rebuilt from it — an agent that answers
    /// a single write with an option set differing in more than the option that
    /// was written has that whole set reflected, because that is what it said
    /// about its own session.
    ///
    /// An answer this layer could not read is a set that did not happen as far
    /// as the surface goes: nothing is rebuilt from a list nobody could read,
    /// and a control that was pressed says what became of it rather than
    /// nothing. The frame is in the trace, complete (§8).
    ///
    /// **And the third rule is judged here** (§15 q7), because this is where
    /// both of the frames that decide it are: the set this client sent, and the
    /// answer that says it succeeded. The store first and the annotation
    /// second, so a screen woken by the timeline moving reads settings that
    /// already account for what woke it.
    fn set_answered(&self, id: i64, outcome: &Result<Value, v1::Error>, answer: &Frame) {
        let Some(setting) = locked(&self.setting_option).take_if(|set| set.call == id) else {
            return;
        };

        let read = match outcome {
            Ok(result) => decode::<v1::SetSessionConfigOptionResponse>(result.clone()),
            Err(error) => Err(CallError::Rejected(error.clone())),
        };
        match read {
            Ok(response) => {
                self.stores
                    .settings
                    .update(|settings| settings.answered(response.config_options));
            }
            Err(error) => {
                self.stores.settings.update(|settings| {
                    settings.option_refused(setting.option.clone(), setting.value, error)
                });
            }
        }

        // A set the agent *refused* owed no option list at all, which is the
        // rule's own boundary and not mercy: the MUST is on the answer to a set
        // that succeeded. What that answer carried is read from the frame
        // itself ([`OptionSet::of`]), whatever this client could decode of it.
        let Ok(result) = outcome else {
            return;
        };
        if let Some(short) = OptionSet::of(result, &setting.option) {
            self.stores.timeline.annotate(
                Annotation::OptionSet(short),
                vec![setting.request, answer.clone()],
            );
        }
    }

    /// Says beside the surface that a config option could not be set, where
    /// nobody answered the set at all (§7.6).
    ///
    /// [`mode_change_failed`](Self::mode_change_failed)'s rule, and the same
    /// pair of cases judged the same way: an unregistered failure is this
    /// call's alone, and a registered one says nothing where the reader has
    /// already said what the agent answered.
    fn option_set_failed(
        &self,
        registered: Option<i64>,
        option: &v1::SessionConfigId,
        value: &v1::SessionConfigOptionValue,
        error: &CallError,
    ) {
        if let Some(id) = registered
            && locked(&self.setting_option)
                .take_if(|set| set.call == id)
                .is_none()
        {
            return;
        }

        self.stores.settings.update(|settings| {
            settings.option_refused(option.clone(), value.clone(), error.clone())
        });
    }

    /// Says beside the surface that a mode could not be set, where nobody
    /// answered the set at all (§7.6).
    ///
    /// `registered` is the id it went out under, or `None` for a failure that
    /// happened before it was a frame — the two judged the way
    /// [`turn_failed`](Self::turn_failed) judges the same pair. An
    /// unregistered failure is this call's alone and nothing can have replaced
    /// it; a registered one says nothing where the reader has already said what
    /// the agent answered, because a set the agent answered is the agent's news
    /// and not the caller's.
    fn mode_change_failed(
        &self,
        registered: Option<i64>,
        mode: &v1::SessionModeId,
        error: &CallError,
    ) {
        if let Some(id) = registered
            && locked(&self.changing_mode)
                .take_if(|change| change.call == id)
                .is_none()
        {
            // The reader has already said what the agent answered, and a set
            // the agent answered is the agent's news rather than this caller's.
            return;
        }

        self.stores
            .settings
            .update(|settings| settings.mode_refused(mode.clone(), error.clone()));
    }

    /// Records what the agent answered a driven Advertisement with (§7.7).
    ///
    /// **Written where the frame is read**, beside the other answer-dispatch
    /// calls and for their reason: the record is what a panel shows, and a panel
    /// must not depend on anybody being awake to await the call.
    ///
    /// **Answered is read off the answer rather than off a decoding of it.** An
    /// agent that answered with something v1 could not read still answered; what
    /// this record is about is whether the advertisement was taken up and what
    /// came back, and the frame is in the trace, complete (§8).
    fn drove(&self, id: i64, outcome: &Result<Value, v1::Error>) {
        let Some(capabilities) = locked(&self.driving).remove(&id) else {
            return;
        };

        // One answer, every Advertisement the call drove: a `session/load`
        // carrying roots is answered on the load row and on the
        // `additionalDirectories` row by the same frame (§7.7).
        self.stores.driven.update(|record| {
            let mut moved = false;
            for capability in capabilities {
                moved |= match outcome {
                    Ok(_) => record.answered(capability),
                    // The agent's own error, whole: a client that paraphrased a
                    // refusal would be standing between the reader and the wire.
                    Err(error) => record.refused(capability, CallError::Rejected(error.clone())),
                };
            }
            moved
        });
    }

    /// Says which Advertisements the call going out under `id` is driving
    /// (§7.7), where it drives any.
    ///
    /// Nothing is registered for a call that drives none — `session/new` without
    /// roots is one, and so is the listing the tool refreshes on its own
    /// initiative — because an entry the record has nothing to say about is one
    /// the teardown would sweep and one `drove` would find and do nothing with.
    fn registers(&self, id: i64, driving: &[AgentCapability]) {
        if !driving.is_empty() {
            locked(&self.driving).insert(id, driving.to_vec());
        }
    }

    /// Says in the record that a driven Advertisement got no answer at all
    /// (§7.7).
    ///
    /// `driven` is what the call was driving, and an empty list writes nothing —
    /// because a record of what was driven has nothing to say about a call that
    /// drove none of it.
    ///
    /// `registered` is the id it went out under, or `None` for a failure that
    /// happened before it was a frame — the two judged the way
    /// [`mode_change_failed`](Self::mode_change_failed) judges the same pair. An
    /// unregistered failure is this call's alone and nothing can have replaced
    /// it; a registered one says nothing where the reader has already said what
    /// the agent answered, or where the teardown has already refused it on the
    /// way out.
    fn drive_failed(&self, registered: Option<i64>, driven: &[AgentCapability], error: &CallError) {
        if driven.is_empty() {
            return;
        }
        if let Some(id) = registered
            && locked(&self.driving).remove(&id).is_none()
        {
            return;
        }

        // Every one of them, and never short-circuited: a call that drove two
        // Advertisements is refused on both rows or the second would be left
        // reading *not driven* about an ask that was made.
        self.stores.driven.update(|record| {
            let mut moved = false;
            for capability in driven {
                moved |= record.refused(*capability, error.clone());
            }
            moved
        });
    }

    /// Reads a settings change out of an update that is one (§7.6).
    ///
    /// Two of the eleven stable variants say what the live session is
    /// configured as: `current_mode_update` names the mode it is now in, and
    /// `config_option_update` restates the option set whole. Both are the
    /// store's third source, and neither *advertises* anything — that is the
    /// setup response's to do, which is why a mode announced by an agent that
    /// published no modes leaves this store alone
    /// ([`SessionSettings::now_in`]).
    ///
    /// **The timeline keeps them either way.** The surface says what a setting
    /// is now; the timeline is where *when* it changed stays on the record, and
    /// this reads the update on the way past rather than in place of it.
    ///
    /// **And a replay is not one of them.** An announcement naming the session
    /// a `session/load` is still asking for is that session's account of
    /// itself, and the store does not describe that session yet — what it
    /// describes is whichever one is live until the answer arrives. Writing the
    /// replay here would put the opening session's mode on the row of the
    /// session being left, and a load the agent then *refused* would leave it
    /// there for good: a session showing a mode it never stated. So the replay
    /// is left to the answer, which rebuilds the store from the setup response
    /// whole — the ordering rule reached from the other side (§7.6). The mode
    /// it named is kept where a refused set's is, beside the surface
    /// ([`SessionSettings::unoffered_mode`]), and the frame is on the timeline
    /// and in the trace either way (§8).
    fn reconfigured(&self, notification: &v1::SessionNotification) {
        match &notification.update {
            v1::SessionUpdate::CurrentModeUpdate(announced) => {
                // A set still on the wire has been answered by the agent's own
                // announcement, whichever order the two arrive in: an agent
                // that states a mode has restated it, and the answer that comes
                // next must not report the set as unrestated over the top of a
                // frame already read.
                if let Some(changing) = locked(&self.changing_mode).as_mut() {
                    changing.restated = true;
                }
                if self.stated_while_opening(&notification.session_id, &announced.current_mode_id) {
                    return;
                }
                self.stores
                    .settings
                    .update(|settings| settings.now_in(&announced.current_mode_id));
            }
            v1::SessionUpdate::ConfigOptionUpdate(restated) => {
                if self.opening_now(&notification.session_id) {
                    return;
                }
                self.stores
                    .settings
                    .update(|settings| settings.offers(restated.config_options.clone()));
            }
            _ => {}
        }
    }

    /// Notes that the agent said something in a session (§15 q7).
    ///
    /// Two questions, both answered from the same frame: whether a load in
    /// flight has had anything replayed into it, and whether this connection
    /// has ever heard the agent talk in that session at all — which is what
    /// says there was a conversation for a load to replay.
    ///
    /// The session id is read off the envelope rather than out of a decoded
    /// notification: an update this client cannot decode is still the agent
    /// talking, and a rule about what crossed must not turn on what the typed
    /// layer recognized (§8). An update naming no session names none, and
    /// decides nothing.
    fn heard(&self, params: &Value, frame: &Frame) {
        let Some(session) = params
            .get("sessionId")
            .and_then(Value::as_str)
            .map(v1::SessionId::new)
        else {
            return;
        };

        if let Some(loading) = locked(&self.loading)
            .as_mut()
            .filter(|loading| loading.session == session)
        {
            loading.replayed = true;
        }
        // The first one only: the claim it is kept for is that the session was
        // not empty, and one message is the whole of that.
        locked(&self.conversed)
            .entry(session)
            .or_insert_with(|| frame.clone());
    }

    /// Says so when a `session/load` answered before the conversation had been
    /// replayed — and says nothing at all where the frames cannot decide it
    /// (§15 q7).
    ///
    /// **Only reached with a load in hand**, which is this rule's half of the
    /// admission rule: the frame is proof this client asked an agent to load a
    /// session, which is the only condition under which the ordering MUST
    /// applies.
    ///
    /// Two things draw nothing, and both are the boundary rather than mercy. A
    /// load the agent *refused* restored no session, so there was no
    /// conversation it owed anybody — the MUST is on a load that answers. And a
    /// load of a session this connection has never heard a word in replayed
    /// nothing into a session nothing is known to have been said in, which is
    /// the shape the trace cannot tell from an empty session; an inspector that
    /// annotated there would be inventing a rule rather than reporting one.
    fn loaded(&self, id: i64, outcome: &Result<Value, v1::Error>, answer: &Frame) {
        let Some(loading) = locked(&self.loading).take_if(|load| load.call == id) else {
            return;
        };

        if outcome.is_err() || loading.replayed {
            return;
        }
        let Some(said) = locked(&self.conversed).get(&loading.session).cloned() else {
            return;
        };

        // Chronologically, which is how the evidence reads: the agent talking in
        // that session, the load that asked for it back, and the answer that
        // came without it.
        self.stores.timeline.annotate(
            Annotation::Replay(Replay::Nothing),
            vec![said, loading.request, answer.clone()],
        );
    }

    async fn call<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: &impl Serialize,
    ) -> Result<T, CallError> {
        decode(self.rpc.call(method, encode(params)).await?)
    }

    /// Asks something that drives an Advertisement, and leaves the ask on the
    /// record (§7.7).
    ///
    /// **Prepared, registered, and only then sent** — the rule stated in five
    /// places here, and the reason [`call`](Self::call) is not enough for these:
    /// the answer is allowed to arrive before anyone is looking for it, so the
    /// id has to mean something by the time the frame is on the wire. The split
    /// allocates the id exactly where the one-shot call allocated it and changes
    /// no wire ordering, which the fixtures that answer by counting reads depend
    /// on (`crates/core/tests/capability.rs`).
    ///
    /// `driving` is the capability the call is about, or `None` for one this
    /// record does not key on — the listing the tool refreshes on its own
    /// initiative, which sends the same frame and writes nothing.
    ///
    /// **What an answer does is not here**: it is [`drove`](Self::drove)'s, at
    /// the reader, because a panel must not depend on anybody being awake to
    /// await the call. What is left for the caller is the failure that never
    /// became a frame an agent could answer.
    ///
    /// [`restore`](Self::restore) prepares and registers by hand rather than
    /// through this, and has to: it gives up the live session between the
    /// registration and the send, and the registration has to be the earlier of
    /// the two ([`restore`](Self::restore) says why).
    async fn drive(
        &self,
        method: &str,
        params: Value,
        driving: Option<AgentCapability>,
    ) -> Result<Value, CallError> {
        let driving: Vec<_> = driving.into_iter().collect();
        let call = self
            .rpc
            .prepare(method, params)
            .inspect_err(|error| self.drive_failed(None, &driving, error))?;
        let sent = call.id();
        self.registers(sent, &driving);

        let outcome = self.rpc.send(call).await;
        if let Err(error) = &outcome {
            // A call that never became a frame the agent could answer has no
            // reader event, so the record is written by whoever awaited it —
            // under the id guard the turn and the two setters already use.
            self.drive_failed(Some(sent), &driving, error);
        }
        outcome
    }

    fn session(&self) -> Result<v1::SessionId, CallError> {
        self.stores.session.get().ok_or(CallError::NoSession)
    }
}

impl Setup {
    /// The settings a session setup response published (§7.6).
    ///
    /// Three arms because the schema gives three types, exactly as
    /// [`Client::restore`]'s request does: the responses carry the same two
    /// optional fields, and which type they are read as is the whole of what
    /// differs.
    ///
    /// **An answer this layer could not read published nothing.** The session
    /// was opened — the agent answered, and that answer is what made it live —
    /// so a decoding failure here is this client failing to recognize
    /// something rather than a session that did not open, and it costs the
    /// surface rather than the call. The frame is in the trace, complete (§8).
    ///
    /// **And an answer nobody could read says nothing about a replay either**,
    /// which is what falling back to the default settings leaves: whether the
    /// agent offered the mode it stated is a question about a field in an
    /// answer this layer could not get to, so the surface reports what it has
    /// rather than a disagreement it cannot show.
    fn published(self, result: Value, stated: Option<v1::SessionModeId>) -> SessionSettings {
        // The two fields, off whichever of the three types this answer is. What
        // is done with them is said once below, because it is one rule: which
        // call published them changes nothing about what they mean.
        let read = match self {
            Self::New => decode::<v1::NewSessionResponse>(result)
                .map(|answer| (answer.modes, answer.config_options)),
            Self::Restore(Restore::Load) => decode::<v1::LoadSessionResponse>(result)
                .map(|answer| (answer.modes, answer.config_options)),
            Self::Restore(Restore::Resume) => decode::<v1::ResumeSessionResponse>(result)
                .map(|answer| (answer.modes, answer.config_options)),
        };
        read.map(|(modes, config_options)| {
            SessionSettings::published(modes, config_options, stated)
        })
        .unwrap_or_default()
    }
}

/// Which Advertisement one of the two ways of reopening a session drives
/// (§7.7).
///
/// **One place, because it is one decision** — and one both this client and the
/// [`Inspector`](crate::Inspector) have to make: the caller writes the record
/// where there was no client to ask at all, which is the shape `authenticate`
/// and the two setters already have.
///
/// **One operation with a property, and two Advertisements** ([`Restore`]). An
/// agent claims `loadSession` and `sessionCapabilities.resume` separately and
/// may claim either alone, so what was driven is whichever call the property
/// selected — writing a resume's outcome on the load row would be the panel
/// reporting an answer about a call that never went out.
pub(crate) fn drives(how: Restore) -> AgentCapability {
    match how {
        Restore::Load => AgentCapability::Load,
        Restore::Resume => AgentCapability::Resume,
    }
}

/// Whether a session-opening call drives `additionalDirectories`, **read off
/// the frame that is about to cross** (§7.7).
///
/// The field is omitted when the list is empty, so a request that carries it
/// asked for roots and one that does not asked for none — which is why this asks
/// the encoded parameters rather than the list this client assembled, or the
/// intent behind it. *What crossed must not turn on what this client
/// recognized* is the rule the incomplete-option-set annotation already reads
/// its evidence by.
///
/// A list because that is what a call registers: the reopen adds it to the
/// Advertisement the call itself drives, and `session/new` drives this or
/// nothing.
fn drives_roots(params: &Value) -> Vec<AgentCapability> {
    params
        .get(AgentCapability::AdditionalDirectories.name())
        .map(|_| AgentCapability::AdditionalDirectories)
        .into_iter()
        .collect()
}

/// A request's parameters as JSON.
///
/// Infallible in practice and treated as such: these are the schema crate's own
/// types, and a type that cannot serialize itself is a bug in this program
/// rather than something an agent did — the failure the inspector must never
/// invent is one it blames on the agent.
pub(crate) fn encode(params: &impl Serialize) -> Value {
    serde_json::to_value(params).unwrap_or(Value::Null)
}

/// An answer, read as v1.
///
/// A result that will not decode is not an error the agent reported — it is
/// this layer failing to recognize something — so it is named as such and the
/// frame stays in the trace to be read by a human (§8).
fn decode<T: serde::de::DeserializeOwned>(result: Value) -> Result<T, CallError> {
    crate::decode::from_value(result).map_err(|problem| CallError::Undecodable(problem.to_string()))
}
