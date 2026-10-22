use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, LogicalSize, Manager, PhysicalPosition, State, WebviewUrl, WebviewWindowBuilder,
};

use crate::persistence;

#[derive(Debug, Clone, Serialize)]
pub struct SkinWindowInfo {
    pub label: String,
    pub path: PathBuf,
    pub name: String,
    pub groups: Vec<String>,
    pub always_on_top: bool,
    pub click_through: bool,
    pub draggable: bool,
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
    #[serde(default)]
    pub always_on_bottom: bool,
    #[serde(default)]
    pub on_hover: u8,
    pub click_through: bool,
    pub draggable: bool,
    pub skip_taskbar: bool,
    #[serde(default = "default_opacity")]
    pub opacity: u8,
    #[serde(default = "default_fade_duration")]
    pub fade_duration: u32,
}

fn default_opacity() -> u8 {
    255
}

fn default_fade_duration() -> u32 {
    250
}

#[derive(Default)]
struct MoveSave {
    last: Option<std::time::Instant>,
}

#[derive(Default)]
pub struct SkinWindows {
    inner: Mutex<HashMap<String, SkinWindowInfo>>,
    sizes: Mutex<HashMap<String, (u32, u32)>>,
    moves: Mutex<HashMap<String, MoveSave>>,
    dragging: Mutex<HashSet<String>>,
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

    pub fn remove(&self, label: &str) {
        if let Ok(mut map) = self.inner.lock() {
            map.remove(label);
        }
        if let Ok(mut sizes) = self.sizes.lock() {
            sizes.remove(label);
        }
        if let Ok(mut moves) = self.moves.lock() {
            moves.remove(label);
        }
        if let Ok(mut dragging) = self.dragging.lock() {
            dragging.remove(label);
        }
    }

    pub fn set_dragging(&self, label: &str, dragging: bool) {
        if let Ok(mut set) = self.dragging.lock() {
            if dragging {
                set.insert(label.to_string());
            } else {
                set.remove(label);
            }
        }
    }

    pub fn is_dragging(&self, label: &str) -> bool {
        self.dragging
            .lock()
            .map(|set| set.contains(label))
            .unwrap_or(false)
    }

    pub fn allow_move_save(&self, label: &str) -> bool {
        let Ok(mut moves) = self.moves.lock() else {
            return false;
        };
        let now = std::time::Instant::now();
        let entry = moves.entry(label.to_string()).or_default();
        if let Some(last) = entry.last {
            if now.duration_since(last) < std::time::Duration::from_millis(300) {
                return false;
            }
        }
        entry.last = Some(now);
        true
    }

    fn update(&self, label: &str, flags: WindowFlags) {
        if let Ok(mut map) = self.inner.lock() {
            if let Some(info) = map.get_mut(label) {
                info.always_on_top = flags.always_on_top;
                info.click_through = flags.click_through;
                info.draggable = flags.draggable;
                info.skip_taskbar = flags.skip_taskbar;
            }
        }
    }

    pub fn set_draggable(&self, label: &str, draggable: bool) {
        if let Ok(mut map) = self.inner.lock() {
            if let Some(info) = map.get_mut(label) {
                info.draggable = draggable;
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

    pub fn find_targets(&self, target: &str) -> Vec<String> {
        let target = target.trim();
        if target == "*" {
            return self.list().into_iter().map(|info| info.label).collect();
        }
        let key = target.to_lowercase();
        let map = match self.inner.lock() {
            Ok(map) => map,
            Err(_) => return Vec::new(),
        };
        let mut labels: Vec<String> = map
            .values()
            .filter(|info| {
                info.label.eq_ignore_ascii_case(&key)
                    || info.name.eq_ignore_ascii_case(&key)
                    || info.path.to_string_lossy().to_lowercase().contains(&key)
                    || info
                        .groups
                        .iter()
                        .any(|group| group.eq_ignore_ascii_case(&key))
            })
            .map(|info| info.label.clone())
            .collect();
        labels.sort();
        labels
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
    let state = persistence::load_state(&path);
    let (skin, _) = petrichor_core::build_skin(&path);
    let mut groups = skin.settings.group.clone();
    if let Some(state) = &state {
        for group in &state.group {
            if !groups
                .iter()
                .any(|current| current.eq_ignore_ascii_case(group))
            {
                groups.push(group.clone());
            }
        }
    }

    let always_on_top = state
        .as_ref()
        .map(|state| state.always_on_top > 0)
        .unwrap_or(false);
    let click_through = state
        .as_ref()
        .map(|state| state.click_through)
        .unwrap_or(false);
    let draggable = state.as_ref().map(|state| state.draggable).unwrap_or(false);

    windows.insert(SkinWindowInfo {
        label: label.clone(),
        path: path.clone(),
        name: skin.name.clone(),
        groups,
        always_on_top,
        click_through,
        draggable,
        skip_taskbar: true,
    });

    petrichor_core::logging::notice(format!(
        "Ouverture de la fenêtre de skin {label} : {}",
        path.display()
    ));

    let mut builder = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html".into()))
        .title("Petrichor")
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .skip_taskbar(true)
        .resizable(false)
        .inner_size(200.0, 100.0)
        .position(100.0, 100.0)
        .visible(false);

    if let Some(state) = &state {
        if state.save_position {
            builder = builder.position(state.window_x as f64, state.window_y as f64);
        }
        if state.always_on_top > 0 {
            builder = builder.always_on_top(true);
        } else if state.always_on_top <= -2 {
            builder = builder.always_on_bottom(true);
        }
    }

    match builder.build() {
        Ok(window) => {
            if click_through {
                let _ = window.set_ignore_cursor_events(true);
            }
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

const SNAP_DISTANCE: i32 = 16;

#[allow(clippy::too_many_arguments)]
fn adjusted_position(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    monitor_x: i32,
    monitor_y: i32,
    monitor_width: i32,
    monitor_height: i32,
    snap_edges: bool,
    keep_on_screen: bool,
) -> (i32, i32) {
    let mut x = x;
    let mut y = y;
    if snap_edges {
        if (x - monitor_x).abs() <= SNAP_DISTANCE {
            x = monitor_x;
        }
        if (y - monitor_y).abs() <= SNAP_DISTANCE {
            y = monitor_y;
        }
        let right = monitor_x + monitor_width - width;
        if (x - right).abs() <= SNAP_DISTANCE {
            x = right;
        }
        let bottom = monitor_y + monitor_height - height;
        if (y - bottom).abs() <= SNAP_DISTANCE {
            y = bottom;
        }
    }
    if keep_on_screen {
        if width >= monitor_width {
            x = monitor_x;
        } else {
            x = x.clamp(monitor_x, monitor_x + monitor_width - width);
        }
        if height >= monitor_height {
            y = monitor_y;
        } else {
            y = y.clamp(monitor_y, monitor_y + monitor_height - height);
        }
    }
    (x, y)
}

pub fn snap_and_keep(app: &AppHandle, windows: &SkinWindows, label: &str) -> Result<(), String> {
    let Some(info) = windows.info(label) else {
        return Ok(());
    };
    let Some(state) = persistence::load_state(&info.path) else {
        return Ok(());
    };
    if !state.snap_edges && !state.keep_on_screen {
        return Ok(());
    }
    let Some(window) = app.get_webview_window(label) else {
        return Ok(());
    };
    let position = window.outer_position().map_err(|error| error.to_string())?;
    let size = window.outer_size().map_err(|error| error.to_string())?;
    let monitor = window
        .current_monitor()
        .map_err(|error| error.to_string())?
        .or(window
            .primary_monitor()
            .map_err(|error| error.to_string())?);
    let Some(monitor) = monitor else {
        return Ok(());
    };

    let mpos = monitor.position();
    let msize = monitor.size();
    let (x, y) = adjusted_position(
        position.x,
        position.y,
        size.width as i32,
        size.height as i32,
        mpos.x,
        mpos.y,
        msize.width as i32,
        msize.height as i32,
        state.snap_edges,
        state.keep_on_screen,
    );
    if x == position.x && y == position.y {
        return Ok(());
    }
    window
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|error| error.to_string())?;
    save_position(app, windows, label)?;
    Ok(())
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
        .set_always_on_bottom(flags.always_on_bottom)
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

pub fn fit(
    app: &AppHandle,
    windows: &SkinWindows,
    label: &str,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| format!("fenêtre introuvable: {label}"))?;

    let size = (
        width.max(1.0).round() as u32,
        height.max(1.0).round() as u32,
    );
    if let Ok(sizes) = windows.sizes.lock() {
        if sizes.get(label) == Some(&size) {
            return Ok(());
        }
    }

    window
        .set_size(LogicalSize::new(size.0 as f64, size.1 as f64))
        .map_err(|e| e.to_string())?;

    if let Ok(mut sizes) = windows.sizes.lock() {
        sizes.insert(label.to_string(), size);
    }

    Ok(())
}

pub fn save_position(app: &AppHandle, windows: &SkinWindows, label: &str) -> Result<(), String> {
    let info = windows
        .info(label)
        .ok_or_else(|| format!("skin introuvable : {label}"))?;
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| format!("fenêtre introuvable : {label}"))?;
    let position = window.outer_position().map_err(|error| error.to_string())?;

    persistence::update_state(&info.path, |state| {
        state.window_x = position.x;
        state.window_y = position.y;
        state.save_position = true;
    });

    Ok(())
}

pub fn show(app: &AppHandle, label: &str) -> Result<(), String> {
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| format!("fenêtre introuvable: {label}"))?;
    window.show().map_err(|e| e.to_string())
}

pub fn hide(app: &AppHandle, label: &str) -> Result<(), String> {
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| format!("fenêtre introuvable: {label}"))?;
    window.hide().map_err(|e| e.to_string())
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
    windows: State<'_, SkinWindows>,
    label: String,
    width: f64,
    height: f64,
) -> Result<(), String> {
    fit(&app, &windows, &label, width, height)
}

#[tauri::command]
pub fn show_skin_window(app: AppHandle, label: String) -> Result<(), String> {
    show(&app, &label)
}

#[tauri::command]
pub fn hide_skin_window(app: AppHandle, label: String) -> Result<(), String> {
    hide(&app, &label)
}

#[tauri::command]
pub fn set_skin_dragging(
    app: AppHandle,
    windows: State<'_, SkinWindows>,
    selection: State<'_, crate::selection::SelectionState>,
    label: String,
    dragging: bool,
) -> Result<(), String> {
    windows.set_dragging(&label, dragging);
    if dragging {
        selection.start_drag(&app, &windows, &label)?;
    } else {
        selection.finish_drag(&app, &windows, &label)?;
    }
    Ok(())
}

#[tauri::command]
pub fn save_skin_position(
    app: AppHandle,
    windows: State<'_, SkinWindows>,
    label: String,
) -> Result<(), String> {
    save_position(&app, &windows, &label)
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
            name: "a".to_string(),
            groups: Vec::new(),
            always_on_top: false,
            click_through: false,
            draggable: false,
            skip_taskbar: true,
        });
        assert_eq!(windows.list().len(), 1);

        windows.update(
            "skin-1",
            WindowFlags {
                always_on_top: true,
                always_on_bottom: false,
                on_hover: 0,
                click_through: true,
                draggable: true,
                skip_taskbar: false,
                opacity: 255,
                fade_duration: 250,
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
    fn snaps_and_keeps_windows_on_monitor() {
        assert_eq!(
            adjusted_position(101, 54, 200, 100, 100, 50, 1000, 800, true, true),
            (100, 50)
        );
        assert_eq!(
            adjusted_position(890, 760, 200, 100, 100, 50, 1000, 800, true, true),
            (900, 750)
        );
        assert_eq!(
            adjusted_position(-40, 760, 200, 100, 0, 0, 800, 800, false, true),
            (0, 700)
        );
        assert_eq!(
            adjusted_position(300, 300, 200, 100, 0, 0, 800, 800, false, true),
            (300, 300)
        );
    }

    #[test]
    fn finds_window_by_path() {
        let windows = SkinWindows::new();
        windows.insert(SkinWindowInfo {
            label: "skin-1".to_string(),
            path: PathBuf::from("/tmp/a.ini"),
            name: "a".to_string(),
            groups: Vec::new(),
            always_on_top: false,
            click_through: false,
            draggable: false,
            skip_taskbar: true,
        });
        assert_eq!(
            windows.find_by_path(Path::new("/tmp/a.ini")).as_deref(),
            Some("skin-1")
        );
        assert!(windows.find_by_path(Path::new("/tmp/b.ini")).is_none());
    }
}
