use std::fs;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OsRelease {
    pub name: String,
    pub version: String,
    pub id: String,
}

impl OsRelease {
    pub fn read() -> OsRelease {
        let mut release = OsRelease {
            name: "Linux".to_string(),
            version: String::new(),
            id: "linux".to_string(),
        };

        if let Ok(text) = fs::read_to_string("/etc/os-release") {
            for line in text.lines() {
                let line = line.trim();
                let (key, value) = match line.split_once('=') {
                    Some(pair) => pair,
                    None => continue,
                };
                let value = value.trim().trim_matches('"').to_string();
                match key {
                    "NAME" => release.name = value,
                    "VERSION" | "VERSION_ID" if release.version.is_empty() => {
                        release.version = value;
                    }
                    "ID" => release.id = value,
                    _ => {}
                }
            }
        }

        release
    }
}

pub trait SystemBackend: Send + Sync {
    fn hostname(&self) -> String;
    fn user_name(&self) -> String;
    fn os_release(&self) -> OsRelease;
    fn uptime_secs(&self) -> u64;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LinuxBackend;

impl LinuxBackend {
    pub fn new() -> Self {
        LinuxBackend
    }
}

impl SystemBackend for LinuxBackend {
    fn hostname(&self) -> String {
        fs::read_to_string("/proc/sys/kernel/hostname")
            .or_else(|_| fs::read_to_string("/etc/hostname"))
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    }

    fn user_name(&self) -> String {
        std::env::var("USER")
            .or_else(|_| std::env::var("LOGNAME"))
            .unwrap_or_default()
    }

    fn os_release(&self) -> OsRelease {
        OsRelease::read()
    }

    fn uptime_secs(&self) -> u64 {
        fs::read_to_string("/proc/uptime")
            .ok()
            .and_then(|text| {
                text.split_whitespace()
                    .next()
                    .and_then(|value| value.parse::<f64>().ok())
            })
            .map(|secs| secs as u64)
            .unwrap_or(0)
    }
}

pub fn backend() -> LinuxBackend {
    LinuxBackend::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_release_is_linux() {
        let release = OsRelease::read();
        assert!(!release.name.is_empty());
        assert!(!release.id.is_empty());
    }

    #[test]
    fn linux_backend_returns_release() {
        let backend = LinuxBackend::new();
        assert!(!backend.os_release().name.is_empty());
    }
}
