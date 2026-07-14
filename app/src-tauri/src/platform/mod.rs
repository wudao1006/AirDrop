mod clipboard_monitor;
#[cfg(any(target_os = "windows", target_os = "linux"))]
mod extended_clipboard;

#[derive(Default)]
pub(crate) struct SystemClipboardContent {
    pub(crate) text: Option<String>,
    pub(crate) rich: Option<(String, Option<String>, Option<String>)>,
    pub(crate) image: Option<(Vec<u8>, u32, u32)>,
    pub(crate) files: Vec<String>,
}

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

#[cfg(mobile)]
pub(crate) use clipboard_monitor::ClipboardMonitorHandle;
pub(crate) use clipboard_monitor::{read_system_clipboard, start_clipboard_monitor};
#[cfg(any(target_os = "windows", target_os = "linux"))]
pub(crate) use extended_clipboard::{
    write_file_clipboard, write_rich_clipboard, ExtendedClipboard,
};

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub(crate) struct ExtendedClipboard;

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
impl ExtendedClipboard {
    pub(crate) fn new() -> Result<Self, String> {
        Err("当前平台暂不支持读取富文本剪贴板".into())
    }

    pub(crate) fn read_content(&self) -> Result<SystemClipboardContent, String> {
        Err("当前平台暂不支持读取富文本和文件剪贴板".into())
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub(crate) fn write_rich_clipboard(
    _text: String,
    _html: Option<String>,
    _rtf: Option<String>,
) -> Result<(), String> {
    Err("当前平台暂不支持写入富文本剪贴板".into())
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub(crate) fn write_file_clipboard(_files: Vec<String>) -> Result<(), String> {
    Err("当前平台暂不支持写入文件剪贴板".into())
}

pub(crate) fn clipboard_poll_interval() -> std::time::Duration {
    current::clipboard_poll_interval()
}

pub(crate) fn platform_name() -> &'static str {
    current::platform_name()
}

#[cfg(target_os = "android")]
pub(crate) fn device_name() -> String {
    current::device_name()
}

#[cfg(not(target_os = "android"))]
pub(crate) fn device_name() -> String {
    let name = gethostname::gethostname()
        .to_string_lossy()
        .trim()
        .to_string();
    if name.is_empty() {
        "AirDrop Device".into()
    } else {
        name
    }
}

#[cfg(mobile)]
pub(crate) fn suspend_mobile_runtime(app: &tauri::AppHandle) {
    use tauri::Manager;

    app.state::<ClipboardMonitorHandle>().set_active(false);
    app.state::<crate::core::discovery::DiscoveryHandle>()
        .suspend();
    app.state::<crate::core::transport::TransportHandle>()
        .suspend(app.clone());
    let state = app.state::<crate::core::service::ServiceState>();
    let _ = crate::core::service::set_mobile_activity(&state, app, "suspended");
}

#[cfg(mobile)]
pub(crate) fn resume_mobile_runtime(app: &tauri::AppHandle) {
    use tauri::Manager;

    let state = app.state::<crate::core::service::ServiceState>();
    let _ = crate::core::service::set_mobile_activity(&state, app, "reconnecting");
    app.state::<crate::core::transport::TransportHandle>()
        .resume();
    if let Err(error) = app
        .state::<crate::core::discovery::DiscoveryHandle>()
        .resume(app.clone())
    {
        tracing::warn!(error = %error, "LAN discovery unavailable after mobile resume");
    }
    app.state::<ClipboardMonitorHandle>().set_active(true);
    let _ = crate::core::service::set_mobile_activity(&state, app, "foreground_live");
}
