//! Sing to find: hearing what someone sings and turning it into text on this
//! device.
//!
//! | Module      | Does                                                        |
//! |-------------|-------------------------------------------------------------|
//! | [`capture`] | Records the microphone, or plays a WAV file instead          |
//! | [`audio`]   | Brings audio to 16 kHz mono and keeps the last minute of it  |
//! | [`vad`]     | Tells voice from silence and background noise                |
//! | [`model`]   | Downloads and checks the Whisper model                       |
//! | `whisper`   | Transcribes with whisper.cpp (feature `whisper`)             |
//!
//! The audio never leaves the device; only the words Whisper heard do, when
//! the app searches for the song.

pub mod audio;
pub mod capture;
pub mod model;
pub mod vad;
#[cfg(feature = "whisper")]
mod whisper;

use thiserror::Error;

#[cfg(feature = "whisper")]
pub use whisper::Transcriber;

/// Whisper's sample rate; every sample in this crate is 16 kHz mono.
pub const SAMPLE_RATE: u32 = 16_000;

/// True when this build can transcribe, that is with the `whisper` feature.
pub const fn can_transcribe() -> bool {
    cfg!(feature = "whisper")
}

/// A word Whisper heard; `start` is seconds from the start of the audio it got.
#[derive(Debug, Clone, PartialEq)]
pub struct TimedWord {
    pub text: String,
    pub start: f64,
    /// The first word of a segment. Whisper often dates it to the end of the
    /// silence before it rather than to the voice, so it is worth checking
    /// against voice detection.
    pub opens_segment: bool,
}

/// What Whisper heard in a stretch of audio.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Transcript {
    pub text: String,
    pub words: Vec<TimedWord>,
    /// The detected language, such as "en".
    pub language: Option<String>,
}

impl Transcript {
    /// True for the phrases Whisper tends to invent for silence or music,
    /// such as "Thank you for watching".
    pub fn is_hallucination(&self) -> bool {
        is_hallucination(&self.text)
    }
}

/// See [`Transcript::is_hallucination`].
pub fn is_hallucination(text: &str) -> bool {
    const PHRASES: [&str; 9] = [
        "thank you for watching",
        "thanks for watching",
        "please subscribe",
        "subscribe to",
        "amara org",
        "untertitel",
        "vielen dank fur",
        "sous titres",
        "transcribed by",
    ];
    // Whisper writes sounds in brackets, such as "[Music]".
    let mut depth = 0usize;
    let spoken: String = text
        .chars()
        .filter(|c| match c {
            '[' | '(' => {
                depth += 1;
                false
            }
            ']' | ')' => {
                depth = depth.saturating_sub(1);
                false
            }
            _ => depth == 0,
        })
        .collect();
    let folded: String = spoken
        .to_lowercase()
        .replace(['ü', 'Ü'], "u")
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let folded = folded.split_whitespace().collect::<Vec<_>>().join(" ");
    folded.is_empty() || PHRASES.iter().any(|phrase| folded.contains(phrase))
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TranscribeError {
    #[error("the speech model could not be loaded: {0}")]
    Model(String),
    #[error("the transcription failed: {0}")]
    Failed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_invented_phrases() {
        assert!(is_hallucination("Thank you for watching!"));
        assert!(is_hallucination(" [Music] ♪ "), "nothing but noise marks");
        assert!(is_hallucination("Untertitel im Auftrag des ZDF"));
        assert!(is_hallucination("Vielen Dank für's Zuschauen"));
        assert!(!is_hallucination("I can hear the morning calling"));
        let heard = Transcript {
            text: "Paper boats are sailing".into(),
            ..Transcript::default()
        };
        assert!(!heard.is_hallucination());
    }
}
