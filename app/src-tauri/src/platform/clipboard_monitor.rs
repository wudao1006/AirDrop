use crate::{core::service, platform};
use std::thread;
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub(crate) fn start_clipboard_monitor(app: AppHandle) {
    thread::spawn(move || {
        let mut previous = None::<String>;
        loop {
            thread::sleep(platform::clipboard_poll_interval());
            let Ok(text) = app.clipboard().read_text() else {
                continue;
            };
            if text.trim().is_empty() || previous.as_ref() == Some(&text) {
                continue;
            }
            previous = Some(text.clone());
            let now = OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into());
            let state = app.state::<service::ServiceState>();
            let _ = service::capture_local_clipboard(&state, &app, text, now);
        }
    });
}
