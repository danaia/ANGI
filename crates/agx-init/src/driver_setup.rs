//! Run the fixed driver installer for callers of the existing administrator socket.
use agx_init::DriverPreparation;
use std::{
    io::{self, Read},
    os::fd::AsRawFd,
    path::Path,
    process::{Child, Command, Stdio},
};

const GUI: &str = "/usr/bin/agx-gui";

pub(crate) struct DriverSetup {
    outcome: Option<DriverPreparation>,
    child: Option<Child>,
}

impl DriverSetup {
    pub(crate) fn new(pid1: bool) -> Self {
        Self {
            outcome: (pid1 && Path::new(GUI).is_file()).then_some(DriverPreparation::Available),
            child: None,
        }
    }

    pub(crate) fn pid(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id)
    }

    pub(crate) fn refresh(&mut self) {
        let Some(child) = &mut self.child else { return };
        let result = match child.try_wait() {
            Ok(None) => return,
            Ok(Some(exit)) if exit.success() => read_outcome(child),
            Ok(Some(exit)) => Err(io::Error::other(format!("driver installer exited: {exit}"))),
            Err(error) => Err(error),
        };
        self.outcome = Some(result.unwrap_or_else(|error| {
            eprintln!("agx-init: driver installer: {error}");
            DriverPreparation::Incomplete(
                "The driver installer stopped before it finished. Try again.".into(),
            )
        }));
        self.child = None;
    }

    pub(crate) fn status(&mut self, start: bool) -> Option<DriverPreparation> {
        self.outcome.as_ref()?;
        if start && self.child.is_none() {
            match Command::new(GUI)
                .arg("--prepare-drivers")
                .env_clear()
                .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
                .env("LANG", "C.UTF-8")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .spawn()
            {
                Ok(child) => {
                    self.child = Some(child);
                    self.outcome = Some(DriverPreparation::Working);
                }
                Err(error) => {
                    eprintln!("agx-init: could not start driver installer: {error}");
                    self.outcome = Some(DriverPreparation::Incomplete(
                        "The driver installer could not start.".into(),
                    ));
                }
            }
        }
        self.outcome.clone()
    }
}

fn read_outcome(child: &mut Child) -> io::Result<DriverPreparation> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("missing installer output"))?;
    // Even a descendant retaining the pipe cannot block PID 1.
    if unsafe { libc::fcntl(stdout.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut bytes = Vec::new();
    stdout.take(4097).read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        return Err(io::Error::other("driver installer output is too large"));
    }
    Ok(serde_json::from_slice(&bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agx_init::supervisor::Supervisor;
    use std::time::{Duration, Instant};

    #[test]
    fn service_reaper_leaves_the_driver_child_result_for_its_owner() {
        let child = Command::new("/bin/sh")
            .args(["-c", "printf '\"RestartRequired\"'"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let proc_stat = format!("/proc/{}/stat", child.id());
        let deadline = Instant::now() + Duration::from_secs(2);
        while !std::fs::read_to_string(&proc_stat)
            .unwrap()
            .rsplit_once(") ")
            .is_some_and(|(_, stat)| stat.starts_with("Z "))
        {
            assert!(Instant::now() < deadline, "driver child did not exit");
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut driver_setup = DriverSetup {
            outcome: Some(DriverPreparation::Working),
            child: Some(child),
        };
        Supervisor::new(Vec::new(), false).tick_with_child(driver_setup.pid());
        driver_setup.refresh();
        assert!(matches!(
            driver_setup.status(false),
            Some(DriverPreparation::RestartRequired)
        ));
    }
}
