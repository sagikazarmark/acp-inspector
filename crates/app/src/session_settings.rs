//! The Session Settings surface (`docs/architecture.md` §7.6, §9): what the
//! live session is configured as, and what it could be configured as.
//!
//! **It sits in the right details rail because it belongs to the Session.** The
//! Agent rail is Connection-scoped — the Agent's identity, its capabilities,
//! its auth, its Session listing — so putting these rows there would say they
//! belong to the Connection, which is exactly the misreading the third gating
//! shape exists to prevent: Modes and Config Options are *data on a Session
//! setup response*, not capabilities, and they describe the one Session that is
//! open. A Mode is also what a user sets immediately before prompting, so the
//! rail remains next to the prompt whose answer it changes.
//!
//! **Each row names the method that would change it.** A mode row goes out as
//! `session/set_mode` and a config option row as `session/set_config_option`, so
//! which method a control drives is readable without going to the trace — and
//! where an agent published the same setting both ways, **both rows appear**,
//! because a duplicated control is a fact about that agent rather than a mess to
//! tidy away.
//!
//! **The affordance is gated on presence of the data and on nothing else.** An
//! agent that published no modes gets no mode row at all, so that absence reads
//! as the agent's answer rather than as an empty widget. What is gated is core's
//! answer ([`SessionSettings`](acp_inspector_core::SessionSettings)); this file
//! renders it.
//!
//! **Both rows are driven, using the native control suited to their values.** A
//! compact pair remains directly exercisable as a segmented group, while three
//! or more, long or grouped values use a native select and a boolean uses one
//! native checkbox-backed toggle. All of them send the same setter payloads as
//! the controls they replace, including during a live turn (§7.6).
//!
//! **The two kinds write different value shapes, and the control that knows the
//! kind is what decides which.** A select's write carries a bare value id, a
//! boolean's carries the boolean under a `type` discriminator — the
//! discriminator describes the shape of a *value* rather than the kind of an
//! option — and a boolean option is on the wire at all only because this client
//! claimed the capability that lets an agent publish one (§7.3).
//!
//! **Nothing is applied optimistically, so the row never moves on its own.** What
//! a mode row shows is the mode the *agent* last stated, and what a set produces
//! is a sentence beside it: acknowledged and not restated, where the agent
//! answered and said nothing more, or the refusal it answered with. Both are
//! core's answer ([`ModeChange`](acp_inspector_core::ModeChange)) — displaying the
//! requested mode would be this window stating a configuration its agent never
//! claimed, and displaying nothing would read as a broken control.
//!
//! **A config option row moves only because the agent moved it**, which is the
//! same rule reached from the other side: `session/set_config_option` answers
//! with the complete replacement option set, so what these rows show is the list
//! core was handed. Nothing here merges an answer into what was on screen. What
//! is left to say is a refusal ([`ConfigRefusal`](acp_inspector_core::ConfigRefusal)),
//! said under the row it is about — or beside the surface, where the agent's own
//! answer has since stopped publishing the option it was about.
//!
//! **And it says what it is**: a decoded view. The schema skips invalid entries
//! when it deserializes an option list, so a malformed or future-shaped option
//! can vanish before core ever sees it. The raw-first rule is not weakened — the
//! frame is in the trace, complete (§8) — but a surface that under-reported
//! without saying so would imply a completeness the typed layer cannot deliver.

use acp_inspector_core::{ConfigRefusal, ModeChange, v1};
use dioxus::prelude::*;

use crate::update::current_value;

/// A value the user picked for a config option: the option, and the value.
///
/// Both, because `session/set_config_option` names both and a value means
/// nothing without the option it belongs to — two agents may well offer a value
/// called `default`.
///
/// **The value carries its own shape.** A select's is a value id and a
/// boolean's is a boolean, and which one goes out is decided where the control
/// is drawn — by the kind of the option the user clicked — rather than by
/// anything downstream reading the option back to find out. The schema puts the
/// `type` discriminator on the wire for the one and leaves it off the other.
#[derive(Clone, PartialEq)]
pub struct Choice {
    pub option: v1::SessionConfigId,
    pub value: v1::SessionConfigOptionValue,
}

/// The live Session's modes and Config Options, in the right details rail.
#[component]
pub fn SessionSettings(
    /// What core made of the session setup response, and of everything the
    /// agent has said about its settings since — including what became of the
    /// last mode this window asked for.
    settings: acp_inspector_core::SessionSettings,
    /// Changes whenever a Session or Connection replaces the one these
    /// controls belong to, so unsubmitted native-select choices do not leak.
    settings_epoch: u64,
    /// Whether the agent that published all this is still there. The settings
    /// outlive it on purpose (§5), so the surface goes on describing a session
    /// whose agent has gone — but a set needs somebody to answer it, and a
    /// control that cannot do what it says is worse than one that says it
    /// cannot.
    connected: bool,
    /// Whether a `session/set_mode` call is waiting for its answer.
    changing_mode: bool,
    /// The Config Option whose `session/set_config_option` call is waiting. The
    /// setter has one pending slot, so every Config Option control yields until
    /// that answer arrives while the named row says what is loading.
    setting_option: Option<v1::SessionConfigId>,
    /// A mode the user picked, on its way to `session/set_mode`. The id and
    /// nothing else: that is the whole of what the call carries.
    on_set_mode: EventHandler<v1::SessionModeId>,
    /// A value the user picked for a config option, on its way to
    /// `session/set_config_option`.
    on_set_config_option: EventHandler<Choice>,
) -> Element {
    let modes = settings.modes.as_ref();
    let options = settings.config_options.as_deref().unwrap_or_default();
    let count = rows(&settings);
    // Where a config option refusal goes, decided once: under the row it names,
    // and beside the surface where there is no such row. An agent's answer is
    // the complete replacement set, so it is entitled to stop publishing the
    // very option a set was refused for — and a refusal drawn only under a row
    // it matched would vanish with the row, which would be the window keeping
    // the secret this surface exists to tell.
    let refusal = settings.config_refusal.as_ref();
    let refused_row = refusal.and_then(|refusal| {
        options
            .iter()
            .position(|option| option.id == refusal.option)
    });

    rsx! {
        section { class: "rail-group settings", "data-slot": "session-settings",
            if count == 0 {
                // An absence that reads as the agent's answer (§7.6). Said
                // rather than drawn as an empty control, because a reader
                // looking at nothing has to be able to tell "this agent
                // published none" from "this tool has not got round to it" —
                // and the two ways of publishing none are two different answers
                // an agent gave, so they are two different sentences.
                p { class: "alert", "data-slot": "settings-notice", role: "status",
                    if settings.is_empty() {
                        "This agent published no modes and no config options for this session — neither "
                        code { "modes" }
                        " nor "
                        code { "configOptions" }
                        " was on the response that opened it."
                    } else {
                        "This agent published an empty "
                        code { "configOptions" }
                        " list for this session: a list, and nothing in it."
                    }
                }
            }

            // The modes, as the drawing draws them: one full-width row per mode,
            // the id the wire carries at the near edge and the name the agent
            // gave it at the far one, with the current one bounded in the
            // accent. Every mode is a control, including the one the session is
            // already in: what an agent does with a set to the mode it is
            // already in is a thing to find out (§7.6).
            if let Some(modes) = modes {
                div {
                    class: "rail-group",
                    "data-slot": "setting",
                    aria_busy: changing_mode,
                    div { class: "rail-head",
                        span { class: "eyebrow", "Mode" }
                        span { class: "setting-note", "{v1::AGENT_METHOD_NAMES.session_set_mode}" }
                    }
                    // Which mode the session is in is the bounded row, and it
                    // is said in words only where there is no such row: an
                    // agent may state a mode it never offered, and a surface
                    // whose whole account of the current mode was a mark on one
                    // of the offered values would draw that session as being in
                    // no mode at all.
                    if !modes.available_modes.iter().any(|mode| mode.id == modes.current_mode_id) {
                        {setting_current(Some((modes.current_mode_id.to_string(), false)), None)}
                    }
                    {mode_values(modes, settings_epoch, connected && !changing_mode, on_set_mode)}
                    if changing_mode {
                        p { class: "hint", "data-slot": "setting-loading", role: "status",
                            "Changing Mode with "
                            code { "{v1::AGENT_METHOD_NAMES.session_set_mode}" }
                            "..."
                        }
                    }
                    // What became of the last set, where there is anything left
                    // to say about it — under the rows it is about.
                    if let Some(change) = settings.mode_change.as_ref() {
                        {became_of(change)}
                    }
                }
            }

            // And one row per config option, whatever its id — an agent that
            // also published one called `mode` published two settings, and this
            // is where both of them are.
            if !options.is_empty() {
                div { class: "rail-group",
                    div { class: "rail-head",
                        span { class: "eyebrow", "Settings" }
                        span { class: "setting-note", "{v1::AGENT_METHOD_NAMES.session_set_config_option}" }
                    }
                    for (row, option) in options.iter().enumerate() {
                        {
                            let description_id = option.description.as_ref().map(|_| format!("config-description-{row}"));
                            let loading = setting_option.as_ref() == Some(&option.id);
                            rsx! { div {
                            key: "{option.id}",
                            class: "setting",
                            "data-slot": "setting",
                            aria_busy: loading,
                            div { class: "setting-line",
                                span { class: "setting-name", "{option.name}" }
                                {setting_current(stated_current(&option.kind), Some(option.id.to_string()))}
                            }
                            if let Some(description) = option.description.as_deref() {
                                p { class: "hint", id: description_id.clone(), "{description}" }
                            }
                            {values(
                                option,
                                description_id,
                                settings_epoch,
                                connected && setting_option.is_none(),
                                on_set_config_option,
                            )}
                            if loading {
                                p { class: "hint", "data-slot": "setting-loading", role: "status",
                                    "Changing {option.name} with "
                                    code { "{v1::AGENT_METHOD_NAMES.session_set_config_option}" }
                                    "..."
                                }
                            }
                            // What the agent said about the last set, under the
                            // row it was about.
                            if let Some(refusal) = refusal.filter(|_| refused_row == Some(row)) {
                                {declined(refusal)}
                            }
                            } }
                        }
                    }
                }
            }

            // **Why every control on the surface is unavailable, said once.**
            // The settings outlive the agent that published them on purpose
            // (§5), so this rail goes on describing a session whose agent has
            // gone — and until this line existed the whole of what it said
            // about that was a row of controls nobody could press. The Agent
            // rail answers the same question with the same sentence in the same
            // place; only where there is a control it is about.
            if !connected && count > 0 {
                p {
                    class: "hint availability",
                    "data-slot": "availability",
                    role: "status",
                    "Setting actions are unavailable while disconnected."
                }
            }

            // The mode row that is not there, and what the agent did to make it
            // worth explaining rather than merely absent (§7.6).
            if let Some(mode) = settings.unoffered_mode.as_ref() {
                {unoffered(mode)}
            }

            // And the one that names no row of its own, beside the surface it
            // was refused at.
            if let Some(refusal) = refusal.filter(|_| refused_row.is_none()) {
                {declined(refusal)}
            }

            // Said on the surface rather than in a document nobody reading this
            // screen has open (§7.6).
            // **Not a notice.** It reports nothing an agent did — it says what
            // kind of view this is — and drawn in the chrome the agent's own
            // answers take it was a second identical card under the first, which
            // on an empty surface is two boxes at the top of an empty rail with
            // nothing saying which of them is the finding.
            p { class: "hint", "data-slot": "decoded",
                "Decoded view. Complete frames remain in Trace."
            }
        }
    }
}

/// How many settings the agent published, which is how many rows there are.
///
/// The modes are one setting and not one per mode: a set of values a single
/// call chooses between is one control, and the count on the header is a count
/// of controls.
fn rows(settings: &acp_inspector_core::SessionSettings) -> usize {
    usize::from(settings.modes.is_some())
        + settings
            .config_options
            .as_ref()
            .map_or(0, |options| options.len())
}

/// What the setting is set to, and the id the agent published it under.
///
/// **The method moved up to the name and the id came down here**, because the
/// two facts that were sharing this line could not both fit on it. A method is
/// twenty-five characters at the rail's width and `Current: verbose` is most of
/// what is left, so the method wrapped to a line of its own the moment a value
/// ran past six characters — the line it was put here to save, taken back by
/// the data. The name is the short half of the row and has room for it.
///
/// It still costs no line and it is still on the row it drives, which is what
/// §9 asks: which method a control drives is readable without going to the
/// trace. What it no longer is is *adjacent* to the control, and that is the
/// price of a layout that does not rearrange itself around the length of an
/// agent's value.
fn setting_current(current: Option<(String, bool)>, identifier: Option<String>) -> Element {
    rsx! {
        span { class: "setting-note", "data-slot": "current",
            if let Some((current, offered)) = current {
                "current: {current}"
                if !offered {
                    span { class: "detail-warn", " (not offered)" }
                }
            }
            if let Some(identifier) = identifier {
                span { class: "sr-only",
                    ". Config option id: "
                    if identifier.is_empty() { "(empty)" } else { "{identifier}" }
                }
            }
        }
    }
}

fn stated_current(kind: &v1::SessionConfigKind) -> Option<(String, bool)> {
    let current = current_value(kind)?;
    let offered = match kind {
        v1::SessionConfigKind::Select(select) => {
            select_offers(&select.options, &select.current_value)
        }
        v1::SessionConfigKind::Boolean(_) => true,
        _ => true,
    };

    Some((current, offered))
}

/// One mode in a short segmented group, as the button that would set it.
///
/// **Every mode is one, including the one the session is already in.** An
/// affordance is gated on the advertisement and on nothing else (§7.6): what an
/// agent does with a set to the mode it is already in is a thing to find out,
/// and a window that withheld the control would be answering it on the agent's
/// behalf. The same goes for a turn in flight, which nothing here knows about.
///
/// It says what a read-only value says, in the same words and the same markup
/// ([`stated`]); what it adds is the button around them.
///
/// **`connected` is not a gate on the agent's state, it is whether there is an
/// agent.** The settings outlive the connection that filled them (§5), so this
/// surface goes on describing a session whose agent has left — and a control
/// that cannot do what it says is worse than one that says it cannot, which is
/// the rule the agent panel's buttons already follow. It is not the whole of
/// saying so: an agent that leaves *between* the render and the click is
/// answered by core, which refuses the set with the news that it has gone
/// ([`ModeChange::Refused`]), and this surface reports that beside the row.
fn mode_segment(
    mode: &v1::SessionMode,
    current: bool,
    connected: bool,
    on_set_mode: EventHandler<v1::SessionModeId>,
) -> Element {
    let id = mode.id.to_string();
    let picked = mode.id.clone();

    rsx! {
        button {
            key: "{id}",
            class: "mode-row",
            r#type: "button",
            "data-current": "{current}",
            aria_pressed: current,
            aria_disabled: (!connected).then_some("true"),
            tabindex: (!connected).then_some("-1"),
            onclick: move |_| {
                if connected {
                    on_set_mode.call(picked.clone());
                }
            },
            span { class: "mode-id", "{id}" }
            span { class: "mode-name", "{mode.name}" }
            if current {
                span { class: "sr-only", " — current" }
            }
        }
    }
}

fn mode_values(
    modes: &v1::SessionModeState,
    _settings_epoch: u64,
    connected: bool,
    on_set_mode: EventHandler<v1::SessionModeId>,
) -> Element {
    rsx! {
        div {
            class: "setting-values modes",
            "data-slot": "values",
            role: "group",
            aria_label: "Set Mode",
            for mode in modes.available_modes.iter() {
                {mode_segment(mode, mode.id == modes.current_mode_id, connected, on_set_mode)}
            }
        }
    }
}

/// What became of the last `session/set_mode`, said beside the row it was about
/// (§7.6).
///
/// **Two sentences, because they are two different pieces of news.** A refusal
/// is the agent saying no, and reads as the failure it is. An acknowledgement
/// nobody restated is *permitted* silence — the specification obliges no agent
/// to announce a mode it just set — so it reads as a plain statement of what is
/// and is not known, which is the shape the previous ring used to tell a
/// resume-only user that an empty timeline was correct rather than a bug.
fn became_of(change: &ModeChange) -> Element {
    match change {
        ModeChange::Acknowledged(mode) => rsx! {
            p { class: "alert", "data-slot": "setting-result", role: "status",
                code { "{v1::AGENT_METHOD_NAMES.session_set_mode}" }
                " to "
                code { "{mode}" }
                " was acknowledged and not restated: the agent answered, and has said nothing about the session's mode since. The row shows the mode it last stated."
            }
        },
        ModeChange::Refused { mode, error } => rsx! {
            p { class: "alert alert-error", "data-slot": "setting-result", role: "alert",
                code { "{v1::AGENT_METHOD_NAMES.session_set_mode}" }
                " to "
                code { "{mode}" }
                " failed: {error}"
            }
        },
        // `ModeChange` is non-exhaustive. An outcome this window has not heard
        // of is still an outcome, and saying nothing about it would leave a
        // control that was pressed looking like one that was not.
        _ => rsx! {
            p { class: "alert", "data-slot": "setting-result", role: "status", "An outcome this window has no rendering for." }
        },
    }
}

/// A mode the agent stated for this session while it was opening, where the
/// answer that opened it published no modes at all (§7.6).
///
/// **Said because the absence has an explanation, and the explanation is about
/// the agent.** A session with no mode row is ordinarily the agent answering
/// that it published none, which the surface leaves to read as exactly that.
/// This is the one case where something else happened: `session/load` MUST
/// replay before it answers, so the agent announced a mode change and then
/// answered without offering the modes it changed between. The answer wins —
/// no advertisement, no affordance — and an agent author would want to know
/// they had done that.
///
/// **Beside the surface rather than under a row**, because the row it is about
/// is the one that is not there. It says what the agent did in the words the
/// wire used, and points at where the rest is: the announcement is on the
/// timeline, and the frames are in the trace (§8).
///
/// **It says what crossed and stops there.** A `session/load` MUST replay
/// before it answers, which is why this case exists and is worth naming — but
/// the same two frames in the same order can arrive under a `session/resume`,
/// which MUST NOT replay at all, and calling that a replay would be this window
/// reading an intention out of an ordering. What is claimed is what the frames
/// decide: the agent stated a mode, and then answered without offering any.
///
/// Not a Conformance Annotation and not written as one: no MUST obliges an
/// agent to publish the modes it announces, so this is a fact about the traffic
/// rather than a rule about it.
fn unoffered(mode: &v1::SessionModeId) -> Element {
    rsx! {
        p { class: "alert", "data-slot": "settings-notice", role: "status",
            "This agent stated a mode for this session before the answer that opened it — a "
            code { "current_mode_update" }
            " naming "
            code { "{mode}" }
            ", which is the order a "
            code { "session/load" }
            " replay arrives in — and then answered with no "
            code { "modes" }
            " at all. The answer is what advertises a mode control, so there is none; the announcement is on the timeline, where it happened."
        }
    }
}

/// What became of the last `session/set_config_option` (§7.6).
///
/// One sentence, where the mode row has two, because there is only one thing
/// left to say: a set that worked answered with the whole option set, so what
/// it did is the row itself. A set that did not is this.
///
/// **It says the set failed and stops there**, which is the whole of what this
/// window knows: the agent's refusal, an agent that had gone, or an answer core
/// could not read as v1 are three different sentences in the error itself, and
/// each of them is a set whose option set never arrived. What the agent did
/// with it — one of those three cannot say — is a question for the trace, which
/// has the frame.
fn declined(refusal: &ConfigRefusal) -> Element {
    rsx! {
        p { class: "alert alert-error", "data-slot": "setting-result", role: "alert",
            code { "{v1::AGENT_METHOD_NAMES.session_set_config_option}" }
            " setting "
            code { "{refusal.option}" }
            " to "
            {match asked_for(&refusal.value) {
                Some(value) => rsx! { code { "{value}" } },
                None => rsx! { span { class: "unnamed", "a value this window has no rendering for" } },
            }}
            " failed: {refusal.error}"
        }
    }
}

/// The value a set asked for, as the wire carried it — or `None` for a value
/// shape this window cannot name one out of.
///
/// **The ask, not the option's current kind.** What a refusal is about is the
/// value that went out, and the two shapes read differently on the wire: a
/// select's is a quoted value id, a boolean's is an unquoted literal. Saying
/// either as the other would be this window reporting a frame that never
/// crossed.
///
/// `SessionConfigOptionValue` is non-exhaustive, and a shape with no rendering
/// gets no guess: naming a value out of a shape this window just said it does
/// not know would be inventing one, which is [`current_value`]'s rule for the
/// same reason.
fn asked_for(value: &v1::SessionConfigOptionValue) -> Option<String> {
    match value {
        v1::SessionConfigOptionValue::ValueId { value } => Some(value.to_string()),
        v1::SessionConfigOptionValue::Boolean { value } => Some(value.to_string()),
        _ => None,
    }
}

/// What a config option could be set to, and — where this window can drive it —
/// the controls that would set it.
///
/// **Both settable kinds are driven, and each writes its own value shape.** A
/// select's write carries a bare value id and a boolean's carries the boolean
/// under a `type` discriminator; the discriminator describes the shape of a
/// *value* rather than the kind of an option, which is why the control that
/// knows the kind is the one that builds the value.
///
/// Select values retain their published names and ids in either a segmented
/// group or native select. A boolean is one native toggle whose checked state is
/// the Agent's stated current value.
fn values(
    option: &v1::SessionConfigOption,
    description_id: Option<String>,
    settings_epoch: u64,
    connected: bool,
    on_set_config_option: EventHandler<Choice>,
) -> Element {
    match &option.kind {
        v1::SessionConfigKind::Select(select) => choices(
            &option.id,
            &option.name,
            select,
            description_id,
            settings_epoch,
            connected,
            on_set_config_option,
        ),
        // The value as the wire has it, because that is what the option is set
        // to: a boolean rendered as a word of this window's choosing would be a
        // paraphrase of a literal.
        v1::SessionConfigKind::Boolean(boolean) => switchable(
            &option.id,
            &option.name,
            boolean.current_value,
            description_id,
            connected,
            on_set_config_option,
        ),
        // `SessionConfigKind` is non-exhaustive: an option shape this window has
        // no rendering for is still one the agent published, and a row with no
        // values is better than an option quietly missing from the list.
        //
        // **A forward-compatibility arm, and nothing reaches it today.** v1's
        // kind is internally tagged with exactly `select` and `boolean` and
        // every option list is deserialized with the item-skipping combinator,
        // so an option whose kind is a *third* string never arrives here as an
        // unknown kind — it vanishes from the list before core sees it, which
        // is driven in `crates/core/tests/session_settings.rs` and is exactly the
        // under-reporting the decoded-view sentence below names. The day the
        // schema grows a variant this window does not drive, the option is
        // still on the list and still read-only.
        _ => rsx! {
            p { class: "alert unnamed", "An option kind this window has no rendering for." }
        },
    }
}

/// A boolean option as the chip the drawing gives it: the value the agent
/// stated, and pressing it asks for the inverse.
///
/// **A chip and not a toggle.** What is drawn is the *agent's* stated value
/// (§7.6) — nothing is applied optimistically — and a switch that slid under
/// the pointer before the agent answered would be this window stating a
/// configuration its agent never claimed. `aria-pressed` carries the value to
/// anyone who cannot see the chip.
fn switchable(
    option: &v1::SessionConfigId,
    name: &str,
    current_value: bool,
    description_id: Option<String>,
    connected: bool,
    on_set_config_option: EventHandler<Choice>,
) -> Element {
    let option = option.clone();
    let accessible_name = config_control_name(name, &option);

    rsx! {
        div { class: "setting-values", "data-slot": "values",
            button {
                class: "chip",
                r#type: "button",
                "data-current": "{current_value}",
                aria_label: accessible_name,
                aria_describedby: description_id,
                aria_pressed: current_value,
                aria_disabled: (!connected).then_some("true"),
                tabindex: (!connected).then_some("-1"),
                onclick: move |_| {
                    if connected {
                        on_set_config_option.call(Choice {
                            option: option.clone(),
                            value: v1::SessionConfigOptionValue::boolean(!current_value),
                        });
                    }
                },
                "{current_value}"
            }
        }
    }
}

/// The values a select offers: the agent's own list, as the chips the drawing
/// draws them — or a native select where the agent *grouped* them, because a
/// group is a structure a row of chips cannot say.
fn choices(
    option: &v1::SessionConfigId,
    name: &str,
    select: &v1::SessionConfigSelect,
    description_id: Option<String>,
    settings_epoch: u64,
    connected: bool,
    on_set_config_option: EventHandler<Choice>,
) -> Element {
    if let v1::SessionConfigSelectOptions::Ungrouped(values) = &select.options {
        let accessible_name = config_control_name(name, option);
        return rsx! {
            div {
                class: "setting-values",
                "data-slot": "values",
                role: "group",
                aria_label: accessible_name,
                aria_describedby: description_id.clone(),
                for value in values.iter() {
                    {choice_segment(
                        option,
                        value,
                        value.value == select.current_value,
                        description_id.clone(),
                        connected,
                        on_set_config_option,
                    )}
                }
            }
        };
    }

    rsx! {
        ConfigSelect {
            key: "{settings_epoch}-{option}",
            option: option.clone(),
            name: name.to_owned(),
            select: select.clone(),
            description_id,
            connected,
            on_set_config_option,
        }
    }
}

/// The values a *grouped* select offers, as the platform's own list.
///
/// **The one shape a row of chips cannot say.** An agent that grouped its
/// values said something about them, and flattening the groups into one row
/// would be this window throwing that away; a native `<optgroup>` keeps it,
/// with the keyboard behaviour and the popup the platform already has.
/// Choosing prepares the ask and *Set* sends it, including the current value,
/// repeatedly (§7.6).
#[component]
fn ConfigSelect(
    option: v1::SessionConfigId,
    name: String,
    select: v1::SessionConfigSelect,
    description_id: Option<String>,
    connected: bool,
    on_set_config_option: EventHandler<Choice>,
) -> Element {
    let initial = if select_offers(&select.options, &select.current_value) {
        select.current_value.to_string()
    } else {
        first_select_value(&select.options).unwrap_or_default()
    };
    let mut selected = use_signal(|| initial.clone());
    let pending = selected();
    let shown = if select_offers_id(&select.options, &pending) {
        pending
    } else {
        initial
    };
    let has_values = first_select_value(&select.options).is_some();
    let available = connected && has_values;
    let value_name = format!("Value for {}", config_target_name(&name, &option));
    let apply_name = format!("Apply selected {}", config_target_name(&name, &option));
    let submit = shown.clone();
    let submitted_option = option.clone();

    rsx! {
        form {
            class: "setting-values select-action",
            "data-slot": "values",
            onsubmit: move |event| {
                event.prevent_default();
                if available {
                    on_set_config_option.call(Choice {
                        option: submitted_option.clone(),
                        value: v1::SessionConfigOptionValue::value_id(v1::SessionConfigValueId::new(submit.clone())),
                    });
                }
            },
            select {
                class: "select select-xs setting-select",
                aria_label: value_name,
                aria_describedby: description_id.clone(),
                value: "{shown}",
                aria_disabled: (!available).then_some("true"),
                tabindex: (!available).then_some("-1"),
                onchange: move |event| selected.set(event.value()),
                match &select.options {
                    v1::SessionConfigSelectOptions::Ungrouped(values) => rsx! {
                        for value in values.iter() {
                            option {
                                key: "{value.value}",
                                value: "{value.value}",
                                selected: value.value.to_string() == shown,
                                disabled: !available,
                                {labelled(&value.value.to_string(), &value.name)}
                            }
                        }
                    },
                    v1::SessionConfigSelectOptions::Grouped(groups) => rsx! {
                        for group in groups.iter() {
                            optgroup {
                                key: "{group.group}",
                                label: labelled(&group.group.to_string(), &group.name),
                                for value in group.options.iter() {
                                    option {
                                        key: "{value.value}",
                                        value: "{value.value}",
                                        selected: value.value.to_string() == shown,
                                        disabled: !available,
                                        {labelled(&value.value.to_string(), &value.name)}
                                    }
                                }
                            }
                        }
                    },
                    _ => rsx! {
                        option { disabled: true, "Values this window has no rendering for." }
                    },
                }
            }
            button {
                r#type: "submit",
                class: "btn btn-xs btn-quiet",
                aria_label: apply_name,
                aria_describedby: description_id,
                aria_disabled: (!available).then_some("true"),
                tabindex: (!available).then_some("-1"),
                "Set"
            }
        }
    }
}

fn select_offers(
    options: &v1::SessionConfigSelectOptions,
    current: &v1::SessionConfigValueId,
) -> bool {
    match options {
        v1::SessionConfigSelectOptions::Ungrouped(values) => {
            values.iter().any(|value| value.value == *current)
        }
        v1::SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|group| group.options.iter())
            .any(|value| value.value == *current),
        _ => false,
    }
}

fn select_offers_id(options: &v1::SessionConfigSelectOptions, id: &str) -> bool {
    match options {
        v1::SessionConfigSelectOptions::Ungrouped(values) => {
            values.iter().any(|value| value.value.to_string() == id)
        }
        v1::SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|group| group.options.iter())
            .any(|value| value.value.to_string() == id),
        _ => false,
    }
}

fn first_select_value(options: &v1::SessionConfigSelectOptions) -> Option<String> {
    match options {
        v1::SessionConfigSelectOptions::Ungrouped(values) => {
            values.first().map(|value| value.value.to_string())
        }
        v1::SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .find_map(|group| group.options.first())
            .map(|value| value.value.to_string()),
        _ => None,
    }
}

fn labelled(id: &str, name: &str) -> String {
    if id == name {
        name.to_owned()
    } else {
        format!("{name} ({id})")
    }
}

fn config_control_name(name: &str, id: &v1::SessionConfigId) -> String {
    format!("Set {}", config_target_name(name, id))
}

fn config_target_name(name: &str, id: &v1::SessionConfigId) -> String {
    let id = id.to_string();
    match (name.is_empty(), id.is_empty()) {
        (true, true) => "Config Option with empty name and id".to_owned(),
        (true, false) => format!("Config Option {id}"),
        (false, true) => format!("{name} (empty id)"),
        (false, false) => format!("{name} ({id})"),
    }
}

/// One value the agent published, as the chip that would set it.
///
/// [`mode_segment`]'s rules apply here too: every value is a control, including
/// the one the session is already set to, and `connected` says whether there is
/// anybody to ask rather than judging the agent's state.
fn choice_segment(
    option: &v1::SessionConfigId,
    value: &v1::SessionConfigSelectOption,
    current: bool,
    description_id: Option<String>,
    connected: bool,
    on_set_config_option: EventHandler<Choice>,
) -> Element {
    let id = value.value.to_string();
    let picked = Choice {
        option: option.clone(),
        value: v1::SessionConfigOptionValue::value_id(value.value.clone()),
    };

    rsx! {
        button {
            key: "{id}",
            class: "chip",
            r#type: "button",
            "data-current": "{current}",
            aria_pressed: current,
            aria_label: labelled(&id, &value.name),
            aria_describedby: description_id,
            aria_disabled: (!connected).then_some("true"),
            tabindex: (!connected).then_some("-1"),
            onclick: move |_| {
                if connected {
                    on_set_config_option.call(picked.clone());
                }
            },
            "{value.name}"
        }
    }
}

#[cfg(test)]
mod tests {
    use acp_inspector_core::{CallError, ConfigRefusal, ModeChange};

    use super::*;

    /// What a row is read by on screen — what it *is*, never how it is painted.
    ///
    /// One setting the agent published, whichever half of the surface it came
    /// from.
    const SETTING: &str = r#"data-slot="setting""#;
    /// And the one of them the agent says the session is at, which is a fact
    /// about the value rather than a class it is drawn in.
    const CURRENT: &str = r#"data-current="true""#;

    fn has_natively_disabled_control(html: &str) -> bool {
        html.split('<').any(|element| {
            ["button ", "select ", "input "]
                .iter()
                .any(|tag| element.starts_with(tag))
                && element
                    .split_once('>')
                    .is_some_and(|(attributes, _)| attributes.contains("disabled=true"))
        })
    }

    /// The two modes and the one select option a conformant agent publishes,
    /// which is what the upstream test agent sends.
    fn modes() -> v1::SessionModeState {
        v1::SessionModeState::new(
            "chat",
            vec![
                v1::SessionMode::new("chat", "Chat"),
                v1::SessionMode::new("plan", "Plan"),
            ],
        )
    }

    fn verbosity() -> v1::SessionConfigOption {
        v1::SessionConfigOption::select(
            "verbosity",
            "Verbosity",
            "normal",
            vec![
                v1::SessionConfigSelectOption::new("brief", "Brief"),
                v1::SessionConfigSelectOption::new("normal", "Normal"),
                v1::SessionConfigSelectOption::new("verbose", "Verbose"),
            ],
        )
    }

    fn detailed_output() -> v1::SessionConfigOption {
        v1::SessionConfigOption::select(
            "output",
            "Output",
            "normal",
            vec![
                v1::SessionConfigSelectOption::new("brief", "Brief"),
                v1::SessionConfigSelectOption::new("normal", "Normal"),
                v1::SessionConfigSelectOption::new("verbose", "Verbose"),
                v1::SessionConfigSelectOption::new("exhaustive", "Exhaustive"),
            ],
        )
    }

    fn grouped_output() -> v1::SessionConfigOption {
        v1::SessionConfigOption::select(
            "output",
            "Output",
            "normal",
            vec![
                v1::SessionConfigSelectGroup::new(
                    "everyday",
                    "Everyday",
                    vec![
                        v1::SessionConfigSelectOption::new("brief", "Brief"),
                        v1::SessionConfigSelectOption::new("normal", "Normal"),
                    ],
                ),
                v1::SessionConfigSelectGroup::new(
                    "diagnostic",
                    "Diagnostic",
                    vec![v1::SessionConfigSelectOption::new("verbose", "Verbose")],
                ),
            ],
        )
    }

    fn long_output() -> v1::SessionConfigOption {
        v1::SessionConfigOption::select(
            "output",
            "Output",
            "brief",
            vec![
                v1::SessionConfigSelectOption::new(
                    "brief",
                    "Brief, with only the essential result",
                ),
                v1::SessionConfigSelectOption::new(
                    "detailed",
                    "Detailed, with every diagnostic included",
                ),
            ],
        )
    }

    /// A config option whose id is literally `mode`, which is what a real agent
    /// publishes beside its modes.
    fn mode_option() -> v1::SessionConfigOption {
        v1::SessionConfigOption::select(
            "mode",
            "Mode",
            "chat",
            vec![
                v1::SessionConfigSelectOption::new("chat", "Chat"),
                v1::SessionConfigSelectOption::new("plan", "Plan"),
            ],
        )
    }

    fn published(
        modes: Option<v1::SessionModeState>,
        config_options: Option<Vec<v1::SessionConfigOption>>,
    ) -> acp_inspector_core::SessionSettings {
        let mut settings = acp_inspector_core::SessionSettings::default();
        settings.modes = modes;
        settings.config_options = config_options;
        settings
    }

    /// The surface itself, rendered — the screen and not a value on the way to
    /// it. With an agent on the other end, which is the ordinary case; the one
    /// that has gone is [`shown_without_an_agent`].
    fn shown(settings: acp_inspector_core::SessionSettings) -> String {
        rendered(settings, true)
    }

    /// The same surface, for a session whose agent has left.
    fn shown_without_an_agent(settings: acp_inspector_core::SessionSettings) -> String {
        rendered(settings, false)
    }

    #[test]
    fn a_surface_whose_agent_has_gone_says_why_its_controls_do_nothing() {
        // The settings outlive the agent that published them on purpose (§5),
        // so this rail goes on describing a session whose agent has left — and
        // the whole of what it said about that was a row of controls nobody
        // could press. The Agent rail answers the same question with the same
        // sentence, and this is that sentence in the same place.
        let gone = shown_without_an_agent(published(Some(modes()), Some(vec![verbosity()])));
        assert!(
            gone.contains(r#"data-slot="availability""#)
                && gone.contains("Setting actions are unavailable while disconnected."),
            "{gone}"
        );

        let live = shown(published(Some(modes()), Some(vec![verbosity()])));
        assert!(
            !live.contains(r#"data-slot="availability""#),
            "a live agent's controls need no excuse: {live}"
        );

        // And it is drawn only where there is a control it is about: a session
        // whose agent published nothing has nothing on this rail that a
        // connection would make pressable, and the sentence would be describing
        // an absence that is the agent's rather than the connection's.
        let empty = shown_without_an_agent(acp_inspector_core::SessionSettings::default());
        assert!(!empty.contains(r#"data-slot="availability""#), "{empty}");
    }

    #[test]
    fn session_settings_are_the_rails_own_groups_rather_than_cards_on_it() {
        let populated = shown(published(Some(modes()), Some(vec![mode_option()])));

        for component in [
            r#"class="rail-group settings""#,
            r#"class="rail-head""#,
            r#"class="eyebrow">Mode<"#,
            r#"class="eyebrow">Settings<"#,
            r#"class="setting-note""#,
            r#"class="mode-row""#,
            r#"class="chip""#,
        ] {
            assert!(
                populated.contains(component),
                "the surface is drawn with {component}: {populated}"
            );
        }
        assert!(
            !populated.contains("card"),
            "the grouped rows are the rail's own regions rather than nested cards: {populated}"
        );
        assert_eq!(
            populated.matches(SETTING).count(),
            2,
            "a Mode and a Config Option describing modes remain separate controls: {populated}"
        );
        assert!(
            populated.contains(CURRENT),
            "and what each of them is set to is on the value that is it: {populated}"
        );

        let absent = shown(acp_inspector_core::SessionSettings::default());
        assert!(
            absent.contains(r#"class="alert" data-slot="settings-notice""#)
                && !absent.contains("alert-warning")
                && !absent.contains("alert-error"),
            "absence is a neutral daisyUI notice: {absent}"
        );

        let refused = shown(refused(v1::SessionConfigId::new("verbosity"), "shouting"));
        assert!(
            refused.contains(r#"class="alert alert-error" data-slot="setting-result""#)
                && refused.contains("unsupported verbosity value"),
            "the Agent's refusal is a daisyUI failure notice: {refused}"
        );
    }

    fn rendered(settings: acp_inspector_core::SessionSettings, connected: bool) -> String {
        rendered_with_work(settings, connected, false, None)
    }

    fn rendered_with_work(
        settings: acp_inspector_core::SessionSettings,
        connected: bool,
        changing_mode: bool,
        setting_option: Option<v1::SessionConfigId>,
    ) -> String {
        #[component]
        fn Host(
            settings: acp_inspector_core::SessionSettings,
            connected: bool,
            changing_mode: bool,
            setting_option: Option<v1::SessionConfigId>,
        ) -> Element {
            rsx! {
                SessionSettings {
                    settings,
                    settings_epoch: 0,
                    connected,
                    changing_mode,
                    setting_option,
                    on_set_mode: move |_: v1::SessionModeId| {},
                    on_set_config_option: move |_: Choice| {},
                }
            }
        }

        let mut dom = VirtualDom::new_with_props(
            Host,
            HostProps {
                settings,
                connected,
                changing_mode,
                setting_option,
            },
        );
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    /// What the agent last said about the mode, and what became of the last set
    /// — the two halves of a mode row that has been driven.
    fn asked(change: ModeChange) -> acp_inspector_core::SessionSettings {
        let mut settings = published(Some(modes()), None);
        settings.mode_change = Some(change);
        settings
    }

    /// The one option the agent published, and a set it refused — of that
    /// option or of one it never published at all.
    fn refused(option: v1::SessionConfigId, value: &str) -> acp_inspector_core::SessionSettings {
        let mut settings = published(None, Some(vec![verbosity()]));
        settings.config_refusal = Some(ConfigRefusal {
            option,
            value: v1::SessionConfigOptionValue::value_id(v1::SessionConfigValueId::new(value)),
            error: CallError::Rejected(v1::Error::new(
                i32::from(v1::ErrorCode::InvalidParams),
                "unsupported verbosity value `shouting`",
            )),
        });
        settings
    }

    #[test]
    fn every_mode_the_agent_published_is_a_full_width_row_that_would_set_it() {
        // The drawing gives the modes the rail's whole width, one row each, the
        // id the set carries at the near edge and the name the agent gave it at
        // the far one. However many there are: a mode is what a reader sets
        // immediately before prompting, and a list of three that hid itself
        // behind a popup would cost that reader the press this rail exists for.
        let html = shown(published(Some(modes()), None));

        assert_eq!(
            html.matches("<button").count(),
            2,
            "one control per mode the agent published: {html}"
        );
        assert!(
            html.contains(r#"role="group""#)
                && html.contains(r#"aria-label="Set Mode""#)
                && html.contains(r#"aria-pressed=true"#),
            "the modes form one keyboard-accessible group: {html}"
        );
        for mode in ["chat", "plan"] {
            let named = format!(r#"class="mode-id">{mode}<"#);
            assert!(
                html.contains(&named),
                "the id is on the row — it is what the set sends, and what a reader checks this \
                 screen against the trace by: {html}"
            );
        }
        assert!(
            html.contains(r#"class="mode-name">Chat<"#)
                && html.contains(r#"class="mode-name">Plan<"#),
            "and the agent's own name for it is beside the id: {html}"
        );

        let many = shown(published(
            Some(v1::SessionModeState::new(
                "chat",
                vec![
                    v1::SessionMode::new("chat", "Chat"),
                    v1::SessionMode::new("plan", "Plan"),
                    v1::SessionMode::new("review", "Review"),
                ],
            )),
            None,
        ));
        assert_eq!(
            many.matches(r#"class="mode-row""#).count(),
            3,
            "a third mode is a third row and never a popup: {many}"
        );
    }

    #[test]
    fn the_values_of_an_ungrouped_select_are_one_chip_each() {
        let html = shown(published(None, Some(vec![verbosity()])));

        assert_eq!(
            html.matches(r#"class="chip""#).count(),
            3,
            "one chip per value the agent published: {html}"
        );
        assert!(
            html.contains(r#"role="group""#)
                && html.contains(r#"aria-label="Set Verbosity (verbosity)""#),
            "the chips are one named group rather than three unrelated buttons: {html}"
        );
        for value in ["Brief", "Normal", "Verbose"] {
            assert!(html.contains(value), "the value {value} is drawn: {html}");
        }
        let current = html
            .split("<button")
            .find(|chip| chip.contains(CURRENT))
            .expect("the value the agent says the session is at");
        assert!(
            current.contains(r#"aria-pressed=true"#) && current.contains(">Normal<"),
            "and the one it is at is pressed: {html}"
        );
        assert!(
            !html.contains("<select"),
            "an ungrouped list has no structure a popup would keep: {html}"
        );
    }

    #[test]
    fn a_boolean_option_is_one_accessibly_named_chip_showing_the_stated_value() {
        // A chip and not a switch: nothing is applied optimistically (§7.6), and
        // a toggle that slid under the pointer before the agent answered would
        // be this window stating a configuration its agent never claimed.
        let html = shown(published(
            None,
            Some(vec![v1::SessionConfigOption::boolean("fast", "Fast", true)]),
        ));

        assert_eq!(
            html.matches(r#"class="chip""#).count(),
            1,
            "a boolean is one control, because it has one other value: {html}"
        );
        assert!(
            !html.contains(r#"type="checkbox""#) && !html.contains("toggle"),
            "and it is not a switch: {html}"
        );
        assert!(
            html.contains(r#"aria-label="Set Fast (fast)""#)
                && html.contains(r#"aria-pressed=true"#)
                && html.contains(CURRENT),
            "the chip has a complete accessible name and carries the value: {html}"
        );
        assert!(
            html.contains(">true</button>"),
            "the Agent's current boolean remains stated as the wire has it: {html}"
        );
    }

    #[test]
    fn only_a_grouped_select_becomes_the_platforms_own_list() {
        // A group is the one shape a row of chips cannot say, so it is the one
        // shape that goes to a native select: an agent that grouped its values
        // said something about them, and flattening the groups would be this
        // window throwing that away.
        let numerous = shown(published(None, Some(vec![detailed_output()])));
        assert_eq!(
            numerous.matches(r#"class="chip""#).count(),
            4,
            "four ungrouped values are four chips: {numerous}"
        );
        assert!(
            !numerous.contains("<select") && numerous.contains("current: normal"),
            "the current selection is stated and can be sent again: {numerous}"
        );

        let grouped = shown(published(None, Some(vec![grouped_output()])));
        assert!(
            grouped.contains(r#"class="select select-xs setting-select""#)
                && grouped.contains(r#"aria-label="Value for Output (output)""#)
                && grouped.contains(r#"aria-label="Apply selected Output (output)""#)
                && grouped.contains(r#">Set</button>"#),
            "a grouped select keeps the platform's popup and an explicit Set: {grouped}"
        );
        assert!(
            grouped.matches("<optgroup").count() == 2
                && grouped.contains(r#"label="Everyday (everyday)""#)
                && grouped.contains(r#"label="Diagnostic (diagnostic)""#),
            "Agent-provided value groups and identifiers remain explicit: {grouped}"
        );

        let long = shown(published(None, Some(vec![long_output()])));
        assert!(
            long.contains("Brief, with only the essential result") && !long.contains("<select"),
            "a long value is a wide chip and not a reason to hide the list: {long}"
        );

        let unknown = shown(published(
            None,
            Some(vec![v1::SessionConfigOption::select(
                "output",
                "Output",
                "future-value",
                vec![
                    v1::SessionConfigSelectOption::new("brief", "Brief"),
                    v1::SessionConfigSelectOption::new("normal", "Normal"),
                    v1::SessionConfigSelectOption::new("verbose", "Verbose"),
                    v1::SessionConfigSelectOption::new("exhaustive", "Exhaustive"),
                ],
            )]),
        ));
        assert!(
            unknown.contains("current: future-value")
                && unknown.contains("not offered")
                && !unknown.contains(CURRENT),
            "an unknown current value is stated as an Agent-provided mismatch, and none of the \
             offered values is marked as it: {unknown}"
        );

        let empty_id = shown(published(
            None,
            Some(vec![v1::SessionConfigOption::select(
                "output",
                "Output",
                "",
                vec![
                    v1::SessionConfigSelectOption::new("", "Agent default"),
                    v1::SessionConfigSelectOption::new("brief", "Brief"),
                    v1::SessionConfigSelectOption::new("normal", "Normal"),
                    v1::SessionConfigSelectOption::new("verbose", "Verbose"),
                ],
            )]),
        ));
        assert!(
            empty_id.contains(">Agent default</button>")
                && empty_id.contains(CURRENT)
                && !empty_id.contains("disabled=true"),
            "an empty Agent-provided id remains a settable value: {empty_id}"
        );
    }

    #[test]
    fn setting_calls_are_explicitly_loading_and_descriptions_name_their_controls() {
        let described = verbosity().description("How much detail the Agent should return.");
        let settings = published(
            Some(modes()),
            Some(vec![
                described,
                v1::SessionConfigOption::boolean("fast", "Fast", true),
            ]),
        );
        let html = rendered_with_work(settings, true, true, Some(v1::SessionConfigId::new("fast")));

        assert!(
            html.contains("Changing Mode with")
                && html.contains("Changing Fast with")
                && html.matches(r#"aria-busy=true"#).count() == 2,
            "each pending setter is stated on the row it belongs to: {html}"
        );
        assert!(
            html.contains(r#"id="config-description-0""#)
                && html.contains(r#"aria-describedby="config-description-0""#)
                && html.contains("How much detail the Agent should return."),
            "Agent-provided descriptions are associated with their controls: {html}"
        );
        assert_eq!(
            html.matches(r#"aria-disabled="true""#).count(),
            6,
            "both in-flight setter families are unavailable until their answers arrive: {html}"
        );
        assert_eq!(html.matches(r#"tabindex="-1""#).count(), 6, "{html}");
        assert!(
            !has_natively_disabled_control(&html),
            "pending controls retain focusable nodes: {html}"
        );
    }

    #[test]
    fn a_boolean_option_control_is_dead_when_the_agent_that_published_it_has_gone() {
        let html = shown_without_an_agent(published(
            None,
            Some(vec![v1::SessionConfigOption::boolean("fast", "Fast", true)]),
        ));

        assert_eq!(
            html.matches(r#"aria-disabled="true""#).count(),
            1,
            "the chip says it cannot be changed: {html}"
        );
        assert!(!has_natively_disabled_control(&html), "{html}");
    }

    #[test]
    fn a_refused_boolean_set_says_the_value_it_asked_for_as_the_wire_carried_it() {
        // A rejected boolean set reads as a rejected select set does (§7.6),
        // and what it names is the ask: a boolean, unquoted, rather than a
        // value id the write never carried.
        let mut settings = published(
            None,
            Some(vec![v1::SessionConfigOption::boolean("fast", "Fast", true)]),
        );
        settings.config_refusal = Some(ConfigRefusal {
            option: v1::SessionConfigId::new("fast"),
            value: v1::SessionConfigOptionValue::boolean(false),
            error: CallError::Rejected(v1::Error::new(
                i32::from(v1::ErrorCode::InvalidParams),
                "`fast` cannot be turned off here",
            )),
        });
        let html = shown(settings);

        assert!(
            html.contains("cannot be turned off"),
            "what the agent said: {html}"
        );
        assert!(
            html.contains("<code>false</code>"),
            "about the value that was asked for: {html}"
        );
        let after_the_current_mark = html
            .split_once(CURRENT)
            .expect("one value is still the current one")
            .1;
        assert!(
            after_the_current_mark.contains("true"),
            "and nothing was applied, so nothing was rolled back: {html}"
        );
    }

    #[test]
    fn the_value_controls_are_dead_when_the_agent_that_published_them_has_gone() {
        let html = shown_without_an_agent(published(None, Some(vec![verbosity()])));

        assert_eq!(
            html.matches(r#"aria-disabled="true""#).count(),
            3,
            "every chip says it cannot be pressed: {html}"
        );
        assert!(!has_natively_disabled_control(&html), "{html}");
    }

    #[test]
    fn a_refused_config_option_says_what_the_agent_said_beside_the_row() {
        // A failed setter costs the attempt and nothing else (§7.6): the values
        // are where the agent's last statement left them, and the refusal is
        // beside the surface rather than only in the trace.
        let html = shown(refused(v1::SessionConfigId::new("verbosity"), "shouting"));

        assert!(
            html.contains("unsupported verbosity value"),
            "what the agent said: {html}"
        );
        assert!(
            html.contains("shouting"),
            "about the value that was asked for: {html}"
        );
        assert!(
            html.contains("current: normal") && html.contains(CURRENT),
            "and nothing was applied, so nothing was rolled back: {html}"
        );
    }

    #[test]
    fn a_refusal_about_an_option_the_agent_no_longer_publishes_is_still_said() {
        // The answer to a set is the *complete* option set, so an agent may
        // stop publishing the very option that was refused. A refusal drawn
        // only under a row it matched would vanish with it, which would be the
        // window keeping the secret this surface exists to tell.
        let html = shown(refused(v1::SessionConfigId::new("colour"), "green"));

        assert!(
            html.contains("unsupported verbosity value") && html.contains("colour"),
            "said beside the surface instead: {html}"
        );
    }

    #[test]
    fn the_controls_are_dead_when_the_agent_that_published_them_has_gone() {
        // The settings outlive the connection that filled them, so the surface
        // keeps describing the session — but a control that cannot do what it
        // says is worse than one that says it cannot, which is the rule the
        // agent panel's buttons already follow.
        let html = shown_without_an_agent(published(Some(modes()), None));

        assert_eq!(
            html.matches("<button").count(),
            2,
            "the controls are still drawn, because the session is still described: {html}"
        );
        assert_eq!(
            html.matches(r#"aria-disabled="true""#).count(),
            2,
            "and both of them say they cannot: {html}"
        );
        assert!(!has_natively_disabled_control(&html), "{html}");
        assert!(
            !shown(published(Some(modes()), None)).contains("disabled"),
            "which is the connection's answer and not a control that is always dead"
        );
    }

    #[test]
    fn a_set_the_agent_never_restated_reads_as_acknowledged_and_unconfirmed() {
        // The middle case of the ring (§7.6). The row keeps showing the mode
        // the *agent* stated, and the sentence beside it says what happened to
        // the ask — because showing the requested mode would be the inspector
        // stating a configuration its agent never claimed, and showing nothing
        // would read as a broken control.
        let html = shown(asked(ModeChange::Acknowledged(v1::SessionModeId::new(
            "plan",
        ))));

        assert!(
            html.contains("acknowledged") && html.contains("not restated"),
            "the set is reported as what it was: {html}"
        );
        let current = html
            .split("<button")
            .find(|row| row.contains(CURRENT))
            .expect("the row is drawn");
        assert!(
            current.contains("chat"),
            "and the row is still at the mode the agent stated: {html}"
        );
    }

    #[test]
    fn a_refused_set_says_what_the_agent_said_beside_the_row() {
        // A failed setter costs the attempt and nothing else (§7.6): the row is
        // where the agent's last statement left it, and the refusal is beside
        // the surface rather than only in the trace.
        let html = shown(asked(ModeChange::Refused {
            mode: v1::SessionModeId::new("architect"),
            error: CallError::Rejected(v1::Error::new(
                i32::from(v1::ErrorCode::InvalidParams),
                "unsupported mode `architect`",
            )),
        }));

        assert!(
            html.contains("unsupported mode"),
            "what the agent said: {html}"
        );
        assert!(
            html.contains("architect"),
            "about the mode that was asked for: {html}"
        );
        let current = html
            .split("<button")
            .find(|row| row.contains(CURRENT))
            .expect("the row is drawn");
        assert!(
            current.contains("chat"),
            "and nothing was applied, so nothing was rolled back: {html}"
        );
    }

    #[test]
    fn the_modes_the_agent_published_are_shown_with_the_one_it_is_in() {
        // What the ring is for: a user could watch their agent announce a mode
        // change and could not ask what mode the session was in, or what modes
        // there were (§7.6).
        let html = shown(published(Some(modes()), None));

        assert!(html.contains("Chat") && html.contains("Plan"), "{html}");
        assert!(html.contains(CURRENT), "and which one it is in: {html}");

        let above_the_mode_it_is_in = html
            .split_once("chat")
            .expect("the mode it is in is drawn")
            .0;
        assert!(
            above_the_mode_it_is_in.contains("Mode"),
            "under the setting's name: {html}"
        );
    }

    #[test]
    fn a_mode_row_names_the_method_that_would_change_it() {
        let html = shown(published(Some(modes()), None));

        assert!(html.contains("session/set_mode"), "{html}");
    }

    #[test]
    fn a_config_option_shows_its_current_value_and_the_method_that_changes_it() {
        let html = shown(published(None, Some(vec![verbosity()])));

        assert!(
            html.contains("Verbosity"),
            "the option it published: {html}"
        );
        assert!(
            html.contains("session/set_config_option"),
            "and the method that changes it: {html}"
        );

        assert!(
            html.contains("current: normal") && html.contains(CURRENT),
            "and its stated current value is on the row and on the value that is it: {html}"
        );
    }

    #[test]
    fn a_mode_the_agent_never_offered_is_still_the_mode_it_says_it_is_in() {
        // The two halves can disagree, and the surface reports the
        // disagreement rather than papering over it: an announcement names a
        // mode, and nothing obliges an agent to have offered the one it named.
        // A row that only marked the matching value would show a session with
        // no mode set at all.
        let modes = v1::SessionModeState::new(
            "architect",
            vec![
                v1::SessionMode::new("chat", "Chat"),
                v1::SessionMode::new("plan", "Plan"),
            ],
        );
        let html = shown(published(Some(modes), None));

        assert!(
            html.contains("architect"),
            "the mode it says it is in is on the row: {html}"
        );
        assert!(
            !html.contains(CURRENT),
            "and none of the values it offered is marked, because none of them is it: {html}"
        );
    }

    #[test]
    fn a_setting_published_as_both_a_mode_and_a_config_option_is_two_rows() {
        // A duplicated control is a fact about that agent and not a mess to tidy
        // away (§7.6): an agent author who published the same setting twice
        // finds out by seeing it twice.
        let html = shown(published(Some(modes()), Some(vec![mode_option()])));

        assert_eq!(
            html.matches(SETTING).count(),
            2,
            "two rows, one per setting: {html}"
        );
        assert!(
            html.contains("session/set_mode") && html.contains("session/set_config_option"),
            "each naming the method that would change it: {html}"
        );
    }

    #[test]
    fn an_agent_that_published_no_modes_gets_no_mode_control() {
        // Gated on the advertisement and on nothing else, so the absence reads
        // as the agent's answer rather than as an empty widget (§7.6).
        let html = shown(published(None, Some(vec![verbosity()])));

        assert!(
            !html.contains("session/set_mode"),
            "no mode row at all: {html}"
        );
        assert_eq!(html.matches(SETTING).count(), 1, "{html}");
    }

    #[test]
    fn an_agent_that_published_no_config_options_gets_no_config_option_rows() {
        let html = shown(published(Some(modes()), None));

        assert!(
            !html.contains("session/set_config_option"),
            "no config option rows at all: {html}"
        );
        assert_eq!(html.matches(SETTING).count(), 1, "{html}");
    }

    #[test]
    fn an_agent_that_published_nothing_says_so_rather_than_drawing_an_empty_control() {
        let html = shown(acp_inspector_core::SessionSettings::default());

        assert!(
            html.contains("published no modes and no config options"),
            "{html}"
        );
        assert!(!html.contains(SETTING), "{html}");
    }

    #[test]
    fn an_empty_option_list_reads_as_the_list_it_is_rather_than_as_an_empty_control() {
        // A list with nothing in it is a different answer from no list at all,
        // and both of them are the agent's (§7.6). Neither draws a control, and
        // neither is left to be inferred from a surface with nothing on it.
        let html = shown(published(None, Some(vec![])));

        assert!(html.contains("empty"), "{html}");
        assert!(
            !html.contains(SETTING),
            "and no control to go with it: {html}"
        );
        assert!(
            !html.contains("published no modes and no config options"),
            "which is not the same as having published nothing: {html}"
        );
    }

    #[test]
    fn a_mode_the_answer_did_not_offer_is_no_control_and_is_said_on_the_surface() {
        // The one case the ring's two rules collide in (§7.6). `session/load`
        // MUST replay before it answers, so a mode the agent stated crosses
        // ahead of the answer that says whether it offered any — and the answer
        // wins, absolutely: no advertisement, no affordance. What is left to do
        // is say what the agent did, because inferring a control from replayed
        // traffic would be this window deciding on the agent's behalf what it
        // supports, and an agent author would want to know they had published
        // one and not the other.
        let mut settings = published(None, Some(vec![verbosity()]));
        settings.unoffered_mode = Some(v1::SessionModeId::new("plan"));
        let html = shown(settings);

        assert!(
            !html.contains("session/set_mode"),
            "no mode control at all: {html}"
        );
        assert!(
            html.contains("current_mode_update") && html.contains("plan"),
            "and the surface names what the agent did: {html}"
        );
        assert!(
            html.contains("<code>modes</code>"),
            "including the field the answer left out, which is the advertisement: {html}"
        );
        assert!(
            html.contains(SETTING),
            "the half it did publish is a row like any other: {html}"
        );
    }

    #[test]
    fn a_session_that_published_nothing_but_replayed_a_mode_says_both() {
        // Two absences and one of them has an explanation: the agent published
        // nothing, which is its answer and says so — and it stated a mode
        // anyway, which is the asymmetry worth seeing.
        let mut settings = acp_inspector_core::SessionSettings::default();
        settings.unoffered_mode = Some(v1::SessionModeId::new("plan"));
        let html = shown(settings);

        assert!(
            html.contains("published no modes and no config options"),
            "what the agent published: {html}"
        );
        assert!(
            html.contains("current_mode_update"),
            "and what it said anyway: {html}"
        );
        assert!(!html.contains(SETTING), "and no control: {html}");
    }

    #[test]
    fn the_surface_says_it_is_a_decoded_view_and_where_the_whole_record_is() {
        // The limit is named rather than implied (§7.6): the schema skips
        // invalid entries in an option list, so a surface that claimed
        // completeness would be claiming something the typed layer cannot
        // deliver.
        let html = shown(published(Some(modes()), Some(vec![verbosity()])));

        assert!(html.contains("Decoded view"), "{html}");
        assert!(
            html.contains("Complete frames remain in Trace"),
            "and where the complete record is: {html}"
        );
    }
}
