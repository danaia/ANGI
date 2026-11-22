//! ANGI Files: a native, virtualized file browser. It backs the Files window and the folder picker
//! Agent Angi opens from workspace 2.
//!
//! `hub.rs` owns commands, the clipboard and undo history for both browsers; `pane.rs` one
//! browser's state; `model.rs` the lazily formatted rows; `worker.rs` the scan and operation
//! threads; `listing.rs`, `ops.rs` and `trash.rs` the filesystem work, free of any UI types so the
//! same engine can serve agents later. `ui/files.slint` draws it.
mod entry;
mod hub;
mod keys;
mod listing;
mod model;
mod ops;
mod pane;
mod places;
pub(crate) mod server;
#[cfg(test)]
mod tests;
pub(super) mod trash;
mod worker;

use crate::{App, AppWindow, Files, wm::Kind};
use hub::{Effect, Hub, PICKER, WINDOW};
use slint::{ComponentHandle, Timer, TimerMode};
use std::{cell::{Cell, RefCell}, path::PathBuf, rc::Rc, sync::mpsc, time::Duration};

/// Polling is fast only while something is outstanding; changes made by other programs are noticed
/// by the scan thread and surface within one slow tick.
const FAST: Duration = Duration::from_millis(25);
const SLOW: Duration = Duration::from_millis(300);

fn publish(ui: &AppWindow, hub: &Hub, pane: usize) {
    let files = ui.global::<Files>();
    if pane == WINDOW {
        files.set_browser(hub.state(WINDOW));
    } else {
        files.set_chooser(hub.state(PICKER));
    }
}

fn publish_visible(ui: &AppWindow, hub: &Hub) {
    publish(ui, hub, WINDOW);
    if hub.picker_open {
        publish(ui, hub, PICKER);
    }
}

pub(super) fn open_folder(ui: &AppWindow, app: &Rc<RefCell<App>>, path: &str) {
    if let Ok(mut app) = app.try_borrow_mut() {
        app.run(ui, crate::keymap::Cmd::Open(Kind::Files));
    }
    ui.global::<Files>().invoke_command(WINDOW as i32, "navigate".into(), path.into());
}

pub(super) fn install(ui: &AppWindow, app: &Rc<RefCell<App>>) -> Rc<Timer> {
    let (replies, inbox) = mpsc::channel();
    let hub = Rc::new(RefCell::new(Hub::new(replies)));
    let servers = Rc::new(server::Servers::new());
    publish_visible(ui, &hub.borrow());

    // Polling is fast only while something is outstanding. Starting work speeds it up at once.
    let timer = Rc::new(Timer::default());
    let interval = Rc::new(Cell::new(SLOW));
    let pace = {
        let (timer, interval) = (Rc::downgrade(&timer), interval.clone());
        Rc::new(move |busy: bool| {
            let wanted = if busy { FAST } else { SLOW };
            if let (true, Some(timer)) = (wanted != interval.get(), timer.upgrade()) {
                interval.set(wanted);
                timer.set_interval(wanted);
            }
        })
    };

    // Desktop effects: terminals, local servers, and Agent Angi's folder choice.
    let perform = {
        let (weak, app, servers) = (ui.as_weak(), app.clone(), servers.clone());
        Rc::new(move |effect: Effect| {
            let Some(ui) = weak.upgrade() else { return };
            match effect {
                Effect::None => {}
                Effect::Terminal(dir) => {
                    if let Ok(mut app) = app.try_borrow_mut() {
                        app.terminal_open_in(&ui, dir);
                    }
                }
                Effect::PythonServer(dir) => servers.start(dir),
                Effect::Choose(dir) => {
                    ui.global::<Files>().set_chooser_error("".into());
                    ui.global::<crate::AgentScope>().invoke_use_project(dir.to_string_lossy().into_owned().into());
                }
            }
        })
    };

    let files = ui.global::<Files>();
    {
        let (weak, hub, perform, pace) = (ui.as_weak(), hub.clone(), perform.clone(), pace.clone());
        files.on_command(move |pane, name, arg| {
            let (Some(ui), Ok(mut hub)) = (weak.upgrade(), hub.try_borrow_mut()) else { return };
            let effect = hub.command(pane as usize, &name, &arg);
            pace(hub.busy());
            publish_visible(&ui, &hub);
            drop(hub);
            perform(effect);
        });
    }
    {
        let (weak, hub) = (ui.as_weak(), hub.clone());
        files.on_select(move |pane, row, ctrl, shift| {
            let (Some(ui), Ok(mut hub)) = (weak.upgrade(), hub.try_borrow_mut()) else { return };
            hub.select(pane as usize, row as usize, ctrl, shift);
            publish(&ui, &hub, pane as usize);
        });
    }
    {
        let (weak, hub, pace) = (ui.as_weak(), hub.clone(), pace.clone());
        files.on_activate(move |pane, row| {
            let (Some(ui), Ok(mut hub)) = (weak.upgrade(), hub.try_borrow_mut()) else { return };
            hub.activate(pane as usize, row as usize);
            pace(hub.busy());
            publish(&ui, &hub, pane as usize);
        });
    }
    {
        let (weak, hub, perform, pace) = (ui.as_weak(), hub.clone(), perform.clone(), pace.clone());
        files.on_key(move |pane, key, ctrl, alt, shift, page| {
            let (Some(ui), Ok(mut hub)) = (weak.upgrade(), hub.try_borrow_mut()) else { return false };
            let Some(effect) = hub.key(pane as usize, &key, ctrl, alt, shift, page.max(1) as usize) else { return false };
            pace(hub.busy());
            publish_visible(&ui, &hub);
            drop(hub);
            perform(effect);
            true
        });
    }
    {
        // A click inside the window gives it the desktop's focus, as Settings does.
        let (weak, app) = (ui.as_weak(), app.clone());
        files.on_activated(move || {
            if let (Some(ui), Ok(mut app)) = (weak.upgrade(), app.try_borrow_mut()) {
                if app.desk.focused() != Some(Kind::Files) {
                    app.desk.focus(Kind::Files);
                    app.refresh(&ui);
                }
            }
        });
    }
    {
        let (weak, hub, pace) = (ui.as_weak(), hub.clone(), pace.clone());
        files.on_open_picker(move |start| {
            let (Some(ui), Ok(mut hub)) = (weak.upgrade(), hub.try_borrow_mut()) else { return };
            let start = (!start.is_empty()).then(|| PathBuf::from(start.as_str()));
            hub.open_picker(start);
            pace(hub.busy());
            let files = ui.global::<Files>();
            files.set_chooser_error("".into());
            files.set_chooser_open(true);
            publish(&ui, &hub, PICKER);
        });
    }

    let weak = ui.as_weak();
    let app = app.clone();
    timer.start(TimerMode::Repeated, SLOW, move || {
        let (Some(ui), Ok(mut hub)) = (weak.upgrade(), hub.try_borrow_mut()) else { return };
        hub.picker_open = ui.global::<Files>().get_chooser_open();
        let mut changed = false;
        for reply in servers.replies.try_iter() {
            match reply {
                Ok(url) => {
                    hub.panes[WINDOW].status = format!("Python server: {url} · stops when the desktop session ends");
                    if let Ok(mut app) = app.try_borrow_mut() {
                        app.run(&ui, crate::keymap::Cmd::Open(Kind::Web));
                        app.browser_open(&ui, &url);
                    }
                }
                Err(error) => hub.panes[WINDOW].status = error,
            }
            changed = true;
        }
        for reply in inbox.try_iter() {
            hub.handle(reply);
            changed = true;
        }
        if changed {
            publish_visible(&ui, &hub);
        }
        pace(hub.busy());
    });
    timer
}
