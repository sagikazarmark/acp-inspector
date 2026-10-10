//! The spawn form (`docs/architecture.md` §9): four fields, a button that hands
//! them to core, and the few invocations that worked before.
//!
//! The fields are text and they leave as text — [`AgentCommand`] is where a
//! line becomes an argument, so the rules live somewhere they can be tested and
//! somewhere a second surface would inherit them from.
//!
//! **One form, on the registry's field components.** Each field has a visible
//! label that is also its accessible name, a description it is described by,
//! and an error region that speaks once the reader has left the field or tried
//! to launch. The fields sit in a `<form>`, so Enter in a single-line field and
//! ⌘/Ctrl+Enter in a multi-line one launch, the way any dialog form does.
//!
//! **The recent list is the convenience the spec allows and stops at** (§1.1):
//! a row's label refills the fields, and its play button launches it directly.
//! Nothing here decides what is remembered or where it is kept — that is
//! [`RecentCommands`](acp_inspector_core::RecentCommands).
//!
//! **A failed launch is said here** (X1): the dialog stays open on the fields
//! that produced it, with the reason in an alert above them, rather than
//! closing onto a window that changed tabs behind its backdrop.

use std::rc::Rc;

use acp_inspector_core::{AgentCommand, CallError, ConnectionStatus, Diagnostic, DiagnosticKind};
use dioxus::prelude::*;
use dioxus_field::{Binding, FieldContext, FieldMetaValues};
use dioxus_free_icons::{Icon, icons::ld_icons::LdPlay};
use dioxus_primitives::dioxus_attributes::attributes;

use crate::components::{
    field::{
        Field, FieldAppearance, FieldDescription, FieldDescriptionAppearance, FieldError,
        FieldLabel,
    },
    input::Input,
    textarea::Textarea,
};

/// The Command field's DOM id, which a failed launch hands focus back to.
pub const COMMAND: &str = "launch-command";

/// Why the last launch did not get as far as an agent, as the dialog says it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchFailure {
    /// One sentence, the reason first.
    pub headline: String,
    /// Supporting lines: the command as run, how it exited, what it last said.
    pub detail: Vec<String>,
    /// The `PATH` a bare command name was looked up in, where the failure was
    /// that it was not found.
    pub path: Option<String>,
}

/// What the dialog should say about the last launch, if anything.
///
/// `lines` are the diagnostics recorded since that launch began, oldest first.
/// `path` is the search path the spawn used. An agent that described itself is
/// not a failed launch: what went wrong after that is the session's, and is
/// said on the timeline.
pub fn launch_failure(
    status: ConnectionStatus,
    problem: Option<&CallError>,
    described: bool,
    lines: &[Diagnostic],
    path: Option<&str>,
) -> Option<LaunchFailure> {
    if described {
        return None;
    }
    let spawn = lines.iter().rev().find_map(|line| match &line.kind {
        DiagnosticKind::SpawnFailed { command, error } => Some((command, error)),
        _ => None,
    });
    if let Some((command, error)) = spawn {
        let bare = command
            .split_whitespace()
            .next()
            .is_some_and(|program| !program.contains('/'));
        return Some(LaunchFailure {
            headline: format!("Couldn't start the agent: {error}"),
            detail: vec![format!("Command: {command}")],
            path: (error.kind() == std::io::ErrorKind::NotFound && bare)
                .then(|| path.map(str::to_owned))
                .flatten(),
        });
    }
    if status == ConnectionStatus::FailedToStart {
        return Some(LaunchFailure {
            headline: match problem {
                Some(problem) if *problem != CallError::Disconnected => {
                    format!("Couldn't start the agent: {problem}")
                }
                _ => "Couldn't start the agent.".to_owned(),
            },
            detail: Vec::new(),
            path: None,
        });
    }
    let problem = problem?;
    let mut detail = Vec::new();
    let headline = match problem {
        CallError::Disconnected => {
            if let Some(status) = lines.iter().rev().find_map(|line| match &line.kind {
                DiagnosticKind::AgentExited(status) => Some(status),
                _ => None,
            }) {
                detail.push(format!("It exited with {status}."));
            }
            if let Some(said) = lines.iter().rev().find_map(|line| match &line.kind {
                DiagnosticKind::Stderr(text) if !text.trim().is_empty() => Some(text.trim()),
                _ => None,
            }) {
                detail.push(format!("Last output: {said}"));
            }
            "The agent stopped before it answered.".to_owned()
        }
        problem => format!("Couldn't start the agent: {problem}"),
    };
    Some(LaunchFailure {
        headline,
        detail,
        path: None,
    })
}

/// A search path as a reader scans it: one entry per segment, with the home
/// directory written `~`.
pub fn abbreviate_path(path: &str, home: Option<&str>) -> String {
    path.split(':')
        .map(|entry| match home {
            Some(home) if !home.is_empty() && entry.starts_with(home) => {
                format!("~{}", &entry[home.len()..])
            }
            _ => entry.to_owned(),
        })
        .collect::<Vec<_>>()
        .join(":")
}

/// A whole command line typed into Command, split the way a shell would read
/// it: leading `NAME=value` words are environment, the next word is the
/// program, and everything after it is arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandLine {
    pub env: Vec<String>,
    pub program: String,
    pub args: Vec<String>,
}

/// Whether `command` reads as a command line rather than a program, and how
/// it would split if so.
///
/// Whitespace in a command is either a command line or a path with a space in
/// it. A path is read as one when it starts like a path and its last word still
/// has a `/` in it — `/Applications/My Agent.app/Contents/MacOS/agent` — and is
/// left alone; anything else with whitespace is offered a split, never split
/// without asking.
pub fn command_line(command: &str) -> Option<CommandLine> {
    let command = command.trim();
    if !command.contains(char::is_whitespace) {
        return None;
    }
    let path_like = command.starts_with(['/', '~', '.'])
        && command
            .rsplit(char::is_whitespace)
            .next()
            .is_some_and(|last| last.contains('/'));
    if path_like {
        return None;
    }
    let mut words = words(command).into_iter().peekable();
    let mut env = Vec::new();
    while let Some(word) = words.peek() {
        let assignment = word.split_once('=').is_some_and(|(name, _)| {
            !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        });
        if !assignment {
            break;
        }
        env.extend(words.next());
    }
    let program = words.next()?;
    Some(CommandLine {
        env,
        program,
        args: words.collect(),
    })
}

/// Shell-style words: whitespace separates, single and double quotes group,
/// a backslash escapes the next character outside single quotes.
fn words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quote = None::<char>;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(open), c) if c == open => quote = None,
            (Some('"') | None, '\\') => {
                if let Some(next) = chars.next() {
                    word.push(next);
                }
                started = true;
            }
            (Some(_), c) => word.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            (None, c) => {
                word.push(c);
                started = true;
            }
        }
    }
    if started {
        words.push(word);
    }
    words
}

/// The 1-based numbers of environment lines that set nothing because they
/// have no `=` — the lines core drops (`AgentCommand::variables`).
pub fn unset_lines(env: &str) -> Vec<usize> {
    env.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty() && !line.contains('='))
        .map(|(index, _)| index + 1)
        .collect()
}

fn unset_warning(lines: &[usize]) -> String {
    match lines {
        [one] => format!(
            "Line {one} has no \u{201c}=\u{201d}, so it sets nothing. Write it as NAME=value."
        ),
        many => {
            let (last, rest) = many.split_last().expect("not empty");
            let rest = rest
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Lines {rest} and {last} have no \u{201c}=\u{201d}, so they set nothing. Write each as NAME=value."
            )
        }
    }
}

/// Field metadata for one of the four launch fields.
fn meta(id: &str, error: Option<&str>) -> FieldMetaValues {
    FieldMetaValues {
        id: Some(Rc::from(id)),
        errors: error.into_iter().map(Rc::from).collect(),
        ..FieldMetaValues::default()
    }
}

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
    /// Why the last launch did not reach an agent, if it did not.
    #[props(default)]
    failure: Option<LaunchFailure>,
    /// The invocation the window last launched. The fields follow it, so a
    /// dialog opened over a running agent shows what is running.
    #[props(default)]
    launched: Option<AgentCommand>,
    on_launch: EventHandler<AgentCommand>,
) -> Element {
    let mut command = use_signal(String::new);
    let mut args = use_signal(String::new);
    let mut env = use_signal(String::new);
    let mut cwd = use_signal(String::new);
    let mut recall_confirmation = use_signal(|| None::<String>);
    let mut clear_recall_confirmation = move || recall_confirmation.set(None);
    // Whether the reader tried to launch, or left Command: either is when an
    // empty Command becomes something worth saying.
    let mut attempted = use_signal(|| false);
    let mut left_command = use_signal(|| false);

    use_effect(use_reactive!(|launched| {
        if let Some(entry) = launched {
            command.set(entry.command.clone());
            args.set(entry.args.clone());
            env.set(entry.env.clone());
            cwd.set(entry.cwd.clone());
        }
    }));

    let agent = use_memo(move || AgentCommand {
        command: command(),
        args: args(),
        env: env(),
        cwd: cwd(),
        ..AgentCommand::default()
    });

    // Selecting a remembered invocation's label is a *refill*: the fields are
    // where a run is decided. Its play button is the direct way, beside it.
    let mut refill = move |entry: &AgentCommand| {
        let invocation = invocation(entry);
        command.set(entry.command.clone());
        args.set(entry.args.clone());
        env.set(entry.env.clone());
        cwd.set(entry.cwd.clone());
        recall_confirmation.set(Some(format!(
            "Filled in {invocation}. Press Launch to start it."
        )));
    };

    let runnable = agent().is_runnable();
    let command_error =
        (!runnable && (attempted() || left_command())).then_some("Enter a command to launch.");
    let split = command_line(&command());
    let unset = unset_lines(&env());
    let mcp_valid = crate::mcp::valid();
    let reason = if launching {
        Some("Starting the agent\u{2026}")
    } else if !runnable {
        Some("Enter a command to launch.")
    } else if !mcp_valid {
        Some("Fix the MCP server fields first.")
    } else {
        None
    };

    let mut submit = move || {
        if launching {
            return;
        }
        attempted.set(true);
        let mcp_ok = crate::mcp::attempt_launch();
        if !agent.peek().is_runnable() {
            focus(COMMAND);
            return;
        }
        if !mcp_ok {
            return;
        }
        clear_recall_confirmation();
        on_launch.call(agent());
    };

    rsx! {
        form {
            class: "contents",
            "data-slot": "spawn-form",
            novalidate: true,
            onsubmit: move |event| {
                event.prevent_default();
                submit();
            },
            // Enter in a textarea is a new line; ⌘/Ctrl+Enter is the launch.
            onkeydown: move |event| {
                let modifiers = event.modifiers();
                if event.key() == Key::Enter && (modifiers.meta() || modifiers.ctrl()) {
                    event.prevent_default();
                    submit();
                }
            },
            div { class: "dialog-body", "data-slot": "spawn",
                if let Some(failure) = &failure {
                    div {
                        id: "launch-failure",
                        class: "alert alert-error launch-failure",
                        "data-slot": "launch-failure",
                        role: "alert",
                        tabindex: "-1",
                        div { class: "launch-failure-body",
                            p { class: "launch-failure-headline", "{failure.headline}" }
                            for line in failure.detail.iter() {
                                p { class: "hint mono", "{line}" }
                            }
                            if let Some(path) = &failure.path {
                                p { class: "hint", "data-slot": "launch-path",
                                    "Searched PATH: "
                                    code { class: "mono", title: "{path}",
                                        {abbreviate_path(path, std::env::var("HOME").ok().as_deref())}
                                    }
                                }
                            }
                            p { class: "hint", "Everything the agent wrote is in the Diagnostics tab." }
                        }
                    }
                }

                Field {
                    context: FieldContext::new(Binding::<String>::from(command)).with_meta_values(meta(COMMAND, command_error)),
                    appearance: FieldAppearance::None,
                    class: "form-field",
                    FieldLabel { "Command" }
                    Input {
                        class: "mono",
                        on_change: move |_| clear_recall_confirmation(),
                        on_focus_exit: move |()| left_command.set(true),
                        // String keys: `autofocus` and `spellcheck` are on both
                        // attribute sets the component extends (U9, #21).
                        // **The platform focuses it, once per opening.**
                        attributes: attributes!(input {
                            "autofocus": true,
                            "spellcheck": "false",
                            autocomplete: "off",
                            placeholder: "npx",
                        }),
                    }
                    FieldDescription { appearance: FieldDescriptionAppearance::None, class: "hint",
                        "Executable name or path, without arguments or shell quotes."
                    }
                    if let Some(split) = split.clone() {
                        FieldDescription {
                            appearance: FieldDescriptionAppearance::None,
                            class: "hint split-offer",
                            "data-slot": "split-offer",
                            "This looks like a whole command line. "
                            button {
                                class: "btn btn-xs btn-quiet",
                                "data-slot": "split-command",
                                r#type: "button",
                                onclick: move |_| {
                                    let mut lines = split.args.clone();
                                    let kept = args.peek().trim().to_owned();
                                    if !kept.is_empty() {
                                        lines.push(kept);
                                    }
                                    args.set(lines.join("\n"));
                                    if !split.env.is_empty() {
                                        let mut vars = split.env.clone();
                                        let kept = env.peek().trim().to_owned();
                                        if !kept.is_empty() {
                                            vars.push(kept);
                                        }
                                        env.set(vars.join("\n"));
                                    }
                                    command.set(split.program.clone());
                                    clear_recall_confirmation();
                                    focus(COMMAND);
                                },
                                "Split into command + arguments"
                            }
                        }
                    }
                    FieldError {}
                }
                Field {
                    context: FieldContext::new(Binding::<String>::from(args)).with_meta_values(meta("launch-args", None)),
                    appearance: FieldAppearance::None,
                    class: "form-field",
                    FieldLabel { "Arguments" }
                    Textarea {
                        class: "mono",
                        on_change: move |_| clear_recall_confirmation(),
                        attributes: attributes!(textarea {
                            rows: 3,
                            "spellcheck": "false",
                            placeholder: "@zed-industries/claude-code-acp",
                        }),
                    }
                    FieldDescription { appearance: FieldDescriptionAppearance::None, class: "hint",
                        "One per line. Put a flag and its value on separate lines. No shell quoting or expansion."
                    }
                }
                Field {
                    context: FieldContext::new(Binding::<String>::from(env)).with_meta_values(meta("launch-env", None)),
                    appearance: FieldAppearance::None,
                    class: "form-field",
                    FieldLabel { "Environment" }
                    Textarea {
                        class: "mono",
                        on_change: move |_| clear_recall_confirmation(),
                        attributes: attributes!(textarea {
                            rows: 2,
                            "spellcheck": "false",
                            placeholder: "ACP_LOG=debug",
                        }),
                    }
                    FieldDescription { appearance: FieldDescriptionAppearance::None, class: "hint",
                        "One NAME=value per line, added to the inspector's own environment."
                    }
                    if !unset.is_empty() {
                        FieldDescription {
                            appearance: FieldDescriptionAppearance::None,
                            class: "hint field-warning",
                            "data-slot": "env-warning",
                            {unset_warning(&unset)}
                        }
                    }
                }
                Field {
                    context: FieldContext::new(Binding::<String>::from(cwd)).with_meta_values(meta("launch-cwd", None)),
                    appearance: FieldAppearance::None,
                    class: "form-field",
                    FieldLabel { "Working directory" }
                    Input {
                        class: "mono",
                        on_change: move |_| clear_recall_confirmation(),
                        attributes: attributes!(input {
                            "spellcheck": "false",
                            autocomplete: "off",
                            placeholder: "the inspector's own",
                        }),
                    }
                    FieldDescription { appearance: FieldDescriptionAppearance::None, class: "hint",
                        "Empty uses the inspector's own; relative paths resolve against it. The agent starts here, and the session gets it as an absolute path."
                    }
                }

                crate::mcp::McpEditor { prefix: "launch" }

                if !recent.is_empty() {
                    div { class: "form-field recent-field",
                        div { class: "form-label",
                            span { class: "eyebrow", id: "recent-label", "Recent" }
                        }
                        div { class: "recents", role: "list", aria_labelledby: "recent-label",
                            for (nth , entry) in recent.iter().enumerate() {
                                // By position, because the list *is* an order: the
                                // same invocation relaunched moves to the front.
                                div { class: "recent-row", key: "{nth}", role: "listitem",
                                    button {
                                        class: "recent-label recall",
                                        r#type: "button",
                                        title: "Fill in the form with {invocation(entry)}",
                                        onclick: {
                                            let entry = entry.clone();
                                            move |_| refill(&entry)
                                        },
                                        {running_label(entry)}
                                    }
                                    if let Some(under) = under(entry) {
                                        span { class: "recent-when", title: "{under}", "{under}" }
                                    }
                                    button {
                                        class: "btn btn-xs btn-quiet recent-launch",
                                        "data-slot": "recent-launch",
                                        r#type: "button",
                                        aria_label: "Launch {invocation(entry)}",
                                        title: "Launch {invocation(entry)}",
                                        aria_disabled: launching.then_some("true"),
                                        onclick: {
                                            let entry = entry.clone();
                                            move |_| {
                                                if !launching && crate::mcp::attempt_launch() {
                                                    clear_recall_confirmation();
                                                    on_launch.call(entry.clone());
                                                }
                                            }
                                        },
                                        span { class: "disclosure-icon", aria_hidden: "true",
                                            Icon { class: "icon", width: 16, height: 16, icon: LdPlay }
                                        }
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
                p { id: "launch-reason", class: "hint",
                    match reason {
                        Some(reason) => rsx! { "{reason}" },
                        None if running => rsx! { "Stops the running agent and starts this one. The trace is kept." },
                        None => rsx! { "Starts the agent and opens a session." },
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
                    "data-slot": "launch",
                    r#type: "submit",
                    aria_describedby: "launch-reason",
                    // Launching: the handshake is in flight and a second one
                    // would race it. Otherwise the button always answers, with
                    // the reason in the line beside it when it cannot launch.
                    aria_disabled: launching.then_some("true"),
                    tabindex: launching.then_some("-1"),
                    if launching { "Launching" } else if running { "Restart with these settings" } else { "Launch" }
                }
            }
        }
    }
}

/// Moves focus to a control by id, once it is painted.
fn focus(id: &'static str) {
    document::eval(&format!(
        "requestAnimationFrame(() => document.getElementById({id:?})?.focus());"
    ));
}

/// The program a path names, without the directories that led to it.
///
/// **The end of a command path is the identifying part of it**, and the end is
/// what a line that runs out of room loses. The whole of it is on the row's
/// tooltip, which is where a label keeps what it does not show.
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
/// answer.
fn tail(path: &str, segments: usize) -> String {
    let parts: Vec<&str> = path.trim_end_matches('/').split('/').collect();
    if parts.len() <= segments + 1 {
        return path.to_owned();
    }

    format!("…/{}", parts[parts.len() - segments..].join("/"))
}

/// A remembered invocation on one line: the program and its arguments, the way
/// it would read if it had been typed at a shell.
pub(crate) fn invocation(command: &AgentCommand) -> String {
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
/// on screen. Counted by core's reading of the field
/// ([`AgentCommand::variables`]), so what the row says is how many the agent
/// would be given.
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
    use std::{any::Any, rc::Rc, sync::Arc, time::SystemTime};

    use dioxus::core::{ElementId, Mutation, NoOpMutations};
    use dioxus::html::{PlatformEventData, SerializedFormData, SerializedMouseData};

    use super::*;

    /// The form, rendered — the screen itself rather than a value on the way to
    /// it, because what it draws is markup.
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
        settle(&mut dom);
        dioxus_ssr::render(&dom)
    }

    /// Lets effects run and what they set be drawn.
    fn settle(dom: &mut VirtualDom) {
        for _ in 0..3 {
            dom.render_immediate(&mut NoOpMutations);
        }
    }

    fn running() -> AgentCommand {
        command("npx", "@zed-industries/claude-code-acp", "", "/tmp")
    }

    fn mouse_event() -> Event<dyn Any> {
        Event::new(
            Rc::new(PlatformEventData::new(Box::<SerializedMouseData>::default())) as Rc<dyn Any>,
            true,
        )
    }

    fn form_event(value: &str) -> Event<dyn Any> {
        Event::new(
            Rc::new(PlatformEventData::new(Box::new(SerializedFormData::new(
                value.to_owned(),
                Vec::new(),
            )))) as Rc<dyn Any>,
            true,
        )
    }

    fn target(edits: &[Mutation], attribute: &str, expected: &str) -> ElementId {
        edits
            .iter()
            .rev()
            .find_map(|edit| match edit {
                Mutation::SetAttribute {
                    name,
                    value: dioxus::core::AttributeValue::Text(value),
                    id,
                    ..
                } if *name == attribute && *value == expected => Some(*id),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing {attribute}={expected}"))
    }

    fn listeners(edits: &[Mutation], event: &str) -> Vec<ElementId> {
        edits
            .iter()
            .filter_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if name == event => Some(*id),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn launch_fields_are_four_labelled_fields_of_one_form() {
        let html = shown();

        assert!(
            html.contains(r#"data-slot="spawn-form""#) && html.contains("<form"),
            "the fields are a form, so Enter launches: {html}"
        );
        assert_eq!(
            html.matches(r#"class="form-field""#).count(),
            4,
            "one field per value the transport takes: {html}"
        );
        for (id, label) in [
            ("launch-command", "Command"),
            ("launch-args", "Arguments"),
            ("launch-env", "Environment"),
            ("launch-cwd", "Working directory"),
        ] {
            assert!(
                html.contains(&format!(r#"for="{id}" class="label">{label}</label>"#))
                    && html.contains(&format!(r#"id="{id}""#)),
                "the {label} control is named by its visible label: {html}"
            );
        }
        assert!(
            !html.contains(r#"aria-label="Command""#),
            "the visible label is the accessible name, not a second one: {html}"
        );
        assert!(
            html.contains(r#"autofocus=true"#),
            "the platform puts the keyboard in Command each time the dialog is shown: {html}"
        );
        assert!(
            html.contains(r#"type="submit""#) && html.contains(">Launch</button>"),
            "Launch submits the form: {html}"
        );
        assert!(
            html.contains("Starts the agent and opens a session.")
                || html.contains("Enter a command to launch."),
            "the foot says what Launch does, or why it cannot: {html}"
        );
        assert!(
            !html.contains("aria-invalid=\"true\"")
                && !html.contains("Enter a command to launch.</div>"),
            "nothing is wrong before the reader has done anything: {html}"
        );
    }

    #[test]
    fn the_mcp_editor_is_below_the_working_directory() {
        #[derive(Clone, Props)]
        struct Props {
            inspector: acp_inspector_core::Inspector,
        }
        impl PartialEq for Props {
            fn eq(&self, _: &Self) -> bool {
                true
            }
        }
        fn host(props: Props) -> Element {
            crate::mcp::provide(props.inspector);
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
        let mut dom = VirtualDom::new_with_props(
            host,
            Props {
                inspector: acp_inspector_core::Inspector::new(),
            },
        );
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);
        let cwd = html.find(r#"id="launch-cwd""#).expect("cwd");
        let mcp = html.find(r#"data-slot="mcp-editor""#).expect("mcp");
        assert!(cwd < mcp, "the MCP disclosure follows the fields: {html}");
        assert!(html.contains("MCP servers (0)"), "{html}");
    }

    #[test]
    fn launching_with_no_command_says_so_on_the_field_and_does_not_launch() {
        #[component]
        fn Host() -> Element {
            let mut launches = use_signal(|| 0);
            rsx! {
                SpawnForm {
                    running: false,
                    launching: false,
                    recent: Vec::new(),
                    unsaved: None,
                    on_launch: move |_: AgentCommand| launches += 1,
                }
                span { "data-slot": "launch-count", "{launches}" }
            }
        }
        set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let mut dom = VirtualDom::new(Host);
        let edits = dom.rebuild_to_vec().edits;
        let submit = listeners(&edits, "submit")[0];
        dom.runtime().handle_event("submit", form_event(""), submit);
        settle(&mut dom);
        dom.render_immediate(&mut NoOpMutations);
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains(r#"data-slot="launch-count">0"#),
            "nothing launches: {html}"
        );
        assert!(
            html.contains("Enter a command to launch.") && html.contains(r#"aria-invalid="true""#),
            "the reason is on the field and the field is marked: {html}"
        );
    }

    #[test]
    fn a_failed_launch_is_an_alert_inside_the_dialog() {
        #[component]
        fn Host(failure: LaunchFailure) -> Element {
            rsx! {
                SpawnForm {
                    running: false,
                    launching: false,
                    recent: Vec::new(),
                    unsaved: None,
                    failure,
                    on_launch: move |_: AgentCommand| {},
                }
            }
        }
        let failure = LaunchFailure {
            headline: "Couldn't start the agent: No such file or directory (os error 2)".to_owned(),
            detail: vec!["Command: claude-code-acp".to_owned()],
            path: Some("/usr/bin:/bin".to_owned()),
        };
        let mut dom = VirtualDom::new_with_props(Host, HostProps { failure });
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains(r#"id="launch-failure""#)
                && html.contains(r#"role="alert""#)
                && html.contains(r#"tabindex="-1""#),
            "the alert can be announced and focused: {html}"
        );
        assert!(
            html.contains("Couldn&#39;t start the agent: No such file or directory")
                && html.contains("Command: claude-code-acp")
                && html.contains("Searched PATH:")
                && html.contains("/usr/bin:/bin"),
            "{html}"
        );
    }

    #[test]
    fn a_spawn_that_failed_is_worded_as_one_with_the_path_it_searched() {
        let lines = vec![Diagnostic {
            at: SystemTime::UNIX_EPOCH,
            kind: DiagnosticKind::SpawnFailed {
                command: "claude-code-acp --stdio".to_owned(),
                error: Arc::new(std::io::Error::from(std::io::ErrorKind::NotFound)),
            },
        }];
        let failure = launch_failure(
            ConnectionStatus::FailedToStart,
            Some(&CallError::Disconnected),
            false,
            &lines,
            Some("/usr/bin:/bin"),
        )
        .expect("a failure");
        assert!(
            failure.headline.starts_with("Couldn't start the agent: "),
            "{failure:?}"
        );
        assert!(!failure.headline.contains("connection to the agent is gone"));
        assert_eq!(failure.path.as_deref(), Some("/usr/bin:/bin"));
        assert_eq!(failure.detail, ["Command: claude-code-acp --stdio"]);

        // A path that was not found was not looked up in PATH.
        let lines = vec![Diagnostic {
            at: SystemTime::UNIX_EPOCH,
            kind: DiagnosticKind::SpawnFailed {
                command: "/opt/agent".to_owned(),
                error: Arc::new(std::io::Error::from(std::io::ErrorKind::NotFound)),
            },
        }];
        let failure = launch_failure(
            ConnectionStatus::FailedToStart,
            None,
            false,
            &lines,
            Some("/bin"),
        )
        .unwrap();
        assert_eq!(failure.path, None);

        // An agent that described itself has not failed to launch.
        assert_eq!(
            launch_failure(
                ConnectionStatus::Connected,
                Some(&CallError::NoSession),
                true,
                &[],
                None
            ),
            None
        );
        // Nor has a window that never launched.
        assert_eq!(
            launch_failure(ConnectionStatus::Disconnected, None, false, &[], None),
            None
        );
    }

    #[test]
    fn an_agent_that_exits_before_answering_says_how_and_what_it_said_last() {
        let lines = vec![
            Diagnostic {
                at: SystemTime::UNIX_EPOCH,
                kind: DiagnosticKind::Stderr("error: missing API key".to_owned()),
            },
            Diagnostic {
                at: SystemTime::UNIX_EPOCH,
                kind: DiagnosticKind::Stderr("  ".to_owned()),
            },
        ];
        let failure = launch_failure(
            ConnectionStatus::Lost,
            Some(&CallError::Disconnected),
            false,
            &lines,
            None,
        )
        .unwrap();
        assert_eq!(failure.headline, "The agent stopped before it answered.");
        assert_eq!(failure.detail, ["Last output: error: missing API key"]);
    }

    #[test]
    fn a_whole_command_line_in_command_is_offered_a_split() {
        assert_eq!(command_line("npx"), None);
        assert_eq!(
            command_line("/Applications/My Agent.app/Contents/MacOS/agent"),
            None,
            "a path with a space in it is a path"
        );
        assert_eq!(
            command_line("ACP_LOG=debug npx -y '@zed/claude code' --flag=\"a b\""),
            Some(CommandLine {
                env: vec!["ACP_LOG=debug".to_owned()],
                program: "npx".to_owned(),
                args: vec![
                    "-y".to_owned(),
                    "@zed/claude code".to_owned(),
                    "--flag=a b".to_owned()
                ],
            })
        );
        assert_eq!(
            command_line("/usr/local/bin/agent --stdio").map(|line| (line.program, line.args)),
            Some((
                "/usr/local/bin/agent".to_owned(),
                vec!["--stdio".to_owned()]
            ))
        );
        assert_eq!(words(r#"a\ b "c\"d" ''"#), ["a b", "c\"d", ""]);
    }

    #[test]
    fn the_split_offer_is_drawn_and_moves_words_into_their_fields() {
        #[component]
        fn Host() -> Element {
            rsx! {
                SpawnForm {
                    running: false,
                    launching: false,
                    recent: Vec::new(),
                    unsaved: None,
                    launched: command("FOO=1 npx -y agent", "--verbose", "", ""),
                    on_launch: move |_: AgentCommand| {},
                }
            }
        }
        set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let mut dom = VirtualDom::new(Host);
        let mut edits = dom.rebuild_to_vec().edits;
        for _ in 0..3 {
            edits.extend(dom.render_immediate_to_vec().edits);
        }
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains(r#"data-slot="split-offer""#)
                && html.contains("Split into command + arguments"),
            "{html}"
        );
        // Drive the split through whichever rendered button performs it.
        let clicks = listeners(&edits, "click");
        for id in clicks {
            dom.runtime().handle_event("click", mouse_event(), id);
            dom.render_immediate(&mut NoOpMutations);
            let html = dioxus_ssr::render(&dom);
            if html.contains(r#"value="npx""#) {
                assert!(
                    html.contains("-y\nagent\n--verbose") && html.contains(r#"value="FOO=1""#),
                    "the words went to their fields: {html}"
                );
                return;
            }
        }
        panic!("no control split the command line");
    }

    #[test]
    fn env_lines_without_an_equals_sign_are_warned_about() {
        assert_eq!(unset_lines("A=1\n\nHALF\n B=2 \nTOKEN"), [3, 5]);
        assert_eq!(
            unset_warning(&[3]),
            "Line 3 has no \u{201c}=\u{201d}, so it sets nothing. Write it as NAME=value."
        );
        assert!(unset_warning(&[3, 5, 7]).starts_with("Lines 3, 5 and 7 have no"));

        #[component]
        fn Host() -> Element {
            rsx! {
                SpawnForm {
                    running: false,
                    launching: false,
                    recent: Vec::new(),
                    unsaved: None,
                    launched: command("npx", "", "ACP_LOG=debug\nHALF_TYPED", ""),
                    on_launch: move |_: AgentCommand| {},
                }
            }
        }
        let mut dom = VirtualDom::new(Host);
        dom.rebuild_in_place();
        settle(&mut dom);
        let html = dioxus_ssr::render(&dom);
        let warning_id = html
            .split(r#"data-slot="env-warning""#)
            .next()
            .and_then(|before| before.rsplit("id=\"").next())
            .and_then(|id| id.split('"').next())
            .expect("the warning has an id")
            .to_owned();
        assert!(
            html.contains("Line 2 has no"),
            "the warning is drawn: {html}"
        );
        let env = html
            .split(r#"id="launch-env""#)
            .next()
            .and_then(|before| before.rsplit("<textarea").next())
            .expect("env textarea");
        assert!(
            env.contains(&warning_id),
            "the env field is described by the warning ({warning_id}): {env}"
        );
    }

    #[test]
    fn recent_commands_refill_from_the_label_and_launch_from_the_play_button() {
        #[component]
        fn Host() -> Element {
            let mut launched = use_signal(Vec::<AgentCommand>::new);
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
                    on_launch: move |agent: AgentCommand| launched.write().push(agent),
                }
                span { "data-slot": "launches", "{launched.read().len()}" }
                for agent in launched.read().iter() {
                    span { "data-slot": "launched", "{agent.command}" }
                }
            }
        }

        set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let mut dom = VirtualDom::new(Host);
        let edits = dom.rebuild_to_vec().edits;
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains(r#"class="recent-label recall""#)
                && html.contains("Fill in the form with bunx --flag agent-package")
                && html.contains(r#"aria-label="Launch bunx --flag agent-package""#),
            "each row refills and launches: {html}"
        );
        assert!(
            !html.contains(">stdio<"),
            "the one transport is not repeated on every row: {html}"
        );
        let clicks = listeners(&edits, "click");
        assert_eq!(
            clicks.len(),
            3,
            "the row's label and play button, and Cancel"
        );

        // The label refills without launching.
        let recall = target(
            &edits,
            "title",
            "Fill in the form with bunx --flag agent-package",
        );
        dom.runtime().handle_event("click", mouse_event(), recall);
        settle(&mut dom);
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
        assert!(html.contains(r#"data-slot="launches">0"#), "{html}");
        assert!(
            html.contains("Filled in bunx --flag agent-package. Press Launch to start it."),
            "{html}"
        );

        // Editing a field clears the confirmation.
        let input = listeners(&edits, "input")[0];
        dom.runtime()
            .handle_event("input", form_event("edited"), input);
        settle(&mut dom);
        assert!(!dioxus_ssr::render(&dom).contains("Filled in"));

        // The play button launches that invocation directly.
        let play = target(&edits, "aria-label", "Launch bunx --flag agent-package");
        dom.runtime().handle_event("click", mouse_event(), play);
        settle(&mut dom);
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains(r#"data-slot="launches">1"#)
                && html.contains(r#"data-slot="launched">bunx<"#),
            "the remembered invocation, not the edited fields: {html}"
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
            html.contains("Launching") && html.contains("Starting the agent"),
            "loading is explicit in words: {html}"
        );
        let launch = html
            .split("<button")
            .find(|button| button.contains(r#"data-slot="launch""#))
            .expect("the launch button");
        assert!(
            launch.contains(r#"aria-disabled="true""#)
                && launch.contains(r#"tabindex="-1""#)
                && !launch.contains("disabled=true"),
            "a launch cannot be repeated without replacing the focused button: {launch}"
        );
    }

    #[test]
    fn a_dialog_over_a_running_agent_offers_a_restart_with_its_settings() {
        #[component]
        fn Host() -> Element {
            rsx! {
                SpawnForm {
                    running: true,
                    launching: false,
                    recent: Vec::new(),
                    unsaved: None,
                    launched: running(),
                    on_launch: move |_: AgentCommand| {},
                }
            }
        }
        let mut dom = VirtualDom::new(Host);
        dom.rebuild_in_place();
        settle(&mut dom);
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains(">Restart with these settings</button>") && !html.contains(">Running<"),
            "{html}"
        );
        assert!(
            html.contains(r#"value="npx""#),
            "the fields hold what is running: {html}"
        );
    }

    #[test]
    fn a_path_is_shortened_at_home() {
        assert_eq!(
            abbreviate_path("/Users/me/.cargo/bin:/usr/bin:/Users/me", Some("/Users/me")),
            "~/.cargo/bin:/usr/bin:~"
        );
        assert_eq!(abbreviate_path("/usr/bin", None), "/usr/bin");
    }

    #[test]
    fn a_remembered_row_says_only_what_is_different_about_that_invocation() {
        assert_eq!(under(&command("npx", "", "", "")), None);
        let somewhere = under(&command("npx", "", "", "/home/laborant/phoenix/inspector"))
            .expect("a directory that was typed is a difference worth naming");
        assert_eq!(somewhere, "in …/phoenix/inspector");
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
            ..AgentCommand::default()
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
        assert_eq!(
            under(&command("npx", "", "", "/tmp")),
            Some("in /tmp".to_owned())
        );
        let secrets = command(
            "npx",
            "",
            "ACP_LOG=debug\nANTHROPIC_API_KEY=sk-secret",
            "/tmp",
        );
        let said = under(&secrets).expect("a line under it");
        assert_eq!(said, "in /tmp · 2 variables");
        assert!(!said.contains("sk-secret"), "{said}");
        assert_eq!(
            under(&command("npx", "", "ACP_LOG=debug\nHALF_TYPED", "")),
            Some("1 variable".to_owned())
        );
    }
}
