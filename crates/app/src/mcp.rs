//! Shared, in-memory MCP input shown before Launch and in the Sessions dialog.
use acp_inspector_core::{Inspector, McpDraft, McpEnv, McpServerDraft, McpTransport, v1};
use dioxus::prelude::*;

#[derive(Clone)]
pub struct McpInput {
    draft: Signal<McpDraft>,
    inspector: Inspector,
    capabilities: Signal<Option<v1::McpCapabilities>>,
}

pub fn provide(inspector: Inspector) {
    let draft = use_signal(|| inspector.mcp_draft());
    let mut capabilities = use_signal(|| {
        inspector
            .agent()
            .map(|agent| agent.agent_capabilities.mcp_capabilities)
    });
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
    });
}

pub fn valid() -> bool {
    try_use_context::<McpInput>().is_none_or(|input| input.draft.read().findings().is_empty())
}

pub fn correct_in_sessions() {
    crate::session::open();
    document::eval(
        "const section = document.querySelector('#sessions-dialog [data-slot=mcp-editor]'); const details = section?.querySelector('details'); if (details) details.open = true; section?.querySelector('[aria-describedby]')?.focus();",
    );
}

pub fn ready_to_open() -> bool {
    if try_use_context::<McpInput>().is_none_or(|input| {
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
        change(&mut draft);
        // Synchronous publication: shortcuts in the same event batch must see
        // the edited list, including an incomplete row, not the previous list.
        self.inspector.set_mcp_draft(draft.clone());
    }
}

#[component]
pub fn McpEditor(prefix: &'static str) -> Element {
    let Some(input) = try_use_context::<McpInput>() else {
        return rsx! {};
    };
    let draft = input.draft.read().clone();
    let capabilities = use_memo(move || {
        if prefix == "launch" {
            None
        } else {
            (input.capabilities)()
        }
    });
    let findings = draft.findings_for(capabilities.read().as_ref());
    let draft_signal = input.draft;
    use_effect(move || {
        if !draft_signal
            .read()
            .findings_for(capabilities.read().as_ref())
            .is_empty()
        {
            let eval = document::eval(
                "const id = await dioxus.recv(); const details = document.getElementById(id); if (details) details.open = true;",
            );
            let _ = eval.send(format!("{prefix}-mcp-details"));
        }
    });
    let edit = use_callback(move |change: Edit| input.edit(|draft| change.apply(draft)));
    rsx! {
        section { class: "mcp-editor", "data-slot":"mcp-editor",
            p { class:"hint", "{draft.servers.len()} MCP servers will be supplied on the next Session-opening call." }
            details { id:"{prefix}-mcp-details",
                summary { "MCP servers" }
                p { class:"hint", "Inspector-supplied input, shared with Launch and Sessions. Edits affect subsequent new/load/resume calls, not the live Session. Reconnect keeps this list; recent commands do not save it." }
                p { class:"hint", "The Agent runs stdio commands and connects to HTTP/SSE URLs. A Session setup answer does not confirm individual MCP connections. Definitions, environment values and headers appear verbatim in Trace and exports." }
                if capabilities.read().is_none() { p { class:"hint", "HTTP/SSE Advertisements will be checked after initialize, before opening the first Session." } }
                for (index, server) in draft.servers.iter().enumerate() {
                    fieldset { key:"{index}", class:"mcp-server",
                        legend { "MCP server {index + 1}" }
                        ListActions { prefix, label:format!("MCP server {}",index+1), base:"MCP server".to_owned(), index, len:draft.servers.len(), on_change:move |action| edit.call(Edit::Server(index,action)) }
                        label { r#for:"{prefix}-mcp-{index}-name", "Server name" }
                        input { id:"{prefix}-mcp-{index}-name", class:"input", value:server.name.clone(), aria_label:"MCP server {index + 1} name", aria_describedby:findings.iter().any(|f|f.server==index&&f.field=="name").then(||format!("{prefix}-mcp-{index}-name-finding")),
                            oninput:move |event| edit.call(Edit::Name(index,event.value())) }
                        for finding in findings.iter().filter(|f|f.server==index&&f.field=="name") {
                            p { id:"{prefix}-mcp-{index}-name-finding", class:"detail detail-warn", "{finding.message}" }
                        }
                        label { r#for:"{prefix}-mcp-{index}-transport", "Transport" }
                        select { id:"{prefix}-mcp-{index}-transport", class:"select", value:server.transport.token(), aria_label:"MCP server {index + 1} transport", aria_describedby:findings.iter().any(|f|f.server==index&&f.field=="transport").then(||format!("{prefix}-mcp-{index}-transport-finding")),
                            onchange:move |event| edit.call(Edit::Transport(index,match event.value().as_str() {"http"=>McpTransport::Http,"sse"=>McpTransport::Sse,_=>McpTransport::Stdio})),
                            for transport in [McpTransport::Stdio,McpTransport::Http,McpTransport::Sse] {
                                option { value:transport.token(), disabled:capabilities.read().as_ref().is_some_and(|caps| !transport.advertised(caps)), "{transport.token()}" }
                            }
                        }
                        for finding in findings.iter().filter(|f|f.server==index&&f.field=="transport") {
                            p { id:"{prefix}-mcp-{index}-transport-finding", class:"detail detail-warn", role:"status", "{finding.message}" }
                        }
                        if server.transport == McpTransport::Stdio {
                        label { r#for:"{prefix}-mcp-{index}-command", "Absolute executable path" }
                        input { id:"{prefix}-mcp-{index}-command", class:"input", value:server.command.clone(), aria_label:"MCP server {index + 1} command", spellcheck:false, aria_describedby:findings.iter().any(|f|f.server==index&&f.field=="command").then(||format!("{prefix}-mcp-{index}-command-finding")),
                            oninput:move |event| edit.call(Edit::Command(index,event.value())) }
                        for finding in findings.iter().filter(|f|f.server==index&&f.field=="command") {
                            p { id:"{prefix}-mcp-{index}-command-finding", class:"detail detail-warn", "{finding.message}" }
                        }
                        p { class:"hint", "Arguments in order; an empty row sends an empty argument." }
                        for (arg, value) in server.args.iter().enumerate() {
                            div { key:"{arg}", class:"mcp-row",
                                input { class:"input", value:value.clone(), aria_label:"MCP server {index + 1} argument {arg + 1}", oninput:move |event| edit.call(Edit::Argument(index,arg,event.value())) }
                                ListActions { prefix, label:format!("MCP server {} argument {}",index+1,arg+1), base:format!("MCP server {} argument",index+1), index:arg, len:server.args.len(), on_change:move |action| edit.call(Edit::Arguments(index,arg,action)) }
                            }
                        }
                        button { r#type:"button", class:"btn btn-xs btn-quiet", onclick:move |_| edit.call(Edit::AddArgument(index)), "Add argument to MCP server {index + 1}" }
                        p { class:"hint", "Environment in order; empty values and duplicate names are preserved." }
                        for (pair, env) in server.env.iter().enumerate() {
                            div { key:"{pair}", class:"mcp-row",
                                input { class:"input", value:env.name.clone(), aria_label:"MCP server {index + 1} environment {pair + 1} name", oninput:move |event| edit.call(Edit::EnvName(index,pair,event.value())) }
                                input { class:"input", value:env.value.clone(), aria_label:"MCP server {index + 1} environment {pair + 1} value", oninput:move |event| edit.call(Edit::EnvValue(index,pair,event.value())) }
                                ListActions { prefix, label:format!("MCP server {} environment {}",index+1,pair+1), base:format!("MCP server {} environment",index+1), index:pair, len:server.env.len(), on_change:move |action| edit.call(Edit::Environment(index,pair,action)) }
                            }
                        }
                        button { r#type:"button", class:"btn btn-xs btn-quiet", onclick:move |_| edit.call(Edit::AddEnv(index)), "Add environment to MCP server {index + 1}" }
                        } else {
                            label { r#for:"{prefix}-mcp-{index}-url", "URL" }
                            input { id:"{prefix}-mcp-{index}-url", class:"input", value:server.url.clone(), aria_label:"MCP server {index + 1} URL", spellcheck:false,
                                aria_describedby:findings.iter().any(|f|f.server==index&&f.field=="url").then(||format!("{prefix}-mcp-{index}-url-finding")),
                                oninput:move |event| edit.call(Edit::Url(index,event.value())) }
                            for finding in findings.iter().filter(|f|f.server==index&&f.field=="url") {
                                p { id:"{prefix}-mcp-{index}-url-finding", class:"detail detail-warn", "{finding.message}" }
                            }
                            p { class:"hint", "Headers in order; empty values and duplicate names are preserved." }
                            for (pair, header) in server.headers.iter().enumerate() {
                                div { key:"{pair}", class:"mcp-row",
                                    input {class:"input",value:header.name.clone(),aria_label:"MCP server {index + 1} header {pair + 1} name",oninput:move |event|edit.call(Edit::HeaderName(index,pair,event.value()))}
                                    input {class:"input",value:header.value.clone(),aria_label:"MCP server {index + 1} header {pair + 1} value",oninput:move |event|edit.call(Edit::HeaderValue(index,pair,event.value()))}
                                    ListActions {prefix,label:format!("MCP server {} header {}",index+1,pair+1),base:format!("MCP server {} header",index+1),index:pair,len:server.headers.len(),on_change:move |action|edit.call(Edit::Headers(index,pair,action))}
                                }
                            }
                            button {r#type:"button",class:"btn btn-xs btn-quiet",onclick:move |_| edit.call(Edit::AddHeader(index)),"Add header to MCP server {index + 1}"}
                        }
                    }
                }
                button { r#type:"button", class:"btn btn-xs btn-quiet", onclick:move |_| edit.call(Edit::AddServer), "Add MCP server" }
            }
        }
    }
}

#[derive(Clone, Copy)]
enum ListAction {
    Remove,
    Up,
    Down,
}

#[component]
fn ListActions(
    prefix: &'static str,
    label: String,
    base: String,
    index: usize,
    len: usize,
    on_change: EventHandler<ListAction>,
) -> Element {
    let change = use_callback(move |action| {
        on_change.call(action);
        let target = match action {
            ListAction::Up if index > 1 => format!("Move {base} {} up", index),
            ListAction::Down if index + 2 < len => format!("Move {base} {} down", index + 2),
            ListAction::Up => format!("Remove {base} {}", index),
            ListAction::Down => format!("Remove {base} {}", index + 2),
            ListAction::Remove if len > 1 => format!("Remove {base} {}", (index + 1).min(len - 1)),
            ListAction::Remove => String::new(),
        };
        // Position identifies display order, not focus ownership: after moving,
        // follow the moved item; after removal, follow its nearest survivor.
        let eval = document::eval(
            "const [id, label] = await dioxus.recv(); requestAnimationFrame(() => requestAnimationFrame(() => { const root = document.getElementById(id); const buttons = Array.from(root?.querySelectorAll('button') || []); const target = buttons.find(b => b.getAttribute('aria-label') === label) || buttons[buttons.length - 1]; target?.focus(); }));",
        );
        let _ = eval.send((format!("{prefix}-mcp-details"), target));
    });
    rsx! { div { class:"mcp-actions",
        button { r#type:"button", class:"btn btn-xs btn-ghost", aria_label:"Move {label} up", disabled:index==0, onclick:move |_| change.call(ListAction::Up), "↑" }
        button { r#type:"button", class:"btn btn-xs btn-ghost", aria_label:"Move {label} down", disabled:index+1>=len, onclick:move |_| change.call(ListAction::Down), "↓" }
        button { r#type:"button", class:"btn btn-xs btn-ghost", aria_label:"Remove {label}", onclick:move |_| change.call(ListAction::Remove), "Remove" }
    } }
}

fn rearrange<T>(items: &mut Vec<T>, index: usize, action: ListAction) {
    if index >= items.len() {
        return;
    }
    match action {
        ListAction::Remove => {
            items.remove(index);
        }
        ListAction::Up if index > 0 => items.swap(index, index - 1),
        ListAction::Down if index + 1 < items.len() => items.swap(index, index + 1),
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
                    rearrange(&mut s.headers, j, action);
                }
            }
            Self::Server(i, action) => rearrange(&mut draft.servers, i, action),
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
                    rearrange(&mut s.args, j, action);
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
                    rearrange(&mut s.env, j, action);
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
        assert!(dioxus_ssr::render(&dom).contains("checked after initialize"));
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
            .split("</details>")
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
        let transport = target(&edits, "MCP server 1 transport");
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
        let value = target(&edits, "MCP server 1 header 2 value");
        edits.extend(input(&mut dom, value, " two words "));
        let up = target(&edits, "Move MCP server 1 header 2 up");
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

    fn target(edits: &[Mutation], label: &str) -> dioxus::core::ElementId {
        edits
            .iter()
            .rev()
            .find_map(|edit| match edit {
                Mutation::SetAttribute {
                    name: "aria-label",
                    value: dioxus::core::AttributeValue::Text(value),
                    id,
                    ..
                } if *value == label => Some(*id),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing target {label}"))
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
        inspector.set_mcp_draft(McpDraft {
            servers: vec![
                McpServerDraft {
                    name: "first".into(),
                    command: std::env::current_exe()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
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
                    command: std::env::current_exe()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
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
        assert!(dioxus_ssr::render(&dom).contains("2 MCP servers will be supplied"));
        let command = target(&edits, "MCP server 1 command");
        edits.extend(input(&mut dom, command, "relative"));
        assert!(inspector.mcp_draft().definitions().is_err());
        assert!(dioxus_ssr::render(&dom).contains("Enter an absolute executable path."));
        edits.extend(input(
            &mut dom,
            command,
            &std::env::current_exe().unwrap().to_string_lossy(),
        ));
        let argument = target(&edits, "MCP server 1 argument 1");
        edits.extend(input(&mut dom, argument, " leading and trailing "));
        let up = target(&edits, "Move MCP server 1 argument 2 up");
        edits.extend(click(&mut dom, up));
        assert_eq!(
            inspector.mcp_draft().servers[0].args,
            vec!["two words", " leading and trailing "]
        );
        let value = target(&edits, "MCP server 1 environment 2 value");
        edits.extend(input(&mut dom, value, ""));
        assert_eq!(
            inspector.mcp_draft().servers[0]
                .env
                .iter()
                .map(|p| (&*p.name, &*p.value))
                .collect::<Vec<_>>(),
            vec![("X", ""), ("X", "")]
        );
        let down = target(&edits, "Move MCP server 1 down");
        edits.extend(click(&mut dom, down));
        assert_eq!(inspector.mcp_draft().servers[0].name, "second");
        let remove = target(&edits, "Remove MCP server 2");
        click(&mut dom, remove);
        assert_eq!(inspector.mcp_draft().servers.len(), 1);
        assert!(inspector.mcp_draft().definitions().is_ok());
    }
}
