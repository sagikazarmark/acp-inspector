//! The trace decorator (`docs/architecture.md` §6.2): every frame, both
//! directions, direction- and timestamp-tagged, recorded before anything above
//! the seam sees it.
//!
//! **One capture mechanism for the whole program.** The trace view, the JSONL
//! export and misbehaving-agent logging are three consumers of this one log;
//! nothing else in the inspector touches the wire. That is what makes the trace
//! trustworthy: there is no path a frame can take that skips capture. Retention
//! and Clear can remove captured Frames, always counted in `dropped`.
//!
//! It is a decorator rather than something the transport does, because then
//! every transport gets it — the deferred WebSocket factory records identically
//! without writing a line of recording code.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use crate::connection::Connection;
use crate::export::Export;
use crate::frame::{Direction, Frame, FrameKind, FrameSummary};
use crate::store::{Changes, Log, locked};

/// The ordered log of every frame on a connection.
///
/// A handle, not the log: cloning shares one log, so the UI, the exporter and
/// whatever else consumes it are all looking at the same frames.
#[derive(Clone)]
pub struct Trace {
    entries: Log<TracedFrame>,
    /// How many connections have been tapped, which is where each frame's
    /// [`connection`](TracedFrame::connection) comes from.
    connections: Arc<AtomicU64>,
    pairs: Arc<Mutex<Pairs>>,
}

/// The other half of a request/response pair, as the trace saw them cross.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Counterpart {
    /// The ordinal the other frame was captured under.
    pub ordinal: u64,
    /// From the request crossing to the answer crossing, on the monotonic
    /// clock (an incoming frame counts from when it was read).
    pub elapsed: Duration,
    /// Set on an answer whose id matches the request's only with its type
    /// changed (`"3"` or `3.0` for `3`). JSON-RPC does not match those, so the
    /// request itself stays unanswered and has no counterpart.
    pub mistyped: bool,
}

/// Requests waiting for an answer, and the pairs made so far, by ordinal.
#[derive(Default)]
struct Pairs {
    open: HashMap<(u64, Direction, String), Open>,
    links: HashMap<u64, Counterpart>,
}

struct Open {
    ordinal: u64,
    at: Instant,
    loose: Option<String>,
}

/// One frame, as the trace holds it.
#[derive(Clone, Debug)]
pub struct TracedFrame {
    /// When the frame passed the seam — read from the agent, or handed to the
    /// transport.
    pub at: SystemTime,
    /// Which connection it crossed, counting from one.
    ///
    /// **The trace outlives any one agent** ([`tap`](Trace::tap) is why it
    /// exists), so a trace can hold two agents' traffic and an export can carry
    /// it. Without this, a consumer correlating JSON-RPC ids would fold two id
    /// spaces into one — both agents' answers to "id 1" — and read a
    /// conversation nobody had.
    pub connection: u64,
    pub direction: Direction,
    pub frame: Frame,
}

impl Trace {
    /// How many frames the trace keeps.
    ///
    /// **A ring, and a large one** (§15 q3). The three candidates were a cap
    /// that stops recording, a ring that drops the oldest, and no bound at all;
    /// a cap that stops is the worst of them for an inspector, because a
    /// misbehaving agent's *last* frames are the ones being looked for and a
    /// stopped log throws away exactly those. Unbounded is what this was until
    /// the export landed, and it is a promise no long-running window can keep:
    /// frames are cheap but not free, and a chatty agent left running overnight
    /// has no natural end.
    ///
    /// So: keep the newest, count what went, and say so in every export — the
    /// MCP Inspector's 1000 is a tenth of this and it clears on disconnect,
    /// which this deliberately does not do (an agent that died is when its
    /// frames are worth the most).
    pub const CAPACITY: usize = 10_000;
    /// Raw UTF-8 bytes, independently of the entry cap.
    pub const BYTE_CAPACITY: usize = 64 * 1024 * 1024;

    /// Wraps a connection so both of its frame ends record into a new trace.
    ///
    /// The connection returned is the only one worth keeping: the tap is
    /// installed on the values themselves, so there is no untapped view of the
    /// same frames left over to use by accident.
    pub fn decorate(connection: Connection) -> (Connection, Self) {
        let trace = Self::default();
        (trace.tap(connection), trace)
    }

    /// Wraps a connection so its frames record into *this* trace.
    ///
    /// What [`decorate`](Self::decorate) does, for a trace that already exists:
    /// the inspector's trace outlives any one connection, so connecting a
    /// second agent has to be able to point the same log at a new pipe.
    #[must_use]
    pub fn tap(&self, connection: Connection) -> Connection {
        let ordinal = self.connections.fetch_add(1, Ordering::Relaxed) + 1;
        connection.tapped(Tap {
            trace: self.clone(),
            connection: ordinal,
        })
    }

    /// Every frame so far, oldest first.
    ///
    /// A snapshot by value: consumers read the log while frames keep arriving,
    /// and none of them should be holding a lock while they render.
    ///
    /// The newest [`CAPACITY`](Self::CAPACITY) of them, when an agent has said
    /// more than that; [`dropped`](Self::dropped) is how many went.
    pub fn entries(&self) -> Vec<TracedFrame> {
        self.entries.entries()
    }

    /// How many frames it holds.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// How many frames it captured and no longer holds — cleared, or aged out
    /// of [`CAPACITY`](Self::CAPACITY).
    ///
    /// A trace that has forgotten something has to be able to say so: it is the
    /// difference between a whole session and what was left of one, and the
    /// export puts the number in its header for the same reason.
    pub fn dropped(&self) -> usize {
        self.entries.dropped()
    }

    /// The frames it holds and the count of the ones it does not, from one
    /// moment.
    ///
    /// What [`entries`](Self::entries) and [`dropped`](Self::dropped) cannot do
    /// together, for the two consumers that need them to agree. The
    /// [`export`](Self::export) is one: a header that counted a moment its
    /// records did not come from would be a self-description that was wrong. A
    /// renderer is the other, and for a subtler reason — the two numbers are
    /// what a frame's *identity* is made of. `dropped + position` is the ordinal
    /// a frame was captured under, which stays the same as the ring rotates
    /// underneath it; read a moment apart, the ring can turn between them and
    /// every frame on screen appears to shift by one.
    pub fn snapshot(&self) -> (Vec<TracedFrame>, usize) {
        self.entries.snapshot()
    }

    /// Takes the trace as evidence: the frames it holds, right now, in the
    /// pinned JSONL schema (§10, `docs/trace-export.md`).
    ///
    /// A snapshot. Whatever the agent says next is not in it, and neither a
    /// [`clear`](Self::clear) nor another connection can reach back into one.
    pub fn export(&self) -> Export {
        let (frames, dropped) = self.snapshot();
        Export::new(SystemTime::now(), frames, dropped)
    }

    /// Throws away every frame the trace holds, so that what follows is the
    /// interaction the user is about to reproduce and nothing else (§9).
    ///
    /// **Clear is not the opposite of export, and not an undo of one.** An
    /// export already taken is a file, and this cannot reach it; an export
    /// taken afterwards carries only frames captured since, and says in its
    /// header how many it never had. Clearing does not stop the recording,
    /// disconnect anything, or wait for a turn to end — and nothing else clears
    /// this log: an agent that vanished keeps its frames until somebody asks
    /// for them to go, which is where this differs from the MCP Inspector's
    /// clear-on-disconnect.
    pub fn clear(&self) {
        self.entries.clear();
        let dropped = self.entries.dropped() as u64;
        locked(&self.pairs)
            .links
            .retain(|&ordinal, other| ordinal >= dropped && other.ordinal >= dropped);
    }

    /// The frame that answers, or is answered by, the one captured under
    /// `ordinal`: a request and its response on the same connection, matched
    /// by id in either direction.
    pub fn counterpart(&self, ordinal: u64) -> Option<Counterpart> {
        locked(&self.pairs).links.get(&ordinal).copied()
    }

    /// Watches the trace: the frame count now, and a wait for the next frame.
    ///
    /// The trace view is a rendering of this — it re-reads
    /// [`entries`](Self::entries) when the count moves, so a frame is on screen
    /// because it crossed the wire and for no other reason.
    pub fn changes(&self) -> Changes<usize> {
        self.entries.changes()
    }

    /// Stamps and records one frame. Called by the tap on a connection's frame
    /// ends, before the frame goes anywhere else.
    ///
    /// Answers with the ordinal it captured the frame under, which is the
    /// frame's identity in this trace: `dropped + position` is the same number
    /// read off a snapshot ([`snapshot`](Self::snapshot)), so a consumer that
    /// kept one can find the row again after the ring has turned. It is how a
    /// Timeline entry says which frames it was made of and where they are
    /// (§6.2: what the trace knows about a frame, it knows first).
    fn record(&self, connection: u64, direction: Direction, frame: &Frame) -> u64 {
        // Stamped when the transport read it, not when it was consumed.
        let at = frame.read_at().unwrap_or_else(SystemTime::now);
        let instant = frame.read_instant().unwrap_or_else(Instant::now);
        // Parsed before the log is held, so a large frame does not stall readers.
        let summary = frame.summary();
        let entry = TracedFrame {
            at,
            connection,
            direction,
            frame: frame.clone(),
        };
        self.entries.append_then(entry, |ordinal, dropped| {
            self.pair(connection, direction, summary, ordinal, instant, dropped);
        })
    }

    /// Pairs a request with its answer, under the log's lock so the pairing
    /// sees frames in the order they were recorded.
    fn pair(
        &self,
        connection: u64,
        direction: Direction,
        summary: &FrameSummary,
        ordinal: u64,
        at: Instant,
        dropped: usize,
    ) {
        let Some(key) = summary.key.as_ref() else {
            return;
        };
        let mut pairs = locked(&self.pairs);
        match summary.kind {
            FrameKind::Call => {
                pairs.open.insert(
                    (connection, direction, key.exact.clone()),
                    Open {
                        ordinal,
                        at,
                        loose: key.loose.clone(),
                    },
                );
            }
            FrameKind::Result | FrameKind::Error => {
                let asker = direction.opposite();
                if let Some(request) = pairs.open.remove(&(connection, asker, key.exact.clone())) {
                    let elapsed = at.saturating_duration_since(request.at);
                    pairs.links.insert(
                        request.ordinal,
                        Counterpart {
                            ordinal,
                            elapsed,
                            mistyped: false,
                        },
                    );
                    pairs.links.insert(
                        ordinal,
                        Counterpart {
                            ordinal: request.ordinal,
                            elapsed,
                            mistyped: false,
                        },
                    );
                } else if let Some(loose) = &key.loose {
                    let request = pairs.open.iter().find_map(|((on, way, _), open)| {
                        (*on == connection && *way == asker && open.loose.as_ref() == Some(loose))
                            .then_some((open.ordinal, open.at))
                    });
                    if let Some((request, since)) = request {
                        pairs.links.insert(
                            ordinal,
                            Counterpart {
                                ordinal: request,
                                elapsed: at.saturating_duration_since(since),
                                mistyped: true,
                            },
                        );
                    }
                }
            }
            _ => {}
        }
        // Forget what the ring has already let go of, now and then.
        let dropped = dropped as u64;
        if pairs.links.len() > 2 * Self::CAPACITY {
            pairs
                .links
                .retain(|&ordinal, other| ordinal >= dropped && other.ordinal >= dropped);
        }
        if pairs.open.len() > Self::CAPACITY {
            pairs.open.retain(|_, open| open.ordinal >= dropped);
        }
    }

    /// A trace rebuilt from frames read back out of an export, numbered from
    /// `dropped` as they were when they were captured, and paired again.
    ///
    /// Nothing taps it: it is a record of a connection that is over. Pairs are
    /// timed from the frames' own stamps, which are the only clock a file has.
    pub(crate) fn rebuilt(frames: Vec<TracedFrame>, dropped: usize) -> Self {
        let trace = Self::default();
        trace.entries.forgotten(dropped);
        let connections = frames.iter().map(|traced| traced.connection).max();
        trace
            .connections
            .store(connections.unwrap_or(0), Ordering::Relaxed);
        let earliest = frames.iter().map(|traced| traced.at).min();
        let base = Instant::now();
        for traced in frames {
            let since = earliest
                .and_then(|earliest| traced.at.duration_since(earliest).ok())
                .unwrap_or_default();
            let instant = base.checked_add(since).unwrap_or(base);
            let summary = traced.frame.summary().clone();
            let (connection, direction) = (traced.connection, traced.direction);
            trace.entries.append_then(traced, |ordinal, dropped| {
                trace.pair(connection, direction, &summary, ordinal, instant, dropped);
            });
        }
        trace
    }

    /// Records a frame and hands it on without letting go of the log in
    /// between, so what the log says about order is what the transport will do
    /// about order — even with several tasks sending at once.
    ///
    /// `handoff` is synchronous and does one thing (put the frame in the pipe);
    /// it must not touch this trace, and cannot, being private.
    fn record_then(
        &self,
        connection: u64,
        direction: Direction,
        frame: Frame,
        handoff: impl FnOnce(Frame),
    ) -> u64 {
        let summary = frame.summary().clone();
        let entry = TracedFrame {
            at: SystemTime::now(),
            connection,
            direction,
            frame: frame.clone(),
        };
        self.entries.append_then(entry, |ordinal, dropped| {
            self.pair(
                connection,
                direction,
                &summary,
                ordinal,
                Instant::now(),
                dropped,
            );
            handoff(frame);
        })
    }
}

/// A trace, and which connection it is recording — what a tapped connection's
/// frame ends hold.
///
/// The ordinal is fixed when the connection is tapped rather than read at
/// record time, because it is a fact about the pipe the frame crossed: a frame
/// still on its way out when the next agent is launched belongs to the
/// connection that sent it.
#[derive(Clone)]
pub(crate) struct Tap {
    trace: Trace,
    connection: u64,
}

impl Tap {
    pub(crate) fn record(&self, direction: Direction, frame: &Frame) -> u64 {
        self.trace.record(self.connection, direction, frame)
    }

    pub(crate) fn record_then(
        &self,
        direction: Direction,
        frame: Frame,
        handoff: impl FnOnce(Frame),
    ) -> u64 {
        self.trace
            .record_then(self.connection, direction, frame, handoff)
    }
}

impl Default for Trace {
    fn default() -> Self {
        Self {
            entries: Log::budgeted(Self::CAPACITY, Self::BYTE_CAPACITY, |entry| {
                entry.frame.as_str().len()
            }),
            connections: Arc::default(),
            pairs: Arc::default(),
        }
    }
}
