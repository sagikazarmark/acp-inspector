//! The two accounts as rows: what the agent advertised and what this client
//! claimed.

use acp_inspector_core::{AgentCapability, AuthState, DrivenRecord, Restore, v1};

use super::rows::{Claim, Claimed, Shape, Unreached};

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
pub(super) fn advertised(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::fixtures::*;

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
