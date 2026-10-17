use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::diagnostics::Diagnostics;
use crate::ini::IniDocument;
use crate::model::{build_skin_with_document, BackgroundMode, Meter, Skin};
use crate::readers::{
    read_bool, read_color, read_f32, read_i32, read_u32, try_read_rect_ltrb, Color,
};
use crate::style::resolved_options;
use crate::variables::Variables;

const FONT_EXTENSIONS: [&str; 4] = ["ttf", "otf", "ttc", "woff"];
const IMAGE_EXTENSIONS: [&str; 6] = ["png", "jpg", "jpeg", "gif", "bmp", "webp"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Relative {
    Absolute,
    Position,
    Edge,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Dimension {
    pub value: f64,
    pub relative: Relative,
}

impl Dimension {
    pub fn parse(value: &str) -> Dimension {
        let trimmed = value.trim();
        let (body, relative) = if let Some(rest) = trimmed.strip_suffix('R') {
            (rest, Relative::Edge)
        } else if let Some(rest) = trimmed.strip_suffix('r') {
            (rest, Relative::Position)
        } else {
            (trimmed, Relative::Absolute)
        };
        Dimension {
            value: body.trim().parse::<f64>().unwrap_or(0.0),
            relative,
        }
    }
}

impl Default for Dimension {
    fn default() -> Self {
        Dimension {
            value: 0.0,
            relative: Relative::Absolute,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CommonMeter {
    pub name: String,
    pub kind: String,
    pub x: Dimension,
    pub y: Dimension,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub padding: [i32; 4],
    pub hidden: bool,
    pub solid_color: Option<Color>,
    pub solid_color2: Option<Color>,
    pub gradient_angle: f32,
    pub group: Vec<String>,
    pub measure_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StringMeter {
    pub text: Option<String>,
    pub font_face: String,
    pub font_size: f32,
    pub font_color: Color,
    pub font_weight: i32,
    pub string_align: String,
    pub string_case: String,
    pub string_style: String,
    pub string_effect: String,
    pub prefix: String,
    pub postfix: String,
    pub num_of_decimals: i32,
    pub angle: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImageMeter {
    pub image: Option<PathBuf>,
    pub preserve_aspect_ratio: u8,
    pub tile: bool,
    pub scale_margins: [f32; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BarMeter {
    pub color: Color,
    pub orientation: String,
    pub flip: bool,
    pub border: u32,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum RenderMeter {
    String {
        #[serde(flatten)]
        common: CommonMeter,
        #[serde(flatten)]
        props: StringMeter,
    },
    Image {
        #[serde(flatten)]
        common: CommonMeter,
        #[serde(flatten)]
        props: ImageMeter,
    },
    Bar {
        #[serde(flatten)]
        common: CommonMeter,
        #[serde(flatten)]
        props: BarMeter,
    },
    Unsupported {
        #[serde(flatten)]
        common: CommonMeter,
        kind: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Background {
    pub mode: BackgroundMode,
    pub image: Option<PathBuf>,
    pub color: Color,
    pub color2: Option<Color>,
    pub gradient_angle: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderModel {
    pub name: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub update_ms: u32,
    pub dynamic_window_size: bool,
    pub background: Background,
    pub meters: Vec<RenderMeter>,
    pub local_fonts: Vec<PathBuf>,
    pub measure_values: BTreeMap<String, String>,
}

pub fn build_render_model(ini_path: &Path) -> (RenderModel, Diagnostics) {
    let (skin, document, diagnostics) = build_skin_with_document(ini_path);

    let mut variables = Variables::new();
    for (name, value) in &skin.variables {
        variables.set(name, value);
    }

    let meters = skin
        .meters
        .iter()
        .map(|meter| render_meter(&document, &skin, meter, &variables))
        .collect();

    let model = RenderModel {
        name: skin.name.clone(),
        width: skin.settings.skin_width,
        height: skin.settings.skin_height,
        update_ms: skin.settings.update_ms,
        dynamic_window_size: skin.settings.dynamic_window_size,
        background: render_background(&skin),
        meters,
        local_fonts: collect_local_fonts(&skin),
        measure_values: crate::measure::evaluate(&skin),
    };

    (model, diagnostics)
}

fn render_meter(
    document: &IniDocument,
    skin: &Skin,
    meter: &Meter,
    variables: &Variables,
) -> RenderMeter {
    let options = resolved_options(document, &meter.name);
    let common = build_common(meter, &options, variables);

    match meter.kind.trim().to_ascii_uppercase().as_str() {
        "STRING" => RenderMeter::String {
            common,
            props: build_string(&options, variables),
        },
        "IMAGE" => RenderMeter::Image {
            common,
            props: build_image(skin, &options, variables),
        },
        "BAR" => RenderMeter::Bar {
            common,
            props: build_bar(skin, &options, variables),
        },
        other => RenderMeter::Unsupported {
            common,
            kind: other.to_string(),
        },
    }
}

fn opt(options: &[(String, String)], variables: &Variables, key: &str) -> Option<String> {
    options
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(key))
        .map(|(_, value)| variables.expand(value))
}

fn build_common(meter: &Meter, options: &[(String, String)], variables: &Variables) -> CommonMeter {
    let padding = opt(options, variables, "Padding")
        .and_then(|value| try_read_rect_ltrb(&value).ok())
        .map(|rect| {
            [
                rect.left.round() as i32,
                rect.top.round() as i32,
                rect.right.round() as i32,
                rect.bottom.round() as i32,
            ]
        })
        .unwrap_or([0; 4]);

    CommonMeter {
        name: meter.name.clone(),
        kind: meter.kind.trim().to_string(),
        x: opt(options, variables, "X")
            .map(|value| Dimension::parse(&value))
            .unwrap_or_default(),
        y: opt(options, variables, "Y")
            .map(|value| Dimension::parse(&value))
            .unwrap_or_default(),
        width: opt(options, variables, "W").and_then(|value| value.trim().parse().ok()),
        height: opt(options, variables, "H").and_then(|value| value.trim().parse().ok()),
        padding,
        hidden: opt(options, variables, "Hidden")
            .map(|value| read_bool(&value, false))
            .unwrap_or(false),
        solid_color: opt(options, variables, "SolidColor")
            .map(|value| read_color(&value, Color::TRANSPARENT)),
        solid_color2: opt(options, variables, "SolidColor2")
            .map(|value| read_color(&value, Color::TRANSPARENT)),
        gradient_angle: opt(options, variables, "GradientAngle")
            .map(|value| read_f32(&value, 0.0))
            .unwrap_or(0.0),
        group: opt(options, variables, "Group")
            .map(|value| split_list(&value))
            .unwrap_or_default(),
        measure_names: collect_numbered(options, variables, "MeasureName"),
    }
}

fn build_string(options: &[(String, String)], variables: &Variables) -> StringMeter {
    StringMeter {
        text: opt(options, variables, "Text"),
        font_face: opt(options, variables, "FontFace").unwrap_or_else(|| "sans-serif".to_string()),
        font_size: opt(options, variables, "FontSize")
            .map(|value| read_f32(&value, 10.0))
            .unwrap_or(10.0),
        font_color: opt(options, variables, "FontColor")
            .map(|value| read_color(&value, Color::BLACK))
            .unwrap_or(Color::BLACK),
        font_weight: opt(options, variables, "FontWeight")
            .map(|value| read_i32(&value, -1))
            .unwrap_or(-1),
        string_align: opt(options, variables, "StringAlign")
            .unwrap_or_else(|| "LEFT".to_string())
            .trim()
            .to_ascii_uppercase(),
        string_case: opt(options, variables, "StringCase")
            .unwrap_or_else(|| "NONE".to_string())
            .trim()
            .to_ascii_uppercase(),
        string_style: opt(options, variables, "StringStyle")
            .unwrap_or_else(|| "NORMAL".to_string())
            .trim()
            .to_ascii_uppercase(),
        string_effect: opt(options, variables, "StringEffect")
            .unwrap_or_else(|| "NONE".to_string())
            .trim()
            .to_ascii_uppercase(),
        prefix: opt(options, variables, "Prefix").unwrap_or_default(),
        postfix: opt(options, variables, "Postfix").unwrap_or_default(),
        num_of_decimals: opt(options, variables, "NumOfDecimals")
            .map(|value| read_i32(&value, -1))
            .unwrap_or(-1),
        angle: opt(options, variables, "Angle")
            .map(|value| read_f32(&value, 0.0))
            .unwrap_or(0.0),
    }
}

fn build_image(skin: &Skin, options: &[(String, String)], variables: &Variables) -> ImageMeter {
    let image = opt(options, variables, "ImageName")
        .filter(|value| !value.trim().is_empty())
        .and_then(|value| resolve_asset(skin, value.trim(), &IMAGE_EXTENSIONS));

    let scale_margins = opt(options, variables, "ScaleMargins")
        .map(|value| {
            let mut margins = [0.0_f32; 4];
            for (index, part) in value.split(',').take(4).enumerate() {
                margins[index] = part.trim().parse().unwrap_or(0.0);
            }
            margins
        })
        .unwrap_or([0.0; 4]);

    ImageMeter {
        image,
        preserve_aspect_ratio: opt(options, variables, "PreserveAspectRatio")
            .map(|value| read_u32(&value, 0) as u8)
            .unwrap_or(0),
        tile: opt(options, variables, "Tile")
            .map(|value| read_bool(&value, false))
            .unwrap_or(false),
        scale_margins,
    }
}

fn build_bar(skin: &Skin, options: &[(String, String)], variables: &Variables) -> BarMeter {
    let orientation = opt(options, variables, "BarOrientation")
        .unwrap_or_else(|| "HORIZONTAL".to_string())
        .trim()
        .to_ascii_uppercase();

    let measure_names = collect_numbered(options, variables, "MeasureName");
    let value = measure_names
        .first()
        .and_then(|name| skin.measure(name))
        .and_then(|measure| measure.options.get("MaxValue"))
        .map(|value| crate::readers::read_f64(&variables.expand(value), 100.0))
        .filter(|max| *max > 0.0)
        .map(|max| {
            let resolved = measure_names
                .first()
                .and_then(|name| crate::measure::evaluate(skin).get(name).cloned())
                .unwrap_or_default();
            let number = crate::readers::read_f64(&resolved, 0.0);
            (number / max).clamp(0.0, 1.0)
        })
        .unwrap_or(0.0);

    BarMeter {
        color: opt(options, variables, "BarColor")
            .map(|value| read_color(&value, Color::new(0, 200, 100, 255)))
            .unwrap_or(Color::new(0, 200, 100, 255)),
        orientation,
        flip: opt(options, variables, "Flip")
            .map(|value| read_bool(&value, false))
            .unwrap_or(false),
        border: opt(options, variables, "BarBorder")
            .map(|value| read_u32(&value, 0))
            .unwrap_or(0),
        value,
    }
}

fn render_background(skin: &Skin) -> Background {
    let settings = &skin.settings;
    let image = settings
        .background
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .and_then(|value| resolve_asset(skin, value.trim(), &IMAGE_EXTENSIONS));

    Background {
        mode: settings.background_mode,
        image,
        color: settings.solid_color.unwrap_or(Color::TRANSPARENT),
        color2: settings.solid_color2,
        gradient_angle: settings.gradient_angle,
    }
}

fn collect_local_fonts(skin: &Skin) -> Vec<PathBuf> {
    let mut fonts = Vec::new();

    for declared in &skin.settings.local_fonts {
        if let Some(path) = resolve_asset(skin, declared.trim(), &FONT_EXTENSIONS) {
            fonts.push(path);
        }
    }

    let fonts_dir = skin.root.join("@Resources").join("Fonts");
    if let Ok(entries) = std::fs::read_dir(&fonts_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if FONT_EXTENSIONS.iter().any(|f| f.eq_ignore_ascii_case(ext)) {
                        fonts.push(path);
                    }
                }
            }
        }
    }

    fonts.sort();
    fonts.dedup();
    fonts
}

fn resolve_asset(skin: &Skin, value: &str, extensions: &[&str]) -> Option<PathBuf> {
    let candidate = if let Some(relative) = value.strip_prefix("@Resources/") {
        skin.root.join("@Resources").join(relative)
    } else {
        let path = PathBuf::from(value);
        if path.is_absolute() {
            path
        } else {
            skin.root.join(path)
        }
    };

    if candidate.is_file() {
        return Some(candidate);
    }

    if candidate.extension().is_none() {
        for extension in extensions {
            let with_extension = candidate.with_extension(extension);
            if with_extension.is_file() {
                return Some(with_extension);
            }
        }
    }

    None
}

fn split_list(value: &str) -> Vec<String> {
    value
        .split('|')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn collect_numbered(
    options: &[(String, String)],
    variables: &Variables,
    base: &str,
) -> Vec<String> {
    let mut values = Vec::new();
    for n in 0..=9 {
        let key = if n == 0 {
            base.to_string()
        } else {
            format!("{base}{n}")
        };
        if let Some(value) = opt(options, variables, &key) {
            if !value.trim().is_empty() {
                values.push(value.trim().to_string());
            }
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_skin(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("Demo.ini");
        fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn parses_relative_dimensions() {
        assert_eq!(
            Dimension::parse("5R"),
            Dimension {
                value: 5.0,
                relative: Relative::Edge
            }
        );
        assert_eq!(
            Dimension::parse("12r"),
            Dimension {
                value: 12.0,
                relative: Relative::Position
            }
        );
        assert_eq!(
            Dimension::parse("-3"),
            Dimension {
                value: -3.0,
                relative: Relative::Absolute
            }
        );
        assert_eq!(Dimension::parse("oups").value, 0.0);
    }

    #[test]
    fn expands_variables_in_options() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_skin(
            dir.path(),
            "[Variables]\nAccent=FF8800\n[MeterTitle]\nMeter=String\nText=#Accent#\n",
        );
        let (model, diagnostics) = build_render_model(&path);
        assert!(diagnostics.is_empty());

        match &model.meters[0] {
            RenderMeter::String { props, .. } => {
                assert_eq!(props.text.as_deref(), Some("FF8800"));
            }
            other => panic!("meter inattendu : {other:?}"),
        }
    }

    #[test]
    fn merges_meter_styles() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_skin(
            dir.path(),
            "\
[StyleBase]
FontSize=24
FontColor=FFFFFFFF
[MeterTitle]
Meter=String
MeterStyle=StyleBase
Text=Salut
",
        );
        let (model, _) = build_render_model(&path);
        match &model.meters[0] {
            RenderMeter::String { common, props } => {
                assert_eq!(props.font_size, 24.0);
                assert_eq!(common.name, "MeterTitle");
            }
            other => panic!("meter inattendu : {other:?}"),
        }
    }

    #[test]
    fn classifies_meters() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_skin(
            dir.path(),
            "\
[MeterText]
Meter=String
Text=hi
[MeterBar]
Meter=Bar
[MeterPic]
Meter=Image
[MeterWeird]
Meter=Histogram
",
        );
        let (model, _) = build_render_model(&path);
        assert!(matches!(model.meters[0], RenderMeter::String { .. }));
        assert!(matches!(model.meters[1], RenderMeter::Bar { .. }));
        assert!(matches!(model.meters[2], RenderMeter::Image { .. }));
        assert!(matches!(model.meters[3], RenderMeter::Unsupported { .. }));
    }

    #[test]
    fn resolves_image_from_resources() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("@Resources/Images")).unwrap();
        fs::write(dir.path().join("@Resources/Images/logo.png"), b"x").unwrap();
        let path = write_skin(
            dir.path(),
            "[MeterLogo]\nMeter=Image\nImageName=@Resources/Images/logo\n",
        );
        let (model, _) = build_render_model(&path);
        match &model.meters[0] {
            RenderMeter::Image { props, .. } => {
                assert!(props.image.as_ref().unwrap().ends_with("logo.png"));
            }
            other => panic!("meter inattendu : {other:?}"),
        }
    }

    #[test]
    fn builds_background_gradient() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_skin(
            dir.path(),
            "[Petrichor]\nSolidColor=101418FF\nSolidColor2=202830FF\nGradientAngle=90\n",
        );
        let (model, _) = build_render_model(&path);
        assert_eq!(model.background.color, Color::new(0x10, 0x14, 0x18, 0xFF));
        assert_eq!(
            model.background.color2,
            Some(Color::new(0x20, 0x28, 0x30, 0xFF))
        );
        assert_eq!(model.background.gradient_angle, 90.0);
    }

    #[test]
    fn serializes_with_type_tag() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_skin(dir.path(), "[MeterText]\nMeter=String\nText=hi\n");
        let (model, _) = build_render_model(&path);
        let json = serde_json::to_string(&model).unwrap();
        assert!(json.contains("\"type\":\"string\""));
        assert!(json.contains("\"font_size\""));
    }
}
