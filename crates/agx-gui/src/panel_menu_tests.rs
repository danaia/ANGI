use crate::{App, AppWindow, Theme, keymap::Cmd, mods, wm::Kind};
use slint::platform::{
    Key, Platform, PlatformError, PointerEventButton, WindowAdapter, WindowEvent,
    software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
};
use slint::{ComponentHandle, LogicalPosition, Model, PhysicalSize};
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
    ui.set_ws_occupied(app.borrow().occupied.clone().into());
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
    hook!(on_header_menu_action, |state, ui, index| state
        .choose_header_action(&ui, index));
    hook!(on_header_menu_dismiss, |state, ui| state
        .dismiss_header_menu(&ui));
    hook!(on_workspace_menu, |state, ui, index, x, y| state
        .open_workspace_menu(&ui, index, x, y));
    hook!(on_key, |state, ui, name, ctrl, alt, shift, meta| state.key(
        &ui,
        &name,
        mods(ctrl, alt, shift, meta)
    ));

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
        assert!(ui.get_header_menu_open());
        assert!(!ui.get_menu_open());
        key(Key::Escape);
        assert!(!ui.get_header_menu_open());
        assert!(app.borrow().desk.is_open(Kind::Overview));
    }
    click(x, y, PointerEventButton::Right);
    click(30., 35., PointerEventButton::Left);
    assert!(!ui.get_header_menu_open());
    assert!(
        !ui.get_menu_open(),
        "the dismissing click must not open Apps"
    );

    click(x, y, PointerEventButton::Right);
    click(x + 30., y + 92., PointerEventButton::Left);
    assert!(!ui.get_header_menu_open());
    assert_eq!(app.borrow().desk.current(), 0);
    assert!(app.borrow().desk.scene().rect_of(Kind::Overview).is_none());

    app.borrow_mut().run(&ui, Cmd::Workspace(2));
    let panel = app.borrow().desk.scene().rect_of(Kind::Overview).unwrap();
    render();
    click(panel.x + 36., panel.y + 22., PointerEventButton::Right);
    assert!(ui.get_header_menu_open());
    key(Key::UpArrow);
    key(Key::Return);
    assert!(!ui.get_header_menu_open());
    assert!(!app.borrow().desk.is_open(Kind::Overview));

    // Both a workspace pill and an otherwise inactive part of the top bar open the same menu.
    for (x, y) in [(81., 34.), (500., 34.), (38., 34.), (601., 34.)] {
        let before = app.borrow().desk.workspace_count();
        click(x, y, PointerEventButton::Right);
        assert!(ui.get_header_menu_open());
        key(Key::Return);
        assert!(!ui.get_header_menu_open());
        assert_eq!(app.borrow().desk.workspace_count(), before + 1);
        assert_eq!(ui.get_ws_occupied().row_count(), before + 1);
        assert_eq!(app.borrow().desk.current(), 2);
    }

    app.borrow_mut().run(&ui, Cmd::Workspace(0));
    app.borrow_mut().desk.focus(Kind::Cpu);
    let chord = |text: &str| {
        for modifier in [Key::Meta, Key::Shift] {
            ui.window().dispatch_event(WindowEvent::KeyPressed {
                text: modifier.into(),
            });
        }
        ui.window()
            .dispatch_event(WindowEvent::KeyPressed { text: text.into() });
        ui.window()
            .dispatch_event(WindowEvent::KeyReleased { text: text.into() });
        for modifier in [Key::Shift, Key::Meta] {
            ui.window().dispatch_event(WindowEvent::KeyReleased {
                text: modifier.into(),
            });
        }
    };
    chord(")");
    assert_eq!(app.borrow().desk.workspace_count(), 10);
    assert_eq!(ui.get_ws_occupied().row_count(), 10);
    assert_eq!(app.borrow().desk.current(), 9);
    assert_eq!(app.borrow().desk.focused(), Some(Kind::Cpu));

    // A ten-workspace menu still fits on the minimum-height desktop.
    let cpu = app.borrow().desk.scene().rect_of(Kind::Cpu).unwrap();
    render();
    click(cpu.x + 32., cpu.y + 22., PointerEventButton::Right);
    assert!(ui.get_header_menu_open());
    assert_eq!(ui.get_header_menu_rows().row_count(), 10);
    key(Key::UpArrow);
    key(Key::Return);
    assert!(!app.borrow().desk.is_open(Kind::Cpu));

    // With all ten workspaces present, a pill still offers removal.
    click(81., 34., PointerEventButton::Right);
    let rows = ui.get_header_menu_rows();
    assert_eq!(rows.row_count(), 1);
    assert_eq!(rows.row_data(0).unwrap().label.as_str(), "Remove workspace");
    key(Key::Return);
    assert_eq!(app.borrow().desk.current(), 8);
    assert_eq!(ui.get_ws_occupied().row_count(), 9);
    app.borrow_mut().run(&ui, Cmd::Workspace(0));
    let desktop = &app.borrow().desk;
    assert!(desktop.scene().rect_of(Kind::Settings).is_some());
    assert!(desktop.scene().rect_of(Kind::AgentAngi).is_some());
}
