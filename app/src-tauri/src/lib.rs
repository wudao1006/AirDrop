mod core;
mod platform;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let log_guard = core::logging::initialize(&data_dir).map_err(std::io::Error::other)?;
            app.manage(log_guard);
            let default_panic_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |panic| {
                tracing::error!(panic = %panic, "AirDrop process panicked");
                default_panic_hook(panic);
            }));
            let state =
                core::service::ServiceState::open(&data_dir).map_err(std::io::Error::other)?;
            app.manage(state);
            let transport =
                core::transport::start(app.handle().clone()).map_err(std::io::Error::other)?;
            app.manage(transport);
            tracing::info!(data_dir = %data_dir.display(), "AirDrop core started");
            platform::start_clipboard_monitor(app.handle().clone());
            if let Err(error) = core::discovery::start(app.handle().clone()) {
                tracing::warn!(error = %error, "LAN discovery unavailable");
            }
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
            core::service::allow_pairing,
            core::service::begin_pairing,
            core::service::confirm_pairing,
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
