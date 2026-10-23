use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use petrichor_core::compat::{self, command_in_path, translate_shell_command, Mappings};
use petrichor_core::{Encoding, Skin, Variables};
use petrichor_sys::{backend, SystemBackend};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Plan {
    A,
    B,
    C,
}

impl Plan {
    pub fn code(self) -> &'static str {
        match self {
            Plan::A => "A",
            Plan::B => "B",
            Plan::C => "C",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CompatLevel {
    Native,
    Automatic,
    Manual,
    Unsupported,
}

impl CompatLevel {
    pub fn code(self) -> &'static str {
        match self {
            CompatLevel::Native => "N0",
            CompatLevel::Automatic => "N1",
            CompatLevel::Manual => "N2",
            CompatLevel::Unsupported => "N3",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            CompatLevel::Native => "Natif",
            CompatLevel::Automatic => "Adapté automatiquement",
            CompatLevel::Manual => "Adaptation manuelle",
            CompatLevel::Unsupported => "Non supporté",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CompatEntry {
    pub feature: String,
    pub requested: String,
    pub resolved: Option<String>,
    pub level: CompatLevel,
    pub plan: Option<Plan>,
    pub message: Option<String>,
    pub action: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct CompatReport {
    pub skin: String,
    pub entries: Vec<CompatEntry>,
}

impl CompatReport {
    pub fn new(skin: impl Into<String>) -> Self {
        CompatReport {
            skin: skin.into(),
            entries: Vec::new(),
        }
    }

    pub fn record(
        &mut self,
        feature: impl Into<String>,
        requested: impl Into<String>,
        resolved: Option<String>,
        level: CompatLevel,
        plan: Option<Plan>,
        message: Option<String>,
    ) {
        self.record_action(feature, requested, resolved, level, plan, message, None);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_action(
        &mut self,
        feature: impl Into<String>,
        requested: impl Into<String>,
        resolved: Option<String>,
        level: CompatLevel,
        plan: Option<Plan>,
        message: Option<String>,
        action: Option<String>,
    ) {
        let entry = CompatEntry {
            feature: feature.into(),
            requested: requested.into(),
            resolved,
            level,
            plan,
            message,
            action,
        };
        if self.entries.iter().any(|existing| {
            existing.feature == entry.feature
                && existing.requested == entry.requested
                && existing.resolved == entry.resolved
        }) {
            return;
        }
        self.entries.push(entry);
    }

    pub fn overall(&self) -> CompatLevel {
        self.entries
            .iter()
            .map(|entry| entry.level)
            .max()
            .unwrap_or(CompatLevel::Native)
    }

    pub fn has_unsupported(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.level == CompatLevel::Unsupported)
    }

    pub fn count_by_level(&self, level: CompatLevel) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.level == level)
            .count()
    }

    pub fn is_native(&self) -> bool {
        self.entries
            .iter()
            .all(|entry| entry.level == CompatLevel::Native)
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|error| error.to_string())
    }

    pub fn to_text(&self) -> String {
        let mut output = format!(
            "Rapport de compatibilité — {}\nNiveau global : {} ({})\n",
            self.skin,
            self.overall().code(),
            self.overall().label()
        );
        for entry in &self.entries {
            output.push_str(&format!(
                "\n[{}] {} : {}",
                entry.level.code(),
                entry.feature,
                entry.requested
            ));
            if let Some(resolved) = &entry.resolved {
                output.push_str(&format!(" → {resolved}"));
            }
            if let Some(plan) = entry.plan {
                output.push_str(&format!(" (plan {})", plan.code()));
            }
            if let Some(message) = &entry.message {
                output.push_str(&format!("\n    {message}"));
            }
            if let Some(action) = &entry.action {
                output.push_str(&format!("\n    Action : {action}"));
            }
        }
        output.push('\n');
        output
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Capability {
    pub id: String,
    pub label: String,
    pub available: bool,
    pub fallback: bool,
    pub detail: String,
    pub hint: Option<String>,
}

impl Capability {
    fn new(
        id: &str,
        label: &str,
        available: bool,
        fallback: bool,
        detail: impl Into<String>,
        hint: Option<&str>,
    ) -> Self {
        Capability {
            id: id.to_string(),
            label: label.to_string(),
            available,
            fallback,
            detail: detail.into(),
            hint: hint.map(str::to_string),
        }
    }
}

pub fn detect_capabilities() -> Vec<Capability> {
    let dbus = std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some()
        || std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .is_some_and(|runtime| runtime.join("bus").exists());
    let pipewire = std::env::var_os("PIPEWIRE_RUNTIME_DIR")
        .map(PathBuf::from)
        .is_some_and(|runtime| runtime.join("pipewire-0").exists())
        || std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .is_some_and(|runtime| runtime.join("pipewire-0").exists())
        || command_in_path("pw-cli");
    let networkmanager = command_in_path("nmcli") || Path::new("/run/NetworkManager").exists();
    let hwmon = Path::new("/sys/class/hwmon").is_dir();
    let fontconfig = command_in_path("fc-match") || command_in_path("fc-list");
    vec![
        Capability::new(
            "mpris",
            "MPRIS",
            dbus,
            false,
            if dbus {
                "Bus de session D-Bus détecté."
            } else {
                "Aucun bus de session D-Bus."
            },
            Some("Installez et démarrez une session D-Bus."),
        ),
        Capability::new(
            "networkmanager",
            "NetworkManager",
            networkmanager,
            command_in_path("iw"),
            if networkmanager {
                "NetworkManager ou nmcli disponible."
            } else {
                "NetworkManager absent ; repli sur /proc/net et iw."
            },
            Some("Installez NetworkManager pour les informations Wi-Fi détaillées."),
        ),
        Capability::new(
            "wine",
            "Wine",
            compat::wine_available(),
            false,
            if compat::wine_available() {
                "Wine est disponible pour les exécutables Windows."
            } else {
                "Wine absent ; les exécutables Windows ne sont pas lancés."
            },
            Some("Installez Wine pour exécuter les commandes .exe/.bat/.ps1."),
        ),
        Capability::new(
            "blur",
            "Flou",
            true,
            true,
            "Flou natif indisponible ; approximation CSS backdrop-filter.",
            None,
        ),
        Capability::new(
            "hwmon",
            "Capteurs hwmon",
            hwmon,
            false,
            if hwmon {
                "Capteurs matériels exposés par /sys/class/hwmon."
            } else {
                "Aucun capteur hwmon."
            },
            Some("Chargez les modules noyau lm-sensors pour les températures."),
        ),
        Capability::new(
            "pipewire",
            "PipeWire",
            pipewire,
            command_in_path("pactl"),
            if pipewire {
                "PipeWire ou PulseAudio disponible."
            } else {
                "Aucun serveur audio compatible détecté."
            },
            Some("Installez PipeWire ou PulseAudio."),
        ),
        Capability::new(
            "fontconfig",
            "Fontconfig",
            fontconfig,
            false,
            if fontconfig {
                "Fontconfig est disponible pour les polices système."
            } else {
                "Fontconfig absent ; les polices seront résolues par le WebView."
            },
            Some("Installez fontconfig pour une substitution de polices fiable."),
        ),
        Capability::new(
            "registry",
            "Registre virtuel",
            true,
            false,
            "Registre virtuel JSON et mappings Linux actifs.",
            None,
        ),
    ]
}

pub fn analyze_skin(skin: &Skin) -> CompatReport {
    let mappings = compat::mappings();
    let capabilities = detect_capabilities();
    analyze_skin_with(skin, mappings, &capabilities)
}

pub fn analyze_skin_with(
    skin: &Skin,
    mappings: &Mappings,
    capabilities: &[Capability],
) -> CompatReport {
    let mut report = CompatReport::new(skin.name.clone());
    let mut variables = Variables::new();
    for (name, value) in &skin.variables {
        variables.set(name, value);
    }
    for measure in &skin.measures {
        analyze_measure(&mut report, measure, mappings, capabilities);
    }
    for meter in &skin.meters {
        analyze_meter(&mut report, meter, mappings, &variables);
    }
    if skin.settings.blur {
        report.record_action(
            "blur",
            "Blur=1",
            Some("backdrop-filter CSS".to_string()),
            CompatLevel::Manual,
            Some(Plan::B),
            Some(
                "Le flou natif n'est pas exposé par Tauri ; approximation logicielle.".to_string(),
            ),
            Some("Vérifiez le rendu du skin avec le flou approximé.".to_string()),
        );
    }
    if let Some(value) = skin
        .settings
        .background
        .as_deref()
        .filter(|value| looks_windows(value))
    {
        record_path(&mut report, "background", value);
    }
    for font in &skin.settings.local_fonts {
        if looks_windows(font) {
            record_path(&mut report, "local_font", font);
        }
    }
    for action in actions(skin) {
        analyze_action(&mut report, "action", &variables.expand(&action), mappings);
    }
    report
}

fn analyze_measure(
    report: &mut CompatReport,
    measure: &petrichor_core::Measure,
    mappings: &Mappings,
    capabilities: &[Capability],
) {
    let kind = measure.kind.trim().to_ascii_uppercase();
    match kind.as_str() {
        "REGISTRY" => {
            let hive = measure.options.get("HKey").unwrap_or("HKEY_CURRENT_USER");
            let key = measure.options.get("KeyName").unwrap_or_default();
            let value = measure.options.get("ValueName").unwrap_or_default();
            let path = format!(
                "{}\\{}",
                hive.trim_end_matches('\\'),
                format!("{key}\\{value}").trim_matches('\\')
            );
            let resolved = backend().registry_value(&path);
            report.record_action(
                "registry",
                path.clone(),
                resolved.clone(),
                if resolved.is_some() {
                    CompatLevel::Automatic
                } else {
                    CompatLevel::Unsupported
                },
                if resolved.is_some() {
                    Some(Plan::A)
                } else {
                    Some(Plan::C)
                },
                if resolved.is_some() {
                    Some("Clé résolue par le registre virtuel ou ses mappings Linux.".to_string())
                } else {
                    Some("Clé absente du registre virtuel et des mappings GSettings.".to_string())
                },
                if resolved.is_some() {
                    None
                } else {
                    Some("Ajoutez la clé dans compat.json (section registry).".to_string())
                },
            );
        }
        "PLUGIN" => {
            let requested = measure.options.get("Plugin").unwrap_or_default().trim();
            if requested.is_empty() {
                report.record_action(
                    "plugin",
                    "(vide)",
                    None,
                    CompatLevel::Unsupported,
                    Some(Plan::C),
                    Some("Option Plugin absente ; mesure String vide.".to_string()),
                    Some("Renseignez Plugin= dans le skin.".to_string()),
                );
                return;
            }
            let module = petrichor_core::plugins::plugin_module_name(requested);
            if petrichor_core::plugins::create_builtin(&module).is_none() {
                let fallback = find_native_plugin(&module, requested);
                if fallback {
                    report.record(
                        "plugin",
                        requested,
                        Some("plugin natif".to_string()),
                        CompatLevel::Automatic,
                        Some(Plan::B),
                        Some("Plugin tiers chargé via l'ABI native.".to_string()),
                    );
                } else {
                    report.record_action(
                        "plugin",
                        requested,
                        None,
                        CompatLevel::Unsupported,
                        Some(Plan::C),
                        Some("Plugin inconnu ; mesure String vide, skin stable.".to_string()),
                        Some("Installez le plugin natif ou retirez la mesure.".to_string()),
                    );
                }
            }
        }
        "NOWPLAYING" => {
            let requested = measure.options.get("PlayerName").unwrap_or_default().trim();
            if requested.is_empty()
                || requested.eq_ignore_ascii_case("best")
                || requested.eq_ignore_ascii_case("default")
            {
                return;
            }
            let alias = mappings
                .players
                .get(&requested.to_ascii_lowercase())
                .cloned();
            if let Some(target) = alias {
                if target.eq_ignore_ascii_case(requested) {
                    return;
                }
                if target.eq_ignore_ascii_case("best") {
                    report.record_action(
                        "nowplaying",
                        requested,
                        Some("BEST".to_string()),
                        CompatLevel::Manual,
                        Some(Plan::B),
                        Some(
                            "Aucun lecteur Linux équivalent ; repli sur le premier lecteur MPRIS actif."
                                .to_string(),
                        ),
                        Some("Ajoutez un mapping dans compat.json (section players).".to_string()),
                    );
                } else {
                    report.record_action(
                        "nowplaying",
                        requested,
                        Some(target.clone()),
                        CompatLevel::Automatic,
                        Some(Plan::A),
                        Some("Lecteur MPRIS sélectionné automatiquement.".to_string()),
                        None,
                    );
                }
            } else {
                report.record_action(
                    "nowplaying",
                    requested,
                    Some("BEST".to_string()),
                    CompatLevel::Manual,
                    Some(Plan::B),
                    Some(
                        "Lecteur non reconnu ; repli sur le premier lecteur MPRIS actif."
                            .to_string(),
                    ),
                    Some("Ajoutez un alias dans compat.json (section players).".to_string()),
                );
            }
            if capabilities
                .iter()
                .find(|item| item.id == "mpris")
                .is_some_and(|item| !item.available)
            {
                report.record(
                    "nowplaying-capability",
                    "MPRIS",
                    None,
                    CompatLevel::Unsupported,
                    Some(Plan::C),
                    Some("Aucun bus MPRIS disponible ; état STOPPED et valeur vide.".to_string()),
                );
            }
        }
        "SYSINFO" => {
            let data = measure.options.get("Data").unwrap_or_default();
            if windows_only_sysinfo(data) {
                report.record_action(
                    "sysinfo",
                    data,
                    Some(String::new()),
                    CompatLevel::Manual,
                    Some(Plan::C),
                    Some("Valeur Windows sans équivalent fiable ; repli neutre.".to_string()),
                    Some("Adaptez la mesure ou masquez le meter dépendant.".to_string()),
                );
            }
        }
        "WIFISTATUS" => report.record(
            "wifi",
            "WifiStatus",
            Some("NetworkManager/iw".to_string()),
            CompatLevel::Automatic,
            Some(Plan::A),
            Some("Identifiants d'interface traduits vers NetworkManager.".to_string()),
        ),
        "RECYCLEMANAGER" => report.record(
            "trash",
            "RecycleManager",
            Some("XDG Trash".to_string()),
            CompatLevel::Automatic,
            Some(Plan::A),
            Some("Corbeille XDG utilisée.".to_string()),
        ),
        "MEDIAKEY" => report.record(
            "mediakey",
            "MediaKey",
            Some("MPRIS".to_string()),
            CompatLevel::Automatic,
            Some(Plan::A),
            Some("Touches média traduites vers MPRIS.".to_string()),
        ),
        "RUNCOMMAND" | "ACTIONTIMER" => {
            for key in ["Program", "Parameter"] {
                if let Some(value) = measure.options.get(key) {
                    analyze_command(report, key, value, mappings);
                }
            }
        }
        _ => {}
    }
}

fn analyze_meter(
    report: &mut CompatReport,
    meter: &petrichor_core::Meter,
    mappings: &Mappings,
    variables: &Variables,
) {
    if let Some(face) = meter.options.get("FontFace") {
        let face = variables.expand(face);
        if let Some(resolved) = mappings.fonts.get(&face.trim().to_ascii_lowercase()) {
            if !resolved.eq_ignore_ascii_case(face.trim()) {
                report.record_action(
                    "font",
                    face,
                    Some(resolved.clone()),
                    CompatLevel::Automatic,
                    Some(Plan::B),
                    Some("Police Windows remplacée par une police Linux.".to_string()),
                    None,
                );
            }
        }
    }
    for entry in &meter.options.entries {
        let value = variables.expand(&entry.value);
        if path_option(&entry.key) && looks_windows(&value) {
            record_path(report, &entry.key, &value);
        }
        if command_option(&entry.key) {
            analyze_command(report, &entry.key, &value, mappings);
        }
    }
}

fn analyze_command(report: &mut CompatReport, feature: &str, value: &str, mappings: &Mappings) {
    let trimmed = unwrap_command(value);
    if trimmed.is_empty() || trimmed.starts_with('!') || !command_like(trimmed) {
        return;
    }
    let translated = translate_shell_command(trimmed);
    let observed = |report: &mut CompatReport, resolved: String| {
        if resolved != trimmed {
            report.record_action(
                feature,
                trimmed,
                Some(resolved),
                CompatLevel::Automatic,
                Some(Plan::A),
                Some("Commande Windows traduite vers l'équivalent Linux.".to_string()),
                None,
            );
        }
    };
    if let Some(resolved) = translated {
        observed(report, resolved);
        return;
    }
    let wine = mappings
        .commands
        .contains_key(&trimmed.to_ascii_lowercase())
        || trimmed.to_ascii_lowercase().ends_with(".exe")
        || trimmed.to_ascii_lowercase().ends_with(".bat")
        || trimmed.to_ascii_lowercase().ends_with(".ps1");
    report.record_action(
        feature,
        trimmed,
        None,
        if wine && !compat::wine_available() {
            CompatLevel::Manual
        } else {
            CompatLevel::Unsupported
        },
        Some(Plan::C),
        Some(if wine && !compat::wine_available() {
            "Commande Windows non traduisible sans Wine.".to_string()
        } else {
            "Commande non reconnue ; elle ne sera pas exécutée.".to_string()
        }),
        Some("Vérifiez la commande ou ajoutez un mapping dans compat.json.".to_string()),
    );
}

fn unwrap_command(value: &str) -> &str {
    let trimmed = value.trim();
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(trimmed)
        .trim();
    inner.trim_matches('"').trim()
}

fn analyze_action(report: &mut CompatReport, feature: &str, value: &str, mappings: &Mappings) {
    let value = value.trim();
    if let Some(inner) = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    {
        let inner = inner.trim();
        if inner.starts_with('!') {
            for bang in inner.split('[') {
                let bang = bang.trim_end_matches(']');
                if let Some((name, rest)) = bang.split_once(char::is_whitespace) {
                    let name = name.trim_start_matches('!');
                    if is_external_command(name) {
                        analyze_command(report, feature, &format!("{name} {rest}"), mappings);
                    }
                } else if bang.starts_with('!') {
                    let name = bang.trim_start_matches('!');
                    if is_external_command(name) {
                        analyze_command(report, feature, name, mappings);
                    }
                }
            }
        } else {
            let command = inner.trim_matches('"');
            if command_like(command) {
                analyze_command(report, feature, command, mappings);
            }
        }
    } else if let Some(start) = value.find("[!") {
        for bang in value[start..].split('[') {
            let bang = bang.trim_end_matches(']');
            if let Some((name, rest)) = bang.split_once(char::is_whitespace) {
                let name = name.trim_start_matches('!');
                if is_external_command(name) {
                    analyze_command(report, feature, &format!("{name} {rest}"), mappings);
                }
            } else if bang.starts_with('!') {
                let name = bang.trim_start_matches('!');
                if is_external_command(name) {
                    analyze_command(report, feature, name, mappings);
                }
            }
        }
    } else if command_like(value) {
        analyze_command(report, feature, value, mappings);
    }
}

fn actions(skin: &Skin) -> Vec<String> {
    let settings = &skin.settings;
    let mut actions = Vec::new();
    for action in [
        &settings.on_refresh_action,
        &settings.on_close_action,
        &settings.on_focus_action,
        &settings.on_unfocus_action,
        &settings.on_update_action,
        &settings.on_wake_action,
    ]
    .into_iter()
    .flatten()
    {
        actions.push(action.clone());
    }
    for item in &settings.context_menu {
        actions.push(item.action.clone());
    }
    actions
}

fn record_path(report: &mut CompatReport, feature: &str, value: &str) {
    let resolved = petrichor_core::compat::translate_path_checked(value);
    let translated = resolved
        .as_deref()
        .is_some_and(|resolved| resolved != value.trim());
    report.record_action(
        format!("path:{feature}"),
        value,
        resolved,
        if translated {
            CompatLevel::Automatic
        } else {
            CompatLevel::Manual
        },
        Some(if translated { Plan::A } else { Plan::C }),
        Some(if translated {
            "Chemin ou variable Windows traduit.".to_string()
        } else {
            "Chemin Windows non résolu ; accès neutre.".to_string()
        }),
        if translated {
            None
        } else {
            Some("Vérifiez le point de montage ou ajoutez un mapping de chemin.".to_string())
        },
    );
}

fn windows_only_sysinfo(data: &str) -> bool {
    matches!(
        data.trim().to_ascii_uppercase().as_str(),
        "SID" | "DOMAIN" | "USERLOGONTIME" | "LOGONTIME" | "OSVERSION" | "ADMINDIR"
    )
}

fn looks_windows(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("%appdata%")
        || lower.contains("%localappdata%")
        || lower.contains("%userprofile%")
        || lower.contains("%temp%")
        || lower.contains("%programfiles%")
        || lower.contains("c:\\")
        || lower.contains('\\')
}

fn command_like(value: &str) -> bool {
    let first = value.split_whitespace().next().unwrap_or_default();
    let lower = first.to_ascii_lowercase();
    first.starts_with('[')
        || looks_windows(first)
        || lower.ends_with(".exe")
        || lower.ends_with(".bat")
        || lower.ends_with(".ps1")
        || lower == "cmd"
        || lower == "cmd.exe"
        || lower == "start"
        || lower == "tasklist"
        || lower == "get-process"
        || lower == "ping"
        || lower == "dir"
        || lower == "type"
        || lower == "del"
        || lower == "copy"
        || lower == "move"
        || lower == "where"
}

fn is_external_command(name: &str) -> bool {
    !matches!(
        name.to_ascii_uppercase().as_str(),
        "UPDATE"
            | "REDRAW"
            | "REFRESH"
            | "HIDE"
            | "SHOW"
            | "TOGGLE"
            | "HIDEFADE"
            | "SHOWFADE"
            | "TOGGLEFADE"
            | "ZPOS"
            | "CLICKTHROUGH"
            | "DRAGGABLE"
            | "SNAPEDGES"
            | "KEEPONSCREEN"
            | "AUTOSELECTSCREEN"
            | "SETTRANSPARENCY"
            | "SETVARIABLE"
            | "SETOPTION"
            | "MOVE"
            | "BANG"
            | "EXECUTE"
            | "DELAY"
            | "PLAY"
            | "PLAYLOOP"
            | "PLAYSTOP"
            | "ABOUT"
            | "MANAGE"
    ) && command_like(name)
}

fn path_option(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "imagename"
            | "barimage"
            | "background"
            | "image"
            | "localimage"
            | "file"
            | "path"
            | "folder"
            | "downloadfile"
            | "icon"
            | "cover"
            | "coverpath"
            | "tooltipicon"
    )
}

fn command_option(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.ends_with("action")
        || key.ends_with("command")
        || matches!(
            key.as_str(),
            "program" | "parameter" | "startin" | "onrefreshaction" | "oncloseaction"
        )
}

fn find_native_plugin(module: &str, requested: &str) -> bool {
    if Path::new(requested).is_file() {
        return true;
    }
    let roots = [
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
            .unwrap_or_default()
            .join("petrichor")
            .join("Plugins"),
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(".wine"))
            .unwrap_or_default(),
    ];
    roots.iter().any(|root| {
        root.join(format!("{module}.so")).is_file() || root.join(format!("{module}.dll")).is_file()
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct MigrationChange {
    pub line: usize,
    pub key: String,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MigrationResult {
    pub path: PathBuf,
    pub backup: PathBuf,
    pub changes: Vec<MigrationChange>,
}

pub fn preview_migration(path: &Path) -> Result<Vec<MigrationChange>, String> {
    plan_migration(path).map(|(changes, _)| changes)
}

fn plan_migration(path: &Path) -> Result<(Vec<MigrationChange>, String), String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let text = Encoding::detect(&bytes).decode(&bytes);
    let mappings = compat::mappings();
    let mut changes = Vec::new();
    let mut output = String::with_capacity(text.len());
    for (index, line) in text.split_inclusive('\n').enumerate() {
        let (body, newline) = line
            .strip_suffix("\r\n")
            .map(|body| (body, "\r\n"))
            .or_else(|| line.strip_suffix('\n').map(|body| (body, "\n")))
            .unwrap_or((line, ""));
        let trimmed = body.trim_start();
        if trimmed.starts_with(';') || trimmed.starts_with('#') || trimmed.starts_with('[') {
            output.push_str(body);
            output.push_str(newline);
            continue;
        }
        let Some((key, value)) = body.split_once('=') else {
            output.push_str(body);
            output.push_str(newline);
            continue;
        };
        let key = key.trim();
        let after = migrate_value(key, value.trim(), mappings);
        if after != value.trim() {
            changes.push(MigrationChange {
                line: index + 1,
                key: key.to_string(),
                before: value.trim().to_string(),
                after: after.clone(),
            });
            output.push_str(&format!("{key}={after}{newline}"));
        } else {
            output.push_str(body);
            output.push_str(newline);
        }
    }
    Ok((changes, output))
}

pub fn migrate_skin(path: &Path) -> Result<MigrationResult, String> {
    let (changes, output) = plan_migration(path)?;
    if changes.is_empty() {
        return Ok(MigrationResult {
            path: path.to_path_buf(),
            backup: PathBuf::new(),
            changes,
        });
    }
    let backup = PathBuf::from(format!("{}.petrichor.bak", path.display()));
    std::fs::copy(path, &backup).map_err(|error| error.to_string())?;
    std::fs::write(path, output).map_err(|error| error.to_string())?;
    Ok(MigrationResult {
        path: path.to_path_buf(),
        backup,
        changes,
    })
}

fn migrate_value(key: &str, value: &str, mappings: &Mappings) -> String {
    match key.to_ascii_lowercase().as_str() {
        "fontface" => mappings
            .fonts
            .get(&value.trim().to_ascii_lowercase())
            .cloned()
            .unwrap_or_else(|| value.to_string()),
        "playername" => mappings
            .players
            .get(&value.trim().to_ascii_lowercase())
            .cloned()
            .unwrap_or_else(|| value.to_string()),
        key if path_option(key) => petrichor_core::compat::translate_path_checked(value)
            .unwrap_or_else(|| value.to_string()),
        key if command_option(key) => {
            if looks_windows(value) || command_like(value) {
                translate_shell_command(value)
                    .or_else(|| petrichor_core::compat::translate_path_checked(value))
                    .unwrap_or_else(|| value.to_string())
            } else {
                value.to_string()
            }
        }
        _ => {
            if looks_windows(value) {
                petrichor_core::compat::translate_path_checked(value)
                    .unwrap_or_else(|| value.to_string())
            } else {
                value.to_string()
            }
        }
    }
}

pub fn export_report(report: &CompatReport) -> Result<PathBuf, String> {
    let directory = compat::compat_path()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .ok_or_else(|| "répertoire de configuration introuvable".to_string())?
        .join("Compatibility");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let name = report
        .skin
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let path = directory.join(format!("{name}.json"));
    std::fs::write(
        &path,
        serde_json::to_string_pretty(report).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(path)
}

pub fn registry_defaults() -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    values.insert(
        "HKCU\\Control Panel\\International\\LocaleName".to_string(),
        std::env::var("LANG")
            .unwrap_or_else(|_| "en_US.UTF-8".to_string())
            .split('.')
            .next()
            .unwrap_or("en_US")
            .to_string(),
    );
    values.insert(
        "HKCU\\Control Panel\\Desktop\\WallpaperStyle".to_string(),
        "10".to_string(),
    );
    let release = petrichor_sys::OsRelease::read();
    values.insert(
        "HKLM\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\ProductName".to_string(),
        release.name,
    );
    values.insert(
        "HKLM\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\CurrentVersion".to_string(),
        release.version,
    );
    values
}

pub fn missing_capabilities(report: &CompatReport) -> Vec<String> {
    let mut missing = BTreeSet::new();
    for entry in &report.entries {
        if entry.level == CompatLevel::Unsupported {
            if let Some(plan) = entry.plan {
                missing.insert(format!("{} ({})", entry.feature, plan.code()));
            }
        }
    }
    missing.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn skin_file(content: &str) -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Skin.ini");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        (directory, path)
    }

    #[test]
    fn report_tracks_levels_and_overall() {
        let mut report = CompatReport::new("Test");
        report.record(
            "plugin",
            "Weather",
            None,
            CompatLevel::Unsupported,
            Some(Plan::C),
            None,
        );
        report.record(
            "font",
            "Arial",
            Some("Liberation Sans".to_string()),
            CompatLevel::Automatic,
            Some(Plan::B),
            None,
        );
        assert_eq!(report.overall(), CompatLevel::Unsupported);
        assert!(report.has_unsupported());
        assert_eq!(report.count_by_level(CompatLevel::Automatic), 1);
        assert!(report.to_text().contains("N3"));
    }

    #[test]
    fn analyzes_registry_fonts_and_plugins() {
        let (_directory, path) = skin_file(
            "[Metadata]\nName=Compat\n[Variables]\n[MeterText]\nMeter=String\nFontFace=Segoe UI\n\
             [MeasureReg]\nMeasure=Registry\nHKey=HKCU\nKeyName=Software\\Petrichor\nValueName=Theme\n\
             [MeasurePlugin]\nMeasure=Plugin\nPlugin=Unknown.dll\n",
        );
        let (skin, _) = petrichor_core::build_skin(&path);
        let mappings = Mappings::builtin();
        let capabilities = detect_capabilities();
        let report = analyze_skin_with(&skin, &mappings, &capabilities);
        assert!(report
            .entries
            .iter()
            .any(|entry| entry.feature == "font" && entry.level == CompatLevel::Automatic));
        assert!(report
            .entries
            .iter()
            .any(|entry| entry.feature == "plugin" && entry.level == CompatLevel::Unsupported));
    }

    #[test]
    fn migrates_fonts_paths_and_players() {
        let (directory, path) = skin_file(
            "[MeterText]\nFontFace=Segoe UI\nImageName=C:\\Users\\me\\bg.png\n\
             [MeasurePlayer]\nPlayerName=Winamp\n",
        );
        let result = migrate_skin(&path).unwrap();
        assert!(!result.changes.is_empty());
        assert!(result.backup.is_file());
        let migrated = std::fs::read_to_string(&path).unwrap();
        assert!(migrated.contains("FontFace=Cantarell"));
        assert!(migrated.contains("PlayerName=audacious"));
        assert!(migrated.contains("ImageName=/"));
        assert!(!migrated.contains("Segoe UI"));
        std::fs::remove_dir_all(directory.path()).ok();
    }

    #[test]
    fn previews_migration_without_writing() {
        let (directory, path) = skin_file("[MeterText]\nFontFace=Segoe UI\n");
        let before = std::fs::read_to_string(&path).unwrap();
        let changes = preview_migration(&path).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].after, "Cantarell");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
        assert!(!path.with_extension("ini.petrichor.bak").exists());
        std::fs::remove_dir_all(directory.path()).ok();
    }

    #[test]
    fn reports_blur_background_and_unresolved_paths() {
        let (_directory, path) = skin_file(
            "[Petrichor]\nBlur=1\nBackground=C:\\PetrichorMissing\\bg.png\nLocalFont=C:\\Users\\bob\\font.ttf\n",
        );
        let (skin, _) = petrichor_core::build_skin(&path);
        let report = analyze_skin_with(&skin, &Mappings::builtin(), &detect_capabilities());
        assert!(report
            .entries
            .iter()
            .any(|entry| entry.feature == "blur" && entry.level == CompatLevel::Manual));
        assert!(report
            .entries
            .iter()
            .any(|entry| entry.feature == "path:background" && entry.level == CompatLevel::Manual));
        assert!(report.entries.iter().any(
            |entry| entry.feature == "path:local_font" && entry.level == CompatLevel::Automatic
        ));
    }

    #[test]
    fn leaves_compatible_skin_unchanged() {
        let (_directory, path) = skin_file("[MeterText]\nMeter=String\nFontFace=Cantarell\n");
        let result = migrate_skin(&path).unwrap();
        assert!(result.changes.is_empty());
        assert!(result.backup.as_os_str().is_empty());
    }

    #[test]
    fn exposes_capabilities_and_defaults() {
        let capabilities = detect_capabilities();
        assert!(capabilities.iter().any(|item| item.id == "mpris"));
        let defaults = registry_defaults();
        assert!(defaults.contains_key("HKCU\\Control Panel\\Desktop\\WallpaperStyle"));
    }
}
