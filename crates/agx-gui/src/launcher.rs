//! Launcher tabs, built-in tools, searchable desktop commands and model updates. `apps.rs` owns the
//! index of command-line tools and servers that run on ANGI; `packages.rs` their install state and actions.
mod apps;
mod packages;

use crate::{
    App, AppWindow, MenuRow,
    keymap::Cmd,
    library::Availability,
    wm::{Desktop, Kind, Mode},
};
use slint::{ModelRc, VecModel};

/// Launcher tabs, in the order the tab strip shows them. The numbers are `AppLauncher.section`.
/// Apps is the landing tab. Install lists software compatible with the device.
const APPS: i32 = 0;
const COMMANDS: i32 = 1;
const SETTINGS: i32 = 2;
const INSTALL: i32 = 3;
/// Vertical space one row takes in the results list, and the space a section heading adds above its first row.
const ROW_STRIDE: f32 = 70.0;
const HEADING: f32 = 34.0;
/// `AppIcon` kind for installable applications.
const APP_ICON: i32 = 16;

/// How a row presents an install: its state (see `MenuRow.state`), progress and what Enter does.
#[derive(Default)]
struct Status {
    state: i32,
    progress: f32,
    /// What Enter does, shown in the footer; empty means "Open".
    action: String,
    /// Installing keeps the launcher open so several apps can be queued; launching closes it.
    keeps_open: bool,
    removable: bool,
}

struct Entry {
    label: String,
    description: String,
    hint: String,
    aliases: &'static str,
    icon: i32,
    section: i32,
    running: bool,
    command: Cmd,
    status: Status,
    /// "Recommended" or empty: a badge beside the name.
    badge: &'static str,
    /// Set on the first row of a section ("INSTALLED · 2"); the list draws it above that row.
    heading: String,
}

fn catalogue(
    desktop: &Desktop,
    modifier: &str,
    availability: &dyn Fn(&apps::Package) -> Availability,
) -> Vec<Entry> {
    let mut entries = Vec::new();
    for (kind, (label, description, aliases)) in Kind::ALL.into_iter().zip([
        (
            "Overview",
            "Your Pi at a glance",
            "angi dashboard home status",
        ),
        (
            "CPU & Safety",
            "Processor activity and network exposure",
            "performance processor usage cpu monitor load network security ports",
        ),
        (
            "Memory & Storage",
            "Memory usage and available disk space",
            "ram disk drive storage memory",
        ),
        (
            "Network",
            "Connections, addresses, and remote access",
            "wifi wi-fi ethernet internet vnc ssh",
        ),
        (
            "Hardware",
            "Explore devices connected to your Pi",
            "devices board usb peripherals",
        ),
        (
            "Services",
            "See what is running on your system",
            "process daemon background service",
        ),
        (
            "System",
            "Operating system and machine details",
            "info version kernel host",
        ),
        (
            "Web",
            "Browse the web with WPE WebKit",
            "browser web internet website wpe webkit",
        ),
        (
            "Terminal",
            "Native shell, tabs and searchable scrollback",
            "terminal shell cli command bash ssh tmux",
        ),
        (
            "Settings",
            "Mouse, Bluetooth and desktop preferences",
            "settings preferences mouse sensitivity acceleration scrolling bluetooth wireless pair devices",
        ),
        (
            "Agent Angi",
            "Build with Agent Angi",
            "agent angi herdr build",
        ),
        (
            "Files",
            "Browse, search and organize files and folders",
            "files file manager folder explorer nautilus browse directory trash copy move",
        ),
    ]) {
        entries.push(Entry {
            label: label.into(),
            description: description.into(),
            hint: if desktop.is_open(kind) { "Running" } else { "" }.into(),
            aliases,
            icon: if kind == Kind::Web {
                12
            } else if kind == Kind::Settings {
                14
            } else if kind == Kind::Terminal {
                7
            } else if kind == Kind::Files {
                15
            } else {
                kind.index() as i32
            },
            section: SETTINGS,
            running: desktop.is_open(kind),
            command: Cmd::Open(kind),
            status: Status::default(),
            // Web is ANGI's own WPE WebKit build, tuned for the Pi's GPU; there is no better one to install.
            badge: if kind == Kind::Web { "Recommended" } else { "" },
            heading: String::new(),
        });
    }
    entries.push(Entry {
        label: "OpenCode".into(),
        description: "AI coding agent in a native terminal tab".into(),
        hint: "Terminal".into(),
        aliases: "opencode ai coding agent assistant",
        icon: 13,
        section: SETTINGS,
        running: false,
        command: Cmd::OpenCode,
        status: Status::default(),
        badge: "",
        heading: String::new(),
    });
    entries.push(Entry {
        label: "System Console".into(),
        description: "Switch to ANGI’s text-based tools".into(),
        hint: "TUI".into(),
        aliases: "console tui recovery",
        icon: 7,
        section: SETTINGS,
        running: false,
        command: Cmd::Tui,
        status: Status::default(),
        badge: "",
        heading: String::new(),
    });
    let mut command = |label: String, description: String, hint: String, icon, cmd| {
        entries.push(Entry {
            label,
            description,
            hint,
            icon,
            command: cmd,
            aliases: "",
            section: COMMANDS,
            running: false,
            status: Status::default(),
            badge: "",
            heading: String::new(),
        });
    };
    if let Some(focused) = desktop.focused() {
        let title = focused.title();
        for (label, description, shortcut, cmd) in [
            (
                "Close window",
                format!("Close {title}; reopen it from Apps"),
                "Q",
                Cmd::Close,
            ),
            (
                "Float or tile window",
                format!("Change how {title} is arranged"),
                "T",
                Cmd::ToggleFloat,
            ),
            (
                "Fullscreen window",
                format!("Give {title} the whole screen"),
                "F",
                Cmd::Fullscreen,
            ),
            (
                "Maximize window",
                format!("Expand {title} within the desktop"),
                "",
                Cmd::Maximize,
            ),
            (
                "Toggle split",
                "Change the direction of tiled windows".into(),
                "J",
                Cmd::ToggleSplit,
            ),
        ] {
            command(
                label.into(),
                description,
                if shortcut.is_empty() {
                    "".into()
                } else {
                    format!("{modifier}+{shortcut}")
                },
                9,
                cmd,
            );
        }
    }
    command(
        if desktop.mode() == Mode::Tiled {
            "Windowed mode"
        } else {
            "Tiled mode"
        }
        .into(),
        "Change the layout of this workspace".into(),
        format!("{modifier}+L"),
        9,
        Cmd::CycleLayout,
    );
    for n in 0..desktop.workspace_count() {
        command(
            format!("Workspace {}", n + 1),
            "Switch desktop workspace".into(),
            format!("{modifier}+{}", (n + 1) % 10),
            8,
            Cmd::Workspace(n),
        );
    }
    command(
        "Sign out".into(),
        "Return to the sign-in screen".into(),
        "".into(),
        11,
        Cmd::Logout,
    );
    command(
        "Restart".into(),
        "Reboot your Pi after confirmation".into(),
        "".into(),
        10,
        Cmd::Reboot,
    );
    command(
        "Shut down".into(),
        "Power off your Pi after confirmation".into(),
        "".into(),
        10,
        Cmd::Shutdown,
    );
    entries.extend(
        apps::index()
            .iter()
            .enumerate()
            .filter_map(|(index, package)| {
                let status = availability(package);
                (package.native || !matches!(status, Availability::Unavailable(_)))
                    .then(|| packages::entry(index, package, status))
            }),
    );
    entries
}

/// Without a query a tab lists its own entries. A query searches every tab at once, so a built-in
/// tool, an installable app and a command can all be found from wherever the launcher is open;
/// the open tab's matches come first among equally good ones.
fn results(
    desktop: &Desktop,
    modifier: &str,
    query: &str,
    section: i32,
    availability: &dyn Fn(&apps::Package) -> Availability,
) -> Vec<Entry> {
    let query = query.trim().to_lowercase();
    let mut catalogue = catalogue(desktop, modifier, availability);
    let mut featured = Vec::new();
    if query.is_empty() && section == APPS {
        for kind in [Kind::Web, Kind::Files, Kind::Terminal] {
            if let Some(index) = catalogue.iter().position(|entry| entry.command == Cmd::Open(kind)) {
                featured.push(catalogue.remove(index));
            }
        }
    }
    let mut entries: Vec<_> = catalogue
        .into_iter()
        .filter(|entry| {
            if query.is_empty() {
                return entry.section == section;
            }
            if section == INSTALL && entry.section != INSTALL {
                return false;
            }
            let searchable =
                format!("{} {} {}", entry.label, entry.description, entry.aliases).to_lowercase();
            query
                .split_whitespace()
                .all(|word| searchable.contains(word))
        })
        .collect();
    if !query.is_empty() {
        // A title match wins over an incidental word in a description.
        entries.sort_by_key(|entry| {
            let label = entry.label.to_lowercase();
            let title = if label.starts_with(&query) {
                0
            } else if format!(" {label}").contains(&format!(" {query}")) {
                // A later word, as in "Google Chrome"; not the middle of one, as in "Telegram".
                1
            } else {
                2
            };
            // Installed apps come before ones that are not, wherever they match.
            (title, entry.section != section, !packages::is_installed(entry), entry.section)
        });
    } else if section == APPS {
        if let Some(first) = entries.first_mut() {
            first.heading = "INSTALLED".into();
        }
        entries.splice(0..0, featured);
    } else if section == INSTALL {
        packages::arrange_install(&mut entries);
    }
    entries
}

impl App {
    pub(super) fn set_menu_rows(&mut self, ui: &AppWindow, query: &str) {
        self.menu_query = query.to_string();
        let library = self.library.clone();
        let entries = results(
            &self.desk,
            self.modk.name(),
            query,
            ui.get_menu_section(),
            &|package| library.catalogue_availability(&package.id, package.native),
        );
        let mut top = 0.0;
        let rows: Vec<_> = entries
            .iter()
            .map(|entry| MenuRow {
                top: {
                    if !entry.heading.is_empty() {
                        top += HEADING;
                    }
                    let here = top;
                    top += ROW_STRIDE;
                    here
                },
                heading: entry.heading.clone().into(),
                badge: entry.badge.into(),
                label: entry.label.clone().into(),
                description: entry.description.clone().into(),
                hint: {
                    let binding = self.bindings.hint(entry.command);
                    if entry.section == COMMANDS || !binding.is_empty() {
                        binding.into()
                    } else {
                        entry.hint.clone().into()
                    }
                },
                icon: entry.icon,
                app: entry.section != COMMANDS,
                running: entry.running,
                state: entry.status.state,
                progress: entry.status.progress,
                action: entry.status.action.clone().into(),
                keeps_open: entry.status.keeps_open,
                removable: entry.status.removable,
            })
            .collect();
        self.menu = entries.into_iter().map(|entry| entry.command).collect();
        ui.set_menu_rows(ModelRc::new(VecModel::from(rows)));
    }

    pub(super) fn open_menu(&mut self, ui: &AppWindow) {
        self.menu_workspace = self.desk.current();
        ui.set_session_open(false);
        ui.set_menu_section(APPS);
        self.set_menu_rows(ui, "");
        ui.set_menu_index(0);
        ui.set_menu_open(true);
        // Local package eligibility can change between catalogue visits.
        self.library.refresh(false);
        // Cheap when the cached answer is fresh; otherwise learns of newer releases of installed apps.
        self.library.check_if_stale();
    }

    pub(super) fn menu_accept(&mut self, ui: &AppWindow, index: i32) {
        if let Some(command) = self.menu.get(index as usize).copied() {
            if matches!(command, Cmd::Open(_) | Cmd::OpenCode) {
                self.desk.goto(self.menu_workspace);
            }
            self.run(ui, command);
        }
    }
}

#[cfg(test)]
mod tests;
