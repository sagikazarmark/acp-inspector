//! What the spawn form remembers: the last few invocations that worked, so
//! relaunching one is a click (`docs/architecture.md` §15 q6).
//!
//! A convenience, not a catalog: a short most-recent-first list in one file
//! under the platform's state directory. The file is the inspector's own and
//! not a contract; it carries a format tag and version only so a later shape
//! can ignore an older file. A file it cannot read is treated as no list.

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

/// The shape's version. Bumped when an older file would be misread; older
/// files are then ignored.
const VERSION: u32 = 1;

/// The file itself.
const FILE: &str = "recent.json";

/// What is said when there is nowhere on this machine to keep the list.
const NOWHERE: &str = "no directory to remember commands in";

/// The invocations that recently worked, newest first.
///
/// A handle: clones share one list. The in-memory list is updated even when
/// saving fails; the save result tells the caller it will not persist.
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

    /// The list kept in a particular file.
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

    /// The default location: the platform's state directory (data directory on
    /// macOS and Windows), under the inspector's own directory. `None` when
    /// there is no home directory.
    pub fn location() -> Option<PathBuf> {
        kept::location(FILE)
    }

    /// Where this list is kept, or `None` if it has nowhere.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// The remembered invocations, newest first.
    pub fn entries(&self) -> Vec<AgentCommand> {
        self.lock().clone()
    }

    /// Remembers an invocation at the front, and saves the list.
    ///
    /// An invocation already present (ignoring variable values) moves to the
    /// front rather than repeating; the oldest is dropped past
    /// [`CAPACITY`](Self::CAPACITY). What is stored is
    /// [`AgentCommand::remembered`], so variable values are kept only when the
    /// launch asked for that. The entry stays in the list even if saving fails.
    ///
    /// The file is written under the list's lock, so concurrent records reach
    /// the disk in the order they reached the list.
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
    /// saves the list.
    pub fn forget_values(&self, command: &AgentCommand) -> io::Result<()> {
        let mut held = self.lock();
        let Some(entry) = held.iter_mut().find(|remembered| *remembered == command) else {
            return Ok(());
        };
        *entry = entry.without_env_values();
        self.save(&held)
    }

    /// Writes the list atomically (write beside, sync, rename via
    /// `kept::replace`), so an interrupted save leaves the old or the new list.
    /// On Unix the file is `0600`: it may hold variable values such as API keys.
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

/// The list a file holds, or `None` if it is missing, unparsable or something
/// else. An unusable file is set aside as `.bak` rather than overwritten.
///
/// Entries are normalized and trimmed to [`RecentCommands::CAPACITY`]; values
/// already in the file are kept.
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
                // Files written before values stopped being kept by default hold
                // values without the flag; set it so a relaunch keeps them.
                command.remember_env_values |= command.has_env_values();
                command
            })
            .collect();
        commands.truncate(RecentCommands::CAPACITY);
        commands
    })
}
