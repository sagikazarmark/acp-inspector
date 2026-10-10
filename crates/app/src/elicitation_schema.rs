//! ACP's flat form profile adapted to a generic Draft 2020-12 engine (§7.8).

use schemaform::{Dialect, FormDefinition};
use serde_json::{Map, Value};

use std::collections::BTreeSet;

pub struct Prepared {
    pub definition: Result<FormDefinition, String>,
    pub unrendered: Map<String, Value>,
    /// The unrendered properties the agent listed as required. They are taken
    /// out of the engine's `required` so it does not report what it cannot
    /// draw, and the host still marks them.
    pub required_unrendered: BTreeSet<String>,
    /// Rendered boolean properties, so a finding about one can say that an
    /// unchecked box is "not set" rather than `false`.
    pub booleans: BTreeSet<String>,
}

pub fn prepare(raw: &str) -> Result<Prepared, String> {
    let mut schema = Value::Object(raw_content(raw)?);
    // ACP's requested schema is always an object. An agent that leaves the
    // root `type` out still means one, and the engine needs it said.
    if schema.get("type").is_none() {
        schema["type"] = Value::String("object".into());
    }
    if schema.get("properties").is_none() {
        schema["properties"] = Value::Object(Map::new());
    }
    let mut unrendered = Map::new();
    let mut booleans = BTreeSet::new();
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
        for (name, property) in properties.iter_mut() {
            if property["type"] == "boolean" {
                booleans.insert(name.clone());
            }
            if let Some(format) = property.get("format").and_then(Value::as_str) {
                let note = format_note(format);
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
    let mut required_unrendered = BTreeSet::new();
    if let Some(required) = schema.get_mut("required").and_then(Value::as_array_mut) {
        required.retain(|name| match name.as_str() {
            Some(name) if unrendered.contains_key(name) => {
                required_unrendered.insert(name.to_owned());
                false
            }
            _ => true,
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
        required_unrendered,
        booleans,
    })
}

/// The note added to a property's description for a `format` the form does
/// not check. `date-time` says more: its picker yields a local time with no
/// zone, which is not RFC 3339 (sagikazarmark/schemaform#61), so Raw is the
/// way to send a conformant value.
fn format_note(format: &str) -> String {
    match format {
        "date-time" => "Format date-time: the picker gives a local time with no time zone, which is not RFC 3339. Use Raw to send a conformant value, such as 2026-01-31T09:30:00Z.".to_owned(),
        _ => format!("Format {format}: not checked here."),
    }
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
    fn a_schema_without_a_root_type_is_still_an_object_form() {
        // `{}` and a schema that lists properties but no `type` are both an
        // object by ACP's definition; neither may lose its form.
        let mut empty = prepare("{}")
            .unwrap()
            .definition
            .expect("an empty schema is an empty form")
            .create_form_with_defaults(json!({}))
            .unwrap();
        assert_eq!(
            empty.prepare_advisory_submission().submission().form_data(),
            &json!({})
        );

        let prepared = prepare(
            r#"{"properties":{"name":{"type":"string","default":"Ada"}},"required":["name"]}"#,
        )
        .unwrap();
        assert!(prepared.unrendered.is_empty());
        let mut form = prepared
            .definition
            .expect("properties without a root type still compile")
            .create_form_with_defaults(json!({}))
            .unwrap();
        assert_eq!(
            form.prepare_advisory_submission().submission().form_data(),
            &json!({"name":"Ada"})
        );

        // A root `type` the agent did state is left alone.
        assert!(prepare(r#"{"type":"string"}"#).unwrap().definition.is_err());
    }

    #[test]
    fn unrendered_required_properties_and_booleans_are_remembered() {
        let prepared = prepare(
            r#"{"type":"object","properties":{
            "future":{"type":"future"},"later":{"type":"later"},"ok":{"type":"boolean"}},
            "required":["future","ok"]}"#,
        )
        .unwrap();
        assert_eq!(
            prepared.required_unrendered.iter().collect::<Vec<_>>(),
            ["future"]
        );
        assert_eq!(prepared.booleans.iter().collect::<Vec<_>>(), ["ok"]);
    }

    #[test]
    fn a_date_time_note_says_the_picker_is_local_and_raw_is_conformant() {
        assert!(format_note("date-time").contains("no time zone"));
        assert!(format_note("date-time").contains("Use Raw"));
        assert_eq!(format_note("email"), "Format email: not checked here.");
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
