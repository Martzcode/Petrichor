pub mod config;
pub mod error;
pub mod logging;

pub use config::{GlobalSettings, IniFile, Section};
pub use error::{CoreError, Result};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn version() -> &'static str {
    VERSION
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_not_empty() {
        assert!(!super::version().is_empty());
    }
}
