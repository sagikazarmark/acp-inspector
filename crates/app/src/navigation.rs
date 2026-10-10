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
    /// Whether the rail is open as a drawer, as it is on narrow windows. Only the
    /// narrow layout reads it.
    pub drawer: Signal<bool>,
}

/// The toolbar button that opens the drawer; focus returns here on close.
pub(crate) const DRAWER_TOGGLE: &str = "rail-toggle";
/// The rail, focused when the drawer opens on a rail with no tabs.
pub(crate) const RAIL: &str = "rail";

pub(crate) fn use_navigation() -> Navigation {
    Navigation {
        spine: use_signal(Spine::default),
        region: use_signal(Focus::default),
        console: use_signal(Tab::default),
        rail: use_signal(rail::Tab::default),
        revealed: use_signal(Vec::new),
        sought: use_signal(|| None),
        drawer: use_signal(|| false),
    }
}

#[derive(serde::Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub(crate) enum Destination {
    Control {
        id: &'static str,
    },
    Entry {
        ordinal: u64,
    },
    Frames {
        ordinals: Vec<u64>,
    },
    Layout,
    Cancel,
    Latest {
        list: &'static str,
    },
    /// The composer's prompt field, once it is enabled.
    Prompt,
    /// The Console's Clear, armed for its confirming second press.
    ArmClear,
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
        self.drawer.set(chosen == Focus::Rail);
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
        self.drawer.set(false);
        self.console.set(chosen);
        Destination::Control { id: chosen.id() }.focus();
    }

    pub fn rail(mut self, chosen: rail::Tab) {
        self.region.set(Focus::Rail);
        self.rail.set(chosen);
        Destination::Control { id: chosen.id() }.focus();
    }

    /// Shows a rail tab from outside the rail, opening the drawer.
    pub fn show_rail(mut self, chosen: rail::Tab) {
        self.drawer.set(true);
        self.rail(chosen);
    }

    /// Toggles the drawer. Opening focuses the selected rail tab (or the rail);
    /// closing focuses the toolbar button.
    pub fn toggle_drawer(mut self, tabbed: bool) {
        let open = !(self.drawer)();
        self.drawer.set(open);
        Destination::Control {
            id: match (open, tabbed) {
                (true, true) => (self.rail)().id(),
                (true, false) => RAIL,
                (false, _) => DRAWER_TOGGLE,
            },
        }
        .focus();
    }

    /// Closes the drawer and returns focus to the toolbar button.
    pub fn close_drawer(mut self) {
        if (self.drawer)() {
            self.drawer.set(false);
            Destination::Control { id: DRAWER_TOGGLE }.focus();
        }
    }

    pub fn reveal(mut self, frames: Vec<u64>) {
        self.region.set(Focus::Wire);
        self.drawer.set(false);
        self.console.set(Tab::Trace);
        self.revealed.set(frames.clone());
        self.sought.set(None);
        Destination::Frames { ordinals: frames }.focus();
    }

    /// Shows and focuses the waiting request without marking it as sought.
    pub fn waiting(mut self, ordinal: u64) {
        self.spine.set(Spine::Split);
        self.region.set(Focus::Turn);
        self.drawer.set(false);
        Destination::Entry { ordinal }.focus();
    }

    /// The prompt, which is on the timeline.
    pub fn prompt(mut self) {
        self.spine.set(Spine::Split);
        self.region.set(Focus::Turn);
        self.drawer.set(false);
        Destination::Prompt.focus();
    }

    /// Clear from the palette, menu or shortcut: shows the Trace and arms and
    /// focuses its Clear, so a second press confirms.
    pub fn clear(mut self) {
        self.region.set(Focus::Wire);
        self.drawer.set(false);
        self.console.set(Tab::Trace);
        Destination::ArmClear.focus();
    }

    pub fn seek(mut self, ordinal: u64) {
        self.spine.set(Spine::Split);
        self.region.set(Focus::Turn);
        self.drawer.set(false);
        self.sought.set(Some(ordinal));
        self.revealed.set(Vec::new());
        Destination::Entry { ordinal }.focus();
    }
}
