use petrichor_core::error::{CoreError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptKind {
    Measure,
    Meter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitializeResult {
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateResult {
    Continue,
}

pub trait ScriptHost: Send {
    fn script_file(&self) -> Option<&str>;
}

#[derive(Debug, Default)]
pub struct ScriptEngine {
    loaded: usize,
}

impl ScriptEngine {
    pub fn new() -> Self {
        ScriptEngine { loaded: 0 }
    }

    pub fn loaded_count(&self) -> usize {
        self.loaded
    }

    pub fn check_script_file(path: &str) -> Result<&str> {
        if path.is_empty() {
            return Err(CoreError::Other("chemin de script vide".into()));
        }
        if !path.to_ascii_lowercase().ends_with(".lua") {
            return Err(CoreError::Other(format!(
                "extension de script non supportée: {path}"
            )));
        }
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_lua_files() {
        assert!(ScriptEngine::check_script_file("Script.lua").is_ok());
        assert!(ScriptEngine::check_script_file("Script.LUA").is_ok());
    }

    #[test]
    fn rejects_other_extensions() {
        assert!(ScriptEngine::check_script_file("").is_err());
        assert!(ScriptEngine::check_script_file("script.js").is_err());
    }
}
