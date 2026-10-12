//! The auth methods, where the login stands, and `logout`. See
//! `docs/architecture.md` §7.1 and §7.7.

use acp_inspector_core::{AuthState, v1};
use dioxus::prelude::*;

use super::AuthenticationCall;

/// The advertised auth methods, where authentication stands, and `logout`.
///
/// `logout` carries no session id, so it sits here rather than among the
/// session controls. It is gated on the advertisement alone, so it is offered
/// even with nobody logged in.
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

                // No methods: the `authenticate` row below already says so.
                if !methods.is_empty() {
                    {method_rows(methods, pending, connected, on_authenticate)}
                }

                // Laid out like a method row: the call, its consequence, the button.
                if logs_out {
                    div { class: "auth-method logout",
                            div {
                                strong { class: "mono", "logout" }
                                // Visible, and still the button's description.
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
/// For a method of the default `agent` type the agent runs the login itself
/// when `authenticate` is sent; otherwise the reader logs in from a terminal
/// and restarts the agent. Neutral in tone: wanting a login is an ordinary
/// protocol state (§7.1).
fn wants_login(methods: &[v1::AuthMethod], auth: &AuthState) -> Element {
    rsx! {
        div { class: "alert login", "data-slot": "login-guidance",
            div {
                strong { "The agent wants a login." }
                {outcome(auth)}
                p {
                    {match named_methods(methods) {
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
        // Not daisyUI's `list-row`, whose grid does not fit two facts and a call.
        ul { class: "methods",
            for (key, method) in crate::keys::distinct(methods.iter().map(|method| method.id().to_string())).iter().zip(methods) {
                li { key: "{key}", class: "auth-method",
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

/// The login guidance and its Authenticate actions for another screen (the
/// Timeline's empty state), or `None` while the agent has not asked for a login.
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

/// The advertised methods by name, or `None` when it advertised none.
fn named_methods(methods: &[v1::AuthMethod]) -> Option<String> {
    let named: Vec<_> = methods.iter().map(v1::AuthMethod::name).collect();
    (!named.is_empty()).then(|| named.join(", "))
}

/// What became of authentication, in the agent's own words where it said any.
fn outcome(auth: &AuthState) -> Element {
    match auth {
        AuthState::Unasked => rsx! {},
        // The agent's code and message, as written.
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
        AuthState::Refused { method, error } => rsx! {
            p { class: "detail", role: "status", aria_live: "polite",
                code { "{method}" }
                " was not accepted: {error}"
            }
        },
        // `AuthState` is non-exhaustive: an unknown state is not drawn as a known one.
        _ => rsx! {
            p { class: "detail", role: "status", aria_live: "polite",
                "An authentication state this window has no rendering for."
            }
        },
    }
}

/// Where the login stands, in a few words for the identity line. Nothing for
/// `Unasked`: nobody has tried to log in, which is not "logged out".
pub(super) fn standing(auth: &AuthState) -> Element {
    let (said, tone) = match auth {
        AuthState::Required(_) => ("Login required", "standing-wanted"),
        AuthState::Authenticated(_) => ("Authenticated", "standing-done"),
        AuthState::Refused { .. } => ("Login refused", "standing-wanted"),
        // Including unknown states, which the block below describes.
        _ => return rsx! {},
    };

    rsx! {
        span { class: "standing {tone}", "data-slot": "standing", "{said}" }
    }
}

/// What `logout` costs, and the id the button is described by.
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

    /// The `logout` button, not the capability row of the same name.
    const LOGOUT: &str = ">Log out</button>";

    #[test]
    fn the_logout_button_sits_with_the_auth_methods_and_not_among_the_session_controls() {
        // `logout` carries no session id, so it is not a session control.
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
        let panel = shown(only_listing(), true);

        assert!(
            !panel.contains(LOGOUT),
            "no claim, no button anywhere: {panel}"
        );
    }

    #[test]
    fn the_logout_button_is_offered_whatever_the_agent_has_said_about_logging_in() {
        // Gated on the advertisement, not on the auth state (§7.7).
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
        let auth = authentication(&shown(everything(), false));

        assert!(
            auth.contains(&format!("disabled=true{LOGOUT}")),
            "the button says it cannot: {auth}"
        );
    }

    #[test]
    fn the_auth_row_draws_its_fourth_fact_from_the_auth_state() {
        // `authenticate` has no entry in the driven record; the row reads
        // `AuthState`, in each of its three states (§7.7).
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

        // A full driven record does not change the row.
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
        // Including a connection that died under `authenticate`, which core
        // folds into a refusal.
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
        // A login is wanted, and nobody has sent `authenticate` yet.
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
        // A successful `logout` returns the auth state to unasked (§7.7).
        let row = row_for(
            &rendered_auth(everything(), AuthState::Unasked),
            "authenticate",
        );

        assert!(
            row.contains(&format!(r#"{DRIVEN} not called"#)),
            "the acceptance the agent took back is not still on the row: {row}"
        );
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
        let row = row_for(&shown(only_listing(), true), "authenticate");

        assert!(
            row.contains(&format!(r#"{STATE}not advertised"#)),
            "the claim it did not make: {row}"
        );
        assert!(!row.contains(DRIVEN), "and nothing about driving it: {row}");
    }

    #[test]
    fn the_login_is_claimed_in_a_shape_of_its_own_and_the_row_says_which() {
        // `authMethods` is a list beside `agentCapabilities`, not inside it.
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
        assert!(
            advertisement(&shown(everything(), true), "agent").contains("list at "),
            "and the shape is stated where the run is"
        );
    }
}
