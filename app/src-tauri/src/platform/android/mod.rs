use std::time::Duration;

// Android clipboard access is foreground-scoped and resumes with the app lifecycle.
pub(super) fn clipboard_poll_interval() -> Duration {
    Duration::from_millis(900)
}
