//! True Shuffle in the app: picks the source, writes an honest order to the
//! one "Singline Shuffle" playlist, starts it on this computer's Spotify and
//! follows what plays. The rules live in `spotify_connect::shuffle`.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use media_session::Command;
use serde::Serialize;
use spotify_connect::model::preferred_device;
use spotify_connect::shuffle::{
    fresh_rng, merge_sources, mix_members, NextRound, ShuffleState, UP_NEXT,
};
use spotify_connect::{Playlist, SpotifyClient, SpotifyError, TrackInfo, UpNext, LIKED_SONGS};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::i18n::{resolve, Texts};
use crate::playback::Request;
use crate::spotify::{self, Spotify};
use crate::AppState;

/// The event that carries [`View`] to the shuffle window.
pub const EVENT: &str = "shuffle";
/// Spotify can report a track change a moment after the player does.
const PLAYBACK_CHECKS: usize = 3;
/// A Spotify app that just started appears as a device a few seconds later.
const DEVICE_ATTEMPTS: usize = 8;
const KEPT_NAME_LIMIT: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    /// `None` for Liked Songs, which the interface names.
    pub name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Activity {
    Loading {
        loaded: usize,
        total: usize,
    },
    Writing {
        written: usize,
        total: usize,
    },
    Starting,
    Working,
    /// "Keep playing" is adding the next round.
    Continuing,
}

/// One line under the Shuffle & play button.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Notice {
    Empty,
    Full,
    NoDevice,
    Kept { name: String },
    Auth,
    Offline,
    Premium,
    Http { status: u16 },
    Failed { detail: String },
}

impl Notice {
    fn from_error(error: &SpotifyError) -> Self {
        match error {
            SpotifyError::LoginExpired | SpotifyError::NotConnected => Notice::Auth,
            SpotifyError::Offline | SpotifyError::Network(_) => Notice::Offline,
            SpotifyError::Forbidden(_) => Notice::Premium,
            SpotifyError::Http(status) => Notice::Http { status: *status },
            other => Notice::Failed {
                detail: other.to_string(),
            },
        }
    }
}

/// What the shuffle window shows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub connected: bool,
    pub sources: Vec<Source>,
    pub sources_loaded: bool,
    /// Mixes shuffled before whose sources all exist.
    pub mixes: Vec<String>,
    pub selected: String,
    pub members: Vec<String>,
    pub played: usize,
    pub total: usize,
    pub removed: usize,
    pub keep_playing: bool,
    pub activity: Option<Activity>,
    pub notice: Option<Notice>,
    pub has_shuffle: bool,
    pub active_source: Option<String>,
    /// The playing track was confirmed to play from the shuffle playlist.
    pub in_shuffle: bool,
    pub up_next: Vec<UpNext>,
}

#[derive(Default)]
struct Live {
    sources: Vec<Source>,
    sources_loaded: bool,
    activity: Option<Activity>,
    notice: Option<Notice>,
    current: Option<String>,
    in_shuffle: bool,
}

pub struct Shuffle {
    /// `None` in the demo, which never changes the listener's files.
    path: Option<PathBuf>,
    state: Mutex<ShuffleState>,
    live: Mutex<Live>,
}

impl Shuffle {
    pub fn new(demo: bool, data_dir: Option<&Path>) -> Self {
        let path = data_dir
            .filter(|_| !demo)
            .map(|directory| directory.join("shuffle-state.json"));
        let state = path
            .as_deref()
            .and_then(|path| fs::read_to_string(path).ok())
            .map(|text| ShuffleState::from_json(&text))
            .unwrap_or_default();
        Self {
            path,
            state: Mutex::new(state),
            live: Mutex::new(Live {
                sources: vec![liked()],
                ..Live::default()
            }),
        }
    }

    fn state(&self) -> MutexGuard<'_, ShuffleState> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn live(&self) -> MutexGuard<'_, Live> {
        self.live.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Writes the progress atomically, so a crash never leaves half a file.
    fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        let written = self
            .state()
            .to_json()
            .map_err(std::io::Error::other)
            .and_then(|text| {
                if let Some(directory) = path.parent() {
                    fs::create_dir_all(directory)?;
                }
                let temporary = path.with_extension("json.tmp");
                fs::write(&temporary, text)?;
                fs::rename(&temporary, path)
            });
        if let Err(error) = written {
            log::error!("could not save the shuffle progress: {error}");
        }
    }

    fn view(&self, connected: bool) -> View {
        let state = self.state();
        let live = self.live();
        let available: HashSet<&str> = live
            .sources
            .iter()
            .map(|source| source.id.as_str())
            .collect();
        let selected = state.selected_source().to_owned();
        let (played, total) = state.progress(&selected);
        View {
            connected,
            sources: live.sources.clone(),
            sources_loaded: live.sources_loaded,
            mixes: state
                .mixes(&available)
                .into_iter()
                .map(str::to_owned)
                .collect(),
            members: state
                .selected_members()
                .into_iter()
                .map(str::to_owned)
                .collect(),
            played,
            total,
            removed: state.removed_count(&selected),
            selected,
            keep_playing: state.keep_playing,
            activity: live.activity,
            notice: live.notice.clone(),
            has_shuffle: state.has_shuffle(),
            active_source: state.active_source.clone(),
            in_shuffle: live.in_shuffle,
            up_next: if live.in_shuffle {
                state.up_next(UP_NEXT)
            } else {
                Vec::new()
            },
        }
    }
}

fn liked() -> Source {
    Source {
        id: LIKED_SONGS.to_owned(),
        name: None,
    }
}

fn view(app: &AppHandle) -> View {
    let connected = app.state::<Spotify>().client().is_some();
    app.state::<Shuffle>().view(connected)
}

fn emit(app: &AppHandle) {
    let _ = app.emit(EVENT, view(app));
}

/// Starts an action unless another one runs. Clears the last notice.
fn begin(app: &AppHandle, activity: Activity) -> bool {
    {
        let shuffle = app.state::<Shuffle>();
        let mut live = shuffle.live();
        if live.activity.is_some() {
            return false;
        }
        live.activity = Some(activity);
        live.notice = None;
    }
    emit(app);
    true
}

fn set_activity(app: &AppHandle, activity: Activity) {
    app.state::<Shuffle>().live().activity = Some(activity);
    emit(app);
}

/// Ends the running action, with a notice or none.
fn finish(app: &AppHandle, notice: Option<Notice>) {
    {
        let shuffle = app.state::<Shuffle>();
        let mut live = shuffle.live();
        live.activity = None;
        if notice.is_some() {
            live.notice = notice;
        }
    }
    emit(app);
}

fn fail(app: &AppHandle, error: &SpotifyError) {
    log::warn!("True Shuffle: {error}");
    spotify::note_error(app, error);
    finish(app, Some(Notice::from_error(error)));
}

fn connected_client(app: &AppHandle) -> Option<(std::sync::Arc<SpotifyClient>, String)> {
    let spotify = app.state::<Spotify>();
    Some((spotify.client()?, spotify.account_id()?))
}

/// Shows the playlists the listener owns or collaborates on: the only ones a
/// Spotify app in development mode may read. The shuffle playlist itself is
/// left out.
fn apply_sources(app: &AppHandle, playlists: &[Playlist], user_id: &str) {
    let shuffle = app.state::<Shuffle>();
    let changed = {
        let mut state = shuffle.state();
        let own = state.playlist_id.clone();
        let mut sources: Vec<Source> = playlists
            .iter()
            .filter(|playlist| playlist.readable_by(user_id) && Some(&playlist.id) != own.as_ref())
            .map(|playlist| Source {
                id: playlist.id.clone(),
                name: Some(playlist.name.clone()),
            })
            .collect();
        sources.sort_by_key(|source| source.name.clone().unwrap_or_default().to_lowercase());
        sources.dedup_by(|a, b| a.id == b.id);
        sources.insert(0, liked());
        let available: HashSet<&str> = sources.iter().map(|source| source.id.as_str()).collect();
        let changed = state.keep_selection_available(&available);
        let mut live = shuffle.live();
        live.sources = sources;
        live.sources_loaded = true;
        changed
    };
    if changed {
        shuffle.save();
    }
}

/// The tracks of a source, or of every source in a mix, each song once.
async fn load_tracks(
    app: &AppHandle,
    client: &SpotifyClient,
    source: &str,
) -> Result<Vec<TrackInfo>, SpotifyError> {
    let mut lists = Vec::new();
    for member in mix_members(source) {
        let progress = app.clone();
        lists.push(
            client
                .tracks(member, move |loaded, total| {
                    set_activity(&progress, Activity::Loading { loaded, total });
                })
                .await?,
        );
    }
    Ok(merge_sources(lists))
}

fn this_computer() -> Option<String> {
    std::env::var("COMPUTERNAME")
        .ok()
        .or_else(|| std::env::var("HOSTNAME").ok())
        .or_else(|| fs::read_to_string("/etc/hostname").ok())
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
}

/// Reuses the shuffle playlist while it exists; one deleted in Spotify is
/// made again.
async fn write_playlist(
    app: &AppHandle,
    client: &SpotifyClient,
    order: &[String],
    existing: &HashSet<String>,
) -> Result<String, SpotifyError> {
    let total = order.len();
    let progress =
        |app: AppHandle| move |written| set_activity(&app, Activity::Writing { written, total });
    let shuffle = app.state::<Shuffle>();
    let reusable = shuffle
        .state()
        .playlist_id
        .clone()
        .filter(|id| existing.contains(id));
    if let Some(id) = reusable {
        match client
            .replace_items(&id, order, progress(app.clone()))
            .await
        {
            Ok(()) => return Ok(id),
            Err(SpotifyError::NotFound) => {}
            Err(error) => return Err(error),
        }
    }
    let texts = Texts::new(resolve(app.state::<AppState>().settings().language));
    let id = client
        .create_playlist(
            &texts.get("shuffle.playlistName"),
            &texts.get("shuffle.playlistDescription"),
        )
        .await?;
    // Stored before writing, so a failed write does not make a second playlist.
    shuffle.state().playlist_id = Some(id.clone());
    shuffle.save();
    client
        .replace_items(&id, order, progress(app.clone()))
        .await?;
    Ok(id)
}

/// Plays the playlist on this computer's Spotify through the Web API. Opens
/// Spotify first when it is not running.
async fn start_playback(client: &SpotifyClient, playlist_id: &str) -> Result<bool, SpotifyError> {
    let name = this_computer();
    for attempt in 0..DEVICE_ATTEMPTS {
        let devices = client.devices().await?;
        if let Some(device) = preferred_device(&devices, name.as_deref()) {
            if let Some(id) = &device.id {
                client.play_playlist(playlist_id, id).await?;
                return Ok(true);
            }
        }
        if attempt == 0 {
            // Starts Spotify on this computer, which then shows up as a device.
            let _ = spotify::open_external(format!("spotify:playlist:{playlist_id}")).await;
        }
        tokio::time::sleep(Duration::from_millis(1_500)).await;
    }
    Ok(false)
}

async fn shuffle_and_play(app: AppHandle) {
    let Some((client, user_id)) = connected_client(&app) else {
        return finish(&app, Some(Notice::Auth));
    };
    let shuffle = app.state::<Shuffle>();
    // The old order stops counting while it is replaced.
    let (source, previous) = {
        let mut state = shuffle.state();
        (
            state.selected_source().to_owned(),
            state.active_source.take(),
        )
    };
    let restore_previous = || {
        let mut state = shuffle.state();
        if state.active_source.is_none() {
            state.active_source.clone_from(&previous);
        }
    };
    let playlists = match client.playlists().await {
        Ok(playlists) => playlists,
        Err(error) => {
            restore_previous();
            return fail(&app, &error);
        }
    };
    apply_sources(&app, &playlists, &user_id);
    let tracks = match load_tracks(&app, &client, &source).await {
        Ok(tracks) => tracks,
        Err(error) => {
            restore_previous();
            return fail(&app, &error);
        }
    };
    let plan = shuffle.state().plan(&source, &tracks, &mut fresh_rng());
    let Some(plan) = plan else {
        restore_previous();
        return finish(&app, Some(Notice::Empty));
    };
    set_activity(
        &app,
        Activity::Writing {
            written: 0,
            total: plan.order.len(),
        },
    );
    let existing: HashSet<String> = playlists.into_iter().map(|playlist| playlist.id).collect();
    let playlist_id = match write_playlist(&app, &client, &plan.order, &existing).await {
        Ok(id) => id,
        Err(error) => {
            restore_previous();
            return fail(&app, &error);
        }
    };
    shuffle.state().commit(plan);
    shuffle.save();
    set_activity(&app, Activity::Starting);
    match start_playback(&client, &playlist_id).await {
        Ok(true) => finish(&app, None),
        Ok(false) => finish(&app, Some(Notice::NoDevice)),
        Err(error) => fail(&app, &error),
    }
}

/// Follows the player: a track counts as played only while Spotify plays
/// it from the shuffle playlist.
pub fn track_changed(app: &AppHandle, uri: Option<&str>) {
    let Some(shuffle) = app.try_state::<Shuffle>() else {
        return;
    };
    {
        let mut live = shuffle.live();
        let was_in_shuffle = live.in_shuffle;
        live.current = uri.map(str::to_owned);
        live.in_shuffle = false;
        if !was_in_shuffle && uri.is_none() {
            return;
        }
    }
    emit(app);
    let Some(uri) = uri
        .filter(|uri| spotify::is_track_uri(uri))
        .map(str::to_owned)
    else {
        return;
    };
    let (source, playlist_id) = {
        let state = shuffle.state();
        if !state.order.contains(&uri) {
            return;
        }
        match (state.active_source.clone(), state.playlist_id.clone()) {
            (Some(source), Some(playlist_id)) => (source, playlist_id),
            _ => return,
        }
    };
    let Some(client) = app.state::<Spotify>().client() else {
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let shuffle = app.state::<Shuffle>();
        let still_current = || shuffle.live().current.as_deref() == Some(uri.as_str());
        for _ in 0..PLAYBACK_CHECKS {
            if !still_current() {
                return;
            }
            let playing = match client.currently_playing().await {
                Ok(Some(playing)) => playing,
                Ok(None) => return,
                Err(error) => return spotify::note_error(&app, &error),
            };
            if playing.item_uri.as_deref() != Some(uri.as_str()) {
                tokio::time::sleep(Duration::from_millis(1_500)).await;
                continue;
            }
            if !still_current() || !playing.is_playlist(&playlist_id) {
                return;
            }
            let next_round = {
                let mut state = shuffle.state();
                if state.active_source.as_deref() != Some(source.as_str()) {
                    return;
                }
                let index = state.record_play(&uri);
                index.filter(|index| state.keep_playing && state.needs_next_round(*index))
            };
            shuffle.live().in_shuffle = true;
            shuffle.save();
            emit(&app);
            if let Some(index) = next_round {
                continue_round(&app, &client, &source, &playlist_id, index).await;
            }
            return;
        }
    });
}

/// "Keep playing": appends the next round while the last tracks play.
async fn continue_round(
    app: &AppHandle,
    client: &SpotifyClient,
    source: &str,
    playlist_id: &str,
    index: usize,
) {
    if !begin(app, Activity::Continuing) {
        return;
    }
    let tracks = match load_tracks(app, client, source).await {
        Ok(tracks) => tracks,
        Err(error) => return fail(app, &error),
    };
    set_activity(app, Activity::Continuing);
    let shuffle = app.state::<Shuffle>();
    let next = shuffle
        .state()
        .plan_next_round(index, &tracks, &mut fresh_rng());
    match next {
        NextRound::Append(plan) => {
            if let Err(error) = client.append_items(playlist_id, &plan.order).await {
                return fail(app, &error);
            }
            if shuffle.state().append(plan) {
                shuffle.save();
            }
            finish(app, None);
        }
        NextRound::Full => finish(app, Some(Notice::Full)),
        NextRound::Nothing => finish(app, None),
    }
}

/// Opens the True Shuffle window, for example from Settings. Async, because
/// building a window in a synchronous command can deadlock on Windows.
#[tauri::command]
pub async fn shuffle_open_window(app: AppHandle) {
    crate::windows::open_shuffle(&app);
}

#[tauri::command]
pub fn shuffle_view(app: AppHandle) -> View {
    view(&app)
}

#[tauri::command]
pub async fn shuffle_load_sources(app: AppHandle) -> Result<View, String> {
    if let Some((client, user_id)) = connected_client(&app) {
        match client.playlists().await {
            Ok(playlists) => apply_sources(&app, &playlists, &user_id),
            Err(error) => {
                spotify::note_error(&app, &error);
                app.state::<Shuffle>().live().notice = Some(Notice::from_error(&error));
            }
        }
        emit(&app);
    }
    Ok(view(&app))
}

#[tauri::command]
pub fn shuffle_select(app: AppHandle, shuffle: State<'_, Shuffle>, source: String) -> View {
    shuffle.state().select(&source);
    shuffle.live().notice = None;
    shuffle.save();
    emit(&app);
    view(&app)
}

#[tauri::command]
pub fn shuffle_toggle_source(app: AppHandle, shuffle: State<'_, Shuffle>, source: String) -> View {
    shuffle.state().toggle_member(&source);
    shuffle.live().notice = None;
    shuffle.save();
    emit(&app);
    view(&app)
}

#[tauri::command]
pub fn shuffle_play(app: AppHandle) -> View {
    if begin(
        &app,
        Activity::Loading {
            loaded: 0,
            total: 0,
        },
    ) {
        tauri::async_runtime::spawn(shuffle_and_play(app.clone()));
    }
    view(&app)
}

#[tauri::command]
pub fn shuffle_reset(app: AppHandle, shuffle: State<'_, Shuffle>) -> View {
    if shuffle.live().activity.is_none() {
        let mut state = shuffle.state();
        let source = state.selected_source().to_owned();
        state.reset(&source);
    }
    shuffle.save();
    emit(&app);
    view(&app)
}

#[tauri::command]
pub fn shuffle_restore_removed(app: AppHandle, shuffle: State<'_, Shuffle>) -> View {
    if shuffle.live().activity.is_none() {
        let mut state = shuffle.state();
        let source = state.selected_source().to_owned();
        state.restore_removed(&source);
    }
    shuffle.save();
    emit(&app);
    view(&app)
}

#[tauri::command]
pub fn shuffle_set_keep_playing(app: AppHandle, shuffle: State<'_, Shuffle>, on: bool) -> View {
    shuffle.state().keep_playing = on;
    shuffle.save();
    emit(&app);
    view(&app)
}

/// Skips the playing track, takes it out of the shuffle playlist and keeps
/// it out of every future shuffle of the source.
#[tauri::command]
pub fn shuffle_remove_current(app: AppHandle, shuffle: State<'_, Shuffle>) -> View {
    let uri = {
        let live = shuffle.live();
        live.in_shuffle.then(|| live.current.clone()).flatten()
    };
    let playlist_id = shuffle.state().playlist_id.clone();
    let (Some(uri), Some(playlist_id)) = (uri, playlist_id) else {
        return view(&app);
    };
    if !begin(&app, Activity::Working) {
        return view(&app);
    }
    // The removal stands even if Spotify cannot be told right now.
    if shuffle.state().remove(&uri) {
        shuffle.save();
    }
    shuffle.live().in_shuffle = false;
    app.state::<AppState>()
        .request(Request::Command(Command::Next));
    let task_app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(client) = task_app.state::<Spotify>().client() else {
            return finish(&task_app, Some(Notice::Auth));
        };
        match client.remove_items(&playlist_id, &[uri]).await {
            Ok(()) => finish(&task_app, None),
            Err(error) => fail(&task_app, &error),
        }
    });
    view(&app)
}

/// Copies the order, without removed tracks, to a new private playlist
/// that Singline never changes again.
#[tauri::command]
pub fn shuffle_keep(app: AppHandle, shuffle: State<'_, Shuffle>, name: String) -> View {
    let name: String = name.trim().chars().take(KEPT_NAME_LIMIT).collect();
    let order = {
        let state = shuffle.state();
        state.has_shuffle().then(|| state.kept_order())
    };
    let Some(order) = order.filter(|order| !order.is_empty() && !name.is_empty()) else {
        return view(&app);
    };
    if !begin(&app, Activity::Working) {
        return view(&app);
    }
    let task_app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(client) = task_app.state::<Spotify>().client() else {
            return finish(&task_app, Some(Notice::Auth));
        };
        let kept = async {
            let id = client.create_playlist(&name, "").await?;
            client.replace_items(&id, &order, |_| {}).await
        }
        .await;
        match kept {
            Ok(()) => finish(&task_app, Some(Notice::Kept { name })),
            Err(error) => fail(&task_app, &error),
        }
    });
    view(&app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_survives_a_restart_but_not_the_demo() {
        let directory =
            std::env::temp_dir().join(format!("singline-shuffle-{}", std::process::id()));
        let shuffle = Shuffle::new(false, Some(&directory));
        shuffle.state().toggle_member("p1");
        shuffle.state().keep_playing = false;
        shuffle.save();
        let again = Shuffle::new(false, Some(&directory));
        let view = again.view(true);
        assert_eq!(view.selected, "mix:liked,p1");
        assert_eq!(view.members, ["liked", "p1"]);
        assert!(!view.keep_playing);
        assert_eq!(view.sources, [liked()]);
        assert!(view.up_next.is_empty());
        let demo = Shuffle::new(true, Some(&directory));
        assert_eq!(demo.view(false).selected, LIKED_SONGS);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn notices_name_the_problem() {
        assert_eq!(
            Notice::from_error(&SpotifyError::LoginExpired),
            Notice::Auth
        );
        assert_eq!(
            Notice::from_error(&SpotifyError::Forbidden(None)),
            Notice::Premium
        );
        assert_eq!(
            Notice::from_error(&SpotifyError::Http(502)),
            Notice::Http { status: 502 }
        );
        let json = serde_json::to_value(Activity::Writing {
            written: 100,
            total: 250,
        })
        .expect("json");
        assert_eq!(
            json,
            serde_json::json!({"kind": "writing", "written": 100, "total": 250})
        );
    }
}
