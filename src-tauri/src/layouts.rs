use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use petrichor_core::{ApplicationConfig, GlobalSettings, IniFile, Registry};
use serde::Serialize;

use crate::commands;
use crate::persistence;
use crate::storage;
use crate::windows::{self, SkinWindows};

#[derive(Debug, Clone, Serialize)]
pub struct LayoutInfo {
    pub name: String,
    pub path: PathBuf,
    pub modified_ms: u64,
    pub wallpaper: bool,
    pub skin_count: usize,
    pub is_favorite: bool,
}

pub fn layouts_dir() -> Option<PathBuf> {
    persistence::config_path().and_then(|path| path.parent().map(|parent| parent.join("Layouts")))
}

pub fn layout_path(name: &str) -> Option<PathBuf> {
    layout_dir(name).map(|dir| dir.join("Petrichor.ini"))
}

fn layout_dir(name: &str) -> Option<PathBuf> {
    let name = name.trim();
    if !safe_name(name) {
        return None;
    }
    layouts_dir().map(|dir| dir.join(name))
}

pub fn list() -> Vec<String> {
    list_info().into_iter().map(|info| info.name).collect()
}

pub fn list_info() -> Vec<LayoutInfo> {
    let Some(dir) = layouts_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let favorites = storage::favorite_layouts();
    let mut layouts: Vec<LayoutInfo> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if !safe_name(&name) || name.eq_ignore_ascii_case("@Backup") {
                return None;
            }
            let path = entry.path().join("Petrichor.ini");
            if !path.is_file() {
                return None;
            }
            let modified_ms = std::fs::metadata(&path)
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis() as u64)
                .unwrap_or(0);
            let config = ApplicationConfig::load(&path).ok();
            let skin_count = config
                .as_ref()
                .map(|config| {
                    config
                        .skin_sections()
                        .filter(|section| section.ends_with(".ini"))
                        .count()
                })
                .unwrap_or(0);
            Some(LayoutInfo {
                is_favorite: favorites.contains(&name.to_lowercase()),
                name,
                path: entry.path(),
                modified_ms,
                wallpaper: find_wallpaper(&entry.path()).is_some(),
                skin_count,
            })
        })
        .collect();
    layouts.sort_by_key(|layout| layout.name.to_lowercase());
    layouts
}

pub fn save(
    name: &str,
    exclude_unused: bool,
    include_wallpaper: bool,
    active: &HashSet<String>,
) -> Result<LayoutInfo, String> {
    let dir = layout_dir(name).ok_or_else(|| format!("nom de layout invalide : {name}"))?;
    let source =
        persistence::config_path().ok_or_else(|| "configuration introuvable".to_string())?;
    let mut config = ApplicationConfig::load(&source).map_err(|error| error.to_string())?;
    if exclude_unused {
        config.ini = filter_skin_sections(&config.ini, active);
    }
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    save_atomic(&config, &dir.join("Petrichor.ini"))?;
    if include_wallpaper {
        save_wallpaper(&dir)?;
    }
    let info = list_info()
        .into_iter()
        .find(|layout| layout.name.eq_ignore_ascii_case(name.trim()))
        .ok_or_else(|| "layout enregistré mais introuvable".to_string())?;
    Ok(info)
}

pub fn delete(name: &str) -> Result<(), String> {
    if name.trim().eq_ignore_ascii_case("@Backup") {
        return Err("le layout @Backup ne peut pas être supprimé".to_string());
    }
    let dir = layout_dir(name).ok_or_else(|| format!("nom de layout invalide : {name}"))?;
    if !dir.is_dir() {
        return Err(format!("layout introuvable : {name}"));
    }
    let root = layouts_dir().ok_or_else(|| "répertoire des layouts introuvable".to_string())?;
    let canonical_root = root.canonicalize().unwrap_or(root);
    let canonical_dir = dir.canonicalize().map_err(|error| error.to_string())?;
    if !canonical_dir.starts_with(&canonical_root) || canonical_dir == canonical_root {
        return Err("chemin de layout hors du répertoire autorisé".to_string());
    }
    std::fs::remove_dir_all(&canonical_dir).map_err(|error| error.to_string())
}

pub fn edit(name: &str) -> Result<(), String> {
    let dir = layout_dir(name).ok_or_else(|| format!("nom de layout invalide : {name}"))?;
    let path = dir.join("Petrichor.ini");
    if !path.is_file() {
        return Err(format!("layout introuvable : {name}"));
    }
    Command::new("xdg-open")
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("éditeur indisponible : {error}"))
}

pub fn load(app: &tauri::AppHandle, windows: &SkinWindows, name: &str) -> Result<usize, String> {
    let path = layout_path(name).ok_or_else(|| format!("layout invalide : {name}"))?;
    if !path.is_file() {
        return Err(format!("layout introuvable : {name}"));
    }
    let destination =
        persistence::config_path().ok_or_else(|| "configuration introuvable".to_string())?;

    let current = ApplicationConfig::load(&destination).ok();
    let mut config = ApplicationConfig::load(&path).map_err(|error| error.to_string())?;
    if let Some(current) = &current {
        config.set_global(&preserved_global(&config.global(), &current.global()));
        if config.ini.section("TrayMeasure").is_none() {
            if let Some(section) = current.ini.section("TrayMeasure") {
                for (key, value) in section.iter() {
                    config.ini.set("TrayMeasure", key, value);
                }
            }
        }
    }

    backup_current(&destination)?;
    save_atomic(&config, &destination)?;
    if let Some(wallpaper) = find_wallpaper(path.parent().unwrap_or(&path)) {
        if let Err(error) = crate::bangs::set_wallpaper(&wallpaper.to_string_lossy()) {
            petrichor_core::logging::warning(format!(
                "Fond d'écran du layout non appliqué : {error}"
            ));
        }
    }

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

fn backup_current(destination: &Path) -> Result<(), String> {
    let Some(parent) = destination.parent() else {
        return Ok(());
    };
    let backup = parent.join("Layouts").join("@Backup");
    std::fs::create_dir_all(&backup).map_err(|error| error.to_string())?;
    if destination.is_file() {
        std::fs::copy(destination, backup.join("Petrichor.ini"))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn save_atomic(config: &ApplicationConfig, destination: &Path) -> Result<(), String> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temporary = destination.with_extension("ini.tmp");
    config.save(&temporary).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, destination).map_err(|error| error.to_string())
}

fn filter_skin_sections(ini: &IniFile, active: &HashSet<String>) -> IniFile {
    let mut filtered = IniFile::new();
    for name in ini.section_names() {
        if name.eq_ignore_ascii_case("Petrichor")
            || name.eq_ignore_ascii_case("TrayMeasure")
            || !name.ends_with(".ini")
        {
            if let Some(section) = ini.section(name) {
                let target = filtered.section_mut(name);
                for (key, value) in section.iter() {
                    target.set(key, value);
                }
            }
            continue;
        }
        if active.contains(&name.to_lowercase()) {
            if let Some(section) = ini.section(name) {
                let target = filtered.section_mut(name);
                for (key, value) in section.iter() {
                    target.set(key, value);
                }
            }
        }
    }
    filtered
}

pub fn active_skin_keys(windows: &SkinWindows) -> HashSet<String> {
    windows
        .list()
        .into_iter()
        .map(|info| persistence::skin_key(&info.path).to_lowercase())
        .collect()
}

fn preserved_global(layout: &GlobalSettings, current: &GlobalSettings) -> GlobalSettings {
    let mut merged = layout.clone();
    merged.skin_path = current.skin_path.clone();
    merged.language = current.language;
    merged.disable_version_check = current.disable_version_check;
    merged.disable_auto_update = current.disable_auto_update;
    merged.tray_icon = current.tray_icon;
    merged.tray_execute_r = current.tray_execute_r.clone();
    merged.tray_execute_m = current.tray_execute_m.clone();
    merged.tray_execute_dr = current.tray_execute_dr.clone();
    merged.tray_execute_dm = current.tray_execute_dm.clone();
    merged.net_in_speed = current.net_in_speed;
    merged.net_out_speed = current.net_out_speed;
    merged
}

fn find_wallpaper(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.is_file()
                && path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .map(|stem| stem.eq_ignore_ascii_case("Wallpaper"))
                    .unwrap_or(false)
        })
}

fn save_wallpaper(dir: &Path) -> Result<(), String> {
    let Some(wallpaper) = current_wallpaper() else {
        return Ok(());
    };
    for entry in std::fs::read_dir(dir)
        .map_err(|error| error.to_string())?
        .flatten()
    {
        let path = entry.path();
        if path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(|stem| stem.eq_ignore_ascii_case("Wallpaper"))
            .unwrap_or(false)
        {
            let _ = std::fs::remove_file(path);
        }
    }
    let extension = wallpaper
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("bmp");
    std::fs::copy(&wallpaper, dir.join(format!("Wallpaper.{extension}")))
        .map(|_| ())
        .map_err(|error| format!("copie du fond d'écran impossible : {error}"))
}

fn current_wallpaper() -> Option<PathBuf> {
    if let Some(path) = gnome_wallpaper() {
        return Some(path);
    }
    kde_wallpaper()
}

fn gnome_wallpaper() -> Option<PathBuf> {
    let output = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.background", "picture-uri"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout);
    decode_wallpaper(value.trim())
}

fn kde_wallpaper() -> Option<PathBuf> {
    let path = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?
        .join("plasma-org.kde.plasma.desktop-appletsrc");
    let text = std::fs::read_to_string(path).ok()?;
    text.lines()
        .filter_map(|line| line.strip_prefix("Image="))
        .find_map(|value| decode_wallpaper(value.trim()))
}

fn decode_wallpaper(value: &str) -> Option<PathBuf> {
    let value = value.trim().trim_matches('\'').trim_matches('"');
    if value.is_empty() {
        return None;
    }
    let value = value.strip_prefix("file://").unwrap_or(value);
    let mut path = String::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&value[index + 1..index + 3], 16) {
                path.push(char::from(byte));
                index += 3;
                continue;
            }
        }
        path.push(char::from(bytes[index]));
        index += 1;
    }
    let path = PathBuf::from(path);
    path.is_file().then_some(path)
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

#[tauri::command]
pub fn save_layout(
    windows: tauri::State<'_, SkinWindows>,
    name: String,
    exclude_unused: bool,
    include_wallpaper: bool,
) -> Result<LayoutInfo, String> {
    save(
        &name,
        exclude_unused,
        include_wallpaper,
        &active_skin_keys(&windows),
    )
}

#[tauri::command]
pub fn delete_layout(name: String) -> Result<(), String> {
    delete(&name)
}

#[tauri::command]
pub fn edit_layout(name: String) -> Result<(), String> {
    edit(&name)
}

#[tauri::command]
pub fn load_layout(
    app: tauri::AppHandle,
    windows: tauri::State<'_, SkinWindows>,
    name: String,
) -> Result<usize, String> {
    load(&app, &windows, &name)
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

    #[test]
    fn filters_unused_skin_sections() {
        let ini = IniFile::parse(
            "[Petrichor]\nDebug=1\n[TrayMeasure]\nMeasure=CPU\n[Clock\\Clock.ini]\nWindowX=1\n[Other\\Other.ini]\nWindowX=2\n",
        );
        let mut active = HashSet::new();
        active.insert("clock\\clock.ini".to_string());
        let filtered = filter_skin_sections(&ini, &active);
        assert!(filtered.section("Clock\\Clock.ini").is_some());
        assert!(filtered.section("Other\\Other.ini").is_none());
        assert!(filtered.section("Petrichor").is_some());
        assert!(filtered.section("TrayMeasure").is_some());
    }

    #[test]
    fn preserves_user_global_settings() {
        let layout = GlobalSettings {
            debug: true,
            net_in_speed: 42,
            tray_icon: false,
            ..Default::default()
        };
        let current = GlobalSettings {
            debug: false,
            net_in_speed: 100,
            tray_icon: true,
            ..Default::default()
        };
        let merged = preserved_global(&layout, &current);
        assert!(merged.debug);
        assert_eq!(merged.net_in_speed, 100);
        assert!(merged.tray_icon);
    }

    #[test]
    fn decodes_wallpaper_uri() {
        let directory =
            std::env::temp_dir().join(format!("petrichor-wallpaper-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let file = directory.join("Petrichor Wallpaper.png");
        std::fs::write(&file, b"image").unwrap();
        let uri = format!("'file://{}'", file.to_string_lossy().replace(' ', "%20"));
        assert_eq!(decode_wallpaper(&uri), Some(file));
        std::fs::remove_dir_all(directory).ok();
    }
}
