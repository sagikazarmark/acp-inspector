//! What the live session is configured as (`docs/architecture.md` §7.6): its
//! modes and its config options, together.
//!
//! **One store, because what the user reads is one thing.** The protocol names
//! one half — *config option* — and has no word for both, so the pair is the
//! inspector's own (`CONTEXT.md`, *Session Settings*), and the two kinds beneath
//! it keep the protocol's meanings exactly. They arrive by different routes and
//! are changed by different methods; what they have in common is the question a
//! reader is asking, which is what this session is set to.
//!
//! **Advertisement here is presence of data, not a capability** (§7.6). Modes
//! and config options are not claimed in `initialize` and are not per-connection:
//! they are fields on the setup response of the session that is currently live,
//! so a `None` here is the agent's answer that it published none — never this
//! crate's shorthand for "not yet known". That is what lets a surface gate a
//! control on the advertisement and on nothing else.
//!
//! **A decoded view, and it says so** (§7.6, §8). The schema skips invalid
//! entries when it deserializes an option list, so a malformed or future-shaped
//! option can vanish before this crate ever sees it. The raw-first rule is not
//! weakened — every frame is in the trace, complete — but what is held here is
//! what v1 could read of what the agent sent, and a surface over it says as
//! much rather than implying a completeness the typed layer cannot deliver.

use agent_client_protocol_schema::v1;

use crate::call::CallError;

/// The modes and the config options of the live session.
///
/// A value, not a handle: the store is a field the typed layer writes
/// ([`Inspector::session_settings`](crate::Inspector::session_settings)), and
/// this is what one read of it says.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct SessionSettings {
    /// The modes the agent published for this session, with the one it says is
    /// current named among them — or `None` where it published none.
    ///
    /// Changed with `session/set_mode`.
    pub modes: Option<v1::SessionModeState>,
    /// The config options the agent published for this session, each with its
    /// current value — or `None` where it published none.
    ///
    /// Changed with `session/set_config_option`. An empty list is not the same
    /// answer as `None`: an agent that published `[]` said it has a list and it
    /// is empty, and an agent that published nothing said nothing.
    pub config_options: Option<Vec<v1::SessionConfigOption>>,
    /// What became of the last `session/set_mode` sent in this session, where
    /// there is anything left to say about it — and `None` where none has been
    /// sent, or where the agent has since stated a mode and settled one it had
    /// only acknowledged ([`now_in`](Self::now_in)).
    ///
    /// **It is never what [`modes`](Self::modes) says the session is in.** The
    /// mode on the row is the agent's word; this is the record of an ask, which
    /// is a different thing and stays a different field.
    pub mode_change: Option<ModeChange>,
    /// Why the last `session/set_config_option` sent in this session did not
    /// happen — and `None` where none has been sent, or where the last one was
    /// answered.
    ///
    /// **Only the refusals, because a success has nothing left to say.**
    /// `session/set_config_option` answers with the complete replacement option
    /// set, so what became of a set that worked is [`config_options`] itself:
    /// there is no acknowledged-and-unconfirmed state to be in, which is the
    /// whole of the asymmetry with [`mode_change`](Self::mode_change) and the
    /// protocol's rather than a decision made here.
    ///
    /// [`config_options`]: Self::config_options
    pub config_refusal: Option<ConfigRefusal>,
    /// A mode the agent stated while this session was opening, where the answer
    /// that opened it then published no modes at all — and `None` in every
    /// other case, which is nearly all of them.
    ///
    /// **The one case two of the ring's rules collide in, held so a surface can
    /// say what happened.** `session/load` MUST replay the conversation before
    /// it answers, so a replayed `current_mode_update` crosses ahead of the
    /// response that carries [`modes`](Self::modes); one rule says the store is
    /// fed by the announcements and the other says it is rebuilt from the setup
    /// response. **The response wins, absolutely** — no advertisement, no
    /// affordance — and this is the record of what the agent did to make the
    /// two disagree: it stated a mode change it did not offer. An agent author
    /// would want to know they had done that, and a mode control inferred from
    /// replayed traffic would be the inspector deciding on the agent's behalf
    /// what it supports (§7.6).
    ///
    /// **Held for either way of reopening a session**, because the frames are
    /// the same two frames in the same order: a `session/load` MUST replay
    /// ahead of its answer and a `session/resume` MUST NOT replay at all, and
    /// an agent that did it under a resume did the thing this is about with a
    /// rule broken on top. What is named is what crossed; which of the two it
    /// was is the trace's to say.
    ///
    /// **It is not a mode the session is in and gates nothing.** What it names
    /// is a frame that crossed; the announcement itself is on the timeline,
    /// where *when* a setting changed stays on the record, and the frame is in
    /// the trace (§8).
    pub unoffered_mode: Option<v1::SessionModeId>,
}

/// A `session/set_config_option` this crate could not take, and why (§7.6).
///
/// The agent's own refusal, or the news that there was nobody to ask — the
/// same pair [`ModeChange::Refused`] holds, and held for the same reason: a
/// control that did nothing and said nothing would be the window keeping a
/// secret this store is there to tell, and the frame is in the trace either way
/// (§8).
///
/// **And a third case this setter has and the other does not**: an answer this
/// layer could not read as v1 ([`CallError::Undecodable`]). That is not the
/// agent saying no and is not claimed to be — what the sentence carries is the
/// decoder's own complaint — but it is the same news for the surface, because
/// the option set is *the answer*, and a list nobody could read is not one the
/// store may be rebuilt from. The frame is in the trace, complete, which is
/// where a reader goes to see what the agent actually sent (§8).
///
/// **Nothing was applied, so nothing is rolled back.** The option set is
/// whatever the agent last stated, before the set and after it.
///
/// Constructible, unlike most of what this crate hands out: it is what a
/// refusal *is* rather than a handle on something core owns, and both the
/// window that renders one and the tests that state one expected read it whole.
#[derive(Clone, Debug, PartialEq)]
pub struct ConfigRefusal {
    /// The option that was asked about, which is what the refusal is about —
    /// and not necessarily one the agent still publishes.
    pub option: v1::SessionConfigId,
    /// The value it was asked for, in the shape the write carried it: a bare
    /// value id for a select, a boolean under its `type` discriminator for a
    /// boolean.
    ///
    /// **The ask as it went out, not the option's kind restated.** The
    /// discriminator describes the shape of a *value* rather than the kind of
    /// an option, and what a refusal is about is the value that was sent — so
    /// this is the same type the request carried, and a surface reporting the
    /// refusal is reporting what crossed the wire.
    pub value: v1::SessionConfigOptionValue,
    pub error: CallError,
}

/// What became of a `session/set_mode` (§7.6).
///
/// **Both arms leave the mode alone**, which is what "nothing is applied
/// optimistically" is: the surface goes on showing whatever the agent last
/// stated, and this says what happened to the ask beside it. A row that moved
/// to the requested mode would be the inspector stating a configuration its
/// agent never claimed; a row that said nothing at all would read as a broken
/// control.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ModeChange {
    /// The agent answered the set, and has said nothing about its mode since.
    ///
    /// Acknowledged and unconfirmed: `session/set_mode` answers `{}` and **no
    /// rule anywhere obliges the agent to follow up**, so this is a permitted
    /// silence reported as such rather than a fault. A `current_mode_update`
    /// settles it ([`SessionSettings::now_in`]).
    Acknowledged(v1::SessionModeId),
    /// The set did not happen, and this is why.
    ///
    /// The agent's own refusal, or the news that there was nobody to ask. It is
    /// held beside the surface rather than left in the trace because a control
    /// that did nothing and said nothing would be the window keeping a secret
    /// this store is there to tell — and the frame is in the trace either way
    /// (§8).
    Refused {
        /// The mode that was asked for, which is what the refusal is about.
        mode: v1::SessionModeId,
        error: CallError,
    },
}

impl SessionSettings {
    /// What a session setup response published.
    ///
    /// The store's first source and the one that outranks the others (§7.6):
    /// this is the whole of what the newly-opened session is configured as, so
    /// it replaces rather than merges.
    ///
    /// `stated` is whatever mode the agent named while this session was opening
    /// — a `session/load`'s replay, in the case the rule is about — and it is
    /// kept only where the answer published no modes to gate a control on
    /// ([`unoffered_mode`](Self::unoffered_mode)). An answer that *did* publish
    /// modes has said what the session's mode is, and a replay that preceded it
    /// is a thing that happened on the timeline rather than a disagreement.
    pub(crate) fn published(
        modes: Option<v1::SessionModeState>,
        config_options: Option<Vec<v1::SessionConfigOption>>,
        stated: Option<v1::SessionModeId>,
    ) -> Self {
        Self {
            unoffered_mode: stated.filter(|_| modes.is_none()),
            modes,
            config_options,
            // A session that has just opened has been asked for nothing. The
            // whole value is replaced rather than folded into, which is what
            // makes "switching sessions discards and rebuilds the settings"
            // true of these fields as well as of the other two.
            mode_change: None,
            config_refusal: None,
        }
    }

    /// Whether the agent published no settings at all for this session.
    ///
    /// Which is a thing a surface says out loud rather than a reason to draw
    /// nothing: an absence that reads as the agent's answer is the point of
    /// gating on the advertisement.
    ///
    /// What *this client* asked for does not count, because publishing is the
    /// agent's: an agent that published nothing has published nothing however
    /// many sets it has been sent.
    pub fn is_empty(&self) -> bool {
        self.modes.is_none() && self.config_options.is_none()
    }

    /// The agent says the session is in a mode now — a `current_mode_update`.
    ///
    /// **It names a mode, not a menu**, so the set that was published is
    /// untouched. And an announcement from an agent that published *no* modes
    /// changes nothing at all: the advertisement is the setup response's, and
    /// inferring one from traffic would be the inspector deciding on the agent's
    /// behalf what it supports (§7.6). The announcement is on the timeline
    /// either way, which is where *when* a setting changed stays on the record.
    ///
    /// **And it settles an acknowledgement**, which is the agent that announces
    /// its change being rewarded with a surface that agrees with it: what an
    /// acknowledgement claims is that the agent has said nothing about the mode
    /// since, and an agent that states a mode has said something. *Any*
    /// announcement settles it, even one naming a mode nobody asked for —
    /// that reads the way the load rule reads a replay, and can only make this
    /// quieter than it might have been rather than wrong about an agent.
    ///
    /// **A refusal survives it.** That one claims the agent answered *no* to a
    /// particular ask, which stays true however many modes it states
    /// afterwards; taking it back would leave a user who asked for a mode they
    /// cannot have with nothing on screen saying so. It goes when the next set
    /// replaces it, or with the session it was asked in.
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

    /// The agent answered a `session/set_mode` and said nothing else about it
    /// (§7.6).
    ///
    /// The mode is left exactly where the agent's last statement put it: what
    /// is recorded is the ask and its acknowledgement, so that a surface can
    /// say the set was made and not confirmed instead of inventing a value or
    /// pretending nothing happened.
    ///
    /// Answers with whether anything changed.
    pub(crate) fn acknowledged(&mut self, mode: v1::SessionModeId) -> bool {
        self.record(ModeChange::Acknowledged(mode))
    }

    /// The mode was not set, and this is what was said about it (§7.6).
    ///
    /// The agent's refusal, or the news that there was nobody to ask. Nothing
    /// else moves: a set that failed cost the attempt, and the mode it did not
    /// change is still whatever the agent last stated.
    ///
    /// Answers with whether anything changed.
    pub(crate) fn mode_refused(&mut self, mode: v1::SessionModeId, error: CallError) -> bool {
        self.record(ModeChange::Refused { mode, error })
    }

    /// Holds what became of a set, where it is news.
    ///
    /// One place, because [`acknowledged`](Self::acknowledged) and
    /// [`mode_refused`](Self::mode_refused) differ in what they report and not in what
    /// they do with it.
    fn record(&mut self, change: ModeChange) -> bool {
        if self.mode_change.as_ref() == Some(&change) {
            return false;
        }
        self.mode_change = Some(change);
        true
    }

    /// The agent restates its config options — a `config_option_update`.
    ///
    /// **The complete set, so the store takes it whole.** A list that has lost
    /// an option is the agent saying its session has one fewer, which is its
    /// word about its own session; folding the new list into the old one would
    /// be the inspector holding on to an option the agent stopped publishing.
    ///
    /// **And it is taken even where the setup response published none**, which
    /// is where this parts company with [`now_in`](Self::now_in) — the
    /// asymmetry is the protocol's rather than a decision made here. A mode
    /// announcement names *one mode* and no set, so there is nothing in it an
    /// affordance could be built from and inferring one would be the inspector
    /// deciding what the agent supports. This notification carries **the whole
    /// list**: taking it is taking the agent's word about its own session, and
    /// refusing it would be a surface showing less than the agent sent, which
    /// is the one thing this layer may not do (§8). What the setup response
    /// outranks is a replay that came *before* it, never what the agent says
    /// next.
    ///
    /// Answers with whether anything changed.
    pub(crate) fn offers(&mut self, options: Vec<v1::SessionConfigOption>) -> bool {
        if self.config_options.as_deref() == Some(options.as_slice()) {
            return false;
        }
        self.config_options = Some(options);
        true
    }

    /// The agent answered a `session/set_config_option` with an option set
    /// (§7.6).
    ///
    /// **The store is rebuilt from the list the agent returned, and that is the
    /// whole of it.** The answer is the complete replacement set, so nothing
    /// here merges it into what was held, patches the option that was set, or
    /// assumes that option is the only one that moved: an agent is entitled to
    /// answer a single write with a wholly different set, and if it does, that
    /// is what the surface shows. It is [`offers`](Self::offers)'s rule reached
    /// by the other route, which is why they take the list the same way.
    ///
    /// **And the ask is settled**, because this is the answer to it: a refusal
    /// held here is what became of the *last* set, and the last set is this
    /// one. A success has nothing else to say — what it did is in the option
    /// set the agent just sent.
    ///
    /// Answers with whether anything changed.
    pub(crate) fn answered(&mut self, options: Vec<v1::SessionConfigOption>) -> bool {
        let settled = self.config_refusal.take().is_some();
        self.offers(options) || settled
    }

    /// The config option was not set, and this is what was said about it
    /// (§7.6).
    ///
    /// The agent's refusal, or the news that there was nobody to ask. Nothing
    /// else moves: a set that failed cost the attempt, and the options it did
    /// not change are still the ones the agent last stated. It is
    /// [`mode_refused`](Self::mode_refused)'s rule for the other setter, and holds for
    /// the same reasons — including that a later `config_option_update` does
    /// not take it back, because what it claims is that the agent answered *no*
    /// to a particular ask.
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
