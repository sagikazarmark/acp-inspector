//! The Agent screen: each party's account of itself, and the login affordances
//! that hang off the agent's.
//!
//! Everything is read from the `initialize` exchange and nothing is inferred
//! from later traffic: an affordance appears because the agent advertised it,
//! and whether it did is core's answer. The agent's and this client's accounts
//! are drawn by the same code at the same weight, and a claim not made is a row
//! that says so rather than an omission. See `docs/architecture.md` §7.5–§7.7
//! and §9.

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

/// A lifecycle call that failed, named by its method: which advertised method
/// failed is the finding, not that something did.
#[derive(Clone, Debug, PartialEq)]
pub struct Failed {
    /// The method that failed, by its name on the wire.
    pub method: &'static str,
    /// What the agent said, or the news that there was nobody to ask.
    pub error: CallError,
}

impl Failed {
    /// Pairs a call's error with its method: `outcome.err().map(Failed::of(…))`.
    pub fn of(method: &'static str) -> impl Fn(CallError) -> Self {
        move |error| Self { method, error }
    }
}

/// A listed session to reopen, and the roots to reopen it with. `None` roots
/// ask core to send the roots the listing reported.
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

/// Everything the Agent screen and the Sessions dialog draw from.
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
    /// This client's claim and requested protocol version, as core sends them.
    pub client: acp_inspector_core::ClientClaim,
    pub client_info: v1::Implementation,
    /// What became of driving each advertisement on this connection (§7.7).
    pub driven: DrivenRecord,
    /// Where authentication stands; the `authenticate` row reads it.
    pub auth: AuthState,
    /// Whether the agent advertised listing, which decides whether close and
    /// delete have a row to be pressed on.
    pub lists_sessions: bool,
}

impl Claims {
    /// The session half of the inputs, as the Sessions dialog takes it.
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

    /// Inputs for a test or mock about what is drawn: every handler is a no-op
    /// and every affordance is present.
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
#[component]
pub fn AgentClaims(claims: Claims, whose: Advertiser) -> Element {
    // Which rows are shown. Reset when moving between the accounts: a
    // narrowing belongs to the account it was asked of.
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
        // No `data-advertiser` here: the account inside carries it.
        div { class: "rail-group claims", "data-slot": "claims",
            match whose {
                // This client's claim needs no agent: it is chosen at launch
                // and sent in the request.
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
                        // Some agents demand a login before describing themselves.
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
                                    if agent.protocol_version != client.protocol_version {
                                        span { class: "badge badge-sm badge-warning", "data-slot": "version-mismatch",
                                            "asked for v{client.protocol_version}"
                                        }
                                    }
                                },
                                standing(&auth),
                            )}
                            // Only where there is a login to draw; what the agent
                            // claimed about auth is in the rows below either way.
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

/// Who the party says it is. `agentInfo` is optional in v1, so its absence is
/// said rather than filled in.
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

/// Who made an Advertisement (`CONTEXT.md`): the agent or this client. The two
/// are never merged into one list; who claimed a thing is the point.
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
            Self::Agent => "Agent advertisement",
            Self::Client => "Client advertisement",
        }
    }

    /// Which account a block is, for tests. No style rule may read it: the two
    /// accounts are drawn identically, which a test in `style.rs` holds.
    fn whose(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Client => "client",
        }
    }

    /// Where the claims were made, and for this client why they matter.
    fn caption(self) -> Element {
        match self {
            Self::Agent => rsx! {
                "In its "
                code { "initialize" }
                " result: per-connection, and true of every session on it."
            },
            // An agent may gate on this claim, which explains option shapes
            // that would otherwise have no visible cause (§7.6).
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
