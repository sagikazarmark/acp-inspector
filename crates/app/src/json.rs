//! What a payload is *made of*, said in colour and in nothing else.
//!
//! **It is a lexer and not a parser, and that is the whole design.** The one
//! surface this is for is the Console's frame pane, whose subject is a single
//! frame a reader selected — and the rule that surface is held to is §8's: what
//! is drawn is the bytes, in the order they arrived, with nothing added,
//! reordered, folded or summarised. A parser would have to *succeed* to say
//! anything, so a frame that is not JSON — the most interesting frame an
//! inspector can capture — would either be drawn twice or not at all. A lexer
//! walks whatever it is given, hands back every character it was handed, and
//! colours the ones it recognizes.
//!
//! That is also why there is no dependency here. `serde_json` is in this crate
//! for the elicitation panel, which builds an answer *value*; this builds no
//! value at all, and running the bytes through a data model on the way to a
//! `<pre>` would be the one thing the raw-first rule forbids.
//!
//! **Colour is the only mark.** No line numbers, no fold arrows, no key sorting,
//! no ellipsis: four token classes and the punctuation between them, so a reader
//! can find the field they want at a glance and still be looking at the frame.

/// The classes a token can be drawn in, which are the four things JSON has and
/// the space between them.
///
/// A key is a string in the position a key is in, which is the one thing a
/// lexer has to look ahead for — and the one distinction that earns its keep,
/// because a frame read at a glance is read by its field names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
    Key,
    Text,
    Number,
    Literal,
    Punctuation,
}

impl Token {
    /// What the stylesheet calls it.
    pub fn class(self) -> &'static str {
        match self {
            Self::Key => "json-key",
            Self::Text => "json-string",
            Self::Number => "json-number",
            Self::Literal => "json-literal",
            Self::Punctuation => "json-punctuation",
        }
    }
}

/// The payload, split into what it is made of.
///
/// **Every character comes back**, in order: concatenating the pieces is the
/// text that went in, which is the assertion the test at the bottom of this file
/// makes and the reason this can be pointed at a frame nobody can parse.
pub fn tokens(payload: &str) -> Vec<(Token, String)> {
    let mut tokens: Vec<(Token, String)> = Vec::new();
    let characters: Vec<char> = payload.chars().collect();
    let mut at = 0;

    while at < characters.len() {
        let character = characters[at];
        let (token, length) = match character {
            '"' => {
                let length = string_length(&characters[at..]);
                // A string is a key when the next thing that is not whitespace
                // is the colon that would separate it from its value.
                let after = at + length;
                let key = characters[after..]
                    .iter()
                    .find(|character| !character.is_whitespace())
                    == Some(&':');
                (if key { Token::Key } else { Token::Text }, length)
            }
            '-' | '0'..='9' => {
                let length = characters[at..]
                    .iter()
                    .take_while(|character| {
                        matches!(character, '-' | '+' | '.' | 'e' | 'E' | '0'..='9')
                    })
                    .count();
                (Token::Number, length)
            }
            't' | 'f' | 'n' => {
                let length = characters[at..]
                    .iter()
                    .take_while(|character| character.is_ascii_alphabetic())
                    .count();
                (Token::Literal, length)
            }
            _ => {
                // Everything else — the braces, the commas, the indentation, and
                // whatever a frame that is not JSON at all is made of — is the
                // space between the tokens.
                let length = characters[at..]
                    .iter()
                    .take_while(|character| {
                        !matches!(character, '"' | '-' | '0'..='9' | 't' | 'f' | 'n')
                    })
                    .count()
                    .max(1);
                (Token::Punctuation, length)
            }
        };

        let text: String = characters[at..at + length].iter().collect();
        // Runs of the same class are one span rather than one per character,
        // which is what keeps a ten-kilobyte frame from becoming ten thousand
        // elements.
        match tokens.last_mut() {
            Some((last, held)) if *last == token && token == Token::Punctuation => {
                held.push_str(&text);
            }
            _ => tokens.push((token, text)),
        }
        at += length;
    }

    tokens
}

/// How many characters the string starting at `characters[0]` occupies,
/// including both quotes — or the rest of the input, where nothing closes it.
///
/// An unterminated string is a frame worth seeing rather than a reason to
/// panic: it is exactly the sort of thing the tool exists to catch an agent
/// doing.
fn string_length(characters: &[char]) -> usize {
    let mut at = 1;
    while at < characters.len() {
        match characters[at] {
            '\\' => at += 2,
            '"' => return (at + 1).min(characters.len()),
            _ => at += 1,
        }
    }

    characters.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drawn(payload: &str) -> String {
        tokens(payload)
            .into_iter()
            .map(|(_, text)| text)
            .collect::<String>()
    }

    fn classed(payload: &str, token: Token) -> Vec<String> {
        tokens(payload)
            .into_iter()
            .filter(|(kind, _)| *kind == token)
            .map(|(_, text)| text)
            .collect()
    }

    #[test]
    fn every_character_of_the_payload_comes_back() {
        for payload in [
            r#"{"jsonrpc":"2.0","id":1,"method":"session/prompt"}"#,
            "{\n  \"a\": [1, -2.5e3, true, null],\n  \"b\": {}\n}",
            // The frames this window exists for: the ones nobody can parse.
            "not json at all",
            r#"{"unterminated": "string"#,
            "",
        ] {
            assert_eq!(drawn(payload), payload, "the lexer hands back what it read");
        }
    }

    #[test]
    fn a_field_name_is_told_from_the_text_beside_it() {
        let payload = r#"{"method": "session/prompt", "id": 7}"#;
        assert_eq!(
            classed(payload, Token::Key),
            vec![r#""method""#, r#""id""#],
            "a string in a key's position is a key"
        );
        assert_eq!(
            classed(payload, Token::Text),
            vec![r#""session/prompt""#],
            "and one in a value's position is not"
        );
        assert_eq!(classed(payload, Token::Number), vec!["7"]);
    }

    #[test]
    fn a_key_is_still_a_key_when_the_payload_is_laid_out() {
        // The pane draws indented payloads, so the colon a key is found by is
        // routinely a space and a newline away from it.
        let payload = "{\n  \"session\"  :  \"s-1\"\n}";
        assert_eq!(classed(payload, Token::Key), vec![r#""session""#]);
        assert_eq!(classed(payload, Token::Text), vec![r#""s-1""#]);
    }

    #[test]
    fn the_three_literals_are_one_class_and_a_quoted_one_is_not() {
        let payload = r#"[true, false, null, "true"]"#;
        assert_eq!(
            classed(payload, Token::Literal),
            vec!["true", "false", "null"]
        );
        assert_eq!(classed(payload, Token::Text), vec![r#""true""#]);
    }

    #[test]
    fn an_escaped_quote_does_not_end_the_string() {
        let payload = r#"{"said": "he said \"no\"", "then": 1}"#;
        assert_eq!(
            classed(payload, Token::Text),
            vec![r#""he said \"no\"""#],
            "the escape is part of the string it is in"
        );
        assert_eq!(classed(payload, Token::Key), vec![r#""said""#, r#""then""#]);
    }
}
