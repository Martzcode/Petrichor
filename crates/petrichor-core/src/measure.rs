use std::cell::Cell;
use std::collections::BTreeMap;

use chrono::{DateTime, Local};
use petrichor_sys::{CpuTimes, SystemBackend};

use crate::formula::{self, EvalContext};
use crate::model::{Measure, Options, Skin};
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
}

impl MeasureEngine {
    pub fn new(skin: &Skin) -> Self {
        let mut variables = Variables::new();
        for (name, value) in &skin.variables {
            variables.set(name, value);
        }

        let states = skin
            .measures
            .iter()
            .map(|measure| MeasureState::new(measure, skin.settings.default_update_divider))
            .collect();

        MeasureEngine {
            states,
            variables,
            update_number: 0,
            rng: Cell::new(seed()),
        }
    }

    pub fn with_seed(mut self, seed: u64) -> Self {
        self.rng = Cell::new(seed);
        self
    }

    pub fn variables(&self) -> &Variables {
        &self.variables
    }

    pub fn set_variable(&mut self, name: &str, value: &str) {
        self.variables.set(name, value);
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
                    self.states[index].last_value = value.clone();
                    values.insert(self.states[index].key.clone(), value);
                }
                continue;
            }

            let previous = self.states[index].last_value.clone();
            let context = Context {
                values: &values,
                variables: &self.variables,
                counter: self.update_number as f64,
                rng: &self.rng,
            };

            let mut value = compute(&mut self.states[index], &context, backend, now);
            apply_bounds(&self.states[index], &mut value);
            let text = self.states[index].substitute.apply(&display_text(&value));
            value.string = Some(text.clone());

            values.insert(self.states[index].key.clone(), value.clone());

            if let Some(action) = self.states[index].options.get("OnUpdateAction") {
                actions.push(self.variables.expand(action));
            }
            if previous.string.as_deref().unwrap_or_default() != text {
                if let Some(action) = self.states[index].options.get("OnChangeAction") {
                    actions.push(self.variables.expand(action));
                }
            }

            self.states[index].last_value = value;
        }

        let mut strings = BTreeMap::new();
        let mut values_by_name = BTreeMap::new();
        for state in &self.states {
            strings.insert(state.name.clone(), display_text(&state.last_value));
            values_by_name.insert(state.name.clone(), state.last_value.clone());
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
        }
    }
}

impl MeasureState {
    fn new(measure: &Measure, default_divider: u32) -> Self {
        MeasureState {
            name: measure.name.clone(),
            key: measure.name.to_ascii_lowercase(),
            kind: measure.kind.trim().to_ascii_uppercase(),
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
        "MEMORY" => memory_value(backend, false),
        "PHYSICALMEMORY" => memory_value(backend, true),
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
        .and_then(|value| value.trim().parse::<usize>().ok());

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

fn memory_value(backend: &dyn SystemBackend, physical: bool) -> MeasureValue {
    let info = backend.memory_info();
    if physical {
        let used_mb = info.used_kb() as f64 / 1024.0;
        MeasureValue {
            number: used_mb,
            string: Some(format_display(used_mb)),
            min: Some(0.0),
            max: Some(info.total_kb as f64 / 1024.0),
        }
    } else {
        let percent = info.used_percent();
        MeasureValue {
            number: percent,
            string: Some(format_display(percent)),
            min: Some(0.0),
            max: Some(100.0),
        }
    }
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
        value.max = Some(read_f64(max, value.max.unwrap_or(0.0)));
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
    use petrichor_sys::{CpuSnapshot, MemoryInfo, OsRelease};
    use std::path::PathBuf;

    struct MockBackend {
        uptime: u64,
        cpu: CpuSnapshot,
        memory: MemoryInfo,
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
                },
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
    fn computes_memory_percent_and_physical() {
        let mut engine = MeasureEngine::new(&skin(vec![
            measure("Memory", "Mem", &[]),
            measure("PhysicalMemory", "Phys", &[]),
        ]));
        let report = update_once(&mut engine);
        assert_eq!(report.strings.get("Mem").unwrap(), "25");
        assert_eq!(report.values.get("Mem").unwrap().max, Some(100.0));
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
            "Memory",
            "Free",
            &[("InvertMeasure", "1")],
        )]));
        let report = update_once(&mut engine);
        assert_eq!(report.strings.get("Free").unwrap(), "75");
    }

    #[test]
    fn unknown_measure_is_empty() {
        let mut engine = MeasureEngine::new(&skin(vec![measure("Nope", "N", &[])]));
        let report = update_once(&mut engine);
        assert_eq!(report.strings.get("N").unwrap(), "");
    }
}
