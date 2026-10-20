mod bangs;
mod commands;
mod persistence;
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
        .on_window_event(|window, event| {
            if !window.label().starts_with("skin-") {
                return;
            }
            let app = window.app_handle();
            let windows = app.state::<windows::SkinWindows>();
            match event {
                tauri::WindowEvent::Moved(position) => {
                    if windows.is_dragging(window.label())
                        && windows.allow_move_save(window.label())
                    {
                        if let Some(info) = windows.info(window.label()) {
                            persistence::update_state(&info.path, |state| {
                                state.window_x = position.x;
                                state.window_y = position.y;
                                state.save_position = true;
                            });
                        }
                    }
                }
                tauri::WindowEvent::Destroyed => {
                    if let Some(info) = windows.info(window.label()) {
                        app.state::<commands::MeasureStore>().remove(&info.path);
                    }
                    windows.remove(window.label());
                }
                _ => {}
            }
        })
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
            commands::execute_bang,
            commands::get_paths,
            commands::frontend_log,
            commands::demo_event,
            windows::toggle_skin_window,
            windows::close_skin_window,
            windows::skin_window_info,
            windows::list_skin_windows,
            windows::apply_skin_window_flags,
            windows::fit_skin_window,
            windows::show_skin_window,
            windows::save_skin_position,
            windows::set_skin_dragging,
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

            #[cfg(any(debug_assertions, feature = "dev-harness"))]
            if let Ok(value) = std::env::var("PETRICHOR_OPEN_SKIN") {
                for path in value
                    .split(',')
                    .map(str::trim)
                    .filter(|path| !path.is_empty())
                {
                    let state = app.state::<windows::SkinWindows>();
                    match windows::open_window(app.handle(), &state, path.into()) {
                        Ok(label) => petrichor_core::logging::notice(format!(
                            "Skin de démonstration {path} ouvert sous {label}"
                        )),
                        Err(error) => petrichor_core::logging::error(format!(
                            "Échec d'ouverture de {path} : {error}"
                        )),
                    }
                }
            }

            #[cfg(any(debug_assertions, feature = "dev-harness"))]
            if let Ok(command) = std::env::var("PETRICHOR_TEST_BANG") {
                if let Some((label, bang)) = command.split_once('|') {
                    let windows = app.state::<windows::SkinWindows>();
                    let measures = app.state::<commands::MeasureStore>();
                    bangs::execute_chain(
                        app.handle(),
                        &windows,
                        &measures,
                        Some(label.trim()),
                        bang.trim(),
                    );
                }
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erreur au lancement de Petrichor");
}
