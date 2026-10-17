use std::path::PathBuf;

use petrichor_core::{build_skin, Registry};

fn demo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../skins-demo")
        .canonicalize()
        .expect("skins-demo introuvable")
}

#[test]
fn scans_demo_corpus() {
    let registry = Registry::scan(demo_root());
    let names: Vec<&str> = registry.folders.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"Clock"));
    assert!(names.contains(&"System"));
    assert!(registry.file_count() >= 2);
}

#[test]
fn loads_every_demo_skin_without_errors() {
    let registry = Registry::scan(demo_root());
    let mut loaded = 0;

    for folder in &registry.folders {
        for file in &folder.files {
            let (skin, diagnostics) = build_skin(&file.path);
            assert!(
                !diagnostics.has_errors(),
                "erreurs dans {}: {:?}",
                file.path.display(),
                diagnostics.items
            );
            assert!(!skin.name.is_empty());
            loaded += 1;
        }
    }

    assert!(loaded >= 2, "aucun skin de démonstration chargé");
}

#[test]
fn resolves_included_styles_in_demo() {
    let registry = Registry::scan(demo_root());
    let folder = registry.folder_by_name("System").unwrap();
    let file = folder.file("Monitor.ini").unwrap();
    let (skin, diagnostics) = build_skin(&file.path);

    assert!(!diagnostics.has_errors(), "{:?}", diagnostics.items);
    assert_eq!(skin.metadata.name.as_deref(), Some("Moniteur"));
    assert!(skin.settings.dynamic_window_size);
    assert!(skin.meters.len() >= 4);
    assert!(skin.measures.len() >= 2);
    assert!(skin
        .styles
        .iter()
        .any(|s| s.name.eq_ignore_ascii_case("StyleLabel")));
    assert!(skin.included_files.len() >= 2);
}
