use tauri::menu::{
    CheckMenuItemBuilder, ContextMenu, Menu, MenuBuilder, MenuEvent, MenuItemBuilder,
    SubmenuBuilder,
};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use petrichor_core::{ApplicationConfig, Registry};

use crate::commands;
use crate::layouts;
use crate::menu::MenuState;
use crate::selection::SelectionState;
use crate::windows::{self, SkinWindows};

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| tauri::Error::AssetNotFound("icône de tray".into()))?;

    TrayIconBuilder::with_id("petrichor")
        .icon(icon)
        .tooltip("Petrichor")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            let app = tray.app_handle();
            match event {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => show_main(app),
                TrayIconEvent::Click {
                    button: MouseButton::Right,
                    button_state: MouseButtonState::Up,
                    ..
                } => run_tray_action(app, "TrayExecuteR"),
                TrayIconEvent::Click {
                    button: MouseButton::Middle,
                    button_state: MouseButtonState::Up,
                    ..
                } => run_tray_action(app, "TrayExecuteM"),
                TrayIconEvent::DoubleClick {
                    button: MouseButton::Right,
                    ..
                } => run_tray_action(app, "TrayExecuteDR"),
                TrayIconEvent::DoubleClick {
                    button: MouseButton::Middle,
                    ..
                } => run_tray_action(app, "TrayExecuteDM"),
                _ => {}
            }
        })
        .build(app)?;

    Ok(())
}

pub fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let show = MenuItemBuilder::with_id("show", "Afficher Petrichor").build(app)?;
    let hide_skins = MenuItemBuilder::with_id("hide-skins", "Masquer les skins").build(app)?;
    let show_skins = MenuItemBuilder::with_id("show-skins", "Afficher les skins").build(app)?;
    let selection = app.state::<SelectionState>();
    let selection_mode = CheckMenuItemBuilder::with_id("selection", "Mode sélection")
        .checked(selection.is_active())
        .build(app)?;
    let logs = MenuItemBuilder::with_id("logs", "Journal").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quitter").build(app)?;

    let mut builder = MenuBuilder::new(app);
    builder = builder.item(&show);

    let layout_names = layouts::list();
    if !layout_names.is_empty() {
        let mut submenu = SubmenuBuilder::new(app, "Layouts");
        for name in layout_names {
            let item = MenuItemBuilder::with_id(format!("layout:{name}"), &name).build(app)?;
            submenu = submenu.item(&item);
        }
        let submenu = submenu.build()?;
        builder = builder.item(&submenu);
    }

    builder = builder.separator();

    if let Some(root) = commands::skin_root() {
        let registry = Registry::scan(root.to_string_lossy().to_string());
        for folder in &registry.folders {
            if folder.files.is_empty() {
                continue;
            }
            let mut submenu = SubmenuBuilder::new(app, &folder.name);
            for file in &folder.files {
                let item = MenuItemBuilder::with_id(
                    format!("skin:{}", file.path.display()),
                    &file.filename,
                )
                .build(app)?;
                submenu = submenu.item(&item);
            }
            let submenu = submenu.build()?;
            builder = builder.item(&submenu);
        }
        builder = builder.separator();
    }

    builder
        .item(&show_skins)
        .item(&hide_skins)
        .separator()
        .item(&selection_mode)
        .separator()
        .item(&logs)
        .separator()
        .item(&quit)
        .build()
}

pub fn show_menu(app: &AppHandle, label: Option<&str>) -> Result<(), String> {
    let window = label
        .and_then(|label| app.get_webview_window(label))
        .or_else(|| app.get_webview_window("main"))
        .ok_or_else(|| "aucune fenêtre pour afficher le menu tray".to_string())?;
    let menu = build_menu(app).map_err(|error| error.to_string())?;
    let state = app.state::<MenuState>();
    if let Ok(mut slot) = state.tray.lock() {
        *slot = Some(menu.clone());
    }
    menu.popup(window.as_ref().window())
        .map_err(|error| error.to_string())
}

pub fn handle_menu_event(app: &AppHandle, event: &MenuEvent) {
    let id = event.id().as_ref();
    match id {
        "show" | "logs" => show_main(app),
        "selection" => {
            let windows = app.state::<SkinWindows>();
            let selection = app.state::<SelectionState>();
            if let Err(error) = selection.toggle(app, &windows) {
                petrichor_core::logging::error(format!(
                    "Échec du basculement du mode sélection : {error}"
                ));
            }
        }
        "quit" => app.exit(0),
        "show-skins" => {
            let windows = app.state::<SkinWindows>();
            for info in windows.list() {
                let _ = windows::show(app, &info.label);
            }
        }
        "hide-skins" => {
            let windows = app.state::<SkinWindows>();
            for info in windows.list() {
                let _ = windows::hide(app, &info.label);
            }
        }
        other => {
            if let Some(name) = other.strip_prefix("layout:") {
                let windows = app.state::<SkinWindows>();
                if let Err(error) = layouts::load(app, &windows, name) {
                    petrichor_core::logging::error(format!(
                        "Échec du chargement du layout {name} : {error}"
                    ));
                    crate::notifications::send(
                        "Petrichor",
                        &format!("Impossible de charger le layout « {name} » : {error}"),
                    );
                }
            } else if let Some(path) = other.strip_prefix("skin:") {
                let windows = app.state::<SkinWindows>();
                if let Err(error) = windows::toggle_window(app, &windows, path.into()) {
                    petrichor_core::logging::error(format!(
                        "Échec du basculement du skin depuis le tray : {error}"
                    ));
                }
            }
        }
    }
}

fn run_tray_action(app: &AppHandle, key: &str) {
    let Some(config_path) = crate::persistence::config_path() else {
        return;
    };
    let Ok(config) = ApplicationConfig::load(&config_path) else {
        return;
    };
    let global = config.global();
    let action = match key {
        "TrayExecuteR" => global.tray_execute_r,
        "TrayExecuteM" => global.tray_execute_m,
        "TrayExecuteDR" => global.tray_execute_dr,
        "TrayExecuteDM" => global.tray_execute_dm,
        _ => None,
    };
    let Some(action) = action.filter(|value| !value.trim().is_empty()) else {
        return;
    };
    let windows = app.state::<SkinWindows>();
    let measures = app.state::<commands::MeasureStore>();
    crate::bangs::execute_chain(app, &windows, &measures, None, &action);
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}
