//! The sleep timer: pauses the music after a set time or when the song ends.
//!
//! Pure: the app checks [`SleepTimer::remaining`] after every reading of the
//! players and sends a pause once it reaches zero.

use std::time::{Duration, Instant};

/// What the sleep timer waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SleepMode {
    /// Pauses after this many minutes.
    Minutes(u32),
    /// Pauses just before the song playing now ends.
    EndOfTrack,
}

impl SleepMode {
    /// The choices the menu offers, besides off.
    pub const CHOICES: [SleepMode; 5] = [
        SleepMode::Minutes(15),
        SleepMode::Minutes(30),
        SleepMode::Minutes(45),
        SleepMode::Minutes(60),
        SleepMode::EndOfTrack,
    ];
}

/// Seconds left in the song below which a jump back up means the song
/// started over, as with repeat-one.
const RESTART_WINDOW: f64 = 5.0;

#[derive(Debug, Clone, PartialEq)]
pub struct SleepTimer {
    mode: SleepMode,
    /// When a timer for a set time is due.
    deadline: Option<Instant>,
    /// The song that was playing when "end of track" was chosen.
    track_key: Option<String>,
    /// Seconds left in that song at the last check.
    last_left: Option<f64>,
}

impl SleepTimer {
    /// Starts a timer at `now`. Waiting for the end of the song needs a song,
    /// so "end of track" without one gives `None`.
    pub fn start(mode: SleepMode, now: Instant, track_key: Option<&str>) -> Option<Self> {
        let deadline = match mode {
            SleepMode::Minutes(minutes) => Some(now + Duration::from_secs(u64::from(minutes) * 60)),
            SleepMode::EndOfTrack => {
                track_key?;
                None
            }
        };
        Some(Self {
            mode,
            deadline,
            track_key: track_key.map(str::to_owned),
            last_left: None,
        })
    }

    pub fn mode(&self) -> SleepMode {
        self.mode
    }

    /// Seconds until the pause, checked at `now`: zero means pause now, `None`
    /// that the end of the song is not known yet. `track_key` and `track_left`
    /// describe the followed song and the seconds left in it. Players are only
    /// read now and then, so the end of a song is caught `lead` seconds early
    /// rather than after the next one started. A different song, or none, means
    /// the one the timer waits for ended.
    pub fn remaining(
        &mut self,
        now: Instant,
        track_key: Option<&str>,
        track_left: Option<f64>,
        lead: f64,
    ) -> Option<f64> {
        if let Some(deadline) = self.deadline {
            return Some(deadline.saturating_duration_since(now).as_secs_f64());
        }
        if track_key != self.track_key.as_deref() {
            return Some(0.0);
        }
        let left = track_left.filter(|left| left.is_finite())?.max(0.0);
        let started_over = self
            .last_left
            .is_some_and(|last| last <= RESTART_WINDOW && left > last + 1.0);
        self.last_left = Some(left);
        if started_over || left <= lead {
            Some(0.0)
        } else {
            Some(left - lead)
        }
    }
}

/// Whole minutes left, rounded up, as the menu shows them.
pub fn minutes_left(seconds: f64) -> u64 {
    if seconds.is_finite() && seconds > 0.0 {
        (seconds / 60.0).ceil() as u64
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(start: Instant, seconds: f64) -> Instant {
        start + Duration::from_secs_f64(seconds)
    }

    fn close(value: Option<f64>, expected: f64) -> bool {
        value.is_some_and(|value| (value - expected).abs() < 1e-6)
    }

    #[test]
    fn a_set_time_counts_down_to_zero() {
        let start = Instant::now();
        let mut timer = SleepTimer::start(SleepMode::Minutes(15), start, None).expect("starts");
        assert_eq!(timer.mode(), SleepMode::Minutes(15));
        assert!(close(timer.remaining(start, None, None, 0.6), 900.0));
        assert!(
            close(
                timer.remaining(at(start, 899.5), Some("other song"), Some(3.0), 0.6),
                0.5
            ),
            "songs do not matter for a set time"
        );
        assert_eq!(
            timer.remaining(at(start, 900.0), None, None, 0.6),
            Some(0.0)
        );
        assert_eq!(
            timer.remaining(at(start, 2_000.0), None, None, 0.6),
            Some(0.0)
        );
    }

    #[test]
    fn end_of_track_needs_a_song() {
        assert_eq!(
            SleepTimer::start(SleepMode::EndOfTrack, Instant::now(), None),
            None
        );
    }

    #[test]
    fn end_of_track_pauses_just_before_the_end() {
        let now = Instant::now();
        let mut timer =
            SleepTimer::start(SleepMode::EndOfTrack, now, Some("song")).expect("starts");
        assert!(close(
            timer.remaining(now, Some("song"), Some(90.0), 0.6),
            89.4
        ));
        assert_eq!(
            timer.remaining(now, Some("song"), None, 0.6),
            None,
            "an unknown end waits for the next song"
        );
        assert!(close(
            timer.remaining(now, Some("song"), Some(0.9), 0.6),
            0.3
        ));
        assert_eq!(
            timer.remaining(now, Some("song"), Some(0.5), 0.6),
            Some(0.0)
        );
    }

    #[test]
    fn end_of_track_pauses_when_the_song_changes() {
        let now = Instant::now();
        let mut timer =
            SleepTimer::start(SleepMode::EndOfTrack, now, Some("song")).expect("starts");
        assert_eq!(
            timer.remaining(now, Some("next"), Some(200.0), 0.6),
            Some(0.0)
        );
        assert_eq!(timer.remaining(now, None, None, 0.6), Some(0.0));
    }

    #[test]
    fn end_of_track_notices_a_repeated_song() {
        let now = Instant::now();
        let mut timer =
            SleepTimer::start(SleepMode::EndOfTrack, now, Some("song")).expect("starts");
        assert!(close(
            timer.remaining(now, Some("song"), Some(1.2), 0.6),
            0.6
        ));
        assert_eq!(
            timer.remaining(now, Some("song"), Some(179.7), 0.6),
            Some(0.0),
            "the same song started over"
        );

        let mut seeking =
            SleepTimer::start(SleepMode::EndOfTrack, now, Some("song")).expect("starts");
        seeking.remaining(now, Some("song"), Some(100.0), 0.6);
        assert!(
            close(
                seeking.remaining(now, Some("song"), Some(150.0), 0.6),
                149.4
            ),
            "seeking back in the middle is no restart"
        );
    }

    #[test]
    fn minutes_round_up() {
        assert_eq!(minutes_left(900.0), 15);
        assert_eq!(minutes_left(899.0), 15);
        assert_eq!(minutes_left(60.0), 1);
        assert_eq!(minutes_left(0.4), 1);
        assert_eq!(minutes_left(0.0), 0);
        assert_eq!(minutes_left(f64::NAN), 0);
    }
}
