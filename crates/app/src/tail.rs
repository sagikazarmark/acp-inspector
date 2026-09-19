//! The way back to the newest row.
//!
//! **The three evidence lists follow their own tail and stay where they are
//! put.** They are laid out bottom-up, so a surface nobody has touched shows its
//! latest row without a scroll script, and one the reader has scrolled back
//! through stays scrolled back while rows go on arriving — which is the right
//! behaviour and the reason `§9`'s lists have no scroll script at all. What was
//! missing is the other half of it: having scrolled away, the only way back to
//! the tail was to scroll there, on a list that may have grown by a thousand
//! rows in the meantime.
//!
//! So each list carries a control that appears once it is not at its tail.
//! Nothing about the list changes — no auto-scroll, no jump on arrival, no
//! anchoring — and the reader is never moved anywhere they did not ask to go.
//!
//! **Where the scroll position is read is the engine**, because it is the one
//! thing about a scroller only the engine knows. [`WATCH`] marks the list with
//! whether it is at its tail and the stylesheet decides what that looks like; no
//! signal is written, nothing re-renders, and a list scrolled with the wheel
//! costs one attribute write per event.

use dioxus::prelude::*;

/// Mark every tailing list with whether it is at its tail.
///
/// One delegated listener in the capture phase rather than one per list: the
/// lists mount and unmount as tabs are selected and as surfaces empty, and a
/// listener attached per element is a listener to reattach on every one of
/// those. `scroll` does not bubble, which is why this captures.
///
/// **The sign of `scrollTop` is not the same everywhere.** A `column-reverse`
/// scroller reads `0` at its tail in both engines this shell runs on and counts
/// away from it in opposite directions — negative in Blink, positive in older
/// WebKit — so the distance is what is asked about and never the direction.
/// Returning to the tail is `0` either way.
pub const WATCH: &str = r#"
const TAIL = 24;
const mark = (list) => {
  list.dataset.tailed = Math.abs(list.scrollTop) < TAIL ? "true" : "false";
};
document.addEventListener(
  "scroll",
  (event) => {
    const target = event.target;
    if (target instanceof Element && target.matches("[data-tail]")) {
      mark(target);
    }
  },
  true,
);
"#;

/// Send a list back to its newest row.
///
/// Reset paging first, then let the shared navigation bridge wait for that
/// page to render before scrolling. The list name is this crate's own constant,
/// sent as data. Setting the position fires a scroll event of its own, so the mark
/// [`WATCH`] keeps is corrected by the same path that set it.
pub fn latest(list: &'static str, anchor: Option<Signal<Option<u64>>>) {
    if let Some(mut anchor) = anchor {
        anchor.set(None);
    }
    crate::navigation::Destination::Latest { list }.focus();
}

/// The control, beside the list rather than inside it.
///
/// **It was a sticky item in the scroller and that was wrong three ways.** A
/// list item takes the list's own row styling, it takes part in the flex
/// layout — including the auto margin that makes the newest row hold the free
/// space, which it stole by being the first item of its type — and a sticky box
/// inside a scroller contributes to that scroller's scrollable overflow, so
/// there was room to scroll *past* the newest row and the control appeared for
/// having done it.
///
/// Out of the list, none of those exist: it is positioned against the box that
/// holds the list, which is a named ancestor and not the window — the rule #74
/// left behind, kept by giving it something to be positioned against rather
/// than by staying in the flow.
///
/// Hidden, it is hidden from the keyboard too: a control faded to nothing that
/// still takes a tab stop is a focus ring on empty space.
pub fn to_latest(
    list: &'static str,
    what: &'static str,
    anchor: Option<Signal<Option<u64>>>,
) -> Element {
    rsx! {
        div { class: "tail-anchor",
            button {
                class: "to-latest",
                "data-slot": "to-latest",
                r#type: "button",
                title: "Scroll back to the newest {what}",
                aria_label: format!("Newest {what}"),
                onclick: move |_| latest(list, anchor),
                "Newest {what}"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tail_is_read_by_distance_rather_than_by_direction() {
        // The one fact this file depends on and the one it cannot assert
        // against a real engine: a reversed scroller reads zero at its tail and
        // counts away from it with a sign that is the engine's business.
        assert!(
            WATCH.contains("Math.abs(list.scrollTop)"),
            "the direction a reversed scroller counts in is not asked about"
        );
        assert!(
            WATCH.contains(
                r#"addEventListener(
  "scroll","#
            ) && WATCH.contains("true,"),
            "scroll does not bubble, so the one listener captures"
        );
    }

    #[test]
    fn returning_to_the_tail_carries_nothing_the_agent_wrote() {
        // The rule every `eval` in this crate is written under. The list names
        // are this crate's own constants and are quoted into the script.
        for list in [crate::timeline::ENTRIES, crate::trace::FRAMES] {
            assert!(
                list.chars().all(|character| character.is_ascii_lowercase()),
                "{list} is a plain name"
            );
        }
    }
}
