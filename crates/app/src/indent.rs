//! The indentation switch, and the one thing the window does with an answer:
//! draw a payload spaced out (`docs/architecture.md` §8, §9).
//!
//! **Presentational, like everything else here** (§5). What indenting does to a
//! payload, what it refuses to touch and what an unreadable settings file falls
//! back to are core's ([`Indentation`], [`Settings`](acp_inspector_core::Settings));
//! this is the switch that offers the choice and the call every surface drawing
//! bytes makes on the way to the screen.
//!
//! **Reached from context rather than handed down**, which is the rule the copy
//! control already follows (`crate::copy`) and for the same reason: the surfaces
//! that draw bytes are the Trace's frames, a Timeline entry's raw evidence and a
//! tool call's raw input and output, and a preference threaded to all three
//! would be the Console, the Trace, the Timeline and every entry in it carrying
//! a value for somebody else. What is in the context is the signal, so a change
//! redraws the screens that read it.
//!
//! **Off is the default and off is the record.** A window nobody has switched
//! draws exactly what crossed (§8), and what leaves the window — the clipboard,
//! the JSONL export — is the wire's own bytes whatever the switch says: this
//! adds whitespace on the way to a reader's eyes and to nothing else.
//!
//! **And only where a reader is looking.** Every payload on these screens is
//! inside a disclosure, most of them closed, and a Trace holding ten thousand
//! Frames would otherwise be ten thousand documents laid out to be seen by
//! nobody on every render. So the layout is spent on the disclosures that are
//! open, which is what [`drawn`] takes a name and a state for.
//!
//! **What this governs is what crossed the wire**: a Frame on the Trace tab, a
//! Timeline entry's raw evidence, a tool call's raw input and output. Two things
//! that look like it are deliberately not on that list. An elicitation's property
//! schema (`crate::elicitation`) is drawn indented always, because it is this
//! window re-serializing a type it decoded rather than bytes an agent sent —
//! there is no wire form of it to be faithful to. And a diff's two sides, or an
//! embedded text resource (`crate::update`), are left exactly alone even where
//! they hold JSON: that is file content, and a tool that re-spaced one half of a
//! diff would be reporting a change nobody made.

use std::collections::HashSet;

use acp_inspector_core::Indentation;
use dioxus::prelude::*;

/// The disclosures the reader has turned over from the state their markup drew
/// them in.
///
/// **Turned rather than open**, because a `<details>` is the platform's own
/// control and this window means to keep it that way: nothing here sets whether
/// one is open, and the summary's own activation — a pointer or the keyboard, on
/// the control the reader pressed — is what says one changed. What the markup
/// drew is known at the call site, and this is the other half of the answer.
///
/// A bit that goes astray costs a payload its whitespace and nothing else, which
/// is why it may be kept this cheaply: the state this falls back to is the wire,
/// and the wire is what the tool is for.
#[derive(Clone, Copy)]
struct Turned(Signal<HashSet<String>>);

/// Puts the window's indentation, and what it has been asked to lay out, where
/// every surface that draws bytes can find them. **Once, at the root**, which is
/// also the one place the choice is written.
pub fn provide(chosen: Indentation) -> Signal<Indentation> {
    use_context_provider(|| Turned(Signal::new(HashSet::new())));
    use_context_provider(|| Signal::new(chosen))
}

/// How this window is drawing payloads, from anywhere under the root.
///
/// **Or the wire, where there is no window over this at all.** The screen tests
/// in this crate build one component's markup with no renderer around it, and a
/// frame row that panicked for want of a preference would make them hostage to a
/// corner of the window they are not about — so the answer outside a window is
/// the same answer a window nobody has switched gives.
///
/// Not a hook: this is called from the functions that draw one row, which are
/// called in loops and behind conditions, and a hook there would be a hook whose
/// position in the order depends on the traffic.
pub fn chosen() -> Indentation {
    if !rendering() {
        return Indentation::default();
    }

    try_consume_context::<Signal<Indentation>>()
        .map_or_else(Indentation::default, |chosen| chosen())
}

/// A payload as this window draws it inside the disclosure called `key`, whose
/// markup was drawn `open`: laid out where it is JSON, the reader asked for
/// that, and they have that disclosure open — and exactly as it arrived
/// otherwise.
///
/// The one call every surface drawing bytes makes, so that what "indented" means
/// cannot come to differ between the two screens that draw the same frame, and so
/// that no surface can lay out a payload nobody is looking at. **The key is one
/// window-wide name**: a Frame's ordinal, an entry's identity, a tool call's id
/// and which half of it — each surface spells its own and every one of them says
/// which surface it came from.
///
/// **What a closed disclosure holds is the wire**, which is also what a bit gone
/// astray falls back to. Nothing is read out of the store at all while the switch
/// is off, so a window in its default state never subscribes to it and a
/// disclosure opening on that window redraws nothing.
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

/// Remembers that a disclosure was turned over, which is the whole of what its
/// summary's activation says.
///
/// Nothing where there is no window over this: a screen test that clicks a
/// summary is not a test about the bar.
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

/// Whether there is a window over this at all — asked before reaching for
/// context, because reaching for it outside one is the panic rather than the
/// `None` (`dioxus_core::Runtime`).
fn rendering() -> bool {
    dioxus::core::Runtime::try_current()
        .is_some_and(|runtime| runtime.try_current_scope_id().is_some())
}

/// The switch, in the bar beside the appearance — the same segmented group,
/// because it is the same kind of preference: one question about how the whole
/// window reads, with two exclusive answers.
///
/// The words are the two the choice has (`Indentation`): the wire, or the wire
/// laid out. Each button carries the complete name; the group carries the
/// question.
#[component]
pub fn IndentationControl(chosen: Indentation, on_choose: EventHandler<Indentation>) -> Element {
    rsx! {
        div {
            class: "seg indentation",
            role: "group",
            aria_label: "How payloads are drawn",
            title: "Draw JSON payloads indented. What crossed the wire is unchanged: an export and a copy still carry the bytes.",
            for choice in Indentation::ALL {
                button {
                    key: "{choice.token()}",
                    class: "seg-item seg-word",
                    "data-slot": "indentation",
                    "data-indentation": choice.token(),
                    r#type: "button",
                    aria_pressed: choice == chosen,
                    aria_label: "{word(choice)} payloads",
                    onclick: move |_| on_choose.call(choice),
                    "{word(choice)}"
                }
            }
        }
    }
}

/// What each choice is called on its segment: the wire's own bytes, or the same
/// bytes spaced out.
fn word(indentation: Indentation) -> &'static str {
    match indentation {
        Indentation::Wire => "wire",
        Indentation::Indented => "indent",
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
    fn indentation_is_a_named_segmented_group_beside_the_appearance() {
        let html = screen(Indentation::Wire);

        assert!(
            html.contains(r#"class="seg indentation""#)
                && html.contains(r#"role="group""#)
                && html.contains(r#"aria-label="How payloads are drawn""#)
                && html.contains("title=\"Draw JSON"),
            "the group carries the question and the tooltip that qualifies it: {html}"
        );
        // The two answers the choice has, each with its complete name.
        assert!(
            html.contains(r#"data-indentation="wire""#)
                && html.contains(r#"aria-label="wire payloads""#)
                && html.contains(r#"data-indentation="indented""#)
                && html.contains(r#"aria-label="indent payloads""#),
            "both answers are named: {html}"
        );
        // A window nobody has switched draws the wire.
        assert!(
            html.contains(r#"data-indentation="wire" type="button" aria-pressed=true"#),
            "the window opens on the bytes that crossed: {html}"
        );
    }

    #[test]
    fn the_persisted_choice_is_what_the_switch_shows() {
        let html = screen(Indentation::Indented);
        assert!(
            html.contains(r#"data-indentation="indented" type="button" aria-pressed=true"#),
            "{html}"
        );
    }

    #[test]
    fn a_surface_with_no_window_over_it_draws_the_wire() {
        // What every screen test in this crate renders into, and the reason
        // this answers rather than panics: a frame row is not about the bar.
        // Neither an open disclosure nor a closed one is a reason to reach for a
        // preference that is not there.
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

        // And one drawn open — an entry whose evidence *is* its rendering (§8) —
        // which is laid out from the first render, and stops being when the
        // reader closes it. The event says only that a disclosure changed, which
        // is why what is kept is the turning rather than the state.
        assert_eq!(payload(true, false, frame), laid_out);
        assert_eq!(payload(true, true, frame), wire);
    }
}
