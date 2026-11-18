//! The messages the extension sends, and how a tab becomes a [`Reading`].
//! The extension cleans its messages too; everything is checked again here.

use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::model::{
    Capabilities, PlaybackStatus, PlayerId, PlayerInfo, PlayerSnapshot, Reading, Service, Track,
};
use crate::normalize::{browser_media, service_from_url, track_key};

pub(crate) const MAX_TABS: usize = 32;
pub(crate) const MAX_CLOSED: usize = 64;
const MAX_TEXT: usize = 512;
const MAX_URL: usize = 2048;
/// A sample older than this is taken as current; clocks may disagree.
const MAX_SAMPLE_AGE: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PairRequest {
    pub code: String,
    #[serde(default)]
    pub browser: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct UpdateRequest {
    pub tabs: Vec<TabUpdate>,
    pub closed: Vec<u64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct Controls {
    pub play_pause: bool,
    pub next: bool,
    pub previous: bool,
    pub seek: bool,
}

/// What one tab plays, as the extension reports it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct TabUpdate {
    pub tab_id: u64,
    pub url: String,
    /// From the Media Session: for YouTube the video's title and channel.
    pub title: String,
    pub artist: String,
    pub album: String,
    pub artwork: String,
    /// The tab's title, for pages that set no Media Session metadata.
    pub page_title: String,
    pub current_time: Option<f64>,
    pub duration: Option<f64>,
    pub playback_rate: Option<f64>,
    pub paused: bool,
    pub ended: bool,
    /// Milliseconds since 1970, when the page read the position.
    pub sampled_at: Option<f64>,
    pub controls: Controls,
}

impl Default for TabUpdate {
    /// A tab that does not say otherwise is taken as paused.
    fn default() -> Self {
        Self {
            tab_id: 0,
            url: String::new(),
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            artwork: String::new(),
            page_title: String::new(),
            current_time: None,
            duration: None,
            playback_rate: None,
            paused: true,
            ended: false,
            sampled_at: None,
            controls: Controls::default(),
        }
    }
}

fn clipped(text: &str, limit: usize) -> String {
    text.trim().chars().take(limit).collect()
}

fn https(url: &str) -> String {
    let url = clipped(url, MAX_URL);
    if url
        .get(..8)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"))
    {
        url
    } else {
        String::new()
    }
}

impl TabUpdate {
    /// The update with every text cut to its limit, only https addresses and
    /// only finite numbers.
    pub fn sanitized(self) -> Self {
        let finite = |value: Option<f64>| value.filter(|value| value.is_finite());
        Self {
            tab_id: self.tab_id,
            url: https(&self.url),
            title: clipped(&self.title, MAX_TEXT),
            artist: clipped(&self.artist, MAX_TEXT),
            album: clipped(&self.album, MAX_TEXT),
            artwork: https(&self.artwork),
            page_title: clipped(&self.page_title, MAX_TEXT),
            current_time: finite(self.current_time).filter(|time| *time >= 0.0),
            duration: finite(self.duration)
                .filter(|duration| *duration > 0.0 && *duration < 86_400.0),
            playback_rate: finite(self.playback_rate).filter(|rate| *rate > 0.0 && *rate <= 16.0),
            sampled_at: finite(self.sampled_at),
            ..self
        }
    }

    pub fn playing(&self) -> bool {
        !self.paused && !self.ended
    }
}

/// When the page read the position, as an [`Instant`]: `sampled_at` is
/// compared with the time of this computer, which the browser shares.
pub(crate) fn sample_instant(sampled_at: Option<f64>, now: Instant, now_unix_ms: f64) -> Instant {
    let age = sampled_at
        .map(|sampled| (now_unix_ms - sampled).max(0.0) / 1_000.0)
        .filter(|age| age.is_finite())
        .map(Duration::from_secs_f64)
        .filter(|age| *age <= MAX_SAMPLE_AGE)
        .unwrap_or_default();
    now.checked_sub(age).unwrap_or(now)
}

/// The reading for one tab. A tab without a known duration has no track yet:
/// the page is still loading the song, or it plays a live stream, and the
/// lyrics search needs the duration.
pub(crate) fn reading(
    id: PlayerId,
    browser: &str,
    tab: &TabUpdate,
    sampled_at: Instant,
) -> Reading {
    let url = Some(tab.url.as_str()).filter(|url| !url.is_empty());
    let (title, artist) = if tab.title.is_empty() {
        (tab.page_title.as_str(), "")
    } else {
        (tab.title.as_str(), tab.artist.as_str())
    };
    let media = browser_media(url, title, artist);
    let service = media.service(url.and_then(service_from_url).unwrap_or(Service::Other));
    let album = Some(tab.album.as_str()).filter(|album| !album.is_empty());
    let track = (!media.title.is_empty())
        .then_some(tab.duration)
        .flatten()
        .map(|duration| Track {
            key: track_key(&media.title, &media.artist, album),
            source_id: None,
            title: media.title.clone(),
            artist: media.artist.clone(),
            album: album.map(str::to_owned),
            duration: Some(duration),
            artwork_url: Some(tab.artwork.clone()).filter(|url| !url.is_empty()),
        });
    let status = if tab.ended {
        PlaybackStatus::Stopped
    } else if tab.paused {
        PlaybackStatus::Paused
    } else {
        PlaybackStatus::Playing
    };
    Reading::Snapshot(PlayerSnapshot {
        player: PlayerInfo {
            id,
            name: browser.to_owned(),
            service,
            web: true,
        },
        status,
        position: track.as_ref().and(tab.current_time),
        track,
        sampled_at,
        rate: tab.playback_rate.unwrap_or(1.0),
        capabilities: Capabilities {
            play_pause: tab.controls.play_pause,
            next: tab.controls.next,
            previous: tab.controls.previous,
            seek: tab.controls.seek,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the extension sends for a YouTube tab; its tests build the same.
    const FIXTURE: &str = include_str!("../../../../extensions/browser/test/fixtures/update.json");

    fn fixture() -> UpdateRequest {
        serde_json::from_str(FIXTURE).expect("the extension's update parses")
    }

    fn snapshot(reading: Reading) -> PlayerSnapshot {
        match reading {
            Reading::Snapshot(snapshot) => snapshot,
            other => panic!("expected a snapshot, got {other:?}"),
        }
    }

    #[test]
    fn reads_what_the_extension_sends() {
        let request = fixture();
        assert_eq!(request.closed, [7]);
        let tab = request.tabs[0].clone().sanitized();
        assert_eq!(tab.tab_id, 12);
        let now = Instant::now();
        let reading = snapshot(reading(PlayerId::new("bridge:c1:12"), "Chrome", &tab, now));
        assert_eq!(reading.player.name, "Chrome");
        assert_eq!(
            (reading.player.service, reading.player.web),
            (Service::Other, true)
        );
        assert_eq!(reading.status, PlaybackStatus::Playing);
        assert_eq!(reading.position, Some(30.5));
        let track = reading.track.expect("a track");
        assert_eq!(
            (track.title.as_str(), track.artist.as_str()),
            ("Paper Lanterns", "Example Artist"),
            "the video's title and channel become the song's"
        );
        assert_eq!(track.duration, Some(200.0));
        assert_eq!(
            track.key,
            track_key("Paper Lanterns", "Example Artist", None)
        );
        assert_eq!(
            track.artwork_url.as_deref(),
            Some("https://i.ytimg.com/vi/abc/maxresdefault.jpg")
        );
        assert_eq!(
            reading.capabilities,
            Capabilities {
                play_pause: true,
                next: true,
                previous: false,
                seek: true
            }
        );
    }

    #[test]
    fn youtube_music_and_other_sites_are_named() {
        let tab = |url: &str| TabUpdate {
            url: url.into(),
            title: "Song (Official Video)".into(),
            artist: "Artist".into(),
            album: "Album".into(),
            duration: Some(180.0),
            current_time: Some(1.0),
            ..TabUpdate::default()
        };
        let read = |url: &str| {
            snapshot(reading(
                PlayerId::new("bridge:c:1"),
                "Firefox",
                &tab(url).sanitized(),
                Instant::now(),
            ))
        };
        let music = read("https://music.youtube.com/watch?v=1");
        assert_eq!(music.player.service, Service::YouTubeMusic);
        assert_eq!(
            music.track.expect("track").title,
            "Song (Official Video)",
            "YouTube Music is not cleaned up"
        );
        assert_eq!(
            read("https://open.spotify.com/").player.service,
            Service::Spotify
        );
        assert_eq!(
            read("https://music.amazon.de/").player.service,
            Service::AmazonMusic
        );
        assert_eq!(
            read("https://music.apple.com/").player.service,
            Service::AppleMusic
        );
        assert_eq!(
            read("javascript:alert(1)").player.service,
            Service::Other,
            "only https pages count"
        );
    }

    #[test]
    fn a_tab_without_duration_has_no_track_yet() {
        let tab = TabUpdate {
            title: "Live stream".into(),
            current_time: Some(5.0),
            duration: Some(f64::INFINITY),
            ..TabUpdate::default()
        }
        .sanitized();
        let reading = snapshot(reading(PlayerId::new("b"), "Edge", &tab, Instant::now()));
        assert_eq!((reading.track, reading.position), (None, None));
        assert_eq!(reading.status, PlaybackStatus::Paused, "paused by default");
    }

    #[test]
    fn the_page_title_stands_in_for_missing_metadata() {
        let tab = TabUpdate {
            url: "https://www.youtube.com/watch?v=1".into(),
            page_title: "(1) Artist - Song (Lyrics) - YouTube".into(),
            duration: Some(200.0),
            ..TabUpdate::default()
        };
        let track = snapshot(reading(PlayerId::new("b"), "Edge", &tab, Instant::now()))
            .track
            .expect("track");
        assert_eq!(
            (track.title.as_str(), track.artist.as_str()),
            ("Song", "Artist")
        );
    }

    #[test]
    fn everything_is_checked_again() {
        let tab = TabUpdate {
            url: "http://music.youtube.com/".into(),
            title: "t".repeat(2_000),
            artwork: "data:image/png;base64,AAAA".into(),
            current_time: Some(-3.0),
            duration: Some(f64::NAN),
            playback_rate: Some(100.0),
            sampled_at: Some(f64::INFINITY),
            ..TabUpdate::default()
        }
        .sanitized();
        assert_eq!(tab.url, "");
        assert_eq!(tab.title.chars().count(), MAX_TEXT);
        assert_eq!(tab.artwork, "");
        assert_eq!(
            (
                tab.current_time,
                tab.duration,
                tab.playback_rate,
                tab.sampled_at
            ),
            (None, None, None, None)
        );
    }

    #[test]
    fn sample_times_are_turned_into_instants() {
        let now = Instant::now();
        let unix = 1_800_000_000_000.0;
        assert_eq!(
            now.duration_since(sample_instant(Some(unix - 250.0), now, unix)),
            Duration::from_millis(250)
        );
        assert_eq!(
            sample_instant(Some(unix + 5_000.0), now, unix),
            now,
            "from the future"
        );
        assert_eq!(
            sample_instant(Some(unix - 60_000.0), now, unix),
            now,
            "too old to trust"
        );
        assert_eq!(sample_instant(None, now, unix), now);
    }
}
