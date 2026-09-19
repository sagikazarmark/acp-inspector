//! The center screen's store (`docs/architecture.md` §9): what the agent said
//! during the session, decoded as v1 where the typed layer recognizes it and
//! kept as itself where it does not.
//!
//! Two rules make this store what it is, and both are v2 seams built now rather
//! than retrofitted later (§11):
//!
//! - **Entities are id-keyed and merge-updated**, never keyed by arrival order
//!   alone. A tool call that streams four updates is one entry that changed
//!   four times, not four entries — and when v2 makes every id-carrying update
//!   an upsert patch, that is this store doing what it already does.
//! - **Unknown traffic is displayed, not dropped** (§8). A frame the typed
//!   layer could not decode is an [`Unrecognized`] entry rather than an error,
//!   which is what lets `_`-prefixed methods, `_meta` payloads, unstable-v1
//!   methods and early v2 traffic be *seen*.
//!
//! The timeline decorates; it never replaces. Every entry carries the frames it
//! was decoded from, and every one of those is in the trace as well, recorded
//! before this layer was allowed to have an opinion (§6.2).
//!
//! One entry is not the agent's voice but the inspector's: a conformance
//! [`Annotation`] (§15 q7) is here because it belongs beside the traffic it
//! describes, and it carries that traffic like every other entry carries its
//! frames — the claim and its evidence, read together.
//!
//! It is not a [`Log`](crate::store::Log), close as it looks: a log only ever
//! appends and signals its length, and this one changes entries in place and
//! signals a revision, because a subscriber watching a count would never learn
//! that a tool call finished. Bending one abstraction over both shapes would
//! cost more than the accessors they have in common.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::SystemTime;

use agent_client_protocol_schema::v1;
use tokio::sync::watch;

use crate::blocking::RequestId;
use crate::conformance::Annotation;
use crate::elicitation::ElicitationRequest;
use crate::frame::Frame;
use crate::permission::PermissionRequest;
use crate::store::{Changes, locked};
use crate::turn::TurnOutcome;

/// The session's timeline.
///
/// A handle, not the entries: cloning shares one timeline, so the window, a
/// test and whatever comes next are looking at the same session.
#[derive(Clone)]
pub struct Timeline {
    entries: Arc<Mutex<Entries>>,
    /// How many times the timeline changed — *not* how many entries it has,
    /// because a merge changes an entry without adding one, and a subscriber
    /// watching the length would never learn that a tool call finished.
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
    /// entries, and one per update that merged into a tool call.
    ///
    /// **The decoration keeps what it decorates** (§8). An entry that could
    /// only be read through `kind` would be an interpretation standing in for
    /// what the agent said: the fields v1 has no name for, the `_meta` a real
    /// agent attaches, the shape of the JSON itself. They are here, on the
    /// entry, as well as in the trace.
    ///
    /// And each says where the trace put it, so *as well as* can be read as one
    /// thing rather than two: an entry knows the rows it was made of, and the
    /// rows can be found again after the ring has turned under them.
    pub frames: Vec<Recorded>,
    pub kind: EntryKind,
    /// The turn this arrived in, or `None` where it arrived outside one.
    ///
    /// **`None` is a fact about the traffic and not a gap in the record.** An
    /// agent that goes on talking after it ended its turn, one that says
    /// something before anything was ever prompted, and a session replayed by
    /// `session/load` are all traffic that belongs to no turn — which is
    /// something a reader of an inspector wants to see rather than have tidied
    /// into whichever turn it happened to fall between.
    ///
    /// Stamped when the entry is made and never restamped: a tool call that
    /// began in one turn and streamed updates into the next belongs to the turn
    /// that started it.
    pub turn: Option<u64>,
}

/// One turn the session had, and what became of it (`CONTEXT.md`, *Turn*).
///
/// The span rather than the traffic: which entries were in it is on the entries
/// themselves, so a turn with nothing in it is still a turn that happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    /// Which turn of this session it is, counting from one.
    pub id: u64,
    /// When the prompt that opened it went out.
    pub at: SystemTime,
    /// What it came to, or `None` while it is still the live turn — where that
    /// one stands is [`Inspector::turn`](crate::Inspector::turn)'s answer and
    /// not a second copy of it kept here.
    pub outcome: Option<TurnOutcome>,
}

/// One frame an entry was made of, and where the Trace recorded it.
#[derive(Clone, Debug)]
pub struct Recorded {
    pub frame: Frame,
    /// The ordinal the Trace captured it under — `dropped + position` read off
    /// a [snapshot](crate::Trace::snapshot), and stable as the ring rotates.
    ///
    /// `None` where the entry was made of a frame this client *sent*: a
    /// Conformance Annotation is read beside the call it judges as well as the
    /// answer, and the send path does not carry the ordinal back to the typed
    /// layer today. It costs an annotation nothing — an annotation renders its
    /// deciding frames open rather than a click away (§9), so it is the entry
    /// with the least use for a way to reach them.
    pub captured: Option<u64>,
}

/// An entry's identity.
///
/// The agent's own id where the protocol gives one, and arrival order where it
/// does not — so "keyed by id" is a fact about tool calls and "keyed by
/// arrival" is a statement that this kind of update has nothing to merge with,
/// rather than an accident of how the list was built.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EntryId {
    /// A tool call, keyed by the session it belongs to and the id the agent
    /// gave it. The session is part of the key because the id is only the
    /// agent's promise about *its* session.
    ToolCall(v1::SessionId, v1::ToolCallId),
    /// A permission request, keyed by the JSON-RPC id it arrived under — which
    /// is the id its answer will be addressed to, and the only thing that tells
    /// two identical-looking requests apart.
    Permission(RequestId),
    /// An elicitation, keyed the same way and for the same reason (§7.8).
    ///
    /// Its own variant rather than one *blocking request* key, because the two
    /// are answered by different screens and an id space they shared would make
    /// a permission request and an elicitation that arrived under the same
    /// JSON-RPC id — which nothing forbids across a reconnect — one entry.
    Elicitation(RequestId),
    /// Everything the protocol gives no id: one entry, keyed by when it turned
    /// up, merging with nothing.
    Arrival(u64),
}

/// What an entry is.
#[derive(Clone, Debug)]
#[non_exhaustive]
// The decoded variant is the big one and also the usual one, so boxing it would
// buy a smaller enum with an allocation per update — the wrong way round. The
// size is the schema's own (`v1::SessionUpdate` is what an agent can say), and
// the schema crate carries this same allowance for the same reason.
#[allow(clippy::large_enum_variant)]
pub enum EntryKind {
    /// A `session/update` the typed layer decoded as v1 — the decoration on a
    /// frame the trace already has (§8).
    Update(v1::SessionUpdate),
    /// A permission request the agent is waiting on, at the point in the turn
    /// where it stopped for it (§7.2).
    ///
    /// Not just a record of something that happened: it carries the resolver, so
    /// the screen rendering it is the screen that answers it, and the turn goes
    /// on when it does.
    Permission(PermissionRequest),
    /// An elicitation the agent is waiting on, where it asked (§7.8).
    ///
    /// The other entry that answers rather than only records. It may be about no
    /// session at all — a request-scoped elicitation is tied to a call rather
    /// than to a conversation — which is why the timeline draws these even when
    /// nothing is open.
    Elicitation(ElicitationRequest),
    /// A frame it did not, kept as itself.
    Unrecognized(Unrecognized),
    /// A place the specification says MUST and the frames say it was not met
    /// (§15 q7).
    ///
    /// The one entry that is the inspector's own voice rather than the agent's,
    /// and it is here for the same reason everything else is: it belongs beside
    /// the traffic it describes, which is what the timeline is. The frames it
    /// carries are that traffic — the `session/cancel` this client sent, and
    /// whatever answered the turn — so the claim and its evidence are read
    /// together.
    Annotation(Annotation),
}

/// A frame the typed layer left alone, and why (§8).
///
/// First-class: an unrecognized entry sits in the timeline beside the decoded
/// ones rather than in an error list, because an agent doing something this
/// client has no name for is the most interesting thing that can happen in an
/// inspector.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Unrecognized {
    /// A method the inspector does not service: an extension method, an
    /// unstable-v1 one, or a client service it declined by advertising no
    /// capability (§7.3). Requests were answered method-not-found; the call is
    /// evidence either way.
    NotServiced { method: String },
    /// A method it does service, carrying something v1 could not read — a
    /// `sessionUpdate` variant with no v1 name, a field of the wrong shape, v2
    /// traffic arriving early — or a frame that was not JSON-RPC at all, which
    /// announced no method to go by.
    Undecodable {
        method: Option<String>,
        /// What the decoder said. Displayed, not logged: the point of an
        /// inspector is that the reason is on the screen next to the frame.
        problem: String,
    },
    /// An answer to a request this client never sent — or, for the one
    /// notification that names something rather than reporting it, an
    /// `elicitation/complete` for an id nobody here is holding (§7.8). The
    /// specification says such a completion is ignored, and this is what
    /// ignoring it looks like in a tool that never drops a frame.
    Unsolicited,
    /// A method this client services, asking for a shape it never advertised:
    /// today, an elicitation in a mode outside the two it claims (§7.8).
    ///
    /// Answered invalid-params, which is the error the specification names for a
    /// mode the client did not advertise — and deliberately *not* answered on
    /// the reader's behalf, because declining and cancelling are both claims
    /// about a person and nothing drew this one for anybody.
    Unadvertised {
        method: String,
        /// What was asked for that this client never claimed, in the agent's own
        /// spelling.
        detail: String,
    },
}

#[derive(Default)]
struct Entries {
    entries: Vec<TimelineEntry>,
    /// Where each id-carrying entry is, for the merge.
    ///
    /// Cleared when a connection starts ([`Timeline::reopen`]) while the
    /// entries stay: ids are one agent's, and the next agent's identical id is
    /// a different tool call.
    index: HashMap<EntryId, usize>,
    /// The next arrival key. Never reset, so an entry's id stays unique for as
    /// long as the window is open — which is what a keyed list needs of it.
    arrivals: u64,
    /// Every turn this session has had, in order.
    turns: Vec<Turn>,
    /// Which of them is open, if one is. What an entry made now is stamped
    /// with.
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
    /// Every retained entry, oldest first. Retention holes are in `snapshot`.
    ///
    /// A snapshot by value, like the trace's: consumers read while the agent
    /// keeps talking, and none of them should be holding a lock while they
    /// render.
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

    /// Every turn this session has had, oldest first.
    ///
    /// Read from the same snapshot the entries are, and announced by the same
    /// revision: an entry stamped with a turn and the turn it names have to
    /// arrive at a reader together, or a screen draws a group whose outcome it
    /// has not been told about yet.
    pub fn turns(&self) -> Vec<Turn> {
        self.lock().turns.clone()
    }

    /// Opens a turn, and returns which one it is.
    ///
    /// Called where the prompt goes out, so that everything the agent says
    /// afterwards is stamped with it. A turn already open is left alone and its
    /// id returned: prompting over a running turn is one turn continuing from
    /// this store's point of view, and what the *state* makes of that is
    /// `client`'s (§11 seam 2).
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

    /// Closes the open turn with what it came to.
    ///
    /// Nothing where no turn is open: a failure reported after the stream
    /// already settled the turn is the same conclusion arriving twice, and the
    /// first one is the one the agent gave.
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
                    EntryKind::Update(notification.update),
                    turn,
                ));
                if !matches!(id, EntryId::Arrival(_)) {
                    entries.index.insert(id, position);
                }
            }
        }

        self.moved(&mut entries);
    }

    /// Puts a blocking request where the turn stopped for it (§7.2).
    ///
    /// Keyed by the id it arrived under, and not in the merge index: nothing
    /// merges into a request. An agent that asks twice under one id has asked
    /// twice, and two entries is what that looks like.
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

    /// Puts an elicitation where the agent asked it (§7.8).
    ///
    /// The same rules as a permission request, for the same reasons: keyed by
    /// the id it arrived under, never in the merge index, and two asks under one
    /// id are two entries. An elicitation that belongs to no session is an entry
    /// like any other — what a request-scoped one is tied to is a call, and the
    /// entry says so rather than being filed under a session it never named.
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

    /// Adds the frame that said an elicitation's out-of-band half finished, to
    /// the entries it named (§7.8).
    ///
    /// A notification rather than an update, and it merges rather than arriving:
    /// what completed is an entry already on the list, so the frame joins the
    /// frames that entry is made of — the same way a tool call's updates land on
    /// the card they are about ([§11](../../../docs/architecture.md) seam 3). The
    /// request itself has already been told; this is the record catching up with
    /// it.
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

    /// Says that a rule the specification states as a MUST was not met, beside
    /// the frames that decide it (§15 q7).
    ///
    /// At the end, like everything else: an annotation is made when the
    /// violation became decidable, and where that falls in the session is part
    /// of what it says. Never merges, and nothing merges into it — a second
    /// violation is a second thing that happened.
    pub(crate) fn annotate(&self, annotation: Annotation, frames: Vec<Frame>) {
        // Frames this client sent, mostly, and the ordinal for one of those is
        // not carried back up the send path (`Recorded::captured`).
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

    /// Adds a frame the typed layer could not use. Never merges: there is no id
    /// to merge on, and guessing at one would be the inspector interpreting
    /// traffic it just said it does not understand.
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
    /// The entries stay — what an agent said is evidence, and connecting to
    /// another one is not a reason to lose it, which is the rule the trace and
    /// the diagnostic log already follow. Only the merge index goes, because
    /// the ids it holds were the last agent's.
    ///
    /// Which is the opposite of what a *session* switch does
    /// ([`discard`](Self::discard)), and deliberately: a new connection is a
    /// new agent, whose session nobody has opened yet, while a new session on
    /// the same agent replaces the one this store is a view of.
    pub(crate) fn reopen(&self) {
        let mut entries = self.lock();
        entries.index.clear();
        // Whatever was open belonged to the last agent, which is gone. The
        // entries and their turns stay *here*, because a connection is started
        // optimistically and a command that was mistyped gets this far: what
        // empties the view is the session that replaces the one it is a view of
        // ([`discard`](Self::discard)), which a launch that never started an
        // agent never reaches.
        entries.open = None;
        self.moved(&mut entries);
    }

    /// Discards the session this is a view of, so the next one can be built
    /// into it (§7.5).
    ///
    /// **The inspector holds one live session, and this store is its view of
    /// it.** Opening another discards and rebuilds — no merge, no partition and
    /// no second timeline — which is the rule the host repository settled for
    /// hydration, and the one `session/load` exists to make possible: a session
    /// re-opened is a session replayed into an empty timeline.
    ///
    /// Every frame was captured in Trace before this layer had an opinion.
    /// Trace is untouched by a switch, subject to its own counted retention
    /// budgets. The arrival counter keeps counting for the same
    /// reason it survives a connection: an entry's id has to stay unique for as
    /// long as the window is open.
    pub(crate) fn discard(&self) {
        let mut entries = self.lock();
        entries.entries.clear();
        entries.index.clear();
        // The turns go with the entries: they are this session's, and the
        // session is what a discard replaces.
        entries.turns.clear();
        entries.open = None;
        entries.next_turn = 0;
        entries.dropped = Retention::default();
        entries.raw_bytes = 0;
        entries.raw_frames = 0;
        self.moved(&mut entries);
    }

    /// A way to say the timeline changed, for a change this store did not make.
    ///
    /// A pending request answers itself (`permission`), and the entry holding
    /// it changes without the list of entries changing at all — so what is
    /// handed out is the announcement and never the entries: a resolver that
    /// held the list it is in would be a cycle between a list and the things in
    /// it.
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

/// Says that something on the timeline changed, without being able to change
/// it: the half of a store a value that changes itself needs.
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
            self.turns.retain(|turn| {
                if excess == 0
                    || self.open == Some(turn.id)
                    || self.entries.iter().any(|e| e.turn == Some(turn.id))
                {
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
            _ => false,
        }
    }
    pub fn is_waiting(&self) -> bool {
        match &self.kind {
            EntryKind::Permission(request) => request.is_waiting(),
            EntryKind::Elicitation(request) => request.is_waiting(),
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
            Self::Unadvertised { method, .. } => Some(method),
            Self::Unsolicited => None,
        }
    }
}

/// The id an update merges on, if the protocol gave it one.
///
/// Tool calls, and for now only tool calls: they are the one v1 entity that
/// arrives as a create followed by patches. The others either carry no id
/// (chunks, plans, usage) or replace themselves wholesale, and inventing a key
/// for them would coalesce a session's history into a single line.
fn merge_key(notification: &v1::SessionNotification) -> Option<EntryId> {
    let session = notification.session_id.clone();
    match &notification.update {
        v1::SessionUpdate::ToolCall(call) => {
            Some(EntryId::ToolCall(session, call.tool_call_id.clone()))
        }
        v1::SessionUpdate::ToolCallUpdate(update) => {
            Some(EntryId::ToolCall(session, update.tool_call_id.clone()))
        }
        _ => None,
    }
}

/// Folds an update into the entry it belongs to.
///
/// A patch, not a replacement: an update carries only the fields that changed
/// (`ToolCallUpdateFields` is all `Option`), so an entry that took the last
/// status would lose the title that came with the create.
fn merge(kind: &mut EntryKind, update: v1::SessionUpdate) {
    let EntryKind::Update(existing) = kind else {
        // Unrecognized entries are never in the index, so nothing merges into
        // one.
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
        // A create for an id that already has an entry — the agent restating
        // the whole call — is the whole call, so it replaces.
        (existing, update) => *existing = update,
    }
}

fn patch(call: &mut v1::ToolCall, fields: v1::ToolCallUpdateFields) {
    let v1::ToolCallUpdateFields {
        kind,
        status,
        title,
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
        content,
        locations,
        raw_input,
        raw_output,
        ..
    } = update;

    fields.kind = kind.or(fields.kind);
    fields.status = status.or(fields.status);
    fields.title = title.or_else(|| fields.title.take());
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
