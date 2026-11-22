//! Desktop shortcut lookup and saved per-account overrides.
mod catalog;
mod editor;

pub(super) use editor::install;
#[cfg(test)]
pub(crate) use editor::test_editor;
mod pointer;
use crate::keymap::{Cmd, ModKey, Mods, bind, unshift};
use catalog::Entry;
pub(super) use pointer::{attach_pointer, dispatch_pointer};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Action {
    Command(Cmd),
    Move,
    Resize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Device {
    Keyboard,
    Mouse,
}

impl Device {
    fn from_index(index: i32) -> Self {
        if index == 1 {
            Self::Mouse
        } else {
            Self::Keyboard
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Trigger {
    Key(String),
    Mouse(i32),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Binding {
    trigger: Trigger,
    bits: u8,
}

impl Binding {
    fn key(name: &str, bits: u8) -> Self {
        // Slint represents F1–F24 as a consecutive range of private-use characters.
        let lower = match name.chars().next() {
            Some(c @ '\u{f704}'..='\u{f71b}') => format!("f{}", c as u32 - 0xf704 + 1),
            _ => name.to_lowercase(),
        };
        Self {
            trigger: Trigger::Key(
                if bits & Mods::SHIFT != 0 {
                    unshift(&lower)
                } else {
                    &lower
                }
                .into(),
            ),
            bits,
        }
    }

    fn device(&self) -> Device {
        match self.trigger {
            Trigger::Key(_) => Device::Keyboard,
            Trigger::Mouse(_) => Device::Mouse,
        }
    }

    fn label(&self) -> String {
        let mut names = Vec::new();
        for (bit, name) in [
            (Mods::CTRL, "Ctrl"),
            (Mods::ALT, "Alt"),
            (Mods::SHIFT, "Shift"),
            (Mods::META, "Super"),
        ] {
            if self.bits & bit != 0 {
                names.push(name.to_owned());
            }
        }
        names.push(match &self.trigger {
            Trigger::Mouse(button) => match button {
                0 => "Left click".into(),
                1 => "Right click".into(),
                2 => "Middle click".into(),
                4 => "Back button".into(),
                5 => "Forward button".into(),
                _ => format!("Mouse button {button}"),
            },
            Trigger::Key(key) => match key.as_str() {
                "pageup" => "Page Up".into(),
                "pagedown" => "Page Down".into(),
                name if name.chars().count() == 1 => name.to_uppercase(),
                name => {
                    let mut chars = name.chars();
                    chars.next().map_or_else(String::new, |c| {
                        c.to_uppercase().collect::<String>() + chars.as_str()
                    })
                }
            },
        });
        names.join(" + ")
    }

    fn allowed(&self) -> bool {
        self.bits & !0x0f == 0
            && match &self.trigger {
                Trigger::Mouse(n) => matches!(n, 1 | 2 | 4 | 5) || (*n == 0 && self.bits != 0),
                Trigger::Key(key) => !key.is_empty() && !key.chars().any(char::is_control),
            }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Overrides {
    keyboard: BTreeMap<String, Vec<Binding>>,
    mouse: BTreeMap<String, Vec<Binding>>,
}

impl Overrides {
    fn for_device(&self, device: Device) -> &BTreeMap<String, Vec<Binding>> {
        match device {
            Device::Keyboard => &self.keyboard,
            Device::Mouse => &self.mouse,
        }
    }

    fn set(&mut self, device: Device, id: &str, bindings: Vec<Binding>) {
        match device {
            Device::Keyboard => &mut self.keyboard,
            Device::Mouse => &mut self.mouse,
        }
        .insert(id.into(), bindings);
    }
}

pub(super) struct Mappings {
    entries: Vec<Entry>,
    overrides: Overrides,
    path: Option<PathBuf>,
}

impl Mappings {
    pub(super) fn new(modifier: ModKey) -> Self {
        Self {
            entries: catalog::entries(modifier),
            overrides: Overrides::default(),
            path: crate::settings::config_file("key-bindings.json"),
        }
    }

    pub(super) fn load(&mut self) {
        let Some(path) = &self.path else { return };
        let result = std::fs::read_to_string(path)
            .and_then(|text| serde_json::from_str::<Overrides>(&text).map_err(io::Error::from));
        match result {
            Ok(mut overrides) => {
                for (device, entries) in [
                    (Device::Keyboard, &mut overrides.keyboard),
                    (Device::Mouse, &mut overrides.mouse),
                ] {
                    entries.retain(|id, bindings| {
                        self.entries.iter().any(|entry| entry.id == *id)
                            && bindings
                                .iter()
                                .all(|binding| binding.device() == device && binding.allowed())
                    });
                }
                self.overrides = overrides;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => eprintln!("agx-gui: key bindings: {error}"),
        }
    }

    fn bindings(&self, entry: &Entry, device: Device) -> Vec<Binding> {
        self.overrides
            .for_device(device)
            .get(&entry.id)
            .cloned()
            .unwrap_or_else(|| {
                entry
                    .defaults
                    .iter()
                    .filter(|binding| binding.device() == device)
                    .cloned()
                    .collect()
            })
    }

    fn override_owner(&self, binding: &Binding) -> Option<&Entry> {
        let overrides = self.overrides.for_device(binding.device());
        self.entries.iter().find(|entry| {
            overrides
                .get(&entry.id)
                .is_some_and(|set| set.contains(binding))
        })
    }

    fn default_owner(&self, binding: &Binding, modk: ModKey) -> Option<&Entry> {
        let overrides = self.overrides.for_device(binding.device());
        let action = match &binding.trigger {
            Trigger::Key(name) => {
                let cmd = bind(name, Mods::from_bits(binding.bits), modk)?;
                Action::Command(cmd)
            }
            Trigger::Mouse(button) if modk.held(Mods::from_bits(binding.bits)) => match button {
                0 => Action::Move,
                1 => Action::Resize,
                _ => return None,
            },
            Trigger::Mouse(_) => return None,
        };
        self.entries
            .iter()
            .find(|entry| entry.action == action && !overrides.contains_key(&entry.id))
    }

    fn owner(&self, binding: &Binding, modk: ModKey) -> Option<&Entry> {
        self.override_owner(binding)
            .or_else(|| self.default_owner(binding, modk))
    }

    pub(super) fn custom_key(&self, key: &str, mods: Mods) -> Option<Cmd> {
        match self.override_owner(&Binding::key(key, mods.bits()))?.action {
            Action::Command(cmd) => Some(cmd),
            _ => None,
        }
    }

    pub(super) fn default_key(&self, key: &str, mods: Mods, modk: ModKey) -> Option<Cmd> {
        match self
            .default_owner(&Binding::key(key, mods.bits()), modk)?
            .action
        {
            Action::Command(cmd) => Some(cmd),
            _ => None,
        }
    }

    pub(super) fn mouse(&self, button: i32, mods: Mods, modk: ModKey) -> Option<Action> {
        self.owner(
            &Binding {
                trigger: Trigger::Mouse(button),
                bits: mods.bits(),
            },
            modk,
        )
        .map(|entry| entry.action)
    }

    pub(super) fn hint(&self, cmd: Cmd) -> String {
        let Some(entry) = self
            .entries
            .iter()
            .find(|entry| entry.action == Action::Command(cmd))
        else {
            return String::new();
        };
        [Device::Keyboard, Device::Mouse]
            .into_iter()
            .find_map(|device| self.bindings(entry, device).first().map(Binding::label))
            .unwrap_or_default()
    }

    fn save(&mut self, next: Overrides) -> io::Result<()> {
        let path = self
            .path
            .as_deref()
            .ok_or_else(|| io::Error::other("Settings folder is unavailable"))?;
        crate::settings::write_private(path, &serde_json::to_string_pretty(&next)?)?;
        self.overrides = next;
        Ok(())
    }
}
