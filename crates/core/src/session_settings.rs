//! What the live session is configured as: its modes and its config options
//! (`CONTEXT.md`, *Session Settings*). See `docs/architecture.md` §7.6.
//!
//! Advertisement here is presence of data, not a capability: modes and config
//! options are fields on the live session's setup response, so `None` means the
//! agent published none, never "not yet known".
//!
//! This is a decoded view. The schema skips invalid entries when it
//! deserializes an option list, so a malformed option can be missing here; the
//! trace still holds the complete frame.

use agent_client_protocol_schema::v1;

use crate::call::CallError;

/// The modes and the config options of the live session, as one read of
/// [`Inspector::session_settings`](crate::Inspector::session_settings).
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct SessionSettings {
    /// The modes the agent published for this session, with the current one
    /// named among them, or `None` where it published none. Changed with
    /// `session/set_mode`.
    pub modes: Option<v1::SessionModeState>,
    /// The config options the agent published for this session, each with its
    /// current value, or `None` where it published none. Changed with
    /// `session/set_config_option`. An empty list is not the same as `None`.
    pub config_options: Option<Vec<v1::SessionConfigOption>>,
    /// What became of the last `session/set_mode` sent in this session, or
    /// `None` where none has been sent or a later mode announcement settled an
    /// acknowledgement. Never what [`modes`](Self::modes) says the session is in.
    pub mode_change: Option<ModeChange>,
    /// Why the last `session/set_config_option` sent in this session did not
    /// happen, or `None` where none has been sent or the last one was answered.
    /// A success needs no record: its answer is the new
    /// [`config_options`](Self::config_options).
    pub config_refusal: Option<ConfigRefusal>,
    /// A mode the agent stated while this session was opening, where the answer
    /// that opened it then published no modes; `None` otherwise.
    ///
    /// A `session/load` replays ahead of its answer, so a replayed
    /// `current_mode_update` can cross before the response that carries the
    /// modes. The response wins; this records that the agent stated a mode it
    /// did not offer (§7.6). Held for `session/resume` too. It gates nothing.
    pub unoffered_mode: Option<v1::SessionModeId>,
}

/// A `session/set_config_option` that did not take effect, and why (§7.6).
///
/// The error is the agent's refusal, a missing connection, or an answer that
/// could not be read as v1 ([`CallError::Undecodable`]): a list nobody could
/// read is not one the store is rebuilt from. Nothing is rolled back, because
/// nothing was applied.
#[derive(Clone, Debug, PartialEq)]
pub struct ConfigRefusal {
    /// The option that was asked about; not necessarily one the agent still
    /// publishes.
    pub option: v1::SessionConfigId,
    /// The value asked for, in the shape the request carried it.
    pub value: v1::SessionConfigOptionValue,
    pub error: CallError,
}

/// What became of a `session/set_mode` (§7.6).
///
/// Neither arm moves the mode: the surface shows what the agent last stated,
/// and this reports the ask beside it.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ModeChange {
    /// The agent answered the set and has said nothing about its mode since.
    ///
    /// `session/set_mode` answers `{}` and nothing obliges a follow-up, so this
    /// is a permitted silence, not a fault. A later `current_mode_update`
    /// settles it.
    Acknowledged(v1::SessionModeId),
    /// The set did not happen: the agent refused it, or there was nobody to ask.
    Refused {
        /// The mode that was asked for.
        mode: v1::SessionModeId,
        error: CallError,
    },
}

impl SessionSettings {
    /// What a session setup response published. Replaces rather than merges.
    ///
    /// `stated` is a mode the agent named while the session was opening (a
    /// replay); it is kept as [`unoffered_mode`](Self::unoffered_mode) only
    /// where the answer published no modes.
    pub(crate) fn published(
        modes: Option<v1::SessionModeState>,
        config_options: Option<Vec<v1::SessionConfigOption>>,
        stated: Option<v1::SessionModeId>,
    ) -> Self {
        Self {
            unoffered_mode: stated.filter(|_| modes.is_none()),
            modes,
            config_options,
            // A session that has just opened has been asked for nothing.
            mode_change: None,
            config_refusal: None,
        }
    }

    /// Whether the agent published no settings at all for this session.
    /// Sets this client sent do not count.
    pub fn is_empty(&self) -> bool {
        self.modes.is_none() && self.config_options.is_none()
    }

    /// The agent says the session is in a mode now (`current_mode_update`).
    ///
    /// Leaves the published set alone, and changes nothing where the agent
    /// published no modes: the advertisement is the setup response's (§7.6).
    /// Any announcement settles an acknowledgement; a refusal survives it,
    /// because the agent still said no to that ask.
    ///
    /// Answers with whether anything changed.
    pub(crate) fn now_in(&mut self, mode: &v1::SessionModeId) -> bool {
        let settled = matches!(self.mode_change, Some(ModeChange::Acknowledged(_)));
        if settled {
            self.mode_change = None;
        }

        let Some(modes) = self.modes.as_mut() else {
            return settled;
        };
        if modes.current_mode_id == *mode {
            return settled;
        }
        modes.current_mode_id = mode.clone();
        true
    }

    /// The agent answered a `session/set_mode` and said nothing else. The mode
    /// stays where the agent's last statement put it.
    ///
    /// Answers with whether anything changed.
    pub(crate) fn acknowledged(&mut self, mode: v1::SessionModeId) -> bool {
        self.record(ModeChange::Acknowledged(mode))
    }

    /// The mode was not set. Nothing else moves.
    ///
    /// Answers with whether anything changed.
    pub(crate) fn mode_refused(&mut self, mode: v1::SessionModeId, error: CallError) -> bool {
        self.record(ModeChange::Refused { mode, error })
    }

    fn record(&mut self, change: ModeChange) -> bool {
        if self.mode_change.as_ref() == Some(&change) {
            return false;
        }
        self.mode_change = Some(change);
        true
    }

    /// The agent restates its config options (`config_option_update`).
    ///
    /// The list is complete, so it replaces the held one. Unlike a mode
    /// announcement it is taken even where the setup response published none:
    /// it carries the whole list, so it is the agent's own advertisement.
    ///
    /// Answers with whether anything changed.
    pub(crate) fn offers(&mut self, options: Vec<v1::SessionConfigOption>) -> bool {
        if self.config_options.as_deref() == Some(options.as_slice()) {
            return false;
        }
        self.config_options = Some(options);
        true
    }

    /// The agent answered a `session/set_config_option` with an option set.
    ///
    /// The store is rebuilt from the returned list whole, with no merging, and
    /// any held refusal is settled because this answers the last set.
    ///
    /// Answers with whether anything changed.
    pub(crate) fn answered(&mut self, options: Vec<v1::SessionConfigOption>) -> bool {
        let settled = self.config_refusal.take().is_some();
        self.offers(options) || settled
    }

    /// The config option was not set. Nothing else moves, and a later
    /// `config_option_update` does not clear the refusal.
    ///
    /// Answers with whether anything changed.
    pub(crate) fn option_refused(
        &mut self,
        option: v1::SessionConfigId,
        value: v1::SessionConfigOptionValue,
        error: CallError,
    ) -> bool {
        let refusal = ConfigRefusal {
            option,
            value,
            error,
        };
        if self.config_refusal.as_ref() == Some(&refusal) {
            return false;
        }
        self.config_refusal = Some(refusal);
        true
    }
}
