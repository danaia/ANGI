//! Floating geometry, workspace layout transitions and fullscreen state.
use super::{Axis, COLLAPSED_HEIGHT, Desktop, Edges, Full, Kind, MIN_H, MIN_W, Mode, Node, Rect};

impl Desktop {
    pub(super) fn constrain_floats(&mut self) {
        for rect in self.float_rect.values_mut() {
            rect.x = rect
                .x
                .clamp(self.screen.x - rect.w + 80.0, self.screen.right() - 80.0);
            rect.y = rect.y.clamp(
                self.usable.y,
                (self.screen.bottom() - COLLAPSED_HEIGHT).max(self.usable.y),
            );
        }
    }
    pub fn is_collapsed(&self, kind: Kind) -> bool {
        self.fullscreen_of().is_none() && self.is_floating(kind) && self.collapsed.contains(&kind)
    }

    pub(super) fn visible_float_rect(&self, kind: Kind, rect: Rect) -> Rect {
        if self.is_collapsed(kind) {
            Rect {
                h: COLLAPSED_HEIGHT,
                ..rect
            }
        } else {
            rect
        }
    }

    /// Fold the body away while retaining the full floating geometry for restoration.
    pub fn toggle_collapsed(&mut self, kind: Kind) {
        if !self.is_floating(kind) || self.fullscreen_of().is_some() {
            return;
        }
        let Some(rect) = self.rect_of(kind) else {
            return;
        };
        self.float_rect.entry(kind).or_insert(rect);
        if !self.collapsed.remove(&kind) {
            self.collapsed.insert(kind);
        }
        self.focus(kind);
    }
    /// Super+T. Returns a note when the workspace is in window mode.
    pub fn toggle_float(&mut self) -> Option<&'static str> {
        if self.ws[self.cur].mode == Mode::Floating {
            return Some("Windowed mode: use the layout toggle or Super+L to return to tiled");
        }
        let f = self.focused()?;
        self.collapsed.remove(&f);
        let rect = self.rect_of(f)?;
        if self.ws[self.cur].floats.contains(&f) {
            self.ws[self.cur].floats.retain(|x| *x != f);
            // Tile it back where it was dropped: beside the tile it overlaps most,
            // on the side it sits.
            let drop = self.drop_placement(rect);
            let mut n = self.next_split;
            let w = &mut self.ws[self.cur];
            match (&mut w.tree, drop) {
                (Some(t), Some((anchor, axis, before))) => {
                    t.insert_at(anchor, f, axis, &mut n, before);
                }
                _ => w.tree = Some(Node::Leaf(f)),
            }
            self.next_split = n;
        } else {
            self.float_rect.insert(f, rect);
            let w = &mut self.ws[self.cur];
            w.tree = w.tree.take().and_then(|t| t.remove(f));
            w.floats.push(f);
        }
        None
    }
    /// Where a floating window at `rect` would tile: (anchor tile, split axis, goes before).
    fn drop_placement(&self, rect: Rect) -> Option<(Kind, Axis, bool)> {
        let (tiled, _) = self.tiled(&self.ws[self.cur]);
        let overlap = |r: &Rect| {
            let w = (rect.right().min(r.right()) - rect.x.max(r.x)).max(0.0);
            let h = (rect.bottom().min(r.bottom()) - rect.y.max(r.y)).max(0.0);
            w * h
        };
        let (cx, cy) = rect.center();
        let dist = |r: &Rect| {
            let (ox, oy) = r.center();
            (cx - ox).hypot(cy - oy)
        };
        let (kind, target) = tiled.iter().copied().max_by(|(_, a), (_, b)| {
            overlap(a)
                .total_cmp(&overlap(b))
                .then(dist(b).total_cmp(&dist(a)))
        })?;
        let (tx, ty) = target.center();
        let (dx, dy) = ((cx - tx) / target.w.max(1.0), (cy - ty) / target.h.max(1.0));
        Some(if dx.abs() >= dy.abs() {
            (kind, Axis::Horizontal, dx < 0.0)
        } else {
            (kind, Axis::Vertical, dy < 0.0)
        })
    }
    pub fn toggle_fullscreen(&mut self, kind: Full) {
        let Some(f) = self.focused() else { return };
        self.collapsed.remove(&f);
        let w = &mut self.ws[self.cur];
        w.full = match w.full {
            Some((_, cur)) if cur == kind => None,
            _ => Some((f, kind)),
        };
    }
    /// Switch layouts, keeping each window's floating geometry and collapsed state.
    pub fn cycle_layout(&mut self) {
        if self.ws[self.cur].mode == Mode::Tiled {
            let (tiled, _) = self.tiled(&self.ws[self.cur]);
            for (k, r) in tiled {
                if self.ws[self.cur].has_windowed_layout {
                    self.float_rect.entry(k).or_insert(r);
                } else {
                    self.float_rect.insert(k, r);
                }
            }
            self.ws[self.cur].has_windowed_layout = true;
        }
        let w = &mut self.ws[self.cur];
        w.full = None;
        w.mode = if w.mode == Mode::Tiled {
            Mode::Floating
        } else {
            Mode::Tiled
        };
    }

    pub fn move_floating(&mut self, k: Kind, dx: f32, dy: f32) {
        if let Some(mut r) = self.float_rect.get(&k).copied().or_else(|| self.rect_of(k)) {
            r.x = (r.x + dx).clamp(self.screen.x - r.w + 80.0, self.screen.right() - 80.0);
            r.y = (r.y + dy).clamp(
                self.usable.y,
                (self.screen.bottom() - COLLAPSED_HEIGHT).max(self.usable.y),
            );
            self.float_rect.insert(k, r);
        }
    }
    pub fn resize_floating_edges(&mut self, k: Kind, e: Edges, dx: f32, dy: f32) {
        if self.is_collapsed(k) {
            return;
        }
        let Some(mut r) = self.rect_of(k) else { return };
        if e.right {
            r.w = (r.w + dx).max(MIN_W);
        }
        if e.bottom {
            r.h = (r.h + dy).max(MIN_H);
        }
        if e.left {
            let w = (r.w - dx).max(MIN_W);
            r.x += r.w - w;
            r.w = w;
        }
        if e.top {
            let h = (r.h - dy).max(MIN_H);
            r.y += r.h - h;
            r.h = h;
        }
        self.float_rect.insert(k, r);
    }
}
