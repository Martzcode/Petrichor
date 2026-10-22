use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

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
    pub swap_total_kb: u64,
    pub swap_free_kb: u64,
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

    pub fn swap_used_kb(&self) -> u64 {
        self.swap_total_kb.saturating_sub(self.swap_free_kb)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiskSpaceInfo {
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub available_bytes: u64,
    pub fs_type: String,
    pub volume_type: String,
    pub label: String,
    pub removable: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetSample {
    pub interface: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrashInfo {
    pub count: u64,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WifiNetwork {
    pub ssid: String,
    pub quality: f64,
    pub signal_dbm: i32,
    pub security: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WifiInfo {
    pub connected: bool,
    pub ssid: String,
    pub quality: f64,
    pub signal_dbm: i32,
    pub encryption: String,
    pub auth: String,
    pub phy: String,
    pub tx_rate_kbps: f64,
    pub rx_rate_kbps: f64,
    pub networks: Vec<WifiNetwork>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MediaPlayerInfo {
    pub bus_name: String,
    pub identity: String,
    pub desktop_entry: String,
    pub status: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NowPlayingInfo {
    pub bus_name: String,
    pub identity: String,
    pub status: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: String,
    pub file: String,
    pub lyrics: String,
    pub genre: String,
    pub track_id: String,
    pub duration_secs: f64,
    pub position_secs: f64,
    pub rating: f64,
    pub track_number: i64,
    pub year: i64,
    pub volume: f64,
    pub shuffle: bool,
    pub repeat: String,
}

impl NowPlayingInfo {
    pub fn state(&self) -> i32 {
        match self.status.as_str() {
            "Playing" => 1,
            "Paused" => 2,
            _ => 0,
        }
    }

    pub fn progress(&self) -> f64 {
        if self.duration_secs <= 0.0 {
            0.0
        } else {
            (self.position_secs / self.duration_secs * 100.0).clamp(0.0, 100.0)
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SysInfoValue {
    pub number: f64,
    pub text: String,
}

impl SysInfoValue {
    pub fn text(value: impl Into<String>) -> Self {
        SysInfoValue {
            number: 0.0,
            text: value.into(),
        }
    }

    pub fn number(value: f64) -> Self {
        SysInfoValue {
            number: value,
            text: String::new(),
        }
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

    fn disk_space(&self, _drive: &str, _disk_quota: bool) -> Option<DiskSpaceInfo> {
        None
    }

    fn net_sample(&self, _interface: &str) -> Option<NetSample> {
        None
    }

    fn net_interfaces(&self) -> Vec<String> {
        Vec::new()
    }

    fn process_names(&self) -> Vec<String> {
        Vec::new()
    }

    fn trash_info(&self) -> TrashInfo {
        TrashInfo::default()
    }

    fn wifi_info(&self) -> WifiInfo {
        WifiInfo::default()
    }

    fn wifi_info_for(&self, _index: usize) -> WifiInfo {
        self.wifi_info()
    }

    fn sysinfo(&self, _kind: &str, _data: &str) -> Option<SysInfoValue> {
        None
    }

    fn media_key(&self, _command: &str) -> bool {
        false
    }

    fn media_players(&self) -> Vec<MediaPlayerInfo> {
        Vec::new()
    }

    fn now_playing(&self, _player: &str) -> Option<NowPlayingInfo> {
        None
    }

    fn media_control(&self, _player: &str, _command: &str) -> bool {
        false
    }

    fn media_cover(&self, _file: &str, _art_url: &str) -> Option<String> {
        None
    }

    fn registry_value(&self, _path: &str) -> Option<String> {
        None
    }

    fn empty_trash(&self) -> bool {
        false
    }

    fn open_trash(&self) -> bool {
        false
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

    fn disk_space(&self, drive: &str, disk_quota: bool) -> Option<DiskSpaceInfo> {
        disk_space(drive, disk_quota)
    }

    fn net_sample(&self, interface: &str) -> Option<NetSample> {
        fs::read_to_string("/proc/net/dev")
            .ok()
            .and_then(|text| select_net_sample(&parse_net_dev(&text), interface))
    }

    fn net_interfaces(&self) -> Vec<String> {
        fs::read_dir("/sys/class/net")
            .map(|entries| {
                let mut names: Vec<String> = entries
                    .flatten()
                    .map(|entry| entry.file_name().to_string_lossy().to_string())
                    .filter(|name| name != "lo")
                    .collect();
                names.sort();
                names
            })
            .unwrap_or_default()
    }

    fn process_names(&self) -> Vec<String> {
        fs::read_dir("/proc")
            .map(|entries| {
                entries
                    .flatten()
                    .filter_map(|entry| {
                        let name = entry.file_name().to_string_lossy().to_string();
                        name.parse::<u32>().ok()?;
                        fs::read_to_string(entry.path().join("comm"))
                            .ok()
                            .map(|text| text.trim().to_string())
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn trash_info(&self) -> TrashInfo {
        trash_info()
    }

    fn wifi_info(&self) -> WifiInfo {
        wifi_info()
    }

    fn wifi_info_for(&self, index: usize) -> WifiInfo {
        wifi_info_for(index)
    }

    fn sysinfo(&self, kind: &str, data: &str) -> Option<SysInfoValue> {
        sysinfo(self, kind, data)
    }

    fn media_key(&self, command: &str) -> bool {
        media_key(command)
    }

    fn media_players(&self) -> Vec<MediaPlayerInfo> {
        mpris_media_players()
    }

    fn now_playing(&self, player: &str) -> Option<NowPlayingInfo> {
        mpris_now_playing(player)
    }

    fn media_control(&self, player: &str, command: &str) -> bool {
        mpris_control(player, command)
    }

    fn media_cover(&self, file: &str, art_url: &str) -> Option<String> {
        media_cover(file, art_url)
    }

    fn registry_value(&self, path: &str) -> Option<String> {
        registry_value(path)
    }

    fn empty_trash(&self) -> bool {
        empty_trash()
    }

    fn open_trash(&self) -> bool {
        std::process::Command::new("xdg-open")
            .arg("trash://")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

fn disk_space(drive: &str, disk_quota: bool) -> Option<DiskSpaceInfo> {
    use std::ffi::CString;

    let path = if drive.trim().is_empty() {
        "/"
    } else {
        drive.trim()
    };
    let mount = mount_info(path);
    let c_path = CString::new(path).ok()?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    let result = unsafe { libc::statvfs(c_path.as_ptr(), stats.as_mut_ptr()) };
    if result != 0 {
        return None;
    }
    let stats = unsafe { stats.assume_init() };
    let unit = if stats.f_frsize == 0 {
        stats.f_bsize
    } else {
        stats.f_frsize
    };
    let total_bytes = (stats.f_blocks as u128 * unit as u128).min(u64::MAX as u128) as u64;
    let free_bytes = (stats.f_bfree as u128 * unit as u128).min(u64::MAX as u128) as u64;
    let available_bytes = if disk_quota {
        (stats.f_bavail as u128 * unit as u128).min(u64::MAX as u128) as u64
    } else {
        free_bytes
    };

    let (fs_type, device) = mount.unwrap_or_default();
    let volume_type = volume_type(&fs_type, &device);
    let removable = volume_type == "Removable" || volume_type == "CDRom";
    let label = volume_label(&device);

    Some(DiskSpaceInfo {
        total_bytes,
        free_bytes,
        available_bytes,
        fs_type,
        volume_type,
        label,
        removable,
    })
}

fn mount_info(path: &str) -> Option<(String, String)> {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.into());
    let canonical = canonical.to_string_lossy();
    let text = fs::read_to_string("/proc/mounts").ok()?;
    let mut best: Option<(usize, String, String)> = None;
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let device = fields.next().unwrap_or_default();
        let mount = fields.next().unwrap_or_default();
        let fs_type = fields.next().unwrap_or_default();
        let matches =
            canonical == mount || (mount != "/" && canonical.starts_with(&format!("{mount}/")));
        if matches && best.as_ref().is_none_or(|(len, _, _)| mount.len() > *len) {
            best = Some((mount.len(), fs_type.to_string(), device.to_string()));
        }
    }
    best.map(|(_, fs_type, device)| (fs_type, device))
}

fn volume_type(fs_type: &str, device: &str) -> String {
    let lower = fs_type.to_ascii_lowercase();
    if matches!(lower.as_str(), "tmpfs" | "ramfs") {
        return "Ram".to_string();
    }
    if matches!(lower.as_str(), "iso9660" | "udf") || device.starts_with("/dev/sr") {
        return "CDRom".to_string();
    }
    if ["nfs", "cifs", "smb", "sshfs", "fuse.sshfs"]
        .iter()
        .any(|kind| lower == *kind || lower.starts_with(&format!("{kind}.")))
    {
        return "Network".to_string();
    }
    let name = std::path::Path::new(device)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let removable = fs::read_to_string(format!("/sys/block/{name}/removable"))
        .map(|value| value.trim() == "1")
        .unwrap_or(false);
    if removable || device.starts_with("/dev/sr") {
        "Removable".to_string()
    } else {
        "Fixed".to_string()
    }
}

fn volume_label(device: &str) -> String {
    let Some(name) = std::path::Path::new(device).file_name() else {
        return String::new();
    };
    let target = std::path::Path::new("/dev/disk/by-label");
    let Ok(entries) = fs::read_dir(target) else {
        return String::new();
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if let Ok(link) = fs::read_link(&path) {
            if link.file_name() == Some(name) {
                return entry.file_name().to_string_lossy().to_string();
            }
        }
    }
    String::new()
}

pub fn parse_net_dev(text: &str) -> Vec<NetSample> {
    let mut samples = Vec::new();
    for line in text.lines() {
        let Some((interface, values)) = line.split_once(':') else {
            continue;
        };
        let interface = interface.trim();
        let fields: Vec<u64> = values
            .split_whitespace()
            .map(|value| value.parse().unwrap_or(0))
            .collect();
        if fields.len() < 9 {
            continue;
        }
        samples.push(NetSample {
            interface: interface.to_string(),
            rx_bytes: fields[0],
            tx_bytes: fields[8],
        });
    }
    samples
}

pub fn select_net_sample(samples: &[NetSample], interface: &str) -> Option<NetSample> {
    let requested = interface.trim();
    if requested.is_empty() || requested.eq_ignore_ascii_case("best") {
        return samples
            .iter()
            .filter(|sample| sample.interface != "lo")
            .max_by_key(|sample| sample.rx_bytes.saturating_add(sample.tx_bytes))
            .cloned();
    }
    if let Ok(index) = requested.parse::<usize>() {
        return samples.get(index).cloned();
    }
    samples
        .iter()
        .find(|sample| sample.interface.eq_ignore_ascii_case(requested))
        .cloned()
}

fn trash_info() -> TrashInfo {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| home.map(|path| path.join(".local/share")));
    let Some(root) = data.map(|path| path.join("Trash/files")) else {
        return TrashInfo::default();
    };
    let mut info = TrashInfo::default();
    let Ok(entries) = fs::read_dir(root) else {
        return info;
    };
    for entry in entries.flatten() {
        info.count += 1;
        info.size_bytes = info
            .size_bytes
            .saturating_add(directory_size(&entry.path()));
    }
    info
}

fn directory_size(path: &std::path::Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.is_file() {
        return metadata.len();
    }
    if !metadata.is_dir() {
        return 0;
    }
    fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| directory_size(&entry.path()))
                .fold(0u64, u64::saturating_add)
        })
        .unwrap_or(0)
}

fn wifi_info() -> WifiInfo {
    wifi_info_dbus(None).unwrap_or_else(|| wifi_info_command(None))
}

fn wifi_info_for(index: usize) -> WifiInfo {
    let interfaces = wifi_interfaces();
    let Some(interface) = interfaces.get(index) else {
        return if interfaces.is_empty() {
            wifi_info()
        } else {
            WifiInfo::default()
        };
    };
    wifi_info_dbus(Some(interface)).unwrap_or_else(|| wifi_info_command(Some(interface)))
}

fn wifi_interfaces() -> Vec<String> {
    if let Some(interfaces) = nm_wifi_devices().map(|devices| {
        devices
            .into_iter()
            .map(|(interface, _)| interface)
            .collect()
    }) {
        return interfaces;
    }
    let output = std::process::Command::new("nmcli")
        .args(["-t", "-f", "DEVICE,TYPE", "device", "status"])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    parse_wifi_interfaces(&text)
}

const NM_DEST: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const NM_IFACE: &str = "org.freedesktop.NetworkManager";
const NM_DEVICE_IFACE: &str = "org.freedesktop.NetworkManager.Device";
const NM_WIRELESS_IFACE: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const NM_AP_IFACE: &str = "org.freedesktop.NetworkManager.AccessPoint";

fn dbus_property<T: TryFrom<zbus::zvariant::OwnedValue>>(
    connection: &zbus::blocking::Connection,
    destination: &str,
    path: &str,
    interface: &str,
    property: &str,
) -> Option<T> {
    let reply = connection
        .call_method(
            Some(destination),
            path,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(interface, property),
        )
        .ok()?;
    let value: zbus::zvariant::OwnedValue = reply.body().deserialize().ok()?;
    T::try_from(value).ok()
}

fn nm_property<T: TryFrom<zbus::zvariant::OwnedValue>>(
    connection: &zbus::blocking::Connection,
    path: &str,
    interface: &str,
    property: &str,
) -> Option<T> {
    dbus_property(connection, NM_DEST, path, interface, property)
}

fn nm_wifi_devices() -> Option<Vec<(String, String)>> {
    let connection = zbus::blocking::Connection::system().ok()?;
    let paths: Vec<zbus::zvariant::OwnedObjectPath> =
        nm_property(&connection, NM_PATH, NM_IFACE, "Devices")?;
    let mut devices = Vec::new();
    for path in paths {
        let path = path.as_str().to_string();
        let Some(device_type) =
            nm_property::<u32>(&connection, &path, NM_DEVICE_IFACE, "DeviceType")
        else {
            continue;
        };
        if device_type != 2 {
            continue;
        }
        let Some(interface) =
            nm_property::<String>(&connection, &path, NM_DEVICE_IFACE, "Interface")
        else {
            continue;
        };
        if !interface.is_empty() {
            devices.push((interface, path));
        }
    }
    devices.sort();
    Some(devices)
}

fn nm_access_point(connection: &zbus::blocking::Connection, path: &str) -> WifiNetwork {
    let ssid: Vec<u8> = nm_property(connection, path, NM_AP_IFACE, "Ssid").unwrap_or_default();
    let ssid = String::from_utf8_lossy(&ssid)
        .trim_end_matches('\0')
        .to_string();
    let quality: f64 = nm_property::<u8>(connection, path, NM_AP_IFACE, "Strength")
        .map(f64::from)
        .unwrap_or(0.0);
    let flags: u32 = nm_property(connection, path, NM_AP_IFACE, "Flags").unwrap_or(0);
    let wpa: u32 = nm_property(connection, path, NM_AP_IFACE, "WpaFlags").unwrap_or(0);
    let rsn: u32 = nm_property(connection, path, NM_AP_IFACE, "RsnFlags").unwrap_or(0);
    WifiNetwork {
        ssid,
        quality,
        signal_dbm: quality_to_dbm(quality),
        security: nm_security(flags, wpa, rsn),
    }
}

fn nm_security(flags: u32, wpa: u32, rsn: u32) -> String {
    if rsn & 0x400 != 0 {
        "WPA3".to_string()
    } else if rsn != 0 {
        "WPA2".to_string()
    } else if wpa != 0 {
        "WPA".to_string()
    } else if flags & 1 != 0 {
        "WEP".to_string()
    } else {
        String::new()
    }
}

fn wifi_info_dbus(interface: Option<&String>) -> Option<WifiInfo> {
    let connection = zbus::blocking::Connection::system().ok()?;
    let devices = nm_wifi_devices()?;
    let selected = match interface {
        Some(interface) => devices.iter().find(|(name, _)| name == interface).cloned(),
        None => devices.first().cloned(),
    }?;
    let (name, path) = selected;
    let active_path: Option<zbus::zvariant::OwnedObjectPath> =
        nm_property(&connection, &path, NM_WIRELESS_IFACE, "ActiveAccessPoint");
    let active_path = active_path
        .map(|value| value.as_str().to_string())
        .filter(|value| value != "/");
    let ap_paths: Vec<zbus::zvariant::OwnedObjectPath> = connection
        .call_method(
            Some(NM_DEST),
            path.as_str(),
            Some(NM_WIRELESS_IFACE),
            "GetAccessPoints",
            &(),
        )
        .ok()
        .and_then(|reply| reply.body().deserialize().ok())
        .unwrap_or_default();

    let mut info = WifiInfo::default();
    for ap_path in ap_paths {
        let ap_path = ap_path.as_str().to_string();
        let network = nm_access_point(&connection, &ap_path);
        if network.ssid.is_empty() {
            continue;
        }
        if active_path.as_deref() == Some(ap_path.as_str()) {
            info.connected = true;
            info.ssid = network.ssid.clone();
            info.quality = network.quality;
            info.signal_dbm = network.signal_dbm;
            info.encryption = network.security.clone();
            info.auth = network.security.clone();
        }
        info.networks.push(network);
    }
    info.networks
        .sort_by(|left, right| right.quality.total_cmp(&left.quality));

    let frequency: u32 = active_path
        .as_deref()
        .and_then(|path| nm_property(&connection, path, NM_AP_IFACE, "Frequency"))
        .unwrap_or(0);
    let max_bitrate: u32 = active_path
        .as_deref()
        .and_then(|path| nm_property(&connection, path, NM_AP_IFACE, "MaxBitrate"))
        .unwrap_or(0);
    let device_bitrate: u32 =
        nm_property(&connection, &path, NM_WIRELESS_IFACE, "Bitrate").unwrap_or(0);
    let mut phy = phy_label(frequency, f64::from(max_bitrate));
    let mut tx = f64::from(device_bitrate.max(max_bitrate));
    let mut rx = 0.0;
    if info.connected {
        if let Some(link) = read_iw_link(&name) {
            if !link.phy.is_empty() {
                phy = link.phy;
            }
            if link.tx_kbps > 0.0 {
                tx = link.tx_kbps;
            }
            rx = link.rx_kbps;
            if let Some(signal) = link.signal_dbm {
                info.signal_dbm = signal;
            }
        }
    }
    info.phy = phy;
    info.tx_rate_kbps = tx;
    info.rx_rate_kbps = rx;
    Some(info)
}

struct IwLink {
    phy: String,
    tx_kbps: f64,
    rx_kbps: f64,
    signal_dbm: Option<i32>,
}

fn read_iw_link(interface: &str) -> Option<IwLink> {
    let output = std::process::Command::new("iw")
        .args(["dev", interface, "link"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(parse_iw_link(&String::from_utf8_lossy(&output.stdout)))
}

fn parse_iw_link(text: &str) -> IwLink {
    let mut link = IwLink {
        phy: String::new(),
        tx_kbps: 0.0,
        rx_kbps: 0.0,
        signal_dbm: None,
    };
    let mut frequency = 0.0;
    for line in text.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("freq:") {
            frequency = value
                .split_whitespace()
                .next()
                .and_then(|value| value.parse().ok())
                .unwrap_or(0.0);
        } else if let Some(value) = line.strip_prefix("signal:") {
            link.signal_dbm = value
                .split_whitespace()
                .next()
                .and_then(|value| value.parse().ok());
        } else if let Some(value) = line.strip_prefix("rx bitrate:") {
            link.rx_kbps = parse_iw_bitrate(value);
        } else if let Some(value) = line.strip_prefix("tx bitrate:") {
            link.tx_kbps = parse_iw_bitrate(value);
        }
    }
    link.phy = phy_label(frequency as u32, link.tx_kbps.max(link.rx_kbps));
    link
}

fn parse_iw_bitrate(value: &str) -> f64 {
    let mut fields = value.split_whitespace();
    let Some(rate) = fields.next().and_then(|value| value.parse::<f64>().ok()) else {
        return 0.0;
    };
    match fields.next().map(str::to_ascii_lowercase).as_deref() {
        Some("gbit/s") => rate * 1_000_000.0,
        Some("kbit/s") => rate,
        _ => rate * 1_000.0,
    }
}

fn phy_label(frequency: u32, bitrate_kbps: f64) -> String {
    if frequency >= 5925 {
        return "802.11ax".to_string();
    }
    if frequency >= 3000 {
        if bitrate_kbps >= 1_000_000.0 {
            "802.11ax".to_string()
        } else if bitrate_kbps >= 300_000.0 {
            "802.11ac".to_string()
        } else if bitrate_kbps >= 100_000.0 {
            "802.11n".to_string()
        } else {
            "802.11a".to_string()
        }
    } else if bitrate_kbps >= 500_000.0 {
        "802.11ax".to_string()
    } else if bitrate_kbps >= 100_000.0 {
        "802.11n".to_string()
    } else if bitrate_kbps > 0.0 {
        "802.11g".to_string()
    } else {
        String::new()
    }
}

fn parse_wifi_interfaces(text: &str) -> Vec<String> {
    let mut interfaces: Vec<String> = text
        .lines()
        .filter_map(|line| {
            let fields = split_nmcli(line);
            if fields.len() >= 2 && fields[1] == "wifi" && !fields[0].is_empty() {
                Some(fields[0].clone())
            } else {
                None
            }
        })
        .collect();
    interfaces.sort();
    interfaces
}

fn wifi_info_command(interface: Option<&String>) -> WifiInfo {
    let mut command = std::process::Command::new("nmcli");
    command.args([
        "-t",
        "-f",
        "IN-USE,SSID,SIGNAL,SECURITY",
        "dev",
        "wifi",
        "list",
        "--rescan",
        "no",
    ]);
    if let Some(interface) = interface {
        command.args(["ifname", interface]);
    }
    let Ok(output) = command.output() else {
        return WifiInfo::default();
    };
    parse_nmcli(&String::from_utf8_lossy(&output.stdout))
}

pub fn parse_nmcli(text: &str) -> WifiInfo {
    let mut info = WifiInfo::default();
    for line in text.lines() {
        let mut fields = split_nmcli(line).into_iter();
        let active = fields.next().unwrap_or_default();
        let ssid = fields.next().unwrap_or_default();
        let quality = fields
            .next()
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(0.0);
        let security = fields.next().unwrap_or_default();
        if ssid.is_empty() {
            continue;
        }
        let network = WifiNetwork {
            ssid: ssid.clone(),
            quality,
            signal_dbm: quality_to_dbm(quality),
            security: security.clone(),
        };
        if active.trim() == "*" && !info.connected {
            info.connected = true;
            info.ssid = ssid;
            info.quality = quality;
            info.signal_dbm = network.signal_dbm;
            info.encryption = security.clone();
            info.auth = security;
        }
        info.networks.push(network);
    }
    info
}

fn split_nmcli(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    for ch in line.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == ':' {
            fields.push(std::mem::take(&mut current));
        } else {
            current.push(ch);
        }
    }
    fields.push(current);
    fields
}

fn quality_to_dbm(quality: f64) -> i32 {
    ((quality / 100.0) * 70.0 - 100.0).round() as i32
}

fn sysinfo(backend: &LinuxBackend, kind: &str, data: &str) -> Option<SysInfoValue> {
    let kind = kind.trim().to_ascii_uppercase();
    let interfaces = backend.net_interfaces();
    let index = data.trim().parse::<usize>().ok();
    let interface = if data.trim().is_empty() || data.trim() == "-1" {
        interfaces.first().cloned()
    } else if let Some(index) = index {
        interfaces.get(index).cloned()
    } else {
        Some(data.trim().to_string())
    };

    match kind.as_str() {
        "COMPUTER_NAME" | "HOST_NAME" => Some(SysInfoValue::text(backend.hostname())),
        "USER_NAME" => Some(SysInfoValue::text(backend.user_name())),
        "USER_SID" => Some(SysInfoValue::text(unsafe { libc::getuid() }.to_string())),
        "OS_VERSION" => {
            let release = backend.os_release();
            Some(SysInfoValue::text(if release.version.is_empty() {
                release.name
            } else {
                release.version
            }))
        }
        "OS_PRODUCT_NAME" => Some(SysInfoValue::text(backend.os_release().name)),
        "OS_BITS" => Some(SysInfoValue::number(f64::from(usize::BITS))),
        "PAGESIZE" => Some(SysInfoValue::number(
            unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as f64,
        )),
        "NUM_MONITORS" => {
            let screens = screen_info();
            Some(SysInfoValue::number(screens.monitors.len().max(1) as f64))
        }
        "SCREEN_SIZE" => {
            let (_, _, width, height) = selected_monitor(&screen_info(), index);
            Some(SysInfoValue::text(format!("{width:.0}x{height:.0}")))
        }
        "WORK_AREA" => {
            let (left, top, width, height) = selected_monitor(&screen_info(), index);
            Some(SysInfoValue::text(format!(
                "{left:.0},{top:.0},{width:.0},{height:.0}"
            )))
        }
        "SCREEN_WIDTH" | "WORK_AREA_WIDTH" => {
            let (_, _, width, _) = selected_monitor(&screen_info(), index);
            Some(SysInfoValue::number(width))
        }
        "SCREEN_HEIGHT" | "WORK_AREA_HEIGHT" => {
            let (_, _, _, height) = selected_monitor(&screen_info(), index);
            Some(SysInfoValue::number(height))
        }
        "WORK_AREA_LEFT" => {
            let (left, _, _, _) = selected_monitor(&screen_info(), index);
            Some(SysInfoValue::number(left))
        }
        "WORK_AREA_TOP" => {
            let (_, top, _, _) = selected_monitor(&screen_info(), index);
            Some(SysInfoValue::number(top))
        }
        "VIRTUAL_SCREEN_LEFT" => {
            let screens = screen_info();
            let left = screens
                .monitors
                .iter()
                .map(|monitor| monitor.0)
                .fold(f64::INFINITY, f64::min);
            Some(SysInfoValue::number(if left.is_finite() {
                left
            } else {
                0.0
            }))
        }
        "VIRTUAL_SCREEN_TOP" => {
            let screens = screen_info();
            let top = screens
                .monitors
                .iter()
                .map(|monitor| monitor.1)
                .fold(f64::INFINITY, f64::min);
            Some(SysInfoValue::number(if top.is_finite() {
                top
            } else {
                0.0
            }))
        }
        "VIRTUAL_SCREEN_WIDTH" => Some(SysInfoValue::number(screen_info().width)),
        "VIRTUAL_SCREEN_HEIGHT" => Some(SysInfoValue::number(screen_info().height)),
        "DNS_SERVER" => Some(SysInfoValue::text(
            fs::read_to_string("/etc/resolv.conf")
                .ok()
                .and_then(|text| {
                    text.lines()
                        .find_map(|line| line.trim().strip_prefix("nameserver "))
                        .map(|value| {
                            value
                                .split_whitespace()
                                .next()
                                .unwrap_or_default()
                                .to_string()
                        })
                })
                .unwrap_or_default(),
        )),
        "DOMAIN_NAME" => Some(SysInfoValue::text(domain_name())),
        "DOMAIN_WORKGROUP" => Some(SysInfoValue::text(String::new())),
        "INTERNET_CONNECTIVITY"
        | "INTERNET_CONNECTIVITY_V4"
        | "INTERNET_CONNECTIVITY_V6"
        | "LAN_CONNECTIVITY"
        | "LAN_CONNECTIVITY_V4"
        | "LAN_CONNECTIVITY_V6" => Some(SysInfoValue::number(if has_default_route() {
            1.0
        } else {
            0.0
        })),
        "ADAPTER_ALIAS" => Some(SysInfoValue::text(interface.unwrap_or_default())),
        "ADAPTER_DESCRIPTION" => Some(SysInfoValue::text(
            interface
                .as_deref()
                .map(adapter_description)
                .unwrap_or_default(),
        )),
        "ADAPTER_GUID" => Some(SysInfoValue::text(
            interface
                .as_deref()
                .and_then(|name| read_sys(format!("/sys/class/net/{name}/address")))
                .unwrap_or_default(),
        )),
        "ADAPTER_TYPE" => Some(SysInfoValue::text(
            interface
                .as_deref()
                .map(|name| {
                    if std::path::Path::new(&format!("/sys/class/net/{name}/wireless")).exists() {
                        "Wireless"
                    } else {
                        "Ethernet"
                    }
                    .to_string()
                })
                .unwrap_or_default(),
        )),
        "ADAPTER_STATE" => Some(SysInfoValue::text(
            interface
                .as_deref()
                .and_then(|name| read_sys(format!("/sys/class/net/{name}/operstate")))
                .unwrap_or_default(),
        )),
        "ADAPTER_STATUS" => Some(SysInfoValue::text(
            interface
                .as_deref()
                .map(|name| {
                    if read_sys(format!("/sys/class/net/{name}/operstate"))
                        .is_some_and(|state| state == "up")
                    {
                        "Connected"
                    } else {
                        "Disconnected"
                    }
                    .to_string()
                })
                .unwrap_or_default(),
        )),
        "ADAPTER_TRANSMIT_SPEED" | "ADAPTER_RECEIVE_SPEED" => Some(SysInfoValue::number(
            interface
                .as_deref()
                .and_then(|name| read_sys(format!("/sys/class/net/{name}/speed")))
                .and_then(|value| value.parse::<f64>().ok())
                .map(|mbps| mbps * 1_000_000.0)
                .unwrap_or(0.0),
        )),
        "MAC_ADDRESS" => Some(SysInfoValue::text(
            interface
                .as_deref()
                .and_then(|name| read_sys(format!("/sys/class/net/{name}/address")))
                .unwrap_or_default(),
        )),
        "IP_ADDRESS" => Some(SysInfoValue::text(
            interface
                .as_deref()
                .map(|name| ipv4_info(name).0)
                .unwrap_or_default(),
        )),
        "NET_MASK" => Some(SysInfoValue::text(
            interface
                .as_deref()
                .map(|name| ipv4_info(name).1)
                .unwrap_or_default(),
        )),
        "GATEWAY_ADDRESS" | "GATEWAY_ADDRESS_V4" => Some(SysInfoValue::text(
            interface
                .as_deref()
                .and_then(default_gateway)
                .unwrap_or_default(),
        )),
        "GATEWAY_ADDRESS_V6" => Some(SysInfoValue::text(String::new())),
        "TIMEZONE_ISDST" => Some(SysInfoValue::number(if local_isdst() { 1.0 } else { 0.0 })),
        "TIMEZONE_BIAS" => Some(SysInfoValue::number(f64::from(-local_offset_minutes()))),
        "TIMEZONE_STANDARD_BIAS" => Some(SysInfoValue::number(f64::from(
            -(local_offset_minutes() - if local_isdst() { 60 } else { 0 }),
        ))),
        "TIMEZONE_DAYLIGHT_BIAS" => Some(SysInfoValue::number(60.0)),
        "TIMEZONE_STANDARD_NAME" | "TIMEZONE_DAYLIGHT_NAME" => {
            Some(SysInfoValue::text(timezone_name()))
        }
        "IDLE_TIME" => Some(SysInfoValue::number(idle_time_seconds())),
        "USER_LOGONTIME" => Some(SysInfoValue::number(user_logon_time())),
        "LAST_SLEEP_TIME" | "LAST_WAKE_TIME" => {
            let (sleep, wake) = sleep_wake_times();
            Some(SysInfoValue::number(if kind == "LAST_SLEEP_TIME" {
                unix_to_filetime(sleep)
            } else {
                unix_to_filetime(wake)
            }))
        }
        _ => None,
    }
}

const WINDOWS_EPOCH_OFFSET: f64 = 11_644_473_600.0;

fn unix_to_filetime(seconds: f64) -> f64 {
    if seconds > 0.0 {
        seconds + WINDOWS_EPOCH_OFFSET
    } else {
        0.0
    }
}

fn idle_time_seconds() -> f64 {
    if let Ok(value) = std::env::var("PETRICHOR_IDLE_SECONDS") {
        if let Ok(value) = value.parse::<f64>() {
            return value.max(0.0);
        }
    }
    if let Ok(output) = std::process::Command::new("xprintidle").output() {
        if output.status.success() {
            if let Ok(milliseconds) = String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<f64>()
            {
                return (milliseconds / 1000.0).max(0.0);
            }
        }
    }
    if let Some(seconds) = screensaver_idle_seconds() {
        return seconds;
    }
    if let Some(seconds) = mutter_idle_seconds() {
        return seconds;
    }
    logind_idle_seconds().unwrap_or(0.0)
}

fn screensaver_idle_seconds() -> Option<f64> {
    let connection = zbus::blocking::Connection::session().ok()?;
    for destination in ["org.freedesktop.ScreenSaver", "org.kde.screensaver"] {
        if let Ok(reply) = connection.call_method(
            Some(destination),
            "/org/freedesktop/ScreenSaver",
            Some("org.freedesktop.ScreenSaver"),
            "GetSessionIdleTime",
            &(),
        ) {
            if let Ok(seconds) = reply.body().deserialize::<u32>() {
                return Some(f64::from(seconds));
            }
        }
    }
    None
}

fn mutter_idle_seconds() -> Option<f64> {
    let connection = zbus::blocking::Connection::session().ok()?;
    let reply = connection
        .call_method(
            Some("org.gnome.Mutter.IdleMonitor"),
            "/org/gnome/Mutter/IdleMonitor/Core",
            Some("org.gnome.Mutter.IdleMonitor"),
            "GetIdletime",
            &(),
        )
        .ok()?;
    let milliseconds: u64 = reply.body().deserialize().ok()?;
    Some(milliseconds as f64 / 1000.0)
}

fn logind_idle_seconds() -> Option<f64> {
    let connection = zbus::blocking::Connection::system().ok()?;
    let path = logind_session_path(&connection)?;
    let since: u64 = dbus_property(
        &connection,
        "org.freedesktop.login1",
        &path,
        "org.freedesktop.login1.Session",
        "IdleSinceHint",
    )?;
    if since == 0 {
        return Some(0.0);
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs_f64();
    Some((now - since as f64 / 1_000_000.0).max(0.0))
}

fn user_logon_time() -> f64 {
    if let Ok(value) = std::env::var("PETRICHOR_LOGONTIME_UNIX") {
        if let Ok(value) = value.parse::<f64>() {
            return unix_to_filetime(value);
        }
    }
    static CACHE: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *CACHE.get_or_init(|| {
        let Some(connection) = zbus::blocking::Connection::system().ok() else {
            return 0.0;
        };
        let Some(path) = logind_session_path(&connection) else {
            return 0.0;
        };
        let timestamp: u64 = dbus_property(
            &connection,
            "org.freedesktop.login1",
            &path,
            "org.freedesktop.login1.Session",
            "Timestamp",
        )
        .unwrap_or(0);
        if timestamp == 0 {
            0.0
        } else {
            unix_to_filetime(timestamp as f64 / 1_000_000.0)
        }
    })
}

fn logind_session_path(connection: &zbus::blocking::Connection) -> Option<String> {
    if let Ok(session) = std::env::var("XDG_SESSION_ID") {
        if !session.is_empty() {
            if let Ok(reply) = connection.call_method(
                Some("org.freedesktop.login1"),
                "/org/freedesktop/login1",
                Some("org.freedesktop.login1.Manager"),
                "GetSession",
                &(session.as_str()),
            ) {
                if let Ok(path) = reply
                    .body()
                    .deserialize::<zbus::zvariant::OwnedObjectPath>()
                {
                    return Some(path.as_str().to_string());
                }
            }
        }
    }
    let reply = connection
        .call_method(
            Some("org.freedesktop.login1"),
            "/org/freedesktop/login1",
            Some("org.freedesktop.login1.Manager"),
            "GetSessionByPID",
            &(std::process::id()),
        )
        .ok()?;
    let path: zbus::zvariant::OwnedObjectPath = reply.body().deserialize().ok()?;
    Some(path.as_str().to_string())
}

fn sleep_wake_times() -> (f64, f64) {
    let env_value = |name: &str| {
        std::env::var(name)
            .ok()
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    if std::env::var("PETRICHOR_LAST_SLEEP_UNIX").is_ok()
        || std::env::var("PETRICHOR_LAST_WAKE_UNIX").is_ok()
    {
        return (
            env_value("PETRICHOR_LAST_SLEEP_UNIX"),
            env_value("PETRICHOR_LAST_WAKE_UNIX"),
        );
    }

    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};

    struct SleepCache {
        timestamp: Option<Instant>,
        times: (f64, f64),
        loading: bool,
    }

    static CACHE: OnceLock<Mutex<SleepCache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| {
        Mutex::new(SleepCache {
            timestamp: None,
            times: (0.0, 0.0),
            loading: false,
        })
    });
    let Ok(mut guard) = cache.lock() else {
        return (0.0, 0.0);
    };
    let stale = guard
        .timestamp
        .is_none_or(|timestamp| timestamp.elapsed() >= Duration::from_secs(300));
    if stale && !guard.loading {
        guard.loading = true;
        let _ = std::thread::Builder::new()
            .name("petrichor-sleep-times".to_string())
            .spawn(|| {
                let times = read_sleep_wake_times().unwrap_or((0.0, 0.0));
                if let Some(cache) = CACHE.get() {
                    if let Ok(mut guard) = cache.lock() {
                        guard.timestamp = Some(Instant::now());
                        guard.times = times;
                        guard.loading = false;
                    }
                }
            });
    }
    guard.times
}

fn read_sleep_wake_times() -> Option<(f64, f64)> {
    for boot in 0..5 {
        let boot = boot.to_string();
        let output = std::process::Command::new("journalctl")
            .args([
                "--no-pager",
                "-o",
                "short-unix",
                "-b",
                &boot,
                "-n",
                "50",
                "-g",
                "suspend entry|suspend exit|Suspending system|System resumed|Entering sleep state|returned from sleep",
            ])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        let times = parse_sleep_journal(&text);
        if times.0 > 0.0 || times.1 > 0.0 {
            return Some(times);
        }
    }
    Some((0.0, 0.0))
}

fn parse_sleep_journal(text: &str) -> (f64, f64) {
    let mut sleep = 0.0;
    let mut wake = 0.0;
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(timestamp) = fields.next().and_then(|value| value.parse::<f64>().ok()) else {
            continue;
        };
        let message = line.to_ascii_lowercase();
        if message.contains("suspend entry")
            || message.contains("suspending system")
            || message.contains("entering sleep state")
        {
            sleep = timestamp;
        } else if message.contains("suspend exit")
            || message.contains("system resumed")
            || message.contains("returned from sleep")
        {
            wake = timestamp;
        }
    }
    (sleep, wake)
}

fn parse_xrandr_monitors(text: &str) -> Vec<(f64, f64, f64, f64)> {
    let mut monitors = Vec::new();
    for line in text.lines().skip(1) {
        let mut fields = line.split_whitespace();
        let _index = fields.next();
        let _name = fields.next();
        let Some(geometry) = fields.next() else {
            continue;
        };
        let Some((width, rest)) = geometry.split_once('x') else {
            continue;
        };
        let width = width.split('/').next().unwrap_or_default();
        let (height, signs, offsets_text) = match rest.find(['+', '-']) {
            Some(index) => (&rest[..index], &rest[index..], &rest[index + 1..]),
            None => (rest, "", "0+0"),
        };
        let mut signs = signs
            .chars()
            .filter(|character| matches!(character, '+' | '-'));
        let sign_x = signs.next();
        let sign_y = signs.next();
        let height = height.split('/').next().unwrap_or_default();
        let mut offsets = offsets_text.split(['+', '-']);
        let mut left = offsets.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let mut top = offsets.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
        if sign_x == Some('-') {
            left = -left;
        }
        if sign_y == Some('-') {
            top = -top;
        }
        let width = width.parse().unwrap_or(0.0);
        let height = height.parse().unwrap_or(0.0);
        if width > 0.0 && height > 0.0 {
            monitors.push((left, top, width, height));
        }
    }
    monitors
}

#[derive(Debug, Default, Clone)]
struct ScreenInfo {
    monitors: Vec<(f64, f64, f64, f64)>,
    width: f64,
    height: f64,
}

fn selected_monitor(screens: &ScreenInfo, index: Option<usize>) -> (f64, f64, f64, f64) {
    index
        .and_then(|index| screens.monitors.get(index).copied())
        .or_else(|| screens.monitors.first().copied())
        .unwrap_or((0.0, 0.0, 0.0, 0.0))
}

fn screen_info() -> ScreenInfo {
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};

    static CACHE: OnceLock<Mutex<Option<(Instant, ScreenInfo)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cache.lock() {
        if let Some((timestamp, screens)) = guard.as_ref() {
            if timestamp.elapsed() < Duration::from_secs(2) {
                return screens.clone();
            }
        }
    }
    let screens = screen_info_uncached();
    if let Ok(mut guard) = cache.lock() {
        *guard = Some((Instant::now(), screens.clone()));
    }
    screens
}

fn screen_info_uncached() -> ScreenInfo {
    let mut info = ScreenInfo::default();
    if let Ok(output) = std::process::Command::new("xrandr")
        .arg("--listmonitors")
        .output()
    {
        if let Ok(text) = String::from_utf8(output.stdout) {
            info.monitors = parse_xrandr_monitors(&text);
        }
    }
    if info.monitors.is_empty() {
        if let Ok(value) = std::env::var("PETRICHOR_SCREEN_GEOMETRY") {
            let values: Vec<f64> = value
                .split(',')
                .filter_map(|value| value.trim().parse().ok())
                .collect();
            if values.len() >= 4 {
                info.monitors
                    .push((values[0], values[1], values[2], values[3]));
            }
        }
    }
    if info.monitors.is_empty() {
        if let Ok(value) = std::env::var("PETRICHOR_SCREEN_SIZE") {
            let mut parts = value.split(['x', 'X']);
            let width = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
            let height = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
            if width > 0.0 && height > 0.0 {
                info.monitors.push((0.0, 0.0, width, height));
            }
        }
    }
    if info.monitors.is_empty() {
        info.monitors.push((0.0, 0.0, 0.0, 0.0));
    }
    let left = info
        .monitors
        .iter()
        .map(|monitor| monitor.0)
        .fold(f64::INFINITY, f64::min);
    let top = info
        .monitors
        .iter()
        .map(|monitor| monitor.1)
        .fold(f64::INFINITY, f64::min);
    let right = info
        .monitors
        .iter()
        .map(|monitor| monitor.0 + monitor.2)
        .fold(f64::NEG_INFINITY, f64::max);
    let bottom = info
        .monitors
        .iter()
        .map(|monitor| monitor.1 + monitor.3)
        .fold(f64::NEG_INFINITY, f64::max);
    info.width = (right - left).max(0.0);
    info.height = (bottom - top).max(0.0);
    info
}

fn read_sys(path: impl AsRef<std::path::Path>) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
}

fn adapter_description(name: &str) -> String {
    read_sys(format!("/sys/class/net/{name}/device/uevent"))
        .and_then(|text| {
            text.lines().find_map(|line| {
                line.strip_prefix("DRIVER=")
                    .map(|driver| format!("{name} ({driver})"))
            })
        })
        .unwrap_or_else(|| name.to_string())
}

fn domain_name() -> String {
    fs::read_to_string("/etc/resolv.conf")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                let line = line.trim();
                line.strip_prefix("search ")
                    .or_else(|| line.strip_prefix("domain "))
                    .map(|value| {
                        value
                            .split_whitespace()
                            .next()
                            .unwrap_or_default()
                            .to_string()
                    })
            })
        })
        .unwrap_or_default()
}

fn has_default_route() -> bool {
    fs::read_to_string("/proc/net/route")
        .map(|text| {
            text.lines().skip(1).any(|line| {
                let mut fields = line.split_whitespace();
                let _interface = fields.next();
                let destination = fields.next().unwrap_or_default();
                destination == "00000000"
            })
        })
        .unwrap_or(false)
}

fn default_gateway(interface: &str) -> Option<String> {
    let text = fs::read_to_string("/proc/net/route").ok()?;
    for line in text.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 3 || !fields[0].eq_ignore_ascii_case(interface) || fields[1] != "00000000"
        {
            continue;
        }
        let value = u32::from_str_radix(fields[2], 16).ok()?;
        let bytes = value.to_le_bytes();
        return Some(format!(
            "{}.{}.{}.{}",
            bytes[0], bytes[1], bytes[2], bytes[3]
        ));
    }
    None
}

fn ipv4_info(interface: &str) -> (String, String) {
    use std::ffi::CStr;

    let mut result = (String::new(), String::new());
    let mut addresses: *mut libc::ifaddrs = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut addresses) } != 0 {
        return result;
    }
    let mut current = addresses;
    while !current.is_null() {
        let entry = unsafe { &*current };
        if !entry.ifa_name.is_null() {
            let name = unsafe { CStr::from_ptr(entry.ifa_name) }.to_string_lossy();
            if name == interface && !entry.ifa_addr.is_null() {
                let family = unsafe { (*entry.ifa_addr).sa_family as i32 };
                if family == libc::AF_INET {
                    result.0 = sockaddr_ipv4(entry.ifa_addr);
                    result.1 = sockaddr_ipv4(entry.ifa_netmask);
                    break;
                }
            }
        }
        current = entry.ifa_next;
    }
    unsafe { libc::freeifaddrs(addresses) };
    result
}

fn sockaddr_ipv4(address: *const libc::sockaddr) -> String {
    if address.is_null() {
        return String::new();
    }
    let address = unsafe { &*(address as *const libc::sockaddr_in) };
    let bytes = address.sin_addr.s_addr.to_ne_bytes();
    format!("{}.{}.{}.{}", bytes[0], bytes[1], bytes[2], bytes[3])
}

fn local_offset_minutes() -> i32 {
    chrono::Local::now().offset().local_minus_utc() / 60
}

fn local_isdst() -> bool {
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut local = std::mem::zeroed::<libc::tm>();
        libc::localtime_r(&now, &mut local);
        local.tm_isdst > 0
    }
}

fn timezone_name() -> String {
    chrono::Local::now().format("%Z").to_string()
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
            "SwapTotal" => info.swap_total_kb = amount,
            "SwapFree" => info.swap_free_kb = amount,
            _ => {}
        }
    }

    if info.available_kb == 0 {
        info.available_kb = info.free_kb + info.buffers_kb + info.cached_kb;
    }

    info
}

fn empty_trash() -> bool {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return false;
    };
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"))
        .join("Trash");
    let mut removed = false;
    for folder in ["files", "info", "expunged"] {
        let directory = base.join(folder);
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let result = if path.is_dir() {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            };
            removed |= result.is_ok();
        }
    }
    removed
}

pub fn normalize_registry_path(path: &str) -> String {
    let path = path.trim().replace('/', "\\");
    let (hive, rest) = path.split_once('\\').unwrap_or((path.as_str(), ""));
    let hive_upper = hive.trim().to_ascii_uppercase();
    let hive = match hive_upper.as_str() {
        "HKCU" => "HKEY_CURRENT_USER",
        "HKLM" => "HKEY_LOCAL_MACHINE",
        "HKCR" => "HKEY_CLASSES_ROOT",
        "HKU" => "HKEY_USERS",
        "HKCC" => "HKEY_CURRENT_CONFIG",
        other => other,
    };
    if rest.is_empty() {
        hive.to_string()
    } else {
        format!("{hive}\\{rest}")
    }
}

fn registry_value(path: &str) -> Option<String> {
    let file = std::env::var_os("PETRICHOR_REGISTRY")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config"))
                })
                .map(|base| base.join("petrichor").join("registry.json"))
        })?;
    let text = fs::read_to_string(file).ok()?;
    let values: BTreeMap<String, String> = serde_json::from_str(&text).ok()?;
    registry_lookup(&values, path)
}

fn registry_lookup(values: &BTreeMap<String, String>, path: &str) -> Option<String> {
    let wanted = normalize_registry_path(path).to_ascii_lowercase();
    values
        .iter()
        .find(|(key, _)| normalize_registry_path(key).to_ascii_lowercase() == wanted)
        .map(|(_, value)| value.clone())
}

fn media_key(command: &str) -> bool {
    use std::process::{Command, Stdio};

    let run = |program: &str, args: &[&str]| {
        Command::new(program)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    };

    let command = command.trim().to_ascii_lowercase();
    match command.as_str() {
        "playpause" | "play" | "pause" | "stop" | "nexttrack" | "prevtrack" => {
            let method = match command.as_str() {
                "playpause" => "PlayPause",
                "play" => "Play",
                "pause" => "Pause",
                "stop" => "Stop",
                "nexttrack" => "Next",
                _ => "Previous",
            };
            if mpris_action(method) {
                return true;
            }
            let action = match command.as_str() {
                "playpause" => "play-pause",
                "play" => "play",
                "pause" => "pause",
                "stop" => "stop",
                "nexttrack" => "next",
                _ => "previous",
            };
            run("playerctl", &[action])
        }
        "volumemute" => {
            run("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"])
                || run("pactl", &["set-sink-mute", "@DEFAULT_SINK@", "toggle"])
                || run("amixer", &["-q", "sset", "Master", "toggle"])
        }
        "volumeup" => {
            run("wpctl", &["set-volume", "@DEFAULT_AUDIO_SINK@", "5%+"])
                || run("pactl", &["set-sink-volume", "@DEFAULT_SINK@", "+5%"])
                || run("amixer", &["-q", "sset", "Master", "5%+"])
        }
        "volumedown" => {
            run("wpctl", &["set-volume", "@DEFAULT_AUDIO_SINK@", "5%-"])
                || run("pactl", &["set-sink-volume", "@DEFAULT_SINK@", "-5%"])
                || run("amixer", &["-q", "sset", "Master", "5%-"])
        }
        _ => false,
    }
}

const MPRIS_PATH: &str = "/org/mpris/MediaPlayer2";
const MPRIS_ROOT_IFACE: &str = "org.mpris.MediaPlayer2";
const MPRIS_PLAYER_IFACE: &str = "org.mpris.MediaPlayer2.Player";

fn mpris_media_players() -> Vec<MediaPlayerInfo> {
    let Ok(connection) = zbus::blocking::Connection::session() else {
        return Vec::new();
    };
    mpris_players(&connection)
        .into_iter()
        .map(|bus_name| {
            let identity: String = dbus_property(
                &connection,
                &bus_name,
                MPRIS_PATH,
                MPRIS_ROOT_IFACE,
                "Identity",
            )
            .unwrap_or_else(|| bus_suffix(&bus_name));
            let desktop_entry: String = dbus_property(
                &connection,
                &bus_name,
                MPRIS_PATH,
                MPRIS_ROOT_IFACE,
                "DesktopEntry",
            )
            .unwrap_or_default();
            let status: String = dbus_property(
                &connection,
                &bus_name,
                MPRIS_PATH,
                MPRIS_PLAYER_IFACE,
                "PlaybackStatus",
            )
            .unwrap_or_else(|| "Stopped".to_string());
            MediaPlayerInfo {
                bus_name,
                identity,
                desktop_entry,
                status,
            }
        })
        .collect()
}

fn bus_suffix(bus_name: &str) -> String {
    bus_name
        .strip_prefix("org.mpris.MediaPlayer2.")
        .unwrap_or(bus_name)
        .to_string()
}

fn mpris_match(requested: &str, player: &MediaPlayerInfo) -> bool {
    let wanted = requested.trim().to_ascii_lowercase();
    if wanted.is_empty()
        || matches!(
            wanted.as_str(),
            "best" | "default" | "wlm" | "winamp" | "wmp" | "itunes" | "mediamonkey"
        )
    {
        return false;
    }
    let bus = player.bus_name.to_ascii_lowercase();
    bus.ends_with(&format!(".{wanted}"))
        || player.identity.eq_ignore_ascii_case(requested)
        || player.desktop_entry.eq_ignore_ascii_case(requested)
        || bus_suffix(&player.bus_name).eq_ignore_ascii_case(requested)
}

fn mpris_select(players: &[MediaPlayerInfo], requested: &str) -> Option<MediaPlayerInfo> {
    if let Some(player) = players.iter().find(|player| mpris_match(requested, player)) {
        return Some(player.clone());
    }
    players
        .iter()
        .find(|player| player.status == "Playing")
        .or_else(|| players.iter().find(|player| player.status == "Paused"))
        .or_else(|| players.first())
        .cloned()
}

fn mpris_now_playing(requested: &str) -> Option<NowPlayingInfo> {
    let connection = zbus::blocking::Connection::session().ok()?;
    let players = mpris_media_players();
    let player = mpris_select(&players, requested)?;
    let bus_name = player.bus_name.clone();
    let metadata: std::collections::HashMap<String, zbus::zvariant::OwnedValue> = dbus_property(
        &connection,
        &bus_name,
        MPRIS_PATH,
        MPRIS_PLAYER_IFACE,
        "Metadata",
    )
    .unwrap_or_default();
    let status: String = dbus_property(
        &connection,
        &bus_name,
        MPRIS_PATH,
        MPRIS_PLAYER_IFACE,
        "PlaybackStatus",
    )
    .unwrap_or_else(|| "Stopped".to_string());
    let position: i64 = dbus_property(
        &connection,
        &bus_name,
        MPRIS_PATH,
        MPRIS_PLAYER_IFACE,
        "Position",
    )
    .unwrap_or(0);
    let volume: f64 = dbus_property(
        &connection,
        &bus_name,
        MPRIS_PATH,
        MPRIS_PLAYER_IFACE,
        "Volume",
    )
    .unwrap_or(0.0);
    let shuffle: bool = dbus_property(
        &connection,
        &bus_name,
        MPRIS_PATH,
        MPRIS_PLAYER_IFACE,
        "Shuffle",
    )
    .unwrap_or(false);
    let repeat: String = dbus_property(
        &connection,
        &bus_name,
        MPRIS_PATH,
        MPRIS_PLAYER_IFACE,
        "LoopStatus",
    )
    .unwrap_or_else(|| "None".to_string());

    Some(NowPlayingInfo {
        bus_name,
        identity: player.identity,
        status,
        title: metadata_string(&metadata, "xesam:title"),
        artist: metadata_list(&metadata, "xesam:artist").join(", "),
        album: metadata_string(&metadata, "xesam:album"),
        art_url: metadata_string(&metadata, "mpris:artUrl"),
        file: metadata_string(&metadata, "xesam:url"),
        lyrics: metadata_string(&metadata, "xesam:lyrics"),
        genre: metadata_list(&metadata, "xesam:genre").join(", "),
        track_id: metadata_string(&metadata, "mpris:trackid"),
        duration_secs: metadata_number(&metadata, "mpris:length") / 1_000_000.0,
        position_secs: position as f64 / 1_000_000.0,
        rating: metadata_number(&metadata, "xesam:rating").clamp(0.0, 5.0),
        track_number: metadata_number(&metadata, "xesam:trackNumber") as i64,
        year: metadata_year(&metadata),
        volume: (volume * 100.0).clamp(0.0, 100.0),
        shuffle,
        repeat: repeat.to_ascii_lowercase(),
    })
}

fn metadata_string(
    metadata: &std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
    key: &str,
) -> String {
    metadata
        .get(key)
        .and_then(|value| String::try_from(value.try_clone().ok()?).ok())
        .unwrap_or_default()
}

fn metadata_list(
    metadata: &std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
    key: &str,
) -> Vec<String> {
    metadata
        .get(key)
        .and_then(|value| Vec::<String>::try_from(value.try_clone().ok()?).ok())
        .unwrap_or_default()
}

fn metadata_number(
    metadata: &std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
    key: &str,
) -> f64 {
    let Some(value) = metadata.get(key).cloned() else {
        return 0.0;
    };
    if let Ok(number) = f64::try_from(value.clone()) {
        return number;
    }
    if let Ok(number) = i64::try_from(value.clone()) {
        return number as f64;
    }
    if let Ok(number) = u64::try_from(value) {
        return number as f64;
    }
    0.0
}

fn metadata_year(metadata: &std::collections::HashMap<String, zbus::zvariant::OwnedValue>) -> i64 {
    let created = metadata_string(metadata, "xesam:contentCreated");
    created
        .get(..4)
        .and_then(|year| year.parse().ok())
        .unwrap_or_else(|| {
            metadata_number(metadata, "xesam:year").clamp(0.0, i64::MAX as f64) as i64
        })
}

fn mpris_control(requested: &str, command: &str) -> bool {
    let Ok(connection) = zbus::blocking::Connection::session() else {
        return false;
    };
    let players = mpris_media_players();
    let player = mpris_select(&players, requested);
    let (action, args) = command
        .trim()
        .split_once(' ')
        .map(|(action, args)| (action, args.trim()))
        .unwrap_or((command.trim(), ""));
    let action = action.to_ascii_lowercase();

    if action == "openplayer" {
        return open_media_player(requested, player.as_ref());
    }
    if action == "toggleplayer" {
        if let Some(player) = &player {
            return mpris_quit(&connection, &player.bus_name);
        }
        return open_media_player(requested, None);
    }
    let Some(player) = player else {
        return false;
    };
    if action == "closeplayer" {
        return mpris_quit(&connection, &player.bus_name);
    }

    let method = match action.as_str() {
        "play" => "Play",
        "pause" => "Pause",
        "playpause" => "PlayPause",
        "next" => "Next",
        "previous" => "Previous",
        "stop" => "Stop",
        _ => "",
    };
    if !method.is_empty() {
        return connection
            .call_method(
                Some(player.bus_name.as_str()),
                MPRIS_PATH,
                Some(MPRIS_PLAYER_IFACE),
                method,
                &(),
            )
            .is_ok();
    }

    let info = mpris_now_playing(requested);
    match action.as_str() {
        "setposition" => {
            let Some(info) = info else {
                return false;
            };
            let value = parse_relative(args, info.progress());
            let position = (value.clamp(0.0, 100.0) / 100.0) * info.duration_secs;
            let track = zbus::zvariant::ObjectPath::try_from(info.track_id.as_str())
                .ok()
                .map(|path| path.to_owned())
                .unwrap_or_else(|| {
                    zbus::zvariant::ObjectPath::try_from(
                        "/org/mpris/MediaPlayer2/TrackList/NoTrack",
                    )
                    .expect("chemin de piste MPRIS valide")
                    .to_owned()
                });
            connection
                .call_method(
                    Some(player.bus_name.as_str()),
                    MPRIS_PATH,
                    Some(MPRIS_PLAYER_IFACE),
                    "SetPosition",
                    &(track, (position * 1_000_000.0) as i64),
                )
                .is_ok()
        }
        "setvolume" => {
            let Some(info) = info else {
                return false;
            };
            let volume = parse_relative(args, info.volume).clamp(0.0, 100.0);
            connection
                .call_method(
                    Some(player.bus_name.as_str()),
                    MPRIS_PATH,
                    Some(MPRIS_PLAYER_IFACE),
                    "SetVolume",
                    &(volume / 100.0),
                )
                .is_ok()
        }
        "setshuffle" => {
            let Some(info) = info else {
                return false;
            };
            let state = match args.trim() {
                "-1" => !info.shuffle,
                value => value != "0",
            };
            connection
                .call_method(
                    Some(player.bus_name.as_str()),
                    MPRIS_PATH,
                    Some(MPRIS_PLAYER_IFACE),
                    "SetShuffle",
                    &(state,),
                )
                .is_ok()
        }
        "setrepeat" => {
            let Some(info) = info else {
                return false;
            };
            let state = match args.trim() {
                "-1" => {
                    if info.repeat == "none" {
                        "track"
                    } else {
                        "none"
                    }
                }
                "0" => "none",
                "1" => "track",
                "2" => "playlist",
                _ => return false,
            };
            connection
                .call_method(
                    Some(player.bus_name.as_str()),
                    MPRIS_PATH,
                    Some(MPRIS_PLAYER_IFACE),
                    "SetLoopStatus",
                    &(state,),
                )
                .is_ok()
        }
        "setrating" => {
            let map = std::collections::HashMap::from([(
                "xesam:rating",
                zbus::zvariant::Value::from(args.trim().parse::<f64>().unwrap_or(0.0)),
            )]);
            connection
                .call_method(
                    Some(player.bus_name.as_str()),
                    MPRIS_PATH,
                    Some("org.freedesktop.DBus.Properties"),
                    "Set",
                    &(MPRIS_PLAYER_IFACE, "Metadata", map),
                )
                .is_ok()
        }
        _ => false,
    }
}

fn parse_relative(args: &str, current: f64) -> f64 {
    let args = args.trim();
    if let Some(relative) = args.strip_prefix(['+', '-']) {
        let sign = if args.starts_with('-') { -1.0 } else { 1.0 };
        return current + sign * relative.trim().parse::<f64>().unwrap_or(0.0);
    }
    args.parse().unwrap_or(current)
}

fn mpris_quit(connection: &zbus::blocking::Connection, bus_name: &str) -> bool {
    connection
        .call_method(
            Some(bus_name),
            MPRIS_PATH,
            Some(MPRIS_ROOT_IFACE),
            "Quit",
            &(),
        )
        .is_ok()
}

fn open_media_player(requested: &str, player: Option<&MediaPlayerInfo>) -> bool {
    if player.is_some() {
        return true;
    }
    let desktop = if requested.trim().is_empty() || requested.eq_ignore_ascii_case("best") {
        String::new()
    } else {
        requested.trim().to_ascii_lowercase()
    };
    if !desktop.is_empty() {
        if std::process::Command::new("gtk-launch")
            .arg(&desktop)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
        {
            return true;
        }
        if std::process::Command::new("gio")
            .args(["launch", &desktop])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
        {
            return true;
        }
    }
    false
}

fn media_cover(file: &str, art_url: &str) -> Option<String> {
    let art_url = art_url.trim();
    if !art_url.is_empty() {
        if let Some(path) = art_url.strip_prefix("file://") {
            let path = percent_decode(path);
            if std::path::Path::new(&path).is_file() {
                return Some(path);
            }
        } else if art_url.starts_with('/') {
            if std::path::Path::new(art_url).is_file() {
                return Some(art_url.to_string());
            }
        } else {
            return Some(art_url.to_string());
        }
    }

    let file = file.trim().strip_prefix("file://").unwrap_or(file.trim());
    if file.is_empty() {
        return None;
    }
    embedded_cover(file)
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&value[index + 1..index + 3], 16) {
                output.push(byte);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).to_string()
}

fn embedded_cover(file: &str) -> Option<String> {
    use lofty::file::TaggedFileExt;
    use lofty::picture::PictureType;
    use lofty::probe::Probe;

    let tagged = Probe::open(file).ok()?.read().ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
    let picture = tag
        .pictures()
        .iter()
        .find(|picture| picture.pic_type() == PictureType::CoverFront)
        .or_else(|| tag.pictures().first())?;
    let extension = picture
        .mime_type()
        .and_then(|mime| mime.as_str().split('/').nth(1))
        .unwrap_or("jpg")
        .replace("jpeg", "jpg");
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .map(|path| path.join("petrichor").join("covers"))?;
    std::fs::create_dir_all(&cache).ok()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    file.hash(&mut hasher);
    let path = cache.join(format!("{:016x}.{extension}", hasher.finish()));
    std::fs::write(&path, picture.data()).ok()?;
    Some(path.to_string_lossy().to_string())
}

fn mpris_action(method: &str) -> bool {
    let Ok(connection) = zbus::blocking::Connection::session() else {
        return false;
    };
    let players = mpris_players(&connection);
    if players.is_empty() {
        return false;
    }
    players.into_iter().any(|player| {
        connection
            .call_method(
                Some(player.as_str()),
                "/org/mpris/MediaPlayer2",
                Some("org.mpris.MediaPlayer2.Player"),
                method,
                &(),
            )
            .is_ok()
    })
}

fn mpris_players(connection: &zbus::blocking::Connection) -> Vec<String> {
    let names: Vec<String> = connection
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "ListNames",
            &(),
        )
        .ok()
        .and_then(|reply| reply.body().deserialize().ok())
        .unwrap_or_default();
    let mut players: Vec<(u8, String)> = names
        .into_iter()
        .filter(|name| name.starts_with("org.mpris.MediaPlayer2."))
        .map(|name| {
            let status: String = dbus_property(
                connection,
                &name,
                "/org/mpris/MediaPlayer2",
                "org.mpris.MediaPlayer2.Player",
                "PlaybackStatus",
            )
            .unwrap_or_default();
            let priority = if status == "Playing" {
                0
            } else if status == "Paused" {
                1
            } else {
                2
            };
            (priority, name)
        })
        .collect();
    players.sort();
    players.into_iter().map(|(_, name)| name).collect()
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
    fn parses_swap_memory() {
        let text = "MemTotal: 1000 kB\nSwapTotal: 500 kB\nSwapFree: 200 kB\n";
        let info = parse_memory_info(text);
        assert_eq!(info.swap_total_kb, 500);
        assert_eq!(info.swap_used_kb(), 300);
    }

    #[test]
    fn parses_net_dev_counters() {
        let text = "Inter-|   Receive                                                |  Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n    lo: 100 1 0 0 0 0 0 0 200 2 0 0 0 0 0 0\n  eth0: 1000 10 0 0 0 0 0 0 2000 20 0 0 0 0 0 0\n";
        let samples = parse_net_dev(text);
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[1].interface, "eth0");
        assert_eq!(samples[1].rx_bytes, 1000);
        assert_eq!(samples[1].tx_bytes, 2000);
    }

    #[test]
    fn selects_best_and_named_network_interface() {
        let samples = vec![
            NetSample {
                interface: "lo".to_string(),
                rx_bytes: 9_000,
                tx_bytes: 9_000,
            },
            NetSample {
                interface: "eth0".to_string(),
                rx_bytes: 100,
                tx_bytes: 200,
            },
            NetSample {
                interface: "wlan0".to_string(),
                rx_bytes: 1_000,
                tx_bytes: 2_000,
            },
        ];
        assert_eq!(
            select_net_sample(&samples, "BEST").unwrap().interface,
            "wlan0"
        );
        assert_eq!(
            select_net_sample(&samples, "eth0").unwrap().interface,
            "eth0"
        );
        assert_eq!(select_net_sample(&samples, "2").unwrap().interface, "wlan0");
    }

    #[test]
    fn parses_nmcli_wifi_interfaces() {
        let text = "wlan0:wifi\neth0:ethernet\nwlan1:wifi\n";
        assert_eq!(parse_wifi_interfaces(text), vec!["wlan0", "wlan1"]);
    }

    #[test]
    fn parses_nmcli_wifi_output() {
        let text = "*:Home\\:5G:80:WPA2\n:Neighbor:42:WPA1 WPA2\n";
        let info = parse_nmcli(text);
        assert!(info.connected);
        assert_eq!(info.ssid, "Home:5G");
        assert_eq!(info.quality, 80.0);
        assert_eq!(info.networks.len(), 2);
        assert_eq!(info.networks[1].ssid, "Neighbor");
    }

    #[test]
    fn normalizes_and_reads_virtual_registry() {
        let mut values = BTreeMap::new();
        values.insert(
            "HKCU\\Software\\Petrichor\\Theme".to_string(),
            "dark".to_string(),
        );
        assert_eq!(
            registry_lookup(&values, "HKEY_CURRENT_USER\\Software\\Petrichor\\Theme"),
            Some("dark".to_string())
        );
        assert_eq!(
            normalize_registry_path("HKCU/Software/Foo"),
            "HKEY_CURRENT_USER\\Software\\Foo"
        );
        assert_eq!(registry_lookup(&values, "HKLM\\Missing"), None);
    }

    #[test]
    fn parses_xrandr_monitors() {
        let text = "Monitors: 3\n 0: +*eDP-1 1920/344x1080/193+0+0  eDP-1\n 1: +HDMI-1 1280/300x720/200+1920+0  HDMI-1\n 2: +DP-1 1280/300x720/200-1280+0  DP-1\n";
        let monitors = parse_xrandr_monitors(text);
        assert_eq!(monitors.len(), 3);
        assert_eq!(monitors[0], (0.0, 0.0, 1920.0, 1080.0));
        assert_eq!(monitors[1], (1920.0, 0.0, 1280.0, 720.0));
        assert_eq!(monitors[2], (-1280.0, 0.0, 1280.0, 720.0));
    }

    #[test]
    fn parses_iw_link_rates_and_phy() {
        let text = "Connected to aa:bb (on wlan0)\n\tSSID: Home\n\tfreq: 5180.0\n\tsignal: -53 dBm\n\trx bitrate: 60.0 MBit/s MCS 3\n\ttx bitrate: 866.7 MBit/s MCS 9\n";
        let link = parse_iw_link(text);
        assert_eq!(link.rx_kbps, 60_000.0);
        assert_eq!(link.tx_kbps, 866_700.0);
        assert_eq!(link.signal_dbm, Some(-53));
        assert_eq!(link.phy, "802.11ac");
    }

    #[test]
    fn derives_phy_from_frequency_and_bitrate() {
        assert_eq!(phy_label(2412, 120_000.0), "802.11n");
        assert_eq!(phy_label(5180, 400_000.0), "802.11ac");
        assert_eq!(phy_label(5180, 1_200_000.0), "802.11ax");
        assert_eq!(phy_label(6115, 0.0), "802.11ax");
        assert_eq!(phy_label(0, 0.0), "");
    }

    #[test]
    fn parses_suspend_journal() {
        let text = "1792570000.5 fedora systemd[1]: Suspending system...\n1792570100.5 fedora kernel: PM: suspend exit\n1792570200.5 fedora systemd[1]: Suspending system...\n";
        let (sleep, wake) = parse_sleep_journal(text);
        assert_eq!(sleep, 1_792_570_200.5);
        assert_eq!(wake, 1_792_570_100.5);
        assert_eq!(unix_to_filetime(0.0), 0.0);
        assert_eq!(unix_to_filetime(1.0), 11_644_473_601.0);
    }

    #[test]
    fn selects_mpris_player_and_formats_state() {
        let players = vec![
            MediaPlayerInfo {
                bus_name: "org.mpris.MediaPlayer2.vlc".to_string(),
                identity: "VLC".to_string(),
                desktop_entry: "vlc".to_string(),
                status: "Paused".to_string(),
            },
            MediaPlayerInfo {
                bus_name: "org.mpris.MediaPlayer2.spotify".to_string(),
                identity: "Spotify".to_string(),
                desktop_entry: "spotify".to_string(),
                status: "Playing".to_string(),
            },
        ];
        assert_eq!(
            mpris_select(&players, "Spotify")
                .expect("lecteur Spotify")
                .bus_name,
            "org.mpris.MediaPlayer2.spotify"
        );
        assert_eq!(
            mpris_select(&players, "WLM")
                .expect("lecteur BEST")
                .bus_name,
            "org.mpris.MediaPlayer2.spotify"
        );
        let info = NowPlayingInfo {
            status: "Paused".to_string(),
            duration_secs: 200.0,
            position_secs: 50.0,
            ..NowPlayingInfo::default()
        };
        assert_eq!(info.state(), 2);
        assert_eq!(info.progress(), 25.0);
    }

    #[test]
    fn decodes_cover_uri() {
        assert_eq!(percent_decode("A%20B%2Fc"), "A B/c");
    }

    #[test]
    fn mpris_probe_when_requested() {
        if std::env::var("PETRICHOR_MPRIS_PROBE").is_err() {
            return;
        }
        assert!(!mpris_players(&zbus::blocking::Connection::session().unwrap()).is_empty());
        assert!(media_key("PlayPause"));
    }

    #[test]
    fn computes_system_times() {
        let logon = user_logon_time();
        let idle = idle_time_seconds();
        let (sleep, wake) = sleep_wake_times();
        eprintln!("system times: logon={logon} idle={idle} sleep={sleep} wake={wake}");
        assert!(idle >= 0.0);
        if logon > 0.0 {
            assert!(logon > WINDOWS_EPOCH_OFFSET);
        }
    }

    #[test]
    fn wifi_dbus_probe() {
        if let Some(info) = wifi_info_dbus(None) {
            eprintln!(
                "wifi dbus: connected={} ssid={} phy={} tx={} rx={} networks={}",
                info.connected,
                info.ssid,
                info.phy,
                info.tx_rate_kbps,
                info.rx_rate_kbps,
                info.networks.len()
            );
        }
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
