use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::diagnostics::Diagnostics;
use crate::ini::{IniDocument, IniSection};

pub const MAX_INCLUDE_DEPTH: usize = 100;

#[derive(Debug, Clone, Default)]
pub struct IncludeResult {
    pub document: IniDocument,
    pub files: Vec<PathBuf>,
    pub diagnostics: Diagnostics,
}

pub fn resolve(entry: &Path, skin_root: &Path) -> IncludeResult {
    let mut result = IncludeResult::default();
    let mut stack: Vec<PathBuf> = Vec::new();
    let mut variables: BTreeMap<String, String> = BTreeMap::new();
    merge_file(entry, skin_root, 0, &mut stack, &mut variables, &mut result);
    result
}

#[allow(clippy::too_many_arguments)]
fn merge_file(
    path: &Path,
    skin_root: &Path,
    depth: usize,
    stack: &mut Vec<PathBuf>,
    variables: &mut BTreeMap<String, String>,
    result: &mut IncludeResult,
) {
    if depth > MAX_INCLUDE_DEPTH {
        result.diagnostics.push(
            crate::diagnostics::Diagnostic::error(format!(
                "profondeur d'inclusion maximale ({MAX_INCLUDE_DEPTH}) dépassée"
            ))
            .with_file(path.to_path_buf()),
        );
        return;
    }

    let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if stack.contains(&resolved) {
        result.diagnostics.push(
            crate::diagnostics::Diagnostic::error(format!(
                "boucle d'inclusion détectée: {}",
                path.display()
            ))
            .with_file(path.to_path_buf()),
        );
        return;
    }

    let document = match IniDocument::load(path) {
        Ok(document) => document,
        Err(error) => {
            result.diagnostics.push(
                crate::diagnostics::Diagnostic::error(error.to_string())
                    .with_file(path.to_path_buf()),
            );
            return;
        }
    };

    if !result.files.iter().any(|f| f == path) {
        result.files.push(path.to_path_buf());
    }

    collect_variables(&document, variables);

    stack.push(resolved);
    merge_sections(&mut result.document, &document);

    let includes = collect_includes(&document);
    for raw in includes {
        let expanded = expand_string(&raw, variables);
        let include_path = resolve_path(&expanded, skin_root);
        merge_file(
            &include_path,
            skin_root,
            depth + 1,
            stack,
            variables,
            result,
        );
    }
    stack.pop();
}

fn collect_includes(document: &IniDocument) -> Vec<String> {
    let mut includes = Vec::new();
    for section in &document.sections {
        for entry in &section.entries {
            if is_include_key(&entry.key) && !entry.value.trim().is_empty() {
                includes.push(entry.value.trim().to_string());
            }
        }
    }
    includes
}

fn is_include_key(key: &str) -> bool {
    key.trim().to_ascii_lowercase().starts_with("@include")
}

fn collect_variables(document: &IniDocument, variables: &mut BTreeMap<String, String>) {
    if let Some(section) = document.section("Variables") {
        for entry in &section.entries {
            variables
                .entry(entry.key.to_ascii_lowercase())
                .or_insert_with(|| entry.value.clone());
        }
    }
}

fn merge_sections(target: &mut IniDocument, source: &IniDocument) {
    for section in &source.sections {
        if section.name.is_empty() {
            continue;
        }
        match target
            .sections
            .iter()
            .position(|s| s.name.eq_ignore_ascii_case(&section.name))
        {
            Some(index) => {
                let existing = &mut target.sections[index];
                for entry in &section.entries {
                    if is_include_key(&entry.key) {
                        continue;
                    }
                    if existing.entry(&entry.key).is_none() {
                        existing.entries.push(entry.clone());
                    }
                }
            }
            None => {
                let mut clone: IniSection = section.clone();
                clone.entries.retain(|e| !is_include_key(&e.key));
                if clone.entries.is_empty() {
                    clone.entries = section
                        .entries
                        .iter()
                        .filter(|e| !is_include_key(&e.key))
                        .cloned()
                        .collect();
                }
                target.sections.push(clone);
            }
        }
    }
}

fn resolve_path(value: &str, skin_root: &Path) -> PathBuf {
    let trimmed = value.trim().trim_matches('"').trim();
    let candidate = Path::new(trimmed);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        skin_root.join(candidate)
    }
}

fn expand_string(input: &str, variables: &BTreeMap<String, String>) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut output = String::with_capacity(input.len());
    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '#' {
            if let Some(offset) = chars[i + 1..].iter().position(|&c| c == '#') {
                let name: String = chars[i + 1..i + 1 + offset].iter().collect();
                match variables.get(&name.to_ascii_lowercase()) {
                    Some(value) => output.push_str(value),
                    None => {
                        output.push('#');
                        output.push_str(&name);
                        output.push('#');
                    }
                }
                i += offset + 2;
                continue;
            }
            output.push('#');
            i += 1;
        } else if ch == '%' {
            if let Some(offset) = chars[i + 1..].iter().position(|&c| c == '%') {
                let name: String = chars[i + 1..i + 1 + offset].iter().collect();
                if !name.is_empty() {
                    match std::env::var(&name) {
                        Ok(value) => output.push_str(&value),
                        Err(_) => {
                            output.push('%');
                            output.push_str(&name);
                            output.push('%');
                        }
                    }
                    i += offset + 2;
                    continue;
                }
            }
            output.push('%');
            i += 1;
        } else {
            output.push(ch);
            i += 1;
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn merges_simple_include() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("main.ini"),
            "[Variables]\nAccent=FF0000\n@Include=common.ini\n\n[MeterText]\nMeter=String\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("common.ini"),
            "[Variables]\nBase=10\n\n[MeasureBase]\nMeasure=Calc\nFormula=1\n",
        )
        .unwrap();

        let result = resolve(&dir.path().join("main.ini"), dir.path());
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert_eq!(result.document.get("Variables", "Base"), Some("10"));
        assert_eq!(result.document.get("Variables", "Accent"), Some("FF0000"));
        assert!(result.document.has_section("MeasureBase"));
        assert_eq!(result.files.len(), 2);
    }

    #[test]
    fn entry_file_overrides_include() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("main.ini"),
            "[MeterText]\nMeter=String\nFontSize=20\n@Include=style.ini\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("style.ini"),
            "[MeterText]\nMeter=String\nFontSize=12\nFontColor=FFFFFFFF\n",
        )
        .unwrap();

        let result = resolve(&dir.path().join("main.ini"), dir.path());
        assert_eq!(result.document.get("MeterText", "FontSize"), Some("20"));
        assert_eq!(
            result.document.get("MeterText", "FontColor"),
            Some("FFFFFFFF")
        );
    }

    #[test]
    fn detects_include_loop() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.ini"), "[S]\n@Include=b.ini\n").unwrap();
        fs::write(dir.path().join("b.ini"), "[T]\n@Include=a.ini\n").unwrap();

        let result = resolve(&dir.path().join("a.ini"), dir.path());
        assert!(result.diagnostics.has_errors());
        assert!(result
            .diagnostics
            .items
            .iter()
            .any(|d| d.message.contains("boucle d'inclusion")));
    }

    #[test]
    fn supports_numbered_includes_and_variable_paths() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("main.ini"),
            "[Variables]\nShared=shared.ini\n@Include2=#Shared#\n",
        )
        .unwrap();
        fs::write(dir.path().join("shared.ini"), "[Extra]\nKey=1\n").unwrap();

        let result = resolve(&dir.path().join("main.ini"), dir.path());
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert_eq!(result.document.get("Extra", "Key"), Some("1"));
    }

    #[test]
    fn missing_include_reports_error() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("main.ini"), "[S]\n@Include=absent.ini\n").unwrap();
        let result = resolve(&dir.path().join("main.ini"), dir.path());
        assert!(result.diagnostics.has_errors());
    }
}
