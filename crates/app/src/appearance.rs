//! The appearance switch, and marking the document element with the choice.
//! The values and their fallback are core's ([`Appearance`]).

use acp_inspector_core::Appearance;
use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::ld_icons::{LdMoon, LdSun},
};

/// Marks the document element with an explicit choice, or unmarks it for
/// `System` so the stylesheet's `prefers-color-scheme` rules answer again.
///
/// The document element rather than the app root, because `body` paints the
/// ground and would otherwise keep following the media query. The script is a
/// constant apart from `theme_name`, which only returns names this crate wrote.
/// Fire-and-forget.
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

/// The three-way segmented switch in the bar: Light, Dark and System. Each
/// button carries the choice's full name and `aria-pressed`; the glyphs are
/// decoration.
#[component]
pub fn AppearanceControl(chosen: Appearance, on_choose: EventHandler<Appearance>) -> Element {
    rsx! {
        div { class: "seg appearance", role: "group", aria_label: "Appearance",
            // Light, Dark, then System, as drawn.
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

/// What each choice is drawn as: icons for the two schemes, the word for
/// System.
fn glyph(appearance: Appearance) -> Element {
    match appearance {
        Appearance::Light => rsx! { Icon { class: "icon", width: 16, height: 16, icon: LdSun } },
        Appearance::Dark => rsx! { Icon { class: "icon", width: 16, height: 16, icon: LdMoon } },
        Appearance::System => rsx! { "Auto" },
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

        assert!(
            html.contains(r#"class="seg appearance""#)
                && html.contains(r#"role="group""#)
                && html.contains(r#"aria-label="Appearance""#),
            "the group carries the question: {html}"
        );
        // Each button carries its full name and whether it is on.
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
        // The icons are decoration from the icon set, not font glyphs.
        assert!(
            html.matches(r#"<span aria-hidden="true"><svg class="icon" height="16" width="16""#)
                .count()
                == 2
                && !html.contains('☀')
                && !html.contains('☾'),
            "the two schemes are drawn as the sky they are: {html}"
        );
    }
}
