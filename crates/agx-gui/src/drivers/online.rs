//! Public device IDs are the only machine-derived values sent to the lookup.
//! Returned HTML is reduced to names and Kconfig symbols. It is never rendered.
use super::inventory::{BusId, Kind};
use std::{
    io::Read,
    process::{Command, Stdio},
};

const LIMIT: usize = 768 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Result {
    Found { name: String, configs: Vec<String> },
    KernelSource { name: String },
    NoMatch,
    Unavailable,
}

pub(super) fn device(kind: Kind, id: BusId) -> Result {
    let url = format!(
        "https://linux-hardware.org/?id={}:{:04x}-{:04x}",
        id.bus, id.vendor, id.product
    );
    match fetch(&url) {
        Ok(body) => parse_device(&body, kind, id),
        Err(Some(404)) => Result::NoMatch,
        Err(_) => Result::Unavailable,
    }
}

fn fetch(url: &str) -> std::result::Result<String, Option<u16>> {
    let mut child = Command::new("curl")
        .args([
            "-q",
            "--silent",
            "--proto",
            "=https",
            "--connect-timeout",
            "3",
            "--max-time",
            "7",
            "--max-filesize",
            "786432",
            "--write-out",
            "\n%{http_code}",
            "--",
            url,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| None)?;
    let mut data = Vec::new();
    let read = child
        .stdout
        .take()
        .expect("piped curl output")
        .take((LIMIT + 5) as u64)
        .read_to_end(&mut data);
    if read.is_err() || data.len() > LIMIT + 4 {
        let _ = child.kill();
    }
    if !child.wait().map_err(|_| None)?.success() || read.is_err() || data.len() > LIMIT + 4 {
        return Err(None);
    }
    let data = String::from_utf8_lossy(&data);
    let (body, code) = data.rsplit_once('\n').ok_or(None)?;
    match code.parse().ok() {
        Some(200) => Ok(body.to_owned()),
        other => Err(other),
    }
}

pub(super) fn parse_device(body: &str, kind: Kind, id: BusId) -> Result {
    if !body.contains(&id.display()) {
        return Result::Unavailable;
    }
    let name = body
        .split_once("<h2 class='top'>")
        .and_then(|(_, rest)| rest.split_once("</h2>"))
        .map(|(label, _)| {
            label
                .trim()
                .trim_start_matches("Device '")
                .trim_end_matches('\'')
        })
        .filter(|label| !label.contains(['<', '>', '&']) && label.chars().all(|c| !c.is_control()))
        .unwrap_or_default()
        .chars()
        .take(100)
        .collect();
    let mut configs = Vec::new();
    if let Some((_, table)) = body.split_once("<a name='kernel_drivers'>")
        && let Some((table, _)) = table.split_once("</table>")
    {
        for row in table.split("<tr>").skip(1) {
            let mut cells = row
                .split("<td>")
                .skip(1)
                .filter_map(|cell| cell.split_once("</td>").map(|(value, _)| value.trim()));
            let (Some(path), Some(symbols)) = (cells.nth(1), cells.next()) else {
                continue;
            };
            for symbol in symbols
                .split_whitespace()
                .filter(|symbol| related(kind, path, symbol))
            {
                if !configs.iter().any(|known| known == symbol) {
                    configs.push(symbol.to_owned());
                }
                if configs.len() == 8 {
                    return Result::Found { name, configs };
                }
            }
        }
    }
    Result::Found { name, configs }
}

fn related(kind: Kind, path: &str, symbol: &str) -> bool {
    let Some(config) = symbol.strip_prefix("CONFIG_") else {
        return false;
    };
    if config.is_empty()
        || config.len() > 64
        || !config
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
    {
        return false;
    }
    match kind {
        Kind::Graphics => path.starts_with("drivers/gpu/") && config.starts_with("DRM_"),
        Kind::Sound => {
            path.starts_with("sound/") && config.starts_with("SND_") && config != "SND_SOC"
        }
        Kind::Wifi => {
            path.starts_with("drivers/net/wireless/")
                && !matches!(config, "WLAN" | "WIRELESS" | "CFG80211" | "MAC80211")
                && !config.starts_with("WLAN_VENDOR_")
        }
        Kind::Bluetooth => path.starts_with("drivers/bluetooth/") && config.starts_with("BT_"),
    }
}

/// For integrated devices with Device Tree bindings, query the Linux source that
/// owns the matching driver family. An SDIO or USB ID can still be looked up above.
pub(super) fn reference(compatible: &str, driver: &str) -> Option<Result> {
    let is_broadcom = compatible.starts_with("brcm,");
    let (name, file, marker) = match driver {
        "v3d" => ("Broadcom V3D", "drivers/gpu/drm/v3d/v3d_drv.c", compatible),
        "vc4" => ("Broadcom VC4", "drivers/gpu/drm/vc4/vc4_drv.c", compatible),
        "brcmfmac" => (
            "Broadcom Wi-Fi",
            "drivers/net/wireless/broadcom/brcm80211/brcmfmac/bcmsdh.c",
            "brcmf_sdmmc_ids",
        ),
        "hci_uart" | "hci_bcm" | "hci_uart_bcm" if is_broadcom => (
            "Broadcom Bluetooth",
            "drivers/bluetooth/hci_bcm.c",
            compatible,
        ),
        "bcm2835_audio" | "snd_bcm2835" => (
            "Broadcom audio",
            "drivers/staging/vc04_services/bcm2835-audio/bcm2835.c",
            "MODULE_DEVICE_TABLE(vchiq",
        ),
        _ if is_broadcom && compatible.ends_with("-v3d") => {
            ("Broadcom V3D", "drivers/gpu/drm/v3d/v3d_drv.c", compatible)
        }
        _ if is_broadcom
            && ["-vc4", "-vc5", "-vc6"]
                .iter()
                .any(|suffix| compatible.ends_with(suffix)) =>
        {
            ("Broadcom VC4", "drivers/gpu/drm/vc4/vc4_drv.c", compatible)
        }
        _ if is_broadcom && compatible.ends_with("-bt") => (
            "Broadcom Bluetooth",
            "drivers/bluetooth/hci_bcm.c",
            compatible,
        ),
        _ if compatible == "brcm,bcm2835-audio" => (
            "Broadcom audio",
            "drivers/staging/vc04_services/bcm2835-audio/bcm2835.c",
            "MODULE_DEVICE_TABLE(vchiq",
        ),
        _ => return None,
    };
    let url = format!("https://raw.githubusercontent.com/torvalds/linux/master/{file}");
    let marker = if marker.is_empty() {
        "MODULE_DEVICE_TABLE(of"
    } else {
        marker
    };
    let result = match fetch(&url) {
        Ok(body) if body.contains(marker) => Result::KernelSource { name: name.into() },
        Ok(_) | Err(Some(404)) => Result::NoMatch,
        Err(_) => Result::Unavailable,
    };
    Some(result)
}
