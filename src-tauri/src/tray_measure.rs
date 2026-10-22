use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use tauri::image::Image;
use tauri::{AppHandle, Manager};

use petrichor_core::readers::Color;
use petrichor_core::{ApplicationConfig, TrayMeasure};
use petrichor_sys::{CpuTimes, SystemBackend};

use crate::commands;
use crate::persistence;

const ICON_SIZE: u32 = 32;
const HISTORY_LEN: usize = 8;

#[derive(Default)]
pub struct TrayMeasureState {
    history: Mutex<VecDeque<f64>>,
    previous_cpu: Mutex<Option<CpuTimes>>,
    custom: Mutex<bool>,
}

impl TrayMeasureState {
    pub fn new() -> Self {
        TrayMeasureState::default()
    }
}

pub fn spawn(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let mut failing = false;
        loop {
            if let Err(error) = update(&app) {
                if !failing {
                    petrichor_core::logging::warning(format!(
                        "Mise à jour du tray measure impossible : {error}"
                    ));
                    failing = true;
                }
            } else {
                failing = false;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    });
}

fn update(app: &AppHandle) -> Result<(), String> {
    let config_path = persistence::config_path().ok_or_else(|| "config introuvable".to_string())?;
    let config = ApplicationConfig::load(&config_path).map_err(|error| error.to_string())?;
    let state = app.state::<TrayMeasureState>();
    let Some(measure) = TrayMeasure::from_ini(&config.ini) else {
        let was_custom = *state.custom.lock().map_err(|error| error.to_string())?;
        if was_custom {
            if let Some(tray) = app.tray_by_id("petrichor") {
                if let Some(icon) = app.default_window_icon().cloned() {
                    tray.set_icon(Some(icon))
                        .map_err(|error| error.to_string())?;
                }
                let _ = tray.set_tooltip(Some("Petrichor"));
            }
            *state.custom.lock().map_err(|error| error.to_string())? = false;
        }
        return Ok(());
    };
    let value = measure_value(&measure.measure, &state);
    let history = {
        let mut history = state.history.lock().map_err(|error| error.to_string())?;
        history.push_back(value);
        while history.len() > HISTORY_LEN {
            history.pop_front();
        }
        history.clone()
    };
    let icon = build_icon(&measure, value, &history, config_path.parent());
    let Some(tray) = app.tray_by_id("petrichor") else {
        return Ok(());
    };
    if let Some(icon) = icon {
        tray.set_icon(Some(icon))
            .map_err(|error| error.to_string())?;
    }
    tray.set_tooltip(Some(format!(
        "Petrichor — {} {:.0}%",
        measure.measure, value
    )))
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn measure_value(measure: &str, state: &TrayMeasureState) -> f64 {
    let backend = petrichor_sys::backend();
    match measure.trim().to_uppercase().as_str() {
        "CPU" => {
            let snapshot = backend.cpu_snapshot();
            let mut previous = state.previous_cpu.lock().ok();
            let value = previous
                .as_ref()
                .and_then(|slot| **slot)
                .map(|old| snapshot.overall.usage_percent(old))
                .unwrap_or(0.0);
            if let Some(slot) = previous.as_mut() {
                **slot = Some(snapshot.overall);
            }
            value
        }
        "RAM" | "MEMORY" | "PHYSICALMEMORY" => backend.memory_info().used_percent(),
        "SWAP" | "SWAPMEMORY" => {
            let info = backend.memory_info();
            if info.swap_total_kb == 0 {
                0.0
            } else {
                let used = info.swap_total_kb.saturating_sub(info.swap_free_kb);
                (used as f64 / info.swap_total_kb as f64 * 100.0).clamp(0.0, 100.0)
            }
        }
        "DISK" | "DISKSPACE" | "DRIVE" => backend
            .disk_space("/", false)
            .map(|disk| {
                if disk.total_bytes == 0 {
                    0.0
                } else {
                    ((disk.total_bytes - disk.available_bytes) as f64 / disk.total_bytes as f64
                        * 100.0)
                        .clamp(0.0, 100.0)
                }
            })
            .unwrap_or(0.0),
        _ => 0.0,
    }
}

fn build_icon(
    measure: &TrayMeasure,
    value: f64,
    history: &VecDeque<f64>,
    base: Option<&Path>,
) -> Option<Image<'static>> {
    let value = value.clamp(0.0, 100.0);
    match measure.tray_meter.as_str() {
        "BITMAP" => measure
            .tray_bitmap
            .as_deref()
            .and_then(|bitmap| resolve_bitmap(bitmap, base))
            .and_then(|path| bitmap_icon(&path, value, measure.tray_bitmap_frames))
            .or_else(|| Some(histogram_icon(measure, history))),
        "SOLID" => Some(solid_icon(measure.tray_color1)),
        _ => Some(histogram_icon(measure, history)),
    }
}

fn resolve_bitmap(bitmap: &str, base: Option<&Path>) -> Option<PathBuf> {
    let candidate = Path::new(bitmap);
    if candidate.is_absolute() {
        return candidate.is_file().then(|| candidate.to_path_buf());
    }
    let from_config = base.map(|base| base.join(candidate));
    if let Some(path) = from_config.filter(|path| path.is_file()) {
        return Some(path);
    }
    commands::skin_root()
        .map(|root| root.join(candidate))
        .filter(|path| path.is_file())
}

fn bitmap_icon(path: &Path, value: f64, frames: u32) -> Option<Image<'static>> {
    let image = Image::from_path(path).ok()?;
    let frames = frames.max(1);
    let frame_width = image.width() / frames;
    if frame_width == 0 {
        return None;
    }
    let index = ((value / 100.0) * (frames - 1) as f64).round() as u32;
    let source = image.rgba();
    let height = image.height();
    let mut rgba = Vec::with_capacity((frame_width * height * 4) as usize);
    for row in 0..height {
        let start = ((row * image.width() + index * frame_width) * 4) as usize;
        let end = start + (frame_width * 4) as usize;
        rgba.extend_from_slice(&source[start..end]);
    }
    Some(Image::new_owned(rgba, frame_width, height))
}

fn solid_icon(color: Color) -> Image<'static> {
    let mut rgba = Vec::with_capacity((ICON_SIZE * ICON_SIZE * 4) as usize);
    for _ in 0..ICON_SIZE * ICON_SIZE {
        rgba.extend_from_slice(&[color.r, color.g, color.b, color.a]);
    }
    Image::new_owned(rgba, ICON_SIZE, ICON_SIZE)
}

fn histogram_icon(measure: &TrayMeasure, history: &VecDeque<f64>) -> Image<'static> {
    let mut rgba = vec![0u8; (ICON_SIZE * ICON_SIZE * 4) as usize];
    let count = HISTORY_LEN;
    let bar_width = 3u32;
    let gap = 1u32;
    let total_width = count as u32 * (bar_width + gap) - gap;
    let left = ICON_SIZE.saturating_sub(total_width) / 2;

    for index in 0..count {
        let value = history
            .get(history.len().saturating_sub(count - index))
            .copied()
            .unwrap_or(0.0)
            .clamp(0.0, 100.0);
        let height = ((value / 100.0) * ICON_SIZE as f64).round() as u32;
        let x = left + index as u32 * (bar_width + gap);
        let color = blend(
            measure.tray_color2,
            measure.tray_color1,
            (index + 1) as f32 / count as f32,
        );
        for y in ICON_SIZE.saturating_sub(height)..ICON_SIZE {
            for px in x..(x + bar_width).min(ICON_SIZE) {
                let offset = ((y * ICON_SIZE + px) * 4) as usize;
                rgba[offset..offset + 4].copy_from_slice(&[color.r, color.g, color.b, color.a]);
            }
        }
    }
    Image::new_owned(rgba, ICON_SIZE, ICON_SIZE)
}

fn blend(from: Color, to: Color, amount: f32) -> Color {
    let amount = amount.clamp(0.0, 1.0);
    let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * amount).round() as u8;
    Color {
        r: mix(from.r, to.r),
        g: mix(from.g, to.g),
        b: mix(from.b, to.b),
        a: mix(from.a, to.a),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolates_colors() {
        let from = Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        };
        let to = Color {
            r: 255,
            g: 100,
            b: 50,
            a: 255,
        };
        assert_eq!(
            blend(from, to, 0.5),
            Color {
                r: 128,
                g: 50,
                b: 25,
                a: 255
            }
        );
    }

    #[test]
    fn histogram_icon_has_expected_size() {
        let measure = TrayMeasure::default();
        let history = VecDeque::from([10.0, 20.0, 40.0]);
        let icon = histogram_icon(&measure, &history);
        assert_eq!(icon.width(), ICON_SIZE);
        assert_eq!(icon.height(), ICON_SIZE);
    }
}
