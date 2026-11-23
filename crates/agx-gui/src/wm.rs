//! Window management modelled on Omarchy's Hyprland setup: dwindle (binary
//! split) tiling, 10px outer and 5px inner gaps, directional focus and swap,
//! per-window floating, fullscreen and maximize, workspaces, and a floating
//! "window mode" for a whole workspace. Pure logic: no UI types, fully tested.
use std::collections::{HashMap, HashSet};

mod scene;
mod native;
mod preview;
mod startup;
mod tiling;
mod windowed;
mod workspaces;

// Pebble Line desktop metrics; ui/pebble/tokens.slint mirrors them (gap-out, header, frame-inset,
// header-button, header-inset). Change both together.
pub const GAP_OUT: f32 = 16.0;
pub const GAP_IN: f32 = 8.0;
/// Title bar height inside a window frame; the Slint frame uses the same value.
pub const HEADER: f32 = 44.0;
pub const COLLAPSED_HEIGHT: f32 = HEADER + BORDER;
pub const BORDER: f32 = 2.0;
/// Width of each title bar button (float, fullscreen, close), from the right.
pub const BTN_W: f32 = 32.0;
/// Space between the frame's right edge and the close button, clear of the rounded corner.
pub const BTN_INSET: f32 = 10.0;
pub const MIN_W: f32 = 220.0;
pub const MIN_H: f32 = 120.0;
pub const INITIAL_WORKSPACES: usize = 5;
pub const MAX_WORKSPACES: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }
    pub fn inset(&self, d: f32) -> Rect {
        Rect::new(
            self.x + d,
            self.y + d,
            (self.w - 2.0 * d).max(0.0),
            (self.h - 2.0 * d).max(0.0),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}
/// `Horizontal` puts the two children side by side; `Vertical` stacks them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Tiled,
    /// Omarchy's floating workspace: every window floats with a title bar.
    Floating,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Full {
    /// Covers the whole screen, bars included.
    Screen,
    /// Fills the workspace area ("full width" in Omarchy).
    Maximized,
}

/// Each panel exists once, so its kind doubles as the window id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Overview,
    Cpu,
    Memory,
    Network,
    Hardware,
    Services,
    System,
    Web,
    Terminal,
    Settings,
    AgentAngi,
    Files,
    TerminalWindow(u32),
    Native(u32),
}
impl Kind {
    pub const ALL: [Kind; 12] = [
        Kind::Overview,
        Kind::Cpu,
        Kind::Memory,
        Kind::Network,
        Kind::Hardware,
        Kind::Services,
        Kind::System,
        Kind::Web,
        Kind::Terminal,
        Kind::Settings,
        Kind::AgentAngi,
        Kind::Files,
    ];
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Terminal | Self::TerminalWindow(_))
    }
    pub fn index(self) -> usize {
        match self {
            Self::TerminalWindow(id) => Self::ALL.len() + id as usize,
            // Native Wayland window IDs occupy the negative Slint model keys.
            Self::Native(id) => (-(id as i32)) as usize,
            other => Self::ALL.iter().position(|kind| *kind == other).unwrap(),
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Kind::Overview => "ANGI",
            Kind::Cpu => "CPU & Safety",
            Kind::Memory => "Memory and storage",
            Kind::Network => "Network",
            Kind::Hardware => "Hardware",
            Kind::Services => "Services",
            Kind::System => "System",
            Kind::Web => "Web",
            Kind::Terminal | Kind::TerminalWindow(_) => "Terminal",
            Kind::Settings => "Settings",
            Kind::AgentAngi => "Agent Angi",
            Kind::Files => "Files",
            Kind::Native(_) => "Application",
        }
    }
}

#[derive(Clone, Debug)]
enum Node {
    Leaf(Kind),
    Split {
        id: u32,
        axis: Axis,
        ratio: f32,
        a: Box<Node>,
        b: Box<Node>,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct Handle {
    pub split: u32,
    pub axis: Axis,
    /// The cell the split divides; used to turn pointer position into a ratio.
    pub region: Rect,
    /// Where the gap sits: an x for side-by-side splits, a y for stacked ones.
    pub pos: f32,
}

#[derive(Clone, Debug)]
struct Workspace {
    tree: Option<Node>,
    /// Windows floated one by one (Super+T); they are not in the tree.
    floats: Vec<Kind>,
    /// Every window here, back to front.
    z: Vec<Kind>,
    mode: Mode,
    focus: Option<Kind>,
    full: Option<(Kind, Full)>,
    has_windowed_layout: bool,
}
impl Workspace {
    fn empty() -> Self {
        Self {
            tree: None,
            floats: vec![],
            z: vec![],
            mode: Mode::Tiled,
            focus: None,
            full: None,
            has_windowed_layout: false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Placed {
    pub kind: Kind,
    pub rect: Rect,
    pub floating: bool,
    pub focused: bool,
    pub collapsed: bool,
}
#[derive(Clone, Debug, Default)]
pub struct Scene {
    /// Back to front.
    pub windows: Vec<Placed>,
    pub handles: Vec<Handle>,
    /// True when a window covers the whole screen and the bars should hide.
    pub screen_full: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Float,
    Full,
    Close,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Edges {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}
#[derive(Clone, Copy, Debug)]
pub enum Hit {
    None,
    Button(Kind, Button),
    Header(Kind),
    Edge(Kind, Edges),
    Handle(Handle),
    Body(Kind),
}

pub struct Desktop {
    ws: Vec<Workspace>,
    cur: usize,
    prev: usize,
    next_split: u32,
    usable: Rect,
    screen: Rect,
    float_rect: HashMap<Kind, Rect>,
    collapsed: HashSet<Kind>,
}

impl Desktop {
    pub fn set_area(&mut self, screen: Rect, usable: Rect) {
        // Follow display sizing for windows still in their original layout. Once
        // moved or resized, a floating window keeps its user-chosen geometry.
        let unchanged: Vec<_> = self
            .ws
            .iter()
            .filter(|w| w.mode == Mode::Floating)
            .flat_map(|w| self.tiled(w).0)
            .filter(|(k, r)| self.float_rect.get(k) == Some(r))
            .map(|(k, _)| k)
            .collect();
        self.screen = screen;
        self.usable = usable;
        let resized: Vec<_> = self.ws.iter().flat_map(|w| self.tiled(w).0).collect();
        for (k, r) in resized {
            if unchanged.contains(&k) {
                self.float_rect.insert(k, r);
            }
        }
        self.constrain_floats();
    }
    pub fn current(&self) -> usize {
        self.cur
    }
    pub fn mode(&self) -> Mode {
        self.ws[self.cur].mode
    }
    pub fn focused(&self) -> Option<Kind> {
        self.ws[self.cur].focus
    }
    pub fn occupied(&self, i: usize) -> bool {
        self.ws.get(i).is_some_and(|w| !w.z.is_empty())
    }
    pub fn is_open(&self, k: Kind) -> bool {
        self.ws.iter().any(|w| w.z.contains(&k))
    }
    pub fn is_floating(&self, k: Kind) -> bool {
        let w = &self.ws[self.cur];
        w.mode == Mode::Floating || w.floats.contains(&k)
    }
    pub fn fullscreen_of(&self) -> Option<(Kind, Full)> {
        self.ws[self.cur].full
    }

    fn tiled(&self, w: &Workspace) -> (Vec<(Kind, Rect)>, Vec<Handle>) {
        let (mut out, mut handles) = (vec![], vec![]);
        if let Some(tree) = &w.tree {
            tree.layout(self.usable.inset(GAP_OUT - GAP_IN), &mut out, &mut handles);
        }
        (out, handles)
    }

    fn default_float(&self, n: usize) -> Rect {
        let (w, h) = (self.usable.w * 0.6, self.usable.h * 0.6);
        let off = 30.0 * (n % 6) as f32;
        Rect::new(self.usable.x + 40.0 + off, self.usable.y + 30.0 + off, w, h)
    }

    pub fn scene(&self) -> Scene {
        let w = &self.ws[self.cur];
        if let Some((k, f)) = w.full {
            let rect = match f {
                Full::Screen => self.screen,
                Full::Maximized => self.usable.inset(GAP_OUT),
            };
            return Scene {
                windows: vec![Placed {
                    kind: k,
                    rect,
                    floating: false,
                    focused: true,
                    collapsed: false,
                }],
                handles: vec![],
                screen_full: f == Full::Screen,
            };
        }
        let (tiled, handles) = self.tiled(w);
        let tiled: HashMap<Kind, Rect> = tiled.into_iter().collect();
        let mut windows = vec![];
        let focused = |k| w.focus == Some(k);
        match w.mode {
            Mode::Tiled => {
                for k in &w.z {
                    if let Some(r) = tiled.get(k) {
                        windows.push(Placed {
                            kind: *k,
                            rect: *r,
                            floating: false,
                            focused: focused(*k),
                            collapsed: false,
                        });
                    }
                }
                for k in &w.z {
                    if w.floats.contains(k) {
                        let rect = self
                            .float_rect
                            .get(k)
                            .copied()
                            .unwrap_or_else(|| self.default_float(0));
                        windows.push(Placed {
                            kind: *k,
                            rect: self.visible_float_rect(*k, rect),
                            floating: true,
                            focused: focused(*k),
                            collapsed: self.is_collapsed(*k),
                        });
                    }
                }
                Scene {
                    windows,
                    handles,
                    screen_full: false,
                }
            }
            Mode::Floating => {
                for (n, k) in w.z.iter().enumerate() {
                    let rect = self
                        .float_rect
                        .get(k)
                        .copied()
                        .or_else(|| tiled.get(k).copied())
                        .unwrap_or_else(|| self.default_float(n));
                    windows.push(Placed {
                        kind: *k,
                        rect: self.visible_float_rect(*k, rect),
                        floating: true,
                        focused: focused(*k),
                        collapsed: self.is_collapsed(*k),
                    });
                }
                Scene {
                    windows,
                    handles: vec![],
                    screen_full: false,
                }
            }
        }
    }

    pub fn rect_of(&self, k: Kind) -> Option<Rect> {
        if self.fullscreen_of().is_none()
            && self.is_floating(k)
            && let Some(rect) = self.float_rect.get(&k)
        {
            return Some(self.visible_float_rect(k, *rect));
        }
        self.scene().rect_of(k)
    }

    // --- focus ---------------------------------------------------------
    pub fn focus(&mut self, k: Kind) {
        let w = &mut self.ws[self.cur];
        if w.z.contains(&k) {
            w.focus = Some(k);
            w.z.retain(|x| *x != k);
            w.z.push(k);
        }
    }
    pub fn neighbor(&self, from: Kind, dir: Dir) -> Option<Kind> {
        let scene = self.scene();
        let me = scene.windows.iter().find(|p| p.kind == from)?;
        let (cx, cy) = me.rect.center();
        scene
            .windows
            .iter()
            .filter(|p| p.kind != from)
            .filter_map(|p| {
                let (ox, oy) = p.rect.center();
                let (along, across) = match dir {
                    Dir::Left => (cx - ox, (cy - oy).abs()),
                    Dir::Right => (ox - cx, (cy - oy).abs()),
                    Dir::Up => (cy - oy, (cx - ox).abs()),
                    Dir::Down => (oy - cy, (cx - ox).abs()),
                };
                (along > 1.0).then_some((along + 2.0 * across, p.kind))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, k)| k)
    }
    pub fn focus_dir(&mut self, dir: Dir) {
        if let Some(f) = self.focused()
            && let Some(n) = self.neighbor(f, dir)
        {
            self.focus(n);
        }
    }
    pub fn cycle_focus(&mut self, back: bool) {
        let mut order: Vec<Kind> = self.ws[self.cur].z.clone();
        order.sort_by_key(|k| k.index());
        let Some(f) = self.focused() else { return };
        let Some(pos) = order.iter().position(|k| *k == f) else {
            return;
        };
        let n = order.len();
        let next = if back {
            (pos + n - 1) % n
        } else {
            (pos + 1) % n
        };
        self.focus(order[next]);
    }

    // --- open, close, move ----------------------------------------------
    /// Open (or bring here) a panel on the current workspace.
    pub fn open(&mut self, k: Kind) {
        self.collapsed.remove(&k);
        if self.ws[self.cur].z.contains(&k) {
            self.focus(k);
            return;
        }
        self.detach(k);
        self.attach(self.cur, k);
        self.focus(k);
    }
    pub fn close(&mut self, k: Kind) {
        self.detach(k);
        self.collapsed.remove(&k);
    }
    /// Remove a panel from whatever workspace holds it.
    fn detach(&mut self, k: Kind) {
        self.collapsed.remove(&k);
        for w in &mut self.ws {
            if !w.z.contains(&k) {
                continue;
            }
            w.tree = w.tree.take().and_then(|t| t.remove(k));
            w.floats.retain(|x| *x != k);
            w.z.retain(|x| *x != k);
            if w.full.is_some_and(|(f, _)| f == k) {
                w.full = None;
            }
            if w.focus == Some(k) {
                w.focus = w.z.last().copied();
            }
        }
    }
    fn attach(&mut self, ws: usize, k: Kind) {
        let n = self.ws[ws].z.len();
        let anchor = self.ws[ws]
            .focus
            .filter(|f| self.ws[ws].tree.as_ref().is_some_and(|t| t.contains(*f)));
        let axis = anchor
            .and_then(|a| {
                let saved = self.cur;
                self.cur = ws;
                let r = self.rect_of(a);
                self.cur = saved;
                r
            })
            .map_or(Axis::Horizontal, |r| {
                if r.w >= r.h {
                    Axis::Horizontal
                } else {
                    Axis::Vertical
                }
            });
        let mut next = self.next_split;
        let w = &mut self.ws[ws];
        match (&mut w.tree, anchor) {
            (None, _) => w.tree = Some(Node::Leaf(k)),
            (Some(t), Some(a)) => {
                t.insert_at(a, k, axis, &mut next, false);
            }
            (Some(t), None) => {
                // No tiled focus: split the first leaf.
                let mut leaves = vec![];
                leaves_of(t, &mut leaves);
                t.insert_at(leaves[0], k, axis, &mut next, false);
            }
        }
        self.next_split = next;
        self.ws[ws].z.push(k);
        let fallback = self.default_float(n);
        self.float_rect.entry(k).or_insert(fallback);
    }
    // --- layout operations ------------------------------------------------
    pub fn swap(&mut self, a: Kind, b: Kind) {
        let w = &mut self.ws[self.cur];
        if let Some(t) = &mut w.tree
            && t.contains(a)
            && t.contains(b)
        {
            t.swap_leaves(a, b);
        }
    }
    pub fn swap_dir(&mut self, dir: Dir) {
        if let Some(f) = self.focused()
            && let Some(n) = self.neighbor(f, dir)
        {
            self.swap(f, n);
        }
    }
    pub fn toggle_split(&mut self) {
        if let Some(f) = self.focused()
            && let Some(t) = &mut self.ws[self.cur].tree
        {
            t.toggle_parent(f);
        }
    }
    // --- resizing ---------------------------------------------------------
    /// Keyboard resize by pixels. Tiled windows move their split; floating ones change size.
    pub fn resize(&mut self, dw: f32, dh: f32) {
        if let Some(f) = self.focused() {
            self.resize_window(f, dw, dh);
        }
    }
    pub fn resize_window(&mut self, k: Kind, dw: f32, dh: f32) {
        if self.is_collapsed(k) {
            return;
        }
        if self.is_floating(k) {
            let mut r = self.rect_of(k).unwrap_or(self.default_float(0));
            r.w = (r.w + dw).max(MIN_W);
            r.h = (r.h + dh).max(MIN_H);
            self.float_rect.insert(k, r);
            return;
        }
        let area = self.usable.inset(GAP_OUT - GAP_IN);
        for (delta, axis) in [(dw, Axis::Horizontal), (dh, Axis::Vertical)] {
            if delta == 0.0 {
                continue;
            }
            let found = self.ws[self.cur]
                .tree
                .as_ref()
                .and_then(|t| t.find_split(area, k, axis));
            if let Some((id, in_a, len)) = found
                && let Some(t) = &mut self.ws[self.cur].tree
                && let Some(r) = t.ratio_of(id)
            {
                let d = delta / len.max(1.0);
                t.set_ratio(id, if in_a { r + d } else { r - d });
            }
        }
    }
    #[cfg(test)]
    fn set_ratio_for(&mut self, k: Kind, ratio: f32) {
        let area = self.usable.inset(GAP_OUT - GAP_IN);
        let found = self.ws[self.cur]
            .tree
            .as_ref()
            .and_then(|t| t.find_split(area, k, Axis::Horizontal));
        if let (Some((id, _, _)), Some(t)) = (found, &mut self.ws[self.cur].tree) {
            t.set_ratio(id, ratio);
        }
    }
    pub fn set_split(&mut self, split: u32, ratio: f32) {
        if let Some(t) = &mut self.ws[self.cur].tree {
            t.set_ratio(split, ratio);
        }
    }
    /// Split ratio that would put the gap under the pointer.
    #[cfg(test)]
    pub fn handle_ratio(&self, split: u32, pointer_x: f32, pointer_y: f32) -> Option<f32> {
        self.scene().handle_ratio(split, pointer_x, pointer_y)
    }

    // --- hit testing ----------------------------------------------------------
    #[cfg(test)]
    pub fn hit(&self, x: f32, y: f32) -> Hit {
        self.scene().hit(x, y)
    }
}

fn leaves_of(n: &Node, out: &mut Vec<Kind>) {
    match n {
        Node::Leaf(k) => out.push(*k),
        Node::Split { a, b, .. } => {
            leaves_of(a, out);
            leaves_of(b, out);
        }
    }
}

#[cfg(test)]
mod tests;
