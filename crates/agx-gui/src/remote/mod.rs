//! Optional VNC access to the desktop, so a viewer such as Remmina can see and drive
//! the GUI of a Pi whose HDMI output is not in use. Off unless `AGX_GUI_VNC` is set.
//!
//! The server listens on loopback only and offers no VNC password: reach it through
//! an OpenSSH tunnel (see `scripts/vnc-tunnel.sh`). Frames come from Slint's own window
//! snapshot, so this works with the KMS and the preview backends alike.
//!
//! Signing in replaces the root login process with the user's desktop. The login
//! screen passes its listening socket and connected viewers on (see `handoff`), so a
//! viewer stays connected across the sign-in instead of having to reconnect.
mod frame;
mod handoff;
pub(crate) mod input;
mod resize;
mod rfb;
mod server;

use frame::Frame;
use slint::{ComponentHandle, Timer, TimerMode};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::{Duration, Instant},
};

pub const ENV: &str = "AGX_GUI_VNC";
const DEFAULT_PORT: u16 = 5900;
const POLL: Duration = Duration::from_millis(33);
const BUSY_INTERVAL: Duration = Duration::from_millis(66);
const IDLE_INTERVAL: Duration = Duration::from_millis(500);

/// Keeps the capture timer alive; dropping it stops capturing.
pub struct Remote {
    _timer: Timer,
    shared: std::sync::Arc<server::Shared>,
}

/// What the launcher must do before replacing this process with the desktop.
pub struct Exec {
    /// Descriptors that must stay open across the exec.
    pub fds: Vec<i32>,
    /// Environment variables for the new process, as (name, value).
    pub env: [(&'static str, String); 2],
}

impl Remote {
    pub fn has_viewers(&self) -> bool {
        self.shared.has_viewers()
    }

    pub fn stop_accepting_if_idle(&self) -> bool {
        self.shared.stop_accepting_if_idle()
    }

    /// Describe the open connections for the process that will replace this one.
    pub fn handoff_for_exec(&self) -> Option<Exec> {
        let handoff = self.shared.handoff()?;
        Some(Exec {
            fds: handoff.fds(),
            env: [
                (handoff::ENV, handoff.encode()),
                (handoff::FDS_ENV, handoff.fds_env()),
            ],
        })
    }
}

/// `None` when VNC is off. Accepts `on`/`1`, a port, or a loopback `ip:port`.
fn parse_listen(value: Option<&str>) -> Result<Option<SocketAddr>, String> {
    let value = value.map(str::trim).unwrap_or_default();
    let loopback = |port| SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let addr = match value {
        "" | "0" | "off" => return Ok(None),
        "1" | "on" => loopback(DEFAULT_PORT),
        _ => match value.parse::<u16>() {
            Ok(port) => loopback(port),
            Err(_) => value.parse().map_err(|_| {
                format!("{ENV}: expected on, off, a port or a loopback address, got {value:?}")
            })?,
        },
    };
    if !addr.ip().is_loopback() {
        return Err(format!(
            "{ENV}: only loopback addresses are allowed; reach the desktop through an SSH tunnel"
        ));
    }
    Ok(Some(addr))
}

/// Start serving `ui` when `AGX_GUI_VNC` asks for it. Failures are logged and never
/// stop the desktop or the login screen from starting.
pub fn start<C: ComponentHandle + 'static>(ui: &C) -> Option<Remote> {
    let addr = match parse_listen(std::env::var(ENV).ok().as_deref()) {
        Ok(Some(addr)) => addr,
        Ok(None) => return None,
        Err(message) => {
            eprintln!("agx-gui: {message}");
            return None;
        }
    };
    let weak = ui.as_weak();
    let input_weak = std::sync::Mutex::new(weak.clone());
    let shared = server::Shared::new(move |input| {
        let weak = input_weak
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let _ = weak.upgrade_in_event_loop(move |ui| input::dispatch(ui.window(), input));
    });
    if std::env::var("SLINT_BACKEND").as_deref() == Ok("vnc-software") {
        let resize_weak = std::sync::Mutex::new(weak.clone());
        shared.set_resize_handler(move |width, height| {
            let weak = resize_weak
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone();
            let (sender, receiver) = std::sync::mpsc::sync_channel(1);
            weak.upgrade_in_event_loop(move |ui| {
                ui.window().set_size(slint::PhysicalSize::new(
                    u32::from(width),
                    u32::from(height),
                ));
                let frame = ui.window().take_snapshot().ok().map(Frame::from_snapshot);
                eprintln!("agx-gui: vnc desktop resized to {width}x{height}");
                let _ = sender.send(frame);
            })
            .ok()?;
            receiver.recv().ok().flatten()
        });
        if let Some((width, height)) = std::env::var(handoff::ENV)
            .ok()
            .and_then(|text| handoff::Handoff::parse(&text))
            .and_then(|handoff| handoff.viewers.first().map(|viewer| viewer.size))
            && let (Ok(width), Ok(height)) = (u16::try_from(width), u16::try_from(height))
            && resize::supported_size(width, height)
        {
            ui.window().set_size(slint::PhysicalSize::new(
                u32::from(width),
                u32::from(height),
            ));
        }
    }
    let inherited = std::env::var(handoff::ENV)
        .ok()
        .and_then(|text| handoff::Handoff::parse(&text))
        .is_some_and(|handoff| server::adopt(&handoff, addr, &shared));
    if inherited {
        eprintln!("agx-gui: vnc continuing on {addr} for viewers from the login screen");
    } else if let Err(error) = server::listen(addr, shared.clone()) {
        eprintln!("agx-gui: vnc: cannot listen on {addr}: {error}");
        return None;
    } else {
        eprintln!("agx-gui: vnc listening on {addr} (SSH tunnel only)");
    }
    let (mut last, mut warned) = (Instant::now() - IDLE_INTERVAL, false);
    let timer = Timer::default();
    let capture_shared = shared.clone();
    timer.start(TimerMode::Repeated, POLL, move || {
        let shared = &capture_shared;
        let interval = if shared.input_is_recent() {
            BUSY_INTERVAL
        } else {
            IDLE_INTERVAL
        };
        if last.elapsed() < interval || !shared.wants_frame() {
            return;
        }
        let Some(ui) = weak.upgrade() else { return };
        last = Instant::now();
        match ui.window().take_snapshot() {
            Ok(snapshot) => shared.publish(Frame::from_snapshot(snapshot)),
            Err(error) if !warned => {
                warned = true;
                eprintln!("agx-gui: vnc: cannot capture the window: {error}");
            }
            Err(_) => {}
        }
    });
    Some(Remote {
        _timer: timer,
        shared,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vnc_is_off_unless_asked_for() {
        for off in [None, Some(""), Some("off"), Some("0"), Some("  ")] {
            assert_eq!(parse_listen(off), Ok(None));
        }
    }

    #[test]
    fn shorthand_and_ports_bind_loopback() {
        assert_eq!(
            parse_listen(Some("on")),
            Ok(Some("127.0.0.1:5900".parse().unwrap()))
        );
        assert_eq!(
            parse_listen(Some("1")),
            Ok(Some("127.0.0.1:5900".parse().unwrap()))
        );
        assert_eq!(
            parse_listen(Some("5901")),
            Ok(Some("127.0.0.1:5901".parse().unwrap()))
        );
        assert_eq!(
            parse_listen(Some("[::1]:5902")),
            Ok(Some("[::1]:5902".parse().unwrap()))
        );
    }

    #[test]
    fn non_loopback_and_garbage_are_refused() {
        assert!(parse_listen(Some("0.0.0.0:5900")).is_err());
        assert!(parse_listen(Some("192.168.1.184:5900")).is_err());
        assert!(parse_listen(Some("yes please")).is_err());
    }
}
