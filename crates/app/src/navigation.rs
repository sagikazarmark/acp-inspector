//! Window-session navigation shared by the toolbar, palette, menu and evidence links.

use dioxus::prelude::*;

use crate::{Focus, Spine, console::Tab, rail};

#[derive(Clone, Copy)]
pub(crate) struct Navigation {
    pub spine: Signal<Spine>,
    pub region: Signal<Focus>,
    pub console: Signal<Tab>,
    pub rail: Signal<rail::Tab>,
    pub revealed: Signal<Vec<u64>>,
    pub sought: Signal<Option<u64>>,
}

pub(crate) fn use_navigation() -> Navigation {
    Navigation {
        spine: use_signal(Spine::default),
        region: use_signal(Focus::default),
        console: use_signal(Tab::default),
        rail: use_signal(rail::Tab::default),
        revealed: use_signal(Vec::new),
        sought: use_signal(|| None),
    }
}

#[derive(serde::Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub(crate) enum Destination {
    Control { id: &'static str },
    Entry { ordinal: u64 },
    Frames { ordinals: Vec<u64> },
    Layout,
    Cancel,
    Latest { list: &'static str },
}

impl Destination {
    pub(crate) fn focus(self) {
        // Start from the handler; scripts wait for the destination's paint.
        // This also handles repeated navigation to an already-selected row.
        let focusing = document::eval(include_str!("navigation.js"));
        let _ = focusing.send(self);
    }
}

impl Navigation {
    /// A layout choice is not a request to move the keyboard. Preserve it
    /// unless the chosen layout actually hides the focused control.
    pub fn layout(mut self, chosen: Spine) {
        if chosen == Spine::Wire {
            Destination::Layout
        } else {
            Destination::Cancel
        }
        .focus();
        self.spine.set(chosen);
        // Full wire must also be Messages at narrow widths. Split restores
        // space without changing which narrow region the reader was using.
        if chosen == Spine::Wire {
            self.region.set(Focus::Wire);
        }
    }

    pub fn region(mut self, chosen: Focus) {
        if chosen == Focus::Turn {
            self.spine.set(Spine::Split);
        }
        self.region.set(chosen);
        Destination::Control {
            id: match chosen {
                Focus::Turn => "timeline",
                Focus::Wire => (self.console)().id(),
                Focus::Rail => (self.rail)().id(),
            },
        }
        .focus();
    }

    pub fn console(mut self, chosen: Tab) {
        self.region.set(Focus::Wire);
        self.console.set(chosen);
        Destination::Control { id: chosen.id() }.focus();
    }

    pub fn rail(mut self, chosen: rail::Tab) {
        self.region.set(Focus::Rail);
        self.rail.set(chosen);
        Destination::Control { id: chosen.id() }.focus();
    }

    pub fn reveal(mut self, frames: Vec<u64>) {
        self.region.set(Focus::Wire);
        self.console.set(Tab::Trace);
        self.revealed.set(frames.clone());
        self.sought.set(None);
        Destination::Frames { ordinals: frames }.focus();
    }

    pub fn seek(mut self, ordinal: u64) {
        self.spine.set(Spine::Split);
        self.region.set(Focus::Turn);
        self.sought.set(Some(ordinal));
        self.revealed.set(Vec::new());
        Destination::Entry { ordinal }.focus();
    }
}
