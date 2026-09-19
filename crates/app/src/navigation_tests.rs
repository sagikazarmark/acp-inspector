//! Real controls over App's shared navigation seam, with both evidence surfaces.
use std::{any::Any, rc::Rc, time::SystemTime};

use acp_inspector_core::{Direction, EntryId, EntryKind, Frame, Recorded};
use dioxus::core::{AttributeValue, ElementId, Mutation};
use dioxus::html::{PlatformEventData, SerializedMouseData};

use super::*;

fn host() -> Element {
    let nav = navigation::use_navigation();
    let frame = Frame::new(r#"{"jsonrpc":"2.0","method":"session/update"}"#);
    let frames = use_signal(|| {
        vec![TracedFrame {
            connection: 1,
            at: SystemTime::UNIX_EPOCH,
            direction: Direction::FromAgent,
            frame: frame.clone(),
        }]
    });
    let entries = use_signal(|| {
        vec![TimelineEntry {
            id: EntryId::Arrival(0),
            at: SystemTime::UNIX_EPOCH,
            turn: None,
            frames: vec![Recorded {
                frame,
                captured: Some(0),
            }],
            kind: EntryKind::Update(v1::SessionUpdate::AgentMessageChunk(v1::ContentChunk::new(
                v1::ContentBlock::from("Navigation evidence"),
            ))),
        }]
    });
    let decoded = use_signal(|| HashSet::from([0]));
    let lines = use_signal(Vec::new);
    let commands = commands(PaletteWiring {
        connected: false,
        described: false,
        traced: true,
        spine: (nav.spine)(),
        on_spine: EventHandler::new(move |chosen| nav.layout(chosen)),
        on_tab: EventHandler::new(move |chosen| nav.console(chosen)),
        on_rail: EventHandler::new(move |chosen| nav.rail(chosen)),
        on_new_session: EventHandler::new(|_| {}),
        on_stop: EventHandler::new(|_| {}),
        on_export: EventHandler::new(|_| {}),
        on_clear: EventHandler::new(|_| {}),
        on_indent: EventHandler::new(|_| {}),
        on_appearance: EventHandler::new(|_| {}),
    });
    rsx! {
        TopBar {
            status: ConnectionStatus::Disconnected, appearance: Appearance::System,
            indentation: Indentation::Wire, subject: None, protocol: None, session: None,
            spine: (nav.spine)(), on_spine: move |chosen| nav.layout(chosen),
            on_choose: |_| {}, on_indent: |_| {},
        }
        for command in commands {
            button { aria_label: format!("Palette {}", command.label),
                onclick: move |_| command.run.call(()), "{command.label}" }
        }
        div { class: (nav.region)().class(),
            Rail {
                status: ConnectionStatus::Disconnected, launched: None, session: None,
                claims: agent::Claims::about(None), settings: SessionSettings::default(),
                settings_epoch: 0, changing_mode: false, setting_option: None, commands: 0,
                tab: (nav.rail)(), on_tab: move |chosen| nav.rail(chosen),
                on_stop: |_| {}, on_set_mode: |_| {}, on_set_config_option: |_| {},
            }
            Screens {
                spine: (nav.spine)(),
                timeline: rsx! { Timeline {
                    entries, turn: TurnState::Idle, session: Some(v1::SessionId::new("session")),
                    connected: true, held_frames: 0..1, blocked: false, sought: (nav.sought)(),
                    described: true, turns: vec![], problem: None, mode: None,
                    on_prompt: |_| Ok(()), on_elicit: |_| {}, on_stop: |_| {},
                    on_set_mode: |_| {}, on_answer: |_| {},
                    on_reveal: move |frames| nav.reveal(frames),
                } },
                console: rsx! { Console {
                    frames, dropped_frames: 0, saved: None, on_export: |_| {}, on_clear: |_| {},
                    lines, dropped_lines: 0, tab: (nav.console)(),
                    on_select: move |chosen| nav.console(chosen),
                    revealed: nav.revealed, decoded, on_seek: move |ordinal| nav.seek(ordinal),
                } },
            }
        }
        RegionBar { selected: (nav.region)(), on_select: move |chosen| nav.region(chosen) }
    }
}

fn target(edits: &[Mutation], attribute: &str, expected: &str) -> ElementId {
    edits
        .iter()
        .rev()
        .find_map(|edit| match edit {
            Mutation::SetAttribute {
                name,
                value: AttributeValue::Text(value),
                id,
                ..
            } if *name == attribute && *value == expected => Some(*id),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing {attribute}={expected}"))
}

fn click(dom: &mut VirtualDom, id: ElementId) -> Vec<Mutation> {
    let data = Rc::new(PlatformEventData::new(Box::new(
        SerializedMouseData::default(),
    ))) as Rc<dyn Any>;
    dom.runtime()
        .handle_event("click", Event::new(data, true), id);
    dom.render_immediate_to_vec().edits
}

#[test]
fn trace_turn_reveals_timeline_from_full_wire_and_narrow_messages() {
    set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    let mut dom = VirtualDom::new(host);
    let mut edits = dom.rebuild_to_vec().edits;
    let full = target(&edits, "title", Spine::Wire.says());
    edits.extend(click(&mut dom, full));
    let turn = target(&edits, "aria-label", "Show Timeline entry for Frame 0");
    click(&mut dom, turn);
    let html = dioxus_ssr::render(&dom);
    assert!(
        html.contains("split spine-split"),
        "seeking must reveal the Timeline"
    );
    assert!(
        html.contains("body on-turn"),
        "seeking must select narrow Session"
    );
    assert!(html.contains("row sought"), "the requested entry is marked");
    click(&mut dom, target(&edits, "aria-label", "Messages"));
    click(&mut dom, turn);
    assert!(dioxus_ssr::render(&dom).contains("body on-turn"));
}

#[test]
fn palette_rail_destinations_reveal_details_and_select_the_requested_tab() {
    set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    let mut dom = VirtualDom::new(host);
    let edits = dom.rebuild_to_vec().edits;
    for (command, tab) in [
        ("Capabilities", "rail-panel-capabilities"),
        ("Session settings", "rail-panel-session"),
    ] {
        click(&mut dom, target(&edits, "aria-label", "Messages"));
        click(
            &mut dom,
            target(&edits, "aria-label", &format!("Palette {command}")),
        );
        let html = dioxus_ssr::render(&dom);
        assert!(html.contains("body on-rail"), "{command} reveals Details");
        assert!(html.contains(&format!("id=\"{tab}\"")));
    }
}

#[test]
fn full_wire_and_mobile_session_never_hide_both_screens() {
    set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    let mut dom = VirtualDom::new(host);
    let mut edits = dom.rebuild_to_vec().edits;
    for full in ["toolbar", "palette"] {
        let control = if full == "toolbar" {
            target(&edits, "title", Spine::Wire.says())
        } else {
            target(&edits, "aria-label", "Palette JSON-RPC full")
        };
        edits.extend(click(&mut dom, control));
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains("spine-wire") && html.contains("body on-wire"),
            "{full} full wire reveals Messages"
        );
        edits.extend(click(&mut dom, target(&edits, "aria-label", "Session")));
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains("spine-split") && html.contains("body on-turn"),
            "Session reveals Timeline even after full wire"
        );
    }
}

#[test]
fn timeline_evidence_reveals_trace_after_diagnostics() {
    set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    let mut dom = VirtualDom::new(host);
    let mut edits = dom.rebuild_to_vec().edits;
    let evidence = target(&edits, "title", "Find this frame in the wire log");
    // Leave Trace, then follow actual Timeline evidence back to it.
    edits.extend(click(
        &mut dom,
        target(&edits, "aria-label", "Palette Diagnostics"),
    ));
    edits.extend(click(&mut dom, evidence));
    let html = dioxus_ssr::render(&dom);
    assert!(html.contains("body on-wire"));
    assert!(
        html.contains("frame-row revealed"),
        "the Frame must be visible and selected"
    );
    assert!(html.contains("id=\"console-tab-trace\" class=\"wire-tab\" type=\"button\" role=\"tab\" aria-selected=true"));
    // Navigate back and repeat the same destinations, rather than relying on
    // a changing ordinal to trigger a second visit.
    let new_turn = target(&edits, "aria-label", "Show Timeline entry for Frame 0");
    click(&mut dom, new_turn);
    assert!(dioxus_ssr::render(&dom).contains("body on-turn"));
}
