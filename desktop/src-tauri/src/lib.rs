mod service;

use std::{thread, time::Duration};
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

fn start_clipboard_monitor(app: AppHandle) {
    thread::spawn(move || {
        let mut previous = None::<String>;
        loop {
            thread::sleep(Duration::from_millis(700));
            let Ok(text) = app.clipboard().read_text() else { continue; };
            if text.trim().is_empty() || previous.as_ref() == Some(&text) { continue; }
            previous = Some(text.clone());
            let now = OffsetDateTime::now_utc().format(&Rfc3339).unwrap_or_else(|_| "1970-01-01T00:00:00Z".into());
            let state = app.state::<service::ServiceState>();
            let _ = service::capture_local_clipboard(&state, &app, text, now);
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(service::ServiceState::default())
        .setup(|app| {
            start_clipboard_monitor(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            service::get_snapshot,
            service::set_pause,
            service::set_synchronization_paused,
            service::set_app_activity,
            service::publish_local_clipboard,
            service::update_settings,
            service::create_import_intent,
            service::confirm_import,
            service::cancel_import,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build AirDrop desktop application");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::WindowEvent { label, event, .. } = event {
            if label == "main" && matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                app_handle.exit(0);
            }
        }
    });
}
