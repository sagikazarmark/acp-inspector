//! Renderings for decoded `session/update`s: one for each of the fourteen
//! stable v1 variants that schema 1.11 exposes (`crates/core/tests/typed.rs`
//! names the same set). Unstable variants are not decoded. See
//! `docs/architecture.md` §7.2 and §8.
//!
//! The schema's enums are `#[non_exhaustive]`, so every `match` ends in a `_`
//! arm that says it has no rendering; the timeline then opens the raw JSON.
//! Nothing here rewrites what the agent sent.

use acp_inspector_core::v1;
use dioxus::prelude::*;

use crate::copy::Copy;
use crate::disclosure;

/// Whose words a streamed chunk is: one per chunk variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Voice {
    User,
    Agent,
    Thought,
}

impl Voice {
    /// The label (the wire's own variant name) and tone for this voice's chunks.
    pub fn told(self) -> (&'static str, &'static str) {
        match self {
            Self::User => ("user_message_chunk", "user"),
            Self::Agent => ("agent_message_chunk", "agent"),
            Self::Thought => ("agent_thought_chunk", "thought"),
        }
    }
}

/// The voice and chunk of a streamed-chunk update, or `None` for any other.
pub fn spoken(update: &v1::SessionUpdate) -> Option<(Voice, &v1::ContentChunk)> {
    Some(match update {
        v1::SessionUpdate::UserMessageChunk(chunk) => (Voice::User, chunk),
        v1::SessionUpdate::AgentMessageChunk(chunk) => (Voice::Agent, chunk),
        v1::SessionUpdate::AgentThoughtChunk(chunk) => (Voice::Thought, chunk),
        _ => return None,
    })
}

/// An update's label, tone and rendering, or `None` for a variant with no
/// rendering (the timeline shows its JSON instead).
pub fn render(update: &v1::SessionUpdate) -> Option<(&'static str, &'static str, Element)> {
    if let Some((voice, chunk)) = spoken(update) {
        let (label, tone) = voice.told();
        return Some((label, tone, content(&chunk.content)));
    }

    Some(match update {
        v1::SessionUpdate::ToolCall(call) => ("tool call", "tool", tool_call(call)),
        v1::SessionUpdate::ToolCallUpdate(update) => {
            ("tool call", "tool", tool_call_update(update))
        }
        v1::SessionUpdate::Plan(plan) => ("plan", "plan", plan_entries(plan)),
        v1::SessionUpdate::AvailableCommandsUpdate(update) => {
            ("commands", "note", commands(update))
        }
        v1::SessionUpdate::CurrentModeUpdate(update) => (
            "mode",
            "note",
            rsx! {
                p { "The session is now in mode " code { "{update.current_mode_id}" } "." }
            },
        ),
        v1::SessionUpdate::ConfigOptionUpdate(update) => ("config", "note", config(update)),
        v1::SessionUpdate::SessionInfoUpdate(update) => ("session", "note", info(update)),
        v1::SessionUpdate::UsageUpdate(update) => ("usage", "note", usage(update)),
        v1::SessionUpdate::Notice(notice) => ("notice", "note", noticed(notice)),
        // Core folds a compaction's summary chunks into its update, so a
        // compaction is one card however many frames it took.
        v1::SessionUpdate::CompactionUpdate(update) => ("compaction", "note", compaction(update)),
        v1::SessionUpdate::CompactionSummaryChunk(chunk) => (
            "compaction",
            "note",
            rsx! {
                div { class: "evidence", "data-slot": "compaction",
                    div { class: "evidence-head",
                        strong { class: "evidence-title", "Context compaction" }
                        code { class: "tag evidence-kind", "{chunk.compaction_id}" }
                    }
                    div { class: "said", {content(&chunk.content)} }
                }
            },
        ),
        _ => return None,
    })
}

/// Information the agent showed the reader outside the session's history:
/// its severity, title and description, as sent.
fn noticed(notice: &v1::Notice) -> Element {
    let (severity, tone) = match &notice.severity {
        v1::NoticeSeverity::Info => ("info".to_owned(), ""),
        v1::NoticeSeverity::Warning => ("warning".to_owned(), "warn"),
        v1::NoticeSeverity::Error => ("error".to_owned(), "bad"),
        v1::NoticeSeverity::Other(other) => (other.clone(), ""),
        _ => ("a severity this window has no name for".to_owned(), ""),
    };
    rsx! {
        div { class: "evidence", "data-slot": "notice",
            div { class: "evidence-head",
                strong { class: "evidence-title", "{notice.title}" }
                span { class: "evidence-state {tone}", "{severity}" }
            }
            if let Some(description) = &notice.description {
                p { class: "evidence-note", "{description}" }
            }
        }
    }
}

/// A context compaction: where it stands, its summary, and its error, with the
/// three patch states (absent, cleared, set) kept apart.
fn compaction(update: &v1::CompactionUpdate) -> Element {
    let (status, tone) = match &update.status {
        v1::CompactionStatus::InProgress => ("in_progress".to_owned(), "working"),
        v1::CompactionStatus::Completed => ("completed".to_owned(), "done"),
        v1::CompactionStatus::Failed => ("failed".to_owned(), "bad"),
        v1::CompactionStatus::Cancelled => ("cancelled".to_owned(), "waiting"),
        v1::CompactionStatus::Other(other) => (other.clone(), "waiting"),
        _ => ("unrecognized status".to_owned(), "waiting"),
    };
    let summary: Option<Option<Vec<v1::ContentBlock>>> = update.summary.clone().into();
    let error: Option<Option<String>> = update.error.clone().into();

    rsx! {
        div { class: "evidence", "data-slot": "compaction",
            div { class: "evidence-head",
                span { class: "dot dot-{marked(tone)}", aria_hidden: "true" }
                strong { class: "evidence-title", "Context compaction" }
                code { class: "tag evidence-kind", title: "compactionId", "{update.compaction_id}" }
                span { class: "evidence-state {tone}", "{status}" }
            }
            {match summary {
                Some(Some(blocks)) if !blocks.is_empty() => rsx! {
                    div { class: "said", "data-slot": "compaction-summary",
                        for (position, block) in blocks.iter().enumerate() {
                            span { key: "{position}", {content(block)} }
                        }
                    }
                },
                Some(_) => rsx! { p { class: "evidence-note", "Summary cleared." } },
                None => rsx! {},
            }}
            {match error {
                Some(Some(error)) => rsx! { p { class: "evidence-note", "data-slot": "compaction-error", "Error: {error}" } },
                _ => rsx! {},
            }}
        }
    }
}

/// One content block, wherever one turns up: in a message, in a thought, in a
/// tool call's output.
pub fn content(block: &v1::ContentBlock) -> Element {
    match block {
        // Bare text, so a message's chunks read as one message.
        v1::ContentBlock::Text(text) => rsx! { Windowed { text: text.text.clone() } },
        v1::ContentBlock::Image(image) => rsx! {
            figure { class: "media",
                img {
                    src: "data:{image.mime_type};base64,{image.data}",
                    alt: "an image the agent sent",
                }
                figcaption { class: "mono", "{image.mime_type}" }
            }
        },
        // No player; the bytes are in the frame.
        v1::ContentBlock::Audio(audio) => rsx! {
            span { class: "media mono", "audio · {audio.mime_type}" }
        },
        v1::ContentBlock::ResourceLink(link) => rsx! {
            span { class: "resource",
                code { "{link.uri}" }
                if let Some(description) = &link.description {
                    " — {description}"
                }
            }
        },
        v1::ContentBlock::Resource(resource) => embedded(&resource.resource),
        _ => rsx! {
            span { class: "unnamed", "content this window has no rendering for" }
        },
    }
}

/// A resource the agent embedded rather than linked.
fn embedded(resource: &v1::EmbeddedResourceResource) -> Element {
    match resource {
        v1::EmbeddedResourceResource::TextResourceContents(text) => rsx! {
            div { class: "resource",
                code { class: "mono", "{text.uri}" }
                Windowed { text: text.text.clone(), class: "quoted" }
            }
        },
        // Opaque base64: its size is the useful fact.
        v1::EmbeddedResourceResource::BlobResourceContents(blob) => rsx! {
            span { class: "resource",
                code { "{blob.uri}" }
                " — {blob.blob.len()} base64 characters"
            }
        },
        _ => rsx! {
            span { class: "unnamed", "an embedded resource this window has no rendering for" }
        },
    }
}

/// A tool call, with its updates already merged in by the store (§11 seam 3).
fn tool_call(call: &v1::ToolCall) -> Element {
    let (status, tone) = status(call.status);

    rsx! {
        div { class: "evidence", "data-slot": "tool-call",
            div { class: "evidence-head",
                span { class: "dot dot-{marked(tone)}", aria_hidden: "true" }
                strong { class: "evidence-title", "{call.title}" }
                {tool_name(call.name.as_deref())}
                span { class: "tag evidence-kind", "{kind(call.kind)}" }
                span { class: "evidence-state {tone}", "{status}" }
            }
            {call_id(&call.tool_call_id)}
            {locations(&call.locations)}
            {outputs(&call.content)}
            {raw(&call.tool_call_id, "input", call.raw_input.as_ref())}
            {raw(&call.tool_call_id, "output", call.raw_output.as_ref())}
        }
    }
}

/// An update for a tool call the session never saw created, drawn with only
/// the fields it carried. Also how a permission request draws its tool call.
pub fn tool_call_update(update: &v1::ToolCallUpdate) -> Element {
    let fields = &update.fields;
    let told = fields.status.map(status);

    rsx! {
        div { class: "evidence", "data-slot": "tool-call",
            div { class: "evidence-head",
                if let Some((_, tone)) = told {
                    span { class: "dot dot-{marked(tone)}", aria_hidden: "true" }
                }
                strong { class: "evidence-title",
                    {match &fields.title {
                        Some(title) => rsx! { "{title}" },
                        None => rsx! { code { "{update.tool_call_id}" } },
                    }}
                }
                {tool_name(fields.name.as_deref())}
                if let Some(doing) = fields.kind {
                    span { class: "tag evidence-kind", "{kind(doing)}" }
                }
                if let Some((what, tone)) = told {
                    span { class: "evidence-state {tone}", "{what}" }
                }
            }
            // Drawn here only where the title above is not already the id.
            if fields.title.is_some() {
                {call_id(&update.tool_call_id)}
            }
            p { class: "evidence-note",
                "An update for a tool call this session never saw created."
            }
            if let Some(within) = &fields.locations {
                {locations(within)}
            }
            if let Some(produced) = &fields.content {
                {outputs(produced)}
            }
            {raw(&update.tool_call_id, "input", fields.raw_input.as_ref())}
            {raw(&update.tool_call_id, "output", fields.raw_output.as_ref())}
        }
    }
}

/// The tool's programmatic name, where the agent gave one (schema 1.8).
fn tool_name(name: Option<&str>) -> Element {
    rsx! {
        if let Some(name) = name {
            code { class: "tag evidence-kind mono", "data-slot": "tool-name", title: "name", "{name}" }
        }
    }
}

/// The tool call's wire id, which a reader searches the Trace for.
fn call_id(id: &v1::ToolCallId) -> Element {
    rsx! {
        div { class: "evidence-meta", "data-slot": "tool-call-id",
            span { class: "sr-only", "toolCallId " }
            code { class: "mono", title: "toolCallId", "{id}" }
            Copy { text: id.to_string(), what: "the tool call id" }
        }
    }
}

/// The files a tool call said it was working in.
fn locations(locations: &[v1::ToolCallLocation]) -> Element {
    rsx! {
        if !locations.is_empty() {
            ul { class: "locations",
                for (position, location) in locations.iter().enumerate() {
                    li { key: "{position}",
                        "{location.path.display()}"
                        if let Some(line) = location.line {
                            ":{line}"
                        }
                    }
                }
            }
        }
    }
}

/// What a tool call produced.
fn outputs(outputs: &[v1::ToolCallContent]) -> Element {
    rsx! {
        for (position, output) in outputs.iter().enumerate() {
            div { key: "{position}", class: "evidence-row",
                {match output {
                    v1::ToolCallContent::Content(block) => content(&block.content),
                    v1::ToolCallContent::Diff(diff) => rsx! {
                        div {
                            class: "diff",
                            role: "region",
                            aria_label: "Diff for {diff.path.display()}",
                            code { "{diff.path.display()}" }
                            if let Some(old) = &diff.old_text {
                                del { class: "diff-version removed",
                                    span { class: "sr-only", "Removed content:" }
                                    Windowed { text: old.clone(), class: "quoted" }
                                }
                            }
                            ins { class: "diff-version added",
                                span { class: "sr-only", "Added content:" }
                                Windowed { text: diff.new_text.clone(), class: "quoted" }
                            }
                        }
                    },
                    // No terminal is embedded (§7.3), so the id is all there is.
                    v1::ToolCallContent::Terminal(terminal) => rsx! {
                        span { class: "resource mono", "terminal {terminal.terminal_id}" }
                    },
                    _ => rsx! {
                        span { class: "unnamed", "tool output this window has no rendering for" }
                    },
                }}
            }
        }
    }
}

/// A tool call's raw input or output, as the JSON it arrived as.
///
/// Drawn through the indentation switch (`crate::indent`), laid out only while
/// its disclosure is open.
fn raw(
    call: &v1::ToolCallId,
    what: &'static str,
    value: Option<&impl std::fmt::Display>,
) -> Element {
    let disclosure = named(call, what);
    let toggling = disclosure.clone();

    rsx! {
        if let Some(value) = value {
            details { class: "disclose", "data-slot": "tool-raw",
                summary {
                    aria_label: "Show or hide raw {what}",
                    title: "Show or hide raw {what}",
                    onclick: move |event| {
                        // Not a press of the Timeline row. Propagation only: the
                        // native toggle still runs.
                        event.stop_propagation();
                        crate::indent::toggled(&toggling);
                    },
                    {disclosure::chevron()}
                    "{what}"
                }
                // Copy in the block's corner, where it stays as the block scrolls.
                div { class: "quoted-block",
                    Windowed {
                        text: crate::indent::drawn(&disclosure, false, &value.to_string()),
                        class: "quoted",
                    }
                    div { class: "quoted-copy",
                        // The value as it arrived, whatever the layout above says.
                        Copy { text: value.to_string(), what: "raw {what}" }
                    }
                }
            }
        }
    }
}

/// How much of one piece of tool content is drawn before the reader asks for
/// the rest: every byte drawn is laid out again on each Timeline render.
pub(crate) const WINDOW: usize = 64 * 1024;

/// Text drawn up to [`WINDOW`] bytes, with a control for the rest. In a `pre`
/// of `class` where one is given, inline otherwise; the cut is at a character
/// boundary.
#[component]
pub(crate) fn Windowed(text: String, #[props(into, default)] class: Option<String>) -> Element {
    let mut whole = use_signal(|| false);
    let cut = text.len() > WINDOW && !whole();
    let shown = if cut {
        crate::page::prefix(&text, WINDOW)
    } else {
        text.as_str()
    };
    let kib = text.len().div_ceil(1024);

    rsx! {
        if let Some(class) = class {
            pre { class: "{class}", "{shown}" }
        } else {
            "{shown}"
        }
        if cut {
            span { class: "hint windowed", "data-slot": "windowed",
                " … first {WINDOW / 1024} KiB of {kib} KiB. "
                button {
                    class: "btn btn-xs btn-quiet",
                    r#type: "button",
                    "data-slot": "show-all",
                    onclick: move |event| {
                        event.stop_propagation();
                        whole.set(true);
                    },
                    "Show all"
                }
            }
        }
    }
}

/// The name a tool call's raw input or output has in the open-disclosure set
/// (`crate::indent`), shared by the toggle and the laid-out block.
///
/// Keyed on the call, not where it is drawn: a call shown twice (beside a
/// permission request) is marked open in both places.
fn named(call: &v1::ToolCallId, what: &str) -> String {
    format!("tool {call} {what}")
}

/// The agent's plan for the turn.
fn plan_entries(plan: &v1::Plan) -> Element {
    rsx! {
        div { class: "evidence", "data-slot": "plan",
            div { class: "evidence-head",
                span { class: "wire-name", "plan · {plan.entries.len()} entries" }
            }
            ul { class: "plan",
                for (position, entry) in plan.entries.iter().enumerate() {
                    {plan_entry(position, entry)}
                }
            }
        }
    }
}

/// One plan entry, as a line of running text.
fn plan_entry(position: usize, entry: &v1::PlanEntry) -> Element {
    let (mark, tone, status) = planned(&entry.status);
    let priority = priority(&entry.priority);

    rsx! {
        li { key: "{position}", class: "evidence-row {tone}",
            span { class: "dot dot-{mark}", aria_hidden: "true" }
            span { class: "sr-only", "{status}: " }
            span { class: "grow", "{entry.content}" }
            span { class: "priority",
                "{priority}"
                span { class: "sr-only", " priority" }
            }
        }
    }
}

/// The commands the agent says it can run.
fn commands(update: &v1::AvailableCommandsUpdate) -> Element {
    rsx! {
        div { class: "evidence", "data-slot": "commands",
            div { class: "evidence-head",
                span { class: "wire-name", "available_commands_update · {update.available_commands.len()}" }
            }
            ul { class: "plan",
                for (position, command) in update.available_commands.iter().enumerate() {
                    li { key: "{position}", class: "evidence-row",
                        code { class: "mono", "/{command.name}" }
                        span { class: "grow", "{command.description}" }
                    }
                }
            }
        }
    }
}

/// The session's configuration, as the agent now holds it.
fn config(update: &v1::ConfigOptionUpdate) -> Element {
    rsx! {
        div { class: "evidence", "data-slot": "config",
            ul { class: "plan",
                for (position, option) in update.config_options.iter().enumerate() {
                    li { key: "{position}", class: "evidence-row",
                        span { class: "grow", "{option.name}" }
                        code { class: "mono",
                            {match current_value(&option.kind) {
                                Some(value) => rsx! { "{value}" },
                                None => rsx! { "a value this window has no rendering for" },
                            }}
                        }
                    }
                }
            }
        }
    }
}

/// What a config option is set to, as the wire carries it, or `None` for a
/// kind this window cannot read. Shared with
/// [`session_settings`](crate::session_settings).
pub fn current_value(kind: &v1::SessionConfigKind) -> Option<String> {
    match kind {
        v1::SessionConfigKind::Select(select) => Some(select.current_value.to_string()),
        v1::SessionConfigKind::Boolean(boolean) => Some(boolean.current_value.to_string()),
        _ => None,
    }
}

/// A session info update. Both fields are `MaybeUndefined`, and absent,
/// cleared and set are drawn differently.
fn info(update: &v1::SessionInfoUpdate) -> Element {
    let title: Option<Option<String>> = update.title.clone().into();
    let updated: Option<Option<String>> = update.updated_at.clone().into();

    rsx! {
        div { class: "evidence", "data-slot": "session-info",
            ul { class: "plan",
                if let Some(title) = title {
                    li { class: "evidence-row",
                        span { class: "grow", "title" }
                        {match title {
                            Some(title) => rsx! { code { class: "mono", "{title}" } },
                            None => rsx! { span { class: "cleared", "cleared" } },
                        }}
                    }
                }
                if let Some(updated) = updated {
                    li { class: "evidence-row",
                        span { class: "grow", "updated" }
                        {match updated {
                            Some(updated) => rsx! { code { class: "mono", "{updated}" } },
                            None => rsx! { span { class: "cleared", "cleared" } },
                        }}
                    }
                }
            }
        }
    }
}

/// What the turn has cost so far.
fn usage(update: &v1::UsageUpdate) -> Element {
    rsx! {
        p {
            "{update.used} of {update.size} tokens in context"
            if let Some(cost) = &update.cost {
                " · {cost.amount} {cost.currency}"
            }
        }
    }
}

/// Where a tool call stands, and the tone it reads in.
fn status(status: v1::ToolCallStatus) -> (&'static str, &'static str) {
    match status {
        // The wire's own values, so the card matches the frame.
        v1::ToolCallStatus::Pending => ("pending", "waiting"),
        v1::ToolCallStatus::InProgress => ("in_progress", "working"),
        v1::ToolCallStatus::Completed => ("completed", "done"),
        v1::ToolCallStatus::Failed => ("failed", "bad"),
        _ => ("unrecognized status", "waiting"),
    }
}

/// What kind of thing a tool call is doing, in the protocol's own word for it.
fn kind(kind: v1::ToolKind) -> &'static str {
    match kind {
        v1::ToolKind::Read => "read",
        v1::ToolKind::Edit => "edit",
        v1::ToolKind::Delete => "delete",
        v1::ToolKind::Move => "move",
        v1::ToolKind::Search => "search",
        v1::ToolKind::Execute => "execute",
        v1::ToolKind::Think => "think",
        v1::ToolKind::Fetch => "fetch",
        v1::ToolKind::SwitchMode => "switch mode",
        v1::ToolKind::Other => "other",
        _ => "unrecognized kind",
    }
}

/// The window-wide dot for a tool call's tone. Decorative: the status word
/// beside it says the same.
fn marked(tone: &str) -> &'static str {
    match tone {
        "working" => "wait",
        "done" => "done",
        "bad" => "gone",
        // `waiting`, and anything unrecognised.
        _ => "idle",
    }
}

/// Where a plan entry stands: its decorative mark, visual tone and spoken name.
fn planned(status: &v1::PlanEntryStatus) -> (&'static str, &'static str, &'static str) {
    match status {
        v1::PlanEntryStatus::Pending => ("idle", "waiting", "pending"),
        v1::PlanEntryStatus::InProgress => ("wait", "working", "in progress"),
        v1::PlanEntryStatus::Completed => ("done", "done", "completed"),
        _ => ("idle", "unnamed", "unrecognized status"),
    }
}

fn priority(priority: &v1::PlanEntryPriority) -> &'static str {
    match priority {
        v1::PlanEntryPriority::High => "high",
        v1::PlanEntryPriority::Medium => "medium",
        v1::PlanEntryPriority::Low => "low",
        _ => "unrecognized priority",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One of each stable v1 `sessionUpdate` variant. Written by hand: a
    /// `#[non_exhaustive]` enum cannot be enumerated.
    fn every_variant() -> Vec<v1::SessionUpdate> {
        let chunk = || v1::ContentChunk::new(v1::ContentBlock::from("hello"));

        vec![
            v1::SessionUpdate::UserMessageChunk(chunk()),
            v1::SessionUpdate::AgentMessageChunk(chunk()),
            v1::SessionUpdate::AgentThoughtChunk(chunk()),
            v1::SessionUpdate::ToolCall(v1::ToolCall::new("call-1", "Reading a file")),
            v1::SessionUpdate::ToolCallUpdate(v1::ToolCallUpdate::new(
                "call-1",
                v1::ToolCallUpdateFields::default(),
            )),
            v1::SessionUpdate::Plan(v1::Plan::new(vec![])),
            v1::SessionUpdate::AvailableCommandsUpdate(v1::AvailableCommandsUpdate::new(vec![])),
            v1::SessionUpdate::CurrentModeUpdate(v1::CurrentModeUpdate::new("ask")),
            v1::SessionUpdate::ConfigOptionUpdate(v1::ConfigOptionUpdate::new(vec![])),
            v1::SessionUpdate::SessionInfoUpdate(v1::SessionInfoUpdate::new()),
            v1::SessionUpdate::UsageUpdate(v1::UsageUpdate::new(120, 200_000)),
            v1::SessionUpdate::Notice(v1::Notice::new(v1::NoticeSeverity::Info, "Heads up")),
            v1::SessionUpdate::CompactionUpdate(v1::CompactionUpdate::new(
                "k1",
                v1::CompactionStatus::InProgress,
            )),
            v1::SessionUpdate::CompactionSummaryChunk(v1::CompactionSummaryChunk::new(
                "k1",
                v1::ContentBlock::from("summary"),
            )),
        ]
    }

    #[test]
    fn every_stable_v1_variant_has_a_rendering() {
        // §7.2: every stable variant renders as itself, so the raw-first path
        // means the agent sent something new.
        let variants = every_variant();
        assert_eq!(
            variants.len(),
            14,
            "the fourteen stable variants `agent-client-protocol-schema` 1.11 exposes"
        );

        for update in variants {
            assert!(
                render(&update).is_some(),
                "{update:?} renders as itself rather than as unknown traffic"
            );
        }
    }

    #[test]
    fn a_tool_calls_name_is_on_its_card() {
        let call = v1::ToolCall::new("call-1", "Reading a file").name("read_file");
        let (_, _, body) = render(&v1::SessionUpdate::ToolCall(call)).expect("it renders");
        let html = dioxus_ssr::render_element(body);
        assert!(
            html.contains(r#"data-slot="tool-name""#) && html.contains(">read_file<"),
            "{html}"
        );

        let unnamed = v1::ToolCall::new("call-1", "Reading a file");
        let (_, _, body) = render(&v1::SessionUpdate::ToolCall(unnamed)).expect("it renders");
        assert!(!dioxus_ssr::render_element(body).contains("tool-name"));

        let update = v1::ToolCallUpdate::new(
            "call-1",
            v1::ToolCallUpdateFields::new().name("write_file".to_owned()),
        );
        let html = dioxus_ssr::render_element(tool_call_update(&update));
        assert!(html.contains(">write_file<"), "{html}");
    }

    #[test]
    fn a_notice_and_a_compaction_read_as_themselves() {
        let notice = v1::Notice::new(v1::NoticeSeverity::Warning, "Rate limited")
            .description("Retrying in 30 seconds".to_owned());
        let (label, _, body) = render(&v1::SessionUpdate::Notice(notice)).expect("it renders");
        assert_eq!(label, "notice");
        let html = dioxus_ssr::render_element(body);
        assert!(html.contains("Rate limited") && html.contains("Retrying in 30 seconds"));
        assert!(html.contains(">warning<"), "{html}");

        let compaction = v1::CompactionUpdate::new("k1", v1::CompactionStatus::Completed)
            .summary(vec![v1::ContentBlock::from("kept the plan")])
            .error(acp_inspector_core::MaybeUndefined::Null);
        let (label, _, body) =
            render(&v1::SessionUpdate::CompactionUpdate(compaction)).expect("it renders");
        assert_eq!(label, "compaction");
        let html = dioxus_ssr::render_element(body);
        assert!(
            html.contains("Context compaction") && html.contains(">completed<"),
            "{html}"
        );
        assert!(html.contains("kept the plan"), "{html}");
        assert!(
            !html.contains("compaction-error"),
            "a cleared error is no error: {html}"
        );
    }

    #[test]
    fn structured_timeline_families_use_quiet_bounded_groups() {
        for update in every_variant().into_iter().filter(|update| {
            matches!(
                update,
                v1::SessionUpdate::Plan(_)
                    | v1::SessionUpdate::AvailableCommandsUpdate(_)
                    | v1::SessionUpdate::ConfigOptionUpdate(_)
                    | v1::SessionUpdate::SessionInfoUpdate(_)
            )
        }) {
            let (_, _, body) = render(&update).expect("the stable family renders");
            let html = dioxus_ssr::render_element(body);
            // The window's own class rather than Tailwind utilities, so both
            // themes are styled by one declaration.
            assert!(
                html.contains(r#"class="evidence""#),
                "{update:?} is a bounded group in the flow: {html}"
            );
        }
    }

    /// A tool call carrying raw input, rendered in a virtual DOM (its disclosure
    /// needs a runtime).
    fn tool_card(indentation: acp_inspector_core::Indentation, opened: bool) -> String {
        #[component]
        fn Host(indentation: acp_inspector_core::Indentation, opened: bool) -> Element {
            crate::indent::provide(indentation);
            let call = v1::ToolCall::new("call-1", "Reading a file")
                .raw_input(serde_json::json!({"path": "settings.json"}));
            if opened {
                // What activating the summary does.
                crate::indent::toggled(&named(&call.tool_call_id, "input"));
            }
            let (_, _, body) = render(&v1::SessionUpdate::ToolCall(call)).expect("it renders");
            rsx! { {body} }
        }

        let mut dom = VirtualDom::new_with_props(
            Host,
            HostProps {
                indentation,
                opened,
            },
        );
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    #[test]
    fn tool_raw_evidence_uses_the_shared_named_outline_disclosure() {
        let html = tool_card(acp_inspector_core::Indentation::Wire, false);
        let summary = html
            .split_once("<summary")
            .expect("a tool raw-evidence disclosure")
            .1
            .split_once("</summary>")
            .expect("a complete tool raw-evidence disclosure")
            .0;

        assert!(
            summary.contains(r#"aria-label="Show or hide raw input""#)
                && summary.contains(r#"title="Show or hide raw input""#)
                && summary.contains(r#"class="disclosure-icon""#)
                && summary.contains("<svg"),
            "tool raw evidence uses the shared named outline disclosure: {summary}"
        );
    }

    #[test]
    fn a_tool_call_shows_its_id_and_its_raw_values_can_be_copied() {
        let html = tool_card(acp_inspector_core::Indentation::Wire, false);
        assert!(
            html.contains(r#"data-slot="tool-call-id""#)
                && html.contains(r#"title="toolCallId">call-1</code>"#),
            "the id the wire carries is on the card: {html}"
        );
        assert!(
            html.contains(r#"aria-label="Copy raw input""#)
                && html.contains(r#"aria-label="Copy the tool call id""#),
            "{html}"
        );
    }

    #[test]
    fn large_tool_content_is_windowed_until_the_reader_asks_for_all_of_it() {
        let big = format!("{}🦀{}", "a".repeat(WINDOW - 2), "tail".repeat(1000));
        let html = dioxus_ssr::render_element(content(&v1::ContentBlock::from(big.as_str())));
        assert!(
            html.contains(r#"data-slot="windowed""#)
                && html.contains(r#"data-slot="show-all""#)
                && !html.contains("tail")
                && html.len() < WINDOW + 2048,
            "only the first window is drawn"
        );
        let small = dioxus_ssr::render_element(content(&v1::ContentBlock::from("short")));
        assert!(small.contains("short") && !small.contains("windowed"));
    }

    #[test]
    fn opening_raw_evidence_does_not_press_the_row_around_it() {
        use std::{any::Any, rc::Rc};

        use dioxus::core::Mutation;
        use dioxus::html::{PlatformEventData, SerializedMouseData};

        #[component]
        fn Host() -> Element {
            let mut reveals = use_signal(|| 0);
            let call = v1::ToolCall::new("call-1", "Reading a file")
                .raw_input(serde_json::json!({"path": "settings.json"}));
            let (_, _, body) = render(&v1::SessionUpdate::ToolCall(call)).expect("it renders");
            rsx! {
                article { onclick: move |_| reveals += 1, {body} }
                span { "data-slot": "reveals", "{reveals}" }
            }
        }

        set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let mut dom = VirtualDom::new(Host);
        let edits = dom.rebuild_to_vec().edits;
        // The listeners in document order: the row, then the raw summary.
        let listeners: Vec<_> = edits
            .iter()
            .filter_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if name == "click" => Some(*id),
                _ => None,
            })
            .collect();
        let summary = *listeners
            .iter()
            .find(|id| {
                edits.iter().any(|edit| {
                    matches!(edit, Mutation::SetAttribute { name: "aria-label", value: dioxus::core::AttributeValue::Text(text), id: at, .. } if at == *id && text == "Show or hide raw input")
                })
            })
            .expect("the raw input summary listens for clicks");
        dom.runtime().handle_event(
            "click",
            Event::new(
                Rc::new(PlatformEventData::new(Box::<SerializedMouseData>::default()))
                    as Rc<dyn Any>,
                true,
            ),
            summary,
        );
        dom.render_immediate(&mut dioxus::core::NoOpMutations);
        assert!(
            dioxus_ssr::render(&dom).contains(r#"data-slot="reveals">0"#),
            "{}",
            dioxus_ssr::render(&dom)
        );
    }

    #[test]
    fn a_tool_calls_raw_evidence_is_laid_out_where_the_reader_opened_it() {
        let opened = tool_card(acp_inspector_core::Indentation::Indented, true);
        assert!(
            opened.contains(
                "<pre class=\"quoted\">{\n  &#34;path&#34;: &#34;settings.json&#34;\n}</pre>"
            ),
            "the arguments are laid out for reading: {opened}"
        );

        // A closed card holds the value as it arrived.
        let closed = tool_card(acp_inspector_core::Indentation::Indented, false);
        assert!(
            closed.contains("<pre class=\"quoted\">{&#34;path&#34;:&#34;settings.json&#34;}</pre>"),
            "a closed disclosure holds what arrived: {closed}"
        );
    }

    #[test]
    fn pending_permission_tool_facts_are_bounded_marks_without_a_list_bullet() {
        let update = v1::ToolCallUpdate::new(
            "call-1",
            v1::ToolCallUpdateFields::new()
                .title("Permission request")
                .kind(v1::ToolKind::Execute)
                .status(v1::ToolCallStatus::Pending),
        );
        let html = dioxus_ssr::render_element(tool_call_update(&update));
        let head = html
            .split_once(r#"<div class="evidence-head">"#)
            .and_then(|(_, rest)| rest.split_once("</div>"))
            .map(|(head, _)| head)
            .expect("the permission tool header");

        assert!(
            head.contains(r#"class="dot dot-idle""#)
                && head.contains(r#"class="evidence-title""#)
                && head.contains(r#"class="tag evidence-kind""#)
                && head.contains(r#"class="evidence-state waiting""#)
                && head.contains("pending")
                && head.contains("Permission request")
                && head.contains("execute"),
            "pending, title, and kind read as three facts on one line: {head}"
        );
    }

    #[test]
    fn plan_status_and_priority_are_text_borne() {
        let html = dioxus_ssr::render_element(plan_entries(&v1::Plan::new(vec![
            v1::PlanEntry::new(
                "Read the parser",
                v1::PlanEntryPriority::High,
                v1::PlanEntryStatus::Pending,
            ),
            v1::PlanEntry::new(
                "Change the parser",
                v1::PlanEntryPriority::Medium,
                v1::PlanEntryStatus::InProgress,
            ),
            v1::PlanEntry::new(
                "Test the parser",
                v1::PlanEntryPriority::Low,
                v1::PlanEntryStatus::Completed,
            ),
        ])));

        for (content, status, priority) in [
            ("Read the parser", "pending", "high"),
            ("Change the parser", "in progress", "medium"),
            ("Test the parser", "completed", "low"),
        ] {
            let row = html
                .split("<li")
                .find(|row| row.contains(content))
                .unwrap_or_else(|| panic!("the plan row for {content}: {html}"));
            assert!(
                row.contains(&format!(r#"class="sr-only">{status}: </span>"#))
                    && row.contains(&format!(">{priority}<"))
                    && row.contains(r#"class="sr-only"> priority</span>"#),
                "the {content} row carries its own status and priority: {row}"
            );
        }
        assert_eq!(
            html.matches(r#"aria-hidden="true""#).count(),
            3,
            "the dot beside each row is decoration once the word exists: {html}"
        );
    }

    #[test]
    fn diff_sides_are_labelled_semantic_changes() {
        let diff = v1::Diff::new("src/parser.rs", "let cap = 2000;").old_text("let cap = 100;");
        let html = dioxus_ssr::render_element(outputs(&[v1::ToolCallContent::Diff(diff)]));

        assert!(
            html.contains(r#"role="region" aria-label="Diff for src/parser.rs""#),
            "the path names the diff region: {html}"
        );
        assert!(
            html.contains(r#"<del class="diff-version removed">"#)
                && html.contains(r#"class="sr-only">Removed content:</span>"#)
                && html.contains("let cap = 100;"),
            "the old bytes are a labelled deletion: {html}"
        );
        assert!(
            html.contains(r#"<ins class="diff-version added">"#)
                && html.contains(r#"class="sr-only">Added content:</span>"#)
                && html.contains("let cap = 2000;"),
            "the new bytes are a labelled insertion: {html}"
        );
    }
}
