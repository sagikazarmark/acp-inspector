//! Drive the same layout boundary as App, with the real stateful Timeline.
//! Events enter rendered controls; assertions read their rendered values.

use std::{any::Any, cell::RefCell, rc::Rc, time::Duration};

use acp_inspector_core::{ElicitationRequest, EntryId, EntryKind};
use dioxus::core::{AttributeValue, ElementId, Mutation};
use dioxus::html::{
    PlatformEventData, SerializedFileData, SerializedFormData, SerializedFormObject,
    SerializedMouseData,
};

use super::*;

#[derive(Clone, Props)]
struct HostProps {
    request: Option<ElicitationRequest>,
    sent: Rc<RefCell<Vec<Vec<v1::ContentBlock>>>>,
    answers: Rc<RefCell<Vec<elicitation::Answer>>>,
}

impl PartialEq for HostProps {
    fn eq(&self, _: &Self) -> bool {
        false
    }
}

fn host(props: HostProps) -> Element {
    let mut spine = use_signal(Spine::default);
    let mut session = use_signal(|| Some(v1::SessionId::new("first")));
    let mut epoch = use_signal(|| 0_u64);
    let mut entries = use_signal(|| {
        props
            .request
            .into_iter()
            .map(|request| TimelineEntry {
                id: EntryId::Elicitation(request.id().clone()),
                at: std::time::SystemTime::UNIX_EPOCH,
                frames: vec![],
                kind: EntryKind::Elicitation(request),
                turn: None,
            })
            .collect::<Vec<_>>()
    });
    rsx! {
        for chosen in Spine::ALL {
            button { aria_label: chosen.label(), onclick: move |_| spine.set(chosen), "{chosen.label()}" }
        }
        button { aria_label: "Switch Session".to_owned(), onclick: move |_| {
            session.set(Some(v1::SessionId::new("second")));
            entries.set(vec![]);
        }, "Switch Session" }
        button { aria_label: "Reopen Session".to_owned(), onclick: move |_| epoch += 1, "Reopen Session" }
        button { aria_label: "Refresh requests".to_owned(), onclick: move |_| {
            let current = entries();
            entries.set(current);
        }, "Refresh requests" }
        Screens {
            spine: spine(),
            timeline: rsx! { Timeline {
                entries, turn: TurnState::Idle, session: session(), prompt_epoch: epoch(),
                connected: true, held_frames: 0..0, blocked: false, sought: None,
                described: true, turns: vec![], problem: None, on_new_session: None,
                mode: None, image_advertised: true,
                on_prompt: move |content| { props.sent.borrow_mut().push(content); Ok(()) },
                on_elicit: move |answer| props.answers.borrow_mut().push(answer),
                on_reveal: |_| {}, on_stop: |_| {}, on_set_mode: |_| {}, on_answer: |_| {},
            } },
            console: rsx! { aside { "Console" } },
        }
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

fn event(data: impl Any) -> Event<dyn Any> {
    Event::new(
        Rc::new(PlatformEventData::new(Box::new(data))) as Rc<dyn Any>,
        true,
    )
}

fn listener(edits: &[Mutation], event: &str) -> ElementId {
    edits
        .iter()
        .rev()
        .find_map(|edit| match edit {
            Mutation::NewEventListener { name, id } if name == event => Some(*id),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing {event} listener"))
}

fn click(dom: &mut VirtualDom, id: ElementId) -> Vec<Mutation> {
    dom.runtime()
        .handle_event("click", event(SerializedMouseData::default()), id);
    dom.render_immediate_to_vec().edits
}

fn input(dom: &mut VirtualDom, id: ElementId, text: &str) -> Vec<Mutation> {
    dom.runtime().handle_event(
        "input",
        event(SerializedFormData::new(text.into(), vec![])),
        id,
    );
    dom.render_immediate_to_vec().edits
}

async fn attach(dom: &mut VirtualDom, edits: &[Mutation]) {
    let picker = edits
        .iter()
        .find_map(|edit| match edit {
            Mutation::NewEventListener { name, id } if name == "change" => Some(*id),
            _ => None,
        })
        .expect("attachment picker");
    dom.runtime().handle_event(
        "change",
        event(SerializedFormData::new(
            "".into(),
            vec![SerializedFormObject {
                key: "prompt-attachment".into(),
                text: None,
                file: Some(SerializedFileData {
                    path: "draft.png".into(),
                    size: 8,
                    last_modified: 0,
                    content_type: None,
                    contents: Some(b"\x89PNG\r\n\x1a\n".to_vec().into()),
                }),
            }],
        )),
        picker,
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            dom.wait_for_work().await;
            dom.render_immediate_to_vec();
            if dioxus_ssr::render(dom).contains("data:image/png;base64,iVBORw0KGgo=") {
                break;
            }
        }
    })
    .await
    .expect("attachment read completes");
}

#[tokio::test]
async fn layout_preserves_prompt_and_attachment_but_session_changes_reset_them() {
    set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    let sent = Rc::new(RefCell::new(vec![]));
    let mut dom = VirtualDom::new_with_props(
        host,
        HostProps {
            request: None,
            sent: sent.clone(),
            answers: Default::default(),
        },
    );
    let edits = dom.rebuild_to_vec().edits;
    input(&mut dom, listener(&edits, "input"), "Keep this prompt");
    attach(&mut dom, &edits).await;
    click(&mut dom, target(&edits, "aria-label", "JSON-RPC full"));
    let hidden = dioxus_ssr::render(&dom);
    assert!(
        hidden.contains("class=\"split spine-wire\""),
        "the layout applies the CSS hide state"
    );
    assert!(
        hidden.contains("Keep this prompt"),
        "the hidden Timeline stays mounted"
    );
    click(&mut dom, target(&edits, "aria-label", "Split"));
    let html = dioxus_ssr::render(&dom);
    assert!(
        html.contains("Keep this prompt"),
        "layout must retain prompt text"
    );
    assert!(
        html.contains("data:image/png;base64,iVBORw0KGgo="),
        "layout must retain attachment bytes"
    );
    click(&mut dom, listener(&edits, "click"));
    let content = sent.borrow_mut().pop().expect("preserved draft sends");
    assert_eq!(
        serde_json::to_value(content).unwrap(),
        serde_json::json!([
            {"type":"text", "text":"Keep this prompt"},
            {"type":"image", "data":"iVBORw0KGgo=", "mimeType":"image/png"}
        ])
    );

    // Both reset paths matter: a different id, and reopening the same Session.
    let wire = target(&edits, "aria-label", "JSON-RPC full");
    let split = target(&edits, "aria-label", "Split");
    let resets =
        ["Switch Session", "Reopen Session"].map(|name| (name, target(&edits, "aria-label", name)));
    let mut controls = edits;
    for (reset, control) in resets {
        input(&mut dom, listener(&controls, "input"), "Discard this draft");
        attach(&mut dom, &controls).await;
        click(&mut dom, wire);
        controls = click(&mut dom, control);
        controls.extend(click(&mut dom, split));
        let html = dioxus_ssr::render(&dom);
        assert!(
            !html.contains("Discard this draft"),
            "{reset} clears text even while hidden"
        );
        assert!(
            !html.contains("data-slot=\"attachment\""),
            "{reset} clears attachments"
        );
    }
}

#[tokio::test]
async fn layout_preserves_elicitation_form_raw_and_resolved_values() {
    // A scripted Agent supplies a form with a deterministic field and default;
    // the request and its resolution still pass through the real core boundary.
    let inspector = Inspector::new();
    let frame = r#"{"jsonrpc":"2.0","id":"draft-form","method":"elicitation/create","params":{"sessionId":"first","mode":"form","message":"Name this run","requestedSchema":{"type":"object","properties":{"name":{"type":"string","default":"Original"}}}}}"#;
    inspector.connect(
        &AgentCommand {
            command: "sh".into(),
            args: format!("-c\nprintf '%s\\n' '{frame}'; cat >/dev/null"),
            ..AgentCommand::default()
        }
        .factory(),
    );
    let request = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(request) = inspector.pending_elicitations().first() {
                break request.clone();
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("Agent asks for input");
    set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    let answers = Rc::new(RefCell::new(vec![]));
    let mut dom = VirtualDom::new_with_props(
        host,
        HostProps {
            request: Some(request.clone()),
            sent: Default::default(),
            answers: answers.clone(),
        },
    );
    let edits = dom.rebuild_to_vec().edits;
    let field = target(&edits, "data-schemaform-control", "string");
    input(&mut dom, field, "Edited form");
    assert!(dioxus_ssr::render(&dom).contains("value=\"Edited form\""));
    let wire = target(&edits, "aria-label", "JSON-RPC full");
    let split = target(&edits, "aria-label", "Split");
    click(&mut dom, wire);
    click(&mut dom, split);
    assert!(
        dioxus_ssr::render(&dom).contains("value=\"Edited form\""),
        "layout retains the form engine's edited value"
    );

    let raw = target(&edits, "data-surface", "Raw");
    let form = target(&edits, "data-surface", "Form");
    let raw_edits = click(&mut dom, raw);
    input(
        &mut dom,
        listener(&raw_edits, "input"),
        r#"{"name":"Independent raw"}"#,
    );
    click(&mut dom, wire);
    click(&mut dom, split);
    assert!(
        dioxus_ssr::render(&dom).contains("Independent raw"),
        "layout retains Raw and its draft"
    );
    click(&mut dom, form);
    assert!(dioxus_ssr::render(&dom).contains("value=\"Edited form\""));

    click(&mut dom, target(&edits, "data-answer", "accept"));
    let answer = answers.borrow_mut().pop().expect("edited form answers");
    let elicitation::Reply::Accept(data) = answer.reply else {
        panic!("Accept")
    };
    assert_eq!(
        data,
        Some(
            serde_json::json!({"name":"Edited form"})
                .as_object()
                .unwrap()
                .clone()
        )
    );
    request.accept(data).await.unwrap();
    click(&mut dom, target(&edits, "aria-label", "Refresh requests"));
    click(&mut dom, wire);
    click(&mut dom, split);
    let html = dioxus_ssr::render(&dom);
    assert!(
        html.contains("resolved") && html.contains("value=\"Edited form\""),
        "resolved fields keep the displayed answer"
    );
    click(&mut dom, raw);
    assert!(dioxus_ssr::render(&dom).contains("Independent raw"));
    click(&mut dom, wire);
    click(&mut dom, target(&edits, "aria-label", "Switch Session"));
    click(&mut dom, split);
    assert!(
        !dioxus_ssr::render(&dom).contains("data-slot=\"elicitation\""),
        "Session switches discard old Timeline entries"
    );
    inspector.disconnect();
}
