//! "How to connect": plain sentences and ready-to-paste commands for what is connected and open
//! right now. Pure, so the wording is checked without a window.
use agx_security::{Kind, Link, Status};

/// The port the desktop's VNC server listens on, on loopback, and the port a viewer uses locally.
const VNC_PORT: u16 = 5900;
const VIEWER_PORT: u16 = 5941;

fn wired(status: &Status, kind: Kind) -> Option<&Link> {
    status.links.iter().find(|link| link.kind == kind && link.connected)
}

/// The address another computer should use: the first open wired link with an address, else Wi-Fi.
fn reachable(status: &Status) -> Option<&Link> {
    [(Kind::Ethernet, status.ethernet_open), (Kind::Usb, status.usb_open)]
        .into_iter()
        .filter(|(_, open)| *open)
        .filter_map(|(kind, _)| wired(status, kind))
        .chain(wired(status, Kind::Wifi))
        .find(|link| !link.address.is_empty())
}

pub(super) fn suggestions(status: &Status, user: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut any_wired = false;
    for (kind, open) in [(Kind::Ethernet, status.ethernet_open), (Kind::Usb, status.usb_open)] {
        let Some(link) = wired(status, kind) else { continue };
        any_wired = true;
        let label = kind.label();
        if !open {
            lines.push(format!(
                "{label} is connected but closed. Choose Allow access… to let the computer on the cable sign in."
            ));
        } else if link.address.is_empty() {
            lines.push(format!("{label} is open but has no address yet. Give it a moment to come up."));
        } else {
            lines.push(format!("From the computer on the {label} cable: ssh {user}@{}", link.address));
        }
    }
    if !any_wired {
        lines.push("To connect another computer directly, plug in an Ethernet or USB cable.".into());
    }
    if let Some(wifi) = wired(status, Kind::Wifi).filter(|link| !link.address.is_empty()) {
        lines.push(format!(
            "Over Wi-Fi: ssh {user}@{}. Wi-Fi access is not controlled on this page.",
            wifi.address
        ));
    }
    match (status.vnc_enabled, status.vnc_listening, reachable(status)) {
        (true, true, Some(link)) => lines.push(format!(
            "Remote desktop: run ssh -N -L {VIEWER_PORT}:127.0.0.1:{VNC_PORT} {user}@{} on the other computer, then point a VNC viewer at 127.0.0.1:{VIEWER_PORT}. It cannot be reached any other way.",
            link.address
        )),
        (true, true, None) => lines.push(
            "Remote desktop is on, but nothing is open to reach it. Open a connection above first.".into(),
        ),
        (true, false, _) => lines.push(
            "Remote desktop starts at your next sign-in: sign out and back in. Refresh desktop does not start it.".into(),
        ),
        (false, true, _) => lines.push(
            "Remote desktop is still running in this session. It stops at your next sign-in.".into(),
        ),
        (false, false, _) => {}
    }
    lines
}
