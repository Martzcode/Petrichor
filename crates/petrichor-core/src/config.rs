use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

#[derive(Debug, Clone, Default)]
pub struct Section {
    pub name: String,
    entries: Vec<(String, String)>,
}

impl Section {
    pub fn new(name: impl Into<String>) -> Self {
        Section {
            name: name.into(),
            entries: Vec::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    pub fn set(&mut self, key: &str, value: &str) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
        {
            entry.1 = value.to_string();
        } else {
            self.entries.push((key.to_string(), value.to_string()));
        }
    }

    pub fn remove(&mut self, key: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|(k, _)| !k.eq_ignore_ascii_case(key));
        self.entries.len() != before
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct IniFile {
    sections: Vec<Section>,
}

impl IniFile {
    pub fn new() -> Self {
        IniFile {
            sections: Vec::new(),
        }
    }

    pub fn parse(text: &str) -> Self {
        let mut ini = IniFile::new();
        let mut current: Option<usize> = None;

        for raw_line in text.lines() {
            let line = raw_line.trim_matches(|c: char| c == '\u{feff}').trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                let name = line[1..line.len() - 1].trim().to_string();
                ini.sections.push(Section::new(name));
                current = Some(ini.sections.len() - 1);
                continue;
            }

            if let Some(eq) = line.find('=') {
                let key = line[..eq].trim();
                let value = line[eq + 1..].trim();
                if key.is_empty() {
                    continue;
                }
                let index = match current {
                    Some(index) => index,
                    None => {
                        ini.sections.push(Section::new(String::new()));
                        current = Some(ini.sections.len() - 1);
                        ini.sections.len() - 1
                    }
                };
                ini.sections[index].set(key, value);
            }
        }

        if ini
            .sections
            .first()
            .map(|s| s.name.is_empty() && s.is_empty())
            .unwrap_or(false)
        {
            ini.sections.remove(0);
        }

        ini
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|e| CoreError::io(path, e))?;
        Ok(IniFile::parse(&text))
    }

    pub fn section(&self, name: &str) -> Option<&Section> {
        self.sections
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }

    pub fn section_mut(&mut self, name: &str) -> &mut Section {
        if let Some(index) = self
            .sections
            .iter()
            .position(|s| s.name.eq_ignore_ascii_case(name))
        {
            return &mut self.sections[index];
        }
        self.sections.push(Section::new(name));
        self.sections.last_mut().unwrap()
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.section(section).and_then(|s| s.get(key))
    }

    pub fn set(&mut self, section: &str, key: &str, value: &str) {
        self.section_mut(section).set(key, value);
    }

    pub fn remove_section(&mut self, name: &str) -> bool {
        let before = self.sections.len();
        self.sections.retain(|s| !s.name.eq_ignore_ascii_case(name));
        self.sections.len() != before
    }

    pub fn section_names(&self) -> impl Iterator<Item = &str> {
        self.sections.iter().map(|s| s.name.as_str())
    }

    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        for section in &self.sections {
            if section.name.is_empty() && section.entries.is_empty() {
                continue;
            }
            if !out.is_empty() {
                out.push('\n');
            }
            out.push('[');
            out.push_str(&section.name);
            out.push_str("]\n");
            for (key, value) in &section.entries {
                out.push_str(key);
                out.push('=');
                out.push_str(value);
                out.push('\n');
            }
        }
        out
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
        }
        std::fs::write(path, self.render()).map_err(|e| CoreError::io(path, e))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSettings {
    pub debug: bool,
    pub logging: bool,
    pub net_in_speed: u64,
    pub net_out_speed: u64,
    pub disable_dragging: bool,
    pub disable_version_check: bool,
    pub disable_auto_update: bool,
    pub hardware_acceleration: u8,
    pub skin_path: PathBuf,
    pub language: Option<u32>,
    pub tray_icon: bool,
    pub normal_stay_desktop: bool,
}

impl Default for GlobalSettings {
    fn default() -> Self {
        GlobalSettings {
            debug: false,
            logging: false,
            net_in_speed: 1_000_000,
            net_out_speed: 1_000_000,
            disable_dragging: false,
            disable_version_check: false,
            disable_auto_update: false,
            hardware_acceleration: 2,
            skin_path: PathBuf::new(),
            language: None,
            tray_icon: true,
            normal_stay_desktop: false,
        }
    }
}

pub const RAINMETER_SECTION: &str = "Petrichor";

impl GlobalSettings {
    pub fn from_ini(ini: &IniFile) -> Self {
        let mut settings = GlobalSettings::default();
        let section = match ini.section(RAINMETER_SECTION) {
            Some(section) => section,
            None => return settings,
        };

        if let Some(value) = section.get("Debug") {
            settings.debug = parse_bool(value);
        }
        if let Some(value) = section.get("Logging") {
            settings.logging = parse_bool(value);
        }
        if let Some(value) = section.get("NetInSpeed") {
            settings.net_in_speed = value.parse().unwrap_or(settings.net_in_speed);
        }
        if let Some(value) = section.get("NetOutSpeed") {
            settings.net_out_speed = value.parse().unwrap_or(settings.net_out_speed);
        }
        if let Some(value) = section.get("DisableDragging") {
            settings.disable_dragging = parse_bool(value);
        }
        if let Some(value) = section.get("DisableVersionCheck") {
            settings.disable_version_check = parse_bool(value);
        }
        if let Some(value) = section.get("DisableAutoUpdate") {
            settings.disable_auto_update = parse_bool(value);
        }
        if let Some(value) = section.get("HardwareAcceleration") {
            settings.hardware_acceleration =
                value.parse().unwrap_or(settings.hardware_acceleration);
        }
        if let Some(value) = section.get("SkinPath") {
            settings.skin_path = PathBuf::from(value);
        }
        if let Some(value) = section.get("Language") {
            settings.language = value.parse().ok();
        }
        if let Some(value) = section.get("TrayIcon") {
            settings.tray_icon = parse_bool(value);
        }
        if let Some(value) = section.get("NormalStayDesktop") {
            settings.normal_stay_desktop = parse_bool(value);
        }

        settings
    }

    pub fn apply_to_ini(&self, ini: &mut IniFile) {
        ini.set(RAINMETER_SECTION, "Debug", bool_str(self.debug));
        ini.set(RAINMETER_SECTION, "Logging", bool_str(self.logging));
        ini.set(
            RAINMETER_SECTION,
            "NetInSpeed",
            &self.net_in_speed.to_string(),
        );
        ini.set(
            RAINMETER_SECTION,
            "NetOutSpeed",
            &self.net_out_speed.to_string(),
        );
        ini.set(
            RAINMETER_SECTION,
            "DisableDragging",
            bool_str(self.disable_dragging),
        );
        ini.set(
            RAINMETER_SECTION,
            "DisableVersionCheck",
            bool_str(self.disable_version_check),
        );
        ini.set(
            RAINMETER_SECTION,
            "DisableAutoUpdate",
            bool_str(self.disable_auto_update),
        );
        ini.set(
            RAINMETER_SECTION,
            "HardwareAcceleration",
            &self.hardware_acceleration.to_string(),
        );
        if !self.skin_path.as_os_str().is_empty() {
            ini.set(
                RAINMETER_SECTION,
                "SkinPath",
                &self.skin_path.to_string_lossy(),
            );
        }
        if let Some(language) = self.language {
            ini.set(RAINMETER_SECTION, "Language", &language.to_string());
        }
        ini.set(RAINMETER_SECTION, "TrayIcon", bool_str(self.tray_icon));
        ini.set(
            RAINMETER_SECTION,
            "NormalStayDesktop",
            bool_str(self.normal_stay_desktop),
        );
    }
}

pub fn parse_bool(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn bool_str(value: bool) -> &'static str {
    if value {
        "1"
    } else {
        "0"
    }
}

pub fn merge_maps<K: Ord + Clone, V: Clone>(
    base: &BTreeMap<K, V>,
    overlay: &BTreeMap<K, V>,
) -> BTreeMap<K, V> {
    let mut merged = base.clone();
    for (key, value) in overlay {
        merged.insert(key.clone(), value.clone());
    }
    merged
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkinState {
    pub window_x: i32,
    pub window_y: i32,
    pub anchor_x: f32,
    pub anchor_y: f32,
    pub always_on_top: i32,
    pub on_hover: i32,
    pub draggable: bool,
    pub snap_edges: bool,
    pub click_through: bool,
    pub keep_on_screen: bool,
    pub save_position: bool,
    pub start_hidden: bool,
    pub auto_select_screen: bool,
    pub alpha_value: u8,
    pub fade_duration: u32,
    pub group: Vec<String>,
    pub drag_group: Vec<String>,
    pub active: i32,
    pub load_order: Option<i32>,
    pub defaults: BTreeMap<String, String>,
}

impl Default for SkinState {
    fn default() -> Self {
        SkinState {
            window_x: 0,
            window_y: 0,
            anchor_x: 1.0,
            anchor_y: 1.0,
            always_on_top: -1,
            on_hover: 0,
            draggable: false,
            snap_edges: false,
            click_through: false,
            keep_on_screen: false,
            save_position: false,
            start_hidden: false,
            auto_select_screen: false,
            alpha_value: 255,
            fade_duration: 250,
            group: Vec::new(),
            drag_group: Vec::new(),
            active: 0,
            load_order: None,
            defaults: BTreeMap::new(),
        }
    }
}

impl SkinState {
    pub fn from_section(section: &Section) -> SkinState {
        use crate::readers::{read_bool, read_f32, read_i32, read_u32};
        let mut state = SkinState::default();

        if let Some(value) = section.get("WindowX") {
            state.window_x = read_i32(value, state.window_x);
        }
        if let Some(value) = section.get("WindowY") {
            state.window_y = read_i32(value, state.window_y);
        }
        if let Some(value) = section.get("AnchorX") {
            state.anchor_x = read_f32(value, state.anchor_x);
        }
        if let Some(value) = section.get("AnchorY") {
            state.anchor_y = read_f32(value, state.anchor_y);
        }
        if let Some(value) = section.get("AlwaysOnTop") {
            state.always_on_top = read_i32(value, state.always_on_top);
        }
        if let Some(value) = section.get("OnHover") {
            state.on_hover = read_i32(value, state.on_hover);
        }
        if let Some(value) = section.get("Draggable") {
            state.draggable = read_bool(value, false);
        }
        if let Some(value) = section.get("SnapEdges") {
            state.snap_edges = read_bool(value, false);
        }
        if let Some(value) = section.get("ClickThrough") {
            state.click_through = read_bool(value, false);
        }
        if let Some(value) = section.get("KeepOnScreen") {
            state.keep_on_screen = read_bool(value, false);
        }
        if let Some(value) = section.get("SavePosition") {
            state.save_position = read_bool(value, false);
        }
        if let Some(value) = section.get("StartHidden") {
            state.start_hidden = read_bool(value, false);
        }
        if let Some(value) = section.get("AutoSelectScreen") {
            state.auto_select_screen = read_bool(value, false);
        }
        if let Some(value) = section.get("AlphaValue") {
            state.alpha_value = read_u32(value, state.alpha_value as u32).min(255) as u8;
        }
        if let Some(value) = section.get("FadeDuration") {
            state.fade_duration = read_u32(value, state.fade_duration);
        }
        if let Some(value) = section.get("Group") {
            state.group = split_pipe(value);
        }
        if let Some(value) = section.get("DragGroup") {
            state.drag_group = split_pipe(value);
        }
        if let Some(value) = section.get("Active") {
            state.active = read_i32(value, state.active);
        }
        if let Some(value) = section.get("LoadOrder") {
            state.load_order = value.trim().parse().ok();
        }
        for (key, value) in section.iter() {
            if key.len() > 7 && key[..7].eq_ignore_ascii_case("Default") {
                state
                    .defaults
                    .insert(key[7..].to_string(), value.to_string());
            }
        }

        state
    }

    pub fn apply_to_section(&self, section: &mut Section) {
        section.set("WindowX", &self.window_x.to_string());
        section.set("WindowY", &self.window_y.to_string());
        section.set("AnchorX", &self.anchor_x.to_string());
        section.set("AnchorY", &self.anchor_y.to_string());
        section.set("AlwaysOnTop", &self.always_on_top.to_string());
        section.set("OnHover", &self.on_hover.to_string());
        section.set("Draggable", bool_str(self.draggable));
        section.set("SnapEdges", bool_str(self.snap_edges));
        section.set("ClickThrough", bool_str(self.click_through));
        section.set("KeepOnScreen", bool_str(self.keep_on_screen));
        section.set("SavePosition", bool_str(self.save_position));
        section.set("StartHidden", bool_str(self.start_hidden));
        section.set("AutoSelectScreen", bool_str(self.auto_select_screen));
        section.set("AlphaValue", &self.alpha_value.to_string());
        section.set("FadeDuration", &self.fade_duration.to_string());
        if !self.group.is_empty() {
            section.set("Group", &self.group.join("|"));
        }
        if !self.drag_group.is_empty() {
            section.set("DragGroup", &self.drag_group.join("|"));
        }
        section.set("Active", &self.active.to_string());
        if let Some(load_order) = self.load_order {
            section.set("LoadOrder", &load_order.to_string());
        }
        for (key, value) in &self.defaults {
            section.set(&format!("Default{key}"), value);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ApplicationConfig {
    pub ini: IniFile,
}

impl ApplicationConfig {
    pub fn new() -> Self {
        ApplicationConfig {
            ini: IniFile::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        Ok(ApplicationConfig {
            ini: IniFile::load(path)?,
        })
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.ini.save(path)
    }

    pub fn global(&self) -> GlobalSettings {
        GlobalSettings::from_ini(&self.ini)
    }

    pub fn set_global(&mut self, settings: &GlobalSettings) {
        settings.apply_to_ini(&mut self.ini);
    }

    pub fn skin_state(&self, section: &str) -> SkinState {
        match self.ini.section(section) {
            Some(section) => SkinState::from_section(section),
            None => SkinState::default(),
        }
    }

    pub fn set_skin_state(&mut self, section: &str, state: &SkinState) {
        let target = self.ini.section_mut(section);
        target.name = section.to_string();
        state.apply_to_section(target);
    }

    pub fn skin_sections(&self) -> impl Iterator<Item = &str> {
        self.ini.section_names()
    }
}

fn split_pipe(value: &str) -> Vec<String> {
    value
        .split('|')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
; commentaire
Note=Valeur
[Petrichor]
Debug=1
NetInSpeed=500
[Chemin\\Skin]
WindowX=100
WindowY=200
Active=1
";

    #[test]
    fn parses_sections_and_keys() {
        let ini = IniFile::parse(SAMPLE);
        assert_eq!(ini.get("Petrichor", "Debug"), Some("1"));
        assert_eq!(ini.get("Chemin\\Skin", "WindowX"), Some("100"));
        assert_eq!(ini.get("Chemin\\Skin", "Active"), Some("1"));
    }

    #[test]
    fn key_lookup_is_case_insensitive() {
        let ini = IniFile::parse(SAMPLE);
        assert_eq!(ini.get("petrichor", "debug"), Some("1"));
    }

    #[test]
    fn roundtrip_preserves_values() {
        let ini = IniFile::parse(SAMPLE);
        let text = ini.render();
        let reparsed = IniFile::parse(&text);
        assert_eq!(reparsed.get("Petrichor", "NetInSpeed"), Some("500"));
        assert_eq!(reparsed.get("Chemin\\Skin", "WindowY"), Some("200"));
    }

    #[test]
    fn set_overwrites_existing() {
        let mut ini = IniFile::parse(SAMPLE);
        ini.set("Petrichor", "Debug", "0");
        assert_eq!(ini.get("Petrichor", "Debug"), Some("0"));
    }

    #[test]
    fn global_settings_roundtrip() {
        let ini = IniFile::parse(SAMPLE);
        let settings = GlobalSettings::from_ini(&ini);
        assert!(settings.debug);
        assert_eq!(settings.net_in_speed, 500);

        let mut out = IniFile::new();
        settings.apply_to_ini(&mut out);
        assert_eq!(out.get(RAINMETER_SECTION, "Debug"), Some("1"));
    }

    #[test]
    fn skin_state_roundtrip() {
        let ini = IniFile::parse(
            "[Clock\\Clock.ini]\nWindowX=100\nWindowY=250\nAlwaysOnTop=1\nDraggable=1\nAlphaValue=200\nGroup=Top|Clock\n",
        );
        let state = SkinState::from_section(ini.section("Clock\\Clock.ini").unwrap());
        assert_eq!(state.window_x, 100);
        assert_eq!(state.always_on_top, 1);
        assert!(state.draggable);
        assert_eq!(state.alpha_value, 200);
        assert_eq!(state.group, vec!["Top", "Clock"]);

        let mut out = Section::new("Clock\\Clock.ini");
        state.apply_to_section(&mut out);
        assert_eq!(out.get("WindowX"), Some("100"));
        assert_eq!(out.get("Group"), Some("Top|Clock"));
    }

    #[test]
    fn application_config_persists_skin_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Petrichor.ini");

        let mut config = ApplicationConfig::new();
        let global = GlobalSettings {
            debug: true,
            ..GlobalSettings::default()
        };
        config.set_global(&global);
        config.set_skin_state(
            "Clock\\Clock.ini",
            &SkinState {
                window_x: 42,
                ..SkinState::default()
            },
        );
        config.save(&path).unwrap();

        let reloaded = ApplicationConfig::load(&path).unwrap();
        assert!(reloaded.global().debug);
        assert_eq!(reloaded.skin_state("Clock\\Clock.ini").window_x, 42);
        assert!(reloaded
            .skin_sections()
            .any(|s| s.eq_ignore_ascii_case("Clock\\Clock.ini")));
    }
}
