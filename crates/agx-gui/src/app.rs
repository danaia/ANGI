//! Desktop state, layout synchronization, commands and session actions.
use crate::input::Drag;
use crate::{
    AppWindow, BOTTOM, Sys, TOP, Terminal, browser, input,
    keymap::{Cmd, ModKey, Mods},
    quit,
    scene_model::SceneModel,
    terminal,
    wm::{self, Desktop, Full, Kind, Mode, Rect},
};
use slint::{ComponentHandle, Model, Timer, VecModel};
use std::process::Command;
use std::{rc::Rc, time::Duration};
mod header_menu;
use header_menu::HeaderMenu;
pub(super) struct App {
    pub(super) terminal: terminal::State,
    pub(super) terminal_kind: Kind,
    pub(super) terminal_windows: std::collections::HashMap<Kind, terminal::State>,
    pub(super) next_terminal: u32,
    pub(super) terminal_views: Rc<VecModel<crate::TerminalView>>,
    pub(super) browser: browser::Browser,
    pub(super) desk: Desktop,
    pub(super) view: SceneModel,
    pub(super) occupied: Rc<VecModel<bool>>,
    pub(super) modk: ModKey,
    pub(super) bindings: crate::key_bindings::Mappings,
    pub(super) shortcut_mods: Mods,
    pub(super) captured_mouse: Option<(i32, bool)>,
    pub(super) drag: Drag,
    pub(super) mods: Mods,
    pub(super) last: (f32, f32),
    pub(super) target: Option<Kind>,
    pub(super) ghost: Option<Rect>,
    pub(super) cursor: i32,
    pub(super) menu: Vec<Cmd>,
    pub(super) menu_workspace: usize,
    /// What the launcher's search box holds, so state changes can re-render the same results.
    pub(super) menu_query: String,
    pub(super) library: crate::library::Library,
    pub(super) pending: Option<Cmd>,
    /// Logical size of the window as last laid out.
    pub(super) size: (f32, f32),
    pub(super) header_clicks: input::HeaderClicks,
    header_menu: HeaderMenu,
}

pub(super) fn session_action(index: i32) -> Option<Cmd> {
    [Cmd::Logout, Cmd::Tui, Cmd::Reboot, Cmd::Shutdown]
        .get(index as usize)
        .copied()
}

impl App {
    pub(super) fn new() -> Self {
        let screen = Rect::new(0.0, 0.0, 1280.0, 720.0);
        let desk = Desktop::new(screen, Rect::new(0.0, TOP, 1280.0, 720.0 - TOP - BOTTOM));
        let view = SceneModel::default();
        view.sync(desk.scene(), None);
        let modk = ModKey::from_env();
        Self {
            terminal: terminal::State::default(),
            terminal_kind: Kind::Terminal,
            terminal_windows: Default::default(),
            next_terminal: 0,
            terminal_views: Default::default(),
            browser: browser::Browser::default(),
            desk,
            view,
            occupied: Rc::new(VecModel::from(vec![false; wm::INITIAL_WORKSPACES])),
            modk,
            bindings: crate::key_bindings::Mappings::new(modk),
            shortcut_mods: Mods::default(),
            captured_mouse: None,
            drag: Drag::None,
            mods: Mods::default(),
            last: (0.0, 0.0),
            target: None,
            ghost: None,
            cursor: 0,
            menu: vec![],
            menu_workspace: 0,
            menu_query: String::new(),
            library: crate::library::Library::start(),
            pending: None,
            size: (1280.0, 720.0),
            header_clicks: input::HeaderClicks::default(),
            header_menu: HeaderMenu::default(),
        }
    }

    pub(super) fn resized(&mut self, ui: &AppWindow, w: f32, h: f32) {
        if std::env::var_os("AGX_GUI_DEBUG").is_some() {
            eprintln!("layout area {w}x{h}");
        }
        self.size = (w, h);
        ui.global::<Sys>()
            .set_screen_size(format!("{}x{}", w as i32, h as i32).into());
        self.desk.set_area(
            Rect::new(0.0, 0.0, w, h),
            Rect::new(0.0, TOP, w, (h - TOP - BOTTOM).max(0.0)),
        );
        self.refresh(ui);
    }

    pub(super) fn refresh(&self, ui: &AppWindow) {
        self.view.sync(self.desk.scene(), self.target);
        let browser_visible = self
            .view
            .scene
            .borrow()
            .windows
            .iter()
            .any(|window| window.kind == Kind::Web && !window.collapsed);
        let terminal_search = self.desk.focused().is_some_and(Kind::is_terminal)
            && ui.global::<Terminal>().get_search_visible()
            && self
                .view
                .scene
                .borrow()
                .windows
                .iter()
                .any(|w| w.kind.is_terminal() && !w.collapsed);
        if !ui.global::<crate::Calendar>().get_open() && !ui.get_header_menu_open()
            && !terminal_search
            && self.desk.focused() != Some(Kind::Settings)
            && self.desk.focused() != Some(Kind::AgentAngi)
            && self.desk.focused() != Some(Kind::Files)
            && (self.desk.focused() != Some(Kind::Web) || !browser_visible)
        {
            ui.invoke_focus_desktop();
        }
        let workspace_count = self.desk.workspace_count();
        while self.occupied.row_count() > workspace_count {
            self.occupied.remove(self.occupied.row_count() - 1);
        }
        for i in 0..workspace_count {
            let occupied = self.desk.occupied(i);
            match self.occupied.row_data(i) {
                None => self.occupied.push(occupied),
                Some(old) if old != occupied => self.occupied.set_row_data(i, occupied),
                _ => {}
            }
        }
        let current = self.desk.current();
        #[cfg(feature = "wayland")]
        if current as i32 != ui.get_workspace() {
            crate::graphics::wayland_workspaces::select(current);
        }
        ui.set_workspace(current as i32);
        ui.set_windowed_mode(self.desk.mode() == Mode::Floating);
        ui.set_layout_name(
            match (self.desk.fullscreen_of(), self.desk.mode()) {
                (Some((_, Full::Screen)), _) => "fullscreen",
                (Some((_, Full::Maximized)), _) => "maximized",
                (None, Mode::Tiled) => "tiled",
                (None, Mode::Floating) => "windowed",
            }
            .into(),
        );
        ui.set_focus_title(self.desk.focused().map_or("", Kind::title).into());
        ui.set_screen_full(self.view.scene.borrow().screen_full);
        self.refresh_drag(ui);
    }

    pub(super) fn fullscreen_action(&mut self, ui: &AppWindow, action: i32) {
        let Some((kind, Full::Screen)) = self.desk.fullscreen_of() else {
            return;
        };
        self.desk.focus(kind);
        if action == 2 {
            self.run(ui, Cmd::Close);
            return;
        }
        self.desk.toggle_fullscreen(Full::Screen);
        if action == 1 {
            if !self.desk.is_floating(kind) {
                self.desk.toggle_float();
            }
            self.desk.toggle_collapsed(kind);
        }
        self.refresh(ui);
    }

    pub(super) fn refresh_drag(&self, ui: &AppWindow) {
        ui.set_dragging(!matches!(self.drag, Drag::None));
        ui.set_cursor(self.cursor);
        match self.ghost {
            Some(g) => {
                ui.set_ghost_x(g.x);
                ui.set_ghost_y(g.y);
                ui.set_ghost_w(g.w);
                ui.set_ghost_h(g.h);
                ui.set_ghost_visible(true);
            }
            None => ui.set_ghost_visible(false),
        }
    }

    pub(super) fn toast(&self, ui: &AppWindow, text: &str) {
        ui.set_toast(text.into());
        let weak = ui.as_weak();
        Timer::single_shot(Duration::from_secs(4), move || {
            if let Some(ui) = weak.upgrade() {
                ui.set_toast("".into());
            }
        });
    }

    // --- commands -----------------------------------------------------------
    pub(super) fn run(&mut self, ui: &AppWindow, cmd: Cmd) {
        self.dismiss_header_menu(ui);
        self.header_clicks = input::HeaderClicks::default();
        match cmd {
            Cmd::Focus(d) => self.desk.focus_dir(d),
            Cmd::Swap(d) => self.desk.swap_dir(d),
            Cmd::Close => {
                if let Some(k) = self.desk.focused() {
                    if k == Kind::Web && self.browser_request_close() {
                        return;
                    }
                    self.desk.close(k);
                    self.toast(
                        ui,
                        &format!("Closed {}. Reopen it from Applications.", k.title()),
                    );
                }
            }
            Cmd::ToggleSplit => self.desk.toggle_split(),
            Cmd::ToggleFloat => {
                if let Some(note) = self.desk.toggle_float() {
                    self.toast(ui, note);
                }
            }
            Cmd::Fullscreen => self.desk.toggle_fullscreen(Full::Screen),
            Cmd::Maximize => self.desk.toggle_fullscreen(Full::Maximized),
            Cmd::CycleLayout => self.desk.cycle_layout(),
            Cmd::AddWorkspace => {
                self.desk.ensure_workspace(self.desk.workspace_count());
            }
            Cmd::RemoveWorkspace(index) => {
                #[cfg(feature = "wayland")]
                let previous_count = self.desk.workspace_count();
                self.desk.remove_workspace(index);
                #[cfg(feature = "wayland")]
                if self.desk.workspace_count() != previous_count {
                    crate::graphics::wayland_workspaces::remove(index, self.desk.current());
                }
            }
            Cmd::Workspace(n) => self.desk.goto(n),
            Cmd::MoveToWorkspace { ws, follow } => self.desk.move_to_workspace(ws, follow),
            Cmd::NextWorkspace => self.desk.next_workspace(false),
            Cmd::PrevWorkspace => self.desk.next_workspace(true),
            Cmd::CycleFocus(back) => self.desk.cycle_focus(back),
            Cmd::Resize { dw, dh } => self.desk.resize(dw, dh),
            Cmd::Menu => self.open_menu(ui),
            Cmd::Calendar => ui.invoke_open_calendar(),
            Cmd::Open(Kind::Terminal) => self.terminal_new_window(ui),
            Cmd::Open(k) => {
                self.desk.open(k);
                if k == Kind::Settings {
                    let settings = ui.global::<crate::MouseSettings>();
                    settings.set_focus_serial(settings.get_focus_serial() + 1);
                }
            }
            Cmd::OpenCode => self.terminal_open_opencode(ui),
            Cmd::Package(index) => {
                self.package_activate(ui, index);
                // Not the refresh below: it hands keyboard focus to the desktop, and an install keeps the
                // launcher open so more apps can be chosen. Opening an app refreshes through the terminal.
                return;
            }
            Cmd::Tui => self.terminal_open_tui(ui),
            Cmd::Logout => quit(0),
            Cmd::Reboot | Cmd::Shutdown | Cmd::RefreshDesktop => {
                ui.set_confirm_text(
                    if cmd == Cmd::RefreshDesktop {
                        "Refresh the ANGI desktop? Open terminal and browser sessions will close. You will stay signed in."
                    } else if cmd == Cmd::Reboot {
                        "Reboot this Pi? Network sessions will close."
                    } else {
                        "Power off this Pi? Network sessions will close."
                    }
                    .into(),
                );
                self.pending = Some(cmd);
                ui.invoke_focus_desktop();
            }
        }
        self.refresh(ui);
    }

    pub(super) fn confirm(&mut self, ui: &AppWindow, yes: bool) {
        ui.set_confirm_text("".into());
        let cmd = self.pending.take();
        if yes && cmd == Some(Cmd::RefreshDesktop) {
            match crate::settings::flush() {
                Ok(()) => crate::session_refresh::request(),
                Err(error) => self.toast(
                    ui,
                    &format!("Could not save settings before refresh: {error}"),
                ),
            }
        } else if yes {
            power(ui, cmd);
        } else if self.desk.focused() == Some(Kind::Settings) {
            let settings = ui.global::<crate::MouseSettings>();
            settings.set_focus_serial(settings.get_focus_serial() + 1);
        }
    }
}

fn power(ui: &AppWindow, cmd: Option<Cmd>) {
    let verb = match cmd {
        Some(Cmd::Reboot) => "reboot",
        Some(Cmd::Shutdown) => "shutdown",
        _ => return,
    };
    let sys = ui.global::<Sys>();
    if cfg!(feature = "kms") {
        // The CLI already confirms by flag; this GUI asked the user first.
        match Command::new("agx").args([verb, "--yes"]).status() {
            Ok(status) if status.success() => {}
            Ok(status) => sys.set_warnings(format!("agx {verb} exited with {status}").into()),
            Err(error) => sys.set_warnings(format!("agx {verb}: {error}").into()),
        }
    } else {
        sys.set_warnings(format!("{verb} is disabled in the desktop preview").into());
    }
}
