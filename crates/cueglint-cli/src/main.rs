use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand, ValueEnum};
use cueglint_core::{
    ExportFormat, MAX_BACKUP_BYTES, Mode, NotesAppendOutcome, NotesCancelReason, NotesCompletion,
    NotesJobStatus, RequestStatus, Speaker, Store, TranscriptSource, TranscriptUpdate,
    TranscriptionBackend,
    files::read_bounded_text,
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
    io::{IsTerminal, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
use uuid::Uuid;
use zeroize::Zeroizing;

#[global_allocator]
static ALLOCATOR: cueglint_core::knowledge::pdf_worker::Allocator =
    cueglint_core::knowledge::pdf_worker::Allocator;

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
    /// Generate or review separate extractive highlights; never replace manual notes.
    Highlights {
        #[command(subcommand)]
        command: HighlightsCommand,
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
    /// Ingest UTF-8 text, Markdown, bounded DOCX or text-based PDF; inspect local retrieval.
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
    /// Transcribe a mono 16kHz PCM16 WAV locally (at most 30 seconds).
    Transcribe {
        wav: PathBuf,
        /// Override the stored transcription selection or measured default profile.
        #[arg(long)]
        profile: Option<String>,
        /// Override the stored local processor. Metal requires an explicitly enabled build feature.
        #[arg(long)]
        backend: Option<TranscriptionBackend>,
        /// Override the stored input-language code; non-English requires whisper-base.
        #[arg(long, value_parser = parse_transcription_language)]
        language: Option<String>,
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
enum HighlightsCommand {
    /// Show the current job, accepted highlights, source freshness and manual notes revision.
    Review { session: String },
    /// Select whole utterances locally from a complete stopped transcript; leave a draft for review.
    /// Ctrl-C terminates the process; pending highlights are interrupted on the next Store open.
    Generate {
        session: String,
        /// Override the stored generation selection or measured default profile.
        #[arg(long)]
        profile: Option<String>,
    },
    /// Accept one ready job without changing manual notes.
    Accept { session: String, job: String },
    /// Discard one unreviewed draft while preserving accepted highlights and manual notes.
    Discard { session: String, job: String },
    /// Append one accepted record once; fail if manual notes changed since review.
    Append {
        session: String,
        generated: String,
        #[arg(long)]
        expected_notes_revision: u64,
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
    /// Save an editable copy with the same instructions and file scope.
    Duplicate {
        id: String,
    },
    /// Use only the listed library documents. An empty list uses no documents.
    SetFiles {
        id: String,
        #[arg(value_name = "DOCUMENT_ID")]
        document_ids: Vec<String>,
        /// Search the entire library, including subsequently imported documents.
        #[arg(long, conflicts_with = "document_ids")]
        all: bool,
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
    SetTranscriptionBackend {
        backend: TranscriptionBackend,
    },
    SetTranscriptionLanguage {
        #[arg(value_parser = parse_transcription_language)]
        language: String,
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

fn parse_transcription_language(language: &str) -> Result<String> {
    cueglint_core::validate_transcription_language(language)?;
    Ok(language.into())
}

fn main() -> Result<()> {
    cueglint_core::knowledge::pdf_worker::dispatch();
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
        Command::Highlights { command } => match command {
            HighlightsCommand::Review { session } => review_highlights(&store, &session)?,
            HighlightsCommand::Accept { session, job } => {
                print_json(&store.accept_notes(&session, &job)?)?;
            }
            HighlightsCommand::Discard { session, job } => {
                print_json(&store.discard_notes(&session, &job)?)?;
            }
            HighlightsCommand::Append {
                session,
                generated,
                expected_notes_revision,
            } => {
                let outcome =
                    store.append_generated_notes(&session, &generated, expected_notes_revision)?;
                let conflict = matches!(&outcome, NotesAppendOutcome::Conflict { .. });
                print_json(&outcome)?;
                ensure!(
                    !conflict,
                    "manual notes changed; review highlights and notes before explicitly retrying with the current revision"
                );
            }
            HighlightsCommand::Generate { session, profile } => {
                {
                    let current = store.session(&session)?;
                    ensure!(
                        !current.notes_job.as_ref().is_some_and(|job| matches!(
                            job.status,
                            NotesJobStatus::Pending | NotesJobStatus::Ready
                        )),
                        "accept or discard the current highlights draft before generating another"
                    );
                }
                let input = store.notes_input(&session)?;
                let cancel = AtomicBool::new(false);
                let mut generator = LocalGenerator::load_profile_with_cancel(
                    &models_directory,
                    profile.as_deref().unwrap_or(generation_profile),
                    &cancel,
                )?;
                let prepared = generator.plan_notes(&input, &cancel)?;
                let job = store.begin_notes(&session, prepared.plan().clone())?;
                let completion = generator.generate_notes(&prepared, &cancel, |progress| {
                    let _ = writeln!(
                        std::io::stderr().lock(),
                        "Validated highlight chunks {}/{}",
                        progress.completed_chunks,
                        progress.total_chunks
                    );
                });
                finish_highlights(&mut store, &session, &job.id, completion, &cancel)?;
                review_highlights(&store, &session)?;
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
            } => print_json(&store.update_mode_text(id, name, instructions)?)?,
            ModeCommand::Duplicate { id } => {
                let copy = store.mode(&id)?.duplicate();
                print_json(&store.upsert_mode(copy)?)?;
            }
            ModeCommand::SetFiles {
                id,
                document_ids,
                all,
            } => {
                let mut mode = store.mode(&id)?;
                mode.knowledge_document_ids = (!all).then_some(document_ids);
                print_json(&store.upsert_mode(mode)?)?;
            }
            ModeCommand::Delete { id } => {
                store.delete_mode(&id)?;
                print_json(&serde_json::json!({"deleted": id}))?;
            }
        },
        Command::Knowledge { command } => match command {
            KnowledgeCommand::List => print_json(&store.knowledge()?)?,
            KnowledgeCommand::Add { file, title } => {
                let text = cueglint_core::knowledge::read_document(&file, &AtomicBool::new(false))?;
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
                SettingsCommand::SetTranscriptionBackend { backend } => {
                    settings.transcription_backend = backend;
                }
                SettingsCommand::SetTranscriptionLanguage { language } => {
                    settings.transcription_language = language;
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
                    if purpose == "transcription" {
                        cueglint_local::asr::validate_language(
                            profile,
                            &preferences.transcription_language,
                        )?;
                    }
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
            let backend_compiled =
                cueglint_local::asr::backend_compiled(preferences.transcription_backend);
            print_json(
                &serde_json::json!({"inference_available": if preferences.answer_provider.provider.is_local() { serde_json::json!(model["state"] == "verified") } else { serde_json::Value::Null },
                "local_inference_available": model["state"] == "verified", "answer_provider": preferences.answer_provider,
                "remote_availability": "not_tested",
                "transcription_available": transcription_available(
                    transcription["state"] == "verified", preferences.transcription_backend),
                "transcription_backend": {"selected": preferences.transcription_backend,
                    "compiled": backend_compiled, "device_availability": "not_tested"}, "model": model,
                "transcription_language": preferences.transcription_language,
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
        Command::Transcribe {
            wav,
            profile,
            backend,
            language,
        } => {
            let profile = profile.as_deref().unwrap_or(transcription_profile);
            let backend = backend.unwrap_or(preferences.transcription_backend);
            let language = language
                .as_deref()
                .unwrap_or(&preferences.transcription_language);
            ensure!(
                cueglint_local::asr::backend_compiled(backend),
                "Metal ASR is unavailable in this build; select CPU or build with cueglint-local/metal"
            );
            cueglint_local::asr::validate_language(profile, language)?;
            let samples = cueglint_local::audio::load_wav(&wav)?;
            let started = std::time::Instant::now();
            let (mut transcriber, profile) =
                cueglint_local::asr::Transcriber::load_profile_with_language(
                    &models_directory,
                    profile,
                    backend,
                    language,
                    &std::sync::atomic::AtomicBool::new(false),
                )?;
            let text =
                transcriber.transcribe(&samples, &std::sync::atomic::AtomicBool::new(false))?;
            print_json(
                &serde_json::json!({"text": text, "language": transcriber.language(), "sample_rate": 16000,
                "sample_count": samples.len(), "elapsed_ms": started.elapsed().as_millis(), "model": profile,
                "backend": transcriber.backend(), "requested_backend": backend,
                "frontend_revision": cueglint_local::asr_frontend::REVISION,
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

fn transcription_available(model_verified: bool, backend: TranscriptionBackend) -> Option<bool> {
    if !model_verified || !cueglint_local::asr::backend_compiled(backend) {
        Some(false)
    } else if backend == TranscriptionBackend::Cpu {
        Some(true)
    } else {
        None
    }
}

fn review_highlights(store: &Store, session_id: &str) -> Result<()> {
    let session = store.session(session_id)?;
    let job_freshness = session
        .notes_job
        .as_ref()
        .map(|job| store.notes_freshness(session_id, &job.plan.source))
        .transpose()?;
    let accepted_freshness = session
        .generated_notes
        .as_ref()
        .map(|notes| store.notes_freshness(session_id, &notes.plan.source))
        .transpose()?;
    print_json(&serde_json::json!({
        "kind":"extractive_highlights", "session_id":session.id,
        "notes_revision":session.notes_revision, "user_notes":session.notes,
        "job":session.notes_job, "job_freshness":job_freshness,
        "accepted":session.generated_notes, "accepted_freshness":accepted_freshness,
    }))
}

fn finish_highlights(
    store: &mut Store,
    session: &str,
    job: &str,
    completion: Result<NotesCompletion>,
    cancel: &AtomicBool,
) -> Result<()> {
    let result = completion.and_then(|completion| {
        ensure!(
            !cancel.load(Ordering::Relaxed),
            "highlights job cancelled before publication"
        );
        ensure!(
            store.complete_notes(session, job, completion)?,
            "highlights job is no longer current; no draft was published"
        );
        Ok(())
    });
    if let Err(error) = result {
        let reason = if cancel.load(Ordering::Relaxed) {
            NotesCancelReason::User
        } else {
            NotesCancelReason::Failed
        };
        if let Err(retire_error) = store.cancel_notes(session, job, reason) {
            return Err(error.context(format!(
                "could not retire highlights job {job}: {retire_error:#}"
            )));
        }
        return Err(error);
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
    use cueglint_core::{NotesPlan, NotesProvenance};

    #[test]
    fn transcription_language_arguments_are_explicit_codes_without_auto_detection() {
        for language in ["en", "pt", "haw"] {
            let cli = Cli::try_parse_from([
                "cueglint-cli",
                "transcribe",
                "original.wav",
                "--language",
                language,
            ])
            .unwrap();
            assert!(matches!(cli.command,
                Command::Transcribe {language: Some(selected), ..} if selected == language));
            let cli = Cli::try_parse_from([
                "cueglint-cli",
                "settings",
                "set-transcription-language",
                language,
            ])
            .unwrap();
            assert!(matches!(cli.command,
                Command::Settings {command: SettingsCommand::SetTranscriptionLanguage {language: selected}} if selected == language));
        }
        let cli = Cli::try_parse_from(["cueglint-cli", "transcribe", "original.wav"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Transcribe { language: None, .. }
        ));
        for language in ["auto", "en-US", "Portuguese", " pt", "translate"] {
            assert!(
                Cli::try_parse_from([
                    "cueglint-cli",
                    "transcribe",
                    "original.wav",
                    "--language",
                    language,
                ])
                .is_err()
            );
            assert!(
                Cli::try_parse_from([
                    "cueglint-cli",
                    "settings",
                    "set-transcription-language",
                    language,
                ])
                .is_err()
            );
        }
    }

    #[test]
    fn transcription_backend_arguments_require_an_explicit_local_processor() {
        for (value, expected) in [
            ("cpu", TranscriptionBackend::Cpu),
            ("metal", TranscriptionBackend::Metal),
        ] {
            let cli = Cli::try_parse_from([
                "cueglint-cli",
                "transcribe",
                "original.wav",
                "--backend",
                value,
            ])
            .unwrap();
            assert!(
                matches!(cli.command, Command::Transcribe {backend: Some(backend), ..} if backend == expected)
            );
            let cli = Cli::try_parse_from([
                "cueglint-cli",
                "settings",
                "set-transcription-backend",
                value,
            ])
            .unwrap();
            assert!(
                matches!(cli.command, Command::Settings {command: SettingsCommand::SetTranscriptionBackend {backend}} if backend == expected)
            );
        }
        let cli = Cli::try_parse_from(["cueglint-cli", "transcribe", "original.wav"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Transcribe { backend: None, .. }
        ));
        assert!(
            Cli::try_parse_from([
                "cueglint-cli",
                "transcribe",
                "original.wav",
                "--backend",
                "auto",
            ])
            .is_err()
        );
    }

    #[test]
    fn model_readiness_does_not_claim_a_metal_device_was_initialized() {
        assert_eq!(
            transcription_available(false, TranscriptionBackend::Cpu),
            Some(false)
        );
        assert_eq!(
            transcription_available(true, TranscriptionBackend::Cpu),
            Some(true)
        );
        assert_eq!(
            transcription_available(false, TranscriptionBackend::Metal),
            Some(false)
        );
        assert_eq!(
            transcription_available(true, TranscriptionBackend::Metal),
            if cueglint_local::asr::backend_compiled(TranscriptionBackend::Metal) {
                None
            } else {
                Some(false)
            }
        );
    }

    fn supplied(index: usize) -> NotesCompletion {
        NotesCompletion {
            selections: vec![vec![index]],
            provenance: NotesProvenance::Supplied {},
        }
    }

    fn pending_highlights() -> (Store, String, NotesPlan, String) {
        let mut store = Store::open_in_memory().unwrap();
        let session = store.start_session("Original title", None).unwrap().id;
        store
            .update_notes(&session, "Original human notes\r\nCafé 🦀")
            .unwrap();
        store
            .update_summary(&session, "Legacy user summary.")
            .unwrap();
        store
            .upsert_transcript(
                &session,
                TranscriptUpdate {
                    id: "original-utterance".into(),
                    text: "Maybe someone could help; no commitment.".into(),
                    speaker: Speaker::Unknown,
                    source: TranscriptSource::Manual,
                    finalized: true,
                },
            )
            .unwrap();
        store.stop_session(&session).unwrap();
        let plan = NotesPlan {
            source: store.notes_input(&session).unwrap().source,
            chunks: vec![vec![0]],
        };
        let accepted = store.begin_notes(&session, plan.clone()).unwrap();
        store
            .complete_notes(&session, &accepted.id, supplied(0))
            .unwrap();
        store.accept_notes(&session, &accepted.id).unwrap();
        let job = store.begin_notes(&session, plan.clone()).unwrap().id;
        (store, session, plan, job)
    }

    #[test]
    fn highlight_generation_completion_and_failures_preserve_manual_and_accepted_notes() {
        for case in ["success", "runner-error", "invalid-completion", "cancelled"] {
            let (mut store, session, _, job) = pending_highlights();
            let before = store.session(&session).unwrap();
            let cancel = AtomicBool::new(case == "cancelled");
            let completion = match case {
                "runner-error" => Err(anyhow::anyhow!("controlled worker error")),
                "invalid-completion" => Ok(supplied(99)),
                _ => Ok(supplied(0)),
            };
            let result = finish_highlights(&mut store, &session, &job, completion, &cancel);
            let after = store.session(&session).unwrap();
            assert_eq!(after.notes, before.notes);
            assert_eq!(after.notes_revision, before.notes_revision);
            assert_eq!(after.title, before.title);
            assert_eq!(after.summary, before.summary);
            assert_eq!(after.generated_notes, before.generated_notes);
            let current = after.notes_job.unwrap();
            if case == "success" {
                assert!(result.is_ok());
                assert_eq!(current.status, NotesJobStatus::Ready);
                assert!(current.candidate.is_some());
            } else {
                assert!(result.is_err());
                assert_eq!(current.status, NotesJobStatus::Cancelled);
                assert_eq!(
                    current.cancel_reason,
                    Some(if case == "cancelled" {
                        NotesCancelReason::User
                    } else {
                        NotesCancelReason::Failed
                    })
                );
                assert!(current.candidate.is_none());
            }
        }
    }

    #[test]
    fn late_highlight_failure_or_completion_cannot_retire_the_next_job() {
        let (mut store, session, plan, old) = pending_highlights();
        store
            .cancel_notes(&session, &old, NotesCancelReason::User)
            .unwrap();
        store.begin_notes(&session, plan).unwrap();
        let before = store.session(&session).unwrap();
        for completion in [Err(anyhow::anyhow!("late worker error")), Ok(supplied(0))] {
            assert!(
                finish_highlights(
                    &mut store,
                    &session,
                    &old,
                    completion,
                    &AtomicBool::new(false)
                )
                .is_err()
            );
            assert_eq!(store.session(&session).unwrap(), before);
        }
    }

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
