//! The unit of the wire: one JSON-RPC message, verbatim, and which way it went.

use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
use std::fmt;
use std::sync::{Arc, OnceLock};

/// One JSON-RPC message as it crossed the wire (`CONTEXT.md`, *Frame*).
///
/// **Text, not a parsed value.** The inspector's subject matter is what an agent
/// actually emitted, including what no schema recognizes, so nothing below the
/// typed layer parses, validates, or reformats a frame — it is carried and
/// recorded exactly as it arrived (§8, the raw-first rule). A frame that is not
/// JSON at all is still a frame, and still gets shown.
///
/// Cheap to clone, because every frame is cloned at least once: the trace keeps
/// a copy of everything that passes the seam.
#[derive(Clone)]
pub struct Frame(Arc<Contents>);

struct Contents {
    text: Arc<str>,
    summary: OnceLock<FrameSummary>,
}

impl PartialEq for Frame {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.as_str() == other.as_str()
    }
}
impl Eq for Frame {}

/// Which way a frame went. The inspector is always the client, so the two
/// directions are the two ends of its own connection — never a third party's
/// (§13: observing other people's traffic is Wiretap's charter, not this one).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    ToAgent,
    FromAgent,
}

impl Frame {
    pub fn new(text: impl Into<Arc<str>>) -> Self {
        Self(Arc::new(Contents {
            text: text.into(),
            summary: OnceLock::new(),
        }))
    }

    pub fn as_str(&self) -> &str {
        &self.0.text
    }

    /// What the frame says it is, for a surface that has to draw a thousand of
    /// them in one column.
    ///
    /// **This is not the typed layer and it decodes nothing** (§8). It reads
    /// the four fields the JSON-RPC envelope itself defines — `method`, `id`,
    /// `result`, `error` — and says which of the four shapes the envelope is
    /// in. It never looks inside `params`, never names a v1 type, and never
    /// decides whether the agent was right; the Timeline is where a frame
    /// becomes something with a meaning, and that is still true.
    ///
    /// **A frame it cannot read is still a frame**, which is the rule this
    /// method exists under rather than around: anything that is not a JSON
    /// object is [`FrameKind::Unreadable`], and the surface that asked draws
    /// exactly what it drew before — the frame's own text, verbatim. The
    /// summary is a thing put *in front of* the bytes, never instead of them.
    pub fn summary(&self) -> &FrameSummary {
        self.0.summary.get_or_init(|| {
            serde_json::from_str(self.as_str()).unwrap_or_else(|_| FrameSummary::unreadable())
        })
    }

    /// Whether this frame can cross a message-per-line transport as the single
    /// message it claims to be.
    ///
    /// Only `\n` is asked about: it is the framing character, and a frame that
    /// contains one would arrive at the agent as two messages while the trace
    /// showed one — the inspector lying about the wire, which is the one thing
    /// it may never do.
    pub(crate) fn is_one_message(&self) -> bool {
        !self.as_str().contains('\n')
    }
}

/// Which of the JSON-RPC envelope's four shapes a frame is in.
///
/// The envelope's own vocabulary and nothing above it: what a *call* is for is
/// the Timeline's answer, and whether the agent should have sent one is the
/// Conformance Annotations'.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    /// A method with an id: somebody is waiting for an answer.
    Call,
    /// A method without one: nobody is.
    Notification,
    /// An answer that worked.
    Result,
    /// An answer that did not.
    Error,
    /// Not a JSON object, or a JSON object in none of those four shapes —
    /// which is a frame worth seeing rather than a frame to hide (§8).
    Unreadable,
}

/// What a frame says it is, read off the envelope alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameSummary {
    pub kind: FrameKind,
    /// The method it named, where it named one.
    pub method: Option<String>,
    /// The id it carried, as it was written.
    pub id: Option<String>,
}

// Validate the envelope once, borrowing and skipping payloads without allocating
// their object trees. Last duplicate key wins, as in the former Value reader.
impl<'de> Deserialize<'de> for FrameSummary {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Envelope;
        impl<'de> Visitor<'de> for Envelope {
            type Value = FrameSummary;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an envelope object")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let (mut method, mut id, mut result, mut error) = (None, None, false, false);
                while let Some(key) = map.next_key::<String>()? {
                    let raw = map.next_value::<&serde_json::value::RawValue>()?;
                    match key.as_str() {
                        "method" => method = serde_json::from_str::<String>(raw.get()).ok(),
                        "id" => {
                            // Use the existing Value number semantics for this label only;
                            // never convert IDs through f64 ourselves or decode payloads.
                            id = match serde_json::from_str::<serde_json::Value>(raw.get())
                                .map_err(serde::de::Error::custom)?
                            {
                                serde_json::Value::String(s) => Some(s),
                                serde_json::Value::Number(n) => Some(n.to_string()),
                                _ => None,
                            };
                        }
                        "result" => result = true,
                        "error" => error = true,
                        _ => {}
                    }
                }
                let kind = match (&method, error, &id) {
                    (Some(_), _, Some(_)) => FrameKind::Call,
                    (Some(_), _, None) => FrameKind::Notification,
                    (None, true, _) => FrameKind::Error,
                    (None, false, _) if result => FrameKind::Result,
                    _ => FrameKind::Unreadable,
                };
                Ok(FrameSummary { kind, method, id })
            }
        }
        deserializer.deserialize_map(Envelope)
    }
}

impl FrameSummary {
    fn unreadable() -> Self {
        Self {
            kind: FrameKind::Unreadable,
            method: None,
            id: None,
        }
    }

    /// The one word a row leads with, or `None` where the envelope said nothing
    /// this can put there.
    ///
    /// The method where there is one, because that is what a reader scans a
    /// trace for; otherwise what kind of answer it is, because an answer's own
    /// method is the *call's* and correlating the two is the reader's business
    /// rather than a label's.
    pub fn label(&self) -> Option<&str> {
        match (&self.method, self.kind) {
            (Some(method), _) => Some(method),
            (None, FrameKind::Result) => Some("result"),
            (None, FrameKind::Error) => Some("error"),
            _ => None,
        }
    }
}

impl fmt::Display for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Frame {
    /// The frame's text, not `Frame("…")` around it: every place this shows up —
    /// a failed assertion, a log line — wants the JSON that was on the wire.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_cache_is_shared_and_payload_numbers_are_not_interpreted() {
        let frame = Frame::new(
            r#"{"id":18446744073709551615,"method":"first","method":"last","params":{"huge":1e9999,"marker":{"$serde_json::private::Number":"x"}}}"#,
        );
        let clone = frame.clone();
        assert!(std::ptr::eq(frame.summary(), clone.summary()));
        assert_eq!(frame.summary().method.as_deref(), Some("last"));
        assert_eq!(frame.summary().id.as_deref(), Some("18446744073709551615"));
        assert_eq!(
            Frame::new(r#"{"method":"x","params":[1,]}"#).summary().kind,
            FrameKind::Unreadable
        );
    }

    fn summary(text: &str) -> FrameSummary {
        Frame::new(text).summary().clone()
    }

    #[test]
    fn the_envelopes_four_shapes_are_told_apart_by_the_envelope_alone() {
        let call = summary(r#"{"jsonrpc":"2.0","id":3,"method":"session/prompt","params":{}}"#);
        assert_eq!(call.kind, FrameKind::Call);
        assert_eq!(call.label(), Some("session/prompt"));
        assert_eq!(call.id.as_deref(), Some("3"));

        let notification =
            summary(r#"{"jsonrpc":"2.0","method":"session/update","params":{"x":1}}"#);
        assert_eq!(notification.kind, FrameKind::Notification);
        assert_eq!(notification.label(), Some("session/update"));
        assert_eq!(notification.id, None);

        // An answer carries the id and not the method — correlating the two is
        // the reader's, so the label says which kind of answer it is.
        let result = summary(r#"{"jsonrpc":"2.0","id":3,"result":{"sessionId":"s1"}}"#);
        assert_eq!(result.kind, FrameKind::Result);
        assert_eq!(result.label(), Some("result"));
        assert_eq!(result.id.as_deref(), Some("3"));

        let failed = summary(r#"{"jsonrpc":"2.0","id":3,"error":{"code":-32601}}"#);
        assert_eq!(failed.kind, FrameKind::Error);
        assert_eq!(failed.label(), Some("error"));
    }

    #[test]
    fn an_id_is_rendered_as_it_was_written() {
        // JSON-RPC allows either, and an agent that changed which it sends is
        // showing the reader something rather than nothing.
        assert_eq!(
            summary(r#"{"id":"3","result":{}}"#).id.as_deref(),
            Some("3")
        );
        assert_eq!(summary(r#"{"id":3,"result":{}}"#).id.as_deref(), Some("3"));
    }

    #[test]
    fn a_frame_this_cannot_read_is_still_a_frame() {
        // The rule §8 states, asserted at the one method that could break it:
        // what comes back is *no label*, never a guess and never an error, and
        // the surface goes on drawing the bytes.
        for unreadable in [
            "not json at all",
            "[1,2,3]",
            r#""a bare string""#,
            "{}",
            r#"{"jsonrpc":"2.0"}"#,
        ] {
            let summary = summary(unreadable);
            assert_eq!(
                summary.kind,
                FrameKind::Unreadable,
                "{unreadable} is not one of the four shapes"
            );
            assert_eq!(
                summary.label(),
                None,
                "and it is not labelled: {unreadable}"
            );
        }
    }

    #[test]
    fn a_method_on_an_answer_is_still_the_method() {
        // An agent that puts both on one object has sent something worth
        // seeing. The method is what the row leads with either way, because it
        // is the more specific of the two.
        let both = summary(r#"{"id":1,"method":"session/update","result":{}}"#);
        assert_eq!(both.kind, FrameKind::Call);
        assert_eq!(both.label(), Some("session/update"));
    }
}
