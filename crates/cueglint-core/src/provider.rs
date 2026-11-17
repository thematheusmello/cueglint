//! Provider choices and reported hosted provenance. This module never reads keys or opens sockets.

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use url::{Host, Url};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    #[default]
    Local,
    OpenAi,
    Anthropic,
    Gemini,
    Perplexity,
    OpenRouter,
    OpenAiCompatible,
}

impl ProviderId {
    pub const ALL: [Self; 7] = [
        Self::Local,
        Self::OpenAi,
        Self::Anthropic,
        Self::Gemini,
        Self::Perplexity,
        Self::OpenRouter,
        Self::OpenAiCompatible,
    ];

    pub fn is_local(self) -> bool {
        self == Self::Local
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::OpenAi => "open_ai",
            Self::Anthropic => "anthropic",
            Self::Gemini => "gemini",
            Self::Perplexity => "perplexity",
            Self::OpenRouter => "open_router",
            Self::OpenAiCompatible => "open_ai_compatible",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Local => "Local Candle",
            Self::OpenAi => "OpenAI",
            Self::Anthropic => "Anthropic",
            Self::Gemini => "Gemini",
            Self::Perplexity => "Perplexity",
            Self::OpenRouter => "OpenRouter",
            Self::OpenAiCompatible => "OpenAI-compatible server",
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProviderConfig {
    pub provider: ProviderId,
    pub model_id: String,
    pub base_url: Option<String>,
}

impl ProviderConfig {
    /// Incomplete model selection is allowed in Settings so model listing can be deliberate.
    pub fn validate(&self) -> Result<()> {
        if !self.model_id.is_empty() {
            validate_model_id(&self.model_id)?;
        }
        if self.provider.is_local() {
            ensure!(
                self.model_id.is_empty() && self.base_url.is_none(),
                "local generation uses the selected local profile"
            );
        } else {
            self.endpoint()?;
        }
        Ok(())
    }

    pub fn validate_for_generation(&self) -> Result<()> {
        self.validate()?;
        ensure!(
            !self.provider.is_local(),
            "local generation requires the local engine"
        );
        validate_model_id(&self.model_id)
    }

    /// Canonical API base, including its path. Credentials are bound to this entire value.
    pub fn endpoint(&self) -> Result<String> {
        let builtin = match self.provider {
            ProviderId::Local => anyhow::bail!("local generation has no endpoint"),
            ProviderId::OpenAi => "https://api.openai.com/v1/",
            ProviderId::Anthropic => "https://api.anthropic.com/v1/",
            ProviderId::Gemini => "https://generativelanguage.googleapis.com/v1beta/",
            ProviderId::Perplexity => "https://api.perplexity.ai/v1/",
            ProviderId::OpenRouter => "https://openrouter.ai/api/v1/",
            ProviderId::OpenAiCompatible => {
                return canonical_endpoint(
                    self.base_url
                        .as_deref()
                        .context("configure the compatible server API base URL")?,
                );
            }
        };
        ensure!(
            self.base_url.is_none(),
            "built-in providers use their fixed official HTTPS endpoint"
        );
        Ok(builtin.to_owned())
    }

    pub fn credential_ref(&self) -> Result<CredentialRef> {
        Ok(CredentialRef {
            provider: self.provider,
            endpoint: self.endpoint()?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialRef {
    provider: ProviderId,
    endpoint: String,
}

impl CredentialRef {
    pub fn provider(&self) -> ProviderId {
        self.provider
    }
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CredentialStatus {
    Missing,
    Stored,
    Unavailable { reason: String },
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Citation {
    pub url: String,
    pub title: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct HostedInferenceMetadata {
    pub provider: ProviderId,
    pub requested_model: String,
    pub returned_model: Option<String>,
    /// A vendor-reported version only; aliases and unreported weight revisions remain unknown.
    pub model_version: Option<String>,
    pub endpoint: String,
    pub request_id: Option<String>,
    pub usage: TokenUsage,
    pub max_output_tokens: u32,
    pub elapsed_ms: u64,
    pub finish_reason: String,
    pub citations: Vec<Citation>,
    pub prompt_sha256: String,
    pub template_revision: String,
}

impl HostedInferenceMetadata {
    pub fn validate_for(&self, config: &ProviderConfig) -> Result<()> {
        config.validate_for_generation()?;
        ensure!(
            self.provider == config.provider
                && self.requested_model == config.model_id
                && self.endpoint == config.endpoint()?,
            "hosted provenance does not match the frozen provider, model and endpoint"
        );
        for model in [&self.returned_model, &self.model_version]
            .into_iter()
            .flatten()
        {
            validate_model_id(model)?;
        }
        if let Some(id) = &self.request_id {
            bounded_text(id, 256)?;
        }
        ensure!(
            (1..=1024).contains(&self.max_output_tokens),
            "invalid hosted output token budget"
        );
        ensure!(
            self.usage
                .input_tokens
                .into_iter()
                .chain(self.usage.output_tokens)
                .all(|count| count <= 100_000_000),
            "invalid hosted token usage"
        );
        ensure!(self.elapsed_ms <= 300_000, "invalid hosted elapsed time");
        ensure!(
            !self.finish_reason.is_empty()
                && self.finish_reason.len() <= 64
                && self
                    .finish_reason
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_'),
            "invalid hosted finish reason"
        );
        ensure!(
            [
                "stop",
                "length",
                "content_filter",
                "end_turn",
                "max_tokens",
                "stop_sequence",
                "refusal",
                "STOP",
                "MAX_TOKENS",
                "completed"
            ]
            .contains(&self.finish_reason.as_str()),
            "hosted provenance requires a supported terminal finish reason"
        );
        ensure!(self.citations.len() <= 64, "too many hosted citations");
        for citation in &self.citations {
            citation.validate()?;
        }
        ensure!(
            self.prompt_sha256.len() == 64
                && self.prompt_sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid hosted prompt hash"
        );
        bounded_text(&self.template_revision, 100)?;
        Ok(())
    }
}

impl Citation {
    pub fn validate(&self) -> Result<()> {
        bounded_text(&self.url, 4096)?;
        let url = Url::parse(&self.url).context("invalid citation URL")?;
        ensure!(
            matches!(url.scheme(), "https" | "http")
                && url.host().is_some()
                && url.username().is_empty()
                && url.password().is_none(),
            "citation requires an HTTP URL without credentials"
        );
        if let Some(title) = &self.title {
            bounded_text(title, 512)?;
        }
        Ok(())
    }
}

pub fn validate_model_id(model: &str) -> Result<()> {
    ensure!(
        !model.is_empty()
            && model.len() <= 256
            && model
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.:/@".contains(&b)),
        "model ID must contain 1–256 ordinary identifier characters"
    );
    Ok(())
}

fn bounded_text(value: &str, limit: usize) -> Result<()> {
    ensure!(
        !value.trim().is_empty() && value.len() <= limit && !value.chars().any(char::is_control),
        "invalid hosted metadata text"
    );
    Ok(())
}

fn canonical_endpoint(value: &str) -> Result<String> {
    ensure!(
        !value.is_empty() && value.len() <= 2048 && !value.chars().any(char::is_control),
        "invalid compatible server URL"
    );
    let mut url = Url::parse(value).context("invalid compatible server URL")?;
    ensure!(
        url.host().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "API base URL requires a host and no credentials, query or fragment"
    );
    let loopback = match url.host() {
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        Some(Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        None => false,
    };
    ensure!(
        url.scheme() == "https" || (url.scheme() == "http" && loopback),
        "API base URL requires HTTPS except on the loopback interface"
    );
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url.into())
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProviderMessage {
    pub role: MessageRole,
    pub content: String,
}
