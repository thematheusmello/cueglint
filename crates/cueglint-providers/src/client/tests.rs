//! Original protocol mocks over real loopback HTTP. These tests do not establish live vendor access.
use super::*;
use crate::CredentialStore;
use cueglint_core::{CredentialRef, CredentialStatus};
use serde_json::json;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    thread::{self, JoinHandle},
};

const KEY: &str = "cueglint-original-protocol-test-key";

#[derive(Debug)]
struct Captured {
    target: String,
    headers: Vec<(String, String)>,
    body: Value,
}
impl Captured {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

struct Reply {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
    headers: Vec<(String, String)>,
    delay_headers: Duration,
    delay_body: Duration,
}
impl Reply {
    fn sse(body: String) -> Self {
        Self {
            status: 200,
            content_type: "text/event-stream; charset=utf-8",
            body: body.into_bytes(),
            headers: Vec::new(),
            delay_headers: Duration::ZERO,
            delay_body: Duration::ZERO,
        }
    }
    fn json(value: Value) -> Self {
        Self {
            content_type: "application/json",
            body: serde_json::to_vec(&value).unwrap(),
            ..Self::sse(String::new())
        }
    }
}

struct Server {
    endpoint: String,
    handle: JoinHandle<Vec<Captured>>,
}
impl Server {
    fn new(replies: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            for reply in replies {
                let started = Instant::now();
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && started.elapsed() < Duration::from_secs(5) =>
                        {
                            thread::sleep(Duration::from_millis(5))
                        }
                        Err(error) => panic!("mock accept failed: {error}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                requests.push(read_request(&mut stream));
                thread::sleep(reply.delay_headers);
                let mut head = format!(
                    "HTTP/1.1 {} Mock\r\nContent-Type: {}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n",
                    reply.status, reply.content_type
                );
                for (name, value) in reply.headers {
                    head.push_str(&format!("{name}: {value}\r\n"));
                }
                head.push_str("\r\n");
                if stream.write_all(head.as_bytes()).is_err() {
                    continue;
                }
                stream.flush().unwrap_or_default();
                thread::sleep(reply.delay_body);
                // The HTTP decoder receives byte-sized chunks, so split UTF-8 reaches the real client.
                for chunk in reply.body.chunks(1) {
                    if stream
                        .write_all(b"1\r\n")
                        .and_then(|_| stream.write_all(chunk))
                        .and_then(|_| stream.write_all(b"\r\n"))
                        .is_err()
                    {
                        break;
                    }
                }
                let _ = stream.write_all(b"0\r\n\r\n");
            }
            requests
        });
        Self { endpoint, handle }
    }
    fn finish(self) -> Vec<Captured> {
        self.handle.join().unwrap()
    }
}

fn read_request(stream: &mut TcpStream) -> Captured {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut byte = [0];
    while !bytes.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).unwrap();
        bytes.push(byte[0]);
        assert!(bytes.len() <= 64 * 1024);
    }
    let head = String::from_utf8(bytes).unwrap();
    let mut lines = head.split("\r\n");
    let target = lines
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_owned();
    let headers: Vec<_> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.to_owned(), value.trim().to_owned()))
        .collect();
    let len = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .map(|(_, value)| value.parse::<usize>().unwrap())
        .unwrap_or(0);
    assert!(len <= protocol::MAX_REQUEST_BYTES);
    let mut body = vec![0; len];
    stream.read_exact(&mut body).unwrap();
    Captured {
        target,
        headers,
        body: if body.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&body).unwrap()
        },
    }
}

fn selected(provider: ProviderId, endpoint: &str) -> ProviderConfig {
    ProviderConfig {
        provider,
        model_id: "original/mock-model:q4".to_owned(),
        base_url: (provider == ProviderId::OpenAiCompatible).then(|| endpoint.to_owned()),
    }
}

fn mock_client(provider: ProviderId, endpoint: &str) -> ProviderClient {
    let mut config = selected(provider, endpoint);
    if provider == ProviderId::Gemini {
        config.model_id = "original-mock-model".to_owned();
    }
    let mut client = ProviderClient::new(config).unwrap();
    // This private test-only override can never be requested by production callers.
    client.endpoint = Url::parse(endpoint).unwrap();
    client
}

fn frozen(client: &ProviderClient) -> GenerationRequest {
    GenerationRequest {
        id: "original-request-001".to_owned(),
        session_id: "original-session-001".to_owned(),
        prompt: "Answer this original protocol fixture.".to_owned(),
        context: ContextSnapshot {
            revision: 4,
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
        provider: client.config.clone(),
    }
}

fn event(value: Value) -> String {
    format!("data: {}\r\n\r\n", serde_json::to_string(&value).unwrap())
}
fn named(name: &str, value: Value) -> String {
    format!("event: {name}\n{}", event(value))
}

fn stream(provider: ProviderId) -> String {
    let mut body = ": original mock heartbeat\r\n\r\n".to_owned();
    match provider {
        ProviderId::OpenAi | ProviderId::OpenRouter | ProviderId::OpenAiCompatible => {
            body += &event(
                json!({"id":"mock-response-001","model":"reported-model","choices":[{"index":0,"delta":{"content":"Olá "},"finish_reason":null}]}),
            );
            body += &event(
                json!({"id":"mock-response-001","model":"reported-model","choices":[{"index":0,"delta":{"content":"🌍"},"finish_reason":null}]}),
            );
            body += &event(json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}));
            if provider == ProviderId::OpenRouter {
                body += &event(
                    json!({"choices":[{"index":0,"delta":{"content":""},"finish_reason":"stop","native_finish_reason":"stop"}],"usage":{"prompt_tokens":7,"completion_tokens":2}}),
                );
            } else if provider == ProviderId::OpenAi {
                body +=
                    &event(json!({"choices":[],"usage":{"prompt_tokens":7,"completion_tokens":2}}));
            }
            body += "data: [DONE]\n\n";
        }
        ProviderId::Anthropic => {
            body += &named(
                "message_start",
                json!({"type":"message_start","message":{"id":"mock-response-001","model":"reported-model","usage":{"input_tokens":7,"output_tokens":0}}}),
            );
            body += &named(
                "content_block_start",
                json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            );
            body += &named(
                "content_block_delta",
                json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Olá 🌍"}}),
            );
            body += &named(
                "message_delta",
                json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":2}}),
            );
            body += &named("message_stop", json!({"type":"message_stop"}));
        }
        ProviderId::Gemini => {
            body += &event(
                json!({"responseId":"mock-response-001","modelVersion":"reported-version","candidates":[{"index":0,"content":{"parts":[{"text":"hidden reasoning","thought":true},{"text":"Olá 🌍"}]}}]}),
            );
            body += &event(
                json!({"responseId":"mock-response-001","modelVersion":"reported-version","candidates":[{"index":0,"finishReason":"STOP","citationMetadata":{"citationSources":[{"uri":"https://example.com/original-source"}]}}],"usageMetadata":{"promptTokenCount":7,"candidatesTokenCount":2}}),
            );
        }
        ProviderId::Perplexity => {
            body += &named(
                "response.created",
                json!({"type":"response.created","response":{"id":"mock-response-001","model":"reported-model","status":"in_progress"}}),
            );
            body += &named(
                "response.output_text.delta",
                json!({"type":"response.output_text.delta","delta":"Olá 🌍"}),
            );
            body += &named(
                "response.output_item.done",
                json!({"type":"response.output_item.done","item":{"type":"search_results","results":[{"url":"https://example.com/original-source","title":"Original source"}]}}),
            );
            body += &named(
                "response.completed",
                json!({"type":"response.completed","response":{"id":"mock-response-001","model":"reported-model","status":"completed","output":[],"usage":{"input_tokens":7,"output_tokens":2}}}),
            );
        }
        ProviderId::Local => unreachable!(),
    }
    body
}

#[test]
fn mock_all_six_real_http_clients_stream_fragmented_utf8_and_send_provider_specific_auth_and_bodies()
 {
    for provider in ProviderId::ALL
        .into_iter()
        .filter(|provider| !provider.is_local())
    {
        let server = Server::new(vec![Reply::sse(stream(provider))]);
        let client = mock_client(provider, &server.endpoint);
        let key = Secret::new(KEY.to_owned()).unwrap();
        let mut events = Vec::new();
        let output = client
            .generate(
                &frozen(&client),
                128,
                Some(&key),
                &AtomicBool::new(false),
                |event| events.push(event),
            )
            .unwrap();
        assert_eq!(output.text, "Olá 🌍");
        assert_eq!(output.metadata.requested_model, client.config.model_id);
        assert_eq!(
            output.metadata.request_id.as_deref(),
            Some("mock-response-001")
        );
        assert_eq!(output.metadata.provider, provider);
        assert_eq!(output.metadata.prompt_sha256.len(), 64);
        if provider == ProviderId::OpenAiCompatible {
            assert_eq!(output.metadata.usage, cueglint_core::TokenUsage::default());
        } else {
            assert_eq!(output.metadata.usage.input_tokens, Some(7));
            assert_eq!(output.metadata.usage.output_tokens, Some(2));
        }
        if provider == ProviderId::Gemini {
            assert_eq!(
                output.metadata.model_version.as_deref(),
                Some("reported-version")
            );
            assert_eq!(output.metadata.returned_model, None);
        } else {
            assert_eq!(
                output.metadata.returned_model.as_deref(),
                Some("reported-model")
            );
            assert_eq!(output.metadata.model_version, None);
        }
        let text: String = events
            .iter()
            .filter_map(|event| {
                if let StreamEvent::TextDelta(text) = event {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(text, output.text);
        if matches!(provider, ProviderId::Gemini | ProviderId::Perplexity) {
            assert_eq!(output.metadata.citations.len(), 1);
        }
        let requests = server.finish();
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        assert_eq!(request.header("Content-Type"), Some("application/json"));
        assert!(!serde_json::to_string(&request.body).unwrap().contains(KEY));
        if provider == ProviderId::Gemini {
            assert_eq!(request.header("x-goog-api-key"), Some(KEY));
            assert_eq!(request.header("Authorization"), None);
            assert!(request.target.ends_with(":streamGenerateContent?alt=sse"));
            assert_eq!(request.body["generationConfig"]["maxOutputTokens"], 128);
            assert!(request.body.get("store").is_none());
            assert!(request.body.get("systemInstruction").is_some());
        } else {
            assert_eq!(
                request.header("Authorization"),
                Some(format!("Bearer {KEY}").as_str())
            );
            assert_eq!(request.header("x-goog-api-key"), None);
        }
        match provider {
            ProviderId::Anthropic => {
                assert_eq!(request.header("anthropic-version"), Some("2026-10-21"));
                assert_eq!(request.target, "/v1/messages");
                assert_eq!(request.body["max_tokens"], 128);
                assert!(request.body.get("system").is_some());
                assert!(request.body.get("temperature").is_none());
            }
            ProviderId::OpenAi => {
                assert_eq!(request.body["max_completion_tokens"], 128);
                assert_eq!(request.body["store"], false);
                assert_eq!(request.body["stream_options"]["include_usage"], true);
            }
            ProviderId::OpenRouter => {
                assert_eq!(request.body["provider"]["allow_fallbacks"], false);
                assert_eq!(request.body["max_tokens"], 128);
            }
            ProviderId::Perplexity => {
                assert_eq!(request.target, "/v1/agent");
                assert_eq!(request.body["max_steps"], 1);
                assert_eq!(request.body["tools"], json!([]));
                assert_eq!(request.body["store"], false);
                assert_eq!(request.body["max_output_tokens"], 128);
                assert_eq!(request.body["input"][0]["type"], "message");
            }
            _ => {}
        }
    }
}

#[test]
fn mock_perplexity_final_only_output_is_kept_without_duplicates_and_conflicting_text_is_rejected() {
    let completed = named(
        "response.completed",
        json!({
            "type":"response.completed", "response":{
                "id":"mock-final-only-001", "model":"reported-model", "status":"completed",
                "output":[{"type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"Original final-only answer 🌍","annotations":[]}]}],
                "usage":{"input_tokens":4,"output_tokens":6}
            }
        }),
    );
    for prefix in [
        String::new(),
        named(
            "response.output_text.delta",
            json!({"type":"response.output_text.delta","delta":"Original final-only answer 🌍"}),
        ),
    ] {
        let server = Server::new(vec![Reply::sse(prefix + &completed)]);
        let client = mock_client(ProviderId::Perplexity, &server.endpoint);
        let mut text = String::new();
        let output = client
            .generate(
                &frozen(&client),
                128,
                Some(&Secret::new(KEY.to_owned()).unwrap()),
                &AtomicBool::new(false),
                |event| {
                    if let StreamEvent::TextDelta(delta) = event {
                        text.push_str(&delta);
                    }
                },
            )
            .unwrap();
        assert_eq!(output.text, "Original final-only answer 🌍");
        assert_eq!(text, output.text);
        assert_eq!(output.metadata.usage.output_tokens, Some(6));
        assert_eq!(server.finish().len(), 1);
    }
    let prefix = named(
        "response.output_text.delta",
        json!({"type":"response.output_text.delta","delta":"Conflicting streamed answer"}),
    );
    let server = Server::new(vec![Reply::sse(prefix + &completed)]);
    let client = mock_client(ProviderId::Perplexity, &server.endpoint);
    assert_eq!(
        client
            .generate(
                &frozen(&client),
                128,
                Some(&Secret::new(KEY.to_owned()).unwrap()),
                &AtomicBool::new(false),
                |_| {}
            )
            .unwrap_err()
            .kind,
        ErrorKind::Protocol
    );
    server.finish();
}

#[test]
fn mock_catalogs_use_real_auth_pagination_and_honest_manual_perplexity_ids() {
    let key = Secret::new(KEY.to_owned()).unwrap();
    for provider in [
        ProviderId::OpenAi,
        ProviderId::OpenRouter,
        ProviderId::OpenAiCompatible,
        ProviderId::Anthropic,
        ProviderId::Gemini,
    ] {
        let values = match provider {
            ProviderId::Anthropic => vec![
                json!({"data":[{"id":"first","display_name":"Original first"}],"has_more":true,"last_id":"first"}),
                json!({"data":[{"id":"second","display_name":"Original second"}],"has_more":false}),
            ],
            ProviderId::Gemini => vec![
                json!({"models":[{"name":"models/first","displayName":"Original first","supportedGenerationMethods":["generateContent"]},{"name":"models/embedding","supportedGenerationMethods":["embedContent"]}],"nextPageToken":"original-next"}),
                json!({"models":[{"name":"models/second","supportedGenerationMethods":["generateContent"]}]}),
            ],
            _ => vec![json!({"data":[{"id":"first","name":"Original first"},{"id":"second"}]})],
        };
        let server = Server::new(values.into_iter().map(Reply::json).collect());
        let client = mock_client(provider, &server.endpoint);
        let catalog = client
            .list_models(Some(&key), &AtomicBool::new(false))
            .unwrap();
        assert_eq!(
            catalog
                .models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        assert!(catalog.manual_entry);
        assert!(!catalog.truncated);
        let requests = server.finish();
        if provider == ProviderId::Anthropic {
            assert!(requests[1].target.contains("after_id=first"));
            assert_eq!(requests[0].header("anthropic-version"), Some("2026-10-21"));
        }
        if provider == ProviderId::Gemini {
            assert!(requests[1].target.contains("pageToken=original-next"));
            assert_eq!(requests[0].header("x-goog-api-key"), Some(KEY));
        }
    }
    let client = ProviderClient::new(selected(ProviderId::Perplexity, "")).unwrap();
    let catalog = client.list_models(None, &AtomicBool::new(false)).unwrap();
    assert!(catalog.models.is_empty());
    assert!(catalog.manual_entry);
    assert_eq!(
        catalog.documentation_url.as_deref(),
        Some("https://docs.perplexity.ai/docs/agent-api/models")
    );
}

#[test]
fn mock_auth_rate_model_server_and_http200_errors_are_typed_and_redacted_without_retries() {
    for (status, code, expected) in [
        (401, "invalid_api_key", ErrorKind::Authentication),
        (429, "rate_limit_exceeded", ErrorKind::RateLimited),
        (404, "model_not_found", ErrorKind::ModelUnavailable),
        (400, "model_not_found", ErrorKind::ModelUnavailable),
        (503, "overloaded_error", ErrorKind::Server),
    ] {
        let mut reply = Reply::json(
            json!({"error":{"code":code,"message":format!("Never expose {KEY} or the original private prompt")}}),
        );
        reply.status = status;
        reply
            .headers
            .push(("Retry-After".to_owned(), "12".to_owned()));
        let server = Server::new(vec![reply]);
        let client = mock_client(ProviderId::OpenAiCompatible, &server.endpoint);
        let error = client
            .generate(
                &frozen(&client),
                128,
                Some(&Secret::new(KEY.to_owned()).unwrap()),
                &AtomicBool::new(false),
                |_| {},
            )
            .unwrap_err();
        assert_eq!(error.kind, expected);
        assert_eq!(error.status, Some(status));
        assert_eq!(error.retry_after_seconds, Some(12));
        assert!(!format!("{error:?} {error}").contains(KEY));
        assert!(!error.to_string().contains("private prompt"));
        assert_eq!(server.finish().len(), 1);
    }
    let server = Server::new(vec![Reply::sse(event(
        json!({"error":{"code":429,"message":KEY}}),
    ))]);
    let client = mock_client(ProviderId::OpenRouter, &server.endpoint);
    let error = client
        .generate(
            &frozen(&client),
            128,
            Some(&Secret::new(KEY.to_owned()).unwrap()),
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap_err();
    // Echoed credentials are rejected before interpreting or emitting response data.
    assert_eq!(error.kind, ErrorKind::Protocol);
    server.finish();
    let server = Server::new(vec![Reply::sse(event(
        json!({"error":{"code":429,"message":"original mock quota error"}}),
    ))]);
    let client = mock_client(ProviderId::OpenRouter, &server.endpoint);
    assert_eq!(
        client
            .generate(
                &frozen(&client),
                128,
                Some(&Secret::new(KEY.to_owned()).unwrap()),
                &AtomicBool::new(false),
                |_| {}
            )
            .unwrap_err()
            .kind,
        ErrorKind::RateLimited
    );
    server.finish();
}

#[test]
fn mock_midstream_errors_and_missing_vendor_terminals_never_complete() {
    for provider in ProviderId::ALL
        .into_iter()
        .filter(|provider| !provider.is_local())
    {
        let body = match provider {
            ProviderId::Anthropic => {
                stream(provider).replace(&named("message_stop", json!({"type":"message_stop"})), "")
            }
            ProviderId::Perplexity => named(
                "response.output_text.delta",
                json!({"type":"response.output_text.delta","delta":"incomplete"}),
            ),
            ProviderId::Gemini => {
                event(json!({"candidates":[{"content":{"parts":[{"text":"incomplete"}]}}]}))
            }
            _ => stream(provider).replace("data: [DONE]\n\n", ""),
        };
        let server = Server::new(vec![Reply::sse(body)]);
        let client = mock_client(provider, &server.endpoint);
        assert_eq!(
            client
                .generate(
                    &frozen(&client),
                    128,
                    Some(&Secret::new(KEY.to_owned()).unwrap()),
                    &AtomicBool::new(false),
                    |_| {}
                )
                .unwrap_err()
                .kind,
            ErrorKind::Protocol
        );
        server.finish();
    }
    let partial = event(json!({"choices":[{"delta":{"content":"original partial"}}]}));
    let server = Server::new(vec![Reply::sse(
        partial + &event(json!({"error":{"code":503,"message":"original stream failure"}})),
    )]);
    let client = mock_client(ProviderId::OpenAiCompatible, &server.endpoint);
    let mut deltas = Vec::new();
    assert_eq!(
        client
            .generate(
                &frozen(&client),
                128,
                None,
                &AtomicBool::new(false),
                |event| deltas.push(event)
            )
            .unwrap_err()
            .kind,
        ErrorKind::Server
    );
    assert!(matches!(&deltas[0], StreamEvent::TextDelta(text) if text == "original partial"));
    server.finish();
}

#[test]
fn mock_redirects_never_forward_keys_to_another_origin() {
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    target.set_nonblocking(true).unwrap();
    let mut reply = Reply::json(json!({}));
    reply.status = 307;
    reply.headers.push((
        "Location".to_owned(),
        format!("http://{}/stolen", target.local_addr().unwrap()),
    ));
    let server = Server::new(vec![reply]);
    let client = mock_client(ProviderId::OpenAiCompatible, &server.endpoint);
    assert_eq!(
        client
            .generate(
                &frozen(&client),
                128,
                Some(&Secret::new(KEY.to_owned()).unwrap()),
                &AtomicBool::new(false),
                |_| {}
            )
            .unwrap_err()
            .kind,
        ErrorKind::RedirectBlocked
    );
    assert!(
        matches!(target.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
    assert_eq!(server.finish().len(), 1);
}

#[test]
fn mock_stalled_headers_and_body_cancel_promptly_while_slow_stream_survives_poll_intervals() {
    for headers in [true, false] {
        let mut reply = Reply::sse(stream(ProviderId::OpenAiCompatible));
        if headers {
            reply.delay_headers = Duration::from_millis(600);
        } else {
            reply.delay_body = Duration::from_millis(600);
        }
        let server = Server::new(vec![reply]);
        let client = mock_client(ProviderId::OpenAiCompatible, &server.endpoint);
        let cancel = Arc::new(AtomicBool::new(false));
        let setter = cancel.clone();
        let trigger = thread::spawn(move || {
            thread::sleep(Duration::from_millis(150));
            setter.store(true, Ordering::Release);
        });
        let started = Instant::now();
        assert_eq!(
            client
                .generate(&frozen(&client), 128, None, &cancel, |_| {})
                .unwrap_err()
                .kind,
            ErrorKind::Cancelled
        );
        assert!(started.elapsed() < Duration::from_millis(450));
        trigger.join().unwrap();
        server.finish();
    }
    let mut reply = Reply::sse(stream(ProviderId::OpenAiCompatible));
    reply.delay_headers = Duration::from_millis(200);
    reply.delay_body = Duration::from_millis(250);
    let server = Server::new(vec![reply]);
    let client = mock_client(ProviderId::OpenAiCompatible, &server.endpoint);
    let started = Instant::now();
    assert_eq!(
        client
            .generate(&frozen(&client), 128, None, &AtomicBool::new(false), |_| {})
            .unwrap()
            .text,
        "Olá 🌍"
    );
    assert!(started.elapsed() >= Duration::from_millis(450));
    assert_eq!(server.finish().len(), 1);
}

#[test]
fn mock_stalled_tls_connect_and_dns_cancel_without_waiting_for_network_timeouts() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("https://{}/v1/", listener.local_addr().unwrap());
    let (seen_tx, seen_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut bytes = [0; 512];
        let len = stream.read(&mut bytes).unwrap();
        seen_tx.send(len).unwrap();
        thread::sleep(Duration::from_millis(600));
    });
    let client = mock_client(ProviderId::OpenAiCompatible, &endpoint);
    let cancel = Arc::new(AtomicBool::new(false));
    let setter = cancel.clone();
    let trigger = thread::spawn(move || {
        assert!(seen_rx.recv_timeout(Duration::from_secs(2)).unwrap() > 0);
        setter.store(true, Ordering::Release);
    });
    let started = Instant::now();
    assert_eq!(
        client
            .generate(&frozen(&client), 128, None, &cancel, |_| {})
            .unwrap_err()
            .kind,
        ErrorKind::Cancelled
    );
    assert!(started.elapsed() < Duration::from_millis(450));
    trigger.join().unwrap();
    server.join().unwrap();
    struct StalledDns;
    impl reqwest::dns::Resolve for StalledDns {
        fn resolve(&self, _: reqwest::dns::Name) -> reqwest::dns::Resolving {
            Box::pin(std::future::pending())
        }
    }
    let mut client = mock_client(
        ProviderId::OpenAiCompatible,
        "https://original-mock.invalid/v1/",
    );
    client.http = Client::builder()
        .no_proxy()
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .dns_resolver(Arc::new(StalledDns))
        .build()
        .unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let setter = cancel.clone();
    let trigger = thread::spawn(move || {
        thread::sleep(Duration::from_millis(150));
        setter.store(true, Ordering::Release);
    });
    let started = Instant::now();
    assert_eq!(
        client
            .generate(&frozen(&client), 128, None, &cancel, |_| {})
            .unwrap_err()
            .kind,
        ErrorKind::Cancelled
    );
    assert!(started.elapsed() < Duration::from_millis(450));
    trigger.join().unwrap();
}

#[test]
fn mock_global_deadline_bounds_a_stalled_stream() {
    let mut reply = Reply::sse(stream(ProviderId::OpenAiCompatible));
    reply.delay_body = Duration::from_millis(500);
    let server = Server::new(vec![reply]);
    let mut client = mock_client(ProviderId::OpenAiCompatible, &server.endpoint);
    client.timeout = Duration::from_millis(100);
    let started = Instant::now();
    assert_eq!(
        client
            .generate(&frozen(&client), 128, None, &AtomicBool::new(false), |_| {})
            .unwrap_err()
            .kind,
        ErrorKind::Timeout
    );
    assert!(started.elapsed() < Duration::from_millis(400));
    server.finish();
}

#[test]
fn config_mismatch_and_precancel_reject_before_credentials_or_a_socket_are_used() {
    struct NoReads;
    impl CredentialStore for NoReads {
        fn status(&self, _: &CredentialRef) -> CredentialStatus {
            panic!("must not read status")
        }
        fn load(&self, _: &CredentialRef) -> ProviderResult<Option<Secret>> {
            panic!("must not load a key")
        }
        fn store(&self, _: &CredentialRef, _: Secret) -> ProviderResult<()> {
            unreachable!()
        }
        fn delete(&self, _: &CredentialRef) -> ProviderResult<()> {
            unreachable!()
        }
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
    let client = mock_client(ProviderId::OpenAiCompatible, &endpoint);
    let mut request = frozen(&client);
    request.provider.model_id = "other-model".to_owned();
    assert_eq!(
        client
            .generate_with_store(&request, 128, &NoReads, &AtomicBool::new(false), |_| {})
            .unwrap_err()
            .kind,
        ErrorKind::InvalidConfig
    );
    request = frozen(&client);
    assert_eq!(
        client
            .generate_with_store(&request, 128, &NoReads, &AtomicBool::new(true), |_| {})
            .unwrap_err()
            .kind,
        ErrorKind::Cancelled
    );
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
}

#[test]
fn mock_secret_prefix_buffer_handles_overlap_utf8_and_ordinary_streaming() {
    let key = Secret::new("abababac".to_owned()).unwrap();
    let chunks = ["Olá ", "🌍 ", "aba", "bab", "ax ", "fim ababa"];
    let mut body = chunks
        .iter()
        .map(|text| event(json!({"choices":[{"delta":{"content":text}}]})))
        .collect::<String>();
    body += &event(json!({"choices":[{"delta":{},"finish_reason":"stop"}]}));
    body += "data: [DONE]\n\n";
    let server = Server::new(vec![Reply::sse(body)]);
    let client = mock_client(ProviderId::OpenAiCompatible, &server.endpoint);
    let mut deltas = Vec::new();
    let output = client
        .generate(
            &frozen(&client),
            128,
            Some(&key),
            &AtomicBool::new(false),
            |event| {
                if let StreamEvent::TextDelta(text) = event {
                    deltas.push(text);
                }
            },
        )
        .unwrap();
    assert_eq!(output.text, chunks.concat());
    assert_eq!(deltas.concat(), output.text);
    assert_eq!(deltas[0], "Olá ");
    assert!(
        deltas.len() > 2,
        "ordinary text must still stream before the terminal"
    );
    assert_eq!(
        deltas.last().unwrap(),
        "ababa",
        "a valid terminal releases an ordinary shared suffix"
    );
    server.finish();
    let body = event(json!({"choices":[{"delta":{"content":"Olá 🌍 abababababa"}}]}))
        + &event(json!({"choices":[{"delta":{"content":"c"}}]}));
    let server = Server::new(vec![Reply::sse(body)]);
    let client = mock_client(ProviderId::OpenAiCompatible, &server.endpoint);
    let mut shown = String::new();
    assert_eq!(
        client
            .generate(
                &frozen(&client),
                128,
                Some(&key),
                &AtomicBool::new(false),
                |event| {
                    if let StreamEvent::TextDelta(text) = event {
                        shown.push_str(&text);
                    }
                }
            )
            .unwrap_err()
            .kind,
        ErrorKind::Protocol
    );
    assert_eq!(
        shown, "Olá 🌍 abab",
        "retain the overlapping suffix that begins the eventual full echo"
    );
    server.finish();
}

#[test]
fn mock_response_limits_and_secret_echoes_fail_before_emission_or_persistence() {
    let secret = Secret::new(KEY.to_owned()).unwrap();
    for split in [1, 12, KEY.len() - 1] {
        let (first, second) = KEY.split_at(split);
        let body = event(json!({"choices":[{"delta":{"content":first}}]}))
            + &event(json!({"choices":[{"delta":{"content":second}}]}));
        let server = Server::new(vec![Reply::sse(body)]);
        let client = mock_client(ProviderId::OpenAiCompatible, &server.endpoint);
        let mut shown = String::new();
        assert_eq!(
            client
                .generate(
                    &frozen(&client),
                    128,
                    Some(&secret),
                    &AtomicBool::new(false),
                    |event| {
                        if let StreamEvent::TextDelta(text) = event {
                            shown.push_str(&text);
                        }
                    }
                )
                .unwrap_err()
                .kind,
            ErrorKind::Protocol
        );
        assert!(
            shown.is_empty(),
            "no credential prefix may cross the event boundary"
        );
        server.finish();
    }
    assert!(!format!("{secret:?}").contains(KEY));
    let body = format!("data: {}\n\n", "x".repeat(crate::sse::MAX_EVENT_BYTES + 1));
    let server = Server::new(vec![Reply::sse(body)]);
    let client = mock_client(ProviderId::OpenAiCompatible, &server.endpoint);
    assert_eq!(
        client
            .generate(&frozen(&client), 128, None, &AtomicBool::new(false), |_| {})
            .unwrap_err()
            .kind,
        ErrorKind::LimitExceeded
    );
    server.finish();
    let models = (0..protocol::MAX_MODELS + 1)
        .map(|index| json!({"id":format!("original-model-{index}")}))
        .collect::<Vec<_>>();
    let server = Server::new(vec![Reply::json(json!({"data":models}))]);
    let client = mock_client(ProviderId::OpenAiCompatible, &server.endpoint);
    let catalog = client.list_models(None, &AtomicBool::new(false)).unwrap();
    assert_eq!(catalog.models.len(), protocol::MAX_MODELS);
    assert!(catalog.truncated);
    server.finish();
}
