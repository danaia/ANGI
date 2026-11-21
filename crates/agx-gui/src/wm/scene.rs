//! Hit testing and spatial queries over a laid-out scene.
use super::{
    Axis, BORDER, BTN_INSET, BTN_W, Button, Edges, HEADER, Handle, Hit, Kind, Placed, Rect, Scene,
};
const GRAB: f32 = 8.0;
const EDGE: f32 = 6.0;

impl Scene {
    pub fn handle_ratio(&self, split: u32, pointer_x: f32, pointer_y: f32) -> Option<f32> {
        let h = self.handles.iter().find(|h| h.split == split)?;
        Some(match h.axis {
            Axis::Horizontal => (pointer_x - h.region.x) / h.region.w.max(1.0),
            Axis::Vertical => (pointer_y - h.region.y) / h.region.h.max(1.0),
        })
    }

    pub fn rect_of(&self, k: Kind) -> Option<Rect> {
        self.windows.iter().find(|p| p.kind == k).map(|p| p.rect)
    }

    pub fn hit(&self, x: f32, y: f32) -> Hit {
        for p in self.windows.iter().rev().filter(|p| p.floating) {
            if let Some(h) = hit_window(p, x, y, true) {
                return h;
            }
        }
        for h in &self.handles {
            if let Some(hit) = handle_hit(h, x, y) {
                return hit;
            }
        }
        for p in self.windows.iter().rev() {
            if let Some(h) = hit_window(p, x, y, false) {
                return h;
            }
        }
        Hit::None
    }
    /// The tiled window under a point, for drop targets.
    pub fn tile_at(&self, x: f32, y: f32) -> Option<Kind> {
        self.windows
            .iter()
            .find(|p| !p.floating && p.rect.contains(x, y))
            .map(|p| p.kind)
    }
}

fn handle_hit(h: &Handle, x: f32, y: f32) -> Option<Hit> {
    let r = h.region;
    let inside = match h.axis {
        Axis::Horizontal => (x - h.pos).abs() <= GRAB && y >= r.y && y < r.bottom(),
        Axis::Vertical => (y - h.pos).abs() <= GRAB && x >= r.x && x < r.right(),
    };
    inside.then_some(Hit::Handle(*h))
}

fn hit_window(p: &Placed, x: f32, y: f32, grab_edges: bool) -> Option<Hit> {
    let r = p.rect;
    if grab_edges && !p.collapsed {
        let outer = Rect::new(r.x - EDGE, r.y - EDGE, r.w + 2.0 * EDGE, r.h + 2.0 * EDGE);
        if !outer.contains(x, y) {
            return None;
        }
        let e = Edges {
            left: x < r.x + EDGE,
            right: x >= r.right() - EDGE,
            top: y < r.y + EDGE,
            bottom: y >= r.bottom() - EDGE,
        };
        if e != Edges::default() {
            return Some(Hit::Edge(p.kind, e));
        }
    }
    if !r.contains(x, y) {
        return None;
    }
    if p.collapsed || y < r.y + HEADER {
        let from_right = r.right() - BTN_INSET - x;
        if from_right >= 0.0 && y >= r.y + BORDER {
            let b = match (from_right / BTN_W) as i32 {
                0 => Some(Button::Close),
                1 => Some(Button::Full),
                2 => Some(Button::Float),
                _ => None,
            };
            if let Some(b) = b {
                return Some(Hit::Button(p.kind, b));
            }
        }
        return Some(Hit::Header(p.kind));
    }
    Some(Hit::Body(p.kind))
}
