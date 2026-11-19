use super::*;
use cueglint_local::screen::scoped::{
    self, CaptureFailure, CleanupUnconfirmed, PickerEvent, WindowCapture,
};
use std::{ffi::OsStr, num::NonZeroU32, time::Instant};
#[path = "runtime_scoped_timing.rs"]
mod timing;
use timing::{WindowTimingReport, WindowWorkerTiming};

pub(crate) type WindowWorkerResult =
    Result<(WindowCapture, Option<WindowWorkerTiming>), CaptureFailure>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerPhase {
    Choosing,
    Capturing,
    Retiring,
}

#[derive(Clone)]
pub struct WindowPickerStatus {
    pub id: String,
    pub phase: PickerPhase,
    pub question: bool,
    pub deadline: Instant,
    pub(crate) cancel: Arc<AtomicBool>,
}
impl WindowPickerStatus {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }
}

pub(super) struct WindowQuestion {
    pub session: Option<String>,
    pub prompt: String,
    pub mode: Option<String>,
}

pub(super) struct WindowJob {
    question: Option<WindowQuestion>,
    cancel: Arc<AtomicBool>,
    permit: Option<ScreenPermit>,
    worker: Option<thread::JoinHandle<()>>,
}

pub(super) struct UnconfirmedCapture {
    _native: CleanupUnconfirmed,
    _permit: ScreenPermit,
}

impl Runtime {
    pub fn window_question(
        &mut self,
        store: &mut Store,
        session: Option<String>,
        prompt: String,
        mode: Option<String>,
    ) -> Result<()> {
        self.ensure_answer_ready(&prompt)?;
        if let Some(id) = &session {
            store.session(id)?;
        }
        self.start_window_picker(
            Some(WindowQuestion {
                session,
                prompt,
                mode,
            }),
            store,
        )
    }

    pub(super) fn start_window_picker(
        &mut self,
        question: Option<WindowQuestion>,
        store: &mut Store,
    ) -> Result<()> {
        ensure!(
            !self.fixture,
            "screen acquisition is disabled in deterministic fixtures"
        );
        ensure!(
            scoped::is_supported(),
            "Single-window capture requires macOS 15.2 or later"
        );
        ensure!(
            !self.snapshot.window_cleanup_unconfirmed,
            "window stream stop could not be confirmed; restart Cueglint before capturing again"
        );
        let permit = ScreenPermit::acquire(&self.screen_busy)?;
        if question.is_some() {
            self.cancel(store)?;
        }
        self.snapshot.screen = None;
        let cancel = Arc::new(AtomicBool::new(false));
        self.snapshot.window_picker = Some(WindowPickerStatus {
            id: uuid(),
            phase: PickerPhase::Choosing,
            question: question.is_some(),
            deadline: Instant::now() + Duration::from_secs(120),
            cancel: cancel.clone(),
        });
        self.window_job = Some(WindowJob {
            question,
            cancel,
            permit: Some(permit),
            worker: None,
        });
        Ok(())
    }

    pub fn cancel_window_picker(&mut self) {
        if let Some(status) = &mut self.snapshot.window_picker {
            status.cancel();
            if status.phase == PickerPhase::Choosing {
                status.phase = PickerPhase::Retiring;
            }
        }
    }

    pub fn window_picker_event(&mut self, event: PickerEvent) -> Result<Option<String>> {
        let Some(status) = &mut self.snapshot.window_picker else {
            return Ok(None);
        };
        if event.job_id() != status.id {
            return Ok(None);
        }
        if status.phase == PickerPhase::Choosing && Instant::now() >= status.deadline {
            status.phase = PickerPhase::Retiring;
            anyhow::bail!("Window selection timed out.");
        }
        if status.phase == PickerPhase::Choosing && status.cancelled() {
            status.phase = PickerPhase::Retiring;
            return Ok(Some("Window selection cancelled.".into()));
        }
        match event {
            PickerEvent::Selected(selection) if status.phase == PickerPhase::Choosing => {
                let job = self
                    .window_job
                    .as_mut()
                    .context("window operation missing")?;
                let tx = self.tx.clone();
                let ctx = self.ctx.clone();
                let cancel = job.cancel.clone();
                let id = status.id.clone();
                let measure = status.question
                    && std::env::var_os("CUEGLINT_SCOPED_WINDOW_TIMING").as_deref()
                        == Some(OsStr::new("1"));
                match thread::Builder::new()
                    .name("cueglint-window-context".into())
                    .spawn(move || {
                        let result = scoped::capture_with_timing(selection, &cancel, measure).map(
                            |(frame, timing)| {
                                (
                                    frame,
                                    timing.map(|native| WindowWorkerTiming {
                                        native,
                                        event_ready: Instant::now(),
                                    }),
                                )
                            },
                        );
                        let _ = tx.send(Event::WindowCaptured(id, result));
                        ctx.request_repaint();
                    }) {
                    Ok(worker) => {
                        job.worker = Some(worker);
                        status.phase = PickerPhase::Capturing;
                    }
                    Err(error) => {
                        status.phase = PickerPhase::Retiring;
                        return Err(error).context("starting selected-window capture");
                    }
                }
            }
            PickerEvent::Cancelled { .. } if status.phase == PickerPhase::Choosing => {
                status.phase = PickerPhase::Retiring;
                return Ok(Some("Window selection cancelled.".into()));
            }
            PickerEvent::Failed { message, .. } if status.phase == PickerPhase::Choosing => {
                status.phase = PickerPhase::Retiring;
                return Err(anyhow::Error::msg(message));
            }
            PickerEvent::Retired { .. } if status.phase == PickerPhase::Retiring => {
                self.window_job = None;
                self.snapshot.window_picker = None;
            }
            _ => {}
        }
        Ok(None)
    }

    pub fn window_capture_finished(
        &mut self,
        store: &mut Store,
        selected: &mut Option<String>,
        id: String,
        result: WindowWorkerResult,
    ) -> Result<(bool, Option<WindowTimingReport>)> {
        let received = result
            .as_ref()
            .is_ok_and(|(_, timing)| timing.is_some())
            .then(Instant::now);
        let status = self
            .snapshot
            .window_picker
            .as_mut()
            .context("window capture missing")?;
        ensure!(
            status.id == id && status.phase == PickerPhase::Capturing,
            "window capture identity changed"
        );
        status.phase = PickerPhase::Retiring;
        let job = self
            .window_job
            .as_mut()
            .context("window operation missing")?;
        if let Some(worker) = job.worker.take() {
            worker
                .join()
                .map_err(|_| anyhow::anyhow!("window capture worker panicked"))?;
        }
        let (frame, timing) = match result {
            Ok(result) => result,
            Err(mut failure) => {
                if let Some(native) = failure.cleanup.take() {
                    self.snapshot.window_cleanup_unconfirmed = true;
                    self.unconfirmed_capture = Some(UnconfirmedCapture {
                        _native: native,
                        _permit: job.permit.take().context("screen capture slot missing")?,
                    });
                } else if job.cancel.load(Ordering::Acquire) {
                    return Ok((false, None));
                }
                return Err(failure).context("capturing the selected window");
            }
        };
        if job.cancel.load(Ordering::Acquire) {
            return Ok((false, None));
        }
        ensure!(
            frame.id == id,
            "window capture returned a different identity"
        );
        let screen = ScreenContext {
            id: frame.id,
            source: ScreenSource::Window(
                NonZeroU32::new(frame.window.window_id).context("invalid window ID")?,
            ),
            captured_at: frame.captured_at,
            width: frame.width,
            height: frame.height,
            sha256: frame.sha256,
            pixel_sha256: Some(frame.pixel_sha256),
            text: frame.text,
        };
        let summary = screen_status(&screen);
        if let Some(question) = job.question.take() {
            let cancel = job.cancel.clone();
            self.ensure_answer_ready(&question.prompt)?;
            let timing = timing
                .zip(received)
                .map(|(capture, worker_received)| (capture, worker_received, Instant::now()));
            let request = match question.session {
                Some(id) => store.begin_request_with_screen(&id, &question.prompt, Some(screen))?,
                None => store.start_chat_with_screen(
                    question.mode.as_deref(),
                    &question.prompt,
                    screen,
                )?,
            };
            let timing = timing.map(|(capture, worker_received, freeze_started)| {
                (
                    capture,
                    worker_received,
                    freeze_started,
                    Instant::now(),
                    request.id.clone(),
                    cancel.clone(),
                )
            });
            if cancel.load(Ordering::Acquire) {
                store.cancel_request(&request.session_id, &request.id)?;
                return Ok((true, None));
            }
            self.snapshot.screen = Some(summary);
            *selected = Some(request.session_id.clone());
            self.generation_cancel = Some(cancel.clone());
            self.generate(store, request, cancel)?;
            let timing = timing.map(
                |(
                    capture,
                    worker_received,
                    freeze_started,
                    request_persisted,
                    request_id,
                    cancel,
                )| WindowTimingReport {
                    capture_id: id,
                    request_id,
                    cancel,
                    capture,
                    worker_received,
                    freeze_started,
                    request_persisted,
                    inference_queued: Instant::now(),
                },
            );
            return Ok((true, timing));
        }
        self.snapshot.screen = Some(summary);
        Ok((false, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cueglint_local::screen::scoped::WindowIdentity;

    #[test]
    fn late_window_picker_outcome_expires_the_ticket_and_leaves_cleanup_owned() -> Result<()> {
        let store = Store::open_in_memory()?;
        let mut runtime = Runtime::new(&store, true, egui::Context::default())?;
        let id = uuid();
        let cancel = Arc::new(AtomicBool::new(false));
        runtime.snapshot.window_picker = Some(WindowPickerStatus {
            id: id.clone(),
            phase: PickerPhase::Choosing,
            question: false,
            deadline: Instant::now(),
            cancel: cancel.clone(),
        });
        runtime.window_job = Some(WindowJob {
            question: None,
            cancel,
            permit: Some(ScreenPermit::acquire(&runtime.screen_busy)?),
            worker: None,
        });
        let error = runtime
            .window_picker_event(PickerEvent::Cancelled { job_id: id.clone() })
            .unwrap_err();
        assert_eq!(error.to_string(), "Window selection timed out.");
        assert_eq!(
            runtime.snapshot.window_picker.as_ref().unwrap().phase,
            PickerPhase::Retiring
        );
        assert!(ScreenPermit::acquire(&runtime.screen_busy).is_err());
        runtime.window_picker_event(PickerEvent::Retired { job_id: id })?;
        assert!(ScreenPermit::acquire(&runtime.screen_busy).is_ok());
        Ok(())
    }

    #[test]
    fn cancelled_scope_cannot_publish_pixels_or_release_another_pickers_slot() -> Result<()> {
        let mut store = Store::open_in_memory()?;
        let mut runtime = Runtime::new(&store, true, egui::Context::default())?;
        let id = uuid();
        let cancel = Arc::new(AtomicBool::new(false));
        runtime.snapshot.window_picker = Some(WindowPickerStatus {
            id: id.clone(),
            phase: PickerPhase::Capturing,
            question: true,
            deadline: Instant::now() + Duration::from_secs(1),
            cancel: cancel.clone(),
        });
        runtime.window_job = Some(WindowJob {
            question: Some(WindowQuestion {
                session: None,
                prompt: "Read the selected window.".into(),
                mode: None,
            }),
            cancel,
            permit: Some(ScreenPermit::acquire(&runtime.screen_busy)?),
            worker: None,
        });

        let mut selected = None;
        // The app has requested cancellation while the worker still holds the queued result.
        runtime.snapshot.window_picker.as_ref().unwrap().cancel();
        let instant = Instant::now();
        let timing = WindowWorkerTiming {
            native: scoped::WindowCaptureTiming {
                frame_observed: instant,
                stream_stopped: instant,
                rgba_ready: instant,
                ocr_started: instant,
                ocr_finished: instant,
            },
            event_ready: instant,
        };
        let (changed, timing) = runtime.window_capture_finished(
            &mut store,
            &mut selected,
            id.clone(),
            Ok((
                WindowCapture {
                    id: id.clone(),
                    window: WindowIdentity {
                        window_id: 31,
                        owner_pid: 7,
                    },
                    captured_at: 100,
                    width: 538,
                    height: 288,
                    sha256: "a".repeat(64),
                    pixel_sha256: "b".repeat(64),
                    png: Vec::new(),
                    text: "Original private window".into(),
                },
                Some(timing),
            )),
        )?;
        assert!(timing.is_none());
        assert!(!changed && selected.is_none());
        assert!(store.sessions("")?.is_empty());
        assert!(runtime.snapshot.screen.is_none());
        assert_eq!(
            runtime.snapshot.window_picker.as_ref().unwrap().phase,
            PickerPhase::Retiring
        );

        runtime.window_picker_event(PickerEvent::Retired { job_id: uuid() })?;
        assert!(ScreenPermit::acquire(&runtime.screen_busy).is_err());
        runtime.window_picker_event(PickerEvent::Retired { job_id: id })?;
        assert!(runtime.snapshot.window_picker.is_none());
        assert!(ScreenPermit::acquire(&runtime.screen_busy).is_ok());
        Ok(())
    }
}
