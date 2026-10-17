use std::collections::BTreeSet;

use crate::ini::{IniDocument, IniSection};

pub const STYLE_OPTION: &str = "MeterStyle";

pub fn style_list(section: &IniSection) -> Vec<String> {
    match section.get(STYLE_OPTION) {
        Some(value) => value
            .split('|')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect(),
        None => Vec::new(),
    }
}

pub fn resolve_option(document: &IniDocument, section: &str, key: &str) -> Option<String> {
    let mut visited = BTreeSet::new();
    resolve_option_inner(document, section, key, &mut visited)
}

fn resolve_option_inner(
    document: &IniDocument,
    section: &str,
    key: &str,
    visited: &mut BTreeSet<String>,
) -> Option<String> {
    let key_lower = key.to_ascii_lowercase();
    if !visited.insert(format!("{}::{}", section.to_ascii_lowercase(), key_lower)) {
        return None;
    }

    let current = document.section(section)?;

    if let Some(value) = current.get(key) {
        return Some(value.to_string());
    }

    let styles = style_list(current);
    for style in styles.iter().rev() {
        if let Some(value) = resolve_option_inner(document, style, key, visited) {
            return Some(value);
        }
    }

    None
}

pub fn resolved_options(document: &IniDocument, section: &str) -> Vec<(String, String)> {
    let mut result: Vec<(String, String)> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    collect_options(
        document,
        section,
        &mut result,
        &mut seen,
        &mut BTreeSet::new(),
    );
    result
}

fn collect_options(
    document: &IniDocument,
    section: &str,
    result: &mut Vec<(String, String)>,
    seen: &mut BTreeSet<String>,
    stack: &mut BTreeSet<String>,
) {
    let section_key = section.to_ascii_lowercase();
    if !stack.insert(section_key.clone()) {
        return;
    }

    if let Some(current) = document.section(section) {
        for entry in &current.entries {
            let key = entry.key.to_ascii_lowercase();
            if key == STYLE_OPTION.to_ascii_lowercase() {
                continue;
            }
            if seen.insert(key.clone()) {
                result.push((entry.key.clone(), entry.value.clone()));
            }
        }

        let styles = style_list(current);
        for style in styles.iter().rev() {
            collect_options(document, style, result, seen, stack);
        }
    }

    stack.remove(&section_key);
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "\
[Base]
FontSize=12
FontColor=FFFFFFFF
SolidColor=00000000

[Accent]
FontColor=FF8800FF
StringAlign=Center

[Button]
Meter=String
MeterStyle=Base|Accent
FontSize=20
Text=OK
";

    fn doc() -> IniDocument {
        IniDocument::parse(DOC)
    }

    #[test]
    fn lists_styles() {
        let document = doc();
        let section = document.section("Button").unwrap();
        assert_eq!(style_list(section), vec!["Base", "Accent"]);
    }

    #[test]
    fn resolves_direct_option() {
        let document = doc();
        assert_eq!(
            resolve_option(&document, "Button", "FontSize").unwrap(),
            "20"
        );
    }

    #[test]
    fn resolves_inherited_option() {
        let document = doc();
        assert_eq!(
            resolve_option(&document, "Button", "SolidColor").unwrap(),
            "00000000"
        );
    }

    #[test]
    fn later_style_wins_on_conflict() {
        let document = doc();
        assert_eq!(
            resolve_option(&document, "Button", "FontColor").unwrap(),
            "FF8800FF"
        );
    }

    #[test]
    fn missing_option_is_none() {
        let document = doc();
        assert!(resolve_option(&document, "Button", "Absent").is_none());
    }

    #[test]
    fn handles_style_cycles() {
        let document = IniDocument::parse("[A]\nMeterStyle=B\nFontSize=1\n[B]\nMeterStyle=A\n");
        assert_eq!(resolve_option(&document, "A", "FontSize").unwrap(), "1");
    }

    #[test]
    fn collects_own_options_first() {
        let document = doc();
        let options = resolved_options(&document, "Button");
        let keys: Vec<&str> = options.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys[0], "Meter");
        assert!(keys.contains(&"FontSize"));
        assert!(keys.contains(&"SolidColor"));
        let font_size = options
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("FontSize"))
            .unwrap();
        assert_eq!(font_size.1, "20");
    }
}
