//! Whether a payload is drawn as it arrived or laid out to be read
//! (`docs/architecture.md` §8).
//!
//! A display preference only: recorded frames, exports and copies always carry
//! the wire bytes. Indenting validates the text as JSON and then re-spaces it
//! character by character instead of decode-and-print, so key order, duplicate
//! keys, number spelling and escapes are preserved. Text that is not one
//! complete JSON document is drawn unchanged.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// One level of nesting.
const STEP: &str = "  ";

/// Whether payloads are drawn as they arrived or indented for reading.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Indentation {
    /// Exactly the bytes that crossed (the default).
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
    /// unrecognized (e.g. a value from a future version).
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

    /// A payload as this draws it: borrowed and untouched under
    /// [`Wire`](Self::Wire), or when the text is not one complete JSON document.
    pub fn draw(self, text: &str) -> Cow<'_, str> {
        match self {
            Self::Wire => Cow::Borrowed(text),
            Self::Indented => indented(text).map_or(Cow::Borrowed(text), Cow::Owned),
        }
    }
}

/// The same JSON with whitespace between its tokens, or `None` where the text is
/// not JSON. Validated by a parser first, so the re-spacing pass only walks
/// well-formed input; every output byte comes from `text`.
fn indented(text: &str) -> Option<String> {
    serde_json::from_str::<serde::de::IgnoredAny>(text).ok()?;
    Some(respaced(text))
}

/// Walks well-formed JSON and writes it back with one token per line.
///
/// Strings are copied whole (punctuation inside them is not structure), and an
/// escape is copied with the character it escapes, so `\"` never ends a string.
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
                // An empty collection stays `{}` on one line, closed here so the
                // closing arm never unwinds a level that was not opened.
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
            // The sender's whitespace is replaced. Literals are copied
            // character by character, so `1.0` and `1e3` keep their spelling.
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
        for text in [
            "not json at all",
            "",
            // JSON followed by something else is not one document.
            r#"{"jsonrpc":"2.0"} trailing"#,
            // Truncated.
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
        // Key order, number spelling and escapes a decode-and-print would change.
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
