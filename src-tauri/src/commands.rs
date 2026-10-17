use std::path::Path;

use serde::Serialize;

use petrichor_core::logging::{self, Entry, Level};
use petrichor_core::{build_skin, Diagnostics, Registry, Skin};
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
