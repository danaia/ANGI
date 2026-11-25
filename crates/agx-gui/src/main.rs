//! AGX desktop: a Slint front end over the same system data as the TUI, with
//! Omarchy/Hyprland-style window management (see wm.rs and keymap.rs).
//!
//! Exit codes let a launcher script react to the user's choice:
//! 0 = quit.
mod bluetooth;
mod browser;
mod calendar;
mod files;
mod graphics;
mod input;
mod key_bindings;
mod keymap;
mod launcher;
mod library;
mod login;
mod memory_processes;
mod open_request;
mod web_preview;
mod remote;
mod scene_model;
mod security;
mod terminal;
mod wm;
mod work_editor;

mod agent_attachments;
mod agent_chat;
mod agent_history;
mod agent_knowledge;
#[cfg(test)]
mod scroll_probe; // TEMPORARY
mod agent_project;
mod agent_providers;
mod agent_swarm;
mod agent_voice;
mod app;
mod build_notice;
mod cpu_temperature;
mod drivers;
mod network_safety;
mod night_light;
#[cfg(test)]
mod panel_menu_tests;
#[cfg(test)]
mod pointer_tests;
mod runtime;
mod services;
mod appearance;
mod settings;
mod sound;
mod session_refresh;
mod system_ui;
mod storage;
use app::{App, session_action};
use keymap::Mods;
use slint::ComponentHandle;
#[cfg(feature = "kms")]
use std::{process::Command, time::Duration};
use std::{
    process::ExitCode,
    sync::atomic::{AtomicI32, Ordering},
};
#[cfg(test)]
use {
    input::Motion,
    keymap::Cmd,
    slint::{Model, ModelRc},
    std::{cell::RefCell, rc::Rc},
    wm::Kind,
};
slint::include_modules!();
// Room for the Pebble Line top bar pill and hint line; ui/pebble/tokens.slint mirrors these.
const TOP: f32 = 60.0;
const BOTTOM: f32 = 30.0;
static EXIT_CODE: AtomicI32 = AtomicI32::new(0);
fn mods(ctrl: bool, alt: bool, shift: bool, meta: bool) -> Mods {
    Mods {
        ctrl,
        alt,
        shift,
        meta,
    }
}

fn main() -> ExitCode {
    // Started as `open FILE` (a link to this binary) or `agx-gui open FILE`: ask the running desktop, never start one.
    if let Some(targets) = open_request::client_arguments() {
        return open_request::client(targets);
    }
    if std::env::args().nth(1).as_deref() == Some("--help") {
        println!(
            "ANGI desktop\nUsage: agx-gui [--login | --login-preview | --cursor-preview | --prepare-web-storage]\n       agx-gui open FILE_OR_ADDRESS..."
        );
        return ExitCode::SUCCESS;
    }
    if std::env::args().nth(1).as_deref() == Some("--prepare-web-storage") {
        return match browser::prepare_accounts() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("agx-gui: cannot prepare Web storage: {error}");
                ExitCode::FAILURE
            }
        };
    }
    if std::env::args().nth(1).as_deref() == Some("--prepare-drivers") {
        return drivers::provision::run_for_settings();
    }
    prepare_display();
    if let Err(error) = graphics::initialize() {
        eprintln!("agx-gui: {error}");
        return ExitCode::FAILURE;
    }
    quit_on_signals();
    let result = match std::env::args().nth(1).as_deref() {
        Some("--cursor-preview") => CursorPreviewWindow::new()
            .map_err(Into::into)
            .and_then(|ui| ui.run().map_err(Into::into)),
        Some("--login") => login::run(false),
        Some("--login-preview") => login::run(true),
        _ => runtime::run(),
    };
    // Keys typed into the GUI also reach the terminal; do not replay them into the shell.
    // SAFETY: tcflush on stdin is harmless when it is not a terminal.
    unsafe { libc::tcflush(0, libc::TCIFLUSH) };
    if let Err(error) = result {
        eprintln!("agx-gui: {error}");
        return ExitCode::from(1);
    }
    if let Err(error) = session_refresh::restart_if_requested() {
        eprintln!("agx-gui: could not refresh desktop: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::from(EXIT_CODE.load(Ordering::SeqCst) as u8)
}

/// The framebuffer backend puts the console in graphics mode and only restores it
/// when the program exits normally, so Ctrl+C or a kill must end the event loop
/// instead of the process. SIGUSR1 is the one exception: it asks for a desktop refresh.
fn quit_on_signals() {
    use signal_hook::{
        consts::{SIGHUP, SIGINT, SIGTERM, SIGUSR1},
        iterator::Signals,
    };
    if let Ok(mut signals) = Signals::new([SIGINT, SIGTERM, SIGHUP, SIGUSR1]) {
        std::thread::spawn(move || {
            for signal in signals.forever() {
                let _ = slint::invoke_from_event_loop(move || {
                    if signal == SIGUSR1 {
                        session_refresh::request_from_signal();
                    } else {
                        let _ = slint::quit_event_loop();
                    }
                });
            }
        });
    }
}

/// The appliance image has no udev, so nothing loads the display driver at
/// boot. Load it here when the DRM device is missing.
#[cfg(feature = "kms")]
fn prepare_display() {
    if unsafe { libc::geteuid() } == 0 {
        // vc4 drives HDMI; v3d provides the Pi 5's GPU render device.
        let _ = Command::new("modprobe").arg("v3d").status();
    }
    let card = std::path::Path::new("/dev/dri/card0");
    if card.exists() {
        return;
    }
    // HDMI reads the monitor over the SoC's I2C bus; without that driver vc4 never finishes binding.
    for module in ["i2c-brcmstb", "vc4"] {
        match Command::new("modprobe").arg(module).status() {
            Ok(status) if status.success() => {}
            Ok(status) => eprintln!("agx-gui: modprobe {module} exited with {status}"),
            Err(error) => eprintln!("agx-gui: modprobe {module}: {error}"),
        }
    }
    for _ in 0..50 {
        if card.exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    eprintln!(
        "agx-gui: /dev/dri/card0 did not appear; is dtoverlay=vc4-kms-v3d set in config.txt?"
    );
}
#[cfg(not(feature = "kms"))]
fn prepare_display() {}

fn quit(code: i32) {
    EXIT_CODE.store(code, Ordering::SeqCst);
    let _ = slint::quit_event_loop();
}
