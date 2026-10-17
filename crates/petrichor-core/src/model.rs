use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::diagnostics::Diagnostics;
use crate::include;
use crate::ini::IniDocument;
use crate::readers::{read_bool, read_color, read_f32, read_u32, Color};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum BackgroundMode {
    #[default]
    Image,
    Copy,
    Solid,
    ScaledImage,
    TiledImage,
}

impl BackgroundMode {
    pub fn parse(value: &str) -> Option<BackgroundMode> {
        match value.trim().to_ascii_uppercase().as_str() {
            "IMAGE" => Some(BackgroundMode::Image),
            "COPY" => Some(BackgroundMode::Copy),
            "SOLID" => Some(BackgroundMode::Solid),
            "SCALED_IMAGE" => Some(BackgroundMode::ScaledImage),
            "TILED_IMAGE" => Some(BackgroundMode::TiledImage),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum BevelType {
    #[default]
    None,
    Up,
    Down,
}

impl BevelType {
    pub fn parse(value: &str) -> Option<BevelType> {
        match value.trim().to_ascii_uppercase().as_str() {
            "NONE" => Some(BevelType::None),
            "UP" => Some(BevelType::Up),
            "DOWN" => Some(BevelType::Down),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SkinSettings {
    pub update_ms: u32,
    pub transition_update_ms: u32,
    pub default_update_divider: u32,
    pub skin_width: Option<u32>,
    pub skin_height: Option<u32>,
    pub dynamic_window_size: bool,
    pub accurate_text: bool,
    pub tooltip_hidden: bool,
    pub background: Option<String>,
    pub background_mode: BackgroundMode,
    pub solid_color: Option<Color>,
    pub solid_color2: Option<Color>,
    pub gradient_angle: f32,
    pub bevel_type: BevelType,
    pub group: Vec<String>,
    pub drag_group: Vec<String>,
    pub blur: bool,
    pub local_fonts: Vec<String>,
    pub on_refresh_action: Option<String>,
    pub on_close_action: Option<String>,
    pub on_focus_action: Option<String>,
    pub on_unfocus_action: Option<String>,
    pub on_update_action: Option<String>,
    pub on_wake_action: Option<String>,
    pub mouse_action_cursor: bool,
    pub mouse_action_cursor_name: Option<String>,
    pub defaults: BTreeMap<String, String>,
}

impl Default for SkinSettings {
    fn default() -> Self {
        SkinSettings {
            update_ms: 1000,
            transition_update_ms: 100,
            default_update_divider: 1,
            skin_width: None,
            skin_height: None,
            dynamic_window_size: false,
            accurate_text: false,
            tooltip_hidden: false,
            background: None,
            background_mode: BackgroundMode::Image,
            solid_color: None,
            solid_color2: None,
            gradient_angle: 0.0,
            bevel_type: BevelType::None,
            group: Vec::new(),
            drag_group: Vec::new(),
            blur: false,
            local_fonts: Vec::new(),
            on_refresh_action: None,
            on_close_action: None,
            on_focus_action: None,
            on_unfocus_action: None,
            on_update_action: None,
            on_wake_action: None,
            mouse_action_cursor: true,
            mouse_action_cursor_name: None,
            defaults: BTreeMap::new(),
        }
    }
}

impl SkinSettings {
    pub fn from_section(section: &crate::ini::IniSection) -> SkinSettings {
        let mut settings = SkinSettings::default();

        if let Some(value) = section.get("Update") {
            settings.update_ms = read_u32(value, settings.update_ms).max(1);
        }
        if let Some(value) = section.get("TransitionUpdate") {
            settings.transition_update_ms = read_u32(value, settings.transition_update_ms);
        }
        if let Some(value) = section.get("DefaultUpdateDivider") {
            settings.default_update_divider =
                read_u32(value, settings.default_update_divider).max(1);
        }
        if let Some(value) = section.get("SkinWidth") {
            settings.skin_width = value.trim().parse().ok();
        }
        if let Some(value) = section.get("SkinHeight") {
            settings.skin_height = value.trim().parse().ok();
        }
        if let Some(value) = section.get("DynamicWindowSize") {
            settings.dynamic_window_size = read_bool(value, false);
        }
        if let Some(value) = section.get("AccurateText") {
            settings.accurate_text = read_bool(value, false);
        }
        if let Some(value) = section.get("ToolTipHidden") {
            settings.tooltip_hidden = read_bool(value, false);
        }
        if let Some(value) = section.get("Background") {
            settings.background = Some(value.to_string());
        }
        if let Some(value) = section.get("BackgroundMode") {
            if let Some(mode) = BackgroundMode::parse(value) {
                settings.background_mode = mode;
            }
        }
        if let Some(value) = section.get("SolidColor") {
            settings.solid_color = Some(read_color(value, Color::TRANSPARENT));
        }
        if let Some(value) = section.get("SolidColor2") {
            settings.solid_color2 = Some(read_color(value, Color::TRANSPARENT));
        }
        if let Some(value) = section.get("GradientAngle") {
            settings.gradient_angle = read_f32(value, 0.0);
        }
        if let Some(value) = section.get("BevelType") {
            if let Some(bevel) = BevelType::parse(value) {
                settings.bevel_type = bevel;
            }
        }
        if let Some(value) = section.get("Group") {
            settings.group = split_list(value);
        }
        if let Some(value) = section.get("DragGroup") {
            settings.drag_group = split_list(value);
        }
        if let Some(value) = section.get("Blur") {
            settings.blur = read_bool(value, false);
        }
        settings.local_fonts = collect_numbered(section, "LocalFont");
        settings.on_refresh_action = section.get("OnRefreshAction").map(str::to_string);
        settings.on_close_action = section.get("OnCloseAction").map(str::to_string);
        settings.on_focus_action = section.get("OnFocusAction").map(str::to_string);
        settings.on_unfocus_action = section.get("OnUnfocusAction").map(str::to_string);
        settings.on_update_action = section.get("OnUpdateAction").map(str::to_string);
        settings.on_wake_action = section.get("OnWakeAction").map(str::to_string);
        if let Some(value) = section.get("MouseActionCursor") {
            settings.mouse_action_cursor = read_bool(value, true);
        }
        settings.mouse_action_cursor_name =
            section.get("MouseActionCursorName").map(str::to_string);

        for entry in &section.entries {
            if entry.key.len() > 7 && entry.key[..7].eq_ignore_ascii_case("Default") {
                settings
                    .defaults
                    .insert(entry.key[7..].to_string(), entry.value.clone());
            }
        }

        settings
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Metadata {
    pub name: Option<String>,
    pub author: Option<String>,
    pub information: Option<String>,
    pub version: Option<String>,
    pub license: Option<String>,
}

impl Metadata {
    pub fn from_section(section: &crate::ini::IniSection) -> Metadata {
        Metadata {
            name: section.get("Name").map(str::to_string),
            author: section.get("Author").map(str::to_string),
            information: section.get("Information").map(str::to_string),
            version: section.get("Version").map(str::to_string),
            license: section.get("License").map(str::to_string),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OptionEntry {
    pub key: String,
    pub value: String,
    pub line: u32,
}

impl OptionEntry {
    fn from_entries(section: &crate::ini::IniSection) -> Vec<OptionEntry> {
        section
            .entries
            .iter()
            .map(|e| OptionEntry {
                key: e.key.clone(),
                value: e.value.clone(),
                line: e.line,
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Options {
    pub entries: Vec<OptionEntry>,
}

impl Options {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.key.eq_ignore_ascii_case(key))
            .map(|e| e.value.as_str())
    }

    pub fn contains(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Measure {
    pub name: String,
    pub kind: String,
    pub line: u32,
    pub options: Options,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Meter {
    pub name: String,
    pub kind: String,
    pub line: u32,
    pub options: Options,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StyleSection {
    pub name: String,
    pub options: Options,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Skin {
    pub name: String,
    pub folder: String,
    pub ini_path: PathBuf,
    pub root: PathBuf,
    pub variables: BTreeMap<String, String>,
    pub metadata: Metadata,
    pub settings: SkinSettings,
    pub measures: Vec<Measure>,
    pub meters: Vec<Meter>,
    pub styles: Vec<StyleSection>,
    pub included_files: Vec<PathBuf>,
}

impl Skin {
    pub fn variable(&self, name: &str) -> Option<&str> {
        self.variables
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn measure(&self, name: &str) -> Option<&Measure> {
        self.measures
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(name))
    }

    pub fn meter(&self, name: &str) -> Option<&Meter> {
        self.meters
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(name))
    }
}

pub fn build_skin(ini_path: &Path) -> (Skin, Diagnostics) {
    let (skin, _document, diagnostics) = build_skin_with_document(ini_path);
    (skin, diagnostics)
}

pub fn build_skin_with_document(ini_path: &Path) -> (Skin, IniDocument, Diagnostics) {
    let root = ini_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let resolved = include::resolve(ini_path, &root);
    let document = resolved.document;

    let folder = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let name = ini_path
        .file_stem()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let variables = document
        .section("Variables")
        .map(|s| {
            s.entries
                .iter()
                .map(|e| (e.key.clone(), e.value.clone()))
                .collect()
        })
        .unwrap_or_default();

    let metadata = document
        .section("Metadata")
        .map(Metadata::from_section)
        .unwrap_or_default();

    let settings = document
        .section("Petrichor")
        .map(SkinSettings::from_section)
        .unwrap_or_default();

    let mut measures = Vec::new();
    let mut meters = Vec::new();
    let mut styles = Vec::new();

    for section in &document.sections {
        if section.name.is_empty() {
            continue;
        }
        let lower = section.name.to_ascii_lowercase();
        if lower == "variables" || lower == "metadata" || lower == "petrichor" {
            continue;
        }

        if let Some(kind) = section.get("Measure") {
            measures.push(Measure {
                name: section.name.clone(),
                kind: kind.trim().to_string(),
                line: section.line,
                options: Options {
                    entries: OptionEntry::from_entries(section),
                },
            });
        } else if let Some(kind) = section.get("Meter") {
            meters.push(Meter {
                name: section.name.clone(),
                kind: kind.trim().to_string(),
                line: section.line,
                options: Options {
                    entries: OptionEntry::from_entries(section),
                },
            });
        } else {
            styles.push(StyleSection {
                name: section.name.clone(),
                options: Options {
                    entries: OptionEntry::from_entries(section),
                },
            });
        }
    }

    let skin = Skin {
        name,
        folder,
        ini_path: ini_path.to_path_buf(),
        root,
        variables,
        metadata,
        settings,
        measures,
        meters,
        styles,
        included_files: resolved.files,
    };

    let mut diagnostics = resolved.diagnostics;
    crate::validate::validate_document(&document, ini_path, &mut diagnostics);

    (skin, document, diagnostics)
}

fn split_list(value: &str) -> Vec<String> {
    value
        .split('|')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn collect_numbered(section: &crate::ini::IniSection, base: &str) -> Vec<String> {
    let mut result = Vec::new();
    for n in 0..=9 {
        let key = if n == 0 {
            base.to_string()
        } else {
            format!("{base}{n}")
        };
        if let Some(value) = section.get(&key) {
            result.push(value.to_string());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const SAMPLE: &str = "\
[Metadata]
Name=Horloge
Author=Alice

[Variables]
Primary=FF8800

[Petrichor]
Update=500
DynamicWindowSize=1
Group=Top|Clock
SolidColor=00000080

[MeasureTime]
Measure=Time
Format=%H:%M

[StyleTitle]
FontSize=24
FontColor=#Primary#

[MeterLabel]
Meter=String
MeasureName=MeasureTime
MeterStyle=StyleTitle
Text=%1
";

    #[test]
    fn classifies_sections() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Clock.ini");
        fs::write(&path, SAMPLE).unwrap();

        let (skin, diagnostics) = build_skin(&path);
        assert!(diagnostics.is_empty());
        assert_eq!(skin.measures.len(), 1);
        assert_eq!(skin.meters.len(), 1);
        assert_eq!(skin.styles.len(), 1);
        assert_eq!(skin.measures[0].name, "MeasureTime");
        assert_eq!(skin.measures[0].kind, "Time");
        assert_eq!(skin.meters[0].kind, "String");
    }

    #[test]
    fn parses_metadata_and_variables() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Clock.ini");
        fs::write(&path, SAMPLE).unwrap();
        let (skin, _) = build_skin(&path);

        assert_eq!(skin.metadata.name.as_deref(), Some("Horloge"));
        assert_eq!(skin.metadata.author.as_deref(), Some("Alice"));
        assert_eq!(skin.variable("Primary"), Some("FF8800"));
    }

    #[test]
    fn parses_skin_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Clock.ini");
        fs::write(&path, SAMPLE).unwrap();
        let (skin, _) = build_skin(&path);

        assert_eq!(skin.settings.update_ms, 500);
        assert!(skin.settings.dynamic_window_size);
        assert_eq!(skin.settings.group, vec!["Top", "Clock"]);
        assert_eq!(skin.settings.solid_color, Some(Color::new(0, 0, 0, 0x80)));
    }

    #[test]
    fn looks_up_measures_and_meters_case_insensitively() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Clock.ini");
        fs::write(&path, SAMPLE).unwrap();
        let (skin, _) = build_skin(&path);

        assert!(skin.measure("measuretime").is_some());
        assert!(skin.meter("meterlabel").is_some());
        assert_eq!(skin.name, "Clock");
    }

    #[test]
    fn resolves_includes_in_model() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("main.ini"),
            "[MeterA]\nMeter=String\n@Include=extra.ini\n",
        )
        .unwrap();
        fs::write(dir.path().join("extra.ini"), "[MeasureB]\nMeasure=CPU\n").unwrap();
        let (skin, diagnostics) = build_skin(&dir.path().join("main.ini"));
        assert!(diagnostics.is_empty());
        assert_eq!(skin.measures.len(), 1);
        assert_eq!(skin.meters.len(), 1);
        assert_eq!(skin.included_files.len(), 2);
    }
}
