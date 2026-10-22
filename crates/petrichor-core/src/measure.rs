use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Local};
use petrichor_sys::{CpuTimes, NetSample, SystemBackend};

use crate::formula::{self, EvalContext};
use crate::model::{Measure, Meter, Options, Skin};
use crate::plugins::{RuntimeContext, RuntimeSlot};
use crate::readers::{read_bool, read_f64, read_u32};
use crate::reference::{resolve_in_text, MeasureSource, MeasureValue};
use crate::variables::Variables;

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateReport {
    pub strings: BTreeMap<String, String>,
    pub values: BTreeMap<String, MeasureValue>,
    pub actions: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MeasureEngine {
    states: Vec<MeasureState>,
    variables: Variables,
    update_number: u64,
    rng: Cell<u64>,
    stats: BTreeMap<String, f64>,
    stats_path: Option<PathBuf>,
    stats_dirty: bool,
    window_handle: u64,
}

#[derive(Debug, Default)]
struct WebFetchState {
    generation: u64,
    in_flight: Option<u64>,
    result: Option<(u64, Result<WebContent, ()>)>,
}

#[derive(Debug, Clone)]
struct WebContent {
    text: String,
    path: Option<String>,
}

#[derive(Debug, Clone)]
struct WebRequest {
    url: String,
    user_agent: Option<String>,
    headers: Vec<(String, String)>,
    proxy: Option<String>,
    flags: Vec<String>,
    code_page: u32,
    download: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct MeasureState {
    name: String,
    key: String,
    kind: String,
    options: Options,
    update_divider: u32,
    disabled: bool,
    paused: bool,
    invert: bool,
    substitute: Substitute,
    last_value: MeasureValue,
    loop_value: f64,
    loop_initialized: bool,
    previous_cpu: Option<CpuTimes>,
    previous_cores: Vec<CpuTimes>,
    average_size: usize,
    average_buffer: Vec<f64>,
    net_sample: Option<NetSample>,
    net_accumulator: f64,
    net_loaded: bool,
    net_last_time_ms: Option<i64>,
    auto_max_samples: Vec<f64>,
    if_states: BTreeMap<String, bool>,
    root: PathBuf,
    web_content: String,
    web_path: Option<String>,
    web_shared: Arc<Mutex<WebFetchState>>,
    web_last_update: Option<u64>,
    web_loaded: bool,
    now_track: Option<String>,
    now_players: Arc<Mutex<BTreeMap<String, String>>>,
    skin_name: String,
    skin_path: PathBuf,
    meters: Arc<Vec<Meter>>,
    sections: Arc<BTreeMap<String, BTreeMap<String, String>>>,
    runtime: Option<RuntimeSlot>,
    pending_actions: Vec<String>,
}

impl MeasureEngine {
    pub fn new(skin: &Skin) -> Self {
        let mut variables = Variables::new();
        for (name, value) in &skin.variables {
            variables.set(name, value);
        }

        let now_players = Arc::new(Mutex::new(BTreeMap::new()));
        let sections = Arc::new(skin.sections.clone());
        let mut states: Vec<MeasureState> = skin
            .measures
            .iter()
            .map(|measure| {
                MeasureState::new(
                    measure,
                    skin.settings.default_update_divider,
                    skin,
                    sections.clone(),
                    now_players.clone(),
                )
            })
            .collect();
        let stats_path = if cfg!(test) { None } else { stats_path(skin) };
        let stats = stats_path
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str::<BTreeMap<String, f64>>(&text).ok())
            .unwrap_or_default();
        for state in &mut states {
            if let Some(value) = stats.get(&state.key) {
                state.net_accumulator = *value;
                state.net_loaded = true;
            }
        }

        MeasureEngine {
            states,
            variables,
            update_number: 0,
            rng: Cell::new(seed()),
            stats,
            stats_path,
            stats_dirty: false,
            window_handle: 0,
        }
    }

    pub fn with_seed(mut self, seed: u64) -> Self {
        self.rng = Cell::new(seed);
        self
    }

    pub fn variables(&self) -> &Variables {
        &self.variables
    }

    pub fn measure_kind(&self, name: &str) -> Option<&str> {
        self.states
            .iter()
            .find(|state| state.name.eq_ignore_ascii_case(name))
            .map(|state| state.kind.as_str())
    }

    pub fn command_measure(&mut self, name: &str, command: &str) -> bool {
        let Some(index) = self
            .states
            .iter()
            .position(|state| state.name.eq_ignore_ascii_case(name))
        else {
            return false;
        };
        let kind = self.states[index].kind.clone();
        if kind == "NOWPLAYING" {
            let request = now_player_request(
                &self.states[index],
                &self.variables,
                &self.states[index].now_players,
            );
            return petrichor_sys::backend().media_control(&request, command);
        }
        if kind == "MEDIAKEY" {
            return petrichor_sys::backend().media_key(command.trim());
        }
        if kind == "PLUGIN" || kind == "SCRIPT" {
            let state = &self.states[index];
            let Some(runtime) = state.runtime.clone() else {
                return false;
            };
            let backend = petrichor_sys::backend();
            let context = RuntimeContext {
                skin_name: &state.skin_name,
                skin_root: &state.root,
                skin_path: &state.skin_path,
                measure_name: &state.name,
                options: &state.options,
                variables: &self.variables,
                backend: &backend,
                values: &BTreeMap::new(),
                meters: &state.meters,
                sections: &state.sections,
                skin_window_handle: self.window_handle,
                counter: self.update_number as f64,
            };
            return runtime.command(command, &context);
        }
        let state = &mut self.states[index];
        match (
            state.kind.as_str(),
            command.trim().to_ascii_lowercase().as_str(),
        ) {
            ("WEBPARSER", "update" | "reset") => {
                state.web_last_update = None;
                if command.trim().eq_ignore_ascii_case("reset") {
                    state.web_content.clear();
                    state.web_path = None;
                    state.web_loaded = false;
                }
                if let Ok(mut shared) = state.web_shared.lock() {
                    shared.generation = shared.generation.wrapping_add(1);
                    shared.in_flight = None;
                    shared.result = None;
                }
                true
            }
            ("LOOP", "reset") => {
                state.loop_initialized = false;
                true
            }
            _ => false,
        }
    }

    pub fn set_variable(&mut self, name: &str, value: &str) {
        self.variables.set(name, value);
    }

    pub fn set_window_handle(&mut self, handle: u64) {
        self.window_handle = handle;
    }

    pub fn tick(&mut self, backend: &dyn SystemBackend) -> UpdateReport {
        self.update(backend, Local::now())
    }

    pub fn update(&mut self, backend: &dyn SystemBackend, now: DateTime<Local>) -> UpdateReport {
        self.update_number += 1;
        self.sync_states();

        let mut values: BTreeMap<String, MeasureValue> = self
            .states
            .iter()
            .map(|state| (state.key.clone(), state.last_value.clone()))
            .collect();
        let runtimes: BTreeMap<String, RuntimeSlot> = self
            .states
            .iter()
            .filter_map(|state| {
                state
                    .runtime
                    .clone()
                    .map(|runtime| (state.key.clone(), runtime))
            })
            .collect();
        let mut actions = Vec::new();

        for index in 0..self.states.len() {
            let due = {
                let state = &self.states[index];
                !state.disabled
                    && !state.paused
                    && self
                        .update_number
                        .is_multiple_of(u64::from(state.update_divider))
            };

            if !due {
                if self.states[index].disabled {
                    let value = empty_value();
                    self.states[index].if_states.clear();
                    self.states[index].last_value = value.clone();
                    values.insert(self.states[index].key.clone(), value);
                }
                continue;
            }

            let previous = self.states[index].last_value.clone();
            let measure_name = self.states[index].name.clone();
            let options = self.states[index].options.clone();
            let runtime = self.states[index].runtime.clone();
            let skin_name = self.states[index].skin_name.clone();
            let skin_path = self.states[index].skin_path.clone();
            let skin_root = self.states[index].root.clone();
            let meters = self.states[index].meters.clone();
            let sections = self.states[index].sections.clone();
            let context = Context {
                values: &values,
                variables: &self.variables,
                options: &options,
                runtime: runtime.as_ref(),
                runtimes: &runtimes,
                backend,
                skin_name: &skin_name,
                skin_root: &skin_root,
                skin_path: &skin_path,
                measure_name: &measure_name,
                meters: &meters,
                sections: &sections,
                skin_window_handle: self.window_handle,
                counter: self.update_number as f64,
                rng: &self.rng,
            };

            let mut value = compute(&mut self.states[index], &context, backend, now);
            apply_average(&mut self.states[index], &mut value);
            apply_bounds(&self.states[index], &mut value);
            let text = self.states[index].substitute.apply(&display_text(&value));
            value.string = Some(text.clone());

            if let Some(action) = self.states[index].options.get("OnUpdateAction") {
                actions.push(self.variables.expand(action));
            }
            if previous.string.as_deref().unwrap_or_default() != text {
                if let Some(action) = self.states[index].options.get("OnChangeAction") {
                    actions.push(self.variables.expand(action));
                }
            }
            let mut if_values = values.clone();
            if_values.insert(self.states[index].key.clone(), value.clone());
            let if_context = Context {
                values: &if_values,
                variables: &self.variables,
                options: &options,
                runtime: runtime.as_ref(),
                runtimes: &runtimes,
                backend,
                skin_name: &skin_name,
                skin_root: &skin_root,
                skin_path: &skin_path,
                measure_name: &measure_name,
                meters: &meters,
                sections: &sections,
                skin_window_handle: self.window_handle,
                counter: self.update_number as f64,
                rng: &self.rng,
            };
            actions.extend(if_actions(
                &mut self.states[index],
                &if_context,
                &value,
                &text,
            ));
            actions.extend(std::mem::take(&mut self.states[index].pending_actions));

            values.insert(self.states[index].key.clone(), value.clone());
            self.states[index].last_value = value;
        }

        let mut strings = BTreeMap::new();
        let mut values_by_name = BTreeMap::new();
        for state in &self.states {
            strings.insert(state.name.clone(), display_text(&state.last_value));
            values_by_name.insert(state.name.clone(), state.last_value.clone());
        }

        for state in &self.states {
            if state.kind.starts_with("NET")
                && cumulative_enabled(state)
                && self.stats.get(&state.key) != Some(&state.net_accumulator)
            {
                self.stats.insert(state.key.clone(), state.net_accumulator);
                self.stats_dirty = true;
            }
        }
        if self.stats_dirty {
            if let Some(path) = &self.stats_path {
                if let Ok(text) = serde_json::to_string(&self.stats) {
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let temporary = path.with_extension("tmp");
                    if std::fs::write(&temporary, text).is_ok() {
                        let _ = std::fs::rename(&temporary, path);
                    }
                }
                self.stats_dirty = false;
            }
        }

        UpdateReport {
            strings,
            values: values_by_name,
            actions,
        }
    }

    fn sync_states(&mut self) {
        for state in &mut self.states {
            if let Some(divider) = state.options.get("UpdateDivider") {
                state.update_divider = read_u32(divider, state.update_divider).max(1);
            }
            if let Some(value) = state.options.get("Disabled") {
                state.disabled = read_bool(value, state.disabled);
            }
            if let Some(value) = state.options.get("Paused") {
                state.paused = read_bool(value, state.paused);
            }
            if let Some(value) = state.options.get("AverageSize") {
                state.average_size = read_u32(value, state.average_size as u32) as usize;
            }
        }
    }
}

impl MeasureState {
    fn new(
        measure: &Measure,
        default_divider: u32,
        skin: &Skin,
        sections: Arc<BTreeMap<String, BTreeMap<String, String>>>,
        now_players: Arc<Mutex<BTreeMap<String, String>>>,
    ) -> Self {
        let kind = measure.kind.trim().to_ascii_uppercase();
        let runtime = match kind.as_str() {
            "PLUGIN" => crate::plugins::create_plugin_measure(
                measure,
                &skin.name,
                &skin.root,
                &skin.ini_path,
            ),
            "SCRIPT" => crate::plugins::create_script_measure(measure, &skin.root),
            _ => None,
        }
        .map(RuntimeSlot::new);
        MeasureState {
            name: measure.name.clone(),
            key: measure.name.to_ascii_lowercase(),
            kind,
            options: measure.options.clone(),
            update_divider: measure
                .options
                .get("UpdateDivider")
                .map(|value| read_u32(value, 1))
                .unwrap_or(default_divider)
                .max(1),
            disabled: measure
                .options
                .get("Disabled")
                .map(|value| read_bool(value, false))
                .unwrap_or(false),
            paused: measure
                .options
                .get("Paused")
                .map(|value| read_bool(value, false))
                .unwrap_or(false),
            invert: measure
                .options
                .get("InvertMeasure")
                .map(|value| read_bool(value, false))
                .unwrap_or(false),
            substitute: Substitute::from_options(&measure.options),
            last_value: empty_value(),
            loop_value: 0.0,
            loop_initialized: false,
            previous_cpu: None,
            previous_cores: Vec::new(),
            average_size: measure
                .options
                .get("AverageSize")
                .map(|value| read_u32(value, 0) as usize)
                .unwrap_or(0),
            average_buffer: Vec::new(),
            net_sample: None,
            net_accumulator: 0.0,
            net_loaded: false,
            net_last_time_ms: None,
            auto_max_samples: Vec::new(),
            if_states: BTreeMap::new(),
            root: skin.root.clone(),
            web_content: String::new(),
            web_path: None,
            web_shared: Arc::new(Mutex::new(WebFetchState::default())),
            web_last_update: None,
            web_loaded: false,
            now_track: None,
            now_players,
            skin_name: skin.name.clone(),
            skin_path: skin.ini_path.clone(),
            meters: Arc::new(skin.meters.clone()),
            sections,
            runtime,
            pending_actions: Vec::new(),
        }
    }
}

fn compute(
    state: &mut MeasureState,
    context: &Context<'_>,
    backend: &dyn SystemBackend,
    now: DateTime<Local>,
) -> MeasureValue {
    match state.kind.as_str() {
        "TIME" => time_value(state, context, now),
        "UPTIME" => uptime_value(state, context, backend),
        "CPU" => cpu_value(state, backend),
        "MEMORY" => memory_value(state, backend, MemoryKind::Both),
        "PHYSICALMEMORY" => memory_value(state, backend, MemoryKind::Physical),
        "SWAPMEMORY" | "VIRTUALMEMORY" => memory_value(state, backend, MemoryKind::Swap),
        "FREEDISKSPACE" => disk_value(state, context, backend),
        "NETIN" => net_value(state, context, backend, NetKind::In, now),
        "NETOUT" => net_value(state, context, backend, NetKind::Out, now),
        "NETTOTAL" => net_value(state, context, backend, NetKind::Total, now),
        "PROCESS" => process_value(state, context, backend),
        "RECYCLEMANAGER" => recycle_value(state, context, backend),
        "SYSINFO" => sysinfo_value(state, context, backend),
        "WIFISTATUS" => wifi_value(state, context, backend),
        "WEBPARSER" => webparser_value(state, context),
        "REGISTRY" => registry_measure_value(state, context, backend),
        "NOWPLAYING" => now_playing_value(state, context, backend),
        "PLUGIN" | "SCRIPT" => runtime_value(state, context, backend),
        "MEDIAKEY" => empty_value(),
        "STRING" => string_value(state, context),
        "CALC" => calc_value(state, context),
        "LOOP" => loop_value(state),
        _ => empty_value(),
    }
}

fn time_value(state: &MeasureState, context: &Context<'_>, now: DateTime<Local>) -> MeasureValue {
    let format = state
        .options
        .get("Format")
        .map(|value| context.variables.expand(value))
        .unwrap_or_else(|| "%H:%M:%S".to_string());
    let text = format_time(now, &format);
    MeasureValue {
        number: 0.0,
        string: Some(text),
        min: None,
        max: None,
    }
}

fn format_time(now: DateTime<Local>, format: &str) -> String {
    use chrono::format::{Item, StrftimeItems};

    let items: Vec<Item> = StrftimeItems::new(format).collect();
    if items.iter().any(|item| matches!(item, Item::Error)) {
        return now.format("%H:%M:%S").to_string();
    }
    now.format_with_items(items.iter().cloned()).to_string()
}

fn uptime_value(
    state: &MeasureState,
    context: &Context<'_>,
    backend: &dyn SystemBackend,
) -> MeasureValue {
    let total = backend.uptime_secs();
    let days = total / 86_400;
    let hours = (total % 86_400) / 3_600;
    let minutes = (total % 3_600) / 60;
    let seconds = total % 60;

    let text = match state.options.get("Format") {
        Some(format) => context
            .variables
            .expand(format)
            .replace("%d", &days.to_string())
            .replace("%H", &format!("{hours:02}"))
            .replace("%M", &format!("{minutes:02}"))
            .replace("%S", &format!("{seconds:02}")),
        None => format!("{days}d {hours:02}:{minutes:02}:{seconds:02}"),
    };

    MeasureValue {
        number: total as f64,
        string: Some(text),
        min: None,
        max: None,
    }
}

fn cpu_value(state: &mut MeasureState, backend: &dyn SystemBackend) -> MeasureValue {
    let snapshot = backend.cpu_snapshot();
    let processor = state
        .options
        .get("Processor")
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .map(|value| value - 1);

    let (current, previous) = match processor {
        Some(index) => (
            snapshot.cores.get(index).copied(),
            state.previous_cores.get(index).copied(),
        ),
        None => (Some(snapshot.overall), state.previous_cpu),
    };

    let usage = match (current, previous) {
        (Some(current), Some(previous)) => current.usage_percent(previous),
        _ => 0.0,
    };

    state.previous_cpu = Some(snapshot.overall);
    state.previous_cores = snapshot.cores;

    MeasureValue {
        number: usage,
        string: Some(format_display(usage)),
        min: Some(0.0),
        max: Some(100.0),
    }
}

enum MemoryKind {
    Both,
    Physical,
    Swap,
}

fn memory_value(
    state: &MeasureState,
    backend: &dyn SystemBackend,
    kind: MemoryKind,
) -> MeasureValue {
    let info = backend.memory_info();
    let total = read_bool_option(state, "Total");
    let (used, maximum) = match kind {
        MemoryKind::Both => (
            (info.used_kb() + info.swap_used_kb()) as f64 / 1024.0,
            (info.total_kb + info.swap_total_kb) as f64 / 1024.0,
        ),
        MemoryKind::Physical => (
            info.used_kb() as f64 / 1024.0,
            info.total_kb as f64 / 1024.0,
        ),
        MemoryKind::Swap => (
            info.swap_used_kb() as f64 / 1024.0,
            info.swap_total_kb as f64 / 1024.0,
        ),
    };
    let (number, maximum) = if total {
        (maximum, maximum)
    } else {
        (used, maximum)
    };
    MeasureValue {
        number,
        string: Some(format_display(number)),
        min: Some(0.0),
        max: Some(maximum),
    }
}

fn webparser_value(state: &mut MeasureState, context: &Context<'_>) -> MeasureValue {
    if let Some((source, index)) = web_chain_source(state, context) {
        let text = web_extract(state, context, &source, index);
        return web_measure_value(text);
    }

    poll_web_result(state, context);

    let update_rate = state
        .options
        .get("UpdateRate")
        .map(|value| read_u32(value, 600) as u64)
        .unwrap_or(600)
        .max(1);
    let counter = context.counter as u64;
    let refresh = state
        .web_last_update
        .is_none_or(|last| counter.saturating_sub(last) >= update_rate);
    if refresh {
        let in_flight = state
            .web_shared
            .lock()
            .ok()
            .and_then(|shared| shared.in_flight)
            .is_some();
        if !in_flight {
            state.web_last_update = Some(counter);
            start_web_fetch(state, context);
        }
    }

    if let Some(path) = &state.web_path {
        return MeasureValue {
            number: 0.0,
            string: Some(path.clone()),
            min: None,
            max: None,
        };
    }

    let text = if state.web_loaded {
        let pattern = state
            .options
            .get("RegExp")
            .map(|value| context.variables.expand(value))
            .unwrap_or_default();
        let index = state
            .options
            .get("StringIndex")
            .map(|value| read_u32(value, 0) as usize)
            .filter(|index| *index > 0)
            .or_else(|| {
                state
                    .options
                    .get("StringIndex2")
                    .map(|value| read_u32(value, 0) as usize)
                    .filter(|index| *index > 0)
            })
            .unwrap_or(0);
        web_extract_with_pattern(state, context, &state.web_content.clone(), &pattern, index)
    } else {
        state.web_content.clone()
    };

    web_measure_value(text)
}

fn web_measure_value(text: String) -> MeasureValue {
    MeasureValue {
        number: text.trim().parse().unwrap_or(0.0),
        string: Some(text),
        min: None,
        max: None,
    }
}

fn web_chain_source(state: &MeasureState, context: &Context<'_>) -> Option<(String, usize)> {
    let raw = state.options.get("Url")?;
    let expanded = context.variables.expand(raw);
    let trimmed = expanded.trim();
    if !trimmed.contains('[') || trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return None;
    }
    let index = state
        .options
        .get("StringIndex2")
        .map(|value| read_u32(value, 0) as usize)
        .filter(|index| *index > 0)
        .or_else(|| {
            state
                .options
                .get("StringIndex")
                .map(|value| read_u32(value, 0) as usize)
                .filter(|index| *index > 0)
        })
        .unwrap_or(0);
    Some((resolve_in_text(&expanded, context), index))
}

fn web_extract(
    state: &mut MeasureState,
    context: &Context<'_>,
    source: &str,
    index: usize,
) -> String {
    let pattern = state
        .options
        .get("RegExp")
        .map(|value| context.variables.expand(value))
        .unwrap_or_default();
    web_extract_with_pattern(state, context, source, &pattern, index)
}

fn web_extract_with_pattern(
    state: &mut MeasureState,
    context: &Context<'_>,
    source: &str,
    pattern: &str,
    index: usize,
) -> String {
    if pattern.is_empty() {
        return source.to_string();
    }
    let fallback = state
        .options
        .get("ErrorString")
        .map(|value| context.variables.expand(value))
        .unwrap_or_default();
    match apply_web_regex(source, pattern, index) {
        Some(mut text) => {
            let decode = state
                .options
                .get("DecodeCharacterReference")
                .map(|value| read_u32(value, 0))
                .unwrap_or(0);
            if decode > 0 {
                text = decode_character_references(&text);
            }
            let decode_codepoints = state
                .options
                .get("DecodeCodePoints")
                .map(|value| read_bool(value, false))
                .unwrap_or(false);
            if decode_codepoints {
                text = decode_code_points(&text);
            }
            text
        }
        None => {
            let log_errors = state
                .options
                .get("LogSubstringErrors")
                .map(|value| read_bool(value, true))
                .unwrap_or(true);
            if log_errors {
                tracing::warn!(
                    measure = %state.name,
                    pattern = %pattern,
                    index,
                    "WebParser : sous-chaîne introuvable"
                );
            }
            if let Some(action) = state.options.get("OnRegExpErrorAction") {
                state.pending_actions.push(context.variables.expand(action));
            }
            fallback
        }
    }
}

fn web_debug_dump(state: &MeasureState, context: &Context<'_>, content: &str) {
    let debug = state
        .options
        .get("Debug")
        .map(|value| read_u32(value, 0))
        .unwrap_or(0);
    if debug == 0 {
        return;
    }
    if debug >= 2 {
        let file = state
            .options
            .get("Debug2File")
            .map(|value| resolve_in_text(&context.variables.expand(value), context))
            .unwrap_or_else(|| "WebParserDump.txt".to_string());
        let path = PathBuf::from(&file);
        let path = if path.is_absolute() {
            path
        } else {
            state.root.join(path)
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(error) = std::fs::write(&path, content) {
            tracing::warn!(
                measure = %state.name,
                path = %path.display(),
                %error,
                "WebParser : impossible d'écrire le dump"
            );
        }
    } else {
        tracing::debug!(measure = %state.name, len = content.len(), "WebParser : contenu reçu");
    }
}

fn web_flags(state: &MeasureState) -> Vec<String> {
    state
        .options
        .get("Flags")
        .map(|value| {
            value
                .split([',', ' ', '|', ';'])
                .filter(|flag| !flag.trim().is_empty())
                .map(|flag| flag.trim().to_ascii_lowercase())
                .collect()
        })
        .unwrap_or_default()
}

fn poll_web_result(state: &mut MeasureState, context: &Context<'_>) {
    let result = {
        let Ok(mut shared) = state.web_shared.lock() else {
            return;
        };
        match shared.result.take() {
            Some((generation, result)) if generation == shared.generation => Some(result),
            _ => None,
        }
    };
    let Some(result) = result else {
        return;
    };
    match result {
        Ok(content) => {
            web_debug_dump(state, context, &content.text);
            state.web_content = content.text;
            state.web_path = content.path;
            state.web_loaded = true;
            if let Some(action) = state.options.get("FinishAction") {
                state.pending_actions.push(context.variables.expand(action));
            }
        }
        Err(()) => {
            state.web_loaded = false;
            state.web_path = None;
            state.web_content = state
                .options
                .get("ErrorString")
                .map(|value| context.variables.expand(value))
                .unwrap_or_default();
            for key in ["OnConnectErrorAction", "OnDownloadErrorAction"] {
                if let Some(action) = state.options.get(key) {
                    state.pending_actions.push(context.variables.expand(action));
                }
            }
        }
    }
}

fn start_web_fetch(state: &mut MeasureState, context: &Context<'_>) {
    let Some(request) = build_web_request(state, context) else {
        state.web_loaded = false;
        state.web_path = None;
        state.web_content = state
            .options
            .get("ErrorString")
            .map(|value| context.variables.expand(value))
            .unwrap_or_default();
        for key in ["OnConnectErrorAction", "OnDownloadErrorAction"] {
            if let Some(action) = state.options.get(key) {
                state.pending_actions.push(context.variables.expand(action));
            }
        }
        return;
    };

    let shared = state.web_shared.clone();
    let generation = {
        let Ok(mut shared_state) = shared.lock() else {
            return;
        };
        if shared_state.in_flight.is_some() || shared_state.result.is_some() {
            return;
        }
        shared_state.generation = shared_state.generation.wrapping_add(1);
        let generation = shared_state.generation;
        shared_state.in_flight = Some(generation);
        shared_state.result = None;
        generation
    };

    let name = format!("webparser-{}", state.key);
    let _ = std::thread::Builder::new().name(name).spawn(move || {
        let result = execute_web_request(&request);
        if let Ok(mut shared_state) = shared.lock() {
            if shared_state.in_flight == Some(generation) {
                shared_state.in_flight = None;
                shared_state.result = Some((generation, result));
            }
        }
    });
}

fn build_web_request(state: &MeasureState, context: &Context<'_>) -> Option<WebRequest> {
    let url = state.options.get("Url")?;
    let url = resolve_in_text(&context.variables.expand(url), context);
    if url.trim().is_empty() {
        return None;
    }

    let flags = web_flags(state);
    let mut headers = Vec::new();
    if state
        .options
        .get("ForceReload")
        .map(|value| read_bool(value, false))
        .unwrap_or(false)
        || flags
            .iter()
            .any(|flag| flag == "forcereload" || flag == "pragmanocache")
    {
        headers.push(("Cache-Control".to_string(), "no-cache".to_string()));
        headers.push(("Pragma".to_string(), "no-cache".to_string()));
    }
    if let Some(user_agent) = state.options.get("UserAgent") {
        headers.push((
            "User-Agent".to_string(),
            context.variables.expand(user_agent),
        ));
    }
    for index in 1..=16 {
        let key = if index == 1 {
            "Header".to_string()
        } else {
            format!("Header{index}")
        };
        let header = state
            .options
            .get(&key)
            .or_else(|| state.options.get(&format!("Header{index}")));
        let Some(header) = header else {
            continue;
        };
        let header = context.variables.expand(header);
        if let Some((name, value)) = header.split_once(':') {
            headers.push((name.trim().to_string(), value.trim().to_string()));
        }
    }

    let proxy = state
        .options
        .get("ProxyServer")
        .map(|value| context.variables.expand(value))
        .filter(|value| !value.trim().is_empty());
    let code_page = state
        .options
        .get("CodePage")
        .map(|value| read_u32(value, 0))
        .unwrap_or(0);
    let download = (state
        .options
        .get("Download")
        .map(|value| read_bool(value, false))
        .unwrap_or(false)
        || flags.iter().any(|flag| flag == "tempfile"))
    .then(|| {
        let path = state
            .options
            .get("DownloadFile")
            .map(|value| resolve_in_text(&context.variables.expand(value), context))
            .unwrap_or_default();
        if path.is_empty() {
            state
                .root
                .join(".petrichor-webparser")
                .join(format!("{}.tmp", state.key))
        } else {
            let path = PathBuf::from(path);
            if path.is_absolute() {
                path
            } else {
                state.root.join(path)
            }
        }
    });

    Some(WebRequest {
        url,
        user_agent: None,
        headers,
        proxy,
        flags,
        code_page,
        download,
    })
}

fn execute_web_request(request: &WebRequest) -> Result<WebContent, ()> {
    let has_flag = |name: &str| request.flags.iter().any(|flag| flag == name);
    let mut builder =
        reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(10));
    if has_flag("ignorehttpredirect") || has_flag("ignorehttpsredirect") {
        builder = builder.redirect(reqwest::redirect::Policy::none());
    }
    if has_flag("secure") {
        builder = builder.https_only(true);
    }
    if has_flag("ignorecertname") || has_flag("ignorecertdate") {
        builder = builder.danger_accept_invalid_certs(true);
    }
    if let Some(proxy) = request.proxy.as_deref() {
        if let Ok(proxy) = reqwest::Proxy::all(proxy) {
            builder = builder.proxy(proxy);
        }
    }
    let client = builder.build().map_err(|_| ())?;
    let mut call = client.get(&request.url);
    for (name, value) in &request.headers {
        call = call.header(name, value);
    }
    if let Some(user_agent) = &request.user_agent {
        call = call.header(reqwest::header::USER_AGENT, user_agent);
    }

    let response = call.send().map_err(|_| ())?;
    if !response.status().is_success() {
        return Err(());
    }
    let bytes = response.bytes().map_err(|_| ())?;
    let text = decode_web_bytes(&bytes, request.code_page);

    if let Some(path) = &request.download {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| ())?;
        }
        std::fs::write(path, &bytes).map_err(|_| ())?;
        return Ok(WebContent {
            text,
            path: Some(path.display().to_string()),
        });
    }

    Ok(WebContent { text, path: None })
}

fn decode_web_bytes(bytes: &[u8], code_page: u32) -> String {
    match code_page {
        1200 => {
            let (pairs, _) = bytes.as_chunks::<2>();
            let units: Vec<u16> = pairs.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
            String::from_utf16_lossy(&units)
        }
        1201 => {
            let (pairs, _) = bytes.as_chunks::<2>();
            let units: Vec<u16> = pairs.iter().map(|pair| u16::from_be_bytes(*pair)).collect();
            String::from_utf16_lossy(&units)
        }
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

fn apply_web_regex(content: &str, pattern: &str, index: usize) -> Option<String> {
    let regex = fancy_regex::Regex::new(pattern).ok()?;
    let captures = regex.captures(content).ok()??;
    captures
        .get(index)
        .map(|matched| matched.as_str().to_string())
}

fn decode_code_points(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        let Some(kind) = chars.peek().copied() else {
            output.push(character);
            break;
        };
        let digits = match kind {
            'u' | 'U' => 4,
            'x' | 'X' => 2,
            _ => {
                output.push(character);
                continue;
            }
        };
        chars.next();
        let mut value = String::new();
        for _ in 0..digits {
            match chars.peek().copied() {
                Some(next) if next.is_ascii_hexdigit() => {
                    value.push(next);
                    chars.next();
                }
                _ => break,
            }
        }
        if value.len() == digits {
            if let Ok(code) = u32::from_str_radix(&value, 16) {
                if let Some(decoded) = char::from_u32(code) {
                    output.push(decoded);
                    continue;
                }
            }
        }
        output.push(character);
        output.push(kind);
        output.push_str(&value);
    }
    output
}

fn decode_character_references(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        output.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if let Some(end) = after.find(';').filter(|end| *end <= 10) {
            let name = after[..end].strip_prefix('#').unwrap_or(&after[..end]);
            if let Some(decoded) = crate::variables::decode_entity(name) {
                output.push(decoded);
                rest = &after[end + 1..];
                continue;
            }
        }
        output.push('&');
        rest = after;
    }
    output.push_str(rest);
    output
}

fn registry_measure_value(
    state: &MeasureState,
    context: &Context<'_>,
    backend: &dyn SystemBackend,
) -> MeasureValue {
    let option = |key: &str, default: &str| {
        state
            .options
            .get(key)
            .map(|value| context.variables.expand(value))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| default.to_string())
    };
    let hive = option("HKey", "HKEY_CURRENT_USER");
    let key = option("KeyName", "");
    let value = option("ValueName", "");
    let path = format!(
        "{}\\{}",
        hive.trim_end_matches('\\'),
        format!("{key}\\{value}").trim_matches('\\')
    );
    let text = backend
        .registry_value(&path)
        .or_else(|| {
            state
                .options
                .get("DefaultValue")
                .map(|v| context.variables.expand(v))
        })
        .unwrap_or_default();
    let trimmed = text.trim();
    let number = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .and_then(|hex| i64::from_str_radix(hex, 16).ok())
        .map(|value| value as f64)
        .or_else(|| trimmed.parse().ok())
        .unwrap_or(0.0);
    MeasureValue {
        number,
        string: Some(text),
        min: None,
        max: None,
    }
}

fn cumulative_enabled(state: &MeasureState) -> bool {
    state
        .options
        .get("Cumulative")
        .map(|value| read_bool(value, false))
        .unwrap_or(false)
}

fn stats_path(skin: &Skin) -> Option<PathBuf> {
    let base = std::env::var_os("PETRICHOR_STATE_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_STATE_HOME").map(PathBuf::from))
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
        })?;
    let name = format!("{}-{}", skin.folder, skin.name)
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    Some(base.join("petrichor").join(format!("{name}.json")))
}

fn read_bool_option(state: &MeasureState, key: &str) -> bool {
    state
        .options
        .get(key)
        .map(|value| read_bool(value, false))
        .unwrap_or(false)
}

fn expand_option(state: &MeasureState, context: &Context<'_>, key: &str) -> Option<String> {
    state
        .options
        .get(key)
        .map(|value| context.variables.expand(value))
}

fn indexed_options(state: &MeasureState, prefix: &str) -> Vec<(u32, String)> {
    let prefix = prefix.to_ascii_lowercase();
    let mut values: Vec<(u32, String)> = state
        .options
        .entries
        .iter()
        .filter_map(|entry| {
            let key = entry.key.to_ascii_lowercase();
            let rest = key.strip_prefix(&prefix)?;
            let index = if rest.is_empty() {
                1
            } else {
                rest.parse::<u32>().ok()?
            };
            Some((index, entry.value.clone()))
        })
        .collect();
    values.sort_by_key(|(index, _)| *index);
    values
}

fn option_with_index<'a>(state: &'a MeasureState, prefix: &str, index: u32) -> Option<&'a str> {
    if index <= 1 {
        return state.options.get(prefix);
    }
    state.options.get(&format!("{prefix}{index}"))
}

fn apply_average(state: &mut MeasureState, value: &mut MeasureValue) {
    if state.average_size == 0 {
        return;
    }
    state.average_buffer.push(value.number);
    while state.average_buffer.len() > state.average_size {
        state.average_buffer.remove(0);
    }
    let sum: f64 = state.average_buffer.iter().sum();
    value.number = sum / state.average_buffer.len() as f64;
    value.string = None;
}

fn disk_value(
    state: &MeasureState,
    context: &Context<'_>,
    backend: &dyn SystemBackend,
) -> MeasureValue {
    let drive = expand_option(state, context, "Drive").unwrap_or_else(|| "/".to_string());
    let quota = state
        .options
        .get("DiskQuota")
        .map(|value| read_bool(value, true))
        .unwrap_or(true);
    let Some(info) = backend.disk_space(&drive, quota) else {
        return empty_value();
    };
    let ignore_removable = state
        .options
        .get("IgnoreRemovable")
        .map(|value| read_bool(value, true))
        .unwrap_or(true);
    if ignore_removable && info.removable {
        return empty_value();
    }
    if read_bool_option(state, "Type") {
        return MeasureValue {
            number: 0.0,
            string: Some(info.volume_type.clone()),
            min: None,
            max: None,
        };
    }
    if read_bool_option(state, "Label") {
        return MeasureValue {
            number: 0.0,
            string: Some(info.label.clone()),
            min: None,
            max: None,
        };
    }

    let total = read_bool_option(state, "Total");
    let number = if total {
        info.total_bytes as f64
    } else {
        info.available_bytes as f64
    };
    MeasureValue {
        number,
        string: Some(format_display(number)),
        min: Some(0.0),
        max: Some(info.total_bytes as f64),
    }
}

enum NetKind {
    In,
    Out,
    Total,
}

fn net_value(
    state: &mut MeasureState,
    context: &Context<'_>,
    backend: &dyn SystemBackend,
    kind: NetKind,
    now: DateTime<Local>,
) -> MeasureValue {
    let interface =
        expand_option(state, context, "Interface").unwrap_or_else(|| "BEST".to_string());
    let Some(sample) = backend.net_sample(&interface) else {
        return empty_value();
    };

    let (current, previous) = match &state.net_sample {
        Some(previous) if previous.interface == sample.interface => (
            match kind {
                NetKind::In => sample.rx_bytes,
                NetKind::Out => sample.tx_bytes,
                NetKind::Total => sample.rx_bytes.saturating_add(sample.tx_bytes),
            },
            Some(match kind {
                NetKind::In => previous.rx_bytes,
                NetKind::Out => previous.tx_bytes,
                NetKind::Total => previous.rx_bytes.saturating_add(previous.tx_bytes),
            }),
        ),
        _ => (
            match kind {
                NetKind::In => sample.rx_bytes,
                NetKind::Out => sample.tx_bytes,
                NetKind::Total => sample.rx_bytes.saturating_add(sample.tx_bytes),
            },
            None,
        ),
    };

    let interval = state
        .net_last_time_ms
        .map(|last| ((now.timestamp_millis() - last) as f64 / 1000.0).max(0.001))
        .unwrap_or(0.0);
    let delta = previous
        .map(|previous| current.saturating_sub(previous) as f64)
        .unwrap_or(0.0);
    let bits = state
        .options
        .get("UseBits")
        .map(|value| read_bool(value, false))
        .unwrap_or(false);
    let bits_factor = if bits { 8.0 } else { 1.0 };

    let cumulative = state
        .options
        .get("Cumulative")
        .map(|value| read_bool(value, false))
        .unwrap_or(false);
    let number = if cumulative {
        if previous.is_some() {
            state.net_accumulator += delta;
        } else if !state.net_loaded {
            state.net_accumulator = current as f64;
        }
        state.net_accumulator * bits_factor
    } else if interval > 0.0 {
        delta / interval * bits_factor
    } else {
        0.0
    };

    state.net_sample = Some(sample);
    state.net_loaded = false;
    state.net_last_time_ms = Some(now.timestamp_millis());

    let mut max = state
        .options
        .get("MaxValue")
        .map(|value| read_f64(value, 0.0))
        .filter(|value| *value > 0.0);
    if max.is_none() {
        let speed_key = match kind {
            NetKind::In => "NetInSpeed",
            NetKind::Out => "NetOutSpeed",
            NetKind::Total => "NetTotalSpeed",
        };
        max = state
            .options
            .get(speed_key)
            .map(|value| read_f64(value, 0.0) * bits_factor)
            .filter(|value| *value > 0.0);
    }
    if max.is_none() {
        state.auto_max_samples.push(number);
        while state.auto_max_samples.len() > 3 {
            state.auto_max_samples.remove(0);
        }
        let mut samples = state.auto_max_samples.clone();
        samples.sort_by(|a, b| a.total_cmp(b));
        max = Some(samples[samples.len() / 2]);
    }

    MeasureValue {
        number,
        string: Some(format_display(number)),
        min: Some(0.0),
        max,
    }
}

fn process_value(
    state: &MeasureState,
    context: &Context<'_>,
    backend: &dyn SystemBackend,
) -> MeasureValue {
    let requested = expand_option(state, context, "ProcessName")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let running = !requested.is_empty()
        && backend
            .process_names()
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&requested));
    let number = if running { 1.0 } else { -1.0 };
    MeasureValue {
        number,
        string: Some(number.to_string()),
        min: Some(-1.0),
        max: Some(1.0),
    }
}

fn recycle_value(
    state: &MeasureState,
    context: &Context<'_>,
    backend: &dyn SystemBackend,
) -> MeasureValue {
    let info = backend.trash_info();
    let kind = expand_option(state, context, "RecycleType")
        .unwrap_or_else(|| "COUNT".to_string())
        .to_ascii_uppercase();
    let number = if kind == "SIZE" {
        info.size_bytes as f64
    } else {
        info.count as f64
    };
    MeasureValue {
        number,
        string: Some(format_display(number)),
        min: Some(0.0),
        max: None,
    }
}

fn sysinfo_value(
    state: &MeasureState,
    context: &Context<'_>,
    backend: &dyn SystemBackend,
) -> MeasureValue {
    let kind = expand_option(state, context, "SysInfoType").unwrap_or_default();
    let data = expand_option(state, context, "SysInfoData").unwrap_or_default();
    let Some(info) = backend.sysinfo(&kind, &data) else {
        return empty_value();
    };
    let number = if info.text.is_empty() {
        info.number
    } else {
        info.text.parse().unwrap_or(info.number)
    };
    MeasureValue {
        number,
        string: Some(if info.text.is_empty() {
            format_display(info.number)
        } else {
            info.text
        }),
        min: None,
        max: None,
    }
}

fn wifi_value(
    state: &MeasureState,
    context: &Context<'_>,
    backend: &dyn SystemBackend,
) -> MeasureValue {
    let interface = state
        .options
        .get("WifiIntfID")
        .map(|value| read_u32(value, 0) as usize)
        .unwrap_or(0);
    let info = backend.wifi_info_for(interface);
    let kind = expand_option(state, context, "WifiInfoType")
        .unwrap_or_else(|| "SSID".to_string())
        .to_ascii_uppercase();
    let limit = state
        .options
        .get("WifiListLimit")
        .map(|value| read_u32(value, 5).max(1) as usize)
        .unwrap_or(5);
    match kind.as_str() {
        "QUALITY" => MeasureValue {
            number: info.quality,
            string: Some(format_display(info.quality)),
            min: Some(0.0),
            max: Some(100.0),
        },
        "TXRATE" => MeasureValue {
            number: info.tx_rate_kbps,
            string: Some(format_display(info.tx_rate_kbps)),
            min: Some(0.0),
            max: None,
        },
        "RXRATE" => MeasureValue {
            number: info.rx_rate_kbps,
            string: Some(format_display(info.rx_rate_kbps)),
            min: Some(0.0),
            max: None,
        },
        "ENCRYPTION" => MeasureValue::text(info.encryption),
        "AUTH" => MeasureValue::text(info.auth),
        "PHY" => MeasureValue::text(info.phy),
        "LIST" => {
            let style = state
                .options
                .get("WifiListStyle")
                .map(|value| read_u32(value, 0))
                .unwrap_or(0);
            let mut seen = Vec::new();
            let mut lines = Vec::new();
            for network in &info.networks {
                if lines.len() >= limit || seen.contains(&network.ssid) {
                    continue;
                }
                seen.push(network.ssid.clone());
                let mut line = network.ssid.clone();
                if style & 1 != 0 && !info.phy.is_empty() {
                    line.push_str(&format!(" @{}", info.phy));
                }
                if style & 2 != 0 && !network.security.is_empty() {
                    line.push_str(&format!(" ({})", network.security));
                }
                if style & 4 != 0 {
                    line.push_str(&format!(" [{:.0}]", network.quality));
                }
                lines.push(line);
            }
            MeasureValue::text(lines.join("\n"))
        }
        _ => MeasureValue::text(info.ssid),
    }
}

fn if_actions(
    state: &mut MeasureState,
    context: &Context<'_>,
    value: &MeasureValue,
    text: &str,
) -> Vec<String> {
    let mut actions = Vec::new();
    let mut triggers: Vec<(String, bool, String, String, bool)> = Vec::new();

    for (key, threshold) in [
        ("above", "IfAboveValue"),
        ("below", "IfBelowValue"),
        ("equal", "IfEqualValue"),
    ] {
        let Some(raw) = state.options.get(threshold) else {
            continue;
        };
        let threshold = context.variables.expand(raw).parse::<f64>().unwrap_or(0.0);
        let matched = match key {
            "above" => value.number > threshold,
            "below" => value.number < threshold,
            _ => (value.number as i64) == (threshold as i64),
        };
        let action_key = match key {
            "above" => "IfAboveAction",
            "below" => "IfBelowAction",
            _ => "IfEqualAction",
        };
        let action = state
            .options
            .get(action_key)
            .unwrap_or_default()
            .to_string();
        triggers.push((key.to_string(), matched, action, String::new(), false));
    }

    let condition_mode = read_bool_option(state, "IfConditionMode");
    for (index, expression) in indexed_options(state, "IfCondition") {
        let expression = context.variables.expand(&expression);
        let matched = formula::evaluate_with(&expression, context)
            .map(|number| number != 0.0)
            .unwrap_or(false);
        let true_action = option_with_index(state, "IfTrueAction", index)
            .map(|action| context.variables.expand(action))
            .unwrap_or_default();
        let false_action = option_with_index(state, "IfFalseAction", index)
            .map(|action| context.variables.expand(action))
            .unwrap_or_default();
        triggers.push((
            format!("condition:{index}"),
            matched,
            true_action,
            false_action,
            condition_mode,
        ));
    }

    let match_mode = read_bool_option(state, "IfMatchMode");
    for (index, pattern) in indexed_options(state, "IfMatch") {
        let pattern = context.variables.expand(&pattern);
        let matched = fancy_regex::Regex::new(&pattern)
            .map(|regex| regex.is_match(text).unwrap_or(false))
            .unwrap_or(false);
        let true_action = option_with_index(state, "IfMatchAction", index)
            .map(|action| context.variables.expand(action))
            .unwrap_or_default();
        let false_action = option_with_index(state, "IfNotMatchAction", index)
            .map(|action| context.variables.expand(action))
            .unwrap_or_default();
        triggers.push((
            format!("match:{index}"),
            matched,
            true_action,
            false_action,
            match_mode,
        ));
    }

    for (key, matched, true_action, false_action, mode) in triggers {
        let previous = state.if_states.get(&key).copied().unwrap_or(false);
        if matched && (!previous || mode) && !true_action.is_empty() {
            actions.push(true_action);
        }
        if !matched && (previous || mode) && !false_action.is_empty() {
            actions.push(false_action);
        }
        state.if_states.insert(key, matched);
    }

    actions
}

fn string_value(state: &MeasureState, context: &Context<'_>) -> MeasureValue {
    let text = state
        .options
        .get("Text")
        .map(|value| {
            let expanded = context.variables.expand(value);
            resolve_in_text(&expanded, context)
        })
        .unwrap_or_default();
    let number = text.parse().unwrap_or(0.0);
    MeasureValue {
        number,
        string: Some(text),
        min: None,
        max: None,
    }
}

fn calc_value(state: &MeasureState, context: &Context<'_>) -> MeasureValue {
    let formula = state
        .options
        .get("Formula")
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let number = match formula {
        None => context.counter,
        Some(value) if value.eq_ignore_ascii_case("counter") => context.counter,
        Some(value) => {
            let expanded = context.variables.expand(value);
            formula::evaluate_with(&expanded, context).unwrap_or(0.0)
        }
    };

    MeasureValue {
        number,
        string: Some(format_display(number)),
        min: None,
        max: None,
    }
}

fn loop_value(state: &mut MeasureState) -> MeasureValue {
    let start = read_f64(state.options.get("Start").unwrap_or("0"), 0.0);
    let end = read_f64(state.options.get("End").unwrap_or("0"), 0.0);
    let step = read_f64(state.options.get("Step").unwrap_or("1"), 1.0);
    let reset = state
        .options
        .get("Reset")
        .map(|value| read_bool(value, false))
        .unwrap_or(false);

    if reset || !state.loop_initialized {
        state.loop_value = start;
        state.loop_initialized = true;
    } else if (step > 0.0 && state.loop_value + step <= end)
        || (step < 0.0 && state.loop_value + step >= end)
    {
        state.loop_value += step;
    } else if step != 0.0 {
        state.loop_value = start;
    }

    let value = state.loop_value;
    MeasureValue {
        number: value,
        string: Some(format_display(value)),
        min: Some(start),
        max: Some(end),
    }
}

fn apply_bounds(state: &MeasureState, value: &mut MeasureValue) {
    if let Some(min) = state.options.get("MinValue") {
        value.min = Some(read_f64(min, value.min.unwrap_or(0.0)));
    }
    if let Some(max) = state.options.get("MaxValue") {
        let max = read_f64(max, value.max.unwrap_or(0.0));
        if !state.kind.starts_with("NET") || max > 0.0 {
            value.max = Some(max);
        }
    }

    if !state.invert {
        return;
    }
    if let (Some(min), Some(max)) = (value.min, value.max) {
        value.number = min + max - value.number;
        value.string = Some(format_display(value.number));
    }
}

struct Context<'a> {
    values: &'a BTreeMap<String, MeasureValue>,
    variables: &'a Variables,
    options: &'a Options,
    runtime: Option<&'a RuntimeSlot>,
    runtimes: &'a BTreeMap<String, RuntimeSlot>,
    backend: &'a dyn SystemBackend,
    skin_name: &'a str,
    skin_root: &'a std::path::Path,
    skin_path: &'a std::path::Path,
    measure_name: &'a str,
    meters: &'a [Meter],
    sections: &'a BTreeMap<String, BTreeMap<String, String>>,
    skin_window_handle: u64,
    counter: f64,
    rng: &'a Cell<u64>,
}

impl EvalContext for Context<'_> {
    fn resolve(&self, name: &str) -> Option<f64> {
        if name.eq_ignore_ascii_case("counter") {
            return Some(self.counter);
        }
        if let Some(value) = self.values.get(&name.to_ascii_lowercase()) {
            return Some(value.number);
        }
        self.variables
            .get(name)
            .and_then(|value| value.parse().ok())
    }

    fn random(&self) -> f64 {
        next_random(self.rng)
    }
}

impl MeasureSource for Context<'_> {
    fn measure(&self, name: &str) -> Option<MeasureValue> {
        if name.eq_ignore_ascii_case("counter") {
            return Some(MeasureValue::number(self.counter));
        }
        self.values.get(&name.to_ascii_lowercase()).cloned()
    }

    fn call(&self, measure: &str, function: &str) -> Option<String> {
        let runtime = self
            .runtimes
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(measure))
            .map(|(_, runtime)| runtime)
            .or(self.runtime)?;
        let context = RuntimeContext {
            skin_name: self.skin_name,
            skin_root: self.skin_root,
            skin_path: self.skin_path,
            measure_name: self.measure_name,
            options: self.options,
            variables: self.variables,
            backend: self.backend,
            values: self.values,
            meters: self.meters,
            sections: self.sections,
            skin_window_handle: self.skin_window_handle,
            counter: self.counter,
        };
        runtime.call(function, &context)
    }
}

#[derive(Debug, Clone, Default)]
struct Substitute {
    rules: Vec<SubRule>,
}

#[derive(Debug, Clone)]
enum SubRule {
    Literal(String, String),
    Regex(regex::Regex, String),
}

impl Substitute {
    fn from_options(options: &Options) -> Substitute {
        let Some(raw) = options.get("Substitute") else {
            return Substitute::default();
        };
        let regex_mode = options
            .get("RegExpSubstitute")
            .map(|value| read_bool(value, false))
            .unwrap_or(false);
        Substitute::parse(raw, regex_mode)
    }

    fn parse(raw: &str, regex_mode: bool) -> Substitute {
        let mut rules = Vec::new();
        for (from, to) in parse_pairs(raw) {
            if from.is_empty() {
                continue;
            }
            if regex_mode {
                match regex::Regex::new(&from) {
                    Ok(pattern) => rules.push(SubRule::Regex(pattern, to)),
                    Err(_) => rules.push(SubRule::Literal(from, to)),
                }
            } else {
                rules.push(SubRule::Literal(from, to));
            }
        }
        Substitute { rules }
    }

    fn apply(&self, input: &str) -> String {
        let mut output = input.to_string();
        for rule in &self.rules {
            output = match rule {
                SubRule::Literal(from, to) => output.replace(from.as_str(), to),
                SubRule::Regex(pattern, to) => {
                    pattern.replace_all(&output, to.as_str()).to_string()
                }
            };
        }
        output
    }
}

fn parse_pairs(raw: &str) -> Vec<(String, String)> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = raw.chars();

    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                if !in_quotes {
                    segments.push(std::mem::take(&mut current));
                }
            }
            '\\' if in_quotes => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            _ if in_quotes => current.push(ch),
            _ => {}
        }
    }

    segments
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| (pair[0].clone(), pair[1].clone()))
        .collect()
}

fn runtime_value(
    state: &mut MeasureState,
    context: &Context<'_>,
    backend: &dyn SystemBackend,
) -> MeasureValue {
    let Some(runtime) = state.runtime.clone() else {
        return empty_value();
    };
    let runtime_context = RuntimeContext {
        skin_name: &state.skin_name,
        skin_root: &state.root,
        skin_path: &state.skin_path,
        measure_name: &state.name,
        options: &state.options,
        variables: context.variables,
        backend,
        values: context.values,
        meters: &state.meters,
        sections: &state.sections,
        skin_window_handle: context.skin_window_handle,
        counter: context.counter,
    };
    let output = runtime.update(&runtime_context);
    state.pending_actions.extend(output.actions);
    output.value
}

fn now_player_request(
    state: &MeasureState,
    variables: &Variables,
    players: &Arc<Mutex<BTreeMap<String, String>>>,
) -> String {
    let raw = state.options.get("PlayerName").unwrap_or("WLM");
    let expanded = variables.expand(raw);
    let trimmed = expanded.trim();
    let reference = trimmed
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .map(str::trim)
        .unwrap_or(trimmed);
    if trimmed.starts_with('[') {
        return players
            .lock()
            .ok()
            .and_then(|players| players.get(&reference.to_ascii_lowercase()).cloned())
            .unwrap_or_else(|| "BEST".to_string());
    }
    reference.to_string()
}

fn now_playing_value(
    state: &mut MeasureState,
    context: &Context<'_>,
    backend: &dyn SystemBackend,
) -> MeasureValue {
    let request = now_player_request(state, context.variables, &state.now_players);
    let Some(info) = backend.now_playing(&request) else {
        return MeasureValue {
            number: 0.0,
            string: Some(String::new()),
            min: None,
            max: None,
        };
    };

    if let Ok(mut players) = state.now_players.lock() {
        players.insert(state.key.clone(), info.bus_name.clone());
    }

    if state.now_track.as_deref() != Some(info.track_id.as_str()) {
        state.now_track = Some(info.track_id.clone());
        if let Some(action) = state.options.get("TrackChangeAction") {
            state.pending_actions.push(context.variables.expand(action));
        }
    }

    let player_type = state
        .options
        .get("PlayerType")
        .unwrap_or_default()
        .trim()
        .to_ascii_uppercase();
    let disable_leading_zero = state
        .options
        .get("DisableLeadingZero")
        .map(|value| read_bool(value, false))
        .unwrap_or(false);

    let text_value = |text: String| MeasureValue {
        number: text.parse().unwrap_or(0.0),
        string: Some(text),
        min: None,
        max: None,
    };
    let number_value = |number: f64, min: Option<f64>, max: Option<f64>| MeasureValue {
        number,
        string: Some(format_display(number)),
        min,
        max,
    };

    match player_type.as_str() {
        "ARTIST" => text_value(info.artist),
        "TITLE" => text_value(info.title),
        "ALBUM" => text_value(info.album),
        "COVER" => text_value(
            backend
                .media_cover(&info.file, &info.art_url)
                .unwrap_or_else(|| info.art_url.clone()),
        ),
        "DURATION" => text_value(format_media_time(info.duration_secs, disable_leading_zero)),
        "POSITION" => text_value(format_media_time(info.position_secs, disable_leading_zero)),
        "PROGRESS" => number_value(info.progress(), Some(0.0), Some(100.0)),
        "RATING" => number_value(info.rating, Some(0.0), Some(5.0)),
        "STATE" => number_value(f64::from(info.state()), Some(0.0), Some(2.0)),
        "STATUS" => number_value(1.0, Some(0.0), Some(1.0)),
        "VOLUME" => number_value(info.volume, Some(0.0), Some(100.0)),
        "SHUFFLE" => number_value(if info.shuffle { 1.0 } else { 0.0 }, Some(0.0), Some(1.0)),
        "REPEAT" => number_value(
            match info.repeat.as_str() {
                "track" => 1.0,
                "playlist" => 2.0,
                _ => 0.0,
            },
            Some(0.0),
            Some(2.0),
        ),
        "LYRICS" => text_value(info.lyrics),
        "FILE" => text_value(info.file),
        "NUMBER" => number_value(info.track_number as f64, None, None),
        "YEAR" => number_value(info.year as f64, None, None),
        "GENRE" => text_value(info.genre),
        _ => empty_value(),
    }
}

fn format_media_time(seconds: f64, disable_leading_zero: bool) -> String {
    let total = seconds.max(0.0) as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let secs = total % 60;
    if hours > 0 {
        if disable_leading_zero {
            format!("{hours}:{minutes:02}:{secs:02}")
        } else {
            format!("{hours:02}:{minutes:02}:{secs:02}")
        }
    } else if disable_leading_zero {
        format!("{minutes}:{secs:02}")
    } else {
        format!("{minutes:02}:{secs:02}")
    }
}

fn empty_value() -> MeasureValue {
    MeasureValue {
        number: 0.0,
        string: Some(String::new()),
        min: None,
        max: None,
    }
}

fn display_text(value: &MeasureValue) -> String {
    match &value.string {
        Some(text) => text.clone(),
        None => format_display(value.number),
    }
}

fn format_display(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
    let rounded = (value * 100.0).round() / 100.0;
    let mut text = format!("{rounded:.2}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text == "-0" {
        text = "0".to_string();
    }
    text
}

fn seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15)
        | 1
}

fn next_random(state: &Cell<u64>) -> f64 {
    let mut value = state.get();
    if value == 0 {
        value = 0x9E37_79B9_7F4A_7C15;
    }
    value ^= value << 13;
    value ^= value >> 7;
    value ^= value << 17;
    state.set(value);
    (value >> 11) as f64 / (1u64 << 53) as f64
}

pub fn evaluate(skin: &Skin) -> BTreeMap<String, String> {
    evaluate_with(skin, &petrichor_sys::backend(), Local::now())
}

pub fn evaluate_with(
    skin: &Skin,
    backend: &dyn SystemBackend,
    now: DateTime<Local>,
) -> BTreeMap<String, String> {
    MeasureEngine::new(skin).update(backend, now).strings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{OptionEntry, SkinSettings};
    use petrichor_sys::{
        CpuSnapshot, DiskSpaceInfo, MemoryInfo, NetSample, NowPlayingInfo, OsRelease, SysInfoValue,
        TrashInfo, WifiInfo,
    };
    use std::path::PathBuf;

    struct MockBackend {
        uptime: u64,
        cpu: CpuSnapshot,
        memory: MemoryInfo,
        disk: Option<DiskSpaceInfo>,
        net: Option<NetSample>,
        processes: Vec<String>,
        trash: TrashInfo,
        wifi: WifiInfo,
        sysinfo: Option<SysInfoValue>,
        registry: Option<String>,
        media: Option<NowPlayingInfo>,
    }

    impl MockBackend {
        fn new() -> Self {
            MockBackend {
                uptime: 90_061,
                cpu: CpuSnapshot {
                    overall: CpuTimes {
                        total: 200,
                        idle: 60,
                    },
                    cores: vec![CpuTimes {
                        total: 100,
                        idle: 30,
                    }],
                },
                memory: MemoryInfo {
                    total_kb: 16_000_000,
                    available_kb: 12_000_000,
                    free_kb: 8_000_000,
                    buffers_kb: 0,
                    cached_kb: 0,
                    swap_total_kb: 2_000_000,
                    swap_free_kb: 1_500_000,
                },
                disk: None,
                net: None,
                processes: Vec::new(),
                trash: TrashInfo::default(),
                wifi: WifiInfo::default(),
                sysinfo: None,
                registry: None,
                media: None,
            }
        }
    }

    impl SystemBackend for MockBackend {
        fn hostname(&self) -> String {
            "test".to_string()
        }
        fn user_name(&self) -> String {
            "test".to_string()
        }
        fn os_release(&self) -> OsRelease {
            OsRelease {
                name: "Test".to_string(),
                version: String::new(),
                id: "test".to_string(),
            }
        }
        fn uptime_secs(&self) -> u64 {
            self.uptime
        }
        fn cpu_snapshot(&self) -> CpuSnapshot {
            self.cpu.clone()
        }
        fn memory_info(&self) -> MemoryInfo {
            self.memory
        }
        fn disk_space(&self, _drive: &str, _disk_quota: bool) -> Option<DiskSpaceInfo> {
            self.disk.clone()
        }
        fn net_sample(&self, _interface: &str) -> Option<NetSample> {
            self.net.clone()
        }
        fn process_names(&self) -> Vec<String> {
            self.processes.clone()
        }
        fn trash_info(&self) -> TrashInfo {
            self.trash.clone()
        }
        fn wifi_info(&self) -> WifiInfo {
            self.wifi.clone()
        }
        fn sysinfo(&self, _kind: &str, _data: &str) -> Option<SysInfoValue> {
            self.sysinfo.clone()
        }
        fn registry_value(&self, _path: &str) -> Option<String> {
            self.registry.clone()
        }
        fn now_playing(&self, _player: &str) -> Option<NowPlayingInfo> {
            self.media.clone()
        }
        fn media_cover(&self, _file: &str, art_url: &str) -> Option<String> {
            (!art_url.is_empty()).then(|| art_url.to_string())
        }
    }

    fn measure(kind: &str, name: &str, entries: &[(&str, &str)]) -> Measure {
        Measure {
            name: name.to_string(),
            kind: kind.to_string(),
            line: 1,
            options: Options {
                entries: entries
                    .iter()
                    .map(|(key, value)| OptionEntry {
                        key: key.to_string(),
                        value: value.to_string(),
                        line: 1,
                    })
                    .collect(),
            },
        }
    }

    fn skin(measures: Vec<Measure>) -> Skin {
        Skin {
            name: "Test".to_string(),
            folder: "Test".to_string(),
            ini_path: PathBuf::from("/tmp/Test.ini"),
            root: PathBuf::from("/tmp"),
            variables: BTreeMap::new(),
            metadata: Default::default(),
            settings: SkinSettings::default(),
            measures,
            meters: Vec::new(),
            styles: Vec::new(),
            included_files: Vec::new(),
            sections: BTreeMap::new(),
        }
    }

    fn fixed_now() -> DateTime<Local> {
        use chrono::TimeZone;
        Local
            .with_ymd_and_hms(2026, 10, 19, 13, 45, 6)
            .single()
            .unwrap()
    }

    fn update_once(engine: &mut MeasureEngine) -> UpdateReport {
        engine.update(&MockBackend::new(), fixed_now())
    }

    fn wait_for_web<F>(engine: &mut MeasureEngine, predicate: F) -> UpdateReport
    where
        F: Fn(&UpdateReport) -> bool,
    {
        for _ in 0..200 {
            let report = update_once(engine);
            if predicate(&report) {
                return report;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("WebParser : résultat non disponible dans le délai imparti");
    }

    #[test]
    fn formats_time_value() {
        let mut engine = MeasureEngine::new(&skin(vec![measure(
            "Time",
            "Heure",
            &[("Format", "%H:%M:%S")],
        )]));
        let report = update_once(&mut engine);
        assert_eq!(report.strings.get("Heure").unwrap(), "13:45:06");
    }

    #[test]
    fn invalid_time_format_falls_back_to_default() {
        let mut engine =
            MeasureEngine::new(&skin(vec![measure("Time", "Heure", &[("Format", "%Q")])]));
        let report = update_once(&mut engine);
        assert_eq!(report.strings.get("Heure").unwrap(), "13:45:06");
    }

    #[test]
    fn formats_uptime_with_default_and_format() {
        let mut engine = MeasureEngine::new(&skin(vec![
            measure("Uptime", "Up", &[]),
            measure("Uptime", "Up2", &[("Format", "%H:%M")]),
        ]));
        let report = update_once(&mut engine);
        assert_eq!(report.strings.get("Up").unwrap(), "1d 01:01:01");
        assert_eq!(report.strings.get("Up2").unwrap(), "01:01");
        assert_eq!(report.values.get("Up").unwrap().number, 90_061.0);
    }

    #[test]
    fn script_measure_updates_and_calls_functions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Test.lua");
        std::fs::write(
            &path,
            r#"
            function Update()
                SKIN:Bang('!SetVariable', 'FromLua', 'yes')
                return 7
            end
            function GetStringValue()
                return 'lua'
            end
            function Double(value)
                return value * 2
            end
            "#,
        )
        .unwrap();
        let mut skin = skin(vec![
            measure(
                "Script",
                "ScriptMeasure",
                &[("ScriptFile", path.to_str().unwrap())],
            ),
            measure(
                "String",
                "Message",
                &[("Text", "[ScriptMeasure:Double(3)]")],
            ),
        ]);
        skin.root = dir.path().to_path_buf();
        let mut engine = MeasureEngine::new(&skin);
        let report = update_once(&mut engine);
        let value = report.values.get("ScriptMeasure").unwrap();
        assert_eq!(value.number, 7.0);
        assert_eq!(value.string.as_deref(), Some("lua"));
        assert_eq!(report.strings.get("Message").unwrap(), "6");
        assert!(report
            .actions
            .iter()
            .any(|action| action.contains("FromLua")));
    }

    #[test]
    fn computes_cpu_usage_from_previous_sample() {
        let mut engine = MeasureEngine::new(&skin(vec![measure("CPU", "Cpu", &[])]));
        let first = update_once(&mut engine);
        assert_eq!(first.strings.get("Cpu").unwrap(), "0");

        let backend = MockBackend {
            cpu: CpuSnapshot {
                overall: CpuTimes {
                    total: 400,
                    idle: 80,
                },
                cores: vec![CpuTimes {
                    total: 200,
                    idle: 40,
                }],
            },
            ..MockBackend::new()
        };
        let second = engine.update(&backend, fixed_now());
        assert_eq!(second.strings.get("Cpu").unwrap(), "90");
        assert_eq!(second.values.get("Cpu").unwrap().max, Some(100.0));
    }

    #[test]
    fn computes_memory_and_physical() {
        let mut engine = MeasureEngine::new(&skin(vec![
            measure("Memory", "Mem", &[]),
            measure("PhysicalMemory", "Phys", &[]),
        ]));
        let report = update_once(&mut engine);
        assert_eq!(report.strings.get("Mem").unwrap(), "4394.53");
        assert_eq!(report.values.get("Mem").unwrap().max, Some(17578.125));
        assert_eq!(report.strings.get("Phys").unwrap(), "3906.25");
    }

    #[test]
    fn resolves_string_text_with_references() {
        let mut engine = MeasureEngine::new(&skin(vec![
            measure("Calc", "Value", &[("Formula", "21 * 2")]),
            measure("String", "Label", &[("Text", "Total : [Value]")]),
        ]));
        let report = update_once(&mut engine);
        assert_eq!(report.strings.get("Label").unwrap(), "Total : 42");
    }

    #[test]
    fn calc_references_measure_and_counter() {
        let mut engine = MeasureEngine::new(&skin(vec![
            measure("Calc", "Base", &[("Formula", "10")]),
            measure("Calc", "Sum", &[("Formula", "Base + 5")]),
            measure("Calc", "Tick", &[]),
        ]));
        let first = update_once(&mut engine);
        assert_eq!(first.strings.get("Sum").unwrap(), "15");
        assert_eq!(first.strings.get("Tick").unwrap(), "1");
        let second = update_once(&mut engine);
        assert_eq!(second.strings.get("Tick").unwrap(), "2");
    }

    #[test]
    fn loop_cycles_and_wraps() {
        let mut engine = MeasureEngine::new(&skin(vec![measure(
            "Loop",
            "Counter",
            &[("Start", "0"), ("End", "2"), ("Step", "1")],
        )]));
        let values: Vec<String> = (0..5)
            .map(|_| update_once(&mut engine).strings["Counter"].clone())
            .collect();
        assert_eq!(values, vec!["0", "1", "2", "0", "1"]);
    }

    #[test]
    fn loop_reset_option_restarts_each_update() {
        let mut engine = MeasureEngine::new(&skin(vec![measure(
            "Loop",
            "Counter",
            &[("Start", "5"), ("End", "9"), ("Step", "1"), ("Reset", "1")],
        )]));
        let values: Vec<String> = (0..2)
            .map(|_| update_once(&mut engine).strings["Counter"].clone())
            .collect();
        assert_eq!(values, vec!["5", "5"]);
    }

    #[test]
    fn respects_update_divider() {
        let mut engine = MeasureEngine::new(&skin(vec![measure(
            "Calc",
            "Tick",
            &[("UpdateDivider", "2")],
        )]));
        let first = update_once(&mut engine);
        assert_eq!(first.strings.get("Tick").unwrap(), "");
        let second = update_once(&mut engine);
        assert_eq!(second.strings.get("Tick").unwrap(), "2");
        let third = update_once(&mut engine);
        assert_eq!(third.strings.get("Tick").unwrap(), "2");
    }

    #[test]
    fn disabled_measure_is_empty() {
        let mut engine = MeasureEngine::new(&skin(vec![measure(
            "String",
            "Text",
            &[("Text", "secret"), ("Disabled", "1")],
        )]));
        let report = update_once(&mut engine);
        assert_eq!(report.strings.get("Text").unwrap(), "");
    }

    #[test]
    fn paused_measure_keeps_last_value() {
        let mut engine = MeasureEngine::new(&skin(vec![measure("Calc", "Tick", &[])]));
        assert_eq!(update_once(&mut engine).strings["Tick"], "1");
        engine.states[0].paused = true;
        assert_eq!(update_once(&mut engine).strings["Tick"], "1");
    }

    #[test]
    fn applies_substitute_literal_and_regex() {
        let mut engine = MeasureEngine::new(&skin(vec![
            measure(
                "String",
                "Literal",
                &[("Text", "a-b-c"), ("Substitute", "\"-\":\"+\"")],
            ),
            measure(
                "String",
                "Regex",
                &[
                    ("Text", "abc123"),
                    ("RegExpSubstitute", "1"),
                    ("Substitute", "\"([a-z]+)([0-9]+)\":\"$2-$1\""),
                ],
            ),
        ]));
        let report = update_once(&mut engine);
        assert_eq!(report.strings.get("Literal").unwrap(), "a+b+c");
        assert_eq!(report.strings.get("Regex").unwrap(), "123-abc");
    }

    #[test]
    fn emits_on_update_and_on_change_actions() {
        let mut engine = MeasureEngine::new(&skin(vec![measure(
            "Calc",
            "Tick",
            &[("OnUpdateAction", "!Update"), ("OnChangeAction", "!Redraw")],
        )]));
        let first = update_once(&mut engine);
        assert_eq!(
            first.actions,
            vec!["!Update".to_string(), "!Redraw".to_string()]
        );
        let second = update_once(&mut engine);
        assert!(second.actions.contains(&"!Update".to_string()));
        assert!(second.actions.contains(&"!Redraw".to_string()));
    }

    #[test]
    fn inverts_measure_value() {
        let mut engine = MeasureEngine::new(&skin(vec![measure(
            "Calc",
            "Free",
            &[
                ("Formula", "25"),
                ("MinValue", "0"),
                ("MaxValue", "100"),
                ("InvertMeasure", "1"),
            ],
        )]));
        let report = update_once(&mut engine);
        assert_eq!(report.strings.get("Free").unwrap(), "75");
    }

    #[test]
    fn reports_free_disk_space_variants() {
        let disk = DiskSpaceInfo {
            total_bytes: 1_000,
            free_bytes: 400,
            available_bytes: 300,
            fs_type: "ext4".to_string(),
            volume_type: "Fixed".to_string(),
            label: "Root".to_string(),
            removable: false,
        };
        let backend = MockBackend {
            disk: Some(disk),
            ..MockBackend::new()
        };
        let mut engine = MeasureEngine::new(&skin(vec![
            measure("FreeDiskSpace", "Free", &[]),
            measure("FreeDiskSpace", "Total", &[("Total", "1")]),
            measure("FreeDiskSpace", "Type", &[("Type", "1")]),
            measure("FreeDiskSpace", "Label", &[("Label", "1")]),
        ]));
        let report = engine.update(&backend, fixed_now());
        assert_eq!(report.strings["Free"], "300");
        assert_eq!(report.strings["Total"], "1000");
        assert_eq!(report.strings["Type"], "Fixed");
        assert_eq!(report.strings["Label"], "Root");
        assert_eq!(report.values["Free"].max, Some(1_000.0));
    }

    #[test]
    fn computes_swap_memory_in_megabytes() {
        let mut engine = MeasureEngine::new(&skin(vec![measure("SwapMemory", "Swap", &[])]));
        let report = update_once(&mut engine);
        assert_eq!(report.strings["Swap"], "488.28");
        assert_eq!(report.values["Swap"].max, Some(1953.125));
    }

    #[test]
    fn computes_network_rate_and_bits() {
        let first = NetSample {
            interface: "eth0".to_string(),
            rx_bytes: 1_000,
            tx_bytes: 2_000,
        };
        let second = NetSample {
            interface: "eth0".to_string(),
            rx_bytes: 1_100,
            tx_bytes: 2_400,
        };
        let mut engine = MeasureEngine::new(&skin(vec![
            measure("NetIn", "In", &[]),
            measure("NetOut", "Out", &[("UseBits", "1")]),
        ]));
        let backend = MockBackend {
            net: Some(first),
            ..MockBackend::new()
        };
        assert_eq!(engine.update(&backend, fixed_now()).strings["In"], "0");

        let later = fixed_now() + chrono::Duration::seconds(1);
        let backend = MockBackend {
            net: Some(second),
            ..MockBackend::new()
        };
        let report = engine.update(&backend, later);
        assert_eq!(report.strings["In"], "100");
        assert_eq!(report.strings["Out"], "3200");
    }

    #[test]
    fn computes_cumulative_network_totals() {
        let mut engine = MeasureEngine::new(&skin(vec![measure(
            "NetTotal",
            "Total",
            &[("Cumulative", "1")],
        )]));
        let backend = MockBackend {
            net: Some(NetSample {
                interface: "eth0".to_string(),
                rx_bytes: 1_000,
                tx_bytes: 2_000,
            }),
            ..MockBackend::new()
        };
        let report = engine.update(&backend, fixed_now());
        assert_eq!(report.strings["Total"], "3000");
    }

    #[test]
    fn detects_process_and_trash() {
        let backend = MockBackend {
            processes: vec!["TestProc".to_string()],
            trash: TrashInfo {
                count: 3,
                size_bytes: 4_096,
            },
            ..MockBackend::new()
        };
        let mut engine = MeasureEngine::new(&skin(vec![
            measure("Process", "Running", &[("ProcessName", "testproc")]),
            measure("Process", "Stopped", &[("ProcessName", "absent")]),
            measure("RecycleManager", "Count", &[]),
            measure("RecycleManager", "Size", &[("RecycleType", "SIZE")]),
        ]));
        let report = engine.update(&backend, fixed_now());
        assert_eq!(report.strings["Running"], "1");
        assert_eq!(report.strings["Stopped"], "-1");
        assert_eq!(report.strings["Count"], "3");
        assert_eq!(report.strings["Size"], "4096");
    }

    #[test]
    fn resolves_sysinfo_and_wifi() {
        let backend = MockBackend {
            sysinfo: Some(SysInfoValue::text("example.org")),
            wifi: WifiInfo {
                connected: true,
                ssid: "Home".to_string(),
                quality: 72.0,
                networks: vec![
                    petrichor_sys::WifiNetwork {
                        ssid: "Home".to_string(),
                        quality: 72.0,
                        signal_dbm: -50,
                        security: "WPA2".to_string(),
                    },
                    petrichor_sys::WifiNetwork {
                        ssid: "Neighbor".to_string(),
                        quality: 40.0,
                        signal_dbm: -72,
                        security: "WPA1".to_string(),
                    },
                ],
                ..WifiInfo::default()
            },
            ..MockBackend::new()
        };
        let mut engine = MeasureEngine::new(&skin(vec![
            measure("SysInfo", "Domain", &[("SysInfoType", "DOMAIN_NAME")]),
            measure("WifiStatus", "Ssid", &[]),
            measure("WifiStatus", "Quality", &[("WifiInfoType", "QUALITY")]),
            measure(
                "WifiStatus",
                "List",
                &[
                    ("WifiInfoType", "LIST"),
                    ("WifiListStyle", "4"),
                    ("WifiListLimit", "1"),
                ],
            ),
        ]));
        let report = engine.update(&backend, fixed_now());
        assert_eq!(report.strings["Domain"], "example.org");
        assert_eq!(report.strings["Ssid"], "Home");
        assert_eq!(report.strings["Quality"], "72");
        assert_eq!(report.strings["List"], "Home [72]");
    }

    #[test]
    fn applies_average_size() {
        let mut engine =
            MeasureEngine::new(&skin(vec![measure("Calc", "Avg", &[("AverageSize", "2")])]));
        let values: Vec<String> = (0..3)
            .map(|_| update_once(&mut engine).strings["Avg"].clone())
            .collect();
        assert_eq!(values, vec!["1", "1.5", "2.5"]);
    }

    #[test]
    fn triggers_if_actions_on_transitions() {
        let mut engine = MeasureEngine::new(&skin(vec![measure(
            "Calc",
            "Tick",
            &[
                ("IfAboveValue", "0"),
                ("IfAboveAction", "above"),
                ("IfCondition", "Tick >= 2"),
                ("IfTrueAction", "condition"),
            ],
        )]));
        let first = update_once(&mut engine);
        assert!(first.actions.contains(&"above".to_string()));
        assert!(!first.actions.contains(&"condition".to_string()));
        let second = update_once(&mut engine);
        assert!(!second.actions.contains(&"above".to_string()));
        assert!(second.actions.contains(&"condition".to_string()));
        let third = update_once(&mut engine);
        assert!(!third.actions.contains(&"condition".to_string()));
    }

    #[test]
    fn triggers_if_match_each_cycle_in_mode() {
        let mut engine = MeasureEngine::new(&skin(vec![measure(
            "Calc",
            "Tick",
            &[
                ("IfMatch", "2"),
                ("IfMatchAction", "match"),
                ("IfMatchMode", "1"),
            ],
        )]));
        let first = update_once(&mut engine);
        assert!(!first.actions.contains(&"match".to_string()));
        let second = update_once(&mut engine);
        assert!(second.actions.contains(&"match".to_string()));
        let third = update_once(&mut engine);
        assert!(!third.actions.contains(&"match".to_string()));
    }

    #[test]
    fn reads_virtual_registry_value() {
        let mut backend = MockBackend::new();
        backend.registry = Some("0x10".to_string());
        let mut engine = MeasureEngine::new(&skin(vec![measure(
            "Registry",
            "Reg",
            &[
                ("HKey", "HKCU"),
                ("KeyName", "Software\\Petrichor"),
                ("ValueName", "Theme"),
            ],
        )]));
        let report = engine.update(&backend, fixed_now());
        assert_eq!(report.strings.get("Reg").unwrap(), "0x10");
        assert_eq!(report.values.get("Reg").unwrap().number, 16.0);
    }

    #[test]
    fn applies_web_regex_and_character_references() {
        assert_eq!(
            apply_web_regex("<b>42</b>", r"<b>(\d+)</b>", 1).unwrap(),
            "42"
        );
        assert_eq!(
            apply_web_regex("<b>aac</b>", r"<b>(\w)\1c</b>", 1).unwrap(),
            "a"
        );
        assert_eq!(apply_web_regex("foo bar", r"foo(?!baz)", 0).unwrap(), "foo");
        assert_eq!(
            decode_character_references("Fish &amp; Chips &#65;"),
            "Fish & Chips A"
        );
        assert_eq!(decode_code_points(r"\u0041\x42"), "AB");
        assert_eq!(decode_code_points(r"\uZZZZ"), r"\uZZZZ");
    }

    #[test]
    fn webparser_fetches_without_blocking_the_update() {
        use std::io::{Read, Write};
        use std::time::{Duration, Instant};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 2048];
            let _ = stream.read(&mut buffer);
            std::thread::sleep(Duration::from_millis(400));
            let body = "<html>slow</html>";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let url = format!("http://127.0.0.1:{port}/");
        let mut engine = MeasureEngine::new(&skin(vec![measure(
            "WebParser",
            "Web",
            &[
                ("Url", url.as_str()),
                ("RegExp", "<html>(.*?)</html>"),
                ("StringIndex", "1"),
            ],
        )]));
        let started = Instant::now();
        let _ = update_once(&mut engine);
        assert!(
            started.elapsed() < Duration::from_millis(200),
            "le rendu a été bloqué par le téléchargement"
        );
        let report = wait_for_web(&mut engine, |report| {
            report
                .strings
                .get("Web")
                .is_some_and(|value| value == "slow")
        });
        server.join().unwrap();
        assert_eq!(report.strings.get("Web").unwrap(), "slow");
    }

    #[test]
    fn webparser_expands_variable_urls_before_chaining() {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 2048];
            let _ = stream.read(&mut buffer);
            let body = "<title>Variable</title>";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let mut engine = MeasureEngine::new(&skin(vec![measure(
            "WebParser",
            "Web",
            &[
                ("Url", "[#Feed]"),
                ("RegExp", "<title>(.*?)</title>"),
                ("StringIndex", "1"),
            ],
        )]));
        engine.set_variable("Feed", &format!("http://127.0.0.1:{port}/"));
        let report = wait_for_web(&mut engine, |report| {
            report
                .strings
                .get("Web")
                .is_some_and(|value| value == "Variable")
        });
        server.join().unwrap();
        assert_eq!(report.strings.get("Web").unwrap(), "Variable");
    }

    #[test]
    fn webparser_chains_child_measure_without_second_fetch() {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 2048];
            let _ = stream.read(&mut buffer);
            let body = "<item><sub>Hello</sub></item>";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let url = format!("http://127.0.0.1:{port}/");
        let mut engine = MeasureEngine::new(&skin(vec![
            measure(
                "WebParser",
                "Parent",
                &[
                    ("Url", url.as_str()),
                    ("RegExp", "<item>(.*?)</item>"),
                    ("StringIndex", "1"),
                ],
            ),
            measure(
                "WebParser",
                "Child",
                &[
                    ("Url", "[Parent]"),
                    ("RegExp", "<sub>(.*?)</sub>"),
                    ("StringIndex2", "1"),
                ],
            ),
        ]));
        let report = wait_for_web(&mut engine, |report| {
            report
                .strings
                .get("Child")
                .is_some_and(|value| value == "Hello")
        });
        server.join().unwrap();
        assert_eq!(report.strings.get("Parent").unwrap(), "<sub>Hello</sub>");
        assert_eq!(report.strings.get("Child").unwrap(), "Hello");
    }

    #[test]
    fn webparser_honors_redirect_and_cache_flags() {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (sender, receiver) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 2048];
            let read = stream.read(&mut buffer).unwrap_or(0);
            let request = String::from_utf8_lossy(&buffer[..read]).to_ascii_lowercase();
            let _ = sender.send(request);
            let response = "HTTP/1.1 302 Found\r\nLocation: /elsewhere\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            stream.write_all(response.as_bytes()).unwrap();
        });

        let url = format!("http://127.0.0.1:{port}/");
        let mut engine = MeasureEngine::new(&skin(vec![measure(
            "WebParser",
            "Web",
            &[
                ("Url", url.as_str()),
                ("Flags", "IgnoreHTTPRedirect,PragmaNoCache"),
                ("ErrorString", "redirect-blocked"),
            ],
        )]));
        let report = wait_for_web(&mut engine, |report| {
            report
                .strings
                .get("Web")
                .is_some_and(|value| value == "redirect-blocked")
        });
        server.join().unwrap();
        assert_eq!(report.strings.get("Web").unwrap(), "redirect-blocked");
        let request = receiver.recv().unwrap();
        assert!(request.contains("pragma: no-cache"), "{request}");
    }

    #[test]
    fn webparser_downloads_and_dumps_content() {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 2048];
            let _ = stream.read(&mut buffer);
            let body = "<html>payload</html>";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let directory = tempfile::tempdir().unwrap();
        let url = format!("http://127.0.0.1:{port}/");
        let mut test_skin = skin(vec![measure(
            "WebParser",
            "Web",
            &[
                ("Url", &url),
                ("Download", "1"),
                ("DownloadFile", "downloaded.html"),
                ("Debug", "2"),
                ("Debug2File", "dump.html"),
            ],
        )]);
        test_skin.root = directory.path().to_path_buf();
        let mut engine = MeasureEngine::new(&test_skin);
        let downloaded = directory.path().join("downloaded.html");
        let expected_path = downloaded.display().to_string();
        let report = wait_for_web(&mut engine, |report| {
            report
                .strings
                .get("Web")
                .is_some_and(|value| value == &expected_path)
        });
        server.join().unwrap();

        let dump = directory.path().join("dump.html");
        assert_eq!(
            std::fs::read_to_string(&downloaded).unwrap(),
            "<html>payload</html>"
        );
        assert_eq!(
            std::fs::read_to_string(&dump).unwrap(),
            "<html>payload</html>"
        );
        assert_eq!(
            report.strings.get("Web").unwrap(),
            &downloaded.display().to_string()
        );
    }

    #[test]
    fn command_measure_resets_webparser_and_loop() {
        let mut engine = MeasureEngine::new(&skin(vec![
            measure("WebParser", "Web", &[("Url", "http://127.0.0.1:1/")]),
            measure("Loop", "Loop", &[("Start", "1"), ("End", "5")]),
        ]));
        assert!(engine.command_measure("Web", "Update"));
        assert!(engine.command_measure("Web", "Reset"));
        assert!(engine.command_measure("Loop", "Reset"));
        assert!(!engine.command_measure("Missing", "Reset"));
    }

    #[test]
    fn webparser_fetches_and_extracts_text() {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 2048];
            let _ = stream.read(&mut buffer);
            let body = "<html><title>Hello</title></html>";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let url = format!("http://127.0.0.1:{port}/");
        let mut engine = MeasureEngine::new(&skin(vec![measure(
            "WebParser",
            "Web",
            &[
                ("Url", &url),
                ("RegExp", "<title>(.*?)</title>"),
                ("StringIndex", "1"),
            ],
        )]));
        let report = wait_for_web(&mut engine, |report| {
            report
                .strings
                .get("Web")
                .is_some_and(|value| value == "Hello")
        });
        server.join().unwrap();
        assert_eq!(report.strings.get("Web").unwrap(), "Hello");
    }

    #[test]
    fn nowplaying_reads_mpris_metadata_and_track_changes() {
        let mut backend = MockBackend::new();
        backend.media = Some(NowPlayingInfo {
            bus_name: "org.mpris.MediaPlayer2.spotify".to_string(),
            identity: "Spotify".to_string(),
            status: "Playing".to_string(),
            title: "Titre".to_string(),
            artist: "Artiste".to_string(),
            duration_secs: 65.0,
            position_secs: 5.0,
            track_id: "/track/1".to_string(),
            ..NowPlayingInfo::default()
        });
        let mut engine = MeasureEngine::new(&skin(vec![
            measure(
                "NowPlaying",
                "Parent",
                &[
                    ("PlayerName", "BEST"),
                    ("PlayerType", "STATUS"),
                    ("TrackChangeAction", "[!Update]"),
                ],
            ),
            measure(
                "NowPlaying",
                "Title",
                &[("PlayerName", "[Parent]"), ("PlayerType", "TITLE")],
            ),
            measure(
                "NowPlaying",
                "Duration",
                &[("PlayerName", "[Parent]"), ("PlayerType", "DURATION")],
            ),
            measure(
                "NowPlaying",
                "Progress",
                &[("PlayerName", "[Parent]"), ("PlayerType", "PROGRESS")],
            ),
        ]));
        let report = engine.update(&backend, fixed_now());
        assert_eq!(report.strings.get("Title").unwrap(), "Titre");
        assert_eq!(report.strings.get("Duration").unwrap(), "01:05");
        assert_eq!(
            report.values.get("Progress").unwrap().number,
            100.0 / 13.0 * 1.0
        );
        assert!(report.actions.iter().any(|action| action == "[!Update]"));

        let second = engine.update(&backend, fixed_now());
        assert!(!second.actions.iter().any(|action| action == "[!Update]"));
    }

    #[test]
    fn nowplaying_handles_missing_player() {
        let mut engine = MeasureEngine::new(&skin(vec![measure(
            "NowPlaying",
            "Title",
            &[("PlayerName", "BEST"), ("PlayerType", "TITLE")],
        )]));
        let report = engine.update(&MockBackend::new(), fixed_now());
        assert_eq!(report.strings.get("Title").unwrap(), "");
        assert_eq!(report.values.get("Title").unwrap().number, 0.0);
    }

    #[test]
    fn unknown_measure_is_empty() {
        let mut engine = MeasureEngine::new(&skin(vec![measure("Nope", "N", &[])]));
        let report = update_once(&mut engine);
        assert_eq!(report.strings.get("N").unwrap(), "");
    }
}
