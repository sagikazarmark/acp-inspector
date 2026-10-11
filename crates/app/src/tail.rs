//! The way back to the newest row of an evidence list.
//!
//! The lists are laid out bottom-up and never auto-scroll; once a list is not
//! at its tail it shows a control that returns there. [`WATCH`] marks each list
//! with whether it is at its tail and the stylesheet decides what that looks
//! like, so scrolling writes one attribute and re-renders nothing.

use dioxus::prelude::*;

/// Marks every tailing list with whether it is at its tail.
///
/// One capture-phase listener, because lists mount and unmount and `scroll`
/// does not bubble. A `column-reverse` scroller reads `0` at its tail but counts
/// away with an engine-dependent sign, so only the distance is checked.
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

/// Sends a list back to its newest row.
///
/// Paging is reset first, and the navigation bridge waits for that page to
/// render before scrolling. The scroll fires its own event, which updates
/// [`WATCH`]'s mark.
pub fn latest(list: &'static str, anchor: Option<Signal<Option<u64>>>) {
    if let Some(mut anchor) = anchor {
        anchor.set(None);
    }
    crate::navigation::Destination::Latest { list }.focus();
}

/// The control, positioned against the box holding the list rather than inside
/// it: inside the scroller it would take row styles, join the flex layout and
/// add scrollable overflow. Hidden, it is also out of the tab order.
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
        // List names are quoted into an `eval`, so they must be plain names.
        for list in [crate::timeline::ENTRIES, crate::trace::FRAMES] {
            assert!(
                list.chars().all(|character| character.is_ascii_lowercase()),
                "{list} is a plain name"
            );
        }
    }
}
