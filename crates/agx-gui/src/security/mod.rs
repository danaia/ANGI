//! Settings → Security and the first-boot Security step. All socket traffic to agx-securityd
//! runs on one worker thread; this module mirrors the daemon's state into the UI and turns the
//! page's callbacks into requests.
mod suggest;
#[cfg(test)]
mod tests;

use crate::{App, AppWindow, SecuritySettings, wm::Kind as Window};
use agx_security::{Kind, Reply, Request, Status};
use slint::{ComponentHandle, Global, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::mpsc,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

const POLL: Duration = Duration::from_millis(2000);

type Outcome = Result<Reply, String>;

/// What the page needs from a status, without any UI types, so it can be checked on its own.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Summary {
    pub ethernet_connected: bool,
    pub ethernet_address: String,
    pub ethernet_listening: bool,
    pub usb_connected: bool,
    /// The computer on the Ethernet cable, once the neighbor table knows it.
    pub peer: String,
}

pub(crate) fn summarize(status: &Status) -> Summary {
    let wired = |kind| status.links.iter().filter(move |link| link.kind == kind && link.connected);
    let addressed = wired(Kind::Ethernet)
        .filter(|link| !link.address.is_empty())
        .max_by_key(|link| link.open);
    Summary {
        ethernet_connected: wired(Kind::Ethernet).next().is_some(),
        ethernet_address: addressed.map(|link| link.address.clone()).unwrap_or_default(),
        ethernet_listening: addressed.is_some_and(|link| link.open),
        usb_connected: wired(Kind::Usb).next().is_some(),
        peer: wired(Kind::Ethernet)
            .map(|link| link.peer.clone())
            .find(|peer| !peer.is_empty())
            .unwrap_or_default(),
    }
}

pub(crate) fn link_kind(link: i32) -> Option<Kind> {
    match link {
        1 => Some(Kind::Ethernet),
        2 => Some(Kind::Usb),
        _ => None,
    }
}

fn show(page: SecuritySettings<'_>, outcome: Outcome) {
    let reply = match outcome {
        Ok(reply) => reply,
        Err(message) => {
            page.set_available(false);
            page.set_message(message.into());
            page.set_error("".into());
            return;
        }
    };
    let status = &reply.status;
    let summary = summarize(status);
    page.set_available(true);
    page.set_public_key(status.public_key.as_str().into());
    page.set_key_fingerprint(status.key_fingerprint.as_str().into());
    page.set_host_fingerprint(status.host_fingerprint.as_str().into());
    page.set_ethernet_open(status.ethernet_open);
    page.set_usb_open(status.usb_open);
    page.set_ethernet_connected(summary.ethernet_connected);
    page.set_ethernet_address(summary.ethernet_address.into());
    page.set_ethernet_listening(summary.ethernet_listening);
    page.set_usb_connected(summary.usb_connected);
    // Once any account exists the administrator's password is what authorizes opening a link.
    page.set_password_required(agx_profile::service::setup_claimed().unwrap_or(true));
    page.set_vnc_enabled(status.vnc_enabled);
    page.set_vnc_listening(status.vnc_listening);
    page.set_vnc_viewers(status.vnc_viewers as i32);
    let account = agx_profile::service::status()
        .ok()
        .and_then(|profile| profile.accounts.first().map(|account| account.username.clone()));
    // Prefill the administrator's name so only the password is left to type.
    if page.get_account().is_empty() {
        page.set_account(account.clone().unwrap_or_default().into());
    }
    let user = account.unwrap_or_else(|| "root".into());
    page.set_suggestions(ModelRc::new(VecModel::from(
        suggest::suggestions(status, &user).into_iter().map(SharedString::from).collect::<Vec<_>>(),
    )));
    if let Some(peer) = &reply.peer {
        page.set_peer_address(peer.address.as_str().into());
        page.set_peer_fingerprint(peer.fingerprint.as_str().into());
        page.set_peer_pinned(peer.pinned);
    } else if page.get_peer_address() != summary.peer {
        // A different computer (or none) on the cable: forget what was read from the last one.
        page.set_peer_address(summary.peer.as_str().into());
        page.set_peer_fingerprint("".into());
        page.set_peer_pinned(false);
    }
    if reply.ok {
        page.set_error("".into());
        if !reply.message.is_empty() {
            page.set_message(reply.message.as_str().into());
            // A message comes only with a change the daemon accepted; open forms close on it.
            page.set_ok_serial(page.get_ok_serial().wrapping_add(1));
        }
    } else {
        page.set_error(reply.message.as_str().into());
    }
}

pub(super) fn install(ui: &AppWindow, app: &Rc<RefCell<App>>) -> Timer {
    let app = app.clone();
    watch(ui, move || {
        app.try_borrow()
            .is_ok_and(|app| app.desk.is_open(Window::Settings))
    })
}

pub(crate) fn watch<C: ComponentHandle + 'static>(
    ui: &C,
    open: impl Fn() -> bool + 'static,
) -> Timer
where
    for<'a> SecuritySettings<'a>: Global<'a, C>,
{
    let (jobs, work) = mpsc::channel::<Request>();
    let (results, outcomes) = mpsc::channel::<Outcome>();
    std::thread::spawn(move || {
        for request in work {
            let outcome = agx_security::request(&request).map_err(|error| error.to_string());
            if results.send(outcome).is_err() {
                break;
            }
        }
    });
    // Replies still on their way back; a poll is not queued behind another.
    let pending = Rc::new(Cell::new(0u32));
    let send = {
        let (jobs, pending) = (jobs.clone(), pending.clone());
        move |request: Request| {
            pending.set(pending.get() + 1);
            let _ = jobs.send(request);
        }
    };
    let page = ui.global::<SecuritySettings>();
    {
        let send = send.clone();
        page.on_set_link(move |link, open, username, password| {
            let Some(kind) = link_kind(link) else { return };
            send(if open {
                Request::Open {
                    kind,
                    username: username.to_string(),
                    password: Zeroizing::new(password.to_string()),
                }
            } else {
                Request::Close { kind }
            });
        });
    }
    {
        let send = send.clone();
        page.on_set_vnc(move |on, username, password| {
            send(Request::Vnc {
                on,
                username: username.to_string(),
                password: Zeroizing::new(password.to_string()),
            });
        });
    }
    {
        let send = send.clone();
        page.on_close_all(move || send(Request::CloseAll));
    }
    {
        let send = send.clone();
        page.on_check_peer(move || send(Request::Peer));
    }
    {
        let (send, weak) = (send.clone(), ui.as_weak());
        page.on_trust_peer(move || {
            let Some(ui) = weak.upgrade() else { return };
            let fingerprint = ui.global::<SecuritySettings>().get_peer_fingerprint();
            if !fingerprint.is_empty() {
                send(Request::Pin { fingerprint: fingerprint.to_string() });
            }
        });
    }

    let timer = Timer::default();
    let weak = ui.as_weak();
    let mut last_poll = Instant::now() - POLL;
    timer.start(TimerMode::Repeated, Duration::from_millis(150), move || {
        let Some(ui) = weak.upgrade() else { return };
        let page = ui.global::<SecuritySettings>();
        while let Ok(outcome) = outcomes.try_recv() {
            pending.set(pending.get().saturating_sub(1));
            show(ui.global::<SecuritySettings>(), outcome);
        }
        page.set_busy(pending.get() > 0);
        // Poll only while the Security page is on screen.
        if !open() || !page.get_active() || pending.get() > 0 {
            return;
        }
        if last_poll.elapsed() >= POLL {
            last_poll = Instant::now();
            send(Request::Status);
        }
    });
    timer
}
