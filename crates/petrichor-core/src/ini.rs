use std::path::{Path, PathBuf};

use crate::error::{CoreError, Result};

pub fn unquote(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16Le,
    Utf16Be,
}

impl Encoding {
    pub fn detect(bytes: &[u8]) -> Encoding {
        if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            return Encoding::Utf8;
        }
        if bytes.starts_with(&[0xFF, 0xFE]) {
            return Encoding::Utf16Le;
        }
        if bytes.starts_with(&[0xFE, 0xFF]) {
            return Encoding::Utf16Be;
        }

        if looks_like_utf16(bytes, 0) {
            return Encoding::Utf16Le;
        }
        if looks_like_utf16(bytes, 1) {
            return Encoding::Utf16Be;
        }

        Encoding::Utf8
    }

    pub fn decode(self, bytes: &[u8]) -> String {
        match self {
            Encoding::Utf8 => {
                let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
                String::from_utf8_lossy(bytes).into_owned()
            }
            Encoding::Utf16Le => {
                let bytes = bytes.strip_prefix(&[0xFF, 0xFE]).unwrap_or(bytes);
                decode_utf16(bytes, true)
            }
            Encoding::Utf16Be => {
                let bytes = bytes.strip_prefix(&[0xFE, 0xFF]).unwrap_or(bytes);
                decode_utf16(bytes, false)
            }
        }
    }
}

fn looks_like_utf16(bytes: &[u8], offset: usize) -> bool {
    if bytes.len() < 4 {
        return false;
    }
    let sample = &bytes[offset..bytes.len().min(offset + 64)];
    let mut pairs = 0usize;
    let mut zeros = 0usize;
    let mut i = 0;
    while i + 1 < sample.len() {
        pairs += 1;
        if sample[i + 1] == 0 {
            zeros += 1;
        }
        i += 2;
    }
    pairs > 0 && zeros * 2 >= pairs
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|chunk| {
            if little_endian {
                u16::from_le_bytes(*chunk)
            } else {
                u16::from_be_bytes(*chunk)
            }
        })
        .collect();
    char::decode_utf16(units)
        .map(|r| r.unwrap_or('\u{FFFD}'))
        .collect()
}

pub fn normalize_newlines(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_string();
    }
    text.replace("\r\n", "\n").replace('\r', "\n")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IniEntry {
    pub key: String,
    pub value: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IniSection {
    pub name: String,
    pub line: u32,
    pub entries: Vec<IniEntry>,
}

impl IniSection {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entry(key).map(|e| e.value.as_str())
    }

    pub fn entry(&self, key: &str) -> Option<&IniEntry> {
        self.entries
            .iter()
            .find(|e| e.key.eq_ignore_ascii_case(key))
    }

    pub fn set(&mut self, key: &str, value: &str, line: u32) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|e| e.key.eq_ignore_ascii_case(key))
        {
            entry.value = value.to_string();
        } else {
            self.entries.push(IniEntry {
                key: key.to_string(),
                value: value.to_string(),
                line,
            });
        }
    }

    pub fn remove(&mut self, key: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| !e.key.eq_ignore_ascii_case(key));
        self.entries.len() != before
    }

    pub fn keys_with_prefix<'a>(&'a self, prefix: &'a str) -> impl Iterator<Item = &'a IniEntry> {
        self.entries
            .iter()
            .filter(move |e| e.key.to_ascii_lowercase().starts_with(prefix))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IniDocument {
    pub sections: Vec<IniSection>,
    pub file: Option<PathBuf>,
}

impl IniDocument {
    pub fn new() -> Self {
        IniDocument::default()
    }

    pub fn parse(text: &str) -> Self {
        let mut document = IniDocument::new();
        let mut current: Option<usize> = None;
        let normalized = normalize_newlines(text);
        let stripped = normalized.strip_prefix('\u{feff}').unwrap_or(&normalized);

        for (index, raw_line) in stripped.lines().enumerate() {
            let line_number = (index + 1) as u32;
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }

            if let Some(name) = parse_section_header(line) {
                document.sections.push(IniSection {
                    name,
                    line: line_number,
                    entries: Vec::new(),
                });
                current = Some(document.sections.len() - 1);
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                if key.is_empty() {
                    continue;
                }
                let value = strip_inline_comment(value.trim());
                let index = match current {
                    Some(index) => index,
                    None => {
                        document.sections.push(IniSection {
                            name: String::new(),
                            line: line_number,
                            entries: Vec::new(),
                        });
                        current = Some(document.sections.len() - 1);
                        document.sections.len() - 1
                    }
                };
                document.sections[index].set(key, &value, line_number);
            }
        }

        if document
            .sections
            .first()
            .map(|s| s.name.is_empty() && s.entries.is_empty())
            .unwrap_or(false)
        {
            document.sections.remove(0);
        }

        document
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        IniDocument::parse(&Encoding::detect(bytes).decode(bytes))
    }

    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path).map_err(|e| CoreError::io(path, e))?;
        let mut document = IniDocument::from_bytes(&bytes);
        document.file = Some(path.to_path_buf());
        Ok(document)
    }

    pub fn section(&self, name: &str) -> Option<&IniSection> {
        self.sections
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }

    pub fn section_mut(&mut self, name: &str) -> &mut IniSection {
        if let Some(index) = self
            .sections
            .iter()
            .position(|s| s.name.eq_ignore_ascii_case(name))
        {
            return &mut self.sections[index];
        }
        self.sections.push(IniSection {
            name: name.to_string(),
            line: 0,
            entries: Vec::new(),
        });
        self.sections.last_mut().unwrap()
    }

    pub fn has_section(&self, name: &str) -> bool {
        self.section(name).is_some()
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.section(section).and_then(|s| s.get(key))
    }

    pub fn set(&mut self, section: &str, key: &str, value: &str) {
        self.section_mut(section).set(key, value, 0);
    }

    pub fn section_names(&self) -> impl Iterator<Item = &str> {
        self.sections.iter().map(|s| s.name.as_str())
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        for section in &self.sections {
            if section.name.is_empty() && section.entries.is_empty() {
                continue;
            }
            if !out.is_empty() {
                out.push('\n');
            }
            out.push('[');
            out.push_str(&section.name);
            out.push_str("]\n");
            for entry in &section.entries {
                out.push_str(&entry.key);
                out.push('=');
                out.push_str(&entry.value);
                out.push('\n');
            }
        }
        out
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
        }
        std::fs::write(path, self.render()).map_err(|e| CoreError::io(path, e))
    }
}

fn parse_section_header(line: &str) -> Option<String> {
    if line.starts_with('[') {
        if let Some(end) = line.find(']') {
            let name = line[1..end].trim();
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

fn strip_inline_comment(value: &str) -> String {
    if is_number_list(value, ';') {
        return value.trim().to_string();
    }
    let mut result = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch == ';' {
            break;
        }
        result.push(ch);
    }
    result.trim().to_string()
}

fn is_number_list(value: &str, separator: char) -> bool {
    let parts: Vec<&str> = value
        .split(separator)
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    parts.len() > 1 && parts.iter().all(|part| part.parse::<f32>().is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
; en-tête
[Variables]
Nom=Monde
[MeasureCPU]
Measure=CPU
; commentaire interne
Processor=0

[MeterText]
Meter=String
Text=Bonjour #Nom#
";

    #[test]
    fn parses_sections_with_lines() {
        let doc = IniDocument::parse(SAMPLE);
        assert_eq!(doc.sections.len(), 3);
        assert_eq!(doc.sections[0].name, "Variables");
        assert_eq!(doc.sections[0].line, 2);
        assert_eq!(doc.get("MeasureCPU", "Measure"), Some("CPU"));
        assert_eq!(doc.get("MeasureCPU", "Processor"), Some("0"));
    }

    #[test]
    fn tracks_entry_line_numbers() {
        let doc = IniDocument::parse(SAMPLE);
        let section = doc.section("MeterText").unwrap();
        assert_eq!(section.entry("Text").unwrap().line, 11);
    }

    #[test]
    fn strips_inline_comments() {
        let doc = IniDocument::parse("[S]\nKey=value ; note\n");
        assert_eq!(doc.get("S", "Key"), Some("value"));
    }

    #[test]
    fn keeps_transformation_matrix_values() {
        let doc = IniDocument::parse("[S]\nMatrix=1;0;0;1;5;6\n");
        assert_eq!(doc.get("S", "Matrix"), Some("1;0;0;1;5;6"));
    }

    #[test]
    fn key_lookup_is_case_insensitive() {
        let doc = IniDocument::parse(SAMPLE);
        assert_eq!(doc.get("measurecpu", "processor"), Some("0"));
    }

    #[test]
    fn decodes_utf8_bom() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(b"[S]\nKey=Valeur\n");
        let doc = IniDocument::from_bytes(&bytes);
        assert_eq!(doc.get("S", "Key"), Some("Valeur"));
    }

    #[test]
    fn decodes_utf16_le() {
        let text = "[S]\r\nKey=Valeur\r\n";
        let mut bytes = vec![0xFF, 0xFE];
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let doc = IniDocument::from_bytes(&bytes);
        assert_eq!(doc.get("S", "Key"), Some("Valeur"));
    }

    #[test]
    fn decodes_utf16_be_without_bom() {
        let text = "[Variables]\nGreeting=Héllo\n";
        let mut bytes = Vec::new();
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
        let doc = IniDocument::from_bytes(&bytes);
        assert_eq!(doc.get("Variables", "Greeting"), Some("Héllo"));
    }

    #[test]
    fn handles_lone_cr_line_endings() {
        let doc = IniDocument::parse("[S]\rKey=1\rOther=2");
        assert_eq!(doc.get("S", "Key"), Some("1"));
        assert_eq!(doc.get("S", "Other"), Some("2"));
    }

    #[test]
    fn roundtrip_render() {
        let doc = IniDocument::parse(SAMPLE);
        let rendered = doc.render();
        let reparsed = IniDocument::parse(&rendered);
        assert_eq!(reparsed.get("Variables", "Nom"), Some("Monde"));
        assert_eq!(reparsed.get("MeterText", "Text"), Some("Bonjour #Nom#"));
    }

    #[test]
    fn detects_encoding_variants() {
        assert_eq!(Encoding::detect(b"[S]"), Encoding::Utf8);
        assert_eq!(Encoding::detect(&[0xFF, 0xFE, b'[']), Encoding::Utf16Le);
        assert_eq!(
            Encoding::detect(&[0xFE, 0xFF, 0x00, b'[']),
            Encoding::Utf16Be
        );
    }
}
