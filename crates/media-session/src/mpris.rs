//! Reading players on Linux through MPRIS, the D-Bus interface most Linux media
//! players and all major browsers implement.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use zbus::blocking::fdo::{DBusProxy, PropertiesProxy};
use zbus::blocking::{Connection, Proxy};
use zbus::names::InterfaceName;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

use crate::backend::{Backend, CommandError};
use crate::model::{
    Capabilities, Command, PlaybackStatus, PlayerId, PlayerInfo, PlayerSnapshot, Problem, Reading,
    Service, Track,
};
use crate::normalize::{
    browser_media, identify_mpris, join_artists, spotify_uri_from_mpris, track_key,
};

pub const BUS_PREFIX: &str = "org.mpris.MediaPlayer2.";
pub const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const ROOT_INTERFACE: &str = "org.mpris.MediaPlayer2";
const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";
const NO_TRACK: &str = "/org/mpris/MediaPlayer2/TrackList/NoTrack";

/// Reads every MPRIS player on the session bus. Connects lazily and reconnects
/// after the bus went away.
#[derive(Default)]
pub struct MprisBackend {
    address: Option<String>,
    connection: Option<Connection>,
    last_error: Option<String>,
}

impl MprisBackend {
    /// Uses the session bus of the logged-in user.
    pub fn session() -> Self {
        Self::default()
    }

    /// Uses the bus at a D-Bus address such as `unix:path=/tmp/bus`, for tests.
    pub fn with_address(address: impl Into<String>) -> Self {
        Self {
            address: Some(address.into()),
            ..Self::default()
        }
    }

    fn connection(&mut self) -> zbus::Result<Connection> {
        if let Some(connection) = &self.connection {
            return Ok(connection.clone());
        }
        let connection = match &self.address {
            Some(address) => {
                zbus::blocking::connection::Builder::address(address.as_str())?.build()?
            }
            None => Connection::session()?,
        };
        self.connection = Some(connection.clone());
        Ok(connection)
    }

    fn player_names(connection: &Connection) -> zbus::Result<Vec<String>> {
        let mut names: Vec<String> = DBusProxy::new(connection)?
            .list_names()?
            .into_iter()
            .map(|name| name.to_string())
            .filter(|name| name.starts_with(BUS_PREFIX))
            .collect();
        names.sort();
        Ok(names)
    }
}

impl Backend for MprisBackend {
    fn name(&self) -> &'static str {
        "mpris"
    }

    fn poll(&mut self) -> Vec<Reading> {
        let names = match self
            .connection()
            .and_then(|connection| Self::player_names(&connection).map(|names| (connection, names)))
        {
            Ok((connection, names)) => {
                self.last_error = None;
                return names
                    .iter()
                    .filter_map(|name| read_player(&connection, name))
                    .collect();
            }
            Err(error) => error,
        };
        self.connection = None;
        self.last_error = Some(format!("D-Bus: {names}"));
        Vec::new()
    }

    fn send(&mut self, player: &PlayerId, command: Command) -> Result<(), CommandError> {
        let bus_name = player
            .as_str()
            .strip_prefix("mpris:")
            .ok_or(CommandError::PlayerGone)?;
        let connection = self
            .connection()
            .map_err(|error| CommandError::Failed(error.to_string()))?;
        let failed = |error: zbus::Error| CommandError::Failed(error.to_string());
        let proxy =
            Proxy::new(&connection, bus_name, OBJECT_PATH, PLAYER_INTERFACE).map_err(failed)?;
        let result = match command {
            Command::PlayPause => proxy.call_method("PlayPause", &()),
            Command::Pause => proxy.call_method("Pause", &()),
            Command::Next => proxy.call_method("Next", &()),
            Command::Previous => proxy.call_method("Previous", &()),
            Command::SeekTo { position } => {
                let target = (position.max(0.0) * 1_000_000.0) as i64;
                let properties = player_properties(&connection, bus_name).map_err(failed)?;
                let metadata = metadata(&properties);
                match metadata
                    .get("mpris:trackid")
                    .and_then(|value| as_text(value))
                    .filter(|id| id != NO_TRACK)
                {
                    Some(track_id) => {
                        let path = ObjectPath::try_from(track_id.as_str())
                            .map_err(|error| CommandError::Failed(error.to_string()))?;
                        proxy.call_method("SetPosition", &(path, target))
                    }
                    None => {
                        let current = properties
                            .get("Position")
                            .and_then(|value| as_f64(value))
                            .unwrap_or(0.0) as i64;
                        proxy.call_method("Seek", &(target - current))
                    }
                }
            }
        };
        result.map(|_| ()).map_err(|error| match error {
            zbus::Error::MethodError(name, _, _)
                if name.as_str() == "org.freedesktop.DBus.Error.ServiceUnknown" =>
            {
                CommandError::PlayerGone
            }
            other => failed(other),
        })
    }

    fn poll_interval(&self) -> Duration {
        Duration::from_millis(500)
    }

    fn last_error(&self) -> Option<String> {
        self.last_error.clone()
    }
}

fn player_properties(
    connection: &Connection,
    bus_name: &str,
) -> zbus::Result<HashMap<String, OwnedValue>> {
    Ok(properties_proxy(connection, bus_name)?
        .get_all(InterfaceName::from_static_str_unchecked(PLAYER_INTERFACE))?)
}

fn properties_proxy<'a>(
    connection: &Connection,
    bus_name: &'a str,
) -> zbus::Result<PropertiesProxy<'a>> {
    PropertiesProxy::builder(connection)
        .destination(bus_name)?
        .path(OBJECT_PATH)?
        .build()
}

/// Reads one player. Returns `None` when it vanished in the meantime.
fn read_player(connection: &Connection, bus_name: &str) -> Option<Reading> {
    let proxy = properties_proxy(connection, bus_name).ok()?;
    let root = proxy
        .get_all(InterfaceName::from_static_str_unchecked(ROOT_INTERFACE))
        .unwrap_or_default();
    let identity = root.get("Identity").and_then(|value| as_text(value));

    let properties = match proxy.get_all(InterfaceName::from_static_str_unchecked(PLAYER_INTERFACE))
    {
        Ok(properties) => properties,
        Err(zbus::fdo::Error::ServiceUnknown(_) | zbus::fdo::Error::NameHasNoOwner(_)) => {
            return None
        }
        Err(error) => {
            let (service, name, web) = identify_mpris(bus_name, identity.as_deref(), None);
            return Some(Reading::Unavailable {
                player: PlayerInfo {
                    id: player_id(bus_name),
                    name,
                    service,
                    web,
                },
                problem: Problem::ReadFailed {
                    detail: error.to_string(),
                },
                playing: None,
            });
        }
    };
    let sampled_at = Instant::now();
    Some(Reading::Snapshot(snapshot(
        bus_name,
        identity.as_deref(),
        &properties,
        sampled_at,
    )))
}

fn player_id(bus_name: &str) -> PlayerId {
    PlayerId::new(format!("mpris:{bus_name}"))
}

/// Builds a snapshot from the properties of `org.mpris.MediaPlayer2.Player`.
pub(crate) fn snapshot(
    bus_name: &str,
    identity: Option<&str>,
    properties: &HashMap<String, OwnedValue>,
    sampled_at: Instant,
) -> PlayerSnapshot {
    let metadata = metadata(properties);
    let text = |key: &str| {
        metadata
            .get(key)
            .and_then(|value| as_text(value))
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    };
    let url = text("xesam:url");
    let (service, name, web) = identify_mpris(bus_name, identity, url.as_deref());
    let artists = metadata
        .get("xesam:artist")
        .map(|value| as_texts(value))
        .unwrap_or_default();
    let song = text("xesam:title").map(|title| (title, join_artists(&artists)));
    // A browser tab may be YouTube Music, which only its address or title tells.
    let (service, song) = match song {
        Some((title, artist)) if web => {
            let media = browser_media(url.as_deref(), &title, &artist);
            (media.service(service), Some((media.title, media.artist)))
        }
        song => (service, song),
    };

    let flag = |key: &str| {
        properties
            .get(key)
            .and_then(|value| as_bool(value))
            .unwrap_or(false)
    };
    let status = match properties
        .get("PlaybackStatus")
        .and_then(|value| as_text(value))
        .as_deref()
    {
        Some("Playing") => PlaybackStatus::Playing,
        Some("Paused") => PlaybackStatus::Paused,
        _ => PlaybackStatus::Stopped,
    };
    let controllable = properties
        .get("CanControl")
        .and_then(|value| as_bool(value))
        .unwrap_or(true);
    let capabilities = Capabilities {
        play_pause: controllable && (flag("CanPause") || flag("CanPlay")),
        next: controllable && flag("CanGoNext"),
        previous: controllable && flag("CanGoPrevious"),
        seek: controllable && flag("CanSeek"),
    };

    let track = song
        .filter(|(title, _)| !title.is_empty())
        .map(|(title, artist)| {
            let album = text("xesam:album");
            let track_id = text("mpris:trackid").filter(|id| id != NO_TRACK);
            let source_id = match service {
                Service::Spotify => track_id
                    .as_deref()
                    .and_then(spotify_uri_from_mpris)
                    .or(track_id),
                _ => track_id,
            };
            Track {
                key: track_key(&title, &artist, album.as_deref()),
                source_id,
                duration: metadata
                    .get("mpris:length")
                    .and_then(|value| as_f64(value))
                    .filter(|us| *us > 0.0)
                    .map(|us| us / 1_000_000.0),
                artwork_url: text("mpris:artUrl"),
                title,
                artist,
                album,
            }
        });
    let position = track
        .as_ref()
        .and(properties.get("Position").and_then(|value| as_f64(value)))
        .filter(|us| *us >= 0.0)
        .map(|us| us / 1_000_000.0);
    let rate = properties
        .get("Rate")
        .and_then(|value| as_f64(value))
        .filter(|rate| *rate > 0.0)
        .unwrap_or(1.0);

    PlayerSnapshot {
        player: PlayerInfo {
            id: player_id(bus_name),
            name,
            service,
            web,
        },
        status,
        track,
        position,
        sampled_at,
        rate,
        capabilities,
    }
}

fn metadata(properties: &HashMap<String, OwnedValue>) -> HashMap<String, OwnedValue> {
    properties
        .get("Metadata")
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| HashMap::<String, OwnedValue>::try_from(value).ok())
        .unwrap_or_default()
}

fn as_text(value: &Value<'_>) -> Option<String> {
    match value {
        Value::Str(text) => Some(text.to_string()),
        Value::ObjectPath(path) => Some(path.as_str().to_owned()),
        Value::Value(inner) => as_text(inner),
        _ => None,
    }
}

fn as_texts(value: &Value<'_>) -> Vec<String> {
    match value {
        Value::Array(array) => array.inner().iter().filter_map(as_text).collect(),
        Value::Value(inner) => as_texts(inner),
        other => as_text(other).into_iter().collect(),
    }
}

fn as_f64(value: &Value<'_>) -> Option<f64> {
    match value {
        Value::U8(number) => Some(f64::from(*number)),
        Value::I16(number) => Some(f64::from(*number)),
        Value::U16(number) => Some(f64::from(*number)),
        Value::I32(number) => Some(f64::from(*number)),
        Value::U32(number) => Some(f64::from(*number)),
        Value::I64(number) => Some(*number as f64),
        Value::U64(number) => Some(*number as f64),
        Value::F64(number) => Some(*number).filter(|n| n.is_finite()),
        Value::Value(inner) => as_f64(inner),
        _ => None,
    }
}

fn as_bool(value: &Value<'_>) -> Option<bool> {
    match value {
        Value::Bool(flag) => Some(*flag),
        Value::Value(inner) => as_bool(inner),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(value: Value<'_>) -> OwnedValue {
        OwnedValue::try_from(value).expect("plain values convert")
    }

    fn properties(entries: Vec<(&str, OwnedValue)>) -> HashMap<String, OwnedValue> {
        entries
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect()
    }

    fn spotify_metadata() -> OwnedValue {
        let metadata: HashMap<String, OwnedValue> = [
            (
                "mpris:trackid",
                owned(Value::from(
                    ObjectPath::try_from("/com/spotify/track/abc").expect("valid path"),
                )),
            ),
            ("xesam:title", owned(Value::from("Song"))),
            ("xesam:artist", owned(Value::from(vec!["One", "Two"]))),
            ("xesam:album", owned(Value::from("Album"))),
            ("mpris:length", owned(Value::from(215_000_000_u64))),
            (
                "mpris:artUrl",
                owned(Value::from("https://i.scdn.co/image/x")),
            ),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect();
        owned(Value::from(metadata))
    }

    #[test]
    fn reads_spotify_metadata() {
        let props = properties(vec![
            ("PlaybackStatus", owned(Value::from("Playing"))),
            ("Metadata", spotify_metadata()),
            ("Position", owned(Value::from(12_500_000_i64))),
            ("Rate", owned(Value::from(1.0))),
            ("CanPause", owned(Value::from(true))),
            ("CanGoNext", owned(Value::from(true))),
            ("CanSeek", owned(Value::from(false))),
        ]);
        let snapshot = snapshot(
            "org.mpris.MediaPlayer2.spotify",
            Some("Spotify"),
            &props,
            Instant::now(),
        );
        assert_eq!(snapshot.player.service, Service::Spotify);
        assert_eq!(snapshot.status, PlaybackStatus::Playing);
        assert_eq!(snapshot.position, Some(12.5));
        let track = snapshot.track.expect("track");
        assert_eq!(track.artist, "One, Two");
        assert_eq!(track.duration, Some(215.0));
        assert_eq!(track.source_id.as_deref(), Some("spotify:track:abc"));
        assert_eq!(
            snapshot.capabilities,
            Capabilities {
                play_pause: true,
                next: true,
                previous: false,
                seek: false
            }
        );
    }

    #[test]
    fn a_player_without_metadata_has_no_track_or_position() {
        let props = properties(vec![
            ("PlaybackStatus", owned(Value::from("Stopped"))),
            ("Position", owned(Value::from(0_i64))),
            ("CanControl", owned(Value::from(false))),
            ("CanPause", owned(Value::from(true))),
        ]);
        let snapshot = snapshot("org.mpris.MediaPlayer2.vlc", None, &props, Instant::now());
        assert_eq!(snapshot.status, PlaybackStatus::Stopped);
        assert_eq!((snapshot.track, snapshot.position), (None, None));
        assert_eq!(
            snapshot.capabilities,
            Capabilities::default(),
            "CanControl false disables everything"
        );
        assert_eq!(snapshot.player.name, "Vlc");
    }

    #[test]
    fn nested_variants_are_unwrapped() {
        let nested = owned(Value::Value(Box::new(Value::from("Paused"))));
        let props = properties(vec![("PlaybackStatus", nested)]);
        assert_eq!(
            snapshot("org.mpris.MediaPlayer2.x", None, &props, Instant::now()).status,
            PlaybackStatus::Paused
        );
    }

    fn browser_tab(title: &str, artist: &str, url: Option<&str>) -> HashMap<String, OwnedValue> {
        let mut metadata: HashMap<String, OwnedValue> = [
            ("xesam:title", owned(Value::from(title))),
            ("xesam:artist", owned(Value::from(vec![artist]))),
            ("mpris:length", owned(Value::from(200_000_000_i64))),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect();
        if let Some(url) = url {
            metadata.insert("xesam:url".into(), owned(Value::from(url)));
        }
        properties(vec![
            ("PlaybackStatus", owned(Value::from("Playing"))),
            ("Metadata", owned(Value::from(metadata))),
            ("Position", owned(Value::from(1_000_000_i64))),
        ])
    }

    #[test]
    fn youtube_music_tabs_are_recognized() {
        let by_url = snapshot(
            "org.mpris.MediaPlayer2.chromium.instance42",
            Some("Chromium"),
            &browser_tab(
                "Halo",
                "Beyoncé",
                Some("https://music.youtube.com/watch?v=1"),
            ),
            Instant::now(),
        );
        assert_eq!(by_url.player.service, Service::YouTubeMusic);
        assert!(by_url.player.web);
        assert_eq!(by_url.track.expect("track").title, "Halo");

        let by_title = snapshot(
            "org.mpris.MediaPlayer2.firefox.instance_1_7",
            Some("Mozilla Firefox"),
            &browser_tab("Halo - YouTube Music", "", None),
            Instant::now(),
        );
        assert_eq!(by_title.player.service, Service::YouTubeMusic);
        let track = by_title.track.expect("track");
        assert_eq!(track.title, "Halo");
        assert_eq!(track.key, track_key("Halo", "", None));

        let other = snapshot(
            "org.mpris.MediaPlayer2.vlc",
            Some("VLC media player"),
            &browser_tab("Clip - YouTube Music", "", None),
            Instant::now(),
        );
        assert_eq!(
            (
                other.player.service,
                other.track.expect("track").title.as_str()
            ),
            (Service::Other, "Clip - YouTube Music"),
            "only browser tabs are looked at"
        );
    }
}
