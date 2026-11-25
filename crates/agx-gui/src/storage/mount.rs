//! List data storage and run the administrator helper off the UI thread.
use crate::{AppWindow, DriveMount};
use serde_json::Value;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::process::Command;

const HELPER: &str = "/usr/libexec/agx-agent/data_volume.py";

fn choices(devices: &[Value], id_key: &str) -> (ModelRc<SharedString>, ModelRc<SharedString>) {
    let (labels, ids): (Vec<_>, Vec<_>) = devices
        .iter()
        .filter_map(|device| {
            let size = crate::system_ui::bytes(device.get("bytes")?.as_u64()?);
            let path = device.get("path")?.as_str()?;
            let id = device.get(id_key)?.as_str()?;
            let label = device
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let name = if label.is_empty() {
                format!("{path} · {size}")
            } else {
                format!("{label} · {path} · {size}")
            };
            Some((SharedString::from(name), SharedString::from(id)))
        })
        .unzip();
    (
        ModelRc::new(VecModel::from(labels)),
        ModelRc::new(VecModel::from(ids)),
    )
}

fn run(args: &[&str]) -> Result<String, String> {
    let output = Command::new("/usr/bin/angi-root")
        .args(["/usr/bin/python3", HELPER])
        .args(args)
        .output()
        .map_err(|error| format!("Could not start the ANGI drive helper: {error}"))?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr);
        return Err(message.trim().to_owned());
    }
    String::from_utf8(output.stdout).map_err(|_| "The drive helper returned invalid text.".into())
}

pub(super) fn install(ui: &AppWindow, worker: super::worker::Worker) {
    let weak = ui.as_weak();
    let page = ui.global::<DriveMount>();
    page.on_show_picker(move || {
        let Some(ui) = weak.upgrade() else { return };
        let page = ui.global::<DriveMount>();
        if page.get_open() {
            return;
        }
        page.set_open(true);
        page.set_busy(true);
        page.set_partitions(ModelRc::default());
        page.set_drives(ModelRc::default());
        page.set_partition_ids(ModelRc::default());
        page.set_drive_ids(ModelRc::default());
        page.set_message("Looking for storage…".into());
        let weak = ui.as_weak();
        std::thread::spawn(move || {
            let outcome = run(&["list-picker"]).and_then(|data| {
                let result = (|| {
                    let picker = serde_json::from_str::<Value>(&data).ok()?;
                    Some((
                        picker.get("partitions")?.as_array()?.clone(),
                        picker.get("drives")?.as_array()?.clone(),
                        picker.get("format_ready")?.as_bool()?,
                    ))
                })();
                result.ok_or_else(|| "The drive helper returned an invalid device list.".into())
            });
            let _ = weak.upgrade_in_event_loop(move |ui| {
                let page = ui.global::<DriveMount>();
                page.set_busy(false);
                match outcome {
                    Ok((partitions, drives, format_ready)) => {
                        let (partitions, partition_ids) = choices(&partitions, "uuid");
                        let (drives, drive_ids) = choices(&drives, "id");
                        let message = if partitions.row_count() > 0 {
                            ""
                        } else if drives.row_count() > 0 {
                            "No ext4 partition is available. Choose Quick Format to prepare an NVMe drive."
                        } else if !format_ready {
                            "No ext4 partition is available. Quick format requires the ANGI disk-formatting tools."
                        } else {
                            "No mountable ext4 partition was found on an internal drive."
                        };
                        page.set_message(message.into());
                        page.set_partition_ids(partition_ids);
                        page.set_drive_ids(drive_ids);
                        page.set_partitions(partitions);
                        page.set_drives(drives);
                    }
                    Err(error) => page.set_message(error.into()),
                }
            });
        });
    });

    let weak = ui.as_weak();
    page.on_apply(move |index, format| {
        let Some(ui) = weak.upgrade() else { return };
        let page = ui.global::<DriveMount>();
        if !page.get_open() || page.get_busy() {
            return;
        }
        let (model, action, message) = if format {
            (
                page.get_drive_ids(),
                "format",
                "Formatting and mounting the selected drive…",
            )
        } else {
            (
                page.get_partition_ids(),
                "mount",
                "Mounting the selected partition…",
            )
        };
        let Some(id) = model.row_data(index as usize) else {
            return;
        };
        page.set_busy(true);
        page.set_message(message.into());
        let weak = ui.as_weak();
        let worker = worker.clone();
        std::thread::spawn(move || {
            let outcome = run(&[action, id.as_str()]);
            let _ = weak.upgrade_in_event_loop(move |ui| {
                let page = ui.global::<DriveMount>();
                page.set_busy(false);
                match outcome {
                    Ok(_) => {
                        page.set_open(false);
                        let _ = worker.refresh();
                        ui.invoke_focus_desktop();
                    }
                    Err(error) => {
                        if format {
                            // The old partition list may no longer describe the drive.
                            page.set_drives(ModelRc::default());
                            page.set_partitions(ModelRc::default());
                            page.set_drive_ids(ModelRc::default());
                            page.set_partition_ids(ModelRc::default());
                        }
                        page.set_message(error.into());
                    }
                }
            });
        });
    });
}
