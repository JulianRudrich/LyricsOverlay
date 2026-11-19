//! Audio as Whisper wants it, 16 kHz mono, and the recording Sing to find
//! keeps: the last minute of samples with voice detection and a level for the
//! meter. Times are seconds on the audio clock, which starts at the first sample.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::vad::{self, Vad, FRAME};
use crate::SAMPLE_RATE;

/// Mixes interleaved frames down to mono and brings them to 16 kHz. Lowering
/// the rate averages the samples that make up each output sample, which keeps
/// most of the aliasing out of the speech band.
#[derive(Debug, Clone)]
pub struct Resampler {
    channels: usize,
    /// Input samples per output sample.
    step: f64,
    phase: f64,
    sum: f32,
    count: u32,
    last: f32,
}

impl Resampler {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            channels: usize::from(channels.max(1)),
            step: f64::from(sample_rate.max(1)) / f64::from(SAMPLE_RATE),
            phase: 0.0,
            sum: 0.0,
            count: 0,
            last: 0.0,
        }
    }

    /// Appends the 16 kHz mono version of `interleaved` to `out`.
    pub fn push(&mut self, interleaved: &[f32], out: &mut Vec<f32>) {
        for frame in interleaved.chunks(self.channels) {
            let mono = frame.iter().sum::<f32>() / frame.len() as f32;
            self.sum += mono;
            self.count += 1;
            self.phase += 1.0;
            while self.phase >= self.step {
                if self.count > 0 {
                    self.last = self.sum / self.count as f32;
                    self.sum = 0.0;
                    self.count = 0;
                }
                out.push(self.last);
                self.phase -= self.step;
            }
        }
    }
}

/// The last minute or so of what the microphone heard.
#[derive(Debug, Clone)]
pub struct Recording {
    keep: usize,
    samples: VecDeque<f32>,
    /// Index on the audio clock of `samples[0]`.
    first: u64,
    received: u64,
    partial: Vec<f32>,
    /// Whether each 20 ms frame since `first_frame` had voice.
    frames: VecDeque<bool>,
    first_frame: u64,
    vad: Vad,
    level: f32,
    peak: f32,
    started: Option<Instant>,
    last_voice: Option<f64>,
}

impl Recording {
    /// Keeps the last `keep` of audio.
    pub fn new(keep: Duration) -> Self {
        Self {
            keep: (keep.as_secs_f64() * f64::from(SAMPLE_RATE)) as usize,
            samples: VecDeque::new(),
            first: 0,
            received: 0,
            partial: Vec::with_capacity(FRAME),
            frames: VecDeque::new(),
            first_frame: 0,
            vad: Vad::new(),
            level: 0.0,
            peak: 0.0,
            started: None,
            last_voice: None,
        }
    }

    /// Takes 16 kHz mono samples that arrived at `now`.
    pub fn push(&mut self, samples: &[f32], now: Instant) {
        if self.started.is_none() {
            let length = Duration::from_secs_f64(samples.len() as f64 / f64::from(SAMPLE_RATE));
            self.started = Some(now.checked_sub(length).unwrap_or(now));
        }
        for &sample in samples {
            let sample = if sample.is_finite() {
                sample.clamp(-1.0, 1.0)
            } else {
                0.0
            };
            self.peak = self.peak.max(sample.abs());
            self.samples.push_back(sample);
            self.partial.push(sample);
            self.received += 1;
            if self.partial.len() == FRAME {
                self.frame();
            }
        }
        while self.samples.len() > self.keep {
            self.samples.pop_front();
            self.first += 1;
        }
        let frames_kept = self.keep / FRAME;
        while self.frames.len() > frames_kept {
            self.frames.pop_front();
            self.first_frame += 1;
        }
    }

    fn frame(&mut self) {
        let voice = self.vad.push(&self.partial);
        let level = vad::meter(vad::rms(&self.partial));
        // Rises at once and falls slowly, like a meter.
        self.level = level.max(self.level * 0.8);
        self.frames.push_back(voice);
        if voice {
            self.last_voice = Some(self.duration());
        }
        self.partial.clear();
    }

    /// Seconds of audio received so far: the audio clock now.
    pub fn duration(&self) -> f64 {
        self.received as f64 / f64::from(SAMPLE_RATE)
    }

    /// When the first sample was recorded; audio time `t` happened at this plus `t`.
    pub fn started(&self) -> Option<Instant> {
        self.started
    }

    /// The meter level, from 0 to 1.
    pub fn level(&self) -> f32 {
        self.level
    }

    /// The loudest sample so far. Exactly 0 means the system sends silence,
    /// which is what macOS does without permission to use the microphone.
    pub fn peak(&self) -> f32 {
        self.peak
    }

    pub fn is_voice(&self) -> bool {
        self.vad.is_voice()
    }

    /// The end of the last frame with voice.
    pub fn last_voice(&self) -> Option<f64> {
        self.last_voice
    }

    fn frame_seconds() -> f64 {
        FRAME as f64 / f64::from(SAMPLE_RATE)
    }

    /// Seconds with voice between `from` and `to`.
    pub fn voiced(&self, from: f64, to: f64) -> f64 {
        let seconds = Self::frame_seconds();
        self.frames
            .iter()
            .enumerate()
            .filter(|(index, voice)| {
                let start = (self.first_frame + *index as u64) as f64 * seconds;
                **voice && start >= from && start + seconds <= to + 1e-9
            })
            .count() as f64
            * seconds
    }

    /// The start of the first frame with voice at or after `from`.
    pub fn first_voice_after(&self, from: f64) -> Option<f64> {
        let seconds = Self::frame_seconds();
        self.frames
            .iter()
            .enumerate()
            .map(|(index, voice)| ((self.first_frame + index as u64) as f64 * seconds, *voice))
            .find(|(start, voice)| *voice && *start >= from)
            .map(|(start, _)| start)
    }

    /// Where voice starts after silence, between `from` and `to`: the starts
    /// of phrases, where a stretch to transcribe should begin.
    pub fn onsets(&self, from: f64, to: f64) -> Vec<f64> {
        let seconds = Self::frame_seconds();
        let mut previous = false;
        let mut found = Vec::new();
        for (index, voice) in self.frames.iter().enumerate() {
            let start = (self.first_frame + index as u64) as f64 * seconds;
            if *voice && !previous && start >= from && start <= to {
                found.push(start);
            }
            previous = *voice;
        }
        found
    }

    /// The samples between `from` and `to`, as far as they are kept, with the
    /// audio time of the first one.
    pub fn samples(&self, from: f64, to: f64) -> (f64, Vec<f32>) {
        let rate = f64::from(SAMPLE_RATE);
        let start = ((from.max(0.0) * rate) as u64).clamp(self.first, self.received);
        let end = ((to.max(0.0) * rate) as u64).clamp(start, self.received);
        let skip = (start - self.first) as usize;
        let take = (end - start) as usize;
        let samples = self.samples.iter().skip(skip).take(take).copied().collect();
        (start as f64 / rate, samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(seconds: f64, level: f32) -> Vec<f32> {
        let count = (seconds * f64::from(SAMPLE_RATE)) as usize;
        (0..count)
            .map(|n| {
                let t = n as f32 / SAMPLE_RATE as f32;
                level * std::f32::consts::SQRT_2 * (2.0 * std::f32::consts::PI * 300.0 * t).sin()
            })
            .collect()
    }

    #[test]
    fn resamples_and_mixes_down() {
        let mut stereo = Resampler::new(48_000, 2);
        let mut out = Vec::new();
        // One second of 48 kHz stereo: left 0.5, right -0.1.
        let input: Vec<f32> = (0..48_000).flat_map(|_| [0.5, -0.1]).collect();
        stereo.push(&input, &mut out);
        assert_eq!(out.len(), 16_000);
        assert!(out.iter().all(|sample| (sample - 0.2).abs() < 1e-6));

        // Odd rates and chunk sizes add up to the same length.
        let mut odd = Resampler::new(22_050, 1);
        let mut total = Vec::new();
        for chunk in vec![0.25_f32; 22_050].chunks(333) {
            odd.push(chunk, &mut total);
        }
        assert!((15_999..=16_001).contains(&total.len()), "{}", total.len());

        let mut up = Resampler::new(8_000, 1);
        let mut doubled = Vec::new();
        up.push(&[0.1, 0.2], &mut doubled);
        assert_eq!(doubled, [0.1, 0.1, 0.2, 0.2]);
    }

    #[test]
    fn keeps_the_last_minute_with_voice_and_level() {
        let mut recording = Recording::new(Duration::from_secs(2));
        let now = Instant::now();
        recording.push(&vec![0.0; 16_000], now);
        assert_eq!(recording.peak(), 0.0);
        assert!(recording.last_voice().is_none() && !recording.is_voice());
        recording.push(&tone(1.0, 0.1), now);
        recording.push(&vec![0.0; 8_000], now);
        assert_eq!(recording.duration(), 2.5);
        assert!(recording.peak() > 0.1);
        let voiced = recording.voiced(0.0, 2.5);
        // 0.96 s of the tone after its onset, and 0.3 s lingering after it.
        assert!((1.2..=1.3).contains(&voiced), "{voiced}");
        let onset = recording.first_voice_after(0.0).expect("voice");
        assert!((1.0..1.1).contains(&onset), "{onset}");
        assert_eq!(recording.onsets(0.0, 2.5), [onset]);
        assert!(
            recording.onsets(1.5, 2.5).is_empty(),
            "voice that goes on is no onset"
        );
        let end = recording.last_voice().expect("voice");
        assert!(
            (2.0..2.4).contains(&end),
            "the voice lingers briefly: {end}"
        );
        assert!(recording.level() > 0.0);

        // Only the last two seconds are kept.
        let (start, kept) = recording.samples(0.0, 2.5);
        assert_eq!((start, kept.len()), (0.5, 32_000));
        let (start, part) = recording.samples(1.0, 1.25);
        assert_eq!((start, part.len()), (1.0, 4_000));
        assert!(part.iter().any(|sample| sample.abs() > 0.05));
        assert!(recording.samples(3.0, 4.0).1.is_empty());
        assert!(recording.started().is_some_and(|started| started <= now));
    }
}
