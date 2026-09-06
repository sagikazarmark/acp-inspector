//! Whether a payload is drawn as it arrived or laid out to be read
//! (`docs/architecture.md` §8, §9).
//!
//! **A reading aid, and only ever that.** A frame is text and stays text
//! ([`Frame`](crate::Frame)): nothing here touches what was recorded, what an
//! export writes or what a copy takes — this answers one question, which is what
//! a surface draws when a reader has asked for the long form. The default is the
//! wire's own bytes, because the raw-first rule (§8) is what this tool is for and
//! a preference is not allowed to be the reason somebody sees something other
//! than what crossed.
//!
//! **Indenting adds whitespace and changes nothing else.** The text is validated
//! as JSON and then *re-spaced*, character by character, rather than parsed into
//! a value and printed back: a round trip through a decoder sorts an object's
//! keys, collapses two fields that share a name, and respells numbers and
//! escapes — every one of which is the inspector quietly editing the evidence. So
//! the bytes a reader sees indented are the bytes that crossed, in the agent's
//! own order and the agent's own spelling, with newlines and spaces between them.
//!
//! **A payload that is not JSON is drawn as it always was.** A frame can be
//! anything — that is the whole point of §8 — so anything this cannot read as one
//! complete JSON document is left exactly alone rather than reported, refused or
//! guessed at.
//!
//! It is in core for the reason [`Appearance`](crate::Appearance) is (§5): what
//! is stored, what an unknown value falls back to and what indenting does to a
//! payload are decisions testable without a window
//! (`crates/core/tests/indentation.rs`), and the desktop crate draws what this answers.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// How many spaces one level of nesting is worth. Two, which is what a reader of
/// JSON expects and what leaves a deeply nested `params` still legible in a
/// column that shares its window with two other screens.
const STEP: &str = "  ";

/// Whether payloads are drawn as they arrived or indented for reading.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Indentation {
    /// Exactly the bytes that crossed — the default, and the answer the whole
    /// tool is built around (§8).
    #[default]
    Wire,
    /// The same bytes with JSON's structure spaced out, where the payload is
    /// JSON.
    Indented,
}

impl Indentation {
    /// Every choice, in the order the control moves through them.
    pub const ALL: [Self; 2] = [Self::Wire, Self::Indented];

    /// The stable token stored on disk.
    pub fn token(self) -> &'static str {
        match self {
            Self::Wire => "wire",
            Self::Indented => "indented",
        }
    }

    /// Parses a token back, falling back to [`Wire`](Self::Wire) for anything
    /// unrecognized.
    ///
    /// A value written by a future version must never leave a reader looking at
    /// a rendering nobody here can describe: the bytes as they crossed are what
    /// every version of this tool can honestly draw.
    pub fn from_token(token: &str) -> Self {
        match token {
            "indented" => Self::Indented,
            _ => Self::Wire,
        }
    }

    /// The switch's own reading: on is indented.
    pub fn of(indented: bool) -> Self {
        if indented { Self::Indented } else { Self::Wire }
    }

    /// Whether the switch is on, for the control that draws it.
    pub fn indented(self) -> bool {
        self == Self::Indented
    }

    /// A payload as this draws it.
    ///
    /// Borrowed and untouched under [`Wire`](Self::Wire), and for anything that
    /// is not one complete JSON document under either: the answer to "this is
    /// not JSON" is the text, because a frame that no schema recognizes is
    /// exactly the frame its reader came to see (§8).
    pub fn draw(self, text: &str) -> Cow<'_, str> {
        match self {
            Self::Wire => Cow::Borrowed(text),
            Self::Indented => indented(text).map_or(Cow::Borrowed(text), Cow::Owned),
        }
    }
}

/// The same JSON with whitespace between its tokens, or `None` where the text is
/// not JSON.
///
/// The validity question is asked first and asked by a parser, so the pass below
/// only ever walks a document that is known to be well-formed — which is what
/// lets it be a re-spacing rather than a decoder. Nothing is built from the
/// parse: it is read and dropped, and every byte that comes out of this function
/// came out of `text`.
fn indented(text: &str) -> Option<String> {
    serde_json::from_str::<serde::de::IgnoredAny>(text).ok()?;
    Some(respaced(text))
}

/// Walks well-formed JSON and writes it back with one token per line.
///
/// Strings are copied through whole — a `{` inside one is a character in a value
/// and not a nesting the layout should answer to — and an escape is copied with
/// the character it escapes, so a `\"` never ends the string it is in.
fn respaced(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    let mut depth = 0usize;
    let mut characters = text.chars().peekable();

    while let Some(character) = characters.next() {
        match character {
            '"' => {
                out.push('"');
                while let Some(inside) = characters.next() {
                    out.push(inside);
                    match inside {
                        '\\' => {
                            if let Some(escaped) = characters.next() {
                                out.push(escaped);
                            }
                        }
                        '"' => break,
                        _ => {}
                    }
                }
            }
            '{' | '[' => {
                out.push(character);
                // An empty collection is closed where it was opened: `{}` says
                // what it is, and two lines with nothing between them say it
                // worse. Closed *here*, so the level it never opened is not one
                // the arm below has to unwind.
                skip_space(&mut characters);
                if matches!(characters.peek(), Some('}' | ']')) {
                    out.push(characters.next().expect("the close just peeked at"));
                    continue;
                }
                depth += 1;
                newline(&mut out, depth);
            }
            '}' | ']' => {
                depth = depth.saturating_sub(1);
                newline(&mut out, depth);
                out.push(character);
            }
            ',' => {
                out.push(',');
                newline(&mut out, depth);
            }
            ':' => out.push_str(": "),
            // Whatever whitespace the sender used is not the sender's message.
            // Numbers, `true`, `false` and `null` are copied character by
            // character, which is what keeps `1.0`, `1e3` and a hundred digits
            // of precision spelled the way they arrived.
            _ if character.is_whitespace() => {}
            _ => out.push(character),
        }
    }

    out
}

fn newline(out: &mut String, depth: usize) {
    out.push('\n');
    for _ in 0..depth {
        out.push_str(STEP);
    }
}

fn skip_space(characters: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while characters.peek().is_some_and(|next| next.is_whitespace()) {
        characters.next();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_payload_that_is_not_json_is_the_payload() {
        // §8, at the one function that could break it. Not an error, not a
        // refusal and not a guess: the text.
        for text in [
            "not json at all",
            "",
            // JSON followed by something else is not one document, and half of
            // it laid out would be the tool inventing a frame.
            r#"{"jsonrpc":"2.0"} trailing"#,
            // Truncated: the frame a transport cut in half is a finding.
            r#"{"jsonrpc":"2.0","#,
        ] {
            assert_eq!(
                Indentation::Indented.draw(text),
                text,
                "{text} is drawn as itself"
            );
        }
    }

    #[test]
    fn indenting_only_moves_the_bytes_apart() {
        // Everything a decode-and-print would change, in one frame: two fields
        // in the agent's order rather than in an alphabet's, a number spelled
        // the way it was sent, and an escape nobody normalised.
        let frame = r#"{"zeta":1,"alpha":{"kept":1.50,"escaped":"a\"b"},"empty":{},"none":[]}"#;

        assert_eq!(
            Indentation::Indented.draw(frame),
            concat!(
                "{\n",
                "  \"zeta\": 1,\n",
                "  \"alpha\": {\n",
                "    \"kept\": 1.50,\n",
                "    \"escaped\": \"a\\\"b\"\n",
                "  },\n",
                "  \"empty\": {},\n",
                "  \"none\": []\n",
                "}"
            )
        );
    }

    #[test]
    fn punctuation_inside_a_string_is_a_character_and_not_a_structure() {
        let frame = r#"{"text":"{\"a\": [1,2]}"}"#;

        assert_eq!(
            Indentation::Indented.draw(frame),
            "{\n  \"text\": \"{\\\"a\\\": [1,2]}\"\n}",
            "a brace in a value never opens a level"
        );
    }

    #[test]
    fn the_wire_is_the_default_and_touches_nothing() {
        let frame = r#"{"jsonrpc":"2.0","id":1}"#;
        assert_eq!(Indentation::default(), Indentation::Wire);
        assert_eq!(Indentation::Wire.draw(frame), frame);
    }
}
