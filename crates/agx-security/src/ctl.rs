//! Command-line client for the running service, so the same requests the desktop makes can be
//! made from a shell: `agx-securityd status`, `vnc on|off`, `close-all`, `open|close <link>`, `peer`.
//! As root no password is needed. Otherwise set AGX_SECURITY_USER and AGX_SECURITY_PASSWORD.
use agx_security::{Kind, Reply, Request, request};
use zeroize::Zeroizing;

const USAGE: &str = "usage: agx-securityd [status | vnc on|off | close-all | open|close ethernet|usb | peer]";

fn link(name: Option<&String>) -> Result<Kind, String> {
    match name.map(String::as_str) {
        Some("ethernet") => Ok(Kind::Ethernet),
        Some("usb") => Ok(Kind::Usb),
        _ => Err(USAGE.into()),
    }
}

fn credentials() -> (String, Zeroizing<String>) {
    (
        std::env::var("AGX_SECURITY_USER").unwrap_or_default(),
        Zeroizing::new(std::env::var("AGX_SECURITY_PASSWORD").unwrap_or_default()),
    )
}

fn build(args: &[String]) -> Result<Request, String> {
    match args.first().map(String::as_str) {
        Some("status") => Ok(Request::Status),
        Some("close-all") => Ok(Request::CloseAll),
        Some("peer") => Ok(Request::Peer),
        Some("vnc") => {
            let (username, password) = credentials();
            match args.get(1).map(String::as_str) {
                Some("on") => Ok(Request::Vnc { on: true, username, password }),
                Some("off") => Ok(Request::Vnc { on: false, username, password }),
                _ => Err(USAGE.into()),
            }
        }
        Some("open") => {
            let (username, password) = credentials();
            Ok(Request::Open { kind: link(args.get(1))?, username, password })
        }
        Some("close") => Ok(Request::Close { kind: link(args.get(1))? }),
        _ => Err(USAGE.into()),
    }
}

fn print(reply: &Reply) {
    let status = &reply.status;
    let word = |open| if open { "open" } else { "closed" };
    if !reply.message.is_empty() {
        println!("{}{}", if reply.ok { "" } else { "refused: " }, reply.message);
    }
    println!("ssh ethernet {} | ssh usb {} | policy saved: {}", word(status.ethernet_open), word(status.usb_open), status.configured);
    println!(
        "vnc configured: {} | serving: {} | viewers: {}",
        status.vnc_enabled, status.vnc_listening, status.vnc_viewers
    );
    for link in &status.links {
        println!(
            "  {:<12} {:<8} {:<12} ssh {:<6} {} {}",
            link.name,
            format!("{:?}", link.kind).to_lowercase(),
            if link.connected { "connected" } else { "no link" },
            word(link.open),
            link.address,
            link.peer
        );
    }
    println!("key {}", status.key_fingerprint);
    println!("host {}", status.host_fingerprint);
    if let Some(peer) = &reply.peer {
        println!("peer {}:{} {} pinned: {}", peer.address, peer.port, peer.fingerprint, peer.pinned);
    }
}

/// Returns the process exit status.
pub fn run(args: &[String]) -> i32 {
    let request_to_send = match build(args) {
        Ok(request) => request,
        Err(message) => {
            eprintln!("{message}");
            return 2;
        }
    };
    match request(&request_to_send) {
        Ok(reply) => {
            print(&reply);
            if reply.ok { 0 } else { 1 }
        }
        Err(error) => {
            eprintln!("agx-securityd: {error}");
            1
        }
    }
}
