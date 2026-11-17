use anyhow::{Context, Result, ensure};
use cueglint_core::{
    AudioMetadata, GenerationRequest, ScreenContext, SessionStatus, Speaker, Store,
    TranscriptSource, TranscriptUpdate,
};
use cueglint_local::{
    asr::Transcriber,
    audio::{Capture, Endpoint, Source, Utterance},
    generation::{GenerationOptions, GenerationOutput, LocalGenerator},
    models::{self, DownloadProgress},
    screen,
};
use eframe::egui;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
    time::Duration,
};

#[derive(Clone, Default)]
pub struct RuntimeSnapshot {
    pub capture: Option<CaptureStatus>,
    pub generation: Option<GenerationStatus>,
    pub model_job: Option<ModelJob>,
    pub model_root: PathBuf,
    pub screen_enabled: bool,
    pub installed_profiles: Vec<String>,
    pub screen: Option<ScreenStatus>,
}
#[derive(Clone)]
pub struct CaptureStatus {
    pub session_id: String,
    pub state: String,
    pub dropped_packets: u64,
    pub device_errors: u64,
}
#[derive(Clone)]
pub struct GenerationStatus {
    pub session_id: String,
    pub request_id: String,
    pub state: String,
    pub text: String,
}
#[derive(Clone)]
pub struct ModelJob {
    pub profile_id: String,
    pub file: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub state: String,
}
#[derive(Clone)]
pub struct ScreenStatus {
    pub id: String,
    pub display_id: u32,
    pub captured_at: i64,
    pub width: u32,
    pub height: u32,
    pub characters: usize,
}

#[derive(Clone, Copy)]
pub enum EndAction {
    Stop,
    Pause,
    Mute,
}
struct CaptureJob {
    session: String,
    stop: Arc<AtomicBool>,
    end_action: Option<EndAction>,
}
struct GenerateJob {
    request: GenerationRequest,
    root: PathBuf,
    cancel: Arc<AtomicBool>,
}
struct ScreenPermit(Arc<AtomicBool>);
impl ScreenPermit {
    fn acquire(gate: &Arc<AtomicBool>) -> Result<Self> {
        ensure!(
            gate.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok(),
            "screen capture is still running; wait and retry"
        );
        Ok(Self(gate.clone()))
    }
}
impl Drop for ScreenPermit {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
pub enum Event {
    Models(PathBuf, Vec<String>),
    InstallProgress(String, DownloadProgress),
    InstallDone(String, PathBuf, Result<String, String>),
    Screen(String, String, String, Result<ScreenContext, String>),
    ScreenPreview(Result<ScreenContext, String>),
    GenerationText(String, String),
    GenerationDone(String, Result<GenerationOutput, String>),
    CaptureStarted(String),
    CaptureMetrics(String, u64, u64),
    Transcript(String, Utterance, String),
    CaptureError(String, String),
    CaptureEnded(String),
}

pub struct Runtime {
    pub snapshot: RuntimeSnapshot,
    events: Receiver<Event>,
    tx: SyncSender<Event>,
    generator: SyncSender<GenerateJob>,
    ctx: egui::Context,
    generation_cancel: Option<Arc<AtomicBool>>,
    install_cancel: Option<Arc<AtomicBool>>,
    install_id: Option<String>,
    capture: Option<CaptureJob>,
    fixture: bool,
    screen_busy: Arc<AtomicBool>,
    verification_busy: Arc<AtomicBool>,
}

fn root(store: &Store) -> Result<PathBuf> {
    Ok(std::env::var_os("CUEGLINT_MODEL_DIR")
        .map(PathBuf::from)
        .or_else(|| store.settings().ok()?.model_directory.map(PathBuf::from))
        .unwrap_or(
            Store::default_path()?
                .parent()
                .context("invalid data path")?
                .join("models"),
        ))
}

impl Runtime {
    pub fn new(store: &Store, fixture: bool, ctx: egui::Context) -> Result<Self> {
        let (tx, events) = mpsc::sync_channel(128);
        let (generator, jobs) = mpsc::sync_channel::<GenerateJob>(2);
        let generate_tx = tx.clone();
        let generate_ctx = ctx.clone();
        thread::Builder::new()
            .name("cueglint-inference".into())
            .spawn(move || {
                let mut model: Option<(PathBuf, LocalGenerator)> = None;
                while let Ok(job) = jobs.recv() {
                    let result = (|| -> Result<GenerationOutput> {
                        ensure!(!job.cancel.load(Ordering::Relaxed), "request cancelled");
                        if model.as_ref().is_none_or(|(loaded, _)| loaded != &job.root) {
                            model = None;
                            let loaded = LocalGenerator::load_with_cancel(&job.root, &job.cancel)?;
                            model = Some((job.root.clone(), loaded));
                        }
                        let mut text = String::new();
                        let request = job.request.id.clone();
                        model.as_mut().unwrap().1.generate(
                            &job.request,
                            GenerationOptions {
                                max_new_tokens: 256,
                            },
                            &job.cancel,
                            |delta| {
                                text.push_str(delta);
                                let _ = generate_tx
                                    .try_send(Event::GenerationText(request.clone(), text.clone()));
                                generate_ctx.request_repaint();
                            },
                        )
                    })()
                    .map_err(|e| format!("{e:#}"));
                    if generate_tx
                        .send(Event::GenerationDone(job.request.id, result))
                        .is_err()
                    {
                        break;
                    }
                    generate_ctx.request_repaint();
                }
            })?;
        let mut runtime = Self {
            snapshot: RuntimeSnapshot {
                model_root: root(store)?,
                screen_enabled: store.settings()?.screen_enabled,
                ..Default::default()
            },
            events,
            tx,
            generator,
            ctx,
            generation_cancel: None,
            install_cancel: None,
            install_id: None,
            capture: None,
            fixture,
            screen_busy: Arc::new(AtomicBool::new(false)),
            verification_busy: Arc::new(AtomicBool::new(false)),
        };
        runtime.refresh_models();
        Ok(runtime)
    }

    pub fn try_event(&self) -> Option<Event> {
        self.events.try_recv().ok()
    }

    pub fn refresh_models(&mut self) {
        let root = self.snapshot.model_root.clone();
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        if self.fixture {
            return;
        }
        let busy = self.verification_busy.clone();
        if busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        thread::spawn(move || {
            let installed = models::profiles()
                .unwrap_or_default()
                .iter()
                .filter(|p| models::verify(&root, &p.id).is_ok())
                .map(|p| p.id.clone())
                .collect();
            busy.store(false, Ordering::Release);
            let _ = tx.send(Event::Models(root, installed));
            ctx.request_repaint();
        });
    }

    pub fn settings_changed(&mut self, store: &mut Store) -> Result<()> {
        let next = root(store)?;
        if self.snapshot.model_root != next {
            ensure!(
                self.capture.is_none(),
                "stop capture before changing the model directory"
            );
            self.cancel(store)?;
            self.cancel_install();
            self.snapshot.model_root = next;
            self.snapshot.installed_profiles.clear();
            self.refresh_models();
        }
        self.snapshot.screen_enabled = store.settings()?.screen_enabled;
        Ok(())
    }

    pub fn install(&mut self, profile: String) -> Result<()> {
        ensure!(
            !self.fixture,
            "model installation is disabled in deterministic fixtures"
        );
        ensure!(
            self.install_id.is_none(),
            "a model installation is already running"
        );
        models::profile(&profile)?;
        let id = uuid();
        let cancel = Arc::new(AtomicBool::new(false));
        self.install_cancel = Some(cancel.clone());
        self.install_id = Some(id.clone());
        self.snapshot.model_job = Some(ModelJob {
            profile_id: profile.clone(),
            file: String::new(),
            downloaded_bytes: 0,
            total_bytes: 0,
            state: "downloading".into(),
        });
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        let root = self.snapshot.model_root.clone();
        thread::spawn(move || {
            let result = models::install(&root, &profile, &cancel, |progress| {
                let _ = tx.try_send(Event::InstallProgress(id.clone(), progress));
                ctx.request_repaint();
            })
            .map(|_| profile)
            .map_err(|e| format!("{e:#}"));
            let _ = tx.send(Event::InstallDone(id, root, result));
            ctx.request_repaint();
        });
        Ok(())
    }

    pub fn cancel_install(&mut self) {
        if let Some(cancel) = &self.install_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn cancel(&mut self, store: &mut Store) -> Result<()> {
        if let Some(cancel) = self.generation_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        if let Some(status) = &mut self.snapshot.generation {
            if let Ok(session) = store.session(&status.session_id)
                && session.active_request_id.as_deref() == Some(&status.request_id)
            {
                store.cancel_request(&status.session_id, &status.request_id)?;
            }
            status.state = "cancelled".into();
            status.text.clear();
        }
        Ok(())
    }

    pub fn ask(&mut self, store: &mut Store, session: &str, prompt: String) -> Result<()> {
        ensure!(
            !self.fixture,
            "real inference is disabled in deterministic fixtures"
        );
        self.cancel(store)?;
        let cancel = Arc::new(AtomicBool::new(false));
        self.generation_cancel = Some(cancel.clone());
        if self.snapshot.screen_enabled {
            let permit = ScreenPermit::acquire(&self.screen_busy)?;
            let id = uuid();
            let tx = self.tx.clone();
            let ctx = self.ctx.clone();
            let display = store.settings()?.display_id;
            let session = session.to_owned();
            self.snapshot.generation = Some(GenerationStatus {
                session_id: session.clone(),
                request_id: id.clone(),
                state: "capturing screen".into(),
                text: String::new(),
            });
            thread::spawn(move || {
                let result = fresh_screen(display).map_err(|e| format!("{e:#}"));
                drop(permit);
                if !cancel.load(Ordering::Relaxed) {
                    let _ = tx.send(Event::Screen(id, session, prompt, result));
                    ctx.request_repaint();
                }
            });
        } else {
            let request = store.begin_request(session, &prompt)?;
            self.generate(store, request, cancel)?;
        }
        Ok(())
    }

    fn generate(
        &mut self,
        store: &mut Store,
        request: GenerationRequest,
        cancel: Arc<AtomicBool>,
    ) -> Result<()> {
        self.snapshot.generation = Some(GenerationStatus {
            session_id: request.session_id.clone(),
            request_id: request.id.clone(),
            state: "generating".into(),
            text: String::new(),
        });
        let session = request.session_id.clone();
        let id = request.id.clone();
        if self
            .generator
            .try_send(GenerateJob {
                request,
                root: self.snapshot.model_root.clone(),
                cancel,
            })
            .is_err()
        {
            store.cancel_request(&session, &id)?;
            self.snapshot.generation.as_mut().unwrap().state = "error".into();
            self.generation_cancel = None;
            anyhow::bail!("local inference queue is full; cancel and retry");
        }
        Ok(())
    }

    pub fn preview_screen(&mut self, store: &Store) -> Result<()> {
        ensure!(
            !self.fixture,
            "screen acquisition is disabled in deterministic fixtures"
        );
        let permit = ScreenPermit::acquire(&self.screen_busy)?;
        let display = store.settings()?.display_id;
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        thread::spawn(move || {
            let result = fresh_screen(display).map_err(|e| format!("{e:#}"));
            drop(permit);
            let _ = tx.send(Event::ScreenPreview(result));
            ctx.request_repaint();
        });
        Ok(())
    }

    pub fn start_capture(&mut self, store: &Store, session: &str) -> Result<()> {
        if self.fixture {
            return Ok(());
        }
        if let Some(capture) = &self.capture {
            ensure!(
                capture.session == session,
                "another session owns audio capture"
            );
            return Ok(());
        }
        if store.session(session)?.muted {
            return Ok(());
        }
        let settings = store.settings()?;
        if !settings.microphone_enabled && !settings.system_audio_enabled {
            self.snapshot.capture = None;
            return Ok(());
        }
        ensure!(
            self.snapshot
                .installed_profiles
                .iter()
                .any(|p| p == models::TRANSCRIPTION_PROFILE),
            "Install and verify the Whisper English model before listening; session is retained locally"
        );
        let stop = Arc::new(AtomicBool::new(false));
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        let id = session.to_owned();
        let root = self.snapshot.model_root.clone();
        let stopped = stop.clone();
        let record = store.session(session)?;
        let timeline = (
            record.created_at,
            record
                .transcript
                .iter()
                .filter_map(|t| t.audio.as_ref().map(|a| a.end_ns))
                .max()
                .unwrap_or(0),
        );
        self.capture = Some(CaptureJob {
            session: id.clone(),
            stop,
            end_action: None,
        });
        self.snapshot.capture = Some(CaptureStatus {
            session_id: id.clone(),
            state: "initializing".into(),
            dropped_packets: 0,
            device_errors: 0,
        });
        thread::spawn(move || {
            let result = capture_session(
                &root,
                &id,
                settings,
                timeline,
                stopped,
                tx.clone(),
                ctx.clone(),
            );
            if let Err(error) = result {
                let _ = tx.send(Event::CaptureError(id.clone(), format!("{error:#}")));
            }
            let _ = tx.send(Event::CaptureEnded(id));
            ctx.request_repaint();
        });
        Ok(())
    }

    /// Stops acquisition immediately; state finalization waits for every queued final utterance.
    pub fn end_capture(&mut self, session: &str, action: EndAction) -> Result<bool> {
        if let Some(job) = &mut self.capture {
            ensure!(job.session == session, "a different session owns capture");
            if matches!(job.end_action, Some(EndAction::Stop)) {
                return Ok(true);
            }
            job.end_action = Some(action);
            job.stop.store(true, Ordering::Relaxed);
            if let Some(status) = &mut self.snapshot.capture {
                status.state = "finalizing".into();
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Returns whether durable state changed and an optional user notice.
    pub fn apply(&mut self, store: &mut Store, event: Event) -> Result<(bool, Option<String>)> {
        match event {
            Event::Models(root, installed) if root == self.snapshot.model_root => {
                self.snapshot.installed_profiles = installed
            }
            Event::Models(_, _) => self.refresh_models(),
            Event::InstallProgress(id, p) if self.install_id.as_ref() == Some(&id) => {
                self.snapshot.model_job = Some(ModelJob {
                    profile_id: p.profile_id,
                    file: p.file,
                    downloaded_bytes: p.downloaded_bytes,
                    total_bytes: p.total_bytes,
                    state: "downloading".into(),
                })
            }
            Event::InstallDone(id, root, result) if self.install_id.as_ref() == Some(&id) => {
                self.install_id = None;
                self.install_cancel = None;
                match result {
                    Ok(profile) if root == self.snapshot.model_root => {
                        if !self.snapshot.installed_profiles.contains(&profile) {
                            self.snapshot.installed_profiles.push(profile);
                        }
                        if let Some(job) = &mut self.snapshot.model_job {
                            job.state = "ready".into();
                        }
                    }
                    Ok(_) => {
                        if let Some(job) = &mut self.snapshot.model_job {
                            job.state = "cancelled".into();
                        }
                    }
                    Err(error) => {
                        if let Some(job) = &mut self.snapshot.model_job {
                            job.state = "error".into();
                        }
                        return Ok((false, Some(error)));
                    }
                }
            }
            Event::Screen(job, session, prompt, result)
                if self
                    .snapshot
                    .generation
                    .as_ref()
                    .is_some_and(|g| g.request_id == job && g.state == "capturing screen") =>
            {
                let screen = match result {
                    Ok(screen) => screen,
                    Err(error) => {
                        self.snapshot.generation.as_mut().unwrap().state = "error".into();
                        self.generation_cancel = None;
                        return Ok((false, Some(error)));
                    }
                };
                self.snapshot.screen = Some(screen_status(&screen));
                let request = match store.begin_request_with_screen(&session, &prompt, Some(screen))
                {
                    Ok(request) => request,
                    Err(error) => {
                        self.snapshot.generation.as_mut().unwrap().state = "error".into();
                        self.generation_cancel = None;
                        return Err(error);
                    }
                };
                self.generate(
                    store,
                    request,
                    self.generation_cancel
                        .clone()
                        .context("request cancelled")?,
                )?;
                return Ok((true, None));
            }
            Event::ScreenPreview(result) => {
                let screen = result.map_err(anyhow::Error::msg)?;
                self.snapshot.screen = Some(screen_status(&screen));
            }
            Event::GenerationText(id, text)
                if self
                    .snapshot
                    .generation
                    .as_ref()
                    .is_some_and(|g| g.request_id == id && g.state == "generating") =>
            {
                self.snapshot.generation.as_mut().unwrap().text = text;
            }
            Event::GenerationDone(id, result)
                if self
                    .snapshot
                    .generation
                    .as_ref()
                    .is_some_and(|g| g.request_id == id && g.state == "generating") =>
            {
                let status = self.snapshot.generation.as_mut().unwrap();
                self.generation_cancel = None;
                status.state = "error".into();
                match result {
                    Ok(output)
                        if output.finish_reason
                            != cueglint_local::generation::FinishReason::Cancelled =>
                    {
                        if !output.text.trim().is_empty()
                            && store.complete_request_with_metadata(
                                &status.session_id,
                                &id,
                                &output.text,
                                output.metadata(),
                            )?
                        {
                            status.text = output.text;
                            status.state = "completed".into();
                        } else {
                            if output.text.trim().is_empty() {
                                store.cancel_request(&status.session_id, &id)?;
                            }
                            status.text.clear();
                            status.state = "cancelled".into();
                        }
                        return Ok((true, None));
                    }
                    Ok(_) => {
                        store.cancel_request(&status.session_id, &id)?;
                        status.state = "cancelled".into();
                        status.text.clear();
                        return Ok((true, None));
                    }
                    Err(error) => {
                        store.cancel_request(&status.session_id, &id)?;
                        status.state = "error".into();
                        status.text.clear();
                        return Ok((true, Some(error)));
                    }
                }
            }
            Event::CaptureStarted(id) if self.capture.as_ref().is_some_and(|c| c.session == id) => {
                if let Some(s) = &mut self.snapshot.capture {
                    if s.state == "initializing" {
                        s.state = "listening".into();
                    }
                }
            }
            Event::CaptureMetrics(id, dropped, errors)
                if self.capture.as_ref().is_some_and(|c| c.session == id) =>
            {
                if let Some(s) = &mut self.snapshot.capture {
                    s.dropped_packets = dropped;
                    s.device_errors = errors;
                }
            }
            Event::Transcript(id, utterance, text)
                if self.capture.as_ref().is_some_and(|c| c.session == id) =>
            {
                if !text.trim().is_empty() {
                    let profile = models::profile(models::TRANSCRIPTION_PROFILE)?;
                    let artifact = profile
                        .files
                        .iter()
                        .find(|f| f.name == "model.safetensors")
                        .context("ASR manifest missing weights")?;
                    store.upsert_transcript_with_audio(
                        &id,
                        TranscriptUpdate {
                            id: utterance.id,
                            speaker: Speaker::Unknown,
                            text,
                            finalized: utterance.finalized,
                            source: if utterance.source == Source::Microphone {
                                TranscriptSource::Microphone
                            } else {
                                TranscriptSource::SystemAudio
                            },
                        },
                        AudioMetadata {
                            start_ns: utterance.start_ns,
                            end_ns: utterance.end_ns,
                            sample_rate: 16_000,
                            model_profile: profile.id.clone(),
                            model_revision: artifact.revision.clone(),
                            model_sha256: artifact.sha256.clone(),
                            language: "en".into(),
                        },
                    )?;
                    return Ok((true, None));
                }
            }
            Event::CaptureError(id, error)
                if self.capture.as_ref().is_some_and(|c| c.session == id) =>
            {
                if let Some(s) = &mut self.snapshot.capture {
                    s.state = "error".into();
                }
                return Ok((false, Some(error)));
            }
            Event::CaptureEnded(id) if self.capture.as_ref().is_some_and(|c| c.session == id) => {
                let job = self.capture.take().unwrap();
                match job.end_action {
                    Some(EndAction::Stop) => {
                        store.stop_session(&id)?;
                        store.save_session(&id)?;
                    }
                    Some(EndAction::Pause) => {
                        store.pause_session(&id)?;
                    }
                    Some(EndAction::Mute) => {
                        store.mute_session(&id, true)?;
                    }
                    None => {
                        if store.session(&id)?.status == SessionStatus::Recording {
                            store.pause_session(&id)?;
                        }
                    }
                }
                if let Some(status) = &mut self.snapshot.capture {
                    if status.state != "error" {
                        status.state = "stopped".into();
                    }
                }
                return Ok((true, None));
            }
            _ => {}
        }
        Ok((false, None))
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        if let Some(job) = &self.capture {
            job.stop.store(true, Ordering::Relaxed);
        }
        if let Some(cancel) = &self.generation_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        self.cancel_install();
    }
}

fn uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn fresh_screen(display: Option<u32>) -> Result<ScreenContext> {
    let s = screen::capture(display)?;
    Ok(ScreenContext {
        id: s.id,
        display_id: s.display_id,
        captured_at: s.captured_at,
        width: s.width,
        height: s.height,
        sha256: s.sha256,
        text: s.text,
    })
}
fn screen_status(s: &ScreenContext) -> ScreenStatus {
    ScreenStatus {
        id: s.id.clone(),
        display_id: s.display_id,
        captured_at: s.captured_at,
        width: s.width,
        height: s.height,
        characters: s.text.chars().count(),
    }
}

fn capture_session(
    root: &std::path::Path,
    session: &str,
    settings: cueglint_core::Settings,
    timeline: (i64, u64),
    stop: Arc<AtomicBool>,
    tx: SyncSender<Event>,
    ctx: egui::Context,
) -> Result<()> {
    let installed = models::verify(root, models::TRANSCRIPTION_PROFILE)?;
    let mut transcriber = Transcriber::load(&installed.directory)?;
    if stop.load(Ordering::Relaxed) {
        return Ok(());
    }
    let (utterances, pending) = mpsc::sync_channel::<Utterance>(4);
    let capture_tx = tx.clone();
    let capture_ctx = ctx.clone();
    let capture_session = session.to_owned();
    let capture_stop = stop.clone();
    let capture_thread = thread::spawn(move || -> Result<()> {
        let elapsed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis() as i64
            - timeline.0;
        let offset_ns = timeline.1.max(elapsed.max(0) as u64 * 1_000_000);
        let mut capture = Capture::start(
            settings.microphone_enabled,
            settings.system_audio_enabled,
            settings.microphone_device_id.as_deref(),
            settings.display_id,
        )?;
        let mut mic = Endpoint::new(Source::Microphone);
        let mut system = Endpoint::new(Source::SystemAudio);
        capture_tx.send(Event::CaptureStarted(capture_session.clone()))?;
        capture_ctx.request_repaint();
        let mut metrics_at = std::time::Instant::now();
        while !capture_stop.load(Ordering::Relaxed) {
            if let Ok(packet) = capture.packets.recv_timeout(Duration::from_millis(50)) {
                let endpoint = if packet.source == Source::Microphone {
                    &mut mic
                } else {
                    &mut system
                };
                for utterance in endpoint.push(&packet)? {
                    queue_utterance(&utterances, utterance, offset_ns, || {
                        if capture_stop.load(Ordering::Relaxed) {
                            capture.stop()?;
                        }
                        Ok(())
                    })?;
                }
            }
            if metrics_at.elapsed() >= Duration::from_secs(1) {
                capture_tx.send(Event::CaptureMetrics(
                    capture_session.clone(),
                    capture.dropped_packets.load(Ordering::Relaxed),
                    capture.device_errors.load(Ordering::Relaxed),
                ))?;
                capture_ctx.request_repaint();
                metrics_at = std::time::Instant::now();
                ensure!(
                    capture.device_errors.load(Ordering::Relaxed) == 0,
                    "audio device reported an error; capture stopped, reconnect the source and resume"
                );
            }
        }
        capture.stop()?;
        while let Ok(packet) = capture.packets.try_recv() {
            let e = if packet.source == Source::Microphone {
                &mut mic
            } else {
                &mut system
            };
            for u in e.push(&packet)? {
                queue_utterance(&utterances, u, offset_ns, || {
                    if capture_stop.load(Ordering::Relaxed) {
                        capture.stop()?;
                    }
                    Ok(())
                })?;
            }
        }
        for u in [mic.finish(), system.finish()].into_iter().flatten() {
            queue_utterance(&utterances, u, offset_ns, || {
                if capture_stop.load(Ordering::Relaxed) {
                    capture.stop()?;
                }
                Ok(())
            })?;
        }
        capture_tx.send(Event::CaptureMetrics(
            capture_session,
            capture.dropped_packets.load(Ordering::Relaxed),
            capture.device_errors.load(Ordering::Relaxed),
        ))?;
        Ok(())
    });
    let decode_cancel = AtomicBool::new(false);
    let mut decoding_error = None;
    for utterance in pending {
        match transcriber.transcribe(&utterance.samples, &decode_cancel) {
            Ok(text) => {
                if tx
                    .send(Event::Transcript(session.to_owned(), utterance, text))
                    .is_err()
                {
                    stop.store(true, Ordering::Relaxed);
                    break;
                }
                ctx.request_repaint();
            }
            Err(error) => {
                stop.store(true, Ordering::Relaxed);
                decoding_error = Some(error);
                break;
            }
        }
    }
    stop.store(true, Ordering::Relaxed);
    let capture_result = capture_thread
        .join()
        .map_err(|_| anyhow::anyhow!("audio capture worker panicked"))?;
    if let Some(error) = decoding_error {
        return Err(error);
    }
    capture_result
}

fn queue_utterance(
    tx: &SyncSender<Utterance>,
    mut utterance: Utterance,
    offset_ns: u64,
    mut on_backpressure: impl FnMut() -> Result<()>,
) -> Result<()> {
    utterance.start_ns += offset_ns;
    utterance.end_ns += offset_ns;
    if utterance.finalized {
        loop {
            match tx.try_send(utterance) {
                Ok(()) => break,
                Err(TrySendError::Full(pending)) => {
                    utterance = pending;
                    on_backpressure()?;
                    thread::sleep(Duration::from_millis(10));
                }
                Err(TrySendError::Disconnected(_)) => {
                    anyhow::bail!("transcription worker disconnected")
                }
            }
        }
    } else {
        match tx.try_send(utterance) {
            Ok(()) | Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(_)) => {
                anyhow::bail!("transcription worker disconnected")
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn runtime(store: &Store) -> Runtime {
        Runtime::new(store, true, egui::Context::default()).unwrap()
    }
    #[test]
    fn failed_screen_acquisition_cannot_leave_thinking_forever() {
        let mut store = Store::open_in_memory().unwrap();
        let id = store.start_session("Screen failure", None).unwrap().id;
        let mut r = runtime(&store);
        r.snapshot.generation = Some(GenerationStatus {
            session_id: id.clone(),
            request_id: "test-job".into(),
            state: "capturing screen".into(),
            text: String::new(),
        });
        let (_, notice) = r
            .apply(
                &mut store,
                Event::Screen(
                    "test-job".into(),
                    id,
                    "Question".into(),
                    Err("permission denied".into()),
                ),
            )
            .unwrap();
        assert!(notice.unwrap().contains("permission denied"));
        assert_eq!(r.snapshot.generation.as_ref().unwrap().state, "error");
    }
    #[test]
    fn stale_models_and_cancelled_streams_do_not_reappear() {
        let mut store = Store::open_in_memory().unwrap();
        let id = store.start_session("Isolation", None).unwrap().id;
        let mut r = runtime(&store);
        r.apply(
            &mut store,
            Event::Models(
                PathBuf::from("wrong-root"),
                vec![models::GENERATION_PROFILE.into()],
            ),
        )
        .unwrap();
        assert!(r.snapshot.installed_profiles.is_empty());
        let request = store.begin_request(&id, "Question").unwrap();
        r.snapshot.generation = Some(GenerationStatus {
            session_id: id.clone(),
            request_id: request.id.clone(),
            state: "generating".into(),
            text: "partial".into(),
        });
        r.cancel(&mut store).unwrap();
        r.apply(
            &mut store,
            Event::GenerationText(request.id, "stale".into()),
        )
        .unwrap();
        assert!(r.snapshot.generation.as_ref().unwrap().text.is_empty());
        assert!(store.session(&id).unwrap().active_request_id.is_none());
    }
    #[test]
    fn stopping_flushes_final_transcript_before_saving() {
        let mut store = Store::open_in_memory().unwrap();
        let id = store.start_session("Stop finalization", None).unwrap().id;
        let mut r = runtime(&store);
        r.capture = Some(CaptureJob {
            session: id.clone(),
            stop: Arc::new(AtomicBool::new(false)),
            end_action: None,
        });
        assert!(r.end_capture(&id, EndAction::Stop).unwrap());
        assert!(r.end_capture(&id, EndAction::Pause).unwrap());
        assert_eq!(store.session(&id).unwrap().status, SessionStatus::Recording);
        let utterance = Utterance {
            id: "segment".into(),
            source: Source::Microphone,
            start_ns: 0,
            end_ns: 1_000_000_000,
            samples: vec![0.0],
            finalized: true,
        };
        r.apply(
            &mut store,
            Event::Transcript(id.clone(), utterance, "Last utterance".into()),
        )
        .unwrap();
        r.apply(&mut store, Event::CaptureEnded(id.clone()))
            .unwrap();
        let saved = store.session(&id).unwrap();
        assert_eq!(saved.status, SessionStatus::Stopped);
        assert!(saved.saved_at.is_some());
        assert_eq!(saved.transcript[0].text, "Last utterance");
    }
    #[test]
    fn muted_resume_does_not_reopen_hardware_and_old_install_cannot_change_new_root() {
        let mut store = Store::open_in_memory().unwrap();
        let id = store.start_session("Muted", None).unwrap().id;
        store.mute_session(&id, true).unwrap();
        let mut r = runtime(&store);
        r.fixture = false;
        r.start_capture(&store, &id).unwrap();
        assert!(r.capture.is_none());
        r.install_id = Some("old-install".into());
        r.apply(
            &mut store,
            Event::InstallDone(
                "old-install".into(),
                PathBuf::from("obsolete-root"),
                Ok(models::GENERATION_PROFILE.into()),
            ),
        )
        .unwrap();
        assert!(r.snapshot.installed_profiles.is_empty());
    }
    #[test]
    fn screenshot_jobs_are_single_flight_and_release_on_error() {
        let gate = Arc::new(AtomicBool::new(false));
        let first = ScreenPermit::acquire(&gate).unwrap();
        assert!(ScreenPermit::acquire(&gate).is_err());
        drop(first);
        assert!(ScreenPermit::acquire(&gate).is_ok());
    }
    #[test]
    fn empty_generation_and_invalid_screen_question_leave_no_thinking_job() {
        use cueglint_local::generation::{FinishReason, ModelIdentity};
        let mut store = Store::open_in_memory().unwrap();
        let session = store.start_session("Terminal failures", None).unwrap().id;
        let mut r = runtime(&store);
        let request = store.begin_request(&session, "Question").unwrap();
        r.snapshot.generation = Some(GenerationStatus {
            session_id: session.clone(),
            request_id: request.id.clone(),
            state: "generating".into(),
            text: String::new(),
        });
        let output = GenerationOutput {
            text: String::new(),
            prompt_tokens: 1,
            generated_tokens: 0,
            max_new_tokens: 1,
            elapsed_ms: 0,
            finish_reason: FinishReason::EndOfTurn,
            model: ModelIdentity {
                profile_id: "fixture".into(),
                revision: "0".repeat(40),
                sha256: "0".repeat(64),
                tokenizer_sha256: "0".repeat(64),
                backend: "fixture".into(),
            },
            omitted_transcript_segments: 0,
            omitted_chat_turns: 0,
            screen_truncated: false,
            sources: vec![],
        };
        r.apply(&mut store, Event::GenerationDone(request.id, Ok(output)))
            .unwrap();
        assert!(store.session(&session).unwrap().active_request_id.is_none());
        r.snapshot.generation = Some(GenerationStatus {
            session_id: session.clone(),
            request_id: "screen-job".into(),
            state: "capturing screen".into(),
            text: String::new(),
        });
        let screen = ScreenContext {
            id: "fixture-screen".into(),
            display_id: 1,
            captured_at: 0,
            width: 1,
            height: 1,
            sha256: "0".repeat(64),
            text: "Original fixture".into(),
        };
        assert!(
            r.apply(
                &mut store,
                Event::Screen("screen-job".into(), session, "q".repeat(65_537), Ok(screen))
            )
            .is_err()
        );
        assert_eq!(r.snapshot.generation.as_ref().unwrap().state, "error");
    }
    #[test]
    fn full_final_queue_runs_stop_control_before_audio_can_be_enqueued() {
        let (tx, rx) = mpsc::sync_channel(1);
        let utterance = Utterance {
            id: "test".into(),
            source: Source::Microphone,
            start_ns: 0,
            end_ns: 1,
            samples: vec![],
            finalized: true,
        };
        tx.send(utterance.clone()).unwrap();
        let (stopped_tx, stopped_rx) = mpsc::channel();
        let thread = thread::spawn(move || {
            queue_utterance(&tx, utterance, 10, || {
                let _ = stopped_tx.send(());
                Ok(())
            })
        });
        stopped_rx
            .recv_timeout(Duration::from_millis(500))
            .expect("stop control was blocked by full queue");
        rx.recv().unwrap();
        let final_audio = rx.recv().unwrap();
        assert_eq!(final_audio.start_ns, 10);
        thread.join().unwrap().unwrap();
    }
}
