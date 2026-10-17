mod commands;

use tauri::{Emitter, Manager};

use commands::{LOG_EVENT, READY_EVENT};

pub fn run() {
    petrichor_core::logging::init(false);

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .invoke_handler(tauri::generate_handler![
            commands::get_version,
            commands::get_system_info,
            commands::get_logs,
            commands::clear_logs,
            commands::demo_event,
        ])
        .setup(|app| {
            petrichor_core::logging::notice(format!(
                "Petrichor {} démarré",
                petrichor_core::version()
            ));
            if let Some(entry) = petrichor_core::logging::logger().entries().last().cloned() {
                let _ = app.emit(READY_EVENT, entry.clone());
                let _ = app.emit(LOG_EVENT, entry);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erreur au lancement de Petrichor");
}
