//! Session settings (§7.6): the two setters, and reading what a setup
//! response, a setter's answer or an announcement says the session is
//! configured as.

use agent_client_protocol_schema::v1;
use serde_json::Value;

use crate::call::CallError;
use crate::conformance::{Annotation, OptionSet};
use crate::frame::Frame;
use crate::restore::Restore;
use crate::session_settings::SessionSettings;
use crate::store::locked;

use super::{Client, decode, encode};

/// A `session/set_mode` on the wire, and what has crossed since it went out.
///
/// Named for the call rather than for the store it writes into, the way
/// [`Opening`] and [`Loading`] are — `session/set_config_option` has one of
/// these of its own ([`SettingOption`]), and *changing* on its own would be
/// ambiguous between them.
pub(super) struct ChangingMode {
    /// The id its answer will arrive under, which is what says the answer is
    /// this set's.
    pub(super) call: i64,
    /// The mode that was asked for. Kept because what a surface reports is the
    /// *ask* — the mode itself is only ever the agent's word.
    pub(super) mode: v1::SessionModeId,
    /// Whether the agent has stated a mode since this went out.
    ///
    /// An agent that announced before it answered has confirmed the change,
    /// and an answer that then reported the set as unrestated would be
    /// contradicting a frame already read. Anything that crossed counts,
    /// whatever mode it named ([`SessionSettings::now_in`] says why).
    pub(super) restated: bool,
}

/// A `session/set_config_option` on the wire.
///
/// **Nothing is watched for while it is out**, which is where this parts
/// company with [`ChangingMode`]: the answer carries the complete option set,
/// so there is nothing an announcement could settle and nothing left
/// unconfirmed to report. What is kept is the ask, because a refusal is about
/// the ask and the answer to a refused set says nothing about what was asked —
/// and because the third rule is decided from it (§15 q7).
pub(super) struct SettingOption {
    /// The id its answer will arrive under, which is what says the answer is
    /// this set's.
    pub(super) call: i64,
    /// The option that was asked about.
    pub(super) option: v1::SessionConfigId,
    /// The value it was asked for, in the shape the write carried it.
    pub(super) value: v1::SessionConfigOptionValue,
    /// The request frame, kept for [`Loading`]'s reason: the annotation is read
    /// beside the frames that decide it, and this is the half that says which
    /// option the answer owed.
    pub(super) request: Frame,
}

/// A call that opens a session, on the wire.
pub(super) struct Opening {
    /// The id its answer will arrive under, which is what says the answer is
    /// this call's.
    pub(super) call: i64,
    pub(super) how: Setup,
    /// The session it is opening, where that is known before the answer — which
    /// is every way of opening one but `session/new`, whose id the agent mints
    /// in the answer itself.
    ///
    /// What it is for is telling a *replay* from the traffic beside it: a call
    /// that reopens a session says which session it is asking for, so an
    /// announcement naming that session while the call is out is that session's
    /// own account of itself, and one naming the session being left is the last
    /// word of a conversation this client walked away from. Attributing the
    /// second to the session that is opening would be reporting an asymmetry
    /// the agent never produced.
    pub(super) session: Option<v1::SessionId>,
    /// The mode the agent has stated for that session since the call went out,
    /// where it has stated one.
    ///
    /// The last one, because that is what the session's mode would be: an agent
    /// replaying several mode changes replayed a conversation that ended in the
    /// last of them.
    pub(super) stated: Option<v1::SessionModeId>,
}

/// Which call is opening the session, and therefore how its answer reads.
///
/// **One thing to a reader and three types in the schema.** `session/new`,
/// `session/load` and `session/resume` answer with three different structs
/// carrying the same two optional fields, which is why the inspector's own
/// vocabulary has a word for what they have in common and the protocol does not
/// (`CONTEXT.md`, *Session setup response*).
#[derive(Clone, Copy)]
pub(super) enum Setup {
    New,
    Restore(Restore),
}

impl Client {
    /// `session/set_mode`: asks the agent to put the live session in a mode
    /// (§7.6).
    ///
    /// **Nothing is applied optimistically, so nothing is rolled back.** The
    /// mode on the surface is the agent's own last statement of it, and stays
    /// that whatever this call answers: an inspector that moved the row when it
    /// asked would be stating a configuration its agent never claimed, which is
    /// the one thing this tool must never do. What the answer produces is a
    /// [`ModeChange`](crate::ModeChange) beside it — an acknowledgement the
    /// agent never restated, or a refusal — written by the reader that sees the
    /// answer cross ([`changed_mode`](Self::changed_mode)).
    ///
    /// **Sent whenever there is a session to send it about**, including one
    /// with a turn in flight: the specification is silent on what an agent
    /// should do with a change it was not expecting, and an inspector that
    /// withheld the call would be deciding on the agent's behalf what may be
    /// observed (§7.6). Gating on the advertisement is the *surface's*, and it
    /// is the presence of published modes and nothing else.
    ///
    /// Registered before the frame goes out, for the reason a turn registers
    /// its id before the prompt does: the answer is allowed to arrive before
    /// anyone here is looking for it.
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
        // Nothing is read out of a successful answer: `session/set_mode`
        // answers `_meta` and nothing else, so what the answer is here is the
        // agent's word that it did what it was asked — and the whole point of
        // the ring is that its word is all there is. The frame is in the trace
        // either way (§8).
        outcome.map(|_| ())
    }

    /// `session/set_config_option`: asks the agent to set one of the live
    /// session's config options (§7.6).
    ///
    /// **The answer is the complete replacement option set, and the store takes
    /// it.** There is nothing to infer and nothing to compute here: the option
    /// set is rebuilt from the list the agent returned, whatever is in it —
    /// nothing merges, patches or assumes the option that was set is the only
    /// one that moved. Written by the reader that sees the answer cross
    /// ([`set_answered`](Self::set_answered)), for the reason the mode setter's
    /// outcome is.
    ///
    /// **The value carries its own shape, and the schema serializes each of
    /// them.** A select's goes out as a bare value id — no `type` discriminator
    /// over it — and a boolean's goes out as `"type":"boolean"` beside an
    /// unquoted `value`, because the discriminator describes the *shape of a
    /// value* rather than the kind of an option. Handing this call a
    /// [`v1::SessionConfigOptionValue`] rather than a value id is what makes
    /// that the schema's business instead of this crate's: a client that sent
    /// the one shape where the other was required is a mistake a real client
    /// made against a real agent, and the type is where it stops being possible.
    ///
    /// **The boolean shape exists on the wire only because this client claimed
    /// it** ([`client_capabilities`]): a conformant agent must not publish a
    /// boolean config option to a client that never advertised
    /// `clientCapabilities.session.configOptions.boolean`, so nothing here has
    /// a kind to write until that claim goes out.
    ///
    /// **Sent whenever there is a session to send it about**, including one
    /// with a turn in flight, and gated at the surface on the advertisement and
    /// nothing else — [`set_mode`](Self::set_mode) argues both, and the
    /// argument is the same one.
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
            // Read a second time by whoever asked, so that what this answers is
            // what the surface says: an answer the reader could not take is a
            // set that did not happen, and a caller told otherwise would be
            // told something the store disagrees with.
            .and_then(decode::<v1::SetSessionConfigOptionResponse>);
        if let Err(error) = &outcome {
            self.option_set_failed(Some(id), option, value, error);
        }
        outcome.map(|_| ())
    }

    /// Says that a call which opens a session is on the wire (§7.6).
    ///
    /// A second one registered while the first is still out replaces it, and
    /// the first then publishes nothing: two sessions opening at once is one
    /// user pressing two buttons, and the surface belongs to whichever session
    /// ends up live. The same simplification the load rule makes about a second
    /// load.
    pub(super) fn opening(&self, call: i64, how: Setup, session: Option<v1::SessionId>) {
        *locked(&self.opening) = Some(Opening {
            call,
            how,
            session,
            stated: None,
        });
    }

    /// Fills the settings from the session setup response that just crossed
    /// (§7.6).
    ///
    /// **The store's first source, written where the frame is read.** What the
    /// answer published is the whole of what the session that is opening is
    /// configured as, so it replaces rather than merges — and an announcement
    /// the agent makes after it is a later fact about the same session, which
    /// is why this is written in wire order rather than by whoever awaited the
    /// call.
    ///
    /// **A refused open published nothing**, and changes nothing: the session
    /// was not opened, so the surface still describes whichever one is live.
    ///
    /// **And the answer outranks whatever was replayed ahead of it**, which is
    /// the one case the ring's two rules collide in (§7.6). A `session/load`
    /// MUST replay before it answers, so a mode the agent stated on the way is
    /// a frame this reader has already taken — and an answer publishing no
    /// `modes` is the agent saying it offers none, which wins absolutely. What
    /// the replay is kept for is saying so
    /// ([`SessionSettings::unoffered_mode`]): the agent stated a mode change it
    /// did not offer, and that asymmetry is worth seeing.
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

    /// Notes a mode the agent stated for the session that is opening (§7.6).
    ///
    /// **Only for the session the open names**, which is what makes this a
    /// reading of a replay rather than of whatever else is crossing: a
    /// `session/load` says which session it wants back, and the session being
    /// left is entitled to go on talking while it does — the last words of a
    /// turn the switch just cancelled, say. Attributing those to the session
    /// that is opening would report an asymmetry the agent never produced.
    ///
    /// **So a `session/new` records nothing**, because its session has no id
    /// until the answer mints one and nothing crossing before that can be shown
    /// to be about it. That is under-reporting rather than mis-reporting, which
    /// is the direction a rule decided from frames is allowed to fail in.
    ///
    /// Answers with whether the update was that session's, which is also the
    /// answer to whether the settings store should take it
    /// ([`reconfigured`](Self::reconfigured)).
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

    /// Whether an update names the session a call still on the wire is opening.
    ///
    /// [`stated_while_opening`](Self::stated_while_opening) without the mode to
    /// remember: the other settings notification carries a whole option set,
    /// which the setup response is about to replace whatever it says, so there
    /// is nothing to keep and only the question of whose account it is.
    pub(super) fn opening_now(&self, session: &v1::SessionId) -> bool {
        locked(&self.opening)
            .as_ref()
            .is_some_and(|opening| opening.session.as_ref() == Some(session))
    }

    /// Says what became of the `session/set_mode` this answer is the answer to
    /// (§7.6).
    ///
    /// **A success the agent never restated is the case the ring is about.**
    /// The answer carries nothing, and no rule obliges the agent to say
    /// anything afterwards, so what is recorded is the ask and its
    /// acknowledgement — and the mode on the surface stays the agent's own last
    /// statement of it. An agent that *did* announce, before answering, has
    /// already settled it: `restated` is that frame, read where it crossed, and
    /// nothing is recorded over it.
    pub(super) fn changed_mode(&self, id: i64, outcome: &Result<Value, v1::Error>) {
        let Some(changing) = locked(&self.changing_mode).take_if(|change| change.call == id) else {
            return;
        };

        match outcome {
            // Already settled by a frame this reader saw first: the agent
            // announced the change before answering the set, so there is
            // nothing left unconfirmed to report.
            Ok(_) if changing.restated => {}
            Ok(_) => {
                self.stores
                    .settings
                    .update(|settings| settings.acknowledged(changing.mode));
            }
            // The refusal is written here rather than by whoever awaited the
            // call, for the reason the acknowledgement is: this is where the
            // frame is, and the store must not depend on anybody being awake.
            Err(error) => {
                self.stores.settings.update(|settings| {
                    settings.mode_refused(changing.mode, CallError::Rejected(error.clone()))
                });
            }
        }
    }

    /// Rebuilds the option set from the answer to the `session/set_config_option`
    /// this answer is the answer to (§7.6).
    ///
    /// **The agent's word, taken entirely.** The answer is the complete
    /// replacement set, so the store is rebuilt from it — an agent that answers
    /// a single write with an option set differing in more than the option that
    /// was written has that whole set reflected, because that is what it said
    /// about its own session.
    ///
    /// An answer this layer could not read is a set that did not happen as far
    /// as the surface goes: nothing is rebuilt from a list nobody could read,
    /// and a control that was pressed says what became of it rather than
    /// nothing. The frame is in the trace, complete (§8).
    ///
    /// **And the third rule is judged here** (§15 q7), because this is where
    /// both of the frames that decide it are: the set this client sent, and the
    /// answer that says it succeeded. The store first and the annotation
    /// second, so a screen woken by the timeline moving reads settings that
    /// already account for what woke it.
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

        // A set the agent *refused* owed no option list at all, which is the
        // rule's own boundary and not mercy: the MUST is on the answer to a set
        // that succeeded. What that answer carried is read from the frame
        // itself ([`OptionSet::of`]), whatever this client could decode of it.
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

    /// Says beside the surface that a config option could not be set, where
    /// nobody answered the set at all (§7.6).
    ///
    /// [`mode_change_failed`](Self::mode_change_failed)'s rule, and the same
    /// pair of cases judged the same way: an unregistered failure is this
    /// call's alone, and a registered one says nothing where the reader has
    /// already said what the agent answered.
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

    /// Says beside the surface that a mode could not be set, where nobody
    /// answered the set at all (§7.6).
    ///
    /// `registered` is the id it went out under, or `None` for a failure that
    /// happened before it was a frame — the two judged the way
    /// [`turn_failed`](Self::turn_failed) judges the same pair. An
    /// unregistered failure is this call's alone and nothing can have replaced
    /// it; a registered one says nothing where the reader has already said what
    /// the agent answered, because a set the agent answered is the agent's news
    /// and not the caller's.
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
            // The reader has already said what the agent answered, and a set
            // the agent answered is the agent's news rather than this caller's.
            return;
        }

        self.stores
            .settings
            .update(|settings| settings.mode_refused(mode.clone(), error.clone()));
    }

    /// Reads a settings change out of an update that is one (§7.6).
    ///
    /// Two of the eleven stable variants say what the live session is
    /// configured as: `current_mode_update` names the mode it is now in, and
    /// `config_option_update` restates the option set whole. Both are the
    /// store's third source, and neither *advertises* anything — that is the
    /// setup response's to do, which is why a mode announced by an agent that
    /// published no modes leaves this store alone
    /// ([`SessionSettings::now_in`]).
    ///
    /// **The timeline keeps them either way.** The surface says what a setting
    /// is now; the timeline is where *when* it changed stays on the record, and
    /// this reads the update on the way past rather than in place of it.
    ///
    /// **And a replay is not one of them.** An announcement naming the session
    /// a `session/load` is still asking for is that session's account of
    /// itself, and the store does not describe that session yet — what it
    /// describes is whichever one is live until the answer arrives. Writing the
    /// replay here would put the opening session's mode on the row of the
    /// session being left, and a load the agent then *refused* would leave it
    /// there for good: a session showing a mode it never stated. So the replay
    /// is left to the answer, which rebuilds the store from the setup response
    /// whole — the ordering rule reached from the other side (§7.6). The mode
    /// it named is kept where a refused set's is, beside the surface
    /// ([`SessionSettings::unoffered_mode`]), and the frame is on the timeline
    /// and in the trace either way (§8).
    pub(super) fn reconfigured(&self, notification: &v1::SessionNotification) {
        match &notification.update {
            v1::SessionUpdate::CurrentModeUpdate(announced) => {
                // A set still on the wire has been answered by the agent's own
                // announcement, whichever order the two arrive in: an agent
                // that states a mode has restated it, and the answer that comes
                // next must not report the set as unrestated over the top of a
                // frame already read.
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
    /// The settings a session setup response published (§7.6).
    ///
    /// Three arms because the schema gives three types, exactly as
    /// [`Client::restore`]'s request does: the responses carry the same two
    /// optional fields, and which type they are read as is the whole of what
    /// differs.
    ///
    /// **An answer this layer could not read published nothing.** The session
    /// was opened — the agent answered, and that answer is what made it live —
    /// so a decoding failure here is this client failing to recognize
    /// something rather than a session that did not open, and it costs the
    /// surface rather than the call. The frame is in the trace, complete (§8).
    ///
    /// **And an answer nobody could read says nothing about a replay either**,
    /// which is what falling back to the default settings leaves: whether the
    /// agent offered the mode it stated is a question about a field in an
    /// answer this layer could not get to, so the surface reports what it has
    /// rather than a disagreement it cannot show.
    pub(super) fn published(
        self,
        result: Value,
        stated: Option<v1::SessionModeId>,
    ) -> SessionSettings {
        // The two fields, off whichever of the three types this answer is. What
        // is done with them is said once below, because it is one rule: which
        // call published them changes nothing about what they mean.
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
