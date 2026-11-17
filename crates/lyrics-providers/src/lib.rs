//! Finding lyrics for a track: your own lyrics first, then the local cache,
//! then LRCLIB.

pub mod lrclib;
pub mod store;

use std::sync::{Arc, Mutex, MutexGuard};

use lyrics_core::{lrc, Lyrics, SyncLevel};
use serde::Serialize;

pub use lrclib::{LookupError, LookupResult, LrclibClient, Query};
pub use store::{ImportError, Store, StoreError, MAX_LYRICS_BYTES};

const DAY: i64 = 24 * 60 * 60;

/// Where the lyrics for a track came from, or why there are none.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LyricsOutcome {
    /// Your own lyrics, which always win.
    Local {
        lyrics: Lyrics,
    },
    Online {
        lyrics: Lyrics,
    },
    Instrumental,
    NotFound,
    /// Online search is turned off and the track has no own lyrics.
    LocalOnly,
    Offline,
    RateLimited {
        retry_after_secs: u64,
    },
    Failed {
        message: String,
    },
}

impl LyricsOutcome {
    /// True when trying again later may give a different answer.
    pub fn can_retry(&self) -> bool {
        matches!(
            self,
            LyricsOutcome::NotFound
                | LyricsOutcome::Offline
                | LyricsOutcome::RateLimited { .. }
                | LyricsOutcome::Failed { .. }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LookupOptions {
    /// Search LRCLIB when the track has no own lyrics.
    pub online: bool,
    /// Skip the cache, for "search again".
    pub refresh: bool,
}

impl Default for LookupOptions {
    fn default() -> Self {
        Self {
            online: true,
            refresh: false,
        }
    }
}

pub struct LyricsService {
    client: LrclibClient,
    store: Arc<Mutex<Store>>,
}

impl LyricsService {
    pub fn new(client: LrclibClient, store: Arc<Mutex<Store>>) -> Self {
        Self { client, store }
    }

    fn store(&self) -> MutexGuard<'_, Store> {
        self.store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Your own lyrics for the track, when it has some with timed text.
    pub fn local(&self, track_key: &str) -> Option<Lyrics> {
        let text = self.store().local_lyrics(track_key)?;
        let lines = lrc::parse(&text);
        lines
            .iter()
            .any(|line| !line.is_break())
            .then(|| Lyrics::timed(lines))
    }

    pub async fn lyrics_for(
        &self,
        track_key: &str,
        query: &Query,
        options: LookupOptions,
    ) -> LyricsOutcome {
        if let Some(lyrics) = self.local(track_key) {
            return LyricsOutcome::Local { lyrics };
        }
        if !options.online {
            return LyricsOutcome::LocalOnly;
        }

        let key = query.cache_key();
        let now = store::unix_now();
        if !options.refresh {
            let cached = self.store().cached::<LookupResult>(&key, now);
            if let Some(result) = cached {
                return outcome(result);
            }
        }
        match self.client.find(query).await {
            Ok(result) => {
                let expires_at = now + lifetime(&result);
                let _ = self.store().store_cached(&key, &result, now, expires_at);
                outcome(result)
            }
            Err(LookupError::Offline) => LyricsOutcome::Offline,
            Err(LookupError::RateLimited { retry_after }) => LyricsOutcome::RateLimited {
                retry_after_secs: retry_after.as_secs().max(1),
            },
            Err(error) => LyricsOutcome::Failed {
                message: error.to_string(),
            },
        }
    }
}

fn outcome(result: LookupResult) -> LyricsOutcome {
    match result {
        LookupResult::Found { lyrics } => LyricsOutcome::Online { lyrics },
        LookupResult::Instrumental => LyricsOutcome::Instrumental,
        LookupResult::NotFound => LyricsOutcome::NotFound,
    }
}

/// How long an online answer is reused. Answers that may improve, such as
/// "not found" or plain text, are asked again sooner.
fn lifetime(result: &LookupResult) -> i64 {
    match result {
        LookupResult::Found { lyrics } if lyrics.sync == SyncLevel::Unsynced => 3 * DAY,
        LookupResult::Found { .. } | LookupResult::Instrumental => 30 * DAY,
        LookupResult::NotFound => DAY,
    }
}
