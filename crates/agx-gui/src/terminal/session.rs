//! Alacritty's PTY/parser thread and bounded per-session terminal state.
use alacritty_terminal::{
    Term,
    event::{Event, EventListener, WindowSize},
    event_loop::{EventLoop, EventLoopSender, Msg},
    grid::Dimensions,
    sync::FairMutex,
    term::{Config, Osc52},
    tty,
};
use std::{
    io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Size {
    pub cols: usize,
    pub lines: usize,
}
impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.lines
    }
    fn screen_lines(&self) -> usize {
        self.lines
    }
    fn columns(&self) -> usize {
        self.cols
    }
}
impl Size {
    pub fn window(self) -> WindowSize {
        WindowSize {
            num_cols: self.cols as u16,
            num_lines: self.lines as u16,
            cell_width: 9,
            cell_height: 19,
        }
    }
}
pub(super) struct Signals {
    pub dirty: AtomicBool,
    pub exited: AtomicBool,
    pub title: Mutex<String>,
    pub size: Mutex<WindowSize>,
    sender: Mutex<Option<EventLoopSender>>,
}
#[derive(Clone)]
pub(super) struct Proxy(pub Arc<Signals>);
impl Proxy {
    pub(super) fn new(size: Size) -> Self {
        Self(Arc::new(Signals {
            dirty: AtomicBool::new(true),
            exited: AtomicBool::new(false),
            title: Mutex::new("Shell".into()),
            size: Mutex::new(size.window()),
            sender: Mutex::new(None),
        }))
    }
}
impl EventListener for Proxy {
    fn send_event(&self, event: Event) {
        let reply = match event {
            Event::Title(title) => {
                *self.0.title.lock().unwrap() = title
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(128)
                    .collect();
                None
            }
            Event::ResetTitle => {
                *self.0.title.lock().unwrap() = "Shell".into();
                None
            }
            Event::PtyWrite(text) => Some(text),
            Event::ColorRequest(index, format) => Some(format(super::render::palette(index))),
            Event::TextAreaSizeRequest(format) => Some(format(*self.0.size.lock().unwrap())),
            Event::Exit | Event::ChildExit(_) => {
                self.0.exited.store(true, Ordering::Release);
                None
            }
            _ => None,
        };
        if let Some(text) = reply.filter(|text| text.len() <= 4096) {
            if let Some(sender) = &*self.0.sender.lock().unwrap() {
                let _ = sender.send(Msg::Input(text.into_bytes().into()));
            }
        }
        self.0.dirty.store(true, Ordering::Release);
    }
}
/// What an Agent Angi terminal run is for: the operating system or a project folder. (Chat has its own
/// bubble view, `agent_chat.rs`.)
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AgentRun {
    Os { single: bool },
    /// A folder the agent runtime validates.
    Project { folder: String, single: bool },
}
/// Command line for Agent Angi: the operating system by default, or `--project DIR`.
pub(super) fn agent_arguments(prompt: String, run: AgentRun) -> Vec<String> {
    let mut arguments = vec!["angi".into(), "--prompt".into(), prompt];
    let single = match run {
        AgentRun::Os { single } => single,
        AgentRun::Project { folder, single } => {
            arguments.extend(["--project".into(), folder]);
            single
        }
    };
    if single {
        arguments.push("--single".into());
    }
    arguments
}
pub(super) struct Session {
    pub term: Arc<FairMutex<Term<Proxy>>>,
    pub signals: Arc<Signals>,
    pub size: Size,
    pub starting: bool,
    sender: EventLoopSender,
    worker: Option<JoinHandle<()>>,
    child_handle: OwnedFd,
    child_pid: i32,
    /// Started by `Session::herdr`: counts as a Herdr tab even before `agent` has exec'd into it.
    attaches: bool,
}
impl Session {
    pub fn new(size: Size) -> io::Result<Self> {
        Self::in_directory(size, None)
    }
    pub fn in_directory(size: Size, directory: Option<std::path::PathBuf>) -> io::Result<Self> {
        Self::start(size, None, directory)
    }
    pub fn working_directory(&self) -> Option<std::path::PathBuf> {
        let alive = || {
            let mut descriptor = libc::pollfd {
                fd: self.child_handle.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            unsafe { libc::poll(&mut descriptor, 1, 0) == 0 }
        };
        if !alive() {
            return None;
        }
        let directory = std::fs::read_link(format!("/proc/{}/cwd", self.child_pid)).ok()?;
        (alive() && directory.is_dir()).then_some(directory)
    }
    /// An installed program (an absolute path) running in its own tab, titled `title`.
    pub fn program(size: Size, path: &str, title: &str) -> io::Result<Self> {
        Self::start(size, Some((path, vec![], title)), None)
    }
    pub fn tui(size: Size) -> io::Result<Self> {
        Self::start(size, Some(("/usr/bin/agx", vec!["tui".into(), "--embedded".into()], "ANGI TUI")), None)
    }

    pub fn opencode(size: Size) -> io::Result<Self> {
        Self::start(size, Some(("/usr/bin/opencode", vec![], "OpenCode")), None)
    }
    /// Agent Angi on the operating system, or in a project folder.
    pub fn agent(size: Size, prompt: String, run: AgentRun) -> io::Result<Self> {
        Self::start(
            size,
            Some(("/usr/bin/agent", agent_arguments(prompt, run), "Agent Angi")),
            None,
        )
    }
    /// Herdr on the newest swarm that is still running (`agent attach`).
    pub fn herdr(size: Size) -> io::Result<Self> {
        let mut session = Self::start(size, Some(("/usr/bin/agent", vec!["attach".into()], "Agent Angi")), None)?;
        session.attaches = true;
        Ok(session)
    }
    /// Whether this tab is a live Herdr client. Closing such a tab leaves its swarm running.
    pub fn runs_herdr(&self) -> bool {
        if self.signals.exited.load(Ordering::Acquire) {
            return false;
        }
        self.attaches
            || std::fs::read(format!("/proc/{}/cmdline", self.child_pid))
                .ok()
                .and_then(|bytes| bytes.split(|b| *b == 0).next().map(<[u8]>::to_vec))
                .is_some_and(|program| program.rsplit(|b| *b == b'/').next() == Some(b"herdr"))
    }
    fn start(
        size: Size,
        command: Option<(&str, Vec<String>, &str)>,
        directory: Option<std::path::PathBuf>,
    ) -> io::Result<Self> {
        if unsafe { libc::geteuid() } == 0 {
            return Err(io::Error::other(
                "Open Terminal from a signed-in ANGI session.",
            ));
        }
        let proxy = Proxy::new(size);
        let signals = proxy.0.clone();
        let config = Config {
            scrolling_history: 10_000,
            osc52: Osc52::Disabled,
            kitty_keyboard: false,
            ..Config::default()
        };
        let term = Arc::new(FairMutex::new(Term::new(config, &size, proxy.clone())));
        let shell = std::env::var("AGX_TERMINAL_SHELL")
            .ok()
            .or_else(|| {
                std::path::Path::new("/bin/bash")
                    .is_file()
                    .then(|| "/bin/bash".into())
            })
            .or_else(|| std::env::var("SHELL").ok())
            .filter(|s| std::path::Path::new(s).is_absolute() && std::path::Path::new(s).is_file())
            .unwrap_or_else(|| "/bin/sh".into());
        let rcfile = "/usr/share/agx/terminal/bashrc";
        let args = if std::path::Path::new(&shell)
            .file_name()
            .is_some_and(|name| name == "bash")
        {
            if std::path::Path::new(rcfile).is_file() {
                vec!["--rcfile".into(), rcfile.into(), "-i".into()]
            } else {
                vec!["-i".into()]
            }
        } else {
            vec!["-l".into()]
        };
        let starting = command.is_some();
        let program = if let Some((program, arguments, title)) = command {
            *signals.title.lock().unwrap() = title.into();
            tty::Shell::new(program.into(), arguments)
        } else {
            tty::Shell::new(shell.clone(), args)
        };
        let options = tty::Options {
            shell: Some(program),
            working_directory: directory.or_else(|| std::env::var_os("HOME").map(Into::into)),
            env: [
                ("SHELL".into(), shell),
                ("TERM".into(), "xterm-256color".into()),
                ("COLORTERM".into(), "truecolor".into()),
                ("TERM_PROGRAM".into(), "ANGI".into()),
                ("AGX_TERMINAL_CONTEXT".into(), "gui".into()),
                ("LC_CTYPE".into(), "C.UTF-8".into()),
                // Apps installed from the launcher are found by name, even in terminals already open.
                ("PATH".into(), crate::library::terminal_path()),
            ]
            .into(),
            ..tty::Options::default()
        };
        let pty = tty::new(&options, size.window(), 0)?;
        let pid = pty.child().id() as i32;
        let descriptor = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as i32;
        if descriptor < 0 {
            return Err(io::Error::last_os_error());
        }
        let child_handle = unsafe { OwnedFd::from_raw_fd(descriptor) };
        let event_loop = EventLoop::new(term.clone(), proxy, pty, true, false)?;
        let sender = event_loop.channel();
        *signals.sender.lock().unwrap() = Some(sender.clone());
        // Drop the returned event loop/PTY immediately on child exit, even if
        // its tab remains open to inspect scrollback. Never retain a stale PID.
        let io_worker = event_loop.spawn();
        let worker = std::thread::spawn(move || {
            let _ = io_worker.join();
        });
        Ok(Self {
            term,
            signals,
            size,
            starting,
            sender,
            worker: Some(worker),
            child_handle,
            child_pid: pid,
            attaches: false,
        })
    }
    pub fn write(&self, bytes: Vec<u8>) {
        if !bytes.is_empty() && bytes.len() <= 1024 * 1024 {
            let _ = self.sender.send(Msg::Input(bytes.into()));
        }
    }
    pub fn resize(&mut self, size: Size) {
        if size == self.size {
            return;
        }
        self.term.lock().resize(size);
        self.size = size;
        *self.signals.size.lock().unwrap() = size.window();
        let _ = self.sender.send(Msg::Resize(size.window()));
        self.signals.dirty.store(true, Ordering::Release);
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.sender.send(Msg::Shutdown);
        // Child cleanup must not block window movement or rendering.
        if let Some(worker) = self.worker.take() {
            let handle = self.child_handle.try_clone().ok();
            crate::session_refresh::spawn_cleanup(move || {
                if let Some(fd) = &handle {
                    unsafe {
                        libc::syscall(
                            libc::SYS_pidfd_send_signal,
                            fd.as_raw_fd(),
                            libc::SIGHUP,
                            std::ptr::null::<libc::siginfo_t>(),
                            0,
                        );
                    }
                }
                for _ in 0..20 {
                    if worker.is_finished() {
                        let _ = worker.join();
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                // A pidfd keeps cleanup tied to this child even after its numeric PID is reused.
                if let Some(fd) = &handle {
                    unsafe {
                        libc::syscall(
                            libc::SYS_pidfd_send_signal,
                            fd.as_raw_fd(),
                            libc::SIGKILL,
                            std::ptr::null::<libc::siginfo_t>(),
                            0,
                        );
                    }
                }
                let _ = worker.join();
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_sessions_inherit_the_live_shell_directory_after_cd() {
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let size = Size {
            cols: 80,
            lines: 24,
        };
        let shell = Session::start(size, Some(("/bin/sh", vec!["-c".into(), "read -r start; cd /usr; read -r finish".into()], "Directory test")), Some(std::path::PathBuf::from("/tmp"))).unwrap();
        let wait_for = |session: &Session, expected: &str| {
            for _ in 0..100 {
                if session.working_directory().as_deref() == Some(std::path::Path::new(expected)) {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            panic!(
                "shell directory did not become {expected}: {:?}",
                session.working_directory()
            );
        };
        wait_for(&shell, "/tmp");
        shell.write(b"\r".to_vec());
        wait_for(&shell, "/usr");
        let inherited = Session::in_directory(size, shell.working_directory()).unwrap();
        wait_for(&inherited, "/usr");
    }
    #[test]
    fn a_tab_counts_as_herdr_only_while_its_client_is_alive() {
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let size = Size { cols: 80, lines: 24 };
        // `exec -a` makes the process look like `herdr --session …`, as after `agent` execs into it.
        let command = |script: &str| Some(("/bin/bash", vec!["-c".into(), script.into()], "Herdr test"));
        let client = Session::start(size, command("exec -a /usr/bin/herdr sleep 30"), None).unwrap();
        let shell = Session::start(size, command("exec sleep 30"), None).unwrap();
        let mut waited = 0;
        while !client.runs_herdr() && waited < 100 {
            std::thread::sleep(std::time::Duration::from_millis(20));
            waited += 1;
        }
        assert!(client.runs_herdr(), "an exec'd herdr client is a Herdr tab");
        assert!(!shell.runs_herdr(), "an ordinary program is not");
        // A tab started by the button counts at once, before `agent` has exec'd into Herdr.
        let attaching = Session::start(size, command("exec sleep 30"), None).map(|mut tab| { tab.attaches = true; tab }).unwrap();
        assert!(attaching.runs_herdr());
        // Once the client has gone, the tab is no longer one, so the button starts a new one.
        let ended = Session::start(size, command("exit 0"), None).map(|mut tab| { tab.attaches = true; tab }).unwrap();
        let mut waited = 0;
        while ended.runs_herdr() && waited < 100 {
            std::thread::sleep(std::time::Duration::from_millis(20));
            waited += 1;
        }
        assert!(!ended.runs_herdr(), "an exited tab is not a Herdr tab");
    }
    #[test]
    fn closing_a_tab_terminates_a_shell_that_ignores_hangup() {
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let session = Session::new(Size {
            cols: 80,
            lines: 24,
        })
        .unwrap();
        let handle = session.child_handle.try_clone().unwrap();
        session.write(b"trap '' HUP; read -r ANGI_TEST_BLOCK\r".to_vec());
        std::thread::sleep(std::time::Duration::from_millis(80));
        drop(session);
        let mut poll = libc::pollfd {
            fd: handle.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        assert_eq!(
            unsafe { libc::poll(&mut poll, 1, 3000) },
            1,
            "closing the tab must not leave its shell alive"
        );
        assert_ne!(poll.revents & libc::POLLIN, 0);
    }
}
