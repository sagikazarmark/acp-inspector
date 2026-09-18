//! ACP's flat form profile adapted to a generic Draft 2020-12 engine (§7.8).

use schemaform::{Dialect, FormDefinition};
use serde_json::{Map, Value};

pub struct Prepared {
    pub definition: Result<FormDefinition, String>,
    pub unrendered: Map<String, Value>,
}

pub fn prepare(raw: &str) -> Result<Prepared, String> {
    let mut schema = Value::Object(raw_content(raw)?);
    if schema.get("properties").is_none() {
        schema["properties"] = Value::Object(Map::new());
    }
    let mut unrendered = Map::new();
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        properties.retain(|name, property| {
            let known = property
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| {
                    matches!(kind, "string" | "number" | "integer" | "boolean")
                        || kind == "array"
                            && (property["items"]["type"] == "string"
                                || property["items"].get("type").is_none()
                                    && property["items"]["anyOf"].is_array())
                });
            if !known {
                unrendered.insert(name.clone(), property.clone());
            }
            known
        });
        for property in properties.values_mut() {
            if let Some(format) = property.get("format").and_then(Value::as_str) {
                let note = format!("Format {format}: not checked here.");
                let description = property
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                property["description"] = Value::String(if description.is_empty() {
                    note
                } else {
                    format!("{description}\n{note}")
                });
            }
            if property["type"] == "array" {
                property["uniqueItems"] = Value::Bool(true);
            }
        }
    }
    if let Some(required) = schema.get_mut("required").and_then(Value::as_array_mut) {
        required.retain(|name| {
            !name
                .as_str()
                .is_some_and(|name| unrendered.contains_key(name))
        });
    }
    // 0.5 treats open objects as informational, so preserve the Agent's root
    // policy instead of closing it solely to suppress an advisory.
    let definition = FormDefinition::compiler(schema)
        .default_dialect(Dialect::Draft202012)
        .compile()
        .map_err(|error| error.to_string());
    Ok(Prepared {
        definition,
        unrendered,
    })
}

/// Raw answers can contain any object, including names serde_json uses for its
/// internal exact-number representation. Read raw fragments so those names
/// remain keys and literal -0 retains its sign under arbitrary_precision.
pub fn raw_content(text: &str) -> Result<Map<String, Value>, String> {
    let raw = serde_json::from_str::<&serde_json::value::RawValue>(text)
        .map_err(|error| error.to_string())?;
    match literal(raw, 0)? {
        Value::Object(object) => Ok(object),
        _ => Err("`content` is an object or nothing — that is the shape the method has.".into()),
    }
}

fn literal(raw: &serde_json::value::RawValue, depth: usize) -> Result<Value, String> {
    if depth >= 128 {
        return Err("JSON nesting limit exceeded".into());
    }
    let text = raw.get();
    match text.as_bytes()[0] {
        b'{' => {
            let object: std::collections::BTreeMap<String, Box<serde_json::value::RawValue>> =
                serde_json::from_str(text).map_err(|error| error.to_string())?;
            object
                .into_iter()
                .map(|(key, value)| Ok((key, literal(&value, depth + 1)?)))
                .collect::<Result<Map<_, _>, String>>()
                .map(Value::Object)
        }
        b'[' => {
            let items: Vec<&serde_json::value::RawValue> =
                serde_json::from_str(text).map_err(|error| error.to_string())?;
            items
                .into_iter()
                .map(|item| literal(item, depth + 1))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array)
        }
        b'-' if text == "-0" => Ok(Value::from(-0.0)),
        _ => serde_json::from_str(text).map_err(|error| error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_forms_accept_and_unknown_array_items_do_not_poison_siblings() {
        let mut empty = prepare(r#"{"type":"object"}"#)
            .unwrap()
            .definition
            .unwrap()
            .create_form_with_defaults(json!({}))
            .unwrap();
        assert_eq!(
            empty.prepare_advisory_submission().submission().form_data(),
            &json!({})
        );
        let prepared = prepare(r#"{"type":"object","properties":{"name":{"type":"string","default":"Ada"},"future":{"type":"array","items":{"type":"number"}}},"required":["future"]}"#).unwrap();
        assert!(prepared.unrendered.contains_key("future"));
        assert!(prepared.definition.is_ok());
        let broken = prepare(r#"{"type":"object","properties":{"bad":{"type":"string","pattern":"["},"future":{"type":"future"}}}"#).unwrap();
        assert!(broken.definition.is_err());
        assert!(broken.unrendered.contains_key("future"));
    }

    #[test]
    fn raw_answers_preserve_private_marker_objects_and_numeric_tokens() {
        let raw = raw_content(
            r#"{"object":{"$serde_json::private::Number":"hello"},"negative":-0,"literal":1.50}"#,
        )
        .unwrap();
        assert_eq!(raw["object"]["$serde_json::private::Number"], "hello");
        assert_eq!(raw["negative"].to_string(), "-0.0");
        assert_eq!(raw["literal"].to_string(), "1.50");
    }

    #[test]
    fn unknown_properties_stay_raw_while_scalar_and_multiple_choice_defaults_seed() {
        let prepared = prepare(
            r#"{"type":"object","properties":{
            "amount":{"type":"number","default":1.50,"maximum":1e2},
            "tags":{"type":"array","items":{"type":"string","enum":["a","b"]},"default":["b"]},
            "future":{"type":"future"}},"required":["amount","future"]}"#,
        )
        .unwrap();
        assert_eq!(prepared.unrendered["future"], json!({"type":"future"}));
        let mut form = prepared
            .definition
            .unwrap()
            .create_form_with_defaults(json!({}))
            .unwrap();
        let (_, submission) = form.prepare_advisory_submission().into_parts();
        assert_eq!(submission.form_data()["amount"].to_string(), "1.50");
        assert_eq!(submission.form_data()["tags"], json!(["b"]));
        assert_eq!(submission.findings().count(), 0);
    }

    #[test]
    fn advisory_submission_carries_three_broken_rules_and_the_entered_data() {
        let prepared = prepare(
            r#"{"type":"object","properties":{
            "age":{"type":"integer","maximum":120},
            "confidence":{"type":"number","maximum":1},
            "name":{"type":"string"}},"required":["name"]}"#,
        )
        .unwrap();
        let mut form = prepared
            .definition
            .unwrap()
            .create_form(json!({"age":999,"confidence":2}))
            .unwrap();
        let (_, submission) = form.prepare_advisory_submission().into_parts();
        assert_eq!(submission.form_data(), &json!({"age":999,"confidence":2}));
        assert_eq!(submission.findings().count(), 3);
    }
}
