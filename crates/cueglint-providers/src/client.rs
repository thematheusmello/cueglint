use crate::{
    CredentialStore, ErrorKind, ModelCatalog, ProviderError, ProviderOutput, ProviderResult,
    Secret, StreamEvent, load_for_config,
    protocol::{self, ResponseState},
    sse::{Decoder, MAX_STREAM_BYTES},
};
use cueglint_core::{
    ContextSnapshot, GenerationRequest, HostedInferenceMetadata, ProviderConfig, ProviderId,
    RequestStatus,
};
use futures_util::future::{Either, select};
use reqwest::{
    Client, Method, Response,
    header::{AUTHORIZATION, HeaderValue},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    future::Future,
    net::ToSocketAddrs,
    pin::pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::runtime::Runtime;
use url::Url;
use zeroize::Zeroizing;

const CANCEL_POLL: Duration = Duration::from_millis(50);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Own one client on an existing bounded worker. No request or credential lookup occurs in `new`.
pub struct ProviderClient {
    config: ProviderConfig,
    endpoint: Url,
    http: Client,
    runtime: Option<Runtime>,
    timeout: Duration,
}

impl ProviderClient {
    pub fn new(config: ProviderConfig) -> ProviderResult<Self> {
        config.validate().map_err(|_| invalid())?;
        let endpoint =
            Url::parse(&config.endpoint().map_err(|_| invalid())?).map_err(|_| invalid())?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .max_blocking_threads(1)
            .thread_name("cueglint-provider-dns")
            .build()
            .map_err(|_| transport())?;
        let http = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .referer(false)
            .http1_only()
            .connection_verbose(false)
            .connect_timeout(Duration::from_secs(20))
            .timeout(REQUEST_TIMEOUT)
            .pool_max_idle_per_host(0)
            .dns_resolver(Arc::new(BoundedDns))
            .build()
            .map_err(|_| transport())?;
        Ok(Self {
            config,
            endpoint,
            http,
            runtime: Some(runtime),
            timeout: REQUEST_TIMEOUT,
        })
    }

    /// The actor can use this entry point to check the frozen config before even loading its key.
    pub fn generate_with_store(
        &self,
        request: &GenerationRequest,
        max_output_tokens: u32,
        store: &dyn CredentialStore,
        cancel: &AtomicBool,
        on_event: impl FnMut(StreamEvent),
    ) -> ProviderResult<ProviderOutput> {
        self.validate_request(request, max_output_tokens)?;
        check_cancel(cancel)?;
        let key = load_for_config(store, &self.config)?;
        self.generate(request, max_output_tokens, key.as_ref(), cancel, on_event)
    }

    pub fn generate(
        &self,
        request: &GenerationRequest,
        max_output_tokens: u32,
        key: Option<&Secret>,
        cancel: &AtomicBool,
        on_event: impl FnMut(StreamEvent),
    ) -> ProviderResult<ProviderOutput> {
        self.validate_request(request, max_output_tokens)?;
        check_cancel(cancel)?;
        let messages = protocol::messages(request)?;
        let prompt = serde_json::to_vec(&messages).map_err(|_| invalid())?;
        let prompt_sha256 = format!("{:x}", Sha256::digest(&prompt));
        let (path, body) = protocol::request_body(
            self.config.provider,
            &self.config.model_id,
            &messages,
            max_output_tokens,
        )?;
        let body = serde_json::to_vec(&body).map_err(|_| invalid())?;
        if body.len() > protocol::MAX_REQUEST_BYTES {
            return Err(ProviderError::new(ErrorKind::LimitExceeded));
        }
        let builder = self
            .http
            .post(self.url(&path)?)
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .body(body);
        let builder = self.authenticate(builder, key)?;
        let started = Instant::now();
        let state = self.run(self.stream(builder, key, cancel, on_event), cancel)?;
        let metadata = HostedInferenceMetadata {
            provider: self.config.provider,
            requested_model: self.config.model_id.clone(),
            returned_model: state.model,
            model_version: state.version,
            endpoint: self.config.endpoint().map_err(|_| invalid())?,
            request_id: state.request_id,
            usage: state.usage,
            max_output_tokens,
            elapsed_ms: started.elapsed().as_millis() as u64,
            finish_reason: state.finish.ok_or_else(protocol_error)?,
            citations: state.citations,
            prompt_sha256,
            template_revision: protocol::TEMPLATE_REVISION.to_owned(),
        };
        metadata
            .validate_for(&request.provider)
            .map_err(|_| protocol_error())?;
        check_cancel(cancel)?;
        Ok(ProviderOutput {
            text: state.text,
            metadata,
        })
    }

    pub fn list_models(
        &self,
        key: Option<&Secret>,
        cancel: &AtomicBool,
    ) -> ProviderResult<ModelCatalog> {
        check_cancel(cancel)?;
        if self.config.provider == ProviderId::Perplexity {
            return Ok(ModelCatalog {
                models: Vec::new(),
                manual_entry: true,
                documentation_url: Some(
                    "https://docs.perplexity.ai/docs/agent-api/models".to_owned(),
                ),
                truncated: false,
            });
        }
        self.run(
            async {
                let mut models = Vec::new();
                let mut cursor: Option<String> = None;
                let mut seen_cursors = Vec::new();
                let mut truncated = false;
                for page in 0..4 {
                    let mut url = self.url("models")?;
                    if self.config.provider == ProviderId::Anthropic {
                        url.query_pairs_mut().append_pair("limit", "1000");
                        if let Some(cursor) = &cursor {
                            url.query_pairs_mut().append_pair("after_id", cursor);
                        }
                    } else if self.config.provider == ProviderId::Gemini {
                        url.query_pairs_mut().append_pair("pageSize", "100");
                        if let Some(cursor) = &cursor {
                            url.query_pairs_mut().append_pair("pageToken", cursor);
                        }
                    }
                    let builder = self.authenticate(
                        self.http
                            .request(Method::GET, url)
                            .header("Accept", "application/json"),
                        key,
                    )?;
                    let response = self
                        .response(builder.send().await.map_err(http_error)?)
                        .await?;
                    let value = self.json(response, MAX_STREAM_BYTES).await?;
                    protocol::reject_secret(&value, key)?;
                    if value.get("error").is_some_and(|error| !error.is_null()) {
                        return Err(protocol::remote_error(&value, None));
                    }
                    let (next, full) =
                        protocol::model_page(self.config.provider, &value, &mut models)?;
                    truncated |= full;
                    if next.is_none() || full {
                        break;
                    }
                    if page == 3 {
                        truncated = true;
                        break;
                    }
                    let next = next.unwrap();
                    if seen_cursors.contains(&next) {
                        return Err(protocol_error());
                    }
                    seen_cursors.push(next.clone());
                    cursor = Some(next);
                }
                Ok(ModelCatalog {
                    models,
                    manual_entry: true,
                    documentation_url: None,
                    truncated,
                })
            },
            cancel,
        )
    }

    /// A deliberate, billable synthetic request. No current session, transcript or screen is read.
    pub fn test_connection(
        &self,
        key: Option<&Secret>,
        cancel: &AtomicBool,
    ) -> ProviderResult<HostedInferenceMetadata> {
        let request = GenerationRequest {
            id: "connection-test".to_owned(),
            session_id: "connection-test".to_owned(),
            prompt: "Reply with the single word OK.".to_owned(),
            context: ContextSnapshot {
                revision: 0,
                mode: None,
                transcript: Vec::new(),
                knowledge: Vec::new(),
                screen: None,
                chat: Vec::new(),
                selection_version: 1,
                omitted_transcript_segments: 0,
                omitted_chat_turns: 0,
                diagnostics: Vec::new(),
            },
            status: RequestStatus::Pending,
            created_at: 0,
            provider: self.config.clone(),
        };
        self.generate(&request, 16, key, cancel, |_| {})
            .map(|output| output.metadata)
    }

    fn validate_request(&self, request: &GenerationRequest, max: u32) -> ProviderResult<()> {
        if self.config != request.provider
            || request.status != RequestStatus::Pending
            || !(1..=1024).contains(&max)
        {
            return Err(invalid());
        }
        self.config.validate_for_generation().map_err(|_| invalid())
    }

    fn url(&self, path: &str) -> ProviderResult<Url> {
        let url = self.endpoint.join(path).map_err(|_| invalid())?;
        if url.origin() != self.endpoint.origin() || !url.path().starts_with(self.endpoint.path()) {
            return Err(invalid());
        }
        Ok(url)
    }

    fn authenticate(
        &self,
        mut builder: reqwest::RequestBuilder,
        key: Option<&Secret>,
    ) -> ProviderResult<reqwest::RequestBuilder> {
        if let Some(key) = key {
            let value = Zeroizing::new(if self.config.provider == ProviderId::Gemini {
                key.expose().to_owned()
            } else {
                format!("Bearer {}", key.expose())
            });
            let mut header = HeaderValue::from_str(&value).map_err(|_| invalid())?;
            header.set_sensitive(true);
            builder = if self.config.provider == ProviderId::Gemini {
                builder.header("x-goog-api-key", header)
            } else {
                builder.header(AUTHORIZATION, header)
            };
        } else if self.config.provider != ProviderId::OpenAiCompatible {
            return Err(ProviderError::new(ErrorKind::MissingCredential));
        }
        if self.config.provider == ProviderId::Anthropic {
            builder = builder.header("anthropic-version", "2026-10-21");
        }
        Ok(builder)
    }

    fn run<T>(
        &self,
        future: impl Future<Output = ProviderResult<T>>,
        cancel: &AtomicBool,
    ) -> ProviderResult<T> {
        self.runtime
            .as_ref()
            .ok_or_else(transport)?
            .block_on(async {
                let started = Instant::now();
                let mut pending = pin!(future);
                loop {
                    check_cancel(cancel)?;
                    if started.elapsed() >= self.timeout {
                        return Err(ProviderError::new(ErrorKind::Timeout));
                    }
                    let delay = pin!(tokio::time::sleep(
                        CANCEL_POLL.min(self.timeout.saturating_sub(started.elapsed()))
                    ));
                    // Retain the same request future across polls: a slow response is not a failed read.
                    match select(pending.as_mut(), delay).await {
                        Either::Left((result, _)) => {
                            check_cancel(cancel)?;
                            return result;
                        }
                        Either::Right(_) => {}
                    }
                }
            })
    }

    async fn stream(
        &self,
        builder: reqwest::RequestBuilder,
        key: Option<&Secret>,
        cancel: &AtomicBool,
        mut emit: impl FnMut(StreamEvent),
    ) -> ProviderResult<ResponseState> {
        let mut response = self
            .response(builder.send().await.map_err(http_error)?)
            .await?;
        if response
            .headers()
            .get("Content-Type")
            .and_then(|value| value.to_str().ok())
            .is_none_or(|value| {
                !value
                    .split(';')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .eq_ignore_ascii_case("text/event-stream")
            })
        {
            return Err(protocol_error());
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_STREAM_BYTES as u64)
        {
            return Err(ProviderError::new(ErrorKind::LimitExceeded));
        }
        let mut decoder = Decoder::default();
        let mut state = ResponseState::new(key);
        while let Some(bytes) = response.chunk().await.map_err(http_error)? {
            decoder.push(&bytes, |event| {
                check_cancel(cancel)?;
                state.event(
                    self.config.provider,
                    &event.name,
                    &event.data,
                    key,
                    &mut emit,
                )
            })?;
            if state.terminal {
                break;
            }
        }
        if self.config.provider == ProviderId::Gemini && state.finish.is_some() {
            state.terminal = true;
        }
        if !state.terminal || state.finish.is_none() || state.text.trim().is_empty() {
            return Err(protocol_error());
        }
        state.publish_text(true, &mut emit)?;
        Ok(state)
    }

    async fn response(&self, response: Response) -> ProviderResult<Response> {
        let status = response.status();
        if status.is_redirection() {
            return Err(ProviderError {
                kind: ErrorKind::RedirectBlocked,
                status: Some(status.as_u16()),
                retry_after_seconds: None,
            });
        }
        if status.is_success() {
            return Ok(response);
        }
        let retry_after = response
            .headers()
            .get("Retry-After")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value <= 86400);
        let value = self.json(response, 64 * 1024).await.unwrap_or(Value::Null);
        let mut error = protocol::remote_error(&value, Some(status.as_u16()));
        error.retry_after_seconds = retry_after;
        Err(error)
    }

    async fn json(&self, mut response: Response, max: usize) -> ProviderResult<Value> {
        if response
            .content_length()
            .is_some_and(|length| length > max as u64)
        {
            return Err(ProviderError::new(ErrorKind::LimitExceeded));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(http_error)? {
            if bytes.len() + chunk.len() > max {
                return Err(ProviderError::new(ErrorKind::LimitExceeded));
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| protocol_error())
    }
}

impl Drop for ProviderClient {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

// System DNS cannot be interrupted, so one process-wide slot bounds work even after cancellation.
static DNS_ACTIVE: AtomicBool = AtomicBool::new(false);
struct BoundedDns;
impl reqwest::dns::Resolve for BoundedDns {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let name = name.as_str().to_owned();
        Box::pin(async move {
            if DNS_ACTIVE
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                return Err(std::io::Error::other("DNS resolver is busy").into());
            }
            struct Permit;
            impl Drop for Permit {
                fn drop(&mut self) {
                    DNS_ACTIVE.store(false, Ordering::Release);
                }
            }
            let permit = Permit;
            let addresses = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                (name.as_str(), 0)
                    .to_socket_addrs()
                    .map(|addresses| addresses.take(16).collect::<Vec<_>>())
            })
            .await??;
            Ok(Box::new(addresses.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

fn check_cancel(cancel: &AtomicBool) -> ProviderResult<()> {
    if cancel.load(Ordering::Acquire) {
        Err(ProviderError::new(ErrorKind::Cancelled))
    } else {
        Ok(())
    }
}
fn invalid() -> ProviderError {
    ProviderError::new(ErrorKind::InvalidConfig)
}
fn protocol_error() -> ProviderError {
    ProviderError::new(ErrorKind::Protocol)
}
fn transport() -> ProviderError {
    ProviderError::new(ErrorKind::Transport)
}
fn http_error(error: reqwest::Error) -> ProviderError {
    if error.is_timeout() {
        ProviderError::new(ErrorKind::Timeout)
    } else {
        transport()
    }
}

#[cfg(test)]
mod tests;
