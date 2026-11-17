//! Estimating the playback position between two readings from a player.

/// A position reading plus the moment it was taken.
///
/// Players report their position only now and then. Between readings the overlay
/// advances the position itself, which keeps the highlight smooth without asking
/// the player many times a second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaybackClock {
    position: f64,
    sampled_at: f64,
    is_playing: bool,
    rate: f64,
}

impl PlaybackClock {
    /// `position` and `sampled_at` are seconds. Use the same monotonic clock for
    /// `sampled_at` here and for `time` in [`position_at`](Self::position_at).
    pub fn new(position: f64, sampled_at: f64, is_playing: bool) -> Self {
        Self {
            position: if position.is_finite() {
                position.max(0.0)
            } else {
                0.0
            },
            sampled_at,
            is_playing,
            rate: 1.0,
        }
    }

    /// Sets the playback speed, for players that report one. Invalid rates are ignored.
    pub fn with_rate(mut self, rate: f64) -> Self {
        if rate.is_finite() && rate > 0.0 {
            self.rate = rate;
        }
        self
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing
    }

    /// The estimated position at `time`. Never runs past `duration` when the
    /// duration is known, and never rewinds for a `time` before the reading.
    pub fn position_at(&self, time: f64, duration: f64) -> f64 {
        let elapsed = time - self.sampled_at;
        let advance = if self.is_playing && elapsed.is_finite() {
            elapsed.max(0.0) * self.rate
        } else {
            0.0
        };
        let estimated = self.position + advance;
        if duration.is_finite() && duration > 0.0 {
            estimated.min(duration)
        } else if estimated.is_finite() {
            estimated
        } else {
            self.position
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_scales_the_advance() {
        let clock = PlaybackClock::new(10.0, 100.0, true).with_rate(1.5);
        assert_eq!(clock.position_at(102.0, 0.0), 13.0);
    }

    #[test]
    fn invalid_rates_are_ignored() {
        for rate in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let clock = PlaybackClock::new(10.0, 100.0, true).with_rate(rate);
            assert_eq!(clock.position_at(101.0, 0.0), 11.0);
        }
    }
}
