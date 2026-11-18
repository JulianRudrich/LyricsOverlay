//! Puts a pretend Spotify on the session bus, for trying the app without a real
//! player: `cargo run -p media-session --example fake_mpris_player -- "Title" "Artist" SECONDS`.
//!
//! `--bus chromium.instance1 --identity Chromium --url https://www.youtube.com/watch?v=1`
//! pretends to be a browser tab instead.

#[cfg(target_os = "linux")]
fn main() {
    use std::time::Duration;

    use media_session::fake_mpris::{FakePlayer, FakeTrack};

    let mut bus = "spotify".to_owned();
    let mut identity = "Spotify".to_owned();
    let mut url = None;
    let mut positional = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bus" => bus = args.next().unwrap_or(bus),
            "--identity" => identity = args.next().unwrap_or(identity),
            "--url" => url = args.next(),
            _ => positional.push(arg),
        }
    }
    let mut positional = positional.into_iter();
    let title = positional.next().unwrap_or_else(|| "Paper Lanterns".into());
    let artist = positional.next().unwrap_or_else(|| "Example Artist".into());
    let seconds: u64 = positional
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(180);
    let start: f64 = positional
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0.0);
    let track = FakeTrack {
        track_id: "/com/spotify/track/fake".into(),
        title,
        artists: vec![artist],
        album: if url.is_some() {
            String::new()
        } else {
            "Example Album".into()
        },
        length: Duration::from_secs(seconds),
    };
    let player =
        FakePlayer::spawn(None, &bus, &identity, track, true).expect("session bus is reachable");
    player.set_url(url);
    player.seek_to(Duration::from_secs_f64(start));
    println!("Pretend {identity} is playing. Press Ctrl+C to stop.");
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("The pretend MPRIS player only exists on Linux.");
}
