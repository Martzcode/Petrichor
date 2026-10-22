use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(test)]
use petrichor_plugin::{NativePlugin, PluginHost};

pub fn run_plugin_runner() {
    petrichor_plugin::run_runner();
}
use petrichor_script::{ScriptContext, ScriptEngine, ScriptMeasureValue, ScriptMeter};
use petrichor_sys::{CpuTimes, NetSample, SystemBackend};
use serde_json::{json, Value as JsonValue};

#[cfg(test)]
use crate::formula;
use crate::model::{Measure, Meter, Options};
#[cfg(test)]
use crate::reference::resolve_in_text;
use crate::reference::{MeasureSource, MeasureValue};
use crate::variables::Variables;

#[derive(Debug, Clone)]
pub struct RuntimeOutput {
    pub value: MeasureValue,
    pub actions: Vec<String>,
}

impl RuntimeOutput {
    pub fn empty() -> RuntimeOutput {
        RuntimeOutput {
            value: MeasureValue {
                number: 0.0,
                string: Some(String::new()),
                min: None,
                max: None,
            },
            actions: Vec::new(),
        }
    }

    pub fn text(text: impl Into<String>) -> RuntimeOutput {
        RuntimeOutput {
            value: MeasureValue {
                number: 0.0,
                string: Some(text.into()),
                min: None,
                max: None,
            },
            actions: Vec::new(),
        }
    }

    pub fn number(number: f64, min: Option<f64>, max: Option<f64>) -> RuntimeOutput {
        RuntimeOutput {
            value: MeasureValue {
                number,
                string: Some(format_number(number)),
                min,
                max,
            },
            actions: Vec::new(),
        }
    }

    fn with_actions(mut self, actions: Vec<String>) -> RuntimeOutput {
        self.actions.extend(actions);
        self
    }
}

pub struct RuntimeContext<'a> {
    pub skin_name: &'a str,
    pub skin_root: &'a Path,
    pub skin_path: &'a Path,
    pub measure_name: &'a str,
    pub options: &'a Options,
    pub variables: &'a Variables,
    pub backend: &'a dyn SystemBackend,
    pub values: &'a BTreeMap<String, MeasureValue>,
    pub meters: &'a [Meter],
    pub sections: &'a BTreeMap<String, BTreeMap<String, String>>,
    pub skin_window_handle: u64,
    pub counter: f64,
}

impl RuntimeContext<'_> {
    pub fn option(&self, key: &str) -> Option<String> {
        self.options
            .get(key)
            .map(|value| self.variables.expand(value))
    }

    pub fn option_or(&self, key: &str, default: &str) -> String {
        self.option(key).unwrap_or_else(|| default.to_string())
    }

    pub fn bool_option(&self, key: &str, default: bool) -> bool {
        self.options
            .get(key)
            .map(|value| crate::readers::read_bool(value, default))
            .unwrap_or(default)
    }

    pub fn number_option(&self, key: &str, default: f64) -> f64 {
        self.options
            .get(key)
            .map(|value| crate::readers::read_f64(value, default))
            .unwrap_or(default)
    }

    pub fn path_option(&self, key: &str, default: &str) -> PathBuf {
        let value = self.option_or(key, default);
        let path = PathBuf::from(value);
        if path.is_absolute() {
            path
        } else {
            self.skin_root.join(path)
        }
    }
}

pub trait RuntimeMeasure: Send {
    fn update(&mut self, context: &RuntimeContext<'_>) -> RuntimeOutput;
    fn command(&mut self, _command: &str, _context: &RuntimeContext<'_>) -> bool {
        false
    }
    fn call(&mut self, _function: &str, _context: &RuntimeContext<'_>) -> Option<String> {
        None
    }
}

fn format_number(number: f64) -> String {
    if !number.is_finite() {
        return "0".to_string();
    }
    let rounded = (number * 100.0).round() / 100.0;
    let mut text = format!("{rounded:.2}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if text == "-0" {
        "0".to_string()
    } else {
        text
    }
}

#[cfg(test)]
struct PluginHostContext<'a, 'b> {
    context: &'a RuntimeContext<'b>,
    actions: Vec<String>,
}

#[cfg(test)]
impl PluginHost for PluginHostContext<'_, '_> {
    fn read_string(&mut self, option: &str, default: &str, replace: bool) -> String {
        let value = self
            .context
            .options
            .get(option)
            .unwrap_or(default)
            .to_string();
        if replace {
            self.context.variables.expand(&value)
        } else {
            value
        }
    }

    fn read_formula(&mut self, option: &str, default: f64) -> f64 {
        let Some(value) = self.context.options.get(option) else {
            return default;
        };
        formula::evaluate(&self.context.variables.expand(value)).unwrap_or(default)
    }

    fn execute(&mut self, action: &str) {
        self.actions.push(self.context.variables.expand(action));
    }

    fn log(&mut self, level: i32, message: &str) {
        if level >= 2 {
            tracing::warn!(measure = %self.context.measure_name, "{message}");
        } else {
            tracing::debug!(measure = %self.context.measure_name, "{message}");
        }
    }

    fn get(&mut self, kind: &str) -> String {
        match kind.trim().to_ascii_uppercase().as_str() {
            "MEASURENAME" => self.context.measure_name.to_string(),
            "SKIN" => self.context.skin_root.to_string_lossy().to_string(),
            "SETTINGSFILE" => settings_file().to_string_lossy().to_string(),
            "SKINNAME" => self.context.skin_name.to_string(),
            "SKINWINDOWHANDLE" => self.context.skin_window_handle.to_string(),
            _ => String::new(),
        }
    }

    fn read_string_from_section(
        &mut self,
        section: &str,
        option: &str,
        default: &str,
        replace: bool,
    ) -> String {
        let value = self
            .context
            .sections
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(section))
            .and_then(|(_, entries)| {
                entries
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case(option))
                    .map(|(_, value)| value.as_str())
            })
            .unwrap_or(default)
            .to_string();
        if replace {
            self.context.variables.expand(&value)
        } else {
            value
        }
    }

    fn read_formula_from_section(&mut self, section: &str, option: &str, default: f64) -> f64 {
        let value = self.read_string_from_section(section, option, "", true);
        if value.is_empty() {
            return default;
        }
        crate::formula::evaluate(&value).unwrap_or(default)
    }

    fn read_path(&mut self, option: &str, default: &str) -> String {
        let value = self.read_string(option, default, true);
        let path = Path::new(&value);
        if path.is_absolute() {
            value
        } else {
            self.context
                .skin_root
                .join(path)
                .to_string_lossy()
                .to_string()
        }
    }

    fn replace_variables(&mut self, text: &str) -> String {
        let expanded = self.context.variables.expand(text);
        resolve_in_text(&expanded, self.context)
    }

    fn path_to_absolute(&mut self, path: &str) -> String {
        let expanded = self.replace_variables(path);
        let path = Path::new(&expanded);
        if path.is_absolute() {
            expanded
        } else {
            self.context
                .skin_root
                .join(path)
                .to_string_lossy()
                .to_string()
        }
    }
}

impl MeasureSource for RuntimeContext<'_> {
    fn measure(&self, name: &str) -> Option<MeasureValue> {
        self.values.get(&name.to_ascii_lowercase()).cloned()
    }
}

fn settings_file() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_default()
        .join("petrichor")
        .join("Petrichor.ini")
}

struct ScriptMeasureRuntime {
    engine: ScriptEngine,
    last_error: Option<String>,
}

impl ScriptMeasureRuntime {
    fn new(path: &Path) -> Result<ScriptMeasureRuntime, String> {
        ScriptEngine::load(path)
            .map(|engine| ScriptMeasureRuntime {
                engine,
                last_error: None,
            })
            .map_err(|error| error.to_string())
    }
}

impl RuntimeMeasure for ScriptMeasureRuntime {
    fn update(&mut self, context: &RuntimeContext<'_>) -> RuntimeOutput {
        let script_context = script_context(context);
        let output = self.engine.update(&script_context);
        if let Some(error) = output.error {
            if self.last_error.as_deref() != Some(error.as_str()) {
                tracing::warn!(measure = %context.measure_name, "script : {error}");
                self.last_error = Some(error);
            }
        } else {
            self.last_error = None;
        }
        RuntimeOutput {
            value: MeasureValue {
                number: output.value.number,
                string: Some(if output.value.string.is_empty() {
                    crate::plugins::format_number(output.value.number)
                } else {
                    output.value.string
                }),
                min: output.value.min,
                max: output.value.max,
            },
            actions: output.actions,
        }
    }

    fn command(&mut self, command: &str, context: &RuntimeContext<'_>) -> bool {
        let command = command.trim();
        if command.is_empty() {
            return false;
        }
        let script_context = script_context(context);
        let output = self.engine.update(&script_context);
        let _ = output;
        let (name, arguments) = parse_script_call(command);
        self.engine.call(&name, &arguments).is_some()
    }

    fn call(&mut self, function: &str, _context: &RuntimeContext<'_>) -> Option<String> {
        let (name, arguments) = parse_script_call(function);
        self.engine.call(&name, &arguments)
    }
}

fn script_context(context: &RuntimeContext<'_>) -> ScriptContext {
    let options = context
        .options
        .entries
        .iter()
        .map(|entry| (entry.key.clone(), context.variables.expand(&entry.value)))
        .collect::<BTreeMap<_, _>>();
    let values = context
        .values
        .iter()
        .map(|(name, value)| {
            (
                name.clone(),
                ScriptMeasureValue {
                    number: value.number,
                    string: value.string.clone().unwrap_or_default(),
                    min: value.min,
                    max: value.max,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let meters = context
        .meters
        .iter()
        .map(|meter| {
            let option = |key: &str| meter.options.get(key).unwrap_or_default().to_string();
            let number = |key: &str, default: f64| {
                meter
                    .options
                    .get(key)
                    .and_then(|value| value.trim().parse::<f64>().ok())
                    .unwrap_or(default)
            };
            (
                meter.name.to_ascii_lowercase(),
                ScriptMeter {
                    name: meter.name.clone(),
                    options: meter
                        .options
                        .entries
                        .iter()
                        .map(|entry| (entry.key.clone(), entry.value.clone()))
                        .collect(),
                    x: number("X", 0.0),
                    y: number("Y", 0.0),
                    w: number("W", 0.0),
                    h: number("H", 0.0),
                    text: option("Text"),
                    hidden: option("Hidden").eq_ignore_ascii_case("true")
                        || option("Hidden") == "1",
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    ScriptContext {
        measure_name: context.measure_name.to_string(),
        skin_name: context.skin_name.to_string(),
        skin_root: context.skin_root.to_path_buf(),
        skin_path: context.skin_path.to_path_buf(),
        options,
        variables: context.variables.to_raw_map(),
        values,
        meters,
        counter: context.counter,
    }
}

fn parse_script_call(input: &str) -> (String, Vec<String>) {
    let input = input.trim();
    let Some(open) = input.find('(') else {
        return (input.to_string(), Vec::new());
    };
    let name = input[..open].trim().to_string();
    let arguments = input[open + 1..].trim_end_matches(')').trim();
    if arguments.is_empty() {
        return (name, Vec::new());
    }
    (name, split_script_arguments(arguments))
}

fn split_script_arguments(input: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut depth = 0usize;
    for character in input.chars() {
        match character {
            '"' | '\'' if depth == 0 => {
                if quote == Some(character) {
                    quote = None;
                    current.push(character);
                } else if quote.is_none() {
                    quote = Some(character);
                    current.push(character);
                } else {
                    current.push(character);
                }
            }
            '(' if quote.is_none() => {
                depth += 1;
                current.push(character);
            }
            ')' if quote.is_none() => {
                depth = depth.saturating_sub(1);
                current.push(character);
            }
            ',' if quote.is_none() && depth == 0 => {
                arguments.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(character),
        }
    }
    if !current.trim().is_empty() {
        arguments.push(current.trim().to_string());
    }
    arguments
}

pub fn create_script_measure(
    measure: &Measure,
    skin_root: &Path,
) -> Option<Box<dyn RuntimeMeasure>> {
    let file = measure
        .options
        .get("ScriptFile")
        .or_else(|| measure.options.get("File"))
        .unwrap_or_default()
        .trim();
    if file.is_empty() {
        return Some(Box::new(DegradedPlugin {
            message: format!(
                "MeasureScript {} : option ScriptFile absente, mesure vide",
                measure.name
            ),
            warned: false,
        }));
    }
    let path = PathBuf::from(file);
    let path = if path.is_absolute() {
        path
    } else {
        skin_root.join(path)
    };
    match ScriptMeasureRuntime::new(&path) {
        Ok(runtime) => Some(Box::new(runtime)),
        Err(error) => Some(Box::new(DegradedPlugin {
            message: format!(
                "MeasureScript {} : impossible de charger « {} » : {error}",
                measure.name,
                path.display()
            ),
            warned: false,
        })),
    }
}

#[cfg(test)]
struct NativePluginMeasure {
    plugin: NativePlugin,
    initialized: bool,
    max_value: f64,
    pending_actions: Vec<String>,
}

#[cfg(test)]
impl NativePluginMeasure {
    fn new(plugin: NativePlugin) -> NativePluginMeasure {
        NativePluginMeasure {
            plugin,
            initialized: false,
            max_value: 0.0,
            pending_actions: Vec::new(),
        }
    }
}

#[cfg(test)]
impl RuntimeMeasure for NativePluginMeasure {
    fn update(&mut self, context: &RuntimeContext<'_>) -> RuntimeOutput {
        let mut host = PluginHostContext {
            context,
            actions: Vec::new(),
        };
        if !self.initialized {
            if self.plugin.initialize(&mut host).is_err() {
                return RuntimeOutput::empty();
            }
            self.max_value = self.plugin.reload(&mut host);
            self.initialized = true;
        }
        let number = self.plugin.update(&mut host);
        self.pending_actions.append(&mut host.actions);
        let text = self.plugin.read_string();
        RuntimeOutput {
            value: MeasureValue {
                number,
                string: Some(text),
                min: Some(0.0),
                max: Some(self.max_value),
            },
            actions: std::mem::take(&mut self.pending_actions),
        }
    }

    fn command(&mut self, command: &str, context: &RuntimeContext<'_>) -> bool {
        let mut host = PluginHostContext {
            context,
            actions: Vec::new(),
        };
        let handled = self.plugin.execute_bang(&mut host, command);
        self.pending_actions.append(&mut host.actions);
        handled
    }

    fn call(&mut self, function: &str, context: &RuntimeContext<'_>) -> Option<String> {
        let (name, args) = split_function(function);
        let mut host = PluginHostContext {
            context,
            actions: Vec::new(),
        };
        let result = self.plugin.call(&mut host, &name, &args);
        self.pending_actions.append(&mut host.actions);
        result
    }
}

struct PluginProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl PluginProcess {
    fn start() -> Result<PluginProcess, String> {
        let mut command = runner_command()?;
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| format!("lancement du runner de plugins : {error}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "stdin du runner indisponible".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "stdout du runner indisponible".to_string())?;
        Ok(PluginProcess {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    fn request(&mut self, request: &JsonValue) -> Result<JsonValue, String> {
        let line = serde_json::to_string(request)
            .map_err(|error| format!("encodage de la requête plugin : {error}"))?;
        writeln!(self.stdin, "{line}")
            .and_then(|_| self.stdin.flush())
            .map_err(|error| format!("écriture vers le runner de plugins : {error}"))?;
        let mut response = String::new();
        let read = self
            .stdout
            .read_line(&mut response)
            .map_err(|error| format!("lecture du runner de plugins : {error}"))?;
        if read == 0 {
            return Err("le runner de plugins s'est arrêté".to_string());
        }
        let response: JsonValue = serde_json::from_str(&response)
            .map_err(|error| format!("réponse invalide du runner de plugins : {error}"))?;
        if response.get("ok").and_then(JsonValue::as_bool) == Some(true) {
            Ok(response)
        } else {
            Err(response
                .get("error")
                .and_then(JsonValue::as_str)
                .unwrap_or("erreur inconnue du runner de plugins")
                .to_string())
        }
    }
}

impl Drop for PluginProcess {
    fn drop(&mut self) {
        if writeln!(self.stdin, "{{\"op\":\"shutdown\"}}")
            .and_then(|_| self.stdin.flush())
            .is_ok()
        {
            for _ in 0..10 {
                if matches!(self.child.try_wait(), Ok(Some(_))) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn runner_command() -> Result<Command, String> {
    if let Some(path) = std::env::var_os("PETRICHOR_PLUGIN_RUNNER") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(Command::new(path));
        }
    }
    if let Ok(current) = std::env::current_exe() {
        if let Some(directory) = current.parent() {
            for name in ["petrichor-plugin-runner", "petrichor-plugin-runner.exe"] {
                let candidate = directory.join(name);
                if candidate.is_file() {
                    return Ok(Command::new(candidate));
                }
            }
        }
        let mut command = Command::new(current);
        command.arg("--plugin-runner");
        return Ok(command);
    }
    Err("runner de plugins introuvable".to_string())
}

fn isolated_context(context: &RuntimeContext<'_>, include_sections: bool) -> JsonValue {
    let options: BTreeMap<&str, &str> = context
        .options
        .entries
        .iter()
        .map(|entry| (entry.key.as_str(), entry.value.as_str()))
        .collect();
    let mut value = json!({
        "skin_name": context.skin_name,
        "root": context.skin_root.to_string_lossy(),
        "skin_path": context.skin_path.to_string_lossy(),
        "measure_name": context.measure_name,
        "options": options,
        "variables": context.variables.to_raw_map(),
        "values": context.values,
        "window_handle": context.skin_window_handle,
    });
    if include_sections {
        if let Some(object) = value.as_object_mut() {
            object.insert("sections".to_string(), json!(context.sections));
        }
    }
    value
}

struct IsolatedPluginMeasure {
    path: PathBuf,
    process: Option<PluginProcess>,
    pending_actions: Vec<String>,
    max_value: f64,
    warned: bool,
}

impl IsolatedPluginMeasure {
    fn new(path: PathBuf) -> IsolatedPluginMeasure {
        IsolatedPluginMeasure {
            path,
            process: None,
            pending_actions: Vec::new(),
            max_value: 0.0,
            warned: false,
        }
    }

    fn ensure(&mut self, context: &RuntimeContext<'_>) -> bool {
        if self.process.is_some() {
            return true;
        }
        let started = PluginProcess::start().and_then(|mut process| {
            let response = process.request(&json!({
                "op": "init",
                "path": self.path.to_string_lossy(),
                "context": isolated_context(context, true),
            }))?;
            Ok((process, response))
        });
        match started {
            Ok((process, response)) => {
                self.max_value = response
                    .get("max")
                    .and_then(JsonValue::as_f64)
                    .unwrap_or(0.0);
                if let Some(actions) = response.get("actions").and_then(JsonValue::as_array) {
                    self.pending_actions.extend(
                        actions
                            .iter()
                            .filter_map(JsonValue::as_str)
                            .map(str::to_string),
                    );
                }
                self.process = Some(process);
                self.warned = false;
                true
            }
            Err(error) => {
                self.fail(error);
                false
            }
        }
    }

    fn fail(&mut self, error: String) {
        self.process = None;
        if !self.warned {
            tracing::warn!(
                plugin = %self.path.display(),
                "plugin isolé indisponible : {error}"
            );
            self.warned = true;
        }
    }

    fn actions(&mut self, response: &JsonValue) {
        if let Some(actions) = response.get("actions").and_then(JsonValue::as_array) {
            self.pending_actions.extend(
                actions
                    .iter()
                    .filter_map(JsonValue::as_str)
                    .map(str::to_string),
            );
        }
    }
}

impl RuntimeMeasure for IsolatedPluginMeasure {
    fn update(&mut self, context: &RuntimeContext<'_>) -> RuntimeOutput {
        if !self.ensure(context) {
            return RuntimeOutput::empty();
        }
        let response = self.process.as_mut().map(|process| {
            process.request(&json!({
                "op": "update",
                "context": isolated_context(context, false),
            }))
        });
        let response = match response {
            Some(Ok(response)) => response,
            Some(Err(error)) => {
                self.fail(error);
                return RuntimeOutput::empty();
            }
            None => return RuntimeOutput::empty(),
        };
        self.max_value = response
            .get("max")
            .and_then(JsonValue::as_f64)
            .unwrap_or(self.max_value);
        self.actions(&response);
        let number = response
            .get("number")
            .and_then(JsonValue::as_f64)
            .unwrap_or(0.0);
        let text = response
            .get("text")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
            .to_string();
        RuntimeOutput {
            value: MeasureValue {
                number,
                string: Some(text),
                min: Some(0.0),
                max: Some(self.max_value),
            },
            actions: std::mem::take(&mut self.pending_actions),
        }
    }

    fn command(&mut self, command: &str, context: &RuntimeContext<'_>) -> bool {
        if !self.ensure(context) {
            return false;
        }
        let response = self.process.as_mut().map(|process| {
            process.request(&json!({
                "op": "bang",
                "context": isolated_context(context, false),
                "command": command,
            }))
        });
        match response {
            Some(Ok(response)) => {
                self.actions(&response);
                response
                    .get("handled")
                    .and_then(JsonValue::as_bool)
                    .unwrap_or(false)
            }
            Some(Err(error)) => {
                self.fail(error);
                false
            }
            None => false,
        }
    }

    fn call(&mut self, function: &str, context: &RuntimeContext<'_>) -> Option<String> {
        if !self.ensure(context) {
            return None;
        }
        let (name, arguments) = split_function(function);
        let response = self.process.as_mut().map(|process| {
            process.request(&json!({
                "op": "call",
                "context": isolated_context(context, false),
                "function": name,
                "arguments": arguments,
            }))
        });
        match response {
            Some(Ok(response)) => {
                self.actions(&response);
                response
                    .get("text")
                    .and_then(JsonValue::as_str)
                    .map(str::to_string)
            }
            Some(Err(error)) => {
                self.fail(error);
                None
            }
            None => None,
        }
    }
}

#[derive(Debug)]
struct DegradedPlugin {
    message: String,
    warned: bool,
}

impl RuntimeMeasure for DegradedPlugin {
    fn update(&mut self, _context: &RuntimeContext<'_>) -> RuntimeOutput {
        if !self.warned {
            tracing::warn!("{}", self.message);
            self.warned = true;
        }
        RuntimeOutput::empty()
    }
}

pub fn create_plugin_measure(
    measure: &Measure,
    skin_name: &str,
    skin_root: &Path,
    skin_path: &Path,
) -> Option<Box<dyn RuntimeMeasure>> {
    let name = measure.options.get("Plugin").unwrap_or_default().trim();
    if name.is_empty() {
        return Some(Box::new(DegradedPlugin {
            message: format!(
                "MeasurePlugin {} : option Plugin absente, mesure vide",
                measure.name
            ),
            warned: false,
        }));
    }
    let module = plugin_module_name(name);
    if let Some(builtin) = create_builtin(&module) {
        return Some(builtin);
    }

    let mut roots = Vec::new();
    roots.push(skin_root.to_path_buf());
    if let Some(parent) = skin_root.parent() {
        roots.push(parent.to_path_buf());
    }
    if let Some(path) = std::env::var_os("PETRICHOR_PLUGINS") {
        roots.extend(std::env::split_paths(&path));
    }
    roots.push(
        settings_file()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default(),
    );
    roots.push(
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
            .unwrap_or_default()
            .join("petrichor"),
    );

    let path = if name.contains('/') || name.ends_with(".so") {
        PathBuf::from(name)
    } else {
        petrichor_plugin::find_plugin(&module, &roots).unwrap_or_default()
    };
    if path.as_os_str().is_empty() {
        return Some(Box::new(DegradedPlugin {
            message: format!(
                "Plugin {module} introuvable pour {} (skin {skin_name}, {}) : mesure vide",
                measure.name,
                skin_path.display()
            ),
            warned: false,
        }));
    }
    Some(Box::new(IsolatedPluginMeasure::new(path)))
}

fn plugin_module_name(name: &str) -> String {
    let name = Path::new(name)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| name.to_string());
    name.strip_suffix(".dll")
        .or_else(|| name.strip_suffix(".so"))
        .unwrap_or(&name)
        .to_string()
}

fn split_function(function: &str) -> (String, Vec<String>) {
    let Some(start) = function.find('(') else {
        return (function.trim().to_string(), Vec::new());
    };
    let end = function.rfind(')').unwrap_or(function.len());
    let name = function[..start].trim().to_string();
    let args = split_arguments(&function[start + 1..end]);
    (name, args)
}

fn split_arguments(input: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut depth = 0i32;
    for character in input.chars() {
        if let Some(active) = quote {
            if character == active {
                quote = None;
            } else {
                current.push(character);
            }
            continue;
        }
        match character {
            '"' | '\'' => quote = Some(character),
            '(' => {
                depth += 1;
                current.push(character);
            }
            ')' => {
                depth -= 1;
                current.push(character);
            }
            ',' if depth == 0 => {
                arguments.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(character),
        }
    }
    if !current.trim().is_empty() || !arguments.is_empty() {
        arguments.push(current.trim().to_string());
    }
    arguments
}

pub fn create_builtin(name: &str) -> Option<Box<dyn RuntimeMeasure>> {
    match name.to_ascii_lowercase().as_str() {
        "runcommand" => Some(Box::new(RunCommandMeasure::default())),
        "actiontimer" => Some(Box::new(ActionTimerMeasure::default())),
        "power" => Some(Box::new(PowerMeasure)),
        "ping" => Some(Box::new(PingMeasure::default())),
        "audiolevel" => Some(Box::new(AudioLevelMeasure::default())),
        "win7audio" | "audio" => Some(Box::new(AudioMeasure)),
        "fileview" => Some(Box::new(FileViewMeasure::default())),
        "folderinfo" => Some(Box::new(FolderInfoMeasure)),
        "coretemp" => Some(Box::new(CoreTempMeasure)),
        "speedfan" => Some(Box::new(SpeedFanMeasure)),
        "resmon" => Some(Box::new(ResMonMeasure)),
        "usagemonitor" => Some(Box::new(UsageMonitorMeasure::default())),
        "perfmon" => Some(Box::new(PerfMonMeasure::default())),
        "advancedcpu" => Some(Box::new(AdvancedCpuMeasure::default())),
        "quote" => Some(Box::new(QuoteMeasure::default())),
        "inputtext" => Some(Box::new(InputTextMeasure::default())),
        "windowmessage" => Some(Box::new(WindowMessageMeasure::default())),
        _ => None,
    }
}

#[derive(Default)]
struct RunCommandMeasure {
    output: String,
    started: bool,
    finished: bool,
    finish_action_sent: bool,
    child: Option<std::process::Child>,
    readers: Vec<std::thread::JoinHandle<()>>,
    stdout: Arc<Mutex<Vec<u8>>>,
    stderr: Arc<Mutex<Vec<u8>>>,
    started_at: Option<Instant>,
}

impl RunCommandMeasure {
    fn program(context: &RuntimeContext<'_>) -> Option<String> {
        let program = context.option("Program")?;
        if program.trim().is_empty() {
            return None;
        }
        let parameter = context.option_or("Parameter", "");
        if parameter.trim().is_empty() {
            Some(program)
        } else {
            Some(format!("{program} {parameter}"))
        }
    }

    fn start(&mut self, context: &RuntimeContext<'_>) -> bool {
        let Some(command) = Self::program(context) else {
            self.output.clear();
            self.started = true;
            self.finished = true;
            return false;
        };
        self.stop_process();
        self.output.clear();
        self.finished = false;
        self.finish_action_sent = false;
        let mut child = match Command::new("sh")
            .arg("-c")
            .arg(&command)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                self.output = format!("Erreur : {error}");
                self.started = true;
                self.finished = true;
                return false;
            }
        };
        self.stdout = Arc::new(Mutex::new(Vec::new()));
        self.stderr = Arc::new(Mutex::new(Vec::new()));
        if let Some(stdout) = child.stdout.take() {
            self.readers.push(spawn_reader(stdout, self.stdout.clone()));
        }
        if let Some(stderr) = child.stderr.take() {
            self.readers.push(spawn_reader(stderr, self.stderr.clone()));
        }
        apply_window_state(child.id(), &context.option_or("State", "HIDE"));
        self.child = Some(child);
        self.started = true;
        self.started_at = Some(Instant::now());
        true
    }

    fn stop_process(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
    }

    fn timeout(&self, context: &RuntimeContext<'_>) -> Option<Duration> {
        let milliseconds = context.number_option("Timeout", -1.0);
        (milliseconds > 0.0).then(|| Duration::from_millis(milliseconds as u64))
    }

    fn finish(&mut self, context: &RuntimeContext<'_>) -> RuntimeOutput {
        if let Some(mut child) = self.child.take() {
            let _ = child.wait();
        }
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
        let stdout = self
            .stdout
            .lock()
            .map(|bytes| bytes.clone())
            .unwrap_or_default();
        let stderr = self
            .stderr
            .lock()
            .map(|bytes| bytes.clone())
            .unwrap_or_default();
        let encoding = context.option_or("OutputType", "UTF8").to_ascii_uppercase();
        self.output = decode_output(&stdout, &encoding);
        if self.output.is_empty() && !stderr.is_empty() {
            self.output = decode_output(&stderr, &encoding);
        }
        self.finished = true;
        self.started_at = None;
        let mut result = RuntimeOutput::text(self.output.clone());
        if !self.finish_action_sent {
            if let Some(action) = context.option("FinishAction") {
                result.actions.push(action);
                self.finish_action_sent = true;
            }
        }
        result
    }

    fn poll(&mut self, context: &RuntimeContext<'_>) -> RuntimeOutput {
        if self.finished {
            return RuntimeOutput::text(self.output.clone());
        }
        if let Some(timeout) = self.timeout(context) {
            if self
                .started_at
                .is_some_and(|started| started.elapsed() >= timeout)
            {
                if let Some(child) = self.child.as_mut() {
                    let _ = child.kill();
                }
                return self.finish(context);
            }
        }
        let exited = self
            .child
            .as_mut()
            .and_then(|child| child.try_wait().ok().flatten())
            .is_some();
        if exited {
            return self.finish(context);
        }
        let encoding = context.option_or("OutputType", "UTF8").to_ascii_uppercase();
        let partial = self
            .stdout
            .lock()
            .map(|bytes| decode_output(&bytes, &encoding))
            .unwrap_or_default();
        RuntimeOutput::text(partial)
    }
}

fn spawn_reader(
    mut source: impl std::io::Read + Send + 'static,
    buffer: Arc<Mutex<Vec<u8>>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut chunk = [0u8; 8192];
        loop {
            match source.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    if let Ok(mut buffer) = buffer.lock() {
                        buffer.extend_from_slice(&chunk[..read]);
                    }
                }
            }
        }
    })
}

fn apply_window_state(pid: u32, state: &str) {
    let state = state.trim().to_ascii_uppercase();
    if state.is_empty() {
        return;
    }
    let actions: &[&str] = match state.as_str() {
        "HIDE" => &["windowunmap"],
        "SHOW" => &["windowmap", "windowactivate"],
        "MINIMIZE" => &["windowminimize"],
        "MAXIMIZE" => &["windowactivate", "windowsize", "100%", "100%"],
        _ => return,
    };
    let pid = pid.to_string();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        let mut command = Command::new("xdotool");
        command.args(["search", "--sync", "--pid", &pid]);
        command.args(actions);
        let _ = command.stdout(Stdio::null()).stderr(Stdio::null()).status();
    });
}

fn terminate_process(child: &mut std::process::Child) {
    let pid = child.id();
    let _ = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    for _ in 0..10 {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
}

impl RuntimeMeasure for RunCommandMeasure {
    fn update(&mut self, context: &RuntimeContext<'_>) -> RuntimeOutput {
        if !self.started {
            self.start(context);
            return RuntimeOutput::empty();
        }
        self.poll(context)
    }

    fn command(&mut self, command: &str, context: &RuntimeContext<'_>) -> bool {
        match command.trim().to_ascii_lowercase().as_str() {
            "run" => {
                self.start(context);
                true
            }
            "kill" => {
                if self.child.is_some() {
                    if let Some(child) = self.child.as_mut() {
                        let _ = child.kill();
                    }
                    self.finish(context);
                } else {
                    self.stop_process();
                    self.finished = true;
                }
                true
            }
            "close" => {
                if let Some(child) = self.child.as_mut() {
                    terminate_process(child);
                    self.finish(context);
                } else {
                    self.stop_process();
                    self.finished = true;
                }
                true
            }
            _ => false,
        }
    }
}

fn decode_output(bytes: &[u8], encoding: &str) -> String {
    match encoding {
        "UTF16" | "UTF-16" | "UNICODE" => {
            let (bytes, little_endian) = if bytes.starts_with(&[0xFF, 0xFE]) {
                (&bytes[2..], true)
            } else if bytes.starts_with(&[0xFE, 0xFF]) {
                (&bytes[2..], false)
            } else {
                (bytes, true)
            };
            let units: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|chunk| {
                    if little_endian {
                        u16::from_le_bytes(*chunk)
                    } else {
                        u16::from_be_bytes(*chunk)
                    }
                })
                .collect();
            String::from_utf16_lossy(&units)
        }
        "ANSI" => String::from_utf8_lossy(bytes).to_string(),
        _ => String::from_utf8_lossy(bytes).to_string(),
    }
}

#[derive(Default)]
struct ActionTimerMeasure {
    timers: Vec<ActionTimer>,
}

#[derive(Default)]
struct ActionTimer {
    tokens: Vec<String>,
    index: usize,
    next_at: Option<Instant>,
    running: bool,
}

impl ActionTimerMeasure {
    fn timer(&mut self, index: usize) -> &mut ActionTimer {
        while self.timers.len() <= index {
            self.timers.push(ActionTimer::default());
        }
        &mut self.timers[index]
    }
}

impl RuntimeMeasure for ActionTimerMeasure {
    fn update(&mut self, _context: &RuntimeContext<'_>) -> RuntimeOutput {
        let mut actions = Vec::new();
        let now = Instant::now();
        for timer in &mut self.timers {
            if !timer.running {
                continue;
            }
            while timer.running {
                let next_at = timer.next_at.unwrap_or(now);
                if now < next_at {
                    break;
                }
                let Some(token) = timer.tokens.get(timer.index).cloned() else {
                    timer.running = false;
                    timer.next_at = None;
                    timer.tokens.clear();
                    timer.index = 0;
                    break;
                };
                timer.index += 1;
                if let Some(wait) = token.strip_prefix("Wait ") {
                    let milliseconds = wait.trim().parse::<u64>().unwrap_or(0);
                    timer.next_at = Some(next_at + Duration::from_millis(milliseconds));
                } else {
                    actions.push(token);
                    timer.next_at = Some(next_at);
                }
            }
        }
        RuntimeOutput::empty().with_actions(actions)
    }

    fn command(&mut self, command: &str, context: &RuntimeContext<'_>) -> bool {
        let command = command.trim();
        let lower = command.to_ascii_lowercase();
        let (execute, rest) = if let Some(rest) = lower.strip_prefix("execute") {
            (true, &command[command.len() - rest.len()..])
        } else if let Some(rest) = lower.strip_prefix("stop") {
            (false, &command[command.len() - rest.len()..])
        } else {
            return false;
        };
        let Ok(index) = rest.trim().parse::<usize>() else {
            return false;
        };
        if index == 0 {
            return false;
        }
        let timer = self.timer(index - 1);
        if !execute {
            timer.running = false;
            timer.tokens.clear();
            timer.index = 0;
            timer.next_at = None;
            return true;
        }
        if timer.running {
            return true;
        }
        let key = format!("ActionList{index}");
        let Some(raw) = context.option(&key) else {
            return false;
        };
        timer.tokens = action_timer_tokens(&raw, context);
        timer.index = 0;
        timer.next_at = Some(Instant::now());
        timer.running = true;
        true
    }
}

fn action_timer_tokens(raw: &str, context: &RuntimeContext<'_>) -> Vec<String> {
    raw.split('|')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(|token| {
            let upper = token.to_ascii_uppercase();
            if upper.starts_with("WAIT ") {
                format!("Wait {}", token[5..].trim())
            } else if let Some(value) = context.option(token) {
                value
            } else {
                token.to_string()
            }
        })
        .collect()
}

struct PowerMeasure;

impl RuntimeMeasure for PowerMeasure {
    fn update(&mut self, context: &RuntimeContext<'_>) -> RuntimeOutput {
        let reading = read_power();
        match context
            .option_or("PowerState", "ACLINE")
            .to_ascii_uppercase()
            .as_str()
        {
            "ACLINE" => {
                RuntimeOutput::number(if reading.online { 1.0 } else { 0.0 }, Some(0.0), Some(1.0))
            }
            "STATUS" => RuntimeOutput::number(f64::from(reading.status_code), Some(0.0), Some(4.0)),
            "STATUS2" => RuntimeOutput::text(reading.status.clone()),
            "LIFETIME" => {
                let format = context.option_or("Format", "%H:%M");
                let seconds = reading.seconds_left.unwrap_or(0);
                RuntimeOutput {
                    value: MeasureValue {
                        number: seconds as f64,
                        string: Some(format_seconds_custom(seconds, &format)),
                        min: None,
                        max: None,
                    },
                    actions: Vec::new(),
                }
            }
            "MHZ" => RuntimeOutput::number(cpu_mhz(), None, None),
            "HZ" => RuntimeOutput::number(cpu_mhz() * 1_000_000.0, None, None),
            "PERCENT" => RuntimeOutput::number(reading.percent, Some(0.0), Some(100.0)),
            _ => RuntimeOutput::empty(),
        }
    }
}

struct PowerReading {
    online: bool,
    status: String,
    status_code: i32,
    percent: f64,
    seconds_left: Option<u64>,
}

fn read_power() -> PowerReading {
    let mut reading = PowerReading {
        online: false,
        status: "Unknown".to_string(),
        status_code: 0,
        percent: 100.0,
        seconds_left: None,
    };
    let Ok(entries) = fs::read_dir("/sys/class/power_supply") else {
        return reading;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let kind = fs::read_to_string(path.join("type")).unwrap_or_default();
        if (kind.trim() == "Mains" || kind.trim() == "USB")
            && fs::read_to_string(path.join("online"))
                .map(|value| value.trim() == "1")
                .unwrap_or(false)
        {
            reading.online = true;
        }
        if kind.trim() != "Battery" {
            continue;
        }
        reading.status = fs::read_to_string(path.join("status"))
            .map(|value| value.trim().to_string())
            .unwrap_or_else(|_| "Unknown".to_string());
        reading.percent = read_number(&path, "capacity").unwrap_or(0.0);
        reading.status_code = match reading.status.as_str() {
            "Charging" => 1,
            "Critical" => 2,
            "Low" => 3,
            "Full" | "Discharging" | "Not charging" => 4,
            _ => 0,
        };
        let energy_now = read_number(&path, "energy_now");
        let power_now = read_number(&path, "power_now");
        let charge_now = read_number(&path, "charge_now");
        let current_now = read_number(&path, "current_now");
        let seconds = match (energy_now, power_now) {
            (Some(energy), Some(power)) if power > 0.0 => Some((energy / power * 3600.0) as u64),
            _ => match (charge_now, current_now) {
                (Some(charge), Some(current)) if current > 0.0 => {
                    Some((charge / current * 3600.0) as u64)
                }
                _ => None,
            },
        };
        reading.seconds_left = seconds;
    }
    reading
}

fn read_number(path: &Path, name: &str) -> Option<f64> {
    fs::read_to_string(path.join(name))
        .ok()
        .and_then(|value| value.trim().parse().ok())
}

fn cpu_mhz() -> f64 {
    for path in [
        "/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq",
        "/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_cur_freq",
    ] {
        if let Ok(value) = fs::read_to_string(path) {
            if let Ok(value) = value.trim().parse::<f64>() {
                return value / 1000.0;
            }
        }
    }
    0.0
}

fn format_seconds_custom(seconds: u64, format: &str) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    format
        .replace("%H", &hours.to_string())
        .replace("%M", &format!("{minutes:02}"))
        .replace("%S", &format!("{:02}", seconds % 60))
}

#[derive(Default)]
struct PingMeasure {
    value: f64,
    checked: Option<u64>,
    text: String,
}

impl RuntimeMeasure for PingMeasure {
    fn update(&mut self, context: &RuntimeContext<'_>) -> RuntimeOutput {
        let update_rate = context.number_option("UpdateRate", 32.0).max(1.0) as u64;
        let counter = context.counter as u64;
        if self
            .checked
            .is_none_or(|last| counter.saturating_sub(last) >= update_rate)
        {
            self.checked = Some(counter);
            let destination = context.option_or("DestAddress", "127.0.0.1");
            let timeout = context.number_option("Timeout", 30_000.0).max(1.0) / 1000.0;
            let output = Command::new("ping")
                .args([
                    "-c",
                    "1",
                    "-W",
                    &format!("{:.0}", timeout.max(1.0)),
                    &destination,
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output();
            self.value = output
                .ok()
                .filter(|output| output.status.success())
                .and_then(|output| {
                    let text = String::from_utf8_lossy(&output.stdout).to_string();
                    text.split("time=")
                        .nth(1)
                        .and_then(|value| value.split_whitespace().next())
                        .and_then(|value| value.parse::<f64>().ok())
                })
                .unwrap_or_else(|| context.number_option("TimeoutValue", 30_000.0));
            self.text = format_number(self.value);
        }
        let finish_action = context.option_or("FinishAction", "");
        RuntimeOutput {
            value: MeasureValue {
                number: self.value,
                string: Some(self.text.clone()),
                min: None,
                max: None,
            },
            actions: if finish_action.is_empty() {
                Vec::new()
            } else {
                vec![finish_action]
            },
        }
    }
}

#[derive(Clone)]
struct AudioSnapshot {
    samples: Vec<f32>,
    channels: usize,
    sample_rate: u32,
    source: String,
}

static AUDIO_PARENTS: std::sync::LazyLock<Mutex<BTreeMap<String, AudioSnapshot>>> =
    std::sync::LazyLock::new(|| Mutex::new(BTreeMap::new()));

#[derive(Default)]
struct AudioLevelMeasure {
    sampler: Option<AudioSampler>,
    rms_envelope: f64,
    peak_envelope: f64,
    fft_envelope: f64,
    band_envelope: f64,
    last_update: Option<Instant>,
}

struct AudioSampler {
    child: std::process::Child,
    samples: Arc<Mutex<Vec<f32>>>,
    channels: usize,
    sample_rate: u32,
    source: String,
}

impl AudioSampler {
    fn start(context: &RuntimeContext<'_>) -> Option<AudioSampler> {
        use std::io::Read;

        let input = context
            .option_or("Port", "Output")
            .eq_ignore_ascii_case("Input");
        let configured = context.option("ID").unwrap_or_default();
        let source = if configured.trim().is_empty() {
            if input {
                "@DEFAULT_SOURCE@".to_string()
            } else {
                "@DEFAULT_MONITOR@".to_string()
            }
        } else {
            configured
        };
        let (channels, sample_rate) = audio_spec(input, &source);
        let mut child = Command::new("parec")
            .args([
                "--format=float32le",
                &format!("--rate={sample_rate}"),
                &format!("--channels={channels}"),
                "--latency-msec=50",
                &format!("--device={source}"),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .or_else(|_| {
                Command::new("pw-record")
                    .args([
                        "--format",
                        "f32",
                        "--rate",
                        &sample_rate.to_string(),
                        "--channels",
                        &channels.to_string(),
                        &format!("--target={source}"),
                        "-",
                    ])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
            })
            .ok()?;
        let mut stdout = child.stdout.take()?;
        let samples = Arc::new(Mutex::new(Vec::new()));
        let shared = samples.clone();
        std::thread::spawn(move || {
            let mut bytes = [0u8; 16_384];
            loop {
                match stdout.read(&mut bytes) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => {
                        let Ok(mut samples) = shared.lock() else {
                            break;
                        };
                        for chunk in bytes[..read].as_chunks::<4>().0 {
                            samples.push(f32::from_le_bytes(*chunk));
                        }
                        let max = 96_000 * channels;
                        if samples.len() > max {
                            let excess = samples.len() - max;
                            samples.drain(..excess);
                        }
                    }
                }
            }
        });
        Some(AudioSampler {
            child,
            samples,
            channels,
            sample_rate,
            source,
        })
    }

    fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn snapshot(&self) -> Vec<f32> {
        self.samples
            .lock()
            .map(|samples| samples.clone())
            .unwrap_or_default()
    }

    fn device_list(input: bool) -> String {
        Command::new("pactl")
            .args(["list", "short", if input { "sources" } else { "sinks" }])
            .output()
            .ok()
            .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
            .unwrap_or_default()
    }
}

fn audio_device_name(input: bool, source: &str) -> String {
    let device = if source.starts_with('@') || source.is_empty() {
        if input {
            "@DEFAULT_SOURCE@"
        } else {
            "@DEFAULT_SINK@"
        }
    } else {
        source
    };
    let command = if input {
        "get-source-description"
    } else {
        "get-sink-description"
    };
    Command::new("pactl")
        .args([command, device])
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| device.trim_start_matches('@').to_string())
}

fn audio_spec(input: bool, source: &str) -> (usize, u32) {
    let device = if source.starts_with('@') || source.is_empty() {
        if input {
            "@DEFAULT_SOURCE@"
        } else {
            "@DEFAULT_SINK@"
        }
    } else {
        source
    };
    let command = if input {
        "get-source-sample-specification"
    } else {
        "get-sink-sample-specification"
    };
    let specification = Command::new("pactl")
        .args([command, device])
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default();
    let mut channels = 2usize;
    let mut rate = 48_000u32;
    for token in specification.split_whitespace() {
        if let Some(value) = token.strip_suffix("ch") {
            if let Ok(value) = value.parse::<usize>() {
                channels = value.clamp(1, 8);
            }
        } else if let Some(value) = token.strip_suffix("Hz") {
            if let Ok(value) = value.parse::<u32>() {
                rate = value.clamp(8_000, 192_000);
            }
        }
    }
    (channels, rate)
}

impl Drop for AudioSampler {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AudioChannel {
    Index(usize),
    Sum,
}

impl RuntimeMeasure for AudioLevelMeasure {
    fn update(&mut self, context: &RuntimeContext<'_>) -> RuntimeOutput {
        let kind = context.option_or("Type", "RMS").to_ascii_lowercase();
        let input = context
            .option_or("Port", "Output")
            .eq_ignore_ascii_case("Input");
        let parent_name = context.option_or("Parent", "");
        let is_child = !parent_name.trim().is_empty();

        if !is_child {
            let restart = match self.sampler.as_mut() {
                Some(sampler) => !sampler.is_running(),
                None => true,
            };
            if restart {
                self.sampler = AudioSampler::start(context);
            }
        }

        let snapshot = if is_child {
            AUDIO_PARENTS
                .lock()
                .ok()
                .and_then(|parents| parents.get(&parent_name.to_ascii_lowercase()).cloned())
        } else {
            self.sampler.as_ref().map(|sampler| AudioSnapshot {
                samples: sampler.snapshot(),
                channels: sampler.channels,
                sample_rate: sampler.sample_rate,
                source: sampler.source.clone(),
            })
        };

        if kind == "deviceid" || kind == "devicename" || kind == "devicelist" {
            return match kind.as_str() {
                "deviceid" => RuntimeOutput::text(
                    snapshot.map(|snapshot| snapshot.source).unwrap_or_else(|| {
                        context.option_or(
                            "ID",
                            if input {
                                "@DEFAULT_SOURCE@"
                            } else {
                                "@DEFAULT_SINK@"
                            },
                        )
                    }),
                ),
                "devicename" => RuntimeOutput::text(
                    snapshot
                        .map(|snapshot| audio_device_name(input, &snapshot.source))
                        .unwrap_or_else(|| context.option_or("ID", "default")),
                ),
                _ => RuntimeOutput::text(AudioSampler::device_list(input)),
            };
        }

        let Some(snapshot) = snapshot else {
            return RuntimeOutput::number(0.0, Some(0.0), Some(1.0));
        };
        if !is_child {
            if let Ok(mut parents) = AUDIO_PARENTS.lock() {
                parents.insert(context.measure_name.to_ascii_lowercase(), snapshot.clone());
            }
        }
        let samples = snapshot.samples;
        let channels = snapshot.channels;
        let sample_rate = snapshot.sample_rate;
        if kind == "format" {
            return RuntimeOutput::text(format!("{}Hz PCM 32b {}ch", sample_rate, channels));
        }
        if kind == "devicestatus" {
            return RuntimeOutput::number(1.0, Some(0.0), Some(1.0));
        }
        let elapsed_ms = self
            .last_update
            .map(|last| last.elapsed().as_secs_f64() * 1000.0)
            .unwrap_or(1000.0);
        self.last_update = Some(Instant::now());
        let channel = parse_audio_channel(&context.option_or("Channel", "Sum"));
        let selected = select_audio_channel(&samples, channels, channel);
        let rms_gain = context.number_option("RMSGain", 1.0).max(0.0);
        let peak_gain = context.number_option("PeakGain", 1.0).max(0.0);
        let rms = (audio_rms(&selected) * rms_gain).clamp(0.0, 1.0);
        let peak = (audio_peak(&selected) * peak_gain).clamp(0.0, 1.0);
        let sensitivity = context.number_option("Sensitivity", 35.0).max(1.0);
        let size = context.number_option("FFTSize", 2048.0).max(16.0) as usize;
        let fft_index = context.number_option("FFTIdx", 0.0).max(0.0) as usize;
        let overlap = context.number_option("FFTOverlap", 0.0);
        let magnitudes = audio_fft_overlap(&selected, size, overlap);
        let fft_gain = context.number_option("FFTGain", 1.0).max(0.0);
        let fft_raw = (audio_db_normalize(
            magnitudes
                .get(fft_index.min(magnitudes.len().saturating_sub(1)))
                .copied()
                .unwrap_or(0.0),
            sensitivity,
        ) * fft_gain)
            .clamp(0.0, 1.0);
        let (band_raw, band_frequency) = audio_band(context, &selected, sample_rate as f64);
        match kind.as_str() {
            "rms" => RuntimeOutput::number(
                smooth_audio(
                    &mut self.rms_envelope,
                    rms,
                    context.number_option("RMSAttack", 300.0),
                    context.number_option("RMSDecay", 300.0),
                    elapsed_ms,
                ),
                Some(0.0),
                Some(1.0),
            ),
            "peak" => RuntimeOutput::number(
                smooth_audio(
                    &mut self.peak_envelope,
                    peak,
                    context.number_option("PeakAttack", 50.0),
                    context.number_option("PeakDecay", 2500.0),
                    elapsed_ms,
                ),
                Some(0.0),
                Some(1.0),
            ),
            "fft" => RuntimeOutput::number(
                smooth_audio(
                    &mut self.fft_envelope,
                    fft_raw,
                    context.number_option("FFTAttack", 300.0),
                    context.number_option("FFTDecay", 300.0),
                    elapsed_ms,
                ),
                Some(0.0),
                Some(1.0),
            ),
            "band" => RuntimeOutput::number(
                smooth_audio(
                    &mut self.band_envelope,
                    band_raw,
                    context.number_option("FFTAttack", 300.0),
                    context.number_option("FFTDecay", 300.0),
                    elapsed_ms,
                ),
                Some(0.0),
                Some(1.0),
            ),
            "fftfreq" => {
                let size = size.next_power_of_two().clamp(2, 16_384);
                let frequency = fft_index.min(size / 2) as f64 * sample_rate as f64 / size as f64;
                RuntimeOutput::number(frequency, None, None)
            }
            "bandfreq" => RuntimeOutput::number(band_frequency, None, None),
            _ => RuntimeOutput::number(rms, Some(0.0), Some(1.0)),
        }
    }
}

fn smooth_audio(current: &mut f64, target: f64, attack_ms: f64, decay_ms: f64, dt_ms: f64) -> f64 {
    let milliseconds = if target > *current {
        attack_ms.max(0.0)
    } else {
        decay_ms.max(0.0)
    };
    if milliseconds <= 0.0 || dt_ms <= 0.0 {
        *current = target.clamp(0.0, 1.0);
        return *current;
    }
    let alpha = 0.01f64.powf(dt_ms / milliseconds);
    *current = (target + alpha * (*current - target)).clamp(0.0, 1.0);
    *current
}

fn parse_audio_channel(name: &str) -> AudioChannel {
    match name.trim().to_ascii_uppercase().as_str() {
        "SUM" | "AVG" | "" => AudioChannel::Sum,
        "L" | "FL" | "0" => AudioChannel::Index(0),
        "R" | "FR" | "1" => AudioChannel::Index(1),
        "C" | "2" => AudioChannel::Index(2),
        "LFE" | "SUB" | "3" => AudioChannel::Index(3),
        "BL" | "4" => AudioChannel::Index(4),
        "BR" | "5" => AudioChannel::Index(5),
        "SL" | "6" => AudioChannel::Index(6),
        "SR" | "7" => AudioChannel::Index(7),
        _ => name
            .trim()
            .parse::<usize>()
            .map(AudioChannel::Index)
            .unwrap_or(AudioChannel::Sum),
    }
}

fn select_audio_channel(samples: &[f32], channels: usize, channel: AudioChannel) -> Vec<f32> {
    if channels == 0 {
        return Vec::new();
    }
    match channel {
        AudioChannel::Index(index) if index < channels => samples
            .chunks_exact(channels)
            .map(|frame| frame[index])
            .collect(),
        AudioChannel::Index(_) => Vec::new(),
        AudioChannel::Sum => samples
            .chunks_exact(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect(),
    }
}

fn audio_rms(samples: &[f32]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum = samples
        .iter()
        .map(|sample| f64::from(*sample).powi(2))
        .sum::<f64>();
    (sum / samples.len() as f64).sqrt()
}

fn audio_peak(samples: &[f32]) -> f64 {
    samples
        .iter()
        .map(|sample| f64::from(*sample).abs())
        .fold(0.0, f64::max)
}

fn audio_db_normalize(magnitude: f64, sensitivity: f64) -> f64 {
    if magnitude <= 0.0 {
        return 0.0;
    }
    let decibels = 20.0 * magnitude.log10();
    ((decibels + sensitivity) / sensitivity).clamp(0.0, 1.0)
}

fn audio_fft_overlap(samples: &[f32], size: usize, overlap: f64) -> Vec<f64> {
    let size = size.next_power_of_two().clamp(2, 16_384);
    let overlap = overlap.clamp(0.0, 90.0);
    if overlap <= 0.0 || samples.len() < size {
        return audio_fft(samples, size);
    }
    let step = (size as f64 * (100.0 - overlap) / 100.0).round() as usize;
    let step = step.max(1);
    let mut starts = Vec::new();
    let mut start = samples.len().saturating_sub(size);
    loop {
        starts.push(start);
        if starts.len() >= 8 || start < step {
            break;
        }
        start -= step;
    }
    let mut output = vec![0.0f64; size / 2];
    let count = starts.len() as f64;
    for start in starts {
        let magnitudes = audio_fft(&samples[start..start + size], size);
        for (value, magnitude) in output.iter_mut().zip(magnitudes) {
            *value += magnitude;
        }
    }
    for value in &mut output {
        *value /= count;
    }
    output
}

fn audio_fft(samples: &[f32], size: usize) -> Vec<f64> {
    let size = size.next_power_of_two().clamp(2, 16_384);
    let mut real = vec![0.0f64; size];
    let mut imaginary = vec![0.0f64; size];
    let count = samples.len().min(size);
    let offset = samples.len().saturating_sub(size);
    for (index, sample) in samples[offset..offset + count].iter().enumerate() {
        let window = 0.5 - 0.5 * (std::f64::consts::TAU * index as f64 / size as f64).cos();
        real[index] = f64::from(*sample) * window;
    }
    let mut target = 0usize;
    for source in 0..size {
        if source > 0 {
            let mut mask = size >> 1;
            while target & mask != 0 {
                target ^= mask;
                mask >>= 1;
            }
            target ^= mask;
        }
        if source < target {
            real.swap(source, target);
            imaginary.swap(source, target);
        }
    }
    let mut length = 2usize;
    while length <= size {
        let angle = -std::f64::consts::TAU / length as f64;
        let step_real = angle.cos();
        let step_imaginary = angle.sin();
        for start in (0..size).step_by(length) {
            let mut w_real = 1.0;
            let mut w_imaginary = 0.0;
            for index in 0..length / 2 {
                let even = start + index;
                let odd = even + length / 2;
                let odd_real = real[odd] * w_real - imaginary[odd] * w_imaginary;
                let odd_imaginary = real[odd] * w_imaginary + imaginary[odd] * w_real;
                real[odd] = real[even] - odd_real;
                imaginary[odd] = imaginary[even] - odd_imaginary;
                real[even] += odd_real;
                imaginary[even] += odd_imaginary;
                let next_real = w_real * step_real - w_imaginary * step_imaginary;
                w_imaginary = w_real * step_imaginary + w_imaginary * step_real;
                w_real = next_real;
            }
        }
        length <<= 1;
    }
    (0..size / 2)
        .map(|index| real[index].hypot(imaginary[index]) * 2.0 / size as f64)
        .collect()
}

fn audio_band(context: &RuntimeContext<'_>, samples: &[f32], sample_rate: f64) -> (f64, f64) {
    let size = context.number_option("FFTSize", 2048.0).max(16.0) as usize;
    let bands = context.number_option("Bands", 10.0).max(1.0) as usize;
    let index = context
        .number_option("BandIdx", 0.0)
        .max(0.0)
        .min(bands.saturating_sub(1) as f64) as usize;
    let min = context.number_option("FreqMin", 20.0).max(0.0);
    let max = context.number_option("FreqMax", 20_000.0).max(min + 1.0);
    let ratio = (max / min.max(1.0)).powf(1.0 / bands as f64);
    let low = min.max(1.0) * ratio.powi(index as i32);
    let high = low * ratio;
    let magnitudes = audio_fft(samples, size);
    let size = size.next_power_of_two().clamp(2, 16_384);
    let start = ((low * size as f64 / sample_rate).floor() as usize).min(magnitudes.len());
    let end = ((high * size as f64 / sample_rate).ceil() as usize)
        .max(start + 1)
        .min(magnitudes.len());
    let value = magnitudes[start..end].iter().copied().fold(0.0, f64::max)
        * context.number_option("FFTGain", 1.0).max(0.0);
    let sensitivity = context.number_option("Sensitivity", 35.0).max(1.0);
    (audio_db_normalize(value, sensitivity), low)
}

struct AudioMeasure;

impl RuntimeMeasure for AudioMeasure {
    fn update(&mut self, _context: &RuntimeContext<'_>) -> RuntimeOutput {
        let volume = read_volume();
        RuntimeOutput::number(volume, Some(0.0), Some(100.0))
    }

    fn command(&mut self, command: &str, _context: &RuntimeContext<'_>) -> bool {
        let mut parts = command.split_whitespace();
        let action = parts.next().unwrap_or_default().to_ascii_lowercase();
        let argument = parts.next().unwrap_or_default();
        match action.as_str() {
            "setvolume" => set_volume(argument.parse().unwrap_or(0.0)),
            "changevolume" => {
                let current = read_volume();
                let delta = argument.parse::<f64>().unwrap_or(0.0);
                set_volume((current + delta).clamp(0.0, 100.0))
            }
            "togglemute" => set_mute("toggle"),
            "mute" => set_mute("1"),
            "unmute" => set_mute("0"),
            "setoutputindex" => set_output_index(argument),
            "togglenext" => cycle_output(1),
            "toggleprevious" => cycle_output(-1),
            _ => false,
        }
    }
}

fn read_volume() -> f64 {
    if let Ok(output) = Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .output()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        if let Some(value) = text.split("Volume:").nth(1) {
            if let Ok(volume) = value
                .split_whitespace()
                .next()
                .unwrap_or("0")
                .parse::<f64>()
            {
                return (volume * 100.0).clamp(0.0, 100.0);
            }
        }
    }
    if let Ok(output) = Command::new("pactl")
        .args(["get-sink-volume", "@DEFAULT_SINK@"])
        .output()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        if let Some(value) = text.split('%').next().and_then(|value| {
            value
                .split_whitespace()
                .rev()
                .find_map(|part| part.parse::<f64>().ok())
        }) {
            return value.clamp(0.0, 100.0);
        }
    }
    0.0
}

fn set_volume(volume: f64) -> bool {
    let volume = volume.clamp(0.0, 100.0);
    let value = format!("{volume}%");
    run_tool("wpctl", &["set-volume", "@DEFAULT_AUDIO_SINK@", &value])
        || run_tool("pactl", &["set-sink-volume", "@DEFAULT_SINK@", &value])
}

fn set_mute(state: &str) -> bool {
    run_tool("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", state])
        || run_tool("pactl", &["set-sink-mute", "@DEFAULT_SINK@", state])
}

fn run_tool(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn output_devices() -> Vec<(String, String)> {
    Command::new("pactl")
        .args(["list", "short", "sinks"])
        .output()
        .ok()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(|line| {
                    let mut fields = line.split('\t');
                    let index = fields.next()?.trim().to_string();
                    let name = fields.next()?.trim().to_string();
                    (!name.is_empty()).then_some((index, name))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn default_output() -> Option<String> {
    Command::new("pactl")
        .args(["get-default-sink"])
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|name| !name.is_empty())
}

fn set_output(name: &str) -> bool {
    run_tool("pactl", &["set-default-sink", name])
}

fn set_output_index(argument: &str) -> bool {
    let devices = output_devices();
    if devices.is_empty() {
        return false;
    }
    if let Ok(index) = argument.trim().parse::<usize>() {
        let position = if index == 0 {
            0
        } else if index <= devices.len() {
            index - 1
        } else {
            index
        };
        return devices
            .get(position)
            .map(|(_, name)| set_output(name))
            .unwrap_or(false);
    }
    devices
        .iter()
        .find(|(_, name)| name.eq_ignore_ascii_case(argument.trim()))
        .map(|(_, name)| set_output(name))
        .unwrap_or(false)
}

fn cycle_output(delta: isize) -> bool {
    let devices = output_devices();
    if devices.is_empty() {
        return false;
    }
    let current = default_output()
        .and_then(|current| {
            devices
                .iter()
                .position(|(_, name)| name == &current || name.ends_with(&current))
        })
        .unwrap_or(0);
    let length = devices.len() as isize;
    let position = (current as isize + delta).rem_euclid(length) as usize;
    devices
        .get(position)
        .map(|(_, name)| set_output(name))
        .unwrap_or(false)
}

#[derive(Clone)]
struct FileEntry {
    path: PathBuf,
    name: String,
    is_dir: bool,
    size: u64,
    extension: String,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    accessed: Option<SystemTime>,
}

#[derive(Default)]
struct FileViewMeasure {
    entries: Vec<FileEntry>,
    index: usize,
    index_offset: usize,
    loaded_at: Option<u64>,
    folder: String,
    pending_actions: Vec<String>,
    folder_size: u64,
    file_count: u64,
    folder_count: u64,
    icon_cache: BTreeMap<(String, usize), String>,
}

impl RuntimeMeasure for FileViewMeasure {
    fn update(&mut self, context: &RuntimeContext<'_>) -> RuntimeOutput {
        let counter = context.counter as u64;
        let update_rate = context.number_option("UpdateRate", 1.0).max(1.0) as u64;
        if self
            .loaded_at
            .is_none_or(|last| counter.saturating_sub(last) >= update_rate)
        {
            self.loaded_at = Some(counter);
            self.refresh(context);
        }
        if self.entries.is_empty() {
            return RuntimeOutput::empty();
        }

        let count = context.number_option("Count", 1.0).max(1.0) as usize;
        let ignore_count = context.bool_option("IgnoreCount", false);
        let true_index = if ignore_count {
            self.index
        } else {
            self.index % count + self.index_offset
        };
        let Some(entry) = self.entries.get(true_index).cloned() else {
            return RuntimeOutput::empty();
        };

        let kind = context.option_or("Type", "FolderPath").to_ascii_lowercase();
        let date_type = context.option_or("DateType", "Modified");
        let display_name = if context.bool_option("HideExtensions", false) && !entry.is_dir {
            Path::new(&entry.name)
                .file_stem()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|| entry.name.clone())
        } else {
            entry.name.clone()
        };
        let (number, string) = match kind.as_str() {
            "foldersize" => (self.folder_size as f64, None),
            "filecount" => (self.file_count as f64, None),
            "foldercount" => (self.folder_count as f64, None),
            "filename" => ((true_index + 1) as f64, Some(display_name)),
            "filetype" => ((true_index + 1) as f64, Some(entry.extension.clone())),
            "filesize" => (entry.size as f64, None),
            "filedate" => {
                let time = entry_time(&entry, &date_type);
                (
                    time.map(epoch_seconds).unwrap_or(0.0),
                    time.map(format_time),
                )
            }
            "filepath" => (0.0, Some(entry.path.to_string_lossy().to_string())),
            "pathtofile" => (
                0.0,
                Some(
                    entry
                        .path
                        .parent()
                        .map(|path| path.to_string_lossy().to_string())
                        .unwrap_or_default(),
                ),
            ),
            "icon" => {
                let size = context.option_or("IconSize", "MEDIUM").to_ascii_lowercase();
                let size = match size.as_str() {
                    "small" => 16,
                    "large" => 48,
                    "extralarge" => 256,
                    _ => 32,
                };
                let key = (entry.path.to_string_lossy().to_string(), size);
                let path = self
                    .icon_cache
                    .entry(key)
                    .or_insert_with(|| {
                        icon_file(&entry.path, size)
                            .map(|path| {
                                if let Some(destination) = context.option("IconPath") {
                                    let destination = PathBuf::from(destination);
                                    if !destination.as_os_str().is_empty() {
                                        if let Some(parent) = destination.parent() {
                                            let _ = fs::create_dir_all(parent);
                                        }
                                        if fs::copy(&path, &destination).is_ok() {
                                            return destination.to_string_lossy().to_string();
                                        }
                                    }
                                }
                                path.to_string_lossy().to_string()
                            })
                            .unwrap_or_else(|| context.option_or("IconPath", ""))
                    })
                    .clone();
                (0.0, Some(path))
            }
            _ => (0.0, Some(self.path(context).to_string_lossy().to_string())),
        };

        RuntimeOutput {
            value: MeasureValue {
                number,
                string,
                min: Some(1.0),
                max: Some(self.entries.len() as f64),
            },
            actions: std::mem::take(&mut self.pending_actions),
        }
    }

    fn command(&mut self, command: &str, context: &RuntimeContext<'_>) -> bool {
        let count = context.number_option("Count", 1.0).max(1.0) as usize;
        let length = self.entries.len();
        match command.trim().to_ascii_lowercase().as_str() {
            "pageup" => self.index_offset = self.index_offset.saturating_sub(count),
            "pagedown" => {
                self.index_offset = (self.index_offset + count).min(length.saturating_sub(count));
            }
            "indexup" => {
                if self.index == 0 && self.index_offset >= count {
                    self.index_offset -= count;
                    self.index = count - 1;
                } else {
                    self.index = self.index.saturating_sub(1);
                }
            }
            "indexdown" => {
                if self.index + 1 >= count && self.index_offset + count < length {
                    self.index_offset += count;
                    self.index = 0;
                } else {
                    self.index = (self.index + 1) % count;
                }
            }
            "update" => {
                self.refresh(context);
            }
            "previousfolder" => {
                let path = self.path(context);
                if let Some(parent) = path.parent() {
                    self.folder = parent.to_string_lossy().to_string();
                    self.refresh(context);
                }
            }
            "open" => {
                if let Some(entry) = self.entries.get(self.true_index(context)) {
                    let _ = Command::new("xdg-open").arg(&entry.path).spawn();
                }
            }
            "followpath" => {
                if let Some(entry) = self.entries.get(self.true_index(context)) {
                    if entry.is_dir {
                        self.folder = entry.path.to_string_lossy().to_string();
                        self.refresh(context);
                    }
                }
            }
            "contextmenu" => {
                let path = self.path(context);
                let (target, is_dir) = self
                    .entries
                    .get(self.true_index(context))
                    .map(|entry| (entry.path.clone(), entry.is_dir))
                    .unwrap_or((path, true));
                show_context_menu(target, is_dir);
            }
            "properties" => {
                if let Some(entry) = self.entries.get(self.true_index(context)) {
                    show_properties(&entry.path);
                }
            }
            _ => return false,
        }
        true
    }
}

impl FileViewMeasure {
    fn path(&self, context: &RuntimeContext<'_>) -> PathBuf {
        if self.folder.is_empty() {
            context.path_option("Path", ".")
        } else {
            PathBuf::from(&self.folder)
        }
    }

    fn true_index(&self, context: &RuntimeContext<'_>) -> usize {
        if context.bool_option("IgnoreCount", false) {
            self.index
        } else {
            let count = context.number_option("Count", 1.0).max(1.0) as usize;
            self.index % count + self.index_offset
        }
    }

    fn refresh(&mut self, context: &RuntimeContext<'_>) {
        let path = self.path(context);
        self.folder = path.to_string_lossy().to_string();
        let mut entries = Vec::new();
        let show_file = context.bool_option("ShowFile", true);
        let show_folder = context.bool_option("ShowFolder", true);
        let show_dotdot = context.bool_option("ShowDotDot", true);
        let show_hidden = context.bool_option("ShowHidden", true);
        let show_system = context.bool_option("ShowSystem", false);
        let recursive = context.number_option("Recursive", 0.0) > 0.0;
        let extensions: Vec<String> = context
            .option("Extensions")
            .unwrap_or_default()
            .split([';', '|', ','])
            .map(|value| value.trim().trim_start_matches('*').to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .collect();
        let wildcard = context.option_or("WildcardSearch", "*");
        if show_dotdot {
            if let Some(parent) = path.parent() {
                entries.push(FileEntry {
                    path: parent.to_path_buf(),
                    name: "..".to_string(),
                    is_dir: true,
                    size: 0,
                    extension: String::new(),
                    modified: None,
                    created: None,
                    accessed: None,
                });
            }
        }
        collect_files(
            &path,
            recursive,
            show_file,
            show_folder,
            show_hidden,
            show_system,
            &extensions,
            &wildcard,
            &mut entries,
            0,
        );

        let sort_type = context.option_or("SortType", "Name").to_ascii_lowercase();
        let sort_date_type = context.option_or("SortDateType", "Modified");
        let ascending = context.bool_option("SortAscending", true);
        entries.sort_by(|left, right| {
            let order = match sort_type.as_str() {
                "size" => left.size.cmp(&right.size),
                "type" => left
                    .extension
                    .cmp(&right.extension)
                    .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase())),
                "date" => {
                    entry_time(left, &sort_date_type).cmp(&entry_time(right, &sort_date_type))
                }
                _ => left
                    .name
                    .to_lowercase()
                    .cmp(&right.name.to_lowercase())
                    .then_with(|| left.name.cmp(&right.name)),
            };
            if ascending {
                order
            } else {
                order.reverse()
            }
        });

        self.file_count = entries.iter().filter(|entry| !entry.is_dir).count() as u64;
        self.folder_count = entries.iter().filter(|entry| entry.is_dir).count() as u64;
        self.folder_size = entries.iter().map(|entry| entry.size).sum();
        self.entries = entries;
        self.index = (context.number_option("Index", 1.0).max(1.0) - 1.0) as usize;
        self.index_offset = 0;
        self.pending_actions.clear();
        let finish_action = context.option_or("FinishAction", "");
        if !finish_action.is_empty() {
            self.pending_actions.push(finish_action);
        }
    }
}

type IconLookupCache = Mutex<BTreeMap<(String, usize), Option<PathBuf>>>;

static ICON_LOOKUP_CACHE: std::sync::LazyLock<IconLookupCache> =
    std::sync::LazyLock::new(|| Mutex::new(BTreeMap::new()));

fn icon_file(path: &Path, size: usize) -> Option<PathBuf> {
    let mut candidates = icon_candidates(path);
    if candidates.is_empty() {
        candidates.push("text-x-generic".to_string());
    }
    if path.is_dir() {
        candidates.insert(0, "folder".to_string());
        candidates.push("inode-directory".to_string());
    }
    candidates.push("application-x-executable".to_string());
    candidates.push("text-x-generic".to_string());
    candidates.dedup();
    let key = (candidates.join("|"), size);
    if let Some(cached) = ICON_LOOKUP_CACHE
        .lock()
        .ok()
        .and_then(|cache| cache.get(&key).cloned())
    {
        return cached;
    }

    let mut bases = vec![
        PathBuf::from("/usr/share/icons"),
        PathBuf::from("/usr/local/share/icons"),
        PathBuf::from("/usr/share/pixmaps"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        bases.push(home.join(".local/share/icons"));
        bases.push(home.join(".icons"));
    }
    let themes = icon_theme_dirs(&bases);
    let mut found = None;
    'outer: for theme in &themes {
        for icon in &candidates {
            if let Some(path) = find_icon_in_theme(theme, icon, size) {
                found = Some(path);
                break 'outer;
            }
        }
    }
    if found.is_none() {
        for base in &bases {
            for icon in &candidates {
                if let Some(path) = find_icon_recursive(base, icon, 0) {
                    found = Some(path);
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }
    }
    if let Some(cache) = ICON_LOOKUP_CACHE.lock().ok().as_deref_mut() {
        cache.insert(key, found.clone());
    }
    found
}

fn icon_candidates(path: &Path) -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(output) = Command::new("gio")
        .args(["info", "-a", "standard::icon", "--"])
        .arg(path)
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            if let Some((_, value)) = text
                .lines()
                .find_map(|line| line.split_once("standard::icon:"))
            {
                for name in value.split(',') {
                    let name = name
                        .trim()
                        .trim_matches('\'')
                        .trim_matches('"')
                        .trim_end_matches("-symbolic")
                        .to_string();
                    if !name.is_empty() && !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
        }
    }
    if let Ok(output) = Command::new("xdg-mime")
        .args(["query", "filetype"])
        .arg(path)
        .output()
    {
        if output.status.success() {
            let mime = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !mime.is_empty() {
                let name = mime.replace('/', "-");
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
    }
    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        let lower = extension.to_ascii_lowercase();
        let name = format!("text-x-{lower}");
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

fn icon_theme_dirs(bases: &[PathBuf]) -> Vec<PathBuf> {
    let mut preferred = Vec::new();
    if let Ok(theme) = std::env::var("GTK_ICON_THEME") {
        preferred.push(theme);
    }
    if let Ok(output) = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "icon-theme"])
        .output()
    {
        if output.status.success() {
            let theme = String::from_utf8_lossy(&output.stdout)
                .trim()
                .trim_matches('\'')
                .trim_matches('"')
                .to_string();
            if !theme.is_empty() && !preferred.contains(&theme) {
                preferred.push(theme);
            }
        }
    }
    for name in [
        "hicolor",
        "Adwaita",
        "Yaru",
        "Papirus",
        "Papirus-Dark",
        "breeze",
        "breeze-dark",
    ] {
        if !preferred.iter().any(|value| value == name) {
            preferred.push(name.to_string());
        }
    }

    let mut themes = Vec::new();
    for name in &preferred {
        for base in bases {
            let path = base.join(name);
            if path.is_dir() && !themes.contains(&path) {
                themes.push(path);
            }
        }
    }
    for base in bases {
        if let Ok(entries) = fs::read_dir(base) {
            let mut others: Vec<PathBuf> = entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_dir() && !themes.contains(path))
                .collect();
            others.sort();
            themes.extend(others);
        }
    }
    themes
}

fn find_icon_in_theme(theme: &Path, icon: &str, size: usize) -> Option<PathBuf> {
    let mut sizes = Vec::new();
    if let Ok(entries) = fs::read_dir(theme) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let pixels = name
                .trim_end_matches('x')
                .split_once('x')
                .and_then(|(value, _)| value.parse::<usize>().ok())
                .unwrap_or(if name == "scalable" {
                    usize::MAX / 2
                } else {
                    0
                });
            if pixels > 0 {
                sizes.push((pixels, path));
            }
        }
    }
    sizes.sort_by_key(|(pixels, _)| {
        if *pixels >= usize::MAX / 4 {
            size + 1
        } else {
            pixels.abs_diff(size)
        }
    });
    for (_, directory) in sizes {
        let Ok(categories) = fs::read_dir(&directory) else {
            continue;
        };
        for category in categories.flatten() {
            let category = category.path();
            if !category.is_dir() {
                continue;
            }
            for extension in ["png", "svg", "xpm"] {
                let candidate = category.join(format!("{icon}.{extension}"));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

fn show_context_menu(path: PathBuf, is_dir: bool) {
    std::thread::spawn(move || {
        let parent = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| path.clone());
        let items = context_menu_items(is_dir);
        let selected = context_menu_selection(&path, &items)?;
        match selected.as_str() {
            "Ouvrir" => {
                let _ = Command::new("xdg-open").arg(&path).spawn();
            }
            "Ouvrir avec…" => {
                let opened = Command::new("mimeopen")
                    .args(["-d", "--"])
                    .arg(&path)
                    .spawn()
                    .is_ok();
                if !opened {
                    let _ = Command::new("gio").arg("open").arg(&path).spawn();
                }
            }
            "Copier le chemin" => copy_to_clipboard(&path.to_string_lossy()),
            "Ouvrir le dossier parent" => {
                let _ = Command::new("xdg-open").arg(parent).spawn();
            }
            "Propriétés" => show_properties(&path),
            _ => {}
        }
        Some(())
    });
}

fn context_menu_items(is_dir: bool) -> Vec<&'static str> {
    let mut items = vec!["Ouvrir", "Ouvrir avec…", "Copier le chemin"];
    if is_dir {
        items.push("Ouvrir le dossier parent");
    }
    items.push("Propriétés");
    items.push("Annuler");
    items
}

fn context_menu_selection(path: &Path, items: &[&str]) -> Option<String> {
    let title = path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    let yad = Command::new("yad")
        .args([
            "--list",
            "--no-headers",
            "--print-column=1",
            &format!("--title={title}"),
            &format!("--text={}", path.to_string_lossy()),
            "--column=Action",
        ])
        .args(items)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    if let Ok(output) = yad {
        if output.status.success() {
            let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
            return (!value.is_empty()).then_some(value);
        }
        return None;
    }
    let mut zenity = Command::new("zenity");
    zenity
        .args([
            "--list",
            "--hide-header",
            &format!("--title={title}"),
            &format!("--text={}", path.to_string_lossy()),
            "--column=Action",
        ])
        .args(items)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
}

fn copy_to_clipboard(value: &str) {
    for (program, args) in [
        ("wl-copy", Vec::<&str>::new()),
        ("xclip", vec!["-selection", "clipboard"]),
        ("xsel", vec!["--clipboard", "--input"]),
    ] {
        let mut child = match Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(_) => continue,
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(value.as_bytes());
        }
        let _ = child.wait();
        return;
    }
}

fn find_icon_recursive(directory: &Path, icon: &str, depth: usize) -> Option<PathBuf> {
    if depth > 6 {
        return None;
    }
    let entries = fs::read_dir(directory).ok()?;
    let mut directories = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            directories.push(path);
        } else {
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            let matches = ["png", "svg", "xpm"]
                .iter()
                .any(|extension| name.eq_ignore_ascii_case(&format!("{icon}.{extension}")));
            if matches {
                return Some(path);
            }
        }
    }
    for directory in directories {
        if let Some(path) = find_icon_recursive(&directory, icon, depth + 1) {
            return Some(path);
        }
    }
    None
}

fn show_properties(path: &Path) {
    let metadata = fs::metadata(path).ok();
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_default();
    let size = metadata
        .as_ref()
        .map(|metadata| human_size(metadata.len()))
        .unwrap_or_else(|| "?".to_string());
    let modified = metadata
        .and_then(|metadata| metadata.modified().ok())
        .map(format_time)
        .unwrap_or_else(|| "?".to_string());
    let kind = if path.is_dir() { "Dossier" } else { "Fichier" };
    let text = format!(
        "Nom : {name}\nType : {kind}\nTaille : {size}\nModifié : {modified}\nChemin : {}",
        path.to_string_lossy()
    );
    std::thread::spawn(move || {
        let zenity = Command::new("zenity")
            .args(["--info", "--title=Propriétés", &format!("--text={text}")])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        if zenity.is_ok() {
            return;
        }
        let kdialog = Command::new("kdialog")
            .args(["--title", "Propriétés", "--msgbox", &text])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        if kdialog.is_ok() {
            return;
        }
        let _ = Command::new("xmessage")
            .arg(&text)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    });
}

fn entry_time(entry: &FileEntry, kind: &str) -> Option<SystemTime> {
    match kind.to_ascii_lowercase().as_str() {
        "created" => entry.created,
        "accessed" => entry.accessed,
        _ => entry.modified,
    }
}

fn epoch_seconds(time: SystemTime) -> f64 {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as f64)
        .unwrap_or(0.0)
}

fn format_time(time: SystemTime) -> String {
    use chrono::{DateTime, Local};
    let datetime: DateTime<Local> = time.into();
    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn human_size(size: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = size as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", size, UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_files(
    path: &Path,
    recursive: bool,
    show_file: bool,
    show_folder: bool,
    show_hidden: bool,
    show_system: bool,
    extensions: &[String],
    wildcard: &str,
    output: &mut Vec<FileEntry>,
    depth: usize,
) {
    if depth > 16 {
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_dir = path.is_dir();
        if (is_dir && !show_folder) || (!is_dir && !show_file) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !show_hidden && name.starts_with('.') && name != ".." {
            continue;
        }
        if !show_system && is_system_entry(&name) {
            continue;
        }
        if !matches_wildcard(&name, wildcard) {
            continue;
        }
        let extension = path
            .extension()
            .map(|value| format!(".{}", value.to_string_lossy().to_ascii_lowercase()))
            .unwrap_or_default();
        if !is_dir && !extensions.is_empty() && !extensions.iter().any(|value| value == &extension)
        {
            continue;
        }
        let metadata = entry.metadata().ok();
        output.push(FileEntry {
            path: path.clone(),
            name,
            is_dir,
            size: if is_dir {
                0
            } else {
                metadata.as_ref().map(|meta| meta.len()).unwrap_or(0)
            },
            extension,
            modified: metadata.as_ref().and_then(|meta| meta.modified().ok()),
            created: metadata.as_ref().and_then(|meta| meta.created().ok()),
            accessed: metadata.as_ref().and_then(|meta| meta.accessed().ok()),
        });
        if recursive && is_dir {
            collect_files(
                &path,
                recursive,
                show_file,
                show_folder,
                show_hidden,
                show_system,
                extensions,
                wildcard,
                output,
                depth + 1,
            );
        }
    }
}

fn is_system_entry(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "lost+found" | "$recycle.bin" | "system volume information"
    )
}

fn matches_wildcard(name: &str, pattern: &str) -> bool {
    if pattern.is_empty() || pattern == "*" {
        return true;
    }
    regex::Regex::new(&format!(
        "^{}$",
        regex::escape(pattern)
            .replace("\\*", ".*")
            .replace("\\?", ".")
    ))
    .map(|regex| regex.is_match(name))
    .unwrap_or(true)
}

struct FolderInfoMeasure;

impl RuntimeMeasure for FolderInfoMeasure {
    fn update(&mut self, context: &RuntimeContext<'_>) -> RuntimeOutput {
        let folder = context.path_option("Folder", ".");
        let recursive = context.bool_option("IncludeSubFolders", false);
        let include_hidden = context.bool_option("IncludeHiddenFiles", false);
        let include_system = context.bool_option("IncludeSystemFiles", false);
        let filter = context.option("RegExpFilter").unwrap_or_default();
        let regex = (!filter.is_empty())
            .then(|| regex::Regex::new(&filter).ok())
            .flatten();
        let mut files = 0u64;
        let mut folders = 0u64;
        let mut size = 0u64;
        scan_folder(
            &folder,
            recursive,
            include_hidden,
            include_system,
            regex.as_ref(),
            0,
            &mut files,
            &mut folders,
            &mut size,
        );
        let info_type = context.option_or("InfoType", "FolderSize");
        let human = info_type.to_ascii_lowercase().ends_with("str");
        let (number, text) = match info_type.to_ascii_lowercase().as_str() {
            "foldercount" | "foldercountstr" => (folders as f64, Some(folders.to_string())),
            "filecount" | "filecountstr" => (files as f64, Some(files.to_string())),
            _ => (size as f64, Some(human_size(size))),
        };
        RuntimeOutput {
            value: MeasureValue {
                number,
                string: if human { text } else { None },
                min: None,
                max: None,
            },
            actions: Vec::new(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn scan_folder(
    path: &Path,
    recursive: bool,
    include_hidden: bool,
    include_system: bool,
    regex: Option<&regex::Regex>,
    depth: usize,
    files: &mut u64,
    folders: &mut u64,
    size: &mut u64,
) {
    if depth > 32 {
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if !include_hidden && name.starts_with('.') {
            continue;
        }
        if !include_system && is_system_entry(&name) {
            continue;
        }
        if regex.is_some_and(|regex| !regex.is_match(&name)) {
            continue;
        }
        if path.is_dir() {
            *folders += 1;
            if recursive {
                scan_folder(
                    &path,
                    recursive,
                    include_hidden,
                    include_system,
                    regex,
                    depth + 1,
                    files,
                    folders,
                    size,
                );
            }
        } else {
            *files += 1;
            *size = size.saturating_add(entry.metadata().map(|meta| meta.len()).unwrap_or(0));
        }
    }
}

struct CoreTempMeasure;

impl RuntimeMeasure for CoreTempMeasure {
    fn update(&mut self, context: &RuntimeContext<'_>) -> RuntimeOutput {
        let values = hwmon_values("temp", 1000.0);
        if values.is_empty() {
            return RuntimeOutput::number(0.0, None, None);
        }
        let index = context.number_option("CoreTempIndex", 1.0).max(1.0) as usize - 1;
        let max_temperature = context
            .option_or("CoreTempType", "Temperature")
            .eq_ignore_ascii_case("MaxTemperature");
        let value = if max_temperature {
            values.iter().copied().fold(f64::MIN, f64::max)
        } else {
            values.get(index).copied().unwrap_or(0.0)
        };
        RuntimeOutput::number(value, None, None)
    }
}

struct SpeedFanMeasure;

impl RuntimeMeasure for SpeedFanMeasure {
    fn update(&mut self, context: &RuntimeContext<'_>) -> RuntimeOutput {
        let index = context.number_option("SpeedFanNumber", 1.0).max(1.0) as usize - 1;
        let kind = context
            .option_or("SpeedFanType", "TEMPERATURE")
            .to_ascii_uppercase();
        let mut value = match kind.as_str() {
            "FAN" => hwmon_values("fan", 1.0),
            "VOLTAGE" => hwmon_values("in", 1000.0),
            _ => hwmon_values("temp", 1000.0),
        }
        .get(index)
        .copied()
        .unwrap_or(0.0);
        if kind == "TEMPERATURE" {
            value = match context
                .option_or("SpeedFanScale", "C")
                .to_ascii_uppercase()
                .as_str()
            {
                "F" => value * 9.0 / 5.0 + 32.0,
                "K" => value + 273.15,
                _ => value,
            };
        }
        RuntimeOutput::number(value, None, None)
    }
}

fn hwmon_values(prefix: &str, divisor: f64) -> Vec<f64> {
    let mut values = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/hwmon") else {
        return values;
    };
    for entry in entries.flatten() {
        let Ok(files) = fs::read_dir(entry.path()) else {
            continue;
        };
        let mut current: Vec<(usize, f64)> = files
            .flatten()
            .filter_map(|file| {
                let name = file.file_name().to_string_lossy().to_string();
                let index = name.strip_prefix(prefix)?.strip_suffix("_input")?;
                let index = index.parse::<usize>().ok()?;
                let value = fs::read_to_string(file.path())
                    .ok()?
                    .trim()
                    .parse::<f64>()
                    .ok()?;
                Some((index, value / divisor))
            })
            .collect();
        current.sort_by_key(|(index, _)| *index);
        values.extend(current.into_iter().map(|(_, value)| value));
    }
    values
}

#[derive(Default)]
struct ResMonMeasure;

impl RuntimeMeasure for ResMonMeasure {
    fn update(&mut self, context: &RuntimeContext<'_>) -> RuntimeOutput {
        let kind = context
            .option_or("ResCountType", "GDI")
            .to_ascii_uppercase();
        let process = context.option_or("ProcessName", "");
        let process = (!process.trim().is_empty()).then_some(process.trim());
        let value = match kind.as_str() {
            "HANDLE" => count_process_handles(process),
            "WINDOW" => count_windows(process),
            "GDI" => count_resource_links(process, ResourceKind::Gdi),
            "USER" => count_resource_links(process, ResourceKind::User) + count_windows(process),
            _ => 0,
        };
        RuntimeOutput::number(value as f64, None, None)
    }
}

#[derive(Clone, Copy)]
enum ResourceKind {
    Gdi,
    User,
}

fn process_matches(entry: &fs::DirEntry, process: Option<&str>) -> bool {
    let Some(process) = process else {
        return true;
    };
    let comm = fs::read_to_string(entry.path().join("comm")).unwrap_or_default();
    if comm.trim().eq_ignore_ascii_case(process) {
        return true;
    }
    let cmdline = fs::read(entry.path().join("cmdline")).unwrap_or_default();
    let command = String::from_utf8_lossy(&cmdline);
    command
        .split('\0')
        .next()
        .and_then(|value| Path::new(value).file_name())
        .map(|value| value.to_string_lossy().eq_ignore_ascii_case(process))
        .unwrap_or(false)
}

fn count_process_handles(process: Option<&str>) -> u64 {
    let mut total = 0u64;
    let Ok(entries) = fs::read_dir("/proc") else {
        return 0;
    };
    for entry in entries.flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .chars()
            .all(|character| character.is_ascii_digit())
        {
            continue;
        }
        if !process_matches(&entry, process) {
            continue;
        }
        total += fs::read_dir(entry.path().join("fd"))
            .map(|fds| fds.count() as u64)
            .unwrap_or(0);
    }
    total
}

fn process_pids(process: Option<&str>) -> Vec<u32> {
    let mut pids = Vec::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return pids;
    };
    for entry in entries.flatten() {
        let Some(pid) = entry.file_name().to_string_lossy().parse::<u32>().ok() else {
            continue;
        };
        if process_matches(&entry, process) {
            pids.push(pid);
        }
    }
    pids
}

fn count_resource_links(process: Option<&str>, kind: ResourceKind) -> u64 {
    let mut total = 0u64;
    let Ok(entries) = fs::read_dir("/proc") else {
        return 0;
    };
    for entry in entries.flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .chars()
            .all(|character| character.is_ascii_digit())
        {
            continue;
        }
        if !process_matches(&entry, process) {
            continue;
        }
        let mut seen = std::collections::BTreeSet::new();
        if let Ok(fds) = fs::read_dir(entry.path().join("fd")) {
            for fd in fds.flatten() {
                let target = fs::read_link(fd.path()).unwrap_or_default();
                let target = target.to_string_lossy().to_ascii_lowercase();
                let matches = match kind {
                    ResourceKind::Gdi => {
                        target.contains("/dev/dri")
                            || target.contains("/dev/fb")
                            || target.contains("nvidia")
                            || target.contains("/dev/shm/")
                    }
                    ResourceKind::User => {
                        target.contains(".x11-unix")
                            || target.contains("wayland")
                            || target.contains("/tmp/.x")
                    }
                };
                if matches {
                    seen.insert(target);
                }
            }
        }
        if matches!(kind, ResourceKind::Gdi) {
            if let Ok(maps) = fs::read_to_string(entry.path().join("maps")) {
                for line in maps.lines() {
                    let lower = line.to_ascii_lowercase();
                    if lower.contains("/dev/dri")
                        || lower.contains("/dev/nvidia")
                        || lower.contains("nvidia")
                    {
                        if let Some(path) = line.split_whitespace().nth(5) {
                            seen.insert(path.to_ascii_lowercase());
                        }
                    }
                }
            }
        }
        total += seen.len() as u64;
    }
    total
}

fn count_windows(process: Option<&str>) -> u64 {
    let pids = process.map(|process| process_pids(Some(process)));
    if pids.as_ref().is_some_and(Vec::is_empty) {
        return 0;
    }
    if let Some(pids) = pids.as_ref() {
        let mut total = 0u64;
        for pid in pids {
            total += xdotool_windows(Some(&pid.to_string()))
                .unwrap_or_else(|| wmctrl_windows(Some(*pid)));
        }
        return total;
    }
    xdotool_windows(None).unwrap_or_else(|| wmctrl_windows(None))
}

fn xdotool_windows(pid: Option<&str>) -> Option<u64> {
    let mut command = Command::new("xdotool");
    command.args(["search", "--onlyvisible"]);
    if let Some(pid) = pid {
        command.args(["--pid", pid]);
    }
    command
        .args(["--name", ".*"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count() as u64
        })
}

fn wmctrl_windows(pid: Option<u32>) -> u64 {
    let Ok(output) = Command::new("wmctrl").arg("-lp").output() else {
        return 0;
    };
    if !output.status.success() {
        return 0;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| {
            let mut fields = line.split_whitespace();
            let _window = fields.next();
            let _desktop = fields.next();
            let window_pid = fields.next().and_then(|value| value.parse::<u32>().ok());
            match pid {
                Some(pid) => window_pid == Some(pid),
                None => true,
            }
        })
        .count() as u64
}

#[derive(Default)]
struct UsageMonitorMeasure {
    previous_cpu: Option<CpuTimes>,
    previous_net: Option<NetSample>,
    previous_time: Option<Instant>,
    previous_processes: BTreeMap<u32, ProcessSample>,
    total_bytes: f64,
}

#[derive(Debug, Clone, Default)]
struct ProcessSample {
    name: String,
    cpu_ticks: u64,
    rss_kb: u64,
    read_bytes: u64,
    write_bytes: u64,
    handles: u64,
    threads: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcessMetric {
    Cpu,
    Rss,
    IoRead,
    IoWrite,
    IoTotal,
    Handles,
    Threads,
}

impl UsageMonitorMeasure {
    fn gpu_process_output(
        &mut self,
        context: &RuntimeContext<'_>,
        alias: &str,
    ) -> Option<RuntimeOutput> {
        let explicit = context.option("PIDToName").is_some()
            || context
                .option("Name")
                .is_some_and(|value| !value.trim().is_empty())
            || context.number_option("Index", 0.0) > 0.0;
        if !explicit {
            return None;
        }
        let samples = gpu_process_samples();
        if samples.is_empty() {
            return None;
        }
        let pid_to_name = context.bool_option("PIDToName", true);
        let metric = match alias {
            "VRAM" | "VRAMSHARED" => ProcessMetric::Rss,
            _ => ProcessMetric::Cpu,
        };
        let percent = context.bool_option("Percent", false);
        let raw_value = context.bool_option("RawValue", false);
        let rollup = context.bool_option("Rollup", true);
        let mut entries: Vec<(String, u32, f64)> = samples
            .iter()
            .map(|(pid, sample)| {
                let name = if pid_to_name {
                    process_name(*pid).unwrap_or_else(|| sample.name.clone())
                } else {
                    format!("pid_{pid}")
                };
                let value = if alias == "VRAM" || alias == "VRAMSHARED" {
                    sample.memory as f64
                } else {
                    sample.utilization
                };
                (name, *pid, value)
            })
            .collect();

        let whitelist = parse_process_list(&context.option_or("Whitelist", ""));
        let blacklist = if whitelist.is_empty() {
            parse_process_list(&context.option_or("Blacklist", ""))
        } else {
            Vec::new()
        };
        entries.retain(|(name, _, _)| {
            let lower = name.to_ascii_lowercase();
            if !whitelist.is_empty() {
                return whitelist.iter().any(|value| lower.contains(value));
            }
            !blacklist.iter().any(|value| lower.contains(value))
        });
        let name_option = context.option_or("Name", "");
        if !name_option.trim().is_empty() {
            let name = name_option.trim().to_ascii_lowercase();
            let pid = name
                .strip_prefix("pid_")
                .unwrap_or(&name)
                .parse::<u32>()
                .ok();
            entries.retain(|(entry_name, entry_pid, _)| {
                entry_name.to_ascii_lowercase().contains(&name) || pid == Some(*entry_pid)
            });
        }
        if rollup {
            let mut grouped: BTreeMap<String, f64> = BTreeMap::new();
            for (name, _, value) in entries {
                let entry = grouped.entry(name).or_insert(0.0);
                if alias == "VRAM" || alias == "VRAMSHARED" {
                    *entry += value;
                } else {
                    *entry = entry.max(value);
                }
            }
            entries = grouped
                .into_iter()
                .map(|(name, value)| (name, 0, value))
                .collect();
        }
        let index = context.number_option("Index", 0.0) as i64;
        let selected = if !name_option.trim().is_empty() {
            entries.iter().map(|(_, _, value)| *value).sum::<f64>()
        } else if index < 0 {
            if entries.is_empty() {
                0.0
            } else {
                entries.iter().map(|(_, _, value)| *value).sum::<f64>() / entries.len() as f64
            }
        } else if index == 0 {
            entries.iter().map(|(_, _, value)| *value).sum()
        } else {
            let mut sorted: Vec<f64> = entries.iter().map(|(_, _, value)| *value).collect();
            sorted.sort_by(|left, right| right.total_cmp(left));
            sorted.get((index - 1) as usize).copied().unwrap_or(0.0)
        };
        let value = match metric {
            ProcessMetric::Rss if percent => {
                let (_, total) = gpu_memory();
                if total == 0 {
                    0.0
                } else {
                    selected / total as f64 * 100.0
                }
            }
            ProcessMetric::Rss if !raw_value => selected / 1024.0 / 1024.0,
            _ => selected,
        };
        Some(RuntimeOutput::number(
            value,
            Some(0.0),
            if metric == ProcessMetric::Rss {
                let (_, total) = gpu_memory();
                (percent.then_some(100.0))
                    .or_else(|| (!raw_value).then_some(total as f64 / 1024.0 / 1024.0))
            } else {
                Some(100.0)
            },
        ))
    }

    fn process_output(
        &mut self,
        context: &RuntimeContext<'_>,
        category: &str,
        counter: &str,
    ) -> Option<RuntimeOutput> {
        let category_lower = category.to_ascii_lowercase();
        let counter_lower = counter.to_ascii_lowercase();
        let is_process = (category_lower.contains("process")
            && !category_lower.contains("processor"))
            || counter_lower.contains("working set")
            || counter_lower.contains("private bytes")
            || counter_lower.contains("handle count")
            || counter_lower.contains("thread count")
            || (counter_lower.contains("processor time") && !category_lower.contains("processor"));
        if !is_process {
            return None;
        }

        let metric = if counter_lower.contains("processor time") || counter_lower.contains("cpu") {
            ProcessMetric::Cpu
        } else if counter_lower.contains("working set")
            || counter_lower.contains("private bytes")
            || counter_lower.contains("memory")
        {
            ProcessMetric::Rss
        } else if counter_lower.contains("read") && counter_lower.contains("bytes") {
            ProcessMetric::IoRead
        } else if counter_lower.contains("write") && counter_lower.contains("bytes") {
            ProcessMetric::IoWrite
        } else if counter_lower.contains("io") || counter_lower.contains("data bytes") {
            ProcessMetric::IoTotal
        } else if counter_lower.contains("handle") {
            ProcessMetric::Handles
        } else if counter_lower.contains("thread") {
            ProcessMetric::Threads
        } else {
            return None;
        };

        let current = process_samples();
        let snapshot = context.backend.cpu_snapshot();
        let total_delta = self
            .previous_cpu
            .map(|previous| snapshot.overall.total.saturating_sub(previous.total))
            .unwrap_or(0);
        self.previous_cpu = Some(snapshot.overall);

        let raw_value = context.bool_option("RawValue", false);
        let percent = context.bool_option("Percent", false);
        let rollup = context.bool_option("Rollup", true);
        let mut entries: Vec<(String, u32, f64)> = Vec::new();
        for (pid, sample) in &current {
            let previous = self.previous_processes.get(pid);
            let value = match metric {
                ProcessMetric::Cpu => {
                    let previous_ticks = previous.map(|value| value.cpu_ticks).unwrap_or(0);
                    let delta = sample.cpu_ticks.saturating_sub(previous_ticks);
                    if total_delta == 0 {
                        0.0
                    } else {
                        delta as f64 / total_delta as f64 * 100.0
                    }
                }
                ProcessMetric::Rss => sample.rss_kb as f64,
                ProcessMetric::IoRead => {
                    if raw_value {
                        sample.read_bytes as f64
                    } else {
                        sample
                            .read_bytes
                            .saturating_sub(previous.map(|value| value.read_bytes).unwrap_or(0))
                            as f64
                    }
                }
                ProcessMetric::IoWrite => {
                    if raw_value {
                        sample.write_bytes as f64
                    } else {
                        sample
                            .write_bytes
                            .saturating_sub(previous.map(|value| value.write_bytes).unwrap_or(0))
                            as f64
                    }
                }
                ProcessMetric::IoTotal => {
                    if raw_value {
                        sample.read_bytes.saturating_add(sample.write_bytes) as f64
                    } else {
                        sample
                            .read_bytes
                            .saturating_add(sample.write_bytes)
                            .saturating_sub(
                                previous
                                    .map(|value| value.read_bytes.saturating_add(value.write_bytes))
                                    .unwrap_or(0),
                            ) as f64
                    }
                }
                ProcessMetric::Handles => sample.handles as f64,
                ProcessMetric::Threads => sample.threads as f64,
            };
            entries.push((sample.name.clone(), *pid, value));
        }
        self.previous_processes = current;

        let whitelist = parse_process_list(&context.option_or("Whitelist", ""));
        let blacklist = if whitelist.is_empty() {
            parse_process_list(&context.option_or("Blacklist", "_Total|Idle"))
        } else {
            Vec::new()
        };
        entries.retain(|(name, _, _)| {
            let lower = name.to_ascii_lowercase();
            if !whitelist.is_empty() {
                return whitelist.iter().any(|value| lower.contains(value));
            }
            !blacklist.iter().any(|value| lower.contains(value))
        });
        let name_option = context.option_or("Name", "");
        if !name_option.trim().is_empty() {
            let name = name_option.trim().to_ascii_lowercase();
            let pid = name.parse::<u32>().ok();
            entries.retain(|(entry_name, entry_pid, _)| {
                entry_name.to_ascii_lowercase().contains(&name) || pid == Some(*entry_pid)
            });
        }

        if rollup {
            let mut grouped: BTreeMap<String, f64> = BTreeMap::new();
            for (name, _, value) in entries {
                *grouped.entry(name).or_insert(0.0) += value;
            }
            entries = grouped
                .into_iter()
                .map(|(name, value)| (name, 0, value))
                .collect();
        }

        let index = context.number_option("Index", 0.0) as i64;
        let selected = if !name_option.trim().is_empty() {
            entries.iter().map(|(_, _, value)| *value).sum::<f64>()
        } else if index < 0 {
            if entries.is_empty() {
                0.0
            } else {
                entries.iter().map(|(_, _, value)| *value).sum::<f64>() / entries.len() as f64
            }
        } else if index == 0 {
            entries.iter().map(|(_, _, value)| *value).sum()
        } else {
            let mut sorted: Vec<f64> = entries.iter().map(|(_, _, value)| *value).collect();
            sorted.sort_by(|left, right| right.total_cmp(left));
            sorted.get((index - 1) as usize).copied().unwrap_or(0.0)
        };

        let number = if metric == ProcessMetric::Rss && percent {
            let memory = context.backend.memory_info();
            if memory.total_kb == 0 {
                0.0
            } else {
                selected / memory.total_kb as f64 * 100.0
            }
        } else if metric == ProcessMetric::Rss && !raw_value {
            selected / 1024.0
        } else {
            selected
        };
        Some(RuntimeOutput::number(
            number,
            Some(0.0),
            (metric == ProcessMetric::Rss && percent)
                .then_some(100.0)
                .or((metric == ProcessMetric::Cpu).then_some(100.0)),
        ))
    }
}

#[derive(Debug, Clone, Default)]
struct GpuProcessSample {
    name: String,
    utilization: f64,
    memory: u64,
}

fn gpu_process_samples() -> BTreeMap<u32, GpuProcessSample> {
    let mut samples = BTreeMap::new();
    if let Ok(output) = Command::new("nvidia-smi")
        .args(["pmon", "-c", "1"])
        .output()
    {
        if output.status.success() {
            merge_pmon_output(&mut samples, &String::from_utf8_lossy(&output.stdout));
        }
    }
    if let Ok(output) = Command::new("nvidia-smi")
        .args([
            "--query-compute-apps=pid,process_name,used_memory",
            "--format=csv,noheader,nounits",
        ])
        .output()
    {
        if output.status.success() {
            merge_compute_apps(&mut samples, &String::from_utf8_lossy(&output.stdout));
        }
    }
    samples
}

fn merge_pmon_output(samples: &mut BTreeMap<u32, GpuProcessSample>, text: &str) {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 8 {
            continue;
        }
        let Ok(pid) = fields[1].parse::<u32>() else {
            continue;
        };
        let utilization = fields[3].parse::<f64>().unwrap_or(0.0);
        let name = fields.last().copied().unwrap_or("").to_string();
        let sample = samples.entry(pid).or_insert_with(|| GpuProcessSample {
            name,
            ..GpuProcessSample::default()
        });
        sample.utilization = sample.utilization.max(utilization);
    }
}

fn merge_compute_apps(samples: &mut BTreeMap<u32, GpuProcessSample>, text: &str) {
    for line in text.lines() {
        let mut fields = line.splitn(3, ',');
        let Some(pid) = fields
            .next()
            .and_then(|value| value.trim().parse::<u32>().ok())
        else {
            continue;
        };
        let name = fields.next().unwrap_or("").trim().to_string();
        let memory = fields
            .next()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(0)
            .saturating_mul(1024 * 1024);
        let sample = samples.entry(pid).or_insert_with(|| GpuProcessSample {
            name: name.clone(),
            ..GpuProcessSample::default()
        });
        sample.memory = sample.memory.saturating_add(memory);
        if sample.name.is_empty() {
            sample.name = name;
        }
    }
}

fn process_name(pid: u32) -> Option<String> {
    fs::read_to_string(format!("/proc/{pid}/comm"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_process_list(value: &str) -> Vec<String> {
    value
        .split(['|', ';', ','])
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect()
}

fn process_samples() -> BTreeMap<u32, ProcessSample> {
    let mut samples = BTreeMap::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return samples;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(pid) = name.parse::<u32>() else {
            continue;
        };
        let path = entry.path();
        let Ok(stat) = fs::read_to_string(path.join("stat")) else {
            continue;
        };
        let Some(start) = stat.find('(') else {
            continue;
        };
        let Some(end) = stat.rfind(')') else {
            continue;
        };
        let process_name = stat[start + 1..end].to_string();
        let fields: Vec<&str> = stat[end + 1..].split_whitespace().collect();
        if fields.len() < 13 {
            continue;
        }
        let cpu_ticks = fields[11]
            .parse::<u64>()
            .unwrap_or(0)
            .saturating_add(fields[12].parse::<u64>().unwrap_or(0));
        let status = fs::read_to_string(path.join("status")).unwrap_or_default();
        let rss_kb = status_value(&status, "VmRSS").unwrap_or(0);
        let threads = status_value(&status, "Threads").unwrap_or(0);
        let io = fs::read_to_string(path.join("io")).unwrap_or_default();
        let read_bytes = io_value(&io, "read_bytes").unwrap_or(0);
        let write_bytes = io_value(&io, "write_bytes").unwrap_or(0);
        let handles = fs::read_dir(path.join("fd"))
            .map(|fds| fds.count() as u64)
            .unwrap_or(0);
        samples.insert(
            pid,
            ProcessSample {
                name: process_name,
                cpu_ticks,
                rss_kb,
                read_bytes,
                write_bytes,
                handles,
                threads,
            },
        );
    }
    samples
}

fn status_value(status: &str, key: &str) -> Option<u64> {
    status.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.trim() == key {
            value.split_whitespace().next()?.parse().ok()
        } else {
            None
        }
    })
}

fn io_value(io: &str, key: &str) -> Option<u64> {
    io.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.trim() == key {
            value.trim().parse().ok()
        } else {
            None
        }
    })
}

impl RuntimeMeasure for UsageMonitorMeasure {
    fn update(&mut self, context: &RuntimeContext<'_>) -> RuntimeOutput {
        let requested = context.option_or("Alias", "CPU").to_ascii_uppercase();
        let category = context.option_or("Category", "");
        let counter = context.option_or("Counter", "");
        if let Some(output) = self.process_output(context, &category, &counter) {
            return output;
        }
        let alias = if requested == "CUSTOM" {
            counter_alias(&category, &counter)
                .unwrap_or("CPU")
                .to_string()
        } else {
            requested
        };
        if matches!(alias.as_str(), "GPU" | "VRAM" | "VRAMSHARED") {
            if let Some(output) = self.gpu_process_output(context, &alias) {
                return output;
            }
        }
        let percent = context.bool_option("Percent", false);
        let raw_value = context.bool_option("RawValue", false);
        match alias.as_str() {
            "CPU" => {
                let snapshot = context.backend.cpu_snapshot();
                let usage = self
                    .previous_cpu
                    .map(|previous| snapshot.overall.usage_percent(previous))
                    .unwrap_or(0.0);
                self.previous_cpu = Some(snapshot.overall);
                RuntimeOutput::number(usage, Some(0.0), Some(100.0))
            }
            "RAM" | "RAMSHARED" => {
                let memory = context.backend.memory_info();
                let value = if percent {
                    memory.used_percent()
                } else if raw_value {
                    memory.used_kb() as f64
                } else {
                    memory.used_kb() as f64 / 1024.0
                };
                RuntimeOutput::number(
                    value,
                    Some(0.0),
                    Some(if percent {
                        100.0
                    } else if raw_value {
                        memory.total_kb as f64
                    } else {
                        memory.total_kb as f64 / 1024.0
                    }),
                )
            }
            "GPU" => RuntimeOutput::number(gpu_usage(), Some(0.0), Some(100.0)),
            "VRAM" | "VRAMSHARED" => {
                let (used, total) = gpu_memory();
                let value = if percent {
                    if total == 0 {
                        0.0
                    } else {
                        used as f64 / total as f64 * 100.0
                    }
                } else if raw_value {
                    used as f64
                } else {
                    used as f64 / 1024.0 / 1024.0
                };
                RuntimeOutput::number(
                    value,
                    Some(0.0),
                    Some(if percent {
                        100.0
                    } else if raw_value {
                        total as f64
                    } else {
                        total as f64 / 1024.0 / 1024.0
                    }),
                )
            }
            "NETDOWN" | "NETUP" => {
                let interface = context.option_or("Interface", "BEST");
                let sample = context.backend.net_sample(&interface);
                let now = Instant::now();
                let value = match (
                    sample.clone(),
                    self.previous_net.clone(),
                    self.previous_time,
                ) {
                    (Some(sample), Some(previous), Some(time)) => {
                        if raw_value {
                            if alias == "NETDOWN" {
                                sample.rx_bytes as f64
                            } else {
                                sample.tx_bytes as f64
                            }
                        } else {
                            let bytes = if alias == "NETDOWN" {
                                sample.rx_bytes.saturating_sub(previous.rx_bytes)
                            } else {
                                sample.tx_bytes.saturating_sub(previous.tx_bytes)
                            };
                            bytes as f64 / now.duration_since(time).as_secs_f64().max(0.001)
                        }
                    }
                    (Some(sample), _, _) if raw_value => {
                        if alias == "NETDOWN" {
                            sample.rx_bytes as f64
                        } else {
                            sample.tx_bytes as f64
                        }
                    }
                    _ => 0.0,
                };
                self.previous_net = sample;
                self.previous_time = Some(now);
                RuntimeOutput::number(value, None, None)
            }
            "IO" | "IOREAD" | "IOWRITE" => {
                let instance = context.option("DiskInstance");
                let (read, write) = disk_bytes_filtered(instance.as_deref());
                let total = if alias == "IOREAD" {
                    read
                } else if alias == "IOWRITE" {
                    write
                } else {
                    read + write
                };
                let value = if raw_value {
                    total as f64
                } else {
                    total.saturating_sub(self.total_bytes as u64) as f64
                };
                self.total_bytes = total as f64;
                RuntimeOutput::number(value, None, None)
            }
            "PROCESSES" => RuntimeOutput::number(process_count() as f64, None, None),
            "THREADS" => RuntimeOutput::number(thread_count() as f64, None, None),
            _ => RuntimeOutput::number(0.0, None, None),
        }
    }
}

fn counter_alias(category: &str, counter: &str) -> Option<&'static str> {
    let value = format!("{category} {counter}").to_ascii_lowercase();
    if value.contains("vram")
        || value.contains("dedicated memory")
        || value.contains("shared usage")
        || value.contains("gpu process memory")
    {
        return Some("VRAM");
    }
    if value.contains("gpu") {
        return Some("GPU");
    }
    if value.contains("processor") || value.contains("cpu") {
        return Some("CPU");
    }
    if value.contains("memory") || value.contains("mbytes") {
        return Some("RAM");
    }
    if value.contains("network") || value.contains("bytes received") {
        if value.contains("received") {
            return Some("NETDOWN");
        }
        if value.contains("sent") {
            return Some("NETUP");
        }
        return Some("NETDOWN");
    }
    if value.contains("disk") {
        if value.contains("read") {
            return Some("IOREAD");
        }
        if value.contains("write") {
            return Some("IOWRITE");
        }
        return Some("IO");
    }
    if value.contains("thread") {
        return Some("THREADS");
    }
    if value.contains("process") {
        return Some("PROCESSES");
    }
    None
}

fn process_count() -> u64 {
    fs::read_dir("/proc")
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .chars()
                        .all(|character| character.is_ascii_digit())
                })
                .count() as u64
        })
        .unwrap_or(0)
}

fn thread_count() -> u64 {
    let Ok(entries) = fs::read_dir("/proc") else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .chars()
                .all(|character| character.is_ascii_digit())
        })
        .map(|entry| {
            fs::read_dir(entry.path().join("task"))
                .map(|tasks| tasks.count() as u64)
                .unwrap_or(0)
        })
        .sum()
}

fn gpu_usage() -> f64 {
    let mut amd = Vec::new();
    if let Ok(entries) = fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            let path = entry.path().join("device/gpu_busy_percent");
            if let Ok(text) = fs::read_to_string(path) {
                if let Ok(value) = text.trim().parse::<f64>() {
                    amd.push(value);
                }
            }
        }
    }
    if let Some(value) = amd.into_iter().reduce(f64::max) {
        return value;
    }
    nvidia_smi_values("utilization.gpu")
        .first()
        .copied()
        .unwrap_or(0.0)
}

fn gpu_memory() -> (u64, u64) {
    let mut amd = Vec::new();
    if let Ok(entries) = fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            let device = entry.path().join("device");
            let used = fs::read_to_string(device.join("mem_info_vram_used"))
                .ok()
                .and_then(|value| value.trim().parse::<u64>().ok());
            let total = fs::read_to_string(device.join("mem_info_vram_total"))
                .ok()
                .and_then(|value| value.trim().parse::<u64>().ok());
            if let (Some(used), Some(total)) = (used, total) {
                amd.push((used, total));
            }
        }
    }
    if let Some(value) = amd.into_iter().max_by_key(|(_, total)| *total) {
        return value;
    }
    let used = nvidia_smi_values("memory.used")
        .first()
        .copied()
        .unwrap_or(0.0);
    let total = nvidia_smi_values("memory.total")
        .first()
        .copied()
        .unwrap_or(0.0);
    (
        (used * 1024.0 * 1024.0) as u64,
        (total * 1024.0 * 1024.0) as u64,
    )
}

fn nvidia_smi_values(field: &str) -> Vec<f64> {
    Command::new("nvidia-smi")
        .args([
            &format!("--query-gpu={field}"),
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(|line| line.trim().parse::<f64>().ok())
                .collect()
        })
        .unwrap_or_default()
}

fn disk_bytes_filtered(instance: Option<&str>) -> (u64, u64) {
    let Ok(text) = fs::read_to_string("/proc/diskstats") else {
        return (0, 0);
    };
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 14 || fields[2].starts_with("loop") || fields[2].starts_with("ram") {
                return None;
            }
            if let Some(instance) = instance {
                let instance = instance.trim();
                if !instance.is_empty()
                    && !fields[2].eq_ignore_ascii_case(instance)
                    && !fields[2].starts_with(instance)
                {
                    return None;
                }
            }
            let read: u64 = fields[5].parse().ok()?;
            let write: u64 = fields[9].parse().ok()?;
            Some((read * 512, write * 512))
        })
        .fold((0u64, 0u64), |(read, write), (next_read, next_write)| {
            (
                read.saturating_add(next_read),
                write.saturating_add(next_write),
            )
        })
}

#[derive(Default)]
struct PerfMonMeasure {
    usage: UsageMonitorMeasure,
}

impl RuntimeMeasure for PerfMonMeasure {
    fn update(&mut self, context: &RuntimeContext<'_>) -> RuntimeOutput {
        let object = context.option_or("PerfMonObject", "Processor");
        let counter = context.option_or("PerfMonCounter", "% Processor Time");
        let alias = counter_alias(&object, &counter).unwrap_or_else(|| {
            if object.to_ascii_lowercase().contains("memory") {
                "RAM"
            } else {
                "CPU"
            }
        });
        let difference = context.bool_option("PerfMonDifference", false);

        if alias == "RAM" {
            let memory = context.backend.memory_info();
            let lower = format!("{object} {counter}").to_ascii_lowercase();
            let value = if lower.contains("available") {
                memory.available_kb as f64 / 1024.0
            } else if lower.contains("percent") {
                memory.used_percent()
            } else if lower.contains("bytes") || difference {
                memory.used_kb() as f64 * 1024.0
            } else {
                memory.used_kb() as f64 / 1024.0
            };
            return RuntimeOutput::number(value, None, None);
        }

        let mut option_entries = context.options.entries.clone();
        option_entries.push(crate::model::OptionEntry {
            key: "Alias".to_string(),
            value: alias.to_string(),
            line: 0,
        });
        if difference && matches!(alias, "IO" | "IOREAD" | "IOWRITE") {
            option_entries.push(crate::model::OptionEntry {
                key: "RawValue".to_string(),
                value: "0".to_string(),
                line: 0,
            });
        }
        let instance = context.option_or("PerfMonInstance", "");
        if !instance.trim().is_empty() {
            let key = if matches!(alias, "NETDOWN" | "NETUP") {
                "Interface"
            } else {
                "DiskInstance"
            };
            if matches!(alias, "NETDOWN" | "NETUP" | "IO" | "IOREAD" | "IOWRITE") {
                option_entries.push(crate::model::OptionEntry {
                    key: key.to_string(),
                    value: instance,
                    line: 0,
                });
            }
        }
        let options = Options {
            entries: option_entries,
        };
        let nested = RuntimeContext {
            options: &options,
            ..*context
        };
        self.usage.update(&nested)
    }
}

#[derive(Default)]
struct AdvancedCpuMeasure {
    previous_total: u64,
    previous_processes: BTreeMap<String, u64>,
}

impl RuntimeMeasure for AdvancedCpuMeasure {
    fn update(&mut self, context: &RuntimeContext<'_>) -> RuntimeOutput {
        let snapshot = context.backend.cpu_snapshot();
        let total = snapshot.overall.total;
        let total_delta = total.saturating_sub(self.previous_total);
        let current = process_cpu_ticks();
        let includes: Vec<String> = context
            .option_or("CPUInclude", "")
            .split(';')
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .collect();
        let excludes: Vec<String> = context
            .option_or("CPUExclude", "")
            .split(';')
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .collect();
        let top_process = context.number_option("TopProcess", 0.0) as i32;

        let mut best: Option<(String, u64)> = None;
        let mut sum = 0u64;
        for (name, ticks) in &current {
            let lower = name.to_ascii_lowercase();
            if !includes.is_empty() && !includes.iter().any(|value| value == &lower) {
                continue;
            }
            if excludes.iter().any(|value| value == &lower) {
                continue;
            }
            let previous = self.previous_processes.get(name).copied().unwrap_or(0);
            let delta = ticks.saturating_sub(previous);
            sum = sum.saturating_add(delta);
            if best.as_ref().is_none_or(|(_, value)| delta > *value) {
                best = Some((name.clone(), delta));
            }
        }

        let value = if top_process == 0 {
            sum
        } else {
            best.as_ref().map(|(_, value)| *value).unwrap_or(0)
        };
        let usage = if total_delta == 0 {
            0.0
        } else {
            (value as f64 / total_delta as f64 * 100.0).clamp(0.0, 100.0)
        };
        self.previous_total = total;
        self.previous_processes = current;
        RuntimeOutput {
            value: MeasureValue {
                number: usage,
                string: (top_process == 2).then(|| best.map(|(name, _)| name).unwrap_or_default()),
                min: Some(0.0),
                max: Some(100.0),
            },
            actions: Vec::new(),
        }
    }
}

fn process_cpu_ticks() -> BTreeMap<String, u64> {
    let mut values = BTreeMap::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return values;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.chars().all(|character| character.is_ascii_digit()) {
            continue;
        }
        let Ok(text) = fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        let Some(start) = text.find('(') else {
            continue;
        };
        let Some(end) = text.rfind(')') else {
            continue;
        };
        let process = text[start + 1..end].to_string();
        let fields: Vec<&str> = text[end + 1..].split_whitespace().collect();
        if fields.len() < 13 {
            continue;
        }
        let user = fields[11].parse::<u64>().unwrap_or(0);
        let system = fields[12].parse::<u64>().unwrap_or(0);
        *values.entry(process).or_insert(0) += user.saturating_add(system);
    }
    values
}

#[derive(Default)]
struct QuoteMeasure {
    files: Vec<PathBuf>,
    loaded: bool,
}

impl RuntimeMeasure for QuoteMeasure {
    fn update(&mut self, context: &RuntimeContext<'_>) -> RuntimeOutput {
        if !self.loaded {
            self.files = quote_files(context);
            self.loaded = true;
        }
        let path = if self.files.is_empty() {
            context.path_option("PathName", "")
        } else {
            let index = (context.counter as usize) % self.files.len();
            self.files[index].clone()
        };
        let Ok(text) = fs::read_to_string(&path) else {
            return RuntimeOutput::empty();
        };
        let separator = context.option_or("Separator", "\n");
        let quotes: Vec<&str> = text
            .split(&separator)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect();
        if quotes.is_empty() {
            return RuntimeOutput::empty();
        }
        let index = (context.counter as usize) % quotes.len();
        RuntimeOutput::text(quotes[index].to_string())
    }
}

fn quote_files(context: &RuntimeContext<'_>) -> Vec<PathBuf> {
    let path = context.path_option("PathName", "");
    if path.is_file() {
        return vec![path];
    }
    let filters: Vec<String> = context
        .option("FileFilter")
        .unwrap_or_default()
        .split(';')
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect();
    let mut files = Vec::new();
    collect_quote_files(
        &path,
        context.bool_option("Subfolders", true),
        &filters,
        &mut files,
        0,
    );
    files
}

fn collect_quote_files(
    path: &Path,
    recursive: bool,
    filters: &[String],
    files: &mut Vec<PathBuf>,
    depth: usize,
) {
    if depth > 16 {
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if recursive {
                collect_quote_files(&path, recursive, filters, files, depth + 1);
            }
        } else if filters.is_empty()
            || filters.iter().any(|filter| {
                path.file_name()
                    .map(|name| name.to_string_lossy().to_ascii_lowercase().contains(filter))
                    .unwrap_or(false)
            })
        {
            files.push(path);
        }
    }
}

#[derive(Default)]
struct InputTextMeasure {
    pending: Arc<Mutex<Vec<String>>>,
    running: Arc<Mutex<bool>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct InputOptions {
    default_value: String,
    password: bool,
    input_limit: usize,
    input_number: bool,
    title: String,
    text: String,
    on_dismiss: String,
    width: usize,
    height: usize,
    x: i32,
    y: i32,
    font_face: String,
    font_size: usize,
    font_color: String,
    string_style: String,
    solid_color: String,
    top_most: bool,
    string_align: String,
    focus_dismiss: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InputStep {
    action: String,
    needs_input: bool,
    options: InputOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum InputPlan {
    Variable {
        name: String,
        options: Box<InputOptions>,
    },
    Batch(Vec<InputStep>),
}

impl RuntimeMeasure for InputTextMeasure {
    fn update(&mut self, _context: &RuntimeContext<'_>) -> RuntimeOutput {
        let actions = self
            .pending
            .lock()
            .map(|mut pending| std::mem::take(&mut *pending))
            .unwrap_or_default();
        RuntimeOutput::empty().with_actions(actions)
    }

    fn command(&mut self, command: &str, context: &RuntimeContext<'_>) -> bool {
        if self.running.lock().map(|running| *running).unwrap_or(false) {
            return false;
        }
        let Some(plan) = parse_input_plan(command, &|key| context.option(key)) else {
            return false;
        };
        if let Ok(mut running) = self.running.lock() {
            *running = true;
        }
        let pending = self.pending.clone();
        let running = self.running.clone();
        std::thread::spawn(move || {
            let mut actions = Vec::new();
            let dismissed = match &plan {
                InputPlan::Variable { name, options } => match read_input(options) {
                    Some(value) => {
                        actions.push(format!(
                            "[!SetVariable \"{}\" \"{}\"]",
                            name,
                            value.replace('"', "\\\"")
                        ));
                        false
                    }
                    None => true,
                },
                InputPlan::Batch(steps) => {
                    let mut dismissed = false;
                    for step in steps {
                        let mut action = step.action.clone();
                        if step.needs_input {
                            match read_input(&step.options) {
                                Some(value) => action = replace_user_input(&action, &value),
                                None => {
                                    dismissed = true;
                                    break;
                                }
                            }
                        }
                        actions.push(action);
                    }
                    dismissed
                }
            };
            if dismissed {
                let on_dismiss = match &plan {
                    InputPlan::Variable { options, .. } => options.on_dismiss.clone(),
                    InputPlan::Batch(steps) => steps
                        .first()
                        .map(|step| step.options.on_dismiss.clone())
                        .unwrap_or_default(),
                };
                if !on_dismiss.is_empty() {
                    actions.push(on_dismiss);
                }
            }
            if let Ok(mut pending) = pending.lock() {
                pending.extend(actions);
            }
            if let Ok(mut running) = running.lock() {
                *running = false;
            }
        });
        true
    }
}

fn parse_input_plan(command: &str, option: &impl Fn(&str) -> Option<String>) -> Option<InputPlan> {
    let command = command.trim();
    if command.is_empty() {
        return None;
    }
    let base = input_options(option);
    let mut parts = command.splitn(2, char::is_whitespace);
    let first = parts.next().unwrap_or_default();
    let rest = parts.next().unwrap_or_default().trim();
    if first.eq_ignore_ascii_case("ExecuteBatch") {
        let (minimum, maximum) = parse_input_range(rest);
        let mut steps = Vec::new();
        for index in minimum..=maximum {
            let key = format!("Command{index}");
            let Some(action) = option(&key) else {
                break;
            };
            if action.trim().is_empty() {
                break;
            }
            steps.push(InputStep {
                needs_input: action.to_ascii_uppercase().contains("$USERINPUT$"),
                action,
                options: base.clone(),
            });
        }
        return (!steps.is_empty()).then_some(InputPlan::Batch(steps));
    }
    if rest.is_empty() {
        return Some(InputPlan::Variable {
            name: first.to_string(),
            options: Box::new(base),
        });
    }
    None
}

fn parse_input_range(value: &str) -> (usize, usize) {
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("all") {
        return (1, 1_000_000_000);
    }
    if let Some((minimum, maximum)) = value.split_once('-') {
        if let (Ok(minimum), Ok(maximum)) = (
            minimum.trim().parse::<usize>(),
            maximum.trim().parse::<usize>(),
        ) {
            return (minimum, maximum.max(minimum));
        }
    }
    value
        .parse()
        .map(|index| (index, index))
        .unwrap_or((1, 1_000_000_000))
}

fn input_options(option: &impl Fn(&str) -> Option<String>) -> InputOptions {
    InputOptions {
        default_value: option("DefaultValue").unwrap_or_default(),
        password: option("Password")
            .map(|value| value.eq_ignore_ascii_case("true") || value == "1")
            .unwrap_or(false),
        input_limit: option("InputLimit")
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(0),
        input_number: option("InputNumber")
            .map(|value| value.eq_ignore_ascii_case("true") || value == "1")
            .unwrap_or(false),
        title: option("Title").unwrap_or_else(|| "Petrichor".to_string()),
        text: option("Text").unwrap_or_default(),
        on_dismiss: option("OnDismissAction").unwrap_or_default(),
        width: option("W")
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(0),
        height: option("H")
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(0),
        x: option("X")
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(0),
        y: option("Y")
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(0),
        font_face: option("FontFace").unwrap_or_default(),
        font_size: option("FontSize")
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(0),
        font_color: option("FontColor").unwrap_or_default(),
        string_style: option("StringStyle").unwrap_or_default(),
        solid_color: option("SolidColor").unwrap_or_default(),
        top_most: option("TopMost")
            .map(|value| value.eq_ignore_ascii_case("true") || value == "1")
            .unwrap_or(false),
        string_align: option("StringAlign").unwrap_or_default(),
        focus_dismiss: option("FocusDismiss")
            .map(|value| value.eq_ignore_ascii_case("true") || value == "1")
            .unwrap_or(false),
    }
}

fn input_font(options: &InputOptions) -> String {
    let mut description = if options.font_face.is_empty() {
        if options.string_style.is_empty() && options.font_size == 0 {
            return String::new();
        }
        "Sans".to_string()
    } else {
        options.font_face.clone()
    };
    let style = options.string_style.to_ascii_lowercase();
    if style.contains("bold") {
        description.push_str(" Bold");
    }
    if style.contains("italic") {
        description.push_str(" Italic");
    }
    if options.font_size > 0 {
        description.push_str(&format!(" {}", options.font_size));
    }
    description
}

fn input_markup(text: &str, options: &InputOptions) -> String {
    let style = options.string_style.to_ascii_lowercase();
    let mut attributes = String::new();
    if style.contains("bold") {
        attributes.push_str(" font_weight='bold'");
    }
    if style.contains("italic") {
        attributes.push_str(" font_style='italic'");
    }
    if !options.font_color.is_empty() {
        attributes.push_str(&format!(" foreground='{}'", options.font_color));
    }
    if !options.font_face.is_empty() {
        attributes.push_str(&format!(" font_family='{}'", options.font_face));
    }
    if options.font_size > 0 {
        attributes.push_str(&format!(" font_size='{}'", options.font_size * 1024));
    }
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    if attributes.is_empty() {
        escaped
    } else {
        format!("<span{attributes}>{escaped}</span>")
    }
}

const GTK_INPUT_SCRIPT: &str = r#"
import json, sys
try:
    import gi
    gi.require_version('Gtk', '3.0')
    from gi.repository import Gtk, Gdk, Pango
except Exception:
    sys.exit(2)
try:
    options = json.load(sys.stdin)
except Exception:
    sys.exit(2)
dialog = Gtk.Dialog(title=options.get('title') or 'Petrichor')
dialog.add_button('Annuler', Gtk.ResponseType.CANCEL)
dialog.add_button('OK', Gtk.ResponseType.OK)
dialog.set_default_response(Gtk.ResponseType.OK)
content = dialog.get_content_area()
content.set_spacing(8)
label = Gtk.Label()
label.set_line_wrap(True)
label.set_markup(options.get('markup') or '')
content.add(label)
entry = Gtk.Entry()
entry.set_text(options.get('default') or '')
entry.set_visibility(not options.get('password'))
limit = int(options.get('max_length') or 0)
if limit > 0:
    entry.set_max_length(limit)
font = options.get('font') or ''
if font:
    entry.modify_font(Pango.FontDescription(font))
font_color = Gdk.RGBA()
color = options.get('font_color') or ''
if color and font_color.parse(color):
    entry.modify_text(Gtk.StateType.NORMAL, font_color)
background = Gdk.RGBA()
solid = options.get('solid_color') or ''
if solid and background.parse(solid):
    dialog.override_background_color(Gtk.StateFlags.NORMAL, background)
align = (options.get('string_align') or '').lower()
entry.set_alignment(1.0 if 'right' in align else 0.0 if 'left' in align else 0.5)
dialog.set_default_size(int(options.get('width') or 360), int(options.get('height') or 120))
x = int(options.get('x') or 0)
y = int(options.get('y') or 0)
if x > 0 and y > 0:
    dialog.move(x, y)
if options.get('top_most'):
    dialog.set_keep_above(True)
entry.connect('activate', lambda widget: dialog.response(Gtk.ResponseType.OK))
if options.get('focus_dismiss'):
    dialog.connect('focus-out-event', lambda widget, event: (dialog.response(Gtk.ResponseType.CANCEL), False)[1])
content.show_all()
entry.grab_focus()
response = dialog.run()
value = entry.get_text() if response == Gtk.ResponseType.OK else None
dialog.destroy()
if value is None:
    sys.exit(1)
sys.stdout.write(value)
"#;

fn read_input_gtk(options: &InputOptions, text: &str) -> Option<Option<String>> {
    let payload = json!({
        "title": options.title,
        "markup": input_markup(text, options),
        "default": options.default_value,
        "password": options.password,
        "max_length": options.input_limit,
        "width": options.width,
        "height": options.height,
        "x": options.x,
        "y": options.y,
        "font": input_font(options),
        "font_color": options.font_color,
        "solid_color": options.solid_color,
        "string_align": options.string_align,
        "top_most": options.top_most,
        "focus_dismiss": options.focus_dismiss,
    });
    let mut child = Command::new("python3")
        .args(["-c", GTK_INPUT_SCRIPT])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(payload.to_string().as_bytes());
    }
    let output = child.wait_with_output().ok()?;
    if output.status.code() == Some(2) {
        return None;
    }
    if output.status.success() {
        Some(Some(
            String::from_utf8_lossy(&output.stdout)
                .trim_end()
                .to_string(),
        ))
    } else {
        Some(None)
    }
}

fn read_input(options: &InputOptions) -> Option<String> {
    let text = if options.text.is_empty() {
        "Entrez une valeur :"
    } else {
        &options.text
    };

    let mut yad = Command::new("yad");
    yad.arg("--entry")
        .arg(format!("--title={}", options.title))
        .arg(format!("--text={text}"));
    if !options.default_value.is_empty() {
        yad.arg(format!("--entry-text={}", options.default_value));
    }
    if options.password {
        yad.arg("--hide-text");
    }
    if options.width > 0 {
        yad.arg(format!("--width={}", options.width));
    }
    if options.height > 0 {
        yad.arg(format!("--height={}", options.height));
    }
    if options.x > 0 {
        yad.arg(format!("--posx={}", options.x));
    }
    if options.y > 0 {
        yad.arg(format!("--posy={}", options.y));
    }
    let font = input_font(options);
    if !font.is_empty() {
        yad.arg(format!("--fontname={font}"));
    }
    if !options.font_color.is_empty() {
        yad.arg(format!("--foreground={}", options.font_color));
    }
    if !options.solid_color.is_empty() {
        yad.arg(format!("--background={}", options.solid_color));
    }
    if options.top_most {
        yad.arg("--on-top");
    }
    if options.focus_dismiss {
        yad.arg("--close-on-unfocus");
    }
    if !options.string_align.is_empty() {
        yad.arg(format!("--text-align={}", options.string_align));
    }
    let yad = yad.stdin(Stdio::null()).stderr(Stdio::null()).output();
    let mut value = match yad {
        Ok(output) => output.status.success().then(|| {
            String::from_utf8_lossy(&output.stdout)
                .trim_end()
                .to_string()
        }),
        Err(_) => {
            let mut zenity = Command::new("zenity");
            zenity
                .arg("--entry")
                .arg(format!("--title={}", options.title));
            zenity.arg(format!("--text={}", input_markup(text, options)));
            if !options.default_value.is_empty() {
                zenity.arg(format!("--entry-text={}", options.default_value));
            }
            if options.password {
                zenity.arg("--hide-text");
            }
            if options.width > 0 {
                zenity.arg(format!("--width={}", options.width));
            }
            if options.top_most {
                zenity.arg("--modal");
            }
            let zenity = zenity.stdin(Stdio::null()).stderr(Stdio::null()).output();
            match zenity {
                Ok(output) => output.status.success().then(|| {
                    String::from_utf8_lossy(&output.stdout)
                        .trim_end()
                        .to_string()
                }),
                Err(_) => match read_input_gtk(options, text) {
                    Some(value) => value,
                    None => {
                        let mut kdialog = Command::new("kdialog");
                        if options.password {
                            kdialog.arg("--password").arg(text);
                        } else {
                            kdialog
                                .arg("--inputbox")
                                .arg(text)
                                .arg("0")
                                .arg("0")
                                .arg(&options.default_value);
                        }
                        kdialog.arg("--title").arg(&options.title);
                        if options.width > 0 && options.height > 0 {
                            let x = if options.x > 0 { options.x } else { 0 };
                            let y = if options.y > 0 { options.y } else { 0 };
                            kdialog
                                .arg("--geometry")
                                .arg(format!("{}x{}+{}+{}", options.width, options.height, x, y));
                        }
                        if options.top_most {
                            kdialog.arg("--keep-above");
                        }
                        kdialog
                            .stdin(Stdio::null())
                            .stderr(Stdio::null())
                            .output()
                            .ok()
                            .filter(|output| output.status.success())
                            .map(|output| {
                                String::from_utf8_lossy(&output.stdout)
                                    .trim_end()
                                    .to_string()
                            })
                    }
                },
            }
        }
    }?;
    if options.input_limit > 0 && value.chars().count() > options.input_limit {
        value = value.chars().take(options.input_limit).collect();
    }
    if options.input_number && !value.is_empty() && value.parse::<f64>().is_err() {
        return None;
    }
    Some(value)
}

fn replace_user_input(action: &str, value: &str) -> String {
    let mut output = String::with_capacity(action.len() + value.len());
    let mut rest = action;
    loop {
        let lower = rest.to_ascii_lowercase();
        let Some(index) = lower.find("$userinput$") else {
            output.push_str(rest);
            break;
        };
        output.push_str(&rest[..index]);
        output.push_str(value);
        rest = &rest[index + "$userinput$".len()..];
    }
    output
}

#[derive(Default)]
struct WindowMessageMeasure {
    title: String,
}

type WindowMessage = (u64, u64, u64);

static WINDOW_MESSAGES: std::sync::LazyLock<Mutex<BTreeMap<String, WindowMessage>>> =
    std::sync::LazyLock::new(|| Mutex::new(BTreeMap::new()));

impl RuntimeMeasure for WindowMessageMeasure {
    fn update(&mut self, context: &RuntimeContext<'_>) -> RuntimeOutput {
        let key = window_message_key(context);
        let message = parse_window_message(&context.option_or("WindowMessage", ""));
        if let Some(message) = message {
            let message = WINDOW_MESSAGES
                .lock()
                .ok()
                .map(|mut messages| {
                    if let Some(existing) = messages.get(&key) {
                        *existing
                    } else {
                        messages.insert(key, message);
                        message
                    }
                })
                .unwrap_or(message);
            return RuntimeOutput {
                value: MeasureValue {
                    number: message.1 as f64,
                    string: Some(format!("{} {} {}", message.0, message.1, message.2)),
                    min: None,
                    max: None,
                },
                actions: Vec::new(),
            };
        }
        if self.title.is_empty() {
            self.title = window_title(context).unwrap_or_default();
        }
        RuntimeOutput::text(self.title.clone())
    }

    fn command(&mut self, command: &str, context: &RuntimeContext<'_>) -> bool {
        let command = command.trim();
        let rest = if command.eq_ignore_ascii_case("SendMessage") {
            ""
        } else if command
            .get(.."SendMessage".len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("SendMessage"))
        {
            command["SendMessage".len()..].trim()
        } else {
            return false;
        };
        let Some(message) = parse_window_message(rest) else {
            return false;
        };
        if let Ok(mut messages) = WINDOW_MESSAGES.lock() {
            messages.insert(window_message_key(context), message);
        }
        true
    }
}

fn window_message_key(context: &RuntimeContext<'_>) -> String {
    let name = context.option_or("WindowName", "");
    let class = context.option_or("WindowClass", "");
    format!(
        "{}|{}",
        name.trim().to_ascii_lowercase(),
        class.trim().to_ascii_lowercase()
    )
}

fn parse_window_message(value: &str) -> Option<(u64, u64, u64)> {
    let mut parts = value.split_whitespace();
    Some((
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    ))
}

fn window_title(context: &RuntimeContext<'_>) -> Option<String> {
    let name = context.option_or("WindowName", "");
    let class = context.option_or("WindowClass", "");
    if name.is_empty() && class.is_empty() {
        return None;
    }
    let mut search = Command::new("xdotool");
    search.arg("search");
    if !class.is_empty() {
        search.arg("--class").arg(class);
    } else {
        search.arg("--name").arg(name);
    }
    let output = search
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let id = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()?
        .trim()
        .to_string();
    let output = Command::new("xdotool")
        .args(["getwindowname", &id])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::OptionEntry;
    use petrichor_sys::{
        CpuSnapshot, MemoryInfo, NetSample, OsRelease, SysInfoValue, TrashInfo, WifiInfo,
    };

    struct MockBackend {
        cpu: CpuSnapshot,
        memory: MemoryInfo,
        net: Option<NetSample>,
    }

    impl MockBackend {
        fn new() -> Self {
            MockBackend {
                cpu: CpuSnapshot::default(),
                memory: MemoryInfo {
                    total_kb: 1024,
                    available_kb: 512,
                    ..MemoryInfo::default()
                },
                net: None,
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
                name: "Linux".to_string(),
                version: String::new(),
                id: "linux".to_string(),
            }
        }
        fn uptime_secs(&self) -> u64 {
            0
        }
        fn cpu_snapshot(&self) -> CpuSnapshot {
            self.cpu.clone()
        }
        fn memory_info(&self) -> MemoryInfo {
            self.memory
        }
        fn net_sample(&self, _interface: &str) -> Option<NetSample> {
            self.net.clone()
        }
        fn sysinfo(&self, _kind: &str, _data: &str) -> Option<SysInfoValue> {
            None
        }
        fn trash_info(&self) -> TrashInfo {
            TrashInfo::default()
        }
        fn wifi_info(&self) -> WifiInfo {
            WifiInfo::default()
        }
    }

    fn options(entries: &[(&str, &str)]) -> Options {
        Options {
            entries: entries
                .iter()
                .map(|(key, value)| OptionEntry {
                    key: key.to_string(),
                    value: value.to_string(),
                    line: 1,
                })
                .collect(),
        }
    }

    fn context<'a>(
        options: &'a Options,
        variables: &'a Variables,
        backend: &'a dyn SystemBackend,
    ) -> RuntimeContext<'a> {
        RuntimeContext {
            skin_name: "Test",
            skin_root: Path::new("/tmp"),
            skin_path: Path::new("/tmp/Test.ini"),
            measure_name: "Measure",
            options,
            variables,
            backend,
            values: Box::leak(Box::new(BTreeMap::new())),
            meters: &[],
            sections: Box::leak(Box::new(BTreeMap::new())),
            skin_window_handle: 0,
            counter: 1.0,
        }
    }

    #[test]
    fn builtin_aliases_and_reserved_names() {
        assert!(create_builtin("RunCommand").is_some());
        assert!(create_builtin("Win7Audio").is_some());
        assert!(create_builtin("Audio").is_some());
        assert!(create_builtin("Unknown").is_none());
        assert_eq!(plugin_module_name("RunCommand.dll"), "RunCommand");
        assert_eq!(plugin_module_name("/tmp/My.so"), "My");
    }

    #[test]
    fn splits_function_arguments() {
        let (name, args) = split_function("Greet(\"a,b\", 2, true)");
        assert_eq!(name, "Greet");
        assert_eq!(args, vec!["a,b", "2", "true"]);
    }

    #[test]
    fn power_and_ping_return_ranges() {
        let backend = MockBackend::new();
        let vars = Variables::new();
        let opts = options(&[("PowerState", "PERCENT")]);
        let mut power = PowerMeasure;
        let output = power.update(&context(&opts, &vars, &backend));
        assert!((0.0..=100.0).contains(&output.value.number));

        let opts = options(&[("DestAddress", "127.0.0.1"), ("Timeout", "100")]);
        let mut ping = PingMeasure::default();
        let output = ping.update(&context(&opts, &vars, &backend));
        assert!(output.value.number >= 0.0);
    }

    #[test]
    fn folder_info_counts_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "hello").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub/b.txt"), "world!").unwrap();
        let backend = MockBackend::new();
        let vars = Variables::new();
        let opts = options(&[
            ("Folder", dir.path().to_str().unwrap()),
            ("InfoType", "FileCount"),
            ("IncludeSubFolders", "1"),
        ]);
        let output = FolderInfoMeasure.update(&context(&opts, &vars, &backend));
        assert_eq!(output.value.number, 2.0);

        let opts = options(&[
            ("Folder", dir.path().to_str().unwrap()),
            ("InfoType", "FolderSize"),
            ("IncludeSubFolders", "1"),
        ]);
        let output = FolderInfoMeasure.update(&context(&opts, &vars, &backend));
        assert_eq!(output.value.number, 11.0);
    }

    #[test]
    fn fileview_lists_and_navigates() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("b.txt"), "b").unwrap();
        fs::write(dir.path().join("a.txt"), "a").unwrap();
        let backend = MockBackend::new();
        let vars = Variables::new();
        let opts = options(&[
            ("Path", dir.path().to_str().unwrap()),
            ("Type", "FileName"),
            ("ShowDotDot", "0"),
            ("Count", "10"),
            ("SortAscending", "1"),
        ]);
        let ctx = context(&opts, &vars, &backend);
        let mut view = FileViewMeasure::default();
        let first = view.update(&ctx);
        assert_eq!(first.value.string.as_deref(), Some("a.txt"));
        assert!(view.command("IndexDown", &ctx));
        let second = view.update(&ctx);
        assert_eq!(second.value.string.as_deref(), Some("b.txt"));
    }

    #[test]
    fn fileview_honors_filters_sorting_and_types() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "a").unwrap();
        fs::write(dir.path().join("b.log"), "bbb").unwrap();
        fs::write(dir.path().join(".hidden"), "hidden").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        let backend = MockBackend::new();
        let vars = Variables::new();
        let opts = options(&[
            ("Path", dir.path().to_str().unwrap()),
            ("Type", "FileName"),
            ("ShowDotDot", "0"),
            ("ShowHidden", "0"),
            ("SortType", "Size"),
            ("SortAscending", "0"),
            ("Count", "10"),
        ]);
        let ctx = context(&opts, &vars, &backend);
        let mut view = FileViewMeasure::default();
        let first = view.update(&ctx);
        assert_eq!(first.value.string.as_deref(), Some("b.log"));

        let opts = options(&[
            ("Path", dir.path().to_str().unwrap()),
            ("Type", "FileType"),
            ("ShowDotDot", "0"),
            ("SortType", "Size"),
            ("SortAscending", "0"),
            ("Count", "10"),
        ]);
        let ctx = context(&opts, &vars, &backend);
        assert_eq!(view.update(&ctx).value.string.as_deref(), Some(".log"));

        let opts = options(&[
            ("Path", dir.path().to_str().unwrap()),
            ("Type", "FolderSize"),
            ("ShowDotDot", "0"),
            ("Count", "10"),
        ]);
        let ctx = context(&opts, &vars, &backend);
        assert_eq!(view.update(&ctx).value.number, 4.0);

        let opts = options(&[
            ("Path", dir.path().to_str().unwrap()),
            ("Type", "FileName"),
            ("ShowDotDot", "0"),
            ("ShowHidden", "0"),
            ("HideExtensions", "1"),
            ("SortType", "Name"),
            ("Count", "10"),
        ]);
        let ctx = context(&opts, &vars, &backend);
        assert_eq!(view.update(&ctx).value.string.as_deref(), Some("b"));
    }

    #[test]
    fn folder_info_respects_hidden_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("visible.txt"), "1").unwrap();
        fs::write(dir.path().join(".hidden"), "12345").unwrap();
        let backend = MockBackend::new();
        let vars = Variables::new();
        let opts = options(&[
            ("Folder", dir.path().to_str().unwrap()),
            ("InfoType", "FileCount"),
        ]);
        let ctx = context(&opts, &vars, &backend);
        assert_eq!(FolderInfoMeasure.update(&ctx).value.number, 1.0);

        let opts = options(&[
            ("Folder", dir.path().to_str().unwrap()),
            ("InfoType", "FileCount"),
            ("IncludeHiddenFiles", "1"),
        ]);
        let ctx = context(&opts, &vars, &backend);
        assert_eq!(FolderInfoMeasure.update(&ctx).value.number, 2.0);

        let opts = options(&[
            ("Folder", dir.path().to_str().unwrap()),
            ("InfoType", "FolderSizeStr"),
            ("IncludeHiddenFiles", "1"),
        ]);
        let ctx = context(&opts, &vars, &backend);
        assert_eq!(
            FolderInfoMeasure.update(&ctx).value.string.as_deref(),
            Some("6 B")
        );
    }

    #[test]
    fn run_command_streams_output_and_runs_finish_action_once() {
        let backend = MockBackend::new();
        let vars = Variables::new();
        let opts = options(&[
            ("Program", "printf hello"),
            ("FinishAction", "[!Update]"),
            ("Timeout", "5000"),
        ]);
        let ctx = context(&opts, &vars, &backend);
        let mut command = RunCommandMeasure::default();
        let _ = command.update(&ctx);
        let mut finished = false;
        for _ in 0..200 {
            std::thread::sleep(Duration::from_millis(10));
            let output = command.update(&ctx);
            if output.actions.iter().any(|action| action == "[!Update]") {
                assert_eq!(output.value.string.as_deref(), Some("hello"));
                finished = true;
                break;
            }
        }
        assert!(finished, "RunCommand doit terminer");
        let output = command.update(&ctx);
        assert!(output.actions.is_empty());
        assert_eq!(output.value.string.as_deref(), Some("hello"));
    }

    #[test]
    fn usage_monitor_process_counters_and_filters() {
        let pid = std::process::id();
        let sample = process_samples().remove(&pid).expect("processus courant");
        assert!(sample.handles > 0);
        assert!(sample.threads > 0);
        let pid_value = pid.to_string();
        let backend = MockBackend::new();
        let vars = Variables::new();
        let opts = options(&[
            ("Alias", "CUSTOM"),
            ("Category", "Process"),
            ("Counter", "Working Set - Private"),
            ("Name", &pid_value),
            ("RawValue", "1"),
        ]);
        let mut monitor = UsageMonitorMeasure::default();
        let output = monitor.update(&context(&opts, &vars, &backend));
        assert!(output.value.number > 0.0);

        let opts = options(&[
            ("Alias", "CUSTOM"),
            ("Category", "Process"),
            ("Counter", "Handle Count"),
            ("Name", &pid_value),
        ]);
        let output = monitor.update(&context(&opts, &vars, &backend));
        assert!(output.value.number > 0.0);

        let opts = options(&[
            ("Alias", "CUSTOM"),
            ("Category", "Process"),
            ("Counter", "Thread Count"),
            ("Name", &pid_value),
        ]);
        let output = monitor.update(&context(&opts, &vars, &backend));
        assert!(output.value.number > 0.0);
    }

    #[test]
    fn usage_monitor_custom_counter_and_perfmon_memory() {
        let backend = MockBackend::new();
        let vars = Variables::new();
        let opts = options(&[
            ("Alias", "CUSTOM"),
            ("Category", "System"),
            ("Counter", "Processes"),
        ]);
        let mut monitor = UsageMonitorMeasure::default();
        let output = monitor.update(&context(&opts, &vars, &backend));
        assert!(output.value.number > 0.0);

        let opts = options(&[
            ("PerfMonObject", "Memory"),
            ("PerfMonCounter", "Available MBytes"),
        ]);
        let mut perf = PerfMonMeasure::default();
        let output = perf.update(&context(&opts, &vars, &backend));
        assert_eq!(output.value.number, 0.5);
    }

    #[test]
    fn usage_monitor_ram_and_network() {
        let mut backend = MockBackend::new();
        backend.net = Some(NetSample {
            interface: "eth0".to_string(),
            rx_bytes: 1024,
            tx_bytes: 512,
        });
        let vars = Variables::new();
        let opts = options(&[("Alias", "RAM"), ("Percent", "1")]);
        let mut monitor = UsageMonitorMeasure::default();
        let output = monitor.update(&context(&opts, &vars, &backend));
        assert_eq!(output.value.number, 50.0);

        let opts = options(&[("Alias", "NETDOWN")]);
        let mut monitor = UsageMonitorMeasure::default();
        let first = monitor.update(&context(&opts, &vars, &backend));
        assert!(first.value.number >= 0.0);
        backend.net = Some(NetSample {
            interface: "eth0".to_string(),
            rx_bytes: 2048,
            tx_bytes: 512,
        });
        let second = monitor.update(&context(&opts, &vars, &backend));
        assert!(second.value.number >= 0.0);
    }

    #[test]
    fn quote_reads_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("quotes.txt");
        fs::write(&path, "Première\nDeuxième").unwrap();
        let backend = MockBackend::new();
        let vars = Variables::new();
        let opts = options(&[("PathName", path.to_str().unwrap()), ("Separator", "\n")]);
        let output = QuoteMeasure::default().update(&context(&opts, &vars, &backend));
        assert!(["Première", "Deuxième"].contains(&output.value.string.as_deref().unwrap()));
    }

    #[test]
    fn audio_level_rms_peak_and_channels() {
        let samples = [0.5, -0.5, 0.25, -0.25];
        let left = select_audio_channel(&samples, 2, AudioChannel::Index(0));
        assert_eq!(left, vec![0.5, 0.25]);
        assert!((audio_rms(&left) - 0.395_284_7).abs() < 1e-6);
        assert_eq!(audio_peak(&left), 0.5);
        let sum = select_audio_channel(&samples, 2, AudioChannel::Sum);
        assert_eq!(sum, vec![0.0, 0.0]);
        assert_eq!(parse_audio_channel("L"), AudioChannel::Index(0));
        assert_eq!(parse_audio_channel("Sum"), AudioChannel::Sum);
    }

    #[test]
    fn audio_level_fft_detects_tone() {
        let sample_rate = 44_100.0;
        let frequency = 1000.0;
        let samples: Vec<f32> = (0..1024)
            .map(|index| {
                (std::f64::consts::TAU * frequency * index as f64 / sample_rate).sin() as f32
            })
            .collect();
        let magnitudes = audio_fft(&samples, 1024);
        let (index, _) = magnitudes
            .iter()
            .enumerate()
            .max_by(|(_, left), (_, right)| left.total_cmp(right))
            .unwrap();
        let expected = (frequency * 1024.0 / sample_rate).round() as usize;
        assert!(index.abs_diff(expected) <= 1, "{index} != {expected}");
    }

    #[test]
    fn audio_level_overlap_and_parent_snapshot() {
        let sample_rate = 48_000.0;
        let frequency = 1000.0;
        let samples: Vec<f32> = (0..2048)
            .map(|index| {
                (std::f64::consts::TAU * frequency * index as f64 / sample_rate).sin() as f32
            })
            .collect();
        let overlap = audio_fft_overlap(&samples, 1024, 50.0);
        assert_eq!(overlap.len(), 512);
        let (index, _) = overlap
            .iter()
            .enumerate()
            .max_by(|(_, left), (_, right)| left.total_cmp(right))
            .unwrap();
        let expected = (frequency * 1024.0 / sample_rate).round() as usize;
        assert!(index.abs_diff(expected) <= 1, "{index} != {expected}");

        AUDIO_PARENTS.lock().unwrap().insert(
            "parent".to_string(),
            AudioSnapshot {
                samples: samples.clone(),
                channels: 2,
                sample_rate: 48_000,
                source: "test-source".to_string(),
            },
        );
        let backend = MockBackend::new();
        let vars = Variables::new();
        let opts = options(&[("Parent", "Parent"), ("Type", "Format")]);
        let ctx = context(&opts, &vars, &backend);
        let output = AudioLevelMeasure::default().update(&ctx);
        assert_eq!(output.value.string.as_deref(), Some("48000Hz PCM 32b 2ch"));
        AUDIO_PARENTS.lock().unwrap().remove("parent");
    }

    #[test]
    fn audio_level_band_range_is_logarithmic() {
        let backend = MockBackend::new();
        let vars = Variables::new();
        let opts = options(&[
            ("FFTSize", "1024"),
            ("Bands", "4"),
            ("FreqMin", "20"),
            ("FreqMax", "20000"),
            ("BandIdx", "1"),
        ]);
        let ctx = context(&opts, &vars, &backend);
        let samples = vec![0.0f32; 1024];
        let (_, low) = audio_band(&ctx, &samples, 44_100.0);
        assert!(low >= 20.0);
        assert!(low < 20000.0);
    }

    #[test]
    fn resmon_handles_and_input_text_visual_options() {
        let backend = MockBackend::new();
        let vars = Variables::new();
        let opts = options(&[("ResCountType", "HANDLE")]);
        let output = ResMonMeasure.update(&context(&opts, &vars, &backend));
        assert!(output.value.number > 0.0);
        for kind in ["GDI", "USER", "WINDOW"] {
            let opts = options(&[("ResCountType", kind)]);
            assert!(
                ResMonMeasure
                    .update(&context(&opts, &vars, &backend))
                    .value
                    .number
                    >= 0.0
            );
        }

        let entries = BTreeMap::from([
            ("W".to_string(), "320".to_string()),
            ("H".to_string(), "120".to_string()),
            ("FontFace".to_string(), "DejaVu Sans".to_string()),
            ("FontSize".to_string(), "14".to_string()),
            ("FontColor".to_string(), "#ffffff".to_string()),
            ("SolidColor".to_string(), "#000000".to_string()),
            ("StringStyle".to_string(), "Bold Italic".to_string()),
            ("StringAlign".to_string(), "Center".to_string()),
            ("TopMost".to_string(), "1".to_string()),
            ("FocusDismiss".to_string(), "1".to_string()),
        ]);
        let plan = parse_input_plan("Variable", &|key| entries.get(key).cloned()).unwrap();
        match plan {
            InputPlan::Variable { options, .. } => {
                assert_eq!(options.width, 320);
                assert_eq!(options.height, 120);
                assert!(options.top_most);
                assert!(options.focus_dismiss);
                assert_eq!(input_font(&options), "DejaVu Sans Bold Italic 14");
                assert!(input_markup("hi", &options).contains("font_weight='bold'"));
            }
            _ => panic!("plan variable attendu"),
        }
    }

    #[test]
    fn fileview_context_menu_and_icon_theme_lookup() {
        assert!(context_menu_items(true).contains(&"Ouvrir le dossier parent"));
        assert!(!context_menu_items(false).contains(&"Ouvrir le dossier parent"));
        assert_eq!(context_menu_items(true).last(), Some(&"Annuler"));

        let dir = tempfile::tempdir().unwrap();
        let theme = dir.path().join("hicolor");
        let category = theme.join("48x48/mimetypes");
        fs::create_dir_all(&category).unwrap();
        let icon = category.join("demo-icon.png");
        fs::write(&icon, b"png").unwrap();
        assert_eq!(find_icon_in_theme(&theme, "demo-icon", 32), Some(icon));
    }

    #[test]
    fn resmon_process_scope_and_usage_monitor_gpu_parsers() {
        let name = fs::read_to_string("/proc/self/comm")
            .unwrap()
            .trim()
            .to_string();
        assert!(process_pids(Some(&name)).contains(&std::process::id()));
        assert!(count_process_handles(Some(&name)) > 0);

        let mut samples = BTreeMap::new();
        merge_pmon_output(
            &mut samples,
            "# gpu pid type sm mem enc dec command\n0 1234 G 42 7 0 0 firefox\n",
        );
        merge_compute_apps(&mut samples, "1234, /usr/lib/firefox/firefox, 256\n");
        let sample = samples.get(&1234).unwrap();
        assert_eq!(sample.name, "firefox");
        assert_eq!(sample.utilization, 42.0);
        assert_eq!(sample.memory, 256 * 1024 * 1024);
        assert_eq!(
            counter_alias("GPU Process Memory", "Dedicated Usage"),
            Some("VRAM")
        );
        assert_eq!(
            counter_alias("GPU Engine", "Utilization Percentage"),
            Some("GPU")
        );
    }

    #[test]
    fn input_text_parses_batch_and_variable_commands() {
        let entries = BTreeMap::from([
            ("DefaultValue".to_string(), "initial".to_string()),
            (
                "Command1".to_string(),
                r#"[!SetVariable A "$UserInput$"]"#.to_string(),
            ),
            ("Command2".to_string(), "[!SetVariable B fixed]".to_string()),
            ("OnDismissAction".to_string(), "[!Update]".to_string()),
        ]);
        let plan = parse_input_plan("ExecuteBatch 1-2", &|key| entries.get(key).cloned()).unwrap();
        match plan {
            InputPlan::Batch(steps) => {
                assert_eq!(steps.len(), 2);
                assert!(steps[0].needs_input);
                assert!(!steps[1].needs_input);
                assert_eq!(steps[0].options.default_value, "initial");
                assert_eq!(steps[0].options.on_dismiss, "[!Update]");
            }
            _ => panic!("plan batch attendu"),
        }
        let plan = parse_input_plan("MyVariable", &|key| entries.get(key).cloned()).unwrap();
        assert!(matches!(plan, InputPlan::Variable { name, .. } if name == "MyVariable"));
        assert!(parse_input_plan("ExecuteBatch 3", &|key| entries.get(key).cloned()).is_none());
        assert_eq!(
            replace_user_input("a $USERINPUT$ b $UserInput$", "x"),
            "a x b x"
        );
        assert_eq!(parse_input_range("2-5"), (2, 5));
        assert_eq!(parse_input_range("All"), (1, 1_000_000_000));
    }

    #[test]
    fn window_message_posts_and_reads_values() {
        let key = format!(
            "petrichor-test-window-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let backend = MockBackend::new();
        let vars = Variables::new();
        let sender_opts = options(&[("WindowName", &key), ("WindowMessage", "16 5 7")]);
        let sender_ctx = context(&sender_opts, &vars, &backend);
        let mut sender = WindowMessageMeasure::default();
        let output = sender.update(&sender_ctx);
        assert_eq!(output.value.number, 5.0);
        assert!(sender.command("SendMessage 17 9 11", &sender_ctx));

        let receiver_opts = options(&[("WindowName", &key), ("WindowMessage", "17 0 0")]);
        let receiver_ctx = context(&receiver_opts, &vars, &backend);
        let mut receiver = WindowMessageMeasure::default();
        let output = receiver.update(&receiver_ctx);
        assert_eq!(output.value.number, 9.0);
        assert_eq!(output.value.string.as_deref(), Some("17 9 11"));
        assert!(!sender.command("Unknown 1 2 3", &sender_ctx));
        assert_eq!(parse_window_message("16 5 7"), Some((16, 5, 7)));
        assert!(parse_window_message("16 5").is_none());
    }

    #[test]
    fn action_timer_queues_actions() {
        let backend = MockBackend::new();
        let vars = Variables::new();
        let opts = options(&[("ActionList1", "Wait 1 | [!Update]")]);
        let ctx = context(&opts, &vars, &backend);
        let mut timer = ActionTimerMeasure::default();
        assert!(timer.command("Execute1", &ctx));
        std::thread::sleep(Duration::from_millis(30));
        let output = timer.update(&ctx);
        assert!(output.actions.iter().any(|action| action == "[!Update]"));
    }

    #[test]
    fn native_plugin_abi_loads() {
        if std::env::var("PETRICHOR_PLUGIN_PROBE").is_err() {
            return;
        }
        let path =
            PathBuf::from(std::env::var("PETRICHOR_PLUGIN_PATH").expect("PETRICHOR_PLUGIN_PATH"));
        let plugin = NativePlugin::load(&path).unwrap();
        let backend = MockBackend::new();
        let vars = Variables::new();
        let opts = options(&[("Message", "hello")]);
        let ctx = context(&opts, &vars, &backend);
        let mut measure = NativePluginMeasure::new(plugin);
        let output = measure.update(&ctx);
        assert_eq!(output.value.string.as_deref(), Some("hello"));
        assert!(measure.command("Bang", &ctx));
    }
}

#[derive(Clone)]
pub struct RuntimeSlot(Arc<Mutex<Box<dyn RuntimeMeasure>>>);

impl std::fmt::Debug for RuntimeSlot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("RuntimeSlot")
    }
}

impl RuntimeSlot {
    pub fn new(measure: Box<dyn RuntimeMeasure>) -> RuntimeSlot {
        RuntimeSlot(Arc::new(Mutex::new(measure)))
    }

    pub fn update(&self, context: &RuntimeContext<'_>) -> RuntimeOutput {
        self.0
            .lock()
            .map(|mut measure| measure.update(context))
            .unwrap_or_else(|_| RuntimeOutput::empty())
    }

    pub fn command(&self, command: &str, context: &RuntimeContext<'_>) -> bool {
        self.0
            .lock()
            .map(|mut measure| measure.command(command, context))
            .unwrap_or(false)
    }

    pub fn call(&self, function: &str, context: &RuntimeContext<'_>) -> Option<String> {
        self.0
            .lock()
            .ok()
            .and_then(|mut measure| measure.call(function, context))
    }
}
