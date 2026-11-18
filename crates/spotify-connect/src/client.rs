//! The HTTP side of the connection: logging in through the browser, keeping
//! the tokens fresh, and the Web API calls the app makes. Tokens never appear
//! in errors; the access token lives only in memory.

use std::fmt::Write as _;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use futures_util::stream::{self, StreamExt};
use reqwest::header::{ACCEPT, CONTENT_LENGTH, CONTENT_TYPE, RETRY_AFTER};
use reqwest::Method;
use serde::Serialize;
use serde_json::json;
use thiserror::Error;
use tokio::sync::Mutex;

use crate::callback::{CallbackListener, CallbackPages, ListenError};
use crate::credentials::{CredentialError, CredentialStore};
use crate::model::{
    self, CurrentlyPlaying, Device, Playlist, TimeRange, TokenResponse, TopItem, TrackInfo, User,
};
use crate::pkce::{self, CallbackOutcome, Grant, Pkce};
use crate::shuffle::LIKED_SONGS;

/// The Spotify Web API.
pub const API_URL: &str = "https://api.spotify.com/v1";

/// Items per request when writing a playlist, the most Spotify takes.
const WRITE_BATCH: usize = 100;
const PAGE_SIZE: usize = 50;
/// Pages that load at the same time after the first.
const PARALLEL_PAGES: usize = 4;
/// Stops paging through a list that never ends.
const MAX_PAGES: usize = 1_000;
/// A token this close to expiring is refreshed first.
const EXPIRY_MARGIN: Duration = Duration::from_secs(60);
/// The longest "try again later" that is waited out once instead of failing.
const MAX_RETRY_WAIT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The Client ID of the listener's own Spotify app.
    pub client_id: String,
    pub user_agent: String,
    pub accounts_url: String,
    pub api_url: String,
    pub callback_port: u16,
}

impl Config {
    pub fn new(client_id: &str, user_agent: &str) -> Self {
        Self {
            client_id: client_id.trim().to_owned(),
            user_agent: user_agent.to_owned(),
            accounts_url: pkce::ACCOUNTS_URL.to_owned(),
            api_url: API_URL.to_owned(),
            callback_port: pkce::CALLBACK_PORT,
        }
    }

    pub fn redirect_uri(&self) -> String {
        format!(
            "http://127.0.0.1:{}{}",
            self.callback_port,
            pkce::CALLBACK_PATH
        )
    }
}

/// Where the login is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Persistence {
    /// In the system's credential store: connected across restarts.
    System,
    /// Only in memory, because there is no credential store: connected until quit.
    Memory,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SpotifyError {
    #[error("not connected to Spotify")]
    NotConnected,
    /// The login was revoked or the Client ID no longer exists.
    #[error("the Spotify login expired")]
    LoginExpired,
    /// The token endpoint refused the code, for example for a wrong Client ID.
    #[error("Spotify refused the login: {0}")]
    LoginRefused(String),
    #[error("no internet connection")]
    Offline,
    #[error("Spotify is busy; try again in {} s", .retry_after.as_secs())]
    RateLimited { retry_after: Duration },
    /// No Premium, or an account the Spotify app does not allow.
    #[error("Spotify refused the request{}", .0.as_deref().map(|m| format!(": {m}")).unwrap_or_default())]
    Forbidden(Option<String>),
    #[error("Spotify did not find it")]
    NotFound,
    #[error("Spotify answered HTTP {0}")]
    Http(u16),
    #[error("Spotify sent an answer that could not be read")]
    InvalidResponse,
    #[error("the request failed: {0}")]
    Network(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LoginError {
    #[error(transparent)]
    Listen(#[from] ListenError),
    #[error("the operating system provided no random numbers")]
    Random,
    /// Cancel was pressed on Spotify's page.
    #[error("the login was cancelled")]
    Denied,
    #[error("Spotify reported {0}")]
    Failed(String),
    #[error("the browser did not come back in time")]
    TimedOut,
    #[error(transparent)]
    Spotify(#[from] SpotifyError),
}

/// Top artists or top tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopKind {
    Artists,
    Tracks,
}

/// A login waiting for the browser.
pub struct PendingLogin {
    url: String,
    redirect_uri: String,
    pkce: Pkce,
    state: String,
    listener: CallbackListener,
}

impl PendingLogin {
    /// The login page to open in the browser.
    pub fn url(&self) -> &str {
        &self.url
    }
}

/// Tokens of the current login. No `Debug`, so they cannot be printed.
#[derive(Default)]
struct Tokens {
    access: Option<String>,
    expires_at: Option<Instant>,
    /// Read from the credential store once, or its only copy without one.
    refresh: Option<String>,
    scope: Option<String>,
}

impl Tokens {
    fn valid_access(&self) -> Option<String> {
        let expires_at = self.expires_at?;
        (expires_at > Instant::now() + EXPIRY_MARGIN)
            .then(|| self.access.clone())
            .flatten()
    }

    fn set_access(&mut self, token: &TokenResponse) {
        self.access = Some(token.access_token.clone());
        self.expires_at = Some(Instant::now() + Duration::from_secs(token.expires_in));
        if token.scope.is_some() {
            self.scope.clone_from(&token.scope);
        }
    }
}

enum TokenFailure {
    Refused {
        error: String,
        description: Option<String>,
    },
    Other(SpotifyError),
}

struct Answer {
    status: u16,
    body: Vec<u8>,
}

pub struct SpotifyClient {
    http: reqwest::Client,
    config: Config,
    credentials: Arc<dyn CredentialStore>,
    tokens: Mutex<Tokens>,
    persistence: StdMutex<Option<Persistence>>,
    store_problem: StdMutex<Option<String>>,
}

impl SpotifyClient {
    pub fn new(config: Config, credentials: Arc<dyn CredentialStore>) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(&config.user_agent)
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(8))
            .build()
            .unwrap_or_default();
        Self {
            http,
            config,
            credentials,
            tokens: Mutex::new(Tokens::default()),
            persistence: StdMutex::new(None),
            store_problem: StdMutex::new(None),
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Where the login is kept, once known.
    pub fn persistence(&self) -> Option<Persistence> {
        *self.persistence.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Why the credential store could not be used, for the log.
    pub fn store_problem(&self) -> Option<String> {
        self.store_problem
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    fn note_store(&self, result: Result<(), CredentialError>) -> Persistence {
        let persistence = match result {
            Ok(()) => Persistence::System,
            Err(CredentialError::Unavailable(reason)) => {
                *self.store_problem.lock().unwrap_or_else(|p| p.into_inner()) = Some(reason);
                Persistence::Memory
            }
        };
        *self.persistence.lock().unwrap_or_else(|p| p.into_inner()) = Some(persistence);
        persistence
    }

    /// Prepares a login: listens on the callback port and builds the page to
    /// open in the browser.
    pub async fn start_login(&self) -> Result<PendingLogin, LoginError> {
        let pkce = Pkce::generate().map_err(|_| LoginError::Random)?;
        let state = pkce::random_state().map_err(|_| LoginError::Random)?;
        let listener = CallbackListener::bind(self.config.callback_port).await?;
        let redirect_uri = listener.redirect_uri();
        let url = pkce::authorize_url(
            &self.config.accounts_url,
            &self.config.client_id,
            &redirect_uri,
            &pkce::SCOPES,
            &state,
            &pkce.challenge,
        );
        Ok(PendingLogin {
            url,
            redirect_uri,
            pkce,
            state,
            listener,
        })
    }

    /// Waits for the browser, trades the code for tokens and keeps the login.
    pub async fn finish_login(
        &self,
        login: PendingLogin,
        pages: &CallbackPages,
        timeout: Duration,
    ) -> Result<Persistence, LoginError> {
        let PendingLogin {
            redirect_uri,
            pkce,
            state,
            listener,
            ..
        } = login;
        let outcome = tokio::time::timeout(timeout, listener.wait(&state, pages))
            .await
            .map_err(|_| LoginError::TimedOut)?;
        let code = match outcome {
            CallbackOutcome::Code(code) => code,
            CallbackOutcome::Denied => return Err(LoginError::Denied),
            CallbackOutcome::Failed(error) => return Err(LoginError::Failed(error)),
            CallbackOutcome::Foreign => return Err(LoginError::Failed("unexpected_answer".into())),
        };
        Ok(self
            .exchange_code(&code, &pkce.verifier, &redirect_uri)
            .await?)
    }

    /// Trades a login code for tokens and stores the refresh token.
    pub async fn exchange_code(
        &self,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
    ) -> Result<Persistence, SpotifyError> {
        let token = self
            .request_token(&Grant::AuthorizationCode { code, verifier }, redirect_uri)
            .await
            .map_err(|failure| match failure {
                TokenFailure::Refused { error, description } => {
                    SpotifyError::LoginRefused(description.unwrap_or(error))
                }
                TokenFailure::Other(error) => error,
            })?;
        let refresh = token
            .refresh_token
            .clone()
            .ok_or(SpotifyError::InvalidResponse)?;
        let mut tokens = self.tokens.lock().await;
        *tokens = Tokens::default();
        tokens.set_access(&token);
        tokens.refresh = Some(refresh.clone());
        let store = Arc::clone(&self.credentials);
        Ok(self.note_store(blocking(move || store.save(&refresh)).await))
    }

    /// Whether there is a login, in memory or in the credential store.
    pub async fn has_login(&self) -> bool {
        let mut tokens = self.tokens.lock().await;
        self.refresh_token(&mut tokens).await.is_some()
    }

    /// Whether the login granted every scope the app uses; `None` before
    /// the first token.
    pub async fn granted_all_scopes(&self) -> Option<bool> {
        let tokens = self.tokens.lock().await;
        tokens.scope.as_deref().map(pkce::grants_all_scopes)
    }

    /// Forgets the login, in memory and in the credential store.
    pub async fn forget(&self) -> Result<(), CredentialError> {
        let mut tokens = self.tokens.lock().await;
        *tokens = Tokens::default();
        let store = Arc::clone(&self.credentials);
        blocking(move || store.delete()).await
    }

    async fn refresh_token(&self, tokens: &mut Tokens) -> Option<String> {
        if let Some(refresh) = &tokens.refresh {
            return Some(refresh.clone());
        }
        let store = Arc::clone(&self.credentials);
        match blocking(move || store.load()).await {
            Ok(Some(refresh)) => {
                self.note_store(Ok(()));
                tokens.refresh = Some(refresh.clone());
                Some(refresh)
            }
            Ok(None) => None,
            Err(error) => {
                self.note_store(Err(error));
                None
            }
        }
    }

    async fn access_token(&self) -> Result<String, SpotifyError> {
        let mut tokens = self.tokens.lock().await;
        if let Some(access) = tokens.valid_access() {
            return Ok(access);
        }
        self.refresh_locked(&mut tokens).await
    }

    /// After a 401: refreshes, unless another request already did.
    async fn renew_access_token(&self, rejected: &str) -> Result<String, SpotifyError> {
        let mut tokens = self.tokens.lock().await;
        if let Some(access) = tokens.valid_access().filter(|access| access != rejected) {
            return Ok(access);
        }
        tokens.access = None;
        self.refresh_locked(&mut tokens).await
    }

    /// Runs with the tokens locked, so concurrent requests share one refresh.
    async fn refresh_locked(&self, tokens: &mut Tokens) -> Result<String, SpotifyError> {
        let refresh = self
            .refresh_token(tokens)
            .await
            .ok_or(SpotifyError::NotConnected)?;
        match self
            .request_token(&Grant::RefreshToken(&refresh), &self.config.redirect_uri())
            .await
        {
            Ok(token) => {
                tokens.set_access(&token);
                if let Some(rotated) = token.refresh_token.filter(|rotated| *rotated != refresh) {
                    tokens.refresh = Some(rotated.clone());
                    let store = Arc::clone(&self.credentials);
                    self.note_store(blocking(move || store.save(&rotated)).await);
                }
                Ok(token.access_token)
            }
            // A revoked login, or a Client ID that no longer exists.
            Err(TokenFailure::Refused { error, .. })
                if error == "invalid_grant" || error == "invalid_client" =>
            {
                *tokens = Tokens::default();
                let store = Arc::clone(&self.credentials);
                let _ = blocking(move || store.delete()).await;
                Err(SpotifyError::LoginExpired)
            }
            Err(TokenFailure::Refused { error, description }) => {
                Err(SpotifyError::LoginRefused(description.unwrap_or(error)))
            }
            Err(TokenFailure::Other(error)) => Err(error),
        }
    }

    async fn request_token(
        &self,
        grant: &Grant<'_>,
        redirect_uri: &str,
    ) -> Result<TokenResponse, TokenFailure> {
        let body = pkce::token_request_body(&self.config.client_id, redirect_uri, grant);
        let response = self
            .http
            .post(format!(
                "{}/api/token",
                self.config.accounts_url.trim_end_matches('/')
            ))
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(ACCEPT, "application/json")
            .body(body)
            .send()
            .await
            .map_err(|error| TokenFailure::Other(network_error(error)))?;
        let status = response.status().as_u16();
        let wait = retry_after(&response);
        let bytes = response
            .bytes()
            .await
            .map_err(|error| TokenFailure::Other(network_error(error)))?;
        match status {
            200..=299 => serde_json::from_slice(&bytes)
                .map_err(|_| TokenFailure::Other(SpotifyError::InvalidResponse)),
            400 | 401 | 403 => match serde_json::from_slice::<model::TokenError>(&bytes) {
                Ok(refused) => Err(TokenFailure::Refused {
                    error: refused.error,
                    description: refused.error_description,
                }),
                Err(_) => Err(TokenFailure::Other(SpotifyError::Http(status))),
            },
            429 => Err(TokenFailure::Other(SpotifyError::RateLimited {
                retry_after: wait,
            })),
            other => Err(TokenFailure::Other(SpotifyError::Http(other))),
        }
    }

    /// Sends a Web API request. Refreshes and repeats once after a 401,
    /// waits once for a short "try again later", and repeats a request that
    /// can safely run twice once after a server error.
    async fn call(
        &self,
        method: Method,
        url: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<Answer, SpotifyError> {
        let mut token = self.access_token().await?;
        let (mut renewed, mut waited, mut repeated) = (false, false, false);
        loop {
            let mut request = self
                .http
                .request(method.clone(), url)
                .bearer_auth(&token)
                .header(ACCEPT, "application/json");
            request = match body {
                Some(json) => request.json(json),
                // Spotify wants a length even without a body (else HTTP 411).
                None if method != Method::GET => {
                    request.header(CONTENT_LENGTH, "0").body(Vec::new())
                }
                None => request,
            };
            let response = request.send().await.map_err(network_error)?;
            let status = response.status().as_u16();
            let wait = retry_after(&response);
            let bytes = response.bytes().await.map_err(network_error)?;
            match status {
                200..=299 => {
                    return Ok(Answer {
                        status,
                        body: bytes.to_vec(),
                    })
                }
                401 if !renewed => {
                    renewed = true;
                    token = self.renew_access_token(&token).await?;
                }
                401 => return Err(SpotifyError::LoginExpired),
                429 if !waited && wait <= MAX_RETRY_WAIT => {
                    waited = true;
                    tokio::time::sleep(wait).await;
                }
                429 => return Err(SpotifyError::RateLimited { retry_after: wait }),
                500..=599 if !repeated && method != Method::POST => {
                    repeated = true;
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                403 => return Err(SpotifyError::Forbidden(model::api_error_message(&bytes))),
                404 => return Err(SpotifyError::NotFound),
                other => return Err(SpotifyError::Http(other)),
            }
        }
    }

    fn api(&self, path: &str) -> String {
        format!("{}{path}", self.config.api_url.trim_end_matches('/'))
    }

    async fn get(&self, url: &str) -> Result<Vec<u8>, SpotifyError> {
        Ok(self.call(Method::GET, url, None).await?.body)
    }

    pub async fn me(&self) -> Result<User, SpotifyError> {
        let body = self.get(&self.api("/me")).await?;
        model::user(&body).map_err(|_| SpotifyError::InvalidResponse)
    }

    /// Whether the track is in Liked Songs.
    pub async fn is_saved(&self, uri: &str) -> Result<bool, SpotifyError> {
        let body = self
            .get(&self.api(&format!("/me/library/contains?uris={}", query_value(uri))))
            .await?;
        model::saved_contains(&body)
            .map(|answers| answers.first().copied().unwrap_or(false))
            .map_err(|_| SpotifyError::InvalidResponse)
    }

    /// Adds the track to Liked Songs or removes it.
    pub async fn set_saved(&self, uri: &str, saved: bool) -> Result<(), SpotifyError> {
        let method = if saved { Method::PUT } else { Method::DELETE };
        let url = self.api(&format!("/me/library?uris={}", query_value(uri)));
        self.call(method, &url, None).await.map(|_| ())
    }

    /// Spotify's own ranking of the listener's artists or tracks.
    pub async fn top(
        &self,
        kind: TopKind,
        range: TimeRange,
        limit: usize,
    ) -> Result<Vec<TopItem>, SpotifyError> {
        let kind = match kind {
            TopKind::Artists => "artists",
            TopKind::Tracks => "tracks",
        };
        let url = self.api(&format!(
            "/me/top/{kind}?time_range={}&limit={}",
            range.as_param(),
            limit.clamp(1, 50)
        ));
        model::top_items(&self.get(&url).await?).map_err(|_| SpotifyError::InvalidResponse)
    }

    /// Every playlist the listener follows, owns or collaborates on.
    pub async fn playlists(&self) -> Result<Vec<Playlist>, SpotifyError> {
        let mut playlists = Vec::new();
        for page in 0..MAX_PAGES {
            let url = self.api(&format!(
                "/me/playlists?limit={PAGE_SIZE}&offset={}",
                page * PAGE_SIZE
            ));
            let page = model::playlist_page(&self.get(&url).await?)
                .map_err(|_| SpotifyError::InvalidResponse)?;
            playlists.extend(page.items);
            if page.next.is_none() {
                break;
            }
        }
        Ok(playlists)
    }

    /// The tracks of Liked Songs ([`LIKED_SONGS`]) or of a playlist, in
    /// their order. After the first page, several pages load at once;
    /// `progress` hears the entries loaded and the total.
    pub async fn tracks(
        &self,
        source: &str,
        mut progress: impl FnMut(usize, usize) + Send,
    ) -> Result<Vec<TrackInfo>, SpotifyError> {
        let path = if source == LIKED_SONGS {
            "/me/tracks".to_owned()
        } else {
            format!("/playlists/{}/items", checked_id(source)?)
        };
        let first = self.track_page(&path, 0).await?;
        let mut tracks = first.items;
        let Some(total) = first.total.filter(|_| first.next.is_some()) else {
            return Ok(tracks);
        };
        progress(PAGE_SIZE.min(total), total);
        let offsets = (PAGE_SIZE..total).step_by(PAGE_SIZE).take(MAX_PAGES);
        let mut pages = stream::iter(offsets)
            .map(|offset| self.track_page(&path, offset))
            .buffered(PARALLEL_PAGES);
        let mut loaded = PAGE_SIZE;
        while let Some(page) = pages.next().await {
            tracks.extend(page?.items);
            loaded += PAGE_SIZE;
            progress(loaded.min(total), total);
        }
        Ok(tracks)
    }

    async fn track_page(
        &self,
        path: &str,
        offset: usize,
    ) -> Result<model::Page<TrackInfo>, SpotifyError> {
        let url = self.api(&format!("{path}?limit={PAGE_SIZE}&offset={offset}"));
        model::track_page(&self.get(&url).await?).map_err(|_| SpotifyError::InvalidResponse)
    }

    pub async fn devices(&self) -> Result<Vec<Device>, SpotifyError> {
        let body = self.get(&self.api("/me/player/devices")).await?;
        model::devices(&body).map_err(|_| SpotifyError::InvalidResponse)
    }

    /// Plays the playlist from its first track on the device, with Spotify's
    /// own shuffle and repeat off. Spotify's servers resolve the playlist, so
    /// one rewritten seconds ago plays as written.
    pub async fn play_playlist(
        &self,
        playlist_id: &str,
        device_id: &str,
    ) -> Result<(), SpotifyError> {
        let id = checked_id(playlist_id)?;
        let device = query_value(device_id);
        let body =
            json!({ "context_uri": format!("spotify:playlist:{id}"), "offset": { "position": 0 } });
        self.call(
            Method::PUT,
            &self.api(&format!("/me/player/play?device_id={device}")),
            Some(&body),
        )
        .await?;
        self.call(
            Method::PUT,
            &self.api(&format!(
                "/me/player/shuffle?state=false&device_id={device}"
            )),
            None,
        )
        .await?;
        self.call(
            Method::PUT,
            &self.api(&format!("/me/player/repeat?state=off&device_id={device}")),
            None,
        )
        .await?;
        Ok(())
    }

    /// What plays now; `None` when nothing does.
    pub async fn currently_playing(&self) -> Result<Option<CurrentlyPlaying>, SpotifyError> {
        let answer = self
            .call(Method::GET, &self.api("/me/player/currently-playing"), None)
            .await?;
        if answer.status == 204 || answer.body.iter().all(u8::is_ascii_whitespace) {
            return Ok(None);
        }
        model::currently_playing(&answer.body)
            .map(Some)
            .map_err(|_| SpotifyError::InvalidResponse)
    }

    /// Creates a private playlist and returns its ID.
    pub async fn create_playlist(
        &self,
        name: &str,
        description: &str,
    ) -> Result<String, SpotifyError> {
        let body = json!({ "name": name, "description": description, "public": false });
        let answer = self
            .call(Method::POST, &self.api("/me/playlists"), Some(&body))
            .await?;
        model::created_id(&answer.body).map_err(|_| SpotifyError::InvalidResponse)
    }

    /// Replaces every item of the playlist: the first request replaces, the
    /// next ones append. `progress` hears how many are written.
    pub async fn replace_items(
        &self,
        playlist_id: &str,
        uris: &[String],
        mut progress: impl FnMut(usize) + Send,
    ) -> Result<(), SpotifyError> {
        let url = self.api(&format!("/playlists/{}/items", checked_id(playlist_id)?));
        let mut batches = uris.chunks(WRITE_BATCH);
        let first = batches.next().unwrap_or_default();
        self.call(Method::PUT, &url, Some(&json!({ "uris": first })))
            .await?;
        let mut written = first.len();
        progress(written);
        for batch in batches {
            self.call(Method::POST, &url, Some(&json!({ "uris": batch })))
                .await?;
            written += batch.len();
            progress(written);
        }
        Ok(())
    }

    /// Adds items after the existing ones.
    pub async fn append_items(
        &self,
        playlist_id: &str,
        uris: &[String],
    ) -> Result<(), SpotifyError> {
        let url = self.api(&format!("/playlists/{}/items", checked_id(playlist_id)?));
        for batch in uris.chunks(WRITE_BATCH) {
            self.call(Method::POST, &url, Some(&json!({ "uris": batch })))
                .await?;
        }
        Ok(())
    }

    /// Removes every occurrence of these tracks from the playlist.
    pub async fn remove_items(
        &self,
        playlist_id: &str,
        uris: &[String],
    ) -> Result<(), SpotifyError> {
        let url = self.api(&format!("/playlists/{}/items", checked_id(playlist_id)?));
        for batch in uris.chunks(WRITE_BATCH) {
            let items: Vec<serde_json::Value> =
                batch.iter().map(|uri| json!({ "uri": uri })).collect();
            self.call(Method::DELETE, &url, Some(&json!({ "items": items })))
                .await?;
        }
        Ok(())
    }
}

/// Runs a credential store call on a thread that may block.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, CredentialError> + Send + 'static,
) -> Result<T, CredentialError> {
    tokio::task::spawn_blocking(work).await.unwrap_or_else(|_| {
        Err(CredentialError::Unavailable(
            "the credential store stopped answering".into(),
        ))
    })
}

/// Spotify IDs are base62; anything else never reaches a URL path.
fn checked_id(id: &str) -> Result<&str, SpotifyError> {
    if !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        Ok(id)
    } else {
        Err(SpotifyError::NotFound)
    }
}

/// Encodes a query value but keeps the colons of a Spotify URI readable.
fn query_value(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

/// Seconds from `Retry-After`, at least one.
fn retry_after(response: &reqwest::Response) -> Duration {
    let seconds = response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(1)
        .clamp(1, 3_600);
    Duration::from_secs(seconds)
}

/// Describes a failed request without its URL.
fn network_error(error: reqwest::Error) -> SpotifyError {
    let error = error.without_url();
    if error.is_connect() {
        SpotifyError::Offline
    } else if error.is_timeout() {
        SpotifyError::Network("timed out".into())
    } else if error.is_decode() || error.is_body() {
        SpotifyError::InvalidResponse
    } else {
        SpotifyError::Network(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_query_values_are_safe() {
        assert_eq!(
            checked_id("37i9dQZF1DXcBWIGoYBM5M"),
            Ok("37i9dQZF1DXcBWIGoYBM5M")
        );
        assert_eq!(checked_id("../me"), Err(SpotifyError::NotFound));
        assert_eq!(checked_id(""), Err(SpotifyError::NotFound));
        assert_eq!(query_value("spotify:track:4uLU6h"), "spotify:track:4uLU6h");
        assert_eq!(query_value("a b&c=d"), "a%20b%26c%3Dd");
    }

    #[test]
    fn errors_read_well() {
        assert_eq!(
            SpotifyError::Forbidden(Some("Premium required".into())).to_string(),
            "Spotify refused the request: Premium required"
        );
        assert_eq!(
            SpotifyError::Forbidden(None).to_string(),
            "Spotify refused the request"
        );
        assert_eq!(
            SpotifyError::RateLimited {
                retry_after: Duration::from_secs(4)
            }
            .to_string(),
            "Spotify is busy; try again in 4 s"
        );
        let config = Config::new(" abc ", "agent");
        assert_eq!(config.client_id, "abc");
        assert_eq!(config.redirect_uri(), pkce::REDIRECT_URI);
    }
}
