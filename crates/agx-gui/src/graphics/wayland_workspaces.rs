//! Send desktop workspace changes to the build's Weston desktop shell.
use slint::ComponentHandle;
use std::{
    os::unix::{fs::FileTypeExt, net::UnixDatagram},
    path::PathBuf,
};

const SHELL_APP_ID: &str = "org.angi.Desktop";

pub(crate) fn identify_shell() -> Result<(), slint::PlatformError> {
    slint::set_xdg_app_id(SHELL_APP_ID)
}

pub(crate) fn select(index: usize) {
    send(&format!("S{index}"));
}

pub(crate) fn remove(index: usize, current: usize) {
    send(&format!("R{index} {current}"));
}

pub(crate) fn fullscreen_controls() -> Option<crate::FullscreenActionWindow> {
    let endpoint = socket_path(".angi2")?;
    if !std::fs::metadata(endpoint).is_ok_and(|info| info.file_type().is_socket()) {
        return None;
    }
    let controls = crate::FullscreenActionWindow::new().and_then(|controls| {
        controls.on_action(|action| send(&format!("F{action}")));
        controls.show()?;
        Ok(controls)
    });
    controls
        .map_err(|error| eprintln!("agx-gui: fullscreen controls: {error}"))
        .ok()
}

fn socket_path(suffix: &str) -> Option<PathBuf> {
    let display = std::env::var_os("WAYLAND_DISPLAY").filter(|name| !name.is_empty())?;
    let mut path = PathBuf::from(display);
    if !path.is_absolute() {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
        path = PathBuf::from(runtime).join(path);
    }
    let mut path = path.into_os_string();
    path.push(suffix);
    Some(path.into())
}

fn send(message: &str) {
    // Preview builds and stock compositors can run without the ANGI endpoint.
    if !super::using_winit() {
        return;
    }
    let Some(path) = socket_path(".angi2") else {
        return;
    };
    if let Err(error) = UnixDatagram::unbound().and_then(|socket| {
        socket.set_nonblocking(true)?;
        match socket.send_to(message.as_bytes(), &path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                socket.send_to(message.as_bytes(), path.with_extension("angi"))
            }
            result => result,
        }
    }) && !matches!(
        error.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
    ) {
        eprintln!("agx-gui: Wayland workspace change: {error}");
    }
}
