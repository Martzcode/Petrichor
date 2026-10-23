use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;

use crate::persistence;
use crate::windows::WindowFlags;
use petrichor_core::logging::{self, Entry, Level};
use petrichor_core::measure::{MeasureEngine, UpdateReport};
use petrichor_core::reference::MeasureValue;
use petrichor_core::render::{build_render_model_with_options, RenderModel};
use petrichor_core::{build_skin, build_skin_with_overrides, Diagnostics, Registry, Skin};
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
pub struct MeasureSnapshot {
    pub strings: BTreeMap<String, String>,
    pub values: BTreeMap<String, MeasureValue>,
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
    skin_root().map(|path| path.to_string_lossy().to_string())
}

pub(crate) fn skin_root() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("PETRICHOR_SKINS") {
        let path = PathBuf::from(path);
        if path.is_dir() {
            return Some(path);
        }
    }
    let bundled = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../skins-demo");
    if bundled.is_dir() {
        return Some(bundled);
    }
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .map(|base| base.join("Petrichor").join("Skins"))
        .filter(|path| path.is_dir())
}

#[derive(Debug, Clone, Serialize)]
pub struct AppPaths {
    pub config: Option<String>,
    pub skins: Option<String>,
    pub log: Option<String>,
}

#[tauri::command]
pub fn get_paths() -> AppPaths {
    let config = persistence::config_path();
    AppPaths {
        log: config
            .as_deref()
            .and_then(Path::parent)
            .map(petrichor_core::logging::default_log_path)
            .map(|path| path.to_string_lossy().to_string()),
        config: config.map(|path| path.to_string_lossy().to_string()),
        skins: default_skin_root(),
    }
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
pub fn load_render_model(path: String, store: tauri::State<'_, MeasureStore>) -> RenderLoadResult {
    let path = Path::new(&path);
    let overlay = store.variables(path);
    let overrides = store.options(path);
    let (model, diagnostics) =
        build_render_model_with_options(path, overlay.as_ref(), overrides.as_ref());
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
        flags: window_flags(path),
        model,
        diagnostics,
    }
}

pub(crate) fn window_flags(path: &Path) -> WindowFlags {
    let defaults = WindowFlags {
        always_on_top: false,
        always_on_bottom: false,
        on_hover: 0,
        click_through: false,
        draggable: false,
        skip_taskbar: true,
        opacity: 255,
        fade_duration: 250,
    };
    persistence::load_state(path)
        .map(|state| WindowFlags {
            always_on_top: state.always_on_top >= 1,
            always_on_bottom: state.always_on_top <= -2,
            on_hover: state.on_hover.clamp(0, 2) as u8,
            click_through: state.click_through,
            draggable: state.draggable,
            skip_taskbar: defaults.skip_taskbar,
            opacity: state.alpha_value,
            fade_duration: state.fade_duration,
        })
        .unwrap_or(defaults)
}

fn window_handle(
    app: &tauri::AppHandle,
    windows: &crate::windows::SkinWindows,
    path: &Path,
) -> u64 {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use tauri::Manager;
    let Some(label) = windows.find_by_path(path) else {
        return 0;
    };
    let Some(window) = app.get_webview_window(&label) else {
        return 0;
    };
    match window.window_handle().map(|handle| handle.as_raw()) {
        Ok(RawWindowHandle::Xlib(handle)) => handle.window,
        Ok(RawWindowHandle::Xcb(handle)) => u64::from(handle.window.get()),
        Ok(RawWindowHandle::Win32(handle)) => handle.hwnd.get() as u64,
        _ => 0,
    }
}

#[tauri::command]
pub fn read_measures(
    app: tauri::AppHandle,
    windows: tauri::State<'_, crate::windows::SkinWindows>,
    store: tauri::State<'_, MeasureStore>,
    path: String,
) -> MeasureSnapshot {
    let handle = window_handle(&app, &windows, Path::new(&path));
    let Some(report) = store.update(Path::new(&path), handle) else {
        return MeasureSnapshot {
            strings: BTreeMap::new(),
            values: BTreeMap::new(),
        };
    };

    if !report.actions.is_empty() {
        let label = windows.find_by_path(Path::new(&path));
        for action in &report.actions {
            crate::bangs::execute_chain(&app, &windows, &store, label.as_deref(), action);
        }
    }

    MeasureSnapshot {
        strings: report.strings,
        values: report.values,
    }
}

#[tauri::command]
pub fn execute_bang(
    app: tauri::AppHandle,
    windows: tauri::State<'_, crate::windows::SkinWindows>,
    store: tauri::State<'_, MeasureStore>,
    label: Option<String>,
    command: String,
) -> Result<(), String> {
    crate::bangs::execute_chain(&app, &windows, &store, label.as_deref(), &command);
    Ok(())
}

pub type OptionOverrides = BTreeMap<String, BTreeMap<String, String>>;

pub struct MeasureStore {
    engines: Mutex<HashMap<PathBuf, MeasureEngine>>,
    options: Mutex<HashMap<PathBuf, OptionOverrides>>,
}

impl MeasureStore {
    pub fn new() -> Self {
        MeasureStore {
            engines: Mutex::new(HashMap::new()),
            options: Mutex::new(HashMap::new()),
        }
    }

    fn store_key(path: &Path) -> PathBuf {
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
    }

    fn with_engine<T>(
        &self,
        path: &Path,
        action: impl FnOnce(&mut MeasureEngine) -> T,
    ) -> Option<T> {
        let key = Self::store_key(path);
        let overrides = self
            .options
            .lock()
            .ok()
            .and_then(|options| options.get(&key).cloned());
        let mut engines = self.engines.lock().ok()?;
        let engine = engines.entry(key).or_insert_with(|| {
            let (skin, _diagnostics) = match overrides {
                Some(overrides) => {
                    let (skin, _, diagnostics) = build_skin_with_overrides(path, &overrides);
                    (skin, diagnostics)
                }
                None => build_skin(path),
            };
            MeasureEngine::new(&skin)
        });
        Some(action(engine))
    }

    fn update(&self, path: &Path, window_handle: u64) -> Option<UpdateReport> {
        self.with_engine(path, |engine| {
            engine.set_window_handle(window_handle);
            engine.tick(&backend())
        })
    }

    pub fn variables(&self, path: &Path) -> Option<petrichor_core::Variables> {
        self.with_engine(path, |engine| engine.variables().clone())
    }

    pub fn measure_kind(&self, path: &Path, name: &str) -> Option<String> {
        self.with_engine(path, |engine| engine.measure_kind(name).map(str::to_string))
            .flatten()
    }

    pub fn command_measure(&self, path: &Path, name: &str, command: &str) -> bool {
        self.with_engine(path, |engine| engine.command_measure(name, command))
            .unwrap_or(false)
    }

    pub fn set_variable(&self, path: &Path, name: &str, value: &str) {
        self.with_engine(path, |engine| engine.set_variable(name, value));
    }

    fn skin(&self, path: &Path) -> Option<petrichor_core::Skin> {
        let key = Self::store_key(path);
        let overrides = self
            .options
            .lock()
            .ok()
            .and_then(|options| options.get(&key).cloned());
        Some(match overrides {
            Some(overrides) => petrichor_core::build_skin_with_overrides(path, &overrides).0,
            None => petrichor_core::build_skin(path).0,
        })
    }

    pub fn meter_names_in_group(&self, path: &Path, group: &str) -> Vec<String> {
        let Some(skin) = self.skin(path) else {
            return Vec::new();
        };
        skin.meters
            .iter()
            .filter(|meter| group_matches(&meter.options, group))
            .map(|meter| meter.name.clone())
            .collect()
    }

    pub fn measure_names_in_group(&self, path: &Path, group: &str) -> Vec<String> {
        let Some(skin) = self.skin(path) else {
            return Vec::new();
        };
        skin.measures
            .iter()
            .filter(|measure| group_matches(&measure.options, group))
            .map(|measure| measure.name.clone())
            .collect()
    }

    pub fn meter_hidden(&self, path: &Path, name: &str) -> bool {
        self.skin(path)
            .and_then(|skin| {
                skin.meter(name)
                    .and_then(|meter| meter.options.get("Hidden"))
                    .map(|value| petrichor_core::readers::read_bool(value, false))
            })
            .unwrap_or(false)
    }

    pub fn measure_flag(&self, path: &Path, name: &str, option: &str) -> bool {
        self.skin(path)
            .and_then(|skin| {
                skin.measure(name)
                    .and_then(|measure| measure.options.get(option))
                    .map(|value| petrichor_core::readers::read_bool(value, false))
            })
            .unwrap_or(false)
    }

    pub fn set_option_group(&self, path: &Path, group: &str, option: &str, value: &str) {
        let Some(skin) = self.skin(path) else {
            return;
        };
        let mut names: Vec<String> = skin
            .meters
            .iter()
            .filter(|meter| group_matches(&meter.options, group))
            .map(|meter| meter.name.clone())
            .collect();
        names.extend(
            skin.measures
                .iter()
                .filter(|measure| group_matches(&measure.options, group))
                .map(|measure| measure.name.clone()),
        );
        for name in names {
            self.set_option(path, &name, option, value);
        }
    }

    pub fn options(&self, path: &Path) -> Option<OptionOverrides> {
        let key = Self::store_key(path);
        self.options.lock().ok()?.get(&key).cloned()
    }

    pub fn remove(&self, path: &Path) {
        let key = Self::store_key(path);
        if let Ok(mut engines) = self.engines.lock() {
            engines.remove(&key);
        }
        if let Ok(mut options) = self.options.lock() {
            options.remove(&key);
        }
    }

    pub fn set_option(&self, path: &Path, section: &str, option: &str, value: &str) {
        let key = Self::store_key(path);
        if let Ok(mut options) = self.options.lock() {
            options
                .entry(key.clone())
                .or_default()
                .entry(section.to_string())
                .or_default()
                .insert(option.to_string(), value.to_string());
        }

        let variables = self.with_engine(path, |engine| engine.variables().to_raw_map());
        if let Ok(mut engines) = self.engines.lock() {
            engines.remove(&key);
        }
        if let Some(variables) = variables {
            self.with_engine(path, |engine| {
                for (name, value) in &variables {
                    engine.set_variable(name, value);
                }
            });
        }
    }
}

fn group_matches(options: &petrichor_core::Options, group: &str) -> bool {
    if group.trim() == "*" {
        return true;
    }
    options
        .get("Group")
        .map(|value| {
            value
                .split('|')
                .map(str::trim)
                .any(|candidate| candidate.eq_ignore_ascii_case(group.trim()))
        })
        .unwrap_or(false)
}

impl Default for MeasureStore {
    fn default() -> Self {
        Self::new()
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_meter_and_measure_groups() {
        let path =
            std::env::temp_dir().join(format!("petrichor-groups-{}.ini", std::process::id()));
        std::fs::write(
            &path,
            "\
[MeterA]
Meter=String
Group=Top|Shared
Text=A

[MeterB]
Meter=String
Group=Bottom
Text=B

[MeasureA]
Measure=Calc
Group=Shared
Formula=1
",
        )
        .unwrap();

        let store = MeasureStore::new();
        assert_eq!(
            store.meter_names_in_group(&path, "top"),
            vec!["MeterA".to_string()]
        );
        assert_eq!(
            store.meter_names_in_group(&path, "Shared"),
            vec!["MeterA".to_string()]
        );
        assert_eq!(
            store.measure_names_in_group(&path, "shared"),
            vec!["MeasureA".to_string()]
        );
        assert!(!store.meter_hidden(&path, "MeterA"));

        store.set_option(&path, "MeterA", "Hidden", "1");
        assert!(store.meter_hidden(&path, "MeterA"));

        store.set_option_group(&path, "Shared", "Hidden", "1");
        assert!(store.meter_hidden(&path, "MeterA"));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn store_keeps_variables_between_reads() {
        let path = std::env::temp_dir().join(format!("petrichor-store-{}.ini", std::process::id()));
        std::fs::write(&path, "[Variables]\nFoo=bar\n[MeasureCalc]\nMeasure=Calc\n").unwrap();

        let store = MeasureStore::new();
        store.set_variable(&path, "Foo", "baz");

        let variables = store.variables(&path).unwrap();
        assert_eq!(variables.get("Foo"), Some("baz"));
        assert_eq!(variables.get("foo"), Some("baz"));

        store.remove(&path);
        let variables = store.variables(&path).unwrap();
        assert_eq!(variables.get("Foo"), Some("bar"));

        std::fs::remove_file(&path).ok();
    }
}
