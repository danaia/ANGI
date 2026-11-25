//! Adapter from the signed-in Drivers page to the administrator service manager.
use super::{AppWindow, DriversSettings};
use agx_init::{DriverPreparation, Request};
use slint::ComponentHandle;
use std::time::{Duration, Instant};

fn request(start: bool) -> Option<DriverPreparation> {
    agx_init::request(&Request::DriverPreparation { start })
        .ok()?
        .driver_preparation
}

pub(super) fn status() -> Option<DriverPreparation> {
    request(false)
}

pub(super) fn prepare() -> DriverPreparation {
    let deadline = Instant::now() + Duration::from_secs(300);
    let mut outcome = request(true);
    while matches!(outcome, Some(DriverPreparation::Working)) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_secs(1));
        outcome = request(false);
    }
    outcome.unwrap_or_else(|| {
        DriverPreparation::Incomplete(
            "Could not reach the ANGI driver installer. Sign in as the local administrator and try again.".into(),
        )
    })
}

pub(super) fn publish(ui: &AppWindow, outcome: DriverPreparation) {
    let page = ui.global::<DriversSettings>();
    page.set_prepare_available(true);
    let note = match outcome {
        DriverPreparation::Available => String::new(),
        DriverPreparation::Working => {
            "The driver installer is still running. Check again shortly.".into()
        }
        DriverPreparation::Ready => "Drivers are ready.".into(),
        DriverPreparation::RestartRequired => "Drivers installed. Restart ANGI to use them.".into(),
        DriverPreparation::Incomplete(message) => message,
    };
    page.set_prepare_note(note.into());
}
