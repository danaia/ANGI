//! Dictation for the Agent Angi message box: record, show the live waveform, transcribe.
//!
//! Click the microphone to start and again to stop; the words are appended to the message for the
//! person to edit before sending. Capture is in `audio`, calls to the agent runtime (which owns the
//! credentials and the saved engine) are in `cloud`, and the first-use choice between an OpenAI key
//! and an on-device model is `setup`. Every transcription carries its recording's number, so a
//! cancelled or replaced recording can never write into the message.
mod audio;
mod cloud;
mod setup;

use crate::{AgentVoice, AppWindow};
use audio::Heard;
use cloud::Refusal;
use setup::Update;
use slint::{ComponentHandle, ModelRc, Timer, TimerMode, VecModel};
use std::{
    cell::RefCell,
    collections::VecDeque,
    rc::Rc,
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};

/// Levels kept for the waveform; the bar draws as many of the newest as its width holds.
const COLUMNS: usize = 240;
const LONGEST: Duration = Duration::from_secs(300);

struct Transcribed(u64, Result<String, Refusal>);

#[derive(Default)]
struct Recording {
    id: u64,
    pid: Option<i32>,
    started: Option<Instant>,
    heard: Option<Receiver<Heard>>,
    levels: VecDeque<f32>,
    /// The last recording, kept while the engine is chosen so nothing has to be said twice.
    retry: Option<Vec<u8>>,
}

pub(super) fn install(ui: &AppWindow) -> Timer {
    let state = Rc::new(RefCell::new(Recording::default()));
    let (transcripts, finished) = mpsc::channel::<Transcribed>();
    let (setups, updates) = mpsc::channel::<Update>();
    let voice = ui.global::<AgentVoice>();

    let (weak, shared, tx) = (ui.as_weak(), state.clone(), setups.clone());
    voice.on_toggle(move || {
        let Some(ui) = weak.upgrade() else { return; };
        let (voice, mut rec) = (ui.global::<AgentVoice>(), shared.borrow_mut());
        match voice.get_state().as_str() {
            // With nothing set up, the click opens the choice at once instead of recording.
            "idle" if !voice.get_configured() => setup::open(&voice, &tx),
            "idle" => begin(&voice, &mut rec),
            "listening" => end_recording(&voice, &mut rec),
            _ => {}
        }
    });

    let (weak, shared) = (ui.as_weak(), state.clone());
    voice.on_cancel(move || {
        let Some(ui) = weak.upgrade() else { return; };
        discard(&ui.global::<AgentVoice>(), &mut shared.borrow_mut());
    });

    let (weak, tx) = (ui.as_weak(), setups.clone());
    voice.on_open_setup(move || {
        let Some(ui) = weak.upgrade() else { return; };
        setup::open(&ui.global::<AgentVoice>(), &tx);
    });

    let (weak, shared) = (ui.as_weak(), state.clone());
    voice.on_close_setup(move || {
        let Some(ui) = weak.upgrade() else { return; };
        let voice = ui.global::<AgentVoice>();
        if voice.get_working().is_empty() {
            voice.set_setup_open(false);
            voice.set_pending(false);
            shared.borrow_mut().retry = None;
        }
    });

    let (weak, tx) = (ui.as_weak(), setups.clone());
    voice.on_choose(move |id, key| {
        let Some(ui) = weak.upgrade() else { return; };
        setup::choose(&ui.global::<AgentVoice>(), &id, &key, &tx);
    });

    setup::refresh(&setups);
    let (weak, timer) = (ui.as_weak(), Timer::default());
    timer.start(TimerMode::Repeated, Duration::from_millis(50), move || {
        let Some(ui) = weak.upgrade() else { return; };
        let (voice, mut rec) = (ui.global::<AgentVoice>(), state.borrow_mut());
        pump_audio(&voice, &mut rec, &transcripts);
        while let Ok(Transcribed(id, result)) = finished.try_recv() {
            if id == rec.id {
                settle(&voice, &mut rec, result, &setups);
            }
        }
        while let Ok(update) = updates.try_recv() {
            apply(&voice, &mut rec, update, &transcripts);
        }
    });
    timer
}

fn raise(voice: &AgentVoice<'_>, message: &str) {
    voice.set_alert(message.into());
}

fn signal(pid: Option<i32>) {
    if let Some(pid) = pid {
        // SAFETY: a plain signal to the arecord process this module started.
        unsafe { libc::kill(pid, libc::SIGTERM) };
    }
}

fn show_levels(voice: &AgentVoice<'_>, levels: &VecDeque<f32>) {
    voice.set_levels(ModelRc::new(VecModel::from(levels.iter().copied().collect::<Vec<_>>())));
}

fn begin(voice: &AgentVoice<'_>, rec: &mut Recording) {
    voice.set_alert("".into());
    rec.id += 1;
    rec.retry = None;
    let (heard, hear) = mpsc::channel();
    let Some(pid) = audio::record(heard) else {
        raise(voice, "The microphone could not start. Connect a microphone and try again.");
        return;
    };
    (rec.pid, rec.heard, rec.started) = (Some(pid), Some(hear), Some(Instant::now()));
    rec.levels = VecDeque::from(vec![0.0; COLUMNS]);
    show_levels(voice, &rec.levels);
    voice.set_seconds(0);
    voice.set_state("listening".into());
}

/// Stop listening; `pump_audio` receives what was recorded and sends it for transcription.
fn end_recording(voice: &AgentVoice<'_>, rec: &mut Recording) {
    signal(rec.pid.take());
    voice.set_state("transcribing".into());
}

fn discard(voice: &AgentVoice<'_>, rec: &mut Recording) {
    rec.id += 1;
    signal(rec.pid.take());
    (rec.heard, rec.started, rec.retry) = (None, None, None);
    voice.set_state("idle".into());
}

fn transcribe(voice: &AgentVoice<'_>, rec: &Recording, wav: Vec<u8>, transcripts: &Sender<Transcribed>) {
    voice.set_state("transcribing".into());
    let (id, tx) = (rec.id, transcripts.clone());
    std::thread::spawn(move || {
        let _ = tx.send(Transcribed(id, cloud::transcribe(&wav)));
    });
}

/// Move the waveform along, keep the clock, and take the finished recording when there is one.
fn pump_audio(voice: &AgentVoice<'_>, rec: &mut Recording, transcripts: &Sender<Transcribed>) {
    let Some(heard) = rec.heard.take() else { return; };
    let mut finished = None;
    let mut moved = false;
    while let Ok(event) = heard.try_recv() {
        match event {
            Heard::Level(level) => {
                // Quick to rise, slow to fall, so the dots ripple instead of flicker.
                let smoothed = level.max(rec.levels.back().copied().unwrap_or(0.0) * 0.55);
                rec.levels.pop_front();
                rec.levels.push_back(smoothed);
                moved = true;
            }
            Heard::Finished(samples, complaint) => finished = Some((samples, complaint)),
        }
    }
    if moved {
        show_levels(voice, &rec.levels);
    }
    let Some((samples, complaint)) = finished else {
        rec.heard = Some(heard);
        if let Some(started) = rec.started.filter(|_| voice.get_state() == "listening") {
            voice.set_seconds(started.elapsed().as_secs() as i32);
            if started.elapsed() > LONGEST {
                end_recording(voice, rec);
            }
        }
        return;
    };
    rec.pid = None;
    rec.started = None;
    if voice.get_state() == "idle" {
        return; // cancelled while the last audio was in flight
    }
    if samples.len() < audio::MIN_SAMPLES {
        voice.set_state("idle".into());
        if complaint.trim().is_empty() {
            raise(voice, "That was too short. Click the microphone, speak, then click it again.");
        } else {
            raise(voice, "No microphone was found. Connect a USB or Bluetooth microphone and try again.");
        }
    } else if audio::is_silent(&samples) {
        voice.set_state("idle".into());
        raise(voice, "No sound was picked up. Check that your microphone is connected and not muted.");
    } else {
        let wav = audio::wav(&samples);
        rec.retry = Some(wav.clone());
        transcribe(voice, rec, wav, transcripts);
    }
}

/// A transcription came back: the words, the setup overlay when no engine is chosen, or an alert.
fn settle(voice: &AgentVoice<'_>, rec: &mut Recording, result: Result<String, Refusal>, setups: &Sender<Update>) {
    voice.set_state("idle".into());
    match result {
        Ok(text) if text.is_empty() => {
            rec.retry = None;
            raise(voice, "No words were recognised. Try again a little closer to the microphone.");
        }
        Ok(text) => {
            rec.retry = None;
            voice.set_transcript(text.into());
            voice.set_serial(voice.get_serial() + 1);
        }
        Err(refusal) if matches!(refusal.code.as_str(), "choose-engine" | "needs-key") => {
            // Keep the recording and let the person choose how it is transcribed.
            voice.set_pending(true);
            setup::open(voice, setups);
            if refusal.code == "needs-key" {
                voice.set_setup_error(refusal.message.into());
            }
        }
        Err(refusal) => {
            rec.retry = None;
            raise(voice, &refusal.message);
        }
    }
}

fn apply(voice: &AgentVoice<'_>, rec: &mut Recording, update: Update, transcripts: &Sender<Transcribed>) {
    match update {
        Update::Options(Ok(options)) => setup::show(voice, &options),
        Update::Options(Err(refusal)) => voice.set_setup_error(refusal.message.into()),
        Update::Progress(fraction) => voice.set_progress(fraction),
        Update::Chosen(result) => {
            voice.set_working("".into());
            voice.set_progress(0.0);
            match result {
                Err(refusal) => voice.set_setup_error(refusal.message.into()),
                Ok(()) => {
                    voice.set_configured(true);
                    voice.set_setup_open(false);
                    voice.set_pending(false);
                    if let Some(wav) = rec.retry.clone() {
                        transcribe(voice, rec, wav, transcripts);
                    }
                }
            }
        }
    }
}
