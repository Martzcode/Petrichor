use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use petrichor_core::{IniFile, Registry};
use serde::{Deserialize, Serialize};

use crate::persistence;

const FAVORITES: &str = "Favorites";
const GAME_MODE: &str = "GameMode";
static DATA_LOCK: Mutex<()> = Mutex::new(());

pub fn data_path() -> Option<PathBuf> {
    persistence::config_path()
        .and_then(|path| path.parent().map(|parent| parent.join("Petrichor.data")))
}

fn load() -> IniFile {
    data_path()
        .filter(|path| path.is_file())
        .and_then(|path| IniFile::load(&path).ok())
        .unwrap_or_default()
}

fn save(ini: &IniFile) -> Result<(), String> {
    let path = data_path().ok_or_else(|| "répertoire de configuration introuvable".to_string())?;
    ini.save(&path).map_err(|error| error.to_string())
}

fn skin_favorite_key(path: &Path) -> String {
    format!("skin:{}", persistence::skin_key(path).to_lowercase())
}

fn layout_favorite_key(name: &str) -> String {
    format!("layout:{}", name.trim().to_lowercase())
}

fn is_enabled(value: Option<&str>) -> bool {
    value
        .map(|value| matches!(value.trim(), "1" | "true" | "TRUE" | "yes"))
        .unwrap_or(false)
}

pub fn favorite_skins() -> BTreeSet<String> {
    let _guard = DATA_LOCK.lock();
    let ini = load();
    ini.section(FAVORITES)
        .map(|section| {
            section
                .iter()
                .filter(|(key, value)| {
                    key.to_ascii_lowercase().starts_with("skin:") && is_enabled(Some(value))
                })
                .map(|(key, _)| key[5..].to_string())
                .collect()
        })
        .unwrap_or_default()
}

pub fn favorite_layouts() -> BTreeSet<String> {
    let _guard = DATA_LOCK.lock();
    let ini = load();
    ini.section(FAVORITES)
        .map(|section| {
            section
                .iter()
                .filter(|(key, value)| {
                    key.to_ascii_lowercase().starts_with("layout:") && is_enabled(Some(value))
                })
                .map(|(key, _)| key[7..].to_string())
                .collect()
        })
        .unwrap_or_default()
}

pub fn is_skin_favorite(path: &Path) -> bool {
    favorite_skins().contains(&persistence::skin_key(path).to_lowercase())
}

pub fn set_skin_favorite(path: &Path, favorite: bool) -> Result<(), String> {
    let _guard = DATA_LOCK.lock();
    let mut ini = load();
    let key = skin_favorite_key(path);
    if favorite {
        ini.set(FAVORITES, &key, "1");
    } else {
        ini.section_mut(FAVORITES).remove(&key);
    }
    save(&ini)
}

pub fn set_layout_favorite(name: &str, favorite: bool) -> Result<(), String> {
    let _guard = DATA_LOCK.lock();
    let mut ini = load();
    let key = layout_favorite_key(name);
    if favorite {
        ini.set(FAVORITES, &key, "1");
    } else {
        ini.section_mut(FAVORITES).remove(&key);
    }
    save(&ini)
}

pub fn apply_favorites(registry: &mut Registry) {
    let favorites = favorite_skins();
    for folder in &mut registry.folders {
        for file in &mut folder.files {
            file.is_favorite =
                favorites.contains(&persistence::skin_key(&file.path).to_lowercase());
        }
        folder.has_favorite = folder.files.iter().any(|file| file.is_favorite);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameModeConfig {
    #[serde(default)]
    pub start_action: String,
    #[serde(default)]
    pub stop_action: String,
    #[serde(default)]
    pub detect_fullscreen: bool,
    #[serde(default)]
    pub detect_process: bool,
    #[serde(default)]
    pub process_list: Vec<String>,
}

impl GameModeConfig {
    fn list_value(&self) -> String {
        self.process_list.join("|")
    }

    fn integrity(&self) -> u64 {
        let mut hash = 17 * 31u64;
        let value = format!(
            "{}{}{}{}{}",
            self.start_action,
            self.stop_action,
            self.detect_fullscreen,
            self.detect_process,
            self.list_value()
        );
        for byte in value.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }

    fn key(&self, name: &str) -> String {
        format!("{name}{}", self.integrity())
    }
}

pub fn game_mode() -> GameModeConfig {
    let _guard = DATA_LOCK.lock();
    let ini = load();
    game_mode_from_ini(&ini)
}

pub fn save_game_mode(config: &GameModeConfig) -> Result<(), String> {
    let _guard = DATA_LOCK.lock();
    let mut ini = load();
    write_game_mode(&mut ini, config);
    save(&ini)
}

fn game_mode_from_ini(ini: &IniFile) -> GameModeConfig {
    let section = match ini.section(GAME_MODE) {
        Some(section) => section,
        None => return GameModeConfig::default(),
    };
    let hash_keys = section
        .iter()
        .map(|(key, _)| key.to_string())
        .filter(|key| key.ends_with(|ch: char| ch.is_ascii_digit()))
        .collect::<Vec<_>>();
    let suffix = hash_keys
        .iter()
        .find_map(|key| key.strip_prefix("star"))
        .unwrap_or_default()
        .to_string();
    let value = |name: &str| {
        section
            .get(&format!("{name}{suffix}"))
            .or_else(|| section.get(name))
            .unwrap_or_default()
            .to_string()
    };
    GameModeConfig {
        start_action: value("star"),
        stop_action: value("stop"),
        detect_fullscreen: is_enabled(
            section
                .get(&format!("full{suffix}"))
                .or_else(|| section.get("full")),
        ),
        detect_process: is_enabled(
            section
                .get(&format!("mode{suffix}"))
                .or_else(|| section.get("mode")),
        ),
        process_list: value("list")
            .split('|')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .collect(),
    }
}

fn write_game_mode(ini: &mut IniFile, config: &GameModeConfig) {
    ini.remove_section(GAME_MODE);
    let section = ini.section_mut(GAME_MODE);
    section.set(&config.key("star"), &config.start_action);
    section.set(&config.key("stop"), &config.stop_action);
    section.set(
        &config.key("full"),
        if config.detect_fullscreen { "1" } else { "0" },
    );
    section.set(
        &config.key("mode"),
        if config.detect_process { "1" } else { "0" },
    );
    section.set(&config.key("list"), &config.list_value());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_mode_roundtrip_uses_integrity_suffix() {
        let mut ini = IniFile::new();
        let config = GameModeConfig {
            start_action: "Gaming".to_string(),
            stop_action: String::new(),
            detect_fullscreen: true,
            detect_process: true,
            process_list: vec!["game".to_string(), "retroarch".to_string()],
        };
        write_game_mode(&mut ini, &config);
        let suffix = config.integrity().to_string();
        assert!(ini.get(GAME_MODE, &format!("star{suffix}")).is_some());
        assert_eq!(game_mode_from_ini(&ini), config);
    }

    #[test]
    fn integrity_changes_with_content() {
        let mut config = GameModeConfig::default();
        let first = config.integrity();
        config.start_action = "Gaming".to_string();
        assert_ne!(first, config.integrity());
    }
}
