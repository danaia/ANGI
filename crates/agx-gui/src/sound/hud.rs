use super::OutputVolume;
use crate::{AppWindow, VolumeHud};
use slint::ComponentHandle;
use std::time::{Duration, Instant};

const HOLD: Duration = Duration::from_millis(1600);

#[derive(Default)]
pub(super) struct Hud {
    until: Option<Instant>,
    #[cfg(feature = "wayland")]
    native: Option<crate::VolumeHudWindow>,
}

impl Hud {
    fn global<'a>(&'a self, ui: &'a AppWindow) -> VolumeHud<'a> {
        #[cfg(feature = "wayland")]
        if let Some(native) = &self.native {
            return native.global::<VolumeHud>();
        }
        ui.global::<VolumeHud>()
    }

    pub(super) fn update(&mut self, ui: &AppWindow) {
        if self.until.is_some_and(|until| Instant::now() >= until) {
            self.global(ui).set_shown(false);
            self.until = None;
        }

        #[cfg(feature = "wayland")]
        {
            if self.native.is_none() && ui.global::<VolumeHud>().get_native_available() {
                ui.global::<VolumeHud>().set_native_available(false);
                match crate::VolumeHudWindow::new().and_then(|native| {
                    native.show()?;
                    Ok(native)
                }) {
                    Ok(native) => {
                        ui.global::<VolumeHud>().set_shown(false);
                        self.native = Some(native);
                    }
                    Err(error) => eprintln!("agx-gui: volume overlay: {error}"),
                }
            }
            if let Some(native) = &self.native {
                native.global::<crate::Theme>().set_dark(ui.global::<crate::Theme>().get_dark());
            }
        }
    }

    pub(super) fn show(&mut self, ui: &AppWindow, level: OutputVolume) {
        let hud = self.global(ui);
        hud.set_percent(level.percent.clamp(0, 100));
        hud.set_muted(level.muted);
        hud.set_shown(true);
        self.until = Some(Instant::now() + HOLD);
    }
}
