use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, EventTarget, LogicalPosition, Manager, PhysicalPosition, WebviewWindow,
};

use petrichor_core::bang::{self, Bang};
use petrichor_core::logging::{self, Level};
use petrichor_core::SkinState;
use petrichor_sys::SystemBackend;

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
    Draggable {
        value: bool,
    },
    #[serde(rename = "mouseActions")]
    MouseActions {
        target: String,
        actions: Vec<String>,
        state: String,
    },
    Opacity {
        value: u8,
    },
    Fade {
        show: bool,
        duration_ms: u32,
    },
    Selection {
        active: bool,
        selected: bool,
        color: String,
    },
    #[serde(rename = "onHover")]
    OnHover {
        value: u8,
    },
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
    let root = path.as_deref().and_then(Path::parent);

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

        if !bang.is_bang() {
            match bang.name.as_str() {
                "PLAY" => play_sound(root, &bang, false, &bang.name),
                "PLAYLOOP" => play_sound(root, &bang, true, &bang.name),
                "PLAYSTOP" => stop_sound(),
                _ => spawn_external(&bang),
            }
            continue;
        }

        if bang.name == "DELAY" {
            let milliseconds = bang.number(0).unwrap_or(0.0).clamp(0.0, 60_000.0);
            std::thread::sleep(Duration::from_millis(milliseconds as u64));
            continue;
        }

        if bang.name == "DEACTIVATECONFIGGROUP" {
            if let Err(error) = deactivate_group(app, windows, &bang) {
                log_error(&bang, error);
            }
            continue;
        }

        if let Some((base, wildcard)) = skin_group_bang(&bang.name) {
            if let Err(error) = execute_skin_group(app, windows, measures, &bang, base, wildcard) {
                log_error(&bang, error);
            }
            continue;
        }

        if let Err(error) = execute(app, windows, measures, label, &bang) {
            log_error(&bang, error);
        }
    }
}

fn log_error(bang: &Bang, error: String) {
    logging::logger().log(
        Level::Warning,
        Some("bang"),
        format!("Bang {} ignoré : {error}", bang.name),
    );
}

fn execute_skin_group(
    app: &AppHandle,
    windows: &SkinWindows,
    measures: &MeasureStore,
    bang: &Bang,
    base: &str,
    wildcard_meter: bool,
) -> Result<(), String> {
    let mut args = bang.args.clone();
    let target = args
        .pop()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "groupe cible manquant".to_string())?;
    let targets = windows.find_targets(&target);
    if targets.is_empty() {
        return Err(format!("aucun skin dans le groupe {target}"));
    }

    if wildcard_meter {
        args.insert(0, "*".to_string());
    }
    let inner = Bang {
        name: base.to_string(),
        raw_name: base.to_string(),
        args,
        external: false,
    };

    for label in targets {
        if let Err(error) = execute(app, windows, measures, Some(&label), &inner) {
            logging::logger().log(
                Level::Warning,
                Some("bang"),
                format!("Bang {base} ignoré pour {label} : {error}"),
            );
        }
    }
    Ok(())
}

fn skin_group_bang(name: &str) -> Option<(&'static str, bool)> {
    Some(match name {
        "REFRESHGROUP" => ("REFRESH", false),
        "UPDATEGROUP" => ("UPDATE", false),
        "REDRAWGROUP" => ("REDRAW", false),
        "HIDEGROUP" => ("HIDE", false),
        "SHOWGROUP" => ("SHOW", false),
        "TOGGLEGROUP" => ("TOGGLE", false),
        "HIDEFADEGROUP" => ("HIDEFADE", false),
        "SHOWFADEGROUP" => ("SHOWFADE", false),
        "TOGGLEFADEGROUP" => ("TOGGLEFADE", false),
        "ZPOSGROUP" => ("ZPOS", false),
        "CLICKTHROUGHGROUP" => ("CLICKTHROUGH", false),
        "DRAGGABLEGROUP" => ("DRAGGABLE", false),
        "SNAPEDGESGROUP" => ("SNAPEDGES", false),
        "FADEDURATIONGROUP" => ("FADEDURATION", false),
        "KEEPONSCREENGROUP" => ("KEEPONSCREEN", false),
        "AUTOSELECTSCREENGROUP" => ("AUTOSELECTSCREEN", false),
        "SETTRANSPARENCYGROUP" => ("SETTRANSPARENCY", false),
        "SETVARIABLEGROUP" => ("SETVARIABLE", false),
        "DISABLEMOUSEACTIONSKINGROUP" => ("DISABLEMOUSEACTION", true),
        "CLEARMOUSEACTIONSKINGROUP" => ("CLEARMOUSEACTION", true),
        "ENABLEMOUSEACTIONSKINGROUP" => ("ENABLEMOUSEACTION", true),
        "TOGGLEMOUSEACTIONSKINGROUP" => ("TOGGLEMOUSEACTION", true),
        _ => return None,
    })
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
        "HIDE" => {
            let label = require_label(label)?;
            hide_window(app, label)?;
            emit_current_opacity(app, windows, label)?;
        }
        "SHOW" => {
            let label = require_label(label)?;
            show_window(app, label)?;
            emit_current_opacity(app, windows, label)?;
        }
        "TOGGLE" => {
            let label = require_label(label)?;
            let visible = window(app, label)?.is_visible().unwrap_or(false);
            if visible {
                hide_window(app, label)?;
            } else {
                show_window(app, label)?;
            }
            emit_current_opacity(app, windows, label)?;
        }
        "HIDEFADE" => start_fade(app, windows, require_label(label)?, false)?,
        "SHOWFADE" => {
            let label = require_label(label)?;
            show_window(app, label)?;
            start_fade(app, windows, label, true)?;
        }
        "TOGGLEFADE" => {
            let label = require_label(label)?;
            let visible = window(app, label)?.is_visible().unwrap_or(false);
            if !visible {
                show_window(app, label)?;
            }
            start_fade(app, windows, label, !visible)?;
        }
        "FADEDURATION" => {
            let label = require_label(label)?;
            let duration = bang.number(0).unwrap_or(250.0).clamp(0.0, 60_000.0) as u32;
            persist(windows, label, |state| state.fade_duration = duration);
        }
        "SETTRANSPARENCY" => {
            let label = require_label(label)?;
            let alpha = bang.number(0).unwrap_or(255.0).clamp(0.0, 255.0) as u8;
            persist(windows, label, |state| state.alpha_value = alpha);
            emit(app, label, SkinCommand::Opacity { value: alpha })?;
        }
        "HIDEMETER" => set_meter_visibility(
            app,
            windows,
            measures,
            require_label(label)?,
            bang.arg(0),
            true,
        )?,
        "SHOWMETER" => set_meter_visibility(
            app,
            windows,
            measures,
            require_label(label)?,
            bang.arg(0),
            false,
        )?,
        "TOGGLEMETER" => {
            let label = require_label(label)?;
            let name = require_arg(bang, 0, "meter manquant")?;
            let hidden = meter_hidden(windows, measures, label, name)?;
            set_meter_visibility(app, windows, measures, label, Some(name), !hidden)?;
        }
        "HIDEMETERGROUP" => set_meter_group_visibility(
            app,
            windows,
            measures,
            require_label(label)?,
            bang.arg(0),
            Some(true),
        )?,
        "SHOWMETERGROUP" => set_meter_group_visibility(
            app,
            windows,
            measures,
            require_label(label)?,
            bang.arg(0),
            Some(false),
        )?,
        "TOGGLEMETERGROUP" => set_meter_group_visibility(
            app,
            windows,
            measures,
            require_label(label)?,
            bang.arg(0),
            None,
        )?,
        "UPDATEMETER" | "UPDATEMETERGROUP" => {
            emit(app, require_label(label)?, SkinCommand::Refresh)?
        }
        "MOVEMETER" => move_meter(app, windows, measures, require_label(label)?, bang)?,
        "DISABLEMEASURE" => set_measure_flag(
            app,
            windows,
            measures,
            require_label(label)?,
            bang.arg(0),
            "Disabled",
            Some(true),
        )?,
        "ENABLEMEASURE" => set_measure_flag(
            app,
            windows,
            measures,
            require_label(label)?,
            bang.arg(0),
            "Disabled",
            Some(false),
        )?,
        "TOGGLEMEASURE" => toggle_measure_flag(
            app,
            windows,
            measures,
            require_label(label)?,
            bang.arg(0),
            "Disabled",
        )?,
        "PAUSEMEASURE" => set_measure_flag(
            app,
            windows,
            measures,
            require_label(label)?,
            bang.arg(0),
            "Paused",
            Some(true),
        )?,
        "UNPAUSEMEASURE" => set_measure_flag(
            app,
            windows,
            measures,
            require_label(label)?,
            bang.arg(0),
            "Paused",
            Some(false),
        )?,
        "TOGGLEPAUSEMEASURE" => toggle_measure_flag(
            app,
            windows,
            measures,
            require_label(label)?,
            bang.arg(0),
            "Paused",
        )?,
        "DISABLEMEASUREGROUP" => set_measure_group_flag(
            app,
            windows,
            measures,
            require_label(label)?,
            bang.arg(0),
            "Disabled",
            Some(true),
        )?,
        "ENABLEMEASUREGROUP" => set_measure_group_flag(
            app,
            windows,
            measures,
            require_label(label)?,
            bang.arg(0),
            "Disabled",
            Some(false),
        )?,
        "TOGGLEMEASUREGROUP" => toggle_measure_group_flag(
            app,
            windows,
            measures,
            require_label(label)?,
            bang.arg(0),
            "Disabled",
        )?,
        "PAUSEMEASUREGROUP" => set_measure_group_flag(
            app,
            windows,
            measures,
            require_label(label)?,
            bang.arg(0),
            "Paused",
            Some(true),
        )?,
        "UNPAUSEMEASUREGROUP" => set_measure_group_flag(
            app,
            windows,
            measures,
            require_label(label)?,
            bang.arg(0),
            "Paused",
            Some(false),
        )?,
        "TOGGLEPAUSEMEASUREGROUP" => toggle_measure_group_flag(
            app,
            windows,
            measures,
            require_label(label)?,
            bang.arg(0),
            "Paused",
        )?,
        "UPDATEMEASURE" | "UPDATEMEASUREGROUP" => {
            emit(app, require_label(label)?, SkinCommand::Update)?
        }
        "DISABLEMOUSEACTION" => set_mouse_action(
            app,
            windows,
            require_label(label)?,
            bang.arg(0),
            bang.arg(1),
            "disabled",
            false,
        )?,
        "CLEARMOUSEACTION" => set_mouse_action(
            app,
            windows,
            require_label(label)?,
            bang.arg(0),
            bang.arg(1),
            "cleared",
            false,
        )?,
        "ENABLEMOUSEACTION" => set_mouse_action(
            app,
            windows,
            require_label(label)?,
            bang.arg(0),
            bang.arg(1),
            "enabled",
            false,
        )?,
        "TOGGLEMOUSEACTION" => set_mouse_action(
            app,
            windows,
            require_label(label)?,
            bang.arg(0),
            bang.arg(1),
            "toggle",
            false,
        )?,
        "DISABLEMOUSEACTIONGROUP" => {
            set_mouse_action_group(app, windows, require_label(label)?, bang, "disabled", false)?
        }
        "CLEARMOUSEACTIONGROUP" => {
            set_mouse_action_group(app, windows, require_label(label)?, bang, "cleared", false)?
        }
        "ENABLEMOUSEACTIONGROUP" => {
            set_mouse_action_group(app, windows, require_label(label)?, bang, "enabled", false)?
        }
        "TOGGLEMOUSEACTIONGROUP" => {
            set_mouse_action_group(app, windows, require_label(label)?, bang, "toggle", false)?
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
        "SETANCHOR" => {
            let label = require_label(label)?;
            let x = bang.number(0).unwrap_or(1.0);
            let y = bang.number(1).unwrap_or(1.0);
            persist(windows, label, |state| {
                state.anchor_x = x as f32;
                state.anchor_y = y as f32;
            });
        }
        "ZPOS" | "CHANGEZPOS" => {
            let label = require_label(label)?;
            let value = bang.number(0).unwrap_or(0.0) as i32;
            let window = window(app, label)?;
            window
                .set_always_on_top(value > 0)
                .map_err(|error| error.to_string())?;
            window
                .set_always_on_bottom(value <= -2)
                .map_err(|error| error.to_string())?;
            persist(windows, label, |state| state.always_on_top = value);
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
        "SNAPEDGES" => set_state_flag(windows, require_label(label)?, bang.bool_arg(0), |state| {
            &mut state.snap_edges
        }),
        "KEEPONSCREEN" => {
            set_state_flag(windows, require_label(label)?, bang.bool_arg(0), |state| {
                &mut state.keep_on_screen
            })
        }
        "AUTOSELECTSCREEN" => {
            set_state_flag(windows, require_label(label)?, bang.bool_arg(0), |state| {
                &mut state.auto_select_screen
            })
        }
        "SETVARIABLE" => {
            let label = require_label(label)?;
            let name = require_arg(bang, 0, "nom de variable manquant")?;
            let value = join_from(bang, 1);
            let info = info(windows, label)?;
            measures.set_variable(&info.path, name, &value);
            emit(app, label, SkinCommand::Refresh)?;
        }
        "SETOPTION" => {
            let label = require_label(label)?;
            let section = require_arg(bang, 0, "section manquante")?;
            let option = require_arg(bang, 1, "option manquante")?;
            let value = join_from(bang, 2);
            let info = info(windows, label)?;
            measures.set_option(&info.path, section, option, value.trim());
            emit(app, label, SkinCommand::Refresh)?;
        }
        "SETOPTIONGROUP" => {
            let label = require_label(label)?;
            let group = require_arg(bang, 0, "groupe manquant")?;
            let option = require_arg(bang, 1, "option manquante")?;
            let value = join_from(bang, 2);
            let info = info(windows, label)?;
            measures.set_option_group(&info.path, group, option, value.trim());
            emit(app, label, SkinCommand::Refresh)?;
        }
        "COMMANDMEASURE" => {
            let label = require_label(label)?;
            let name = require_arg(bang, 0, "mesure manquante")?;
            let command = require_arg(bang, 1, "commande manquante")?;
            let info = info(windows, label)?;
            let kind = measures.measure_kind(&info.path, name);
            let handled = if kind.as_deref() == Some("MEDIAKEY") {
                petrichor_sys::backend().media_key(command)
            } else {
                measures.command_measure(&info.path, name, command)
            };
            if !handled {
                return Err(format!(
                    "commande de mesure non prise en charge : {command}"
                ));
            }
            emit(app, label, SkinCommand::Refresh)?;
        }
        "PLUGINBANG" => return Err("plugins non pris en charge".to_string()),
        "SHOWBLUR" | "HIDEBLUR" | "TOGGLEBLUR" | "ADDBLUR" | "REMOVEBLUR" => {
            logging::notice("Bangs de flou acceptés (rendu natif non disponible)");
        }
        "WRITEKEYVALUE" => write_key_value(windows, require_label(label)?, bang)?,
        "SETCLIP" => set_clipboard(&join_from(bang, 0))?,
        "SETWALLPAPER" => set_wallpaper(&join_from(bang, 0))?,
        "EDITSKIN" => {
            let label = require_label(label)?;
            let info = info(windows, label)?;
            spawn_detached("xdg-open", &[info.path.to_string_lossy().to_string()])?;
        }
        "RESETSTATS" => {
            let label = require_label(label)?;
            let info = info(windows, label)?;
            measures.remove(&info.path);
            emit(app, label, SkinCommand::Refresh)?;
        }
        "DEACTIVATECONFIGGROUP" => deactivate_group(app, windows, bang)?,
        "EMPTYBIN" | "EMPTYBINSILENT" => {
            petrichor_sys::backend().empty_trash();
        }
        "OPENBIN" => {
            petrichor_sys::backend().open_trash();
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
        "SKINMENU" | "SKINCUSTOMMENU" => {
            let label = require_label(label)?;
            let menus = app.state::<crate::menu::MenuState>();
            crate::menu::show_skin(app, windows, measures, &menus, label, None)?;
        }
        "TRAYMENU" => crate::tray::show_menu(app, label)?,
        "LOADLAYOUT" => {
            let name = bang.args.join(" ");
            crate::layouts::load(app, windows, &name)?;
        }
        "SETWINDOWPOSITION" => {
            let label = require_label(label)?;
            let x = bang
                .number(0)
                .ok_or_else(|| "position X invalide".to_string())? as i32;
            let y = bang
                .number(1)
                .ok_or_else(|| "position Y invalide".to_string())? as i32;
            let target = bang
                .arg(2)
                .filter(|value| value.parse::<f64>().is_err())
                .and_then(|value| windows.find_by_path(std::path::Path::new(value)))
                .unwrap_or_else(|| label.to_string());
            let window = window(app, &target)?;
            window
                .set_position(PhysicalPosition::new(x, y))
                .map_err(|error| error.to_string())?;
            persist(windows, &target, |state| {
                state.window_x = x;
                state.window_y = y;
                state.save_position = true;
            });
        }
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

fn set_meter_visibility(
    app: &AppHandle,
    windows: &SkinWindows,
    measures: &MeasureStore,
    label: &str,
    name: Option<&str>,
    hidden: bool,
) -> Result<(), String> {
    let name = name
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "meter manquant".to_string())?;
    let info = info(windows, label)?;
    measures.set_option(&info.path, name, "Hidden", if hidden { "1" } else { "0" });
    emit(app, label, SkinCommand::Refresh)
}

fn set_meter_group_visibility(
    app: &AppHandle,
    windows: &SkinWindows,
    measures: &MeasureStore,
    label: &str,
    group: Option<&str>,
    hidden: Option<bool>,
) -> Result<(), String> {
    let group = require_optional(group, "groupe manquant")?;
    let info = info(windows, label)?;
    for name in measures.meter_names_in_group(&info.path, group) {
        let hidden = hidden.unwrap_or_else(|| measures.meter_hidden(&info.path, &name));
        measures.set_option(&info.path, &name, "Hidden", if hidden { "1" } else { "0" });
    }
    emit(app, label, SkinCommand::Refresh)
}

fn move_meter(
    app: &AppHandle,
    windows: &SkinWindows,
    measures: &MeasureStore,
    label: &str,
    bang: &Bang,
) -> Result<(), String> {
    let x = bang
        .number(0)
        .ok_or_else(|| "position X manquante".to_string())?;
    let y = bang
        .number(1)
        .ok_or_else(|| "position Y manquante".to_string())?;
    let name = require_arg(bang, 2, "meter manquant")?;
    let info = info(windows, label)?;
    measures.set_option(&info.path, name, "X", &x.to_string());
    measures.set_option(&info.path, name, "Y", &y.to_string());
    emit(app, label, SkinCommand::Refresh)
}

fn meter_hidden(
    windows: &SkinWindows,
    measures: &MeasureStore,
    label: &str,
    name: &str,
) -> Result<bool, String> {
    let info = info(windows, label)?;
    Ok(measures.meter_hidden(&info.path, name))
}

fn set_measure_flag(
    app: &AppHandle,
    windows: &SkinWindows,
    measures: &MeasureStore,
    label: &str,
    name: Option<&str>,
    option: &str,
    value: Option<bool>,
) -> Result<(), String> {
    let name = require_optional(name, "mesure manquante")?;
    let info = info(windows, label)?;
    let value = value.unwrap_or_else(|| measures.measure_flag(&info.path, name, option));
    measures.set_option(&info.path, name, option, if value { "1" } else { "0" });
    emit(app, label, SkinCommand::Refresh)
}

fn toggle_measure_flag(
    app: &AppHandle,
    windows: &SkinWindows,
    measures: &MeasureStore,
    label: &str,
    name: Option<&str>,
    option: &str,
) -> Result<(), String> {
    let name = require_optional(name, "mesure manquante")?;
    let info = info(windows, label)?;
    let value = measures.measure_flag(&info.path, name, option);
    set_measure_flag(
        app,
        windows,
        measures,
        label,
        Some(name),
        option,
        Some(!value),
    )
}

fn set_measure_group_flag(
    app: &AppHandle,
    windows: &SkinWindows,
    measures: &MeasureStore,
    label: &str,
    group: Option<&str>,
    option: &str,
    value: Option<bool>,
) -> Result<(), String> {
    let group = require_optional(group, "groupe manquant")?;
    let info = info(windows, label)?;
    for name in measures.measure_names_in_group(&info.path, group) {
        let value = value.unwrap_or_else(|| measures.measure_flag(&info.path, &name, option));
        measures.set_option(&info.path, &name, option, if value { "1" } else { "0" });
    }
    emit(app, label, SkinCommand::Refresh)
}

fn toggle_measure_group_flag(
    app: &AppHandle,
    windows: &SkinWindows,
    measures: &MeasureStore,
    label: &str,
    group: Option<&str>,
    option: &str,
) -> Result<(), String> {
    let group = require_optional(group, "groupe manquant")?;
    let info = info(windows, label)?;
    for name in measures.measure_names_in_group(&info.path, group) {
        let value = measures.measure_flag(&info.path, &name, option);
        measures.set_option(&info.path, &name, option, if value { "0" } else { "1" });
    }
    emit(app, label, SkinCommand::Refresh)
}

fn set_mouse_action(
    app: &AppHandle,
    _windows: &SkinWindows,
    label: &str,
    target: Option<&str>,
    action: Option<&str>,
    state: &str,
    group_target: bool,
) -> Result<(), String> {
    let target = if target == Some("*") {
        "*".to_string()
    } else {
        require_optional(target, "meter cible manquant")?.to_string()
    };
    let action = require_optional(action, "action souris manquante")?;
    let target = if group_target {
        format!("group:{target}")
    } else {
        target
    };
    emit(
        app,
        label,
        SkinCommand::MouseActions {
            target,
            actions: parse_action_names(action),
            state: state.to_string(),
        },
    )
}

fn set_mouse_action_group(
    app: &AppHandle,
    windows: &SkinWindows,
    label: &str,
    bang: &Bang,
    state: &str,
    _skin_group: bool,
) -> Result<(), String> {
    let first = bang.arg(0).unwrap_or_default();
    let second = bang.arg(1).unwrap_or_default();
    let (target, action) = if looks_like_action(first) && !looks_like_action(second) {
        (second, first)
    } else {
        (first, second)
    };
    set_mouse_action(app, windows, label, Some(target), Some(action), state, true)
}

fn parse_action_names(value: &str) -> Vec<String> {
    value
        .split('|')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn looks_like_action(value: &str) -> bool {
    let value = value.trim();
    value == "*" || value.to_ascii_lowercase().contains("mouse")
}

fn require_optional<'a>(value: Option<&'a str>, message: &str) -> Result<&'a str, String> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| message.to_string())
}

fn require_arg<'a>(bang: &'a Bang, index: usize, message: &str) -> Result<&'a str, String> {
    require_optional(bang.arg(index), message)
}

fn join_from(bang: &Bang, index: usize) -> String {
    bang.args
        .get(index..)
        .map(|args| args.join(" "))
        .unwrap_or_default()
}

fn set_state_flag(
    windows: &SkinWindows,
    label: &str,
    value: bool,
    field: impl FnOnce(&mut SkinState) -> &mut bool,
) {
    persist(windows, label, |state| *field(state) = value);
}

fn emit_current_opacity(app: &AppHandle, windows: &SkinWindows, label: &str) -> Result<(), String> {
    let info = info(windows, label)?;
    let alpha = persistence::load_state(&info.path)
        .map(|state| state.alpha_value)
        .unwrap_or(255);
    emit(app, label, SkinCommand::Opacity { value: alpha })
}

fn start_fade(
    app: &AppHandle,
    windows: &SkinWindows,
    label: &str,
    show: bool,
) -> Result<(), String> {
    let info = info(windows, label)?;
    let duration = persistence::load_state(&info.path)
        .map(|state| state.fade_duration)
        .unwrap_or(250);
    emit(
        app,
        label,
        SkinCommand::Fade {
            show,
            duration_ms: duration,
        },
    )
}

fn write_key_value(windows: &SkinWindows, label: &str, bang: &Bang) -> Result<(), String> {
    let section = require_arg(bang, 0, "section manquante")?;
    let key = require_arg(bang, 1, "clé manquante")?;
    let value = require_arg(bang, 2, "valeur manquante")?;
    let info = info(windows, label)?;
    let root = info
        .path
        .parent()
        .ok_or_else(|| "dossier du skin introuvable".to_string())?;
    let target = match bang.arg(3).filter(|value| !value.trim().is_empty()) {
        Some(file) => {
            let file = PathBuf::from(file);
            if file.is_absolute() {
                file
            } else {
                root.join(file)
            }
        }
        None => info.path.clone(),
    };

    let mut allowed = vec![root.to_path_buf()];
    if let Some(path) = persistence::config_path() {
        if let Some(parent) = path.parent() {
            allowed.push(parent.to_path_buf());
        }
    }
    if let Some(path) = crate::commands::skin_root() {
        allowed.push(path);
    }
    petrichor_core::write_key_value(&target, &allowed, section, key, value)
}

fn deactivate_group(app: &AppHandle, windows: &SkinWindows, bang: &Bang) -> Result<(), String> {
    let target = require_arg(bang, 0, "groupe cible manquant")?;
    let targets = windows.find_targets(target);
    if targets.is_empty() {
        return Err(format!("aucun skin dans le groupe {target}"));
    }
    for label in targets {
        windows::close(app, windows, &label)?;
    }
    Ok(())
}

fn spawn_external(bang: &Bang) {
    if let Err(error) = spawn_detached(&bang.raw_name, &bang.args) {
        logging::logger().log(
            Level::Warning,
            Some("bang"),
            format!("Commande externe {} échouée : {error}", bang.raw_name),
        );
    }
}

fn spawn_detached(program: &str, args: &[String]) -> Result<(), String> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn play_sound(root: Option<&Path>, bang: &Bang, looping: bool, name: &str) {
    let Some(file) = bang.arg(0).filter(|value| !value.trim().is_empty()) else {
        log_error(bang, "fichier audio manquant".to_string());
        return;
    };
    let path = resolve_audio_path(root, file);
    let candidates: Vec<Vec<String>> = if looping {
        vec![
            vec![
                "ffplay".into(),
                "-nodisp".into(),
                "-autoexit".into(),
                "-loop".into(),
                "0".into(),
                "-loglevel".into(),
                "quiet".into(),
                path.clone(),
            ],
            vec![
                "mpv".into(),
                "--no-video".into(),
                "--really-quiet".into(),
                "--loop=inf".into(),
                path.clone(),
            ],
        ]
    } else {
        vec![
            vec![
                "ffplay".into(),
                "-nodisp".into(),
                "-autoexit".into(),
                "-loglevel".into(),
                "quiet".into(),
                path.clone(),
            ],
            vec![
                "mpv".into(),
                "--no-video".into(),
                "--really-quiet".into(),
                path.clone(),
            ],
            vec!["paplay".into(), path.clone()],
            vec!["pw-play".into(), path.clone()],
            vec!["aplay".into(), "-q".into(), path.clone()],
        ]
    };

    let playback = playback();
    let mut children = match playback.lock() {
        Ok(children) => children,
        Err(_) => return,
    };
    children.retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_))));
    for candidate in candidates {
        match Command::new(&candidate[0])
            .args(&candidate[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => {
                children.push(child);
                logging::notice(format!("{name} : {}", candidate[0]));
                return;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                logging::logger().log(
                    Level::Warning,
                    Some("bang"),
                    format!("Lecture audio échouée : {error}"),
                );
                return;
            }
        }
    }
    logging::logger().log(
        Level::Warning,
        Some("bang"),
        "Aucun lecteur audio disponible (ffplay, mpv, paplay, pw-play, aplay)",
    );
}

fn resolve_audio_path(root: Option<&Path>, file: &str) -> String {
    let path = PathBuf::from(file);
    if path.is_absolute() {
        path.to_string_lossy().to_string()
    } else if let Some(root) = root {
        root.join(path).to_string_lossy().to_string()
    } else {
        file.to_string()
    }
}

fn playback() -> &'static Mutex<Vec<Child>> {
    static PLAYBACK: OnceLock<Mutex<Vec<Child>>> = OnceLock::new();
    PLAYBACK.get_or_init(|| Mutex::new(Vec::new()))
}

fn stop_sound() {
    if let Ok(mut children) = playback().lock() {
        for child in children.iter_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        children.clear();
    }
}

fn set_clipboard(text: &str) -> Result<(), String> {
    let candidates: [(&str, &[&str]); 3] = [
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["--clipboard", "--input"]),
    ];
    for (program, args) in candidates {
        match Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(mut child) => {
                if let Some(stdin) = child.stdin.as_mut() {
                    stdin
                        .write_all(text.as_bytes())
                        .map_err(|error| error.to_string())?;
                }
                let _ = child.wait();
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("aucun outil presse-papiers disponible".to_string())
}

pub(crate) fn set_wallpaper(path: &str) -> Result<(), String> {
    let path = PathBuf::from(path);
    if !path.is_file() {
        return Err(format!("fond d'écran introuvable : {}", path.display()));
    }
    let file = path.to_string_lossy().to_string();
    let uri = format!("file://{file}");
    let candidates: [(&str, Vec<&str>); 4] = [
        ("plasma-apply-wallpaperimage", vec![file.as_str()]),
        (
            "gsettings",
            vec![
                "set",
                "org.gnome.desktop.background",
                "picture-uri",
                uri.as_str(),
            ],
        ),
        ("feh", vec!["--bg-fill", file.as_str()]),
        ("swaybg", vec!["-i", file.as_str()]),
    ];
    for (program, args) in candidates {
        if program == "swaybg" {
            if spawn_detached(
                program,
                &args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>(),
            )
            .is_ok()
            {
                return Ok(());
            }
            continue;
        }
        match Command::new(program)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
        {
            Ok(status) if status.success() => {
                if program == "gsettings" {
                    let _ = Command::new("gsettings")
                        .args([
                            "set",
                            "org.gnome.desktop.background",
                            "picture-uri-dark",
                            &uri,
                        ])
                        .status();
                }
                return Ok(());
            }
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("aucun outil de fond d'écran disponible".to_string())
}

fn require_label(label: Option<&str>) -> Result<&str, String> {
    label.ok_or_else(|| "aucune fenêtre cible".to_string())
}

fn info(windows: &SkinWindows, label: &str) -> Result<windows::SkinWindowInfo, String> {
    windows
        .info(label)
        .ok_or_else(|| format!("skin introuvable : {label}"))
}

fn window(app: &AppHandle, label: &str) -> Result<WebviewWindow, String> {
    app.get_webview_window(label)
        .ok_or_else(|| format!("fenêtre introuvable : {label}"))
}

fn show_window(app: &AppHandle, label: &str) -> Result<(), String> {
    window(app, label)?
        .show()
        .map_err(|error| error.to_string())
}

fn hide_window(app: &AppHandle, label: &str) -> Result<(), String> {
    window(app, label)?
        .hide()
        .map_err(|error| error.to_string())
}

pub(crate) fn emit(app: &AppHandle, label: &str, command: SkinCommand) -> Result<(), String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_raw_action_names() {
        assert_eq!(
            parse_action_names("LeftMouseUp|RightMouseUpAction"),
            vec!["LeftMouseUp", "RightMouseUpAction"]
        );
        assert_eq!(parse_action_names(" * "), vec!["*"]);
        assert!(parse_action_names(" ").is_empty());
    }

    #[test]
    fn recognises_action_and_group_arguments() {
        assert!(looks_like_action("*"));
        assert!(looks_like_action("LeftMouseUpAction"));
        assert!(!looks_like_action("Top"));
    }

    #[test]
    fn maps_skin_group_bangs() {
        assert_eq!(skin_group_bang("REFRESHGROUP"), Some(("REFRESH", false)));
        assert_eq!(
            skin_group_bang("DISABLEMOUSEACTIONSKINGROUP"),
            Some(("DISABLEMOUSEACTION", true))
        );
        assert_eq!(skin_group_bang("REFRESH"), None);
    }

    #[test]
    fn resolves_audio_paths_against_skin_root() {
        let root = Path::new("/tmp/skin");
        assert_eq!(
            resolve_audio_path(Some(root), "sound.wav"),
            "/tmp/skin/sound.wav"
        );
        assert_eq!(
            resolve_audio_path(Some(root), "/tmp/other.wav"),
            "/tmp/other.wav"
        );
    }
}
