//! The spawn form (`docs/architecture.md` §9): four fields, a button that hands
//! them to core, and the few invocations that worked before.
//!
//! The fields are text and they leave as text — [`AgentCommand`] is where a
//! line becomes an argument, so the rules live somewhere they can be tested and
//! somewhere a second surface would inherit them from.
//!
//! The recent list is the convenience the spec allows and stops at (§1.1): a
//! click puts an invocation back in the fields, and the Launch button that was
//! always there runs it. Nothing here decides what is remembered or where it is
//! kept — that is [`RecentCommands`](acp_inspector_core::RecentCommands), for the
//! same reason as everything else in this crate (§5) — and nothing here edits,
//! names, imports or exports one, because the catalog that would do those is
//! deferred and this is what was built instead.
//!
//! **The form does not fold, because it is not on the window** (§9). It folded
//! for as long as it was a permanent column: written once per connection, read
//! never afterwards, and four fifths of the rail at the default window size, so
//! the agent's own account of itself began below it from the first launch
//! onward. A dialog answers the same measurement better than a fold does — one
//! that is not open takes no room at all — so the fold, the chevron that worked
//! it, and the line naming the running agent all go with the column. What is
//! running is on the toolbar, which is where the window says what it is
//! connected to whatever else is on screen ([`crate::connect`]).

use acp_inspector_core::AgentCommand;
use dioxus::prelude::*;

/// The four fields, a button that hands them over as typed, and what has been
/// launched before.
#[component]
pub fn SpawnForm(
    running: bool,
    /// Whether the launch handshake has returned. A Connection exists before
    /// initialize and session/new finish, so this is narrower than `running`.
    launching: bool,
    /// The invocations that worked, newest first — core's list, rendered.
    recent: Vec<AgentCommand>,
    /// Why the list will not outlive this window, if it will not. A form that
    /// half-remembers is worse than one that does not pretend to, so a list that
    /// could not be written says so where it is shown.
    unsaved: Option<String>,
    on_launch: EventHandler<AgentCommand>,
) -> Element {
    let mut command = use_signal(String::new);
    let mut args = use_signal(String::new);
    let mut env = use_signal(String::new);
    let mut cwd = use_signal(String::new);
    let mut recall_confirmation = use_signal(|| None::<String>);
    let mut clear_recall_confirmation = move || recall_confirmation.set(None);

    let agent = use_memo(move || AgentCommand {
        command: command(),
        args: args(),
        env: env(),
        cwd: cwd(),
    });

    // Selecting a remembered invocation is a *refill*, not a launch: the fields
    // are where a run is decided, and an agent that started because a list was
    // clicked would be a form that ran something the user had not looked at.
    // Launch is the click after it, in the place it always is.
    let mut refill = move |entry: &AgentCommand| {
        let invocation = invocation(entry);
        command.set(entry.command.clone());
        args.set(entry.args.clone());
        env.set(entry.env.clone());
        cwd.set(entry.cwd.clone());
        recall_confirmation.set(Some(format!(
            "Invocation refilled: {invocation}. Review the fields, then Launch."
        )));
    };

    let launch_available = !launching && !running && agent().is_runnable() && crate::mcp::valid();

    rsx! {
        div { class: "dialog-body", "data-slot": "spawn",
            crate::mcp::McpEditor { prefix:"launch" }
            // The four fields the transport takes, each named by the key it
            // goes out under and what it is for — which is how the drawing lays
            // a form out, and what makes a field legible to somebody reading
            // the protocol rather than the label.
            div { class: "form-field",
                div { class: "form-label",
                    span { class: "form-key", "cmd" }
                    span { class: "form-note", "command to spawn" }
                }
                input {
                    class: "input",
                    value: "{command}",
                    spellcheck: false,
                    aria_label: "Command",
                    // **The platform focuses it, once per opening.**
                    // `showModal` puts the keyboard on the autofocus element of
                    // the dialog it is showing.
                    autofocus: true,
                    placeholder: "npx @zed-industries/claude-code-acp",
                    oninput: move |event| {
                        command.set(event.value());
                        clear_recall_confirmation();
                    },
                }
            }
            div { class: "form-field",
                div { class: "form-label",
                    span { class: "form-key", "args" }
                    span { class: "form-note", "arguments, one per line" }
                }
                textarea {
                    class: "textarea",
                    rows: 3,
                    value: "{args}",
                    spellcheck: false,
                    aria_label: "Arguments",
                    placeholder: "--log-level debug",
                    oninput: move |event| {
                        args.set(event.value());
                        clear_recall_confirmation();
                    },
                }
            }
            div { class: "form-field",
                div { class: "form-label",
                    span { class: "form-key", "env" }
                    span { class: "form-note", "environment, KEY=value per line" }
                }
                textarea {
                    class: "textarea",
                    rows: 2,
                    value: "{env}",
                    spellcheck: false,
                    aria_label: "Environment",
                    placeholder: "ACP_LOG=debug",
                    oninput: move |event| {
                        env.set(event.value());
                        clear_recall_confirmation();
                    },
                }
            }
            div { class: "form-field",
                div { class: "form-label",
                    span { class: "form-key", "cwd" }
                    span { class: "form-note", "session working directory" }
                }
                input {
                    class: "input",
                    value: "{cwd}",
                    spellcheck: false,
                    aria_label: "Working directory",
                    placeholder: "the inspector's own",
                    oninput: move |event| {
                        cwd.set(event.value());
                        clear_recall_confirmation();
                    },
                }
            }

            if !recent.is_empty() {
                div { class: "form-field",
                    div { class: "form-label",
                        span { class: "eyebrow", "Recent" }
                    }
                    div { class: "recents",
                        for (nth , entry) in recent.iter().enumerate() {
                            // By position, because the list *is* an order: the
                            // same invocation relaunched moves to the front,
                            // and a key made of its text would move a row the
                            // reader would rather saw redrawn where it now is.
                            div { class: "recent-row", key: "{nth}",
                                span { class: "recent-kind", "stdio" }
                                button {
                                    class: "recent-label recall",
                                    r#type: "button",
                                    // The whole invocation, and what pressing
                                    // it does: the row shows the program and
                                    // its arguments, and a click is a refill
                                    // rather than the launch a list of past
                                    // runs might be read as offering.
                                    title: "Put this back in the form: {invocation(entry)}",
                                    onclick: {
                                        let entry = entry.clone();
                                        move |_| refill(&entry)
                                    },
                                    {running_label(entry)}
                                }
                                if let Some(under) = under(entry) {
                                    span { class: "recent-when", title: "{under}", "{under}" }
                                }
                            }
                        }
                    }
                    p {
                        class: "hint",
                        "data-slot": "recall-confirmation",
                        role: "status",
                        aria_live: "polite",
                        aria_atomic: "true",
                        if let Some(message) = recall_confirmation() {
                            "{message}"
                        }
                    }
                }
            }

            // Under the form and not on a row of the list, because it is about
            // the launch that just worked rather than about any one invocation
            // in it.
            if let Some(reason) = &unsaved {
                p { class: "alert alert-warning", "data-slot": "unsaved", role: "status",
                    "Launched, but not remembered for next time: {reason}."
                }
            }
        }

        div { class: "dialog-foot",
            p { class: "hint",
                if running {
                    "connected — launching again ends the agent that is running"
                } else {
                    "cwd is sent with session/new · must be absolute"
                }
            }
            button {
                class: "btn btn-md btn-quiet",
                r#type: "button",
                onclick: move |_| crate::connect::close(),
                "Cancel"
            }
            button {
                class: "btn btn-md btn-filled launch",
                r#type: "button",
                // Running: one connection at a time, and stopping is a decision
                // taken deliberately rather than by relaunching over the top of
                // an agent mid-turn.
                aria_disabled: (!launch_available).then_some("true"),
                tabindex: (!launch_available).then_some("-1"),
                onclick: move |_| {
                    if launch_available {
                        clear_recall_confirmation();
                        on_launch.call(agent());
                    }
                },
                if launching { "Launching" } else if running { "Running" } else { "Launch" }
            }
        }
    }
}

/// The program a path names, without the directories that led to it.
///
/// **The end of a command path is the identifying part of it**, and the end is
/// what a line that runs out of room loses: `/home/laborant/…/.testy/bin/testy`
/// truncates to `/home/laborant/dioxus-chat.orig/phoe…`, which is every
/// invocation under one checkout drawn identically. The whole of it is on the
/// row's tooltip, which is where a label keeps what it does not show.
fn program(command: &str) -> &str {
    let command = command.trim();
    command
        .rsplit_once('/')
        .map_or(command, |(_, program)| program)
}

/// The last of a path, kept when the whole of it will not fit.
///
/// Two segments and an ellipsis: enough to tell `…/phoenix/inspector` from
/// `…/phoenix/bridge`, which is the question a working directory is read to
/// answer. The elision is drawn, so nobody reads it as the whole path, and the
/// tooltip carries what was elided.
fn tail(path: &str, segments: usize) -> String {
    let parts: Vec<&str> = path.trim_end_matches('/').split('/').collect();
    if parts.len() <= segments + 1 {
        return path.to_owned();
    }

    format!("…/{}", parts[parts.len() - segments..].join("/"))
}

/// A remembered invocation on one line: the program and its arguments, the way
/// it would read if it had been typed at a shell.
///
/// Joined with spaces and not quoted, because it is a label rather than
/// something to paste: what the form is refilled with is the invocation itself,
/// one argument per line, exactly as it was launched.
fn invocation(command: &AgentCommand) -> String {
    std::iter::once(command.command.trim())
        .chain(command.arguments())
        .collect::<Vec<_>>()
        .join(" ")
}

/// A remembered invocation as a row draws it: the program, then its arguments,
/// with the directories that led to the program on the tooltip.
fn running_label(command: &AgentCommand) -> String {
    std::iter::once(program(&command.command))
        .chain(command.arguments())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The rest of what makes it a different invocation: where it ran, and how many
/// variables it carried.
///
/// The environment is counted rather than shown. Its values are where a token
/// would be, and a list of invocations under a form is not the place to put one
/// on screen — the fields it refills are, once the user has asked for it.
///
/// Counted by core's reading of the field ([`AgentCommand::variables`]), so what
/// the row says is how many the agent would be given — a line that sets nothing
/// is not a variable here for the same reason it is not one at the spawn. A
/// blank working directory is likewise nothing to say: a remembered row says
/// what is *different* about that invocation.
fn under(command: &AgentCommand) -> Option<String> {
    let variables = command.variables().count();
    let mut said = Vec::new();
    if !command.cwd.trim().is_empty() {
        said.push(format!("in {}", tail(command.cwd.trim(), 2)));
    }
    if variables > 0 {
        said.push(match variables {
            1 => "1 variable".to_owned(),
            many => format!("{many} variables"),
        });
    }
    (!said.is_empty()).then(|| said.join(" · "))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The form, rendered — the screen itself rather than a value on the way to
    /// it, the way the Console's tests are, because what it draws is markup.
    fn shown() -> String {
        #[component]
        fn Host() -> Element {
            rsx! {
                SpawnForm {
                    running: false,
                    launching: false,
                    recent: Vec::new(),
                    unsaved: None,
                    on_launch: move |_: AgentCommand| {},
                }
            }
        }

        let mut dom = VirtualDom::new(Host);
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    fn running() -> AgentCommand {
        command("npx", "@zed-industries/claude-code-acp", "", "/tmp")
    }

    #[test]
    fn nothing_here_folds_and_nothing_here_says_what_is_running() {
        // Both were answers to being a permanent column of the window, and the
        // dialog is a better answer to the same measurement: one that is not
        // open takes no room at all. What is running belongs to the toolbar,
        // which says it whatever else is on screen — a form that repeated it
        // would be describing an agent nobody can see while it is shut.
        let html = shown();
        assert!(
            html.contains(r#"placeholder="npx @zed-industries/claude-code-acp""#),
            "the fields: {html}"
        );
        assert!(
            !html.contains("aria-expanded") && !html.contains("collapse"),
            "and nothing folds them: {html}"
        );
        assert!(
            !html.contains(r#"data-slot="running""#),
            "what is running is the toolbar's line: {html}"
        );
    }

    #[test]
    fn launch_fields_are_four_named_rows_of_the_dialog() {
        // Four rows, each named twice: the key the value goes out under and a
        // line saying what it is for. The grouping legends went with the
        // fieldsets — a dialog whose whole subject is one launch does not need
        // to tell the reader that the command and its arguments belong
        // together.
        let html = shown();

        for component in [
            r#"class="dialog-body" data-slot="spawn""#,
            r#"class="form-field""#,
            r#"class="input""#,
            r#"class="textarea""#,
            r#"class="btn btn-md btn-filled launch""#,
        ] {
            assert!(
                html.contains(component),
                "the launch workflow is drawn with {component}: {html}"
            );
        }
        assert_eq!(
            html.matches(r#"class="form-field""#).count(),
            4,
            "one row per field the transport takes: {html}"
        );
        for (key, note) in [
            ("cmd", "command to spawn"),
            ("args", "arguments, one per line"),
            ("env", "environment, KEY=value per line"),
            ("cwd", "session working directory"),
        ] {
            assert!(
                html.contains(&format!(r#"class="form-key">{key}<"#))
                    && html.contains(&format!(r#"class="form-note">{note}<"#)),
                "the {key} field is named by its key and by what it is for: {html}"
            );
        }
        assert!(
            !html.contains("card") && !html.contains("<fieldset"),
            "the fields are flat content of the dialog rather than a nested card: {html}"
        );
        for label in ["Command", "Arguments", "Environment", "Working directory"] {
            assert!(
                html.contains(&format!(r#"aria-label="{label}""#)),
                "the {label} control keeps its accessible name: {html}"
            );
        }
        assert!(
            html.contains(r#"autofocus=true"#),
            "and the platform puts the keyboard in Command each time the dialog is shown: {html}"
        );
    }

    #[test]
    fn recent_commands_are_full_width_selectable_rows_that_refill_before_launch() {
        #[component]
        fn Host() -> Element {
            rsx! {
                SpawnForm {
                    running: false,
                    launching: false,
                    recent: vec![running()],
                    unsaved: None,
                    on_launch: move |_: AgentCommand| {},
                }
            }
        }

        let html = dioxus_ssr::render_element(rsx! { Host {} });
        assert!(
            html.contains(r#"class="recents"#)
                && html.contains(r#"class="recent-row"#)
                && html.contains(r#"class="recent-label recall"#),
            "recent commands are full-width selectable rows: {html}"
        );
        assert!(
            html.contains("Put this back in the form: npx @zed-industries/claude-code-acp"),
            "selection remains a refill rather than a launch: {html}"
        );
        assert!(
            html.contains("npx @zed-industries/claude-code-acp"),
            "{html}"
        );
        assert!(
            html.contains(r#"data-slot="recall-confirmation""#)
                && html.contains(r#"role="status""#)
                && html.contains(r#"aria-live="polite""#),
            "Recent keeps a persistent polite confirmation region: {html}"
        );
        assert!(
            !html.contains("Invocation refilled:"),
            "nothing has been recalled yet: {html}"
        );
    }

    #[test]
    fn recall_refills_without_launching_and_confirmation_clears_on_edit_or_launch() {
        use std::{any::Any, rc::Rc};

        use dioxus::core::{ElementId, Mutation, NoOpMutations};
        use dioxus::html::{PlatformEventData, SerializedFormData, SerializedMouseData};

        #[component]
        fn Host() -> Element {
            let mut launches = use_signal(|| 0);
            rsx! {
                SpawnForm {
                    running: false,
                    launching: false,
                    recent: vec![command(
                        "bunx",
                        "--flag\nagent-package",
                        "TOKEN=remembered-secret",
                        "/remembered/path",
                    )],
                    unsaved: None,
                    on_launch: move |_: AgentCommand| launches += 1,
                }
                span { "data-slot": "launch-count", "{launches}" }
            }
        }

        set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let mouse_event = || {
            Event::new(
                Rc::new(PlatformEventData::new(Box::<SerializedMouseData>::default()))
                    as Rc<dyn Any>,
                true,
            )
        };
        let mut dom = VirtualDom::new(Host);
        let edits = dom.rebuild_to_vec().edits;
        let click_listeners = edits
            .iter()
            .filter_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if name == "click" => Some(*id),
                _ => None,
            })
            .collect::<Vec<ElementId>>();
        let input_listeners = edits
            .iter()
            .filter_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if name == "input" => Some(*id),
                _ => None,
            })
            .collect::<Vec<ElementId>>();
        assert_eq!(
            click_listeners.len(),
            3,
            "the remembered row, Cancel and Launch are the only click targets"
        );
        assert_eq!(input_listeners.len(), 4, "each Launch field accepts input");

        dom.runtime()
            .handle_event("click", mouse_event(), click_listeners[0]);
        dom.render_immediate(&mut NoOpMutations);

        let html = dioxus_ssr::render(&dom);
        for refilled in [
            r#"value="bunx""#,
            r#"value="--flag
agent-package""#,
            r#"value="TOKEN=remembered-secret""#,
            r#"value="/remembered/path""#,
        ] {
            assert!(html.contains(refilled), "{refilled} is refilled: {html}");
        }
        assert!(
            html.contains(r#"data-slot="launch-count">0"#),
            "recall never invokes Launch: {html}"
        );
        assert!(
            html.contains(
                "Invocation refilled: bunx --flag agent-package. Review the fields, then Launch."
            ),
            "recall confirms what changed and the next step: {html}"
        );

        for (field, input) in ["Command", "Arguments", "Working directory", "Environment"]
            .into_iter()
            .zip(input_listeners)
        {
            let input_event = Event::new(
                Rc::new(PlatformEventData::new(Box::new(SerializedFormData::new(
                    format!("edited {field}"),
                    Vec::new(),
                )))) as Rc<dyn Any>,
                true,
            );
            dom.runtime().handle_event("input", input_event, input);
            dom.render_immediate(&mut NoOpMutations);
            let html = dioxus_ssr::render(&dom);
            assert!(
                !html.contains("Invocation refilled:"),
                "editing {field} clears the recall confirmation: {html}"
            );

            dom.runtime()
                .handle_event("click", mouse_event(), click_listeners[0]);
            dom.render_immediate(&mut NoOpMutations);
        }

        dom.runtime()
            .handle_event("click", mouse_event(), click_listeners[2]);
        dom.render_immediate(&mut NoOpMutations);
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains(r#"data-slot="launch-count">1"#),
            "Launch is still a separate action after recall: {html}"
        );
        assert!(
            !html.contains("Invocation refilled:"),
            "Launch clears the recall confirmation: {html}"
        );
    }

    #[test]
    fn an_unsaved_recent_invocation_is_restrained_readable_status() {
        #[component]
        fn Host() -> Element {
            rsx! {
                SpawnForm {
                    running: false,
                    launching: false,
                    recent: Vec::new(),
                    unsaved: Some("permission denied".to_owned()),
                    on_launch: move |_: AgentCommand| {},
                }
            }
        }

        let html = dioxus_ssr::render_element(rsx! { Host {} });
        assert!(
            html.contains(r#"class="alert alert-warning" data-slot="unsaved" role="status"#)
                && html.contains("Launched, but not remembered for next time: permission denied."),
            "the persistence warning remains explicit: {html}"
        );
        assert!(
            !html.contains("badge"),
            "the warning does not become a badge: {html}"
        );
    }

    #[test]
    fn a_launch_in_progress_says_it_is_launching() {
        #[component]
        fn Host() -> Element {
            rsx! {
                SpawnForm {
                    running: false,
                    launching: true,
                    recent: Vec::new(),
                    unsaved: None,
                    on_launch: move |_: AgentCommand| {},
                }
            }
        }

        let html = dioxus_ssr::render_element(rsx! { Host {} });
        assert!(
            html.contains("Launching"),
            "loading is explicit in words: {html}"
        );
        assert!(
            html.contains(r#"aria-disabled="true""#)
                && html.contains(r#"tabindex="-1""#)
                && !html.contains("disabled=true"),
            "a launch cannot be repeated without replacing the focused button: {html}"
        );
    }

    #[test]
    fn a_remembered_row_says_only_what_is_different_about_that_invocation() {
        // A blank working directory is nothing to say about a remembered
        // invocation: it ran wherever this window runs, which is true of every
        // row and so distinguishes none of them.
        assert_eq!(under(&command("npx", "", "", "")), None);

        // What is said is said as a place rather than as a bare path, and
        // elided from the front because the end of a directory is what tells
        // two of them apart.
        let somewhere = under(&command("npx", "", "", "/home/laborant/phoenix/inspector"))
            .expect("a directory that was typed is a difference worth naming");
        assert_eq!(somewhere, "in …/phoenix/inspector");

        // And the environment is counted rather than shown, because its values
        // are where a token would be.
        let carrying = under(&command("npx", "", "TOKEN=secret\nACP_LOG=debug", ""))
            .expect("variables are a difference too");
        assert_eq!(carrying, "2 variables");
        assert!(!carrying.contains("secret"));
    }

    fn command(command: &str, args: &str, env: &str, cwd: &str) -> AgentCommand {
        AgentCommand {
            command: command.to_owned(),
            args: args.to_owned(),
            env: env.to_owned(),
            cwd: cwd.to_owned(),
        }
    }

    #[test]
    fn a_remembered_invocation_reads_as_the_command_it_is() {
        assert_eq!(
            invocation(&command("npx", "@zed-industries/claude-code-acp", "", "")),
            "npx @zed-industries/claude-code-acp"
        );
        assert_eq!(invocation(&command("testy", "", "", "")), "testy");
    }

    #[test]
    fn what_is_under_it_says_where_and_how_many_never_what() {
        assert_eq!(under(&command("npx", "", "", "")), None);
        // Said as a place, and elided from the front where a path is long
        // enough to need it: the end of a directory is what tells two of them
        // apart, and it is the end a row runs out of room for.
        assert_eq!(
            under(&command("npx", "", "", "/tmp")),
            Some("in /tmp".to_owned())
        );
        assert_eq!(
            under(&command(
                "npx",
                "",
                "",
                "/home/somebody/work/phoenix/inspector"
            )),
            Some("in …/phoenix/inspector".to_owned())
        );
        // The values stay off the row: `ANTHROPIC_API_KEY=sk-...` is exactly the
        // shape of the second line of somebody's spawn form.
        let secrets = command(
            "npx",
            "",
            "ACP_LOG=debug\nANTHROPIC_API_KEY=sk-secret",
            "/tmp",
        );
        let said = under(&secrets).expect("a line under it");
        assert_eq!(said, "in /tmp · 2 variables");
        assert!(!said.contains("sk-secret"), "{said}");

        // And it counts what the agent would be given, not lines: a half-typed
        // variable sets nothing at the spawn and is nothing here (`command.rs`).
        assert_eq!(
            under(&command("npx", "", "ACP_LOG=debug\nHALF_TYPED", "")),
            Some("1 variable".to_owned())
        );
    }
}
