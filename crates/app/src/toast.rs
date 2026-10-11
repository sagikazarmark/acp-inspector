//! A transient note about what the window itself just did, such as a copy.
//!
//! Never about the agent and never a failure: those are drawn where they
//! happened and stay, since a message that disappears can be missed. One note
//! at a time; a new one replaces the last.

use std::time::Duration;

use dioxus::prelude::*;

/// How long a sentence stays. Not adjustable, and nothing to dismiss.
const LINGER: Duration = Duration::from_secs(2);

/// How long a [`notice`](Announcer::notice) stays: long enough to read a path.
const NOTICE: Duration = Duration::from_secs(10);

/// What the window is saying about itself, if anything. One signal held by the
/// root.
#[derive(Clone, Copy, PartialEq)]
pub struct Announcer(Signal<Option<String>>, Signal<Duration>);

impl Announcer {
    /// Says one thing, replacing whatever was being said and restarting the
    /// clock.
    pub fn say(&self, what: String) {
        // A write, not a read, so callers never subscribe to announcements.
        let (mut said, mut linger) = (self.0, self.1);
        linger.set(LINGER);
        said.set(Some(what));
    }

    /// Says something about the window's own state the reader may act on later,
    /// such as a set-aside state file, and keeps it up long enough to read.
    pub fn notice(&self, what: String) {
        let (mut said, mut linger) = (self.0, self.1);
        linger.set(NOTICE);
        said.set(Some(what));
    }

    /// What is on screen, for the one component that draws it.
    pub fn said(&self) -> Option<String> {
        self.0.read().clone()
    }
}

/// Provides the window's announcer and starts the clock that empties it. Once,
/// at the root.
///
/// The clock lives here rather than in [`Announcer::say`] because a task
/// spawned by a click belongs to the clicked component, and copy buttons sit on
/// rows that come and go. [`use_resource`] restarts on each new note, so a
/// second announcement drops the first's timer.
pub fn provide() -> Announcer {
    let announcer = use_context_provider(|| Announcer(Signal::new(None), Signal::new(LINGER)));

    use_resource(move || async move {
        // Read and dropped in one statement: a guard held across an await is a
        // lock held across an await.
        if announcer.0.read().is_none() {
            return;
        }
        let linger = *announcer.1.peek();
        tokio::time::sleep(linger).await;
        // That clear is itself a change, so the future runs once more and
        // returns above.
        announcer.0.clone().set(None);
    });

    announcer
}

/// The announcer, or a detached one where nothing above provides it (screen
/// tests render single components). The clipboard write happens either way.
pub fn announcer() -> Announcer {
    use_hook(|| {
        try_consume_context().unwrap_or_else(|| Announcer(Signal::new(None), Signal::new(LINGER)))
    })
}

/// The toast. The live region is always present and mostly empty, because
/// screen readers announce text that changes inside a region, not a region
/// that appears with its text. It lets every click through.
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
        // A live region that arrived with its first sentence would have it
        // inserted rather than changed, which is not reliably announced
        // (especially in a WebView).
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
