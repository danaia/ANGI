//! The computer on the other end of the Ethernet cable: read its SSH host key and pin the one
//! the person confirmed. Pins use the OpenSSH known_hosts format.
use agx_security::{Paths, Peer, sha256_fingerprint};
use std::{
    fs, io,
    process::{Command, Stdio},
};

const PORTS: [u16; 2] = [22, 2222];

fn entry_host(address: &str, port: u16) -> String {
    if port == 22 { address.to_owned() } else { format!("[{address}]:{port}") }
}

/// The Ed25519 host key line `ssh-ed25519 AAAA…` the peer presents, and its port.
fn scan(address: &str) -> Option<(u16, String)> {
    PORTS.into_iter().find_map(|port| {
        let output = Command::new("ssh-keyscan")
            .args(["-t", "ed25519", "-T", "3", "-p", &port.to_string(), address])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| !line.starts_with('#'))
            .find_map(|line| {
                let mut fields = line.split_whitespace().skip(1);
                Some((port, format!("{} {}", fields.next()?, fields.next()?)))
            })
    })
}

fn pinned(paths: &Paths, host: &str, key: &str) -> bool {
    fs::read_to_string(&paths.peers).is_ok_and(|text| {
        text.lines().any(|line| line.strip_prefix(host).and_then(|rest| rest.strip_prefix(' ')) == Some(key))
    })
}

pub fn read(paths: &Paths, address: &str) -> io::Result<Peer> {
    let (port, key) = scan(address).ok_or_else(|| {
        io::Error::other("The connected computer did not present an SSH host key. Check that its SSH server is running.")
    })?;
    Ok(Peer {
        address: address.to_owned(),
        port,
        fingerprint: sha256_fingerprint(&key).unwrap_or_default(),
        pinned: pinned(paths, &entry_host(address, port), &key),
    })
}

/// Record the peer's key only if it still presents the fingerprint the person confirmed.
pub fn pin(paths: &Paths, address: &str, fingerprint: &str) -> io::Result<Peer> {
    let (port, key) = scan(address)
        .ok_or_else(|| io::Error::other("The connected computer stopped answering."))?;
    if sha256_fingerprint(&key).as_deref() != Some(fingerprint) {
        return Err(io::Error::other(
            "The connected computer's key changed since you looked at it. Nothing was saved.",
        ));
    }
    let host = entry_host(address, port);
    let mut lines: Vec<String> = fs::read_to_string(&paths.peers)
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.starts_with(&format!("{host} ")))
        .map(str::to_owned)
        .collect();
    lines.push(format!("{host} {key}"));
    agx_security::write_atomic(&paths.peers, (lines.join("\n") + "\n").as_bytes(), 0o644)?;
    read(paths, address)
}
