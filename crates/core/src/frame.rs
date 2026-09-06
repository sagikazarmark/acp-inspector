//! The unit of the wire: one JSON-RPC message, verbatim, and which way it went.

use std::fmt;
use std::sync::Arc;

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
#[derive(Clone, PartialEq, Eq)]
pub struct Frame(Arc<str>);

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
        Self(text.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
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
    pub fn summary(&self) -> FrameSummary {
        let Ok(serde_json::Value::Object(envelope)) =
            serde_json::from_str::<serde_json::Value>(&self.0)
        else {
            return FrameSummary::unreadable();
        };

        // The id as it was written. JSON-RPC allows a string or a number and
        // this is a label, so it is rendered rather than typed: an agent that
        // sent `"3"` where it sent `3` last time is a difference worth seeing
        // on the row rather than one normalised away by the tool reporting it.
        let id = match envelope.get("id") {
            Some(serde_json::Value::String(id)) => Some(id.clone()),
            Some(serde_json::Value::Number(id)) => Some(id.to_string()),
            _ => None,
        };
        let method = match envelope.get("method") {
            Some(serde_json::Value::String(method)) => Some(method.clone()),
            _ => None,
        };

        let kind = match (&method, envelope.contains_key("error"), &id) {
            (Some(_), _, Some(_)) => FrameKind::Call,
            (Some(_), _, None) => FrameKind::Notification,
            (None, true, _) => FrameKind::Error,
            (None, false, _) if envelope.contains_key("result") => FrameKind::Result,
            _ => FrameKind::Unreadable,
        };

        FrameSummary { kind, method, id }
    }

    /// Whether this frame can cross a message-per-line transport as the single
    /// message it claims to be.
    ///
    /// Only `\n` is asked about: it is the framing character, and a frame that
    /// contains one would arrive at the agent as two messages while the trace
    /// showed one — the inspector lying about the wire, which is the one thing
    /// it may never do.
    pub(crate) fn is_one_message(&self) -> bool {
        !self.0.contains('\n')
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
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Frame {
    /// The frame's text, not `Frame("…")` around it: every place this shows up —
    /// a failed assertion, a log line — wants the JSON that was on the wire.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(text: &str) -> FrameSummary {
        Frame::new(text).summary()
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
