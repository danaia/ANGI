//! Microphone capture through `arecord`, with a live loudness reading for the waveform.
//!
//! `arecord` is the one capture path ANGI already ships (PulseAudio on the device, PipeWire on a
//! workstation). Audio stays in memory; nothing is written to disk.
use std::{
    io::Read,
    process::{Command, Stdio},
    sync::mpsc::Sender,
};

pub(super) const RATE: u32 = 16_000;
/// One loudness reading per 50 ms, so the waveform moves about twenty times a second.
const WINDOW_SAMPLES: usize = RATE as usize / 20;
/// Quieter than this over the whole recording means nobody spoke (about -40 dBFS).
const SILENT_PEAK: i16 = 330;
/// Under this there is nothing to transcribe (a click of the button).
pub(super) const MIN_SAMPLES: usize = RATE as usize * 3 / 10;

pub(super) enum Heard {
    Level(f32),
    /// The recording ended: its samples, and arecord's own complaint if it produced none.
    Finished(Vec<i16>, String),
}

/// Map an RMS amplitude (0..=32768) to 0..=1 on a decibel scale: -55 dBFS is silence, -15 is full.
pub(super) fn level(rms: f32) -> f32 {
    if rms < 1.0 {
        return 0.0;
    }
    ((20.0 * (rms / 32768.0).log10() + 55.0) / 40.0).clamp(0.0, 1.0)
}

fn rms(samples: &[i16]) -> f32 {
    let sum: f64 = samples.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
    (sum / samples.len().max(1) as f64).sqrt() as f32
}

pub(super) fn is_silent(samples: &[i16]) -> bool {
    samples.iter().all(|&s| s.unsigned_abs() < SILENT_PEAK as u16)
}

/// A mono 16-bit WAV file around the samples.
pub(super) fn wav(samples: &[i16]) -> Vec<u8> {
    let bytes = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + bytes as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + bytes).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&bytes.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

/// Start recording on a worker thread. Returns arecord's process ID, so the caller can end the
/// recording with SIGTERM; the thread then reports `Finished`. `None` when arecord cannot start.
pub(super) fn record(events: Sender<Heard>) -> Option<i32> {
    let mut child = Command::new("arecord")
        .args(["-q", "-t", "raw", "-f", "S16_LE", "-r", "16000", "-c", "1", "--period-time=50000"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let pid = child.id() as i32;
    let (mut out, mut err) = (child.stdout.take()?, child.stderr.take()?);
    std::thread::spawn(move || {
        let mut samples: Vec<i16> = Vec::new();
        let (mut chunk, mut carry) = ([0u8; 4096], Vec::<u8>::new());
        let mut window_start = 0;
        while let Ok(n) = out.read(&mut chunk) {
            if n == 0 {
                break;
            }
            carry.extend_from_slice(&chunk[..n]);
            let whole = carry.len() / 2 * 2;
            samples.extend(carry[..whole].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])));
            carry.drain(..whole);
            while samples.len() - window_start >= WINDOW_SAMPLES {
                let end = window_start + WINDOW_SAMPLES;
                let _ = events.send(Heard::Level(level(rms(&samples[window_start..end]))));
                window_start = end;
            }
        }
        let mut complaint = String::new();
        let _ = err.read_to_string(&mut complaint);
        let _ = child.wait();
        let _ = events.send(Heard::Finished(samples, complaint));
    });
    Some(pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loudness_runs_from_silence_to_full_on_a_decibel_scale() {
        assert_eq!(level(0.0), 0.0);
        assert!(level(20.0) < 0.05, "room noise stays flat");
        let speech = level(3000.0);
        assert!(speech > 0.6 && speech < 0.95, "ordinary speech lands high but not pinned: {speech}");
        assert_eq!(level(32768.0), 1.0);
        assert!(level(300.0) < level(3000.0));
    }

    #[test]
    fn a_recording_with_no_voice_is_silent() {
        assert!(is_silent(&[0, 10, -20, 300]));
        assert!(!is_silent(&[0, 10, -2000]));
        assert!(is_silent(&[]));
    }

    #[test]
    fn the_wav_header_describes_the_samples() {
        let file = wav(&[1, -1, 2]);
        assert_eq!(&file[..4], b"RIFF");
        assert_eq!(&file[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(file[24..28].try_into().unwrap()), 16_000);
        assert_eq!(u32::from_le_bytes(file[40..44].try_into().unwrap()), 6);
        assert_eq!(file.len(), 44 + 6);
        assert_eq!(i16::from_le_bytes([file[46], file[47]]), -1);
    }
}
