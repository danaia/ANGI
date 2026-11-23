//! Per-account appearance: the Pebble Line light or dark theme, and the night light filter.
//!
//! The choice lives in `~/.config/agx/appearance.conf` as `theme=dark|light`, `night-light=on|off`
//! and `night-warmth=0..100`. The theme drives `Theme.dark` in `ui/pebble/tokens.slint`, so every
//! colour token switches live; the night light drives `NightLight` in `ui/night-light.slint`, the
//! warm filter laid over the whole desktop.
use crate::{AppWindow, AppearanceSettings, NightLight, Theme};
use slint::ComponentHandle;
use std::{fs, io};

const FILE: &str = "appearance.conf";
const DEFAULT_WARMTH: u8 = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Appearance {
    dark: bool,
    night_light: bool,
    /// 0 is the gentlest glow, 100 the deepest night.
    warmth: u8,
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            dark: true,
            night_light: false,
            warmth: DEFAULT_WARMTH,
        }
    }
}
impl Appearance {
    fn parse(text: &str) -> io::Result<Self> {
        let mut prefs = Self::default();
        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            match line.split_once('=') {
                Some(("theme", "dark")) => prefs.dark = true,
                Some(("theme", "light")) => prefs.dark = false,
                Some(("theme", _)) => return Err(io::Error::other("Unknown theme")),
                Some(("night-light", "on")) => prefs.night_light = true,
                Some(("night-light", "off")) => prefs.night_light = false,
                Some(("night-light", _)) => return Err(io::Error::other("Unknown night light")),
                Some(("night-warmth", value)) => {
                    prefs.warmth = value
                        .parse()
                        .ok()
                        .filter(|warmth| *warmth <= 100)
                        .ok_or_else(|| io::Error::other("Night warmth must be 0 to 100"))?;
                }
                _ => return Err(io::Error::other("Unknown appearance preference")),
            }
        }
        Ok(prefs)
    }
    fn encode(self) -> String {
        format!(
            "theme={}\nnight-light={}\nnight-warmth={}\n",
            if self.dark { "dark" } else { "light" },
            if self.night_light { "on" } else { "off" },
            self.warmth
        )
    }
    fn read(ui: &AppWindow) -> Self {
        let night = ui.global::<NightLight>();
        Self {
            dark: ui.global::<Theme>().get_dark(),
            night_light: night.get_enabled(),
            warmth: night.get_warmth().round().clamp(0.0, 100.0) as u8,
        }
    }
    fn apply(self, ui: &AppWindow) {
        crate::night_light::set(self.night_light, f32::from(self.warmth));
        ui.global::<Theme>().set_dark(self.dark);
        let night = ui.global::<NightLight>();
        night.set_enabled(self.night_light);
        night.set_warmth(f32::from(self.warmth));
    }
}

fn load() -> Appearance {
    let Some(path) = crate::settings::config_file(FILE) else {
        return Appearance::default();
    };
    match fs::read_to_string(path) {
        Ok(text) => Appearance::parse(&text).unwrap_or_else(|error| {
            eprintln!("agx-gui: appearance: {error}; using defaults");
            Appearance::default()
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Appearance::default(),
        Err(error) => {
            eprintln!("agx-gui: appearance: {error}");
            Appearance::default()
        }
    }
}

/// Apply the saved appearance and save it again whenever Settings changes the theme or night light.
pub fn install(ui: &AppWindow, theme_changed: impl Fn(bool) + 'static) {
    if std::env::var("SLINT_BACKEND").as_deref() == Ok("vnc-software") {
        crate::night_light::use_software_frames(ui);
    }
    crate::night_light::install_preview(ui);
    load().apply(ui);
    theme_changed(ui.global::<Theme>().get_dark());
    let save = {
        let weak = ui.as_weak();
        let theme_changed = std::rc::Rc::new(theme_changed);
        move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let prefs = Appearance::read(&ui);
            theme_changed(prefs.dark);
            // The slider commits on release, so this is the only time the screen is re-filtered.
            crate::night_light::set(prefs.night_light, f32::from(prefs.warmth));
            ui.window().request_redraw();
            let status = match crate::settings::config_file(FILE)
                .ok_or_else(|| io::Error::other("Home directory unavailable"))
                .and_then(|path| {
                    crate::settings::write_private(&path, &prefs.encode())
                }) {
                Ok(()) => "Saved for your account".to_owned(),
                Err(error) => format!("Applied for this session. Could not save: {error}"),
            };
            ui.global::<AppearanceSettings>().set_status(status.into());
        }
    };
    ui.global::<AppearanceSettings>().on_changed(save.clone());
    ui.global::<NightLight>().on_changed(save);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn appearance_round_trips_and_rejects_unknown_values() {
        for dark in [true, false] {
            for (night_light, warmth) in [(false, 0), (true, 55), (true, 100)] {
                let prefs = Appearance {
                    dark,
                    night_light,
                    warmth,
                };
                assert_eq!(Appearance::parse(&prefs.encode()).unwrap(), prefs);
            }
        }
        assert_eq!(Appearance::parse("").unwrap(), Appearance::default());
        // Files written before night light existed keep their theme and get the default filter.
        let older = Appearance::parse("theme=light\n").unwrap();
        assert!(!older.dark && !older.night_light && older.warmth == DEFAULT_WARMTH);
        for text in [
            "theme=blue",
            "theme",
            "accent=red",
            "night-light=maybe",
            "night-warmth=101",
            "night-warmth=-5",
            "night-warmth=warm",
        ] {
            assert!(Appearance::parse(text).is_err(), "{text}");
        }
    }

    /// Every hex colour token in brand/pebble-line/tokens.json has the same light and dark values in
    /// the Slint `Theme`, so the desktop and web apps cannot drift apart.
    #[test]
    fn slint_theme_matches_the_pebble_line_tokens() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let json: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(root.join("../../brand/pebble-line/tokens.json")).unwrap(),
        )
        .unwrap();
        let slint = fs::read_to_string(root.join("ui/pebble/tokens.slint")).unwrap();
        let mut checked = 0;
        for token in json["color"]["tokens"].as_array().unwrap() {
            let (Some(light), Some(dark)) = (
                token["value"]["light"].as_str(),
                token["value"]["dark"].as_str(),
            ) else {
                continue;
            };
            if !light.starts_with('#') {
                continue; // rgba() scrim; tokens.slint writes it as 8-digit hex.
            }
            let name = token["name"].as_str().unwrap();
            let line = format!("out property <color> {name}: dark ? {dark} : {light};");
            assert!(
                slint.contains(&line),
                "tokens.slint is missing or differs: {line}"
            );
            checked += 1;
        }
        assert!(
            checked >= 24,
            "expected the full Pebble Line palette, checked {checked}"
        );
    }

    /// Feature files take colours from `Theme`; hex values belong in the token file and the few
    /// documented exceptions (docs/design-system.md, rule 1).
    #[test]
    fn feature_ui_uses_theme_tokens_not_hex_colours() {
        let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
        let allowed = [
            "cursor.slint",
            "cursor-preview.slint",
            "settings-appearance.slint",
            "night-light.slint",
        ];
        let mut offenders = Vec::new();
        for entry in fs::read_dir(&ui).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if !name.ends_with(".slint") || allowed.contains(&name.as_str()) {
                continue;
            }
            let text = fs::read_to_string(&path).unwrap();
            for (number, line) in text.lines().enumerate() {
                let hex = line.match_indices('#').any(|(i, _)| {
                    let digits = line[i + 1..]
                        .chars()
                        .take_while(char::is_ascii_hexdigit)
                        .count();
                    digits == 6 || digits == 8
                });
                if hex {
                    offenders.push(format!("{name}:{}", number + 1));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "hex colours outside ui/pebble/tokens.slint: {offenders:?}"
        );
    }
}
