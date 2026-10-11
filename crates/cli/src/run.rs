//! `run`: one agent, one session, the prompts in order, and the record of it.
//!
//! Everything here is core's public API driven the way the desktop shell
//! drives it — connect, `initialize`, `session/new`, a prompt at a time — with
//! a policy standing in for the person who would answer the permission panel.
//! What comes out is a [`Report`]: what the agent claimed, how each turn ended,
//! what was answered on whose behalf, which MUSTs the trace says were broken,
//! and where the evidence went.

use std::collections::HashSet;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use acp_inspector_core::{
    Annotation, CallError, ConnectionStatus, Diagnostic, DiagnosticKind, EntryKind, Export,
    Inspector, PermissionRequest, StdioSpawn, v1,
};

use crate::args::{Policy, Run};

/// How long a graceful stop may take before the agent is killed outright. Core
/// gives each polite step its own grace and reports within six seconds; this
/// is that, and a little slack.
const STOPPING: Duration = Duration::from_secs(7);

/// How long a connection that has gone away gets to say why.
const SETTLING: Duration = Duration::from_secs(2);

/// What a run came to, and everything the summary says about it.
pub struct Report {
    /// The agent's command line, as it was spawned.
    pub command: String,
    pub policy: Policy,
    pub ending: Ending,
    /// What `initialize` answered, if it did.
    pub agent: Option<v1::InitializeResponse>,
    pub session: Option<v1::SessionId>,
    pub turns: Vec<TurnReport>,
    pub permissions: Vec<Answered>,
    /// Elicitations cancelled because nobody was there to fill them in.
    pub elicitations: usize,
    pub annotations: Vec<Annotation>,
    pub diagnostics: Vec<Diagnostic>,
    /// Diagnostics the log no longer holds (its own ring).
    pub diagnostics_dropped: usize,
    pub frames: usize,
    pub dropped: usize,
    /// Where the export went, or why it did not; `None` when none was asked for.
    pub export: Option<Result<PathBuf, String>>,
}

/// How the run ended.
pub enum Ending {
    /// Every prompt was answered with a stop reason.
    Completed,
    /// The agent or the protocol failed, at this stage.
    Failed(Failure),
    /// `--timeout` ran out first.
    TimedOut(Duration),
    /// Ctrl-C.
    Interrupted,
}

/// Where a run failed.
pub enum Failure {
    /// The session's working directory could not be named.
    WorkingDirectory(String),
    /// The agent never started.
    Spawn(String),
    /// `initialize` had no answer.
    Handshake(CallError),
    /// `session/new` had no answer.
    Session(CallError),
    /// A prompt was answered with an error, or not at all.
    Prompt { turn: usize, error: CallError },
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WorkingDirectory(problem) => {
                write!(f, "the session's working directory: {problem}")
            }
            Self::Spawn(problem) => write!(f, "the agent did not start: {problem}"),
            Self::Handshake(error) => write!(f, "`initialize` failed: {error}"),
            Self::Session(error) => write!(f, "`session/new` failed: {error}"),
            Self::Prompt { turn, error } => write!(f, "turn {turn} failed: {error}"),
        }
    }
}

/// One prompt, and what became of the turn it opened.
pub struct TurnReport {
    pub prompt: String,
    /// `None` when the run ended before the turn did.
    pub outcome: Option<Result<v1::StopReason, CallError>>,
    /// The agent's message chunks in this turn, concatenated.
    pub message: String,
    pub tool_calls: Vec<ToolCallReport>,
}

/// A tool call as it last stood.
pub struct ToolCallReport {
    pub id: String,
    pub title: Option<String>,
    pub kind: Option<String>,
    pub status: Option<String>,
}

/// A permission request, and what the policy answered it with.
pub struct Answered {
    pub tool_call: String,
    pub title: Option<String>,
    /// The options offered, as `(id, kind)`.
    pub offered: Vec<(String, String)>,
    pub answer: Answer,
}

pub enum Answer {
    /// This option was selected.
    Selected { option: String, kind: String },
    /// No option fit the policy, so the turn was cancelled — which answers the
    /// request `cancelled`, the one honest way to say nobody chose.
    CancelledTurn,
    /// The answer could not be sent.
    Failed(String),
}

impl Report {
    /// The process's exit code (docs/cli.md, "Exit codes").
    pub fn exit_code(&self) -> u8 {
        match &self.ending {
            Ending::Interrupted => 130,
            Ending::TimedOut(_) => 3,
            Ending::Failed(_) => 1,
            Ending::Completed if matches!(self.export, Some(Err(_))) => 1,
            Ending::Completed if !self.annotations.is_empty() => 2,
            Ending::Completed => 0,
        }
    }
}

/// Runs the agent `run` describes through `prompts`, and reports on it.
pub async fn run(run: &Run, prompts: Vec<String>) -> Report {
    let inspector = Inspector::new();
    let spawn = factory(run);
    let answers = Arc::new(Mutex::new(Answers::default()));
    let mut turns = Vec::new();

    let answering = tokio::spawn(answer(inspector.clone(), run.permission, answers.clone()));

    let ending = {
        let driven = async {
            let cwd = session_cwd(run.cwd.as_deref()).map_err(Failure::WorkingDirectory)?;
            drive(&inspector, &spawn, cwd, &prompts, &mut turns).await
        };
        let bounded = async {
            match run.timeout {
                Some(limit) => match tokio::time::timeout(limit, driven).await {
                    Ok(driven) => driven.map_or_else(Ending::Failed, |()| Ending::Completed),
                    Err(_) => Ending::TimedOut(limit),
                },
                None => driven
                    .await
                    .map_or_else(Ending::Failed, |()| Ending::Completed),
            }
        };
        tokio::select! {
            ending = bounded => ending,
            _ = tokio::signal::ctrl_c() => Ending::Interrupted,
        }
    };

    answering.abort();
    if lost(&ending) {
        settle(&inspector).await;
    }
    stop(&inspector).await;

    // A spawn failure reaches `initialize` as a connection that went away; the
    // diagnostic says why, which is the sentence worth printing.
    let diagnostics = inspector.diagnostics().snapshot();
    let ending = match ending {
        Ending::Failed(Failure::Handshake(_)) => diagnostics
            .0
            .iter()
            .find_map(|diagnostic| match &diagnostic.kind {
                DiagnosticKind::SpawnFailed { command, error } => {
                    Some(Failure::Spawn(format!("`{command}`: {error}")))
                }
                _ => None,
            })
            .map_or(ending, Ending::Failed),
        ending => ending,
    };

    // Annotations are read after the stop, because one of them — a cancelled
    // turn nobody resolved — is decidable only once the connection is gone.
    let entries = inspector.timeline().entries();
    let annotations = entries
        .iter()
        .filter_map(|entry| match &entry.kind {
            EntryKind::Annotation(annotation) => Some(annotation.clone()),
            _ => None,
        })
        .collect();
    for (index, turn) in turns.iter_mut().enumerate() {
        fill(turn, index as u64 + 1, &entries);
    }

    let export = inspector.trace().export();
    let written = run
        .export
        .as_deref()
        .map(|path| write_export(&export, path).map_err(|error| error.to_string()));

    let answers = std::mem::take(&mut *answers.lock().unwrap_or_else(|p| p.into_inner()));
    Report {
        command: spawn.command_line(),
        policy: run.permission,
        ending,
        agent: inspector.agent(),
        session: inspector.session(),
        turns,
        permissions: answers.permissions,
        elicitations: answers.elicitations,
        annotations,
        diagnostics: diagnostics.0,
        diagnostics_dropped: diagnostics.1,
        frames: export.frames(),
        dropped: export.dropped(),
        export: written,
    }
}

/// The spawn the options describe. Built directly rather than through
/// `AgentCommand`, whose one-argument-per-line fields are a form's: a command
/// line already has its arguments split, and trimming them would change them.
fn factory(run: &Run) -> StdioSpawn {
    let mut spawn = StdioSpawn::new(&run.command).args(&run.args);
    if run.clear_env {
        spawn = spawn.env_clear();
    }
    for name in &run.unset {
        spawn = spawn.env_remove(name);
    }
    spawn = spawn.envs(run.env.iter().map(|(key, value)| (key, value)));
    match &run.cwd {
        Some(cwd) => spawn.cwd(cwd),
        None => spawn,
    }
}

/// The absolute path ACP wants for `session/new`: `--cwd` resolved against
/// this process's directory, which is also where the agent is started.
fn session_cwd(cwd: Option<&Path>) -> Result<PathBuf, String> {
    let cwd = cwd.unwrap_or(Path::new(""));
    let current = std::env::current_dir().map_err(|error| error.to_string())?;
    Ok(current.join(cwd))
}

/// The handshake and the turns, stopping at the first thing that fails.
async fn drive(
    inspector: &Inspector,
    spawn: &StdioSpawn,
    cwd: PathBuf,
    prompts: &[String],
    turns: &mut Vec<TurnReport>,
) -> Result<(), Failure> {
    inspector.connect(spawn);
    inspector.initialize().await.map_err(Failure::Handshake)?;
    inspector
        .new_session(cwd, None)
        .await
        .map_err(Failure::Session)?;

    for (index, prompt) in prompts.iter().enumerate() {
        turns.push(TurnReport {
            prompt: prompt.clone(),
            outcome: None,
            message: String::new(),
            tool_calls: Vec::new(),
        });
        let outcome = inspector.prompt(prompt).await;
        if let Some(turn) = turns.last_mut() {
            turn.outcome = Some(outcome.clone());
        }
        outcome.map_err(|error| Failure::Prompt {
            turn: index + 1,
            error,
        })?;
    }
    Ok(())
}

/// Whether the run failed because the connection went away — which a call can
/// learn before the reader has recorded why.
fn lost(ending: &Ending) -> bool {
    matches!(
        ending,
        Ending::Failed(
            Failure::Handshake(CallError::Disconnected)
                | Failure::Session(CallError::Disconnected)
                | Failure::Prompt {
                    error: CallError::Disconnected,
                    ..
                }
        )
    )
}

/// Gives the reader a moment to record why the connection ended (a failed
/// spawn, an exit status) before stopping it — stopping first would cut the
/// evidence off.
async fn settle(inspector: &Inspector) {
    let mut changes = inspector.status_changes();
    let _ = tokio::time::timeout(SETTLING, async {
        while inspector.status() == ConnectionStatus::Connected {
            if changes.next().await.is_none() {
                return;
            }
        }
    })
    .await;
}

/// Stops the agent the way the desktop's Disconnect does — stdin closed, then
/// SIGTERM, then SIGKILL — and waits for core to say which step did it.
async fn stop(inspector: &Inspector) {
    let live = inspector.status() == ConnectionStatus::Connected;
    let mut changes = inspector.diagnostics().changes();
    inspector.disconnect_gracefully();
    if live {
        let stopped = || {
            inspector
                .diagnostics()
                .entries()
                .iter()
                .any(|diagnostic| matches!(diagnostic.kind, DiagnosticKind::Stopped { .. }))
        };
        let _ = tokio::time::timeout(STOPPING, async {
            while !stopped() {
                if changes.next().await.is_none() {
                    return;
                }
            }
        })
        .await;
    }
    // Whatever is still stopping is killed now, rather than outliving us.
    inspector.disconnect();
}

/// What the policy answered on the person's behalf.
#[derive(Default)]
struct Answers {
    permissions: Vec<Answered>,
    elicitations: usize,
}

/// Answers every blocking request as it arrives, for as long as the run lasts.
///
/// Permission requests by `policy`; elicitations are cancelled — dismissed
/// without a choice — because a form nobody can see has nobody to fill it in,
/// and saying "declined" would claim a decision nobody made.
async fn answer(inspector: Inspector, policy: Policy, answers: Arc<Mutex<Answers>>) {
    let mut changes = inspector.timeline().changes();
    let mut asked = HashSet::new();
    let mut elicited = HashSet::new();
    loop {
        let pending = inspector.pending_permissions();
        asked.retain(|id: &String| {
            pending
                .iter()
                .any(|request| request.id().to_string() == *id)
        });
        for request in pending {
            if !asked.insert(request.id().to_string()) {
                continue;
            }
            let answered = respond(&inspector, policy, &request).await;
            lock(&answers).permissions.push(answered);
        }

        let pending = inspector.pending_elicitations();
        elicited.retain(|id: &String| {
            pending
                .iter()
                .any(|request| request.id().to_string() == *id)
        });
        for request in pending {
            if !elicited.insert(request.id().to_string()) {
                continue;
            }
            if request.cancel().await.is_ok() {
                lock(&answers).elicitations += 1;
            }
        }

        if changes.next().await.is_none() {
            return;
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Answers one permission request by `policy`.
async fn respond(inspector: &Inspector, policy: Policy, request: &PermissionRequest) -> Answered {
    let options = request.options();
    let choice = match policy {
        Policy::Allow => options
            .iter()
            .find(|option| option.kind == v1::PermissionOptionKind::AllowOnce)
            .or_else(|| rejection(options)),
        Policy::Ask | Policy::Reject => rejection(options),
    };

    let answer = match choice {
        Some(option) => match request.select(&option.option_id).await {
            Ok(()) => Answer::Selected {
                option: option.option_id.to_string(),
                kind: spelt(&option.kind),
            },
            Err(error) => Answer::Failed(error.to_string()),
        },
        // Core will not answer `cancelled` on its own say-so, because only a
        // cancelled turn makes that true — so the turn is cancelled.
        None => match inspector.cancel().await {
            Ok(()) => Answer::CancelledTurn,
            Err(error) => Answer::Failed(error.to_string()),
        },
    };

    let tool_call = request.tool_call();
    Answered {
        tool_call: tool_call.tool_call_id.to_string(),
        title: tool_call.fields.title.clone(),
        offered: options
            .iter()
            .map(|option| (option.option_id.to_string(), spelt(&option.kind)))
            .collect(),
        answer,
    }
}

/// The first reject option the agent offered, in its order.
fn rejection(options: &[v1::PermissionOption]) -> Option<&v1::PermissionOption> {
    options.iter().find(|option| {
        matches!(
            option.kind,
            v1::PermissionOptionKind::RejectOnce | v1::PermissionOptionKind::RejectAlways
        )
    })
}

/// Fills a turn's message and tool calls from the timeline entries stamped
/// with its id.
fn fill(turn: &mut TurnReport, id: u64, entries: &[acp_inspector_core::TimelineEntry]) {
    for entry in entries.iter().filter(|entry| entry.turn == Some(id)) {
        let EntryKind::Update(update) = &entry.kind else {
            continue;
        };
        match update {
            v1::SessionUpdate::AgentMessageChunk(chunk) => {
                if let v1::ContentBlock::Text(text) = &chunk.content {
                    turn.message.push_str(&text.text);
                }
            }
            v1::SessionUpdate::ToolCall(call) => turn.tool_calls.push(ToolCallReport {
                id: call.tool_call_id.to_string(),
                title: Some(call.title.clone()),
                kind: Some(spelt(&call.kind)),
                status: Some(spelt(&call.status)),
            }),
            v1::SessionUpdate::ToolCallUpdate(update) => turn.tool_calls.push(ToolCallReport {
                id: update.tool_call_id.to_string(),
                title: update.fields.title.clone(),
                kind: update.fields.kind.as_ref().map(spelt),
                status: update.fields.status.as_ref().map(spelt),
            }),
            _ => {}
        }
    }
}

/// A protocol value as the wire spells it.
pub fn spelt(value: &impl serde::Serialize) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(text)) => text,
        Ok(other) => other.to_string(),
        Err(error) => format!("<{error}>"),
    }
}

/// Writes the export to `path`, or into it when it is a directory.
///
/// An explicit path is replaced rather than renamed around — a CI job writes
/// the same artefact name every run — but never half-written: the document goes
/// to a sibling first and is renamed over. On Unix the file is `0600`, like
/// every export, because nothing in it is redacted.
fn write_export(export: &Export, path: &Path) -> io::Result<PathBuf> {
    if path.is_dir() {
        return export.save_in(path);
    }
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not a file name"))?;
    let mut partial = name.to_owned();
    partial.push(format!(".{}.partial", std::process::id()));
    let partial = path.with_file_name(partial);

    let written = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&partial)?;
        write!(file, "{export}")?;
        file.sync_all()?;
        std::fs::rename(&partial, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    written.map(|()| path.to_path_buf())
}
