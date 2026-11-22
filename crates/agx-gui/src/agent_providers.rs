//! Provider setup transport; OpenCode alone owns OAuth credentials.
use crate::{AgentProviders, App, AppWindow, CardModels, keymap::Cmd, wm::Kind};
use serde_json::{Value, json};
use slint::{ComponentHandle, ModelRc, Timer, TimerMode, VecModel};
use std::{cell::RefCell, io::{BufRead, BufReader, Write}, process::{Command, Stdio}, rc::Rc, sync::mpsc, time::{Duration, Instant}};

pub(super) fn install(ui: &AppWindow, app: &Rc<RefCell<App>>) -> Timer {
    let (send, receive) = mpsc::channel::<Value>();
    let (results, updates) = mpsc::channel::<(Value, Value)>();
    std::thread::spawn(move || {
        let child = Command::new("/usr/bin/agent").arg("provider-bridge")
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
        let Ok(mut child) = child else {
            for request in receive { let _ = results.send((request, json!({"error":"Provider setup requires the ANGI agent runtime."}))); }
            return;
        };
        let mut input = child.stdin.take().unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        for request in receive {
            let mut line = String::new();
            let response = if writeln!(input, "{request}").and_then(|_| input.flush()).is_ok() && output.read_line(&mut line).is_ok() {
                serde_json::from_str(&line).unwrap_or_else(|_| json!({"error":"Provider service stopped. Refresh the desktop to reconnect."}))
            } else { json!({"error":"Provider service stopped. Refresh the desktop to reconnect."}) };
            if results.send((request, response)).is_err() { break; }
        }
        drop(input);
        let _ = child.wait();
    });
    let (weak, sender) = (ui.as_weak(), send.clone());
    ui.global::<AgentProviders>().on_action(move |action, value| {
        let Some(ui) = weak.upgrade() else { return; };
        let global = ui.global::<AgentProviders>();
        let provider = global.get_provider().to_string();
        let request = match action {
            0 => json!({"action":"status","provider":provider}),
            1 => json!({"action":"connect","provider":provider}),
            2 => json!({"action":"apple","provider":provider,"address":value.as_str()}),
            3 => json!({"action":"activate","provider":provider}),
            5 => json!({"action":"tailscale","provider":"lmstudio"}),
            6 => json!({"action":"lmstudio","provider":"lmstudio","address":value.as_str()}),
            _ => return,
        };
        global.set_busy(true);
        let _ = sender.send(request);
    });
    let _ = send.send(json!({"action":"overview"}));
    let (weak, sender) = (ui.as_weak(), send.clone());
    ui.global::<AgentProviders>().on_save(move |role, model| {
        let Some(ui) = weak.upgrade() else { return; };
        let global = ui.global::<AgentProviders>();
        global.set_busy(true);
        let _ = sender.send(json!({"action":"save","provider":global.get_provider().as_str(),"role":role.as_str(),"model":model.as_str()}));
    });
    let (weak, state) = (ui.as_weak(), app.clone());
    ui.global::<AgentProviders>().on_open_link(move |url| {
        if let (Some(ui), Ok(mut app)) = (weak.upgrade(), state.try_borrow_mut()) {
            if url.starts_with("https://") {
                app.run(&ui, Cmd::Open(Kind::Web));
                app.browser_navigate(&ui, &url);
            }
        }
    });
    let timer = Timer::default();
    let (weak, state) = (ui.as_weak(), app.clone());
    let mut last_poll = Instant::now();
    let mut last_apple_check = None;
    let mut checking_apple = false;
    timer.start(TimerMode::Repeated, Duration::from_millis(150), move || {
        let Some(ui) = weak.upgrade() else { return; };
        let global = ui.global::<AgentProviders>();
        while let Ok((request, response)) = updates.try_recv() {
            if let Some(cards) = response["overview"].as_object() { show_cards(&global, cards); }
            if request["provider"] == "apple" {
                last_apple_check = Some(Instant::now());
                if request["background"] == true {
                    checking_apple = false;
                    if global.get_open() && global.get_provider() == "apple" && !global.get_busy() {
                        if let Some(connected) = response["connected"].as_bool() {
                            global.set_connected(connected);
                        }
                        if let Some(status) = response["status"].as_str() {
                            global.set_status(status.into());
                        }
                    }
                    continue;
                }
            }
            if request["action"] == "overview" { continue; }
            if request["provider"].as_str() != Some(global.get_provider().as_str()) { continue; }
            global.set_busy(false);
            if let Some(error) = response["error"].as_str() { global.set_status(error.into()); continue; }
            if request["provider"] == "apple" && global.get_apple_address().is_empty() {
                if let Some(address) = response["address"].as_str() {
                    global.set_apple_address(address.into());
                }
            }
            if let Some(status) = response["status"].as_str() { global.set_status(status.into()); }
            if let Some(tailscale) = response["tailscale"].as_object() {
                let ready = tailscale.get("ready").and_then(Value::as_bool).unwrap_or(false);
                global.set_tailscale(tailscale.get("text").and_then(Value::as_str).unwrap_or("").into());
                global.set_tailscale_ready(ready);
                global.set_tailscale_login(tailscale.get("login").and_then(Value::as_bool).unwrap_or(false));
                if ready { global.set_watching(false); }
            }
            if let Some(connected) = response["connected"].as_bool() {
                global.set_connected(connected);
                // OpenAI's browser sign-in ends when connected; LM Studio's ends when Tailscale is up (above).
                if connected && global.get_provider() != "lmstudio" { global.set_watching(false); }
            }
            if let Some(models) = response["models"].as_array() {
                global.set_models(ModelRc::new(VecModel::from(models.iter().filter_map(Value::as_str).map(Into::into).collect::<Vec<_>>())));
            }
            if let Some(choices) = response["choices"].as_object() {
                // Always reset every role, so a service with no choice never shows another service's model.
                let role = |name: &str| choices.get(name).and_then(Value::as_str).unwrap_or("").into();
                global.set_orchestrator(role("orchestrator"));
                global.set_builder(role("builder"));
                global.set_tester(role("tester"));
                global.set_os_updates(role("os"));
                global.set_single_agent(role("single"));
                // The dropdowns show a model by position, so find each saved model in the list just received.
                if let Some(models) = response["models"].as_array() {
                    let position = |name: &str| choices.get(name).and_then(Value::as_str)
                        .and_then(|chosen| models.iter().position(|model| model.as_str() == Some(chosen))).map_or(-1, |index| index as i32);
                    global.set_role_index(ModelRc::new(VecModel::from(vec![position("orchestrator"), position("builder"), position("tester"), position("os"), position("single")])));
                }
            }
            if let Some(url) = response["url"].as_str().filter(|url| url.starts_with("https://")) {
                global.set_watching(true);
                if let Ok(mut app) = state.try_borrow_mut() {
                    app.run(&ui, Cmd::Open(Kind::Web));
                    app.browser_navigate(&ui, url);
                }
            }
        }
        if global.get_open() && global.get_watching() && matches!(global.get_provider().as_str(), "openai" | "lmstudio") && !global.get_busy() && last_poll.elapsed() > Duration::from_secs(3) {
            last_poll = Instant::now();
            global.set_busy(true);
            let _ = send.send(json!({"action":"status","provider":global.get_provider().as_str()}));
        }
        if !global.get_open() {
            last_apple_check = None;
        } else if !global.get_busy() && !checking_apple
            && last_apple_check.is_none_or(|checked| checked.elapsed() >= Duration::from_secs(5))
        {
            checking_apple = send.send(json!({"action":"status","provider":"apple","background":true})).is_ok();
        }
    });
    timer
}

/// Card state for Apple Intelligence, OpenAI and LM Studio, in the order the cards are shown.
fn show_cards(global: &AgentProviders<'_>, cards: &serde_json::Map<String, Value>) {
    let order = ["apple", "openai", "lmstudio"];
    let flag = |name: &str, key: &str| cards.get(name).and_then(|card| card[key].as_bool()).unwrap_or(false);
    global.set_apple_configured(flag("apple", "configured"));
    global.set_ready(ModelRc::new(VecModel::from(order.map(|name| flag(name, "connected")).to_vec())));
    global.set_active_index(order.iter().position(|name| flag(name, "active")).map_or(-1, |index| index as i32));
    // Footer text omits the provider prefix: "openai/gpt-6.1-sol" reads as "gpt-6.1-sol".
    let model = |name: &str, role: &str| -> slint::SharedString {
        let full = cards.get(name).and_then(|card| card["models"][role].as_str()).unwrap_or("");
        full.split_once('/').map_or(full, |(_, short)| short).into()
    };
    let models = order.map(|name| CardModels { orchestrator: model(name, "orchestrator"), builder: model(name, "builder"), tester: model(name, "tester"), os: model(name, "os") });
    global.set_card_models(ModelRc::new(VecModel::from(models.to_vec())));
}
