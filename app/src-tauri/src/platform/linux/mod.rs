use std::time::Duration;

// X11, Wayland and desktop-portal clipboard integration live here.
pub(super) fn clipboard_poll_interval() -> Duration {
    Duration::from_millis(700)
}

pub(super) fn platform_name() -> &'static str {
    "linux"
}
