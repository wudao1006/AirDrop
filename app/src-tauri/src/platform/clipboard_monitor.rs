use crate::{core::service, platform};
use std::thread;
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub(crate) fn start_clipboard_monitor(app: AppHandle) {
    thread::spawn(move || {
        let mut previous_text = None::<String>;
        let mut previous_image = None::<[u8; 32]>;
        let mut initialized = false;
        let mut consecutive_failures = 0_u8;
        let mut failure_reported = false;
        loop {
            thread::sleep(platform::clipboard_poll_interval());
            let text_result = app.clipboard().read_text();
            let image_result = app
                .clipboard()
                .read_image()
                .map(|image| (image.rgba().to_vec(), image.width(), image.height()));
            if text_result.is_ok() || image_result.is_ok() {
                consecutive_failures = 0;
                if failure_reported {
                    let state = app.state::<service::ServiceState>();
                    let _ = service::report_clipboard_recovered(&state, &app);
                    failure_reported = false;
                }
            } else {
                consecutive_failures = consecutive_failures.saturating_add(1);
                if consecutive_failures >= 3 && !failure_reported {
                    let state = app.state::<service::ServiceState>();
                    let _ = service::report_clipboard_failure(
                        &state,
                        &app,
                        "暂时无法读取系统剪贴板中的文本或图片".into(),
                    );
                    failure_reported = true;
                }
                continue;
            }

            let current_text = text_result.ok().filter(|text| !text.trim().is_empty());
            let current_image = image_result.ok().and_then(|(rgba, width, height)| {
                let expected = (width as usize)
                    .checked_mul(height as usize)
                    .and_then(|pixels| pixels.checked_mul(4));
                (expected == Some(rgba.len()) && !rgba.is_empty()).then(|| {
                    (
                        service::image_hash(&rgba, width, height),
                        rgba,
                        width,
                        height,
                    )
                })
            });

            if !initialized {
                previous_text = current_text;
                previous_image = current_image.as_ref().map(|(hash, ..)| *hash);
                initialized = true;
                continue;
            }

            let now = || {
                OffsetDateTime::now_utc()
                    .format(&Rfc3339)
                    .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
            };
            if let Some(text) = current_text.as_ref() {
                if previous_text.as_ref() != Some(text) {
                    let state = app.state::<service::ServiceState>();
                    let _ = service::capture_local_clipboard(&state, &app, text.clone(), now());
                }
            }
            if let Some((hash, rgba, width, height)) = current_image.as_ref() {
                if previous_image.as_ref() != Some(hash) {
                    let state = app.state::<service::ServiceState>();
                    let _ = service::capture_local_image(
                        &state,
                        &app,
                        rgba.clone(),
                        *width,
                        *height,
                        now(),
                    );
                }
            }
            previous_text = current_text;
            previous_image = current_image.map(|(hash, ..)| hash);
        }
    });
}
