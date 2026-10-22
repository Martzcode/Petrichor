use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use petrichor_plugin::{NativePlugin, PluginHost};

struct TestHost {
    options: BTreeMap<String, String>,
    sections: BTreeMap<String, BTreeMap<String, String>>,
    actions: Vec<String>,
    logs: Vec<(i32, String)>,
}

impl TestHost {
    fn new() -> TestHost {
        TestHost {
            options: BTreeMap::from([
                ("Message".to_string(), "hello".to_string()),
                ("Formula".to_string(), "2.5".to_string()),
            ]),
            sections: BTreeMap::from([(
                "Section".to_string(),
                BTreeMap::from([("Value".to_string(), "12".to_string())]),
            )]),
            actions: Vec::new(),
            logs: Vec::new(),
        }
    }
}

impl PluginHost for TestHost {
    fn read_string(&mut self, option: &str, default: &str, _replace: bool) -> String {
        self.options
            .get(option)
            .cloned()
            .unwrap_or_else(|| default.to_string())
    }

    fn read_formula(&mut self, option: &str, default: f64) -> f64 {
        self.options
            .get(option)
            .and_then(|value| value.parse().ok())
            .unwrap_or(default)
    }

    fn execute(&mut self, action: &str) {
        self.actions.push(action.to_string());
    }

    fn log(&mut self, level: i32, message: &str) {
        self.logs.push((level, message.to_string()));
    }

    fn get(&mut self, kind: &str) -> String {
        match kind {
            "MEASURENAME" => "DemoMeasure".to_string(),
            "SKINNAME" => "Demo".to_string(),
            _ => String::new(),
        }
    }

    fn read_string_from_section(
        &mut self,
        section: &str,
        option: &str,
        default: &str,
        _replace: bool,
    ) -> String {
        self.sections
            .get(section)
            .and_then(|values| values.get(option))
            .cloned()
            .unwrap_or_else(|| default.to_string())
    }

    fn read_formula_from_section(&mut self, section: &str, option: &str, default: f64) -> f64 {
        self.read_string_from_section(section, option, "", true)
            .parse()
            .unwrap_or(default)
    }

    fn read_path(&mut self, option: &str, default: &str) -> String {
        let value = self
            .options
            .get(option)
            .cloned()
            .unwrap_or_else(|| default.to_string());
        format!("/absolute/{value}")
    }

    fn replace_variables(&mut self, text: &str) -> String {
        text.replace("%NAME%", "bonjour")
    }

    fn path_to_absolute(&mut self, path: &str) -> String {
        format!("/absolute/{path}")
    }
}

#[test]
fn demo_plugin_compiles_and_runs_through_abi() {
    if Command::new("cc").arg("--version").output().is_err() {
        eprintln!("cc absent : test ABI ignoré");
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("demo.c");
    let library = directory.path().join("libpetrichor_demo.so");
    std::fs::write(&source, DEMO_PLUGIN).unwrap();
    let include = Path::new(env!("CARGO_MANIFEST_DIR")).join("include");
    let status = Command::new("cc")
        .args(["-shared", "-fPIC", "-Wall", "-Wextra", "-Werror"])
        .arg("-I")
        .arg(&include)
        .arg(&source)
        .arg("-o")
        .arg(&library)
        .status()
        .unwrap();
    assert!(status.success(), "compilation C du plugin de démonstration");

    let mut plugin = NativePlugin::load(&library).unwrap();
    assert_eq!(plugin.version().as_int(), 4);
    let mut host = TestHost::new();
    plugin.initialize(&mut host).unwrap();
    assert_eq!(plugin.reload(&mut host), 100.0);
    assert_eq!(plugin.update(&mut host), 2.0);
    assert_eq!(plugin.read_string(), "hello");
    assert!(plugin.execute_bang(&mut host, "Ping"));
    assert_eq!(host.actions, vec!["Ping".to_string()]);
    assert_eq!(
        plugin.call(&mut host, "Echo", &["abc".to_string()]),
        Some("abc".to_string())
    );
    assert_eq!(
        plugin.call(&mut host, "Formula", &[]),
        Some("2.5".to_string())
    );
    assert_eq!(
        plugin.call(&mut host, "Host", &[]),
        Some("12|2|2.500000|/absolute/PathValue|bonjour|/absolute/relative".to_string())
    );
    assert!(plugin.call(&mut host, "Missing", &[]).is_none());
    assert!(plugin.call(&mut host, "Update", &[]).is_none());
    drop(plugin);
}

const DEMO_PLUGIN: &str = r#"#include "petrichor_plugin.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct Demo {
    PetrichorHostApi *api;
    char message[256];
    char output[256];
    char host[512];
    double value;
} Demo;

uint32_t PluginVersion(void) {
    return PETRICHOR_PLUGIN_VERSION;
}

const char *PluginAuthor(void) {
    return "Petrichor tests";
}

void Initialize(void **data, PetrichorHostApi *api) {
    Demo *demo = (Demo *)calloc(1, sizeof(Demo));
    if (demo == NULL) {
        *data = NULL;
        return;
    }
    demo->api = api;
    demo->value = 1.0;
    *data = demo;
}

void Reload(void *data, PetrichorHostApi *api, double *max_value) {
    Demo *demo = (Demo *)data;
    if (demo == NULL) {
        return;
    }
    demo->api = api;
    const char *message = api->read_string(api->userdata, "Message", "default", 1);
    snprintf(demo->message, sizeof(demo->message), "%s", message);
    *max_value = api->read_formula(api->userdata, "Formula", 0.0) * 40.0;
    const char *section = api->read_string_from_section(
        api->userdata, "Section", "Value", "0", 0);
    int integer = api->read_int(api->userdata, "Formula", 0);
    double number = api->read_double(api->userdata, "Formula", 0.0);
    const char *path = api->read_path(api->userdata, "Path", "PathValue");
    const char *replaced = api->replace_variables(api->userdata, "%NAME%");
    const char *absolute = api->path_to_absolute(api->userdata, "relative");
    snprintf(demo->host, sizeof(demo->host), "%s|%d|%f|%s|%s|%s",
        section, integer, number, path, replaced, absolute);
}

double Update(void *data) {
    Demo *demo = (Demo *)data;
    if (demo == NULL) {
        return 0.0;
    }
    demo->value += 1.0;
    return demo->value;
}

const char *GetString(void *data) {
    Demo *demo = (Demo *)data;
    return demo == NULL ? "" : demo->message;
}

void ExecuteBang(void *data, const char *args) {
    Demo *demo = (Demo *)data;
    if (demo == NULL) {
        return;
    }
    snprintf(demo->output, sizeof(demo->output), "%s", args == NULL ? "" : args);
    demo->api->execute(demo->api->userdata, demo->output);
}

const char *Echo(void *data, int argc, const char *argv[]) {
    Demo *demo = (Demo *)data;
    if (demo == NULL || argc < 1 || argv == NULL) {
        return "";
    }
    snprintf(demo->output, sizeof(demo->output), "%s", argv[0]);
    return demo->output;
}

const char *Host(void *data, int argc, const char *argv[]) {
    Demo *demo = (Demo *)data;
    (void)argc;
    (void)argv;
    return demo == NULL ? "" : demo->host;
}

const char *Formula(void *data, int argc, const char *argv[]) {
    Demo *demo = (Demo *)data;
    (void)argc;
    (void)argv;
    if (demo == NULL) {
        return "";
    }
    snprintf(demo->output, sizeof(demo->output), "2.5");
    return demo->output;
}

void Finalize(void *data) {
    free(data);
}
"#;
