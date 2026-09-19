//! The capability/auth display (`docs/architecture.md` §9): the agent's own
//! account of itself, and the affordances that hang off it.
//!
//! **Everything here is read from the `initialize` result** — who the agent
//! says it is, what it says it can do, how it says it can be logged into — and
//! nothing is inferred from what has happened since. That is the gating rule
//! the spec states and this panel obeys: the `session/list` button appears
//! because the agent *advertised* listing, not because there is anything to
//! list, and it disappears again only when another agent claims something else.
//! Whether a capability was advertised is core's question
//! ([`Inspector::lists_sessions`](acp_inspector_core::Inspector::lists_sessions));
//! the claim and its Affordance render the same answer.
//!
//! **`session/new` is the affordance that gates on nothing**, because the
//! protocol gates it on nothing (§7.5): every v1 agent opens sessions, so it is
//! offered whenever there is an agent to ask — which is what lets a second
//! session be opened without going back through the spawn form. What opening
//! one costs is stated where the button is, because it is a switch and a switch
//! is not free: [`Inspector::new_session`](acp_inspector_core::Inspector::new_session)
//! is where it is argued and where it happens.
//!
//! **A listed session is a way into that session and a way out of it** (§7.5):
//! the row carries one affordance for opening it — replay is a property of the
//! call, not two buttons — and one for each of the two ways of ending it, close
//! and delete, which are two operations behind two claims. Every one of them
//! gates on the advertisement alone: the corners the specification leaves
//! undefined are offered rather than withheld, and what the agent does in them
//! is what a reader came here to find out. A listing row is *where* a session
//! the agent holds is named, so an agent that advertises close or delete and
//! not `session/list` has both claims displayed above and no row to press them
//! on — a consequence of where the affordance lives, recorded in §7.5 rather
//! than papered over with a second place to name a session.
//!
//! **The session lifecycle is shown whole** (§7.5): all six capabilities v1
//! defines, each stating whether this agent advertised it, and the two shapes
//! the claims are made in kept apart rather than flattened. Agents disagree
//! sharply here — of three surveyed from source, one implements all six
//! lifecycle methods, one five, and one only `session/load` — so what an agent
//! left out is as much of an answer as what it claimed, and a row it never
//! advertised is displayed rather than omitted — and displayed at the weight
//! and the contrast of one it did advertise, because the half of the answer
//! that is an absence is not an aside (#83). Which half a row is reads off its
//! weight on screen, and off the words the row says either way for a reader
//! who is listening.
//!
//! **Both parties are audited, not one** (§7.6, #86). Beside what the agent
//! advertised stands what *this client* advertised — one claim today, support
//! for boolean config options — because an agent that gates on that claim
//! offers this tool a different thing than it offers a client without it, and
//! without the claim on screen the option shape a reader is looking at has no
//! visible cause. The two accounts are drawn by one function from one list
//! markup, so neither is the other's footnote, and they are kept apart rather
//! than merged into a single list of "capabilities": who claimed a thing is the
//! point of showing it. This client's own block does not wait for an agent —
//! the claim is fixed for every connection and rides the `initialize` *request*
//! ([`client_capabilities`](acp_inspector_core::client_capabilities)), so it is
//! knowable before there is an agent and unchanged by anything one answers.
//!
//! **Authentication is display-only** (§1.1). The panel shows the advertised
//! methods, sends `authenticate` with the one the user picks, and renders
//! `-32000 auth_required` as what it is: the agent saying it will not go on
//! until somebody logs in. It says so *and* says where — the user's own
//! terminal — because every practical login is a flow the agent runs itself,
//! and an inspector that implied it could do it here would be promising a
//! terminal it deliberately does not embed.
//!
//! **And `logout` sits with those methods** (§7.7), because the call carries no
//! session id and its place on screen is its scope on the wire. It is gated on
//! the advertisement and on nothing else — offered with nobody logged in, since
//! that corner is one the specification leaves entirely undefined and driving it
//! is the point — and it leaves the live session strictly alone, because what
//! the agent does to a session it has logged out of is the finding rather than
//! something for this window to pre-empt.
//!
//! **`authenticate` sits with them too, and draws its fourth fact from the auth
//! state** (§7.7). It is the one row that does not ask core's record, because
//! core's record has no entry for it: [`AuthState`] has held not-driven,
//! accepted-with-a-method and refused-with-the-agent's-own-error since the MVP,
//! and a second store holding the same three things would be two things to
//! disagree about one login. The block below the rows reads the same value, so
//! the two cannot say different things about the same login — including where a
//! connection died under an `authenticate` in flight, which that state has
//! always called a refusal.
//!
//! **A row nothing here can reach draws no outcome and says why** (§7.7). *Not
//! driven* on such a row is a sentence about the agent when the true sentence is
//! about this tool, and a reader who takes it at face value goes hunting for a
//! button that never existed. Which rows those are is a fact about where the
//! affordances live rather than a state a store holds, so it is decided in
//! [`advertised`] beside the rows: `session/resume` where load is advertised too
//! and preferred, `session/close` and `session/delete` where no listing names a
//! session to press them on, and the three prompt content shapes and two MCP
//! transports this tool has no affordance for on any agent. Where a whole run of
//! rows shares the one reason — those last two, and only those — the run says it
//! once above them and the rows say nothing, because a rail 280px wide carrying
//! one sentence about this tool three times over is what makes it read as three
//! facts about the agent. And a row the agent never advertised says nothing
//! about driving at all — there is no claim to have driven, and the row's answer
//! is the *not advertised* it already gives.

use acp_inspector_core::{
    AgentCapability, AuthState, CallError, Cursor, Driven, DrivenRecord, Restore, Roots,
    SessionListing, v1,
};
use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::hi_outline_icons::{HiPlus, HiRefresh},
};

/// A lifecycle call that did not do what it said, and which call it was.
///
/// **The method, not a paraphrase of the effect.** Four affordances sit
/// together here and every one of them can fail on its own — an agent that
/// advertised `session/close` and then refused it is a finding about that
/// method (§7.5), and a line reading "it did not work" would have thrown away
/// the half of it that is evidence.
#[derive(Clone, Debug, PartialEq)]
pub struct Failed {
    /// The method that failed, by its name on the wire.
    pub method: &'static str,
    /// What the agent said, or the news that there was nobody to ask.
    pub error: CallError,
}

impl Failed {
    /// Reads a call's outcome as something to say, or as nothing to say.
    ///
    /// One shape for all four affordances, because the four handlers say the
    /// same sentence about different methods: `outcome.err().map(Failed::of(…))`
    /// at each of them, rather than the same four lines written four times.
    pub fn of(method: &'static str) -> impl Fn(CallError) -> Self {
        move |error| Self { method, error }
    }
}

/// A listed session, and the roots it is to be reopened with (§7.7).
///
/// **The two halves of one click**: which session the row is about, as the agent
/// described it, and what the control beside the button holds. `None` where the
/// agent advertised no `additionalDirectories` and there is therefore no control
/// — which is core's cue to send the roots the listing reported, because a
/// reopen goes on being a reopen whatever was advertised.
#[derive(Clone, Debug, PartialEq)]
pub struct Opening {
    /// The session to reopen, handed back as the agent described it.
    pub session: v1::SessionInfo,
    /// The roots to open it with, or `None` for the ones the listing reported.
    pub roots: Option<Roots>,
}

/// One authentication call currently in flight from the Agent panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthenticationCall {
    Authenticate(v1::AuthMethodId),
    Logout,
}

/// The two accounts, as the centre screen draws them (§9).
///
/// **They left the rail because they are most of it.** Measured on a populated
/// window they were 1430px of a 1994px rail in a 568px viewport, against four
/// hundred pixels for everything a reader presses — so the panel that exists to
/// drive an agent put its own specification between the reader and its buttons.
/// Collapsing them behind a summary bought the vertical space and none of the
/// width: a capability row is a name, the field it was claimed in, whether it
/// was advertised and what became of driving it, which is more than a 288px rail
/// can put on one line.
///
/// Here they have the centre's width, so a row is a row and the two accounts can
/// be read against each other — which is what having both is for (§7.6): an
/// agent that degrades what it offers for a client that claimed less is doing
/// something whose cause is on this screen and nowhere else.
#[derive(Clone, PartialEq)]
pub struct Claims {
    /// The live Session, which the listing reads to name its current row.
    pub session: Option<v1::SessionId>,
    pub auth_pending: Vec<AuthenticationCall>,
    pub listing: Option<SessionListing>,
    pub listing_pending: bool,
    pub restores: Option<Restore>,
    pub closes: bool,
    pub deletes: bool,
    pub opens_with_roots: bool,
    pub connected: bool,
    pub failed: Option<Failed>,
    pub logs_out: bool,
    pub on_authenticate: EventHandler<v1::AuthMethodId>,
    pub on_logout: EventHandler<()>,
    pub on_new_session: EventHandler<Option<Roots>>,
    pub on_open: EventHandler<Opening>,
    pub on_close: EventHandler<v1::SessionId>,
    pub on_delete: EventHandler<v1::SessionId>,
    pub on_list: EventHandler<Option<Cursor>>,
    /// The `initialize` result, or `None` until an agent has answered one.
    pub agent: Option<v1::InitializeResponse>,
    /// What this client claimed about itself, and who it said it was — core's
    /// own values, so the screen and the wire cannot say different things.
    pub client: v1::ClientCapabilities,
    pub client_info: v1::Implementation,
    /// What became of driving each advertisement on this connection (§7.7).
    pub driven: DrivenRecord,
    /// Where authentication stands, which the `authenticate` row reads its
    /// fourth fact from.
    pub auth: AuthState,
    /// Whether the agent advertised listing — core's answer, which decides
    /// whether two lifecycle rows have an affordance that can reach them.
    pub lists_sessions: bool,
}

impl Claims {
    /// What this agent said about sessions, and the four calls that act on
    /// them, as one value.
    ///
    /// Built here rather than by the surface that draws it: which of these
    /// fields are about sessions is this type's own business, and a dialog
    /// assembling them itself would be a second place that has to be right.
    pub(crate) fn listing(&self) -> Listing<'_> {
        Listing {
            advertised: self.lists_sessions,
            answered: self.listing.as_ref(),
            live: self.session.as_ref(),
            pending: self.listing_pending,
            restores: self.restores,
            closes: self.closes,
            deletes: self.deletes,
            roots: self.opens_with_roots,
            on_list: self.on_list,
            on_open: self.on_open,
            on_close: self.on_close,
            on_delete: self.on_delete,
        }
    }

    /// The screen's inputs with the interactive half left quiet.
    ///
    /// For a test or a mock whose question is about what is *drawn*: every
    /// handler is a no-op and every affordance is present, because a screen
    /// that hid its controls would answer a different question from the one
    /// being asked.
    #[cfg(test)]
    pub fn about(agent: Option<v1::InitializeResponse>) -> Self {
        Self {
            agent,
            client: acp_inspector_core::client_capabilities(),
            client_info: acp_inspector_core::client_info(),
            driven: DrivenRecord::default(),
            auth: AuthState::default(),
            lists_sessions: true,
            session: None,
            auth_pending: Vec::new(),
            listing: None,
            listing_pending: false,
            restores: Some(Restore::Load),
            closes: true,
            deletes: true,
            opens_with_roots: true,
            connected: true,
            failed: None,
            logs_out: true,
            on_authenticate: EventHandler::new(move |_| {}),
            on_logout: EventHandler::new(move |()| {}),
            on_new_session: EventHandler::new(move |_| {}),
            on_open: EventHandler::new(move |_| {}),
            on_close: EventHandler::new(move |_| {}),
            on_delete: EventHandler::new(move |_| {}),
            on_list: EventHandler::new(move |_| {}),
        }
    }
}

/// One advertiser's account of itself, on a screen of its own.
///
/// **Two screens rather than two columns.** They were drawn side by side, which
/// is what §9's *beside* asked for while both were in a rail; on the centre
/// screen it meant each got half the width and the agent's account — the one
/// that changes, the one a reader came for — shared its screen with a block that
/// is the same on every launch of every agent. They are two views in one
/// tablist now: still never merged, still one click apart, and each with the
/// whole width for its rows.
#[component]
pub fn AgentClaims(claims: Claims, whose: Advertiser) -> Element {
    // Only the half of `Claims` this screen draws. The other half is about
    // sessions, and is drawn by the dialog the Timeline's header opens
    // (`crate::session`) — this screen is what the agent *said*.
    // Which rows of whichever account is on screen (`Only`). A hook, so it is
    // taken before the branch below; window-session state like the Console's
    // own filter, and reset by moving between the two accounts, because a
    // narrowing is a question about the account it was asked of.
    let only = use_signal(Only::default);
    let Claims {
        agent,
        client,
        client_info,
        driven,
        auth,
        lists_sessions,
        auth_pending,
        connected,
        logs_out,
        on_authenticate,
        on_logout,
        ..
    } = claims;

    rsx! {
        // No `data-advertiser` here: the account inside already carries one,
        // and a second on the wrapper is the same name meaning two things — a
        // reader looking for the agent's block would find this and stop.
        div { class: "rail-group claims", "data-slot": "claims",
            match whose {
                // **This client's own claim does not wait for an agent**,
                // because it does not depend on one: it is fixed for every
                // connection and goes out in the request rather than coming back
                // in the answer (§7.6). Read before a launch, it is what the
                // next agent will be told before it is told.
                Advertiser::Client => rsx! {
                    div { class: "rail-group account",
                        {identity(Advertiser::Client, Some(&client_info), rsx! {}, rsx! {})}
                        {advertises(Advertiser::Client, claimed(&client), only)}
                    }
                },
                Advertiser::Agent => match &agent {
                    None => rsx! {
                        p { class: "empty",
                            "What the agent says about itself — its name, its capabilities, and how it can be logged into — appears here as it answers "
                            code { "initialize" }
                            "."
                        }
                        // An agent that demands a login before it will say who
                        // it is has not read the spec, and is exactly the sort
                        // of agent this tool is pointed at.
                        if auth.needs_login() {
                            div { class: "rail-group account",
                                {login(&[], &auth, &auth_pending, logs_out, connected, on_authenticate, on_logout)}
                            }
                        }
                    },
                    Some(agent) => rsx! {
                        div { class: "rail-group account",
                            {identity(
                                Advertiser::Agent,
                                agent.agent_info.as_ref(),
                                rsx! {
                                    span { class: "badge badge-sm", "ACP v{agent.protocol_version}" }
                                },
                                standing(&auth),
                            )}
                            // Only where there is a login to draw. An agent
                            // that advertised no method, has been asked for
                            // none and offers no `logout` has nothing here, and
                            // a heading introducing that was the loudest way to
                            // say nothing on the screen. What it claimed about
                            // auth is in the rows below either way (§7.7).
                            if !agent.auth_methods.is_empty() || logs_out || auth.needs_login()
                                || !auth_pending.is_empty()
                            {
                                {login(&agent.auth_methods, &auth, &auth_pending, logs_out, connected, on_authenticate, on_logout)}
                            }
                            {advertises(Advertiser::Agent, advertised(agent, &driven, &auth, lists_sessions), only)}
                        }
                    },
                },
            }
        }
    }
}

/// Who the agent says it is.
///
/// `agentInfo` is optional in v1 and real agents leave it out, so its absence
/// is said rather than filled in: a display that invented a name for an agent
/// that gave none would be describing something else.
fn identity(
    whose: Advertiser,
    info: Option<&v1::Implementation>,
    protocol: Element,
    standing: Element,
) -> Element {
    rsx! {
        header {
            class: "who",
            "data-claim": "identity",
            "data-identity": "{whose.whose()}",
            {match info {
                // **Who it is, as a heading rather than as a table of three
                // rows.** `title`, `name` and `version` were a two-column list
                // in a rail, which is what a 288px column does to three short
                // facts; on a screen of its own they are the line that says
                // whose screen it is. Every one of them is still drawn, and the
                // two that are protocol strings are still monospace.
                Some(info) => rsx! {
                    h2 { class: "who-name",
                        {info.title.clone().unwrap_or_else(|| info.name.to_string())}
                    }
                    p { class: "who-facts",
                        span { "{info.name}" }
                        span { "{info.version}" }
                        {protocol}
                        {standing}
                    }
                },
                None => rsx! {
                    h2 { class: "who-name", "{whose.identity_title()}" }
                    p { class: "who-facts",
                        span { class: "cleared", "the agent sent no " code { "agentInfo" } }
                        {protocol}
                        {standing}
                    }
                },
            }}
        }
    }
}

/// Who made an Advertisement (`CONTEXT.md`, *Advertisement*): the agent, or
/// this client.
///
/// **Not merged into one list of capabilities** (§9): who claimed a thing is
/// the point of showing it, and a row that did not say whose claim it was would
/// leave a reader unable to tell an agent's offer from an answer to this
/// client's. The two the glossary names — Agent Capability and Client
/// Capability — are the two this has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Advertiser {
    /// The agent, in its `initialize` result.
    Agent,
    /// This client, in its `initialize` request.
    Client,
}

impl Advertiser {
    fn identity_title(self) -> &'static str {
        match self {
            Self::Agent => "Agent identity",
            Self::Client => "Client identity",
        }
    }

    /// What the block is called on screen.
    fn title(self) -> &'static str {
        match self {
            // The domain's own noun, and short enough to share its line with
            // the count that opens it: `What the agent advertised` needed 182
            // of the 186 pixels left beside a chevron and `6 of 13`, which is a
            // heading four pixels from wrapping. It also pairs with the
            // identity block above it — Agent identity, Agent advertisement —
            // where the old wording did not.
            Self::Agent => "Agent advertisement",
            Self::Client => "Client advertisement",
        }
    }

    /// Which of the two it is, for a test to find one block among the two.
    ///
    /// **Not for the stylesheet**, which is the point of it being here at all:
    /// the two accounts are drawn identically (#86), so a rule that read this
    /// would be the beginning of one of them becoming the other's footnote.
    /// `style.rs` holds that line with a test of its own, which is why the
    /// class it bans is a name nothing else on the screen uses.
    fn whose(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Client => "client",
        }
    }

    /// Where the claims were made, and — for this client — why a reader is
    /// being shown them at all.
    fn caption(self) -> Element {
        match self {
            Self::Agent => rsx! {
                "In its "
                code { "initialize" }
                " result: per-connection, and true of every session on it."
            },
            // **The cause an option shape would otherwise not have** (§7.6).
            // An agent that gates on this claim offers a different thing to a
            // client that made it, so a reader comparing what they see here
            // against what another client sees is reading an answer to this
            // block.
            Self::Client => rsx! {
                "In every "
                code { "initialize" }
                " request this tool sends, fixed and not configurable. What this client does not claim, a conformant agent must not send — so an offer that differs from the one another client gets is an answer to this."
            },
        }
    }
}

/// What an account says at a glance: how much of it was claimed, and whether
/// anything claimed was refused when somebody took it up.
///
/// **A count is not a grade** (§9). It says how many of the rows below say
/// *advertised*, which is the arithmetic of the list a reader would otherwise
/// do by scrolling it; it scores nothing, orders nothing, and the refusals are
/// counted rather than judged — one refusal is a finding to go and read, not a
/// mark against the agent.
struct Counted {
    advertised: usize,
    total: usize,
    refused: usize,
}

/// Which of an account's rows are on screen.
///
/// **Thirteen rows is a table, and a table is read with a question.** The two a
/// reader brings are *what did it not claim* and *what did it refuse* — and
/// answering either meant reading all thirteen and holding the answer in your
/// head. Narrowing is presentation and nothing else: the count on the summary
/// line goes on counting the whole account, because that count is the agent's
/// answer rather than this control's.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Only {
    #[default]
    Everything,
    Advertised,
    Unadvertised,
    Refused,
}

impl Only {
    /// The four, in the order the chips are drawn.
    const ALL: [Self; 4] = [
        Self::Everything,
        Self::Advertised,
        Self::Unadvertised,
        Self::Refused,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Everything => "All",
            Self::Advertised => "Advertised",
            Self::Unadvertised => "Not advertised",
            Self::Refused => "Refused",
        }
    }

    fn keeps(self, claim: &Claim) -> bool {
        match self {
            Self::Everything => true,
            Self::Advertised => claim.advertised,
            Self::Unadvertised => !claim.advertised,
            Self::Refused => matches!(claim.fourth, Some(Fourth::Outcome(Driven::Refused(_)))),
        }
    }

    /// How many of an account's rows this would leave.
    fn counts(self, groups: &[(&'static str, Vec<Claimed>)]) -> usize {
        groups
            .iter()
            .flat_map(|(_, runs)| runs.iter())
            .flat_map(|run| run.claims.iter())
            .filter(|claim| self.keeps(claim))
            .count()
    }
}

fn counting(groups: &[(&'static str, Vec<Claimed>)]) -> Counted {
    let claims = || {
        groups
            .iter()
            .flat_map(|(_, runs)| runs.iter())
            .flat_map(|run| run.claims.iter())
    };

    Counted {
        advertised: claims().filter(|claim| claim.advertised).count(),
        total: claims().count(),
        refused: claims()
            .filter(|claim| matches!(claim.fourth, Some(Fourth::Outcome(Driven::Refused(_)))))
            .count(),
    }
}

/// One advertiser's account of itself: the claims, grouped the way the protocol
/// groups them and split by the shape each was made in.
///
/// **The same rows for both of them**, which is the whole of drawing one as
/// plainly as the other: one function, one list markup, one statement of shape,
/// so a stylesheet cannot make either account the footnote of the other.
/// What a narrowing is called in a sentence rather than on a chip.
fn which_said(only: Only) -> &'static str {
    match only {
        Only::Everything => "every claim",
        Only::Advertised => "the claims this agent advertised",
        Only::Unadvertised => "the claims it did not advertise",
        Only::Refused => "the claims it advertised and then refused",
    }
}

fn advertises(
    by: Advertiser,
    groups: Vec<(&'static str, Vec<Claimed>)>,
    mut only: Signal<Only>,
) -> Element {
    // The headline the summary carries, counted before the rows are consumed
    // and before any of them are narrowed away: it is the agent's answer about
    // itself, and a count that moved when a chip was pressed would be this
    // window's answer about its own filter.
    let counted = counting(&groups);
    // Which chips there are to press, by what each would leave. A chip that
    // narrowed to nothing is a control that does nothing, and one reading
    // `Refused 0` is the empty state's sentence in a shape that means *there
    // are some* (ADR 0006).
    let chips: Vec<(Only, usize)> = Only::ALL
        .into_iter()
        .map(|which| (which, which.counts(&groups)))
        .filter(|(which, count)| *count > 0 || *which == Only::Everything)
        .collect();
    let narrowed = only();
    let showing = narrowed.counts(&groups);
    // Narrowing takes rows off the screen, so the rows that are left are the
    // ones this asked for and never all of them (§9's Console rule, which this
    // is the second surface to state): a run whose every row went is not a run
    // the agent did not make.
    let groups: Vec<_> = groups
        .into_iter()
        .map(|(group, runs)| {
            let runs: Vec<_> = runs
                .into_iter()
                .filter_map(|mut run| {
                    run.claims.retain(|claim| narrowed.keeps(claim));
                    (!run.claims.is_empty()).then_some(run)
                })
                .collect();
            (group, runs)
        })
        .filter(|(_, runs)| !runs.is_empty())
        .collect();
    // What a whole run says once instead of what every row in it said
    // separately, decided before the markup because a run states it above its
    // rows and the rows have to know it was said (§7.7).
    let groups: Vec<_> = groups
        .into_iter()
        .map(|(group, runs)| {
            let runs: Vec<_> = runs
                .into_iter()
                .map(|run| match run.shared_reason() {
                    Some(why) => (run.hushed(), Some(why)),
                    None => (run, None),
                })
                .collect();
            (group, runs)
        })
        .collect();
    rsx! {
        div {
            class: "rail-group advertisement",
            "data-claim": "advertised",
            // Which of the two accounts this block is, for a test to find one
            // by. Data rather than a class because no rule may read it: the two
            // are the same rows at the same weight (#86), and the stylesheet is
            // held to that in `style.rs`.
            "data-advertiser": "{by.whose()}",
            div { class: "rail-head",
                span { class: "eyebrow", "{by.title()}" }
                // **The answer the rows are read to get**, on the line that
                // opens them: how many of the claims below were made, and — the
                // one thing a count cannot say — how many were refused when
                // something drove them (§7.7).
                span { class: "rail-count", "data-slot": "claim-count",
                    "{counted.advertised}/{counted.total}"
                    if counted.refused > 0 {
                        span { class: "claim-refused", "data-slot": "claim-refused",
                            " · {counted.refused} refused"
                        }
                    }
                }
            }
            p { class: "hint", {by.caption()} }

            // The question the table is read with, as chips over it. Only where
            // there is more than one to press: an account whose every row is
            // advertised has one answer, and a row of chips offering to narrow
            // to it is chrome.
            if chips.len() > 1 {
                div { class: "setting-values", role: "group", aria_label: "Narrow the claims",
                    for (which , count) in chips.iter().copied() {
                        button {
                            key: "{which.label()}",
                            class: "chip",
                            r#type: "button",
                            aria_pressed: which == narrowed,
                            onclick: move |_| only.set(which),
                            "{which.label()} {count}"
                        }
                    }
                }
            }
            // **A narrowed surface says what it is hiding**, so that a hidden
            // row and a claim the agent never made never look alike.
            if narrowed != Only::Everything {
                p { class: "hint", "data-slot": "narrowed", role: "status",
                    "Showing {showing} of {counted.total}: {which_said(narrowed)}."
                    button {
                        class: "chip",
                        r#type: "button",
                        onclick: move |_| only.set(Only::Everything),
                        "show all"
                    }
                }
            }

            div { "data-slot": "advertisement-rows",
                for (group , runs) in groups {
                    section { key: "{group}", class: "caps-group",
                        div { class: "caps-group-head",
                            span { class: "caps-group-name top", "{group}" }
                            span { class: "caps-group-rule", aria_hidden: "true" }
                        }
                        for (run , shared) in runs {
                            div { key: "{run.at}",
                                div { class: "caps-group-head",
                                    span { class: "caps-group-name", "{run.at}" }
                                    span { class: "caps-group-rule", aria_hidden: "true" }
                                    span { class: "caps-group-count", {shaped(run.shape)} }
                                }
                                // The shape a run of claims was made in, said
                                // rather than left to be inferred from the field
                                // names: a reader who cannot see the shape reads
                                // their own agent's `initialize` result as
                                // inconsistent rather than as conformant.
                                {stated(run.shape, run.at)}
                                // The reason nothing here drives a single row of
                                // this run, said once above them (§7.7) rather
                                // than word for word on each, which made one
                                // fact about this tool look like three about the
                                // agent.
                                if let Some(why) = shared {
                                    p { class: "hint", "data-slot": "unreached",
                                        "{why.said_of_all()}"
                                    }
                                }
                                {rows_of(run.claims)}
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One run's claims, as the rows the drawing gives a capability list: the name
/// the protocol calls it, the answer at the far end, and what else is known
/// under both.
fn rows_of(claims: Vec<Claim>) -> Element {
    rsx! {
        // A list, and the platform's own: what a run of claims is is a list of
        // them, and a reader hearing this screen is told how many there are
        // before the first one rather than counting rows as they arrive.
        ul { class: "caps", "data-slot": "capabilities",
            for claim in claims {
                li {
                    key: "{claim.name}",
                    class: if claim.advertised { "cap yes" } else { "cap no" },
                    span { class: "cap-name", "{claim.name}" }
                    // The answer this screen exists to give, in text (§7.5,
                    // #83). Both halves are drawn at the same weight and the
                    // same size: the half that is an absence is not an aside.
                    span { class: "cap-answer", "data-slot": "state",
                        if claim.advertised { "advertised" } else { "not advertised" }
                    }
                    // And the two facts under them: the field the claim was made
                    // in, where it is not the name already said, and what became
                    // of driving it — or why nothing here can (§7.7).
                    if claim.field != claim.name || claim.fourth.is_some() {
                        span { class: "cap-said",
                            if claim.field != claim.name {
                                span { class: "cap-field", "{claim.field}" }
                            }
                            if claim.field != claim.name && claim.fourth.is_some() {
                                span { aria_hidden: "true", " · " }
                            }
                            if let Some(fourth) = &claim.fourth {
                                span { "data-slot": "driven", {came_back(fourth)} }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The shape a run was made in, in the two words a count column has room for.
fn shaped(shape: Shape) -> &'static str {
    match shape {
        Shape::Flag => "boolean",
        Shape::Object => "object",
        Shape::List => "list",
    }
}

/// Whether the advertisement was ever driven on this connection and what came
/// back — or, where nothing here can drive it, why not (§7.7).
///
/// **Weighted like the rest of the row and no louder.** A refusal gets no
/// colour of its own, no count and no ordering: it is the third thing that
/// happened rather than the bad one, and an agent that advertised a capability
/// and then refused it is a finding the reader draws rather than a grade this
/// tool awards.
///
/// The agent's own error is what a refusal says, whole. A window that
/// paraphrased it would be standing between the reader and the wire — the rule
/// the auth block's refusal already follows.
///
/// The leading space keeps the visible state and this fact separate when a
/// non-visual reader composes the row's text.
fn came_back(fourth: &Fourth) -> Element {
    match fourth {
        Fourth::Outcome(Driven::Unasked) => rsx! { " not driven" },
        Fourth::Outcome(Driven::Answered) => rsx! { " driven and answered" },
        Fourth::Outcome(Driven::Refused(error)) => rsx! { " driven and refused: {error}" },
        // `Driven` is non-exhaustive, and an outcome this window has not been
        // taught must not be drawn as one it has.
        Fourth::Outcome(_) => rsx! { " driven, with an outcome this window has no rendering for" },
        Fourth::Unreached(why) => rsx! { "{why.said()}" },
    }
}

/// The fourth fact a capability row carries, which is one of two things (§7.7).
///
/// **Never both, and never one dressed as the other.** *Not driven* on a row no
/// affordance reaches is a sentence about the agent when the true sentence is
/// about this tool, and a reader who takes it at face value goes hunting for a
/// button that never existed.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Fourth {
    /// Something here can drive the advertisement, and this is what became of
    /// driving it — core's record for every capability but one, and the auth
    /// state for that one.
    Outcome(Driven),
    /// Nothing here can drive it, and this is why.
    Unreached(Unreached),
}

/// Why nothing in this tool drives an advertisement (§7.7).
///
/// **A fact about where the affordances live, not a state a store holds.**
/// Core's record answers what became of driving a capability; it cannot answer
/// whether anything here *could*, because that is a question about this window
/// — which is why these are decided beside the rows and written down as an
/// enumeration rather than looked up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unreached {
    /// `session/resume` where `session/load` is advertised too: the restore
    /// property prefers load ([`Restore::preferred`]), so a resume is never sent
    /// against such an agent and the row would otherwise read *not driven*
    /// forever (§7.5).
    LoadPreferred,
    /// `session/close` or `session/delete` where `session/list` is not: both
    /// affordances hang off a listing row, so there is no row to press them on
    /// — the consequence §7.5 records, made visible where it bites.
    NoListingRow,
    /// Embedded context remains deferred (§1.1).
    OneTextBlock,
    /// The two MCP transports. Every session is opened with an empty
    /// `mcpServers`, and that too is deferred with a ring owed (§1.1).
    NoMcpServers,
}

impl Unreached {
    /// The half of the sentence that is the reason, without the correction in
    /// front of it.
    ///
    /// Split out because the correction is said once wherever the reason is
    /// shared — a run of rows nothing here reaches states it above them — and
    /// the two forms must not be able to drift into saying different things
    /// about the same limit.
    fn because(self) -> &'static str {
        match self {
            Self::LoadPreferred => "it prefers session/load, which this agent advertised too",
            Self::NoListingRow => {
                "it is pressed on a listing row, and this agent advertised no session/list"
            }
            Self::OneTextBlock => "the composer sends text, images and audio, not embedded context",
            Self::NoMcpServers => "a session is opened with an empty mcpServers",
        }
    }

    /// What the row says instead of an outcome.
    ///
    /// Every one of them says *this tool cannot drive it* first and the reason
    /// second, because the first half is the correction a reader needs — the
    /// blankness is the inspector's and not their agent's — and the second half
    /// is what tells them whether it is a limit they can do anything about.
    ///
    /// The leading space is the one [`came_back`] takes, for the same reason.
    fn said(self) -> String {
        format!(" this tool cannot drive it: {}", self.because())
    }

    /// The same sentence about a whole run of rows, said above them.
    ///
    /// **The correction is not dropped, only said once** (§7.7): a reader who
    /// meets three rows and no outcome still has to be told the blankness is
    /// this tool's, and it is the plural that makes it one statement about the
    /// run rather than the first of three identical ones. No leading space,
    /// because it is a sentence of its own rather than the fourth thing a row
    /// composes.
    fn said_of_all(self) -> String {
        format!("this tool cannot drive any of these: {}", self.because())
    }
}

/// How a run of claims was made, in the protocol's own terms.
///
/// Said rather than left to be inferred from the field names: a reader who
/// cannot see the shape reads their own agent's `initialize` result as
/// inconsistent rather than as conformant.
fn stated(shape: Shape, at: &'static str) -> Element {
    rsx! {
        p { class: "hint shape", "data-slot": "shape",
            {match shape {
                Shape::Flag => rsx! {
                    "boolean on "
                    code { "{at}" }
                    "; "
                    code { "true" }
                    " claims"
                },
                Shape::Object => rsx! {
                    "object under "
                    code { "{at}" }
                    "; "
                    code { "{{}}" }
                    " claims too"
                },
                Shape::List => rsx! {
                    "list at "
                    code { "{at}" }
                    "; an entry claims, empty none"
                },
            }}
        }
    }
}

/// The shapes ACP v1 states an advertisement in.
///
/// The first two are kept apart rather than flattened into one notion of
/// support (§7.5). `session/load` is gated by a top-level boolean while resume,
/// close, delete, list and `additionalDirectories` are gated by the *presence*
/// of a capability object under `sessionCapabilities` — and the schema records
/// that the asymmetry is expected to be unified in a later protocol version.
/// Until it is, an agent author reading this screen should be able to see why
/// their own agent advertises support in two different shapes.
///
/// The third is neither, and is here because an Advertisement is wider than a
/// capability (`CONTEXT.md`, *Advertisement*): `authenticate` is gated on the
/// `authMethods` the agent listed, which is a list on the `initialize` result
/// beside the capability object rather than anything inside it (§7.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    /// A boolean field, where `false` and absent are the same claim.
    Flag,
    /// A capability object, where the presence of the object is the claim and
    /// an empty one advertises as loudly as a full one.
    Object,
    /// A list, where an entry is the claim and an empty list is no claim at
    /// all.
    List,
}

/// A run of claims made in one shape at one place in the `initialize` result.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Claimed {
    /// The shape every claim in the run was made in.
    shape: Shape,
    /// Where the run lives, by the name the protocol gives that object.
    at: &'static str,
    claims: Vec<Claim>,
}

impl Claimed {
    /// The reason nothing in this tool drives *any* row of the run, where every
    /// row of it has the same one (§7.7).
    ///
    /// **A run and not a group**, because a run is what shares a shape and a
    /// place in the `initialize` result, and the sentence hoisted here is about
    /// where the affordances are rather than about the agent: the three prompt
    /// content shapes are one run and one limit, and so are the two MCP
    /// transports. The session lifecycle is not — an unreachable `session/close`
    /// sits beside a `session/list` that was driven — so it keeps the reason on
    /// the rows that have one, which is the answer for a mixed run.
    ///
    /// **Two or more, never one.** A run of a single row has nothing to say
    /// twice, and the plural sentence a hoist puts above it would be reporting a
    /// pattern where there is one fact.
    fn shared_reason(&self) -> Option<Unreached> {
        let mut reasons = self.claims.iter().map(|claim| match claim.fourth {
            Some(Fourth::Unreached(why)) => Some(why),
            _ => None,
        });
        let first = reasons.next().flatten()?;
        (self.claims.len() > 1 && reasons.all(|why| why == Some(first))).then_some(first)
    }

    /// The same run with the fourth fact taken off its rows, for when the run
    /// itself is about to say it.
    ///
    /// The fact is removed rather than merely undrawn: a row that kept it in the
    /// markup and hid it would be the dimness rule broken by another name, and
    /// what a reader — or a screen reader — meets is a run whose rows report no
    /// outcome under a sentence saying why none of them can.
    fn hushed(mut self) -> Self {
        for claim in &mut self.claims {
            claim.fourth = None;
        }
        self
    }
}

/// One advertisement: what it is called, where the claim is read from, whether
/// this agent made it, and — where the agent made it — what happened when
/// somebody took it up, or why nothing here can take it up.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Claim {
    /// What the protocol calls the capability — the method it gates where it
    /// gates one, the field name where it gates a field.
    name: &'static str,
    /// The field the claim itself is made in, which for the session lifecycle
    /// is not the method name it gates.
    field: &'static str,
    advertised: bool,
    /// What became of driving the advertisement, or why nothing here can
    /// (§7.7) — and `None` on a row that says nothing about driving at all.
    ///
    /// **A fourth element of the same row, not a lane of its own**: no colour
    /// for a refusal, no count, no ordering. Which half a row is stays carried
    /// by weight on screen and by words in the accessibility tree, as
    /// advertised/not-advertised already is.
    fourth: Option<Fourth>,
}

impl Claim {
    fn new(name: &'static str, field: &'static str, advertised: bool) -> Self {
        Self {
            name,
            field,
            advertised,
            fourth: None,
        }
    }

    /// A capability the protocol calls by the very field it is claimed in,
    /// which is every one of them outside the session lifecycle.
    fn self_named(name: &'static str, advertised: bool) -> Self {
        Self::new(name, name, advertised)
    }

    /// The row's fourth fact, read out of core's record (§7.7).
    ///
    /// **The capability is named rather than matched**: the record is keyed on
    /// [`AgentCapability`], so a row asks about a variant that has to exist —
    /// a key this file matched loosely would render a capability upstream adds
    /// as *unknown* instead of breaking a build, which is what the schema
    /// tripwire behind these lists exists to prevent.
    fn driven(self, record: &DrivenRecord, capability: AgentCapability) -> Self {
        self.if_advertised(Fourth::Outcome(record.of(capability)))
    }

    /// The auth row's fourth fact, read out of the auth state (§7.7).
    ///
    /// **The one row that does not ask the record, because `authenticate` has
    /// no entry in it** — the argument is at the top of this file and the
    /// mapping is [`AuthState::driven`]. The block below these rows reads the
    /// same value, so the two cannot say different things about one login.
    fn logged_in(self, auth: &AuthState) -> Self {
        self.if_advertised(Fourth::Outcome(auth.driven()))
    }

    /// Why nothing in this tool reaches *this agent's* claim (§7.7).
    ///
    /// Which agent it is decides it: a `session/resume` is unreachable only
    /// beside an advertised `session/load`, and a `session/close` only where
    /// there is no listing to press it on.
    fn unreached(self, why: Unreached) -> Self {
        self.if_advertised(Fourth::Unreached(why))
    }

    /// [`driven`](Self::driven), [`logged_in`](Self::logged_in) and
    /// [`unreached`](Self::unreached) all go through here, and
    /// [`undrivable`](Self::undrivable) pointedly does not.
    ///
    /// **A row the agent did not advertise says nothing about driving** (§7.7).
    /// There is no claim to have driven, and *not driven* about a capability
    /// nobody offered is an answer to a question nobody asked — the row's
    /// answer is the *not advertised* it already gives.
    fn if_advertised(mut self, fourth: Fourth) -> Self {
        self.fourth = self.advertised.then_some(fourth);
        self
    }

    /// Why nothing in this tool reaches the claim on *any* agent (§7.7).
    ///
    /// **Said whether or not this agent advertised it**, which is where it
    /// parts company with [`unreached`](Self::unreached): the sentence is about
    /// this tool, and what the agent claimed does not change it. An agent
    /// author who sees their `image` row blank wants to know it is blank here
    /// for everyone.
    fn undrivable(mut self, why: Unreached) -> Self {
        self.fourth = Some(Fourth::Unreached(why));
        self
    }
}

/// The advertised capabilities, as rows: what the protocol calls it, the field
/// that gates it, and whether this agent offered it.
///
/// Named by their method or field names rather than paraphrased: the reader of
/// an inspector is reading the protocol, and a capability display that renamed
/// `session/load` to "can resume conversations" would be a translation they did
/// not ask for.
///
/// The `unstable_*` capabilities the schema crate hides behind features are not
/// here, for the reason nothing else decodes them either (§8): they are traffic
/// this client does not claim to understand, and the `initialize` frame in the
/// trace is where an agent's unstable claims stay readable. `session/fork` is
/// the session lifecycle's own instance of that rule.
///
/// **This list is written by hand, and one test asks the protocol whether it is
/// still the whole of the stable set** —
/// `crates/core/tests/capability.rs`'s
/// `the_capability_set_the_display_names_is_the_whole_stable_set_v1_defines`,
/// which reads the schema the crate derives rather than this list or its own.
/// A row dropped from here fails a test that names it; a capability *added*
/// upstream is missing from every hand-written copy by construction, and that
/// one is what the schema can be asked about. A capability nobody drew a row
/// for is the failure this whole screen exists to prevent (§9), so it is worth
/// a build breaking over.
///
/// **A row also says whether the advertisement was ever driven** (§7.7), where
/// something in this tool can drive it: `record` is core's, and a row names the
/// [`AgentCapability`] it is about rather than matching a string, which is what
/// keeps that key inside the same two-list arrangement this list is half of.
/// The five session lifecycle methods carry the fact, because each of them is
/// a call something in this tool sends; so does `logout`, which the button
/// below the auth methods drives; and so does `additionalDirectories`, which is
/// a *field* on a request rather than a method and is driven by the roots
/// control beside `session/new` and on every listing row. Whether that field
/// crossed is read off the outgoing frame, so an empty control leaves the row
/// reading *not driven* because nothing was asked rather than because nothing
/// could be. `authenticate` carries it too and reads it out of [`AuthState`],
/// which is that record for the auth methods and has been since the MVP.
///
/// **Where no affordance can reach a row, it draws no outcome and says why**
/// (§7.7). Which rows those are is a fact about where the affordances live, so
/// it is decided here rather than looked up: `session/resume` is never sent
/// against an agent that advertised load too ([`Restore::preferred`]), close and
/// delete have nowhere to be pressed on an agent that advertised no listing, and
/// the three prompt content shapes and the two MCP transports have no affordance
/// on any agent. *Not driven* on one of those would be a sentence about the
/// agent when the true sentence is about this tool.
///
/// **Where a whole run shares the one reason, the run says it once**
/// ([`Claimed::shared_reason`]) — which is the two runs decided here whose rows
/// are all `undrivable`, prompt content and MCP servers. It is a rendering
/// decision rather than a different fact, so the claims are built the same way
/// either way: the row still knows why nothing reaches it, and the panel is what
/// decides that three rows repeating one sentence about this tool would read as
/// three facts about the agent.
///
/// **And a row the agent did not advertise says nothing about driving at all**,
/// because there is no claim to have driven — with the five this tool cannot
/// drive on any agent excepted, since what those rows say is about this tool and
/// an agent's silence does not change it.
fn advertised(
    agent: &v1::InitializeResponse,
    record: &DrivenRecord,
    auth: &AuthState,
    lists_sessions: bool,
) -> Vec<(&'static str, Vec<Claimed>)> {
    // The whole result rather than the capability object, because one of these
    // rows is claimed outside it: `authMethods` is an Advertisement sitting
    // beside `agentCapabilities` rather than inside it (§7.1).
    let capabilities = &agent.agent_capabilities;
    let sessions = &capabilities.session_capabilities;
    let prompt = &capabilities.prompt_capabilities;
    let mcp = &capabilities.mcp_capabilities;

    // Core's own preference, asked rather than restated (§7.5): a resume is sent
    // only where the agent's advertisement entitles this tool to send one, so an
    // agent that advertised load beside it has a resume row nothing here reaches.
    let resume = Claim::new("session/resume", "resume", sessions.resume.is_some());
    let resume = match Restore::preferred(capabilities) {
        Some(Restore::Load) => resume.unreached(Unreached::LoadPreferred),
        _ => resume.driven(record, AgentCapability::Resume),
    };
    // Both ways out of a session are pressed on a listing row, which is where a
    // session the agent claims to hold is named (§7.5) — so an agent that
    // advertised no listing has both claims displayed and no row to press them
    // on.
    let ending = |name, field, advertised, capability| {
        let row = Claim::new(name, field, advertised);
        if lists_sessions {
            row.driven(record, capability)
        } else {
            row.unreached(Unreached::NoListingRow)
        }
    };

    vec![
        (
            "Sessions",
            vec![
                Claimed {
                    shape: Shape::Flag,
                    at: "agentCapabilities",
                    claims: vec![
                        Claim::new("session/load", "loadSession", capabilities.load_session)
                            .driven(record, AgentCapability::Load),
                    ],
                },
                Claimed {
                    shape: Shape::Object,
                    at: "sessionCapabilities",
                    claims: vec![
                        Claim::new("session/list", "list", lists_sessions)
                            .driven(record, AgentCapability::List),
                        resume,
                        ending(
                            "session/close",
                            "close",
                            sessions.close.is_some(),
                            AgentCapability::Close,
                        ),
                        ending(
                            "session/delete",
                            "delete",
                            sessions.delete.is_some(),
                            AgentCapability::Delete,
                        ),
                        Claim::self_named(
                            "additionalDirectories",
                            sessions.additional_directories.is_some(),
                        )
                        .driven(record, AgentCapability::AdditionalDirectories),
                    ],
                },
            ],
        ),
        (
            "Prompt content",
            vec![Claimed {
                shape: Shape::Flag,
                at: "promptCapabilities",
                claims: vec![
                    Claim::self_named("image", prompt.image).driven(record, AgentCapability::Image),
                    Claim::self_named("audio", prompt.audio).driven(record, AgentCapability::Audio),
                    Claim::self_named("embeddedContext", prompt.embedded_context)
                        .undrivable(Unreached::OneTextBlock),
                ],
            }],
        ),
        (
            "MCP servers",
            vec![Claimed {
                shape: Shape::Flag,
                at: "mcpCapabilities",
                claims: vec![
                    Claim::self_named("http", mcp.http).undrivable(Unreached::NoMcpServers),
                    Claim::self_named("sse", mcp.sse).undrivable(Unreached::NoMcpServers),
                ],
            }],
        ),
        (
            "Auth",
            vec![
                // Not a capability object and not a boolean: an Advertisement is
                // wider than a capability (`CONTEXT.md`), and this one is the
                // list of methods the agent said it would take. Its own run,
                // because it is claimed somewhere else entirely — on the
                // `initialize` result beside `agentCapabilities` rather than
                // inside it.
                Claimed {
                    shape: Shape::List,
                    at: "authMethods",
                    claims: vec![
                        Claim::new(
                            "authenticate",
                            "authMethods",
                            !agent.auth_methods.is_empty(),
                        )
                        .logged_in(auth),
                    ],
                },
                Claimed {
                    shape: Shape::Object,
                    at: "auth",
                    claims: vec![
                        Claim::self_named("logout", capabilities.auth.logout.is_some())
                            .driven(record, AgentCapability::Logout),
                    ],
                },
            ],
        ),
    ]
}

/// What *this client* claimed about itself, as rows: the data shape it renders,
/// the service it performs, and the services it still declines.
///
/// **The claim is a fact about the request core sends**, so it is read out of
/// the same value that goes on the wire
/// ([`client_capabilities`](acp_inspector_core::client_capabilities)) rather than
/// written out here — a panel that stated the claim in its own words would be
/// this window advertising something on core's behalf.
///
/// Named and shaped exactly as the agent's rows are: `fs/read_text_file` is the
/// method the claim gates and `readTextFile` is the field it is made in, and a
/// row says both wherever they differ. `terminal` gates every `terminal/*`
/// method at once, which is what the row's name says. A run is named by where
/// it lives under `clientCapabilities` — the root by its own name, and a nested
/// object by the path to it, because `session.configOptions` is two levels a
/// reader cannot guess from a row.
///
/// The claims come first and the declines follow them, because the declines
/// are the stance that survived the narrowing (§7.3) and a reader who stopped at
/// the first row would have read this client as offering more than it does.
/// Elicitation is among the claims rather than the declines from the fifth ring
/// on (§7.8): it is a service this tool performs, drawn a mode at a time because
/// an agent gates on the modes and not on the object holding them.
///
/// **This list is written by hand too**, and pinned by the same kind of test as
/// the agent's: `crates/core/tests/capability.rs`'s
/// `the_client_capability_set_the_display_names_is_the_whole_stable_set_v1_defines`
/// asks the schema whether a client capability has appeared that nobody drew a
/// row for.
fn claimed(capabilities: &v1::ClientCapabilities) -> Vec<(&'static str, Vec<Claimed>)> {
    let fs = &capabilities.fs;
    let boolean_config_options = capabilities
        .session
        .as_ref()
        .and_then(|session| session.config_options.as_ref())
        .is_some_and(|options| options.boolean.is_some());

    let elicitation = capabilities.elicitation.as_ref();

    vec![
        (
            "Config options",
            vec![Claimed {
                shape: Shape::Object,
                at: "session.configOptions",
                claims: vec![Claim::self_named("boolean", boolean_config_options)],
            }],
        ),
        // The service this client performs, claimed a mode at a time (§7.8) —
        // two rows rather than one, because the agent gates on the modes and
        // not on the object they sit in, and an agent that finds one of them
        // missing is being told a different thing from an agent that finds
        // neither.
        (
            "Elicitation",
            vec![Claimed {
                shape: Shape::Object,
                at: "elicitation",
                claims: vec![
                    Claim::self_named(
                        "form",
                        elicitation.is_some_and(|modes| modes.form.is_some()),
                    ),
                    Claim::self_named("url", elicitation.is_some_and(|modes| modes.url.is_some())),
                ],
            }],
        ),
        (
            "Filesystem",
            vec![Claimed {
                shape: Shape::Flag,
                at: "fs",
                claims: vec![
                    Claim::new("fs/read_text_file", "readTextFile", fs.read_text_file),
                    Claim::new("fs/write_text_file", "writeTextFile", fs.write_text_file),
                ],
            }],
        ),
        (
            "Terminal",
            vec![Claimed {
                shape: Shape::Flag,
                at: "clientCapabilities",
                claims: vec![Claim::new("terminal/*", "terminal", capabilities.terminal)],
            }],
        ),
        (
            "Auth",
            vec![Claimed {
                shape: Shape::Flag,
                at: "auth",
                claims: vec![Claim::new(
                    "Terminal authentication",
                    "terminal",
                    capabilities.auth.terminal,
                )],
            }],
        ),
    ]
}

/// The advertised auth methods, where authentication stands, the way out of a
/// login, and — when the agent has asked for one — where to go and do it.
///
/// **`logout` sits here, with the auth methods** (§7.7), and not among the
/// session controls: the call carries no session id, so its place on screen
/// matches its scope on the wire. It is gated on the advertisement and on
/// nothing else — offered wherever the agent claimed it, including with nobody
/// logged in, because "logout without a login" is a corner the specification
/// leaves entirely undefined and driving it is the point.
fn login(
    methods: &[v1::AuthMethod],
    auth: &AuthState,
    pending: &[AuthenticationCall],
    logs_out: bool,
    connected: bool,
    on_authenticate: EventHandler<v1::AuthMethodId>,
    on_logout: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "rail-group authentication", "data-claim": "authentication",
            div { class: "rail-group",
                if auth.needs_login() {
                    // Neutral, and deliberately: an agent that wants a login is
                    // in an ordinary protocol state rather than a broken one,
                    // and a warning here would be this window grading an agent
                    // for asking (§7.1).
                    div { class: "alert login",
                        div {
                            strong { "The agent wants a login." }
                            {outcome(auth)}
                            p {
                                {match named_methods(methods) {
                                    // Which is a finding: the agent asked for a login
                                    // and named no way to do one.
                                    None => rsx! { "It advertised no way to do it." },
                                    Some(named) => rsx! { "It advertised {named}." },
                                }}
                            }
                            p {
                                "Run the agent's own login in your own terminal — the inspector has none to run it in and never will — then launch it again. Sending "
                                code { "authenticate" }
                                " below tells the agent which method you used; it does not perform one."
                            }
                        }}
                } else {
                    {outcome(auth)}
                }

                for (index, call) in pending.iter().enumerate() {
                    p { key: "{index}",
                        class: "hint authentication-pending",
                        role: "status",
                        aria_live: "polite",
                        aria_busy: "true",
                        {match call {
                            AuthenticationCall::Authenticate(pending) => {
                                let method = methods
                                    .iter()
                                    .find(|method| method.id() == pending)
                                    .map(|method| format!("{} ({})", method.name(), method.id()))
                                    .unwrap_or_else(|| pending.to_string());
                                rsx! { "Authenticating with {method}." }
                            }
                            AuthenticationCall::Logout => rsx! { "Logging out." },
                        }}
                    }
                }

                // No line where there are none: the `authenticate` row of the
                // advertisement below says the agent claimed no method, in the
                // words §7.7 gives every other claim, and a sentence here
                // saying it again was the same fact drawn twice on one screen.
                if !methods.is_empty() {
                    // **Not daisyUI's list.** `list-row` is a grid of its own,
                    // and a row that had been made a flex of *here is a login,
                    // here is the call that uses it* became one grid item in
                    // its first track: the name and the button shrink-wrapped
                    // together at the near edge with nine hundred pixels of
                    // nothing after them. What this list needs from a component
                    // library is nothing — the rows are two facts and a call —
                    // so it takes nothing and states its own.
                    ul { class: "methods",
                        for method in methods {
                            li { key: "{method.id()}", class: "auth-method",
                                    div {
                                        strong { "{method.name()}" }
                                        div { class: "hint mono", "{method.id()}" }
                                        if let Some(description) = method.description() {
                                            div { class: "hint", "{description}" }
                                        }
                                    }
                                    button {
                                        class: "btn btn-xs btn-quiet authenticate",
                                        r#type: "button",
                                        aria_label: "Authenticate with {method.name()} ({method.id()})",
                                        aria_busy: pending.iter().any(|call| matches!(call, AuthenticationCall::Authenticate(id) if id == method.id())),
                                        disabled: !connected,
                                        onclick: {
                                            let id = method.id().clone();
                                            move |_| on_authenticate.call(id.clone())
                                        },
                                        if pending.iter().any(|call| matches!(call, AuthenticationCall::Authenticate(id) if id == method.id())) {
                                            "Authenticating"
                                        } else {
                                            "Authenticate"
                                        }
                                    }
                            }
                        }
                    }
                }

                // The other half of what the agent's auth block claims, and the
                // capability this panel drew for four rings with no method anywhere
                // in the tool behind it (§7.7). Labelled with the call it sends,
                // like every other affordance here.
                //
                // **A row, like the methods above it, and it was a button on
                // its own.** Right-aligned in an otherwise empty block, which
                // on a 288px rail is a control at the edge its neighbours' calls
                // are at and on a screen the width of this one is a button with
                // fourteen hundred pixels of nothing to its left — nothing
                // naming it, nothing saying what it costs, and no row for a
                // reader's eye to travel along to reach it. The sentence that
                // would have named it was `sr-only`: the only reader this
                // window told what `logout` does was the one not looking at it.
                //
                // So the call takes the name column, the consequence takes the
                // line under it, and the button stays where every other call on
                // this screen is — which is the shape the login rows above
                // already use, and the reason they are legible and this was not.
                // It is still not a third row of that list (§7.7): the hairline
                // and the block are unchanged, and what changed is that the
                // block has something in it besides a button.
                if logs_out {
                    div { class: "auth-method logout",
                            div {
                                strong { class: "mono", "logout" }
                                // What it does not say is the point of saying
                                // anything: the call answers `{}`, so a success
                                // is the agent taking back what it accepted and
                                // nothing more — not a claim that the next
                                // session will need a login, and not an
                                // instruction to the live session, which is left
                                // exactly as it is. Drawn now rather than read
                                // aloud, and it still describes the control:
                                // same id, same `aria-describedby`, one reader
                                // more.
                                p { class: "hint", id: LOGOUT_COSTS, {LOGOUT} }
                            }
                            button {
                                class: "btn btn-xs btn-quiet logout-action",
                                r#type: "button",
                                aria_describedby: LOGOUT_COSTS,
                                aria_busy: pending.contains(&AuthenticationCall::Logout),
                                disabled: !connected,
                                onclick: move |_| on_logout.call(()),
                                if pending.contains(&AuthenticationCall::Logout) {
                                    "Logging out"
                                } else {
                                    "Logout"
                                }
                            }
                    }
                }

                if !connected && (!methods.is_empty() || logs_out) {
                    p {
                        class: "hint availability",
                        role: "status",
                        "Authentication actions are unavailable while disconnected."
                    }
                }
            }
        }
    }
}

/// The advertised methods, by the names the agent gave them, or `None` when it
/// advertised none.
///
/// The login state names them because that is what the user has to go and do:
/// "log in" is not an instruction, and "log in with the browser, or with an API
/// key" is.
fn named_methods(methods: &[v1::AuthMethod]) -> Option<String> {
    let named: Vec<_> = methods.iter().map(v1::AuthMethod::name).collect();
    (!named.is_empty()).then(|| named.join(", "))
}

/// What became of authentication, in the agent's own words where it said any.
fn outcome(auth: &AuthState) -> Element {
    match auth {
        AuthState::Unasked => rsx! {},
        // The code as a number and the message as written: the reader of an
        // inspector is reading the agent's error, not this window's account of
        // one.
        AuthState::Required(error) => {
            let number = i32::from(error.code);
            rsx! {
                p { class: "detail", role: "status", aria_live: "polite",
                    code { "{number}" }
                    " {error.message}"
                }
            }
        }
        AuthState::Authenticated(method) => rsx! {
            p { class: "detail", role: "status", aria_live: "polite",
                "The agent accepted "
                code { "authenticate" }
                " with "
                code { "{method}" }
                "."
            }
        },
        // What it refused with, verbatim: an `authenticate` that failed is a
        // fact about this agent and this method, and the reason is the agent's
        // to give.
        AuthState::Refused { method, error } => rsx! {
            p { class: "detail", role: "status", aria_live: "polite",
                code { "{method}" }
                " was not accepted: {error}"
            }
        },
        // `AuthState` is non-exhaustive, and a state this window has not been
        // taught must not be rendered as one it has.
        _ => rsx! {
            p { class: "detail", role: "status", aria_live: "polite",
                "An authentication state this window has no rendering for."
            }
        },
    }
}

/// Where the login stands, in the two or three words the identity line has room
/// for.
///
/// **A state and not a section.** Authentication was a titled block with a
/// heading, a methods list and a logout, drawn whether or not the agent had
/// anything to log into — and for the majority of agents, which advertise no
/// method at all, that heading introduced one sentence saying so. Where it
/// stands belongs on the line that says who the agent is, which is where every
/// application that has an account puts it; what was *done* stays below, with
/// the agent's own words for it ([`outcome`]).
///
/// Nothing is said where nothing has happened. `Unasked` is not *logged out* —
/// it is an agent nobody has tried to log into, which for an agent advertising
/// no auth is every moment of its life, and a line reporting it would be this
/// window inventing a state the protocol has not got.
fn standing(auth: &AuthState) -> Element {
    let (said, tone) = match auth {
        AuthState::Required(_) => ("Login required", "standing-wanted"),
        AuthState::Authenticated(_) => ("Authenticated", "standing-done"),
        AuthState::Refused { .. } => ("Login refused", "standing-wanted"),
        // Including `Unasked`, and including a state this window has not been
        // taught: the block below says so in a sentence, and a word invented
        // for it here would be a second answer.
        _ => return rsx! {},
    };

    rsx! {
        span { class: "standing {tone}", "data-slot": "standing", "{said}" }
    }
}

/// The session lifecycle affordances, and whatever the agent last answered into
/// them (§7.5).
///
/// **Three parts, in the order a reader needs them**: the session they are in,
/// the sessions they could be in, and the way to a new one. It was one flat
/// run — two buttons, five paragraphs of consequence, then every session the
/// agent named as an undifferentiated list with the live one marked by a word
/// halfway down its row. That is the shape a rail leaves behind, where a block
/// is read top to bottom once; a surface opened to *change* the session is read
/// by looking for the one you are in and the one you want.
///
/// **The live session is stated whether or not it was listed.** It is what the
/// window is showing, and it used to appear only if `session/list` had been
/// pressed and only as one row among the others — so a dialog opened on a
/// working connection could say nothing at all about the session behind it. An
/// agent that never advertised listing still has a live session, and the calls
/// that end one carry nothing but its id.
///
/// **The way to a new session is at the foot**, where a dialog's own action
/// goes, because it is what a reader does when none of the above was what they
/// wanted. The roots the session opens with are beside it (§7.7), where the
/// agent advertised `additionalDirectories` and nowhere else.
pub(crate) fn sessions(
    listing: Listing<'_>,
    connected: bool,
    failed: Option<&Failed>,
    creating: Signal<String>,
    on_new_session: EventHandler<Option<Roots>>,
) -> Element {
    let roots = listing.roots;
    let on_list = listing.on_list;
    // The listed sessions, split on the one the window is showing. Two lists
    // rather than one with a marked row: which session you are in is the first
    // question this surface is opened with, and a word in the middle of the
    // fourth row is an answer a reader has to search for.
    let answered = listing
        .answered
        .map(|answered| answered.sessions.as_slice());
    // **While a listing is in flight, the live session keeps its id and loses
    // its description.** Which session the window is in is not the listing's to
    // say — it is the one that was opened — but where it runs and what it is
    // called came out of the answer now being replaced, and a row that went on
    // showing them would be presenting a stale page as the current one.
    let current = (!listing.pending)
        .then(|| {
            answered.and_then(|sessions| {
                sessions
                    .iter()
                    .find(|session| Some(&session.session_id) == listing.live)
            })
        })
        .flatten();
    let others: Vec<&v1::SessionInfo> = answered
        .unwrap_or_default()
        .iter()
        .filter(|session| Some(&session.session_id) != listing.live)
        .collect();

    rsx! {
        div { class: "dialog-body", "data-slot": "sessions",
            // **What is on this surface outlives the agent on purpose** (§5),
            // and until this line existed the whole of what it said about that
            // was a screen of controls nobody could press. The record stays —
            // an agent that went away mid-session is the finding, and a window
            // that emptied itself would be deleting the evidence to tidy up —
            // so what changes is that it says which of the two it is. The same
            // sentence in the same register as Session Settings and the login,
            // and only where there is a control it is about.
            if !connected {
                p {
                    class: "hint availability",
                    "data-slot": "availability",
                    role: "status",
                    "The connection has ended. These are the sessions the last agent reported; nothing here can be sent until another agent is running."
                }
            }

            // **Where you are**, drawn from the listing where the agent has
            // been asked for one and from the live id alone where it has not.
            if let Some(live) = listing.live {
                section { class: "rail-group", "data-slot": "session-current",
                    div { class: "rail-head",
                        h3 { class: "eyebrow", "Current session" }
                    }
                    ul { class: "session-list",
                        {session_row(current, live, &listing, connected)}
                    }
                }
            }

            // **What else the agent is holding**, behind the advertisement that
            // gates it: an agent that never claimed `session/list` has no
            // listing and no button to ask for one (§7.5).
            if listing.advertised {
                section { class: "rail-group session-listing",
                    header { class: "rail-head",
                        h3 { class: "eyebrow", "Other sessions" }
                        if !others.is_empty() {
                            span { class: "rail-count", "{others.len()}" }
                        }
                        button {
                            class: "btn btn-xs btn-quiet",
                            r#type: "button",
                            disabled: !connected,
                            onclick: move |_| on_list.call(None),
                            // Beside the method's own name and never instead of
                            // it: what a control sends is the label (§7.5), and
                            // the glyph is what makes *ask again* findable in a
                            // row of words that are all method names.
                            span { class: "disclosure-icon", aria_hidden: "true",
                                Icon { class: "icon", icon: HiRefresh }
                            }
                            "session/list"
                        }
                    }
                    {listed(&listing, &others, connected)}
                }
            }

            // The agent's own refusal, or the news that it is no longer there:
            // an affordance that did nothing and said nothing would be the
            // window keeping a secret core is there to tell. The call is named
            // because there are four of them here — an agent that advertised a
            // capability and then failed it is a finding about *that* method,
            // and one is worth reading precisely because the other three may
            // have worked.
            if let Some(failed) = failed {
                p { class: "alert alert-error", "data-slot": "session-failure", role: "alert",
                    code { "{failed.method}" }
                    " failed: {failed.error}"
                }
            }

            footer { class: "rail-group",
                div { class: "session-actions",
                    button {
                        class: "btn btn-sm btn-filled",
                        r#type: "button",
                        disabled: !connected,
                        // What pressing it costs, on the control rather than
                        // under it: the consequence of a button read by
                        // everyone who opens this and needed by a reader once.
                        // The words are unchanged and still in the
                        // accessibility tree, described by the paragraph below
                        // rather than merely near it.
                        title: SESSION_NEW_COSTS,
                        aria_describedby: "session-new-costs",
                        // `None` where the agent advertised no control, so that a click
                        // says *no roots were asked for* rather than *the empty control
                        // was*: for a session that does not exist yet the two ask for
                        // the same thing, and only one of them is true.
                        onclick: move |_| on_new_session.call(roots.then(|| Roots::supplied(creating()))),
                        span { class: "disclosure-icon", aria_hidden: "true",
                            Icon { class: "icon", icon: HiPlus }
                        }
                        "session/new"
                    }

                    // Said once for every roots control in this block — the one
                    // beside `session/new` and the one on each listed row —
                    // rather than on each of them.
                    if roots {
                        details { class: "disclose bare roots-new",
                            summary {
                                aria_label: "Edit additionalDirectories for a new session",
                                title: "Edit additionalDirectories for a new session",
                                {crate::disclosure::chevron()}
                                code { "additionalDirectories" }
                            }
                            div { class: "disclose-body",
                                // Inside the disclosure, which is where the field it
                                // is a rule about is. Above it, it was a paragraph
                                // about a box nobody had opened yet — always drawn,
                                // read once, and read at the moment it is least
                                // useful.
                                {roots_rule()}
                                {roots_field(
                                    creating,
                                    false,
                                    "additionalDirectories for a new session".to_owned(),
                                )}
                            }
                        }
                    }
                }

                // The same sentence the control carries, kept in the document
                // for the reader who is not pointing at anything: a tooltip is
                // not an accessible name and `aria-describedby` needs something
                // to point at.
                p { class: "sr-only", id: "session-new-costs", {SESSION_NEW_COSTS} }
            }
        }
    }
}

/// One session, as a row of either list.
///
/// **The same row for the live session and for the rest**, which is what keeps
/// the two lists two lists rather than two designs: every affordance the agent
/// advertised is on both, gated the way §7.5 gates it, and what differs is the
/// heading above the list and the word the row says about itself.
///
/// `info` is `None` for a live session the agent has not listed — there is no
/// `SessionInfo` to draw, and no reopen either, because reopening one hands the
/// agent back its own description of it. What the row can still carry is the
/// two calls that take nothing but an id.
fn session_row(
    info: Option<&v1::SessionInfo>,
    id: &v1::SessionId,
    sessions: &Listing<'_>,
    connected: bool,
) -> Element {
    let Listing {
        live,
        restores,
        closes,
        deletes,
        roots,
        on_open,
        on_close,
        on_delete,
        ..
    } = *sessions;
    let current = live == Some(id);

    rsx! {
        li {
            key: "{id}",
            class: if current { "session-row live" } else { "session-row" },
            // The drawing's row: a dot for which one this is, the title the
            // agent gave it, and what it last said about it at the far end.
            div { class: "session-head",
                span {
                    class: if current { "dot dot-live" } else { "dot dot-idle" },
                    aria_hidden: "true",
                }
                span { class: "session-title",
                    match info.and_then(|info| info.title.as_ref()) {
                        Some(title) => rsx! { "{title}" },
                        None => rsx! { span { class: "cleared", "untitled session" } },
                    }
                }
                // What a row *is*, in words, because a dot is decoration (§7.5).
                span { class: "sr-only",
                    if current { "Current session. " } else { "Available session. " }
                }
                span { class: "session-meta",
                    if let Some(updated) = info.and_then(|info| info.updated_at.as_ref()) {
                        "updated {updated}"
                    } else if info.is_none() && !sessions.pending {
                        "not in a listing"
                    }
                }
            }
            div { class: "session-line",
                code { class: "session-id", "{id}" }
                if let Some(info) = info {
                    span { class: "session-id", title: "{info.cwd.display()}", "{info.cwd.display()}" }
                }
            // **Every control on this row is bounded**, which is the one thing
            // the row's hierarchy was saying that nobody meant. Reopen and
            // close were `ghost` — a label and no boundary — beside a `delete`
            // that carries the outline its tone comes with, so a row of three
            // calls read as one control with two headings over it, and the
            // control it read as was the destructive one. They are one group of
            // push buttons now and the tone is the only thing that still tells
            // them apart, which is the difference there actually is.
                div { class: "session-actions",
                if let (Some(info), Some(how)) = (info, restores) {
                    Reopen {
                        session: info.clone(),
                        how,
                        roots,
                        connected,
                        on_open,
                    }
                }
                // Each on its own claim: an agent that advertised one of the
                // two is offered that one, and the row is a report of what it
                // said rather than a menu of what a session can have done to
                // it. **A step down from the way in, and never hidden.** They
                // were revealed on hover, which is the rule for a control that
                // repeats something already on the row and the wrong one for
                // the only way to do a thing: what is not drawn cannot be
                // looked for, and a list that has to be swept with a pointer to
                // be read is a list that cannot be read at a glance. `btn-xs`
                // against the reopen's `btn-sm` is the difference that was
                // meant all along.
                if closes {
                    button {
                        class: "btn btn-xs btn-quiet session-end",
                        r#type: "button",
                        aria_label: "session/close session {id}",
                        aria_describedby: ENDING,
                        disabled: !connected,
                        onclick: {
                            let id = id.clone();
                            move |_| on_close.call(id.clone())
                        },
                        "session/close"
                    }
                }
                if deletes {
                    button {
                        class: "btn btn-xs btn-quiet btn-bad session-end",
                        r#type: "button",
                        aria_label: "session/delete session {id}",
                        aria_describedby: ENDING,
                        disabled: !connected,
                        onclick: {
                            let id = id.clone();
                            move |_| on_delete.call(id.clone())
                        },
                        "session/delete"
                    }
                }
                }
            }
        }
    }
}

/// What the listing affordance is made of: what the agent said it can do, what
/// it last answered, and the two ways of asking it for more.
///
/// One value rather than a fistful of arguments threaded through two functions
/// — they are one question the panel asks about sessions, and what a row can do
/// is only meaningful beside the row it does it to.
#[derive(Clone, Copy)]
pub(crate) struct Listing<'a> {
    /// Whether the agent advertised `session/list` — core's answer, and the
    /// gate this whole group sits behind.
    advertised: bool,
    /// What it answered the last time it was asked, if it has been.
    answered: Option<&'a SessionListing>,
    /// The live Session, used only to state current versus available in words.
    live: Option<&'a v1::SessionId>,
    /// Whether the answer, or its next page, is currently in flight.
    pending: bool,
    /// How a listed session is opened, or `None` where the agent advertised no
    /// way to.
    restores: Option<Restore>,
    /// Whether the agent advertised `session/close`, and whether it advertised
    /// `session/delete` — two gates because they are two claims, each made in
    /// its own field (§7.5).
    closes: bool,
    deletes: bool,
    /// Whether the agent advertised `additionalDirectories` — the gate on the
    /// roots control, beside the create button and on every row (§7.7).
    roots: bool,
    on_list: EventHandler<Option<Cursor>>,
    on_open: EventHandler<Opening>,
    on_close: EventHandler<v1::SessionId>,
    on_delete: EventHandler<v1::SessionId>,
}

/// The sessions the agent named that are not the one in front of the reader,
/// and the states that stand in for them.
///
/// **A listed session is a way into that session** (§7.5), which is what makes
/// the listing actionable rather than text to read. One affordance per row and
/// never two: `session/load` and `session/resume` restore the same thing and
/// differ only in whether the conversation replays first, so the *call* carries
/// the difference ([`Restore`]) and the screen states it — [`opening`] is where
/// it is stated. Which call this agent gets is core's answer, preference and
/// all; a row is offered it whenever the agent advertised one, and an agent that
/// advertised a method it never implemented is driven anyway, because its
/// refusal is the finding.
fn listed(sessions: &Listing<'_>, others: &[&v1::SessionInfo], connected: bool) -> Element {
    let Listing {
        answered,
        pending,
        restores,
        closes,
        deletes,
        on_list,
        ..
    } = *sessions;
    let next_page = answered.and_then(|listing| listing.next_page.clone());

    rsx! {
        if pending {
            p {
                class: "alert",
                "data-slot": "session-list-status",
                role: "status",
                aria_live: "polite",
                span { class: "status status-info", aria_hidden: "true" }
                "Loading sessions."
                if answered.is_some() {
                    " Previous Session rows are stale and hidden; the prior count and any paging remain visible until the agent answers."
                }
            }
        } else { match answered {
            // Advertised and unasked: the button is here because the agent
            // said it lists, not because there is anything to see yet.
            None => rsx! {
                // A line rather than a bounded notice. The other two states of
                // this region are news that arrived — a page loading, a page
                // that came back empty — and a box is right for those; this one
                // is what the surface says before anything has been asked, and
                // drawing it as an alert made *nothing has happened yet* the
                // most boxed-in thing in the dialog.
                p { class: "hint", "data-slot": "session-list-status", role: "status", "The agent says it can list its sessions." }
            },
            Some(listing) if listing.sessions.is_empty() => rsx! {
                p { class: "alert", "data-slot": "session-list-status", role: "status", aria_live: "polite", "The agent listed no sessions." }
            },
            // Listed, and every session it named is the one already in front of
            // the reader. Said rather than left as a gap under a heading: an
            // empty region where rows were is a listing that failed, and this
            // one succeeded.
            Some(_) if others.is_empty() => rsx! {
                p { class: "hint", "data-slot": "session-list-status", role: "status", aria_live: "polite",
                    "The agent listed no sessions besides this one."
                }
            },
            Some(_) => rsx! {
                {opening(restores)}
                {ending(closes, deletes)}
                ul { class: "session-list",
                    for session in others.iter().copied() {
                        {session_row(Some(session), &session.session_id, sessions, connected)}
                    }
                }
            },
        } }

        // A page the agent kept back. The cursor goes straight back to core,
        // unread: what is inside it is the agent's business, and a client that
        // parsed it would be guessing at a token no agent in the survey has
        // ever sent.
        SessionPaging {
            available: next_page.is_some(),
            connected,
            on_next: move |()| on_list.call(next_page.clone()),
        }
    }
}

/// The Affordance for a page the Agent kept back.
///
/// Availability is separate from the callback so presentation tests never need
/// to manufacture or inspect the opaque [`Cursor`] the callback closes over.
#[component]
fn SessionPaging(available: bool, connected: bool, on_next: EventHandler<()>) -> Element {
    rsx! {
        if available {
            div { class: "session-line",
                span { class: "hint",
                    if connected {
                        "The agent has more sessions."
                    } else {
                        "The agent has more sessions. Next page is unavailable while disconnected."
                    }
                }
                button {
                    class: "btn btn-xs btn-quiet next-page",
                    r#type: "button",
                    aria_label: "Ask the agent for the next page of sessions",
                    disabled: !connected,
                    onclick: move |_| on_next.call(()),
                    "Next page"
                }
            }
        }
    }
}

/// The way into one listed session, and the roots it will be reopened with
/// (§7.7).
///
/// **A component rather than more markup in the row above**, because the roots
/// are state: each row's control holds its own list, and a list somebody typed
/// survives the listing being asked for again — which it is, on the tool's own
/// initiative, after every close and delete.
///
/// **Prefilled with what the listing reported**, so that reopening stays a
/// reopen unless the user deliberately changes it (§7.5). Changing it is the
/// point of the control and not a hazard: the schema permits a list that differs
/// from any previously reported one as long as the `cwd` matches, and what the
/// agent does with a list it did not report is the finding.
///
/// **A control nobody changed asks for nothing**, and says so with `None` rather
/// than by handing back a copy of what it was prefilled with. That is what keeps
/// a default reopen the *listing's own* roots however this window rendered them
/// — a path this control could not draw faithfully would otherwise reach the
/// wire as the drawing rather than as the path.
///
/// **The control is gated and the reopen is not.** Where the agent advertised no
/// `additionalDirectories` there is nothing here to type in and the click says
/// `None` for that reason instead, which is core's cue to hand the agent back
/// the roots it reported — because a reopen gated into asking for less would be
/// reopening a different session from the one it named.
///
/// **What the user typed survives the listing being asked for again**, which it
/// is on the tool's own initiative after every close and delete: the row keeps
/// its own state for as long as the session keeps its place in the listing, and
/// a control that emptied itself out from under the typing would be spending the
/// user's work on housekeeping they did not ask for.
#[component]
fn Reopen(
    /// The session this row is about, as the agent described it.
    session: v1::SessionInfo,
    /// Which call reopens it — core's answer, preference included (§7.5).
    how: Restore,
    /// Whether the agent advertised `additionalDirectories`.
    roots: bool,
    connected: bool,
    on_open: EventHandler<Opening>,
) -> Element {
    let reported = Roots::reported(&session.additional_directories);
    let supplying = use_signal(|| reported.text().to_owned());
    let session_id = session.session_id.to_string();
    let open_label = format!("{} session {session_id}", how.method());
    let roots_label = format!("Edit additionalDirectories for session {session_id}");
    let field_label = format!("additionalDirectories for session {session_id}");
    let opened = session.clone();

    // What the agent said this session's roots are, as one line. The summary
    // carries it so that shutting the control does not take away the agent's own
    // words: those are what the field is prefilled from and what will cross the
    // wire if nobody edits them, and a disclosure that hid them would be buying
    // room with the evidence (§7.7).
    let announced = reported.text().trim().replace('\n', " · ");

    rsx! {
        button {
            class: "btn btn-xs btn-quiet btn-go reopen",
            r#type: "button",
            aria_label: "{open_label}",
            // What pressing it costs, on the control rather than above the
            // list of them (`OPENING`).
            aria_describedby: OPENING,
            disabled: !connected,
            onclick: move |_| {
                on_open
                    .call(Opening {
                        session: opened.clone(),
                        roots: asking(roots, &supplying(), &reported),
                    })
            },
            "{how.method()}"
        }
        if roots {
            // **Under the call rather than in front of it.** These are what the
            // reopen will carry, so they belong to that button — and drawn
            // first, at the head of the row's action column, the disclosure was
            // the widest thing on the row: a modifier read as the row's
            // subject, with the session's own working directory squeezed into
            // an ellipsis beside it.
            //
            // **Shut by default, and one per row.** Eleven of these textareas —
            // one for `session/new` and one for each listed session — were two
            // and a half screens of the same control repeated, on a rail already
            // six screens long. What it costs is a click before the roots of a
            // *particular* listed session can be edited; what it does not cost
            // is knowing what they are, which the summary says whether it is
            // open or shut, or that they can be edited at all, which is what the
            // control being named rather than hidden is for.
            details { class: "disclose bare roots-supplied", "data-slot": "roots-supplied",
                summary { aria_label: "{roots_label}", title: "{roots_label}",
                    {crate::disclosure::chevron()}
                    // No `reported` badge in front of it. The badge and the
                    // words after it said one thing twice — *reported ·
                    // additionalDirectories · the agent reported none* — which
                    // on a row is three chips for one fact.
                    code { "additionalDirectories" }
                    // The agent's own words get the row's colour, not a hint's:
                    // what it reported is content here, and the word saying it
                    // reported nothing is this window talking.
                    if announced.is_empty() {
                        span { class: "hint", " none reported" }
                    } else {
                        span { class: "mono", " {announced}" }
                    }
                }
                div { class: "disclose-body",
                    {roots_field(supplying, false, field_label)}
                }
            }
        }
    }
}

/// What a click on a listing row asks for: the roots the user supplied, or
/// `None` for the ones the listing reported (§7.7).
///
/// **Two ways to mean the default and one to mean a change.** An agent that
/// advertised no `additionalDirectories` drew no control, and a control nobody
/// changed is not a change — both say `None`, which is core's cue to hand the
/// agent back the roots it reported rather than a rendering of them. A control
/// the user *emptied* is a change like any other and asks for no roots at all,
/// which is the one thing a fallback to the reported list would get wrong.
fn asking(advertised: bool, supplied: &str, reported: &Roots) -> Option<Roots> {
    let supplied = Roots::supplied(supplied);
    (advertised && &supplied != reported).then_some(supplied)
}

/// The roots control: the list of additional workspace roots a session is to be
/// opened with, as the user supplies them (§7.7).
///
/// One control for both places it appears, because it is one thing: the field
/// `session/new`, `session/load` and `session/resume` all carry, and the same
/// rules for reading it either way.
///
/// **It says the paths must be absolute**, and says what happens to one that is
/// not, because that is the rule the user would otherwise learn from the agent's
/// error — and would learn wrongly, since the inspector resolves a relative path
/// against the session's own `cwd` before it sends one rather than letting a
/// malformed request cross ([`Roots`]).
///
/// **It stays live when the agent is gone**, unlike the button beside it. A
/// field is not an affordance — nothing here sends anything — and the spawn
/// form's own fields keep the same rule while its Launch button is disabled: a
/// list typed against an agent that has just died is a list the next one is
/// asked for, and a control that emptied itself out from under the user would be
/// this window throwing their typing away to make a point the button already
/// makes.
fn roots_field(mut roots: Signal<String>, named: bool, field_label: String) -> Element {
    rsx! {
        label { class: "form-field", "data-slot": "roots",
            // The field it supplies, except where the control is inside a
            // disclosure whose summary already names it. Left out of the markup
            // rather than hidden by a rule: a fact kept in the document and
            // painted away is the dimness rule broken by another name, which is
            // the reason `hushed` removes the fourth fact rather than dimming it.
            div { class: "form-label",
                if named {
                    code { class: "form-key", "additionalDirectories" }
                } else {
                    span { class: "form-key", "Open with these roots" }
                }
                span { class: "form-note", "one absolute path per line" }
            }
            textarea {
                class: "textarea",
                aria_label: "{field_label}",
                rows: 2,
                value: "{roots}",
                spellcheck: false,
                placeholder: "/srv/data",
                oninput: move |event| roots.set(event.value()),
            }
        }
    }
}

/// What every one of those controls is held to, said once above all of them
/// (§7.7).
///
/// **It is a sentence about this tool, so it is said once.** The rule is the
/// inspector's — the schema names the session's `cwd` as the base, and this
/// window resolves a relative path against it before anything crosses rather
/// than letting a malformed request go out ([`Roots`]) — and a control drawn on
/// `session/new` and again on every listed session repeated it verbatim on each.
/// An agent advertising `additionalDirectories` with ten sessions listed printed
/// this paragraph eleven times, which is the shape §7.7 already refused for the
/// fourth fact: one fact about the inspector drawn as eleven about the agent.
///
/// The control keeps the half that is about *its own* field — one absolute path
/// per line, beside the box it is typed in — because that half is a label rather
/// than a rule, and a label belongs on the thing it labels.
/// What pressing `session/new` costs, in the words the rail used to print.
///
/// One string, because it is now said in two places that must not drift: on the
/// control as its tooltip, and in the document as what the control is described
/// by.
pub(crate) const SESSION_NEW_COSTS: &str = "Opens and switches to a new session. A turn still running is cancelled first; the trace keeps every frame.";

/// What `logout` costs, and the id the button points at for it.
const LOGOUT: &str = "Asks the agent to give back what it accepted. It carries no session id and leaves the live session alone — what the agent does with a session it has logged out of is the thing to watch.";
const LOGOUT_COSTS: &str = "logout-costs";

fn roots_rule() -> Element {
    rsx! {
        p { class: "hint", "data-slot": "roots-rule",
            "Each path must be absolute. A relative one is resolved against the session's "
            code { "cwd" }
            "."
        }
    }
}

/// What the row's own controls cost, by the ids they point at.
///
/// **The same treatment `session/new` already gets.** These sentences were five
/// visible paragraphs above the list — written for a rail read once, and a wall
/// between a reader and the thing they opened this to press. They are on the
/// controls they are about now: on the tooltip for a pointer, in the document
/// for `aria-describedby`, and the accessibility tree carries exactly what it
/// carried before. What stays visible is the one that is not about a control at
/// all — an agent that advertised no way back into a session has an absence to
/// explain, and an absence has nothing to hang a description on.
pub(crate) const OPENING: &str = "session-open-costs";
pub(crate) const ENDING: &str = "session-end-costs";

/// What opening one of the listed sessions will do, said once above them all.
///
/// **Replay is a property of the call, and this is where the property is
/// read.** The two calls restore the same thing; the one difference is whether
/// the conversation arrives with it, and that difference decides whether the
/// timeline below can be rebuilt at all — so it is stated in words rather than
/// left to be inferred from what does or does not turn up.
///
/// Where the agent offers only `session/resume`, saying so is the whole point:
/// a resumed session's **empty timeline is correct**, and a screen that let the
/// reader discover that on their own would have them reading a fault in the
/// inspector instead of a fact about the agent.
///
/// Which sentence is drawn is decided by [`Restore::replays`] and not by which
/// call it is — the property is core's, and a window that re-derived it from
/// the method name would be a second place to get it wrong.
fn opening(restores: Option<Restore>) -> Element {
    rsx! {
        p {
            class: if restores.is_some() { "sr-only" } else { "hint" },
            id: OPENING,
            {match restores {
                // Gated on the advertisement, here as everywhere: an agent that
                // claimed neither is asked for neither, and its listing is a
                // description with nothing to click.
                None => rsx! {
                    "The agent advertised neither "
                    code { "session/load" }
                    " nor "
                    code { "session/resume" }
                    ", so there is no way in to a session it already has."
                },
                // Split on the *property* rather than on which call it is, which
                // is what makes this two sentences and not one per method:
                // `Restore` is non-exhaustive, and a way of opening a session
                // that this window has never heard of still either brings the
                // conversation with it or does not.
                Some(how) if how.replays() => rsx! {
                    "Opening one makes it the live session. "
                    code { "{how.method()}" }
                    " replays its conversation into the timeline before it answers, so what was said comes back with it."
                },
                Some(how) => rsx! {
                    "Opening one makes it the live session. "
                    strong { "This agent does not support viewing previous messages" }
                    " — it advertised "
                    code { "{how.method()}" }
                    ", which restores a session without replaying it, so the timeline stays empty. That is correct, not a failure."
                },
            }}
        }
        if restores.is_some() {
            p { class: "sr-only",
                "It is the switch "
                code { "session/new" }
                " is: a turn still running is cancelled first, and the timeline starts again — the trace keeps every frame."
            }
        }
    }
}

/// What ending one of the listed sessions will do, said once above them all.
///
/// **Three things a reader cannot get from the button** (§7.5). That the
/// listing is asked again afterwards, because whether a closed session is still
/// there is the agent's answer and the reason to press the button at all. That
/// ending the *live* session is allowed and leaves the inspector with none open
/// — the timeline goes with it and the trace keeps every frame. And that the
/// corners the specification leaves undefined are offered rather than withheld,
/// so a button that does something surprising is the agent being surprising.
///
/// Nothing here says what the agent *should* do with any of it, because the
/// specification does not: close and delete are advertised capabilities with
/// undefined edges, and a window that promised an outcome would be inventing
/// one.
fn ending(closes: bool, deletes: bool) -> Element {
    let named = match (closes, deletes) {
        (true, true) => "session/close and session/delete",
        (true, false) => "session/close",
        (false, true) => "session/delete",
        // The agent claimed neither, so there is nothing to explain: the rows
        // below carry no way to end a session.
        (false, false) => return rsx! {},
    };

    rsx! {
        p { class: "sr-only", id: ENDING,
            code { "{named}" }
            " went out because the agent advertised it, whatever state the session is in. The listing is asked for again afterwards, because what the agent still claims to hold is the thing worth seeing."
            " Ending the live session is allowed and leaves no session open — the timeline goes with it, and the trace keeps every frame."
        }
    }
}

#[cfg(test)]
mod tests {
    use acp_inspector_core::ProtocolVersion;

    use super::*;

    /// An agent that described itself as `capabilities` say and offered no way
    /// to log in.
    ///
    /// The `initialize` result rather than the capability object, because one
    /// of the rows this panel draws is claimed outside it: `authenticate` is
    /// gated on `authMethods`, which sits beside `agentCapabilities` on the
    /// result rather than inside it (§7.1).
    fn described(capabilities: v1::AgentCapabilities) -> v1::InitializeResponse {
        v1::InitializeResponse::new(ProtocolVersion::V1).agent_capabilities(capabilities)
    }

    /// Everything a v1 agent can advertise, advertised — which is what Testy
    /// advertises, held here as a value because this crate has no runtime to
    /// drive an agent with (§5, §12). That the real agent still claims every one
    /// of them is asserted where an agent can be driven, in
    /// `crates/core/tests/capability.rs`'s
    /// `testy_advertises_every_capability_the_record_is_keyed_on`; the
    /// capability objects are empty ones there and here, because `{}` is the
    /// whole of the claim.
    ///
    /// **The auth method is one of them**, and it is why this is an `initialize`
    /// result: an agent that claimed every capability and named no way to log in
    /// has not advertised everything, and the `authenticate` row would say so.
    fn everything() -> v1::InitializeResponse {
        described(
            v1::AgentCapabilities::new()
                .load_session(true)
                .prompt_capabilities(
                    v1::PromptCapabilities::new()
                        .image(true)
                        .audio(true)
                        .embedded_context(true),
                )
                .mcp_capabilities(v1::McpCapabilities::new().http(true).sse(true))
                .session_capabilities(
                    v1::SessionCapabilities::new()
                        .list(v1::SessionListCapabilities::new())
                        .delete(v1::SessionDeleteCapabilities::new())
                        .additional_directories(v1::SessionAdditionalDirectoriesCapabilities::new())
                        .resume(v1::SessionResumeCapabilities::new())
                        .close(v1::SessionCloseCapabilities::new()),
                )
                .auth(v1::AgentAuthCapabilities::new().logout(v1::LogoutCapabilities::new())),
        )
        .auth_methods(vec![a_login()])
    }

    /// Every Advertisement plus the identity fields an Agent may supply.
    fn identified() -> v1::InitializeResponse {
        everything().agent_info(v1::Implementation::new("test-agent", "1.2.3").title("Test Agent"))
    }

    /// The one way in an agent that offers one offers, as Testy's own does.
    fn a_login() -> v1::AuthMethod {
        v1::AuthMethod::Agent(v1::AuthMethodAgent::new(
            "browser",
            "Log in with the browser",
        ))
    }

    /// An agent that answered `initialize` and claimed nothing in it — the other
    /// half of every fixture pair here, because a claim not made is an answer
    /// too.
    fn nothing() -> v1::InitializeResponse {
        described(v1::AgentCapabilities::default())
    }

    /// An agent nobody has driven anything on, which is what every test about
    /// the *claims* is about: the fourth fact is what happened afterwards, and
    /// these are the rows before anything happened.
    fn undriven() -> DrivenRecord {
        DrivenRecord::default()
    }

    /// The rows an agent's own account of itself is drawn as, with nothing
    /// driven and nobody logged in.
    fn claims(agent: &v1::InitializeResponse) -> Vec<(&'static str, Vec<Claimed>)> {
        advertised(
            agent,
            &undriven(),
            &AuthState::default(),
            agent.agent_capabilities.session_capabilities.list.is_some(),
        )
    }

    /// Every row the display draws, in the order it draws them: the group it is
    /// under, where it was claimed, in what shape, and the claim itself.
    fn rows(agent: &v1::InitializeResponse) -> Vec<(&'static str, &'static str, Shape, Claim)> {
        claims(agent)
            .into_iter()
            .flat_map(|(group, runs)| {
                runs.into_iter().flat_map(move |run| {
                    run.claims
                        .into_iter()
                        .map(move |claim| (group, run.at, run.shape, claim))
                })
            })
            .collect()
    }

    /// The rows of the sessions group, which is the one this ring is about.
    fn session_rows(agent: &v1::InitializeResponse) -> Vec<(&'static str, Shape, Claim)> {
        rows(agent)
            .into_iter()
            .filter(|(group, ..)| *group == "Sessions")
            .map(|(_, at, shape, claim)| (at, shape, claim))
            .collect()
    }

    fn marks(agent: &v1::InitializeResponse) -> Vec<bool> {
        rows(agent)
            .into_iter()
            .map(|(.., claim)| claim.advertised)
            .collect()
    }

    #[test]
    fn an_agent_that_advertises_everything_reads_as_advertising_everything() {
        // Every row wired to the field it names: an agent that offered all of
        // it has no unlit row, and an agent that offered none of it has no lit
        // one. What is *displayed* is the agent's claim, so a row reading the
        // wrong field would be this window making a claim of its own.
        assert!(
            marks(&everything()).iter().all(|yes| *yes),
            "{:?}",
            claims(&everything())
        );
        assert!(
            !marks(&nothing()).iter().any(|yes| *yes),
            "{:?}",
            claims(&nothing())
        );
    }

    #[test]
    fn every_session_capability_v1_defines_is_a_row() {
        // The whole set, and the same set either way: an agent that advertised
        // none of them is described by six rows reading "not advertised", not
        // by an empty list. Which is the point of the screen — a capability
        // left out reads as a question nobody asked rather than as an answer.
        let named = |agent| {
            session_rows(&agent)
                .into_iter()
                .map(|(_, _, claim)| claim.name)
                .collect::<Vec<_>>()
        };

        assert_eq!(
            named(nothing()),
            [
                "session/load",
                "session/list",
                "session/resume",
                "session/close",
                "session/delete",
                "additionalDirectories",
            ]
        );
        assert_eq!(named(everything()), named(nothing()));
    }

    #[test]
    fn an_agent_that_advertised_none_of_them_has_every_session_row_read_as_not_advertised() {
        // The second half of the fixture pair this ring is verified against
        // (#77): the agent that claimed nothing. Every session row is here and
        // every one of them is unlit.
        let rows = session_rows(&nothing());

        assert_eq!(rows.len(), 6);
        assert!(
            rows.iter().all(|(_, _, claim)| !claim.advertised),
            "{rows:?}"
        );
    }

    /// The panel over an agent that described itself, rendered — the screen
    /// itself and not a value on the way to it.
    fn shown(agent: v1::InitializeResponse, connected: bool) -> String {
        rendered(agent, connected, None, None)
    }

    /// One advertiser's rows, split off the rendered panel.
    ///
    /// The two advertisements sit in one column and are the same markup by
    /// design (#86), so a test asking about one of them has to say which — and
    /// a test that asked the whole panel for "advertised" would be answered by
    /// the other one.
    fn advertisement(html: &str, whose: &str) -> String {
        let block = html
            .split_once(&format!(r#"data-advertiser="{whose}""#))
            .unwrap_or_else(|| panic!("the {whose}'s advertisement is drawn: {html}"))
            .1;

        // Up to whatever claim block comes next, which is the other
        // advertiser's or the authentication one under both: only a block of
        // claims says which claim it is, so the next one is always a sibling.
        block
            .split_once(r#"data-claim="#)
            .map_or(block, |(block, _)| block)
            .to_owned()
    }

    #[test]
    fn what_this_client_advertised_is_shown_beside_what_the_agent_advertised() {
        // **The panel has been auditing one of the two parties** (#86). A
        // boolean config option arrives from an agent that gates on this
        // client's claim, so without the claim on screen the option shape has
        // no visible cause — and a tool whose thesis is that claims should be
        // legible was hiding its own.
        let panel = shown(everything(), true);

        let client = advertisement(&panel, "client");
        assert!(
            client.contains("boolean") && client.contains(&format!(r#"{STATE}advertised"#)),
            "the data shape this client claims about itself: {client}"
        );
        // And the service it performs, a mode at a time (§7.8) — what an agent
        // gates an elicitation on, drawn where an agent author finds out why
        // one arrived here at all.
        for mode in ["form", "url"] {
            assert!(
                client.contains(mode),
                "the {mode} elicitation mode is claimed on screen too: {client}"
            );
        }
        // And the agent's own account is still here, beside it and unmerged:
        // who claimed a thing is the point of showing it.
        let agent = advertisement(&panel, "agent");
        assert!(
            agent.contains("session/load"),
            "the agent's account is still drawn: {agent}"
        );
        assert!(
            !agent.contains("configOptions"),
            "and the client's claim is not folded into it: {agent}"
        );
    }

    #[test]
    fn the_filesystem_and_terminal_services_are_drawn_as_declined() {
        // The MVP stance, narrowed and not abandoned (§7.3): the claim above is
        // a *data shape* this client renders, and the *services* it would have
        // to perform are still declined — on screen as on the wire, because a
        // panel that showed only the claim would read as a client that offers
        // more than it does.
        //
        // Two of the three §7.3 names, and now the whole of that row: the
        // third left it with the fifth ring, which is what the two elicitation
        // claims above are (§7.8). These two stay because they are what the
        // sentence always meant — a filesystem this tool cannot mediate and a
        // terminal it does not embed.
        let client = advertisement(&shown(everything(), true), "client");

        for declined in ["fs/read_text_file", "fs/write_text_file", "terminal/*"] {
            let row = client
                .split_once(declined)
                .unwrap_or_else(|| panic!("{declined} has a row: {client}"))
                .1;
            let said = row.split_once(STATE).expect("the row says its state").1;
            assert!(
                said.starts_with("not advertised"),
                "and says it was declined: {row}"
            );
        }
    }

    #[test]
    fn terminal_authentication_is_drawn_as_declined() {
        // Schema 1.7 makes this a stable client claim. The inspector cannot
        // reproduce the Agent invocation in an interactive terminal (§7.1),
        // and the reader must see that refusal alongside the other claims.
        let client = advertisement(&shown(everything(), true), "client");
        let row = client
            .split_once("Terminal authentication")
            .expect("terminal authentication has its own row")
            .1;
        let said = row.split_once(STATE).expect("the row says its state").1;
        assert!(said.starts_with("not advertised"), "{row}");
    }

    #[test]
    fn this_clients_claim_does_not_wait_for_an_agent_to_describe_itself() {
        // It is fixed for every connection and it goes out in the *request*
        // (§7.6), so it is knowable before an agent exists and unchanged by
        // anything one says. A panel that only drew it beside an answer would
        // imply the claim was one — and an agent that failed to describe itself
        // was still told this.
        let panel = rendered_without_an_agent();
        let client = advertisement(&panel, "client");

        assert!(
            client.contains("boolean") && client.contains(&format!(r#"{STATE}advertised"#)),
            "what the next agent will be told, before there is one: {client}"
        );
    }

    #[test]
    fn the_clients_advertisement_is_drawn_as_plainly_as_the_agents() {
        // **The same rows, at the same weight** (#86, and #83's rule about the
        // half of an answer that is an absence). The client's claim is not an
        // aside beside the agent's: it is one of the two accounts this panel
        // exists to put side by side, so it is the same markup — the same list,
        // the same state in words, the same mark as decoration over it — and
        // whatever a stylesheet does to one it does to both.
        let panel = shown(everything(), true);
        let client = advertisement(&panel, "client");
        let rows = capability_rows(&client);

        assert!(!rows.is_empty(), "the client's claims are rows: {client}");
        for row in rows {
            assert!(row.contains(STATE), "each says its state in words: {row}");
        }

        // Nothing about the two blocks says one of them is a footnote: the
        // shape each claim was made in is stated for the client the way it is
        // for the agent.
        assert!(
            client.contains(SHAPE),
            "the shape of the client's claims is stated too: {client}"
        );
    }

    /// Two sessions, as an agent that lists them would have described them.
    ///
    /// The rows the open affordance hangs off, and the only thing this crate
    /// can hold them in: it has no runtime to drive an agent with (§5, §12), so
    /// what a real listing looks like is asserted where one can be asked for
    /// (`crates/core/tests/restore.rs`).
    fn two_sessions() -> SessionListing {
        let mut listing = SessionListing::default();
        listing.sessions = vec![
            v1::SessionInfo::new("sess-1", "/tmp").title("The first one"),
            v1::SessionInfo::new("sess-2", "/tmp"),
        ];
        listing
    }

    /// The panel, with whatever the agent answered into its listing and
    /// whichever way in it advertised.
    fn rendered(
        agent: v1::InitializeResponse,
        connected: bool,
        restores: Option<Restore>,
        listing: Option<SessionListing>,
    ) -> String {
        rendered_with(
            Some(agent),
            connected,
            restores,
            listing,
            None,
            undriven(),
            AuthState::default(),
        )
    }

    /// The panel over an agent that claims everything, with a lifecycle call
    /// that did not do what it said.
    fn rendered_failing(failed: Option<Failed>) -> String {
        rendered_with(
            Some(everything()),
            true,
            Some(Restore::Load),
            None,
            failed,
            undriven(),
            AuthState::default(),
        )
    }

    /// The panel before any agent has answered `initialize`.
    fn rendered_without_an_agent() -> String {
        rendered_with(
            None,
            false,
            None,
            None,
            None,
            undriven(),
            AuthState::default(),
        )
    }

    /// The panel over an agent that claims everything, with something already
    /// driven on it — which is the record the fourth fact is drawn from (§7.7).
    fn rendered_driven(driven: DrivenRecord) -> String {
        rendered_with(
            Some(everything()),
            true,
            Some(Restore::Load),
            None,
            None,
            driven,
            AuthState::default(),
        )
    }

    /// The panel over an agent that claims everything, with authentication
    /// standing wherever the test needs it to.
    ///
    /// The state the affordances here are pointedly *not* gated on (§7.7), which
    /// is why it is a fixture rather than a constant: a button that appeared only
    /// once somebody had logged in would be this window deciding what may be
    /// observed.
    fn rendered_auth(agent: v1::InitializeResponse, auth: AuthState) -> String {
        rendered_with(
            Some(agent),
            true,
            Some(Restore::Load),
            None,
            None,
            undriven(),
            auth,
        )
    }

    /// The panel, whole: every input the sessions group is drawn from, and
    /// `None` for the agent that has not described itself yet.
    fn rendered_with(
        agent: Option<v1::InitializeResponse>,
        connected: bool,
        restores: Option<Restore>,
        listing: Option<SessionListing>,
        failed: Option<Failed>,
        driven: DrivenRecord,
        auth: AuthState,
    ) -> String {
        rendered_with_listing_state(
            agent,
            connected,
            restores,
            ListingState {
                listing,
                pending: false,
                advertised: None,
            },
            failed,
            driven,
            AuthenticationState {
                state: auth,
                pending: Vec::new(),
            },
        )
    }

    struct ListingState {
        listing: Option<SessionListing>,
        pending: bool,
        advertised: Option<bool>,
    }

    struct AuthenticationState {
        state: AuthState,
        pending: Vec<AuthenticationCall>,
    }

    fn rendered_with_listing_state(
        agent: Option<v1::InitializeResponse>,
        connected: bool,
        restores: Option<Restore>,
        listing: ListingState,
        failed: Option<Failed>,
        driven: DrivenRecord,
        authentication: AuthenticationState,
    ) -> String {
        let ListingState {
            listing,
            pending: listing_pending,
            advertised: lists_sessions_override,
        } = listing;
        let AuthenticationState {
            state: auth,
            pending: auth_pending,
        } = authentication;
        #[component]
        fn Host(
            agent: Option<v1::InitializeResponse>,
            session: Option<v1::SessionId>,
            connected: bool,
            restores: Option<Restore>,
            listing: Option<SessionListing>,
            failed: Option<Failed>,
            driven: DrivenRecord,
            auth: AuthState,
            listing_pending: bool,
            auth_pending: Vec<AuthenticationCall>,
            lists_sessions_override: Option<bool>,
        ) -> Element {
            // The five gates read off the same claims core reads them off, so
            // a fixture cannot advertise one thing and be drawn as another —
            // and an agent that has claimed nothing has not claimed these.
            let claimed = agent
                .as_ref()
                .map(|agent| agent.agent_capabilities.clone())
                .unwrap_or_default();
            let sessions = &claimed.session_capabilities;
            let lists_sessions = lists_sessions_override.unwrap_or_else(|| sessions.list.is_some());
            let closes = sessions.close.is_some();
            let deletes = sessions.delete.is_some();
            let opens_with_roots = sessions.additional_directories.is_some();
            let logs_out = claimed.auth.logout.is_some();
            let claims = Claims {
                driven: driven.clone(),
                auth: auth.clone(),
                lists_sessions,
                session: session.clone(),
                auth_pending: auth_pending.clone(),
                listing: listing.clone(),
                listing_pending,
                restores,
                closes,
                deletes,
                opens_with_roots,
                connected,
                failed: failed.clone(),
                logs_out,
                ..Claims::about(agent.clone())
            };
            rsx! {
                // Both accounts, because the window draws both: the fixture
                // stands in for the whole centre screen, and a test asking
                // whether a fact is on screen should not have to know which of
                // the two tabs has it.
                for whose in [Advertiser::Agent, Advertiser::Client] {
                    AgentClaims { key: "{whose.whose()}", claims: claims.clone(), whose }
                }
                // And the sessions, which are a dialog off the Timeline's own
                // header rather than a block of either screen
                // (`crate::session`). Rendered here because these tests ask
                // whether the *window* draws a fact, and which surface holds it
                // is exactly what this fixture exists not to know.
                crate::session::Sessions { claims }
            }
        }

        let mut dom = VirtualDom::new_with_props(
            Host,
            HostProps {
                agent,
                session: Some(v1::SessionId::new("sess-1")),
                connected,
                restores,
                listing,
                failed,
                driven,
                auth,
                listing_pending,
                auth_pending,
                lists_sessions_override,
            },
        );
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    /// The listing as it is drawn, for an agent that lists and opens sessions
    /// the way `restores` says.
    fn listing_for(restores: Option<Restore>) -> String {
        affordances(&rendered(
            everything(),
            true,
            restores,
            Some(two_sessions()),
        ))
    }

    /// An agent that lists its sessions and claims nothing else about them.
    ///
    /// The fixture for the gates this ring adds: a listing with rows in it and
    /// no advertisement behind any of the things a row could do.
    fn only_listing() -> v1::InitializeResponse {
        described(v1::AgentCapabilities::new().session_capabilities(
            v1::SessionCapabilities::new().list(v1::SessionListCapabilities::new()),
        ))
    }

    #[test]
    fn a_listed_session_can_be_closed_and_deleted_where_the_agent_advertised_them() {
        // The end of the lifecycle ring (§7.5): every row carries the two calls
        // this agent claimed, named by the method each will send.
        let listed = listing_for(Some(Restore::Load));

        for row in rows_shown(&listed) {
            assert!(
                row.contains("session/close") && row.contains("session/delete"),
                "each listed session can be closed and deleted: {row}"
            );
        }
    }

    #[test]
    fn every_listed_session_affordance_names_the_session_it_drives() {
        for (how, opening) in [
            (Restore::Load, "session/load"),
            (Restore::Resume, "session/resume"),
        ] {
            let listed = listing_for(Some(how));

            for (row, session) in rows_shown(&listed).into_iter().zip(["sess-1", "sess-2"]) {
                for affordance in [opening, "session/close", "session/delete"] {
                    let name = format!(r#"aria-label="{affordance} session {session}""#);
                    assert!(
                        row.contains(&name),
                        "{affordance} identifies {session}: {row}"
                    );
                }
                for name in [
                    format!("Edit additionalDirectories for session {session}"),
                    format!("additionalDirectories for session {session}"),
                ] {
                    assert!(
                        row.contains(&format!(r#"aria-label="{name}""#)),
                        "the repeated roots control identifies {session}: {row}"
                    );
                }
                assert!(
                    row.contains(&format!(
                        r#"title="Edit additionalDirectories for session {session}""#
                    )),
                    "the roots disclosure tooltip identifies {session}: {row}"
                );
            }
        }
    }

    #[test]
    fn listed_sessions_state_which_one_is_current_without_changing_its_affordances() {
        let listed = listing_for(Some(Restore::Load));
        let rows = rows_shown(&listed);

        // The live one is under a heading that says so, which is where the
        // word moved when the marked row became a section of its own: what §7.5
        // asks is that the state is carried in words, not that every row
        // repeats its own section.
        assert!(
            listed.contains(r#"<h3 class="eyebrow">Current session</h3>"#),
            "the live Session is explicit in words: {listed}"
        );
        assert!(
            !rows[0].contains("session-state"),
            "and does not say it twice on the row under that heading: {}",
            rows[0]
        );
        assert!(
            rows[1].contains("Available session"),
            "another listed Session is explicit in words: {}",
            rows[1]
        );
        for affordance in ["session/load", "session/close", "session/delete"] {
            assert!(
                rows[0].contains(affordance),
                "being current is a state, not a gate on {affordance}: {}",
                rows[0]
            );
        }
    }

    #[test]
    fn a_long_reported_root_remains_complete_in_the_summary_and_field() {
        let path = format!("/{}", "a".repeat(2_000));
        let mut listing = SessionListing::default();
        listing.sessions = vec![
            v1::SessionInfo::new("sess-1", "/tmp")
                .additional_directories(vec![path.clone().into()]),
        ];
        let panel = affordances(&rendered(
            everything(),
            true,
            Some(Restore::Load),
            Some(listing),
        ));

        assert!(
            panel.matches(&path).count() >= 2,
            "the complete path remains in both the closed summary and textarea: {panel}"
        );
        assert!(
            !panel.contains('…'),
            "protocol paths are not replaced with inaccessible ellipses: {panel}"
        );
        let row = rows_shown(&panel)[0];
        for affordance in ["session/load", "session/close", "session/delete"] {
            let name = format!(r#"aria-label="{affordance} session sess-1""#);
            assert!(
                row.contains(&name),
                "long content does not cost the target-specific {affordance} name: {row}"
            );
        }
    }

    #[test]
    fn session_lifecycle_and_roots_are_the_dialogs_own_rows() {
        let panel = rendered(
            everything(),
            true,
            Some(Restore::Load),
            Some(reporting_roots()),
        );
        assert!(
            panel.contains(r#"class="dialog-body" data-slot="sessions""#)
                && !panel.contains("card"),
            "the Session lifecycle is flat content of its dialog rather than a card in it: {panel}"
        );
        let lifecycle = affordances(&panel);

        for component in [
            r#"class="session-list""#,
            r#"class="session-row""#,
            r#"class="session-head""#,
            r#"class="session-line""#,
            r#"class="session-actions""#,
            "disclose bare roots-new",
            "disclose bare roots-supplied",
            r#"class="textarea""#,
        ] {
            assert!(
                lifecycle.contains(component),
                "the session lifecycle is drawn with {component}: {lifecycle}"
            );
        }
        assert!(
            lifecycle.contains(r#"class="disclosure-icon" aria-hidden="true""#)
                && lifecycle.contains("<svg"),
            "new and reported roots use the shared Heroicons Outline disclosure marker: {lifecycle}"
        );
        // **The hierarchy is the tone now and no longer the size.** Every
        // control on a row is the row's own height in the drawing, so what
        // separates the way in from the ways out is the accent one carries and
        // the other two do not — and the destructive one is the only one drawn
        // in the failure hue.
        for (class, method) in [
            ("btn btn-sm btn-filled", "session/new"),
            ("btn btn-xs btn-quiet btn-go reopen", "session/load"),
            ("btn btn-xs btn-quiet session-end", "session/close"),
            ("btn btn-xs btn-quiet btn-bad session-end", "session/delete"),
        ] {
            let before = lifecycle
                .split_once(&format!(">{method}</button>"))
                .unwrap_or_else(|| panic!("{method} retains its visible label: {lifecycle}"))
                .0;
            let button = before
                .rsplit_once("<button")
                .expect("the method label belongs to a button")
                .1;
            assert!(
                button.contains(&format!(r#"class="{class}""#)),
                "{method} retains its visible label and agreed control hierarchy: {lifecycle}"
            );
        }

        let refused = affordances(&rendered_failing(Some(Failed {
            method: "session/delete",
            error: CallError::Rejected(v1::Error::method_not_found()),
        })));
        assert!(
            refused.contains("alert alert-error"),
            "a lifecycle refusal uses a daisyUI failure notice: {refused}"
        );

        let loading = affordances(&rendered_with_listing_state(
            Some(everything()),
            true,
            Some(Restore::Load),
            ListingState {
                listing: Some(two_sessions()),
                pending: true,
                advertised: None,
            },
            None,
            undriven(),
            AuthenticationState {
                state: AuthState::default(),
                pending: Vec::new(),
            },
        ));
        assert!(
            loading.contains(
                r#"class="alert" data-slot="session-list-status" role="status" aria-live="polite""#
            ) && loading.contains("Loading sessions.")
                && loading.contains("Previous Session rows are stale and hidden")
                && loading.contains("the prior count and any paging remain visible"),
            "an in-flight listing is an accessible daisyUI status: {loading}"
        );
        let listed_while_loading = loading
            .split_once(r#"class="rail-group session-listing""#)
            .expect("the listing section")
            .1;
        assert!(
            !listed_while_loading.contains("session-row"),
            "stale rows are not presented as the pending page: {loading}"
        );
        // And the live session keeps its id while that page is in flight,
        // because which session the window is in is not the listing's answer —
        // but it loses the description that *was* the listing's, so a refresh
        // never leaves a stale working directory on screen.
        assert!(
            loading.contains("session-row live") && loading.contains("sess-1"),
            "the live session is still named while the listing reloads: {loading}"
        );
        assert!(
            !loading.contains("session-cwd"),
            "and nothing the replaced page said about it stays on screen: {loading}"
        );

        let empty = affordances(&rendered(
            everything(),
            true,
            Some(Restore::Load),
            Some(SessionListing::default()),
        ));
        assert!(
            empty.contains(
                r#"class="alert" data-slot="session-list-status" role="status" aria-live="polite""#
            ) && empty.contains("The agent listed no sessions."),
            "an empty listing is an accessible daisyUI status: {empty}"
        );

        #[component]
        fn Paged(connected: bool) -> Element {
            rsx! { SessionPaging { available: true, connected, on_next: move |()| {} } }
        }
        let mut dom = VirtualDom::new_with_props(Paged, PagedProps { connected: true });
        dom.rebuild_in_place();
        let paged = dioxus_ssr::render(&dom);
        assert!(
            paged.contains("btn btn-xs btn-quiet next-page")
                && paged.contains("The agent has more sessions.")
                && paged.contains(r#"aria-label="Ask the agent for the next page of sessions""#)
                && paged.contains(">Next page</button>"),
            "a paged listing exposes its daisyUI paging Affordance: {paged}"
        );

        let mut dom = VirtualDom::new_with_props(Paged, PagedProps { connected: false });
        dom.rebuild_in_place();
        let unavailable = dioxus_ssr::render(&dom);
        assert!(
            unavailable.contains("Next page is unavailable while disconnected.")
                && unavailable.contains("disabled=true"),
            "paging availability is stated in words rather than by dimness alone: {unavailable}"
        );
    }

    #[test]
    fn identities_advertisements_and_authentication_are_compact_grouped_facts() {
        let panel = shown(identified(), true);

        for component in [
            r#"class="rail-group claims" data-slot="claims""#,
            r#"class="rail-group account""#,
            r#"class="who" data-claim="identity""#,
            r#"class="who-name""#,
            r#"class="who-facts""#,
            r#"class="rail-group advertisement""#,
            r#"class="rail-count" data-slot="claim-count""#,
            r#"class="caps" data-slot="capabilities""#,
            r#"class="cap yes""#,
            r#"class="rail-group authentication" data-claim="authentication""#,
            r#"class="auth-method""#,
            "btn btn-xs btn-quiet authenticate",
            "btn btn-xs btn-quiet logout-action",
        ] {
            assert!(
                panel.contains(component),
                "the Agent presentation is drawn with {component}: {panel}"
            );
        }
        assert!(
            !panel.contains("card"),
            "repeated claims are groups of the rail rather than floating cards: {panel}"
        );
        assert!(
            panel.contains("Test Agent") && panel.contains("test-agent") && panel.contains("1.2.3"),
            "the Agent identity remains complete: {panel}"
        );
        assert!(
            panel.contains(r#"data-identity="client""#)
                && panel.contains("ACP Inspector")
                && panel.contains("acp-inspector"),
            "the client identity sent during initialize is shown with its claims: {panel}"
        );
        assert!(
            panel.contains(r#"aria-label="Authenticate with Log in with the browser (browser)""#)
                && panel.contains(">Authenticate</button>")
                && panel.contains(">Logout</button>"),
            "consequential authentication actions keep visible labels and identify their target: {panel}"
        );

        let absent = shown(nothing(), true);
        assert!(
            absent.contains("the agent sent no") && absent.contains(r#"class="cap no""#),
            "an absent identity and an unmade claim are said rather than left blank: {absent}"
        );

        let required = authentication(&rendered_auth(
            identified(),
            AuthState::Required(v1::Error::new(
                i32::from(v1::ErrorCode::AuthRequired),
                "authenticate first",
            )),
        ));
        assert!(
            required.contains(r#"class="alert login""#)
                && !required.contains("alert-warning")
                && !required.contains("alert-error"),
            "authentication-required is a neutral daisyUI notice: {required}"
        );

        let refused = authentication(&rendered_auth(
            identified(),
            AuthState::Refused {
                method: browser(),
                error: CallError::Disconnected,
            },
        ));
        assert!(
            refused.contains("was not accepted")
                && !refused.contains("detail bad")
                && !refused.contains("alert-error"),
            "authentication refusal remains an ordinary fact: {refused}"
        );
    }

    #[test]
    fn disconnected_authentication_actions_say_they_are_unavailable() {
        let auth = authentication(&shown(identified(), false));

        assert!(
            auth.contains("Authentication actions are unavailable while disconnected.")
                && auth.contains(r#"role="status""#)
                && auth.contains("disabled=true"),
            "disabled state is stated in words rather than carried by dimness alone: {auth}"
        );
    }

    #[test]
    fn authentication_loading_names_each_call_without_changing_advertisement_gates() {
        let panel = rendered_with_listing_state(
            Some(identified()),
            true,
            Some(Restore::Load),
            ListingState {
                listing: None,
                pending: false,
                advertised: None,
            },
            None,
            undriven(),
            AuthenticationState {
                state: AuthState::default(),
                pending: vec![
                    AuthenticationCall::Authenticate(browser()),
                    AuthenticationCall::Logout,
                ],
            },
        );
        let auth = authentication(&panel);

        assert!(
            auth.contains("Authenticating with Log in with the browser (browser).")
                && auth.contains("Logging out.")
                && auth.contains(r#"role="status""#)
                && auth.contains(r#"aria-live="polite""#)
                && auth.contains(r#"aria-busy="true""#),
            "loading is a named, announced authentication fact: {auth}"
        );
        assert!(
            auth.contains(">Authenticating</button>")
                && auth.contains(">Logging out</button>")
                && !auth.contains("disabled=true"),
            "pending calls remain available because their Advertisements still gate them: {auth}"
        );
    }

    #[test]
    fn an_agent_that_advertised_one_of_the_two_is_offered_that_one() {
        // Two claims in two fields, so two gates: an agent that closes sessions
        // and does not delete them is offered close and not delete (§7.5). Both
        // ways round, because a screen wired to the wrong field would pass every
        // test that only ever advertised both.
        for (claimed, offered, withheld) in [
            ("close", "session/close", "session/delete"),
            ("delete", "session/delete", "session/close"),
        ] {
            let capabilities = v1::AgentCapabilities::new().session_capabilities(match claimed {
                "close" => v1::SessionCapabilities::new()
                    .list(v1::SessionListCapabilities::new())
                    .close(v1::SessionCloseCapabilities::new()),
                _ => v1::SessionCapabilities::new()
                    .list(v1::SessionListCapabilities::new())
                    .delete(v1::SessionDeleteCapabilities::new()),
            });
            let listed = affordances(&rendered(
                described(capabilities),
                true,
                None,
                Some(two_sessions()),
            ));

            for row in rows_shown(&listed) {
                assert!(row.contains(offered), "the claim it made: {row}");
                assert!(!row.contains(withheld), "and only that one: {row}");
            }
            // And the sentence above the rows names what it will send, which is
            // the same one claim.
            assert!(
                listed.contains(offered) && !listed.contains(withheld),
                "said once above them all: {listed}"
            );
        }
    }

    #[test]
    fn an_agent_that_advertised_neither_close_nor_delete_is_offered_neither() {
        // Gated on the advertisement and on nothing else, each gate its own: the
        // listing is still here, the rows are still here, and there is nothing
        // on them to close or delete with.
        let listed = affordances(&rendered(only_listing(), true, None, Some(two_sessions())));

        assert!(
            listed.contains("sess-1"),
            "the listing is still here: {listed}"
        );
        for row in rows_shown(&listed) {
            assert!(
                !row.contains("session/close") && !row.contains("session/delete"),
                "and no way to end a session: {row}"
            );
        }
    }

    #[test]
    fn a_capability_the_agent_advertised_and_then_failed_says_which_one() {
        // §7.5's capability lie, where a reader meets it: the affordance was
        // drawn because the agent claimed the method, so the failure is about
        // that method — and four affordances sit here, three of which may have
        // worked. The agent's own words are kept whole beside it.
        let refused = affordances(&rendered_failing(Some(Failed {
            method: "session/close",
            error: CallError::Undecodable("not what a close answers".to_owned()),
        })));

        assert!(
            refused.contains("session/close") && refused.contains("failed"),
            "the call that failed, named: {refused}"
        );
        assert!(
            refused.contains("not what a close answers"),
            "with what the agent said about it: {refused}"
        );
    }

    #[test]
    fn the_ways_out_of_a_session_are_quieter_than_the_way_in_and_never_hidden() {
        // Revealing a control on hover is the rule for one that repeats
        // something already on the row — the Timeline's `raw` and its way back
        // to the wire. It is the wrong rule for the only way to do a thing:
        // what is not drawn cannot be looked for, and a list that has to be
        // swept with a pointer to be read cannot be read at a glance.
        let listed = affordances(&rendered(
            everything(),
            true,
            Some(Restore::Load),
            Some(two_sessions()),
        ));
        for row in rows_shown(&listed) {
            for ends in ["session/close", "session/delete"] {
                assert!(row.contains(ends), "{ends} is drawn: {row}");
            }
        }

        // What separates them from the way in is the size the two are drawn at,
        // which is the difference that was meant when they were hidden.
        assert!(
            listed.contains("btn btn-xs btn-quiet btn-go reopen")
                && listed.contains("btn btn-xs btn-quiet session-end"),
            "the way in carries the accent and the ways out carry none: {listed}"
        );
    }

    #[test]
    fn a_connection_that_ended_says_so_where_its_controls_are() {
        // The record outlives the agent on purpose (§5) — an agent that went
        // away mid-session is the finding, and a window that emptied itself
        // would be deleting the evidence to tidy up. What it owes is to say
        // which of the two states it is in, where there are controls it is
        // about.
        let gone = affordances(&rendered(
            everything(),
            false,
            Some(Restore::Load),
            Some(two_sessions()),
        ));
        assert!(
            gone.contains(r#"data-slot="availability""#)
                && gone.contains("The connection has ended"),
            "a dialog full of inert controls says why: {gone}"
        );
        assert!(
            gone.contains("sess-1") && gone.contains("sess-2"),
            "and keeps what the last agent reported: {gone}"
        );

        let live = affordances(&rendered(
            everything(),
            true,
            Some(Restore::Load),
            Some(two_sessions()),
        ));
        assert!(
            !live.contains(r#"data-slot="availability""#),
            "and says nothing while there is an agent to answer: {live}"
        );
    }

    #[test]
    fn an_agent_that_has_gone_cannot_be_asked_to_close_or_delete_one() {
        // The rule every button here follows: one that cannot do what it says is
        // worse than one that says it cannot.
        let listed = affordances(&rendered(
            everything(),
            false,
            Some(Restore::Load),
            Some(two_sessions()),
        ));

        for row in rows_shown(&listed) {
            assert_eq!(
                row.matches("disabled").count(),
                row.matches("<button").count(),
                "every way into or out of a session is disabled: {row}"
            );
        }
    }

    /// What the panel's parts are read by: what each one *is*, never the class
    /// it happens to be painted in — a test that matched the paint would be a
    /// test the redesign has to be talked out of (#104).
    ///
    /// One list of capability rows, in one group of one advertiser's block.
    const CAPABILITIES: &str = r#"data-slot="capabilities""#;
    /// The mark that repeats what a row says in words, and is decoration.
    /// The row's own visible answer, in words.
    const STATE: &str = r#"data-slot="state">"#;
    /// What became of driving the claim, or why nothing did (§7.7) — up to
    /// the words themselves, which are what most of the rows are read for.
    const DRIVEN: &str = r#"data-slot="driven">"#;
    /// The shape a run of claims was made in — and, since one run is delimited
    /// by the next one's shape, what a run is read by.
    const SHAPE: &str = r#"data-slot="shape""#;
    /// Why nothing here drives a single row of a run, said once above them.
    const UNREACHED: &str = r#"data-slot="unreached""#;
    /// The roots a session would be opened with.
    const ROOTS: &str = r#"data-slot="roots""#;
    /// The block of session affordances, which the two blocks above it end at.
    const SESSIONS: &str = r#"data-slot="sessions""#;

    /// Every `<li>` in a fragment of the screen, one string each.
    fn rows_shown(html: &str) -> Vec<&str> {
        // Bounded at the row's own end. Unbounded, the last row of a list ran
        // to the end of the document — so a control belonging to the surface
        // rather than to any row, like the dialog's own dismiss, counted as one
        // of that row's.
        html.split("<li")
            .skip(1)
            .map(|row| row.split_once("</li>").map_or(row, |(row, _)| row))
            .collect()
    }

    /// The capability rows, one string each, across every group and every
    /// shape — the `<li>`s inside a `.caps` list and nothing else, so the
    /// listing's own rows further down cannot be mistaken for one.
    fn capability_rows(html: &str) -> Vec<&str> {
        html.split(CAPABILITIES)
            .skip(1)
            .flat_map(|list| {
                rows_shown(
                    list.split_once("</ul>")
                        .expect("a capability list is closed")
                        .0,
                )
            })
            .collect()
    }

    /// Every run of claims in a fragment of the screen, one string each: the
    /// shape it was made in, whatever the run says about nothing here driving
    /// it, and its rows.
    ///
    /// Read by the shape rather than by the box it is painted in (#104), which
    /// is what delimits a run in the first place: a run is the claims made in
    /// one shape at one place in the `initialize` result, so the next shape is
    /// where one ends.
    fn runs(html: &str) -> Vec<&str> {
        const CLOSE: &str = "</ul>";
        html.split(SHAPE)
            .skip(1)
            .map(|run| {
                let end = run.find(CLOSE).expect("a run's capability list is closed");
                &run[..end + CLOSE.len()]
            })
            .collect()
    }

    /// The run the row naming `capability` belongs to, out of the agent's own
    /// block.
    fn run_for(panel: &str, capability: &str) -> String {
        runs(&advertisement(panel, "agent"))
            .into_iter()
            .find(|run| run.contains(capability))
            .unwrap_or_else(|| panic!("the {capability} row is in a run: {panel}"))
            .to_owned()
    }

    #[test]
    fn a_capability_row_says_in_text_whether_the_agent_advertised_it() {
        // The state is text-borne, the way tool-call and plan status are: an
        // unlit row that reached a reader as a bare capability name would have
        // handed them the name and kept the answer (#83). Both fixtures,
        // because a row that said "not advertised" whatever the agent claimed
        // would pass a test that only ever asked the agent that claimed
        // nothing.
        for (capabilities, state) in [(everything(), "advertised"), (nothing(), "not advertised")] {
            // The whole span, because "advertised" is a substring of "not
            // advertised" and a test that only asked for the shorter one would
            // read every row as the answer it hoped for.
            let said = format!(r#"{STATE}{state}</span>"#);
            let panel = shown(capabilities, true);
            // The agent's rows, because the fixture is an agent: this client's
            // own claim is drawn beside them and says what *it* advertised
            // (#86), which is not this agent's answer to anything.
            let agent = advertisement(&panel, "agent");
            let rows = capability_rows(&agent);

            assert!(!rows.is_empty(), "there are capability rows to read");
            for row in rows {
                assert!(row.contains(&said), "the row says its state: {row}");
            }
        }
    }

    /// A refusal in the agent's own words, as the record would have carried
    /// one.
    fn turned_down() -> Driven {
        Driven::Refused(CallError::Rejected(v1::Error::method_not_found()))
    }

    /// The record of an agent whose `session/load` was driven, and what came of
    /// it.
    fn record(outcome: Driven) -> DrivenRecord {
        DrivenRecord::from_iter([(AgentCapability::Load, outcome)])
    }

    /// Every row an affordance reaches on an agent that advertised everything,
    /// and the capability each of them is the record's key for (§7.7).
    ///
    /// **`session/resume` is not among them, and its absence is the ring's last
    /// decision** (§7.5): an agent that advertised load too never gets a resume
    /// sent to it, so on this fixture that row says why rather than reporting an
    /// outcome. Where resume *is* what this tool would send, it records like the
    /// rest — which is
    /// `an_agent_that_advertised_resume_alone_records_outcomes_on_the_row`.
    ///
    /// `authenticate` is not here either, for the other reason: it is a row an
    /// affordance reaches and it has no entry in this record at all, because
    /// `AuthState` is its record (§7.7).
    ///
    /// Stated by name rather than derived from `AgentCapability::ALL`, because
    /// what is being asserted is the *wiring*: a row that asked the record about
    /// the wrong capability would report one agent's answer on another's row,
    /// and a list computed from the same source as the panel's could not catch
    /// it.
    const REACHED: [(&str, AgentCapability); 6] = [
        ("session/load", AgentCapability::Load),
        ("session/list", AgentCapability::List),
        ("session/close", AgentCapability::Close),
        ("session/delete", AgentCapability::Delete),
        (
            "additionalDirectories",
            AgentCapability::AdditionalDirectories,
        ),
        ("logout", AgentCapability::Logout),
    ];

    /// What a row says where nothing in this tool can drive it (§7.7), and the
    /// whole of what those rows have in common: the sentence is about the tool.
    const CANNOT: &str = " this tool cannot drive it";

    /// The same sentence about a whole run, which is where it is said when
    /// every row of the run has the one reason (§7.7).
    const CANNOT_ANY: &str = "this tool cannot drive any of these";

    /// Every capability row nothing in this tool can drive, however the panel
    /// says so: on the row, or once above the run the row belongs to.
    ///
    /// One list rather than two, because the rule is about what a *reader* is
    /// told — a row whose run says why is a row that says why, and a test that
    /// counted only the row-borne form would pass a panel that had quietly
    /// stopped saying it about five of them.
    fn unreachable_rows(block: &str) -> Vec<&str> {
        runs(block)
            .into_iter()
            .flat_map(|run| {
                let said_of_all = run.contains(CANNOT_ANY);
                capability_rows(run)
                    .into_iter()
                    .filter(move |row| said_of_all || row.contains(CANNOT))
            })
            .collect()
    }

    /// Everything a row says when it *is* reporting an outcome — so a test can
    /// assert a row says none of them.
    ///
    /// Every arm [`came_back`] has, the outcome this window has no rendering for
    /// included: a row that leaked *that* onto an unreached row would be
    /// reporting an outcome as surely as the three named ones.
    const OUTCOMES: [&str; 4] = [
        " not driven",
        " driven and answered",
        " driven and refused",
        " driven, with an outcome",
    ];

    /// The capability row naming `capability`, out of the agent's own block.
    ///
    /// Off the capability rows rather than off the panel, because the
    /// affordances further down name the same methods and are things to click
    /// rather than claims to read.
    fn row_for(panel: &str, capability: &str) -> String {
        capability_rows(&advertisement(panel, "agent"))
            .into_iter()
            .find(|row| row.contains(capability))
            .unwrap_or_else(|| panic!("the {capability} row is drawn: {panel}"))
            .to_owned()
    }

    /// The `session/load` row of the agent's own block.
    fn load_row(panel: &str) -> String {
        row_for(panel, "session/load")
    }

    #[test]
    fn the_load_row_says_whether_the_advertisement_was_ever_driven() {
        // The fourth fact (§7.7): a capability nobody pressed the button on and
        // one the agent honoured stop looking the same on the screen built to
        // audit its claims. All three outcomes, because a row that said one
        // thing whatever the record held would pass a test that only ever asked
        // about one of them.
        for (driven, says, and_not) in [
            (Driven::Unasked, " not driven", " driven and"),
            (Driven::Answered, " driven and answered", " not driven"),
            (turned_down(), " driven and refused", " driven and answered"),
        ] {
            let row = load_row(&rendered_driven(record(driven)));

            assert!(
                row.contains(&format!(r#"{DRIVEN}{says}"#)),
                "the row says what became of driving it: {row}"
            );
            assert!(!row.contains(and_not), "and says only that: {row}");
        }

        // And a refusal carries the agent's own error, which is the reason to
        // draw it rather than a paraphrase of it.
        let refused = load_row(&rendered_driven(record(turned_down())));
        assert!(
            refused.contains("Method not found"),
            "in the agent's own words: {refused}"
        );
    }

    #[test]
    fn a_refusal_is_a_fourth_element_of_the_same_row_and_no_louder() {
        // **No new panel, no lane, no colour of its own** (§7.7). It is read
        // after what the row already said, in the same row, at the same weight:
        // a refusal is the third thing that happened rather than the bad one,
        // and an agent that advertised a capability and then refused it is a
        // finding the reader draws rather than a grade this tool awards. The
        // stylesheet holds the other half of this (`style.rs`).
        let row = load_row(&rendered_driven(record(turned_down())));

        let state = row.find(STATE).expect("the row says its state");
        let driven = row.find(DRIVEN).expect("and what became of driving it");
        assert!(state < driven, "the fourth fact is the fourth thing: {row}");
        assert!(
            !row.contains("bad"),
            "and it is not painted as a failure: {row}"
        );
    }

    /// Every capability row of the agent's own block that reports an outcome.
    fn reporting(panel: &str) -> Vec<String> {
        capability_rows(&advertisement(panel, "agent"))
            .into_iter()
            .filter(|row| OUTCOMES.iter().any(|outcome| row.contains(outcome)))
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn every_row_an_affordance_reaches_reports_an_outcome_and_no_other_row_does() {
        // The rows something in this tool sends a call for (§7.7). The rows
        // nothing here can drive report no outcome, because *not driven* on one
        // of those would be a sentence about the agent when the truth is about
        // this tool — and a row wired to nothing would be reporting the same
        // thing by accident.
        //
        // `authenticate` is the one this list does not carry, because it is the
        // one whose outcome is not in that record at all: `AuthState` is its
        // record (§7.7), and the row is reached all the same.
        let mut expected: Vec<_> = REACHED.iter().map(|(capability, _)| *capability).collect();
        expected.push("authenticate");
        expected.push("image");
        expected.push("audio");
        let panel = rendered_driven(record(Driven::Answered));

        let carrying = reporting(&panel);
        for capability in &expected {
            assert!(
                carrying.iter().any(|row| row.contains(capability)),
                "{capability} reports an outcome: {carrying:?}"
            );
        }
        assert_eq!(
            carrying.len(),
            expected.len(),
            "and nothing else does: {carrying:?}"
        );
    }

    #[test]
    fn a_row_no_affordance_reaches_draws_no_outcome_and_says_why() {
        // **The sentence is about this tool** (§7.7). Six rows on an agent that
        // advertised everything: the resume this tool never sends because it
        // prefers load, and the five it cannot drive on any agent at all. Each
        // says so where the outcome would have been — on the row, or, where
        // every row of a run has the one reason, once above the run — so a
        // reader stops hunting for a button that never existed.
        let panel = rendered_driven(record(Driven::Answered));

        let agent = advertisement(&panel, "agent");
        let unreached = unreachable_rows(&agent);
        assert_eq!(
            unreached
                .iter()
                .filter(|row| OUTCOMES.iter().any(|outcome| row.contains(outcome)))
                .count(),
            0,
            "none of them reports an outcome as well: {unreached:?}"
        );
        for named in ["session/resume", "embeddedContext", "http", "sse"] {
            assert!(
                unreached.iter().any(|row| row.contains(named)),
                "{named} says this tool cannot drive it: {unreached:?}"
            );
        }
        assert_eq!(unreached.len(), 4, "and only those: {unreached:?}");
    }

    #[test]
    fn the_resume_row_beside_an_advertised_load_says_which_one_this_tool_prefers() {
        // **The restore property prefers load** (§7.5), so a resume is never
        // sent against such an agent — and a row reading *not driven* forever
        // would read as the agent being left untested by accident rather than as
        // the tool decision it is.
        let row = row_for(&rendered_driven(record(Driven::Answered)), "session/resume");

        assert!(
            row.contains("prefers session/load"),
            "the row says which call this tool sends instead: {row}"
        );
        assert!(
            !OUTCOMES.iter().any(|outcome| row.contains(outcome)),
            "and reports no outcome at all: {row}"
        );
    }

    /// An agent that reopens sessions the one way this tool would then send.
    fn only_resuming() -> v1::InitializeResponse {
        described(v1::AgentCapabilities::new().session_capabilities(
            v1::SessionCapabilities::new().resume(v1::SessionResumeCapabilities::new()),
        ))
    }

    #[test]
    fn an_agent_that_advertised_resume_alone_records_outcomes_on_the_row() {
        // The other half of the pair: where resume is what an agent's
        // advertisement entitles this tool to send, the row is one an affordance
        // reaches and it records like every other (§7.7).
        let panel = rendered_with(
            Some(only_resuming()),
            true,
            Some(Restore::Resume),
            None,
            None,
            DrivenRecord::from_iter([(AgentCapability::Resume, turned_down())]),
            AuthState::default(),
        );

        let row = row_for(&panel, "session/resume");
        assert!(
            row.contains(&format!(r#"{DRIVEN} driven and refused"#))
                && row.contains("Method not found"),
            "what became of driving it, in the agent's own words: {row}"
        );
        assert!(
            !row.contains(CANNOT),
            "and nothing about this tool being unable to: {row}"
        );
    }

    /// An agent that ends the sessions it holds and will not name one.
    fn ending_without_listing() -> v1::InitializeResponse {
        described(
            v1::AgentCapabilities::new().session_capabilities(
                v1::SessionCapabilities::new()
                    .close(v1::SessionCloseCapabilities::new())
                    .delete(v1::SessionDeleteCapabilities::new()),
            ),
        )
    }

    /// The same agent, listing them too.
    fn ending_with_listing() -> v1::InitializeResponse {
        described(
            v1::AgentCapabilities::new().session_capabilities(
                v1::SessionCapabilities::new()
                    .list(v1::SessionListCapabilities::new())
                    .close(v1::SessionCloseCapabilities::new())
                    .delete(v1::SessionDeleteCapabilities::new()),
            ),
        )
    }

    #[test]
    fn close_and_delete_with_no_listing_say_there_is_no_row_to_press_them_on() {
        // **The consequence §7.5 records, made visible where it bites** (§7.7):
        // both affordances hang off a listing row, so an agent that advertised
        // neither listing has both claims displayed and nowhere to drive them
        // from.
        let panel = rendered(ending_without_listing(), true, None, None);

        for capability in ["session/close", "session/delete"] {
            let row = row_for(&panel, capability);
            assert!(
                row.contains("no listing row") || row.contains("pressed on a listing row"),
                "{capability} says where the affordance is: {row}"
            );
            assert!(
                row.contains("advertised no session/list"),
                "and why this agent has none: {row}"
            );
            assert!(
                !OUTCOMES.iter().any(|outcome| row.contains(outcome)),
                "and reports no outcome: {row}"
            );
        }
    }

    #[test]
    fn close_and_delete_beside_a_listing_record_outcomes_normally() {
        // The same two claims on an agent that names its sessions: there is a
        // row to press them on, so the fourth fact is what became of pressing
        // it.
        let panel = rendered_with(
            Some(ending_with_listing()),
            true,
            None,
            None,
            None,
            DrivenRecord::from_iter([(AgentCapability::Close, turned_down())]),
            AuthState::default(),
        );

        let closed = row_for(&panel, "session/close");
        assert!(
            closed.contains(&format!(r#"{DRIVEN} driven and refused"#)),
            "what became of driving it: {closed}"
        );
        let deleted = row_for(&panel, "session/delete");
        assert!(
            deleted.contains(&format!(r#"{DRIVEN} not driven"#)),
            "and the one nobody drove says that: {deleted}"
        );
        for row in [&closed, &deleted] {
            assert!(!row.contains(CANNOT), "neither is unreachable: {row}");
        }
    }

    #[test]
    fn the_prompt_content_and_mcp_runs_say_this_tool_cannot_drive_them_either_way() {
        // **Whether or not the agent advertised them** (§7.7), because the
        // sentence is about this tool: the composer sends one text block and
        // `mcpServers` is always empty, and an agent's silence does not change
        // either. Both fixtures, because a run that said it whatever it was
        // handed would pass a test that only ever asked one of them.
        for agent in [everything(), nothing()] {
            let panel = shown(agent, true);

            for (capability, because) in [
                ("embeddedContext", "not embedded context"),
                ("http", "empty mcpServers"),
                ("sse", "empty mcpServers"),
            ] {
                let run = run_for(&panel, capability);
                assert!(
                    (run.contains(CANNOT_ANY) || run.contains(CANNOT)) && run.contains(because),
                    "the run {capability} is in says this tool cannot drive it, and why: {run}"
                );
                let row = row_for(&panel, capability);
                assert!(
                    !OUTCOMES.iter().any(|outcome| row.contains(outcome)),
                    "and never an outcome: {row}"
                );
            }
        }
    }

    #[test]
    fn a_run_nothing_here_reaches_says_so_once_rather_than_on_every_row() {
        // **One fact about this tool, not three about the agent** (§7.7). The
        // three prompt content shapes share one reason and the two MCP
        // transports share another, and a rail 280px wide carrying the same
        // sentence three times over is what made the reason read as a property
        // of each row.
        //
        // The rows are checked for silence rather than the sentence for being
        // singular, because that is the thing that would come back: a hoist
        // that left the row-borne copy in place would read as one statement and
        // still be drawn as four.
        let panel = shown(everything(), true);
        let agent = advertisement(&panel, "agent");

        for run in runs(&agent) {
            if !run.contains(CANNOT_ANY) {
                continue;
            }
            assert_eq!(
                run.matches(CANNOT_ANY).count(),
                1,
                "the run says it once: {run}"
            );
            for row in capability_rows(run) {
                assert!(
                    !row.contains(DRIVEN),
                    "and no row under it repeats it: {row}"
                );
            }
        }
        // And the mixed run keeps it on the row that has one: an unreachable
        // `session/resume` sits beside a `session/list` something here drives,
        // so there is nothing for the run to say about all of them.
        let sessions = run_for(&panel, "session/resume");
        assert!(
            !sessions.contains(CANNOT_ANY) && sessions.contains(CANNOT),
            "a run whose rows do not share the reason keeps it on the row: {sessions}"
        );
    }

    #[test]
    fn a_row_the_agent_did_not_advertise_says_nothing_about_driving() {
        // There is no claim to have driven, so there is nothing to say: *not
        // driven* about a capability nobody offered is an answer to a question
        // nobody asked, and the row's answer is the *not advertised* it already
        // gives (§7.7).
        //
        // The five this tool cannot drive on any agent say so whatever the
        // agent claimed — but they say it above their run now rather than on
        // the row, so there is nothing left here to except: every row in this
        // block is silent about driving, and the runs are where the exception
        // reads.
        let panel = shown(nothing(), true);

        for row in capability_rows(&advertisement(&panel, "agent")) {
            if row.contains("cap-name\">audio") || row.contains("cap-name\">embeddedContext") {
                continue;
            }
            assert!(
                !row.contains(DRIVEN),
                "a claim this agent never made has nothing to report: {row}"
            );
        }
    }

    #[test]
    fn each_reachable_row_draws_the_outcome_of_the_capability_it_names() {
        // **The wiring, row by row** (§7.7): the record is keyed on a capability
        // and a row asks it about one, so a row asking about the wrong one would
        // report an answer the agent gave about a different call. Driving one at
        // a time is what tells that apart — every row lit at once would pass
        // whatever each of them asked for.
        for (capability, key) in REACHED {
            let panel = rendered_driven(DrivenRecord::from_iter([(key, turned_down())]));

            let row = row_for(&panel, capability);
            assert!(
                row.contains(&format!(r#"{DRIVEN} driven and refused"#))
                    && row.contains("Method not found"),
                "{capability} says what became of driving it, in the agent's own words: {row}"
            );
            for (other, _) in REACHED.iter().filter(|(other, _)| *other != capability) {
                let row = row_for(&panel, other);
                assert!(
                    row.contains(&format!(r#"{DRIVEN} not driven"#)),
                    "and {other} is a row nobody drove: {row}"
                );
            }
        }
    }

    #[test]
    fn every_fourth_fact_and_every_absence_of_one_is_carried_by_words() {
        // **Not by dimness** (§7.7, and #83's rule about the half of an answer
        // that is an absence). The state above it is visible text; nothing
        // repeats *this*, so it is text too. A reader who gets neither colour
        // nor weight still reads an outcome, or reads that there is none and
        // why.
        //
        // Both fixtures, because an agent that claimed nothing draws the five
        // rows this tool cannot drive on any agent and nothing else, and a row
        // that was words on one fixture and a bare class on the other would pass
        // half a test.
        for agent in [everything(), nothing()] {
            let panel = rendered_with(
                Some(agent),
                true,
                Some(Restore::Load),
                None,
                None,
                record(turned_down()),
                AuthState::default(),
            );

            let block = advertisement(&panel, "agent");
            for run in runs(&block) {
                // The run's own sentence is held to the same rule as a row's:
                // the reason five rows have no fourth fact is read there now,
                // and a reason hidden from the accessibility tree would be the
                // dimness rule broken one level up.
                let said_of_all = run.contains(CANNOT_ANY);
                assert!(
                    !run.contains(&format!(r#"{UNREACHED} aria-hidden"#)),
                    "what a run says about driving is not hidden either: {run}"
                );

                for row in capability_rows(run) {
                    let Some((_, said)) = row.split_once(DRIVEN) else {
                        // The absence, which is the other half of the rule: a
                        // row that says nothing about driving says why in words
                        // it already had — the agent never made the claim — or
                        // its run says why for all of them.
                        assert!(
                            said_of_all || row.contains(&format!(r#"{STATE}not advertised"#)),
                            "a row with no fourth fact says in words why it has none: {row}"
                        );
                        continue;
                    };
                    let said = said
                        .split_once("</span>")
                        .expect("the fourth fact is closed")
                        .0;
                    assert!(
                        said.trim().len() > 1,
                        "the fourth fact is a sentence rather than a class: {row}"
                    );
                    assert!(
                        !row.contains(r#"data-slot="driven" aria-hidden"#),
                        "and it is not hidden from the reader who needs it most: {row}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_clients_own_block_gains_no_fourth_fact() {
        // **Both advertisers stay drawn identically** (#86, §7.7): the record is
        // about what an *agent* did when it was driven, so the client's rows
        // carry no such fact — and that must not become a difference in markup
        // or styling that tells the two blocks apart. `style.rs` forbids the
        // rule; this forbids the row.
        let panel = rendered_driven(record(turned_down()));
        let client = advertisement(&panel, "client");

        assert!(
            !client.contains(DRIVEN),
            "the client's claims have nobody to have driven them: {client}"
        );
        // And they are the same rows they always were, in the same markup as
        // the agent's beside them.
        for row in capability_rows(&client) {
            assert!(
                row.contains(STATE),
                "drawn as plainly as the agent's: {row}"
            );
        }
    }

    /// The authentication block: the advertised methods, where authentication
    /// stands, and the affordances that hang off the agent's auth claims.
    ///
    /// Found by its own marker rather than by a heading, because it has none:
    /// where the login *stands* is on the identity line now and what was done
    /// about it is here, so a titled section introducing one sentence is gone
    /// (`standing`). Cut at the advertisement below it, because the rows there
    /// name `authenticate` and `logout` as claims — and a block that read past
    /// this boundary would find a claim and call it a control.
    fn authentication(html: &str) -> String {
        let block = html
            .split_once(r#"data-claim="authentication""#)
            .expect("the auth block is drawn")
            .1;

        block
            .split_once(r#"data-claim="advertised""#)
            .map_or(block, |(auth, _)| auth)
            .to_owned()
    }

    /// The button that sends `logout`, as it is drawn — never the capability row
    /// of the same name, which is a claim to read rather than a thing to click.
    const LOGOUT: &str = ">Logout</button>";

    #[test]
    fn the_logout_button_sits_with_the_auth_methods_and_not_among_the_session_controls() {
        // **Its place on screen matches its scope on the wire** (§7.7): the
        // request carries no session id, so it belongs to the connection the
        // agent panel already describes and not to the session controls under
        // it.
        let panel = shown(everything(), true);

        let auth = authentication(&panel);
        assert!(
            auth.contains(LOGOUT),
            "the button is with the auth methods: {auth}"
        );
        let sessions = affordances(&panel);
        assert!(
            !sessions.contains(LOGOUT),
            "and nowhere near the session controls: {sessions}"
        );
    }

    #[test]
    fn an_agent_that_advertised_no_logout_is_offered_no_button() {
        // Gated on the advertisement like everything else here — and nowhere
        // else, which is what the whole panel being searched asserts.
        let panel = shown(only_listing(), true);

        assert!(
            !panel.contains(LOGOUT),
            "no claim, no button anywhere: {panel}"
        );
    }

    #[test]
    fn the_logout_button_is_offered_whatever_the_agent_has_said_about_logging_in() {
        // **Affordances gate on the Advertisement and never on what this tool
        // believes the agent's state makes sensible** (§7.7). "Logout without a
        // login" is a corner the specification leaves entirely undefined, which
        // is the reason to offer the button rather than a reason to hide it.
        for auth in [
            AuthState::Unasked,
            AuthState::Authenticated(v1::AuthMethodId::new("browser")),
        ] {
            let panel = rendered_auth(everything(), auth.clone());

            assert!(
                authentication(&panel).contains(LOGOUT),
                "offered with the auth state at {auth:?}: {panel}"
            );
        }
    }

    #[test]
    fn an_agent_that_has_gone_cannot_be_asked_to_log_out() {
        // The rule every button here follows: one that cannot do what it says is
        // worse than one that says it cannot. What a click that raced the
        // connection away records is core's, and asserted where an agent can be
        // driven (`crates/core/tests/capability.rs`).
        let auth = authentication(&shown(everything(), false));

        assert!(
            auth.contains(&format!("disabled=true{LOGOUT}")),
            "the button says it cannot: {auth}"
        );
    }

    /// The method the panel's fixtures advertise a way in with.
    fn browser() -> v1::AuthMethodId {
        v1::AuthMethodId::new("browser")
    }

    #[test]
    fn the_auth_row_draws_its_fourth_fact_from_the_auth_state() {
        // **One store, and `authenticate` is not in the record** (§7.7).
        // `AuthState` has held not-driven, accepted-with-a-method and
        // refused-with-the-agent's-own-error since the MVP, so the row asks it
        // — a second store holding the same three things would be two things to
        // disagree about one login. All three, because a row that said one of
        // them whatever the state held would pass a test that only ever asked
        // about one.
        for (auth, says) in [
            (AuthState::Unasked, " not driven"),
            (AuthState::Authenticated(browser()), " driven and answered"),
            (
                AuthState::Refused {
                    method: browser(),
                    error: CallError::Disconnected,
                },
                " driven and refused",
            ),
        ] {
            let row = row_for(&rendered_auth(everything(), auth.clone()), "authenticate");

            assert!(
                row.contains(&format!(r#"{DRIVEN}{says}"#)),
                "with the auth state at {auth:?}: {row}"
            );
        }

        // And nothing about the login reaches the record, which is the half of
        // the rule this window can be asked about: a panel handed a record with
        // everything in it draws the same row.
        let driven = row_for(
            &rendered_driven(DrivenRecord::from_iter(
                AgentCapability::ALL.map(|capability| (capability, Driven::Answered)),
            )),
            "authenticate",
        );
        assert!(
            driven.contains(&format!(r#"{DRIVEN} not driven"#)),
            "the record has no say over this row: {driven}"
        );
    }

    #[test]
    fn the_auth_block_and_the_auth_row_never_disagree_about_the_same_login() {
        // **Including after a connection died under an `authenticate` in
        // flight** (§7.7), which is the case the two stores this ring declined
        // to build would have told two stories about: core folds it into a
        // refusal, and both halves of this panel read that one value.
        let panel = rendered_auth(
            everything(),
            AuthState::Refused {
                method: browser(),
                error: CallError::Disconnected,
            },
        );

        let said = "the connection to the agent is gone";
        let block = authentication(&panel);
        assert!(
            block.contains("was not accepted") && block.contains(said),
            "the block says what became of the login: {block}"
        );
        let row = row_for(&panel, "authenticate");
        assert!(
            row.contains(" driven and refused") && row.contains(said),
            "and the row says the same thing about it: {row}"
        );
    }

    #[test]
    fn an_agent_asking_for_a_login_nobody_has_answered_leaves_the_row_undriven() {
        // The state that looks most like a disagreement and is not: the agent
        // answered something with `-32000 auth_required`, so the block says a
        // login is wanted — and nobody has sent `authenticate`, so the row says
        // nothing was driven. Two facts about two different things, from one
        // store (§7.7).
        let panel = rendered_auth(
            everything(),
            AuthState::Required(v1::Error::new(
                i32::from(v1::ErrorCode::AuthRequired),
                "authenticate first",
            )),
        );

        let block = authentication(&panel);
        assert!(
            block.contains("The agent wants a login.") && block.contains("authenticate first"),
            "the block says what the agent said: {block}"
        );
        let row = row_for(&panel, "authenticate");
        assert!(
            row.contains(&format!(r#"{DRIVEN} not driven"#)),
            "and the row says nobody has answered it: {row}"
        );
    }

    #[test]
    fn a_login_the_agent_has_taken_back_stops_being_reported_as_driven() {
        // A successful `logout` returns the auth state to unasked, because the
        // agent has retracted what it accepted (§7.7) — so the row stops saying
        // *driven and answered* along with the block above it. What the row
        // reports is where the login stands now, not that a call was once made:
        // the record of how it got there is the trace.
        let row = row_for(
            &rendered_auth(everything(), AuthState::Unasked),
            "authenticate",
        );

        assert!(
            row.contains(&format!(r#"{DRIVEN} not driven"#)),
            "the acceptance the agent took back is not still on the row: {row}"
        );
        // And the row that *is* about the logout says what became of driving it,
        // which is where that call's outcome lives.
        let logged_out = row_for(
            &rendered_driven(DrivenRecord::from_iter([(
                AgentCapability::Logout,
                Driven::Answered,
            )])),
            "logout",
        );
        assert!(
            logged_out.contains(&format!(r#"{DRIVEN} driven and answered"#)),
            "out of the record, where the call that was made is: {logged_out}"
        );
    }

    #[test]
    fn an_agent_that_advertised_no_way_to_log_in_has_a_row_saying_so_and_nothing_more() {
        // Gated on the advertisement like every other row: `authMethods` is the
        // claim `authenticate` hangs off (§7.1), and an agent that listed none
        // has made no claim to have driven.
        let row = row_for(&shown(only_listing(), true), "authenticate");

        assert!(
            row.contains(&format!(r#"{STATE}not advertised"#)),
            "the claim it did not make: {row}"
        );
        assert!(!row.contains(DRIVEN), "and nothing about driving it: {row}");
    }

    #[test]
    fn the_login_is_claimed_in_a_shape_of_its_own_and_the_row_says_which() {
        // An Advertisement is wider than a capability (`CONTEXT.md`): this one
        // is a *list* on the `initialize` result, beside `agentCapabilities`
        // rather than inside it — so the run says so, the way the other two
        // shapes are said (§7.5).
        let rows = rows(&everything());
        let login = rows
            .iter()
            .find(|(.., claim)| claim.name == "authenticate")
            .expect("the login has a row");

        assert_eq!(
            (login.0, login.1, login.2, login.3.field),
            ("Auth", "authMethods", Shape::List, "authMethods"),
            "claimed in the list the agent named its methods in"
        );
        // The words are the caption's own and are deliberately terse — eight of
        // the nine wrapped to two lines in a 280px rail, which is the largest
        // single piece of scaffolding on it. What is asserted is that the
        // *list* shape is named here, which is the half of it a reader cannot
        // infer from `authMethods`.
        assert!(
            advertisement(&shown(everything(), true), "agent").contains("list at "),
            "and the shape is stated where the run is"
        );
    }

    /// How many ways into the session a row offers.
    ///
    /// The two calls that reopen one, counted together: they are one operation
    /// with a property (§7.5), so a row offering both would be the screen asking
    /// the reader to choose between two calls the protocol says are the same
    /// call. The ways *out* are not counted here — close and delete are two
    /// operations, and a row may carry both.
    fn ways_in(row: &str) -> usize {
        row.matches(">session/load</button>").count()
            + row.matches(">session/resume</button>").count()
    }

    /// The affordances at the bottom of the panel, apart from the capability
    /// rows above them — which name the same methods, and are a display of what
    /// was claimed rather than a thing to click.
    fn affordances(html: &str) -> String {
        let from = html
            .rsplit_once(SESSIONS)
            .expect("the session affordances are drawn")
            .1;
        // Bounded by the block that follows rather than by the end of the
        // document. This read to the end while the sessions block happened to be
        // last, which made it a helper that knew the panel's order rather than
        // its shape — and the capability rows name the same methods these
        // buttons do, deliberately (§9), so an unbounded read finds
        // `session/close` in a row that is a claim and calls it a control.
        match from.split_once(r#"data-claim="#) {
            Some((block, _)) => block.to_owned(),
            None => from.to_owned(),
        }
    }

    #[test]
    fn a_session_can_be_opened_whatever_the_agent_advertised_about_sessions() {
        // `session/new` is the one lifecycle method the protocol does not gate
        // (§7.5): every v1 agent opens sessions, so the affordance is there for
        // an agent that advertised nothing at all — while the listing beside it
        // is absent, because that one *is* the agent's claim to make.
        let offered = affordances(&shown(nothing(), true));

        assert!(
            offered.contains("session/new"),
            "the create affordance is not gated on a capability: {offered}"
        );
        assert!(
            !offered.contains("session/list"),
            "and the listing still is: {offered}"
        );
        assert!(
            offered.contains("cancelled first"),
            "what a switch costs is stated where the button is: {offered}"
        );

        // The other half of the same rule: an agent that claimed listing is
        // offered both, and neither of them because there is anything to list.
        let claimed = affordances(&shown(everything(), true));
        assert!(
            claimed.contains("session/new") && claimed.contains("session/list"),
            "{claimed}"
        );
    }

    #[test]
    fn the_listing_claim_and_affordance_cannot_contradict_each_other() {
        let html = rendered_with_listing_state(
            Some(everything()),
            true,
            None,
            ListingState {
                listing: None,
                pending: false,
                advertised: Some(false),
            },
            None,
            undriven(),
            AuthenticationState {
                state: AuthState::default(),
                pending: Vec::new(),
            },
        );
        let claim = row_for(&html, "session/list");
        assert!(claim.contains("not advertised"), "{claim}");
        assert!(
            !affordances(&html).contains(">session/list</button>"),
            "one panel must not say session/list was not advertised while offering it: {html}"
        );
    }

    #[test]
    fn an_agent_that_has_gone_leaves_the_affordance_saying_it_cannot() {
        // What the panel already does with the buttons it had: the claims
        // outlive the agent that made them (§5), and a button that cannot do
        // what it says is worse than one that says it cannot.
        let offered = affordances(&shown(nothing(), false));

        let button = offered
            .split_once("session/new")
            .expect("the create affordance is drawn")
            .0
            .rsplit_once("<button")
            .expect("drawn as a button")
            .1;
        assert!(
            button.contains("disabled"),
            "an agent that is gone cannot be asked for a session: {offered}"
        );
    }

    #[test]
    fn a_listed_session_is_a_way_into_that_session() {
        // The ring's point about the listing (§7.5): it stops being text you
        // can read. Every row the agent named carries the affordance, and the
        // affordance is named by the method it will send — the reader of an
        // inspector is reading the protocol.
        let listed = listing_for(Some(Restore::Load));
        let rows = rows_shown(&listed);

        assert_eq!(rows.len(), 2, "one row per listed session: {listed}");
        for row in rows {
            assert!(
                row.contains("<button") && row.contains("session/load"),
                "each is a way in: {row}"
            );
        }
    }

    #[test]
    fn replay_is_a_property_of_the_call_and_not_two_buttons() {
        // **One screen, one affordance per session** (§7.5). `session/load` and
        // `session/resume` restore the same thing and differ only in whether
        // the conversation replays first, so a row offers the one this agent
        // gets and says what that means — never both, which would ask the
        // reader to choose between two calls the protocol says are the same
        // call.
        for (how, sent, other) in [
            (Restore::Load, "session/load", "session/resume"),
            (Restore::Resume, "session/resume", "session/load"),
        ] {
            let listed = listing_for(Some(how));
            for row in rows_shown(&listed) {
                assert!(row.contains(sent), "{row}");
                assert!(!row.contains(other), "and only the one: {row}");
                // Counted among the ways *in* rather than among the buttons:
                // the row also carries the two ways out (§7.5), and those are
                // two calls the protocol keeps apart rather than one with a
                // property.
                assert_eq!(ways_in(row), 1, "one way in, not two: {row}");
            }
        }
    }

    #[test]
    fn an_agent_that_only_resumes_says_previous_messages_cannot_be_viewed() {
        // Which is the whole reason the screen says anything at all: a resumed
        // session's timeline is empty and *correct*, and a reader left to work
        // that out reads it as a fault in the inspector.
        let resuming = listing_for(Some(Restore::Resume));

        assert!(
            resuming.contains("does not support viewing previous messages"),
            "it says so plainly: {resuming}"
        );
        assert!(
            resuming.contains("correct, not a failure"),
            "and says what the empty timeline means: {resuming}"
        );

        // And the other way round, the replay is what is promised.
        let loading = listing_for(Some(Restore::Load));
        assert!(
            loading.contains("replays its conversation into the timeline before it answers"),
            "{loading}"
        );
        assert!(
            !loading.contains("does not support viewing previous messages"),
            "{loading}"
        );
    }

    #[test]
    fn an_agent_that_advertised_neither_is_offered_neither() {
        // Gated on the advertisement, like everything else here: the listing is
        // still shown — the agent said it lists — and there is nothing to click
        // on a row, with the reason said rather than left as an absence.
        let listed = listing_for(None);

        assert!(
            listed.contains("sess-1"),
            "the listing is still here: {listed}"
        );
        for row in rows_shown(&listed) {
            assert_eq!(ways_in(row), 0, "and no way in: {row}");
        }
        assert!(
            listed.contains("no way in to a session it already has"),
            "with the reason: {listed}"
        );
    }

    #[test]
    fn an_agent_that_has_gone_cannot_be_asked_to_open_one() {
        // The rule the other two buttons already follow: a button that cannot
        // do what it says is worse than one that says it cannot.
        let listed = affordances(&rendered(
            everything(),
            false,
            Some(Restore::Load),
            Some(two_sessions()),
        ));

        for row in rows_shown(&listed) {
            assert!(row.contains("<button") && row.contains("disabled"), "{row}");
        }
    }

    /// Two sessions as an agent that reported roots for one of them would have
    /// described them.
    ///
    /// Its own fixture rather than roots added to [`two_sessions`], because what
    /// a row is prefilled with is the thing being asserted and a listing where
    /// every session reported the same list could not tell a prefill from a
    /// constant.
    fn reporting_roots() -> SessionListing {
        let mut listing = SessionListing::default();
        listing.sessions = vec![
            v1::SessionInfo::new("sess-1", "/tmp")
                .additional_directories(vec!["/srv/data".into(), "/srv/models".into()]),
            v1::SessionInfo::new("sess-2", "/tmp"),
        ];
        listing
    }

    /// Every roots control in a fragment of the screen, one string each — cut at
    /// the label that holds it, which is the whole control.
    fn roots_controls(html: &str) -> Vec<&str> {
        html.split(ROOTS)
            .skip(1)
            .map(|control| {
                control
                    .split_once("</label>")
                    .expect("a control is closed")
                    .0
            })
            .collect()
    }

    #[test]
    fn the_roots_a_session_opens_with_are_asked_for_where_the_agent_advertised_them() {
        // **The advertisement gates the control** (§7.7), like every other
        // affordance here: `additionalDirectories` is a field on a request
        // rather than a method, so what an agent that claimed it gets is a place
        // to supply one — beside the create button, and on each row that reopens
        // a session.
        let offered = affordances(&rendered(
            everything(),
            true,
            Some(Restore::Load),
            Some(two_sessions()),
        ));

        let controls = roots_controls(&offered);
        assert_eq!(
            controls.len(),
            3,
            "one beside `session/new` and one per listed session: {offered}"
        );
        for control in controls {
            assert!(
                control.contains("<textarea"),
                "each is somewhere to type one: {control}"
            );
        }
        // And each is named as the protocol names it — once per control, on the
        // control itself beside `session/new` and on the summary that stands for
        // it on a listing row, where the disclosure is what carries the name.
        assert_eq!(
            offered
                .matches("<code>additionalDirectories</code>")
                .count(),
            3,
            "named once per place there is to supply one: {offered}"
        );
        assert!(
            offered.contains(r#"aria-label="Edit additionalDirectories for a new session""#)
                && offered.contains(r#"title="Edit additionalDirectories for a new session""#)
                && offered.contains(r#"aria-label="additionalDirectories for a new session""#),
            "the new Session disclosure and field retain complete names and a tooltip: {offered}"
        );
    }

    #[test]
    fn an_agent_that_advertised_no_additional_directories_is_offered_nowhere_to_supply_any() {
        // Gated on the advertisement and on nothing else — and the rest of the
        // row is untouched, because the reopen goes on being offered and goes on
        // sending the roots the listing reported (core's rule, asserted in
        // `crates/core/tests/roots.rs`). What is withheld is the control, not the call.
        let listed = affordances(&rendered(
            only_listing(),
            true,
            Some(Restore::Load),
            Some(reporting_roots()),
        ));

        assert!(
            roots_controls(&listed).is_empty(),
            "no claim, no control anywhere: {listed}"
        );
        for row in rows_shown(&listed) {
            assert_eq!(ways_in(row), 1, "and the way in is still here: {row}");
        }
    }

    #[test]
    fn a_reopen_is_prefilled_with_the_roots_the_listing_reported_for_that_session() {
        // **So that reopening stays a reopen unless the user deliberately
        // changes it** (§7.5). Per row, because the roots are the session's: a
        // control that showed one session's roots on another's row would ask an
        // agent to reopen a session as a different one.
        let listed = affordances(&rendered(
            everything(),
            true,
            Some(Restore::Load),
            Some(reporting_roots()),
        ));

        let rows = rows_shown(&listed);
        assert_eq!(rows.len(), 2, "one row per listed session: {listed}");
        assert!(
            rows[0].contains("value=\"/srv/data\n/srv/models\""),
            "the roots this session reported, one per line: {}",
            rows[0]
        );
        assert!(
            rows[1].contains(r#"value="""#),
            "and a session that reported none is prefilled with none: {}",
            rows[1]
        );
    }

    #[test]
    fn a_reopen_asks_for_the_reported_roots_unless_the_user_changed_them() {
        // **What a click means, decided where it can be asserted** (§12). A
        // control the user has not touched is not a change: it says `None`, and
        // core hands the agent back the roots it reported — byte for byte, and
        // not a rendering of them that came back through a textarea. A control
        // they *emptied* is a change like any other and asks for no roots at
        // all, which is the case a fallback to the reported list would get
        // exactly backwards.
        let reported = Roots::reported(&["/srv/data".into()]);

        assert_eq!(
            asking(false, "", &reported),
            None,
            "an agent that advertised nothing drew no control to change"
        );
        assert_eq!(
            asking(true, "/srv/data", &reported),
            None,
            "and a control nobody changed is not a change"
        );
        assert_eq!(
            asking(true, "/srv/data\n/srv/models", &reported),
            Some(Roots::supplied("/srv/data\n/srv/models")),
            "a list they added to is what is asked for"
        );
        assert_eq!(
            asking(true, "", &reported),
            Some(Roots::supplied("")),
            "and one they cleared asks for no roots, rather than for the reported ones again"
        );
    }

    #[test]
    fn the_rule_a_root_must_keep_is_stated_once_above_every_control_it_binds() {
        // The rule the user would otherwise learn from the agent's error — and
        // learn wrongly, because a relative path never reaches the wire: it is
        // resolved against the session's own `cwd` first (§7.7).
        //
        // It used to be said inside each control, which meant an agent
        // advertising `additionalDirectories` with two sessions listed said it
        // three times and one with ten said it eleven. That is the shape §7.7
        // already refused for the fourth fact — one sentence about the inspector
        // drawn as eleven facts about the agent — so it is hoisted here for the
        // same reason and held to the same rule: said once, not dropped.
        let offered = affordances(&rendered(
            everything(),
            true,
            Some(Restore::Load),
            Some(two_sessions()),
        ));

        assert_eq!(
            offered.matches("Each path must be absolute").count(),
            1,
            "one sentence about this tool, said once: {offered}"
        );
        assert!(
            offered.contains("resolved against the session"),
            "and what becomes of a path that is not: {offered}"
        );

        // What stays on every control is the half that labels the box it is
        // beside rather than the half that states the rule.
        let controls = roots_controls(&offered);
        assert!(controls.len() > 1, "more than one control to bind");
        for control in controls {
            assert!(
                control.contains("one absolute path per line"),
                "each control still says what goes in it: {control}"
            );
        }
    }

    #[test]
    fn a_listed_sessions_roots_are_legible_without_opening_its_control() {
        // The control is shut on a listing row (§7.7) — eleven textareas was
        // two and a half screens of one control repeated. What must survive
        // shutting it is the agent's own words: the roots it reported are what
        // the field is prefilled from and what crosses the wire if nobody edits
        // them, so a disclosure that hid them would be buying room with the
        // evidence.
        let offered = affordances(&rendered(
            everything(),
            true,
            Some(Restore::Load),
            Some(two_sessions()),
        ));

        for row in rows_shown(&offered) {
            let summary = row
                .split_once("<summary")
                .expect("a shut control names itself")
                .1
                .split_once("</summary>")
                .expect("a summary that ends")
                .0;
            assert!(
                summary.contains("additionalDirectories"),
                "named by the field it supplies: {summary}"
            );
            assert!(
                summary.contains("/srv") || summary.contains("none reported"),
                "and saying what the agent reported: {summary}"
            );
        }
    }

    #[test]
    fn the_two_gating_shapes_are_not_flattened_into_one() {
        // §7.5: `session/load` is gated by a top-level boolean and the other
        // five by the presence of a capability object. An agent author reading
        // this screen has to be able to see *why* their agent advertises
        // support in two different shapes, so the display keeps them apart and
        // names the field each claim is made in.
        let by_shape = |shape| {
            session_rows(&everything())
                .into_iter()
                .filter(|(_, row, _)| *row == shape)
                .map(|(at, _, claim)| (at, claim.name, claim.field))
                .collect::<Vec<_>>()
        };

        assert_eq!(
            by_shape(Shape::Flag),
            [("agentCapabilities", "session/load", "loadSession")],
            "one boolean, and it is the top-level one"
        );
        assert_eq!(
            by_shape(Shape::Object),
            [
                ("sessionCapabilities", "session/list", "list"),
                ("sessionCapabilities", "session/resume", "resume"),
                ("sessionCapabilities", "session/close", "close"),
                ("sessionCapabilities", "session/delete", "delete"),
                (
                    "sessionCapabilities",
                    "additionalDirectories",
                    "additionalDirectories"
                ),
            ],
            "and the rest are objects, whose presence is the claim"
        );
    }
}
