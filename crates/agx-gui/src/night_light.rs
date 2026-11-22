//! Night light: per-channel colour correction for the whole display.
//!
//! The warmth slider maps to a colour temperature (about 4500 K for "Soft glow" down to 2300 K for
//! "Deep night") and a gentle dim. Each of red, green and blue is scaled by its own gain, so blue
//! falls much further than red, which a tinted overlay cannot do. Three paths apply the same gains:
//!
//! - `gl`: a multiply pass drawn over the finished frame in Slint's render hook (HDMI and the
//!   desktop preview), so the browser, terminal and every dialog are covered.
//! - [`filter_pixels`]: the same scaling on captured frames, for the VNC-only software display.
//! - Otherwise `NightLight.native` stays false and `ui/night-light.slint` draws an amber overlay.
use crate::{AppWindow, NightLight};
use slint::ComponentHandle;
use std::sync::Mutex;

#[cfg(any(feature = "kms", feature = "desktop"))]
pub mod gl;

/// Warmest and softest colour temperatures, in kelvin, at warmth 100 and 0.
const KELVIN_SOFT: f32 = 4500.0;
const KELVIN_DEEP: f32 = 2300.0;
/// Overall brightness at warmth 100; the night view is also a little darker.
const DIM_DEEP: f32 = 0.8;

#[derive(Clone, Copy)]
struct State {
    /// `None` while night light is off.
    gain: Option<[f32; 3]>,
    /// The display filters itself, so the Slint overlay is hidden.
    native: bool,
    /// Captured VNC frames are scaled here (the software display has no GL pass).
    software_frames: bool,
}
static STATE: Mutex<State> = Mutex::new(State {
    gain: None,
    native: false,
    software_frames: false,
});
fn state() -> State {
    *STATE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
fn update(change: impl FnOnce(&mut State)) {
    change(&mut STATE.lock().unwrap_or_else(|poisoned| poisoned.into_inner()));
}

/// Relative red, green and blue of a black-body colour temperature (Tanner Helland's fit).
fn kelvin_rgb(kelvin: f32) -> [f32; 3] {
    let t = kelvin / 100.0;
    let red = if t <= 66.0 {
        255.0
    } else {
        329.698_73 * (t - 60.0).powf(-0.133_204_76)
    };
    let green = if t <= 66.0 {
        99.470_8 * t.ln() - 161.119_57
    } else {
        288.122_17 * (t - 60.0).powf(-0.075_514_85)
    };
    let blue = if t >= 66.0 {
        255.0
    } else if t <= 19.0 {
        0.0
    } else {
        138.517_73 * (t - 10.0).ln() - 305.044_8
    };
    [red, green, blue].map(|channel| (channel / 255.0).clamp(0.0, 1.0))
}

/// Gains for a warmth from 0 (soft glow) to 100 (deep night).
pub fn gain_for(warmth: f32) -> [f32; 3] {
    let ratio = (warmth / 100.0).clamp(0.0, 1.0);
    let rgb = kelvin_rgb(KELVIN_SOFT + (KELVIN_DEEP - KELVIN_SOFT) * ratio);
    let dim = 1.0 - (1.0 - DIM_DEEP) * ratio;
    rgb.map(|channel| channel * dim)
}

/// Turn the filter on or off and set its warmth. Call `request_redraw` afterwards.
pub fn set(enabled: bool, warmth: f32) {
    update(|state| state.gain = enabled.then(|| gain_for(warmth)));
}

/// The gains to draw with, or `None` while night light is off.
pub fn gain() -> Option<[f32; 3]> {
    state().gain
}

/// The display now applies night light itself, so hide the overlay.
pub fn mark_native(ui: &AppWindow) {
    update(|state| state.native = true);
    ui.global::<NightLight>().set_native(true);
}

/// The VNC-only software display has no GL pass: scale captured frames instead.
pub fn use_software_frames(ui: &AppWindow) {
    update(|state| state.software_frames = true);
    mark_native(ui);
}

/// The gains for captured frames, only on the software display.
pub fn frame_gain() -> Option<[f32; 3]> {
    let state = state();
    state.software_frames.then_some(state.gain).flatten()
}

/// Scale RGBA pixels by the current gains in place. Does nothing unless a software frame filter is on.
pub fn filter_pixels(bytes: &mut [u8]) {
    let Some(gain) = frame_gain() else {
        return;
    };
    let tables = gain.map(|g| std::array::from_fn::<u8, 256, _>(|v| (v as f32 * g).round() as u8));
    for pixel in bytes.chunks_exact_mut(4) {
        pixel[0] = tables[0][usize::from(pixel[0])];
        pixel[1] = tables[1][usize::from(pixel[1])];
        pixel[2] = tables[2][usize::from(pixel[2])];
    }
}

/// The desktop preview (winit) has no browser hook, so it installs its own render hook.
#[cfg(feature = "desktop")]
pub fn install_preview(ui: &AppWindow) {
    if std::env::var("SLINT_BACKEND").as_deref() == Ok("winit-femtovg") {
        gl::install_preview(ui);
    }
}
#[cfg(not(feature = "desktop"))]
pub fn install_preview(_ui: &AppWindow) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blue_falls_further_than_red_as_warmth_rises() {
        let mut last = [1.0_f32; 3];
        for warmth in [0.0, 25.0, 50.0, 75.0, 100.0] {
            let gain = gain_for(warmth);
            assert!(gain[2] < last[2] && gain[1] <= last[1] && gain[0] <= last[0]);
            assert!(gain[0] > gain[1] && gain[1] > gain[2], "{gain:?}");
            last = gain;
        }
        // Deep night keeps about a fifth of the blue and well over half of the red.
        let deep = gain_for(100.0);
        assert!(deep[2] < 0.2 && deep[0] > 0.7, "{deep:?}");
        assert!(gain_for(0.0)[2] > 0.7);
        assert_eq!(gain_for(-30.0), gain_for(0.0));
        assert_eq!(gain_for(400.0), gain_for(100.0));
    }

    #[test]
    fn software_frames_scale_each_channel_and_keep_alpha() {
        set(true, 100.0);
        let mut pixels = [255, 255, 255, 255, 0, 0, 0, 200];
        filter_pixels(&mut pixels);
        assert_eq!(pixels, [255, 255, 255, 255, 0, 0, 0, 200], "off until frames opt in");
        update(|state| state.software_frames = true);
        let gain = gain_for(100.0);
        filter_pixels(&mut pixels);
        assert_eq!(pixels[0], (255.0 * gain[0]).round() as u8);
        assert_eq!(pixels[2], (255.0 * gain[2]).round() as u8);
        assert_eq!(pixels[3], 255);
        assert_eq!(&pixels[4..], [0, 0, 0, 200]);
        set(false, 100.0);
        let mut white = [255, 255, 255, 255];
        filter_pixels(&mut white);
        assert_eq!(white, [255, 255, 255, 255]);
        update(|state| state.software_frames = false);
    }
}
