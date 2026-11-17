use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand, ValueEnum};
use cueglint_core::{
    ExportFormat, MAX_BACKUP_BYTES, MAX_DOCUMENT_BYTES, Mode, RequestStatus, Speaker, Store,
    TranscriptSource, TranscriptUpdate,
};
use cueglint_local::{
    generation::{FinishReason, GenerationOptions, LocalGenerator},
    models,
};
use serde::Serialize;
use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Parser)]
#[command(
    version,
    about = "Offline Cueglint session, transcript, knowledge and backup tools"
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
    /// Report model file configuration; configuration does not imply running inference.
    ModelStatus,
    /// List, explicitly download, or hash-verify pinned local model profiles.
    Models {
        #[command(subcommand)]
        command: ModelsCommand,
    },
    /// Generate and persist a real local answer from a frozen request context.
    Ask {
        session: String,
        prompt: String,
        #[arg(long, default_value_t = 128)]
        max_tokens: usize,
        /// Send decoded text deltas to stderr while keeping the final JSON on stdout.
        #[arg(long)]
        stream: bool,
    },
    /// Transcribe a mono 16kHz PCM16 WAV locally (English tiny.en, at most 30 seconds).
    Transcribe { wav: PathBuf },
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
    SetModel {
        path: PathBuf,
    },
    ClearModel,
    SetModelDirectory {
        path: PathBuf,
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
    Install { profile: String },
    Verify { profile: String },
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
    let models_directory = cli
        .models_dir
        .or(store.settings()?.model_directory.map(PathBuf::from))
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
                    "message": "Context prepared only; no answer was generated. Use ask with an installed model to run local inference."}),
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
                SettingsCommand::SetModel { path } => {
                    settings.model_path =
                        Some(path.to_str().context("model path is not UTF-8")?.to_owned())
                }
                SettingsCommand::ClearModel => settings.model_path = None,
                SettingsCommand::SetModelDirectory { path } => {
                    settings.model_directory = Some(
                        path.to_str()
                            .context("model directory is not UTF-8")?
                            .to_owned(),
                    )
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
        Command::ModelStatus => print_json(
            &serde_json::json!({"model": store.model_status()?, "models_directory": models_directory,
            "inference_available": models::verify(&models_directory, models::GENERATION_PROFILE).is_ok()}),
        )?,
        Command::Models { command } => match command {
            ModelsCommand::List => print_json(&models::profiles()?)?,
            ModelsCommand::Verify { profile } => {
                print_json(&models::verify(&models_directory, &profile)?)?
            }
            ModelsCommand::Install { profile } => {
                let mut last_progress = (String::new(), 101);
                let installed = models::install(
                    &models_directory,
                    &profile,
                    &std::sync::atomic::AtomicBool::new(false),
                    |progress| {
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
                    },
                )?;
                print_json(&installed)?;
            }
        },
        Command::Ask {
            session,
            prompt,
            max_tokens,
            stream,
        } => {
            ensure!(
                (1..=1024).contains(&max_tokens),
                "--max-tokens must be between 1 and 1024"
            );
            let cancel = std::sync::atomic::AtomicBool::new(false);
            let mut generator = LocalGenerator::load(&models_directory)?;
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
        Command::Transcribe { wav } => {
            let samples = cueglint_local::audio::load_wav(&wav)?;
            let installed = models::verify(&models_directory, models::TRANSCRIPTION_PROFILE)?;
            let started = std::time::Instant::now();
            let mut transcriber = cueglint_local::asr::Transcriber::load(&installed.directory)?;
            let text =
                transcriber.transcribe(&samples, &std::sync::atomic::AtomicBool::new(false))?;
            print_json(
                &serde_json::json!({"text": text, "language": "en", "sample_rate": 16000,
                "sample_count": samples.len(), "elapsed_ms": started.elapsed().as_millis(), "model": installed.profile}),
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
        #[cfg(unix)]
        if let (Ok(output_metadata), Ok(reserved_metadata)) =
            (std::fs::metadata(output), std::fs::metadata(reserved))
        {
            use std::os::unix::fs::MetadataExt;
            ensure!(
                output_metadata.dev() != reserved_metadata.dev()
                    || output_metadata.ino() != reserved_metadata.ino(),
                "an export cannot overwrite a hard link to the active database or its sidecar files"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
