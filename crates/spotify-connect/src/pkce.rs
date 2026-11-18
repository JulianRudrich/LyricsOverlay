//! Logging in to Spotify with the authorization code flow and PKCE (RFC 7636),
//! the flow Spotify asks of apps that cannot keep a client secret. Pure apart
//! from [`Pkce::generate`] and [`random_state`], which ask the operating
//! system for random bytes.

use std::fmt::{self, Write as _};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Spotify's accounts service: the login page and the token endpoint.
pub const ACCOUNTS_URL: &str = "https://accounts.spotify.com";

/// The port of the short-lived listener that receives the login.
pub const CALLBACK_PORT: u16 = 43821;
pub const CALLBACK_PATH: &str = "/callback";
/// The redirect URI listeners add to their Spotify app. Spotify accepts plain
/// HTTP only for a loopback IP literal, never for "localhost".
pub const REDIRECT_URI: &str = "http://127.0.0.1:43821/callback";

/// What the connection may do, and nothing more.
pub const SCOPES: [&str; 9] = [
    // ♥, and Liked Songs as a shuffle source.
    "user-library-read",
    "user-library-modify",
    // Playlists as shuffle sources, and the shuffle playlist itself.
    "playlist-read-private",
    "playlist-read-collaborative",
    "playlist-modify-private",
    // Starting a shuffle on this computer and following what plays.
    "user-read-playback-state",
    "user-read-currently-playing",
    "user-modify-playback-state",
    // Top artists and tracks.
    "user-top-read",
];

/// Whether a login granted every scope in [`SCOPES`]. Logins made before a
/// scope was added need to connect again.
pub fn grants_all_scopes(granted: &str) -> bool {
    let granted: Vec<&str> = granted.split_whitespace().collect();
    SCOPES.iter().all(|scope| granted.contains(scope))
}

/// Spotify Client IDs are 32 hexadecimal digits.
pub fn is_valid_client_id(client_id: &str) -> bool {
    client_id.len() == 32 && client_id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("the operating system provided no random numbers")]
pub struct RandomError;

/// The secret of one login attempt and its hash, which goes to the browser.
#[derive(Clone, PartialEq, Eq)]
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

impl Pkce {
    pub fn from_random_bytes(bytes: &[u8]) -> Self {
        let verifier = URL_SAFE_NO_PAD.encode(bytes);
        let challenge = challenge(&verifier);
        Self {
            verifier,
            challenge,
        }
    }

    /// A verifier from 64 random bytes: 86 characters, within the 43 to 128
    /// that RFC 7636 allows.
    pub fn generate() -> Result<Self, RandomError> {
        let mut bytes = [0u8; 64];
        getrandom::fill(&mut bytes).map_err(|_| RandomError)?;
        Ok(Self::from_random_bytes(&bytes))
    }
}

impl fmt::Debug for Pkce {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Pkce")
            .field("verifier", &"<hidden>")
            .field("challenge", &self.challenge)
            .finish()
    }
}

/// The S256 code challenge: the unpadded base64url SHA-256 of the verifier.
pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// The `state` that ties the browser's answer to one login attempt.
pub fn random_state() -> Result<String, RandomError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| RandomError)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

/// The login page to open in the browser. `accounts_url` is
/// [`ACCOUNTS_URL`] outside of tests.
pub fn authorize_url(
    accounts_url: &str,
    client_id: &str,
    redirect_uri: &str,
    scopes: &[&str],
    state: &str,
    challenge: &str,
) -> String {
    let scope = scopes.join(" ");
    format!(
        "{}/authorize?{}",
        accounts_url.trim_end_matches('/'),
        form_encode(&[
            ("client_id", client_id),
            ("response_type", "code"),
            ("redirect_uri", redirect_uri),
            ("code_challenge_method", "S256"),
            ("code_challenge", challenge),
            ("scope", &scope),
            ("state", state),
        ])
    )
}

/// What the app trades for tokens.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Grant<'a> {
    /// The code from the browser, with the verifier of the same attempt.
    AuthorizationCode {
        code: &'a str,
        verifier: &'a str,
    },
    RefreshToken(&'a str),
}

impl fmt::Debug for Grant<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Grant::AuthorizationCode { .. } => "AuthorizationCode(<hidden>)",
            Grant::RefreshToken(_) => "RefreshToken(<hidden>)",
        })
    }
}

/// The `application/x-www-form-urlencoded` body for the token endpoint. A
/// public client sends its Client ID and no secret.
pub fn token_request_body(client_id: &str, redirect_uri: &str, grant: &Grant<'_>) -> String {
    match *grant {
        Grant::AuthorizationCode { code, verifier } => form_encode(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", client_id),
            ("code_verifier", verifier),
        ]),
        Grant::RefreshToken(token) => form_encode(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", token),
            ("client_id", client_id),
        ]),
    }
}

/// Joins pairs into a query string or form body.
pub fn form_encode(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(name, value)| format!("{}={}", percent_encode(name), percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Encodes every byte except RFC 3986's unreserved characters, so `:`, `/`,
/// `+` and spaces never reach Spotify raw.
pub fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

/// Splits a query string or form body into decoded pairs.
pub fn parse_form(text: &str) -> Vec<(String, String)> {
    text.split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            (percent_decode(name), percent_decode(value))
        })
        .collect()
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        let escaped = (byte == b'%')
            .then(|| bytes.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| std::str::from_utf8(hex).ok())
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match (byte, escaped) {
            (_, Some(escaped)) => {
                decoded.push(escaped);
                index += 3;
            }
            (b'+', None) => {
                decoded.push(b' ');
                index += 1;
            }
            (byte, None) => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/// What the browser brought back to the redirect URI.
#[derive(Clone, PartialEq, Eq)]
pub enum CallbackOutcome {
    /// The login worked; the code is traded for tokens next.
    Code(String),
    /// The listener pressed Cancel on Spotify's page.
    Denied,
    /// Spotify reported another problem, such as `invalid_scope`.
    Failed(String),
    /// An answer for another login attempt, or for none. It is ignored, so no
    /// other page can end or take over this login.
    Foreign,
}

impl fmt::Debug for CallbackOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CallbackOutcome::Code(_) => formatter.write_str("Code(<hidden>)"),
            CallbackOutcome::Denied => formatter.write_str("Denied"),
            CallbackOutcome::Failed(error) => write!(formatter, "Failed({error:?})"),
            CallbackOutcome::Foreign => formatter.write_str("Foreign"),
        }
    }
}

/// Reads the query string of a request to the redirect URI.
pub fn callback_outcome(query: &str, expected_state: &str) -> CallbackOutcome {
    let pairs = parse_form(query);
    let value = |name: &str| {
        pairs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    };
    // Spotify sends the state with errors too; without it nothing counts.
    if !value("state").is_some_and(|state| same_secret(state, expected_state)) {
        return CallbackOutcome::Foreign;
    }
    if let Some(error) = value("error") {
        return if error == "access_denied" {
            CallbackOutcome::Denied
        } else {
            CallbackOutcome::Failed(error.to_owned())
        };
    }
    match value("code") {
        Some(code) if !code.is_empty() => CallbackOutcome::Code(code.to_owned()),
        _ => CallbackOutcome::Failed("missing_code".into()),
    }
}

/// Compares without stopping at the first difference.
fn same_secret(left: &str, right: &str) -> bool {
    left.len() == right.len()
        && left
            .bytes()
            .zip(right.bytes())
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example from RFC 7636, appendix B.
    const RFC_BYTES: [u8; 32] = [
        116, 24, 223, 180, 151, 153, 224, 37, 79, 250, 96, 125, 216, 173, 187, 186, 22, 212, 37,
        77, 105, 214, 191, 240, 91, 88, 5, 88, 83, 132, 141, 121,
    ];

    #[test]
    fn matches_the_rfc_example() {
        let pkce = Pkce::from_random_bytes(&RFC_BYTES);
        assert_eq!(pkce.verifier, "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
        assert_eq!(
            pkce.challenge,
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn generated_verifiers_are_long_enough_and_differ() {
        let first = Pkce::generate().expect("random bytes");
        let second = Pkce::generate().expect("random bytes");
        assert_eq!(first.verifier.len(), 86);
        assert!(first
            .verifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'));
        assert_ne!(first.verifier, second.verifier);
        assert_eq!(first.challenge, challenge(&first.verifier));
        assert!(!format!("{first:?}").contains(&first.verifier));

        let state = random_state().expect("random bytes");
        assert_eq!(state.len(), 22);
        assert_ne!(state, random_state().expect("random bytes"));
    }

    #[test]
    fn builds_the_login_page_address() {
        let url = authorize_url(
            "https://accounts.spotify.com/",
            "abc123",
            REDIRECT_URI,
            &["user-library-read", "user-top-read"],
            "s+t/a=te",
            "challenge",
        );
        assert_eq!(
            url,
            "https://accounts.spotify.com/authorize?client_id=abc123&response_type=code\
             &redirect_uri=http%3A%2F%2F127.0.0.1%3A43821%2Fcallback&code_challenge_method=S256\
             &code_challenge=challenge&scope=user-library-read%20user-top-read&state=s%2Bt%2Fa%3Dte"
        );
        let query = url.split_once('?').map(|(_, query)| query).unwrap_or("");
        let pairs = parse_form(query);
        assert!(pairs.contains(&("redirect_uri".into(), REDIRECT_URI.into())));
        assert!(pairs.contains(&("scope".into(), "user-library-read user-top-read".into())));
        assert!(pairs.contains(&("state".into(), "s+t/a=te".into())));
    }

    #[test]
    fn builds_token_request_bodies() {
        let code = token_request_body(
            "client",
            REDIRECT_URI,
            &Grant::AuthorizationCode {
                code: "the code",
                verifier: "verifier~1",
            },
        );
        assert_eq!(
            code,
            "grant_type=authorization_code&code=the%20code\
             &redirect_uri=http%3A%2F%2F127.0.0.1%3A43821%2Fcallback&client_id=client\
             &code_verifier=verifier~1"
        );
        let refresh = token_request_body("client", REDIRECT_URI, &Grant::RefreshToken("r/1+2"));
        assert_eq!(
            refresh,
            "grant_type=refresh_token&refresh_token=r%2F1%2B2&client_id=client"
        );
        let hidden = format!("{:?}", Grant::RefreshToken("r/1+2"));
        assert!(!hidden.contains("r/1+2"));
    }

    #[test]
    fn encodes_everything_but_unreserved_characters() {
        assert_eq!(percent_encode("aZ09-._~"), "aZ09-._~");
        assert_eq!(percent_encode("a b+c:d/é"), "a%20b%2Bc%3Ad%2F%C3%A9");
        assert_eq!(
            parse_form("a=1+2&b=%C3%A9%2b&c&=x&d=%zz&e=%4"),
            vec![
                ("a".into(), "1 2".into()),
                ("b".into(), "é+".into()),
                ("c".into(), String::new()),
                (String::new(), "x".into()),
                ("d".into(), "%zz".into()),
                ("e".into(), "%4".into()),
            ]
        );
    }

    #[test]
    fn reads_the_answer_of_the_login_page() {
        assert_eq!(
            callback_outcome("code=AQB%2Fx&state=abc", "abc"),
            CallbackOutcome::Code("AQB/x".into())
        );
        assert_eq!(
            callback_outcome("error=access_denied&state=abc", "abc"),
            CallbackOutcome::Denied
        );
        assert_eq!(
            callback_outcome("error=invalid_scope&state=abc", "abc"),
            CallbackOutcome::Failed("invalid_scope".into())
        );
        assert_eq!(
            callback_outcome("state=abc", "abc"),
            CallbackOutcome::Failed("missing_code".into())
        );
        for foreign in [
            "code=x&state=abd",
            "code=x&state=ab",
            "code=x",
            "error=access_denied",
            "",
        ] {
            assert_eq!(
                callback_outcome(foreign, "abc"),
                CallbackOutcome::Foreign,
                "{foreign}"
            );
        }
        assert!(!format!("{:?}", CallbackOutcome::Code("secret".into())).contains("secret"));
    }

    #[test]
    fn checks_scopes_and_client_ids() {
        assert!(grants_all_scopes(&SCOPES.join(" ")));
        assert!(grants_all_scopes(&format!(
            "streaming {}",
            SCOPES.join(" ")
        )));
        assert!(!grants_all_scopes("user-library-read user-top-read"));
        assert!(!grants_all_scopes(""));

        assert!(is_valid_client_id("0123456789abcdef0123456789ABCDEF"));
        assert!(!is_valid_client_id("0123456789abcdef0123456789abcde"));
        assert!(!is_valid_client_id("0123456789abcdef0123456789abcdeg"));
        assert!(!is_valid_client_id(" 0123456789abcdef0123456789abcde"));
    }

    #[test]
    fn redirect_uri_uses_the_callback_port() {
        assert_eq!(
            REDIRECT_URI,
            format!("http://127.0.0.1:{CALLBACK_PORT}{CALLBACK_PATH}")
        );
    }
}
