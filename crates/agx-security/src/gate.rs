//! The SSH gate. Dropbear listens on loopback only; the gate owns port 22 and accepts on one
//! socket per network interface, bound to that interface with SO_BINDTODEVICE. Closing a wired
//! link closes its socket and cuts its sessions, whatever address the link has or acquires.
use agx_security::{Interface, Policy};
use std::{
    collections::{HashMap, HashSet},
    io::{self, Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

const MAX_SESSIONS: usize = 64;

struct Listener {
    stop: Arc<AtomicBool>,
    sessions: Arc<Mutex<Vec<TcpStream>>>,
}

pub struct Gate {
    port: u16,
    backend: SocketAddr,
    listeners: Mutex<HashMap<String, Listener>>,
    count: Arc<AtomicUsize>,
}

impl Gate {
    pub fn new(port: u16, backend: SocketAddr) -> Self {
        Self { port, backend, listeners: Mutex::default(), count: Arc::default() }
    }

    /// Make the open sockets match the policy and the interfaces that exist right now.
    pub fn sync(&self, policy: Policy, interfaces: &[Interface]) {
        let wanted: HashSet<&str> = interfaces
            .iter()
            .filter(|interface| policy.allows(interface.kind))
            .map(|interface| interface.name.as_str())
            .collect();
        let mut listeners = self.listeners.lock().unwrap_or_else(|poison| poison.into_inner());
        let stale: Vec<String> = listeners
            .keys()
            .filter(|name| !wanted.contains(name.as_str()))
            .cloned()
            .collect();
        for name in stale {
            if let Some(listener) = listeners.remove(&name) {
                listener.stop.store(true, Ordering::SeqCst);
                for session in listener.sessions.lock().unwrap_or_else(|p| p.into_inner()).drain(..) {
                    let _ = session.shutdown(Shutdown::Both);
                }
            }
        }
        for name in wanted {
            if listeners.contains_key(name) {
                continue;
            }
            match bind(name, self.port) {
                Ok(listener) => {
                    let stop = Arc::new(AtomicBool::new(false));
                    let sessions = Arc::new(Mutex::new(Vec::new()));
                    let (flag, held) = (stop.clone(), sessions.clone());
                    let (backend, count) = (self.backend, self.count.clone());
                    if thread::Builder::new()
                        .name(format!("ssh-{name}"))
                        .spawn(move || accept(listener, backend, flag, held, count))
                        .is_ok()
                    {
                        listeners.insert(name.to_owned(), Listener { stop, sessions });
                    }
                }
                // The link may have no usable address yet; the next sync retries.
                Err(error) => eprintln!("agx-securityd: cannot listen on {name}: {error}"),
            }
        }
    }

    pub fn listening(&self, name: &str) -> bool {
        self.listeners
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .contains_key(name)
    }
}

fn bind(device: &str, port: u16) -> io::Result<TcpListener> {
    let dual = unsafe { libc::socket(libc::AF_INET6, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0) };
    let (fd, v6) = if dual >= 0 {
        (dual, true)
    } else {
        let fd = unsafe { libc::socket(libc::AF_INET, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        (fd, false)
    };
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    let raw = fd.as_raw_fd();
    let set = |level, option, value: &[u8]| {
        let result = unsafe {
            libc::setsockopt(raw, level, option, value.as_ptr().cast(), value.len() as libc::socklen_t)
        };
        if result == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
    };
    set(libc::SOL_SOCKET, libc::SO_REUSEADDR, &1i32.to_ne_bytes())?;
    set(libc::SOL_SOCKET, libc::SO_BINDTODEVICE, device.as_bytes())?;
    let result = if v6 {
        set(libc::IPPROTO_IPV6, libc::IPV6_V6ONLY, &0i32.to_ne_bytes())?;
        let address = libc::sockaddr_in6 {
            sin6_family: libc::AF_INET6 as libc::sa_family_t,
            sin6_port: port.to_be(),
            sin6_flowinfo: 0,
            sin6_addr: libc::in6_addr { s6_addr: [0; 16] },
            sin6_scope_id: 0,
        };
        unsafe {
            libc::bind(raw, (&address as *const libc::sockaddr_in6).cast(), size_of_val(&address) as u32)
        }
    } else {
        let address = libc::sockaddr_in {
            sin_family: libc::AF_INET as libc::sa_family_t,
            sin_port: port.to_be(),
            sin_addr: libc::in_addr { s_addr: 0 },
            sin_zero: [0; 8],
        };
        unsafe {
            libc::bind(raw, (&address as *const libc::sockaddr_in).cast(), size_of_val(&address) as u32)
        }
    };
    if result != 0 || unsafe { libc::listen(raw, 16) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let listener = TcpListener::from(fd);
    listener.set_nonblocking(true)?;
    Ok(listener)
}

fn accept(
    listener: TcpListener,
    backend: SocketAddr,
    stop: Arc<AtomicBool>,
    sessions: Arc<Mutex<Vec<TcpStream>>>,
    count: Arc<AtomicUsize>,
) {
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((client, _)) => {
                if count.fetch_add(1, Ordering::SeqCst) >= MAX_SESSIONS {
                    count.fetch_sub(1, Ordering::SeqCst);
                    continue;
                }
                let (stop, sessions, finished) = (stop.clone(), sessions.clone(), count.clone());
                let spawned = thread::Builder::new().name("ssh-session".into()).spawn(move || {
                    let _ = relay(client, backend, &stop, &sessions);
                    finished.fetch_sub(1, Ordering::SeqCst);
                });
                if spawned.is_err() {
                    count.fetch_sub(1, Ordering::SeqCst);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(200));
            }
            Err(_) => thread::sleep(Duration::from_millis(500)),
        }
    }
}

fn relay(
    client: TcpStream,
    backend: SocketAddr,
    stop: &AtomicBool,
    sessions: &Mutex<Vec<TcpStream>>,
) -> io::Result<()> {
    client.set_nonblocking(false)?;
    client.set_nodelay(true)?;
    let server = TcpStream::connect_timeout(&backend, Duration::from_secs(3))?;
    server.set_nodelay(true)?;
    {
        // A link closed while this session starts must not leave it behind.
        let mut held = sessions.lock().unwrap_or_else(|poison| poison.into_inner());
        if stop.load(Ordering::SeqCst) {
            return Ok(());
        }
        held.push(client.try_clone()?);
        held.push(server.try_clone()?);
    }
    let (mut from_client, mut to_server) = (client.try_clone()?, server.try_clone()?);
    let upstream = thread::spawn(move || {
        let _ = copy(&mut from_client, &mut to_server);
        let _ = to_server.shutdown(Shutdown::Both);
        let _ = from_client.shutdown(Shutdown::Both);
    });
    let (mut from_server, mut to_client) = (server.try_clone()?, client.try_clone()?);
    let _ = copy(&mut from_server, &mut to_client);
    let _ = client.shutdown(Shutdown::Both);
    let _ = server.shutdown(Shutdown::Both);
    let _ = upstream.join();
    let mut held = sessions.lock().unwrap_or_else(|poison| poison.into_inner());
    held.retain(|stream| stream.peer_addr().is_ok());
    Ok(())
}

fn copy(from: &mut TcpStream, to: &mut TcpStream) -> io::Result<()> {
    let mut buffer = [0u8; 16 * 1024];
    loop {
        let read = from.read(&mut buffer)?;
        if read == 0 {
            return Ok(());
        }
        to.write_all(&buffer[..read])?;
    }
}
