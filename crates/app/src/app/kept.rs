//! What the window keeps between launches: the recent-commands list and the
//! settings file, both read and written off the UI thread.

use acp_inspector_core::{AgentCommand, Appearance, Indentation, RecentCommands, Settings};
use dioxus::prelude::*;

use crate::toast::Announcer;
use crate::{appearance, indent};

/// The kept files, once read, and the preferences read out of them.
#[derive(Clone, Copy)]
pub(crate) struct Kept {
    pub recent: Signal<Option<RecentCommands>>,
    pub settings: Signal<Option<Settings>>,
    pub remembered: Signal<Vec<AgentCommand>>,
    pub appearance: Signal<Appearance>,
    pub indentation: Signal<Indentation>,
    pub on_remove_recent: EventHandler<AgentCommand>,
    pub on_forget_recent: EventHandler<AgentCommand>,
}

impl Kept {
    // Applied now and written off the UI thread; a failed write is not
    // reported, since the window is already in the chosen state.
    pub fn choose_appearance(mut self, chosen: Appearance) {
        appearance::paint(chosen);
        self.appearance.set(chosen);
        persist(self.settings.peek().clone(), move |stored| {
            stored.set_appearance(chosen)
        });
    }

    pub fn choose_indentation(mut self, chosen: Indentation) {
        self.indentation.set(chosen);
        persist(self.settings.peek().clone(), move |stored| {
            stored.set_indentation(chosen)
        });
    }
}

/// Reads both files and provides the window's preferences.
pub(crate) fn use_kept(announcer: Announcer) -> Kept {
    // Both files are read once, off the UI thread; until then the window shows
    // the defaults, as on a first run.
    let mut recent = use_signal(|| None::<RecentCommands>);
    let mut settings = use_signal(|| None::<Settings>);
    let mut remembered = use_signal(Vec::<AgentCommand>::new);
    let mut appearance = use_signal(Appearance::default);
    // Provided as context to every surface that draws payloads (`indent.rs`).
    let mut indentation = indent::provide(Indentation::default());
    use_future(move || async move {
        let Ok((stored, remembered_list)) =
            tokio::task::spawn_blocking(|| (Settings::load(), RecentCommands::load())).await
        else {
            return;
        };
        // An unreadable file was moved aside rather than overwritten; say where.
        if let Some(notice) = set_aside(remembered_list.backup(), stored.backup()) {
            announcer.notice(notice);
        }
        let chosen = stored.appearance();
        if chosen != Appearance::default() {
            appearance::paint(chosen);
        }
        appearance.set(chosen);
        indentation.set(stored.indentation());
        remembered.set(remembered_list.entries());
        settings.set(Some(stored));
        recent.set(Some(remembered_list));
    });

    // Forgetting an invocation, or only its kept values, off the UI thread.
    let edit_recent = use_callback(move |(entry, forget): (AgentCommand, bool)| {
        let Some(list) = recent.peek().clone() else {
            return;
        };
        spawn(async move {
            if let Ok((error, entries)) = tokio::task::spawn_blocking(move || {
                let done = if forget {
                    list.forget_values(&entry)
                } else {
                    list.remove(&entry)
                };
                (done.err(), list.entries())
            })
            .await
            {
                remembered.set(entries);
                if let Some(error) = error {
                    announcer.notice(format!("Couldn't update the recent commands file: {error}"));
                }
            }
        });
    });

    Kept {
        recent,
        settings,
        remembered,
        appearance,
        indentation,
        on_remove_recent: EventHandler::new(move |entry: AgentCommand| {
            edit_recent.call((entry, false))
        }),
        on_forget_recent: EventHandler::new(move |entry: AgentCommand| {
            edit_recent.call((entry, true))
        }),
    }
}

/// The notice for state files moved aside at startup, or `None` when both were
/// read.
fn set_aside(
    recent: Option<&std::path::Path>,
    settings: Option<&std::path::Path>,
) -> Option<String> {
    let said: Vec<String> = [("recent commands", recent), ("settings", settings)]
        .into_iter()
        .filter_map(|(what, moved)| {
            moved.map(|moved| {
                format!(
                    "Your {what} file couldn't be read and was saved as {}.",
                    moved.display()
                )
            })
        })
        .collect();
    (!said.is_empty()).then(|| said.join(" "))
}

/// Writes a preference off the UI thread. A failed write is not reported: the
/// window is already in the chosen state.
fn persist(
    settings: Option<Settings>,
    write: impl FnOnce(&Settings) -> std::io::Result<()> + Send + 'static,
) {
    if let Some(settings) = settings {
        spawn(async move {
            let _ = tokio::task::spawn_blocking(move || write(&settings)).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_state_file_set_aside_at_startup_is_said_with_where_it_went() {
        use std::path::Path;
        assert_eq!(set_aside(None, None), None);
        assert_eq!(
            set_aside(Some(Path::new("/state/recent.json.bak")), None).as_deref(),
            Some(
                "Your recent commands file couldn't be read and was saved as /state/recent.json.bak."
            )
        );
        let both = set_aside(
            Some(Path::new("/state/recent.json.bak")),
            Some(Path::new("/state/settings.json.bak")),
        )
        .unwrap();
        assert!(
            both.contains("recent commands") && both.contains("settings.json.bak"),
            "{both}"
        );
    }
}
