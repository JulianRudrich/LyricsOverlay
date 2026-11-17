//! Turning what each operating system reports into the same, clean metadata.
//!
//! Everything here is pure, so the platform quirks are tested on every system,
//! not only where they occur.

use sha2::{Digest, Sha256};

use crate::model::Service;

/// A key for per-song settings that is the same in every player.
///
/// Based on title, artist and album rather than a player's own id, so a timing
/// correction or your own lyrics follow the song from Spotify to Apple Music.
/// Case, accents and punctuation do not matter.
pub fn track_key(title: &str, artist: &str, album: Option<&str>) -> String {
    let mut hasher = Sha256::new();
    for field in [title, artist, album.unwrap_or("")] {
        let normalized = lyrics_core::matching::normalized(field);
        hasher.update(normalized.len().to_le_bytes());
        hasher.update(normalized.as_bytes());
    }
    let digest = hasher.finalize();
    let hex: String = digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("track:{hex}")
}

/// Spotify on Linux reports `/com/spotify/track/ID`; elsewhere it is `spotify:track:ID`.
pub fn spotify_uri_from_mpris(track_id: &str) -> Option<String> {
    let rest = track_id.strip_prefix("/com/spotify/")?;
    let (kind, id) = rest.split_once('/')?;
    (!kind.is_empty() && !id.is_empty() && !id.contains('/'))
        .then(|| format!("spotify:{kind}:{id}"))
}

/// Apple Music for Windows puts "Artist — Album" into the artist field and leaves
/// the album empty. Splits it back apart; other fields are returned unchanged.
pub fn split_artist_album(artist: &str, album: Option<&str>) -> (String, Option<String>) {
    let album = album.map(str::trim).filter(|album| !album.is_empty());
    if album.is_none() {
        if let Some((name, record)) = artist.rsplit_once(" — ") {
            let (name, record) = (name.trim(), record.trim());
            if !name.is_empty() && !record.is_empty() {
                return (name.to_owned(), Some(record.to_owned()));
            }
        }
    }
    (artist.trim().to_owned(), album.map(str::to_owned))
}

/// Joins a list of artists the way the players show them.
pub fn join_artists<S: AsRef<str>>(artists: &[S]) -> String {
    artists
        .iter()
        .map(|artist| artist.as_ref().trim())
        .filter(|artist| !artist.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Recognizes the service behind a URL, for web players that expose one.
pub fn service_from_url(url: &str) -> Option<Service> {
    let host = url
        .split_once("://")
        .map_or(url, |(_, rest)| rest)
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let host = host
        .rsplit_once('@')
        .map_or(host.as_str(), |(_, host)| host);
    let host = host.split(':').next().unwrap_or(host);
    let matches = |domain: &str| host == domain || host.ends_with(&format!(".{domain}"));
    if matches("open.spotify.com") {
        Some(Service::Spotify)
    } else if matches("music.apple.com") {
        Some(Service::AppleMusic)
    } else if host.starts_with("music.amazon.") || host.contains(".music.amazon.") {
        Some(Service::AmazonMusic)
    } else {
        None
    }
}

/// What an MPRIS bus name, the player's identity and its URL say about a player.
/// Returns the service, a display name and whether it is a web player.
pub fn identify_mpris(
    bus_name: &str,
    identity: Option<&str>,
    url: Option<&str>,
) -> (Service, String, bool) {
    let short = bus_name
        .strip_prefix("org.mpris.MediaPlayer2.")
        .unwrap_or(bus_name);
    let app = short
        .split('.')
        .next()
        .unwrap_or(short)
        .to_ascii_lowercase();
    let name = identity
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| capitalize(&app));
    let web = is_browser(&app);
    let service = if app == "spotify" {
        Service::Spotify
    } else if app.contains("amazon") && app.contains("music") {
        Service::AmazonMusic
    } else {
        url.and_then(service_from_url).unwrap_or(Service::Other)
    };
    (service, name, web)
}

/// What a Windows app user model id says about a player.
/// Returns the service, a display name and whether it is a web player.
pub fn identify_smtc(app_id: &str) -> (Service, String, bool) {
    let lower = app_id.to_ascii_lowercase();
    let executable = lower
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(&lower)
        .trim_end_matches(".exe");
    if lower.contains("spotify") {
        return (Service::Spotify, "Spotify".into(), false);
    }
    if lower.contains("applemusic") || lower.contains("apple music") {
        return (Service::AppleMusic, "Apple Music".into(), false);
    }
    if lower.contains("amazon") && lower.contains("music") {
        return (Service::AmazonMusic, "Amazon Music".into(), false);
    }
    let browser = [
        ("msedge", "Edge"),
        ("chrome", "Chrome"),
        ("firefox", "Firefox"),
        ("brave", "Brave"),
        ("opera", "Opera"),
        ("vivaldi", "Vivaldi"),
    ]
    .into_iter()
    .find(|(needle, _)| executable.contains(needle) || lower.contains(needle));
    if let Some((_, name)) = browser {
        return (Service::Other, name.into(), true);
    }
    let name = app_id
        .rsplit(['\\', '/', '!'])
        .find(|part| !part.is_empty() && !part.eq_ignore_ascii_case("app"))
        .unwrap_or(app_id)
        .trim_end_matches(".exe");
    (Service::Other, name.to_owned(), false)
}

fn is_browser(app: &str) -> bool {
    [
        "firefox",
        "chromium",
        "chrome",
        "brave",
        "vivaldi",
        "opera",
        "edge",
        "microsoftedge",
        "epiphany",
        "librewolf",
        "zen",
    ]
    .iter()
    .any(|browser| app.starts_with(browser))
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_keys_ignore_formatting_but_not_content() {
        let key = track_key("Halo", "Beyoncé", Some("I Am… Sasha Fierce"));
        assert_eq!(key, track_key("HALO", "beyonce", Some("I am sasha fierce")));
        assert!(key.starts_with("track:") && key.len() == "track:".len() + 32);
        assert_ne!(key, track_key("Halo", "Beyoncé", None));
        assert_ne!(
            track_key("ab", "c", None),
            track_key("a", "bc", None),
            "fields cannot run together"
        );
    }

    #[test]
    fn spotify_ids_from_linux() {
        assert_eq!(
            spotify_uri_from_mpris("/com/spotify/track/4uLU6hMCjMI75M1A2tKUQC").as_deref(),
            Some("spotify:track:4uLU6hMCjMI75M1A2tKUQC")
        );
        assert_eq!(
            spotify_uri_from_mpris("/com/spotify/episode/xyz").as_deref(),
            Some("spotify:episode:xyz")
        );
        assert_eq!(
            spotify_uri_from_mpris("/org/mpris/MediaPlayer2/TrackList/NoTrack"),
            None
        );
        assert_eq!(spotify_uri_from_mpris("/com/spotify/track/"), None);
    }

    #[test]
    fn apple_music_on_windows_is_split_apart() {
        assert_eq!(
            split_artist_album("Billie Eilish — HIT ME HARD AND SOFT", None),
            ("Billie Eilish".into(), Some("HIT ME HARD AND SOFT".into()))
        );
        assert_eq!(
            split_artist_album("A — B — C", Some("")),
            ("A — B".into(), Some("C".into())),
            "the album follows the last dash"
        );
        assert_eq!(
            split_artist_album("Jay-Z", None),
            ("Jay-Z".into(), None),
            "plain hyphens stay"
        );
        assert_eq!(
            split_artist_album("A — B", Some("Album")),
            ("A — B".into(), Some("Album".into())),
            "a real album wins"
        );
        assert_eq!(
            split_artist_album(" — Album", None),
            ("— Album".into(), None),
            "nothing before the dash"
        );
    }

    #[test]
    fn artists_are_joined() {
        assert_eq!(join_artists(&["A", " B ", ""]), "A, B");
        assert_eq!(join_artists::<&str>(&[]), "");
    }

    #[test]
    fn web_players_are_recognized_by_url() {
        assert_eq!(
            service_from_url("https://music.amazon.de/albums/B0"),
            Some(Service::AmazonMusic)
        );
        assert_eq!(
            service_from_url("https://music.amazon.com"),
            Some(Service::AmazonMusic)
        );
        assert_eq!(
            service_from_url("https://music.apple.com/de/album/1"),
            Some(Service::AppleMusic)
        );
        assert_eq!(
            service_from_url("https://open.spotify.com/track/1"),
            Some(Service::Spotify)
        );
        assert_eq!(service_from_url("https://www.youtube.com/watch?v=1"), None);
        assert_eq!(
            service_from_url("https://evil.example/music.amazon.com"),
            None,
            "the path does not count"
        );
        assert_eq!(
            service_from_url("https://music.apple.com.evil.example/"),
            None
        );
    }

    #[test]
    fn mpris_players() {
        assert_eq!(
            identify_mpris("org.mpris.MediaPlayer2.spotify", Some("Spotify"), None),
            (Service::Spotify, "Spotify".into(), false)
        );
        assert_eq!(
            identify_mpris(
                "org.mpris.MediaPlayer2.firefox.instance_1_42",
                Some("Mozilla Firefox"),
                Some("https://music.amazon.de/my/library")
            ),
            (Service::AmazonMusic, "Mozilla Firefox".into(), true)
        );
        assert_eq!(
            identify_mpris("org.mpris.MediaPlayer2.chromium.instance123", None, None),
            (Service::Other, "Chromium".into(), true)
        );
        assert_eq!(
            identify_mpris("org.mpris.MediaPlayer2.vlc", Some(""), None),
            (Service::Other, "Vlc".into(), false)
        );
    }

    #[test]
    fn windows_players() {
        assert_eq!(identify_smtc("Spotify.exe").0, Service::Spotify);
        assert_eq!(
            identify_smtc("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify").0,
            Service::Spotify
        );
        assert_eq!(
            identify_smtc("AppleInc.AppleMusicWin_nzyj5cx40ttqa!App"),
            (Service::AppleMusic, "Apple Music".into(), false)
        );
        assert_eq!(
            identify_smtc("C:\\Users\\x\\AppData\\Local\\Amazon Music\\Amazon Music.exe").0,
            Service::AmazonMusic
        );
        assert_eq!(
            identify_smtc("MSEdge"),
            (Service::Other, "Edge".into(), true)
        );
        assert_eq!(
            identify_smtc("308046B0AF4A39CB"),
            (Service::Other, "308046B0AF4A39CB".into(), false)
        );
        assert_eq!(
            identify_smtc("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic").1,
            "Microsoft.ZuneMusic"
        );
    }
}
