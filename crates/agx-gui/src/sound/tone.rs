//! A soft two-note confirmation with a short, diffuse reverb tail.
const RATE: usize = 48_000;

pub(super) fn pcm() -> Vec<u8> {
    let mut dry = vec![0.0; RATE * 4 / 5];
    for (frequency, start) in [(523.25, 0), (659.25, RATE / 12)] {
        for (i, sample) in dry[start..].iter_mut().take(RATE / 3).enumerate() {
            let t = i as f32 / RATE as f32;
            let phase = std::f32::consts::TAU * frequency * t;
            let attack = (t / 0.015).min(1.0);
            let release = ((1.0 / 3.0 - t) / 0.10).clamp(0.0, 1.0);
            let envelope = attack * release * (-8.0 * t).exp();
            *sample += 0.12 * envelope * (phase.sin() + 0.14 * (2.0 * phase).sin());
        }
    }
    let reflections = [
        (31, 0.22),
        (53, 0.15),
        (79, 0.10),
        (113, 0.07),
        (167, 0.04),
        (241, 0.02),
    ];
    let mut pcm = Vec::with_capacity(dry.len() * 2);
    for (i, direct) in dry.iter().enumerate() {
        let mut sample = *direct;
        for (delay, gain) in reflections {
            if let Some(j) = i.checked_sub(RATE * delay / 1000) {
                sample += gain * dry[j];
            }
        }
        pcm.extend_from_slice(&((32767.0 * sample) as i16).to_le_bytes());
    }
    pcm
}
