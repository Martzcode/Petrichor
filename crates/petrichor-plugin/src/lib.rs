use petrichor_core::error::{CoreError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginVersion {
    V1,
    V2,
    V3,
    V4,
}

impl PluginVersion {
    pub fn from_int(value: i32) -> Option<PluginVersion> {
        match value {
            1 => Some(PluginVersion::V1),
            2 => Some(PluginVersion::V2),
            3 => Some(PluginVersion::V3),
            4 => Some(PluginVersion::V4),
            _ => None,
        }
    }
}

pub trait Plugin: Send {
    fn name(&self) -> &str;
    fn version(&self) -> PluginVersion;
    fn update(&mut self);
    fn read_string(&self, name: &str, option: &str) -> Option<String>;
    fn execute_bang(&mut self, bang: &str, args: &str) -> bool;
}

#[derive(Default)]
pub struct PluginRegistry {
    plugins: Vec<Box<dyn Plugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        PluginRegistry {
            plugins: Vec::new(),
        }
    }

    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        self.plugins.push(plugin);
    }

    pub fn find(&self, name: &str) -> Option<&dyn Plugin> {
        self.plugins
            .iter()
            .find(|p| p.name().eq_ignore_ascii_case(name))
            .map(|p| p.as_ref())
    }

    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }
}

pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(CoreError::Other("nom de plugin vide".into()));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
    {
        return Err(CoreError::Other(format!("nom de plugin invalide: {name}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dummy;

    impl Plugin for Dummy {
        fn name(&self) -> &str {
            "Dummy"
        }
        fn version(&self) -> PluginVersion {
            PluginVersion::V4
        }
        fn update(&mut self) {}
        fn read_string(&self, _name: &str, _option: &str) -> Option<String> {
            None
        }
        fn execute_bang(&mut self, _bang: &str, _args: &str) -> bool {
            false
        }
    }

    #[test]
    fn registry_finds_plugin_case_insensitively() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(Dummy));
        assert!(registry.find("dummy").is_some());
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn invalid_names_are_rejected() {
        assert!(validate_name("").is_err());
        assert!(validate_name("bad/name").is_err());
        assert!(validate_name("Good_Name").is_ok());
    }
}
