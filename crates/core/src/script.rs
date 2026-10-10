//! Scripted answers for client services (`docs/client-services.md`): a rules
//! file chosen at launch that answers the agent's `fs/*`, `terminal/*` and
//! other unserviced requests with canned results or errors.
//!
//! **A mock, not mediation.** A rule hands back the JSON it was written with;
//! nothing here reads a file, writes one, or runs a command (F16). The first
//! rule that matches answers, and what no rule matches falls back to the
//! launch settings or to method-not-found, as the file says.

use std::fmt;

use agent_client_protocol_schema::v1;
use serde_json::{Map, Value};

/// A parsed rules file: the rules in order, and what happens to a request
/// none of them matches.
#[derive(Clone, Debug, PartialEq)]
pub struct ServiceScript {
    source: String,
    rules: Vec<Rule>,
    unmatched: Unmatched,
}

/// What a request no rule matches is answered with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Unmatched {
    /// What the launch settings would do without a rules file: a card to answer
    /// by hand where the service was claimed or the reader opted in, and
    /// method-not-found otherwise.
    #[default]
    Manual,
    /// Method-not-found, whatever was claimed.
    MethodNotFound,
}

/// One rule: which requests it matches, and the answer it gives them.
#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    /// The method, exactly or as a `*` pattern (`terminal/*`).
    pub method: String,
    /// The `path` param, exactly or as a `*` pattern. `None` matches any.
    pub path: Option<String>,
    /// The leading words of the command line `terminal/create` asks for
    /// (`command` then `args`). `None` matches any.
    pub command: Option<String>,
    pub answer: Scripted,
}

/// The canned answer a rule gives.
#[derive(Clone, Debug, PartialEq)]
pub enum Scripted {
    /// A JSON-RPC result, sent as written.
    Result(Value),
    /// A JSON-RPC error.
    Error(v1::Error),
}

/// Why a rules file could not be used, in a sentence for the reader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptError(String);

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ScriptError {}

fn wrong(message: impl Into<String>) -> ScriptError {
    ScriptError(message.into())
}

impl ServiceScript {
    /// Reads a rules file. `source` is what the file is called on screen.
    ///
    /// Strict about shape, so a typo is reported rather than turning a rule
    /// into one that never matches: unknown keys, a rule with both or neither
    /// of `result` and `error`, and an error without an integer code are all
    /// refused with the rule's number.
    pub fn parse(source: impl Into<String>, text: &str) -> Result<Self, ScriptError> {
        let value: Value = crate::decode::frame_value(text)
            .map_err(|error| wrong(format!("Not JSON: {error}")))?;
        let Value::Object(file) = value else {
            return Err(wrong(
                "The file must be a JSON object with a \"rules\" list.",
            ));
        };
        for key in file.keys() {
            if !matches!(
                key.as_str(),
                "rules" | "unmatched" | "description" | "$schema"
            ) {
                return Err(wrong(format!(
                    "Unknown key \"{key}\" (expected \"rules\", \"unmatched\" or \"description\")."
                )));
            }
        }
        let unmatched = match file.get("unmatched") {
            None => Unmatched::Manual,
            Some(Value::String(said)) if said == "manual" => Unmatched::Manual,
            Some(Value::String(said)) if said == "method-not-found" => Unmatched::MethodNotFound,
            Some(other) => {
                return Err(wrong(format!(
                    "\"unmatched\" must be \"manual\" or \"method-not-found\", not {other}."
                )));
            }
        };
        let Some(Value::Array(listed)) = file.get("rules") else {
            return Err(wrong("The file needs a \"rules\" list."));
        };
        let rules = listed
            .iter()
            .enumerate()
            .map(|(position, rule)| {
                Rule::parse(rule).map_err(|why| wrong(format!("Rule {}: {why}", position + 1)))
            })
            .collect::<Result<_, _>>()?;

        Ok(Self {
            source: source.into(),
            rules,
            unmatched,
        })
    }

    /// What the file is called on screen.
    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    pub fn unmatched(&self) -> Unmatched {
        self.unmatched
    }

    /// The first rule that matches a request, and its position (from zero).
    pub fn find(&self, method: &str, params: &Value) -> Option<(usize, &Rule)> {
        self.rules
            .iter()
            .enumerate()
            .find(|(_, rule)| rule.matches(method, params))
    }
}

impl Rule {
    fn parse(value: &Value) -> Result<Self, String> {
        let Value::Object(rule) = value else {
            return Err("a rule must be an object".into());
        };
        for key in rule.keys() {
            if !matches!(
                key.as_str(),
                "method" | "path" | "command" | "result" | "error" | "description"
            ) {
                return Err(format!(
                    "unknown key \"{key}\" (expected method, path, command, result or error)"
                ));
            }
        }
        let method = text(rule, "method")?.ok_or("a rule needs a \"method\"")?;
        let path = text(rule, "path")?;
        let command = text(rule, "command")?;
        let answer = match (rule.get("result"), rule.get("error")) {
            (Some(result), None) => Scripted::Result(result.clone()),
            (None, Some(error)) => Scripted::Error(error_of(error)?),
            (Some(_), Some(_)) => {
                return Err("give either \"result\" or \"error\", not both".into());
            }
            (None, None) => return Err("a rule needs a \"result\" or an \"error\"".into()),
        };
        Ok(Self {
            method,
            path,
            command,
            answer,
        })
    }

    /// Whether this rule answers a request.
    pub fn matches(&self, method: &str, params: &Value) -> bool {
        if !glob(&self.method, method) {
            return false;
        }
        if let Some(pattern) = &self.path {
            let Some(path) = params.get("path").and_then(Value::as_str) else {
                return false;
            };
            if !glob(pattern, path) {
                return false;
            }
        }
        if let Some(prefix) = &self.command {
            let Some(line) = command_line(params) else {
                return false;
            };
            let wanted: Vec<&str> = prefix.split_whitespace().collect();
            if line.len() < wanted.len() || line.iter().zip(&wanted).any(|(had, want)| had != want)
            {
                return false;
            }
        }
        true
    }
}

/// `command` then each of `args`, as words, when the request names a command.
fn command_line(params: &Value) -> Option<Vec<&str>> {
    let command = params.get("command").and_then(Value::as_str)?;
    let mut line = vec![command];
    if let Some(args) = params.get("args").and_then(Value::as_array) {
        line.extend(args.iter().filter_map(Value::as_str));
    }
    Some(line)
}

fn text(rule: &Map<String, Value>, key: &str) -> Result<Option<String>, String> {
    match rule.get(key) {
        None => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(other) => Err(format!("\"{key}\" must be a string, not {other}")),
    }
}

fn error_of(value: &Value) -> Result<v1::Error, String> {
    let Value::Object(error) = value else {
        return Err("\"error\" must be an object with \"code\" and \"message\"".into());
    };
    for key in error.keys() {
        if !matches!(key.as_str(), "code" | "message" | "data") {
            return Err(format!(
                "unknown error key \"{key}\" (expected code, message or data)"
            ));
        }
    }
    let code = error
        .get("code")
        .and_then(Value::as_i64)
        .and_then(|code| i32::try_from(code).ok())
        .ok_or("\"error.code\" must be a whole number")?;
    let message = match error.get("message") {
        Some(Value::String(message)) => message.clone(),
        None => String::new(),
        Some(other) => return Err(format!("\"error.message\" must be a string, not {other}")),
    };
    Ok(v1::Error::new(code, message).data(error.get("data").cloned()))
}

/// Whether `text` matches `pattern`, where `*` stands for any run of
/// characters (slashes included) and everything else stands for itself. A
/// pattern with no `*` is an exact match.
pub fn glob(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0, 0);
    // Where the last `*` was, and how much of the text it has eaten so far.
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        if p < pattern.len() && pattern[p] == '*' {
            star = Some((p, t));
            p += 1;
        } else if p < pattern.len() && pattern[p] == text[t] {
            p += 1;
            t += 1;
        } else if let Some((at, ate)) = star {
            p = at + 1;
            t = ate + 1;
            star = Some((at, ate + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_star_stands_for_any_run_and_nothing_else_is_special() {
        assert!(glob("/work/*.rs", "/work/main.rs"));
        assert!(glob("/work/*.rs", "/work/src/deep/lib.rs"));
        assert!(glob("*", ""));
        assert!(glob("a*b*c", "aXXbYYc"));
        assert!(glob("/exact", "/exact"));
        assert!(!glob("/exact", "/exact/more"));
        assert!(!glob("/work/*.rs", "/work/main.rsx"));
        assert!(!glob("a*b*c", "aXXbYY"));
        assert!(glob("terminal/*", "terminal/create"));
        assert!(!glob("terminal/*", "fs/read_text_file"));
        assert!(glob("?", "?"), "only * is a wildcard");
        assert!(!glob("?", "x"));
    }

    #[test]
    fn the_first_matching_rule_answers() {
        let script = ServiceScript::parse(
            "rules.json",
            r#"{"rules":[
                {"method":"fs/read_text_file","path":"/work/README.md","result":{"content":"exact"}},
                {"method":"fs/read_text_file","path":"/work/*","result":{"content":"glob"}},
                {"method":"terminal/create","command":"cargo test","result":{"terminalId":"t"}},
                {"method":"fs/*","error":{"code":-32002,"message":"Resource not found"}}
            ]}"#,
        )
        .unwrap();

        let read = |path: &str| {
            script
                .find("fs/read_text_file", &json!({"sessionId":"s","path":path}))
                .map(|(at, _)| at)
        };
        assert_eq!(read("/work/README.md"), Some(0));
        assert_eq!(read("/work/src/main.rs"), Some(1));
        assert_eq!(read("/elsewhere"), Some(3));
        assert_eq!(
            script
                .find(
                    "terminal/create",
                    &json!({"command":"cargo","args":["test","--all"]})
                )
                .map(|(at, _)| at),
            Some(2)
        );
        assert_eq!(
            script.find(
                "terminal/create",
                &json!({"command":"cargo","args":["build"]})
            ),
            None
        );
        assert_eq!(
            script.find(
                "terminal/create",
                &json!({"command":"cargonaut","args":["test"]})
            ),
            None,
            "a prefix is whole words"
        );
        assert_eq!(script.unmatched(), Unmatched::Manual);
        let Scripted::Error(error) = &script.rules()[3].answer else {
            panic!("an error rule");
        };
        assert_eq!(i32::from(error.code), -32002);
    }

    #[test]
    fn a_mistake_is_refused_with_the_rule_it_is_in() {
        for (text, said) in [
            ("[]", "JSON object"),
            (r#"{"rule":[]}"#, "Unknown key \"rule\""),
            (
                r#"{"rules":[{"result":{}}]}"#,
                "Rule 1: a rule needs a \"method\"",
            ),
            (
                r#"{"rules":[{"method":"x","result":{},"error":{"code":1}}]}"#,
                "Rule 1: give either",
            ),
            (
                r#"{"rules":[{"method":"x"}]}"#,
                "Rule 1: a rule needs a \"result\"",
            ),
            (
                r#"{"rules":[{"method":"x","result":{}},{"method":"y","pth":"/"}]}"#,
                "Rule 2: unknown key \"pth\"",
            ),
            (
                r#"{"rules":[{"method":"x","error":{"code":"no"}}]}"#,
                "whole number",
            ),
            (
                r#"{"rules":[],"unmatched":"never"}"#,
                "\"unmatched\" must be",
            ),
        ] {
            let error = ServiceScript::parse("f", text).expect_err(text);
            assert!(
                error.to_string().contains(said),
                "{text}: {error} should say {said}"
            );
        }
        assert_eq!(
            ServiceScript::parse("f", r#"{"rules":[],"unmatched":"method-not-found"}"#)
                .unwrap()
                .unmatched(),
            Unmatched::MethodNotFound
        );
    }
}
