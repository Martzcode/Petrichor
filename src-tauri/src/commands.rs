use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::windows::WindowFlags;
use petrichor_core::logging::{self, Entry, Level};
use petrichor_core::render::{build_render_model, RenderModel};
use petrichor_core::{build_skin, ApplicationConfig, Diagnostics, Registry, Skin};
use petrichor_sys::{backend, SystemBackend};

pub const LOG_EVENT: &str = "petrichor://log";
pub const READY_EVENT: &str = "petrichor://ready";

#[derive(Debug, Clone, Serialize)]
pub struct SystemInfo {
    pub version: String,
    pub os_name: String,
    pub os_version: String,
    pub hostname: String,
    pub user_name: String,
    pub uptime_secs: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkinLoadResult {
    pub skin: Skin,
    pub diagnostics: Diagnostics,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderLoadResult {
    pub model: RenderModel,
    pub diagnostics: Diagnostics,
    pub flags: WindowFlags,
}

#[tauri::command]
pub fn get_version() -> String {
    petrichor_core::version().to_string()
}

#[tauri::command]
pub fn get_system_info() -> SystemInfo {
    let backend = backend();
    let release = backend.os_release();
    SystemInfo {
        version: petrichor_core::version().to_string(),
        os_name: release.name,
        os_version: release.version,
        hostname: backend.hostname(),
        user_name: backend.user_name(),
        uptime_secs: backend.uptime_secs(),
    }
}

#[tauri::command]
pub fn get_logs() -> Vec<Entry> {
    logging::logger().entries()
}

#[tauri::command]
pub fn clear_logs() {
    logging::logger().clear();
}

#[tauri::command]
pub fn scan_skins(root: String) -> Registry {
    Registry::scan(root)
}

#[tauri::command]
pub fn default_skin_root() -> Option<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../skins-demo");
    path.is_dir().then(|| path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn load_skin(path: String) -> SkinLoadResult {
    let (skin, diagnostics) = build_skin(Path::new(&path));
    for diagnostic in diagnostics.iter() {
        logging::logger().log(
            match diagnostic.severity {
                petrichor_core::Severity::Error => Level::Error,
                petrichor_core::Severity::Warning => Level::Warning,
                petrichor_core::Severity::Info => Level::Notice,
            },
            Some(&skin.name),
            diagnostic.to_string(),
        );
    }
    SkinLoadResult { skin, diagnostics }
}

#[tauri::command]
pub fn load_render_model(path: String) -> RenderLoadResult {
    let (model, diagnostics) = build_render_model(Path::new(&path));
    for diagnostic in diagnostics.iter() {
        logging::logger().log(
            match diagnostic.severity {
                petrichor_core::Severity::Error => Level::Error,
                petrichor_core::Severity::Warning => Level::Warning,
                petrichor_core::Severity::Info => Level::Notice,
            },
            Some(&model.name),
            diagnostic.to_string(),
        );
    }
    RenderLoadResult {
        flags: window_flags(&model.name),
        model,
        diagnostics,
    }
}

fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("Petrichor").join("Petrichor.ini"))
}

fn window_flags(skin: &str) -> WindowFlags {
    let defaults = WindowFlags {
        always_on_top: false,
        click_through: false,
        skip_taskbar: true,
    };
    config_path()
        .and_then(|path| ApplicationConfig::load(&path).ok())
        .map(|config| config.skin_state(skin))
        .map(|state| WindowFlags {
            always_on_top: state.always_on_top >= 1,
            click_through: state.click_through,
            skip_taskbar: defaults.skip_taskbar,
        })
        .unwrap_or(defaults)
}

#[tauri::command]
pub fn read_measures(path: String) -> BTreeMap<String, String> {
    let (skin, _diagnostics) = build_skin(Path::new(&path));
    petrichor_core::measure::evaluate(&skin)
}

#[tauri::command]
pub fn frontend_log(level: String, message: String) {
    let level = match level.to_ascii_lowercase().as_str() {
        "error" => Level::Error,
        "warning" => Level::Warning,
        "debug" => Level::Debug,
        _ => Level::Notice,
    };
    logging::logger().log(level, Some("frontend"), message);
}

#[tauri::command]
pub fn demo_event(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Emitter;

    logging::logger().log(
        Level::Notice,
        Some("demo"),
        "Événement de démonstration émis vers Angular",
    );

    if let Some(entry) = logging::logger().entries().last().cloned() {
        app.emit(LOG_EVENT, entry).map_err(|e| e.to_string())?;
    }

    Ok(())
}
