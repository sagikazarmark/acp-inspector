//! `acp-inspector-cli`: the ACP Inspector without a window.
//!
//! A second surface over `acp-inspector-core` (`docs/architecture.md` §5): it
//! spawns an agent, drives the handshake and the prompts it is given, answers
//! permission requests by policy, writes the same JSONL trace the desktop app
//! exports, and exits with a code a CI job can branch on. Everything it knows
//! about ACP is core's; this crate is arguments in and a summary out.

mod args;
mod inspect;
mod report;
mod run;

use std::io::Read;
use std::process::ExitCode;

use args::{Invocation, Prompt, Usage};

/// The exit code for a command line that does not parse (`sysexits.h`'s
/// `EX_USAGE`).
const USAGE: u8 = 64;

fn main() -> ExitCode {
    let invocation = match args::parse(std::env::args_os().skip(1)) {
        Ok(invocation) => invocation,
        Err(Usage(problem)) => return usage(&problem),
    };
    ExitCode::from(match invocation {
        Invocation::Help => {
            print!("{}", args::HELP);
            0
        }
        Invocation::Version => {
            println!("acp-inspector-cli {}", env!("CARGO_PKG_VERSION"));
            0
        }
        Invocation::Inspect(inspect) => inspect::inspect(&inspect),
        Invocation::Run(run) => {
            let prompts = match prompts(&run.prompts) {
                Ok(prompts) => prompts,
                Err(problem) => return usage(&problem),
            };
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    eprintln!("acp-inspector-cli: cannot start the async runtime: {error}");
                    return ExitCode::from(1);
                }
            };
            let report = runtime.block_on(run::run(&run, prompts));
            if run.json {
                println!("{:#}", report::json(&report));
            } else {
                print!("{}", report::human(&report));
            }
            report.exit_code()
        }
    })
}

fn usage(problem: &str) -> ExitCode {
    eprintln!("acp-inspector-cli: {problem}");
    eprintln!("Run `acp-inspector-cli --help` for usage.");
    ExitCode::from(USAGE)
}

/// Every prompt's text, files read. A file that cannot be read is a mistake
/// on the command line, found before an agent is spawned for nothing.
fn prompts(prompts: &[Prompt]) -> Result<Vec<String>, String> {
    prompts
        .iter()
        .map(|prompt| match prompt {
            Prompt::Text(text) => Ok(text.clone()),
            Prompt::File(path) if path.as_os_str() == "-" => {
                let mut text = String::new();
                std::io::stdin()
                    .read_to_string(&mut text)
                    .map_err(|error| format!("cannot read a prompt from stdin: {error}"))?;
                Ok(text)
            }
            Prompt::File(path) => std::fs::read_to_string(path)
                .map_err(|error| format!("cannot read --prompt-file {}: {error}", path.display())),
        })
        .collect()
}
