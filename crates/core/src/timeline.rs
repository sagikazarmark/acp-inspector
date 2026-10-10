//! The timeline store: what the agent said during the session, decoded as v1
//! where the typed layer recognizes it and kept raw where it does not. See
//! `docs/architecture.md` §9 and §11.
//!
//! - Entities are id-keyed and merge-updated: a tool call that streams four
//!   updates is one entry that changed four times.
//! - Unknown traffic is displayed, not dropped: a frame the typed layer could
//!   not decode becomes an [`Unrecognized`] entry (§8).
//!
//! Every entry carries the frames it was made of, which the trace also holds.
//! A conformance [`Annotation`] is the one entry in the inspector's own voice,
//! carrying the frames that decide it.
//!
//! Unlike a [`Log`](crate::store::Log), entries change in place, so the store
//! signals a revision rather than a length.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::SystemTime;

use agent_client_protocol_schema::{MaybeUndefined, v1};
use tokio::sync::watch;

use crate::blocking::RequestId;
use crate::conformance::Annotation;
use crate::elicitation::ElicitationRequest;
use crate::frame::Frame;
use crate::permission::PermissionRequest;
use crate::service::ServiceRequest;
use crate::store::{Changes, locked};
use crate::turn::TurnOutcome;

/// The session's timeline. A handle: clones share one timeline.
#[derive(Clone)]
pub struct Timeline {
    entries: Arc<Mutex<Entries>>,
    /// Bumped on every change, including an entry merged in place.
    revision: Arc<watch::Sender<u64>>,
}

/// One thing that happened in the session, as the timeline holds it.
#[derive(Clone, Debug)]
pub struct TimelineEntry {
    /// What merges into what.
    pub id: EntryId,
    /// When the last frame behind it arrived.
    pub at: SystemTime,
    /// The frames this entry was made of, verbatim and in order — one for most
    /// entries, one per update merged into a tool call — each with where the
    /// trace recorded it (§8).
    pub frames: Vec<Recorded>,
    pub kind: EntryKind,
    /// The turn this arrived in, or `None` where it arrived outside one (e.g.
    /// after a turn ended, before any prompt, or a `session/load` replay).
    /// Stamped when the entry is made and never restamped.
    pub turn: Option<u64>,
}

/// One turn the session had, and what became of it (`CONTEXT.md`, *Turn*).
/// Entries name their turn, so a turn with no entries still exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    /// Which turn of this session it is, counting from one.
    pub id: u64,
    /// When the prompt that opened it went out.
    pub at: SystemTime,
    /// What it came to, or `None` while it is still the live turn (see
    /// [`Inspector::turn`](crate::Inspector::turn)).
    pub outcome: Option<TurnOutcome>,
}

/// One frame an entry was made of, and where the Trace recorded it.
#[derive(Clone, Debug)]
pub struct Recorded {
    pub frame: Frame,
    /// The ordinal the Trace captured it under — `dropped + position` read off
    /// a [snapshot](crate::Trace::snapshot), stable as the ring rotates.
    ///
    /// `None` for a frame this client sent (as in an annotation): the send
    /// path does not carry the ordinal back to the typed layer.
    pub captured: Option<u64>,
}

/// An entry's identity: the agent's own id where the protocol gives one,
/// arrival order where it does not.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EntryId {
    /// A tool call, keyed by session and the agent's id (the id is only unique
    /// within a session).
    ToolCall(v1::SessionId, v1::ToolCallId),
    /// A permission request, keyed by the JSON-RPC id it arrived under.
    Permission(RequestId),
    /// An elicitation, keyed by the JSON-RPC id it arrived under. Separate from
    /// `Permission` so the two cannot collide across a reconnect.
    Elicitation(RequestId),
    /// A client-service request, keyed by the JSON-RPC id it arrived under.
    Service(RequestId),
    /// A context compaction, keyed by session and compaction id: its updates
    /// upsert and its summary chunks append into one entry.
    Compaction(v1::SessionId, v1::CompactionId),
    /// Everything the protocol gives no id: one entry, keyed by when it turned
    /// up, merging with nothing.
    Arrival(u64),
}

/// What an entry is.
#[derive(Clone, Debug)]
#[non_exhaustive]
// The decoded variant is both the largest and the most common, so boxing it
// would add an allocation per update.
#[allow(clippy::large_enum_variant)]
pub enum EntryKind {
    /// A `session/update` the typed layer decoded as v1 — the decoration on a
    /// frame the trace already has (§8).
    Update(v1::SessionUpdate),
    /// A permission request, where the turn stopped for it (§7.2). Carries its
    /// resolver, so the screen rendering it can answer it.
    Permission(PermissionRequest),
    /// An elicitation, where the agent asked (§7.8). Carries its resolver; may
    /// belong to no session when scoped to a JSON-RPC call.
    Elicitation(ElicitationRequest),
    /// A client-service request (`docs/client-services.md`): a claimed `fs/*`
    /// or `terminal/*` call, or another request this client does not service,
    /// waiting for the reader's answer or already answered by a rules file.
    Service(ServiceRequest),
    /// A frame it did not, kept as itself.
    Unrecognized(Unrecognized),
    /// A spec MUST the captured frames show was not met (§15 q7), carrying
    /// those frames.
    Annotation(Annotation),
}

/// A frame the typed layer left alone, and why (§8). Shown on the timeline
/// beside decoded entries, not in an error list.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Unrecognized {
    /// A method the inspector does not service: an extension method, an
    /// unstable-v1 one, or an unclaimed client service. Requests were answered
    /// method-not-found.
    NotServiced { method: String },
    /// A method it does service, carrying something v1 could not read — a
    /// `sessionUpdate` variant with no v1 name, a field of the wrong shape, v2
    /// traffic arriving early — or a frame that was not JSON-RPC at all, which
    /// announced no method to go by.
    Undecodable {
        method: Option<String>,
        /// What the decoder said, shown next to the frame.
        problem: String,
    },
    /// An answer to a request this client never sent, or an
    /// `elicitation/complete` for an id nobody here holds (which the spec
    /// says to ignore).
    Unsolicited,
    /// A serviced method asking for a shape this client never advertised —
    /// today, an elicitation mode outside the two it claims (§7.8). Answered
    /// invalid-params rather than declined on the reader's behalf.
    Unadvertised {
        method: String,
        /// What was asked for, in the agent's own spelling.
        detail: String,
    },
    /// A `session/update` for a session that is neither live nor opening. It
    /// changed nothing here: the live session's settings are its own.
    ForeignSession { session: v1::SessionId },
    /// A placeholder for a frame the transport dropped for its length; the
    /// entry's frame is the start of it.
    Dropped { length: usize, limit: usize },
    /// An answer whose id is a call of this client's with its type changed
    /// (`"3"` or `3.0` for `3`). JSON-RPC does not match it, so the call is
    /// still waiting.
    MistypedId { id: String, call: i64 },
    /// An error answering no request: its id was null or missing.
    Unaddressed { error: v1::Error },
    /// A blocking request refused because too many were already waiting on
    /// the reader; the agent was answered with an error.
    OverCapacity { method: String, limit: usize },
}

#[derive(Default)]
struct Entries {
    entries: Vec<TimelineEntry>,
    /// Where each id-carrying entry is, for the merge. Cleared on a new
    /// connection ([`Timeline::reopen`]): ids are one agent's.
    index: HashMap<EntryId, usize>,
    /// The next arrival key. Never reset, so ids stay unique for the window's
    /// lifetime.
    arrivals: u64,
    /// Every turn this session has had, in order.
    turns: Vec<Turn>,
    /// Which of them is open, if one is; new entries are stamped with it.
    open: Option<u64>,
    next_turn: u64,
    dropped: Retention,
    raw_bytes: usize,
    raw_frames: usize,
}

/// Explicit holes in this Session's retained Timeline. Entries leave whole,
/// including their deciding evidence; pending Blocking Requests are exempt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Retention {
    pub entries: usize,
    pub frames: usize,
    pub bytes: usize,
    pub turns: usize,
}

impl Timeline {
    pub const CAPACITY: usize = 1_000;
    pub const BYTE_CAPACITY: usize = 16 * 1024 * 1024;
    pub const FRAME_CAPACITY: usize = 2_000;

    /// Entries, Turn records and hole counts from one instant.
    pub fn snapshot(&self) -> (Vec<TimelineEntry>, Vec<Turn>, Retention) {
        let mut held = self.lock();
        held.retain();
        (held.entries.clone(), held.turns.clone(), held.dropped)
    }
    /// Every retained entry, oldest first, by value. Retention holes are in
    /// `snapshot`.
    pub fn entries(&self) -> Vec<TimelineEntry> {
        let mut held = self.lock();
        held.retain();
        held.entries.clone()
    }

    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lock().entries.is_empty()
    }

    /// Watches the timeline: a number that grows every time anything about it
    /// changed, including an entry changing in place.
    pub fn changes(&self) -> Changes<u64> {
        Changes::new(self.revision.subscribe())
    }

    /// Every turn this session has had, oldest first. Announced by the same
    /// revision as the entries, so an entry and the turn it names arrive
    /// together.
    pub fn turns(&self) -> Vec<Turn> {
        self.lock().turns.clone()
    }

    /// Opens a turn where the prompt goes out, and returns its id. A turn
    /// already open is kept and its id returned.
    pub(crate) fn open_turn(&self) -> u64 {
        let mut entries = self.lock();
        if let Some(open) = entries.open {
            return open;
        }

        entries.next_turn += 1;
        let id = entries.next_turn;
        entries.turns.push(Turn {
            id,
            at: SystemTime::now(),
            outcome: None,
        });
        entries.open = Some(id);
        self.moved(&mut entries);
        id
    }

    /// Closes the open turn with what it came to. Nothing where no turn is
    /// open: the first conclusion (the agent's) stands.
    pub(crate) fn close_turn(&self, outcome: TurnOutcome) {
        let mut entries = self.lock();
        let Some(open) = entries.open.take() else {
            return;
        };
        if let Some(turn) = entries.turns.iter_mut().find(|turn| turn.id == open) {
            turn.outcome = Some(outcome);
        }
        self.moved(&mut entries);
    }

    /// Puts one decoded notification where it belongs: onto the entry it
    /// updates, or at the end. The frame it was decoded from goes with it.
    pub(crate) fn update(&self, notification: v1::SessionNotification, frame: Recorded) {
        let id = merge_key(&notification);
        let mut entries = self.lock();
        entries.charge(&frame);
        let existing = id.as_ref().and_then(|id| entries.index.get(id)).copied();

        match existing {
            Some(position) => {
                let entry = &mut entries.entries[position];
                entry.at = SystemTime::now();
                entry.frames.push(frame);
                merge(&mut entry.kind, notification.update);
            }
            None => {
                let id = id.unwrap_or_else(|| entries.arrival());
                let position = entries.entries.len();
                let turn = entries.open;
                entries.entries.push(TimelineEntry::new(
                    id.clone(),
                    frame,
                    EntryKind::Update(created(notification.update)),
                    turn,
                ));
                if !matches!(id, EntryId::Arrival(_)) {
                    entries.index.insert(id, position);
                }
            }
        }

        self.moved(&mut entries);
    }

    /// Puts a permission request where the turn stopped for it (§7.2). Keyed
    /// by its JSON-RPC id but not merged: two asks under one id are two entries.
    pub(crate) fn blocked(&self, request: PermissionRequest, frame: Recorded) {
        let mut entries = self.lock();
        entries.charge(&frame);
        let turn = entries.open;
        entries.entries.push(TimelineEntry::new(
            EntryId::Permission(request.id().clone()),
            frame,
            EntryKind::Permission(request),
            turn,
        ));
        self.moved(&mut entries);
    }

    /// Puts an elicitation where the agent asked it (§7.8), on the same terms
    /// as [`blocked`](Self::blocked).
    pub(crate) fn elicited(&self, request: ElicitationRequest, frame: Recorded) {
        let mut entries = self.lock();
        entries.charge(&frame);
        let turn = entries.open;
        entries.entries.push(TimelineEntry::new(
            EntryId::Elicitation(request.id().clone()),
            frame,
            EntryKind::Elicitation(request),
            turn,
        ));
        self.moved(&mut entries);
    }

    /// Puts a client-service request where the agent asked it, on the same
    /// terms as [`blocked`](Self::blocked).
    pub(crate) fn serviced(&self, request: ServiceRequest, frame: Recorded) {
        let mut entries = self.lock();
        entries.charge(&frame);
        let turn = entries.open;
        entries.entries.push(TimelineEntry::new(
            EntryId::Service(request.id().clone()),
            frame,
            EntryKind::Service(request),
            turn,
        ));
        self.moved(&mut entries);
    }

    /// Adds an `elicitation/complete` frame to the entries it names (§7.8),
    /// the way a tool call's updates join its entry (`docs/architecture.md`
    /// §11 seam 3).
    pub(crate) fn completed(&self, requests: &[RequestId], frame: Recorded) {
        let mut entries = self.lock();
        let mut added = 0;
        for entry in &mut entries.entries {
            if let EntryId::Elicitation(id) = &entry.id
                && requests.contains(id)
            {
                entry.frames.push(frame.clone());
                added += 1;
            }
        }
        entries.raw_frames += added;
        entries.raw_bytes += added * frame.frame.as_str().len();
        if added == 0 {
            // The named request may have aged out since its answer. Completion
            // is still traffic: retain it raw rather than swallowing it into a
            // row that no longer exists.
            entries.charge(&frame);
            let id = entries.arrival();
            let turn = entries.open;
            entries.entries.push(TimelineEntry::new(
                id,
                frame,
                EntryKind::Unrecognized(Unrecognized::Undecodable {
                    method: Some("elicitation/complete".into()),
                    problem: "The elicitation's Timeline entry is no longer retained.".into(),
                }),
                turn,
            ));
        }
        self.moved(&mut entries);
    }

    /// Appends a conformance annotation beside the frames that decide it
    /// (§15 q7). Never merges.
    pub(crate) fn annotate(&self, annotation: Annotation, frames: Vec<Frame>) {
        // Mostly frames this client sent, whose ordinal is not carried back
        // (`Recorded::captured`).
        let frames: Vec<_> = frames
            .into_iter()
            .map(|frame| Recorded {
                frame,
                captured: None,
            })
            .collect();
        let mut entries = self.lock();
        for frame in &frames {
            entries.charge(frame);
        }
        let id = entries.arrival();
        let turn = entries.open;
        entries.entries.push(TimelineEntry {
            id,
            at: SystemTime::now(),
            frames,
            kind: EntryKind::Annotation(annotation),
            turn,
        });
        self.moved(&mut entries);
    }

    /// Adds a frame to the entry `id` names, where it is still held. Says
    /// whether it was.
    pub(crate) fn attach(&self, id: &EntryId, frame: Recorded) -> bool {
        let mut entries = self.lock();
        let Some(entry) = entries.entries.iter_mut().find(|entry| &entry.id == id) else {
            return false;
        };
        entry.frames.push(frame.clone());
        entries.charge(&frame);
        self.moved(&mut entries);
        true
    }

    /// Appends a frame the typed layer could not use. Never merges: there is
    /// no id to merge on.
    pub(crate) fn unrecognized(&self, unrecognized: Unrecognized, frame: Recorded) {
        let mut entries = self.lock();
        entries.charge(&frame);
        let id = entries.arrival();
        let turn = entries.open;
        entries.entries.push(TimelineEntry::new(
            id,
            frame,
            EntryKind::Unrecognized(unrecognized),
            turn,
        ));
        self.moved(&mut entries);
    }

    /// Starts a new connection's worth of timeline.
    ///
    /// The entries stay; only the merge index (the last agent's ids) and the
    /// open turn go. Contrast [`discard`](Self::discard), which a session
    /// switch uses.
    pub(crate) fn reopen(&self) {
        let mut entries = self.lock();
        entries.index.clear();
        // The open turn was the last agent's. Entries stay: a mistyped command
        // reaches here too, and only a replacing session empties the view.
        entries.open = None;
        self.moved(&mut entries);
    }

    /// Empties the view so the next session can be built into it (§7.5).
    ///
    /// One live session, one view: no merge and no second timeline. The trace
    /// is untouched. The arrival counter keeps counting so ids stay unique.
    pub(crate) fn discard(&self) {
        let mut entries = self.lock();
        entries.entries.clear();
        entries.index.clear();
        entries.turns.clear();
        entries.open = None;
        entries.next_turn = 0;
        entries.dropped = Retention::default();
        entries.raw_bytes = 0;
        entries.raw_frames = 0;
        self.moved(&mut entries);
    }

    /// A way to announce a change this store did not make — a pending request
    /// answering itself. Hands out only the announcement, never the entries,
    /// to avoid a cycle between the list and its items.
    pub(crate) fn ticker(&self) -> Ticker {
        Ticker(Arc::clone(&self.revision))
    }

    /// Announces a change while the entries are still held, so a subscriber
    /// that wakes on a revision and then reads can never find less than it was
    /// told about.
    fn moved(&self, held: &mut MutexGuard<'_, Entries>) {
        held.retain();
        self.revision.send_modify(|revision| *revision += 1);
    }

    fn lock(&self) -> MutexGuard<'_, Entries> {
        locked(&self.entries)
    }
}

/// Announces a timeline change without access to the entries.
#[derive(Clone)]
pub(crate) struct Ticker(Arc<watch::Sender<u64>>);

impl Ticker {
    pub(crate) fn tick(&self) {
        self.0.send_modify(|revision| *revision += 1);
    }
}

impl Default for Timeline {
    fn default() -> Self {
        Self {
            entries: Arc::default(),
            revision: Arc::new(watch::Sender::new(0)),
        }
    }
}

impl Entries {
    fn charge(&mut self, frame: &Recorded) {
        self.raw_bytes += frame.frame.as_str().len();
        self.raw_frames += 1;
    }

    fn retain(&mut self) {
        if self.entries.len() <= Timeline::CAPACITY
            && self.raw_frames <= Timeline::FRAME_CAPACITY
            && self.raw_bytes <= Timeline::BYTE_CAPACITY
            && self.turns.len() <= Timeline::CAPACITY
        {
            return;
        }
        let latest_commands = self.entries.iter().rposition(|entry| {
            matches!(
                entry.kind,
                EntryKind::Update(v1::SessionUpdate::AvailableCommandsUpdate(_))
            )
        });
        let mut count = self.entries.len();
        let mut removed = Vec::new();
        for (position, entry) in self.entries.iter().enumerate() {
            if count <= Timeline::CAPACITY
                && self.raw_frames <= Timeline::FRAME_CAPACITY
                && self.raw_bytes <= Timeline::BYTE_CAPACITY
            {
                break;
            }
            if entry.retention_pinned() || latest_commands == Some(position) {
                continue;
            }
            count -= 1;
            let bytes = entry
                .frames
                .iter()
                .map(|f| f.frame.as_str().len())
                .sum::<usize>();
            self.raw_frames -= entry.frames.len();
            self.raw_bytes -= bytes;
            self.dropped.entries += 1;
            self.dropped.frames += entry.frames.len();
            self.dropped.bytes += bytes;
            removed.push(position);
        }
        if !removed.is_empty() {
            // Preserve the Connection boundary in the merge index: rebuilding
            // it from every retained entry would resurrect the previous Agent's IDs.
            self.index
                .retain(|_, position| match removed.binary_search(position) {
                    Ok(_) => false,
                    Err(before) => {
                        *position -= before;
                        true
                    }
                });
            let mut position = 0;
            self.entries.retain(|_| {
                let keep = removed.binary_search(&position).is_err();
                position += 1;
                keep
            });
        }
        if self.turns.len() > Timeline::CAPACITY {
            let mut excess = self.turns.len() - Timeline::CAPACITY;
            // One pass over the entries, not one per turn.
            let used: std::collections::HashSet<u64> =
                self.entries.iter().filter_map(|entry| entry.turn).collect();
            self.turns.retain(|turn| {
                if excess == 0 || self.open == Some(turn.id) || used.contains(&turn.id) {
                    return true;
                }
                excess -= 1;
                self.dropped.turns += 1;
                false
            });
        }
    }

    fn arrival(&mut self) -> EntryId {
        self.arrivals += 1;
        EntryId::Arrival(self.arrivals)
    }
}

impl TimelineEntry {
    fn retention_pinned(&self) -> bool {
        match &self.kind {
            EntryKind::Permission(request) => matches!(
                request.state(),
                crate::permission::PermissionState::Waiting
                    | crate::permission::PermissionState::Answering
            ),
            EntryKind::Elicitation(request) => matches!(
                request.state(),
                crate::elicitation::ElicitationState::Waiting
                    | crate::elicitation::ElicitationState::Answering
            ),
            EntryKind::Service(request) => matches!(
                request.state(),
                crate::service::ServiceState::Waiting | crate::service::ServiceState::Answering
            ),
            _ => false,
        }
    }
    pub fn is_waiting(&self) -> bool {
        match &self.kind {
            EntryKind::Permission(request) => request.is_waiting(),
            EntryKind::Elicitation(request) => request.is_waiting(),
            EntryKind::Service(request) => request.is_waiting(),
            _ => false,
        }
    }
    /// An entry as it starts: one frame, and what the typed layer made of it.
    fn new(id: EntryId, frame: Recorded, kind: EntryKind, turn: Option<u64>) -> Self {
        Self {
            id,
            at: SystemTime::now(),
            frames: vec![frame],
            kind,
            turn,
        }
    }
}

impl Unrecognized {
    /// The method it announced, when it announced one.
    pub fn method(&self) -> Option<&str> {
        match self {
            Self::NotServiced { method } => Some(method),
            Self::Undecodable { method, .. } => method.as_deref(),
            Self::Unadvertised { method, .. } | Self::OverCapacity { method, .. } => Some(method),
            Self::ForeignSession { .. } => Some("session/update"),
            Self::Unsolicited
            | Self::Dropped { .. }
            | Self::MistypedId { .. }
            | Self::Unaddressed { .. } => None,
        }
    }
}

/// The id an update merges on, if the protocol gave it one.
///
/// Only tool calls and compactions arrive as a create followed by patches;
/// keying anything else would collapse history into one line.
fn merge_key(notification: &v1::SessionNotification) -> Option<EntryId> {
    let session = notification.session_id.clone();
    match &notification.update {
        v1::SessionUpdate::ToolCall(call) => {
            Some(EntryId::ToolCall(session, call.tool_call_id.clone()))
        }
        v1::SessionUpdate::ToolCallUpdate(update) => {
            Some(EntryId::ToolCall(session, update.tool_call_id.clone()))
        }
        v1::SessionUpdate::CompactionUpdate(update) => {
            Some(EntryId::Compaction(session, update.compaction_id.clone()))
        }
        v1::SessionUpdate::CompactionSummaryChunk(chunk) => {
            Some(EntryId::Compaction(session, chunk.compaction_id.clone()))
        }
        _ => None,
    }
}

/// What an update is as the first entry for its id.
///
/// A summary chunk for a compaction nobody announced creates an in-progress
/// compaction holding that one block, which is what the schema says a
/// first-seen id means. Everything else is itself.
fn created(update: v1::SessionUpdate) -> v1::SessionUpdate {
    match update {
        v1::SessionUpdate::CompactionSummaryChunk(chunk) => v1::SessionUpdate::CompactionUpdate(
            v1::CompactionUpdate::new(chunk.compaction_id, v1::CompactionStatus::InProgress)
                .summary(vec![chunk.content]),
        ),
        update => update,
    }
}

/// One patch field of a compaction update: absent leaves it, `null` clears
/// it, a value replaces it (an empty summary is a value that clears).
fn patched<T>(field: &mut MaybeUndefined<T>, update: MaybeUndefined<T>) {
    if !matches!(update, MaybeUndefined::Undefined) {
        *field = update;
    }
}

/// Folds an update into the entry it belongs to: a patch, not a replacement,
/// since an update carries only the fields that changed.
fn merge(kind: &mut EntryKind, update: v1::SessionUpdate) {
    let EntryKind::Update(existing) = kind else {
        // Unrecognized entries are never in the index.
        return;
    };

    match (existing, update) {
        (v1::SessionUpdate::ToolCall(call), v1::SessionUpdate::ToolCallUpdate(update)) => {
            patch(call, update.fields);
        }
        (
            v1::SessionUpdate::ToolCallUpdate(existing),
            v1::SessionUpdate::ToolCallUpdate(update),
        ) => {
            // Updates before the create: keep patching, and let the create take
            // over if it ever arrives.
            patch_fields(&mut existing.fields, update.fields);
        }
        // A compaction is an upsert with patch semantics: omitted leaves a
        // field alone, `null` clears it, a value replaces it.
        (
            v1::SessionUpdate::CompactionUpdate(existing),
            v1::SessionUpdate::CompactionUpdate(update),
        ) => {
            existing.status = update.status;
            patched(&mut existing.summary, update.summary);
            patched(&mut existing.error, update.error);
            patched(&mut existing.meta, update.meta);
        }
        // And its summary chunks append, in the order they arrived.
        (
            v1::SessionUpdate::CompactionUpdate(existing),
            v1::SessionUpdate::CompactionSummaryChunk(chunk),
        ) => match &mut existing.summary {
            MaybeUndefined::Value(summary) => summary.push(chunk.content),
            summary => *summary = MaybeUndefined::Value(vec![chunk.content]),
        },
        // A create for an id that already has an entry restates the whole
        // call, so it replaces.
        (existing, update) => *existing = update,
    }
}

fn patch(call: &mut v1::ToolCall, fields: v1::ToolCallUpdateFields) {
    let v1::ToolCallUpdateFields {
        kind,
        status,
        title,
        name,
        content,
        locations,
        raw_input,
        raw_output,
        ..
    } = fields;

    if let Some(kind) = kind {
        call.kind = kind;
    }
    if let Some(status) = status {
        call.status = status;
    }
    if let Some(title) = title {
        call.title = title;
    }
    // Omitted and `null` both leave the name as it was (schema 1.8).
    if let Some(name) = name {
        call.name = Some(name);
    }
    // Collections are overwritten rather than extended, which is what the
    // protocol says an update means by them.
    if let Some(content) = content {
        call.content = content;
    }
    if let Some(locations) = locations {
        call.locations = locations;
    }
    if let Some(raw_input) = raw_input {
        call.raw_input = Some(raw_input);
    }
    if let Some(raw_output) = raw_output {
        call.raw_output = Some(raw_output);
    }
}

fn patch_fields(fields: &mut v1::ToolCallUpdateFields, update: v1::ToolCallUpdateFields) {
    let v1::ToolCallUpdateFields {
        kind,
        status,
        title,
        name,
        content,
        locations,
        raw_input,
        raw_output,
        ..
    } = update;

    fields.kind = kind.or(fields.kind);
    fields.status = status.or(fields.status);
    fields.title = title.or_else(|| fields.title.take());
    fields.name = name.or_else(|| fields.name.take());
    fields.content = content.or_else(|| fields.content.take());
    fields.locations = locations.or_else(|| fields.locations.take());
    fields.raw_input = raw_input.or_else(|| fields.raw_input.take());
    fields.raw_output = raw_output.or_else(|| fields.raw_output.take());
}

#[cfg(test)]
mod retention_tests {
    use super::*;

    fn recorded(text: &str, ordinal: u64) -> Recorded {
        Recorded {
            frame: Frame::new(text),
            captured: Some(ordinal),
        }
    }

    #[test]
    fn merged_tool_evidence_leaves_whole_and_later_patch_starts_a_new_entry() {
        let timeline = Timeline::default();
        for i in 0..=Timeline::FRAME_CAPACITY {
            let notification = crate::decode::from_value(serde_json::json!({"sessionId":"s","update":{"sessionUpdate":"tool_call_update","toolCallId":"t","title":"updated"}})).unwrap();
            timeline.update(notification, recorded("raw patch", i as u64));
        }
        let (entries, _, holes) = timeline.snapshot();
        assert!(entries.is_empty());
        assert_eq!(holes.entries, 1);
        assert_eq!(holes.frames, Timeline::FRAME_CAPACITY + 1);
        let notification = crate::decode::from_value(serde_json::json!({"sessionId":"s","update":{"sessionUpdate":"tool_call_update","toolCallId":"t","status":"completed"}})).unwrap();
        timeline.update(notification, recorded("last patch", 3000));
        let entries = timeline.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].frames.len(), 1);
        assert_eq!(entries[0].frames[0].captured, Some(3000));
    }

    #[test]
    fn turn_ordinals_do_not_reset_when_empty_turn_records_age_out() {
        let timeline = Timeline::default();
        for id in 1..=1500 {
            assert_eq!(timeline.open_turn(), id);
            timeline.close_turn(TurnOutcome::Ended(v1::StopReason::EndTurn));
        }
        let (_, turns, holes) = timeline.snapshot();
        assert_eq!(turns.len(), Timeline::CAPACITY);
        assert_eq!(turns[0].id, 501);
        assert_eq!(holes.turns, 500);
        assert_eq!(timeline.open_turn(), 1501);
    }

    #[test]
    fn completion_after_entry_retention_is_still_raw_evidence() {
        let timeline = Timeline::default();
        timeline.completed(&[], recorded("completion", 42));
        let entries = timeline.entries();
        assert_eq!(entries[0].frames[0].captured, Some(42));
        assert_eq!(entries[0].frames[0].frame.as_str(), "completion");
    }

    #[test]
    fn annotation_and_deciding_frames_age_out_together_and_commands_remain_advertised() {
        let timeline = Timeline::default();
        let commands = crate::decode::from_value(serde_json::json!({"sessionId":"s","update":{"sessionUpdate":"available_commands_update","availableCommands":[{"name":"help","description":"Help"}]}})).unwrap();
        timeline.update(commands, recorded("commands", 0));
        timeline.annotate(
            Annotation::Replay(crate::conformance::Replay::Nothing),
            vec![Frame::new("request"), Frame::new("response")],
        );
        for ordinal in 1..Timeline::CAPACITY as u64 {
            timeline.unrecognized(Unrecognized::Unsolicited, recorded("traffic", ordinal));
        }
        let (entries, _, holes) = timeline.snapshot();
        assert_eq!(entries.len(), Timeline::CAPACITY);
        assert!(matches!(
            entries[0].kind,
            EntryKind::Update(v1::SessionUpdate::AvailableCommandsUpdate(_))
        ));
        assert!(
            entries
                .iter()
                .all(|e| !matches!(e.kind, EntryKind::Annotation(_)))
        );
        assert_eq!((holes.entries, holes.frames, holes.bytes), (1, 2, 15));
        // A new annotation can retain both deciding Frames even if Trace has
        // already lost them: no dangling evidence reference is introduced.
        timeline.annotate(
            Annotation::Replay(crate::conformance::Replay::Nothing),
            vec![Frame::new("request"), Frame::new("response")],
        );
        let held = timeline.entries();
        let annotation = held.last().unwrap();
        assert_eq!(annotation.frames.len(), 2);
        assert_eq!(annotation.frames[0].frame.as_str(), "request");
    }
}
