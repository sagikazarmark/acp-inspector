//! Inline Blocking Requests: the Agent's form or URL, and the reader's answer.
//! Form findings are advisory; Raw accepts any JSON object or no content (§7.8).

use acp_inspector_core::{ElicitationAnswer, ElicitationRequest, ElicitationState, v1};
use dioxus::prelude::*;
use serde_json::{Map, Value};

const FOCUS_TOOL_CALL: &str = r#"
const id = await dioxus.recv();
await new Promise((resolve) => requestAnimationFrame(resolve));
const target = document.querySelector(`[data-tool-call="${CSS.escape(id)}"]`);
if (target) {
  target.scrollIntoView({ block: "center" });
  target.focus({ preventScroll: true });
}
"#;

// Resolved fields stay focusable by summary links. Read-only text and capture
// guards freeze select/checkbox/presence interactions without hiding evidence.
const FREEZE_FORM: &str = r#"
const id = await dioxus.recv();
await new Promise(resolve => requestAnimationFrame(resolve));
const root = document.getElementById(id);
if (!root) return;
const freeze = () => {
if (root.dataset.resolved !== 'true') return;
for (const control of root.querySelectorAll('input,select,textarea,button')) {
  if (control.closest('[data-finding-summary]')) continue;
  if (control.dataset.frozen) continue;
  control.dataset.frozen = 'true';
  control.tabIndex = -1;
  control.setAttribute('aria-disabled', 'true');
  if (control.tagName === 'INPUT' || control.tagName === 'TEXTAREA') control.readOnly = true;
  const value = control.value, checked = control.checked;
  for (const kind of ['beforeinput', 'input', 'change', 'click', 'keydown']) {
    control.addEventListener(kind, event => {
      if (kind === 'keydown' && event.key === 'Tab') return;
      event.preventDefault();
      event.stopImmediatePropagation();
      if (kind === 'input' || kind === 'change') { control.value = value; control.checked = checked; }
    }, true);
  }
}
};
const observer = new MutationObserver(freeze);
observer.observe(root, {attributes:true, attributeFilter:['data-resolved'], childList:true, subtree:true});
freeze();
"#;

#[derive(Clone, PartialEq)]
pub struct Answer {
    pub request: ElicitationRequest,
    pub reply: Reply,
}

#[derive(Clone, PartialEq)]
pub enum Reply {
    Accept(Option<Map<String, Value>>),
    Decline,
    Cancel,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Surface {
    Form,
    Raw,
}

impl Surface {
    fn label(self) -> &'static str {
        match self {
            Self::Form => "Form",
            Self::Raw => "Raw",
        }
    }
}

/// Independent drafts: only a changed engine value replaces the Raw draft.
/// Switching tabs or revealing findings must never erase hand-written content.
#[derive(Clone, PartialEq, Debug)]
struct Answering {
    filled: Map<String, Value>,
    written: String,
    shown: Surface,
}

impl Answering {
    fn new() -> Self {
        Self {
            filled: Map::new(),
            written: "{}".into(),
            shown: Surface::Form,
        }
    }

    fn raw(&mut self, text: String) {
        self.written = text;
        self.shown = Surface::Raw;
    }

    fn synchronize(&mut self, data: Map<String, Value>) {
        if self.filled != data {
            self.filled = data;
            self.written = pretty(&self.filled);
        }
    }

    fn content(&self) -> Result<Option<Map<String, Value>>, String> {
        match self.shown {
            Surface::Form => Ok(Some(self.filled.clone()).filter(|data| !data.is_empty())),
            Surface::Raw if self.written.trim().is_empty() => Ok(None),
            Surface::Raw => crate::elicitation_schema::raw_content(&self.written).map(Some),
        }
    }
}

/// State snapshots are props because the request handle compares by identity:
/// the same handle must still redraw when answered or completed.
#[component]
pub fn Panel(
    request: ElicitationRequest,
    state: ElicitationState,
    completed: bool,
    on_answer: EventHandler<Answer>,
) -> Element {
    let mut answering = use_signal(Answering::new);
    let engine = use_hook(|| {
        std::rc::Rc::new(std::cell::RefCell::new(
            None::<schemaform_dioxus::FormHandle>,
        ))
    });
    let mut problem = use_signal(|| None::<String>);
    // How many findings the form is showing, reported up by the fields so
    // Accept can say it sends anyway.
    let mut findings = use_signal(|| 0_usize);
    let id = use_hook(|| {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        format!(
            "elicitation-{}",
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )
    });
    let waiting = state == ElicitationState::Waiting;
    let shown = answering.read().shown;
    let sending = answering.read().content();
    let unusable = sending.clone().err();
    let available = waiting && unusable.is_none();
    let accept_label = match findings() {
        count if waiting && shown == Surface::Form && request.form().is_some() && count > 0 => {
            format!(
                "Accept anyway ({count} finding{})",
                if count == 1 { "" } else { "s" }
            )
        }
        _ => "Accept".to_owned(),
    };
    let accept = {
        let request = request.clone();
        let engine = engine.clone();
        move |_| {
            if !available {
                return;
            }
            let content = if shown == Surface::Form && request.form().is_some() {
                let Some(form) = engine.borrow().clone() else {
                    problem.set(Some("The form is not ready. Use Raw to answer.".into()));
                    return;
                };
                match form.prepare_advisory_submission() {
                    Ok(prepared) => advisory_content(prepared.submission()),
                    Err(error) => {
                        problem.set(Some(error.to_string()));
                        return;
                    }
                }
            } else {
                sending.clone().unwrap_or_default()
            };
            problem.set(None);
            on_answer.call(Answer {
                request: request.clone(),
                reply: Reply::Accept(content),
            });
        }
    };
    let reply = {
        let request = request.clone();
        move |reply: Reply| {
            let request = request.clone();
            move |_| {
                if waiting {
                    on_answer.call(Answer {
                        request: request.clone(),
                        reply: reply.clone(),
                    });
                }
            }
        }
    };
    rsx! {
        div {
            class: if waiting { "evidence blocking elicit" } else { "evidence blocking elicit answered" },
            "data-slot": "elicitation",
            // Controls answer the request; they do not activate the enclosing
            // Timeline row's Frame-navigation handler and steal focus.
            onclick: move |event| event.stop_propagation(),
            div { class: "blocking-head",
                span { class: "wire-name", "{v1::CLIENT_METHOD_NAMES.elicitation_create}",
                    if request.form().is_some() { " · form" } else { " · url" }
                }
                span { class: "blocking-wait", if waiting { "Waiting for you" } else { "Resolved" } }
            }
            div { class: "blocking-body",
                p { class: "blocking-title", "{request.message()}" }
                span { class: "blocking-meta", "{request.id()}" }
                {scope(&request)}
                if request.form().is_some() {
                    div { class: "elicitation-form", "data-slot": "form",
                        {tabs(&id, shown, {
                            let engine = engine.clone();
                            move |chosen| {
                                let mut state = answering.write();
                                if let Some(form) = engine.borrow().as_ref()
                                    && let Ok(Value::Object(data)) = form.reader().form_data() {
                                    state.synchronize(data);
                                }
                                state.shown = chosen;
                            }
                        })}
                        // Keep the engine mounted so edits, touched state and
                        // findings survive a visit to Raw. Enter in a field
                        // never answers: Accept is the only way to send.
                        div {
                            id: "{id}-panel-{Surface::Form.label()}",
                            role: "tabpanel",
                            aria_labelledby: "{id}-tab-{Surface::Form.label()}",
                            hidden: shown != Surface::Form,
                            if let Some(raw_schema) = request.raw_form() {
                                EngineFields {
                                    raw_schema: raw_schema.to_owned(), waiting,
                                    on_ready: move |form| *engine.borrow_mut() = Some(form),
                                    on_problem: move |error| problem.set(Some(error)),
                                    on_findings: move |count| findings.set(count),
                                }
                            }
                        }
                        div {
                            id: "{id}-panel-{Surface::Raw.label()}",
                            role: "tabpanel",
                            aria_labelledby: "{id}-tab-{Surface::Raw.label()}",
                            hidden: shown != Surface::Raw,
                            if shown == Surface::Raw {
                                textarea { class: "textarea", "data-slot": "raw-content",
                                    aria_label: "Content to send", rows: 8,
                                    value: "{answering.read().written}", readonly: !waiting,
                                    oninput: move |event| { if waiting { answering.write().raw(event.value()); } },
                                }
                            }
                        }
                        p { class: "hint", "data-slot": "sends", "Accept sends what the {shown.label()} tab holds." }
                        if let Some(error) = unusable { p { class: "detail detail-warn", "data-slot": "unusable", "{error}" } }
                        if let Some(error) = problem.read().clone() { p { class: "detail detail-warn", role: "status", "{error}" } }
                    }
                } else if let Some(url) = request.url() { {target(url, completed, &state)} }
                else { p { class: "unnamed", "A mode this window has no rendering for. What arrived is below, as it arrived." } }
                {said(&state, completed, request.url().is_some())}
                div { class: "answers",
                    {answer("accept", accept_label, "btn-filled", available, accept, &state)}
                    {answer("decline", "Decline", "btn-quiet btn-bad", waiting, reply(Reply::Decline), &state)}
                    {answer("cancel", "Cancel", "btn-quiet", waiting, reply(Reply::Cancel), &state)}
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct InspectorFormShell;
impl schemaform_dioxus::ShellRenderer for InspectorFormShell {
    fn shell(&self, context: schemaform_dioxus::ShellContext) -> Element {
        rsx! { AdvisoryBody { heading_id: context.heading_id, label: context.presentation.label,
            help: context.presentation.help.map(|help| (help.id, help.text)), summary: context.summary, body: context.body,
        } }
    }
}

#[component]
fn AdvisoryBody(
    heading_id: Option<String>,
    label: String,
    help: Option<(String, String)>,
    summary: Element,
    body: Element,
) -> Element {
    let advisory = use_signal(|| true);
    use_context_provider(|| {
        crate::components::schemaform_daisyui::AdvisoryPresentation(advisory.into())
    });
    rsx! {
        if let Some(id) = heading_id { strong { id, class: "form-title", "{label}" } }
        if let Some((id, text)) = help { p { id, class: "hint", "{text}" } }
        {summary} {body}
    }
}

#[component]
fn EngineFields(
    raw_schema: String,
    waiting: bool,
    on_ready: EventHandler<schemaform_dioxus::FormHandle>,
    on_problem: EventHandler<String>,
    #[props(default)] on_findings: EventHandler<usize>,
) -> Element {
    let prepared =
        use_hook(|| crate::elicitation_schema::prepare(&raw_schema).map(std::rc::Rc::new));
    match prepared {
        Ok(prepared) => rsx! {
            BoundFields { prepared: PreparedForm(prepared.clone()), waiting, on_ready, on_problem, on_findings }
            for (name, schema) in &prepared.unrendered {
                div { class: "field", "data-field": "{name}", "data-slot": "unrenderable",
                    "data-required": prepared.required_unrendered.contains(name).to_string(),
                    strong { "{name}" }
                    // The engine is not told this one is required — it cannot
                    // draw it — so the mark is the host's to keep.
                    if prepared.required_unrendered.contains(name) {
                        span { class: "required-mark", "data-slot": "required", " Required" }
                    }
                    p { class: "hint", "No control for this property type. Use Raw to fill it." }
                    pre { class: "raw", "{schema}" }
                }
            }
        },
        Err(error) => form_problem(&error),
    }
}

#[component]
fn BoundFields(
    prepared: PreparedForm,
    waiting: bool,
    on_ready: EventHandler<schemaform_dioxus::FormHandle>,
    on_problem: EventHandler<String>,
    on_findings: EventHandler<usize>,
) -> Element {
    use crate::components::schemaform_daisyui as daisy;
    use schemaform_dioxus::{
        RenderConfiguration, SchemaForm, SubmissionMode, use_form_with_defaults,
    };
    let definition = match prepared.0.definition.clone() {
        Ok(definition) => definition,
        Err(error) => return form_problem(&error),
    };
    let form = use_form_with_defaults(definition, serde_json::json!({}));
    let form = match form {
        Ok(form) => form,
        Err(error) => return form_problem(&error.to_string()),
    };
    let booleans = prepared.0.booleans.clone();
    let bound = use_hook(|| {
        RenderConfiguration::builder()
            .localizer(std::sync::Arc::new(
                crate::elicitation_words::FindingWords { booleans },
            ))
            .controls(daisy::controls_with_density(
                daisy::Appearance::None,
                daisy::Density::Compact,
            ))
            .structure(
                daisy::structure_with(daisy::Appearance::None).with_shell(InspectorFormShell),
            )
            .summary_presenter(daisy::findings_with(daisy::Appearance::None))
            .local_presenter(daisy::findings_with(daisy::Appearance::None))
            .build()
            .bind(&form)
            .map_err(|error| error.to_string())
    });
    let bound = match bound {
        Ok(bound) => bound,
        Err(error) => return form_problem(&error),
    };
    use_hook({
        let form = form.clone();
        move || on_ready.call(form)
    });
    let id = use_hook(|| {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        format!(
            "elicitation-fields-{}",
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )
    });
    let freezing_id = id.clone();
    // The findings on screen, read so this re-renders as they change, and
    // reported up for Accept's label.
    let visible = form
        .reader()
        .read()
        .map(|projection| {
            projection
                .findings
                .iter()
                .filter(|finding| {
                    matches!(
                        finding,
                        schemaform_dioxus::FindingProjection::Validation { .. }
                            | schemaform_dioxus::FindingProjection::Parse { .. }
                    )
                })
                .count()
        })
        .unwrap_or(0);
    use_effect(use_reactive!(|visible| on_findings.call(visible)));
    rsx! {
        div { id, class: "elicitation-fields", "data-resolved": (!waiting).to_string(),
            onmounted: move |_| { let freezing = document::eval(FREEZE_FORM); let _ = freezing.send(freezing_id.clone()); },
            SchemaForm { form: bound, submission_mode: SubmissionMode::Advisory,
                on_submit: |_| {},
                // Enter in a single-field form submits it natively. That is
                // not an answer: Accept is the one way to send.
                on_advisory_submit: |_| {},
                on_error: move |error: schemaform_dioxus::HandleError| on_problem.call(error.to_string()),
            }
        }
    }
}

fn advisory_content(submission: &schemaform::AdvisorySubmission) -> Option<Map<String, Value>> {
    submission
        .form_data()
        .as_object()
        .cloned()
        .filter(|data| !data.is_empty())
}

fn form_problem(problem: &str) -> Element {
    rsx! { p { class: "detail detail-warn", "The form could not be prepared: {problem}. Use Raw to answer." } }
}

#[derive(Clone)]
struct PreparedForm(std::rc::Rc<crate::elicitation_schema::Prepared>);
impl PartialEq for PreparedForm {
    fn eq(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.0, &other.0)
    }
}

fn scope(request: &ElicitationRequest) -> Element {
    rsx! {
        p { class: "scope hint", "data-slot": "scope",
            if let Some(session) = request.session_id() { "In session " code { class: "mono", "{session}" } }
            else if let Some(call) = request.request_scope() { "Outside any session, tied to request " code { class: "mono", "{call}" } }
            else { "A scope this window has no name for." }
            if let Some(id) = request.tool_call_id().map(ToString::to_string) {
                ", raised by tool call " code { class: "mono", "{id}" }
                button { class: "btn btn-ghost btn-xs reach", "data-slot": "reach-tool-call",
                    title: "Move focus to the tool call this question came from",
                    onclick: move |_| { let focusing = document::eval(FOCUS_TOOL_CALL); let _ = focusing.send(id.clone()); },
                    "Go to tool call"
                }
            }
        }
    }
}

/// The Form/Raw switch, as a tablist whose tabs control the two panels in
/// [`Panel`]. One Tab stop: the arrows, Home and End move between the tabs and
/// select as they go.
fn tabs(id: &str, surface: Surface, on_choose: impl FnMut(Surface) + Clone + 'static) -> Element {
    const ORDER: [Surface; 2] = [Surface::Form, Surface::Raw];
    rsx! { div { class: "form-tabs seg", role: "tablist", aria_label: "How to answer",
        for choice in ORDER {
            button { key: "{choice.label()}", class: "seg-item seg-word", r#type: "button", role: "tab",
                id: "{id}-tab-{choice.label()}",
                aria_controls: "{id}-panel-{choice.label()}",
                tabindex: if choice == surface { "0" } else { "-1" },
                "data-slot": "surface", "data-surface": "{choice.label()}", aria_selected: choice == surface,
                onclick: { let mut on_choose = on_choose.clone(); move |_| on_choose(choice) },
                onkeydown: {
                    let mut on_choose = on_choose.clone();
                    let id = id.to_owned();
                    move |event: Event<KeyboardData>| {
                        let at = ORDER.iter().position(|each| *each == choice).unwrap_or(0);
                        let Some(next) = crate::console::step(ORDER.len(), at, &event.key(), crate::console::Axis::Horizontal) else {
                            return;
                        };
                        event.prevent_default();
                        let next = ORDER[next];
                        on_choose(next);
                        crate::console::focus_id(&format!("{id}-tab-{}", next.label()));
                    }
                },
                "{choice.label()}"
            }
        }
    } }
}

/// Where a URL-mode request wants the reader to go.
///
/// Only an http(s) URL with a host is a link. The text is parsed, and the host
/// shown is the one the browser would open (so a `user@host` spoof says so).
/// Anything else is text. A request that can no longer be acted on stops being
/// a link; an accepted, uncompleted one keeps it, since accepting consented to
/// the visit.
fn target(url: &str, completed: bool, state: &ElicitationState) -> Element {
    let live = match state {
        ElicitationState::Waiting | ElicitationState::Answering => true,
        ElicitationState::Answered(ElicitationAnswer::Accepted(_)) => !completed,
        _ => false,
    };
    let parsed = Destination::of(url);
    rsx! { div { class: "elicitation-url", "data-slot": "url",
        match &parsed {
            Destination::Web { host, credentials, .. } => rsx! {
                p { class: "hint", "It wants you to visit " strong { class: "host", "{host}" } }
                if *credentials {
                    p { class: "detail detail-warn", "data-slot": "url-credentials",
                        "This link has a user name or password before the host. The page it opens is on "
                        strong { "{host}" }
                        ", whatever the text before the "
                        code { "@" }
                        " says."
                    }
                }
            },
            Destination::Inert => rsx! {
                p { class: "detail detail-warn", "data-slot": "url-inert",
                    "Not a web link (only http and https are opened), so it is shown as text."
                }
            },
        }
        if let (Destination::Web { href, host, .. }, true) = (&parsed, live) {
            a { class: "url-target", "data-slot": "url-target", href: "{href}", title: "Open {host} in your browser", "{url}" }
            p { class: "hint", "Opens in your browser. This window never loads or fetches it." }
        } else {
            code { class: "url-target mono", "data-slot": "url-text", "{url}" }
            if matches!(parsed, Destination::Web { .. }) {
                p { class: "hint", "data-slot": "url-closed", "No longer a link: this request is over." }
            }
        }
        if completed { p { class: "detail", "data-slot": "completed", "The agent says the interaction finished." } }
    } }
}

/// What a URL-mode request's text turned out to be.
#[derive(Debug, PartialEq)]
enum Destination {
    /// An http(s) URL with a host: the normalized link, the host the browser
    /// would open, and whether a user name or password sits in front of it.
    Web {
        href: String,
        host: String,
        credentials: bool,
    },
    /// Anything else, which is never made a link.
    Inert,
}

impl Destination {
    fn of(text: &str) -> Self {
        let Ok(url) = url::Url::parse(text.trim()) else {
            return Self::Inert;
        };
        if !matches!(url.scheme(), "http" | "https") {
            return Self::Inert;
        }
        let Some(host) = url.host_str().filter(|host| !host.is_empty()) else {
            return Self::Inert;
        };
        Self::Web {
            host: host.to_owned(),
            credentials: !url.username().is_empty() || url.password().is_some(),
            href: url.as_str().to_owned(),
        }
    }
}

fn said(state: &ElicitationState, completed: bool, url: bool) -> Element {
    match state {
        ElicitationState::Waiting => {
            rsx! { p { class: "detail detail-live", "The agent is waiting for your answer." } }
        }
        ElicitationState::Answering => rsx! { p { class: "detail", "Sending…" } },
        ElicitationState::Answered(ElicitationAnswer::Accepted(content)) => rsx! {
            p { class: "detail", "Sent " code { "accept" }
                if let Some(content) = content { " with {content.len()} fields" } else { " with no content" }
                "."
                if url && !completed { " Accepting is consent, not completion: the agent has not said the interaction finished." }
            }
        },
        ElicitationState::Answered(ElicitationAnswer::Declined) => {
            rsx! { p { class: "detail", "Sent " code { "decline" } "." } }
        }
        ElicitationState::Answered(ElicitationAnswer::Cancelled) => {
            rsx! { p { class: "detail detail-warn", "Sent " code { "cancel" } "." } }
        }
        ElicitationState::Abandoned => {
            rsx! { p { class: "detail detail-warn", "Can no longer be answered: the turn ended or the agent disconnected." } }
        }
        ElicitationState::CancelledByAgent => {
            rsx! { p { class: "detail detail-warn", "data-slot": "withdrawn", "The agent withdrew this request." } }
        }
        _ => rsx! { p { class: "detail", "A state this window has no rendering for." } },
    }
}

fn answer(
    slot: &'static str,
    label: impl std::fmt::Display,
    variant: &'static str,
    available: bool,
    on_click: impl FnMut(Event<MouseData>) + 'static,
    state: &ElicitationState,
) -> Element {
    let chosen = matches!(
        (slot, state),
        (
            "accept",
            ElicitationState::Answered(ElicitationAnswer::Accepted(_))
        ) | (
            "decline",
            ElicitationState::Answered(ElicitationAnswer::Declined)
        ) | (
            "cancel",
            ElicitationState::Answered(ElicitationAnswer::Cancelled)
        )
    );
    rsx! { button { key: "{slot}", class: if chosen { format!("answer btn btn-sm btn-filled picked {variant}") } else { format!("answer btn btn-sm {variant}") },
        "data-answer": slot, "data-state": if chosen { "chosen" } else if available { "waiting" } else { "unavailable" },
        aria_pressed: chosen, aria_disabled: (!available).then_some("true"), tabindex: (!available).then_some("-1"),
        onclick: { let mut on_click = on_click; move |event| { if available { on_click(event); } } }, "{label}"
    } }
}

fn pretty(content: &Map<String, Value>) -> String {
    serde_json::to_string_pretty(content).expect("JSON values serialize")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn a_real_request_answers_once_and_redraws_without_replacing_accept() {
        use acp_inspector_core::{AgentCommand, Inspector};
        use dioxus::core::{Mutation, NoOpMutations};
        use dioxus::html::{PlatformEventData, SerializedMouseData};
        use std::{any::Any, cell::RefCell, rc::Rc, time::Duration};
        let inspector = Inspector::new();
        let frame = r#"{"jsonrpc":"2.0","id":"form","method":"elicitation/create","params":{"sessionId":"s","mode":"form","message":"Answer me","requestedSchema":{"type":"object","properties":{"age":{"type":"integer","default":999,"maximum":120}}}}}"#;
        inspector.connect(
            &AgentCommand {
                command: "sh".into(),
                args: format!("-c\nprintf '%s\\n' '{frame}'; cat >/dev/null"),
                ..AgentCommand::default()
            }
            .factory(),
        );
        let request = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(request) = inspector.pending_elicitations().first() {
                    break request.clone();
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let replies = Rc::new(RefCell::new(Vec::new()));
        #[derive(Clone, Props)]
        struct HostProps {
            request: ElicitationRequest,
            replies: Rc<RefCell<Vec<Answer>>>,
        }
        impl PartialEq for HostProps {
            fn eq(&self, _: &Self) -> bool {
                false
            }
        }
        fn host(props: HostProps) -> Element {
            rsx! { Panel { state: props.request.state(), completed: props.request.completed(), request: props.request,
                on_answer: move |answer| props.replies.borrow_mut().push(answer),
            } }
        }
        set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let mut dom = VirtualDom::new_with_props(
            host,
            HostProps {
                request: request.clone(),
                replies: replies.clone(),
            },
        );
        let edits = dom.rebuild_to_vec().edits;
        let accept = edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::SetAttribute {
                    name: "data-answer",
                    value: dioxus::core::AttributeValue::Text(value),
                    id,
                    ..
                } if value == "accept" => Some(*id),
                _ => None,
            })
            .expect("Accept is rendered");
        let click = || {
            Event::new(
                Rc::new(PlatformEventData::new(Box::<SerializedMouseData>::default()))
                    as Rc<dyn Any>,
                true,
            )
        };

        // Both surfaces are tabpanels their tabs control, and only one is shown.
        let html = dioxus_ssr::render(&dom);
        assert_eq!(html.matches(r#"role="tabpanel""#).count(), 2, "{html}");
        assert!(
            html.contains(r#"aria-controls="elicitation-"#)
                && html.contains(r#"tabindex="0""#)
                && html.contains(r#"tabindex="-1""#),
            "{html}"
        );

        // Enter in a one-field form submits it natively. That is not an
        // answer: Accept is the only way to send.
        let form = edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if name == "submit" => Some(*id),
                _ => None,
            })
            .expect("the engine's form listens for submit");
        dom.runtime().handle_event(
            "submit",
            Event::new(
                Rc::new(PlatformEventData::new(Box::new(
                    dioxus::html::SerializedFormData::new(String::new(), vec![]),
                ))) as Rc<dyn Any>,
                true,
            ),
            form,
        );
        dom.render_immediate(&mut NoOpMutations);
        // The count reaches Accept through an effect, on the next scheduler
        // turn.
        for _ in 0..3 {
            let _ = tokio::time::timeout(Duration::from_millis(50), dom.wait_for_work()).await;
            dom.render_immediate(&mut NoOpMutations);
        }
        assert!(
            replies.borrow().is_empty(),
            "submitting the form sends nothing"
        );
        // Submitting did reveal the finding, and Accept now says what it is
        // about to send past.
        assert!(
            dioxus_ssr::render(&dom).contains("Accept anyway (1 finding)"),
            "{}",
            dioxus_ssr::render(&dom)
        );

        dom.runtime().handle_event("click", click(), accept);
        let answer = replies.borrow_mut().pop().unwrap();
        let Reply::Accept(data) = answer.reply else {
            panic!("accept")
        };
        assert_eq!(data, Some(content(json!({"age":999}))));
        answer.request.accept(data).await.unwrap();
        dom.mark_dirty(dioxus::core::ScopeId::APP);
        let edits = dom.render_immediate_to_vec().edits;
        assert!(!edits.iter().any(|edit| matches!(edit, Mutation::Remove{id} | Mutation::ReplaceWith{id,..} if *id == accept)));
        let html = dioxus_ssr::render(&dom);
        assert!(html.contains("resolved") && html.contains("data-state=\"chosen\""));
        dom.runtime().handle_event("click", click(), accept);
        dom.render_immediate(&mut NoOpMutations);
        assert!(
            replies.borrow().is_empty(),
            "repeat activation sends nothing"
        );
        inspector.disconnect();
    }

    fn content(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn the_visible_surface_sends_and_raw_survives_until_the_form_changes() {
        let mut answer = Answering::new();
        assert_eq!(answer.content().unwrap(), None);
        answer.synchronize(content(json!({"age":999})));
        answer.raw(r#"{"age":"old"}"#.into());
        assert_eq!(
            answer.content().unwrap(),
            Some(content(json!({"age":"old"})))
        );
        answer.shown = Surface::Form;
        assert_eq!(answer.content().unwrap(), Some(content(json!({"age":999}))));
        answer.synchronize(content(json!({"age":999})));
        answer.shown = Surface::Raw;
        assert_eq!(
            answer.content().unwrap(),
            Some(content(json!({"age":"old"})))
        );
        answer.synchronize(content(json!({"age":1000})));
        assert_eq!(
            answer.content().unwrap(),
            Some(content(json!({"age":1000})))
        );
    }

    #[test]
    fn raw_refuses_only_invalid_json_or_a_non_object() {
        let mut answer = Answering::new();
        for text in ["[]", "null", "1", "{broken"] {
            answer.raw(text.into());
            assert!(answer.content().is_err());
        }
        answer.raw("  ".into());
        assert_eq!(answer.content().unwrap(), None);
        answer.raw("{}".into());
        assert_eq!(answer.content().unwrap(), Some(Map::new()));
        answer.raw(r#"{"age":"old","extra":{"nested":true}}"#.into());
        assert!(answer.content().is_ok());
    }

    fn form_of(schema: Value) -> String {
        #[component]
        fn Host(schema: Value) -> Element {
            rsx! { EngineFields { raw_schema: schema.to_string(), waiting: true, on_ready: |_| {}, on_problem: |_| {} } }
        }
        let mut dom = VirtualDom::new_with_props(Host, HostProps { schema });
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    #[test]
    fn the_registry_draws_formats_choices_defaults_and_unknown_types() {
        let html = form_of(json!({"type":"object","title":"Agent form","properties":{
            "name":{"type":"string","default":"Ada"}, "age":{"type":"integer"},
            "email":{"type":"string","format":"email"},"birthday":{"type":"string","format":"date"},
            "confirmed":{"type":"boolean","default":true},
            "choice":{"type":"string","oneOf":[{"const":"s","title":"Small"}]},
            "tags":{"type":"array","items":{"type":"string","enum":["acp","rust"]},"default":["rust"]},
            "future":{"type":"future","depth":3}}}));
        for text in [
            "Agent form",
            "value=\"Ada\"",
            "type=\"email\"",
            "type=\"date\"",
            "type=\"checkbox\"",
            "Small",
            "multiple-choice",
            "novalidate",
            "No control for this property type",
            "depth",
        ] {
            assert!(html.contains(text), "{text}: {html}");
        }
    }

    #[test]
    fn a_required_property_without_a_control_keeps_its_required_mark() {
        let html = form_of(json!({"type":"object","properties":{
            "future":{"type":"future"},"later":{"type":"later"}},"required":["future"]}));
        let row = |name: &str| {
            html.split(r#"data-slot="unrenderable""#)
                .find(|row| row.contains(&format!("<strong>{name}</strong>")))
                .unwrap_or_else(|| panic!("the {name} row: {html}"))
                .to_owned()
        };
        assert!(row("future").contains(r#"data-slot="required""#), "{html}");
        assert!(!row("later").contains(r#"data-slot="required""#), "{html}");
    }

    #[test]
    fn a_required_boolean_left_unset_is_not_called_false() {
        fn host() -> Element {
            rsx! { EngineFields {
                raw_schema: json!({"type":"object","required":["agree","name"],"properties":{
                    "agree":{"type":"boolean"},"name":{"type":"string"}}}).to_string(),
                waiting: true,
                on_ready: |form: schemaform_dioxus::FormHandle| { form.prepare_advisory_submission().unwrap(); },
                on_problem: |_| {},
            } }
        }
        let mut dom = VirtualDom::new(host);
        dom.rebuild_in_place();
        dom.render_immediate(&mut dioxus::core::NoOpMutations);
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains("Required, not set (unchecked is not false)."),
            "{html}"
        );
        assert!(html.contains("Required, nothing filled in."), "{html}");
    }

    #[test]
    fn a_default_beyond_engine_limits_keeps_the_raw_fallback_local() {
        let html = form_of(
            json!({"type":"object","properties":{"name":{"type":"string","default":"x".repeat(262_145)}}}),
        );
        assert!(html.contains("Use Raw to answer"));
    }

    #[test]
    fn released_renderer_reports_all_three_rules_with_field_names_without_gating() {
        use std::{cell::RefCell, rc::Rc};
        #[derive(Clone, Props)]
        struct HostProps {
            engine: Rc<RefCell<Option<schemaform_dioxus::FormHandle>>>,
        }
        impl PartialEq for HostProps {
            fn eq(&self, other: &Self) -> bool {
                Rc::ptr_eq(&self.engine, &other.engine)
            }
        }
        fn host(props: HostProps) -> Element {
            rsx! { EngineFields {
                raw_schema: r#"{"type":"object","properties":{"age":{"type":"integer","title":"Age","maximum":120,"default":999},"confidence":{"type":"number","title":"Confidence","maximum":1,"default":2},"name":{"type":"string","title":"Name"}},"required":["name"]}"#.to_owned(),
                waiting: true, on_ready: move |form| *props.engine.borrow_mut() = Some(form), on_problem: |_| {},
            } }
        }
        let engine = Rc::new(RefCell::new(None));
        let mut dom = VirtualDom::new_with_props(
            host,
            HostProps {
                engine: engine.clone(),
            },
        );
        dom.rebuild_in_place();
        let prepared = engine
            .borrow()
            .as_ref()
            .unwrap()
            .prepare_advisory_submission()
            .unwrap();
        assert_eq!(
            advisory_content(prepared.submission()),
            Some(content(json!({"age":999,"confidence":2})))
        );
        dom.render_immediate(&mut dioxus::core::NoOpMutations);
        let html = dioxus_ssr::render(&dom);
        for label in ["Age (/age)", "Confidence (/confidence)", "Name (/name)"] {
            assert!(html.contains(label), "{html}");
        }
        assert_eq!(
            html.matches("data-finding=\"required\"").count(),
            2,
            "summary and local finding: {html}"
        );
        assert_eq!(
            html.matches("data-finding=\"maximum\"").count(),
            4,
            "summary and local findings: {html}"
        );
        assert!(html.contains("Above the maximum of 120."), "{html}");
        assert!(html.contains("Above the maximum of 1."), "{html}");
        assert!(html.contains("Required, nothing filled in."), "{html}");
        for (binding, rule) in [
            ("/age", "maximum"),
            ("/confidence", "maximum"),
            ("/name", "required"),
        ] {
            assert_finding_description(&html, binding, rule);
        }
    }

    fn assert_finding_description(html: &str, binding: &str, rule: &str) {
        let tag = html
            .split('<')
            .find(|tag| {
                tag.starts_with("input ")
                    && tag
                        .split('>')
                        .next()
                        .unwrap()
                        .contains(&format!("name=\"{binding}\""))
            })
            .unwrap();
        let tag = tag.split('>').next().unwrap();
        let described = tag
            .split("aria-describedby=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        assert!(
            described
                .split_whitespace()
                .any(|id| html.split('<').any(|element| {
                    let attributes = element.split('>').next().unwrap();
                    attributes.contains(&format!("id=\"{id}\""))
                        && attributes.contains(&format!("data-finding=\"{rule}\""))
                })),
            "{binding} must reference its {rule} finding"
        );
    }

    #[test]
    fn findings_follow_edits_and_advisory_submission_keeps_every_representable_value() {
        use std::{cell::RefCell, rc::Rc};
        #[derive(Clone, Props)]
        struct HostProps {
            engine: Rc<RefCell<Option<schemaform_dioxus::FormHandle>>>,
        }
        impl PartialEq for HostProps {
            fn eq(&self, other: &Self) -> bool {
                Rc::ptr_eq(&self.engine, &other.engine)
            }
        }
        fn host(props: HostProps) -> Element {
            rsx! { EngineFields {
                raw_schema: json!({"type":"object","required":["required"],"properties":{
                    "age":{"type":"integer","minimum":0,"maximum":120},
                    "confidence":{"type":"number","minimum":0,"maximum":1},
                    "name":{"type":"string","minLength":3,"maxLength":5,"pattern":"^[a-z]+$"},
                    "email":{"type":"string","format":"email"},
                    "tags":{"type":"array","minItems":1,"maxItems":2,"items":{"type":"string","enum":["a","b","c"]}},
                    "required":{"type":"string"}
                }}).to_string(), waiting: true,
                on_ready: move |form| *props.engine.borrow_mut() = Some(form), on_problem: |_| {},
            } }
        }
        let engine = Rc::new(RefCell::new(None));
        let mut dom = VirtualDom::new_with_props(
            host,
            HostProps {
                engine: engine.clone(),
            },
        );
        dom.rebuild_in_place();
        let form = engine.borrow().as_ref().unwrap().clone();
        let _runtime = dioxus::core::RuntimeGuard::new(dom.runtime());
        let node_at = |binding: &str| {
            let root = form.reader().read().unwrap().root;
            form.node(root)
                .unwrap()
                .unwrap()
                .read()
                .unwrap()
                .unwrap()
                .children
                .iter()
                .find_map(|id| {
                    let node = form.node(*id).ok()??;
                    (node.read().ok()??.binding.as_ref()?.as_str() == binding).then_some(node)
                })
                .unwrap()
        };
        let cases = [
            (json!({"age":-1}), "Below the minimum of 0."),
            (json!({"age":121}), "Above the maximum of 120."),
            (json!({"confidence":-0.1}), "Below the minimum of 0."),
            (json!({"confidence":1.1}), "Above the maximum of 1."),
            (
                json!({"name":"é🙂"}),
                "Too few characters; at least 3 stated.",
            ),
            (
                json!({"name":"ab"}),
                "Too few characters; at least 3 stated.",
            ),
            (
                json!({"name":"abcdef"}),
                "Too many characters; at most 5 stated.",
            ),
            (json!({"tags":[]}), "Too few choices; at least 1 stated."),
            (
                json!({"tags":["a","b","c"]}),
                "Too many choices; at most 2 stated.",
            ),
            (json!({}), "Required, nothing filled in."),
            (json!({"name":"ABC"}), "Does not match the stated pattern"),
        ];
        for (data, wording) in cases {
            form.reinitialize(data.clone()).unwrap();
            let submitted = form.prepare_advisory_submission().unwrap();
            assert_eq!(submitted.submission().form_data(), &data);
            dom.render_immediate(&mut dioxus::core::NoOpMutations);
            let html = dioxus_ssr::render(&dom);
            assert!(html.contains(wording), "{wording}: {html}");
            assert!(html.contains("novalidate"));
            assert!(html.contains("Format email: not checked here."));
            // A second invalid edit must not hide findings by resetting
            // touched/submitted state. Correct through public field actions.
            let binding = data
                .as_object()
                .unwrap()
                .keys()
                .next()
                .map(|key| format!("/{key}"))
                .unwrap_or("/required".into());
            let node = node_at(&binding);
            if binding != "/tags" && binding != "/required" {
                let still_bad = match binding.as_str() {
                    "/age" => {
                        if data["age"].as_i64().unwrap() < 0 {
                            "-2"
                        } else {
                            "999"
                        }
                    }
                    "/confidence" => {
                        if data["confidence"].as_f64().unwrap() < 0.0 {
                            "-2"
                        } else {
                            "3"
                        }
                    }
                    _ => {
                        if wording.starts_with("Too many") {
                            "abcdefg"
                        } else if wording.starts_with("Does not") {
                            "DEF"
                        } else {
                            "a"
                        }
                    }
                };
                node.actions().input_text(still_bad).unwrap();
                dom.render_immediate(&mut dioxus::core::NoOpMutations);
                assert!(dioxus_ssr::render(&dom).contains(wording));
            }
            if binding == "/tags" {
                if data["tags"].as_array().unwrap().is_empty() {
                    node.actions().toggle_choice(json!("a")).unwrap();
                } else {
                    node.actions().toggle_choice(json!("c")).unwrap();
                }
            } else {
                node.actions()
                    .input_text(match binding.as_str() {
                        "/age" => "20",
                        "/confidence" => "0.5",
                        _ => "abc",
                    })
                    .unwrap();
            }
            node_at("/required").actions().input_text("yes").unwrap();
            dom.render_immediate(&mut dioxus::core::NoOpMutations);
            assert!(
                !dioxus_ssr::render(&dom).contains("data-finding=\""),
                "corrected values remove the notes"
            );
        }
        // Exercise edit buffers at the public adapter boundary, rather than
        // pretending an unparseable string is committed numeric data.
        let root = form.reader().read().unwrap().root;
        let children = form
            .node(root)
            .unwrap()
            .unwrap()
            .read()
            .unwrap()
            .unwrap()
            .children;
        for (binding, text, wording) in [
            ("/age", "1.5", "Not a whole number."),
            ("/confidence", "old", "Not a number."),
        ] {
            let node = children
                .iter()
                .find_map(|id| {
                    let node = form.node(*id).ok()??;
                    let projection = node.read().ok()??;
                    (projection.binding.as_ref()?.as_str() == binding).then_some(node)
                })
                .unwrap();
            node.actions().input_text(text).unwrap();
            form.prepare_advisory_submission().unwrap();
            dom.render_immediate(&mut dioxus::core::NoOpMutations);
            let html = dioxus_ssr::render(&dom);
            assert!(html.contains(wording), "{html}");
            node.actions().input_text("1").unwrap();
            dom.render_immediate(&mut dioxus::core::NoOpMutations);
            assert!(!dioxus_ssr::render(&dom).contains(wording));
        }
    }

    #[test]
    fn oversized_pattern_findings_do_not_claim_the_pattern_was_null() {
        let prepared = crate::elicitation_schema::prepare(
            &json!({"type":"object","properties":{
                "name":{"type":"string","pattern":format!("^{}$", "a".repeat(4200))}
            }})
            .to_string(),
        )
        .unwrap();
        let mut form = prepared
            .definition
            .unwrap()
            .create_form(json!({"name":"b"}))
            .unwrap();
        let submission = form.prepare_advisory_submission();
        let finding = submission
            .submission()
            .findings()
            .find_map(|finding| match finding {
                schemaform::form::SubmissionBlocker::Validation(finding) => Some(finding),
                _ => None,
            })
            .unwrap();
        assert_eq!(finding.parameters()["omitted"], true);
        fn host() -> Element {
            rsx! { EngineFields {
                raw_schema: json!({"type":"object","properties":{"name":{"type":"string","default":"b","pattern":format!("^{}$", "a".repeat(4200))}}}).to_string(),
                waiting:true, on_ready: |form: schemaform_dioxus::FormHandle| { form.prepare_advisory_submission().unwrap(); },
                on_problem: |_| {},
            } }
        }
        let mut dom = VirtualDom::new(host);
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);
        assert!(html.contains("exceeds the finding display limit"), "{html}");
        assert!(!html.contains("pattern null"));
    }

    fn url_card(url: &str, state: ElicitationState, completed: bool) -> String {
        dioxus_ssr::render_element(target(url, completed, &state))
    }

    #[test]
    fn only_an_http_link_is_a_link_and_it_names_the_host_it_opens() {
        let html = url_card(
            "https://example.com/x?y#z",
            ElicitationState::Waiting,
            false,
        );
        assert!(
            html.contains(r#"href="https://example.com/x?y#z""#)
                && html.contains(r#"<strong class="host">example.com</strong>"#)
                && html.contains("Opens in your browser"),
            "{html}"
        );
        assert!(!html.contains("url-credentials"), "{html}");

        // A user name in front of the host: the host shown is the one the
        // browser opens, and the card says why.
        let spoof = url_card(
            "https://trusted.example@evil.example/login",
            ElicitationState::Waiting,
            false,
        );
        assert!(
            spoof.contains(r#"<strong class="host">evil.example</strong>"#)
                && !spoof.contains(r#"<strong class="host">trusted.example"#)
                && spoof.contains(r#"data-slot="url-credentials""#),
            "{spoof}"
        );
        let password = url_card(
            "http://user:secret@example.com",
            ElicitationState::Waiting,
            false,
        );
        assert!(password.contains("url-credentials"), "{password}");

        // Every other scheme, and text that is no URL at all, stays text.
        for inert in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,<b>hi</b>",
            "dioxus://index.html",
            "mailto:someone@example.com",
            "not a url",
            "https://",
        ] {
            let html = url_card(inert, ElicitationState::Waiting, false);
            assert!(
                !html.contains("<a ") && html.contains(r#"data-slot="url-text""#),
                "{inert} is not a link: {html}"
            );
            assert!(html.contains(r#"data-slot="url-inert""#), "{html}");
        }
    }

    #[test]
    fn a_url_stops_being_a_link_once_the_request_is_over() {
        for state in [
            ElicitationState::Answered(ElicitationAnswer::Declined),
            ElicitationState::Answered(ElicitationAnswer::Cancelled),
            ElicitationState::Abandoned,
        ] {
            let html = url_card("https://example.com", state, false);
            assert!(
                !html.contains("<a ") && html.contains(r#"data-slot="url-closed""#),
                "{html}"
            );
        }
        // Accepted is consent to visit it, so the link stays until the agent
        // says the interaction finished.
        let accepted = || ElicitationState::Answered(ElicitationAnswer::Accepted(None));
        assert!(url_card("https://example.com", accepted(), false).contains("<a "));
        assert!(!url_card("https://example.com", accepted(), true).contains("<a "));
    }

    #[test]
    fn url_consent_is_separate_from_completion() {
        let state = ElicitationState::Answered(ElicitationAnswer::Accepted(None));
        assert!(
            dioxus_ssr::render_element(said(&ElicitationState::CancelledByAgent, false, false))
                .contains("The agent withdrew this request.")
        );
        assert!(dioxus_ssr::render_element(said(&state, false, true)).contains("not completion"));
        assert!(!dioxus_ssr::render_element(said(&state, true, true)).contains("not completion"));
    }
}
