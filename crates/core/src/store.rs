//! Observable stores: a value or log plus a signal that it moved, which a UI or
//! a test subscribes to. Core never calls into a UI. See
//! `docs/architecture.md` §5.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

use tokio::sync::watch;

/// Takes a store's lock, poisoned or not, so a panic elsewhere does not turn
/// the record into a second panic here.
pub(crate) fn locked<T>(state: &Mutex<T>) -> MutexGuard<'_, T> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A list of what happened, and a signal when more happens.
///
/// Entries are appended and never reordered or edited. A
/// [`budgeted`](Log::budgeted) log gives up its oldest entries and a
/// [`clear`](Log::clear) gives up all of them; both are counted as dropped.
///
/// A handle: clones share one log.
pub(crate) struct Log<T> {
    held: Arc<Mutex<Held<T>>>,
    /// The entry count, as a signal subscribers can watch without cloning the
    /// log.
    length: Arc<watch::Sender<usize>>,
}

/// The entries a log still has and the count of those it does not, under one
/// lock so an export's header always describes the records beside it.
struct Held<T> {
    entries: VecDeque<T>,
    /// Entries captured and no longer held. Never reset.
    dropped: usize,
    /// How many entries this log keeps, or `None` to keep everything.
    capacity: Option<usize>,
    bytes: usize,
    byte_limit: usize,
    weight: fn(&T) -> usize,
}

/// One value that changes, and a signal when it does: what is true now, where a
/// [`Log`] is everything that happened.
///
/// A handle: clones share one field.
pub(crate) struct Field<T>(Arc<watch::Sender<T>>);

/// A subscription to a store: what it says now, and a way to wait for it to say
/// something else. Hides the channel so consumers need not share core's runtime.
pub struct Changes<T>(watch::Receiver<T>);

impl<T> Log<T> {
    /// A ring that keeps its newest `capacity` entries (and at most
    /// `byte_limit` bytes by `weight`), counting the rest as dropped.
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

    /// Drops every entry it holds, counting them as dropped. Not a reset:
    /// ordinals keep counting.
    pub(crate) fn clear(&self) {
        let mut held = self.lock();
        held.dropped += held.entries.len();
        held.entries.clear();
        held.bytes = 0;
        self.length.send_replace(0);
    }

    /// Counts `count` entries as dropped before they were ever held here, as an
    /// imported file reports.
    pub(crate) fn forgotten(&self, count: usize) {
        self.lock().dropped += count;
    }

    /// Watches this log, starting from its current length.
    pub(crate) fn changes(&self) -> Changes<usize> {
        Changes(self.length.subscribe())
    }

    pub(crate) fn append(&self, entry: T) -> u64 {
        self.append_then(entry, |_, _| {})
    }

    /// Appends an entry and runs `handoff(ordinal, dropped)` before releasing
    /// the lock.
    ///
    /// The trace uses `handoff` to enqueue a frame under the same lock it
    /// records it, so concurrent senders cannot record in one order and send in
    /// another.
    ///
    /// Returns the entry's ordinal: entries taken before it, from zero. It
    /// survives rotation and [`clear`](Self::clear), so `dropped + position`
    /// finds the entry again.
    pub(crate) fn append_then(&self, entry: T, handoff: impl FnOnce(u64, usize)) -> u64 {
        let mut held = self.lock();
        let ordinal = (held.dropped + held.entries.len()) as u64;
        held.bytes += (held.weight)(&entry);
        held.entries.push_back(entry);
        while held
            .capacity
            .is_some_and(|capacity| held.entries.len() > capacity)
            || held.bytes > held.byte_limit
        {
            let removed = held.entries.pop_front().expect("over budget has entries");
            held.bytes -= (held.weight)(&removed);
            held.dropped += 1;
        }
        handoff(ordinal, held.dropped);
        // Announced under the lock so a woken subscriber never finds fewer
        // entries than announced. Always sent: a full ring appends without its
        // length changing.
        self.length.send_replace(held.entries.len());
        ordinal
    }

    fn lock(&self) -> MutexGuard<'_, Held<T>> {
        locked(&self.held)
    }
}

impl<T: Clone> Log<T> {
    /// Every entry it holds, oldest first, copied so nobody renders under the
    /// lock.
    pub(crate) fn entries(&self) -> Vec<T> {
        self.lock().entries.iter().cloned().collect()
    }

    /// The entries and the dropped count, read under one lock.
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

    /// Changes the value in place, deciding from the current value with no gap
    /// between read and write; `change` returns whether it changed anything,
    /// and subscribers are told only when it did.
    pub(crate) fn update(&self, change: impl FnOnce(&mut T) -> bool) -> bool {
        self.0.send_if_modified(change)
    }

    /// Changes the value in place and returns the result, read under the same
    /// lock. Subscribers are always told.
    pub(crate) fn change(&self, change: impl FnOnce(&mut T)) -> T {
        let mut changed = None;
        self.0.send_modify(|value| {
            change(value);
            changed = Some(value.clone());
        });
        changed.expect("send_modify runs its closure")
    }

    /// Watches this field, starting from its current value.
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

    /// Waits for the next change and returns the store's value then, or `None`
    /// once the store is gone.
    ///
    /// Changes coalesce: a slow consumer wakes once with the latest value.
    pub async fn next(&mut self) -> Option<T> {
        self.0.changed().await.ok()?;
        Some(self.0.borrow_and_update().clone())
    }
}
