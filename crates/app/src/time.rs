//! Timestamps, for reading.
//!
//! Core stamps every frame and every diagnostic with a [`SystemTime`] and stops
//! there (`docs/architecture.md` §6.2): what a moment *looks like* is
//! presentation, and it belongs on the side of the boundary that knows who is
//! looking. Local time, not UTC — these lines sit beside the same agent's
//! output in the user's own terminal, and two clocks would make comparing them
//! a subtraction.

use std::time::SystemTime;

use jiff::Timestamp;
use jiff::tz::TimeZone;

/// `HH:MM:SS.mmm`, local.
///
/// Milliseconds because the interesting distances in a trace are small: which
/// notification arrived before which, and how long the agent sat on a request.
/// The date is left off — an inspector session is a sitting, not an archive,
/// and the export is what carries a full timestamp (§10).
pub fn stamp(at: SystemTime) -> String {
    let Ok(moment) = Timestamp::try_from(at) else {
        // A clock outside the range of a calendar. Unreachable in practice, and
        // still not worth a panic in a window whose whole job is to show what
        // happened.
        return "--:--:--.---".to_owned();
    };

    let local = moment.to_zoned(TimeZone::system());
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        local.hour(),
        local.minute(),
        local.second(),
        local.millisecond(),
    )
}
