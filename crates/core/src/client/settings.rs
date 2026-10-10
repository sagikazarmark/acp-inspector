//! Session settings: the two setters, and reading what a setup response, a
//! setter's answer or an announcement says the session is configured as. See
//! `docs/architecture.md` §7.6.
//!
//! The settings are written by the reader where each frame crosses, so an
//! announcement and an answer are applied in wire order.

use agent_client_protocol_schema::v1;
use serde_json::Value;

use crate::call::CallError;
use crate::conformance::{Annotation, OptionSet};
use crate::frame::Frame;
use crate::restore::Restore;
use crate::session_settings::SessionSettings;
use crate::store::locked;

use super::{Client, decode, encode};

/// A `session/set_mode` in flight.
pub(super) struct ChangingMode {
    /// The id its answer will arrive under.
    pub(super) call: i64,
    /// The mode asked for; what a surface reports is the ask.
    pub(super) mode: v1::SessionModeId,
    /// Whether the agent has announced any mode since this went out. If it
    /// has, the change is confirmed and the answer adds nothing.
    pub(super) restated: bool,
}

/// A `session/set_config_option` in flight.
///
/// Nothing is watched while it is out: the answer carries the complete option
/// set. The ask is kept for a refusal to name and for the option-set rule
/// (§15 q7).
pub(super) struct SettingOption {
    /// The id its answer will arrive under.
    pub(super) call: i64,
    /// The option asked about.
    pub(super) option: v1::SessionConfigId,
    /// The value asked for, in the shape the request carried it.
    pub(super) value: v1::SessionConfigOptionValue,
    /// The request frame, which the annotation is read beside.
    pub(super) request: Frame,
}

/// A call that opens a session, in flight.
pub(super) struct Opening {
    /// The id its answer will arrive under.
    pub(super) call: i64,
    pub(super) how: Setup,
    /// The session being opened, when known before the answer (every call but
    /// `session/new`). Tells that session's replay from the session being
    /// left, which may still be talking.
    pub(super) session: Option<v1::SessionId>,
    /// The last mode the agent stated for that session while the call was out.
    pub(super) stated: Option<v1::SessionModeId>,
}

/// Which call is opening the session, and so which response type its answer
/// is (`CONTEXT.md`, *Session setup response*).
#[derive(Clone, Copy)]
pub(super) enum Setup {
    New,
    Restore(Restore),
}

impl Client {
    /// `session/set_mode` for the live session (§7.6).
    ///
    /// Nothing is applied optimistically: the surface keeps showing the
    /// agent's last stated mode, and the reader records a
    /// [`ModeChange`](crate::ModeChange) from the answer (`changed_mode`). Sent
    /// even while a turn is running.
    pub(crate) async fn set_mode(&self, mode: &v1::SessionModeId) -> Result<(), CallError> {
        let session = self
            .session()
            .inspect_err(|error| self.mode_change_failed(None, mode, error))?;

        let call = self
            .rpc
            .prepare(
                v1::AGENT_METHOD_NAMES.session_set_mode,
                encode(&v1::SetSessionModeRequest::new(session, mode.clone())),
            )
            .inspect_err(|error| self.mode_change_failed(None, mode, error))?;
        let id = call.id();
        *locked(&self.changing_mode) = Some(ChangingMode {
            call: id,
            mode: mode.clone(),
            restated: false,
        });

        let outcome = self.rpc.send(call).await;
        if let Err(error) = &outcome {
            self.mode_change_failed(Some(id), mode, error);
        }
        // The answer carries only `_meta`; the frame is in the trace.
        outcome.map(|_| ())
    }

    /// `session/set_config_option` for the live session (§7.6).
    ///
    /// The answer is the complete replacement option set, and the reader
    /// rebuilds the store from it (`set_answered`). The value's type decides its
    /// wire shape: a select goes out as a bare value id, a boolean with
    /// `"type":"boolean"`. Sent even while a turn is running.
    pub(crate) async fn set_config_option(
        &self,
        option: &v1::SessionConfigId,
        value: &v1::SessionConfigOptionValue,
    ) -> Result<(), CallError> {
        let session = self
            .session()
            .inspect_err(|error| self.option_set_failed(None, option, value, error))?;

        let call = self
            .rpc
            .prepare(
                v1::AGENT_METHOD_NAMES.session_set_config_option,
                encode(&v1::SetSessionConfigOptionRequest::new(
                    session,
                    option.clone(),
                    value.clone(),
                )),
            )
            .inspect_err(|error| self.option_set_failed(None, option, value, error))?;
        let id = call.id();
        *locked(&self.setting_option) = Some(SettingOption {
            call: id,
            option: option.clone(),
            value: value.clone(),
            request: call.frame().clone(),
        });

        let outcome = self
            .rpc
            .send(call)
            .await
            // Decoded again here so the caller's answer agrees with the store:
            // an answer the reader could not take is a set that did not happen.
            .and_then(decode::<v1::SetSessionConfigOptionResponse>);
        if let Err(error) = &outcome {
            self.option_set_failed(Some(id), option, value, error);
        }
        outcome.map(|_| ())
    }

    /// Notes that a call opening a session is in flight. A second one replaces
    /// the first, which then publishes nothing.
    pub(super) fn opening(&self, call: i64, how: Setup, session: Option<v1::SessionId>) {
        *locked(&self.opening) = Some(Opening {
            call,
            how,
            session,
            stated: None,
        });
    }

    /// Replaces the settings with what the session setup response published.
    ///
    /// A refused open changes nothing. The answer outranks any mode replayed
    /// ahead of it: an answer with no `modes` means the agent offers none, and
    /// the replayed mode is kept only to report that mismatch
    /// ([`SessionSettings::unoffered_mode`]).
    pub(super) fn opened(&self, id: i64, outcome: &Result<Value, v1::Error>) {
        let Some(opening) = locked(&self.opening).take_if(|open| open.call == id) else {
            return;
        };

        let Ok(result) = outcome else {
            return;
        };
        let session = match opening.how {
            Setup::New => result
                .get("sessionId")
                .and_then(Value::as_str)
                .map(v1::SessionId::new),
            Setup::Restore(_) => opening.session.clone(),
        };
        if session.is_some() {
            *locked(&self.opened_last) = session;
        }
        self.stores
            .settings
            .set(opening.how.published(result.clone(), opening.stated));
    }

    /// Notes a mode the agent stated for the session being opened, and answers
    /// whether the update was that session's.
    ///
    /// Only the session the open names counts, so a `session/new` records
    /// nothing: under-reporting rather than misattributing traffic from the
    /// session being left.
    pub(super) fn stated_while_opening(
        &self,
        session: &v1::SessionId,
        mode: &v1::SessionModeId,
    ) -> bool {
        let mut opening = locked(&self.opening);
        let Some(opening) = opening
            .as_mut()
            .filter(|opening| opening.session.as_ref() == Some(session))
        else {
            return false;
        };
        opening.stated = Some(mode.clone());
        true
    }

    /// Whether an update names the session a call in flight is opening.
    pub(super) fn opening_now(&self, session: &v1::SessionId) -> bool {
        locked(&self.opening)
            .as_ref()
            .is_some_and(|opening| opening.session.as_ref() == Some(session))
    }

    /// Records what became of the `session/set_mode` this answer is for.
    ///
    /// A success the agent never restated is recorded as acknowledged; the
    /// surface's mode stays the agent's last statement. If the agent already
    /// announced a mode, nothing is recorded.
    pub(super) fn changed_mode(&self, id: i64, outcome: &Result<Value, v1::Error>) {
        let Some(changing) = locked(&self.changing_mode).take_if(|change| change.call == id) else {
            return;
        };

        match outcome {
            Ok(_) if changing.restated => {}
            Ok(_) => {
                self.stores
                    .settings
                    .update(|settings| settings.acknowledged(changing.mode));
            }
            Err(error) => {
                self.stores.settings.update(|settings| {
                    settings.mode_refused(changing.mode, CallError::Rejected(error.clone()))
                });
            }
        }
    }

    /// Rebuilds the option set from the answer to a `session/set_config_option`,
    /// taking the agent's complete set as given.
    ///
    /// An answer that does not decode is recorded as a refusal. A successful
    /// answer is also checked for the option-set rule (§15 q7); the store is
    /// written first, so a screen woken by the annotation reads current
    /// settings.
    pub(super) fn set_answered(&self, id: i64, outcome: &Result<Value, v1::Error>, answer: &Frame) {
        let Some(setting) = locked(&self.setting_option).take_if(|set| set.call == id) else {
            return;
        };

        let read = match outcome {
            Ok(result) => decode::<v1::SetSessionConfigOptionResponse>(result.clone()),
            Err(error) => Err(CallError::Rejected(error.clone())),
        };
        match read {
            Ok(response) => {
                self.stores
                    .settings
                    .update(|settings| settings.answered(response.config_options));
            }
            Err(error) => {
                self.stores.settings.update(|settings| {
                    settings.option_refused(setting.option.clone(), setting.value, error)
                });
            }
        }

        // The MUST applies only to a set that succeeded. The option list is
        // read from the raw frame, whatever could be decoded of it.
        let Ok(result) = outcome else {
            return;
        };
        if let Some(short) = OptionSet::of(result, &setting.option) {
            self.stores.timeline.annotate(
                Annotation::OptionSet(short),
                vec![setting.request, answer.clone()],
            );
        }
    }

    /// Records that a config option set got no answer at all.
    ///
    /// `registered` is the call's id, or `None` for a failure before it was
    /// sent. A registered failure records nothing if the reader already took
    /// the slot, because then the agent's answer was recorded.
    pub(super) fn option_set_failed(
        &self,
        registered: Option<i64>,
        option: &v1::SessionConfigId,
        value: &v1::SessionConfigOptionValue,
        error: &CallError,
    ) {
        if let Some(id) = registered
            && locked(&self.setting_option)
                .take_if(|set| set.call == id)
                .is_none()
        {
            return;
        }

        self.stores.settings.update(|settings| {
            settings.option_refused(option.clone(), value.clone(), error.clone())
        });
    }

    /// Records that a mode set got no answer at all, guarded like
    /// [`option_set_failed`](Self::option_set_failed).
    pub(super) fn mode_change_failed(
        &self,
        registered: Option<i64>,
        mode: &v1::SessionModeId,
        error: &CallError,
    ) {
        if let Some(id) = registered
            && locked(&self.changing_mode)
                .take_if(|change| change.call == id)
                .is_none()
        {
            return;
        }

        self.stores
            .settings
            .update(|settings| settings.mode_refused(mode.clone(), error.clone()));
    }

    /// Applies a `current_mode_update` or `config_option_update` to the
    /// settings. The timeline keeps the update either way.
    ///
    /// An update for the session still being opened is left to the setup
    /// response, which replaces the settings whole: writing it now would show
    /// the opening session's mode on the session being left, and leave it
    /// there if the open is refused.
    pub(super) fn reconfigured(&self, notification: &v1::SessionNotification) {
        match &notification.update {
            v1::SessionUpdate::CurrentModeUpdate(announced) => {
                // Any announced mode confirms a set in flight, whichever order
                // the announcement and the answer arrive in.
                if let Some(changing) = locked(&self.changing_mode).as_mut() {
                    changing.restated = true;
                }
                if self.stated_while_opening(&notification.session_id, &announced.current_mode_id) {
                    return;
                }
                self.stores
                    .settings
                    .update(|settings| settings.now_in(&announced.current_mode_id));
            }
            v1::SessionUpdate::ConfigOptionUpdate(restated) => {
                if self.opening_now(&notification.session_id) {
                    return;
                }
                self.stores
                    .settings
                    .update(|settings| settings.offers(restated.config_options.clone()));
            }
            _ => {}
        }
    }
}

impl Setup {
    /// The settings a session setup response published.
    ///
    /// An answer that does not decode publishes the default settings: the
    /// session did open, so this costs the surface rather than the call.
    pub(super) fn published(
        self,
        result: Value,
        stated: Option<v1::SessionModeId>,
    ) -> SessionSettings {
        let read = match self {
            Self::New => decode::<v1::NewSessionResponse>(result)
                .map(|answer| (answer.modes, answer.config_options)),
            Self::Restore(Restore::Load) => decode::<v1::LoadSessionResponse>(result)
                .map(|answer| (answer.modes, answer.config_options)),
            Self::Restore(Restore::Resume) => decode::<v1::ResumeSessionResponse>(result)
                .map(|answer| (answer.modes, answer.config_options)),
        };
        read.map(|(modes, config_options)| {
            SessionSettings::published(modes, config_options, stated)
        })
        .unwrap_or_default()
    }
}
