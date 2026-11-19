use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
    thread,
    time::Duration,
};

pub use crate::runtime::RuntimeSnapshot;
use crate::runtime::{EndAction, ProviderAction, Runtime};
use crate::screenshot;
use anyhow::{Context, Result};
use cueglint_core::{
    ExportFormat, KnowledgeDocument, Mode, Session, SessionStatus, Settings, Speaker, Store,
    TranscriptSource, TranscriptUpdate, provider::ProviderConfig,
};
use eframe::egui;
#[path = "worker_highlights.rs"]
mod highlights;
use highlights::execute_highlights;
pub use highlights::{HighlightsAck, HighlightsAction, HighlightsCommand};
#[path = "worker_knowledge.rs"]
mod knowledge;
pub use knowledge::ImportCommand;

pub struct Snapshot {
    pub draft_freshness: Option<cueglint_core::NotesFreshness>,
    pub accepted_freshness: Option<cueglint_core::NotesFreshness>,
    pub sessions: Vec<Session>,
    pub selected: Option<Session>,
    pub modes: Vec<Mode>,
    pub knowledge: Vec<KnowledgeDocument>,
    pub settings: Settings,
    pub runtime: RuntimeSnapshot,
}

pub enum Command {
    Highlights(HighlightsCommand),
    Start {
        title: String,
        mode: Option<String>,
    },
    Open(String),
    NewChat,
    Pause(String),
    Resume(String),
    Stop(String),
    Save(String),
    Mute(String, bool),
    Delete(String),
    Notes(String, String),
    Transcript(String, String, Speaker),
    Ask {
        session: Option<String>,
        prompt: String,
        new_chat_mode: Option<String>,
    },
    SessionMode(String, Option<String>),
    Rename(String, String),
    Mode {
        mode: Mode,
        activate: bool,
    },
    DeleteMode(String),
    Import(ImportCommand),
    DeleteKnowledge(String),
    Settings(Settings),
    Provider(String, ProviderConfig, ProviderAction),
    CancelProvider,
    Export(String, PathBuf),
    ExportBackup(PathBuf),
    RestoreBackup {
        path: PathBuf,
        replace: bool,
    },
    InstallModel(String),
    ImportModel(String, PathBuf),
    RemoveModel(String),
    Cancel,
    CancelInstall,
    CaptureScreen,
    WindowPicker(cueglint_local::screen::scoped::PickerEvent),
    CancelWindowPicker,
    RefreshPermissions,
    RefreshCaptureInputs,
    CancelCaptureInputs(String),
    OpenPrivacySettings,
    ScreenEnabled(bool),
}

pub enum Response {
    Import(String, Result<Snapshot, String>),
    Restored(Result<Snapshot, String>),
    Highlights(HighlightsAck),
    State(Snapshot, Option<String>),
    Error(String),
    Screenshot(Result<screenshot::Receipt, String>),
    Runtime(RuntimeSnapshot),
    Update(Snapshot, Option<String>),
    Notice(String, bool),
}

pub struct Worker {
    pub commands: SyncSender<Command>,
    pub responses: Receiver<Response>,
    pub screenshot: SyncSender<screenshot::Job>,
}

impl Worker {
    pub fn spawn(database: Option<PathBuf>, fixture: Option<String>, ctx: egui::Context) -> Self {
        let (commands, jobs) = mpsc::sync_channel(16);
        let (replies, responses) = mpsc::sync_channel(16);
        let (screenshot, images) = mpsc::sync_channel::<screenshot::Job>(1);
        let image_replies = replies.clone();
        let image_ctx = ctx.clone();
        thread::Builder::new()
            .name("cueglint-screenshot".into())
            .spawn(move || {
                while let Ok(job) = images.recv() {
                    let result = screenshot::save(job).map_err(|error| format!("{error:#}"));
                    if image_replies.send(Response::Screenshot(result)).is_err() {
                        break;
                    }
                    image_ctx.request_repaint();
                }
            })
            .expect("create screenshot worker");
        thread::Builder::new()
            .name("cueglint-store".into())
            .spawn(move || {
                let mut store = match if fixture.is_some() {
                    Store::open_in_memory()
                } else {
                    database
                        .map(Ok)
                        .unwrap_or_else(Store::default_path)
                        .and_then(Store::open)
                } {
                    Ok(store) => store,
                    Err(error) => {
                        let _ = replies.send(Response::Error(format!(
                            "Could not open local data: {error:#}"
                        )));
                        ctx.request_repaint();
                        return;
                    }
                };
                if let Err(error) = store.recover_interrupted() {
                    let _ = replies.send(Response::Error(format!(
                        "Could not recover interrupted work: {error:#}"
                    )));
                    ctx.request_repaint();
                    return;
                }
                let mut selected = None;
                if let Some(ref view) = fixture {
                    match seed_fixture(&mut store, view) {
                        Ok(id) => selected = id,
                        Err(error) => {
                            let _ = replies
                                .send(Response::Error(format!("Fixture setup failed: {error:#}")));
                            ctx.request_repaint();
                            return;
                        }
                    }
                }
                let mut runtime = match Runtime::new(&store, fixture.is_some(), ctx.clone()) {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = replies.send(Response::Error(format!(
                            "Runtime initialization failed: {error:#}"
                        )));
                        ctx.request_repaint();
                        return;
                    }
                };
                let initial = snapshot(&store, selected.as_deref(), &runtime.snapshot);
                let _ = replies.send(match initial {
                    Ok(state) => Response::State(state, None),
                    Err(error) => Response::Error(format!("{error:#}")),
                });
                ctx.request_repaint();
                let mut imports = knowledge::Imports::default();
                loop {
                    if let Some((id, result)) = imports.poll(&mut store) {
                        let result = result
                            .and_then(|_| snapshot(&store, selected.as_deref(), &runtime.snapshot))
                            .map_err(|error| format!("{error:#}"));
                        if replies.send(Response::Import(id, result)).is_err() {
                            break;
                        }
                        ctx.request_repaint();
                    }
                    while let Some(event) = runtime.try_event() {
                        let result = match event {
                            crate::runtime::Event::WindowCaptured(id, frame) => runtime
                                .window_capture_finished(&mut store, &mut selected, id, frame),
                            event => runtime.apply(&mut store, event),
                        };
                        let response = match result {
                            Ok((true, notice)) => {
                                snapshot(&store, selected.as_deref(), &runtime.snapshot)
                                    .map(|state| Response::Update(state, notice))
                            }
                            Ok((false, Some(notice))) => {
                                snapshot(&store, selected.as_deref(), &runtime.snapshot)
                                    .map(|state| Response::Update(state, Some(notice)))
                            }
                            Ok((false, None)) => Ok(Response::Runtime(runtime.snapshot.clone())),
                            Err(error) => snapshot(&store, selected.as_deref(), &runtime.snapshot)
                                .map(|state| {
                                    Response::Update(
                                        state,
                                        Some(format!("Background operation failed: {error:#}")),
                                    )
                                }),
                        };
                        if replies
                            .send(response.unwrap_or_else(|error| {
                                Response::Notice(format!("{error:#}"), true)
                            }))
                            .is_err()
                        {
                            return;
                        }
                        ctx.request_repaint();
                    }
                    match jobs.recv_timeout(Duration::from_millis(25)) {
                        Ok(Command::Import(command)) => {
                            let ready_ctx = ctx.clone();
                            let result = start_import(
                                &store,
                                selected.as_deref(),
                                &runtime.snapshot,
                                &mut imports,
                                command,
                                move || ready_ctx.request_repaint(),
                            )
                            .map(|state| Response::State(state, None))
                            .unwrap_or_else(|error| Response::Error(format!("{error:#}")));
                            if replies.send(result).is_err() {
                                break;
                            }
                            ctx.request_repaint();
                        }
                        Ok(Command::Highlights(command)) => {
                            let result = execute_highlights(
                                &mut store,
                                selected.as_deref(),
                                &mut runtime,
                                &command,
                            )
                            .and_then(|outcome| {
                                snapshot(&store, selected.as_deref(), &runtime.snapshot)
                                    .map(|state| (state, outcome))
                            })
                            .map_err(|error| format!("{error:#}"));
                            if replies
                                .send(Response::Highlights(HighlightsAck { command, result }))
                                .is_err()
                            {
                                break;
                            }
                            ctx.request_repaint();
                        }
                        Ok(Command::WindowPicker(event)) => {
                            let notice = runtime
                                .window_picker_event(event)
                                .unwrap_or_else(|error| Some(format!("{error:#}")));
                            if replies
                                .send(Response::Runtime(runtime.snapshot.clone()))
                                .is_err()
                            {
                                break;
                            }
                            if let Some(message) = notice {
                                if replies.send(Response::Notice(message, true)).is_err() {
                                    break;
                                }
                            }
                            ctx.request_repaint();
                        }
                        Ok(Command::RestoreBackup { path, replace }) => {
                            let result = restore_backup(
                                &mut store,
                                &mut selected,
                                &mut runtime,
                                &imports,
                                path,
                                replace,
                            )
                            .unwrap_or_else(|error| Response::Error(format!("{error:#}")));
                            if replies.send(result).is_err() {
                                break;
                            }
                            ctx.request_repaint();
                        }
                        Ok(command) => {
                            let result = execute(&mut store, &mut selected, &mut runtime, command)
                                .and_then(|notice| {
                                    snapshot(&store, selected.as_deref(), &runtime.snapshot)
                                        .map(|state| Response::State(state, notice))
                                });
                            match result {
                                Ok(response) => {
                                    if replies.send(response).is_err() {
                                        break;
                                    }
                                }
                                Err(error) => {
                                    if replies.send(Response::Error(format!("{error:#}"))).is_err()
                                    {
                                        break;
                                    }
                                    if let Ok(state) =
                                        snapshot(&store, selected.as_deref(), &runtime.snapshot)
                                    {
                                        let _ = replies.send(Response::Update(state, None));
                                    }
                                }
                            }
                            ctx.request_repaint();
                        }
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .expect("create store worker");
        Self {
            commands,
            responses,
            screenshot,
        }
    }
}

fn restore_backup(
    store: &mut Store,
    selected: &mut Option<String>,
    runtime: &mut Runtime,
    imports: &knowledge::Imports,
    path: PathBuf,
    replace: bool,
) -> Result<Response> {
    runtime.ensure_restore_idle()?;
    anyhow::ensure!(
        !imports.is_busy(),
        "Wait for document import to finish before restoring."
    );
    let json = cueglint_core::files::read_backup_file(&path)
        .with_context(|| format!("Could not read backup from {}", path.display()))?;
    let restored = store.restore_local_backup(&json, replace, |settings| {
        runtime
            .prepare_restore(settings)
            .context("Could not prepare the runtime for the restored settings")
    })?;
    *runtime = restored;
    *selected = None;
    runtime.refresh_permissions();
    runtime.refresh_models();
    Ok(Response::Restored(
        snapshot(store, None, &runtime.snapshot).map_err(|error| format!("{error:#}")),
    ))
}

fn start_import(
    store: &Store,
    selected: Option<&str>,
    runtime: &RuntimeSnapshot,
    imports: &mut knowledge::Imports,
    command: ImportCommand,
    on_ready: impl FnOnce() + Send + 'static,
) -> Result<Snapshot> {
    let state = snapshot(store, selected, runtime)?;
    imports.start(command, on_ready)?;
    Ok(state)
}

fn snapshot(store: &Store, selected: Option<&str>, runtime: &RuntimeSnapshot) -> Result<Snapshot> {
    let selected = selected.map(|id| store.session(id)).transpose()?;
    let draft_freshness = selected
        .as_ref()
        .and_then(|session| {
            session
                .notes_job
                .as_ref()
                .filter(|job| job.candidate.is_some())
                .map(|job| (session, &job.plan.source))
        })
        .map(|(session, source)| store.notes_freshness(&session.id, source))
        .transpose()?;
    let accepted_freshness = selected
        .as_ref()
        .and_then(|session| {
            session
                .generated_notes
                .as_ref()
                .map(|notes| (session, &notes.plan.source))
        })
        .map(|(session, source)| store.notes_freshness(&session.id, source))
        .transpose()?;
    Ok(Snapshot {
        draft_freshness,
        accepted_freshness,
        sessions: store.sessions("")?,
        selected,
        modes: store.modes()?,
        knowledge: store.knowledge()?,
        settings: store.settings()?,
        runtime: runtime.clone(),
    })
}

fn question_session(
    store: &mut Store,
    session: Option<String>,
    new_chat_mode: Option<&str>,
) -> Result<String> {
    if let Some(id) = session {
        return Ok(id);
    }
    Ok(store.start_chat(new_chat_mode)?.id)
}

pub(crate) fn execute(
    store: &mut Store,
    selected: &mut Option<String>,
    runtime: &mut Runtime,
    command: Command,
) -> Result<Option<String>> {
    let mut notice = None;
    match command {
        Command::Highlights(_) => anyhow::bail!("highlights require their typed acknowledgement"),
        Command::Start { title, mode } => {
            anyhow::ensure!(
                !runtime.highlights_busy(),
                "wait for highlights cancellation to finish before starting a session"
            );
            *selected = Some(
                if let Some(active) = store
                    .sessions("")?
                    .into_iter()
                    .find(|session| session.status != SessionStatus::Stopped)
                {
                    active.id
                } else {
                    store.start_session(&title, mode.as_deref())?.id
                },
            );
            let id = selected.as_ref().unwrap();
            if store.session(id)?.status == SessionStatus::Recording {
                if let Err(error) = runtime.start_capture(store, id) {
                    store.pause_session(id)?;
                    return Err(error);
                }
            }
        }
        Command::Open(id) => {
            runtime.cancel_all(store)?;
            store.session(&id)?;
            *selected = Some(id);
        }
        Command::NewChat => {
            runtime.cancel(store)?;
            *selected = None;
        }
        Command::Pause(id) => {
            if !runtime.end_capture(&id, EndAction::Pause)? {
                store.pause_session(&id)?;
            }
        }
        Command::Resume(id) => {
            runtime.cancel_all(store)?;
            store.resume_session(&id)?;
            if let Err(error) = runtime.start_capture(store, &id) {
                store.pause_session(&id)?;
                return Err(error);
            }
        }
        Command::Stop(id) => {
            runtime.cancel_all(store)?;
            notice = Some(if runtime.end_capture(&id, EndAction::Stop)? {
                "Finalizing audio before saving the session.".into()
            } else {
                store.finish_session(&id)?;
                "Session saved locally.".into()
            });
        }
        Command::Save(id) => {
            store.save_session(&id)?;
            notice = Some("Session saved locally.".into());
        }
        Command::Mute(id, muted) => {
            if muted {
                if !runtime.end_capture(&id, EndAction::Mute)? {
                    store.mute_session(&id, true)?;
                }
            } else {
                store.mute_session(&id, false)?;
                if store.session(&id)?.status == SessionStatus::Recording {
                    runtime.start_capture(store, &id)?;
                }
            }
        }
        Command::Delete(id) => {
            anyhow::ensure!(
                !runtime
                    .snapshot
                    .capture
                    .as_ref()
                    .is_some_and(|c| c.session_id == id
                        && ["initializing", "listening", "finalizing"].contains(&c.state.as_str())),
                "stop and finalize capture before deleting this session"
            );
            runtime.cancel_all(store)?;
            store.delete_session(&id)?;
            if selected.as_ref() == Some(&id) {
                *selected = None;
            }
        }
        Command::Notes(id, notes) => {
            store.update_notes(&id, &notes)?;
        }
        Command::Transcript(id, text, speaker) => {
            runtime.cancel_highlights(store, cueglint_core::NotesCancelReason::SourceChanged)?;
            let segment_id = format!(
                "manual-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_nanos()
            );
            store.upsert_transcript(
                &id,
                TranscriptUpdate {
                    id: segment_id,
                    speaker,
                    text,
                    finalized: true,
                    source: TranscriptSource::Manual,
                },
            )?;
        }
        Command::Ask {
            session,
            prompt,
            new_chat_mode,
        } => {
            if runtime.snapshot.screen_enabled && !runtime.snapshot.screen_input.uses_display() {
                runtime.window_question(store, session, prompt, new_chat_mode)?;
                return Ok(None);
            }
            runtime.ensure_answer_ready(&prompt)?;
            let id = question_session(store, session, new_chat_mode.as_deref())?;
            *selected = Some(id.clone());
            runtime.ask(store, &id, prompt)?;
        }
        Command::SessionMode(id, mode) => {
            runtime.cancel(store)?;
            store.set_session_mode(&id, mode.as_deref())?;
        }
        Command::Rename(id, title) => {
            store.rename_session(&id, &title)?;
        }
        Command::Mode { mode, .. } => {
            runtime.cancel(store)?;
            store.upsert_mode(mode)?;
            notice = Some("Mode saved.".into());
        }
        Command::DeleteMode(id) => {
            runtime.cancel(store)?;
            store.delete_mode(&id)?;
        }
        Command::Import(_) => {
            anyhow::bail!("document imports require the bounded background worker")
        }
        Command::DeleteKnowledge(id) => {
            runtime.cancel(store)?;
            store.delete_knowledge(&id)?;
        }
        Command::Settings(settings) => {
            anyhow::ensure!(
                !runtime.snapshot.capture.as_ref().is_some_and(|c| [
                    "initializing",
                    "listening",
                    "finalizing"
                ]
                .contains(&c.state.as_str())),
                "stop capture before changing audio or model settings"
            );
            store.update_settings(settings)?;
            runtime.settings_changed(store)?;
            notice = Some("Settings saved locally.".into());
        }
        Command::InstallModel(profile) => runtime.install(profile)?,
        Command::ImportModel(profile, source) => runtime.import_model(profile, source)?,
        Command::RemoveModel(profile) => runtime.remove_model(profile)?,
        Command::Provider(id, config, action) => runtime.provider_operation(id, config, action)?,
        Command::CancelProvider => runtime.cancel_provider(),
        Command::Cancel => runtime.cancel_all(store)?,
        Command::CancelInstall => runtime.cancel_install(),
        Command::WindowPicker(_) => anyhow::bail!("window picker events require the native worker"),
        Command::CancelWindowPicker => runtime.cancel_window_picker(),
        Command::CaptureScreen => {
            runtime.refresh_permissions();
            runtime.preview_screen(store)?;
        }
        Command::RefreshPermissions => runtime.refresh_permissions(),
        Command::RefreshCaptureInputs => runtime.refresh_capture_inputs()?,
        Command::CancelCaptureInputs(id) => runtime.cancel_capture_inputs(&id),
        Command::OpenPrivacySettings => cueglint_local::permissions::open_settings()?,
        Command::ScreenEnabled(enabled) => {
            runtime.cancel(store)?;
            let mut settings = store.settings()?;
            settings.screen_enabled = enabled;
            store.update_settings(settings)?;
            runtime.settings_changed(store)?;
        }
        Command::Export(id, path) => {
            let text = store.export_session(&id, ExportFormat::Markdown)?;
            cueglint_core::files::write_private_export(&path, &text, false)?;
            notice = Some(format!("Exported to {}", path.display()));
        }
        Command::ExportBackup(path) => {
            let text = store
                .backup_json()
                .context("Could not create a backup from local data")?;
            cueglint_core::files::write_private_export(&path, &text, false)
                .with_context(|| format!("Could not save backup to {}", path.display()))?;
            notice = Some(format!("Backup exported to {}", path.display()));
        }
        Command::RestoreBackup { .. } => {
            anyhow::bail!("restore requires its typed acknowledgement")
        }
    }
    Ok(notice)
}

fn seed_fixture(store: &mut Store, view: &str) -> Result<Option<String>> {
    if view.starts_with("native-v2-") || view == "provider-v2" {
        if view == "provider-v2" {
            crate::provider_fixture()?;
        }
        let view = crate::fixture_view(view).context("unknown native v2 fixture view")?;
        store.restore_backup(include_str!("../../../ui/fixtures/native-v2.json"), false)?;
        return Ok(match view {
            "overlay" | "session" => Some("native-v2-design-review".into()),
            "markdown" => Some("native-v2-markdown-short".into()),
            "markdown-long" => Some("native-v2-markdown-long".into()),
            _ => None,
        });
    }
    if matches!(view, "markdown" | "markdown-long") {
        let session = store.start_session("[Fixture] Original Markdown preview", None)?;
        let request = store.begin_request(&session.id, "[Fixture] Render this original answer.")?;
        let answer = if view == "markdown-long" {
            format!("[Fixture] Original long Markdown list.\n\n{}", (1..=80).map(|index| format!("{index}. Original item {index:02}: review local notes, confirm the owner, and write the next step.\n")).collect::<String>())
        } else {
            "[Fixture] **Bold** and *emphasis*; [safe link](https://example.test/fixture).\n\n- Original local list item.\n\n```rust\nlet original = 7;\n```\n".into()
        };
        anyhow::ensure!(
            store.complete_request(&session.id, &request.id, &answer)?,
            "fixture response was not accepted"
        );
        store.stop_session(&session.id)?;
        store.save_session(&session.id)?;
        return Ok(Some(session.id));
    }
    let first = store.start_session("[Fixture] Product planning", None)?;
    store.upsert_transcript(
        &first.id,
        TranscriptUpdate {
            id: "fixture-other".into(),
            speaker: Speaker::Other,
            text: "What should we prioritize for the next release?".into(),
            finalized: true,
            source: TranscriptSource::Manual,
        },
    )?;
    store.upsert_transcript(
        &first.id,
        TranscriptUpdate {
            id: "fixture-you".into(),
            speaker: Speaker::You,
            text: "Let's focus on a reliable local workflow and make the next steps clear.".into(),
            finalized: true,
            source: TranscriptSource::Manual,
        },
    )?;
    store.update_notes(&first.id, "Launch checklist\n• Validate the complete session flow\n• Keep personal data on this device\n• Review the next milestone on Friday")?;
    store.stop_session(&first.id)?;
    store.save_session(&first.id)?;
    let current = store.start_session("[Fixture] Design review", None)?;
    store.update_notes(
        &current.id,
        "Discuss the new onboarding experience.\nWrite down decisions and follow-up tasks here.",
    )?;
    store.ingest_knowledge("[Fixture] Team handbook", "fixture", "The team reviews product changes every Friday. Meeting notes should include decisions, owners, and next steps.")?;
    Ok(if view == "session" || view == "overlay" {
        Some(current.id)
    } else {
        None
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_commits_to_disk_and_resets_runtime_only_after_validation() -> Result<()> {
        let directory =
            std::env::temp_dir().join(format!("cueglint-restore-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory)?;
        let database = directory.join("fixture.sqlite3");
        let backup = directory.join("fixture.json");
        let invalid = directory.join("invalid.json");
        let mut source = Store::open_in_memory()?;
        let id = seed_fixture(&mut source, "native-v2-session")?.unwrap();
        let mut settings = source.settings()?;
        settings.answer_provider = ProviderConfig {
            provider: cueglint_core::ProviderId::OpenAi,
            model_id: "fixture-answer".into(),
            base_url: None,
        };
        settings.microphone_enabled = false;
        settings.screen_enabled = true;
        settings.screen_input = cueglint_core::ScreenInput::Window;
        source.update_settings(settings.clone())?;
        source.update_notes(&id, "Notes from the fixture archive.")?;
        let json = source.backup_json()?;
        cueglint_core::files::write_private_export(&backup, &json, false)?;
        cueglint_core::files::write_private_export(&invalid, "{}", false)?;

        let mut store = Store::open(&database)?;
        let mut selected = seed_fixture(&mut store, "native-v2-session")?;
        assert_eq!(selected.as_deref(), Some(id.as_str()));
        store.update_notes(&id, "Keep these edits until restore succeeds.")?;
        let mut runtime = Runtime::new(&store, true, egui::Context::default())?;
        let imports = knowledge::Imports::default();
        for (path, replace) in [(invalid.clone(), true), (backup.clone(), false)] {
            assert!(
                restore_backup(
                    &mut store,
                    &mut selected,
                    &mut runtime,
                    &imports,
                    path,
                    replace
                )
                .is_err()
            );
            assert_eq!(selected.as_deref(), Some(id.as_str()));
            assert_eq!(
                store.session(&id)?.notes,
                "Keep these edits until restore succeeds."
            );
            assert!(!runtime.snapshot.screen_enabled);
        }
        assert_eq!(std::fs::read_to_string(invalid)?, "{}");

        let Response::Restored(Ok(state)) = restore_backup(
            &mut store,
            &mut selected,
            &mut runtime,
            &imports,
            backup.clone(),
            true,
        )?
        else {
            panic!("committed restore should return the replacement snapshot");
        };
        settings.answer_provider = ProviderConfig::default();
        assert!(selected.is_none() && state.selected.is_none());
        assert_eq!(state.settings, settings);
        assert_eq!(state.runtime.answer_provider, settings.answer_provider);
        assert_eq!(
            state.runtime.screen_input,
            cueglint_core::ScreenInput::Window
        );
        assert!(state.runtime.screen_enabled);
        assert!(state.runtime.capture.is_none() && state.runtime.window_picker.is_none());
        assert!(state.runtime.provider.is_none() && state.runtime.generation.is_none());
        assert_eq!(std::fs::read_to_string(backup)?, json);

        drop(store);
        let reopened = Store::open(&database)?;
        assert_eq!(
            reopened.session(&id)?.notes,
            "Notes from the fixture archive."
        );
        assert_eq!(reopened.settings()?, settings);
        drop(reopened);
        std::fs::remove_dir_all(directory)?;
        Ok(())
    }

    #[test]
    fn a_cancelled_document_import_still_blocks_restore_until_the_slot_is_reaped() -> Result<()> {
        let mut store = Store::open_in_memory()?;
        let mut selected = None;
        let mut runtime = Runtime::new(&store, true, egui::Context::default())?;
        let mut imports = knowledge::Imports::default();
        let command = ImportCommand::new(PathBuf::new());
        imports.start(command.clone(), || {})?;
        command.cancel();

        let error = restore_backup(
            &mut store,
            &mut selected,
            &mut runtime,
            &imports,
            PathBuf::new(),
            true,
        )
        .err()
        .expect("restore must check the import slot before opening the backup");
        assert!(error.to_string().contains("document import"));
        Ok(())
    }

    #[test]
    fn failed_import_snapshot_does_not_start_or_occupy_the_parser_slot() {
        let store = Store::open_in_memory().unwrap();
        let mut imports = knowledge::Imports::default();
        let error = start_import(
            &store,
            Some("missing-session"),
            &RuntimeSnapshot::default(),
            &mut imports,
            ImportCommand::new(PathBuf::from("unread-after-snapshot.txt")),
            || {},
        )
        .err()
        .expect("a missing selected session must fail the initial snapshot");
        assert_eq!(error.to_string(), "session missing-session does not exist");

        let cancelled = ImportCommand::new(PathBuf::new());
        cancelled.cancel();
        let error = imports.start(cancelled, || {}).unwrap_err();
        assert!(
            error.to_string().contains("cancelled"),
            "failed snapshot must leave the import slot free: {error}"
        );
    }

    #[test]
    fn busy_capture_rejects_asr_changes_before_saving_settings() {
        let mut store = Store::open_in_memory().unwrap();
        let session = store
            .start_session("Frozen audio settings", None)
            .unwrap()
            .id;
        store.update_notes(&session, "Keep human notes.").unwrap();
        let mut selected = Some(session.clone());
        let mut runtime = Runtime::new(&store, true, egui::Context::default()).unwrap();
        let original = store.settings().unwrap();
        let mut changed = original.clone();
        changed.transcription_backend = cueglint_core::TranscriptionBackend::Metal;
        changed.transcription_language = "pt".into();
        changed.microphone_device_id = Some("coreaudio:original-input".into());
        changed.display_id = Some(42);
        for state in ["initializing", "listening", "finalizing"] {
            runtime.snapshot.capture = Some(crate::runtime::CaptureStatus {
                session_id: session.clone(),
                state: state.into(),
                dropped_packets: 0,
                device_errors: 0,
            });
            let error = execute(
                &mut store,
                &mut selected,
                &mut runtime,
                Command::Settings(changed.clone()),
            )
            .unwrap_err();
            assert!(error.to_string().contains("stop capture"));
            assert_eq!(store.settings().unwrap(), original);
            assert_eq!(store.session(&session).unwrap().notes, "Keep human notes.");
        }
        runtime.snapshot.capture = None;
        execute(
            &mut store,
            &mut selected,
            &mut runtime,
            Command::Settings(changed.clone()),
        )
        .unwrap();
        assert_eq!(store.settings().unwrap(), changed);
        assert_eq!(
            runtime.snapshot.capture_config,
            cueglint_local::audio::CaptureConfig::from(&changed)
        );
        assert_eq!(
            runtime.snapshot.transcription_backend,
            cueglint_core::TranscriptionBackend::Metal
        );
        assert_eq!(runtime.snapshot.transcription_language, "pt");
    }

    #[test]
    fn native_v2_restores_fixed_sessions_and_real_commands_target_the_restored_store() {
        for view in crate::FIXTURE_VIEWS {
            let name = format!("native-v2-{view}");
            let mut first = Store::open_in_memory().unwrap();
            let mut second = Store::open_in_memory().unwrap();
            let mut selected = seed_fixture(&mut first, &name).unwrap();
            assert_eq!(selected, seed_fixture(&mut second, &name).unwrap());
            assert_eq!(first.sessions("").unwrap(), second.sessions("").unwrap());
            assert_eq!(first.settings().unwrap(), second.settings().unwrap());
            assert_eq!(first.modes().unwrap(), second.modes().unwrap());
            assert_eq!(first.knowledge().unwrap(), second.knowledge().unwrap());
            let sessions = first.sessions("").unwrap();
            assert_eq!(sessions.len(), 4);
            assert!(sessions.iter().all(|session| {
                session.notes_revision == 0
                    && session.notes_job.is_none()
                    && session.generated_notes.is_none()
                    && session
                        .requests
                        .iter()
                        .all(|request| request.provider.provider.is_local())
            }));
            assert!(
                sessions
                    .iter()
                    .all(|session| session.id.starts_with("native-v2-")
                        && session.title.starts_with("[Fixture v2]")
                        && session.created_at < 1_800_000_000_000)
            );
            if let Some(id) = selected.clone() {
                let mut runtime = Runtime::new(&first, true, egui::Context::default()).unwrap();
                execute(
                    &mut first,
                    &mut selected,
                    &mut runtime,
                    Command::Notes(id.clone(), "Original v2 edit".into()),
                )
                .unwrap();
                execute(
                    &mut first,
                    &mut selected,
                    &mut runtime,
                    Command::Rename(id.clone(), "[Fixture v2] Edited title".into()),
                )
                .unwrap();
                assert_eq!(first.session(&id).unwrap().notes, "Original v2 edit");
                assert_eq!(first.session(&id).unwrap().notes_revision, 1);
                assert_eq!(
                    first.session(&id).unwrap().title,
                    "[Fixture v2] Edited title"
                );
                assert_eq!(
                    snapshot(&first, selected.as_deref(), &runtime.snapshot)
                        .unwrap()
                        .selected
                        .unwrap()
                        .id,
                    id
                );
            }
        }
    }

    #[test]
    fn fresh_text_question_uses_the_selected_mode_and_existing_sessions_keep_theirs() {
        let mut store = Store::open_in_memory().unwrap();
        let document = store
            .ingest_knowledge("Attached", "attached.txt", "Marker Alder-731.")
            .unwrap();
        store
            .ingest_knowledge("Other", "other.txt", "Marker Birch-993.")
            .unwrap();
        let mut mode = Mode::new("Original workshop", "Use the selected reference files.");
        mode.knowledge_document_ids = Some(vec![document.id.clone()]);
        let mode = store.upsert_mode(mode).unwrap();
        let fresh = question_session(&mut store, None, Some(&mode.id)).unwrap();
        assert_eq!(
            store.session(&fresh).unwrap().status,
            SessionStatus::Stopped
        );
        let request = store.begin_request(&fresh, "Which marker?").unwrap();
        assert_eq!(request.context.mode, Some(mode));
        assert_eq!(request.context.knowledge.len(), 1);
        assert_eq!(request.context.knowledge[0].document_id, document.id);

        let meeting = store
            .start_session("Separate meeting", Some("meeting"))
            .unwrap();
        let next = question_session(&mut store, None, Some("interview")).unwrap();
        assert_ne!(next, fresh);
        assert_ne!(next, meeting.id);
        let next = store.session(&next).unwrap();
        assert_eq!(next.status, SessionStatus::Stopped);
        assert_eq!(next.mode_id.as_deref(), Some("interview"));
        assert!(next.transcript.is_empty() && next.requests.is_empty());
        assert_eq!(
            question_session(&mut store, Some(fresh.clone()), Some("general")).unwrap(),
            fresh
        );
        assert_eq!(store.session(&meeting.id).unwrap(), meeting);
        assert!(store.start_session("Conflicting meeting", None).is_err());
        assert_eq!(store.sessions("").unwrap().len(), 3);
    }

    #[test]
    fn new_chat_cancels_the_question_and_keeps_the_meeting() {
        let mut store = Store::open_in_memory().unwrap();
        let meeting = store.start_session("Original meeting", None).unwrap();
        let chat = store.start_chat(Some("interview")).unwrap();
        let request = store.begin_request(&chat.id, "Old prompt").unwrap();
        let mut selected = Some(chat.id.clone());
        let mut runtime = Runtime::new(&store, true, egui::Context::default()).unwrap();
        runtime.snapshot.generation = Some(crate::runtime::GenerationStatus {
            session_id: chat.id.clone(),
            request_id: request.id.clone(),
            state: "generating".into(),
            text: "Old partial answer".into(),
        });
        runtime.snapshot.capture = Some(crate::runtime::CaptureStatus {
            session_id: meeting.id.clone(),
            state: "listening".into(),
            dropped_packets: 0,
            device_errors: 0,
        });

        execute(&mut store, &mut selected, &mut runtime, Command::NewChat).unwrap();

        assert!(selected.is_none());
        let generation = runtime.snapshot.generation.as_ref().unwrap();
        assert_eq!(generation.state, "cancelled");
        assert!(generation.text.is_empty());
        assert!(
            !store
                .complete_request(&chat.id, &request.id, "Late answer")
                .unwrap()
        );
        assert_eq!(store.session(&meeting.id).unwrap(), meeting);
        let capture = runtime.snapshot.capture.as_ref().unwrap();
        assert_eq!(capture.session_id, meeting.id);
        assert_eq!(capture.state, "listening");
        assert_eq!(store.sessions("").unwrap().len(), 2);

        let notice = execute(
            &mut store,
            &mut selected,
            &mut runtime,
            Command::Stop(meeting.id.clone()),
        )
        .unwrap();
        assert_eq!(notice.as_deref(), Some("Session saved locally."));
        assert!(store.session(&meeting.id).unwrap().saved_at.is_some());
    }

    #[test]
    fn answer_preflight_failure_does_not_create_an_empty_chat() {
        let mut store = Store::open_in_memory().unwrap();
        let mut selected = None;
        let mut runtime = Runtime::new(&store, true, egui::Context::default()).unwrap();
        assert!(
            execute(
                &mut store,
                &mut selected,
                &mut runtime,
                Command::Ask {
                    session: None,
                    prompt: "Preserve this question".into(),
                    new_chat_mode: Some("general".into()),
                }
            )
            .is_err()
        );
        assert!(selected.is_none());
        assert!(store.sessions("").unwrap().is_empty());
    }

    #[test]
    fn fixture_is_labelled_and_keeps_saved_session() {
        let mut store = Store::open_in_memory().unwrap();
        let current = seed_fixture(&mut store, "session").unwrap().unwrap();
        let state = snapshot(&store, Some(&current), &RuntimeSnapshot::default()).unwrap();
        assert!(
            state
                .sessions
                .iter()
                .all(|session| session.title.starts_with("[Fixture]"))
        );
        assert!(
            state
                .sessions
                .iter()
                .any(|session| session.saved_at.is_some())
        );
        assert!(!state.selected.unwrap().notes.is_empty());
    }

    #[test]
    fn markdown_fixtures_seed_labelled_completed_answers_without_capture_or_inference() {
        for view in ["markdown", "markdown-long"] {
            let mut store = Store::open_in_memory().unwrap();
            let id = seed_fixture(&mut store, view).unwrap().unwrap();
            let session = store.session(&id).unwrap();
            assert!(session.title.starts_with("[Fixture]"));
            assert_eq!(session.status, SessionStatus::Stopped);
            assert!(session.saved_at.is_some() && session.active_request_id.is_none());
            assert!(session.transcript.is_empty());
            assert_eq!(session.requests.len(), 1);
            assert_eq!(
                session.requests[0].status,
                cueglint_core::RequestStatus::Completed
            );
            assert_eq!(session.suggestions.len(), 1);
            let answer = &session.suggestions[0];
            assert!(answer.text.starts_with("[Fixture]"));
            assert!(answer.inference.is_none());
            if view == "markdown-long" {
                assert_eq!(
                    answer
                        .text
                        .lines()
                        .filter(|line| line
                            .split_once(". ")
                            .is_some_and(|(number, _)| number.parse::<usize>().is_ok()))
                        .count(),
                    80
                );
                assert!(answer.text.contains("80. Original item 80:"));
            } else {
                assert!(answer.text.contains("```rust\nlet original = 7;\n```"));
                assert!(answer.text.contains("https://example.test/fixture"));
            }
            assert_eq!(store.sessions("").unwrap().len(), 1);
        }
    }
}
