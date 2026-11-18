//! The short-lived listener on 127.0.0.1 that receives the browser after the
//! Spotify login and shows a page the listener can close. It lives only
//! while one login waits, and answers nothing but the redirect URI.

use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use crate::pkce::{callback_outcome, CallbackOutcome, CALLBACK_PATH};

/// How long one connection may take to send its request. Browsers open
/// spare connections that send nothing; they must not hold up the login.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_REQUEST_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ListenError {
    #[error("port {0} on this computer is used by another app")]
    PortInUse(u16),
    #[error("the login could not listen on this computer: {0}")]
    Io(String),
}

/// The pages the browser shows after the login, in the app's language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallbackPages {
    /// The `lang` of the pages, such as "en".
    pub language: String,
    pub connected: Page,
    pub denied: Page,
    pub failed: Page,
    /// For an answer that belongs to an earlier login attempt.
    pub outdated: Page,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub title: String,
    pub text: String,
}

impl Page {
    pub fn new(title: &str, text: &str) -> Self {
        Self {
            title: title.to_owned(),
            text: text.to_owned(),
        }
    }
}

impl Default for CallbackPages {
    fn default() -> Self {
        Self {
            language: "en".into(),
            connected: Page::new(
                "Connected to Spotify",
                "You can close this tab and go back to Singline.",
            ),
            denied: Page::new(
                "Not connected",
                "The login was cancelled. You can close this tab.",
            ),
            failed: Page::new(
                "Not connected",
                "Spotify reported a problem. Close this tab and try again in Singline.",
            ),
            outdated: Page::new(
                "This login has expired",
                "Go back to Singline and press Connect again.",
            ),
        }
    }
}

pub struct CallbackListener {
    listener: TcpListener,
    port: u16,
}

impl CallbackListener {
    /// Listens on 127.0.0.1 only. Port 0 picks a free port, for tests.
    pub async fn bind(port: u16) -> Result<Self, ListenError> {
        match TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await {
            Ok(listener) => {
                let port = listener
                    .local_addr()
                    .map(|address| address.port())
                    .unwrap_or(port);
                Ok(Self { listener, port })
            }
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                Err(ListenError::PortInUse(port))
            }
            Err(error) => Err(ListenError::Io(error.to_string())),
        }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}{CALLBACK_PATH}", self.port)
    }

    /// Waits until the browser brings an answer for the login with `state`:
    /// a code, a cancel or an error. Other requests get a page and are
    /// otherwise ignored. Dropping the future stops listening.
    pub async fn wait(self, state: &str, pages: &CallbackPages) -> CallbackOutcome {
        let state: Arc<str> = Arc::from(state);
        let pages = Arc::new(pages.clone());
        let (sender, mut answers) = mpsc::channel::<CallbackOutcome>(4);
        loop {
            tokio::select! {
                Some(outcome) = answers.recv() => return outcome,
                accepted = self.listener.accept() => {
                    let Ok((stream, _)) = accepted else {
                        // Out of file handles or similar: try again shortly.
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        continue;
                    };
                    let (sender, state, pages) = (sender.clone(), Arc::clone(&state), Arc::clone(&pages));
                    tokio::spawn(async move {
                        let answered = tokio::time::timeout(REQUEST_TIMEOUT, answer(stream, &state, &pages)).await;
                        if let Ok(Some(outcome)) = answered {
                            let _ = sender.send(outcome).await;
                        }
                    });
                }
            }
        }
    }
}

/// Answers one connection. Returns the outcome when it ends the login.
async fn answer(
    mut stream: TcpStream,
    state: &str,
    pages: &CallbackPages,
) -> Option<CallbackOutcome> {
    let head = read_head(&mut stream).await?;
    let mut parts = head.lines().next().unwrap_or_default().split_whitespace();
    let (method, target) = (
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
    );
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if method != "GET" || path != CALLBACK_PATH {
        let _ = respond(
            &mut stream,
            "404 Not Found",
            "text/plain; charset=utf-8",
            "Not found",
        )
        .await;
        return None;
    }
    let outcome = callback_outcome(query, state);
    let (status, page) = match &outcome {
        CallbackOutcome::Code(_) => ("200 OK", &pages.connected),
        CallbackOutcome::Denied => ("200 OK", &pages.denied),
        CallbackOutcome::Failed(_) => ("200 OK", &pages.failed),
        CallbackOutcome::Foreign => ("400 Bad Request", &pages.outdated),
    };
    let body = page_html(&pages.language, page);
    let _ = respond(&mut stream, status, "text/html; charset=utf-8", &body).await;
    (outcome != CallbackOutcome::Foreign).then_some(outcome)
}

async fn read_head(stream: &mut TcpStream) -> Option<String> {
    let mut head = Vec::with_capacity(1024);
    let mut buffer = [0u8; 2048];
    loop {
        let read = stream.read(&mut buffer).await.ok()?;
        if read == 0 {
            return None;
        }
        head.extend_from_slice(&buffer[..read]);
        if head.windows(4).any(|window| window == b"\r\n\r\n") || head.len() >= MAX_REQUEST_BYTES {
            return Some(String::from_utf8_lossy(&head).into_owned());
        }
    }
}

async fn respond(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
) -> std::io::Result<()> {
    let response = format!(
        "HTTP/1.1 {status}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Cache-Control: no-store\r\n\
         Referrer-Policy: no-referrer\r\n\
         X-Content-Type-Options: nosniff\r\n\
         Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await
}

/// A plain page in the app's grey tones, light or dark like the system.
fn page_html(language: &str, page: &Page) -> String {
    format!(
        "<!doctype html>\n<html lang=\"{}\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{}</title>\n<style>\n\
         :root {{ color-scheme: light dark; font-family: -apple-system, BlinkMacSystemFont, \"Segoe UI\", system-ui, Ubuntu, sans-serif; }}\n\
         body {{ margin: 0; min-height: 100vh; display: grid; place-items: center; background: #f4f5f6; color: #1d1f22; }}\n\
         main {{ max-width: 26rem; padding: 2rem; text-align: center; }}\n\
         h1 {{ margin: 0 0 0.6rem; font-size: 1.35rem; }}\n\
         p {{ margin: 0; line-height: 1.45; color: #6a6f76; }}\n\
         @media (prefers-color-scheme: dark) {{ body {{ background: #1e2226; color: #eceef0; }} p {{ color: #a2a8b0; }} }}\n\
         </style>\n</head>\n<body>\n<main>\n<h1>{}</h1>\n<p>{}</p>\n</main>\n</body>\n</html>\n",
        escape(language),
        escape(&page.title),
        escape(&page.title),
        escape(&page.text)
    )
}

fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_escape_their_texts() {
        let html = page_html(
            "de",
            &Page::new(
                "Verbunden <mit> Spotify",
                "Schließe \"diesen\" Tab & kehre zurück.",
            ),
        );
        assert!(html.contains("<html lang=\"de\">"));
        assert!(html.contains("<h1>Verbunden &lt;mit&gt; Spotify</h1>"));
        assert!(html.contains("Schließe &quot;diesen&quot; Tab &amp; kehre zurück."));
        assert!(!html.contains("<mit>"));
    }

    #[test]
    fn the_default_pages_name_the_app() {
        let pages = CallbackPages::default();
        assert!(pages.connected.text.contains("Singline"));
        assert_eq!(pages.language, "en");
    }
}
