//! The two accounts as rows: what the agent advertised and what this client
//! claimed.

use acp_inspector_core::{AgentCapability, AuthState, DrivenRecord, Restore, v1};

use super::rows::{Claim, Claimed, Shape, Unreached};

/// The agent's account as rows: each capability by its protocol name, the field
/// that gates it, whether it was advertised, and its fourth fact.
///
/// Stable capabilities only; `unstable_*` claims stay readable in the
/// `initialize` frame (§8). The list is written by hand and pinned to the
/// schema by `the_capability_set_the_display_names_is_the_whole_stable_set_v1_defines`
/// in `crates/core/tests/capability.rs`.
///
/// Rows nothing in this tool can reach say why instead of drawing an outcome:
/// resume where load is preferred, and close/delete with no listing to press
/// them on (§7.7). A row the agent did not advertise says nothing about driving.
pub(super) fn advertised(
    agent: &v1::InitializeResponse,
    record: &DrivenRecord,
    auth: &AuthState,
    lists_sessions: bool,
) -> Vec<(&'static str, Vec<Claimed>)> {
    // The whole result, because `authMethods` sits beside `agentCapabilities`.
    let capabilities = &agent.agent_capabilities;
    let sessions = &capabilities.session_capabilities;
    let prompt = &capabilities.prompt_capabilities;
    let mcp = &capabilities.mcp_capabilities;

    // Core's preference (§7.5): beside an advertised load, resume is never sent.
    let resume = Claim::new("session/resume", "resume", sessions.resume.is_some());
    let resume = match Restore::preferred(capabilities) {
        Some(Restore::Load) => resume.unreached(Unreached::LoadPreferred),
        _ => resume.driven(record, AgentCapability::Resume),
    };
    // Close and delete are pressed on a listing row, so without `session/list`
    // nothing here can reach them.
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
                        .driven(record, AgentCapability::EmbeddedContext),
                ],
            }],
        ),
        (
            "MCP servers",
            vec![Claimed {
                shape: Shape::Flag,
                at: "mcpCapabilities",
                claims: vec![
                    Claim::self_named("http", mcp.http).driven(record, AgentCapability::McpHttp),
                    Claim::self_named("sse", mcp.sse).driven(record, AgentCapability::McpSse),
                ],
            }],
        ),
        (
            "Auth",
            vec![
                // Its own run: `authMethods` is a list on the `initialize`
                // result, outside `agentCapabilities`.
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

/// This client's account as rows, read from the claim core sends so the screen
/// and the wire cannot differ. Claims come before declines, and elicitation is
/// one row per mode because agents gate on the modes (§7.8).
///
/// Pinned to the schema like the agent's list, by
/// `the_client_capability_set_the_display_names_is_the_whole_stable_set_v1_defines`
/// in `crates/core/tests/capability.rs`.
pub(super) fn claimed(capabilities: &v1::ClientCapabilities) -> Vec<(&'static str, Vec<Claimed>)> {
    let fs = &capabilities.fs;
    let session = capabilities.session.as_ref();
    let boolean_config_options = session
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
        // Schema 1.11's two session shapes, which the timeline draws.
        (
            "Session",
            vec![Claimed {
                shape: Shape::Object,
                at: "session",
                claims: vec![
                    Claim::self_named(
                        "compaction",
                        session.is_some_and(|session| session.compaction.is_some()),
                    ),
                    Claim::self_named(
                        "notices",
                        session.is_some_and(|session| session.notices.is_some()),
                    ),
                ],
            }],
        ),
        // One row per mode: an agent gates on the modes, not the object (§7.8).
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::fixtures::*;

    /// The rows of the sessions group.
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
        // A row reading the wrong field would be this window making a claim.
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
        // An agent that advertised none is still six rows, not an empty list.
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
        let rows = session_rows(&nothing());

        assert_eq!(rows.len(), 6);
        assert!(
            rows.iter().all(|(_, _, claim)| !claim.advertised),
            "{rows:?}"
        );
    }

    #[test]
    fn what_this_client_advertised_is_shown_beside_what_the_agent_advertised() {
        // A boolean config option has no visible cause without the client's
        // claim on screen.
        let panel = shown(everything(), true);

        let client = advertisement(&panel, "client");
        assert!(
            client.contains("boolean") && client.contains(&format!(r#"{STATE}advertised"#)),
            "the data shape this client claims about itself: {client}"
        );
        for mode in ["form", "url"] {
            assert!(
                client.contains(mode),
                "the {mode} elicitation mode is claimed on screen too: {client}"
            );
        }
        // And the agent's account is still drawn, unmerged.
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
        // The services this client would have to perform stay declined, on
        // screen as on the wire (§7.3).
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
        // Schema 1.7 makes this a stable client claim, which this client
        // declines (§7.1).
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
        // The claim goes out in the request, so it is knowable before an agent
        // exists (§7.6).
        let panel = rendered_without_an_agent();
        let client = advertisement(&panel, "client");

        assert!(
            client.contains("boolean") && client.contains(&format!(r#"{STATE}advertised"#)),
            "what the next agent will be told, before there is one: {client}"
        );
    }

    #[test]
    fn the_clients_advertisement_is_drawn_as_plainly_as_the_agents() {
        // Same markup, same state in words: neither account is a footnote.
        let panel = shown(everything(), true);
        let client = advertisement(&panel, "client");
        let rows = capability_rows(&client);

        assert!(!rows.is_empty(), "the client's claims are rows: {client}");
        for row in rows {
            assert!(row.contains(STATE), "each says its state in words: {row}");
        }

        assert!(
            client.contains(SHAPE),
            "the shape of the client's claims is stated too: {client}"
        );
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

    #[test]
    fn the_two_gating_shapes_are_not_flattened_into_one() {
        // `session/load` is a top-level boolean and the other five are
        // capability objects; the display keeps the two shapes apart (§7.5).
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
