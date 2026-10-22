use std::process::{Command, Stdio};

pub fn send(title: &str, message: &str) {
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return;
    }
    if let Err(error) = Command::new("notify-send")
        .arg("--app-name=Petrichor")
        .arg(title)
        .arg(message)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        petrichor_core::logging::debug(format!("Notification indisponible : {error}"));
    }
}
