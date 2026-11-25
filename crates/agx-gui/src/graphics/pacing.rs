//! Apply measured frequency floors only to the hardware that was benchmarked.
use std::{fs, io, path::Path};

const BENCHMARK_FLOOR: u32 = 400;
const FACTORY_FLOOR: u32 = 200;

fn value(path: &Path, expected: &str) -> bool {
    fs::read_to_string(path).is_ok_and(|text| text.trim() == expected)
}

fn mhz(path: &Path) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// The N5095 Mini S sustained 60 Hz dragging at this floor with the stock CPU
/// policy. Other boards keep their own driver policy until benchmarked.
pub(super) fn apply_beelink_floor(sys: &Path, output: Option<&str>) -> io::Result<bool> {
    let dmi = sys.join("class/dmi/id");
    if !value(&dmi.join("sys_vendor"), "AZW") || !value(&dmi.join("product_name"), "MINI S") {
        return Ok(false);
    }
    let drm = sys.join("class/drm");
    let mut changed = false;
    for connector in fs::read_dir(&drm)? {
        let connector = connector?;
        let name = connector.file_name();
        let Some((card, port)) = name.to_str().and_then(|name| name.split_once('-')) else {
            continue;
        };
        if !card
            .strip_prefix("card")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|byte| byte.is_ascii_digit()))
            || output.is_some_and(|selected| selected != port)
            || !value(&connector.path().join("status"), "connected")
        {
            continue;
        }
        let card = drm.join(card);
        let device = card.join("device");
        let floor = card.join("gt_min_freq_mhz");
        // This PCI device is Jasper Lake UHD Graphics. A nondefault floor may
        // have been picked by the administrator or another power policy.
        if !value(&device.join("vendor"), "0x8086")
            || !value(&device.join("device"), "0x4e55")
            || !fs::read_link(device.join("driver"))
                .is_ok_and(|path| path.file_name().is_some_and(|name| name == "i915"))
            || mhz(&floor) != Some(FACTORY_FLOOR)
            || !mhz(&card.join("gt_max_freq_mhz")).is_some_and(|max| max >= BENCHMARK_FLOOR)
        {
            continue;
        }
        fs::write(floor, b"400\n")?;
        changed = true;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn tunes_only_the_validated_active_intel_adapter_within_its_limits() {
        let fixture = std::env::temp_dir().join(format!("angi-pacing-{}", std::process::id()));
        let dmi = fixture.join("class/dmi/id");
        let card = fixture.join("class/drm/card3");
        let connector = fixture.join("class/drm/card3-DP-1");
        fs::create_dir_all(&dmi).unwrap();
        fs::create_dir_all(&connector).unwrap();
        fs::create_dir_all(card.join("device")).unwrap();
        for (path, contents) in [
            (dmi.join("sys_vendor"), "AZW"),
            (dmi.join("product_name"), "another board"),
            (card.join("device/vendor"), "0x8086"),
            (card.join("device/device"), "0x4e55"),
            (card.join("gt_min_freq_mhz"), "200"),
            (card.join("gt_max_freq_mhz"), "750"),
            (connector.join("status"), "connected"),
        ] {
            fs::write(path, contents).unwrap();
        }
        symlink("/sys/bus/pci/drivers/i915", card.join("device/driver")).unwrap();

        assert!(!apply_beelink_floor(&fixture, None).unwrap());
        fs::write(dmi.join("product_name"), "MINI S").unwrap();
        assert!(!apply_beelink_floor(&fixture, Some("HDMI-A-1")).unwrap());
        fs::write(card.join("gt_max_freq_mhz"), "300").unwrap();
        assert!(!apply_beelink_floor(&fixture, Some("DP-1")).unwrap());
        fs::write(card.join("gt_max_freq_mhz"), "750").unwrap();
        assert!(apply_beelink_floor(&fixture, Some("DP-1")).unwrap());
        assert_eq!(mhz(&card.join("gt_min_freq_mhz")), Some(400));
        assert!(!apply_beelink_floor(&fixture, None).unwrap());

        fs::remove_dir_all(fixture).unwrap();
    }
}
