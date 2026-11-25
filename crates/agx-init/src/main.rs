//! Rust PID 1, with BusyBox init retained as the boot-error recovery path.
mod boot_ssh;
mod driver_setup;

use agx_init::{
    Request,
    supervisor::{SECURITY, Supervisor, definitions, ssh_listen_address},
};
use std::{
    fs::{self, OpenOptions},
    io::{self, BufRead, BufReader, Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            fs::{OpenOptionsExt, PermissionsExt},
            net::UnixListener,
            process::CommandExt,
        },
    },
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
fn command(program: &str, args: &[&str]) -> io::Result<()> {
    if Command::new(program)
        .args(args)
        .env("PATH", "/sbin:/usr/sbin:/bin:/usr/bin")
        .stdin(Stdio::null())
        .status()?
        .success()
    {
        Ok(())
    } else {
        Err(io::Error::other(format!("{program} failed")))
    }
}
fn mount(kind: &str, source: &str, target: &str, options: &str) -> io::Result<()> {
    fs::create_dir_all(target)?;
    if fs::read_to_string("/proc/mounts")
        .unwrap_or_default()
        .lines()
        .any(|line| line.split_whitespace().nth(1) == Some(target))
    {
        return Ok(());
    }
    command("/bin/mount", &["-t", kind, "-o", options, source, target])
}
fn bootstrap() -> io::Result<()> {
    mount("proc", "proc", "/proc", "defaults")?;
    command("/bin/mount", &["-o", "remount,rw", "/"])?;
    mount("devtmpfs", "devtmpfs", "/dev", "defaults")?;
    mount("sysfs", "sysfs", "/sys", "defaults")?;
    mount("tmpfs", "tmpfs", "/run", "mode=0755,nosuid,nodev")?;
    mount("tmpfs", "tmpfs", "/tmp", "mode=1777,nosuid,nodev")?;
    mount("tmpfs", "tmpfs", "/dev/shm", "mode=1777,nosuid,nodev")?;
    mount("devpts", "devpts", "/dev/pts", "gid=5,mode=620")?;
    fs::create_dir_all("/run/lock/subsys")?;
    for (source, target) in [
        ("/proc/self/fd", "/dev/fd"),
        ("/proc/self/fd/0", "/dev/stdin"),
        ("/proc/self/fd/1", "/dev/stdout"),
        ("/proc/self/fd/2", "/dev/stderr"),
    ] {
        if !std::path::Path::new(target).exists() {
            std::os::unix::fs::symlink(source, target)?;
        }
    }
    command("/bin/hostname", &["-F", "/etc/hostname"])?;
    let network = "/usr/libexec/agx-wired-network";
    let network = if std::path::Path::new(network).is_file() { network } else { "/etc/init.d/S40network" };
    // Keep the known board initialization and USB configuration. Long-running services are Rust-owned.
    for script in [
        "/etc/init.d/S01seedrng",
        "/etc/init.d/S02sysctl",
        "/etc/init.d/S10agx-hardware",
        "/etc/init.d/S11modules",
        network,
    ] {
        if script == "/etc/init.d/S10agx-hardware" && !std::path::Path::new(script).exists() {
            continue;
        }
        command(script, &["start"])?;
    }
    // Optional data storage must not prevent recovery if its drive is absent.
    if std::path::Path::new("/etc/init.d/S12agx-data").exists() {
        let _ = command("/etc/init.d/S12agx-data", &["start"]);
    }
    Ok(())
}
fn run(pid1: bool) -> io::Result<()> {
    if unsafe { libc::geteuid() } != 0 {
        return Err(io::Error::other("Service manager requires root"));
    }
    if pid1 {
        bootstrap()?;
    }
    unsafe {
        libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0);
    }
    fs::create_dir_all("/run/agx")?;
    let mut lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open("/run/agx/init.pid")?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(io::Error::other("Service manager already running"));
    }
    lock.set_len(0)?;
    writeln!(lock, "{}", std::process::id())?;
    match fs::remove_file(agx_init::SOCKET) {
        Ok(()) => (),
        Err(e) if e.kind() == io::ErrorKind::NotFound => (),
        Err(e) => return Err(e),
    }
    let listener = UnixListener::bind(agx_init::SOCKET)?;
    agx_profile::service::prepare_access(agx_init::SOCKET)?;
    listener.set_nonblocking(true)?;
    let stop = Arc::new(AtomicBool::new(false));
    let poweroff = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        signal_hook::flag::register(signal, Arc::clone(&stop))?;
    }
    for signal in [signal_hook::consts::SIGUSR1, signal_hook::consts::SIGUSR2] {
        signal_hook::flag::register(signal, Arc::clone(&poweroff))?;
    }
    let mut manager = Supervisor::new(definitions(pid1), pid1);
    let mut driver_setup = driver_setup::DriverSetup::new(pid1);
    manager.tick();
    if pid1 {
        let _ = command("/etc/init.d/S51agx-boot-metrics", &["start"]);
    }
    let boot_started = Instant::now();
    let ssh_address = ssh_listen_address(std::path::Path::new(SECURITY).is_file());
    let mut recovery_verified = false;
    loop {
        driver_setup.refresh();
        manager.tick_with_child(driver_setup.pid());
        if pid1 && !recovery_verified && boot_started.elapsed() > Duration::from_secs(15) {
            // The public SSH gate can be closed throughout first-run setup.
            // Check Dropbear's configured endpoint without opening a network link.
            if !boot_ssh::listening(ssh_address) {
                return Err(io::Error::other(format!(
                    "SSH recovery listener unavailable at {ssh_address}; falling back to BusyBox"
                )));
            }
            recovery_verified = true;
        }
        if stop.load(Ordering::Relaxed)
            || poweroff.load(Ordering::Relaxed)
            || manager.power.is_some_and(|(_, at)| Instant::now() >= at)
        {
            break;
        }
        match listener.accept() {
            Ok((mut socket, _)) => {
                // One bounded request per connection. A broken client cannot terminate PID 1.
                let result = (|| -> io::Result<()> {
                    socket.set_read_timeout(Some(Duration::from_millis(250)))?;
                    socket.set_write_timeout(Some(Duration::from_millis(250)))?;
                    let mut bytes = Vec::new();
                    BufReader::new(&mut socket)
                        .take(4097)
                        .read_until(b'\n', &mut bytes)?;
                    if bytes.len() > 4096 {
                        return Err(io::Error::other("Request too large"));
                    }
                    let request: Request = serde_json::from_slice(&bytes)?;
                    let preparation = match &request {
                        Request::DriverPreparation { start } => {
                            driver_setup.status(*start && manager.power.is_none())
                        }
                        _ => None,
                    };
                    let mut reply = manager.handle(request);
                    reply.driver_preparation = preparation;
                    serde_json::to_writer(&mut socket, &reply)?;
                    socket.write_all(b"\n")
                })();
                let _ = result;
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50))
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
            Err(error) => return Err(error),
        }
    }
    manager.stop_all();
    let deadline = Instant::now() + Duration::from_secs(7);
    while !manager.all_stopped() && Instant::now() < deadline {
        driver_setup.refresh();
        manager.tick_with_child(driver_setup.pid());
        std::thread::sleep(Duration::from_millis(50));
    }
    let reboot =
        !poweroff.load(Ordering::Relaxed) && manager.power.is_none_or(|(reboot, _)| reboot);
    let requested_power = manager.power.is_some();
    drop(manager);
    let _ = fs::remove_file(agx_init::SOCKET);
    if pid1 {
        unsafe {
            libc::sync();
        }
        let _ = command("/bin/umount", &["-a", "-r"]);
        unsafe {
            libc::sync();
            libc::reboot(if reboot {
                libc::RB_AUTOBOOT
            } else {
                libc::RB_POWER_OFF
            });
        }
        return Err(io::Error::last_os_error());
    }
    if requested_power {
        command(
            if reboot {
                "/sbin/reboot"
            } else {
                "/sbin/poweroff"
            },
            &[],
        )?;
    }
    Ok(())
}
fn main() {
    let pid1 = std::process::id() == 1;
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--check"] {
        for definition in definitions(true) {
            let valid = fs::metadata(&definition.program)
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
            if !valid {
                eprintln!("Missing service executable: {}", definition.program);
                std::process::exit(1);
            }
        }
        println!("AGX init service executables available");
        return;
    }
    if !pid1 && args != ["--supervise"] {
        eprintln!("usage: agx-init --check | --supervise (or run as PID 1)");
        std::process::exit(2);
    }
    if let Err(error) = run(pid1) {
        eprintln!("AGX init failed: {error}");
        if pid1 {
            let _ = fs::write("/run/agx-init-boot-error", format!("{error}\n"));
            eprintln!("Starting BusyBox recovery init");
            let error = Command::new("/bin/busybox").arg("init").exec();
            eprintln!("Recovery init failed: {error}");
            loop {
                std::thread::sleep(Duration::from_secs(60));
            }
        }
        std::process::exit(1);
    }
}
