use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, LogicalSize, Manager, State, WebviewUrl, WebviewWindowBuilder};

#[derive(Debug, Clone, Serialize)]
pub struct SkinWindowInfo {
    pub label: String,
    pub path: PathBuf,
    pub always_on_top: bool,
    pub click_through: bool,
    pub skip_taskbar: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SkinToggle {
    Created,
    Shown,
    Hidden,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkinToggleResult {
    pub label: String,
    pub state: SkinToggle,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct WindowFlags {
    pub always_on_top: bool,
    pub click_through: bool,
    pub skip_taskbar: bool,
}

#[derive(Default)]
pub struct SkinWindows {
    inner: Mutex<HashMap<String, SkinWindowInfo>>,
    counter: AtomicU32,
}

impl SkinWindows {
    pub fn new() -> Self {
        SkinWindows::default()
    }

    fn next_label(&self) -> String {
        let id = self.counter.fetch_add(1, Ordering::SeqCst) + 1;
        format!("skin-{id}")
    }

    fn insert(&self, info: SkinWindowInfo) {
        if let Ok(mut map) = self.inner.lock() {
            map.insert(info.label.clone(), info);
        }
    }

    fn remove(&self, label: &str) {
        if let Ok(mut map) = self.inner.lock() {
            map.remove(label);
        }
    }

    fn update(&self, label: &str, flags: WindowFlags) {
        if let Ok(mut map) = self.inner.lock() {
            if let Some(info) = map.get_mut(label) {
                info.always_on_top = flags.always_on_top;
                info.click_through = flags.click_through;
                info.skip_taskbar = flags.skip_taskbar;
            }
        }
    }

    pub fn info(&self, label: &str) -> Option<SkinWindowInfo> {
        self.inner.lock().ok()?.get(label).cloned()
    }

    pub fn find_by_path(&self, path: &Path) -> Option<String> {
        let key = canonical(path);
        let map = self.inner.lock().ok()?;
        map.values()
            .find(|info| canonical(&info.path) == key)
            .map(|info| info.label.clone())
    }

    pub fn list(&self) -> Vec<SkinWindowInfo> {
        match self.inner.lock() {
            Ok(map) => {
                let mut infos: Vec<SkinWindowInfo> = map.values().cloned().collect();
                infos.sort_by(|a, b| a.label.cmp(&b.label));
                infos
            }
            Err(_) => Vec::new(),
        }
    }
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub fn open_window(
    app: &AppHandle,
    windows: &SkinWindows,
    path: PathBuf,
) -> Result<String, String> {
    let path = canonical(&path);
    let label = windows.next_label();

    windows.insert(SkinWindowInfo {
        label: label.clone(),
        path: path.clone(),
        always_on_top: false,
        click_through: false,
        skip_taskbar: true,
    });

    petrichor_core::logging::notice(format!(
        "Ouverture de la fenêtre de skin {label} : {}",
        path.display()
    ));

    let window = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html".into()))
        .title("Petrichor")
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .skip_taskbar(true)
        .resizable(false)
        .inner_size(200.0, 100.0)
        .position(100.0, 100.0)
        .visible(false)
        .build();

    match window {
        Ok(_) => {
            petrichor_core::logging::debug(format!("Fenêtre {label} créée"));
            Ok(label)
        }
        Err(error) => {
            windows.remove(&label);
            petrichor_core::logging::error(format!(
                "Échec de création de la fenêtre de skin {label} : {error}"
            ));
            Err(error.to_string())
        }
    }
}

pub fn apply_flags(
    app: &AppHandle,
    windows: &SkinWindows,
    label: &str,
    flags: WindowFlags,
) -> Result<(), String> {
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| format!("fenêtre introuvable: {label}"))?;

    window
        .set_always_on_top(flags.always_on_top)
        .map_err(|e| e.to_string())?;
    window
        .set_skip_taskbar(flags.skip_taskbar)
        .map_err(|e| e.to_string())?;
    window
        .set_ignore_cursor_events(flags.click_through)
        .map_err(|e| e.to_string())?;

    windows.update(label, flags);
    Ok(())
}

pub fn fit(app: &AppHandle, label: &str, width: f64, height: f64) -> Result<(), String> {
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| format!("fenêtre introuvable: {label}"))?;

    let size = LogicalSize::new(width.max(1.0), height.max(1.0));
    window.set_size(size).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn show(app: &AppHandle, label: &str) -> Result<(), String> {
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| format!("fenêtre introuvable: {label}"))?;
    window.show().map_err(|e| e.to_string())
}

pub fn close(app: &AppHandle, windows: &SkinWindows, label: &str) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(label) {
        window.close().map_err(|e| e.to_string())?;
    }
    windows.remove(label);
    Ok(())
}

pub fn toggle_window(
    app: &AppHandle,
    windows: &SkinWindows,
    path: PathBuf,
) -> Result<SkinToggleResult, String> {
    let Some(label) = windows.find_by_path(&path) else {
        let label = open_window(app, windows, path)?;
        return Ok(SkinToggleResult {
            label,
            state: SkinToggle::Created,
        });
    };

    let window = app
        .get_webview_window(&label)
        .ok_or_else(|| format!("fenêtre introuvable: {label}"))?;

    if window.is_visible().unwrap_or(false) {
        window.hide().map_err(|e| e.to_string())?;
        petrichor_core::logging::notice(format!("Skin {label} masqué"));
        Ok(SkinToggleResult {
            label,
            state: SkinToggle::Hidden,
        })
    } else {
        window.show().map_err(|e| e.to_string())?;
        let _ = window.set_focus();
        petrichor_core::logging::notice(format!("Skin {label} affiché"));
        Ok(SkinToggleResult {
            label,
            state: SkinToggle::Shown,
        })
    }
}

#[tauri::command]
pub fn toggle_skin_window(
    app: AppHandle,
    windows: State<'_, SkinWindows>,
    path: String,
) -> Result<SkinToggleResult, String> {
    toggle_window(&app, &windows, PathBuf::from(path))
}

#[tauri::command]
pub fn close_skin_window(
    app: AppHandle,
    windows: State<'_, SkinWindows>,
    label: String,
) -> Result<(), String> {
    close(&app, &windows, &label)
}

#[tauri::command]
pub fn skin_window_info(windows: State<'_, SkinWindows>, label: String) -> Option<SkinWindowInfo> {
    windows.info(&label)
}

#[tauri::command]
pub fn list_skin_windows(windows: State<'_, SkinWindows>) -> Vec<SkinWindowInfo> {
    windows.list()
}

#[tauri::command]
pub fn apply_skin_window_flags(
    app: AppHandle,
    windows: State<'_, SkinWindows>,
    label: String,
    flags: WindowFlags,
) -> Result<(), String> {
    apply_flags(&app, &windows, &label, flags)
}

#[tauri::command]
pub fn fit_skin_window(
    app: AppHandle,
    label: String,
    width: f64,
    height: f64,
) -> Result<(), String> {
    fit(&app, &label, width, height)
}

#[tauri::command]
pub fn show_skin_window(app: AppHandle, label: String) -> Result<(), String> {
    show(&app, &label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assigns_incrementing_labels() {
        let windows = SkinWindows::new();
        assert_eq!(windows.next_label(), "skin-1");
        assert_eq!(windows.next_label(), "skin-2");
    }

    #[test]
    fn tracks_and_updates_info() {
        let windows = SkinWindows::new();
        windows.insert(SkinWindowInfo {
            label: "skin-1".to_string(),
            path: PathBuf::from("/tmp/a.ini"),
            always_on_top: false,
            click_through: false,
            skip_taskbar: true,
        });
        assert_eq!(windows.list().len(), 1);

        windows.update(
            "skin-1",
            WindowFlags {
                always_on_top: true,
                click_through: true,
                skip_taskbar: false,
            },
        );
        let info = windows.info("skin-1").unwrap();
        assert!(info.always_on_top);
        assert!(info.click_through);
        assert!(!info.skip_taskbar);

        windows.remove("skin-1");
        assert!(windows.info("skin-1").is_none());
    }

    #[test]
    fn finds_window_by_path() {
        let windows = SkinWindows::new();
        windows.insert(SkinWindowInfo {
            label: "skin-1".to_string(),
            path: PathBuf::from("/tmp/a.ini"),
            always_on_top: false,
            click_through: false,
            skip_taskbar: true,
        });
        assert_eq!(
            windows.find_by_path(Path::new("/tmp/a.ini")).as_deref(),
            Some("skin-1")
        );
        assert!(windows.find_by_path(Path::new("/tmp/b.ini")).is_none());
    }
}
