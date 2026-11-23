//! Soft sounds with a short, diffuse reverb tail: a two-note confirmation and a volume blip.
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
    reverb(&dry)
}

/// One warm, rounded note for a volume key press. The pitch follows the new level (a fifth
/// across the range), so stepping up sounds like climbing and stepping down like settling.
pub(super) fn blip(percent: i32) -> Vec<u8> {
    let frequency = 392.0 * 2f32.powf(percent.clamp(0, 100) as f32 / 100.0 * 7.0 / 12.0);
    let mut dry = vec![0.0; RATE * 11 / 20];
    for (i, sample) in dry.iter_mut().take(RATE / 5).enumerate() {
        let t = i as f32 / RATE as f32;
        let phase = std::f32::consts::TAU * frequency * t;
        let attack = (t / 0.008).min(1.0);
        let release = ((0.2 - t) / 0.06).clamp(0.0, 1.0);
        let envelope = attack * release * (-14.0 * t).exp();
        *sample = 0.16 * envelope * (phase.sin() + 0.2 * (2.0 * phase).sin() + 0.05 * (3.0 * phase).sin());
    }
    reverb(&dry)
}

fn reverb(dry: &[f32]) -> Vec<u8> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blip_is_short_quiet_and_rises_with_the_level() {
        let (low, high) = (blip(0), blip(100));
        assert_eq!(low.len(), high.len());
        assert!(low.len() <= RATE * 2 * 3 / 5, "under about 0.6 s of mono 16-bit audio");
        let peak = |pcm: &[u8]| pcm.chunks(2).map(|b| i16::from_le_bytes([b[0], b[1]]).unsigned_abs()).max().unwrap();
        assert!(peak(&low) < 8000 && peak(&high) < 8000, "stays gentle");
        let crossings = |pcm: &[u8]| pcm.chunks(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect::<Vec<_>>().windows(2).filter(|w| (w[0] < 0) != (w[1] < 0)).count();
        assert!(crossings(&high) > crossings(&low));
    }
}
