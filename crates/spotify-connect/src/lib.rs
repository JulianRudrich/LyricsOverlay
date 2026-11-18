//! The optional Spotify connection: logging in with the listener's own
//! Spotify app (PKCE), keeping the login in the system's credential store,
//! the Web API calls for ♥, top lists and playlists, and True Shuffle.
//!
//! Spotify lets an app in development mode serve at most five allowlisted
//! users with Premium, so every listener creates a free Spotify app and uses
//! its Client ID; there is no shared client and no client secret.

pub mod callback;
pub mod client;
pub mod credentials;
pub mod model;
pub mod pkce;
pub mod shuffle;

pub use callback::{CallbackListener, CallbackPages, ListenError};
pub use client::{
    Config, LoginError, PendingLogin, Persistence, SpotifyClient, SpotifyError, TopKind,
};
pub use credentials::{CredentialError, CredentialStore, MemoryCredentials, SystemCredentials};
pub use model::{
    CurrentlyPlaying, Device, Page, Playlist, TimeRange, TokenResponse, TopItem, TrackInfo, User,
};
pub use pkce::{CallbackOutcome, Pkce, REDIRECT_URI, SCOPES};
pub use shuffle::{Cycle, NextRound, Plan, ShuffleState, TrackNote, UpNext, LIKED_SONGS};
