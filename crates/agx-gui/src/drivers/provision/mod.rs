//! First-boot driver preparation. Keep the running greeter and network bound.
mod catalog;
mod files;

use super::inventory::{self, Device, Inventory, Kind};
use agx_init::DriverPreparation;
use std::{
    collections::BTreeSet,
    path::Path,
    process::{Command, ExitCode},
};

pub(crate) struct Outcome {
    pub rows: Vec<String>,
    pub ready: bool,
    pub restart: bool,
    pub error: Option<String>,
}

pub(crate) fn run_for_settings() -> ExitCode {
    let outcome = prepare();
    let status = if outcome.restart {
        DriverPreparation::RestartRequired
    } else if outcome.ready {
        DriverPreparation::Ready
    } else {
        DriverPreparation::Incomplete(outcome.error.unwrap_or_default())
    };
    match serde_json::to_writer(std::io::stdout().lock(), &status) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

fn scan() -> Inventory {
    inventory::scan(
        Path::new("/sys"),
        Path::new("/proc"),
        Path::new("/lib/modules"),
    )
}

pub(crate) fn prepare() -> Outcome {
    // The preview must never modify the host, and the real greeter runs as root.
    if unsafe { libc::geteuid() } != 0 {
        return Outcome {
            rows: Vec::new(),
            ready: false,
            restart: false,
            error: Some("Driver preparation requires the ANGI setup session.".into()),
        };
    }

    let initial = scan();
    load_local(&initial);
    let detected = scan();
    let install = catalog::prepare(&detected);
    let restart = Path::new("/run/agx-driver-restart-required").exists();
    let (mut rows, missing) = summarize(&scan());
    #[cfg(feature = "kms-provision-graphics")]
    let missing = if crate::graphics::provisioned::runtime_available() {
        missing
    } else {
        rows.insert(
            0,
            "Graphics acceleration needs a downloaded graphics bundle.".into(),
        );
        true
    };
    let error = match install {
        Ok(_) if restart => {
            rows.insert(
                0,
                "Drivers installed. Restart ANGI to activate them and check the hardware again."
                    .into(),
            );
            None
        }
        Ok(catalog::Status::Installed) | Ok(catalog::Status::NoMatch) if missing => {
            Some("Some detected devices still need a working driver.".into())
        }
        Ok(catalog::Status::Unconfigured) if missing => Some(
            "Some detected devices need drivers. This image has no online driver feed configured."
                .into(),
        ),
        Err(error) => {
            eprintln!("agx-gui: driver preparation: {error}");
            let access_required = error
                .get_ref()
                .is_some_and(|cause| cause.is::<catalog::FeedAccessRequired>());
            Some(if access_required {
                "The driver feed requires access. Ask your ANGI provider how to register for downloads."
            } else {
                "The online driver check could not finish. Check the connection and retry."
            }.into())
        }
        _ => None,
    };
    Outcome {
        rows,
        ready: error.is_none() && !restart,
        restart,
        error,
    }
}

fn load_local(inventory: &Inventory) {
    let modules: BTreeSet<_> = inventory
        .groups
        .iter()
        // A new DRM device may take over the framebuffer used by the greeter.
        .filter(|(kind, _)| **kind != Kind::Graphics)
        .flat_map(|(_, devices)| devices)
        .filter(|device| device.bound.is_empty())
        .flat_map(|device| &device.candidates)
        .filter(|name| {
            !name.is_empty()
                && !name.starts_with('-')
                && name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
        })
        .collect();
    for module in modules {
        let result = Command::new("modprobe").arg(module).output();
        if !result.is_ok_and(|output| output.status.success()) {
            eprintln!("agx-gui: could not activate module {module}");
        }
    }
}

fn summarize(inventory: &Inventory) -> (Vec<String>, bool) {
    let mut missing = false;
    let rows = Kind::ALL
        .into_iter()
        .map(|kind| {
            let label = match kind {
                Kind::Graphics => "Graphics",
                Kind::Sound => "Sound",
                Kind::Wifi => "Wi-Fi",
                Kind::Bluetooth => "Bluetooth",
            };
            let Some(devices) = inventory
                .groups
                .get(&kind)
                .filter(|devices| !devices.is_empty())
            else {
                return format!("{label} · No device detected");
            };
            let status = devices
                .iter()
                .map(|device| {
                    if device.endpoints.is_empty() {
                        missing = true;
                    }
                    describe(device)
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{label} · {status}")
        })
        .collect();
    (rows, missing)
}

fn describe(device: &Device) -> String {
    if !device.endpoints.is_empty() {
        let names = if device.bound.is_empty() {
            "Ready"
        } else {
            &device.bound
        };
        return names.to_owned();
    }
    let name = device
        .id
        .map(|id| id.display())
        .unwrap_or_else(|| device.name.clone());
    if device.bound.is_empty() && device.candidates.is_empty() {
        format!("{name} needs a driver")
    } else if device.bound.is_empty() {
        format!("{name} has an inactive driver")
    } else {
        format!("{name} has a driver but no active device")
    }
}

#[cfg(test)]
mod tests;
