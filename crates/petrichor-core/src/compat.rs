use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Mappings {
    #[serde(default)]
    pub paths: BTreeMap<String, String>,
    #[serde(default)]
    pub commands: BTreeMap<String, String>,
    #[serde(default)]
    pub fonts: BTreeMap<String, String>,
    #[serde(default)]
    pub players: BTreeMap<String, String>,
    #[serde(default)]
    pub registry: BTreeMap<String, String>,
}

impl Mappings {
    pub fn builtin() -> Mappings {
        let mut mappings = Mappings::default();
        for (name, value) in [
            ("winamp", "audacious"),
            ("wmp", "vlc"),
            ("windows media player", "vlc"),
            ("itunes", "rhythmbox"),
            ("mediamonkey", "deadbeef"),
            ("musicbee", "best"),
            ("aimp", "best"),
            ("foobar2000", "best"),
            ("spotify", "spotify"),
            ("vlc", "vlc"),
            ("mpd", "mpd"),
            ("deezer", "deezer"),
            ("tidal", "tidal"),
            ("youtube music", "youtube-music"),
        ] {
            mappings.players.insert(name.to_string(), value.to_string());
        }
        for (name, value) in [
            ("segoe ui", "Cantarell"),
            ("arial", "Liberation Sans"),
            ("tahoma", "DejaVu Sans"),
            ("verdana", "DejaVu Sans"),
            ("trebuchet ms", "DejaVu Sans"),
            ("calibri", "Carlito"),
            ("consolas", "DejaVu Sans Mono"),
            ("courier new", "Liberation Mono"),
            ("times new roman", "Liberation Serif"),
            ("microsoft sans serif", "Noto Sans"),
            ("segoe ui semibold", "Cantarell"),
            ("segoe ui light", "Cantarell"),
        ] {
            mappings.fonts.insert(name.to_string(), value.to_string());
        }
        for (name, value) in [
            ("tasklist", "ps"),
            ("get-process", "ps"),
            ("dir", "ls"),
            ("type", "cat"),
            ("del", "rm"),
            ("erase", "rm"),
            ("copy", "cp"),
            ("move", "mv"),
            ("cls", "clear"),
            ("where", "command -v"),
            ("taskmgr", "gnome-system-monitor"),
            ("notepad", "gedit"),
            ("explorer", "xdg-open"),
            ("control", "gnome-control-center"),
            ("calc", "gnome-calculator"),
            ("mspaint", "xdg-open"),
        ] {
            mappings
                .commands
                .insert(name.to_string(), value.to_string());
        }
        mappings
    }

    pub fn from_json(text: &str) -> Result<Mappings, String> {
        let mut mappings: Mappings =
            serde_json::from_str(text).map_err(|error| error.to_string())?;
        mappings.normalize();
        Ok(mappings)
    }

    pub fn from_path(path: &Path) -> Result<Mappings, String> {
        let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        Mappings::from_json(&text)
    }

    pub fn merged_with(&self, other: &Mappings) -> Mappings {
        let mut merged = self.clone();
        merged.paths.extend(other.paths.clone());
        merged.commands.extend(other.commands.clone());
        merged.fonts.extend(other.fonts.clone());
        merged.players.extend(other.players.clone());
        merged.registry.extend(other.registry.clone());
        merged.normalize();
        merged
    }

    pub fn normalize(&mut self) {
        for map in [
            &mut self.paths,
            &mut self.commands,
            &mut self.fonts,
            &mut self.players,
            &mut self.registry,
        ] {
            let normalized = std::mem::take(map)
                .into_iter()
                .map(|(key, value)| (key.trim().to_ascii_lowercase(), value))
                .filter(|(key, _)| !key.is_empty())
                .collect();
            *map = normalized;
        }
    }

    fn path_value(&self, key: &str) -> Option<&str> {
        self.paths
            .get(&key.to_ascii_lowercase())
            .map(String::as_str)
    }

    fn command_value(&self, key: &str) -> Option<&str> {
        self.commands
            .get(&key.to_ascii_lowercase())
            .map(String::as_str)
    }

    fn font_value(&self, key: &str) -> Option<&str> {
        self.fonts
            .get(&key.to_ascii_lowercase())
            .map(String::as_str)
    }

    fn player_value(&self, key: &str) -> Option<&str> {
        self.players
            .get(&key.to_ascii_lowercase())
            .map(String::as_str)
    }

    fn registry_value(&self, key: &str) -> Option<&str> {
        self.registry
            .get(&key.to_ascii_lowercase())
            .map(String::as_str)
    }
}

pub fn compat_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("PETRICHOR_COMPAT") {
        return Some(PathBuf::from(path));
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    let upper = base.join("Petrichor").join("compat.json");
    let lower = base.join("petrichor").join("compat.json");
    Some(if !upper.exists() && lower.exists() {
        lower
    } else {
        upper
    })
}

pub fn mappings() -> &'static Mappings {
    static MAPPINGS: OnceLock<Mappings> = OnceLock::new();
    MAPPINGS.get_or_init(|| {
        let builtin = Mappings::builtin();
        let Some(path) = compat_path() else {
            return builtin;
        };
        match Mappings::from_path(&path) {
            Ok(overrides) => builtin.merged_with(&overrides),
            Err(error) => {
                crate::logging::warning(format!(
                    "compat.json ignoré ({}): {error}",
                    path.display()
                ));
                builtin
            }
        }
    })
}

pub fn windows_env(name: &str) -> Option<String> {
    let key = name.trim().to_ascii_uppercase();
    if let Some(value) = mappings().path_value(&key) {
        return Some(value.to_string());
    }
    let home = || std::env::var("HOME").ok();
    let xdg = |variable: &str, fallback: &str| {
        std::env::var(variable)
            .ok()
            .or_else(|| home().map(|home| format!("{home}/{fallback}")))
    };
    Some(match key.as_str() {
        "USERPROFILE" | "HOMEPATH" => home()?,
        "HOMEDRIVE" | "SYSTEMDRIVE" | "SYSTEMROOT" | "WINDIR" => "/".to_string(),
        "APPDATA" => xdg("XDG_CONFIG_HOME", ".config")?,
        "LOCALAPPDATA" => xdg("XDG_DATA_HOME", ".local/share")?,
        "TEMP" | "TMP" => std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string()),
        "PROGRAMFILES" | "PROGRAMFILES(X86)" | "PROGRAMW6432" => "/usr".to_string(),
        "PROGRAMDATA" | "ALLUSERSPROFILE" | "PUBLIC" => "/usr/share".to_string(),
        "USERNAME" => std::env::var("USER")
            .or_else(|_| std::env::var("LOGNAME"))
            .ok()?,
        "COMPUTERNAME" => std::fs::read_to_string("/proc/sys/kernel/hostname")
            .ok()?
            .trim()
            .to_string(),
        _ => return None,
    })
}

pub fn expand_windows_env(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find('%') {
        output.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else {
            output.push('%');
            output.push_str(after);
            return output;
        };
        let name = &after[..end];
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            output.push('%');
            output.push_str(after);
            return output;
        }
        if let Ok(value) = std::env::var(name) {
            output.push_str(&value);
        } else if let Some(value) = windows_env(name) {
            output.push_str(&value);
        } else {
            output.push('%');
            output.push_str(name);
            output.push('%');
        }
        rest = &after[end + 1..];
    }
    output.push_str(rest);
    output
}

pub fn translate_path(input: &str) -> String {
    translate_path_checked(input)
        .unwrap_or_else(|| expand_windows_env(input.trim()).replace('\\', "/"))
}

pub fn translate_path_checked(input: &str) -> Option<String> {
    translate_path_checked_with(input, mappings())
}

fn translate_path_checked_with(input: &str, mappings: &Mappings) -> Option<String> {
    let expanded = expand_windows_env(input.trim());
    if let Some(relative) = expanded.strip_prefix("@Resources/") {
        return Some(format!("@Resources/{}", relative.replace('\\', "/")));
    }
    let path = expanded.replace('\\', "/");
    if let Some(mapped) = mapped_path(&path, mappings) {
        return Some(mapped);
    }
    let lower = path.to_ascii_lowercase();
    if let Some(rest) = strip_windows_prefix(&path, &lower, "c:/users/") {
        if let Ok(home) = std::env::var("HOME") {
            let mut parts = rest.splitn(2, '/');
            let _user = parts.next().unwrap_or_default();
            let tail = parts.next().unwrap_or_default();
            return Some(if tail.is_empty() {
                home
            } else {
                format!("{home}/{tail}")
            });
        }
    }
    for (prefix, target) in [
        ("c:/program files (x86)", "/usr"),
        ("c:/program files", "/usr"),
        ("c:/programdata", "/usr/share"),
        ("c:/windows", "/"),
        ("c:/", "/"),
    ] {
        if lower == prefix {
            return Some(target.to_string());
        }
        if let Some(rest) = strip_windows_prefix(&path, &lower, &format!("{prefix}/")) {
            return Some(format!("{target}/{rest}"));
        }
    }
    if path.len() > 2 && path.as_bytes()[1] == b':' {
        if let Ok(prefix) = std::env::var("WINEPREFIX") {
            return Some(format!("{prefix}/drive_c/{}", &path[3..]));
        }
        if let Ok(home) = std::env::var("HOME") {
            let drive = format!("{home}/.wine/drive_c/{}", &path[3..]);
            if Path::new(&drive).exists() {
                return Some(drive);
            }
        }
        let mount = format!("/mnt/{}/{}", path[..1].to_ascii_lowercase(), &path[3..]);
        if Path::new(&mount).exists() {
            return Some(mount);
        }
        return None;
    }
    Some(path)
}

fn mapped_path(path: &str, mappings: &Mappings) -> Option<String> {
    let lower = path.to_ascii_lowercase();
    mappings.paths.iter().find_map(|(prefix, target)| {
        if lower == *prefix {
            return Some(target.clone());
        }
        lower
            .strip_prefix(prefix)
            .filter(|rest| rest.starts_with('/'))?;
        let rest = path.get(prefix.len()..)?;
        Some(format!("{}{}", target.trim_end_matches('/'), rest))
    })
}

fn strip_windows_prefix<'a>(path: &'a str, lower: &str, prefix: &str) -> Option<&'a str> {
    lower.strip_prefix(prefix).map(|_| &path[prefix.len()..])
}

pub fn substitute_font(face: &str) -> Option<String> {
    let requested = face.trim().trim_matches(['"', '\'']);
    if requested.is_empty() {
        return None;
    }
    mappings()
        .font_value(requested)
        .map(str::to_string)
        .filter(|resolved| !resolved.eq_ignore_ascii_case(requested))
}

pub fn player_alias(name: &str) -> Option<String> {
    let requested = name.trim();
    if requested.is_empty() {
        return None;
    }
    mappings()
        .player_value(requested)
        .map(str::to_string)
        .filter(|resolved| !resolved.eq_ignore_ascii_case(requested))
}

pub fn registry_override(path: &str) -> Option<String> {
    mappings().registry_value(path).map(str::to_string)
}

pub fn wine_available() -> bool {
    command_in_path("wine")
}

pub fn command_in_path(program: &str) -> bool {
    let path = Path::new(program);
    if path.components().count() > 1 {
        return path.is_file();
    }
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
        .unwrap_or(false)
}

fn native_command(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    let stem = lower.strip_suffix(".exe").unwrap_or(&lower);
    mappings()
        .command_value(&lower)
        .or_else(|| mappings().command_value(stem))
}

pub fn translate_exec(program: &str, args: &[String]) -> Option<(String, Vec<String>)> {
    let program = expand_windows_env(program);
    let lower = program.to_ascii_lowercase();
    let args = args
        .iter()
        .map(|arg| expand_windows_env(arg))
        .collect::<Vec<_>>();
    if let Some(mapped) = native_command(&lower) {
        return Some((mapped.to_string(), args));
    }
    if lower == "cmd" || lower == "cmd.exe" {
        if let Some(index) = args.iter().position(|arg| arg.eq_ignore_ascii_case("/c")) {
            let command = args[index + 1..].join(" ");
            return Some(("sh".to_string(), vec!["-c".to_string(), command]));
        }
        return None;
    }
    if lower.ends_with(".bat") || lower.ends_with(".cmd") {
        if !wine_available() {
            return None;
        }
        let mut output = vec![
            "cmd".to_string(),
            "/c".to_string(),
            translate_path(&program),
        ];
        output.extend(args);
        return Some(("wine".to_string(), output));
    }
    if lower.ends_with(".ps1") {
        if !wine_available() {
            return None;
        }
        let mut output = vec![
            "powershell".to_string(),
            "-File".to_string(),
            translate_path(&program),
        ];
        output.extend(args);
        return Some(("wine".to_string(), output));
    }
    if lower.ends_with(".exe") {
        if !wine_available() {
            return None;
        }
        let mut output = vec![translate_path(&program)];
        output.extend(args);
        return Some(("wine".to_string(), output));
    }
    if lower == "start" {
        return Some(("xdg-open".to_string(), args));
    }
    let translated = mappings()
        .command_value(&lower)
        .map(str::to_string)
        .unwrap_or_else(|| translate_path(&program));
    Some((translated, args))
}

pub fn translate_shell_command(command: &str) -> Option<String> {
    let expanded = expand_windows_env(command);
    let trimmed = expanded.trim();
    if trimmed.is_empty() {
        return Some(String::new());
    }
    let (first, rest) = split_first_token(trimmed);
    let lower = first.to_ascii_lowercase();
    if lower == "cmd" || lower == "cmd.exe" {
        if let Some(rest) = rest
            .strip_prefix("/c ")
            .or_else(|| rest.strip_prefix("/C "))
        {
            return Some(translate_shell_command(rest).unwrap_or_else(|| rest.to_string()));
        }
        if rest.eq_ignore_ascii_case("/c") {
            return Some(String::new());
        }
        return None;
    }
    if lower == "start" {
        return Some(format!("xdg-open {rest}"));
    }
    if let Some(mapped) = native_command(&lower) {
        let rest = translate_shell_arguments(&rest)?;
        return Some(if rest.is_empty() {
            mapped.to_string()
        } else {
            format!("{mapped} {rest}")
        });
    }
    if lower.ends_with(".exe") || lower.ends_with(".bat") || lower.ends_with(".cmd") {
        if !wine_available() {
            return None;
        }
        let program = translate_path_checked(&first).unwrap_or_else(|| first.clone());
        let rest = translate_shell_arguments(&rest)?;
        return Some(if rest.is_empty() {
            format!("wine {program}")
        } else {
            format!("wine {} {rest}", shell_quote(&program))
        });
    }
    let program = if let Some(mapped) = mappings().command_value(&lower) {
        mapped.to_string()
    } else if is_windows_path(&first) {
        translate_path_checked(&first)?
    } else {
        translate_path(&first)
    };
    let rest = translate_shell_arguments(&rest)?;
    Some(if rest.is_empty() {
        program
    } else {
        format!("{program} {rest}")
    })
}

fn split_first_token(value: &str) -> (String, String) {
    let trimmed = value.trim();
    if let Some(quote) = trimmed
        .chars()
        .next()
        .filter(|ch| *ch == '"' || *ch == '\'')
    {
        if let Some(end) = trimmed[1..].find(quote) {
            return (
                trimmed[1..1 + end].to_string(),
                trimmed[2 + end..].trim_start().to_string(),
            );
        }
    }
    trimmed
        .split_once(char::is_whitespace)
        .map(|(first, rest)| (first.to_string(), rest.trim_start().to_string()))
        .unwrap_or_else(|| (trimmed.to_string(), String::new()))
}

fn shell_quote(value: &str) -> String {
    if value.contains(char::is_whitespace) && !value.starts_with('"') {
        format!("\"{value}\"")
    } else {
        value.to_string()
    }
}

fn is_windows_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

fn translate_shell_arguments(value: &str) -> Option<String> {
    let expanded = value.replace("%TEMP%", "/tmp").replace(
        "%APPDATA%",
        &windows_env("APPDATA").unwrap_or_else(|| "~/.config".to_string()),
    );
    let chars: Vec<char> = expanded.chars().collect();
    let mut output = String::with_capacity(expanded.len());
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if ch == '"' || ch == '\'' {
            let start = index + 1;
            let mut end = start;
            while end < chars.len() && chars[end] != ch {
                end += 1;
            }
            let inner: String = chars[start..end].iter().collect();
            output.push(ch);
            if is_windows_path(&inner) {
                output.push_str(&translate_path_checked(&inner)?);
            } else {
                output.push_str(&inner);
            }
            if end < chars.len() {
                output.push(ch);
                index = end + 1;
            } else {
                index = end;
            }
            continue;
        }
        let at_boundary = index == 0 || chars[index - 1].is_whitespace();
        if at_boundary && is_windows_path(&chars[index..].iter().collect::<String>()) {
            let start = index;
            let mut end = index;
            while end < chars.len() && !chars[end].is_whitespace() {
                end += 1;
            }
            let token: String = chars[start..end].iter().collect();
            output.push_str(&translate_path_checked(&token)?);
            index = end;
            continue;
        }
        output.push(ch);
        index += 1;
    }
    if output.contains("-n ") {
        output = output.replace("-n ", "-c ");
    }
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_windows_environment_and_paths() {
        let expanded = expand_windows_env("%APPDATA%/Petrichor");
        assert!(!expanded.contains("%APPDATA%"));
        assert!(expanded.ends_with("/Petrichor"));
        assert_eq!(
            translate_path(r"C:\Program Files\Rainmeter"),
            "/usr/Rainmeter"
        );
        assert_eq!(translate_path("Images\\bg.png"), "Images/bg.png");
    }

    #[test]
    fn applies_custom_path_mappings() {
        let mut mappings = Mappings::default();
        mappings
            .paths
            .insert("c:/games".to_string(), "/home/alice/Games".to_string());
        assert_eq!(
            translate_path_checked_with("C:\\Games\\Skyrim", &mappings),
            Some("/home/alice/Games/Skyrim".to_string())
        );
        assert_eq!(
            translate_path_checked_with("C:\\Games", &mappings),
            Some("/home/alice/Games".to_string())
        );
    }

    #[test]
    fn translates_known_commands_and_flags() {
        assert_eq!(translate_shell_command("dir C:\\Skins"), None);
        assert_eq!(
            translate_path_checked("C:\\PetrichorMissing\\tool.exe"),
            None
        );
        assert_eq!(
            translate_shell_command("ping -n 2 example.org").as_deref(),
            Some("ping -c 2 example.org")
        );
        assert_eq!(
            translate_shell_command("cmd /c tasklist").as_deref(),
            Some("ps")
        );
        assert_eq!(
            translate_shell_command("start https://example.org").as_deref(),
            Some("xdg-open https://example.org")
        );
        assert_eq!(translate_shell_command("script-inconnu.exe"), None);
        assert_eq!(
            translate_shell_command("taskmgr.exe").as_deref(),
            Some("gnome-system-monitor")
        );
        let copied = translate_shell_command(
            "copy \"C:\\Users\\bob\\file.txt\" \"C:\\Users\\bob\\backup.txt\"",
        )
        .unwrap();
        assert!(copied.starts_with("cp "));
        assert!(!copied.contains("C:\\Users"));
        assert!(copied.contains("backup.txt"));
    }

    #[test]
    fn substitutes_fonts_and_players() {
        assert_eq!(substitute_font("Segoe UI").as_deref(), Some("Cantarell"));
        assert_eq!(substitute_font("Cantarell"), None);
        assert_eq!(player_alias("Winamp").as_deref(), Some("audacious"));
        assert_eq!(player_alias("Spotify"), None);
    }

    #[test]
    fn merges_user_mappings() {
        let builtin = Mappings::builtin();
        let overrides = Mappings::from_json(
            r#"{"fonts":{"Segoe UI":"Inter"},"commands":{"mytool":"/usr/bin/mytool"}}"#,
        )
        .unwrap();
        let merged = builtin.merged_with(&overrides);
        assert_eq!(merged.font_value("segoe ui"), Some("Inter"));
        assert_eq!(merged.command_value("MYTOOL"), Some("/usr/bin/mytool"));
    }
}
