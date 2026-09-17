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
pub(crate) fn frame_value(text: &str) -> serde_json::Result<Value> {
    let raw = serde_json::from_str::<&serde_json::value::RawValue>(text)?;
    value(raw, 0)
}

pub(crate) fn from_value<T: DeserializeOwned>(value: Value) -> serde_json::Result<T> {
    // Numbers were canonicalized once, at the Frame boundary. Re-rounding the
    // canonical spelling would apply stock's rounding twice.
    serde_json::from_value(value)
}

fn problem(message: &str) -> serde_json::Error {
    <serde_json::Error as serde::de::Error>::custom(message)
}

fn value(raw: &serde_json::value::RawValue, depth: usize) -> serde_json::Result<Value> {
    let text = raw.get();
    match text.as_bytes()[0] {
        b'{' | b'[' if depth >= 127 => Err(problem("recursion limit exceeded")),
        b'{' => {
            let object: RawObject = serde_json::from_str(text)?;
            let mut result = serde_json::Map::new();
            for (key, raw) in object.0 {
                result.insert(key, value(&raw, depth + 1)?);
            }
            Ok(Value::Object(result))
        }
        b'[' => {
            let items: Vec<&serde_json::value::RawValue> = serde_json::from_str(text)?;
            items
                .into_iter()
                .map(|raw| value(raw, depth + 1))
                .collect::<Result<_, _>>()
                .map(Value::Array)
        }
        b'-' | b'0'..=b'9' => {
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
        // Only strings, booleans and null reach the ordinary Value reader.
        _ => serde_json::from_str(text),
    }
}

/// Preserve input member order without Value's private-number-map recognition.
struct RawObject(Vec<(String, Box<serde_json::value::RawValue>)>);

impl<'de> serde::Deserialize<'de> for RawObject {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectVisitor;
        impl<'de> serde::de::Visitor<'de> for ObjectVisitor {
            type Value = RawObject;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a JSON object")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut members = Vec::new();
                while let Some(member) = map.next_entry()? {
                    members.push(member);
                }
                Ok(RawObject(members))
            }
        }
        deserializer.deserialize_map(ObjectVisitor)
    }
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
