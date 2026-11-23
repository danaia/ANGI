//! Restart the desktop after its windows and child sessions have been cleaned up.
use std::{
    io,
    os::unix::process::CommandExt,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};
static CLEANUPS: std::sync::Mutex<Vec<std::thread::JoinHandle<()>>> =
    std::sync::Mutex::new(Vec::new());

/// Normal window closes stay asynchronous; a desktop refresh waits for these jobs.
pub(crate) fn spawn_cleanup(job: impl FnOnce() + Send + 'static) {
    let handle = std::thread::spawn(job);
    let mut jobs = CLEANUPS.lock().unwrap_or_else(|p| p.into_inner());
    jobs.retain(|job| !job.is_finished());
    jobs.push(handle);
}

/// `angi-update refresh` looks for this text in the running desktop before signalling it: a desktop
/// without the SIGUSR1 handler below would be terminated by the signal instead of refreshed.
#[used]
static SIGNAL_MARKER: &str = "angi-refresh-on-sigusr1";

/// SIGUSR1 from the same account (the update script, after installing a release) refreshes the desktop
/// exactly as Settings does, saving settings first. Must run on the event loop's thread.
pub(super) fn request_from_signal() {
    match crate::settings::flush() {
        Ok(()) => request(),
        Err(error) => eprintln!("agx-gui: refresh by signal skipped, settings not saved: {error}"),
    }
}

static REQUESTED: AtomicBool = AtomicBool::new(false);

pub(super) fn request() {
    REQUESTED.store(true, Ordering::SeqCst);
    crate::quit(0);
}

pub(super) fn restart_if_requested() -> io::Result<()> {
    if !REQUESTED.swap(false, Ordering::SeqCst) {
        return Ok(());
    }
    // Resolve the installed entry point again, so a staged update takes effect.
    // exec retains the authenticated account and the supervisor's existing child PID.
    #[cfg(feature = "kms")]
    if std::env::var_os("HOME").is_some_and(|home| crate::graphics::prefer_wayland(home.as_ref())) {
        return replace_with(std::path::Path::new("/usr/bin/angi-wayland"), &["--login"]);
    }
    #[cfg(feature = "kms")]
    let executable = std::path::PathBuf::from(
        if cfg!(feature = "wayland") && crate::graphics::using_winit() {
            "/usr/libexec/agx-gui-wayland"
        } else {
            "/usr/bin/agx-gui"
        },
    );
    #[cfg(not(feature = "kms"))]
    let executable = std::env::current_exe()?;
    replace_with(&executable, &[])
}

fn replace_with(executable: &std::path::Path, args: &[&str]) -> io::Result<()> {
    let jobs = std::mem::take(&mut *CLEANUPS.lock().unwrap_or_else(|p| p.into_inner()));
    for job in jobs {
        let _ = job.join();
    }
    // Login handoff descriptors may lack CLOEXEC. Do not carry old display or
    // VNC sockets into the new desktop, or reuse stale handoff descriptor numbers.
    #[cfg(target_os = "linux")]
    if unsafe { libc::syscall(libc::SYS_close_range, 3u32, u32::MAX, 4u32) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Err(Command::new(executable)
        .args(args)
        .env_remove("AGX_GUI_VNC_HANDOFF")
        .env_remove("AGX_GUI_VNC_FDS")
        .exec())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};

    #[test]
    fn refresh_exec_waits_for_cleanup_preserves_identity_and_closes_old_fds() {
        const PROBE: &str = "AGX_REFRESH_TEST_SCRIPT";
        if let Some(script) = std::env::var_os(PROBE) {
            let marker = std::path::PathBuf::from(&script).with_extension("done");
            // This child has not started any application threads yet.
            unsafe {
                std::env::set_var("AGX_EXPECT_PID", std::process::id().to_string());
                std::env::set_var("AGX_EXPECT_UID", libc::geteuid().to_string());
            }
            let file = fs::File::open("/dev/null").unwrap();
            use std::os::fd::AsRawFd;
            assert_eq!(unsafe { libc::dup2(file.as_raw_fd(), 99) }, 99);
            spawn_cleanup(move || {
                std::thread::sleep(std::time::Duration::from_millis(50));
                fs::write(marker, b"done").unwrap();
            });
            panic!(
                "exec failed: {:?}",
                replace_with(std::path::Path::new(&script), &[])
            );
        }
        let directory =
            std::env::temp_dir().join(format!("agx-refresh-test-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let script = directory.join("desktop");
        fs::write(
            &script,
            r#"#!/bin/sh
set -eu
test "$$" = "$AGX_EXPECT_PID"
test "$(id -u)" = "$AGX_EXPECT_UID"
test -f "$0.done"
test ! -e /proc/$$/fd/99
test -z "${AGX_GUI_VNC_HANDOFF+x}"
test -z "${AGX_GUI_VNC_FDS+x}"
"#,
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        let result = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "session_refresh::tests::refresh_exec_waits_for_cleanup_preserves_identity_and_closes_old_fds", "--nocapture"])
            .env(PROBE, &script)
            .env("AGX_GUI_VNC_HANDOFF", "old-session")
            .env("AGX_GUI_VNC_FDS", "99")
            .output().unwrap();
        fs::remove_dir_all(directory).unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}
