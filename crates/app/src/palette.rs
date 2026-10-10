//! Every command this window has, by name (ADR 0009).
//!
//! **It is the menu bar's list, reached the other way.** The strip at the top of
//! the window is how a desktop application is read when you are looking for what
//! it can do; this is how one is driven when you already know. Every entry here
//! is a control that exists on screen or a menu item that exists in the strip —
//! the same rule `shell.rs` states, for the same reason.
//!
//! **The platform owns the modality**, as it does for the other two dialogs
//! (`connect.rs`): `showModal` traps focus, dims the window behind it, closes on
//! Escape and gives focus back to whatever opened it. What is not the platform's
//! is the shortcut, because there is no such thing as a native one here — a
//! keystroke that works wherever the reader is has to be a listener on the
//! document, and this module's one script is it.
//!
//! **A combobox over a listbox** (A2): focus stays in the field, the highlighted
//! row is the field's `aria-activedescendant`, and the row is scrolled into view
//! as the arrow keys move it.
//!
//! **Nothing here decides anything.** A command is a label, a sentence and a
//! handler the window already had; this file filters a list and calls one.

use dioxus::prelude::*;

/// The dialog, by the name the scripts below reach it under.
const DIALOG: &str = "palette-dialog";

/// The list the field controls.
const LIST: &str = "palette-list";

/// The field inside it, by the name those scripts find it under. A slot rather
/// than a class, because what this names is *the thing the keyboard belongs in*
/// rather than how it is painted.
const QUERY: &str = "[data-slot='palette-query']";

/// The window's keyboard shortcuts, as data: the menu bar binds them
/// (`shell.rs`) and the palette and rail print them, so the three cannot
/// disagree about which key does what.
pub mod shortcut {
    /// A key a shortcut is made of.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Key {
        Letter(char),
        Digit(u8),
        Period,
        Backspace,
        Backslash,
    }

    /// The platform's primary modifier (⌘ on macOS, Ctrl elsewhere), maybe
    /// Shift, and a key.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Chord {
        pub shift: bool,
        pub key: Key,
    }

    const fn primary(key: Key) -> Chord {
        Chord { shift: false, key }
    }

    pub const PALETTE: Chord = primary(Key::Letter('K'));
    pub const EXPORT: Chord = primary(Key::Letter('E'));
    pub const CLEAR: Chord = Chord {
        shift: true,
        key: Key::Backspace,
    };
    pub const NEW_SESSION: Chord = primary(Key::Letter('N'));
    pub const RESTART: Chord = primary(Key::Letter('R'));
    pub const CANCEL_TURN: Chord = primary(Key::Period);
    pub const SHOW_TRACE: Chord = primary(Key::Digit(1));
    pub const SHOW_DIAGNOSTICS: Chord = primary(Key::Digit(2));
    pub const TRACE_ONLY: Chord = primary(Key::Backslash);
    #[cfg_attr(not(feature = "desktop"), allow(dead_code))]
    pub const QUIT: Chord = primary(Key::Letter('Q'));

    /// Whether this build runs where ⌘ is the primary modifier.
    pub const MAC: bool = cfg!(target_os = "macos");

    impl Chord {
        /// The chord as the platform writes it: `⌘⇧⌫` on macOS,
        /// `Ctrl+Shift+Backspace` elsewhere.
        pub fn label(self, mac: bool) -> String {
            let key = match self.key {
                Key::Letter(letter) => letter.to_string(),
                Key::Digit(digit) => digit.to_string(),
                Key::Period => ".".to_owned(),
                Key::Backslash => "\\".to_owned(),
                Key::Backspace if mac => "\u{232b}".to_owned(),
                Key::Backspace => "Backspace".to_owned(),
            };
            match (mac, self.shift) {
                (true, false) => format!("\u{2318}{key}"),
                (true, true) => format!("\u{2318}\u{21e7}{key}"),
                (false, false) => format!("Ctrl+{key}"),
                (false, true) => format!("Ctrl+Shift+{key}"),
            }
        }

        /// The chord on the platform this build runs on.
        pub fn here(self) -> String {
            self.label(MAC)
        }
    }
}

/// The shortcut, installed once at the root.
///
/// **A toggle rather than an open**, because that is what every application
/// with this control does: the keystroke that summoned it puts it away. ⌘K on
/// macOS and Ctrl+K elsewhere — Control on a Mac is text editing (Ctrl+K kills
/// to the end of the line), and taking it would break that in every field.
/// Nothing an agent said reaches this string — the only values interpolated
/// are `const`s this crate wrote.
pub fn watch() -> String {
    format!(
        r#"
const mac = /Mac|iPhone|iPad/.test(navigator.platform);
document.addEventListener("keydown", (event) => {{
  const primary = mac ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
  if (!primary || event.key.toLowerCase() !== "k") {{
    return;
  }}
  const palette = document.getElementById({DIALOG:?});
  if (!palette) {{
    return;
  }}
  event.preventDefault();
  if (palette.open) {{
    palette.close();
  }} else {{
    palette.showModal();
    palette.querySelector({QUERY:?})?.focus();
  }}
}});
"#
    )
}

/// Open it, for the control at the foot of the rail.
///
/// **And put the keyboard in the field**, which is the whole of what this
/// surface is: `showModal` gives focus to the dialog itself, and the platform's
/// own `autofocus` is honoured for the *first* showing of a dialog only.
pub fn open() {
    document::eval(&opening());
}

/// The script [`open`] evaluates, so the test below reads the one that runs.
fn opening() -> String {
    format!(
        "const palette = document.getElementById({DIALOG:?});
palette?.showModal();
palette?.querySelector({QUERY:?})?.focus();"
    )
}

/// Shut it, which is what running a command does.
pub fn close() {
    document::eval(&format!("document.getElementById({DIALOG:?})?.close()"));
}

/// The DOM id of the `position`th row.
fn option_id(position: usize) -> String {
    format!("palette-option-{position}")
}

/// Bring a row into view inside the list, without moving focus from the field.
fn reveal(position: usize) {
    document::eval(&format!(
        "requestAnimationFrame(() => document.getElementById({:?})?.scrollIntoView({{ block: 'nearest' }}));",
        option_id(position)
    ));
}

/// One thing the window can be asked to do.
///
/// The group is what it is *about* — the connection, the session, the wire, the
/// window itself — because a list of twenty verbs read in one run is a list
/// nobody reads.
#[derive(Clone, PartialEq)]
pub struct Command {
    pub group: &'static str,
    pub label: String,
    /// What it does, in the words the control that does it uses.
    pub says: String,
    /// The keyboard shortcut that does the same thing, as the platform writes it.
    pub shortcut: Option<String>,
    /// For a switch: whether it is on now.
    pub state: Option<bool>,
    pub run: EventHandler<()>,
}

impl Command {
    pub fn new(group: &'static str, label: &str, says: &str, run: EventHandler<()>) -> Self {
        Self {
            group,
            label: label.to_owned(),
            says: says.to_owned(),
            shortcut: None,
            state: None,
            run,
        }
    }

    /// The same command, with the shortcut that also does it.
    #[must_use]
    pub fn shortcut(mut self, chord: shortcut::Chord) -> Self {
        self.shortcut = Some(chord.here());
        self
    }

    /// The same command, as a switch that is `on` or off.
    #[must_use]
    pub fn state(mut self, on: bool) -> Self {
        self.state = Some(on);
        self
    }

    fn found_by(&self, typed: &str) -> bool {
        found(self.group, &self.label, &self.says, typed)
    }
}

/// Whether a command answers to what has been typed.
///
/// **What it is about, what it is called, and what it does** — all three,
/// because a reader looking for a verb has the sentence and one looking for a
/// surface has the group. Case-insensitive substring and deliberately nothing
/// cleverer, which is the rule the Console's own filter already states.
fn found(group: &str, label: &str, says: &str, typed: &str) -> bool {
    if typed.is_empty() {
        return true;
    }

    format!("{group} {label} {says}")
        .to_lowercase()
        .contains(typed)
}

/// The palette.
#[component]
pub fn Palette(commands: Vec<Command>) -> Element {
    let mut typed = use_signal(String::new);
    let mut highlighted = use_signal(|| 0usize);
    let query = typed().trim().to_lowercase();
    let found: Vec<Command> = commands
        .iter()
        .filter(|command| command.found_by(&query))
        .cloned()
        .collect();
    let highlight = highlighted().min(found.len().saturating_sub(1));
    // What a command does when it is chosen: the thing, and then the way out.
    // The dialog is what the reader opened to *do* something, so it closes
    // itself once they have — and it forgets what was typed, because the next
    // question is a different one.
    let mut reset = move || {
        typed.set(String::new());
        highlighted.set(0);
    };
    // The rows in runs of one group, each run a group of the listbox.
    let mut groups: Vec<(&'static str, Vec<(usize, Command)>)> = Vec::new();
    for (position, command) in found.iter().cloned().enumerate() {
        match groups.last_mut() {
            Some((group, rows)) if *group == command.group => rows.push((position, command)),
            _ => groups.push((command.group, vec![(position, command)])),
        }
    }
    let count = found.len();

    rsx! {
        dialog {
            id: DIALOG,
            class: "modal palette",
            "data-slot": "palette",
            aria_labelledby: "palette-title",
            // **On the dialog rather than on the box inside it**, because a
            // keystroke is only this surface's if it arrives wherever the
            // reader's focus is.
            onkeydown: move |event| {
                let moved = match event.key() {
                    Key::ArrowDown if count > 0 => Some((highlight + 1) % count),
                    Key::ArrowUp if count > 0 => Some((highlight + count - 1) % count),
                    Key::Home if count > 0 && event.modifiers().ctrl() => Some(0),
                    Key::End if count > 0 && event.modifiers().ctrl() => Some(count - 1),
                    _ => None,
                };
                if let Some(next) = moved {
                    event.prevent_default();
                    highlighted.set(next);
                    reveal(next);
                    return;
                }
                match event.key() {
                    Key::Enter if count > 0 => {
                        event.prevent_default();
                        let command = found[highlight].clone();
                        reset();
                        close();
                        command.run.call(());
                    }
                    // **Said here rather than left to the platform**: the field
                    // took the key first and swallowed it, which is what a search
                    // input does with Escape.
                    Key::Escape => {
                        reset();
                        close();
                    }
                    _ => {}
                }
            },
            div {
                class: "modal-box dialog-box palette-box",
                h2 { id: "palette-title", class: "sr-only", "Command palette" }
                input {
                    class: "palette-query",
                    "data-slot": "palette-query",
                    // Not `search`: the type comes with a clear affordance this
                    // window does not draw and with Escape already spoken for.
                    r#type: "text",
                    role: "combobox",
                    aria_expanded: "true",
                    aria_autocomplete: "list",
                    aria_controls: LIST,
                    aria_activedescendant: (count > 0).then(|| option_id(highlight)),
                    value: "{typed}",
                    spellcheck: false,
                    autofocus: true,
                    aria_label: "Run a command",
                    placeholder: "Run a command…",
                    oninput: move |event| {
                        typed.set(event.value());
                        highlighted.set(0);
                    },
                }
                div { id: LIST, class: "palette-list", role: "listbox", aria_label: "Commands",
                    if found.is_empty() {
                        p { class: "empty", role: "presentation", "No matching commands" }
                    }
                    for (group, rows) in groups {
                        div { key: "{group}-{rows[0].0}", role: "group", aria_label: "{group}",
                            // The group's name, over the first command in it.
                            div { class: "palette-group heading", aria_hidden: "true", "{group}" }
                            for (position, command) in rows {
                                {
                                    let chosen = command.clone();
                                    rsx! {
                                        div {
                                            key: "{command.group} {command.label}",
                                            id: option_id(position),
                                            class: "palette-row",
                                            role: "option",
                                            tabindex: "-1",
                                            aria_selected: position == highlight,
                                            aria_checked: command.state.map(|on| if on { "true" } else { "false" }),
                                            onmousemove: move |_| {
                                                if *highlighted.peek() != position {
                                                    highlighted.set(position);
                                                }
                                            },
                                            onclick: move |_| {
                                                let command = chosen.clone();
                                                reset();
                                                close();
                                                command.run.call(());
                                            },
                                            span { class: "palette-label", "{command.label}" }
                                            span { class: "palette-desc", "{command.says}" }
                                            if let Some(on) = command.state {
                                                span { class: "palette-state", "data-on": on, if on { "On" } else { "Off" } }
                                            }
                                            if let Some(shortcut) = &command.shortcut {
                                                kbd { class: "palette-key", "{shortcut}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "palette-foot", aria_hidden: "true",
                    span { "↑↓ navigate" }
                    span { "↵ run" }
                    span { "esc close" }
                    span { class: "palette-count", "{count} of {commands.len()}" }
                }
            }

            // The click outside, which `showModal` does not give (`connect.rs`).
            // Out of the Tab order: Escape is the keyboard's way out.
            form { method: "dialog", class: "modal-backdrop",
                button { "data-slot": "dismiss", tabindex: "-1", onclick: move |_| reset(), "close" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commands() -> Vec<Command> {
        vec![
            Command::new(
                "Connection",
                "Connect…",
                "Start an agent, or run one you have run before",
                EventHandler::new(move |()| {}),
            ),
            Command::new(
                "Connection",
                "Restart",
                "Run the last command again",
                EventHandler::new(move |()| {}),
            )
            .shortcut(shortcut::RESTART),
            Command::new(
                "Session",
                "New session",
                "Opens and switches to a new session",
                EventHandler::new(move |()| {}),
            ),
            Command::new(
                "Window",
                "Pretty JSON",
                "Draw payloads indented",
                EventHandler::new(move |()| {}),
            )
            .state(true),
        ]
    }

    fn shown() -> String {
        #[component]
        fn Host() -> Element {
            rsx! {
                Palette { commands: commands() }
            }
        }

        let mut dom = VirtualDom::new(Host);
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    #[test]
    fn the_platform_owns_the_modality_here_too() {
        let html = shown();
        assert!(
            html.contains(r#"id="palette-dialog""#) && html.contains(r#"class="modal palette""#),
            "{html}"
        );
        assert!(
            html.contains(r#"<form method="dialog" class="modal-backdrop">"#),
            "the click outside, which `showModal` does not give: {html}"
        );
        assert!(
            !html.contains("open=true") && !html.contains(r#"open="#),
            "an attribute would draw the box in the page with no modality at all: {html}"
        );
        assert!(
            html.contains(r#"aria-labelledby="palette-title""#)
                && html.contains(r#"id="palette-title""#),
            "the dialog is named: {html}"
        );
        assert!(
            html.contains(r#"data-slot="dismiss" tabindex="-1""#),
            "the backdrop is not a Tab stop: {html}"
        );
    }

    #[test]
    fn opening_it_puts_the_keyboard_in_the_question() {
        let html = shown();
        assert!(
            html.contains(r#"data-slot="palette-query""#) && html.contains("autofocus"),
            "{html}"
        );
        assert!(!html.contains(r#"type="search""#), "{html}");

        for script in [watch(), opening()] {
            assert!(
                script.contains("showModal()")
                    && script.contains("[data-slot='palette-query']")
                    && script.contains(".focus()"),
                "the way in shows it and then puts the keyboard in it: {script}"
            );
        }
    }

    #[test]
    fn the_shortcut_is_the_platforms_primary_modifier_and_carries_nothing_an_agent_said() {
        let watch = watch();
        assert!(
            watch.contains("metaKey") && watch.contains("ctrlKey") && watch.contains(r#""k""#),
            "{watch}"
        );
        assert!(
            watch.contains("const mac") && watch.contains("mac ? event.metaKey"),
            "⌘ on macOS, Ctrl elsewhere — never Ctrl on a Mac: {watch}"
        );
        assert!(
            watch.contains("palette.close()") && watch.contains("palette.showModal()"),
            "{watch}"
        );
        assert!(watch.contains(r#""palette-dialog""#), "{watch}");
    }

    #[test]
    fn shortcuts_are_written_the_way_each_platform_writes_them() {
        use shortcut::*;
        assert_eq!(RESTART.label(true), "⌘R");
        assert_eq!(RESTART.label(false), "Ctrl+R");
        assert_eq!(CANCEL_TURN.label(true), "⌘.");
        assert_eq!(CANCEL_TURN.label(false), "Ctrl+.");
        assert_eq!(CLEAR.label(true), "⌘⇧⌫");
        assert_eq!(CLEAR.label(false), "Ctrl+Shift+Backspace");
        assert_eq!(PALETTE.label(true), "⌘K");
    }

    #[test]
    fn the_field_is_a_combobox_over_a_listbox_of_options() {
        let html = shown();
        assert!(
            html.contains(r#"role="combobox""#)
                && html.contains(r#"aria-controls="palette-list""#)
                && html.contains(r#"aria-activedescendant="palette-option-0""#),
            "{html}"
        );
        assert!(
            html.contains(r#"id="palette-list""#) && html.contains(r#"role="listbox""#),
            "{html}"
        );
        assert!(
            html.contains(r#"id="palette-option-0" class="palette-row" role="option" tabindex="-1" aria-selected=true"#),
            "options are ids the field can point at, out of the Tab order: {html}"
        );
        assert_eq!(
            html.matches(r#"role="group""#).count(),
            3,
            "each run of one group is a group of the list: {html}"
        );
    }

    #[test]
    fn rows_show_their_shortcut_and_switches_say_whether_they_are_on() {
        let html = shown();
        assert!(
            html.contains(&format!(
                r#"<kbd class="palette-key">{}</kbd>"#,
                shortcut::RESTART.here()
            )),
            "{html}"
        );
        assert!(
            html.contains(r#"aria-checked="true""#) && html.contains(">On</span>"),
            "{html}"
        );
    }

    #[test]
    fn every_command_is_named_grouped_and_says_what_it_does() {
        let html = shown();
        for (group, label, says) in [
            ("Connection", "Connect…", "Start an agent"),
            ("Session", "New session", "Opens and switches"),
        ] {
            assert!(
                html.contains(group) && html.contains(label) && html.contains(says),
                "{label} is on the list with what it is about and what it does: {html}"
            );
        }
        assert_eq!(html.matches(r#"class="palette-group heading""#).count(), 3);
        assert!(html.contains("4 of 4"), "{html}");
    }

    #[test]
    fn a_command_answers_to_what_it_is_about_what_it_is_called_and_what_it_does() {
        assert!(found("Connection", "Connect…", "Start an agent", ""));
        assert!(found("Connection", "Connect…", "Start an agent", "connect"));
        assert!(found(
            "Session",
            "New session",
            "Opens and switches",
            "switches"
        ));
        assert!(found("Trace", "Export…", "Write every frame", "trace"));
        assert!(!found(
            "Trace",
            "Export…",
            "Write every frame",
            "elicitation"
        ));
    }
}
