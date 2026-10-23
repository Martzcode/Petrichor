use std::path::Path;

use petrichor_compat::{
    analyze_skin, detect_capabilities, export_report, migrate_skin, Capability, CompatLevel,
    CompatReport, MigrationChange, MigrationResult,
};
use petrichor_core::logging::{self, Level};

pub fn report_for(path: &Path) -> CompatReport {
    let (skin, _) = petrichor_core::build_skin(path);
    analyze_skin(&skin)
}

pub fn log_report(report: &CompatReport) {
    for entry in &report.entries {
        let level = match entry.level {
            CompatLevel::Native => continue,
            CompatLevel::Automatic => Level::Notice,
            CompatLevel::Manual => Level::Warning,
            CompatLevel::Unsupported => Level::Warning,
        };
        let plan = entry
            .plan
            .map(|plan| format!(" plan {}", plan.code()))
            .unwrap_or_default();
        let resolved = entry
            .resolved
            .as_deref()
            .map(|value| format!(" → {value}"))
            .unwrap_or_default();
        logging::logger().log(
            level,
            Some(&report.skin),
            format!(
                "Compatibilité {} [{}·{}]{}{}{}",
                entry.feature,
                entry.level.code(),
                entry.level.label(),
                plan,
                resolved,
                entry
                    .message
                    .as_deref()
                    .map(|message| format!(" : {message}"))
                    .unwrap_or_default()
            ),
        );
    }
}

#[tauri::command]
pub fn compatibility_capabilities() -> Vec<Capability> {
    detect_capabilities()
}

#[tauri::command]
pub fn compatibility_report(path: String) -> Result<CompatReport, String> {
    let path = Path::new(&path);
    if !path.is_file() {
        return Err(format!("skin introuvable : {}", path.display()));
    }
    Ok(report_for(path))
}

#[tauri::command]
pub fn export_compatibility_report(path: String) -> Result<String, String> {
    let report = compatibility_report(path)?;
    export_report(&report).map(|path| path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn preview_skin_migration(path: String) -> Result<Vec<MigrationChange>, String> {
    let path = Path::new(&path);
    if !path.is_file() {
        return Err(format!("skin introuvable : {}", path.display()));
    }
    petrichor_compat::preview_migration(path)
}

#[tauri::command]
pub fn migrate_skin_file(path: String) -> Result<MigrationResult, String> {
    let path = Path::new(&path);
    if !path.is_file() {
        return Err(format!("skin introuvable : {}", path.display()));
    }
    let result = migrate_skin(path)?;
    if !result.changes.is_empty() {
        logging::notice(format!(
            "Skin migré : {} ({} remplacement(s), sauvegarde {})",
            result.path.display(),
            result.changes.len(),
            result.backup.display()
        ));
    }
    Ok(result)
}
