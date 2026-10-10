//! What the user asked the window to look like (`docs/architecture.md` §9).
//!
//! Three values and no runtime detection: `System` is the *absence* of a mark on
//! the document element, and the stylesheet answers `prefers-color-scheme`.
//! Explicit choices are mapped to presentation theme names by the desktop crate;
//! the stable token here is what the settings file and native control exchange.

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

    /// Parses a token back, falling back to `System` for anything unrecognized
    /// (e.g. a value from a future version).
    pub fn from_token(token: &str) -> Self {
        match token {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::System,
        }
    }
}
