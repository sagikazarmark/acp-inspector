//! The copy control, on every surface that draws bytes.
//!
//! It copies the bytes that crossed, never the indented rendering (§8). The
//! clipboard API is unavailable outside a secure context on some WebViews, so
//! `execCommand` is the fallback. The text is sent over the eval channel rather
//! than spliced into the script. A toast confirms only a write the script
//! reports as done.

use dioxus::prelude::*;
use dioxus_free_icons::{Icon, icons::ld_icons::LdCopy};

use crate::toast;

/// The script both paths live in. It answers one boolean: whether the text is
/// on the clipboard. Neither path throws.
const COPY: &str = r#"
const text = await dioxus.recv();
const focused = document.activeElement;
try {
  await navigator.clipboard.writeText(text);
  return true;
} catch (unavailable) {
  const held = document.createElement("textarea");
  held.value = text;
  held.setAttribute("readonly", "");
  held.style.position = "fixed";
  held.style.opacity = "0";
  document.body.appendChild(held);
  try {
    held.select();
    return document.execCommand("copy");
  } finally {
    held.remove();
    if (focused instanceof HTMLElement && focused.isConnected) {
      focused.focus({ preventScroll: true });
    }
  }
}
"#;

/// Puts one piece of text on the system clipboard, and answers whether it went.
///
/// Callers choose feedback: evidence Copy confirms success with a toast;
/// export retrieval also displays failure beside the path.
pub(crate) async fn copied(text: String) -> bool {
    // The script is constant and the text is sent to it: what is copied is
    // agent-controlled, so it must never become part of a program (§7.3).
    let copying = document::eval(COPY);
    if copying.send(text).is_err() {
        return false;
    }
    // No answer, a wrong answer or a closed window: not a copy anyone can
    // promise.
    copying.join::<bool>().await.unwrap_or(false)
}

/// The copy button, the same control everywhere bytes are drawn.
#[component]
pub fn Copy(
    /// What lands on the clipboard, exactly.
    #[props(default)]
    text: String,
    /// Keep large immutable evidence shared until activation.
    #[props(default)]
    frame: Option<acp_inspector_core::Frame>,
    /// Diagnostic text shared with its preview and byte-window reader.
    #[props(default)]
    shared: Option<std::sync::Arc<str>>,
    /// What it is, for the control's own name — "this frame", "this line".
    what: String,
) -> Element {
    // Reached from context: the control sits on three screens and nothing
    // between them is about it.
    let announcer = toast::announcer();

    rsx! {
        button {
            class: "btn btn-xs btn-quiet copy",
            "data-slot": "copy",
            r#type: "button",
            // The glyph is silent, so the button carries its own name.
            aria_label: "Copy {what}",
            title: "Copy {what}",
            onclick: move |event| {
                // The row underneath is a disclosure; a copy must not also
                // toggle it.
                event.stop_propagation();
                let text = copy_text(&text, frame.as_ref(), shared.as_deref());
                // One string builds both "Copy this frame" and "Copied this
                // frame".
                let what = what.clone();
                spawn(async move {
                    if copied(text).await {
                        announcer.say(format!("Copied {what}"));
                    }
                });
            },
            span { class: "disclosure-icon", aria_hidden: "true",
                Icon { class: "icon", width: 16, height: 16, icon: LdCopy }
            }
        }
    }
}

fn copy_text(
    text: &str,
    frame: Option<&acp_inspector_core::Frame>,
    shared: Option<&str>,
) -> String {
    frame
        .map(|frame| frame.as_str())
        .or(shared)
        .unwrap_or(text)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_copy_takes_the_complete_shared_text_not_a_preview() {
        let raw: std::sync::Arc<str> = format!("{}🦀\tEND\r", "x".repeat(7 * 1024 * 1024)).into();
        let preview = crate::page::prefix(&raw, 512);
        assert_eq!(
            copy_text(preview, None, Some(&raw)).as_bytes(),
            raw.as_bytes()
        );
    }

    #[test]
    fn the_shared_copy_control_names_what_it_takes() {
        let html = dioxus_ssr::render_element(rsx! {
            Copy { text: "verbatim bytes", what: "this frame" }
        });

        assert!(html.contains(r#"data-slot="copy""#), "{html}");
        assert!(html.contains(r#"aria-label="Copy this frame""#), "{html}");
        assert!(html.contains(r#"title="Copy this frame""#), "{html}");
        // The glyph is decorative; the name is on the control.
        assert!(
            html.contains(r#"class="disclosure-icon" aria-hidden="true"><svg"#),
            "the mark comes from the icon family rather than from the font: {html}"
        );
        assert!(
            !html.contains(">copy</button>") && !html.contains('✎'),
            "and no character stands in for it: {html}"
        );
    }

    #[test]
    fn the_fallback_restores_the_copy_controls_focus() {
        let remembered = COPY.find("document.activeElement").expect("focus capture");
        let attempted = COPY
            .find("navigator.clipboard.writeText")
            .expect("Clipboard API attempt");
        assert!(
            remembered < attempted,
            "the fallback remembers the originating control before an asynchronous rejection: {COPY}"
        );
        assert!(
            COPY.contains("focused.isConnected")
                && COPY.contains("focused.focus({ preventScroll: true })"),
            "the fallback restores focus without moving the viewport: {COPY}"
        );
    }
}
