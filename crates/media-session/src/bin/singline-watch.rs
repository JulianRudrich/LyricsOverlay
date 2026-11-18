//! `singline-watch` shows, live, what the music players on this computer
//! report: title, artist, album, position and which controls they offer. It
//! also measures how far each reported position drifts from the expected one.
//!
//! ```text
//! singline-watch                 # print every reading, once a second
//! singline-watch --json          # one JSON object per reading and line
//! singline-watch --once          # read once and exit
//! singline-watch --command playpause [--player ID]
//! singline-watch --command pause
//! singline-watch --command seek=42.5
//! ```

use std::collections::HashMap;
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

use media_session::{default_backends, Backend, Command, PlaybackStatus, PlayerId, Reading};

struct Options {
    json: bool,
    once: bool,
    interval: Duration,
    command: Option<Command>,
    player: Option<PlayerId>,
}

fn main() -> ExitCode {
    let options = match parse_args(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let mut backends = default_backends();
    if backends.is_empty() {
        eprintln!("No media backend exists for this operating system.");
        return ExitCode::FAILURE;
    }
    match options.command {
        Some(command) => send(&mut backends, command, options.player.as_ref()),
        None => {
            watch(&mut backends, &options);
            ExitCode::SUCCESS
        }
    }
}

const USAGE: &str = "Usage: singline-watch [--json] [--once] [--interval MS]\n       singline-watch --command playpause|pause|next|previous|seek=SECONDS [--player ID]";

fn parse_args(args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut options = Options {
        json: false,
        once: false,
        interval: Duration::from_secs(1),
        command: None,
        player: None,
    };
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => options.json = true,
            "--once" => options.once = true,
            "--interval" => {
                let value = args.next().ok_or("--interval needs milliseconds")?;
                let ms: u64 = value
                    .parse()
                    .map_err(|_| format!("invalid interval {value:?}"))?;
                options.interval = Duration::from_millis(ms.max(100));
            }
            "--command" => {
                let value = args.next().ok_or("--command needs a name")?;
                options.command = Some(match value.as_str() {
                    "playpause" => Command::PlayPause,
                    "pause" => Command::Pause,
                    "next" => Command::Next,
                    "previous" => Command::Previous,
                    other => match other.strip_prefix("seek=").map(str::parse::<f64>) {
                        Some(Ok(position)) => Command::SeekTo { position },
                        _ => return Err(format!("unknown command {other:?}")),
                    },
                });
            }
            "--player" => {
                options.player = Some(PlayerId::new(args.next().ok_or("--player needs an id")?))
            }
            "-h" | "--help" => {
                return Err("Shows what the music players on this computer report.".into())
            }
            other => return Err(format!("unknown option {other:?}")),
        }
    }
    Ok(options)
}

fn watch(backends: &mut [Box<dyn Backend>], options: &Options) {
    let started = Instant::now();
    let mut previous: HashMap<PlayerId, (String, f64, Instant)> = HashMap::new();
    if !options.json {
        println!("Watching media players. Press Ctrl+C to stop.\n");
    }
    loop {
        let elapsed = started.elapsed().as_secs_f64();
        for backend in backends.iter_mut() {
            let readings = backend.poll();
            if let Some(error) = backend.last_error() {
                if options.json {
                    println!(
                        "{}",
                        serde_json::json!({ "t": elapsed, "backend": backend.name(), "error": error })
                    );
                } else {
                    println!("t+{elapsed:7.1}s  [{}] {error}", backend.name());
                }
            }
            if readings.is_empty() && !options.json {
                println!("t+{elapsed:7.1}s  [{}] no players open", backend.name());
            }
            for reading in &readings {
                let drift = drift(&mut previous, reading);
                if options.json {
                    println!("{}", json(reading, elapsed, drift));
                } else {
                    println!("t+{elapsed:7.1}s  {}", text(reading, drift));
                }
            }
        }
        if options.once {
            return;
        }
        thread::sleep(options.interval);
    }
}

/// Difference between the reported position and the one expected from the
/// previous reading of the same track, in seconds.
fn drift(
    previous: &mut HashMap<PlayerId, (String, f64, Instant)>,
    reading: &Reading,
) -> Option<f64> {
    let Reading::Snapshot(snapshot) = reading else {
        return None;
    };
    let (Some(track), Some(position)) = (&snapshot.track, snapshot.position) else {
        previous.remove(&snapshot.player.id);
        return None;
    };
    let result = previous
        .get(&snapshot.player.id)
        .and_then(|(key, last, at)| {
            (key == &track.key && snapshot.status == PlaybackStatus::Playing).then(|| {
                let expected =
                    last + snapshot.sampled_at.duration_since(*at).as_secs_f64() * snapshot.rate;
                position - expected
            })
        });
    previous.insert(
        snapshot.player.id.clone(),
        (track.key.clone(), position, snapshot.sampled_at),
    );
    result
}

fn clock(seconds: f64) -> String {
    let total = seconds.max(0.0);
    format!("{}:{:04.1}", (total / 60.0).floor() as u64, total % 60.0)
}

fn text(reading: &Reading, drift: Option<f64>) -> String {
    match reading {
        Reading::Snapshot(snapshot) => {
            let player = &snapshot.player;
            let status = match snapshot.status {
                PlaybackStatus::Playing => "playing",
                PlaybackStatus::Paused => "paused",
                PlaybackStatus::Stopped => "stopped",
            };
            let mut line = format!(
                "{} ({}, {}) {status}",
                player.name,
                player.service.title(),
                player.id
            );
            match &snapshot.track {
                Some(track) => {
                    line.push_str(&format!(
                        "  \"{}\" — {}",
                        track.title,
                        if track.artist.is_empty() {
                            "(no artist)"
                        } else {
                            &track.artist
                        }
                    ));
                    if let Some(album) = &track.album {
                        line.push_str(&format!(" · {album}"));
                    }
                    let position = snapshot
                        .position
                        .map_or_else(|| "no position".to_owned(), clock);
                    let duration = track.duration.map_or_else(|| "?".to_owned(), clock);
                    line.push_str(&format!("  {position} / {duration}"));
                    if (snapshot.rate - 1.0).abs() > f64::EPSILON {
                        line.push_str(&format!("  rate {:.2}", snapshot.rate));
                    }
                    if let Some(drift) = drift {
                        line.push_str(&format!("  drift {drift:+.2}s"));
                    }
                    if let Some(id) = &track.source_id {
                        line.push_str(&format!("  id {id}"));
                    }
                }
                None => line.push_str("  (no track)"),
            }
            let caps = snapshot.capabilities;
            let controls: Vec<&str> = [
                (caps.play_pause, "play/pause"),
                (caps.previous, "previous"),
                (caps.next, "next"),
                (caps.seek, "seek"),
            ]
            .into_iter()
            .filter_map(|(enabled, name)| enabled.then_some(name))
            .collect();
            line.push_str(&format!(
                "  controls: {}",
                if controls.is_empty() {
                    "none".to_owned()
                } else {
                    controls.join(", ")
                }
            ));
            line
        }
        Reading::Unavailable {
            player,
            problem,
            playing,
        } => {
            format!(
                "{} ({}, {}) cannot be read: {problem:?}, playing: {playing:?}",
                player.name,
                player.service.title(),
                player.id
            )
        }
    }
}

fn json(reading: &Reading, elapsed: f64, drift: Option<f64>) -> serde_json::Value {
    match reading {
        Reading::Snapshot(snapshot) => serde_json::json!({
            "t": elapsed,
            "player": snapshot.player,
            "status": snapshot.status,
            "track": snapshot.track,
            "position": snapshot.position,
            "rate": snapshot.rate,
            "capabilities": snapshot.capabilities,
            "drift": drift,
        }),
        Reading::Unavailable {
            player,
            problem,
            playing,
        } => serde_json::json!({
            "t": elapsed,
            "player": player,
            "problem": problem,
            "playing": playing,
        }),
    }
}

fn send(
    backends: &mut [Box<dyn Backend>],
    command: Command,
    player: Option<&PlayerId>,
) -> ExitCode {
    for backend in backends.iter_mut() {
        let readings = backend.poll();
        let target = player.cloned().or_else(|| {
            readings.iter().find_map(|reading| match reading {
                Reading::Snapshot(snapshot) if snapshot.status == PlaybackStatus::Playing => {
                    Some(snapshot.player.id.clone())
                }
                _ => None,
            })
        });
        let Some(target) = target else { continue };
        if !readings.iter().any(|reading| reading.player().id == target) {
            continue;
        }
        return match backend.send(&target, command) {
            Ok(()) => {
                println!("Sent {command:?} to {target}.");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{target}: {error}");
                ExitCode::FAILURE
            }
        };
    }
    eprintln!(
        "No matching player. Start playback or pass --player with an id from the watch output."
    );
    ExitCode::FAILURE
}
