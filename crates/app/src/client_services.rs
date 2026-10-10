//! Client services in the window (`docs/client-services.md`): what the next
//! launch claims (F3, F13), the card a service request is answered from, and
//! the open session's permission policy (F11).
//!
//! **Answered, never performed.** A claimed `fs/*` or `terminal/*` call is a
//! card with the method, the params and a response editor; nothing here reads
//! or writes a file or runs a command. A rules file chosen at launch can answer
//! first, and the card then says which rule did.

use acp_inspector_core::{
    AnsweredBy, ClientClaim, ClientServices, Inspector, PermissionPolicy, ServiceReply,
    ServiceRequest, ServiceScript, ServiceState, v1,
};
use dioxus::prelude::*;
use serde_json::Value;

/// Rules files are small; a bigger one is almost certainly the wrong file.
const RULES_LIMIT: usize = 1024 * 1024;

/// The window's handle on the launch's choices and the session's policy.
#[derive(Clone)]
struct Services {
    inspector: Inspector,
    draft: Signal<ClientServices>,
    policy: Signal<PermissionPolicy>,
}

/// Makes the launch draft and the permission policy available to the spawn
/// form's control and the timeline's, reading both from core and writing both
/// back to it.
pub fn provide(inspector: Inspector) {
    let draft = use_signal(|| inspector.client_services());
    let mut policy = use_signal(|| inspector.permission_policy());
    let watched = inspector.clone();
    use_future(move || {
        let inspector = watched.clone();
        async move {
            let mut changes = inspector.permission_policy_changes();
            while let Some(now) = changes.next().await {
                policy.set(now);
            }
        }
    });
    use_context_provider(move || Services {
        inspector,
        draft,
        policy,
    });
}

/// What the live connection claimed in `initialize`, or what the next launch
/// will claim where none has gone out yet.
pub fn claim(inspector: &Inspector) -> ClientClaim {
    inspector.client_claim().unwrap_or_else(|| {
        let next = current();
        ClientClaim {
            capabilities: next.capabilities(),
            protocol_version: next.protocol(),
        }
    })
}

/// What the next launch will claim, or the default where nothing provided it.
pub fn current() -> ClientServices {
    try_consume_context::<Services>()
        .map(|services| services.draft.read().clone())
        .unwrap_or_default()
}

/// Records a new choice for the next launch.
pub fn change(value: ClientServices) {
    if let Some(mut services) = try_consume_context::<Services>() {
        services.inspector.set_client_services(value.clone());
        services.draft.set(value);
    }
}

/// The launch-time client services: which services `initialize` claims, how
/// other agent requests are answered, a rules file, and the protocol version.
///
/// Collapsed by default under one line that says what is claimed, because the
/// default — nothing claimed, version 1 — is what most launches want.
#[component]
pub fn ClientServicesControl(
    value: ClientServices,
    on_change: EventHandler<ClientServices>,
) -> Element {
    let mut problem = use_signal(|| None::<String>);
    let claimed: Vec<&str> = [
        (value.read_text_file, "read files"),
        (value.write_text_file, "write files"),
        (value.terminal, "terminal"),
    ]
    .into_iter()
    .filter_map(|(on, name)| on.then_some(name))
    .collect();
    let summary = if claimed.is_empty() {
        "none claimed".to_owned()
    } else {
        claimed.join(", ")
    };
    let toggle = |current: &ClientServices, change: fn(&mut ClientServices, bool)| {
        let current = current.clone();
        move |event: Event<FormData>| {
            let mut next = current.clone();
            change(&mut next, event.checked());
            on_change.call(next);
        }
    };
    let removing = value.clone();
    let versioning = value.clone();
    let loading = value.clone();

    rsx! {
        section { class: "client-services", "data-slot": "client-services",
            details { class: "client-services-disclosure",
                summary {
                    span { class: "client-services-summary", "Client services ({summary})" }
                    if value.protocol_version != 1 {
                        span { class: "hint", " · protocol version {value.protocol_version}" }
                    }
                }
                p { class: "hint",
                    "Claimed services are answered by you or by a rules file. Nothing reads or writes files or runs commands."
                }
                fieldset { class: "client-services-claims",
                    legend { class: "form-label", "Claim in initialize" }
                    label { class: "client-services-option",
                        input {
                            class: "checkbox checkbox-xs",
                            r#type: "checkbox",
                            "data-claim": "readTextFile",
                            checked: value.read_text_file,
                            onchange: toggle(&value, |next, on| next.read_text_file = on),
                        }
                        span { "Read text files" }
                        code { class: "hint mono", "fs/read_text_file" }
                    }
                    label { class: "client-services-option",
                        input {
                            class: "checkbox checkbox-xs",
                            r#type: "checkbox",
                            "data-claim": "writeTextFile",
                            checked: value.write_text_file,
                            onchange: toggle(&value, |next, on| next.write_text_file = on),
                        }
                        span { "Write text files" }
                        code { class: "hint mono", "fs/write_text_file" }
                    }
                    label { class: "client-services-option",
                        input {
                            class: "checkbox checkbox-xs",
                            r#type: "checkbox",
                            "data-claim": "terminal",
                            checked: value.terminal,
                            onchange: toggle(&value, |next, on| next.terminal = on),
                        }
                        span { "Terminal" }
                        code { class: "hint mono", "terminal/*" }
                    }
                }
                label { class: "client-services-option",
                    input {
                        class: "checkbox checkbox-xs",
                        r#type: "checkbox",
                        "data-slot": "answer-unserviced",
                        checked: value.answer_unserviced,
                        onchange: toggle(&value, |next, on| next.answer_unserviced = on),
                    }
                    span { "Answer other unhandled agent requests by hand" }
                }
                div { class: "form-field client-services-field", "data-slot": "rules-file",
                    span { class: "form-label", "Rules file" }
                    if let Some(script) = &value.script {
                        div { class: "client-services-rules",
                            span { "{script.source()} · {rules_counted(script)}" }
                            button {
                                class: "btn btn-xs btn-quiet",
                                r#type: "button",
                                onclick: move |_| {
                                    let mut next = removing.clone();
                                    next.script = None;
                                    on_change.call(next);
                                },
                                "Remove"
                            }
                        }
                    }
                    input {
                        r#type: "file",
                        accept: ".json,application/json",
                        aria_label: "Choose a rules file",
                        onchange: move |event| {
                            let files = event.files();
                            let loading = loading.clone();
                            spawn(async move {
                                let Some(file) = files.into_iter().next() else {
                                    return;
                                };
                                match read_rules(file).await {
                                    Ok(script) => {
                                        problem.set(None);
                                        let mut next = loading.clone();
                                        next.script = Some(script);
                                        on_change.call(next);
                                    }
                                    Err(why) => problem.set(Some(why)),
                                }
                            });
                        },
                    }
                    if let Some(why) = problem() {
                        p { class: "detail detail-warn", "data-slot": "rules-problem", role: "status",
                            "Couldn't use that rules file: {why}"
                        }
                    }
                    p { class: "hint",
                        "JSON. The first matching rule answers; see docs/client-services.md."
                    }
                }
                div { class: "form-field client-services-field", "data-slot": "protocol-version",
                    label { class: "form-label", r#for: "launch-protocol-version", "Protocol version" }
                    input {
                        id: "launch-protocol-version",
                        class: "input input-sm mono",
                        r#type: "number",
                        min: "0",
                        max: "65535",
                        value: "{value.protocol_version}",
                        onchange: move |event| {
                            if let Ok(version) = event.value().trim().parse::<u16>() {
                                let mut next = versioning.clone();
                                next.protocol_version = version;
                                on_change.call(next);
                            }
                        },
                    }
                    p { class: "hint", "Sent as protocolVersion in initialize. This inspector speaks version 1." }
                }
            }
        }
    }
}

fn rules_counted(script: &ServiceScript) -> String {
    match script.rules().len() {
        1 => "1 rule".to_owned(),
        count => format!("{count} rules"),
    }
}

/// Reads and parses a chosen rules file.
async fn read_rules(file: dioxus::html::FileData) -> Result<ServiceScript, String> {
    let name = file.name();
    #[cfg(not(target_arch = "wasm32"))]
    let bytes = if file
        .inner()
        .downcast_ref::<dioxus::html::SerializedFileData>()
        .is_some_and(|data| data.contents.is_some())
    {
        file.read_bytes().await.map_err(|e| e.to_string())?.to_vec()
    } else {
        let path = file.path();
        tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
            use std::io::Read;
            let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
            let mut bytes = Vec::new();
            file.take((RULES_LIMIT + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            Ok(bytes)
        })
        .await
        .map_err(|e| e.to_string())??
    };
    #[cfg(target_arch = "wasm32")]
    let bytes = file.read_bytes().await.map_err(|e| e.to_string())?.to_vec();
    if bytes.len() > RULES_LIMIT {
        return Err("it is over 1 MB.".into());
    }
    let text = String::from_utf8(bytes).map_err(|_| "it is not UTF-8 text.".to_owned())?;
    ServiceScript::parse(name, &text).map_err(|error| error.to_string())
}

/// The open session's permission policy, as a select in the timeline's header.
/// Nothing where the window provided no policy to set.
#[component]
pub fn PolicyControl() -> Element {
    let Some(services) = try_use_context::<Services>() else {
        return rsx! {};
    };
    let policy = (services.policy)();
    rsx! {
        PolicySelect {
            value: policy,
            on_change: move |chosen: PermissionPolicy| services.inspector.set_permission_policy(chosen),
        }
    }
}

/// The select itself, apart from where its value lives.
#[component]
pub fn PolicySelect(value: PermissionPolicy, on_change: EventHandler<PermissionPolicy>) -> Element {
    const CHOICES: [(PermissionPolicy, &str, &str); 3] = [
        (PermissionPolicy::Ask, "ask", "Ask me"),
        (
            PermissionPolicy::AllowOnce,
            "allow-once",
            "Allow once automatically",
        ),
        (
            PermissionPolicy::RejectOnce,
            "reject-once",
            "Reject once automatically",
        ),
    ];
    rsx! {
        label { class: "permission-policy", "data-slot": "permission-policy",
            title: "Applies to permission requests that arrive from now on in this session. Never picks an always option. Goes back to Ask me when the session changes or the agent disconnects.",
            span { class: "sr-only", "Permission requests" }
            select {
                class: "select select-xs",
                aria_label: "Permission requests",
                onchange: move |event| {
                    if let Some((chosen, _, _)) =
                        CHOICES.iter().find(|(_, key, _)| *key == event.value())
                    {
                        on_change.call(*chosen);
                    }
                },
                for (choice, key, label) in CHOICES {
                    option { key: "{key}", value: key, selected: choice == value, "{label}" }
                }
            }
        }
    }
}

/// How a service request reads in the timeline: its method and params, where
/// it stands, and — while the agent waits — the response editor.
pub fn card(request: &ServiceRequest) -> Element {
    rsx! {
        ServiceCard { state: request.state(), request: request.clone() }
    }
}

/// Which half of the editor is open.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Result,
    Error,
}

#[component]
fn ServiceCard(request: ServiceRequest, state: ServiceState) -> Element {
    let waiting = state == ServiceState::Waiting;
    let params = pretty(request.params());

    rsx! {
        div {
            class: if waiting { "evidence blocking" } else { "evidence blocking answered" },
            "data-slot": "service",
            "data-method": "{request.method()}",
            // Everything in the card is about answering, not a press of the row.
            onclick: move |event| event.stop_propagation(),
            div { class: "blocking-head",
                span { class: "wire-name", "{request.method()}" }
                span { class: "blocking-wait",
                    if waiting { "Waiting for you" } else { "Resolved" }
                }
            }
            div { class: "blocking-body",
                span { class: "blocking-meta", "{request.id()}" }
                if !request.claimed() {
                    p { class: "hint", "data-slot": "unclaimed", "Not claimed in initialize." }
                }
                pre { class: "raw", "data-slot": "service-params", "{params}" }
                {stands(&state)}
                if waiting {
                    Editor { request: request.clone() }
                }
            }
        }
    }
}

/// The response editor: a result prefilled with a valid skeleton for the
/// method, or an error with a code and a message.
#[component]
fn Editor(request: ServiceRequest) -> Element {
    let mut mode = use_signal(|| Mode::Result);
    let mut result = use_signal(|| pretty(&request.skeleton()));
    let mut code = use_signal(|| i32::from(v1::ErrorCode::InternalError).to_string());
    let mut message = use_signal(String::new);
    let mut problem = use_signal(|| None::<String>);
    let id = format!("{}", request.id()).replace('"', "");

    let send = move |_| {
        let request = request.clone();
        let reply = match mode() {
            Mode::Result => match acp_inspector_core::read_json(&result()) {
                Ok(value) => ServiceReply::Result(value),
                Err(why) => {
                    problem.set(Some(format!("Not valid JSON: {why}")));
                    return;
                }
            },
            Mode::Error => match code().trim().parse::<i32>() {
                Ok(code) => ServiceReply::Error(v1::Error::new(code, message())),
                Err(_) => {
                    problem.set(Some("The error code must be a whole number.".into()));
                    return;
                }
            },
        };
        problem.set(None);
        spawn(async move {
            let _ = match reply {
                ServiceReply::Result(value) => request.respond(value).await,
                ServiceReply::Error(error) => request.refuse(error).await,
            };
        });
    };

    rsx! {
        div { class: "service-editor", "data-slot": "service-editor",
            div { class: "setting-values", role: "group", aria_label: "Answer with",
                button {
                    class: "chip",
                    r#type: "button",
                    aria_pressed: mode() == Mode::Result,
                    onclick: move |_| mode.set(Mode::Result),
                    "Result"
                }
                button {
                    class: "chip",
                    r#type: "button",
                    aria_pressed: mode() == Mode::Error,
                    onclick: move |_| mode.set(Mode::Error),
                    "Error"
                }
            }
            if mode() == Mode::Result {
                textarea {
                    class: "textarea mono",
                    "data-slot": "service-result",
                    aria_label: "Result JSON",
                    rows: 4,
                    spellcheck: "false",
                    value: "{result}",
                    oninput: move |event| result.set(event.value()),
                }
            } else {
                div { class: "service-error",
                    label { r#for: "service-code-{id}", class: "form-label", "Code" }
                    input {
                        id: "service-code-{id}",
                        class: "input input-sm mono",
                        "data-slot": "service-code",
                        value: "{code}",
                        oninput: move |event| code.set(event.value()),
                    }
                    label { r#for: "service-message-{id}", class: "form-label", "Message" }
                    input {
                        id: "service-message-{id}",
                        class: "input input-sm",
                        "data-slot": "service-message",
                        value: "{message}",
                        oninput: move |event| message.set(event.value()),
                    }
                }
            }
            if let Some(why) = problem() {
                p { class: "detail detail-warn", role: "status", "{why}" }
            }
            div { class: "answers",
                button {
                    class: "btn btn-sm btn-filled",
                    r#type: "button",
                    "data-slot": "service-send",
                    onclick: send,
                    "Send"
                }
            }
        }
    }
}

/// Where a service request stands, in a line, and what was sent.
fn stands(state: &ServiceState) -> Element {
    match state {
        ServiceState::Waiting => rsx! {
            p { class: "detail detail-live", "The agent is waiting for your answer." }
        },
        ServiceState::Answering => rsx! { p { class: "detail", "Sending…" } },
        ServiceState::Answered(answer) => {
            let who = match answer.by {
                AnsweredBy::Script { rule } => {
                    format!("Answered by rule {} of the rules file", rule + 1)
                }
                _ => "Sent".to_owned(),
            };
            let scripted = matches!(answer.by, AnsweredBy::Script { .. });
            rsx! {
                p { class: "detail", "data-slot": if scripted { "scripted" } else { "answered" },
                    {match &answer.reply {
                        ServiceReply::Result(_) => rsx! { "{who} with a result." },
                        ServiceReply::Error(error) => rsx! {
                            "{who} with error "
                            code { "{i32::from(error.code)}" }
                            if !error.message.is_empty() { ": {error.message}" }
                            "."
                        },
                    }}
                }
                if let ServiceReply::Result(result) = &answer.reply {
                    pre { class: "raw", "data-slot": "service-sent", "{pretty(result)}" }
                }
            }
        }
        ServiceState::Cancelled => rsx! {
            p { class: "detail detail-warn", "Turn cancelled. Answered with a cancellation error." }
        },
        ServiceState::Abandoned => rsx! {
            p { class: "detail detail-warn",
                "Can no longer be answered: the turn ended or the agent disconnected."
            }
        },
        ServiceState::CancelledByAgent => rsx! {
            p { class: "detail detail-warn", "data-slot": "withdrawn", "The agent withdrew this request." }
        },
        _ => rsx! { p { class: "detail", "A state this window has no rendering for." } },
    }
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use acp_inspector_core::{AgentCommand, EntryKind};

    use super::*;

    /// An inspector whose scripted agent has asked `frame`, launched with
    /// `services`.
    fn asked(services: ClientServices, frame: &str) -> Inspector {
        let inspector = Inspector::new();
        inspector.set_client_services(services);
        inspector.connect(
            &AgentCommand {
                command: "sh".into(),
                args: format!("-c\nprintf '%s\\n' '{frame}'; cat >/dev/null"),
                ..AgentCommand::default()
            }
            .factory(),
        );
        inspector
    }

    async fn service_entry(inspector: &Inspector) -> ServiceRequest {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Some(request) =
                    inspector
                        .timeline()
                        .entries()
                        .into_iter()
                        .find_map(|entry| match entry.kind {
                            EntryKind::Service(request) => Some(request),
                            _ => None,
                        })
                    && !matches!(request.state(), ServiceState::Answering)
                {
                    break request;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("the agent asks")
    }

    fn drawn(request: ServiceRequest) -> String {
        #[component]
        fn Host(request: ServiceRequest) -> Element {
            card(&request)
        }
        let mut dom = VirtualDom::new_with_props(Host, HostProps { request });
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    const READ: &str = r#"{"jsonrpc":"2.0","id":41,"method":"fs/read_text_file","params":{"sessionId":"s1","path":"/work/a.rs"}}"#;

    #[tokio::test]
    async fn a_waiting_service_request_is_a_card_with_its_params_and_an_editor() {
        let inspector = asked(
            ClientServices {
                read_text_file: true,
                ..ClientServices::default()
            },
            READ,
        );
        let html = drawn(service_entry(&inspector).await);

        assert!(html.contains("fs/read_text_file"), "{html}");
        assert!(html.contains("/work/a.rs"), "the params are shown: {html}");
        assert!(html.contains("Waiting for you"), "{html}");
        assert!(
            html.contains("&#34;content&#34;: &#34;&#34;"),
            "the editor starts from a valid result: {html}"
        );
        assert!(html.contains(r#"data-slot="service-send""#), "{html}");
        assert!(
            html.contains(">Result<") && html.contains(">Error<"),
            "{html}"
        );
        assert!(!html.contains("Not claimed"), "it was claimed: {html}");
    }

    #[tokio::test]
    async fn an_opted_in_request_says_it_was_not_claimed() {
        let inspector = asked(
            ClientServices {
                answer_unserviced: true,
                ..ClientServices::default()
            },
            r#"{"jsonrpc":"2.0","id":7,"method":"_vendor/ping","params":{}}"#,
        );
        let html = drawn(service_entry(&inspector).await);
        assert!(html.contains("_vendor/ping"), "{html}");
        assert!(html.contains("Not claimed in initialize."), "{html}");
    }

    #[tokio::test]
    async fn a_scripted_answer_says_which_rule_answered() {
        let script = ServiceScript::parse(
            "rules.json",
            r#"{"rules":[{"method":"fs/read_text_file","path":"/work/*","result":{"content":"canned"}}]}"#,
        )
        .unwrap();
        let inspector = asked(
            ClientServices {
                read_text_file: true,
                script: Some(script),
                ..ClientServices::default()
            },
            READ,
        );
        let html = drawn(service_entry(&inspector).await);
        assert!(
            html.contains("Answered by rule 1 of the rules file with a result."),
            "{html}"
        );
        assert!(html.contains("canned"), "what was sent is shown: {html}");
        assert!(
            !html.contains(r#"data-slot="service-send""#),
            "no editor: {html}"
        );
    }

    fn rendered(app: fn() -> Element) -> String {
        let mut dom = VirtualDom::new(app);
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    #[test]
    fn the_launch_control_shows_what_is_claimed_and_the_version() {
        fn app() -> Element {
            rsx! {
                ClientServicesControl {
                    value: ClientServices {
                        read_text_file: true,
                        terminal: true,
                        protocol_version: 2,
                        ..ClientServices::default()
                    },
                    on_change: move |_| {},
                }
            }
        }
        let html = rendered(app);
        assert!(
            html.contains("Client services (read files, terminal)"),
            "{html}"
        );
        assert!(html.contains("protocol version 2"), "{html}");
        assert!(html.contains(r#"data-claim="readTextFile""#), "{html}");
        assert!(
            html.contains("Answer other unhandled agent requests by hand"),
            "{html}"
        );
        assert!(
            html.contains("Nothing reads or writes files or runs commands."),
            "{html}"
        );
    }

    #[test]
    fn the_default_launch_claims_nothing() {
        fn app() -> Element {
            rsx! { ClientServicesControl { value: ClientServices::default(), on_change: move |_| {} } }
        }
        let html = rendered(app);
        assert!(html.contains("Client services (none claimed)"), "{html}");
        assert!(!html.contains("protocol version"), "{html}");
    }

    #[test]
    fn the_policy_select_offers_three_answers_and_marks_the_chosen_one() {
        fn app() -> Element {
            rsx! { PolicySelect { value: PermissionPolicy::AllowOnce, on_change: move |_| {} } }
        }
        let html = rendered(app);
        for label in [
            "Ask me",
            "Allow once automatically",
            "Reject once automatically",
        ] {
            assert!(html.contains(label), "{label}: {html}");
        }
        assert!(html.contains("Never picks an always option."), "{html}");
        assert!(
            html.contains(r#"value="allow-once" selected"#)
                || html.contains(r#"selected=true value="allow-once""#)
                || html.contains(r#"selected="true""#),
            "{html}"
        );
    }

    #[test]
    fn every_service_state_has_a_line() {
        let states = [
            (ServiceState::Waiting, "waiting for your answer"),
            (ServiceState::Cancelled, "Turn cancelled."),
            (ServiceState::Abandoned, "Can no longer be answered"),
            (
                ServiceState::CancelledByAgent,
                "The agent withdrew this request.",
            ),
            (
                ServiceState::Answered(acp_inspector_core::ServiceAnswer {
                    reply: ServiceReply::Result(serde_json::json!({"content": "x"})),
                    by: AnsweredBy::Script { rule: 1 },
                }),
                "Answered by rule 2 of the rules file with a result.",
            ),
            (
                ServiceState::Answered(acp_inspector_core::ServiceAnswer {
                    reply: ServiceReply::Error(v1::Error::new(-32002, "Resource not found")),
                    by: AnsweredBy::Reader,
                }),
                "Sent with error <code>-32002</code>: Resource not found.",
            ),
        ];
        for (state, said) in states {
            let html = dioxus_ssr::render_element(stands(&state));
            assert!(html.contains(said), "{said}: {html}");
        }
    }
}
