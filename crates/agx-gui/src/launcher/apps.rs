//! The searchable index of software ANGI can install.
//!
//! The list is embedded so search works offline. The installer selects either an ARM64 release
//! artifact or Buildroot packages for the measured device, and publishes compatibility to the launcher.
use serde_json::Value;
use std::sync::OnceLock;

pub(super) struct Package {
    /// Stable id the installer knows the app by.
    pub id: String,
    pub name: String,
    pub summary: String,
    /// Desktop, Terminal, or Server.
    pub kind: String,
    pub aliases: String,
    /// The commands it adds, such as `rg` for ripgrep.
    pub commands: Vec<String>,
    /// The program to launch once installed. An empty value opens a shell.
    pub launch: String,
    /// Shown with a Recommended badge: broadly useful, small, and checked on this device's architecture.
    pub recommended: bool,
    pub native: bool,
}

fn parse(text: &str) -> Vec<Package> {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let field = |item: &Value, key: &str| item[key].as_str().unwrap_or_default().to_string();
    items
        .iter()
        .map(|item| Package {
            id: field(item, "id"),
            name: field(item, "name"),
            summary: field(item, "summary"),
            kind: field(item, "kind"),
            aliases: field(item, "aliases"),
            commands: item["bins"]
                .as_array()
                .map(|bins| bins.iter().filter_map(|bin| bin["name"].as_str().map(String::from)).collect())
                .unwrap_or_default(),
            launch: field(item, "launch"),
            recommended: item["recommended"].as_bool().unwrap_or(false),
            native: item["native"].as_bool().unwrap_or(false),
        })
        .filter(|app| {
            !app.id.is_empty()
                && !app.name.is_empty()
                && !app.commands.is_empty()
                && app.id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
        .collect()
}

/// The whole index in order of usefulness. Indexes into it are stable for the life of the process.
pub(super) fn index() -> &'static [Package] {
    static INDEX: OnceLock<Vec<Package>> = OnceLock::new();
    INDEX.get_or_init(|| parse(include_str!("../../../../apps/agent/apps.json")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_has_the_headline_apps_each_with_a_command_to_run() {
        let names: Vec<_> = index().iter().map(|app| app.name.as_str()).collect();
        for name in ["GitHub CLI", "ripgrep", "btop", "Neovim", "Syncthing", "AdGuard Home"] {
            assert!(names.contains(&name), "{name} is missing from the index");
        }
        assert!(index().len() >= 45, "a malformed file must not silently empty the index");
        let ripgrep = index().iter().find(|app| app.id == "ripgrep").unwrap();
        assert_eq!(ripgrep.commands, ["rg"]);
        assert!(ripgrep.recommended);
        let recommended = index().iter().filter(|app| app.recommended).count();
        assert!((5..=12).contains(&recommended), "a badge only means something when it is rare: {recommended}");
    }

    #[test]
    fn every_entry_is_complete_unique_and_launchable() {
        let Value::Array(raw) =
            serde_json::from_str::<Value>(include_str!("../../../../apps/agent/apps.json")).unwrap()
        else {
            panic!("the index must be a JSON array");
        };
        assert_eq!(raw.len(), index().len(), "an entry was dropped by validation");
        let mut seen = std::collections::HashSet::new();
        for app in index() {
            assert!(!app.summary.is_empty(), "{} is incomplete", app.name);
            assert!(matches!(app.kind.as_str(), "Desktop" | "Terminal" | "Server"), "{} has no kind", app.name);
            assert!(seen.insert(app.id.clone()), "{} is listed twice", app.id);
            assert!(
                app.launch.is_empty() || app.commands.contains(&app.launch),
                "{} launches a command it does not install",
                app.name
            );
        }
    }

    #[test]
    fn malformed_entries_are_dropped() {
        let apps = parse(
            r#"[{"id":"ok","name":"Ok","summary":"s","kind":"Terminal","bins":[{"name":"ok"}]},
                {"id":"Bad Id","name":"Bad","summary":"s","kind":"Terminal","bins":[{"name":"x"}]},
                {"id":"nobins","name":"None","summary":"s","kind":"Terminal","bins":[]},
                {"id":"","name":"Empty","summary":"s","kind":"Terminal","bins":[{"name":"x"}]}]"#,
        );
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].id, "ok");
        assert!(parse("not json").is_empty());
    }
}
