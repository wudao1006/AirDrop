use std::time::Duration;

pub(super) fn clipboard_poll_interval() -> Duration {
    Duration::from_secs(1)
}

pub(super) fn platform_name() -> &'static str {
    "unknown"
}
