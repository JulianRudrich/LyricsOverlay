//! Puts a pretend Spotify on the session bus, for trying the app without a real
//! player: `cargo run -p media-session --example fake_mpris_player -- "Title" "Artist" SECONDS`.

#[cfg(target_os = "linux")]
fn main() {
    use std::time::Duration;

    use media_session::fake_mpris::{FakePlayer, FakeTrack};

    let mut args = std::env::args().skip(1);
    let title = args.next().unwrap_or_else(|| "Paper Lanterns".into());
    let artist = args.next().unwrap_or_else(|| "Example Artist".into());
    let seconds: u64 = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(180);
    let start: f64 = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0.0);
    let track = FakeTrack {
        track_id: "/com/spotify/track/fake".into(),
        title,
        artists: vec![artist],
        album: "Example Album".into(),
        length: Duration::from_secs(seconds),
    };
    let player = FakePlayer::spawn(None, "spotify", "Spotify", track, true)
        .expect("session bus is reachable");
    player.seek_to(Duration::from_secs_f64(start));
    println!("Pretend Spotify is playing. Press Ctrl+C to stop.");
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("The pretend MPRIS player only exists on Linux.");
}
