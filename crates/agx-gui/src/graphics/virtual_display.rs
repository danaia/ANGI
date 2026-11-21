//! Resizable software display for VNC-only sessions, independent of HDMI modes.
//! Rendering is demand-driven by the remote capture timer; there is no scanout buffer.
use slint::platform::{
    EventLoopProxy, Platform, WindowAdapter,
    software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
};
use std::{
    rc::Rc,
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};

enum Event {
    Invoke(Box<dyn FnOnce() + Send>),
    Quit,
}

struct Display {
    sender: Sender<Event>,
    receiver: Receiver<Event>,
}

struct Proxy(Sender<Event>);

impl EventLoopProxy for Proxy {
    fn quit_event_loop(&self) -> Result<(), slint::EventLoopError> {
        self.0
            .send(Event::Quit)
            .map_err(|_| slint::EventLoopError::EventLoopTerminated)
    }

    fn invoke_from_event_loop(
        &self,
        event: Box<dyn FnOnce() + Send>,
    ) -> Result<(), slint::EventLoopError> {
        self.0
            .send(Event::Invoke(event))
            .map_err(|_| slint::EventLoopError::EventLoopTerminated)
    }
}

impl Platform for Display {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        window.set_size(slint::PhysicalSize::new(1280, 720));
        Ok(window)
    }

    fn run_event_loop(&self) -> Result<(), slint::PlatformError> {
        loop {
            slint::platform::update_timers_and_animations();
            let timeout = slint::platform::duration_until_next_timer_update()
                .unwrap_or(Duration::from_millis(50))
                .min(Duration::from_millis(50));
            match self.receiver.recv_timeout(timeout) {
                Ok(Event::Invoke(event)) => event(),
                Ok(Event::Quit) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }

    fn new_event_loop_proxy(&self) -> Option<Box<dyn EventLoopProxy>> {
        Some(Box::new(Proxy(self.sender.clone())))
    }
}

pub(super) fn initialize() -> Result<(), slint::platform::SetPlatformError> {
    let (sender, receiver) = mpsc::channel();
    slint::platform::set_platform(Box::new(Display { sender, receiver }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::ComponentHandle;

    #[test]
    fn virtual_window_renders_at_the_requested_size_and_runs_cross_thread_events() {
        initialize().unwrap();
        let ui = crate::AppWindow::new().unwrap();
        ui.show().unwrap();
        for (w, h) in [(1280, 720), (1600, 900), (800, 600)] {
            ui.window().set_size(slint::PhysicalSize::new(w, h));
            let snapshot = ui.window().take_snapshot().unwrap();
            assert_eq!((snapshot.width(), snapshot.height()), (w, h));
            assert!(
                snapshot
                    .as_bytes()
                    .chunks_exact(4)
                    .any(|p| p[..3] != [0, 0, 0])
            );
        }
        let theme = ui.global::<crate::Theme>();
        theme.set_dark(false);
        let light = ui.window().take_snapshot().unwrap();
        theme.set_dark(true);
        let dark = ui.window().take_snapshot().unwrap();
        assert_ne!(light.as_bytes(), dark.as_bytes());
        let weak = ui.as_weak();
        let worker = std::thread::spawn(move || {
            weak.upgrade_in_event_loop(|ui| {
                ui.window().set_size(slint::PhysicalSize::new(1024, 768));
                slint::quit_event_loop().unwrap();
            })
            .unwrap();
        });
        slint::run_event_loop().unwrap();
        worker.join().unwrap();
        assert_eq!(ui.window().size(), slint::PhysicalSize::new(1024, 768));
    }
}
