use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::Manager;

use crate::layouts;
use crate::storage::{self, GameModeConfig};
use crate::windows::{self, SkinWindows};

pub struct GameModeState {
    active: AtomicBool,
    saved: Mutex<Option<Vec<PathBuf>>>,
}

impl GameModeState {
    pub fn new() -> Self {
        GameModeState {
            active: AtomicBool::new(false),
            saved: Mutex::new(None),
        }
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    fn set_active(&self, active: bool) {
        self.active.store(active, Ordering::SeqCst);
    }

    fn save_current(&self, windows: &SkinWindows) {
        let paths = windows.list().into_iter().map(|info| info.path).collect();
        if let Ok(mut saved) = self.saved.lock() {
            *saved = Some(paths);
        }
    }

    fn take_saved(&self) -> Vec<PathBuf> {
        self.saved
            .lock()
            .ok()
            .and_then(|mut saved| saved.take())
            .unwrap_or_default()
    }
}

impl Default for GameModeState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GameModeStatus {
    pub active: bool,
    pub config: GameModeConfig,
    pub fullscreen: bool,
    pub process: bool,
}

#[tauri::command]
pub fn game_mode_status(app: tauri::AppHandle) -> GameModeStatus {
    let state = app.state::<GameModeState>();
    let config = storage::game_mode();
    GameModeStatus {
        active: state.is_active(),
        fullscreen: config.detect_fullscreen && fullscreen_active(),
        process: config.detect_process && process_active(&config.process_list),
        config,
    }
}

#[tauri::command]
pub fn set_game_mode_config(config: GameModeConfig) -> Result<(), String> {
    storage::save_game_mode(&config)
}

#[tauri::command]
pub fn toggle_game_mode(app: tauri::AppHandle) -> Result<bool, String> {
    let state = app.state::<GameModeState>();
    if state.is_active() {
        leave(&app, &state)?;
        Ok(false)
    } else {
        enter(&app, &state)?;
        Ok(true)
    }
}

pub fn enter(app: &tauri::AppHandle, state: &GameModeState) -> Result<(), String> {
    if state.is_active() {
        return Ok(());
    }
    let config = storage::game_mode();
    let windows = app.state::<SkinWindows>();
    state.save_current(&windows);
    if config.start_action.trim().is_empty() {
        for info in windows.list() {
            windows::close(app, &windows, &info.label)?;
        }
    } else {
        layouts::load(app, &windows, config.start_action.trim())?;
    }
    state.set_active(true);
    crate::notifications::send("Petrichor", "Game Mode activé");
    petrichor_core::logging::notice("Game Mode activé");
    Ok(())
}

pub fn leave(app: &tauri::AppHandle, state: &GameModeState) -> Result<(), String> {
    if !state.is_active() {
        return Ok(());
    }
    let config = storage::game_mode();
    let windows = app.state::<SkinWindows>();
    if !config.stop_action.trim().is_empty() {
        layouts::load(app, &windows, config.stop_action.trim())?;
    } else {
        for info in windows.list() {
            windows::close(app, &windows, &info.label)?;
        }
        for path in state.take_saved() {
            windows::open_window(app, &windows, path)?;
        }
    }
    state.take_saved();
    state.set_active(false);
    crate::notifications::send("Petrichor", "Game Mode désactivé");
    petrichor_core::logging::notice("Game Mode désactivé");
    Ok(())
}

pub fn spawn(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(2));
        loop {
            let config = storage::game_mode();
            if config.detect_fullscreen || config.detect_process {
                let should_enter = (config.detect_fullscreen && fullscreen_active())
                    || (config.detect_process && process_active(&config.process_list));
                let state = app.state::<GameModeState>();
                if should_enter && !state.is_active() {
                    if let Err(error) = enter(&app, &state) {
                        petrichor_core::logging::warning(format!(
                            "Activation automatique du Game Mode impossible : {error}"
                        ));
                    }
                } else if !should_enter && state.is_active() {
                    if let Err(error) = leave(&app, &state) {
                        petrichor_core::logging::warning(format!(
                            "Désactivation automatique du Game Mode impossible : {error}"
                        ));
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(3));
        }
    });
}

fn fullscreen_active() -> bool {
    if std::env::var_os("DISPLAY").is_none() {
        return false;
    }
    let Some(window) = active_window() else {
        return false;
    };
    let output = match Command::new("xprop")
        .args(["-id", &window, "_NET_WM_STATE"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return false,
    };
    parse_fullscreen(&String::from_utf8_lossy(&output.stdout))
}

fn active_window() -> Option<String> {
    let output = Command::new("xprop")
        .args(["-root", "_NET_ACTIVE_WINDOW"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_active_window(&String::from_utf8_lossy(&output.stdout))
}

fn parse_active_window(output: &str) -> Option<String> {
    let id = output.split('#').nth(1)?.trim();
    (id.starts_with("0x") && id.len() > 2).then(|| id.to_string())
}

fn parse_fullscreen(output: &str) -> bool {
    output.contains("_NET_WM_STATE_FULLSCREEN")
}

fn process_active(names: &[String]) -> bool {
    if names.is_empty() {
        return false;
    }
    let expected: Vec<String> = names
        .iter()
        .map(|name| {
            std::path::Path::new(name.trim())
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or(name)
                .to_lowercase()
        })
        .filter(|name| !name.is_empty())
        .collect();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false;
    };
    entries.flatten().any(|entry| {
        let name = entry.file_name();
        if !name.to_string_lossy().chars().all(|ch| ch.is_ascii_digit()) {
            return false;
        }
        let Ok(comm) = std::fs::read_to_string(entry.path().join("comm")) else {
            return false;
        };
        process_matches(comm.trim(), &expected)
    })
}

fn process_matches(process: &str, expected: &[String]) -> bool {
    let process = process.trim().to_lowercase();
    expected
        .iter()
        .any(|name| process == *name || process.starts_with(&format!("{name} ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fullscreen_state() {
        assert!(parse_fullscreen(
            "_NET_WM_STATE(ATOM) = _NET_WM_STATE_FULLSCREEN"
        ));
        assert!(!parse_fullscreen(
            "_NET_WM_STATE(ATOM) = _NET_WM_STATE_ABOVE"
        ));
    }

    #[test]
    fn parses_active_window_id() {
        assert_eq!(
            parse_active_window("_NET_ACTIVE_WINDOW(WINDOW): window id # 0x3a00007"),
            Some("0x3a00007".to_string())
        );
        assert_eq!(
            parse_active_window("_NET_ACTIVE_WINDOW(WINDOW): window id # 0x0"),
            Some("0x0".to_string())
        );
        assert_eq!(parse_active_window("invalide"), None);
    }

    #[test]
    fn matches_process_names() {
        let expected = vec!["retroarch".to_string(), "game".to_string()];
        assert!(process_matches("retroarch", &expected));
        assert!(process_matches("game", &expected));
        assert!(!process_matches("gamepad", &expected));
    }
}
