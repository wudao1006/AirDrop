use std::time::Duration;

pub(super) fn clipboard_poll_interval() -> Duration {
    Duration::from_secs(1)
}
