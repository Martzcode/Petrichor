use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use petrichor_core::{ApplicationConfig, GlobalSettings, Metadata, SkinState};
use serde::{Deserialize, Serialize};
use tauri::{Manager, PhysicalPosition};

use crate::commands;
use crate::gamemode::GameModeStatus;
use crate::layouts::{self, LayoutInfo};
use crate::persistence;
use crate::storage;
use crate::windows::{self, SkinWindowInfo, SkinWindows};

#[derive(Debug, Clone, Serialize)]
pub struct SkinDetails {
    pub path: PathBuf,
    pub name: String,
    pub folder: String,
    pub metadata: Metadata,
    pub state: SkinState,
    pub favorites: bool,
    pub open_label: Option<String>,
    pub compat: petrichor_compat::CompatReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManageSnapshot {
    pub version: String,
    pub skins: petrichor_core::Registry,
    pub windows: Vec<SkinWindowInfo>,
    pub layouts: Vec<LayoutInfo>,
    pub settings: GlobalSettings,
    pub game_mode: GameModeStatus,
    pub paths: commands::AppPaths,
    pub monitors: Vec<MonitorInfo>,
    pub capabilities: Vec<petrichor_compat::Capability>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MonitorInfo {
    pub index: usize,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub primary: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SkinStatePatch {
    pub always_on_top: Option<i32>,
    pub click_through: Option<bool>,
    pub draggable: Option<bool>,
    pub keep_on_screen: Option<bool>,
    pub snap_edges: Option<bool>,
    pub save_position: Option<bool>,
    pub auto_select_screen: Option<bool>,
    pub on_hover: Option<i32>,
    pub alpha_value: Option<u8>,
    pub fade_duration: Option<u32>,
    pub active: Option<i32>,
    pub load_order: Option<i32>,
    pub window_x: Option<i32>,
    pub window_y: Option<i32>,
    pub anchor_x: Option<f32>,
    pub anchor_y: Option<f32>,
    pub start_hidden: Option<bool>,
    pub monitor: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewSkinSpec {
    pub name: String,
    pub author: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct NewSkinResult {
    pub path: PathBuf,
    pub folder: PathBuf,
}

#[tauri::command]
pub fn manage_snapshot(app: tauri::AppHandle) -> Result<ManageSnapshot, String> {
    let root =
        commands::skin_root().ok_or_else(|| "répertoire des skins introuvable".to_string())?;
    let mut registry = petrichor_core::Registry::scan(root.to_string_lossy().to_string());
    storage::apply_favorites(&mut registry);
    apply_active_variants(&mut registry);
    let windows = app.state::<SkinWindows>();
    Ok(ManageSnapshot {
        version: petrichor_core::version().to_string(),
        skins: registry,
        windows: windows.list(),
        layouts: layouts::list_info(),
        settings: settings(),
        game_mode: crate::gamemode::game_mode_status(app.clone()),
        paths: commands::get_paths(),
        monitors: monitor_infos(&app),
        capabilities: petrichor_compat::detect_capabilities(),
    })
}

#[tauri::command]
pub fn skin_details(path: String, app: tauri::AppHandle) -> Result<SkinDetails, String> {
    let path = PathBuf::from(path);
    if !path.is_file() {
        return Err(format!("skin introuvable : {}", path.display()));
    }
    let (skin, _) = petrichor_core::build_skin(&path);
    let compat = petrichor_compat::analyze_skin(&skin);
    let state = persistence::load_state(&path).unwrap_or_default();
    let windows = app.state::<SkinWindows>();
    Ok(SkinDetails {
        name: skin.name,
        folder: skin.folder,
        metadata: skin.metadata,
        state,
        favorites: storage::is_skin_favorite(&path),
        open_label: windows.find_by_path(&path),
        compat,
        path,
    })
}

#[tauri::command]
pub fn update_skin_state(
    path: String,
    patch: SkinStatePatch,
    app: tauri::AppHandle,
) -> Result<SkinState, String> {
    let path = PathBuf::from(path);
    let mut state = persistence::load_state(&path).unwrap_or_default();
    if let Some(value) = patch.always_on_top {
        state.always_on_top = value;
    }
    if let Some(value) = patch.click_through {
        state.click_through = value;
    }
    if let Some(value) = patch.draggable {
        state.draggable = value;
    }
    if let Some(value) = patch.keep_on_screen {
        state.keep_on_screen = value;
    }
    if let Some(value) = patch.snap_edges {
        state.snap_edges = value;
    }
    if let Some(value) = patch.save_position {
        state.save_position = value;
    }
    if let Some(value) = patch.auto_select_screen {
        state.auto_select_screen = value;
    }
    if let Some(value) = patch.on_hover {
        state.on_hover = value.clamp(0, 2);
    }
    if let Some(value) = patch.alpha_value {
        state.alpha_value = value;
    }
    if let Some(value) = patch.fade_duration {
        state.fade_duration = value;
    }
    if let Some(value) = patch.active {
        state.active = value.max(0);
    }
    if let Some(value) = patch.load_order {
        state.load_order = Some(value);
    }
    if let Some(value) = patch.window_x {
        state.window_x = value;
    }
    if let Some(value) = patch.window_y {
        state.window_y = value;
    }
    if let Some(value) = patch.anchor_x {
        state.anchor_x = value;
    }
    if let Some(value) = patch.anchor_y {
        state.anchor_y = value;
    }
    if let Some(value) = patch.start_hidden {
        state.start_hidden = value;
    }

    let windows = app.state::<SkinWindows>();
    let label = windows.find_by_path(&path);
    if let Some(index) = patch.monitor {
        let monitors = app.available_monitors().unwrap_or_default();
        if let Some(monitor) = monitors.get(index) {
            let size = label
                .as_deref()
                .and_then(|label| app.get_webview_window(label))
                .and_then(|window| window.outer_size().ok())
                .map(|size| (size.width as i32, size.height as i32))
                .unwrap_or((0, 0));
            let position = monitor.position();
            let monitor_size = monitor.size();
            state.window_x = position.x + (monitor_size.width as i32 - size.0).max(0) / 2;
            state.window_y = position.y + (monitor_size.height as i32 - size.1).max(0) / 2;
        }
    }

    let saved = state.clone();
    persistence::update_state(&path, move |target| *target = saved);

    if let Some(label) = label {
        let flags = commands::window_flags(&path);
        windows::apply_flags(&app, &windows, &label, flags)?;
        if let Some(window) = app.get_webview_window(&label) {
            let _ = window.set_position(PhysicalPosition::new(state.window_x, state.window_y));
        }
        let _ = crate::bangs::emit(
            &app,
            &label,
            crate::bangs::SkinCommand::Opacity {
                value: state.alpha_value,
            },
        );
        let _ = crate::bangs::emit(
            &app,
            &label,
            crate::bangs::SkinCommand::OnHover {
                value: state.on_hover.clamp(0, 2) as u8,
            },
        );
    }
    Ok(state)
}

#[tauri::command]
pub fn set_active_variant(path: String, app: tauri::AppHandle) -> Result<(), String> {
    let path = PathBuf::from(&path);
    let folder = path
        .parent()
        .ok_or_else(|| "dossier du skin introuvable".to_string())?;
    let entries = std::fs::read_dir(folder).map_err(|error| error.to_string())?;
    for entry in entries.flatten() {
        let candidate = entry.path();
        if candidate
            .extension()
            .is_none_or(|extension| !extension.eq_ignore_ascii_case("ini"))
        {
            continue;
        }
        let active = candidate == path;
        persistence::update_state(&candidate, |state| {
            state.active = if active { 1 } else { 0 };
        });
    }
    let windows = app.state::<SkinWindows>();
    if let Some(label) = windows.find_by_path(&path) {
        let _ = crate::bangs::emit(&app, &label, crate::bangs::SkinCommand::Refresh);
    }
    Ok(())
}

fn monitor_infos(app: &tauri::AppHandle) -> Vec<MonitorInfo> {
    let primary = app
        .primary_monitor()
        .ok()
        .flatten()
        .and_then(|monitor| monitor.name().cloned());
    app.available_monitors()
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(index, monitor)| {
            let position = monitor.position();
            let size = monitor.size();
            MonitorInfo {
                index,
                name: monitor
                    .name()
                    .unwrap_or(&format!("Écran {}", index + 1))
                    .to_string(),
                x: position.x,
                y: position.y,
                width: size.width as i32,
                height: size.height as i32,
                primary: primary.as_deref() == monitor.name().map(String::as_str),
            }
        })
        .collect()
}

#[tauri::command]
pub fn set_skin_favorite(path: String, favorite: bool) -> Result<(), String> {
    storage::set_skin_favorite(Path::new(&path), favorite)
}

#[tauri::command]
pub fn set_layout_favorite(name: String, favorite: bool) -> Result<(), String> {
    storage::set_layout_favorite(&name, favorite)
}

#[tauri::command]
pub fn settings() -> GlobalSettings {
    persistence::config_path()
        .and_then(|path| ApplicationConfig::load(&path).ok())
        .map(|config| config.global())
        .unwrap_or_default()
}

#[tauri::command]
pub fn save_settings(app: tauri::AppHandle, settings: GlobalSettings) -> Result<(), String> {
    let path = persistence::config_path().ok_or_else(|| "configuration introuvable".to_string())?;
    let mut config = ApplicationConfig::load(&path).unwrap_or_default();
    config.set_global(&settings);
    config.save(&path).map_err(|error| error.to_string())?;
    if let Some(tray) = app.tray_by_id("petrichor") {
        let _ = tray.set_visible(settings.tray_icon);
    }
    Ok(())
}

#[tauri::command]
pub fn reset_statistics() -> Result<usize, String> {
    let Some(base) = std::env::var_os("PETRICHOR_STATE_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_STATE_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
    else {
        return Ok(0);
    };
    let directory = base.join("petrichor");
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Ok(0);
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
            && std::fs::remove_file(path).is_ok()
        {
            removed += 1;
        }
    }
    Ok(removed)
}

#[tauri::command]
pub fn delete_log_file() -> Result<(), String> {
    let path = crate::persistence::config_path()
        .as_deref()
        .and_then(Path::parent)
        .map(petrichor_core::logging::default_log_path)
        .ok_or_else(|| "chemin du journal introuvable".to_string())?;
    if path.is_file() {
        std::fs::remove_file(&path).map_err(|error| error.to_string())?;
    }
    petrichor_core::logging::logger().clear();
    Ok(())
}

#[tauri::command]
pub fn open_log_folder() -> Result<(), String> {
    let path = crate::persistence::config_path()
        .as_deref()
        .and_then(Path::parent)
        .map(petrichor_core::logging::default_log_path)
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .ok_or_else(|| "dossier du journal introuvable".to_string())?;
    open_path(&path)
}

#[tauri::command]
pub fn open_skin_folder(path: String) -> Result<(), String> {
    let path = PathBuf::from(path);
    let folder = if path.is_dir() {
        path
    } else {
        path.parent().map(Path::to_path_buf).unwrap_or(path)
    };
    open_path(&folder)
}

#[tauri::command]
pub fn open_skin_editor(path: String) -> Result<(), String> {
    let path = PathBuf::from(path);
    if !path.is_file() {
        return Err(format!("skin introuvable : {}", path.display()));
    }
    let settings = settings();
    let editor = settings
        .skin_editor
        .clone()
        .filter(|editor| !editor.trim().is_empty());
    match editor {
        Some(editor) => spawn(&editor, &[path.to_string_lossy().to_string()]),
        None => spawn("xdg-open", &[path.to_string_lossy().to_string()]),
    }
}

#[tauri::command]
pub fn create_skin(app: tauri::AppHandle, spec: NewSkinSpec) -> Result<NewSkinResult, String> {
    let root =
        commands::skin_root().ok_or_else(|| "répertoire des skins introuvable".to_string())?;
    let folder_name = safe_file_name(&spec.name)?;
    let folder = root.join(&folder_name);
    if folder.exists() {
        return Err(format!("le skin « {folder_name} » existe déjà"));
    }
    std::fs::create_dir_all(folder.join("@Resources").join("Images"))
        .map_err(|error| error.to_string())?;
    let ini = folder.join(format!("{folder_name}.ini"));
    let width = spec.width.max(1);
    let height = spec.height.max(1);
    let text = format!(
        "[Metadata]\nName={}\nAuthor={}\nInformation=Créé avec Petrichor\nVersion=1.0\nLicense=\n\n[Variables]\n\n[Petrichor]\nUpdate=1000\nDynamicWindowSize=1\n\n[MeterBackground]\nMeter=Shape\nShape=Rectangle 0,0,{width},{height} | Fill Color 24,28,33,220 | StrokeWidth 1 | Stroke Color 56,189,248,180\n\n[MeterTitle]\nMeter=String\nX=12\nY=10\nFontSize=14\nFontColor=230,240,250,255\nAntiAlias=1\nText={}\n",
        spec.name.replace(['\n', '\r'], " "),
        spec.author.replace(['\n', '\r'], " "),
        spec.name.replace(['\n', '\r'], " "),
    );
    std::fs::write(&ini, text).map_err(|error| error.to_string())?;
    let windows = app.state::<SkinWindows>();
    let _ = windows::open_window(&app, &windows, ini.clone());
    Ok(NewSkinResult { path: ini, folder })
}

fn safe_file_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty()
        || matches!(name, "." | "..")
        || name.contains(['/', '\\', '\0'])
        || name.chars().any(|ch| ch.is_control())
    {
        return Err("nom de skin invalide".to_string());
    }
    Ok(name.to_string())
}

fn apply_active_variants(registry: &mut petrichor_core::Registry) {
    let favorites = storage::favorite_skins();
    for folder in &mut registry.folders {
        let active = folder
            .files
            .iter()
            .position(|file| {
                persistence::load_state(&file.path)
                    .map(|state| state.active > 0)
                    .unwrap_or(false)
            })
            .unwrap_or(0);
        folder.active_variant = active as i16;
        for file in &mut folder.files {
            file.is_favorite =
                favorites.contains(&persistence::skin_key(&file.path).to_lowercase());
        }
        folder.has_favorite = folder.files.iter().any(|file| file.is_favorite);
    }
}

fn open_path(path: &Path) -> Result<(), String> {
    spawn("xdg-open", &[path.to_string_lossy().to_string()])
}

fn spawn(program: &str, args: &[String]) -> Result<(), String> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("impossible de lancer {program} : {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_new_skin_names() {
        assert_eq!(safe_file_name("Horloge").unwrap(), "Horloge");
        assert!(safe_file_name("../evil").is_err());
        assert!(safe_file_name("a/b").is_err());
        assert!(safe_file_name("").is_err());
    }
}
