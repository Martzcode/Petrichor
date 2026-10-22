use std::sync::Mutex;

use tauri::menu::{
    CheckMenuItemBuilder, ContextMenu, IsMenuItem, Menu, MenuBuilder, MenuEvent, MenuItemBuilder,
    SubmenuBuilder,
};
use tauri::{AppHandle, Manager, State, Wry};

use petrichor_core::{build_skin, SkinState, Variables};

use crate::commands::MeasureStore;
use crate::persistence;
use crate::windows::{self, SkinWindows};

const PREFIX: &str = "skin-menu:";

pub struct MenuState {
    pub(crate) skin: Mutex<Option<Menu<Wry>>>,
    pub(crate) tray: Mutex<Option<Menu<Wry>>>,
}

impl MenuState {
    pub fn new() -> Self {
        MenuState {
            skin: Mutex::new(None),
            tray: Mutex::new(None),
        }
    }
}

impl Default for MenuState {
    fn default() -> Self {
        Self::new()
    }
}

#[tauri::command]
pub fn show_skin_menu(
    app: AppHandle,
    windows: State<'_, SkinWindows>,
    measures: State<'_, MeasureStore>,
    menus: State<'_, MenuState>,
    label: String,
    x: Option<f64>,
    y: Option<f64>,
) -> Result<(), String> {
    let position = x.zip(y).map(|(x, y)| tauri::LogicalPosition::new(x, y));
    show_skin(&app, &windows, &measures, &menus, &label, position)
}

pub fn show_skin(
    app: &AppHandle,
    windows: &SkinWindows,
    measures: &MeasureStore,
    menus: &MenuState,
    label: &str,
    position: Option<tauri::LogicalPosition<f64>>,
) -> Result<(), String> {
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| format!("fenêtre introuvable : {label}"))?;
    let menu = build_skin_menu(app, windows, measures, label)?;

    if let Ok(mut slot) = menus.skin.lock() {
        *slot = Some(menu.clone());
    }

    match position {
        Some(position) => menu.popup_at(window.as_ref().window(), position),
        None => menu.popup(window.as_ref().window()),
    }
    .map_err(|error| error.to_string())
}

fn build_skin_menu(
    app: &AppHandle,
    windows: &SkinWindows,
    measures: &MeasureStore,
    label: &str,
) -> Result<Menu<Wry>, String> {
    let info = windows
        .info(label)
        .ok_or_else(|| format!("skin introuvable : {label}"))?;
    let state = persistence::load_state(&info.path).unwrap_or_default();
    let (skin, _) = build_skin(&info.path);

    let refresh = MenuItemBuilder::with_id(id(label, "refresh"), "Rafraîchir")
        .build(app)
        .map_err(|error| error.to_string())?;
    let edit = MenuItemBuilder::with_id(id(label, "edit"), "Éditer")
        .build(app)
        .map_err(|error| error.to_string())?;
    let manage = MenuItemBuilder::with_id(id(label, "manage"), "Gérer")
        .build(app)
        .map_err(|error| error.to_string())?;

    let normal = CheckMenuItemBuilder::with_id(id(label, "zpos:0"), "Normal")
        .checked(state.always_on_top == -1 || state.always_on_top == 0)
        .build(app)
        .map_err(|error| error.to_string())?;
    let top = CheckMenuItemBuilder::with_id(id(label, "zpos:1"), "Toujours au-dessus")
        .checked(state.always_on_top >= 1)
        .build(app)
        .map_err(|error| error.to_string())?;
    let bottom = CheckMenuItemBuilder::with_id(id(label, "zpos:-2"), "Sous le bureau")
        .checked(state.always_on_top <= -2)
        .build(app)
        .map_err(|error| error.to_string())?;
    let position = SubmenuBuilder::new(app, "Position")
        .item(&normal)
        .item(&top)
        .item(&bottom)
        .build()
        .map_err(|error| error.to_string())?;

    let mut transparency = SubmenuBuilder::new(app, "Transparence");
    for value in [255u8, 204, 153, 102, 51, 0] {
        let item = CheckMenuItemBuilder::with_id(
            id(label, &format!("alpha:{value}")),
            format!("{} %", (u16::from(value) * 100 / 255)),
        )
        .checked(state.alpha_value == value)
        .build(app)
        .map_err(|error| error.to_string())?;
        transparency = transparency.item(&item);
    }
    let transparency = transparency.build().map_err(|error| error.to_string())?;

    let hover_none = CheckMenuItemBuilder::with_id(id(label, "hover:0"), "Aucun")
        .checked(state.on_hover == 0)
        .build(app)
        .map_err(|error| error.to_string())?;
    let hover_hide = CheckMenuItemBuilder::with_id(id(label, "hover:1"), "Masquer")
        .checked(state.on_hover == 1)
        .build(app)
        .map_err(|error| error.to_string())?;
    let hover_fade = CheckMenuItemBuilder::with_id(id(label, "hover:2"), "Fondu")
        .checked(state.on_hover == 2)
        .build(app)
        .map_err(|error| error.to_string())?;
    let on_hover = SubmenuBuilder::new(app, "Survol")
        .item(&hover_none)
        .item(&hover_hide)
        .item(&hover_fade)
        .build()
        .map_err(|error| error.to_string())?;

    let click_through =
        CheckMenuItemBuilder::with_id(id(label, "toggle:clickthrough"), "Clic à travers")
            .checked(state.click_through)
            .build(app)
            .map_err(|error| error.to_string())?;
    let draggable = CheckMenuItemBuilder::with_id(id(label, "toggle:draggable"), "Déplaçable")
        .checked(state.draggable)
        .build(app)
        .map_err(|error| error.to_string())?;
    let keep = CheckMenuItemBuilder::with_id(id(label, "toggle:keep"), "Rester visible")
        .checked(state.keep_on_screen)
        .build(app)
        .map_err(|error| error.to_string())?;
    let snap = CheckMenuItemBuilder::with_id(id(label, "toggle:snap"), "Aligner aux bords")
        .checked(state.snap_edges)
        .build(app)
        .map_err(|error| error.to_string())?;
    let save = MenuItemBuilder::with_id(id(label, "save"), "Enregistrer la position")
        .build(app)
        .map_err(|error| error.to_string())?;

    let custom = custom_items(app, label, &skin, measures, &info.path)?;

    let unload = MenuItemBuilder::with_id(id(label, "unload"), "Décharger")
        .build(app)
        .map_err(|error| error.to_string())?;

    let mut builder = MenuBuilder::new(app)
        .item(&refresh)
        .item(&edit)
        .item(&manage)
        .separator()
        .item(&position)
        .item(&transparency)
        .item(&on_hover)
        .separator()
        .item(&click_through)
        .item(&draggable)
        .item(&keep)
        .item(&snap)
        .item(&save);

    if !custom.is_empty() {
        builder = builder.separator();
        for item in &custom {
            builder = builder.item(item);
        }
    }

    builder
        .separator()
        .item(&unload)
        .build()
        .map_err(|error| error.to_string())
}

fn custom_items(
    app: &AppHandle,
    label: &str,
    skin: &petrichor_core::Skin,
    measures: &MeasureStore,
    path: &std::path::Path,
) -> Result<Vec<tauri::menu::MenuItemKind<Wry>>, String> {
    let mut variables = Variables::new();
    for (name, value) in &skin.variables {
        variables.set(name, value);
    }
    if let Some(overlay) = measures.variables(path) {
        for (name, value) in overlay.to_raw_map() {
            variables.set(&name, &value);
        }
    }

    let mut items = Vec::new();
    for (index, item) in skin.settings.context_menu.iter().enumerate() {
        let title = variables.expand(&item.title);
        if title.trim() == "-" {
            let separator = tauri::menu::PredefinedMenuItem::separator(app)
                .map_err(|error| error.to_string())?;
            items.push(separator.kind());
            continue;
        }
        let command = MenuItemBuilder::with_id(id(label, &format!("custom:{index}")), title.trim())
            .enabled(!item.action.trim().is_empty())
            .build(app)
            .map_err(|error| error.to_string())?;
        items.push(command.kind());
    }
    Ok(items)
}

pub fn handle_event(app: &AppHandle, event: &MenuEvent) {
    let Some(rest) = event.id().as_ref().strip_prefix(PREFIX) else {
        return;
    };
    let Some((label, action)) = rest.split_once(':') else {
        return;
    };

    let result = match action {
        "refresh" => execute(app, label, "!Refresh"),
        "edit" => execute(app, label, "!EditSkin"),
        "manage" => execute(app, label, "!Manage"),
        "save" => {
            let windows = app.state::<SkinWindows>();
            windows::save_position(app, &windows, label)
        }
        "unload" => {
            let windows = app.state::<SkinWindows>();
            windows::close(app, &windows, label)
        }
        "toggle:clickthrough" => toggle(app, label, |state| state.click_through, "ClickThrough"),
        "toggle:draggable" => toggle(app, label, |state| state.draggable, "Draggable"),
        "toggle:keep" => toggle(app, label, |state| state.keep_on_screen, "KeepOnScreen"),
        "toggle:snap" => toggle(app, label, |state| state.snap_edges, "SnapEdges"),
        value if value.starts_with("zpos:") => {
            let value = value.trim_start_matches("zpos:");
            execute(app, label, &format!("!ZPos {value}"))
        }
        value if value.starts_with("alpha:") => {
            let value = value.trim_start_matches("alpha:");
            execute(app, label, &format!("!SetTransparency {value}"))
        }
        value if value.starts_with("hover:") => {
            let value = value
                .trim_start_matches("hover:")
                .parse::<u8>()
                .unwrap_or(0);
            set_on_hover(app, label, value.min(2))
        }
        value if value.starts_with("custom:") => {
            let index = value.trim_start_matches("custom:").parse::<usize>().ok();
            custom_action(app, label, index)
        }
        _ => Ok(()),
    };

    if let Err(error) = result {
        petrichor_core::logging::warning(format!(
            "Échec de l'action de menu pour {label} : {error}"
        ));
    }
}

fn set_on_hover(app: &AppHandle, label: &str, value: u8) -> Result<(), String> {
    let windows = app.state::<SkinWindows>();
    let info = windows
        .info(label)
        .ok_or_else(|| format!("skin introuvable : {label}"))?;
    persistence::update_state(&info.path, |state| state.on_hover = i32::from(value));
    crate::bangs::emit(app, label, crate::bangs::SkinCommand::OnHover { value })
}

fn custom_action(app: &AppHandle, label: &str, index: Option<usize>) -> Result<(), String> {
    let Some(index) = index else {
        return Ok(());
    };
    let windows = app.state::<SkinWindows>();
    let info = windows
        .info(label)
        .ok_or_else(|| format!("skin introuvable : {label}"))?;
    let (skin, _) = build_skin(&info.path);
    let Some(item) = skin.settings.context_menu.get(index) else {
        return Ok(());
    };
    if item.action.trim().is_empty() {
        return Ok(());
    }
    let measures = app.state::<MeasureStore>();
    crate::bangs::execute_chain(app, &windows, &measures, Some(label), &item.action);
    Ok(())
}

fn toggle(
    app: &AppHandle,
    label: &str,
    flag: impl FnOnce(&SkinState) -> bool,
    bang: &str,
) -> Result<(), String> {
    let windows = app.state::<SkinWindows>();
    let info = windows
        .info(label)
        .ok_or_else(|| format!("skin introuvable : {label}"))?;
    let state = persistence::load_state(&info.path).unwrap_or_default();
    let value = if flag(&state) { 0 } else { 1 };
    execute(app, label, &format!("!{bang} {value}"))
}

fn execute(app: &AppHandle, label: &str, command: &str) -> Result<(), String> {
    let windows = app.state::<SkinWindows>();
    let measures = app.state::<MeasureStore>();
    crate::bangs::execute_chain(app, &windows, &measures, Some(label), command);
    Ok(())
}

fn id(label: &str, action: &str) -> String {
    format!("{PREFIX}{label}:{action}")
}
