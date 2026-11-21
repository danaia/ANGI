//! Per-account mouse preferences, live native configuration and durable storage.
use crate::{AppWindow, MouseSettings};
use slint::ComponentHandle;
use std::{
    cell::RefCell,
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq)]
struct Mouse {
    speed: i32,
    adaptive: bool,
    left: bool,
    natural: bool,
    scroll: i32,
}
impl Default for Mouse {
    fn default() -> Self {
        Self {
            speed: 0,
            adaptive: true,
            left: false,
            natural: false,
            scroll: 100,
        }
    }
}
impl Mouse {
    fn parse(text: &str) -> io::Result<Self> {
        let mut mouse = Self::default();
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| io::Error::other("Invalid mouse preference"))?;
            match key {
                "speed" => {
                    mouse.speed = value
                        .parse()
                        .ok()
                        .filter(|v| (-100..=100).contains(v))
                        .ok_or_else(|| io::Error::other("Invalid pointer speed"))?
                }
                "adaptive" => mouse.adaptive = parse_bool(value)?,
                "left" => mouse.left = parse_bool(value)?,
                "natural" => mouse.natural = parse_bool(value)?,
                "scroll" => {
                    mouse.scroll = value
                        .parse()
                        .ok()
                        .filter(|v| (25..=300).contains(v))
                        .ok_or_else(|| io::Error::other("Invalid scroll speed"))?
                }
                _ => return Err(io::Error::other("Unknown mouse preference")),
            }
        }
        Ok(mouse)
    }
    fn encode(&self) -> String {
        format!(
            "speed={}\nadaptive={}\nleft={}\nnatural={}\nscroll={}\n",
            self.speed, self.adaptive, self.left, self.natural, self.scroll
        )
    }
}
fn parse_bool(value: &str) -> io::Result<bool> {
    value
        .parse()
        .map_err(|_| io::Error::other("Invalid mouse preference switch"))
}
thread_local! {
    static MOUSE: RefCell<Mouse> = RefCell::new(Mouse::default());
    #[cfg(feature = "kms")]
    static DEVICES: RefCell<Vec<input::Device>> = const { RefCell::new(Vec::new()) };
}
fn path() -> Option<PathBuf> {
    config_file("mouse.conf")
}
/// A file in the account's ANGI configuration directory (`$XDG_CONFIG_HOME/agx` or `~/.config/agx`).
pub(crate) fn config_file(name: &str) -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|p| Path::new(p).is_absolute())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .map(|base| base.join("agx").join(name))
}
pub fn initialize() {
    if let Some(path) = path() {
        match fs::read_to_string(path) {
            Ok(text) => match Mouse::parse(&text) {
                Ok(mouse) => MOUSE.with(|state| *state.borrow_mut() = mouse),
                Err(error) => eprintln!("agx-gui: mouse settings: {error}; using defaults"),
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => eprintln!("agx-gui: mouse settings: {error}"),
        }
    }
}
fn save(path: &Path, mouse: &Mouse) -> io::Result<()> {
    write_private(path, &mouse.encode())
}
/// Atomically replace a per-account preference file, readable only by its owner.
pub(crate) fn write_private(path: &Path, contents: &str) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    fs::create_dir_all(
        path.parent()
            .ok_or_else(|| io::Error::other("Missing settings directory"))?,
    )?;
    let temporary = path.with_extension(format!("conf.{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
fn populate(ui: &AppWindow, mouse: &Mouse) {
    let settings = ui.global::<MouseSettings>();
    settings.set_speed(mouse.speed as f32);
    settings.set_adaptive(mouse.adaptive);
    settings.set_left_handed(mouse.left);
    settings.set_natural(mouse.natural);
    settings.set_scroll_speed(mouse.scroll as f32);
}
pub fn install(ui: &AppWindow) -> std::rc::Rc<slint::Timer> {
    let save_timer = std::rc::Rc::new(slint::Timer::default());
    let pending = save_timer.clone();
    MOUSE.with(|mouse| populate(ui, &mouse.borrow()));
    ui.global::<MouseSettings>()
        .set_native(cfg!(feature = "kms"));
    let weak = ui.as_weak();
    ui.global::<MouseSettings>().on_changed(move || {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        let settings = ui.global::<MouseSettings>();
        let mouse = Mouse {
            speed: settings.get_speed().round().clamp(-100., 100.) as i32,
            adaptive: settings.get_adaptive(),
            left: settings.get_left_handed(),
            natural: settings.get_natural(),
            scroll: settings.get_scroll_speed().round().clamp(25., 300.) as i32,
        };
        MOUSE.with(|state| *state.borrow_mut() = mouse.clone());
        #[cfg(feature = "kms")]
        DEVICES.with(|devices| {
            for device in devices.borrow_mut().iter_mut() {
                configure(device, &mouse);
            }
        });
        settings.set_status("Applied · saving…".into());
        // Keep synchronous filesystem work out of the slider's motion path.
        let weak = ui.as_weak();
        pending.start(
            slint::TimerMode::SingleShot,
            std::time::Duration::from_millis(350),
            move || {
                let status = match path()
                    .ok_or_else(|| io::Error::other("Home directory unavailable"))
                    .and_then(|path| save(&path, &mouse))
                {
                    Ok(()) => "Saved · changes apply immediately".to_owned(),
                    Err(error) => format!("Applied for this session. Could not save: {error}"),
                };
                if let Some(ui) = weak.upgrade() {
                    ui.global::<MouseSettings>().set_status(status.into());
                }
            },
        );
    });
    let weak = ui.as_weak();
    ui.global::<MouseSettings>().on_reset(move || {
        if let Some(ui) = weak.upgrade() {
            populate(&ui, &Mouse::default());
            ui.global::<MouseSettings>().invoke_changed();
        }
    });
    save_timer
}
pub(super) fn flush() -> io::Result<()> {
    let path = path().ok_or_else(|| io::Error::other("Home directory unavailable"))?;
    MOUSE.with(|mouse| save(&path, &mouse.borrow()))
}

pub fn scroll_factor() -> f64 {
    MOUSE.with(|mouse| mouse.borrow().scroll as f64 / 100.)
}

#[cfg(feature = "kms")]
fn configure(device: &mut input::Device, mouse: &Mouse) {
    use input::AccelProfile;
    if device.config_accel_is_available() {
        let _ = device.config_accel_set_speed(mouse.speed as f64 / 100.);
        let profile = if mouse.adaptive {
            AccelProfile::Adaptive
        } else {
            AccelProfile::Flat
        };
        if device.config_accel_profiles().contains(&profile) {
            let _ = device.config_accel_set_profile(profile);
        }
    }
    if device.config_left_handed_is_available() {
        let _ = device.config_left_handed_set(mouse.left);
    }
    if device.config_scroll_has_natural_scroll() {
        let _ = device.config_scroll_set_natural_scroll_enabled(mouse.natural);
    }
}
#[cfg(feature = "kms")]
pub fn device_event(event: &input::Event) {
    use input::{DeviceCapability, event::DeviceEvent, event::EventTrait};
    match event {
        input::Event::Device(DeviceEvent::Added(event)) => {
            let mut device = event.device();
            if !device.has_capability(DeviceCapability::Pointer) {
                return;
            }
            MOUSE.with(|mouse| configure(&mut device, &mouse.borrow()));
            DEVICES.with(|devices| devices.borrow_mut().push(device));
        }
        input::Event::Device(DeviceEvent::Removed(event)) => {
            DEVICES.with(|devices| {
                devices
                    .borrow_mut()
                    .retain(|device| *device != event.device())
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferences_round_trip_and_reject_invalid_values() {
        let mouse = Mouse {
            speed: 65,
            adaptive: false,
            left: true,
            natural: true,
            scroll: 175,
        };
        assert_eq!(Mouse::parse(&mouse.encode()).unwrap(), mouse);
        for text in [
            "speed=101",
            "speed=-101",
            "speed=NaN",
            "scroll=0",
            "scroll=301",
            "left=yes",
            "unknown=true",
            "speed",
        ] {
            assert!(Mouse::parse(text).is_err(), "{text}");
        }
    }
    #[test]
    fn saved_preferences_replace_existing_file() {
        let directory = std::env::temp_dir().join(format!("agx-mouse-test-{}", std::process::id()));
        let path = directory.join("mouse.conf");
        save(&path, &Mouse::default()).unwrap();
        let mouse = Mouse {
            speed: 75,
            ..Default::default()
        };
        save(&path, &mouse).unwrap();
        assert_eq!(
            Mouse::parse(&fs::read_to_string(&path).unwrap()).unwrap(),
            mouse
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
