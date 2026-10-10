//! What a background task's panic becomes (C8).
//!
//! The pumps and the inspector's reader run as detached tasks. A panic in one
//! would otherwise end only that task: the status would stay Connected while
//! every call waited on a reader that is gone. Each task is run through
//! [`caught`], and a panic is reported the way a transport failure is — a
//! diagnostic that moves the connection to Lost and fails the pending calls.

use std::any::Any;
use std::future::Future;
use std::io;
use std::panic::AssertUnwindSafe;

use futures_util::FutureExt;

/// Runs `task`, and answers with what it said if it panicked.
///
/// The task's state is not looked at again after a panic: the connection it
/// served is torn down, which is why asserting unwind safety is sound here.
pub(crate) async fn caught<F: Future>(task: F) -> Result<F::Output, String> {
    AssertUnwindSafe(task)
        .catch_unwind()
        .await
        .map_err(|payload| said(payload.as_ref()))
}

/// A panic's message, where it carried one.
fn said(payload: &(dyn Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|said| (*said).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic with no message".to_owned())
}

/// The error a panicked task is reported with.
pub(crate) fn internal_error(what: &str) -> io::Error {
    io::Error::other(format!("internal error: {what} (see crash log)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_panic_is_caught_with_its_message() {
        assert_eq!(caught(async { 3 }).await, Ok(3));
        assert_eq!(
            caught(async { panic!("the pump fell over") }).await,
            Err::<(), _>("the pump fell over".to_owned())
        );
        let formatted = caught(async { panic!("frame {}", 7) }).await;
        assert_eq!(formatted, Err::<(), _>("frame 7".to_owned()));
        assert_eq!(
            internal_error("frame 7").to_string(),
            "internal error: frame 7 (see crash log)"
        );
    }
}
