//! True Shuffle: an honest random order without repeats, written to one
//! playlist that Spotify then plays from top to bottom. Pure: the app keeps
//! [`ShuffleState`] as JSON and does the talking to Spotify.
//!
//! A *source* is Liked Songs ([`LIKED_SONGS`]), a playlist ID, or a mix of
//! several of them ([`mix_id`]). Every source has its own [`Cycle`]: no track
//! plays twice before every track of the source has played.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::model::TrackInfo;

/// The source ID of Liked Songs; playlists use their Spotify ID.
pub const LIKED_SONGS: &str = "liked";
/// Spotify playlists hold at most this many items.
pub const MAX_PLAYLIST_ITEMS: usize = 10_000;
/// How many tracks "Up next" shows.
pub const UP_NEXT: usize = 3;

const MIX_PREFIX: &str = "mix:";

/// The ID of a selection of sources. The same sources give the same ID in any
/// order, so picking them again continues their progress. A single source is
/// its own ID; no source gives `None`.
pub fn mix_id<'a>(members: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let members: BTreeSet<&str> = members
        .into_iter()
        .map(str::trim)
        .filter(|member| !member.is_empty() && !member.contains(',') && !is_mix(member))
        .collect();
    match members.len() {
        0 => None,
        1 => members.first().map(|member| (*member).to_owned()),
        _ => Some(format!(
            "{MIX_PREFIX}{}",
            members.into_iter().collect::<Vec<_>>().join(",")
        )),
    }
}

/// The sources of a mix ID, or the source itself.
pub fn mix_members(id: &str) -> Vec<&str> {
    match id.strip_prefix(MIX_PREFIX) {
        Some(members) => members
            .split(',')
            .filter(|member| !member.is_empty())
            .collect(),
        None => vec![id],
    }
}

pub fn is_mix(id: &str) -> bool {
    id.starts_with(MIX_PREFIX)
}

/// The tracks of several sources as one, each song once: the first time a
/// URI appears wins.
pub fn merge_sources(sources: impl IntoIterator<Item = Vec<TrackInfo>>) -> Vec<TrackInfo> {
    let mut seen = HashSet::new();
    let mut merged = Vec::new();
    for source in sources {
        for track in source {
            if seen.insert(track.uri.clone()) {
                merged.push(track);
            }
        }
    }
    merged
}

/// A random generator seeded by the operating system.
pub fn fresh_rng() -> fastrand::Rng {
    getrandom::u64().map_or_else(|_| fastrand::Rng::new(), fastrand::Rng::with_seed)
}

/// The no-repeat rules of one source. Tracks are identified by their URI.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Cycle {
    played: BTreeSet<String>,
    removed: BTreeSet<String>,
    /// Eligible tracks (the source without removed ones) at the last round.
    total: usize,
}

/// One freshly shuffled run of a cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Round {
    pub order: Vec<String>,
    /// Everything had played, so this round started the cycle over.
    pub new_cycle: bool,
}

impl Cycle {
    pub fn played(&self) -> usize {
        self.played.len().min(self.total)
    }

    pub fn total(&self) -> usize {
        self.total
    }

    pub fn removed(&self) -> usize {
        self.removed.len()
    }

    pub fn has_played(&self, uri: &str) -> bool {
        self.played.contains(uri)
    }

    pub fn is_removed(&self, uri: &str) -> bool {
        self.removed.contains(uri)
    }

    /// The tracks of `source` this cycle has not played, in a fresh random
    /// order, at most [`MAX_PLAYLIST_ITEMS`]; larger sources continue in the
    /// next round. When everything has played, a new cycle starts.
    ///
    /// `pending` tracks are still queued from the round before ("Keep
    /// playing"). They are left out, and when only they are unplayed, the new
    /// cycle starts without them, so no song plays twice in a row at the seam.
    pub fn next_round(
        &mut self,
        source: &[String],
        pending: &HashSet<&str>,
        rng: &mut fastrand::Rng,
    ) -> Round {
        let mut seen = HashSet::with_capacity(source.len());
        let eligible: Vec<&str> = source
            .iter()
            .map(String::as_str)
            .filter(|uri| !self.removed.contains(*uri) && seen.insert(*uri))
            .collect();
        // Tracks that left the source no longer count, so progress follows it.
        self.played.retain(|uri| seen.contains(uri.as_str()));
        self.total = eligible.len();

        let mut order: Vec<String> = eligible
            .iter()
            .filter(|uri| !self.played.contains(**uri) && !pending.contains(**uri))
            .map(|uri| (*uri).to_owned())
            .collect();
        let mut new_cycle = false;
        if order.is_empty() {
            let fresh: Vec<String> = eligible
                .iter()
                .filter(|uri| !pending.contains(**uri))
                .map(|uri| (*uri).to_owned())
                .collect();
            if !fresh.is_empty() {
                self.played.clear();
                new_cycle = true;
                order = fresh;
            }
        }
        rng.shuffle(&mut order);
        order.truncate(MAX_PLAYLIST_ITEMS);
        Round { order, new_cycle }
    }

    /// Counts a track as played. Removed tracks never count.
    pub fn mark_played(&mut self, uri: &str) -> bool {
        !self.removed.contains(uri) && self.played.insert(uri.to_owned())
    }

    /// Keeps the track out of every future round, until [`Cycle::restore_removed`].
    pub fn remove(&mut self, uri: &str) -> bool {
        if !self.removed.insert(uri.to_owned()) {
            return false;
        }
        self.played.remove(uri);
        self.total = self.total.saturating_sub(1);
        true
    }

    /// Brings removed tracks back as unplayed; the next round corrects the total.
    pub fn restore_removed(&mut self) -> usize {
        let count = self.removed.len();
        self.total += count;
        self.removed.clear();
        count
    }

    /// Starts the cycle over.
    pub fn reset(&mut self) {
        self.played.clear();
    }
}

/// Title and artist of a shuffled track. Short keys keep the file small for
/// 10,000 tracks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackNote {
    #[serde(rename = "n")]
    pub name: String,
    #[serde(rename = "a")]
    pub artist: String,
    #[serde(rename = "i", skip_serializing_if = "Option::is_none")]
    pub artist_id: Option<String>,
}

/// A track of "Up next".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpNext {
    pub uri: String,
    pub name: String,
    pub artist: String,
}

/// An order that is ready to be written to the shuffle playlist. Nothing
/// changes until the app commits it, so a failed write leaves the progress as
/// it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub source: String,
    pub order: Vec<String>,
    /// This round started the cycle over.
    pub new_cycle: bool,
    cycle: Cycle,
    played_before: BTreeSet<String>,
    notes: BTreeMap<String, TrackNote>,
}

impl Plan {
    /// Eligible tracks of the source, removed ones excluded.
    pub fn total(&self) -> usize {
        self.cycle.total
    }
}

/// What "Keep playing" does when a round nears its end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NextRound {
    Append(Plan),
    /// The playlist holds [`MAX_PLAYLIST_ITEMS`]; Shuffle & play starts over.
    Full,
    Nothing,
}

/// Everything True Shuffle remembers, stored as JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ShuffleState {
    /// Progress per source.
    pub cycles: BTreeMap<String, Cycle>,
    /// The one shuffle playlist, reused for every shuffle.
    pub playlist_id: Option<String>,
    /// The source whose order is in the shuffle playlist. `None` while no
    /// order counts, for example while a new one is written.
    pub active_source: Option<String>,
    /// Mirrors the shuffle playlist. With "Keep playing" it holds several
    /// rounds, so a URI can appear more than once.
    pub order: Vec<String>,
    /// Title and artist per URI in `order`.
    pub notes: BTreeMap<String, TrackNote>,
    /// Position in `order` of the track last confirmed playing from the playlist.
    pub playing_index: Option<usize>,
    /// The source picked in the window; Liked Songs when none.
    pub selected: Option<String>,
    /// Append the next round before the order runs out.
    pub keep_playing: bool,
}

impl Default for ShuffleState {
    fn default() -> Self {
        Self {
            cycles: BTreeMap::new(),
            playlist_id: None,
            active_source: None,
            order: Vec::new(),
            notes: BTreeMap::new(),
            playing_index: None,
            selected: None,
            keep_playing: true,
        }
    }
}

impl ShuffleState {
    /// Reads a stored state; a missing field takes its default and a broken
    /// file gives a fresh state.
    pub fn from_json(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn selected_source(&self) -> &str {
        self.selected.as_deref().unwrap_or(LIKED_SONGS)
    }

    pub fn selected_members(&self) -> Vec<&str> {
        mix_members(self.selected_source())
    }

    /// Picks a source or a mix.
    pub fn select(&mut self, source: &str) {
        if let Some(id) = mix_id(mix_members(source)) {
            self.selected = Some(id);
        }
    }

    /// Adds a source to the selection or takes it out; the last one stays.
    pub fn toggle_member(&mut self, member: &str) {
        let id = {
            let mut members: BTreeSet<&str> = self.selected_members().into_iter().collect();
            if members.contains(member) {
                if members.len() == 1 {
                    return;
                }
                members.remove(member);
            } else {
                members.insert(member);
            }
            mix_id(members)
        };
        if let Some(id) = id {
            self.selected = Some(id);
        }
    }

    /// When a selected source is gone (a deleted playlist), picks Liked Songs.
    /// Returns whether the selection changed.
    pub fn keep_selection_available(&mut self, available: &HashSet<&str>) -> bool {
        let gone = self
            .selected_members()
            .iter()
            .any(|member| !available.contains(member));
        if gone {
            self.selected = None;
        }
        gone
    }

    /// Mixes shuffled before whose sources all still exist, so the picker can
    /// return to them.
    pub fn mixes(&self, available: &HashSet<&str>) -> Vec<&str> {
        self.cycles
            .keys()
            .map(String::as_str)
            .filter(|id| {
                is_mix(id)
                    && mix_members(id)
                        .iter()
                        .all(|member| available.contains(member))
            })
            .collect()
    }

    /// Played and total tracks of a source.
    pub fn progress(&self, source: &str) -> (usize, usize) {
        self.cycles
            .get(source)
            .map_or((0, 0), |cycle| (cycle.played(), cycle.total()))
    }

    pub fn removed_count(&self, source: &str) -> usize {
        self.cycles.get(source).map_or(0, Cycle::removed)
    }

    pub fn reset(&mut self, source: &str) {
        if let Some(cycle) = self.cycles.get_mut(source) {
            cycle.reset();
        }
    }

    pub fn restore_removed(&mut self, source: &str) -> usize {
        self.cycles
            .get_mut(source)
            .map_or(0, Cycle::restore_removed)
    }

    /// An order in the shuffle playlist that can be kept.
    pub fn has_shuffle(&self) -> bool {
        self.playlist_id.is_some() && self.active_source.is_some() && !self.order.is_empty()
    }

    /// Plans Shuffle & play for `source` from its current tracks: the
    /// unplayed ones, freshly shuffled, with artists kept apart. `None` when
    /// there is nothing to play.
    pub fn plan(
        &self,
        source: &str,
        tracks: &[TrackInfo],
        rng: &mut fastrand::Rng,
    ) -> Option<Plan> {
        let mut cycle = self.cycles.get(source).cloned().unwrap_or_default();
        let played_before = cycle.played.clone();
        let uris: Vec<String> = tracks.iter().map(|track| track.uri.clone()).collect();
        let round = cycle.next_round(&uris, &HashSet::new(), rng);
        if round.order.is_empty() {
            return None;
        }
        let artists = artists_by_uri(tracks);
        let order = spread_artists(round.order, |uri| artists.get(uri).copied(), None, rng);
        Some(Plan {
            source: source.to_owned(),
            notes: notes_for(tracks, &order),
            order,
            new_cycle: round.new_cycle,
            cycle,
            played_before,
        })
    }

    /// Stores a planned order once it is in the shuffle playlist.
    pub fn commit(&mut self, plan: Plan) {
        self.cycles.insert(plan.source.clone(), plan.cycle);
        self.order = plan.order;
        self.notes = plan.notes;
        self.playing_index = None;
        self.active_source = Some(plan.source);
    }

    /// Where `uri` plays in the order. Later rounds repeat URIs, so the
    /// nearest fitting place wins: the next track, the same one, the previous
    /// one, then later, then anywhere (a jump back in Spotify).
    pub fn index_of(&self, uri: &str) -> Option<usize> {
        let holds = |index: usize| self.order.get(index).is_some_and(|item| item == uri);
        if let Some(current) = self.playing_index {
            let nearby = [Some(current + 1), Some(current), current.checked_sub(1)];
            if let Some(index) = nearby.into_iter().flatten().find(|index| holds(*index)) {
                return Some(index);
            }
            if let Some(offset) = self
                .order
                .iter()
                .skip(current + 2)
                .position(|item| item == uri)
            {
                return Some(current + 2 + offset);
            }
        }
        self.order.iter().position(|item| item == uri)
    }

    /// Counts a track that Spotify confirmed playing from the shuffle
    /// playlist. Returns its place in the order.
    pub fn record_play(&mut self, uri: &str) -> Option<usize> {
        let source = self.active_source.clone()?;
        let index = self.index_of(uri)?;
        self.cycles.entry(source).or_default().mark_played(uri);
        self.playing_index = Some(index);
        Some(index)
    }

    /// "Keep playing" appends the next round once the second-to-last or the
    /// last track of the order plays.
    pub fn needs_next_round(&self, index: usize) -> bool {
        index + 2 >= self.order.len()
    }

    /// Plans the next round while the track at `index` plays: the tracks
    /// still queued after it are left out, and the first new track's artist
    /// differs from the artist the order ends with.
    pub fn plan_next_round(
        &self,
        index: usize,
        tracks: &[TrackInfo],
        rng: &mut fastrand::Rng,
    ) -> NextRound {
        let Some(source) = self.active_source.as_deref() else {
            return NextRound::Nothing;
        };
        let capacity = MAX_PLAYLIST_ITEMS.saturating_sub(self.order.len());
        if capacity == 0 {
            return NextRound::Full;
        }
        let pending: HashSet<&str> = self
            .order
            .iter()
            .skip(index + 1)
            .map(String::as_str)
            .collect();
        let last_artist = self
            .order
            .last()
            .and_then(|uri| self.notes.get(uri))
            .and_then(|note| note.artist_id.as_deref());
        let mut cycle = self.cycles.get(source).cloned().unwrap_or_default();
        let played_before = cycle.played.clone();
        let uris: Vec<String> = tracks.iter().map(|track| track.uri.clone()).collect();
        let round = cycle.next_round(&uris, &pending, rng);
        if round.order.is_empty() {
            return NextRound::Nothing;
        }
        let artists = artists_by_uri(tracks);
        let mut order = spread_artists(
            round.order,
            |uri| artists.get(uri).copied(),
            last_artist,
            rng,
        );
        order.truncate(capacity);
        NextRound::Append(Plan {
            source: source.to_owned(),
            notes: notes_for(tracks, &order),
            order,
            new_cycle: round.new_cycle,
            cycle,
            played_before,
        })
    }

    /// Adds a planned round once it is in the shuffle playlist. Tracks that
    /// played while it was built still count. Returns false when the shuffle
    /// changed meanwhile.
    pub fn append(&mut self, plan: Plan) -> bool {
        if self.active_source.as_deref() != Some(plan.source.as_str()) {
            return false;
        }
        let mut cycle = plan.cycle;
        if let Some(current) = self.cycles.get(&plan.source) {
            for uri in current.played.difference(&plan.played_before) {
                cycle.mark_played(uri);
            }
        }
        self.cycles.insert(plan.source, cycle);
        self.order.extend(plan.order);
        self.notes.extend(plan.notes);
        true
    }

    /// Takes a track out of the active source for good and out of the order.
    pub fn remove(&mut self, uri: &str) -> bool {
        let Some(source) = self.active_source.clone() else {
            return false;
        };
        self.cycles.entry(source).or_default().remove(uri);
        if let Some(index) = self.playing_index {
            let kept_before = self.order[..index.min(self.order.len())]
                .iter()
                .filter(|item| *item != uri)
                .count();
            let removed_current = self.order.get(index).is_some_and(|item| item == uri);
            // The track before keeps its place, so the next one plays "next".
            self.playing_index = if removed_current {
                kept_before.checked_sub(1)
            } else {
                Some(kept_before)
            };
        }
        self.order.retain(|item| item != uri);
        self.notes.remove(uri);
        true
    }

    /// The order without removed tracks, for "Keep this shuffle".
    pub fn kept_order(&self) -> Vec<String> {
        let removed = self
            .active_source
            .as_deref()
            .and_then(|source| self.cycles.get(source));
        self.order
            .iter()
            .filter(|uri| removed.is_none_or(|cycle| !cycle.is_removed(uri)))
            .cloned()
            .collect()
    }

    /// The tracks after the one playing.
    pub fn up_next(&self, count: usize) -> Vec<UpNext> {
        let Some(index) = self.playing_index else {
            return Vec::new();
        };
        self.order
            .iter()
            .skip(index + 1)
            .take(count)
            .filter_map(|uri| {
                self.notes.get(uri).map(|note| UpNext {
                    uri: uri.clone(),
                    name: note.name.clone(),
                    artist: note.artist.clone(),
                })
            })
            .collect()
    }
}

fn artists_by_uri(tracks: &[TrackInfo]) -> HashMap<&str, &str> {
    tracks
        .iter()
        .filter_map(|track| Some((track.uri.as_str(), track.artist_id.as_deref()?)))
        .collect()
}

fn notes_for(tracks: &[TrackInfo], order: &[String]) -> BTreeMap<String, TrackNote> {
    let wanted: HashSet<&str> = order.iter().map(String::as_str).collect();
    tracks
        .iter()
        .filter(|track| wanted.contains(track.uri.as_str()))
        .map(|track| {
            (
                track.uri.clone(),
                TrackNote {
                    name: track.name.clone(),
                    artist: track.artist.clone(),
                    artist_id: track.artist_id.clone(),
                },
            )
        })
        .collect()
}

/// Reorders `order` so no two neighbours share a main artist, and the first
/// track's artist differs from `previous`, the artist right before it.
///
/// Each step picks an artist at random, weighted by how many tracks they have
/// left, among those that keep a valid arrangement possible for the rest.
/// When one artist has more than half of the tracks, no such arrangement
/// exists; their tracks then come in runs as even as possible, with the other
/// tracks in between. The order within one artist stays random, and tracks
/// without an artist count as an artist of their own.
pub fn spread_artists<'a>(
    order: Vec<String>,
    artist_of: impl Fn(&str) -> Option<&'a str>,
    previous: Option<&str>,
    rng: &mut fastrand::Rng,
) -> Vec<String> {
    let total = order.len();
    let mut groups: Vec<Vec<String>> = Vec::new();
    let mut group_of: HashMap<&'a str, usize> = HashMap::new();
    for uri in order {
        let group = match artist_of(&uri) {
            Some(artist) => *group_of.entry(artist).or_insert_with(|| {
                groups.push(Vec::new());
                groups.len() - 1
            }),
            None => {
                groups.push(Vec::new());
                groups.len() - 1
            }
        };
        groups[group].push(uri);
    }
    for group in &mut groups {
        rng.shuffle(group);
    }

    let mut counts: Vec<usize> = groups.iter().map(Vec::len).collect();
    let mut weights = Weights::new(&counts);
    // How many artists have each count, to follow the largest count cheaply.
    let mut with_count = vec![0usize; total + 1];
    for &count in &counts {
        with_count[count] += 1;
    }
    let mut largest = counts.iter().copied().max().unwrap_or(0);
    let mut dominant: Option<usize> = None;
    let mut last = previous.and_then(|artist| group_of.get(artist).copied());
    let mut run = usize::from(last.is_some());

    let mut spread = Vec::with_capacity(total);
    for remaining in (1..=total).rev() {
        while largest > 0 && with_count[largest] == 0 {
            largest -= 1;
        }
        let pick = if largest * 2 > remaining {
            // One artist has more than half of what is left: only placing it
            // now keeps the rest possible. Two artists cannot both be above half.
            let top = match dominant {
                Some(group) if counts[group] == largest => group,
                _ => counts
                    .iter()
                    .position(|&count| count == largest)
                    .unwrap_or_default(),
            };
            dominant = Some(top);
            let others = remaining - counts[top];
            if last != Some(top) || others == 0 {
                top
            } else {
                // It just played and has to follow itself sooner or later:
                // keep its runs even.
                let target = (run + counts[top]).div_ceil(others + 1);
                if run < target {
                    top
                } else {
                    weights.sample_excluding(top, counts[top], rng)
                }
            }
        } else {
            match last {
                Some(group) => weights.sample_excluding(group, counts[group], rng),
                None => weights.sample(rng),
            }
        };
        let Some(uri) = groups[pick].pop() else {
            break;
        };
        with_count[counts[pick]] -= 1;
        counts[pick] -= 1;
        with_count[counts[pick]] += 1;
        weights.decrement(pick);
        run = if last == Some(pick) { run + 1 } else { 1 };
        last = Some(pick);
        spread.push(uri);
    }
    spread
}

/// Track counts per artist in a Fenwick tree, for weighted picks in O(log n).
struct Weights {
    tree: Vec<usize>,
}

impl Weights {
    fn new(counts: &[usize]) -> Self {
        let mut tree = vec![0; counts.len() + 1];
        for (index, &count) in counts.iter().enumerate() {
            let node = index + 1;
            tree[node] += count;
            let parent = node + (node & node.wrapping_neg());
            if parent < tree.len() {
                let value = tree[node];
                tree[parent] += value;
            }
        }
        Self { tree }
    }

    fn decrement(&mut self, index: usize) {
        let mut node = index + 1;
        while node < self.tree.len() {
            self.tree[node] -= 1;
            node += node & node.wrapping_neg();
        }
    }

    /// The sum of the first `end` counts.
    fn prefix(&self, end: usize) -> usize {
        let mut node = end;
        let mut sum = 0;
        while node > 0 {
            sum += self.tree[node];
            node -= node & node.wrapping_neg();
        }
        sum
    }

    fn total(&self) -> usize {
        self.prefix(self.tree.len() - 1)
    }

    /// The index whose share of the total contains `target`.
    fn find(&self, target: usize) -> usize {
        let len = self.tree.len() - 1;
        let mut position = 0;
        let mut rest = target;
        let mut step = if len == 0 { 0 } else { 1 << len.ilog2() };
        while step > 0 {
            let next = position + step;
            if next <= len && self.tree[next] <= rest {
                position = next;
                rest -= self.tree[next];
            }
            step >>= 1;
        }
        position
    }

    fn sample(&self, rng: &mut fastrand::Rng) -> usize {
        self.find(rng.usize(..self.total().max(1)))
    }

    /// A pick weighted by count, never `excluded` unless nothing else is left.
    fn sample_excluding(
        &self,
        excluded: usize,
        excluded_count: usize,
        rng: &mut fastrand::Rng,
    ) -> usize {
        let weight = self.total().saturating_sub(excluded_count);
        if weight == 0 {
            return excluded;
        }
        let mut target = rng.usize(..weight);
        if target >= self.prefix(excluded) {
            target += excluded_count;
        }
        self.find(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uris(range: std::ops::Range<usize>) -> Vec<String> {
        range.map(|n| format!("spotify:track:{n}")).collect()
    }

    fn track(n: usize, artist: &str) -> TrackInfo {
        TrackInfo {
            uri: format!("spotify:track:{n}"),
            name: format!("Song {n}"),
            artist: format!("Artist {artist}"),
            artist_id: (!artist.is_empty()).then(|| artist.to_owned()),
        }
    }

    fn no_pending() -> HashSet<&'static str> {
        HashSet::new()
    }

    #[test]
    fn mix_ids_do_not_depend_on_the_order() {
        assert_eq!(
            mix_id(["b", "liked", "a"]),
            mix_id(["liked", "a", "b", "a"])
        );
        assert_eq!(
            mix_id(["b", "liked", "a"]).as_deref(),
            Some("mix:a,b,liked")
        );
        assert_eq!(mix_id(["liked"]).as_deref(), Some("liked"));
        assert_eq!(mix_id(["liked", "liked"]).as_deref(), Some("liked"));
        assert_eq!(mix_id(Vec::<&str>::new()), None);
        assert_eq!(mix_id(["", "a,b", "mix:x"]), None);
        assert_eq!(mix_members("mix:a,b,liked"), ["a", "b", "liked"]);
        assert_eq!(mix_members("abc"), ["abc"]);
        assert!(is_mix("mix:a,b") && !is_mix("liked"));
    }

    #[test]
    fn a_mix_counts_a_song_once() {
        let liked = vec![track(1, "a"), track(2, "b"), track(3, "c")];
        let playlist = vec![track(3, "c"), track(4, "d"), track(1, "a")];
        let merged = merge_sources([liked, playlist]);
        let ids: Vec<&str> = merged.iter().map(|track| track.uri.as_str()).collect();
        assert_eq!(
            ids,
            [
                "spotify:track:1",
                "spotify:track:2",
                "spotify:track:3",
                "spotify:track:4"
            ]
        );

        let state = ShuffleState::default();
        let mut rng = fastrand::Rng::with_seed(1);
        let mix = mix_id(["liked", "p1"]).expect("mix");
        let plan = state.plan(&mix, &merged, &mut rng).expect("plan");
        assert_eq!(plan.order.len(), 4);
        assert_eq!(plan.total(), 4);
    }

    #[test]
    fn no_track_repeats_before_every_track_played() {
        let source = uris(0..25);
        let mut cycle = Cycle::default();
        let mut rng = fastrand::Rng::with_seed(7);
        for _ in 0..4 {
            let mut heard = HashSet::new();
            // Listen in uneven chunks; every round offers only unheard tracks.
            while heard.len() < source.len() {
                let round = cycle.next_round(&source, &no_pending(), &mut rng);
                assert!(!round.order.is_empty());
                for uri in round.order.iter().take(7) {
                    assert!(heard.insert(uri.clone()), "{uri} played twice in one cycle");
                    assert!(cycle.mark_played(uri));
                }
                assert_eq!(cycle.played(), heard.len());
                assert_eq!(cycle.total(), 25);
            }
            assert_eq!(cycle.played(), 25);
            let round = cycle.next_round(&source, &no_pending(), &mut rng);
            assert!(round.new_cycle);
            assert_eq!(round.order.len(), 25);
            assert_eq!(cycle.played(), 0);
            cycle.reset();
        }
    }

    #[test]
    fn rounds_are_shuffled_fairly() {
        let source = uris(0..4);
        let mut first = [0usize; 4];
        for seed in 0..4_000 {
            let mut cycle = Cycle::default();
            let mut rng = fastrand::Rng::with_seed(seed);
            let round = cycle.next_round(&source, &no_pending(), &mut rng);
            let index = source
                .iter()
                .position(|uri| *uri == round.order[0])
                .expect("known");
            first[index] += 1;
        }
        for count in first {
            assert!((850..=1_150).contains(&count), "{first:?}");
        }
    }

    #[test]
    fn progress_follows_the_source() {
        let mut cycle = Cycle::default();
        let mut rng = fastrand::Rng::with_seed(3);
        let source = uris(0..10);
        cycle.next_round(&source, &no_pending(), &mut rng);
        for uri in &source[..4] {
            cycle.mark_played(uri);
        }
        // Two played tracks left the playlist, three new ones joined.
        let mut changed: Vec<String> = source[2..].to_vec();
        changed.extend(uris(10..13));
        let round = cycle.next_round(&changed, &no_pending(), &mut rng);
        assert_eq!((cycle.played(), cycle.total()), (2, 11));
        assert_eq!(round.order.len(), 9);
        for uri in uris(10..13) {
            assert!(round.order.contains(&uri), "new tracks join as unplayed");
        }
        assert!(!round.order.contains(&source[2]) && !round.order.contains(&source[3]));
        assert!(!round.new_cycle);
    }

    #[test]
    fn duplicates_in_a_source_count_once() {
        let mut source = uris(0..3);
        source.extend(uris(0..3));
        let mut cycle = Cycle::default();
        let round = cycle.next_round(&source, &no_pending(), &mut fastrand::Rng::with_seed(1));
        assert_eq!(round.order.len(), 3);
        assert_eq!(cycle.total(), 3);
    }

    #[test]
    fn removed_tracks_stay_out_until_restored() {
        let source = uris(0..5);
        let mut cycle = Cycle::default();
        let mut rng = fastrand::Rng::with_seed(11);
        cycle.next_round(&source, &no_pending(), &mut rng);
        cycle.mark_played(&source[0]);
        assert!(cycle.remove(&source[0]));
        assert!(!cycle.remove(&source[0]));
        assert!(cycle.remove(&source[1]));
        assert_eq!((cycle.played(), cycle.total(), cycle.removed()), (0, 3, 2));
        assert!(!cycle.mark_played(&source[1]), "removed tracks never count");

        for _ in 0..3 {
            let round = cycle.next_round(&source, &no_pending(), &mut rng);
            assert_eq!(round.order.len(), 3);
            assert!(!round.order.contains(&source[0]) && !round.order.contains(&source[1]));
            for uri in &round.order {
                cycle.mark_played(uri);
            }
        }
        assert_eq!(cycle.restore_removed(), 2);
        assert_eq!(cycle.total(), 5);
        let round = cycle.next_round(&source, &no_pending(), &mut rng);
        assert!(round.order.contains(&source[0]) && round.order.contains(&source[1]));
        assert_eq!(cycle.total(), 5);
    }

    #[test]
    fn queued_tracks_skip_the_next_round() {
        let source = uris(0..6);
        let mut cycle = Cycle::default();
        let mut rng = fastrand::Rng::with_seed(5);
        let first = cycle.next_round(&source, &no_pending(), &mut rng);
        // Four played, the fifth plays now and the sixth is still queued.
        for uri in &first.order[..5] {
            cycle.mark_played(uri);
        }
        let queued: HashSet<&str> = [first.order[5].as_str()].into();
        let next = cycle.next_round(&source, &queued, &mut rng);
        assert!(next.new_cycle, "only the queued track was unplayed");
        assert_eq!(next.order.len(), 5);
        assert!(!next.order.contains(&first.order[5]));

        // With unplayed tracks beyond the queued ones, the cycle goes on.
        let mut cycle = Cycle::default();
        let first = cycle.next_round(&source, &no_pending(), &mut rng);
        cycle.mark_played(&first.order[0]);
        let queued: HashSet<&str> = first.order[1..3].iter().map(String::as_str).collect();
        let next = cycle.next_round(&source, &queued, &mut rng);
        assert!(!next.new_cycle);
        assert_eq!(next.order.len(), 3);
        assert_eq!(cycle.played(), 1);

        // A source of one queued song gives nothing and forgets nothing.
        let single = uris(0..1);
        let mut cycle = Cycle::default();
        cycle.next_round(&single, &no_pending(), &mut rng);
        let queued: HashSet<&str> = [single[0].as_str()].into();
        let next = cycle.next_round(&single, &queued, &mut rng);
        assert!(next.order.is_empty() && !next.new_cycle);
    }

    #[test]
    fn rounds_fit_into_one_playlist() {
        let source = uris(0..10_050);
        let mut cycle = Cycle::default();
        let mut rng = fastrand::Rng::with_seed(2);
        let round = cycle.next_round(&source, &no_pending(), &mut rng);
        assert_eq!(round.order.len(), MAX_PLAYLIST_ITEMS);
        for uri in &round.order {
            cycle.mark_played(uri);
        }
        let rest = cycle.next_round(&source, &no_pending(), &mut rng);
        assert_eq!(
            rest.order.len(),
            50,
            "larger sources continue in the next shuffle"
        );
        assert!(!rest.new_cycle);
    }

    fn artist(uri: &str, tracks: &[TrackInfo]) -> Option<String> {
        tracks
            .iter()
            .find(|track| track.uri == uri)
            .and_then(|track| track.artist_id.clone())
    }

    fn neighbours_sharing_an_artist(order: &[String], tracks: &[TrackInfo]) -> usize {
        let artists: HashMap<&str, Option<&str>> = tracks
            .iter()
            .map(|track| (track.uri.as_str(), track.artist_id.as_deref()))
            .collect();
        let artist = |uri: &String| artists.get(uri.as_str()).copied().flatten();
        order
            .windows(2)
            .filter(|pair| {
                let first = artist(&pair[0]);
                first.is_some() && first == artist(&pair[1])
            })
            .count()
    }

    fn spread(tracks: &[TrackInfo], previous: Option<&str>, seed: u64) -> Vec<String> {
        let artists = artists_by_uri(tracks);
        let mut order: Vec<String> = tracks.iter().map(|track| track.uri.clone()).collect();
        let mut rng = fastrand::Rng::with_seed(seed);
        rng.shuffle(&mut order);
        spread_artists(order, |uri| artists.get(uri).copied(), previous, &mut rng)
    }

    /// Tracks for the given number of songs per artist.
    fn library(counts: &[usize]) -> Vec<TrackInfo> {
        let mut tracks = Vec::new();
        for (artist, &count) in counts.iter().enumerate() {
            for _ in 0..count {
                tracks.push(track(tracks.len(), &format!("artist{artist}")));
            }
        }
        tracks
    }

    fn assert_permutation(order: &[String], tracks: &[TrackInfo]) {
        let mut sorted = order.to_vec();
        sorted.sort();
        let mut expected: Vec<String> = tracks.iter().map(|track| track.uri.clone()).collect();
        expected.sort();
        assert_eq!(sorted, expected);
    }

    #[test]
    fn keeps_artists_apart_whenever_possible() {
        let mut rng = fastrand::Rng::with_seed(99);
        for seed in 0..400 {
            let artists = rng.usize(1..8);
            let counts: Vec<usize> = (0..artists).map(|_| rng.usize(1..9)).collect();
            let tracks = library(&counts);
            let order = spread(&tracks, None, seed);
            assert_permutation(&order, &tracks);
            let largest = counts.iter().copied().max().unwrap_or(0);
            if largest <= tracks.len().div_ceil(2) {
                assert_eq!(
                    neighbours_sharing_an_artist(&order, &tracks),
                    0,
                    "counts {counts:?}, seed {seed}"
                );
            }
        }
    }

    #[test]
    fn the_first_track_differs_from_the_round_before() {
        let mut rng = fastrand::Rng::with_seed(4);
        for seed in 0..300 {
            let counts: Vec<usize> = (0..rng.usize(2..6)).map(|_| rng.usize(1..6)).collect();
            let tracks = library(&counts);
            let previous = format!("artist{}", rng.usize(0..counts.len()));
            let previous_count = counts[previous[6..].parse::<usize>().expect("index")];
            let order = spread(&tracks, Some(&previous), seed);
            assert_permutation(&order, &tracks);
            let largest = counts.iter().copied().max().unwrap_or(0);
            let feasible =
                previous_count <= tracks.len() / 2 && largest <= tracks.len().div_ceil(2);
            if feasible {
                assert_ne!(
                    artist(&order[0], &tracks).as_deref(),
                    Some(previous.as_str()),
                    "{counts:?}"
                );
                assert_eq!(
                    neighbours_sharing_an_artist(&order, &tracks),
                    0,
                    "{counts:?}"
                );
            }
        }
    }

    #[test]
    fn a_dominant_artist_comes_in_even_runs() {
        // 40 songs by one artist and 10 by others: runs of three or four.
        let mut counts = vec![40];
        counts.extend([1; 10]);
        let tracks = library(&counts);
        for seed in 0..50 {
            let order = spread(&tracks, None, seed);
            assert_permutation(&order, &tracks);
            assert_eq!(neighbours_sharing_an_artist(&order, &tracks), 40 - 11);
            let mut runs = Vec::new();
            let mut run = 0;
            for uri in &order {
                if artist(uri, &tracks).as_deref() == Some("artist0") {
                    run += 1;
                } else {
                    runs.push(run);
                    run = 0;
                }
            }
            runs.push(run);
            assert_eq!(runs.len(), 11, "every other song separates two runs");
            let shortest = runs.iter().min().copied();
            let longest = runs.iter().max().copied();
            assert_eq!((shortest, longest), (Some(3), Some(4)), "{runs:?}");
        }
        // Six against two: A A B A A B A A.
        let tracks = library(&[6, 2]);
        let order = spread(&tracks, None, 1);
        let pattern: String = order
            .iter()
            .map(|uri| {
                if artist(uri, &tracks).as_deref() == Some("artist0") {
                    'A'
                } else {
                    'B'
                }
            })
            .collect();
        assert_eq!(pattern, "AABAABAA");
    }

    #[test]
    fn tracks_without_an_artist_are_their_own() {
        let tracks: Vec<TrackInfo> = (0..6).map(|n| track(n, "")).collect();
        let order = spread(&tracks, Some(""), 3);
        assert_permutation(&order, &tracks);
        let mut mixed = library(&[3, 3]);
        mixed.extend((6..9).map(|n| track(n, "")));
        for seed in 0..50 {
            let order = spread(&mixed, None, seed);
            assert_permutation(&order, &mixed);
            assert_eq!(neighbours_sharing_an_artist(&order, &mixed), 0);
        }
        assert!(spread(&[], Some("x"), 1).is_empty());
    }

    #[test]
    fn spreads_ten_thousand_tracks_quickly() {
        let mut rng = fastrand::Rng::with_seed(8);
        let tracks: Vec<TrackInfo> = (0..MAX_PLAYLIST_ITEMS)
            .map(|n| track(n, &format!("a{}", rng.usize(0..3_000))))
            .collect();
        let distinct: Vec<TrackInfo> = (0..MAX_PLAYLIST_ITEMS)
            .map(|n| track(n, &format!("d{n}")))
            .collect();
        let dominated = library(&[9_000, 1_000]);
        let started = std::time::Instant::now();
        let order = spread(&tracks, None, 1);
        let distinct_order = spread(&distinct, None, 2);
        let dominated_order = spread(&dominated, None, 3);
        let elapsed = started.elapsed();
        assert!(elapsed < std::time::Duration::from_secs(5), "{elapsed:?}");
        assert_eq!(order.len(), MAX_PLAYLIST_ITEMS);
        assert_eq!(neighbours_sharing_an_artist(&order, &tracks), 0);
        assert_eq!(distinct_order.len(), MAX_PLAYLIST_ITEMS);
        assert_eq!(
            neighbours_sharing_an_artist(&dominated_order, &dominated),
            9_000 - 1_001
        );
    }

    #[test]
    fn weights_pick_by_count() {
        let mut weights = Weights::new(&[0, 3, 0, 1, 2]);
        assert_eq!(weights.total(), 6);
        let picks: Vec<usize> = (0..6).map(|target| weights.find(target)).collect();
        assert_eq!(picks, [1, 1, 1, 3, 4, 4]);
        weights.decrement(1);
        assert_eq!((weights.total(), weights.prefix(2)), (5, 2));
        let mut rng = fastrand::Rng::with_seed(1);
        for _ in 0..100 {
            assert_ne!(weights.sample_excluding(4, 2, &mut rng), 4);
            assert_ne!(weights.sample(&mut rng), 0);
        }
        assert_eq!(Weights::new(&[0, 0]).sample_excluding(1, 0, &mut rng), 1);
    }

    fn library_state(tracks: &[TrackInfo], seed: u64) -> ShuffleState {
        let mut state = ShuffleState {
            playlist_id: Some("shuffle".into()),
            ..ShuffleState::default()
        };
        let plan = state
            .plan(LIKED_SONGS, tracks, &mut fastrand::Rng::with_seed(seed))
            .expect("something to play");
        state.commit(plan);
        state
    }

    #[test]
    fn plans_change_nothing_until_committed() {
        let tracks = library(&[3, 2, 2]);
        let mut state = ShuffleState {
            playlist_id: Some("shuffle".into()),
            ..ShuffleState::default()
        };
        let before = state.clone();
        let plan = state
            .plan(LIKED_SONGS, &tracks, &mut fastrand::Rng::with_seed(1))
            .expect("plan");
        assert_eq!(state, before);
        assert_eq!(plan.order.len(), 7);
        assert_eq!(neighbours_sharing_an_artist(&plan.order, &tracks), 0);
        state.commit(plan.clone());
        assert_eq!(state.order, plan.order);
        assert_eq!(state.active_source.as_deref(), Some(LIKED_SONGS));
        assert_eq!(state.progress(LIKED_SONGS), (0, 7));
        assert!(state.has_shuffle());
        assert_eq!(state.notes.len(), 7);
        assert!(state
            .plan("empty", &[], &mut fastrand::Rng::with_seed(1))
            .is_none());
    }

    #[test]
    fn counts_plays_of_the_shuffle_only() {
        let tracks = library(&[1, 1, 1, 1]);
        let mut state = library_state(&tracks, 3);
        let order = state.order.clone();
        assert_eq!(state.record_play(&order[0]), Some(0));
        assert_eq!(state.record_play(&order[1]), Some(1));
        assert_eq!(state.progress(LIKED_SONGS), (2, 4));
        assert_eq!(state.record_play("spotify:track:elsewhere"), None);

        state.active_source = None;
        assert_eq!(
            state.record_play(&order[2]),
            None,
            "no order counts while none is active"
        );
        assert_eq!(state.progress(LIKED_SONGS), (2, 4));
    }

    #[test]
    fn finds_the_right_place_when_rounds_repeat_tracks() {
        let mut state = ShuffleState {
            order: ["a", "b", "c", "b", "a", "c"].map(String::from).to_vec(),
            ..ShuffleState::default()
        };
        assert_eq!(state.index_of("b"), Some(1));
        state.playing_index = Some(2);
        assert_eq!(state.index_of("b"), Some(3), "the next track");
        assert_eq!(state.index_of("c"), Some(2), "the same track");
        assert_eq!(state.index_of("a"), Some(4), "later, in the next round");
        state.playing_index = Some(4);
        assert_eq!(state.index_of("b"), Some(3), "one back");
        assert_eq!(state.index_of("c"), Some(5));
        state.playing_index = Some(5);
        assert_eq!(state.index_of("a"), Some(4));
        assert_eq!(state.index_of("b"), Some(1), "a jump back");
        assert_eq!(state.index_of("z"), None);
    }

    #[test]
    fn keep_playing_appends_the_next_round_without_a_seam_repeat() {
        let tracks = library(&[2, 2, 2]);
        let mut state = library_state(&tracks, 21);
        let mut rng = fastrand::Rng::with_seed(5);
        let first = state.order.clone();
        for uri in &first[..5] {
            state.record_play(uri);
        }
        let index = state.playing_index.expect("playing");
        assert_eq!(index, 4);
        assert!(state.needs_next_round(index));
        assert!(!state.needs_next_round(3));

        let NextRound::Append(plan) = state.plan_next_round(index, &tracks, &mut rng) else {
            panic!("expected a round");
        };
        assert!(plan.new_cycle);
        assert!(
            !plan.order.contains(&first[5]),
            "the queued track skips this round"
        );
        assert_eq!(plan.order.len(), 5);
        // The new round's first artist differs from the last queued track's.
        assert_ne!(artist(&plan.order[0], &tracks), artist(&first[5], &tracks));

        // The queued track plays while the round is written; it still counts.
        state.record_play(&first[5]);
        assert!(state.append(plan));
        assert_eq!(state.order.len(), 11);
        assert_eq!(neighbours_sharing_an_artist(&state.order, &tracks), 0);
        assert_eq!(
            state.progress(LIKED_SONGS),
            (1, 6),
            "a new cycle with the queued track played"
        );
        assert_eq!(state.record_play(&state.order[6].clone()), Some(6));
    }

    #[test]
    fn appending_stops_when_the_playlist_is_full_or_the_shuffle_changed() {
        let tracks = library(&[1, 1]);
        let mut state = library_state(&tracks, 1);
        let full = state.order.clone();
        state.order = full
            .iter()
            .cycle()
            .take(MAX_PLAYLIST_ITEMS)
            .cloned()
            .collect();
        let mut rng = fastrand::Rng::with_seed(1);
        assert_eq!(
            state.plan_next_round(MAX_PLAYLIST_ITEMS - 1, &tracks, &mut rng),
            NextRound::Full
        );

        let mut state = library_state(&tracks, 1);
        let NextRound::Append(plan) = state.plan_next_round(1, &tracks, &mut rng) else {
            panic!("expected a round");
        };
        state.active_source = Some("other".into());
        assert!(!state.append(plan));
        assert_eq!(state.order.len(), 2);

        let mut idle = ShuffleState::default();
        assert_eq!(
            idle.plan_next_round(0, &tracks, &mut rng),
            NextRound::Nothing
        );
        idle.active_source = Some(LIKED_SONGS.into());
        assert_eq!(idle.plan_next_round(0, &[], &mut rng), NextRound::Nothing);
    }

    #[test]
    fn removing_a_track_keeps_it_out_and_the_place_right() {
        let tracks = library(&[1, 1, 1, 1, 1]);
        let mut state = library_state(&tracks, 9);
        let order = state.order.clone();
        state.record_play(&order[0]);
        state.record_play(&order[1]);
        assert!(state.remove(&order[1]));
        assert_eq!(state.order.len(), 4);
        assert!(!state.notes.contains_key(&order[1]));
        assert_eq!(
            state.playing_index,
            Some(0),
            "the track before keeps its place"
        );
        assert_eq!(state.index_of(&order[2]), Some(1));
        assert_eq!(state.up_next(UP_NEXT).len(), 3);
        assert_eq!(state.up_next(UP_NEXT)[0].uri, order[2]);
        assert_eq!(state.progress(LIKED_SONGS), (1, 4));
        assert_eq!(state.removed_count(LIKED_SONGS), 1);
        assert!(!state.kept_order().contains(&order[1]));

        let mut rng = fastrand::Rng::with_seed(3);
        let replan = state.plan(LIKED_SONGS, &tracks, &mut rng).expect("plan");
        assert!(!replan.order.contains(&order[1]) && !replan.order.contains(&order[0]));
        assert_eq!(state.restore_removed(LIKED_SONGS), 1);
        let restored = state.plan(LIKED_SONGS, &tracks, &mut rng).expect("plan");
        assert!(restored.order.contains(&order[1]));

        // Removing the first track leaves no place before it.
        let mut state = library_state(&tracks, 2);
        let first = state.order[0].clone();
        state.record_play(&first);
        state.remove(&first);
        assert_eq!(state.playing_index, None);
        assert!(!ShuffleState::default().remove(&first), "nothing is active");
    }

    #[test]
    fn up_next_shows_the_next_three() {
        let tracks = library(&[1, 1, 1, 1, 1, 1]);
        let mut state = library_state(&tracks, 4);
        assert!(
            state.up_next(UP_NEXT).is_empty(),
            "nothing confirmed playing yet"
        );
        let order = state.order.clone();
        state.record_play(&order[1]);
        let next: Vec<String> = state
            .up_next(UP_NEXT)
            .into_iter()
            .map(|item| item.uri)
            .collect();
        assert_eq!(next, order[2..5]);
        let first = &state.up_next(1)[0];
        assert!(first.name.starts_with("Song ") && first.artist.starts_with("Artist "));
        state.record_play(&order[4]);
        assert_eq!(state.up_next(UP_NEXT).len(), 1);
    }

    #[test]
    fn picks_sources_and_mixes() {
        let mut state = ShuffleState::default();
        assert_eq!(state.selected_source(), LIKED_SONGS);
        state.toggle_member("p1");
        assert_eq!(state.selected_source(), "mix:liked,p1");
        state.toggle_member("liked");
        assert_eq!(state.selected_source(), "p1");
        state.toggle_member("p1");
        assert_eq!(state.selected_source(), "p1", "the last source stays");
        state.select("mix:p2,liked");
        assert_eq!(state.selected_members(), ["liked", "p2"]);

        state.cycles.insert("mix:liked,p2".into(), Cycle::default());
        state
            .cycles
            .insert("mix:gone,liked".into(), Cycle::default());
        state.cycles.insert("p1".into(), Cycle::default());
        let available: HashSet<&str> = ["liked", "p1", "p2"].into();
        assert_eq!(state.mixes(&available), ["mix:liked,p2"]);
        assert!(!state.keep_selection_available(&available));
        state.select("mix:gone,liked");
        assert!(state.keep_selection_available(&available));
        assert_eq!(state.selected_source(), LIKED_SONGS);
    }

    #[test]
    fn reset_starts_a_source_over() {
        let tracks = library(&[1, 1, 1]);
        let mut state = library_state(&tracks, 1);
        let order = state.order.clone();
        state.record_play(&order[0]);
        state.reset("unknown");
        assert_eq!(state.progress(LIKED_SONGS), (1, 3));
        state.reset(LIKED_SONGS);
        assert_eq!(state.progress(LIKED_SONGS), (0, 3));
    }

    #[test]
    fn stores_and_reads_its_state() {
        let tracks = library(&[2, 1]);
        let mut state = library_state(&tracks, 6);
        let first = state.order[0].clone();
        state.record_play(&first);
        state.keep_playing = false;
        let json = state.to_json().expect("json");
        assert!(json.contains("\"n\":\"Song"), "{json}");
        assert_eq!(ShuffleState::from_json(&json), state);

        // Older or partial files keep their defaults; broken ones start fresh.
        let partial = ShuffleState::from_json(r#"{"playlistId": "abc", "order": ["x"]}"#);
        assert_eq!(partial.playlist_id.as_deref(), Some("abc"));
        assert!(partial.keep_playing);
        assert_eq!(
            ShuffleState::from_json("{ not json"),
            ShuffleState::default()
        );
    }
}
