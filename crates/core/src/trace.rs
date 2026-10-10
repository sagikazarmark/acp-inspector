//! The trace decorator (`docs/architecture.md` §6.2): every frame, both
//! directions, timestamped and recorded before anything above the seam sees it.
//!
//! The trace view and the JSONL export both read this one log, and no frame
//! can reach the typed layer without passing through it. Retention and clear
//! remove frames, always counted in `dropped`. Being a decorator, it records
//! identically for every transport.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use crate::connection::Connection;
use crate::export::Export;
use crate::frame::{Direction, Frame, FrameKind, FrameSummary};
use crate::store::{Changes, Log, locked};

/// The ordered log of every frame on a connection. A handle: clones share one
/// log.
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
    /// Which connection it crossed, counting from one. A trace can span several
    /// connections, and JSON-RPC ids are only unique within one.
    pub connection: u64,
    pub direction: Direction,
    pub frame: Frame,
}

impl Trace {
    /// How many frames the trace keeps: a ring that drops the oldest, because
    /// a misbehaving agent's last frames matter most (§15 q3). It is not
    /// cleared on disconnect.
    pub const CAPACITY: usize = 10_000;
    /// Raw UTF-8 bytes, independently of the entry cap.
    pub const BYTE_CAPACITY: usize = 64 * 1024 * 1024;

    /// Wraps a connection so both of its frame ends record into a new trace.
    /// The returned connection is tapped; no untapped view remains.
    pub fn decorate(connection: Connection) -> (Connection, Self) {
        let trace = Self::default();
        (trace.tap(connection), trace)
    }

    /// Wraps a connection so its frames record into this existing trace, as a
    /// new connection ordinal.
    #[must_use]
    pub fn tap(&self, connection: Connection) -> Connection {
        let ordinal = self.connections.fetch_add(1, Ordering::Relaxed) + 1;
        connection.tapped(Tap {
            trace: self.clone(),
            connection: ordinal,
        })
    }

    /// Every frame held, oldest first, as a copy. At most
    /// [`CAPACITY`](Self::CAPACITY); [`dropped`](Self::dropped) counts the rest.
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

    /// How many frames it captured and no longer holds: cleared, or aged out
    /// of [`CAPACITY`](Self::CAPACITY). Never decreases.
    pub fn dropped(&self) -> usize {
        self.entries.dropped()
    }

    /// The frames it holds and the dropped count, read at one moment.
    ///
    /// `dropped + position` is each frame's capture ordinal, stable as the ring
    /// rotates; reading the two separately could shift every frame by one.
    pub fn snapshot(&self) -> (Vec<TracedFrame>, usize) {
        self.entries.snapshot()
    }

    /// The frames held right now, in the pinned JSONL schema
    /// (`docs/trace-export.md`). A snapshot: later frames and clears do not
    /// affect it.
    pub fn export(&self) -> Export {
        let (frames, dropped) = self.snapshot();
        Export::new(SystemTime::now(), frames, dropped)
    }

    /// Drops every frame the trace holds, counting them in
    /// [`dropped`](Self::dropped). Recording continues; nothing else clears the
    /// trace.
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
    pub fn changes(&self) -> Changes<usize> {
        self.entries.changes()
    }

    /// Stamps and records one frame, before it goes anywhere else. Returns its
    /// capture ordinal (`dropped + position` in a snapshot).
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

    /// A trace made of frames already captured (an [`Imported`](crate::Imported)
    /// export, or a test's), numbered from `dropped` and paired again.
    ///
    /// Nothing taps it. Pairs are timed from the frames' own timestamps.
    pub fn from_records(frames: Vec<TracedFrame>, dropped: usize) -> Self {
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

    /// Records a frame and hands it on under the log's lock, so recorded order
    /// is send order even with several tasks sending at once.
    ///
    /// `handoff` must be synchronous and must not touch this trace.
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

/// A trace and the connection ordinal it records under, held by a tapped
/// connection's frame ends.
///
/// The ordinal is fixed at tap time so a frame still in flight when the next
/// agent launches stays with the connection that sent it.
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
