//! The only Value → protocol-type boundary (§8).
//!
//! Canonical numbers belong to the typed copy only. Frame text and values used
//! as raw evidence never pass through here.

use serde::de::DeserializeOwned;
use serde_json::{Number, Value};

fn preserves_number_spelling() -> bool {
    static PRESERVES: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *PRESERVES.get_or_init(|| {
        let spelling = serde_json::from_str::<Value>("0.10").unwrap().to_string();
        spelling != "0.1"
    })
}

/// The envelope's JSON copy. arbitrary_precision loses literal -0 before a
/// Value exists, so preserve its stock floating-point kind at the text boundary.
/// Strings, Frame text, and all other tokens retain their original spelling.
pub(crate) fn frame_value(text: &str) -> serde_json::Result<Value> {
    if !preserves_number_spelling() {
        return serde_json::from_str(text);
    }
    let mut copy = String::with_capacity(text.len());
    let mut quoted = false;
    let mut escaped = false;
    let mut start = 0;
    let bytes = text.as_bytes();
    for (index, &byte) in bytes.iter().enumerate() {
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
        } else if byte == b'"' {
            quoted = true;
        } else if byte == b'-'
            && (index == 0
                || matches!(
                    bytes[index - 1],
                    b':' | b',' | b'[' | b' ' | b'\n' | b'\r' | b'\t'
                ))
            && bytes.get(index + 1) == Some(&b'0')
            && bytes.get(index + 2).is_none_or(|next| {
                matches!(next, b',' | b']' | b'}' | b' ' | b'\n' | b'\r' | b'\t')
            })
        {
            copy.push_str(&text[start..index + 2]);
            copy.push_str(".0");
            start = index + 2;
        }
    }
    copy.push_str(&text[start..]);
    serde_json::from_str(&copy)
}

pub(crate) fn from_value<T: DeserializeOwned>(mut value: Value) -> serde_json::Result<T> {
    canonicalize(&mut value)?;
    serde_json::from_value(value)
}

fn canonicalize(value: &mut Value) -> serde_json::Result<()> {
    match value {
        Value::Number(number) if !number.is_i64() && !number.is_u64() => {
            // Stock already holds the f64. With arbitrary_precision use serde's
            // f64 reader, not Rust's string parser: their rounding can differ.
            let float = if preserves_number_spelling() {
                serde_json::from_str::<f64>(&number.to_string())?
            } else {
                number
                    .as_f64()
                    .expect("stock non-integer numbers are finite f64")
            };
            *number = Number::from_f64(float).ok_or_else(|| {
                <serde_json::Error as serde::de::Error>::custom("number out of range")
            })?;
        }
        Value::Array(values) => {
            for value in values {
                canonicalize(value)?;
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                canonicalize(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::from_value;
    use serde_json::Value;

    #[test]
    fn stock_values_are_identical_after_typed_decoding() {
        // Literal corpus, decoded by the stock reader as the independent
        // oracle. This remains an identity test when the feature is unified:
        // f64 is still read by serde_json's stock numeric parser.
        for spelling in [
            "0",
            "-0",
            "-1",
            "9223372036854775807",
            "-9223372036854775808",
            "18446744073709551615",
            "9007199254740993",
            "0.10",
            "1e2",
            "1.50",
            "4.5e-2",
            "2.5E0",
            "0.50",
            "-0.0",
            "1.2345678901234567",
            "18446744073709551616",
            "-9223372036854775809",
            "1e-300",
            "1e300",
            "1e-0",
            "0.146675314082485333",
            "0.10031360309748868086",
        ] {
            let raw: Value = serde_json::from_str(spelling).unwrap();
            let stock = if spelling == "-0" {
                Value::from(-0.0)
            } else if let Ok(integer) = serde_json::from_str::<u64>(spelling) {
                Value::from(integer)
            } else if let Ok(integer) = serde_json::from_str::<i64>(spelling) {
                Value::from(integer)
            } else {
                Value::from(serde_json::from_str::<f64>(spelling).unwrap())
            };
            let decoded: Value = from_value(super::frame_value(spelling).unwrap()).unwrap();
            assert_eq!(decoded, stock, "{spelling}");
            // Stock stores only machine integers and canonically spelt floats.
            // Comparing strings detects the dependency feature without adding a
            // production feature just to select a test.
            if !super::preserves_number_spelling() {
                assert_eq!(decoded, raw, "stock identity: {spelling}");
            }
        }
    }

    #[test]
    fn nested_values_keep_their_shape_and_only_numbers_are_canonicalized() {
        let raw: Value = serde_json::from_str(
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
