use std::time::Duration;

// NSPasteboard, Vibrancy, menu-bar and macOS permission integration live here.
pub(super) fn clipboard_poll_interval() -> Duration {
    Duration::from_millis(500)
}

pub(super) fn platform_name() -> &'static str {
    "macos"
}
