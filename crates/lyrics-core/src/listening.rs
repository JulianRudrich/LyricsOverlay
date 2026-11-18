//! The listening history: which songs were heard and for how long, and what
//! "Your music" makes of them.
//!
//! Pure like the rest of the crate: the app feeds the trackers its readings of
//! the players with the current Unix time, and [`summarize`] gets the stored
//! sessions with the current time in the listener's time zone.

use std::collections::HashMap;
use std::hash::Hash;

use chrono::{
    DateTime, Datelike, Days, Months, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike,
};
use serde::{Deserialize, Serialize};

/// A song counts as played after this many seconds, or half of a shorter song.
pub const PLAY_SECONDS: f64 = 30.0;
/// How many songs and artists the rankings list.
pub const TOP_COUNT: usize = 10;
/// A day counts toward the streak with at least this many seconds of music.
pub const STREAK_SECONDS: f64 = 60.0;
/// The most bars the chart over time gets; older ones are left out.
const MAX_BUCKETS: usize = 400;

/// One song as it was heard: how long it actually played, not how long it is.
#[derive(Debug, Clone, PartialEq)]
pub struct ListenSession {
    /// The same for a song in every player, see `media_session::normalize::track_key`.
    pub track_key: String,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    /// The service of the player, such as `spotify` or `other`.
    pub service: String,
    /// When the song started, in Unix seconds.
    pub started_at: i64,
    /// The length of the song in seconds, when the player reports it.
    pub duration: Option<f64>,
    /// Seconds actually played: pauses don't count.
    pub listened: f64,
    /// Whether the overlay showed synced lyrics for it.
    pub synced_lyrics: bool,
}

impl ListenSession {
    fn begin(song: Song<'_>, now: f64) -> Self {
        Self {
            track_key: song.key.to_owned(),
            title: song.title.to_owned(),
            artist: song.artist.to_owned(),
            album: song.album.map(str::to_owned),
            service: song.service.to_owned(),
            started_at: if now.is_finite() {
                now.floor() as i64
            } else {
                0
            },
            duration: song
                .duration
                .filter(|duration| duration.is_finite() && *duration > 0.0),
            listened: 0.0,
            synced_lyrics: false,
        }
    }

    /// After 30 seconds, or half of a shorter song.
    pub fn counts_as_play(&self) -> bool {
        let needed = self
            .duration
            .filter(|duration| duration.is_finite() && *duration > 0.0)
            .map_or(PLAY_SECONDS, |duration| PLAY_SECONDS.min(duration / 2.0));
        self.listened >= needed
    }

    /// Left before it counted as a play.
    pub fn skipped(&self) -> bool {
        !self.counts_as_play()
    }
}

/// A song as one player reports it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Song<'a> {
    pub key: &'a str,
    pub title: &'a str,
    pub artist: &'a str,
    pub album: Option<&'a str>,
    pub service: &'a str,
    pub duration: Option<f64>,
}

/// One reading of one player.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Heard<'a> {
    /// The song in the player; `None` when it has none or stopped.
    pub song: Option<Song<'a>>,
    /// Seconds into the song, when the player reports it.
    pub position: Option<f64>,
    pub playing: bool,
    /// Whether the overlay shows synced lyrics for the song right now.
    pub synced_lyrics: bool,
}

/// Turns the readings of one player into sessions.
#[derive(Debug, Clone, Default)]
pub struct ListeningTracker {
    current: Option<ListenSession>,
    last_tick: Option<f64>,
    last_position: Option<f64>,
    playing: bool,
}

impl ListeningTracker {
    /// A longer gap between two readings, such as after sleep or a hung
    /// player, counts as this many seconds at most.
    pub const MAX_GAP: f64 = 5.0;
    /// Shorter sessions are noise from skipping through songs and are dropped.
    pub const MINIMUM: f64 = 1.0;

    pub fn new() -> Self {
        Self::default()
    }

    /// The song being heard, once it played long enough to keep.
    pub fn in_progress(&self) -> Option<&ListenSession> {
        self.current
            .as_ref()
            .filter(|session| session.listened >= Self::MINIMUM)
    }

    /// Whether the last reading had a song playing.
    pub fn playing(&self) -> bool {
        self.playing
    }

    /// Feeds one reading taken at `now`, in Unix seconds. Returns the session
    /// that just ended: on a song change, or when a repeated song starts over.
    pub fn tick(&mut self, heard: Heard<'_>, now: f64) -> Option<ListenSession> {
        let song = heard.song;
        let restarted = match (&self.current, song, heard.position, self.last_position) {
            (Some(current), Some(song), Some(position), Some(last)) => {
                current.track_key == song.key
                    && current
                        .duration
                        .is_some_and(|duration| duration > 10.0 && last > duration - 5.0)
                    && position < 5.0
            }
            _ => false,
        };
        let changed = self
            .current
            .as_ref()
            .map(|session| session.track_key.as_str())
            != song.map(|s| s.key);
        let mut finished = None;
        if changed || restarted {
            finished = self.finish();
            self.current = song.map(|song| ListenSession::begin(song, now));
        } else if let Some(current) = self.current.as_mut() {
            if let (true, Some(last)) = (heard.playing, self.last_tick) {
                let elapsed = now - last;
                if elapsed.is_finite() {
                    current.listened += elapsed.clamp(0.0, Self::MAX_GAP);
                }
            }
            if current.duration.is_none() {
                // Some players report the length a moment after the song starts.
                current.duration = song
                    .and_then(|song| song.duration)
                    .filter(|duration| duration.is_finite() && *duration > 0.0);
            }
        }
        if let (true, Some(current)) = (heard.synced_lyrics, self.current.as_mut()) {
            current.synced_lyrics = true;
        }
        self.last_tick = Some(now);
        self.last_position = heard.position;
        self.playing = heard.playing && self.current.is_some();
        finished
    }

    /// Ends the current session, for example when the player closes.
    pub fn finish(&mut self) -> Option<ListenSession> {
        self.last_tick = None;
        self.last_position = None;
        self.playing = false;
        self.current
            .take()
            .filter(|session| session.listened >= Self::MINIMUM)
    }
}

/// Tracks every open player at once, keyed by player.
#[derive(Debug, Clone)]
pub struct ListeningLog<K> {
    trackers: HashMap<K, ListeningTracker>,
}

impl<K> Default for ListeningLog<K> {
    fn default() -> Self {
        Self {
            trackers: HashMap::new(),
        }
    }
}

impl<K: Clone + Eq + Hash> ListeningLog<K> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one round of readings taken at `now`, in Unix seconds. A player
    /// given `None` is open but could not be read, so its song carries on. A
    /// player missing from `readings` was closed, so its song ends. Returns the
    /// sessions that ended, oldest first.
    pub fn tick(&mut self, readings: Vec<(K, Option<Heard<'_>>)>, now: f64) -> Vec<ListenSession> {
        let mut finished = Vec::new();
        self.trackers.retain(|player, tracker| {
            let open = readings.iter().any(|(key, _)| key == player);
            if !open {
                finished.extend(tracker.finish());
            }
            open
        });
        for (player, heard) in readings {
            if let Some(heard) = heard {
                let tracker = self.trackers.entry(player).or_default();
                finished.extend(tracker.tick(heard, now));
            }
        }
        finished.sort_by_key(|session| session.started_at);
        finished
    }

    /// The songs being heard now, once they played long enough to keep.
    pub fn in_progress(&self) -> Vec<ListenSession> {
        let mut sessions: Vec<ListenSession> = self
            .trackers
            .values()
            .filter_map(|tracker| tracker.in_progress().cloned())
            .collect();
        sessions.sort_by_key(|session| session.started_at);
        sessions
    }

    /// Whether any player played a song at the last reading.
    pub fn playing(&self) -> bool {
        self.trackers.values().any(ListeningTracker::playing)
    }

    /// Ends every session, for example when the app quits.
    pub fn finish_all(&mut self) -> Vec<ListenSession> {
        let mut finished: Vec<ListenSession> = self
            .trackers
            .drain()
            .filter_map(|(_, mut tracker)| tracker.finish())
            .collect();
        finished.sort_by_key(|session| session.started_at);
        finished
    }
}

/// The periods "Your music" offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ListeningRange {
    Today,
    #[default]
    Week,
    Month,
    Year,
    All,
}

impl ListeningRange {
    /// The first day of the range, counted in local days; `None` for all time.
    pub fn first_day(self, today: NaiveDate) -> Option<NaiveDate> {
        match self {
            ListeningRange::Today => Some(today),
            ListeningRange::Week => today.checked_sub_days(Days::new(6)),
            ListeningRange::Month => today.checked_sub_days(Days::new(29)),
            ListeningRange::Year => today
                .with_day(1)
                .and_then(|month| month.checked_sub_months(Months::new(11))),
            ListeningRange::All => None,
        }
    }
}

/// How long one bar of the chart over time lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BucketUnit {
    Hour,
    Day,
    Month,
}

/// One bar of the chart over time.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bucket {
    /// Where the bar starts, in local time.
    pub start: NaiveDateTime,
    pub seconds: f64,
}

/// One row of the most played songs or artists.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedEntry {
    /// The track key, or the artist in lower case.
    pub key: String,
    pub title: String,
    /// The artist of a song; empty for an artist.
    pub subtitle: String,
    pub seconds: f64,
    pub plays: u32,
}

/// Everything the "Your music" window shows for one range.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListeningSummary {
    pub range: ListeningRange,
    /// Whether any song was recorded at all, in any range.
    pub has_history: bool,
    /// Seconds listened in the range.
    pub seconds: f64,
    /// Calendar days the range covers so far, counted from the first recorded
    /// day when that came later.
    pub days: u32,
    pub daily_average: f64,
    /// Songs heard in the range, played or skipped.
    pub songs: u32,
    pub plays: u32,
    /// Plays that had synced lyrics.
    pub synced_plays: u32,
    pub skips: u32,
    /// Days in a row with at least a minute of music, up to today, or up to
    /// yesterday while today is still empty.
    pub streak_days: u32,
    pub unit: BucketUnit,
    /// Listening over time, oldest first: hours for today, days for a week
    /// or a month, months beyond.
    pub buckets: Vec<Bucket>,
    /// Seconds per weekday, Monday first, and hour of the day.
    pub clock: [[f64; 24]; 7],
    pub top_tracks: Vec<RankedEntry>,
    pub top_artists: Vec<RankedEntry>,
}

/// Sums up `sessions` for `range`, with days and hours as the clock shows them
/// in the time zone of `now`. Sessions that start after `now` are left out.
pub fn summarize<Tz: TimeZone>(
    sessions: &[ListenSession],
    range: ListeningRange,
    now: &DateTime<Tz>,
) -> ListeningSummary {
    let zone = now.timezone();
    let today = now.naive_local().date();
    let latest = now.timestamp();
    let heard: Vec<(&ListenSession, NaiveDateTime)> = sessions
        .iter()
        .filter(|session| session.started_at <= latest)
        .filter_map(|session| {
            let local = zone.timestamp_opt(session.started_at, 0).single()?;
            Some((session, local.naive_local()))
        })
        .collect();
    let first_day = heard.iter().map(|(_, time)| time.date()).min();
    let from = range.first_day(today);
    let in_range: Vec<(&ListenSession, NaiveDateTime)> = heard
        .iter()
        .copied()
        .filter(|(_, time)| from.is_none_or(|from| time.date() >= from))
        .collect();

    let seconds: f64 = in_range.iter().map(|(session, _)| session.listened).sum();
    let counted_from = match (from, first_day) {
        (Some(from), Some(first)) => from.max(first),
        (Some(day), None) | (None, Some(day)) => day,
        (None, None) => today,
    };
    let days = today.signed_duration_since(counted_from).num_days() + 1;
    let days = u32::try_from(days.max(1)).unwrap_or(u32::MAX);
    let count = |keep: &dyn Fn(&ListenSession) -> bool| {
        let matching = in_range.iter().filter(|(session, _)| keep(session)).count();
        u32::try_from(matching).unwrap_or(u32::MAX)
    };

    let (unit, starts) = timeline(range, from, first_day, today);
    let mut buckets: Vec<Bucket> = starts
        .iter()
        .map(|start| Bucket {
            start: *start,
            seconds: 0.0,
        })
        .collect();
    let mut clock = [[0.0; 24]; 7];
    for (session, time) in &in_range {
        if let Some(index) = bucket_of(unit, &starts, *time) {
            buckets[index].seconds += session.listened;
        }
        clock[time.weekday().num_days_from_monday() as usize][time.hour() as usize] +=
            session.listened;
    }

    let sessions_in_range = || in_range.iter().map(|(session, _)| *session);
    ListeningSummary {
        range,
        has_history: !sessions.is_empty(),
        seconds,
        days,
        daily_average: seconds / f64::from(days),
        songs: count(&|_| true),
        plays: count(&ListenSession::counts_as_play),
        synced_plays: count(&|session| session.counts_as_play() && session.synced_lyrics),
        skips: count(&ListenSession::skipped),
        streak_days: streak(&heard, today),
        unit,
        buckets,
        clock,
        top_tracks: rank(
            sessions_in_range(),
            |session| Some(session.track_key.clone()),
            |session| (session.title.clone(), session.artist.clone()),
        ),
        top_artists: rank(
            sessions_in_range(),
            |session| {
                let artist = session.artist.trim();
                (!artist.is_empty()).then(|| artist.to_lowercase())
            },
            |session| (session.artist.trim().to_owned(), String::new()),
        ),
    }
}

fn midnight(day: NaiveDate) -> NaiveDateTime {
    day.and_time(NaiveTime::MIN)
}

fn month_index(day: NaiveDate) -> i64 {
    i64::from(day.year()) * 12 + i64::from(day.month0())
}

/// Where the bars of the chart over time start.
fn timeline(
    range: ListeningRange,
    from: Option<NaiveDate>,
    first_day: Option<NaiveDate>,
    today: NaiveDate,
) -> (BucketUnit, Vec<NaiveDateTime>) {
    match range {
        ListeningRange::Today => (
            BucketUnit::Hour,
            (0..24)
                .filter_map(|hour| NaiveTime::from_hms_opt(hour, 0, 0))
                .map(|time| today.and_time(time))
                .collect(),
        ),
        ListeningRange::Week | ListeningRange::Month => {
            (BucketUnit::Day, days(from.unwrap_or(today), today))
        }
        ListeningRange::Year => (BucketUnit::Month, months(from.unwrap_or(today), today)),
        // A short history reads better by day than as one or two bars.
        ListeningRange::All => match first_day {
            Some(first) if today.signed_duration_since(first).num_days() >= 31 => {
                (BucketUnit::Month, months(first, today))
            }
            Some(first) => (BucketUnit::Day, days(first, today)),
            None => (BucketUnit::Day, Vec::new()),
        },
    }
}

fn days(first: NaiveDate, last: NaiveDate) -> Vec<NaiveDateTime> {
    let count = (last.signed_duration_since(first).num_days() + 1).clamp(0, MAX_BUCKETS as i64);
    let back = u64::try_from(count - 1).unwrap_or(0);
    let Some(start) = last.checked_sub_days(Days::new(back)) else {
        return Vec::new();
    };
    start
        .iter_days()
        .take(usize::try_from(count).unwrap_or(0))
        .map(midnight)
        .collect()
}

fn months(first: NaiveDate, last: NaiveDate) -> Vec<NaiveDateTime> {
    let count = (month_index(last) - month_index(first) + 1).clamp(0, MAX_BUCKETS as i64);
    let Some(this_month) = last.with_day(1) else {
        return Vec::new();
    };
    (0..u32::try_from(count).unwrap_or(0))
        .rev()
        .filter_map(|back| this_month.checked_sub_months(Months::new(back)))
        .map(midnight)
        .collect()
}

fn bucket_of(unit: BucketUnit, starts: &[NaiveDateTime], time: NaiveDateTime) -> Option<usize> {
    let first = starts.first()?;
    let offset = match unit {
        BucketUnit::Hour if time.date() == first.date() => {
            i64::from(time.hour()) - i64::from(first.hour())
        }
        BucketUnit::Hour => return None,
        BucketUnit::Day => time.date().signed_duration_since(first.date()).num_days(),
        BucketUnit::Month => month_index(time.date()) - month_index(first.date()),
    };
    usize::try_from(offset)
        .ok()
        .filter(|index| *index < starts.len())
}

/// Ranked by listening time; ties go to more plays, then the name. Entries
/// that never counted as a play are left out.
fn rank<'a>(
    sessions: impl Iterator<Item = &'a ListenSession>,
    group: impl Fn(&ListenSession) -> Option<String>,
    describe: impl Fn(&ListenSession) -> (String, String),
) -> Vec<RankedEntry> {
    struct Group {
        title: String,
        subtitle: String,
        latest: i64,
        seconds: f64,
        plays: u32,
    }
    let mut groups: HashMap<String, Group> = HashMap::new();
    for session in sessions {
        let Some(key) = group(session) else {
            continue;
        };
        let entry = groups.entry(key).or_insert_with(|| {
            let (title, subtitle) = describe(session);
            Group {
                title,
                subtitle,
                latest: session.started_at,
                seconds: 0.0,
                plays: 0,
            }
        });
        // The latest spelling wins, in case a player corrected it.
        if session.started_at > entry.latest {
            (entry.title, entry.subtitle) = describe(session);
            entry.latest = session.started_at;
        }
        entry.seconds += session.listened;
        entry.plays += u32::from(session.counts_as_play());
    }
    let mut entries: Vec<RankedEntry> = groups
        .into_iter()
        .filter(|(_, group)| group.plays > 0)
        .map(|(key, group)| RankedEntry {
            key,
            title: group.title,
            subtitle: group.subtitle,
            seconds: group.seconds,
            plays: group.plays,
        })
        .collect();
    entries.sort_by(|a, b| {
        b.seconds
            .total_cmp(&a.seconds)
            .then(b.plays.cmp(&a.plays))
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
            .then_with(|| a.key.cmp(&b.key))
    });
    entries.truncate(TOP_COUNT);
    entries
}

fn streak(heard: &[(&ListenSession, NaiveDateTime)], today: NaiveDate) -> u32 {
    let mut per_day: HashMap<NaiveDate, f64> = HashMap::new();
    for (session, time) in heard {
        *per_day.entry(time.date()).or_default() += session.listened;
    }
    let active = |day: &NaiveDate| {
        per_day
            .get(day)
            .is_some_and(|seconds| *seconds >= STREAK_SECONDS)
    };
    let mut day = if active(&today) {
        Some(today)
    } else {
        today.pred_opt()
    };
    let mut count = 0;
    while let Some(current) = day.filter(|day| active(day)) {
        count += 1;
        day = current.pred_opt();
    }
    count
}

#[cfg(test)]
mod tests {
    use chrono::FixedOffset;

    use super::*;

    fn song<'a>(key: &'a str, artist: &'a str, duration: Option<f64>) -> Song<'a> {
        Song {
            key,
            title: key,
            artist,
            album: None,
            service: "spotify",
            duration,
        }
    }

    fn playing<'a>(song: Song<'a>, position: f64) -> Heard<'a> {
        Heard {
            song: Some(song),
            position: Some(position),
            playing: true,
            synced_lyrics: false,
        }
    }

    /// Feeds readings twice a second while `song` plays for `seconds`, from the
    /// Unix time `start` on.
    fn play(
        tracker: &mut ListeningTracker,
        song: Song<'_>,
        start: f64,
        seconds: f64,
    ) -> Vec<ListenSession> {
        let mut finished = Vec::new();
        let mut elapsed = 0.0;
        while elapsed <= seconds {
            finished.extend(tracker.tick(playing(song, elapsed), start + elapsed));
            elapsed += 0.5;
        }
        finished
    }

    #[test]
    fn plays_need_thirty_seconds_or_half_a_short_song() {
        let session = |listened: f64, duration: Option<f64>| ListenSession {
            track_key: "k".into(),
            title: "T".into(),
            artist: "A".into(),
            album: None,
            service: "spotify".into(),
            started_at: 0,
            duration,
            listened,
            synced_lyrics: false,
        };
        assert!(session(30.0, Some(200.0)).counts_as_play());
        assert!(session(29.5, Some(200.0)).skipped());
        assert!(
            session(20.0, Some(40.0)).counts_as_play(),
            "half of a 40 s song"
        );
        assert!(session(19.0, Some(40.0)).skipped());
        assert!(
            session(30.0, None).counts_as_play(),
            "an unknown length needs 30 s"
        );
        assert!(session(29.0, None).skipped());
    }

    #[test]
    fn only_time_while_playing_counts() {
        let mut tracker = ListeningTracker::new();
        let tune = song("tune", "Band", Some(200.0));
        assert!(play(&mut tracker, tune, 1_000.0, 10.0).is_empty());
        let paused = Heard {
            playing: false,
            ..playing(tune, 10.0)
        };
        for step in 1..=20 {
            tracker.tick(paused, 1_010.0 + f64::from(step));
        }
        // Playing again counts from the reading before, like the reading after a pause.
        tracker.tick(playing(tune, 10.0), 1_031.0);
        tracker.tick(playing(tune, 12.0), 1_033.0);
        let session = tracker.in_progress().expect("in progress").clone();
        assert!(
            (session.listened - 13.0).abs() < 1e-9,
            "10 s, then 3 s after the pause, but not the 20 s paused: {}",
            session.listened
        );
        assert_eq!(session.started_at, 1_000);
        assert!(tracker.playing());
    }

    #[test]
    fn a_new_song_ends_the_session_before() {
        let mut tracker = ListeningTracker::new();
        let first = song("first", "Band", Some(200.0));
        let second = song("second", "Band", Some(180.0));
        play(&mut tracker, first, 0.0, 40.0);
        let finished = play(&mut tracker, second, 40.5, 3.0);
        let [ended] = finished.as_slice() else {
            panic!("expected one finished session, got {finished:?}");
        };
        assert_eq!(ended.track_key, "first");
        assert!((ended.listened - 40.0).abs() < 1e-9);
        assert!(ended.counts_as_play());
        assert_eq!(
            tracker.in_progress().map(|s| s.track_key.as_str()),
            Some("second")
        );
    }

    #[test]
    fn skipping_through_songs_leaves_no_trace() {
        let mut tracker = ListeningTracker::new();
        tracker.tick(playing(song("a", "X", Some(200.0)), 0.0), 0.0);
        tracker.tick(playing(song("a", "X", Some(200.0)), 0.5), 0.5);
        let finished = tracker.tick(playing(song("b", "X", Some(200.0)), 0.0), 1.0);
        assert_eq!(finished, None, "half a second is below the minimum");
        assert_eq!(tracker.finish(), None);
    }

    #[test]
    fn a_repeated_song_starts_a_new_session() {
        let mut tracker = ListeningTracker::new();
        let tune = song("loop", "Band", Some(20.0));
        tracker.tick(playing(tune, 1.0), 0.0);
        tracker.tick(playing(tune, 19.5), 18.5);
        let finished = tracker.tick(playing(tune, 0.5), 19.5);
        assert_eq!(finished.map(|s| s.track_key), Some("loop".to_owned()));
        assert_eq!(tracker.in_progress(), None, "the new round just started");
    }

    #[test]
    fn long_gaps_count_only_a_little() {
        let mut tracker = ListeningTracker::new();
        let tune = song("tune", "Band", Some(300.0));
        tracker.tick(playing(tune, 0.0), 0.0);
        tracker.tick(playing(tune, 100.0), 3_600.0);
        let session = tracker.finish().expect("kept");
        assert_eq!(session.listened, ListeningTracker::MAX_GAP);
    }

    #[test]
    fn synced_lyrics_and_a_late_length_are_kept() {
        let mut tracker = ListeningTracker::new();
        let unknown = song("tune", "Band", None);
        tracker.tick(playing(unknown, 0.0), 0.0);
        tracker.tick(
            Heard {
                synced_lyrics: true,
                ..playing(song("tune", "Band", Some(240.0)), 1.0)
            },
            1.0,
        );
        tracker.tick(playing(unknown, 2.0), 2.0);
        let session = tracker.finish().expect("kept");
        assert!(session.synced_lyrics, "once shown, it stays true");
        assert_eq!(session.duration, Some(240.0));
    }

    #[test]
    fn the_log_follows_every_player() {
        let mut log = ListeningLog::new();
        let a = song("a", "X", Some(200.0));
        let b = song("b", "Y", Some(200.0));
        for step in 0..=10 {
            let now = f64::from(step);
            log.tick(
                vec![
                    ("spotify", Some(playing(a, now))),
                    ("vlc", Some(playing(b, now))),
                ],
                now,
            );
        }
        assert_eq!(log.in_progress().len(), 2);
        assert!(log.playing());

        let kept = log.tick(
            vec![("spotify", None), ("vlc", Some(playing(b, 11.0)))],
            11.0,
        );
        assert!(kept.is_empty(), "an unreadable player keeps its song");
        let closed = log.tick(vec![("vlc", Some(playing(b, 12.0)))], 12.0);
        assert_eq!(
            closed
                .iter()
                .map(|s| s.track_key.as_str())
                .collect::<Vec<_>>(),
            ["a"],
            "a closed player ends its song"
        );
        let rest = log.finish_all();
        assert_eq!(
            rest.iter()
                .map(|s| s.track_key.as_str())
                .collect::<Vec<_>>(),
            ["b"]
        );
        assert!(log.in_progress().is_empty());
        assert!(!log.playing());
    }

    fn zone() -> FixedOffset {
        FixedOffset::east_opt(3_600).expect("valid offset")
    }

    /// 15:00 on Tuesday, 17 November 2026, one hour east of UTC.
    fn now() -> DateTime<FixedOffset> {
        local(2026, 11, 17, 15, 0)
    }

    fn local(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<FixedOffset> {
        zone()
            .with_ymd_and_hms(year, month, day, hour, minute, 0)
            .single()
            .expect("valid time")
    }

    fn heard(key: &str, artist: &str, at: DateTime<FixedOffset>, listened: f64) -> ListenSession {
        ListenSession {
            track_key: key.into(),
            title: key.to_uppercase(),
            artist: artist.into(),
            album: None,
            service: "spotify".into(),
            started_at: at.timestamp(),
            duration: Some(200.0),
            listened,
            synced_lyrics: false,
        }
    }

    fn day(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).expect("valid date")
    }

    #[test]
    fn ranges_start_on_local_days() {
        let today = day(2026, 11, 17);
        assert_eq!(ListeningRange::Today.first_day(today), Some(today));
        assert_eq!(
            ListeningRange::Week.first_day(today),
            Some(day(2026, 11, 11))
        );
        assert_eq!(
            ListeningRange::Month.first_day(today),
            Some(day(2026, 10, 19))
        );
        assert_eq!(
            ListeningRange::Year.first_day(today),
            Some(day(2025, 12, 1))
        );
        assert_eq!(ListeningRange::All.first_day(today), None);
    }

    #[test]
    fn totals_for_the_range() {
        let mut synced = heard("a", "X", local(2026, 11, 17, 9, 0), 200.0);
        synced.synced_lyrics = true;
        let sessions = [
            synced,
            heard("b", "X", local(2026, 11, 16, 22, 0), 100.0),
            heard("c", "Y", local(2026, 11, 15, 8, 0), 10.0),
            heard("old", "Y", local(2026, 11, 10, 8, 0), 500.0),
            heard("future", "Y", local(2026, 11, 17, 16, 0), 500.0),
        ];
        let summary = summarize(&sessions, ListeningRange::Week, &now());
        assert!(summary.has_history);
        assert_eq!(
            summary.seconds, 310.0,
            "the old and the future song are left out"
        );
        assert_eq!((summary.songs, summary.plays, summary.skips), (3, 2, 1));
        assert_eq!(summary.synced_plays, 1);
        assert_eq!(summary.days, 7);
        assert!((summary.daily_average - 310.0 / 7.0).abs() < 1e-9);
    }

    #[test]
    fn the_average_counts_from_the_first_recorded_day() {
        let sessions = [
            heard("a", "X", local(2026, 11, 15, 9, 0), 300.0),
            heard("b", "X", local(2026, 11, 17, 9, 0), 300.0),
        ];
        let summary = summarize(&sessions, ListeningRange::Month, &now());
        assert_eq!(summary.days, 3, "15 to 17 November");
        assert_eq!(summary.daily_average, 200.0);
        assert_eq!(summarize(&sessions, ListeningRange::Today, &now()).days, 1);
    }

    #[test]
    fn days_follow_the_listeners_time_zone() {
        // 23:30 UTC on Monday is 00:30 on Tuesday one hour east.
        let late = heard("late", "X", local(2026, 11, 17, 0, 30), 120.0);
        let east = summarize(std::slice::from_ref(&late), ListeningRange::Today, &now());
        assert_eq!(east.seconds, 120.0);
        assert_eq!(east.buckets[0].seconds, 120.0, "the first hour of Tuesday");
        assert_eq!(east.clock[1][0], 120.0);

        let utc = now().with_timezone(&chrono::Utc);
        let west = summarize(std::slice::from_ref(&late), ListeningRange::Today, &utc);
        assert_eq!(west.seconds, 0.0, "still Monday in UTC");
        let week = summarize(std::slice::from_ref(&late), ListeningRange::Week, &utc);
        assert_eq!(week.clock[0][23], 120.0, "Monday, 23:00 to 24:00");
    }

    #[test]
    fn today_is_split_into_hours() {
        let sessions = [
            heard("a", "X", local(2026, 11, 17, 9, 10), 100.0),
            heard("b", "X", local(2026, 11, 17, 9, 50), 50.0),
            heard("c", "X", local(2026, 11, 17, 14, 0), 25.0),
            heard("yesterday", "X", local(2026, 11, 16, 9, 0), 999.0),
        ];
        let summary = summarize(&sessions, ListeningRange::Today, &now());
        assert_eq!(summary.unit, BucketUnit::Hour);
        assert_eq!(summary.buckets.len(), 24);
        assert_eq!(
            summary.buckets[9].start,
            day(2026, 11, 17).and_hms_opt(9, 0, 0).expect("valid")
        );
        assert_eq!(summary.buckets[9].seconds, 150.0);
        assert_eq!(summary.buckets[14].seconds, 25.0);
        assert_eq!(
            summary.buckets.iter().map(|b| b.seconds).sum::<f64>(),
            175.0
        );
    }

    #[test]
    fn a_week_and_a_month_are_split_into_days() {
        let sessions = [
            heard("a", "X", local(2026, 11, 11, 12, 0), 60.0),
            heard("b", "X", local(2026, 11, 17, 12, 0), 30.0),
            heard("c", "X", local(2026, 10, 19, 12, 0), 45.0),
        ];
        let week = summarize(&sessions, ListeningRange::Week, &now());
        assert_eq!(week.unit, BucketUnit::Day);
        let seconds: Vec<f64> = week.buckets.iter().map(|b| b.seconds).collect();
        assert_eq!(seconds, [60.0, 0.0, 0.0, 0.0, 0.0, 0.0, 30.0]);
        assert_eq!(week.buckets[0].start, midnight(day(2026, 11, 11)));

        let month = summarize(&sessions, ListeningRange::Month, &now());
        assert_eq!(month.buckets.len(), 30);
        assert_eq!(month.buckets[0].start, midnight(day(2026, 10, 19)));
        assert_eq!(month.buckets[0].seconds, 45.0);
    }

    #[test]
    fn twelve_months_are_split_into_months() {
        let sessions = [
            heard("a", "X", local(2025, 12, 31, 12, 0), 60.0),
            heard("b", "X", local(2026, 11, 2, 12, 0), 30.0),
            heard("too old", "X", local(2025, 11, 30, 12, 0), 999.0),
        ];
        let summary = summarize(&sessions, ListeningRange::Year, &now());
        assert_eq!(summary.unit, BucketUnit::Month);
        assert_eq!(summary.buckets.len(), 12);
        assert_eq!(summary.buckets[0].start, midnight(day(2025, 12, 1)));
        assert_eq!(summary.buckets[11].start, midnight(day(2026, 11, 1)));
        assert_eq!(
            (summary.buckets[0].seconds, summary.buckets[11].seconds),
            (60.0, 30.0)
        );
        assert_eq!(summary.seconds, 90.0);
    }

    #[test]
    fn all_time_goes_by_day_until_a_month_has_passed() {
        let recent = [heard("a", "X", local(2026, 11, 3, 12, 0), 60.0)];
        let short = summarize(&recent, ListeningRange::All, &now());
        assert_eq!((short.unit, short.buckets.len()), (BucketUnit::Day, 15));
        assert_eq!(short.days, 15);

        let long = [
            heard("a", "X", local(2025, 9, 20, 12, 0), 60.0),
            heard("b", "X", local(2026, 11, 17, 12, 0), 30.0),
        ];
        let summary = summarize(&long, ListeningRange::All, &now());
        assert_eq!(summary.unit, BucketUnit::Month);
        assert_eq!(summary.buckets.len(), 15, "September 2025 to November 2026");
        assert_eq!(summary.buckets[0].seconds, 60.0);
        assert_eq!(summary.seconds, 90.0);

        let empty = summarize(&[], ListeningRange::All, &now());
        assert!(!empty.has_history);
        assert!(empty.buckets.is_empty());
        assert_eq!((empty.days, empty.daily_average), (1, 0.0));
    }

    #[test]
    fn the_clock_starts_on_monday() {
        let sessions = [
            heard("a", "X", local(2026, 11, 16, 7, 30), 60.0),
            heard("b", "X", local(2026, 11, 15, 23, 59), 30.0),
        ];
        let summary = summarize(&sessions, ListeningRange::Week, &now());
        assert_eq!(summary.clock[0][7], 60.0, "Monday, 7:00");
        assert_eq!(summary.clock[6][23], 30.0, "Sunday, 23:00");
        assert_eq!(summary.clock.iter().flatten().sum::<f64>(), 90.0);
    }

    #[test]
    fn the_streak_counts_days_in_a_row_with_music() {
        let day_with = |d: u32, listened: f64| heard("a", "X", local(2026, 11, d, 12, 0), listened);
        let sessions = [
            day_with(13, 600.0),
            day_with(14, 30.0),
            day_with(15, 60.0),
            day_with(16, 61.0),
            day_with(17, 10.0),
        ];
        let summary = summarize(&sessions, ListeningRange::Today, &now());
        assert_eq!(
            summary.streak_days, 2,
            "today has too little yet, so it counts up to yesterday; 30 s on the 14th breaks it"
        );
        let more = [&sessions[..], &[day_with(17, 60.0)]].concat();
        assert_eq!(
            summarize(&more, ListeningRange::Today, &now()).streak_days,
            3
        );
        let gap = [day_with(15, 600.0)];
        assert_eq!(summarize(&gap, ListeningRange::All, &now()).streak_days, 0);
    }

    #[test]
    fn rankings_go_by_listening_time() {
        let at = local(2026, 11, 17, 10, 0);
        let mut renamed = heard("one", "Artist A", local(2026, 11, 17, 11, 0), 100.0);
        renamed.title = "One (Remastered)".into();
        let sessions = [
            heard("one", "Artist A", at, 100.0),
            renamed,
            heard("two", "artist a", at, 150.0),
            heard("three", "Artist B", at, 150.0),
            heard("three", "Artist B", at, 10.0),
            heard("skipped", "Artist C", at, 20.0),
            heard("four", "", at, 40.0),
        ];
        let summary = summarize(&sessions, ListeningRange::Today, &now());
        let tracks: Vec<(&str, f64, u32)> = summary
            .top_tracks
            .iter()
            .map(|entry| (entry.title.as_str(), entry.seconds, entry.plays))
            .collect();
        assert_eq!(
            tracks,
            [
                ("One (Remastered)", 200.0, 2),
                ("THREE", 160.0, 1),
                ("TWO", 150.0, 1),
                ("FOUR", 40.0, 1),
            ],
            "a song only skipped is left out; the latest title wins"
        );
        assert_eq!(summary.top_tracks[0].subtitle, "Artist A");

        let artists: Vec<(&str, f64, u32)> = summary
            .top_artists
            .iter()
            .map(|entry| (entry.title.as_str(), entry.seconds, entry.plays))
            .collect();
        assert_eq!(
            artists,
            [("Artist A", 350.0, 3), ("Artist B", 160.0, 1)],
            "names are grouped regardless of case; empty artists are left out"
        );
    }

    #[test]
    fn rankings_stop_at_ten() {
        let at = local(2026, 11, 17, 10, 0);
        let sessions: Vec<ListenSession> = (0..15)
            .map(|index| {
                heard(
                    &format!("song{index:02}"),
                    &format!("A{index}"),
                    at,
                    40.0 + f64::from(index),
                )
            })
            .collect();
        let summary = summarize(&sessions, ListeningRange::Today, &now());
        assert_eq!(summary.top_tracks.len(), TOP_COUNT);
        assert_eq!(summary.top_tracks[0].key, "song14");
        assert_eq!(summary.top_artists.len(), TOP_COUNT);
    }

    #[test]
    fn the_summary_serializes_for_the_interface() {
        let sessions = [heard("a", "X", local(2026, 11, 17, 9, 0), 60.0)];
        let json = serde_json::to_value(summarize(&sessions, ListeningRange::Today, &now()))
            .expect("serializes");
        assert_eq!(json["range"], "today");
        assert_eq!(json["unit"], "hour");
        assert_eq!(json["buckets"][9]["start"], "2026-11-17T09:00:00");
        assert_eq!(json["clock"][1][9], 60.0);
        assert_eq!(json["topTracks"][0]["plays"], 1);
        assert_eq!(json["streakDays"], 1);
    }
}
