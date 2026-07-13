use crate::{core::service, platform};
use std::thread;
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub(crate) fn start_clipboard_monitor(app: AppHandle) {
    thread::spawn(move || {
        let mut previous = None::<String>;
        let mut initialized = false;
        let mut consecutive_failures = 0_u8;
        let mut failure_reported = false;
        loop {
            thread::sleep(platform::clipboard_poll_interval());
            let text = match app.clipboard().read_text() {
                Ok(text) => {
                    consecutive_failures = 0;
                    if failure_reported {
                        let state = app.state::<service::ServiceState>();
                        let _ = service::report_clipboard_recovered(&state, &app);
                        failure_reported = false;
                    }
                    text
                }
                Err(error) => {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    if consecutive_failures >= 3 && !failure_reported {
                        let state = app.state::<service::ServiceState>();
                        let _ = service::report_clipboard_failure(
                            &state,
                            &app,
                            format!("暂时无法读取系统剪贴板：{error}"),
                        );
                        failure_reported = true;
                    }
                    continue;
                }
            };
            if !initialized {
                previous = Some(text);
                initialized = true;
                continue;
            }
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
