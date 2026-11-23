//! Route native mouse input and bridge Super keys on LinuxKMS.
use slint::{
    LogicalPosition, LogicalSize,
    platform::{Key, WindowEvent},
};

fn super_event(code: u32, down: bool) -> Option<WindowEvent> {
    // Linux input-event-codes.h: KEY_LEFTMETA / KEY_RIGHTMETA.
    let text = match code {
        125 => Key::Meta.into(),
        126 => Key::MetaR.into(),
        _ => return None,
    };
    Some(if down {
        WindowEvent::KeyPressed { text }
    } else {
        WindowEvent::KeyReleased { text }
    })
}

#[derive(Default)]
struct PointerMotion {
    position: Option<LogicalPosition>,
    pending: bool,
}
impl PointerMotion {
    fn get(&self, size: LogicalSize) -> LogicalPosition {
        let position = self
            .position
            .unwrap_or(LogicalPosition::new(size.width / 2.0, size.height / 2.0));
        LogicalPosition::new(
            position.x.clamp(0.0, size.width),
            position.y.clamp(0.0, size.height),
        )
    }
    fn relative(&mut self, dx: f64, dy: f64, size: LogicalSize) -> bool {
        let position = self.get(size);
        self.absolute(position.x + dx as f32, position.y + dy as f32, size)
    }
    fn absolute(&mut self, x: f32, y: f32, size: LogicalSize) -> bool {
        self.position = Some(LogicalPosition::new(
            x.clamp(0.0, size.width),
            y.clamp(0.0, size.height),
        ));
        !std::mem::replace(&mut self.pending, true)
    }
    fn take_pending(&mut self) -> Option<LogicalPosition> {
        if std::mem::take(&mut self.pending) {
            self.position
        } else {
            None
        }
    }
}
fn scroll_event(
    position: LogicalPosition,
    horizontal: f64,
    vertical: f64,
    wheel: bool,
) -> Option<WindowEvent> {
    // v120 wheel units: 120 = one notch = 60 logical pixels, like the VNC path.
    // Libinput positive is down/right; Slint positive is content motion up/left.
    let scale = if wheel { 0.5 } else { 1.0 };
    let scale = scale * crate::settings::scroll_factor();
    let delta_x = (-horizontal * scale) as f32;
    let delta_y = (-vertical * scale) as f32;
    if !delta_x.is_finite() || !delta_y.is_finite() || (delta_x == 0.0 && delta_y == 0.0) {
        return None;
    }
    Some(WindowEvent::PointerScrolled {
        position,
        delta_x,
        delta_y,
    })
}

#[cfg(feature = "kms")]
mod native {
    use super::{PointerMotion, scroll_event, super_event};
    use input::{
        Event,
        event::{
            KeyboardEvent, PointerEvent,
            keyboard::{KeyState, KeyboardEventTrait},
            pointer::{Axis, ButtonState, PointerScrollEvent},
        },
    };
    use slint::{
        ComponentHandle, Timer, Window,
        platform::{PointerEventButton, WindowEvent},
    };
    use std::{cell::RefCell, rc::Rc, time::Duration};
    type Target = Rc<dyn Fn(&Event) -> bool>;
    thread_local! { static TARGET: RefCell<Option<Target>> = RefCell::new(None); }
    fn smooth_values(event: &impl PointerScrollEvent) -> (f64, f64) {
        let horizontal = if event.has_axis(Axis::Horizontal) {
            event.scroll_value(Axis::Horizontal)
        } else {
            0.0
        };
        let vertical = if event.has_axis(Axis::Vertical) {
            event.scroll_value(Axis::Vertical)
        } else {
            0.0
        };
        (horizontal, vertical)
    }
    fn flush_motion(window: &Window, pointer: &RefCell<PointerMotion>) {
        let position = pointer.borrow_mut().take_pending();
        if let Some(position) = position {
            crate::key_bindings::dispatch_pointer(window, 2, 3, position);
            window.dispatch_event(WindowEvent::PointerMoved { position });
        }
    }

    pub(in crate::graphics) fn attach<C: ComponentHandle + 'static>(ui: &C) {
        let weak = ui.as_weak();
        let pointer = Rc::new(RefCell::new(PointerMotion::default()));
        TARGET.with(|target| {
            *target.borrow_mut() = Some(Rc::new(move |input| {
                let Some(ui) = weak.upgrade() else {
                    return false;
                };
                let window = ui.window();
                let size = window.size().to_logical(window.scale_factor());
                let motion = match input {
                    Event::Pointer(PointerEvent::Motion(event)) => {
                        Some(pointer.borrow_mut().relative(event.dx(), event.dy(), size))
                    }
                    Event::Pointer(PointerEvent::MotionAbsolute(event)) => {
                        Some(pointer.borrow_mut().absolute(
                            event.absolute_x_transformed(size.width as u32) as f32,
                            event.absolute_y_transformed(size.height as u32) as f32,
                            size,
                        ))
                    }
                    _ => None,
                };
                if let Some(schedule) = motion {
                    if schedule {
                        let pointer = pointer.clone();
                        let weak = weak.clone();
                        // Run after libinput drains the available events, before the
                        // next frame. A high-polling mouse can send many moves per frame.
                        Timer::single_shot(Duration::ZERO, move || {
                            if let Some(ui) = weak.upgrade() {
                                flush_motion(ui.window(), &pointer);
                            }
                        });
                    }
                    return true;
                }
                // Deliver queued motion before buttons, scroll, or modifiers that
                // can change how the desktop interprets the move.
                flush_motion(window, &pointer);
                let event = match input {
                    Event::Keyboard(KeyboardEvent::Key(key)) => {
                        let pressed = key.key_state() == KeyState::Pressed;
                        use crate::sound::VolumeAction;
                        let volume = match key.key() {
                            113 => Some(VolumeAction::Mute),
                            114 => Some(VolumeAction::Step(-5)),
                            115 => Some(VolumeAction::Step(5)),
                            _ => None,
                        };
                        if let Some(action) = volume {
                            return crate::sound::media_key(action, pressed);
                        }
                        let Some(event) = super_event(key.key(), pressed) else {
                            return false;
                        };
                        Some(event)
                    }
                    Event::Pointer(PointerEvent::Button(event)) => {
                        let (binding, button) = match event.button() {
                            0x110 => (Some(0), PointerEventButton::Left),
                            0x111 => (Some(1), PointerEventButton::Right),
                            0x112 => (Some(2), PointerEventButton::Middle),
                            0x113 | 0x116 => (Some(4), PointerEventButton::Back),
                            0x114 | 0x115 => (Some(5), PointerEventButton::Forward),
                            _ => (None, PointerEventButton::Other),
                        };
                        let pressed = event.button_state() == ButtonState::Pressed;
                        let position = pointer.borrow().get(size);
                        if binding.is_some_and(|binding| {
                            crate::key_bindings::dispatch_pointer(
                                window,
                                i32::from(!pressed),
                                binding,
                                position,
                            )
                        }) {
                            return true;
                        }
                        Some(if pressed {
                            WindowEvent::PointerPressed { position, button }
                        } else {
                            WindowEvent::PointerReleased { position, button }
                        })
                    }
                    Event::Pointer(PointerEvent::ScrollWheel(event)) => {
                        let horizontal = if event.has_axis(Axis::Horizontal) {
                            event.scroll_value_v120(Axis::Horizontal)
                        } else {
                            0.0
                        };
                        let vertical = if event.has_axis(Axis::Vertical) {
                            event.scroll_value_v120(Axis::Vertical)
                        } else {
                            0.0
                        };
                        scroll_event(pointer.borrow().get(size), horizontal, vertical, true)
                    }
                    Event::Pointer(PointerEvent::ScrollFinger(event)) => {
                        let (horizontal, vertical) = smooth_values(event);
                        scroll_event(pointer.borrow().get(size), horizontal, vertical, false)
                    }
                    Event::Pointer(PointerEvent::ScrollContinuous(event)) => {
                        let (horizontal, vertical) = smooth_values(event);
                        scroll_event(pointer.borrow().get(size), horizontal, vertical, false)
                    }
                    // Libinput emits the deprecated Axis event alongside each modern
                    // scroll event. Consume it without delivering the same motion twice.
                    #[allow(deprecated)]
                    Event::Pointer(PointerEvent::Axis(_)) => return true,
                    _ => return false,
                };
                if let Some(event) = event {
                    if let WindowEvent::PointerScrolled { position, .. } = &event {
                        // Like RFB, establish hover first so desktop/browser input
                        // uses the actual native wheel target, even before a move.
                        window.dispatch_event(WindowEvent::PointerMoved {
                            position: *position,
                        });
                    }
                    window.dispatch_event(event);
                }
                true
            }))
        });
    }
    pub(in crate::graphics) fn filter(event: &Event) -> bool {
        crate::settings::device_event(event);
        // Release the target borrow before dispatching any UI callbacks.
        let target = TARGET.with(|target| target.borrow().clone());
        target.is_some_and(|target| target(event))
    }
}
#[cfg(feature = "kms")]
pub(super) use native::{attach, filter};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wheel_notches_and_high_resolution_steps_match_vnc_direction_and_distance() {
        let position = LogicalPosition::new(300.0, 200.0);
        for (horizontal, vertical, expected_x, expected_y) in [
            (0.0, 120.0, 0.0, -60.0),
            (120.0, 0.0, -60.0, 0.0),
            (-120.0, -120.0, 60.0, 60.0),
            (0.0, 30.0, 0.0, -15.0),
        ] {
            assert!(matches!(scroll_event(position, horizontal, vertical, true),
                Some(WindowEvent::PointerScrolled { position: target, delta_x, delta_y })
                if target == position && delta_x == expected_x && delta_y == expected_y));
        }
        assert!(matches!(
            scroll_event(position, 2.5, -4.0, false),
            Some(WindowEvent::PointerScrolled {
                delta_x: -2.5,
                delta_y: 4.0,
                ..
            })
        ));
        assert!(scroll_event(position, 0.0, 0.0, true).is_none());
        assert!(scroll_event(position, f64::INFINITY, 1.0, true).is_none());
    }
    #[test]
    fn scroll_targets_follow_native_relative_absolute_and_clamped_cursor_motion() {
        let size = LogicalSize::new(1000.0, 700.0);
        let mut pointer = PointerMotion::default();
        assert_eq!(pointer.get(size), LogicalPosition::new(500.0, 350.0));
        pointer.relative(100.0, -50.0, size);
        assert_eq!(pointer.get(size), LogicalPosition::new(600.0, 300.0));
        pointer.relative(-2000.0, 2000.0, size);
        assert_eq!(pointer.get(size), LogicalPosition::new(0.0, 700.0));
        pointer.absolute(250.0, 100.0, size);
        assert_eq!(pointer.get(size), LogicalPosition::new(250.0, 100.0));
        assert_eq!(
            pointer.get(LogicalSize::new(200.0, 90.0)),
            LogicalPosition::new(200.0, 90.0)
        );
    }

    #[test]
    fn native_motion_batches_keep_the_latest_position_across_input_boundaries() {
        let size = LogicalSize::new(1920.0, 1080.0);
        let mut motion = PointerMotion::default();
        assert!(motion.relative(-2000.0, 0.0, size));
        assert!(!motion.relative(12.0, 4.0, size));
        assert_eq!(
            motion.take_pending(),
            Some(LogicalPosition::new(12.0, 544.0))
        );
        assert_eq!(motion.take_pending(), None);

        assert!(motion.absolute(90.0, 120.0, size));
        assert!(!motion.relative(2.0, -1.0, size));
        assert_eq!(
            motion.take_pending(),
            Some(LogicalPosition::new(92.0, 119.0))
        );
        assert_eq!(motion.get(size), LogicalPosition::new(92.0, 119.0));
    }
    #[test]
    fn both_super_keys_have_matching_press_and_release_events() {
        for (code, key) in [(125, Key::Meta), (126, Key::MetaR)] {
            assert!(
                matches!(super_event(code, true), Some(WindowEvent::KeyPressed { text }) if text == slint::SharedString::from(key))
            );
            assert!(
                matches!(super_event(code, false), Some(WindowEvent::KeyReleased { text }) if text == slint::SharedString::from(key))
            );
        }
        for code in [0, 30, 42, 54, 56, 57, 97, 100] {
            assert!(super_event(code, true).is_none());
        }
    }
    #[test]
    fn physical_super_space_sets_and_clears_the_ui_modifier() {
        use slint::{
            ComponentHandle,
            platform::{
                Platform, PlatformError, WindowAdapter,
                software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
            },
        };
        use std::{cell::RefCell, rc::Rc};
        struct TestPlatform(Rc<MinimalSoftwareWindow>);
        impl Platform for TestPlatform {
            fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
                Ok(self.0.clone())
            }
        }
        let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
        slint::platform::set_platform(Box::new(TestPlatform(window))).unwrap();
        let ui = crate::AppWindow::new().unwrap();
        ui.show().unwrap();
        ui.invoke_focus_desktop();
        let modifiers = Rc::new(RefCell::new(Vec::new()));
        let received = modifiers.clone();
        ui.on_key(move |name, _, _, _, meta| {
            if name == "space" {
                received.borrow_mut().push(meta);
            }
            true
        });
        for code in [125, 126] {
            ui.window().dispatch_event(super_event(code, true).unwrap());
            ui.window()
                .dispatch_event(WindowEvent::KeyPressed { text: " ".into() });
            ui.window()
                .dispatch_event(WindowEvent::KeyReleased { text: " ".into() });
            ui.window()
                .dispatch_event(super_event(code, false).unwrap());
            ui.window()
                .dispatch_event(WindowEvent::KeyPressed { text: " ".into() });
            ui.window()
                .dispatch_event(WindowEvent::KeyReleased { text: " ".into() });
        }
        assert_eq!(*modifiers.borrow(), vec![true, false, true, false]);
    }
}
