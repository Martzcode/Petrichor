mod commands;
mod windows;

use tauri::{Emitter, Manager};

use commands::{LOG_EVENT, READY_EVENT};

pub fn run() {
    petrichor_core::logging::init(false);

    tauri::Builder::default()
        .manage(windows::SkinWindows::new())
        .manage(commands::MeasureStore::new())
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
            commands::scan_skins,
            commands::default_skin_root,
            commands::load_skin,
            commands::load_render_model,
            commands::read_measures,
            commands::frontend_log,
            commands::demo_event,
            windows::toggle_skin_window,
            windows::close_skin_window,
            windows::skin_window_info,
            windows::list_skin_windows,
            windows::apply_skin_window_flags,
            windows::fit_skin_window,
            windows::show_skin_window,
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

            #[cfg(debug_assertions)]
            if let Ok(path) = std::env::var("PETRICHOR_OPEN_SKIN") {
                let state = app.state::<windows::SkinWindows>();
                match windows::open_window(app.handle(), &state, path.clone().into()) {
                    Ok(label) => petrichor_core::logging::notice(format!(
                        "Skin de démonstration {path} ouvert sous {label}"
                    )),
                    Err(error) => petrichor_core::logging::error(format!(
                        "Échec d'ouverture de {path} : {error}"
                    )),
                }
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erreur au lancement de Petrichor");
}
