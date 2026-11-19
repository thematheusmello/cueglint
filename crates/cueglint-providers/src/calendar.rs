//! Explicit read-only access to a Google account's primary calendar.

mod callback;
mod events;
mod jobs;
#[cfg(test)]
mod tests;

pub use events::{CalendarDateTime, CalendarEvent, EventTime, UpcomingEvents, UtcInstant};
pub use jobs::{
    CalendarCommand, CalendarConnection, CalendarDisconnectReport, CalendarOperation,
    CalendarRevocation, CalendarSnapshot, CalendarWorker,
};

use crate::{
    ErrorKind, Secret,
    client::{bounded_body, bounded_http_builder},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::{Client, Response, StatusCode};
use serde::{Deserialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    net::Ipv4Addr,
    time::{Duration, Instant},
};
use tokio::net::TcpListener;
use url::Url;
use zeroize::Zeroizing;

const SCOPE: &str = "https://www.googleapis.com/auth/calendar.events.owned.readonly";
const AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(180);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const TOKEN_RESPONSE_BYTES: usize = 32 * 1024;

pub type CalendarResult<T> = Result<T, CalendarError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalendarError {
    Busy,
    Cancelled,
    NotConfigured,
    NotConnected,
    AlreadyConnected,
    BrowserUnavailable,
    WorkerUnavailable,
    InvalidClient,
    LoopbackUnavailable,
    AuthorizationTimeout,
    AuthorizationDenied,
    AuthorizationFailed,
    ScopeMismatch,
    OfflineAccessMissing,
    RefreshRequired,
    ReconnectRequired,
    VaultUnavailable,
    InvalidWindow,
    InvalidResponse,
    ResponseTooLarge,
    RedirectBlocked,
    HttpStatus(u16),
    Timeout,
    Transport,
}

impl fmt::Display for CalendarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Busy => "Wait for the current Calendar operation to finish.",
            Self::Cancelled => "Calendar operation cancelled.",
            Self::NotConfigured => "Enter a Google Desktop client ID to connect.",
            Self::NotConnected => "Connect Google Calendar before refreshing events.",
            Self::AlreadyConnected => {
                "Disconnect the current Calendar connection before changing it."
            }
            Self::BrowserUnavailable => "Cueglint could not open the default browser.",
            Self::WorkerUnavailable => "The Calendar worker is unavailable.",
            Self::InvalidClient => "The Google Calendar client or credential is invalid.",
            Self::LoopbackUnavailable => {
                "The local Calendar authorization listener is unavailable."
            }
            Self::AuthorizationTimeout => "Calendar authorization timed out.",
            Self::AuthorizationDenied => "Google Calendar access was declined.",
            Self::AuthorizationFailed => "Google could not complete Calendar authorization.",
            Self::ScopeMismatch => "Google did not grant the requested read-only Calendar scope.",
            Self::OfflineAccessMissing => "Google did not grant offline Calendar access.",
            Self::RefreshRequired => "Refresh the Calendar connection before loading events.",
            Self::ReconnectRequired => "Connect to Google Calendar again.",
            Self::VaultUnavailable => "The native Calendar credential store is unavailable.",
            Self::InvalidWindow => "The requested Calendar time window is invalid.",
            Self::InvalidResponse => "Google returned an invalid or incomplete Calendar response.",
            Self::ResponseTooLarge => "The Google Calendar response exceeded its limit.",
            Self::RedirectBlocked => "Google Calendar returned an unexpected redirect.",
            Self::HttpStatus(status) => {
                return write!(f, "Google Calendar returned HTTP {status}.");
            }
            Self::Timeout => "The Google Calendar request timed out.",
            Self::Transport => "The Google Calendar request failed.",
        };
        f.write_str(message)
    }
}

impl std::error::Error for CalendarError {}

/// The caller opens this URL in the system browser after an explicit Connect action.
/// Dropping the pending connection closes its listener.
pub struct PendingConnect {
    authorization_url: Url,
    client_key: String,
    listener: TcpListener,
    host: String,
    redirect_uri: String,
    state: Secret,
    verifier: Secret,
    deadline: tokio::time::Instant,
}

impl PendingConnect {
    pub fn authorization_url(&self) -> &Url {
        &self.authorization_url
    }
}

pub struct CalendarSession {
    access: Secret,
    expires_at: Instant,
    refresh: CalendarRefreshToken,
}

impl CalendarSession {
    pub fn expires_at(&self) -> Instant {
        self.expires_at
    }

    pub fn refresh_token(&self) -> &CalendarRefreshToken {
        &self.refresh
    }
}

/// An opaque offline credential scoped to one Google desktop client.
pub struct CalendarRefreshToken {
    client_key: String,
    secret: Secret,
}

/// Construct and use on a single worker with a Tokio runtime.
pub struct GoogleCalendar {
    http: Client,
    endpoints: Endpoints,
    client_id: String,
    client_secret: Option<Secret>,
    client_key: String,
}

struct Endpoints {
    authorization: Url,
    token: Url,
    revoke: Url,
    events: Url,
}

impl GoogleCalendar {
    pub fn new(client_id: String, client_secret: Option<Secret>) -> CalendarResult<Self> {
        if client_id.is_empty()
            || client_id.len() > 256
            || !client_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        {
            return Err(CalendarError::InvalidClient);
        }
        let client_key = format!("{:x}", Sha256::digest(client_id.as_bytes()));
        let endpoints = Endpoints {
            authorization: google_url("https://accounts.google.com/o/oauth2/v2/auth"),
            token: google_url("https://oauth2.googleapis.com/token"),
            revoke: google_url("https://oauth2.googleapis.com/revoke"),
            events: google_url("https://www.googleapis.com/calendar/v3/calendars/primary/events"),
        };
        Ok(Self {
            http: calendar_http()
                .https_only(true)
                .build()
                .map_err(http_error)?,
            endpoints,
            client_id,
            client_secret,
            client_key,
        })
    }

    pub async fn begin_connect(&self) -> CalendarResult<PendingConnect> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|_| CalendarError::LoopbackUnavailable)?;
        let host = listener
            .local_addr()
            .map_err(|_| CalendarError::LoopbackUnavailable)?
            .to_string();
        let redirect_uri = format!("http://{host}/");
        let state = random_secret()?;
        let verifier = random_secret()?;
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.expose().as_bytes()));
        let mut authorization_url = self.endpoints.authorization.clone();
        authorization_url.query_pairs_mut().extend_pairs([
            ("client_id", self.client_id.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("response_type", "code"),
            ("scope", SCOPE),
            ("state", state.expose()),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("access_type", "offline"),
            ("prompt", "consent"),
        ]);
        Ok(PendingConnect {
            authorization_url,
            client_key: self.client_key.clone(),
            listener,
            host,
            redirect_uri,
            state,
            verifier,
            deadline: tokio::time::Instant::now() + AUTHORIZATION_TIMEOUT,
        })
    }

    pub async fn finish_connect(&self, pending: PendingConnect) -> CalendarResult<CalendarSession> {
        self.check_client(&pending.client_key)?;
        let code = callback::receive(
            pending.listener,
            &pending.host,
            &pending.state,
            pending.deadline,
        )
        .await?;
        self.exchange(
            &[
                ("grant_type", "authorization_code"),
                ("code", code.expose()),
                ("code_verifier", pending.verifier.expose()),
                ("redirect_uri", &pending.redirect_uri),
            ],
            None,
        )
        .await
    }

    /// A successful refresh can return a replacement offline credential.
    pub async fn refresh(&self, refresh: &CalendarRefreshToken) -> CalendarResult<CalendarSession> {
        self.check_client(&refresh.client_key)?;
        self.exchange(
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh.secret.expose()),
            ],
            Some(refresh),
        )
        .await
    }

    /// Revoke at Google. Local credential removal is a separate explicit operation.
    pub async fn revoke(&self, refresh: &CalendarRefreshToken) -> CalendarResult<()> {
        self.check_client(&refresh.client_key)?;
        let response = self
            .http
            .post(self.endpoints.revoke.clone())
            .form(&[("token", refresh.secret.expose())])
            .send()
            .await
            .map_err(http_error)?;
        require_ok(response)?;
        Ok(())
    }

    async fn exchange(
        &self,
        grant: &[(&str, &str)],
        previous: Option<&CalendarRefreshToken>,
    ) -> CalendarResult<CalendarSession> {
        let mut form = grant.to_vec();
        form.push(("client_id", self.client_id.as_str()));
        if let Some(secret) = &self.client_secret {
            form.push(("client_secret", secret.expose()));
        }
        let requested_at = Instant::now();
        let response = self
            .http
            .post(self.endpoints.token.clone())
            .form(&form)
            .send()
            .await
            .map_err(http_error)?;
        if response.status() == StatusCode::BAD_REQUEST && previous.is_some() {
            if response_json::<OAuthError>(response, TOKEN_RESPONSE_BYTES)
                .await
                .is_ok_and(|reply| reply.error == "invalid_grant")
            {
                return Err(CalendarError::ReconnectRequired);
            }
            return Err(CalendarError::HttpStatus(400));
        }
        let reply: TokenReply = response_json(require_ok(response)?, TOKEN_RESPONSE_BYTES).await?;
        let expires_at = requested_at
            .checked_add(Duration::from_secs(reply.expires_in))
            .filter(|_| reply.expires_in > 0)
            .ok_or(CalendarError::InvalidResponse)?;
        if !reply.token_type.eq_ignore_ascii_case("bearer") {
            return Err(CalendarError::InvalidResponse);
        }
        match reply.scope.as_deref() {
            Some(scope) if scope.trim() == SCOPE => {}
            None if previous.is_some() => {}
            _ => return Err(CalendarError::ScopeMismatch),
        }
        let refresh = reply
            .refresh_token
            .or_else(|| previous.map(|token| token.secret.expose().to_owned()))
            .ok_or(CalendarError::OfflineAccessMissing)?;
        Ok(CalendarSession {
            access: token(reply.access_token)?,
            expires_at,
            refresh: CalendarRefreshToken {
                client_key: self.client_key.clone(),
                secret: token(refresh)?,
            },
        })
    }

    fn check_client(&self, key: &str) -> CalendarResult<()> {
        if key != self.client_key {
            return Err(CalendarError::InvalidClient);
        }
        Ok(())
    }

    pub fn native_vault_supported() -> bool {
        cfg!(any(target_os = "macos", target_os = "windows"))
    }

    /// Load only for an explicit Calendar action.
    pub fn load_saved_refresh_token(&self) -> CalendarResult<Option<CalendarRefreshToken>> {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            match self.vault_entry()?.get_password() {
                Ok(secret) => Ok(Some(CalendarRefreshToken {
                    client_key: self.client_key.clone(),
                    secret: Secret::new(secret).map_err(|_| CalendarError::VaultUnavailable)?,
                })),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(_) => Err(CalendarError::VaultUnavailable),
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        Err(CalendarError::VaultUnavailable)
    }

    /// Save after successful exchange or refresh, before adopting the new session.
    pub fn save_refresh_token(&self, refresh: &CalendarRefreshToken) -> CalendarResult<()> {
        self.check_client(&refresh.client_key)?;
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            self.vault_entry()?
                .set_password(refresh.secret.expose())
                .map_err(|_| CalendarError::VaultUnavailable)
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        Err(CalendarError::VaultUnavailable)
    }

    pub fn forget_saved_refresh_token(&self) -> CalendarResult<()> {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            match self.vault_entry()?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(_) => Err(CalendarError::VaultUnavailable),
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        Err(CalendarError::VaultUnavailable)
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn vault_entry(&self) -> CalendarResult<keyring::Entry> {
        keyring::Entry::new("org.cueglint.google-calendar-refresh.v1", &self.client_key)
            .map_err(|_| CalendarError::VaultUnavailable)
    }
}

#[derive(Deserialize)]
struct TokenReply {
    access_token: String,
    expires_in: u64,
    token_type: String,
    scope: Option<String>,
    refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct OAuthError {
    error: String,
}

fn token(value: String) -> CalendarResult<Secret> {
    Secret::new(value).map_err(|_| CalendarError::InvalidResponse)
}

fn random_secret() -> CalendarResult<Secret> {
    let mut bytes = Zeroizing::new([0_u8; 32]);
    getrandom::fill(&mut bytes[..]).map_err(|_| CalendarError::AuthorizationFailed)?;
    Secret::new(URL_SAFE_NO_PAD.encode(&bytes[..])).map_err(|_| CalendarError::AuthorizationFailed)
}

fn google_url(url: &'static str) -> Url {
    Url::parse(url).expect("fixed Google URL")
}

fn calendar_http() -> reqwest::ClientBuilder {
    bounded_http_builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(REQUEST_TIMEOUT)
}

fn require_ok(response: Response) -> CalendarResult<Response> {
    let status = response.status();
    if status.is_redirection() {
        return Err(CalendarError::RedirectBlocked);
    }
    if status != StatusCode::OK {
        return Err(CalendarError::HttpStatus(status.as_u16()));
    }
    Ok(response)
}

async fn response_json<T: DeserializeOwned>(response: Response, max: usize) -> CalendarResult<T> {
    let bytes = bounded_body(response, max)
        .await
        .map_err(|error| match error.kind {
            ErrorKind::Timeout => CalendarError::Timeout,
            ErrorKind::LimitExceeded => CalendarError::ResponseTooLarge,
            _ => CalendarError::Transport,
        })?;
    serde_json::from_slice(&bytes).map_err(|_| CalendarError::InvalidResponse)
}

fn http_error(error: reqwest::Error) -> CalendarError {
    if error.is_timeout() {
        CalendarError::Timeout
    } else {
        CalendarError::Transport
    }
}
