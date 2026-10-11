//! The composer, and where the turn stands. See `docs/architecture.md` §9.
//!
//! The turn's state is read from [`TurnState`], which the update stream drives;
//! nothing here waits on the prompt call. Stop sends `session/cancel`, and the
//! turn is over only when the agent says so.

use crate::attachment::{Attachment, AttachmentPreview};
use crate::attachment_draft::{AttachmentDraft, AttachmentState};
use acp_inspector_core::{CallError, TurnState, v1};
use dioxus::html::{FileData, HasFileData};

/// Native file drops must be accepted synchronously, before IPC. Do not let a
/// synthetic or same-document URI drag reuse Dioxus's last native file paths.
const ATTACHMENT_DROP_BRIDGE: &str = r#"
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

use crate::disclosure::{Role, icon};

/// Picker hints follow each advertisement; signature validation is independent.
fn accepted_attachments(image: bool, audio: bool, embedded: bool) -> &'static str {
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
fn AudioPreview(name: String, src: String) -> Element {
    let mut unavailable = use_signal(|| false);
    rsx! { div { class: "prompt-audio",
        audio { controls: true, preload: "none", src, aria_label: "Preview audio: {name}",
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

/// One command from the agent's latest `available_commands_update`.
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
    /// Whether there is an agent, to tell "no agent" from "no session open".
    connected: bool,
    /// Whether a blocking request is waiting on the user. Derived from pending
    /// requests, not a turn state: the turn is still running.
    blocked: bool,
    /// Why there is no session, when the handshake that opens one failed.
    problem: Option<CallError>,
    /// The agent's latest published commands; the list is hidden when empty.
    commands: Vec<Command>,
    /// The mode the next prompt goes out under, or `None` for no control.
    mode: Option<Cycle>,
    #[props(default)] image_advertised: bool,
    #[props(default)] audio_advertised: bool,
    #[props(default)] embedded_advertised: bool,
    on_prompt: Callback<Vec<v1::ContentBlock>, Result<(), CallError>>,
    on_stop: EventHandler<()>,
    /// Sends `session/set_mode`; the same handler the rail's rows call.
    on_set_mode: EventHandler<v1::SessionModeId>,
) -> Element {
    let mut text = use_signal(String::new);
    let mut draft = use_signal(AttachmentDraft::default);
    let mut queued = use_signal(std::collections::VecDeque::<(u64, FileData)>::new);
    let mut reading = use_signal(|| false);
    let mut attachment_problem = use_signal(|| None::<String>);
    let mut overflow = use_signal(|| None::<String>);
    let mut clipboard_bridge = use_signal(|| None::<document::Eval>);
    let mut paste_waiting = use_signal(|| 0usize);
    let mut deferred_files = use_signal(Vec::<(usize, Vec<FileData>)>::new);
    // The resource link being typed: a URI, a name, a MIME type.
    let mut link_uri = use_signal(String::new);
    let mut link_name = use_signal(String::new);
    let mut link_mime = use_signal(String::new);
    let mut link_problem = use_signal(|| None::<String>);
    let mut link_open = use_signal(|| false);
    let picker_id = use_hook(|| {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        format!(
            "prompt-attachment-{}",
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )
    });
    // The command the keyboard is on, and whether the list was dismissed.
    let mut highlighted = use_signal(|| 0usize);
    let mut dismissed = use_signal(|| false);
    let running = turn.is_running();
    let attachments_advertised = image_advertised || audio_advertised || embedded_advertised;
    let reserve = use_callback(move |names: Vec<String>| {
        let mut rejected = Vec::new();
        let ids = names
            .into_iter()
            .map(|name| match draft.write().reserve(name.clone()) {
                Ok(id) => Some(id),
                Err(_) => {
                    rejected.push(name);
                    None
                }
            })
            .collect::<Vec<_>>();
        if !rejected.is_empty() {
            overflow.set(Some(format!(
                "Not added, 8 files at most: {}. The other files are still attached.",
                rejected.join(", ")
            )));
        }
        attachment_problem.set(None);
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
                let result = Attachment::from_file(file).await.and_then(|attachment| {
                    attachment.require_advertisement(
                        image_advertised,
                        audio_advertised,
                        embedded_advertised,
                    )?;
                    Ok(attachment)
                });
                let Ok(mut draft) = draft.try_write() else {
                    return;
                };
                draft.finish(id, result);
            }
            reading.set(false);
        });
    });
    // A resource link takes a row (and counts toward the row limit) but no bytes.
    let add_link = use_callback(move |()| {
        if !ready || running {
            return;
        }
        let typed = Attachment::link(&link_uri.peek(), &link_name.peek(), &link_mime.peek());
        match typed {
            Ok(link) => {
                link_problem.set(None);
                if let Some(Some(id)) = reserve.call(vec![link.name.clone()]).first() {
                    draft.write().finish(*id, Ok(link));
                    link_uri.set(String::new());
                    link_name.set(String::new());
                    link_mime.set(String::new());
                }
            }
            Err(problem) => link_problem.set(Some(problem)),
        }
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
                let name = draft
                    .read()
                    .entries()
                    .iter()
                    .find(|entry| entry.id == id)
                    .map(|entry| entry.name.clone());
                if let Some(name) = name {
                    draft
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
    // ACP has one turn per session at a time.
    let sendable = ready
        && !running
        && draft.read().sendable()
        && overflow.read().is_none()
        && (!text().trim().is_empty() || !draft.read().entries().is_empty());
    let affordance = ComposerAffordance::for_turn(&turn);
    let affordance_available = affordance.is_available(sendable);
    let show_shortcut = ready && !running;
    let presentation = presented(&turn, blocked, problem.as_ref(), ready, connected);
    let matches = matching(&commands, &text(), dismissed());
    // A `/word` is being typed: the list opens even when nothing matches, so
    // the slash never does nothing.
    let slashing = ready && naming_a_command(&text(), dismissed());
    let offers_commands = !commands.is_empty();
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
            let Some(attachments) = draft.read().content() else {
                return;
            };
            if prompt.trim().is_empty() && attachments.is_empty() {
                return;
            }
            let mut content = Vec::new();
            if !prompt.trim().is_empty() {
                content.push(v1::ContentBlock::from(prompt));
            }
            // Rows were checked against the advertisement when added.
            content.extend(attachments);
            if let Err(error) = on_prompt.call(content) {
                attachment_problem.set(Some(error.to_string()));
                return;
            }
            text.set(String::new());
            attachment_problem.set(None);
            draft.write().clear();
            queued.write().clear();
        }
    };
    // Choosing a command fills the box rather than sending, so its arguments
    // can be typed.
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
                    let _ = bridge.send((id.clone(), crate::attachment::MAX_ATTACHMENT_BYTES));
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

            // The command list overlays the box it completes.
            if slashing {
                div { class: "slash", "data-slot": "commands",
                    div { class: "slash-head",
                        span { class: "heading", title: "available_commands_update", "Agent commands" }
                        span { class: "slash-count", "{matches.len()} of {commands.len()}" }
                    }
                    if matches.is_empty() {
                        p { class: "slash-empty hint", "data-slot": "no-commands",
                            if offers_commands {
                                "No command the agent published starts with that."
                            } else {
                                "The agent hasn't published any commands. It can, at any time, with "
                                code { "available_commands_update" }
                                "."
                            }
                        }
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
                textarea {
                    rows: 3,
                    value: "{text}",
                    spellcheck: false,
                    disabled: !ready,
                    aria_label: "Prompt",
                    aria_describedby: show_shortcut.then_some("prompt-shortcut"),
                    placeholder: match (ready, connected) {
                        (true, _) if offers_commands => "Send a prompt to the agent…  type / for commands",
                        (true, _) => "Send a prompt to the agent…",
                        (false, true) => "Open a session to start prompting.",
                        (false, false) => "Start your agent to open a session.",
                    },
                    oninput: move |event| {
                        text.set(event.value());
                        dismissed.set(false);
                        highlighted.set(0);
                    },
                    onkeydown: move |event| {
                        // While the command list is open, arrows move, Enter or
                        // Tab fills, and Escape dismisses it, empty or not.
                        if slashing && event.key() == Key::Escape {
                            event.prevent_default();
                            dismissed.set(true);
                            return;
                        }
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
                if let Some(problem) = attachment_problem() { p { class: "detail detail-warn", role: "status", "{problem}" } }
                if let Some(problem) = overflow() {
                    div { class: "detail detail-warn", role: "status", "{problem}"
                        button { r#type: "button", class: "btn btn-ghost btn-xs", aria_label: "Dismiss the attachment limit notice", onclick: move |_| overflow.set(None), "Dismiss" }
                    }
                }
                div { class: "prompt-attachments",
                for entry in draft.read().entries().to_vec() {
                    div { key: "{entry.id}", class: "prompt-attachment", "data-slot": "attachment", "data-attachment-id": "{entry.id}",
                        match &entry.state {
                            AttachmentState::Ready(held) => rsx! {
                                {match held.preview() {
                                    AttachmentPreview::Pdf => rsx! { span { class: "hint", "PDF · original file embedded" } },
                                    AttachmentPreview::Text { excerpt } => rsx! { details { summary { "Text preview" } pre { class: "prompt-text-preview", "{excerpt}" } } },
                                    AttachmentPreview::Audio { src } => rsx! { AudioPreview { name: held.name.clone(), src } },
                                    AttachmentPreview::Image { src } => rsx! { img { src, alt: "Selected image: {held.name}" } },
                                    AttachmentPreview::Blob => rsx! { span { class: "hint", "Binary file · original bytes embedded" } },
                                    AttachmentPreview::Link => rsx! { span { class: "hint", "Resource link · the agent is sent the URI, not the contents" } },
                                }}
                                span { "{held.describe()}" }
                            },
                            AttachmentState::Reading => rsx! { span { role: "status", "{entry.name} · Reading file…" } },
                            AttachmentState::Failed(problem) => rsx! { span { class: "detail detail-warn", role: "status", "{entry.name} · {problem}" } },
                        }
                        button { r#type: "button", class: "btn btn-ghost btn-xs", aria_label: "Remove attachment {entry.id}: {entry.name}",
                            onclick: move |_| {
                                draft.write().remove(entry.id);
                                if let Some(bridge) = clipboard_bridge() { let _ = bridge.send(serde_json::json!({"cancel":entry.id})); }
                                queued.write().retain(|(id, _)| *id != entry.id);
                            }, "Remove"
                        }
                    }
                }
                }

                // Always drawn, so the field's height does not change with the
                // hint.
                div { class: "prompt-foot",
                    // Mirrors the rail's mode row: same call, same store.
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
                    if attachments_advertised {
                        div { class: "prompt-attach",
                            // The limits and the other ways in are the tooltip's.
                            label { class: "btn btn-sm btn-quiet attach",
                                title: attachment_rules(embedded_advertised, image_advertised),
                                aria_disabled: (!ready || running).then_some("true"),
                                {icon(Role::Attach)}
                                "Attach"
                                input { id: picker_id.clone(), class: "sr-only", name: "prompt-attachment", r#type: "file", accept: accepted_attachments(image_advertised, audio_advertised, embedded_advertised), multiple: true,
                                    onmounted: {
                                        let id = picker_id.clone();
                                        move |_| { let bridge = document::eval(ATTACHMENT_DROP_BRIDGE); let _ = bridge.send((id.clone(), cfg!(all(feature = "desktop", target_os = "windows")))); }
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
                            if !draft.read().entries().is_empty() {
                                span { class: "hint", "data-slot": "attachment-total",
                                    "{files(draft.read().entries().len())} · {megabytes(draft.read().bytes() as u64)} of 6 MB"
                                }
                            }
                        }
                    }
                    if show_shortcut {
                        span {
                            id: "prompt-shortcut",
                            class: "prompt-hint",
                            if offers_commands { "↵ send · ⇧↵ newline · ↑↓ commands" } else { "↵ send · ⇧↵ newline" }
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
                            {icon(Role::Stop)}
                            if matches!(affordance, ComposerAffordance::Stopping) { "Stopping" } else { "Stop" }
                        } else {
                            {icon(Role::Send)}
                            "Send"
                        }
                    }
                }

                // Below the footer, so the field and Send keep their tab order.
                if ready {
                    details { class: "prompt-link", "data-slot": "resource-link",
                        // The fields exist only while the disclosure is open.
                        ontoggle: move |_| link_open.toggle(),
                        summary { class: "hint", "Add a resource link…" }
                        if link_open() { div { class: "prompt-link-fields",
                            input { class: "input input-sm mono", r#type: "url", aria_label: "Resource URI",
                                placeholder: "file:///path/to/notes.md", spellcheck: false, value: "{link_uri}",
                                oninput: move |event| link_uri.set(event.value()),
                                onkeydown: move |event| if event.key() == Key::Enter { event.prevent_default(); add_link.call(()); },
                            }
                            input { class: "input input-sm", r#type: "text", aria_label: "Resource name",
                                placeholder: "name (optional)", value: "{link_name}",
                                oninput: move |event| link_name.set(event.value()),
                                onkeydown: move |event| if event.key() == Key::Enter { event.prevent_default(); add_link.call(()); },
                            }
                            input { class: "input input-sm mono", r#type: "text", aria_label: "Resource MIME type",
                                placeholder: "MIME type (optional)", spellcheck: false, value: "{link_mime}",
                                oninput: move |event| link_mime.set(event.value()),
                                onkeydown: move |event| if event.key() == Key::Enter { event.prevent_default(); add_link.call(()); },
                            }
                            button { r#type: "button", class: "btn btn-xs btn-quiet", "data-slot": "add-link",
                                disabled: running, onclick: move |_| add_link.call(()), "Add link"
                            }
                        }
                        span { class: "hint", "Sent as a resource_link: the agent gets the URI and reads it itself. Every agent accepts links." }
                        if let Some(problem) = link_problem() { p { class: "detail detail-warn", role: "status", "{problem}" } }
                        }
                    }
                }
            }
            }
        }
    }
}

/// The mode the composer's control shows: the agent's current mode, the next
/// one it published, and whether there is anyone to ask (§7.6).
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

/// Where the turn stands: a tone, a label, a detail and what to announce.
///
/// Every turn state has one, including a pending cancel (an agent that never
/// resolves it is a violation worth seeing) and a running turn blocked on the
/// reader.
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
            // The handshake behind the session failed.
            (Some(problem), ..) => ("bad", "No session", Some(problem.to_string()), None),
            // No session open (closed, deleted, or a switch that never
            // arrived); usually what the reader asked for, so not a failure.
            (None, false, true) => ("off", "No session", Some("not open".to_owned()), None),
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
        // Pending requests were already answered `cancelled`; the turn now
        // waits on the agent.
        TurnState::Cancelling => (
            "warn",
            "Cancelling",
            Some("waiting for the agent to end the turn".to_owned()),
            None,
        ),
        // The badge is a plain word; the detail is the stop reason verbatim.
        TurnState::Ended(reason) => {
            let (tone, label) = match reason {
                v1::StopReason::EndTurn => ("off", "Done"),
                v1::StopReason::Cancelled => ("warn", "Cancelled"),
                v1::StopReason::Refusal => ("off", "Refused"),
                _ => ("off", "Stopped"),
            };
            let reason = stopped(reason);
            (
                tone,
                label,
                Some(reason.to_owned()),
                Some(format!("Turn ended: {reason}.")),
            )
        }
        // The prompt call failed; no stop reason is invented.
        TurnState::Failed(error) => {
            let error = error.to_string();
            (
                "bad",
                "Prompt failed",
                Some(error.clone()),
                Some(format!("Prompt failed: {error}")),
            )
        }
        // `TurnState` is non-exhaustive; never render an unknown state as a
        // known one.
        _ => ("off", "Unrecognized turn state", None, None),
    };

    TurnPresentation {
        tone,
        label,
        detail,
        announcement,
    }
}

/// Where the turn stands, as the badge in the Timeline's header. The detail is
/// drawn separately above the box ([`state`]).
pub(crate) fn badge(presentation: &TurnPresentation) -> Element {
    let component_tone = badge_tone(presentation.tone);
    let tone = presentation.tone;
    let label = presentation.label;

    // An ended turn is a fact rather than a state to act on: a word, so a grey
    // capsule is not taken for a button.
    if component_tone.is_empty() {
        return rsx! {
            span { class: "turn-word {tone}", "data-slot": "turn-state", "{label}" }
        };
    }
    rsx! {
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

/// Whether the draft is a single `/word` the list has not been waved away from.
fn naming_a_command(draft: &str, dismissed: bool) -> bool {
    !dismissed
        && draft
            .strip_prefix('/')
            .is_some_and(|typed| !typed.contains(char::is_whitespace))
}

/// The attachment rules, in full, for the picker's tooltip.
fn attachment_rules(embedded: bool, image: bool) -> String {
    let mut rules =
        "Or drop files on the prompt. Up to 8 attachments, 5 MiB each and 6 MiB in total."
            .to_owned();
    if embedded {
        rules.push_str(" UTF-8 text files are embedded as text, PDFs and other files as binary contents. Text preview shows the first 2000 characters.");
    }
    if image {
        rules.push_str(" Images can also be pasted from the clipboard.");
    }
    rules
}

/// The commands whose names the draft begins, only while the draft is a single
/// `/word`. Owned, because the keydown handler outlives the render.
fn matching(commands: &[Command], draft: &str, dismissed: bool) -> Vec<Command> {
    if !naming_a_command(draft, dismissed) {
        return Vec::new();
    }
    let typed = draft[1..].to_lowercase();

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
        // An ended turn is a fact, not a grade: no colour role.
        _ => "",
    }
}

/// "1 file", "3 files".
fn files(count: usize) -> String {
    if count == 1 {
        "1 file".to_owned()
    } else {
        format!("{count} files")
    }
}

/// A byte count in megabytes, to one decimal: what the 6 MB budget is said in.
fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

/// The `stopReason` an agent gave, as it gave it. Shared with the Timeline so
/// both describe a stop the same way.
pub(crate) fn stopped(reason: &v1::StopReason) -> &'static str {
    match reason {
        v1::StopReason::EndTurn => "end_turn",
        v1::StopReason::MaxTokens => "max_tokens",
        v1::StopReason::MaxTurnRequests => "max_turn_requests",
        v1::StopReason::Refusal => "refusal",
        v1::StopReason::Cancelled => "cancelled",
        // `StopReason` is non-exhaustive; the frame is in the trace.
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
            dioxus_ssr::render(&audio_only).contains("The agent did not advertise image prompts.")
        );
        dom.runtime().handle_event("change", select(), picker);
        let mut mutations = Vec::new();
        for _ in 0..5 {
            dom.wait_for_work().await;
            mutations.extend(dom.render_immediate_to_vec().edits);
            if dioxus_ssr::render(&dom).contains("data-slot=\"attachment\"") {
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
        assert!(!dioxus_ssr::render(&dom).contains("data-slot=\"attachment\""));
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
            if dioxus_ssr::render(&dom).contains("data-slot=\"attachment\"") {
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
                SerializedFileData {
                    path: "/tmp/report #.pdf".into(),
                    size: 9,
                    last_modified: 0,
                    content_type: None,
                    contents: Some(b"%PDF-1.7\n".to_vec().into()),
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
            if dioxus_ssr::render(&dom).contains("PDF · original file embedded") {
                break;
            }
        }
        assert_eq!(
            dioxus_ssr::render(&dom)
                .matches("data-slot=\"attachment\"")
                .count(),
            5
        );
        let preview = dioxus_ssr::render(&dom);
        assert!(preview.contains("report #.pdf · application/pdf · 9 bytes"));
        assert!(
            !preview.contains("<iframe")
                && !preview.contains("<object")
                && !preview.contains("<embed")
        );
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
                .matches("data-slot=\"attachment\"")
                .count(),
            4,
            "non-file drop adds nothing"
        );
        reject.set(true);
        dom.runtime().handle_event("click", click(), send);
        dom.render_immediate(&mut NoOpMutations);
        assert!(sent.borrow().is_empty());
        assert!(
            dioxus_ssr::render(&dom).contains("data-slot=\"attachment\""),
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
            matches!(&sent.borrow()[0][..4],[v1::ContentBlock::Text(text),v1::ContentBlock::Image(image),v1::ContentBlock::Audio(second),v1::ContentBlock::Resource(resource)] if text.text=="Describe these" && image.data=="iVBORw0KGgo=" && image.mime_type=="image/png" && second.data=="UklGRgQAAABXQVZF" && second.mime_type=="audio/wav" && matches!(&resource.resource,v1::EmbeddedResourceResource::TextResourceContents(file) if file.text=="<script>\r\ntext" && file.uri=="file:///tmp/context%20%23.rs"))
        );
        assert!(
            matches!(&sent.borrow()[0][4..],[v1::ContentBlock::Resource(resource)] if matches!(&resource.resource,v1::EmbeddedResourceResource::BlobResourceContents(blob) if blob.blob=="JVBERi0xLjcK" && blob.mime_type.as_deref()==Some("application/pdf") && blob.uri=="file:///tmp/report%20%23.pdf"))
        );
        assert!(!dioxus_ssr::render(&dom).contains("data-slot=\"attachment\""));
    }

    #[test]
    fn a_resource_link_is_sent_to_an_agent_that_advertised_nothing() {
        use dioxus::core::NoOpMutations;
        use dioxus::html::{
            PlatformEventData, SerializedFormData, SerializedMouseData, SerializedToggleData,
        };
        use std::{any::Any, rc::Rc};

        #[derive(Clone, Props)]
        struct HostProps {
            sent: Rc<RefCell<Vec<Vec<v1::ContentBlock>>>>,
        }
        impl PartialEq for HostProps {
            fn eq(&self, other: &Self) -> bool {
                Rc::ptr_eq(&self.sent, &other.sent)
            }
        }
        fn host(props: HostProps) -> Element {
            rsx! {
                Composer {
                    turn: TurnState::Idle,
                    ready: true,
                    connected: true,
                    blocked: false,
                    problem: None,
                    commands: vec![],
                    mode: None,
                    on_prompt: move |content| {
                        props.sent.borrow_mut().push(content);
                        Ok(())
                    },
                    on_stop: |_| {},
                    on_set_mode: |_| {},
                }
            }
        }
        fn listeners(edits: &[Mutation], event: &str) -> Vec<dioxus::core::ElementId> {
            edits
                .iter()
                .filter_map(|edit| match edit {
                    Mutation::NewEventListener { name, id } if name == event => Some(*id),
                    _ => None,
                })
                .collect()
        }
        fn platform(data: impl Any) -> Event<dyn Any> {
            Event::new(
                Rc::new(PlatformEventData::new(Box::new(data))) as Rc<dyn Any>,
                true,
            )
        }

        set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let sent = Rc::new(RefCell::new(Vec::new()));
        let mut dom = VirtualDom::new_with_props(host, HostProps { sent: sent.clone() });
        let edits = dom.rebuild_to_vec().edits;
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains(r#"data-slot="resource-link""#) && !html.contains("Attach media"),
            "links need no advertisement; files do: {html}"
        );
        let send = listeners(&edits, "click")[0];

        let toggle = listeners(&edits, "toggle")[0];
        dom.runtime()
            .handle_event("toggle", platform(SerializedToggleData {}), toggle);
        let opened = dom.render_immediate_to_vec().edits;
        let uri = listeners(&opened, "input")[0];
        dom.runtime().handle_event(
            "input",
            platform(SerializedFormData::new(
                "file:///repo/docs/spec.md".to_owned(),
                Vec::new(),
            )),
            uri,
        );
        dom.render_immediate(&mut NoOpMutations);
        let add = *listeners(&opened, "click").last().expect("Add link");
        dom.runtime()
            .handle_event("click", platform(SerializedMouseData::default()), add);
        dom.render_immediate(&mut NoOpMutations);
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains("spec.md · link · file:///repo/docs/spec.md"),
            "the link is a row like a file: {html}"
        );

        dom.runtime()
            .handle_event("click", platform(SerializedMouseData::default()), send);
        dom.render_immediate(&mut NoOpMutations);
        let sent = sent.borrow();
        assert!(
            matches!(&sent[0][..], [v1::ContentBlock::ResourceLink(link)] if link.uri == "file:///repo/docs/spec.md" && link.name == "spec.md"),
            "{sent:?}"
        );
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
            // The footer is in flow inside the box, under the text.
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
                // The primary action is labelled with a word, not only a glyph.
                && ready.contains(">Send</button>"),
            "the Prompt and its Send Affordance retain durable accessible names: {ready}"
        );
        assert!(
            // No commands published: the hints promise neither / nor ↑↓.
            ready.contains(r#"placeholder="Send a prompt to the agent…""#)
                && !ready.contains("type / for commands")
                && ready.contains(r#"aria-describedby="prompt-shortcut""#)
                && ready.contains("↵ send · ⇧↵ newline<")
                && !ready.contains("↑↓ commands")
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
        // The badge (header) carries the word; the composer carries the detail.
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
                "Refused",
                // An ended turn is a word, not a capsule.
                r#"class="turn-word off""#,
            ),
            (
                TurnState::Ended(v1::StopReason::EndTurn),
                false,
                ">Done<",
                r#"class="turn-word off""#,
            ),
            (
                TurnState::Failed(CallError::Disconnected),
                false,
                "Prompt failed",
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
            // The detail is the composer's, never the badge's.
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
                && disconnected.contains("Start your agent"),
            "the disconnected state remains explicit: {disconnected}"
        );
        let no_session = shown(TurnState::Idle, false, true, false);
        assert!(
            stated(TurnState::Idle, false, true, false).contains("No session")
                && no_session.contains("Open a session to start prompting"),
            "the no-Session state remains distinct: {no_session}"
        );

        // A handshake that never reached a session says why.
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
        // Only the agent's own commands are offered.
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
        // Once there is a space the reader is typing arguments; the list closes.
        assert!(
            offering(published(), "/init ").is_empty(),
            "the list closes when the name is complete"
        );
        assert!(
            offering(published(), "/nope").is_empty(),
            "a name no agent published matches nothing"
        );
        assert!(naming_a_command("/", false) && naming_a_command("/co", false));
        assert!(!naming_a_command("/co x", false) && !naming_a_command("co", false));
        assert!(!naming_a_command("/co", true));
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
        // Nothing is offered until a command is being typed.
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
        assert!(failed.contains("Prompt failed:"), "{failed}");
    }
}
