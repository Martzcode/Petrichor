use std::collections::BTreeSet;
use std::path::Path;

use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::ini::IniDocument;
use crate::readers::{try_read_bool, try_read_color, try_read_i32};

pub const KNOWN_MEASURES: &[&str] = &[
    "CPU",
    "Memory",
    "PhysicalMemory",
    "SwapMemory",
    "FreeDiskSpace",
    "Loop",
    "NetIn",
    "NetOut",
    "NetTotal",
    "Process",
    "RecycleManager",
    "Time",
    "Uptime",
    "Calc",
    "String",
    "SysInfo",
    "WebParser",
    "WifiStatus",
    "MediaKey",
    "Plugin",
    "Script",
    "NowPlaying",
    "Registry",
];

pub const KNOWN_METERS: &[&str] = &[
    "String",
    "Image",
    "Bar",
    "Bitmap",
    "Button",
    "Histogram",
    "Line",
    "Rotator",
    "RoundLine",
    "Shape",
];

const INTEGER_OPTIONS: &[&str] = &[
    "X",
    "Y",
    "W",
    "H",
    "UpdateDivider",
    "UpdateRate",
    "FontSize",
    "BarBorder",
    "AlphaValue",
    "Padding",
    "StrokeWidth",
    "LineWidth",
    "StartAngle",
    "RotationAngle",
];

const COLOR_OPTIONS: &[&str] = &[
    "FontColor",
    "SolidColor",
    "SolidColor2",
    "BarColor",
    "LineColor",
    "BarBorderColor",
    "BevelColor",
    "BevelColor2",
    "SelectedColor",
    "LineColor2",
    "StrokeColor",
];

const BOOLEAN_OPTIONS: &[&str] = &[
    "Hidden",
    "Disabled",
    "AntiAlias",
    "AutoScale",
    "DynamicWindowSize",
    "AccurateText",
];

pub fn validate_document(document: &IniDocument, file: &Path, diagnostics: &mut Diagnostics) {
    let mut seen: BTreeSet<String> = BTreeSet::new();

    for section in &document.sections {
        if section.name.is_empty() {
            continue;
        }
        let lower = section.name.to_ascii_lowercase();

        if !seen.insert(lower.clone()) {
            diagnostics.push(
                Diagnostic::warning(format!(
                    "section « {} » définie plusieurs fois",
                    section.name
                ))
                .with_file(file)
                .with_line(section.line)
                .with_section(&section.name),
            );
        }

        if lower == "variables" || lower == "metadata" || lower == "petrichor" {
            continue;
        }

        let measure = section.get("Measure");
        let meter = section.get("Meter");

        if measure.is_some() && meter.is_some() {
            diagnostics.push(
                Diagnostic::error("une section ne peut pas être à la fois Measure et Meter")
                    .with_file(file)
                    .with_line(section.line)
                    .with_section(&section.name),
            );
            continue;
        }

        if let Some(kind) = measure {
            if !KNOWN_MEASURES
                .iter()
                .any(|k| k.eq_ignore_ascii_case(kind.trim()))
            {
                diagnostics.push(
                    Diagnostic::warning(format!("type de mesure inconnu: « {} »", kind.trim()))
                        .with_file(file)
                        .with_line(section.line)
                        .with_section(&section.name)
                        .with_key("Measure"),
                );
            }
        } else if let Some(kind) = meter {
            if !KNOWN_METERS
                .iter()
                .any(|k| k.eq_ignore_ascii_case(kind.trim()))
            {
                diagnostics.push(
                    Diagnostic::warning(format!("type de meter inconnu: « {} »", kind.trim()))
                        .with_file(file)
                        .with_line(section.line)
                        .with_section(&section.name)
                        .with_key("Meter"),
                );
            }
        }

        validate_options(document, file, &section.name, diagnostics);
    }
}

fn validate_options(
    _document: &IniDocument,
    file: &Path,
    section_name: &str,
    diagnostics: &mut Diagnostics,
) {
    let section = match _document.section(section_name) {
        Some(section) => section,
        None => return,
    };

    for entry in &section.entries {
        let key = entry.key.to_ascii_lowercase();

        if is_dynamic(&entry.value) {
            continue;
        }

        if let Some(option) = INTEGER_OPTIONS
            .iter()
            .find(|o| o.eq_ignore_ascii_case(&key))
        {
            let candidate = if key == "x" || key == "y" {
                entry.value.trim().trim_end_matches(['r', 'R', '%'])
            } else {
                entry.value.trim()
            };
            if try_read_i32(candidate).is_err() {
                diagnostics.push(
                    Diagnostic::warning(format!(
                        "valeur entière invalide pour {option}: « {} »",
                        entry.value
                    ))
                    .with_file(file)
                    .with_line(entry.line)
                    .with_section(section_name)
                    .with_key(&entry.key),
                );
            }
        } else if let Some(option) = COLOR_OPTIONS.iter().find(|o| o.eq_ignore_ascii_case(&key)) {
            if try_read_color(&entry.value).is_err() {
                diagnostics.push(
                    Diagnostic::warning(format!(
                        "couleur invalide pour {option}: « {} »",
                        entry.value
                    ))
                    .with_file(file)
                    .with_line(entry.line)
                    .with_section(section_name)
                    .with_key(&entry.key),
                );
            }
        } else if let Some(option) = BOOLEAN_OPTIONS
            .iter()
            .find(|o| o.eq_ignore_ascii_case(&key))
        {
            if try_read_bool(&entry.value).is_err() {
                diagnostics.push(
                    Diagnostic::warning(format!(
                        "booléen invalide pour {option}: « {} »",
                        entry.value
                    ))
                    .with_file(file)
                    .with_line(entry.line)
                    .with_section(section_name)
                    .with_key(&entry.key),
                );
            }
        }
    }
}

fn is_dynamic(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with('(')
        || trimmed.contains('#')
        || trimmed.contains('[')
        || trimmed.contains('$')
        || trimmed.contains('%')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validate(text: &str) -> Diagnostics {
        let document = IniDocument::parse(text);
        let mut diagnostics = Diagnostics::new();
        validate_document(&document, Path::new("/skins/Test.ini"), &mut diagnostics);
        diagnostics
    }

    #[test]
    fn accepts_valid_skin() {
        let diagnostics = validate(
            "[Variables]\nA=1\n\n[MeasureCPU]\nMeasure=CPU\n\n[MeterText]\nMeter=String\nX=10\nFontColor=FFFFFFFF\n",
        );
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    }

    #[test]
    fn warns_on_unknown_measure_type() {
        let diagnostics = validate("[MeasureWeird]\nMeasure=Bogus\n");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.items[0].severity,
            crate::diagnostics::Severity::Warning
        );
        assert_eq!(diagnostics.items[0].line, Some(1));
    }

    #[test]
    fn errors_on_measure_and_meter_together() {
        let diagnostics = validate("[Section]\nMeasure=CPU\nMeter=String\n");
        assert!(diagnostics.has_errors());
    }

    #[test]
    fn warns_on_invalid_option_values() {
        let diagnostics =
            validate("[MeterText]\nMeter=String\nX=abc\nFontColor=ZZZZZZ\nHidden=peut-être\n");
        assert_eq!(diagnostics.len(), 3);
        assert!(diagnostics
            .items
            .iter()
            .all(|d| d.severity == crate::diagnostics::Severity::Warning));
        assert_eq!(diagnostics.items[0].line, Some(3));
    }

    #[test]
    fn warns_on_duplicate_sections() {
        let diagnostics = validate("[MeterA]\nMeter=String\n\n[MeterA]\nMeter=String\n");
        assert!(diagnostics
            .items
            .iter()
            .any(|d| d.message.contains("définie plusieurs fois")));
    }

    #[test]
    fn allows_style_sections_without_type() {
        let diagnostics = validate("[StyleTitle]\nFontSize=20\nFontColor=FFFFFFFF\n");
        assert!(diagnostics.is_empty());
    }
}
