//! Reads pretend players over a private D-Bus daemon, so the MPRIS backend is
//! tested end to end without touching the desktop session.
#![cfg(target_os = "linux")]

use std::io::{BufRead, BufReader};
use std::process::{Child, Command as Process, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use media_session::fake_mpris::{FakePlayer, FakeTrack};
use media_session::mpris::MprisBackend;
use media_session::{Backend, Command, PlaybackStatus, PlayerId, Reading, Service};

/// A `dbus-daemon` that lives as long as this value.
struct PrivateBus {
    child: Child,
    address: String,
}

impl PrivateBus {
    fn start() -> Option<Self> {
        let mut child = Process::new("dbus-daemon")
            .args(["--session", "--nofork", "--nopidfile", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let mut line = String::new();
        BufReader::new(child.stdout.take()?)
            .read_line(&mut line)
            .ok()?;
        let address = line.trim().to_owned();
        (!address.is_empty()).then_some(Self { child, address })
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn track(title: &str, seconds: u64) -> FakeTrack {
    FakeTrack {
        track_id: "/com/spotify/track/fake123".into(),
        title: title.into(),
        artists: vec!["Example Artist".into(), "Guest".into()],
        album: "Example Album".into(),
        length: Duration::from_secs(seconds),
    }
}

fn snapshot_of(readings: &[Reading], id: &str) -> media_session::PlayerSnapshot {
    readings
        .iter()
        .find_map(|reading| match reading {
            Reading::Snapshot(snapshot) if snapshot.player.id.as_str() == id => {
                Some(snapshot.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no snapshot for {id} in {readings:?}"))
}

fn eventually(mut check: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if check() {
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn reads_and_controls_players_over_dbus() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("dbus-daemon is not available; skipping");
        return;
    };
    let spotify = FakePlayer::spawn(
        Some(&bus.address),
        "spotify",
        "Spotify",
        track("Paper Lanterns", 200),
        true,
    )
    .expect("fake Spotify registers");
    let _vlc = FakePlayer::spawn(
        Some(&bus.address),
        "vlc",
        "VLC media player",
        track("Home Video", 60),
        false,
    )
    .expect("fake VLC registers");
    spotify.seek_to(Duration::from_secs(30));

    let mut backend = MprisBackend::with_address(&bus.address);
    let readings = backend.poll();
    assert_eq!(backend.last_error(), None);
    assert_eq!(readings.len(), 2, "both players are found: {readings:?}");

    let first = snapshot_of(&readings, "mpris:org.mpris.MediaPlayer2.spotify");
    assert_eq!(first.player.service, Service::Spotify);
    assert_eq!(first.player.name, "Spotify");
    assert_eq!(first.status, PlaybackStatus::Playing);
    let song = first.track.as_ref().expect("track metadata");
    assert_eq!(song.title, "Paper Lanterns");
    assert_eq!(song.artist, "Example Artist, Guest");
    assert_eq!(song.album.as_deref(), Some("Example Album"));
    assert_eq!(song.duration, Some(200.0));
    assert_eq!(song.source_id.as_deref(), Some("spotify:track:fake123"));
    let position = first.position.expect("position");
    assert!((30.0..31.0).contains(&position), "position {position}");
    assert!(first.capabilities.seek && first.capabilities.play_pause && first.capabilities.next);

    let vlc = snapshot_of(&readings, "mpris:org.mpris.MediaPlayer2.vlc");
    assert_eq!(
        (vlc.player.service, vlc.status),
        (Service::Other, PlaybackStatus::Paused)
    );

    let id = PlayerId::new("mpris:org.mpris.MediaPlayer2.spotify");
    backend
        .send(&id, Command::PlayPause)
        .expect("pause is accepted");
    assert!(eventually(|| !spotify.is_playing()), "the player paused");
    let paused = snapshot_of(&backend.poll(), id.as_str());
    assert_eq!(paused.status, PlaybackStatus::Paused);

    backend
        .send(&id, Command::SeekTo { position: 120.0 })
        .expect("seek is accepted");
    assert!(
        eventually(|| (119.5..120.5).contains(&spotify.position().as_secs_f64())),
        "seeked to {:?}",
        spotify.position()
    );
    assert!(
        spotify.commands().contains(&"SetPosition".to_owned()),
        "uses the track id for an exact seek"
    );
    backend.send(&id, Command::Next).expect("next is accepted");
    assert!(eventually(|| spotify
        .commands()
        .contains(&"Next".to_owned())));

    drop(spotify);
    assert!(
        eventually(|| backend.poll().iter().all(|r| r.player().id != id)),
        "a closed player disappears"
    );
    assert_eq!(
        backend.send(&id, Command::PlayPause),
        Err(media_session::CommandError::PlayerGone),
        "commands to a closed player say so"
    );
}

#[test]
fn pause_only_ever_pauses() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("dbus-daemon is not available; skipping");
        return;
    };
    let spotify = FakePlayer::spawn(
        Some(&bus.address),
        "spotify",
        "Spotify",
        track("Lullaby", 180),
        true,
    )
    .expect("fake Spotify registers");
    let id = PlayerId::new("mpris:org.mpris.MediaPlayer2.spotify");
    let mut backend = MprisBackend::with_address(&bus.address);
    assert_eq!(
        snapshot_of(&backend.poll(), id.as_str()).status,
        PlaybackStatus::Playing
    );

    backend
        .send(&id, Command::Pause)
        .expect("pause is accepted");
    assert!(eventually(|| !spotify.is_playing()), "the player paused");
    let paused_at = spotify.position();
    backend
        .send(&id, Command::Pause)
        .expect("pausing a paused player is accepted");
    thread::sleep(Duration::from_millis(200));
    assert!(
        !spotify.is_playing(),
        "a second pause does not resume, unlike play/pause"
    );
    assert_eq!(spotify.position(), paused_at, "the position stays put");
    assert_eq!(spotify.commands(), ["Pause", "Pause"]);
    assert_eq!(
        snapshot_of(&backend.poll(), id.as_str()).status,
        PlaybackStatus::Paused
    );
}

#[test]
fn reports_a_missing_bus_instead_of_failing() {
    let mut backend = MprisBackend::with_address("unix:path=/nonexistent/singline-test-bus");
    assert!(backend.poll().is_empty());
    assert!(backend
        .last_error()
        .is_some_and(|error| error.starts_with("D-Bus")));
}

#[test]
fn youtube_tabs_are_read_as_songs() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("dbus-daemon is not available; skipping");
        return;
    };
    let video = FakeTrack {
        track_id: "/org/chromium/MediaPlayer2/TrackList/Track1".into(),
        title: "Example Artist - Paper Lanterns (Official Video)".into(),
        artists: vec!["Example ArtistVEVO".into()],
        album: String::new(),
        length: Duration::from_secs(200),
    };
    let chromium = FakePlayer::spawn(
        Some(&bus.address),
        "chromium.instance42",
        "Chromium",
        video,
        true,
    )
    .expect("fake Chromium registers");
    chromium.set_url(Some("https://www.youtube.com/watch?v=abc".into()));
    let song = FakeTrack {
        track_id: "/org/mozilla/firefox/TrackList/1".into(),
        title: "Paper Lanterns".into(),
        artists: vec!["Example Artist".into()],
        album: "Example Album".into(),
        length: Duration::from_secs(200),
    };
    let firefox = FakePlayer::spawn(
        Some(&bus.address),
        "firefox.instance_1_2",
        "Mozilla Firefox",
        song,
        false,
    )
    .expect("fake Firefox registers");
    firefox.set_url(Some("https://music.youtube.com/watch?v=abc".into()));

    let readings = MprisBackend::with_address(&bus.address).poll();
    let tab = snapshot_of(
        &readings,
        "mpris:org.mpris.MediaPlayer2.chromium.instance42",
    );
    assert_eq!((tab.player.service, tab.player.web), (Service::Other, true));
    let track = tab.track.expect("track metadata");
    assert_eq!(
        (track.title.as_str(), track.artist.as_str()),
        ("Paper Lanterns", "Example Artist"),
        "the video's title and channel become the song's"
    );
    assert_eq!(track.album, None);

    let music = snapshot_of(
        &readings,
        "mpris:org.mpris.MediaPlayer2.firefox.instance_1_2",
    );
    assert_eq!(music.player.service, Service::YouTubeMusic);
    assert_eq!(
        music.track.expect("track metadata").key,
        media_session::normalize::track_key(
            "Paper Lanterns",
            "Example Artist",
            Some("Example Album")
        )
    );
}
