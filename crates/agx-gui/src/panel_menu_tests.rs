use crate::{App, AppWindow, Theme, keymap::Cmd, mods, wm::Kind};
use slint::platform::{
    Key, Platform, PlatformError, PointerEventButton, WindowAdapter, WindowEvent,
    software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
};
use slint::{ComponentHandle, LogicalPosition, PhysicalSize};
use std::{cell::RefCell, rc::Rc};

struct TestPlatform(Rc<MinimalSoftwareWindow>);

impl Platform for TestPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.0.clone())
    }
}

#[test]
fn header_menu_sends_and_closes_the_clicked_window() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(TestPlatform(window.clone()))).unwrap();
    let ui = AppWindow::new().unwrap();
    ui.window().set_size(PhysicalSize::new(640, 400));
    ui.show().unwrap();
    let app = Rc::new(RefCell::new(App::new()));
    ui.set_wins(app.borrow().view.windows.clone().into());
    app.borrow_mut().resized(&ui, 640., 400.);

    macro_rules! hook {
        ($callback:ident, |$state:ident, $ui:ident $(, $arg:ident)*| $body:expr) => {{
            let (state, weak) = (app.clone(), ui.as_weak());
            ui.$callback(move |$($arg),*| {
                let mut $state = state.borrow_mut();
                let $ui = weak.upgrade().unwrap();
                $body
            });
        }};
    }
    hook!(on_pointer, |state,
                       ui,
                       kind,
                       button,
                       x,
                       y,
                       ctrl,
                       alt,
                       shift,
                       meta| state.pointer(
        &ui,
        kind,
        button,
        x,
        y,
        mods(ctrl, alt, shift, meta)
    ));
    hook!(on_panel_menu_move, |state, ui, index| state
        .panel_menu_command(
            &ui,
            Cmd::MoveToWorkspace {
                ws: index as usize,
                follow: false
            }
        ));
    hook!(on_panel_menu_close, |state, ui| state
        .panel_menu_command(&ui, Cmd::Close));
    hook!(on_panel_menu_dismiss, |state, ui| state
        .dismiss_panel_menu(&ui));

    let render = || {
        window.draw_if_needed(|renderer| {
            renderer.render(&mut vec![slint::Rgb8Pixel::default(); 640 * 400], 640);
        });
    };
    let click = |x, y, button| {
        let position = LogicalPosition::new(x, y);
        ui.window()
            .dispatch_event(WindowEvent::PointerMoved { position });
        ui.window()
            .dispatch_event(WindowEvent::PointerPressed { position, button });
        ui.window()
            .dispatch_event(WindowEvent::PointerReleased { position, button });
        render();
    };
    let key = |key: Key| {
        ui.window()
            .dispatch_event(WindowEvent::KeyPressed { text: key.into() });
        ui.window()
            .dispatch_event(WindowEvent::KeyReleased { text: key.into() });
    };
    let panel = app.borrow().desk.rect_of(Kind::Overview).unwrap();
    let (x, y) = (panel.x + 36., panel.y + 22.);
    render();
    for dark in [false, true] {
        ui.global::<Theme>().set_dark(dark);
        click(x, y, PointerEventButton::Right);
        assert!(ui.get_panel_menu_open());
        assert!(!ui.get_menu_open());
        key(Key::Escape);
        assert!(!ui.get_panel_menu_open());
        assert!(app.borrow().desk.is_open(Kind::Overview));
    }
    click(x, y, PointerEventButton::Right);
    click(30., 35., PointerEventButton::Left);
    assert!(!ui.get_panel_menu_open());
    assert!(
        !ui.get_menu_open(),
        "the dismissing click must not open Apps"
    );

    click(x, y, PointerEventButton::Right);
    click(x + 30., y + 92., PointerEventButton::Left);
    assert!(!ui.get_panel_menu_open());
    assert_eq!(app.borrow().desk.current(), 0);
    assert!(app.borrow().desk.scene().rect_of(Kind::Overview).is_none());

    app.borrow_mut().run(&ui, Cmd::Workspace(2));
    let panel = app.borrow().desk.scene().rect_of(Kind::Overview).unwrap();
    render();
    click(panel.x + 36., panel.y + 22., PointerEventButton::Right);
    assert!(ui.get_panel_menu_open());
    key(Key::UpArrow);
    key(Key::Return);
    assert!(!ui.get_panel_menu_open());
    assert!(!app.borrow().desk.is_open(Kind::Overview));
}
