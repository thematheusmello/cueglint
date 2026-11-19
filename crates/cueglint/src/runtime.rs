use anyhow::{Context, Result, ensure};
use cueglint_core::{
    AudioMetadata, GenerationRequest, ScreenContext, ScreenInput, ScreenSource, SessionStatus,
    Settings, Speaker, Store, TranscriptSource, TranscriptUpdate, TranscriptionBackend,
    provider::{CredentialStatus, ProviderConfig},
};
use cueglint_local::{
    asr::Transcriber,
    audio::{
        AudioPacket, Capture, CaptureConfig, Endpoint, PACKET_QUEUE_CAPACITY, Source, Utterance,
    },
    generation::{GenerationOptions, GenerationOutput, LocalGenerator},
    models::{self, DownloadProgress},
    permissions, screen,
};
use cueglint_providers::{
    CredentialStore, NativeCredentialStore, ProviderClient, ProviderOutput, Secret, StreamEvent,
    load_for_config,
};
use eframe::egui;
#[path = "runtime_calendar.rs"]
mod calendar;
#[path = "runtime_highlights.rs"]
mod highlights;
pub use highlights::{HighlightsBinding, HighlightsPhase, HighlightsStatus};
use highlights::{HighlightsOperation, PrepareHighlights, RunHighlights};
#[path = "runtime_capture_inputs.rs"]
mod capture_inputs;
pub use capture_inputs::CaptureInputsStatus;
use capture_inputs::{CaptureInputsReport, InputRefresh};
#[path = "runtime_scoped_screen.rs"]
mod scoped_screen;
pub use scoped_screen::{PickerPhase, WindowPickerStatus};
use scoped_screen::{UnconfirmedCapture, WindowJob};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
    time::Duration,
};

#[derive(Clone, Default)]
pub struct RuntimeSnapshot {
    pub calendar: cueglint_providers::calendar::CalendarSnapshot,
    pub capture_inputs: CaptureInputsStatus,
    pub capture_config: CaptureConfig,
    pub highlights: Option<HighlightsStatus>,
    pub capture: Option<CaptureStatus>,
    pub generation: Option<GenerationStatus>,
    pub model_job: Option<ModelJob>,
    pub model_root: PathBuf,
    pub screen_enabled: bool,
    pub screen_input: ScreenInput,
    pub window_picker: Option<WindowPickerStatus>,
    pub window_cleanup_unconfirmed: bool,
    pub installed_profiles: Vec<String>,
    pub screen: Option<ScreenStatus>,
    pub permissions: permissions::Permissions,
    pub generation_profile: String,
    pub transcription_profile: String,
    pub transcription_backend: TranscriptionBackend,
    pub transcription_language: String,
    pub answer_provider: ProviderConfig,
    pub provider: Option<ProviderStatus>,
}

impl RuntimeSnapshot {
    pub fn generating(&self) -> bool {
        self.generation.as_ref().is_some_and(|status| {
            ["capturing screen", "generating"].contains(&status.state.as_str())
        }) || self
            .window_picker
            .as_ref()
            .is_some_and(|picker| picker.question && picker.phase != PickerPhase::Retiring)
    }
}

#[derive(Clone)]
pub struct ProviderStatus {
    pub id: String,
    pub config: ProviderConfig,
    pub credential: Option<CredentialStatus>,
    pub models: Vec<String>,
    pub documentation_url: Option<String>,
    pub state: String,
    pub message: Option<String>,
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
enum ModelOperation {
    Download,
    Import(PathBuf),
    Remove,
}
pub enum ModelChange {
    Installed(String),
    Removed(String),
}
#[derive(Clone)]
pub struct ScreenStatus {
    pub id: String,
    pub source: ScreenSource,
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
    profile: String,
    cancel: Arc<AtomicBool>,
}
enum InferenceCommand {
    PrepareHighlights(PrepareHighlights),
    RunHighlights(RunHighlights),
    Generate(GenerateJob, InferencePermit),
    Provider(ProviderJob, InferencePermit),
    Reset,
}

// Cancellation clears UI state before queued or native work has returned.
struct InferencePermit(Arc<AtomicUsize>);
impl InferencePermit {
    fn acquire(pending: &Arc<AtomicUsize>) -> Self {
        pending.fetch_add(1, Ordering::Relaxed);
        Self(pending.clone())
    }
}
impl Drop for InferencePermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Release);
    }
}

pub enum ProviderAction {
    Status,
    Save(Secret),
    Delete,
    Models,
    Test,
}
struct ProviderJob {
    id: String,
    config: ProviderConfig,
    action: ProviderAction,
    cancel: Arc<AtomicBool>,
}
pub struct ProviderReport {
    credential: CredentialStatus,
    models: Option<Vec<String>>,
    documentation_url: Option<String>,
    message: Option<String>,
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
    HighlightsPrepared(HighlightsBinding, Result<cueglint_core::NotesPlan, String>),
    HighlightsProgress(HighlightsBinding, cueglint_local::notes::NotesProgress),
    HighlightsDone(
        HighlightsBinding,
        Result<cueglint_core::NotesCompletion, String>,
    ),
    Models(String, PathBuf, Vec<String>),
    InstallProgress(String, DownloadProgress),
    ModelDone(String, PathBuf, Result<ModelChange, String>),
    Screen(String, String, String, Result<ScreenContext, String>),
    ScreenPreview(String, Result<ScreenContext, String>),
    WindowCaptured(String, scoped_screen::WindowWorkerResult),
    CaptureInputs(String, CaptureInputsReport),
    GenerationText(String, String),
    GenerationDone(String, Result<GenerationOutput, String>),
    HostedDone(String, Result<ProviderOutput, String>),
    ProviderDone(String, ProviderConfig, Result<ProviderReport, String>),
    CaptureStarted(String, Option<&'static str>),
    CaptureMetrics(String, u64, u64),
    Transcript(
        String,
        Utterance,
        String,
        Arc<models::ModelProfile>,
        String,
        String,
    ),
    CaptureError(String, String),
    CaptureEnded(String),
}

pub struct Runtime {
    calendar: Option<cueglint_providers::calendar::CalendarWorker>,
    input_refresh: Option<InputRefresh>,
    screen_preview_id: Option<String>,
    window_job: Option<WindowJob>,
    unconfirmed_capture: Option<UnconfirmedCapture>,
    highlights_operation: Option<HighlightsOperation>,
    pub snapshot: RuntimeSnapshot,
    events: Receiver<Event>,
    tx: SyncSender<Event>,
    generator: SyncSender<InferenceCommand>,
    inference_pending: Arc<AtomicUsize>,
    ctx: egui::Context,
    generation_cancel: Option<Arc<AtomicBool>>,
    install_cancel: Option<Arc<AtomicBool>>,
    install_id: Option<String>,
    capture: Option<CaptureJob>,
    fixture: bool,
    screen_busy: Arc<AtomicBool>,
    verification_busy: Arc<AtomicBool>,
    verification_cancel: Arc<AtomicBool>,
    model_epoch: String,
    provider_job: Option<String>,
    provider_cancel: Option<Arc<AtomicBool>>,
}

fn root(settings: &Settings) -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("CUEGLINT_MODEL_DIR")
        .map(PathBuf::from)
        .or_else(|| settings.model_directory.as_deref().map(PathBuf::from))
    {
        return Ok(path);
    }
    Ok(Store::default_path()?
        .parent()
        .context("invalid data path")?
        .join("models"))
}

impl Runtime {
    pub fn new(store: &Store, fixture: bool, ctx: egui::Context) -> Result<Self> {
        let mut runtime = Self::with_credentials(
            &store.settings()?,
            fixture,
            ctx,
            Box::new(NativeCredentialStore),
        )?;
        runtime.refresh_permissions();
        runtime.refresh_models();
        Ok(runtime)
    }

    pub fn prepare_restore(&self, settings: &Settings) -> Result<Self> {
        Self::with_credentials(
            settings,
            self.fixture,
            self.ctx.clone(),
            Box::new(NativeCredentialStore),
        )
    }

    fn with_credentials(
        settings: &Settings,
        fixture: bool,
        ctx: egui::Context,
        credentials: Box<dyn CredentialStore>,
    ) -> Result<Self> {
        let model_root = root(settings)?;
        let (tx, events) = mpsc::sync_channel(128);
        let (generator, jobs) = mpsc::sync_channel::<InferenceCommand>(2);
        let generate_tx = tx.clone();
        let generate_ctx = ctx.clone();
        thread::Builder::new()
            .name("cueglint-inference".into())
            .spawn(move || {
                let mut model: Option<(PathBuf, String, LocalGenerator)> = None;
                let mut prepared_highlights = None;
                while let Ok(command) = jobs.recv() {
                    let (job, permit) = match command {
                        InferenceCommand::PrepareHighlights(job) => {
                            let binding = job.binding.clone();
                            prepared_highlights = None;
                            let result = highlights::prepare(&mut model, job)
                                .map(|(prepared, plan)| {
                                    prepared_highlights = Some(prepared);
                                    plan
                                })
                                .map_err(|error| format!("{error:#}"));
                            if generate_tx
                                .send(Event::HighlightsPrepared(binding, result))
                                .is_err()
                            {
                                break;
                            }
                            generate_ctx.request_repaint();
                            continue;
                        }
                        InferenceCommand::RunHighlights(job) => {
                            let binding = job.binding.clone();
                            let result = highlights::run(
                                &mut model,
                                prepared_highlights.take(),
                                job,
                                |progress| {
                                    let _ = generate_tx.try_send(Event::HighlightsProgress(
                                        binding.clone(),
                                        progress,
                                    ));
                                    generate_ctx.request_repaint();
                                },
                            )
                            .map_err(|error| format!("{error:#}"));
                            if generate_tx
                                .send(Event::HighlightsDone(binding, result))
                                .is_err()
                            {
                                break;
                            }
                            generate_ctx.request_repaint();
                            continue;
                        }
                        InferenceCommand::Generate(job, permit) => (job, permit),
                        InferenceCommand::Provider(job, permit) => {
                            prepared_highlights = None;
                            let id = job.id.clone();
                            let config = job.config.clone();
                            let result = provider_operation(&*credentials, job)
                                .map_err(|error| error.to_string());
                            drop(permit);
                            if generate_tx
                                .send(Event::ProviderDone(id, config, result))
                                .is_err()
                            {
                                break;
                            }
                            generate_ctx.request_repaint();
                            continue;
                        }
                        InferenceCommand::Reset => {
                            model = None;
                            prepared_highlights = None;
                            continue;
                        }
                    };
                    prepared_highlights = None;
                    if !job.request.provider.provider.is_local() {
                        let result = (|| -> Result<ProviderOutput> {
                            ensure!(!job.cancel.load(Ordering::Relaxed), "request cancelled");
                            let client = ProviderClient::new(job.request.provider.clone())?;
                            let mut text = String::new();
                            Ok(client.generate_with_store(
                                &job.request,
                                256,
                                &*credentials,
                                &job.cancel,
                                |event| {
                                    if let StreamEvent::TextDelta(delta) = event {
                                        text.push_str(&delta);
                                        let _ = generate_tx.try_send(Event::GenerationText(
                                            job.request.id.clone(),
                                            text.clone(),
                                        ));
                                        generate_ctx.request_repaint();
                                    }
                                },
                            )?)
                        })()
                        .map_err(|error| error.to_string());
                        drop(permit);
                        if generate_tx
                            .send(Event::HostedDone(job.request.id, result))
                            .is_err()
                        {
                            break;
                        }
                        generate_ctx.request_repaint();
                        continue;
                    }
                    let result = (|| -> Result<GenerationOutput> {
                        ensure!(!job.cancel.load(Ordering::Relaxed), "request cancelled");
                        if model.as_ref().is_none_or(|(loaded, profile, _)| {
                            loaded != &job.root || profile != &job.profile
                        }) {
                            model = None;
                            let loaded = LocalGenerator::load_profile_with_cancel(
                                &job.root,
                                &job.profile,
                                &job.cancel,
                            )?;
                            model = Some((job.root.clone(), job.profile.clone(), loaded));
                        }
                        let mut text = String::new();
                        let request = job.request.id.clone();
                        model.as_mut().unwrap().2.generate(
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
                    drop(permit);
                    if generate_tx
                        .send(Event::GenerationDone(job.request.id, result))
                        .is_err()
                    {
                        break;
                    }
                    generate_ctx.request_repaint();
                }
            })?;
        Ok(Self {
            calendar: None,
            highlights_operation: None,
            snapshot: RuntimeSnapshot {
                calendar: calendar::initial_snapshot(),
                capture_config: CaptureConfig::from(settings),
                model_root,
                screen_enabled: settings.screen_enabled,
                screen_input: settings.screen_input,
                generation_profile: settings
                    .generation_profile
                    .clone()
                    .unwrap_or_else(|| models::GENERATION_PROFILE.into()),
                transcription_profile: settings
                    .transcription_profile
                    .clone()
                    .unwrap_or_else(|| models::TRANSCRIPTION_PROFILE.into()),
                transcription_backend: settings.transcription_backend,
                transcription_language: settings.transcription_language.clone(),
                answer_provider: settings.answer_provider.clone(),
                ..Default::default()
            },
            input_refresh: None,
            screen_preview_id: None,
            window_job: None,
            unconfirmed_capture: None,
            events,
            tx,
            generator,
            inference_pending: Arc::new(AtomicUsize::new(0)),
            ctx,
            generation_cancel: None,
            install_cancel: None,
            install_id: None,
            capture: None,
            fixture,
            screen_busy: Arc::new(AtomicBool::new(false)),
            verification_busy: Arc::new(AtomicBool::new(false)),
            verification_cancel: Arc::new(AtomicBool::new(false)),
            model_epoch: uuid(),
            provider_job: None,
            provider_cancel: None,
        })
    }

    pub fn ensure_restore_idle(&self) -> Result<()> {
        ensure!(
            self.capture.is_none(),
            "Stop audio capture and wait for it to finish before restoring."
        );
        ensure!(
            self.unconfirmed_capture.is_none() && !self.snapshot.window_cleanup_unconfirmed,
            "Window capture cleanup could not be confirmed. Restart Cueglint before restoring."
        );
        ensure!(
            self.window_job.is_none()
                && !self.screen_busy.load(Ordering::Acquire)
                && self.screen_preview_id.is_none(),
            "Wait for screen capture or window selection to finish before restoring."
        );
        ensure!(
            self.inference_pending.load(Ordering::Acquire) == 0
                && self.provider_job.is_none()
                && !self.snapshot.generation.as_ref().is_some_and(|job| {
                    matches!(job.state.as_str(), "generating" | "capturing screen")
                }),
            "Wait for the answer or provider operation to finish before restoring."
        );
        ensure!(
            self.install_id.is_none(),
            "Wait for the model file operation to finish before restoring."
        );
        ensure!(
            !self.highlights_busy(),
            "Wait for highlights work to finish before restoring."
        );
        Ok(())
    }

    pub fn refresh_permissions(&mut self) {
        if !self.fixture {
            self.snapshot.permissions = permissions::status();
        }
    }

    pub fn try_event(&self) -> Option<Event> {
        self.events.try_recv().ok()
    }

    pub fn refresh_models(&mut self) {
        let root = self.snapshot.model_root.clone();
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        if self.fixture || self.install_id.is_some() {
            return;
        }
        let busy = self.verification_busy.clone();
        if busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let epoch = self.model_epoch.clone();
        let cancel = self.verification_cancel.clone();
        thread::spawn(move || {
            let installed = models::profiles()
                .unwrap_or_default()
                .iter()
                .filter(|p| models::verify_with_cancel(&root, &p.id, &cancel).is_ok())
                .map(|p| p.id.clone())
                .collect();
            busy.store(false, Ordering::Release);
            let _ = tx.send(Event::Models(epoch, root, installed));
            ctx.request_repaint();
        });
    }

    pub fn settings_changed(&mut self, store: &mut Store) -> Result<()> {
        let settings = store.settings()?;
        let next = root(&settings)?;
        if self.snapshot.model_root != next {
            ensure!(
                self.capture.is_none(),
                "stop capture before changing the model directory"
            );
            self.cancel(store)?;
            self.cancel_highlights(store, cueglint_core::NotesCancelReason::User)?;
            self.cancel_install();
            self.snapshot.model_root = next;
            self.model_epoch = uuid();
            self.snapshot.installed_profiles.clear();
            self.refresh_models();
        }
        let capture_config = CaptureConfig::from(&settings);
        if capture_config.display_id != self.snapshot.capture_config.display_id
            || settings.screen_enabled != self.snapshot.screen_enabled
            || settings.screen_input != self.snapshot.screen_input
        {
            self.cancel(store)?;
            self.screen_preview_id = None;
            self.snapshot.screen = None;
        }
        self.snapshot.capture_config = capture_config;
        let generation = settings
            .generation_profile
            .unwrap_or_else(|| models::GENERATION_PROFILE.into());
        let transcription = settings
            .transcription_profile
            .unwrap_or_else(|| models::TRANSCRIPTION_PROFILE.into());
        if generation != self.snapshot.generation_profile
            || settings.answer_provider != self.snapshot.answer_provider
        {
            self.cancel(store)?;
            self.cancel_highlights(store, cueglint_core::NotesCancelReason::User)?;
        }
        self.snapshot.generation_profile = generation;
        self.snapshot.transcription_profile = transcription;
        self.snapshot.transcription_backend = settings.transcription_backend;
        self.snapshot.transcription_language = settings.transcription_language;
        self.snapshot.screen_enabled = settings.screen_enabled;
        self.snapshot.screen_input = settings.screen_input;
        if settings.answer_provider != self.snapshot.answer_provider {
            if self.snapshot.provider.as_ref().is_some_and(|status| {
                matches!(
                    status.state.as_str(),
                    "listing models" | "testing connection"
                )
            }) {
                self.cancel_provider();
            }
            self.snapshot.answer_provider = settings.answer_provider;
        }
        Ok(())
    }

    pub fn provider_operation(
        &mut self,
        id: String,
        config: ProviderConfig,
        action: ProviderAction,
    ) -> Result<()> {
        ensure!(
            !self.highlights_busy(),
            "wait for highlights work to finish or cancel"
        );
        ensure!(
            !self.fixture,
            "provider operations are disabled in deterministic fixtures"
        );
        config.validate()?;
        ensure!(
            !config.provider.is_local(),
            "local inference does not use an API key or network test"
        );
        ensure!(
            self.provider_job.is_none(),
            "wait for the provider operation or cancel it"
        );
        ensure!(
            uuid::Uuid::parse_str(&id).is_ok(),
            "invalid provider operation identity"
        );
        let cancel = Arc::new(AtomicBool::new(false));
        let state = match &action {
            ProviderAction::Status => "checking key",
            ProviderAction::Save(_) => "saving key",
            ProviderAction::Delete => "deleting key",
            ProviderAction::Models => "listing models",
            ProviderAction::Test => "testing connection",
        };
        self.generator
            .try_send(InferenceCommand::Provider(
                ProviderJob {
                    id: id.clone(),
                    config: config.clone(),
                    action,
                    cancel: cancel.clone(),
                },
                InferencePermit::acquire(&self.inference_pending),
            ))
            .map_err(|_| anyhow::anyhow!("inference queue is full; wait and retry"))?;
        let models = self
            .snapshot
            .provider
            .as_ref()
            .filter(|status| status.config == config)
            .map(|status| status.models.clone())
            .unwrap_or_default();
        let documentation_url = self
            .snapshot
            .provider
            .as_ref()
            .filter(|status| status.config == config)
            .and_then(|status| status.documentation_url.clone());
        self.snapshot.provider = Some(ProviderStatus {
            id: id.clone(),
            config,
            credential: None,
            models,
            documentation_url,
            state: state.into(),
            message: None,
        });
        self.provider_job = Some(id);
        self.provider_cancel = Some(cancel);
        Ok(())
    }

    pub fn cancel_provider(&mut self) {
        if let Some(cancel) = self.provider_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.provider_job = None;
        if let Some(status) = &mut self.snapshot.provider {
            status.state = "cancelled".into();
        }
    }

    pub fn install(&mut self, profile: String) -> Result<()> {
        self.start_model_job(profile, ModelOperation::Download)
    }

    pub fn import_model(&mut self, profile: String, source: PathBuf) -> Result<()> {
        self.start_model_job(profile, ModelOperation::Import(source))
    }

    pub fn remove_model(&mut self, profile: String) -> Result<()> {
        self.start_model_job(profile, ModelOperation::Remove)
    }

    fn start_model_job(&mut self, profile: String, operation: ModelOperation) -> Result<()> {
        ensure!(
            !self.highlights_busy(),
            "wait for highlights work and cancellation to finish before changing model files"
        );
        ensure!(
            !self.fixture,
            "model changes are disabled in deterministic fixtures"
        );
        ensure!(
            self.install_id.is_none(),
            "a model change is already running"
        );
        ensure!(
            self.capture.is_none(),
            "stop audio capture before changing model files"
        );
        ensure!(
            !self
                .snapshot
                .generation
                .as_ref()
                .is_some_and(|g| matches!(g.state.as_str(), "generating" | "capturing screen")),
            "wait for the answer or cancel it before changing model files"
        );
        models::profile(&profile)?;
        let id = uuid();
        let cancel = Arc::new(AtomicBool::new(false));
        self.install_cancel = Some(cancel.clone());
        self.install_id = Some(id.clone());
        self.model_epoch = uuid();
        self.snapshot.installed_profiles.retain(|p| p != &profile);
        self.snapshot.model_job = Some(ModelJob {
            profile_id: profile.clone(),
            file: String::new(),
            downloaded_bytes: 0,
            total_bytes: 0,
            state: match &operation {
                ModelOperation::Download => "downloading",
                ModelOperation::Import(_) => "importing",
                ModelOperation::Remove => "removing",
            }
            .into(),
        });
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        let root = self.snapshot.model_root.clone();
        let generator = self.generator.clone();
        thread::spawn(move || {
            let mut progress = |progress| {
                let _ = tx.try_send(Event::InstallProgress(id.clone(), progress));
                ctx.request_repaint();
            };
            let result = match operation {
                ModelOperation::Download => {
                    models::install(&root, &profile, &cancel, &mut progress)
                        .map(|_| ModelChange::Installed(profile))
                }
                ModelOperation::Import(source) => {
                    models::import(&root, &profile, &source, &cancel, &mut progress)
                        .map(|_| ModelChange::Installed(profile))
                }
                ModelOperation::Remove => {
                    if cancel.load(Ordering::Relaxed) {
                        Err(anyhow::anyhow!("model removal cancelled"))
                    } else {
                        models::remove(&root, &profile).map(|_| ModelChange::Removed(profile))
                    }
                }
            }
            .map_err(|e| format!("{e:#}"));
            let _ = generator.send(InferenceCommand::Reset);
            let _ = tx.send(Event::ModelDone(id, root, result));
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
        self.cancel_window_picker();
        self.screen_preview_id = None;
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

    pub fn ensure_answer_ready(&self, prompt: &str) -> Result<()> {
        ensure!(
            !self.highlights_busy(),
            "finish or cancel highlights before asking a question"
        );
        ensure!(
            !self.fixture,
            "real inference is disabled in deterministic fixtures"
        );
        ensure!(
            self.install_id.is_none(),
            "wait for the model file operation to finish before asking"
        );
        self.snapshot.answer_provider.validate()?;
        if !self.snapshot.answer_provider.provider.is_local() {
            self.snapshot.answer_provider.validate_for_generation()?;
        }
        ensure!(
            !self.snapshot.answer_provider.provider.is_local()
                || self
                    .snapshot
                    .installed_profiles
                    .contains(&self.snapshot.generation_profile),
            "selected local answer model is not verified; download or import it in Settings"
        );
        cueglint_core::validate_prompt(prompt)?;
        Ok(())
    }

    pub fn ask(&mut self, store: &mut Store, session: &str, prompt: String) -> Result<()> {
        self.ensure_answer_ready(&prompt)?;
        ensure!(
            !self.snapshot.screen_enabled || self.snapshot.screen_input.uses_display(),
            "choose a window in the macOS system picker before capturing screen text"
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

    pub fn ask_saved_meetings(
        &mut self,
        store: &mut Store,
        session: Option<&str>,
        prompt: &str,
        new_chat_mode: Option<&str>,
    ) -> Result<String> {
        ensure!(
            self.snapshot.answer_provider.provider.is_local(),
            "select Local in Settings to ask about saved meetings"
        );
        self.ensure_answer_ready(prompt)?;
        self.cancel(store)?;
        let request = if let Some(session) = session {
            store.begin_request(session, prompt)?
        } else {
            store.start_history_chat(new_chat_mode, prompt)?
        };
        let id = request.session_id.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        self.generation_cancel = Some(cancel.clone());
        self.generate(store, request, cancel)?;
        Ok(id)
    }

    pub fn withdraw_history_answer(&mut self, store: &mut Store) -> Result<()> {
        if let Some(status) = &self.snapshot.generation
            && let Ok(session) = store.session(&status.session_id)
            && session.history_chat
            && session.requests.iter().any(|request| {
                request.id == status.request_id
                    && request.context.past_meetings
                        == Some(cueglint_core::MeetingContext::Withdrawn)
            })
        {
            self.cancel(store)?;
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
            .try_send(InferenceCommand::Generate(
                GenerateJob {
                    request,
                    root: self.snapshot.model_root.clone(),
                    profile: self.snapshot.generation_profile.clone(),
                    cancel,
                },
                InferencePermit::acquire(&self.inference_pending),
            ))
            .is_err()
        {
            store.cancel_request(&session, &id)?;
            self.snapshot.generation.as_mut().unwrap().state = "error".into();
            self.generation_cancel = None;
            anyhow::bail!("inference queue is full; cancel and retry");
        }
        Ok(())
    }

    pub fn preview_screen(&mut self, store: &mut Store) -> Result<()> {
        if self.snapshot.screen_input == ScreenInput::Window {
            return self.start_window_picker(None, store);
        }
        ensure!(
            !self.fixture,
            "screen acquisition is disabled in deterministic fixtures"
        );
        let permit = ScreenPermit::acquire(&self.screen_busy)?;
        let display = store.settings()?.display_id;
        let id = uuid();
        self.screen_preview_id = Some(id.clone());
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        thread::spawn(move || {
            let result = fresh_screen(display).map_err(|e| format!("{e:#}"));
            drop(permit);
            let _ = tx.send(Event::ScreenPreview(id, result));
            ctx.request_repaint();
        });
        Ok(())
    }

    pub fn start_capture(&mut self, store: &Store, session: &str) -> Result<()> {
        ensure!(
            !self.highlights_busy(),
            "wait for highlights cancellation to finish before starting capture"
        );
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
            cueglint_local::asr::backend_compiled(settings.transcription_backend),
            "Metal ASR is unavailable in this build; select CPU in Settings before listening"
        );
        ensure!(
            self.install_id.is_none(),
            "wait for the model file operation to finish before listening"
        );
        cueglint_local::asr::validate_language(
            &self.snapshot.transcription_profile,
            &settings.transcription_language,
        )?;
        ensure!(
            self.snapshot
                .installed_profiles
                .iter()
                .any(|p| p == &self.snapshot.transcription_profile),
            "Install and verify the selected Whisper model before listening; session is retained locally"
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
        if matches!(
            &event,
            Event::HighlightsPrepared(..)
                | Event::HighlightsProgress(..)
                | Event::HighlightsDone(..)
        ) {
            return self.apply_highlights(store, event);
        }
        if matches!(
            &event,
            Event::Screen(..)
                | Event::ScreenPreview(..)
                | Event::CaptureStarted(..)
                | Event::CaptureError(..)
                | Event::CaptureEnded(..)
        ) {
            self.refresh_permissions();
        }
        match event {
            Event::CaptureInputs(id, report) => self.apply_capture_inputs(id, report),
            Event::Models(epoch, root, installed)
                if epoch == self.model_epoch
                    && root == self.snapshot.model_root
                    && self.install_id.is_none() =>
            {
                self.snapshot.installed_profiles = installed
            }
            Event::Models(_, _, _) => self.refresh_models(),
            Event::InstallProgress(id, p) if self.install_id.as_ref() == Some(&id) => {
                if let Some(job) = &mut self.snapshot.model_job {
                    job.file = p.file;
                    job.downloaded_bytes = p.downloaded_bytes;
                    job.total_bytes = p.total_bytes;
                }
            }
            Event::ModelDone(id, root, result) if self.install_id.as_ref() == Some(&id) => {
                self.install_id = None;
                self.install_cancel = None;
                if root != self.snapshot.model_root {
                    if let Some(job) = &mut self.snapshot.model_job {
                        job.state = "cancelled".into();
                    }
                    self.refresh_models();
                    return Ok((false, None));
                }
                match result {
                    Ok(ModelChange::Installed(profile)) => {
                        if !self.snapshot.installed_profiles.contains(&profile) {
                            self.snapshot.installed_profiles.push(profile);
                        }
                        if let Some(job) = &mut self.snapshot.model_job {
                            job.state = "ready".into();
                        }
                    }
                    Ok(ModelChange::Removed(profile)) => {
                        self.snapshot.installed_profiles.retain(|p| p != &profile);
                        if let Some(job) = &mut self.snapshot.model_job {
                            job.state = "removed".into();
                        }
                    }
                    Err(error) => {
                        if let Some(job) = &mut self.snapshot.model_job {
                            job.state = "error".into();
                        }
                        self.refresh_models();
                        return Ok((false, Some(error)));
                    }
                }
                self.refresh_models();
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
            Event::ScreenPreview(id, result) if self.screen_preview_id.as_ref() == Some(&id) => {
                self.screen_preview_id = None;
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
            Event::ProviderDone(id, config, result)
                if self.provider_job.as_ref() == Some(&id)
                    && self
                        .snapshot
                        .provider
                        .as_ref()
                        .is_some_and(|status| status.config == config) =>
            {
                self.provider_job = None;
                self.provider_cancel = None;
                let status = self.snapshot.provider.as_mut().unwrap();
                match result {
                    Ok(report) => {
                        status.credential = Some(report.credential);
                        if let Some(models) = report.models {
                            status.models = models;
                            status.documentation_url = report.documentation_url;
                        }
                        status.state = "ready".into();
                        status.message = report.message;
                    }
                    Err(error) => {
                        status.state = "error".into();
                        status.message = Some(error);
                    }
                }
            }
            Event::HostedDone(id, result)
                if self.snapshot.generation.as_ref().is_some_and(|status| {
                    status.request_id == id && status.state == "generating"
                }) =>
            {
                let status = self.snapshot.generation.as_mut().unwrap();
                self.generation_cancel = None;
                status.state = "error".into();
                match result {
                    Ok(output) if !output.text.trim().is_empty() => {
                        if store.complete_request_with_hosted_metadata(
                            &status.session_id,
                            &id,
                            &output.text,
                            output.metadata,
                        )? {
                            status.text = output.text;
                            status.state = "completed".into();
                        } else {
                            status.text.clear();
                            status.state = "cancelled".into();
                        }
                        return Ok((true, None));
                    }
                    Ok(_) => {
                        store.cancel_request(&status.session_id, &id)?;
                        status.text.clear();
                        status.state = "cancelled".into();
                        return Ok((
                            true,
                            Some("The provider returned an empty answer. You can retry.".into()),
                        ));
                    }
                    Err(error) => {
                        store.cancel_request(&status.session_id, &id)?;
                        status.text.clear();
                        return Ok((true, Some(error)));
                    }
                }
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
            Event::CaptureStarted(id, timing_notice)
                if self.capture.as_ref().is_some_and(|c| c.session == id) =>
            {
                if let Some(s) = &mut self.snapshot.capture {
                    if s.state == "initializing" {
                        s.state = "listening".into();
                        return Ok((false, timing_notice.map(str::to_owned)));
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
            Event::Transcript(id, utterance, text, profile, backend, language)
                if self.capture.as_ref().is_some_and(|c| c.session == id) =>
            {
                if !text.trim().is_empty() {
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
                            language,
                            backend: Some(backend),
                            frontend_revision: Some(cueglint_local::asr_frontend::REVISION.into()),
                            frontend_config_sha256: Some(
                                cueglint_local::asr_frontend::configuration_sha256(),
                            ),
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
                let saved = matches!(job.end_action, Some(EndAction::Stop));
                let finalization = match job.end_action {
                    Some(EndAction::Stop) => store.finish_session(&id).map(|_| ()),
                    Some(EndAction::Pause) => store.pause_session(&id).map(|_| ()),
                    Some(EndAction::Mute) => store.mute_session(&id, true).map(|_| ()),
                    None => store.session(&id).and_then(|session| {
                        if session.status == SessionStatus::Recording {
                            store.pause_session(&id)?;
                        }
                        Ok(())
                    }),
                };
                let mut failed = finalization.is_err();
                if let Some(status) = &mut self.snapshot.capture {
                    failed |= status.state == "error";
                    status.state = if failed { "error" } else { "stopped" }.into();
                }
                finalization.context("finalizing the stopped audio session")?;
                return Ok((
                    true,
                    (saved && !failed).then(|| "Session saved locally.".into()),
                ));
            }
            _ => {}
        }
        Ok((false, None))
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.verification_cancel.store(true, Ordering::Relaxed);
        self.cancel_window_picker();
        if let Some(refresh) = &self.input_refresh {
            refresh.cancel.store(true, Ordering::Relaxed);
        }
        if let Some(operation) = &self.highlights_operation {
            operation.cancel.store(true, Ordering::Relaxed);
        }
        if let Some(job) = &self.capture {
            job.stop.store(true, Ordering::Relaxed);
        }
        if let Some(cancel) = &self.generation_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        self.cancel_install();
        self.cancel_provider();
    }
}

fn uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn provider_operation(
    credentials: &dyn CredentialStore,
    job: ProviderJob,
) -> Result<ProviderReport> {
    ensure!(
        !job.cancel.load(Ordering::Relaxed),
        "provider operation cancelled"
    );
    let reference = job.config.credential_ref()?;
    let mut models = None;
    let mut documentation_url = None;
    let mut message = None;
    match job.action {
        ProviderAction::Status => {}
        ProviderAction::Save(secret) => {
            credentials.store(&reference, secret)?;
            message = Some(
                "Key saved in the OS credential store. The active answer provider is unchanged."
                    .into(),
            );
        }
        ProviderAction::Delete => {
            credentials.delete(&reference)?;
            message = Some("Key deleted. The active answer provider is unchanged.".into());
        }
        ProviderAction::Models => {
            let key = load_for_config(credentials, &job.config)?;
            let catalog =
                ProviderClient::new(job.config)?.list_models(key.as_ref(), &job.cancel)?;
            message = Some(if catalog.models.is_empty() {
                "This provider has no model list available here. Enter a model ID from its documentation.".into()
            } else if catalog.truncated {
                "Model list limited. You can still enter a model ID manually.".into()
            } else {
                "Model list refreshed. Account access and model availability can vary.".into()
            });
            documentation_url = catalog.documentation_url;
            models = Some(catalog.models.into_iter().map(|model| model.id).collect());
        }
        ProviderAction::Test => {
            let key = load_for_config(credentials, &job.config)?;
            ProviderClient::new(job.config)?.test_connection(key.as_ref(), &job.cancel)?;
            message = Some("Connection succeeded using a synthetic test instruction. No meeting context was sent.".into());
        }
    }
    Ok(ProviderReport {
        credential: credentials.status(&reference),
        models,
        documentation_url,
        message,
    })
}
fn fresh_screen(display: Option<u32>) -> Result<ScreenContext> {
    let s = screen::capture(display)?;
    Ok(ScreenContext {
        id: s.id,
        source: ScreenSource::Display(s.display_id),
        captured_at: s.captured_at,
        width: s.width,
        height: s.height,
        sha256: s.sha256,
        pixel_sha256: None,
        text: s.text,
    })
}
fn screen_status(s: &ScreenContext) -> ScreenStatus {
    ScreenStatus {
        id: s.id.clone(),
        source: s.source,
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
    let profile_id = settings
        .transcription_profile
        .as_deref()
        .unwrap_or(models::TRANSCRIPTION_PROFILE);
    let (mut transcriber, profile) = Transcriber::load_profile_with_language(
        root,
        profile_id,
        settings.transcription_backend,
        &settings.transcription_language,
        &stop,
    )?;
    let profile = Arc::new(profile);
    if stop.load(Ordering::Relaxed) {
        return Ok(());
    }
    if settings.microphone_enabled {
        permissions::ensure_microphone(&stop)?;
    }
    #[cfg(not(target_os = "windows"))]
    if settings.system_audio_enabled {
        permissions::ensure_screen()?;
    }
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
        let config = CaptureConfig::from(&settings);
        let mut capture = Capture::start(&config)?;
        let mut endpoints = [
            Endpoint::new(Source::Microphone),
            Endpoint::new(Source::SystemAudio),
        ];
        let mut stop_error = None;
        let running = (|| -> Result<()> {
            capture_tx.send(Event::CaptureStarted(
                capture_session.clone(),
                capture.timing_notice(),
            ))?;
            capture_ctx.request_repaint();
            let mut metrics_at = std::time::Instant::now();
            while !capture_stop.load(Ordering::Relaxed)
                && capture.device_errors.load(Ordering::Relaxed) == 0
            {
                if let Some(packet) = capture.recv_timeout(Duration::from_millis(50))? {
                    let endpoint = if packet.source == Source::Microphone {
                        &mut endpoints[0]
                    } else {
                        &mut endpoints[1]
                    };
                    for utterance in endpoint.push(&packet).with_context(|| {
                        format!(
                            "processing {:?} audio packet {}",
                            packet.source, packet.sequence
                        )
                    })? {
                        queue_utterance(&utterances, utterance, offset_ns, || {
                            if capture_stop.load(Ordering::Relaxed)
                                || capture.device_errors.load(Ordering::Relaxed) != 0
                            {
                                if let Err(error) = capture.stop() {
                                    stop_error = Some(error);
                                }
                            }
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
                }
            }
            Ok(())
        })();
        let finalized = finish_capture_audio(
            capture.stop(),
            capture.stopped_packets(),
            &mut endpoints,
            |utterance| queue_utterance(&utterances, utterance, offset_ns, || {}),
        );
        let device_result = if capture.device_errors.load(Ordering::Relaxed) == 0 {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "audio device reported an error; capture stopped, reconnect the source and resume"
            ))
        };
        let metrics = capture_tx
            .send(Event::CaptureMetrics(
                capture_session,
                capture.dropped_packets.load(Ordering::Relaxed),
                capture.device_errors.load(Ordering::Relaxed),
            ))
            .context("reporting final audio capture metrics");
        audio_errors(
            [
                running,
                stop_error.map_or(Ok(()), Err),
                finalized,
                device_result,
                metrics,
            ]
            .into_iter()
            .filter_map(Result::err),
        )
    });
    let decode_cancel = AtomicBool::new(false);
    let backend = transcriber.backend().to_owned();
    let language = transcriber.language().to_owned();
    let decoding = transcribe_pending(
        pending,
        &stop,
        |utterance| transcriber.transcribe(&utterance.samples, &decode_cancel),
        |utterance, text| {
            tx.send(Event::Transcript(
                session.to_owned(),
                utterance,
                text,
                profile.clone(),
                backend.clone(),
                language.clone(),
            ))
            .context("transcript event receiver disconnected")?;
            ctx.request_repaint();
            Ok(())
        },
    );
    stop.store(true, Ordering::Relaxed);
    let capture_result = capture_thread
        .join()
        .unwrap_or_else(|_| Err(anyhow::anyhow!("audio capture worker panicked")));
    audio_errors(
        [decoding, capture_result]
            .into_iter()
            .filter_map(Result::err),
    )
}

fn finish_capture_audio(
    stopped: Result<()>,
    mut packets: impl Iterator<Item = Result<AudioPacket>>,
    endpoints: &mut [Endpoint; 2],
    mut emit: impl FnMut(Utterance) -> Result<()>,
) -> Result<()> {
    let mut errors: Vec<_> = stopped.err().into_iter().collect();
    // A failed native stop must not let callbacks extend shutdown indefinitely.
    for packet in packets.by_ref().take(PACKET_QUEUE_CAPACITY) {
        let packet = match packet {
            Ok(packet) => packet,
            Err(error) => {
                errors.push(error.context("normalizing final audio packet"));
                continue;
            }
        };
        let endpoint = if packet.source == Source::Microphone {
            &mut endpoints[0]
        } else {
            &mut endpoints[1]
        };
        match endpoint.push(&packet) {
            Ok(utterances) => {
                for utterance in utterances {
                    errors.extend(emit(utterance).err());
                }
            }
            Err(error) => errors.push(error.context(format!(
                "draining {:?} audio packet {}",
                packet.source, packet.sequence
            ))),
        }
    }
    if let Some(packet) = packets.next() {
        errors.push(match packet {
            Ok(packet) => anyhow::anyhow!(
                "audio continued after stop; drain limited to {PACKET_QUEUE_CAPACITY} packets, undelivered {:?} audio packet {} and any trailing audio",
                packet.source,
                packet.sequence
            ),
            Err(error) => error.context(format!(
                "audio continued after stop; drain limited to {PACKET_QUEUE_CAPACITY} packets"
            )),
        });
    }
    for endpoint in endpoints {
        if let Some(utterance) = endpoint.finish() {
            errors.extend(emit(utterance).err());
        }
    }
    audio_errors(errors)
}

fn transcribe_pending(
    pending: Receiver<Utterance>,
    stop: &AtomicBool,
    mut transcribe: impl FnMut(&Utterance) -> Result<String>,
    mut emit: impl FnMut(Utterance, String) -> Result<()>,
) -> Result<()> {
    let mut errors = Vec::new();
    for utterance in pending {
        let identity = format!("{:?} segment {}", utterance.source, utterance.id);
        match transcribe(&utterance) {
            Ok(text) => {
                if let Err(error) = emit(utterance, text) {
                    stop.store(true, Ordering::Relaxed);
                    errors.push(error.context(format!("delivering {identity}")));
                    break;
                }
            }
            Err(error) => {
                stop.store(true, Ordering::Relaxed);
                errors.push(error.context(format!("transcribing {identity}")));
            }
        }
    }
    audio_errors(errors)
}

fn audio_errors(errors: impl IntoIterator<Item = anyhow::Error>) -> Result<()> {
    let mut errors = errors.into_iter();
    let Some(primary) = errors.next() else {
        return Ok(());
    };
    let secondary: Vec<_> = errors.map(|error| format!("{error:#}")).collect();
    if secondary.is_empty() {
        Err(primary)
    } else {
        Err(primary.context(format!("additional audio errors: {}", secondary.join("; "))))
    }
}

fn queue_utterance(
    tx: &SyncSender<Utterance>,
    mut utterance: Utterance,
    offset_ns: u64,
    mut on_backpressure: impl FnMut(),
) -> Result<()> {
    utterance.start_ns += offset_ns;
    utterance.end_ns += offset_ns;
    if utterance.finalized {
        loop {
            match tx.try_send(utterance) {
                Ok(()) => break,
                Err(TrySendError::Full(pending)) => {
                    utterance = pending;
                    on_backpressure();
                    thread::sleep(Duration::from_millis(10));
                }
                Err(TrySendError::Disconnected(utterance)) => {
                    anyhow::bail!(
                        "transcription worker disconnected before {:?} segment {} was delivered",
                        utterance.source,
                        utterance.id
                    )
                }
            }
        }
    } else {
        match tx.try_send(utterance) {
            Ok(()) | Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(utterance)) => {
                anyhow::bail!(
                    "transcription worker disconnected before {:?} segment {} was delivered",
                    utterance.source,
                    utterance.id
                )
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "runtime_audio_tests.rs"]
mod audio_tests;
#[cfg(test)]
#[path = "runtime_highlights_tests.rs"]
mod highlights_tests;
#[cfg(test)]
#[path = "runtime_provider_tests.rs"]
pub(crate) mod provider_tests;

#[cfg(test)]
impl Runtime {
    pub(crate) fn with_scripted_answer_queue(
        store: &Store,
        context: egui::Context,
    ) -> Result<(Self, impl Fn() -> Option<GenerationRequest> + use<>)> {
        let mut runtime = Self::new(store, true, context)?;
        let (sender, jobs) = mpsc::sync_channel(1);
        // Intercept any provider's job before allowing the production queue path.
        runtime.generator = sender;
        runtime.fixture = false;
        runtime.snapshot.installed_profiles = vec![runtime.snapshot.generation_profile.clone()];
        Ok((runtime, move || {
            jobs.try_recv().ok().map(|command| match command {
                InferenceCommand::Generate(job, _) => job.request,
                _ => panic!("expected a scripted answer request"),
            })
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn runtime(store: &Store) -> Runtime {
        Runtime::new(store, true, egui::Context::default()).unwrap()
    }

    #[test]
    fn ask_saved_meetings_queues_local_text_without_capture_and_drops_withdrawn_streams()
    -> Result<()> {
        use crate::worker::{Command, execute};

        let mut store = Store::open_in_memory()?;
        let meeting = store.start_session("Original fixture", None)?;
        store.update_notes(&meeting.id, "Cobalt workshop opens on Friday.")?;
        store.finish_session(&meeting.id)?;
        let mut selected = Some(meeting.id.clone());
        let mut runtime = runtime(&store);
        let (sender, jobs) = mpsc::sync_channel(1);
        runtime.generator = sender;
        runtime.fixture = false;
        runtime.snapshot.screen_enabled = true;
        runtime.snapshot.screen_input = cueglint_core::ScreenInput::Window;
        runtime
            .snapshot
            .installed_profiles
            .push(runtime.snapshot.generation_profile.clone());
        execute(
            &mut store,
            &mut selected,
            &mut runtime,
            Command::AskSavedMeetings {
                prompt: "When does the Cobalt workshop open?".into(),
                new_chat_mode: Some("meeting".into()),
            },
        )?;
        let InferenceCommand::Generate(job, _permit) = jobs.try_recv()? else {
            panic!("saved meeting request must enter the actual inference queue");
        };
        let request = &job.request;
        assert_eq!(selected.as_deref(), Some(request.session_id.as_str()));
        assert_ne!(request.session_id, meeting.id);
        assert!(request.context.screen.is_none() && runtime.snapshot.window_picker.is_none());
        assert!(request.provider.provider.is_local());
        assert!(matches!(
            &request.context.past_meetings,
            Some(cueglint_core::MeetingContext::Excerpts(excerpts))
                if excerpts.len() == 1 && excerpts[0].meeting_id == meeting.id
        ));
        runtime.apply(
            &mut store,
            Event::GenerationText(request.id.clone(), "Friday".into()),
        )?;
        assert_eq!(runtime.snapshot.generation.as_ref().unwrap().text, "Friday");
        execute(
            &mut store,
            &mut selected,
            &mut runtime,
            Command::Notes(meeting.id.clone(), "Cobalt workshop is postponed.".into()),
        )?;
        assert!(job.cancel.load(Ordering::Relaxed));
        runtime.apply(
            &mut store,
            Event::GenerationText(request.id.clone(), "Late private text".into()),
        )?;
        let status = runtime.snapshot.generation.as_ref().unwrap();
        assert_eq!(status.state, "cancelled");
        assert!(status.text.is_empty());

        let mut settings = store.settings()?;
        settings.answer_provider = ProviderConfig {
            provider: cueglint_core::ProviderId::OpenAi,
            model_id: "original-dummy".into(),
            base_url: None,
        };
        runtime.snapshot.answer_provider = settings.answer_provider.clone();
        store.update_settings(settings)?;
        let error = execute(
            &mut store,
            &mut selected,
            &mut runtime,
            Command::Ask {
                session: Some(request.session_id.clone()),
                prompt: "When does the Cobalt workshop open?".into(),
                new_chat_mode: None,
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("select Local"));
        assert!(jobs.try_recv().is_err() && runtime.snapshot.window_picker.is_none());
        Ok(())
    }

    #[test]
    fn history_ask_worker_freezes_fresh_context_and_refuses_to_replace_active_generation()
    -> Result<()> {
        use crate::worker::{Command, execute};

        let mut store = Store::open_in_memory()?;
        let meeting = store.start_session("Original field notebook meeting", Some("meeting"))?;
        store.upsert_transcript(
            &meeting.id,
            TranscriptUpdate {
                id: "original-utterance".into(),
                text: "An unrelated original transcript.".into(),
                source: TranscriptSource::Manual,
                speaker: Speaker::Other,
                finalized: true,
            },
        )?;
        store.update_notes(&meeting.id, "Original private meeting note.")?;
        let prior = store.begin_request(&meeting.id, "Previously discussed details?")?;
        assert!(store.complete_request(&meeting.id, &prior.id, "Previous meeting answer.")?);
        let meeting = store.finish_session(&meeting.id)?;
        let mut selected = Some(meeting.id.clone());
        let mut runtime = runtime(&store);
        assert!(!runtime.snapshot.screen_enabled);
        let (sender, jobs) = mpsc::sync_channel(1);
        runtime.generator = sender;
        runtime.fixture = false;
        runtime
            .snapshot
            .installed_profiles
            .push(runtime.snapshot.generation_profile.clone());
        let command = |prompt: &str| Command::AskFromHistory {
            prompt: prompt.into(),
            new_chat_mode: Some("interview".into()),
        };
        assert!(
            execute(
                &mut store,
                &mut selected,
                &mut runtime,
                command("invalid\0query")
            )
            .is_err()
        );
        assert_eq!(selected.as_deref(), Some(meeting.id.as_str()));
        assert!(jobs.try_recv().is_err());
        assert_eq!(store.sessions("")?, [meeting.clone()]);

        let prompt = "When does the  Río trail open?";
        execute(&mut store, &mut selected, &mut runtime, command(prompt))?;
        let InferenceCommand::Generate(job, _permit) = jobs.try_recv()? else {
            panic!("the enquiry must enter the ordinary inference queue");
        };
        assert_ne!(job.request.session_id, meeting.id);
        assert_eq!(selected.as_deref(), Some(job.request.session_id.as_str()));
        assert_eq!(job.request.prompt, prompt);
        assert_eq!(job.request.provider, ProviderConfig::default());
        assert_eq!(job.request.context.mode, Some(store.mode("interview")?));
        assert!(job.request.context.chat.is_empty());
        assert!(job.request.context.transcript.is_empty());
        assert!(job.request.context.screen.is_none());
        assert_eq!(
            store.session(&job.request.session_id)?.requests,
            [job.request.clone()]
        );
        assert_eq!(store.session(&meeting.id)?, meeting);

        let error = execute(
            &mut store,
            &mut selected,
            &mut runtime,
            command("A second enquiry"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("Wait for the current response"));
        assert_eq!(selected.as_deref(), Some(job.request.session_id.as_str()));
        assert!(!job.cancel.load(Ordering::Relaxed));
        assert_eq!(
            runtime.snapshot.generation.as_ref().unwrap().request_id,
            job.request.id
        );
        assert_eq!(store.sessions("")?.len(), 2);
        assert!(jobs.try_recv().is_err());
        Ok(())
    }

    #[test]
    fn restored_runtime_has_no_events_from_a_previous_capture_with_the_same_session_id() {
        let store = Store::open_in_memory().unwrap();
        let old = runtime(&store);
        let fresh = old.prepare_restore(&store.settings().unwrap()).unwrap();
        let session = "retained-session-id";
        old.tx.send(Event::CaptureEnded(session.into())).unwrap();

        assert!(fresh.try_event().is_none());
        assert!(matches!(
            old.try_event(),
            Some(Event::CaptureEnded(id)) if id == session
        ));
    }

    #[test]
    fn changing_asr_selection_preserves_generation_work_and_the_saved_choice() {
        let mut store = Store::open_in_memory().unwrap();
        let session = store
            .start_session("Independent ASR selection", None)
            .unwrap()
            .id;
        let request = store.begin_request(&session, "Keep answering").unwrap();
        let mut r = runtime(&store);
        assert_eq!(r.snapshot.transcription_backend, TranscriptionBackend::Cpu);
        assert_eq!(r.snapshot.transcription_language, "en");
        let cancelled = Arc::new(AtomicBool::new(false));
        r.generation_cancel = Some(cancelled.clone());
        r.snapshot.generation = Some(GenerationStatus {
            session_id: session.clone(),
            request_id: request.id.clone(),
            state: "generating".into(),
            text: "Current answer".into(),
        });
        let mut settings = store.settings().unwrap();
        settings.transcription_backend = TranscriptionBackend::Metal;
        settings.transcription_language = "pt".into();
        settings.transcription_profile = Some(models::MULTILINGUAL_TRANSCRIPTION_PROFILE.into());
        store.update_settings(settings).unwrap();
        r.settings_changed(&mut store).unwrap();
        assert!(!cancelled.load(Ordering::Relaxed));
        assert_eq!(
            r.snapshot.transcription_backend,
            TranscriptionBackend::Metal
        );
        assert_eq!(r.snapshot.transcription_language, "pt");
        assert_eq!(
            r.snapshot.transcription_profile,
            models::MULTILINGUAL_TRANSCRIPTION_PROFILE
        );
        assert_eq!(
            r.snapshot.generation.as_ref().unwrap().text,
            "Current answer"
        );
        assert_eq!(
            store.session(&session).unwrap().active_request_id,
            Some(request.id)
        );
        assert_eq!(
            runtime(&store).snapshot.transcription_backend,
            TranscriptionBackend::Metal
        );
        assert_eq!(runtime(&store).snapshot.transcription_language, "pt");
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
                "old-verification".into(),
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
        assert!(r.ensure_restore_idle().is_err());
        let utterance = Utterance {
            id: "segment".into(),
            source: Source::Microphone,
            start_ns: 0,
            end_ns: 1_000_000_000,
            samples: vec![0.0],
            finalized: true,
        };
        let mut bound_profile = models::profile("whisper-tiny-en").unwrap();
        let weights = bound_profile
            .files
            .iter_mut()
            .find(|file| file.name == "model.safetensors")
            .unwrap();
        weights.sha256 = "a".repeat(64);
        weights.revision = "b".repeat(40);
        r.snapshot.transcription_language = "pt".into();
        r.apply(
            &mut store,
            Event::Transcript(
                id.clone(),
                utterance,
                "Last utterance".into(),
                Arc::new(bound_profile),
                "fixture".into(),
                "en".into(),
            ),
        )
        .unwrap();
        let (updated, notice) = r
            .apply(&mut store, Event::CaptureEnded(id.clone()))
            .unwrap();
        assert!(updated);
        assert_eq!(notice.as_deref(), Some("Session saved locally."));
        r.ensure_restore_idle().unwrap();
        let saved = store.session(&id).unwrap();
        assert_eq!(saved.status, SessionStatus::Stopped);
        assert!(saved.saved_at.is_some());
        assert_eq!(saved.transcript[0].text, "Last utterance");
        assert_eq!(
            saved.transcript[0].audio.as_ref().unwrap().model_profile,
            "whisper-tiny-en"
        );
        let audio = saved.transcript[0].audio.as_ref().unwrap();
        assert_eq!(audio.language, "en");
        assert_eq!(audio.model_sha256, "a".repeat(64));
        assert_eq!(audio.model_revision, "b".repeat(40));
        assert_eq!(audio.backend.as_deref(), Some("fixture"));
        assert_eq!(
            audio.frontend_revision.as_deref(),
            Some(cueglint_local::asr_frontend::REVISION)
        );
        assert_eq!(
            audio.frontend_config_sha256.as_deref(),
            Some(cueglint_local::asr_frontend::configuration_sha256().as_str())
        );
    }
    #[test]
    fn multilingual_transcript_events_retain_worker_language_and_session_identity() {
        let mut store = Store::open_in_memory().unwrap();
        let other = store.start_session("Unrelated session", None).unwrap().id;
        store.stop_session(&other).unwrap();
        let session = store
            .start_session("Original multilingual event", None)
            .unwrap()
            .id;
        let mut r = runtime(&store);
        r.capture = Some(CaptureJob {
            session: session.clone(),
            stop: Arc::new(AtomicBool::new(false)),
            end_action: None,
        });
        assert_eq!(r.snapshot.transcription_language, "en");
        let utterance = || Utterance {
            id: "original-pt-utterance".into(),
            source: Source::Microphone,
            start_ns: 0,
            end_ns: 1_000_000_000,
            samples: vec![0.0],
            finalized: true,
        };
        let profile =
            Arc::new(models::profile(models::MULTILINGUAL_TRANSCRIPTION_PROFILE).unwrap());
        let stale = r
            .apply(
                &mut store,
                Event::Transcript(
                    other.clone(),
                    utterance(),
                    "Stale unrelated transcript".into(),
                    profile.clone(),
                    "fixture-cpu".into(),
                    "pt".into(),
                ),
            )
            .unwrap();
        assert!(!stale.0);
        assert!(store.session(&other).unwrap().transcript.is_empty());
        r.apply(
            &mut store,
            Event::Transcript(
                session.clone(),
                utterance(),
                "Original authored café text".into(),
                profile,
                "fixture-cpu".into(),
                "pt".into(),
            ),
        )
        .unwrap();
        let saved = store.session(&session).unwrap();
        assert_eq!(saved.transcript[0].text, "Original authored café text");
        let audio = saved.transcript[0].audio.as_ref().unwrap();
        assert_eq!(audio.language, "pt");
        assert_eq!(
            audio.model_profile,
            models::MULTILINGUAL_TRANSCRIPTION_PROFILE
        );
        assert_eq!(audio.backend.as_deref(), Some("fixture-cpu"));
        assert_eq!(store.settings().unwrap().transcription_language, "en");
    }

    #[test]
    fn incompatible_language_is_rejected_before_capture_starts() {
        let mut store = Store::open_in_memory().unwrap();
        let session = store
            .start_session("Explicit language required", None)
            .unwrap()
            .id;
        let mut settings = store.settings().unwrap();
        settings.transcription_language = "pt".into();
        store.update_settings(settings).unwrap();
        let mut r = runtime(&store);
        r.fixture = false;
        r.snapshot
            .installed_profiles
            .push(models::TRANSCRIPTION_PROFILE.into());
        let error = r.start_capture(&store, &session).unwrap_err();
        assert!(error.to_string().contains("English-only"));
        assert!(r.capture.is_none());
        assert!(r.snapshot.capture.is_none());
        assert_eq!(store.settings().unwrap().transcription_language, "pt");
        assert_eq!(
            store.session(&session).unwrap().status,
            SessionStatus::Recording
        );
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
            Event::ModelDone(
                "old-install".into(),
                PathBuf::from("obsolete-root"),
                Ok(ModelChange::Installed(models::GENERATION_PROFILE.into())),
            ),
        )
        .unwrap();
        assert!(r.snapshot.installed_profiles.is_empty());
    }
    #[test]
    fn a_stale_verification_cannot_resurrect_a_removed_model() {
        let mut store = Store::open_in_memory().unwrap();
        let mut r = runtime(&store);
        r.snapshot.installed_profiles = vec![
            models::GENERATION_PROFILE.into(),
            models::TRANSCRIPTION_PROFILE.into(),
        ];
        r.install_id = Some("remove-job".into());
        r.model_epoch = "new-files".into();
        r.apply(
            &mut store,
            Event::ModelDone(
                "remove-job".into(),
                r.snapshot.model_root.clone(),
                Ok(ModelChange::Removed(models::GENERATION_PROFILE.into())),
            ),
        )
        .unwrap();
        r.apply(
            &mut store,
            Event::Models(
                "old-files".into(),
                r.snapshot.model_root.clone(),
                vec![models::GENERATION_PROFILE.into()],
            ),
        )
        .unwrap();
        assert_eq!(
            r.snapshot.installed_profiles,
            vec![models::TRANSCRIPTION_PROFILE]
        );
    }
    #[test]
    fn failed_model_changes_reset_the_cache_before_completion_and_need_fresh_verification() {
        let mut store = Store::open_in_memory().unwrap();
        let mut r = runtime(&store);
        r.fixture = false;
        let (commands, pending) = mpsc::sync_channel(1);
        r.generator = commands;
        let missing = std::env::temp_dir().join(format!("cueglint-missing-model-{}", uuid()));
        r.snapshot.model_root = missing.join("destination");
        r.snapshot.installed_profiles = vec![models::GENERATION_PROFILE.into()];
        r.import_model(models::GENERATION_PROFILE.into(), missing.join("source"))
            .unwrap();
        assert!(r.snapshot.installed_profiles.is_empty());
        let event = r.events.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(matches!(pending.try_recv(), Ok(InferenceCommand::Reset)));
        let (_, notice) = r.apply(&mut store, event).unwrap();
        assert!(notice.is_some());
        assert!(r.ensure_answer_ready("Question").is_err());
        assert!(!missing.exists());
    }
    #[test]
    fn model_file_changes_are_blocked_before_touching_disk_during_capture_or_generation() {
        let store = Store::open_in_memory().unwrap();
        let mut r = runtime(&store);
        assert!(r.remove_model(models::GENERATION_PROFILE.into()).is_err());
        r.fixture = false;
        r.capture = Some(CaptureJob {
            session: "fixture".into(),
            stop: Arc::new(AtomicBool::new(false)),
            end_action: None,
        });
        assert!(
            r.remove_model(models::GENERATION_PROFILE.into())
                .unwrap_err()
                .to_string()
                .contains("stop audio")
        );
        r.capture = None;
        r.snapshot.generation = Some(GenerationStatus {
            session_id: "fixture".into(),
            request_id: "in-flight".into(),
            state: "generating".into(),
            text: String::new(),
        });
        assert!(
            r.import_model(models::GENERATION_PROFILE.into(), PathBuf::from("not-read"))
                .unwrap_err()
                .to_string()
                .contains("cancel")
        );
        assert!(r.install_id.is_none());
    }
    #[test]
    fn missing_or_changing_models_cannot_start_a_request_or_capture() {
        let mut store = Store::open_in_memory().unwrap();
        let session = store.start_session("Removed model", None).unwrap().id;
        let mut r = runtime(&store);
        r.fixture = false;
        assert!(
            r.ask(&mut store, &session, "Question".into())
                .unwrap_err()
                .to_string()
                .contains("not verified")
        );
        r.snapshot.installed_profiles = vec![
            models::GENERATION_PROFILE.into(),
            models::TRANSCRIPTION_PROFILE.into(),
        ];
        r.install_id = Some("model-files-changing".into());
        assert!(
            r.ask(&mut store, &session, "Question".into())
                .unwrap_err()
                .to_string()
                .contains("operation to finish")
        );
        assert!(
            r.start_capture(&store, &session)
                .unwrap_err()
                .to_string()
                .contains("operation to finish")
        );
        assert!(store.session(&session).unwrap().requests.is_empty());
        assert!(r.capture.is_none());
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
    fn selecting_another_model_cancels_old_work_before_accepting_results() {
        let mut store = Store::open_in_memory().unwrap();
        let session = store.start_session("Model replacement", None).unwrap().id;
        let request = store.begin_request(&session, "Question").unwrap();
        let mut r = runtime(&store);
        let cancelled = Arc::new(AtomicBool::new(false));
        r.generation_cancel = Some(cancelled.clone());
        r.snapshot.generation = Some(GenerationStatus {
            session_id: session.clone(),
            request_id: request.id.clone(),
            state: "generating".into(),
            text: "old model".into(),
        });
        let mut settings = store.settings().unwrap();
        settings.generation_profile = Some("qwen2.5-0.5b-instruct".into());
        store.update_settings(settings).unwrap();
        r.settings_changed(&mut store).unwrap();
        assert!(cancelled.load(Ordering::Relaxed));
        assert_eq!(r.snapshot.generation_profile, "qwen2.5-0.5b-instruct");
        assert!(
            !store
                .complete_request(&session, &request.id, "stale model answer")
                .unwrap()
        );
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
            prompt_sha256: "0".repeat(64),
            template_revision: cueglint_local::generation::PROMPT_TEMPLATE_REVISION.into(),
            decoder_policy_revision: None,
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
            source: ScreenSource::Display(1),
            captured_at: 0,
            width: 1,
            height: 1,
            sha256: "0".repeat(64),
            pixel_sha256: None,
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
            })
        });
        stopped_rx
            .recv_timeout(Duration::from_millis(500))
            .expect("stop control was blocked by full queue");
        rx.recv_timeout(Duration::from_millis(500)).unwrap();
        let final_audio = rx.recv_timeout(Duration::from_millis(500)).unwrap();
        assert_eq!(final_audio.start_ns, 10);
        thread.join().unwrap().unwrap();
    }
}
