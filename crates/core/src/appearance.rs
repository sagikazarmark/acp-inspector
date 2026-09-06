//! What the user asked the window to look like (`docs/architecture.md` §9).
//!
//! Three values and no runtime detection: `System` is the *absence* of a mark on
//! the document element, and the stylesheet answers `prefers-color-scheme`.
//! Explicit choices are mapped to presentation theme names by the desktop crate;
//! the stable token here is what the settings file and native control exchange.
//!
//! It is in core for the reason [`RecentCommands`](crate::RecentCommands) is
//! (§5): what is stored, what an unknown value falls back to, and what a broken
//! file means are decisions testable without a window
//! (`crates/core/tests/appearance.rs`), and the desktop crate paints what this answers.
//! Core's "no UI types" rule bars Dioxus elements and components, not a
//! three-valued enum that serializes to a file.

use serde::{Deserialize, Serialize};

/// The colour scheme the window presents in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    /// Follow the operating system — the default, and the only value that
    /// leaves the document element unmarked.
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    /// Every choice, in the order the control offers them.
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    /// The stable token stored on disk and exchanged by the Appearance control.
    pub fn token(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    /// The label the control shows.
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    /// Parses a token back, falling back to `System` for anything unrecognized.
    ///
    /// A value written by a future version must never leave the window
    /// unpainted: an appearance nobody can name is still an appearance the
    /// operating system has an answer for.
    pub fn from_token(token: &str) -> Self {
        match token {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::System,
        }
    }
}
