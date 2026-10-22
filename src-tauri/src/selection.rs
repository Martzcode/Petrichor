use std::collections::HashSet;
use std::sync::Mutex;

use tauri::{AppHandle, Manager, PhysicalPosition};

use petrichor_core::ApplicationConfig;

use crate::bangs::{emit, SkinCommand};
use crate::persistence;
use crate::windows::{self, SkinWindows};

const DEFAULT_SELECTION_COLOR: &str = "#38bdf8";

#[derive(Debug, Clone)]
struct DragState {
    primary: String,
    last: PhysicalPosition<i32>,
}

#[derive(Default)]
struct SelectionInner {
    active: bool,
    selected: HashSet<String>,
    drag: Option<DragState>,
}

#[derive(Default)]
pub struct SelectionState {
    inner: Mutex<SelectionInner>,
}

impl SelectionState {
    pub fn new() -> Self {
        SelectionState::default()
    }

    pub fn is_active(&self) -> bool {
        self.inner.lock().map(|inner| inner.active).unwrap_or(false)
    }

    pub fn toggle(&self, app: &AppHandle, windows: &SkinWindows) -> Result<bool, String> {
        let active = {
            let mut inner = self.inner.lock().map_err(|error| error.to_string())?;
            inner.active = !inner.active;
            if !inner.active {
                inner.selected.clear();
                inner.drag = None;
            }
            inner.active
        };
        emit_selection(self, app, windows)?;
        Ok(active)
    }

    pub fn select(
        &self,
        app: &AppHandle,
        windows: &SkinWindows,
        label: &str,
        additive: bool,
    ) -> Result<bool, String> {
        if windows.info(label).is_none() {
            return Err(format!("fenêtre introuvable: {label}"));
        }
        let selected = {
            let mut inner = self.inner.lock().map_err(|error| error.to_string())?;
            if !inner.active {
                return Ok(false);
            }
            if additive {
                if !inner.selected.remove(label) {
                    inner.selected.insert(label.to_string());
                }
            } else {
                inner.selected.clear();
                inner.selected.insert(label.to_string());
            }
            inner.selected.contains(label)
        };
        emit_selection(self, app, windows)?;
        Ok(selected)
    }

    pub fn start_drag(
        &self,
        app: &AppHandle,
        _windows: &SkinWindows,
        label: &str,
    ) -> Result<(), String> {
        let Some(window) = app.get_webview_window(label) else {
            return Ok(());
        };
        let position = window.outer_position().map_err(|error| error.to_string())?;
        let mut inner = self.inner.lock().map_err(|error| error.to_string())?;
        if !inner.active || !inner.selected.contains(label) {
            return Ok(());
        }
        inner.drag = Some(DragState {
            primary: label.to_string(),
            last: position,
        });
        Ok(())
    }

    pub fn finish_drag(
        &self,
        app: &AppHandle,
        windows: &SkinWindows,
        label: &str,
    ) -> Result<(), String> {
        let drag = {
            let mut inner = self.inner.lock().map_err(|error| error.to_string())?;
            match inner.drag.take() {
                Some(drag) if drag.primary == label => Some(drag),
                Some(drag) => {
                    inner.drag = Some(drag);
                    None
                }
                None => None,
            }
        };
        let Some(drag) = drag else {
            return windows::snap_and_keep(app, windows, label);
        };

        let Some(window) = app.get_webview_window(label) else {
            return Ok(());
        };
        let before = window.outer_position().map_err(|error| error.to_string())?;
        windows::snap_and_keep(app, windows, label)?;
        let after = window.outer_position().map_err(|error| error.to_string())?;
        let dx = after.x - before.x;
        let dy = after.y - before.y;
        if dx == 0 && dy == 0 {
            return Ok(());
        }

        let selected = self
            .inner
            .lock()
            .map(|inner| inner.selected.clone())
            .unwrap_or_default();
        for other in selected {
            if other != drag.primary {
                let _ = move_relative(app, windows, &other, dx, dy);
            }
        }
        Ok(())
    }

    pub fn window_moved(
        &self,
        app: &AppHandle,
        windows: &SkinWindows,
        label: &str,
        position: PhysicalPosition<i32>,
    ) -> Result<(), String> {
        let (selected, dx, dy) = {
            let mut inner = self.inner.lock().map_err(|error| error.to_string())?;
            let Some(drag) = inner.drag.as_mut() else {
                return Ok(());
            };
            if drag.primary != label {
                return Ok(());
            }
            let dx = position.x - drag.last.x;
            let dy = position.y - drag.last.y;
            if dx == 0 && dy == 0 {
                return Ok(());
            }
            drag.last = position;
            (inner.selected.clone(), dx, dy)
        };

        for other in selected {
            if other != label {
                let _ = move_relative(app, windows, &other, dx, dy);
            }
        }
        Ok(())
    }

    pub fn remove(&self, label: &str) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.selected.remove(label);
            if inner
                .drag
                .as_ref()
                .map(|drag| drag.primary == label)
                .unwrap_or(false)
            {
                inner.drag = None;
            }
        }
    }
}

fn move_relative(
    app: &AppHandle,
    windows: &SkinWindows,
    label: &str,
    dx: i32,
    dy: i32,
) -> Result<(), String> {
    let Some(window) = app.get_webview_window(label) else {
        return Ok(());
    };
    let Some(info) = windows.info(label) else {
        return Ok(());
    };
    let current = window.outer_position().map_err(|error| error.to_string())?;
    let target = PhysicalPosition::new(current.x + dx, current.y + dy);
    window
        .set_position(target)
        .map_err(|error| error.to_string())?;
    persistence::update_state(&info.path, |state| {
        state.window_x = target.x;
        state.window_y = target.y;
        state.save_position = true;
    });
    Ok(())
}

fn emit_selection(
    state: &SelectionState,
    app: &AppHandle,
    windows: &SkinWindows,
) -> Result<(), String> {
    let (active, selected) = {
        let inner = state.inner.lock().map_err(|error| error.to_string())?;
        (inner.active, inner.selected.clone())
    };
    let color = selection_color();
    let mut first_error = None;
    for info in windows.list() {
        let command = SkinCommand::Selection {
            active,
            selected: selected.contains(&info.label),
            color: color.clone(),
        };
        if let Err(error) = emit(app, &info.label, command) {
            first_error.get_or_insert(error);
        }
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

pub fn selection_color() -> String {
    persistence::config_path()
        .and_then(|path| ApplicationConfig::load(&path).ok())
        .and_then(|config| config.global().selected_color)
        .map(|color| {
            format!(
                "#{:02x}{:02x}{:02x}{:02x}",
                color.r, color.g, color.b, color.a
            )
        })
        .unwrap_or_else(|| DEFAULT_SELECTION_COLOR.to_string())
}

#[tauri::command]
pub fn select_skin(
    app: AppHandle,
    windows: tauri::State<'_, SkinWindows>,
    selection: tauri::State<'_, SelectionState>,
    label: String,
    additive: bool,
) -> Result<bool, String> {
    selection.select(&app, &windows, &label, additive)
}

#[tauri::command]
pub fn selection_active(selection: tauri::State<'_, SelectionState>) -> bool {
    selection.is_active()
}
