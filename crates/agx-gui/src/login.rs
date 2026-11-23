//! Root greeter only. A successful login execs a fresh, unprivileged session.
mod setup;

use crate::{ComponentHandle, GreeterSetup, LoginWindow, SessionMode, graphics, remote};
use slint::{Timer, TimerMode};
use std::{
    cell::RefCell, ffi::CString, io, os::unix::process::CommandExt, process::Command, rc::Rc,
    time::Duration,
};
use zeroize::Zeroizing;

pub fn run(preview: bool) -> Result<(), Box<dyn std::error::Error>> {
    if !preview && unsafe { libc::geteuid() } != 0 {
        return Err("The login screen must be started by agx-init".into());
    }
    let ui = LoginWindow::new()?;
    graphics::attach_input(&ui);
    ui.set_draw_cursor(graphics::draw_cursor());
    let _setup = (!preview).then(|| setup::install(&ui));
    let _bluetooth = (!preview).then(|| crate::bluetooth::watch(&ui, || true));
    let remote = remote::start(&ui).map(Rc::new);
    let availability = Timer::default();
    update_modes(&ui, remote.as_deref());
    if cfg!(target_arch = "x86_64") && ui.get_wayland_unavailable().is_empty() {
        ui.set_session_mode(SessionMode::Wayland);
    }
    let weak = ui.as_weak();
    let viewers = remote.clone();
    availability.start(TimerMode::Repeated, Duration::from_millis(250), move || {
        if let Some(ui) = weak.upgrade() {
            update_modes(&ui, viewers.as_deref());
        }
    });
    let authenticated = Rc::new(RefCell::new(None));
    let result = authenticated.clone();
    let weak = ui.as_weak();
    ui.on_sign_in(move |username, password, mode| {
        let Some(ui) = weak.upgrade() else { return };
        if ui.get_busy() || ui.global::<GreeterSetup>().get_step() != 0 {
            return;
        }
        ui.set_busy(true);
        ui.set_error_text("".into());
        let username = username.to_string();
        let password = Zeroizing::new(password.to_string());
        let weak = ui.as_weak();
        // Hashing must not block rendering or keyboard/mouse input. Only the result
        // crosses back to the UI; never log the password or put it in argv/env.
        std::thread::spawn(move || {
            let ok = !preview && agx_profile::service::authenticate(&username, &password).is_ok();
            drop(password);
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak.upgrade() else { return };
                if ok {
                    // Store the verified username in a UI property-free callback.
                    ui.invoke_authenticated(username.into(), mode);
                } else {
                    ui.set_error_text(
                        "Username or password is incorrect, or the account is unavailable.".into(),
                    );
                    let weak = ui.as_weak();
                    Timer::single_shot(Duration::from_secs(2), move || {
                        if let Some(ui) = weak.upgrade() {
                            ui.set_busy(false);
                        }
                    });
                }
            });
        });
    });
    let viewers = remote.clone();
    let weak = ui.as_weak();
    ui.on_authenticated(move |username, mode| {
        let Some(ui) = weak.upgrade() else { return };
        if mode == SessionMode::Wayland {
            let reason = graphics::wayland_unavailable(
                viewers.as_deref().is_some_and(remote::Remote::has_viewers),
            )
            .or_else(|| {
                viewers
                    .as_deref()
                    .is_some_and(|remote| !remote.stop_accepting_if_idle())
                    .then_some(graphics::WAYLAND_VNC_UNAVAILABLE)
            });
            if let Some(reason) = reason {
                ui.set_busy(false);
                show_unavailable(&ui, reason);
                ui.invoke_show_login(username);
                return;
            }
        }
        *result.borrow_mut() = Some((username.to_string(), mode));
        let _ = slint::quit_event_loop();
    });
    ui.run()?;
    drop(ui);
    if let Some((username, mode)) = authenticated.borrow_mut().take() {
        // Viewers connected over VNC follow the sign-in into the desktop.
        let handoff = remote.as_deref().and_then(remote::Remote::handoff_for_exec);
        return Err(exec_session(&username, mode, handoff).into());
    }
    Ok(())
}

fn update_modes(ui: &LoginWindow, remote: Option<&remote::Remote>) {
    let reason = graphics::wayland_unavailable(remote.is_some_and(remote::Remote::has_viewers));
    if let Some(reason) = reason {
        show_unavailable(ui, reason);
    } else {
        ui.set_wayland_unavailable("".into());
    }
}

fn show_unavailable(ui: &LoginWindow, reason: &str) {
    ui.set_wayland_unavailable(reason.into());
    if !ui.get_busy() {
        ui.set_session_mode(SessionMode::Native);
    }
}

fn exec_session(username: &str, mode: SessionMode, handoff: Option<remote::Exec>) -> io::Error {
    let Ok(name) = CString::new(username) else {
        return io::Error::other("Invalid account");
    };
    // Copy libc's account data before replacing this process with the session.
    let entry = unsafe { libc::getpwnam(name.as_ptr()) };
    if entry.is_null() {
        return io::Error::other("Account disappeared");
    }
    let (uid, gid, home) = unsafe {
        (
            (*entry).pw_uid,
            (*entry).pw_gid,
            std::ffi::CStr::from_ptr((*entry).pw_dir)
                .to_string_lossy()
                .into_owned(),
        )
    };
    if uid < 1000 {
        return io::Error::other("An ANGI administrator account is required");
    }
    if !cfg!(feature = "headless")
        && let Err(error) = crate::browser::prepare_user_storage(uid, gid)
    {
        eprintln!("agx-gui: Web storage is unavailable: {error}");
    }
    let mode_arg = match mode {
        SessionMode::Native => "native",
        SessionMode::Wayland => "wayland",
    };
    let mut session = Command::new("/usr/bin/agx");
    session
        .args(["desktop", "--mode", mode_arg])
        .env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("HOME", &home)
        .env("USER", username)
        .env("LOGNAME", username)
        .env("SHELL", "/bin/sh")
        .env("TERM", "linux")
        .current_dir(&home);
    for key in graphics::ENVIRONMENT {
        if let Some(value) = std::env::var_os(key) {
            session.env(key, value);
        }
    }
    let keep_open = handoff
        .filter(|_| mode == SessionMode::Native)
        .map_or_else(Vec::new, |exec| {
            for (name, value) in &exec.env {
                session.env(name, value);
            }
            exec.fds
        });
    unsafe {
        session.pre_exec(move || {
            // The software framebuffer backend needs access to the active VT.
            if (!cfg!(feature = "headless") && libc::chown(c"/dev/tty1".as_ptr(), uid, gid) != 0)
                || libc::initgroups(name.as_ptr(), gid) != 0
                || libc::setgid(gid) != 0
                || libc::setuid(uid) != 0
            {
                return Err(io::Error::last_os_error());
            }
            // Do not leak the root greeter's display/input descriptors into the session.
            if libc::syscall(libc::SYS_close_range, 3u32, u32::MAX, 4u32) != 0 {
                return Err(io::Error::last_os_error());
            }
            // The VNC listener and viewers must survive into the desktop. Best effort:
            // a failure only means the viewer has to reconnect.
            for fd in &keep_open {
                libc::fcntl(*fd, libc::F_SETFD, 0);
            }
            Ok(())
        });
    }
    session.exec()
}

#[cfg(test)]
mod tests;
