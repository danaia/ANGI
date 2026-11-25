use crate::gate::Gate;
use agx_security::{Interface, Kind, Policy};
use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    thread,
    time::{Duration, Instant},
};

fn wired(name: &str, kind: Kind) -> Interface {
    Interface { name: name.into(), kind, connected: true, address: String::new(), peer: String::new() }
}

/// A backend that answers every line with an upper-cased copy, standing in for Dropbear.
fn backend() -> SocketAddr {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            thread::spawn(move || {
                let mut stream = stream;
                let mut buffer = [0u8; 64];
                while let Ok(read) = stream.read(&mut buffer) {
                    if read == 0 || stream.write_all(&buffer[..read].to_ascii_uppercase()).is_err() {
                        return;
                    }
                }
            });
        }
    });
    address
}

fn free_port() -> u16 {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap().local_addr().unwrap().port()
}

fn connect(port: u16) -> Option<TcpStream> {
    let stream = TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), Duration::from_secs(1)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    Some(stream)
}

fn exchange(stream: &mut TcpStream) -> bool {
    stream.write_all(b"ssh").is_ok() && {
        let mut reply = [0u8; 3];
        stream.read_exact(&mut reply).is_ok() && &reply == b"SSH"
    }
}

fn wait_until(condition: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    false
}

// `lo` stands in for a wired interface: the gate binds each socket to a device by name.
#[test]
fn a_closed_wired_link_refuses_ssh_and_opening_it_relays_to_the_backend() {
    let port = free_port();
    let gate = Gate::new(port, backend());
    let links = [wired("lo", Kind::Ethernet)];

    gate.sync(Policy::CLOSED, &links);
    assert!(!gate.listening("lo"));
    assert!(connect(port).is_none(), "a closed link must not accept connections");

    gate.sync(Policy::CLOSED.with(Kind::Ethernet, true), &links);
    assert!(gate.listening("lo"));
    assert!(wait_until(|| connect(port).is_some()));
    assert!(exchange(&mut connect(port).unwrap()), "an open link relays to the SSH backend");
}

#[test]
fn closing_a_link_cuts_the_sessions_already_open_on_it() {
    let port = free_port();
    let gate = Gate::new(port, backend());
    let links = [wired("lo", Kind::Usb)];
    gate.sync(Policy::CLOSED.with(Kind::Usb, true), &links);
    assert!(wait_until(|| connect(port).is_some()));
    let mut session = connect(port).unwrap();
    assert!(exchange(&mut session));

    gate.sync(Policy::CLOSED, &links);
    let mut byte = [0u8; 1];
    let _ = session.write_all(b"x");
    assert!(
        matches!(session.read(&mut byte), Ok(0) | Err(_)),
        "the established session ends when its link is closed"
    );
    assert!(wait_until(|| connect(port).is_none()));
}

#[test]
fn links_that_are_not_gated_stay_open_whatever_the_policy() {
    let port = free_port();
    let gate = Gate::new(port, backend());
    gate.sync(Policy::CLOSED, &[wired("lo", Kind::Wifi)]);
    assert!(gate.listening("lo"));
    assert!(wait_until(|| connect(port).is_some()));
}
