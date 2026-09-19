//! Window-session navigation shared by the toolbar, palette, menu and evidence links.

use dioxus::prelude::*;

use crate::{Focus, Spine, console::Tab, rail, timeline, trace};

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

enum Destination {
    Control(&'static str),
    Entry(u64),
    Frames(Vec<u64>),
}

impl Destination {
    fn focus(self) {
        // Start from the handler; scripts wait for the destination's paint.
        // This also handles repeated navigation to an already-selected row.
        match self {
            Self::Control(id) => {
                let focusing = document::eval(FOCUS_DESTINATION);
                let _ = focusing.send(id);
            }
            Self::Entry(ordinal) => timeline::focus_entry(ordinal),
            Self::Frames(frames) => trace::focus_frames(frames),
        }
    }
}

const FOCUS_DESTINATION: &str = r#"
const id = await dioxus.recv();
for (let attempt = 0; attempt < 60; attempt += 1) {
  await new Promise((resolve) => requestAnimationFrame(resolve));
  const target = document.getElementById(id);
  if (target?.getClientRects().length && target.getAttribute('aria-selected') !== 'false'
      && !document.querySelector('dialog[open]')) {
    target.focus({ preventScroll: true });
    break;
  }
}
"#;

// Layout controls keep their focus. Only a keyboard user whose current region
// is being hidden (e.g. by the native layout accelerator) needs a handoff.
const PRESERVE_LAYOUT_FOCUS: &str = r#"
const active = document.activeElement;
for (let attempt = 0; attempt < 60; attempt += 1) {
  await new Promise((resolve) => requestAnimationFrame(resolve));
  if (document.querySelector('.spine-wire')) break;
}
if (active?.closest('.turns, .rail') && !active.getClientRects().length
    && !document.querySelector('dialog[open]')) {
  document.querySelector('.wire-tabs [aria-selected="true"]')?.focus();
}
"#;

impl Navigation {
    /// A layout choice is not a request to move the keyboard. Preserve it
    /// unless the chosen layout actually hides the focused control.
    pub fn layout(mut self, chosen: Spine) {
        if chosen == Spine::Wire {
            document::eval(PRESERVE_LAYOUT_FOCUS);
        }
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
        Destination::Control(match chosen {
            Focus::Turn => "timeline",
            Focus::Wire => (self.console)().id(),
            Focus::Rail => (self.rail)().id(),
        })
        .focus();
    }

    pub fn console(mut self, chosen: Tab) {
        self.region.set(Focus::Wire);
        self.console.set(chosen);
        Destination::Control(chosen.id()).focus();
    }

    pub fn rail(mut self, chosen: rail::Tab) {
        self.region.set(Focus::Rail);
        self.rail.set(chosen);
        Destination::Control(chosen.id()).focus();
    }

    pub fn reveal(mut self, frames: Vec<u64>) {
        self.region.set(Focus::Wire);
        self.console.set(Tab::Trace);
        self.revealed.set(frames.clone());
        self.sought.set(None);
        Destination::Frames(frames).focus();
    }

    pub fn seek(mut self, ordinal: u64) {
        self.spine.set(Spine::Split);
        self.region.set(Focus::Turn);
        self.sought.set(Some(ordinal));
        self.revealed.set(Vec::new());
        Destination::Entry(ordinal).focus();
    }
}
