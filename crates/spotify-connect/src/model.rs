//! The Spotify Web API answers this crate reads. Only the fields the app uses
//! are decoded, and entries that do not fit (local files, podcast episodes,
//! deleted items) are skipped instead of failing the whole page.

use std::fmt;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};

/// What the token endpoint answers. The tokens never appear in `Debug`.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    /// Seconds.
    pub expires_in: u64,
    /// Spotify may rotate the refresh token on refresh; none means keep the old one.
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// Space-separated scopes this login granted.
    #[serde(default)]
    pub scope: Option<String>,
}

impl fmt::Debug for TokenResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TokenResponse")
            .field("access_token", &"<hidden>")
            .field("expires_in", &self.expires_in)
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<hidden>"),
            )
            .field("scope", &self.scope)
            .finish()
    }
}

/// `{"error": "invalid_grant", "error_description": "…"}` from the token endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TokenError {
    pub error: String,
    #[serde(default)]
    pub error_description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct User {
    pub id: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

impl User {
    /// The name to show: the display name, or the user ID without one.
    pub fn name(&self) -> &str {
        self.display_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(&self.id)
    }
}

/// One page of a list. `next` is the address of the following page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next: Option<String>,
    /// Entries in the whole list, skipped ones included.
    pub total: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub owner_id: String,
    pub collaborative: bool,
    pub track_count: Option<usize>,
}

impl Playlist {
    /// A development-mode app may read the items only of playlists the user
    /// owns or collaborates on.
    pub fn readable_by(&self, user_id: &str) -> bool {
        self.owner_id == user_id || self.collaborative
    }
}

/// A playable track of a playlist or of Liked Songs. Title and artist feed
/// "Up next"; the main artist's ID keeps one artist's songs apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackInfo {
    pub uri: String,
    pub name: String,
    pub artist: String,
    pub artist_id: Option<String>,
}

/// A Spotify Connect device from `GET /me/player/devices`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Device {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: String,
    /// "Computer", "Smartphone", "Speaker" …
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub is_restricted: bool,
}

/// Where Shuffle & play starts: Spotify on this computer. Among computers the
/// one named like this computer comes first, then the active one. Other
/// devices only when they already play, so a shuffle never starts on a
/// speaker by surprise.
pub fn preferred_device<'a>(
    devices: &'a [Device],
    this_computer: Option<&str>,
) -> Option<&'a Device> {
    let usable: Vec<&Device> = devices
        .iter()
        .filter(|device| device.id.is_some() && !device.is_restricted)
        .collect();
    let computers: Vec<&Device> = usable
        .iter()
        .copied()
        .filter(|device| device.kind.eq_ignore_ascii_case("computer"))
        .collect();
    let named = this_computer
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .and_then(|name| {
            computers
                .iter()
                .copied()
                .find(|device| device.name.trim().eq_ignore_ascii_case(name))
        });
    named
        .or_else(|| computers.iter().copied().find(|device| device.is_active))
        .or_else(|| usable.iter().copied().find(|device| device.is_active))
        .or_else(|| computers.first().copied())
}

/// What `GET /me/player/currently-playing` reports; nothing when it answers 204.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentlyPlaying {
    pub context_uri: Option<String>,
    pub item_uri: Option<String>,
    pub is_playing: bool,
}

impl CurrentlyPlaying {
    /// Context URIs come as `spotify:playlist:ID` or the older
    /// `spotify:user:NAME:playlist:ID`.
    pub fn is_playlist(&self, playlist_id: &str) -> bool {
        self.context_uri.as_deref().is_some_and(|uri| {
            uri.strip_suffix(playlist_id)
                .is_some_and(|rest| rest.ends_with(":playlist:"))
        })
    }
}

/// The ranges of `GET /me/top/…`, as Spotify defines them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TimeRange {
    /// About the last four weeks.
    Short,
    /// About the last six months.
    Medium,
    /// About the last year.
    Long,
}

impl TimeRange {
    pub const ALL: [TimeRange; 3] = [TimeRange::Short, TimeRange::Medium, TimeRange::Long];

    pub fn as_param(self) -> &'static str {
        match self {
            TimeRange::Short => "short_term",
            TimeRange::Medium => "medium_term",
            TimeRange::Long => "long_term",
        }
    }
}

/// A top artist or track; `uri` opens it in Spotify.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopItem {
    pub id: String,
    pub name: String,
    pub uri: String,
    /// The artists of a track; empty for an artist.
    pub artists: Vec<String>,
}

/// `GET /me`.
pub fn user(body: &[u8]) -> Result<User, serde_json::Error> {
    serde_json::from_slice(body)
}

/// `GET /me/library/contains`: one answer per URI asked.
pub fn saved_contains(body: &[u8]) -> Result<Vec<bool>, serde_json::Error> {
    serde_json::from_slice(body)
}

/// `GET /me/playlists`.
pub fn playlist_page(body: &[u8]) -> Result<Page<Playlist>, serde_json::Error> {
    let raw: RawPage<RawPlaylist> = serde_json::from_slice(body)?;
    Ok(Page {
        items: raw
            .items
            .into_iter()
            .filter_map(|entry| entry.0)
            .filter_map(RawPlaylist::into_playlist)
            .collect(),
        next: raw.next,
        total: raw.total,
    })
}

/// `GET /playlists/{id}/items` (entries hold `item`) and `GET /me/tracks`
/// (entries hold `track`). Keeps playable Spotify tracks only.
pub fn track_page(body: &[u8]) -> Result<Page<TrackInfo>, serde_json::Error> {
    let raw: RawPage<RawEntry> = serde_json::from_slice(body)?;
    Ok(Page {
        items: raw
            .items
            .into_iter()
            .filter_map(|entry| entry.0)
            .filter_map(RawEntry::into_track)
            .collect(),
        next: raw.next,
        total: raw.total,
    })
}

/// `GET /me/player/devices`.
pub fn devices(body: &[u8]) -> Result<Vec<Device>, serde_json::Error> {
    #[derive(Deserialize)]
    struct List {
        #[serde(default)]
        devices: Vec<Lenient<Device>>,
    }
    let list: List = serde_json::from_slice(body)?;
    Ok(list
        .devices
        .into_iter()
        .filter_map(|device| device.0)
        .collect())
}

/// `GET /me/player/currently-playing` with content.
pub fn currently_playing(body: &[u8]) -> Result<CurrentlyPlaying, serde_json::Error> {
    #[derive(Deserialize)]
    struct WithUri {
        #[serde(default)]
        uri: Option<String>,
    }
    #[derive(Deserialize)]
    struct Raw {
        #[serde(default)]
        context: Option<WithUri>,
        #[serde(default)]
        item: Option<WithUri>,
        #[serde(default)]
        is_playing: bool,
    }
    let raw: Raw = serde_json::from_slice(body)?;
    Ok(CurrentlyPlaying {
        context_uri: raw.context.and_then(|context| context.uri),
        item_uri: raw.item.and_then(|item| item.uri),
        is_playing: raw.is_playing,
    })
}

/// `GET /me/top/artists` and `GET /me/top/tracks`.
pub fn top_items(body: &[u8]) -> Result<Vec<TopItem>, serde_json::Error> {
    #[derive(Deserialize)]
    struct Artist {
        #[serde(default)]
        name: Option<String>,
    }
    #[derive(Deserialize)]
    struct Raw {
        id: String,
        uri: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        artists: Vec<Artist>,
    }
    let page: RawPage<Raw> = serde_json::from_slice(body)?;
    Ok(page
        .items
        .into_iter()
        .filter_map(|item| item.0)
        .map(|raw| TopItem {
            id: raw.id,
            name: raw.name.unwrap_or_default(),
            uri: raw.uri,
            artists: raw
                .artists
                .into_iter()
                .filter_map(|artist| artist.name)
                .collect(),
        })
        .collect())
}

/// `{"id": "…"}` of a created playlist.
pub fn created_id(body: &[u8]) -> Result<String, serde_json::Error> {
    #[derive(Deserialize)]
    struct Created {
        id: String,
    }
    serde_json::from_slice::<Created>(body).map(|created| created.id)
}

/// The message of a Web API error, `{"error": {"status": 403, "message": "…"}}`.
pub fn api_error_message(body: &[u8]) -> Option<String> {
    #[derive(Deserialize)]
    struct Inner {
        #[serde(default)]
        message: Option<String>,
    }
    #[derive(Deserialize)]
    struct Outer {
        error: Inner,
    }
    serde_json::from_slice::<Outer>(body)
        .ok()
        .and_then(|outer| outer.error.message)
        .filter(|message| !message.trim().is_empty())
}

#[derive(Deserialize)]
#[serde(bound = "T: DeserializeOwned")]
struct RawPage<T> {
    /// Entries that are `null` or do not fit decode to `None` and are skipped.
    #[serde(default)]
    items: Vec<Lenient<T>>,
    #[serde(default)]
    next: Option<String>,
    #[serde(default)]
    total: Option<usize>,
}

/// Decodes to `None` instead of failing, so one odd entry skips only itself.
struct Lenient<T>(Option<T>);

impl<'de, T: DeserializeOwned> Deserialize<'de> for Lenient<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(Lenient(T::deserialize(value).ok()))
    }
}

#[derive(Deserialize)]
struct RawPlaylist {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    owner: Option<RawOwner>,
    #[serde(default)]
    collaborative: bool,
    /// `{"total": n}`; newer answers call it `items`.
    #[serde(default)]
    tracks: Option<RawTotal>,
    #[serde(default)]
    items: Option<RawTotal>,
}

impl RawPlaylist {
    fn into_playlist(self) -> Option<Playlist> {
        Some(Playlist {
            id: self.id.filter(|id| !id.is_empty())?,
            name: self.name.unwrap_or_default(),
            owner_id: self.owner.and_then(|owner| owner.id).unwrap_or_default(),
            collaborative: self.collaborative,
            track_count: self.items.or(self.tracks).and_then(|count| count.total),
        })
    }
}

#[derive(Deserialize)]
struct RawOwner {
    #[serde(default)]
    id: Option<String>,
}

#[derive(Deserialize)]
struct RawTotal {
    #[serde(default)]
    total: Option<usize>,
}

#[derive(Deserialize)]
struct RawEntry {
    #[serde(default)]
    item: Lenient<RawTrack>,
    #[serde(default)]
    track: Lenient<RawTrack>,
    #[serde(default)]
    is_local: bool,
}

impl<T> Default for Lenient<T> {
    fn default() -> Self {
        Lenient(None)
    }
}

impl RawEntry {
    fn into_track(self) -> Option<TrackInfo> {
        if self.is_local {
            return None;
        }
        let track = self.item.0.or(self.track.0)?;
        let playable = !track.is_local
            && track.kind.as_deref().is_none_or(|kind| kind == "track")
            && track
                .uri
                .as_deref()
                .is_some_and(|uri| uri.starts_with("spotify:track:"));
        if !playable {
            return None;
        }
        let main = track.artists.into_iter().next();
        Some(TrackInfo {
            uri: track.uri?,
            name: track.name.unwrap_or_default(),
            artist: main
                .as_ref()
                .and_then(|artist| artist.name.clone())
                .unwrap_or_default(),
            artist_id: main
                .and_then(|artist| artist.id)
                .filter(|id| !id.is_empty()),
        })
    }
}

#[derive(Deserialize)]
struct RawTrack {
    #[serde(default)]
    uri: Option<String>,
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    is_local: bool,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    artists: Vec<RawArtist>,
}

#[derive(Deserialize)]
struct RawArtist {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bytes(value: serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(&value).expect("valid JSON")
    }

    #[test]
    fn decodes_tokens_without_showing_them() {
        let token: TokenResponse = serde_json::from_value(json!({
            "access_token": "BQD-access", "token_type": "Bearer", "expires_in": 3600,
            "refresh_token": "AQC-refresh", "scope": "user-top-read user-library-read"
        }))
        .expect("token");
        assert_eq!(token.expires_in, 3600);
        assert_eq!(token.refresh_token.as_deref(), Some("AQC-refresh"));
        let shown = format!("{token:?}");
        assert!(
            !shown.contains("BQD-access") && !shown.contains("AQC-refresh"),
            "{shown}"
        );

        let refreshed: TokenResponse =
            serde_json::from_value(json!({"access_token": "new", "expires_in": 3600}))
                .expect("token without rotation");
        assert_eq!(refreshed.refresh_token, None);
        let error: TokenError = serde_json::from_value(
            json!({"error": "invalid_grant", "error_description": "Refresh token revoked"}),
        )
        .expect("error");
        assert_eq!(error.error, "invalid_grant");
    }

    #[test]
    fn decodes_the_current_user() {
        let named =
            user(&bytes(json!({"id": "julian", "display_name": "Julian R."}))).expect("user");
        assert_eq!(named.name(), "Julian R.");
        let unnamed = user(&bytes(json!({"id": "julian", "display_name": null}))).expect("user");
        assert_eq!(unnamed.name(), "julian");
        let blank = user(&bytes(json!({"id": "julian", "display_name": "  "}))).expect("user");
        assert_eq!(blank.name(), "julian");
        assert!(user(b"{}").is_err());
    }

    #[test]
    fn decodes_saved_state() {
        assert_eq!(
            saved_contains(b"[true, false]").expect("list"),
            vec![true, false]
        );
        assert!(saved_contains(b"{\"error\": {}}").is_err());
    }

    #[test]
    fn decodes_playlists_and_finds_readable_ones() {
        let page = playlist_page(&bytes(json!({
            "items": [
                {"id": "own", "name": "Road trip", "owner": {"id": "julian"}, "collaborative": false, "tracks": {"total": 42}},
                {"id": "shared", "name": "Band", "owner": {"id": "mike"}, "collaborative": true, "items": {"total": 7}},
                {"id": "theirs", "name": "Daily Mix 1", "owner": {"id": "spotify"}},
                null,
                {"name": "No ID"}
            ],
            "next": "https://api.spotify.com/v1/me/playlists?offset=50&limit=50",
            "total": 55
        })))
        .expect("page");
        let ids: Vec<&str> = page
            .items
            .iter()
            .map(|playlist| playlist.id.as_str())
            .collect();
        assert_eq!(ids, ["own", "shared", "theirs"]);
        assert_eq!(page.items[0].track_count, Some(42));
        assert_eq!(page.items[1].track_count, Some(7));
        assert_eq!(page.items[2].track_count, None);
        assert_eq!(page.total, Some(55));
        assert!(page.next.is_some());
        let readable: Vec<&str> = page
            .items
            .iter()
            .filter(|playlist| playlist.readable_by("julian"))
            .map(|playlist| playlist.id.as_str())
            .collect();
        assert_eq!(readable, ["own", "shared"]);

        let last = playlist_page(br#"{"items": [], "next": null}"#).expect("empty page");
        assert!(last.items.is_empty() && last.next.is_none());
    }

    #[test]
    fn decodes_playlist_items_and_liked_songs() {
        let track = |uri: &str| {
            json!({"type": "track", "uri": uri, "name": "Song", "is_local": false,
                   "artists": [{"id": "a1", "name": "Main"}, {"id": "a2", "name": "Guest"}]})
        };
        let page = track_page(&bytes(json!({
            "items": [
                {"item": track("spotify:track:1")},
                {"track": track("spotify:track:2")},
                {"item": {"type": "episode", "uri": "spotify:episode:3", "name": "Podcast"}},
                {"is_local": true, "item": {"uri": "spotify:local:a:b:c:1", "name": "Local"}},
                {"item": {"uri": "spotify:local:x", "is_local": true}},
                {"item": null},
                null,
                {"item": {"uri": "spotify:track:4", "name": "No artists"}},
                {"item": {"uri": 42}},
                {"track": track("spotify:track:5"), "item": null}
            ],
            "next": null,
            "total": 10
        })))
        .expect("page");
        let uris: Vec<&str> = page.items.iter().map(|track| track.uri.as_str()).collect();
        assert_eq!(
            uris,
            [
                "spotify:track:1",
                "spotify:track:2",
                "spotify:track:4",
                "spotify:track:5"
            ]
        );
        assert_eq!(
            page.items[0],
            TrackInfo {
                uri: "spotify:track:1".into(),
                name: "Song".into(),
                artist: "Main".into(),
                artist_id: Some("a1".into()),
            }
        );
        assert_eq!(page.items[2].artist, "");
        assert_eq!(page.items[2].artist_id, None);
        assert_eq!(page.total, Some(10));
    }

    #[test]
    fn decodes_devices_and_prefers_this_computer() {
        let list = devices(&bytes(json!({"devices": [
            {"id": "phone", "name": "Pixel", "type": "Smartphone", "is_active": true, "is_restricted": false},
            {"id": "laptop", "name": "Laptop", "type": "Computer", "is_active": false, "is_restricted": false},
            {"id": "desk", "name": "DESKTOP-1", "type": "Computer", "is_active": false, "is_restricted": false},
            {"id": null, "name": "Private", "type": "Computer", "is_active": true},
            {"id": "tv", "name": "TV", "type": "TV", "is_active": false, "is_restricted": true},
            {"name": 7}
        ]})))
        .expect("devices");
        assert_eq!(list.len(), 5);
        let pick =
            |name: Option<&str>| preferred_device(&list, name).and_then(|device| device.id.clone());
        assert_eq!(pick(Some("desktop-1")).as_deref(), Some("desk"));
        // No name match: an active computer would win, then any active device.
        assert_eq!(pick(None).as_deref(), Some("phone"));
        assert_eq!(pick(Some("elsewhere")).as_deref(), Some("phone"));

        let idle = devices(&bytes(json!({"devices": [
            {"id": "speaker", "name": "Kitchen", "type": "Speaker", "is_active": false},
            {"id": "laptop", "name": "Laptop", "type": "Computer", "is_active": false},
            {"id": "pc", "name": "PC", "type": "Computer", "is_active": true}
        ]})))
        .expect("devices");
        assert_eq!(
            preferred_device(&idle, None).and_then(|d| d.id.as_deref()),
            Some("pc")
        );
        assert_eq!(
            preferred_device(&idle[..2], None).and_then(|d| d.id.as_deref()),
            Some("laptop")
        );
        assert_eq!(
            preferred_device(&idle[..1], None),
            None,
            "a speaker never starts by surprise"
        );
        assert_eq!(preferred_device(&[], None), None);
    }

    #[test]
    fn decodes_what_is_playing() {
        let playing = currently_playing(&bytes(json!({
            "context": {"uri": "spotify:playlist:abc"}, "item": {"uri": "spotify:track:1"}, "is_playing": true
        })))
        .expect("playing");
        assert!(playing.is_playlist("abc"));
        assert!(!playing.is_playlist("bc"));
        assert!(!playing.is_playlist("abcd"));
        assert_eq!(playing.item_uri.as_deref(), Some("spotify:track:1"));
        assert!(playing.is_playing);

        let old = currently_playing(&bytes(
            json!({"context": {"uri": "spotify:user:julian:playlist:abc"}}),
        ))
        .expect("old context");
        assert!(old.is_playlist("abc"));
        let album = currently_playing(&bytes(
            json!({"context": {"uri": "spotify:album:abc"}, "item": null}),
        ))
        .expect("album");
        assert!(!album.is_playlist("abc"));
        assert_eq!(album.item_uri, None);
        let nothing = currently_playing(b"{}").expect("empty");
        assert_eq!(nothing.context_uri, None);
    }

    #[test]
    fn decodes_top_items() {
        let artists = top_items(&bytes(json!({"items": [
            {"id": "1", "name": "Artist", "uri": "spotify:artist:1", "images": []},
            null,
            {"name": "broken"}
        ]})))
        .expect("artists");
        assert_eq!(
            artists,
            vec![TopItem {
                id: "1".into(),
                name: "Artist".into(),
                uri: "spotify:artist:1".into(),
                artists: vec![],
            }]
        );
        let tracks = top_items(&bytes(json!({"items": [
            {"id": "2", "name": "Song", "uri": "spotify:track:2",
             "artists": [{"name": "A"}, {"name": null}, {"name": "B"}]}
        ]})))
        .expect("tracks");
        assert_eq!(tracks[0].artists, ["A", "B"]);
        assert!(top_items(b"{}").expect("no items").is_empty());
        assert_eq!(TimeRange::Medium.as_param(), "medium_term");
        assert_eq!(
            serde_json::to_value(TimeRange::Long).expect("json"),
            json!("long")
        );
    }

    #[test]
    fn reads_created_ids_and_error_messages() {
        assert_eq!(
            created_id(br#"{"id": "new", "name": "x"}"#).expect("id"),
            "new"
        );
        assert_eq!(
            api_error_message(br#"{"error": {"status": 403, "message": "Premium required"}}"#)
                .as_deref(),
            Some("Premium required")
        );
        assert_eq!(api_error_message(b"not json"), None);
        assert_eq!(
            api_error_message(br#"{"error": {"status": 500, "message": ""}}"#),
            None
        );
    }
}
