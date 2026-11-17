//! Choosing which player the overlay follows when several are open.

use std::collections::HashSet;
use std::hash::Hash;

/// What a player is doing right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackActivity {
    /// Open, but stopped or without a track.
    Idle,
    Playing,
    Paused,
    /// Playing, but the player does not report a usable track or position.
    PlayingWithoutTiming,
    /// The reading failed, so the previous state is kept.
    Unknown,
}

/// Follows the player that most recently started playing.
///
/// A player that starts playing takes over. Otherwise the current player is kept
/// as long as it plays, so two playing apps do not make the overlay flip back and
/// forth. Paused players are a last resort.
#[derive(Debug, Clone)]
pub struct PlaybackSelection<K> {
    current: Option<K>,
    previously_playing: HashSet<K>,
}

impl<K> Default for PlaybackSelection<K> {
    fn default() -> Self {
        Self {
            current: None,
            previously_playing: HashSet::new(),
        }
    }
}

impl<K: Clone + Eq + Hash> PlaybackSelection<K> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current(&self) -> Option<&K> {
        self.current.as_ref()
    }

    /// Picks a player from the latest readings. `activity` lists every player
    /// that is open, in a stable order that breaks ties. A player missing from
    /// the list counts as closed.
    pub fn choose(&mut self, activity: &[(K, PlaybackActivity)]) -> Option<K> {
        let with = |wanted: PlaybackActivity| -> Vec<&K> {
            activity
                .iter()
                .filter(|(_, state)| *state == wanted)
                .map(|(key, _)| key)
                .collect()
        };
        let playing = with(PlaybackActivity::Playing);
        let blocked = with(PlaybackActivity::PlayingWithoutTiming);
        let paused = with(PlaybackActivity::Paused);

        let newly_playing = playing.iter().copied().find(|key| {
            Some(*key) != self.current.as_ref() && !self.previously_playing.contains(*key)
        });
        let selected = newly_playing
            .or_else(|| self.prefer_current(&playing))
            .or_else(|| self.prefer_current(&blocked))
            .or_else(|| self.prefer_current(&paused))
            .cloned();

        // Closed players are forgotten, so they count as newly playing next time.
        self.previously_playing
            .retain(|key| activity.iter().any(|(open, _)| open == key));
        for (key, state) in activity {
            match state {
                PlaybackActivity::Playing | PlaybackActivity::PlayingWithoutTiming => {
                    self.previously_playing.insert(key.clone());
                }
                PlaybackActivity::Idle | PlaybackActivity::Paused => {
                    self.previously_playing.remove(key);
                }
                PlaybackActivity::Unknown => {}
            }
        }

        self.current = selected.clone();
        selected
    }

    fn prefer_current<'a>(&self, candidates: &[&'a K]) -> Option<&'a K> {
        candidates
            .iter()
            .copied()
            .find(|key| Some(*key) == self.current.as_ref())
            .or_else(|| candidates.first().copied())
    }
}

#[cfg(test)]
mod tests {
    use super::PlaybackActivity::*;
    use super::*;

    #[test]
    fn nothing_open_selects_nothing() {
        let mut selection = PlaybackSelection::<&str>::new();
        assert_eq!(selection.choose(&[]), None);
        assert_eq!(selection.choose(&[("spotify", Idle)]), None);
    }

    #[test]
    fn the_player_that_starts_playing_takes_over() {
        let mut selection = PlaybackSelection::new();
        assert_eq!(
            selection.choose(&[("spotify", Playing), ("music", Idle)]),
            Some("spotify")
        );
        assert_eq!(
            selection.choose(&[("spotify", Playing), ("music", Playing)]),
            Some("music"),
            "music started while spotify was already playing"
        );
        assert_eq!(
            selection.choose(&[("spotify", Playing), ("music", Playing)]),
            Some("music"),
            "the current player is kept while both keep playing"
        );
    }

    #[test]
    fn falls_back_when_the_current_player_stops() {
        let mut selection = PlaybackSelection::new();
        selection.choose(&[("spotify", Playing), ("music", Idle)]);
        selection.choose(&[("spotify", Playing), ("music", Playing)]);
        assert_eq!(
            selection.choose(&[("spotify", Playing), ("music", Paused)]),
            Some("spotify")
        );
    }

    #[test]
    fn a_paused_player_is_kept_until_something_plays() {
        let mut selection = PlaybackSelection::new();
        selection.choose(&[("spotify", Playing)]);
        assert_eq!(
            selection.choose(&[("spotify", Paused), ("music", Paused)]),
            Some("spotify")
        );
        assert_eq!(
            selection.choose(&[("spotify", Paused), ("music", Playing)]),
            Some("music")
        );
    }

    #[test]
    fn playing_beats_playing_without_timing() {
        let mut selection = PlaybackSelection::new();
        assert_eq!(
            selection.choose(&[("radio", PlayingWithoutTiming), ("spotify", Paused)]),
            Some("radio")
        );
        assert_eq!(
            selection.choose(&[("radio", PlayingWithoutTiming), ("spotify", Playing)]),
            Some("spotify")
        );
    }

    #[test]
    fn a_failed_reading_does_not_look_like_a_fresh_start() {
        let mut selection = PlaybackSelection::new();
        selection.choose(&[("spotify", Playing), ("music", Playing)]);
        assert_eq!(selection.current(), Some(&"spotify"));
        selection.choose(&[("spotify", Playing), ("music", Unknown)]);
        assert_eq!(
            selection.choose(&[("spotify", Playing), ("music", Playing)]),
            Some("spotify"),
            "music was already playing before the failed reading"
        );
    }

    #[test]
    fn a_reopened_player_counts_as_newly_playing() {
        let mut selection = PlaybackSelection::new();
        selection.choose(&[("spotify", Playing), ("music", Playing)]);
        selection.choose(&[("spotify", Playing)]);
        assert_eq!(
            selection.choose(&[("spotify", Playing), ("music", Playing)]),
            Some("music")
        );
    }
}
