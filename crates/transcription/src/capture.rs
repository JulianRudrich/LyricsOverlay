//! Where the audio comes from: the default microphone, or a WAV file played in
//! real time instead, for tests (`SINGLINE_MIC_FILE` in the app). Either way
//! the samples arrive as 16 kHz mono on a thread of their own, and the audio
//! stays in memory on this device.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::audio::Resampler;
use crate::SAMPLE_RATE;

/// What to record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The system's default input device.
    Microphone,
    /// A WAV file, played once in real time and followed by silence.
    File(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CaptureError {
    #[error("no microphone was found")]
    NoDevice,
    #[error("the system does not allow Singline to use the microphone")]
    Denied,
    #[error("another app is using the microphone")]
    Busy,
    #[error("this build of Singline cannot record")]
    Unsupported,
    #[error("the audio file could not be read: {0}")]
    File(String),
    #[error("the microphone could not be opened: {0}")]
    Failed(String),
}

/// Receives 16 kHz mono samples as they arrive.
pub type Sink = Box<dyn FnMut(&[f32]) + Send>;

/// A running recording; dropping it stops the recording.
pub struct Capture {
    stop: Option<SyncSender<()>>,
    thread: Option<JoinHandle<()>>,
    failure: Arc<Mutex<Option<CaptureError>>>,
}

impl Capture {
    /// Starts recording from `source`. Returns once the device or file is
    /// open, with the reason when it cannot be.
    pub fn start(source: Source, sink: Sink) -> Result<Self, CaptureError> {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (stop_tx, stop_rx) = mpsc::sync_channel(1);
        let failure = Arc::new(Mutex::new(None));
        let shared = Arc::clone(&failure);
        let thread = thread::Builder::new()
            .name("sing-capture".into())
            .spawn(move || match source {
                Source::Microphone => record(ready_tx, stop_rx, sink, shared),
                Source::File(path) => play(&path, ready_tx, stop_rx, sink),
            })
            .map_err(|error| CaptureError::Failed(error.to_string()))?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                stop: Some(stop_tx),
                thread: Some(thread),
                failure,
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => {
                let _ = thread.join();
                Err(CaptureError::Failed("the recording stopped".into()))
            }
        }
    }

    /// Why the recording stopped by itself, such as an unplugged microphone.
    pub fn failure(&self) -> Option<CaptureError> {
        self.failure
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.try_send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Plays `path` in real time, then silence until stopped.
fn play(
    path: &Path,
    ready: SyncSender<Result<(), CaptureError>>,
    stop: Receiver<()>,
    mut sink: Sink,
) {
    let samples = match read_wav(path) {
        Ok(samples) => samples,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    let _ = ready.send(Ok(()));
    let chunk = SAMPLE_RATE as usize / 50;
    let silence = vec![0.0; chunk];
    let started = Instant::now();
    let mut sent = 0usize;
    loop {
        let piece = samples.get(sent..(sent + chunk).min(samples.len()));
        let piece = piece.filter(|piece| !piece.is_empty()).unwrap_or(&silence);
        sink(piece);
        sent += piece.len();
        let due = started + Duration::from_secs_f64(sent as f64 / f64::from(SAMPLE_RATE));
        match stop.recv_timeout(due.saturating_duration_since(Instant::now())) {
            Err(RecvTimeoutError::Timeout) => {}
            _ => return,
        }
    }
}

/// A WAV file as 16 kHz mono samples.
pub fn read_wav(path: &Path) -> Result<Vec<f32>, CaptureError> {
    let unreadable = |error: hound::Error| CaptureError::File(error.to_string());
    let mut reader = hound::WavReader::open(path).map_err(unreadable)?;
    let spec = reader.spec();
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(unreadable)?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample.clamp(1, 32) - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|value| value as f32 * scale))
                .collect::<Result<_, _>>()
                .map_err(unreadable)?
        }
    };
    let mut resampler = Resampler::new(spec.sample_rate, spec.channels);
    let mut samples = Vec::with_capacity(interleaved.len());
    resampler.push(&interleaved, &mut samples);
    Ok(samples)
}

#[cfg(feature = "microphone")]
fn record(
    ready: SyncSender<Result<(), CaptureError>>,
    stop: Receiver<()>,
    mut sink: Sink,
    failure: Arc<Mutex<Option<CaptureError>>>,
) {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::SampleFormat;

    let host = cpal::default_host();
    let Some(device) = host.default_input_device() else {
        let _ = ready.send(Err(CaptureError::NoDevice));
        return;
    };
    let supported = match device.default_input_config() {
        Ok(supported) => supported,
        Err(error) => {
            let _ = ready.send(Err(device_error(&error)));
            return;
        }
    };
    let config = supported.config();
    let (audio_tx, audio_rx) = mpsc::sync_channel::<Vec<f32>>(256);
    let errors = Arc::clone(&failure);
    let stream = match supported.sample_format() {
        SampleFormat::F32 => stream::<f32>(&device, &config, audio_tx, errors),
        SampleFormat::F64 => stream::<f64>(&device, &config, audio_tx, errors),
        SampleFormat::I8 => stream::<i8>(&device, &config, audio_tx, errors),
        SampleFormat::I16 => stream::<i16>(&device, &config, audio_tx, errors),
        SampleFormat::I32 => stream::<i32>(&device, &config, audio_tx, errors),
        SampleFormat::U8 => stream::<u8>(&device, &config, audio_tx, errors),
        SampleFormat::U16 => stream::<u16>(&device, &config, audio_tx, errors),
        SampleFormat::U32 => stream::<u32>(&device, &config, audio_tx, errors),
        other => Err(CaptureError::Failed(format!(
            "the microphone sends {other} samples"
        ))),
    };
    let stream = match stream {
        Ok(stream) => stream,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    if let Err(error) = stream.play() {
        let _ = ready.send(Err(device_error(&error)));
        return;
    }
    let _ = ready.send(Ok(()));
    let mut resampler = Resampler::new(config.sample_rate, config.channels);
    let mut out = Vec::new();
    while let Err(mpsc::TryRecvError::Empty) = stop.try_recv() {
        match audio_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(chunk) => {
                out.clear();
                resampler.push(&chunk, &mut out);
                sink(&out);
            }
            Err(RecvTimeoutError::Timeout) => {
                if failure.lock().unwrap_or_else(|p| p.into_inner()).is_some() {
                    break;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    drop(stream);
}

#[cfg(feature = "microphone")]
fn stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    audio: SyncSender<Vec<f32>>,
    failure: Arc<Mutex<Option<CaptureError>>>,
) -> Result<cpal::Stream, CaptureError>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    use cpal::traits::DeviceTrait;

    device
        .build_input_stream::<T, _, _>(
            *config,
            move |data: &[T], _| {
                let samples = data
                    .iter()
                    .map(|sample| sample.to_sample::<f32>())
                    .collect();
                // A busy reader loses a chunk rather than blocking the audio thread.
                let _ = audio.try_send(samples);
            },
            move |error| {
                use cpal::ErrorKind;
                // A rerouted stream or a dropout goes on by itself.
                if !matches!(error.kind(), ErrorKind::DeviceChanged | ErrorKind::Xrun) {
                    *failure.lock().unwrap_or_else(|p| p.into_inner()) = Some(device_error(&error));
                }
            },
            None,
        )
        .map_err(|error| device_error(&error))
}

#[cfg(feature = "microphone")]
fn device_error(error: &cpal::Error) -> CaptureError {
    use cpal::ErrorKind;
    match error.kind() {
        ErrorKind::PermissionDenied => CaptureError::Denied,
        ErrorKind::DeviceNotAvailable | ErrorKind::HostUnavailable => CaptureError::NoDevice,
        ErrorKind::DeviceBusy => CaptureError::Busy,
        _ => CaptureError::Failed(error.to_string()),
    }
}

#[cfg(not(feature = "microphone"))]
fn record(
    ready: SyncSender<Result<(), CaptureError>>,
    _stop: Receiver<()>,
    _sink: Sink,
    _failure: Arc<Mutex<Option<CaptureError>>>,
) {
    let _ = ready.send(Err(CaptureError::Unsupported));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_wav(path: &Path, rate: u32, channels: u16, frames: usize) {
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).expect("created");
        for _ in 0..frames * usize::from(channels) {
            writer.write_sample(i16::MAX / 2).expect("written");
        }
        writer.finalize().expect("finished");
    }

    #[test]
    fn reads_a_wav_file_as_16_khz_mono() {
        let path =
            std::env::temp_dir().join(format!("singline-capture-{}.wav", std::process::id()));
        write_wav(&path, 22_050, 2, 22_050);
        let samples = read_wav(&path).expect("readable");
        assert!((15_999..=16_001).contains(&samples.len()));
        assert!(samples.iter().all(|sample| (sample - 0.5).abs() < 0.01));
        assert!(matches!(
            read_wav(Path::new("/no/such/file.wav")),
            Err(CaptureError::File(_))
        ));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_file_plays_in_real_time_then_falls_silent() {
        let path = std::env::temp_dir().join(format!("singline-play-{}.wav", std::process::id()));
        write_wav(&path, 16_000, 1, 1_600);
        let (tx, rx) = mpsc::channel::<Vec<f32>>();
        let started = Instant::now();
        let capture = Capture::start(
            Source::File(path.clone()),
            Box::new(move |samples| {
                let _ = tx.send(samples.to_vec());
            }),
        )
        .expect("starts");
        let mut received = Vec::new();
        while received.len() < 4_800 {
            received.extend(
                rx.recv_timeout(Duration::from_secs(2))
                    .expect("samples arrive"),
            );
        }
        let elapsed = started.elapsed().as_secs_f64();
        assert!(elapsed >= 0.25, "paced like a microphone: {elapsed}");
        assert!(received[..1_600].iter().all(|sample| *sample > 0.4));
        assert!(received[1_600..4_800].iter().all(|sample| *sample == 0.0));
        assert!(capture.failure().is_none());
        drop(capture);
        let _ = std::fs::remove_file(path);
    }
}
