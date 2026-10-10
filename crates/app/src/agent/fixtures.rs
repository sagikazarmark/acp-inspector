//! Shared fixtures for the agent screens' tests.

use acp_inspector_core::{AuthState, DrivenRecord, ProtocolVersion, Restore, SessionListing, v1};
use dioxus::prelude::*;

use super::accounts::advertised;
use super::rows::{Claim, Claimed, Shape};
use super::{Advertiser, AgentClaims, AuthenticationCall, Claims, Failed};

/// The verb a session control is labelled with, for the method it sends.
pub(super) fn verb(method: &str) -> &'static str {
    match method {
        "session/load" => "Load",
        "session/resume" => "Resume",
        "session/close" => "Close",
        "session/delete" => "Delete",
        other => panic!("no verb for {other}"),
    }
}

/// An `initialize` result with these capabilities and no auth methods.
pub(super) fn described(capabilities: v1::AgentCapabilities) -> v1::InitializeResponse {
    v1::InitializeResponse::new(ProtocolVersion::V1).agent_capabilities(capabilities)
}

/// Everything a v1 agent can advertise, including an auth method, as Testy
/// does. That Testy still claims all of it is asserted in
/// `crates/core/tests/capability.rs`.
pub(super) fn everything() -> v1::InitializeResponse {
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
pub(super) fn identified() -> v1::InitializeResponse {
    everything().agent_info(v1::Implementation::new("test-agent", "1.2.3").title("Test Agent"))
}

/// Testy's auth method.
pub(super) fn a_login() -> v1::AuthMethod {
    v1::AuthMethod::Agent(v1::AuthMethodAgent::new(
        "browser",
        "Log in with the browser",
    ))
}

/// An agent that answered `initialize` and claimed nothing.
pub(super) fn nothing() -> v1::InitializeResponse {
    described(v1::AgentCapabilities::default())
}

/// A record with nothing driven.
pub(super) fn undriven() -> DrivenRecord {
    DrivenRecord::default()
}

/// The rows an agent's own account of itself is drawn as, with nothing
/// driven and nobody logged in.
pub(super) fn claims(agent: &v1::InitializeResponse) -> Vec<(&'static str, Vec<Claimed>)> {
    advertised(
        agent,
        &undriven(),
        &AuthState::default(),
        agent.agent_capabilities.session_capabilities.list.is_some(),
    )
}

/// Every row in drawing order: group, where it was claimed, shape, claim.
pub(super) fn rows(
    agent: &v1::InitializeResponse,
) -> Vec<(&'static str, &'static str, Shape, Claim)> {
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

/// The rendered panel over an agent that described itself.
pub(super) fn shown(agent: v1::InitializeResponse, connected: bool) -> String {
    rendered(agent, connected, None, None)
}

/// One advertiser's block, split off the rendered panel. The two blocks are
/// the same markup, so a test has to say which one it means.
pub(super) fn advertisement(html: &str, whose: &str) -> String {
    let block = html
        .split_once(&format!(r#"data-advertiser="{whose}""#))
        .unwrap_or_else(|| panic!("the {whose}'s advertisement is drawn: {html}"))
        .1;

    // Up to the next claim block, which is always a sibling.
    block
        .split_once(r#"data-claim="#)
        .map_or(block, |(block, _)| block)
        .to_owned()
}

/// Two listed sessions. Real listings are asserted in
/// `crates/core/tests/restore.rs`.
pub(super) fn two_sessions() -> SessionListing {
    let mut listing = SessionListing::default();
    listing.sessions = vec![
        v1::SessionInfo::new("sess-1", "/tmp").title("The first one"),
        v1::SessionInfo::new("sess-2", "/tmp"),
    ];
    listing
}

/// The panel, with whatever the agent answered into its listing and
/// whichever way in it advertised.
pub(super) fn rendered(
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

/// The panel over an agent that claims everything, with this driven record.
pub(super) fn rendered_driven(driven: DrivenRecord) -> String {
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

/// The panel over this agent, with this auth state.
pub(super) fn rendered_auth(agent: v1::InitializeResponse, auth: AuthState) -> String {
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
pub(super) fn rendered_with(
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

pub(super) struct ListingState {
    pub(super) listing: Option<SessionListing>,
    pub(super) pending: bool,
    pub(super) advertised: Option<bool>,
}

pub(super) struct AuthenticationState {
    pub(super) state: AuthState,
    pub(super) pending: Vec<AuthenticationCall>,
}

pub(super) fn rendered_with_listing_state(
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
        // Gates read off the fixture's own claims, so it cannot say one thing
        // and draw another.
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
            // Both accounts and the Sessions dialog: tests ask whether the
            // window draws a fact, not which surface holds it.
            for whose in [Advertiser::Agent, Advertiser::Client] {
                AgentClaims { key: "{whose.whose()}", claims: claims.clone(), whose }
            }
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

/// An agent that lists its sessions and claims nothing else about them.
pub(super) fn only_listing() -> v1::InitializeResponse {
    described(v1::AgentCapabilities::new().session_capabilities(
        v1::SessionCapabilities::new().list(v1::SessionListCapabilities::new()),
    ))
}

// Markers tests read the panel by: what a part is, never the class it is
// painted with.

/// One list of capability rows.
pub(super) const CAPABILITIES: &str = r#"data-slot="capabilities""#;

/// A row's visible answer, in words.
pub(super) const STATE: &str = r#"data-slot="state">"#;

/// What became of driving the claim, or why nothing could (§7.7).
pub(super) const DRIVEN: &str = r#"data-slot="driven">"#;

/// The shape a run of claims was made in, which also delimits runs.
pub(super) const SHAPE: &str = r#"data-slot="shape""#;

/// The block of session affordances.
pub(super) const SESSIONS: &str = r#"data-slot="sessions""#;

/// Every `<li>` in a fragment of the screen, one string each.
pub(super) fn rows_shown(html: &str) -> Vec<&str> {
    // Bounded at the row's end, so surface controls are not counted as a row's.
    html.split("<li")
        .skip(1)
        .map(|row| row.split_once("</li>").map_or(row, |(row, _)| row))
        .collect()
}

/// The capability rows only, not the listing's rows.
pub(super) fn capability_rows(html: &str) -> Vec<&str> {
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

/// Every run of claims in a fragment: its shape, any shared reason, its rows.
pub(super) fn runs(html: &str) -> Vec<&str> {
    const CLOSE: &str = "</ul>";
    html.split(SHAPE)
        .skip(1)
        .map(|run| {
            let end = run.find(CLOSE).expect("a run's capability list is closed");
            &run[..end + CLOSE.len()]
        })
        .collect()
}

/// The capability row naming `capability` in the agent's block, not the
/// affordance of the same name.
pub(super) fn row_for(panel: &str, capability: &str) -> String {
    capability_rows(&advertisement(panel, "agent"))
        .into_iter()
        .find(|row| row.contains(capability))
        .unwrap_or_else(|| panic!("the {capability} row is drawn: {panel}"))
        .to_owned()
}

/// The authentication block, cut before the advertisement whose rows name
/// the same calls as claims.
pub(super) fn authentication(html: &str) -> String {
    let block = html
        .split_once(r#"data-claim="authentication""#)
        .expect("the auth block is drawn")
        .1;

    block
        .split_once(r#"data-claim="advertised""#)
        .map_or(block, |(auth, _)| auth)
        .to_owned()
}

/// The method the panel's fixtures advertise a way in with.
pub(super) fn browser() -> v1::AuthMethodId {
    v1::AuthMethodId::new("browser")
}

/// How many ways into the session a row offers: load and resume together,
/// since they are one operation (§7.5).
pub(super) fn ways_in(row: &str) -> usize {
    row.matches(">Load</button>").count() + row.matches(">Resume</button>").count()
}

/// The session affordances, apart from the capability rows that name the
/// same methods.
pub(super) fn affordances(html: &str) -> String {
    let from = html
        .rsplit_once(SESSIONS)
        .expect("the session affordances are drawn")
        .1;
    // Bounded by the next block, so a claim row is never read as a control.
    match from.split_once(r#"data-claim="#) {
        Some((block, _)) => block.to_owned(),
        None => from.to_owned(),
    }
}

/// Two listed sessions, only one of which reported roots, so a prefill can be
/// told from a constant.
pub(super) fn reporting_roots() -> SessionListing {
    let mut listing = SessionListing::default();
    listing.sessions = vec![
        v1::SessionInfo::new("sess-1", "/tmp")
            .additional_directories(vec!["/srv/data".into(), "/srv/models".into()]),
        v1::SessionInfo::new("sess-2", "/tmp"),
    ];
    listing
}
