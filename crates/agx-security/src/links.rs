//! Network interfaces from sysfs, the kernel's neighbor table and getifaddrs.
use crate::Kind;
use std::{ffi::CStr, fs};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Interface {
    pub name: String,
    pub kind: Kind,
    pub connected: bool,
    pub address: String,
    pub peer: String,
}

/// Interface names the kernel and udev assign: `eth0`, `enp1s0`, `enx…` (USB adapters), the
/// gadget's `usb0`, and `wlan0`/`wlp…`/`wlo…`.
pub fn classify(name: &str) -> Kind {
    if name == "lo" {
        Kind::Loopback
    } else if name.starts_with("usb") {
        Kind::Usb
    } else if name.starts_with("wl") {
        Kind::Wifi
    } else if name.starts_with("eth") || name.starts_with("en") {
        Kind::Ethernet
    } else {
        Kind::Other
    }
}

pub fn interfaces() -> Vec<Interface> {
    let mut names: Vec<String> = fs::read_dir("/sys/class/net")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    names.sort();
    let addresses = ipv4_addresses();
    let neighbors = neighbors();
    names
        .into_iter()
        .map(|name| {
            let read = |file: &str| {
                fs::read_to_string(format!("/sys/class/net/{name}/{file}"))
                    .map(|text| text.trim().to_owned())
                    .unwrap_or_default()
            };
            let kind = classify(&name);
            let connected = kind == Kind::Loopback || read("carrier") == "1";
            Interface {
                address: addresses
                    .iter()
                    .find(|(device, _)| *device == name)
                    .map(|(_, address)| address.clone())
                    .unwrap_or_default(),
                peer: neighbors
                    .iter()
                    .find(|(device, _)| *device == name)
                    .map(|(_, address)| address.clone())
                    .unwrap_or_default(),
                name,
                kind,
                connected,
            }
        })
        .collect()
}

fn ipv4_addresses() -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut list) } != 0 {
        return found;
    }
    let mut cursor = list;
    while !cursor.is_null() {
        let entry = unsafe { &*cursor };
        if !entry.ifa_addr.is_null()
            && unsafe { (*entry.ifa_addr).sa_family } as i32 == libc::AF_INET
        {
            let socket = unsafe { &*(entry.ifa_addr as *const libc::sockaddr_in) };
            let name = unsafe { CStr::from_ptr(entry.ifa_name) }.to_string_lossy().into_owned();
            let octets = socket.sin_addr.s_addr.to_ne_bytes();
            found.push((name, std::net::Ipv4Addr::from(octets).to_string()));
        }
        cursor = entry.ifa_next;
    }
    unsafe { libc::freeifaddrs(list) };
    found
}

/// Completed IPv4 neighbor entries: `(device, address)`.
fn neighbors() -> Vec<(String, String)> {
    parse_arp(&fs::read_to_string("/proc/net/arp").unwrap_or_default())
}

pub(crate) fn parse_arp(text: &str) -> Vec<(String, String)> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            // IP address, HW type, Flags, HW address, Mask, Device. Flag 0x2 means resolved.
            let flags = u32::from_str_radix(fields.get(2)?.trim_start_matches("0x"), 16).ok()?;
            (fields.len() >= 6 && flags & 2 != 0)
                .then(|| (fields[5].to_owned(), fields[0].to_owned()))
        })
        .collect()
}
