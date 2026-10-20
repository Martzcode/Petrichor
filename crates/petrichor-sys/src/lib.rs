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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CpuTimes {
    pub total: u64,
    pub idle: u64,
}

impl CpuTimes {
    pub fn usage_percent(&self, previous: CpuTimes) -> f64 {
        let total = self.total.saturating_sub(previous.total);
        let idle = self.idle.saturating_sub(previous.idle);
        if total == 0 {
            return 0.0;
        }
        let busy = total.saturating_sub(idle);
        (busy as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CpuSnapshot {
    pub overall: CpuTimes,
    pub cores: Vec<CpuTimes>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemoryInfo {
    pub total_kb: u64,
    pub available_kb: u64,
    pub free_kb: u64,
    pub buffers_kb: u64,
    pub cached_kb: u64,
}

impl MemoryInfo {
    pub fn used_kb(&self) -> u64 {
        self.total_kb.saturating_sub(self.available_kb)
    }

    pub fn used_percent(&self) -> f64 {
        if self.total_kb == 0 {
            return 0.0;
        }
        (self.used_kb() as f64 / self.total_kb as f64 * 100.0).clamp(0.0, 100.0)
    }
}

pub trait SystemBackend: Send + Sync {
    fn hostname(&self) -> String;
    fn user_name(&self) -> String;
    fn os_release(&self) -> OsRelease;
    fn uptime_secs(&self) -> u64;

    fn cpu_snapshot(&self) -> CpuSnapshot {
        CpuSnapshot::default()
    }

    fn memory_info(&self) -> MemoryInfo {
        MemoryInfo::default()
    }
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

    fn cpu_snapshot(&self) -> CpuSnapshot {
        fs::read_to_string("/proc/stat")
            .map(|text| parse_cpu_snapshot(&text))
            .unwrap_or_default()
    }

    fn memory_info(&self) -> MemoryInfo {
        fs::read_to_string("/proc/meminfo")
            .map(|text| parse_memory_info(&text))
            .unwrap_or_default()
    }
}

pub fn parse_cpu_snapshot(text: &str) -> CpuSnapshot {
    let mut snapshot = CpuSnapshot::default();
    let mut cores = Vec::new();

    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(label) = fields.next() else { continue };
        if !label.starts_with("cpu") {
            break;
        }
        let values: Vec<u64> = fields
            .take(8)
            .map(|value| value.parse().unwrap_or(0))
            .collect();
        let Some(times) = cpu_times(&values) else {
            continue;
        };
        if label == "cpu" {
            snapshot.overall = times;
        } else {
            cores.push(times);
        }
    }

    snapshot.cores = cores;
    snapshot
}

fn cpu_times(values: &[u64]) -> Option<CpuTimes> {
    if values.len() < 4 {
        return None;
    }
    let idle = values[3] + values.get(4).copied().unwrap_or(0);
    let total: u64 = values.iter().take(8).sum();
    Some(CpuTimes { total, idle })
}

pub fn parse_memory_info(text: &str) -> MemoryInfo {
    let mut info = MemoryInfo::default();

    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let amount = value
            .split_whitespace()
            .next()
            .and_then(|number| number.parse::<u64>().ok())
            .unwrap_or(0);
        match key.trim() {
            "MemTotal" => info.total_kb = amount,
            "MemFree" => info.free_kb = amount,
            "MemAvailable" => info.available_kb = amount,
            "Buffers" => info.buffers_kb = amount,
            "Cached" => info.cached_kb = amount,
            _ => {}
        }
    }

    if info.available_kb == 0 {
        info.available_kb = info.free_kb + info.buffers_kb + info.cached_kb;
    }

    info
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

    #[test]
    fn parses_cpu_snapshot() {
        let text = "cpu  10 20 30 40 50 60 70 80 90 100\ncpu0 1 2 3 4 5 6 7 8\ncpu1 2 3 4 5 6 7 8 9\nintr 0\n";
        let snapshot = parse_cpu_snapshot(text);
        assert_eq!(
            snapshot.overall.total,
            10 + 20 + 30 + 40 + 50 + 60 + 70 + 80
        );
        assert_eq!(snapshot.overall.idle, 40 + 50);
        assert_eq!(snapshot.cores.len(), 2);
        assert_eq!(snapshot.cores[0].idle, 4 + 5);
    }

    #[test]
    fn computes_cpu_usage() {
        let previous = CpuTimes {
            total: 100,
            idle: 50,
        };
        let current = CpuTimes {
            total: 200,
            idle: 60,
        };
        assert!((current.usage_percent(previous) - 90.0).abs() < 0.001);
    }

    #[test]
    fn parses_memory_info() {
        let text = "MemTotal:       16000000 kB\nMemFree:         8000000 kB\nMemAvailable:   12000000 kB\nBuffers:          100000 kB\nCached:          200000 kB\n";
        let info = parse_memory_info(text);
        assert_eq!(info.total_kb, 16_000_000);
        assert_eq!(info.available_kb, 12_000_000);
        assert_eq!(info.used_kb(), 4_000_000);
        assert!((info.used_percent() - 25.0).abs() < 0.001);
    }

    #[test]
    fn falls_back_when_available_missing() {
        let text = "MemTotal: 1000 kB\nMemFree: 100 kB\nBuffers: 50 kB\nCached: 50 kB\n";
        let info = parse_memory_info(text);
        assert_eq!(info.available_kb, 200);
    }

    #[test]
    fn host_cpu_and_memory_available() {
        let backend = LinuxBackend::new();
        let cpu = backend.cpu_snapshot();
        assert!(cpu.overall.total > 0);
        let memory = backend.memory_info();
        assert!(memory.total_kb > 0);
    }
}
