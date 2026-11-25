use super::{link_kind, suggest::suggestions, summarize};
use agx_security::{Kind, Link, Status};

fn link(name: &str, kind: Kind, connected: bool, peer: &str) -> Link {
    Link { name: name.into(), kind, connected, address: String::new(), peer: peer.into(), open: false }
}

#[test]
fn the_peer_is_the_computer_on_a_connected_ethernet_cable() {
    let status = Status {
        links: vec![
            link("wlan0", Kind::Wifi, true, "192.168.1.1"),
            link("eth0", Kind::Ethernet, true, "10.78.0.1"),
            link("usb0", Kind::Usb, false, ""),
        ],
        ..Status::default()
    };
    let summary = summarize(&status);
    assert!(summary.ethernet_connected && !summary.usb_connected);
    assert_eq!(summary.peer, "10.78.0.1", "a Wi-Fi gateway is not the cable's peer");
}

#[test]
fn an_unplugged_cable_has_no_peer_even_if_the_neighbor_entry_lingers() {
    let status = Status { links: vec![link("eth0", Kind::Ethernet, false, "10.78.0.1")], ..Status::default() };
    let summary = summarize(&status);
    assert!(!summary.ethernet_connected);
    assert_eq!(summary.peer, "");
}

#[test]
fn only_the_two_wired_links_can_be_chosen() {
    assert_eq!(link_kind(1), Some(Kind::Ethernet));
    assert_eq!(link_kind(2), Some(Kind::Usb));
    assert_eq!(link_kind(0), None);
    assert_eq!(link_kind(3), None);
}

fn open_link(name: &str, kind: Kind, address: &str) -> Link {
    Link { name: name.into(), kind, connected: true, address: address.into(), peer: String::new(), open: true }
}

#[test]
fn an_open_cable_gets_a_ready_to_paste_ssh_command() {
    let status = Status {
        ethernet_open: true,
        links: vec![open_link("eth0", Kind::Ethernet, "10.78.0.2")],
        ..Status::default()
    };
    let lines = suggestions(&status, "dana");
    assert_eq!(lines, ["From the computer on the Ethernet cable: ssh dana@10.78.0.2"]);
}

#[test]
fn a_closed_cable_says_how_to_open_it_and_no_cable_says_to_plug_one_in() {
    let closed = Status { links: vec![open_link("eth0", Kind::Ethernet, "10.78.0.2")], ..Status::default() };
    assert!(suggestions(&closed, "dana")[0].contains("closed. Choose Allow access"));
    let none = suggestions(&Status::default(), "dana");
    assert_eq!(none, ["To connect another computer directly, plug in an Ethernet or USB cable."]);
}

#[test]
fn remote_desktop_is_only_offered_through_an_ssh_tunnel_to_a_reachable_address() {
    let mut status = Status {
        ethernet_open: true,
        vnc_enabled: true,
        vnc_listening: true,
        links: vec![open_link("eth0", Kind::Ethernet, "10.78.0.2")],
        ..Status::default()
    };
    let text = suggestions(&status, "dana").join("\n");
    assert!(text.contains("ssh -N -L 5941:127.0.0.1:5900 dana@10.78.0.2"));
    assert!(text.contains("cannot be reached any other way"));

    status.vnc_listening = false;
    assert!(suggestions(&status, "dana").join("\n").contains("sign out and back in"));
    status.vnc_enabled = false;
    status.vnc_listening = true;
    assert!(suggestions(&status, "dana").join("\n").contains("stops at your next sign-in"));
    status.ethernet_open = false;
    status.vnc_enabled = true;
    assert!(suggestions(&status, "dana").join("\n").contains("nothing is open to reach it"));
}

#[test]
fn wifi_is_mentioned_but_is_not_claimed_to_be_controlled() {
    let status = Status { links: vec![open_link("wlan0", Kind::Wifi, "192.168.1.20")], ..Status::default() };
    let text = suggestions(&status, "dana").join("\n");
    assert!(text.contains("ssh dana@192.168.1.20") && text.contains("not controlled on this page"));
}
