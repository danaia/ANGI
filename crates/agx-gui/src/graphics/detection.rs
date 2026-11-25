//! Startup display discovery. Kernel-forced connectors need real monitor EDID evidence.
use std::{fs, io, path::Path};

fn forced_output(command_line: &str, output: &str) -> bool {
    command_line.split_whitespace().any(|argument| {
        let Some(video) = argument.strip_prefix("video=") else {
            return false;
        };
        let Some((connector, mode)) = video.split_once(':') else {
            return false;
        };
        connector == output && (mode.ends_with('D') || mode.ends_with('e'))
    })
}

fn physical_connector(name: &str) -> bool {
    let Some((kind, number)) = name.rsplit_once('-') else {
        return false;
    };
    !number.is_empty()
        && number.bytes().all(|byte| byte.is_ascii_digit())
        && matches!(
            kind,
            "HDMI-A"
                | "HDMI-B"
                | "DP"
                | "eDP"
                | "DVI-I"
                | "DVI-D"
                | "DVI-A"
                | "VGA"
                | "LVDS"
                | "DSI"
                | "DPI"
                | "USB"
                | "SPI"
        )
}

/// Select a connected physical output, independent of DRM card numbering.
/// A forced `video=...D` reports connected without a cable, so do not trust it alone.
pub(super) fn connected_output(
    directory: &Path,
    command_line: &str,
    requested: Option<&str>,
) -> io::Result<Option<String>> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut outputs = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some((card, output)) = name.split_once('-') else {
            continue;
        };
        if !card.strip_prefix("card").is_some_and(|number| {
            !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
        }) || !physical_connector(output)
            || requested.is_some_and(|selected| selected != output)
        {
            continue;
        }
        if fs::read_to_string(entry.path().join("status"))
            .unwrap_or_default()
            .trim()
            != "connected"
        {
            continue;
        }
        if forced_output(command_line, output)
            && !fs::read(entry.path().join("edid"))
                .is_ok_and(|edid| edid.starts_with(&[0, 255, 255, 255, 255, 255, 255, 0]))
        {
            continue;
        }
        outputs.push(output.to_owned());
    }
    // Prefer an internal display on laptops. Otherwise keep the existing HDMI
    // choice when a desktop has both HDMI and another external connector.
    outputs.sort_by_key(|output| {
        let priority = if output.starts_with("eDP-") || output.starts_with("LVDS-") {
            0
        } else if output.starts_with("HDMI-") {
            1
        } else {
            2
        };
        (priority, output.clone())
    });
    Ok(outputs.into_iter().next())
}

#[cfg(feature = "kms")]
pub(super) fn at_startup() -> io::Result<Option<String>> {
    use std::time::{Duration, Instant};
    let command_line = fs::read_to_string("/proc/cmdline")?;
    let requested = std::env::var("SLINT_DRM_OUTPUT").ok();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let output = connected_output(
            Path::new("/sys/class/drm"),
            &command_line,
            requested.as_deref(),
        )?;
        if output.is_some() || Instant::now() >= deadline {
            return Ok(output);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Displays(std::path::PathBuf);
    impl Displays {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "agx-displays-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn add(&self, name: &str, status: &str, edid: &[u8]) {
            let path = self.0.join(name);
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("status"), status).unwrap();
            fs::write(path.join("edid"), edid).unwrap();
        }
        fn output(&self, command_line: &str, requested: Option<&str>) -> Option<String> {
            connected_output(&self.0, command_line, requested).unwrap()
        }
    }
    impl Drop for Displays {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn detects_either_port_and_ignores_render_nodes_and_disconnected_outputs() {
        let displays = Displays::new();
        displays.add("card0-HDMI-A-1", "disconnected\n", &[]);
        displays.add("card1-HDMI-A-2", "connected\n", &[]);
        displays.add("renderD128", "connected", &[]);
        assert_eq!(displays.output("", None).as_deref(), Some("HDMI-A-2"));
        assert_eq!(displays.output("", Some("HDMI-A-1")), None);
        displays.add("card0-HDMI-A-1", "connected\n", &[]);
        assert_eq!(displays.output("", None).as_deref(), Some("HDMI-A-1"));
        assert_eq!(
            displays.output("", Some("HDMI-A-2")).as_deref(),
            Some("HDMI-A-2")
        );
    }
    #[test]
    fn forced_connector_without_monitor_evidence_is_not_a_physical_display() {
        let displays = Displays::new();
        displays.add("card0-HDMI-A-1", "connected\n", &[]);
        let forced = "root=/dev/mmcblk0p2 video=HDMI-A-1:1280x720@60D";
        assert_eq!(displays.output(forced, None), None);
        displays.add(
            "card0-HDMI-A-1",
            "connected\n",
            &[0, 255, 255, 255, 255, 255, 255, 0],
        );
        assert_eq!(displays.output(forced, None).as_deref(), Some("HDMI-A-1"));
        assert_eq!(
            displays
                .output("video=HDMI-A-2:1280x720@60D", None)
                .as_deref(),
            Some("HDMI-A-1")
        );
    }
    #[test]
    fn supports_displayport_and_internal_panels_without_selecting_virtual_outputs() {
        let displays = Displays::new();
        displays.add("card0-Virtual-1", "connected", &[]);
        displays.add("card1-Writeback-1", "connected", &[]);
        displays.add("card2-DP-3", "connected", &[]);
        assert_eq!(displays.output("", None).as_deref(), Some("DP-3"));
        displays.add("card2-HDMI-A-1", "connected", &[]);
        assert_eq!(displays.output("", None).as_deref(), Some("HDMI-A-1"));
        displays.add("card2-eDP-1", "connected", &[]);
        assert_eq!(displays.output("", None).as_deref(), Some("eDP-1"));
        assert_eq!(displays.output("", Some("DP-3")).as_deref(), Some("DP-3"));
    }
    #[test]
    fn no_connectors_selects_no_physical_output() {
        let displays = Displays::new();
        assert_eq!(displays.output("", None), None);
    }
}
