//! Timestamps for reading. Core stamps frames with a [`SystemTime`]; how one
//! looks is presentation. Local time, to match the agent's output in the user's
//! own terminal.

use std::time::SystemTime;

use jiff::Timestamp;
use jiff::tz::TimeZone;

/// `HH:MM:SS.mmm`, local. Milliseconds because a trace's interesting distances
/// are small; no date, since the export carries full timestamps.
pub fn stamp(at: SystemTime) -> String {
    let Ok(moment) = Timestamp::try_from(at) else {
        // A clock outside the calendar's range: unreachable, and not worth a
        // panic.
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
