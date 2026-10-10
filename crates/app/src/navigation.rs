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
    /// Whether the rail is open as a drawer over the screens, which is how a
    /// window too narrow for three columns shows it (D18). Read only by the
    /// stylesheet's narrow layout; at full width the rail is always there.
    pub drawer: Signal<bool>,
}

/// The toolbar's button that opens the rail as a drawer, where focus goes back
/// to when the drawer closes.
pub(crate) const DRAWER_TOGGLE: &str = "rail-toggle";
/// The rail itself, where focus goes when the drawer opens on a rail with no
/// tabs to land on.
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

    /// A rail tab asked for from somewhere other than the rail (the palette,
    /// the menu): the tab, and the drawer open where the rail is one.
    pub fn show_rail(mut self, chosen: rail::Tab) {
        self.drawer.set(true);
        self.rail(chosen);
    }

    /// Opens or closes the rail's drawer from the toolbar's button (D18).
    /// Opening moves focus into the rail — onto its selected tab, or the rail
    /// itself when it has none — and closing hands it back to the button.
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

    /// Closes the drawer — Escape inside it, or a press on its backdrop — and
    /// hands focus back to the button that opened it.
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

    /// The request the agent is waiting on, shown and focused without marking
    /// it as sought: it is not a frame the reader followed here.
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

    /// Clear asked for from anywhere but the Console's own button (the
    /// palette, the menu, the shortcut): the Trace is shown and its Clear is
    /// armed and focused, so the same second press confirms it (I2, F9).
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
