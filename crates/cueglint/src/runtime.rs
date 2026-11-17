use anyhow::{Context, Result, ensure};
use cueglint_core::{
    AudioMetadata, GenerationRequest, ScreenContext, SessionStatus, Speaker, Store,
    TranscriptSource, TranscriptUpdate,
    provider::{CredentialStatus, ProviderConfig},
};
use cueglint_local::{
    asr::Transcriber,
    audio::{Capture, Endpoint, Source, Utterance},
    generation::{GenerationOptions, GenerationOutput, LocalGenerator},
    models::{self, DownloadProgress},
    permissions, screen,
};
use cueglint_providers::{
    CredentialStore, NativeCredentialStore, ProviderClient, ProviderOutput, Secret, StreamEvent,
    load_for_config,
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
    pub permissions: permissions::Permissions,
    pub generation_profile: String,
    pub transcription_profile: String,
    pub answer_provider: ProviderConfig,
    pub provider: Option<ProviderStatus>,
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
    profile: String,
    cancel: Arc<AtomicBool>,
}
enum InferenceCommand {
    Generate(GenerateJob),
    Provider(ProviderJob),
    Reset,
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
    Models(String, PathBuf, Vec<String>),
    InstallProgress(String, DownloadProgress),
    ModelDone(String, PathBuf, Result<ModelChange, String>),
    Screen(String, String, String, Result<ScreenContext, String>),
    ScreenPreview(Result<ScreenContext, String>),
    GenerationText(String, String),
    GenerationDone(String, Result<GenerationOutput, String>),
    HostedDone(String, Result<ProviderOutput, String>),
    ProviderDone(String, ProviderConfig, Result<ProviderReport, String>),
    CaptureStarted(String),
    CaptureMetrics(String, u64, u64),
    Transcript(String, Utterance, String, String, String),
    CaptureError(String, String),
    CaptureEnded(String),
}

pub struct Runtime {
    pub snapshot: RuntimeSnapshot,
    events: Receiver<Event>,
    tx: SyncSender<Event>,
    generator: SyncSender<InferenceCommand>,
    ctx: egui::Context,
    generation_cancel: Option<Arc<AtomicBool>>,
    install_cancel: Option<Arc<AtomicBool>>,
    install_id: Option<String>,
    capture: Option<CaptureJob>,
    fixture: bool,
    screen_busy: Arc<AtomicBool>,
    verification_busy: Arc<AtomicBool>,
    model_epoch: String,
    provider_job: Option<String>,
    provider_cancel: Option<Arc<AtomicBool>>,
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
        Self::with_credentials(store, fixture, ctx, Box::new(NativeCredentialStore))
    }

    fn with_credentials(
        store: &Store,
        fixture: bool,
        ctx: egui::Context,
        credentials: Box<dyn CredentialStore>,
    ) -> Result<Self> {
        let (tx, events) = mpsc::sync_channel(128);
        let (generator, jobs) = mpsc::sync_channel::<InferenceCommand>(2);
        let generate_tx = tx.clone();
        let generate_ctx = ctx.clone();
        thread::Builder::new()
            .name("cueglint-inference".into())
            .spawn(move || {
                let mut model: Option<(PathBuf, String, LocalGenerator)> = None;
                while let Ok(command) = jobs.recv() {
                    let job = match command {
                        InferenceCommand::Generate(job) => job,
                        InferenceCommand::Provider(job) => {
                            let id = job.id.clone();
                            let config = job.config.clone();
                            let result = provider_operation(&*credentials, job)
                                .map_err(|error| error.to_string());
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
                            continue;
                        }
                    };
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
                permissions: if fixture {
                    Default::default()
                } else {
                    permissions::status()
                },
                generation_profile: store
                    .settings()?
                    .generation_profile
                    .unwrap_or_else(|| models::GENERATION_PROFILE.into()),
                transcription_profile: store
                    .settings()?
                    .transcription_profile
                    .unwrap_or_else(|| models::TRANSCRIPTION_PROFILE.into()),
                answer_provider: store.settings()?.answer_provider,
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
            model_epoch: uuid(),
            provider_job: None,
            provider_cancel: None,
        };
        runtime.refresh_models();
        Ok(runtime)
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
        thread::spawn(move || {
            let installed = models::profiles()
                .unwrap_or_default()
                .iter()
                .filter(|p| models::verify(&root, &p.id).is_ok())
                .map(|p| p.id.clone())
                .collect();
            busy.store(false, Ordering::Release);
            let _ = tx.send(Event::Models(epoch, root, installed));
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
            self.model_epoch = uuid();
            self.snapshot.installed_profiles.clear();
            self.refresh_models();
        }
        let settings = store.settings()?;
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
        }
        self.snapshot.generation_profile = generation;
        self.snapshot.transcription_profile = transcription;
        self.snapshot.screen_enabled = settings.screen_enabled;
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
            .try_send(InferenceCommand::Provider(ProviderJob {
                id: id.clone(),
                config: config.clone(),
                action,
                cancel: cancel.clone(),
            }))
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
            .try_send(InferenceCommand::Generate(GenerateJob {
                request,
                root: self.snapshot.model_root.clone(),
                profile: self.snapshot.generation_profile.clone(),
                cancel,
            }))
            .is_err()
        {
            store.cancel_request(&session, &id)?;
            self.snapshot.generation.as_mut().unwrap().state = "error".into();
            self.generation_cancel = None;
            anyhow::bail!("inference queue is full; cancel and retry");
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
            self.install_id.is_none(),
            "wait for the model file operation to finish before listening"
        );
        ensure!(
            self.snapshot
                .installed_profiles
                .iter()
                .any(|p| p == &self.snapshot.transcription_profile),
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
            Event::Transcript(id, utterance, text, profile_id, backend)
                if self.capture.as_ref().is_some_and(|c| c.session == id) =>
            {
                if !text.trim().is_empty() {
                    let profile = models::profile(&profile_id)?;
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
    let profile_id = settings
        .transcription_profile
        .as_deref()
        .unwrap_or(models::TRANSCRIPTION_PROFILE);
    let installed = models::verify(root, profile_id)?;
    let profile_id = installed.profile.id.clone();
    let mut transcriber = Transcriber::load(&installed.directory)?;
    if stop.load(Ordering::Relaxed) {
        return Ok(());
    }
    if settings.microphone_enabled {
        permissions::ensure_microphone(&stop)?;
    }
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
                    .send(Event::Transcript(
                        session.to_owned(),
                        utterance,
                        text,
                        profile_id.clone(),
                        transcriber.backend().to_owned(),
                    ))
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
#[path = "runtime_provider_tests.rs"]
pub(crate) mod provider_tests;

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
            Event::Transcript(
                id.clone(),
                utterance,
                "Last utterance".into(),
                "whisper-tiny-en".into(),
                "fixture".into(),
            ),
        )
        .unwrap();
        r.apply(&mut store, Event::CaptureEnded(id.clone()))
            .unwrap();
        let saved = store.session(&id).unwrap();
        assert_eq!(saved.status, SessionStatus::Stopped);
        assert!(saved.saved_at.is_some());
        assert_eq!(saved.transcript[0].text, "Last utterance");
        assert_eq!(
            saved.transcript[0].audio.as_ref().unwrap().model_profile,
            "whisper-tiny-en"
        );
        let audio = saved.transcript[0].audio.as_ref().unwrap();
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
