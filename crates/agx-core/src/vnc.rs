//! Read-only VNC socket observation; no connections, authentication or service control.
use crate::VncStatus;
use std::{
    fs,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::Path,
};

fn configured_port(value: &str) -> u16 {
    value
        .trim()
        .parse()
        .ok()
        .or_else(|| {
            value
                .trim()
                .parse::<SocketAddr>()
                .ok()
                .map(|addr| addr.port())
        })
        .filter(|port| *port > 1)
        .unwrap_or(5900)
}

fn endpoint(value: &str) -> Option<SocketAddr> {
    let (address, port) = value.split_once(':')?;
    let port = u16::from_str_radix(port, 16).ok()?;
    // /proc/net/tcp encodes each 32-bit address word in host byte order.
    let word = |s: &str| u32::from_str_radix(s, 16).ok().map(u32::to_ne_bytes);
    let ip = match address.len() {
        8 => IpAddr::V4(Ipv4Addr::from(word(address)?)),
        32 => {
            let mut bytes = [0; 16];
            for (i, part) in bytes.chunks_exact_mut(4).enumerate() {
                part.copy_from_slice(&word(address.get(i * 8..i * 8 + 8)?)?);
            }
            IpAddr::V6(Ipv6Addr::from(bytes))
        }
        _ => return None,
    };
    Some(SocketAddr::new(ip, port))
}

fn sockets(text: &str, status: &mut VncStatus) {
    for line in text.lines().skip(1) {
        let mut fields = line.split_whitespace();
        let _slot = fields.next();
        let Some(local) = fields.next().and_then(endpoint) else {
            continue;
        };
        let _peer = fields.next();
        let state = fields.next();
        if local.port() != status.port {
            continue;
        }
        match state {
            Some("0A") => status.listeners.push(local.to_string()),
            Some("01") => status.connections += 1,
            _ => (),
        }
    }
}

fn gui_rss(proc_root: &Path) -> Option<u64> {
    let mut total = None;
    for entry in fs::read_dir(proc_root).ok()?.flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .bytes()
            .all(|b| b.is_ascii_digit())
        {
            continue;
        }
        if fs::read_to_string(entry.path().join("comm"))
            .ok()
            .as_deref()
            .map(str::trim)
            != Some("agx-gui")
        {
            continue;
        }
        let status = fs::read_to_string(entry.path().join("status")).ok();
        if let Some(rss) = status.as_deref().and_then(|text| {
            text.lines().find_map(|line| {
                line.strip_prefix("VmRSS:")?
                    .split_whitespace()
                    .next()?
                    .parse::<u64>()
                    .ok()
            })
        }) {
            total = Some(total.unwrap_or(0) + rss * 1024);
        }
    }
    total
}

pub fn collect() -> VncStatus {
    // The GUI passes this setting into authenticated sessions. Root can also
    // inspect gui.env; an SSH TUI without access to it observes the default port.
    let setting = std::env::var("AGX_GUI_VNC").ok().or_else(|| {
        fs::read_to_string("/etc/agx/gui.env")
            .ok()?
            .lines()
            .find_map(|line| {
                let (key, value) = line.split_once('=')?;
                (key.trim() == "AGX_GUI_VNC").then(|| value.trim().to_owned())
            })
    });
    let mut status = VncStatus {
        port: configured_port(setting.as_deref().unwrap_or("on")),
        gui_rss_bytes: gui_rss(Path::new("/proc")),
        ..VncStatus::default()
    };
    for name in ["tcp", "tcp6"] {
        match fs::read_to_string(format!("/proc/net/{name}")) {
            Ok(text) => sockets(&text, &mut status),
            Err(error) if name == "tcp6" && error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => status.error = Some(format!("VNC socket status: {error}")),
        }
    }
    status.listeners.sort();
    status.listeners.dedup();
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sockets_distinguish_listeners_viewers_and_outbound_or_closed_connections() {
        let text = "header\n0: 0100007F:170C 00000000:0000 0A\n1: 0100007F:170C 0100007F:C000 01\n2: 0100007F:C000 0100007F:170C 01\n3: 0100007F:170C 0100007F:C001 06\n4: invalid ignored 01\n";
        let mut status = VncStatus {
            port: 5900,
            ..VncStatus::default()
        };
        sockets(text, &mut status);
        assert_eq!(status.listeners, ["127.0.0.1:5900"]);
        assert_eq!(status.connections, 1);
        sockets(
            "header\n0: 00000000000000000000000001000000:170C 00000000000000000000000000000000:0000 0A\n",
            &mut status,
        );
        assert_eq!(status.listeners[1], "[::1]:5900");
    }

    #[test]
    fn custom_port_settings_match_the_gui_syntax() {
        for value in ["on", "1", "off", "", "0"] {
            assert_eq!(configured_port(value), 5900);
        }
        assert_eq!(configured_port(" 5901 "), 5901);
        assert_eq!(configured_port("127.0.0.1:5940"), 5940);
        assert_eq!(configured_port("[::1]:5902"), 5902);
    }
}
