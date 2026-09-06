//! What the spawn form remembers (`docs/architecture.md` §9, and §15 q6's
//! "keep it trivial and local"): the last few invocations that worked, so
//! relaunching one is a click instead of four fields typed again.
//!
//! **A convenience, not a catalog.** The persistent agent catalog with
//! import/export is deferred (§1.1), and everything about this is bounded so it
//! cannot quietly become one: a short list, most-recent-first, nothing to name,
//! nothing to edit, nothing to share. One file, written where the platform keeps
//! a program's own state, read back at startup and otherwise ignored.
//!
//! It is in core for the reason everything else is (§5): what is remembered,
//! when a second launch is the same launch, and how many is enough are decisions
//! testable without a window — `crates/core/tests/recent.rs` — and the desktop crate
//! renders the list and calls [`record`](RecentCommands::record) when a launch
//! worked.
//!
//! **The file is the inspector's own, and is not a contract.** Unlike the trace
//! export (§10), nothing else is meant to read it, so it carries a format tag
//! and a version only so that a later shape can *ignore* an older file rather
//! than misread one. Anything it cannot read is no list at all: a broken
//! convenience is not evidence about an agent, and a form that refused to open
//! over it would be the tail wagging the tool.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

use crate::command::AgentCommand;
use crate::kept::{self, Access};
use crate::store::locked;

/// What the file calls itself, so a file that is something else can be told
/// apart from one this wrote.
const FORMAT: &str = "acp-inspector-recent";

/// The shape's version. It changes when this would read an older file wrongly —
/// at which point the older file is ignored, which is all a convenience owes
/// anybody.
const VERSION: u32 = 1;

/// The file itself.
const FILE: &str = "recent.json";

/// What is said when there is nowhere on this machine to keep the list.
const NOWHERE: &str = "no directory to remember commands in";

/// The invocations that recently worked, newest first.
///
/// A handle, not the list: cloning shares one, so the window can hand it to the
/// callback that records into it without threading a reference through the tree
/// — the same shape [`Inspector`](crate::Inspector) has, for the same reason.
///
/// **What it holds outlives a failure to save it.** Recording answers with
/// whether the file was written, and the entry is in the list either way: a run
/// whose list cannot be persisted still has one, and whoever recorded it is told
/// it will not survive the window rather than left to find out at the next
/// launch.
#[derive(Clone)]
pub struct RecentCommands {
    /// Where the list is kept, or `None` on a machine with nowhere to keep it.
    path: Option<PathBuf>,
    held: Arc<Mutex<Vec<AgentCommand>>>,
}

/// The file, as this writes it.
#[derive(Serialize)]
struct Written<'a> {
    format: &'static str,
    version: u32,
    commands: &'a [AgentCommand],
}

/// The file, as this reads it — the same fields, held loosely enough to
/// recognize a file it did not write.
#[derive(Deserialize)]
struct Stored {
    format: String,
    version: u32,
    #[serde(default)]
    commands: Vec<AgentCommand>,
}

impl RecentCommands {
    /// How many invocations are kept.
    ///
    /// Ten because the list sits under the form it refills: long enough that the
    /// two or three agents somebody is switching between are always in it, short
    /// enough that it stays a strip beside a form rather than something to
    /// search. A list you have to scroll is the catalog this is deliberately not
    /// (§1.1).
    pub const CAPACITY: usize = 10;

    /// The list as the last run of the app left it, from
    /// [`location`](Self::location).
    pub fn load() -> Self {
        match Self::location() {
            Some(path) => Self::load_from(path),
            // Nowhere to keep it: the list still works for this run, and saving
            // says why it will not outlive it.
            None => Self {
                path: None,
                held: Arc::default(),
            },
        }
    }

    /// The list kept in a particular file — what [`load`](Self::load) does once
    /// it knows where, and what the tests drive.
    pub fn load_from(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let held = read(&path).unwrap_or_default();
        Self {
            path: Some(path),
            held: Arc::new(Mutex::new(held)),
        }
    }

    /// Where the list lives when nobody says otherwise: the platform's own
    /// directory for a program's state, under a directory of the inspector's.
    ///
    /// The state directory rather than the config one, because nobody wrote
    /// this and nobody is meant to edit it — it is what the app noticed, not
    /// what the user chose. Platforms that have no such notion (macOS, Windows)
    /// answer with their data directory, which is where a program's own files go
    /// there.
    ///
    /// `None` on a machine with no home directory at all, which is a machine
    /// with nowhere to put it — and is answered by saying so rather than by
    /// picking somewhere, the same way [`AgentCommand::session_cwd`] refuses to
    /// invent a directory.
    ///
    /// [`AgentCommand::session_cwd`]: crate::AgentCommand::session_cwd
    pub fn location() -> Option<PathBuf> {
        kept::location(FILE)
    }

    /// Where this list is kept, or `None` if it has nowhere.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// The remembered invocations, newest first — the order the form offers
    /// them in.
    pub fn entries(&self) -> Vec<AgentCommand> {
        self.lock().clone()
    }

    /// Remembers an invocation, and saves the list.
    ///
    /// **The newest is first, and a relaunch moves rather than repeats.** An
    /// invocation already in the list comes back to the front instead of joining
    /// itself, because relaunching from the list is what the list is for and the
    /// common case must not be the one that fills it with copies. The oldest
    /// goes when there is no room, which is the only way anything leaves.
    ///
    /// **What is remembered is the invocation, not the keystrokes**
    /// ([`AgentCommand::normalized`]): the blank line and the trailing space a
    /// form collects are exactly what the spawn already ignores, so dropping
    /// them costs nothing and keeps two spellings of one command from becoming
    /// two rows that launch the same agent.
    ///
    /// Answers with whether the list reached the disk. The entry is in the list
    /// regardless — see the type's note on why.
    ///
    /// **The file is written under the list's own lock**, so two launches
    /// recorded at once cannot reach the disk in the other order from the one
    /// they reached the list in — which would leave the file describing a
    /// moment that never happened, and outlive the window saying it.
    pub fn record(&self, command: &AgentCommand) -> io::Result<()> {
        let command = command.normalized();
        let mut held = self.lock();

        held.retain(|remembered| *remembered != command);
        held.insert(0, command);
        held.truncate(Self::CAPACITY);

        self.save(&held)
    }

    /// Writes the list where it lives — beside and moved into place, so a save
    /// interrupted partway (the window quit, the disk filled) leaves either the
    /// old list or the new one, never half of a file that would read as no list
    /// at all and take the other nine entries with it. Nothing is flushed to the
    /// platter: this is a convenience, and a list that survives a crash but not a
    /// power cut is the trade the export makes too.
    ///
    /// On Unix the file is the user's own to read (`0600`), for the reason the
    /// export is: the environment field of a remembered invocation is where an
    /// API key would be.
    fn save(&self, commands: &[AgentCommand]) -> io::Result<()> {
        let path = self
            .path
            .as_deref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, NOWHERE))?;

        let document = serde_json::to_string(&Written {
            format: FORMAT,
            version: VERSION,
            commands,
        })
        .map_err(io::Error::other)?;

        kept::replace(path, &document, Access::Private)
    }

    fn lock(&self) -> MutexGuard<'_, Vec<AgentCommand>> {
        locked(&self.held)
    }
}

/// The list a file holds, or `None` if it holds anything else.
///
/// A missing file is the first run of the app; a file that will not parse, or
/// says it is something else, is a file this cannot use — and all three are the
/// same answer, because none of them is a reason a form should not open.
///
/// What comes in is held to what [`record`](RecentCommands::record) would have
/// written: trimmed to the bound, so a hand-enlarged file does not become a list
/// the form has to scroll, and normalized, so an entry somebody typed into the
/// file is the same value as the one relaunching it would record — otherwise the
/// relaunch would sit beside its own copy.
fn read(path: &Path) -> Option<Vec<AgentCommand>> {
    let document = fs::read_to_string(path).ok()?;
    let file: Stored = serde_json::from_str(&document).ok()?;
    (file.format == FORMAT && file.version == VERSION).then(|| {
        let mut commands: Vec<_> = file.commands.iter().map(AgentCommand::normalized).collect();
        commands.truncate(RecentCommands::CAPACITY);
        commands
    })
}
