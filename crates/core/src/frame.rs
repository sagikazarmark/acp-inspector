//! The unit of the wire: one JSON-RPC message, verbatim, and which way it went.

use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
use std::fmt;
use std::sync::{Arc, OnceLock};
use std::time::{Instant, SystemTime};

/// One JSON-RPC message as it crossed the wire (`CONTEXT.md`, *Frame*).
///
/// Text, not a parsed value: carried and recorded exactly as it arrived, even
/// when it is not JSON (§8). Cheap to clone.
#[derive(Clone)]
pub struct Frame(Arc<Contents>);

struct Contents {
    text: Arc<str>,
    summary: OnceLock<FrameSummary>,
    origin: Origin,
    /// When a transport read it, for frames that came off a pipe.
    read: Option<(SystemTime, Instant)>,
    /// Composed by a person rather than by the typed layer.
    hand: bool,
}

/// How the text relates to the bytes that crossed.
enum Origin {
    Verbatim,
    /// Not valid UTF-8: the text is a lossy decoding of these bytes.
    Lossy(Box<[u8]>),
    /// Too long to carry: the text is a lossy decoding of the retained prefix.
    Omitted(Omission),
}

/// What a placeholder stands in for: a line the transport read but dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Omission {
    /// The line's length in bytes, line ending excluded.
    pub length: usize,
    /// The transport's limit it was over.
    pub limit: usize,
}

impl PartialEq for Frame {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.as_str() == other.as_str()
    }
}
impl Eq for Frame {}

/// Which way a frame went, relative to the inspector, which is always the
/// client.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    ToAgent,
    FromAgent,
}

impl Direction {
    pub(crate) fn opposite(self) -> Self {
        match self {
            Self::ToAgent => Self::FromAgent,
            Self::FromAgent => Self::ToAgent,
        }
    }
}

impl Frame {
    pub fn new(text: impl Into<Arc<str>>) -> Self {
        Self::build(text.into(), Origin::Verbatim, None)
    }

    /// A frame a person wrote and sent by hand, carried exactly as given and
    /// marked by [`is_hand_sent`](Self::is_hand_sent).
    pub fn by_hand(text: impl Into<Arc<str>>) -> Self {
        Self(Arc::new(Contents {
            hand: true,
            ..Self::contents(text.into(), Origin::Verbatim, None)
        }))
    }

    /// Whether a person composed this frame by hand. Its answer drives none of
    /// the typed stores and no conformance rule is judged on it.
    pub fn is_hand_sent(&self) -> bool {
        self.0.hand
    }

    /// The bytes of a frame read back out of an export: decoded lossily where
    /// they are not UTF-8, as they were when they crossed, and stamped with
    /// nothing.
    pub(crate) fn rebuilt(bytes: Vec<u8>, hand: bool) -> Self {
        let (text, origin) = match String::from_utf8(bytes) {
            Ok(text) => (text, Origin::Verbatim),
            Err(error) => {
                let bytes = error.into_bytes();
                let text = String::from_utf8_lossy(&bytes).into_owned();
                (text, Origin::Lossy(bytes.into()))
            }
        };
        Self(Arc::new(Contents {
            hand,
            ..Self::contents(text.into(), origin, None)
        }))
    }

    /// A placeholder read back out of an export, which says only how long the
    /// dropped line was.
    pub(crate) fn placeholder(omission: Omission) -> Self {
        Self::build("".into(), Origin::Omitted(omission), None)
    }

    /// A line a transport has just read, stamped now. Bytes that are not UTF-8
    /// are decoded lossily and kept, so the frame is recorded rather than lost.
    pub fn read(bytes: Vec<u8>) -> Self {
        let read = Some((SystemTime::now(), Instant::now()));
        match String::from_utf8(bytes) {
            Ok(text) => Self::build(text.into(), Origin::Verbatim, read),
            Err(error) => {
                let bytes = error.into_bytes();
                let text = String::from_utf8_lossy(&bytes).into_owned();
                Self::build(text.into(), Origin::Lossy(bytes.into()), read)
            }
        }
    }

    /// A placeholder for a line a transport read but could not carry. `prefix`
    /// is what it kept of the line's start.
    pub fn omitted(prefix: &[u8], omission: Omission) -> Self {
        let text = String::from_utf8_lossy(prefix).into_owned();
        let read = Some((SystemTime::now(), Instant::now()));
        Self::build(text.into(), Origin::Omitted(omission), read)
    }

    fn build(text: Arc<str>, origin: Origin, read: Option<(SystemTime, Instant)>) -> Self {
        Self(Arc::new(Self::contents(text, origin, read)))
    }

    fn contents(text: Arc<str>, origin: Origin, read: Option<(SystemTime, Instant)>) -> Contents {
        Contents {
            text,
            summary: OnceLock::new(),
            origin,
            read,
            hand: false,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0.text
    }

    /// The bytes that crossed: the text's own, unless the frame
    /// [is lossy](Self::is_lossy).
    pub fn bytes(&self) -> &[u8] {
        match &self.0.origin {
            Origin::Lossy(bytes) => bytes,
            _ => self.0.text.as_bytes(),
        }
    }

    /// Whether the text is a lossy decoding of bytes that were not UTF-8.
    pub fn is_lossy(&self) -> bool {
        matches!(self.0.origin, Origin::Lossy(_))
    }

    /// Set when this is a placeholder for a frame the transport dropped; the
    /// text is then the start of that frame, not the frame.
    pub fn omission(&self) -> Option<Omission> {
        match self.0.origin {
            Origin::Omitted(omission) => Some(omission),
            _ => None,
        }
    }

    /// When a transport read this frame, for one that came off a pipe.
    pub fn read_at(&self) -> Option<SystemTime> {
        self.0.read.map(|(at, _)| at)
    }

    pub(crate) fn read_instant(&self) -> Option<Instant> {
        self.0.read.map(|(_, at)| at)
    }

    /// What the frame says it is, read off the JSON-RPC envelope alone
    /// (`method`, `id`, `result`, `error`); payloads are never decoded. Computed
    /// once and shared by clones.
    ///
    /// Anything that is not an envelope object is [`FrameKind::Unreadable`].
    pub fn summary(&self) -> &FrameSummary {
        self.0.summary.get_or_init(|| {
            if self.omission().is_some() {
                return FrameSummary::unreadable();
            }
            serde_json::from_str(self.as_str()).unwrap_or_else(|_| FrameSummary::unreadable())
        })
    }

    /// Whether this frame can cross a message-per-line transport as one
    /// message: it contains no `\n`, which would split it on the wire while the
    /// trace showed one.
    pub(crate) fn is_one_message(&self) -> bool {
        !self.as_str().contains('\n')
    }
}

/// Which of the JSON-RPC envelope's four shapes a frame is in.
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
    /// Not a JSON object, or a JSON object in none of those four shapes.
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
    /// The id as a match key, keeping its JSON type.
    pub(crate) key: Option<IdKey>,
}

/// An id as JSON-RPC matches it, and as a careless agent might.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IdKey {
    /// The id's JSON: `3` and `"3"` differ, as JSON-RPC says they do.
    pub(crate) exact: String,
    /// The integer it means whatever its type, where it means one.
    pub(crate) loose: Option<String>,
}

impl IdKey {
    fn of(raw: &str, value: &serde_json::Value) -> Option<Self> {
        match value {
            serde_json::Value::String(text) => Some(Self {
                exact: serde_json::to_string(text).ok()?,
                loose: integral(text).map(|id| id.to_string()),
            }),
            serde_json::Value::Number(_) => Some(Self {
                exact: raw.trim().to_owned(),
                loose: integral(raw).map(|id| id.to_string()),
            }),
            _ => None,
        }
    }
}

/// The integer a number's spelling means, if it means one: `3`, `3.0`, `"3"`.
pub(crate) fn integral(text: &str) -> Option<i64> {
    let text = text.trim();
    text.parse::<i64>().ok().or_else(|| {
        let float = text.parse::<f64>().ok()?;
        (float.fract() == 0.0 && float.abs() < 9.0e15).then_some(float as i64)
    })
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
                let mut id_key = None;
                while let Some(key) = map.next_key::<String>()? {
                    let raw = map.next_value::<&serde_json::value::RawValue>()?;
                    match key.as_str() {
                        "method" => method = serde_json::from_str::<String>(raw.get()).ok(),
                        "id" => {
                            // Use the existing Value number semantics for this label only;
                            // never convert IDs through f64 ourselves or decode payloads.
                            let value = serde_json::from_str::<serde_json::Value>(raw.get())
                                .map_err(serde::de::Error::custom)?;
                            id_key = IdKey::of(raw.get(), &value);
                            id = match value {
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
                Ok(FrameSummary {
                    kind,
                    method,
                    id,
                    key: id_key,
                })
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
            key: None,
        }
    }

    /// The one word a row leads with: the method where there is one, otherwise
    /// `result` or `error`; `None` for an unreadable frame.
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
    /// The frame's text as a string literal, without a `Frame(…)` wrapper.
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

        // An answer carries no method, so the label is the kind of answer.
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
        assert_eq!(
            summary(r#"{"id":"3","result":{}}"#).id.as_deref(),
            Some("3")
        );
        assert_eq!(summary(r#"{"id":3,"result":{}}"#).id.as_deref(), Some("3"));
    }

    #[test]
    fn a_frame_this_cannot_read_is_still_a_frame() {
        // No label, never a guess or an error (§8).
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
        // The method is the more specific of the two, so the row leads with it.
        let both = summary(r#"{"id":1,"method":"session/update","result":{}}"#);
        assert_eq!(both.kind, FrameKind::Call);
        assert_eq!(both.label(), Some("session/update"));
    }
}
