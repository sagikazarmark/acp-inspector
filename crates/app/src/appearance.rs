//! The appearance switch, and the one thing the window does with an answer:
//! mark the document element (`docs/architecture.md` §9).
//!
//! **Presentational, like everything else here** (§5). What the three values are,
//! which token each carries and what an unreadable settings file falls back to
//! are core's ([`Appearance`], [`Settings`](acp_inspector_core::Settings)); this is
//! the `<select>` that offers them and the attribute that makes the stylesheet
//! act on one.

use acp_inspector_core::Appearance;
use dioxus::prelude::*;

/// Marks the document element with an explicit choice, or unmarks it for
/// `System` so the stylesheet's `prefers-color-scheme` rules answer again.
///
/// **The document element, not the app's own root.** The ground is painted by
/// `body`, which would go on reading the media query if the attribute were
/// scoped inside the app — leaving the wrong colour behind the window under an
/// explicit choice.
///
/// [`document::eval`] is the renderer's own bridge into its WebView — the
/// desktop crate's way to touch a node outside the app's subtree, and the reason
/// it needs no `web_sys` of its own. **The script is a constant.** Its only
/// variable part comes from `theme_name`, which returns one of two static theme
/// names this crate wrote: nothing an agent said, a file held or a user typed
/// can reach it.
///
/// Fire-and-forget: the script is queued the moment it is handed over, and there
/// is nothing to wait for and nothing to do if it fails.
pub fn paint(appearance: Appearance) {
    let _ = document::eval(&match theme_name(appearance) {
        None => "document.documentElement.removeAttribute('data-theme');".to_owned(),
        Some(theme) => format!(
            "document.documentElement.setAttribute('data-theme', '{}');",
            theme
        ),
    });
}

fn theme_name(appearance: Appearance) -> Option<&'static str> {
    match appearance {
        Appearance::System => None,
        Appearance::Light => Some("inspector-light"),
        Appearance::Dark => Some("inspector-dark"),
    }
}

/// The three-way switch, in the bar — as the drawing puts it: a segmented
/// group of a sun, a moon and the word for *whichever the desktop is*.
///
/// **A group of buttons rather than a native select**, which is what the design
/// draws and what a segmented control is for: three exclusive answers, one of
/// which is always true, all three visible. What a native select gave for free
/// is given back explicitly here — each button carries the choice's full name,
/// `aria-pressed` says which one is on, and the glyphs are decoration the name
/// does not depend on.
#[component]
pub fn AppearanceControl(chosen: Appearance, on_choose: EventHandler<Appearance>) -> Element {
    rsx! {
        div { class: "seg appearance", role: "group", aria_label: "Appearance",
            // Light, Dark, then System — the order the drawing puts them in,
            // which is the two schemes and then *whichever the desktop is*.
            for choice in [Appearance::Light, Appearance::Dark, Appearance::System] {
                button {
                    key: "{choice.token()}",
                    class: if choice == Appearance::System { "seg-item seg-word" } else { "seg-item seg-glyph" },
                    "data-slot": "appearance",
                    "data-appearance": choice.token(),
                    r#type: "button",
                    aria_pressed: choice == chosen,
                    aria_label: "{choice.label()} appearance",
                    title: "{choice.label()} appearance",
                    onclick: move |_| on_choose.call(choice),
                    span { aria_hidden: "true", {glyph(choice)} }
                }
            }
        }
    }
}

/// What each choice is drawn as: the two schemes get the sky they are, and
/// System gets the word, because *follow the desktop* is not a picture.
fn glyph(appearance: Appearance) -> &'static str {
    match appearance {
        Appearance::Light => "☀",
        Appearance::Dark => "☾",
        Appearance::System => "auto",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[component]
    fn Host(chosen: Appearance) -> Element {
        rsx! { AppearanceControl { chosen, on_choose: |_| {} } }
    }

    fn screen(chosen: Appearance) -> String {
        let mut dom = VirtualDom::new_with_props(Host, HostProps { chosen });
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    #[test]
    fn appearance_choices_select_their_daisyui_theme() {
        assert_eq!(theme_name(Appearance::System), None);
        assert_eq!(theme_name(Appearance::Light), Some("inspector-light"));
        assert_eq!(theme_name(Appearance::Dark), Some("inspector-dark"));
    }

    #[test]
    fn appearance_is_a_named_segmented_group_of_three_exclusive_answers() {
        let html = screen(Appearance::Dark);

        // A group of buttons rather than a native select, which is what the
        // design draws — and what a segmented control is for: three exclusive
        // answers, one of which is always true, all three visible.
        assert!(
            html.contains(r#"class="seg appearance""#)
                && html.contains(r#"role="group""#)
                && html.contains(r#"aria-label="Appearance""#),
            "the group carries the question: {html}"
        );
        // What the native select gave for free is given back explicitly: each
        // button carries the choice's complete name and says whether it is the
        // one that is on.
        for (token, name) in [
            ("light", "Light appearance"),
            ("dark", "Dark appearance"),
            ("system", "System appearance"),
        ] {
            assert!(
                html.contains(&format!(r#"data-appearance="{token}""#))
                    && html.contains(&format!(r#"aria-label="{name}""#))
                    && html.contains(&format!(r#"title="{name}""#)),
                "the {token} choice is named for both readers: {html}"
            );
        }
        assert_eq!(
            html.matches("aria-pressed=true").count(),
            1,
            "exactly one answer is on: {html}"
        );
        // And the glyphs are decoration the name does not depend on.
        assert!(
            html.contains(r#"<span aria-hidden="true">☀</span>"#)
                && html.contains(r#"<span aria-hidden="true">☾</span>"#),
            "the two schemes are drawn as the sky they are: {html}"
        );
    }
}
