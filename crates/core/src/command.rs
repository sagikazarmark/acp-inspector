//! The spawn form's fields, and the factory they describe
//! (`docs/architecture.md` §9).
//!
//! Text in, a connection factory out. The desktop shell carries four strings
//! from four inputs and nothing else, so where a line of text stops being one
//! argument and starts being the next is a decision core makes and core's tests
//! cover — not one each surface improvises, and not one hiding in an event
//! handler.
//!
//! It sits beside the transport rather than inside it because the two change
//! for different reasons: the fields are the *form's*, and the day a second
//! factory arrives (§6.1) is the day this grows a second thing to build.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::stdio::StdioSpawn;

/// An agent to launch, as the user typed it.
///
/// It serializes as its four fields and nothing else, because that is what the
/// spawn form remembers ([`RecentCommands`](crate::RecentCommands)) — and a
/// missing field reads as an empty one, so a file written by an older shape
/// still refills a form.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentCommand {
    /// The program to run.
    pub command: String,
    /// One argument per line.
    ///
    /// No quoting rules to learn and none to get wrong: a path with a space in
    /// it, or a JSON blob passed as a flag, is just a line. Blank lines are
    /// somebody halfway through typing, and are not arguments; surrounding
    /// whitespace on a line is not part of the argument either, because a
    /// trailing space nobody can see is not something a form should be able to
    /// pass to `exec`.
    pub args: String,
    /// One `KEY=VALUE` per line, added to the inherited environment
    /// ([`StdioSpawn::env`]), under the same rules as [`args`](Self::args).
    pub env: String,
    /// The agent's working directory. Empty means the inspector's own.
    pub cwd: String,
}

impl AgentCommand {
    /// The connection factory this form describes.
    pub fn factory(&self) -> StdioSpawn {
        let spawn = StdioSpawn::new(self.command.trim())
            .args(self.arguments())
            .envs(self.variables());

        match self.cwd.trim() {
            "" => spawn,
            cwd => spawn.cwd(cwd),
        }
    }

    /// The arguments this would be run with, in order.
    ///
    /// The same reading [`factory`](Self::factory) spawns with, which is the
    /// point of it being here: a surface that wants to *show* an invocation —
    /// the spawn form's recent list — reads it from the field the same way the
    /// spawn does, rather than splitting lines again and drifting from it.
    pub fn arguments(&self) -> impl Iterator<Item = &str> {
        filled_lines(&self.args)
    }

    /// The variables this would be run with, as name and value.
    ///
    /// A line that sets nothing is not one of them, under the same rule the
    /// spawn follows — so counting these is counting what the agent would
    /// actually be given.
    pub fn variables(&self) -> impl Iterator<Item = (&str, &str)> {
        variables(&self.env)
    }

    /// The working directory a session on this agent gets (§7.1).
    ///
    /// The same field the child process is started in, which is what makes the
    /// two agree — but ACP requires an *absolute* path, while a form is a place
    /// where people type `.` and leave the field empty. So an empty field means
    /// the inspector's own directory, exactly as it does for the spawn, and a
    /// relative one is resolved against it rather than sent as typed for the
    /// agent to guess at.
    ///
    /// `None` when the answer would have to be invented: an absolute path can
    /// only be made out of a relative one by knowing where the inspector itself
    /// is, and a process whose working directory has been deleted underneath it
    /// does not know. Naming some plausible directory instead would be the one
    /// thing this tool may not do — and the agent would be started in that same
    /// unknowable place anyway.
    pub fn session_cwd(&self) -> Option<PathBuf> {
        let cwd = Path::new(self.cwd.trim());
        if cwd.is_absolute() {
            return Some(cwd.to_path_buf());
        }
        // Empty joins to nothing, which is the "the inspector's own directory"
        // case falling out of the relative one.
        Some(std::env::current_dir().ok()?.join(cwd))
    }

    /// The same invocation with what the spawn ignores taken out: every field
    /// trimmed, and the blank lines gone from the two that are lists.
    ///
    /// **It changes nothing about what would run.** [`factory`](Self::factory)
    /// already trims each line and drops the empty ones, and
    /// [`session_cwd`](Self::session_cwd) already trims the directory — this is
    /// that same reading, kept. What it is for is remembering: an invocation
    /// stored as typed would put a trailing space nobody can see between two
    /// rows that launch the same agent.
    ///
    /// It does not rewrite what a line *means*. `FOO = bar` keeps its spaces,
    /// because inside a line is where the user's own text starts and a
    /// convenience that edited it would be answering a question nobody asked.
    pub fn normalized(&self) -> Self {
        Self {
            command: self.command.trim().to_owned(),
            args: joined(&self.args),
            env: joined(&self.env),
            cwd: self.cwd.trim().to_owned(),
        }
    }

    /// Whether there is an agent to launch at all — the Launch button's
    /// condition, so that "nothing typed" is a disabled button rather than a
    /// spawn failure explaining that `` is not a program.
    pub fn is_runnable(&self) -> bool {
        !self.command.trim().is_empty()
    }
}

/// The lines of a field that carry something, trimmed.
fn filled_lines(field: &str) -> impl Iterator<Item = &str> {
    field.lines().map(str::trim).filter(|line| !line.is_empty())
}

/// Those lines back as a field — the one that would have produced them.
fn joined(field: &str) -> String {
    filled_lines(field).collect::<Vec<_>>().join("\n")
}

/// `KEY=VALUE` lines, split at the first `=`.
///
/// A line without one sets nothing. Half-typed input is a state a form spends
/// most of its life in, and guessing what `FOO` was going to mean is worse than
/// waiting for the rest of it.
fn variables(field: &str) -> impl Iterator<Item = (&str, &str)> {
    filled_lines(field)
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim(), value.trim()))
}
