// Original Cueglint implementation. SPDX-License-Identifier: MIT OR Apache-2.0
use super::{Document, DocumentKey, Limits, ParseError, ParseWorker};
use crate::runtime::GenerationStatus;
use cueglint_core::{RequestStatus, Session};
use eframe::egui;
use std::sync::Arc;

const DOCUMENTS: usize = 16;
const SOURCE_BYTES: usize = 4 * 1024 * 1024;

struct Source {
    key: DocumentKey,
    text: Arc<str>,
    streaming: bool,
}

pub enum Preview<'a> {
    Formatted(&'a Document, u64),
    Pending,
    Unformatted(String),
    NotCached,
}

/// One parser and a bounded current-session cache, scheduled only from application logic.
pub struct Previews {
    worker: Option<ParseWorker>,
    unavailable: Option<String>,
    sources: Vec<Source>,
    ready: Vec<(DocumentKey, Result<Arc<Document>, ParseError>)>,
    in_flight: Option<DocumentKey>,
    revision: u64,
    suppressed_stream: Option<(String, String)>,
}

impl Previews {
    pub fn new(ctx: egui::Context, assistant_viewport: egui::ViewportId) -> Self {
        let worker = ParseWorker::new(Limits::default(), move || {
            ctx.request_repaint_of(egui::ViewportId::ROOT);
            ctx.request_repaint_of(assistant_viewport);
        });
        let unavailable = worker.as_ref().err().map(ToString::to_string);
        Self {
            worker: worker.ok(),
            unavailable,
            sources: Vec::new(),
            ready: Vec::new(),
            in_flight: None,
            revision: 0,
            suppressed_stream: None,
        }
    }

    pub fn allows_stream(&self, status: &GenerationStatus) -> bool {
        !self
            .suppressed_stream
            .as_ref()
            .is_some_and(|(session, request)| {
                session == &status.session_id && request == &status.request_id
            })
    }

    /// An accepted cancellation/context command retires its stream before the worker acknowledgement.
    pub fn cancel_stream(&mut self, status: Option<&GenerationStatus>) {
        if let Some(status) = status.filter(|status| status.state == "generating") {
            self.suppressed_stream = Some((status.session_id.clone(), status.request_id.clone()));
        }
        let retired: Vec<_> = self
            .sources
            .iter()
            .filter(|source| source.streaming)
            .map(|source| source.key.clone())
            .collect();
        self.sources.retain(|source| !source.streaming);
        self.ready.retain(|(key, _)| !retired.contains(key));
        if self
            .in_flight
            .as_ref()
            .is_some_and(|key| retired.contains(key))
        {
            if let Some(worker) = &self.worker {
                worker.cancel();
            }
            self.in_flight = None;
        }
    }

    pub fn synchronize(
        &mut self,
        session: Option<&Session>,
        generation: Option<&GenerationStatus>,
        visible: std::ops::Range<usize>,
    ) {
        let mut sources = Vec::new();
        let mut bytes = 0usize;
        if let Some(session) = session {
            let session_id: Arc<str> = session.id.as_str().into();
            let end = visible.end.min(session.requests.len());
            let start = visible.start.min(end).max(end.saturating_sub(DOCUMENTS));
            for request in session.requests[start..end].iter().rev() {
                if sources.len() == DOCUMENTS {
                    break;
                }
                let completed = session
                    .suggestions
                    .iter()
                    .find(|answer| {
                        request.status == RequestStatus::Completed
                            && answer.request_id == request.id
                            && answer.context_revision == request.context.revision
                    })
                    .map(|answer| answer.text.as_str());
                let stream = generation.filter(|status| {
                    request.status == RequestStatus::Pending
                        && session.active_request_id.as_deref() == Some(request.id.as_str())
                        && status.session_id == session.id
                        && status.request_id == request.id
                        && status.state == "generating"
                        && self.allows_stream(status)
                });
                let Some(text) = completed
                    .or_else(|| stream.map(|status| status.text.as_str()))
                    .filter(|text| !text.is_empty())
                else {
                    continue;
                };
                if text.len() > SOURCE_BYTES.saturating_sub(bytes) {
                    continue;
                }
                bytes += text.len();
                let existing = self.sources.iter().find(|source| {
                    source.key.session_id.as_ref() == session.id
                        && source.key.request_id.as_deref() == Some(request.id.as_str())
                        && source.key.context_revision == request.context.revision
                        && source.text.as_ref() == text
                });
                let (key, text) = if let Some(existing) = existing {
                    (existing.key.clone(), existing.text.clone())
                } else {
                    let Some(revision) = self.revision.checked_add(1) else {
                        self.unavailable = Some("Markdown revision limit reached; restart Cueglint to resume formatting.".into());
                        break;
                    };
                    self.revision = revision;
                    (
                        DocumentKey {
                            session_id: session_id.clone(),
                            request_id: Some(request.id.as_str().into()),
                            context_revision: request.context.revision,
                            text_revision: revision,
                        },
                        Arc::from(text),
                    )
                };
                sources.push(Source {
                    key,
                    text,
                    streaming: completed.is_none(),
                });
            }
        }
        self.ready
            .retain(|(key, _)| sources.iter().any(|source| &source.key == key));
        let next = sources
            .iter()
            .find(|source| !self.ready.iter().any(|(key, _)| key == &source.key))
            .map(|source| &source.key);
        if self.in_flight.as_ref().is_some_and(|key| Some(key) != next) {
            if let Some(worker) = &self.worker {
                worker.cancel();
            }
            self.in_flight = None;
        }
        self.sources = sources;
        self.poll();
    }

    pub fn poll(&mut self) {
        let Some(worker) = &self.worker else {
            return;
        };
        if let Some(completion) = worker.poll() {
            if self.in_flight.as_ref() == Some(&completion.key) {
                self.in_flight = None;
                if self
                    .sources
                    .iter()
                    .any(|source| source.key == completion.key)
                {
                    self.ready.push((completion.key, completion.result));
                }
            }
        }
        if self.in_flight.is_some() {
            return;
        }
        for source in &self.sources {
            if self.ready.iter().any(|(key, _)| key == &source.key) {
                continue;
            }
            match worker.submit(source.key.clone(), source.text.clone()) {
                Ok(()) => {
                    self.in_flight = Some(source.key.clone());
                    break;
                }
                Err(error) => self.ready.push((source.key.clone(), Err(error))),
            }
        }
    }

    pub fn get(&self, session: &str, request: &str, context_revision: u64) -> Preview<'_> {
        if let Some(error) = &self.unavailable {
            return Preview::Unformatted(error.clone());
        }
        let Some(source) = self.sources.iter().find(|source| {
            source.key.session_id.as_ref() == session
                && source.key.request_id.as_deref() == Some(request)
                && source.key.context_revision == context_revision
        }) else {
            return Preview::NotCached;
        };
        match self
            .ready
            .iter()
            .find(|(key, _)| key == &source.key)
            .map(|(_, result)| result)
        {
            Some(Ok(document)) => Preview::Formatted(document, source.key.text_revision),
            Some(Err(error)) => Preview::Unformatted(error.to_string()),
            None => Preview::Pending,
        }
    }

    pub fn shutdown(&mut self) {
        if let Some(worker) = &mut self.worker {
            worker.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cueglint_core::Store;
    use std::{
        thread,
        time::{Duration, Instant},
    };

    fn previews() -> Previews {
        Previews::new(egui::Context::default(), egui::ViewportId::ROOT)
    }

    fn completed(count: usize) -> Session {
        let mut store = Store::open_in_memory().unwrap();
        let session = store.start_session("Original Markdown test", None).unwrap();
        for index in 0..count {
            let request = store
                .begin_request(&session.id, &format!("Original question {index}"))
                .unwrap();
            store
                .complete_request(
                    &session.id,
                    &request.id,
                    &format!("**Original answer {index}**"),
                )
                .unwrap();
        }
        store.session(&session.id).unwrap()
    }

    fn wait(
        previews: &mut Previews,
        session: &Session,
        generation: Option<&GenerationStatus>,
        request: &str,
    ) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let context = session
            .requests
            .iter()
            .find(|item| item.id == request)
            .unwrap()
            .context
            .revision;
        loop {
            previews.synchronize(Some(session), generation, 0..usize::MAX);
            if matches!(
                previews.get(&session.id, request, context),
                Preview::Formatted(..)
            ) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "bounded Markdown worker did not produce the current document"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn completed_answers_share_one_worker_and_cache_without_reparsing_notes_changes() {
        let mut previews = previews();
        let mut session = completed(3);
        previews.synchronize(Some(&session), None, 0..usize::MAX);
        let deadline = Instant::now() + Duration::from_secs(5);
        while session.requests.iter().any(|request| {
            !matches!(
                previews.get(&session.id, &request.id, request.context.revision),
                Preview::Formatted(..)
            )
        }) {
            previews.poll();
            assert!(
                Instant::now() < deadline,
                "worker repaint/poll must finish all answers without a new store snapshot"
            );
            thread::sleep(Duration::from_millis(2));
        }
        let request = session.requests.last().unwrap();
        let original = match previews.get(&session.id, &request.id, request.context.revision) {
            Preview::Formatted(doc, _) => doc as *const Document,
            _ => panic!("missing completed answer"),
        };
        session.notes = "An unsaved user's note is separate from answer formatting.".into();
        previews.synchronize(Some(&session), None, 0..usize::MAX);
        let current = match previews.get(&session.id, &request.id, request.context.revision) {
            Preview::Formatted(doc, _) => doc as *const Document,
            _ => panic!("notes must not invalidate a frozen answer"),
        };
        assert_eq!(original, current);
        assert!(previews.in_flight.is_none());
    }

    #[test]
    fn rewritten_stream_text_never_uses_the_previous_same_length_document_and_cancel_is_immediate()
    {
        let mut store = Store::open_in_memory().unwrap();
        let session = store.start_session("Original stream", None).unwrap();
        let request = store
            .begin_request(&session.id, "Original question")
            .unwrap();
        let session = store.session(&session.id).unwrap();
        let mut status = GenerationStatus {
            session_id: session.id.clone(),
            request_id: request.id.clone(),
            state: "generating".into(),
            text: "**old**".into(),
        };
        let mut previews = previews();
        wait(&mut previews, &session, Some(&status), &request.id);
        let previous_revision = previews.sources[0].key.text_revision;
        status.text = "**new**".into();
        previews.synchronize(Some(&session), Some(&status), 0..usize::MAX);
        assert!(previews.sources[0].key.text_revision > previous_revision);
        assert!(matches!(
            previews.get(&session.id, &request.id, request.context.revision),
            Preview::Pending
        ));
        wait(&mut previews, &session, Some(&status), &request.id);
        let Preview::Formatted(current, _) =
            previews.get(&session.id, &request.id, request.context.revision)
        else {
            panic!("missing rewritten stream")
        };
        assert_eq!(current.source().as_ref(), "**new**");
        previews.cancel_stream(Some(&status));
        for _ in 0..3 {
            previews.synchronize(Some(&session), Some(&status), 0..usize::MAX);
        }
        assert!(!previews.allows_stream(&status));
        assert!(matches!(
            previews.get(&session.id, &request.id, request.context.revision),
            Preview::NotCached
        ));
    }

    #[test]
    fn context_cancellation_and_session_switch_cannot_republish_old_content() {
        let mut store = Store::open_in_memory().unwrap();
        let session = store.start_session("Original context", None).unwrap();
        let request = store
            .begin_request(&session.id, "Original question")
            .unwrap();
        let active = store.session(&session.id).unwrap();
        let status = GenerationStatus {
            session_id: session.id.clone(),
            request_id: request.id.clone(),
            state: "generating".into(),
            text: "*old context*".repeat(1000),
        };
        let mut previews = previews();
        previews.synchronize(Some(&active), Some(&status), 0..usize::MAX);
        store.set_session_mode(&session.id, None).unwrap();
        let cancelled = store.session(&session.id).unwrap();
        previews.synchronize(Some(&cancelled), Some(&status), 0..usize::MAX);
        assert!(matches!(
            previews.get(&active.id, &request.id, request.context.revision),
            Preview::NotCached
        ));
        let other = completed(1);
        wait(&mut previews, &other, Some(&status), &other.requests[0].id);
        assert!(matches!(
            previews.get(&active.id, &request.id, request.context.revision),
            Preview::NotCached
        ));
        let Preview::Formatted(doc, _) = previews.get(
            &other.id,
            &other.requests[0].id,
            other.requests[0].context.revision,
        ) else {
            panic!("missing new session answer")
        };
        assert_eq!(doc.source().as_ref(), "**Original answer 0**");
    }

    #[test]
    fn cache_has_document_and_source_byte_bounds_with_explicit_uncached_results() {
        let mut session = completed(DOCUMENTS + 1);
        let mut previews = previews();
        previews.synchronize(Some(&session), None, 0..usize::MAX);
        assert_eq!(previews.sources.len(), DOCUMENTS);
        let oldest = &session.requests[0];
        assert!(matches!(
            previews.get(&session.id, &oldest.id, oldest.context.revision),
            Preview::NotCached
        ));
        for answer in session.suggestions.iter_mut().rev().take(2) {
            answer.text = "x".repeat(SOURCE_BYTES / 2);
        }
        previews.synchronize(Some(&session), None, 0..usize::MAX);
        assert_eq!(previews.sources.len(), 2);
        assert_eq!(
            previews
                .sources
                .iter()
                .map(|source| source.text.len())
                .sum::<usize>(),
            SOURCE_BYTES
        );
        previews.synchronize(None, None, 0..0);
        assert!(
            previews.ready.is_empty()
                && previews.sources.is_empty()
                && previews.in_flight.is_none()
        );
    }
}
