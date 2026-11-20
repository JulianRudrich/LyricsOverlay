//! Your own lyrics for a song: the editor window, importing an LRC file and
//! removing them again. The song database checks and keeps the text; the
//! overlay reloads at once when the lyrics of the playing song change.

use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::{mpsc, Mutex};
use std::time::Instant;

use lyrics_core::lrc;
use lyrics_providers::{ImportError, MAX_LYRICS_BYTES};
use media_session::Track;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::i18n::{resolve, Texts};
use crate::playback::{PlaybackView, Request};
use crate::{windows, AppState};

/// The file types the import offers. GTK compares extensions case-sensitively,
/// so the upper-case spellings are listed as well.
const LYRICS_EXTENSIONS: [&str; 4] = ["lrc", "txt", "LRC", "TXT"];

/// The byte-order mark some editors put at the start of a UTF-8 file.
const BYTE_ORDER_MARK: &[u8] = b"\xEF\xBB\xBF";

/// Why your own lyrics could not be opened, imported, saved or removed. The web
/// interface shows a message for each kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OwnLyricsError {
    /// No song plays, or the demo does, whose sample lyrics stay as they are.
    NoTrack,
    /// The lyrics are larger than 512 KB.
    TooLarge,
    /// The file is not UTF-8 text.
    NotText,
    /// Not a single line has both an LRC timestamp and text.
    MissingTimestamps,
    /// The file could not be read.
    Unreadable,
    /// The song database could not keep the change.
    Storage,
}

impl From<ImportError> for OwnLyricsError {
    fn from(error: ImportError) -> Self {
        match error {
            ImportError::MissingTimedLyrics => Self::MissingTimestamps,
            ImportError::TooLarge => Self::TooLarge,
            ImportError::Storage(_) => Self::Storage,
        }
    }
}

/// The song the editor window belongs to. It is fixed when the window opens,
/// so a song change neither moves the draft to the next song nor stamps it
/// with that song's time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorTrack {
    pub track_key: String,
    pub title: String,
    pub artist: String,
}

impl EditorTrack {
    fn new(track: &Track) -> Self {
        Self {
            track_key: track.key.clone(),
            title: track.title.clone(),
            artist: track.artist.clone(),
        }
    }
}

/// Remembers the editor's song while its window is open.
#[derive(Default)]
pub struct EditorSession(Mutex<Option<EditorTrack>>);

impl EditorSession {
    fn track(&self) -> Option<EditorTrack> {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    fn begin(&self, track: EditorTrack) {
        *self.0.lock().unwrap_or_else(|p| p.into_inner()) = Some(track);
    }
}

/// What the editor shows when it opens.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorState {
    #[serde(flatten)]
    pub track: EditorTrack,
    /// Your own lyrics for the song as LRC text; `None` before you add some.
    pub lrc: Option<String>,
}

/// One edit of the draft, counted in UTF-16 code units like the caret of a
/// text field. See [`lrc::TimestampEdit`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StampEdit {
    pub start: usize,
    pub length: usize,
    pub replacement: String,
    pub next_caret: usize,
}

impl From<lrc::TimestampEdit> for StampEdit {
    fn from(edit: lrc::TimestampEdit) -> Self {
        Self {
            start: edit.start,
            length: edit.length,
            replacement: edit.replacement,
            next_caret: edit.next_caret,
        }
    }
}

/// Why a line was not stamped. The editor shows a hint for each kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StampError {
    /// The editor's song is not the one that plays now.
    OtherTrack,
    /// The player does not report a position.
    NoPosition,
    /// The caret is on a tag line such as `[ar:Artist]`.
    NotALyricLine,
}

/// Stamps the line at `caret` with the lyric position the listener hears at
/// `now`: the player's position moved on since its last reading, plus `offset`,
/// the global and the per-song correction together. Refuses while another song
/// than the editor's plays.
pub fn stamp(
    draft: &str,
    caret: usize,
    track_key: &str,
    playback: &PlaybackView,
    offset: f64,
    now: Instant,
) -> Result<StampEdit, StampError> {
    let playing = playback.track.as_ref().map(|track| track.key.as_str());
    if playing != Some(track_key) {
        return Err(StampError::OtherTrack);
    }
    let position = playback.position_at(now).ok_or(StampError::NoPosition)?;
    lrc::timestamp_edit(draft, caret, position + offset)
        .map(StampEdit::from)
        .ok_or(StampError::NotALyricLine)
}

/// Turns the bytes of a lyrics file into text: UTF-8 of at most 512 KB, without
/// a leading byte-order mark.
pub fn decode_lyrics(bytes: &[u8]) -> Result<String, OwnLyricsError> {
    let bytes = bytes.strip_prefix(BYTE_ORDER_MARK).unwrap_or(bytes);
    if bytes.len() > MAX_LYRICS_BYTES {
        return Err(OwnLyricsError::TooLarge);
    }
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| OwnLyricsError::NotText)
}

/// Reads a lyrics file, never more than one byte beyond the limit.
fn read_lyrics_file(path: &Path) -> Result<String, OwnLyricsError> {
    let limit = (MAX_LYRICS_BYTES + BYTE_ORDER_MARK.len() + 1) as u64;
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| file.take(limit).read_to_end(&mut bytes))
        .map_err(|error| {
            // The path often names the song and the listener, so it stays out.
            log::warn!("could not read a lyrics file: {error}");
            OwnLyricsError::Unreadable
        })?;
    decode_lyrics(&bytes)
}

/// The song that plays now, unless it is the demo, whose lyrics stay fixed.
fn playing_track(state: &AppState) -> Option<Track> {
    if state.demo {
        return None;
    }
    state
        .playback
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .track
        .clone()
}

/// The per-song correction: from the lyrics on show, or from the song database
/// while those still belong to the previous song.
fn song_offset(state: &AppState, track_key: &str) -> f64 {
    let shown = {
        let view = state.lyrics_view.lock().unwrap_or_else(|p| p.into_inner());
        (view.track_key.as_deref() == Some(track_key)).then_some(view.song_offset)
    };
    shown.unwrap_or_else(|| state.store().timing_offset(track_key))
}

/// Shows changed own lyrics at once when they belong to the song that plays.
fn reload_if_playing(state: &AppState, track_key: &str) {
    let playing = state
        .playback
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .track
        .as_ref()
        .is_some_and(|track| track.key == track_key);
    if playing {
        state.request(Request::ReloadLyrics { refresh: false });
    }
}

/// Saves `text` as the song's own lyrics. Invalid text keeps what it had.
fn store_lyrics(state: &AppState, track_key: &str, text: &str) -> Result<(), OwnLyricsError> {
    if state.demo || track_key.is_empty() {
        return Err(OwnLyricsError::NoTrack);
    }
    state
        .store()
        .import_lyrics(track_key, text)
        .map_err(|error| {
            if let ImportError::Storage(detail) = &error {
                log::error!("could not save your own lyrics: {detail}");
            }
            OwnLyricsError::from(error)
        })?;
    reload_if_playing(state, track_key);
    Ok(())
}

/// Opens the editor for the song that plays now; an open editor keeps its song
/// and comes to the front. Async, because building a window inside a
/// synchronous command deadlocks on Windows.
#[tauri::command]
pub async fn open_editor(app: AppHandle) -> Result<(), OwnLyricsError> {
    if app.get_webview_window(windows::EDITOR).is_none() {
        let track = playing_track(&app.state::<AppState>()).ok_or(OwnLyricsError::NoTrack)?;
        app.state::<EditorSession>().begin(EditorTrack::new(&track));
    }
    windows::open_editor(&app);
    Ok(())
}

#[tauri::command]
pub fn close_editor(app: AppHandle) {
    windows::close_editor(&app);
}

#[tauri::command]
pub fn editor_state(
    state: State<'_, AppState>,
    session: State<'_, EditorSession>,
) -> Option<EditorState> {
    let track = session.track()?;
    let lrc = state.store().local_lyrics(&track.track_key);
    Some(EditorState { track, lrc })
}

/// Stamps the line at `caret` (UTF-16) with the time the listener hears now.
#[tauri::command]
pub fn stamp_line(
    state: State<'_, AppState>,
    session: State<'_, EditorSession>,
    draft: String,
    caret: usize,
) -> Result<StampEdit, StampError> {
    let now = Instant::now();
    let track = session.track().ok_or(StampError::OtherTrack)?;
    let playback = state
        .playback
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    let offset = state.settings().global_offset + song_offset(&state, &track.track_key);
    stamp(&draft, caret, &track.track_key, &playback, offset, now)
}

#[tauri::command]
pub async fn save_local_lyrics(
    app: AppHandle,
    track_key: String,
    text: String,
) -> Result<(), OwnLyricsError> {
    store_lyrics(&app.state::<AppState>(), &track_key, &text)
}

/// Lets the listener pick an LRC file and saves it as the song's own lyrics.
/// `title` names the song in the file picker. Answers `false` when the
/// listener cancels.
#[tauri::command]
pub async fn import_lyrics_file(
    app: AppHandle,
    track_key: String,
    title: String,
) -> Result<bool, OwnLyricsError> {
    let language = {
        let state = app.state::<AppState>();
        if state.demo || track_key.is_empty() {
            return Err(OwnLyricsError::NoTrack);
        }
        state.settings().language
    };
    let texts = Texts::new(resolve(language));
    let mut picker = app
        .dialog()
        .file()
        .set_title(
            texts
                .get("ownLyrics.importTitle")
                .replace("{title}", &title),
        )
        .add_filter(texts.get("ownLyrics.fileType"), &LYRICS_EXTENSIONS);
    if let Some(settings) = app.get_webview_window(windows::SETTINGS) {
        picker = picker.set_parent(&settings);
    }
    // The picker runs on the main thread and answers on another one, so
    // nothing waits on the main thread.
    let (sender, receiver) = mpsc::sync_channel(1);
    picker.pick_file(move |file| {
        let _ = sender.send(file);
    });
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let Ok(Some(file)) = receiver.recv() else {
            return Ok(None);
        };
        let path = file.into_path().map_err(|_| OwnLyricsError::Unreadable)?;
        read_lyrics_file(&path).map(Some)
    })
    .await
    .map_err(|_| OwnLyricsError::Unreadable)??;

    match picked {
        Some(text) => store_lyrics(&app.state::<AppState>(), &track_key, &text).map(|()| true),
        None => Ok(false),
    }
}

#[tauri::command]
pub async fn remove_local_lyrics(app: AppHandle, track_key: String) -> Result<(), OwnLyricsError> {
    let state = app.state::<AppState>();
    if state.demo || track_key.is_empty() {
        return Err(OwnLyricsError::NoTrack);
    }
    state
        .store()
        .remove_local_lyrics(&track_key)
        .map_err(|error| {
            log::error!("could not remove your own lyrics: {error}");
            OwnLyricsError::Storage
        })?;
    reload_if_playing(&state, &track_key);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn playing(track_key: &str, position: Option<f64>, playing: bool) -> PlaybackView {
        PlaybackView {
            state: "track",
            track: Some(Track {
                key: track_key.to_owned(),
                source_id: None,
                title: "Paper Lanterns".into(),
                artist: "Example Artist".into(),
                album: None,
                duration: Some(200.0),
                artwork_url: None,
            }),
            playing,
            position,
            ..PlaybackView::idle()
        }
    }

    #[test]
    fn stamps_the_position_the_listener_hears() {
        let view = playing("song", Some(10.0), true);
        let later = view.sampled_at + Duration::from_millis(1_500);
        let edit = stamp("First\nSecond", 0, "song", &view, 0.5, later).expect("stamped");
        assert_eq!(
            edit,
            StampEdit {
                start: 0,
                length: 0,
                replacement: "[00:12.00]".into(),
                next_caret: 16,
            }
        );
    }

    #[test]
    fn a_paused_song_keeps_its_position() {
        let view = playing("song", Some(61.25), false);
        let later = view.sampled_at + Duration::from_secs(3);
        let edit = stamp("[00:01.00] Line", 3, "song", &view, 0.0, later).expect("stamped");
        assert_eq!((edit.start, edit.length), (0, 10), "replaces the old stamp");
        assert_eq!(edit.replacement, "[01:01.25]");
    }

    #[test]
    fn the_position_moves_at_the_player_rate_and_stops_at_the_end() {
        let mut view = playing("song", Some(10.0), true);
        view.rate = 2.0;
        let later = view.sampled_at + Duration::from_secs(2);
        assert_eq!(view.position_at(later), Some(14.0));
        let much_later = view.sampled_at + Duration::from_secs(500);
        assert_eq!(view.position_at(much_later), Some(200.0));
    }

    #[test]
    fn refuses_while_another_song_or_nothing_plays() {
        let now = Instant::now();
        let other = playing("other", Some(5.0), true);
        assert_eq!(
            stamp("Line", 0, "song", &other, 0.0, now),
            Err(StampError::OtherTrack)
        );
        assert_eq!(
            stamp("Line", 0, "song", &PlaybackView::idle(), 0.0, now),
            Err(StampError::OtherTrack)
        );
    }

    #[test]
    fn needs_a_position_and_a_lyric_line() {
        let now = Instant::now();
        let unknown = playing("song", None, true);
        assert_eq!(
            stamp("Line", 0, "song", &unknown, 0.0, now),
            Err(StampError::NoPosition)
        );
        let view = playing("song", Some(5.0), true);
        assert_eq!(
            stamp("[ar:Example Artist]", 2, "song", &view, 0.0, now),
            Err(StampError::NotALyricLine)
        );
    }

    #[test]
    fn decodes_utf8_up_to_the_limit() {
        assert_eq!(
            decode_lyrics(b"\xEF\xBB\xBF[00:01.00] Hall\xC3\xB6"),
            Ok("[00:01.00] Hallö".to_owned())
        );
        assert_eq!(
            decode_lyrics(b"[00:01.00] \xFF"),
            Err(OwnLyricsError::NotText)
        );

        let mut largest = vec![b'a'; MAX_LYRICS_BYTES];
        assert_eq!(
            decode_lyrics(&largest).map(|text| text.len()),
            Ok(MAX_LYRICS_BYTES)
        );
        largest.push(b'a');
        assert_eq!(decode_lyrics(&largest), Err(OwnLyricsError::TooLarge));
    }

    #[test]
    fn reads_files_without_loading_more_than_the_limit() {
        let directory =
            std::env::temp_dir().join(format!("singline-own-lyrics-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("created");
        let small = directory.join("song.lrc");
        std::fs::write(&small, "[00:01.00] One\n").expect("written");
        assert_eq!(read_lyrics_file(&small), Ok("[00:01.00] One\n".to_owned()));

        let large = directory.join("large.lrc");
        std::fs::write(&large, vec![b'a'; MAX_LYRICS_BYTES + 10]).expect("written");
        assert_eq!(read_lyrics_file(&large), Err(OwnLyricsError::TooLarge));

        assert_eq!(
            read_lyrics_file(&directory.join("missing.lrc")),
            Err(OwnLyricsError::Unreadable)
        );
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn errors_reach_the_web_interface_by_name() {
        let names = serde_json::json!([
            OwnLyricsError::NoTrack,
            OwnLyricsError::TooLarge,
            OwnLyricsError::NotText,
            OwnLyricsError::MissingTimestamps,
            OwnLyricsError::Unreadable,
            OwnLyricsError::Storage,
            StampError::OtherTrack,
            StampError::NoPosition,
            StampError::NotALyricLine,
        ]);
        assert_eq!(
            names,
            serde_json::json!([
                "noTrack",
                "tooLarge",
                "notText",
                "missingTimestamps",
                "unreadable",
                "storage",
                "otherTrack",
                "noPosition",
                "notALyricLine"
            ])
        );
        assert_eq!(
            OwnLyricsError::from(ImportError::MissingTimedLyrics),
            OwnLyricsError::MissingTimestamps
        );
    }
}
