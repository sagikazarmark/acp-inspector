//! The Session Settings surface: what the live session is configured as, and
//! what it could be. See `docs/architecture.md` §7.6 and §9.
//!
//! - Modes and Config Options are data on the session setup response, not
//!   capabilities, so they sit in the rail's Session tab. Each row names the
//!   method that would change it; a setting published both ways appears twice.
//! - Gated on the data's presence only: an agent that published no modes gets
//!   no mode row, and the absence is said as its answer.
//! - Nothing is applied optimistically. A mode row shows the mode the agent
//!   last stated, with what became of a set beside it ([`ModeChange`]); config
//!   option rows show the replacement set the agent answered with, and a
//!   refusal ([`ConfigRefusal`]) is said under its row or beside the surface.
//! - A select writes a bare value id and a boolean writes a `type`-tagged
//!   boolean, so the control that knows the kind builds the value.
//! - It is a decoded view: the schema skips invalid option entries, so the
//!   surface says so; the frame is complete in the trace (§8).

use acp_inspector_core::{ConfigRefusal, ModeChange, v1};
use dioxus::prelude::*;

use crate::update::current_value;

/// A value the user picked for a config option, and the option it belongs to.
/// The value's shape (select id or boolean) is decided where the control is
/// drawn.
#[derive(Clone, PartialEq)]
pub struct Choice {
    pub option: v1::SessionConfigId,
    pub value: v1::SessionConfigOptionValue,
}

/// The live Session's modes and Config Options, in the right details rail.
#[component]
pub fn SessionSettings(
    /// Core's settings for the session, including what became of the last
    /// mode set.
    settings: acp_inspector_core::SessionSettings,
    /// Changes whenever a Session or Connection replaces the one these
    /// controls belong to, so unsubmitted native-select choices do not leak.
    settings_epoch: u64,
    /// Whether the agent is still there. The settings outlive it (§5), but a
    /// set needs somebody to answer, so the controls say they are unavailable.
    connected: bool,
    /// Whether a `session/set_mode` call is waiting for its answer.
    changing_mode: bool,
    /// The Config Option whose `session/set_config_option` call is waiting. The
    /// setter has one pending slot, so every Config Option control yields until
    /// that answer arrives while the named row says what is loading.
    setting_option: Option<v1::SessionConfigId>,
    /// A mode the user picked, on its way to `session/set_mode`.
    on_set_mode: EventHandler<v1::SessionModeId>,
    /// A value the user picked for a config option, on its way to
    /// `session/set_config_option`.
    on_set_config_option: EventHandler<Choice>,
) -> Element {
    let modes = settings.modes.as_ref();
    let options = settings.config_options.as_deref().unwrap_or_default();
    let count = rows(&settings);
    // A refusal goes under the row it names, or beside the surface if the
    // agent's replacement set dropped that option, so it never vanishes.
    let refusal = settings.config_refusal.as_ref();
    let refused_row = refusal.and_then(|refusal| {
        options
            .iter()
            .position(|option| option.id == refusal.option)
    });

    rsx! {
        section { class: "rail-group settings", "data-slot": "session-settings",
            if count == 0 {
                // The absence is said as the agent's answer (§7.6); an empty
                // list and no list are different answers, so different sentences.
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

            // Every mode is a control, including the current one: what an agent
            // does with a set to its current mode is worth finding out (§7.6).
            if let Some(modes) = modes {
                div {
                    class: "rail-group",
                    "data-slot": "setting",
                    aria_busy: changing_mode,
                    div { class: "rail-head",
                        span { class: "eyebrow", "Mode" }
                        span { class: "setting-note", "{v1::AGENT_METHOD_NAMES.session_set_mode}" }
                    }
                    // Said in words only where no offered row matches: an agent
                    // may state a mode it never offered.
                    if !modes.available_modes.iter().any(|mode| mode.id == modes.current_mode_id) {
                        {setting_current(Some((modes.current_mode_id.to_string(), false)), None)}
                    }
                    {mode_values(modes, settings_epoch, connected && !changing_mode, on_set_mode)}
                    if changing_mode {
                        p { class: "hint", "data-slot": "setting-loading", role: "status",
                            "Changing mode with "
                            code { "{v1::AGENT_METHOD_NAMES.session_set_mode}" }
                            "..."
                        }
                    }
                    if let Some(change) = settings.mode_change.as_ref() {
                        {became_of(change)}
                    }
                }
            }

            // One row per config option, whatever its id, even `mode`.
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
                            if let Some(refusal) = refusal.filter(|_| refused_row == Some(row)) {
                                {declined(refusal)}
                            }
                            } }
                        }
                    }
                }
            }

            // Why every control is unavailable, said once, and only where there
            // is a control. The settings outlive the agent (§5).
            if !connected && count > 0 {
                p {
                    class: "hint availability",
                    "data-slot": "availability",
                    role: "status",
                    "Setting actions are unavailable while disconnected."
                }
            }

            if let Some(mode) = settings.unoffered_mode.as_ref() {
                {unoffered(mode)}
            }

            // A refusal whose option is no longer published.
            if let Some(refusal) = refusal.filter(|_| refused_row.is_none()) {
                {declined(refusal)}
            }

            // A hint, not a notice: it says what kind of view this is rather
            // than reporting anything the agent did (§7.6).
            if count > 0 {
                p { class: "hint", "data-slot": "decoded",
                    "Changes are sent to the agent immediately."
                }
            }
        }
    }
}

/// How many settings the agent published. The modes count as one setting.
fn rows(settings: &acp_inspector_core::SessionSettings) -> usize {
    usize::from(settings.modes.is_some())
        + settings
            .config_options
            .as_ref()
            .map_or(0, |options| options.len())
}

/// What the setting is set to, and the id the agent published it under. The
/// method the row drives is on the name line, where it fits at rail width.
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

/// One mode as the button that would set it, around the same markup as a
/// read-only value ([`stated`]).
///
/// Every mode is a control, including the current one, even mid-turn (§7.6).
/// `connected` says whether there is an agent to ask, not what state it is in;
/// an agent that leaves between render and click is refused by core
/// ([`ModeChange::Refused`]).
fn mode_segment(
    mode: &v1::SessionMode,
    current: bool,
    connected: bool,
    on_set_mode: EventHandler<v1::SessionModeId>,
) -> Element {
    let id = mode.id.to_string();
    let picked = mode.id.clone();
    // The name once; the id is a caption only where it differs, and always
    // the tooltip.
    let distinct = !id.eq_ignore_ascii_case(mode.name.trim());

    rsx! {
        button {
            key: "{id}",
            class: "mode-row",
            r#type: "button",
            title: "{id}",
            "data-mode": "{id}",
            "data-current": "{current}",
            aria_pressed: current,
            aria_disabled: (!connected).then_some("true"),
            tabindex: (!connected).then_some("-1"),
            onclick: move |_| {
                if connected {
                    on_set_mode.call(picked.clone());
                }
            },
            span { class: "mode-name", "{mode.name}" }
            if distinct {
                span { class: "mode-id", "{id}" }
            }
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

/// What became of the last `session/set_mode` (§7.6): a refusal reads as a
/// failure; an acknowledgement nobody restated is permitted silence, so it is
/// said plainly.
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
        // `ModeChange` is non-exhaustive; an unknown outcome is still said.
        _ => rsx! {
            p { class: "alert", "data-slot": "setting-result", role: "status", "An outcome this window has no rendering for." }
        },
    }
}

/// A mode the agent stated while the session opened, where the answer that
/// opened it published no modes (§7.6). The answer wins, so there is no mode
/// row; this says what crossed, beside the surface.
///
/// It does not call it a replay: the same frames can arrive under
/// `session/resume`, which must not replay. Not a Conformance Annotation,
/// since nothing obliges an agent to publish the modes it announces.
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

/// What became of the last `session/set_config_option` (§7.6): only a failure
/// is said, since a success is the new option set itself. The error says which
/// failure it was.
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

/// The value a set asked for, as the wire carried it: a quoted value id or an
/// unquoted boolean. `None` for a shape this window cannot name, since
/// `SessionConfigOptionValue` is non-exhaustive (as [`current_value`]).
fn asked_for(value: &v1::SessionConfigOptionValue) -> Option<String> {
    match value {
        v1::SessionConfigOptionValue::ValueId { value } => Some(value.to_string()),
        v1::SessionConfigOptionValue::Boolean { value } => Some(value.to_string()),
        _ => None,
    }
}

/// What a config option could be set to, and the controls that would set it.
/// A select writes a bare value id; a boolean writes a `type`-tagged boolean.
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
        // The value as the wire has it, not a paraphrase.
        v1::SessionConfigKind::Boolean(boolean) => switchable(
            &option.id,
            &option.name,
            boolean.current_value,
            description_id,
            connected,
            on_set_config_option,
        ),
        // `SessionConfigKind` is non-exhaustive: a kind with no rendering is
        // still listed, read-only. Unreachable today, since v1 skips unknown
        // kinds when decoding (`crates/core/tests/session_settings.rs`).
        _ => rsx! {
            p { class: "alert unnamed", "An option kind this window has no rendering for." }
        },
    }
}

/// A boolean option as a chip showing the agent's stated value; pressing it
/// asks for the inverse. A chip, not a switch, because nothing is applied
/// optimistically (§7.6). `aria-pressed` carries the value.
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

/// The values a select offers, as chips — or a native select where the agent
/// grouped them, since chips cannot show groups.
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

/// The values a grouped select offers, as a native select with `<optgroup>`s.
/// Choosing prepares the ask and *Set* sends it, even for the current value
/// (§7.6).
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

/// One value the agent published, as the chip that would set it. As with
/// [`mode_segment`], the current value is a control too.
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

    /// One setting row, either half of the surface.
    const SETTING: &str = r#"data-slot="setting""#;
    /// The value the agent says the session is at.
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

    /// Two modes and one select option, as the upstream test agent sends.
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

    /// A config option whose id is literally `mode`, beside the modes.
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

    /// The surface, rendered, with an agent connected.
    fn shown(settings: acp_inspector_core::SessionSettings) -> String {
        rendered(settings, true)
    }

    /// The same surface, for a session whose agent has left.
    fn shown_without_an_agent(settings: acp_inspector_core::SessionSettings) -> String {
        rendered(settings, false)
    }

    #[test]
    fn a_surface_whose_agent_has_gone_says_why_its_controls_do_nothing() {
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

        // Only where there is a control it is about.
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

    /// The agent's stated mode and what became of the last set.
    fn asked(change: ModeChange) -> acp_inspector_core::SessionSettings {
        let mut settings = published(Some(modes()), None);
        settings.mode_change = Some(change);
        settings
    }

    /// One published option, and a refused set of it or of an unpublished one.
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
        // Every mode is a visible row, however many: no popup before prompting.
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
            assert!(
                html.contains(&format!(r#"title="{mode}""#)),
                "the id is on the row — it is what the set sends, and what a reader checks this \
                 screen against the trace by: {html}"
            );
        }
        assert!(
            html.contains(r#"class="mode-name">Chat<"#)
                && html.contains(r#"class="mode-name">Plan<"#)
                && !html.contains(r#"class="mode-id""#),
            "a name that is its id is said once: {html}"
        );
        let renamed = shown(published(
            Some(v1::SessionModeState::new(
                "code",
                vec![v1::SessionMode::new("code", "Write code")],
            )),
            None,
        ));
        assert!(
            renamed.contains(r#"class="mode-name">Write code<"#)
                && renamed.contains(r#"class="mode-id">code<"#),
            "and an id that says something else is a caption: {renamed}"
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
        // A chip, not a switch: nothing is applied optimistically (§7.6).
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
        // Only a grouped select goes to a native select.
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
            html.contains("Changing mode with")
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
        // A rejected boolean set names the ask: an unquoted boolean.
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
        // A failed set leaves the values as the agent last stated them (§7.6).
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
        // The replacement set may drop the refused option; the refusal stays.
        let html = shown(refused(v1::SessionConfigId::new("colour"), "green"));

        assert!(
            html.contains("unsupported verbosity value") && html.contains("colour"),
            "said beside the surface instead: {html}"
        );
    }

    #[test]
    fn the_controls_are_dead_when_the_agent_that_published_them_has_gone() {
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
        // The row keeps the agent's stated mode; the sentence says what became
        // of the ask (§7.6).
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
        // A failed set leaves the row as the agent last stated it (§7.6).
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
        // An agent may announce a mode it never offered; it is still said.
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
        // A setting published both ways appears twice (§7.6).
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
        // No modes, no mode row (§7.6).
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
        // An empty list and no list are different answers, both said (§7.6).
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
        // A mode stated before an answer that offered none: the answer wins (no
        // control), and what the agent did is said (§7.6).
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
    fn the_surface_says_its_controls_act_at_once() {
        // There is no Apply: every control here sends its call when pressed.
        let html = shown(published(Some(modes()), Some(vec![verbosity()])));

        assert!(
            html.contains("Changes are sent to the agent immediately."),
            "{html}"
        );
        assert!(!html.contains("Decoded view"), "{html}");
    }
}
