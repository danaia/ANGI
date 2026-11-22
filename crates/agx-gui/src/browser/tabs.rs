//! Browser tab strip state and desktop commands. WebKit owns the pages and history.
use super::{Browser, protocol::MAX_TABS};
use crate::{App, AppWindow, Web, wm::Kind};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

#[derive(Default)]
pub(super) struct Tabs {
    active: usize,
    titles: Vec<String>,
}

impl Tabs {
    pub fn update(&mut self, web: &Web<'_>, active: usize, titles: Vec<String>) {
        self.active = active;
        self.titles = titles;
        self.publish(web);
    }

    pub fn publish(&self, web: &Web<'_>) {
        let labels = self
            .titles
            .iter()
            .map(|title| SharedString::from(title.as_str()))
            .collect::<Vec<_>>();
        web.set_tabs(ModelRc::new(VecModel::from(labels)));
        web.set_active_tab(self.active as i32);
    }

    fn command(&self, action: u8, index: usize) -> Option<Vec<u8>> {
        if self.titles.is_empty() {
            return None;
        }
        let valid = match action {
            0 => self.titles.len() < MAX_TABS,
            1 => index < self.titles.len() && index != self.active,
            2 => index == self.active,
            _ => false,
        };
        valid.then(|| vec![action, index as u8])
    }
}

impl Browser {
    pub(super) fn tab_shortcut(&self, ui: &AppWindow, key: &str, reverse: bool) -> bool {
        let count = self.tabs.titles.len();
        if count == 0 {
            return false;
        }
        let (action, index) = match key {
            "t" if !reverse => (0, 0),
            "w" if !reverse => (2, self.tabs.active),
            "tab" => (
                1,
                (self.tabs.active + if reverse { count - 1 } else { 1 }) % count,
            ),
            _ => return false,
        };
        if let Some(data) = self.tabs.command(action, index) {
            self.send(17, data);
            self.focus_address(ui, false);
        }
        true
    }
}

impl App {
    pub(crate) fn browser_tab_command(&mut self, ui: &AppWindow, action: i32, index: i32) {
        if ui.global::<Web>().get_popup_kind() != 0 || self.browser.closing.is_some() {
            return;
        }
        let (Ok(action), Ok(index)) = (u8::try_from(action), usize::try_from(index)) else {
            return;
        };
        if let Some(data) = self.browser.tabs.command(action, index) {
            self.desk.focus(Kind::Web);
            self.refresh(ui);
            self.browser.send(17, data);
            self.browser.focus_address(ui, false);
        }
    }
}
