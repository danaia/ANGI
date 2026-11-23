//! Show a workstation build suggestion while a native Pi build is running.
use crate::AppWindow;
use slint::{ComponentHandle, Timer, TimerMode};
use std::{collections::HashMap, fs, path::Path, time::Duration};

fn raspberry_pi(model: &str, compatible: &str) -> bool {
    model.trim_start().starts_with("Raspberry Pi")
        || compatible
            .split('\0')
            .any(|board| board.starts_with("raspberrypi,"))
}

fn process_token(stat: &str) -> Option<u64> {
    let (_, fields) = stat.rsplit_once(") ")?;
    let mut fields = fields.split_whitespace();
    if fields.next()? == "Z" {
        return None;
    }
    fields.nth(18)?.parse().ok()
}

fn active_builds(directory: &Path) -> HashMap<u32, u64> {
    let Ok(entries) = fs::read_dir(directory) else {
        return HashMap::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let pid = entry.file_name().to_str()?.parse::<u32>().ok()?;
            let token = fs::read_to_string(entry.path()).ok()?.trim().parse().ok()?;
            let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
            (process_token(&stat) == Some(token)).then_some((pid, token))
        })
        .collect()
}

pub(super) fn install(ui: &AppWindow) -> Option<Timer> {
    let model = fs::read_to_string("/proc/device-tree/model").unwrap_or_default();
    let compatible = fs::read_to_string("/proc/device-tree/compatible").unwrap_or_default();
    if !raspberry_pi(&model, &compatible) {
        return None;
    }
    let uid = unsafe { libc::geteuid() };
    let path = format!("/var/lib/agx-data/agx-opencode-users/{uid}/build-notices");
    let weak = ui.as_weak();
    let mut previous = HashMap::new();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_secs(1), move || {
        let Some(ui) = weak.upgrade() else { return };
        let active = active_builds(Path::new(&path));
        if active.is_empty() && !previous.is_empty() {
            ui.set_build_notice_open(false);
        } else if active
            .iter()
            .any(|(pid, token)| previous.get(pid) != Some(token))
        {
            ui.set_build_notice_open(true);
        }
        previous = active;
    });
    Some(timer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_pi_models_and_device_tree_compatibility() {
        for model in [
            "Raspberry Pi 5 Model B\0",
            "Raspberry Pi 4 Model B\0",
            "Raspberry Pi Zero 2 W\0",
        ] {
            assert!(raspberry_pi(model, ""));
        }
        assert!(raspberry_pi(
            "",
            "vendor,board\0raspberrypi,compute-module-4\0"
        ));
        assert!(!raspberry_pi("AMD desktop", ""));
    }

    #[test]
    fn finds_live_builds_and_ignores_stale_process_tokens() {
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("angi-build-notice-test-{pid}"));
        fs::create_dir(&dir).unwrap();
        let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
        let token = process_token(&stat).unwrap();
        let marker = dir.join(pid.to_string());
        fs::write(&marker, token.to_string()).unwrap();
        assert_eq!(active_builds(&dir).get(&pid), Some(&token));
        fs::write(&marker, (token + 1).to_string()).unwrap();
        assert!(active_builds(&dir).is_empty());
        fs::remove_dir_all(dir).unwrap();
    }
}
