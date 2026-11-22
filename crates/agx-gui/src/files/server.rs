//! Session-owned Python static servers. Startup and process cleanup never block Slint.
use std::{
    collections::HashMap,
    io::{self, Read},
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

// Python chooses and binds the port before publishing it; no free-port race. Isolated mode
// prevents a selected directory's modules or PYTHONPATH from replacing the standard library.
const SCRIPT: &str = r#"
import functools, http.server, sys
handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=sys.argv[1])
with http.server.ThreadingHTTPServer(('127.0.0.1', 0), handler) as server:
    print(server.server_port, flush=True)
    server.serve_forever()
"#;

pub(crate) struct Server {
    child: Child,
    pub(crate) url: String,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    fn start(dir: &Path) -> io::Result<Self> {
        if !dir.is_dir() {
            return Err(io::Error::other("The selected folder no longer exists."));
        }
        let mut command = Command::new("python3");
        command.args(["-I", "-u", "-c", SCRIPT]).arg(dir);
        Self::launch(command)
    }

    pub(crate) fn launch(mut command: Command) -> io::Result<Self> {
        let child = command
            .current_dir("/")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| io::Error::other(format!("Could not start Python 3: {error}")))?;
        // Own the child immediately so every error path kills and reaps it.
        let mut server = Self { child, url: String::new() };
        let mut stdout = server.child.stdout.take().unwrap();
        let fd = stdout.as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut output = String::new();
        loop {
            match stdout.read_to_string(&mut output) {
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error),
            }
            if let Some((port, _)) = output.split_once('\n') {
                let port = port.trim().parse::<u16>().ok().filter(|port| *port != 0)
                    .ok_or_else(|| io::Error::other("Python returned an invalid server port."))?;
                server.url = format!("http://127.0.0.1:{port}/");
                return Ok(server);
            }
            if let Some(status) = server.child.try_wait()? {
                return Err(io::Error::other(format!("Python server exited during startup ({status}).")));
            }
            if Instant::now() >= deadline {
                return Err(io::Error::other("Python server did not become ready within five seconds."));
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    pub(crate) fn running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

pub(super) struct Servers {
    requests: Option<Sender<PathBuf>>,
    pub replies: Receiver<Result<String, String>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Servers {
    pub fn new() -> Self {
        let (requests, inbox) = mpsc::channel::<PathBuf>();
        let (replies, receiver) = mpsc::channel();
        let worker = thread::spawn(move || {
            let mut servers = HashMap::<PathBuf, Server>::new();
            while let Ok(dir) = inbox.recv() {
                servers.retain(|_, server| matches!(server.child.try_wait(), Ok(None)));
                let result = (|| {
                    let dir = dir.canonicalize()?;
                    if !servers.contains_key(&dir) {
                        servers.insert(dir.clone(), Server::start(&dir)?);
                    }
                    Ok::<_, io::Error>(servers[&dir].url.clone())
                })();
                if replies.send(result.map_err(|e| format!("Could not start Python server: {e}"))).is_err() {
                    break;
                }
            }
            // Dropping the map kills and reaps all session servers.
        });
        Self { requests: Some(requests), replies: receiver, worker: Some(worker) }
    }

    pub fn start(&self, dir: PathBuf) {
        let _ = self.requests.as_ref().unwrap().send(dir);
    }
}

impl Drop for Servers {
    fn drop(&mut self) {
        self.requests.take();
        if let Some(worker) = self.worker.take() {
            crate::session_refresh::spawn_cleanup(move || { let _ = worker.join(); });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::Write, net::TcpStream};

    #[test]
    fn serves_selected_folder_reuses_port_and_cleans_up() {
        let dir = std::env::temp_dir().join(format!("angi-python-server-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("index.html"), "ANGI server test").unwrap();
        // Isolated Python must ignore this local module.
        fs::write(dir.join("http.py"), "raise RuntimeError('wrong module')").unwrap();
        let mut servers = Servers::new();
        servers.start(dir.clone());
        let url = servers.replies.recv_timeout(Duration::from_secs(8)).unwrap().unwrap();
        let address = url.strip_prefix("http://").unwrap().trim_end_matches('/');
        let mut stream = TcpStream::connect(address).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        stream.write_all(b"GET / HTTP/1.0\r\nHost: localhost\r\n\r\n").unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.0 200"), "{response}");
        assert!(response.contains("ANGI server test"));
        servers.start(dir.join("."));
        assert_eq!(servers.replies.recv_timeout(Duration::from_secs(8)).unwrap().unwrap(), url);
        servers.start(dir.join("missing"));
        assert!(servers.replies.recv_timeout(Duration::from_secs(8)).unwrap().is_err());
        servers.requests.take();
        servers.worker.take().unwrap().join().unwrap();
        assert!(TcpStream::connect(address).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
