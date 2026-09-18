//! The inspector reports the reader's input; findings are neither instructions
//! nor Conformance Annotations about the Agent.

use schemaform_dioxus::{Localizer, render::MessageDescriptor};

pub struct FindingWords;

impl Localizer for FindingWords {
    fn localize(&self, message: &MessageDescriptor) -> String {
        // The engine bounds retained finding parameters. Never turn an omitted
        // limit or pattern into a claim that the Agent stated JSON null.
        if message
            .parameters
            .get("omitted")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
        {
            return match message.key.as_deref() {
                Some("schemaform.validation.pattern") => {
                    "Does not match the stated pattern; its text exceeds the finding display limit."
                        .into()
                }
                Some(key) if key.starts_with("schemaform.validation.") => {
                    "Outside the stated rule; its parameters exceed the finding display limit."
                        .into()
                }
                _ => message.fallback.clone(),
            };
        }
        let limit = &message.parameters["limit"];
        match message.key.as_deref().unwrap_or("") {
            "schemaform.validation.maximum" => format!("Above the maximum of {limit}."),
            "schemaform.validation.minimum" => format!("Below the minimum of {limit}."),
            "schemaform.validation.exclusiveMaximum" => format!("At or above the exclusive maximum of {limit}."),
            "schemaform.validation.exclusiveMinimum" => format!("At or below the exclusive minimum of {limit}."),
            "schemaform.validation.minLength" => format!("Too few characters; at least {limit} stated."),
            "schemaform.validation.maxLength" => format!("Too many characters; at most {limit} stated."),
            "schemaform.validation.minItems" => format!("Too few choices; at least {limit} stated."),
            "schemaform.validation.maxItems" => format!("Too many choices; at most {limit} stated."),
            "schemaform.validation.required" => "Required, nothing filled in.".into(),
            "schemaform.validation.pattern" => format!("Does not match the stated pattern {}.", message.parameters["pattern"]),
            "schemaform.parse.invalid-number" => "Not a number. This text is not in the form data; Raw can send it as a string.".into(),
            "schemaform.parse.invalid-integer" => "Not a whole number. This text is not in the form data; Raw can send it as a string.".into(),
            "schemaform.parse.resource-limit-exceeded" => "The entered value exceeds the form's size limit. Raw remains available.".into(),
            _ => message.fallback.clone(),
        }
    }
}
