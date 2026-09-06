//! What the window remembers about how it should look (`docs/architecture.md`
//! §9): the appearance and the indentation preferences, in a file of the
//! inspector's own.
//!
//! **Two preferences, and a file rather than a panel.** Both choices are offered
//! by controls that sit in the bar the window already has, and this is only where
//! the answers are kept — read once at the window's start, written on every
//! change, because a preference that needs a clean exit to persist loses to a
//! crash.
//!
//! **One file, and every preference in it.** A save writes the pair, so a choice
//! made about one is never a way of forgetting the other; the file's version
//! covers the shape rather than the field list, which is why a preference joining
//! it costs no version — a file written before there was an indentation
//! preference names none, and a file that names none is a window drawing the
//! wire.
//!
//! It is beside [`RecentCommands`](crate::RecentCommands) rather than inside it:
//! that file is `0600` because it carries environment variables, and coupling a
//! convenience list's version to a preference's helps neither. What the two do
//! share — where a file of the inspector's lives, and the write-beside-and-move
//! that replaces one — is stated once, in `crates/core/src/kept.rs`.
//!
//! **A save that fails is silent.** The choice still applies for the session —
//! the same shape recording a recent command has, where the entry is in the list
//! whether or not the file was written. Unlike a list of typed invocations,
//! either of these is one click to redo, so they earn no note on screen. Anything
//! this cannot read is [`Appearance::System`] and [`Indentation::Wire`], which
//! are the defaults a window with no preferences at all would open on.

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

/// The shape's version. It changes when this would read an older file wrongly —
/// at which point the older file is ignored, which leaves the window on System
/// and costs one click to put right.
const VERSION: u32 = 1;

/// The file itself.
const FILE: &str = "settings.json";

/// What is said when there is nowhere on this machine to keep the preference.
const NOWHERE: &str = "no directory to remember settings in";

/// The window's preferences, as the last run of the app left them.
///
/// A handle, not the values: cloning shares one, so the window can hand it to
/// the effect that saves into it without threading a reference through the tree
/// — the same shape [`RecentCommands`](crate::RecentCommands) has, for the same
/// reason.
#[derive(Clone)]
pub struct Settings {
    /// Where they are kept, or `None` on a machine with nowhere to keep them.
    path: Option<PathBuf>,
    held: Arc<Mutex<Preferences>>,
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
    /// A file of this version that names no appearance at all is System.
    ///
    /// This covers the *absent* field only. A field holding a token no version
    /// of this has heard of fails the whole parse instead, which `read` below
    /// answers as System too — same outcome by a different road, and
    /// `crates/core/tests/appearance.rs` pins both.
    #[serde(default)]
    appearance: Appearance,
    /// And the same for the indentation, which is what makes a file written
    /// before this preference existed a readable file rather than a version to
    /// bump: it names none, and naming none is Wire.
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
            },
        }
    }

    /// The preferences kept in a particular file — what [`load`](Self::load)
    /// does once it knows where, and what the tests drive.
    pub fn load_from(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let held = read(&path).unwrap_or_default();
        Self {
            path: Some(path),
            held: Arc::new(Mutex::new(held)),
        }
    }

    /// Where the preferences live when nobody says otherwise: the platform's own
    /// directory for a program's state, under a directory of the inspector's —
    /// the same place [`RecentCommands::location`] answers with, in a file of
    /// this store's own.
    ///
    /// The state directory rather than the config one, because nobody wrote this
    /// and nobody is meant to edit it: it is a click the window recorded, not a
    /// file somebody is expected to open. `None` on a machine with no home
    /// directory at all, which is answered by saying so rather than by picking
    /// somewhere.
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

    /// Remembers a choice of appearance, and saves it.
    ///
    /// Answers with whether it reached the disk. The choice is in effect
    /// regardless — see the module's note on why nothing is said about a save
    /// that failed.
    ///
    /// **The file is written under the preferences' own lock**, so two changes
    /// made at once cannot reach the disk in the other order from the one they
    /// were made in — which would outlive the window saying the wrong one was
    /// last.
    pub fn set_appearance(&self, appearance: Appearance) -> io::Result<()> {
        let mut held = self.lock();
        held.appearance = appearance;
        self.save(*held)
    }

    /// Remembers a choice of indentation, and saves it. The same rules as the
    /// appearance's, including the silence about a save that did not land.
    pub fn set_indentation(&self, indentation: Indentation) -> io::Result<()> {
        let mut held = self.lock();
        held.indentation = indentation;
        self.save(*held)
    }

    /// Writes the preferences where they live.
    ///
    /// Ordinary permissions, unlike the recent commands': a colour scheme is not
    /// a secret, and a file that is `0600` for no reason invites the next
    /// preference to be kept somewhere it does not belong.
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

/// The preferences a file holds, or `None` if it holds anything else.
///
/// A missing file is the first run of the app; a file that will not parse, says
/// it is something else, or comes from a version whose shape this cannot read,
/// is a file this cannot use — and all four are the same answer, because none of
/// them is a reason a window should not open: the operating system has an answer
/// for how the window should look, and the wire is an answer for how a payload
/// is drawn.
fn read(path: &Path) -> Option<Preferences> {
    let document = std::fs::read_to_string(path).ok()?;
    let file: Stored = serde_json::from_str(&document).ok()?;
    (file.format == FORMAT && file.version == VERSION).then_some(Preferences {
        appearance: file.appearance,
        indentation: file.indentation,
    })
}
