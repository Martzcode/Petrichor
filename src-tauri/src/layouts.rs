use std::collections::HashSet;
use std::path::{Path, PathBuf};

use petrichor_core::{ApplicationConfig, Registry};

use crate::commands;
use crate::persistence;
use crate::windows::{self, SkinWindows};

pub fn layouts_dir() -> Option<PathBuf> {
    persistence::config_path().and_then(|path| path.parent().map(|parent| parent.join("Layouts")))
}

pub fn layout_path(name: &str) -> Option<PathBuf> {
    let name = name.trim();
    if !safe_name(name) {
        return None;
    }
    layouts_dir().map(|dir| dir.join(name).join("Petrichor.ini"))
}

pub fn list() -> Vec<String> {
    let Some(dir) = layouts_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().join("Petrichor.ini").is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names
}

pub fn load(app: &tauri::AppHandle, windows: &SkinWindows, name: &str) -> Result<usize, String> {
    let path = layout_path(name).ok_or_else(|| format!("layout invalide : {name}"))?;
    if !path.is_file() {
        return Err(format!("layout introuvable : {name}"));
    }
    let destination =
        persistence::config_path().ok_or_else(|| "configuration introuvable".to_string())?;

    let current = persistence::config_path().and_then(|path| ApplicationConfig::load(&path).ok());
    let mut config = ApplicationConfig::load(&path).map_err(|error| error.to_string())?;
    let global = current
        .as_ref()
        .map(|current| current.global())
        .unwrap_or_default();
    config.set_global(&global);
    if config.ini.section("TrayMeasure").is_none() {
        if let Some(section) = current
            .as_ref()
            .and_then(|current| current.ini.section("TrayMeasure"))
        {
            for (key, value) in section.iter() {
                config.ini.set("TrayMeasure", key, value);
            }
        }
    }

    if let Some(parent) = destination.parent() {
        let backup = parent.join("Layouts").join("@Backup");
        std::fs::create_dir_all(&backup).map_err(|error| error.to_string())?;
        if destination.is_file() {
            std::fs::copy(&destination, backup.join("Petrichor.ini"))
                .map_err(|error| error.to_string())?;
        }
    }
    config
        .save(&destination)
        .map_err(|error| error.to_string())?;

    let existing: Vec<String> = windows.list().into_iter().map(|info| info.label).collect();
    for label in existing {
        windows::close(app, windows, &label)?;
    }

    let Some(root) = commands::skin_root() else {
        return Ok(0);
    };
    let registry = Registry::scan(root.to_string_lossy().to_string());
    let available: Vec<PathBuf> = registry
        .folders
        .iter()
        .flat_map(|folder| folder.files.iter())
        .map(|file| PathBuf::from(&file.path))
        .collect();
    let mut opened = 0;
    let mut seen = HashSet::new();
    for section in config.ini.section_names() {
        if section.eq_ignore_ascii_case("Petrichor") || !section.ends_with(".ini") {
            continue;
        }
        let relative = PathBuf::from(section.replace('\\', "/"));
        let direct = root.join(&relative);
        let skin = if direct.is_file() {
            Some(direct)
        } else {
            available
                .iter()
                .find(|file| file.ends_with(&relative))
                .cloned()
        };
        let Some(skin) = skin else {
            continue;
        };
        if !seen.insert(skin.clone()) {
            continue;
        }
        if windows::open_window(app, windows, skin).is_ok() {
            opened += 1;
        }
    }
    crate::notifications::send(
        "Petrichor",
        &format!("Layout « {name} » chargé ({opened} skins)"),
    );
    Ok(opened)
}

fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !Path::new(name)
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_names() {
        assert!(safe_name("Gaming"));
        assert!(safe_name("Bureau 2"));
        assert!(!safe_name(""));
        assert!(!safe_name(".."));
        assert!(!safe_name("../Gaming"));
        assert!(!safe_name("a/b"));
        assert!(!safe_name("a\\b"));
    }

    #[test]
    fn layout_path_uses_petrichor_directory() {
        let path = layout_path("Gaming").unwrap();
        assert!(path.ends_with("Layouts/Gaming/Petrichor.ini"));
    }
}
