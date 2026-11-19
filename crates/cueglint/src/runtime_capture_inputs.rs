use super::*;
use cueglint_local::audio::{self, InputDevice};

#[derive(Clone)]
pub struct CaptureInputsStatus {
    pub request_id: Option<String>,
    pub refreshing: bool,
    pub microphones: Vec<InputDevice>,
    pub displays: Vec<screen::Display>,
    pub microphone_error: Option<String>,
    pub display_error: Option<String>,
}

impl Default for CaptureInputsStatus {
    fn default() -> Self {
        Self {
            request_id: None,
            refreshing: false,
            microphones: Vec::new(),
            displays: Vec::new(),
            microphone_error: Some("Microphone inputs have not been refreshed.".into()),
            display_error: Some("Capture displays have not been refreshed.".into()),
        }
    }
}

pub struct CaptureInputsReport {
    microphones: Result<Vec<InputDevice>, String>,
    displays: Result<Vec<screen::Display>, String>,
}

pub(super) struct InputRefresh {
    pub(super) id: String,
    pub(super) cancel: Arc<AtomicBool>,
}

impl Runtime {
    pub fn refresh_capture_inputs(&mut self) -> Result<()> {
        ensure!(
            !self.fixture,
            "input enumeration is disabled in isolated fixtures"
        );
        ensure!(
            self.input_refresh.is_none(),
            "input refresh is still running; wait for it to finish"
        );
        let id = uuid();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancelled = cancel.clone();
        let request = id.clone();
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        thread::Builder::new()
            .name("cueglint-inputs".into())
            .spawn(move || {
                let microphones = if cancelled.load(Ordering::Relaxed) {
                    Err("Input refresh cancelled.".into())
                } else {
                    audio::input_devices().map_err(|error| format!("{error:#}"))
                };
                let displays = if cancelled.load(Ordering::Relaxed) {
                    Err("Input refresh cancelled.".into())
                } else {
                    screen::authorized_displays().map_err(|error| format!("{error:#}"))
                };
                let _ = tx.send(Event::CaptureInputs(
                    request,
                    CaptureInputsReport {
                        microphones,
                        displays,
                    },
                ));
                ctx.request_repaint();
            })?;
        self.input_refresh = Some(InputRefresh {
            id: id.clone(),
            cancel,
        });
        self.snapshot.capture_inputs.request_id = Some(id);
        self.snapshot.capture_inputs.refreshing = true;
        Ok(())
    }

    pub fn cancel_capture_inputs(&mut self, id: &str) {
        if let Some(refresh) = self
            .input_refresh
            .as_ref()
            .filter(|refresh| refresh.id == id)
        {
            refresh.cancel.store(true, Ordering::Relaxed);
            // Keep the slot until the native call returns; cancellation cannot stop an OS API.
            self.snapshot
                .capture_inputs
                .microphone_error
                .get_or_insert_with(|| "Refresh cancelled; listed inputs may be stale.".into());
            self.snapshot
                .capture_inputs
                .display_error
                .get_or_insert_with(|| "Refresh cancelled; listed displays may be stale.".into());
        }
    }

    pub(super) fn apply_capture_inputs(&mut self, id: String, report: CaptureInputsReport) {
        let Some(refresh) = self
            .input_refresh
            .as_ref()
            .filter(|refresh| refresh.id == id)
        else {
            return;
        };
        let cancelled = refresh.cancel.load(Ordering::Relaxed);
        self.input_refresh = None;
        self.snapshot.capture_inputs.refreshing = false;
        if cancelled {
            return;
        }
        match report.microphones {
            Ok(inputs) => {
                self.snapshot.capture_inputs.microphones = inputs;
                self.snapshot.capture_inputs.microphone_error = None;
            }
            Err(error) => self.snapshot.capture_inputs.microphone_error = Some(error),
        }
        match report.displays {
            Ok(displays) => {
                self.snapshot.capture_inputs.displays = displays;
                self.snapshot.capture_inputs.display_error = None;
            }
            Err(error) => self.snapshot.capture_inputs.display_error = Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelled_and_stale_refreshes_cannot_replace_catalogs_or_release_another_job() {
        let store = Store::open_in_memory().unwrap();
        let mut runtime = Runtime::new(&store, true, egui::Context::default()).unwrap();
        runtime.snapshot.capture_inputs.microphones = vec![InputDevice {
            id: "coreaudio:kept".into(),
            name: "Kept input".into(),
        }];
        runtime.snapshot.capture_inputs.microphone_error = None;
        runtime.input_refresh = Some(InputRefresh {
            id: "owned".into(),
            cancel: Arc::new(AtomicBool::new(false)),
        });
        runtime.snapshot.capture_inputs.refreshing = true;
        let report = || CaptureInputsReport {
            microphones: Ok(Vec::new()),
            displays: Ok(Vec::new()),
        };
        runtime.fixture = false;
        assert!(
            runtime
                .refresh_capture_inputs()
                .unwrap_err()
                .to_string()
                .contains("still running")
        );
        runtime.apply_capture_inputs("stale".into(), report());
        assert!(runtime.snapshot.capture_inputs.refreshing);
        assert_eq!(runtime.input_refresh.as_ref().unwrap().id, "owned");
        runtime.cancel_capture_inputs("stale");
        assert!(
            !runtime
                .input_refresh
                .as_ref()
                .unwrap()
                .cancel
                .load(Ordering::Relaxed)
        );
        runtime.cancel_capture_inputs("owned");
        assert!(runtime.snapshot.capture_inputs.refreshing);
        runtime.apply_capture_inputs("owned".into(), report());
        assert!(runtime.input_refresh.is_none());
        assert!(!runtime.snapshot.capture_inputs.refreshing);
        assert_eq!(
            runtime.snapshot.capture_inputs.microphones[0].id,
            "coreaudio:kept"
        );
        assert!(
            runtime
                .snapshot
                .capture_inputs
                .microphone_error
                .as_ref()
                .unwrap()
                .contains("cancelled")
        );
    }

    #[test]
    fn changing_capture_display_retires_old_screen_preview_and_request_context() {
        let mut store = Store::open_in_memory().unwrap();
        let session = store
            .start_session("Original screen selection", None)
            .unwrap()
            .id;
        store
            .update_notes(&session, "Keep original notes.")
            .unwrap();
        let request = store
            .begin_request(&session, "Question for previous display")
            .unwrap();
        let mut runtime = Runtime::new(&store, true, egui::Context::default()).unwrap();
        runtime.screen_preview_id = Some("previous-preview".into());
        runtime.snapshot.generation = Some(GenerationStatus {
            session_id: session.clone(),
            request_id: request.id,
            state: "generating".into(),
            text: String::new(),
        });
        let mut settings = store.settings().unwrap();
        settings.display_id = Some(42);
        settings.system_audio_enabled = true;
        store.update_settings(settings.clone()).unwrap();
        runtime.settings_changed(&mut store).unwrap();
        assert_eq!(
            runtime.snapshot.capture_config,
            CaptureConfig::from(&settings)
        );
        assert!(runtime.screen_preview_id.is_none());
        assert!(store.session(&session).unwrap().active_request_id.is_none());
        runtime
            .apply(
                &mut store,
                Event::ScreenPreview(
                    "previous-preview".into(),
                    Ok(ScreenContext {
                        id: "old-screen".into(),
                        source: ScreenSource::Display(7),
                        captured_at: 0,
                        width: 1,
                        height: 1,
                        sha256: "a".repeat(64),
                        pixel_sha256: None,
                        text: "Old display text".into(),
                    }),
                ),
            )
            .unwrap();
        assert!(runtime.snapshot.screen.is_none());
        assert_eq!(store.settings().unwrap().display_id, Some(42));
        assert_eq!(
            store.session(&session).unwrap().notes,
            "Keep original notes."
        );
    }

    #[test]
    fn refresh_errors_keep_the_last_catalog_and_saved_missing_selection() {
        let mut store = Store::open_in_memory().unwrap();
        let mut settings = store.settings().unwrap();
        settings.microphone_device_id = Some("coreaudio:temporarily-missing".into());
        settings.display_id = Some(42);
        store.update_settings(settings.clone()).unwrap();
        let mut runtime = Runtime::new(&store, true, egui::Context::default()).unwrap();
        runtime.snapshot.capture_inputs.microphones = vec![InputDevice {
            id: "coreaudio:previous".into(),
            name: "Previous catalog".into(),
        }];
        runtime.snapshot.capture_inputs.displays = vec![screen::Display {
            id: 7,
            width: 1280,
            height: 720,
        }];
        runtime.input_refresh = Some(InputRefresh {
            id: "owned".into(),
            cancel: Arc::new(AtomicBool::new(false)),
        });
        runtime.apply_capture_inputs(
            "owned".into(),
            CaptureInputsReport {
                microphones: Err("input service unavailable".into()),
                displays: Err("display permission required".into()),
            },
        );
        assert_eq!(
            runtime.snapshot.capture_inputs.microphones[0].id,
            "coreaudio:previous"
        );
        assert_eq!(runtime.snapshot.capture_inputs.displays[0].id, 7);
        assert_eq!(
            runtime.snapshot.capture_inputs.microphone_error.as_deref(),
            Some("input service unavailable")
        );
        assert_eq!(
            runtime.snapshot.capture_inputs.display_error.as_deref(),
            Some("display permission required")
        );
        assert_eq!(store.settings().unwrap(), settings);
        assert!(
            runtime.refresh_capture_inputs().is_err(),
            "fixture runtimes never enumerate hardware"
        );
    }
}
