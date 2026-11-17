// Original Cueglint implementation. SPDX-License-Identifier: MIT OR Apache-2.0
use super::{Document, Limits, ParseError, parse_with_cancel};
use std::{
    io,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentKey {
    pub session_id: Arc<str>,
    pub request_id: Option<Arc<str>>,
    pub context_revision: u64,
    pub text_revision: u64,
}

pub struct Completion {
    pub key: DocumentKey,
    pub result: Result<Arc<Document>, ParseError>,
    generation: u64,
}
struct Job {
    key: DocumentKey,
    source: Arc<str>,
    generation: u64,
}
#[derive(Default)]
struct Slots {
    pending: Option<Job>,
    result: Option<Completion>,
}
struct State {
    slots: Mutex<Slots>,
    wake: Condvar,
    generation: AtomicU64,
    closed: AtomicBool,
}

impl State {
    fn new() -> Self {
        Self {
            slots: Mutex::new(Slots::default()),
            wake: Condvar::new(),
            generation: AtomicU64::new(0),
            closed: AtomicBool::new(false),
        }
    }
    fn superseded(&self, generation: u64) -> bool {
        self.closed.load(Ordering::Acquire) || self.generation.load(Ordering::Acquire) != generation
    }
    fn submit(&self, key: DocumentKey, source: Arc<str>) -> Result<(), ParseError> {
        let mut slots = self.slots.lock().unwrap();
        if self.closed.load(Ordering::Acquire) {
            return Err(ParseError::Closed);
        }
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        slots.pending = Some(Job {
            key,
            source,
            generation,
        });
        slots.result = None;
        drop(slots);
        self.wake.notify_one();
        Ok(())
    }
}

/// One worker, one replaceable pending job, one result and one cached immutable document.
/// Submit on source/revision changes, not from rendering. Keep the last good document while parsing.
pub struct ParseWorker {
    state: Arc<State>,
    thread: Option<JoinHandle<()>>,
    limits: Limits,
}
impl ParseWorker {
    pub fn new(limits: Limits, on_ready: impl Fn() + Send + 'static) -> io::Result<Self> {
        let limits = limits.bounded();
        let state = Arc::new(State::new());
        let worker = state.clone();
        let thread = thread::Builder::new()
            .name("cueglint-markdown-preview".into())
            .spawn(move || {
                let mut cache: Option<(DocumentKey, Arc<Document>)> = None;
                loop {
                    let job = {
                        let mut slots = worker.slots.lock().unwrap();
                        while slots.pending.is_none() && !worker.closed.load(Ordering::Acquire) {
                            slots = worker.wake.wait(slots).unwrap();
                        }
                        if worker.closed.load(Ordering::Acquire) {
                            return;
                        }
                        slots.pending.take().unwrap()
                    };
                    let cached = cache.as_ref().filter(|(key, doc)| {
                        key == &job.key && doc.source().as_ref() == job.source.as_ref()
                    });
                    let result = if let Some((_, doc)) = cached {
                        Ok(doc.clone())
                    } else {
                        parse_with_cancel(job.source, limits, || worker.superseded(job.generation))
                            .map(Arc::new)
                    };
                    let mut slots = worker.slots.lock().unwrap();
                    if worker.superseded(job.generation) {
                        continue;
                    }
                    if let Ok(doc) = &result {
                        cache = Some((job.key.clone(), doc.clone()));
                    }
                    slots.result = Some(Completion {
                        key: job.key,
                        result,
                        generation: job.generation,
                    });
                    drop(slots);
                    on_ready();
                }
            })?;
        Ok(Self {
            state,
            thread: Some(thread),
            limits,
        })
    }

    pub fn submit(&self, key: DocumentKey, source: Arc<str>) -> Result<(), ParseError> {
        if source.len() > self.limits.source_bytes {
            self.cancel();
            return Err(ParseError::SourceTooLarge {
                limit: self.limits.source_bytes,
            });
        }
        self.state.submit(key, source)
    }

    pub fn poll(&self) -> Option<Completion> {
        let completion = self.state.slots.lock().unwrap().result.take()?;
        (!self.state.superseded(completion.generation)).then_some(completion)
    }

    pub fn cancel(&self) {
        let mut slots = self.state.slots.lock().unwrap();
        self.state.generation.fetch_add(1, Ordering::AcqRel);
        slots.pending = None;
        slots.result = None;
    }

    /// Cooperatively cancel and join outside draw callbacks, e.g. during the app's shutdown phase.
    pub fn shutdown(&mut self) {
        {
            let mut slots = self.state.slots.lock().unwrap();
            self.state.closed.store(true, Ordering::Release);
            self.state.generation.fetch_add(1, Ordering::AcqRel);
            slots.pending = None;
            slots.result = None;
        }
        self.state.wake.notify_one();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl Drop for ParseWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    fn key(revision: u64) -> DocumentKey {
        DocumentKey {
            session_id: "original-test-session".into(),
            request_id: Some("original-test-request".into()),
            context_revision: 4,
            text_revision: revision,
        }
    }

    #[test]
    fn the_pending_slot_keeps_only_the_latest_revision_and_cancel_invalidates_it() {
        let state = State::new();
        for revision in 0..100 {
            state
                .submit(key(revision), format!("revision {revision}").into())
                .unwrap();
        }
        let slots = state.slots.lock().unwrap();
        assert_eq!(slots.pending.as_ref().unwrap().key.text_revision, 99);
        assert_eq!(
            slots.pending.as_ref().unwrap().source.as_ref(),
            "revision 99"
        );
        assert!(slots.result.is_none());
        assert!(state.superseded(1));
        assert!(!state.superseded(100));
    }

    #[test]
    fn streaming_revisions_and_session_switches_never_publish_old_content() {
        let mut worker = ParseWorker::new(Limits::default(), || {}).unwrap();
        for revision in 0..100 {
            worker
                .submit(
                    key(revision),
                    format!(
                        "# Revision {revision}\n\n{}",
                        "bounded paragraph\n\n".repeat(500)
                    )
                    .into(),
                )
                .unwrap();
        }
        let mut latest = key(100);
        latest.session_id = "new-original-session".into();
        worker
            .submit(latest.clone(), "**final original content**".into())
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let completion = loop {
            if let Some(result) = worker.poll() {
                break result;
            }
            assert!(Instant::now() < deadline, "bounded worker did not finish");
            thread::sleep(Duration::from_millis(2));
        };
        assert_eq!(completion.key, latest);
        assert!(
            completion
                .result
                .unwrap()
                .plain_text()
                .contains("final original content")
        );
        worker.cancel();
        assert!(worker.poll().is_none());
        worker.shutdown();
        assert_eq!(
            worker.submit(key(101), "closed".into()),
            Err(ParseError::Closed)
        );
    }

    #[test]
    fn identical_key_and_source_reuse_one_document_but_changed_text_does_not() {
        let worker = ParseWorker::new(Limits::default(), || {}).unwrap();
        let wait = || {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(result) = worker.poll() {
                    break result.result.unwrap();
                }
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(2));
            }
        };
        worker.submit(key(1), "cached original".into()).unwrap();
        let first = wait();
        worker.submit(key(1), "cached original".into()).unwrap();
        let second = wait();
        assert!(Arc::ptr_eq(&first, &second));
        worker.submit(key(1), "changed original".into()).unwrap();
        let changed = wait();
        assert!(!Arc::ptr_eq(&first, &changed));
    }

    #[test]
    fn rejected_revision_cannot_leave_an_old_completion_visible() {
        let worker = ParseWorker::new(
            Limits {
                source_bytes: 8,
                ..Limits::default()
            },
            || {},
        )
        .unwrap();
        worker.submit(key(1), "old text".into()).unwrap();
        assert_eq!(
            worker.submit(key(2), "oversized original text".into()),
            Err(ParseError::SourceTooLarge { limit: 8 })
        );
        assert!(worker.poll().is_none());
    }
}
