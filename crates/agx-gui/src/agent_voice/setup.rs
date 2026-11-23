//! The speech-to-text overlay: list what suits this device, then download and save the choice.
use super::cloud::{self, Refusal};
use crate::{AgentVoice, VoiceChoice};
use serde_json::Value;
use slint::{ModelRc, SharedString, VecModel};
use std::sync::mpsc::Sender;

/// Everything the overlay's background work reports back.
pub(super) enum Update {
    Options(Result<Value, Refusal>),
    Progress(f32),
    /// An engine was saved (a model downloaded first when needed).
    Chosen(Result<(), Refusal>),
}

/// Ask the runtime what is set up, without opening anything; the answer says whether the microphone can record.
pub(super) fn refresh(updates: &Sender<Update>) {
    let tx = updates.clone();
    std::thread::spawn(move || {
        let _ = tx.send(Update::Options(cloud::options()));
    });
}

/// Open the overlay now; its contents fill in a moment later.
pub(super) fn open(voice: &AgentVoice<'_>, updates: &Sender<Update>) {
    voice.set_setup_error("".into());
    voice.set_setup_open(true);
    refresh(updates);
}

fn megabytes(bytes: u64) -> String {
    format!("{} MB", (bytes as f64 / 1_000_000.0).round())
}

/// Size and speed, as one line.
fn detail(model: &Value) -> String {
    let speed = model["seconds"].as_u64().unwrap_or(0);
    format!("{} · about {speed} s for 10 s of speech", megabytes(model["size"].as_u64().unwrap_or(0)))
}

pub(super) fn show(voice: &AgentVoice<'_>, options: &Value) {
    let device = &options["device"];
    let memory = device["memory_gb"].as_f64().unwrap_or(0.0);
    let name = device["device"].as_str().unwrap_or("");
    let summary = if name.is_empty() { String::new() } else { format!("{name} · {memory:.0} GB memory · {} cores", device["cores"].as_u64().unwrap_or(0)) };
    voice.set_device(summary.into());
    let choices: Vec<VoiceChoice> = options["models"].as_array().into_iter().flatten().map(|model| {
        let name = model["name"].as_str().unwrap_or("");
        let installed = model["installed"] == true;
        let size = megabytes(model["size"].as_u64().unwrap_or(0));
        VoiceChoice {
            id: model["id"].as_str().unwrap_or("").into(),
            name: name.into(),
            quality: model["quality"].as_str().unwrap_or("").into(),
            detail: detail(model).into(),
            action: if installed { format!("Use {name}") } else { format!("Download {name} · {size}") }.into(),
            installed,
            recommended: model["recommended"] == true,
            slow: model["slow"] == true,
        }
    }).collect();
    let models = options["models"].as_array().cloned().unwrap_or_default();
    // Start on the model in use, else the recommended one.
    let start = models.iter().position(|m| options["engine"] == "local" && m["id"] == options["model"])
        .or_else(|| models.iter().position(|m| m["recommended"] == true));
    voice.set_default_index(start.map_or(-1, |index| index as i32));
    voice.set_configured(options["ready"] == true);
    voice.set_choices(ModelRc::new(VecModel::from(choices)));
    voice.set_key_saved(options["key_saved"] == true);
    voice.set_local_ready(options["local_ready"] == true);
    let current: SharedString = match options["engine"].as_str() {
        Some("openai") => "openai".into(),
        Some("local") => options["model"].as_str().unwrap_or("").into(),
        _ => "".into(),
    };
    voice.set_current(current);
}

/// Use "openai" (saving `key` first when one was typed) or download and use a model.
pub(super) fn choose(voice: &AgentVoice<'_>, id: &str, key: &str, updates: &Sender<Update>) {
    if !voice.get_working().is_empty() {
        return;
    }
    voice.set_setup_error("".into());
    voice.set_progress(0.0);
    voice.set_working(id.into());
    let (id, key, tx) = (id.to_string(), key.trim().to_string(), updates.clone());
    std::thread::spawn(move || {
        let result = if id == "openai" {
            (if key.is_empty() { Ok(()) } else { cloud::save_key(&key) }).and_then(|()| cloud::choose("openai", None))
        } else {
            cloud::install(&id, |fraction| { let _ = tx.send(Update::Progress(fraction)); }).and_then(|()| cloud::choose("local", Some(&id)))
        };
        let _ = tx.send(Update::Chosen(result));
    });
}
