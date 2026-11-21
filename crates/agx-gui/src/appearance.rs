//! Per-account appearance: the Pebble Line light or dark theme.
//!
//! The choice lives in `~/.config/agx/appearance.conf` as `theme=dark` or `theme=light`, and drives
//! `Theme.dark` in `ui/pebble/tokens.slint`. Every colour token switches with it, so the change is
//! live with no restart.
use crate::{AppWindow, AppearanceSettings, Theme};
use slint::ComponentHandle;
use std::{fs, io};

const FILE: &str = "appearance.conf";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Mode {
    #[default]
    Dark,
    Light,
}
impl Mode {
    fn parse(text: &str) -> io::Result<Self> {
        let mut mode = Self::default();
        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            match line.split_once('=') {
                Some(("theme", "dark")) => mode = Self::Dark,
                Some(("theme", "light")) => mode = Self::Light,
                Some(("theme", _)) => return Err(io::Error::other("Unknown theme")),
                _ => return Err(io::Error::other("Unknown appearance preference")),
            }
        }
        Ok(mode)
    }
    fn encode(self) -> String {
        format!(
            "theme={}\n",
            if self == Self::Dark { "dark" } else { "light" }
        )
    }
}

fn load() -> Mode {
    let Some(path) = crate::settings::config_file(FILE) else {
        return Mode::default();
    };
    match fs::read_to_string(path) {
        Ok(text) => Mode::parse(&text).unwrap_or_else(|error| {
            eprintln!("agx-gui: appearance: {error}; using dark");
            Mode::default()
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Mode::default(),
        Err(error) => {
            eprintln!("agx-gui: appearance: {error}");
            Mode::default()
        }
    }
}

/// Apply the saved theme and save it again whenever Settings changes it.
pub fn install(ui: &AppWindow) {
    ui.global::<Theme>().set_dark(load() == Mode::Dark);
    let weak = ui.as_weak();
    ui.global::<AppearanceSettings>().on_changed(move || {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        let mode = if ui.global::<Theme>().get_dark() {
            Mode::Dark
        } else {
            Mode::Light
        };
        let status = match crate::settings::config_file(FILE)
            .ok_or_else(|| io::Error::other("Home directory unavailable"))
            .and_then(|path| crate::settings::write_private(&path, &mode.encode()))
        {
            Ok(()) => "Saved for your account".to_owned(),
            Err(error) => format!("Applied for this session. Could not save: {error}"),
        };
        ui.global::<AppearanceSettings>().set_status(status.into());
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn theme_round_trips_and_rejects_unknown_values() {
        for mode in [Mode::Dark, Mode::Light] {
            assert_eq!(Mode::parse(&mode.encode()).unwrap(), mode);
        }
        assert_eq!(Mode::parse("").unwrap(), Mode::Dark);
        for text in ["theme=blue", "theme", "accent=red"] {
            assert!(Mode::parse(text).is_err(), "{text}");
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
