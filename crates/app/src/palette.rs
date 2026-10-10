//! The command palette: every command the window has, by name. Each entry is a
//! control on screen or a menu item (ADR 0009).
//!
//! `showModal` owns the modality; the shortcut is a document listener. The
//! field is a combobox over a listbox: focus stays in the field and the
//! highlighted row is its `aria-activedescendant`.

use dioxus::prelude::*;

/// The dialog, by the name the scripts below reach it under.
const DIALOG: &str = "palette-dialog";

/// The list the field controls.
const LIST: &str = "palette-list";

/// The field inside it, by the selector the scripts focus.
const QUERY: &str = "[data-slot='palette-query']";

/// The window's keyboard shortcuts, as data: the menu bar binds them
/// (`shell.rs`) and the palette and rail print them, so they cannot disagree.
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
    pub const OPEN_TRACE: Chord = primary(Key::Letter('O'));
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

/// The shortcut script, installed once at the root. It toggles the palette.
///
/// ⌘K on macOS and Ctrl+K elsewhere: Ctrl+K on a Mac is a text-editing key.
/// Only this crate's `const`s are interpolated, never anything an agent said.
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

/// Opens it and focuses the field: `showModal` focuses the dialog itself, and
/// `autofocus` is honoured only on a dialog's first showing.
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

/// One thing the window can be asked to do, grouped by what it is about.
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

/// Whether a command's group, label or description contains what was typed
/// (case-insensitive substring, like the Console's filter).
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
    // Running a command closes the dialog and forgets what was typed.
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
            // On the dialog, so keys arrive wherever focus is inside it.
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
                    // Handled here: the field swallows Escape before the dialog.
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
                    // Not `search`, which brings its own clear button and Escape.
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

            // The click outside, which `showModal` does not give. Out of the Tab
            // order: Escape is the keyboard's way out.
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
