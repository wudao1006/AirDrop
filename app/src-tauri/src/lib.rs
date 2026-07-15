mod core;
mod platform;

#[cfg(desktop)]
use tauri::Emitter;
use tauri::Manager;
#[cfg(desktop)]
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default().plugin(tauri_plugin_clipboard_manager::init());
    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    if let Some(window) = app.get_webview_window("floating-orb") {
                        let _ = window.show();
                        let _ = window.unminimize();
                        let _ = window.set_focus();
                        let _ = window.emit("airdrop://orb-open-menu", serde_json::json!({}));
                    } else if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.unminimize();
                        let _ = window.set_focus();
                        let _ = window.emit("airdrop://open-clipboard", ());
                    }
                })
                .build(),
        );
    let app = builder
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
            #[cfg(desktop)]
            {
                let shortcut = state
                    .configured_global_shortcut()
                    .map_err(std::io::Error::other)?;
                if let Err(error) = app.global_shortcut().register(shortcut.as_str()) {
                    tracing::warn!(shortcut, error = %error, "global clipboard shortcut unavailable");
                }
            }
            app.manage(state);
            let transport =
                core::transport::start(app.handle().clone()).map_err(std::io::Error::other)?;
            app.manage(transport);
            tracing::info!(data_dir = %data_dir.display(), "AirDrop core started");
            let clipboard_monitor = platform::start_clipboard_monitor(app.handle().clone());
            app.manage(clipboard_monitor);
            let discovery = core::discovery::DiscoveryHandle::start(app.handle().clone());
            if let Err(error) = discovery.resume(app.handle().clone()) {
                tracing::warn!(error = %error, "LAN discovery unavailable");
            }
            app.manage(discovery);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            core::service::get_snapshot,
            core::transport::get_telemetry,
            core::transport::set_telemetry_observing,
            core::service::copy_diagnostic_report,
            core::service::set_pause,
            core::service::set_synchronization_paused,
            core::service::set_app_activity,
            core::service::publish_local_clipboard,
            core::service::publish_current_clipboard,
            core::service::update_settings,
            core::service::set_global_shortcut,
            core::service::create_import_intent,
            core::service::confirm_import,
            core::service::cancel_import,
            core::service::allow_pairing,
            core::service::begin_pairing,
            core::service::confirm_pairing,
            core::service::set_device_sync_enabled,
            core::service::set_local_device_name,
            core::service::set_device_alias,
            core::service::revoke_device,
            core::service::create_sync_group,
            core::service::confirm_group_invite,
            core::service::set_group_member_direction,
            core::service::remove_group_member,
            core::service::update_group_policy,
            core::service::leave_sync_group,
            core::service::delete_sync_group,
            core::service::prepare_slot_drag,
            core::service::release_slot_drag,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build AirDrop desktop application");

    app.run(|app_handle, event| {
        if matches!(&event, tauri::RunEvent::Exit) {
            if let Err(error) = app_handle
                .state::<core::transport::TransportHandle>()
                .flush_telemetry_history()
            {
                tracing::warn!(error = %error, "transfer history flush failed during shutdown");
            }
            app_handle
                .state::<core::discovery::DiscoveryHandle>()
                .suspend();
        }
        if let tauri::RunEvent::WindowEvent {
            label: window_label,
            event,
            ..
        } = event
        {
            #[cfg(desktop)]
            if window_label == "main" && matches!(event, tauri::WindowEvent::CloseRequested { .. })
            {
                app_handle.exit(0);
            }
            #[cfg(mobile)]
            let _ = window_label;
            #[cfg(mobile)]
            match event {
                tauri::WindowEvent::Suspended => {
                    platform::suspend_mobile_runtime(app_handle);
                }
                tauri::WindowEvent::Resumed => {
                    platform::resume_mobile_runtime(app_handle);
                }
                _ => {}
            }
        }
    });
}
