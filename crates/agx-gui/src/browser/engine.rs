//! Lazy helper process, shared frame storage and bounded off-thread IPC.
use super::{
    gpu,
    protocol::{self, FRAME_BYTES, HEIGHT, WIDTH},
};
use slint::ComponentHandle;
use slint::{Rgba8Pixel, SharedPixelBuffer};
use std::{
    fs::File,
    io,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{fs::FileExt, net::UnixStream, process::CommandExt},
    },
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        mpsc::{self, SyncSender},
    },
    thread,
    time::Duration,
};

pub struct Frame {
    pub pixels: SharedPixelBuffer<Rgba8Pixel>,
    pub sequence: u64,
}
#[derive(Default)]
pub struct Updates {
    pub frame: Option<Frame>,
    pub gpu: Option<gpu::transport::Frame>,
    pub title: Option<String>,
    pub uri: Option<String>,
    pub status: Option<(u8, u8)>,
    pub error: Option<String>,
    pub download: Option<String>,
    pub cursor: Option<u8>,
    pub options: Option<Vec<protocol::OptionItem>>,
    pub dialog: Option<(u8, String, String)>,
    pub popup_closed: bool,
    pub close_requested: bool,
    pub tabs: Option<protocol::TabState>,
    pub ready: bool,
    pub dead: bool,
}
pub struct Engine {
    child: Option<Child>,
    tx: SyncSender<(u8, Vec<u8>)>,
    pub updates: Arc<Mutex<Updates>>,
}

fn configure_graphics(command: &mut Command) {
    let fallback = std::path::Path::new("/usr/lib/agx-web-mesa");
    if fallback.join("libEGL.so.1").is_file()
        && !["libEGL.so.1", "libGLESv2.so.2", "libgbm.so.1"]
            .into_iter()
            .all(|name| std::path::Path::new("/usr/lib").join(name).is_file())
    {
        command
            .env("LD_LIBRARY_PATH", fallback)
            .env("GBM_BACKENDS_PATH", fallback.join("gbm"))
            .env("LIBGL_DRIVERS_PATH", fallback.join("dri"))
            .env("LIBGL_ALWAYS_SOFTWARE", "1");
    }
}

impl Engine {
    pub fn start(ui: &crate::AppWindow) -> io::Result<Self> {
        Self::start_host(ui, false)
    }

    pub(crate) fn start_embedded(ui: &crate::AppWindow) -> io::Result<Self> {
        Self::start_host(ui, true)
    }

    fn start_host(ui: &crate::AppWindow, embedded: bool) -> io::Result<Self> {
        if unsafe { libc::geteuid() } == 0 {
            return Err(io::Error::other("Open Web from a signed-in user session."));
        }
        const WEB_HOST: &str = "/usr/libexec/agx-web";
        let executable = match std::env::var_os("AGX_WEB_HOST") {
            Some(path) => path,
            None if std::path::Path::new(WEB_HOST).is_file() => WEB_HOST.into(),
            None => return Err(io::Error::other("This image needs the ANGI WebKit runtime.")),
        };
        let (control, peer) = UnixStream::pair()?;
        // Both descriptors stay close-on-exec everywhere except the embedding helper.
        let fd = unsafe {
            libc::memfd_create(
                c"angi-web-frame".as_ptr(),
                libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let frame = unsafe { File::from_raw_fd(fd) };
        frame.set_len(FRAME_BYTES)?;
        if unsafe {
            libc::fcntl(
                fd,
                libc::F_ADD_SEALS,
                libc::F_SEAL_GROW | libc::F_SEAL_SHRINK | libc::F_SEAL_SEAL,
            )
        } < 0
        {
            return Err(io::Error::last_os_error());
        }
        let gpu_pair = if !embedded && gpu::available() {
            Some(gpu::transport::pair()?)
        } else {
            None
        };
        let socket = peer.as_raw_fd();
        let mut command = Command::new(executable);
        configure_graphics(&mut command);
        super::storage::configure_child(&mut command);
        command
            .args([
                "--control-fd",
                &socket.to_string(),
                "--frame-fd",
                &fd.to_string(),
            ])
            .env("AGX_WEB_TABS", if embedded { "0" } else { "1" })
            .env("AGX_WEB_EMBEDDED", if embedded { "1" } else { "0" })
            .env_remove("WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS")
            .env_remove("WEBKIT_INSPECTOR_SERVER")
            .env_remove("WEBKIT_INSPECTOR_HTTP_SERVER")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .process_group(0);
        let mut inherited = vec![socket, fd];
        if let Some((_, gpu_peer)) = &gpu_pair {
            let gpu_fd = gpu_peer.as_raw_fd();
            command.args(["--gpu-fd", &gpu_fd.to_string()]);
            inherited.push(gpu_fd);
        }
        let parent_pid = unsafe { libc::getpid() };
        unsafe {
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) < 0 {
                    return Err(io::Error::last_os_error());
                }
                if libc::getppid() != parent_pid {
                    return Err(io::Error::from_raw_os_error(libc::ECHILD));
                }
                for descriptor in &inherited {
                    if libc::fcntl(*descriptor, libc::F_SETFD, 0) < 0 {
                        return Err(io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
        let reader = control.try_clone()?;
        control.set_write_timeout(Some(Duration::from_millis(250)))?;
        let child = command.spawn()?;
        drop(peer);
        let updates = Arc::new(Mutex::new(Updates::default()));
        let shared = updates.clone();
        let weak = ui.as_weak();
        thread::spawn(move || {
            let notify = || {
                if !embedded {
                    let _ = weak.upgrade_in_event_loop(|ui| {
                        ui.global::<crate::Web>().invoke_engine_update()
                    });
                }
            };
            if let Err(error) = receive(reader, frame, &shared, &notify) {
                let mut state = shared.lock().unwrap();
                state.dead = true;
                state.error = Some(format!(
                    "Web stopped: {error}. Close and reopen Web to retry."
                ));
                drop(state);
                notify();
            }
        });
        if let Some((gpu_socket, gpu_peer)) = gpu_pair {
            drop(gpu_peer);
            let shared = updates.clone();
            let weak = ui.as_weak();
            thread::spawn(move || {
                loop {
                    match gpu::transport::receive(&gpu_socket) {
                        Ok(frame) => {
                            let mut state = shared.lock().unwrap();
                            if state.gpu.is_some() {
                                state.error = Some("Web sent an unacknowledged GPU frame".into());
                                state.dead = true;
                                break;
                            }
                            state.gpu = Some(frame);
                            drop(state);
                            // Queue a native draw immediately; do not wait on the 33ms CPU poll.
                            let _ = weak.upgrade_in_event_loop(|ui| ui.window().request_redraw());
                        }
                        Err(error) => {
                            if error.kind() != io::ErrorKind::UnexpectedEof {
                                let mut state = shared.lock().unwrap();
                                state.error = Some(format!("Web GPU transport stopped: {error}"));
                                state.dead = true;
                            }
                            break;
                        }
                    }
                }
            });
        }
        let (tx, rx) = mpsc::sync_channel::<(u8, Vec<u8>)>(128);
        thread::spawn(move || {
            let mut control = control;
            while let Ok((kind, data)) = rx.recv() {
                if protocol::write(&mut control, kind, &data).is_err() {
                    break;
                }
            }
            let _ = control.shutdown(std::net::Shutdown::Both);
        });
        Ok(Self {
            child: Some(child),
            tx,
            updates,
        })
    }
    #[cfg(feature = "kms")]
    pub(super) fn commands(&self) -> SyncSender<(u8, Vec<u8>)> {
        self.tx.clone()
    }
    pub fn send(&self, kind: u8, data: Vec<u8>) {
        // A stuck engine must never block the UI thread or grow an unbounded input queue.
        if self.tx.try_send((kind, data)).is_err() {
            let mut state = self.updates.lock().unwrap();
            state.dead = true;
            state.error = Some("Web is not responding. Close and reopen it to retry.".into());
        }
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // Let WebKit close its data stores without making the desktop wait.
            let _ = self.tx.try_send((12, vec![]));
            crate::session_refresh::spawn_cleanup(move || {
                let deadline = std::time::Instant::now() + Duration::from_millis(500);
                while std::time::Instant::now() < deadline {
                    if matches!(child.try_wait(), Ok(Some(_))) {
                        return;
                    }
                    thread::sleep(Duration::from_millis(20));
                }
                // A stuck helper owns a separate group, including its WebKit children.
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let _ = child.wait();
            });
        }
    }
}
fn receive(
    mut control: UnixStream,
    frame: File,
    shared: &Mutex<Updates>,
    notify: &impl Fn(),
) -> io::Result<()> {
    loop {
        let (kind, data) = protocol::read(&mut control)?;
        match kind {
            1 if data.len() == 16 => {
                let w = u32::from_be_bytes(data[..4].try_into().unwrap());
                let h = u32::from_be_bytes(data[4..8].try_into().unwrap());
                let sequence = u64::from_be_bytes(data[8..].try_into().unwrap());
                if w == 0 || h == 0 || w > WIDTH || h > HEIGHT {
                    return Err(io::Error::other("invalid web frame"));
                }
                let mut pixels = SharedPixelBuffer::<Rgba8Pixel>::new(w, h);
                frame.read_exact_at(pixels.make_mut_bytes(), 0)?;
                let mut state = shared.lock().unwrap();
                if state.frame.is_some() {
                    return Err(io::Error::other("web frame was not acknowledged"));
                }
                state.frame = Some(Frame { pixels, sequence });
                drop(state);
                notify();
            }
            2..=5 if data.len() <= 8192 => {
                let mut state = shared.lock().unwrap();
                if kind == 4 {
                    if data.len() != 2 {
                        return Err(io::Error::other("invalid web status"));
                    }
                    state.status = Some((data[0], data[1].min(100)));
                } else {
                    let value = String::from_utf8(data).map_err(io::Error::other)?;
                    match kind {
                        2 => state.title = Some(value),
                        3 => state.uri = Some(value),
                        _ => state.error = Some(value),
                    }
                }
            }
            6 if data == b"ANGIWEB1" => {
                shared.lock().unwrap().ready = true;
                notify();
            }
            7 if data.len() == 1 && [0, 6, 7].contains(&data[0]) => {
                shared.lock().unwrap().cursor = Some(data[0])
            }
            8 => shared.lock().unwrap().options = Some(protocol::options(&data)?),
            9 => shared.lock().unwrap().dialog = Some(protocol::dialog(&data)?),
            10 if data.is_empty() => shared.lock().unwrap().popup_closed = true,
            11 if data.is_empty() => shared.lock().unwrap().close_requested = true,
            12 => shared.lock().unwrap().tabs = Some(protocol::tabs(&data)?),
            13 if (1..=8192).contains(&data.len()) => {
                let text = String::from_utf8(data).map_err(io::Error::other)?;
                shared.lock().unwrap().download = Some(text);
            }
            _ => return Err(io::Error::other("invalid web host response")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn storage() -> File {
        let fd = unsafe { libc::memfd_create(c"web-test".as_ptr(), libc::MFD_CLOEXEC) };
        assert!(fd >= 0);
        let file = unsafe { File::from_raw_fd(fd) };
        file.set_len(FRAME_BYTES).unwrap();
        file
    }
    fn frame_message(w: u32, h: u32, sequence: u64) -> Vec<u8> {
        let mut data = w.to_be_bytes().to_vec();
        data.extend(h.to_be_bytes());
        data.extend(sequence.to_be_bytes());
        data
    }
    #[test]
    fn frame_transport_copies_rgba_and_keeps_bounded_metadata() {
        let frame = storage();
        frame
            .write_all_at(&[10, 20, 30, 255, 40, 50, 60, 255], 0)
            .unwrap();
        let (reader, mut writer) = UnixStream::pair().unwrap();
        protocol::write(&mut writer, 6, b"ANGIWEB1").unwrap();
        protocol::write(&mut writer, 2, b"Page title").unwrap();
        protocol::write(&mut writer, 4, &[7, 85]).unwrap();
        protocol::write(&mut writer, 1, &frame_message(2, 1, 9)).unwrap();
        drop(writer);
        let state = Mutex::new(Updates::default());
        assert!(receive(reader, frame, &state, &|| {}).is_err()); // Clean EOF ends the reader.
        let mut updates = state.lock().unwrap();
        assert!(updates.ready);
        assert_eq!(updates.title.as_deref(), Some("Page title"));
        assert_eq!(updates.status, Some((7, 85)));
        let frame = updates.frame.take().unwrap();
        assert_eq!(frame.sequence, 9);
        assert_eq!(frame.pixels.as_bytes(), &[10, 20, 30, 255, 40, 50, 60, 255]);
    }
    #[test]
    fn download_notices_are_bounded_utf8() {
        for (message, accepted) in [
            (&b"Saved a.zip to Downloads"[..], true),
            (&[0xff, 0xfe][..], false),
            (&[][..], false),
        ] {
            let (reader, mut writer) = UnixStream::pair().unwrap();
            protocol::write(&mut writer, 13, message).unwrap();
            drop(writer);
            let state = Mutex::new(Updates::default());
            assert!(receive(reader, storage(), &state, &|| {}).is_err());
            assert_eq!(state.lock().unwrap().download.is_some(), accepted);
        }
    }
    #[test]
    fn oversized_and_unacknowledged_frames_are_rejected() {
        for messages in [
            vec![frame_message(WIDTH + 1, 1, 1)],
            vec![frame_message(1, 1, 1), frame_message(1, 1, 2)],
        ] {
            let (reader, mut writer) = UnixStream::pair().unwrap();
            for message in messages {
                protocol::write(&mut writer, 1, &message).unwrap();
            }
            let state = Mutex::new(Updates::default());
            assert_eq!(
                receive(reader, storage(), &state, &|| {})
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::Other
            );
        }
    }
}
