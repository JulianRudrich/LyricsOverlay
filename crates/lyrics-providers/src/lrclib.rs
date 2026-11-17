//! A small client for LRCLIB (<https://lrclib.net>), the free and open lyrics
//! database, and the search order that finds the right version of a song.

use std::time::Duration;

use lyrics_core::matching::{self, LyricsRecord};
use lyrics_core::{Lyrics, SyncLevel};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::Instant;
use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

pub const DEFAULT_BASE_URL: &str = "https://lrclib.net";

/// An exact match may differ from the track by this much.
const EXACT_DURATION_TOLERANCE: f64 = 2.5;
/// A word-synced result wins over a closer line-synced one within this margin.
const WORD_SYNC_BONUS: f64 = 1.0;

/// The track to find lyrics for.
#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    /// Seconds.
    pub duration: f64,
}

impl Query {
    /// The cache key: equal for queries that LRCLIB answers the same.
    pub fn cache_key(&self) -> String {
        format!(
            "lrclib:{}\u{1f}{}\u{1f}{}\u{1f}{}",
            matching::normalized(&self.title),
            matching::normalized(&self.artist),
            matching::normalized(self.album.as_deref().unwrap_or("")),
            self.duration.round() as i64
        )
    }
}

/// What a lookup found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LookupResult {
    Found { lyrics: Lyrics },
    Instrumental,
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LookupError {
    #[error("no internet connection")]
    Offline,
    #[error("LRCLIB is busy; try again in {} s", .retry_after.as_secs())]
    RateLimited { retry_after: Duration },
    #[error("LRCLIB is not available right now (HTTP {0})")]
    Http(u16),
    #[error("LRCLIB sent an answer that could not be read")]
    InvalidResponse,
    #[error("the request failed: {0}")]
    Network(String),
}

pub struct LrclibClient {
    http: reqwest::Client,
    base_url: String,
    retry_after: Mutex<Option<Instant>>,
    pause_between_requests: Duration,
}

impl LrclibClient {
    /// `user_agent` identifies the app to LRCLIB, as its operators ask.
    pub fn new(user_agent: &str) -> Self {
        Self::with_base_url(DEFAULT_BASE_URL, user_agent)
    }

    pub fn with_base_url(base_url: &str, user_agent: &str) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(user_agent)
            .timeout(Duration::from_secs(12))
            .connect_timeout(Duration::from_secs(6))
            .build()
            .unwrap_or_default();
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
            retry_after: Mutex::new(None),
            pause_between_requests: Duration::from_millis(200),
        }
    }

    /// Shortens the courtesy pause between two requests, for tests.
    pub fn with_pause(mut self, pause: Duration) -> Self {
        self.pause_between_requests = pause;
        self
    }

    /// Finds the best lyrics for `query`: the exact record first, then a search,
    /// preferring the same album, word timing, then line timing, then plain text.
    pub async fn find(&self, query: &Query) -> Result<LookupResult, LookupError> {
        let title = query.title.trim();
        let artist = query.artist.trim();
        let duration = query.duration;
        if title.is_empty() || artist.is_empty() || !duration.is_finite() || duration <= 0.0 {
            return Ok(LookupResult::NotFound);
        }
        let album = query
            .album
            .as_deref()
            .map(str::trim)
            .filter(|album| !album.is_empty());

        let mut plain_fallback: Option<Lyrics> = None;
        if (1.0..=3_600.0).contains(&duration.round()) {
            if let Some(record) = self.get(title, artist, album, duration).await? {
                if matching::matches(&record, title, artist, duration)
                    && (record.duration - duration).abs() <= EXACT_DURATION_TOLERANCE
                {
                    match record.to_lyrics(duration) {
                        Some(lyrics) if lyrics.instrumental => {
                            if folded_equal(&record.artist_name, artist)
                                && same_album(&record, album)
                            {
                                return Ok(LookupResult::Instrumental);
                            }
                        }
                        Some(lyrics) if lyrics.sync != SyncLevel::Unsynced => {
                            return Ok(LookupResult::Found { lyrics });
                        }
                        Some(lyrics) => plain_fallback = Some(lyrics),
                        None => {}
                    }
                }
            }
            tokio::time::sleep(self.pause_between_requests).await;
        }

        let records = self.search(title, artist).await?;
        let same: Vec<&LyricsRecord> = records
            .iter()
            .filter(|record| same_album(record, album))
            .collect();
        let best = best_synced(same.iter().copied(), title, artist, duration)
            .or_else(|| best_synced(records.iter(), title, artist, duration));
        if let Some(lyrics) = best.and_then(|record| record.to_lyrics(duration)) {
            return Ok(LookupResult::Found { lyrics });
        }
        if let Some(lyrics) = plain_fallback {
            return Ok(LookupResult::Found { lyrics });
        }
        let plain = records
            .iter()
            .filter(|record| matching::matches(record, title, artist, duration))
            .filter(|record| {
                record
                    .plain_lyrics
                    .as_deref()
                    .is_some_and(|text| !text.trim().is_empty())
            })
            .min_by(|a, b| {
                (a.duration - duration)
                    .abs()
                    .total_cmp(&(b.duration - duration).abs())
            });
        Ok(plain
            .and_then(|record| record.to_lyrics(duration))
            .map_or(LookupResult::NotFound, |lyrics| LookupResult::Found {
                lyrics,
            }))
    }

    /// `/api/get`: the one record for this exact title, artist, album and duration.
    pub async fn get(
        &self,
        title: &str,
        artist: &str,
        album: Option<&str>,
        duration: f64,
    ) -> Result<Option<LyricsRecord>, LookupError> {
        let mut params = vec![
            ("track_name", title.to_owned()),
            ("artist_name", artist.to_owned()),
            ("duration", (duration.round() as i64).to_string()),
        ];
        if let Some(album) = album {
            params.push(("album_name", album.to_owned()));
        }
        match self.fetch("/api/get", &params, true).await? {
            Some(body) => serde_json::from_slice(&body)
                .map(Some)
                .map_err(|_| LookupError::InvalidResponse),
            None => Ok(None),
        }
    }

    /// `/api/search`: every record with this title and artist.
    pub async fn search(
        &self,
        title: &str,
        artist: &str,
    ) -> Result<Vec<LyricsRecord>, LookupError> {
        let params = [
            ("track_name", title.to_owned()),
            ("artist_name", artist.to_owned()),
        ];
        match self.fetch("/api/search", &params, false).await? {
            Some(body) => serde_json::from_slice(&body).map_err(|_| LookupError::InvalidResponse),
            None => Ok(Vec::new()),
        }
    }

    async fn fetch(
        &self,
        path: &str,
        params: &[(&str, String)],
        allow_missing: bool,
    ) -> Result<Option<Vec<u8>>, LookupError> {
        if let Some(until) = *self.retry_after.lock().await {
            let now = Instant::now();
            if now < until {
                return Err(LookupError::RateLimited {
                    retry_after: until - now,
                });
            }
        }
        // Form encoding turns a literal "+" into %2B, which LRCLIB needs.
        let response = self
            .http
            .get(format!("{}{path}", self.base_url))
            .query(params)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(network_error)?;
        let status = response.status().as_u16();
        match status {
            200 => response
                .bytes()
                .await
                .map(|body| Some(body.to_vec()))
                .map_err(network_error),
            404 if allow_missing => Ok(None),
            429 | 503 => {
                let fallback = if status == 429 { 30 } else { 1 };
                let seconds = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.trim().parse::<u64>().ok())
                    .unwrap_or(fallback)
                    .min(3_600);
                let wait = Duration::from_secs_f64(seconds as f64 * jitter());
                let until = Instant::now() + wait;
                let mut retry_after = self.retry_after.lock().await;
                *retry_after = Some(retry_after.map_or(until, |current| current.max(until)));
                Err(LookupError::RateLimited { retry_after: wait })
            }
            other => Err(LookupError::Http(other)),
        }
    }
}

fn network_error(error: reqwest::Error) -> LookupError {
    if error.is_connect() {
        LookupError::Offline
    } else if error.is_timeout() {
        LookupError::Network("timed out".into())
    } else if error.is_decode() || error.is_body() {
        LookupError::InvalidResponse
    } else {
        LookupError::Network(error.to_string())
    }
}

/// Spreads retries of many listeners over a little more than the asked wait.
fn jitter() -> f64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.subsec_nanos());
    1.0 + f64::from(nanos % 250) / 1_000.0
}

/// The matching record with timing, preferring word timing when it is nearly as
/// close to the track's duration.
fn best_synced<'a>(
    records: impl Iterator<Item = &'a LyricsRecord>,
    title: &str,
    artist: &str,
    duration: f64,
) -> Option<&'a LyricsRecord> {
    records
        .filter(|record| {
            matching::matches(record, title, artist, duration) && record.has_synced_text()
        })
        .map(|record| {
            let word = record
                .lyricsfile
                .as_ref()
                .is_some_and(|file| file.has_word_sync())
                || record
                    .to_lyrics(duration)
                    .is_some_and(|lyrics| lyrics.sync == SyncLevel::Word);
            let score =
                (record.duration - duration).abs() - if word { WORD_SYNC_BONUS } else { 0.0 };
            (score, record)
        })
        .reduce(|best, candidate| {
            if candidate.0 < best.0 {
                candidate
            } else {
                best
            }
        })
        .map(|(_, record)| record)
}

fn same_album(record: &LyricsRecord, album: Option<&str>) -> bool {
    match album {
        None => true,
        Some(album) => record
            .album_name
            .as_deref()
            .is_some_and(|name| folded_equal(name, album)),
    }
}

/// Equal apart from case, accents and surrounding whitespace.
fn folded_equal(left: &str, right: &str) -> bool {
    fold(left) == fold(right)
}

fn fold(value: &str) -> String {
    value
        .trim()
        .nfd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_keys_ignore_formatting() {
        let query = |title: &str, duration: f64| Query {
            title: title.into(),
            artist: "Beyoncé".into(),
            album: None,
            duration,
        };
        assert_eq!(
            query("Halo", 261.2).cache_key(),
            query("HALO!", 260.8).cache_key()
        );
        assert_ne!(
            query("Halo", 261.0).cache_key(),
            query("Halo", 262.0).cache_key()
        );
    }

    #[test]
    fn folding_keeps_punctuation() {
        assert!(folded_equal(" Beyoncé ", "BEYONCE"));
        assert!(!folded_equal("AC/DC", "ACDC"));
    }
}
