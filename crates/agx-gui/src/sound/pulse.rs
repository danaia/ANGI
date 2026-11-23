//! PulseAudio discovers connected Bluetooth profiles and owns active stream routing.
use super::{Choice, Control, Options, OutputVolume, Snapshot, VolumeAction, tone};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Seek, Write},
    os::fd::FromRawFd,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Defaults {
    default_sink_name: Option<String>,
    default_source_name: Option<String>,
}

#[derive(Deserialize)]
struct Endpoint {
    name: String,
    description: String,
    monitor_source: Option<String>,
    #[serde(default)]
    mute: bool,
    #[serde(default)]
    volume: BTreeMap<String, ChannelVolume>,
}

#[derive(Deserialize)]
struct ChannelVolume {
    value: u32,
}

impl Endpoint {
    fn output_volume(&self) -> OutputVolume {
        let peak = self
            .volume
            .values()
            .map(|channel| channel.value)
            .max()
            .unwrap_or(0);
        let percent = (f64::from(peak) * 100.0 / 65536.0).round() as i32;
        OutputVolume {
            percent,
            muted: self.mute,
        }
    }
}

#[derive(Deserialize)]
struct Card {
    name: String,
    properties: BTreeMap<String, String>,
    profiles: BTreeMap<String, Profile>,
    active_profile: Option<String>,
}

#[derive(Deserialize)]
struct Profile {
    description: String,
    sinks: u32,
    sources: u32,
    available: bool,
}

impl Profile {
    fn usable(&self) -> bool {
        self.available && (self.sinks != 0 || self.sources != 0)
    }
}

impl Card {
    fn label(&self) -> &str {
        self.properties
            .get("device.description")
            .map(String::as_str)
            .unwrap_or(&self.name)
    }

    fn headset_mode(&self) -> Option<&'static str> {
        self.name.strip_prefix("bluez_card.")?;
        ["handsfree_head_unit", "headset_head_unit"]
            .into_iter()
            .find(|mode| {
                self.profiles
                    .get(*mode)
                    .is_some_and(|p| p.available && p.sources > 0)
            })
    }

    fn bluetooth_source(&self, profile: &str) -> Option<String> {
        let device = self.name.strip_prefix("bluez_card.")?;
        Some(format!("bluez_source.{device}.{profile}"))
    }
}

fn pactl(args: &[&str]) -> Result<Vec<u8>, ()> {
    let result = Command::new("pactl").args(args).output().map_err(|_| ())?;
    result.status.success().then_some(result.stdout).ok_or(())
}

fn read<T: serde::de::DeserializeOwned>(args: &[&str]) -> Result<T, ()> {
    serde_json::from_slice(&pactl(args)?).map_err(|_| ())
}

fn endpoints(devices: Vec<Endpoint>, selected: Option<String>, input: bool) -> Options {
    Options {
        choices: devices
            .into_iter()
            .filter(|device| {
                (!input || device.monitor_source.as_deref().is_none_or(str::is_empty))
                    && device.name != "auto_null"
            })
            .map(|device| Choice {
                id: device.name,
                label: device.description,
            })
            .collect(),
        selected: selected.unwrap_or_default(),
    }
}

fn inputs(sources: Vec<Endpoint>, selected: Option<String>, cards: &[Card]) -> Options {
    let mut options = endpoints(sources, selected, true);
    for card in cards {
        let Some(mode) = card.headset_mode() else {
            continue;
        };
        // A headset source already appears in the regular device list in hands-free mode.
        if card.active_profile.as_deref().is_some_and(|active| {
            matches!(active, "handsfree_head_unit" | "headset_head_unit")
                && card
                    .bluetooth_source(active)
                    .is_some_and(|source| options.choices.iter().any(|choice| choice.id == source))
        }) {
            continue;
        }
        options.choices.push(Choice {
            id: format!("{}/{mode}", card.name),
            label: format!("{} · Hands-free", card.label()),
        });
    }
    options
}

fn profiles(cards: &[Card]) -> Options {
    let mut options = Options::default();
    let cards: Vec<_> = cards
        .iter()
        .filter(|card| {
            card.name.starts_with("bluez_card.") && card.profiles.values().any(Profile::usable)
        })
        .collect();
    let single_card = cards.len() == 1;
    for card in cards {
        for (id, profile) in &card.profiles {
            if !profile.usable() {
                continue;
            }
            let key = format!("{}/{}", card.name, id);
            let active = card.active_profile.as_deref() == Some(id);
            if single_card && active {
                options.selected = key.clone();
            }
            let state = if !single_card && active {
                " · Active"
            } else {
                ""
            };
            let label = card.label();
            options.choices.push(Choice {
                id: key,
                label: format!("{label} · {}{state}", profile.description),
            });
        }
    }
    options
}

pub(super) fn snapshot() -> Result<Snapshot, ()> {
    let defaults: Defaults = read(&["-f", "json", "info"])?;
    let sinks: Vec<Endpoint> = read(&["-f", "json", "list", "sinks"])?;
    let sources = read(&["-f", "json", "list", "sources"])?;
    let cards: Vec<Card> = read(&["-f", "json", "list", "cards"])?;
    let volume = sinks
        .iter()
        .find(|sink| {
            Some(&sink.name) == defaults.default_sink_name.as_ref() && sink.name != "auto_null"
        })
        .map(Endpoint::output_volume);
    Ok(Snapshot {
        volume,
        outputs: endpoints(sinks, defaults.default_sink_name, false),
        inputs: inputs(sources, defaults.default_source_name, &cards),
        profiles: profiles(&cards),
    })
}

fn select_bluetooth_input(name: &str, mode: &str) -> Result<(), &'static str> {
    const UNAVAILABLE: &str = "Bluetooth microphone unavailable";
    let cards: Vec<Card> = read(&["-f", "json", "list", "cards"]).map_err(|_| UNAVAILABLE)?;
    let card = cards
        .iter()
        .find(|card| card.name == name)
        .ok_or(UNAVAILABLE)?;
    if card.headset_mode() != Some(mode) {
        return Err(UNAVAILABLE);
    }
    let previous_mode = card
        .active_profile
        .as_deref()
        .filter(|previous| *previous != mode);
    let previous_output = if previous_mode.is_some() {
        let defaults: Defaults = read(&["-f", "json", "info"]).map_err(|_| UNAVAILABLE)?;
        defaults.default_sink_name
    } else {
        None
    };
    let source = card.bluetooth_source(mode).ok_or(UNAVAILABLE)?;
    if card.active_profile.as_deref() != Some(mode) {
        pactl(&["set-card-profile", name, mode])
            .map_err(|_| "Could not enable hands-free audio")?;
    }
    // The source can take a moment to appear after the Bluetooth profile changes.
    for attempt in 0..10 {
        if attempt != 0 {
            std::thread::sleep(Duration::from_millis(100));
        }
        if pactl(&["set-default-source", &source]).is_ok() {
            return Ok(());
        }
    }
    if let Some(previous) = previous_mode {
        if pactl(&["set-card-profile", name, previous]).is_ok() {
            if let Some(output) = previous_output {
                let _ = pactl(&["set-default-sink", &output]);
            }
        }
    }
    Err(UNAVAILABLE)
}

pub(super) fn select(control: Control, id: &str) -> Result<(), &'static str> {
    let failed = |_| "Could not change the sound device";
    match control {
        Control::Output => {
            pactl(&["set-default-sink", id]).map_err(failed)?;
        }
        Control::Input => {
            if let Some((card, mode)) = id.split_once('/') {
                select_bluetooth_input(card, mode)?;
            } else {
                pactl(&["set-default-source", id]).map_err(failed)?;
            }
        }
        Control::Profile => {
            let (card, profile) = id.split_once('/').ok_or("Bluetooth mode unavailable")?;
            pactl(&["set-card-profile", card, profile]).map_err(failed)?;
        }
    }
    // An input/profile change can succeed even on a system with no speaker.
    let output = match control {
        Control::Output => Some(id.to_owned()),
        _ => {
            let state = snapshot().map_err(|_| "Sound device changed. Test tone unavailable.")?;
            state.outputs.index().map(|_| state.outputs.selected)
        }
    };
    if let Some(output) = output {
        play_tone(Some(&output))
            .map_err(|_| "Sound device changed. Could not play the test tone.")?;
    }
    Ok(())
}

pub(super) fn change_volume(action: VolumeAction) -> Result<(), &'static str> {
    const FAILED: &str = "Could not change the output volume";
    let default: Defaults = read(&["-f", "json", "info"]).map_err(|_| FAILED)?;
    let Some(sink) = default.default_sink_name.filter(|sink| sink != "auto_null") else {
        return Err("No output device");
    };
    let unmute = match action {
        VolumeAction::Mute => {
            pactl(&["set-sink-mute", &sink, "toggle"]).map_err(|_| FAILED)?;
            return Ok(());
        }
        VolumeAction::Set { device, percent } => {
            if sink != device {
                return Err("The output device changed");
            }
            pactl(&[
                "set-sink-volume",
                &sink,
                &format!("{}%", percent.clamp(0, 100)),
            ])
            .map_err(|_| FAILED)?;
            percent > 0
        }
        VolumeAction::Step(step) => {
            let sinks: Vec<Endpoint> =
                read(&["-f", "json", "list", "sinks"]).map_err(|_| FAILED)?;
            let output = sinks
                .iter()
                .find(|output| output.name == sink)
                .ok_or(FAILED)?;
            let current = output.output_volume().percent;
            let next = (current + step).clamp(0, 100);
            if next != current {
                pactl(&["set-sink-volume", &sink, &format!("{:+}%", next - current)])
                    .map_err(|_| FAILED)?;
            }
            step > 0
        }
    };
    if unmute {
        pactl(&["set-sink-mute", &sink, "0"]).map_err(|_| FAILED)?;
    }
    Ok(())
}

pub(super) fn play_tone(sink: Option<&str>) -> Result<(), &'static str> {
    play(sink).ok_or("Could not play the test tone")
}

/// The little tone for a volume key. It starts the player and returns, so key repeat is never
/// held up waiting for the sound to finish; a finished player is reaped in the background.
pub(super) fn play_blip(percent: i32) {
    static LAST: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);
    {
        let Ok(mut last) = LAST.lock() else { return };
        let now = Instant::now();
        if last.is_some_and(|at| now.duration_since(at) < Duration::from_millis(70)) {
            return;
        }
        *last = Some(now);
    }
    let Some(samples) = tone_file(&tone::blip(percent)) else { return };
    let Ok(mut child) = player(None, "ANGI volume").stdin(samples).spawn() else { return };
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}

fn tone_file(pcm: &[u8]) -> Option<File> {
    let fd = unsafe { libc::memfd_create(c"angi-sound-tone".as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return None;
    }
    let mut samples = unsafe { File::from_raw_fd(fd) };
    samples.write_all(pcm).ok()?;
    samples.rewind().ok()?;
    Some(samples)
}

fn player(sink: Option<&str>, stream: &str) -> Command {
    let mut player = Command::new("pacat");
    player.args([
        "--playback",
        "--raw",
        "--format=s16le",
        "--rate=48000",
        "--channels=1",
        "--latency-msec=80",
    ]);
    player.arg(format!("--stream-name={stream}"));
    if let Some(sink) = sink {
        player.args(["--device", sink]);
    }
    player.stdout(Stdio::null()).stderr(Stdio::null());
    player
}

fn play(sink: Option<&str>) -> Option<()> {
    let samples = tone_file(&tone::pcm())?;
    let mut child = player(sink, "ANGI sound test").stdin(samples).spawn().ok()?;
    let deadline = Instant::now() + Duration::from_secs(4);
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().ok()? {
            return status.success().then_some(());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let _ = child.kill();
    let _ = child.wait();
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_devices_exclude_speaker_monitors() {
        let devices = serde_json::from_str(
            r#"[
            {"name":"hdmi.monitor","description":"Monitor of HDMI","monitor_source":"hdmi"},
            {"name":"headset","description":"Headset microphone","monitor_source":""},
                {"name":"usb","description":"USB microphone","monitor_source":null}
        ]"#,
        )
        .unwrap();
        let options = endpoints(devices, Some("hdmi.monitor".into()), true);
        assert_eq!(options.choices.len(), 2);
        assert_eq!(options.id_at(1).as_deref(), Some("headset"));
        assert_eq!(options.id_at(2).as_deref(), Some("usb"));
        assert_eq!(options.index(), None);
    }

    #[test]
    fn bluetooth_modes_expose_playback_and_headset_capture() {
        let cards: Vec<Card> = serde_json::from_str(r#"[{
            "name":"bluez_card.test",
            "properties":{"device.description":"Headset"},
            "active_profile":"a2dp_sink",
            "profiles":{
                "off":{"description":"Off","sinks":0,"sources":0,"available":true},
                "a2dp_sink":{"description":"Stereo","sinks":1,"sources":0,"available":true},
                "handsfree_head_unit":{"description":"Headset","sinks":1,"sources":1,"available":true},
                "unsupported":{"description":"Unavailable","sinks":0,"sources":1,"available":false}
            }
        }]"#).unwrap();
        let options = profiles(&cards);
        assert_eq!(options.selected, "bluez_card.test/a2dp_sink");
        assert_eq!(options.choices.len(), 2);
        assert_eq!(
            options.id_at(1).as_deref(),
            Some("bluez_card.test/handsfree_head_unit")
        );

        let choices = inputs(Vec::new(), None, &cards);
        assert_eq!(
            choices.id_at(1).as_deref(),
            Some("bluez_card.test/handsfree_head_unit")
        );
        assert_eq!(choices.choices[0].label, "Headset · Hands-free");

        let mut cards = cards;
        cards[0].active_profile = Some("handsfree_head_unit".into());
        let source = "bluez_source.test.handsfree_head_unit";
        let choices = inputs(
            vec![Endpoint {
                name: source.into(),
                description: "Headset".into(),
                monitor_source: Some(String::new()),
                mute: false,
                volume: BTreeMap::new(),
            }],
            Some(source.into()),
            &cards,
        );
        assert_eq!(choices.choices.len(), 1);
        assert_eq!(choices.id_at(0).as_deref(), Some(source));

        cards[0]
            .profiles
            .get_mut("handsfree_head_unit")
            .unwrap()
            .available = false;
        assert!(inputs(Vec::new(), None, &cards).choices.is_empty());
    }
}
