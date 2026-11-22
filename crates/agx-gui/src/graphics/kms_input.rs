//! Bridge Super keys and apply ANGI's scroll preferences on LinuxKMS.
//! Movement/clicks still go through Slint; mirror its cursor position for scroll targeting.
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
struct PointerPosition(Option<LogicalPosition>);
impl PointerPosition {
    fn get(&self, size: LogicalSize) -> LogicalPosition {
        let position = self
            .0
            .unwrap_or(LogicalPosition::new(size.width / 2.0, size.height / 2.0));
        LogicalPosition::new(
            position.x.clamp(0.0, size.width),
            position.y.clamp(0.0, size.height),
        )
    }
    fn relative(&mut self, dx: f64, dy: f64, size: LogicalSize) {
        let position = self.get(size);
        self.0 = Some(LogicalPosition::new(
            (position.x + dx as f32).clamp(0.0, size.width),
            (position.y + dy as f32).clamp(0.0, size.height),
        ));
    }
    fn absolute(&mut self, x: f64, y: f64, size: LogicalSize) {
        self.0 = Some(LogicalPosition::new(
            (x as f32).clamp(0.0, size.width),
            (y as f32).clamp(0.0, size.height),
        ));
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
    use super::{PointerPosition, scroll_event, super_event};
    use input::{
        Event,
        event::{
            KeyboardEvent, PointerEvent,
            keyboard::{KeyState, KeyboardEventTrait},
            pointer::{Axis, PointerScrollEvent},
        },
    };
    use slint::{ComponentHandle, platform::WindowEvent};
    use std::{cell::RefCell, rc::Rc};
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
    pub(in crate::graphics) fn attach<C: ComponentHandle + 'static>(ui: &C) {
        let weak = ui.as_weak();
        let pointer = RefCell::new(PointerPosition::default());
        TARGET.with(|target| {
            *target.borrow_mut() = Some(Rc::new(move |input| {
                let Some(ui) = weak.upgrade() else {
                    return false;
                };
                let window = ui.window();
                let size = window.size().to_logical(window.scale_factor());
                let event = match input {
                    Event::Keyboard(KeyboardEvent::Key(key)) => {
                        let Some(event) =
                            super_event(key.key(), key.key_state() == KeyState::Pressed)
                        else {
                            return false;
                        };
                        Some(event)
                    }
                    Event::Pointer(PointerEvent::Motion(event)) => {
                        pointer.borrow_mut().relative(event.dx(), event.dy(), size);
                        return false;
                    }
                    Event::Pointer(PointerEvent::MotionAbsolute(event)) => {
                        pointer.borrow_mut().absolute(
                            event.absolute_x_transformed(size.width as u32),
                            event.absolute_y_transformed(size.height as u32),
                            size,
                        );
                        return false;
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
        let mut pointer = PointerPosition::default();
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
