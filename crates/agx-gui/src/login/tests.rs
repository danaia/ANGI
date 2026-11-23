use crate::{ComponentHandle, LoginWindow, SessionMode, Theme};
use slint::{
    LogicalPosition, PhysicalSize, SharedString,
    platform::{
        Key, Platform, PlatformError, PointerEventButton, WindowAdapter, WindowEvent,
        software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
    },
    private_unstable_api::re_exports::{
        AccessibilityAction, AccessibleRole, AccessibleStringProperty as A11y, ItemRc, WindowInner,
    },
};
use std::{cell::RefCell, rc::Rc};

struct TestPlatform(Rc<MinimalSoftwareWindow>);

impl Platform for TestPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.0.clone())
    }
}

fn item_named(item: ItemRc, label: &str) -> Option<ItemRc> {
    if item.accessible_string_property(A11y::Label).as_deref() == Some(label) {
        return Some(item);
    }
    let mut child = item.first_child();
    while let Some(current) = child {
        child = current.next_sibling();
        if let Some(found) = item_named(current, label) {
            return Some(found);
        }
    }
    None
}

#[test]
fn selector_supports_keyboard_pointer_and_accessibility_at_both_login_sizes() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(TestPlatform(window.clone()))).unwrap();
    let ui = LoginWindow::new().unwrap();
    ui.show().unwrap();
    let root = || ItemRc::new_root(WindowInner::from_pub(ui.window()).component());
    let key = |text: SharedString| {
        ui.window()
            .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        ui.window()
            .dispatch_event(WindowEvent::KeyReleased { text });
    };
    let sign_ins = Rc::new(RefCell::new(Vec::new()));
    let submitted = sign_ins.clone();
    ui.on_sign_in(move |user, password, mode| submitted.borrow_mut().push((user, password, mode)));

    assert_eq!(ui.get_session_mode(), SessionMode::Native);
    for (width, height) in [(640, 400), (1280, 720)] {
        ui.window().set_size(PhysicalSize::new(width, height));
        for dark in [false, true] {
            ui.global::<Theme>().set_dark(dark);
            ui.set_session_mode(SessionMode::Native);
            ui.invoke_show_login("".into());
            ui.set_wayland_unavailable("".into());
            sign_ins.borrow_mut().clear();
            let render = || {
                window.request_redraw();
                window.draw_if_needed(|renderer| {
                    let mut pixels = vec![slint::Rgb8Pixel::default(); (width * height) as usize];
                    renderer.render(&mut pixels, width as usize);
                });
            };
            render();

            let native = item_named(root(), "Native").unwrap();
            let full = item_named(root(), "Full Desktop").unwrap();
            assert_eq!(native.accessible_role(), AccessibleRole::RadioButton);
            assert_eq!(
                native.accessible_string_property(A11y::Checked).as_deref(),
                Some("true")
            );
            assert_eq!(full.accessible_role(), AccessibleRole::RadioButton);
            assert_eq!(
                full.accessible_string_property(A11y::Checked).as_deref(),
                Some("false")
            );
            assert!(
                full.accessible_string_property(A11y::Description)
                    .unwrap()
                    .contains("Chromium")
            );

            key("a".into());
            key(Key::Tab.into());
            key("p".into());
            key(Key::Tab.into());
            key(Key::RightArrow.into());
            assert_eq!(ui.get_session_mode(), SessionMode::Wayland);
            key(Key::LeftArrow.into());
            assert_eq!(ui.get_session_mode(), SessionMode::Native);
            key(Key::RightArrow.into());
            assert_eq!(
                full.accessible_string_property(A11y::Checked).as_deref(),
                Some("true")
            );
            key(Key::Tab.into());
            key(Key::Return.into());
            assert_eq!(
                *sign_ins.borrow(),
                vec![("a".into(), "p".into(), SessionMode::Wayland)]
            );

            ui.set_busy(true);
            assert_eq!(
                full.accessible_string_property(A11y::Enabled).as_deref(),
                Some("false")
            );
            native.accessible_action(&AccessibilityAction::Default);
            assert_eq!(ui.get_session_mode(), SessionMode::Wayland);
            item_named(root(), "Signing in…")
                .unwrap()
                .accessible_action(&AccessibilityAction::Default);
            assert_eq!(sign_ins.borrow().len(), 1);
            ui.set_busy(false);

            ui.set_session_mode(SessionMode::Native);
            ui.set_wayland_unavailable(
                "Full Desktop is unavailable while a VNC viewer is connected.".into(),
            );
            ui.set_error_text(
                "Username or password is incorrect, or the account is unavailable.".into(),
            );
            render();
            assert_eq!(
                full.accessible_string_property(A11y::Enabled).as_deref(),
                Some("false")
            );
            assert!(
                full.accessible_string_property(A11y::Description)
                    .unwrap()
                    .contains("VNC")
            );
            full.accessible_action(&AccessibilityAction::Default);
            assert_eq!(ui.get_session_mode(), SessionMode::Native);

            let sign_in = item_named(root(), "Sign in to Native").unwrap();
            let bounds = sign_in.geometry();
            let size = bounds.size;
            let top = sign_in.map_to_window(bounds.origin);
            assert!(top.y >= 0. && top.y + size.height <= height as f32);
            let position = LogicalPosition::new(top.x + size.width / 2., top.y + size.height / 2.);
            ui.window().dispatch_event(WindowEvent::PointerPressed {
                position,
                button: PointerEventButton::Left,
            });
            ui.window().dispatch_event(WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Left,
            });
            assert_eq!(
                *sign_ins.borrow(),
                vec![
                    ("a".into(), "p".into(), SessionMode::Wayland),
                    ("a".into(), "".into(), SessionMode::Native),
                ]
            );
        }
    }
}
