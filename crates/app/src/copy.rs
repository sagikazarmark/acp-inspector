//! Taking a piece of the evidence away.
//!
//! **Wherever the content is bytes** (§9's density rule names them: the raw
//! JSON, the frames, the ids, the stderr lines). What a reader does with a
//! frame after reading it is paste it into an issue, and a tool whose only way
//! out was an export of the whole trace made them retype the one line they
//! wanted — or select it by hand out of a scroller that is appending underneath
//! them.
//!
//! **It copies the bytes that crossed.** Nothing is rewritten on the way out,
//! for the reason nothing is on the way in (§8): what left this window has to be
//! what crossed the wire, or the paste is evidence about the inspector. That
//! holds while the window is drawing a frame indented (`crate::indent`) — the
//! whitespace is for the reader in front of this screen, and the person the
//! paste is sent to is looking at what the agent sent.
//!
//! Two mechanisms, because a WebView is not a page: the clipboard API is what
//! this should be, and it is unavailable outside a secure context on some of
//! the platforms this shell runs on. The fallback is the old `execCommand`
//! path, which every WebView still honours. The text goes over the channel
//! rather than into the script, so a frame containing a quote, a backslash or a
//! newline is copied rather than executed.
//!
//! **And the window says so afterwards** ([`crate::toast`]), because a
//! clipboard is the one thing this tool does that leaves no mark on any screen:
//! every other control changes something a reader can look at, and this one
//! changes something in another application. The sentence is said only when the
//! script reports the write took — which is the older rule kept rather than
//! traded away, since a confirmation that appeared whatever happened would be
//! the button pretending.

use dioxus::prelude::*;
use dioxus_free_icons::{Icon, icons::hi_outline_icons::HiClipboardCopy};

use crate::toast;

/// The script both paths live in, and the one answer they give back: whether
/// the text is on the clipboard.
///
/// Neither path throws on the way out. The API path resolves or it does not,
/// and `execCommand` answers with a boolean an unsupported WebView reports
/// `false` for — so the two failures this can have are one value here.
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
    // The script is the constant above and never anything else: a frame's text
    // is *sent* to it, so nothing an agent emitted is ever part of a program
    // this window runs. That is not a nicety here — the text being copied is
    // attacker-controlled by definition, since the subject of this tool is an
    // agent that may be behaving badly (§7.3).
    let copying = document::eval(COPY);
    if copying.send(text).is_err() {
        return false;
    }
    // A script that never answered, answered something else, or ran in a window
    // that has gone away is a copy nobody can promise happened.
    copying.join::<bool>().await.unwrap_or(false)
}

/// The button, wherever bytes are drawn.
///
/// **The same control everywhere**, because it does the same thing everywhere:
/// a reader who learns it on a trace row has learnt it on a timeline entry's
/// raw JSON. It is a ghost button in the label register, so it is legible where
/// it sits and does not compete with the line it is about — a copy control that
/// weighed as much as the frame would be a mark on every row of the densest
/// surface in the tool.
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
    // Where this says the write took. Reached from context rather than handed
    // down, because this is the same control on three screens and none of the
    // components between them are about it: a callback threaded from the root
    // through the Console, the Trace, the Diagnostic channel and every Timeline
    // entry would be five components carrying a message for a sixth.
    let announcer = toast::announcer();

    rsx! {
        button {
            class: "btn btn-xs btn-quiet copy",
            "data-slot": "copy",
            r#type: "button",
            // The glyph says nothing a reader can hear, so the button carries
            // its own name — the rule the console's chevron already follows.
            aria_label: "Copy {what}",
            title: "Copy {what}",
            onclick: move |event| {
                // The row underneath is a disclosure on both surfaces, and a
                // copy that also expanded it would be one click doing two
                // things.
                event.stop_propagation();
                let text = copy_text(&text, frame.as_ref(), shared.as_deref());
                // The control's own name, said back: "Copy this frame" pressed
                // is "Copied this frame". One string builds both, so the two
                // cannot come to disagree about what was taken.
                let what = what.clone();
                spawn(async move {
                    if copied(text).await {
                        announcer.say(format!("Copied {what}"));
                    }
                });
            },
            // **A glyph now, from the family.** It was the word `copy`, on the
            // reasoning that a clipboard pictogram is a *character* the platform
            // fonts this window inherits do not all have — true of a character,
            // and this window stopped drawing icons as characters when it took
            // an icon crate: the paths ship with the binary and render the same
            // everywhere. The two controls the same reasoning named — Export and
            // Clear — became icons on the Console's own bar, so the word left
            // this one as the last text control among glyphs. What it is called
            // is unchanged and is where it always was, on the control.
            span { class: "disclosure-icon", aria_hidden: "true",
                Icon { class: "icon", icon: HiClipboardCopy }
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
        // The glyph is decorative and the name is on the control, which is the
        // rule every icon-only control in this window follows: what it takes is
        // in the accessibility tree whether or not anything is drawn.
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
