use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mlua::{Function, Lua, MultiValue, Table, Value, Variadic};

#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("chemin de script invalide : {0}")]
    InvalidPath(String),
    #[error("script introuvable : {0}")]
    NotFound(String),
    #[error("erreur Lua : {0}")]
    Lua(String),
}

pub type Result<T> = std::result::Result<T, ScriptError>;

#[derive(Debug, Clone, Default)]
pub struct ScriptMeasureValue {
    pub number: f64,
    pub string: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

impl ScriptMeasureValue {
    pub fn relative(&self) -> f64 {
        match (self.min, self.max) {
            (Some(min), Some(max)) if max > min => {
                ((self.number - min) / (max - min)).clamp(0.0, 1.0)
            }
            _ => 0.0,
        }
    }

    pub fn formatted(&self, decimals: usize) -> String {
        if !self.string.is_empty() {
            return self.string.clone();
        }
        format!("{:.*}", decimals.min(32), self.number)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScriptMeter {
    pub name: String,
    pub options: BTreeMap<String, String>,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub text: String,
    pub hidden: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ScriptContext {
    pub measure_name: String,
    pub skin_name: String,
    pub skin_root: PathBuf,
    pub skin_path: PathBuf,
    pub options: BTreeMap<String, String>,
    pub variables: BTreeMap<String, String>,
    pub values: BTreeMap<String, ScriptMeasureValue>,
    pub meters: BTreeMap<String, ScriptMeter>,
    pub counter: f64,
}

#[derive(Debug, Clone, Default)]
pub struct ScriptOutput {
    pub value: ScriptMeasureValue,
    pub actions: Vec<String>,
    pub disabled: bool,
    pub error: Option<String>,
}

#[derive(Debug, Default)]
struct HostState {
    context: ScriptContext,
    actions: Vec<String>,
    disabled: bool,
}

pub struct ScriptEngine {
    lua: Lua,
    state: Arc<Mutex<HostState>>,
    path: PathBuf,
}

impl ScriptEngine {
    pub fn load(path: &Path) -> Result<ScriptEngine> {
        if !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("lua"))
        {
            return Err(ScriptError::InvalidPath(path.display().to_string()));
        }
        let source = std::fs::read_to_string(path)
            .map_err(|_| ScriptError::NotFound(path.display().to_string()))?;
        let lua = Lua::new();
        sandbox(&lua)?;
        let state = Arc::new(Mutex::new(HostState::default()));
        install_objects(&lua, state.clone())?;
        lua.load(&source)
            .set_name(path.to_string_lossy().as_ref())
            .exec()
            .map_err(lua_error)?;
        let globals = lua.globals();
        if let Ok(initialize) = globals.get::<Function>("Initialize") {
            initialize.call::<()>(()).map_err(lua_error)?;
        }
        Ok(ScriptEngine {
            lua,
            state,
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn update(&mut self, context: &ScriptContext) -> ScriptOutput {
        {
            let mut state = self.state.lock().expect("état de script");
            state.context = context.clone();
            state.actions.clear();
            state.disabled = false;
        }
        let globals = self.lua.globals();
        sync_properties(&globals, context);
        let mut output = ScriptOutput::default();
        let mut has_number = false;
        let mut has_string = false;
        if let Ok(update) = globals.get::<Function>("Update") {
            match update.call::<Value>(()) {
                Ok(Value::Number(number)) => {
                    output.value.number = number;
                    has_number = true;
                }
                Ok(Value::Integer(number)) => {
                    output.value.number = number as f64;
                    has_number = true;
                }
                Ok(Value::Boolean(value)) => {
                    output.value.number = if value { 1.0 } else { 0.0 };
                    has_number = true;
                }
                Ok(Value::String(text)) => {
                    output.value.string = text.to_string_lossy().to_string();
                    has_string = true;
                }
                Ok(_) => {}
                Err(error) => output.error = Some(error.to_string()),
            }
        }
        if let Ok(get_string) = globals.get::<Function>("GetStringValue") {
            match get_string.call::<Value>(()) {
                Ok(Value::String(text)) => {
                    output.value.string = text.to_string_lossy().to_string();
                    has_string = true;
                }
                Ok(Value::Number(number)) => {
                    output.value.string = format_number(number);
                    has_string = true;
                }
                Ok(_) => {}
                Err(error) => output.error = Some(error.to_string()),
            }
        }
        if let Some(value) = context.values.get(&context.measure_name) {
            if !has_string {
                output.value.string = value.string.clone();
            }
            if !has_number {
                output.value.number = value.number;
            }
            output.value.min = value.min;
            output.value.max = value.max;
        }
        let mut state = self.state.lock().expect("état de script");
        output.actions = std::mem::take(&mut state.actions);
        output.disabled = state.disabled;
        output
    }

    pub fn call(&mut self, function: &str, arguments: &[String]) -> Option<String> {
        let globals = self.lua.globals();
        let function: Function = globals.get(function).ok()?;
        let mut values = Vec::with_capacity(arguments.len());
        for argument in arguments {
            values.push(parse_argument(&self.lua, argument).ok()?);
        }
        let result = function.call::<Value>(MultiValue::from_vec(values)).ok()?;
        Some(format_value(&result))
    }

    pub fn actions(&self) -> Vec<String> {
        self.state
            .lock()
            .map(|state| state.actions.clone())
            .unwrap_or_default()
    }
}

fn sync_properties(globals: &Table, context: &ScriptContext) {
    let Ok(properties) = globals.get::<Table>("PROPERTIES") else {
        return;
    };
    let keys: Vec<String> = properties
        .clone()
        .pairs::<String, Value>()
        .filter_map(|pair| pair.ok().map(|(key, _)| key))
        .collect();
    for key in keys {
        if let Some(value) = context
            .options
            .iter()
            .find(|(option, _)| option.eq_ignore_ascii_case(&key))
            .map(|(_, value)| value)
        {
            if !value.is_empty() {
                let _ = properties.set(key, value.clone());
            }
        }
    }
}

fn sandbox(lua: &Lua) -> Result<()> {
    let globals = lua.globals();
    for name in [
        "os",
        "io",
        "package",
        "debug",
        "dofile",
        "loadfile",
        "load",
        "loadstring",
        "require",
        "collectgarbage",
        "getfenv",
        "setfenv",
        "newproxy",
        "module",
        "print",
    ] {
        globals
            .set(name, Value::Nil)
            .map_err(|error| ScriptError::Lua(error.to_string()))?;
    }
    let log = lua
        .create_function(|_, _message: Variadic<Value>| Ok(()))
        .map_err(lua_error)?;
    globals.set("print", log).map_err(lua_error)?;
    if let Ok(string) = globals.get::<Table>("string") {
        string
            .set("dump", Value::Nil)
            .map_err(|error| ScriptError::Lua(error.to_string()))?;
    }
    Ok(())
}

fn install_objects(lua: &Lua, state: Arc<Mutex<HostState>>) -> Result<()> {
    let skin = lua.create_table().map_err(lua_error)?;
    install_skin_methods(lua, &skin, state.clone())?;
    lua.globals().set("SKIN", skin).map_err(lua_error)?;

    let self_table = lua.create_table().map_err(lua_error)?;
    install_measure_methods(lua, &self_table, state.clone(), None)?;
    lua.globals().set("SELF", self_table).map_err(lua_error)?;

    let meter = lua.create_table().map_err(lua_error)?;
    install_meter_methods(lua, &meter, state.clone(), None)?;
    lua.globals().set("Meter", meter).map_err(lua_error)?;

    let properties = lua.create_table().map_err(lua_error)?;
    lua.globals()
        .set("PROPERTIES", properties)
        .map_err(lua_error)?;
    Ok(())
}

fn install_skin_methods(lua: &Lua, skin: &Table, state: Arc<Mutex<HostState>>) -> Result<()> {
    let host = state.clone();
    skin.set(
        "Bang",
        lua.create_function(move |_, args: Variadic<Value>| {
            let args = without_self(args);
            let mut parts = Vec::new();
            for value in args {
                parts.push(match value {
                    Value::String(text) => text.to_string_lossy().to_string(),
                    Value::Number(number) => format_number(number),
                    Value::Boolean(value) => (if value { "1" } else { "0" }).to_string(),
                    _ => String::new(),
                });
            }
            let action = parts.join(" ");
            if let Ok(mut host) = host.lock() {
                host.actions.push(action);
            }
            Ok(())
        })
        .map_err(lua_error)?,
    )
    .map_err(lua_error)?;

    let host = state.clone();
    skin.set(
        "GetVariable",
        lua.create_function(move |_, args: Variadic<Value>| {
            let args = without_self(args);
            let name = string_arg(&args, 0).unwrap_or_default();
            let default = string_arg(&args, 1);
            let host = host.lock().map_err(|error| {
                mlua::Error::RuntimeError(format!("état de script verrouillé : {error}"))
            })?;
            Ok(host
                .context
                .variables
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(&name))
                .map(|(_, value)| value.clone())
                .or(default)
                .unwrap_or_default())
        })
        .map_err(lua_error)?,
    )
    .map_err(lua_error)?;

    let host = state.clone();
    skin.set(
        "ReplaceVariables",
        lua.create_function(move |_, args: Variadic<Value>| {
            let args = without_self(args);
            let text = string_arg(&args, 0).unwrap_or_default();
            let host = host.lock().map_err(|error| {
                mlua::Error::RuntimeError(format!("état de script verrouillé : {error}"))
            })?;
            Ok(replace_variables(&text, &host.context))
        })
        .map_err(lua_error)?,
    )
    .map_err(lua_error)?;

    let host = state.clone();
    skin.set(
        "ParseFormula",
        lua.create_function(move |_, args: Variadic<Value>| {
            let args = without_self(args);
            let text = string_arg(&args, 0).unwrap_or_default();
            let host = host.lock().map_err(|error| {
                mlua::Error::RuntimeError(format!("état de script verrouillé : {error}"))
            })?;
            let expanded = replace_variables(&text, &host.context);
            Ok(eval_formula(&expanded).unwrap_or(0.0))
        })
        .map_err(lua_error)?,
    )
    .map_err(lua_error)?;

    for name in ["GetW", "GetH", "GetX", "GetY"] {
        skin.set(
            name,
            lua.create_function(move |_, _args: Variadic<Value>| Ok(0.0))
                .map_err(lua_error)?,
        )
        .map_err(lua_error)?;
    }

    let host = state.clone();
    skin.set(
        "MakePathAbsolute",
        lua.create_function(move |_, args: Variadic<Value>| {
            let args = without_self(args);
            let text = string_arg(&args, 0).unwrap_or_default();
            let host = host.lock().map_err(|error| {
                mlua::Error::RuntimeError(format!("état de script verrouillé : {error}"))
            })?;
            let path = PathBuf::from(text);
            Ok(if path.is_absolute() {
                path
            } else {
                host.context.skin_root.join(path)
            }
            .to_string_lossy()
            .to_string())
        })
        .map_err(lua_error)?,
    )
    .map_err(lua_error)?;

    let host = state.clone();
    skin.set(
        "MoveWindow",
        lua.create_function(move |_, args: Variadic<Value>| {
            let args = without_self(args);
            let x = number_arg(&args, 0).unwrap_or(0.0);
            let y = number_arg(&args, 1).unwrap_or(0.0);
            if let Ok(mut host) = host.lock() {
                host.actions.push(format!(
                    "[!SetWindowPosition \"{}\" \"{}\"]",
                    format_number(x),
                    format_number(y)
                ));
            }
            Ok(())
        })
        .map_err(lua_error)?,
    )
    .map_err(lua_error)?;

    let host = state.clone();
    skin.set(
        "FadeWindow",
        lua.create_function(move |_, args: Variadic<Value>| {
            let args = without_self(args);
            let to = number_arg(&args, 1).unwrap_or(0.0).clamp(0.0, 255.0);
            if let Ok(mut host) = host.lock() {
                host.actions
                    .push(format!("[!SetTransparency \"{}\"]", format_number(to)));
            }
            Ok(())
        })
        .map_err(lua_error)?,
    )
    .map_err(lua_error)?;

    let host = state.clone();
    skin.set(
        "GetMeasure",
        lua.create_function(move |lua, args: Variadic<Value>| -> mlua::Result<Table> {
            let name = string_arg(&without_self(args), 0).unwrap_or_default();
            let table = lua.create_table()?;
            install_measure_methods(lua, &table, host.clone(), Some(name))
                .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;
            Ok(table)
        })
        .map_err(lua_error)?,
    )
    .map_err(lua_error)?;

    let host = state.clone();
    skin.set(
        "GetMeter",
        lua.create_function(move |lua, args: Variadic<Value>| -> mlua::Result<Table> {
            let name = string_arg(&without_self(args), 0).unwrap_or_default();
            let table = lua.create_table()?;
            install_meter_methods(lua, &table, host.clone(), Some(name))
                .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;
            Ok(table)
        })
        .map_err(lua_error)?,
    )
    .map_err(lua_error)?;

    Ok(())
}

fn resolve_measure_name(host: &HostState, explicit: Option<&str>) -> String {
    explicit
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| host.context.measure_name.clone())
}

fn resolve_value(host: &HostState, explicit: Option<&str>) -> ScriptMeasureValue {
    let name = resolve_measure_name(host, explicit);
    host.context
        .values
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(&name))
        .map(|(_, value)| value.clone())
        .unwrap_or_default()
}

fn install_measure_methods(
    lua: &Lua,
    table: &Table,
    state: Arc<Mutex<HostState>>,
    explicit_name: Option<String>,
) -> Result<()> {
    table
        .set(
            "GetName",
            lua.create_function({
                let host = state.clone();
                let name = explicit_name.clone();
                move |_, _args: Variadic<Value>| {
                    let host = host.lock().map_err(|error| {
                        mlua::Error::RuntimeError(format!("état de script verrouillé : {error}"))
                    })?;
                    Ok(resolve_measure_name(&host, name.as_deref()))
                }
            })
            .map_err(lua_error)?,
        )
        .map_err(lua_error)?;

    table
        .set(
            "GetOption",
            lua.create_function({
                let host = state.clone();
                move |_, args: Variadic<Value>| {
                    let args = without_self(args);
                    let key = string_arg(&args, 0).unwrap_or_default();
                    let default = string_arg(&args, 1);
                    let replace = bool_arg(&args, 2).unwrap_or(false);
                    let host = host.lock().map_err(|error| {
                        mlua::Error::RuntimeError(format!("état de script verrouillé : {error}"))
                    })?;
                    let value = host
                        .context
                        .options
                        .iter()
                        .find(|(option, _)| option.eq_ignore_ascii_case(&key))
                        .map(|(_, value)| value.clone())
                        .or(default)
                        .unwrap_or_default();
                    Ok(if replace {
                        replace_variables(&value, &host.context)
                    } else {
                        value
                    })
                }
            })
            .map_err(lua_error)?,
        )
        .map_err(lua_error)?;

    table
        .set(
            "GetNumberOption",
            lua.create_function({
                let host = state.clone();
                move |_, args: Variadic<Value>| {
                    let args = without_self(args);
                    let key = string_arg(&args, 0).unwrap_or_default();
                    let default = number_arg(&args, 1);
                    let host = host.lock().map_err(|error| {
                        mlua::Error::RuntimeError(format!("état de script verrouillé : {error}"))
                    })?;
                    Ok(host
                        .context
                        .options
                        .iter()
                        .find(|(option, _)| option.eq_ignore_ascii_case(&key))
                        .map(|(_, value)| eval_formula(value).unwrap_or(0.0))
                        .or(default)
                        .unwrap_or(0.0))
                }
            })
            .map_err(lua_error)?,
        )
        .map_err(lua_error)?;

    table
        .set(
            "GetValue",
            lua.create_function({
                let host = state.clone();
                let name = explicit_name.clone();
                move |_, _args: Variadic<Value>| {
                    let host = host.lock().map_err(|error| {
                        mlua::Error::RuntimeError(format!("état de script verrouillé : {error}"))
                    })?;
                    Ok(resolve_value(&host, name.as_deref()).number)
                }
            })
            .map_err(lua_error)?,
        )
        .map_err(lua_error)?;

    table
        .set(
            "GetRelativeValue",
            lua.create_function({
                let host = state.clone();
                let name = explicit_name.clone();
                move |_, _args: Variadic<Value>| {
                    let host = host.lock().map_err(|error| {
                        mlua::Error::RuntimeError(format!("état de script verrouillé : {error}"))
                    })?;
                    Ok(resolve_value(&host, name.as_deref()).relative())
                }
            })
            .map_err(lua_error)?,
        )
        .map_err(lua_error)?;

    table
        .set(
            "GetValueRange",
            lua.create_function({
                let host = state.clone();
                let name = explicit_name.clone();
                move |_, _args: Variadic<Value>| {
                    let host = host.lock().map_err(|error| {
                        mlua::Error::RuntimeError(format!("état de script verrouillé : {error}"))
                    })?;
                    let value = resolve_value(&host, name.as_deref());
                    Ok(format!(
                        "{} - {}",
                        format_number(value.min.unwrap_or(0.0)),
                        format_number(value.max.unwrap_or(0.0))
                    ))
                }
            })
            .map_err(lua_error)?,
        )
        .map_err(lua_error)?;

    table
        .set(
            "GetMinValue",
            lua.create_function({
                let host = state.clone();
                let name = explicit_name.clone();
                move |_, _args: Variadic<Value>| {
                    let host = host.lock().map_err(|error| {
                        mlua::Error::RuntimeError(format!("état de script verrouillé : {error}"))
                    })?;
                    Ok(resolve_value(&host, name.as_deref()).min.unwrap_or(0.0))
                }
            })
            .map_err(lua_error)?,
        )
        .map_err(lua_error)?;

    table
        .set(
            "GetMaxValue",
            lua.create_function({
                let host = state.clone();
                let name = explicit_name.clone();
                move |_, _args: Variadic<Value>| {
                    let host = host.lock().map_err(|error| {
                        mlua::Error::RuntimeError(format!("état de script verrouillé : {error}"))
                    })?;
                    Ok(resolve_value(&host, name.as_deref()).max.unwrap_or(0.0))
                }
            })
            .map_err(lua_error)?,
        )
        .map_err(lua_error)?;

    table
        .set(
            "GetStringValue",
            lua.create_function({
                let host = state.clone();
                let name = explicit_name.clone();
                move |_, args: Variadic<Value>| {
                    let args = without_self(args);
                    let decimals = number_arg(&args, 0)
                        .map(|value| value.max(0.0) as usize)
                        .unwrap_or(5);
                    let host = host.lock().map_err(|error| {
                        mlua::Error::RuntimeError(format!("état de script verrouillé : {error}"))
                    })?;
                    Ok(resolve_value(&host, name.as_deref()).formatted(decimals))
                }
            })
            .map_err(lua_error)?,
        )
        .map_err(lua_error)?;

    install_toggle_method(lua, table, state.clone(), "Disable", true)?;
    install_toggle_method(lua, table, state, "Enable", false)?;
    Ok(())
}

fn install_toggle_method(
    lua: &Lua,
    table: &Table,
    state: Arc<Mutex<HostState>>,
    name: &str,
    value: bool,
) -> Result<()> {
    table
        .set(
            name,
            lua.create_function(move |_, _args: Variadic<Value>| {
                if let Ok(mut host) = state.lock() {
                    host.disabled = value;
                }
                Ok(())
            })
            .map_err(lua_error)?,
        )
        .map_err(lua_error)?;
    Ok(())
}

fn resolve_meter(host: &HostState, explicit: Option<&str>) -> ScriptMeter {
    let name = resolve_measure_name(host, explicit);
    host.context
        .meters
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(&name))
        .map(|(_, meter)| meter.clone())
        .or_else(|| {
            host.context
                .meters
                .values()
                .find(|meter| meter.name.eq_ignore_ascii_case(&name))
                .cloned()
        })
        .unwrap_or_default()
}

fn install_meter_methods(
    lua: &Lua,
    table: &Table,
    state: Arc<Mutex<HostState>>,
    explicit_name: Option<String>,
) -> Result<()> {
    table
        .set(
            "GetName",
            lua.create_function({
                let host = state.clone();
                let name = explicit_name.clone();
                move |_, _args: Variadic<Value>| {
                    let host = host.lock().map_err(|error| {
                        mlua::Error::RuntimeError(format!("état de script verrouillé : {error}"))
                    })?;
                    Ok(resolve_meter(&host, name.as_deref()).name)
                }
            })
            .map_err(lua_error)?,
        )
        .map_err(lua_error)?;

    for (name, index) in [("GetW", 0), ("GetH", 1), ("GetX", 2), ("GetY", 3)] {
        table
            .set(
                name,
                lua.create_function({
                    let host = state.clone();
                    let explicit = explicit_name.clone();
                    move |_, _args: Variadic<Value>| {
                        let host = host.lock().map_err(|error| {
                            mlua::Error::RuntimeError(format!(
                                "état de script verrouillé : {error}"
                            ))
                        })?;
                        let meter = resolve_meter(&host, explicit.as_deref());
                        Ok(match index {
                            0 => meter.w,
                            1 => meter.h,
                            2 => meter.x,
                            _ => meter.y,
                        })
                    }
                })
                .map_err(lua_error)?,
            )
            .map_err(lua_error)?;
    }

    for (name, option) in [("SetW", "W"), ("SetH", "H"), ("SetX", "X"), ("SetY", "Y")] {
        table
            .set(
                name,
                lua.create_function({
                    let host = state.clone();
                    let explicit = explicit_name.clone();
                    move |_, args: Variadic<Value>| {
                        let value = without_self(args)
                            .iter()
                            .rev()
                            .find_map(|value| match value {
                                Value::Number(number) => Some(*number),
                                _ => None,
                            })
                            .unwrap_or(0.0);
                        let mut host = host.lock().map_err(|error| {
                            mlua::Error::RuntimeError(format!(
                                "état de script verrouillé : {error}"
                            ))
                        })?;
                        let meter = resolve_meter(&host, explicit.as_deref());
                        host.actions.push(format!(
                            "[!SetOption \"{}\" \"{}\" \"{}\"]",
                            meter.name,
                            option,
                            format_number(value)
                        ));
                        Ok(())
                    }
                })
                .map_err(lua_error)?,
            )
            .map_err(lua_error)?;
    }

    table
        .set(
            "Hide",
            lua.create_function({
                let host = state.clone();
                let explicit = explicit_name.clone();
                move |_, _args: Variadic<Value>| {
                    let mut host = host.lock().map_err(|error| {
                        mlua::Error::RuntimeError(format!("état de script verrouillé : {error}"))
                    })?;
                    let meter = resolve_meter(&host, explicit.as_deref());
                    host.actions
                        .push(format!("[!HideMeter \"{}\"]", meter.name));
                    Ok(())
                }
            })
            .map_err(lua_error)?,
        )
        .map_err(lua_error)?;

    table
        .set(
            "Show",
            lua.create_function({
                let host = state.clone();
                let explicit = explicit_name.clone();
                move |_, _args: Variadic<Value>| {
                    let mut host = host.lock().map_err(|error| {
                        mlua::Error::RuntimeError(format!("état de script verrouillé : {error}"))
                    })?;
                    let meter = resolve_meter(&host, explicit.as_deref());
                    host.actions
                        .push(format!("[!ShowMeter \"{}\"]", meter.name));
                    Ok(())
                }
            })
            .map_err(lua_error)?,
        )
        .map_err(lua_error)?;

    table
        .set(
            "SetText",
            lua.create_function({
                let host = state.clone();
                let explicit = explicit_name.clone();
                move |_, args: Variadic<Value>| {
                    let text = without_self(args)
                        .iter()
                        .rev()
                        .find_map(|value| match value {
                            Value::String(text) => Some(text.to_string_lossy().to_string()),
                            _ => None,
                        })
                        .unwrap_or_default();
                    let mut host = host.lock().map_err(|error| {
                        mlua::Error::RuntimeError(format!("état de script verrouillé : {error}"))
                    })?;
                    let meter = resolve_meter(&host, explicit.as_deref());
                    host.actions.push(format!(
                        "[!SetOption \"{}\" \"Text\" \"{}\"]",
                        meter.name, text
                    ));
                    Ok(())
                }
            })
            .map_err(lua_error)?,
        )
        .map_err(lua_error)?;

    table
        .set(
            "GetOption",
            lua.create_function({
                let host = state.clone();
                let explicit = explicit_name.clone();
                move |_, args: Variadic<Value>| {
                    let args = without_self(args);
                    let key = string_arg(&args, 0).unwrap_or_default();
                    let default = string_arg(&args, 1);
                    let host = host.lock().map_err(|error| {
                        mlua::Error::RuntimeError(format!("état de script verrouillé : {error}"))
                    })?;
                    let meter = resolve_meter(&host, explicit.as_deref());
                    Ok(meter
                        .options
                        .iter()
                        .find(|(option, _)| option.eq_ignore_ascii_case(&key))
                        .map(|(_, value)| value.clone())
                        .or(default)
                        .unwrap_or_default())
                }
            })
            .map_err(lua_error)?,
        )
        .map_err(lua_error)?;
    Ok(())
}

fn without_self(args: Variadic<Value>) -> Vec<Value> {
    let mut args = args.into_iter().collect::<Vec<Value>>();
    if matches!(args.first(), Some(Value::Table(_))) {
        args.remove(0);
    }
    args
}

fn string_arg(args: &[Value], index: usize) -> Option<String> {
    match args.get(index)? {
        Value::String(text) => Some(text.to_string_lossy().to_string()),
        Value::Number(number) => Some(format_number(*number)),
        Value::Integer(number) => Some(number.to_string()),
        Value::Boolean(value) => Some((if *value { "1" } else { "0" }).to_string()),
        _ => None,
    }
}

fn number_arg(args: &[Value], index: usize) -> Option<f64> {
    match args.get(index)? {
        Value::Number(number) => Some(*number),
        Value::Integer(number) => Some(*number as f64),
        Value::String(text) => text.to_string_lossy().trim().parse().ok(),
        Value::Boolean(value) => Some(if *value { 1.0 } else { 0.0 }),
        _ => None,
    }
}

fn bool_arg(args: &[Value], index: usize) -> Option<bool> {
    match args.get(index)? {
        Value::Boolean(value) => Some(*value),
        Value::Number(number) => Some(*number != 0.0),
        Value::Integer(number) => Some(*number != 0),
        Value::String(text) => Some(matches!(
            text.to_string_lossy().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes"
        )),
        _ => None,
    }
}

fn parse_argument(lua: &Lua, value: &str) -> mlua::Result<Value> {
    let value = value.trim();
    if let Some(text) = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    {
        return Ok(Value::String(lua.create_string(text)?));
    }
    if let Some(text) = value
        .strip_prefix('\'')
        .and_then(|value| value.strip_suffix('\''))
    {
        return Ok(Value::String(lua.create_string(text)?));
    }
    match value {
        "true" => Ok(Value::Boolean(true)),
        "false" => Ok(Value::Boolean(false)),
        "nil" => Ok(Value::Nil),
        _ => {
            if let Ok(number) = value.parse::<f64>() {
                Ok(Value::Number(number))
            } else {
                Ok(Value::String(lua.create_string(value)?))
            }
        }
    }
}

fn format_value(value: &Value) -> String {
    match value {
        Value::Nil => String::new(),
        Value::Boolean(value) => (if *value { "1" } else { "0" }).to_string(),
        Value::Integer(value) => value.to_string(),
        Value::Number(value) => format_number(*value),
        Value::String(value) => value.to_string_lossy().to_string(),
        _ => String::new(),
    }
}

fn format_number(number: f64) -> String {
    if !number.is_finite() {
        return "0".to_string();
    }
    let mut text = format!("{number:.5}");
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

fn replace_variables(text: &str, context: &ScriptContext) -> String {
    let mut output = text.to_string();
    for (name, value) in &context.variables {
        output = output.replace(&format!("#{name}#"), value);
        output = output.replace(&format!("[#{name}]"), value);
    }
    for (name, value) in &context.values {
        if !value.string.is_empty() {
            output = output.replace(&format!("[{name}]"), &value.string);
        }
    }
    output
}

fn lua_error(error: mlua::Error) -> ScriptError {
    ScriptError::Lua(error.to_string())
}

fn eval_formula(input: &str) -> Option<f64> {
    let mut parser = FormulaParser::new(input);
    let value = parser.expression()?;
    parser.skip_whitespace();
    if parser.peek().is_some() {
        None
    } else {
        Some(value)
    }
}

struct FormulaParser<'a> {
    input: &'a [u8],
    position: usize,
}

impl<'a> FormulaParser<'a> {
    fn new(input: &'a str) -> FormulaParser<'a> {
        FormulaParser {
            input: input.as_bytes(),
            position: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.position).copied()
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
            self.position += 1;
        }
    }

    fn expression(&mut self) -> Option<f64> {
        let mut value = self.term()?;
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(b'+') => {
                    self.position += 1;
                    value += self.term()?;
                }
                Some(b'-') => {
                    self.position += 1;
                    value -= self.term()?;
                }
                _ => return Some(value),
            }
        }
    }

    fn term(&mut self) -> Option<f64> {
        let mut value = self.factor()?;
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(b'*') => {
                    self.position += 1;
                    value *= self.factor()?;
                }
                Some(b'/') => {
                    self.position += 1;
                    let divisor = self.factor()?;
                    if divisor == 0.0 {
                        return None;
                    }
                    value /= divisor;
                }
                Some(b'%') => {
                    self.position += 1;
                    let divisor = self.factor()?;
                    if divisor == 0.0 {
                        return None;
                    }
                    value %= divisor;
                }
                _ => return Some(value),
            }
        }
    }

    fn factor(&mut self) -> Option<f64> {
        self.skip_whitespace();
        match self.peek()? {
            b'(' => {
                self.position += 1;
                let value = self.expression()?;
                self.skip_whitespace();
                (self.peek()? == b')').then_some(())?;
                self.position += 1;
                Some(value)
            }
            b'-' => {
                self.position += 1;
                Some(-self.factor()?)
            }
            b'+' => {
                self.position += 1;
                self.factor()
            }
            _ => {
                let start = self.position;
                while self
                    .peek()
                    .is_some_and(|byte| byte.is_ascii_digit() || byte == b'.')
                {
                    self.position += 1;
                }
                if start == self.position {
                    return None;
                }
                std::str::from_utf8(&self.input[start..self.position])
                    .ok()?
                    .parse()
                    .ok()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn context(path: &Path) -> ScriptContext {
        ScriptContext {
            measure_name: "MeasureScript".to_string(),
            skin_name: "Test".to_string(),
            skin_root: path.parent().unwrap().to_path_buf(),
            skin_path: path.to_path_buf(),
            options: BTreeMap::from([("Gain".to_string(), "2".to_string())]),
            variables: BTreeMap::from([("Name".to_string(), "Petrichor".to_string())]),
            values: BTreeMap::from([(
                "MeasureScript".to_string(),
                ScriptMeasureValue {
                    number: 42.0,
                    string: "42".to_string(),
                    min: Some(0.0),
                    max: Some(100.0),
                },
            )]),
            meters: BTreeMap::new(),
            counter: 1.0,
        }
    }

    #[test]
    fn rejects_non_lua_files() {
        assert!(ScriptEngine::load(Path::new("script.js")).is_err());
        assert!(ScriptEngine::load(Path::new("missing.lua")).is_err());
    }

    #[test]
    fn updates_and_returns_string() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Test.lua");
        fs::write(
            &path,
            "function Update() return 12.5 end\nfunction GetStringValue() return 'ok' end",
        )
        .unwrap();
        let mut script = ScriptEngine::load(&path).unwrap();
        let output = script.update(&context(&path));
        assert_eq!(output.value.number, 12.5);
        assert_eq!(output.value.string, "ok");
    }

    #[test]
    fn keeps_explicit_zero_and_empty_string() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Test.lua");
        fs::write(
            &path,
            "function Update() return 0 end\nfunction GetStringValue() return '' end",
        )
        .unwrap();
        let mut script = ScriptEngine::load(&path).unwrap();
        let output = script.update(&context(&path));
        assert_eq!(output.value.number, 0.0);
        assert_eq!(output.value.string, "");
    }

    #[test]
    fn reports_lua_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Test.lua");
        fs::write(&path, "function Update() error('boom') end").unwrap();
        let mut script = ScriptEngine::load(&path).unwrap();
        let output = script.update(&context(&path));
        assert!(output.error.as_deref().unwrap_or_default().contains("boom"));
    }

    #[test]
    fn exposes_skin_self_and_functions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Test.lua");
        fs::write(
            &path,
            r#"
            function Initialize() end
            function Update()
                SKIN:Bang('!SetVariable', 'Greeting', 'hello')
                return SELF:GetNumberOption('Gain', 1) * 2
            end
            function Greet(name)
                return name .. ':' .. SKIN:GetVariable('Name', '') .. ':' .. SELF:GetValue()
            end
            "#,
        )
        .unwrap();
        let mut script = ScriptEngine::load(&path).unwrap();
        let output = script.update(&context(&path));
        assert_eq!(output.value.number, 4.0);
        assert_eq!(output.value.string, "42");
        assert!(output
            .actions
            .iter()
            .any(|action| action.contains("Greeting")));
        assert_eq!(
            script.call("Greet", &["\"world\"".to_string()]).unwrap(),
            "world:Petrichor:42"
        );
    }

    #[test]
    fn sandbox_removes_dangerous_libraries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Test.lua");
        fs::write(
            &path,
            "function Update() return os == nil and io == nil and package == nil and collectgarbage == nil and load == nil and string.dump == nil and getfenv == nil and 1 or 0 end",
        )
        .unwrap();
        let mut script = ScriptEngine::load(&path).unwrap();
        let output = script.update(&context(&path));
        assert_eq!(output.value.number, 1.0);
    }

    #[test]
    fn injects_properties_from_options() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Test.lua");
        fs::write(
            &path,
            "PROPERTIES = { Gain = 0 }\nfunction Update() return tonumber(PROPERTIES.Gain) end",
        )
        .unwrap();
        let mut script = ScriptEngine::load(&path).unwrap();
        let output = script.update(&context(&path));
        assert_eq!(output.value.number, 2.0);
    }

    #[test]
    fn formula_parser_handles_precedence() {
        assert_eq!(eval_formula("2 + 3 * 4"), Some(14.0));
        assert_eq!(eval_formula("(2 + 3) * 4"), Some(20.0));
        assert_eq!(eval_formula("-5 + 2"), Some(-3.0));
        assert_eq!(eval_formula("1 / 0"), None);
    }

    #[test]
    fn formats_function_returns() {
        assert_eq!(format_value(&Value::Boolean(true)), "1");
        assert_eq!(format_value(&Value::Number(1.5)), "1.5");
        assert_eq!(format_number(1.234567), "1.23457");
    }
}
