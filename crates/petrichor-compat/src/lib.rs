use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Plan {
    A,
    B,
    C,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CompatLevel {
    Native,
    Automatic,
    Manual,
    Unsupported,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompatEntry {
    pub feature: String,
    pub requested: String,
    pub resolved: Option<String>,
    pub level: CompatLevel,
    pub plan: Option<Plan>,
    pub message: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct CompatReport {
    pub skin: String,
    pub entries: Vec<CompatEntry>,
}

impl CompatReport {
    pub fn new(skin: impl Into<String>) -> Self {
        CompatReport {
            skin: skin.into(),
            entries: Vec::new(),
        }
    }

    pub fn record(
        &mut self,
        feature: impl Into<String>,
        requested: impl Into<String>,
        resolved: Option<String>,
        level: CompatLevel,
        plan: Option<Plan>,
        message: Option<String>,
    ) {
        self.entries.push(CompatEntry {
            feature: feature.into(),
            requested: requested.into(),
            resolved,
            level,
            plan,
            message,
        });
    }

    pub fn has_unsupported(&self) -> bool {
        self.entries
            .iter()
            .any(|e| e.level == CompatLevel::Unsupported)
    }

    pub fn count_by_level(&self, level: CompatLevel) -> usize {
        self.entries.iter().filter(|e| e.level == level).count()
    }
}

pub trait CompatLayer: Send + Sync {
    fn resolve_string(&self, value: &str) -> Option<String>;
    fn resolve_path(&self, path: &str) -> Option<PathBuf>;
}

#[derive(Debug, Default)]
pub struct BuiltinVars {
    vars: BTreeMap<String, String>,
}

impl BuiltinVars {
    pub fn new() -> Self {
        BuiltinVars {
            vars: BTreeMap::new(),
        }
    }

    pub fn set(&mut self, name: &str, value: &str) {
        self.vars
            .insert(name.to_ascii_lowercase(), value.to_string());
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.vars
            .get(&name.to_ascii_lowercase())
            .map(|s| s.as_str())
    }

    pub fn expand(&self, input: &str) -> String {
        let mut output = String::with_capacity(input.len());
        let mut rest = input;
        while let Some(start) = rest.find('#') {
            output.push_str(&rest[..start]);
            let after = &rest[start..];
            if let Some(end) = after[1..].find('#') {
                let name = &after[1..1 + end];
                if let Some(value) = self.get(name) {
                    output.push_str(value);
                } else {
                    output.push_str(&after[..end + 2]);
                }
                rest = &after[end + 2..];
            } else {
                output.push_str(after);
                return output;
            }
        }
        output.push_str(rest);
        output
    }
}

pub fn skin_root(base: &Path, skin: &str) -> PathBuf {
    base.join(skin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_tracks_unsupported() {
        let mut report = CompatReport::new("Test");
        report.record(
            "plugin",
            "Weather",
            None,
            CompatLevel::Unsupported,
            None,
            None,
        );
        assert!(report.has_unsupported());
        assert_eq!(report.count_by_level(CompatLevel::Unsupported), 1);
    }

    #[test]
    fn expands_builtin_variables() {
        let mut vars = BuiltinVars::new();
        vars.set("CURRENTPATH", "/home/user/Skins");
        assert_eq!(
            vars.expand("#CURRENTPATH#/Clock.ini"),
            "/home/user/Skins/Clock.ini"
        );
        assert_eq!(vars.expand("#UNKNOWN#"), "#UNKNOWN#");
    }

    #[test]
    fn skin_root_joins() {
        assert_eq!(
            skin_root(Path::new("/skins"), "Clock"),
            PathBuf::from("/skins/Clock")
        );
    }
}
