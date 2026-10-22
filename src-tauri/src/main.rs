#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().any(|argument| argument == "--plugin-runner") {
        petrichor_core::plugins::run_plugin_runner();
        return;
    }
    petrichor_lib::run()
}
