//! Request, reply and state models. Requests deliberately omit Debug: opening a link carries
//! the administrator's password.
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// The class of network link SSH access is decided for.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Loopback,
    Ethernet,
    Usb,
    Wifi,
    Other,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Loopback => "This computer",
            Kind::Ethernet => "Ethernet",
            Kind::Usb => "USB cable",
            Kind::Wifi => "Wi-Fi",
            Kind::Other => "Other network",
        }
    }
    /// Only wired links are gated; the rest keep their existing behavior.
    pub fn gated(self) -> bool {
        matches!(self, Kind::Ethernet | Kind::Usb)
    }
}

/// One network interface and whether SSH answers on it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Link {
    pub name: String,
    pub kind: Kind,
    /// A cable or radio link is present.
    pub connected: bool,
    pub address: String,
    /// The computer at the other end, when the neighbor table knows it.
    pub peer: String,
    /// SSH answers on this interface.
    pub open: bool,
}

/// The SSH identity of the computer on the other end, to be confirmed by the person.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Peer {
    pub address: String,
    pub port: u16,
    pub fingerprint: String,
    /// The fingerprint is already recorded as trusted.
    pub pinned: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Status {
    pub ethernet_open: bool,
    pub usb_open: bool,
    /// Whether a policy has been chosen yet; older systems keep every link open until then.
    pub configured: bool,
    pub links: Vec<Link>,
    /// This computer's public key, one OpenSSH line, ready to paste into an authorized_keys file.
    pub public_key: String,
    pub key_fingerprint: String,
    pub host_fingerprint: String,
    /// VNC is switched on in the configuration; it starts with the next desktop session.
    pub vnc_enabled: bool,
    /// The desktop is listening for VNC right now, and how many viewers are connected.
    pub vnc_listening: bool,
    pub vnc_viewers: u32,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Status,
    /// Let SSH in over a wired link. Needs the administrator's password once an account exists;
    /// before that, the person at the local console is the authority.
    Open {
        kind: Kind,
        #[serde(default)]
        username: String,
        #[serde(default)]
        password: Zeroizing<String>,
    },
    Close {
        kind: Kind,
    },
    /// Turn the desktop's VNC server on or off for the next desktop session. Turning it on takes
    /// the same authorization as opening a link.
    Vnc {
        on: bool,
        #[serde(default)]
        username: String,
        #[serde(default)]
        password: Zeroizing<String>,
    },
    /// Close Ethernet and USB SSH, end their sessions, and turn VNC off.
    CloseAll,
    /// Read the SSH host key presented by the computer on the Ethernet link.
    Peer,
    /// Trust the host key the person confirmed. The daemon scans again and refuses a mismatch.
    Pin {
        fingerprint: String,
    },
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Reply {
    pub ok: bool,
    pub message: String,
    pub status: Status,
    pub peer: Option<Peer>,
}
