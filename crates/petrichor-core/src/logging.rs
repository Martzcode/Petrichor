use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

pub const RING_CAPACITY: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Error,
    Warning,
    Notice,
    Debug,
}

impl Level {
    pub fn from_int(value: i32) -> Level {
        match value {
            1 => Level::Error,
            2 => Level::Warning,
            3 => Level::Notice,
            _ => Level::Debug,
        }
    }

    pub fn as_int(self) -> i32 {
        match self {
            Level::Error => 1,
            Level::Warning => 2,
            Level::Notice => 3,
            Level::Debug => 4,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Level::Error => "ERROR",
            Level::Warning => "WARNING",
            Level::Notice => "NOTICE",
            Level::Debug => "DEBUG",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub level: Level,
    pub timestamp_ms: u128,
    pub source: Option<String>,
    pub message: String,
}

impl Entry {
    pub fn format(&self) -> String {
        match &self.source {
            Some(source) => {
                format!("[{}] ({}) {}", self.level.as_str(), source, self.message)
            }
            None => format!("[{}] {}", self.level.as_str(), self.message),
        }
    }
}

pub struct Logger {
    entries: Mutex<VecDeque<Entry>>,
    file: Mutex<Option<File>>,
    to_file: AtomicBool,
    debug: AtomicBool,
}

impl Logger {
    fn new() -> Self {
        Logger {
            entries: Mutex::new(VecDeque::with_capacity(RING_CAPACITY)),
            file: Mutex::new(None),
            to_file: AtomicBool::new(false),
            debug: AtomicBool::new(false),
        }
    }

    pub fn set_debug(&self, enabled: bool) {
        self.debug.store(enabled, Ordering::Relaxed);
    }

    pub fn debug_enabled(&self) -> bool {
        self.debug.load(Ordering::Relaxed)
    }

    pub fn start_file_logging(&self, path: &Path) -> std::io::Result<()> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        *self.file.lock().unwrap() = Some(file);
        self.to_file.store(true, Ordering::Relaxed);
        Ok(())
    }

    pub fn stop_file_logging(&self) {
        self.to_file.store(false, Ordering::Relaxed);
        *self.file.lock().unwrap() = None;
    }

    pub fn delete_file_log(&self, path: &Path) -> std::io::Result<()> {
        self.stop_file_logging();
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn log(&self, level: Level, source: Option<&str>, message: impl Into<String>) {
        if level == Level::Debug && !self.debug_enabled() {
            return;
        }

        let entry = Entry {
            level,
            timestamp_ms: now_ms(),
            source: source.map(|s| s.to_string()),
            message: message.into(),
        };

        match level {
            Level::Error => tracing::error!(target: "petrichor", "{}", entry.message),
            Level::Warning => tracing::warn!(target: "petrichor", "{}", entry.message),
            Level::Notice => tracing::info!(target: "petrichor", "{}", entry.message),
            Level::Debug => tracing::debug!(target: "petrichor", "{}", entry.message),
        }

        {
            let mut entries = self.entries.lock().unwrap();
            if entries.len() >= RING_CAPACITY {
                entries.pop_front();
            }
            entries.push_back(entry.clone());
        }

        if self.to_file.load(Ordering::Relaxed) {
            if let Some(file) = self.file.lock().unwrap().as_mut() {
                let _ = writeln!(file, "{}", entry.format());
            }
        }
    }

    pub fn entries(&self) -> Vec<Entry> {
        self.entries.lock().unwrap().iter().cloned().collect()
    }

    pub fn clear(&self) {
        self.entries.lock().unwrap().clear();
    }
}

static LOGGER: OnceLock<Logger> = OnceLock::new();

pub fn logger() -> &'static Logger {
    LOGGER.get_or_init(Logger::new)
}

pub fn init(debug: bool) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                if debug {
                    tracing_subscriber::EnvFilter::new("debug")
                } else {
                    tracing_subscriber::EnvFilter::new("info")
                }
            }),
        )
        .try_init();

    logger().set_debug(debug);
}

pub fn error(message: impl Into<String>) {
    logger().log(Level::Error, None, message);
}

pub fn warning(message: impl Into<String>) {
    logger().log(Level::Warning, None, message);
}

pub fn notice(message: impl Into<String>) {
    logger().log(Level::Notice, None, message);
}

pub fn debug(message: impl Into<String>) {
    logger().log(Level::Debug, None, message);
}

pub fn log_skin(level: Level, skin: &str, section: Option<&str>, message: impl Into<String>) {
    let source = match section {
        Some(section) => format!("{skin}\\{section}"),
        None => skin.to_string(),
    };
    logger().log(level, Some(&source), message);
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

pub fn default_log_path(settings_dir: &Path) -> PathBuf {
    settings_dir.join("Petrichor.log")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_roundtrip() {
        for value in 1..=4 {
            assert_eq!(Level::from_int(value).as_int(), value);
        }
        assert_eq!(Level::from_int(99), Level::Debug);
    }

    #[test]
    fn ring_buffer_is_bounded() {
        let logger = Logger::new();
        logger.set_debug(true);
        for i in 0..(RING_CAPACITY + 100) {
            logger.log(Level::Notice, None, format!("message {i}"));
        }
        assert_eq!(logger.entries().len(), RING_CAPACITY);
    }

    #[test]
    fn debug_is_filtered_when_disabled() {
        let logger = Logger::new();
        logger.log(Level::Debug, None, "should be dropped");
        assert!(logger.entries().is_empty());
    }
}
