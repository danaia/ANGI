//! Settings → Bluetooth checks that need the headless desktop. The platform can be installed once
//! per process, so `pointer_tests` owns it and calls `drive` while Settings is open.
use crate::{App, AppWindow, BluetoothSettings, BtApproval, BtDevice, MouseSettings, wm::Kind};
use slint::{
    ComponentHandle, ModelRc,
    platform::{WindowEvent, software_renderer::MinimalSoftwareWindow},
};
use std::{cell::RefCell, rc::Rc};

/// Switches pages by keyboard, loads sample daemon state, drives selection, prompts and
/// approvals through real key events, and exercises the live wiring against a stand-in daemon.
/// Set `AGX_BLUETOOTH_SCREENSHOT` to also write PPM screenshots of the page.
pub(crate) fn drive(
    ui: &AppWindow,
    window: &Rc<MinimalSoftwareWindow>,
    settings_app: &Rc<RefCell<App>>,
    click: &dyn Fn(f32, f32),
    render: &dyn Fn(),
) {
    // Bluetooth page: PageDown switches pages, daemon state is shown, and the keyboard drives it.
    {
        let press = |key: slint::platform::Key| {
            ui.window()
                .dispatch_event(WindowEvent::KeyPressed { text: key.into() });
            slint::platform::update_timers_and_animations();
        };
        let page = ui.global::<BluetoothSettings>();
        let calls = Rc::new(RefCell::new(Vec::<String>::new()));
        let weak = ui.as_weak();
        page.on_select(move |index| {
            weak.upgrade()
                .unwrap()
                .global::<BluetoothSettings>()
                .set_selected(index)
        });
        let log = calls.clone();
        page.on_act(move |address, action| {
            log.borrow_mut().push(format!("act {address} {action}"))
        });
        let log = calls.clone();
        page.on_respond(move |accept, _| log.borrow_mut().push(format!("respond {accept}")));
        let log = calls.clone();
        page.on_decide(move |id, allow| log.borrow_mut().push(format!("decide {id} {allow}")));
        let device = |address: &str,
                      name: &str,
                      detail: &str,
                      glyph: &str,
                      header: &str,
                      paired: bool,
                      connected: bool,
                      bars: i32| BtDevice {
            address: address.into(),
            name: name.into(),
            detail: detail.into(),
            glyph: glyph.into(),
            header: header.into(),
            paired,
            connected,
            trusted: paired,
            busy: false,
            bars,
        };
        page.set_available(true);
        page.set_adapter(true);
        page.set_powered(true);
        page.set_scanning(true);
        page.set_seconds_left(31);
        page.set_status("Searching for devices…".into());
        page.set_devices(ModelRc::new(slint::VecModel::from(vec![
            device(
                "AA:BB:CC:00:00:01",
                "Desk speaker",
                "Speaker · Connected · 80% · Auto-connect",
                "\u{f04c3}",
                "MY DEVICES",
                true,
                true,
                0,
            ),
            device(
                "AA:BB:CC:00:00:02",
                "Keyboard",
                "Keyboard · Not connected · Auto-connect",
                "\u{f030c}",
                "",
                true,
                false,
                0,
            ),
            device(
                "AA:BB:CC:00:00:03",
                "Headphones",
                "Headphones · Very close",
                "\u{f02cb}",
                "NEARBY DEVICES",
                false,
                false,
                4,
            ),
        ])));
        press(slint::platform::Key::PageDown);
        render();
        assert!(
            page.get_active(),
            "the service is polled only while the Bluetooth page is showing"
        );
        press(slint::platform::Key::DownArrow);
        press(slint::platform::Key::DownArrow);
        assert_eq!(
            page.get_selected(),
            1,
            "arrows move through the device list"
        );
        press(slint::platform::Key::Return);
        assert_eq!(
            calls.borrow().last().unwrap(),
            "act AA:BB:CC:00:00:02 1",
            "Enter connects a paired, disconnected device"
        );
        press(slint::platform::Key::Delete);
        assert_eq!(
            page.get_confirm_forget(),
            "AA:BB:CC:00:00:02",
            "removal asks first"
        );
        assert_eq!(
            calls.borrow().len(),
            1,
            "nothing is removed by the first press"
        );
        press(slint::platform::Key::Escape);
        assert_eq!(
            page.get_confirm_forget(),
            "",
            "Escape cancels the confirmation"
        );
        press(slint::platform::Key::Delete);
        press(slint::platform::Key::Delete);
        assert_eq!(
            calls.borrow().last().unwrap(),
            "act AA:BB:CC:00:00:02 3",
            "a second press removes"
        );
        render();
        // A request from an agent takes Enter and Escape until the person answers it.
        page.set_approvals(ModelRc::new(slint::VecModel::from(vec![BtApproval {
            id: 4,
            text: "pair with and always connect to Headphones.".into(),
        }])));
        press(slint::platform::Key::Return);
        assert_eq!(calls.borrow().last().unwrap(), "decide 4 true");
        press(slint::platform::Key::Escape);
        assert_eq!(calls.borrow().last().unwrap(), "decide 4 false");
        page.set_approvals(ModelRc::new(
            slint::VecModel::from(Vec::<BtApproval>::new()),
        ));
        // A pairing prompt does too.
        page.set_prompt_text(
            "Does Headphones show this code? Pair only if both screens match.".into(),
        );
        page.set_prompt_code("123456".into());
        page.set_prompt_kind(1);
        render();
        if let Some(path) = std::env::var_os("AGX_BLUETOOTH_SCREENSHOT") {
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
        press(slint::platform::Key::Return);
        assert_eq!(calls.borrow().last().unwrap(), "respond true");
        press(slint::platform::Key::Escape);
        assert_eq!(calls.borrow().last().unwrap(), "respond false");
        page.set_prompt_kind(0);
        // Maximizing rebuilds the panel; the chosen page and the service polling must survive it.
        let tile = settings_app.borrow().desk.rect_of(Kind::Settings).unwrap();
        click(tile.right() - 46., tile.y + 14.);
        render();
        slint::platform::update_timers_and_animations();
        assert_eq!(
            ui.global::<MouseSettings>().get_page(),
            1,
            "the page survives the tile being rebuilt"
        );
        assert!(page.get_active(), "a rebuilt Bluetooth page keeps polling");
        if let Some(path) = std::env::var_os("AGX_BLUETOOTH_SCREENSHOT") {
            // The same page at full width: signal bars appear and the buttons get more room.
            use std::io::Write;
            let mut pixels = vec![slint::Rgb8Pixel::default(); 1280 * 720];
            window.request_redraw();
            window.draw_if_needed(|renderer| {
                renderer.render(&mut pixels, 1280);
            });
            let mut file = std::io::BufWriter::new(
                std::fs::File::create(std::path::Path::new(&path).with_extension("wide.ppm"))
                    .unwrap(),
            );
            file.write_all(b"P6\n1280 720\n255\n").unwrap();
            for p in pixels {
                file.write_all(&[p.r, p.g, p.b]).unwrap();
            }
        }
        // Restore the tile and give Settings the keyboard without selecting another category.
        let tile = settings_app.borrow().desk.rect_of(Kind::Settings).unwrap();
        click(tile.right() - 46., tile.y + 14.);
        render();
        ui.invoke_focus_settings();
        press(slint::platform::Key::PageUp);
        render();
        assert_eq!(ui.global::<crate::MouseSettings>().get_page(), 0);
        assert!(!page.get_active(), "leaving the page stops polling");
    }
    // The real wiring end to end: a stand-in daemon answers on a private socket, so the worker
    // thread, request framing, poll gating and presentation all run as they do on the device.
    {
        use slint::Model;
        use std::io::{BufRead, BufReader, Write};
        let press = |key: slint::platform::Key| {
            ui.window()
                .dispatch_event(WindowEvent::KeyPressed { text: key.into() });
            slint::platform::update_timers_and_animations();
        };
        let socket = std::env::temp_dir().join(format!("agx-bt-gui-{}.sock", std::process::id()));
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        // Only this test reads the variable, and it runs on this thread before the worker starts.
        unsafe { std::env::set_var(agx_bluetooth::SOCKET_ENV, &socket) };
        let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let log = requests.clone();
        std::thread::spawn(move || {
            let status = agx_bluetooth::Status {
                adapter: Some(agx_bluetooth::Adapter {
                    name: "angi".into(),
                    address: "00:11:22:33:44:55".into(),
                    powered: true,
                    scanning: true,
                }),
                devices: vec![
                    agx_bluetooth::Device {
                        address: "AA:BB:CC:00:00:01".into(),
                        name: "Speaker".into(),
                        paired: true,
                        connected: true,
                        ..Default::default()
                    },
                    agx_bluetooth::Device {
                        address: "AA:BB:CC:00:00:02".into(),
                        name: "Phone".into(),
                        rssi: Some(-50),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            };
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut line = String::new();
                BufReader::new(&stream).read_line(&mut line).unwrap();
                log.lock().unwrap().push(line.trim().to_owned());
                let reply = agx_bluetooth::Reply {
                    ok: true,
                    message: "Bluetooth status".into(),
                    status: status.clone(),
                };
                let _ = writeln!(stream, "{}", serde_json::to_string(&reply).unwrap());
            }
        });
        let watch = crate::bluetooth::install(ui, settings_app);
        let page = ui.global::<BluetoothSettings>();
        press(slint::platform::Key::PageDown);
        let wait = |done: &dyn Fn() -> bool| {
            for _ in 0..80 {
                slint::platform::update_timers_and_animations();
                if done() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            panic!("timed out waiting for the Bluetooth service state to reach the page");
        };
        wait(&|| page.get_devices().row_count() == 2);
        assert!(
            page.get_available() && page.get_adapter() && page.get_powered() && page.get_scanning()
        );
        assert_eq!(page.get_devices().row_data(0).unwrap().header, "MY DEVICES");
        assert_eq!(
            page.get_devices().row_data(1).unwrap().header,
            "NEARBY DEVICES"
        );
        assert_eq!(
            requests.lock().unwrap()[0],
            r#"{"cmd":"status"}"#,
            "the page polls with a plain status request"
        );
        // An action goes through the worker as the same wire request the CLI would send.
        page.invoke_act("AA:BB:CC:00:00:02".into(), 0);
        wait(&|| {
            requests
                .lock()
                .unwrap()
                .iter()
                .any(|r| r == r#"{"cmd":"pair","address":"AA:BB:CC:00:00:02"}"#)
        });
        // A stopped service is reported rather than leaving stale devices on screen.
        std::fs::remove_file(&socket).unwrap();
        wait(&|| !page.get_available());
        assert_eq!(page.get_devices().row_count(), 0);
        assert!(page.get_status().contains("unavailable"));
        drop(watch);
        unsafe { std::env::remove_var(agx_bluetooth::SOCKET_ENV) };
        press(slint::platform::Key::PageUp);
    }
}
