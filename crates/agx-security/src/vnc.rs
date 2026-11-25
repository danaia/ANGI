//! The desktop's VNC option. The server lives inside the desktop process and listens on
//! loopback only, with no VNC password, so SSH is its access control. This only edits the one
//! `AGX_GUI_VNC` line of `gui.env`; the root sign-in screen reads it and hands it to the session, so it applies at the next sign-in.
use crate::{Paths, policy::atomic_write};
use std::{fs, io};

const KEY: &str = "AGX_GUI_VNC";

fn value(text: &str) -> Option<&str> {
    text.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == KEY).then(|| value.trim())
    })
}

/// VNC is switched on in the configuration: `on`, a port, or a loopback address. Absent means off.
pub fn vnc_enabled(paths: &Paths) -> bool {
    fs::read_to_string(&paths.gui_env)
        .ok()
        .is_some_and(|text| value(&text).is_some_and(|value| !value.is_empty() && value != "off"))
}

/// Set `AGX_GUI_VNC=on` or `off`, keeping every other line. A port or address the person chose
/// by hand is kept when turning VNC on.
pub fn set_vnc(paths: &Paths, on: bool) -> io::Result<()> {
    let text = match fs::read_to_string(&paths.gui_env) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    let current = value(&text).filter(|value| *value != "off").map(str::to_owned);
    let wanted = if on { current.unwrap_or_else(|| "on".into()) } else { "off".into() };
    let mut lines: Vec<String> = text
        .lines()
        .filter(|line| line.split_once('=').is_none_or(|(key, _)| key.trim() != KEY))
        .map(str::to_owned)
        .collect();
    lines.push(format!("{KEY}={wanted}"));
    atomic_write(&paths.gui_env, (lines.join("\n") + "\n").as_bytes(), 0o600)
}
