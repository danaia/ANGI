//! Startup display selection, explicit renderer overrides and cursor ownership.
#[cfg(any(feature = "kms", feature = "headless", test))]
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(any(feature = "kms", test))]
mod detection;
#[cfg(any(feature = "kms", test))]
mod kms_input;
#[cfg(feature = "wayland")]
pub(crate) mod native_tiles;
#[cfg(any(feature = "kms", test))]
mod pacing;
#[cfg(feature = "kms-provision-graphics")]
pub(crate) mod provisioned;
mod virtual_display;
#[cfg(feature = "wayland")]
pub(crate) mod wayland_workspaces;

pub const ENVIRONMENT: [&str; 6] = [
    "SLINT_BACKEND",
    "SLINT_DRM_MODE",
    "SLINT_DRM_OUTPUT",
    "SLINT_DEBUG_PERFORMANCE",
    "AGX_GPU_PACING",
    crate::remote::ENV,
];
static SOFTWARE_CURSOR: AtomicBool = AtomicBool::new(false);

/// Called before signal handlers, Slint, or any application worker starts threads.
pub fn initialize() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(any(feature = "kms", feature = "headless"))]
    if unsafe { libc::geteuid() } == 0 {
        // /etc/agx is private to root; the greeter passes these whitelisted
        // options to the authenticated session instead of opening it as that user.
        match std::fs::read_to_string("/etc/agx/gui.env") {
            Ok(config) => {
                for (key, value) in parse_config(&config)? {
                    if std::env::var_os(&key).is_none() {
                        // Only whitelisted Slint options; this file is data, never shell code.
                        unsafe {
                            std::env::set_var(key, value);
                        }
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    crate::settings::initialize();
    let requested = std::env::var("SLINT_BACKEND").unwrap_or_else(|_| {
        if cfg!(feature = "kms") {
            "auto"
        } else if cfg!(feature = "headless") {
            "vnc-software"
        } else {
            "winit-femtovg"
        }
        .into()
    });
    let backend = if requested == "auto" {
        #[cfg(feature = "kms")]
        {
            let output = detection::at_startup()?;
            let recovery = std::fs::read_to_string("/proc/cmdline").is_ok_and(|line| {
                line.split_whitespace()
                    .any(|option| option == "agx.driver_recovery=1")
            });
            let setup_framebuffer = cfg!(target_arch = "x86_64")
                && unsafe { libc::geteuid() } == 0
                && (cfg!(feature = "kms-provision-graphics")
                    || std::path::Path::new("/etc/agx/setup-enabled").is_file())
                && std::path::Path::new("/dev/fb0").exists();
            #[cfg(feature = "kms-provision-graphics")]
            let needs_runtime = !provisioned::runtime_available();
            #[cfg(not(feature = "kms-provision-graphics"))]
            let needs_runtime = false;
            let selected = if let Some(output) = output {
                // Resolve once before threads start. The login exec inherits the
                // concrete backend/output so signing in cannot change display modes.
                unsafe {
                    std::env::set_var("SLINT_DRM_OUTPUT", &output.name);
                }
                if output.virtual_display || recovery || needs_runtime {
                    eprintln!(
                        "agx-gui: {} connected; using the local software renderer",
                        output.name
                    );
                    "linuxkms-software"
                } else {
                    eprintln!("agx-gui: {} connected; using the GPU renderer", output.name);
                    "linuxkms-femtovg"
                }
            } else if setup_framebuffer {
                eprintln!("agx-gui: using the setup framebuffer until a GPU display is available");
                "linuxkms-software"
            } else {
                eprintln!("agx-gui: auto display: no connected local display; using resizable VNC");
                "vnc-software"
            };
            selected.to_owned()
        }
        #[cfg(not(feature = "kms"))]
        {
            "winit-femtovg".to_owned()
        }
    } else {
        requested
    };
    // Slint and the VNC server also consult this variable directly.
    unsafe {
        std::env::set_var("SLINT_BACKEND", &backend);
    }
    if backend == "vnc-software" {
        if !matches!(std::env::var(crate::remote::ENV).as_deref(), Ok(value) if !matches!(value, "" | "off" | "0"))
        {
            return Err("vnc-software requires AGX_GUI_VNC to be enabled".into());
        }
        virtual_display::initialize()?;
        eprintln!("agx-gui: backend=vnc-software (resizable remote display)");
        return Ok(());
    }
    // Explicit selection prevents silent software fallback from masquerading as a GPU result.
    if !matches!(
        backend.as_str(),
        "linuxkms-femtovg" | "linuxkms-software" | "winit-femtovg" | "winit-software"
    ) {
        return Err(format!("Unsupported SLINT_BACKEND: {backend}").into());
    }
    if backend == "linuxkms-software" {
        // The software comparison backend draws its own pointer in the scene.
        unsafe {
            std::env::set_var("AGX_SOFTWARE_CURSOR", "1");
        }
    }
    let selector = slint::BackendSelector::new().backend_name(backend.clone());
    #[cfg(feature = "kms")]
    let selector = selector.with_libinput_event_hook(kms_input::filter);
    selector.select()?;
    #[cfg(feature = "kms")]
    if backend == "linuxkms-femtovg"
        && unsafe { libc::geteuid() } == 0
        && std::env::args().nth(1).as_deref() == Some("--login")
        && std::env::var("AGX_GPU_PACING").as_deref() != Ok("off")
    {
        let output = std::env::var("SLINT_DRM_OUTPUT").ok();
        match pacing::apply_beelink_floor(std::path::Path::new("/sys"), output.as_deref()) {
            Ok(true) => eprintln!("agx-gui: Beelink Mini S GPU floor set to 400 MHz"),
            Ok(false) => {}
            Err(error) => eprintln!("agx-gui: GPU floor could not be applied: {error}"),
        }
    }
    SOFTWARE_CURSOR.store(backend == "linuxkms-software", Ordering::Relaxed);
    eprintln!("agx-gui: backend={backend}");
    Ok(())
}

pub fn draw_cursor() -> bool {
    SOFTWARE_CURSOR.load(Ordering::Relaxed) || std::env::var_os("AGX_GUI_CURSOR").is_some()
}

/// The KMS pointer lives on the display's hardware cursor plane, not in the frame.
pub fn hardware_cursor() -> bool {
    #[cfg(feature = "kms")]
    return i_slint_backend_linuxkms::hwcursor::available();
    #[cfg(not(feature = "kms"))]
    false
}

pub fn using_winit() -> bool {
    std::env::var("SLINT_BACKEND").is_ok_and(|backend| backend.starts_with("winit-"))
}

pub const WAYLAND_VNC_UNAVAILABLE: &str =
    "Full Desktop is unavailable while a VNC viewer is connected.";

pub fn wayland_unavailable(remote_viewer: bool) -> Option<&'static str> {
    use std::{os::unix::fs::FileTypeExt, path::Path};

    if remote_viewer {
        return Some(WAYLAND_VNC_UNAVAILABLE);
    }
    if !cfg!(feature = "wayland")
        || ![
            "/usr/bin/angi-wayland",
            "/usr/bin/weston",
            "/usr/libexec/agx-wayland-shell",
        ]
        .iter()
        .all(|path| Path::new(path).is_file())
    {
        return Some("Full Desktop requires the Weston runtime.");
    }
    if !Path::new("/usr/bin/seatd").is_file() {
        return Some("Full Desktop requires seatd.");
    }
    if !std::fs::metadata("/run/seatd.sock").is_ok_and(|file| file.file_type().is_socket()) {
        return Some("Full Desktop requires the seatd service to be running.");
    }
    if !std::env::var("SLINT_BACKEND").is_ok_and(|backend| backend.starts_with("linuxkms-")) {
        return Some("Full Desktop requires a local display.");
    }
    None
}

#[cfg(any(feature = "kms", feature = "headless", test))]
fn parse_config(config: &str) -> io::Result<Vec<(String, String)>> {
    let mut options = Vec::new();
    for line in config
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| io::Error::other("Expected KEY=value in /etc/agx/gui.env"))?;
        let (key, value) = (key.trim(), value.trim());
        if !ENVIRONMENT.contains(&key) || value.is_empty() || value.contains('\0') {
            return Err(io::Error::other(
                "Invalid graphics option in /etc/agx/gui.env",
            ));
        }
        options.push((key.into(), value.into()));
    }
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_only_accepts_graphics_options_and_never_executes_shell() {
        let options = parse_config(
            "# Comparison run\nSLINT_BACKEND=linuxkms-software\nSLINT_DRM_MODE=4\nAGX_GUI_VNC=on\n",
        )
        .unwrap();
        assert_eq!(
            options[0],
            ("SLINT_BACKEND".into(), "linuxkms-software".into())
        );
        assert!(parse_config("LD_PRELOAD=/tmp/untrusted.so").is_err());
        assert!(parse_config("export SLINT_BACKEND=linuxkms-software").is_err());
        assert!(parse_config("SLINT_BACKEND=").is_err());
    }
}

/// Register the live window for local input missing from Slint's LinuxKMS mapping.
pub fn attach_input<C: slint::ComponentHandle + 'static>(ui: &C) {
    #[cfg(feature = "kms")]
    kms_input::attach(ui);
    #[cfg(not(feature = "kms"))]
    let _ = ui;
}
