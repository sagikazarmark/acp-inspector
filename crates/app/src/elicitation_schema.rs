//! ACP's flat form profile adapted to a generic Draft 2020-12 engine (§7.8).

use schemaform::{AssertedFormat, Dialect, FormDefinition};
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
            if let Some(format) = property
                .get("format")
                .and_then(Value::as_str)
                .filter(|format| {
                    !CHECKED_FORMATS
                        .iter()
                        .any(|checked| checked.name() == *format)
                })
            {
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
        .assert_formats(CHECKED_FORMATS)
        .compile()
        .map_err(|error| error.to_string());
    Ok(Prepared {
        definition,
        unrendered,
        required_unrendered,
        booleans,
    })
}

/// The string formats ACP's form profile names. A value that does not match
/// is an advisory finding like any other; any other format keeps a note that
/// it is not checked.
const CHECKED_FORMATS: [AssertedFormat; 4] = [
    AssertedFormat::Email,
    AssertedFormat::Uri,
    AssertedFormat::Date,
    AssertedFormat::DateTime,
];

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
    fn the_profile_formats_are_checked_and_any_other_keeps_its_note() {
        let prepared = prepare(
            r#"{"type":"object","properties":{
            "email":{"type":"string","format":"email"},"site":{"type":"string","format":"uri"},
            "day":{"type":"string","format":"date"},"when":{"type":"string","format":"date-time"},
            "id":{"type":"string","format":"uuid"}}}"#,
        )
        .unwrap();
        let definition = prepared.definition.unwrap();
        let codes = |data: serde_json::Value| {
            let mut form = definition.create_form(data).unwrap();
            let submission = form.prepare_advisory_submission();
            submission
                .submission()
                .findings()
                .filter_map(|finding| match finding {
                    schemaform::form::SubmissionBlocker::Validation(finding) => Some((
                        finding.code().to_owned(),
                        finding.parameters()["format"].clone(),
                    )),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert!(
            codes(
                serde_json::json!({"email":"ada@example.com","site":"https://example.com",
                "day":"2026-10-10","when":"2026-10-10T12:00:00Z","id":"not a uuid"})
            )
            .is_empty()
        );
        let mut bad = codes(serde_json::json!({"email":"ada","site":"example.com",
            "day":"10/10/2026","when":"2026-10-10T12:00"}));
        bad.sort_by_key(|(_, format)| format.to_string());
        assert_eq!(
            bad,
            ["date", "date-time", "email", "uri"]
                .map(|format| ("format".to_owned(), serde_json::json!(format)))
        );
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
