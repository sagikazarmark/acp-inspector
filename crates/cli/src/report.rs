//! A [`Report`] as a person reads it, and as a script does (`--json`).

use std::fmt::Write as _;

use acp_inspector_core::{Diagnostic, DiagnosticKind, Stop};
use serde_json::{Value, json};

use crate::args::Policy;
use crate::run::{Answer, Ending, Report, spelt};

/// How many of the agent's last stderr lines the summary carries.
const STDERR_TAIL: usize = 20;

/// The JSON summary's format name and version, so a consumer can refuse one it
/// does not know. Fields may be added without a version bump.
const FORMAT: &str = "acp-inspector-cli-run";
const VERSION: u32 = 1;

/// One line about the run's ending.
fn ending(report: &Report) -> String {
    match &report.ending {
        Ending::Completed if report.annotations.is_empty() => "completed".into(),
        Ending::Completed => "completed, with conformance annotations".into(),
        Ending::Failed(failure) => failure.to_string(),
        Ending::TimedOut(limit) => format!("timed out after {} s", limit.as_secs_f64()),
        Ending::Interrupted => "interrupted".into(),
    }
}

fn outcome(report: &Report) -> &'static str {
    match &report.ending {
        Ending::Completed if report.annotations.is_empty() => "ok",
        Ending::Completed => "conformance",
        Ending::Failed(_) => "failed",
        Ending::TimedOut(_) => "timeout",
        Ending::Interrupted => "interrupted",
    }
}

/// A diagnostic that is not a stderr line, as a sentence.
fn remark(diagnostic: &Diagnostic) -> Option<String> {
    Some(match &diagnostic.kind {
        DiagnosticKind::Stderr(_) => return None,
        DiagnosticKind::SpawnFailed { command, error } => {
            format!("could not start `{command}`: {error}")
        }
        DiagnosticKind::AgentExited(status) => format!("the agent exited: {status}"),
        DiagnosticKind::FrameDropped { limit, length } => {
            format!("dropped a {length}-byte frame over the {limit}-byte limit")
        }
        DiagnosticKind::StderrLineCut { limit, length } => {
            format!("cut a {length}-byte stderr line to {limit} bytes")
        }
        DiagnosticKind::StderrLineLossy => "a stderr line was not UTF-8".into(),
        DiagnosticKind::Stopped { by, status } => {
            let by = match by {
                Stop::AlreadyExited => "it had already exited",
                Stop::StdinClosed => "stdin closed",
                Stop::Terminated => "SIGTERM",
                Stop::Killed => "SIGKILL",
                _ => "an unnamed step",
            };
            match status {
                Some(status) => format!("the agent stopped ({by}): {status}"),
                None => format!("the agent stopped ({by})"),
            }
        }
        DiagnosticKind::DrainAbandoned => {
            "the agent exited but something kept its output open; reading was abandoned".into()
        }
        DiagnosticKind::WriteFailed(error) => format!("writing to the agent failed: {error}"),
        DiagnosticKind::TransportFailed(error) => format!("the transport failed: {error}"),
        other => format!("{other:?}"),
    })
}

fn stderr(report: &Report) -> Vec<&str> {
    report
        .diagnostics
        .iter()
        .filter_map(|diagnostic| match &diagnostic.kind {
            DiagnosticKind::Stderr(line) => Some(line.as_str()),
            _ => None,
        })
        .collect()
}

/// The summary for a person.
pub fn human(report: &Report) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "agent        {}", report.command);
    match &report.agent {
        Some(agent) => {
            let info = agent.agent_info.as_ref().map_or_else(
                || "(no agentInfo)".to_owned(),
                |info| format!("{} {}", info.name, info.version),
            );
            let _ = writeln!(
                out,
                "             {info}, protocol {}",
                spelt(&agent.protocol_version)
            );
            let capabilities = serde_json::to_string(&agent.agent_capabilities)
                .unwrap_or_else(|error| format!("<{error}>"));
            let _ = writeln!(out, "capabilities {capabilities}");
            if !agent.auth_methods.is_empty() {
                let methods: Vec<_> = agent.auth_methods.iter().map(spelt_id).collect();
                let _ = writeln!(out, "auth         {}", methods.join(", "));
            }
        }
        None => {
            let _ = writeln!(out, "             (did not answer initialize)");
        }
    }
    if let Some(session) = &report.session {
        let _ = writeln!(out, "session      {session}");
    }

    for (index, turn) in report.turns.iter().enumerate() {
        let ended = match &turn.outcome {
            Some(Ok(reason)) => spelt(reason),
            Some(Err(error)) => format!("failed: {error}"),
            None => "did not end".into(),
        };
        let _ = writeln!(out, "turn {:<7} {ended}", index + 1);
        let _ = writeln!(out, "  prompt     {}", clipped(&turn.prompt, 120));
        if !turn.message.is_empty() {
            let _ = writeln!(out, "  message    {}", clipped(&turn.message, 120));
        }
        for call in &turn.tool_calls {
            let _ = writeln!(
                out,
                "  tool call  {} {} [{}] {}",
                call.id,
                call.title
                    .as_deref()
                    .map_or_else(String::new, |t| format!("{t:?}")),
                call.kind.as_deref().unwrap_or("-"),
                call.status.as_deref().unwrap_or("-"),
            );
        }
    }

    for asked in &report.permissions {
        let about = asked
            .title
            .as_deref()
            .map_or_else(|| asked.tool_call.clone(), |title| format!("{title:?}"));
        let answer = match &asked.answer {
            Answer::Selected { option, kind } => format!("selected `{option}` ({kind})"),
            Answer::CancelledTurn => {
                "no option fit the policy, so the turn was cancelled".to_owned()
            }
            Answer::Failed(error) => format!("could not answer: {error}"),
        };
        let note = if report.policy == Policy::Ask {
            " — policy `ask`: nobody to ask headless, so rejected"
        } else {
            ""
        };
        let _ = writeln!(out, "permission   {about}: {answer}{note}");
    }
    if report.elicitations > 0 {
        let _ = writeln!(
            out,
            "elicitation  {} cancelled (nobody to fill them in headless)",
            report.elicitations
        );
    }

    if report.annotations.is_empty() {
        let _ = writeln!(out, "conformance  no annotations");
    } else {
        for annotation in &report.annotations {
            let _ = writeln!(out, "conformance  MUST: {}", annotation.requires());
            let _ = writeln!(out, "             observed: {}", annotation.observed());
        }
    }

    let lines = stderr(report);
    let _ = writeln!(out, "stderr       {} lines", lines.len());
    for line in lines.iter().skip(lines.len().saturating_sub(STDERR_TAIL)) {
        let _ = writeln!(out, "  | {line}");
    }
    for remark in report.diagnostics.iter().filter_map(remark) {
        let _ = writeln!(out, "transport    {remark}");
    }
    if report.diagnostics_dropped > 0 {
        let _ = writeln!(
            out,
            "transport    {} older diagnostics were dropped",
            report.diagnostics_dropped
        );
    }

    let _ = write!(
        out,
        "trace        {} frames, {} dropped",
        report.frames, report.dropped
    );
    match &report.export {
        Some(Ok(path)) => {
            let _ = writeln!(out, ", exported to {}", path.display());
        }
        Some(Err(error)) => {
            let _ = writeln!(out, ", export failed: {error}");
        }
        None => {
            let _ = writeln!(out);
        }
    }
    let _ = writeln!(
        out,
        "result       {} (exit {})",
        ending(report),
        report.exit_code()
    );
    out
}

/// The summary for a script.
pub fn json(report: &Report) -> Value {
    let agent = report.agent.as_ref().map(|agent| {
        json!({
            "name": agent.agent_info.as_ref().map(|info| info.name.clone()),
            "title": agent.agent_info.as_ref().and_then(|info| info.title.clone()),
            "version": agent.agent_info.as_ref().map(|info| info.version.clone()),
            "protocolVersion": serde_json::to_value(agent.protocol_version).ok(),
            "capabilities": serde_json::to_value(&agent.agent_capabilities).ok(),
            "authMethods": agent.auth_methods.iter().map(spelt_id).collect::<Vec<_>>(),
        })
    });

    let turns: Vec<_> = report
        .turns
        .iter()
        .map(|turn| {
            json!({
                "prompt": turn.prompt,
                "stopReason": match &turn.outcome {
                    Some(Ok(reason)) => Some(spelt(reason)),
                    _ => None,
                },
                "error": match &turn.outcome {
                    Some(Err(error)) => Some(error.to_string()),
                    _ => None,
                },
                "message": turn.message,
                "toolCalls": turn.tool_calls.iter().map(|call| json!({
                    "id": call.id,
                    "title": call.title,
                    "kind": call.kind,
                    "status": call.status,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();

    let permissions: Vec<_> = report
        .permissions
        .iter()
        .map(|asked| {
            let (answer, option, kind, error) = match &asked.answer {
                Answer::Selected { option, kind } => {
                    ("selected", Some(option.clone()), Some(kind.clone()), None)
                }
                Answer::CancelledTurn => ("cancelledTurn", None, None, None),
                Answer::Failed(error) => ("failed", None, None, Some(error.clone())),
            };
            json!({
                "toolCallId": asked.tool_call,
                "title": asked.title,
                "options": asked.offered.iter().map(|(id, kind)| json!({"optionId": id, "kind": kind})).collect::<Vec<_>>(),
                "answer": answer,
                "optionId": option,
                "kind": kind,
                "error": error,
            })
        })
        .collect();

    let annotations: Vec<_> = report
        .annotations
        .iter()
        .map(|annotation| {
            json!({
                "requires": annotation.requires(),
                "observed": annotation.observed(),
            })
        })
        .collect();

    let lines = stderr(report);
    let tail: Vec<_> = lines
        .iter()
        .skip(lines.len().saturating_sub(STDERR_TAIL))
        .collect();

    json!({
        "format": FORMAT,
        "version": VERSION,
        "outcome": outcome(report),
        "exitCode": report.exit_code(),
        "summary": ending(report),
        "command": report.command,
        "permissionPolicy": report.policy.name(),
        "agent": agent,
        "sessionId": report.session.as_ref().map(ToString::to_string),
        "turns": turns,
        "permissions": permissions,
        "elicitationsCancelled": report.elicitations,
        "annotations": annotations,
        "diagnostics": {
            "stderrLines": lines.len(),
            "stderrTail": tail,
            "transport": report.diagnostics.iter().filter_map(remark).collect::<Vec<_>>(),
            "dropped": report.diagnostics_dropped,
        },
        "trace": {
            "frames": report.frames,
            "dropped": report.dropped,
            "export": match &report.export {
                Some(Ok(path)) => Some(path.display().to_string()),
                _ => None,
            },
            "exportError": match &report.export {
                Some(Err(error)) => Some(error.clone()),
                _ => None,
            },
        },
    })
}

/// An auth method by its id.
fn spelt_id(method: &acp_inspector_core::v1::AuthMethod) -> String {
    serde_json::to_value(method)
        .ok()
        .and_then(|value| value.get("id").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_else(|| "?".into())
}

/// One line of at most `limit` characters.
fn clipped(text: &str, limit: usize) -> String {
    let line: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if line.chars().count() <= limit {
        line
    } else {
        let mut clipped: String = line.chars().take(limit - 1).collect();
        clipped.push('…');
        clipped
    }
}
