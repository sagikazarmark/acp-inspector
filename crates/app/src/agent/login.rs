//! The auth methods, where the login stands, and `logout`.

use acp_inspector_core::{AuthState, v1};
use dioxus::prelude::*;

use super::AuthenticationCall;

/// The advertised auth methods, where authentication stands, the way out of a
/// login, and — when the agent has asked for one — where to go and do it.
///
/// **`logout` sits here, with the auth methods** (§7.7), and not among the
/// session controls: the call carries no session id, so its place on screen
/// matches its scope on the wire. It is gated on the advertisement and on
/// nothing else — offered wherever the agent claimed it, including with nobody
/// logged in, because "logout without a login" is a corner the specification
/// leaves entirely undefined and driving it is the point.
pub(super) fn login(
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
                    {wants_login(methods, auth)}
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
                    {method_rows(methods, pending, connected, on_authenticate)}
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
                                    "Log out"
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

/// What to do about a login the agent asked for: its own words, the methods it
/// named, and the two ways through.
///
/// **`authenticate` is a way through, not a label.** For a method of the
/// default `agent` type the agent runs the login itself when it is called —
/// that is what the method type means in the ACP schema — so sending it is
/// how a reader logs in from here. An agent whose login is a command of its
/// own is logged into from a terminal and restarted.
///
/// Neutral, and deliberately: an agent that wants a login is in an ordinary
/// protocol state rather than a broken one (§7.1).
fn wants_login(methods: &[v1::AuthMethod], auth: &AuthState) -> Element {
    rsx! {
        div { class: "alert login", "data-slot": "login-guidance",
            div {
                strong { "The agent wants a login." }
                {outcome(auth)}
                p {
                    {match named_methods(methods) {
                        // Which is a finding: the agent asked for a login and
                        // named no way to do one.
                        None => rsx! { "It advertised no way to do it." },
                        Some(named) => rsx! { "It advertised {named}." },
                    }}
                }
                p {
                    if methods.is_empty() {
                        "Log in with the agent's own login command in a terminal, then restart the agent."
                    } else {
                        "Choose a method below to send "
                        code { "authenticate" }
                        ": for a method the agent handles, that is the login. Or log in with the agent's own login command in a terminal, then restart the agent."
                    }
                }
            }
        }
    }
}

/// The advertised methods, each with the call that uses it.
fn method_rows(
    methods: &[v1::AuthMethod],
    pending: &[AuthenticationCall],
    connected: bool,
    on_authenticate: EventHandler<v1::AuthMethodId>,
) -> Element {
    rsx! {
        // **Not daisyUI's list.** `list-row` is a grid of its own, and these
        // rows are two facts and a call, so the list states its own layout.
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
                        title: "authenticate",
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
}

/// The login guidance and its Authenticate actions, for a screen other than
/// the Capabilities surface to show — or `None` while the agent has not asked
/// for a login (X5). The Timeline's empty state is the caller: a session/new
/// refused for want of a login leaves it with nothing else to say. `main.rs`
/// passes it as the Timeline's `login`.
pub(crate) fn login_guidance(
    methods: &[v1::AuthMethod],
    auth: &AuthState,
    pending: &[AuthenticationCall],
    connected: bool,
    on_authenticate: EventHandler<v1::AuthMethodId>,
) -> Option<Element> {
    auth.needs_login().then(|| {
        rsx! {
            {wants_login(methods, auth)}
            if !methods.is_empty() {
                {method_rows(methods, pending, connected, on_authenticate)}
            }
        }
    })
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
pub(super) fn standing(auth: &AuthState) -> Element {
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

/// What `logout` costs, and the id the button points at for it.
const LOGOUT: &str = "Sends logout. The open session is left as is.";

const LOGOUT_COSTS: &str = "logout-costs";

#[cfg(test)]
mod tests {
    use acp_inspector_core::{AgentCapability, CallError, Driven, DrivenRecord, Restore};

    use crate::agent::rows::Shape;

    use super::*;
    use crate::agent::fixtures::*;

    #[test]
    fn a_login_the_agent_asked_for_can_be_drawn_on_another_screen() {
        #[component]
        fn Host(needs: bool) -> Element {
            let error = v1::Error::auth_required();
            let auth = if needs {
                AuthState::Required(error)
            } else {
                AuthState::default()
            };
            let methods = vec![v1::AuthMethod::Agent(v1::AuthMethodAgent::new(
                "browser",
                "Log in with the browser",
            ))];
            match login_guidance(&methods, &auth, &[], true, EventHandler::new(|_| {})) {
                Some(guidance) => guidance,
                None => rsx! { "nothing" },
            }
        }
        let shown = |needs| {
            let mut dom = VirtualDom::new_with_props(Host, HostProps { needs });
            dom.rebuild_in_place();
            dioxus_ssr::render(&dom)
        };
        let html = shown(true);
        assert!(
            html.contains(r#"data-slot="login-guidance""#)
                && html.contains("Authenticate with Log in with the browser (browser)")
                && html.contains("for a method the agent handles, that is the login")
                && !html.contains("does not perform one"),
            "{html}"
        );
        assert_eq!(shown(false), "nothing");
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

    /// The button that sends `logout`, as it is drawn — never the capability row
    /// of the same name, which is a claim to read rather than a thing to click.
    const LOGOUT: &str = ">Log out</button>";

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
            (AuthState::Unasked, " not called"),
            (AuthState::Authenticated(browser()), " called · OK"),
            (
                AuthState::Refused {
                    method: browser(),
                    error: CallError::Disconnected,
                },
                " called · refused",
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
            driven.contains(&format!(r#"{DRIVEN} not called"#)),
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
            row.contains(" called · refused") && row.contains(said),
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
            row.contains(&format!(r#"{DRIVEN} not called"#)),
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
            row.contains(&format!(r#"{DRIVEN} not called"#)),
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
            logged_out.contains(&format!(r#"{DRIVEN} called · OK"#)),
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
}
