//! Attach compositor-owned windows to the same layout as built-in panels.
use super::{Desktop, Kind, Mode, Rect};

impl Desktop {
    #[cfg(feature = "wayland")]
    pub(crate) fn workspace_of(&self, kind: Kind) -> Option<usize> {
        self.ws.iter().position(|workspace| workspace.z.contains(&kind))
    }

    pub(crate) fn attach_native(&mut self, kind: Kind, workspace: usize, rect: Option<Rect>) {
        if self.is_open(kind) || !self.ensure_workspace(workspace) {
            return;
        }
        let rect = self.native_launch_rect(workspace, rect);
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

    fn native_launch_rect(&self, workspace: usize, saved: Option<Rect>) -> Option<Rect> {
        let w = &self.ws[workspace];
        let occupied: Vec<_> = w.z.iter().filter_map(|kind| {
            let rect = *self.float_rect.get(kind)?;
            (w.mode == Mode::Floating || w.floats.contains(kind))
                .then_some(self.visible_float_rect(*kind, rect))
        }).collect();
        let area = self.usable;
        let preferred = saved.or_else(|| (w.mode == Mode::Floating).then(|| self.default_float(w.z.len())))?;
        let overlap = |a: Rect, b: Rect| {
            (a.right().min(b.right()) - a.x.max(b.x)).max(0.0)
                * (a.bottom().min(b.bottom()) - a.y.max(b.y)).max(0.0)
        };
        let fits = |rect: Rect| {
            rect.x >= area.x && rect.right() <= area.right()
                && rect.y >= area.y && rect.bottom() <= area.bottom()
        };
        if occupied.is_empty() || (fits(preferred) && occupied.iter().all(|r| overlap(*r, preferred) == 0.0)) {
            return Some(preferred);
        }
        let gap = super::GAP_IN;
        let free = occupied.iter().flat_map(|r| [
            Rect::new(area.x, area.y, r.x - gap - area.x, area.h),
            Rect::new(r.right() + gap, area.y, area.right() - r.right() - gap, area.h),
            Rect::new(area.x, area.y, area.w, r.y - gap - area.y),
            Rect::new(area.x, r.bottom() + gap, area.w, area.bottom() - r.bottom() - gap),
        ]).filter(|r| r.w >= super::MIN_W && r.h >= super::MIN_H && fits(*r))
            .filter(|r| occupied.iter().all(|o| overlap(*o, *r) == 0.0))
            .max_by(|a, b| (a.w * a.h).total_cmp(&(b.w * b.h)));
        free.or_else(|| (w.mode == Mode::Floating).then(|| {
            let fallback = self.default_float(w.z.len());
            Rect::new(fallback.x, fallback.y, fallback.w.min(area.w * 0.5), fallback.h.min(area.h * 0.65))
        }))
    }

    #[cfg(feature = "wayland")]
    pub(crate) fn native_float_rect(&self, kind: Kind) -> Option<Rect> {
        let ws = self.workspace_of(kind)?;
        (self.ws[ws].mode == super::Mode::Floating || self.ws[ws].floats.contains(&kind))
            .then(|| self.float_rect.get(&kind).copied())
            .flatten()
    }

    #[cfg(feature = "wayland")]
    pub(crate) fn native_tile_extent(&self, kind: Kind) -> Option<(f32, f32)> {
        let workspace = &self.ws[self.workspace_of(kind)?];
        if workspace.mode != Mode::Tiled || workspace.floats.contains(&kind) {
            return None;
        }
        let rect = self.tiled(workspace).0.into_iter().find(|(k, _)| *k == kind)?.1;
        Some((rect.w / self.usable.w.max(1.0), rect.h / self.usable.h.max(1.0)))
    }

    #[cfg(feature = "wayland")]
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

    #[cfg(feature = "wayland")]
    pub(crate) fn forget_native(&mut self, kind: Kind) {
        self.close(kind);
        self.float_rect.remove(&kind);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restoring_a_floating_window_does_not_cover_an_existing_window() {
        let screen = Rect::new(0., 0., 1920., 1080.);
        let mut desk = Desktop::new(screen, screen);
        desk.goto(3);
        if desk.workspace_mode(3) != Mode::Floating {
            desk.cycle_layout();
        }
        let browser = Kind::Native(1);
        let editor = Kind::Native(2);
        desk.attach_native(browser, 3, Some(Rect::new(400., 300., 1152., 594.)));
        let before = desk.rect_of(browser).unwrap();
        desk.attach_native(editor, 3, Some(Rect::new(129., 112., 1802., 926.)));
        let new = desk.rect_of(editor).unwrap();
        assert_eq!(desk.rect_of(browser), Some(before));
        assert!(new.right() <= before.x || new.x >= before.right()
            || new.bottom() <= before.y || new.y >= before.bottom());
    }
}
