use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

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
    pub pending: bool,
    pub pending_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingUpdate {
    path: PathBuf,
    #[serde(default)]
    version: String,
    #[serde(default)]
    sha256: String,
    #[serde(default)]
    attempted: bool,
}

#[tauri::command]
pub fn check_updates(endpoint: Option<String>) -> Result<UpdateStatus, String> {
    let settings = manage::settings();
    let endpoint = endpoint
        .filter(|value| !value.trim().is_empty())
        .or(settings.update_url)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "aucune URL de mise à jour configurée".to_string())?;
    let manifest = fetch_manifest(&endpoint)?;
    let current = petrichor_core::version().to_string();
    let available = is_newer(&manifest.version, &current);
    let pending = read_pending();
    Ok(UpdateStatus {
        current,
        latest: manifest.version,
        available,
        url: manifest.url,
        sha256: manifest.sha256,
        notes: manifest.notes,
        pending: pending.is_some(),
        pending_path: pending.map(|pending| pending.path),
    })
}

#[tauri::command]
pub fn download_update(
    url: String,
    sha256: String,
    filename: Option<String>,
    version: Option<String>,
) -> Result<PathBuf, String> {
    let target = download(&url, &sha256, filename)?;
    write_pending(&PendingUpdate {
        path: target.clone(),
        version: version.unwrap_or_default(),
        sha256,
        attempted: false,
    })?;
    Ok(target)
}

#[tauri::command]
pub fn install_update(path: Option<String>) -> Result<String, String> {
    let pending = read_pending();
    let target = match path.filter(|value| !value.trim().is_empty()) {
        Some(path) => PathBuf::from(path),
        None => pending
            .as_ref()
            .map(|pending| pending.path.clone())
            .ok_or_else(|| "aucune mise à jour en attente".to_string())?,
    };
    if !target.is_file() {
        return Err(format!("mise à jour introuvable : {}", target.display()));
    }
    if let Some(pending) = &pending {
        if pending.path == target && !pending.sha256.trim().is_empty() {
            verify_sha256(&target, &pending.sha256)?;
        }
    }
    let message = apply_update(&target)?;
    if pending.is_some_and(|pending| pending.path == target) {
        let _ = clear_pending();
    }
    Ok(message)
}

pub fn spawn() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(4));
        install_pending();
        auto_check();
    });
}

fn install_pending() {
    let settings = manage::settings();
    if settings.disable_auto_update {
        return;
    }
    let Some(mut pending) = read_pending() else {
        return;
    };
    if pending.attempted {
        return;
    }
    pending.attempted = true;
    let _ = write_pending(&pending);
    let extension = extension(&pending.path);
    if extension != "deb" && extension != "rpm" {
        petrichor_core::logging::notice(format!(
            "Mise à jour {} prête, installation manuelle requise",
            pending.path.display()
        ));
        crate::notifications::send(
            "Petrichor",
            "Mise à jour téléchargée ; installez-la depuis les réglages",
        );
        return;
    }
    match apply_update(&pending.path) {
        Ok(_) => {
            let _ = clear_pending();
            petrichor_core::logging::notice(format!(
                "Mise à jour {} installée, redémarrage recommandé",
                pending.version
            ));
            crate::notifications::send("Petrichor", "Mise à jour installée ; redémarrez Petrichor");
        }
        Err(error) => petrichor_core::logging::warning(format!(
            "Installation automatique de la mise à jour impossible : {error}"
        )),
    }
}

fn auto_check() {
    let settings = manage::settings();
    if settings.disable_version_check {
        return;
    }
    let Some(endpoint) = settings.update_url.filter(|value| !value.trim().is_empty()) else {
        return;
    };
    let manifest = match fetch_manifest(&endpoint) {
        Ok(manifest) => manifest,
        Err(error) => {
            petrichor_core::logging::debug(format!("Vérification de mise à jour : {error}"));
            return;
        }
    };
    let current = petrichor_core::version().to_string();
    if !is_newer(&manifest.version, &current) {
        return;
    }
    crate::notifications::send(
        "Petrichor",
        &format!("Mise à jour {} disponible", manifest.version),
    );
    if settings.disable_auto_update {
        petrichor_core::logging::notice(format!(
            "Mise à jour {} disponible (téléchargement automatique désactivé)",
            manifest.version
        ));
        return;
    }
    let filename = filename_from_url(&manifest.url, &manifest.version);
    match download(&manifest.url, &manifest.sha256, Some(filename)) {
        Ok(path) => {
            let pending = PendingUpdate {
                path,
                version: manifest.version.clone(),
                sha256: manifest.sha256,
                attempted: false,
            };
            if let Err(error) = write_pending(&pending) {
                petrichor_core::logging::warning(format!(
                    "Mise à jour téléchargée mais non planifiée : {error}"
                ));
                return;
            }
            crate::notifications::send(
                "Petrichor",
                &format!(
                    "Mise à jour {} téléchargée ; elle sera installée au prochain démarrage",
                    manifest.version
                ),
            );
        }
        Err(error) => petrichor_core::logging::warning(format!(
            "Téléchargement de la mise à jour impossible : {error}"
        )),
    }
}

fn fetch_manifest(endpoint: &str) -> Result<UpdateManifest, String> {
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
    serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())
}

fn download(url: &str, sha256: &str, filename: Option<String>) -> Result<PathBuf, String> {
    let url = url.trim();
    if !(url.starts_with("https://") || url.starts_with("http://") || url.starts_with("file://")) {
        return Err("URL de téléchargement invalide".to_string());
    }
    let name = filename
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| filename_from_url(url, ""));
    let name = Path::new(&name)
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
        if let Err(error) = verify_sha256(&temporary, sha256) {
            let _ = std::fs::remove_file(&temporary);
            return Err(error);
        }
    }
    std::fs::rename(&temporary, &target).map_err(|error| error.to_string())?;
    Ok(target)
}

fn verify_sha256(path: &Path, expected: &str) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    if digest.eq_ignore_ascii_case(expected.trim()) {
        Ok(())
    } else {
        Err("empreinte SHA-256 invalide".to_string())
    }
}

fn apply_update(path: &Path) -> Result<String, String> {
    match extension(path).as_str() {
        "deb" => {
            run_privileged("dpkg", &["-i", &path.to_string_lossy()])?;
            Ok("Mise à jour installée ; redémarrez Petrichor.".to_string())
        }
        "rpm" => {
            run_privileged("rpm", &["-U", "--replacepkgs", &path.to_string_lossy()])?;
            Ok("Mise à jour installée ; redémarrez Petrichor.".to_string())
        }
        "appimage" => {
            let mut permissions = std::fs::metadata(path)
                .map_err(|error| error.to_string())?
                .permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(path, permissions).map_err(|error| error.to_string())?;
            Command::new("xdg-open")
                .arg(path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|error| format!("impossible de lancer la mise à jour : {error}"))?;
            Ok("Nouvelle version lancée.".to_string())
        }
        _ => {
            Command::new("xdg-open")
                .arg(path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|error| format!("impossible d'ouvrir la mise à jour : {error}"))?;
            Ok("Programme d'installation lancé.".to_string())
        }
    }
}

fn run_privileged(program: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new("pkexec")
        .arg(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .status()
        .map_err(|error| format!("pkexec indisponible : {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("installation refusée ou échouée".to_string())
    }
}

fn pending_file() -> Result<PathBuf, String> {
    Ok(state_dir()?.join("updates").join("pending.json"))
}

fn read_pending() -> Option<PendingUpdate> {
    let path = pending_file().ok()?;
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_pending(pending: &PendingUpdate) -> Result<(), String> {
    let path = pending_file()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string_pretty(pending).map_err(|error| error.to_string())?;
    std::fs::write(path, text).map_err(|error| error.to_string())
}

fn clear_pending() -> Result<(), String> {
    let path = pending_file()?;
    if path.is_file() {
        std::fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn filename_from_url(url: &str, version: &str) -> String {
    url.rsplit('/')
        .next()
        .map(|name| name.split(['?', '#']).next().unwrap_or(name))
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            let suffix = if version.trim().is_empty() {
                String::new()
            } else {
                format!("-{}", version.trim())
            };
            format!("petrichor-update{suffix}.package")
        })
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_lowercase()
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

    #[test]
    fn extracts_filename_from_url() {
        assert_eq!(
            filename_from_url("https://example.test/petrichor-1.2.deb?token=abc", ""),
            "petrichor-1.2.deb"
        );
        assert_eq!(
            filename_from_url("https://example.test/download", "1.2.0"),
            "download"
        );
    }
}
