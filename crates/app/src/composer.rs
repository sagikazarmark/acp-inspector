//! The composer, and where the turn stands (`docs/architecture.md` §9): what
//! starts a turn, what stops it, and what the stream said became of it.
//!
//! **The turn's state is read, never inferred.** Nothing here waits on the
//! prompt call: the turn is a field the update stream drives (§11 seam 2), so
//! this renders [`TurnState`] and hands clicks back to core, and a turn that
//! ends while nobody is awaiting anything ends on screen just the same. That is
//! also why the stop button is honest about what it does — `session/cancel` is
//! a request, and the turn is over when the agent says it is, with
//! `stopReason: "cancelled"` if it behaves (§7.1).

use crate::media_draft::{MediaDraft, MediaState};
use crate::prompt_media::PromptMedia;
use acp_inspector_core::{CallError, TurnState, v1};
use dioxus::html::{FileData, HasFileData};

/// Native file drops must be accepted synchronously, before IPC. Do not let a
/// synthetic or same-document URI drag reuse Dioxus's last native file paths.
const IMAGE_DROP_BRIDGE: &str = r#"
const [id, windows] = await dioxus.recv();
const root = document.getElementById(id)?.closest('[data-slot=composer]');
if (root) {
  let internal = false;
  let nativeDispatch = false;
  const interpreter = windows ? window.interpreter : null;
  const originalDrop = interpreter?.handleWindowsDragDrop;
  const nativeDrop = originalDrop && function(...args) {
    nativeDispatch = true;
    try { return originalDrop.apply(this, args); }
    finally { nativeDispatch = false; }
  };
  if (nativeDrop) interpreter.handleWindowsDragDrop = nativeDrop;
  const started = () => { internal = true; };
  const ended = () => { internal = false; };
  document.addEventListener('dragstart', started, true);
  document.addEventListener('dragend', ended, true);
  root.addEventListener('dragover', event => event.preventDefault());
  root.addEventListener('drop', event => {
    event.preventDefault();
    if ((!event.isTrusted && !nativeDispatch) || internal) event.stopImmediatePropagation();
    internal = false;
  }, true);
  const removed = new MutationObserver(() => {
    if (!root.isConnected) {
      document.removeEventListener('dragstart', started, true);
      document.removeEventListener('dragend', ended, true);
      if (nativeDrop && interpreter.handleWindowsDragDrop === nativeDrop) interpreter.handleWindowsDragDrop = originalDrop;
      removed.disconnect();
    }
  });
  removed.observe(document.body, {childList:true, subtree:true});
}
"#;
use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::hi_outline_icons::{HiPaperAirplane, HiStop},
};

/// Picker hints follow each advertisement; signature validation is independent.
fn accepted_media(image: bool, audio: bool, embedded: bool) -> &'static str {
    // Text/source filenames are open-ended. Validate contents after selection.
    if embedded {
        return "";
    }
    match (image, audio) {
        (true, true) => ".png,.jpg,.jpeg,.gif,.webp,.wav,.mp3",
        (true, false) => ".png,.jpg,.jpeg,.gif,.webp",
        (false, true) => ".wav,.mp3",
        _ => "",
    }
}

#[component]
fn AudioPreview(media: PromptMedia) -> Element {
    let mut unavailable = use_signal(|| false);
    rsx! { div { class: "prompt-audio",
        audio { controls: true, preload: "none", src: media.preview(), aria_label: "Preview audio: {media.name}",
            onerror: move |_| unavailable.set(true),
        }
        if unavailable() { span { class: "hint", role: "status", "Playback is unavailable in this WebView. The original file can still be sent." } }
    } }
}

/// The one composer Affordance as the Turn changes underneath it.
#[derive(Clone, Copy)]
enum ComposerAffordance {
    Send,
    Stop,
    Stopping,
}

/// One command the agent said it can run, as the composer offers it.
///
/// **The agent's own list and nothing else** (`available_commands_update`): a
/// completion this window invented would be a command nobody can run, and one
/// it remembered after the agent stopped publishing it would be a command that
/// used to exist. Two owned strings rather than the protocol type, because what
/// the control needs is a name to type and a sentence to read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub name: String,
    pub description: String,
}

impl ComposerAffordance {
    fn for_turn(turn: &TurnState) -> Self {
        match turn {
            TurnState::Cancelling => Self::Stopping,
            turn if turn.is_running() => Self::Stop,
            _ => Self::Send,
        }
    }

    fn is_available(self, sendable: bool) -> bool {
        match self {
            Self::Send => sendable,
            Self::Stop => true,
            Self::Stopping => false,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Send => "Send prompt",
            Self::Stop => "Stop turn",
            Self::Stopping => "Stopping turn",
        }
    }

    fn class(self) -> &'static str {
        match self {
            Self::Send => "btn btn-md btn-filled send",
            Self::Stop | Self::Stopping => "btn btn-md btn-quiet btn-bad send stop",
        }
    }
}

/// The prompt box, the buttons, and the turn's state above them.
#[component]
pub fn Composer(
    turn: TurnState,
    /// Whether there is a session to prompt into.
    ready: bool,
    /// Whether there is an agent at all, which is what tells the two absences
    /// apart: a window that has launched nothing, and an agent that is right
    /// there with no session open in it — which is where closing or deleting
    /// the live session leaves this (§7.5). Telling the reader to launch an
    /// agent that is already running would be the screen misreading itself.
    connected: bool,
    /// Whether the agent is waiting on an answer from the user — a blocking
    /// request nobody has answered yet (§7.2).
    ///
    /// Not a turn state and deliberately not one: a turn stopped on a blocking
    /// request — a permission request or an elicitation (§7.8) — is still the
    /// turn that was running, so this is *derived* from a request that is
    /// waiting, and the line below says which of the two the turn is doing.
    blocked: bool,
    /// Why there is not, when the handshake that opens one failed.
    problem: Option<CallError>,
    /// What the agent last said it can be asked to run, newest announcement
    /// only (§7.2's `available_commands_update`). Empty where it has said
    /// nothing, which is most agents and is why the control appears and
    /// disappears with the list rather than being drawn empty.
    commands: Vec<Command>,
    /// The mode the next prompt goes out under, where the agent published any
    /// ([`Cycle`]). `None` is an agent that published none, which is most of
    /// them, and the control is not drawn at all.
    mode: Option<Cycle>,
    #[props(default)] image_advertised: bool,
    #[props(default)] audio_advertised: bool,
    #[props(default)] embedded_advertised: bool,
    on_prompt: Callback<Vec<v1::ContentBlock>, Result<(), CallError>>,
    on_stop: EventHandler<()>,
    /// A mode the reader cycled to, on its way to `session/set_mode` — the same
    /// handler the rail's own rows call.
    on_set_mode: EventHandler<v1::SessionModeId>,
) -> Element {
    let mut text = use_signal(String::new);
    let mut media = use_signal(MediaDraft::default);
    let mut queued = use_signal(std::collections::VecDeque::<(u64, FileData)>::new);
    let mut reading = use_signal(|| false);
    let mut media_problem = use_signal(|| None::<String>);
    let mut overflow = use_signal(|| None::<String>);
    let mut clipboard_bridge = use_signal(|| None::<document::Eval>);
    let mut paste_waiting = use_signal(|| 0usize);
    let mut deferred_files = use_signal(Vec::<(usize, Vec<FileData>)>::new);
    let picker_id = use_hook(|| {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        format!(
            "prompt-media-{}",
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )
    });
    // Which of the matching commands the keyboard is on, and whether the reader
    // has waved the list away. Both are window-session state of one control and
    // neither outlives the draft they are about.
    let mut highlighted = use_signal(|| 0usize);
    let mut dismissed = use_signal(|| false);
    let running = turn.is_running();
    let attachments_advertised = image_advertised || audio_advertised || embedded_advertised;
    let reserve = use_callback(move |names: Vec<String>| {
        let mut rejected = Vec::new();
        let ids = names
            .into_iter()
            .map(|name| match media.write().reserve(name.clone()) {
                Ok(id) => Some(id),
                Err(_) => {
                    rejected.push(name);
                    None
                }
            })
            .collect::<Vec<_>>();
        if !rejected.is_empty() {
            overflow.set(Some(format!("Not added (8 attachment rows maximum): {}. Dismiss this notice to send the remaining files.", rejected.join(", "))));
        }
        media_problem.set(None);
        ids
    });
    let ingest = use_callback(move |files: Vec<FileData>| {
        if !attachments_advertised || !ready || running || files.is_empty() {
            return;
        }
        if paste_waiting() > 0 {
            deferred_files.write().push((paste_waiting(), files));
            return;
        }
        let ids = reserve.call(files.iter().map(|file| file.name()).collect());
        for (id, file) in ids.into_iter().zip(files) {
            if let Some(id) = id {
                queued.write().push_back((id, file));
            }
        }
        if reading() {
            return;
        }
        reading.set(true);
        // One worker per composer drains all batches in arrival order. Removing
        // queued rows cancels their reads; removing the current row discards its
        // completion. The scope cancels the worker on a Session switch.
        spawn(async move {
            loop {
                let next = queued.write().pop_front();
                let Some((id, file)) = next else {
                    break;
                };
                let result = PromptMedia::from_file(file).await.and_then(|media| {
                    if media.advertised(image_advertised, audio_advertised, embedded_advertised) {
                        Ok(media)
                    } else {
                        Err(format!(
                            "The Agent did not advertise {} prompts.",
                            if media.is_text() {
                                "embedded context"
                            } else if media.is_audio() {
                                "audio"
                            } else {
                                "image"
                            }
                        ))
                    }
                });
                let Ok(mut draft) = media.try_write() else {
                    return;
                };
                draft.finish(id, result);
            }
            reading.set(false);
        });
    });
    let clipboard_input = use_callback(move |input: crate::clipboard::ClipboardInput| {
        use crate::clipboard::ClipboardInput;
        match input {
            ClipboardInput::Reserve { names } => {
                let ids = if image_advertised && ready && !running {
                    reserve.call(names)
                } else {
                    vec![None; names.len()]
                };
                let pending = paste_waiting().saturating_sub(1);
                let mut ready_files = Vec::new();
                deferred_files.write().retain_mut(|(waiting, files)| {
                    *waiting = waiting.saturating_sub(1);
                    if *waiting == 0 {
                        ready_files.append(files);
                        false
                    } else {
                        true
                    }
                });
                paste_waiting.set(0);
                ingest.call(ready_files);
                paste_waiting.set(pending);
                Some(ids)
            }
            ClipboardInput::Finish { id, data, error } => {
                let name = media
                    .read()
                    .entries()
                    .iter()
                    .find(|entry| entry.id == id)
                    .map(|entry| entry.name.clone());
                if let Some(name) = name {
                    media
                        .write()
                        .finish(id, crate::clipboard::image(name, data, error));
                }
                None
            }
        }
    });
    #[cfg(test)]
    use_hook(move || {
        tests::CLIPBOARD_INPUT.with(|slot| *slot.borrow_mut() = Some(clipboard_input))
    });
    // Nothing to send while a turn is running: ACP has one turn per session at
    // a time, and a second prompt is the stop button's job first.
    let sendable = ready
        && !running
        && media.read().sendable()
        && overflow.read().is_none()
        && (!text().trim().is_empty()
            || attachments_advertised && !media.read().entries().is_empty());
    let affordance = ComposerAffordance::for_turn(&turn);
    let affordance_available = affordance.is_available(sendable);
    let show_shortcut = ready && !running;
    let presentation = presented(&turn, blocked, problem.as_ref(), ready, connected);
    let matches = matching(&commands, &text(), dismissed());
    let highlight = highlighted().min(matches.len().saturating_sub(1));

    let mut send = move || {
        let prompt = text();
        if sendable {
            // Input/drop and Send can arrive in the same renderer batch. The
            // control's painted availability is not permission to omit a new
            // Reading/Failed row or a count-overflow notice.
            if overflow.read().is_some() {
                return;
            }
            if paste_waiting() > 0 {
                return;
            }
            let Some(attachments) = media.read().content() else {
                return;
            };
            if prompt.trim().is_empty() && (!attachments_advertised || attachments.is_empty()) {
                return;
            }
            let mut content = Vec::new();
            if !prompt.trim().is_empty() {
                content.push(v1::ContentBlock::from(prompt));
            }
            if attachments_advertised {
                content.extend(attachments);
            }
            if let Err(error) = on_prompt.call(content) {
                media_problem.set(Some(error.to_string()));
                return;
            }
            text.set(String::new());
            media_problem.set(None);
            media.write().clear();
            queued.write().clear();
        }
    };
    // Putting one in the box is a *refill*, not a send: what a command takes
    // after its name is the reader's to type, and a control that sent on
    // selection would be prompting with half a sentence.
    let mut fill = move |name: &str| {
        text.set(format!("/{name} "));
        highlighted.set(0);
    };

    let clipboard_id = format!("{picker_id}-clipboard");
    rsx! {
        div { class: "composer", "data-slot": "composer",
            id: clipboard_id.clone(),
            "data-image-paste": if image_advertised && ready && !running { "true" } else { "false" },
            onpaste: move |_| { *paste_waiting.write() += 1; },
            onmounted: {
                let id = clipboard_id;
                move |_| {
                    let mut bridge = document::eval(crate::clipboard::BRIDGE);
                    clipboard_bridge.set(Some(bridge));
                    let _ = bridge.send((id.clone(), crate::prompt_media::MAX_MEDIA_BYTES));
                    spawn(async move {
                        while let Ok(input) = bridge.recv::<crate::clipboard::ClipboardInput>().await {
                            if let Some(ids) = clipboard_input.call(input) { let _ = bridge.send(ids); }
                        }
                    });
                }
            },
            ondragover: move |event| { event.prevent_default(); },
            ondrop: move |event| {
                event.prevent_default();
                event.stop_propagation();
                // Desktop events retain native paths. Require a file-bearing
                // transfer (or WebKit's opaque native URI list) and reject
                // non-filesystem paths before entering the shared read queue.
                let transfer = event.data_transfer();
                // WebKit hides the URI payload for native file drops, while
                // Dioxus supplies the paths captured by its native handler.
                let native_files = transfer.get_data("text/uri-list").is_some_and(|uris| uris.is_empty() || uris.lines().any(|uri| uri.starts_with("file://")));
                if !transfer.files().is_empty() { ingest.call(event.files()); }
                else if native_files { ingest.call(event.files().into_iter().filter(|file| file.path().is_absolute()).collect()); }
            },
            div { class: "composer-inner",
            {state(&presentation)}
            div {
                class: "sr-only",
                "data-slot": "turn-announcement",
                role: "status",
                aria_live: "polite",
                aria_atomic: "true",
                if let Some(announcement) = &presentation.announcement {
                    "{announcement}"
                }
            }

            // One box, with the field and its footer inside it: a textarea
            // scrolls its bottom padding with its content, so a footer floating
            // over one is a band a third line of a prompt runs through.
            //
            // The commands the agent published, over the box they are typed
            // into — the one thing on this window that overlaps content and is
            // not a dialog, because what it is about is the half-typed word
            // underneath it.
            if !matches.is_empty() {
                div { class: "slash", "data-slot": "commands",
                    div { class: "slash-head",
                        span { class: "eyebrow", "available_commands_update" }
                        span { class: "slash-count", "{matches.len()} of {commands.len()}" }
                    }
                    div { class: "slash-list", role: "listbox", aria_label: "Commands the agent published",
                        for (position, command) in matches.iter().enumerate() {
                            button {
                                key: "{command.name}",
                                class: "slash-row",
                                r#type: "button",
                                role: "option",
                                aria_selected: position == highlight,
                                onclick: {
                                    let name = command.name.clone();
                                    move |_| fill(&name)
                                },
                                span { class: "slash-name", "/{command.name}" }
                                span { class: "slash-desc", "{command.description}" }
                            }
                        }
                    }
                }
            }

            div { class: "prompt-box",
                if attachments_advertised {
                    div { class: "prompt-image-picker",
                        label { class: "hint",
                            "Add or drop files · 8 attachments · 5 MiB each · 6 MiB total"
                            if embedded_advertised { span { "UTF-8 text files are embedded as contents. Preview shows the first 2000 characters." } }
                            if image_advertised { span { "Images can also be pasted from the clipboard." } }
                            input { id: picker_id.clone(), name: "prompt-media", r#type: "file", accept: accepted_media(image_advertised, audio_advertised, embedded_advertised), multiple: true,
                                onmounted: {
                                    let id = picker_id.clone();
                                    move |_| { let bridge = document::eval(IMAGE_DROP_BRIDGE); let _ = bridge.send((id.clone(), cfg!(all(feature = "desktop", target_os = "windows")))); }
                                },
                                aria_label: "Attach media", disabled: !ready || running,
                                onchange: move |event| {
                                    let files = event.files();
                                    let reset = document::eval("const id = await dioxus.recv(); const input = document.getElementById(id); if (input) input.value = ''; ");
                                    let _ = reset.send(picker_id.clone());
                                    ingest.call(files);
                                },
                            }
                        }
                        if !media.read().entries().is_empty() {
                            span { class: "hint", "{media.read().entries().len()} attachment rows · {media.read().bytes()} / 6291456 bytes ready" }
                        }
                    }
                }
                if let Some(problem) = media_problem() { p { class: "detail detail-warn", role: "status", "{problem}" } }
                if let Some(problem) = overflow() {
                    div { class: "detail detail-warn", role: "status", "{problem}"
                        button { r#type: "button", class: "btn btn-ghost btn-xs", onclick: move |_| overflow.set(None), "Dismiss attachment limit notice" }
                    }
                }
                div { class: "prompt-images",
                for entry in media.read().entries().to_vec() {
                    div { key: "{entry.id}", class: "prompt-image", "data-slot": "image-attachment", "data-image-id": "{entry.id}",
                        match &entry.state {
                            MediaState::Ready(held) => rsx! {
                                if held.is_text() { details { summary { "Text preview" } pre { class: "prompt-text-preview", "{held.text_preview()}" } } }
                                else if held.is_audio() { AudioPreview { media: held.clone() } }
                                else { img { src: held.preview(), alt: "Selected image: {held.name}" } }
                                span { "{held.name} · {held.mime} · {held.bytes} bytes" }
                            },
                            MediaState::Reading => rsx! { span { role: "status", "{entry.name} · Reading file…" } },
                            MediaState::Failed(problem) => rsx! { span { class: "detail detail-warn", role: "status", "{entry.name} · {problem}" } },
                        }
                        button { r#type: "button", class: "btn btn-ghost btn-xs", aria_label: "Remove attachment {entry.id}: {entry.name}",
                            onclick: move |_| {
                                media.write().remove(entry.id);
                                if let Some(bridge) = clipboard_bridge() { let _ = bridge.send(serde_json::json!({"cancel":entry.id})); }
                                queued.write().retain(|(id, _)| *id != entry.id);
                            }, "Remove"
                        }
                    }
                }
                }
                textarea {
                    rows: 3,
                    value: "{text}",
                    spellcheck: false,
                    disabled: !ready,
                    aria_label: "Prompt",
                    aria_describedby: show_shortcut.then_some("prompt-shortcut"),
                    placeholder: match (ready, connected) {
                        (true, _) => "Send a prompt to the agent…  type / for commands",
                        // The agent is there and nothing is open in it: what to
                        // do about that is beside the capability display, and
                        // saying which two things do it is the difference
                        // between a dead box and a next step.
                        (false, true) => {
                            "No session is open. session/new opens one, and a listed session is a way into one the agent already has."
                        }
                        (false, false) => "Launch an agent to open a session.",
                    },
                    oninput: move |event| {
                        text.set(event.value());
                        dismissed.set(false);
                        highlighted.set(0);
                    },
                    onkeydown: move |event| {
                        // The command list, while there is one: the arrows move
                        // through it, Enter takes the one under the keyboard
                        // rather than sending half a command, and Escape puts it
                        // away without touching what has been typed.
                        if !matches.is_empty() {
                            match event.key() {
                                Key::ArrowDown => {
                                    event.prevent_default();
                                    highlighted.set((highlight + 1) % matches.len());
                                    return;
                                }
                                Key::ArrowUp => {
                                    event.prevent_default();
                                    highlighted
                                        .set((highlight + matches.len() - 1) % matches.len());
                                    return;
                                }
                                Key::Escape => {
                                    event.prevent_default();
                                    dismissed.set(true);
                                    return;
                                }
                                Key::Enter | Key::Tab if !event.modifiers().shift() => {
                                    event.prevent_default();
                                    fill(&matches[highlight].name.clone());
                                    return;
                                }
                                _ => {}
                            }
                        }
                        // Enter sends, shift-enter is a newline: a prompt is
                        // usually a line, and the agents worth pointing this at
                        // are driven by pasted JSON often enough that the
                        // multi-line case has to stay typeable.
                        if submits_prompt(
                            event.key(),
                            event.modifiers().shift(),
                            event.is_composing(),
                        ) {
                            event.prevent_default();
                            send();
                        }
                    },
                }

                // The field's footer: what the keyboard does, and the one
                // control that starts or stops a turn. It is here whether or
                // not it has a hint to carry, because the Affordance is always
                // in it and the field's height must not change with the hint.
                div { class: "prompt-foot",
                    // The mode the next prompt goes out under, cycled from
                    // where the prompt is written — the drawing puts it here as
                    // well as on the rail, because what a reader checks before
                    // pressing Send is beside Send. Both controls drive the one
                    // call and read the one store, so they cannot disagree.
                    if let Some(mode) = &mode {
                        button {
                            class: "mode-cycle",
                            "data-slot": "mode-cycle",
                            r#type: "button",
                            aria_label: "Mode {mode.id}. Set the next mode the agent published",
                            title: "session/set_mode",
                            aria_disabled: (!mode.available).then_some("true"),
                            tabindex: (!mode.available).then_some("-1"),
                            onclick: {
                                let next = mode.next.clone();
                                let available = mode.available;
                                move |_| {
                                    if available && let Some(next) = next.clone() {
                                        on_set_mode.call(next);
                                    }
                                }
                            },
                            "mode: {mode.id}"
                        }
                    }
                    if show_shortcut {
                        span {
                            id: "prompt-shortcut",
                            class: "prompt-hint",
                            "↵ send · ⇧↵ newline · ↑↓ commands"
                        }
                    }

                    // Send and Stop are one DOM node. If the reader activates
                    // it from the keyboard, a Turn transition changes the
                    // Affordance without dropping focus to the document.
                    button {
                        id: "composer-affordance",
                        r#type: "button",
                        class: affordance.class(),
                        aria_label: affordance.name(),
                        title: affordance.name(),
                        // Native `disabled` can drop focus from a button. ARIA
                        // and tab order make an unavailable Affordance inert
                        // while the Turn changes underneath it.
                        aria_disabled: (!affordance_available).then_some("true"),
                        tabindex: (!affordance_available).then_some("-1"),
                        onclick: move |_| match affordance {
                            ComposerAffordance::Send if affordance_available => send(),
                            ComposerAffordance::Stop => on_stop.call(()),
                            ComposerAffordance::Send | ComposerAffordance::Stopping => {}
                        },
                        if matches!(affordance, ComposerAffordance::Stop | ComposerAffordance::Stopping) {
                            span { aria_hidden: "true", Icon { class: "icon", icon: HiStop } }
                            if matches!(affordance, ComposerAffordance::Stopping) { "Stopping" } else { "Stop" }
                        } else {
                            span { aria_hidden: "true", Icon { class: "icon", icon: HiPaperAirplane } }
                            // The word beside the glyph. It was a circle with an
                            // aeroplane in it, which is what a chat client draws
                            // when its send is the only control on the row; this
                            // one shares a footer with a keyboard hint and a
                            // turn's state, and the primary action of a region
                            // says what it does.
                            "Send"
                        }
                    }
                }
            }
            }
        }
    }
}

/// The mode the next prompt goes out under, as the composer's own control needs
/// it: what the agent last said it is, which one comes after it, and whether
/// there is anybody to ask.
///
/// **The store's, not the control's.** What is displayed is the mode the *agent*
/// stated (§7.6), and cycling asks for the next one the agent published — the
/// same call the rail's rows send, from the same list.
#[derive(Clone, Debug, PartialEq)]
pub struct Cycle {
    pub id: String,
    /// The next mode in the agent's own order, or `None` where it published one
    /// mode and there is nothing to cycle to.
    pub next: Option<v1::SessionModeId>,
    pub available: bool,
}

/// Plain Enter submits; Shift+Enter and an IME's candidate-confirming Enter are
/// text editing and remain the textarea's to answer.
fn submits_prompt(key: Key, shifted: bool, composing: bool) -> bool {
    key == Key::Enter && !shifted && !composing
}

/// Where the turn stands, in a line.
///
/// Every state the stream can put it in has one, including the two nobody
/// wants: a turn that ended without a stop reason says so rather than looking
/// like one that never started, and a turn waiting on a cancel says *that*,
/// because an agent that never resolves it is a conformance violation the
/// screen should be showing while it happens (§7.1).
///
/// A running turn says which of two things it is doing, because the difference
/// is whose move it is: an agent that is working, or an agent that has stopped
/// on a question and is waiting for the reader to answer it in the timeline
/// above.
pub(crate) struct TurnPresentation {
    tone: &'static str,
    label: &'static str,
    detail: Option<String>,
    announcement: Option<String>,
}

pub(crate) fn presented(
    turn: &TurnState,
    blocked: bool,
    problem: Option<&CallError>,
    ready: bool,
    connected: bool,
) -> TurnPresentation {
    let (tone, label, detail, announcement) = match turn {
        TurnState::Idle => match (problem, ready, connected) {
            // No turn has happened, and there is a reason there cannot be one
            // yet: the handshake behind the session is what failed, and the
            // console and the trace are where it is written down.
            (Some(problem), ..) => ("bad", "No session", Some(problem.to_string()), None),
            // A session that was closed or deleted, or one switched away from
            // that never arrived (§7.5). It is not a turn that has not started
            // — there is nothing for a turn to happen in — and it is not a
            // failure either, because it is usually what the reader just asked
            // for.
            (None, false, true) => (
                "off",
                "No session",
                Some("nothing is open to prompt into".to_owned()),
                None,
            ),
            (None, false, false) => ("off", "No agent", None, None),
            (None, true, _) => ("off", "No turn yet", None, None),
        },
        TurnState::InFlight if blocked => (
            "warn",
            "Waiting on you",
            Some("the agent stopped on a request in the timeline".to_owned()),
            None,
        ),
        TurnState::InFlight => ("live", "Running", None, None),
        // A cancel that is still owed an answer says the cancel, not the
        // question: the requests it left waiting have already been answered
        // `cancelled` on the way past (§7.2), and what the turn is waiting for
        // now is the agent.
        TurnState::Cancelling => (
            "warn",
            "Cancelling",
            Some("waiting for the agent to end the turn".to_owned()),
            None,
        ),
        // The stop reason verbatim, because it is the artifact: a client that
        // paraphrased it would be standing between the reader and the wire. A
        // cancelled turn reads differently from one that ran out, though — the
        // reader is the one who asked for it.
        TurnState::Ended(reason) => {
            let tone = if matches!(reason, v1::StopReason::Cancelled) {
                "warn"
            } else {
                "off"
            };
            let reason = stopped(reason);
            (
                tone,
                "Ended",
                Some(reason.to_owned()),
                Some(format!("Turn ended: {reason}.")),
            )
        }
        // The agent never said why, so neither does this: an inspector
        // reporting a stop reason the agent did not give is the one thing it
        // may not do.
        TurnState::Failed(error) => {
            let error = error.to_string();
            (
                "bad",
                "Ended without a reason",
                Some(error.clone()),
                Some(format!("Turn ended without a stop reason: {error}")),
            )
        }
        // `TurnState` is non-exhaustive, and a state this window has not heard
        // of must not be rendered as one it has.
        _ => ("off", "Unrecognized turn state", None, None),
    };

    TurnPresentation {
        tone,
        label,
        detail,
        announcement,
    }
}

/// Where the turn stands, as the badge in the Timeline's own header.
///
/// **The state of a region belongs to that region's header**, which is where a
/// desktop application puts it and where the drawing this window was rebuilt
/// from puts it (ADR 0009). The badge is the word alone; the sentence that
/// explains it stays above the box a reader would act in ([`state`]), so
/// nothing is said twice.
pub(crate) fn badge(presentation: &TurnPresentation) -> Element {
    let component_tone = badge_tone(presentation.tone);
    let tone = presentation.tone;
    let label = presentation.label;

    rsx! {
        // Soft, like every other badge that carries a meaning: a solid fill
        // would make where the turn stands the loudest thing on the screen, and
        // a turn that ended normally is not news.
        span { class: "badge badge-soft {component_tone} badge-sm {tone}", "data-slot": "turn-state",
            "{label}"
        }
    }
}

/// What is worth saying about where the turn stands beyond the word for it.
fn state(presentation: &TurnPresentation) -> Element {
    let tone = presentation.tone;

    rsx! {
        div { class: "turn {tone}",
            if let Some(detail) = &presentation.detail {
                span { class: "detail", "{detail}" }
            }
        }
    }
}

/// The commands whose names the draft has begun, or nothing at all.
///
/// **Only while the draft is one word beginning with a slash.** A command's
/// arguments are typed after its name, so the moment there is a space the
/// reader is writing the ask rather than choosing it — and a list still open
/// over the box would be a control competing with the Enter that sends.
/// Owned rather than borrowed, because the keydown handler that moves through
/// this list outlives the render that built it.
fn matching(commands: &[Command], draft: &str, dismissed: bool) -> Vec<Command> {
    if dismissed {
        return Vec::new();
    }
    let Some(typed) = draft.strip_prefix('/') else {
        return Vec::new();
    };
    if typed.contains(char::is_whitespace) {
        return Vec::new();
    }
    let typed = typed.to_lowercase();

    commands
        .iter()
        .filter(|command| command.name.to_lowercase().starts_with(&typed))
        .cloned()
        .collect()
}

fn badge_tone(tone: &str) -> &'static str {
    match tone {
        "bad" => "badge-error",
        "warn" => "badge-warning",
        "live" => "badge-success",
        // No role, so the badge keeps the base content colour its soft variant
        // gives it: a turn that has ended is a fact, not a grade.
        _ => "",
    }
}

/// The `stopReason` an agent gave, as it gave it.
///
/// Shared with the Timeline, which says the same thing about a turn that is
/// over as this says about the one that just ended: one wording, so the record
/// and the live state cannot describe the same stop two ways.
pub(crate) fn stopped(reason: &v1::StopReason) -> &'static str {
    match reason {
        v1::StopReason::EndTurn => "end_turn",
        v1::StopReason::MaxTokens => "max_tokens",
        v1::StopReason::MaxTurnRequests => "max_turn_requests",
        v1::StopReason::Refusal => "refusal",
        v1::StopReason::Cancelled => "cancelled",
        // `StopReason` is non-exhaustive: a reason this window has not been
        // taught is still a reason the agent gave, and the frame carrying it is
        // in the trace.
        _ => "a stop reason this window has no name for",
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use dioxus::core::{Mutation, ScopeId};

    use super::*;
    type ClipboardCallback = Callback<crate::clipboard::ClipboardInput, Option<Vec<Option<u64>>>>;
    thread_local! {
        pub(super) static CLIPBOARD_INPUT: RefCell<Option<ClipboardCallback>> = const { RefCell::new(None) };
    }

    #[tokio::test]
    async fn advertised_media_picker_and_drop_preserve_mixed_content_without_autoplay() {
        use dioxus::core::{Mutation, NoOpMutations};
        use dioxus::html::{
            PlatformEventData, SerializedFileData, SerializedFormData, SerializedFormObject,
            SerializedMouseData,
        };
        use std::{any::Any, rc::Rc};
        #[derive(Clone, Props)]
        struct HostProps {
            advertised: bool,
            audio: bool,
            #[props(default)]
            embedded: bool,
            sent: Rc<RefCell<Vec<Vec<v1::ContentBlock>>>>,
            reject: Rc<std::cell::Cell<bool>>,
        }
        impl PartialEq for HostProps {
            fn eq(&self, other: &Self) -> bool {
                self.advertised == other.advertised
                    && self.audio == other.audio
                    && self.embedded == other.embedded
                    && Rc::ptr_eq(&self.sent, &other.sent)
                    && Rc::ptr_eq(&self.reject, &other.reject)
            }
        }
        fn host(props: HostProps) -> Element {
            rsx! { Composer {turn:TurnState::Idle,ready:true,connected:true,blocked:false,problem:None,commands:vec![],mode:None,
                image_advertised:props.advertised,audio_advertised:props.audio,embedded_advertised:props.embedded,on_prompt:move |content| {
                    if props.reject.get() { Err(CallError::PromptTooLarge) }
                    else { props.sent.borrow_mut().push(content); Ok(()) }
                },on_stop: |_| {},on_set_mode: |_| {}}
            }
        }
        let sent = Rc::new(RefCell::new(Vec::new()));
        let reject = Rc::new(std::cell::Cell::new(false));
        let mut absent = VirtualDom::new_with_props(
            host,
            HostProps {
                advertised: false,
                audio: false,
                embedded: false,
                sent: sent.clone(),
                reject: reject.clone(),
            },
        );
        absent.rebuild_in_place();
        assert!(!dioxus_ssr::render(&absent).contains("Attach media"));
        let mut audio_only = VirtualDom::new_with_props(
            host,
            HostProps {
                advertised: false,
                audio: true,
                embedded: false,
                sent: sent.clone(),
                reject: reject.clone(),
            },
        );
        let audio_edits = audio_only.rebuild_to_vec().edits;
        let html = dioxus_ssr::render(&audio_only);
        assert!(html.contains("accept=\".wav,.mp3\""));
        assert!(!html.contains(".png"));
        let mut embedded_only = VirtualDom::new_with_props(
            host,
            HostProps {
                advertised: false,
                audio: false,
                embedded: true,
                sent: sent.clone(),
                reject: reject.clone(),
            },
        );
        embedded_only.rebuild_in_place();
        let html = dioxus_ssr::render(&embedded_only);
        assert!(html.contains("Attach media") && html.contains("UTF-8 text files"));
        assert!(!html.contains("Images can also be pasted"));
        set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let mut dom = VirtualDom::new_with_props(
            host,
            HostProps {
                advertised: true,
                audio: true,
                embedded: true,
                sent: sent.clone(),
                reject: reject.clone(),
            },
        );
        let edits = dom.rebuild_to_vec().edits;
        let picker = edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if name == "change" => Some(*id),
                _ => None,
            })
            .unwrap();
        let select = || {
            Event::new(
                Rc::new(PlatformEventData::new(Box::new(SerializedFormData::new(
                    "".into(),
                    vec![SerializedFormObject {
                        key: "image".into(),
                        text: None,
                        file: Some(SerializedFileData {
                            path: "wrong.jpg".into(),
                            size: 8,
                            last_modified: 0,
                            content_type: None,
                            contents: Some(b"\x89PNG\r\n\x1a\n".to_vec().into()),
                        }),
                    }],
                )))) as Rc<dyn Any>,
                true,
            )
        };
        let audio_picker = audio_edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if name == "change" => Some(*id),
                _ => None,
            })
            .unwrap();
        audio_only
            .runtime()
            .handle_event("change", select(), audio_picker);
        audio_only.wait_for_work().await;
        audio_only.render_immediate(&mut NoOpMutations);
        assert!(
            dioxus_ssr::render(&audio_only).contains("The Agent did not advertise image prompts.")
        );
        dom.runtime().handle_event("change", select(), picker);
        let mut mutations = Vec::new();
        for _ in 0..5 {
            dom.wait_for_work().await;
            mutations.extend(dom.render_immediate_to_vec().edits);
            if dioxus_ssr::render(&dom).contains("image-attachment") {
                break;
            }
        }
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains("data:image/png;base64,iVBORw0KGgo="),
            "{html}"
        );
        let remove = mutations
            .iter()
            .find_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if name == "click" => Some(*id),
                _ => None,
            })
            .expect("remove attachment click target");
        let click = || {
            Event::new(
                Rc::new(PlatformEventData::new(Box::<SerializedMouseData>::default()))
                    as Rc<dyn Any>,
                true,
            )
        };
        dom.runtime().handle_event("click", click(), remove);
        dom.render_immediate(&mut NoOpMutations);
        assert!(!dioxus_ssr::render(&dom).contains("image-attachment"));
        let paste_target = edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if name == "paste" => Some(*id),
                _ => None,
            })
            .unwrap();
        dom.runtime().handle_event(
            "paste",
            Event::new(
                Rc::new(PlatformEventData::new(Box::new(
                    dioxus::html::SerializedClipboardData {},
                ))) as Rc<dyn Any>,
                true,
            ),
            paste_target,
        );
        dom.runtime().handle_event("change", select(), picker);
        let input = CLIPBOARD_INPUT.with(|slot| slot.borrow().unwrap());
        let ids = input
            .call(crate::clipboard::ClipboardInput::Reserve {
                names: vec!["clipboard.gif".into()],
            })
            .unwrap();
        input.call(crate::clipboard::ClipboardInput::Finish {
            id: ids[0].unwrap(),
            data: Some("R0lGODlh".into()),
            error: None,
        });
        dom.wait_for_work().await;
        dom.render_immediate(&mut NoOpMutations);
        let html = dioxus_ssr::render(&dom);
        assert!(html.find("clipboard.gif").unwrap() < html.find("wrong.jpg").unwrap());
        drop(dom);
        let mut dom = VirtualDom::new_with_props(
            host,
            HostProps {
                advertised: true,
                audio: true,
                embedded: true,
                sent: sent.clone(),
                reject: reject.clone(),
            },
        );
        let edits = dom.rebuild_to_vec().edits;
        let picker = edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if name == "change" => Some(*id),
                _ => None,
            })
            .unwrap();
        dom.runtime().handle_event("change", select(), picker);
        for _ in 0..5 {
            dom.wait_for_work().await;
            dom.render_immediate(&mut NoOpMutations);
            if dioxus_ssr::render(&dom).contains("image-attachment") {
                break;
            }
        }
        let send = edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::SetAttribute {
                    name: "aria-label",
                    value: dioxus::core::AttributeValue::Text(value),
                    id,
                    ..
                } if value == "Send prompt" => Some(*id),
                _ => None,
            })
            .unwrap();
        let drop_target = edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if name == "drop" => Some(*id),
                _ => None,
            })
            .unwrap();
        let drop_files = |files| {
            Event::new(
                Rc::new(PlatformEventData::new(Box::new(
                    dioxus::html::SerializedDragData {
                        mouse: Default::default(),
                        data_transfer: dioxus::html::SerializedDataTransfer {
                            items: vec![],
                            files,
                            effect_allowed: "all".into(),
                            drop_effect: "copy".into(),
                        },
                    },
                ))) as Rc<dyn Any>,
                true,
            )
        };
        dom.runtime().handle_event(
            "drop",
            drop_files(vec![
                SerializedFileData {
                    path: "second.wav".into(),
                    size: 12,
                    last_modified: 0,
                    content_type: None,
                    contents: Some(b"RIFF\x04\0\0\0WAVE".to_vec().into()),
                },
                SerializedFileData {
                    path: "broken.png".into(),
                    size: 3,
                    last_modified: 0,
                    content_type: None,
                    contents: Some(b"bad".to_vec().into()),
                },
                SerializedFileData {
                    path: "/tmp/context #.rs".into(),
                    size: 15,
                    last_modified: 0,
                    content_type: None,
                    contents: Some(b"<script>\r\ntext".to_vec().into()),
                },
            ]),
            drop_target,
        );
        dom.runtime().handle_event("click", click(), send);
        assert!(
            sent.borrow().is_empty(),
            "a newly queued drop blocks Send before rerender"
        );
        let mut added = Vec::new();
        for _ in 0..8 {
            dom.wait_for_work().await;
            added.extend(dom.render_immediate_to_vec().edits);
            if dioxus_ssr::render(&dom).contains("Text preview") {
                break;
            }
        }
        assert_eq!(
            dioxus_ssr::render(&dom)
                .matches("data-slot=\"image-attachment\"")
                .count(),
            4
        );
        let preview = dioxus_ssr::render(&dom);
        assert!(
            preview.contains("Text preview") && preview.contains("&#60;script&#62;"),
            "{preview}"
        );
        assert!(
            preview.contains("<audio")
                && preview.contains("controls")
                && preview.contains("preload=\"none\"")
        );
        assert!(!preview.contains("autoplay"));
        let audio = added
            .iter()
            .find_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if name == "error" => Some(*id),
                _ => None,
            })
            .unwrap();
        struct PlaybackError;
        impl dioxus::html::HasImageData for PlaybackError {
            fn load_error(&self) -> bool {
                true
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
        }
        dom.runtime().handle_event(
            "error",
            Event::new(
                Rc::new(PlatformEventData::new(Box::new(
                    dioxus::html::SerializedImageData::from(&dioxus::html::ImageData::new(
                        PlaybackError,
                    )),
                ))) as Rc<dyn Any>,
                false,
            ),
            audio,
        );
        dom.render_immediate(&mut NoOpMutations);
        assert!(dioxus_ssr::render(&dom).contains("Playback is unavailable in this WebView"));
        dom.runtime().handle_event("click", click(), send);
        assert!(
            sent.borrow().is_empty(),
            "failed files block silent partial submission"
        );
        let dismiss = added
            .iter()
            .find_map(|edit| match edit {
                Mutation::SetAttribute {
                    name: "aria-label",
                    value: dioxus::core::AttributeValue::Text(value),
                    id,
                    ..
                } if value.ends_with("broken.png") => Some(*id),
                _ => None,
            })
            .unwrap();
        dom.runtime().handle_event("click", click(), dismiss);
        dom.render_immediate(&mut NoOpMutations);
        dom.runtime()
            .handle_event("drop", drop_files(vec![]), drop_target);
        dom.render_immediate(&mut NoOpMutations);
        assert_eq!(
            dioxus_ssr::render(&dom)
                .matches("data-slot=\"image-attachment\"")
                .count(),
            3,
            "non-file drop adds nothing"
        );
        reject.set(true);
        dom.runtime().handle_event("click", click(), send);
        dom.render_immediate(&mut NoOpMutations);
        assert!(sent.borrow().is_empty());
        assert!(
            dioxus_ssr::render(&dom).contains("image-attachment"),
            "local refusal preserves draft"
        );
        reject.set(false);
        let textarea = edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if name == "input" => Some(*id),
                _ => None,
            })
            .unwrap();
        dom.runtime().handle_event(
            "input",
            Event::new(
                Rc::new(PlatformEventData::new(Box::new(SerializedFormData::new(
                    "Describe these".into(),
                    vec![],
                )))) as Rc<dyn Any>,
                true,
            ),
            textarea,
        );
        dom.render_immediate(&mut NoOpMutations);
        dom.runtime().handle_event("click", click(), send);
        dom.render_immediate(&mut NoOpMutations);
        assert_eq!(sent.borrow().len(), 1);
        assert!(
            matches!(&sent.borrow()[0][..],[v1::ContentBlock::Text(text),v1::ContentBlock::Image(image),v1::ContentBlock::Audio(second),v1::ContentBlock::Resource(resource)] if text.text=="Describe these" && image.data=="iVBORw0KGgo=" && image.mime_type=="image/png" && second.data=="UklGRgQAAABXQVZF" && second.mime_type=="audio/wav" && matches!(&resource.resource,v1::EmbeddedResourceResource::TextResourceContents(file) if file.text=="<script>\r\ntext" && file.uri=="file:///tmp/context%20%23.rs"))
        );
        assert!(!dioxus_ssr::render(&dom).contains("image-attachment"));
    }

    thread_local! {
        static LIVE_TURN: RefCell<Option<Signal<TurnState>>> = const { RefCell::new(None) };
    }

    #[component]
    fn StatefulComposer() -> Element {
        let turn = use_signal(|| TurnState::Idle);
        use_hook(|| LIVE_TURN.with(|slot| *slot.borrow_mut() = Some(turn)));
        rsx! {
            Composer {
                turn: turn(),
                ready: true,
                connected: true,
                blocked: false,
                problem: None,
                commands: Vec::new(),
                mode: None,
                on_prompt: move |_: Vec<v1::ContentBlock>| Ok(()),
                on_stop: move |()| {},
                on_set_mode: move |_| {},
            }
        }
    }

    fn shown(turn: TurnState, ready: bool, connected: bool, blocked: bool) -> String {
        #[component]
        fn Host(turn: TurnState, ready: bool, connected: bool, blocked: bool) -> Element {
            rsx! {
                Composer {
                    turn,
                    ready,
                    connected,
                    blocked,
                    problem: None,
                    commands: Vec::new(),
                    mode: None,
                    on_prompt: move |_: Vec<v1::ContentBlock>| Ok(()),
                    on_stop: move |()| {},
                    on_set_mode: move |_| {},
                }
            }
        }

        let mut dom = VirtualDom::new_with_props(
            Host,
            HostProps {
                turn,
                ready,
                connected,
                blocked,
            },
        );
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    fn shown_with_problem(problem: CallError) -> String {
        #[component]
        fn Host(problem: CallError) -> Element {
            rsx! {
                Composer {
                    turn: TurnState::Idle,
                    ready: false,
                    connected: true,
                    blocked: false,
                    problem: Some(problem),
                    commands: Vec::new(),
                    mode: None,
                    on_prompt: move |_: Vec<v1::ContentBlock>| Ok(()),
                    on_stop: move |()| {},
                    on_set_mode: move |_| {},
                }
            }
        }

        let mut dom = VirtualDom::new_with_props(Host, HostProps { problem });
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    /// Where the turn stands, as the Timeline's header draws it.
    fn stated(turn: TurnState, ready: bool, connected: bool, blocked: bool) -> String {
        #[component]
        fn Host(turn: TurnState, ready: bool, connected: bool, blocked: bool) -> Element {
            rsx! {
                {badge(&presented(&turn, blocked, None, ready, connected))}
            }
        }

        let mut dom = VirtualDom::new_with_props(
            Host,
            HostProps {
                turn,
                ready,
                connected,
                blocked,
            },
        );
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    #[test]
    fn composer_uses_compact_daisyui_controls_in_one_bounded_field() {
        let ready = shown(TurnState::Idle, true, true, false);
        assert!(
            // One box, with the field and its footer inside it: the radius and
            // the boundary are the box's, because the footer is in flow under
            // the text rather than floating over it.
            ready.contains(r#"class="prompt-box""#)
                && ready.contains("<textarea")
                && ready.contains(r#"class="prompt-foot""#)
                && ready.contains(r#"class="btn btn-md btn-filled send""#),
            "the composer uses compact daisyUI controls in one bounded field: {ready}"
        );
        assert!(
            ready.contains(r#"aria-label="Prompt""#)
                && ready.contains(r#"aria-label="Send prompt""#)
                && ready.contains(r#"title="Send prompt""#)
                && ready.contains(r#"<svg class="icon""#)
                // The word beside the glyph: the primary action of a region
                // says what it does (ADR 0009).
                && ready.contains(">Send</button>"),
            "the Prompt and its Send Affordance retain durable accessible names: {ready}"
        );
        assert!(
            ready.contains("Send a prompt to the agent\u{2026}  type / for commands")
                && ready.contains(r#"aria-describedby="prompt-shortcut""#)
                && ready.contains("↵ send · ⇧↵ newline · ↑↓ commands")
                && ready.contains(r#"aria-disabled="true""#)
                && ready.contains(r#"tabindex="-1""#),
            "blank-prompt prevention and inset keyboard guidance remain visible: {ready}"
        );

        let running = shown(TurnState::InFlight, true, true, false);
        assert!(
            running.contains(r#"class="btn btn-md btn-quiet btn-bad send stop""#)
                && running.contains(r#"<svg class="icon""#)
                && running.contains("Stop"),
            "a running Turn exposes a destructive icon-and-text Stop Affordance: {running}"
        );
        let cancelling = shown(TurnState::Cancelling, true, true, false);
        assert!(
            cancelling.contains("Stopping")
                && cancelling.contains(r#"aria-disabled="true""#)
                && cancelling.contains(r#"tabindex="-1""#),
            "cancellation remains pending until the Agent ends the Turn: {cancelling}"
        );
    }

    #[test]
    fn where_the_turn_stands_is_one_badge_and_the_sentence_under_it_is_not_a_second() {
        // **The word is the header's and the sentence is the composer's**
        // (ADR 0009). The badge says where the turn stands at the far end of the
        // region's own header, which is where a desktop application puts the
        // state of what a region is showing; the sentence that explains it sits
        // above the box a reader would act in. Neither says the other's half, so
        // nothing on the screen is drawn twice.
        for (turn, blocked, words, component) in [
            (
                TurnState::InFlight,
                false,
                "Running",
                "badge badge-soft badge-success",
            ),
            (
                TurnState::InFlight,
                true,
                "Waiting on you",
                "badge badge-soft badge-warning",
            ),
            (
                TurnState::Cancelling,
                false,
                "Cancelling",
                "badge badge-soft badge-warning",
            ),
            (
                TurnState::Ended(v1::StopReason::Refusal),
                false,
                "Ended",
                "badge badge-soft",
            ),
            (
                TurnState::Failed(CallError::Disconnected),
                false,
                "Ended without a reason",
                "badge badge-soft badge-error",
            ),
        ] {
            let html = stated(turn.clone(), true, true, blocked);
            assert!(
                html.contains(words)
                    && html.contains(component)
                    && html.contains(r#"data-slot="turn-state""#),
                "the {words} Turn state uses a fitting daisyUI status: {html}"
            );
            // And the detail, where it has one, is the composer's — never the
            // badge's, which carries the word alone.
            let composed = shown(turn, true, true, blocked);
            assert!(
                !composed.contains("turn-badge"),
                "the composer draws no badge of its own: {composed}"
            );
        }

        // The agent's own word for how a turn ended, verbatim, beside the state.
        let ended = shown(TurnState::Ended(v1::StopReason::Refusal), true, true, false);
        assert!(
            ended.contains("refusal"),
            "the stop reason is the agent's own word: {ended}"
        );

        let disconnected = shown(TurnState::Idle, false, false, false);
        assert!(
            stated(TurnState::Idle, false, false, false).contains("No agent")
                && disconnected.contains("Launch an agent"),
            "the disconnected state remains explicit: {disconnected}"
        );
        let no_session = shown(TurnState::Idle, false, true, false);
        assert!(
            stated(TurnState::Idle, false, true, false).contains("No session")
                && no_session.contains("session/new opens one"),
            "the no-Session state remains distinct: {no_session}"
        );

        // And a handshake that never reached a session says why, in the agent's
        // own words: a launch that failed must not fail silently, which is the
        // one reason this surface is drawn with no session at all
        // (`timeline.rs`).
        let refused = shown_with_problem(CallError::Disconnected);
        assert!(
            refused.contains(&CallError::Disconnected.to_string()),
            "the reason there is no session is the error itself: {refused}"
        );
    }

    /// The composer with a published command list behind it.
    fn offering(commands: Vec<Command>, draft: &str) -> Vec<Command> {
        matching(&commands, draft, false)
    }

    fn published() -> Vec<Command> {
        [
            ("init", "Generate an AGENTS.md for this repository"),
            ("compact", "Summarize the conversation to free context"),
            ("clear", "Clear the session history"),
        ]
        .into_iter()
        .map(|(name, description)| Command {
            name: name.to_owned(),
            description: description.to_owned(),
        })
        .collect()
    }

    #[test]
    fn the_commands_offered_are_the_agents_own_and_only_while_one_is_being_typed() {
        // **The agent's list and nothing else.** A completion this window
        // invented would be a command nobody can run.
        assert_eq!(
            offering(published(), "/c")
                .into_iter()
                .map(|command| command.name)
                .collect::<Vec<_>>(),
            vec!["compact".to_owned(), "clear".to_owned()],
            "a prefix narrows the agent's own names"
        );
        assert_eq!(
            offering(published(), "/").len(),
            3,
            "a bare slash offers all"
        );
        assert!(
            offering(published(), "").is_empty(),
            "an ordinary prompt is not a command"
        );
        // A command's arguments are typed after its name, so the moment there is
        // a space the reader is writing the ask rather than choosing it — and a
        // list still open over the box would compete with the Enter that sends.
        assert!(
            offering(published(), "/init ").is_empty(),
            "the list closes when the name is complete"
        );
        assert!(
            offering(published(), "/nope").is_empty(),
            "a name no agent published matches nothing"
        );
        // Waved away, and the draft is left exactly as it was.
        assert!(matching(&published(), "/c", true).is_empty());
    }

    #[test]
    fn a_published_command_list_is_drawn_over_the_box_it_is_typed_in() {
        #[component]
        fn Host() -> Element {
            rsx! {
                Composer {
                    turn: TurnState::Idle,
                    ready: true,
                    connected: true,
                    blocked: false,
                    problem: None,
                    commands: published(),
                    mode: None,
                    on_prompt: move |_: Vec<v1::ContentBlock>| Ok(()),
                    on_stop: move |()| {},
                    on_set_mode: move |_| {},
                }
            }
        }

        let mut dom = VirtualDom::new(Host);
        dom.rebuild_in_place();
        let empty = dioxus_ssr::render(&dom);
        // Nothing is offered until a command is being typed: the control appears
        // with the question rather than sitting over the box.
        assert!(
            !empty.contains(r#"data-slot="commands""#),
            "an ordinary draft draws no list: {empty}"
        );
    }

    #[test]
    fn the_composer_affordance_stays_in_the_dom_across_turn_states() {
        let mut dom = VirtualDom::new(StatefulComposer);
        let initial = dom.rebuild_to_vec().edits;
        let affordance = initial
            .iter()
            .find_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if name == "click" => Some(*id),
                _ => None,
            })
            .expect("the composer Affordance's click listener");

        for turn in [
            TurnState::InFlight,
            TurnState::Cancelling,
            TurnState::Ended(v1::StopReason::EndTurn),
        ] {
            dom.in_scope(ScopeId::ROOT, || {
                let mut live = LIVE_TURN.with(|slot| slot.borrow().expect("the mounted Turn"));
                live.set(turn);
            });
            let edits = dom.render_immediate_to_vec().edits;
            let disturbed = edits.iter().any(|edit| {
                let target = match edit {
                    Mutation::Remove { id }
                    | Mutation::ReplaceWith { id, .. }
                    | Mutation::NewEventListener { id, .. }
                    | Mutation::RemoveEventListener { id, .. } => id,
                    _ => return false,
                };
                target == &affordance
            });
            assert!(
                !disturbed,
                "the focused Send/Stop Affordance must remain the same DOM element: {edits:?}"
            );
            let html = dioxus_ssr::render(&dom);
            assert_eq!(html.matches(r#"id="composer-affordance""#).count(), 1);
        }
    }

    #[test]
    fn enter_submits_only_outside_ime_composition_and_without_shift() {
        assert!(submits_prompt(Key::Enter, false, false));
        assert!(!submits_prompt(Key::Enter, true, false));
        assert!(!submits_prompt(Key::Enter, false, true));
        assert!(!submits_prompt(Key::Tab, false, false));
    }

    #[test]
    fn turn_endings_are_announced_without_making_the_stream_live() {
        let idle = shown(TurnState::Idle, true, true, false);
        let ended = shown(
            TurnState::Ended(v1::StopReason::MaxTokens),
            true,
            true,
            false,
        );
        let failed = shown(
            TurnState::Failed(CallError::Disconnected),
            true,
            true,
            false,
        );

        for html in [&idle, &ended, &failed] {
            assert!(
                html.contains(r#"data-slot="turn-announcement""#)
                    && html.contains(r#"role="status""#)
                    && html.contains(r#"aria-live="polite""#),
                "the persistent announcement region remains mounted: {html}"
            );
        }
        assert!(!idle.contains("Turn ended:"), "{idle}");
        assert!(ended.contains("Turn ended: max_tokens."), "{ended}");
        assert!(
            failed.contains("Turn ended without a stop reason:"),
            "{failed}"
        );
    }
}
