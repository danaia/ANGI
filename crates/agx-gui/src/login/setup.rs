//! First-run setup in the greeter, before the first administrator signs in.
use crate::drivers::provision as drivers;
mod wifi;

use crate::{GreeterSetup, LoginWindow, WifiChoice};
use agx_net::Request;
use agx_profile::{Save, service};
use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::time::Duration;
use zeroize::Zeroizing;

const RESUME_DRIVERS: &str = "/etc/agx/setup-driver-resume";

pub(super) fn install(ui: &LoginWindow) -> Timer {
    let page = ui.global::<GreeterSetup>();
    page.set_step(-1);
    page.set_wifi_available(wifi::adapter_available());
    let weak = ui.as_weak();
    page.on_scan(move |country| {
        let Some(ui) = weak.upgrade() else { return };
        if ui.global::<GreeterSetup>().get_wifi_available()
            && start(&ui, 1, "Scanning for Wi-Fi networks…")
        {
            let country = country.to_string();
            let page = ui.global::<GreeterSetup>();
            page.set_networks(ModelRc::default());
            page.set_wifi_choices(ModelRc::default());
            let weak = ui.as_weak();
            std::thread::spawn(move || {
                let outcome = wifi::scan(country);
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    let page = ui.global::<GreeterSetup>();
                    page.set_busy(false);
                    page.set_status("".into());
                    match outcome {
                        Ok(rows) => {
                            let status = if rows.is_empty() {
                                "No networks found. Scan again or enter the network name."
                            } else {
                                "Choose a network."
                            };
                            page.set_status(status.into());
                            page.set_wifi_choices(model(rows.iter().map(|row| WifiChoice {
                                id: row.ssid_hex.as_str().into(),
                                security: wifi::security_index(row.security),
                            })));
                            page.set_networks(model(rows.iter().map(|row| {
                                SharedString::from(format!(
                                    "{} · {}",
                                    row.ssid,
                                    row.security.label()
                                ))
                            })));
                        }
                        Err(error) => page.set_error(error.into()),
                    }
                });
            });
        }
    });

    let weak = ui.as_weak();
    page.on_connect(move |country, picked, ssid, security, password, hidden| {
        let Some(ui) = weak.upgrade() else { return };
        let page = ui.global::<GreeterSetup>();
        if !page.get_wifi_available() {
            return;
        }
        let (ssid_hex, security) = if picked < 0 {
            (agx_net::hex(ssid.as_bytes()), wifi::security(security))
        } else {
            let Some(point) = page.get_wifi_choices().row_data(picked as usize) else {
                return;
            };
            (point.id.to_string(), wifi::security(point.security))
        };
        if start(&ui, 1, "Connecting to Wi-Fi…") {
            let country = country.to_string();
            let request = Request::Connect {
                ssid_hex: ssid_hex.clone(),
                security,
                password: Zeroizing::new(password.to_string()),
                hidden,
            };
            let weak = ui.as_weak();
            std::thread::spawn(move || {
                let outcome = wifi::connect(country, ssid_hex, request);
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    let page = ui.global::<GreeterSetup>();
                    page.set_busy(false);
                    page.set_status("".into());
                    match outcome {
                        Ok(network) => {
                            begin_drivers(&ui, format!("Connected to {network}."), false);
                        }
                        Err(error) => page.set_error(error.into()),
                    }
                });
            });
        }
    });

    let weak = ui.as_weak();
    page.on_prepare_drivers(move || {
        if let Some(ui) = weak.upgrade() {
            begin_drivers(&ui, String::new(), false);
        }
    });

    let weak = ui.as_weak();
    page.on_restart_drivers(move || {
        let Some(ui) = weak.upgrade() else { return };
        let page = ui.global::<GreeterSetup>();
        if !page.get_restart_required() || !start(&ui, 2, "Restarting ANGI…") {
            return;
        }
        let weak = ui.as_weak();
        std::thread::spawn(move || {
            let restarted = std::fs::write(RESUME_DRIVERS, b"").is_ok()
                && std::process::Command::new("/usr/bin/agx")
                    .args(["reboot", "--yes"])
                    .status()
                    .is_ok_and(|status| status.success());
            if !restarted {
                let _ = std::fs::remove_file(RESUME_DRIVERS);
                let _ = weak.upgrade_in_event_loop(|ui| {
                    let page = ui.global::<GreeterSetup>();
                    page.set_busy(false);
                    page.set_status("".into());
                    page.set_error(
                        "Could not restart. Try again or restart the computer manually.".into(),
                    );
                });
            }
        });
    });

    let weak = ui.as_weak();
    page.on_continue_setup(move |defer| {
        let Some(ui) = weak.upgrade() else { return };
        let page = ui.global::<GreeterSetup>();
        if page.get_busy() || page.get_step() != 2 || (!defer && !page.get_ready()) {
            return;
        }
        page.set_status("".into());
        page.set_error("".into());
        page.set_step(3);
    });

    let weak = ui.as_weak();
    page.on_finish(move |username| {
        if let Some(ui) = weak.upgrade() {
            ui.invoke_show_login(username);
        }
    });

    let weak = ui.as_weak();
    page.on_create_account(move |name, hostname, username, password| {
        let Some(ui) = weak.upgrade() else { return };
        if start(&ui, 3, "Creating your administrator account…") {
            let save = Save {
                name: name.to_string(),
                hostname: hostname.to_string(),
                username: username.to_string(),
                password: Zeroizing::new(password.to_string()),
                create: true,
            };
            let username = save.username.clone();
            let weak = ui.as_weak();
            std::thread::spawn(move || {
                let outcome = service::save_initial(save, None);
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    let page = ui.global::<GreeterSetup>();
                    page.set_busy(false);
                    page.set_status("".into());
                    match outcome {
                        // Security is the last step (4); the account exists, so the greeter stays
                        // on it instead of jumping to sign-in.
                        Ok(_) => {
                            page.set_created_user(username.into());
                            page.set_step(4);
                        }
                        Err(error) => page.set_error(error.to_string().into()),
                    }
                });
            });
        }
    });

    // An administrator created through USB setup or the TUI also completes desktop setup.
    let timer = Timer::default();
    let weak = ui.as_weak();
    timer.start(TimerMode::Repeated, Duration::from_secs(1), move || {
        let Some(ui) = weak.upgrade() else { return };
        let page = ui.global::<GreeterSetup>();
        // Step 4 (Security) follows the account; the sign-in screen waits for Finish.
        if page.get_step() == 0 || page.get_step() == 4 || page.get_busy() {
            return;
        }
        if page.get_step() <= 1 {
            let available = wifi::adapter_available();
            if page.get_wifi_available() != available {
                page.set_error("".into());
                page.set_wifi_available(available);
            }
        }
        if service::setup_claimed().unwrap_or(false) && page.get_step() < 0 {
            ui.invoke_show_login("".into());
            return;
        }
        match service::status() {
            Ok(status) => {
                if let Some(account) = status.accounts.first() {
                    let username = if page.get_step() < 0 {
                        ""
                    } else {
                        account.username.as_str()
                    };
                    ui.invoke_show_login(username.into());
                } else if page.get_step() < 0 {
                    page.set_error("".into());
                    // Keep the public name for images built with the older internal hostname.
                    if !status.hostname.is_empty() && status.hostname != "agx" {
                        page.set_hostname(status.hostname.into());
                    }
                    page.set_step(1);
                    if std::fs::remove_file(RESUME_DRIVERS).is_ok() {
                        begin_drivers(&ui, String::new(), true);
                    }
                }
            }
            Err(_) if page.get_step() < 0 => {
                page.set_error("Waiting for the ANGI account service…".into());
            }
            Err(_) => {}
        }
    });
    timer
}

fn model<T: Clone + 'static>(values: impl Iterator<Item = T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(values.collect::<Vec<_>>()))
}

fn start(ui: &LoginWindow, step: i32, message: &str) -> bool {
    let page = ui.global::<GreeterSetup>();
    if page.get_busy() || page.get_step() != step {
        return false;
    }
    page.set_busy(true);
    page.set_status(message.into());
    page.set_error("".into());
    true
}

fn begin_drivers(ui: &LoginWindow, mut connection: String, reconnect: bool) {
    let page = ui.global::<GreeterSetup>();
    if page.get_busy() || !matches!(page.get_step(), 1 | 2) {
        return;
    }
    page.set_step(2);
    page.set_ready(false);
    page.set_restart_required(false);
    page.set_driver_rows(ModelRc::default());
    if !start(ui, 2, "Preparing drivers for this computer…") {
        return;
    }
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        if reconnect && let Some(network) = wifi::reconnect() {
            connection = format!("Connected to {network}.");
        }
        let outcome = drivers::prepare();
        let _ = weak.upgrade_in_event_loop(move |ui| {
            let page = ui.global::<GreeterSetup>();
            page.set_driver_rows(model(outcome.rows.into_iter().map(SharedString::from)));
            page.set_ready(outcome.ready);
            page.set_restart_required(outcome.restart);
            page.set_error(outcome.error.unwrap_or_default().into());
            page.set_status(connection.into());
            page.set_busy(false);
        });
    });
}
