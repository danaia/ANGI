//! TCP side of the VNC server: accepts clients, keeps the latest captured frame, and
//! streams changed areas to each client from its own thread. Nothing here touches Slint;
//! the UI thread publishes frames and receives input through [`Shared`].
use super::{
    frame::Frame,
    handoff::{self, Handoff, Viewer},
    input::Input,
    resize::{self, Reply},
    rfb::{self, PixelFormat},
};
use std::{
    collections::{HashMap, VecDeque},
    io,
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    os::fd::AsRawFd,
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

mod session;
use session::stream_updates;

const MAX_CLIENTS: usize = 4;
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(5);
const IO_TIMEOUT: Duration = Duration::from_secs(10);
const SERVER_NAME: &str = "ANGI";

struct ClientState {
    /// An update request is outstanding (or the client is still connecting).
    pending: bool,
    /// The outstanding request asked for the whole window, not only changes.
    full: bool,
    format: PixelFormat,
    desktop_size: bool,
    extended_desktop_size: bool,
    layouts: VecDeque<Reply>,
    closed: bool,
    /// The connection and the picture size the viewer has, once connected; kept so a
    /// new process can adopt the viewer (see `handoff`).
    stream: Option<TcpStream>,
    size: (usize, usize),
}

#[derive(Default)]
struct Hub {
    frame: Option<Arc<Frame>>,
    sequence: u64,
    clients: HashMap<u64, ClientState>,
    next_id: u64,
    registration_closed: bool,
}

/// State shared between the UI thread and the network threads.
pub struct Shared {
    hub: Mutex<Hub>,
    changed: Condvar,
    input: Box<dyn Fn(Input) + Send + Sync>,
    started: Instant,
    last_input_ms: AtomicU64,
    listener: Mutex<Option<TcpListener>>,
    resize: Mutex<Option<Box<dyn Fn(u16, u16) -> Option<Frame> + Send + Sync>>>,
}

impl Shared {
    pub fn new(input: impl Fn(Input) + Send + Sync + 'static) -> Arc<Self> {
        Arc::new(Self {
            hub: Mutex::default(),
            changed: Condvar::new(),
            input: Box::new(input),
            started: Instant::now(),
            last_input_ms: AtomicU64::new(0),
            listener: Mutex::new(None),
            resize: Mutex::new(None),
        })
    }

    fn hub(&self) -> MutexGuard<'_, Hub> {
        self.hub
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn set_resize_handler(
        &self,
        handler: impl Fn(u16, u16) -> Option<Frame> + Send + Sync + 'static,
    ) {
        *self.resize.lock().unwrap_or_else(|p| p.into_inner()) = Some(Box::new(handler));
    }

    fn resize_desktop(&self, id: u64, request: resize::Request) -> io::Result<()> {
        // Serialize resize operations across viewers, never holding the hub while waiting for UI work.
        let handler = self.resize.lock().unwrap_or_else(|p| p.into_inner());
        {
            let hub = self.hub();
            let client = hub
                .clients
                .get(&id)
                .ok_or_else(|| io::Error::other("viewer ended"))?;
            if !client.extended_desktop_size || client.layouts.len() >= 16 {
                return Err(io::Error::other(
                    "unnegotiated or excessive desktop resize requests",
                ));
            }
        }
        let (status, frame) = if !request.valid() {
            (3, None)
        } else if let Some(handler) = handler.as_ref() {
            match handler(request.width, request.height) {
                Some(frame)
                    if (frame.width, frame.height)
                        == (usize::from(request.width), usize::from(request.height)) =>
                {
                    (0, Some(frame))
                }
                _ => (2, None),
            }
        } else {
            (1, None)
        };
        let mut hub = self.hub();
        if let Some(frame) = frame {
            hub.frame = Some(Arc::new(frame));
            hub.sequence += 1;
        }
        let (width, height) = hub
            .frame
            .as_ref()
            .map(|f| (f.width as u16, f.height as u16))
            .unwrap_or((1280, 720));
        for (&viewer, client) in &mut hub.clients {
            if client.extended_desktop_size && (viewer == id || status == 0) {
                if client.layouts.len() >= 16 {
                    client.closed = true;
                    continue;
                }
                client.layouts.push_back(Reply {
                    reason: if viewer == id { 1 } else { 2 },
                    status,
                    width,
                    height,
                });
            }
        }
        self.changed.notify_all();
        Ok(())
    }

    /// Whether some client is waiting for a frame, so the UI thread should capture one.
    pub fn wants_frame(&self) -> bool {
        let mut hub = self.hub();
        if hub.clients.is_empty() {
            // A capture may finish after the last viewer unregisters. Release
            // that late frame on the next timer poll as well.
            hub.frame = None;
            return false;
        }
        hub.clients
            .values()
            .any(|client| client.pending && !client.closed)
    }

    /// Input arrived in the last couple of seconds, so frames should follow quickly.
    pub fn input_is_recent(&self) -> bool {
        let last = self.last_input_ms.load(Ordering::Relaxed);
        last != 0 && self.started.elapsed().as_millis() as u64 - last < 2000
    }

    pub fn publish(&self, frame: Frame) {
        let mut hub = self.hub();
        if hub.frame.as_deref() == Some(&frame) {
            return;
        }
        hub.frame = Some(Arc::new(frame));
        hub.sequence += 1;
        self.changed.notify_all();
    }

    fn register(&self) -> Option<u64> {
        self.register_with(PixelFormat::NATIVE, false, (0, 0))
    }

    fn register_with(
        &self,
        format: PixelFormat,
        desktop_size: bool,
        size: (usize, usize),
    ) -> Option<u64> {
        let mut hub = self.hub();
        if hub.registration_closed || hub.clients.len() >= MAX_CLIENTS {
            return None;
        }
        hub.next_id += 1;
        let id = hub.next_id;
        hub.clients.insert(
            id,
            ClientState {
                pending: true,
                full: true,
                format,
                desktop_size,
                extended_desktop_size: false,
                layouts: VecDeque::new(),
                closed: false,
                stream: None,
                size,
            },
        );
        Some(id)
    }

    pub fn has_viewers(&self) -> bool {
        self.hub().clients.values().any(|client| !client.closed)
    }

    /// Commit to a local-only login if no VNC client is connected. Serialize with
    /// registration so a viewer cannot arrive between this check and exec.
    pub fn stop_accepting_if_idle(&self) -> bool {
        let mut hub = self.hub();
        if hub.clients.values().any(|client| !client.closed) {
            return false;
        }
        hub.registration_closed = true;
        true
    }

    /// Describe the listener and connected viewers so the next process can adopt them.
    /// The caller must keep these descriptors open across its exec.
    pub fn handoff(&self) -> Option<Handoff> {
        let listener = self
            .listener
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()?
            .as_raw_fd();
        let viewers = self
            .hub()
            .clients
            .values()
            .filter(|client| !client.closed)
            .filter_map(|client| {
                Some(Viewer {
                    fd: client.stream.as_ref()?.as_raw_fd(),
                    size: client.size,
                    desktop_size: client.desktop_size,
                    extended_desktop_size: client.extended_desktop_size,
                    format: client.format,
                })
            })
            .collect();
        Some(Handoff { listener, viewers })
    }

    fn unregister(&self, id: u64) {
        let released_capture = {
            let mut hub = self.hub();
            hub.clients.remove(&id);
            hub.clients.is_empty() && hub.frame.take().is_some()
        };
        // A full capture can leave several MiB in glibc arenas. Return free pages
        // once the last viewer leaves, outside the hub lock on the network thread.
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        if released_capture {
            // SAFETY: malloc_trim is thread-safe and does not take any Rust pointers.
            unsafe { libc::malloc_trim(0) };
        }
        #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
        let _ = released_capture;
    }

    fn update_client(&self, id: u64, change: impl FnOnce(&mut ClientState)) {
        if let Some(client) = self.hub().clients.get_mut(&id) {
            change(client);
        }
        self.changed.notify_all();
    }

    fn first_frame(&self) -> io::Result<Arc<Frame>> {
        let deadline = Instant::now() + FIRST_FRAME_TIMEOUT;
        let mut hub = self.hub();
        loop {
            if let Some(frame) = &hub.frame {
                return Ok(frame.clone());
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "no frame was captured",
                ));
            }
            hub = self
                .changed
                .wait_timeout(hub, left)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
    }

    /// Block until `id` has an outstanding request that a newer frame (or a full
    /// refresh) can answer. `None` once the client is gone.
    fn next_job(&self, id: u64, seen: u64) -> Option<Job> {
        let mut hub = self.hub();
        loop {
            let frame = hub.frame.clone();
            let sequence = hub.sequence;
            let client = hub.clients.get_mut(&id).filter(|client| !client.closed)?;
            if let Some(layout) = client.layouts.pop_front() {
                return Some(Job::Layout(layout));
            }
            if let (true, Some(frame)) = (client.pending, frame)
                && (client.full || sequence != seen)
            {
                let full = std::mem::take(&mut client.full);
                // Claim this request before sending. The reader may receive the
                // next one while the socket write is still in progress.
                client.pending = false;
                return Some(Job::Pixels {
                    frame,
                    sequence,
                    full,
                    format: client.format,
                    desktop_size: client.desktop_size,
                    extended_desktop_size: client.extended_desktop_size,
                });
            }
            hub = self
                .changed
                .wait_timeout(hub, Duration::from_secs(1))
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
    }

    fn note_input(&self) {
        self.last_input_ms.store(
            (self.started.elapsed().as_millis() as u64).max(1),
            Ordering::Relaxed,
        );
    }
}

enum Job {
    Layout(Reply),
    Pixels {
        frame: Arc<Frame>,
        sequence: u64,
        full: bool,
        format: PixelFormat,
        desktop_size: bool,
        extended_desktop_size: bool,
    },
}

/// Bind `addr` and serve clients on background threads.
pub fn listen(addr: SocketAddr, shared: Arc<Shared>) -> io::Result<()> {
    serve_listener(TcpListener::bind(addr)?, shared)
}

/// Serve clients arriving on `listener` on background threads.
pub fn serve_listener(listener: TcpListener, shared: Arc<Shared>) -> io::Result<()> {
    *shared
        .listener
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(listener.try_clone()?);
    thread::Builder::new()
        .name("vnc-accept".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let shared = shared.clone();
                let _ = thread::Builder::new()
                    .name("vnc-client".into())
                    .spawn(move || {
                        if let Err(error) = serve(stream, &shared)
                            && error.kind() != io::ErrorKind::UnexpectedEof
                        {
                            eprintln!("agx-gui: vnc client ended: {error}");
                        }
                    });
            }
        })?;
    Ok(())
}

fn serve(stream: TcpStream, shared: &Arc<Shared>) -> io::Result<()> {
    let Some(id) = shared.register() else {
        return Ok(());
    };
    let result = session(&stream, shared, id);
    shared.unregister(id);
    let _ = stream.shutdown(Shutdown::Both);
    result
}

/// Take over what the previous process left behind: the listening socket when it is
/// the one for `addr`, then each viewer, which is served without a new handshake and
/// gets a full picture as soon as one is captured. Returns whether the listener was
/// adopted; otherwise the caller binds its own.
pub fn adopt(handoff: &Handoff, addr: SocketAddr, shared: &Arc<Shared>) -> bool {
    let Some(listener) = handoff::adopt_listener(handoff.listener, addr) else {
        return false;
    };
    for viewer in &handoff.viewers {
        let Some(stream) = handoff::adopt_viewer(viewer.fd) else {
            continue;
        };
        let (shared, viewer) = (shared.clone(), viewer.clone());
        let _ = thread::Builder::new()
            .name("vnc-client".into())
            .spawn(move || {
                let Some(id) =
                    shared.register_with(viewer.format, viewer.desktop_size, viewer.size)
                else {
                    return;
                };
                shared.update_client(id, |client| {
                    client.extended_desktop_size = viewer.extended_desktop_size
                });
                let result = stream
                    .set_nodelay(true)
                    .and_then(|()| stream.set_write_timeout(Some(IO_TIMEOUT)))
                    .and_then(|()| stream_updates(&stream, &shared, id, viewer.size));
                shared.unregister(id);
                let _ = stream.shutdown(Shutdown::Both);
                if let Err(error) = result {
                    eprintln!("agx-gui: vnc viewer ended: {error}");
                }
            });
    }
    serve_listener(listener, shared.clone()).is_ok()
}

fn session(stream: &TcpStream, shared: &Arc<Shared>, id: u64) -> io::Result<()> {
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    // Only the dimensions are needed for the handshake. Keeping the Arc in
    // this session's scope would retain its first capture until disconnect.
    let known_size = {
        let first = shared.first_frame()?;
        (first.width, first.height)
    };
    let (width, height) = (
        u16::try_from(known_size.0).map_err(io::Error::other)?,
        u16::try_from(known_size.1).map_err(io::Error::other)?,
    );
    rfb::handshake(&mut &*stream, width, height, SERVER_NAME)?;
    stream.set_read_timeout(None)?;
    shared.update_client(id, |client| client.pending = false);
    stream_updates(stream, shared, id, known_size)
}

#[cfg(test)]
mod tests;
