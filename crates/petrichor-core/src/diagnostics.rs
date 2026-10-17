use std::path::PathBuf;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub file: Option<PathBuf>,
    pub line: Option<u32>,
    pub section: Option<String>,
    pub key: Option<String>,
    pub message: String,
}

impl Diagnostic {
    pub fn new(severity: Severity, message: impl Into<String>) -> Self {
        Diagnostic {
            severity,
            file: None,
            line: None,
            section: None,
            key: None,
            message: message.into(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Diagnostic::new(Severity::Error, message)
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Diagnostic::new(Severity::Warning, message)
    }

    pub fn with_file(mut self, file: impl Into<PathBuf>) -> Self {
        self.file = Some(file.into());
        self
    }

    pub fn with_line(mut self, line: u32) -> Self {
        self.line = Some(line);
        self
    }

    pub fn with_section(mut self, section: impl Into<String>) -> Self {
        self.section = Some(section.into());
        self
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let level = match self.severity {
            Severity::Error => "ERREUR",
            Severity::Warning => "AVERTISSEMENT",
            Severity::Info => "INFO",
        };
        write!(f, "[{level}] ")?;
        if let Some(file) = &self.file {
            write!(f, "{}", file.display())?;
            if let Some(line) = self.line {
                write!(f, ":{line}")?;
            }
            write!(f, " ")?;
        }
        if let Some(section) = &self.section {
            write!(f, "[{section}] ")?;
        }
        if let Some(key) = &self.key {
            write!(f, "{key}: ")?;
        }
        write!(f, "{}", self.message)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Diagnostics {
    pub items: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Diagnostics { items: Vec::new() }
    }

    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.items.push(diagnostic);
    }

    pub fn extend(&mut self, other: Diagnostics) {
        self.items.extend(other.items);
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.push(Diagnostic::error(message));
    }

    pub fn warning(&mut self, message: impl Into<String>) {
        self.push(Diagnostic::warning(message));
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn has_errors(&self) -> bool {
        self.items.iter().any(|d| d.severity == Severity::Error)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.items.iter()
    }

    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.items.iter().filter(|d| d.severity == Severity::Error)
    }

    pub fn count(&self, severity: Severity) -> usize {
        self.items.iter().filter(|d| d.severity == severity).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_errors() {
        let mut diagnostics = Diagnostics::new();
        diagnostics.error("boum");
        diagnostics.warning("attention");
        assert!(diagnostics.has_errors());
        assert_eq!(diagnostics.count(Severity::Error), 1);
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn formats_location() {
        let diagnostic = Diagnostic::error("option invalide")
            .with_file("/skins/Clock.ini")
            .with_line(12)
            .with_section("MeterText")
            .with_key("FontSize");
        assert_eq!(
            diagnostic.to_string(),
            "[ERREUR] /skins/Clock.ini:12 [MeterText] FontSize: option invalide"
        );
    }
}
