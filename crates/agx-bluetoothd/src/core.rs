//! Bluetooth state, authorization policy and request dispatch, independent of the stack
//! underneath (see `backend.rs`).
use crate::{
    backend::{Backend, Error, Result},
    peer::Peer,
    prompts::Prompter,
};
use agx_bluetooth::{
    Adapter, Approval, ApprovalAction, Device, PROMPT_SECONDS, Reply, Request, SCAN_SECONDS,
    Status, normalize_address,
};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::Duration,
};
// tokio's clock behaves like the system one, and lets tests move time without sleeping.
use tokio::time::Instant;

/// Pairing waits on a person and a radio; connecting should be quick.
const PAIR_TIMEOUT: Duration = Duration::from_secs(PROMPT_SECONDS as u64 + 15);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_APPROVALS: usize = 8;

struct Waiting {
    approval: Approval,
    expires: Instant,
}

#[derive(Default)]
struct State {
    adapter: Option<Adapter>,
    devices: Vec<Device>,
    busy: HashSet<String>,
    approvals: Vec<Waiting>,
    next_approval: u32,
    scan_until: Option<Instant>,
    message: String,
    error: Option<String>,
}

pub struct Core<B: Backend> {
    backend: B,
    prompter: Arc<Prompter>,
    state: Mutex<State>,
}

/// Unnamed strangers are noise; hide them unless the person already uses them. A cached device
/// that is out of range (no signal) is hidden too, so the list shows what is here now.
fn visible(device: &Device) -> bool {
    device.paired
        || device.connected
        || device.trusted
        || (device.rssi.is_some() && !device.name.is_empty())
}

fn rank(device: &Device) -> (u8, std::cmp::Reverse<i16>, String) {
    let group = if device.connected {
        0
    } else if device.paired {
        1
    } else {
        2
    };
    (
        group,
        std::cmp::Reverse(device.rssi.unwrap_or(i16::MIN)),
        device.name.to_lowercase(),
    )
}

impl<B: Backend> Core<B> {
    pub fn new(backend: B, prompter: Arc<Prompter>) -> Arc<Self> {
        Arc::new(Self {
            backend,
            prompter,
            state: Mutex::new(State::default()),
        })
    }

    pub fn status(&self) -> Status {
        let state = self.state.lock().unwrap();
        Status {
            adapter: state.adapter.clone(),
            devices: state.devices.clone(),
            prompt: self.prompter.current(),
            approvals: state
                .approvals
                .iter()
                .map(|waiting| waiting.approval.clone())
                .collect(),
            scan_seconds_left: state.scan_until.map_or(0, |until| {
                until
                    .saturating_duration_since(Instant::now())
                    .as_secs_f32()
                    .ceil() as u32
            }),
            message: state.message.clone(),
            error: state.error.clone(),
        }
    }

    /// Reread the radio, expire stale approvals and stop a search that has run its course.
    pub async fn refresh(&self) {
        let expired_scan = {
            let mut state = self.state.lock().unwrap();
            let now = Instant::now();
            state.approvals.retain(|waiting| waiting.expires > now);
            let expired = state.scan_until.is_some_and(|until| until <= now);
            if expired {
                state.scan_until = None;
            }
            expired
        };
        if expired_scan {
            let _ = self.backend.stop_scan().await;
            self.say("Search finished");
        }
        let adapter = self.backend.adapter().await;
        // A search nobody asked for (the timer is clear) must not keep running beside a stream.
        let orphaned = self.state.lock().unwrap().scan_until.is_none()
            && matches!(&adapter, Ok(Some(a)) if a.scanning);
        if orphaned {
            let _ = self.backend.stop_scan().await;
        }
        let adapter = if orphaned { self.backend.adapter().await } else { adapter };
        let devices = match &adapter {
            Ok(Some(_)) => self.backend.devices().await,
            _ => Ok(Vec::new()),
        };
        let mut state = self.state.lock().unwrap();
        match adapter {
            Ok(adapter) => {
                if adapter.as_ref().is_none_or(|a| !a.scanning) {
                    state.scan_until = None;
                }
                state.adapter = adapter;
            }
            Err(error) => {
                state.adapter = None;
                state.error = Some(error.0);
            }
        }
        match devices {
            Ok(mut devices) => {
                devices.retain(visible);
                for device in &mut devices {
                    device.busy = state.busy.contains(&device.address);
                    if device.name.is_empty() {
                        device.name = device.address.clone();
                    }
                }
                devices.sort_by_key(rank);
                state.devices = devices;
            }
            Err(error) => state.error = Some(error.0),
        }
    }

    /// Keep state current for the desktop; faster while something is happening.
    pub async fn run_background(self: Arc<Self>) {
        loop {
            self.refresh().await;
            let active = {
                let state = self.state.lock().unwrap();
                state.scan_until.is_some() || !state.busy.is_empty()
            } || self.prompter.current().is_some();
            tokio::time::sleep(Duration::from_millis(if active { 1000 } else { 4000 })).await;
        }
    }

    fn say(&self, message: impl Into<String>) {
        let mut state = self.state.lock().unwrap();
        state.message = message.into();
        state.error = None;
    }
    fn fail(&self, message: impl Into<String>) {
        self.state.lock().unwrap().error = Some(message.into());
    }
    fn device(&self, address: &str) -> Option<Device> {
        self.state
            .lock()
            .unwrap()
            .devices
            .iter()
            .find(|d| d.address == address)
            .cloned()
    }
    fn name_of(&self, address: &str) -> String {
        self.device(address)
            .map_or_else(|| address.to_owned(), |d| d.name)
    }

    pub async fn handle(self: &Arc<Self>, request: Request, peer: Peer) -> Reply {
        self.refresh().await;
        let result = self.dispatch(request, peer).await;
        self.refresh().await;
        let status = self.status();
        match result {
            Ok(message) => Reply {
                ok: true,
                message,
                status,
            },
            Err(error) => Reply {
                ok: false,
                message: error.0,
                status,
            },
        }
    }

    async fn dispatch(self: &Arc<Self>, request: Request, peer: Peer) -> Result<String> {
        let desktop = peer == Peer::Desktop;
        match request {
            Request::Status => Ok("Bluetooth status".into()),
            Request::Power { on } => {
                // Turning the radio off can strand a Bluetooth keyboard or mouse, so only the
                // person at the desktop may do it.
                if !on && !desktop {
                    return Err(Error::new(
                        "Turn Bluetooth off from Settings on the ANGI desktop.",
                    ));
                }
                self.backend.set_powered(on).await?;
                if !on {
                    self.state.lock().unwrap().scan_until = None;
                }
                self.say(if on {
                    "Bluetooth is on"
                } else {
                    "Bluetooth is off"
                });
                Ok(self.status_message())
            }
            Request::Scan { on: true } => {
                if !self
                    .state
                    .lock()
                    .unwrap()
                    .adapter
                    .as_ref()
                    .is_some_and(|a| a.powered)
                {
                    return Err(Error::new("Turn Bluetooth on to search for devices."));
                }
                // Arm the timer first so a refresh never mistakes the new search for a stray one.
                self.state.lock().unwrap().scan_until =
                    Some(Instant::now() + Duration::from_secs(SCAN_SECONDS.into()));
                if let Err(error) = self.backend.start_scan().await {
                    self.state.lock().unwrap().scan_until = None;
                    return Err(error);
                }
                self.say("Searching for devices…");
                Ok(self.status_message())
            }
            Request::Scan { on: false } => {
                self.backend.stop_scan().await?;
                self.state.lock().unwrap().scan_until = None;
                self.say("Search stopped");
                Ok(self.status_message())
            }
            Request::Pair { address } => {
                self.guarded(ApprovalAction::Pair, &address, desktop).await
            }
            Request::Connect { address } => {
                let address = known(&address)?;
                // Reaching a device the person already paired is routine; reaching a new one is not.
                let routine = self.device(&address).is_some_and(|d| d.paired);
                self.guarded(ApprovalAction::Connect, &address, desktop || routine)
                    .await
            }
            Request::Trust {
                address,
                trusted: true,
            } => self.guarded(ApprovalAction::Trust, &address, desktop).await,
            Request::Trust {
                address,
                trusted: false,
            } => {
                self.perform(ApprovalAction::Trust, &known(&address)?, false)
                    .await
            }
            Request::Forget { address } => {
                self.guarded(ApprovalAction::Forget, &address, desktop)
                    .await
            }
            Request::Disconnect { address } => {
                let address = known(&address)?;
                self.backend.disconnect(&address).await?;
                let message = format!("Disconnected from {}", self.name_of(&address));
                self.say(message.clone());
                Ok(message)
            }
            Request::Respond { id, accept, value } => {
                if !desktop {
                    return Err(Error::new(
                        "Only the ANGI desktop can answer pairing prompts.",
                    ));
                }
                if self.prompter.respond(id, accept, value) {
                    Ok("Answer sent".into())
                } else {
                    Err(Error::new(
                        "That prompt has ended, or the PIN or passkey was not valid.",
                    ))
                }
            }
            Request::Decide { id, allow } => {
                if !desktop {
                    return Err(Error::new(
                        "Only the ANGI desktop can allow or decline requests.",
                    ));
                }
                let waiting = {
                    let mut state = self.state.lock().unwrap();
                    state
                        .approvals
                        .iter()
                        .position(|w| w.approval.id == id)
                        .map(|index| state.approvals.remove(index))
                };
                let Some(Waiting { approval, .. }) = waiting else {
                    return Err(Error::new("That request has expired."));
                };
                if !allow {
                    self.say(format!(
                        "Declined: {} {}",
                        approval.action.label(),
                        approval.name
                    ));
                    return Ok("Declined".into());
                }
                self.perform(approval.action, &approval.address, true).await
            }
        }
    }

    fn status_message(&self) -> String {
        self.state.lock().unwrap().message.clone()
    }

    /// Run `action` now if `allowed`, otherwise hold it for the person at the desktop.
    async fn guarded(
        self: &Arc<Self>,
        action: ApprovalAction,
        address: &str,
        allowed: bool,
    ) -> Result<String> {
        let address = known(address)?;
        if self.device(&address).is_none() {
            return Err(Error::new(
                "That device is not in range. Search for devices first.",
            ));
        }
        if allowed {
            return self.perform(action, &address, true).await;
        }
        let name = self.name_of(&address);
        let mut state = self.state.lock().unwrap();
        let exists = state
            .approvals
            .iter()
            .any(|w| w.approval.action == action && w.approval.address == address);
        if !exists {
            if state.approvals.len() >= MAX_APPROVALS {
                return Err(Error::new("Too many requests are waiting for approval."));
            }
            state.next_approval = state.next_approval.wrapping_add(1).max(1);
            let approval = Approval {
                id: state.next_approval,
                action,
                address,
                name: name.clone(),
            };
            let expires = Instant::now() + Duration::from_secs(PROMPT_SECONDS.into());
            state.approvals.push(Waiting { approval, expires });
        }
        Ok(format!(
            "Waiting for approval: allow ANGI to {} {name} in Settings → Bluetooth.",
            action.label()
        ))
    }

    async fn perform(
        self: &Arc<Self>,
        action: ApprovalAction,
        address: &str,
        enable: bool,
    ) -> Result<String> {
        let name = self.name_of(address);
        match action {
            ApprovalAction::Pair | ApprovalAction::Connect => {
                if !self.state.lock().unwrap().busy.insert(address.to_owned()) {
                    return Err(Error::new(format!("Already working with {name}.")));
                }
                let (core, address) = (Arc::clone(self), address.to_owned());
                tokio::spawn(async move {
                    core.connect_flow(&address, action == ApprovalAction::Pair)
                        .await
                });
                Ok(if action == ApprovalAction::Pair {
                    format!("Pairing with {name}…")
                } else {
                    format!("Connecting to {name}…")
                })
            }
            ApprovalAction::Trust => {
                self.backend.set_trusted(address, enable).await?;
                let message = if enable {
                    format!("{name} will reconnect automatically")
                } else {
                    format!("{name} will no longer reconnect automatically")
                };
                self.say(message.clone());
                Ok(message)
            }
            ApprovalAction::Forget => {
                self.backend.forget(address).await?;
                let message = format!("Removed {name}");
                self.say(message.clone());
                Ok(message)
            }
        }
    }

    /// Pair (when asked), trust and connect. The caller has already marked the device busy.
    async fn connect_flow(&self, address: &str, pair: bool) {
        let name = self.name_of(address);
        let mut connected = false;
        let outcome = async {
            if pair {
                tokio::time::timeout(PAIR_TIMEOUT, self.backend.pair(address))
                    .await
                    .map_err(|_| Error::new(format!("Pairing with {name} timed out.")))??;
                // Pairing was the person's explicit choice, so keep the device reconnecting.
                self.backend.set_trusted(address, true).await?;
            }
            match tokio::time::timeout(CONNECT_TIMEOUT, self.backend.connect(address)).await {
                Ok(Ok(())) => {
                    connected = true;
                    Ok(if pair {
                        format!("Paired and connected to {name}")
                    } else {
                        format!("Connected to {name}")
                    })
                }
                Ok(Err(error)) if pair => Ok(format!(
                    "Paired with {name}, but it did not connect: {error}"
                )),
                Err(_) if pair => Ok(format!("Paired with {name}, but connecting timed out.")),
                Ok(Err(error)) => Err(error),
                Err(_) => Err(Error::new(format!("Connecting to {name} timed out."))),
            }
        }
        .await;
        self.state.lock().unwrap().busy.remove(address);
        if connected {
            // A running search shares the radio with the new link and makes audio skip.
            let _ = self.backend.stop_scan().await;
            self.state.lock().unwrap().scan_until = None;
        }
        match outcome {
            Ok(message) => self.say(message),
            Err(error) => self.fail(error.0),
        }
        self.refresh().await;
    }
}

fn known(address: &str) -> Result<String> {
    normalize_address(address)
        .ok_or_else(|| Error::new("That is not a Bluetooth address (expected AA:BB:CC:DD:EE:FF)."))
}
