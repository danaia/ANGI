mod bluetooth;
mod start;
use agx_core::Sampler;
use agx_tui::{ColorMode, Options};
use clap::{Parser, Subcommand, ValueEnum};
use std::{
    io::{self, Write},
    process::ExitCode,
    time::Duration,
};
#[derive(Parser)]
#[command(
    name = "agx",
    version,
    about = "AGX headless system administration",
    arg_required_else_help = true,
    after_help = "TUI: 1 Profile, 2 Overview, 3 Hardware, 4 Network, 5 System, 6 Services, 7 Logs, ? Help.\nTab switches screens; arrows and mouse scroll; F8 returns to GUI on HDMI; F9 shuts down; F10 reboots; q quits.\nPower actions require confirmation. Service controls require AGX administrator access. NO_COLOR is respected."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    /// Open the desktop directly
    Desktop {
        /// Graphics session to start
        #[arg(long, value_enum)]
        mode: Option<start::DesktopMode>,
    },
    /// Ask whether to open the desktop (GUI) or the console (TUI)
    Start {
        /// Interface used on Enter or timeout
        #[arg(long, value_enum, default_value = "tui")]
        default: StartDefault,
        /// Seconds to wait before using the default
        #[arg(long, default_value_t = 10)]
        timeout: u64,
    },
    /// Open the interactive console (use ssh -t)
    Tui {
        #[arg(long, hide = true)]
        embedded: bool,
        #[arg(long)]
        ascii: bool,
        #[arg(long)]
        no_color: bool,
        #[arg(long)]
        no_mouse: bool,
        #[arg(long, value_enum, default_value = "auto")]
        color: Color,
    },
    /// Sample system and hardware information
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Inspect Wi-Fi or request scanning and connectivity diagnostics
    Wifi {
        #[command(subcommand)]
        action: Option<WifiCommand>,
        #[arg(long, global = true)]
        json: bool,
    },
    /// Find, pair and connect Bluetooth devices
    Bluetooth {
        #[command(subcommand)]
        action: Option<bluetooth::Action>,
        #[arg(long, global = true)]
        json: bool,
    },
    /// Inspect the configured administrator profile
    Profile {
        #[arg(long)]
        json: bool,
    },
    /// Inspect and control supervised services
    Services {
        #[arg(value_enum, default_value = "status")]
        action: ServiceAction,
        name: Option<String>,
        /// Confirm a stop or restart, including in scripts
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
    /// Read the bounded lifecycle journal
    Logs {
        #[arg(long)]
        service: Option<String>,
        #[arg(long, default_value_t = 0)]
        after: u64,
        #[arg(long)]
        follow: bool,
        #[arg(long)]
        json: bool,
    },
    /// Reboot after confirmation (scripts must supply --yes)
    Reboot {
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
    /// Power off after confirmation (scripts must supply --yes)
    Shutdown {
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
}
#[derive(Clone, Copy, ValueEnum)]
enum Color {
    Auto,
    Truecolor,
    #[value(name = "256")]
    Indexed,
    #[value(name = "16")]
    Basic,
    Never,
}
#[derive(Subcommand)]
enum WifiCommand {
    Status,
    Scan,
    Check,
    Saved,
    Select {
        id: i32,
    },
    Forget {
        id: i32,
        #[arg(long)]
        yes: bool,
    },
    Disconnect {
        #[arg(long)]
        yes: bool,
    },
    Country {
        code: String,
    },
    Connect {
        ssid: String,
        #[arg(long, value_enum, default_value = "wpa2")]
        security: WifiSecurity,
        #[arg(long)]
        hidden: bool,
        /// Read a password from stdin; otherwise prompt with echo disabled
        #[arg(long)]
        password_stdin: bool,
    },
}
#[derive(Clone, Copy, ValueEnum)]
enum WifiSecurity {
    Wpa2,
    Wpa3,
    Owe,
    Open,
}
#[derive(Clone, Copy, ValueEnum)]
enum ServiceAction {
    Status,
    Start,
    Stop,
    Restart,
}
fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("agx: {error}");
            ExitCode::FAILURE
        }
    }
}
fn confirm(yes: bool, json: bool, question: &str) -> io::Result<()> {
    use std::io::IsTerminal;
    if yes {
        return Ok(());
    }
    if json || !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return Err(io::Error::other("Confirmation required; supply --yes"));
    }
    eprint!("{question} [y/N] ");
    io::stderr().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    if matches!(answer.trim(), "y" | "Y" | "yes") {
        Ok(())
    } else {
        Err(io::Error::other("Cancelled"))
    }
}
fn read_password(from_stdin: bool) -> io::Result<zeroize::Zeroizing<String>> {
    use std::io::IsTerminal;
    struct EchoGuard(Option<libc::termios>);
    impl Drop for EchoGuard {
        fn drop(&mut self) {
            if let Some(before) = self.0 {
                unsafe {
                    libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &before);
                }
            }
        }
    }
    let interactive = io::stdin().is_terminal();
    if !from_stdin && !interactive {
        return Err(io::Error::other(
            "Use --password-stdin or an interactive password prompt",
        ));
    }
    // Catch termination while echo is disabled so every exit restores the terminal.
    let interrupted = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    struct SignalGuard(Vec<signal_hook::SigId>);
    impl Drop for SignalGuard {
        fn drop(&mut self) {
            for id in self.0.drain(..) {
                signal_hook::low_level::unregister(id);
            }
        }
    }
    let mut signals = SignalGuard(Vec::new());
    for signal in [
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGHUP,
    ] {
        signals.0.push(signal_hook::flag::register(
            signal,
            std::sync::Arc::clone(&interrupted),
        )?);
    }
    let mut guard = EchoGuard(None);
    if interactive {
        let mut before = std::mem::MaybeUninit::<libc::termios>::uninit();
        if unsafe { libc::tcgetattr(libc::STDIN_FILENO, before.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let before = unsafe { before.assume_init() };
        let mut masked = before;
        masked.c_lflag &= !libc::ECHO;
        if unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &masked) } != 0 {
            return Err(io::Error::last_os_error());
        }
        guard.0 = Some(before);
        eprint!("Wi-Fi password: ");
        io::stderr().flush()?;
    }
    let mut bytes = zeroize::Zeroizing::new(Vec::new());
    let mut byte = zeroize::Zeroizing::new([0u8; 1]);
    loop {
        if interrupted.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Password input cancelled",
            ));
        }
        let mut fd = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut fd, 1, 100) };
        if ready < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if ready == 0 {
            continue;
        }
        let count = unsafe { libc::read(libc::STDIN_FILENO, byte.as_mut_ptr().cast(), 1) };
        if count < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if count == 0 {
            break;
        }
        bytes.push(byte[0]);
        if byte[0] == b'\n' || bytes.len() > 256 {
            break;
        }
    }
    drop(guard);
    if interactive {
        eprintln!();
    }
    if bytes.len() > 256 {
        return Err(io::Error::other("Password input too long"));
    }
    while bytes.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
        bytes.pop();
    }
    let text =
        std::str::from_utf8(&bytes).map_err(|_| io::Error::other("Password must be UTF-8"))?;
    Ok(zeroize::Zeroizing::new(text.to_owned()))
}
fn print_reply(reply: &agx_init::Reply, json: bool, services: bool) -> io::Result<()> {
    if json {
        println!("{}", serde_json::to_string(reply)?);
    } else if services {
        println!("SERVICE          STATE      PID      STARTS");
        for service in &reply.services {
            println!(
                "{:<16} {:<10} {:<8} {}",
                service.name,
                service.state,
                service.pid.map_or("-".into(), |pid| pid.to_string()),
                service.starts
            );
        }
    } else {
        println!("{}", reply.message);
    }
    if !reply.ok {
        return Err(io::Error::other(reply.message.clone()));
    }
    Ok(())
}
#[derive(Clone, Copy, ValueEnum)]
enum StartDefault {
    Gui,
    Tui,
}
fn run_tui() -> io::Result<()> {
    start::run_interfaces(start::Choice::Tui, Options::default(), None)
}
fn run(command: Commands) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Commands::Desktop { mode } => {
            start::run_interfaces(start::Choice::Gui, Options::default(), mode)?;
        }
        Commands::Start { default, timeout } => {
            use std::io::IsTerminal;
            if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
                return Err("agx start needs an interactive terminal".into());
            }
            if !start::gui_installed() {
                // Nothing to choose between yet; keep the existing console flow.
                return Ok(run_tui()?);
            }
            let default = match default {
                StartDefault::Gui => start::Choice::Gui,
                StartDefault::Tui => start::Choice::Tui,
            };
            let choice = start::ask(default, Duration::from_secs(timeout))?;
            if choice == start::Choice::Gui {
                if let Err(error) =
                    start::run_interfaces(start::Choice::Gui, Options::default(), None)
                {
                    eprintln!("Could not run the desktop ({error}); opening the console.");
                    run_tui()?;
                }
            } else {
                run_tui()?;
            }
        }
        Commands::Tui {
            embedded,
            ascii,
            no_color,
            no_mouse,
            color,
        } => {
            let color = if no_color {
                ColorMode::Never
            } else {
                match color {
                    Color::Auto => ColorMode::Auto,
                    Color::Truecolor => ColorMode::TrueColor,
                    Color::Indexed => ColorMode::Indexed,
                    Color::Basic => ColorMode::Basic,
                    Color::Never => ColorMode::Never,
                }
            };
            let options = Options {
                ascii,
                mouse: !no_mouse,
                color,
            };
            if embedded {
                agx_tui::run(options)?;
            } else {
                start::run_interfaces(start::Choice::Tui, options, None)?;
            }
        }
        Commands::Status { json } => {
            let mut sampler = Sampler::default();
            sampler.collect()?;
            std::thread::sleep(Duration::from_millis(200));
            let snapshot = sampler.collect()?;
            if json {
                println!("{}", serde_json::to_string(&snapshot)?);
            } else {
                println!("{} / {}", snapshot.hostname, snapshot.board);
                println!(
                    "Kernel: {} | Uptime: {}s | CPUs: {}",
                    snapshot.kernel, snapshot.uptime_secs, snapshot.cpu_count
                );
                println!(
                    "Memory available: {} / {} MiB",
                    snapshot.memory.available_bytes / 1048576,
                    snapshot.memory.total_bytes / 1048576
                );
                for network in snapshot.networks {
                    println!(
                        "{}: {} [{}]",
                        network.name,
                        network.ipv4.join(", "),
                        network.state
                    );
                }
                for warning in snapshot.warnings {
                    eprintln!("warning: {warning}");
                }
            }
        }
        Commands::Wifi { action, json } => {
            let mut saved_only = false;
            let command = match action.unwrap_or(WifiCommand::Status) {
                WifiCommand::Status => agx_net::Request::Status,
                WifiCommand::Scan => agx_net::Request::Scan,
                WifiCommand::Check => agx_net::Request::Check,
                WifiCommand::Saved => {
                    saved_only = true;
                    agx_net::Request::Status
                }
                WifiCommand::Select { id } => agx_net::Request::SelectSaved { id },
                WifiCommand::Forget { id, yes } => {
                    confirm(yes, json, "Forget this saved Wi-Fi network?")?;
                    agx_net::Request::Forget { id }
                }
                WifiCommand::Disconnect { yes } => {
                    confirm(yes, json, "Disconnect Wi-Fi? USB SSH remains available.")?;
                    agx_net::Request::Disconnect
                }
                WifiCommand::Country { code } => agx_net::Request::Country { country: code },
                WifiCommand::Connect {
                    ssid,
                    security,
                    hidden,
                    password_stdin,
                } => {
                    let security = match security {
                        WifiSecurity::Wpa2 => agx_net::Security::Wpa2,
                        WifiSecurity::Wpa3 => agx_net::Security::Wpa3,
                        WifiSecurity::Owe => agx_net::Security::Owe,
                        WifiSecurity::Open => agx_net::Security::Open,
                    };
                    let password =
                        if matches!(security, agx_net::Security::Wpa2 | agx_net::Security::Wpa3) {
                            read_password(password_stdin)?
                        } else {
                            zeroize::Zeroizing::new(String::new())
                        };
                    let ssid_hex = agx_net::hex(ssid.as_bytes());
                    agx_net::validate_credentials(&ssid_hex, security, &password)?;
                    agx_net::Request::Connect {
                        ssid_hex,
                        security,
                        password,
                        hidden,
                    }
                }
            };
            let reply = agx_net::request(&command)?;
            if json {
                if saved_only {
                    println!("{}", serde_json::to_string(&reply.status.saved)?);
                } else {
                    println!("{}", serde_json::to_string(&reply.status)?);
                }
            } else if saved_only {
                for network in &reply.status.saved {
                    println!(
                        "{} {}{}",
                        network.id,
                        network.ssid,
                        if network.current { " (current)" } else { "" }
                    );
                }
            } else {
                println!("{}: {}", reply.status.phase, reply.message);
            }
            if !reply.ok {
                return Err(reply.message.into());
            }
        }
        Commands::Bluetooth { action, json } => bluetooth::run(action, json)?,
        Commands::Profile { json } => {
            let reply = agx_net::request(&agx_net::Request::ProfileStatus)?;
            if !reply.ok {
                return Err(reply.message.into());
            }
            let profile = reply.profile.ok_or("Profile service unavailable")?;
            if json {
                println!("{}", serde_json::to_string(&profile)?);
            } else {
                println!("{}", serde_json::to_string_pretty(&profile)?);
            }
        }
        Commands::Services {
            action,
            name,
            yes,
            json,
        } => {
            let command = match action {
                ServiceAction::Status => {
                    if name.is_some() || yes {
                        return Err("status takes no service name or --yes".into());
                    }
                    agx_init::Request::Status
                }
                ServiceAction::Start | ServiceAction::Stop | ServiceAction::Restart => {
                    let name = name.ok_or("Specify a service name")?;
                    let action = match action {
                        ServiceAction::Start => agx_init::Action::Start,
                        ServiceAction::Stop => agx_init::Action::Stop,
                        _ => agx_init::Action::Restart,
                    };
                    if !matches!(action, agx_init::Action::Start) {
                        confirm(
                            yes,
                            json,
                            &format!(
                                "{action:?} {name}? Wi-Fi may disconnect; USB SSH remains available."
                            ),
                        )?;
                    }
                    agx_init::Request::Control {
                        name,
                        action,
                        confirmed: true,
                    }
                }
            };
            print_reply(&agx_init::request(&command)?, json, true)?;
        }
        Commands::Logs {
            service,
            mut after,
            follow,
            json,
        } => loop {
            let reply = agx_init::request(&agx_init::Request::Logs {
                service: service.clone(),
                after,
            })?;
            if !reply.ok {
                return Err(reply.message.into());
            }
            if reply.last_sequence < after {
                after = 0;
                continue;
            }
            after = reply.last_sequence;
            if json {
                println!("{}", serde_json::to_string(&reply)?);
            } else {
                if reply.logs_truncated {
                    eprintln!("Older events expired from the 512-event journal");
                }
                for entry in reply.logs {
                    println!(
                        "{} {}: {}",
                        entry.unix_seconds, entry.service, entry.message
                    );
                }
            }
            if !follow {
                break;
            }
            std::thread::sleep(Duration::from_secs(1));
        },
        Commands::Reboot { yes, json } => {
            confirm(yes, json, "Reboot this Pi? SSH sessions will close.")?;
            print_reply(
                &agx_init::request(&agx_init::Request::Reboot { confirmed: true })?,
                json,
                false,
            )?;
        }
        Commands::Shutdown { yes, json } => {
            confirm(yes, json, "Power off this Pi? SSH sessions will close.")?;
            print_reply(
                &agx_init::request(&agx_init::Request::Shutdown { confirmed: true })?,
                json,
                false,
            )?;
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scripts_cannot_implicitly_confirm_disruption() {
        assert!(confirm(false, true, "Reboot?").is_err());
        assert!(confirm(true, true, "Reboot?").is_ok());
        assert!(
            Cli::try_parse_from(["agx", "services", "restart", "network", "--yes", "--json"])
                .is_ok()
        );
        assert!(Cli::try_parse_from(["agx", "reboot", "--unknown"]).is_err());
        assert!(Cli::try_parse_from(["agx", "wifi", "status", "--json"]).is_ok());
    }
}
