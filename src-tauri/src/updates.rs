use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::manage;

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateManifest {
    pub version: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateStatus {
    pub current: String,
    pub latest: String,
    pub available: bool,
    pub url: String,
    pub sha256: String,
    pub notes: String,
}

#[tauri::command]
pub fn check_updates(endpoint: Option<String>) -> Result<UpdateStatus, String> {
    let settings = manage::settings();
    let endpoint = endpoint
        .filter(|value| !value.trim().is_empty())
        .or(settings.update_url)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "aucune URL de mise à jour configurée".to_string())?;
    let output = Command::new("curl")
        .args(["-fsSL", "--max-time", "15", endpoint.trim()])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("curl indisponible : {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "vérification impossible : {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let manifest: UpdateManifest =
        serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())?;
    let current = petrichor_core::version().to_string();
    let available = is_newer(&manifest.version, &current);
    Ok(UpdateStatus {
        current,
        latest: manifest.version,
        available,
        url: manifest.url,
        sha256: manifest.sha256,
        notes: manifest.notes,
    })
}

#[tauri::command]
pub fn download_update(
    url: String,
    sha256: String,
    filename: Option<String>,
) -> Result<PathBuf, String> {
    let url = url.trim();
    if !(url.starts_with("https://") || url.starts_with("http://") || url.starts_with("file://")) {
        return Err("URL de téléchargement invalide".to_string());
    }
    let name = filename
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| {
            url.rsplit('/')
                .next()
                .filter(|name| !name.is_empty())
                .unwrap_or("petrichor-update")
                .to_string()
        });
    let name = std::path::Path::new(&name)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "nom de fichier invalide".to_string())?;
    let directory = state_dir()?.join("updates");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let target = directory.join(name);
    let temporary = target.with_extension("download");
    let output = Command::new("curl")
        .args(["-fsSL", "--max-time", "120", "-o"])
        .arg(&temporary)
        .arg(url)
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("curl indisponible : {error}"))?;
    if !output.status.success() {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!(
            "téléchargement impossible : {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    if !sha256.trim().is_empty() {
        let bytes = std::fs::read(&temporary).map_err(|error| error.to_string())?;
        let digest = format!("{:x}", Sha256::digest(&bytes));
        if !digest.eq_ignore_ascii_case(sha256.trim()) {
            let _ = std::fs::remove_file(&temporary);
            return Err("empreinte SHA-256 invalide".to_string());
        }
    }
    std::fs::rename(&temporary, &target).map_err(|error| error.to_string())?;
    Ok(target)
}

fn state_dir() -> Result<PathBuf, String> {
    std::env::var_os("PETRICHOR_STATE_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_STATE_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .map(|base| base.join("petrichor"))
        .ok_or_else(|| "répertoire d'état introuvable".to_string())
}

fn version_parts(version: &str) -> Vec<u64> {
    version
        .trim()
        .trim_start_matches('v')
        .split(['.', '-', '+'])
        .take(3)
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

fn is_newer(candidate: &str, current: &str) -> bool {
    let candidate = version_parts(candidate);
    let current = version_parts(current);
    for index in 0..candidate.len().max(current.len()) {
        let left = candidate.get(index).copied().unwrap_or(0);
        let right = current.get(index).copied().unwrap_or(0);
        if left != right {
            return left > right;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
    }

    #[test]
    fn hashes_downloaded_bytes() {
        let digest = format!("{:x}", Sha256::digest(b"petrichor"));
        assert_eq!(
            digest,
            "a5a62bf072ce20efe3791ebb415dcd6b672381e9efb9587da93e1e942a37aa1e"
        );
    }
}
