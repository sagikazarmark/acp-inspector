//! What a decoded `session/update` looks like (`docs/architecture.md` §7.2,
//! §9): a rendering for every stable v1 variant, and a way of saying so for the
//! ones there is no rendering for.
//!
//! **Eleven variants, not the spec's twelve.** The spec counts the stable v1
//! `sessionUpdate` variants from the schema metadata (§7.4);
//! `agent-client-protocol-schema` 1.6 exposes eleven outside its `unstable_*`
//! features, and the unstable ones are deliberately not decoded — they are
//! unknown traffic, which is displayed rather than understood (§8).
//! `crates/core/tests/typed.rs` counts the same eleven against a real agent.
//!
//! Every `match` here ends in a `_` arm, and none of them is a fallthrough for
//! tidiness: the schema's enums are `#[non_exhaustive]`, so the arm is what
//! happens when the protocol grows a variant this window has not been taught
//! yet. It says so, and the entry's raw JSON opens itself
//! ([`timeline`](crate::timeline)) — the raw-first rule as the default rather
//! than as the error path.
//!
//! Nothing here rewrites what the agent sent. Text is rendered as the text it
//! was and JSON as the JSON it was: an inspector that reordered, respelled or
//! summarised the wire would be showing its own work rather than the agent's.
//! The one thing a reader may ask for is whitespace between a payload's tokens
//! (`crate::indent`), which is off by default and changes nothing but where the
//! bytes sit on the screen.

use acp_inspector_core::v1;
use dioxus::prelude::*;

use crate::disclosure;

/// Whose words a streamed chunk is.
///
/// The three chunk variants and nothing else — a thought is the agent's too,
/// and reads differently enough to be its own voice. It lives here rather than
/// in the timeline that groups by it, because what a voice is *called* is the
/// same question as what any other update is called, and one table is what
/// keeps a run of chunks reading the same as a lone one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Voice {
    User,
    Agent,
    Thought,
}

impl Voice {
    /// What this voice's chunks are called on screen, and the tone they read
    /// in.
    ///
    /// **The variant the wire carries, not a word for it.** A reader watching
    /// this screen beside the wire log is reading two accounts of the same
    /// traffic, and one of them calling `agent_thought_chunk` a *thought* is a
    /// translation they have to hold in their head. The drawing labels these
    /// rows with the update's own name for the same reason.
    pub fn told(self) -> (&'static str, &'static str) {
        match self {
            Self::User => ("user_message_chunk", "user"),
            Self::Agent => ("agent_message_chunk", "agent"),
            Self::Thought => ("agent_thought_chunk", "thought"),
        }
    }
}

/// The voice an update is in and the chunk it carries, when it is one of the
/// three streamed-chunk variants.
///
/// The one place those three are told from everything else: the timeline groups
/// by what this answers, and renders what it hands back.
pub fn spoken(update: &v1::SessionUpdate) -> Option<(Voice, &v1::ContentChunk)> {
    Some(match update {
        v1::SessionUpdate::UserMessageChunk(chunk) => (Voice::User, chunk),
        v1::SessionUpdate::AgentMessageChunk(chunk) => (Voice::Agent, chunk),
        v1::SessionUpdate::AgentThoughtChunk(chunk) => (Voice::Thought, chunk),
        _ => return None,
    })
}

/// What an update looks like: what it is called, the tone it reads in, and the
/// update itself.
///
/// `None` is the answer for a variant this window has no rendering for, and the
/// timeline shows those as their JSON instead (§8). **Name and rendering come
/// out of one place** so that "this variant has a rendering" is one fact rather
/// than two that could drift — which is what the test at the bottom of this
/// file checks, one constructed variant at a time.
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
        _ => return None,
    })
}

/// One content block, wherever one turns up: in a message, in a thought, in a
/// tool call's output.
pub fn content(block: &v1::ContentBlock) -> Element {
    match block {
        // The text, and nothing around it. Chunks of one message sit side by
        // side in the flow, so what is on screen is the message the agent
        // streamed rather than the pieces it arrived in.
        v1::ContentBlock::Text(text) => rsx! { "{text.text}" },
        v1::ContentBlock::Image(image) => rsx! {
            figure { class: "media",
                // The bytes the agent sent, shown rather than described: an
                // image is content, and a client that only named its MIME type
                // would be hiding what it was handed.
                img {
                    src: "data:{image.mime_type};base64,{image.data}",
                    alt: "an image the agent sent",
                }
                figcaption { class: "mono", "{image.mime_type}" }
            }
        },
        // No player: sound is not something a trace reader is listening for,
        // and the bytes are in the frame underneath.
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
                pre { class: "quoted", "{text.text}" }
            }
        },
        // Base64 of something that is not text, and this window has no idea
        // what: its size is the useful fact about it.
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

/// A tool call, as the bounded inset group that merges its updates in place
/// (§11 seam 3).
///
/// One group per `toolCallId` is the store's doing: every status change, every
/// piece of output and every location the agent sent for this id has already
/// been folded into the call this renders, and the frames behind it are on the
/// entry.
fn tool_call(call: &v1::ToolCall) -> Element {
    let (status, tone) = status(call.status);

    rsx! {
        div { class: "evidence", "data-slot": "tool-call",
            div { class: "evidence-head",
                span { class: "dot dot-{marked(tone)}", aria_hidden: "true" }
                strong { class: "evidence-title", "{call.title}" }
                span { class: "evidence-kind", "{kind(call.kind)}" }
                span { class: "evidence-state {tone}", "{status}" }
            }
            {locations(&call.locations)}
            {outputs(&call.content)}
            {raw(&call.tool_call_id, "input", call.raw_input.as_ref())}
            {raw(&call.tool_call_id, "output", call.raw_output.as_ref())}
        }
    }
}

/// An update for a tool call the session has no create for.
///
/// It is an entry of its own because the store had nothing to merge it into,
/// which is a fact about the agent worth seeing: the fields it carried are what
/// it said, and the rest is what it did not.
///
/// It is also what a permission request names its tool call with
/// ([`permission`](crate::permission)) — the same shape, carrying the same
/// "only what it said" — so the card a reader is being asked about is the card
/// they already know how to read.
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
                if let Some(doing) = fields.kind {
                    span { class: "evidence-kind", "{kind(doing)}" }
                }
                if let Some((what, tone)) = told {
                    span { class: "evidence-state {tone}", "{what}" }
                }
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
                                    pre { class: "quoted", "{old}" }
                                }
                            }
                            ins { class: "diff-version added",
                                span { class: "sr-only", "Added content:" }
                                pre { class: "quoted", "{diff.new_text}" }
                            }
                        }
                    },
                    // The inspector advertises no terminal capability (§7.3),
                    // so it has no terminal to embed — the id is the whole of
                    // what it can honestly show.
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
/// Displayed rather than named as a type: the value is `serde_json`'s, and this
/// crate does not depend on a JSON library to print something that already
/// knows how to print itself — three dependencies is the shape of this crate
/// (`Cargo.toml`), and a fourth for `to_string` would not be.
///
/// The one payload on this screen that is a decoded value rather than a frame,
/// and it is drawn through the same switch as the frames (`crate::indent`): what
/// a reader turned on is a way of reading JSON, and a tool call's arguments are
/// the JSON they most often want it for. Which half of which call it is names the
/// disclosure, so this one is laid out when it is open and never before.
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
                    aria_label: "Show or hide raw {what} evidence",
                    title: "Show or hide raw {what} evidence",
                    onclick: move |_| crate::indent::toggled(&toggling),
                    {disclosure::chevron()}
                    "{what}"
                }
                pre {
                    class: "quoted",
                    "{crate::indent::drawn(&disclosure, false, &value.to_string())}"
                }
            }
        }
    }
}

/// What a tool call's raw input or output is called in the one set that holds
/// what the reader has open (`crate::indent`).
///
/// Built once and shared by the control that turns the disclosure over and the
/// block that is laid out because it did, which is how the two cannot come to
/// disagree about which disclosure they are about.
///
/// **The call, and not where it is drawn**, which is deliberate rather than
/// overlooked: one tool call is on screen twice while a permission request names
/// the card it is about ([`tool_call_update`], `crate::permission`), so opening
/// one of them marks both. The two `<details>` keep their own open state — this
/// window sets neither — so what it costs is the hidden one laying out a payload
/// nobody can see, and what it buys is a name that does not have to be threaded
/// through every surface a tool call can appear on.
fn named(call: &v1::ToolCallId, what: &str) -> String {
    format!("tool {call} {what}")
}

/// The agent's plan for the turn.
fn plan_entries(plan: &v1::Plan) -> Element {
    rsx! {
        div { class: "evidence", "data-slot": "plan",
            div { class: "evidence-head",
                span { class: "eyebrow", "plan · {plan.entries.len()} entries" }
            }
            ul { class: "plan",
                for (position, entry) in plan.entries.iter().enumerate() {
                    {plan_entry(position, entry)}
                }
            }
        }
    }
}

/// One line of running plan text rather than columns: an entry is a sentence,
/// and a column layout would have to be as wide as the longest one.
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
                span { class: "eyebrow", "available_commands_update · {update.available_commands.len()}" }
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

/// What a config option is set to, in the value the wire carries — or `None`
/// for a kind this window cannot read one out of.
///
/// Here rather than beside either of the two screens that ask, because both of
/// them ask the same question of the same protocol type: this is what an
/// announcement of a change says the option is now, and what the Session
/// Settings surface says it *is* ([`session_settings`](crate::session_settings)).
/// An option kind with no rendering has no value this window can name either,
/// and naming one anyway would be a guess about a shape it just said it does not
/// know.
pub fn current_value(kind: &v1::SessionConfigKind) -> Option<String> {
    match kind {
        v1::SessionConfigKind::Select(select) => Some(select.current_value.to_string()),
        v1::SessionConfigKind::Boolean(boolean) => Some(boolean.current_value.to_string()),
        _ => None,
    }
}

/// What the agent renamed the session, or when it last said it did something.
///
/// The two fields are `MaybeUndefined`: absent, explicitly cleared, or set. All
/// three are different things an agent can mean, so all three are shown as
/// different things.
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
        // v1's own four words rather than shorter ones for them: this is the
        // field's value, and a reader checking the card against the frame in
        // the wire log should find the same string in both.
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

/// The dot a tool call's state is drawn as.
///
/// **The window has five dots and this is which of them**: a status word is the
/// row's own vocabulary — `pending`, `in_progress` — and the mark beside it is
/// the one every other surface uses for the same four meanings, so a reader
/// learns the marks once. Nothing is said by the dot that the word beside it
/// does not say.
fn marked(tone: &str) -> &'static str {
    match tone {
        "working" => "wait",
        "done" => "done",
        "bad" => "gone",
        // `waiting`, and anything a later ring adds: nothing is happening yet.
        _ => "idle",
    }
}

/// Where a plan entry stands: its decorative mark, visual tone and spoken name.
/// Where a plan entry stands: the tone of the dot beside it, the tone of the
/// row, and the word a reader who is not looking at either hears.
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

    /// One of each stable v1 `sessionUpdate` variant the schema crate exposes.
    ///
    /// Written out rather than derived, because a `#[non_exhaustive]` enum
    /// cannot be enumerated: this list is a claim, and the day the schema grows
    /// a twelfth variant it is a claim that has to be updated by hand — which
    /// is the same hand that would be adding the arm.
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
        ]
    }

    #[test]
    fn every_stable_v1_variant_has_a_rendering() {
        // The acceptance the spec asks for (§7.2): every stable variant is
        // decoded *and* shown as itself, so that landing in the raw-first path
        // means the agent sent something new — not that this window never got
        // around to it.
        let variants = every_variant();
        assert_eq!(
            variants.len(),
            11,
            "the eleven stable variants `agent-client-protocol-schema` 1.6 exposes"
        );

        for update in variants {
            assert!(
                render(&update).is_some(),
                "{update:?} renders as itself rather than as unknown traffic"
            );
        }
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
            // A bounded box in the flow, in this window's own class rather
            // than in Tailwind's utilities: the class says which kind of box it
            // is and the stylesheet says what that looks like, so the two
            // schemes are one declaration rather than two utilities that only
            // work in one.
            assert!(
                html.contains(r#"class="evidence""#),
                "{update:?} is a bounded group in the flow: {html}"
            );
        }
    }

    /// A tool call carrying raw input, rendered inside a window: its evidence is
    /// a disclosure the reader operates, and markup carrying a control has to be
    /// built where there is a runtime to hang one on.
    fn tool_card(indentation: acp_inspector_core::Indentation, opened: bool) -> String {
        #[component]
        fn Host(indentation: acp_inspector_core::Indentation, opened: bool) -> Element {
            crate::indent::provide(indentation);
            let call = v1::ToolCall::new("call-1", "Reading a file")
                .raw_input(serde_json::json!({"path": "settings.json"}));
            if opened {
                // The same call the summary makes when it is activated, by the
                // name this card gives its disclosure — a screen test has no
                // pointer, and what a click leaves behind is this.
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
            summary.contains(r#"aria-label="Show or hide raw input evidence""#)
                && summary.contains(r#"title="Show or hide raw input evidence""#)
                && summary.contains(r#"class="disclosure-icon""#)
                && summary.contains("<svg"),
            "tool raw evidence uses the shared named outline disclosure: {summary}"
        );
    }

    #[test]
    fn a_tool_calls_raw_evidence_is_laid_out_where_the_reader_opened_it() {
        // The one payload on this screen that is a decoded value rather than a
        // frame, drawn through the same switch as the frames beside it
        // (`crate::indent`) — and it is where a reader most often wants it,
        // since a tool call's arguments are the JSON they came to read.
        let opened = tool_card(acp_inspector_core::Indentation::Indented, true);
        assert!(
            opened.contains(
                "<pre class=\"quoted\">{\n  &#34;path&#34;: &#34;settings.json&#34;\n}</pre>"
            ),
            "the arguments are laid out for reading: {opened}"
        );

        // And a card nobody opened is the value as it arrived, which is what
        // keeps a session's worth of tool calls from being laid out for nobody.
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
                && head.contains(r#"class="evidence-kind""#)
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
