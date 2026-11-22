//! Desktop keybindings, expressed once and tested. Super is the main modifier;
//! AGX_GUI_MOD=alt remains an explicit override for nested desktop previews.
use crate::wm::{Dir, Kind};

#[derive(Clone, Copy, Debug, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

impl Mods {
    pub const CTRL: u8 = 1;
    pub const ALT: u8 = 2;
    pub const SHIFT: u8 = 4;
    pub const META: u8 = 8;

    pub fn bits(self) -> u8 {
        u8::from(self.ctrl) * Self::CTRL | u8::from(self.alt) * Self::ALT
            | u8::from(self.shift) * Self::SHIFT | u8::from(self.meta) * Self::META
    }

    pub fn from_bits(bits: u8) -> Self {
        Self {
            ctrl: bits & Self::CTRL != 0,
            alt: bits & Self::ALT != 0,
            shift: bits & Self::SHIFT != 0,
            meta: bits & Self::META != 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModKey {
    Super,
    Alt,
}
impl ModKey {
    pub fn from_env() -> Self {
        match std::env::var("AGX_GUI_MOD").ok().as_deref() {
            Some("super") => ModKey::Super,
            Some("alt") => ModKey::Alt,
            _ => ModKey::Super,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            ModKey::Super => "Super",
            ModKey::Alt => "Alt",
        }
    }
    pub fn held(self, m: Mods) -> bool {
        match self {
            ModKey::Super => m.meta,
            ModKey::Alt => m.alt,
        }
    }
    /// Omarchy's "a little" modifier (Alt with Super; Ctrl with Alt).
    fn fine(self, m: Mods) -> bool {
        match self {
            ModKey::Super => m.alt,
            ModKey::Alt => m.ctrl,
        }
    }
    /// Omarchy's "a lot" modifier (Ctrl with Super; Super with Alt).
    fn coarse(self, m: Mods) -> bool {
        match self {
            ModKey::Super => m.ctrl,
            ModKey::Alt => m.meta,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cmd {
    Focus(Dir),
    Swap(Dir),
    Close,
    ToggleSplit,
    ToggleFloat,
    Fullscreen,
    Maximize,
    CycleLayout,
    AddWorkspace,
    RemoveWorkspace(usize),
    Workspace(usize),
    MoveToWorkspace { ws: usize, follow: bool },
    NextWorkspace,
    PrevWorkspace,
    CycleFocus(bool),
    Resize { dw: f32, dh: f32 },
    Menu,
    Calendar,
    Open(Kind),
    OpenCode,
    /// Choose one entry of the ANGI software index: install it, update it, or open it once installed.
    Package(usize),
    Tui,
    Logout,
    RefreshDesktop,
    Reboot,
    Shutdown,
}

/// `key` is a normalised name: "left", "tab", "space", "-", "a", "1" ...
pub fn bind(key: &str, m: Mods, modk: ModKey) -> Option<Cmd> {
    if m.meta && key == "f10" {
        return Some(Cmd::Calendar);
    }
    let key = unshift(key);
    let digit = match key.as_bytes() {
        [b'0'] => Some(9),
        [digit @ b'1'..=b'9'] => Some(usize::from(digit - b'1')),
        _ => None,
    };
    if !modk.held(m) {
        // Convenience without a modifier; Alt+Tab is Omarchy's window cycle.
        if m.alt && key == "tab" {
            return Some(Cmd::CycleFocus(m.shift));
        }
        if m.ctrl || m.alt || m.meta {
            return None;
        }
        return match (key, digit) {
            ("/", _) => Some(Cmd::Menu),
            (_, Some(n)) if !m.shift => Some(Cmd::Workspace(n)),
            _ => None,
        };
    }
    let (fine, coarse) = (modk.fine(m), modk.coarse(m));
    let dir = match key {
        "left" => Some(Dir::Left),
        "right" => Some(Dir::Right),
        "up" => Some(Dir::Up),
        "down" => Some(Dir::Down),
        _ => None,
    };
    if let Some(d) = dir {
        return Some(if m.shift { Cmd::Swap(d) } else { Cmd::Focus(d) });
    }
    if let Some(n) = digit {
        return Some(if m.shift {
            Cmd::MoveToWorkspace { ws: n, follow: !fine }
        } else {
            Cmd::Workspace(n)
        });
    }
    let step = if fine { 25.0 } else if coarse { 300.0 } else { 100.0 };
    match key {
        "q" | "w" => Some(Cmd::Close),
        "j" => Some(Cmd::ToggleSplit),
        "t" => Some(Cmd::ToggleFloat),
        "f" if fine => Some(Cmd::Maximize),
        "f" => Some(Cmd::Fullscreen),
        "l" => Some(Cmd::CycleLayout),
        "tab" => Some(if m.shift { Cmd::PrevWorkspace } else { Cmd::NextWorkspace }),
        "space" => Some(Cmd::Menu),
        "enter" => Some(Cmd::Open(Kind::Terminal)),
        // Omarchy binds the keys left of backspace: width, and with Shift height.
        "-" => Some(if m.shift { Cmd::Resize { dw: 0.0, dh: -step } } else { Cmd::Resize { dw: -step, dh: 0.0 } }),
        "=" => Some(if m.shift { Cmd::Resize { dw: 0.0, dh: step } } else { Cmd::Resize { dw: step, dh: 0.0 } }),
        _ => None,
    }
}

/// Shift changes the text a key produces; Omarchy binds the physical key.
pub(super) fn unshift(key: &str) -> &str {
    match key {
        "_" => "-",
        "+" => "=",
        "!" => "1",
        "@" => "2",
        "#" => "3",
        "$" => "4",
        "%" => "5",
        "^" => "6",
        "&" => "7",
        "*" => "8",
        "(" => "9",
        ")" => "0",
        k => k,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const S: ModKey = ModKey::Super;
    fn m(meta: bool, alt: bool, ctrl: bool, shift: bool) -> Mods {
        Mods { meta, alt, ctrl, shift }
    }
    #[test]
    fn omarchy_focus_and_swap() {
        assert_eq!(bind("left", m(true, false, false, false), S), Some(Cmd::Focus(Dir::Left)));
        assert_eq!(bind("down", m(true, false, false, true), S), Some(Cmd::Swap(Dir::Down)));
    }
    #[test]
    fn omarchy_window_state_keys() {
        let sup = m(true, false, false, false);
        assert_eq!(bind("q", sup, S), Some(Cmd::Close));
        assert_eq!(bind("w", sup, S), Some(Cmd::Close));
        assert_eq!(bind("j", sup, S), Some(Cmd::ToggleSplit));
        assert_eq!(bind("t", sup, S), Some(Cmd::ToggleFloat));
        assert_eq!(bind("f", sup, S), Some(Cmd::Fullscreen));
        assert_eq!(bind("f", m(true, true, false, false), S), Some(Cmd::Maximize));
        assert_eq!(bind("l", sup, S), Some(Cmd::CycleLayout));
    }
    #[test]
    fn workspaces_follow_super_digits() {
        assert_eq!(bind("2", m(true, false, false, false), S), Some(Cmd::Workspace(1)));
        assert_eq!(bind("@", m(true, false, false, true), S), Some(Cmd::MoveToWorkspace { ws: 1, follow: true }));
        assert_eq!(bind("@", m(true, true, false, true), S), Some(Cmd::MoveToWorkspace { ws: 1, follow: false }));
        assert_eq!(bind("tab", m(true, false, false, false), S), Some(Cmd::NextWorkspace));
        assert_eq!(bind("tab", m(true, false, false, true), S), Some(Cmd::PrevWorkspace));
        assert_eq!(bind("tab", m(false, true, false, false), S), Some(Cmd::CycleFocus(false)));
        assert_eq!(bind("9", m(true, false, false, false), S), Some(Cmd::Workspace(8)));
        assert_eq!(bind("^", m(true, false, false, true), S), Some(Cmd::MoveToWorkspace { ws: 5, follow: true }));
        assert_eq!(bind(")", m(true, false, false, true), S), Some(Cmd::MoveToWorkspace { ws: 9, follow: true }));
    }
    #[test]
    fn resize_steps_match_omarchy() {
        let step = |mods, key| match bind(key, mods, S) {
            Some(Cmd::Resize { dw, dh }) => (dw, dh),
            other => panic!("{other:?}"),
        };
        assert_eq!(step(m(true, false, false, false), "-"), (-100.0, 0.0));
        assert_eq!(step(m(true, false, false, false), "="), (100.0, 0.0));
        assert_eq!(step(m(true, false, false, true), "+"), (0.0, 100.0));
        assert_eq!(step(m(true, true, false, false), "="), (25.0, 0.0));
        assert_eq!(step(m(true, false, true, false), "="), (300.0, 0.0));
    }
    #[test]
    fn super_space_opens_apps_and_alt_requires_an_explicit_override() {
        assert_eq!(bind("space", m(true, false, false, false), S), Some(Cmd::Menu));
        assert_eq!(bind("enter", m(true, false, false, false), S), Some(Cmd::Open(Kind::Terminal)));
        assert_eq!(bind("space", m(false, true, false, false), S), None);
        assert_eq!(bind("q", m(false, false, false, false), S), None);
        assert_eq!(bind("3", m(false, false, false, false), S), Some(Cmd::Workspace(2)));
        assert_eq!(bind("/", m(false, false, false, false), S), Some(Cmd::Menu));
        assert_eq!(bind("q", m(false, true, false, false), ModKey::Alt), Some(Cmd::Close));
        assert_eq!(bind("=", m(false, true, true, false), ModKey::Alt), Some(Cmd::Resize { dw: 25.0, dh: 0.0 }));
    }
}
