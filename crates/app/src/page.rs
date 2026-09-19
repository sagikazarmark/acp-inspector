//! Bounded DOM lists. An anchor is a stable ordinal, not an offset that drifts
//! when evidence ages out. None follows the newest page.
use dioxus::prelude::*;

pub const SIZE: usize = 200;

pub fn range(ordinals: &[u64], anchor: Option<u64>) -> std::ops::Range<usize> {
    if ordinals.len() <= SIZE {
        return 0..ordinals.len();
    }
    let start = anchor
        .and_then(|id| ordinals.binary_search(&id).ok())
        .unwrap_or_else(|| ordinals.len().saturating_sub(SIZE))
        .min(ordinals.len().saturating_sub(SIZE));
    start..(start + SIZE).min(ordinals.len())
}

pub fn controls(
    ordinals: &[u64],
    range: std::ops::Range<usize>,
    mut anchor: Signal<Option<u64>>,
    list: &'static str,
) -> Element {
    if ordinals.len() <= SIZE {
        return rsx! {};
    }
    let older = ordinals[range.start.saturating_sub(SIZE)];
    let newer = ordinals.get(range.end).copied();
    let start = range.start + 1;
    let end = range.end;
    let total = ordinals.len();
    rsx! {
        nav { class: "narrowed", aria_label: "Evidence pages", "data-slot": "pages",
            button { class: "btn btn-xs btn-quiet", disabled: range.start == 0,
                aria_label: format!("Older evidence page (before row {start})"),
                onclick: move |_| anchor.set(Some(older)), "Older" }
            span { " {start}–{end} of {total} retained rows " }
            button { class: "btn btn-xs btn-quiet", disabled: newer.is_none(),
                onclick: move |_| anchor.set(newer), "Newer" }
            button { class: "btn btn-xs btn-quiet", aria_label: format!("Latest {list} page"),
                onclick: move |_| crate::tail::latest(list, Some(anchor)), "Latest" }
        }
    }
}

/// A literal UTF-8 prefix, never a rewritten or invalid partial code point.
pub fn prefix(text: &str, limit: usize) -> &str {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Large raw evidence is read in literal byte windows. No JSON rewriting and
/// no ellipsis in the content; adjacent windows concatenate to the original.
#[component]
pub fn RawFrame(frame: acp_inspector_core::Frame) -> Element {
    rsx! { RawText { frame: Some(frame), what: "Frame" } }
}

#[component]
pub fn RawText(
    #[props(default)] frame: Option<acp_inspector_core::Frame>,
    #[props(default)] shared: Option<std::sync::Arc<str>>,
    what: String,
) -> Element {
    let mut page = use_signal(|| 0usize);
    let text = frame
        .as_ref()
        .map(|frame| frame.as_str())
        .or(shared.as_deref())
        .unwrap_or_default();
    let size = 16 * 1024;
    let pages = text.len().div_ceil(size).max(1);
    let current = page().min(pages - 1);
    let boundary = |mut position: usize| {
        position = position.min(text.len());
        while !text.is_char_boundary(position) {
            position -= 1;
        }
        position
    };
    let start = boundary(current * size);
    let end = boundary((current + 1) * size);
    rsx! {
        div { class: "raw-row",
            if pages > 1 {
                nav { class: "narrowed", aria_label: "Raw {what} byte windows",
                    button { class: "btn btn-xs btn-quiet", disabled: current == 0,
                        onclick: move |_| page.set(current.saturating_sub(1)), "Previous bytes" }
                    span { " Bytes {start}–{end} of {text.len()} (literal raw UTF-8). " }
                    button { class: "btn btn-xs btn-quiet", disabled: current + 1 == pages,
                        aria_label: format!("Next raw {what} bytes (page {} of {pages})", (current + 2).min(pages)),
                        onclick: move |_| page.set(current + 1), "Next bytes" }
                }
            }
            pre { class: "raw", "{&text[start..end]}" }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn click(dom: &mut VirtualDom, id: dioxus::core::ElementId) {
        use dioxus::html::{PlatformEventData, SerializedMouseData};
        use std::{any::Any, rc::Rc};
        set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let data = Rc::new(PlatformEventData::new(Box::new(
            SerializedMouseData::default(),
        ))) as Rc<dyn Any>;
        dom.runtime()
            .handle_event("click", Event::new(data, true), id);
        dom.render_immediate_to_vec();
    }

    pub(crate) fn labelled(
        edits: &[dioxus::core::Mutation],
        prefix: &str,
    ) -> dioxus::core::ElementId {
        edits
            .iter()
            .find_map(|edit| match edit {
                dioxus::core::Mutation::SetAttribute {
                    name: "aria-label",
                    value: dioxus::core::AttributeValue::Text(text),
                    id,
                    ..
                } if text.starts_with(prefix) => Some(*id),
                _ => None,
            })
            .expect("labelled control")
    }
    #[test]
    fn stable_older_page_and_reveal_survive_rotation() {
        let ids: Vec<_> = (100..10100).collect();
        assert_eq!(range(&ids, None), 9800..10000);
        assert_eq!(range(&ids, Some(150)), 50..250);
        assert_eq!(range(&ids[20..], Some(150)), 30..230);
        assert_eq!(prefix("a🦀z", 4), "a");
    }

    #[test]
    fn filter_that_excludes_anchor_shows_a_full_last_page_with_a_way_back() {
        let older_matches: Vec<_> = (100..700).collect();
        let shown = range(&older_matches, Some(9000));
        assert_eq!(shown, 400..600);
        let previous = older_matches[shown.start.saturating_sub(SIZE)];
        assert_eq!(range(&older_matches, Some(previous)), 200..400);
        assert_eq!(range(&older_matches, Some(699)), 400..600);
        let sparse: Vec<_> = (0..1000).filter(|id| *id != 350).collect();
        assert_eq!(range(&sparse, Some(350)), 799..999);
        assert_eq!(range(&sparse, Some(351)), 350..550);
    }

    #[test]
    fn successive_rendered_raw_windows_reconstruct_utf8_evidence_exactly() {
        use dioxus::core::{AttributeValue, Mutation};
        use dioxus::html::{PlatformEventData, SerializedMouseData};
        use std::{any::Any, rc::Rc};
        set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let raw = format!("{}🦀{}éEND", "x".repeat(16383), "y".repeat(16382));
        let mut dom = VirtualDom::new_with_props(
            RawFrame,
            RawFrameProps::builder()
                .frame(acp_inspector_core::Frame::new(raw.clone()))
                .build(),
        );
        let edits = dom.rebuild_to_vec().edits;
        let next = edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::SetAttribute {
                    name: "aria-label",
                    value: AttributeValue::Text(text),
                    id,
                    ..
                } if text.starts_with("Next raw Frame bytes") => Some(*id),
                _ => None,
            })
            .unwrap();
        let mut reconstructed = String::new();
        for page in 0..3 {
            let html = dioxus_ssr::render(&dom);
            let content = html
                .split_once("<pre class=\"raw\">")
                .unwrap()
                .1
                .split_once("</pre>")
                .unwrap()
                .0;
            reconstructed.push_str(content);
            if page < 2 {
                let data = Rc::new(PlatformEventData::new(Box::new(
                    SerializedMouseData::default(),
                ))) as Rc<dyn Any>;
                dom.runtime()
                    .handle_event("click", Event::new(data, true), next);
                dom.render_immediate_to_vec();
            }
        }
        assert_eq!(reconstructed, raw);
    }
}
