//! The command line, read by hand.
//!
//! Two subcommands and a dozen options do not need a parsing crate, and the
//! grammar is the one every POSIX tool has: `--name value` or `--name=value`,
//! repeatable where the help says so, and `--` before the agent's own command
//! so that its flags are never mistaken for ours.

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

/// What the invocation asked for.
#[derive(Debug, PartialEq)]
pub enum Invocation {
    Help,
    Version,
    Run(Run),
    Inspect(Inspect),
}

/// `run`: spawn an agent, open a session, prompt it, export the trace.
#[derive(Debug, Default, PartialEq)]
pub struct Run {
    pub command: OsString,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(String, String)>,
    pub unset: Vec<String>,
    pub clear_env: bool,
    pub prompts: Vec<Prompt>,
    pub permission: Policy,
    pub timeout: Option<Duration>,
    pub export: Option<PathBuf>,
    pub json: bool,
}

/// Where a prompt's text comes from. Files are read after parsing, so that a
/// usage error is reported before anything touches the disk.
#[derive(Debug, PartialEq)]
pub enum Prompt {
    Text(String),
    File(PathBuf),
}

/// How a permission request is answered when nobody is there to answer it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Policy {
    /// What a person would be asked. Headless, nobody can be, so it is
    /// rejected — and the summary says that is why.
    #[default]
    Ask,
    /// The first `allow_once` option. Never an `allow_always`: a policy for one
    /// run does not get to change what the agent remembers.
    Allow,
    /// The first reject option.
    Reject,
}

impl Policy {
    pub fn name(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Allow => "allow",
            Self::Reject => "reject",
        }
    }
}

/// `inspect`: summarise an exported trace.
#[derive(Debug, PartialEq)]
pub struct Inspect {
    pub path: PathBuf,
    pub json: bool,
}

/// A command line that does not say anything this program can do.
#[derive(Debug, PartialEq)]
pub struct Usage(pub String);

pub const HELP: &str = "\
acp-inspector-cli: drive an ACP agent over stdio without a window.

Usage:
  acp-inspector-cli run [OPTIONS] -- COMMAND [ARGS...]
  acp-inspector-cli inspect [--json] TRACE.jsonl
  acp-inspector-cli --help | --version

run: spawn the agent, `initialize`, `session/new`, send each prompt in order
and wait for its turn to end, answer permission requests by policy, then stop
the agent gracefully and print a summary.

  --cwd DIR             The agent's working directory and the session's cwd
                        (default: this one)
  --env KEY=VALUE       Add a variable to the agent's environment (repeatable)
  --unset NAME          Leave an inherited variable out (repeatable)
  --clear-env           Start the agent with only the --env variables
  --prompt TEXT         A prompt to send (repeatable, sent in order)
  --prompt-file FILE    A prompt read from a file, `-` for stdin (repeatable)
  --permission POLICY   ask (default; rejected, nobody is there to ask),
                        allow (first allow_once, never allow_always) or
                        reject (first reject option). With no such option
                        offered, the turn is cancelled instead.
  --timeout SECS        Give up on the whole run after SECS seconds
  --export PATH         Write the JSONL trace to PATH (a directory gets a
                        generated name), in the format the app exports
  --json                Print the summary as JSON

inspect: read an exported trace and count its frames by method and direction.

  --json                Print the counts as JSON

Exit codes:
  0   the run completed and nothing was annotated
  1   the agent or the protocol failed: spawn, handshake, session or prompt
      error, an unreadable trace, or an export that could not be written
  2   the run completed and the trace broke a MUST the inspector checks
      (a conformance annotation)
  3   --timeout ran out
  64  the command line was wrong
  130 interrupted (Ctrl-C)
";

/// Reads the arguments after the program name.
pub fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Invocation, Usage> {
    let mut arguments = arguments.into_iter();
    let Some(first) = arguments.next() else {
        return Err(Usage("a subcommand is required: `run` or `inspect`".into()));
    };
    match first.to_str() {
        Some("-h" | "--help" | "help") => Ok(Invocation::Help),
        Some("-V" | "--version") => Ok(Invocation::Version),
        Some("run") => run(arguments),
        Some("inspect") => inspect(arguments),
        _ => Err(Usage(format!(
            "unknown subcommand `{}`: expected `run` or `inspect`",
            first.to_string_lossy()
        ))),
    }
}

/// One `--name` or `--name=value` argument, split.
struct Flag {
    name: String,
    inline: Option<OsString>,
}

/// The argument as a flag, or `None` when it is a positional one.
fn flag(argument: &OsString) -> Option<Flag> {
    let text = argument.to_str()?;
    if !text.starts_with('-') || text == "-" {
        return None;
    }
    Some(match text.split_once('=') {
        Some((name, value)) if name.starts_with("--") => Flag {
            name: name.to_owned(),
            inline: Some(value.into()),
        },
        _ => Flag {
            name: text.to_owned(),
            inline: None,
        },
    })
}

/// The value a flag takes: after `=`, or the next argument.
fn value(flag: &Flag, rest: &mut impl Iterator<Item = OsString>) -> Result<OsString, Usage> {
    match &flag.inline {
        Some(value) => Ok(value.clone()),
        None => rest
            .next()
            .ok_or_else(|| Usage(format!("{} needs a value", flag.name))),
    }
}

/// A value that has to be text, because it goes into JSON or into a policy.
fn text(flag: &Flag, value: OsString) -> Result<String, Usage> {
    value
        .into_string()
        .map_err(|_| Usage(format!("{} must be valid UTF-8", flag.name)))
}

fn switch(flag: &Flag) -> Result<(), Usage> {
    match flag.inline {
        Some(_) => Err(Usage(format!("{} takes no value", flag.name))),
        None => Ok(()),
    }
}

fn run(arguments: impl Iterator<Item = OsString>) -> Result<Invocation, Usage> {
    let mut arguments = arguments.peekable();
    let mut run = Run::default();
    let mut command = None;

    while let Some(argument) = arguments.next() {
        if argument == "--" {
            command = arguments.next();
            break;
        }
        let Some(flag) = flag(&argument) else {
            // The first positional argument is the agent's command, and
            // everything after it is the agent's: `--` is only needed when the
            // command itself starts with a dash.
            command = Some(argument);
            break;
        };
        match flag.name.as_str() {
            "-h" | "--help" => return Ok(Invocation::Help),
            "--cwd" => run.cwd = Some(value(&flag, &mut arguments)?.into()),
            "--env" => {
                let pair = text(&flag, value(&flag, &mut arguments)?)?;
                let Some((key, value)) = pair.split_once('=') else {
                    return Err(Usage(format!("--env takes KEY=VALUE, got `{pair}`")));
                };
                if key.is_empty() {
                    return Err(Usage(format!("--env has an empty name in `{pair}`")));
                }
                run.env.push((key.to_owned(), value.to_owned()));
            }
            "--unset" => {
                let name = text(&flag, value(&flag, &mut arguments)?)?;
                if name.is_empty() || name.contains('=') {
                    return Err(Usage(format!(
                        "--unset takes a variable name, got `{name}`"
                    )));
                }
                run.unset.push(name);
            }
            "--clear-env" => {
                switch(&flag)?;
                run.clear_env = true;
            }
            "--prompt" => run
                .prompts
                .push(Prompt::Text(text(&flag, value(&flag, &mut arguments)?)?)),
            "--prompt-file" => run
                .prompts
                .push(Prompt::File(value(&flag, &mut arguments)?.into())),
            "--permission" => {
                run.permission = match text(&flag, value(&flag, &mut arguments)?)?.as_str() {
                    "ask" => Policy::Ask,
                    "allow" => Policy::Allow,
                    "reject" => Policy::Reject,
                    other => {
                        return Err(Usage(format!(
                            "--permission is ask, allow or reject, not `{other}`"
                        )));
                    }
                }
            }
            "--timeout" => {
                let seconds = text(&flag, value(&flag, &mut arguments)?)?;
                let timeout = seconds
                    .parse::<f64>()
                    .ok()
                    .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
                    .and_then(|seconds| Duration::try_from_secs_f64(seconds).ok())
                    .ok_or_else(|| {
                        Usage(format!(
                            "--timeout takes a positive number of seconds, not `{seconds}`"
                        ))
                    })?;
                run.timeout = Some(timeout);
            }
            "--export" => run.export = Some(value(&flag, &mut arguments)?.into()),
            "--json" => {
                switch(&flag)?;
                run.json = true;
            }
            _ => return Err(Usage(format!("unknown option `{}` for run", flag.name))),
        }
    }

    let Some(command) = command.filter(|command| !command.is_empty()) else {
        return Err(Usage(
            "run needs the agent's command: `run [OPTIONS] -- COMMAND [ARGS...]`".into(),
        ));
    };
    run.command = command;
    run.args = arguments.collect();
    Ok(Invocation::Run(run))
}

fn inspect(arguments: impl Iterator<Item = OsString>) -> Result<Invocation, Usage> {
    let mut json = false;
    let mut path = None;
    let mut only_positional = false;

    for argument in arguments {
        if !only_positional {
            if argument == "--" {
                only_positional = true;
                continue;
            }
            if let Some(flag) = flag(&argument) {
                match flag.name.as_str() {
                    "--json" => {
                        switch(&flag)?;
                        json = true;
                    }
                    "-h" | "--help" => return Ok(Invocation::Help),
                    _ => {
                        return Err(Usage(format!("unknown option `{}` for inspect", flag.name)));
                    }
                }
                continue;
            }
        }
        if path.replace(PathBuf::from(argument)).is_some() {
            return Err(Usage("inspect reads one trace at a time".into()));
        }
    }

    let path = path.ok_or_else(|| Usage("inspect needs a trace: `inspect TRACE.jsonl`".into()))?;
    Ok(Invocation::Inspect(Inspect { path, json }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(line: &[&str]) -> Result<Invocation, Usage> {
        parse(line.iter().map(OsString::from))
    }

    fn ran(line: &[&str]) -> Run {
        match parsed(line) {
            Ok(Invocation::Run(run)) => run,
            other => panic!("expected a run, got {other:?}"),
        }
    }

    #[test]
    fn help_and_version_are_recognised_alone() {
        assert_eq!(parsed(&["--help"]), Ok(Invocation::Help));
        assert_eq!(parsed(&["-h"]), Ok(Invocation::Help));
        assert_eq!(parsed(&["help"]), Ok(Invocation::Help));
        assert_eq!(parsed(&["--version"]), Ok(Invocation::Version));
        assert_eq!(parsed(&["-V"]), Ok(Invocation::Version));
    }

    #[test]
    fn a_run_reads_every_option_and_leaves_the_agents_flags_alone() {
        let run = ran(&[
            "run",
            "--cwd",
            "/work",
            "--env",
            "A=1=2",
            "--env=B=",
            "--unset",
            "SECRET",
            "--clear-env",
            "--prompt",
            "hello",
            "--prompt-file=p.txt",
            "--permission",
            "allow",
            "--timeout",
            "2.5",
            "--export",
            "out.jsonl",
            "--json",
            "--",
            "agent",
            "--json",
            "--prompt",
            "x",
        ]);
        assert_eq!(run.command, "agent");
        assert_eq!(run.args, ["--json", "--prompt", "x"]);
        assert_eq!(run.cwd, Some("/work".into()));
        assert_eq!(
            run.env,
            [("A".into(), "1=2".into()), ("B".into(), String::new())]
        );
        assert_eq!(run.unset, ["SECRET"]);
        assert!(run.clear_env);
        assert_eq!(
            run.prompts,
            [Prompt::Text("hello".into()), Prompt::File("p.txt".into())]
        );
        assert_eq!(run.permission, Policy::Allow);
        assert_eq!(run.timeout, Some(Duration::from_millis(2500)));
        assert_eq!(run.export, Some("out.jsonl".into()));
        assert!(run.json);
    }

    #[test]
    fn the_command_may_follow_the_options_without_a_separator() {
        let run = ran(&["run", "--prompt", "hi", "python3", "agent.py", "--flag"]);
        assert_eq!(run.command, "python3");
        assert_eq!(run.args, ["agent.py", "--flag"]);
        assert_eq!(run.permission, Policy::Ask);
    }

    #[test]
    fn mistakes_are_usage_errors() {
        for line in [
            &[][..],
            &["frobnicate"],
            &["run"],
            &["run", "--"],
            &["run", "--prompt"],
            &["run", "--nope", "--", "agent"],
            &["run", "--env", "NOEQUALS", "--", "agent"],
            &["run", "--env", "=v", "--", "agent"],
            &["run", "--unset", "A=B", "--", "agent"],
            &["run", "--permission", "always", "--", "agent"],
            &["run", "--timeout", "0", "--", "agent"],
            &["run", "--timeout", "soon", "--", "agent"],
            &["run", "--json=yes", "--", "agent"],
            &["inspect"],
            &["inspect", "a", "b"],
            &["inspect", "--verbose", "a"],
        ] {
            assert!(
                matches!(parsed(line), Err(Usage(_))),
                "{line:?} should be a usage error"
            );
        }
    }

    #[test]
    fn inspect_takes_one_trace_and_an_optional_json_switch() {
        assert_eq!(
            parsed(&["inspect", "--json", "t.jsonl"]),
            Ok(Invocation::Inspect(Inspect {
                path: "t.jsonl".into(),
                json: true
            }))
        );
        assert_eq!(
            parsed(&["inspect", "--", "--odd-name.jsonl"]),
            Ok(Invocation::Inspect(Inspect {
                path: "--odd-name.jsonl".into(),
                json: false
            }))
        );
    }
}
