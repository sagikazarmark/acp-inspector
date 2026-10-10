//! Launching an agent, restarting it, and letting it go.

use acp_inspector_core::{AgentCommand, CallError, ConnectionStatus, Inspector};
use dioxus::prelude::*;

use super::claims::{AuthenticationWork, Lifecycle};
use super::kept::Kept;
use super::stores::Stores;
#[cfg(feature = "desktop")]
use crate::shell;
use crate::{connect, mcp, navigation, rail};

/// The launch's own state, and the three ways a connection starts or ends.
#[derive(Clone, Copy)]
pub(crate) struct Launch {
    // The invocation the live agent was launched from, kept for the one thing
    // that needs it after the launch: a session created on demand is opened in
    // the same working directory the handshake's was (§7.5), and that is the
    // form's answer rather than this window's. The launched command and not the
    // form's current contents — the fields are editable while an agent runs,
    // and the agent that is running is the one being asked.
    pub launched: Signal<Option<AgentCommand>>,
    // The launch handshake is longer than the Connection transition: core says
    // Connected as soon as the transport exists, while initialize and
    // session/new may still be in flight. Keep that interval explicit in the
    // launch control rather than calling it Running before the launch returns.
    pub launching: Signal<bool>,
    // Why the list will not survive this window, if it will not. Empty on every
    // machine with a home directory, which is why it is a note under the form
    // rather than a screen.
    pub unsaved: Signal<Option<String>>,
    // A remembered invocation the first-run card asked the launch form to be
    // filled with, numbered so choosing the same one twice refills twice.
    pub prefill: Signal<Option<(u64, AgentCommand)>>,
    // Whether an agent this window let go of gracefully is still stopping
    // (F6): the rail says so, and a relaunch waits for it.
    pub stopping: Signal<bool>,
    // Where the diagnostics stood when the last launch began, so a failed
    // launch is told from what that launch wrote and nothing older (X1).
    launch_mark: Signal<usize>,
    // And the invocation the form folds *around*: the launched command, once
    // the agent it started has described itself. Not `Connected` on its own —
    // that status is set optimistically, before the child is known to exist, so
    // a typo would fold the form and re-open it a frame later. This is the same
    // mark the launch handler already uses to decide an invocation is worth
    // remembering.
    pub live: Memo<Option<AgentCommand>>,
    pub launch: Callback<AgentCommand>,
    pub on_restart: EventHandler<()>,
    pub on_disconnect: EventHandler<()>,
}

impl Launch {
    // Why the last launch did not reach an agent, from what that launch wrote
    // (X1). Said inside the Connect dialog, which stays open for it.
    pub fn failure(
        &self,
        stores: Stores,
        problem: Signal<Option<CallError>>,
    ) -> Option<crate::spawn::LaunchFailure> {
        let held = stores.lines.read();
        let since = (self.launch_mark)().saturating_sub((stores.dropped_lines)());
        crate::spawn::launch_failure(
            (stores.status)(),
            problem.read().as_ref(),
            stores.described.read().is_some(),
            held.get(since..).unwrap_or(&[]),
            effective_path(self.launched.read().as_ref()).as_deref(),
        )
    }
}

pub(crate) fn use_launch(
    inspector: &Inspector,
    stores: Stores,
    kept: Kept,
    lifecycle: Lifecycle,
) -> Launch {
    let Lifecycle {
        mut auth_work,
        mut listing_work,
        mut failed,
        mut problem,
    } = lifecycle;
    let Kept {
        recent,
        mut remembered,
        ..
    } = kept;
    let mut unsaved = use_signal(|| None::<String>);
    let prefill = use_signal(|| None::<(u64, AgentCommand)>);
    let stopping_agent = use_signal(|| false);
    let mut launched = use_signal(|| None::<AgentCommand>);
    let mut launching = use_signal(|| false);
    let mut launch_mark = use_signal(|| 0usize);
    let status = stores.status;
    let described = stores.described;
    let live = use_memo(move || {
        if status() == ConnectionStatus::Connected && described.read().is_some() {
            launched()
        } else {
            None
        }
    });

    // Ending the connection, from wherever it is asked for: the agent is asked
    // to stop (stdin closed, then SIGTERM, then SIGKILL — F6) rather than
    // killed, and the rail says "Stopping…" until it has. Focus goes to the
    // way back, Restart, rather than to the document (X6).
    let stopping = inspector.clone();
    let on_disconnect = EventHandler::new(move |()| {
        stopping.disconnect_gracefully();
        watch_stop(stopping.clone(), stopping_agent);
        navigation::Destination::Control { id: rail::RESTART }.focus();
    });

    // The one way an agent is launched: the dialog's Launch, a recent row's
    // play button, Restart and "Restart with these settings" all come here
    // (X1, X4). Spawn, `initialize`, `session/new` — one call, because the
    // order of the three is core's business and not a window's (§5). Starting
    // over a live agent replaces it; the trace is kept.
    let starting = inspector.clone();
    let lines = stores.lines;
    let dropped_lines = stores.dropped_lines;
    let launch = use_callback(move |agent: AgentCommand| {
        if *launching.peek() || !mcp::valid() {
            return;
        }
        let inspector = starting.clone();
        launching.set(true);
        problem.set(None);
        unsaved.set(None);
        failed.set(None);
        listing_work.with_mut(|(connection, pending)| {
            *connection += 1;
            *pending = 0;
        });
        auth_work.with_mut(AuthenticationWork::reset);
        launch_mark.set(*dropped_lines.peek() + lines.peek().len());
        launched.set(Some(agent.clone()));
        spawn(async move {
            // An agent this window disconnected is gone before the next one
            // starts (F6): Restart right after Disconnect waits for the stop.
            inspector.stopped().await;
            let spawned = with_login_path(agent.clone()).await;
            let failure = inspector.start(&spawned).await.err();
            launching.set(false);
            // Remembered because an agent answered for itself — the command
            // exists, it ran, and it speaks ACP — even if the handshake failed
            // after that: an agent that will not open a session until somebody
            // logs in (§7.1) is exactly the case. A command that never started
            // never said anything; that is a typo, and it is still in the
            // fields, with the reason above them.
            if inspector.agent().is_some() {
                let remembering = recent.peek().clone();
                if let Some(remembering) = remembering {
                    let invocation = agent.clone();
                    if let Ok((error, entries)) = tokio::task::spawn_blocking(move || {
                        let error = remembering
                            .record(&invocation)
                            .err()
                            .map(|error| error.to_string());
                        (error, remembering.entries())
                    })
                    .await
                    {
                        unsaved.set(error);
                        remembered.set(entries);
                    }
                }
                // The one thing that shuts the dialog which is not the reader:
                // an agent answered, so the task it was open for is done.
                connect::close();
                if matches!(failure, Some(CallError::InvalidMcp(_))) {
                    mcp::correct_in_sessions();
                } else {
                    // And the keyboard goes where the next thing is typed (X6).
                    navigation::Destination::Prompt.focus();
                }
            } else {
                connect::show_failure();
            }
            problem.set(failure);
        });
    });
    let on_restart = EventHandler::new(move |()| {
        if let Some(command) = launched.peek().clone() {
            launch.call(command);
        }
    });

    Launch {
        launched,
        launching,
        unsaved,
        prefill,
        stopping: stopping_agent,
        launch_mark,
        live,
        launch,
        on_restart,
        on_disconnect,
    }
}

/// Marks `stopping` until the inspector's graceful stop is over (F6).
pub(crate) fn watch_stop(inspector: Inspector, mut stopping: Signal<bool>) {
    if !inspector.is_stopping() {
        return;
    }
    stopping.set(true);
    spawn(async move {
        inspector.stopped().await;
        stopping.set(false);
    });
}

/// The invocation as it is spawned: with the login shell's `PATH` where the
/// app's own is the minimal one a Finder launch gets and the reader did not
/// set one (X2). What is remembered and shown is the invocation as typed.
async fn with_login_path(agent: AgentCommand) -> AgentCommand {
    #[cfg(feature = "desktop")]
    {
        let mut agent = agent;
        if agent.variables().all(|(name, _)| name != "PATH")
            && let Some(path) = shell::login_path().await
        {
            if !agent.env.trim().is_empty() {
                agent.env.push('\n');
            }
            agent.env.push_str(&format!("PATH={path}"));
        }
        agent
    }
    #[cfg(not(feature = "desktop"))]
    agent
}

/// The `PATH` a bare command name is looked up in, for the launch failure
/// alert: the invocation's own, else the login shell's, else this process's.
fn effective_path(agent: Option<&AgentCommand>) -> Option<String> {
    agent
        .and_then(|agent| {
            agent
                .variables()
                .find(|(name, _)| *name == "PATH")
                .map(|(_, value)| value.to_owned())
        })
        .or_else(|| {
            #[cfg(feature = "desktop")]
            {
                shell::effective_path()
            }
            #[cfg(not(feature = "desktop"))]
            {
                std::env::var("PATH").ok()
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spawn_is_given_the_login_path_only_where_none_was_set() {
        let set = AgentCommand {
            command: "npx".to_owned(),
            args: String::new(),
            env: "PATH=/mine".to_owned(),
            cwd: String::new(),
            ..AgentCommand::default()
        };
        assert_eq!(effective_path(Some(&set)).as_deref(), Some("/mine"));
    }
}
