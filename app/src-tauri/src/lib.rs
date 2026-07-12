mod core;
mod platform;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(core::service::ServiceState::default())
        .setup(|app| {
            platform::start_clipboard_monitor(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            core::service::get_snapshot,
            core::service::set_pause,
            core::service::set_synchronization_paused,
            core::service::set_app_activity,
            core::service::publish_local_clipboard,
            core::service::update_settings,
            core::service::create_import_intent,
            core::service::confirm_import,
            core::service::cancel_import,
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
