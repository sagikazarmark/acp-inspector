//! One export job for toolbar, palette and native menu. Snapshot synchronously;
//! serialize and write on the runtime's blocking pool.
use acp_inspector_core::{Export, Trace};
use std::path::PathBuf;

#[derive(Default)]
pub struct Work {
    pub pending: bool,
    pub result: Option<Result<PathBuf, String>>,
}

impl Work {
    pub fn begin(&mut self, trace: &Trace) -> Option<Export> {
        if self.pending {
            return None;
        }
        let snapshot = trace.export();
        self.pending = true;
        self.result = None;
        Some(snapshot)
    }

    pub fn finish(&mut self, result: Result<PathBuf, String>) {
        self.result = Some(result);
        self.pending = false;
    }
}

pub async fn write(snapshot: Export) -> Result<PathBuf, String> {
    in_background(move || snapshot.save()).await
}

async fn in_background(
    save: impl FnOnce() -> std::io::Result<PathBuf> + Send + 'static,
) -> Result<PathBuf, String> {
    tokio::task::spawn_blocking(save)
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn pending_job_does_not_block_executor_or_accept_duplicates_and_failure_can_retry() {
        let trace = Trace::default();
        let mut work = Work::default();
        let snapshot = work.begin(&trace).unwrap();
        assert!(work.pending);
        assert!(work.begin(&trace).is_none());
        let (release, wait) = std::sync::mpsc::channel();
        let (started, ready) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(in_background(move || {
            started.send(()).unwrap();
            wait.recv().unwrap();
            assert_eq!(snapshot.frames(), 0);
            Err(std::io::Error::other("disk probe"))
        }));
        // A single-thread Tokio executor can reach this while the disk worker waits.
        ready.await.unwrap();
        release.send(()).unwrap();
        work.finish(task.await.unwrap());
        assert!(!work.pending);
        assert_eq!(work.result, Some(Err("disk probe".into())));
        assert!(work.begin(&trace).is_some());
        assert!(work.result.is_none());
    }
}
