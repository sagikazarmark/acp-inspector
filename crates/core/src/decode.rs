//! The only Value → protocol-type boundary (§8).
//!
//! Canonical numbers belong to the typed copy only. Frame text and values used
//! as raw evidence never pass through here.

use serde::de::DeserializeOwned;
use serde_json::{Number, Value};

mod stock_number;

/// The envelope's typed copy, read from raw fragments before feature-sensitive
/// Value or f64 readers can lose number spelling or confuse an object with
/// serde_json's private numeric map. Frame text is never rewritten.
///
/// Two passes, each linear in the text: serde_json validates the whole of it
/// (its own depth limit included), then [`Walk`] builds the value from the
/// validated bytes. Reading each container as a fragment of its own instead
/// re-scanned every subtree once per level above it, so a large, deeply
/// nested frame cost its size times its depth.
pub(crate) fn frame_value(text: &str) -> serde_json::Result<Value> {
    let raw = serde_json::from_str::<&serde_json::value::RawValue>(text)?;
    Walk {
        text: raw.get(),
        at: 0,
    }
    .value(0)
}

pub(crate) fn from_value<T: DeserializeOwned>(value: Value) -> serde_json::Result<T> {
    // Numbers were canonicalized once, at the Frame boundary. Re-rounding the
    // canonical spelling would apply stock's rounding twice.
    serde_json::from_value(value)
}

fn problem(message: &str) -> serde_json::Error {
    <serde_json::Error as serde::de::Error>::custom(message)
}

/// A cursor over text serde_json has already validated as one JSON value.
///
/// Not a validator: it relies on that, and only finds where each token ends.
/// Strings are still decoded by serde_json (escapes are its business) and
/// numbers by the stock policy, so only the structure is read here.
struct Walk<'a> {
    text: &'a str,
    at: usize,
}

impl Walk<'_> {
    /// The next byte that is not whitespace, without consuming it.
    fn peek(&mut self) -> serde_json::Result<u8> {
        while let Some(&byte) = self.text.as_bytes().get(self.at) {
            if !matches!(byte, b' ' | b'\t' | b'\n' | b'\r') {
                return Ok(byte);
            }
            self.at += 1;
        }
        Err(problem("unexpected end of JSON"))
    }

    fn value(&mut self, depth: usize) -> serde_json::Result<Value> {
        match self.peek()? {
            b'{' | b'[' if depth >= 127 => Err(problem("recursion limit exceeded")),
            b'{' => {
                self.at += 1;
                let mut result = serde_json::Map::new();
                if self.peek()? == b'}' {
                    self.at += 1;
                    return Ok(Value::Object(result));
                }
                loop {
                    self.peek()?;
                    let key = self.string()?;
                    if self.peek()? != b':' {
                        return Err(problem("expected `:`"));
                    }
                    self.at += 1;
                    // A repeated key keeps its last value, as serde_json's does.
                    result.insert(key, self.value(depth + 1)?);
                    match self.peek()? {
                        b',' => self.at += 1,
                        b'}' => {
                            self.at += 1;
                            return Ok(Value::Object(result));
                        }
                        _ => return Err(problem("expected `,` or `}`")),
                    }
                }
            }
            b'[' => {
                self.at += 1;
                let mut items = Vec::new();
                if self.peek()? == b']' {
                    self.at += 1;
                    return Ok(Value::Array(items));
                }
                loop {
                    items.push(self.value(depth + 1)?);
                    match self.peek()? {
                        b',' => self.at += 1,
                        b']' => {
                            self.at += 1;
                            return Ok(Value::Array(items));
                        }
                        _ => return Err(problem("expected `,` or `]`")),
                    }
                }
            }
            b'"' => self.string().map(Value::String),
            b'-' | b'0'..=b'9' => {
                let start = self.at;
                while let Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') =
                    self.text.as_bytes().get(self.at)
                {
                    self.at += 1;
                }
                number(&self.text[start..self.at])
            }
            _ => {
                for (literal, value) in [
                    ("true", Value::Bool(true)),
                    ("false", Value::Bool(false)),
                    ("null", Value::Null),
                ] {
                    if self.text[self.at..].starts_with(literal) {
                        self.at += literal.len();
                        return Ok(value);
                    }
                }
                Err(problem("expected a JSON value"))
            }
        }
    }

    /// The string starting at the cursor, decoded by serde_json.
    fn string(&mut self) -> serde_json::Result<String> {
        let bytes = self.text.as_bytes();
        if bytes.get(self.at) != Some(&b'"') {
            return Err(problem("expected a string"));
        }
        let start = self.at;
        let mut at = start + 1;
        loop {
            match bytes.get(at) {
                Some(b'\\') => at += 2,
                Some(b'"') => break,
                Some(_) => at += 1,
                None => return Err(problem("unterminated string")),
            }
        }
        self.at = at + 1;
        serde_json::from_str(&self.text[start..self.at])
    }
}

/// A number token, as stock serde_json reads it whatever features are on.
fn number(text: &str) -> serde_json::Result<Value> {
    if text != "-0" {
        if let Ok(integer) = text.parse::<u64>() {
            return Ok(Value::from(integer));
        }
        if let Ok(integer) = text.parse::<i64>() {
            return Ok(Value::from(integer));
        }
    }
    stock_number::float(text)
        .and_then(Number::from_f64)
        .map(Value::Number)
        .ok_or_else(|| problem("number out of range"))
}

#[cfg(test)]
mod tests {
    use super::from_value;
    use serde_json::Value;

    #[test]
    fn raw_fragment_reading_retains_json_structure_and_stock_depth_limit() {
        let value = super::frame_value(
            r#"{"z":1,"$serde_json::private::Number":"hello","a":[-0,"1e2"],"z":2}"#,
        )
        .unwrap();
        assert_eq!(value["z"], 2);
        assert_eq!(value["$serde_json::private::Number"], "hello");
        assert_eq!(value["a"][0].to_string(), "-0.0");
        assert_eq!(value["a"][1], "1e2");
        for invalid in ["01", "1.", "+1", "1e", "[1,]", "{\"a\":}"] {
            assert!(super::frame_value(invalid).is_err(), "{invalid}");
        }
        let nested = |depth| format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
        assert!(super::frame_value(&nested(127)).is_ok());
        assert!(super::frame_value(&nested(128)).is_err());
    }

    #[test]
    fn stock_values_are_identical_after_typed_decoding() {
        // Captured with serde_json 1.0.151 default features in a standalone
        // workspace. Never derive this oracle with the feature-unified reader
        // under test: float_roundtrip changes that reader too (#8).
        for (spelling, expected) in [
            ("0", "0"),
            ("-0", "-0.0"),
            ("-1", "-1"),
            ("9223372036854775807", "9223372036854775807"),
            ("-9223372036854775808", "-9223372036854775808"),
            ("18446744073709551615", "18446744073709551615"),
            ("9007199254740993", "9007199254740993"),
            ("0.10", "0.1"),
            ("1e2", "100.0"),
            ("1.50", "1.5"),
            ("4.5e-2", "0.045"),
            ("2.5E0", "2.5"),
            ("0.50", "0.5"),
            ("-0.0", "-0.0"),
            ("1.2345678901234567", "1.2345678901234567"),
            ("18446744073709551616", "1.8446744073709552e+19"),
            ("-9223372036854775809", "-9.223372036854776e+18"),
            ("1e-300", "1e-300"),
            ("1e300", "1e+300"),
            ("1e-0", "1.0"),
            ("0.146675314082485333", "0.14667531408248535"),
            ("0.10031360309748868086", "0.10031360309748867"),
            (
                "123456789012345678901234567890.123456789",
                "1.2345678901234568e+29",
            ),
            ("1844674407370955161600.999", "1.8446744073709552e+21"),
            (
                "0.123456789012345678901234567890123456789",
                "0.12345678901234568",
            ),
            (
                "0.0000000000000000000000000000012345678901234567890123456789",
                "1.2345678901234568e-30",
            ),
            ("1e-309", "1e-309"),
            ("5e-324", "5e-324"),
            ("2.2250738585072014e-308", "2.2250738585072014e-308"),
            ("1.7976931348623157e308", "1.7976931348623157e+308"),
            ("1e-9999999999999999999999", "0.0"),
            ("-1e-9999999999999999999999", "-0.0"),
            ("0e9999999999999999999999", "0.0"),
            ("-0e9999999999999999999999", "-0.0"),
            ("1e-2147483648", "0.0"),
        ] {
            let decoded: Value = from_value(super::frame_value(spelling).unwrap()).unwrap();
            assert_eq!(decoded.to_string(), expected, "{spelling}");
        }
    }

    #[test]
    fn numbers_stock_cannot_represent_are_rejected_at_the_typed_boundary() {
        for spelling in [
            "1.7976931348623159e308",
            "1e309",
            "-1e9999999999999999999999",
            "1e2147483647",
        ] {
            assert!(super::frame_value(spelling).is_err(), "{spelling}");
        }
    }

    #[test]
    fn whitespace_escapes_and_deep_nesting_read_as_serde_json_reads_them() {
        let text = " {\r\n \"a\" :\t[ 1 , \"x\\\"y\\\\\" , { \"b\" : null } , [ ] , { } ] ,\
                    \"c\\u0041\" : true , \"\\\\\" : false , \"a\" : \"last\" } ";
        let walked = super::frame_value(text).unwrap();
        let stock: Value = serde_json::from_str(text).unwrap();
        assert_eq!(walked, stock);
        assert_eq!(walked["a"], "last", "a repeated key keeps its last value");
        assert_eq!(walked["cA"], true);

        // A payload under every level: each subtree is read once, not once
        // per level above it.
        let mut nested = String::from("\"payload\"");
        // Two levels each: 120 of the 127 the reader takes.
        for _ in 0..60 {
            nested = format!("{{\"pad\":\"{}\",\"next\":[{nested}]}}", "x".repeat(64));
        }
        let walked = super::frame_value(&nested).unwrap();
        assert_eq!(walked, serde_json::from_str::<Value>(&nested).unwrap());
    }

    #[test]
    fn nested_values_keep_their_shape_and_only_numbers_are_canonicalized() {
        let raw: Value = super::frame_value(
            r#"{"outer":[null,true,"0.10",{"values":[0.10,1e2,1.50,4.5e-2,2.5E0]}],"empty":{}}"#,
        )
        .unwrap();
        let decoded: Value = from_value(raw).unwrap();
        assert_eq!(
            decoded,
            serde_json::json!({
                "outer": [null, true, "0.10", {"values": [0.1, 100.0, 1.5, 0.045, 2.5]}],
                "empty": {}
            })
        );
    }
}
