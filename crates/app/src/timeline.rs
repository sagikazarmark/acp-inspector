//! The turn timeline (`docs/architecture.md` §9): the center screen, where a
//! turn is watched as it happens.
//!
//! **A rendering of core's timeline store and nothing more.** Entries arrive
//! decoded as v1 where the typed layer recognized them and raw where it did
//! not; a tool call that streamed four updates arrives as one entry that
//! changed four times, because merging is the store's job (§11 seam 3) and this
//! window would only be guessing at it. What is decided here is what a reader
//! sees, which is the one thing core has no opinion about.
//!
//! Two of those decisions are worth naming. **Chunks are gathered into the
//! message they are chunks of** — a stream of forty `agent_message_chunk`
//! entries is one paragraph and not forty rows, and every entry behind it is
//! still there under the disclosure. That is not the store's merge moved up
//! here: the store keeps every chunk as its own entry on purpose, because each
//! one is a frame that has to stay readable as itself (§8), and this only
//! decides how they *read* — which is the question core has no opinion about.
//! What core owns is identity, and this file never invents any: it groups what
//! the agent already said belongs together, by `messageId` and by adjacency,
//! and keys its rows by the ids core gave them. And **an entry whose point is
//! the frames shows them without being asked**, which is two cases. Traffic
//! this window cannot name is one (§8): an unrecognized entry, or a v1 update
//! variant this file has no arm for, is opened rather than summarized, because
//! for traffic nobody has a name for yet the raw *is* the rendering. A
//! conformance annotation (§15 q7) is the other: its whole claim is about the
//! frames it carries, and evidence a click away would be an assertion the
//! reader has to take on trust.

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

/// Put the Timeline entry named by another control under both the viewport and
/// keyboard focus. The ordinal is numeric and sent over Dioxus's channel, so no
/// Agent-controlled text becomes executable code.
const FOCUS_ENTRY: &str = r#"
const ordinal = await dioxus.recv();
await new Promise((resolve) => requestAnimationFrame(resolve));
const target = document.querySelector(`[data-frames~="${ordinal}"]`);
if (target) {
  target.scrollIntoView({ block: "center" });
  target.focus({ preventScroll: true });
}
"#;

/// The entry list, by the name the scroll watcher and its control know it
/// under (`tail.rs`).
pub(crate) const ENTRIES: &str = "entries";

/// Focus a Timeline entry after it has rendered.
pub(crate) fn focus_entry(ordinal: u64) {
    let focusing = document::eval(FOCUS_ENTRY);
    let _ = focusing.send(ordinal);
}

/// The center screen: what the agent has said this session, and the composer
/// that starts the next turn.
#[component]
pub fn Timeline(
    entries: ReadSignal<Vec<TimelineEntry>>,
    turn: TurnState,
    session: Option<v1::SessionId>,
    /// Whether the agent that opened the session is still there. A session
    /// outlives its agent in the store on purpose — what happened stays
    /// readable — so it is not on its own an answer to "can this prompt".
    connected: bool,
    /// The half-open ordinal range the bounded Trace still holds. Timeline
    /// entries outlive cleared or aged-out Frames, so Reach is offered only
    /// while at least one Frame it names still has a destination.
    held_frames: std::ops::Range<u64>,
    /// Whether the agent is waiting on an answer — core's count of the requests
    /// nobody has answered yet, not a scan of the rows below.
    blocked: bool,
    /// Which trace row's entry the reader asked to be shown, sent from the
    /// Console by somebody reading the wire (§9's two screens, asked to point
    /// at each other). The ordinal the frame was captured under, which is what
    /// an entry keeps about its own frames.
    sought: Option<u64>,
    /// The frames of an entry, on their way to the Console.
    on_reveal: EventHandler<Vec<u64>>,
    /// Whether there is an agent that has described itself, which is what makes
    /// the session control in the header answerable. The two accounts of what
    /// was claimed are the rail's now (`crate::rail`); this screen needs to know
    /// only that there is somebody to open a session on.
    described: bool,
    /// The turns this session has had, and what each came to — core's record,
    /// read beside the entries it stamped. The lines between the groups are
    /// drawn from this and never worked out from the rows, which is the same
    /// rule every other reading on this screen follows (§5).
    turns: Vec<Turn>,
    /// Why there is no session, when the handshake that opens one failed.
    problem: Option<CallError>,
    /// How to open one from here, where there is an agent that could — the same
    /// call the Agent screen's own affordance makes, offered where the absence
    /// is stated. `None` while there is no agent that has described itself,
    /// which is the same gate that button is drawn behind: an empty screen must
    /// not offer a call nobody can answer.
    on_new_session: Option<EventHandler<()>>,
    /// The mode the next prompt goes out under, for the composer's own control
    /// (`composer::Cycle`).
    mode: Option<composer::Cycle>,
    on_prompt: EventHandler<String>,
    on_stop: EventHandler<()>,
    on_set_mode: EventHandler<v1::SessionModeId>,
    /// A permission option the user picked, on its way back to core.
    on_answer: EventHandler<Answer>,
    /// What a reader said to an elicitation, on its way back to core (§7.8).
    /// Its own handler rather than one widened callback: the two answers are
    /// different sentences and core takes them by different methods.
    on_elicit: EventHandler<elicitation::Answer>,
) -> Element {
    let blocks = blocks(entries());
    let count = blocks.len();
    let commands = commands(&entries());
    let rows = rows(blocks, &turns);
    let crossing = Crossing {
        sought,
        reveal: on_reveal,
        held: (held_frames.start, held_frames.end),
    };

    // Where the tool stopped, if it has. Read off the rows rather than from
    // `blocked`, because what this reaches is a row: the flag says a request is
    // unanswered and this says which frame the row that asks it was made of.
    // The newest of *either* kind (§7.8), because what a reader is being sent to
    // is the question nobody has answered, and which sort of question it is is
    // not a distinction their next action turns on.
    let waiting = blocked
        .then(|| {
            entries()
                .iter()
                .rev()
                .find(|entry| {
                    matches!(
                        entry.kind,
                        EntryKind::Permission(_) | EntryKind::Elicitation(_)
                    )
                })
                .and_then(|entry| entry.frames.iter().find_map(|recorded| recorded.captured))
        })
        .flatten();

    rsx! {
        main { class: "turns",
            header { class: "pane-head",
                h2 { class: "pane-title", "Session" }
                // Which call the rows under this belong to, in the protocol's
                // own words: a reader who has scrolled back through four turns
                // is reading a screen that says which one is current.
                if let Some(turn) = turns.last() {
                    span { class: "pane-scope", "{v1::AGENT_METHOD_NAMES.session_prompt} · turn {turn.id}" }
                }
                // **The session is named where the session is drawn**, which is
                // the rail: its id whole, the way to another, and the way to the
                // ones the agent is holding (`crate::rail`). It was named here
                // too and drawn as a control, so the header of the screen the
                // session is *read* on carried the acts on it — a switcher, a
                // copy and a chevron between the region's name and its state,
                // which is three controls in the one place the drawing keeps
                // clear.
                //
                // Where the turn stands, at the far end of the region's own
                // header, which is where a desktop application puts the state of
                // the thing a region is showing. The *sentence* explaining it
                // stays above the composer, so no word is drawn twice.
                span { class: "grow" }
                {composer::badge(&composer::presented(&turn, blocked, problem.as_ref(), connected && session.is_some(), connected))}
            }

            if count == 0 {
                div { class: "empty", "data-slot": "timeline-empty",
                    // Two empties, and they are not the same news. A session
                    // with nothing said in it yet is waiting for the agent; no
                    // session at all is where closing or deleting the live one
                    // leaves this (§7.5), and a screen that said the same thing
                    // about both would have the reader waiting for an agent
                    // that has nothing to talk in. Where the record went is said
                    // with it, because the timeline is the only thing a switch
                    // costs.
                    // **And the one thing that resolves it, where the state is
                    // said.** An empty screen that names a state and offers
                    // nothing is a screen that has told the reader to go and
                    // find the control themselves — on a window where the
                    // control is a rail away and, in the disconnected case,
                    // four fields deep. Two of the three states have such a
                    // thing; the third is waiting on the agent, and an offer
                    // there would be a button for *be patient*.
                    if session.is_none() && connected {
                        h3 { "No session is open" }
                        p { "Traffic from closed, deleted or switched-away Sessions remains in Trace." }
                        if let Some(open) = on_new_session {
                            button {
                                class: "btn btn-md btn-filled",
                                "data-slot": "empty-action",
                                r#type: "button",
                                title: "Open a new session on the running agent",
                                onclick: move |_| open.call(()),
                                "session/new"
                            }
                        }
                    } else if session.is_some() {
                        h3 { "Waiting for session updates" }
                        p {
                            code { "session/update" }
                            " entries appear here, decoded where v1 names them and raw otherwise."
                        }
                    } else {
                        h3 { "No session updates yet" }
                        p { "Launch an Agent to open a Session and watch its updates here." }
                        // The one thing an empty screen with no agent behind it
                        // can offer. It opens the fields rather than filling
                        // them: nothing is typed for the reader and nothing is
                        // spawned — the offer is *here is where you start*.
                        //
                        // **Which is why it stopped saying `Launch an agent`.**
                        // The comment above is what the control does and the
                        // label was what it does not: it opens a form, and
                        // launching is the button *inside* that form. It was
                        // also the third word this window used for one act —
                        // `Connect…` in the bar, `Connect to an agent` on the
                        // dialog, `Launch an agent` here — with two of the three
                        // on screen together, both opening the same dialog, so
                        // a reader had to press one to find out they were the
                        // same offer. One flow, one word, and `Launch` is left
                        // to the control that does it.
                        button {
                            class: "btn btn-md btn-filled",
                            "data-slot": "empty-action",
                            r#type: "button",
                            title: "Open the launch fields",
                            onclick: move |_| crate::connect::open(),
                            "Connect…"
                        }
                    }
                }
            } else {
                div { class: "tailing",
                div { class: "turn-stream", "data-tail": ENTRIES,
                ol { class: "stream",
                    // Newest first in the DOM, oldest first on screen, like the
                    // trace and the console: the list is laid out bottom-up, so
                    // a turn streaming into it stays in view without a scroll
                    // script, and stays where the reader put it once they
                    // scroll back.
                    for row in rows.into_iter().rev() {
                        li {
                            key: "{row.key()}",
                            match &row {
                                Row::Block(block) => block.render(on_answer, on_elicit, crossing),
                                Row::Ended(turn) => ended(turn),
                                Row::Outside => outside(),
                            }
                        }
                    }
                }
                }
                // Beside the list and not in it: a control inside the scroller
                // is a row of it, and this is about the list rather than in it.
                {crate::tail::to_latest(ENTRIES, "entry")}
                }
            }

            // That the agent is waiting on the reader, said where a screen
            // reader hears it and drawn where the reader can act on it (§7.2).
            //
            // **The region is always here, and mostly empty.** An announcement
            // is made by text *changing* inside a live region, so one that
            // appeared along with its sentence would announce nothing at all —
            // which is the whole of what this is for on the one surface where
            // the tool has stopped and is waiting to be told what to do.
            //
            // It is not a badge and does not grade anything (§9): a blocking
            // request is the agent's turn stopping, not a fault of the agent's,
            // and what this says is where it stopped and how to get there. The
            // entry list has no scroll script by design — it stays where the
            // reader put it — so a request that arrives while they are reading
            // further back arrives off screen, and this is the way to it.
            div { class: "waiting", "data-slot": "waiting", role: "status", aria_live: "polite",
                if blocked {
                    span { "The agent is waiting on you." }
                    if let Some(ordinal) = waiting {
                        button {
                            class: "btn btn-xs btn-quiet",
                            "data-slot": "reach-request",
                            title: "Move focus to the request the agent is waiting on",
                            onclick: move |_| focus_entry(ordinal),
                            "go to it"
                        }
                    }
                }
            }

            // The composer belongs to the turn, so it is drawn where the turn
            // is. The Connection's own Stop is in the toolbar and never moves.
            //
            // **And it is drawn where there is a turn to have.** A disabled
            // prompt box is a control, and a control that cannot be used says
            // *this is how you drive this window* to a reader who cannot drive
            // it — so with no Session open it was a hundred and thirty pixels of
            // dead chrome carrying a placeholder that named the two calls which
            // open one. Every word of that is already on the empty screen
            // directly above it, in a heading and a sentence rather than in grey
            // text inside a box nobody can type in, and *with the control that
            // resolves it* — `session/new` where there is an agent, `Connect…`
            // where there is not. The screen said it better and the box said it
            // second.
            //
            // The exception is the whole reason it is a condition rather than
            // `ready`: a handshake that failed has no Session and its own
            // account of why, and this is the surface that draws it. A launch
            // that never reached a Session must not fail silently.
            if session.is_some() || problem.is_some() {
                Composer {
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

/// What the two screens need of each other, carried to the row that draws it.
///
/// One value rather than two parameters through four functions: the reveal and
/// the mark are one feature — the screens point at each other — and a renderer
/// that took them apart would be able to draw half of it.
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
    /// The ordinals of the frames these entries were made of, in order.
    ///
    /// Only the ones the trace numbered: a Conformance Annotation is read
    /// beside a call this client *sent*, and the send path does not carry an
    /// ordinal back up (`acp_inspector_core::Recorded`). An entry made of nothing
    /// but those has no row to reach, and says so by offering no control.
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

    /// The ordinals of these frames the Trace can still be sent to — which is
    /// what a row leads to, and empty for a row that leads nowhere.
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
// An entry is the big variant and also the common one, so boxing it would buy a
// smaller enum at the price of an allocation per entry, every render — the
// wrong way round. The size is the schema's own, and core's `EntryKind` carries
// this same allowance for the same reason.
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
/// Grouping is by adjacency *and* identity: the same voice, and the same
/// `messageId` — which the protocol carries exactly so that a new message can
/// be told from a continuation. Chunks carrying no id group by adjacency alone,
/// because an agent that streams without ids has said nothing to separate them
/// by, and one sentence spread over forty rows is not what it said either.
/// Anything between two chunks ends the run, so the order on screen stays the
/// order on the wire.
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

/// What the agent last said it can be asked to run.
///
/// **The newest announcement whole, and never a merge of them.** An
/// `available_commands_update` carries the complete list the agent is offering
/// now, so a later one *replaces* the earlier: a composer that accumulated them
/// would offer a command the agent had stopped publishing, which is this window
/// inventing an affordance. The entries are the record either way — every
/// announcement is still a row, and the frames are in the trace.
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

/// The voice and the message of an entry that is a streamed chunk, if it is
/// one.
fn chunk(entry: &TimelineEntry) -> Option<(Voice, Option<&v1::MessageId>)> {
    let EntryKind::Update(update) = &entry.kind else {
        return None;
    };

    update::spoken(update).map(|(voice, chunk)| (voice, chunk.message_id.as_ref()))
}

/// A row of the centre screen: something that happened, or the line between one
/// turn and the next.
///
/// **The boundaries are read off the record and never inferred from the rows.**
/// Which turn an entry arrived in is core's stamp (`TimelineEntry::turn`) and
/// what a turn came to is core's record ([`Turn`]); what is decided here is
/// only where the line is drawn and what it says.
/// The variants are not the same size, and boxing the big one would trade the
/// bytes a `Vec<Block>` already spends for an allocation per row on a list that
/// is rebuilt as the agent streams into it.
#[allow(clippy::large_enum_variant)]
enum Row {
    /// Something the agent said, or the inspector said about it.
    Block(Block),
    /// A turn that is over, and what it came to. Drawn under its last row,
    /// because how a turn ended is something known at the end of it.
    Ended(Turn),
    /// The start of a run of traffic belonging to no turn. Drawn above it,
    /// because what those rows *are* is known before reading them — and because
    /// a run that is still arriving has no end to hang a label on.
    Outside,
}

/// The blocks with the turns they happened in marked between them.
///
/// A turn that ended having said nothing still gets its line: a `refusal` that
/// produced no updates is a turn that happened, and a screen that drew nothing
/// for it would be a screen where the prompt vanished.
fn rows(blocks: Vec<Block>, turns: &[Turn]) -> Vec<Row> {
    let mut rows = Vec::new();
    // The last turn whose ending has been drawn, so a turn is closed once and
    // whether or not it had anything in it.
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
    // The highest turn any row has been in, which is how far *outside a turn*
    // is allowed to close: traffic between turn one and turn two ends the
    // first and says nothing about the second.
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
                    rows.push(Row::Outside);
                }
            }
            current = Some(turn);
        }
        rows.push(Row::Block(block));
    }

    // And whatever finished after the last row, which is the ordinary case: the
    // turn the reader just watched.
    close_through(&mut rows, u64::MAX, &mut closed);
    rows
}

impl Row {
    /// Which row this is, for the list that renders it.
    fn key(&self) -> String {
        match self {
            Self::Block(block) => block.key(),
            Self::Ended(turn) => format!("turn {} ended", turn.id),
            // One per run, and a run is identified by what follows it — which
            // is the next block's key, assigned by the list rather than here.
            Self::Outside => "outside a turn".to_owned(),
        }
    }
}

impl Block {
    /// The turn this happened in, which is the turn its first entry arrived in.
    ///
    /// A block is consecutive chunks of one message, and a message that
    /// streamed across the end of a turn belongs to the turn it started in —
    /// the same rule the entry itself is stamped under.
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
    ) -> Element {
        match self {
            Self::Speech { voice, entries, .. } => said(*voice, entries, crossing),
            Self::One(entry) => happened(entry, on_answer, on_elicit, crossing),
        }
    }

    /// Which row this is, for the list that renders it: what its first entry is
    /// called ([`named`], on why it is the entry's own identity). The first
    /// chunk names a message, because a message keeps its first chunk however
    /// many follow it.
    fn key(&self) -> String {
        let identity = match self {
            Self::Speech { entries, .. } => entries.first().map(|entry| &entry.id),
            Self::One(entry) => Some(&entry.id),
        };

        identity.map(named).unwrap_or_default()
    }
}

/// What an entry is called wherever one row has to be told from another: the row
/// it is drawn as, and the disclosure its evidence is read in
/// ([`wire`]).
///
/// **The entry's own identity, not where it turned up** (§11 seam 3): a tool call
/// that changes in place keeps its name, and with it whatever the reader had
/// opened on it — where a name counted off the list would hand that open
/// disclosure to whichever entry happened to land there next.
fn named(id: &EntryId) -> String {
    match id {
        EntryId::ToolCall(session, call) => format!("tool call {session} {call}"),
        // The id the request arrived under, which is what a panel keeps its
        // buttons — and whatever the reader has opened on it — across the
        // re-render that answering it causes.
        EntryId::Permission(request) => format!("permission {request}"),
        // And the same for an elicitation, which keeps more across the
        // re-render answering causes: a half-filled form is the reader's
        // work, and a name that moved would hand it to another row.
        EntryId::Elicitation(request) => format!("elicitation {request}"),
        EntryId::Arrival(arrival) => format!("arrival {arrival}"),
        // `EntryId` is non-exhaustive, and an identity this window has not
        // been taught still has to be one: two rows sharing a name is worse
        // than a name nobody can read.
        other => format!("{other:?}"),
    }
}

/// A message, from however many chunks it arrived in.
fn said(voice: Voice, entries: &[TimelineEntry], crossing: Crossing) -> Element {
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

    rsx! {
        article {
            class: "{drawn_as(tone, crossing.found(&ordinals), !linking.is_empty())}",
            tabindex: "-1",
            "data-entry": "{tone}",
            // Every ordinal this row was made of, so the Console can name one
            // and the row can be found by it. A list, because a streamed
            // message is one row and as many frames as it arrived in.
            "data-frames": frames_attribute(&ordinals),
            // **The row is the way to the frames it was made of**, which is how
            // the drawing links the two screens: pressing a message shows the
            // traffic it came from, rather than a chip on the row saying so.
            onclick: move |_| {
                if !linked.is_empty() {
                    crossing.reveal.call(linked.clone());
                }
            },
            {marked(tone)}
            div { class: "row-body",
                {kind_said(label, at)}
                div { class: "said",
                    // One flow rather than one paragraph per chunk: the pieces
                    // a message arrived in are how it was sent, not how it
                    // reads.
                    for (position, block) in content.into_iter().enumerate() {
                        span { key: "{position}", {update::content(block)} }
                    }
                }
                {reach(&linking, crossing)}
            }
        }
    }
}

/// The line under a turn that is over, saying what it came to.
///
/// **The agent's own word for it**, in the register the composer says it in —
/// `stopped` is shared with that surface so the record of a turn and the state
/// of the live one cannot describe the same stop two ways. A turn that failed
/// says so with the error, because an inspector reporting a stop reason the
/// agent never gave is the one thing it may not do.
fn ended(turn: &Turn) -> Element {
    // The agent's own word for it, in the shape the drawing closes a turn with:
    // the field the answer carried, in the tone of what it says, and which turn
    // it was at the far end.
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
        // Not drawn for a turn that is still running: where the live turn
        // stands is the composer's to say, and this is the record of one that
        // is over.
        None => return rsx! {},
        // `TurnOutcome` is non-exhaustive. An ending this window has not been
        // taught is still an ending, and the frames behind it are in the trace.
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

/// The line above a run of traffic that belongs to no turn.
///
/// **The finding this whole record exists for.** An agent that goes on
/// streaming into a turn it has resolved, or starts talking before anything was
/// asked, produces rows that look exactly like the rows above them on a flat
/// list. Said in words rather than by a stripe, because it is a statement about
/// the traffic and not a severity: a replayed session is entirely out of turn
/// and entirely correct.
fn outside() -> Element {
    rsx! {
        div { class: "boundary outside", "data-slot": "outside-turn",
            span { class: "eyebrow", "Outside any turn" }
            span { "the agent said this with no prompt open" }
        }
    }
}

/// The ordinals a row was made of, as the attribute both screens read.
///
/// Space-separated, which is what makes `[data-frames~="12"]` a match on one of
/// them rather than a match on a row whose list happens to contain those
/// characters — a row made of frame 3 must not answer to a search for frame 31.
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
) -> Element {
    // Whether this window has a rendering is what decides everything about the
    // row, the disclosure included: an entry it cannot name shows its JSON
    // without being asked, because not having a rendering is no reason to show
    // the reader less than the agent sent (§8).
    let rendered = match &entry.kind {
        EntryKind::Update(update) => update::render(update),
        // A blocking request is not something that happened, it is something
        // happening: it renders in the flow like everything else, and the
        // buttons in it are what let the turn go on (§7.2).
        EntryKind::Permission(request) => {
            Some(("permission", "ask", permission::panel(request, on_answer)))
        }
        // And the other one, which is the same kind of row for the same reason
        // (§7.8): the agent stopped here, and what is in this one is how it goes
        // on.
        EntryKind::Elicitation(request) => Some((
            "elicitation",
            "ask",
            rsx! {
                elicitation::Panel { request: request.clone(), on_answer: on_elicit }
            },
        )),
        // A conformance annotation, in the flow where the traffic it is about
        // was — never a modal, a toast, or a lane of its own (§15 q7).
        EntryKind::Annotation(annotation) => Some(("conformance", "spec", claim(annotation))),
        // `EntryKind` is non-exhaustive, and a kind of entry this window has
        // not been taught goes the way an unrecognized one does.
        _ => None,
    };
    let named = rendered.is_some();
    let (label, tone, body) =
        rendered.unwrap_or_else(|| ("unrecognized", "unnamed", unnamed(&entry.kind)));

    let entries = std::slice::from_ref(entry);
    let ordinals = Crossing::ordinals(entries);
    // The id a tool call carries, so an elicitation raised inside one can be a
    // control that takes the reader to it (§7.8). Only tool calls have one, and
    // only they are ever named this way.
    let tool_call = match &entry.id {
        EntryId::ToolCall(_, call) => Some(call.to_string()),
        _ => None,
    };

    let linking = crossing.linking(&ordinals);
    let linked = linking.clone();
    // The frames, drawn under the row only where they *are* the rendering:
    // traffic this window cannot name, and a conformance annotation whose claim
    // is about the frames themselves (§8). Everywhere else the row leads to
    // them and the wire log draws them whole, which is the arrangement §9 asks
    // the two screens for.
    let shows_frames = evidence(entry, named);

    rsx! {
        article {
            class: "{drawn_as(tone, crossing.found(&ordinals), !linking.is_empty())}",
            tabindex: "-1",
            "data-entry": "{tone}",
            "data-tool-call": tool_call,
            "data-frames": frames_attribute(&ordinals),
            onclick: move |_| {
                if !linked.is_empty() {
                    crossing.reveal.call(linked.clone());
                }
            },
            {marked(tone)}
            div { class: "row-body",
                {kind_said(label, Some(entry.at))}
                div { class: "said", {body} }
                if shows_frames {
                    {frames_of(entries)}
                }
                {reach(&linking, crossing)}
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

/// The mark in an entry's gutter, which is what makes a streamed conversation
/// scannable: the eye runs down one column of glyphs rather than reading every
/// kind word on every row.
///
/// **Decoration, and said to be.** Every mark repeats a word the row already
/// carries — the entry kind is beside it, in text, on every row — so a reader
/// who cannot see the glyph loses nothing and one who would otherwise hear it
/// named gains nothing. It is keyed on the tone rather than on the kind because
/// the tone is what one row shares with another of its sort, which is exactly
/// what a column of marks is for.
fn marked(tone: &str) -> Element {
    let glyph = match tone {
        "user" => "›",
        "agent" => "◆",
        "thought" => "~",
        "tool" => "⌘",
        "plan" => "≡",
        "ask" => "!",
        "spec" => "§",
        "unnamed" => "?",
        // `note` and anything a later ring adds: a dot is what a row with no
        // sort of its own is worth, and never a glyph invented for it.
        _ => "·",
    };

    rsx! {
        span { class: "row-mark", aria_hidden: "true", "{glyph}" }
    }
}

/// A conformance annotation, in the two sentences core wrote for it (§15 q7).
///
/// **The wording is not this window's.** What the specification requires and
/// what was observed instead are computed below the presentation layer and
/// asserted there, so a second surface says the same thing — and the day
/// another qualifying MUST is admitted, the rule is a variant in core and
/// nothing here changes at all.
fn claim(annotation: &Annotation) -> Element {
    rsx! {
        p { class: "requires", "{annotation.requires()}" }
        p { class: "observed", "{annotation.observed()}" }
    }
}

/// Whether an entry's raw JSON is part of its rendering rather than a
/// disclosure under it.
///
/// True wherever the frames are the point. Traffic this window cannot name is
/// one case (§8): not having a rendering is no reason to show the reader less
/// than the agent sent. A conformance annotation is the other — its whole claim
/// is about the frames it carries, and evidence a click away would be an
/// assertion the reader has to take on trust.
fn evidence(entry: &TimelineEntry, named: bool) -> bool {
    !named || matches!(entry.kind, EntryKind::Annotation(_))
}

/// An entry with no rendering, said in words (§8).
///
/// It is a finding, not an error: an agent calling a service this client
/// declined by advertising no capability, or sending an update v1 has no name
/// for, is the most interesting thing that can happen in an inspector — so the
/// entry says what happened, and its JSON is open underneath.
fn unnamed(kind: &EntryKind) -> Element {
    rsx! {
        p { class: "unnamed",
            {match kind {
                EntryKind::Unrecognized(Unrecognized::NotServiced { method }) => rsx! {
                    "The agent called "
                    code { "{method}" }
                    ", which this client does not service — it advertised no capability for it, and answered method-not-found."
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
                // A `session/update` variant this window has no arm for: an
                // unstable-v1 one, an extension, or v2 arriving early.
                EntryKind::Update(_) => rsx! {
                    "A v1 update this window has no rendering for."
                },
                // Both enums are non-exhaustive, and evidence this window has
                // not heard of is still evidence.
                _ => rsx! { "Something this window has no rendering for." },
            }}
            " What arrived is below, as it arrived."
        }
    }
}

/// The kind of a row, in words, and the visible label where the drawing gives
/// one.
///
/// **The mark in the gutter is decoration, so this is what carries the kind**
/// — and where the drawing draws no label the word is in the accessibility
/// tree rather than gone. Three rows are drawn bare: the agent's message,
/// which is the message and nothing else, and the cards that name themselves
/// in their own head (a tool call, a plan, a request the turn stopped on),
/// where a second word above the card would be the same fact twice.
///
/// The time is not on the row at all. Every frame in the wire log carries one,
/// which is where a reader asking *when* is asking, and a column of stamps down
/// a conversation is a column nobody reads.
fn kind_said(label: &str, at: Option<SystemTime>) -> Element {
    /// The rows whose body says what they are.
    const NAMED_BY_ITS_BODY: [&str; 5] = [
        "agent_message_chunk",
        "tool call",
        "plan",
        "permission",
        "elicitation",
    ];

    rsx! {
        if NAMED_BY_ITS_BODY.contains(&label) {
            span { class: "sr-only", "{label}" }
        } else {
            div { class: "row-head",
                span { class: "eyebrow", "{label}" }
                // Kept out of sight rather than dropped: what the row is
                // *about* has a time in the record, and a reader who cannot see
                // the flow still hears the order it happened in.
                if let Some(at) = at {
                    span { class: "sr-only", " at {stamp(at)}" }
                }
            }
        }
    }
}

/// The frames an entry was decoded from, drawn where they are the rendering.
///
/// Every entry keeps its frames (§8) and most of them keep them *in the wire
/// log*, which the drawing puts beside the turn for exactly this: a row leads
/// to its traffic ([`reach`]) and the pane there reads it whole. What is drawn
/// here is the case where the frames are not evidence *for* a rendering but the
/// rendering itself — traffic this window cannot name, and a conformance
/// annotation whose claim is about the bytes.
///
/// Laid out where the reader asked for that (`crate::indent`) — the same call
/// the wire log makes about the same frame, so the two screens cannot come to
/// draw one frame two ways. What the copy beside it takes is the frame's own
/// text regardless: whitespace is for reading, and what leaves this window has
/// to be what crossed.
fn frames_of(entries: &[TimelineEntry]) -> Element {
    let frames: Vec<Recorded> = entries
        .iter()
        .flat_map(|entry| entry.frames.iter().cloned())
        .collect();
    // One window-wide name for the layout the reader asked for, which is this
    // entry's own identity rather than where it turned up (§11 seam 3).
    let key = entries
        .first()
        .map(|entry| format!("evidence {}", named(&entry.id)))
        .unwrap_or_default();

    rsx! {
        div { class: "raws", "data-slot": "wire",
            for (position, recorded) in frames.into_iter().enumerate() {
                div { key: "{position}", class: "raw-row",
                    pre {
                        class: "raw",
                        "{crate::indent::drawn(&key, true, recorded.frame.as_str())}"
                    }
                    Copy { text: recorded.frame.to_string(), what: "this frame" }
                }
            }
        }
    }
}

/// The way from a row to the frames it was made of, for a reader who is not
/// pointing at anything.
///
/// **The row itself is the control** — the drawing puts no chip on a message,
/// and pressing one shows the traffic it came from — so what is left to give is
/// the same act, named, to a keyboard and to anything reading the document. It
/// is off screen until it has focus, which is the skip link's pattern and the
/// opposite of the hidden-until-hover one this window forbids: what is revealed
/// by *arriving at it* can be looked for, and what is revealed by sweeping a
/// pointer over the page cannot.
///
/// Nothing at all where the trace no longer holds the frames: a bounded store
/// drops its oldest rows, and a control that led nowhere would be worse than
/// none.
fn reach(ordinals: &[u64], crossing: Crossing) -> Element {
    let reach = ordinals.to_vec();
    let count = reach.len();

    rsx! {
        if count > 0 {
            button {
                class: "btn btn-xs btn-quiet reach",
                "data-slot": "reach",
                r#type: "button",
                title: if count == 1 { "Find this frame in the wire log" } else { "Find these frames in the wire log" },
                onclick: move |_| crossing.reveal.call(reach.clone()),
                if count == 1 { "in the wire log" } else { "{count} frames in the wire log" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use acp_inspector_core::{Ending, EntryId, OptionSet, Replay};

    use super::*;

    /// The conformance annotation's own row, read by what the entry is rather
    /// than by what paints it: every entry publishes its kind, and `spec` is
    /// the inspector's own voice among the agent's (§15 q7).
    const ANNOTATION: &str = r#"data-entry="spec""#;

    /// Every raw-JSON disclosure on the screen, in order, and whether each one
    /// is open.
    ///
    /// The disclosure says which it is on the element itself, so this reads the
    /// tag rather than the class: whether an entry's evidence is on screen is
    /// the assertion, and how a `details` is drawn is not.
    /// How many rows draw their frames under them, which is how many rows the
    /// frames are the rendering *of*.
    fn wires(html: &str) -> usize {
        html.matches(r#"data-slot="wire""#).count()
    }

    fn entry(kind: EntryKind) -> TimelineEntry {
        in_turn(kind, Some(1))
    }

    /// The same, saying which turn it arrived in — `None` for traffic that
    /// belongs to no turn at all.
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

    /// How many chunks each block gathered — zero for a block that is one
    /// entry of something else.
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
        // An agent that streams without ids has said nothing to separate its
        // chunks by, and one sentence spread over four rows is not what it said
        // either.
        let blocks = blocks(vec![spoke("one ", None), spoke("sentence", None)]);

        assert_eq!(gathered(&blocks), [2]);
    }

    #[test]
    fn a_new_message_id_starts_a_new_block() {
        // What `messageId` is for: a change of id is a new message, which is
        // the one thing adjacency cannot tell.
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
        // The order on screen is the order on the wire: a tool call that
        // happened mid-message is not something to render around.
        let blocks = blocks(vec![
            spoke("before", None),
            tool_call(),
            spoke("after", None),
        ]);

        assert_eq!(gathered(&blocks), [1, 0, 1]);
    }

    /// A message, and an annotation about the traffic around it, rendered — the
    /// screen itself and not a value on the way to it.
    ///
    /// It takes the annotation because there is more than one rule now (§15
    /// q7), and how a rule is surfaced is the thing that was promised not to
    /// change when the list grew: every rule takes this same path.
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
                    on_prompt: move |_: String| {},
                    on_stop: move |()| {},
                    on_set_mode: move |_| {},
                    on_answer: move |_: Answer| {},
                    on_elicit: move |_: elicitation::Answer| {},
                    sought: None,
                    on_reveal: move |_: Vec<u64>| {},
                    // These are about a row rather than about the turns it sat
                    // in, so there is no record and no line is drawn.
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
        // Where the ring's rule about annotations lands on a screen: it is a
        // row among the rows, carrying what the specification requires, what
        // was observed instead, and the frames that decide it — never a modal,
        // a toast, or a panel of its own (§15 q7).
        let html = shown(cancelled());

        assert!(
            html.contains("must still resolve"),
            "it says what the specification requires: {html}"
        );
        assert!(
            html.contains("end_turn"),
            "and what was observed instead: {html}"
        );

        // A row among the rows: the message and the annotation are two `li` of
        // the one list, so the annotation is drawn where the traffic was rather
        // than lifted out of it. Nothing here opens a layer over the screen —
        // the two shapes an annotation was told not to take.
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
        // The frames are the claim's evidence, so they are on screen with it:
        // an assertion about traffic the reader has to go looking for is one
        // they have to take on trust. The message above it keeps none — its
        // frames are in the wire log, one press of the row away — which is what
        // makes this a property of annotations rather than of the whole screen.
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
        // **No disclosure over it, because there is nothing to disclose**: these
        // frames are not evidence *for* a rendering, they are the rendering, and
        // a chevron over them would be a control whose closed state hides the
        // whole of what the row says. The drawing has no such control anywhere
        // in the flow, and the rows that keep their frames elsewhere reach them
        // through the wire log instead.
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
        // What #79 promised the day another qualifying MUST was admitted: the
        // rule is a variant in core, its two sentences are core's, and nothing
        // here changed to draw it (§15 q7). The load-ordering rule takes the
        // same row, with the frames that decide it open beneath it.
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
        // The same promise kept a second time (§15 q7): the incomplete-option-set
        // rule is a variant in core, its two sentences are core's, and nothing
        // here changed to draw it — the row, and its evidence open beneath it.
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

    /// What the row stream is, as the kinds of row it is made of — which is
    /// what the boundaries are: rows among the rows, in the order they are read.
    fn shape(blocks: Vec<Block>, turns: &[Turn]) -> Vec<&'static str> {
        rows(blocks, turns)
            .iter()
            .map(|row| match row {
                Row::Block(_) => "block",
                Row::Ended(_) => "ended",
                Row::Outside => "outside",
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

        // And a turn still running has no line: where the live turn stands is
        // the composer's to say, and a boundary drawn under a turn that has not
        // finished would be this screen answering for the agent.
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

        // The finding the record exists for: the agent went on talking after it
        // said the turn was over. The turn closes first, and what follows says
        // what it is.
        assert_eq!(
            shape(vec![spoke_in(Some(1)), spoke_in(None)], &ended),
            ["block", "ended", "outside", "block"],
        );

        // One marker per run rather than one per row: it is a statement about
        // what follows, and repeating it on every row would be the texture this
        // screen just had taken off it.
        assert_eq!(
            shape(vec![spoke_in(None), spoke_in(None)], &[]),
            ["outside", "block", "block"],
        );
    }

    #[test]
    fn a_turn_that_said_nothing_still_says_it_happened() {
        // A `refusal` that produced no updates is a turn that happened, and a
        // screen that drew nothing for it is a screen where the prompt vanished.
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

        // A turn with no stop reason says what happened instead, because a
        // reason the agent never gave is the one thing this may not report.
        let failed = shown(TurnOutcome::Failed(CallError::Disconnected));
        assert!(
            failed.contains("failed") && !failed.contains("end_turn"),
            "{failed}"
        );
    }

    /// The center screen with nothing on it: an agent that may or may not be
    /// there, and a session that is not.
    fn nothing_open(connected: bool) -> String {
        nothing_open_on(connected, false)
    }

    /// The same, saying whether there is an agent that has described itself —
    /// which is the gate the offer to open a session is drawn behind.
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
                    on_prompt: move |_: String| {},
                    on_stop: move |()| {},
                    on_set_mode: move |_| {},
                    on_answer: move |_: Answer| {},
                    on_elicit: move |_: elicitation::Answer| {},
                    sought: None,
                    on_reveal: move |_: Vec<u64>| {},
                    // These are about a row rather than about the turns it sat
                    // in, so there is no record and no line is drawn.
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
        // A screen that names a state and offers nothing has told the reader to
        // go and find the control themselves — a tab away, and in the
        // disconnected case behind a dialog that has to be opened first.
        let openable = nothing_open_on(true, true);
        let offer = empty_action(&openable).expect("a session can be opened from here");
        assert!(
            offer.contains("session/new") && offer.contains("btn-filled"),
            "the offer is the call the Agent screen would make: {offer}"
        );

        // And it is drawn behind the same gate the Agent screen's button is: an
        // agent that has not described itself cannot answer this call, and a
        // screen offering it anyway would be a control that does nothing.
        assert!(
            empty_action(&nothing_open(true)).is_none(),
            "no agent has described itself, so nothing is offered"
        );

        // The other absence has a different answer, and it is a move rather
        // than a call: there is nothing to open a session *on* yet.
        let unlaunched = nothing_open(false);
        let launch = empty_action(&unlaunched).expect("the launch form is offered");
        assert!(
            launch.contains("Connect\u{2026}"),
            "an unlaunched window is sent to the fields: {launch}"
        );
        // In the bar's word rather than in a third one. The offer here and the
        // offer up there open the same dialog, and a reader should be able to
        // see that without pressing one of them.
        assert!(
            !launch.contains("Launch"),
            "and `Launch` is left to the control inside the form that does it: {launch}"
        );

        // And the state that is waiting on the agent offers nothing, because a
        // control there would be a button for *be patient*.
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
                    on_prompt: move |_: String| {},
                    on_stop: move |()| {},
                    on_set_mode: move |_| {},
                    on_answer: move |_: Answer| {},
                    on_elicit: move |_: elicitation::Answer| {},
                    sought: None,
                    on_reveal: move |_: Vec<u64>| {},
                    // These are about a row rather than about the turns it sat
                    // in, so there is no record and no line is drawn.
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
        // Where closing or deleting the live session leaves the screen (§7.5).
        // The agent is still there and nothing is open in it, which is neither
        // a session with nothing in it nor a window waiting to be launched — so
        // it says which of the three it is, and where what was said went.
        let ended = nothing_open(true);

        assert!(
            ended.contains("<h3>No session is open</h3>") && ended.contains("remains in Trace"),
            "it says there is no Session and where its record remains: {ended}"
        );
        assert!(
            !ended.contains("Launch an Agent"),
            "and not to launch an agent, which is right there: {ended}"
        );

        // The other side of the same sentence: before a launch there is nothing
        // to open a session *on*, and that is what to do about it.
        let unlaunched = nothing_open(false);
        assert!(
            unlaunched.contains("<h3>No session updates yet</h3>")
                && unlaunched.contains("Launch an Agent"),
            "an agent that was never started is a different absence: {unlaunched}"
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
        // A region's own header says what the region is showing and what state
        // it is in (ADR 0009). The *sentence* about that state stays above the
        // composer, so nothing is drawn twice.
        let html = screen(vec![vec![Some(0)]], true, None);
        let header = html
            .split_once("<header class=\"pane-head\">")
            .and_then(|(_, rest)| rest.split_once("</header>"))
            .map(|(header, _)| header.to_owned())
            .expect("the Timeline's own header");

        assert!(
            header.contains(r#"<h2 class="pane-title">Session</h2>"#),
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
        // The gutter is what makes a streamed conversation scannable — the eye
        // runs down one column of glyphs rather than reading every kind word —
        // and the glyph is decoration, because the kind is beside it in text on
        // every row.
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
            !html.contains(r#"<span class="eyebrow">agent_message_chunk</span>"#),
            "the agent's message is drawn as the message and nothing else: {html}"
        );
        // The rows the drawing does label keep the word on screen.
        let annotated = shown(cancelled());
        assert!(
            annotated.contains(r#"<span class="eyebrow">conformance</span>"#),
            "a row the drawing names says what it is on screen: {annotated}"
        );
    }

    #[test]
    fn an_unrecognized_entry_is_a_block_of_its_own() {
        // Never gathered into anything: an entry the typed layer could not read
        // is first-class, and folding it into a neighbour would be this window
        // interpreting traffic core just said it could not (§8).
        let blocks = blocks(vec![
            spoke("hello", None),
            entry(EntryKind::Unrecognized(Unrecognized::Unsolicited)),
            spoke("again", None),
        ]);

        assert_eq!(gathered(&blocks), [1, 0, 1]);
    }

    /// The centre screen, over entries made of frames the trace captured under
    /// those ordinals — the screen itself, because what these rings are about
    /// is what a reader can see and reach.
    ///
    /// The props are the ordinals rather than the entries, because a component's
    /// props are compared and `TimelineEntry` is core's and not comparable. Each
    /// inner list is one row's frames; `None` is a frame the trace never
    /// numbered, which is what an annotation's are.
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
                    on_prompt: move |_: String| {},
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

    /// One entry whose evidence is open, in a window switched to indented —
    /// `closed` for the one the reader has since clicked shut.
    fn indented_evidence(frame: &str, closed: bool) -> String {
        #[component]
        fn Host(frame: String, closed: bool) -> Element {
            crate::indent::provide(acp_inspector_core::Indentation::Indented);
            if closed {
                // The same call the summary makes when it is activated, by the
                // name this entry gives its disclosure: the event says only that
                // one was turned over, which is what an entry drawn open has to
                // be read against.
                crate::indent::toggled(&format!("evidence {}", named(&EntryId::Arrival(1))));
            }
            let entries = use_signal(move || {
                vec![TimelineEntry {
                    frames: vec![Recorded {
                        frame: acp_inspector_core::Frame::new(frame.clone()),
                        captured: Some(0),
                    }],
                    // An unrecognized entry, because its evidence is open
                    // without being asked (§8) — which is the entry a reader
                    // most wants laid out, since the raw *is* the rendering.
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
                    on_prompt: move |_: String| {},
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
        // The same call the Trace makes about the same frame (`crate::indent`),
        // so one frame cannot be drawn two ways by the two screens §9 asks to
        // point at each other. This entry is one whose evidence *is* its
        // rendering (§8), so it is open from the first render and laid out
        // without anybody clicking anything.
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

        // And closed again by the reader, it is the bytes: the layout is spent
        // where somebody is looking, which is the same rule on both screens.
        assert_eq!(
            evidence_block(&indented_evidence(frame, true)),
            "{&#34;jsonrpc&#34;:&#34;2.0&#34;,&#34;method&#34;:&#34;fs/read_text_file&#34;}"
        );
    }

    #[test]
    fn evidence_that_is_not_json_is_drawn_as_it_arrived_either_way() {
        // A frame can be anything (§8), and an entry made of one that is not
        // JSON is the entry a reader came for.
        for closed in [true, false] {
            assert_eq!(
                evidence_block(&indented_evidence("not json at all", closed)),
                "not json at all"
            );
        }
    }

    #[test]
    fn a_row_says_which_frames_it_was_made_of_and_offers_the_way_to_them() {
        // The half of the two screens pointing at each other that lives here:
        // an entry carries the ordinals the trace captured its frames under
        // (`acp_inspector_core::Recorded`), so the Console can be told which rows to
        // mark — and the control that tells it sits on the disclosure that
        // names those frames rather than on the row, because the row is about
        // what the agent said and this is about what it sent.
        let html = screen(vec![vec![Some(4), Some(5)]], false, None);

        assert!(
            html.contains(r#"data-frames="4 5""#) && html.contains(r#"tabindex="-1""#),
            "the row names its frames and accepts destination focus: {html}"
        );
        // **The row is the control**, which is how the drawing links the two
        // screens: pressing a message shows the traffic it came from, and the
        // row says so by being drawn as something that can be pressed.
        assert!(
            html.contains(r#"class="row linked agent""#),
            "the row leads to its frames: {html}"
        );
        // And the same act is named for a reader who is not pointing at
        // anything, which is what a click on a row cannot be.
        assert!(
            html.contains(r#"data-slot="reach""#) && html.contains("frames in the wire log"),
            "and offers the way to them by name: {html}"
        );
        assert!(
            !html.contains(r#"data-slot="wire""#),
            "a row this window has a rendering for keeps its frames in the wire log: {html}"
        );
    }

    #[test]
    fn an_entry_made_of_frames_the_trace_never_numbered_offers_no_way_to_them() {
        // A Conformance Annotation is read beside a call this client *sent*,
        // and the send path carries no ordinal back up. A control that led
        // nowhere would be worse than none: a reader who followed it and found
        // nothing marked would read that as a fact about the trace.
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
        // And it is the row that carries frame 9. The list is laid out
        // bottom-up, so the order in the document is not the order on screen —
        // which is exactly why this asks the row and not the position.
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
    fn timeline_navigation_scrolls_and_focuses_its_destination() {
        assert!(FOCUS_ENTRY.contains("scrollIntoView"), "{FOCUS_ENTRY}");
        assert!(
            FOCUS_ENTRY.contains("target.focus({ preventScroll: true })"),
            "{FOCUS_ENTRY}"
        );
    }

    #[test]
    fn a_turn_waiting_on_the_reader_says_so_where_it_can_be_heard() {
        // The entry list is deliberately not scrolled for the reader
        // (`.blocks`), so a request arriving while they are reading further
        // back arrives off screen. The turn stopping is announced rather than
        // only drawn, because the tool has stopped and is waiting to be told
        // what to do.
        //
        // The control beside the sentence needs a real blocking request to
        // reach, and only core can mint one, so what a screen test can hold is
        // the announcement.
        //
        // **One sentence for either kind** (§7.8). A permission request and an
        // elicitation both stop the tool, and naming which of them did would
        // split the reader's next action along a distinction that does not
        // change it.
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
        // The region is always in the document — an announcement is made by
        // text *changing* inside one, so a region that arrived along with its
        // sentence would announce nothing — and empty it draws as nothing.
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
}
