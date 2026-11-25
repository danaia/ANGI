//! Drivers settings owns the worker, online results, and the graphics context report.
mod inventory;
mod online;
pub(crate) mod provision;
mod retry;

use crate::{AppWindow, DriverRow, DriversSettings};
use inventory::{Device, Inventory, Kind};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::{cell::Cell, collections::HashMap, path::Path, sync::mpsc};

#[derive(Clone, Eq, Hash, PartialEq)]
struct Lookup {
    kind: Kind,
    id: Option<inventory::BusId>,
    compatible: String,
    driver: String,
}

type Reports = HashMap<Lookup, online::Result>;

#[derive(Clone, Copy)]
enum Job {
    Inspect,
    Lookup,
    Prepare,
}

fn key(kind: Kind, device: &Device) -> Lookup {
    if let Some(id) = device.id.filter(|id| id.bus != "sdio") {
        return Lookup {
            kind,
            id: Some(id),
            compatible: String::new(),
            driver: String::new(),
        };
    }
    let driver = if device.bound.is_empty() {
        device.candidates.first().cloned().unwrap_or_default()
    } else {
        device.bound.clone()
    };
    Lookup {
        kind,
        id: device.id,
        compatible: device.compatible.clone(),
        driver,
    }
}

fn check(lookup: &Lookup) -> online::Result {
    let Lookup {
        kind,
        id,
        compatible,
        driver,
    } = lookup;
    if let Some(id) = id {
        let result = online::device(*kind, *id);
        if id.bus != "sdio"
            || matches!(&result, online::Result::Found { configs, .. } if !configs.is_empty())
        {
            return result;
        }
        return online::reference(compatible, driver).unwrap_or(result);
    }
    online::reference(compatible, driver).unwrap_or(online::Result::NoMatch)
}

fn check_online(inventory: &Inventory) -> Reports {
    let mut keys = Vec::new();
    for (&kind, devices) in &inventory.groups {
        for device in devices {
            let key = key(kind, device);
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
    }
    keys.truncate(16);
    std::thread::scope(|scope| {
        let workers: Vec<_> = keys
            .chunks(4)
            .map(|batch| {
                scope.spawn(move || {
                    batch
                        .iter()
                        .map(|key| (key.clone(), check(key)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        workers
            .into_iter()
            .filter_map(|worker| worker.join().ok())
            .flatten()
            .collect()
    })
}

fn row(device: &Device, result: Option<&online::Result>) -> DriverRow {
    let (status, level, mut details) = if !device.bound.is_empty() {
        let status = if device.endpoints.is_empty() {
            "Driver bound"
        } else {
            "In use"
        };
        (status, 1, vec![format!("Kernel driver: {}", device.bound)])
    } else if device.candidates.is_empty() {
        ("Driver not found", 2, Vec::new())
    } else {
        (
            "Installed, inactive",
            2,
            vec![format!(
                "Matching modules: {}",
                device.candidates.join(", ")
            )],
        )
    };
    if !device.endpoints.is_empty() {
        details.push(format!("Endpoints: {}", device.endpoints.join(", ")));
    }
    if let Some(id) = device.id {
        details.push(id.display());
    }
    let name = match result {
        Some(online::Result::Found { name, .. } | online::Result::KernelSource { name })
            if !name.is_empty() =>
        {
            name
        }
        _ => &device.name,
    };
    let reference = match result {
        Some(online::Result::Found { configs, .. }) if !configs.is_empty() => {
            format!("Online kernel matches: {}", configs.join(", "))
        }
        Some(online::Result::KernelSource { .. }) => {
            "Driver family found in upstream Linux source".into()
        }
        Some(online::Result::Found { .. } | online::Result::NoMatch) => {
            "No driver match in the online reference".into()
        }
        Some(online::Result::Unavailable) => "Online reference unavailable".into(),
        None => String::new(),
    };
    DriverRow {
        name: name.as_str().into(),
        status: status.into(),
        level,
        details: details.join(" · ").into(),
        reference: reference.into(),
    }
}

fn publish(ui: &AppWindow, inventory: &Inventory, reports: Option<&Reports>) {
    let page = ui.global::<DriversSettings>();
    page.set_machine(inventory.model.as_str().into());
    page.set_kernel(inventory.kernel.as_str().into());
    if let Some(reports) = reports {
        let unavailable = reports
            .values()
            .filter(|result| **result == online::Result::Unavailable)
            .count();
        let message = if reports.is_empty() {
            "No hardware IDs available for online lookup"
        } else if unavailable == reports.len() {
            "Online lookup unavailable. Local driver results are shown."
        } else if unavailable > 0 {
            "Some online references were unavailable"
        } else {
            "Online lookup complete"
        };
        page.set_online_note(message.into());
    }
    let [graphics, sound, wifi, bluetooth] = Kind::ALL.map(|kind| {
        let rows: Vec<_> = inventory
            .groups
            .get(&kind)
            .into_iter()
            .flatten()
            .map(|device| {
                row(
                    device,
                    reports.and_then(|reports| reports.get(&key(kind, device))),
                )
            })
            .collect();
        ModelRc::new(VecModel::from(rows))
    });
    page.set_graphics(graphics);
    page.set_sound(sound);
    page.set_wifi(wifi);
    page.set_bluetooth(bluetooth);
}

pub(super) fn install(ui: &AppWindow) {
    let page = ui.global::<DriversSettings>();
    let backend = std::env::var("SLINT_BACKEND").unwrap_or_default();
    page.set_backend(SharedString::from(backend.as_str()));
    if backend.ends_with("-software") {
        page.set_render_status("Software rendering".into());
        page.set_render_level(2);
    }
    let (jobs, work) = mpsc::channel::<Job>();
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        for job in work {
            let preparation = match job {
                Job::Inspect => retry::status(),
                Job::Prepare => Some(retry::prepare()),
                Job::Lookup => None,
            };
            let online = matches!(job, Job::Lookup);
            let inventory = std::sync::Arc::new(inventory::scan(
                Path::new("/sys"),
                Path::new("/proc"),
                Path::new("/lib/modules"),
            ));
            let local = inventory.clone();
            if weak
                .upgrade_in_event_loop(move |ui| {
                    publish(&ui, &local, None);
                    if let Some(status) = preparation {
                        retry::publish(&ui, status);
                    }
                    if !online {
                        ui.global::<DriversSettings>().set_busy(false);
                    }
                })
                .is_err()
            {
                break;
            }
            if online {
                let reports = check_online(&inventory);
                if weak
                    .upgrade_in_event_loop(move |ui| {
                        publish(&ui, &inventory, Some(&reports));
                        ui.global::<DriversSettings>().set_busy(false);
                    })
                    .is_err()
                {
                    break;
                }
            }
        }
    });
    let first_scan = Cell::new(false);
    let send = {
        let weak = ui.as_weak();
        move |job| {
            let Some(ui) = weak.upgrade() else { return };
            let page = ui.global::<DriversSettings>();
            if page.get_busy() {
                return;
            }
            match job {
                Job::Inspect => (),
                Job::Lookup => page.set_online_note("Checking online…".into()),
                Job::Prepare => page.set_prepare_note("Checking for signed drivers…".into()),
            }
            page.set_busy(jobs.send(job).is_ok());
        }
    };
    let scan = send.clone();
    page.on_open(move || {
        if !first_scan.replace(true) {
            scan(Job::Inspect);
        }
    });
    let prepare = send.clone();
    page.on_check(move || send(Job::Lookup));
    page.on_prepare(move || prepare(Job::Prepare));
}

#[cfg(any(feature = "desktop", feature = "kms", test))]
fn acceleration(renderer: &str) -> (&'static str, i32) {
    let name = renderer.to_ascii_lowercase();
    if [
        "llvmpipe",
        "softpipe",
        "swrast",
        "swiftshader",
        "software rasterizer",
    ]
    .iter()
    .any(|part| name.contains(part))
    {
        ("Software rendering", 2)
    } else if [
        "radeonsi",
        "amd radeon",
        "intel",
        "v3d",
        "vc4",
        "nvidia",
        "adreno",
        "mali",
        "apple",
    ]
    .iter()
    .any(|part| name.contains(part))
    {
        ("Hardware accelerated", 1)
    } else {
        ("OpenGL active", 0)
    }
}

/// Run only from the existing render notifier, while Slint's context is current.
#[cfg(any(feature = "desktop", feature = "kms"))]
pub(crate) fn record_renderer(ui: &slint::Weak<AppWindow>, api: &slint::GraphicsAPI) {
    use std::ffi::{CStr, c_char};
    let slint::GraphicsAPI::NativeOpenGL { get_proc_address } = api else {
        return;
    };
    let address = get_proc_address(c"glGetString");
    if address.is_null() {
        return;
    }
    // OpenGL owns the string. Copy it while the context is current.
    let query: unsafe extern "system" fn(u32) -> *const c_char =
        unsafe { std::mem::transmute(address) };
    let ptr = unsafe { query(0x1f01) }; // GL_RENDERER
    if ptr.is_null() {
        return;
    }
    let renderer = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    let (status, level) = acceleration(&renderer);
    eprintln!("agx-gui: GL renderer: {renderer}");
    let _ = ui.upgrade_in_event_loop(move |ui| {
        let page = ui.global::<DriversSettings>();
        page.set_renderer(renderer.into());
        page.set_render_status(status.into());
        page.set_render_level(level);
    });
}

#[cfg(test)]
mod tests;
