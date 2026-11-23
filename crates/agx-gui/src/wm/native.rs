//! Attach compositor-owned windows to the same layout as built-in panels.
use super::{Desktop, Kind, Mode, Rect};

impl Desktop {
    pub(crate) fn workspace_of(&self, kind: Kind) -> Option<usize> {
        self.ws.iter().position(|workspace| workspace.z.contains(&kind))
    }

    pub(crate) fn attach_native(&mut self, kind: Kind, workspace: usize, rect: Option<Rect>) {
        if self.is_open(kind) || !self.ensure_workspace(workspace) {
            return;
        }
        self.attach(workspace, kind);
        if let Some(rect) = rect {
            self.float_rect.insert(kind, rect);
            if self.ws[workspace].mode == Mode::Tiled {
                let w = &mut self.ws[workspace];
                w.tree = w.tree.take().and_then(|tree| tree.remove(kind));
                w.floats.push(kind);
            }
            self.constrain_floats();
        }
    }

    pub(crate) fn native_float_rect(&self, kind: Kind) -> Option<Rect> {
        let ws = self.workspace_of(kind)?;
        (self.ws[ws].mode == super::Mode::Floating || self.ws[ws].floats.contains(&kind))
            .then(|| self.float_rect.get(&kind).copied())
            .flatten()
    }

    pub(crate) fn native_tile_extent(&self, kind: Kind) -> Option<(f32, f32)> {
        let workspace = &self.ws[self.workspace_of(kind)?];
        if workspace.mode != Mode::Tiled || workspace.floats.contains(&kind) {
            return None;
        }
        let rect = self.tiled(workspace).0.into_iter().find(|(k, _)| *k == kind)?.1;
        Some((rect.w / self.usable.w.max(1.0), rect.h / self.usable.h.max(1.0)))
    }

    pub(crate) fn restore_native_extent(&mut self, kind: Kind, (width, height): (f32, f32)) {
        if !(0.0..=1.0).contains(&width) || !(0.0..=1.0).contains(&height) {
            return;
        }
        let Some(workspace) = self.workspace_of(kind) else { return };
        let current = self.cur;
        self.cur = workspace;
        if self.native_tile_extent(kind).is_some() && let Some(rect) = self.rect_of(kind) {
            self.resize_window(kind, width * self.usable.w - rect.w, height * self.usable.h - rect.h);
        }
        self.cur = current;
    }

    pub(crate) fn workspace_mode(&self, workspace: usize) -> Mode {
        self.ws[workspace].mode
    }

    pub(crate) fn set_native_workspace_mode(&mut self, workspace: usize, mode: Mode) {
        if self.ws[workspace].mode == mode
            || self.ws[workspace].z.iter().any(|kind| matches!(kind, Kind::Native(_)))
        {
            return;
        }
        let current = self.cur;
        self.cur = workspace;
        self.cycle_layout();
        self.cur = current;
    }

    pub(crate) fn forget_native(&mut self, kind: Kind) {
        self.close(kind);
        self.float_rect.remove(&kind);
    }
}
