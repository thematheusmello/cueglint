use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand, ValueEnum};
use cueglint_core::{
    ExportFormat, MAX_BACKUP_BYTES, MAX_DOCUMENT_BYTES, Mode, RequestStatus, Speaker, Store,
    TranscriptSource, TranscriptUpdate,
    provider::{ProviderConfig, ProviderId},
};
use cueglint_local::{
    generation::{FinishReason, GenerationOptions, LocalGenerator},
    models,
};
use cueglint_providers::{
    CredentialStore, NativeCredentialStore, ProviderClient, Secret, StreamEvent, load_for_config,
};
use serde::Serialize;
use std::{
    fs::File,
    io::{IsTerminal, Read, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Parser)]
#[command(
    version,
    about = "Native Cueglint sessions, local models and explicit answer providers"
)]
struct Cli {
    /// Database path. Defaults to platform app data or CUEGLINT_DATA_DIR.
    #[arg(long, global = true)]
    db: Option<PathBuf>,
    /// Root directory for the pinned local model profiles.
    #[arg(long, global = true)]
    models_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start, manage and search persistent sessions.
    Session {
        #[command(subcommand)]
        command: SessionCommand,
    },
    /// Add a manual transcript or revise a stable partial segment.
    Transcript {
        session: String,
        text: String,
        #[arg(long)]
        segment: Option<String>,
        #[arg(long, value_enum, default_value = "you")]
        speaker: CliSpeaker,
        #[arg(long, value_enum, default_value = "manual")]
        source: CliSource,
        /// Emit a transient hypothesis for this process; it is never saved.
        #[arg(long)]
        partial: bool,
    },
    /// Manage local prompt modes.
    Mode {
        #[command(subcommand)]
        command: ModeCommand,
    },
    /// Ingest UTF-8 text and inspect bounded lexical retrieval.
    Knowledge {
        #[command(subcommand)]
        command: KnowledgeCommand,
    },
    /// Inspect frozen context and cancellation without running inference.
    Request {
        #[command(subcommand)]
        command: RequestCommand,
    },
    Settings {
        #[command(subcommand)]
        command: SettingsCommand,
    },
    /// Verify the active pinned generation and transcription profiles.
    ModelStatus,
    /// List, explicitly download, or hash-verify pinned local model profiles.
    Models {
        #[command(subcommand)]
        command: ModelsCommand,
    },
    /// Select an answer provider, manage its native key, or explicitly connect.
    Provider {
        #[command(subcommand)]
        command: ProviderCommand,
    },
    /// Generate and persist an answer using the deliberately selected provider.
    Ask {
        session: String,
        prompt: String,
        #[arg(long, default_value_t = 128)]
        max_tokens: usize,
        /// Override a local generation profile. Rejects use with a remote provider.
        #[arg(long)]
        profile: Option<String>,
        /// Send decoded text deltas to stderr while keeping the final JSON on stdout.
        #[arg(long)]
        stream: bool,
    },
    /// Transcribe a mono 16kHz PCM16 WAV locally (English, at most 30 seconds).
    Transcribe {
        wav: PathBuf,
        /// Override the stored transcription selection or measured default profile.
        #[arg(long)]
        profile: Option<String>,
    },
    /// Export one session as Markdown or JSON.
    Export {
        session: String,
        #[arg(long, value_enum, default_value = "markdown")]
        format: CliFormat,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long, requires = "output")]
        force: bool,
    },
    /// Export a versioned logical backup. Output files are private and created atomically.
    Backup {
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long, requires = "output")]
        force: bool,
    },
    /// Validate a backup before atomically restoring it.
    Restore {
        file: PathBuf,
        /// Replace existing data. Save a backup first if it is still needed.
        #[arg(long)]
        replace: bool,
    },
}

#[derive(Subcommand)]
enum SessionCommand {
    /// Create a session for manual or externally supplied transcript updates.
    Start {
        #[arg(default_value = "Untitled session")]
        title: String,
        #[arg(long)]
        mode: Option<String>,
    },
    List {
        #[arg(default_value = "")]
        query: String,
    },
    Show {
        id: String,
    },
    Pause {
        id: String,
    },
    Resume {
        id: String,
    },
    Mute {
        id: String,
        #[arg(action = clap::ArgAction::Set)]
        muted: bool,
    },
    Stop {
        id: String,
    },
    Save {
        id: String,
    },
    Rename {
        id: String,
        title: String,
    },
    Notes {
        id: String,
        text: String,
    },
    Summary {
        id: String,
        text: String,
    },
    SetMode {
        id: String,
        mode: String,
    },
    ClearMode {
        id: String,
    },
    Delete {
        id: String,
    },
}

#[derive(Subcommand)]
enum ModeCommand {
    List,
    Show {
        id: String,
    },
    Create {
        name: String,
        instructions: String,
    },
    Update {
        id: String,
        name: String,
        instructions: String,
    },
    Delete {
        id: String,
    },
}

#[derive(Subcommand)]
enum KnowledgeCommand {
    List,
    Add {
        file: PathBuf,
        #[arg(long)]
        title: Option<String>,
    },
    Search {
        query: String,
        #[arg(long, default_value_t = 5)]
        limit: usize,
    },
    Delete {
        id: String,
    },
}

#[derive(Subcommand)]
enum RequestCommand {
    /// Freeze context for inspection, then cancel the unconnected worker request.
    Freeze {
        session: String,
        prompt: String,
    },
    Cancel {
        session: String,
        id: String,
    },
    List {
        session: String,
    },
}

#[derive(Subcommand)]
enum SettingsCommand {
    Show,
    /// Persist a canonical shortcut override, such as CommandOrControl+Shift+Backslash.
    SetShortcut {
        action: String,
        binding: String,
    },
    DisableShortcut {
        action: String,
    },
    /// Remove the override and restore the reviewed platform default.
    ResetShortcut {
        action: String,
    },
    SetModelDirectory {
        path: PathBuf,
    },
    SetGenerationProfile {
        profile: String,
    },
    SetTranscriptionProfile {
        profile: String,
    },
    Inputs {
        #[arg(long, action = clap::ArgAction::Set)]
        microphone: bool,
        #[arg(long, action = clap::ArgAction::Set)]
        system_audio: bool,
    },
}

#[derive(Subcommand)]
enum ModelsCommand {
    List,
    Install {
        profile: String,
    },
    Verify {
        profile: String,
    },
    /// Verify and copy catalog-pinned artifacts from a local folder without networking.
    Import {
        profile: String,
        directory: PathBuf,
    },
    /// Remove known artifacts; refuse unknown files and preserve the coordination lock.
    Remove {
        profile: String,
    },
}

#[derive(Subcommand)]
enum ProviderCommand {
    Show,
    /// Save a provider/model choice without sending any context or testing it.
    Select {
        #[arg(value_enum)]
        provider: CliProvider,
        #[arg(long)]
        model: Option<String>,
        /// Custom API base URL, only for the compatible provider.
        #[arg(long)]
        base_url: Option<String>,
    },
    /// Read a key from stdin into the OS credential store. Never pass keys as arguments.
    KeySave,
    KeyDelete,
    KeyStatus,
    /// Explicitly contact the selected provider for its supported model catalog.
    Models,
    /// Send only a synthetic instruction, with no session or meeting context.
    Test,
}

#[derive(Clone, Copy, ValueEnum)]
enum CliProvider {
    Local,
    Openai,
    Anthropic,
    Gemini,
    Perplexity,
    Openrouter,
    Compatible,
}

impl From<CliProvider> for ProviderId {
    fn from(provider: CliProvider) -> Self {
        match provider {
            CliProvider::Local => Self::Local,
            CliProvider::Openai => Self::OpenAi,
            CliProvider::Anthropic => Self::Anthropic,
            CliProvider::Gemini => Self::Gemini,
            CliProvider::Perplexity => Self::Perplexity,
            CliProvider::Openrouter => Self::OpenRouter,
            CliProvider::Compatible => Self::OpenAiCompatible,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum CliSpeaker {
    You,
    Other,
}
#[derive(Clone, Copy, ValueEnum)]
enum CliSource {
    Manual,
    Microphone,
    SystemAudio,
}
#[derive(Clone, Copy, ValueEnum)]
enum CliFormat {
    Json,
    Markdown,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let path = cli.db.map(Ok).unwrap_or_else(Store::default_path)?;
    let mut store = Store::open(&path)?;
    let preferences = store.settings()?;
    let generation_profile = preferences
        .generation_profile
        .as_deref()
        .unwrap_or(models::GENERATION_PROFILE);
    let transcription_profile = preferences
        .transcription_profile
        .as_deref()
        .unwrap_or(models::TRANSCRIPTION_PROFILE);
    let models_directory = cli
        .models_dir
        .or(preferences.model_directory.clone().map(PathBuf::from))
        .unwrap_or_else(|| {
            path.parent()
                .unwrap_or_else(|| Path::new("."))
                .join("models")
        });
    match cli.command {
        Command::Session { command } => match command {
            SessionCommand::Start { title, mode } => {
                print_json(&store.start_session(&title, mode.as_deref())?)?
            }
            SessionCommand::List { query } => print_json(&store.sessions(&query)?)?,
            SessionCommand::Show { id } => print_json(&store.session(&id)?)?,
            SessionCommand::Pause { id } => print_json(&store.pause_session(&id)?)?,
            SessionCommand::Resume { id } => print_json(&store.resume_session(&id)?)?,
            SessionCommand::Mute { id, muted } => print_json(&store.mute_session(&id, muted)?)?,
            SessionCommand::Stop { id } => print_json(&store.stop_session(&id)?)?,
            SessionCommand::Save { id } => print_json(&store.save_session(&id)?)?,
            SessionCommand::Rename { id, title } => {
                print_json(&store.rename_session(&id, &title)?)?
            }
            SessionCommand::Notes { id, text } => print_json(&store.update_notes(&id, &text)?)?,
            SessionCommand::Summary { id, text } => print_json(&store.update_summary(&id, &text)?)?,
            SessionCommand::SetMode { id, mode } => {
                print_json(&store.set_session_mode(&id, Some(&mode))?)?
            }
            SessionCommand::ClearMode { id } => print_json(&store.set_session_mode(&id, None)?)?,
            SessionCommand::Delete { id } => {
                store.delete_session(&id)?;
                print_json(&serde_json::json!({"deleted": id}))?;
            }
        },
        Command::Transcript {
            session,
            text,
            segment,
            speaker,
            source,
            partial,
        } => {
            let update = TranscriptUpdate {
                id: segment.unwrap_or_else(|| Uuid::new_v4().to_string()),
                text,
                finalized: !partial,
                speaker: match speaker {
                    CliSpeaker::You => Speaker::You,
                    CliSpeaker::Other => Speaker::Other,
                },
                source: match source {
                    CliSource::Manual => TranscriptSource::Manual,
                    CliSource::Microphone => TranscriptSource::Microphone,
                    CliSource::SystemAudio => TranscriptSource::SystemAudio,
                },
            };
            print_json(&store.upsert_transcript(&session, update)?)?;
        }
        Command::Mode { command } => match command {
            ModeCommand::List => print_json(&store.modes()?)?,
            ModeCommand::Show { id } => print_json(&store.mode(&id)?)?,
            ModeCommand::Create { name, instructions } => {
                print_json(&store.upsert_mode(Mode::new(name, instructions))?)?
            }
            ModeCommand::Update {
                id,
                name,
                instructions,
            } => print_json(&store.upsert_mode(Mode {
                id,
                name,
                instructions,
                builtin: false,
            })?)?,
            ModeCommand::Delete { id } => {
                store.delete_mode(&id)?;
                print_json(&serde_json::json!({"deleted": id}))?;
            }
        },
        Command::Knowledge { command } => match command {
            KnowledgeCommand::List => print_json(&store.knowledge()?)?,
            KnowledgeCommand::Add { file, title } => {
                let text = read_bounded_text(&file, MAX_DOCUMENT_BYTES)?;
                let title = title.unwrap_or_else(|| {
                    file.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                });
                print_json(&store.ingest_knowledge(&title, &file.to_string_lossy(), &text)?)?;
            }
            KnowledgeCommand::Search { query, limit } => {
                print_json(&store.search_knowledge(&query, limit)?)?
            }
            KnowledgeCommand::Delete { id } => {
                store.delete_knowledge(&id)?;
                print_json(&serde_json::json!({"deleted": id}))?;
            }
        },
        Command::Request { command } => match command {
            RequestCommand::Freeze { session, prompt } => {
                let mut request = store.begin_request(&session, &prompt)?;
                store.cancel_request(&session, &request.id)?;
                request.status = RequestStatus::Cancelled;
                print_json(
                    &serde_json::json!({"inference_executed": false, "request": request,
                    "message": "Context prepared only; no answer was generated. Use ask to run the selected answer provider."}),
                )?;
            }
            RequestCommand::Cancel { session, id } => {
                print_json(&store.cancel_request(&session, &id)?)?
            }
            RequestCommand::List { session } => print_json(&store.session(&session)?.requests)?,
        },
        Command::Settings { command } => {
            let mut settings = store.settings()?;
            let changed = !matches!(&command, SettingsCommand::Show);
            match command {
                SettingsCommand::Show => {}
                SettingsCommand::SetShortcut { action, binding } => {
                    settings.shortcut_bindings.insert(action, Some(binding));
                }
                SettingsCommand::DisableShortcut { action } => {
                    settings.shortcut_bindings.insert(action, None);
                }
                SettingsCommand::ResetShortcut { action } => {
                    ensure!(
                        cueglint_core::SHORTCUT_ACTIONS.contains(&action.as_str()),
                        "unknown shortcut action: {action}"
                    );
                    settings.shortcut_bindings.remove(&action);
                }
                SettingsCommand::SetModelDirectory { path } => {
                    settings.model_directory = Some(
                        path.to_str()
                            .context("model directory is not UTF-8")?
                            .to_owned(),
                    )
                }
                SettingsCommand::SetGenerationProfile { profile } => {
                    ensure!(
                        models::profile(&profile)?.purpose == "generation",
                        "selected profile is not a text-generation model"
                    );
                    settings.generation_profile = Some(profile);
                }
                SettingsCommand::SetTranscriptionProfile { profile } => {
                    ensure!(
                        models::profile(&profile)?.purpose == "transcription",
                        "selected profile is not a transcription model"
                    );
                    settings.transcription_profile = Some(profile);
                }
                SettingsCommand::Inputs {
                    microphone,
                    system_audio,
                } => {
                    settings.microphone_enabled = microphone;
                    settings.system_audio_enabled = system_audio;
                }
            }
            if changed {
                print_json(&store.update_settings(settings)?)?;
            } else {
                print_json(&settings)?;
            }
        }
        Command::ModelStatus => {
            let status = |profile: &str, purpose: &str| match models::profile(profile).and_then(
                |configured| {
                    ensure!(
                        configured.purpose == purpose,
                        "profile purpose does not match the configured engine"
                    );
                    models::verify(&models_directory, profile)
                },
            ) {
                Ok(installed) => {
                    serde_json::json!({"state": "verified", "profile": installed.profile})
                }
                Err(error) => {
                    serde_json::json!({"state": "unavailable", "profile_id": profile, "error": error.to_string()})
                }
            };
            let model = status(generation_profile, "generation");
            let transcription = status(transcription_profile, "transcription");
            print_json(
                &serde_json::json!({"inference_available": if preferences.answer_provider.provider.is_local() { serde_json::json!(model["state"] == "verified") } else { serde_json::Value::Null },
                "local_inference_available": model["state"] == "verified", "answer_provider": preferences.answer_provider,
                "remote_availability": "not_tested",
                "transcription_available": transcription["state"] == "verified", "model": model,
                "transcription": transcription, "models_directory": models_directory}),
            )?;
        }
        Command::Models { command } => {
            let mut last_progress = (String::new(), 101);
            let mut progress = |progress: models::DownloadProgress| {
                let percent = progress.downloaded_bytes * 100 / progress.total_bytes;
                if last_progress != (progress.file.clone(), percent) {
                    let _ = writeln!(
                        std::io::stderr().lock(),
                        "{}: {percent}% ({}/{})",
                        progress.file,
                        progress.downloaded_bytes,
                        progress.total_bytes
                    );
                    last_progress = (progress.file, percent);
                }
            };
            match command {
                ModelsCommand::List => print_json(&models::profiles()?)?,
                ModelsCommand::Verify { profile } => {
                    print_json(&models::verify(&models_directory, &profile)?)?
                }
                ModelsCommand::Install { profile } => {
                    print_json(&models::install(
                        &models_directory,
                        &profile,
                        &std::sync::atomic::AtomicBool::new(false),
                        &mut progress,
                    )?)?;
                }
                ModelsCommand::Import { profile, directory } => {
                    print_json(&models::import(
                        &models_directory,
                        &profile,
                        &directory,
                        &std::sync::atomic::AtomicBool::new(false),
                        &mut progress,
                    )?)?;
                }
                ModelsCommand::Remove { profile } => {
                    models::remove(&models_directory, &profile)?;
                    print_json(&serde_json::json!({"removed": profile}))?;
                }
            }
        }
        Command::Provider { command } => {
            ensure!(
                !matches!(&command, ProviderCommand::KeySave) || !std::io::stdin().is_terminal(),
                "pipe the API key from your password manager to stdin, or use the app's password field; interactive terminal input would echo the key"
            );
            let result = provider_command(
                &mut store,
                command,
                &NativeCredentialStore,
                &mut std::io::stdin().lock(),
            )?;
            print_json(&result)?;
        }
        Command::Ask {
            session,
            prompt,
            max_tokens,
            profile,
            stream,
        } => {
            ensure!(
                (1..=1024).contains(&max_tokens),
                "--max-tokens must be between 1 and 1024"
            );
            let cancel = std::sync::atomic::AtomicBool::new(false);
            if !preferences.answer_provider.provider.is_local() {
                ensure!(
                    profile.is_none(),
                    "--profile applies only to local answers; select the local provider explicitly first"
                );
                preferences.answer_provider.validate_for_generation()?;
                let client = ProviderClient::new(preferences.answer_provider.clone())?;
                let request = store.begin_request(&session, &prompt)?;
                let generated = client.generate_with_store(
                    &request,
                    max_tokens as u32,
                    &NativeCredentialStore,
                    &cancel,
                    |event| {
                        if stream && let StreamEvent::TextDelta(text) = event {
                            let mut stderr = std::io::stderr().lock();
                            let _ = write!(stderr, "{text}");
                            let _ = stderr.flush();
                        }
                    },
                );
                let output = match generated {
                    Ok(output) => output,
                    Err(error) => {
                        store.cancel_request(&session, &request.id)?;
                        return Err(error.into());
                    }
                };
                if stream {
                    let _ = writeln!(std::io::stderr().lock());
                }
                let applied = if output.text.trim().is_empty() {
                    store.cancel_request(&session, &request.id)?;
                    false
                } else {
                    store.complete_request_with_hosted_metadata(
                        &session,
                        &request.id,
                        &output.text,
                        output.metadata.clone(),
                    )?
                };
                return print_json(
                    &serde_json::json!({"request_id": request.id, "session_id": session, "applied": applied, "output": output}),
                );
            }
            let mut generator = LocalGenerator::load_profile(
                &models_directory,
                profile.as_deref().unwrap_or(generation_profile),
            )?;
            let request = store.begin_request(&session, &prompt)?;
            let generated = generator.generate(
                &request,
                GenerationOptions {
                    max_new_tokens: max_tokens,
                },
                &cancel,
                |text| {
                    if stream {
                        let mut stderr = std::io::stderr().lock();
                        let _ = write!(stderr, "{text}");
                        let _ = stderr.flush();
                    }
                },
            );
            let output = match generated {
                Ok(output) => output,
                Err(error) => {
                    store.cancel_request(&session, &request.id)?;
                    return Err(error);
                }
            };
            if stream {
                let _ = writeln!(std::io::stderr().lock());
            }
            let applied = if output.finish_reason == FinishReason::Cancelled
                || output.text.trim().is_empty()
            {
                store.cancel_request(&session, &request.id)?;
                false
            } else {
                store.complete_request_with_metadata(
                    &session,
                    &request.id,
                    &output.text,
                    output.metadata(),
                )?
            };
            print_json(
                &serde_json::json!({"request_id": request.id, "session_id": session, "applied": applied, "output": output}),
            )?;
        }
        Command::Transcribe { wav, profile } => {
            let profile = profile.as_deref().unwrap_or(transcription_profile);
            let samples = cueglint_local::audio::load_wav(&wav)?;
            ensure!(
                models::profile(profile)?.purpose == "transcription",
                "selected profile is not a transcription model"
            );
            let installed = models::verify(&models_directory, profile)?;
            let started = std::time::Instant::now();
            let mut transcriber = cueglint_local::asr::Transcriber::load(&installed.directory)?;
            let text =
                transcriber.transcribe(&samples, &std::sync::atomic::AtomicBool::new(false))?;
            print_json(
                &serde_json::json!({"text": text, "language": "en", "sample_rate": 16000,
                "sample_count": samples.len(), "elapsed_ms": started.elapsed().as_millis(), "model": installed.profile,
                "backend": transcriber.backend(), "frontend_revision": cueglint_local::asr_frontend::REVISION,
                "frontend_config_sha256": cueglint_local::asr_frontend::configuration_sha256()}),
            )?;
        }
        Command::Export {
            session,
            format,
            output,
            force,
        } => {
            ensure_export_not_database(output.as_deref(), &path)?;
            emit(
                &store.export_session(
                    &session,
                    match format {
                        CliFormat::Json => ExportFormat::Json,
                        CliFormat::Markdown => ExportFormat::Markdown,
                    },
                )?,
                output.as_deref(),
                force,
            )?;
        }
        Command::Backup { output, force } => {
            ensure_export_not_database(output.as_deref(), &path)?;
            emit(&store.backup_json()?, output.as_deref(), force)?;
        }
        Command::Restore { file, replace } => {
            let text = read_bounded_text(&file, MAX_BACKUP_BYTES)?;
            store.restore_backup(&text, replace)?;
            print_json(&serde_json::json!({"restored": true}))?;
        }
    }
    Ok(())
}

fn print_json(value: &impl Serialize) -> Result<()> {
    writeln!(
        std::io::stdout().lock(),
        "{}",
        serde_json::to_string_pretty(value)?
    )?;
    Ok(())
}

fn provider_command(
    store: &mut Store,
    command: ProviderCommand,
    credentials: &dyn CredentialStore,
    key_input: &mut impl Read,
) -> Result<serde_json::Value> {
    let config = store.settings()?.answer_provider;
    let cancel = std::sync::atomic::AtomicBool::new(false);
    match command {
        ProviderCommand::Show => Ok(serde_json::to_value(config)?),
        ProviderCommand::Select {
            provider,
            model,
            base_url,
        } => {
            let mut settings = store.settings()?;
            settings.answer_provider = ProviderConfig {
                provider: provider.into(),
                model_id: model.unwrap_or_default(),
                base_url,
            };
            Ok(serde_json::to_value(
                store.update_settings(settings)?.answer_provider,
            )?)
        }
        ProviderCommand::KeySave => {
            let reference = config.credential_ref()?;
            let mut value = Zeroizing::new(String::new());
            key_input
                .take(8195)
                .read_to_string(&mut value)
                .context("API key input must be bounded UTF-8 text")?;
            if value.ends_with('\n') {
                value.pop();
                if value.ends_with('\r') {
                    value.pop();
                }
            }
            let secret = Secret::new(std::mem::take(&mut *value))?;
            credentials.store(&reference, secret)?;
            Ok(
                serde_json::json!({"credential": credentials.status(&reference), "answer_provider_unchanged": true}),
            )
        }
        ProviderCommand::KeyDelete => {
            let reference = config.credential_ref()?;
            credentials.delete(&reference)?;
            Ok(
                serde_json::json!({"credential": credentials.status(&reference), "answer_provider_unchanged": true}),
            )
        }
        ProviderCommand::KeyStatus => Ok(serde_json::to_value(
            credentials.status(&config.credential_ref()?),
        )?),
        ProviderCommand::Models => {
            let key = load_for_config(credentials, &config)?;
            Ok(serde_json::to_value(
                ProviderClient::new(config)?.list_models(key.as_ref(), &cancel)?,
            )?)
        }
        ProviderCommand::Test => {
            let key = load_for_config(credentials, &config)?;
            let metadata = ProviderClient::new(config)?.test_connection(key.as_ref(), &cancel)?;
            Ok(
                serde_json::json!({"connected": true, "meeting_context_sent": false, "metadata": metadata}),
            )
        }
    }
}

fn read_bounded_text(path: &Path, max_bytes: usize) -> Result<String> {
    let mut bytes = Vec::new();
    File::open(path)
        .with_context(|| format!("open {}", path.display()))?
        .take(max_bytes as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= max_bytes,
        "{} exceeds {max_bytes} bytes",
        path.display()
    );
    String::from_utf8(bytes).with_context(|| format!("{} must be UTF-8 text", path.display()))
}

fn emit(text: &str, output: Option<&Path>, force: bool) -> Result<()> {
    let Some(path) = output else {
        writeln!(std::io::stdout().lock(), "{text}")?;
        return Ok(());
    };
    cueglint_core::files::write_private_export(path, text, force)
}

fn ensure_export_not_database(output: Option<&Path>, database: &Path) -> Result<()> {
    let Some(output) = output else {
        return Ok(());
    };
    let output_path = if output.exists() {
        output.canonicalize()?
    } else {
        let parent = output
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        parent
            .canonicalize()?
            .join(output.file_name().context("output path needs a filename")?)
    };
    let database_path = database.canonicalize()?;
    for suffix in ["", ".lock", "-wal", "-shm", "-journal"] {
        let mut reserved = database_path.as_os_str().to_os_string();
        reserved.push(suffix);
        let reserved = Path::new(&reserved);
        ensure!(
            output_path != reserved,
            "an export cannot overwrite the active database or its sidecar files"
        );
        if output.try_exists()? && reserved.try_exists()? {
            ensure!(
                !same_file::is_same_file(output, reserved)?,
                "an export cannot overwrite a hard link to the active database or its sidecar files"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct MemoryCredentials(std::cell::Cell<bool>);
    impl CredentialStore for MemoryCredentials {
        fn status(&self, _: &cueglint_core::CredentialRef) -> cueglint_core::CredentialStatus {
            if self.0.get() {
                cueglint_core::CredentialStatus::Stored
            } else {
                cueglint_core::CredentialStatus::Missing
            }
        }
        fn store(
            &self,
            _: &cueglint_core::CredentialRef,
            _: Secret,
        ) -> cueglint_providers::ProviderResult<()> {
            self.0.set(true);
            Ok(())
        }
        fn delete(
            &self,
            _: &cueglint_core::CredentialRef,
        ) -> cueglint_providers::ProviderResult<()> {
            self.0.set(false);
            Ok(())
        }
        fn load(
            &self,
            _: &cueglint_core::CredentialRef,
        ) -> cueglint_providers::ProviderResult<Option<Secret>> {
            if self.0.get() {
                Secret::new("cueglint-synthetic-cli-key".into()).map(Some)
            } else {
                Ok(None)
            }
        }
    }

    fn parsed_provider(args: &[&str]) -> ProviderCommand {
        let Command::Provider { command } =
            Cli::try_parse_from(std::iter::once("cueglint-cli").chain(args.iter().copied()))
                .unwrap()
                .command
        else {
            panic!("provider command expected")
        };
        command
    }

    #[test]
    fn cli_key_input_uses_stdin_and_native_status_without_serializing_or_changing_selection() {
        let mut store = Store::open_in_memory().unwrap();
        let credentials = MemoryCredentials::default();
        let mut empty = std::io::empty();
        assert_eq!(
            provider_command(
                &mut store,
                parsed_provider(&["provider", "show"]),
                &credentials,
                &mut empty
            )
            .unwrap()["provider"],
            "local"
        );
        let selected = provider_command(
            &mut store,
            parsed_provider(&["provider", "select", "openai", "--model", "original-model"]),
            &credentials,
            &mut empty,
        )
        .unwrap();
        let mut key = std::io::Cursor::new(b"cueglint-synthetic-cli-key\r\n");
        let saved = provider_command(
            &mut store,
            parsed_provider(&["provider", "key-save"]),
            &credentials,
            &mut key,
        )
        .unwrap();
        assert_eq!(saved["credential"]["state"], "stored");
        assert_eq!(saved["answer_provider_unchanged"], true);
        assert_eq!(
            serde_json::to_value(store.settings().unwrap().answer_provider).unwrap(),
            selected
        );
        for text in [
            saved.to_string(),
            store.backup_json().unwrap(),
            serde_json::to_string(&store.settings().unwrap()).unwrap(),
        ] {
            assert!(!text.contains("cueglint-synthetic-cli-key"));
        }
        let deleted = provider_command(
            &mut store,
            parsed_provider(&["provider", "key-delete"]),
            &credentials,
            &mut empty,
        )
        .unwrap();
        assert_eq!(deleted["credential"]["state"], "missing");
        assert_eq!(
            serde_json::to_value(store.settings().unwrap().answer_provider).unwrap(),
            selected
        );
        assert!(
            Cli::try_parse_from([
                "cueglint-cli",
                "provider",
                "key-save",
                "cueglint-synthetic-cli-key"
            ])
            .is_err()
        );
    }

    #[test]
    fn cli_key_validation_and_bad_endpoints_preserve_the_saved_configuration() {
        let mut store = Store::open_in_memory().unwrap();
        let credentials = MemoryCredentials::default();
        let mut empty = std::io::empty();
        assert!(
            provider_command(
                &mut store,
                parsed_provider(&[
                    "provider",
                    "select",
                    "compatible",
                    "--model",
                    "original-model",
                    "--base-url",
                    "http://untrusted.example/v1"
                ]),
                &credentials,
                &mut empty
            )
            .is_err()
        );
        assert!(
            store
                .settings()
                .unwrap()
                .answer_provider
                .provider
                .is_local()
        );
        provider_command(
            &mut store,
            parsed_provider(&["provider", "select", "openai", "--model", "original-model"]),
            &credentials,
            &mut empty,
        )
        .unwrap();
        for input in [
            String::new(),
            "key with spaces".into(),
            "x".repeat(8193),
            "key\nsecond-line".into(),
        ] {
            assert!(
                provider_command(
                    &mut store,
                    parsed_provider(&["provider", "key-save"]),
                    &credentials,
                    &mut std::io::Cursor::new(input)
                )
                .is_err()
            );
            assert!(!credentials.0.get());
        }
        assert!(
            provider_command(
                &mut store,
                parsed_provider(&["provider", "models"]),
                &credentials,
                &mut empty
            )
            .is_err(),
            "missing-key model listing must fail before networking"
        );
        assert_eq!(
            store.settings().unwrap().answer_provider.provider,
            ProviderId::OpenAi
        );
    }

    #[test]
    fn exports_do_not_clobber_existing_files_or_leave_temporary_files() -> Result<()> {
        let directory = std::env::temp_dir().join(format!("cueglint-export-{}", Uuid::new_v4()));
        std::fs::create_dir(&directory)?;
        let path = directory.join("session.md");
        emit("first", Some(&path), false)?;
        assert!(emit("second", Some(&path), false).is_err());
        assert_eq!(std::fs::read_to_string(&path)?, "first");
        emit("second", Some(&path), true)?;
        assert_eq!(std::fs::read_to_string(&path)?, "second");
        assert_eq!(std::fs::read_dir(&directory)?.count(), 1);
        std::fs::remove_dir_all(directory)?;
        Ok(())
    }

    #[test]
    fn input_limit_and_utf8_are_checked_before_ingestion() -> Result<()> {
        let path = std::env::temp_dir().join(format!("cueglint-input-{}", Uuid::new_v4()));
        std::fs::write(&path, "ééé")?;
        assert!(read_bounded_text(&path, 4).is_err());
        assert_eq!(read_bounded_text(&path, 6)?, "ééé");
        std::fs::write(&path, [0xff, 0xfe])?;
        assert!(read_bounded_text(&path, 6).is_err());
        std::fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn export_paths_cannot_replace_database_or_its_live_sidecars() -> Result<()> {
        let directory = std::env::temp_dir().join(format!("cueglint-protect-{}", Uuid::new_v4()));
        std::fs::create_dir(&directory)?;
        let database = directory.join("state.sqlite3");
        std::fs::write(&database, "database")?;
        for suffix in ["", ".lock", "-wal", "-shm", "-journal"] {
            assert!(
                ensure_export_not_database(
                    Some(&directory.join(format!("state.sqlite3{suffix}"))),
                    &database
                )
                .is_err()
            );
        }
        let alias = directory.join("alias");
        std::fs::hard_link(&database, &alias)?;
        assert!(ensure_export_not_database(Some(&alias), &database).is_err());
        ensure_export_not_database(Some(&directory.join("export.json")), &database)?;
        std::fs::remove_dir_all(directory)?;
        Ok(())
    }
}
