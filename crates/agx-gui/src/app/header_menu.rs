//! Context and command dispatch shared by the top bar and window title bars.
use super::App;
use crate::{
    AppWindow, HeaderMenuRow, Icons,
    keymap::Cmd,
    wm::{Kind, MAX_WORKSPACES},
};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::rc::Rc;

#[derive(Default)]
pub(super) struct HeaderMenu {
    window: Option<Kind>,
    commands: Vec<Cmd>,
}

impl App {
    pub(crate) fn open_panel_menu(&mut self, ui: &AppWindow, kind: Kind, x: f32, y: f32) {
        self.desk.focus(kind);
        let mut actions = (0..self.desk.workspace_count())
            .filter(|&ws| ws != self.desk.current())
            .map(|ws| {
                (
                    HeaderMenuRow {
                        label: format!("Workspace {}", ws + 1).into(),
                        ..Default::default()
                    },
                    Cmd::MoveToWorkspace { ws, follow: false },
                )
            })
            .collect::<Vec<_>>();
        actions.push((
            HeaderMenuRow {
                label: "Close window".into(),
                icon: ui.global::<Icons>().get_x(),
                danger: true,
                separator: !actions.is_empty(),
            },
            Cmd::Close,
        ));
        let caption = if self.desk.workspace_count() > 1 {
            "Send to workspace"
        } else {
            ""
        };
        self.show_header_menu(ui, Some(kind), caption, (x, y), actions);
    }

    pub(crate) fn open_workspace_menu(&mut self, ui: &AppWindow, workspace: i32, x: f32, y: f32) {
        let count = self.desk.workspace_count();
        let selected = usize::try_from(workspace)
            .ok()
            .filter(|&index| index < count);
        let mut actions = Vec::new();
        if count < MAX_WORKSPACES {
            actions.push((
                HeaderMenuRow {
                    label: "Add workspace".into(),
                    icon: ui.global::<Icons>().get_plus(),
                    ..Default::default()
                },
                Cmd::AddWorkspace,
            ));
        }
        if let Some(index) = selected.filter(|_| count > 1) {
            actions.push((
                HeaderMenuRow {
                    label: "Remove workspace".into(),
                    icon: ui.global::<Icons>().get_x(),
                    danger: true,
                    separator: !actions.is_empty(),
                },
                Cmd::RemoveWorkspace(index),
            ));
        }
        if actions.is_empty() {
            self.toast(
                ui,
                &format!("All {MAX_WORKSPACES} workspaces are available"),
            );
            return;
        }
        let caption = selected.map_or_else(String::new, |index| format!("Workspace {}", index + 1));
        self.show_header_menu(ui, None, &caption, (x, y), actions);
    }

    fn show_header_menu(
        &mut self,
        ui: &AppWindow,
        window: Option<Kind>,
        caption: &str,
        (x, y): (f32, f32),
        actions: impl IntoIterator<Item = (HeaderMenuRow, Cmd)>,
    ) {
        let (rows, commands): (Vec<_>, Vec<_>) = actions.into_iter().unzip();
        self.header_menu = HeaderMenu { window, commands };
        ui.set_menu_open(false);
        ui.set_session_open(false);
        ui.set_header_menu_caption(caption.into());
        ui.set_header_menu_rows(ModelRc::from(Rc::new(VecModel::from(rows))));
        ui.set_header_menu_x(x);
        ui.set_header_menu_y(y);
        ui.set_header_menu_open(true);
    }

    pub(crate) fn dismiss_header_menu(&mut self, ui: &AppWindow) {
        if ui.get_header_menu_open() {
            self.header_menu = HeaderMenu::default();
            ui.set_header_menu_open(false);
            ui.invoke_focus_desktop();
        }
    }

    pub(crate) fn choose_header_action(&mut self, ui: &AppWindow, index: i32) {
        let Some(&command) = self.header_menu.commands.get(index as usize) else {
            return;
        };
        if let Some(kind) = self.header_menu.window {
            if self.view.scene.borrow().rect_of(kind).is_none() {
                self.dismiss_header_menu(ui);
                return;
            }
            self.desk.focus(kind);
        }
        self.run(ui, command);
    }
}
