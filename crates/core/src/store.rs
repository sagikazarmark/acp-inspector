//! What an observable store is made of (`docs/architecture.md` §5): a log of
//! what happened, and a subscription to its changes.
//!
//! **The UI subscribes; core never renders and never calls into a UI.** A store
//! is therefore a value plus a signal that it moved — the shape a Dioxus signal,
//! a test's `await`, and a later web surface can all consume unchanged, and the
//! reason core needs no idea that any of them exist.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

use tokio::sync::watch;

/// Takes a store's lock, poisoned or not.
///
/// **A poisoned store is still a store**: a panic elsewhere must not turn the
/// record of what an agent did into a second panic here. Every state this crate
/// holds behind a mutex is held for that reason, so the rule is stated once and
/// used wherever one is taken.
pub(crate) fn locked<T>(state: &Mutex<T>) -> MutexGuard<'_, T> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A list of what happened, and a signal when more happens.
///
/// Entries arrive at the end and are never reordered or edited — a log's order
/// is the order things happened in. What it does not promise is to keep them
/// all: a [`budgeted`](Log::budgeted) log gives up its oldest to stay a fixed
/// size, and a [`clear`](Log::clear) gives up everything, both of them counted
/// so that a log which has forgotten something can say so.
///
/// A handle, not the log: cloning shares one log, so every consumer of a store
/// is looking at the same entries.
pub(crate) struct Log<T> {
    held: Arc<Mutex<Held<T>>>,
    /// How many entries there are, as a signal — meaningful on its own, so a
    /// subscriber that only wants to know "is there more" never has to clone
    /// the log to find out.
    length: Arc<watch::Sender<usize>>,
}

/// What a log holds: the entries it still has, and the count of the ones it
/// does not.
///
/// The two under one lock, because a reader that took them separately could
/// report a set of entries alongside a count that describes a different moment
/// — and the export's whole claim is that its header describes its records.
struct Held<T> {
    entries: VecDeque<T>,
    /// Entries this log captured and no longer holds: aged out of a cap, or
    /// dropped by a [`clear`](Log::clear). Never reset, because it is the log's
    /// admission that it is not the whole story, and an admission that resets
    /// is not one.
    dropped: usize,
    /// How many entries this log keeps, or `None` to keep everything.
    capacity: Option<usize>,
    bytes: usize,
    byte_limit: usize,
    weight: fn(&T) -> usize,
}

/// One value that changes, and a signal when it does.
///
/// The other half of what a store can be: a [`Log`] is everything that
/// happened, a field is what is true now. The turn's state is the reason this
/// exists (§11 seam 2) — a turn is not a list of turns, it is a field several
/// things write to.
///
/// A handle, not the value: cloning shares one field.
pub(crate) struct Field<T>(Arc<watch::Sender<T>>);

/// A subscription to a store: what it says now, and a way to wait for it to say
/// something else.
///
/// Wrapping the channel rather than handing it out keeps the async plumbing an
/// implementation detail — core's consumers are a UI and its tests, and neither
/// should have to agree with core about which runtime it is built on.
pub struct Changes<T>(watch::Receiver<T>);

impl<T> Log<T> {
    /// A log that keeps its newest `capacity` entries and counts the rest as
    /// dropped — a ring, not a stop: the entries a bounded log gives up are the
    /// oldest, because the newest are the ones whoever is watching came for.
    pub(crate) fn budgeted(capacity: usize, byte_limit: usize, weight: fn(&T) -> usize) -> Self {
        Self {
            held: Arc::new(Mutex::new(Held {
                capacity: Some(capacity),
                byte_limit,
                weight,
                ..Held::empty()
            })),
            length: Arc::new(watch::Sender::new(0)),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.lock().entries.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.lock().entries.is_empty()
    }

    /// How many entries this log captured and no longer holds.
    pub(crate) fn dropped(&self) -> usize {
        self.lock().dropped
    }

    /// Drops every entry it holds, counting them as dropped.
    ///
    /// Not a reset: what a log has already given out — an export written to a
    /// file — is not this log's any more, and what it gives out next says how
    /// much it threw away to get there.
    pub(crate) fn clear(&self) {
        let mut held = self.lock();
        held.dropped += held.entries.len();
        held.entries.clear();
        held.bytes = 0;
        self.length.send_replace(0);
    }

    /// Watches this log. The subscription starts from the log as it is now, so
    /// a consumer reads once and then waits.
    pub(crate) fn changes(&self) -> Changes<usize> {
        Changes(self.length.subscribe())
    }

    pub(crate) fn append(&self, entry: T) -> u64 {
        self.append_then(entry, || {})
    }

    /// Appends an entry and does one more thing before letting go of the log.
    ///
    /// `handoff` is what makes the log's order the real order: the trace records
    /// a frame and hands it to the transport under the same lock, so two tasks
    /// sending at once cannot record in one order and enqueue in the other.
    ///
    /// Returns the ordinal the entry was appended under — how many entries this
    /// log had taken before it, counting from zero. It is read under the same
    /// lock as the append for the reason the handoff is: two tasks appending at
    /// once must not be able to number themselves in one order and land in the
    /// other. It stays true as the ring rotates and after a clear, because
    /// neither of those is a reset ([`clear`](Self::clear)) — which is what
    /// makes it an identity rather than a position, and what lets a consumer
    /// that kept one find the entry again in `dropped + position`.
    pub(crate) fn append_then(&self, entry: T, handoff: impl FnOnce()) -> u64 {
        let mut held = self.lock();
        let ordinal = (held.dropped + held.entries.len()) as u64;
        held.bytes += (held.weight)(&entry);
        held.entries.push_back(entry);
        // The oldest goes to make room, under the same lock as the newest
        // arriving, so what the log holds and what it says it dropped are never
        // read apart.
        while held
            .capacity
            .is_some_and(|capacity| held.entries.len() > capacity)
            || held.bytes > held.byte_limit
        {
            let removed = held.entries.pop_front().expect("over budget has entries");
            held.bytes -= (held.weight)(&removed);
            held.dropped += 1;
        }
        handoff();
        // What the entry just appended is called, in the same terms a reader of
        // this log has: the last position, plus everything that has left.
        // Announced while the log is still held, so a subscriber that wakes on
        // a count and then reads the entries can never find fewer than it was
        // told about, and concurrent appends cannot announce out of order.
        //
        // Sent rather than compared: a log at its cap appends without its
        // length changing, and a subscriber told nothing happened would stop
        // seeing the newest entries at exactly the point there are the most of
        // them.
        self.length.send_replace(held.entries.len());
        ordinal
    }

    fn lock(&self) -> MutexGuard<'_, Held<T>> {
        locked(&self.held)
    }
}

impl<T: Clone> Log<T> {
    /// Every entry it holds, oldest first.
    ///
    /// A snapshot by value: consumers read while the agent keeps talking, and
    /// none of them should be holding a lock while they render.
    pub(crate) fn entries(&self) -> Vec<T> {
        self.lock().entries.iter().cloned().collect()
    }

    /// The entries and the count of what is missing, from one moment.
    ///
    /// What [`entries`](Self::entries) and [`dropped`](Self::dropped) cannot do
    /// together: an export reads both, and a header that counted a moment its
    /// records did not come from would be a self-description that was wrong.
    pub(crate) fn snapshot(&self) -> (Vec<T>, usize) {
        let held = self.lock();
        (held.entries.iter().cloned().collect(), held.dropped)
    }
}

impl<T> Clone for Log<T> {
    fn clone(&self) -> Self {
        Self {
            held: Arc::clone(&self.held),
            length: Arc::clone(&self.length),
        }
    }
}

impl<T> Default for Log<T> {
    /// An unbounded log: everything, until somebody clears it.
    fn default() -> Self {
        Self {
            held: Arc::new(Mutex::new(Held::empty())),
            length: Arc::new(watch::Sender::new(0)),
        }
    }
}

impl<T> Held<T> {
    fn empty() -> Self {
        Self {
            entries: VecDeque::new(),
            dropped: 0,
            capacity: None,
            bytes: 0,
            byte_limit: usize::MAX,
            weight: |_| 0,
        }
    }
}

impl<T: Clone> Field<T> {
    pub(crate) fn get(&self) -> T {
        self.0.borrow().clone()
    }

    pub(crate) fn set(&self, value: T) {
        self.0.send_replace(value);
    }

    /// Changes the value in place, and says whether it changed.
    ///
    /// What [`set`](Self::set) cannot do: decide *from the current value*
    /// whether to write at all, without a gap between the reading and the
    /// writing. A turn that ended while a cancel was in flight is the reason —
    /// two producers, one field, and the later one must not overwrite an answer
    /// the agent already gave.
    ///
    /// `change` returns whether it changed anything; subscribers are told only
    /// when it did.
    pub(crate) fn update(&self, change: impl FnOnce(&mut T) -> bool) -> bool {
        self.0.send_if_modified(change)
    }

    /// Changes the value in place and answers with what it says afterwards.
    ///
    /// What [`update`](Self::update) cannot do: hand the new value back. A
    /// caller that folds something into a field and then has to report what the
    /// field now holds would otherwise read it again afterwards — through a gap
    /// another writer can reach into. Subscribers are always told, because a
    /// caller that changes a value it also wants back is changing it.
    pub(crate) fn change(&self, change: impl FnOnce(&mut T)) -> T {
        let mut changed = None;
        self.0.send_modify(|value| {
            change(value);
            changed = Some(value.clone());
        });
        changed.expect("send_modify runs its closure")
    }

    /// Watches this field, starting from what it says now — so a consumer
    /// subscribes once and then waits, and never has to poll to find out
    /// whether it missed something.
    pub(crate) fn changes(&self) -> Changes<T> {
        Changes(self.0.subscribe())
    }
}

impl<T> Clone for Field<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T: Default> Default for Field<T> {
    fn default() -> Self {
        Self(Arc::new(watch::Sender::new(T::default())))
    }
}

impl<T: Clone> Changes<T> {
    pub(crate) fn new(receiver: watch::Receiver<T>) -> Self {
        Self(receiver)
    }

    /// Waits for the next change, and answers with what the store says then.
    ///
    /// `None` once nothing will ever change again — the inspector holding the
    /// store is gone — so a consumer looping on this ends with it instead of
    /// spinning.
    ///
    /// **Changes coalesce.** A consumer slower than the agent wakes once with
    /// the latest state rather than once per entry, which is what keeps a
    /// chatty turn from costing one render per frame.
    pub async fn next(&mut self) -> Option<T> {
        self.0.changed().await.ok()?;
        Some(self.0.borrow_and_update().clone())
    }
}
