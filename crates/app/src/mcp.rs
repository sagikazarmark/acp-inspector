//! Shared, in-memory MCP input shown before Launch and in the Sessions dialog.
//!
//! Rows are keyed on core's identities ([`McpServerDraft::identify`]), so a
//! move or removal keeps the other rows' DOM nodes and focus. Findings wait
//! until a field is left or a launch is attempted; a transport the agent did
//! not advertise is said at once.
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use acp_inspector_core::{
    Inspector, McpDraft, McpEnv, McpFinding, McpServerDraft, McpTransport, v1,
};
use dioxus::prelude::*;
use dioxus_field::{FieldContext, FieldMetaValues};
use dioxus_free_icons::{
    Icon,
    icons::ld_icons::{LdArrowDown, LdArrowUp, LdTrash2},
};
use dioxus_primitives::dioxus_attributes::attributes;

use crate::components::{
    field::{Field, FieldError, FieldLabel},
    input::Input,
};

#[derive(Clone)]
pub struct McpInput {
    draft: Signal<McpDraft>,
    inspector: Inspector,
    capabilities: Signal<Option<v1::McpCapabilities>>,
    /// Whether a launch or a session opening was attempted with this draft,
    /// which is what reveals findings on fields nobody has touched.
    attempted: Signal<bool>,
}

pub fn provide(inspector: Inspector) {
    let draft = use_signal(|| {
        let mut draft = inspector.mcp_draft();
        draft.identify();
        inspector.set_mcp_draft(draft.clone());
        draft
    });
    let mut capabilities = use_signal(|| {
        inspector
            .agent()
            .map(|agent| agent.agent_capabilities.mcp_capabilities)
    });
    let attempted = use_signal(|| false);
    let watched = inspector.clone();
    use_future(move || {
        let inspector = watched.clone();
        async move {
            let mut changes = inspector.agent_changes();
            while let Some(agent) = changes.next().await {
                capabilities.set(agent.map(|agent| agent.agent_capabilities.mcp_capabilities));
            }
        }
    });
    use_context_provider(move || McpInput {
        draft,
        inspector,
        capabilities,
        attempted,
    });
}

/// Whether the draft can be sent with a launch. Callable from render and from
/// event handlers alike.
pub fn valid() -> bool {
    try_consume_context::<McpInput>().is_none_or(|input| input.draft.read().findings().is_empty())
}

/// A launch was attempted: say every finding, and take the reader to the first
/// one if there is any. Answers whether the draft is valid.
pub fn attempt_launch() -> bool {
    let Some(mut input) = try_consume_context::<McpInput>() else {
        return true;
    };
    input.attempted.set(true);
    let valid = input.draft.read().findings().is_empty();
    if !valid {
        reveal("#connect-dialog");
    }
    valid
}

/// Open the editor inside `dialog` and put focus in its first invalid field.
fn reveal(dialog: &'static str) {
    document::eval(&format!(
        "const section = document.querySelector('{dialog} [data-slot=mcp-editor]'); const details = section?.querySelector('details'); if (details) details.open = true; requestAnimationFrame(() => section?.querySelector('[aria-invalid=true]')?.focus());"
    ));
}

pub fn correct_in_sessions() {
    if let Some(mut input) = try_consume_context::<McpInput>() {
        input.attempted.set(true);
    }
    crate::session::open();
    reveal("#sessions-dialog");
}

pub fn ready_to_open() -> bool {
    if try_consume_context::<McpInput>().is_none_or(|input| {
        input
            .draft
            .read()
            .findings_for(input.capabilities.read().as_ref())
            .is_empty()
    }) {
        true
    } else {
        correct_in_sessions();
        false
    }
}

impl McpInput {
    fn edit(&self, change: impl FnOnce(&mut McpDraft)) {
        let mut signal = self.draft;
        let mut draft = signal.write();
        // Before, so parallel identity lists are aligned for a reorder; after,
        // so a row just added has one.
        draft.identify();
        change(&mut draft);
        draft.identify();
        // Synchronous publication: shortcuts in the same event batch must see
        // the edited list, including an incomplete row, not the previous list.
        self.inspector.set_mcp_draft(draft.clone());
    }
}

/// Controls that may take focus after a reorder, by DOM id. Not a signal: a
/// control mounting is not a reason to render anything again.
#[derive(Clone, Default)]
struct Focusable {
    mounted: Rc<RefCell<HashMap<String, Rc<MountedData>>>>,
    pending: Signal<Option<String>>,
}

impl Focusable {
    fn register(&self, id: String, element: Rc<MountedData>) {
        self.mounted.borrow_mut().insert(id, element);
    }

    fn request(&self, id: String) {
        let mut pending = self.pending;
        pending.set(Some(id));
    }
}

#[component]
pub fn McpEditor(prefix: &'static str) -> Element {
    let Some(input) = try_use_context::<McpInput>() else {
        return rsx! {};
    };
    let focusable = use_context_provider(|| Focusable {
        mounted: Rc::default(),
        pending: Signal::new(None),
    });
    let mut pending = focusable.pending;
    let registry = focusable.mounted.clone();
    use_effect(move || {
        let Some(id) = pending() else {
            return;
        };
        if let Some(element) = registry.borrow().get(&id).cloned() {
            spawn(async move {
                let _ = element.set_focus(true).await;
            });
        }
        pending.set(None);
    });

    let draft = input.draft.read().clone();
    let capabilities = use_memo(move || {
        if prefix == "launch" {
            None
        } else {
            (input.capabilities)()
        }
    });
    let mut touched = use_signal(HashSet::<(u64, &'static str)>::new);
    let attempted = (input.attempted)();
    let findings = draft.findings_for(capabilities.read().as_ref());
    // What is said now: anything about the agent's advertisement, and anything
    // about a field the reader has left or tried to submit.
    let shown = |server: &McpServerDraft, index: usize, field: &str| -> Option<String> {
        findings
            .iter()
            .find(|finding| finding.server == index && finding.field == field)
            .filter(|finding| {
                finding.field == "transport"
                    || attempted
                    || touched.read().contains(&(server.id, finding.field))
            })
            .map(|finding| finding.message.to_owned())
    };
    let any_shown = findings.iter().any(|finding: &McpFinding| {
        finding.field == "transport"
            || attempted
            || draft
                .servers
                .get(finding.server)
                .is_some_and(|server| touched.read().contains(&(server.id, finding.field)))
    });
    use_effect(use_reactive!(|any_shown| {
        if any_shown {
            let eval = document::eval(
                "const id = await dioxus.recv(); const details = document.getElementById(id); if (details) details.open = true;",
            );
            let _ = eval.send(format!("{prefix}-mcp-details"));
        }
    }));
    let edit = use_callback(move |change: Edit| input.edit(|draft| change.apply(draft)));
    let touch = use_callback(move |key: (u64, &'static str)| {
        if !touched.peek().contains(&key) {
            touched.write().insert(key);
        }
    });
    let count = draft.servers.len();
    let server_ids: Vec<u64> = draft.servers.iter().map(|server| server.id).collect();

    rsx! {
        section { class: "mcp-editor", "data-slot": "mcp-editor",
            details { id: "{prefix}-mcp-details", class: "mcp-disclosure",
                summary {
                    span { class: "mcp-summary", "MCP servers ({count})" }
                    span { class: "hint", " · sent when a session opens" }
                }
                p { class: "hint", "Used for the next new, load or resume. Doesn't change the open session. Not saved between launches." }
                details { class: "mcp-about",
                    summary { class: "hint", "About MCP servers" }
                    p { class: "hint", "The agent runs stdio commands and connects to HTTP and SSE URLs. A session opening doesn't confirm each MCP connection. Definitions, environment values and headers appear verbatim in the trace and in exports." }
                    if capabilities.read().is_none() {
                        p { class: "hint", "HTTP and SSE support is checked after the agent starts, before the first session opens." }
                    }
                }
                for (index, server) in draft.servers.iter().enumerate() {
                    fieldset { key: "{server.id}", class: "mcp-server",
                        legend { "MCP server {index + 1}" }
                        ListActions {
                            prefix,
                            label: format!("MCP server {}", index + 1),
                            ids: server_ids.clone(),
                            index,
                            add: format!("{prefix}-mcp-add-server"),
                            on_change: move |action| edit.call(Edit::Server(index, action)),
                        }
                        McpText {
                            id: format!("{prefix}-mcp-{}-name", server.id),
                            label: "Name",
                            value: server.name.clone(),
                            error: shown(server, index, "name"),
                            on_input: move |value| edit.call(Edit::Name(index, value)),
                            on_blur: {
                                let id = server.id;
                                move |()| touch.call((id, "name"))
                            },
                        }
                        div { class: "mcp-field",
                            label { r#for: "{prefix}-mcp-{server.id}-transport", "Transport" }
                            select {
                                id: "{prefix}-mcp-{server.id}-transport",
                                class: "select",
                                value: server.transport.token(),
                                aria_invalid: shown(server, index, "transport").is_some().then_some("true"),
                                aria_describedby: shown(server, index, "transport").map(|_| format!("{prefix}-mcp-{}-transport-finding", server.id)),
                                onchange: move |event| edit.call(Edit::Transport(index, match event.value().as_str() {
                                    "http" => McpTransport::Http,
                                    "sse" => McpTransport::Sse,
                                    _ => McpTransport::Stdio,
                                })),
                                for transport in [McpTransport::Stdio, McpTransport::Http, McpTransport::Sse] {
                                    option {
                                        value: transport.token(),
                                        disabled: capabilities.read().as_ref().is_some_and(|caps| !transport.advertised(caps)),
                                        "{transport.token()}"
                                    }
                                }
                            }
                            if let Some(message) = shown(server, index, "transport") {
                                p { id: "{prefix}-mcp-{server.id}-transport-finding", class: "detail detail-warn", role: "status", "{message}" }
                            }
                        }
                        if server.transport == McpTransport::Stdio {
                            McpText {
                                id: format!("{prefix}-mcp-{}-command", server.id),
                                label: "Command (absolute path)",
                                value: server.command.clone(),
                                mono: true,
                                error: shown(server, index, "command"),
                                on_input: move |value| edit.call(Edit::Command(index, value)),
                                on_blur: {
                                    let id = server.id;
                                    move |()| touch.call((id, "command"))
                                },
                            }
                            div { class: "mcp-rows", role: "group", aria_label: "Arguments",
                                p { class: "hint", "Arguments, in order. An empty row sends an empty argument." }
                                for (arg, value) in server.args.iter().enumerate() {
                                    div { key: "{server.arg_ids[arg]}", class: "mcp-row",
                                        label {
                                            r#for: "{prefix}-mcp-arg-{server.arg_ids[arg]}",
                                            "Argument {arg + 1}"
                                        }
                                        input {
                                            id: "{prefix}-mcp-arg-{server.arg_ids[arg]}",
                                            class: "input",
                                            value: value.clone(),
                                            spellcheck: "false",
                                            oninput: move |event| edit.call(Edit::Argument(index, arg, event.value())),
                                        }
                                        ListActions {
                                            prefix,
                                            label: format!("MCP server {} argument {}", index + 1, arg + 1),
                                            ids: server.arg_ids.clone(),
                                            index: arg,
                                            add: format!("{prefix}-mcp-{}-add-argument", server.id),
                                            on_change: move |action| edit.call(Edit::Arguments(index, arg, action)),
                                        }
                                    }
                                }
                                AddButton {
                                    id: format!("{prefix}-mcp-{}-add-argument", server.id),
                                    on_click: move |()| edit.call(Edit::AddArgument(index)),
                                    "Add argument"
                                }
                            }
                            div { class: "mcp-rows", role: "group", aria_label: "Environment",
                                p { class: "hint", "Environment, in order. Empty values and repeated names are sent as written." }
                                for (pair, env) in server.env.iter().enumerate() {
                                    Pair {
                                        key: "{server.env_ids[pair]}",
                                        prefix,
                                        id: format!("{prefix}-mcp-env-{}", server.env_ids[pair]),
                                        what: "Variable",
                                        nth: pair,
                                        name: env.name.clone(),
                                        value: env.value.clone(),
                                        on_name: move |value| edit.call(Edit::EnvName(index, pair, value)),
                                        on_value: move |value| edit.call(Edit::EnvValue(index, pair, value)),
                                        actions: rsx! {
                                            ListActions {
                                                prefix,
                                                label: format!("MCP server {} environment {}", index + 1, pair + 1),
                                                ids: server.env_ids.clone(),
                                                index: pair,
                                                add: format!("{prefix}-mcp-{}-add-env", server.id),
                                                on_change: move |action| edit.call(Edit::Environment(index, pair, action)),
                                            }
                                        },
                                    }
                                }
                                AddButton {
                                    id: format!("{prefix}-mcp-{}-add-env", server.id),
                                    on_click: move |()| edit.call(Edit::AddEnv(index)),
                                    "Add variable"
                                }
                            }
                        } else {
                            McpText {
                                id: format!("{prefix}-mcp-{}-url", server.id),
                                label: "URL",
                                value: server.url.clone(),
                                mono: true,
                                error: shown(server, index, "url"),
                                on_input: move |value| edit.call(Edit::Url(index, value)),
                                on_blur: {
                                    let id = server.id;
                                    move |()| touch.call((id, "url"))
                                },
                            }
                            div { class: "mcp-rows", role: "group", aria_label: "Headers",
                                p { class: "hint", "Headers, in order. Empty values and repeated names are sent as written." }
                                for (pair, header) in server.headers.iter().enumerate() {
                                    Pair {
                                        key: "{server.header_ids[pair]}",
                                        prefix,
                                        id: format!("{prefix}-mcp-header-{}", server.header_ids[pair]),
                                        what: "Header",
                                        nth: pair,
                                        name: header.name.clone(),
                                        value: header.value.clone(),
                                        on_name: move |value| edit.call(Edit::HeaderName(index, pair, value)),
                                        on_value: move |value| edit.call(Edit::HeaderValue(index, pair, value)),
                                        actions: rsx! {
                                            ListActions {
                                                prefix,
                                                label: format!("MCP server {} header {}", index + 1, pair + 1),
                                                ids: server.header_ids.clone(),
                                                index: pair,
                                                add: format!("{prefix}-mcp-{}-add-header", server.id),
                                                on_change: move |action| edit.call(Edit::Headers(index, pair, action)),
                                            }
                                        },
                                    }
                                }
                                AddButton {
                                    id: format!("{prefix}-mcp-{}-add-header", server.id),
                                    on_click: move |()| edit.call(Edit::AddHeader(index)),
                                    "Add header"
                                }
                            }
                        }
                    }
                }
                AddButton {
                    id: format!("{prefix}-mcp-add-server"),
                    on_click: move |()| edit.call(Edit::AddServer),
                    "Add MCP server"
                }
            }
        }
    }
}

/// One labelled text field: a visible label that is the accessible name, and
/// an error region that speaks once there is something to say.
#[component]
fn McpText(
    id: String,
    label: &'static str,
    value: ReadSignal<String>,
    #[props(default)] mono: bool,
    error: Option<String>,
    on_input: EventHandler<String>,
    on_blur: EventHandler<()>,
) -> Element {
    let context = FieldContext::empty().with_meta_values(FieldMetaValues {
        id: Some(Rc::from(id.as_str())),
        errors: error.iter().map(|error| Rc::from(error.as_str())).collect(),
        ..FieldMetaValues::default()
    });
    rsx! {
        Field { context, class: "mcp-field",
            FieldLabel { "{label}" }
            Input {
                value,
                class: if mono { "mono" } else { "" },
                on_change: move |value| on_input.call(value),
                on_focus_exit: move |()| on_blur.call(()),
                attributes: attributes!(input { "spellcheck": "false" }),
            }
            FieldError {}
        }
    }
}

/// A name and a value, each with its own visible label, as one group.
#[component]
fn Pair(
    prefix: &'static str,
    id: String,
    what: &'static str,
    nth: usize,
    name: String,
    value: String,
    on_name: EventHandler<String>,
    on_value: EventHandler<String>,
    actions: Element,
) -> Element {
    let _ = prefix;
    rsx! {
        div { class: "mcp-row", role: "group", aria_label: "{what} {nth + 1}",
            label { r#for: "{id}-name", "Name" }
            input {
                id: "{id}-name",
                class: "input",
                value: name,
                spellcheck: "false",
                oninput: move |event| on_name.call(event.value()),
            }
            label { r#for: "{id}-value", "Value" }
            input {
                id: "{id}-value",
                class: "input",
                value,
                spellcheck: "false",
                oninput: move |event| on_value.call(event.value()),
            }
            {actions}
        }
    }
}

/// An "add" control that can take focus back when the last row it made goes.
#[component]
fn AddButton(id: String, on_click: EventHandler<()>, children: Element) -> Element {
    let focusable = use_context::<Focusable>();
    let registered = id.clone();
    rsx! {
        button {
            id: "{id}",
            r#type: "button",
            class: "btn btn-xs btn-quiet",
            onmounted: move |event| focusable.register(registered.clone(), event.data()),
            onclick: move |_| on_click.call(()),
            {children}
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ListAction {
    Remove,
    Up,
    Down,
}

impl ListAction {
    fn slot(self) -> &'static str {
        match self {
            Self::Remove => "remove",
            Self::Up => "up",
            Self::Down => "down",
        }
    }
}

/// Which control takes focus after `action` on the row at `index` of `ids`:
/// the moved row's own control in the direction it can still go, or its
/// remove control at an end; after a removal, the nearest survivor's remove
/// control, or the list's add control when none is left.
fn focus_after(
    prefix: &str,
    ids: &[u64],
    index: usize,
    action: ListAction,
    add: &str,
) -> Option<String> {
    let moved = *ids.get(index)?;
    let control = |id: u64, action: ListAction| format!("{prefix}-mcp-{id}-{}", action.slot());
    Some(match action {
        ListAction::Up if index == 0 => return None,
        ListAction::Up if index > 1 => control(moved, ListAction::Up),
        ListAction::Up => control(moved, ListAction::Remove),
        ListAction::Down if index + 1 >= ids.len() => return None,
        ListAction::Down if index + 2 < ids.len() => control(moved, ListAction::Down),
        ListAction::Down => control(moved, ListAction::Remove),
        ListAction::Remove => match ids
            .get(index + 1)
            .or_else(|| index.checked_sub(1).and_then(|before| ids.get(before)))
        {
            Some(survivor) => control(*survivor, ListAction::Remove),
            None => add.to_owned(),
        },
    })
}

#[component]
fn ListActions(
    prefix: &'static str,
    label: String,
    /// The identities of the list this row is in, in order.
    ids: Vec<u64>,
    index: usize,
    /// The add control's DOM id, for focus once the list is empty.
    add: String,
    on_change: EventHandler<ListAction>,
) -> Element {
    let focusable = use_context::<Focusable>();
    let len = ids.len();
    let id = ids.get(index).copied().unwrap_or_default();
    let change = {
        let focusable = focusable.clone();
        let ids = ids.clone();
        let add = add.clone();
        move |action: ListAction| {
            let target = focus_after(prefix, &ids, index, action, &add);
            on_change.call(action);
            if let Some(target) = target {
                focusable.request(target);
            }
        }
    };
    let button = |action: ListAction| {
        let focusable = focusable.clone();
        let dom = format!("{prefix}-mcp-{id}-{}", action.slot());
        let registered = dom.clone();
        let change = change.clone();
        let (name, icon) = match action {
            ListAction::Up => (
                format!("Move {label} up"),
                rsx! { Icon { class: "icon", width: 16, height: 16, icon: LdArrowUp } },
            ),
            ListAction::Down => (
                format!("Move {label} down"),
                rsx! { Icon { class: "icon", width: 16, height: 16, icon: LdArrowDown } },
            ),
            ListAction::Remove => (
                format!("Remove {label}"),
                rsx! { Icon { class: "icon", width: 16, height: 16, icon: LdTrash2 } },
            ),
        };
        let disabled = match action {
            ListAction::Up => index == 0,
            ListAction::Down => index + 1 >= len,
            ListAction::Remove => false,
        };
        rsx! {
            button {
                id: "{dom}",
                r#type: "button",
                class: "btn btn-xs btn-ghost",
                aria_label: "{name}",
                title: "{name}",
                disabled,
                onmounted: move |event| focusable.register(registered.clone(), event.data()),
                onclick: move |_| change(action),
                span { class: "disclosure-icon", aria_hidden: "true", {icon} }
            }
        }
    };
    rsx! {
        div { class: "mcp-actions",
            {button(ListAction::Up)}
            {button(ListAction::Down)}
            {button(ListAction::Remove)}
        }
    }
}

fn rearrange<T>(items: &mut Vec<T>, ids: &mut Vec<u64>, index: usize, action: ListAction) {
    if index >= items.len() {
        return;
    }
    match action {
        ListAction::Remove => {
            items.remove(index);
            if index < ids.len() {
                ids.remove(index);
            }
        }
        ListAction::Up if index > 0 => {
            items.swap(index, index - 1);
            if index < ids.len() {
                ids.swap(index, index - 1);
            }
        }
        ListAction::Down if index + 1 < items.len() => {
            items.swap(index, index + 1);
            if index + 1 < ids.len() {
                ids.swap(index, index + 1);
            }
        }
        _ => {}
    }
}

enum Edit {
    Transport(usize, McpTransport),
    Url(usize, String),
    AddHeader(usize),
    HeaderName(usize, usize, String),
    HeaderValue(usize, usize, String),
    Headers(usize, usize, ListAction),
    AddServer,
    Server(usize, ListAction),
    Name(usize, String),
    Command(usize, String),
    AddArgument(usize),
    Argument(usize, usize, String),
    Arguments(usize, usize, ListAction),
    AddEnv(usize),
    EnvName(usize, usize, String),
    EnvValue(usize, usize, String),
    Environment(usize, usize, ListAction),
}
impl Edit {
    fn apply(self, draft: &mut McpDraft) {
        match self {
            Self::AddServer => draft.servers.push(McpServerDraft::default()),
            Self::Transport(i, value) => {
                if let Some(s) = draft.servers.get_mut(i) {
                    s.transport = value;
                }
            }
            Self::Url(i, value) => {
                if let Some(s) = draft.servers.get_mut(i) {
                    s.url = value;
                }
            }
            Self::AddHeader(i) => {
                if let Some(s) = draft.servers.get_mut(i) {
                    s.headers.push(McpEnv::default());
                }
            }
            Self::HeaderName(i, j, value) => {
                if let Some(v) = draft.servers.get_mut(i).and_then(|s| s.headers.get_mut(j)) {
                    v.name = value;
                }
            }
            Self::HeaderValue(i, j, value) => {
                if let Some(v) = draft.servers.get_mut(i).and_then(|s| s.headers.get_mut(j)) {
                    v.value = value;
                }
            }
            Self::Headers(i, j, action) => {
                if let Some(s) = draft.servers.get_mut(i) {
                    rearrange(&mut s.headers, &mut s.header_ids, j, action);
                }
            }
            Self::Server(i, action) => {
                let mut ids: Vec<u64> = draft.servers.iter().map(|server| server.id).collect();
                rearrange(&mut draft.servers, &mut ids, i, action);
            }
            Self::Name(i, value) => {
                if let Some(s) = draft.servers.get_mut(i) {
                    s.name = value;
                }
            }
            Self::Command(i, value) => {
                if let Some(s) = draft.servers.get_mut(i) {
                    s.command = value;
                }
            }
            Self::AddArgument(i) => {
                if let Some(s) = draft.servers.get_mut(i) {
                    s.args.push(String::new());
                }
            }
            Self::Argument(i, j, value) => {
                if let Some(v) = draft.servers.get_mut(i).and_then(|s| s.args.get_mut(j)) {
                    *v = value;
                }
            }
            Self::Arguments(i, j, action) => {
                if let Some(s) = draft.servers.get_mut(i) {
                    rearrange(&mut s.args, &mut s.arg_ids, j, action);
                }
            }
            Self::AddEnv(i) => {
                if let Some(s) = draft.servers.get_mut(i) {
                    s.env.push(McpEnv::default());
                }
            }
            Self::EnvName(i, j, value) => {
                if let Some(v) = draft.servers.get_mut(i).and_then(|s| s.env.get_mut(j)) {
                    v.name = value;
                }
            }
            Self::EnvValue(i, j, value) => {
                if let Some(v) = draft.servers.get_mut(i).and_then(|s| s.env.get_mut(j)) {
                    v.value = value;
                }
            }
            Self::Environment(i, j, action) => {
                if let Some(s) = draft.servers.get_mut(i) {
                    rearrange(&mut s.env, &mut s.env_ids, j, action);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::core::Mutation;
    use dioxus::html::{PlatformEventData, SerializedFormData, SerializedMouseData};
    use std::{any::Any, rc::Rc};

    thread_local! {static TEST_CAPS: std::cell::RefCell<Option<Signal<Option<v1::McpCapabilities>>>> = const {std::cell::RefCell::new(None)};}

    #[test]
    fn remote_editor_keeps_unsupported_rows_and_ordered_headers_for_correction() {
        #[derive(Clone, Props)]
        struct Props {
            inspector: Inspector,
        }
        impl PartialEq for Props {
            fn eq(&self, _: &Self) -> bool {
                true
            }
        }
        fn host(props: Props) -> Element {
            provide(props.inspector);
            let input = use_context::<McpInput>();
            use_hook(move || TEST_CAPS.with(|slot| *slot.borrow_mut() = Some(input.capabilities)));
            rsx! {McpEditor{prefix:"launch"} McpEditor{prefix:"remote"}}
        }
        set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let inspector = Inspector::new();
        inspector.set_mcp_draft(McpDraft {
            servers: vec![McpServerDraft {
                name: "remote".into(),
                transport: McpTransport::Http,
                url: "https://example.invalid/mcp".into(),
                headers: vec![
                    McpEnv {
                        name: "X".into(),
                        value: "".into(),
                    },
                    McpEnv {
                        name: "X".into(),
                        value: "two".into(),
                    },
                ],
                ..Default::default()
            }],
        });
        let mut dom = VirtualDom::new_with_props(
            host,
            Props {
                inspector: inspector.clone(),
            },
        );
        let mut edits = dom.rebuild_to_vec().edits;
        assert!(dioxus_ssr::render(&dom).contains("checked after the agent starts"));
        let mut caps = TEST_CAPS.with(|slot| slot.borrow().unwrap());
        caps.set(Some(v1::McpCapabilities::new().sse(true)));
        edits.extend(dom.render_immediate_to_vec().edits);
        let html = dioxus_ssr::render(&dom);
        assert!(html.contains("The Agent did not advertise HTTP MCP definitions."));
        assert!(html.contains("https://example.invalid/mcp"));
        let launch = html
            .split("id=\"launch-mcp-details\"")
            .nth(1)
            .unwrap()
            .split("id=\"remote-mcp-details\"")
            .next()
            .unwrap();
        assert!(!launch.contains("did not advertise"));
        assert!(
            !launch
                .split("</select>")
                .next()
                .unwrap()
                .contains("option value=\"http\" disabled"),
            "next Launch must not use previous Agent gates"
        );
        let server = inspector.mcp_draft().servers[0].clone();
        let transport = target(&edits, "id", &format!("remote-mcp-{}-transport", server.id));
        dom.runtime().handle_event(
            "change",
            Event::new(
                Rc::new(PlatformEventData::new(Box::new(SerializedFormData::new(
                    "sse".into(),
                    vec![],
                )))) as Rc<dyn Any>,
                true,
            ),
            transport,
        );
        edits.extend(dom.render_immediate_to_vec().edits);
        assert!(!dioxus_ssr::render(&dom).contains("did not advertise"));
        let value = target(
            &edits,
            "id",
            &format!("remote-mcp-header-{}-value", server.header_ids[1]),
        );
        edits.extend(input(&mut dom, value, " two words "));
        let up = target(&edits, "aria-label", "Move MCP server 1 header 2 up");
        click(&mut dom, up);
        let definitions = inspector
            .mcp_draft()
            .definitions_for(&v1::McpCapabilities::new().sse(true))
            .unwrap();
        let v1::McpServer::Sse(server) = &definitions[0] else {
            panic!("sse")
        };
        assert_eq!(
            server
                .headers
                .iter()
                .map(|h| (&*h.name, &*h.value))
                .collect::<Vec<_>>(),
            vec![("X", " two words "), ("X", "")]
        );
    }

    fn target(edits: &[Mutation], attribute: &str, expected: &str) -> dioxus::core::ElementId {
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
            .unwrap_or_else(|| panic!("missing target {attribute}={expected}"))
    }
    fn click(dom: &mut VirtualDom, id: dioxus::core::ElementId) -> Vec<Mutation> {
        dom.runtime().handle_event(
            "click",
            Event::new(
                Rc::new(PlatformEventData::new(Box::<SerializedMouseData>::default()))
                    as Rc<dyn Any>,
                true,
            ),
            id,
        );
        dom.render_immediate_to_vec().edits
    }
    fn input(dom: &mut VirtualDom, id: dioxus::core::ElementId, value: &str) -> Vec<Mutation> {
        dom.runtime().handle_event(
            "input",
            Event::new(
                Rc::new(PlatformEventData::new(Box::new(SerializedFormData::new(
                    value.into(),
                    vec![],
                )))) as Rc<dyn Any>,
                true,
            ),
            id,
        );
        dom.render_immediate_to_vec().edits
    }

    #[test]
    fn rendered_editor_publishes_edits_order_and_validation_to_the_shared_inspector() {
        #[derive(Clone, Props)]
        struct Props {
            inspector: Inspector,
        }
        impl PartialEq for Props {
            fn eq(&self, _: &Self) -> bool {
                true
            }
        }
        fn host(props: Props) -> Element {
            provide(props.inspector);
            rsx! {McpEditor{prefix:"test"}}
        }
        set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let inspector = Inspector::new();
        let executable = std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        inspector.set_mcp_draft(McpDraft {
            servers: vec![
                McpServerDraft {
                    name: "first".into(),
                    command: executable.clone(),
                    args: vec!["".into(), "two words".into()],
                    env: vec![
                        McpEnv {
                            name: "X".into(),
                            value: "".into(),
                        },
                        McpEnv {
                            name: "X".into(),
                            value: "two".into(),
                        },
                    ],
                    ..Default::default()
                },
                McpServerDraft {
                    name: "second".into(),
                    command: executable.clone(),
                    ..Default::default()
                },
            ],
        });
        let mut dom = VirtualDom::new_with_props(
            host,
            Props {
                inspector: inspector.clone(),
            },
        );
        let mut edits = dom.rebuild_to_vec().edits;
        edits.extend(dom.render_immediate_to_vec().edits);
        assert!(dioxus_ssr::render(&dom).contains("MCP servers (2)"));
        let first = inspector.mcp_draft().servers[0].clone();
        let command = target(&edits, "id", &format!("test-mcp-{}-command", first.id));
        edits.extend(input(&mut dom, command, "relative"));
        assert!(inspector.mcp_draft().definitions().is_err());
        // Not said while the field is still being typed into.
        assert!(!dioxus_ssr::render(&dom).contains("Enter an absolute executable path."));
        edits.extend(input(&mut dom, command, &executable));
        let argument = target(&edits, "id", &format!("test-mcp-arg-{}", first.arg_ids[0]));
        edits.extend(input(&mut dom, argument, " leading and trailing "));
        let up = target(&edits, "aria-label", "Move MCP server 1 argument 2 up");
        edits.extend(click(&mut dom, up));
        assert_eq!(
            inspector.mcp_draft().servers[0].args,
            vec!["two words", " leading and trailing "]
        );
        // The rows moved with their identities: the second identity is first.
        assert_eq!(
            inspector.mcp_draft().servers[0].arg_ids,
            vec![first.arg_ids[1], first.arg_ids[0]]
        );
        let value = target(
            &edits,
            "id",
            &format!("test-mcp-env-{}-value", first.env_ids[1]),
        );
        edits.extend(input(&mut dom, value, ""));
        assert_eq!(
            inspector.mcp_draft().servers[0]
                .env
                .iter()
                .map(|p| (&*p.name, &*p.value))
                .collect::<Vec<_>>(),
            vec![("X", ""), ("X", "")]
        );
        let down = target(&edits, "aria-label", "Move MCP server 1 down");
        edits.extend(click(&mut dom, down));
        assert_eq!(inspector.mcp_draft().servers[0].name, "second");
        let remove = target(&edits, "aria-label", "Remove MCP server 2");
        click(&mut dom, remove);
        assert_eq!(inspector.mcp_draft().servers.len(), 1);
        assert!(inspector.mcp_draft().definitions().is_ok());
    }

    #[test]
    fn findings_wait_for_a_touch_or_an_attempt_and_mark_the_field_invalid() {
        #[derive(Clone, Props)]
        struct Props {
            inspector: Inspector,
        }
        impl PartialEq for Props {
            fn eq(&self, _: &Self) -> bool {
                true
            }
        }
        fn host(props: Props) -> Element {
            provide(props.inspector);
            let input = use_context::<McpInput>();
            rsx! {
                button { aria_label: "Attempt".to_owned(), onclick: move |_| { let mut attempted = input.attempted; attempted.set(true); }, "Attempt" }
                McpEditor { prefix: "test" }
            }
        }
        set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let inspector = Inspector::new();
        inspector.set_mcp_draft(McpDraft {
            servers: vec![McpServerDraft::default()],
        });
        let mut dom = VirtualDom::new_with_props(
            host,
            Props {
                inspector: inspector.clone(),
            },
        );
        let edits = dom.rebuild_to_vec().edits;
        dom.render_immediate_to_vec();
        let html = dioxus_ssr::render(&dom);
        let id = inspector.mcp_draft().servers[0].id;
        assert!(
            html.contains(&format!(
                r#"for="test-mcp-{id}-name" class="label">Name</label>"#
            )),
            "the visible label names the control: {html}"
        );
        assert!(
            !html.contains("Enter a nonblank server name.")
                && !html.contains(r#"aria-invalid="true""#),
            "nothing is said before the reader is done: {html}"
        );
        click(&mut dom, target(&edits, "aria-label", "Attempt"));
        dom.render_immediate_to_vec();
        dom.render_immediate_to_vec();
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains("Enter a nonblank server name.")
                && html.contains("Enter an absolute executable path.")
                && html.contains(r#"aria-invalid="true""#),
            "an attempt says every finding where it is: {html}"
        );
    }

    #[test]
    fn focus_follows_the_moved_row_or_the_nearest_survivor() {
        let ids = [10, 20, 30];
        let add = "p-mcp-add";
        assert_eq!(
            focus_after("p", &ids, 2, ListAction::Up, add).as_deref(),
            Some("p-mcp-30-up")
        );
        assert_eq!(
            focus_after("p", &ids, 1, ListAction::Up, add).as_deref(),
            Some("p-mcp-20-remove"),
            "a row moved to the top can no longer go up"
        );
        assert_eq!(
            focus_after("p", &ids, 0, ListAction::Down, add).as_deref(),
            Some("p-mcp-10-down")
        );
        assert_eq!(
            focus_after("p", &ids, 1, ListAction::Down, add).as_deref(),
            Some("p-mcp-20-remove")
        );
        assert_eq!(
            focus_after("p", &ids, 1, ListAction::Remove, add).as_deref(),
            Some("p-mcp-30-remove")
        );
        assert_eq!(
            focus_after("p", &ids, 2, ListAction::Remove, add).as_deref(),
            Some("p-mcp-20-remove")
        );
        assert_eq!(
            focus_after("p", &[10], 0, ListAction::Remove, add).as_deref(),
            Some(add)
        );
    }
}
