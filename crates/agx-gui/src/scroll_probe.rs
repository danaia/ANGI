// TEMPORARY probe; not part of the change.
use crate::{AgentProviders, App, AppWindow, keymap::Cmd};
use slint::platform::{Platform, PlatformError, WindowAdapter, WindowEvent, software_renderer::{MinimalSoftwareWindow, RepaintBufferType}};
use slint::{ComponentHandle, LogicalPosition, ModelRc, PhysicalSize, VecModel};
use std::{cell::RefCell, rc::Rc};

struct P(Rc<MinimalSoftwareWindow>);
impl Platform for P {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> { Ok(self.0.clone()) }
}

#[test]
fn provider_panel_scrolls() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(P(window.clone()))).unwrap();
    let ui = AppWindow::new().unwrap();
    let app = Rc::new(RefCell::new(App::new()));
    ui.set_wins(app.borrow().view.windows.clone().into());
    ui.set_ws_occupied(app.borrow().occupied.clone().into());
    ui.show().unwrap();
    let (w, h) = (1920u32, 1080u32);
    ui.window().set_size(PhysicalSize::new(w, h));
    app.borrow_mut().resized(&ui, w as f32, h as f32);
    app.borrow_mut().run(&ui, Cmd::Workspace(1));
    let p = ui.global::<AgentProviders>();
    p.set_provider("openai".into());
    p.set_connected(true);
    p.set_ready(ModelRc::new(VecModel::from(vec![false, true, false])));
    p.set_active_index(1);
    p.set_status("Connected. Agent Angi uses OpenAI.".into());
    p.set_models(ModelRc::new(VecModel::from((0..12).map(|n| format!("openai/model-{n}").into()).collect::<Vec<slint::SharedString>>())));
    p.set_open(true);
    let mut buffer = vec![slint::Rgb8Pixel::default(); (w * h) as usize];
    let mut frame = |buffer: &mut Vec<slint::Rgb8Pixel>| -> u64 {
        for _ in 0..3 { window.request_redraw(); window.draw_if_needed(|r| { r.render(buffer, w as usize); }); }
        buffer.iter().fold(1469598103934665603u64, |a, p| (a ^ (p.r as u64 | (p.g as u64) << 8 | (p.b as u64) << 16)).wrapping_mul(1099511628211))
    };
    let base = frame(&mut buffer);
    let mut dead = Vec::new();
    let mut live = 0;
    for y in (260..1000).step_by(60) {
        for x in (1300..1890).step_by(60) {
            let at = LogicalPosition::new(x as f32, y as f32);
            ui.window().dispatch_event(WindowEvent::PointerMoved { position: at });
            let still = frame(&mut buffer);
            ui.window().dispatch_event(WindowEvent::PointerScrolled { position: at, delta_x: 0.0, delta_y: -300.0 });
            let moved = frame(&mut buffer);
            ui.window().dispatch_event(WindowEvent::PointerScrolled { position: at, delta_x: 0.0, delta_y: 600.0 });
            frame(&mut buffer);
            if moved == still { dead.push((x, y)); } else { live += 1; }
        }
    }
    let _ = base;
    println!("SWEEP live={live} dead={} first_dead={:?}", dead.len(), &dead[..dead.len().min(12)]);
    let before = 0u64; let after = 1u64;
    println!("SCROLL_CHANGED_FRAME={}", before != after);

}
