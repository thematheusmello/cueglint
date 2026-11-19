//! Explicit, bounded requests. Constructing a client or storing a credential sends no request.

pub mod calendar;
mod credentials;
pub use credentials::{CredentialStore, NativeCredentialStore, Secret, load_for_config};
mod client;
mod protocol;
mod sse;
pub use client::ProviderClient;
use cueglint_core::{Citation, HostedInferenceMetadata, TokenUsage};
use serde::Serialize;
use std::fmt;

pub type ProviderResult<T> = Result<T, ProviderError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    InvalidConfig,
    MissingCredential,
    CredentialUnavailable,
    Authentication,
    RateLimited,
    ModelUnavailable,
    InvalidRequest,
    Server,
    Transport,
    Timeout,
    Cancelled,
    Protocol,
    LimitExceeded,
    Unsupported,
    RedirectBlocked,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderError {
    pub kind: ErrorKind,
    pub status: Option<u16>,
    pub retry_after_seconds: Option<u64>,
}

impl ProviderError {
    pub(crate) fn new(kind: ErrorKind) -> Self {
        Self {
            kind,
            status: None,
            retry_after_seconds: None,
        }
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self.kind {
            ErrorKind::InvalidConfig => "Provider configuration is incomplete or invalid.",
            ErrorKind::MissingCredential => {
                "Save an API key for this provider and endpoint before connecting."
            }
            ErrorKind::CredentialUnavailable => {
                "The native credential store is unavailable; no plaintext fallback is used."
            }
            ErrorKind::Authentication => "The provider rejected this API key or account access.",
            ErrorKind::RateLimited => {
                "The provider rate limit or quota was reached. Retry deliberately later."
            }
            ErrorKind::ModelUnavailable => {
                "The selected model is unavailable to this account or endpoint."
            }
            ErrorKind::InvalidRequest => {
                "The provider rejected this request or model configuration."
            }
            ErrorKind::Server => "The provider failed while processing the request.",
            ErrorKind::Transport => "The provider connection failed or its stream was interrupted.",
            ErrorKind::Timeout => "The provider request exceeded the time limit.",
            ErrorKind::Cancelled => "Provider request cancelled.",
            ErrorKind::Protocol => "The provider returned an invalid or incomplete response.",
            ErrorKind::LimitExceeded => "The provider response exceeded a safety limit.",
            ErrorKind::Unsupported => "This provider operation or response type is unsupported.",
            ErrorKind::RedirectBlocked => {
                "The provider requested a redirect; no credentials were forwarded."
            }
        };
        f.write_str(text)
    }
}
impl std::error::Error for ProviderError {}

#[derive(Clone, Debug, Serialize)]
pub struct ProviderOutput {
    pub text: String,
    pub metadata: HostedInferenceMetadata,
}

#[derive(Clone, Debug)]
pub enum StreamEvent {
    TextDelta(String),
    Usage(TokenUsage),
    Citation(Citation),
}

#[derive(Clone, Debug, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ModelCatalog {
    pub models: Vec<ModelInfo>,
    pub manual_entry: bool,
    pub documentation_url: Option<String>,
    pub truncated: bool,
}
