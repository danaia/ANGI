//! Sound device discovery and changes run outside the Slint event loop.
mod meter;
mod pulse;
mod tone;

use crate::{AppWindow, SoundSettings};
use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::{cell::RefCell, rc::Rc, sync::mpsc, time::Duration};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Choice {
    id: String,
    label: String,
}

#[derive(Default, PartialEq, Eq)]
struct Options {
    choices: Vec<Choice>,
    selected: String,
}

impl Options {
    fn index(&self) -> Option<usize> {
        self.choices
            .iter()
            .position(|choice| choice.id == self.selected)
    }

    fn id_at(&self, index: i32) -> Option<String> {
        let offset = i32::from(self.index().is_none());
        self.choices
            .get(usize::try_from(index - offset).ok()?)
            .map(|choice| choice.id.clone())
    }

    fn model(&self, placeholder: &str) -> ModelRc<SharedString> {
        let labels: Vec<_> = self
            .index()
            .is_none()
            .then(|| SharedString::from(placeholder))
            .into_iter()
            .chain(
                self.choices
                    .iter()
                    .map(|choice| SharedString::from(&choice.label)),
            )
            .collect();
        ModelRc::from(Rc::new(VecModel::from(labels)))
    }
}

#[derive(Default, PartialEq, Eq)]
struct Snapshot {
    outputs: Options,
    inputs: Options,
    profiles: Options,
    volume: Option<OutputVolume>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct OutputVolume {
    percent: i32,
    muted: bool,
}

pub(super) enum VolumeAction {
    Set { device: String, percent: i32 },
    Step(i32),
    Mute,
}

thread_local! {
    static MEDIA_KEYS: RefCell<Option<mpsc::Sender<Request>>> = const { RefCell::new(None) };
}

pub(crate) fn media_key(action: VolumeAction, pressed: bool) -> bool {
    MEDIA_KEYS.with(|keys| {
        keys.borrow()
            .as_ref()
            .is_some_and(|sender| !pressed || sender.send(Request::Volume(action)).is_ok())
    })
}

#[derive(Clone, Copy)]
enum Control {
    Output,
    Input,
    Profile,
}

impl Snapshot {
    fn options(&self, control: Control) -> &Options {
        match control {
            Control::Output => &self.outputs,
            Control::Input => &self.inputs,
            Control::Profile => &self.profiles,
        }
    }

    fn show(&self, global: &SoundSettings<'_>, changed: bool) {
        if changed {
            global.set_outputs(self.outputs.model(if self.outputs.choices.is_empty() {
                "No output device"
            } else {
                "Select an output"
            }));
            global.set_inputs(self.inputs.model(if self.inputs.choices.is_empty() {
                "No microphone"
            } else {
                "Select a microphone"
            }));
            global.set_profiles(self.profiles.model("Choose a Bluetooth mode"));
            global.set_has_profiles(!self.profiles.choices.is_empty());
            global.set_has_output(self.outputs.index().is_some());
            global.set_has_input(self.inputs.index().is_some());
        }
        if !global.get_volume_dragging() {
            global.set_volume(self.volume.map_or(0.0, |volume| volume.percent as f32));
        }
        global.set_muted(self.volume.is_some_and(|volume| volume.muted));
        global.set_output_index(self.outputs.index().unwrap_or(0) as i32);
        global.set_input_index(self.inputs.index().unwrap_or(0) as i32);
        global.set_profile_index(self.profiles.index().unwrap_or(0) as i32);
    }
}

enum Request {
    Refresh,
    Select(Control, String),
    Test,
    Volume(VolumeAction),
}

pub(super) fn install(ui: &AppWindow) -> Timer {
    let (send, requests) = mpsc::channel();
    let (reply, updates) = mpsc::channel();
    MEDIA_KEYS.with(|keys| *keys.borrow_mut() = Some(send.clone()));
    std::thread::spawn(move || {
        for request in requests {
            let volume = matches!(request, Request::Volume(_));
            let action = match request {
                Request::Refresh => None,
                Request::Select(control, id) => Some(pulse::select(control, &id)),
                Request::Test => Some(pulse::play_tone(None)),
                Request::Volume(action) => Some(pulse::change_volume(action)),
            };
            if reply.send((action, volume, pulse::snapshot())).is_err() {
                break;
            }
        }
    });

    let global = ui.global::<SoundSettings>();
    let snapshot = Rc::new(RefCell::new(Snapshot::default()));
    let select = send.clone();
    let displayed = snapshot.clone();
    let weak = ui.as_weak();
    global.on_select(move |kind, index| {
        let Some(ui) = weak.upgrade() else { return };
        let global = ui.global::<SoundSettings>();
        if global.get_changing() {
            return;
        }
        let control = match kind {
            0 => Control::Output,
            1 => Control::Input,
            _ => Control::Profile,
        };
        let displayed = displayed.borrow();
        if let Some(id) = displayed.options(control).id_at(index) {
            global.set_changing(select.send(Request::Select(control, id)).is_ok());
        } else {
            displayed.show(&global, false);
        }
    });
    let test = send.clone();
    let weak = ui.as_weak();
    global.on_test(move || {
        if let Some(ui) = weak.upgrade() {
            let global = ui.global::<SoundSettings>();
            if !global.get_changing() {
                global.set_changing(test.send(Request::Test).is_ok());
            }
        }
    });
    let volume = send.clone();
    let displayed = snapshot.clone();
    global.on_adjust_volume(move |percent| {
        let displayed = displayed.borrow();
        if displayed.outputs.index().is_some() {
            let device = displayed.outputs.selected.clone();
            let _ = volume.send(Request::Volume(VolumeAction::Set {
                device,
                percent: percent.round() as i32,
            }));
        }
    });
    global.set_radio_available(std::path::Path::new("/usr/bin/cliamp").is_file());
    snapshot.borrow().show(&global, true);

    let weak = ui.as_weak();
    let timer = Timer::default();
    let poll = RefCell::new((false, 0usize, meter::Meter::new()));
    timer.start(TimerMode::Repeated, Duration::from_millis(120), move || {
        let Some(ui) = weak.upgrade() else { return };
        let global = ui.global::<SoundSettings>();
        let mut poll = poll.borrow_mut();
        let (pending, ticks, meter) = &mut *poll;
        while let Ok((action, volume, result)) = updates.try_recv() {
            if action.is_none() {
                *pending = false;
            } else if !volume {
                global.set_changing(false);
            }
            if volume && !global.get_active() {
                continue;
            }
            let available = result.is_ok();
            if let Some(action) = action {
                global.set_status(action.err().unwrap_or_default().into());
            } else if available && !global.get_available() {
                global.set_status("".into());
            }
            global.set_available(available);
            let state = result.unwrap_or_default();
            let mut last = snapshot.borrow_mut();
            if !global.get_changing() {
                let choices_changed = state.outputs != last.outputs
                    || state.inputs != last.inputs
                    || state.profiles != last.profiles;
                state.show(&global, choices_changed);
                *last = state;
            }
        }
        let current = snapshot.borrow();
        // The panel can be destroyed before Slint clears its active property.
        let active = global.get_active()
            && ui.get_wins().iter().any(|window| {
                window.kind == crate::wm::Kind::Settings.index() as i32
                    && window.focused
                    && !window.collapsed
            });
        let microphone = (active && !global.get_changing() && current.inputs.index().is_some())
            .then_some(current.inputs.selected.as_str());
        meter.update(microphone);
        global.set_meter_level(if active { meter.level() } else { -1 });
        if !active {
            global.set_volume_dragging(false);
            *ticks = 0;
        } else {
            if *ticks == 0 && !*pending && !global.get_changing() {
                *pending = send.send(Request::Refresh).is_ok();
            }
            *ticks = (*ticks + 1) % 15;
        }
    });
    timer
}
