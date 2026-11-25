//! The Unix-socket service: access control, bounded connections and the gate's sync loop.
use crate::{gate::Gate, peer};
use agx_security::{
    Kind, Link, MAX_REQUEST, Paths, Policy, Reply, Request, SOCKET, Status, ensure_client_key,
    host_public_key, interfaces, socket_path,
};
use std::{
    fs::{self, OpenOptions},
    io::{self, BufRead, BufReader, Read, Write},
    net::{Ipv4Addr, SocketAddr},
    os::unix::{
        fs::{OpenOptionsExt, PermissionsExt},
        io::AsRawFd,
        net::{UnixListener, UnixStream},
    },
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

const IO_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_CONNECTIONS: usize = 16;
const SYNC_SECONDS: u64 = 2;

fn err(message: &str) -> io::Error {
    io::Error::other(message.to_owned())
}

struct Service {
    paths: Paths,
    gate: Gate,
}

pub fn run() -> io::Result<()> {
    let path = socket_path();
    let system = path == Path::new(SOCKET);
    if system {
        if unsafe { libc::geteuid() } != 0 {
            return Err(err("The security service requires root"));
        }
        let run = Path::new("/run/agx");
        fs::create_dir_all(run)?;
        fs::set_permissions(run, fs::Permissions::from_mode(0o750))?;
    }
    unsafe { libc::umask(0o077) };
    let mut lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path.with_extension("pid"))?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(err("The security service is already running"));
    }
    lock.set_len(0)?;
    writeln!(lock, "{}", std::process::id())?;
    if path.exists() {
        fs::remove_file(&path)?;
    }
    let listener = UnixListener::bind(&path)?;
    if system {
        agx_profile::service::prepare_access(SOCKET)?;
    }

    let port = std::env::var("AGX_SSH_GATE_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(22);
    let backend = std::env::var("AGX_SSH_BACKEND")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(SocketAddr::from((Ipv4Addr::LOCALHOST, 2022)));
    let service = Arc::new(Service { paths: Paths::system(), gate: Gate::new(port, backend) });
    let background = service.clone();
    thread::Builder::new().name("gate-sync".into()).spawn(move || loop {
        background.sync();
        thread::sleep(Duration::from_secs(SYNC_SECONDS));
    })?;

    let active = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        if active.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            active.fetch_sub(1, Ordering::SeqCst);
            continue;
        }
        let (service, active) = (service.clone(), active.clone());
        thread::spawn(move || {
            let _ = serve(&service, stream);
            active.fetch_sub(1, Ordering::SeqCst);
        });
    }
    Ok(())
}

impl Service {
    fn sync(&self) {
        Policy::adopt(&self.paths);
        let mut links = interfaces();
        if self.paths.image_policy.with_file_name("wired-ssh-only").is_file() {
            links.retain(|link| link.kind.gated());
        }
        self.gate.sync(Policy::load(&self.paths), &links);
    }

    fn status(&self) -> Status {
        let policy = Policy::load(&self.paths);
        let hostname = fs::read_to_string("/etc/hostname").unwrap_or_default();
        let client = ensure_client_key(&self.paths, hostname.trim()).unwrap_or_default();
        let host = host_public_key(&self.paths).unwrap_or_default();
        let vnc = agx_core::vnc_status();
        Status {
            ethernet_open: policy.ethernet,
            usb_open: policy.usb,
            configured: policy.configured,
            links: interfaces()
                .into_iter()
                .filter(|interface| interface.kind != Kind::Loopback)
                .map(|interface| Link {
                    open: self.gate.listening(&interface.name),
                    name: interface.name,
                    kind: interface.kind,
                    connected: interface.connected,
                    address: interface.address,
                    peer: interface.peer,
                })
                .collect(),
            public_key: client.public_key,
            key_fingerprint: client.fingerprint,
            host_fingerprint: host.fingerprint,
            vnc_enabled: agx_security::vnc_enabled(&self.paths),
            vnc_listening: !vnc.listeners.is_empty(),
            vnc_viewers: vnc.connections as u32,
        }
    }

    fn reply(&self, ok: bool, message: &str) -> Reply {
        Reply { ok, message: message.into(), status: self.status(), peer: None }
    }

    fn ethernet_peer(&self) -> Option<String> {
        interfaces()
            .into_iter()
            .find(|interface| interface.kind == Kind::Ethernet && interface.connected && !interface.peer.is_empty())
            .map(|interface| interface.peer)
    }

    fn handle(&self, request: Request, root: bool) -> Reply {
        match request {
            Request::Status => self.reply(true, ""),
            Request::Open { kind, username, password } => {
                if !kind.gated() {
                    return self.reply(false, "Only Ethernet and USB access are controlled here.");
                }
                if let Some(refusal) = self.refuse(root, &username, &password) {
                    return refusal;
                }
                self.change(kind, true)
            }
            Request::Vnc { on: true, username, password } => {
                if let Some(refusal) = self.refuse(root, &username, &password) {
                    return refusal;
                }
                self.vnc(true)
            }
            Request::Vnc { on: false, .. } => self.vnc(false),
            Request::CloseAll => {
                let closed = Policy::load(&self.paths).with(Kind::Ethernet, false).with(Kind::Usb, false);
                if let Err(error) = closed.save(&self.paths) {
                    return self.reply(false, &format!("Could not save the setting: {error}"));
                }
                // Gate first: ending the SSH sessions matters more than the VNC file.
                self.sync();
                match agx_security::set_vnc(&self.paths, false) {
                    Ok(()) => self.reply(true, "Ethernet and USB access are closed and remote desktop is off. The desktop stops serving VNC at your next sign-in."),
                    Err(error) => self.reply(false, &format!("Access is closed, but remote desktop could not be turned off: {error}")),
                }
            }
            Request::Close { kind } if kind.gated() => self.change(kind, false),
            Request::Close { .. } => self.reply(false, "Only Ethernet and USB access are controlled here."),
            Request::Peer => match self.ethernet_peer() {
                None => self.reply(false, "No computer is connected over Ethernet."),
                Some(address) => match peer::read(&self.paths, &address) {
                    Ok(found) => Reply { peer: Some(found), ..self.reply(true, "") },
                    Err(error) => self.reply(false, &error.to_string()),
                },
            },
            Request::Pin { fingerprint } => match self.ethernet_peer() {
                None => self.reply(false, "No computer is connected over Ethernet."),
                Some(address) => match peer::pin(&self.paths, &address, &fingerprint) {
                    Ok(found) => Reply { peer: Some(found), ..self.reply(true, "Trusted.") },
                    Err(error) => self.reply(false, &error.to_string()),
                },
            },
        }
    }

    /// The greeter runs as root at the local console, including the Security step right after
    /// the administrator account is created, so root needs no password. Everything else (the
    /// signed-in desktop) must prove it is the administrator. None means allowed.
    fn refuse(&self, root: bool, username: &str, password: &str) -> Option<Reply> {
        if root || agx_profile::service::authenticate(username, password).is_ok() {
            return None;
        }
        thread::sleep(Duration::from_secs(1));
        Some(self.reply(false, "That username and password did not match."))
    }

    fn vnc(&self, on: bool) -> Reply {
        match agx_security::set_vnc(&self.paths, on) {
            Ok(()) if on => self.reply(true, "Remote desktop is on. It starts at your next sign-in: sign out and back in."),
            Ok(()) => self.reply(true, "Remote desktop is off. It stops at your next sign-in."),
            Err(error) => self.reply(false, &format!("Could not change remote desktop: {error}")),
        }
    }

    fn change(&self, kind: Kind, open: bool) -> Reply {
        let policy = Policy::load(&self.paths).with(kind, open);
        // Saving a policy also settles one that was never chosen: both links keep their
        // current state, only the requested one changes.
        if let Err(error) = policy.save(&self.paths) {
            return self.reply(false, &format!("Could not save the setting: {error}"));
        }
        self.sync();
        self.reply(true, if open { "Access is open." } else { "Access is closed." })
    }
}

fn serve(service: &Service, mut stream: UnixStream) -> io::Result<()> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    let root = peer_uid(&stream) == Some(0);
    let mut line = Vec::new();
    BufReader::new((&mut stream).take(MAX_REQUEST as u64 + 1)).read_until(b'\n', &mut line)?;
    let reply = if line.len() > MAX_REQUEST {
        service.reply(false, "Security request is too large")
    } else {
        match serde_json::from_slice::<Request>(&line) {
            Ok(request) => service.handle(request, root),
            Err(_) => service.reply(false, "Invalid security request"),
        }
    };
    let mut bytes = serde_json::to_vec(&reply)?;
    bytes.push(b'\n');
    stream.write_all(&bytes)
}

fn peer_uid(stream: &UnixStream) -> Option<u32> {
    let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
    let mut length = size_of::<libc::ucred>() as libc::socklen_t;
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut length,
        )
    };
    (result == 0).then_some(credentials.uid)
}

