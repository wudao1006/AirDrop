use crate::{core::service, platform};
use std::thread;
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub(crate) fn start_clipboard_monitor(app: AppHandle) {
    thread::spawn(move || {
        let mut previous_text = None::<String>;
        let mut previous_image = None::<[u8; 32]>;
        let mut previous_rich = None::<[u8; 32]>;
        let mut previous_files = None::<[u8; 32]>;
        let extended_clipboard = match platform::ExtendedClipboard::new() {
            Ok(clipboard) => Some(clipboard),
            Err(error) => {
                let state = app.state::<service::ServiceState>();
                let _ = service::report_clipboard_limitation(&state, &app, error);
                None
            }
        };
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
            let current_rich = extended_clipboard
                .as_ref()
                .and_then(platform::ExtendedClipboard::read_rich);
            let current_files = extended_clipboard
                .as_ref()
                .map(platform::ExtendedClipboard::read_files)
                .unwrap_or_default();
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
                previous_rich = current_rich.as_ref().map(|(text, html, rtf)| {
                    service::rich_hash(text, html.as_deref(), rtf.as_deref())
                });
                previous_files =
                    (!current_files.is_empty()).then(|| service::file_list_hash(&current_files));
                initialized = true;
                continue;
            }

            let now = || {
                OffsetDateTime::now_utc()
                    .format(&Rfc3339)
                    .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
            };
            if !current_files.is_empty() {
                let hash = service::file_list_hash(&current_files);
                if previous_files != Some(hash) {
                    let state = app.state::<service::ServiceState>();
                    let _ =
                        service::capture_local_files(&state, &app, current_files.clone(), now());
                }
            } else if let Some((text, html, rtf)) = current_rich.as_ref() {
                let hash = service::rich_hash(text, html.as_deref(), rtf.as_deref());
                if previous_rich != Some(hash) {
                    let state = app.state::<service::ServiceState>();
                    let _ = service::capture_local_rich(
                        &state,
                        &app,
                        text.clone(),
                        html.clone(),
                        rtf.clone(),
                        now(),
                    );
                }
            } else if let Some(text) = current_text.as_ref() {
                if previous_text.as_ref() != Some(text) {
                    let state = app.state::<service::ServiceState>();
                    let _ = service::capture_local_clipboard(&state, &app, text.clone(), now());
                }
            }
            if current_files.is_empty() {
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
            }
            previous_text = current_text;
            previous_image = current_image.map(|(hash, ..)| hash);
            previous_rich = current_rich.map(|(text, html, rtf)| {
                service::rich_hash(&text, html.as_deref(), rtf.as_deref())
            });
            previous_files =
                (!current_files.is_empty()).then(|| service::file_list_hash(&current_files));
        }
    });
}
