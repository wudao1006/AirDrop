use std::time::Duration;

// Windows-specific clipboard formats, Acrylic, tray and startup integration live here.
pub(super) fn clipboard_poll_interval() -> Duration {
    Duration::from_millis(450)
}
