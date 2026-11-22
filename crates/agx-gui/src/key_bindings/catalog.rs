use super::{Action, Binding, Trigger};
use crate::{
    keymap::{Cmd, ModKey, Mods, bind},
    wm::{Dir, Kind},
};

pub(super) struct Entry {
    pub id: String,
    pub label: String,
    pub action: Action,
    pub defaults: Vec<Binding>,
}

pub(super) fn entries(modifier: ModKey) -> Vec<Entry> {
    let mut entries = Vec::new();
    let main = match modifier {
        ModKey::Super => Mods::META,
        ModKey::Alt => Mods::ALT,
    };
    let fine = if main == Mods::META {
        Mods::ALT
    } else {
        Mods::CTRL
    };
    let coarse = if main == Mods::META {
        Mods::CTRL
    } else {
        Mods::META
    };
    let shift = Mods::SHIFT;
    let mut keys = |id: &str, label: &str, cmd, defaults: &[(&str, u8)]| {
        entries.push(Entry {
            id: id.into(),
            label: label.into(),
            action: Action::Command(cmd),
            defaults: defaults
                .iter()
                .filter(|(key, bits)| bind(key, Mods::from_bits(*bits), modifier) == Some(cmd))
                .map(|(key, bits)| Binding::key(key, *bits))
                .collect(),
        });
    };
    keys(
        "apps",
        "Open Applications",
        Cmd::Menu,
        &[("space", main), ("/", 0)],
    );
    keys("settings", "Open Settings", Cmd::Open(Kind::Settings), &[]);
    keys(
        "terminal",
        "Open Terminal",
        Cmd::Open(Kind::Terminal),
        &[("enter", main)],
    );
    keys("files", "Open Files", Cmd::Open(Kind::Files), &[]);
    keys("web", "Open Web", Cmd::Open(Kind::Web), &[]);
    for (id, kind) in [
        ("overview", Kind::Overview),
        ("network", Kind::Network),
        ("processor", Kind::Cpu),
        ("memory", Kind::Memory),
        ("hardware", Kind::Hardware),
        ("services", Kind::Services),
        ("system", Kind::System),
        ("agent", Kind::AgentAngi),
    ] {
        keys(id, &format!("Open {}", kind.title()), Cmd::Open(kind), &[]);
    }
    keys(
        "calendar",
        "Open Calendar",
        Cmd::Calendar,
        &[("f10", Mods::META)],
    );
    keys(
        "close",
        "Close window",
        Cmd::Close,
        &[("q", main), ("w", main)],
    );
    keys(
        "float",
        "Float or tile window",
        Cmd::ToggleFloat,
        &[("t", main)],
    );
    keys(
        "fullscreen",
        "Toggle fullscreen",
        Cmd::Fullscreen,
        &[("f", main)],
    );
    keys(
        "maximize",
        "Maximize window",
        Cmd::Maximize,
        &[("f", main | fine)],
    );
    keys(
        "split",
        "Toggle split direction",
        Cmd::ToggleSplit,
        &[("j", main)],
    );
    keys(
        "layout",
        "Toggle desktop layout",
        Cmd::CycleLayout,
        &[("l", main)],
    );
    keys(
        "next-window",
        "Next window",
        Cmd::CycleFocus(false),
        &[("tab", Mods::ALT)],
    );
    keys(
        "previous-window",
        "Previous window",
        Cmd::CycleFocus(true),
        &[("tab", Mods::ALT | shift)],
    );
    for (name, label, dir) in [
        ("left", "left", Dir::Left),
        ("right", "right", Dir::Right),
        ("up", "above", Dir::Up),
        ("down", "below", Dir::Down),
    ] {
        keys(
            &format!("focus-{name}"),
            &format!("Focus window {label}"),
            Cmd::Focus(dir),
            &[(name, main)],
        );
        keys(
            &format!("swap-{name}"),
            &format!("Swap with window {label}"),
            Cmd::Swap(dir),
            &[(name, main | shift)],
        );
    }
    keys(
        "next-workspace",
        "Next workspace",
        Cmd::NextWorkspace,
        &[("tab", main)],
    );
    keys(
        "previous-workspace",
        "Previous workspace",
        Cmd::PrevWorkspace,
        &[("tab", main | shift)],
    );
    for ws in 0..10 {
        let n = ws + 1;
        let digit = (n % 10).to_string();
        keys(
            &format!("workspace-{n}"),
            &format!("Switch to workspace {n}"),
            Cmd::Workspace(ws),
            &[(&digit, main), (&digit, 0)],
        );
        keys(
            &format!("follow-{n}"),
            &format!("Move window to workspace {n} and follow"),
            Cmd::MoveToWorkspace { ws, follow: true },
            &[(&digit, main | shift)],
        );
        keys(
            &format!("send-{n}"),
            &format!("Send window to workspace {n}"),
            Cmd::MoveToWorkspace { ws, follow: false },
            &[(&digit, main | shift | fine)],
        );
    }
    for (id, label, key, vertical, sign) in [
        ("narrow", "Narrow window", "-", false, -1.0),
        ("widen", "Widen window", "=", false, 1.0),
        ("shorten", "Shorten window", "-", true, -1.0),
        ("heighten", "Make window taller", "=", true, 1.0),
    ] {
        for (size, suffix, step, bits) in [
            ("normal", "", 100.0, 0),
            ("fine", " a little", 25.0, fine),
            ("coarse", " a lot", 300.0, coarse),
        ] {
            let (dw, dh) = if vertical {
                (0.0, sign * step)
            } else {
                (sign * step, 0.0)
            };
            keys(
                &format!("{id}-{size}"),
                &format!("{label}{suffix}"),
                Cmd::Resize { dw, dh },
                &[(key, main | bits | if vertical { shift } else { 0 })],
            );
        }
    }
    for (id, label, action, button) in [
        ("drag-move", "Drag window to move", Action::Move, 0),
        ("drag-resize", "Drag window to resize", Action::Resize, 1),
    ] {
        entries.push(Entry {
            id: id.into(),
            label: label.into(),
            action,
            defaults: vec![Binding {
                trigger: Trigger::Mouse(button),
                bits: main,
            }],
        });
    }
    entries
}
