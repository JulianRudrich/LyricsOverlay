//! Transcription on this device with whisper.cpp. Slow work: call it from a
//! thread of its own, never from the interface.

use std::path::Path;
use std::sync::Once;

use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

use crate::{TimedWord, TranscribeError, Transcript, SAMPLE_RATE};

/// Segments Whisper itself thinks hold no speech are dropped.
const NO_SPEECH: f32 = 0.7;

/// A loaded Whisper model. Loading takes a moment, so keep it while listening.
pub struct Transcriber {
    context: WhisperContext,
    state: WhisperState,
    threads: i32,
}

impl Transcriber {
    pub fn load(path: &Path) -> Result<Self, TranscribeError> {
        static QUIET: Once = Once::new();
        // Keeps whisper.cpp from printing to the terminal.
        QUIET.call_once(whisper_rs::install_logging_hooks);
        let parameters = WhisperContextParameters {
            use_gpu: false,
            ..WhisperContextParameters::default()
        };
        let context = WhisperContext::new_with_params(path, parameters)
            .map_err(|error| TranscribeError::Model(error.to_string()))?;
        let state = context
            .create_state()
            .map_err(|error| TranscribeError::Model(error.to_string()))?;
        let threads = std::thread::available_parallelism()
            .map_or(4, |count| count.get())
            .clamp(1, 8) as i32;
        Ok(Self {
            context,
            state,
            threads,
        })
    }

    /// The words in 16 kHz mono `samples`, with times in seconds from their
    /// start. `language` is a code such as "en", or `None` to detect it.
    pub fn transcribe(
        &mut self,
        samples: &[f32],
        language: Option<&str>,
    ) -> Result<Transcript, TranscribeError> {
        // Whisper skips anything shorter than a second.
        let minimum = SAMPLE_RATE as usize * 11 / 10;
        let mut audio = samples.to_vec();
        if audio.len() < minimum {
            audio.resize(minimum, 0.0);
        }
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(self.threads);
        params.set_language(Some(language.unwrap_or("auto")));
        params.set_translate(false);
        params.set_no_context(true);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_token_timestamps(true);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        params.set_no_speech_thold(0.6);
        // The encoder looks at 30 s by default; a context sized to the audio
        // is several times faster, and too small a one makes Whisper ramble.
        let seconds = audio.len() as f64 / f64::from(SAMPLE_RATE);
        params.set_audio_ctx(((seconds + 1.0) * 50.0).ceil().clamp(512.0, 1500.0) as i32);
        self.state
            .full(params, &audio)
            .map_err(|error| TranscribeError::Failed(error.to_string()))?;

        let end_of_text = self.context.token_eot();
        let mut text = Vec::new();
        let mut words = Vec::new();
        for segment in self.state.as_iter() {
            if segment.no_speech_probability() > NO_SPEECH {
                continue;
            }
            let start = segment.start_timestamp() as f64 / 100.0;
            let end = segment.end_timestamp() as f64 / 100.0;
            let mut pieces: Vec<(Vec<u8>, f64)> = Vec::new();
            for index in 0..segment.n_tokens() {
                let Some(token) = segment.get_token(index) else {
                    continue;
                };
                if token.token_id() >= end_of_text {
                    continue;
                }
                let Ok(bytes) = token.to_bytes() else {
                    continue;
                };
                let time = token.token_data().t0 as f64 / 100.0;
                match pieces.last_mut() {
                    Some((word, _)) if bytes.first() != Some(&b' ') => {
                        word.extend_from_slice(bytes)
                    }
                    _ => pieces.push((bytes.trim_ascii_start().to_vec(), time)),
                }
            }
            let found: Vec<(String, f64)> = pieces
                .into_iter()
                .map(|(bytes, time)| (String::from_utf8_lossy(&bytes).trim().to_owned(), time))
                .filter(|(word, _)| word.chars().any(char::is_alphanumeric))
                .collect();
            let mut segment_words = timed(found, start, end);
            if let Some(first) = segment_words.first_mut() {
                first.opens_segment = true;
            }
            words.extend(segment_words);
            if let Ok(segment_text) = segment.to_str_lossy() {
                text.push(segment_text.trim().to_owned());
            }
        }
        let language =
            whisper_rs::get_lang_str(self.state.full_lang_id_from_state()).map(str::to_owned);
        Ok(Transcript {
            text: text.join(" ").trim().to_owned(),
            words,
            language,
        })
    }
}

/// Gives every word a start time: the token's own when it lies inside the
/// segment and keeps the order, otherwise spread over the segment by length.
fn timed(words: Vec<(String, f64)>, start: f64, end: f64) -> Vec<TimedWord> {
    let plausible = words
        .iter()
        .all(|(_, time)| *time >= start - 0.5 && *time <= end + 0.5)
        && words.windows(2).all(|pair| pair[0].1 <= pair[1].1);
    let letters: usize = words.iter().map(|(word, _)| word.chars().count()).sum();
    let mut before = 0;
    words
        .into_iter()
        .map(|(text, time)| {
            let estimate = start + (end - start).max(0.0) * before as f64 / letters.max(1) as f64;
            before += text.chars().count();
            TimedWord {
                text,
                start: if plausible { time.max(0.0) } else { estimate },
                opens_segment: false,
            }
        })
        .collect()
}
