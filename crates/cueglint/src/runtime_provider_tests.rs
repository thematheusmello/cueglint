use super::*;
use cueglint_core::{CredentialRef, ProviderId, RequestStatus};
use cueglint_providers::ProviderResult;
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::Mutex,
    time::Instant,
};

const KEY: &str = "cueglint-synthetic-key-never-valid";

#[derive(Clone, Default)]
struct MemoryCredentials(Arc<Mutex<BTreeSet<String>>>);

fn scope(reference: &CredentialRef) -> String {
    format!("{}:{}", reference.provider().as_str(), reference.endpoint())
}

impl CredentialStore for MemoryCredentials {
    fn status(&self, reference: &CredentialRef) -> CredentialStatus {
        if self.0.lock().unwrap().contains(&scope(reference)) {
            CredentialStatus::Stored
        } else {
            CredentialStatus::Missing
        }
    }
    fn store(&self, reference: &CredentialRef, _: Secret) -> ProviderResult<()> {
        self.0.lock().unwrap().insert(scope(reference));
        Ok(())
    }
    fn delete(&self, reference: &CredentialRef) -> ProviderResult<()> {
        self.0.lock().unwrap().remove(&scope(reference));
        Ok(())
    }
    fn load(&self, reference: &CredentialRef) -> ProviderResult<Option<Secret>> {
        if self.status(reference) == CredentialStatus::Stored {
            Secret::new(KEY.into()).map(Some)
        } else {
            Ok(None)
        }
    }
}

fn accept(listener: &TcpListener) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                return stream;
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                thread::sleep(Duration::from_millis(5))
            }
            Err(error) => panic!("loopback fixture accept failed: {error}"),
        }
    }
}

fn read_request(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut request = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0, "loopback request ended before its complete body");
        request.extend_from_slice(&buffer[..count]);
        assert!(request.len() < 128 * 1024);
        if let Some(start) = request.windows(4).position(|value| value == b"\r\n\r\n") {
            let headers = std::str::from_utf8(&request[..start]).unwrap();
            let length = headers
                .lines()
                .find_map(|line| {
                    line.split_once(':')
                        .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                })
                .map(|(_, value)| value.trim().parse::<usize>().unwrap())
                .unwrap_or(0);
            if request.len() >= start + 4 + length {
                break;
            }
        }
    }
    String::from_utf8(request).unwrap()
}

pub(crate) fn sse_answer(model: &str) -> String {
    format!(
        "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
        serde_json::json!({"model": model, "choices": [{"delta": {"content": "Original fixture answer"}}]}),
        serde_json::json!({"model": model, "choices": [{"delta": {}, "finish_reason": "stop"}], "usage": {"prompt_tokens": 9, "completion_tokens": 3}})
    )
}

pub(crate) fn fixture_server(
    responses: Vec<(u16, String)>,
) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!(
        "http://{}/cueglint-test-{}/v1",
        listener.local_addr().unwrap(),
        uuid()
    );
    let (tx, requests) = mpsc::channel();
    let server = thread::spawn(move || {
        for (status, body) in responses {
            let mut stream = accept(&listener);
            tx.send(read_request(&mut stream)).unwrap();
            write!(stream, "HTTP/1.1 {status} Fixture\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    (endpoint, requests, server)
}

fn configuration(endpoint: String, model: &str) -> ProviderConfig {
    ProviderConfig {
        provider: ProviderId::OpenAiCompatible,
        model_id: model.into(),
        base_url: Some(endpoint),
    }
}

fn runtime(store: &Store, credentials: MemoryCredentials) -> Runtime {
    let mut runtime = Runtime::with_credentials(
        &store.settings().unwrap(),
        true,
        egui::Context::default(),
        Box::new(credentials),
    )
    .unwrap();
    runtime.fixture = false;
    runtime
}

fn finish_provider(runtime: &mut Runtime, store: &mut Store) {
    while runtime.provider_job.is_some() {
        let event = runtime.events.recv_timeout(Duration::from_secs(5)).unwrap();
        runtime.apply(store, event).unwrap();
    }
}

fn finish_answer(runtime: &mut Runtime, store: &mut Store) -> (bool, Vec<String>) {
    let mut streamed = false;
    let mut notices = Vec::new();
    while runtime
        .snapshot
        .generation
        .as_ref()
        .is_some_and(|status| status.state == "generating")
    {
        let event = runtime.events.recv_timeout(Duration::from_secs(5)).unwrap();
        streamed |= matches!(&event, Event::GenerationText(_, text) if !text.is_empty());
        if let Some(notice) = runtime.apply(store, event).unwrap().1 {
            notices.push(notice);
        }
    }
    (streamed, notices)
}

#[test]
fn key_save_is_separate_from_selection_and_real_actor_streams_without_local_files() {
    let (endpoint, requests, server) = fixture_server(vec![(200, sse_answer("original-model-a"))]);
    let config = configuration(endpoint, "original-model-a");
    let mut store = Store::open_in_memory().unwrap();
    let session = store
        .start_session("[Fixture] Provider integration", None)
        .unwrap()
        .id;
    let mut runtime = runtime(&store, MemoryCredentials::default());
    assert!(
        store
            .settings()
            .unwrap()
            .answer_provider
            .provider
            .is_local()
    );
    runtime
        .provider_operation(
            uuid(),
            config.clone(),
            ProviderAction::Save(Secret::new(KEY.into()).unwrap()),
        )
        .unwrap();
    finish_provider(&mut runtime, &mut store);
    assert!(
        requests.try_recv().is_err(),
        "saving a key must not make a network request"
    );
    assert!(
        store
            .settings()
            .unwrap()
            .answer_provider
            .provider
            .is_local()
    );
    assert!(store.session(&session).unwrap().requests.is_empty());
    assert_eq!(
        runtime.snapshot.provider.as_ref().unwrap().credential,
        Some(CredentialStatus::Stored)
    );
    let mut settings = store.settings().unwrap();
    settings.answer_provider = config.clone();
    store.update_settings(settings).unwrap();
    runtime.settings_changed(&mut store).unwrap();
    assert!(
        requests.try_recv().is_err(),
        "selection must not make a network request"
    );
    assert!(runtime.snapshot.installed_profiles.is_empty());
    runtime
        .ask(&mut store, &session, "Original tiny question".into())
        .unwrap();
    let (streamed, notices) = finish_answer(&mut runtime, &mut store);
    assert!(streamed && notices.is_empty());
    let sent = requests.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(sent.starts_with("POST /cueglint-test-") && sent.contains("/v1/chat/completions "));
    assert!(sent.contains(KEY));
    assert!(sent.contains("Original tiny question"));
    let saved = store.session(&session).unwrap();
    assert_eq!(saved.requests[0].provider, config);
    assert_eq!(saved.requests[0].status, RequestStatus::Completed);
    assert!(saved.suggestions[0].inference.is_none());
    let metadata = saved.suggestions[0].hosted_inference.as_ref().unwrap();
    assert_eq!(metadata.returned_model.as_deref(), Some("original-model-a"));
    assert!(metadata.model_version.is_none());
    assert_eq!(metadata.usage.output_tokens, Some(3));
    assert!(!store.backup_json().unwrap().contains(KEY));
    assert!(
        !serde_json::to_string(&store.settings().unwrap())
            .unwrap()
            .contains(KEY)
    );
    runtime
        .provider_operation(uuid(), config.clone(), ProviderAction::Delete)
        .unwrap();
    finish_provider(&mut runtime, &mut store);
    assert_eq!(
        runtime.snapshot.provider.as_ref().unwrap().credential,
        Some(CredentialStatus::Missing)
    );
    assert_eq!(store.settings().unwrap().answer_provider, config);
    server.join().unwrap();
}

#[test]
fn provider_error_keeps_context_and_draft_retry_uses_a_new_frozen_request() {
    let (endpoint, requests, server) = fixture_server(vec![
        (500, KEY.into()),
        (200, sse_answer("original-model-b")),
    ]);
    let config = configuration(endpoint, "original-model-b");
    let mut store = Store::open_in_memory().unwrap();
    let session = store
        .start_session("[Fixture] Error recovery", None)
        .unwrap()
        .id;
    store
        .update_notes(&session, "Original notes must survive the provider failure")
        .unwrap();
    let mut settings = store.settings().unwrap();
    settings.answer_provider = config;
    store.update_settings(settings).unwrap();
    let mut runtime = runtime(&store, MemoryCredentials::default());
    runtime
        .ask(&mut store, &session, "Original recovery question".into())
        .unwrap();
    let (_, notices) = finish_answer(&mut runtime, &mut store);
    assert_eq!(runtime.snapshot.generation.as_ref().unwrap().state, "error");
    assert!(!notices.is_empty() && notices.iter().all(|notice| !notice.contains(KEY)));
    assert_eq!(
        store.session(&session).unwrap().requests[0].status,
        RequestStatus::Cancelled
    );
    runtime
        .ask(&mut store, &session, "Original recovery question".into())
        .unwrap();
    assert!(finish_answer(&mut runtime, &mut store).1.is_empty());
    let saved = store.session(&session).unwrap();
    assert_eq!(saved.requests.len(), 2);
    assert_ne!(saved.requests[0].id, saved.requests[1].id);
    assert_eq!(saved.requests[1].status, RequestStatus::Completed);
    assert_eq!(
        saved.notes,
        "Original notes must survive the provider failure"
    );
    assert_eq!(
        requests.try_iter().count(),
        2,
        "no automatic retry or provider fallback"
    );
    server.join().unwrap();
}

#[test]
fn provider_change_cancels_the_frozen_job_and_old_session_streams_stay_retired() {
    let mut store = Store::open_in_memory().unwrap();
    let first = store
        .start_session("[Fixture] Old session", None)
        .unwrap()
        .id;
    let mut settings = store.settings().unwrap();
    settings.answer_provider = configuration("http://127.0.0.1:1/v1".into(), "original-model-a");
    store.update_settings(settings.clone()).unwrap();
    let request = store
        .begin_request(&first, "Original stale question")
        .unwrap();
    let mut runtime = runtime(&store, MemoryCredentials::default());
    let cancel = Arc::new(AtomicBool::new(false));
    runtime.generation_cancel = Some(cancel.clone());
    runtime.snapshot.generation = Some(GenerationStatus {
        session_id: first.clone(),
        request_id: request.id.clone(),
        state: "generating".into(),
        text: "retired".into(),
    });
    settings.answer_provider.model_id = "original-model-b".into();
    store.update_settings(settings.clone()).unwrap();
    runtime.settings_changed(&mut store).unwrap();
    assert!(cancel.load(Ordering::Relaxed));
    assert_eq!(
        store.session(&first).unwrap().requests[0].provider.model_id,
        "original-model-a"
    );
    assert_eq!(
        store.session(&first).unwrap().requests[0].status,
        RequestStatus::Cancelled
    );
    store.stop_session(&first).unwrap();
    let second = store
        .start_session("[Fixture] New session", None)
        .unwrap()
        .id;
    let next = store
        .begin_request(&second, "Original current question")
        .unwrap();
    runtime.snapshot.generation = Some(GenerationStatus {
        session_id: second.clone(),
        request_id: next.id.clone(),
        state: "generating".into(),
        text: String::new(),
    });
    runtime
        .apply(
            &mut store,
            Event::GenerationText(request.id.clone(), "stale remote text".into()),
        )
        .unwrap();
    runtime
        .apply(
            &mut store,
            Event::HostedDone(request.id, Err("stale remote error".into())),
        )
        .unwrap();
    assert!(
        runtime
            .snapshot
            .generation
            .as_ref()
            .unwrap()
            .text
            .is_empty()
    );
    assert_eq!(
        runtime.snapshot.generation.as_ref().unwrap().request_id,
        next.id
    );
    assert_eq!(next.provider, settings.answer_provider);
    assert!(store.session(&second).unwrap().suggestions.is_empty());
}

#[test]
fn a_blocked_native_key_operation_does_not_block_answer_cancellation() {
    struct BlockingCredentials {
        started: SyncSender<()>,
        release: Mutex<Receiver<()>>,
        returned: Arc<AtomicBool>,
    }
    impl CredentialStore for BlockingCredentials {
        fn status(&self, _: &CredentialRef) -> CredentialStatus {
            CredentialStatus::Stored
        }
        fn store(&self, _: &CredentialRef, _: Secret) -> ProviderResult<()> {
            self.started.send(()).unwrap();
            let released = self
                .release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .is_ok();
            self.returned.store(true, Ordering::Relaxed);
            assert!(
                released,
                "the store actor must remain able to cancel while native storage is blocked"
            );
            Ok(())
        }
        fn delete(&self, _: &CredentialRef) -> ProviderResult<()> {
            Ok(())
        }
        fn load(&self, _: &CredentialRef) -> ProviderResult<Option<Secret>> {
            Ok(None)
        }
    }
    let mut store = Store::open_in_memory().unwrap();
    let session = store
        .start_session("[Fixture] Key modal cancellation", None)
        .unwrap()
        .id;
    let request = store
        .begin_request(&session, "Original pending question")
        .unwrap();
    let (started_tx, started) = mpsc::sync_channel(1);
    let (release, release_rx) = mpsc::sync_channel(1);
    let returned = Arc::new(AtomicBool::new(false));
    let credentials = BlockingCredentials {
        started: started_tx,
        release: Mutex::new(release_rx),
        returned: returned.clone(),
    };
    let mut runtime = Runtime::with_credentials(
        &store.settings().unwrap(),
        true,
        egui::Context::default(),
        Box::new(credentials),
    )
    .unwrap();
    runtime.fixture = false;
    let cancelled = Arc::new(AtomicBool::new(false));
    runtime.generation_cancel = Some(cancelled.clone());
    runtime.snapshot.generation = Some(GenerationStatus {
        session_id: session.clone(),
        request_id: request.id.clone(),
        state: "generating".into(),
        text: String::new(),
    });
    runtime
        .provider_operation(
            uuid(),
            ProviderConfig {
                provider: ProviderId::OpenAi,
                model_id: "original-model".into(),
                base_url: None,
            },
            ProviderAction::Save(Secret::new(KEY.into()).unwrap()),
        )
        .unwrap();
    started.recv_timeout(Duration::from_secs(3)).unwrap();
    runtime.cancel(&mut store).unwrap();
    assert!(cancelled.load(Ordering::Relaxed));
    assert!(
        !returned.load(Ordering::Relaxed),
        "Cancel must run before the native credential call returns"
    );
    assert_eq!(
        store.session(&session).unwrap().requests[0].status,
        RequestStatus::Cancelled
    );
    runtime.cancel_provider();
    assert_eq!(
        runtime.snapshot.provider.as_ref().unwrap().state,
        "cancelled"
    );
    assert!(
        runtime
            .ensure_restore_idle()
            .unwrap_err()
            .to_string()
            .contains("provider operation")
    );
    release.send(()).unwrap();
    let event = runtime.events.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(matches!(event, Event::ProviderDone(..)));
    runtime.apply(&mut store, event).unwrap();
    runtime.ensure_restore_idle().unwrap();
}

#[test]
fn queued_key_save_survives_explicit_use_provider_and_acknowledges_its_exact_scope() {
    struct QueuedCredentials {
        started: SyncSender<()>,
        release: Mutex<Receiver<()>>,
        stored: Arc<AtomicBool>,
    }
    impl CredentialStore for QueuedCredentials {
        fn status(&self, reference: &CredentialRef) -> CredentialStatus {
            if reference.provider() == ProviderId::OpenAi && self.stored.load(Ordering::Relaxed) {
                CredentialStatus::Stored
            } else {
                CredentialStatus::Missing
            }
        }
        fn store(&self, reference: &CredentialRef, _: Secret) -> ProviderResult<()> {
            assert_eq!(reference.provider(), ProviderId::OpenAi);
            self.stored.store(true, Ordering::Relaxed);
            Ok(())
        }
        fn delete(&self, _: &CredentialRef) -> ProviderResult<()> {
            Ok(())
        }
        fn load(&self, reference: &CredentialRef) -> ProviderResult<Option<Secret>> {
            assert_eq!(reference.provider(), ProviderId::OpenAiCompatible);
            self.started.send(()).unwrap();
            self.release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            Ok(None)
        }
    }
    let mut store = Store::open_in_memory().unwrap();
    let original = configuration(
        format!("http://127.0.0.1:1/cueglint-test-{}/v1", uuid()),
        "original-old-model",
    );
    let selected = ProviderConfig {
        provider: ProviderId::OpenAi,
        model_id: "original-new-model".into(),
        base_url: None,
    };
    let mut settings = store.settings().unwrap();
    settings.answer_provider = original.clone();
    store.update_settings(settings.clone()).unwrap();
    let session = store
        .start_session("[Fixture] Queued Save then Use", None)
        .unwrap()
        .id;
    let (started_tx, started) = mpsc::sync_channel(1);
    let (release, release_rx) = mpsc::sync_channel(1);
    let stored = Arc::new(AtomicBool::new(false));
    let credentials = QueuedCredentials {
        started: started_tx,
        release: Mutex::new(release_rx),
        stored: stored.clone(),
    };
    let mut runtime = Runtime::with_credentials(
        &store.settings().unwrap(),
        true,
        egui::Context::default(),
        Box::new(credentials),
    )
    .unwrap();
    runtime.fixture = false;
    runtime
        .ask(
            &mut store,
            &session,
            "Original soon-cancelled question".into(),
        )
        .unwrap();
    started.recv_timeout(Duration::from_secs(3)).unwrap();
    let id = uuid();
    runtime
        .provider_operation(
            id.clone(),
            selected.clone(),
            ProviderAction::Save(Secret::new(KEY.into()).unwrap()),
        )
        .unwrap();
    assert_eq!(store.settings().unwrap().answer_provider, original);
    assert!(
        !stored.load(Ordering::Relaxed),
        "save is deterministically queued behind the held inference actor"
    );
    settings.answer_provider = selected.clone();
    store.update_settings(settings).unwrap();
    runtime.settings_changed(&mut store).unwrap();
    assert_eq!(runtime.provider_job.as_deref(), Some(id.as_str()));
    assert!(
        !runtime
            .provider_cancel
            .as_ref()
            .unwrap()
            .load(Ordering::Relaxed)
    );
    assert_eq!(
        store.session(&session).unwrap().requests[0].status,
        RequestStatus::Cancelled
    );
    release.send(()).unwrap();
    finish_provider(&mut runtime, &mut store);
    assert!(stored.load(Ordering::Relaxed));
    let acknowledgement = runtime.snapshot.provider.as_ref().unwrap();
    assert_eq!(acknowledgement.id, id);
    assert_eq!(
        acknowledgement.config.credential_ref().unwrap(),
        selected.credential_ref().unwrap()
    );
    assert_eq!(acknowledgement.credential, Some(CredentialStatus::Stored));
    assert_eq!(acknowledgement.state, "ready");
    assert_eq!(store.settings().unwrap().answer_provider, selected);
    assert!(store.session(&session).unwrap().suggestions.is_empty());
}
