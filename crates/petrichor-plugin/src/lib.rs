use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};

use libloading::{Library, Symbol};

pub mod formula;
pub mod runner;

pub use runner::run_runner;

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("plugin introuvable : {0}")]
    NotFound(String),
    #[error("nom de plugin invalide : {0}")]
    InvalidName(String),
    #[error("symbole manquant dans {path} : {symbol}")]
    MissingSymbol { path: String, symbol: String },
    #[error("échec du chargement de {path} : {message}")]
    Load { path: String, message: String },
    #[error("{0}")]
    InvalidArgument(String),
}

pub type Result<T> = std::result::Result<T, PluginError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginVersion {
    V1,
    V2,
    V3,
    V4,
}

impl PluginVersion {
    pub fn from_int(value: u32) -> Option<PluginVersion> {
        match value {
            1 => Some(PluginVersion::V1),
            2 => Some(PluginVersion::V2),
            3 => Some(PluginVersion::V3),
            4 => Some(PluginVersion::V4),
            _ => None,
        }
    }

    pub fn as_int(self) -> u32 {
        match self {
            PluginVersion::V1 => 1,
            PluginVersion::V2 => 2,
            PluginVersion::V3 => 3,
            PluginVersion::V4 => 4,
        }
    }
}

pub trait PluginHost {
    fn read_string(&mut self, option: &str, default: &str, replace: bool) -> String;
    fn read_formula(&mut self, option: &str, default: f64) -> f64;
    fn execute(&mut self, action: &str);
    fn log(&mut self, level: i32, message: &str);
    fn get(&mut self, kind: &str) -> String;

    fn read_string_from_section(
        &mut self,
        _section: &str,
        option: &str,
        default: &str,
        replace: bool,
    ) -> String {
        self.read_string(option, default, replace)
    }

    fn read_formula_from_section(&mut self, _section: &str, option: &str, default: f64) -> f64 {
        self.read_formula(option, default)
    }

    fn read_int(&mut self, option: &str, default: i32) -> i32 {
        self.read_formula(option, f64::from(default)) as i32
    }

    fn read_double(&mut self, option: &str, default: f64) -> f64 {
        self.read_formula(option, default)
    }

    fn read_path(&mut self, option: &str, default: &str) -> String {
        self.read_string(option, default, true)
    }

    fn replace_variables(&mut self, text: &str) -> String {
        text.to_string()
    }

    fn path_to_absolute(&mut self, path: &str) -> String {
        path.to_string()
    }
}

#[repr(C)]
pub struct PetrichorHostApi {
    pub userdata: *mut c_void,
    pub read_string: Option<
        unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char, c_int) -> *const c_char,
    >,
    pub read_formula: Option<unsafe extern "C" fn(*mut c_void, *const c_char, f64) -> f64>,
    pub execute: Option<unsafe extern "C" fn(*mut c_void, *const c_char)>,
    pub log: Option<unsafe extern "C" fn(*mut c_void, c_int, *const c_char)>,
    pub get: Option<unsafe extern "C" fn(*mut c_void, *const c_char) -> *const c_char>,
    pub read_string_from_section: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *const c_char,
            *const c_char,
            *const c_char,
            c_int,
        ) -> *const c_char,
    >,
    pub read_formula_from_section:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char, f64) -> f64>,
    pub read_int: Option<unsafe extern "C" fn(*mut c_void, *const c_char, c_int) -> c_int>,
    pub read_double: Option<unsafe extern "C" fn(*mut c_void, *const c_char, f64) -> f64>,
    pub read_path:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> *const c_char>,
    pub replace_variables:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char) -> *const c_char>,
    pub path_to_absolute: Option<unsafe extern "C" fn(*mut c_void, *const c_char) -> *const c_char>,
}

type PluginVersionFn = unsafe extern "C" fn() -> u32;
type InitializeFn = unsafe extern "C" fn(*mut *mut c_void, *mut PetrichorHostApi);
type ReloadFn = unsafe extern "C" fn(*mut c_void, *mut PetrichorHostApi, *mut f64);
type UpdateFn = unsafe extern "C" fn(*mut c_void) -> f64;
type GetStringFn = unsafe extern "C" fn(*mut c_void) -> *const c_char;
type ExecuteBangFn = unsafe extern "C" fn(*mut c_void, *const c_char);
type FinalizeFn = unsafe extern "C" fn(*mut c_void);
type FunctionFn = unsafe extern "C" fn(*mut c_void, c_int, *const *const c_char) -> *const c_char;

struct HostBridge<'a> {
    host: &'a mut dyn PluginHost,
    buffers: Vec<CString>,
}

impl<'a> HostBridge<'a> {
    fn new(host: &'a mut dyn PluginHost) -> HostBridge<'a> {
        HostBridge {
            host,
            buffers: Vec::new(),
        }
    }

    fn pointer(&mut self) -> *mut c_void {
        self as *mut HostBridge<'a> as *mut c_void
    }

    fn store(&mut self, value: String) -> *const c_char {
        let value = CString::new(value.replace('\0', "")).unwrap_or_default();
        let pointer = value.as_ptr();
        self.buffers.push(value);
        pointer
    }
}

unsafe fn read_c_string(value: *const c_char) -> String {
    if value.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(value) }
            .to_string_lossy()
            .to_string()
    }
}

unsafe extern "C" fn host_read_string(
    userdata: *mut c_void,
    option: *const c_char,
    default: *const c_char,
    replace: c_int,
) -> *const c_char {
    let bridge = unsafe { &mut *(userdata as *mut HostBridge<'_>) };
    let option = unsafe { read_c_string(option) };
    let default = unsafe { read_c_string(default) };
    let value = bridge.host.read_string(&option, &default, replace != 0);
    bridge.store(value)
}

unsafe extern "C" fn host_read_formula(
    userdata: *mut c_void,
    option: *const c_char,
    default: f64,
) -> f64 {
    let bridge = unsafe { &mut *(userdata as *mut HostBridge<'_>) };
    let option = unsafe { read_c_string(option) };
    bridge.host.read_formula(&option, default)
}

unsafe extern "C" fn host_execute(userdata: *mut c_void, action: *const c_char) {
    let bridge = unsafe { &mut *(userdata as *mut HostBridge<'_>) };
    let action = unsafe { read_c_string(action) };
    bridge.host.execute(&action);
}

unsafe extern "C" fn host_log(userdata: *mut c_void, level: c_int, message: *const c_char) {
    let bridge = unsafe { &mut *(userdata as *mut HostBridge<'_>) };
    let message = unsafe { read_c_string(message) };
    bridge.host.log(level, &message);
}

unsafe extern "C" fn host_get(userdata: *mut c_void, kind: *const c_char) -> *const c_char {
    let bridge = unsafe { &mut *(userdata as *mut HostBridge<'_>) };
    let kind = unsafe { read_c_string(kind) };
    let value = bridge.host.get(&kind);
    bridge.store(value)
}

unsafe extern "C" fn host_read_string_from_section(
    userdata: *mut c_void,
    section: *const c_char,
    option: *const c_char,
    default: *const c_char,
    replace: c_int,
) -> *const c_char {
    let bridge = unsafe { &mut *(userdata as *mut HostBridge<'_>) };
    let section = unsafe { read_c_string(section) };
    let option = unsafe { read_c_string(option) };
    let default = unsafe { read_c_string(default) };
    let value = bridge
        .host
        .read_string_from_section(&section, &option, &default, replace != 0);
    bridge.store(value)
}

unsafe extern "C" fn host_read_formula_from_section(
    userdata: *mut c_void,
    section: *const c_char,
    option: *const c_char,
    default: f64,
) -> f64 {
    let bridge = unsafe { &mut *(userdata as *mut HostBridge<'_>) };
    let section = unsafe { read_c_string(section) };
    let option = unsafe { read_c_string(option) };
    bridge
        .host
        .read_formula_from_section(&section, &option, default)
}

unsafe extern "C" fn host_read_int(
    userdata: *mut c_void,
    option: *const c_char,
    default: c_int,
) -> c_int {
    let bridge = unsafe { &mut *(userdata as *mut HostBridge<'_>) };
    let option = unsafe { read_c_string(option) };
    bridge.host.read_int(&option, default)
}

unsafe extern "C" fn host_read_double(
    userdata: *mut c_void,
    option: *const c_char,
    default: f64,
) -> f64 {
    let bridge = unsafe { &mut *(userdata as *mut HostBridge<'_>) };
    let option = unsafe { read_c_string(option) };
    bridge.host.read_double(&option, default)
}

unsafe extern "C" fn host_read_path(
    userdata: *mut c_void,
    option: *const c_char,
    default: *const c_char,
) -> *const c_char {
    let bridge = unsafe { &mut *(userdata as *mut HostBridge<'_>) };
    let option = unsafe { read_c_string(option) };
    let default = unsafe { read_c_string(default) };
    let value = bridge.host.read_path(&option, &default);
    bridge.store(value)
}

unsafe extern "C" fn host_replace_variables(
    userdata: *mut c_void,
    text: *const c_char,
) -> *const c_char {
    let bridge = unsafe { &mut *(userdata as *mut HostBridge<'_>) };
    let text = unsafe { read_c_string(text) };
    let value = bridge.host.replace_variables(&text);
    bridge.store(value)
}

unsafe extern "C" fn host_path_to_absolute(
    userdata: *mut c_void,
    path: *const c_char,
) -> *const c_char {
    let bridge = unsafe { &mut *(userdata as *mut HostBridge<'_>) };
    let path = unsafe { read_c_string(path) };
    let value = bridge.host.path_to_absolute(&path);
    bridge.store(value)
}

pub struct NativePlugin {
    _library: Library,
    path: PathBuf,
    data: *mut c_void,
    version: PluginVersion,
    host_api: Box<PetrichorHostApi>,
    initialize: InitializeFn,
    reload: Option<ReloadFn>,
    update: UpdateFn,
    get_string: Option<GetStringFn>,
    execute_bang: Option<ExecuteBangFn>,
    finalize: FinalizeFn,
    initialized: bool,
}

unsafe impl Send for NativePlugin {}

impl NativePlugin {
    pub fn load(path: &Path) -> Result<NativePlugin> {
        let library = unsafe { Library::new(path) }.map_err(|error| PluginError::Load {
            path: path.display().to_string(),
            message: error.to_string(),
        })?;
        let display = path.display().to_string();

        let version: PluginVersionFn = unsafe { symbol(&library, b"PluginVersion\0", &display)? };
        let initialize: InitializeFn = unsafe { symbol(&library, b"Initialize\0", &display)? };
        let update: UpdateFn = unsafe { symbol(&library, b"Update\0", &display)? };
        let finalize: FinalizeFn = unsafe { symbol(&library, b"Finalize\0", &display)? };
        let reload: Option<ReloadFn> = unsafe { optional_symbol(&library, b"Reload\0") };
        let get_string: Option<GetStringFn> = unsafe { optional_symbol(&library, b"GetString\0") };
        let execute_bang: Option<ExecuteBangFn> =
            unsafe { optional_symbol(&library, b"ExecuteBang\0") };
        let version = PluginVersion::from_int(unsafe { version() }).ok_or_else(|| {
            PluginError::InvalidArgument(format!("version de plugin inconnue dans {display}"))
        })?;

        Ok(NativePlugin {
            _library: library,
            path: path.to_path_buf(),
            data: std::ptr::null_mut(),
            version,
            host_api: Box::new(PetrichorHostApi {
                userdata: std::ptr::null_mut(),
                read_string: Some(host_read_string),
                read_formula: Some(host_read_formula),
                execute: Some(host_execute),
                log: Some(host_log),
                get: Some(host_get),
                read_string_from_section: Some(host_read_string_from_section),
                read_formula_from_section: Some(host_read_formula_from_section),
                read_int: Some(host_read_int),
                read_double: Some(host_read_double),
                read_path: Some(host_read_path),
                replace_variables: Some(host_replace_variables),
                path_to_absolute: Some(host_path_to_absolute),
            }),
            initialize,
            reload,
            update,
            get_string,
            execute_bang,
            finalize,
            initialized: false,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn version(&self) -> PluginVersion {
        self.version
    }

    pub fn initialize(&mut self, host: &mut dyn PluginHost) -> Result<()> {
        if self.initialized {
            return Ok(());
        }
        let mut bridge = HostBridge::new(host);
        self.host_api.userdata = bridge.pointer();
        unsafe {
            (self.initialize)(&mut self.data, &mut *self.host_api);
        }
        self.initialized = true;
        Ok(())
    }

    pub fn reload(&mut self, host: &mut dyn PluginHost) -> f64 {
        let Some(reload) = self.reload else {
            return 0.0;
        };
        let mut max_value = 0.0;
        let mut bridge = HostBridge::new(host);
        self.host_api.userdata = bridge.pointer();
        unsafe {
            reload(self.data, &mut *self.host_api, &mut max_value);
        }
        max_value
    }

    pub fn update(&mut self, host: &mut dyn PluginHost) -> f64 {
        let mut bridge = HostBridge::new(host);
        self.host_api.userdata = bridge.pointer();
        unsafe { (self.update)(self.data) }
    }

    pub fn read_string(&self) -> String {
        let Some(get_string) = self.get_string else {
            return String::new();
        };
        let pointer = unsafe { get_string(self.data) };
        unsafe { read_c_string(pointer) }
    }

    pub fn execute_bang(&mut self, host: &mut dyn PluginHost, args: &str) -> bool {
        let Some(execute_bang) = self.execute_bang else {
            return false;
        };
        let Ok(args) = CString::new(args.replace('\0', "")) else {
            return false;
        };
        let mut bridge = HostBridge::new(host);
        self.host_api.userdata = bridge.pointer();
        unsafe {
            execute_bang(self.data, args.as_ptr());
        }
        true
    }

    pub fn call(
        &mut self,
        host: &mut dyn PluginHost,
        function: &str,
        arguments: &[String],
    ) -> Option<String> {
        if !valid_function(function) || reserved_function(function) {
            return None;
        }
        let name = format!("{function}\0");
        let symbol: Symbol<FunctionFn> =
            unsafe { self._library.get::<FunctionFn>(name.as_bytes()) }.ok()?;
        let function = *symbol;
        let arguments: Vec<CString> = arguments
            .iter()
            .map(|argument| CString::new(argument.replace('\0', "")).unwrap_or_default())
            .collect();
        let pointers: Vec<*const c_char> =
            arguments.iter().map(|argument| argument.as_ptr()).collect();
        let mut bridge = HostBridge::new(host);
        self.host_api.userdata = bridge.pointer();
        let pointer = unsafe { function(self.data, pointers.len() as c_int, pointers.as_ptr()) };
        let value = unsafe { read_c_string(pointer) };
        Some(value)
    }
}

fn valid_function(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn reserved_function(name: &str) -> bool {
    [
        "Initialize",
        "Reload",
        "Update",
        "GetString",
        "ExecuteBang",
        "Finalize",
        "GetPluginAuthor",
        "GetPluginVersion",
    ]
    .iter()
    .any(|reserved| name.eq_ignore_ascii_case(reserved))
}

impl Drop for NativePlugin {
    fn drop(&mut self) {
        if self.initialized && !self.data.is_null() {
            unsafe {
                (self.finalize)(self.data);
            }
        }
        self.data = std::ptr::null_mut();
    }
}

unsafe fn symbol<T: Copy>(library: &Library, name: &[u8], path: &str) -> Result<T> {
    unsafe { library.get::<T>(name) }
        .map(|symbol| *symbol)
        .map_err(|_| PluginError::MissingSymbol {
            path: path.to_string(),
            symbol: String::from_utf8_lossy(name)
                .trim_end_matches('\0')
                .to_string(),
        })
}

unsafe fn optional_symbol<T: Copy>(library: &Library, name: &[u8]) -> Option<T> {
    unsafe { library.get::<T>(name) }.map(|symbol| *symbol).ok()
}

pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(PluginError::InvalidName("nom de plugin vide".into()));
    }
    if !name
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '.')
    {
        return Err(PluginError::InvalidName(name.to_string()));
    }
    Ok(())
}

pub fn candidate_paths(name: &str, roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for root in roots {
        for extension in ["so", "plugin.so"] {
            paths.push(root.join(format!("{name}.{extension}")));
            paths.push(root.join("Plugins").join(format!("{name}.{extension}")));
        }
    }
    paths
}

pub fn find_plugin(name: &str, roots: &[PathBuf]) -> Result<PathBuf> {
    validate_name(name)?;
    candidate_paths(name, roots)
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| PluginError::NotFound(name.to_string()))
}
