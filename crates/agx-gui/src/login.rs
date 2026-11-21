//! Root greeter only. A successful login execs a fresh, unprivileged session.
mod setup;

use crate::{ComponentHandle, GreeterSetup, LoginWindow, graphics, remote};
use slint::Timer;
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
    let remote = remote::start(&ui);
    let authenticated = Rc::new(RefCell::new(None::<String>));
    let result = authenticated.clone();
    let weak = ui.as_weak();
    ui.on_sign_in(move |username, password| {
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
                    ui.invoke_authenticated(username.into());
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
    ui.on_authenticated(move |username| {
        *result.borrow_mut() = Some(username.to_string());
        let _ = slint::quit_event_loop();
    });
    ui.run()?;
    drop(ui);
    if let Some(username) = authenticated.borrow_mut().take() {
        // Viewers connected over VNC follow the sign-in into the desktop.
        let handoff = remote.as_ref().and_then(remote::Remote::handoff_for_exec);
        return Err(exec_session(&username, handoff).into());
    }
    Ok(())
}

fn exec_session(username: &str, handoff: Option<remote::Exec>) -> io::Error {
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
    let mut session = Command::new("/usr/bin/agx");
    session
        .arg("desktop")
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
    let keep_open = handoff.map_or_else(Vec::new, |exec| {
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
