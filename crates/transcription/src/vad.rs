//! Voice activity detection: is someone singing right now?
//!
//! Deliberately simple. A frame counts as voice when it is clearly louder than
//! the background, which is estimated from the quieter frames of the last few
//! seconds, so a fan or a quiet room does not count, while singing over
//! music does. Voice has to last a few frames to start and lingers a moment
//! after it stops, so short clicks are ignored and breaths do not cut a line.

use std::collections::VecDeque;

use crate::SAMPLE_RATE;

/// Samples in one frame: 20 ms.
pub const FRAME: usize = SAMPLE_RATE as usize / 50;

/// Below this level (about −44 dBFS) nothing is voice.
const QUIETEST_VOICE: f32 = 0.006;
/// Voice is this much louder than the background (about 8 dB).
const ABOVE_BACKGROUND: f32 = 2.5;
/// The background estimate stays between these levels.
const BACKGROUND: (f32, f32) = (0.002, 0.02);
/// Frames of history for the background: 5 seconds.
const HISTORY: usize = 250;
/// Loud frames in a row before voice starts: 60 ms.
const ONSET: u32 = 3;
/// Frames voice lingers after the last loud one: 300 ms.
const HANGOVER: u32 = 15;

#[derive(Debug, Clone)]
pub struct Vad {
    history: VecDeque<f32>,
    loud_run: u32,
    linger: u32,
    voice: bool,
}

impl Default for Vad {
    fn default() -> Self {
        Self::new()
    }
}

impl Vad {
    pub fn new() -> Self {
        Self {
            history: VecDeque::with_capacity(HISTORY),
            loud_run: 0,
            linger: 0,
            voice: false,
        }
    }

    /// The background level the last few seconds suggest.
    pub fn background(&self) -> f32 {
        if self.history.is_empty() {
            return BACKGROUND.0;
        }
        let mut levels: Vec<f32> = self.history.iter().copied().collect();
        levels.sort_by(f32::total_cmp);
        levels[levels.len() / 10].clamp(BACKGROUND.0, BACKGROUND.1)
    }

    /// Takes one frame of 16 kHz samples; true while there is voice.
    pub fn push(&mut self, frame: &[f32]) -> bool {
        let level = rms(frame);
        let threshold = (self.background() * ABOVE_BACKGROUND).max(QUIETEST_VOICE);
        if self.history.len() == HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(level);

        if level > threshold {
            self.loud_run += 1;
            if self.loud_run >= ONSET {
                self.voice = true;
                self.linger = HANGOVER;
            }
        } else {
            self.loud_run = 0;
            if self.linger > 0 {
                self.linger -= 1;
            } else {
                self.voice = false;
            }
        }
        self.voice
    }

    pub fn is_voice(&self) -> bool {
        self.voice
    }
}

/// The root mean square of `samples`, from 0 to 1.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let energy: f32 = samples.iter().map(|sample| sample * sample).sum();
    (energy / samples.len() as f32).sqrt()
}

/// A level from 0 to 1 for a meter: −60 dBFS and below is 0, −10 dBFS is 1.
pub fn meter(rms: f32) -> f32 {
    if rms <= 0.0 {
        return 0.0;
    }
    let decibels = 20.0 * rms.log10();
    ((decibels + 60.0) / 50.0).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame of a 220 Hz tone with this root mean square.
    fn tone(level: f32, index: usize) -> Vec<f32> {
        let amplitude = level * std::f32::consts::SQRT_2;
        (0..FRAME)
            .map(|n| {
                let t = (index * FRAME + n) as f32 / SAMPLE_RATE as f32;
                amplitude * (2.0 * std::f32::consts::PI * 220.0 * t).sin()
            })
            .collect()
    }

    /// A frame of noise with about this root mean square.
    fn noise(level: f32, seed: &mut u32) -> Vec<f32> {
        (0..FRAME)
            .map(|_| {
                *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let uniform = (*seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
                uniform * level * 3f32.sqrt()
            })
            .collect()
    }

    fn run(vad: &mut Vad, frames: impl IntoIterator<Item = Vec<f32>>) -> Vec<bool> {
        frames.into_iter().map(|frame| vad.push(&frame)).collect()
    }

    #[test]
    fn silence_and_a_quiet_room_are_not_voice() {
        let mut vad = Vad::new();
        assert!(run(&mut vad, (0..100).map(|_| vec![0.0; FRAME]))
            .iter()
            .all(|voice| !voice));
        let mut seed = 7;
        assert!(run(&mut vad, (0..300).map(|_| noise(0.003, &mut seed)))
            .iter()
            .all(|voice| !voice));
    }

    #[test]
    fn singing_starts_after_a_few_frames_and_lingers_briefly() {
        let mut vad = Vad::new();
        let mut seed = 1;
        run(&mut vad, (0..50).map(|_| noise(0.002, &mut seed)));
        let sung = run(&mut vad, (0..25).map(|index| tone(0.05, index)));
        assert_eq!(&sung[..2], [false, false], "a click is not voice");
        assert!(sung[2..].iter().all(|voice| *voice));
        let after = run(&mut vad, (0..30).map(|_| noise(0.002, &mut seed)));
        assert!(
            after[..HANGOVER as usize].iter().all(|voice| *voice),
            "a breath keeps the line"
        );
        assert!(!after[HANGOVER as usize], "then it ends");
    }

    #[test]
    fn a_steady_hum_becomes_the_background() {
        let mut vad = Vad::new();
        let mut seed = 3;
        // A fan at about −28 dBFS for five seconds.
        let hum = run(&mut vad, (0..250).map(|_| noise(0.04, &mut seed)));
        assert!(
            !hum[200..].iter().any(|voice| *voice),
            "the hum is the background now"
        );
        // Singing clearly above it still counts.
        let mixed: Vec<Vec<f32>> = (0..20)
            .map(|index| {
                let hum = noise(0.04, &mut seed);
                tone(0.25, index)
                    .iter()
                    .zip(hum)
                    .map(|(a, b)| a + b)
                    .collect()
            })
            .collect();
        assert!(run(&mut vad, mixed)
            .iter()
            .skip(ONSET as usize)
            .all(|voice| *voice));
    }

    #[test]
    fn the_meter_spans_quiet_to_loud() {
        assert_eq!(meter(0.0), 0.0);
        assert_eq!(meter(0.0005), 0.0);
        assert!((meter(0.01) - 0.4).abs() < 1e-3);
        assert_eq!(meter(1.0), 1.0);
        assert!((rms(&[0.5, -0.5]) - 0.5).abs() < 1e-6);
        assert_eq!(rms(&[]), 0.0);
    }
}
