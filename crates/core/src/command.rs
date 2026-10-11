//! The spawn form's fields, and the factory they describe
//! (`docs/architecture.md` §9).
//!
//! How a line of text becomes arguments and variables is decided here, in core,
//! so every surface reads the form the same way.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::stdio::StdioSpawn;

/// An agent to launch, as the user typed it.
///
/// Serializes as its fields, which is what
/// [`RecentCommands`](crate::RecentCommands) stores; a missing field reads as
/// empty, so files from older shapes still load.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentCommand {
    /// The program to run.
    pub command: String,
    /// One argument per line, with no quoting rules. Each line is trimmed, and
    /// blank lines are not arguments.
    pub args: String,
    /// One `KEY=VALUE` per line, added to the inherited environment
    /// ([`StdioSpawn::env`]), under the same rules as [`args`](Self::args).
    pub env: String,
    /// The agent's working directory. Empty means the inspector's own.
    pub cwd: String,
    /// Inherited variables to leave out, one name per line. Variables set in
    /// [`env`](Self::env) are still set.
    pub unset: String,
    /// Start the agent with only the variables in [`env`](Self::env), not
    /// the inspector's environment.
    pub clear_env: bool,
    /// Whether the recent list may keep the values in [`env`](Self::env).
    ///
    /// Off by default because the recent list is a plaintext file; without it
    /// only names are kept ([`remembered`](Self::remembered)). It changes
    /// nothing about what runs.
    pub remember_env_values: bool,
}

/// What a minimal environment keeps from the inspector's: enough for a shell
/// and a package runner to find themselves.
pub const MINIMAL_ENV: &[&str] = &[
    "PATH", "HOME", "USER", "LOGNAME", "SHELL", "TMPDIR", "LANG", "TERM",
];

impl AgentCommand {
    /// The connection factory this form describes.
    pub fn factory(&self) -> StdioSpawn {
        let mut spawn = StdioSpawn::new(self.command.trim()).args(self.arguments());
        if self.clear_env {
            spawn = spawn.env_clear();
        }
        for name in self.unset_names() {
            spawn = spawn.env_remove(name);
        }
        let spawn = spawn.envs(self.variables());

        match self.cwd.trim() {
            "" => spawn,
            cwd => spawn.cwd(cwd),
        }
    }

    /// The inherited variables this would leave out, by name.
    pub fn unset_names(&self) -> impl Iterator<Item = &str> {
        filled_lines(&self.unset)
    }

    /// [`MINIMAL_ENV`] as `KEY=VALUE` lines with the inspector's own values,
    /// for an [`env`](Self::env) field to start from alongside
    /// [`clear_env`](Self::clear_env).
    pub fn minimal_env() -> String {
        MINIMAL_ENV
            .iter()
            .filter_map(|name| Some(format!("{name}={}", std::env::var(name).ok()?)))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The arguments this would be run with, in order — the same reading
    /// [`factory`](Self::factory) uses.
    pub fn arguments(&self) -> impl Iterator<Item = &str> {
        filled_lines(&self.args)
    }

    /// The variables this would be run with, as name and value. Lines without
    /// `=` set nothing and are skipped.
    pub fn variables(&self) -> impl Iterator<Item = (&str, &str)> {
        variables(&self.env)
    }

    /// The absolute working directory for a session on this agent (§7.1).
    ///
    /// Empty means the inspector's own directory, as for the spawn; a relative
    /// path is resolved against it. `None` when the inspector's own working
    /// directory cannot be determined (e.g. it was deleted).
    pub fn session_cwd(&self) -> Option<PathBuf> {
        let cwd = Path::new(self.cwd.trim());
        if cwd.is_absolute() {
            return Some(cwd.to_path_buf());
        }
        // Joining an empty path yields the current directory itself.
        Some(std::env::current_dir().ok()?.join(cwd))
    }

    /// The same invocation with what the spawn ignores taken out: every field
    /// trimmed, and blank lines removed from the list fields.
    ///
    /// Changes nothing about what would run, and never rewrites inside a line
    /// (`FOO = bar` keeps its spaces). Used so remembered rows compare equal.
    pub fn normalized(&self) -> Self {
        Self {
            command: self.command.trim().to_owned(),
            args: joined(&self.args),
            env: joined(&self.env),
            cwd: self.cwd.trim().to_owned(),
            unset: joined(&self.unset),
            clear_env: self.clear_env,
            remember_env_values: self.remember_env_values,
        }
    }

    /// The invocation as the recent list keeps it: [`normalized`](Self::normalized),
    /// and with only the variable names in [`env`](Self::env) unless
    /// [`remember_env_values`](Self::remember_env_values) is set.
    ///
    /// An invocation that sets no variables is kept without the flag, so it
    /// maps to one row.
    pub fn remembered(&self) -> Self {
        let mut kept = self.normalized();
        if kept.variables().next().is_none() {
            kept.remember_env_values = false;
        }
        if kept.remember_env_values {
            kept
        } else {
            kept.without_env_values()
        }
    }

    /// The same invocation with every variable's value taken out: `NAME=value`
    /// becomes `NAME=`. Lines that set nothing are kept as they are.
    pub fn without_env_values(&self) -> Self {
        Self {
            env: self
                .env
                .lines()
                .map(|line| match line.split_once('=') {
                    Some((name, _)) => format!("{}=", name.trim()),
                    None => line.to_owned(),
                })
                .collect::<Vec<_>>()
                .join("\n"),
            remember_env_values: false,
            ..self.clone()
        }
    }

    /// Whether any variable carries a value — what a remembered entry would be
    /// keeping on disk.
    pub fn has_env_values(&self) -> bool {
        self.variables().any(|(_, value)| !value.is_empty())
    }

    /// Whether this is a remembered entry whose variable values were not kept:
    /// it names variables, and none of them has a value to launch with.
    pub fn env_values_missing(&self) -> bool {
        !self.remember_env_values && self.variables().next().is_some() && !self.has_env_values()
    }

    /// Whether there is a command to launch at all (the Launch button's
    /// condition).
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

/// `KEY=VALUE` lines, split at the first `=`. A line without one is half-typed
/// input and sets nothing.
fn variables(field: &str) -> impl Iterator<Item = (&str, &str)> {
    filled_lines(field)
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim(), value.trim()))
}
