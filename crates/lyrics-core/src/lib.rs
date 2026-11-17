//! Pure lyric logic for Lyrics Overlay: parsing, matching and timing.
//!
//! Nothing in this crate touches the network, the file system or a player. Every
//! function is deterministic, so the whole crate is covered by plain unit tests.

pub mod clock;
pub mod lrc;
pub mod lyricsfile;
pub mod matching;
pub mod model;
pub mod plain;
pub mod selection;
mod text;

pub use clock::PlaybackClock;
pub use matching::LyricsRecord;
pub use model::{LyricLine, LyricWord, Lyrics, SyncLevel};
pub use selection::{PlaybackActivity, PlaybackSelection};
