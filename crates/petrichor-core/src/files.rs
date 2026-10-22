use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

pub fn write_key_value(
    target: &Path,
    allowed_roots: &[PathBuf],
    section: &str,
    key: &str,
    value: &str,
) -> Result<(), String> {
    let section = section.trim();
    let key = key.trim();
    if section.is_empty() || section.contains(['[', ']', '\r', '\n']) {
        return Err("section invalide".to_string());
    }
    if key.is_empty() || key.contains(['=', '\r', '\n']) {
        return Err("clé invalide".to_string());
    }
    if value.contains(['\r', '\n']) {
        return Err("valeur multi-ligne refusée".to_string());
    }
    if target
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("chemin contenant '..' refusé".to_string());
    }
    if target
        .components()
        .any(|component| matches!(component, Component::Prefix(_) | Component::RootDir))
        && target.is_relative()
    {
        return Err("chemin invalide".to_string());
    }

    let allowed: Vec<PathBuf> = allowed_roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .collect();
    if allowed.is_empty() {
        return Err("aucun dossier autorisé pour l'écriture".to_string());
    }

    if target.is_dir() {
        return Err("la cible est un dossier".to_string());
    }
    if target.exists() {
        let metadata = fs::symlink_metadata(target).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            let resolved = target.canonicalize().map_err(|error| error.to_string())?;
            ensure_allowed(&resolved, &allowed)?;
        }
        if metadata.permissions().readonly() {
            return Err("fichier en lecture seule".to_string());
        }
        let resolved = target.canonicalize().map_err(|error| error.to_string())?;
        ensure_allowed(&resolved, &allowed)?;
    } else {
        let parent = target
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let resolved_parent = nearest_existing(parent)
            .and_then(|parent| parent.canonicalize().ok())
            .ok_or_else(|| "dossier cible introuvable".to_string())?;
        ensure_allowed(
            &resolved_parent.join(target.file_name().unwrap_or_default()),
            &allowed,
        )?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let existing = if target.exists() {
        fs::read_to_string(target).map_err(|error| error.to_string())?
    } else {
        String::new()
    };
    let updated = update_ini_text(&existing, section, key, value);
    atomic_write(target, &updated)
}

fn ensure_allowed(path: &Path, roots: &[PathBuf]) -> Result<(), String> {
    if roots.iter().any(|root| path.starts_with(root)) {
        Ok(())
    } else {
        Err(format!(
            "écriture hors périmètre refusée : {}",
            path.display()
        ))
    }
}

fn nearest_existing(path: &Path) -> Option<&Path> {
    let mut current = Some(path);
    while let Some(candidate) = current {
        if candidate.exists() {
            return Some(candidate);
        }
        current = candidate.parent();
    }
    None
}

fn atomic_write(target: &Path, content: &str) -> Result<(), String> {
    let parent = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("petrichor");
    let temporary = parent.join(format!(".{name}.{}.tmp", std::process::id()));

    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        file.write_all(content.as_bytes())
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        if let Ok(metadata) = fs::metadata(target) {
            let _ = fs::set_permissions(&temporary, metadata.permissions());
        }
        fs::rename(&temporary, target).map_err(|error| error.to_string())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn update_ini_text(text: &str, section: &str, key: &str, value: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let section_index = lines.iter().position(|line| {
        line.trim()
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
            .map(|name| name.trim().eq_ignore_ascii_case(section))
            .unwrap_or(false)
    });

    let Some(section_index) = section_index else {
        if !lines.is_empty() && !lines.last().is_some_and(String::is_empty) {
            lines.push(String::new());
        }
        lines.push(format!("[{section}]"));
        lines.push(format!("{key}={value}"));
        return finish(lines, text.ends_with('\n'));
    };

    let end = lines
        .iter()
        .enumerate()
        .skip(section_index + 1)
        .find(|(_, line)| line.trim().starts_with('['))
        .map(|(index, _)| index)
        .unwrap_or(lines.len());

    if let Some(index) = (section_index + 1..end).find(|&index| {
        lines[index]
            .split_once('=')
            .map(|(candidate, _)| candidate.trim().eq_ignore_ascii_case(key))
            .unwrap_or(false)
    }) {
        let original = lines[index]
            .split_once('=')
            .map(|(candidate, _)| candidate.trim())
            .unwrap_or(key);
        lines[index] = format!("{original}={value}");
    } else {
        let insert = (section_index + 1..end)
            .rev()
            .find(|&index| !lines[index].trim().is_empty())
            .map(|index| index + 1)
            .unwrap_or(section_index + 1);
        lines.insert(insert, format!("{key}={value}"));
    }

    finish(lines, text.ends_with('\n'))
}

fn finish(lines: Vec<String>, trailing_newline: bool) -> String {
    let mut output = lines.join("\n");
    if trailing_newline || !output.is_empty() {
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn roots(dir: &Path) -> Vec<PathBuf> {
        vec![dir.to_path_buf()]
    }

    #[test]
    fn updates_existing_key_and_preserves_comments() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("skin.ini");
        fs::write(
            &path,
            "; commentaire\n[Variables]\nFoo=bar\n\n[Other]\nX=1\n",
        )
        .unwrap();

        write_key_value(&path, &roots(dir.path()), "Variables", "Foo", "baz").unwrap();

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("; commentaire"));
        assert!(text.contains("Foo=baz"));
        assert!(text.contains("[Other]\nX=1"));
    }

    #[test]
    fn creates_missing_section_and_key() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("skin.ini");
        fs::write(&path, "[Variables]\nFoo=bar\n").unwrap();

        write_key_value(&path, &roots(dir.path()), "Variables", "New", "value").unwrap();
        write_key_value(&path, &roots(dir.path()), "Extra", "Key", "v").unwrap();

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("Foo=bar\nNew=value"));
        assert!(text.contains("[Extra]\nKey=v"));
    }

    #[test]
    fn rejects_parent_traversal_and_outside_roots() {
        let dir = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let target = dir.path().join("..").join("escape.ini");
        assert!(write_key_value(&target, &roots(dir.path()), "S", "K", "V").is_err());

        let target = outside.path().join("outer.ini");
        assert!(write_key_value(&target, &roots(dir.path()), "S", "K", "V").is_err());
    }

    #[test]
    fn rejects_multiline_values_and_readonly_files() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("skin.ini");
        fs::write(&path, "[S]\nK=V\n").unwrap();
        assert!(write_key_value(&path, &roots(dir.path()), "S", "K", "a\nb").is_err());
        assert!(write_key_value(&path, &roots(dir.path()), "S", "K", "a\rb").is_err());

        let original = fs::metadata(&path).unwrap().permissions();
        let mut permissions = original.clone();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions).unwrap();
        assert!(write_key_value(&path, &roots(dir.path()), "S", "K", "x").is_err());
        fs::set_permissions(&path, original).unwrap();
    }
}
