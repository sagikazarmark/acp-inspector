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
    /// The invocation the live agent was launched from, not the form's current
    /// contents: new sessions open with it.
    pub launched: Signal<Option<AgentCommand>>,
    /// Whether a launch is in flight. Core says Connected once the transport
    /// exists, while `initialize` and `session/new` may still be running.
    pub launching: Signal<bool>,
    /// Why the recent list will not survive this window, if it will not.
    pub unsaved: Signal<Option<String>>,
    /// An invocation the first-run card asked the launch form to be filled with,
    /// numbered so choosing the same one twice refills twice.
    pub prefill: Signal<Option<(u64, AgentCommand)>>,
    /// Whether a gracefully disconnected agent is still stopping; a relaunch
    /// waits for it.
    pub stopping: Signal<bool>,
    // Where the diagnostics stood when the last launch began, so a failure is
    // told from what that launch wrote.
    launch_mark: Signal<usize>,
    /// The launched command once its agent has described itself. Not `Connected`
    /// alone: that is set optimistically, before the child is known to exist.
    pub live: Memo<Option<AgentCommand>>,
    pub launch: Callback<AgentCommand>,
    pub on_restart: EventHandler<()>,
    pub on_disconnect: EventHandler<()>,
}

impl Launch {
    /// Why the last launch did not reach an agent, from what that launch wrote.
    pub fn failure(
        &self,
        stores: Stores,
        problem: Signal<Option<CallError>>,
    ) -> Option<crate::spawn::LaunchFailure> {
        let held = stores.lines.read();
        let since = (self.launch_mark)().saturating_sub((stores.dropped_lines)());
        let launched = self.launched.read();
        let failure = crate::spawn::launch_failure(
            (stores.status)(),
            problem.read().as_ref(),
            stores.described.read().is_some(),
            held.get(since..).unwrap_or(&[]),
            effective_path(launched.as_ref()).as_deref(),
        )?;
        // A command not found on a PATH the login shell never supplied: say
        // so, since that PATH is the likely reason.
        #[cfg(feature = "desktop")]
        let failure = {
            let mut failure = failure;
            if failure.path.is_some() && !sets_path(launched.as_ref()) {
                failure.detail.extend(shell::login_path_missed());
            }
            failure
        };
        Some(failure)
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

    // Asks the agent to stop (stdin closed, then SIGTERM, then SIGKILL) and
    // moves focus to Restart.
    let stopping = inspector.clone();
    let on_disconnect = EventHandler::new(move |()| {
        stopping.disconnect_gracefully();
        watch_stop(stopping.clone(), stopping_agent);
        navigation::Destination::Control { id: rail::RESTART }.focus();
    });

    // Every launch comes here: the dialog, a recent row, Restart. Spawn,
    // `initialize` and `session/new` are one core call; the trace is kept.
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
            // Restart right after Disconnect waits for the graceful stop.
            inspector.stopped().await;
            let spawned = with_login_path(agent.clone()).await;
            let failure = inspector.start(&spawned).await.err();
            launching.set(false);
            // Remembered once an agent answered, even if the handshake then
            // failed (e.g. it wants a login). A command that never started is a
            // typo and is not remembered.
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
                // An agent answered, so the dialog's task is done.
                connect::close();
                if matches!(failure, Some(CallError::InvalidMcp(_))) {
                    mcp::correct_in_sessions();
                } else {
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

/// Marks `stopping` until the inspector's graceful stop is over.
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

/// The invocation as spawned: with the login shell's `PATH` where the reader
/// set none, since a Finder launch gets a minimal one. What is remembered is the
/// invocation as typed.
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

/// Whether the invocation set its own `PATH`.
#[cfg(feature = "desktop")]
fn sets_path(agent: Option<&AgentCommand>) -> bool {
    agent.is_some_and(|agent| agent.variables().any(|(name, _)| name == "PATH"))
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
