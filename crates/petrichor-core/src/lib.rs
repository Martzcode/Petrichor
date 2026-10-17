pub mod config;
pub mod diagnostics;
pub mod error;
pub mod formula;
pub mod include;
pub mod ini;
pub mod logging;
pub mod measure;
pub mod model;
pub mod readers;
pub mod reference;
pub mod registry;
pub mod render;
pub mod style;
pub mod validate;
pub mod variables;

pub use config::{ApplicationConfig, GlobalSettings, IniFile, Section, SkinState};
pub use diagnostics::{Diagnostic, Diagnostics, Severity};
pub use error::{CoreError, Result};
pub use ini::{Encoding, IniDocument, IniEntry, IniSection};
pub use model::{
    build_skin, build_skin_with_document, BackgroundMode, BevelType, Measure, Metadata, Meter,
    Options, Skin, SkinSettings,
};
pub use readers::{Color, ReadError, ReadResult, Rect};
pub use reference::{MeasureSource, MeasureValue, Reference, ReferenceError};
pub use registry::{Registry, SkinFile, SkinFolder};
pub use render::{build_render_model, Background, CommonMeter, RenderMeter, RenderModel};
pub use style::{resolve_option, style_list};
pub use validate::validate_document;
pub use variables::Variables;

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
