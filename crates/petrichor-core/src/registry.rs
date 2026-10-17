use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkinFile {
    pub filename: String,
    pub path: PathBuf,
    pub is_favorite: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkinFolder {
    pub name: String,
    pub path: PathBuf,
    pub level: u16,
    pub files: Vec<SkinFile>,
    pub base_id: u32,
    pub active_variant: i16,
    pub has_favorite: bool,
}

impl SkinFolder {
    pub fn file(&self, filename: &str) -> Option<&SkinFile> {
        self.files
            .iter()
            .find(|f| f.filename.eq_ignore_ascii_case(filename))
    }

    pub fn active_file(&self) -> Option<&SkinFile> {
        if self.active_variant >= 0 {
            if let Some(file) = self.files.get(self.active_variant as usize) {
                return Some(file);
            }
        }
        self.files.first()
    }

    pub fn inactive_files(&self) -> impl Iterator<Item = &SkinFile> {
        let active = self.active_variant.max(0) as usize;
        self.files.iter().enumerate().filter_map(
            move |(i, f)| {
                if i == active {
                    None
                } else {
                    Some(f)
                }
            },
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Registry {
    pub root: PathBuf,
    pub folders: Vec<SkinFolder>,
}

impl Registry {
    pub fn scan(root: impl AsRef<Path>) -> Registry {
        let root = root.as_ref().to_path_buf();
        let mut folders: Vec<SkinFolder> = Vec::new();
        if root.is_dir() {
            collect_folders(&root, 1, &mut folders);
        }

        let mut keep: Vec<bool> = folders.iter().map(|f| !f.files.is_empty()).collect();
        for i in (0..folders.len()).rev() {
            if keep[i] {
                let child = folders[i].path.clone();
                for j in 0..i {
                    if child != folders[j].path && child.starts_with(&folders[j].path) {
                        keep[j] = true;
                    }
                }
            }
        }

        let mut next_id = 0u32;
        let mut kept = Vec::new();
        for (index, mut folder) in folders.into_iter().enumerate() {
            if keep[index] {
                folder.base_id = next_id;
                next_id += 1;
                kept.push(folder);
            }
        }

        Registry {
            root,
            folders: kept,
        }
    }

    pub fn folder(&self, path: impl AsRef<Path>) -> Option<&SkinFolder> {
        let target = normalize(path.as_ref());
        self.folders.iter().find(|f| normalize(&f.path) == target)
    }

    pub fn folder_by_name(&self, name: &str) -> Option<&SkinFolder> {
        self.folders
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(name))
    }

    pub fn is_empty(&self) -> bool {
        self.folders.is_empty()
    }

    pub fn len(&self) -> usize {
        self.folders.len()
    }

    pub fn file_count(&self) -> usize {
        self.folders.iter().map(|f| f.files.len()).sum()
    }

    pub fn set_favorite(&mut self, path: impl AsRef<Path>, filename: &str, favorite: bool) {
        let target = normalize(path.as_ref());
        if let Some(folder) = self
            .folders
            .iter_mut()
            .find(|f| normalize(&f.path) == target)
        {
            if let Some(file) = folder
                .files
                .iter_mut()
                .find(|f| f.filename.eq_ignore_ascii_case(filename))
            {
                file.is_favorite = favorite;
            }
            folder.has_favorite = folder.files.iter().any(|f| f.is_favorite);
        }
    }
}

fn collect_folders(dir: &Path, level: u16, out: &mut Vec<SkinFolder>) {
    let mut entries: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(read_dir) => read_dir.flatten().map(|e| e.path()).collect(),
        Err(_) => return,
    };
    entries.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));

    for path in entries {
        if path.is_dir() {
            out.push(build_folder(&path, level));
            collect_folders(&path, level + 1, out);
        }
    }
}

fn build_folder(dir: &Path, level: u16) -> SkinFolder {
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let mut files: Vec<SkinFile> = std::fs::read_dir(dir)
        .map(|read_dir| {
            read_dir
                .flatten()
                .map(|e| e.path())
                .filter(|p| is_ini_file(p))
                .map(|path| SkinFile {
                    filename: path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    path,
                    is_favorite: false,
                })
                .collect()
        })
        .unwrap_or_default();

    files.sort_by_key(|f| f.filename.to_lowercase());

    SkinFolder {
        name,
        path: dir.to_path_buf(),
        level,
        files,
        base_id: 0,
        active_variant: 0,
        has_favorite: false,
    }
}

fn is_ini_file(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .map(|e| e.eq_ignore_ascii_case("ini"))
            .unwrap_or(false)
}

pub fn normalize(path: &Path) -> String {
    let mut out = String::new();
    for component in path.components() {
        use std::path::Component;
        match component {
            Component::RootDir => out.push('/'),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.ends_with('/') {
                    out.push('/');
                }
                out.push_str("..");
            }
            Component::Normal(part) => {
                if !out.is_empty() && !out.ends_with('/') {
                    out.push('/');
                }
                out.push_str(&part.to_string_lossy());
            }
            Component::Prefix(_) => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("Clock")).unwrap();
        fs::write(root.join("Clock/Clock.ini"), "[S]\nMeter=String\n").unwrap();
        fs::write(root.join("Clock/ClockDark.ini"), "[S]\nMeter=String\n").unwrap();
        fs::create_dir_all(root.join("System/Monitor")).unwrap();
        fs::write(root.join("System/Monitor/CPU.ini"), "[S]\nMeasure=CPU\n").unwrap();
        fs::create_dir_all(root.join("Empty")).unwrap();
        fs::write(root.join("README.txt"), "ignore me").unwrap();
        dir
    }

    #[test]
    fn scans_folders_in_preorder() {
        let dir = fixture();
        let registry = Registry::scan(dir.path());
        let names: Vec<&str> = registry.folders.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["Clock", "System", "Monitor"]);
    }

    #[test]
    fn assigns_levels_and_ids() {
        let dir = fixture();
        let registry = Registry::scan(dir.path());
        assert_eq!(registry.folder_by_name("Clock").unwrap().level, 1);
        assert_eq!(registry.folder_by_name("Monitor").unwrap().level, 2);
        let ids: Vec<u32> = registry.folders.iter().map(|f| f.base_id).collect();
        assert_eq!(ids, vec![0, 1, 2]);
    }

    #[test]
    fn lists_only_ini_files() {
        let dir = fixture();
        let registry = Registry::scan(dir.path());
        let clock = registry.folder_by_name("Clock").unwrap();
        let files: Vec<&str> = clock.files.iter().map(|f| f.filename.as_str()).collect();
        assert_eq!(files, vec!["Clock.ini", "ClockDark.ini"]);
    }

    #[test]
    fn skips_folders_without_ini() {
        let dir = fixture();
        let registry = Registry::scan(dir.path());
        assert!(registry.folder_by_name("Empty").is_none());
        assert_eq!(registry.len(), 3);
        assert_eq!(registry.file_count(), 3);
    }

    #[test]
    fn finds_folder_case_insensitively() {
        let dir = fixture();
        let registry = Registry::scan(dir.path());
        assert!(registry.folder_by_name("clock").is_some());
    }

    #[test]
    fn toggles_favorites() {
        let dir = fixture();
        let mut registry = Registry::scan(dir.path());
        registry.set_favorite(dir.path().join("Clock"), "clock.ini", true);
        let clock = registry.folder_by_name("Clock").unwrap();
        assert!(clock.file("Clock.ini").unwrap().is_favorite);
        assert!(clock.has_favorite);
    }

    #[test]
    fn normalizes_paths() {
        assert_eq!(normalize(Path::new("/a/b/../c/./d")), "/a/b/../c/d");
    }
}
