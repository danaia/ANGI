//! The library worker: runs `angi-update` for installs and reads its JSON events.
use super::{Job, Request, State};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    process::{Command, Stdio},
    sync::{Arc, Mutex, mpsc::Receiver},
};

fn program() -> String {
    std::env::var("AGX_UPDATE_PROGRAM").unwrap_or_else(|_| "/usr/bin/angi-update".into())
}

fn lock(state: &Mutex<State>) -> std::sync::MutexGuard<'_, State> {
    state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(super) fn run(state: &Arc<Mutex<State>>, requests: Receiver<Request>) {
    for request in requests {
        match request {
            // A check can wait on the network; it must not hold up an install behind it.
            Request::Refresh { check: true } => {
                let state = state.clone();
                std::thread::spawn(move || refresh(&state, true));
            }
            Request::Refresh { check: false } => refresh(state, false),
            Request::Install(id) => job(state, &id, false),
            Request::Remove(id) => job(state, &id, true),
        }
    }
}

/// Reads `angi-update apps` (local files only), or `apps --check` to also refresh stale update checks.
fn refresh(state: &Mutex<State>, check: bool) {
    refresh_with(&program(), state, check);
}

fn refresh_with(program: &str, state: &Mutex<State>, check: bool) {
    let mut command = Command::new(program);
    command.arg("apps");
    if check {
        command.arg("--check");
    }
    let Ok(output) = command.stdin(Stdio::null()).stderr(Stdio::null()).output() else {
        return;
    };
    if let Ok(listing) = serde_json::from_slice::<Value>(&output.stdout) {
        apply_listing(&mut lock(state), &listing);
    }
}

pub(super) fn apply_listing(state: &mut State, listing: &Value) {
    let Some(apps) = listing["apps"].as_object() else {
        return;
    };
    // The installer decides what counts as newer and how a tag reads ("jq-1.8.2" shows as "1.8.2"),
    // so the desktop never compares release-tag formats itself.
    state.installed = apps
        .iter()
        .filter_map(|(id, item)| {
            let shown = item["version"].as_str().or_else(|| item["tag"].as_str())?;
            Some((id.clone(), shown.to_string()))
        })
        .collect();
    state.latest = apps
        .iter()
        .map(|(id, item)| (id.clone(), item["update"].as_str().unwrap_or("").to_string()))
        .collect();
    if let Some(unavailable) = listing["unavailable"].as_object() {
        state.catalogue_checked = true;
        state.unavailable = unavailable
            .iter()
            .filter_map(|(id, reason)| Some((id.clone(), reason.as_str()?.to_owned())))
            .collect();
    }
    state.changed();
}

fn job(state: &Mutex<State>, id: &str, removing: bool) {
    job_with(&program(), state, id, removing);
}

pub(super) fn job_with(program: &str, state: &Mutex<State>, id: &str, removing: bool) {
    let action = if removing { "remove" } else { "install" };
    let child = Command::new(program)
        .args(["app", action, id, "--progress"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(error) => {
            finish(&mut lock(state), id, Err(format!("The installer could not start: {error}")));
            return;
        }
    };
    // Drain stderr so a chatty child can never block on a full pipe; keep the tail for diagnosis.
    let errors = child.stderr.take().map(|pipe| {
        std::thread::spawn(move || {
            BufReader::new(pipe)
                .lines()
                .map_while(Result::ok)
                .last()
                .unwrap_or_default()
        })
    });
    let mut outcome = None;
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(event) = serde_json::from_str::<Value>(&line) {
                if let Some(result) = apply_event(&mut lock(state), id, removing, &event) {
                    outcome = Some(result);
                }
            }
        }
    }
    let _ = child.wait();
    let stderr = errors.and_then(|handle| handle.join().ok()).unwrap_or_default();
    // The installer's last event is the verdict; without one it died, and its stderr says why.
    let outcome = outcome.unwrap_or_else(|| {
        Err(if stderr.is_empty() {
            "The installer stopped before finishing. Nothing was changed.".to_string()
        } else {
            stderr
        })
    });
    finish(&mut lock(state), id, outcome);
    refresh_with(program, state, false);
}

/// Folds one installer event into the state. Returns the final outcome once the event is terminal.
pub(super) fn apply_event(
    state: &mut State,
    id: &str,
    removing: bool,
    event: &Value,
) -> Option<Result<String, String>> {
    let message = event["message"].as_str().unwrap_or("").to_string();
    match event["state"].as_str()? {
        "running" => {
            let fraction = event["fraction"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0) as f32;
            state.jobs.insert(id.into(), Job::Working { fraction, message, removing });
            state.changed();
            None
        }
        "succeeded" => Some(Ok(message)),
        "failed" => Some(Err(message)),
        _ => None,
    }
}

/// Records the end of a job and queues its outcome for a toast.
pub(super) fn finish(state: &mut State, id: &str, outcome: Result<String, String>) {
    match outcome {
        Ok(message) => {
            state.jobs.remove(id);
            state.notices.push(message);
        }
        Err(message) => {
            state.notices.push(message.clone());
            state.jobs.insert(id.into(), Job::Failed(message));
        }
    }
    state.changed();
}
