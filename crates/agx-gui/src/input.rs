//! Desktop keyboard routing, pointer gestures and drag lifecycle.
use crate::{
    App, AppWindow, TOP,
    key_bindings::Action,
    keymap::{Cmd, Mods},
    wm::{self, Button, Edges, Full, Hit, Kind, Rect},
};
use std::time::{Duration, Instant};

/// A title-bar double click is two nearby clicks, never the end of a drag.
#[derive(Default)]
pub(super) struct HeaderClicks {
    pressed: Option<(Kind, f32, f32)>,
    last: Option<(Kind, f32, f32, Instant)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_clicks_require_the_same_header_and_no_drag() {
        let mut clicks = HeaderClicks::default();
        let now = Instant::now();
        clicks.press(Kind::Overview, 40., 50.);
        assert_eq!(clicks.release(40., 50., now), None);
        clicks.press(Kind::Overview, 42., 50.);
        assert_eq!(
            clicks.release(42., 50., now + Duration::from_millis(200)),
            Some(Kind::Overview)
        );
        clicks.press(Kind::Overview, 40., 50.);
        clicks.release(40., 50., now);
        clicks.press(Kind::Cpu, 40., 50.);
        assert_eq!(
            clicks.release(40., 50., now + Duration::from_millis(200)),
            None
        );
        clicks.press(Kind::Cpu, 40., 50.);
        clicks.moved(80., 80.);
        clicks.moved(40., 50.);
        assert_eq!(
            clicks.release(40., 50., now + Duration::from_millis(300)),
            None
        );
    }

    #[test]
    fn slow_clicks_and_cancelled_presses_do_not_collapse() {
        let mut clicks = HeaderClicks::default();
        let now = Instant::now();
        clicks.press(Kind::Overview, 40., 50.);
        clicks.release(40., 50., now);
        clicks.press(Kind::Overview, 40., 50.);
        assert_eq!(clicks.release(40., 50., now + Duration::from_secs(1)), None);
        clicks.cancel();
        assert_eq!(
            clicks.release(40., 50., now + Duration::from_millis(1100)),
            None
        );
    }
}

impl HeaderClicks {
    fn cancel(&mut self) {
        self.pressed = None;
        self.last = None;
    }

    fn press(&mut self, kind: Kind, x: f32, y: f32) {
        self.pressed = Some((kind, x, y));
    }

    fn moved(&mut self, x: f32, y: f32) {
        if self
            .pressed
            .is_some_and(|(_, px, py)| (x - px).hypot(y - py) > 5.0)
        {
            self.cancel();
        }
    }

    fn release(&mut self, x: f32, y: f32, now: Instant) -> Option<Kind> {
        let (kind, px, py) = self.pressed.take()?;
        if (x - px).hypot(y - py) > 5.0 {
            self.cancel();
            return None;
        }
        if self.last.take().is_some_and(|(last, lx, ly, time)| {
            last == kind
                && now.duration_since(time) <= Duration::from_millis(400)
                && (x - lx).hypot(y - ly) <= 5.0
        }) {
            Some(kind)
        } else {
            self.last = Some((kind, x, y, now));
            None
        }
    }
}

pub(super) enum Drag {
    None,
    /// Tiled windows are dragged as a ghost and swapped on drop; floating ones move.
    Move {
        kind: Kind,
        tiled: bool,
        start: (f32, f32),
    },
    ResizeTiled {
        kind: Kind,
        sx: f32,
        sy: f32,
    },
    ResizeFloat {
        kind: Kind,
        edges: Edges,
    },
    Split {
        split: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Motion {
    Cursor,
    Overlay,
    Geometry,
}

fn owner(hit: Hit) -> Option<Kind> {
    match hit {
        Hit::Button(k, _) | Hit::Header(k) | Hit::Edge(k, _) | Hit::Body(k) => Some(k),
        Hit::Handle(_) | Hit::None => None,
    }
}

impl App {
    pub(super) fn key(&mut self, ui: &AppWindow, name: &str, mods: Mods) -> bool {
        self.header_clicks.cancel();
        // A key press always ends a drag, so a lost button release cannot strand one.
        if !matches!(self.drag, Drag::None) {
            self.drag = Drag::None;
            self.ghost = None;
            self.target = None;
            self.refresh(ui);
        }
        if name == "\u{f70d}" && let Some(Cmd::Calendar) = self.bindings.default_key(name, mods, self.modk) {
            self.run(ui, Cmd::Calendar);
            return true;
        }
        if let Some(result) = self.terminal_shortcut(ui, name, mods) {
            return result;
        }
        if let Some(result) = self.browser_shortcut(ui, name, mods) {
            return result;
        }
        match self.bindings.custom_key(name, mods).or_else(|| self.bindings.default_key(name, mods, self.modk)) {
            Some(cmd) => {
                self.run(ui, cmd);
                true
            }
            None => false,
        }
    }

    // --- pointer ------------------------------------------------------------
    pub(super) fn pointer(
        &mut self,
        ui: &AppWindow,
        kind: i32,
        button: i32,
        x: f32,
        y: f32,
        mods: Mods,
    ) {
        if kind == 2 && self.captured_mouse.is_some_and(|(_, drag)| drag) {
            return;
        }
        if kind == 0 && let Some(action) = self.bindings.mouse(button, mods, self.modk)
            && self.run_mouse_binding(ui, action, x, y, mods)
        {
            return;
        }
        if self.terminal_pointer(ui, kind, button, x, y, mods) {
            self.header_clicks.cancel();
            return;
        }
        if self.browser_pointer(ui, kind, button, x, y, mods) {
            self.header_clicks.cancel();
            return;
        }
        match kind {
            0 => {
                self.down(ui, button, x, y, mods);
                self.refresh(ui);
            }
            1 | 3 => {
                #[cfg(feature = "wayland")]
                let restore_native_focus = kind == 1 && button == 0
                    && matches!(self.view.scene.borrow().hit(x, y), Hit::Header(Kind::Native(_)));
                // A release can carry a newer position than the last move event.
                if kind == 1 {
                    self.moved(x, y);
                }
                self.up(x, y, kind == 1);
                self.refresh(ui);
                #[cfg(feature = "wayland")]
                if restore_native_focus && let Some(Kind::Native(id)) = self.desk.focused() {
                    self.native_tiles.focus(id);
                }
            }
            _ => self.update_pointer(ui, x, y),
        }
    }

    fn update_pointer(&mut self, ui: &AppWindow, x: f32, y: f32) {
        match self.moved(x, y) {
            Motion::Cursor => ui.set_cursor(self.cursor),
            Motion::Overlay => {
                self.view.target(self.target);
                self.refresh_drag(ui);
            }
            Motion::Geometry => {
                self.sync_scene(ui);
                self.refresh_drag(ui);
            }
        }
    }

    pub(super) fn native_mouse_binding(&mut self, ui: &AppWindow, kind: i32, button: i32, x: f32, y: f32) -> bool {
        if let Some((pressed, drag)) = self.captured_mouse {
            if kind == 1 && button == pressed {
                self.captured_mouse = None;
                if drag { self.pointer(ui, 1, button, x, y, self.shortcut_mods); }
                return true;
            }
            if kind == 2 && drag {
                self.update_pointer(ui, x, y);
                return true;
            }
            return false;
        }
        if kind == 0
            && let Some(action) = self.bindings.mouse(button, self.shortcut_mods, self.modk)
            && self.run_mouse_binding(ui, action, x, y, self.shortcut_mods)
        {
            self.captured_mouse = Some((button, !matches!(self.drag, Drag::None)));
            return true;
        }
        false
    }

    pub(super) fn run_mouse_binding(&mut self, ui: &AppWindow, action: Action, x: f32, y: f32, mods: Mods) -> bool {
        if let Action::Command(cmd) = action {
            self.run(ui, cmd);
            return true;
        }
        let hit = self.view.scene.borrow().hit(x, y);
        let Some(kind) = owner(hit) else { return false };
        self.header_clicks.cancel();
        self.last = (x, y);
        self.mods = mods;
        self.desk.focus(kind);
        match action {
            Action::Move => self.begin_move(kind, x, y),
            Action::Resize => self.begin_resize(kind, x, y),
            Action::Command(_) => unreachable!(),
        }
        self.refresh(ui);
        true
    }

    pub(super) fn begin_move(&mut self, k: Kind, x: f32, y: f32) {
        if self.desk.fullscreen_of().is_some() {
            return;
        }
        self.drag = Drag::Move {
            kind: k,
            tiled: !self.desk.is_floating(k),
            start: (x, y),
        };
    }

    pub(super) fn begin_resize(&mut self, k: Kind, x: f32, y: f32) {
        if self.desk.fullscreen_of().is_some() {
            return;
        }
        self.drag = if self.desk.is_floating(k) {
            Drag::ResizeFloat {
                kind: k,
                edges: Edges {
                    right: true,
                    bottom: true,
                    ..Default::default()
                },
            }
        } else {
            let (cx, cy) = self
                .view
                .scene
                .borrow()
                .rect_of(k)
                .map_or((x, y), |r| r.center());
            Drag::ResizeTiled {
                kind: k,
                sx: if x < cx { -1.0 } else { 1.0 },
                sy: if y < cy { -1.0 } else { 1.0 },
            }
        };
    }

    pub(super) fn down(&mut self, ui: &AppWindow, button: i32, x: f32, y: f32, mods: Mods) {
        self.mods = mods;
        self.last = (x, y);
        // Slint owns the top bar buttons. Blank space does not start a desktop gesture.
        if button == 0 && y < TOP && !matches!(self.desk.fullscreen_of(), Some((_, Full::Screen))) {
            return;
        }
        let hit = self.view.scene.borrow().hit(x, y);
        if button != 0 || !matches!(hit, Hit::Header(k) if self.desk.is_floating(k)) {
            self.header_clicks.cancel();
        }
        match button {
            0 => {
                match hit {
                    Hit::Button(k, b) => {
                        self.desk.focus(k);
                        self.run(
                            ui,
                            match b {
                                Button::Float => Cmd::ToggleFloat,
                                Button::Full => Cmd::Fullscreen,
                                Button::Close => Cmd::Close,
                            },
                        );
                    }
                    Hit::Header(k) => {
                        self.desk.focus(k);
                        if self.desk.is_floating(k) {
                            self.header_clicks.press(k, x, y);
                        }
                        self.begin_move(k, x, y);
                    }
                    Hit::Edge(k, e) => {
                        self.desk.focus(k);
                        self.drag = Drag::ResizeFloat { kind: k, edges: e };
                    }
                    Hit::Handle(h) => self.drag = Drag::Split { split: h.split },
                    Hit::Body(k) => self.desk.focus(k),
                    Hit::None => {}
                }
            }
            1 => {
                if let Hit::Header(k) | Hit::Button(k, _) = hit {
                    self.open_panel_menu(ui, k, x, y);
                }
            }
            _ => {}
        }
    }

    pub(super) fn moved(&mut self, x: f32, y: f32) -> Motion {
        self.header_clicks.moved(x, y);
        let (dx, dy) = (x - self.last.0, y - self.last.1);
        self.last = (x, y);
        match self.drag {
            Drag::None => self.cursor = self.hover(x, y),
            Drag::Move {
                kind, tiled: false, ..
            } => self.desk.move_floating(kind, dx, dy),
            Drag::Move {
                kind,
                tiled: true,
                start,
            } => {
                self.ghost = self
                    .view
                    .scene
                    .borrow()
                    .rect_of(kind)
                    .map(|r| Rect::new(r.x + x - start.0, r.y + y - start.1, r.w, r.h));
                self.target = self
                    .view
                    .scene
                    .borrow()
                    .tile_at(x, y)
                    .filter(|t| *t != kind);
            }
            Drag::ResizeTiled { kind, sx, sy } => self.desk.resize_window(kind, sx * dx, sy * dy),
            Drag::ResizeFloat { kind, edges } => {
                self.desk.resize_floating_edges(kind, edges, dx, dy)
            }
            Drag::Split { split } => {
                if let Some(r) = self.view.scene.borrow().handle_ratio(split, x, y) {
                    self.desk.set_split(split, r);
                }
            }
        }
        match self.drag {
            Drag::None => Motion::Cursor,
            Drag::Move { tiled: true, .. } => Motion::Overlay,
            _ => Motion::Geometry,
        }
    }

    pub(super) fn up(&mut self, x: f32, y: f32, commit: bool) {
        if commit {
            if let Some(kind) = self.header_clicks.release(x, y, Instant::now()) {
                self.desk.toggle_collapsed(kind);
            }
        } else {
            self.header_clicks.cancel();
        }
        if commit
            && let Drag::Move {
                kind, tiled: true, ..
            } = self.drag
            && let Some(t) = self
                .view
                .scene
                .borrow()
                .tile_at(x, y)
                .filter(|t| *t != kind)
        {
            self.desk.swap(kind, t);
        }
        self.drag = Drag::None;
        self.ghost = None;
        self.target = None;
        self.cursor = self.hover(x, y);
    }

    pub(super) fn hover(&self, x: f32, y: f32) -> i32 {
        if y < TOP {
            return 6;
        }
        match self.view.scene.borrow().hit(x, y) {
            Hit::Header(_) => 1,
            Hit::Button(..) => 6,
            Hit::Handle(h) => {
                if h.axis == wm::Axis::Horizontal {
                    2
                } else {
                    3
                }
            }
            Hit::Edge(_, e) => match (e.left || e.right, e.top || e.bottom) {
                (true, true) => {
                    if (e.left && e.top) || (e.right && e.bottom) {
                        4
                    } else {
                        5
                    }
                }
                (true, false) => 2,
                _ => 3,
            },
            _ => 0,
        }
    }

    pub(super) fn scroll(&mut self, ui: &AppWindow, dx: f32, delta: f32, mods: Mods) {
        if self.terminal_scroll(ui, delta, mods) {
            return;
        }
        if !self.modk.held(mods) && self.browser_scroll(dx, delta, mods) {
            return;
        }
        if self.modk.held(mods) && delta != 0.0 {
            self.desk.next_workspace(delta > 0.0);
            self.refresh(ui);
        }
    }
}
