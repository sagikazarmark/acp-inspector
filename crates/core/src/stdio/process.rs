//! OS ownership below the framed transport. Termination is synchronous, so it
//! also works during native event-loop exit and when Tokio drops its tasks.

use std::io;
use std::process::ExitStatus;
use std::sync::{Arc, Mutex};
use tokio::process::{ChildStderr, ChildStdin, ChildStdout, Command};

pub(super) struct Process(Mutex<platform::Process>);

pub(super) struct Spawned {
    pub process: Arc<Process>,
    pub stdin: ChildStdin,
    pub stdout: ChildStdout,
    pub stderr: ChildStderr,
}

impl Process {
    pub fn spawn(command: Command) -> io::Result<Spawned> {
        let (process, stdin, stdout, stderr) = platform::Process::spawn(command)?;
        Ok(Spawned {
            process: Arc::new(Self(Mutex::new(process))),
            stdin,
            stdout,
            stderr,
        })
    }

    pub fn terminate(&self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).terminate();
    }

    pub async fn wait(&self) -> io::Result<ExitStatus> {
        loop {
            let status = self.0.lock().unwrap_or_else(|e| e.into_inner()).try_wait();
            match status {
                Ok(Some(status)) => return Ok(status),
                Ok(None) => {}
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
            // waitid(WNOWAIT) has no portable async readiness API. Poll just
            // the one launcher; never wait on the pipe readers or descendants.
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
}

/// Held by the watch future even before its first poll. Other owners may still
/// exist when runtime shutdown drops that future.
pub(super) struct WatchOwner(pub Arc<Process>);

impl Drop for WatchOwner {
    fn drop(&mut self) {
        self.0.terminate();
    }
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::os::unix::process::CommandExt;

    pub struct Process {
        child: Option<std::process::Child>,
        terminated: bool,
    }

    impl Process {
        pub fn spawn(
            mut command: Command,
        ) -> io::Result<(Self, ChildStdin, ChildStdout, ChildStderr)> {
            // std owns the child: Tokio must not reap the group leader before
            // we signal its group. Keeping it unreaped pins the numeric PGID.
            let child = command.as_std_mut().process_group(0).spawn()?;
            let mut process = Self {
                child: Some(child),
                terminated: false,
            };
            let child = process.child.as_mut().unwrap();
            let stdin = ChildStdin::from_std(child.stdin.take().unwrap())?;
            let stdout = ChildStdout::from_std(child.stdout.take().unwrap())?;
            let stderr = ChildStderr::from_std(child.stderr.take().unwrap())?;
            Ok((process, stdin, stdout, stderr))
        }

        pub fn terminate(&mut self) {
            if !self.terminated {
                if let Some(child) = &self.child {
                    // SAFETY: this positive PID is our unreaped child and the
                    // leader of the private group created before exec. Never
                    // signal this numeric group again after reaping its leader.
                    unsafe {
                        libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
                    }
                }
                self.terminated = true;
            }
        }

        pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
            let child = self.child.as_ref().unwrap();
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            // SAFETY: valid zero-initialized output; WNOWAIT observes exit
            // without releasing the PID. Available on Linux and macOS.
            let result = unsafe {
                libc::waitid(
                    libc::P_PID,
                    child.id(),
                    &mut info,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            };
            if result == -1 {
                return Err(io::Error::last_os_error());
            }
            if unsafe { info.si_pid() } == 0 {
                return Ok(None);
            }
            // Descendants may still hold both pipes. End them before draining
            // the bytes already written; their inherited handles now reach EOF.
            self.terminate();
            self.child.as_mut().unwrap().try_wait()
        }
    }

    impl Drop for Process {
        fn drop(&mut self) {
            self.terminate();
            if let Some(mut child) = self.child.take() {
                // Reaping must not hold up disconnect or runtime destruction.
                // This short-lived OS thread does not need an async runtime.
                if !matches!(child.try_wait(), Ok(Some(_))) {
                    let _ =
                        std::thread::Builder::new()
                            .name("agent-reap".into())
                            .spawn(move || {
                                let _ = child.wait();
                            });
                }
            }
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use process_wrap::tokio::{ChildWrapper, CommandWrap, JobObject, KillOnDrop};

    pub struct Process(Box<dyn ChildWrapper>);

    impl Process {
        pub fn spawn(command: Command) -> io::Result<(Self, ChildStdin, ChildStdout, ChildStderr)> {
            // JobObject suspends creation, assigns the private job, then
            // resumes. Descendants cannot run before ownership is established.
            // KillOnDrop sets KILL_ON_JOB_CLOSE, including on setup failure.
            let mut command = CommandWrap::from(command);
            command.wrap(KillOnDrop).wrap(JobObject);
            let mut process = Self(command.spawn()?);
            let stdin = process.0.stdin().take().unwrap();
            let stdout = process.0.stdout().take().unwrap();
            let stderr = process.0.stderr().take().unwrap();
            Ok((process, stdin, stdout, stderr))
        }

        pub fn terminate(&mut self) {
            let _ = self.0.start_kill();
        }

        pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
            // Wait for the launcher, not for the job: a descendant holding
            // inherited pipes must not prevent natural exit from being seen.
            let status = self.0.inner_mut().try_wait()?;
            if status.is_some() {
                self.terminate();
            }
            Ok(status)
        }
    }
}
