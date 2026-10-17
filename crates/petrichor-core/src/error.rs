use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("erreur d'entrée/sortie sur {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("format de configuration invalide: {0}")]
    Parse(String),

    #[error("section introuvable: {0}")]
    SectionNotFound(String),

    #[error("option introuvable: {section}.{key}")]
    OptionNotFound { section: String, key: String },

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;

impl CoreError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        CoreError::Io {
            path: path.into(),
            source,
        }
    }
}
