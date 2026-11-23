//! Installable packages in the launcher: how each state looks, and what choosing a row does.
//!
//! Choosing an app that is not installed installs it in place; the launcher stays open, the row shows
//! live progress, and the program is on the terminal's PATH the moment it finishes. Choosing an installed
//! app opens it in Wayland or a terminal; choosing one with an update waiting installs the update.
use super::{APP_ICON, APPS, INSTALL, Entry, Status, apps, apps::Package};
use crate::{App, AppWindow, keymap::Cmd, library::Availability};
use std::process::{Command, Stdio};

/// `MenuRow.state` values for package rows; 0 is every other row.
const AVAILABLE: i32 = 1;
const QUEUED: i32 = 2;
const WORKING: i32 = 3;
const INSTALLED: i32 = 4;
const UPDATE: i32 = 5;
const FAILED: i32 = 6;
const UNAVAILABLE: i32 = 7;

pub(super) fn package(index: usize) -> Option<&'static Package> {
    apps::index().get(index)
}

pub(super) fn entry(index: usize, package: &'static Package, availability: Availability) -> Entry {
    let summary = package.summary.clone();
    let (state, hint, description, progress, action, keeps_open, removable, running) = match availability {
        Availability::Available => (AVAILABLE, if package.native { "Buildroot".into() } else { package.kind.clone() }, summary, 0.0, "Install", true, false, false),
        Availability::Unavailable(reason) => (UNAVAILABLE, "Unavailable".into(), reason, 0.0, "", true, false, false),
        Availability::Queued { removing } => (
            QUEUED,
            if removing { "Removing…" } else { "Waiting…" }.into(),
            summary,
            0.0,
            "",
            true,
            false,
            false,
        ),
        Availability::Working { fraction, message, removing } => (
            WORKING,
            if removing { "Removing…".into() } else { format!("{}%", (fraction * 100.0).round() as u32) },
            message,
            fraction,
            "",
            true,
            false,
            false,
        ),
        Availability::Failed(error) => (FAILED, "Retry".into(), error, 0.0, "Retry", true, false, false),
        Availability::Installed { tag, update: None } => {
            (INSTALLED, format!("Installed · {tag}"), summary, 0.0, "Open", false, true, true)
        }
        Availability::Installed { tag, update: Some(latest) } => (
            UPDATE,
            "Update available".into(),
            format!("{tag} → {latest}  ·  {summary}"),
            0.0,
            "Update",
            true,
            true,
            true,
        ),
    };
    Entry {
        label: package.name.clone(),
        description,
        hint,
        aliases: &package.aliases,
        icon: APP_ICON,
        section: if running { APPS } else { INSTALL },
        running,
        command: Cmd::Package(index),
        status: Status { state, progress, action: action.into(), keeps_open, removable },
        badge: if package.recommended { "Recommended" } else { "" },
        heading: String::new(),
    }
}

/// An app that is installed (up to date or with an update waiting).
pub(super) fn is_installed(entry: &Entry) -> bool {
    matches!(entry.status.state, INSTALLED | UPDATE)
}

/// The four requested defaults stay above other compatible software, in catalog order.
pub(super) fn arrange_install(entries: &mut [Entry]) {
    if let Some(first) = entries.iter_mut().find(|entry| {
        matches!(entry.command, Cmd::Package(index) if package(index).is_some_and(|app| app.native))
    }) {
        first.heading = "FEATURED".into();
    }
    if let Some(first) = entries.iter_mut().find(|entry| {
        matches!(entry.command, Cmd::Package(index) if package(index).is_some_and(|app| !app.native))
    }) {
        first.heading = "MORE SOFTWARE".into();
    }
}

impl App {
    /// Re-renders the open launcher with the current query. The selection follows the same entry even when
    /// it moves, as an app does from Available to Installed when its install finishes.
    pub(crate) fn refresh_menu(&mut self, ui: &AppWindow) {
        let selected = usize::try_from(ui.get_menu_index()).ok().and_then(|i| self.menu.get(i)).copied();
        let query = self.menu_query.clone();
        self.set_menu_rows(ui, &query);
        if let Some(index) = selected.and_then(|cmd| self.menu.iter().position(|other| *other == cmd)) {
            if index as i32 != ui.get_menu_index() {
                ui.set_menu_index(index as i32);
            }
        }
    }

    pub(crate) fn package_activate(&mut self, ui: &AppWindow, index: usize) {
        let Some(package) = package(index) else {
            return;
        };
        match self.library.catalogue_availability(&package.id, package.native) {
            Availability::Installed { update: None, .. } => self.package_open(ui, package),
            Availability::Unavailable(_) | Availability::Queued { .. } | Availability::Working { .. } => {}
            Availability::Available | Availability::Failed(_) | Availability::Installed { .. } => {
                self.library.install(&package.id);
                self.refresh_menu(ui);
            }
        }
    }

    /// Opens desktop apps as Wayland clients and terminal tools in ANGI Terminal.
    fn package_open(&mut self, ui: &AppWindow, package: &Package) {
        self.desk.goto(self.menu_workspace);
        if package.kind == "Desktop" {
            if !std::env::var_os("WAYLAND_DISPLAY").is_some_and(|display| !display.is_empty()) {
                self.toast(ui, &format!("Open {} from an ANGI Wayland session.", package.name));
                return;
            }
            let program = crate::library::bin_directory().join(&package.launch);
            match Command::new(program).stdin(Stdio::null()).spawn() {
                Ok(mut child) => {
                    std::thread::spawn(move || {
                        let _ = child.wait();
                    });
                }
                Err(error) => self.toast(ui, &format!("Could not open {}. {error}", package.name)),
            }
            return;
        }
        if package.launch.is_empty() {
            self.run(ui, Cmd::Open(crate::wm::Kind::Terminal));
            let command = package.commands.first().map(String::as_str).unwrap_or(&package.name);
            self.toast(ui, &format!("{} is installed. Type {command} to run it.", package.name));
            return;
        }
        let path = crate::library::bin_directory().join(&package.launch);
        let path = path.to_string_lossy().into_owned();
        self.terminal_open_program(ui, path, package.launch.clone());
    }

    /// Removes the installed app on launcher row `row`. Its settings and data are left alone.
    pub(crate) fn menu_remove(&mut self, ui: &AppWindow, row: i32) {
        if let Some(Cmd::Package(index)) = self.menu.get(row as usize).copied() {
            self.package_remove(ui, index);
        }
    }

    fn package_remove(&mut self, ui: &AppWindow, index: usize) {
        if let Some(package) = package(index) {
            if matches!(self.library.catalogue_availability(&package.id, package.native), Availability::Installed { .. }) {
                self.library.remove(&package.id);
                self.refresh_menu(ui);
            }
        }
    }
}
