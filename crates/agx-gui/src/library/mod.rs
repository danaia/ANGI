//! The app library's live state: what is installed, what is updating, and the worker that runs installs.
//!
//! Installation itself is `angi-update app install ID --progress` (see `apps/agent/app_install.py`), run
//! on a worker thread so the desktop never waits. Its JSON events land in `State`; the launcher and the
//! toast timer read it. `worker.rs` owns the processes and `tests.rs` the state transitions.
mod worker;
#[cfg(test)]
mod tests;

use crate::{App, AppWindow};
use slint::{ComponentHandle, Timer, TimerMode};
use std::{
    cell::RefCell,
    collections::HashMap,
    path::PathBuf,
    rc::Rc,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

/// How long an answer from GitHub is trusted before opening the launcher asks again.
const CHECK_EVERY: Duration = Duration::from_secs(10 * 60);

/// A job for one app that has not finished yet.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Job {
    Queued { removing: bool },
    Working { fraction: f32, message: String, removing: bool },
    Failed(String),
}

#[derive(Default)]
pub(crate) struct State {
    /// App id to installed release tag.
    pub installed: HashMap<String, String>,
    pub unavailable: HashMap<String, String>,
    pub catalogue_checked: bool,
    /// App id to the newest release tag seen by the last update check ("" when unknown).
    pub latest: HashMap<String, String>,
    pub jobs: HashMap<String, Job>,
    /// Outcomes the desktop has not yet shown as a toast.
    pub notices: Vec<String>,
    /// Bumped on every change so the UI redraws only when something happened.
    pub generation: u64,
    /// When the launcher last asked GitHub about installed apps.
    pub checked: Option<Instant>,
}

/// What the launcher shows for one catalogue entry.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Availability {
    Available,
    Unavailable(String),
    Queued { removing: bool },
    Working { fraction: f32, message: String, removing: bool },
    Failed(String),
    Installed { tag: String, update: Option<String> },
}

pub(crate) enum Request {
    /// Re-read what is installed; `check` also asks GitHub when the cached answer is stale.
    Refresh { check: bool },
    Install(String),
    Remove(String),
}

#[derive(Clone)]
pub(crate) struct Library {
    state: Arc<Mutex<State>>,
    requests: mpsc::Sender<Request>,
}

impl State {
    pub(crate) fn availability(&self, id: &str) -> Availability {
        match self.jobs.get(id) {
            Some(Job::Queued { removing }) => return Availability::Queued { removing: *removing },
            Some(Job::Working { fraction, message, removing }) => {
                return Availability::Working {
                    fraction: *fraction,
                    message: message.clone(),
                    removing: *removing,
                };
            }
            Some(Job::Failed(error)) => return Availability::Failed(error.clone()),
            None => {}
        }
        match self.installed.get(id) {
            Some(tag) => {
                let update = self
                    .latest
                    .get(id)
                    .filter(|latest| !latest.is_empty() && *latest != tag)
                    .cloned();
                Availability::Installed { tag: tag.clone(), update }
            }
            None => Availability::Available,
        }
    }

    pub(crate) fn changed(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }
}

impl Library {
    /// Starts the worker and the first read of what is installed.
    pub(crate) fn start() -> Self {
        let state = Arc::new(Mutex::new(State::default()));
        let (requests, receiver) = mpsc::channel();
        let shared = state.clone();
        std::thread::spawn(move || worker::run(&shared, receiver));
        let library = Self { state, requests };
        library.refresh(false);
        library
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn catalogue_availability(&self, id: &str, native: bool) -> Availability {
        let state = self.lock();
        if let Some(reason) = state.unavailable.get(id) {
            return Availability::Unavailable(reason.clone());
        }
        if !state.catalogue_checked {
            if std::env::var_os("AGX_UPDATE_PROGRAM").is_none()
                && !std::path::Path::new("/usr/bin/angi-update").is_file()
            {
                return Availability::Unavailable("This image lacks the ANGI app installer.".into());
            }
            if native {
                return Availability::Unavailable("Waiting for compatibility information from the ANGI app installer.".into());
            }
        }
        if !native && std::env::consts::ARCH != "aarch64" {
            return Availability::Unavailable("This release is built for 64-bit ARM Linux.".into());
        }
        state.availability(id)
    }

    #[cfg(test)]
    pub(crate) fn availability(&self, id: &str) -> Availability {
        self.lock().availability(id)
    }

    pub(crate) fn generation(&self) -> u64 {
        self.lock().generation
    }

    pub(crate) fn take_notices(&self) -> Vec<String> {
        std::mem::take(&mut self.lock().notices)
    }

    /// Asks GitHub about newer releases of installed apps, at most every ten minutes.
    pub(crate) fn check_if_stale(&self) {
        let mut state = self.lock();
        if state.checked.is_some_and(|at| at.elapsed() < CHECK_EVERY) {
            return;
        }
        state.checked = Some(Instant::now());
        drop(state);
        self.refresh(true);
    }

    pub(crate) fn refresh(&self, check: bool) {
        let _ = self.requests.send(Request::Refresh { check });
    }

    /// Queues an install or update. False when that app already has a job running.
    pub(crate) fn install(&self, id: &str) -> bool {
        self.enqueue(id, false, Request::Install(id.into()))
    }

    pub(crate) fn remove(&self, id: &str) -> bool {
        self.enqueue(id, true, Request::Remove(id.into()))
    }

    fn enqueue(&self, id: &str, removing: bool, request: Request) -> bool {
        let mut state = self.lock();
        if matches!(state.jobs.get(id), Some(Job::Queued { .. } | Job::Working { .. })) {
            return false;
        }
        state.jobs.insert(id.into(), Job::Queued { removing });
        state.changed();
        drop(state);
        self.requests.send(request).is_ok()
    }
}

/// The apps `bin/` directory of this account. The native terminal puts it first on PATH.
pub(crate) fn bin_directory() -> PathBuf {
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("/var/lib/agx-data/agx-opencode-users/{uid}/apps/bin"))
}

/// `PATH` for terminal sessions: installed apps first, so a program is found the moment it is installed.
pub(crate) fn terminal_path() -> String {
    let current = std::env::var("PATH").unwrap_or_else(|_| "/usr/sbin:/usr/bin:/sbin:/bin".into());
    format!("{}:{current}", bin_directory().display())
}

/// Turns library changes into toasts and live launcher rows. Runs on the UI thread.
pub(crate) fn install(ui: &AppWindow, app: &Rc<RefCell<App>>) -> Timer {
    let (weak, state) = (ui.as_weak(), app.clone());
    let mut seen = 0;
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(200), move || {
        let (Some(ui), Ok(mut app)) = (weak.upgrade(), state.try_borrow_mut()) else {
            return;
        };
        let generation = app.library.generation();
        if generation == seen {
            return;
        }
        seen = generation;
        if let Some(notice) = app.library.take_notices().pop() {
            app.toast(&ui, &notice);
        }
        if ui.get_menu_open() {
            app.refresh_menu(&ui);
        }
    });
    timer
}
