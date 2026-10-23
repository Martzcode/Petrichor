mod bangs;
mod commands;
mod gamemode;
mod layouts;
mod manage;
mod menu;
mod notifications;
mod packages;
mod persistence;
mod selection;
mod storage;
mod tray;
mod tray_measure;
mod updates;
mod windows;

use tauri::{Emitter, Manager};

use commands::{LOG_EVENT, READY_EVENT};

pub fn run() {
    petrichor_core::logging::init(false);

    tauri::Builder::default()
        .manage(windows::SkinWindows::new())
        .manage(commands::MeasureStore::new())
        .manage(menu::MenuState::new())
        .manage(selection::SelectionState::new())
        .manage(tray_measure::TrayMeasureState::new())
        .manage(gamemode::GameModeState::new())
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
            let selection = app.state::<selection::SelectionState>();
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
                    let _ = selection.window_moved(app, &windows, window.label(), *position);
                }
                tauri::WindowEvent::Destroyed => {
                    if let Some(info) = windows.info(window.label()) {
                        app.state::<commands::MeasureStore>().remove(&info.path);
                    }
                    selection.remove(window.label());
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
            manage::manage_snapshot,
            manage::skin_details,
            manage::update_skin_state,
            manage::set_skin_favorite,
            manage::set_layout_favorite,
            manage::settings,
            manage::save_settings,
            manage::reset_statistics,
            manage::delete_log_file,
            manage::open_log_folder,
            manage::open_skin_folder,
            manage::open_skin_editor,
            manage::create_skin,
            layouts::save_layout,
            layouts::delete_layout,
            layouts::edit_layout,
            layouts::load_layout,
            gamemode::game_mode_status,
            gamemode::set_game_mode_config,
            gamemode::toggle_game_mode,
            packages::inspect_package,
            packages::create_package,
            packages::install_package,
            updates::check_updates,
            updates::download_update,
            commands::frontend_log,
            commands::demo_event,
            windows::toggle_skin_window,
            windows::close_skin_window,
            windows::skin_window_info,
            windows::list_skin_windows,
            windows::apply_skin_window_flags,
            windows::fit_skin_window,
            windows::show_skin_window,
            windows::hide_skin_window,
            windows::save_skin_position,
            windows::set_skin_dragging,
            selection::select_skin,
            selection::selection_active,
            menu::show_skin_menu,
        ])
        .setup(|app| {
            app.on_menu_event(|app, event| {
                tray::handle_menu_event(app, &event);
                menu::handle_event(app, &event);
            });
            if let Err(error) = tray::setup(app.handle()) {
                petrichor_core::logging::warning(format!("Icône de tray indisponible : {error}"));
            } else {
                tray_measure::spawn(app.handle());
            }
            gamemode::spawn(app.handle().clone());
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
