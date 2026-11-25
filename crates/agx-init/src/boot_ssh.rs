//! Verify Dropbear's local listener without relying on the public SSH gate.
use std::{fs, net::Ipv4Addr};

pub(super) fn listening(address: &str) -> bool {
    ["/proc/net/tcp", "/proc/net/tcp6"]
        .iter()
        .any(|path| fs::read_to_string(path).is_ok_and(|table| table_has_listener(&table, address)))
}

fn table_has_listener(table: &str, address: &str) -> bool {
    let (host, port) = address.rsplit_once(':').unwrap_or(("", address));
    let Ok(port) = port.parse::<u16>() else {
        return false;
    };
    let host = if host.is_empty() {
        None
    } else {
        let Ok(host) = host.parse::<Ipv4Addr>() else {
            return false;
        };
        Some(format!("{:08X}", u32::from_ne_bytes(host.octets())))
    };

    table.lines().any(|line| {
        let mut fields = line.split_whitespace();
        let Some((local_host, local_port)) = fields.nth(1).and_then(|local| local.split_once(':'))
        else {
            return false;
        };
        fields.nth(1) == Some("0A")
            && u16::from_str_radix(local_port, 16) == Ok(port)
            && host.as_deref().is_none_or(|host| local_host == host)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_public_gate_can_still_have_a_working_ssh_backend() {
        let loopback = u32::from_ne_bytes(Ipv4Addr::LOCALHOST.octets());
        let table = format!("0: {loopback:08X}:07E6 00000000:0000 0A\n");
        assert!(table_has_listener(&table, "127.0.0.1:2022"));
        assert!(!table_has_listener(&table, "22"));
    }

    #[test]
    fn checks_the_local_listening_address_and_port() {
        let table = "0: 00000000:0016 00000000:0000 0A\n\
                     1: 00000000:07E6 00000000:0000 0A\n";
        assert!(table_has_listener(table, "22"));
        assert!(!table_has_listener(table, "127.0.0.1:2022"));
        assert!(!table_has_listener(
            "0: 00000000:0016 00000000:0000 01",
            "22"
        ));
    }
}
