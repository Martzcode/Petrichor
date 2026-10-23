use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::diagnostics::Diagnostics;
use crate::ini::IniDocument;
use crate::measure::MeasureEngine;
use crate::model::{
    build_skin_with_document, build_skin_with_overrides, BackgroundMode, ContextItem, Meter, Skin,
};
use crate::readers::{
    read_bool, read_color, read_f32, read_floats, read_i32, read_u32, try_read_rect_ltrb, Color,
};
use crate::reference::MeasureValue;
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
    pub left_mouse_up_action: Option<String>,
    pub mouse_over_action: Option<String>,
    pub mouse_leave_action: Option<String>,
    pub mouse_actions: BTreeMap<String, String>,
    pub mouse_action_cursor: bool,
    pub mouse_action_cursor_name: Option<String>,
    pub container: Option<String>,
    pub transformation_matrix: Option<Vec<f32>>,
    pub bevel_type: String,
    pub bevel_color: Color,
    pub bevel_color2: Color,
    pub anti_alias: bool,
    pub tooltip_text: Option<String>,
    pub tooltip_title: Option<String>,
    pub tooltip_icon: Option<String>,
    pub tooltip_width: u32,
    pub tooltip_type: bool,
    pub tooltip_hidden: bool,
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
    pub inline_settings: Vec<String>,
    pub inline_patterns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImageMeter {
    pub image: Option<PathBuf>,
    pub preserve_aspect_ratio: u8,
    pub tile: bool,
    pub scale_margins: [f32; 4],
    pub crop: Option<[f32; 4]>,
    pub greyscale: bool,
    pub tint: Option<Color>,
    pub color_matrix: Option<Vec<f32>>,
    pub exif: bool,
    pub flip: String,
    pub rotate: f32,
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
pub struct HistogramMeter {
    pub primary_color: Color,
    pub secondary_color: Color,
    pub both_color: Color,
    pub primary_image: Option<PathBuf>,
    pub secondary_image: Option<PathBuf>,
    pub both_image: Option<PathBuf>,
    pub auto_scale: bool,
    pub flip: bool,
    pub graph_start: String,
    pub graph_orientation: String,
    pub primary: f64,
    pub secondary: f64,
    pub primary_measure: Option<String>,
    pub secondary_measure: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LineMeter {
    pub line_count: u32,
    pub colors: Vec<Color>,
    pub scales: Vec<f32>,
    pub flip: bool,
    pub auto_scale: bool,
    pub line_width: f32,
    pub horizontal_lines: u32,
    pub horizontal_color: Color,
    pub graph_start: String,
    pub graph_orientation: String,
    pub transform_stroke: String,
    pub values: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RotatorMeter {
    pub image: Option<PathBuf>,
    pub offset_x: f32,
    pub offset_y: f32,
    pub start_angle: f32,
    pub rotation_angle: f32,
    pub value_remainder: f32,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoundLineMeter {
    pub line_width: f32,
    pub line_length: f32,
    pub line_start: f32,
    pub start_angle: f32,
    pub rotation_angle: f32,
    pub value_remainder: f32,
    pub line_color: Color,
    pub solid: bool,
    pub control_angle: bool,
    pub control_start: bool,
    pub control_length: bool,
    pub start_shift: f32,
    pub length_shift: f32,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BitmapMeter {
    pub image: Option<PathBuf>,
    pub frames: u32,
    pub zero_frame: bool,
    pub separation: u32,
    pub extend: bool,
    pub digits: u32,
    pub transition_frames: u32,
    pub align: String,
    pub value: f64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ButtonMeter {
    pub image: Option<PathBuf>,
    pub command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShapeMeter {
    pub shapes: Vec<ShapePart>,
    pub fill_color: Color,
    pub fill_color2: Option<Color>,
    pub stroke_color: Color,
    pub stroke_color2: Option<Color>,
    pub stroke_width: f32,
    pub stroke_dash: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShapePart {
    pub kind: String,
    pub params: Vec<f32>,
    pub path: Option<String>,
    pub fill_color: Option<Color>,
    pub fill_color2: Option<Color>,
    pub stroke_color: Option<Color>,
    pub stroke_color2: Option<Color>,
    pub stroke_width: Option<f32>,
    pub stroke_dash: Option<Vec<f32>>,
    pub offset: Option<[f32; 2]>,
    pub rotate: Option<f32>,
    pub scale: Option<[f32; 2]>,
    pub skew: Option<[f32; 2]>,
    pub anchor: Option<[f32; 2]>,
    pub combine: Option<ShapeCombine>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShapeCombine {
    pub parent: usize,
    pub operations: Vec<ShapeCombineOp>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShapeCombineOp {
    pub mode: String,
    pub shape: usize,
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
    Histogram {
        #[serde(flatten)]
        common: CommonMeter,
        #[serde(flatten)]
        props: HistogramMeter,
    },
    Line {
        #[serde(flatten)]
        common: CommonMeter,
        #[serde(flatten)]
        props: LineMeter,
    },
    Rotator {
        #[serde(flatten)]
        common: CommonMeter,
        #[serde(flatten)]
        props: RotatorMeter,
    },
    RoundLine {
        #[serde(flatten)]
        common: CommonMeter,
        #[serde(flatten)]
        props: RoundLineMeter,
    },
    Bitmap {
        #[serde(flatten)]
        common: CommonMeter,
        #[serde(flatten)]
        props: BitmapMeter,
    },
    Button {
        #[serde(flatten)]
        common: CommonMeter,
        #[serde(flatten)]
        props: ButtonMeter,
    },
    Shape {
        #[serde(flatten)]
        common: CommonMeter,
        #[serde(flatten)]
        props: ShapeMeter,
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
    pub tooltip_hidden: bool,
    pub on_refresh_action: Option<String>,
    pub context_menu: Vec<ContextItem>,
    pub background: Background,
    pub meters: Vec<RenderMeter>,
    pub local_fonts: Vec<PathBuf>,
    pub blur: bool,
    pub measure_values: BTreeMap<String, String>,
}

pub fn build_render_model(ini_path: &Path) -> (RenderModel, Diagnostics) {
    build_render_model_with_variables(ini_path, None)
}

pub fn build_render_model_with_variables(
    ini_path: &Path,
    overlay: Option<&Variables>,
) -> (RenderModel, Diagnostics) {
    let (skin, document, diagnostics) = build_skin_with_document(ini_path);
    build_model_from_skin(skin, document, overlay, diagnostics)
}

pub fn build_render_model_with_options(
    ini_path: &Path,
    overlay: Option<&Variables>,
    overrides: Option<&BTreeMap<String, BTreeMap<String, String>>>,
) -> (RenderModel, Diagnostics) {
    let (skin, document, diagnostics) = match overrides {
        Some(overrides) => build_skin_with_overrides(ini_path, overrides),
        None => build_skin_with_document(ini_path),
    };
    build_model_from_skin(skin, document, overlay, diagnostics)
}

fn build_model_from_skin(
    skin: Skin,
    document: IniDocument,
    overlay: Option<&Variables>,
    diagnostics: Diagnostics,
) -> (RenderModel, Diagnostics) {
    let mut variables = Variables::new();
    for (name, value) in &skin.variables {
        variables.set(name, value);
    }
    if let Some(overlay) = overlay {
        for (name, value) in overlay.to_raw_map() {
            variables.set(&name, &value);
        }
    }

    let mut engine = MeasureEngine::new(&skin);
    if let Some(overlay) = overlay {
        for (name, value) in overlay.to_raw_map() {
            engine.set_variable(&name, &value);
        }
    }
    let report = engine.update(&petrichor_sys::backend(), chrono::Local::now());

    let meters = skin
        .meters
        .iter()
        .map(|meter| render_meter(&document, &skin, meter, &variables, &report.values))
        .collect();

    let model = RenderModel {
        name: skin.name.clone(),
        width: skin.settings.skin_width,
        height: skin.settings.skin_height,
        update_ms: skin.settings.update_ms,
        dynamic_window_size: skin.settings.dynamic_window_size,
        tooltip_hidden: skin.settings.tooltip_hidden,
        on_refresh_action: skin.settings.on_refresh_action.clone(),
        context_menu: skin
            .settings
            .context_menu
            .iter()
            .map(|item| ContextItem {
                title: variables.expand(&item.title),
                action: item.action.clone(),
            })
            .collect(),
        background: render_background(&skin),
        meters,
        local_fonts: collect_local_fonts(&skin),
        blur: skin.settings.blur,
        measure_values: report.strings,
    };

    (model, diagnostics)
}

fn render_meter(
    document: &IniDocument,
    skin: &Skin,
    meter: &Meter,
    variables: &Variables,
    values: &BTreeMap<String, MeasureValue>,
) -> RenderMeter {
    let options = resolved_options(document, &meter.name);
    let common = build_common(meter, &options, variables, skin);

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
            props: build_bar(&options, variables, values),
        },
        "HISTOGRAM" => RenderMeter::Histogram {
            common,
            props: build_histogram(skin, &options, variables, values),
        },
        "LINE" => RenderMeter::Line {
            common,
            props: build_line(&options, variables, values),
        },
        "ROTATOR" => RenderMeter::Rotator {
            common,
            props: build_rotator(skin, &options, variables, values),
        },
        "ROUNDLINE" => RenderMeter::RoundLine {
            common,
            props: build_roundline(&options, variables, values),
        },
        "BITMAP" => RenderMeter::Bitmap {
            common,
            props: build_bitmap(skin, &options, variables, values),
        },
        "BUTTON" => RenderMeter::Button {
            common,
            props: build_button(skin, &options, variables),
        },
        "SHAPE" => RenderMeter::Shape {
            common,
            props: build_shape(&options, variables),
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

const MOUSE_ACTIONS: [&str; 21] = [
    "LeftMouseUpAction",
    "LeftMouseDownAction",
    "LeftMouseDoubleClickAction",
    "RightMouseUpAction",
    "RightMouseDownAction",
    "RightMouseDoubleClickAction",
    "MiddleMouseUpAction",
    "MiddleMouseDownAction",
    "MiddleMouseDoubleClickAction",
    "X1MouseUpAction",
    "X1MouseDownAction",
    "X1MouseDoubleClickAction",
    "X2MouseUpAction",
    "X2MouseDownAction",
    "X2MouseDoubleClickAction",
    "MouseScrollUpAction",
    "MouseScrollDownAction",
    "MouseScrollLeftAction",
    "MouseScrollRightAction",
    "MouseOverAction",
    "MouseLeaveAction",
];

fn mouse_actions(options: &[(String, String)], variables: &Variables) -> BTreeMap<String, String> {
    let mut actions = BTreeMap::new();
    for name in MOUSE_ACTIONS {
        if let Some(value) = opt(options, variables, name) {
            if !value.trim().is_empty() {
                actions.insert(name.to_ascii_lowercase(), value);
            }
        }
    }
    actions
}

fn build_common(
    meter: &Meter,
    options: &[(String, String)],
    variables: &Variables,
    skin: &Skin,
) -> CommonMeter {
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
        left_mouse_up_action: opt(options, variables, "LeftMouseUpAction"),
        mouse_over_action: opt(options, variables, "MouseOverAction"),
        mouse_leave_action: opt(options, variables, "MouseLeaveAction"),
        mouse_actions: mouse_actions(options, variables),
        mouse_action_cursor: opt(options, variables, "MouseActionCursor")
            .map(|value| read_bool(&value, skin.settings.mouse_action_cursor))
            .unwrap_or(skin.settings.mouse_action_cursor),
        mouse_action_cursor_name: opt(options, variables, "MouseActionCursorName")
            .or_else(|| skin.settings.mouse_action_cursor_name.clone()),
        container: opt(options, variables, "Container")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        transformation_matrix: opt(options, variables, "TransformationMatrix").and_then(|value| {
            let values: Vec<f32> = value
                .split(';')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .filter_map(|part| part.parse::<f32>().ok())
                .collect();
            (values.len() == 6).then_some(values)
        }),
        bevel_type: opt(options, variables, "BevelType")
            .unwrap_or_else(|| "NONE".to_string())
            .trim()
            .to_ascii_uppercase(),
        bevel_color: opt(options, variables, "BevelColor")
            .map(|value| read_color(&value, Color::WHITE))
            .unwrap_or(Color::WHITE),
        bevel_color2: opt(options, variables, "BevelColor2")
            .map(|value| read_color(&value, Color::BLACK))
            .unwrap_or(Color::BLACK),
        anti_alias: opt(options, variables, "AntiAlias")
            .map(|value| read_bool(&value, false))
            .unwrap_or(false),
        tooltip_text: opt(options, variables, "ToolTipText"),
        tooltip_title: opt(options, variables, "ToolTipTitle"),
        tooltip_icon: opt(options, variables, "ToolTipIcon"),
        tooltip_width: opt(options, variables, "ToolTipWidth")
            .map(|value| read_u32(&value, 1000))
            .unwrap_or(1000),
        tooltip_type: opt(options, variables, "ToolTipType")
            .map(|value| read_bool(&value, false))
            .unwrap_or(false),
        tooltip_hidden: opt(options, variables, "ToolTipHidden")
            .map(|value| read_bool(&value, skin.settings.tooltip_hidden))
            .unwrap_or(skin.settings.tooltip_hidden),
    }
}

fn build_string(options: &[(String, String)], variables: &Variables) -> StringMeter {
    StringMeter {
        text: opt(options, variables, "Text"),
        font_face: {
            let face =
                opt(options, variables, "FontFace").unwrap_or_else(|| "sans-serif".to_string());
            crate::compat::substitute_font(&face).unwrap_or(face)
        },
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
        inline_settings: collect_indexed(options, variables, "InlineSetting"),
        inline_patterns: collect_indexed(options, variables, "InlinePattern"),
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
        crop: opt(options, variables, "Crop").and_then(|value| {
            let values = read_floats(&value);
            (values.len() == 4).then(|| {
                [
                    values[0] as f32,
                    values[1] as f32,
                    values[2] as f32,
                    values[3] as f32,
                ]
            })
        }),
        greyscale: opt(options, variables, "Greyscale")
            .or_else(|| opt(options, variables, "Grayscale"))
            .map(|value| read_bool(&value, false))
            .unwrap_or(false),
        tint: opt(options, variables, "TintColor")
            .or_else(|| opt(options, variables, "Tint"))
            .map(|value| read_color(&value, Color::WHITE)),
        color_matrix: opt(options, variables, "ColorMatrix")
            .and_then(|value| {
                let values: Vec<f32> = read_floats(&value)
                    .into_iter()
                    .map(|part| part as f32)
                    .collect();
                (values.len() == 20).then_some(values)
            })
            .or_else(|| {
                let rows = collect_numbered(options, variables, "ColorMatrix");
                let values: Vec<f32> = rows
                    .iter()
                    .take(5)
                    .flat_map(|row| {
                        read_floats(row)
                            .into_iter()
                            .map(|part| part as f32)
                            .collect::<Vec<_>>()
                    })
                    .collect();
                (values.len() == 20).then_some(values)
            }),
        exif: opt(options, variables, "EXIF")
            .or_else(|| opt(options, variables, "Exif"))
            .map(|value| read_bool(&value, false))
            .unwrap_or(false),
        flip: opt(options, variables, "Flip")
            .unwrap_or_else(|| "NONE".to_string())
            .trim()
            .to_ascii_uppercase(),
        rotate: opt(options, variables, "Rotate")
            .map(|value| read_f32(&value, 0.0))
            .unwrap_or(0.0),
    }
}

fn build_bar(
    options: &[(String, String)],
    variables: &Variables,
    values: &BTreeMap<String, MeasureValue>,
) -> BarMeter {
    let orientation = opt(options, variables, "BarOrientation")
        .unwrap_or_else(|| "HORIZONTAL".to_string())
        .trim()
        .to_ascii_uppercase();

    let measure_names = collect_numbered(options, variables, "MeasureName");
    let value = measure_names
        .first()
        .and_then(|name| lookup_value(values, name))
        .map(ratio)
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

fn build_histogram(
    skin: &Skin,
    options: &[(String, String)],
    variables: &Variables,
    values: &BTreeMap<String, MeasureValue>,
) -> HistogramMeter {
    let primary_measure = collect_numbered(options, variables, "MeasureName");
    let secondary_measure = opt(options, variables, "SecondaryMeasureName")
        .or_else(|| opt(options, variables, "MeasureName2"))
        .or_else(|| primary_measure.get(1).cloned());

    HistogramMeter {
        primary_color: opt(options, variables, "PrimaryColor")
            .map(|value| read_color(&value, Color::new(0, 255, 0, 255)))
            .unwrap_or(Color::new(0, 255, 0, 255)),
        secondary_color: opt(options, variables, "SecondaryColor")
            .map(|value| read_color(&value, Color::new(255, 0, 0, 255)))
            .unwrap_or(Color::new(255, 0, 0, 255)),
        both_color: opt(options, variables, "BothColor")
            .map(|value| read_color(&value, Color::new(255, 255, 0, 255)))
            .unwrap_or(Color::new(255, 255, 0, 255)),
        primary_image: opt(options, variables, "PrimaryImage")
            .filter(|value| !value.trim().is_empty())
            .and_then(|value| resolve_asset(skin, value.trim(), &IMAGE_EXTENSIONS)),
        secondary_image: opt(options, variables, "SecondaryImage")
            .filter(|value| !value.trim().is_empty())
            .and_then(|value| resolve_asset(skin, value.trim(), &IMAGE_EXTENSIONS)),
        both_image: opt(options, variables, "BothImage")
            .filter(|value| !value.trim().is_empty())
            .and_then(|value| resolve_asset(skin, value.trim(), &IMAGE_EXTENSIONS)),
        auto_scale: opt(options, variables, "AutoScale")
            .map(|value| read_bool(&value, false))
            .unwrap_or(false),
        flip: opt(options, variables, "Flip")
            .map(|value| read_bool(&value, false))
            .unwrap_or(false),
        graph_start: opt(options, variables, "GraphStart")
            .unwrap_or_else(|| "RIGHT".to_string())
            .trim()
            .to_ascii_uppercase(),
        graph_orientation: opt(options, variables, "GraphOrientation")
            .unwrap_or_else(|| "VERTICAL".to_string())
            .trim()
            .to_ascii_uppercase(),
        primary: primary_measure
            .first()
            .and_then(|name| lookup_value(values, name))
            .map(ratio)
            .unwrap_or(0.0),
        secondary: secondary_measure
            .as_deref()
            .and_then(|name| lookup_value(values, name))
            .map(ratio)
            .unwrap_or(0.0),
        primary_measure: primary_measure.first().cloned(),
        secondary_measure,
    }
}

fn build_line(
    options: &[(String, String)],
    variables: &Variables,
    values: &BTreeMap<String, MeasureValue>,
) -> LineMeter {
    let line_count = opt(options, variables, "LineCount")
        .map(|value| read_u32(&value, 1))
        .unwrap_or(1)
        .clamp(1, 9);
    let measure_names = collect_numbered(options, variables, "MeasureName");

    let mut colors = Vec::new();
    let mut scales = Vec::new();
    let mut line_values = Vec::new();
    for index in 0..line_count as usize {
        let color_key = indexed_key("LineColor", index);
        let scale_key = indexed_key("Scale", index);
        colors.push(
            opt(options, variables, &color_key)
                .map(|value| read_color(&value, Color::WHITE))
                .unwrap_or(Color::WHITE),
        );
        scales.push(
            opt(options, variables, &scale_key)
                .map(|value| read_f32(&value, 1.0))
                .unwrap_or(1.0),
        );
        line_values.push(
            measure_names
                .get(index)
                .and_then(|name| lookup_value(values, name))
                .map(ratio)
                .unwrap_or(0.0),
        );
    }

    LineMeter {
        line_count,
        colors,
        scales,
        flip: opt(options, variables, "Flip")
            .map(|value| read_bool(&value, false))
            .unwrap_or(false),
        auto_scale: opt(options, variables, "AutoScale")
            .map(|value| read_bool(&value, false))
            .unwrap_or(false),
        line_width: opt(options, variables, "LineWidth")
            .map(|value| read_f32(&value, 1.0))
            .unwrap_or(1.0)
            .max(0.0),
        horizontal_lines: opt(options, variables, "HorizontalLines")
            .map(|value| read_u32(&value, 0))
            .unwrap_or(0),
        horizontal_color: opt(options, variables, "HorizontalColor")
            .or_else(|| opt(options, variables, "HorizontalLineColor"))
            .map(|value| read_color(&value, Color::new(128, 128, 128, 128)))
            .unwrap_or(Color::new(128, 128, 128, 128)),
        graph_start: opt(options, variables, "GraphStart")
            .unwrap_or_else(|| "RIGHT".to_string())
            .trim()
            .to_ascii_uppercase(),
        graph_orientation: opt(options, variables, "GraphOrientation")
            .unwrap_or_else(|| "HORIZONTAL".to_string())
            .trim()
            .to_ascii_uppercase(),
        transform_stroke: opt(options, variables, "TransformStroke")
            .unwrap_or_else(|| "NORMAL".to_string())
            .trim()
            .to_ascii_uppercase(),
        values: line_values,
    }
}

fn indexed_key(base: &str, index: usize) -> String {
    if index == 0 {
        base.to_string()
    } else {
        format!("{base}{}", index + 1)
    }
}

fn first_measure_name(options: &[(String, String)], variables: &Variables) -> Option<String> {
    collect_numbered(options, variables, "MeasureName")
        .first()
        .cloned()
}

fn measure_ratio(values: &BTreeMap<String, MeasureValue>, name: Option<&str>) -> f64 {
    name.and_then(|name| lookup_value(values, name))
        .map(ratio)
        .unwrap_or(0.0)
}

fn build_rotator(
    skin: &Skin,
    options: &[(String, String)],
    variables: &Variables,
    values: &BTreeMap<String, MeasureValue>,
) -> RotatorMeter {
    let measure = first_measure_name(options, variables);
    RotatorMeter {
        image: opt(options, variables, "ImageName")
            .filter(|value| !value.trim().is_empty())
            .and_then(|value| resolve_asset(skin, value.trim(), &IMAGE_EXTENSIONS)),
        offset_x: opt(options, variables, "OffsetX")
            .map(|value| read_f32(&value, 0.0))
            .unwrap_or(0.0),
        offset_y: opt(options, variables, "OffsetY")
            .map(|value| read_f32(&value, 0.0))
            .unwrap_or(0.0),
        start_angle: opt(options, variables, "StartAngle")
            .map(|value| read_f32(&value, 0.0))
            .unwrap_or(0.0),
        rotation_angle: opt(options, variables, "RotationAngle")
            .map(|value| read_f32(&value, std::f32::consts::TAU))
            .unwrap_or(std::f32::consts::TAU),
        value_remainder: opt(options, variables, "ValueRemainder")
            .map(|value| read_f32(&value, 0.0))
            .unwrap_or(0.0),
        value: measure_ratio(values, measure.as_deref()),
    }
}

fn build_roundline(
    options: &[(String, String)],
    variables: &Variables,
    values: &BTreeMap<String, MeasureValue>,
) -> RoundLineMeter {
    let measure = first_measure_name(options, variables);
    RoundLineMeter {
        line_width: opt(options, variables, "LineWidth")
            .map(|value| read_f32(&value, 1.0))
            .unwrap_or(1.0)
            .max(0.0),
        line_length: opt(options, variables, "LineLength")
            .map(|value| read_f32(&value, 20.0))
            .unwrap_or(20.0)
            .max(0.0),
        line_start: opt(options, variables, "LineStart")
            .map(|value| read_f32(&value, -1.0))
            .unwrap_or(-1.0),
        start_angle: opt(options, variables, "StartAngle")
            .map(|value| read_f32(&value, 0.0))
            .unwrap_or(0.0),
        rotation_angle: opt(options, variables, "RotationAngle")
            .map(|value| read_f32(&value, std::f32::consts::TAU))
            .unwrap_or(std::f32::consts::TAU),
        value_remainder: opt(options, variables, "ValueRemainder")
            .map(|value| read_f32(&value, 0.0))
            .unwrap_or(0.0),
        line_color: opt(options, variables, "LineColor")
            .map(|value| read_color(&value, Color::BLACK))
            .unwrap_or(Color::BLACK),
        solid: opt(options, variables, "Solid")
            .map(|value| read_bool(&value, false))
            .unwrap_or(false),
        control_angle: opt(options, variables, "ControlAngle")
            .map(|value| read_bool(&value, true))
            .unwrap_or(true),
        control_start: opt(options, variables, "ControlStart")
            .map(|value| read_bool(&value, false))
            .unwrap_or(false),
        control_length: opt(options, variables, "ControlLength")
            .map(|value| read_bool(&value, false))
            .unwrap_or(false),
        start_shift: opt(options, variables, "StartShift")
            .map(|value| read_f32(&value, 0.0))
            .unwrap_or(0.0),
        length_shift: opt(options, variables, "LengthShift")
            .map(|value| read_f32(&value, 0.0))
            .unwrap_or(0.0),
        value: measure_ratio(values, measure.as_deref()),
    }
}

fn build_bitmap(
    skin: &Skin,
    options: &[(String, String)],
    variables: &Variables,
    values: &BTreeMap<String, MeasureValue>,
) -> BitmapMeter {
    let measure = first_measure_name(options, variables);
    let (value, text) = measure
        .as_deref()
        .and_then(|name| lookup_value(values, name))
        .map(|value| {
            let text = value
                .string
                .clone()
                .unwrap_or_else(|| format!("{}", value.number.round() as i64));
            (ratio(value), text)
        })
        .unwrap_or((0.0, String::new()));

    BitmapMeter {
        image: opt(options, variables, "BitmapImage")
            .filter(|value| !value.trim().is_empty())
            .and_then(|value| resolve_asset(skin, value.trim(), &IMAGE_EXTENSIONS)),
        frames: opt(options, variables, "BitmapFrames")
            .map(|value| read_u32(&value, 1))
            .unwrap_or(1)
            .max(1),
        zero_frame: opt(options, variables, "BitmapZeroFrame")
            .map(|value| read_bool(&value, false))
            .unwrap_or(false),
        separation: opt(options, variables, "BitmapSeparation")
            .map(|value| read_u32(&value, 0))
            .unwrap_or(0),
        extend: opt(options, variables, "BitmapExtend")
            .map(|value| read_bool(&value, false))
            .unwrap_or(false),
        digits: opt(options, variables, "BitmapDigits")
            .map(|value| read_u32(&value, 0))
            .unwrap_or(0),
        transition_frames: opt(options, variables, "BitmapTransitionFrames")
            .map(|value| read_u32(&value, 0))
            .unwrap_or(0),
        align: opt(options, variables, "BitmapAlign")
            .unwrap_or_else(|| "LEFT".to_string())
            .trim()
            .to_ascii_uppercase(),
        value,
        text,
    }
}

fn build_shape(options: &[(String, String)], variables: &Variables) -> ShapeMeter {
    let fill_color = opt(options, variables, "FillColor")
        .or_else(|| opt(options, variables, "SolidColor"))
        .map(|value| read_color(&value, Color::BLACK))
        .unwrap_or(Color::BLACK);
    let fill_color2 =
        opt(options, variables, "FillColor2").map(|value| read_color(&value, Color::BLACK));
    let stroke_color = opt(options, variables, "StrokeColor")
        .or_else(|| opt(options, variables, "LineColor"))
        .map(|value| read_color(&value, Color::WHITE))
        .unwrap_or(Color::WHITE);
    let stroke_color2 = opt(options, variables, "StrokeColor2")
        .or_else(|| opt(options, variables, "LineColor2"))
        .map(|value| read_color(&value, Color::WHITE));
    let stroke_width = opt(options, variables, "StrokeWidth")
        .or_else(|| opt(options, variables, "LineWidth"))
        .map(|value| read_f32(&value, 0.0))
        .unwrap_or(0.0);
    let stroke_dash = opt(options, variables, "StrokeDash")
        .or_else(|| opt(options, variables, "LineDash"))
        .map(|value| {
            read_floats(&value)
                .into_iter()
                .map(|part| part.max(0.0) as f32)
                .collect()
        })
        .unwrap_or_default();

    let mut shapes: Vec<ShapePart> = collect_indexed(options, variables, "Shape")
        .iter()
        .map(|value| parse_shape_part(value, fill_color, stroke_color, stroke_width))
        .collect();

    for index in 0..shapes.len() {
        let Some(combine) = shapes[index].combine.clone() else {
            continue;
        };
        if let Some(parent) = shapes.get(combine.parent) {
            let style = (
                parent.fill_color,
                parent.fill_color2,
                parent.stroke_color,
                parent.stroke_color2,
                parent.stroke_width,
                parent.stroke_dash.clone(),
                parent.offset,
                parent.rotate,
                parent.scale,
                parent.skew,
                parent.anchor,
            );
            let part = &mut shapes[index];
            part.fill_color = style.0;
            part.fill_color2 = style.1;
            part.stroke_color = style.2;
            part.stroke_color2 = style.3;
            part.stroke_width = style.4;
            part.stroke_dash = style.5;
            part.offset = style.6;
            part.rotate = style.7;
            part.scale = style.8;
            part.skew = style.9;
            part.anchor = style.10;
        }
    }

    ShapeMeter {
        shapes,
        fill_color,
        fill_color2,
        stroke_color,
        stroke_color2,
        stroke_width,
        stroke_dash,
    }
}

fn parse_shape_part(value: &str, fill: Color, stroke: Color, stroke_width: f32) -> ShapePart {
    let mut segments = value.split('|').map(str::trim);
    let head = segments.next().unwrap_or_default();
    let kind_end = head.find(char::is_whitespace).unwrap_or(head.len());
    let kind = head[..kind_end].to_ascii_lowercase();
    let rest = head[kind_end..].trim();

    let mut part = ShapePart {
        kind: kind.clone(),
        params: Vec::new(),
        path: None,
        fill_color: Some(fill),
        fill_color2: None,
        stroke_color: Some(stroke),
        stroke_color2: None,
        stroke_width: Some(stroke_width),
        stroke_dash: None,
        offset: None,
        rotate: None,
        scale: None,
        skew: None,
        anchor: None,
        combine: None,
    };

    if kind == "combine" {
        part.combine = Some(parse_combine(rest, segments.clone()));
    } else if kind == "path" {
        let path = rest.trim_matches('"').trim().to_string();
        part.path = (!path.is_empty()).then_some(path);
    } else {
        part.params = read_floats(rest)
            .into_iter()
            .map(|part| part as f32)
            .collect();
    }

    for segment in segments {
        let lower = segment.to_ascii_lowercase();
        let tail = |keyword: &str| {
            lower
                .starts_with(keyword)
                .then(|| segment[keyword.len()..].trim())
        };
        if let Some(value) = tail("fill color2").or_else(|| tail("fillcolor2")) {
            part.fill_color2 = Some(read_color(value, fill));
        } else if let Some(value) = tail("fill color").or_else(|| tail("fillcolor")) {
            part.fill_color = Some(read_color(value, fill));
        } else if let Some(value) = tail("stroke width").or_else(|| tail("strokewidth")) {
            part.stroke_width = Some(read_f32(value, stroke_width));
        } else if let Some(value) = tail("stroke color2").or_else(|| tail("strokecolor2")) {
            part.stroke_color2 = Some(read_color(value, stroke));
        } else if let Some(value) = tail("stroke color").or_else(|| tail("strokecolor")) {
            part.stroke_color = Some(read_color(value, stroke));
        } else if let Some(value) = tail("stroke dash").or_else(|| tail("strokedash")) {
            part.stroke_dash = Some(
                read_floats(value)
                    .into_iter()
                    .map(|value| value.max(0.0) as f32)
                    .collect(),
            );
        } else if let Some(value) = tail("offset") {
            let values = read_floats(value);
            if values.len() >= 2 {
                part.offset = Some([values[0] as f32, values[1] as f32]);
            }
        } else if let Some(value) = tail("rotate") {
            let values = read_floats(value);
            if let Some(angle) = values.first() {
                part.rotate = Some(*angle as f32);
                if values.len() >= 3 {
                    part.anchor = Some([values[1] as f32, values[2] as f32]);
                }
            }
        } else if let Some(value) = tail("scale") {
            let values = read_floats(value);
            if values.len() >= 2 {
                part.scale = Some([values[0] as f32, values[1] as f32]);
                if values.len() >= 4 {
                    part.anchor = Some([values[2] as f32, values[3] as f32]);
                }
            }
        } else if let Some(value) = tail("skew") {
            let values = read_floats(value);
            if values.len() >= 2 {
                part.skew = Some([values[0] as f32, values[1] as f32]);
                if values.len() >= 4 {
                    part.anchor = Some([values[2] as f32, values[3] as f32]);
                }
            }
        }
    }

    part
}

fn parse_combine<'a>(value: &str, segments: impl Iterator<Item = &'a str>) -> ShapeCombine {
    let parent = parse_shape_ref(value).unwrap_or(0);
    let mut operations = Vec::new();
    let mut mode = "UNION".to_string();
    for segment in segments {
        let text = segment.trim();
        let lower = text.to_ascii_lowercase();
        if matches!(lower.as_str(), "union" | "xor" | "intersect" | "exclude") {
            mode = lower.to_ascii_uppercase();
        } else if let Some(shape) = parse_shape_ref(text) {
            operations.push(ShapeCombineOp {
                mode: mode.clone(),
                shape,
            });
        }
    }
    ShapeCombine { parent, operations }
}

fn parse_shape_ref(value: &str) -> Option<usize> {
    let text = value.trim();
    let prefix = text.get(..5)?;
    if !prefix.eq_ignore_ascii_case("shape") {
        return None;
    }
    text.get(5..)?.trim().parse::<usize>().ok()?.checked_sub(1)
}

fn build_button(skin: &Skin, options: &[(String, String)], variables: &Variables) -> ButtonMeter {
    ButtonMeter {
        image: opt(options, variables, "ButtonImage")
            .filter(|value| !value.trim().is_empty())
            .and_then(|value| resolve_asset(skin, value.trim(), &IMAGE_EXTENSIONS)),
        command: opt(options, variables, "ButtonCommand"),
    }
}

fn lookup_value<'a>(
    values: &'a BTreeMap<String, MeasureValue>,
    name: &str,
) -> Option<&'a MeasureValue> {
    values.get(name).or_else(|| {
        values
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value)
    })
}

fn ratio(measure: &MeasureValue) -> f64 {
    let min = measure.min.unwrap_or(0.0);
    let max = measure.max.unwrap_or(100.0);
    let span = max - min;
    if span.abs() < f64::EPSILON {
        0.0
    } else {
        ((measure.number - min) / span).clamp(0.0, 1.0)
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
    let translated = crate::compat::translate_path_checked(value)?;
    let candidate = if let Some(relative) = translated.strip_prefix("@Resources/") {
        skin.root.join("@Resources").join(relative)
    } else {
        let path = PathBuf::from(translated);
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

fn collect_indexed(options: &[(String, String)], variables: &Variables, base: &str) -> Vec<String> {
    let mut values = Vec::new();
    for n in 1..=9 {
        let key = if n == 1 {
            base.to_string()
        } else {
            format!("{base}{n}")
        };
        let Some(value) = opt(options, variables, &key) else {
            continue;
        };
        values.push(value);
    }
    values
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
    fn exposes_every_mouse_action_and_cursor() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_skin(
            dir.path(),
            "\
[Petrichor]
MouseActionCursor=1
MouseActionCursorName=Hand

[MeterPanel]
Meter=String
Text=M
LeftMouseDownAction=!Log down
LeftMouseDoubleClickAction=!Log double
RightMouseUpAction=!Log right
MiddleMouseDownAction=!Log middle
X1MouseUpAction=!Log x1
X2MouseUpAction=!Log x2
MouseScrollUpAction=!Log up
MouseScrollDownAction=!Log down
MouseScrollLeftAction=!Log left
MouseScrollRightAction=!Log right
MouseOverAction=!Log over
MouseLeaveAction=!Log leave
",
        );
        let (model, diagnostics) = build_render_model(&path);
        assert!(diagnostics.is_empty());
        let RenderMeter::String { common, .. } = &model.meters[0] else {
            panic!("meter String attendu");
        };
        assert_eq!(common.mouse_actions.len(), 12);
        assert_eq!(
            common
                .mouse_actions
                .get("rightmouseupaction")
                .map(String::as_str),
            Some("!Log right")
        );
        assert!(common.mouse_action_cursor);
        assert_eq!(common.mouse_action_cursor_name.as_deref(), Some("Hand"));
    }

    #[test]
    fn expands_context_menu_titles() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_skin(
            dir.path(),
            "\
[Variables]
Label=Horloge

[Petrichor]
ContextTitle=Ouvrir #Label#
ContextAction=[!Log menu]
ContextTitle2=-
",
        );
        let (model, diagnostics) = build_render_model(&path);
        assert!(diagnostics.is_empty());
        assert_eq!(model.context_menu.len(), 2);
        assert_eq!(model.context_menu[0].title, "Ouvrir Horloge");
        assert_eq!(model.context_menu[0].action, "[!Log menu]");
        assert_eq!(model.context_menu[1].title, "-");
    }

    #[test]
    fn parses_image_pipeline_options() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_skin(
            dir.path(),
            "\
[Picture]
Meter=Image
ImageName=missing.png
Crop=1,2,30,40
Greyscale=1
TintColor=FF0000
ColorMatrix=1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1,0.1,0.2,0.3,0
EXIF=1
Flip=HORIZONTAL
Rotate=90
",
        );
        let (model, _) = build_render_model(&path);
        match &model.meters[0] {
            RenderMeter::Image { props, .. } => {
                assert_eq!(props.crop, Some([1.0, 2.0, 30.0, 40.0]));
                assert!(props.greyscale);
                assert_eq!(props.tint, Some(Color::new(255, 0, 0, 255)));
                assert_eq!(props.color_matrix.as_ref().unwrap().len(), 20);
                assert!((props.color_matrix.as_ref().unwrap()[16] - 0.1).abs() < 0.001);
                assert!(props.exif);
                assert_eq!(props.flip, "HORIZONTAL");
                assert_eq!(props.rotate, 90.0);
            }
            other => panic!("meter inattendu : {other:?}"),
        }
    }

    #[test]
    fn parses_numbered_color_matrices() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_skin(
            dir.path(),
            "\
[Picture]
Meter=Image
ImageName=missing.png
ColorMatrix1=1,0,0,0
ColorMatrix2=0,1,0,0
ColorMatrix3=0,0,1,0
ColorMatrix4=0,0,0,1
ColorMatrix5=0.1,0.2,0.3,0
",
        );
        let (model, _) = build_render_model(&path);
        match &model.meters[0] {
            RenderMeter::Image { props, .. } => {
                let matrix = props.color_matrix.as_ref().unwrap();
                assert_eq!(matrix.len(), 20);
                assert!((matrix[16] - 0.1).abs() < 0.001);
                assert!((matrix[19]).abs() < 0.001);
            }
            other => panic!("meter inattendu : {other:?}"),
        }
    }

    #[test]
    fn parses_shape_arcs_and_curves() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_skin(
            dir.path(),
            "\
[Panel]
Meter=Shape
Shape=Arc 0,0,100,100,50,50,0,1,0,1
Shape2=Curve 0,0,100,100,50,0,50,100,1
",
        );
        let (model, diagnostics) = build_render_model(&path);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        match &model.meters[0] {
            RenderMeter::Shape { props, .. } => {
                assert_eq!(props.shapes[0].kind, "arc");
                assert_eq!(props.shapes[0].params.len(), 10);
                assert_eq!(props.shapes[0].params[9], 1.0);
                assert_eq!(props.shapes[1].kind, "curve");
                assert_eq!(props.shapes[1].params.len(), 9);
                assert_eq!(props.shapes[1].params[8], 1.0);
            }
            other => panic!("meter inattendu : {other:?}"),
        }
    }

    #[test]
    fn parses_shape_parts_and_styles() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_skin(
            dir.path(),
            "\
[Panel]
Meter=Shape
Shape=Rectangle 0,0,100,50,6 | Fill Color 255,0,0,128 | Stroke Color 0,255,0 | StrokeWidth 2
Shape2=Ellipse 50,25,20,20 | Fill Color 10,20,30,255 | Offset 5,6 | Rotate 45,10,20 | Scale 2,3 | Skew 5,10
Shape3=Line 0,0,100,50 | Stroke Width 3
Shape4=Path \"M 0,0 L 10,10 C 20,20 30,30 40,40 Z\"
Shape5=Combine Shape1 | Union | Shape2 | Exclude | Shape3
FillColor=1E293B
StrokeColor=38BDF8
",
        );
        let (model, diagnostics) = build_render_model(&path);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");

        match &model.meters[0] {
            RenderMeter::Shape { props, .. } => {
                assert_eq!(props.shapes.len(), 5);
                assert_eq!(props.shapes[0].kind, "rectangle");
                assert_eq!(props.shapes[0].params, [0.0, 0.0, 100.0, 50.0, 6.0]);
                assert_eq!(props.shapes[0].fill_color, Some(Color::new(255, 0, 0, 128)));
                assert_eq!(props.shapes[0].stroke_width, Some(2.0));
                assert_eq!(props.shapes[1].kind, "ellipse");
                assert_eq!(props.shapes[1].offset, Some([5.0, 6.0]));
                assert_eq!(props.shapes[1].rotate, Some(45.0));
                assert_eq!(props.shapes[1].anchor, Some([10.0, 20.0]));
                assert_eq!(props.shapes[1].scale, Some([2.0, 3.0]));
                assert_eq!(props.shapes[1].skew, Some([5.0, 10.0]));
                assert_eq!(props.shapes[2].kind, "line");
                assert_eq!(props.shapes[2].stroke_width, Some(3.0));
                assert_eq!(props.shapes[3].kind, "path");
                assert!(props.shapes[3].path.as_deref().unwrap().contains("C 20"));
                let combine = props.shapes[4].combine.as_ref().unwrap();
                assert_eq!(combine.parent, 0);
                assert_eq!(combine.operations.len(), 2);
                assert_eq!(combine.operations[0].mode, "UNION");
                assert_eq!(combine.operations[0].shape, 1);
                assert_eq!(combine.operations[1].mode, "EXCLUDE");
                assert_eq!(combine.operations[1].shape, 2);
                assert_eq!(props.shapes[4].fill_color, Some(Color::new(255, 0, 0, 128)));
            }
            other => panic!("meter inattendu : {other:?}"),
        }
    }

    #[test]
    fn parses_container_transform_tooltip_and_inline() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_skin(
            dir.path(),
            "\
[Frame]
Meter=String
W=120
H=80
SolidColor=101010
[Child]
Meter=String
Container=Frame
ToolTipText=Bonjour
ToolTipTitle=Titre
ToolTipIcon=INFO
ToolTipWidth=200
ToolTipType=1
TransformationMatrix=1;0;0;1;5;6
BevelType=UP
BevelColor=FF0000
AntiAlias=1
InlineSetting=COLOR FF0000
InlinePattern=CPU
Text=CPU 42
",
        );
        let (model, diagnostics) = build_render_model(&path);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");

        match &model.meters[1] {
            RenderMeter::String { common, props } => {
                assert_eq!(common.container.as_deref(), Some("Frame"));
                assert_eq!(
                    common.transformation_matrix.as_deref(),
                    Some([1.0, 0.0, 0.0, 1.0, 5.0, 6.0].as_slice())
                );
                assert_eq!(common.bevel_type, "UP");
                assert_eq!(common.tooltip_text.as_deref(), Some("Bonjour"));
                assert_eq!(common.tooltip_title.as_deref(), Some("Titre"));
                assert_eq!(common.tooltip_icon.as_deref(), Some("INFO"));
                assert_eq!(common.tooltip_width, 200);
                assert!(common.tooltip_type);
                assert!(common.anti_alias);
                assert_eq!(props.inline_settings, ["COLOR FF0000"]);
                assert_eq!(props.inline_patterns, ["CPU"]);
            }
            other => panic!("meter inattendu : {other:?}"),
        }
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
[MeterHisto]
Meter=Histogram
[MeterCurve]
Meter=Line
[MeterDial]
Meter=Rotator
[MeterArc]
Meter=RoundLine
[MeterFrames]
Meter=Bitmap
[MeterClick]
Meter=Button
[MeterWeird]
Meter=Shape
",
        );
        let (model, _) = build_render_model(&path);
        assert!(matches!(model.meters[0], RenderMeter::String { .. }));
        assert!(matches!(model.meters[1], RenderMeter::Bar { .. }));
        assert!(matches!(model.meters[2], RenderMeter::Image { .. }));
        assert!(matches!(model.meters[3], RenderMeter::Histogram { .. }));
        assert!(matches!(model.meters[4], RenderMeter::Line { .. }));
        assert!(matches!(model.meters[5], RenderMeter::Rotator { .. }));
        assert!(matches!(model.meters[6], RenderMeter::RoundLine { .. }));
        assert!(matches!(model.meters[7], RenderMeter::Bitmap { .. }));
        assert!(matches!(model.meters[8], RenderMeter::Button { .. }));
        assert!(matches!(model.meters[9], RenderMeter::Shape { .. }));
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
    fn applies_option_overrides() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_skin(dir.path(), "[MeterTitle]\nMeter=String\nText=avant\n");

        let mut overrides: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        overrides
            .entry("MeterTitle".to_string())
            .or_default()
            .insert("Text".to_string(), "après".to_string());

        let (model, _) = build_render_model_with_options(&path, None, Some(&overrides));
        match &model.meters[0] {
            RenderMeter::String { props, .. } => {
                assert_eq!(props.text.as_deref(), Some("après"));
            }
            other => panic!("meter inattendu : {other:?}"),
        }
    }

    #[test]
    fn substitutes_windows_fonts_and_reads_blur() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_skin(
            dir.path(),
            "[Petrichor]\nBlur=1\n[MeterTitle]\nMeter=String\nFontFace=Segoe UI\nText=test\n",
        );

        let (model, _) = build_render_model(&path);
        assert!(model.blur);
        match &model.meters[0] {
            RenderMeter::String { props, .. } => {
                assert_eq!(props.font_face.as_str(), "Cantarell");
            }
            other => panic!("meter inattendu : {other:?}"),
        }
    }

    #[test]
    fn builds_histogram_and_line_meters() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_skin(
            dir.path(),
            "\
[CPU]
Measure=Calc
Formula=50
[RAM]
Measure=Calc
Formula=25
[MeterHisto]
Meter=Histogram
MeasureName=CPU
SecondaryMeasureName=RAM
PrimaryColor=00FF00FF
SecondaryColor=FF0000FF
[MeterCurve]
Meter=Line
LineCount=2
MeasureName=CPU
MeasureName2=RAM
LineColor=FFFFFFFF
LineColor2=00FF00FF
Scale2=2
",
        );
        let (model, _) = build_render_model(&path);
        match &model.meters[0] {
            RenderMeter::Histogram { props, .. } => {
                assert_eq!(props.primary, 0.5);
                assert_eq!(props.secondary, 0.25);
                assert_eq!(props.primary_measure.as_deref(), Some("CPU"));
                assert_eq!(props.secondary_measure.as_deref(), Some("RAM"));
                assert_eq!(props.primary_color, Color::new(0, 255, 0, 255));
            }
            other => panic!("meter inattendu : {other:?}"),
        }
        match &model.meters[1] {
            RenderMeter::Line { props, .. } => {
                assert_eq!(props.line_count, 2);
                assert_eq!(props.values, vec![0.5, 0.25]);
                assert_eq!(props.colors.len(), 2);
                assert_eq!(props.colors[1], Color::new(0, 255, 0, 255));
                assert_eq!(props.scales, vec![1.0, 2.0]);
            }
            other => panic!("meter inattendu : {other:?}"),
        }
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
