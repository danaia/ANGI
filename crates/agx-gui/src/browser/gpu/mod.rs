//! Native GPU presentation; software/VNC keeps the existing CPU frame transport.
#[cfg(feature = "kms")]
mod completion;
#[cfg(feature = "kms")]
mod egl;
#[cfg(feature = "kms")]
mod renderer;
pub(super) mod transport;
use std::sync::atomic::{AtomicBool, Ordering};
static AVAILABLE: AtomicBool = AtomicBool::new(false);
pub(super) fn available() -> bool {
    AVAILABLE.load(Ordering::Relaxed) && std::env::var("AGX_WEB_GPU").as_deref() != Ok("off")
}
#[cfg(feature = "kms")]
pub(super) fn install(ui: &crate::AppWindow, app: &std::rc::Rc<std::cell::RefCell<crate::App>>) {
    use crate::Web;
    use slint::{ComponentHandle, Image, RenderingState};
    if std::env::var("SLINT_BACKEND").as_deref() != Ok("linuxkms-femtovg") {
        return;
    }
    let (weak, state) = (ui.as_weak(), app.clone());
    let mut renderer: Option<renderer::Renderer> = None;
    // Night light shares this render hook: Slint allows only one.
    let mut night_light: Option<crate::night_light::gl::Pass> = None;
    let result = ui
        .window()
        .set_rendering_notifier(move |event, api| match event {
            RenderingState::RenderingSetup => {
                night_light = crate::night_light::gl::setup(&weak, api);
                match unsafe { renderer::Renderer::new(api) } {
                    Ok(gpu) => {
                        renderer = Some(gpu);
                        AVAILABLE.store(true, Ordering::Relaxed);
                        eprintln!("agx-web: native GPU bridge ready");
                    }
                    Err(error) => {
                        eprintln!("agx-web: GPU bridge unavailable: {error}; using CPU frames")
                    }
                }
            }
            RenderingState::BeforeRendering => {
                let Some(ui) = weak.upgrade() else { return };
                let Some(gpu) = renderer.as_mut() else { return };
                let Ok(app) = state.try_borrow() else { return };
                let Some(engine) = &app.browser.engine else {
                    // Closing Web must return its GPU storage as well as its processes.
                    if gpu.has_buffers() {
                        ui.global::<Web>().set_frame(Image::default());
                        unsafe {
                            gpu.destroy();
                        }
                    }
                    return;
                };
                unsafe {
                    gpu.prepare();
                }
                if gpu.pending() {
                    return;
                }
                let (frame, floor) = {
                    let mut updates = engine.updates.lock().unwrap();
                    let floor = updates.tabs.as_ref().map_or(app.browser.frame_floor, |tabs| tabs.frame_floor);
                    (updates.gpu.take(), floor)
                };
                if let Some(frame) = frame {
                    if !app.browser.visible || frame.sequence < floor {
                        engine.send(11, frame.sequence.to_be_bytes().to_vec());
                        return;
                    }
                    let (width, height, sequence) = (frame.width, frame.height, frame.sequence);
                    match unsafe { gpu.copy(frame, engine.commands()) } {
                        Ok(image) => {
                            app.browser.frame_sequence.set(sequence);
                            let web = ui.global::<Web>();
                            web.set_frame_width(width as i32);
                            web.set_frame_height(height as i32);
                            web.set_frame(image);
                            web.set_ready(true);
                        }
                        Err(error) => {
                            AVAILABLE.store(false, Ordering::Relaxed);
                            eprintln!(
                                "agx-web: GPU frame failed: {error}; switching to CPU frames"
                            );
                        }
                    }
                }
            }
            RenderingState::AfterRendering => {
                if let Some(pass) = &night_light {
                    unsafe { pass.draw() };
                }
                if let Some(gpu) = renderer.as_mut() {
                    if let Err(error) = unsafe { gpu.after_render() } {
                        AVAILABLE.store(false, Ordering::Relaxed);
                        eprintln!("agx-web: GPU completion failed: {error}");
                        unsafe {
                            gpu.fallback();
                        }
                    }
                    if gpu.needs_poll() {
                        if let Some(ui) = weak.upgrade() {
                            ui.window().request_redraw();
                        }
                    }
                }
            }
            RenderingState::RenderingTeardown => {
                if let Some(pass) = night_light.take() {
                    unsafe { pass.destroy() };
                }
                AVAILABLE.store(false, Ordering::Relaxed);
                if let Some(ui) = weak.upgrade() {
                    ui.global::<Web>().set_frame(Image::default());
                }
                if let Some(mut gpu) = renderer.take() {
                    unsafe {
                        gpu.destroy();
                    }
                }
            }
            _ => {}
        });
    if let Err(error) = result {
        eprintln!("agx-web: native rendering hook unavailable: {error}");
    }
}
#[cfg(not(feature = "kms"))]
pub(super) fn install(_ui: &crate::AppWindow, _app: &std::rc::Rc<std::cell::RefCell<crate::App>>) {}

/// Opt-in diagnostics compare the CPU fallback and native presentation rate.
pub(super) fn trace_cpu_frame() {
    use std::{
        sync::{Mutex, OnceLock},
        time::Instant,
    };
    static TRACE: OnceLock<bool> = OnceLock::new();
    static STATS: Mutex<Option<(Instant, u32)>> = Mutex::new(None);
    if !*TRACE.get_or_init(|| std::env::var_os("AGX_WEB_TRACE").is_some()) {
        return;
    }
    let mut stats = STATS.lock().unwrap();
    let (since, frames) = stats.get_or_insert((Instant::now(), 0));
    *frames += 1;
    if since.elapsed().as_secs_f64() >= 2.0 {
        eprintln!(
            "agx-web: CPU bridge {:.1} fps",
            *frames as f64 / since.elapsed().as_secs_f64()
        );
        *since = Instant::now();
        *frames = 0;
    }
}
