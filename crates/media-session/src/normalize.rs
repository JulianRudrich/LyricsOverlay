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

/// The host of a URL in lower case, without user name and port.
fn host_of(url: &str) -> String {
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
    host.split(':').next().unwrap_or(host).to_owned()
}

/// True for `domain` itself and every subdomain of it.
fn on_domain(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|rest| rest.ends_with('.'))
}

/// Recognizes the service behind a URL, for web players that expose one.
pub fn service_from_url(url: &str) -> Option<Service> {
    let host = host_of(url);
    if on_domain(&host, "open.spotify.com") {
        Some(Service::Spotify)
    } else if on_domain(&host, "music.apple.com") {
        Some(Service::AppleMusic)
    } else if host.starts_with("music.amazon.") || host.contains(".music.amazon.") {
        Some(Service::AmazonMusic)
    } else if on_domain(&host, "music.youtube.com") {
        Some(Service::YouTubeMusic)
    } else {
        None
    }
}

/// Websites whose players need their own handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Site {
    /// youtube.com and youtu.be, where a video's title and channel stand in for
    /// song and artist.
    YouTube,
    /// music.youtube.com, which reports songs as cleanly as a music app.
    YouTubeMusic,
}

impl Site {
    /// YouTube Music is followed like the music apps; a YouTube video like any
    /// other web player, because most videos are not music.
    pub fn service(self) -> Service {
        match self {
            Site::YouTube => Service::Other,
            Site::YouTubeMusic => Service::YouTubeMusic,
        }
    }
}

/// Recognizes YouTube and YouTube Music by a tab's address.
pub fn site_from_url(url: &str) -> Option<Site> {
    let host = host_of(url);
    if on_domain(&host, "music.youtube.com") {
        Some(Site::YouTubeMusic)
    } else if ["youtube.com", "youtu.be", "youtube-nocookie.com"]
        .iter()
        .any(|domain| on_domain(&host, domain))
    {
        Some(Site::YouTube)
    } else {
        None
    }
}

/// Recognizes a page title such as "Song - YouTube Music" or "(3) Video -
/// YouTube", which browsers report for a tab that sets no metadata of its own.
/// Returns the site and the title without the site's name.
pub fn site_from_title(title: &str) -> Option<(Site, String)> {
    let title = title.trim();
    [
        (" - YouTube Music", Site::YouTubeMusic),
        (" - YouTube", Site::YouTube),
    ]
    .into_iter()
    .find_map(|(suffix, site)| {
        let rest = without_notification_count(title.strip_suffix(suffix)?.trim());
        (!rest.is_empty()).then(|| (site, rest.to_owned()))
    })
}

/// YouTube puts the number of unread notifications in front: "(3) Title".
fn without_notification_count(title: &str) -> &str {
    title
        .strip_prefix('(')
        .and_then(|rest| rest.split_once(") "))
        .filter(|(count, _)| {
            !count.is_empty() && count.chars().all(|c| c.is_ascii_digit() || c == '+')
        })
        .map_or(title, |(_, rest)| rest.trim_start())
}

/// What a browser reports about one tab, made ready for the overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserMedia {
    /// The site, from the tab's address or else from its title.
    pub site: Option<Site>,
    pub title: String,
    pub artist: String,
}

impl BrowserMedia {
    /// The service the tab counts as: YouTube Music, or `fallback` for every
    /// other site.
    pub fn service(&self, fallback: Service) -> Service {
        match self.site {
            Some(site) if fallback == Service::Other => site.service(),
            _ => fallback,
        }
    }
}

/// Recognizes the site of a browser tab from its address, when the browser
/// tells it, or from a title such as "Song - YouTube Music".
pub fn browser_media(url: Option<&str>, title: &str, artist: &str) -> BrowserMedia {
    let mut title = title.trim().to_owned();
    let mut site = url.and_then(site_from_url);
    if site.is_none() {
        if let Some((found, rest)) = site_from_title(&title) {
            site = Some(found);
            title = rest;
        }
    }
    BrowserMedia {
        site,
        title,
        artist: artist.trim().to_owned(),
    }
}

/// The YouTube Music app that Chrome and Edge install from music.youtube.com.
const YOUTUBE_MUSIC_WEB_APP: &str = "cinhimbnkkaeohfgghhklpknlkffjgod";

/// Desktop apps for YouTube Music, such as th-ch's "YouTube Music" or
/// "YouTube Music Desktop App".
fn is_youtube_music_app(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    (lower.contains("youtube") && lower.contains("music")) || lower.contains("ytmdesktop")
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
    let youtube_music = is_youtube_music_app(&app)
        || identity.is_some_and(|identity| identity.trim().eq_ignore_ascii_case("YouTube Music"));
    let web = is_browser(&app) && !youtube_music;
    let service = if app == "spotify" {
        Service::Spotify
    } else if app.contains("amazon") && app.contains("music") {
        Service::AmazonMusic
    } else if youtube_music {
        Service::YouTubeMusic
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
    if lower.contains(YOUTUBE_MUSIC_WEB_APP) {
        return (Service::YouTubeMusic, "YouTube Music".into(), true);
    }
    if is_youtube_music_app(&lower) {
        return (Service::YouTubeMusic, "YouTube Music".into(), false);
    }
    // Firefox names its sessions after a hash of its install folder; this is
    // the one for the default folder.
    if lower == "308046b0af4a39cb" {
        return (Service::Other, "Firefox".into(), true);
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
        "plasma-browser-integration",
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
        assert_eq!(
            service_from_url("https://music.youtube.com/watch?v=1&list=2"),
            Some(Service::YouTubeMusic)
        );
        assert_eq!(
            service_from_url("https://www.youtube.com/watch?v=1"),
            None,
            "a video is not a music service"
        );
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
            (Service::Other, "Firefox".into(), true),
            "Firefox in its default folder"
        );
        assert_eq!(
            identify_smtc("4F2C1A0B9D8E7F6A"),
            (Service::Other, "4F2C1A0B9D8E7F6A".into(), false)
        );
        assert_eq!(
            identify_smtc("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic").1,
            "Microsoft.ZuneMusic"
        );
    }

    #[test]
    fn youtube_is_recognized_by_url() {
        for url in [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtube.com/watch?v=1",
            "https://m.youtube.com/watch?v=1",
            "https://youtu.be/dQw4w9WgXcQ?t=42",
            "https://www.youtube-nocookie.com/embed/1",
            "HTTPS://WWW.YOUTUBE.COM/shorts/1",
        ] {
            assert_eq!(site_from_url(url), Some(Site::YouTube), "{url}");
        }
        assert_eq!(
            site_from_url("https://music.youtube.com/watch?v=1"),
            Some(Site::YouTubeMusic)
        );
        for url in [
            "https://notyoutube.com/watch?v=1",
            "https://youtube.com.evil.example/",
            "https://evil.example/www.youtube.com",
            "https://open.spotify.com/track/1",
            "",
        ] {
            assert_eq!(site_from_url(url), None, "{url}");
        }
        assert_eq!(Site::YouTube.service(), Service::Other);
        assert_eq!(Site::YouTubeMusic.service(), Service::YouTubeMusic);
    }

    #[test]
    fn youtube_is_recognized_by_page_title() {
        assert_eq!(
            site_from_title("Halo - YouTube Music"),
            Some((Site::YouTubeMusic, "Halo".into()))
        );
        assert_eq!(
            site_from_title("(3) Beyoncé - Halo (Official Video) - YouTube"),
            Some((Site::YouTube, "Beyoncé - Halo (Official Video)".into())),
            "without the notification count"
        );
        assert_eq!(
            site_from_title("(99+) Song - YouTube "),
            Some((Site::YouTube, "Song".into()))
        );
        assert_eq!(
            site_from_title("(Live) Song - YouTube"),
            Some((Site::YouTube, "(Live) Song".into())),
            "only a number is a notification count"
        );
        assert_eq!(site_from_title(" - YouTube"), None);
        assert_eq!(site_from_title("YouTube"), None);
        assert_eq!(site_from_title("My YouTube Mix"), None);
    }

    #[test]
    fn browser_tabs_get_their_site() {
        let media = browser_media(
            Some("https://music.youtube.com/watch?v=1"),
            " Halo ",
            "Beyoncé",
        );
        assert_eq!(media.site, Some(Site::YouTubeMusic));
        assert_eq!(media.service(Service::Other), Service::YouTubeMusic);
        assert_eq!(
            (media.title.as_str(), media.artist.as_str()),
            ("Halo", "Beyoncé")
        );

        let titled = browser_media(None, "Halo - YouTube Music", "");
        assert_eq!(
            (titled.site, titled.title.as_str()),
            (Some(Site::YouTubeMusic), "Halo")
        );
        assert_eq!(
            titled.service(Service::Spotify),
            Service::Spotify,
            "a known service stays"
        );

        let video = browser_media(
            Some("https://www.youtube.com/watch?v=1"),
            "Video",
            "Channel",
        );
        assert_eq!(video.service(Service::Other), Service::Other);

        let plain = browser_media(Some("https://example.com/"), "Song - Remastered", "Artist");
        assert_eq!(
            plain,
            BrowserMedia {
                site: None,
                title: "Song - Remastered".into(),
                artist: "Artist".into()
            }
        );
    }

    #[test]
    fn youtube_music_apps() {
        assert_eq!(
            identify_mpris(
                "org.mpris.MediaPlayer2.youtube-music",
                Some("YouTube Music"),
                None
            ),
            (Service::YouTubeMusic, "YouTube Music".into(), false)
        );
        assert_eq!(
            identify_mpris(
                "org.mpris.MediaPlayer2.chromium.instance7",
                Some("YouTube Music"),
                None
            ),
            (Service::YouTubeMusic, "YouTube Music".into(), false),
            "an Electron app on Chromium's MPRIS"
        );
        assert_eq!(
            identify_mpris(
                "org.mpris.MediaPlayer2.plasma-browser-integration",
                Some("Mozilla Firefox"),
                Some("https://music.youtube.com/watch?v=1")
            ),
            (Service::YouTubeMusic, "Mozilla Firefox".into(), true)
        );
        assert_eq!(
            identify_mpris(
                "org.mpris.MediaPlayer2.chromium.instance9",
                Some("Chromium"),
                Some("https://www.youtube.com/watch?v=1")
            ),
            (Service::Other, "Chromium".into(), true),
            "a video is a web player like any other"
        );
        assert_eq!(
            identify_smtc("Chrome._crx_cinhimbnkkaeohfgghhklpknlkffjgod"),
            (Service::YouTubeMusic, "YouTube Music".into(), true),
            "the app Chrome installs from music.youtube.com"
        );
        assert_eq!(
            identify_smtc("MSEdge._crx_cinhimbnkkaeohfgghhklpknlkffjgod").0,
            Service::YouTubeMusic
        );
        assert_eq!(
            identify_smtc("com.github.th-ch.youtube-music"),
            (Service::YouTubeMusic, "YouTube Music".into(), false)
        );
        assert_eq!(identify_smtc("ytmdesktop.app").0, Service::YouTubeMusic);
        assert_eq!(
            identify_smtc("Chrome._crx_aghbiahbpaijignceidepookljebhfak").0,
            Service::Other,
            "other installed web apps stay unknown"
        );
    }
}
