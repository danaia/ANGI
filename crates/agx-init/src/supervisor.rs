//! Fixed service definitions, bounded lifecycle journal, restart backoff, and process-tree cleanup.
use crate::{Action, Log, Reply, Request, Service};
use std::{
    collections::{BTreeMap, VecDeque},
    fs, io,
    os::unix::{fs::OpenOptionsExt, process::CommandExt},
    process::{Child, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const LOG_LIMIT: usize = 512;
#[derive(Clone)]
pub struct Definition {
    pub name: &'static str,
    pub program: String,
    pub args: Vec<String>,
    pub controllable: bool,
}
impl Definition {
    fn new(name: &'static str, program: &str, args: &[&str], controllable: bool) -> Self {
        Self {
            name,
            program: program.into(),
            args: args.iter().map(|s| (*s).into()).collect(),
            controllable,
        }
    }
}
/// Keeps display and input devices usable by agx-admin; see the script header.
pub const DISPLAY_ACCESS: &str = "/usr/libexec/agx-display-access";
fn display_access() -> Definition {
    Definition::new("display-access", "/bin/sh", &[DISPLAY_ACCESS], false)
}
/// Tailscale in userspace mode, for Agent Angi's LM Studio; see apps/tailscale/run.sh.
pub const TAILSCALE: &str = "/usr/libexec/agx-tailscale/run";
fn tailscale() -> Definition {
    Definition::new("tailscale", "/bin/sh", &[TAILSCALE], true)
}
/// D-Bus, BlueZ and agx-bluetoothd as one chain; see apps/bluetooth/run.sh.
pub const BLUETOOTH: &str = "/usr/libexec/agx-bluetooth/run";
fn bluetooth() -> Definition {
    Definition::new("bluetooth", "/bin/sh", &[BLUETOOTH], true)
}
fn console_definition(gui: bool) -> Definition {
    if gui {
        Definition::new("console", "/usr/bin/agx-gui", &["--login"], false)
    } else {
        Definition::new(
            "console",
            "/sbin/getty",
            &["-L", "tty1", "0", "vt100"],
            false,
        )
    }
}

/// Give the greeter its own controlling terminal, without a text login first.
/// Serial and SSH recovery are independent supervised services.
fn attach_graphical_console() -> io::Result<()> {
    unsafe {
        if libc::setsid() < 0 {
            return Err(io::Error::last_os_error());
        }
        if libc::chown(c"/dev/tty1".as_ptr(), 0, 0) != 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = libc::open(c"/dev/tty1".as_ptr(), libc::O_RDWR | libc::O_NOCTTY);
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        if libc::ioctl(fd, libc::TIOCSCTTY, 0) < 0 || libc::ioctl(fd, 0x5606, 1) < 0 {
            // VT_ACTIVATE
            libc::close(fd);
            return Err(io::Error::last_os_error());
        }
        for target in [0, 1] {
            if libc::dup2(fd, target) < 0 {
                libc::close(fd);
                return Err(io::Error::last_os_error());
            }
        }
        if fd > 2 {
            libc::close(fd);
        }
        let mut terminal: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(0, &mut terminal) != 0 {
            return Err(io::Error::last_os_error());
        }
        libc::cfmakeraw(&mut terminal);
        if libc::tcsetattr(0, libc::TCSAFLUSH, &terminal) != 0 {
            return Err(io::Error::last_os_error());
        }
        // Recover a VT left in graphics mode after a forced termination.
        libc::ioctl(0, 0x4B3A, 0); // KDSETMODE, KD_TEXT
    }
    Ok(())
}
/// Serial login devices in preference order (Pi 5 debug UART, then the Pi 4 mini UART).
const SERIAL_CONSOLES: [&str; 3] = ["ttyAMA10", "ttyS0", "ttyAMA0"];

fn configured_serial_console(cmdline: &str) -> Option<&'static str> {
    cmdline
        .split_ascii_whitespace()
        .filter_map(|arg| arg.strip_prefix("console="))
        .filter_map(|console| console.split(',').next())
        .filter_map(|tty| {
            SERIAL_CONSOLES
                .into_iter()
                .find(|supported| *supported == tty)
        })
        .last()
}
pub fn definitions(pid1: bool) -> Vec<Definition> {
    let mut services = Vec::new();
    if pid1 {
        services.extend([
            Definition::new(
                "syslog",
                "/sbin/syslogd",
                &["-n", "-s", "128", "-b", "2"],
                false,
            ),
            Definition::new("kernel-log", "/sbin/klogd", &["-n"], false),
            Definition::new(
                "ssh",
                "/usr/sbin/dropbear",
                &["-F", "-E", "-g", "-p", "22", "-R"],
                false,
            ),
            console_definition(std::path::Path::new("/usr/bin/agx-gui").is_file()),
        ]);
        // Follow the firmware command line when a board exposes multiple UARTs.
        let cmdline = fs::read_to_string("/proc/cmdline").unwrap_or_default();
        if let Some(tty) = configured_serial_console(&cmdline)
            .into_iter()
            .chain(SERIAL_CONSOLES)
            .find(|tty| std::path::Path::new("/dev").join(tty).exists())
        {
            services.push(Definition::new(
                "serial-console",
                "/sbin/getty",
                &["-L", tty, "115200", "vt100"],
                false,
            ));
        }
        services.push(Definition::new("cron", "/usr/sbin/crond", &["-f"], true));
        // Only images that ship the script (the desktop) run this service.
        if std::path::Path::new(DISPLAY_ACCESS).is_file() {
            services.push(display_access());
        }
        // Likewise only where scripts/deploy-tailscale.sh installed it.
        if std::path::Path::new(TAILSCALE).is_file() {
            services.push(tailscale());
        }
        // Only images built with BlueZ ship the script.
        if std::path::Path::new(BLUETOOTH).is_file() {
            services.push(bluetooth());
        }
    }
    services.push(Definition::new("network", "/usr/sbin/agx-netd", &[], true));
    services
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Identity {
    pid: i32,
    start: u64,
    parent: i32,
}
fn identity(pid: i32) -> Option<Identity> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (_, tail) = stat.rsplit_once(") ")?;
    let fields: Vec<_> = tail.split_whitespace().collect();
    Some(Identity {
        pid,
        parent: fields.get(1)?.parse().ok()?,
        start: fields.get(19)?.parse().ok()?,
    })
}
fn signal_process(process: Identity, signal: i32) {
    if identity(process.pid).is_some_and(|current| current.start == process.start) {
        unsafe {
            libc::kill(process.pid, signal);
        }
    }
}
struct Managed {
    definition: Definition,
    child: Option<Child>,
    descendants: BTreeMap<i32, Identity>,
    desired: bool,
    stopping: Option<Instant>,
    retry: Instant,
    started: Instant,
    failures: u32,
    starts: u64,
    last_exit: Option<String>,
}
impl Managed {
    fn new(definition: Definition) -> Self {
        Self {
            definition,
            child: None,
            descendants: BTreeMap::new(),
            desired: true,
            stopping: None,
            retry: Instant::now(),
            started: Instant::now(),
            failures: 0,
            starts: 0,
            last_exit: None,
        }
    }
    fn state(&self) -> Service {
        Service {
            name: self.definition.name.into(),
            state: if self.stopping.is_some() {
                "stopping"
            } else if self.child.is_some() {
                "running"
            } else if !self.desired {
                "stopped"
            } else {
                "backoff"
            }
            .into(),
            pid: self.child.as_ref().map(Child::id),
            starts: self.starts,
            last_exit: self.last_exit.clone(),
            controllable: self.definition.controllable,
        }
    }
    fn observe_tree(&mut self, processes: &[Identity]) {
        self.descendants
            .retain(|pid, process| identity(*pid).is_some_and(|p| p.start == process.start));
        if let Some(child) = &self.child
            && let Some(process) = identity(child.id() as i32)
        {
            self.descendants.insert(process.pid, process);
        }
        loop {
            let before = self.descendants.len();
            for process in processes {
                if self.descendants.contains_key(&process.parent) {
                    self.descendants.insert(process.pid, *process);
                }
            }
            if before == self.descendants.len() {
                break;
            }
        }
    }
    fn signal(&self, signal: i32) {
        // Signal only recorded PID/start-time pairs, including children in separate process groups.
        for process in self.descendants.values().rev() {
            signal_process(*process, signal);
        }
    }
    fn stop(&mut self) {
        if self.child.is_some() && self.stopping.is_none() {
            // Give the service leader a chance to run its own shutdown and preserve settings.
            if let Some(child) = &self.child
                && let Some(process) = identity(child.id() as i32)
            {
                signal_process(process, libc::SIGTERM);
            }
            self.stopping = Some(Instant::now());
        }
    }
}
pub struct Supervisor {
    services: Vec<Managed>,
    journal: VecDeque<Log>,
    sequence: u64,
    pub power: Option<(bool, Instant)>,
    pid1: bool,
}
impl Supervisor {
    pub fn new(definitions: Vec<Definition>, pid1: bool) -> Self {
        let mut manager = Self {
            services: definitions.into_iter().map(Managed::new).collect(),
            journal: VecDeque::new(),
            sequence: 0,
            power: None,
            pid1,
        };
        manager.log("init", "Rust service manager started");
        manager
    }
    fn log(&mut self, service: &str, message: &str) {
        self.sequence += 1;
        self.journal.push_back(Log {
            sequence: self.sequence,
            unix_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            service: service.into(),
            message: message.into(),
        });
        while self.journal.len() > LOG_LIMIT {
            self.journal.pop_front();
        }
    }
    pub fn tick(&mut self) {
        let processes: Vec<_> = fs::read_dir("/proc")
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| entry.file_name().to_str()?.parse().ok())
            .filter_map(identity)
            .collect();
        let mut events = Vec::new();
        for service in &mut self.services {
            service.observe_tree(&processes);
            if service
                .stopping
                .is_some_and(|at| at.elapsed() >= Duration::from_secs(5))
            {
                service.signal(libc::SIGKILL);
            }
            if let Some(child) = &mut service.child {
                match child.try_wait() {
                    Ok(Some(exit)) => {
                        let expected = service.stopping.take().is_some();
                        service.signal(libc::SIGKILL);
                        service.child = None;
                        service.last_exit = Some(exit.to_string());
                        if !expected {
                            service.failures =
                                if service.started.elapsed() > Duration::from_secs(30) {
                                    1
                                } else {
                                    service.failures.saturating_add(1)
                                };
                        }
                        service.retry = Instant::now()
                            + if expected {
                                Duration::ZERO
                            } else {
                                backoff(service.failures)
                            };
                        events.push((
                            service.definition.name,
                            format!(
                                "Exited: {exit}{}",
                                if expected {
                                    " (requested)"
                                } else {
                                    "; restart scheduled"
                                }
                            ),
                        ));
                    }
                    Err(error) => {
                        events.push((service.definition.name, format!("Wait failed: {error}")));
                    }
                    Ok(None) => (),
                }
            }
            if service.child.is_none()
                && service.desired
                && Instant::now() >= service.retry
                && self.power.is_none()
            {
                let mut command = Command::new(&service.definition.program);
                command
                    .args(&service.definition.args)
                    .env("PATH", "/sbin:/usr/sbin:/bin:/usr/bin")
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                if service.definition.name == "console" && service.definition.args == ["--login"] {
                    // The greeter and unprivileged session inherit the same stderr.
                    // Keep profiling output available over SSH; rotate between sessions.
                    let log = "/run/agx/gui.log";
                    if fs::metadata(log).is_ok_and(|m| m.len() > 4 * 1024 * 1024) {
                        let _ = fs::rename(log, "/run/agx/gui.previous.log");
                    }
                    if let Ok(log) = fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .mode(0o600)
                        .custom_flags(libc::O_NOFOLLOW)
                        .open(log)
                    {
                        command.stderr(log);
                    }
                    unsafe {
                        command.pre_exec(attach_graphical_console);
                    }
                }
                match command.spawn() {
                    Ok(child) => {
                        let pid = child.id();
                        service.child = Some(child);
                        service.started = Instant::now();
                        service.starts += 1;
                        if let Some(process) = identity(pid as i32) {
                            service.descendants.insert(process.pid, process);
                        }
                        events.push((service.definition.name, format!("Started PID {pid}")));
                    }
                    Err(error) => {
                        service.failures = service.failures.saturating_add(1);
                        service.retry = Instant::now() + backoff(service.failures);
                        service.last_exit = Some(error.to_string());
                        events.push((
                            service.definition.name,
                            format!("Start failed: {error}; retry scheduled"),
                        ));
                    }
                }
            }
        }
        for (name, event) in events {
            self.log(name, &event);
        }
        // Reap only untracked children here; never race Child::try_wait for managed leaders.
        let tracked: Vec<_> = self
            .services
            .iter()
            .filter_map(|s| s.child.as_ref().map(Child::id))
            .collect();
        for process in processes
            .iter()
            .filter(|p| p.parent == std::process::id() as i32 && !tracked.contains(&(p.pid as u32)))
        {
            unsafe {
                libc::waitpid(process.pid, std::ptr::null_mut(), libc::WNOHANG);
            }
        }
    }
    pub fn handle(&mut self, request: Request) -> Reply {
        let mut ok = true;
        let mut message = "Services sampled".to_owned();
        let mut logs = Vec::new();
        let mut truncated = false;
        match request {
            Request::Status => (),
            Request::Logs { service, after } => {
                truncated = after > 0
                    && self
                        .journal
                        .front()
                        .is_some_and(|entry| after.saturating_add(1) < entry.sequence);
                logs = self
                    .journal
                    .iter()
                    .filter(|entry| {
                        entry.sequence > after
                            && service.as_ref().is_none_or(|s| s == &entry.service)
                    })
                    .cloned()
                    .collect();
                message = "Service lifecycle journal (this boot; latest 512 events)".into();
            }
            Request::Control {
                name,
                action,
                confirmed,
            } => {
                if self.power.is_some() {
                    ok = false;
                    message = "System is shutting down".into();
                } else if let Some(service) =
                    self.services.iter_mut().find(|s| s.definition.name == name)
                {
                    if !service.definition.controllable {
                        ok = false;
                        message = "Recovery service is protected".into();
                    } else if !matches!(action, Action::Start) && !confirmed {
                        ok = false;
                        message = "Stop/restart requires confirmation".into();
                    } else {
                        match action {
                            Action::Start => {
                                service.desired = true;
                                service.retry = Instant::now();
                            }
                            Action::Stop => {
                                service.desired = false;
                                service.stop();
                            }
                            Action::Restart => {
                                service.desired = true;
                                service.retry = Instant::now();
                                service.stop();
                            }
                        }
                        message = format!("{name}: {action:?} requested");
                    }
                } else {
                    ok = false;
                    message = "Unknown service".into();
                }
                self.log("init", &message);
            }
            Request::Reboot { confirmed } | Request::Shutdown { confirmed } => {
                if !confirmed {
                    ok = false;
                    message = "Power operation requires confirmation".into();
                } else {
                    let reboot = matches!(request, Request::Reboot { .. });
                    self.power = Some((reboot, Instant::now() + Duration::from_secs(2)));
                    message = if reboot {
                        "Reboot requested. SSH sessions will close."
                    } else {
                        "Shutdown requested. SSH sessions will close."
                    }
                    .into();
                    self.log("init", &message);
                }
            }
        }
        Reply {
            ok,
            message,
            pid1: self.pid1,
            services: self.services.iter().map(Managed::state).collect(),
            logs,
            last_sequence: self.sequence,
            logs_truncated: truncated,
        }
    }
    pub fn stop_all(&mut self) {
        for service in &mut self.services {
            service.desired = false;
            service.stop();
        }
    }
    pub fn all_stopped(&self) -> bool {
        self.services.iter().all(|s| s.child.is_none())
    }
}
impl Drop for Supervisor {
    fn drop(&mut self) {
        for service in &mut self.services {
            service.signal(libc::SIGKILL);
            if let Some(mut child) = service.child.take() {
                let _ = child.wait();
            }
        }
    }
}
fn backoff(failures: u32) -> Duration {
    Duration::from_secs(1u64 << failures.saturating_sub(1).min(5))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firmware_console_selects_the_uart_for_each_pi() {
        for tty in SERIAL_CONSOLES {
            let args = format!("console=tty1 console={tty},115200 rootwait");
            assert_eq!(configured_serial_console(&args), Some(tty));
        }
        assert_eq!(configured_serial_console("console=tty1 rootwait"), None);
    }

    fn pump(manager: &mut Supervisor, until: impl Fn(&mut Supervisor) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            manager.tick();
            if until(manager) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "supervisor did not reach expected state"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    #[test]
    fn tailscale_is_a_controllable_service_run_through_its_script() {
        let definition = tailscale();
        assert_eq!(definition.name, "tailscale");
        assert_eq!(definition.program, "/bin/sh");
        assert_eq!(definition.args, [TAILSCALE]);
        assert!(definition.controllable);
        // Defined only where deploy-tailscale.sh installed the script, never by default.
        let present = definitions(true).iter().any(|service| service.name == "tailscale");
        assert_eq!(present, std::path::Path::new(TAILSCALE).is_file());
    }
    #[test]
    fn bluetooth_is_a_controllable_service_run_through_its_script() {
        let definition = bluetooth();
        assert_eq!(definition.name, "bluetooth");
        assert_eq!(definition.program, "/bin/sh");
        assert_eq!(definition.args, [BLUETOOTH]);
        assert!(definition.controllable);
        // Defined only where the image installed the script, never by default.
        let present = definitions(true).iter().any(|service| service.name == "bluetooth");
        assert_eq!(present, std::path::Path::new(BLUETOOTH).is_file());
    }
    #[test]
    fn desktop_console_enters_greeter_and_headless_keeps_getty() {
        let gui = console_definition(true);
        assert_eq!(gui.name, "console");
        assert_eq!(gui.program, "/usr/bin/agx-gui");
        assert_eq!(gui.args, ["--login"]);
        assert!(!gui.controllable);
        let text = console_definition(false);
        assert_eq!(text.program, "/sbin/getty");
        assert_eq!(text.args, ["-L", "tty1", "0", "vt100"]);
    }
    #[test]
    fn real_process_crash_restarts_and_stop_does_not_restart() {
        let mut manager = Supervisor::new(
            vec![Definition::new("test", "/bin/sleep", &["60"], true)],
            false,
        );
        pump(&mut manager, |m| m.services[0].starts == 1);
        let pid = manager.services[0].child.as_ref().unwrap().id();
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
        pump(&mut manager, |m| m.services[0].starts == 2);
        assert_ne!(manager.services[0].child.as_ref().unwrap().id(), pid);
        assert!(
            !manager
                .handle(Request::Control {
                    name: "test".into(),
                    action: Action::Stop,
                    confirmed: false
                })
                .ok
        );
        assert!(
            manager
                .handle(Request::Control {
                    name: "test".into(),
                    action: Action::Stop,
                    confirmed: true
                })
                .ok
        );
        pump(&mut manager, |m| m.all_stopped());
        manager.tick();
        assert_eq!(manager.services[0].starts, 2);
        assert!(!manager.services[0].desired);
    }
    #[test]
    fn display_access_is_a_protected_shell_service() {
        let d = display_access();
        assert_eq!(d.name, "display-access");
        assert_eq!(d.program, "/bin/sh");
        assert_eq!(d.args, [DISPLAY_ACCESS]);
        assert!(!d.controllable, "a stray stop would lock the desktop out");
    }

    #[test]
    fn protected_services_and_bounded_filtered_journal() {
        let mut manager = Supervisor::new(
            vec![Definition::new("ssh", "/bin/false", &[], false)],
            false,
        );
        assert!(
            !manager
                .handle(Request::Control {
                    name: "ssh".into(),
                    action: Action::Stop,
                    confirmed: true
                })
                .ok
        );
        assert!(!manager.handle(Request::Reboot { confirmed: false }).ok);
        for _ in 0..600 {
            manager.log("network", "Started");
        }
        let reply = manager.handle(Request::Logs {
            service: Some("network".into()),
            after: 1,
        });
        assert!(reply.logs_truncated);
        assert_eq!(reply.logs.len(), LOG_LIMIT);
        assert!(reply.logs.iter().all(|entry| entry.service == "network"));
        assert!(
            manager
                .handle(Request::Logs {
                    service: Some("ssh".into()),
                    after: 0
                })
                .logs
                .is_empty()
        );
    }
    #[test]
    fn spawn_failure_backs_off_without_busy_looping() {
        let mut manager = Supervisor::new(
            vec![Definition::new(
                "missing",
                "/no/such/agx-service",
                &[],
                true,
            )],
            false,
        );
        manager.tick();
        for _ in 0..20 {
            manager.tick();
        }
        assert_eq!(manager.services[0].failures, 1);
        assert!(manager.services[0].state().last_exit.is_some());
    }
}
