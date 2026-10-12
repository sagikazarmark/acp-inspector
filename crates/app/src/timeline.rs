//! The turn timeline: the centre screen, a rendering of core's timeline store.
//! See `docs/architecture.md` §9.
//!
//! Merging is the store's job (§11 seam 3); this file decides only how entries
//! read. Streamed chunks are gathered into the message they belong to (by
//! `messageId` and adjacency) while every chunk stays an entry underneath.
//! Unrecognized traffic and conformance annotations show their frames without
//! being asked, because for them the raw frames are the point (§8).

use std::collections::HashSet;
use std::time::SystemTime;

use acp_inspector_core::{
    Annotation, CallError, EntryId, EntryKind, Recorded, TimelineEntry, Turn, TurnOutcome,
    TurnState, Unrecognized, v1,
};
use dioxus::prelude::*;

use crate::composer::{self, Composer};
use crate::copy::Copy;
use crate::elicitation;
use crate::permission::{self, Answer};
use crate::time::stamp;
use crate::update::{self, Voice};

/// The entry list's name for the scroll watcher (`tail.rs`).
pub(crate) const ENTRIES: &str = "entries";

/// Focus a Timeline entry after it has rendered.
pub(crate) fn focus_entry(ordinal: u64) {
    crate::navigation::Destination::Entry { ordinal }.focus();
}

/// The center screen: what the agent has said this session, and the composer
/// that starts the next turn.
#[component]
pub fn Timeline(
    entries: ReadSignal<Vec<TimelineEntry>>,
    #[props(default)] retention: acp_inspector_core::Retention,
    turn: TurnState,
    session: Option<v1::SessionId>,
    /// Whether the agent is still there. A session outlives its agent in the
    /// store, so a session alone does not mean it can be prompted.
    connected: bool,
    /// The half-open ordinal range the bounded Trace still holds. Timeline
    /// entries outlive cleared or aged-out Frames, so Reach is offered only
    /// while at least one Frame it names still has a destination.
    held_frames: std::ops::Range<u64>,
    /// Whether the agent is waiting on an answer: core's count, not a scan.
    blocked: bool,
    /// The captured ordinal of a frame the Console asked this screen to show.
    sought: Option<u64>,
    /// The frames of an entry, on their way to the Console.
    on_reveal: EventHandler<Vec<u64>>,
    /// Whether an agent has described itself, so a session can be opened.
    described: bool,
    /// The session's turns and their outcomes, from core. The lines between
    /// turns are drawn from this, never worked out from the rows (§5).
    turns: Vec<Turn>,
    /// Why there is no session, when the handshake that opens one failed.
    problem: Option<CallError>,
    /// Opens a session from the empty state; `None` until an agent has
    /// described itself.
    on_new_session: Option<EventHandler<()>>,
    /// The mode the next prompt goes out under, for the composer's own control
    /// (`composer::Cycle`).
    mode: Option<composer::Cycle>,
    #[props(default)] image_advertised: bool,
    #[props(default)] audio_advertised: bool,
    #[props(default)] embedded_advertised: bool,
    #[props(default)] prompt_epoch: u64,
    on_prompt: Callback<Vec<v1::ContentBlock>, Result<(), CallError>>,
    on_stop: EventHandler<()>,
    on_set_mode: EventHandler<v1::SessionModeId>,
    /// A permission option the user picked, on its way back to core.
    on_answer: EventHandler<Answer>,
    /// What a reader said to an elicitation, on its way back to core (§7.8).
    on_elicit: EventHandler<elicitation::Answer>,
    /// The agent's login guidance (`agent::login_guidance`), shown in the empty
    /// state when the agent wants a login.
    #[props(default)]
    login: Option<Element>,
    /// Recent invocations for the first-run card; `None` draws it without.
    #[props(default)]
    recent: Option<Element>,
) -> Element {
    let blocks = blocks(entries());
    let count = blocks.len();
    let commands = commands(&entries.read());
    let rows = rows(blocks, &turns);
    let crossing = Crossing {
        sought,
        reveal: on_reveal,
        held: (held_frames.start, held_frames.end),
    };

    // Paged, because every drawn row is laid out again whenever the agent
    // streams. Row positions are the ordinals `crate::page` pages by.
    let mut anchor = use_signal(|| None::<u64>);
    let positions: Vec<u64> = (0..rows.len() as u64).collect();
    let total_rows = rows.len();
    // Show the page holding the frame the Console asked for. Keyed on the ask,
    // so paging away afterwards is not undone by the next render.
    let sought_row = sought.and_then(|ordinal| rows.iter().position(|row| row.holds(ordinal)));
    use_effect(use_reactive!(|sought| {
        if sought.is_some()
            && let Some(row) = sought_row
        {
            anchor.set(Some(page_start(row, total_rows)));
        }
    }));
    let range = crate::page::range(&positions, anchor());

    // The list's one Tab stop: the last focused row while it is on the page,
    // else the newest.
    let focused = use_signal(|| None::<String>);
    // Only rows first drawn after the list mounted fade up, and only once, so
    // a region shown again does not replay every arrival.
    let baseline = use_hook(|| rows.iter().map(Row::key).collect::<HashSet<String>>());
    let settled = use_signal(HashSet::<String>::new);
    let page: Vec<&Row> = rows[range.clone()].iter().collect();
    let current = focused()
        .filter(|key| page.iter().any(|row| row.is_block() && row.key() == *key))
        .or_else(|| {
            page.iter()
                .rev()
                .find(|row| row.is_block())
                .map(|row| row.key())
        });

    // The frame of the newest waiting request of either kind (§7.8): `blocked`
    // says one is waiting, this finds the row to go to.
    let waiting = blocked
        .then(|| {
            entries
                .read()
                .iter()
                .rev()
                .find(|entry| entry.is_waiting())
                .and_then(|entry| entry.frames.iter().find_map(|recorded| recorded.captured))
        })
        .flatten();
    // And the page it is on, so going to it can turn to that page first.
    let waiting_page = waiting.and_then(|ordinal| {
        rows.iter()
            .position(|row| row.holds(ordinal))
            .map(|row| page_start(row, rows.len()))
    });
    let total = rows.len();
    let latest_page = range.end == total;

    rsx! {
        main { class: "turns",
            id: "timeline",
            tabindex: "-1",
            header { class: "pane-head",
                h2 { class: "pane-title", "Timeline" }
                // The current turn, and once it is over, how it ended.
                if let Some(turn) = turns.last() {
                    span { class: "pane-scope", "data-slot": "turn-scope",
                        title: "{v1::AGENT_METHOD_NAMES.session_prompt}",
                        if turn.outcome.is_some() { "turn {turn.id} · {over(turn)}" } else { "turn {turn.id}" }
                    }
                }
                span { class: "grow" }
                // The session's permission policy, only while there is one.
                if connected && session.is_some() {
                    crate::client_services::PolicyControl {}
                }
                {composer::badge(&composer::presented(&turn, blocked, problem.as_ref(), connected && session.is_some(), connected))}
            }

            if retention.entries > 0 || retention.turns > 0 {
                p { class: "narrowed", "data-slot": "timeline-retention",
                    "Older timeline items were removed to save memory ({retention.entries} entries, {retention.frames} frames). The trace keeps its own history."
                }
            }
            if count == 0 {
                div { class: "empty", "data-slot": "timeline-empty",
                    // Each empty state offers the one action that resolves it,
                    // where there is one.
                    if let Some(login) = login.filter(|_| session.is_none()) {
                        // A new session would be refused again until a login.
                        h3 { "Log in to continue" }
                        div { class: "empty-login", "data-slot": "empty-login", {login} }
                    } else if session.is_none() && connected {
                        h3 { "No session open" }
                        p { "Open a new session to start prompting." }
                        if let Some(open) = on_new_session {
                            button {
                                class: "btn btn-md btn-filled",
                                "data-slot": "empty-action",
                                r#type: "button",
                                title: "{v1::AGENT_METHOD_NAMES.session_new}",
                                onclick: move |_| open.call(()),
                                "New session"
                            }
                        }
                    } else if session.is_some() {
                        h3 { "Session open" }
                        p { "Send a prompt below. Agent updates appear here." }
                    } else {
                        {onboarding(recent)}
                    }
                }
            } else {
                {crate::page::controls(&positions, range.clone(), anchor, ENTRIES)}
                div { class: "tailing",
                div { class: "turn-stream", "data-tail": ENTRIES,
                    "data-page-latest": "{latest_page}",
                ol { class: "stream",
                    // One Tab stop for the list: ↑/↓/Home/End move between rows
                    // and Enter shows a row's frames.
                    onmounted: move |_| crate::console::rove(
                        &format!(r#"[data-tail="{ENTRIES}"] .stream"#),
                        "article.row",
                        Some(r#"[data-slot="reach"]"#),
                    ),
                    // Oldest first in the DOM, so reading and focus order follow
                    // the conversation. The scroller is laid out bottom-up, which
                    // keeps a streaming turn in view without a scroll script.
                    for row in page.into_iter() {
                        li {
                            key: "{row.key()}",
                            match row {
                                Row::Block(block) => block.render(
                                    on_answer,
                                    on_elicit,
                                    crossing,
                                    Roving {
                                        current: current.as_deref() == Some(row.key().as_str()),
                                        key: row.key(),
                                        focused,
                                        arrived: !baseline.contains(&row.key())
                                            && !settled.read().contains(&row.key()),
                                        settled,
                                    },
                                ),
                                Row::Ended(turn) => ended(turn),
                                Row::Outside(_) => outside(),
                            }
                        }
                    }
                }
                }
                // Outside the scroller, so it is not one of the rows.
                {crate::tail::to_latest(ENTRIES, "entry", Some(anchor))}
                }
            }

            // That the agent is waiting on the reader (§7.2). The live region is
            // always present: only text changing inside one is announced.
            div { class: "waiting", "data-slot": "waiting", role: "status", aria_live: "polite",
                if blocked {
                    span { "The agent is waiting on you." }
                    if let Some(ordinal) = waiting {
                        button {
                            class: "btn btn-xs btn-quiet",
                            "data-slot": "reach-request",
                            title: "Move focus to the request the agent is waiting on",
                            onclick: move |_| {
                                if let Some(start) = waiting_page {
                                    anchor.set((start as usize + crate::page::SIZE < total).then_some(start));
                                }
                                focus_entry(ordinal);
                            },
                            "Go to request"
                        }
                    }
                }
            }

            // Only where there is a session, or a failed handshake whose reason
            // the composer shows.
            if session.is_some() || problem.is_some() {
                Composer {
                    key: "{prompt_epoch}-{session:?}",
                    image_advertised,
                    audio_advertised,
                    embedded_advertised,
                    turn,
                    ready: connected && session.is_some(),
                    connected,
                    blocked,
                    problem,
                    commands,
                    mode,
                    on_prompt,
                    on_stop,
                    on_set_mode,
                }
            }
        }
    }
}

/// The first page start that shows `row`, in a list of `total` rows paged by
/// [`crate::page::SIZE`] — the same arithmetic `crate::page::range` clamps to.
fn page_start(row: usize, total: usize) -> u64 {
    let size = crate::page::SIZE;
    (row.saturating_sub(size / 2)).min(total.saturating_sub(size)) as u64
}

/// How a turn that is over came out, in the header's few words.
fn over(turn: &Turn) -> &'static str {
    match &turn.outcome {
        Some(TurnOutcome::Ended(v1::StopReason::EndTurn)) => "done",
        Some(TurnOutcome::Ended(v1::StopReason::Cancelled)) => "cancelled",
        Some(TurnOutcome::Ended(_)) => "stopped",
        Some(TurnOutcome::Failed(_)) => "failed",
        _ => "over",
    }
}

/// The first-run card: what this window is for, a `Connect…` button that
/// opens the launch dialog, and the recent invocations where there are any.
fn onboarding(recent: Option<Element>) -> Element {
    let palette = if cfg!(target_os = "macos") {
        "⌘K"
    } else {
        "Ctrl K"
    };
    rsx! {
        div { class: "onboarding", "data-slot": "onboarding",
            span { class: "onboarding-icon", {crate::disclosure::icon(crate::disclosure::Role::Start)} }
            h3 { "Inspect an ACP agent" }
            p { "Start your agent to see the conversation here." }
            div { class: "onboarding-actions",
                button {
                    class: "btn btn-md btn-filled",
                    "data-slot": "empty-action",
                    r#type: "button",
                    title: "Open the launch fields",
                    onclick: move |_| crate::connect::open(),
                    "Connect…"
                }
                span { class: "hint", "data-slot": "palette-hint",
                    "or press "
                    kbd { "{palette}" }
                    " for every command"
                }
            }
            if let Some(recent) = recent {
                div { class: "onboarding-recent", "data-slot": "onboarding-recent",
                    h4 { id: "onboarding-recent-label", "Recent" }
                    {recent}
                }
            }
        }
    }
}

/// What a row needs for the list's roving Tab stop and its fade-up.
#[derive(Clone)]
struct Roving {
    /// Whether this row is the list's Tab stop.
    current: bool,
    /// The row's own name, which is what the list remembers as focused.
    key: String,
    focused: Signal<Option<String>>,
    /// Whether the row arrived after the list was drawn and has not yet
    /// finished fading up.
    arrived: bool,
    /// The rows that have finished fading up, so they never do again.
    settled: Signal<HashSet<String>>,
}

impl Roving {
    fn tabindex(&self) -> &'static str {
        if self.current { "0" } else { "-1" }
    }

    /// The mark the stylesheet fades a row up by, while it is still arriving.
    fn arrived(&self) -> Option<&'static str> {
        self.arrived.then_some("true")
    }

    /// Settles the row once its own fade has run. Animations inside the row
    /// bubble here too and are ignored.
    fn settle(&self) -> impl FnMut(AnimationEvent) + 'static {
        let mut settled = self.settled;
        let key = self.key.clone();
        let arrived = self.arrived;
        move |event: AnimationEvent| {
            if arrived && event.data().animation_name() == "fadeup" {
                settled.write().insert(key.clone());
            }
        }
    }
}

/// How the Timeline and the Console point at each other, carried to each row.
#[derive(Clone, Copy)]
struct Crossing {
    /// The frame a reader came here to find, from the Console.
    sought: Option<u64>,
    /// An entry's frames, on their way to the Console.
    reveal: EventHandler<Vec<u64>>,
    /// The half-open ordinal range the bounded Trace still holds.
    held: (u64, u64),
}

impl Crossing {
    /// The ordinals of the frames these entries were made of, in order. Frames
    /// this client sent carry no ordinal (`acp_inspector_core::Recorded`).
    fn ordinals(entries: &[TimelineEntry]) -> Vec<u64> {
        entries
            .iter()
            .flat_map(|entry| entry.frames.iter().filter_map(|recorded| recorded.captured))
            .collect()
    }

    /// Whether the frame somebody came looking for is one of these.
    fn found(self, ordinals: &[u64]) -> bool {
        self.sought.is_some_and(|sought| ordinals.contains(&sought))
    }

    /// Whether a captured Frame still has a row in the Trace.
    fn holds(self, ordinal: u64) -> bool {
        (self.held.0..self.held.1).contains(&ordinal)
    }

    /// The ordinals the Trace still holds a row for.
    fn linking(self, ordinals: &[u64]) -> Vec<u64> {
        ordinals
            .iter()
            .copied()
            .filter(|ordinal| self.holds(*ordinal))
            .collect()
    }
}

/// One thing on screen: an entry, or the message a run of chunks makes up.
#[derive(Debug)]
// The big variant is also the common one; boxing it would allocate per entry
// on every render. Core's `EntryKind` carries the same allowance.
#[allow(clippy::large_enum_variant)]
enum Block {
    /// Consecutive chunks in one voice, belonging to one message.
    Speech {
        voice: Voice,
        /// The message they are chunks of, when the agent named one.
        message: Option<v1::MessageId>,
        entries: Vec<TimelineEntry>,
    },
    /// Everything else: one entry, one block.
    One(TimelineEntry),
}

/// The entries, with consecutive chunks of one message gathered into it.
///
/// A run shares a voice and a `messageId` (chunks without ids group by
/// adjacency alone). Anything between two chunks ends the run, so screen order
/// stays wire order.
fn blocks(entries: Vec<TimelineEntry>) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();

    for entry in entries {
        let Some((voice, message)) = chunk(&entry) else {
            blocks.push(Block::One(entry));
            continue;
        };
        let message = message.cloned();

        match blocks.last_mut() {
            Some(Block::Speech {
                voice: streaming,
                message: streamed,
                entries,
            }) if *streaming == voice && *streamed == message => entries.push(entry),
            _ => blocks.push(Block::Speech {
                voice,
                message,
                entries: vec![entry],
            }),
        }
    }

    blocks
}

/// The commands in the newest `available_commands_update`. Each announcement
/// replaces the last, so they are never merged.
pub(crate) fn commands(entries: &[TimelineEntry]) -> Vec<composer::Command> {
    entries
        .iter()
        .rev()
        .find_map(|entry| match &entry.kind {
            EntryKind::Update(v1::SessionUpdate::AvailableCommandsUpdate(update)) => Some(
                update
                    .available_commands
                    .iter()
                    .map(|command| composer::Command {
                        name: command.name.to_string(),
                        description: command.description.to_string(),
                    })
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default()
}

/// The voice and message of a streamed-chunk entry.
fn chunk(entry: &TimelineEntry) -> Option<(Voice, Option<&v1::MessageId>)> {
    let EntryKind::Update(update) = &entry.kind else {
        return None;
    };

    update::spoken(update).map(|(voice, chunk)| (voice, chunk.message_id.as_ref()))
}

/// A row of the centre screen: a block, or the line between turns. Turn
/// boundaries come from core's `TimelineEntry::turn` stamp and [`Turn`] record,
/// never inferred from the rows.
///
/// Unboxed for the same reason as [`Block`]: the list is rebuilt as the agent
/// streams.
#[allow(clippy::large_enum_variant)]
enum Row {
    /// Something the agent said, or the inspector said about it.
    Block(Block),
    /// A turn that is over, and what it came to; drawn under its last row.
    Ended(Turn),
    /// The start of a run of traffic belonging to no turn; drawn above it,
    /// since a run still arriving has no end to label. Holds the key of the
    /// run's first row: a session has as many such runs as it has gaps
    /// between turns, and each is a row of its own.
    Outside(String),
}

/// The blocks with turn boundaries between them. A turn that ended with no
/// updates still gets its line.
fn rows(blocks: Vec<Block>, turns: &[Turn]) -> Vec<Row> {
    let mut rows = Vec::new();
    // The last turn whose ending has been drawn, so each is closed once.
    let mut closed = 0;
    let close_through = |rows: &mut Vec<Row>, upto: u64, closed: &mut u64| {
        for turn in turns {
            if turn.id > *closed && turn.id <= upto && turn.outcome.is_some() {
                rows.push(Row::Ended(turn.clone()));
            }
        }
        *closed = (*closed).max(upto);
    };

    let mut current: Option<Option<u64>> = None;
    // The highest turn any row has been in: traffic outside a turn closes
    // turns up to here and no further.
    let mut reached = 0;

    for block in blocks {
        let turn = block.turn();
        if current != Some(turn) {
            match turn {
                Some(id) => {
                    close_through(&mut rows, id.saturating_sub(1), &mut closed);
                    reached = reached.max(id);
                }
                None => {
                    close_through(&mut rows, reached, &mut closed);
                    rows.push(Row::Outside(block.key()));
                }
            }
            current = Some(turn);
        }
        rows.push(Row::Block(block));
    }

    // And whatever finished after the last row.
    close_through(&mut rows, u64::MAX, &mut closed);
    rows
}

impl Row {
    /// Whether the list's keyboard stops on this row (turn lines are skipped).
    fn is_block(&self) -> bool {
        matches!(self, Self::Block(_))
    }

    /// Whether the frame captured under `ordinal` is one this row was made of.
    fn holds(&self, ordinal: u64) -> bool {
        let Self::Block(block) = self else {
            return false;
        };
        let entries = match block {
            Block::Speech { entries, .. } => entries.as_slice(),
            Block::One(entry) => std::slice::from_ref(entry),
        };
        Crossing::ordinals(entries).contains(&ordinal)
    }

    /// Which row this is, for the list that renders it.
    fn key(&self) -> String {
        match self {
            Self::Block(block) => block.key(),
            Self::Ended(turn) => format!("turn {} ended", turn.id),
            Self::Outside(first) => format!("outside a turn, from {first}"),
        }
    }
}

impl Block {
    /// The turn its first entry arrived in: a message streamed across a turn's
    /// end belongs to the turn it started in.
    fn turn(&self) -> Option<u64> {
        match self {
            Self::Speech { entries, .. } => entries.first().and_then(|entry| entry.turn),
            Self::One(entry) => entry.turn,
        }
    }

    fn render(
        &self,
        on_answer: EventHandler<Answer>,
        on_elicit: EventHandler<elicitation::Answer>,
        crossing: Crossing,
        roving: Roving,
    ) -> Element {
        match self {
            Self::Speech { voice, entries, .. } => said(*voice, entries, crossing, roving),
            Self::One(entry) => happened(entry, on_answer, on_elicit, crossing, roving),
        }
    }

    /// The row's key: its first entry's [`named`] identity, which is stable
    /// however many chunks follow.
    fn key(&self) -> String {
        let identity = match self {
            Self::Speech { entries, .. } => entries.first().map(|entry| &entry.id),
            Self::One(entry) => Some(&entry.id),
        };

        identity.map(named).unwrap_or_default()
    }
}

/// An entry's stable name, used as its row key and disclosure name.
///
/// Built from the entry's own id, not its position (§11 seam 3), so an entry
/// that changes in place keeps whatever the reader had opened on it.
fn named(id: &EntryId) -> String {
    match id {
        EntryId::ToolCall(session, call) => format!("tool call {session} {call}"),
        // Keyed on the request id so the panel survives the re-render that
        // answering it causes.
        EntryId::Permission(request) => format!("permission {request}"),
        // Likewise, so a half-filled form stays with its row.
        EntryId::Elicitation(request) => format!("elicitation {request}"),
        EntryId::Service(request) => format!("service {request}"),
        EntryId::Compaction(session, compaction) => format!("compaction {session} {compaction}"),
        EntryId::Arrival(arrival) => format!("arrival {arrival}"),
        // Non-exhaustive: an unknown id still needs a distinct name.
        other => format!("{other:?}"),
    }
}

/// A message, from however many chunks it arrived in.
fn said(voice: Voice, entries: &[TimelineEntry], crossing: Crossing, roving: Roving) -> Element {
    let (label, tone) = voice.told();
    let ordinals = Crossing::ordinals(entries);
    let at = entries.first().map(|entry| entry.at);
    let content: Vec<_> = entries
        .iter()
        .filter_map(|entry| match &entry.kind {
            EntryKind::Update(update) => update::spoken(update).map(|(_, chunk)| &chunk.content),
            _ => None,
        })
        .collect();

    let linking = crossing.linking(&ordinals);
    let linked = linking.clone();

    let tabindex = roving.tabindex();
    let mut focused = roving.focused;
    let key = roving.key.clone();

    rsx! {
        article {
            class: "{drawn_as(tone, crossing.found(&ordinals), !linking.is_empty())}",
            tabindex,
            "data-entry": "{tone}",
            // Every ordinal this row was made of, so the Console can find it.
            "data-frames": frames_attribute(&ordinals),
            "data-arrived": roving.arrived(),
            onanimationend: roving.settle(),
            onfocus: move |_| focused.set(Some(key.clone())),
            // Pressing the row shows the frames it was made of.
            onclick: move |event| pressed(&event, crossing, linked.clone()),
            {marked(tone)}
            div { class: "row-body",
                {kind_said(label, at)}
                div { class: "said",
                    // One flow, not one paragraph per chunk.
                    for (position, block) in content.into_iter().enumerate() {
                        span { key: "{position}", {update::content(block)} }
                    }
                }
                {reach(&linking, crossing, roving.current)}
            }
        }
    }
}

/// Whether a click on a row is a press of the row.
///
/// A row holds controls this file does not draw, and selectable text, so the
/// click is checked where it landed: on a control, or ending a selection, it
/// is not a press. The point goes to the script as data.
const ROW_PRESS: &str = r#"
const [x, y] = await dioxus.recv();
const selection = window.getSelection();
if (selection && !selection.isCollapsed && selection.toString().trim() !== "") return false;
const hit = document.elementFromPoint(x, y);
return !(hit && hit.closest("button, a, summary, input, textarea, select, label, [role=button], [role=tab], [contenteditable=true]"));
"#;

/// A click on a row: reveal its frames, unless the click was on a control or
/// ended a text selection. Where the check cannot run, the click counts.
fn pressed(event: &Event<MouseData>, crossing: Crossing, linked: Vec<u64>) {
    if linked.is_empty() {
        return;
    }
    let point = event.client_coordinates();
    spawn(async move {
        let checking = document::eval(ROW_PRESS);
        let press = match checking.send((point.x, point.y)) {
            Ok(()) => checking.join::<bool>().await.unwrap_or(true),
            Err(_) => true,
        };
        if press {
            crossing.reveal.call(linked);
        }
    });
}

/// The line under a turn that is over: the agent's stop reason (worded by
/// `composer::stopped`, as the composer does), or the error it failed with.
fn ended(turn: &Turn) -> Element {
    let (what, tone) = match &turn.outcome {
        Some(TurnOutcome::Ended(reason)) => (
            format!("stopReason: {}", crate::composer::stopped(reason)),
            match reason {
                v1::StopReason::EndTurn => "",
                v1::StopReason::Refusal | v1::StopReason::Cancelled => "warn",
                _ => "warn",
            },
        ),
        Some(TurnOutcome::Failed(error)) => (format!("failed: {error}"), "bad"),
        // A running turn's state is the composer's to say.
        None => return rsx! {},
        // Non-exhaustive: an unknown ending is still an ending.
        _ => (
            "ended in a way this window has no name for".to_owned(),
            "warn",
        ),
    };

    rsx! {
        div { class: "ended", "data-slot": "turn-ended",
            span { class: "stop-reason {tone}", "{what}" }
            span { class: "ended-meta", "turn {turn.id}" }
        }
    }
}

/// The line above a run of traffic that belongs to no turn. Said in words, not
/// as a severity: a replayed session is entirely out of turn and correct.
fn outside() -> Element {
    rsx! {
        div { class: "boundary outside", "data-slot": "outside-turn",
            span { class: "heading", "Outside a turn" }
            span { "sent while no prompt was running" }
        }
    }
}

/// The ordinals a row was made of, as the attribute both screens read.
/// Space-separated, so `[data-frames~="3"]` cannot match frame 31.
fn frames_attribute(ordinals: &[u64]) -> String {
    ordinals
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

/// One entry that is not a chunk: a tool call, a plan, a usage report, a
/// request the turn stopped for — or something nobody here has a name for.
fn happened(
    entry: &TimelineEntry,
    on_answer: EventHandler<Answer>,
    on_elicit: EventHandler<elicitation::Answer>,
    crossing: Crossing,
    roving: Roving,
) -> Element {
    // An entry with no rendering shows its JSON without being asked (§8).
    let rendered = match &entry.kind {
        EntryKind::Update(update) => update::render(update),
        // Blocking requests render in the flow (§7.2, §7.8).
        EntryKind::Permission(request) => {
            Some(("permission", "ask", permission::panel(request, on_answer)))
        }
        EntryKind::Elicitation(request) => Some((
            "elicitation",
            "ask",
            rsx! {
                elicitation::Panel {
                    state: request.state(), completed: request.completed(),
                    request: request.clone(), on_answer: on_elicit,
                }
            },
        )),
        // A client-service request, answered from its card.
        EntryKind::Service(request) => {
            Some(("service", "ask", crate::client_services::card(request)))
        }
        // In the flow beside the traffic it is about (§15 q7).
        EntryKind::Annotation(annotation) => Some(("conformance", "spec", claim(annotation))),
        // Non-exhaustive: an unknown kind is drawn as unrecognized.
        _ => None,
    };
    let named = rendered.is_some();
    let (label, tone, body) =
        rendered.unwrap_or_else(|| ("unrecognized", "unnamed", unnamed(&entry.kind)));

    let entries = std::slice::from_ref(entry);
    let ordinals = Crossing::ordinals(entries);
    // So an elicitation raised inside a tool call can link to it (§7.8).
    let tool_call = match &entry.id {
        EntryId::ToolCall(_, call) => Some(call.to_string()),
        _ => None,
    };

    let linking = crossing.linking(&ordinals);
    let linked = linking.clone();
    // Frames are drawn under the row only where they are the rendering;
    // elsewhere the row leads to the wire log.
    let shows_frames = evidence(entry, named);
    let tabindex = roving.tabindex();
    let mut focused = roving.focused;
    let key = roving.key.clone();

    rsx! {
        article {
            class: "{drawn_as(tone, crossing.found(&ordinals), !linking.is_empty())}",
            tabindex,
            "data-entry": "{tone}",
            "data-tool-call": tool_call,
            "data-frames": frames_attribute(&ordinals),
            "data-arrived": roving.arrived(),
            onanimationend: roving.settle(),
            onfocus: move |_| focused.set(Some(key.clone())),
            onclick: move |event| pressed(&event, crossing, linked.clone()),
            {marked(tone)}
            div { class: "row-body",
                {kind_said(label, Some(entry.at))}
                div { class: "said", {body} }
                if shows_frames {
                    {frames_of(entries)}
                }
                {reach(&linking, crossing, roving.current)}
            }
        }
    }
}

/// What a row is drawn as: its tone, whether the Console sent the reader to it,
/// and whether pressing it leads anywhere.
fn drawn_as(tone: &str, sought: bool, linked: bool) -> String {
    let sought = if sought { " sought" } else { "" };
    let linked = if linked { " linked" } else { "" };

    format!("row{sought}{linked} {tone}")
}

/// The decorative mark in an entry's gutter, keyed on its tone. Hidden from
/// assistive technology: the row's kind is always also said in text.
fn marked(tone: &str) -> Element {
    use crate::disclosure::{Role, icon};

    let role = match tone {
        "user" => Role::User,
        "agent" => Role::Agent,
        "thought" => Role::Thought,
        "tool" => Role::Tool,
        "plan" => Role::Plan,
        "ask" => Role::Ask,
        "spec" => Role::Spec,
        "unnamed" => Role::Unnamed,
        // `note` and anything unrecognised.
        _ => Role::Note,
    };

    rsx! {
        span { class: "row-mark", aria_hidden: "true", {icon(role)} }
    }
}

/// A conformance annotation, in the two sentences core wrote for it (§15 q7).
fn claim(annotation: &Annotation) -> Element {
    rsx! {
        p { class: "requires", "{annotation.requires()}" }
        p { class: "observed", "{annotation.observed()}" }
    }
}

/// Whether an entry's frames are part of its rendering: true for traffic with
/// no rendering (§8) and for a conformance annotation, whose claim is about
/// the frames.
fn evidence(entry: &TimelineEntry, named: bool) -> bool {
    !named || matches!(entry.kind, EntryKind::Annotation(_))
}

/// An entry with no rendering, said in words; its JSON is open underneath
/// (§8).
fn unnamed(kind: &EntryKind) -> Element {
    rsx! {
        p { class: "unnamed",
            {match kind {
                EntryKind::Unrecognized(Unrecognized::NotServiced { method }) => rsx! {
                    "The agent called "
                    code { "{method}" }
                    ". This client advertised no capability for it, so it answered method-not-found."
                },
                EntryKind::Unrecognized(Unrecognized::Ignored { method }) => rsx! {
                    "The agent sent "
                    code { "{method}" }
                    ", a notification this client does not know. Nothing answers a notification, so it was ignored."
                },
                EntryKind::Unrecognized(Unrecognized::Undecodable { method, problem }) => rsx! {
                    if let Some(method) = method {
                        code { "{method}" }
                        " carried something v1 could not read: "
                    } else {
                        "A frame that was not JSON-RPC at all: "
                    }
                    "{problem}"
                },
                EntryKind::Unrecognized(Unrecognized::Unsolicited) => rsx! {
                    "An answer to a request this client never sent."
                },
                EntryKind::Unrecognized(Unrecognized::ForeignSession { session }) => rsx! {
                    "An update for session "
                    code { "{session}" }
                    ", not the open one. It changed nothing here."
                },
                EntryKind::Unrecognized(Unrecognized::Dropped { length, limit }) => rsx! {
                    "A frame of {sized(*length)} was dropped (limit {sized(*limit)}). Its start is below."
                },
                EntryKind::Unrecognized(Unrecognized::MistypedId { id, call }) => rsx! {
                    "An answer for call {call} with id "
                    code { "{id}" }
                    " ({id_kind(id)}). The call is still waiting."
                },
                EntryKind::Unrecognized(Unrecognized::Unaddressed { error }) => rsx! {
                    "An error with no id: "
                    code { "{i32::from(error.code)}" }
                    " {error.message}"
                },
                EntryKind::Unrecognized(Unrecognized::OverCapacity { method, limit }) => rsx! {
                    "Over {limit} requests waiting, so this "
                    code { "{method}" }
                    " was refused."
                },
                // An unstable-v1 variant, an extension, or v2 arriving early.
                EntryKind::Update(_) => rsx! {
                    "A v1 update this window has no rendering for."
                },
                // Both enums are non-exhaustive.
                _ => rsx! { "Something this window has no rendering for." },
            }}
            " What arrived is below, as it arrived."
        }
    }
}

/// A byte count the way a reader says it: bytes, KB or MB, binary units, to
/// one decimal where that is not zero.
fn sized(bytes: usize) -> String {
    const KB: f64 = 1024.0;
    let amount = bytes as f64;
    let (value, unit) = if amount >= KB * KB {
        (amount / (KB * KB), "MB")
    } else if amount >= KB {
        (amount / KB, "KB")
    } else {
        return format!("{bytes} bytes");
    };
    let said = format!("{value:.1}");
    format!("{} {unit}", said.strip_suffix(".0").unwrap_or(&said))
}

/// What is wrong with an id that names a call but is not the call's id.
fn id_kind(id: &str) -> &'static str {
    if id.starts_with('"') {
        "a string"
    } else {
        "not a whole number"
    }
}

/// The kind of a row, in words: a visible label, or a screen-reader-only one
/// for rows whose body already names them. The time is screen-reader-only;
/// the wire log carries the visible stamps.
fn kind_said(label: &str, at: Option<SystemTime>) -> Element {
    /// The rows whose body says what they are.
    const NAMED_BY_ITS_BODY: [&str; 7] = [
        "agent_message_chunk",
        "tool call",
        "plan",
        "permission",
        "elicitation",
        "service",
        "compaction",
    ];

    rsx! {
        if NAMED_BY_ITS_BODY.contains(&label) {
            span { class: "sr-only", "{label}" }
        } else {
            div { class: "row-head",
                span { class: "wire-name", "{label}" }
                if let Some(at) = at {
                    span { class: "sr-only", " at {stamp(at)}" }
                }
            }
        }
    }
}

/// The frames an entry was decoded from, drawn where they are the rendering
/// (see [`evidence`]).
///
/// Laid out through `crate::indent`, as the wire log does; the copy always
/// takes the frame's own text.
fn frames_of(entries: &[TimelineEntry]) -> Element {
    let frames: Vec<Recorded> = entries
        .iter()
        .flat_map(|entry| entry.frames.iter().cloned())
        .collect();
    // Named by the entry's identity, not its position (§11 seam 3).
    let key = entries
        .first()
        .map(|entry| format!("evidence {}", named(&entry.id)))
        .unwrap_or_default();

    rsx! {
        div { class: "raws", "data-slot": "wire",
            for (position, recorded) in frames.into_iter().enumerate() {
                div { key: "{position}", class: "raw-row",
                    if recorded.frame.as_str().len() > 16 * 1024 {
                        crate::page::RawFrame { frame: recorded.frame.clone() }
                    } else {
                    pre {
                        class: "raw",
                        "{crate::indent::drawn(&key, true, recorded.frame.as_str())}"
                    }
                    }
                    Copy { frame: recorded.frame.clone(), what: "this frame" }
                }
            }
        }
    }
}

/// The keyboard's way from a row to its frames: the row press, named. Off
/// screen until focused (the skip-link pattern), and absent once the trace no
/// longer holds the frames. A Tab stop only on the list's current row.
fn reach(ordinals: &[u64], crossing: Crossing, current: bool) -> Element {
    let reach = ordinals.to_vec();
    let count = reach.len();

    rsx! {
        if count > 0 {
            button {
                class: "btn btn-xs btn-quiet reach",
                "data-slot": "reach",
                r#type: "button",
                tabindex: if current { "0" } else { "-1" },
                // `navigation_tests.rs` finds the control by this title.
                title: if count == 1 { "Find this frame in the wire log" } else { "Find these frames in the wire log" },
                onclick: move |event| {
                    // The row's own press would reveal them again.
                    event.stop_propagation();
                    crossing.reveal.call(reach.clone());
                },
                if count == 1 { "Show frame" } else { "Show {count} frames" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use acp_inspector_core::{Ending, EntryId, OptionSet, Replay};

    use super::*;

    /// A conformance annotation's row, by its published kind.
    const ANNOTATION: &str = r#"data-entry="spec""#;

    /// How many rows draw their frames under them.
    fn wires(html: &str) -> usize {
        html.matches(r#"data-slot="wire""#).count()
    }

    fn entry(kind: EntryKind) -> TimelineEntry {
        in_turn(kind, Some(1))
    }

    /// An entry in `turn`, or in no turn for `None`.
    fn in_turn(kind: EntryKind, turn: Option<u64>) -> TimelineEntry {
        TimelineEntry {
            id: EntryId::Arrival(1),
            at: SystemTime::UNIX_EPOCH,
            frames: vec![Recorded {
                frame: acp_inspector_core::Frame::new("{}"),
                captured: Some(0),
            }],
            kind,
            turn,
        }
    }

    fn chunked(text: &str, message: Option<&str>) -> v1::ContentChunk {
        let chunk = v1::ContentChunk::new(v1::ContentBlock::from(text));
        match message {
            Some(message) => chunk.message_id(v1::MessageId::new(message)),
            None => chunk,
        }
    }

    fn spoke(text: &str, message: Option<&str>) -> TimelineEntry {
        entry(EntryKind::Update(v1::SessionUpdate::AgentMessageChunk(
            chunked(text, message),
        )))
    }

    fn thought(text: &str) -> TimelineEntry {
        entry(EntryKind::Update(v1::SessionUpdate::AgentThoughtChunk(
            chunked(text, None),
        )))
    }

    fn tool_call() -> TimelineEntry {
        entry(EntryKind::Update(v1::SessionUpdate::ToolCall(
            v1::ToolCall::new("call-1", "Reading a file"),
        )))
    }

    /// How many chunks each block gathered; zero for a non-chunk block.
    fn gathered(blocks: &[Block]) -> Vec<usize> {
        blocks
            .iter()
            .map(|block| match block {
                Block::Speech { entries, .. } => entries.len(),
                Block::One(_) => 0,
            })
            .collect()
    }

    #[test]
    fn a_message_streamed_in_chunks_is_one_block() {
        let blocks = blocks(vec![
            spoke("Hel", Some("m1")),
            spoke("lo, ", Some("m1")),
            spoke("world", Some("m1")),
        ]);

        assert_eq!(gathered(&blocks), [3]);
    }

    #[test]
    fn chunks_without_a_message_id_group_by_adjacency() {
        let blocks = blocks(vec![spoke("one ", None), spoke("sentence", None)]);

        assert_eq!(gathered(&blocks), [2]);
    }

    #[test]
    fn a_new_message_id_starts_a_new_block() {
        let blocks = blocks(vec![
            spoke("first", Some("m1")),
            spoke("second", Some("m2")),
        ]);

        assert_eq!(gathered(&blocks), [1, 1]);
    }

    #[test]
    fn a_change_of_voice_starts_a_new_block() {
        let blocks = blocks(vec![thought("hmm"), spoke("hello", None)]);

        assert_eq!(gathered(&blocks), [1, 1]);
    }

    #[test]
    fn anything_between_two_chunks_ends_the_run() {
        // Screen order is wire order.
        let blocks = blocks(vec![
            spoke("before", None),
            tool_call(),
            spoke("after", None),
        ]);

        assert_eq!(gathered(&blocks), [1, 0, 1]);
    }

    /// The rendered screen: a message, and an annotation after it.
    fn shown(annotation: Annotation) -> String {
        #[component]
        fn Host(annotation: Annotation) -> Element {
            let entries = use_signal(move || {
                vec![
                    spoke("working on it", None),
                    entry(EntryKind::Annotation(annotation.clone())),
                ]
            });
            rsx! {
                Timeline {
                    entries,
                    turn: TurnState::Ended(v1::StopReason::EndTurn),
                    session: None,
                    connected: false,
                    held_frames: 0..u64::MAX,
                    blocked: false,
                    problem: None,
                    mode: None,
                    on_prompt: move |_: Vec<v1::ContentBlock>| Ok(()),
                    on_stop: move |()| {},
                    on_set_mode: move |_| {},
                    on_answer: move |_: Answer| {},
                    on_elicit: move |_: elicitation::Answer| {},
                    sought: None,
                    on_reveal: move |_: Vec<u64>| {},
                    described: false,
                    turns: Vec::new(),
                }
            }
        }

        let mut dom = VirtualDom::new_with_props(Host, HostProps { annotation });
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    /// The one a cancelled turn that ended some other way draws.
    fn cancelled() -> Annotation {
        Annotation::Cancellation(Ending::Stopped(v1::StopReason::EndTurn))
    }

    #[test]
    fn a_conformance_annotation_is_read_in_the_flow_beside_the_traffic() {
        // A row among the rows, never a modal or toast (§15 q7).
        let html = shown(cancelled());

        assert!(
            html.contains("must still resolve"),
            "it says what the specification requires: {html}"
        );
        assert!(
            html.contains("end_turn"),
            "and what was observed instead: {html}"
        );

        let rows = html.matches(r#"<article class="row"#).count();
        assert_eq!(
            rows, 2,
            "the message and the annotation, side by side: {html}"
        );
        assert!(
            !html.contains("<dialog") && !html.to_lowercase().contains("toast"),
            "and neither of them is a modal or a toast: {html}"
        );
    }

    #[test]
    fn an_annotations_evidence_is_under_it_rather_than_a_screen_away() {
        // The frames are the claim's evidence; the message above keeps its
        // frames in the wire log.
        let html = shown(cancelled());

        let annotation = html
            .rsplit_once(ANNOTATION)
            .expect("the annotation is drawn")
            .1;
        assert_eq!(
            wires(annotation),
            1,
            "an annotation's frames are drawn beneath it: {annotation}"
        );
        assert_eq!(wires(&html), 1, "and only its own are: {html}");
    }

    #[test]
    fn evidence_that_is_the_rendering_is_drawn_plainly_and_stays_copyable() {
        // No disclosure: these frames are the rendering itself.
        let html = shown(cancelled());
        let frames = html
            .split_once(r#"data-slot="wire""#)
            .expect("the annotation's frames")
            .1;

        assert!(
            !frames.contains("<summary") && !frames.contains("<details"),
            "the frames are read rather than opened: {frames}"
        );
        assert!(
            frames.contains(r#"<pre class="raw">"#)
                && frames.contains(r#"aria-label="Copy this frame""#),
            "and what crossed stays exactly copyable: {frames}"
        );
    }

    #[test]
    fn a_second_rule_is_surfaced_by_the_same_row_as_the_first() {
        // Every rule takes the same row, with its words from core (§15 q7).
        let html = shown(Annotation::Replay(Replay::Nothing));

        assert!(
            html.contains("must replay the entire conversation"),
            "it says what the specification requires: {html}"
        );
        assert!(
            html.contains("replayed nothing"),
            "and what was observed instead: {html}"
        );
        assert_eq!(
            html.matches(r#"<article class="row"#).count(),
            2,
            "a row among the rows: {html}"
        );
        assert_eq!(wires(&html), 1, "with its evidence under it: {html}");
    }

    #[test]
    fn a_third_rule_takes_the_same_row_as_the_two_before_it() {
        let html = shown(Annotation::OptionSet(OptionSet::Missing(
            v1::SessionConfigId::new("verbosity"),
        )));

        assert!(
            html.contains("complete set of configuration options"),
            "it says what the specification requires: {html}"
        );
        assert!(
            html.contains("verbosity"),
            "and what was observed instead, naming the agent's own option: {html}"
        );
        assert_eq!(
            html.matches(r#"<article class="row"#).count(),
            2,
            "a row among the rows: {html}"
        );
        assert_eq!(wires(&html), 1, "with its evidence under it: {html}");
    }

    /// A turn that is over, and what it came to.
    fn turn_record(id: u64, outcome: Option<TurnOutcome>) -> Turn {
        Turn {
            id,
            at: SystemTime::UNIX_EPOCH,
            outcome,
        }
    }

    /// The row stream, as the kinds of row in order.
    fn shape(blocks: Vec<Block>, turns: &[Turn]) -> Vec<&'static str> {
        rows(blocks, turns)
            .iter()
            .map(|row| match row {
                Row::Block(_) => "block",
                Row::Ended(_) => "ended",
                Row::Outside(_) => "outside",
            })
            .collect()
    }

    fn spoke_in(turn: Option<u64>) -> Block {
        Block::One(in_turn(
            EntryKind::Update(v1::SessionUpdate::AgentMessageChunk(chunked("said", None))),
            turn,
        ))
    }

    #[test]
    fn a_turn_that_is_over_is_closed_under_its_last_row() {
        let ended = [turn_record(
            1,
            Some(TurnOutcome::Ended(v1::StopReason::EndTurn)),
        )];
        assert_eq!(
            shape(vec![spoke_in(Some(1)), spoke_in(Some(1))], &ended),
            ["block", "block", "ended"],
            "the line goes under the turn, because how one ended is known at its end"
        );

        // A turn still running has no line; its state is the composer's.
        assert_eq!(
            shape(vec![spoke_in(Some(1))], &[turn_record(1, None)]),
            ["block"],
        );
    }

    #[test]
    fn traffic_belonging_to_no_turn_is_marked_where_the_run_begins() {
        let ended = [turn_record(
            1,
            Some(TurnOutcome::Ended(v1::StopReason::EndTurn)),
        )];

        // The agent went on talking after the turn was over: the turn closes
        // first, then the run is marked.
        assert_eq!(
            shape(vec![spoke_in(Some(1)), spoke_in(None)], &ended),
            ["block", "ended", "outside", "block"],
        );

        // One marker per run, not per row.
        assert_eq!(
            shape(vec![spoke_in(None), spoke_in(None)], &[]),
            ["outside", "block", "block"],
        );
    }

    #[test]
    fn every_run_outside_a_turn_is_a_row_of_its_own() {
        // Claude's agent publishes its commands before the first prompt and
        // again after it: two runs, two markers, which the list keys apart.
        let arrived = |arrival, turn| {
            let mut block = spoke_in(turn);
            if let Block::One(entry) = &mut block {
                entry.id = EntryId::Arrival(arrival);
            }
            block
        };
        let ended = [turn_record(
            1,
            Some(TurnOutcome::Ended(v1::StopReason::EndTurn)),
        )];
        let rows = rows(
            vec![arrived(1, None), arrived(2, Some(1)), arrived(3, None)],
            &ended,
        );
        let keys: Vec<String> = rows.iter().map(Row::key).collect();
        let unique: std::collections::HashSet<&String> = keys.iter().collect();
        assert_eq!(
            unique.len(),
            keys.len(),
            "every row has its own key: {keys:?}"
        );
    }

    #[test]
    fn a_turn_that_said_nothing_still_says_it_happened() {
        let turns = [
            turn_record(1, Some(TurnOutcome::Ended(v1::StopReason::Refusal))),
            turn_record(2, Some(TurnOutcome::Ended(v1::StopReason::EndTurn))),
        ];
        assert_eq!(
            shape(vec![spoke_in(Some(2))], &turns),
            ["ended", "block", "ended"],
            "the empty turn is closed above the one that followed it"
        );
    }

    #[test]
    fn a_boundary_says_the_agents_own_word_for_how_the_turn_stopped() {
        #[component]
        fn Host(turn: Turn) -> Element {
            ended(&turn)
        }

        let shown = |outcome| {
            let mut dom = VirtualDom::new_with_props(
                Host,
                HostProps {
                    turn: turn_record(3, Some(outcome)),
                },
            );
            dom.rebuild_in_place();
            dioxus_ssr::render(&dom)
        };

        let stopped = shown(TurnOutcome::Ended(v1::StopReason::MaxTokens));
        assert!(
            stopped.contains("turn 3")
                && stopped.contains("stopReason: max_tokens")
                && !stopped.contains("Max tokens"),
            "the field the answer carried, verbatim: {stopped}"
        );

        // A failed turn reports the error, never a stop reason the agent did
        // not give.
        let failed = shown(TurnOutcome::Failed(CallError::Disconnected));
        assert!(
            failed.contains("failed") && !failed.contains("end_turn"),
            "{failed}"
        );
    }

    /// The centre screen with no session open.
    fn nothing_open(connected: bool) -> String {
        nothing_open_on(connected, false)
    }

    /// The same, saying whether a session can be opened.
    fn nothing_open_on(connected: bool, openable: bool) -> String {
        #[component]
        fn Host(connected: bool, openable: bool) -> Element {
            let entries = use_signal(Vec::<TimelineEntry>::new);
            rsx! {
                Timeline {
                    entries,
                    turn: TurnState::Idle,
                    session: None,
                    connected,
                    held_frames: 0..u64::MAX,
                    blocked: false,
                    problem: None,
                    mode: None,
                    on_prompt: move |_: Vec<v1::ContentBlock>| Ok(()),
                    on_stop: move |()| {},
                    on_set_mode: move |_| {},
                    on_answer: move |_: Answer| {},
                    on_elicit: move |_: elicitation::Answer| {},
                    sought: None,
                    on_reveal: move |_: Vec<u64>| {},
                    described: false,
                    turns: Vec::new(),
                    on_new_session: openable.then(|| EventHandler::new(move |()| {})),
                }
            }
        }

        let mut dom = VirtualDom::new_with_props(
            Host,
            HostProps {
                connected,
                openable,
            },
        );
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    /// The control an empty screen offers, if it offers one.
    fn empty_action(html: &str) -> Option<String> {
        html.split("<button")
            .skip(1)
            .filter_map(|rest| rest.split_once("</button>"))
            .map(|(button, _)| button.to_owned())
            .find(|button| button.contains(r#"data-slot="empty-action""#))
    }

    #[test]
    fn an_empty_screen_offers_the_one_thing_that_resolves_it() {
        let openable = nothing_open_on(true, true);
        let offer = empty_action(&openable).expect("a session can be opened from here");
        assert!(
            offer.contains("session/new") && offer.contains("btn-filled"),
            "the offer is the call the Agent screen would make: {offer}"
        );

        // Behind the same gate as the Agent screen's button.
        assert!(
            empty_action(&nothing_open(true)).is_none(),
            "no agent has described itself, so nothing is offered"
        );

        // With no agent, the offer is the launch dialog.
        let unlaunched = nothing_open(false);
        let launch = empty_action(&unlaunched).expect("the launch form is offered");
        assert!(
            launch.contains("Connect\u{2026}"),
            "an unlaunched window is sent to the fields: {launch}"
        );
        // In the bar's word, since both open the same dialog.
        assert!(
            !launch.contains("Launch"),
            "and `Launch` is left to the control inside the form that does it: {launch}"
        );

        // Waiting on the agent offers nothing.
        assert!(
            empty_action(&waiting_on_the_agent()).is_none(),
            "a session with nothing said in it yet has nothing to offer"
        );
    }

    /// A live session that the agent has said nothing in.
    fn waiting_on_the_agent() -> String {
        #[component]
        fn Host() -> Element {
            let entries = use_signal(Vec::<TimelineEntry>::new);
            rsx! {
                Timeline {
                    entries,
                    turn: TurnState::Idle,
                    session: Some(v1::SessionId::new("s1")),
                    connected: true,
                    held_frames: 0..u64::MAX,
                    blocked: false,
                    problem: None,
                    mode: None,
                    on_prompt: move |_: Vec<v1::ContentBlock>| Ok(()),
                    on_stop: move |()| {},
                    on_set_mode: move |_| {},
                    on_answer: move |_: Answer| {},
                    on_elicit: move |_: elicitation::Answer| {},
                    sought: None,
                    on_reveal: move |_: Vec<u64>| {},
                    described: false,
                    turns: Vec::new(),
                    on_new_session: Some(EventHandler::new(move |()| {})),
                }
            }
        }

        let mut dom = VirtualDom::new(Host);
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    #[test]
    fn having_no_live_session_reads_as_itself_rather_than_as_an_empty_one() {
        // After the live session is closed or deleted (§7.5): an agent, and no
        // session. Each of the three empty states reads as itself.
        let ended = nothing_open(true);

        assert!(
            ended.contains("<h3>No session open</h3>")
                && ended.contains("Open a new session to start prompting."),
            "it says there is no session and what to do about it: {ended}"
        );
        assert!(
            !ended.contains("Start your agent"),
            "and not to start an agent, which is right there: {ended}"
        );

        // Before a launch: the first-run card, with the palette's shortcut.
        let unlaunched = nothing_open(false);
        assert!(
            unlaunched.contains(r#"data-slot="onboarding""#)
                && unlaunched.contains("Start your agent to see the conversation here.")
                && unlaunched.contains(r#"data-slot="palette-hint""#)
                && unlaunched.contains("<kbd>"),
            "an agent that was never started is a different absence: {unlaunched}"
        );
        assert!(
            unlaunched.contains(r#"data-icon="Start""#),
            "the card leads with an icon, not a glyph: {unlaunched}"
        );

        // And a session that is open and quiet says so.
        let waiting = waiting_on_the_agent();
        assert!(
            waiting.contains("<h3>Session open</h3>")
                && waiting.contains("Send a prompt below. Agent updates appear here."),
            "{waiting}"
        );
    }

    #[test]
    fn a_login_the_agent_asked_for_is_offered_where_the_session_would_be() {
        // A session/new refused for want of a login: offer the login, not a
        // button that would be refused again.
        #[component]
        fn Host() -> Element {
            let entries = use_signal(Vec::<TimelineEntry>::new);
            rsx! {
                Timeline {
                    entries,
                    turn: TurnState::Idle,
                    session: None,
                    connected: true,
                    held_frames: 0..u64::MAX,
                    blocked: false,
                    problem: None,
                    mode: None,
                    on_prompt: move |_: Vec<v1::ContentBlock>| Ok(()),
                    on_stop: move |()| {},
                    on_set_mode: move |_| {},
                    on_answer: move |_: Answer| {},
                    on_elicit: move |_: elicitation::Answer| {},
                    sought: None,
                    on_reveal: move |_: Vec<u64>| {},
                    described: true,
                    turns: Vec::new(),
                    on_new_session: Some(EventHandler::new(move |()| {})),
                    login: Some(rsx! { button { "data-slot": "authenticate", "Authenticate" } }),
                }
            }
        }

        let mut dom = VirtualDom::new(Host);
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains(r#"data-slot="empty-login""#)
                && html.contains(r#"data-slot="authenticate""#)
                && !html.contains("No session open"),
            "{html}"
        );
    }

    #[test]
    fn the_header_says_a_finished_turn_is_done() {
        #[component]
        fn Host(outcome: Option<TurnOutcome>) -> Element {
            let entries = use_signal(|| vec![spoke("hello", None)]);
            rsx! {
                Timeline {
                    entries,
                    turn: TurnState::Idle,
                    session: Some(v1::SessionId::new("s1")),
                    connected: true,
                    held_frames: 0..u64::MAX,
                    blocked: false,
                    problem: None,
                    mode: None,
                    on_prompt: move |_: Vec<v1::ContentBlock>| Ok(()),
                    on_stop: move |()| {},
                    on_set_mode: move |_| {},
                    on_answer: move |_: Answer| {},
                    on_elicit: move |_: elicitation::Answer| {},
                    sought: None,
                    on_reveal: move |_: Vec<u64>| {},
                    described: true,
                    turns: vec![turn_record(2, outcome.clone())],
                }
            }
        }
        let scope = |outcome| {
            let mut dom = VirtualDom::new_with_props(Host, HostProps { outcome });
            dom.rebuild_in_place();
            let html = dioxus_ssr::render(&dom);
            html.split_once(r#"data-slot="turn-scope""#)
                .and_then(|(_, rest)| rest.split_once('>'))
                .and_then(|(_, rest)| rest.split_once('<'))
                .map(|(scope, _)| scope.to_owned())
                .expect("the header names the turn")
        };
        assert_eq!(scope(None), "turn 2");
        assert_eq!(
            scope(Some(TurnOutcome::Ended(v1::StopReason::EndTurn))),
            "turn 2 · done"
        );
        assert_eq!(
            scope(Some(TurnOutcome::Failed(CallError::Disconnected))),
            "turn 2 · failed"
        );
    }

    #[test]
    fn a_long_timeline_is_drawn_a_page_at_a_time() {
        let rows: Vec<Vec<Option<u64>>> = (0..450).map(|n| vec![Some(n)]).collect();
        let html = screen(rows.clone(), false, None);
        assert_eq!(
            html.matches("<article").count(),
            crate::page::SIZE,
            "only the newest page is drawn"
        );
        assert!(html.contains(r#"data-frames="449""#) && !html.contains(r#"data-frames="0""#));
        assert!(html.contains("251–450 of 450 retained rows"), "{html}");
        // Exactly one row is the list's Tab stop.
        assert_eq!(
            html.matches(r#"<article class="row linked agent" tabindex="0""#)
                .count(),
            1,
            "one Tab stop for the list"
        );
    }

    #[test]
    fn the_timeline_is_a_flat_center_surface_rather_than_a_card() {
        let html = nothing_open(false);
        let surface = html
            .split_once("<main")
            .and_then(|(_, main)| main.split_once('>').map(|(tag, _)| tag))
            .expect("the Timeline main landmark");

        assert!(
            surface.contains(r#"class="turns""#),
            "the turn is a column of the body rather than a card on it: {surface}"
        );
        assert!(
            !surface
                .split_ascii_whitespace()
                .any(|part| part.contains("card")),
            "the center split surface must not gain inset card chrome: {surface}"
        );
    }

    #[test]
    fn the_header_names_the_screen_the_session_and_where_the_turn_stands() {
        // ADR 0009: the header names the region and its state; the sentence
        // about the state stays above the composer.
        let html = screen(vec![vec![Some(0)]], true, None);
        let header = html
            .split_once("<header class=\"pane-head\">")
            .and_then(|(_, rest)| rest.split_once("</header>"))
            .map(|(header, _)| header.to_owned())
            .expect("the Timeline's own header");

        assert!(
            header.contains(r#"<h2 class="pane-title">Timeline</h2>"#),
            "the screen names itself: {header}"
        );
        assert!(
            !header.contains(r#"data-slot="open-sessions""#) && !header.contains("sess-1"),
            "the session is named and acted on in the rail, not over the screen it is read on: {header}"
        );
        assert!(
            header.contains(r#"data-slot="turn-state""#) && header.contains("Waiting on you"),
            "and where the turn stands is at the far end of it: {header}"
        );
        assert!(
            !header.contains("stopped on a request"),
            "the sentence explaining it belongs above the composer: {header}"
        );
    }

    #[test]
    fn an_entry_leads_with_a_mark_that_repeats_what_the_row_already_says() {
        // The gutter glyph is decoration; the kind is always in text.
        let html = screen(vec![vec![Some(0)]], false, None);
        assert!(
            html.contains(r#"<span class="row-mark" aria-hidden="true">"#),
            "the mark is hidden from the accessibility tree: {html}"
        );
        assert!(
            html.contains(r#"<span class="sr-only">agent_message_chunk</span>"#),
            "and what it stands for is in words, where the drawing leaves the row bare: {html}"
        );
        assert!(
            !html.contains(r#"<span class="wire-name">agent_message_chunk</span>"#),
            "the agent's message is drawn as the message and nothing else: {html}"
        );
        // The rows the drawing does label keep the word on screen.
        let annotated = shown(cancelled());
        assert!(
            annotated.contains(r#"<span class="wire-name">conformance</span>"#),
            "a row the drawing names says what it is on screen: {annotated}"
        );
    }

    #[test]
    fn an_unrecognized_entry_is_a_block_of_its_own() {
        // Never folded into a neighbour (§8).
        let blocks = blocks(vec![
            spoke("hello", None),
            entry(EntryKind::Unrecognized(Unrecognized::Unsolicited)),
            spoke("again", None),
        ]);

        assert_eq!(gathered(&blocks), [1, 0, 1]);
    }

    /// The rendered centre screen. Each inner list is one row's frame ordinals
    /// (`None` for a frame the trace never numbered); props are ordinals because
    /// `TimelineEntry` is not comparable.
    fn screen(rows: Vec<Vec<Option<u64>>>, blocked: bool, sought: Option<u64>) -> String {
        screen_with_held(rows, blocked, sought, 0..u64::MAX)
    }

    fn screen_with_held(
        rows: Vec<Vec<Option<u64>>>,
        blocked: bool,
        sought: Option<u64>,
        held_frames: std::ops::Range<u64>,
    ) -> String {
        #[component]
        fn Host(
            rows: Vec<Vec<Option<u64>>>,
            blocked: bool,
            sought: Option<u64>,
            held_frames: std::ops::Range<u64>,
        ) -> Element {
            let entries = use_signal(move || {
                rows.iter()
                    .enumerate()
                    .map(|(position, ordinals)| TimelineEntry {
                        id: EntryId::Arrival(position as u64),
                        frames: ordinals
                            .iter()
                            .map(|captured| Recorded {
                                frame: acp_inspector_core::Frame::new(r#"{"jsonrpc":"2.0"}"#),
                                captured: *captured,
                            })
                            .collect(),
                        ..spoke("streaming", Some(&format!("m{position}")))
                    })
                    .collect::<Vec<_>>()
            });
            rsx! {
                Timeline {
                    entries,
                    turn: if blocked { TurnState::InFlight } else { TurnState::Idle },
                    session: Some(v1::SessionId::new("sess-1")),
                    connected: true,
                    held_frames,
                    blocked,
                    problem: None,
                    mode: None,
                    on_prompt: move |_: Vec<v1::ContentBlock>| Ok(()),
                    on_stop: move |()| {},
                    on_set_mode: move |_| {},
                    on_answer: move |_: Answer| {},
                    on_elicit: move |_: elicitation::Answer| {},
                    sought,
                    on_reveal: move |_: Vec<u64>| {},
                    described: false,
                    turns: Vec::new(),
                }
            }
        }

        let mut dom = VirtualDom::new_with_props(
            Host,
            HostProps {
                rows,
                blocked,
                sought,
                held_frames,
            },
        );
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    /// One entry whose evidence is open, indented; `closed` once clicked shut.
    fn indented_evidence(frame: &str, closed: bool) -> String {
        #[component]
        fn Host(frame: String, closed: bool) -> Element {
            crate::indent::provide(acp_inspector_core::Indentation::Indented);
            if closed {
                // What activating the summary does.
                crate::indent::toggled(&format!("evidence {}", named(&EntryId::Arrival(1))));
            }
            let entries = use_signal(move || {
                vec![TimelineEntry {
                    frames: vec![Recorded {
                        frame: acp_inspector_core::Frame::new(frame.clone()),
                        captured: Some(0),
                    }],
                    // Unrecognized, so its evidence is open without being asked.
                    ..entry(EntryKind::Unrecognized(Unrecognized::NotServiced {
                        method: "fs/read_text_file".to_owned(),
                    }))
                }]
            });
            rsx! {
                Timeline {
                    entries,
                    turn: TurnState::Idle,
                    session: Some(v1::SessionId::new("sess-1")),
                    connected: true,
                    held_frames: 0..u64::MAX,
                    blocked: false,
                    problem: None,
                    mode: None,
                    on_prompt: move |_: Vec<v1::ContentBlock>| Ok(()),
                    on_stop: move |()| {},
                    on_set_mode: move |_| {},
                    on_answer: move |_: Answer| {},
                    on_elicit: move |_: elicitation::Answer| {},
                    sought: None,
                    on_reveal: move |_: Vec<u64>| {},
                    described: false,
                    turns: Vec::new(),
                }
            }
        }

        let mut dom = VirtualDom::new_with_props(
            Host,
            HostProps {
                frame: frame.to_owned(),
                closed,
            },
        );
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    /// The raw block of the first piece of evidence on the screen.
    fn evidence_block(html: &str) -> String {
        html.split_once(r#"<pre class="raw">"#)
            .expect("an entry's raw evidence")
            .1
            .split_once("</pre>")
            .expect("a complete block")
            .0
            .to_owned()
    }

    #[test]
    fn an_entrys_evidence_is_laid_out_where_it_is_open() {
        // The same layout call the Trace makes (`crate::indent`).
        let frame = r#"{"jsonrpc":"2.0","method":"fs/read_text_file"}"#;
        let html = indented_evidence(frame, false);

        assert_eq!(
            evidence_block(&html),
            "{\n  &#34;jsonrpc&#34;: &#34;2.0&#34;,\n  &#34;method&#34;: &#34;fs/read_text_file&#34;\n}",
            "the bytes are moved apart and none of them are changed"
        );
        assert!(
            html.contains(r#"aria-label="Copy this frame""#),
            "and what a copy takes is still the frame: {html}"
        );

        // Closed by the reader, it is the bytes as they arrived.
        assert_eq!(
            evidence_block(&indented_evidence(frame, true)),
            "{&#34;jsonrpc&#34;:&#34;2.0&#34;,&#34;method&#34;:&#34;fs/read_text_file&#34;}"
        );
    }

    #[test]
    fn evidence_that_is_not_json_is_drawn_as_it_arrived_either_way() {
        for closed in [true, false] {
            assert_eq!(
                evidence_block(&indented_evidence("not json at all", closed)),
                "not json at all"
            );
        }
    }

    #[test]
    fn a_row_says_which_frames_it_was_made_of_and_offers_the_way_to_them() {
        let html = screen(vec![vec![Some(4), Some(5)]], false, None);

        assert!(
            html.contains(r#"data-frames="4 5""#) && html.contains(r#"tabindex="0""#),
            "the row names its frames and accepts destination focus: {html}"
        );
        // The row itself is the control.
        assert!(
            html.contains(r#"class="row linked agent""#),
            "the row leads to its frames: {html}"
        );
        // And the same act is named for the keyboard.
        assert!(
            html.contains(r#"data-slot="reach""#) && html.contains("Show 2 frames"),
            "and offers the way to them by name: {html}"
        );
        assert!(
            !html.contains(r#"data-slot="wire""#),
            "a row this window has a rendering for keeps its frames in the wire log: {html}"
        );
    }

    #[test]
    fn an_entry_made_of_frames_the_trace_never_numbered_offers_no_way_to_them() {
        // Frames this client sent carry no ordinal, so there is nothing to
        // reach.
        let html = screen(vec![vec![None]], false, None);

        assert!(
            html.contains(r#"data-frames="""#),
            "the row names no frames: {html}"
        );
        assert!(
            !html.contains(r#"data-slot="reach""#),
            "so there is nothing to reach: {html}"
        );
    }

    #[test]
    fn an_entry_whose_frames_left_the_bounded_trace_offers_no_dead_reach_action() {
        let html = screen_with_held(vec![vec![Some(4), Some(5)]], false, None, 6..10);

        assert!(html.contains(r#"data-frames="4 5""#), "{html}");
        assert!(
            !html.contains(r#"data-slot="reach""#),
            "an entry remains readable after its Trace destination expires, without a dead control: {html}"
        );
    }

    #[test]
    fn the_row_a_reader_came_from_the_console_to_find_is_the_one_marked() {
        let html = screen(vec![vec![Some(7)], vec![Some(9)]], false, Some(9));

        assert_eq!(
            html.matches("row sought").count(),
            1,
            "one row is marked: {html}"
        );
        let marked = html
            .split("<article")
            .find(|row| row.contains("row sought"))
            .expect("the marked row");
        assert!(
            marked.contains(r#"data-frames="9""#),
            "and it is the one asked for: {html}"
        );
    }

    #[test]
    fn a_turn_waiting_on_the_reader_says_so_where_it_can_be_heard() {
        // A request may arrive off screen, so the stop is announced; one
        // sentence for either kind (§7.8). The control beside it needs a real
        // request, which only core can mint.
        let html = screen(vec![vec![Some(1)]], true, None);

        assert!(
            html.contains(r#"aria-live="polite""#) && html.contains(r#"role="status""#),
            "a live region says it: {html}"
        );
        assert!(
            html.contains("The agent is waiting on you."),
            "and says that something is being waited on: {html}"
        );
    }

    #[test]
    fn a_turn_nobody_is_waiting_on_says_nothing_at_all() {
        // The live region is always present, and empty here.
        let html = screen(vec![vec![Some(1)]], false, None);

        assert!(
            html.contains(r#"data-slot="waiting""#),
            "the region is here: {html}"
        );
        assert!(
            !html.contains("The agent is waiting on you."),
            "and it says nothing: {html}"
        );
    }

    #[test]
    fn transport_and_envelope_findings_each_read_as_one_plain_line() {
        let said = |unrecognized: Unrecognized| {
            dioxus_ssr::render_element(unnamed(&EntryKind::Unrecognized(unrecognized)))
        };
        let foreign = said(Unrecognized::ForeignSession {
            session: v1::SessionId::new("other"),
        });
        assert!(
            foreign.contains("An update for session <code>other</code>, not the open one."),
            "{foreign}"
        );
        let dropped = said(Unrecognized::Dropped {
            length: 12 * 1024 * 1024,
            limit: 10 * 1024 * 1024,
        });
        assert!(
            dropped.contains("A frame of 12 MB was dropped (limit 10 MB)."),
            "{dropped}"
        );
        let mistyped = said(Unrecognized::MistypedId {
            id: r#""3""#.to_owned(),
            call: 3,
        });
        assert!(
            mistyped.contains("An answer for call 3 with id <code>&#34;3&#34;</code> (a string). The call is still waiting."),
            "{mistyped}"
        );
        let float = said(Unrecognized::MistypedId {
            id: "3.0".to_owned(),
            call: 3,
        });
        assert!(float.contains("(not a whole number)"), "{float}");
        let unaddressed = said(Unrecognized::Unaddressed {
            error: v1::Error::new(-32700, "Parse error"),
        });
        assert!(
            unaddressed.contains("An error with no id: <code>-32700</code> Parse error"),
            "{unaddressed}"
        );
        let refused = said(Unrecognized::OverCapacity {
            method: "session/request_permission".to_owned(),
            limit: 100,
        });
        assert!(
            refused.contains("Over 100 requests waiting, so this <code>session/request_permission</code> was refused."),
            "{refused}"
        );
        let ignored = said(Unrecognized::Ignored {
            method: "_auth/status_update".to_owned(),
        });
        assert!(
            ignored.contains("The agent sent <code>_auth/status_update</code>, a notification this client does not know."),
            "{ignored}"
        );
        assert!(
            !ignored.contains("answered"),
            "and claims no answer was sent: {ignored}"
        );
        for html in [foreign, dropped, mistyped, unaddressed, refused, ignored] {
            assert!(
                !html.contains("no rendering for"),
                "each has its own line: {html}"
            );
        }
    }

    thread_local! {
        static ARRIVING: std::cell::RefCell<Option<Signal<Vec<TimelineEntry>>>> =
            const { std::cell::RefCell::new(None) };
    }

    #[test]
    fn only_a_row_that_arrives_after_the_list_was_drawn_fades_up() {
        // Rows the list opened with are still; a later row fades up once.
        #[component]
        fn Host() -> Element {
            let entries = use_signal(|| vec![spoke("already here", Some("m0"))]);
            use_hook(move || ARRIVING.with(|slot| *slot.borrow_mut() = Some(entries)));
            rsx! {
                Timeline {
                    entries,
                    turn: TurnState::InFlight,
                    session: Some(v1::SessionId::new("sess-1")),
                    connected: true,
                    held_frames: 0..u64::MAX,
                    blocked: false,
                    problem: None,
                    mode: None,
                    on_prompt: move |_: Vec<v1::ContentBlock>| Ok(()),
                    on_stop: move |()| {},
                    on_set_mode: move |_| {},
                    on_answer: move |_: Answer| {},
                    on_elicit: move |_: elicitation::Answer| {},
                    sought: None,
                    on_reveal: move |_: Vec<u64>| {},
                    described: false,
                    turns: Vec::new(),
                }
            }
        }
        let mut dom = VirtualDom::new(Host);
        dom.rebuild_in_place();
        let opened = dioxus_ssr::render(&dom);
        assert!(
            opened.contains("already here") && !opened.contains("data-arrived"),
            "what the list opened with does not fade up: {opened}"
        );

        let mut entries = ARRIVING.with(|slot| slot.borrow().expect("the host ran"));
        entries.write().push(TimelineEntry {
            id: EntryId::Arrival(2),
            ..spoke("just arrived", Some("m1"))
        });
        dom.render_immediate_to_vec();
        let grown = dioxus_ssr::render(&dom);
        let arrived = grown
            .split("<article")
            .filter(|row| row.contains(r#"data-arrived="true""#))
            .collect::<Vec<_>>();
        assert_eq!(arrived.len(), 1, "{grown}");
        assert!(arrived[0].contains("just arrived"), "{grown}");
    }

    #[test]
    fn byte_counts_read_in_the_unit_a_reader_would_use() {
        assert_eq!(sized(512), "512 bytes");
        assert_eq!(sized(1536), "1.5 KB");
        assert_eq!(sized(10 * 1024 * 1024), "10 MB");
        assert_eq!(sized(10 * 1024 * 1024 + 1), "10 MB");
    }
}
