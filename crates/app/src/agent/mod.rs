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
//! the claim is chosen at launch and rides the `initialize` *request*
//! ([`ClientClaim`](acp_inspector_core::ClientClaim)), so it is knowable before
//! there is an agent and unchanged by anything one answers.
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

mod accounts;
mod advertisement;
#[cfg(test)]
mod fixtures;
mod login;
mod roots;
mod rows;
mod sessions;

use acp_inspector_core::{
    AuthState, CallError, Cursor, DrivenRecord, Restore, Roots, SessionListing, v1,
};
use dioxus::prelude::*;

use accounts::{advertised, claimed};
use advertisement::{Only, advertises};
pub(crate) use login::login_guidance;
use login::{login, standing};
pub(crate) use sessions::{Listing, SESSION_NEW_COSTS, sessions};

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
    /// What this client claimed about itself and the protocol version it asked
    /// for, and who it said it was — core's own values, so the screen and the
    /// wire cannot say different things.
    pub client: acp_inspector_core::ClientClaim,
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
            client: acp_inspector_core::ClientClaim::default(),
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
                // because it does not depend on one: it is chosen at launch
                // and goes out in the request rather than coming back in the
                // answer (§7.6). Read before a launch, it is what the next
                // agent will be told before it is told.
                Advertiser::Client => rsx! {
                    div { class: "rail-group account",
                        {identity(
                            Advertiser::Client,
                            Some(&client_info),
                            rsx! {
                                span { class: "badge badge-sm", "data-slot": "asked-version",
                                    title: "protocolVersion sent in initialize",
                                    "asks ACP v{client.protocol_version}"
                                }
                            },
                            rsx! {},
                        )}
                        {advertises(Advertiser::Client, claimed(&client.capabilities), only)}
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
                                    span { class: "badge badge-sm", "data-slot": "negotiated-version",
                                        title: "protocolVersion the agent answered with",
                                        "ACP v{agent.protocol_version}"
                                    }
                                    // The agent answered with a version other
                                    // than the one this client asked for.
                                    if agent.protocol_version != client.protocol_version {
                                        span { class: "badge badge-sm badge-warning", "data-slot": "version-mismatch",
                                            "asked for v{client.protocol_version}"
                                        }
                                    }
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
                "In the "
                code { "initialize" }
                " request. Files and terminal are chosen at launch under Client services and answered by you or a rules file. Agents may offer different options to clients with different capabilities."
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::fixtures::*;

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
                && panel.contains(">Log out</button>"),
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

    /// Both accounts, with this client having launched claiming file reads at
    /// protocol version 2 against an agent that answered version 1.
    fn launched_at_version_two() -> (String, String) {
        #[component]
        fn Host(whose: Advertiser) -> Element {
            let services = acp_inspector_core::ClientServices {
                read_text_file: true,
                protocol_version: 2,
                ..acp_inspector_core::ClientServices::default()
            };
            let client = acp_inspector_core::ClientClaim {
                capabilities: services.capabilities(),
                protocol_version: services.protocol(),
            };
            rsx! {
                AgentClaims {
                    claims: Claims { client, ..Claims::about(Some(everything())) },
                    whose,
                }
            }
        }
        let render = |whose| {
            let mut dom = VirtualDom::new_with_props(Host, HostProps { whose });
            dom.rebuild_in_place();
            dioxus_ssr::render(&dom)
        };
        (render(Advertiser::Agent), render(Advertiser::Client))
    }

    #[test]
    fn a_version_other_than_the_one_asked_for_is_flagged() {
        let (agent, client) = launched_at_version_two();
        assert!(
            agent.contains(r#"data-slot="negotiated-version""#),
            "{agent}"
        );
        assert!(
            agent.contains(r#"data-slot="version-mismatch""#) && agent.contains("asked for v2"),
            "{agent}"
        );
        assert!(client.contains("asks ACP v2"), "{client}");
    }

    #[test]
    fn the_launchs_claims_are_drawn_as_claimed() {
        let (_, client) = launched_at_version_two();
        let client = advertisement(&client, "client");
        for (row, advertised) in [
            ("fs/read_text_file", true),
            ("fs/write_text_file", false),
            ("terminal/*", false),
            (">compaction<", true),
            (">notices<", true),
        ] {
            let said = client
                .split_once(row)
                .unwrap_or_else(|| panic!("{row} has a row: {client}"))
                .1
                .split_once(STATE)
                .expect("the row says its state")
                .1;
            assert_eq!(said.starts_with("advertised"), advertised, "{row}: {said}");
        }
    }

    #[test]
    fn the_default_claim_is_not_flagged() {
        let html = shown(everything(), true);
        assert!(!html.contains("version-mismatch"), "{html}");
    }
}
