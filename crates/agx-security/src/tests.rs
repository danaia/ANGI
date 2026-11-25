use std::os::unix::fs::PermissionsExt;
use crate::{Kind, Paths, Policy, classify, sha256_fingerprint};

fn paths(name: &str) -> Paths {
    let dir = std::env::temp_dir().join(format!("agx-security-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    Paths {
        policy: dir.join("data-security.conf"),
        image_policy: dir.join("security.conf"),
        data_mount: None,
        key_dir: dir.join("ssh"),
        host_key: dir.join("host_key"),
        peers: dir.join("peer_hosts"),
        gui_env: dir.join("gui.env"),
    }
}

#[test]
fn fingerprint_matches_ssh_keygen() {
    let line = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFNoFyA+ziRu1AqckuaqslMfNYnR7imRI2pVDkKfeHz7 test";
    assert_eq!(
        sha256_fingerprint(line).as_deref(),
        Some("SHA256:och863+uceBbJXM0jSajB0d2zKu43kJ7fMML26aATcw")
    );
    assert_eq!(sha256_fingerprint("ssh-rsa AAAAC3NzaC1lZDI1NTE5AAAAIFNoFyA+ziRu1AqckuaqslMfNYnR7imRI2pVDkKfeHz7"), None);
    assert_eq!(sha256_fingerprint("not a key"), None);
}

#[test]
fn interface_names_map_to_link_kinds() {
    for (name, kind) in [
        ("lo", Kind::Loopback),
        ("eth0", Kind::Ethernet),
        ("enp1s0", Kind::Ethernet),
        ("enx00e04c680001", Kind::Ethernet),
        ("usb0", Kind::Usb),
        ("wlan0", Kind::Wifi),
        ("wlo1", Kind::Wifi),
        ("tailscale0", Kind::Other),
        ("docker0", Kind::Other),
    ] {
        assert_eq!(classify(name), kind, "{name}");
    }
}

#[test]
fn a_missing_policy_keeps_older_systems_open_and_a_damaged_one_closes() {
    let paths = paths("policy");
    assert_eq!(Policy::load(&paths), Policy::LEGACY);
    assert!(Policy::load(&paths).allows(Kind::Ethernet));
    std::fs::write(&paths.image_policy, "ethernet=open\nusb=maybe\n").unwrap();
    assert_eq!(Policy::load(&paths), Policy::CLOSED);
    std::fs::write(&paths.image_policy, "ethernet=open\n").unwrap();
    assert_eq!(Policy::load(&paths), Policy::CLOSED, "an incomplete file is not trusted");
    std::fs::write(&paths.image_policy, "ethernet=open\nethernet=closed\nusb=open\n").unwrap();
    assert_eq!(Policy::load(&paths), Policy::CLOSED, "duplicates are rejected");
}

#[test]
fn opening_one_link_leaves_the_other_and_persists() {
    let paths = paths("save");
    let policy = Policy::CLOSED.with(Kind::Ethernet, true);
    policy.save(&paths).unwrap();
    let loaded = Policy::load(&paths);
    assert!(loaded.ethernet && !loaded.usb && loaded.configured);
    assert!(loaded.allows(Kind::Wifi) && loaded.allows(Kind::Loopback), "other links are not gated");
    assert_eq!(std::fs::read_to_string(&paths.policy).unwrap(), "ethernet=open\nusb=closed\n");
}

#[test]
fn arp_parsing_keeps_only_resolved_neighbors() {
    let table = "IP address       HW type     Flags       HW address            Mask     Device\n\
10.78.0.2        0x1         0x2         7c:83:34:b3:6d:35     *        enp1s0\n\
10.78.0.9        0x1         0x0         00:00:00:00:00:00     *        enp1s0\n\
192.168.1.1      0x1         0x2         aa:bb:cc:dd:ee:ff     *        wlo1\n";
    assert_eq!(
        crate::links::parse_arp(table),
        vec![("enp1s0".to_owned(), "10.78.0.2".to_owned()), ("wlo1".to_owned(), "192.168.1.1".to_owned())]
    );
}

#[test]
fn a_request_to_open_never_prints_its_password() {
    let request = crate::Request::Open { kind: Kind::Ethernet, username: "dana".into(), password: "secret".to_owned().into() };
    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains(r#""op":"open""#) && json.contains(r#""kind":"ethernet""#));
}

#[test]
fn the_saved_choice_outranks_the_image_default_and_survives_a_new_root_filesystem() {
    let paths = paths("adopt");
    std::fs::write(&paths.image_policy, "ethernet=open\nusb=closed\n").unwrap();
    let from_image = Policy::load(&paths);
    assert!(from_image.ethernet && !from_image.usb);

    // First run copies what is in force to the data volume.
    Policy::adopt(&paths);
    assert_eq!(std::fs::read_to_string(&paths.policy).unwrap(), "ethernet=open\nusb=closed\n");

    // An update replaces the image's file with a stricter default; the earlier choice stands.
    std::fs::write(&paths.image_policy, "ethernet=closed\nusb=closed\n").unwrap();
    assert!(Policy::load(&paths).ethernet);

    // A person's later choice replaces the adopted one, and adopt never overwrites it.
    Policy::load(&paths).with(Kind::Ethernet, false).save(&paths).unwrap();
    Policy::adopt(&paths);
    assert!(!Policy::load(&paths).ethernet);
}

#[test]
fn a_device_with_no_policy_anywhere_keeps_wired_ssh_open_and_records_that() {
    let paths = paths("legacy-adopt");
    Policy::adopt(&paths);
    assert_eq!(std::fs::read_to_string(&paths.policy).unwrap(), "ethernet=open\nusb=open\n");
}

#[test]
fn vnc_is_off_until_asked_for_and_keeps_the_other_graphics_options() {
    let paths = paths("vnc");
    assert!(!crate::vnc_enabled(&paths), "no gui.env means off");
    std::fs::write(&paths.gui_env, "SLINT_BACKEND=auto\n").unwrap();
    assert!(!crate::vnc_enabled(&paths));

    crate::set_vnc(&paths, true).unwrap();
    assert!(crate::vnc_enabled(&paths));
    assert_eq!(std::fs::read_to_string(&paths.gui_env).unwrap(), "SLINT_BACKEND=auto\nAGX_GUI_VNC=on\n");

    crate::set_vnc(&paths, false).unwrap();
    assert!(!crate::vnc_enabled(&paths));
    assert_eq!(std::fs::read_to_string(&paths.gui_env).unwrap(), "SLINT_BACKEND=auto\nAGX_GUI_VNC=off\n");
}

#[test]
fn turning_vnc_on_keeps_a_port_the_person_chose_and_replaces_a_stale_line() {
    let paths = paths("vnc-port");
    std::fs::write(&paths.gui_env, "AGX_GUI_VNC=127.0.0.1:5905\nSLINT_BACKEND=auto\n").unwrap();
    assert!(crate::vnc_enabled(&paths));
    crate::set_vnc(&paths, true).unwrap();
    let text = std::fs::read_to_string(&paths.gui_env).unwrap();
    assert_eq!(text.matches("AGX_GUI_VNC").count(), 1);
    assert!(text.contains("AGX_GUI_VNC=127.0.0.1:5905"));
    assert!(text.contains("SLINT_BACKEND=auto"));
    assert_eq!(std::fs::metadata(&paths.gui_env).unwrap().permissions().mode() & 0o777, 0o600);
}
