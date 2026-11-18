//! Finding a song from a few of its words, for Sing to find.
//!
//! LRCLIB searches titles and artists, not lyrics, so a [`SongSearch`] provider
//! turns the words someone sang into candidate songs, such as
//! [`crate::genius::GeniusClient`]. [`find_sung`] loads every candidate's
//! lyrics and ranks the candidates by how well they explain what was heard.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use lyrics_core::alignment::{self, Alignment, HeardWord};
use lyrics_core::{Lyrics, SyncLevel};
use serde::Serialize;
use thiserror::Error;

use crate::{LookupOptions, LyricsOutcome, LyricsService};

/// A song a search provider found for some words of its lyrics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SongCandidate {
    pub title: String,
    pub artist: String,
    pub artwork_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SearchError {
    #[error("the song search needs an access token")]
    NoToken,
    #[error("the song search refused the access token")]
    Unauthorized,
    #[error("no internet connection")]
    Offline,
    #[error("the song search is busy; try again in {} s", .retry_after.as_secs())]
    RateLimited { retry_after: Duration },
    #[error("the song search is not available right now (HTTP {0})")]
    Http(u16),
    #[error("the song search sent an answer that could not be read")]
    InvalidResponse,
    #[error("the request failed: {0}")]
    Network(String),
}

pub type SearchFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<SongCandidate>, SearchError>> + Send + 'a>>;

/// Turns words of a song's lyrics into the songs they may come from.
pub trait SongSearch: Send + Sync {
    /// Short name for diagnostics, such as `genius`.
    fn name(&self) -> &'static str;

    /// Songs whose lyrics may contain `words`, the most likely first. Only the
    /// words are sent, nothing else about the listener.
    fn search<'a>(&'a self, words: &'a str) -> SearchFuture<'a>;
}

/// A candidate with its lyrics and where the heard words stand in them.
#[derive(Debug, Clone, PartialEq)]
pub struct SungMatch {
    pub candidate: SongCandidate,
    pub lyrics: Lyrics,
    /// True for your own lyrics, false for LRCLIB.
    pub own: bool,
    /// Seconds, when LRCLIB knows the length of the song.
    pub duration: Option<f64>,
    pub album: Option<String>,
    pub alignment: Alignment,
}

/// What [`find_sung`] found.
#[derive(Debug, Clone, PartialEq)]
pub struct SungSearch {
    /// Candidates with synced lyrics, the best match first.
    pub matches: Vec<SungMatch>,
    /// How many songs the provider suggested.
    pub candidates: usize,
    /// A lyrics lookup that failed for a reason worth telling, such as being offline.
    pub lookup_problem: Option<LyricsOutcome>,
}

/// Asks `search` for songs with `query` in their lyrics, loads the synced
/// lyrics of the first `limit` of them and ranks them by how well they explain
/// `heard`. `track_key` gives the key of a title and artist, for your own lyrics.
pub async fn find_sung(
    search: &dyn SongSearch,
    service: &LyricsService,
    query: &str,
    heard: &[HeardWord],
    limit: usize,
    options: LookupOptions,
    track_key: impl Fn(&str, &str) -> String,
) -> Result<SungSearch, SearchError> {
    let candidates = search.search(query).await?;
    let suggested = candidates.len();
    let mut loaded = Vec::new();
    let mut lookup_problem = None;
    for candidate in candidates.into_iter().take(limit) {
        let key = track_key(&candidate.title, &candidate.artist);
        let found = service
            .song_lyrics(&key, &candidate.title, &candidate.artist, options)
            .await;
        let (lyrics, own) = match found.outcome {
            LyricsOutcome::Local { lyrics } => (lyrics, true),
            LyricsOutcome::Online { lyrics } => (lyrics, false),
            other @ (LyricsOutcome::Offline
            | LyricsOutcome::RateLimited { .. }
            | LyricsOutcome::Failed { .. }) => {
                lookup_problem.get_or_insert(other);
                continue;
            }
            _ => continue,
        };
        // Without timing the lyrics cannot follow the singer.
        if lyrics.sync == SyncLevel::Unsynced || lyrics.instrumental {
            continue;
        }
        loaded.push((candidate, lyrics, own, found.duration, found.album));
    }
    let words: Vec<_> = loaded
        .iter()
        .map(|(_, lyrics, ..)| alignment::song_words(&lyrics.lines))
        .collect();
    let mut matches = Vec::new();
    let mut entries: Vec<Option<_>> = loaded.into_iter().map(Some).collect();
    for (index, alignment) in alignment::rank(heard, &words) {
        if let Some((candidate, lyrics, own, duration, album)) = entries[index].take() {
            matches.push(SungMatch {
                candidate,
                lyrics,
                own,
                duration,
                album,
                alignment,
            });
        }
    }
    Ok(SungSearch {
        matches,
        candidates: suggested,
        lookup_problem,
    })
}
