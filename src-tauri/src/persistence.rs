use std::path::{Path, PathBuf};

use petrichor_core::logging::{self, Level};
use petrichor_core::{ApplicationConfig, SkinState};

pub fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("Petrichor").join("Petrichor.ini"))
}

pub fn skin_key(path: &Path) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let parts: Vec<String> = canonical
        .components()
        .filter_map(|component| component.as_os_str().to_str().map(str::to_string))
        .collect();

    if parts.is_empty() {
        return canonical.to_string_lossy().to_string();
    }

    let start = parts.len().saturating_sub(2);
    parts[start..].join("\\")
}

pub fn load_state(path: &Path) -> Option<SkinState> {
    let config_path = config_path()?;
    let config = ApplicationConfig::load(&config_path).ok()?;
    let key = skin_key(path);
    config.ini.section(&key).map(SkinState::from_section)
}

pub fn update_state(path: &Path, update: impl FnOnce(&mut SkinState)) {
    let Some(config_path) = config_path() else {
        return;
    };

    let mut config = ApplicationConfig::load(&config_path).unwrap_or_default();
    let key = skin_key(path);
    let mut state = config
        .ini
        .section(&key)
        .map(SkinState::from_section)
        .unwrap_or_default();

    update(&mut state);
    config.set_skin_state(&key, &state);

    if let Err(error) = config.save(&config_path) {
        logging::logger().log(
            Level::Warning,
            Some("config"),
            format!(
                "Échec d'enregistrement de {} : {error}",
                config_path.display()
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_short_keys_from_the_last_components() {
        let key = skin_key(Path::new("/tmp/Clock/Clock.ini"));
        assert_eq!(key, "Clock\\Clock.ini");
        assert!(!key.contains('/'));
    }

    #[test]
    fn config_path_ends_with_petrichor_ini() {
        if let Some(path) = config_path() {
            assert!(path.ends_with("Petrichor/Petrichor.ini"));
        }
    }
}
