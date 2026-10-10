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

/// An agent that described itself as `capabilities` say and offered no way
/// to log in.
///
/// The `initialize` result rather than the capability object, because one
/// of the rows this panel draws is claimed outside it: `authenticate` is
/// gated on `authMethods`, which sits beside `agentCapabilities` on the
/// result rather than inside it (§7.1).
pub(super) fn described(capabilities: v1::AgentCapabilities) -> v1::InitializeResponse {
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

/// The one way in an agent that offers one offers, as Testy's own does.
pub(super) fn a_login() -> v1::AuthMethod {
    v1::AuthMethod::Agent(v1::AuthMethodAgent::new(
        "browser",
        "Log in with the browser",
    ))
}

/// An agent that answered `initialize` and claimed nothing in it — the other
/// half of every fixture pair here, because a claim not made is an answer
/// too.
pub(super) fn nothing() -> v1::InitializeResponse {
    described(v1::AgentCapabilities::default())
}

/// An agent nobody has driven anything on, which is what every test about
/// the *claims* is about: the fourth fact is what happened afterwards, and
/// these are the rows before anything happened.
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

/// Every row the display draws, in the order it draws them: the group it is
/// under, where it was claimed, in what shape, and the claim itself.
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

/// The panel over an agent that described itself, rendered — the screen
/// itself and not a value on the way to it.
pub(super) fn shown(agent: v1::InitializeResponse, connected: bool) -> String {
    rendered(agent, connected, None, None)
}

/// One advertiser's rows, split off the rendered panel.
///
/// The two advertisements sit in one column and are the same markup by
/// design (#86), so a test asking about one of them has to say which — and
/// a test that asked the whole panel for "advertised" would be answered by
/// the other one.
pub(super) fn advertisement(html: &str, whose: &str) -> String {
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

/// Two sessions, as an agent that lists them would have described them.
///
/// The rows the open affordance hangs off, and the only thing this crate
/// can hold them in: it has no runtime to drive an agent with (§5, §12), so
/// what a real listing looks like is asserted where one can be asked for
/// (`crates/core/tests/restore.rs`).
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

/// The panel over an agent that claims everything, with something already
/// driven on it — which is the record the fourth fact is drawn from (§7.7).
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

/// The panel over an agent that claims everything, with authentication
/// standing wherever the test needs it to.
///
/// The state the affordances here are pointedly *not* gated on (§7.7), which
/// is why it is a fixture rather than a constant: a button that appeared only
/// once somebody had logged in would be this window deciding what may be
/// observed.
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

/// An agent that lists its sessions and claims nothing else about them.
///
/// The fixture for the gates this ring adds: a listing with rows in it and
/// no advertisement behind any of the things a row could do.
pub(super) fn only_listing() -> v1::InitializeResponse {
    described(v1::AgentCapabilities::new().session_capabilities(
        v1::SessionCapabilities::new().list(v1::SessionListCapabilities::new()),
    ))
}

/// What the panel's parts are read by: what each one *is*, never the class
/// it happens to be painted in — a test that matched the paint would be a
/// test the redesign has to be talked out of (#104).
///
/// One list of capability rows, in one group of one advertiser's block.
pub(super) const CAPABILITIES: &str = r#"data-slot="capabilities""#;

/// The mark that repeats what a row says in words, and is decoration.
/// The row's own visible answer, in words.
pub(super) const STATE: &str = r#"data-slot="state">"#;

/// What became of driving the claim, or why nothing did (§7.7) — up to
/// the words themselves, which are what most of the rows are read for.
pub(super) const DRIVEN: &str = r#"data-slot="driven">"#;

/// The shape a run of claims was made in — and, since one run is delimited
/// by the next one's shape, what a run is read by.
pub(super) const SHAPE: &str = r#"data-slot="shape""#;

/// The block of session affordances, which the two blocks above it end at.
pub(super) const SESSIONS: &str = r#"data-slot="sessions""#;

/// Every `<li>` in a fragment of the screen, one string each.
pub(super) fn rows_shown(html: &str) -> Vec<&str> {
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

/// Every run of claims in a fragment of the screen, one string each: the
/// shape it was made in, whatever the run says about nothing here driving
/// it, and its rows.
///
/// Read by the shape rather than by the box it is painted in (#104), which
/// is what delimits a run in the first place: a run is the claims made in
/// one shape at one place in the `initialize` result, so the next shape is
/// where one ends.
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

/// The capability row naming `capability`, out of the agent's own block.
///
/// Off the capability rows rather than off the panel, because the
/// affordances further down name the same methods and are things to click
/// rather than claims to read.
pub(super) fn row_for(panel: &str, capability: &str) -> String {
    capability_rows(&advertisement(panel, "agent"))
        .into_iter()
        .find(|row| row.contains(capability))
        .unwrap_or_else(|| panic!("the {capability} row is drawn: {panel}"))
        .to_owned()
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

/// How many ways into the session a row offers.
///
/// The two calls that reopen one, counted together: they are one operation
/// with a property (§7.5), so a row offering both would be the screen asking
/// the reader to choose between two calls the protocol says are the same
/// call. The ways *out* are not counted here — close and delete are two
/// operations, and a row may carry both.
pub(super) fn ways_in(row: &str) -> usize {
    row.matches(">Load</button>").count() + row.matches(">Resume</button>").count()
}

/// The affordances at the bottom of the panel, apart from the capability
/// rows above them — which name the same methods, and are a display of what
/// was claimed rather than a thing to click.
pub(super) fn affordances(html: &str) -> String {
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

/// Two sessions as an agent that reported roots for one of them would have
/// described them.
///
/// Its own fixture rather than roots added to [`two_sessions`], because what
/// a row is prefilled with is the thing being asserted and a listing where
/// every session reported the same list could not tell a prefill from a
/// constant.
pub(super) fn reporting_roots() -> SessionListing {
    let mut listing = SessionListing::default();
    listing.sessions = vec![
        v1::SessionInfo::new("sess-1", "/tmp")
            .additional_directories(vec!["/srv/data".into(), "/srv/models".into()]),
        v1::SessionInfo::new("sess-2", "/tmp"),
    ];
    listing
}
