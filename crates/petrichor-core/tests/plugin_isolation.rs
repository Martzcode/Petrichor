use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use petrichor_core::model::{Measure, OptionEntry, Options};
use petrichor_core::plugins::{create_plugin_measure, RuntimeContext};
use petrichor_core::variables::Variables;

fn compile_plugin(directory: &Path, name: &str, source: &str) -> Option<PathBuf> {
    if Command::new("cc").arg("--version").output().is_err() {
        eprintln!("cc absent : test d'isolation ignoré");
        return None;
    }
    let source_path = directory.join(format!("{name}.c"));
    let library = directory.join(format!("lib{name}.so"));
    std::fs::write(&source_path, source).unwrap();
    let include = Path::new(env!("CARGO_MANIFEST_DIR")).join("../petrichor-plugin/include");
    let status = Command::new("cc")
        .args(["-shared", "-fPIC", "-Wall", "-Wextra", "-Werror"])
        .arg("-I")
        .arg(&include)
        .arg(&source_path)
        .arg("-o")
        .arg(&library)
        .status()
        .unwrap();
    assert!(status.success(), "compilation C de {name}");
    Some(library)
}

fn measure(plugin: &Path) -> Measure {
    let options = vec![
        ("Plugin", plugin.to_string_lossy().to_string()),
        ("Message", "bonjour".to_string()),
        ("Formula", "1+1".to_string()),
        ("Path", "relative.txt".to_string()),
    ];
    Measure {
        name: "Isolated".to_string(),
        kind: "Plugin".to_string(),
        line: 1,
        options: Options {
            entries: options
                .into_iter()
                .map(|(key, value)| OptionEntry {
                    key: key.to_string(),
                    value,
                    line: 1,
                })
                .collect(),
        },
    }
}

#[test]
fn plugin_runs_out_of_process_and_crashes_do_not_stop_the_host() {
    let directory = tempfile::tempdir().unwrap();
    let runner = env!("CARGO_BIN_EXE_petrichor-plugin-runner");
    std::env::set_var("PETRICHOR_PLUGIN_RUNNER", runner);
    let Some(plugin) = compile_plugin(directory.path(), "isolation_demo", DEMO_PLUGIN) else {
        return;
    };
    let Some(crash) = compile_plugin(directory.path(), "isolation_crash", CRASH_PLUGIN) else {
        return;
    };

    let backend = petrichor_sys::backend();
    let variables = Variables::new();
    let values = BTreeMap::from([(
        "other".to_string(),
        petrichor_core::reference::MeasureValue::text("value"),
    )]);
    let sections = BTreeMap::from([(
        "DemoSection".to_string(),
        BTreeMap::from([("Value".to_string(), "42".to_string())]),
    )]);
    let skin = measure(&plugin);
    let options = skin.options.clone();
    let context = RuntimeContext {
        skin_name: "Test",
        skin_root: directory.path(),
        skin_path: &directory.path().join("Test.ini"),
        measure_name: "Isolated",
        options: &options,
        variables: &variables,
        backend: &backend,
        values: &values,
        meters: &[],
        sections: &sections,
        skin_window_handle: 12345,
        counter: 1.0,
    };
    let mut runtime = create_plugin_measure(&skin, "Test", directory.path(), context.skin_path)
        .expect("mesure plugin");
    let output = runtime.update(&context);
    assert_eq!(output.value.number, 2.0);
    assert_eq!(output.value.string.as_deref(), Some("bonjour"));
    assert_eq!(output.value.max, Some(80.0));
    assert!(runtime.command("Ping", &context));
    let output = runtime.update(&context);
    assert!(output.actions.iter().any(|action| action == "Ping"));
    assert_eq!(runtime.call("Echo(abc)", &context).as_deref(), Some("abc"));
    let expected_host = format!(
        "42|2|2.000000|{}/relative.txt|value|{}/relative|12345",
        directory.path().display(),
        directory.path().display()
    );
    assert_eq!(
        runtime.call("Host", &context).as_deref(),
        Some(expected_host.as_str())
    );

    let crash_skin = measure(&crash);
    let crash_options = crash_skin.options.clone();
    let crash_context = RuntimeContext {
        skin_root: directory.path(),
        skin_path: &directory.path().join("Crash.ini"),
        measure_name: "Crash",
        options: &crash_options,
        ..context
    };
    let mut crash_runtime = create_plugin_measure(
        &crash_skin,
        "Test",
        directory.path(),
        crash_context.skin_path,
    )
    .expect("mesure plugin crash");
    let output = crash_runtime.update(&crash_context);
    assert_eq!(output.value.number, 0.0);
    let output = crash_runtime.update(&crash_context);
    assert_eq!(output.value.number, 0.0);
    let output = runtime.update(&context);
    assert_eq!(output.value.string.as_deref(), Some("bonjour"));
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

uint32_t PluginVersion(void) { return PETRICHOR_PLUGIN_VERSION; }

void Initialize(void **data, PetrichorHostApi *api) {
    Demo *demo = (Demo *)calloc(1, sizeof(Demo));
    demo->api = api;
    demo->value = 1.0;
    *data = demo;
}

void Reload(void *data, PetrichorHostApi *api, double *max_value) {
    Demo *demo = (Demo *)data;
    demo->api = api;
    const char *message = api->read_string(api->userdata, "Message", "default", 1);
    snprintf(demo->message, sizeof(demo->message), "%s", message);
    *max_value = api->read_formula(api->userdata, "Formula", 0.0) * 40.0;
    const char *section = api->read_string_from_section(
        api->userdata, "DemoSection", "Value", "0", 0);
    int integer = api->read_int(api->userdata, "Formula", 0);
    double number = api->read_double(api->userdata, "Formula", 0.0);
    const char *path = api->read_path(api->userdata, "Path", "default.txt");
    const char *replaced = api->replace_variables(api->userdata, "[Other]");
    const char *absolute = api->path_to_absolute(api->userdata, "relative");
    const char *window = api->get(api->userdata, "SKINWINDOWHANDLE");
    snprintf(demo->host, sizeof(demo->host), "%s|%d|%f|%s|%s|%s|%s",
        section, integer, number, path, replaced, absolute, window);
}

double Update(void *data) {
    Demo *demo = (Demo *)data;
    demo->value += 1.0;
    return demo->value;
}

const char *GetString(void *data) {
    Demo *demo = (Demo *)data;
    return demo->message;
}

void ExecuteBang(void *data, const char *args) {
    Demo *demo = (Demo *)data;
    snprintf(demo->output, sizeof(demo->output), "%s", args);
    demo->api->execute(demo->api->userdata, demo->output);
}

const char *Host(void *data, int argc, const char *argv[]) {
    Demo *demo = (Demo *)data;
    (void)argc;
    (void)argv;
    return demo == NULL ? "" : demo->host;
}

const char *Echo(void *data, int argc, const char *argv[]) {
    Demo *demo = (Demo *)data;
    if (argc < 1 || argv == NULL) {
        return "";
    }
    snprintf(demo->output, sizeof(demo->output), "%s", argv[0]);
    return demo->output;
}

void Finalize(void *data) { free(data); }
"#;

const CRASH_PLUGIN: &str = r#"#include "petrichor_plugin.h"
#include <stdlib.h>

uint32_t PluginVersion(void) { return PETRICHOR_PLUGIN_VERSION; }

void Initialize(void **data, PetrichorHostApi *api) {
    (void)api;
    *data = calloc(1, 1);
}

double Update(void *data) {
    (void)data;
    abort();
    return 0.0;
}

void Finalize(void *data) { free(data); }
"#;
