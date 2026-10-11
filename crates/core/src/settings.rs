//! What the window remembers about how it should look (`docs/architecture.md`
//! §9): the appearance and indentation preferences, in a file of the
//! inspector's own.
//!
//! Read once at start, written on every change so a crash loses nothing. A save
//! writes both preferences; a missing field reads as its default, so adding a
//! preference needs no version bump. A failed save is silent: the choice still
//! applies for the session. An unreadable file means [`Appearance::System`] and
//! [`Indentation::Wire`]. File location and atomic replace live in
//! `crates/core/src/kept.rs`.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

use crate::appearance::Appearance;
use crate::indentation::Indentation;
use crate::kept::{self, Access};
use crate::store::locked;

/// What the file calls itself, so a file that is something else can be told
/// apart from one this wrote.
const FORMAT: &str = "acp-inspector-settings";

/// The shape's version. Bumped when an older file would be misread; older
/// files are then ignored.
const VERSION: u32 = 1;

/// The file itself.
const FILE: &str = "settings.json";

/// What is said when there is nowhere on this machine to keep the preference.
const NOWHERE: &str = "no directory to remember settings in";

/// The window's preferences, as the last run of the app left them.
///
/// A handle: clones share one set of values.
#[derive(Clone)]
pub struct Settings {
    /// Where they are kept, or `None` on a machine with nowhere to keep them.
    path: Option<PathBuf>,
    held: Arc<Mutex<Preferences>>,
    backup: Option<PathBuf>,
}

/// Everything the window remembers, which is what a save writes and what a load
/// answers with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Preferences {
    appearance: Appearance,
    indentation: Indentation,
}

/// The file, as this writes it.
#[derive(Serialize)]
struct Written {
    format: &'static str,
    version: u32,
    appearance: Appearance,
    indentation: Indentation,
}

/// The file, as this reads it — the same fields, held loosely enough to
/// recognize a file it did not write.
#[derive(Deserialize)]
struct Stored {
    format: String,
    version: u32,
    /// An absent field is System. An unknown token fails the whole parse, which
    /// `read` also answers as System (`crates/core/tests/appearance.rs`).
    #[serde(default)]
    appearance: Appearance,
    /// An absent field is Wire, so files from before this preference still load.
    #[serde(default)]
    indentation: Indentation,
}

impl Settings {
    /// The preferences as the last run of the app left them, from
    /// [`location`](Self::location).
    pub fn load() -> Self {
        match Self::location() {
            Some(path) => Self::load_from(path),
            // Nowhere to keep them: the choice still applies for this run, and
            // saving says why it will not outlive it.
            None => Self {
                path: None,
                held: Arc::default(),
                backup: None,
            },
        }
    }

    /// The preferences kept in a particular file.
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

    /// The default location: the same directory as
    /// [`RecentCommands::location`], in a file of its own. `None` when there is
    /// no home directory.
    ///
    /// [`RecentCommands::location`]: crate::RecentCommands::location
    pub fn location() -> Option<PathBuf> {
        kept::location(FILE)
    }

    /// Where these preferences are kept, or `None` if they have nowhere.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// The colour scheme the window was asked to present in.
    pub fn appearance(&self) -> Appearance {
        self.lock().appearance
    }

    /// Whether the window was asked to draw payloads indented (§8).
    pub fn indentation(&self) -> Indentation {
        self.lock().indentation
    }

    /// Remembers a choice of appearance, and saves it. The choice applies even
    /// if the save fails.
    ///
    /// The file is written under the lock, so concurrent changes reach the disk
    /// in the order they were made.
    pub fn set_appearance(&self, appearance: Appearance) -> io::Result<()> {
        let mut held = self.lock();
        held.appearance = appearance;
        self.save(*held)
    }

    /// Remembers a choice of indentation, and saves it, under the same rules as
    /// [`set_appearance`](Self::set_appearance).
    pub fn set_indentation(&self, indentation: Indentation) -> io::Result<()> {
        let mut held = self.lock();
        held.indentation = indentation;
        self.save(*held)
    }

    /// Writes the preferences where they live, with ordinary permissions:
    /// nothing in them is secret.
    fn save(&self, preferences: Preferences) -> io::Result<()> {
        let path = self
            .path
            .as_deref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, NOWHERE))?;

        let document = serde_json::to_string(&Written {
            format: FORMAT,
            version: VERSION,
            appearance: preferences.appearance,
            indentation: preferences.indentation,
        })
        .map_err(io::Error::other)?;

        kept::replace(path, &document, Access::Ordinary)
    }

    fn lock(&self) -> MutexGuard<'_, Preferences> {
        locked(&self.held)
    }
}

/// The preferences a file holds, or `None` if it is missing, unparsable,
/// something else, or another version. An unusable file is set aside as `.bak`
/// rather than overwritten.
fn read(path: &Path) -> (Option<Preferences>, Option<PathBuf>) {
    kept::load(path, parse)
}

fn parse(document: &str) -> Option<Preferences> {
    let file: Stored = serde_json::from_str(document).ok()?;
    (file.format == FORMAT && file.version == VERSION).then_some(Preferences {
        appearance: file.appearance,
        indentation: file.indentation,
    })
}
