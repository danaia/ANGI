//! Shared service-manager protocol. The control socket is restricted to AGX administrators.
use serde::{Deserialize, Serialize};
use std::{
    io::{self, BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    time::Duration,
};
pub mod supervisor;
pub const SOCKET: &str = "/run/agx/init.sock";
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Start,
    Stop,
    Restart,
}
#[derive(Deserialize, Serialize)]
pub enum Request {
    Status,
    Logs {
        service: Option<String>,
        after: u64,
    },
    Control {
        name: String,
        action: Action,
        confirmed: bool,
    },
    Reboot {
        confirmed: bool,
    },
    Shutdown {
        confirmed: bool,
    },
    DriverPreparation {
        start: bool,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum DriverPreparation {
    Available,
    Working,
    Ready,
    RestartRequired,
    Incomplete(String),
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Service {
    pub name: String,
    pub state: String,
    pub pid: Option<u32>,
    pub starts: u64,
    pub last_exit: Option<String>,
    pub controllable: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Log {
    pub sequence: u64,
    pub unix_seconds: u64,
    pub service: String,
    pub message: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Reply {
    pub ok: bool,
    pub message: String,
    pub pid1: bool,
    pub services: Vec<Service>,
    pub logs: Vec<Log>,
    pub last_sequence: u64,
    pub logs_truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver_preparation: Option<DriverPreparation>,
}
pub fn request(request: &Request) -> io::Result<Reply> {
    let mut socket = UnixStream::connect(SOCKET)
        .map_err(|e| io::Error::new(e.kind(), "ANGI service manager unavailable"))?;
    socket.set_read_timeout(Some(Duration::from_secs(3)))?;
    socket.set_write_timeout(Some(Duration::from_secs(2)))?;
    serde_json::to_writer(&mut socket, request)?;
    socket.write_all(b"\n")?;
    let mut bytes = Vec::new();
    BufReader::new(socket)
        .take(262145)
        .read_until(b'\n', &mut bytes)?;
    if bytes.len() > 262144 {
        return Err(io::Error::other("Service manager response too large"));
    }
    serde_json::from_slice(&bytes).map_err(|_| io::Error::other("Invalid service manager response"))
}
