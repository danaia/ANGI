//! Bridge the Rust desktop layout to the compositor's independently rendered windows.
use crate::{
    App, AppWindow, Theme,
    keymap::Mods,
    wm::{Desktop, HEADER, Kind, MAX_WORKSPACES, Mode, Rect, Scene},
};
use serde::{Deserialize, Serialize};
use slint::{ComponentHandle, Timer, TimerMode};
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    fmt::Write,
    io,
    os::{
        fd::AsRawFd,
        linux::net::SocketAddrExt,
        unix::net::{SocketAddr, UnixDatagram},
    },
    rc::Rc,
    sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Window {
    workspace: usize,
    app_id: String,
    title: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
struct Placement {
    workspace: usize,
    windowed: bool,
    floating: Option<Rect>,
    #[serde(default)]
    tiled: Option<(f32, f32)>,
}

#[derive(Default)]
pub(crate) struct NativeTiles(RefCell<Option<Connection>>);

struct Connection {
    socket: UnixDatagram,
    packets: Receiver<String>,
    retry_queued: bool,
    windows: BTreeMap<u32, Window>,
    sent: HashMap<u32, String>,
    focus: Option<(usize, u32)>,
    snapshot_focus: u32,
    next_query: Instant,
    overlay: bool,
    layout_retry: bool,
    placements: HashMap<String, Placement>,
    save_at: Option<Instant>,
}

const PREFS: &str = "native-tiles.json";
/// Periodic work only: re-registering shortcuts and saving placements.
/// Compositor events wake the UI loop as they arrive.
const HOUSEKEEPING: Duration = Duration::from_millis(500);
/// Datagrams waiting for the UI thread; the compositor drops events beyond the
/// socket buffer either way, so a stalled UI never grows this queue unbounded.
const PACKET_QUEUE: usize = 256;

thread_local! {
    /// Drains compositor events on the UI thread; set by [`install`].
    static DRAIN: RefCell<Option<Rc<dyn Fn()>>> = const { RefCell::new(None) };
}

fn run_drain() {
    if let Some(drain) = DRAIN.with(|drain| drain.borrow().clone()) {
        drain();
    }
}

/// Run the drain on a later turn of the UI loop, after the current borrow ends.
fn queue_drain(delay: Duration) {
    Timer::single_shot(delay, run_drain);
}

/// Block on the compositor socket off the UI thread and wake the UI loop only
/// when events arrive, so shortcuts and new windows are handled immediately.
fn spawn_reader(socket: UnixDatagram, tx: SyncSender<String>) -> io::Result<()> {
    std::thread::Builder::new().name("agx-native-tiles".into()).spawn(move || {
        let mut buffer = vec![0u8; 65536];
        loop {
            let mut fd = libc::pollfd { fd: socket.as_raw_fd(), events: libc::POLLIN, revents: 0 };
            if unsafe { libc::poll(&mut fd, 1, -1) } < 0 {
                if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return;
            }
            let mut received = false;
            loop {
                match socket.recv(&mut buffer) {
                    Ok(count) => {
                        let Ok(message) = std::str::from_utf8(&buffer[..count]) else { continue };
                        match tx.try_send(message.to_owned()) {
                            Ok(()) | Err(TrySendError::Full(_)) => received = true,
                            Err(TrySendError::Disconnected(_)) => return,
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                    Err(_) => {
                        // A refused or failed read must not spin; the compositor
                        // owns the session, so wait for it to come back.
                        std::thread::sleep(Duration::from_millis(100));
                        break;
                    }
                }
            }
            if received && slint::invoke_from_event_loop(run_drain).is_err() {
                return;
            }
        }
    })?;
    Ok(())
}

impl Connection {
    fn connect() -> io::Result<Self> {
        let endpoint = super::wayland_workspaces::socket_path(".angi2")
            .ok_or_else(|| io::Error::other("Wayland socket missing"))?;
        let address = SocketAddr::from_abstract_name(format!("angi-tiles-{}", std::process::id()))?;
        let socket = UnixDatagram::bind_addr(&address)?;
        socket.connect(endpoint)?;
        // Shared by the reader's duplicate: neither thread may block on a full peer.
        socket.set_nonblocking(true)?;
        let (tx, packets) = sync_channel(PACKET_QUEUE);
        spawn_reader(socket.try_clone()?, tx)?;
        let placements = crate::settings::config_file(PREFS)
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Ok(Self {
            socket,
            packets,
            retry_queued: false,
            windows: BTreeMap::new(),
            sent: HashMap::new(),
            focus: None,
            snapshot_focus: 0,
            next_query: Instant::now(),
            overlay: false,
            layout_retry: false,
            placements,
            save_at: None,
        })
    }

    fn send(&self, message: &str) -> bool {
        self.socket.send(message.as_bytes()).is_ok()
    }

    fn save(&mut self) {
        if let Some(path) = crate::settings::config_file(PREFS)
            && let Ok(data) = serde_json::to_string(&self.placements)
            && let Err(error) = crate::settings::write_private(&path, &data)
        {
            eprintln!("agx-gui: native tile settings: {error}");
        }
        self.save_at = None;
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if self.save_at.is_some() {
            self.save();
        }
    }
}

impl NativeTiles {
    pub(crate) fn close(&self, id: u32) {
        if let Some(connection) = self.0.borrow().as_ref() {
            connection.send(&format!("C{id}"));
        }
    }

    pub(crate) fn focus(&self, id: u32) {
        if let Some(connection) = self.0.borrow().as_ref() {
            connection.send(&format!("I{id}"));
        }
    }

    pub(crate) fn layout(&self, ui: &AppWindow, desk: &Desktop, scene: &Scene, resizing: bool) {
        let mut state = self.0.borrow_mut();
        let Some(connection) = state.as_mut() else { return };
        let overlay = ui.get_native_overlay_open();
        connection.overlay = overlay;
        connection.layout_retry = false;
        ui.set_native_tiles_active(!connection.windows.is_empty());
        let current = desk.current();
        let mut preferred = HashMap::new();

        for (&id, window) in &connection.windows {
            let kind = Kind::Native(id);
            let Some(workspace) = desk.workspace_of(kind) else { continue };
            let mut message = placement_message(id, workspace, scene, overlay, ui.global::<Theme>().get_radius_lg());
            if resizing {
                message.push_str(" D");
            }
            if connection.sent.get(&id) != Some(&message) {
                if connection.send(&message) {
                    connection.sent.insert(id, message);
                } else {
                    connection.layout_retry = true;
                }
            }
            if !window.app_id.is_empty() {
                preferred.entry(window.app_id.clone()).or_insert(Placement {
                    workspace,
                    windowed: desk.workspace_mode(workspace) == Mode::Floating,
                    floating: desk.native_float_rect(kind),
                    tiled: desk.native_tile_extent(kind),
                });
            }
        }
        for (app, placement) in preferred {
            if connection.placements.get(&app) != Some(&placement)
                && (connection.placements.len() < 128 || connection.placements.contains_key(&app))
            {
                connection.placements.insert(app, placement);
                connection.save_at = Some(Instant::now() + Duration::from_millis(350));
            }
        }
        let desired = desk.focused().and_then(|kind| match kind {
            Kind::Native(id) if scene.windows.iter().any(|p| p.kind == kind && !p.collapsed) && !overlay => Some(id),
            _ => None,
        }).unwrap_or(0);
        let focus = (current, desired);
        if !connection.windows.is_empty() && connection.focus != Some(focus) {
            if !connection.layout_retry && connection.send(&format!("I{desired}")) {
                connection.focus = Some(focus);
            } else {
                connection.layout_retry = true;
            }
        }
        if connection.layout_retry && !connection.retry_queued {
            // The compositor's queue was full: resend on the next frame.
            connection.retry_queued = true;
            queue_drain(Duration::from_millis(16));
        }
    }
}

fn placement_message(id: u32, workspace: usize, scene: &Scene, covered: bool, radius: f32) -> String {
    let Some(index) = scene.windows.iter().position(|placed| placed.kind == Kind::Native(id)
        && !placed.collapsed && !covered)
    else {
        return format!("G{id} {workspace} 0 0 0 0 0 0");
    };
    let frame = scene.windows[index].rect;
    let inset = if scene.screen_full { 0.0 } else { HEADER };
    let x = frame.x.round() as i32;
    let y = (frame.y + inset).round() as i32;
    let width = (frame.right().round() as i32 - x).max(0);
    let height = (frame.bottom().round() as i32 - y).max(0);
    let radius = radius.min(frame.h / 2.0).min(frame.w / 2.0).round() as i32;
    let mut message = format!("G{id} {workspace} {x} {y} {width} {height} {} {radius}", u8::from(scene.screen_full));
    // Higher floating panels remain in front of the native body. Weston clips
    // both its drawing and input to the exposed region.
    for higher in &scene.windows[index + 1..] {
        let other = higher.rect;
        if other.x < frame.right() && other.right() > frame.x
            && other.bottom() > y as f32 && other.y < frame.bottom()
        {
            let left = other.x.round() as i32;
            let top = other.y.round() as i32;
            let _ = write!(message, " {left} {top} {} {}", other.right().round() as i32 - left,
                other.bottom().round() as i32 - top);
        }
    }
    message
}

fn text_field(hex: &str) -> Option<String> {
    if hex == "-" {
        return Some(String::new());
    }
    if hex.len() > 512 || !hex.len().is_multiple_of(2) {
        return None;
    }
    let bytes = hex.as_bytes().chunks_exact(2).map(|pair| {
        let digits = std::str::from_utf8(pair).ok()?;
        u8::from_str_radix(digits, 16).ok()
    }).collect::<Option<Vec<_>>>()?;
    Some(String::from_utf8_lossy(&bytes).chars().filter(|c| !c.is_control()).collect())
}

fn snapshot(message: &str) -> Option<(u32, BTreeMap<u32, Window>)> {
    let mut lines = message.lines();
    let focus = lines.next()?.strip_prefix("V ")?.parse().ok()?;
    let mut windows = BTreeMap::new();
    for line in lines {
        let mut fields = line.split_ascii_whitespace();
        let id: u32 = fields.next()?.parse().ok()?;
        let workspace: usize = fields.next()?.parse().ok()?;
        let app_id = text_field(fields.next()?)?;
        let title = text_field(fields.next()?)?;
        if id == 0 || id > i32::MAX as u32 || workspace >= MAX_WORKSPACES || fields.next().is_some() {
            return None;
        }
        windows.insert(id, Window { workspace, app_id, title });
    }
    Some((focus, windows))
}

fn apply_snapshot(app: &mut App, focus: u32, windows: BTreeMap<u32, Window>) -> bool {
    let mut state = app.native_tiles.0.borrow_mut();
    let Some(connection) = state.as_mut() else { return false };
    if windows == connection.windows && focus == connection.snapshot_focus {
        return false;
    }
    let mut titles = app.view.native_titles.borrow_mut();
    for id in connection.windows.keys().filter(|id| !windows.contains_key(id)) {
        app.desk.forget_native(Kind::Native(*id));
        titles.remove(id);
        connection.sent.remove(id);
    }
    for (&id, window) in &windows {
        if !connection.windows.contains_key(&id) {
            let placement = connection.placements.get(&window.app_id).filter(|p| p.workspace < MAX_WORKSPACES);
            let workspace = placement.map_or(window.workspace, |p| p.workspace);
            if app.desk.ensure_workspace(workspace) {
                let mode = app.desk.workspace_mode(workspace);
                let rect = placement.filter(|p| mode == Mode::Floating || !p.windowed)
                    .and_then(|p| p.floating).filter(|r| {
                    [r.x, r.y, r.w, r.h].into_iter().all(|v| v.is_finite() && v.abs() <= 32768.0)
                        && r.w >= crate::wm::MIN_W && r.h >= crate::wm::MIN_H
                });
                app.desk.attach_native(Kind::Native(id), workspace, rect);
                if rect.is_none() && let Some(extent) = placement.and_then(|p| p.tiled)
                    && (extent.0 < 0.85 || extent.1 < 0.85) {
                    app.desk.restore_native_extent(Kind::Native(id), extent);
                }
                if id == focus {
                    app.desk.goto(workspace);
                }
            }
        }
        let title = if window.title.is_empty() { &window.app_id } else { &window.title };
        if !title.is_empty() {
            titles.insert(id, title.clone());
        }
    }
    let kind = Kind::Native(focus);
    if focus != connection.snapshot_focus && app.desk.workspace_of(kind) == Some(app.desk.current()) {
        app.desk.focus(kind);
    }
    connection.snapshot_focus = focus;
    connection.windows = windows;
    true
}

fn drain(app: &mut App, ui: &AppWindow) {
    let mut packets = Vec::new();
    let needs_layout = {
        let mut state = app.native_tiles.0.borrow_mut();
        let Some(connection) = state.as_mut() else { return };
        let now = Instant::now();
        if now >= connection.next_query {
            let mut query = String::from("Q");
            for (key, bits) in app.bindings.native_keyboard_bindings(app.modk) {
                let _ = write!(query, " {key} {bits}");
            }
            connection.send(&query);
            connection.next_query = now + Duration::from_secs(2);
        }
        if connection.save_at.is_some_and(|when| now >= when) {
            connection.save();
        }
        connection.retry_queued = false;
        packets.extend(connection.packets.try_iter().take(PACKET_QUEUE));
        connection.layout_retry || connection.overlay != ui.get_native_overlay_open()
    };
    let mut changed = needs_layout;
    for message in packets {
        if let Some((focus, windows)) = snapshot(&message) {
            changed |= apply_snapshot(app, focus, windows);
        } else if let Some((name, bits)) = message.strip_prefix("K ").and_then(|command| command.split_once(' '))
            && let Ok(bits) = bits.parse()
        {
            // Flush the compositor's focus event before resolving the shortcut.
            if changed {
                app.refresh(ui);
                changed = false;
            }
            app.key(ui, name, Mods::from_bits(bits));
        } else if let Some(action) = message.strip_prefix("F ").and_then(|value| value.parse().ok()) {
            app.fullscreen_action(ui, action);
        } else if let Some((id, request)) = message.strip_prefix("X ").and_then(|value| value.split_once(' '))
            && let Ok(id) = id.parse()
            && app.desk.workspace_of(Kind::Native(id)) == Some(app.desk.current())
        {
            use crate::wm::Full;
            let kind = Kind::Native(id);
            app.desk.focus(kind);
            match request {
                "full" if app.desk.fullscreen_of() != Some((kind, Full::Screen)) => app.desk.toggle_fullscreen(Full::Screen),
                "normal" if app.desk.fullscreen_of() == Some((kind, Full::Screen)) => app.desk.toggle_fullscreen(Full::Screen),
                "max" if app.desk.fullscreen_of() != Some((kind, Full::Maximized)) => app.desk.toggle_fullscreen(Full::Maximized),
                "unmax" if app.desk.fullscreen_of() == Some((kind, Full::Maximized)) => app.desk.toggle_fullscreen(Full::Maximized),
                "min" if !app.desk.is_collapsed(kind) => {
                    if !app.desk.is_floating(kind) { app.desk.toggle_float(); }
                    app.desk.toggle_collapsed(kind);
                }
                _ => {}
            }
            changed = true;
        }
    }
    if changed {
        app.refresh(ui);
    }
}

pub(crate) fn install(ui: &AppWindow, app: &Rc<RefCell<App>>) -> Option<Timer> {
    let connection = Connection::connect()
        .map_err(|error| eprintln!("agx-gui: native tiles: {error}"))
        .ok()?;
    *app.borrow().native_tiles.0.borrow_mut() = Some(connection);
    let (weak, app) = (ui.as_weak(), Rc::downgrade(app));
    DRAIN.with(|drain_slot| {
        *drain_slot.borrow_mut() = Some(Rc::new(move || {
            let (Some(ui), Some(app)) = (weak.upgrade(), app.upgrade()) else { return };
            match app.try_borrow_mut() {
                Ok(mut app) => drain(&mut app, &ui),
                // Another handler holds the desktop; run once it returns.
                Err(_) => queue_drain(Duration::from_millis(4)),
            }
        }));
    });
    // Opening a Slint menu or dialog must hide native bodies on the same frame.
    ui.on_native_overlay_changed(|| queue_drain(Duration::ZERO));
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, HOUSEKEEPING, run_drain);
    Some(timer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wm::Placed;

    #[test]
    fn reader_forwards_compositor_datagrams_without_a_ui_poll() {
        let (compositor, desktop) = UnixDatagram::pair().unwrap();
        desktop.set_nonblocking(true).unwrap();
        let (tx, rx) = sync_channel(PACKET_QUEUE);
        spawn_reader(desktop, tx).unwrap();
        compositor.send(b"K q 8").unwrap();
        compositor.send(b"V 0\n").unwrap();
        let wait = Duration::from_secs(2);
        assert_eq!(rx.recv_timeout(wait).unwrap(), "K q 8");
        assert_eq!(rx.recv_timeout(wait).unwrap(), "V 0\n");
    }

    #[test]
    fn native_body_runs_edge_to_edge_below_header_and_clips_higher_frames() {
        let native = Placed {
            kind: Kind::Native(5),
            rect: Rect::new(100.0, 70.0, 600.0, 400.0),
            floating: false,
            focused: true,
            collapsed: false,
        };
        let mut scene = Scene {
            windows: vec![native, Placed { kind: Kind::Settings, rect: Rect::new(600.0, 90.0, 200.0, 250.0), ..native }],
            ..Scene::default()
        };
        assert_eq!(placement_message(5, 2, &scene, false, 28.0), "G5 2 100 114 600 356 0 28 600 90 200 250");
        assert_eq!(placement_message(5, 2, &scene, true, 28.0), "G5 2 0 0 0 0 0 0");
        scene.windows.truncate(1);
        scene.screen_full = true;
        assert_eq!(placement_message(5, 2, &scene, false, 28.0), "G5 2 100 70 600 400 1 28");
    }

    #[test]
    fn parses_multiple_unrelated_native_clients_and_rejects_bad_records() {
        let (focus, windows) = snapshot("V 8\n3 0 636f6465 56697375616c2053747564696f20436f6465\n8 2 - 56657565\n").unwrap();
        assert_eq!(focus, 8);
        assert_eq!(windows[&3].title, "Visual Studio Code");
        assert_eq!(windows[&8].workspace, 2);
        assert!(windows[&8].app_id.is_empty());
        assert!(snapshot("V 0\n1 10 12 34\n").is_none());
        assert!(snapshot("V 0\n1 4 qq 34\n").is_none());
    }
}
