//! Prints what Whisper hears in a WAV file, word by word with start times.
//!
//! ```text
//! cargo run --release -p transcription --example transcribe -- ggml-base.bin song.wav [from] [to]
//! ```

use std::path::PathBuf;
use std::time::Instant;

use transcription::{capture, Transcriber, SAMPLE_RATE};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let usage = "usage: transcribe MODEL WAV [FROM_SECONDS] [TO_SECONDS]";
    let model = PathBuf::from(arguments.next().ok_or(usage)?);
    let wav = PathBuf::from(arguments.next().ok_or(usage)?);
    let from: f64 = arguments.next().map_or(Ok(0.0), |value| value.parse())?;
    let to: f64 = arguments
        .next()
        .map_or(Ok(f64::MAX), |value| value.parse())?;

    let samples = capture::read_wav(&wav)?;
    let rate = f64::from(SAMPLE_RATE);
    let start = ((from * rate) as usize).min(samples.len());
    let end = ((to.min(samples.len() as f64 / rate) * rate) as usize).max(start);
    let loading = Instant::now();
    let mut transcriber = Transcriber::load(&model)?;
    println!("model loaded in {:.2} s", loading.elapsed().as_secs_f64());
    let working = Instant::now();
    let transcript = transcriber.transcribe(&samples[start..end], None)?;
    println!(
        "{:.1} s of audio transcribed in {:.2} s, language {:?}",
        (end - start) as f64 / rate,
        working.elapsed().as_secs_f64(),
        transcript.language
    );
    println!("{}", transcript.text);
    for word in &transcript.words {
        println!("{:7.2}  {}", from + word.start, word.text);
    }
    Ok(())
}
