mod clipboard_monitor;
#[cfg(any(target_os = "windows", target_os = "linux"))]
mod extended_clipboard;

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

    pub(crate) fn read_rich(&self) -> Option<(String, Option<String>, Option<String>)> {
        None
    }

    pub(crate) fn read_files(&self) -> Vec<String> {
        Vec::new()
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
