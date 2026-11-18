//! Hiding the overlay while nothing plays.

use std::time::{Duration, Instant};

/// How long the overlay stays after playback stops before auto-hide hides it.
pub const AUTO_HIDE_GRACE: Duration = Duration::from_secs(3);

/// Hides the overlay a few seconds after the music stops and shows it again
/// when music resumes. It only ever hides on top of the listener's own choice:
/// an overlay the listener hid stays hidden, even while music plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoHide {
    /// The last moment the overlay had a reason to show: music played, the
    /// app started, or the listener asked to see it.
    last_reason: Instant,
}

impl AutoHide {
    /// Starts with a grace period, so the overlay shows briefly after launch
    /// even when nothing plays.
    pub fn new(now: Instant) -> Self {
        Self { last_reason: now }
    }

    /// Starts the grace period over, for example when the listener shows the
    /// overlay again while nothing plays.
    pub fn wake(&mut self, now: Instant) {
        self.last_reason = self.last_reason.max(now);
    }

    /// Records whether there is a reason to show the overlay at `now`: the
    /// followed player plays a song, or something else needs the overlay in
    /// view, such as the open settings window.
    pub fn observe(&mut self, active: bool, now: Instant) {
        if active {
            self.wake(now);
        }
    }

    /// Whether the overlay shows at `now`. `chosen` is the listener's own
    /// choice, `enabled` the auto-hide setting.
    pub fn shows(&self, chosen: bool, enabled: bool, now: Instant) -> bool {
        chosen && (!enabled || now.saturating_duration_since(self.last_reason) < AUTO_HIDE_GRACE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(start: Instant, seconds: f64) -> Instant {
        start + Duration::from_secs_f64(seconds)
    }

    #[test]
    fn hides_a_few_seconds_after_the_music_stops() {
        let start = Instant::now();
        let mut policy = AutoHide::new(start);
        policy.observe(true, at(start, 10.0));
        assert!(policy.shows(true, true, at(start, 10.0)));
        policy.observe(false, at(start, 12.9));
        assert!(
            policy.shows(true, true, at(start, 12.9)),
            "still in the grace period"
        );
        policy.observe(false, at(start, 13.0));
        assert!(!policy.shows(true, true, at(start, 13.0)));
        policy.observe(true, at(start, 60.0));
        assert!(
            policy.shows(true, true, at(start, 60.0)),
            "shows again as soon as music resumes"
        );
    }

    #[test]
    fn shows_briefly_after_launch() {
        let start = Instant::now();
        let policy = AutoHide::new(start);
        assert!(policy.shows(true, true, at(start, 2.0)));
        assert!(!policy.shows(true, true, at(start, 3.5)));
    }

    #[test]
    fn hiding_it_yourself_always_wins() {
        let start = Instant::now();
        let mut policy = AutoHide::new(start);
        policy.observe(true, at(start, 1.0));
        assert!(!policy.shows(false, true, at(start, 1.0)));
        assert!(
            !policy.shows(false, false, at(start, 1.0)),
            "with auto-hide off too"
        );
    }

    #[test]
    fn off_means_the_listeners_choice_alone() {
        let start = Instant::now();
        let policy = AutoHide::new(start);
        assert!(policy.shows(true, false, at(start, 3_600.0)));
    }

    #[test]
    fn showing_it_again_starts_the_grace_over() {
        let start = Instant::now();
        let mut policy = AutoHide::new(start);
        assert!(!policy.shows(true, true, at(start, 20.0)));
        policy.wake(at(start, 20.0));
        assert!(policy.shows(true, true, at(start, 22.0)));
        assert!(!policy.shows(true, true, at(start, 23.5)));
        policy.wake(at(start, 5.0));
        assert!(
            !policy.shows(true, true, at(start, 23.5)),
            "an older moment never moves the grace back"
        );
    }
}
