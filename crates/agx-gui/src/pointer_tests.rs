use super::*;

#[test]
fn session_dropdown_routes_only_the_four_session_actions() {
    assert_eq!(session_action(0), Some(Cmd::Logout));
    assert_eq!(session_action(1), Some(Cmd::Tui));
    assert_eq!(session_action(2), Some(Cmd::Reboot));
    assert_eq!(session_action(3), Some(Cmd::Shutdown));
    assert_eq!(session_action(-1), None);
    assert_eq!(session_action(4), None);
}

#[test]
fn session_button_opens_dropdown_and_mouse_selects_tui() {
    use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
    use slint::platform::{
        Platform, PlatformError, PointerEventButton, WindowAdapter, WindowEvent,
    };
    struct TestPlatform(Rc<MinimalSoftwareWindow>);
    impl Platform for TestPlatform {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
            Ok(self.0.clone())
        }
    }
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(TestPlatform(window.clone()))).unwrap();
    let ui = AppWindow::new().unwrap();
    ui.window().set_size(slint::PhysicalSize::new(1280, 720));
    ui.show().unwrap();
    let selected = Rc::new(RefCell::new(Vec::new()));
    let result = selected.clone();
    ui.on_session_action(move |index| result.borrow_mut().push(index));
    let render = || {
        window.draw_if_needed(|renderer| {
            renderer.render(&mut vec![slint::Rgb8Pixel::default(); 1280 * 720], 1280);
        });
    };
    let click = |x, y| {
        let position = slint::LogicalPosition::new(x, y);
        ui.window().dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        ui.window().dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
    };
    render();
    // The session button sits at the right end of the top bar pill.
    click(1240.0, 34.0);
    assert!(
        ui.get_session_open(),
        "header button must open the dropdown"
    );
    ui.set_draw_cursor(true);
    render();
    ui.window().dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(1150.0, 115.0),
    });
    assert_eq!(ui.get_cursor_x(), 1150.0);
    assert_eq!(ui.get_cursor_y(), 115.0);
    click(1150.0, 172.0);
    assert!(!ui.get_session_open());
    assert_eq!(*selected.borrow(), vec![1], "third row enters the TUI");
    render();
    // The session button sits at the right end of the top bar pill.
    click(1240.0, 34.0);
    assert!(ui.get_session_open());
    ui.window().dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::Escape.into(),
    });
    assert!(
        !ui.get_session_open(),
        "Escape dismisses without choosing an action"
    );
    assert_eq!(*selected.borrow(), vec![1]);
    render();
    // The session button sits at the right end of the top bar pill.
    click(1240.0, 34.0);
    render();
    {
        let mut terminals = App::new();
        terminals.run(&ui, Cmd::Open(Kind::Terminal));
        let first = terminals.desk.focused().unwrap();
        ui.global::<Terminal>().set_font_size(19.);
        for _ in 0..11 { terminals.run(&ui, Cmd::Open(Kind::Terminal)); }
        assert_eq!(terminals.desk.scene().windows.len(), 16);
        assert_ne!(terminals.desk.focused(), Some(first));
        assert_eq!(ui.global::<Terminal>().get_font_size(), 14.);
        terminals.terminal_select(&ui, first);
        assert_eq!(ui.global::<Terminal>().get_font_size(), 19.);
        ui.global::<Terminal>().set_font_size(14.);
    }
    let settings_app = Rc::new(RefCell::new(App::new()));
    ui.set_wins(ModelRc::from(settings_app.borrow().view.windows.clone()));
    {
        let (weak, state) = (ui.as_weak(), settings_app.clone());
        ui.on_settings_open(move || {
            state
                .borrow_mut()
                .run(&weak.upgrade().unwrap(), Cmd::Open(Kind::Settings));
        });
        let (weak, state) = (ui.as_weak(), settings_app.clone());
        ui.on_pointer(move |kind, button, x, y, ctrl, alt, shift, meta| {
            state.borrow_mut().pointer(
                &weak.upgrade().unwrap(),
                kind,
                button,
                x,
                y,
                mods(ctrl, alt, shift, meta),
            );
        });
    }
    click(1150.0, 92.0);
    assert!(
        settings_app.borrow().desk.is_open(Kind::Settings),
        "Settings opens as a tile from the first menu row"
    );
    render();
    if let Some(path) = std::env::var_os("AGX_SETTINGS_SCREENSHOT") {
        use std::io::Write;
        let mut pixels = vec![slint::Rgb8Pixel::default(); 1280 * 720];
        window.request_redraw();
        window.draw_if_needed(|renderer| {
            renderer.render(&mut pixels, 1280);
        });
        let mut file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
        file.write_all(b"P6\n1280 720\n255\n").unwrap();
        for p in pixels {
            file.write_all(&[p.r, p.g, p.b]).unwrap();
        }
    }
    {
        let (weak, state) = (ui.as_weak(), settings_app.clone());
        ui.global::<MouseSettings>().on_refresh(move || {
            state
                .borrow_mut()
                .run(&weak.upgrade().unwrap(), Cmd::RefreshDesktop);
        });
        let (weak, state) = (ui.as_weak(), settings_app.clone());
        ui.on_confirm(move |yes| state.borrow_mut().confirm(&weak.upgrade().unwrap(), yes));
    }
    let tile = settings_app.borrow().desk.rect_of(Kind::Settings).unwrap();
    click(
        if tile.w - 12. < 500. {
            tile.x + 98.
        } else {
            tile.right() - 98.
        },
        tile.bottom() - 34.,
    );
    assert!(ui.get_confirm_text().contains("stay signed in"));
    assert_eq!(settings_app.borrow().pending, Some(Cmd::RefreshDesktop));
    ui.window().dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::Escape.into(),
    });
    assert!(ui.get_confirm_text().is_empty());
    assert_eq!(settings_app.borrow().pending, None);
    assert!(settings_app.borrow().desk.is_open(Kind::Settings));
    // Tab stays within the Settings controls, and the speed slider accepts arrow keys.
    for _ in 0..1 {
        ui.window().dispatch_event(WindowEvent::KeyPressed {
            text: slint::platform::Key::Tab.into(),
        });
    }
    ui.window().dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::RightArrow.into(),
    });
    let first_speed = ui.global::<MouseSettings>().get_speed();
    assert!(first_speed > 0., "keyboard changes pointer speed");
    for _ in 0..7 {
        ui.window().dispatch_event(WindowEvent::KeyPressed {
            text: slint::platform::Key::Tab.into(),
        });
    }
    ui.window().dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::RightArrow.into(),
    });
    assert!(
        ui.global::<MouseSettings>().get_speed() > first_speed,
        "Tab wraps within Settings"
    );
    ui.window().dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::Escape.into(),
    });
    assert!(
        settings_app.borrow().desk.is_open(Kind::Settings),
        "Escape keeps the tile open"
    );
    // Settings → Bluetooth: pages, keyboard, prompts, and the real service wiring (see bluetooth/tests.rs).
    crate::bluetooth::tests::drive(&ui, &window, &settings_app, &click, &render);
    let settings_rect = settings_app.borrow().desk.rect_of(Kind::Settings).unwrap();
    click(settings_rect.right() - 12., settings_rect.y + 14.);
    assert!(
        !settings_app.borrow().desk.is_open(Kind::Settings),
        "normal window close removes Settings"
    );
    assert_eq!(
        *selected.borrow(),
        vec![1],
        "Settings never triggers a session action"
    );
    render();
    let wheel = Rc::new(RefCell::new(Vec::new()));
    let received = wheel.clone();
    ui.on_scroll(move |dx, dy, _, _, _, _| received.borrow_mut().push((dx, dy)));
    remote::input::dispatch(
        ui.window(),
        remote::input::Input::Pointer {
            x: 600,
            y: 500,
            action: remote::input::PointerAction::Scroll { dx: 1, dy: 1 },
        },
    );
    assert_eq!(
        *wheel.borrow(),
        vec![(-60.0, 60.0)],
        "RFB right/up wheels reach the desktop in Slint's content-motion coordinates"
    );
    let mut app = App::new();
    ui.set_wins(ModelRc::from(app.view.windows.clone()));
    app.desk.open(Kind::Web);
    app.refresh(&ui);
    render();
    let keys = Rc::new(RefCell::new(Vec::new()));
    let received = keys.clone();
    ui.on_key(move |name, _, _, _, meta| {
        received.borrow_mut().push((name.to_string(), meta));
        true
    });
    app.desk.close(Kind::Web);
    app.refresh(&ui);
    render();
    remote::input::dispatch(
        ui.window(),
        remote::input::Input::Key {
            down: true,
            keysym: 0xffeb,
        },
    );
    remote::input::dispatch(
        ui.window(),
        remote::input::Input::Key {
            down: true,
            keysym: 32,
        },
    );
    assert!(
        keys.borrow()
            .iter()
            .any(|(name, meta)| name == "space" && *meta),
        "closing the focused browser restores desktop Super shortcuts"
    );
    for keysym in [32, 0xffeb] {
        remote::input::dispatch(ui.window(), remote::input::Input::Key { down: false, keysym });
    }
    // Moving a tile to Trash takes one activation. Permanent deletion requires confirmation.
    let storage = ui.global::<StorageView>();
    storage.set_sd_cells(ModelRc::new(slint::VecModel::from(vec![StorageCell {
        path: "/home/test/Downloads/archive.zip".into(),
        detail: "32 MiB file size".into(),
        tint: slint::Color::from_rgb_u8(60, 110, 70),
    }])));
    let selected = Rc::new(RefCell::new(Vec::new()));
    let received = selected.clone();
    let weak = ui.as_weak();
    storage.on_select_file(move |drive, index| {
        received.borrow_mut().push((drive, index));
        let ui = weak.upgrade().unwrap();
        let storage = ui.global::<StorageView>();
        storage.set_selected_path("/home/test/Downloads/archive.zip".into());
        storage.set_selected_detail("32 MiB file size".into());
        storage.set_selected_open(true);
    });
    let deletions = Rc::new(RefCell::new(0));
    let received = deletions.clone();
    storage.on_delete_file(move || *received.borrow_mut() += 1);
    render();
    let overview = app.desk.rect_of(Kind::Overview).unwrap();
    // Use the actual model's desktop handler so a premature focus/stack update
    // would destroy the pressed tile and lose the first click.
    let live_app = Rc::new(RefCell::new(app));
    let (weak, state) = (ui.as_weak(), live_app.clone());
    ui.on_pointer(move |kind, button, x, y, ctrl, alt, shift, meta| {
        state.borrow_mut().pointer(&weak.upgrade().unwrap(), kind, button, x, y, mods(ctrl, alt, shift, meta));
    });
    let category = Rc::new(RefCell::new(None));
    let received = category.clone();
    storage.on_select_category(move |index| *received.borrow_mut() = Some(index));
    // A short Overview tile starts with the icon strip: 24 px padding, 32 px buttons, 16 px apart.
    for index in 0..8 {
        click(overview.x + 42. + 48. * index as f32, overview.y + 75.);
        assert_eq!(*category.borrow(), Some(index));
    }
    click(overview.x + 32., overview.y + 146.);
    assert_eq!(*selected.borrow(), vec![(0, 0)]);
    assert!(storage.get_selected_open());
    render();
    let key = |text: slint::SharedString| {
        ui.window().dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        ui.window().dispatch_event(WindowEvent::KeyReleased { text });
    };
    key(slint::platform::Key::Tab.into());
    key(slint::platform::Key::Return.into());
    assert!(!storage.get_confirming());
    assert_eq!(*deletions.borrow(), 1);
    storage.set_selected_open(false);
    render();
    storage.set_selected_trashed(true);
    storage.set_selected_open(true);
    render();
    key(slint::platform::Key::Tab.into());
    key(slint::platform::Key::Return.into());
    assert!(storage.get_confirming());
    assert_eq!(*deletions.borrow(), 1);
    key(slint::platform::Key::Escape.into());
    assert!(!storage.get_confirming());
    assert_eq!(*deletions.borrow(), 1);
    key(slint::platform::Key::Tab.into());
    key(slint::platform::Key::Return.into());
    assert!(storage.get_confirming());
    key(slint::platform::Key::Tab.into());
    key(slint::platform::Key::Return.into());
    assert_eq!(*deletions.borrow(), 2);
    storage.set_selected_open(false);
    floating_drag_tracks_each_physical_move(&ui, &render);
    crate::services::tests::review(&ui, &render);
}

// Exercise Slint's actual grabbed TouchArea, including layout/render between events.
// Model-only pointer tests cannot catch duplicate callbacks in moving coordinates.
fn floating_drag_tracks_each_physical_move(ui: &AppWindow, render: &impl Fn()) {
    use slint::platform::{PointerEventButton, WindowEvent};
    let app = Rc::new(RefCell::new(App::new()));
    app.borrow_mut().desk.focus(Kind::Cpu);
    ui.set_wins(ModelRc::from(app.borrow().view.windows.clone()));
    app.borrow().refresh(ui);
    let motions = Rc::new(RefCell::new(Vec::new()));
    let (weak, state, received) = (ui.as_weak(), app.clone(), motions.clone());
    ui.on_pointer(move |kind, button, x, y, ctrl, alt, shift, meta| {
        if kind == 2 { received.borrow_mut().push((x, y)); }
        state.borrow_mut().pointer(&weak.upgrade().unwrap(), kind, button, x, y, mods(ctrl, alt, shift, meta));
    });
    render();
    let rect = app.borrow().desk.rect_of(Kind::Cpu).unwrap();
    let start = slint::LogicalPosition::new(rect.x + 50., rect.y + 14.);
    ui.window().dispatch_event(WindowEvent::PointerMoved { position: start });
    ui.window().dispatch_event(WindowEvent::PointerPressed {
        position: start, button: PointerEventButton::Left,
    });
    motions.borrow_mut().clear();
    for step in 1..=60 {
        let distance = if step <= 30 { step } else { 60 - step } as f32;
        let dx = distance * 3.25;
        let dy = distance * 1.5;
        let position = slint::LogicalPosition::new(start.x + dx, start.y + dy);
        ui.window().dispatch_event(WindowEvent::PointerMoved { position });
        render();
        assert_eq!(motions.borrow().len(), step, "one desktop update per physical move");
        assert_eq!(motions.borrow()[step - 1], (position.x, position.y));
        let moved = app.borrow().desk.rect_of(Kind::Cpu).unwrap();
        assert_eq!((moved.x, moved.y), (rect.x + dx, rect.y + dy), "window stays anchored to the pointer");
    }
    ui.window().dispatch_event(WindowEvent::PointerReleased {
        position: start, button: PointerEventButton::Left,
    });
    assert!(matches!(app.borrow().drag, crate::input::Drag::None));

    // The same moving-coordinate bug also amplified top/left edge resizing.
    let start = slint::LogicalPosition::new(rect.x, rect.y);
    ui.window().dispatch_event(WindowEvent::PointerPressed {
        position: start, button: PointerEventButton::Left,
    });
    motions.borrow_mut().clear();
    let end = slint::LogicalPosition::new(start.x - 40., start.y - 30.);
    ui.window().dispatch_event(WindowEvent::PointerMoved { position: end });
    render();
    assert_eq!(&*motions.borrow(), &[(end.x, end.y)]);
    let resized = app.borrow().desk.rect_of(Kind::Cpu).unwrap();
    assert_eq!((resized.x, resized.y, resized.w, resized.h),
        (rect.x - 40., rect.y - 30., rect.w + 40., rect.h + 30.));
    ui.window().dispatch_event(WindowEvent::PointerReleased {
        position: end, button: PointerEventButton::Left,
    });

    app.borrow_mut().desk.cycle_layout();
    app.borrow().refresh(ui);
    render();
    let rect = app.borrow().desk.rect_of(Kind::Cpu).unwrap();
    let start = slint::LogicalPosition::new(rect.x + 50., rect.y + 14.);
    ui.window().dispatch_event(WindowEvent::PointerPressed {
        position: start, button: PointerEventButton::Left,
    });
    motions.borrow_mut().clear();
    let end = slint::LogicalPosition::new(start.x + 20., start.y + 10.);
    ui.window().dispatch_event(WindowEvent::PointerMoved { position: end });
    render();
    assert_eq!(&*motions.borrow(), &[(end.x, end.y)]);
    assert_eq!(app.borrow().ghost, Some(wm::Rect::new(rect.x + 20., rect.y + 10., rect.w, rect.h)));
    assert_eq!(app.borrow().desk.rect_of(Kind::Cpu), Some(rect));
    ui.window().dispatch_event(WindowEvent::PointerReleased {
        position: end, button: PointerEventButton::Left,
    });
    assert!(app.borrow().ghost.is_none());
}

#[test]
fn floating_edges_and_corners_select_directional_resize_cursors() {
    let mut app = App::new();
    app.desk.toggle_float();
    app.view.sync(app.desk.scene(), None);
    let r = app.desk.rect_of(app.desk.focused().unwrap()).unwrap();
    for (x, y, cursor) in [
        (r.x, r.y + r.h / 2.0, 2),
        (r.right() - 1.0, r.y + r.h / 2.0, 2),
        (r.x + r.w / 2.0, r.y, 3),
        (r.x + r.w / 2.0, r.bottom() - 1.0, 3),
        (r.x, r.y, 4),
        (r.right() - 1.0, r.bottom() - 1.0, 4),
        (r.right() - 1.0, r.y, 5),
        (r.x, r.bottom() - 1.0, 5),
    ] {
        assert_eq!(app.hover(x, y), cursor, "cursor at {x},{y}");
    }
}

#[test]
fn hover_uses_cached_scene_and_never_requests_model_updates() {
    let mut app = App::new();
    let before: Vec<_> = app.view.windows.iter().collect();
    let windows = app.view.scene.borrow().windows.as_ptr();
    for i in 0..10_000 {
        assert_eq!(
            app.moved((i % 1280) as f32, (i % 720) as f32),
            Motion::Cursor
        );
    }
    assert_eq!(app.view.scene.borrow().windows.as_ptr(), windows);
    assert_eq!(app.view.windows.iter().collect::<Vec<_>>(), before);
}

#[test]
fn tiled_drag_is_overlay_only_and_drop_commits_the_swap() {
    let mut app = App::new();
    app.desk.cycle_layout();
    app.view.sync(app.desk.scene(), None);
    let source = app.view.scene.borrow().rect_of(Kind::Overview).unwrap();
    let target = app.view.scene.borrow().rect_of(Kind::Cpu).unwrap();
    app.begin_move(Kind::Overview, source.x + 40.0, source.y + 10.0);
    let (x, y) = target.center();
    assert_eq!(app.moved(x, y), Motion::Overlay);
    assert_eq!(app.target, Some(Kind::Cpu));
    assert_eq!(app.desk.rect_of(Kind::Overview), Some(source));
    app.up(x, y, true);
    assert_eq!(app.desk.rect_of(Kind::Overview), Some(target));
    assert!(app.ghost.is_none());
    assert!(app.target.is_none());
}
