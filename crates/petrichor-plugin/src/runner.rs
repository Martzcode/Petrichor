use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{json, Value as JsonValue};

use crate::{NativePlugin, PluginHost};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RunnerContext {
    #[serde(default)]
    pub skin_name: String,
    #[serde(default)]
    pub root: PathBuf,
    #[serde(default)]
    pub skin_path: PathBuf,
    #[serde(default)]
    pub measure_name: String,
    #[serde(default)]
    pub options: BTreeMap<String, String>,
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
    #[serde(default)]
    pub sections: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(default)]
    pub values: BTreeMap<String, RunnerValue>,
    #[serde(default)]
    pub window_handle: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RunnerValue {
    #[serde(default)]
    pub number: f64,
    #[serde(default)]
    pub string: Option<String>,
}

impl RunnerContext {
    fn option(&self, name: &str) -> Option<&str> {
        self.options
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    fn expand(&self, input: &str) -> String {
        let mut output = String::with_capacity(input.len());
        let chars: Vec<char> = input.chars().collect();
        let mut index = 0usize;
        while index < chars.len() {
            if (chars[index] == '#' || chars[index] == '$')
                && index + 1 < chars.len()
                && chars[index + 1] != '*'
            {
                let closing = if chars[index] == '#' { '#' } else { '$' };
                if let Some(offset) = chars[index + 1..].iter().position(|ch| *ch == closing) {
                    let name: String = chars[index + 1..index + 1 + offset].iter().collect();
                    if let Some(value) = self
                        .variables
                        .iter()
                        .find(|(key, _)| key.eq_ignore_ascii_case(&name))
                        .map(|(_, value)| value)
                    {
                        output.push_str(value);
                        index += offset + 2;
                        continue;
                    }
                }
            }
            output.push(chars[index]);
            index += 1;
        }
        output
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum RunnerRequest {
    Init {
        path: PathBuf,
        context: RunnerContext,
    },
    Update {
        context: RunnerContext,
    },
    Bang {
        context: RunnerContext,
        command: String,
    },
    Call {
        context: RunnerContext,
        function: String,
        #[serde(default)]
        arguments: Vec<String>,
    },
    Shutdown,
}

#[derive(Default)]
struct RunnerHost {
    context: RunnerContext,
    actions: Vec<String>,
}

impl RunnerHost {
    fn set_context(&mut self, mut context: RunnerContext) {
        if context.sections.is_empty() {
            context.sections = std::mem::take(&mut self.context.sections);
        }
        if context.values.is_empty() {
            context.values = std::mem::take(&mut self.context.values);
        }
        self.context = context;
        self.actions.clear();
    }

    fn section_value(&self, section: &str, option: &str) -> Option<&str> {
        self.context
            .sections
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(section))
            .and_then(|(_, entries)| {
                entries
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case(option))
                    .map(|(_, value)| value.as_str())
            })
    }

    fn take_actions(&mut self) -> Vec<String> {
        std::mem::take(&mut self.actions)
    }
}

impl PluginHost for RunnerHost {
    fn read_string(&mut self, option: &str, default: &str, replace: bool) -> String {
        let value = self.context.option(option).unwrap_or(default).to_string();
        if replace {
            self.context.expand(&value)
        } else {
            value
        }
    }

    fn read_formula(&mut self, option: &str, default: f64) -> f64 {
        self.context
            .option(option)
            .map(|value| self.context.expand(value))
            .and_then(|value| crate::formula::evaluate(&value).ok())
            .unwrap_or(default)
    }

    fn execute(&mut self, action: &str) {
        self.actions.push(self.context.expand(action));
    }

    fn log(&mut self, level: i32, message: &str) {
        if level >= 2 {
            eprintln!("[plugin:{}] {message}", self.context.measure_name);
        }
    }

    fn get(&mut self, kind: &str) -> String {
        match kind.trim().to_ascii_uppercase().as_str() {
            "MEASURENAME" => self.context.measure_name.clone(),
            "SKIN" => self.context.root.to_string_lossy().to_string(),
            "SETTINGSFILE" => settings_file().to_string_lossy().to_string(),
            "SKINNAME" => self.context.skin_name.clone(),
            "SKINWINDOWHANDLE" => self.context.window_handle.to_string(),
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
            .section_value(section, option)
            .unwrap_or(default)
            .to_string();
        if replace {
            self.context.expand(&value)
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
        let path = PathBuf::from(&value);
        if path.is_absolute() {
            value
        } else {
            self.context.root.join(path).to_string_lossy().to_string()
        }
    }

    fn replace_variables(&mut self, text: &str) -> String {
        let expanded = self.context.expand(text);
        let mut output = String::with_capacity(expanded.len());
        let mut rest = expanded.as_str();
        while let Some(start) = rest.find('[') {
            output.push_str(&rest[..start]);
            let remainder = &rest[start + 1..];
            let Some(end) = remainder.find(']') else {
                output.push_str(&rest[start..]);
                return output;
            };
            let name = remainder[..end].trim();
            match self
                .context
                .values
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value)
            {
                Some(value) => {
                    if let Some(string) = value.string.as_deref().filter(|s| !s.is_empty()) {
                        output.push_str(string);
                    } else {
                        output.push_str(&runner_number(value.number));
                    }
                }
                None => {
                    output.push('[');
                    output.push_str(&remainder[..end]);
                    output.push(']');
                }
            }
            rest = &remainder[end + 1..];
        }
        output.push_str(rest);
        output
    }

    fn path_to_absolute(&mut self, path: &str) -> String {
        let expanded = self.context.expand(path);
        let path = PathBuf::from(&expanded);
        if path.is_absolute() {
            expanded
        } else {
            self.context.root.join(path).to_string_lossy().to_string()
        }
    }
}

fn runner_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        value.to_string()
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

fn failure(message: impl Into<String>) -> JsonValue {
    json!({ "ok": false, "error": message.into() })
}

pub fn run_runner() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    let mut plugin: Option<NativePlugin> = None;
    let mut host = RunnerHost::default();
    let mut max_value = 0.0f64;

    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            break;
        };
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<RunnerRequest>(&line) {
            Ok(RunnerRequest::Init { path, context }) => {
                host.set_context(context);
                match NativePlugin::load(&path) {
                    Ok(mut loaded) => {
                        if let Err(error) = loaded.initialize(&mut host) {
                            failure(error.to_string())
                        } else {
                            max_value = loaded.reload(&mut host);
                            plugin = Some(loaded);
                            json!({
                                "ok": true,
                                "max": max_value,
                                "actions": host.take_actions(),
                            })
                        }
                    }
                    Err(error) => failure(error.to_string()),
                }
            }
            Ok(RunnerRequest::Update { context }) => {
                host.set_context(context);
                match plugin.as_mut() {
                    Some(loaded) => {
                        let number = loaded.update(&mut host);
                        let text = loaded.read_string();
                        json!({
                            "ok": true,
                            "number": number,
                            "max": max_value,
                            "text": text,
                            "actions": host.take_actions(),
                        })
                    }
                    None => failure("plugin non initialisé"),
                }
            }
            Ok(RunnerRequest::Bang { context, command }) => {
                host.set_context(context);
                let handled = plugin
                    .as_mut()
                    .map(|loaded| loaded.execute_bang(&mut host, &command))
                    .unwrap_or(false);
                json!({
                    "ok": true,
                    "handled": handled,
                    "actions": host.take_actions(),
                })
            }
            Ok(RunnerRequest::Call {
                context,
                function,
                arguments,
            }) => {
                host.set_context(context);
                match plugin.as_mut() {
                    Some(loaded) => json!({
                        "ok": true,
                        "text": loaded.call(&mut host, &function, &arguments),
                        "actions": host.take_actions(),
                    }),
                    None => failure("plugin non initialisé"),
                }
            }
            Ok(RunnerRequest::Shutdown) => {
                let _ = writeln!(output, "{}", json!({ "ok": true }));
                let _ = output.flush();
                break;
            }
            Err(error) => failure(format!("requête invalide : {error}")),
        };
        if writeln!(output, "{response}").is_err() {
            break;
        }
        if output.flush().is_err() {
            break;
        }
    }
}
