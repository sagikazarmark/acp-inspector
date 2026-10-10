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
    backup: Option<PathBuf>,
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
                backup: None,
            },
        }
    }

    /// The list kept in a particular file — what [`load`](Self::load) does once
    /// it knows where, and what the tests drive.
    pub fn load_from(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let (held, backup) = read(&path);
        Self {
            path: Some(path),
            held: Arc::new(Mutex::new(held.unwrap_or_default())),
            backup,
        }
    }

    /// Where loading set aside a file it could not read, if it did.
    pub fn backup(&self) -> Option<&Path> {
        self.backup.as_deref()
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
    /// goes when there is no room.
    ///
    /// **What is remembered is the invocation, not the keystrokes**
    /// ([`AgentCommand::remembered`]): the blank line and the trailing space a
    /// form collects are exactly what the spawn already ignores, and the values
    /// of its variables are kept only when the launch asked for that
    /// ([`AgentCommand::remember_env_values`]) — the file is plaintext, and a
    /// value is where an API key would be. Two launches of one invocation that
    /// differ only in its variables' values are one row, the newer.
    ///
    /// Answers with whether the list reached the disk. The entry is in the list
    /// regardless — see the type's note on why.
    ///
    /// **The file is written under the list's own lock**, so two launches
    /// recorded at once cannot reach the disk in the other order from the one
    /// they reached the list in.
    pub fn record(&self, command: &AgentCommand) -> io::Result<()> {
        let command = command.remembered();
        let same = command.without_env_values();
        let mut held = self.lock();

        held.retain(|remembered| remembered.without_env_values() != same);
        held.insert(0, command);
        held.truncate(Self::CAPACITY);

        self.save(&held)
    }

    /// Forgets an entry — the one [`entries`](Self::entries) answered with —
    /// and saves the list. An entry that is not there leaves the file alone.
    pub fn remove(&self, command: &AgentCommand) -> io::Result<()> {
        let mut held = self.lock();
        let before = held.len();
        held.retain(|remembered| remembered != command);
        if held.len() == before {
            return Ok(());
        }
        self.save(&held)
    }

    /// Takes the variables' values out of an entry, keeping their names, and
    /// saves the list — for entries written before values stopped being kept
    /// by default, and for any launch that chose to keep them.
    pub fn forget_values(&self, command: &AgentCommand) -> io::Result<()> {
        let mut held = self.lock();
        let Some(entry) = held.iter_mut().find(|remembered| *remembered == command) else {
            return Ok(());
        };
        *entry = entry.without_env_values();
        self.save(&held)
    }

    /// Writes the list where it lives — beside and moved into place, so a save
    /// interrupted partway (the window quit, the disk filled) leaves either the
    /// old list or the new one, never half of a file that would read as no list
    /// at all and take the other nine entries with it. The new file is synced
    /// before the move (`kept::replace`), so a remove is as durable as a record.
    ///
    /// On Unix the file is the user's own to read (`0600`), for the reason the
    /// export is: a launch that chose to keep its variables' values keeps them
    /// here, and a value is where an API key would be.
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
/// same answer, because none of them is a reason a form should not open. An
/// unusable file is set aside as `.bak` rather than overwritten.
///
/// What comes in is trimmed to the bound, so a hand-enlarged file does not
/// become a list the form has to scroll, and normalized, so an entry somebody
/// typed into the file is the same value as the one relaunching it would
/// record. Values already in the file are kept: a file written before values
/// stopped being kept by default still refills its forms, and dropping them is
/// the reader's choice ([`forget_values`](RecentCommands::forget_values)).
fn read(path: &Path) -> (Option<Vec<AgentCommand>>, Option<PathBuf>) {
    kept::load(path, parse)
}

fn parse(document: &str) -> Option<Vec<AgentCommand>> {
    let file: Stored = serde_json::from_str(document).ok()?;
    (file.format == FORMAT && file.version == VERSION).then(|| {
        let mut commands: Vec<_> = file
            .commands
            .iter()
            .map(|command| {
                let mut command = command.normalized();
                // An entry that holds values keeps them: the flag says so, so a
                // relaunch from it does not drop them unasked. Only files written
                // before values stopped being kept by default have this shape.
                command.remember_env_values |= command.has_env_values();
                command
            })
            .collect();
        commands.truncate(RecentCommands::CAPACITY);
        commands
    })
}
