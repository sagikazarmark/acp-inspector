//! A word about what the *tool* just did, and then nothing.
//!
//! **The window's one transient voice, and the only kind of statement it is
//! for.** Everything else on these screens is the record — a frame that
//! crossed, a line the agent wrote, what a call came back with — and all of it
//! stays until the trace drops it, because a reader who looked away is owed the
//! evidence when they look back. This says something else: that *this window*
//! did the thing that was asked of it, and did it somewhere the reader cannot
//! see. A clipboard write is the whole of that today ([`crate::copy`]) — the
//! bytes went out of the process, and the only other way to find out whether
//! they arrived is to paste.
//!
//! **Nothing about the agent is ever said here**, and neither is a failure.
//! What went wrong is drawn where the thing that failed is: a refused call on
//! the row it was refused on (§7.5), an export's error under the button that
//! asked for it, an agent that never started on the tab carrying its own
//! reason (§9). A message that takes itself away is a message a reader can
//! miss, and none of the record is allowed to depend on having been watched —
//! which is also why a copy that did not take says nothing at all rather than
//! flashing red here.
//!
//! **One at a time.** A second copy replaces the first rather than piling under
//! it: the latest is the one the reader just asked for, and a stack in the
//! corner is a panel nobody opened, over the densest surface in the tool.

use std::time::Duration;

use dioxus::prelude::*;

/// How long a sentence stays.
///
/// Long enough to be read twice at three words, short enough that a reader who
/// was looking at the row they copied is not still being told about it by the
/// time they look up. It is not adjustable and there is nothing to dismiss:
/// what this says is never something to act on.
const LINGER: Duration = Duration::from_secs(2);

/// What the window is saying about itself, if anything.
///
/// A handle rather than a store: it is one signal, held by the root and reached
/// from wherever a control has something to say.
#[derive(Clone, Copy, PartialEq)]
pub struct Announcer(Signal<Option<String>>);

impl Announcer {
    /// Says one thing, which takes the place of whatever was being said.
    ///
    /// The clock starts here, and it starts *again* on the next call: a reader
    /// copying three rows in a row sees the third for its full time rather than
    /// for what was left of the first's.
    pub fn say(&self, what: String) {
        // Callable from an event handler, an async task, anywhere — a write is
        // not a subscription, so nothing that clicks becomes a subscriber to
        // every announcement the window ever makes.
        let mut said = self.0;
        said.set(Some(what));
    }

    /// What is on screen, for the one component that draws it.
    pub fn said(&self) -> Option<String> {
        self.0.read().clone()
    }
}

/// Puts the window's announcer where every control under it can find one, and
/// starts the clock that empties it. **Once, at the root.**
///
/// The clock is here rather than in [`Announcer::say`] on purpose: a task
/// spawned by a click belongs to the component that was clicked, and the copy
/// buttons sit on rows that come and go — a trace that is cleared while a note
/// is up would take the clock with the row and leave the sentence on screen for
/// the rest of the session. This one belongs to the window, which outlives
/// every row in it.
///
/// [`use_resource`] restarts its future whenever something it read has changed,
/// which is exactly the clock this needs: reading the note *is* the
/// subscription, so a second announcement drops the first's timer and starts
/// its own.
pub fn provide() -> Announcer {
    let announcer = use_context_provider(|| Announcer(Signal::new(None)));

    use_resource(move || async move {
        // Read and dropped in the one statement — a signal's guard held across
        // an await is a lock held across an await.
        if announcer.0.read().is_none() {
            return;
        }
        tokio::time::sleep(LINGER).await;
        // Which is itself a change, so this future is started once more and
        // returns at the line above. That is the whole of the loop.
        announcer.0.clone().set(None);
    });

    announcer
}

/// The announcer to say something into, from anywhere under the root.
///
/// **Or a detached one, where there is nothing above this component.** The
/// screen tests in this crate render one component with no window around it,
/// and a copy button that panicked for want of somewhere to announce would make
/// them hostage to a corner of the window they are not about. Nobody hears a
/// detached announcer, and the clipboard write happens either way.
pub fn announcer() -> Announcer {
    use_hook(|| try_consume_context().unwrap_or_else(|| Announcer(Signal::new(None))))
}

/// The toast, wherever the window has one.
///
/// **The region is always here, and mostly empty**, which is the rule the
/// composer's live region already states (`timeline.rs`): an announcement is
/// made by text *changing* inside a live region, so one that appeared along
/// with its sentence would announce nothing at all — and this is a message
/// about something that left the window, which is the case where a reader who
/// cannot see the corner has nothing else to go on.
///
/// It draws over the screens rather than in one of them, because the control it
/// answers for is on three of them (§9's density rule: wherever the content is
/// bytes, it can be copied) — and it lets every click through, so a reader
/// copying a second row never finds this in the way of the first.
#[component]
pub fn Toast(said: Option<String>) -> Element {
    rsx! {
        div { class: "toast toast-end toast-bottom", "data-slot": "toasts", role: "status", aria_live: "polite",
            if let Some(said) = said {
                p { class: "alert", "{said}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[component]
    fn Host(said: Option<String>) -> Element {
        rsx! {
            Toast { said }
        }
    }

    fn screen(said: Option<&str>) -> String {
        let mut dom = VirtualDom::new_with_props(
            Host,
            HostProps {
                said: said.map(str::to_owned),
            },
        );
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    #[test]
    fn the_region_is_in_the_document_with_nothing_to_say() {
        // A live region announces text that *changes* inside it. One that
        // arrived along with its first sentence would have that sentence
        // inserted rather than changed, and an insertion is not reliably
        // announced — least of all in a WebView. The composer's `.waiting`
        // region is here for the same reason and was quietly broken by a
        // stylesheet once; this asks the question of the markup.
        let html = screen(None);

        assert!(
            html.contains(r#"aria-live="polite""#) && html.contains(r#"role="status""#),
            "the region is a live region: {html}"
        );
        assert!(
            !html.contains("<p"),
            "and it is drawing nothing at all: {html}"
        );
    }

    #[test]
    fn what_the_window_says_is_in_the_region_that_announces_it() {
        let html = screen(Some("Copied this frame"));

        let region = html
            .split_once(r#"aria-live="polite""#)
            .expect("the live region")
            .1;
        assert!(
            region.contains("Copied this frame"),
            "the sentence is inside the region, not beside it: {html}"
        );
    }

    #[test]
    fn copy_feedback_uses_daisyui_toast_and_notice_presentation() {
        let html = screen(Some("Copied this frame"));

        assert!(
            html.contains(r#"class="toast "#),
            "daisyUI positions the live region as a toast: {html}"
        );
        assert!(
            html.contains(r#"class="alert"#),
            "the confirmation is a daisyUI notice: {html}"
        );
    }
}
