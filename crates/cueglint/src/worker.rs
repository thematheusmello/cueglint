use std::{
    fs,
    path::PathBuf,
    sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
    thread,
    time::Duration,
};

pub use crate::runtime::RuntimeSnapshot;
use crate::runtime::{EndAction, Runtime};
use anyhow::{Context, Result};
use cueglint_core::{
    ExportFormat, KnowledgeDocument, Mode, Session, SessionStatus, Settings, Speaker, Store,
    TranscriptSource, TranscriptUpdate,
};
use eframe::egui;

pub struct Snapshot {
    pub sessions: Vec<Session>,
    pub selected: Option<Session>,
    pub modes: Vec<Mode>,
    pub knowledge: Vec<KnowledgeDocument>,
    pub settings: Settings,
    pub runtime: RuntimeSnapshot,
}

pub enum Command {
    Start { title: String, mode: Option<String> },
    Open(String),
    Pause(String),
    Resume(String),
    Stop(String),
    Save(String),
    Mute(String, bool),
    Delete(String),
    Notes(String, String),
    Transcript(String, String, Speaker),
    Ask(Option<String>, String),
    SessionMode(String, Option<String>),
    Rename(String, String),
    Mode(Mode),
    DeleteMode(String),
    Import(PathBuf),
    DeleteKnowledge(String),
    Settings(Settings),
    Export(String, PathBuf),
    InstallModel(String),
    Cancel,
    CancelInstall,
    CaptureScreen,
    RefreshPermissions,
    OpenPrivacySettings,
    ScreenEnabled(bool),
}

pub enum Response {
    State(Snapshot, Option<String>),
    Error(String),
    Screenshot(Result<PathBuf, String>),
    Runtime(RuntimeSnapshot),
    Update(Snapshot, Option<String>),
    Notice(String, bool),
}

pub struct Worker {
    pub commands: SyncSender<Command>,
    pub responses: Receiver<Response>,
    pub screenshot: SyncSender<(PathBuf, egui::ColorImage)>,
}

impl Worker {
    pub fn spawn(database: Option<PathBuf>, fixture: Option<String>, ctx: egui::Context) -> Self {
        let (commands, jobs) = mpsc::sync_channel(16);
        let (replies, responses) = mpsc::sync_channel(16);
        let (screenshot, images) = mpsc::sync_channel::<(PathBuf, egui::ColorImage)>(1);
        let image_replies = replies.clone();
        let image_ctx = ctx.clone();
        thread::Builder::new()
            .name("cueglint-screenshot".into())
            .spawn(move || {
                while let Ok((path, image)) = images.recv() {
                    let bytes: Vec<u8> = image
                        .pixels
                        .iter()
                        .flat_map(|pixel| pixel.to_array())
                        .collect();
                    let result = image::save_buffer_with_format(
                        &path,
                        &bytes,
                        image.size[0] as u32,
                        image.size[1] as u32,
                        image::ColorType::Rgba8,
                        image::ImageFormat::Png,
                    )
                    .map(|()| path)
                    .map_err(|error| error.to_string());
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
                loop {
                    while let Some(event) = runtime.try_event() {
                        let response = match runtime.apply(&mut store, event) {
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

fn snapshot(store: &Store, selected: Option<&str>, runtime: &RuntimeSnapshot) -> Result<Snapshot> {
    Ok(Snapshot {
        sessions: store.sessions("")?,
        selected: selected.map(|id| store.session(id)).transpose()?,
        modes: store.modes()?,
        knowledge: store.knowledge()?,
        settings: store.settings()?,
        runtime: runtime.clone(),
    })
}

fn execute(
    store: &mut Store,
    selected: &mut Option<String>,
    runtime: &mut Runtime,
    command: Command,
) -> Result<Option<String>> {
    let mut notice = None;
    match command {
        Command::Start { title, mode } => {
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
            runtime.cancel(store)?;
            store.session(&id)?;
            *selected = Some(id);
        }
        Command::Pause(id) => {
            if !runtime.end_capture(&id, EndAction::Pause)? {
                store.pause_session(&id)?;
            }
        }
        Command::Resume(id) => {
            store.resume_session(&id)?;
            if let Err(error) = runtime.start_capture(store, &id) {
                store.pause_session(&id)?;
                return Err(error);
            }
        }
        Command::Stop(id) => {
            runtime.cancel(store)?;
            if !runtime.end_capture(&id, EndAction::Stop)? {
                store.stop_session(&id)?;
                store.save_session(&id)?;
            }
            notice = Some(
                "Capture stopped. Final audio is finalized before the session is saved.".into(),
            );
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
            runtime.cancel(store)?;
            store.delete_session(&id)?;
            if selected.as_ref() == Some(&id) {
                *selected = None;
            }
        }
        Command::Notes(id, notes) => {
            store.update_notes(&id, &notes)?;
        }
        Command::Transcript(id, text, speaker) => {
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
        Command::Ask(id, prompt) => {
            let id = if let Some(id) = id {
                id
            } else if let Some(active) = store
                .sessions("")?
                .into_iter()
                .find(|session| session.status != SessionStatus::Stopped)
            {
                active.id
            } else {
                store.start_session("New chat", None)?.id
            };
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
        Command::Mode(mode) => {
            runtime.cancel(store)?;
            store.upsert_mode(mode)?;
            notice = Some("Mode saved.".into());
        }
        Command::DeleteMode(id) => {
            runtime.cancel(store)?;
            store.delete_mode(&id)?;
        }
        Command::Import(path) => {
            let length = fs::metadata(&path)
                .with_context(|| format!("Cannot read {}", path.display()))?
                .len();
            anyhow::ensure!(
                length <= 2 * 1024 * 1024,
                "Choose a text or Markdown document under 2 MiB"
            );
            use std::io::Read;
            let mut bytes = Vec::new();
            fs::File::open(&path)?
                .take(2 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            anyhow::ensure!(bytes.len() <= 2 * 1024 * 1024, "Document exceeds 2 MiB");
            let text = String::from_utf8(bytes)
                .context("Knowledge documents must be UTF-8 text or Markdown")?;
            let title = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Document");
            store.ingest_knowledge(title, &path.to_string_lossy(), &text)?;
            notice = Some("Document added to your local knowledge.".into());
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
        Command::Cancel => runtime.cancel(store)?,
        Command::CancelInstall => runtime.cancel_install(),
        Command::CaptureScreen => {
            runtime.refresh_permissions();
            runtime.preview_screen(store)?;
        }
        Command::RefreshPermissions => runtime.refresh_permissions(),
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
    }
    Ok(notice)
}

fn seed_fixture(store: &mut Store, view: &str) -> Result<Option<String>> {
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
}
