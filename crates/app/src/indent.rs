//! The indentation switch, and the one call every surface drawing payload bytes
//! makes on the way to the screen. See `docs/architecture.md` §8.
//!
//! What indenting does is core's ([`Indentation`]). The choice is provided as
//! context so the Trace, Timeline evidence and tool-call payloads read it
//! without threading. Off is the default, and the clipboard and export always
//! carry the wire's own bytes. Only open disclosures are laid out.
//!
//! Not governed here: an elicitation schema (drawn indented always, since it is
//! re-serialized rather than received) and diffs or embedded text resources
//! (file content, never re-spaced).

use std::collections::HashSet;

use acp_inspector_core::Indentation;
use dioxus::prelude::*;

/// The disclosures the reader has toggled away from the state their markup
/// drew them in. The `<details>` stays the platform's own control; a lost bit
/// only costs a payload its whitespace.
#[derive(Clone, Copy)]
struct Turned(Signal<HashSet<String>>);

/// Provides the window's indentation and the toggled set. Called once, at the
/// root.
pub fn provide(chosen: Indentation) -> Signal<Indentation> {
    use_context_provider(|| Turned(Signal::new(HashSet::new())));
    use_context_provider(|| Signal::new(chosen))
}

/// How this window is drawing payloads, or the wire outside a window (screen
/// tests render without one).
///
/// Not a hook: it is called from row-drawing functions in loops and behind
/// conditions.
pub fn chosen() -> Indentation {
    if !rendering() {
        return Indentation::default();
    }

    try_consume_context::<Signal<Indentation>>()
        .map_or_else(Indentation::default, |chosen| chosen())
}

/// A payload as drawn inside the disclosure `key`, whose markup was drawn
/// `open`: laid out when the switch is on, the payload is JSON and the
/// disclosure is open; the wire's bytes otherwise.
///
/// `key` is window-wide, so each surface spells its own. With the switch off
/// nothing is read from the toggled set, so opening a disclosure redraws
/// nothing.
pub fn drawn(key: &str, open: bool, text: &str) -> String {
    let chosen = chosen();
    if chosen == Indentation::Wire {
        return text.to_owned();
    }
    if !showing(key, open) {
        return text.to_owned();
    }

    chosen.draw(text).into_owned()
}

/// Records that a disclosure was toggled. A no-op outside a window.
pub fn toggled(key: &str) {
    let Some(turned) = turned() else {
        return;
    };
    let mut keys = turned.0;
    keys.with_mut(|turned| {
        if !turned.remove(key) {
            turned.insert(key.to_owned());
        }
    });
}

/// Whether the disclosure called `key`, drawn `open`, is open now.
fn showing(key: &str, open: bool) -> bool {
    let flipped = turned().is_some_and(|turned| turned.0.read().contains(key));
    open != flipped
}

fn turned() -> Option<Turned> {
    rendering().then(try_consume_context::<Turned>).flatten()
}

/// Whether there is a window over this at all. Asked first, because consuming
/// context outside one panics rather than returning `None`.
fn rendering() -> bool {
    dioxus::core::Runtime::try_current()
        .is_some_and(|runtime| runtime.try_current_scope_id().is_some())
}

/// The "Pretty JSON" switch in the bar. The tooltip says what it does not
/// change: the bytes a copy and an export carry.
#[component]
pub fn IndentationControl(chosen: Indentation, on_choose: EventHandler<Indentation>) -> Element {
    let on = chosen.indented();
    rsx! {
        button {
            class: "seg seg-switch indentation",
            "data-slot": "indentation",
            "data-indentation": chosen.token(),
            r#type: "button",
            role: "switch",
            aria_checked: if on { "true" } else { "false" },
            title: "Draw JSON payloads indented. What crossed the wire is unchanged: a copy and an export still carry the exact bytes.",
            onclick: move |_| on_choose.call(Indentation::of(!on)),
            span { class: "seg-item seg-word", "Pretty JSON" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[component]
    fn Host(chosen: Indentation) -> Element {
        rsx! { IndentationControl { chosen, on_choose: |_| {} } }
    }

    fn screen(chosen: Indentation) -> String {
        let mut dom = VirtualDom::new_with_props(Host, HostProps { chosen });
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    #[test]
    fn indentation_is_one_named_switch_beside_the_appearance() {
        let html = screen(Indentation::Wire);

        assert!(
            html.contains(r#"role="switch""#)
                && html.contains(">Pretty JSON<")
                && html.contains("title=\"Draw JSON"),
            "one switch with a plain name and the tooltip that qualifies it: {html}"
        );
        assert!(
            !html.contains(">wire<") && !html.contains(">indent<"),
            "no jargon segments: {html}"
        );
        // A window nobody has switched draws the wire.
        assert!(
            html.contains(r#"aria-checked="false""#),
            "the window opens on the bytes that crossed: {html}"
        );
    }

    #[test]
    fn the_persisted_choice_is_what_the_switch_shows() {
        let html = screen(Indentation::Indented);
        assert!(
            html.contains(r#"data-indentation="indented""#)
                && html.contains(r#"aria-checked="true""#),
            "{html}"
        );
    }

    #[test]
    fn a_surface_with_no_window_over_it_draws_the_wire() {
        // Screen tests render without a window: answer, don't panic.
        let frame = r#"{"jsonrpc":"2.0","id":1}"#;
        assert_eq!(chosen(), Indentation::Wire);
        assert_eq!(drawn("frame 0", true, frame), frame);
        assert_eq!(drawn("frame 0", false, frame), frame);
        // And a click with nothing to remember it is not a panic either.
        toggled("frame 0");
    }

    /// One payload, drawn in a window that is switched on, with the disclosure
    /// it sits in in the state the arguments describe.
    fn payload(open: bool, clicked: bool, text: &str) -> String {
        #[component]
        fn Host(open: bool, clicked: bool, text: String) -> Element {
            provide(Indentation::Indented);
            if clicked {
                toggled("payload");
            }
            let drawn = drawn("payload", open, &text);
            rsx! { pre { "{drawn}" } }
        }

        let mut dom = VirtualDom::new_with_props(
            Host,
            HostProps {
                open,
                clicked,
                text: text.to_owned(),
            },
        );
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);
        html.split_once("<pre>")
            .expect("the payload")
            .1
            .split_once("</pre>")
            .expect("a complete payload")
            .0
            .to_owned()
    }

    #[test]
    fn a_payload_is_laid_out_where_its_disclosure_is_open_and_nowhere_else() {
        let frame = r#"{"id":1}"#;
        // As the renderer escapes them into the document.
        let laid_out = "{\n  &#34;id&#34;: 1\n}";
        let wire = "{&#34;id&#34;:1}";

        // A disclosure the markup drew closed, and the click that opened it.
        assert_eq!(payload(false, false, frame), wire);
        assert_eq!(payload(false, true, frame), laid_out);

        // One drawn open is laid out from the first render and stops being when
        // closed: the event only says a disclosure changed.
        assert_eq!(payload(true, false, frame), laid_out);
        assert_eq!(payload(true, true, frame), wire);
    }
}
