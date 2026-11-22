//! Workspace growth, navigation and window transfers.
use super::{Desktop, Kind, MAX_WORKSPACES, Mode, Workspace};

impl Desktop {
    pub fn workspace_count(&self) -> usize {
        self.ws.len()
    }

    /// Create any missing numbered workspaces through the requested destination.
    pub fn ensure_workspace(&mut self, target: usize) -> bool {
        if target >= MAX_WORKSPACES {
            return false;
        }
        while self.ws.len() <= target {
            let mut workspace = Workspace::empty();
            workspace.mode = Mode::Floating;
            workspace.has_windowed_layout = true;
            self.ws.push(workspace);
        }
        true
    }

    pub fn move_to_workspace(&mut self, target: usize, follow: bool) {
        let new = target >= self.workspace_count();
        if target == self.cur || !self.ensure_workspace(target) {
            return;
        }
        let Some(k) = self.focused() else {
            if new && follow {
                self.goto(target);
            }
            return;
        };
        self.transfer_window(k, target);
        self.ws[target].focus = Some(k);
        if follow {
            self.goto(target);
        }
    }

    /// Keep open windows in the previous workspace, or the next if removing the first.
    pub fn remove_workspace(&mut self, index: usize) {
        if self.workspace_count() <= 1 || index >= self.workspace_count() {
            return;
        }
        let target = if index == 0 { 1 } else { index - 1 };
        let current = self.cur;
        let source_focus = self.ws[index].focus;
        let target_focus = self.ws[target].focus;
        self.cur = index;
        for kind in self.ws[index].z.clone() {
            self.transfer_window(kind, target);
        }
        self.ws[target].focus = if current == index {
            source_focus.or(target_focus)
        } else {
            target_focus.or(source_focus)
        };
        self.ws.remove(index);

        let remap = |old| {
            let retained = if old == index { target } else { old };
            retained - usize::from(retained > index)
        };
        self.cur = remap(current);
        self.prev = remap(self.prev);
    }

    fn transfer_window(&mut self, k: Kind, target: usize) {
        let was_float = self.ws[self.cur].floats.contains(&k);
        let was_collapsed = self.collapsed.contains(&k);
        let rect = if was_collapsed || self.fullscreen_of().is_some() {
            self.float_rect.get(&k).copied()
        } else {
            self.rect_of(k)
        };
        self.detach(k);
        self.attach(target, k);
        if let Some(r) = rect {
            self.float_rect.insert(k, r);
        }
        if was_collapsed {
            self.collapsed.insert(k);
        }
        if was_float {
            let w = &mut self.ws[target];
            w.tree = w.tree.take().and_then(|t| t.remove(k));
            w.floats.push(k);
        }
    }

    pub fn goto(&mut self, index: usize) {
        if index < self.workspace_count() && index != self.cur {
            self.prev = self.cur;
            self.cur = index;
        }
    }

    pub fn next_workspace(&mut self, back: bool) {
        let count = self.workspace_count();
        self.goto(if back {
            (self.cur + count - 1) % count
        } else {
            (self.cur + 1) % count
        });
    }
}
