//! Session settings as the window drives them: the in-flight counts, the
//! composer's mode cycle, and the two setters.

use acp_inspector_core::{Inspector, SessionSettings, v1};
use dioxus::prelude::*;

use crate::{composer, session_settings};

/// Setter calls still awaiting answers. Presentation only: the stores say what
/// the agent answered.
#[derive(Clone, Default)]
pub(crate) struct SessionSettingsWork {
    pub generation: u64,
    pub modes: usize,
    pub option: Option<v1::SessionConfigId>,
    options: usize,
}

impl SessionSettingsWork {
    pub fn reset(&mut self) {
        self.generation += 1;
        self.modes = 0;
        self.option = None;
        self.options = 0;
    }

    fn begin_mode(&mut self) -> u64 {
        self.modes += 1;
        self.generation
    }

    fn finish_mode(&mut self, generation: u64) {
        if self.generation == generation {
            self.modes = self.modes.saturating_sub(1);
        }
    }

    fn begin_option(&mut self, option: v1::SessionConfigId) -> u64 {
        self.option = Some(option);
        self.options += 1;
        self.generation
    }

    fn finish_option(&mut self, generation: u64) {
        if self.generation != generation {
            return;
        }
        self.options = self.options.saturating_sub(1);
        if self.options == 0 {
            self.option = None;
        }
    }
}

/// The composer's mode control, read from the Session Settings store: the mode
/// the agent last stated, and the next mode it published (§7.6).
pub(crate) fn cycle(settings: &SessionSettings, connected: bool) -> Option<composer::Cycle> {
    let modes = settings.modes.as_ref()?;
    let offered = &modes.available_modes;
    let at = offered
        .iter()
        .position(|mode| mode.id == modes.current_mode_id);
    let next = at
        .and_then(|at| offered.get((at + 1) % offered.len().max(1)))
        .or_else(|| offered.first())
        .map(|mode| mode.id.clone())
        // A single published mode has nowhere to cycle to.
        .filter(|_| offered.len() > 1);

    Some(composer::Cycle {
        id: modes.current_mode_id.to_string(),
        available: connected && next.is_some(),
        next,
    })
}

/// Sends `session/set_mode`. Not waited for: the answer lands in the Session
/// Settings store.
pub(crate) fn set_mode(
    inspector: Inspector,
    mut settings_work: Signal<SessionSettingsWork>,
) -> EventHandler<v1::SessionModeId> {
    EventHandler::new(move |mode: v1::SessionModeId| {
        let inspector = inspector.clone();
        let generation = settings_work.with_mut(SessionSettingsWork::begin_mode);
        spawn(async move {
            let _ = inspector.set_mode(&mode).await;
            settings_work.with_mut(|work| work.finish_mode(generation));
        });
    })
}

/// Sends `session/set_config_option`. The reader stores the replacement option
/// set before the call returns.
pub(crate) fn set_config_option(
    inspector: Inspector,
    mut settings_work: Signal<SessionSettingsWork>,
) -> EventHandler<session_settings::Choice> {
    EventHandler::new(move |choice: session_settings::Choice| {
        let inspector = inspector.clone();
        let generation = settings_work.with_mut(|work| work.begin_option(choice.option.clone()));
        spawn(async move {
            let _ = inspector
                .set_config_option(&choice.option, &choice.value)
                .await;
            settings_work.with_mut(|work| work.finish_option(generation));
        });
    })
}
