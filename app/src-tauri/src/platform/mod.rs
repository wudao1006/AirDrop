mod clipboard_monitor;

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(
    target_os = "android",
    target_os = "linux",
    target_os = "macos",
    target_os = "windows"
)))]
mod unsupported;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "android")]
use android as current;
#[cfg(target_os = "linux")]
use linux as current;
#[cfg(target_os = "macos")]
use macos as current;
#[cfg(not(any(
    target_os = "android",
    target_os = "linux",
    target_os = "macos",
    target_os = "windows"
)))]
use unsupported as current;
#[cfg(target_os = "windows")]
use windows as current;

pub(crate) use clipboard_monitor::start_clipboard_monitor;

pub(crate) fn clipboard_poll_interval() -> std::time::Duration {
    current::clipboard_poll_interval()
}
