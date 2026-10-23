use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use petrichor_core::{Encoding, IniFile};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{Emitter, Manager};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::commands;
use crate::layouts;
use crate::updates;
use crate::windows::SkinWindows;

const METADATA_FILE: &str = "Package.ini";
const COMPONENTS: [&str; 5] = ["Skins", "Layouts", "Plugins", "Fonts", "Addons"];
const MAGIC: &[u8] = b"PETRICHOR-PKG-1\n";
const FOOTER: &[u8] = b"\nPETRICHOR-PKG-END\n";
const PROGRESS_EVENT: &str = "petrichor://package-progress";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageMetadata {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub load: String,
    #[serde(default = "default_true")]
    pub merge_skins: bool,
    #[serde(default)]
    pub variable_files: Vec<String>,
    #[serde(default)]
    pub minimum_petrichor: String,
    #[serde(default)]
    pub banner: String,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
pub struct PackageInfo {
    pub path: PathBuf,
    pub metadata: PackageMetadata,
    pub components: BTreeMap<String, usize>,
    pub files: usize,
    pub bytes: u64,
    pub banner_data: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreatePackageSpec {
    pub source: String,
    pub output: String,
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub load: String,
    #[serde(default)]
    pub layouts: Vec<String>,
    #[serde(default)]
    pub include_plugins: bool,
    #[serde(default = "default_true")]
    pub merge_skins: bool,
    #[serde(default)]
    pub variable_files: Vec<String>,
    #[serde(default)]
    pub banner: Option<String>,
    #[serde(default)]
    pub include_fonts: bool,
    #[serde(default)]
    pub include_addons: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackageProgress {
    pub phase: String,
    pub current: usize,
    pub total: usize,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreatePackageResult {
    pub path: PathBuf,
    pub files: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallResult {
    pub metadata: PackageMetadata,
    pub installed: Vec<String>,
    pub files: usize,
    pub layout_loaded: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct PackageRoots {
    skins: PathBuf,
    layouts: PathBuf,
    plugins: PathBuf,
    fonts: PathBuf,
    addons: PathBuf,
}

impl PackageRoots {
    fn from_app() -> Result<Self, String> {
        let config = crate::persistence::config_path()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .ok_or_else(|| "répertoire de configuration introuvable".to_string())?;
        let skins =
            commands::skin_root().ok_or_else(|| "répertoire des skins introuvable".to_string())?;
        let data_root = skins.parent().unwrap_or(&config).to_path_buf();
        Ok(PackageRoots {
            skins,
            layouts: config.join("Layouts"),
            plugins: data_root.join("Plugins"),
            fonts: data_root.join("Fonts"),
            addons: data_root.join("Addons"),
        })
    }

    fn get(&self, component: &str) -> Option<&Path> {
        match component {
            "Skins" => Some(&self.skins),
            "Layouts" => Some(&self.layouts),
            "Plugins" => Some(&self.plugins),
            "Fonts" => Some(&self.fonts),
            "Addons" => Some(&self.addons),
            _ => None,
        }
    }
}

#[tauri::command]
pub fn inspect_package(path: String) -> Result<PackageInfo, String> {
    inspect(Path::new(&path))
}

#[tauri::command]
pub async fn create_package(
    app: tauri::AppHandle,
    spec: CreatePackageSpec,
) -> Result<CreatePackageResult, String> {
    tauri::async_runtime::spawn_blocking(move || create(&spec, Some(&app)))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn install_package(
    app: tauri::AppHandle,
    path: String,
    components: Option<Vec<String>>,
    merge: bool,
    preserve_variables: bool,
) -> Result<InstallResult, String> {
    let roots = PackageRoots::from_app()?;
    let blocking_app = app.clone();
    let mut result = tauri::async_runtime::spawn_blocking(move || {
        install(
            Path::new(&path),
            &roots,
            components.as_deref(),
            merge,
            preserve_variables,
            Some(&blocking_app),
        )
    })
    .await
    .map_err(|error| error.to_string())??;
    if !result.metadata.load.trim().is_empty()
        && result.installed.iter().any(|item| item == "Layouts")
    {
        let windows = app.state::<SkinWindows>();
        match layouts::load(&app, &windows, result.metadata.load.trim()) {
            Ok(_) => result.layout_loaded = Some(result.metadata.load.clone()),
            Err(error) => result.warnings.push(format!(
                "Layout « {} » non chargé : {error}",
                result.metadata.load
            )),
        }
    }
    Ok(result)
}

fn inspect(path: &Path) -> Result<PackageInfo, String> {
    let raw = std::fs::read(path).map_err(|error| error.to_string())?;
    let data = unwrap_package(&raw)?;
    let mut archive = ZipArchive::new(Cursor::new(data)).map_err(|error| error.to_string())?;
    let mut metadata = PackageMetadata::default();
    let mut components: BTreeMap<String, usize> = BTreeMap::new();
    let mut bytes = 0u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|error| error.to_string())?;
        if entry.is_dir() {
            continue;
        }
        bytes += entry.size();
        let name = entry.name().replace('\\', "/");
        if name.eq_ignore_ascii_case(METADATA_FILE) {
            let mut text = String::new();
            let mut entry = entry;
            entry
                .read_to_string(&mut text)
                .map_err(|error| error.to_string())?;
            metadata = metadata_from_ini(&IniFile::parse(&text));
        } else if let Some(component) = name.split('/').next() {
            if COMPONENTS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(component))
            {
                *components
                    .entry(canonical_component(component))
                    .or_insert(0) += 1;
            }
        }
    }
    if components.is_empty() {
        return Err(
            "aucun composant Skins/Layouts/Plugins/Fonts/Addons dans le paquet".to_string(),
        );
    }
    let banner_data = if metadata.banner.trim().is_empty() {
        None
    } else {
        let banner_name = metadata.banner.trim();
        let banner_index = (0..archive.len()).find(|index| {
            archive.by_index(*index).is_ok_and(|entry| {
                !entry.is_dir() && entry.name().eq_ignore_ascii_case(banner_name)
            })
        });
        banner_index.and_then(|index| {
            let mut entry = archive.by_index(index).ok()?;
            if entry.size() > 8 * 1024 * 1024 {
                return None;
            }
            let mut data = Vec::new();
            entry.read_to_end(&mut data).ok()?;
            let mime = image_mime(banner_name);
            Some(format!("data:{mime};base64,{}", BASE64.encode(data)))
        })
    };
    Ok(PackageInfo {
        path: path.to_path_buf(),
        metadata,
        files: components.values().sum(),
        bytes,
        components,
        banner_data,
    })
}

fn image_mime(name: &str) -> &'static str {
    match Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        _ => "image/png",
    }
}

fn create(
    spec: &CreatePackageSpec,
    app: Option<&tauri::AppHandle>,
) -> Result<CreatePackageResult, String> {
    let source = PathBuf::from(spec.source.trim());
    if !source.is_dir() {
        return Err(format!("dossier source introuvable : {}", source.display()));
    }
    let output = PathBuf::from(spec.output.trim());
    if output.as_os_str().is_empty() {
        return Err("fichier de sortie manquant".to_string());
    }
    if spec.name.trim().is_empty() {
        return Err("nom du paquet manquant".to_string());
    }
    let skin_folder = source
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "nom de dossier source invalide".to_string())?;

    let mut files = Vec::new();
    collect_files(
        &source,
        &PathBuf::from("Skins").join(skin_folder),
        &mut files,
    )?;
    for name in &spec.layouts {
        let Some(path) = layouts::layout_path(name) else {
            return Err(format!("layout invalide : {name}"));
        };
        if !path.is_file() {
            return Err(format!("layout introuvable : {name}"));
        }
        let dir = path.parent().unwrap_or(Path::new(""));
        collect_files(dir, &PathBuf::from("Layouts").join(name.trim()), &mut files)?;
    }
    let Some(data_root) =
        commands::skin_root().and_then(|root| root.parent().map(Path::to_path_buf))
    else {
        return Err("répertoire de données introuvable".to_string());
    };
    if spec.include_plugins {
        let plugins = data_root.join("Plugins");
        if plugins.is_dir() {
            collect_files(&plugins, Path::new("Plugins"), &mut files)?;
        }
    }
    if spec.include_fonts {
        let fonts = data_root.join("Fonts");
        if fonts.is_dir() {
            collect_files(&fonts, Path::new("Fonts"), &mut files)?;
        }
    }
    if spec.include_addons {
        let addons = data_root.join("Addons");
        if addons.is_dir() {
            collect_files(&addons, Path::new("Addons"), &mut files)?;
        }
    }

    let mut banner = String::new();
    if let Some(path) = spec
        .banner
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        let path = PathBuf::from(path);
        if !path.is_file() {
            return Err(format!("bannière introuvable : {}", path.display()));
        }
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("png");
        banner = format!("Banner.{extension}");
        files.push((banner.clone(), path));
    }
    if !files.iter().any(|(target, _)| target.starts_with("Skins/")) {
        return Err("aucun fichier de skin à empaqueter".to_string());
    }

    let mut variable_files = spec
        .variable_files
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if variable_files.is_empty() {
        let prefix = format!("Skins/{skin_folder}/");
        variable_files = files
            .iter()
            .filter_map(|(target, _)| target.strip_prefix(&prefix))
            .filter(|target| target.to_lowercase().ends_with(".inc"))
            .map(str::to_string)
            .collect();
    }
    let metadata = PackageMetadata {
        name: spec.name.trim().to_string(),
        author: spec.author.trim().to_string(),
        version: spec.version.trim().to_string(),
        load: spec.load.trim().to_string(),
        merge_skins: spec.merge_skins,
        variable_files,
        minimum_petrichor: petrichor_core::version().to_string(),
        banner,
    };
    files.sort_by(|left, right| left.0.cmp(&right.0));

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temporary = output.with_extension(format!(
        "{}.tmp",
        output
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("zip")
    ));
    let mut buffer = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(&mut buffer);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    let metadata_text = metadata_to_ini(&metadata).render();
    writer
        .start_file(METADATA_FILE, options)
        .map_err(|error| error.to_string())?;
    writer
        .write_all(metadata_text.as_bytes())
        .map_err(|error| error.to_string())?;
    let mut bytes = metadata_text.len() as u64;
    let total = files.len() + 1;
    emit_progress(app, "create", 1, total, "Métadonnées du paquet");
    for (index, (target, source)) in files.iter().enumerate() {
        writer
            .start_file(target, options)
            .map_err(|error| error.to_string())?;
        let content = std::fs::read(source).map_err(|error| error.to_string())?;
        writer
            .write_all(&content)
            .map_err(|error| error.to_string())?;
        bytes += content.len() as u64;
        emit_progress(app, "create", index + 2, total, target);
    }
    writer.finish().map_err(|error| error.to_string())?;
    let data = buffer.into_inner();
    let key = sha256_hex(&data);
    let mut packaged =
        Vec::with_capacity(MAGIC.len() + key.len() + data.len() + FOOTER.len() + key.len() + 1);
    packaged.extend_from_slice(MAGIC);
    packaged.extend_from_slice(key.as_bytes());
    packaged.push(b'\n');
    packaged.extend_from_slice(&data);
    packaged.extend_from_slice(FOOTER);
    packaged.extend_from_slice(key.as_bytes());
    std::fs::write(&temporary, packaged).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, &output).map_err(|error| error.to_string())?;
    emit_progress(app, "create", total, total, "Paquet terminé");
    Ok(CreatePackageResult {
        path: output,
        files: files.len() + 1,
        bytes,
    })
}

fn install(
    path: &Path,
    roots: &PackageRoots,
    components: Option<&[String]>,
    merge: bool,
    preserve_variables: bool,
    app: Option<&tauri::AppHandle>,
) -> Result<InstallResult, String> {
    let info = inspect(path)?;
    check_minimum_version(&info.metadata)?;
    let selected: BTreeSet<String> = match components {
        Some(values) if !values.is_empty() => values
            .iter()
            .map(|value| canonical_component(value))
            .collect(),
        _ => info.components.keys().cloned().collect(),
    };
    let raw = std::fs::read(path).map_err(|error| error.to_string())?;
    let data = unwrap_package(&raw)?;
    let mut archive = ZipArchive::new(Cursor::new(data)).map_err(|error| error.to_string())?;
    let mut installed = BTreeSet::new();
    let mut replaced = BTreeSet::new();
    let mut count = 0usize;
    let total = archive.len();

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let raw_name = entry.name().replace('\\', "/");
        if raw_name.eq_ignore_ascii_case(METADATA_FILE) {
            continue;
        }
        let relative = safe_relative(&raw_name)
            .ok_or_else(|| format!("chemin dangereux dans le paquet : {raw_name}"))?;
        let mut parts = relative.iter();
        let component = parts
            .next()
            .and_then(|part| part.to_str())
            .map(canonical_component)
            .ok_or_else(|| format!("composant invalide dans le paquet : {raw_name}"))?;
        if !selected.contains(&component) {
            continue;
        }
        let root = roots
            .get(&component)
            .ok_or_else(|| format!("composant non pris en charge : {component}"))?;
        let remainder: PathBuf = parts.collect();
        if remainder.as_os_str().is_empty() {
            continue;
        }
        if component == "Skins" && is_reserved_skin_path(&remainder) {
            return Err("le dossier Skins/@Backup est réservé aux sauvegardes".to_string());
        }
        let target = root.join(&remainder);
        let existing =
            if preserve_variables && preserves_variables(&raw_name, &target, &info.metadata) {
                std::fs::read(&target).ok()
            } else {
                None
            };

        if !merge {
            let replace_root = if component == "Skins" || component == "Layouts" {
                remainder
                    .components()
                    .next()
                    .map(|part| root.join(part.as_os_str()))
            } else {
                Some(root.to_path_buf())
            };
            if let Some(replace_root) = replace_root {
                if replaced.insert(replace_root.clone()) && replace_root.exists() {
                    if replace_root.is_dir() {
                        if component == "Skins" {
                            backup_skin(&replace_root, &roots.skins)?;
                        }
                        std::fs::remove_dir_all(&replace_root)
                            .map_err(|error| error.to_string())?;
                    } else {
                        std::fs::remove_file(&replace_root).map_err(|error| error.to_string())?;
                    }
                }
            }
        }

        let mut content = Vec::new();
        entry
            .read_to_end(&mut content)
            .map_err(|error| error.to_string())?;
        if let Some(existing) = existing {
            content = merge_variables(&existing, &content);
        }
        write_atomic(&target, &content)?;
        installed.insert(component);
        count += 1;
        emit_progress(app, "install", index + 1, total, &raw_name);
    }

    Ok(InstallResult {
        metadata: info.metadata,
        installed: installed.into_iter().collect(),
        files: count,
        layout_loaded: None,
        warnings: Vec::new(),
    })
}

fn is_reserved_skin_path(path: &Path) -> bool {
    path.components().next().is_some_and(|component| {
        component
            .as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case("@Backup")
    })
}

fn backup_skin(source: &Path, skins_root: &Path) -> Result<(), String> {
    let Some(name) = source.file_name() else {
        return Ok(());
    };
    if name.to_string_lossy().eq_ignore_ascii_case("@Backup") {
        return Ok(());
    }
    let backup = skins_root.join("@Backup").join(name);
    if backup.exists() {
        if backup.is_dir() {
            std::fs::remove_dir_all(&backup).map_err(|error| error.to_string())?;
        } else {
            std::fs::remove_file(&backup).map_err(|error| error.to_string())?;
        }
    }
    copy_directory(source, &backup)
}

fn copy_directory(source: &Path, target: &Path) -> Result<(), String> {
    std::fs::create_dir_all(target).map_err(|error| error.to_string())?;
    for entry in std::fs::read_dir(source)
        .map_err(|error| error.to_string())?
        .flatten()
    {
        let path = entry.path();
        let destination = target.join(entry.file_name());
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            copy_directory(&path, &destination)?;
        } else if metadata.is_file() {
            std::fs::copy(&path, &destination).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn write_atomic(target: &Path, content: &[u8]) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temporary = target.with_extension(format!(
        "{}.tmp",
        target
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("tmp")
    ));
    std::fs::write(&temporary, content).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, target).map_err(|error| error.to_string())
}

fn check_minimum_version(metadata: &PackageMetadata) -> Result<(), String> {
    let required = metadata.minimum_petrichor.trim();
    if required.is_empty() {
        return Ok(());
    }
    if updates::is_newer(required, petrichor_core::version()) {
        return Err(format!(
            "ce paquet requiert Petrichor {required} ou supérieur (version actuelle : {})",
            petrichor_core::version()
        ));
    }
    Ok(())
}

fn preserves_variables(raw_name: &str, target: &Path, metadata: &PackageMetadata) -> bool {
    if metadata.variable_files.is_empty() {
        return target.extension().is_some_and(|extension| {
            extension.eq_ignore_ascii_case("ini") || extension.eq_ignore_ascii_case("inc")
        });
    }
    let archive_name = normalize_variable_file(raw_name);
    let target_name = target
        .file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    metadata.variable_files.iter().any(|pattern| {
        let pattern = normalize_variable_file(pattern);
        !pattern.is_empty()
            && (archive_name == pattern
                || archive_name.ends_with(&format!("/{pattern}"))
                || target_name == pattern)
    })
}

fn normalize_variable_file(value: &str) -> String {
    value
        .trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_lowercase()
}

fn merge_variables(existing: &[u8], incoming: &[u8]) -> Vec<u8> {
    let encoding = Encoding::detect(existing);
    let existing = encoding.decode(existing);
    let existing = IniFile::parse(&existing);
    let Some(variables) = existing.section("Variables") else {
        return incoming.to_vec();
    };
    let incoming = Encoding::detect(incoming).decode(incoming);
    let mut merged = IniFile::parse(&incoming);
    for (key, value) in variables.iter() {
        merged.set("Variables", key, value);
    }
    merged.render().into_bytes()
}

fn collect_files(
    source: &Path,
    prefix: &Path,
    output: &mut Vec<(String, PathBuf)>,
) -> Result<(), String> {
    let mut entries = std::fs::read_dir(source)
        .map_err(|error| error.to_string())?
        .flatten()
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        let relative = path
            .strip_prefix(source)
            .map_err(|error| error.to_string())?;
        let target = prefix.join(relative);
        if metadata.is_dir() {
            collect_files(&path, &target, output)?;
        } else if metadata.is_file() {
            let name = target
                .iter()
                .map(|part| part.to_str())
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| "nom de fichier non UTF-8".to_string())?
                .join("/");
            output.push((name, path));
        }
    }
    Ok(())
}

fn safe_relative(name: &str) -> Option<PathBuf> {
    let path = Path::new(name);
    if path.is_absolute() {
        return None;
    }
    let mut output = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => output.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!output.as_os_str().is_empty()).then_some(output)
}

fn canonical_component(value: &str) -> String {
    COMPONENTS
        .iter()
        .find(|component| component.eq_ignore_ascii_case(value.trim()))
        .map(|component| (*component).to_string())
        .unwrap_or_else(|| value.trim().to_string())
}

fn metadata_from_ini(ini: &IniFile) -> PackageMetadata {
    let metadata = ini.section("Metadata");
    let options = ini.section("Options");
    let get = |section: Option<&petrichor_core::Section>, key: &str| {
        section
            .and_then(|section| section.get(key))
            .map(str::trim)
            .unwrap_or_default()
            .to_string()
    };
    PackageMetadata {
        name: get(metadata, "Name"),
        author: get(metadata, "Author"),
        version: get(metadata, "Version"),
        load: get(options, "Load"),
        merge_skins: options
            .and_then(|section| section.get("MergeSkins"))
            .map(|value| matches!(value.trim(), "1" | "true" | "TRUE"))
            .unwrap_or(true),
        variable_files: get(options, "VariableFiles")
            .split(['|', ';'])
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect(),
        minimum_petrichor: get(options, "MinimumPetrichor"),
        banner: get(metadata, "Banner"),
    }
}

fn metadata_to_ini(metadata: &PackageMetadata) -> IniFile {
    let mut ini = IniFile::new();
    ini.set("Metadata", "Name", &metadata.name);
    ini.set("Metadata", "Author", &metadata.author);
    ini.set("Metadata", "Version", &metadata.version);
    ini.set("Metadata", "LoadType", "Skins");
    ini.set("Options", "Load", &metadata.load);
    ini.set(
        "Options",
        "MergeSkins",
        if metadata.merge_skins { "1" } else { "0" },
    );
    ini.set(
        "Options",
        "VariableFiles",
        &metadata.variable_files.join("|"),
    );
    ini.set("Options", "MinimumPetrichor", &metadata.minimum_petrichor);
    if !metadata.banner.is_empty() {
        ini.set("Metadata", "Banner", &metadata.banner);
    }
    ini
}

fn emit_progress(
    app: Option<&tauri::AppHandle>,
    phase: &str,
    current: usize,
    total: usize,
    message: &str,
) {
    if let Some(app) = app {
        let _ = app.emit(
            PROGRESS_EVENT,
            PackageProgress {
                phase: phase.to_string(),
                current,
                total,
                message: message.to_string(),
            },
        );
    }
}

fn sha256_hex(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

fn unwrap_package(raw: &[u8]) -> Result<Vec<u8>, String> {
    if !raw.starts_with(MAGIC) {
        return Ok(raw.to_vec());
    }
    let rest = &raw[MAGIC.len()..];
    let key_end = rest
        .iter()
        .position(|byte| *byte == b'\n')
        .ok_or_else(|| "en-tête de paquet invalide".to_string())?;
    let header_key =
        std::str::from_utf8(&rest[..key_end]).map_err(|_| "clé de paquet invalide".to_string())?;
    let body = &rest[key_end + 1..];
    if body.len() < FOOTER.len() + 64 {
        return Err("paquet tronqué".to_string());
    }
    let footer_start = body.len() - FOOTER.len() - 64;
    if !body[footer_start..].starts_with(FOOTER) {
        return Err("pied de paquet invalide".to_string());
    }
    let footer_key = &body[footer_start + FOOTER.len()..];
    if header_key.as_bytes() != footer_key {
        return Err("clés de validation incohérentes".to_string());
    }
    let data = &body[..footer_start];
    if sha256_hex(data).as_bytes() != footer_key {
        return Err("clé de validation du paquet invalide".to_string());
    }
    Ok(data.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("petrichor-package-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn roots(base: &Path) -> PackageRoots {
        PackageRoots {
            skins: base.join("Skins"),
            layouts: base.join("Layouts"),
            plugins: base.join("Plugins"),
            fonts: base.join("Fonts"),
            addons: base.join("Addons"),
        }
    }

    #[test]
    fn creates_inspects_and_reinstalls_package() {
        let base = temp("cycle");
        let skin = base.join("Skins").join("Clock");
        std::fs::create_dir_all(skin.join("@Resources")).unwrap();
        std::fs::write(
            skin.join("Clock.ini"),
            "[Variables]\nColor=255,255,255\n[MeterTime]\nMeter=String\nText=Hello\n",
        )
        .unwrap();
        std::fs::write(skin.join("@Resources").join("note.txt"), "resource").unwrap();
        let archive = base.join("Clock.zip");
        let spec = CreatePackageSpec {
            source: skin.to_string_lossy().to_string(),
            output: archive.to_string_lossy().to_string(),
            name: "Clock".to_string(),
            author: "Alice".to_string(),
            version: "1.0".to_string(),
            load: String::new(),
            layouts: Vec::new(),
            include_plugins: false,
            merge_skins: true,
            variable_files: Vec::new(),
            banner: None,
            include_fonts: false,
            include_addons: false,
        };
        create(&spec, None).unwrap();
        let info = inspect(&archive).unwrap();
        assert_eq!(info.metadata.name, "Clock");
        assert_eq!(info.components.get("Skins"), Some(&2));

        let install_root = temp("install");
        let target = roots(&install_root);
        let result = install(&archive, &target, None, true, true, None).unwrap();
        assert_eq!(result.files, 2);
        assert!(target.skins.join("Clock/Clock.ini").is_file());
        assert!(target.skins.join("Clock/@Resources/note.txt").is_file());

        std::fs::remove_dir_all(base).ok();
        std::fs::remove_dir_all(install_root).ok();
    }

    #[test]
    fn roundtrips_metadata_options() {
        let metadata = PackageMetadata {
            name: "Suite".to_string(),
            author: "Alice".to_string(),
            version: "2.0".to_string(),
            load: "@Backup".to_string(),
            merge_skins: false,
            variable_files: vec!["Variables.inc".to_string(), "Theme.inc".to_string()],
            minimum_petrichor: "0.1".to_string(),
            banner: "Banner.png".to_string(),
        };
        let parsed = metadata_from_ini(&metadata_to_ini(&metadata));
        assert_eq!(parsed.name, "Suite");
        assert!(!parsed.merge_skins);
        assert_eq!(parsed.variable_files, ["Variables.inc", "Theme.inc"]);
        assert_eq!(parsed.banner, "Banner.png");
    }

    #[test]
    fn creates_non_ascii_banner_and_variable_package() {
        let base = temp("unicode");
        let skin = base.join("Skins").join("Horloge Été");
        std::fs::create_dir_all(&skin).unwrap();
        std::fs::write(
            skin.join("Horloge Été.ini"),
            "[Variables]\nColor=0,0,0\n[MeterTime]\nMeter=String\n",
        )
        .unwrap();
        std::fs::write(skin.join("Variables.inc"), "[Variables]\nColor=0,0,0\n").unwrap();
        std::fs::write(skin.join("Theme.inc"), "[Variables]\nTheme=dark\n").unwrap();
        let banner = base.join("banner.png");
        std::fs::write(&banner, b"\x89PNG\r\n\x1a\n").unwrap();
        let archive = base.join("Horloge.zip");
        let spec = CreatePackageSpec {
            source: skin.to_string_lossy().to_string(),
            output: archive.to_string_lossy().to_string(),
            name: "Horloge Été".to_string(),
            author: "Alice".to_string(),
            version: "1.0".to_string(),
            load: String::new(),
            layouts: Vec::new(),
            include_plugins: false,
            merge_skins: true,
            variable_files: vec!["Variables.inc".to_string()],
            banner: Some(banner.to_string_lossy().to_string()),
            include_fonts: false,
            include_addons: false,
        };
        create(&spec, None).unwrap();
        let info = inspect(&archive).unwrap();
        assert_eq!(info.metadata.name, "Horloge Été");
        assert_eq!(info.metadata.variable_files, ["Variables.inc"]);
        assert!(info
            .banner_data
            .as_deref()
            .is_some_and(|data| data.starts_with("data:image/png;base64,")));

        let install_root = temp("unicode-install");
        let target = roots(&install_root);
        let existing_skin = target.skins.join("Horloge Été");
        std::fs::create_dir_all(&existing_skin).unwrap();
        std::fs::write(
            existing_skin.join("Horloge Été.ini"),
            "[Variables]\nColor=9,9,9\n",
        )
        .unwrap();
        std::fs::write(
            existing_skin.join("Variables.inc"),
            "[Variables]\nColor=1,2,3\n",
        )
        .unwrap();
        std::fs::write(
            existing_skin.join("Theme.inc"),
            "[Variables]\nTheme=light\n",
        )
        .unwrap();
        install(&archive, &target, None, true, true, None).unwrap();
        let ini = std::fs::read_to_string(existing_skin.join("Horloge Été.ini")).unwrap();
        let variables = std::fs::read_to_string(existing_skin.join("Variables.inc")).unwrap();
        let theme = std::fs::read_to_string(existing_skin.join("Theme.inc")).unwrap();
        assert!(ini.contains("Color=0,0,0"));
        assert!(variables.contains("Color=1,2,3"));
        assert!(theme.contains("Theme=dark"));

        std::fs::remove_dir_all(base).ok();
        std::fs::remove_dir_all(install_root).ok();
    }

    #[test]
    fn detects_variable_files_in_skin() {
        let directory = temp("detect-variables");
        let skin = directory.join("Skin");
        std::fs::create_dir_all(&skin).unwrap();
        std::fs::write(skin.join("Skin.ini"), "[Meter]\nMeter=String\n").unwrap();
        std::fs::write(skin.join("Variables.inc"), "[Variables]\nColor=1,2,3\n").unwrap();
        let archive = directory.join("Skin.zip");
        let spec = CreatePackageSpec {
            source: skin.to_string_lossy().to_string(),
            output: archive.to_string_lossy().to_string(),
            name: "Skin".to_string(),
            author: String::new(),
            version: String::new(),
            load: String::new(),
            layouts: Vec::new(),
            include_plugins: false,
            merge_skins: true,
            variable_files: Vec::new(),
            banner: None,
            include_fonts: false,
            include_addons: false,
        };
        create(&spec, None).unwrap();
        let info = inspect(&archive).unwrap();
        assert_eq!(info.metadata.variable_files, ["Variables.inc"]);
        std::fs::remove_dir_all(directory).ok();
    }

    #[test]
    fn accepts_legacy_plain_zip() {
        let directory = temp("legacy");
        let skin = directory.join("Skin");
        std::fs::create_dir_all(&skin).unwrap();
        std::fs::write(skin.join("Skin.ini"), "[Meter]\nMeter=String\n").unwrap();
        let archive = directory.join("Skin.zip");
        let spec = CreatePackageSpec {
            source: skin.to_string_lossy().to_string(),
            output: archive.to_string_lossy().to_string(),
            name: "Skin".to_string(),
            author: String::new(),
            version: String::new(),
            load: String::new(),
            layouts: Vec::new(),
            include_plugins: false,
            merge_skins: true,
            variable_files: Vec::new(),
            banner: None,
            include_fonts: false,
            include_addons: false,
        };
        create(&spec, None).unwrap();
        let raw = std::fs::read(&archive).unwrap();
        let data = unwrap_package(&raw).unwrap();
        std::fs::write(&archive, data).unwrap();
        let info = inspect(&archive).unwrap();
        assert_eq!(info.metadata.name, "Skin");
        assert_eq!(info.components.get("Skins"), Some(&1));
        std::fs::remove_dir_all(directory).ok();
    }

    #[test]
    fn preserves_variables_in_utf16() {
        let incoming = "[Variables]\nColor=0,0,0\n[Meter]\nMeter=String\n";
        let mut existing = vec![0xFF, 0xFE];
        for unit in "[Variables]\nColor=1,2,3\n".encode_utf16() {
            existing.extend_from_slice(&unit.to_le_bytes());
        }
        let merged = String::from_utf8(merge_variables(&existing, incoming.as_bytes())).unwrap();
        assert!(merged.contains("Color=1,2,3"));
    }

    #[test]
    fn rejects_package_requiring_newer_version() {
        let metadata = PackageMetadata {
            minimum_petrichor: "99.0.0".to_string(),
            ..PackageMetadata::default()
        };
        assert!(check_minimum_version(&metadata).is_err());
    }

    #[test]
    fn backs_up_replaced_skins() {
        let base = temp("backup");
        let skin = base.join("Skins").join("Clock");
        std::fs::create_dir_all(&skin).unwrap();
        std::fs::write(skin.join("Clock.ini"), "[Meter]\nMeter=String\n").unwrap();
        let archive = base.join("Clock.zip");
        let spec = CreatePackageSpec {
            source: skin.to_string_lossy().to_string(),
            output: archive.to_string_lossy().to_string(),
            name: "Clock".to_string(),
            author: String::new(),
            version: String::new(),
            load: String::new(),
            layouts: Vec::new(),
            include_plugins: false,
            merge_skins: true,
            variable_files: Vec::new(),
            banner: None,
            include_fonts: false,
            include_addons: false,
        };
        create(&spec, None).unwrap();

        let install_root = temp("backup-install");
        let target = roots(&install_root);
        let existing = target.skins.join("Clock");
        std::fs::create_dir_all(&existing).unwrap();
        std::fs::write(existing.join("Clock.ini"), "[Meter]\nMeter=Old\n").unwrap();
        install(&archive, &target, None, false, false, None).unwrap();
        let backup = target.skins.join("@Backup/Clock/Clock.ini");
        assert!(backup.is_file());
        assert!(std::fs::read_to_string(backup)
            .unwrap()
            .contains("Meter=Old"));
        assert!(std::fs::read_to_string(existing.join("Clock.ini"))
            .unwrap()
            .contains("Meter=String"));
        std::fs::remove_dir_all(base).ok();
        std::fs::remove_dir_all(install_root).ok();
    }

    #[test]
    fn rejects_tampered_envelope() {
        let directory = temp("envelope");
        let archive = directory.join("Clock.zip");
        let skin = directory.join("Clock");
        std::fs::create_dir_all(&skin).unwrap();
        std::fs::write(skin.join("Clock.ini"), "[Meter]\nMeter=String\n").unwrap();
        let spec = CreatePackageSpec {
            source: skin.to_string_lossy().to_string(),
            output: archive.to_string_lossy().to_string(),
            name: "Clock".to_string(),
            author: String::new(),
            version: String::new(),
            load: String::new(),
            layouts: Vec::new(),
            include_plugins: false,
            merge_skins: true,
            variable_files: Vec::new(),
            banner: None,
            include_fonts: false,
            include_addons: false,
        };
        create(&spec, None).unwrap();
        let mut raw = std::fs::read(&archive).unwrap();
        let position = raw.len() / 2;
        raw[position] ^= 0xff;
        std::fs::write(&archive, raw).unwrap();
        assert!(inspect(&archive).is_err());
        std::fs::remove_dir_all(directory).ok();
    }

    #[test]
    fn preserves_existing_variables() {
        let incoming = "[Variables]\nColor=0,0,0\nNew=1\n[Meter]\nMeter=String\n";
        let directory = temp("variables");
        let existing = directory.join("Skin.ini");
        std::fs::write(&existing, "[Variables]\nColor=1,2,3\n").unwrap();
        let existing = std::fs::read(&existing).unwrap();
        let merged = String::from_utf8(merge_variables(&existing, incoming.as_bytes())).unwrap();
        assert!(merged.contains("Color=1,2,3"));
        assert!(merged.contains("New=1"));
        std::fs::remove_dir_all(directory).ok();
    }

    #[test]
    fn reserves_backup_skin_destination() {
        assert!(is_reserved_skin_path(Path::new("@Backup/Clock/Clock.ini")));
        assert!(!is_reserved_skin_path(Path::new("Clock/@Backup/data.inc")));
    }

    #[test]
    fn rejects_unsafe_archive_paths() {
        assert!(safe_relative("Skins/Clock/Clock.ini").is_some());
        assert!(safe_relative("../evil").is_none());
        assert!(safe_relative("/etc/passwd").is_none());
        assert!(safe_relative("Skins/../../evil").is_none());
    }
}
