//! The inline elicitation panel (`docs/architecture.md` §7.8, §9): the question
//! the agent asked, drawn where it asked it.
//!
//! **Inline, like the permission panel beside it.** No modal and nothing to
//! dismiss: an elicitation is traffic, so it sits in the timeline in wire order
//! and the trace stays readable while it waits.
//!
//! **The form is the agent's schema, and the schema is not a gate.** Every
//! control here comes from a property the agent sent, carrying the title, the
//! description and the constraints it stated — and a value outside those
//! constraints is *reported* and still sent. A tool that could not put `age:
//! 999` on the wire could not find out what the agent does with it, which is
//! what somebody came here for. The one shape that is not a matter of taste is
//! the method's own: `content` is a JSON object or it is nothing.
//!
//! **And a raw tab, because typed controls cannot say everything.** A number
//! input cannot express a string where an integer was asked for, so the panel
//! carries the object it would send and lets it be written by hand. Whichever
//! surface was edited last is what sends, and the panel says which.
//!
//! **A URL is shown and never followed here.** The full URL and its host are on
//! screen before anything happens, nothing is prefetched, and the link hands off
//! to the reader's own browser — the window this tool draws never loads the page
//! it sent somebody to, which is the only honest reading of a client that must
//! not be able to inspect the interaction.

use acp_inspector_core::{ElicitationAnswer, ElicitationRequest, ElicitationState, v1};
use dioxus::prelude::*;
use serde_json::{Map, Value};

/// Put the tool call an elicitation named under the viewport and keyboard
/// focus. The id is Agent-controlled text and is sent over Dioxus's channel
/// rather than interpolated into the script, and escaped before it reaches a
/// selector.
const FOCUS_TOOL_CALL: &str = r#"
const id = await dioxus.recv();
await new Promise((resolve) => requestAnimationFrame(resolve));
const target = document.querySelector(`[data-tool-call="${CSS.escape(id)}"]`);
if (target) {
  target.scrollIntoView({ block: "center" });
  target.focus({ preventScroll: true });
}
"#;

/// The reader's answer to an elicitation: the request, and what they said.
///
/// The request travels with the answer because the request is what knows how to
/// be answered — it carries its own resolver — so a handler holding one of these
/// needs nothing else to finish the job.
#[derive(Clone, PartialEq)]
pub struct Answer {
    pub request: ElicitationRequest,
    pub reply: Reply,
}

/// One of the three things a reader can say, in the protocol's own vocabulary.
#[derive(Clone, PartialEq)]
pub enum Reply {
    /// Filled in, or consented to. `None` is an acceptance with no content,
    /// which the specification permits and a URL acceptance normally is.
    Accept(Option<Map<String, Value>>),
    Decline,
    Cancel,
}

/// Which surface the reader is answering on, and therefore what an acceptance
/// sends.
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

/// What the reader has written, and what an acceptance would send.
///
/// **A value rather than three signals**, because the rule that decides what
/// goes on the wire is the only real logic in this file and a component is a
/// poor place to keep something that has to be asserted. The panel holds one of
/// these; the tests hold one too.
///
/// **The surface being shown is the surface that sends.** The two are kept in
/// step rather than merged — what is typed in one is still there on returning to
/// it — but an acceptance carries what the reader can see, because a control
/// that sent something other than what is on screen is a control that lied
/// about what it does (§7.8).
#[derive(Clone, PartialEq, Debug)]
struct Answering {
    /// What the form's controls have been given, the agent's own defaults
    /// included.
    filled: Map<String, Value>,
    /// What the raw tab holds. Kept in step with the form until the reader
    /// writes in it, so switching to it shows what the form would have sent.
    written: String,
    shown: Surface,
}

impl Answering {
    fn new(schema: Option<&v1::ElicitationSchema>) -> Self {
        let filled = schema.map(defaults).unwrap_or_default();
        Self {
            written: pretty(&filled),
            filled,
            shown: Surface::Form,
        }
    }

    /// A form control was used. The raw tab follows, because it has not been
    /// written in yet as far as anybody can tell — and if it has, the reader is
    /// on the form now and this is what they are answering with.
    fn write(&mut self, name: String, value: Option<Value>) {
        match value {
            Some(value) => {
                self.filled.insert(name, value);
            }
            None => {
                self.filled.remove(&name);
            }
        }
        self.written = pretty(&self.filled);
        self.shown = Surface::Form;
    }

    /// The raw tab was written in. The form is left exactly as it was: going
    /// back to it is going back to what it held, not to a parse of this.
    fn raw(&mut self, written: String) {
        self.written = written;
        self.shown = Surface::Raw;
    }

    fn show(&mut self, surface: Surface) {
        self.shown = surface;
    }

    /// What an acceptance sends, or why it cannot.
    ///
    /// The only thing that can be wrong is a shape the method does not have:
    /// `content` is an object or it is absent. A value the agent's own schema
    /// forbids is not wrong here — it is the point (§7.8) — so nothing else in
    /// this function refuses anything.
    ///
    /// An empty *form* sends no content, because nothing was filled in. An
    /// empty *object*, typed by hand, sends `{}`: that is a thing the reader
    /// wrote, and rewriting it to *nothing* would be this window editing an
    /// answer on its way out.
    fn content(&self) -> Result<Option<Map<String, Value>>, String> {
        match self.shown {
            Surface::Form => Ok(Some(self.filled.clone()).filter(|filled| !filled.is_empty())),
            Surface::Raw if self.written.trim().is_empty() => Ok(None),
            Surface::Raw => match serde_json::from_str::<Value>(&self.written) {
                Ok(Value::Object(object)) => Ok(Some(object)),
                Ok(_) => Err(
                    "`content` is an object or nothing — that is the shape the method has."
                        .to_owned(),
                ),
                Err(problem) => Err(problem.to_string()),
            },
        }
    }
}

/// The question, the form or the URL, and the three answers.
#[component]
pub fn Panel(request: ElicitationRequest, on_answer: EventHandler<Answer>) -> Element {
    let schema = request.form().cloned();
    let mut answering = use_signal(|| Answering::new(schema.as_ref()));

    let state = request.state();
    let waiting = request.is_waiting();
    let shown = answering.read().shown;
    let sending = answering.read().content();
    let unusable = sending.clone().err();

    let answered = {
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
    let accepted = sending.unwrap_or_default();

    rsx! {
        div {
            class: if waiting { "evidence blocking elicit" } else { "evidence blocking elicit answered" },
            "data-slot": "elicitation",
            div { class: "blocking-head",
                span { class: "eyebrow",
                    "{v1::CLIENT_METHOD_NAMES.elicitation_create}"
                    if schema.is_some() { " · form" } else { " · url" }
                }
                span { class: "blocking-wait",
                    if waiting { "awaiting client input" } else { "resolved" }
                }
            }
            div { class: "blocking-body",
                // The agent's own words for what it needs, which the
                // specification asks a client to present.
                p { class: "blocking-title", "{request.message()}" }
                span { class: "blocking-meta", "{request.id()}" }

                {scope(&request)}

                if let Some(schema) = schema.clone() {
                    div { class: "elicitation-form", "data-slot": "form",
                        {tabs(shown, move |chosen| answering.write().show(chosen))}
                        if shown == Surface::Form {
                            {
                                let filled = answering.read().filled.clone();
                                fields(&schema, &filled, move |name, value| {
                                    answering.write().write(name, value)
                                }, waiting)
                            }
                        } else {
                            {
                                let written = answering.read().written.clone();
                                raw(&written, move |text| answering.write().raw(text), waiting)
                            }
                        }
                        // Which surface answers, said where the answer is
                        // written rather than left to be discovered by sending
                        // one thing while looking at another.
                        p { class: "hint", "data-slot": "sends",
                            "Accepting sends what the {shown.label()} tab holds."
                        }
                        if let Some(problem) = unusable.clone() {
                            p { class: "detail detail-warn", "data-slot": "unusable", "{problem}" }
                        }
                    }
                } else if let Some(url) = request.url() {
                    {target(url, request.completed())}
                } else {
                    p { class: "unnamed",
                        "A mode this window has no rendering for. What arrived is below, as it arrived."
                    }
                }

                {said(&state, request.completed(), request.url().is_some())}

                div { class: "answers",
                    {answer("accept", "Accept", "btn-filled", waiting && unusable.is_none(), answered(Reply::Accept(accepted)), &state)}
                    {answer("decline", "Decline", "btn-quiet btn-bad", waiting, answered(Reply::Decline), &state)}
                    {answer("cancel", "Cancel", "btn-quiet", waiting, answered(Reply::Cancel), &state)}
                }
            }
        }
    }
}

/// What the elicitation is tied to, said rather than assumed.
///
/// A session, optionally a tool call in it, or a JSON-RPC call outside any
/// session — the third being the one an agent asks before there is a
/// conversation to ask inside.
fn scope(request: &ElicitationRequest) -> Element {
    let tool_call = request.tool_call_id().map(ToString::to_string);

    rsx! {
        p { class: "scope hint", "data-slot": "scope",
            if let Some(session) = request.session_id() {
                "In session "
                code { class: "mono", "{session}" }
            } else if let Some(call) = request.request_scope() {
                "Outside any session, tied to request "
                code { class: "mono", "{call}" }
            } else {
                "A scope this window has no name for."
            }
            if let Some(id) = tool_call {
                ", raised by tool call "
                code { class: "mono", "{id}" }
                button {
                    class: "btn btn-ghost btn-xs reach",
                    "data-slot": "reach-tool-call",
                    title: "Move focus to the tool call this question came from",
                    onclick: {
                        let id = id.clone();
                        move |_| {
                            let focusing = document::eval(FOCUS_TOOL_CALL);
                            let _ = focusing.send(id.clone());
                        }
                    },
                    "go to it"
                }
            }
        }
    }
}

/// The two surfaces an answer can be written in, in the tablist this window
/// already uses for the same job.
fn tabs(surface: Surface, on_choose: impl FnMut(Surface) + Clone + 'static) -> Element {
    rsx! {
        div { class: "form-tabs seg", role: "tablist", aria_label: "How to answer",
            for (choice , label) in [(Surface::Form, "Form"), (Surface::Raw, "Raw")] {
                button {
                    key: "{label}",
                    class: "seg-item seg-word",
                    r#type: "button",
                    role: "tab",
                    "data-slot": "surface",
                    "data-surface": "{label}",
                    aria_selected: choice == surface,
                    onclick: {
                        let mut on_choose = on_choose.clone();
                        move |_| on_choose(choice)
                    },
                    "{label}"
                }
            }
        }
    }
}

/// Every property the agent asked for, as the control its type calls for.
///
/// The values in and one edit out: what an edit *means* — which surface now
/// answers, and what the raw tab holds — is [`Answering`]'s, so that it is a
/// thing a test can assert rather than a thing a component does.
fn fields(
    schema: &v1::ElicitationSchema,
    filled: &Map<String, Value>,
    on_write: impl FnMut(String, Option<Value>) + Clone + 'static,
    waiting: bool,
) -> Element {
    let required = schema.required.clone().unwrap_or_default();

    rsx! {
        div { class: "fields", "data-slot": "fields",
            if let Some(title) = schema.title.as_ref() {
                strong { class: "form-title", "{title}" }
            }
            if let Some(description) = schema.description.as_ref() {
                p { class: "hint", "{description}" }
            }
            for (name , property) in schema.properties.clone() {
                {
                    let held = filled.get(&name).cloned();
                    let needed = required.contains(&name);
                    field(name.clone(), &property, held, needed, waiting, {
                        let mut on_write = on_write.clone();
                        let name = name.clone();
                        move |value| on_write(name.clone(), value)
                    })
                }
            }
            if schema.properties.is_empty() {
                p { class: "unnamed", "The agent asked for a form with no fields in it." }
            }
        }
    }
}

/// One property, as a labelled control.
fn field(
    name: String,
    property: &v1::ElicitationPropertySchema,
    held: Option<Value>,
    required: bool,
    waiting: bool,
    on_value: impl FnMut(Option<Value>) + Clone + 'static,
) -> Element {
    let (title, description, constraint) = about(property);
    let label = title.unwrap_or_else(|| name.clone());

    rsx! {
        label { key: "{name}", class: "field", "data-slot": "field", "data-field": "{name}",
            span { class: "field-label",
                span { class: "field-name", "{label}" }
                span { class: "hint mono", "{name}" }
                if required {
                    span { class: "field-note", "required" }
                }
            }
            if let Some(description) = description {
                span { class: "hint", "{description}" }
            }
            {control(&name, property, held, waiting, on_value)}
            if let Some(constraint) = constraint {
                span { class: "field-note mono", "{constraint}" }
            }
        }
    }
}

/// The title, description and stated constraints of a property — reported, and
/// never enforced (§7.8).
fn about(
    property: &v1::ElicitationPropertySchema,
) -> (Option<String>, Option<String>, Option<String>) {
    match property {
        v1::ElicitationPropertySchema::String(string) => {
            let mut said = Vec::new();
            if let Some(min) = string.min_length {
                said.push(format!("at least {min} characters"));
            }
            if let Some(max) = string.max_length {
                said.push(format!("at most {max} characters"));
            }
            if let Some(pattern) = string.pattern.as_ref() {
                said.push(format!("matching {pattern}"));
            }
            if let Some(format) = string.format.as_ref() {
                said.push(format!("as {}", spelt(format)));
            }
            (
                string.title.clone(),
                string.description.clone(),
                joined(said),
            )
        }
        v1::ElicitationPropertySchema::Number(number) => {
            let mut said = Vec::new();
            if let Some(min) = number.minimum {
                said.push(format!("at least {min}"));
            }
            if let Some(max) = number.maximum {
                said.push(format!("at most {max}"));
            }
            (
                number.title.clone(),
                number.description.clone(),
                joined(said),
            )
        }
        v1::ElicitationPropertySchema::Integer(integer) => {
            let mut said = Vec::new();
            if let Some(min) = integer.minimum {
                said.push(format!("at least {min}"));
            }
            if let Some(max) = integer.maximum {
                said.push(format!("at most {max}"));
            }
            (
                integer.title.clone(),
                integer.description.clone(),
                joined(said),
            )
        }
        v1::ElicitationPropertySchema::Boolean(boolean) => {
            (boolean.title.clone(), boolean.description.clone(), None)
        }
        v1::ElicitationPropertySchema::Array(array) => {
            let mut said = Vec::new();
            if let Some(min) = array.min_items {
                said.push(format!("at least {min} chosen"));
            }
            if let Some(max) = array.max_items {
                said.push(format!("at most {max} chosen"));
            }
            (array.title.clone(), array.description.clone(), joined(said))
        }
        // A type this window has no name for. The specification is explicit
        // that a client must not render one as a control it *does* have, and
        // this file agrees for its own reason (§8).
        _ => (None, None, None),
    }
}

fn joined(said: Vec<String>) -> Option<String> {
    (!said.is_empty()).then(|| said.join(", "))
}

/// A string format as the agent spelled it, which is also the input type it
/// asks a browser for.
fn spelt(format: &v1::StringFormat) -> &'static str {
    match format {
        v1::StringFormat::Email => "email",
        v1::StringFormat::Uri => "uri",
        v1::StringFormat::Date => "date",
        v1::StringFormat::DateTime => "date-time",
        _ => "a format this window has no name for",
    }
}

/// The control a property type calls for, and no control at all for a type this
/// window does not know.
fn control(
    name: &str,
    property: &v1::ElicitationPropertySchema,
    held: Option<Value>,
    waiting: bool,
    mut on_value: impl FnMut(Option<Value>) + Clone + 'static,
) -> Element {
    let text = held.as_ref().map(|value| match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    });
    match property {
        v1::ElicitationPropertySchema::String(string) => {
            if let Some(options) = choices(string) {
                return rsx! {
                    select {
                        class: "select select-sm",
                        aria_label: "{name}",
                        aria_disabled: (!waiting).then_some("true"),
                        tabindex: (!waiting).then_some("-1"),
                        onchange: move |event| {
                            let chosen = event.value();
                            on_value(
                                (!chosen.is_empty()).then(|| Value::String(chosen)),
                            );
                        },
                        option { value: "", selected: text.is_none(), "—" }
                        for (value , title) in options {
                            option {
                                key: "{value}",
                                value: "{value}",
                                selected: text.as_deref() == Some(value.as_str()),
                                "{title}"
                            }
                        }
                    }
                };
            }

            let kind = match string.format.as_ref() {
                Some(v1::StringFormat::Email) => "email",
                Some(v1::StringFormat::Uri) => "url",
                Some(v1::StringFormat::Date) => "date",
                Some(v1::StringFormat::DateTime) => "datetime-local",
                _ => "text",
            };
            rsx! {
                input {
                    r#type: kind,
                    class: "input input-sm",
                    aria_label: "{name}",
                    value: text.unwrap_or_default(),
                    aria_disabled: (!waiting).then_some("true"),
                    tabindex: (!waiting).then_some("-1"),
                    oninput: move |event| {
                        let written = event.value();
                        on_value((!written.is_empty()).then(|| Value::String(written)));
                    },
                }
            }
        }
        v1::ElicitationPropertySchema::Integer(_) | v1::ElicitationPropertySchema::Number(_) => {
            rsx! {
                input {
                    r#type: "number",
                    class: "input input-sm",
                    aria_label: "{name}",
                    value: text.unwrap_or_default(),
                    aria_disabled: (!waiting).then_some("true"),
                    tabindex: (!waiting).then_some("-1"),
                    oninput: move |event| {
                        let written = event.value();
                        // What was typed, as a number where it is one and as
                        // itself where it is not: a value the agent's own
                        // schema forbids is a value this tool can send (§7.8).
                        on_value(match written.as_str() {
                            "" => None,
                            written => Some(
                                serde_json::from_str::<Value>(written)
                                    .ok()
                                    .filter(Value::is_number)
                                    .unwrap_or_else(|| Value::String(written.to_owned())),
                            ),
                        });
                    },
                }
            }
        }
        v1::ElicitationPropertySchema::Boolean(_) => {
            let on = held.as_ref().and_then(Value::as_bool).unwrap_or(false);
            rsx! {
                input {
                    r#type: "checkbox",
                    class: "toggle toggle-sm",
                    aria_label: "{name}",
                    checked: on,
                    aria_disabled: (!waiting).then_some("true"),
                    tabindex: (!waiting).then_some("-1"),
                    onclick: move |event| {
                        event.prevent_default();
                        if waiting {
                            on_value(Some(Value::Bool(!on)));
                        }
                    },
                }
            }
        }
        v1::ElicitationPropertySchema::Array(array) => {
            let chosen: Vec<String> = held
                .as_ref()
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| value.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            let Some(items) = selectable(&array.items) else {
                return rsx! {
                    p { class: "unnamed", "data-slot": "unrenderable",
                        "An item type this window has no control for, so it draws none. The Raw tab can still answer it."
                    }
                };
            };

            rsx! {
                div { class: "multi",
                    for (value , title) in items {
                        label { key: "{value}", class: "choice",
                            input {
                                r#type: "checkbox",
                                class: "checkbox checkbox-sm",
                                aria_label: "{title}",
                                checked: chosen.contains(&value),
                                aria_disabled: (!waiting).then_some("true"),
                                tabindex: (!waiting).then_some("-1"),
                                onclick: {
                                    let value = value.clone();
                                    let chosen = chosen.clone();
                                    let mut on_value = on_value.clone();
                                    move |event: Event<MouseData>| {
                                        event.prevent_default();
                                        if !waiting {
                                            return;
                                        }
                                        let mut next = chosen.clone();
                                        if let Some(at) = next.iter().position(|held| held == &value) {
                                            next.remove(at);
                                        } else {
                                            next.push(value.clone());
                                        }
                                        on_value(
                                            (!next.is_empty())
                                                .then(|| Value::Array(next.into_iter().map(Value::String).collect())),
                                        );
                                    }
                                },
                            }
                            span { "{title}" }
                        }
                    }
                }
            }
        }
        // The specification says a client that does not understand a property
        // type must not render it as a known input control, and this window
        // would not want to: a guess drawn as a text box is a guess with a
        // control's worth of confidence behind it (§8).
        _ => rsx! {
            div { class: "unrenderable", "data-slot": "unrenderable",
                p { class: "unnamed",
                    "A property type this window has no control for, so it draws none. The Raw tab can still answer it."
                }
                pre { class: "raw", "{raw_schema(property)}" }
            }
        },
    }
}

/// A single-select's values and the titles to show for them, or `None` where the
/// property is plain text.
fn choices(string: &v1::StringPropertySchema) -> Option<Vec<(String, String)>> {
    if let Some(options) = string.one_of.as_ref() {
        return Some(
            options
                .iter()
                .map(|option| (option.value.clone(), option.title.clone()))
                .collect(),
        );
    }
    string.enum_values.as_ref().map(|values| {
        values
            .iter()
            .map(|value| (value.clone(), value.clone()))
            .collect()
    })
}

/// A multi-select's values and their titles, or `None` for an item type this
/// window has no control for.
fn selectable(items: &v1::MultiSelectItems) -> Option<Vec<(String, String)>> {
    match items {
        v1::MultiSelectItems::String(strings) => Some(
            strings
                .values
                .iter()
                .map(|value| (value.clone(), value.clone()))
                .collect(),
        ),
        v1::MultiSelectItems::Titled(titled) => Some(
            titled
                .options
                .iter()
                .map(|option| (option.value.clone(), option.title.clone()))
                .collect(),
        ),
        _ => None,
    }
}

/// The property as the agent sent it, for the types this window draws no
/// control for.
fn raw_schema(property: &v1::ElicitationPropertySchema) -> String {
    serde_json::to_string_pretty(property)
        .unwrap_or_else(|_| "a property this window could not even re-serialize".to_owned())
}

/// The object an acceptance would send, written by hand.
fn raw(
    written: &str,
    mut on_write: impl FnMut(String) + Clone + 'static,
    waiting: bool,
) -> Element {
    rsx! {
        div { class: "raw-answer",
            textarea {
                class: "textarea",
                "data-slot": "raw-content",
                aria_label: "The content this answer will send",
                rows: 8,
                value: "{written}",
                aria_disabled: (!waiting).then_some("true"),
                tabindex: (!waiting).then_some("-1"),
                oninput: move |event| on_write(event.value()),
            }
            p { class: "hint",
                "Whatever is here is what an acceptance sends, exactly as written — including values the agent's own schema forbids."
            }
        }
    }
}

/// The URL, its host, and the one control that opens it.
///
/// The anchor is the hand-off: this window refuses `http(s)` navigation inside
/// itself and gives the URL to the platform's browser, so the page is never
/// loaded anywhere this tool could read it. Nothing is fetched until the click.
fn target(url: &str, completed: bool) -> Element {
    let host = host(url);

    rsx! {
        div { class: "elicitation-url", "data-slot": "url",
            p { class: "hint",
                if let Some(host) = host.clone() {
                    "It wants you to visit "
                    strong { class: "host", "{host}" }
                } else {
                    "It wants you to visit a URL this window cannot read a host out of."
                }
            }
            // In full, and verbatim: a host is what a reader checks and the
            // whole URL is what they are agreeing to.
            a { class: "url-target", "data-slot": "url-target", href: "{url}", "{url}" }
            p { class: "hint",
                "Opens in your browser. This window never loads it, and never fetches it before you ask."
            }
            if completed {
                p { class: "detail", "data-slot": "completed",
                    "The agent says the interaction finished."
                }
            }
        }
    }
}

/// The host of a URL, read off the text rather than parsed: this window has no
/// URL parser and does not need one to show a reader what they are agreeing to.
fn host(url: &str) -> Option<String> {
    let after = url.split_once("://")?.1;
    let host = after.split(['/', '?', '#']).next()?;
    (!host.is_empty()).then(|| host.to_owned())
}

/// Where the request stands, in a line.
fn said(state: &ElicitationState, completed: bool, url: bool) -> Element {
    match state {
        ElicitationState::Waiting => rsx! {
            p { class: "detail detail-live", "The agent is waiting here until you answer." }
        },
        ElicitationState::Answering => rsx! {
            p { class: "detail", "Sending the answer..." }
        },
        ElicitationState::Answered(ElicitationAnswer::Accepted(content)) => rsx! {
            p { class: "detail",
                "Sent "
                code { "accept" }
                if let Some(content) = content {
                    " with " {plural(content.len())}
                } else {
                    " with no content"
                }
                "."
                if url && !completed {
                    " Accepting is consent, not completion: the agent has not said the interaction finished."
                }
            }
        },
        ElicitationState::Answered(ElicitationAnswer::Declined) => rsx! {
            p { class: "detail", "Sent " code { "decline" } ". The agent was told no." }
        },
        ElicitationState::Answered(ElicitationAnswer::Cancelled) => rsx! {
            p { class: "detail detail-warn",
                "Sent "
                code { "cancel" }
                ". A dismissed question, or the answer a client owes everything it leaves waiting."
            }
        },
        // Nobody answered it and nobody can: the turn it was asked in ended, or
        // the agent went away holding it.
        ElicitationState::Abandoned => rsx! {
            p { class: "detail detail-warn",
                "Nobody answered this, and nobody can now. The turn ended, the agent went away, or an answer could not be sent."
            }
        },
        // Both enums are non-exhaustive, and a state this window has not been
        // taught must not be rendered as one it has.
        _ => rsx! {
            p { class: "detail", "A state this window has no rendering for." }
        },
    }
}

fn plural(fields: usize) -> Element {
    if fields == 1 {
        rsx! { "1 field" }
    } else {
        rsx! { "{fields} fields" }
    }
}

/// One of the three answers, as the button that sends it.
///
/// Never natively disabled: an activated control keeps its DOM node and its
/// keyboard focus while the answer is in flight, guarding activation instead —
/// the contract every guarded control in this window follows (§9).
fn answer(
    slot: &'static str,
    label: &'static str,
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
    let classes = if chosen {
        format!("answer btn btn-sm btn-filled picked {variant}")
    } else {
        format!("answer btn btn-sm {variant}")
    };

    rsx! {
        button {
            key: "{slot}",
            class: "{classes}",
            "data-answer": "{slot}",
            "data-state": if chosen { "chosen" } else if available { "waiting" } else { "unavailable" },
            aria_pressed: chosen,
            aria_disabled: (!available).then_some("true"),
            tabindex: (!available).then_some("-1"),
            onclick: on_click,
            "{label}"
        }
    }
}

/// What the agent said each field starts as.
///
/// Pre-populated where the schema supplies a default, which the specification
/// asks of a client that supports them — and which is what makes a ten-field
/// form one click to answer.
fn defaults(schema: &v1::ElicitationSchema) -> Map<String, Value> {
    let mut content = Map::new();
    for (name, property) in &schema.properties {
        let value = match property {
            v1::ElicitationPropertySchema::String(string) => {
                string.default.clone().map(Value::String)
            }
            v1::ElicitationPropertySchema::Number(number) => number
                .default
                .and_then(serde_json::Number::from_f64)
                .map(Value::Number),
            v1::ElicitationPropertySchema::Integer(integer) => {
                integer.default.map(|default| Value::Number(default.into()))
            }
            v1::ElicitationPropertySchema::Boolean(boolean) => boolean.default.map(Value::Bool),
            v1::ElicitationPropertySchema::Array(array) => array
                .default
                .clone()
                .map(|values| Value::Array(values.into_iter().map(Value::String).collect())),
            _ => None,
        };
        if let Some(value) = value {
            content.insert(name.clone(), value);
        }
    }
    content
}

/// The content as the raw tab shows it.
fn pretty(content: &Map<String, Value>) -> String {
    serde_json::to_string_pretty(&Value::Object(content.clone()))
        .unwrap_or_else(|_| "{}".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// One field's worth of content, which is all these tests send.
    fn content(field: &str, value: Value) -> Map<String, Value> {
        let mut content = Map::new();
        content.insert(field.to_owned(), value);
        content
    }

    /// A schema rendered as the form it asks for, with nothing answered yet.
    ///
    /// The panel itself needs an `ElicitationRequest`, which only core can mint
    /// — so what a screen test holds is the same thing the permission panel's
    /// tests hold: the pieces that draw, given what they draw from.
    fn form_of(schema: v1::ElicitationSchema, waiting: bool) -> String {
        #[component]
        fn Host(schema: v1::ElicitationSchema, waiting: bool) -> Element {
            let filled = defaults(&schema);
            rsx! {
                div { {fields(&schema, &filled, move |_, _| {}, waiting)} }
            }
        }

        let mut dom = VirtualDom::new_with_props(Host, HostProps { schema, waiting });
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    /// The three answers, in whatever state the request is in.
    fn answers_of(state: ElicitationState, available: bool) -> String {
        #[component]
        fn Host(state: ElicitationState, available: bool) -> Element {
            rsx! {
                div {
                    {answer("accept", "Accept", "btn-success", available, |_| {}, &state)}
                    {answer("decline", "Decline", "btn-error", available, |_| {}, &state)}
                    {answer("cancel", "Cancel", "btn-secondary", available, |_| {}, &state)}
                }
            }
        }

        let mut dom = VirtualDom::new_with_props(Host, HostProps { state, available });
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    /// The URL body of a URL-mode elicitation.
    fn url_of(url: String, completed: bool) -> String {
        #[component]
        fn Host(url: String, completed: bool) -> Element {
            rsx! {
                div { {target(&url, completed)} }
            }
        }

        let mut dom = VirtualDom::new_with_props(Host, HostProps { url, completed });
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    /// Where the request stands, in the line the panel says it in.
    fn said_of(state: ElicitationState, completed: bool, url: bool) -> String {
        #[component]
        fn Host(state: ElicitationState, completed: bool, url: bool) -> Element {
            rsx! {
                div { {said(&state, completed, url)} }
            }
        }

        let mut dom = VirtualDom::new_with_props(
            Host,
            HostProps {
                state,
                completed,
                url,
            },
        );
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    fn schema(json: serde_json::Value) -> v1::ElicitationSchema {
        serde_json::from_value(json).expect("a schema the crate can read")
    }

    #[test]
    fn the_surface_being_shown_is_the_surface_that_answers() {
        // §7.8's rule, in the one place it is decided. A reader accepts what
        // they can see: a control that sent something other than what is on
        // screen would be lying about what it does.
        let schema = schema(serde_json::json!({
            "type": "object",
            "properties": {"name": {"type": "string", "default": "Ada"}}
        }));
        let mut answering = Answering::new(Some(&schema));

        assert_eq!(
            answering.content(),
            Ok(Some(content("name", json!("Ada")))),
            "the agent's defaults are what an untouched form sends"
        );

        answering.raw(r#"{"name": 7}"#.to_owned());
        assert_eq!(
            answering.content(),
            Ok(Some(content("name", json!(7)))),
            "writing in the raw tab is answering on it"
        );

        // Back to the form, without touching a control. What the form holds is
        // what sends — and what was written by hand is still there to go back
        // to, which is the difference between switching surfaces and losing
        // work.
        answering.show(Surface::Form);
        assert_eq!(answering.content(), Ok(Some(content("name", json!("Ada")))));
        assert_eq!(answering.written, r#"{"name": 7}"#);

        // And using a control takes the answer back to the form, whichever tab
        // was written in last.
        answering.raw(r#"{"name": 7}"#.to_owned());
        answering.write("name".to_owned(), Some(json!("Grace")));
        assert_eq!(answering.shown, Surface::Form);
        assert_eq!(
            answering.content(),
            Ok(Some(content("name", json!("Grace"))))
        );
        assert_eq!(
            answering.written, "{\n  \"name\": \"Grace\"\n}",
            "and the raw tab follows the form until it is written in again"
        );
    }

    #[test]
    fn the_only_answer_this_panel_refuses_is_a_shape_the_method_does_not_have() {
        // A schema violation is the point (§7.8), so nothing about the agent's
        // own constraints can refuse. `content` being an object or absent is
        // not a constraint of the agent's — it is what the method *is*.
        let mut answering = Answering::new(None);

        answering.raw("[1, 2]".to_owned());
        assert_eq!(
            answering.content(),
            Err("`content` is an object or nothing — that is the shape the method has.".to_owned())
        );

        answering.raw("{oh no".to_owned());
        assert!(
            answering.content().is_err(),
            "and text that is not JSON at all says what the parser said"
        );

        answering.raw(r#"{"age": "old", "extra": {"nested": true}}"#.to_owned());
        let sent = answering
            .content()
            .expect("wrongly typed values are sendable");
        let sent = sent.expect("and they are content");
        assert_eq!(sent.get("age"), Some(&json!("old")));
        assert_eq!(
            sent.get("extra"),
            Some(&json!({"nested": true})),
            "including shapes the typed answer path could not have expressed"
        );
    }

    #[test]
    fn an_empty_form_sends_nothing_and_an_empty_object_sends_itself() {
        // Two different answers that look alike. Nothing filled in is an
        // acceptance with no content, which the specification permits; `{}`
        // typed by hand is a thing the reader wrote, and rewriting it to
        // *nothing* would be this window editing an answer on its way out.
        let mut answering = Answering::new(None);
        assert_eq!(answering.content(), Ok(None), "an empty form sends nothing");

        answering.raw("   ".to_owned());
        assert_eq!(answering.content(), Ok(None), "and so does an empty tab");

        answering.raw("{}".to_owned());
        assert_eq!(
            answering.content(),
            Ok(Some(Map::new())),
            "but an object the reader wrote is sent as itself"
        );
    }

    #[test]
    fn every_property_kind_the_schema_defines_draws_the_control_it_calls_for() {
        // The table §7.8 states, asserted where it is drawn: a string is a text
        // box, a string with values is a select, numbers are numbers, a boolean
        // is a switch and an array is the set of its choices.
        let html = form_of(
            schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "email": {"type": "string", "format": "email"},
                    "born": {"type": "string", "format": "date"},
                    "priority": {"type": "string", "enum": ["low", "high"]},
                    "size": {"type": "string", "oneOf": [{"const": "s", "title": "Small"}]},
                    "age": {"type": "integer", "minimum": 0, "maximum": 120},
                    "confidence": {"type": "number"},
                    "confirmed": {"type": "boolean"},
                    "tags": {"type": "array", "items": {"type": "string", "enum": ["acp"]}}
                }
            })),
            true,
        );

        for name in [
            "name",
            "email",
            "born",
            "priority",
            "size",
            "age",
            "confidence",
            "confirmed",
            "tags",
        ] {
            assert!(
                html.contains(&format!(r#"data-field="{name}""#)),
                "{name} has a field: {html}"
            );
        }
        assert!(
            html.contains(r#"type="email""#),
            "a format is an input type: {html}"
        );
        assert!(html.contains(r#"type="date""#), "{html}");
        assert!(html.contains(r#"type="number""#), "{html}");
        assert!(html.contains(r#"type="checkbox""#), "{html}");
        assert!(html.contains("<select"), "values are a select: {html}");
        assert!(
            html.contains("Small"),
            "a titled option reads as its title: {html}"
        );
        assert!(
            html.contains("acp"),
            "and a multi-select draws its choices: {html}"
        );
    }

    #[test]
    fn a_property_type_this_window_cannot_draw_gets_no_control_and_shows_its_schema() {
        // The specification says a client that does not understand a property
        // type must not render it as one it does, and §8 says the same thing
        // about everything else: what arrived is shown as it arrived.
        let html = form_of(
            schema(serde_json::json!({
                "type": "object",
                "properties": {"holo": {"type": "_holographic", "depth": 3}}
            })),
            true,
        );

        assert!(
            html.contains(r#"data-slot="unrenderable""#),
            "it says it has no control for this: {html}"
        );
        assert!(
            !html.contains("<input") && !html.contains("<select"),
            "and draws none: {html}"
        );
        assert!(
            html.contains("_holographic") && html.contains("depth"),
            "the schema is shown as it arrived: {html}"
        );
    }

    #[test]
    fn constraints_are_reported_and_never_enforced() {
        // §7.8's rule where a reader meets it: the agent's own limits are on
        // screen, and not one of them is a gate. A control that refused `age:
        // 999` would be the tool deciding which agent behaviour is worth
        // finding out about.
        let html = form_of(
            schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "age": {"type": "integer", "minimum": 0, "maximum": 120},
                    "name": {"type": "string", "minLength": 2, "pattern": "^[a-z]+$"}
                },
                "required": ["age"]
            })),
            true,
        );

        assert!(html.contains("at least 0, at most 120"), "{html}");
        assert!(
            html.contains("at least 2 characters") && html.contains("matching ^[a-z]+$"),
            "{html}"
        );
        assert!(
            html.contains("required"),
            "and what it said was required: {html}"
        );
        for gate in [r#"min="0""#, r#"max="120""#, "required=", "minlength="] {
            assert!(
                !html.contains(gate),
                "no constraint is a gate on the control: {gate} in {html}"
            );
        }
    }

    #[test]
    fn defaults_the_agent_supplied_are_filled_in() {
        // The specification asks a client that supports defaults to
        // pre-populate them, and it is what makes a ten-field form one click to
        // answer.
        let schema = schema(serde_json::json!({
            "type": "object",
            "properties": {
                "name": {"type": "string", "default": "Ada"},
                "age": {"type": "integer", "default": 36},
                "confirmed": {"type": "boolean", "default": true},
                "nothing": {"type": "string"}
            }
        }));

        let filled = defaults(&schema);
        assert_eq!(filled.get("name").and_then(Value::as_str), Some("Ada"));
        assert_eq!(filled.get("age").and_then(Value::as_i64), Some(36));
        assert_eq!(filled.get("confirmed").and_then(Value::as_bool), Some(true));
        assert!(
            !filled.contains_key("nothing"),
            "a field with no default starts absent rather than empty: {filled:?}"
        );

        let html = form_of(schema, true);
        assert!(html.contains(r#"value="Ada""#), "{html}");
        assert!(
            html.contains(r#"checked="true""#) || html.contains("checked"),
            "{html}"
        );
    }

    #[test]
    fn the_three_answers_are_always_offered_and_keep_their_focus() {
        // Clear decline *and* cancel controls are what the specification asks
        // of a client, and all three carry the window's guarded-not-disabled
        // contract: an activated button keeps its DOM node and its keyboard
        // position while the answer is in flight (§9).
        let waiting = answers_of(ElicitationState::Waiting, true);

        for slot in ["accept", "decline", "cancel"] {
            assert!(
                waiting.contains(&format!(r#"data-answer="{slot}""#)),
                "{slot} is offered: {waiting}"
            );
        }
        assert!(
            !waiting.contains("disabled"),
            "a waiting answer is operable: {waiting}"
        );

        let sent = answers_of(
            ElicitationState::Answered(ElicitationAnswer::Declined),
            false,
        );
        assert!(
            sent.contains(r#"aria-disabled="true""#) && sent.contains(r#"tabindex="-1""#),
            "an answered one guards rather than dropping focus: {sent}"
        );
        assert!(
            !sent.contains(" disabled"),
            "and is never natively disabled: {sent}"
        );
        assert!(
            sent.contains("picked"),
            "what was sent stays legible: {sent}"
        );
    }

    #[test]
    fn a_url_is_shown_in_full_beside_its_host_and_is_opened_by_nothing_here() {
        // Everything the specification asks of a client before anything is
        // opened: the full URL, the host it goes to, consent taken by the
        // reader's own click — and nothing fetched in the meantime.
        let html = url_of(
            "https://accounts.example.com/oauth?state=9".to_owned(),
            false,
        );

        assert!(
            html.contains("https://accounts.example.com/oauth?state=9"),
            "in full: {html}"
        );
        assert!(
            html.contains("accounts.example.com"),
            "and its host, called out: {html}"
        );
        assert!(
            html.contains("Opens in your browser"),
            "which says where it goes: {html}"
        );
        assert!(
            html.matches("<a").count() == 1 && !html.contains("<button"),
            "one control, and it is the link: {html}"
        );

        let completed = url_of("https://example.com/x".to_owned(), true);
        assert!(
            completed.contains(r#"data-slot="completed""#),
            "and the agent's own word for finished is drawn where it lands: {completed}"
        );
    }

    #[test]
    fn a_host_is_read_out_of_the_url_or_said_not_to_be() {
        assert_eq!(
            host("https://example.com/x?y#z").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            host("http://localhost:8080").as_deref(),
            Some("localhost:8080")
        );
        assert_eq!(host("not a url"), None);
    }

    #[test]
    fn accepting_a_url_says_it_is_consent_rather_than_completion() {
        // The distinction the protocol makes and a reader would not: an accept
        // means somebody agreed to go, and the agent has not said the thing
        // they went to do is done.
        let consented = said_of(
            ElicitationState::Answered(ElicitationAnswer::Accepted(None)),
            false,
            true,
        );
        assert!(
            consented.contains("not completion"),
            "an accepted URL says what it did not claim: {consented}"
        );

        let finished = said_of(
            ElicitationState::Answered(ElicitationAnswer::Accepted(None)),
            true,
            true,
        );
        assert!(
            !finished.contains("not completion"),
            "and stops saying it once the agent has: {finished}"
        );
    }

    #[test]
    fn the_tool_call_a_question_came_from_is_reached_by_focus_and_not_only_by_scrolling() {
        assert!(
            FOCUS_TOOL_CALL.contains("scrollIntoView"),
            "{FOCUS_TOOL_CALL}"
        );
        assert!(
            FOCUS_TOOL_CALL.contains("target.focus({ preventScroll: true })"),
            "{FOCUS_TOOL_CALL}"
        );
        assert!(
            FOCUS_TOOL_CALL.contains("CSS.escape(id)"),
            "an id the agent chose is escaped before it reaches a selector: {FOCUS_TOOL_CALL}"
        );
    }
}
