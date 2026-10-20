use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Emitter, EventTarget, LogicalPosition, Manager, WebviewWindow};

use petrichor_core::bang::{self, Bang};
use petrichor_core::logging::{self, Level};
use petrichor_core::SkinState;

use crate::commands::MeasureStore;
use crate::persistence;
use crate::windows::{self, SkinWindows};

pub const COMMAND_EVENT: &str = "petrichor://command";

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "command", rename_all = "lowercase")]
pub enum SkinCommand {
    Redraw,
    Update,
    Refresh,
    Draggable { value: bool },
}

pub fn execute_chain(
    app: &AppHandle,
    windows: &SkinWindows,
    measures: &MeasureStore,
    label: Option<&str>,
    command: &str,
) {
    let path = label
        .and_then(|label| windows.info(label))
        .map(|info| info.path);

    for mut bang in bang::parse(command) {
        if let Some(path) = path.as_deref() {
            if let Some(variables) = measures.variables(path) {
                bang.args = bang
                    .args
                    .iter()
                    .map(|argument| variables.expand(argument))
                    .collect();
            }
        }

        if let Err(error) = execute(app, windows, measures, label, &bang) {
            logging::logger().log(
                Level::Warning,
                Some("bang"),
                format!("Bang {} ignoré : {error}", bang.name),
            );
        }
    }
}

fn execute(
    app: &AppHandle,
    windows: &SkinWindows,
    measures: &MeasureStore,
    label: Option<&str>,
    bang: &Bang,
) -> Result<(), String> {
    if !bang.is_bang() {
        logging::logger().log(
            Level::Warning,
            Some("bang"),
            format!("Commande externe non prise en charge : {}", bang.name),
        );
        return Ok(());
    }

    match bang.name.as_str() {
        "REFRESH" => {
            let label = require_label(label)?;
            emit(app, label, SkinCommand::Refresh)?;
            logging::notice(format!("Skin {label} rechargé"));
        }
        "REDRAW" => emit(app, require_label(label)?, SkinCommand::Redraw)?,
        "UPDATE" => emit(app, require_label(label)?, SkinCommand::Update)?,
        "HIDE" | "HIDEFADE" => {
            window(app, require_label(label)?)?
                .hide()
                .map_err(|error| error.to_string())?;
        }
        "SHOW" | "SHOWFADE" => {
            window(app, require_label(label)?)?
                .show()
                .map_err(|error| error.to_string())?;
        }
        "TOGGLE" | "TOGGLEFADE" => {
            let label = require_label(label)?;
            let window = window(app, label)?;
            if window.is_visible().unwrap_or(false) {
                window.hide().map_err(|error| error.to_string())?;
            } else {
                window.show().map_err(|error| error.to_string())?;
            }
        }
        "MOVE" => {
            let label = require_label(label)?;
            let x = bang
                .number(0)
                .ok_or_else(|| "position X manquante".to_string())?;
            let y = bang
                .number(1)
                .ok_or_else(|| "position Y manquante".to_string())?;
            window(app, label)?
                .set_position(LogicalPosition::new(x, y))
                .map_err(|error| error.to_string())?;
            persist(windows, label, |state| {
                state.window_x = x.round() as i32;
                state.window_y = y.round() as i32;
                state.save_position = true;
            });
        }
        "ZPOS" | "CHANGEZPOS" => {
            let label = require_label(label)?;
            let value = bang.number(0).unwrap_or(0.0);
            let on_top = value > 0.0;
            window(app, label)?
                .set_always_on_top(on_top)
                .map_err(|error| error.to_string())?;
            persist(windows, label, |state| {
                state.always_on_top = i32::from(on_top);
            });
        }
        "CLICKTHROUGH" => {
            let label = require_label(label)?;
            let value = bang.bool_arg(0);
            window(app, label)?
                .set_ignore_cursor_events(value)
                .map_err(|error| error.to_string())?;
            persist(windows, label, |state| state.click_through = value);
        }
        "DRAGGABLE" => {
            let label = require_label(label)?;
            let value = bang.bool_arg(0);
            windows.set_draggable(label, value);
            persist(windows, label, |state| state.draggable = value);
            emit(app, label, SkinCommand::Draggable { value })?;
        }
        "SETVARIABLE" => {
            let label = require_label(label)?;
            let name = bang
                .arg(0)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "nom de variable manquant".to_string())?;
            let value = bang.arg(1).unwrap_or_default();
            let info = windows
                .info(label)
                .ok_or_else(|| format!("skin introuvable : {label}"))?;
            measures.set_variable(&info.path, name, value);
            emit(app, label, SkinCommand::Refresh)?;
        }
        "SETOPTION" => {
            let label = require_label(label)?;
            let section = bang
                .arg(0)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "section manquante".to_string())?;
            let option = bang
                .arg(1)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "option manquante".to_string())?;
            let value = bang.args[2..].join(" ");
            let info = windows
                .info(label)
                .ok_or_else(|| format!("skin introuvable : {label}"))?;
            measures.set_option(&info.path, section, option, value.trim());
            emit(app, label, SkinCommand::Refresh)?;
        }
        "LOG" => {
            let message = bang.args.join(" ");
            logging::logger().log(Level::Notice, Some("skin"), message);
        }
        "ABOUT" | "MANAGE" => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        "QUIT" => app.exit(0),
        "REFRESHAPP" => app.restart(),
        "ACTIVATECONFIG" => {
            let path = resolve_config_path(bang).ok_or_else(|| "skin introuvable".to_string())?;
            windows::open_window(app, windows, path)?;
        }
        "DEACTIVATECONFIG" => {
            let path = resolve_config_path(bang).ok_or_else(|| "skin introuvable".to_string())?;
            if let Some(label) = windows.find_by_path(&path) {
                windows::close(app, windows, &label)?;
            }
        }
        "TOGGLECONFIG" => {
            let path = resolve_config_path(bang).ok_or_else(|| "skin introuvable".to_string())?;
            windows::toggle_window(app, windows, path)?;
        }
        other => {
            logging::logger().log(
                Level::Warning,
                Some("bang"),
                format!("Bang non pris en charge : !{other}"),
            );
        }
    }

    Ok(())
}

fn require_label(label: Option<&str>) -> Result<&str, String> {
    label.ok_or_else(|| "aucune fenêtre cible".to_string())
}

fn window(app: &AppHandle, label: &str) -> Result<WebviewWindow, String> {
    app.get_webview_window(label)
        .ok_or_else(|| format!("fenêtre introuvable : {label}"))
}

fn emit(app: &AppHandle, label: &str, command: SkinCommand) -> Result<(), String> {
    app.emit_to(EventTarget::webview_window(label), COMMAND_EVENT, command)
        .map_err(|error| error.to_string())
}

fn persist(windows: &SkinWindows, label: &str, update: impl FnOnce(&mut SkinState)) {
    if let Some(info) = windows.info(label) {
        persistence::update_state(&info.path, update);
    }
}

fn resolve_config_path(bang: &Bang) -> Option<PathBuf> {
    let first = bang.arg(0)?;
    let mut path = PathBuf::from(first);

    if let Some(second) = bang.arg(1) {
        path = path.join(second);
    }

    if path.is_file() {
        return Some(path);
    }

    if path.is_dir() {
        return std::fs::read_dir(path)
            .ok()?
            .flatten()
            .map(|entry| entry.path())
            .find(|candidate| is_ini(candidate));
    }

    None
}

fn is_ini(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("ini"))
        .unwrap_or(false)
}
