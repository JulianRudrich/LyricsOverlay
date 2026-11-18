//! Genius (<https://genius.com>) as a [`SongSearch`] provider: its search
//! finds songs by words of their lyrics. It needs a Client Access Token, which
//! anyone gets for free by creating an API client at
//! <https://genius.com/api-clients>.

use std::time::Duration;

use lyrics_core::matching;
use serde::Deserialize;

use crate::search::{SearchError, SearchFuture, SongCandidate, SongSearch};

pub const DEFAULT_BASE_URL: &str = "https://api.genius.com";

/// Words beyond this many only slow the search down.
const MAX_QUERY_WORDS: usize = 24;

pub struct GeniusClient {
    http: reqwest::Client,
    base_url: String,
    token: String,
}

impl GeniusClient {
    pub fn new(token: &str, user_agent: &str) -> Self {
        Self::with_base_url(DEFAULT_BASE_URL, token, user_agent)
    }

    pub fn with_base_url(base_url: &str, token: &str, user_agent: &str) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(user_agent)
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(6))
            .build()
            .unwrap_or_default();
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
            token: token.trim().to_owned(),
        }
    }

    /// Songs whose lyrics contain `words`, in the order Genius ranks them.
    /// Translations and other pages that are not songs are left out.
    pub async fn search_songs(&self, words: &str) -> Result<Vec<SongCandidate>, SearchError> {
        if self.token.is_empty() {
            return Err(SearchError::NoToken);
        }
        let query = words
            .split_whitespace()
            .take(MAX_QUERY_WORDS)
            .collect::<Vec<_>>()
            .join(" ");
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let response = self
            .http
            .get(format!("{}/search", self.base_url))
            .query(&[("q", query.as_str())])
            .bearer_auth(&self.token)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(network_error)?;
        let status = response.status().as_u16();
        match status {
            200 => {}
            401 | 403 => return Err(SearchError::Unauthorized),
            429 => {
                let seconds = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.trim().parse::<u64>().ok())
                    .unwrap_or(30)
                    .min(3_600);
                return Err(SearchError::RateLimited {
                    retry_after: Duration::from_secs(seconds),
                });
            }
            other => return Err(SearchError::Http(other)),
        }
        let body: Envelope = response
            .json()
            .await
            .map_err(|_| SearchError::InvalidResponse)?;
        Ok(candidates(body))
    }
}

impl SongSearch for GeniusClient {
    fn name(&self) -> &'static str {
        "genius"
    }

    fn search<'a>(&'a self, words: &'a str) -> SearchFuture<'a> {
        Box::pin(self.search_songs(words))
    }
}

#[derive(Deserialize)]
struct Envelope {
    response: Hits,
}

#[derive(Deserialize)]
struct Hits {
    #[serde(default)]
    hits: Vec<Hit>,
}

#[derive(Deserialize)]
struct Hit {
    #[serde(rename = "type", default)]
    kind: String,
    result: Option<Song>,
}

#[derive(Deserialize)]
struct Song {
    #[serde(default)]
    title: String,
    primary_artist: Option<Artist>,
    #[serde(default)]
    artist_names: Option<String>,
    #[serde(default)]
    song_art_image_thumbnail_url: Option<String>,
}

#[derive(Deserialize)]
struct Artist {
    #[serde(default)]
    name: String,
}

fn candidates(body: Envelope) -> Vec<SongCandidate> {
    let mut found: Vec<SongCandidate> = Vec::new();
    for hit in body.response.hits {
        let Some(song) = hit.result.filter(|_| hit.kind == "song") else {
            continue;
        };
        let title = clean(&song.title);
        let artist = clean(
            &song
                .primary_artist
                .map(|artist| artist.name)
                .or(song.artist_names)
                .unwrap_or_default(),
        );
        // "Genius English Translations" and the like are not the song itself.
        if title.is_empty() || artist.is_empty() || artist.starts_with("Genius") {
            continue;
        }
        let same = |other: &SongCandidate| {
            matching::normalized(&other.title) == matching::normalized(&title)
                && matching::normalized(&other.artist) == matching::normalized(&artist)
        };
        if found.iter().any(same) {
            continue;
        }
        found.push(SongCandidate {
            title,
            artist,
            artwork_url: song
                .song_art_image_thumbnail_url
                .filter(|url| url.starts_with("https://")),
        });
    }
    found
}

fn clean(value: &str) -> String {
    value
        .replace(['\u{a0}', '\u{200b}'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn network_error(error: reqwest::Error) -> SearchError {
    if error.is_connect() {
        SearchError::Offline
    } else if error.is_timeout() {
        SearchError::Network("timed out".into())
    } else if error.is_decode() || error.is_body() {
        SearchError::InvalidResponse
    } else {
        SearchError::Network(error.to_string())
    }
}
