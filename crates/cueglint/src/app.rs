use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use cueglint_core::{
    ExportFormat, GenerationRequest, MeetingContext, Mode, ScreenInput, ScreenSource, Session,
    SessionStatus, Speaker, TranscriptionBackend,
    provider::{CredentialRef, CredentialStatus, ProviderConfig, ProviderId},
    utc_date,
};
use cueglint_local::{asr, models, permissions::Status as PermissionStatus};
use cueglint_providers::Secret;
use cueglint_providers::calendar::{
    CalendarConnection, CalendarDateTime, CalendarEvent, CalendarOperation, CalendarRevocation,
    EventTime,
};
use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Frame, Id, Layout, Margin, Pos2, Rect, RichText,
    Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2, ViewportCommand, ViewportId, pos2, vec2,
};
use zeroize::Zeroize;

use crate::legacy_fonts::{DETAIL_TITLE_FONT, LegacyFonts};
use crate::markdown;
use crate::reply_window::{Navigation as ReplyNavigation, ReplyWindow};
use crate::runtime::{PickerPhase, ProviderAction, RuntimeSnapshot};
use crate::screenshot;
use crate::shortcuts::{self, Action, Chord};
use crate::{
    Options,
    worker::{CalendarCommand, Command, ImportCommand, Response, Snapshot, Worker},
};
#[path = "app_highlights.rs"]
mod highlights;
use highlights::HighlightsUi;
#[path = "app_backup.rs"]
mod backup;
use backup::BackupReview;
#[cfg(test)]
#[path = "app_backup_restore_tests.rs"]
mod backup_restore_tests;
#[path = "calendar_reminders.rs"]
mod calendar_reminders;
#[path = "app_capture_inputs.rs"]
mod capture_inputs;
#[path = "app_calendar_reminders.rs"]
mod reminder_ui;
use calendar_reminders::CalendarReminders;
#[path = "app_scoped_screen.rs"]
mod scoped_screen;
use scoped_screen::WindowPickerUi;
#[cfg(test)]
#[path = "app_highlights_tests.rs"]
mod highlights_tests;
#[cfg(test)]
#[path = "app_history_tests.rs"]
mod history_tests;
#[cfg(test)]
#[path = "app_knowledge_tests.rs"]
mod knowledge_tests;

const BG: Color32 = Color32::from_rgb(44, 40, 41);
const SIDEBAR: Color32 = Color32::from_rgb(49, 45, 46);
const CARD: Color32 = Color32::from_rgb(53, 49, 50);
const SELECTED: Color32 = Color32::from_rgb(69, 65, 66);
const TEXT: Color32 = Color32::from_rgb(244, 242, 243);
const MUTED: Color32 = Color32::from_rgb(174, 169, 172);
const LINE: Color32 = Color32::from_rgb(67, 63, 64);
const BLUE: Color32 = Color32::from_rgb(57, 134, 204);
const TEAL: Color32 = Color32::from_rgb(149, 220, 204);
const DETAIL_CHIP_FILL: Color32 = Color32::from_gray(37);
const SETTINGS_HISTORY_LIMIT: usize = 64;
fn overlay_id() -> ViewportId {
    ViewportId::from_hash_of("cueglint-assistant")
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    General,
    Calendar,
    Modes,
    Knowledge,
    Shortcuts,
    Security,
    Billing,
    About,
}

impl Page {
    fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Calendar => "Calendar",
            Self::Modes => "Modes",
            Self::Knowledge => "Knowledge",
            Self::Shortcuts => "Keyboard Shortcuts",
            Self::Security => "Security",
            Self::Billing => "Billing",
            Self::About => "About",
        }
    }
    fn icon(self) -> Icon {
        match self {
            Self::General => Icon::Gear,
            Self::Calendar => Icon::Calendar,
            Self::Modes => Icon::Grid,
            Self::Knowledge => Icon::File,
            Self::Shortcuts => Icon::Keyboard,
            Self::Security => Icon::Lock,
            Self::Billing => Icon::Card,
            Self::About => Icon::Info,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum OverlayPanel {
    Compact,
    History,
    Chat,
    Detail,
}
#[derive(Clone, Copy, PartialEq)]
enum ChatTab {
    Transcript,
    Notes,
}

#[derive(Clone, Debug, PartialEq)]
enum Acknowledgement {
    Calendar,
    Highlights(crate::worker::HighlightsCommand),
    Import(String),
    Cancel,
    Other,
    Notes,
    Restore,
    Switch,
    NewChat,
    Question,
    NewQuestion,
    ActivateMode(String),
    ProviderKey(CredentialRef, String),
}

struct QuestionSubmission {
    draft: Option<String>,
    prompt: String,
    session_id: Option<String>,
    previous_requests: Vec<String>,
    previous_generation: Option<String>,
    acknowledged: bool,
    window: bool,
}

#[derive(Clone, Copy)]
enum ScreenshotStage {
    Initialized,
    StateReady,
    RootUi,
    FontsReady,
    ContentReady,
    Requested,
    EventReceived,
    Queued,
    WorkerError,
    SaveError,
    QueueError,
}

impl ScreenshotStage {
    fn label(self) -> &'static str {
        match self {
            Self::Initialized => "initialized",
            Self::StateReady => "state_ready",
            Self::RootUi => "root_ui",
            Self::FontsReady => "fonts_ready",
            Self::ContentReady => "content_ready",
            Self::Requested => "requested",
            Self::EventReceived => "event_received",
            Self::Queued => "queued",
            Self::WorkerError => "worker_error",
            Self::SaveError => "save_error",
            Self::QueueError => "queue_error",
        }
    }
}

#[path = "app_releases.rs"]
mod releases;

pub struct Cueglint {
    fonts: LegacyFonts,
    worker: Worker,
    state: Option<Snapshot>,
    runtime: RuntimeSnapshot,
    answer_previews: markdown::Previews,
    reply_window: ReplyWindow,
    reply_navigation: Option<(String, ReplyNavigation)>,
    model_profiles: Vec<cueglint_local::models::ModelProfile>,
    pending: usize,
    acknowledgement_kinds: VecDeque<Acknowledgement>,
    page: Page,
    settings_back: VecDeque<Page>,
    settings_forward: VecDeque<Page>,
    settings_search: String,
    calendar_client_id: String,
    calendar_show_more: bool,
    calendar_reminders: CalendarReminders,
    release_check: releases::ReleaseCheck,
    calendar_reminder_fixture_ready: bool,
    reveal_permissions: bool,
    reveal_provider: bool,
    provider_fixture_caption: Option<String>,
    provider_fixture_ready: bool,
    shortcuts: shortcuts::Manager,
    recording_shortcut: Option<Action>,
    retry_shortcut: Option<Action>,
    settings_visible: bool,
    move_delta: Vec2,
    response_scroll: f32,
    overlay_open: bool,
    focus_assistant: bool,
    overlay_panel: OverlayPanel,
    chat_tab: ChatTab,
    history_search: String,
    question: String,
    question_submission: Option<QuestionSubmission>,
    window_picker: Option<WindowPickerUi>,
    notes: String,
    notes_dirty: bool,
    notes_saving: bool,
    notes_failed: bool,
    highlights: HighlightsUi,
    after_notes: Option<Command>,
    notes_edited: Instant,
    close_after_save: bool,
    quitting: bool,
    quit_started: bool,
    selected_mode: Option<String>,
    mode_menu: bool,
    mode_draft: Option<Mode>,
    import_path: String,
    knowledge_import: Option<ImportCommand>,
    model_path: String,
    model_import_path: String,
    provider_draft: Option<ProviderConfig>,
    provider_key: String,
    provider_key_submission: Option<(CredentialRef, String)>,
    backup_path: String,
    backup_review: Option<BackupReview>,
    export_path: String,
    transcript: String,
    speaker_you: bool,
    session_title: String,
    notice: Option<(String, bool)>,
    fixture: Option<String>,
    screenshot: Option<PathBuf>,
    screenshot_requested: bool,
    screenshot_request: Option<screenshot::Request>,
    screenshot_trace: Option<u16>,
    screenshot_adapter: Option<serde_json::Value>,
    ready_at: Option<Instant>,
    delete_session: Option<String>,
    delete_document: Option<String>,
}

impl Cueglint {
    pub fn new(cc: &eframe::CreationContext<'_>, options: Options) -> Self {
        let fonts = configure(&cc.egui_ctx, cc.wgpu_render_state.is_some());
        let screenshot_trace = options.screenshot.as_ref().map(|_| 0);
        let screenshot_request = options
            .screenshot
            .as_ref()
            .map(|_| screenshot::Request::new(options.fixture.clone()));
        let overlay_open = options.assistant_on_start();
        let focus_assistant = overlay_open && options.fixture.is_none();
        let profiles = cueglint_local::models::profiles();
        let profile_error = profiles
            .as_ref()
            .err()
            .map(|error| (format!("Invalid bundled model manifest: {error:#}"), true));
        let model_profiles = profiles.unwrap_or_default();
        let provider_preview = (options.fixture.as_deref() == Some("provider-v2"))
            .then(crate::provider_fixture)
            .transpose();
        let provider_error = provider_preview
            .as_ref()
            .err()
            .map(|error| (format!("Invalid provider fixture: {error:#}"), true));
        let (provider_draft, provider_fixture_caption) = provider_preview.ok().flatten().unzip();
        let mut calendar_reminders = if options.fixture.is_none() {
            reminder_ui::restore(cc.storage)
        } else {
            CalendarReminders::default()
        };
        if options.fixture.as_deref() == Some(crate::calendar_fixture::REMINDER) {
            calendar_reminders.set_enabled(true);
        }
        let worker = Worker::spawn(
            options.database,
            options.fixture.clone(),
            cc.egui_ctx.clone(),
        );
        let overlay_panel = if matches!(
            options.fixture.as_deref().and_then(crate::fixture_view),
            Some("markdown" | "markdown-long")
        ) {
            OverlayPanel::Chat
        } else if options.fixture.as_deref().and_then(crate::fixture_view) == Some("session") {
            OverlayPanel::Detail
        } else if options.fixture.as_deref().and_then(crate::fixture_view) == Some("history") {
            OverlayPanel::History
        } else {
            OverlayPanel::Compact
        };
        let screenshot_adapter = options.screenshot.as_ref().and_then(|_| cc.wgpu_render_state.as_ref()).map(|state| {
            let adapter = state.adapter.get_info();
            serde_json::json!({"name":adapter.name,"vendor":adapter.vendor,"device":adapter.device,"device_type":format!("{:?}",adapter.device_type),"backend":format!("{:?}",adapter.backend)})
        });
        let mut app = Self {
            fonts,
            worker,
            state: None,
            runtime: RuntimeSnapshot::default(),
            answer_previews: markdown::Previews::new(cc.egui_ctx.clone(), overlay_id()),
            reply_window: ReplyWindow::default(),
            reply_navigation: None,
            model_profiles,
            pending: 1,
            acknowledgement_kinds: VecDeque::from([Acknowledgement::Other]),
            page: if options.fixture.as_deref() == Some(crate::calendar_fixture::SETTINGS) {
                Page::Calendar
            } else {
                Page::General
            },
            settings_back: VecDeque::new(),
            settings_forward: VecDeque::new(),
            settings_search: String::new(),
            calendar_client_id: String::new(),
            calendar_show_more: false,
            calendar_reminders,
            release_check: releases::ReleaseCheck::default(),
            calendar_reminder_fixture_ready: false,
            reveal_permissions: false,
            reveal_provider: provider_fixture_caption.is_some(),
            provider_fixture_caption,
            provider_fixture_ready: false,
            shortcuts: shortcuts::Manager::disabled(cc.egui_ctx.clone()),
            recording_shortcut: None,
            retry_shortcut: None,
            settings_visible: !overlay_open,
            move_delta: Vec2::ZERO,
            response_scroll: 0.0,
            focus_assistant,
            overlay_open,
            overlay_panel,
            chat_tab: if matches!(
                options.fixture.as_deref(),
                Some("transcript-v1" | "transcript-empty-v1")
            ) {
                ChatTab::Transcript
            } else {
                ChatTab::Notes
            },
            history_search: String::new(),
            question: String::new(),
            question_submission: None,
            window_picker: None,
            notes: String::new(),
            notes_dirty: false,
            notes_saving: false,
            notes_failed: false,
            highlights: HighlightsUi::default(),
            after_notes: None,
            notes_edited: Instant::now(),
            close_after_save: false,
            quitting: false,
            quit_started: false,
            selected_mode: None,
            mode_menu: false,
            mode_draft: None,
            import_path: String::new(),
            knowledge_import: None,
            model_path: String::new(),
            model_import_path: String::new(),
            provider_draft,
            provider_key: String::new(),
            provider_key_submission: None,
            backup_path: String::new(),
            backup_review: None,
            export_path: String::new(),
            transcript: String::new(),
            speaker_you: false,
            session_title: String::new(),
            notice: provider_error.or(profile_error),
            fixture: options.fixture,
            screenshot: options.screenshot,
            screenshot_requested: false,
            screenshot_request,
            screenshot_trace,
            screenshot_adapter,
            ready_at: None,
            delete_session: None,
            delete_document: None,
        };
        app.trace_screenshot(ScreenshotStage::Initialized, &cc.egui_ctx);
        app
    }

    pub fn install_shortcuts(&mut self, manager: shortcuts::Manager) {
        self.shortcuts = manager;
    }

    fn send(&mut self, command: Command) -> bool {
        if self
            .acknowledgement_kinds
            .contains(&Acknowledgement::Restore)
            || (self.backup_restore_blocks_controls() && !matches!(&command, Command::Notes(..)))
        {
            return false;
        }
        if matches!(&command, Command::Cancel)
            && let Some(import) = &self.knowledge_import
        {
            import.cancel();
        }
        if matches!(&command, Command::Settings(_)) && self.pending > 0 {
            self.notice = Some((
                "Wait for the current operation to finish before changing settings.".into(),
                false,
            ));
            return false;
        }
        let history_question = if let Command::AskFromHistory { prompt, .. }
        | Command::AskSavedMeetings { prompt, .. } = &command
        {
            if !self.answer_ready() || self.quitting {
                self.notice = Some(("The assistant is busy. Try asking again.".into(), true));
                return false;
            }
            let mut submission = self.prepare_question_submission(None, prompt.clone(), None);
            if matches!(&command, Command::AskSavedMeetings { .. }) {
                submission.window = false;
            }
            Some(submission)
        } else {
            None
        };
        let retires_stream = matches!(
            &command,
            Command::Cancel
                | Command::Open(_)
                | Command::NewChat
                | Command::Stop(_)
                | Command::Delete(_)
                | Command::SessionMode(_, _)
                | Command::Mode { .. }
                | Command::DeleteMode(_)
                | Command::DeleteKnowledge(_)
                | Command::ScreenEnabled(_)
        ) || matches!(&command, Command::Settings(settings) if self.state.as_ref().is_some_and(|state|
            settings.answer_provider != state.settings.answer_provider
                || settings.generation_profile != state.settings.generation_profile
                || settings.model_directory != state.settings.model_directory
                || settings.display_id != state.settings.display_id
                || settings.screen_input != state.settings.screen_input
                || settings.screen_enabled != state.settings.screen_enabled));
        if (retires_stream
            || matches!(
                &command,
                Command::CancelWindowPicker | Command::Start { .. } | Command::Resume(_)
            ))
            && let Some(picker) = &self.runtime.window_picker
        {
            picker.cancel();
        }
        let kind = match &command {
            Command::Calendar(_) => Acknowledgement::Calendar,
            Command::Highlights(command) => Acknowledgement::Highlights(command.clone()),
            Command::Import(command) => Acknowledgement::Import(command.id.clone()),
            Command::Cancel => Acknowledgement::Cancel,
            Command::Notes(_, _) => Acknowledgement::Notes,
            Command::RestoreBackup { .. } => Acknowledgement::Restore,
            Command::Start { .. } | Command::Open(_) | Command::Delete(_) => {
                Acknowledgement::Switch
            }
            Command::NewChat => Acknowledgement::NewChat,
            Command::Ask { session: None, .. }
            | Command::AskFromHistory { .. }
            | Command::AskSavedMeetings { .. } => Acknowledgement::NewQuestion,
            Command::Ask {
                session: Some(_), ..
            } => Acknowledgement::Question,
            Command::Mode {
                mode,
                activate: true,
            } => Acknowledgement::ActivateMode(mode.id.clone()),
            Command::Provider(id, config, ProviderAction::Save(_)) => config
                .credential_ref()
                .map(|reference| Acknowledgement::ProviderKey(reference, id.clone()))
                .unwrap_or(Acknowledgement::Other),
            _ => Acknowledgement::Other,
        };
        match self.worker.commands.try_send(command) {
            Ok(()) => {
                if retires_stream {
                    self.answer_previews
                        .cancel_stream(self.runtime.generation.as_ref());
                }
                self.pending += 1;
                self.acknowledgement_kinds.push_back(kind);
                if let Some(submission) = history_question {
                    self.question_submission = Some(submission);
                    self.notice = None;
                }
                true
            }
            Err(error) => {
                self.notice = Some((format!("Could not queue the operation: {error}"), true));
                false
            }
        }
    }

    fn selected(&self) -> Option<&Session> {
        self.state.as_ref()?.selected.as_ref()
    }

    fn active_session(&self) -> Option<&Session> {
        self.state
            .as_ref()?
            .sessions
            .iter()
            .find(|session| session.status != SessionStatus::Stopped)
    }

    fn poll(&mut self, ctx: &egui::Context) {
        self.fonts.poll(ctx);
        let mut markdown_changed = false;
        while let Ok(response) = self.worker.responses.try_recv() {
            markdown_changed |= matches!(
                &response,
                Response::State(..)
                    | Response::Update(..)
                    | Response::Runtime(_)
                    | Response::Restored(_)
            );
            match response {
                Response::Import(id, result) => {
                    if !self
                        .knowledge_import
                        .as_ref()
                        .is_some_and(|job| job.id == id)
                    {
                        continue;
                    }
                    self.knowledge_import = None;
                    match result {
                        Ok(state) => {
                            markdown_changed = true;
                            self.apply_snapshot(
                                state,
                                Some("Document added to your local knowledge.".into()),
                                false,
                            );
                        }
                        Err(error) => self.notice = Some((error, true)),
                    }
                }
                Response::Highlights(ack) => {
                    markdown_changed = true;
                    self.acknowledge_highlights(ack);
                }
                Response::State(state, notice) => {
                    self.pending = self.pending.saturating_sub(1);
                    let acknowledgement = self.acknowledgement_kinds.pop_front();
                    if matches!(
                        acknowledgement,
                        Some(Acknowledgement::Question | Acknowledgement::NewQuestion)
                    ) {
                        if let Some(submission) = &mut self.question_submission {
                            submission.acknowledged = true;
                        }
                    }
                    self.apply_snapshot(
                        state,
                        notice,
                        acknowledgement == Some(Acknowledgement::Notes),
                    );
                    match acknowledgement {
                        Some(Acknowledgement::ActivateMode(id)) => {
                            self.selected_mode = Some(id);
                            self.notice =
                                Some(("Mode selected for your next session.".into(), false));
                        }
                        Some(Acknowledgement::NewChat) => {
                            self.question_submission = None;
                            self.question.clear();
                            self.notice = None;
                            self.show_assistant();
                            self.overlay_panel = OverlayPanel::Compact;
                        }
                        _ => {}
                    }
                }
                Response::Restored(result) => self.backup_restored(result, ctx),
                Response::Update(state, notice) => self.apply_snapshot(state, notice, false),
                Response::Runtime(runtime) => self.runtime = runtime,
                Response::Notice(message, error) => self.notice = Some((message, error)),
                Response::Error(error) => {
                    self.trace_screenshot(ScreenshotStage::WorkerError, ctx);
                    self.pending = self.pending.saturating_sub(1);
                    let acknowledgement = self.acknowledgement_kinds.pop_front();
                    if let Some(Acknowledgement::Import(id)) = &acknowledgement
                        && self
                            .knowledge_import
                            .as_ref()
                            .is_some_and(|job| &job.id == id)
                    {
                        self.knowledge_import.take().unwrap().cancel();
                    }
                    if matches!(
                        acknowledgement,
                        Some(Acknowledgement::Question | Acknowledgement::NewQuestion)
                    ) {
                        self.question_submission = None;
                    }
                    if acknowledgement == Some(Acknowledgement::Notes) {
                        self.notes_failed = true;
                        self.notes_saving = false;
                        self.after_notes = None;
                        self.highlights.append_after_notes = None;
                    }
                    if let Some(Acknowledgement::ProviderKey(reference, id)) =
                        acknowledgement.as_ref()
                        && self.provider_key_submission.as_ref().is_some_and(
                            |(pending_ref, pending)| pending_ref == reference && pending == id,
                        )
                    {
                        self.provider_key_submission = None;
                    }
                    self.close_after_save = false;
                    self.notice = Some((error, true));
                }
                Response::Screenshot(result) => match result {
                    Ok(receipt) => {
                        println!("{}", receipt.json());
                        if self.runtime.window_picker.is_some() {
                            self.request_quit(ctx);
                        } else {
                            self.quitting = true;
                            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
                        }
                    }
                    Err(error) => {
                        self.trace_screenshot(ScreenshotStage::SaveError, ctx);
                        self.notice = Some((format!("Screenshot failed: {error}"), true));
                    }
                },
            }
        }
        if self.ready_at.is_some() {
            self.trace_screenshot(ScreenshotStage::StateReady, ctx);
        }
        if self.fonts.ready() {
            self.trace_screenshot(ScreenshotStage::FontsReady, ctx);
        }
        if self.screenshot_content_ready() {
            self.trace_screenshot(ScreenshotStage::ContentReady, ctx);
        }
        self.reconcile_question_submission();
        self.reconcile_provider_key(ctx);
        if matches!(
            self.runtime.calendar.connection,
            CalendarConnection::Connected
        ) {
            self.calendar_client_id.clear();
        }
        if self.runtime.calendar.upcoming.is_none() {
            self.calendar_show_more = false;
        }
        self.reconcile_highlights();
        self.drain_after_notes();
        markdown_changed |= self.apply_reply_navigation();
        if markdown_changed {
            self.synchronize_markdown();
        } else {
            self.answer_previews.poll();
        }
        if self.notes_dirty
            && !self.notes_saving
            && !self.notes_failed
            && self.notes_edited.elapsed() >= Duration::from_millis(500)
        {
            self.save_notes();
        }
        if self.close_after_save && self.ready_to_close() {
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
        }
        if self.notes_dirty {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if self.screenshot.is_some()
            && !self.screenshot_requested
            && self.fonts.ready()
            && self.screenshot_content_ready()
            && self
                .ready_at
                .is_some_and(|at| at.elapsed() > Duration::from_millis(700))
        {
            // shortcut: eframe 0.35 drops child screenshot commands, use primary fixture windows until child readback is supported.
            let target = ViewportId::ROOT;
            self.trace_screenshot(ScreenshotStage::Requested, ctx);
            ctx.send_viewport_cmd_to(
                target,
                ViewportCommand::Screenshot(
                    self.screenshot_request
                        .as_ref()
                        .expect("screenshot output owns a request")
                        .token(),
                ),
            );
            ctx.request_repaint_of(target);
            self.screenshot_requested = true;
        }
        if self.screenshot.is_some() {
            // GPU map callbacks need later submissions even when Settings is idle.
            ctx.request_repaint_after_for(Duration::from_millis(100), ViewportId::ROOT);
        }
        self.capture_screenshot(ctx);
    }

    fn trace_screenshot(&mut self, stage: ScreenshotStage, ctx: &egui::Context) {
        let Some(seen) = &mut self.screenshot_trace else {
            return;
        };
        let bit = 1u16 << stage as u16;
        if *seen & bit != 0 {
            return;
        }
        *seen |= bit;
        let (visible, minimized, occluded) = ctx.input(|input| {
            (
                input.viewport().visible(),
                input.viewport().minimized,
                input.viewport().occluded,
            )
        });
        // Keep stdout as one saved receipt; explicit screenshot diagnostics go to stderr.
        eprintln!(
            "{}",
            serde_json::json!({
                "event":"native_screenshot_stage", "stage":stage.label(), "fixture":self.fixture,
                "request_id":self.screenshot_request.as_ref().map(|request| request.id().to_string()),
                "font_state":self.fonts.receipt(),
                "content_ready":self.screenshot_content_ready(),
                "provider_reveal_pending":self.reveal_provider,
                "viewport": if ctx.viewport_id() == ViewportId::ROOT { "root" } else { "other" },
                "visible":visible, "minimized":minimized, "occluded":occluded,
                "pixels_per_point":ctx.pixels_per_point(), "zoom_factor":ctx.zoom_factor(),
            })
        );
    }

    fn screenshot_content_ready(&self) -> bool {
        if let Some(view @ ("transcript-v1" | "transcript-empty-v1")) = self.fixture.as_deref() {
            return self.overlay_panel == OverlayPanel::Detail
                && self.chat_tab == ChatTab::Transcript
                && self.selected().is_some_and(|s| {
                    s.id == view
                        && s.status == SessionStatus::Stopped
                        && s.transcript.is_empty() == (view == "transcript-empty-v1")
                });
        }
        if self.fixture.as_deref() == Some("provider-v2") {
            return self.provider_fixture_ready;
        }
        if self.calendar_reminder_preview() {
            return self.calendar_reminder_fixture_ready;
        }
        if self.calendar_preview() {
            let expected_view = if self.fixture.as_deref() == Some(crate::calendar_fixture::HISTORY)
            {
                self.overlay_panel == OverlayPanel::History
                    && self.runtime.calendar.upcoming.is_some()
            } else {
                self.page == Page::Calendar
            };
            return self.runtime.calendar.connection == CalendarConnection::Connected
                && expected_view;
        }
        if !matches!(
            self.fixture.as_deref().and_then(crate::fixture_view),
            Some("markdown" | "markdown-long")
        ) {
            return true;
        }
        self.selected().is_some_and(|session| {
            !session.requests.is_empty()
                && session.requests.iter().all(|request| {
                    matches!(
                        self.answer_previews.get(
                            &session.id,
                            &request.id,
                            request.context.revision
                        ),
                        markdown::Preview::Formatted(..)
                    )
                })
        })
    }

    fn apply_snapshot(&mut self, state: Snapshot, notice: Option<String>, notes_ack: bool) {
        let changed = state.selected.as_ref().map(|s| &s.id) != self.selected().map(|s| &s.id);
        if changed || !self.notes_dirty {
            self.notes = state
                .selected
                .as_ref()
                .map(|s| s.notes.clone())
                .unwrap_or_default();
            self.notes_dirty = false;
        } else if state
            .selected
            .as_ref()
            .is_some_and(|s| s.notes == self.notes)
        {
            self.notes_dirty = false;
        }
        if notes_ack {
            self.notes_saving = false;
        }
        if changed {
            self.highlights.append_after_notes = None;
            self.highlights.open = false;
            self.highlights.conflict = false;
            self.session_title = state
                .selected
                .as_ref()
                .map(|s| s.title.clone())
                .unwrap_or_default();
        }
        if self.model_path.is_empty() {
            self.model_path = state.settings.model_directory.clone().unwrap_or_default();
        }
        if self.provider_draft.is_none() {
            self.provider_draft = Some(state.settings.answer_provider.clone());
        }
        let mode_available = self
            .selected_mode
            .as_ref()
            .is_some_and(|id| state.modes.iter().any(|mode| &mode.id == id));
        if !mode_available || (changed && state.selected.is_some()) {
            self.selected_mode = state
                .selected
                .as_ref()
                .and_then(|s| s.mode_id.clone())
                .or_else(|| state.modes.first().map(|m| m.id.clone()));
        }
        self.runtime = state.runtime.clone();
        self.state = Some(state);
        self.drain_after_notes();
        if let Some(notice) = notice {
            self.notice = Some((notice, false));
        }
        self.ready_at.get_or_insert_with(Instant::now);
    }

    fn synchronize_markdown(&mut self) {
        self.reply_window.synchronize(
            self.state
                .as_ref()
                .and_then(|state| state.selected.as_ref()),
        );
        self.answer_previews.synchronize(
            self.state
                .as_ref()
                .and_then(|state| state.selected.as_ref()),
            self.runtime.generation.as_ref(),
            self.reply_window.range(),
        );
    }

    fn apply_reply_navigation(&mut self) -> bool {
        let Some((session_id, navigation)) = self.reply_navigation.take() else {
            return false;
        };
        let Some(session) = self
            .state
            .as_ref()
            .and_then(|state| state.selected.as_ref())
            .filter(|session| session.id == session_id)
        else {
            return false;
        };
        self.reply_window.navigate(session, navigation);
        self.response_scroll = 0.0;
        true
    }

    fn generating(&self) -> bool {
        self.runtime.generating()
    }

    fn model_job_busy(&self) -> bool {
        self.runtime.model_job.as_ref().is_some_and(|job| {
            ["downloading", "importing", "removing"].contains(&job.state.as_str())
        })
    }

    fn model_cleanup_pending(&self) -> bool {
        // Downloads retain resumable parts and may await network I/O; import/remove must finish local cleanup before exit.
        self.runtime
            .model_job
            .as_ref()
            .is_some_and(|job| ["importing", "removing"].contains(&job.state.as_str()))
    }

    fn answer_ready(&self) -> bool {
        self.fixture.is_none()
            && self.state.is_some()
            && self.pending == 0
            && self.question_submission.is_none()
            && !self.generating()
            && !self.model_job_busy()
            && !self.highlights_busy()
            && if self.runtime.answer_provider.provider.is_local() {
                self.runtime
                    .installed_profiles
                    .iter()
                    .any(|profile| profile == self.generation_profile())
            } else {
                self.runtime
                    .answer_provider
                    .validate_for_generation()
                    .is_ok()
            }
    }

    fn selected_chat_requires_local(&self) -> bool {
        self.selected().is_some_and(|session| session.history_chat)
            && !self.runtime.answer_provider.provider.is_local()
    }

    fn reconcile_question_submission(&mut self) {
        let Some(submission) = &mut self.question_submission else {
            return;
        };
        if !submission.acknowledged {
            return;
        }
        if submission.window {
            if self
                .runtime
                .window_picker
                .as_ref()
                .is_some_and(|picker| picker.question && picker.phase != PickerPhase::Retiring)
            {
                return;
            }
            if submission.previous_generation.as_deref()
                == self
                    .runtime
                    .generation
                    .as_ref()
                    .map(|generation| generation.request_id.as_str())
            {
                self.question_submission = None;
                return;
            }
        }
        let Some(generation) = &self.runtime.generation else {
            return;
        };
        if submission.previous_generation.as_deref() == Some(&generation.request_id)
            || submission
                .session_id
                .as_ref()
                .is_some_and(|id| id != &generation.session_id)
        {
            return;
        }
        submission
            .session_id
            .get_or_insert_with(|| generation.session_id.clone());
        let accepted = self.state.as_ref().is_some_and(|state| {
            state
                .selected
                .iter()
                .chain(state.sessions.iter())
                .any(|session| {
                    session.id == generation.session_id
                        && session.requests.iter().any(|request| {
                            request.id == generation.request_id
                                && request.prompt == submission.prompt
                                && !submission.previous_requests.contains(&request.id)
                        })
                })
        });
        if accepted {
            if submission.draft.as_deref() == Some(self.question.as_str()) {
                self.question.clear();
            } else if submission.draft.is_none()
                && self.overlay_panel == OverlayPanel::History
                && self.state.as_ref().is_some_and(|state| {
                    state
                        .selected
                        .as_ref()
                        .is_some_and(|session| session.id == generation.session_id)
                })
            {
                self.overlay_panel = OverlayPanel::Chat;
                self.chat_tab = ChatTab::Notes;
            }
            self.question_submission = None;
        } else if ["error", "cancelled"].contains(&generation.state.as_str()) {
            self.question_submission = None;
        }
    }

    fn save_notes(&mut self) {
        if let Some(id) = self.selected().map(|s| s.id.clone()) {
            self.notes_saving = self.send(Command::Notes(id, self.notes.clone()));
            if self.notes_saving {
                self.notes_failed = false;
            }
        }
    }

    fn start(&mut self) {
        self.start_named_session("Untitled session".into());
    }

    fn start_named_session(&mut self, title: String) {
        self.after_saving_notes(Command::Start {
            title,
            mode: self.selected_mode.clone(),
        });
        self.show_assistant();
        self.overlay_panel = OverlayPanel::Chat;
        self.chat_tab = ChatTab::Notes;
    }

    fn new_chat(&mut self) {
        self.after_saving_notes(Command::NewChat);
    }

    fn capture_busy(&self) -> bool {
        self.runtime.capture.as_ref().is_some_and(|capture| {
            ["initializing", "listening", "finalizing"].contains(&capture.state.as_str())
                || (capture.state == "error"
                    && self.state.as_ref().is_some_and(|state| {
                        state.sessions.iter().any(|session| {
                            session.id == capture.session_id
                                && session.status == SessionStatus::Recording
                        })
                    }))
        })
    }

    fn ready_to_close(&self) -> bool {
        !self.notes_dirty
            && !self.notes_saving
            && self.pending == 0
            && self.knowledge_import.is_none()
            && !self.capture_busy()
            && !self.model_cleanup_pending()
            && !self.highlights_busy()
            && self.highlights.pending.is_none()
            && self.runtime.window_picker.is_none()
    }

    fn request_quit(&mut self, ctx: &egui::Context) {
        if let Some(import) = &self.knowledge_import {
            import.cancel();
        }
        self.quitting = true;
        self.close_after_save = true;
        if !self.quit_started {
            self.quit_started = true;
            self.highlights.append_after_notes = None;
            self.after_notes = None;
            self.send(Command::Cancel);
            self.send(Command::CancelInstall);
            if let Some(id) = self.active_session().map(|session| session.id.clone()) {
                self.after_saving_notes(Command::Stop(id));
            } else if self.notes_dirty && !self.notes_saving {
                self.save_notes();
            }
        }
        ctx.send_viewport_cmd_to(
            ViewportId::ROOT,
            if self.ready_to_close() {
                ViewportCommand::Close
            } else {
                ViewportCommand::CancelClose
            },
        );
    }

    fn show_assistant(&mut self) {
        self.recording_shortcut = None;
        self.overlay_open = true;
        self.focus_assistant = true;
    }

    fn blocked_capture_permission(&self) -> Option<&'static str> {
        let blocked = |status| {
            matches!(
                status,
                PermissionStatus::Required
                    | PermissionStatus::Denied
                    | PermissionStatus::Restricted
            )
        };
        let capture_failed = self
            .runtime
            .capture
            .as_ref()
            .is_some_and(|capture| capture.state == "error");
        let settings = self.state.as_ref().map(|state| &state.settings);
        if blocked(self.runtime.permissions.screen)
            && ((self.runtime.screen_enabled && self.runtime.screen_input.uses_display())
                || (capture_failed
                    && settings.is_some_and(|settings| settings.system_audio_enabled)))
        {
            Some("Screen access needed · Settings")
        } else if capture_failed
            && blocked(self.runtime.permissions.microphone)
            && settings.is_some_and(|settings| settings.microphone_enabled)
        {
            Some("Microphone access needed · Settings")
        } else {
            None
        }
    }

    fn show_settings(&mut self, ctx: &egui::Context) {
        self.overlay_open = false;
        self.focus_assistant = false;
        self.mode_menu = false;
        self.settings_visible = true;
        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Visible(true));
        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Focus);
    }

    fn visit_settings_page(&mut self, page: Page) {
        if self.backup_restore_blocks_controls() {
            return;
        }
        self.recording_shortcut = None;
        if self.page != page {
            if self.settings_back.len() == SETTINGS_HISTORY_LIMIT {
                self.settings_back.pop_front();
            }
            self.settings_back.push_back(self.page);
            self.settings_forward.clear();
            self.page = page;
        }
    }

    fn step_settings_history(&mut self, forward: bool) {
        if self.backup_restore_blocks_controls() {
            return;
        }
        let (source, destination) = if forward {
            (&mut self.settings_forward, &mut self.settings_back)
        } else {
            (&mut self.settings_back, &mut self.settings_forward)
        };
        if let Some(page) = source.pop_back() {
            destination.push_back(self.page);
            self.page = page;
            self.recording_shortcut = None;
        }
    }

    fn dispatch_shortcut(&mut self, action: Action, ctx: &egui::Context) {
        if self.quitting || self.backup_restore_blocks_controls() {
            return;
        }
        match action {
            Action::Visibility if self.overlay_open => {
                self.overlay_open = false;
                self.focus_assistant = false;
                self.mode_menu = false;
                if self.shortcuts.status(Action::Visibility) == shortcuts::Status::Registered {
                    self.settings_visible = false;
                    ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Visible(false));
                } else {
                    self.show_settings(ctx);
                    self.notice = Some(("Settings stays open because the global visibility shortcut is unavailable. Open assistant or Stop session remains available here.".into(), false));
                }
            }
            Action::Visibility => self.show_assistant(),
            Action::NewChat => {
                if self.pending == 0 {
                    self.new_chat();
                }
            }
            Action::Settings => self.show_settings(ctx),
            Action::Session => {
                if let Some(id) = self.active_session().map(|session| session.id.clone()) {
                    self.after_saving_notes(Command::Stop(id));
                } else if self.pending == 0 {
                    self.start();
                }
            }
            Action::MoveLeft => self.move_delta.x -= 32.0,
            Action::MoveRight => self.move_delta.x += 32.0,
            Action::MoveUp => self.move_delta.y -= 32.0,
            Action::MoveDown => self.move_delta.y += 32.0,
            Action::ScrollUp if self.overlay_panel == OverlayPanel::Chat => {
                self.response_scroll += 80.0
            }
            Action::ScrollDown if self.overlay_panel == OverlayPanel::Chat => {
                self.response_scroll -= 80.0
            }
            Action::Ask => {
                self.show_assistant();
                self.ask();
            }
            _ => {}
        }
    }

    fn move_window(&mut self, ctx: &egui::Context) {
        let delta = std::mem::take(&mut self.move_delta);
        if delta == Vec2::ZERO {
            return;
        }
        let target = if self.overlay_open {
            overlay_id()
        } else if self.settings_visible {
            ViewportId::ROOT
        } else {
            return;
        };
        let position = ctx
            .input(|input| {
                input
                    .raw
                    .viewports
                    .get(&target)
                    .and_then(|viewport| viewport.outer_rect)
                    .map(|rect| rect.min)
            })
            .unwrap_or(if target == overlay_id() {
                pos2(440.0, 90.0)
            } else {
                pos2(100.0, 100.0)
            });
        ctx.send_viewport_cmd_to(target, ViewportCommand::OuterPosition(position + delta));
    }

    fn save_shortcut(&mut self, action: Action, value: Option<String>, reset: bool) {
        let Some(state) = &self.state else {
            return;
        };
        let mut settings = state.settings.clone();
        if reset {
            settings.shortcut_bindings.remove(action.id());
        } else {
            settings.shortcut_bindings.insert(action.id().into(), value);
        }
        if let Some(value) = shortcuts::binding(&settings.shortcut_bindings, action) {
            let chord = match Chord::parse(value) {
                Ok(chord) => chord,
                Err(error) => {
                    self.notice = Some((error, true));
                    return;
                }
            };
            if chord.matches(egui::Key::Q, egui::Modifiers::COMMAND) {
                self.notice = Some(("This combination is reserved for Quit.".into(), true));
                return;
            }
            if let Some(other) = Action::ALL.into_iter().find(|other| {
                *other != action
                    && shortcuts::binding(&settings.shortcut_bindings, *other)
                        .and_then(|value| Chord::parse(value).ok())
                        .as_ref()
                        == Some(&chord)
            }) {
                self.notice = Some((
                    format!(
                        "Already assigned to {}. Clear or change that shortcut first.",
                        other.label()
                    ),
                    true,
                ));
                return;
            }
        }
        if self.send(Command::Settings(settings)) {
            self.recording_shortcut = None;
        }
    }

    fn keyboard(&mut self, ctx: &egui::Context) {
        if self.backup_restore_blocks_controls() {
            return;
        }
        let events = ctx.input(|input| input.events.clone());
        for event in events {
            let egui::Event::Key {
                key,
                physical_key,
                pressed: true,
                repeat,
                modifiers,
            } = event
            else {
                continue;
            };
            let key = physical_key.unwrap_or(key);
            if matches!(
                key,
                egui::Key::ShiftLeft
                    | egui::Key::ShiftRight
                    | egui::Key::ControlLeft
                    | egui::Key::ControlRight
                    | egui::Key::AltLeft
                    | egui::Key::AltRight
                    | egui::Key::SuperLeft
                    | egui::Key::SuperRight
            ) {
                continue;
            }
            let mut consumed = false;
            if let Some(action) = self.recording_shortcut {
                consumed = true;
                if !repeat {
                    if key == egui::Key::Escape {
                        self.recording_shortcut = None;
                    } else if modifiers.is_none()
                        && (key == egui::Key::Delete
                            || (cfg!(target_os = "macos") && key == egui::Key::Backspace))
                    {
                        self.save_shortcut(action, None, false);
                    } else {
                        match Chord::record(key, modifiers) {
                            Ok(value) => self.save_shortcut(action, Some(value), false),
                            Err(error) => {
                                self.notice = Some((
                                    format!("Use Command or Control with an ordinary key. {error}"),
                                    true,
                                ))
                            }
                        }
                    }
                }
            } else if Chord::parse("CommandOrControl+Q")
                .unwrap()
                .matches(key, modifiers)
            {
                consumed = true;
                if !repeat {
                    self.request_quit(ctx);
                }
            } else if key == egui::Key::Escape && modifiers.is_none() {
                if egui::Popup::is_any_open(ctx) {
                    continue;
                }
                consumed = true;
                if !repeat {
                    self.overlay_panel = OverlayPanel::Compact;
                    self.mode_menu = false;
                }
            } else if let Some(action) = self.shortcuts.local_action(key, modifiers, repeat) {
                consumed = true;
                self.dispatch_shortcut(action, ctx);
            } else if self.shortcuts.owns_key(key, modifiers) {
                consumed = true;
            }
            if consumed {
                ctx.request_repaint();
                ctx.input_mut(|input| input.events.retain(|event| !matches!(event, egui::Event::Key { key: event_key, physical_key: event_physical, modifiers: event_modifiers, .. } if event_physical.unwrap_or(*event_key) == key && *event_modifiers == modifiers)));
            }
        }
        if self.recording_shortcut.is_some() {
            ctx.input_mut(|input| {
                input
                    .events
                    .retain(|event| !matches!(event, egui::Event::Text(_)))
            });
        }
        self.move_window(ctx);
    }

    fn after_saving_notes(&mut self, command: Command) {
        if self.backup_restore_blocks_controls() {
            return;
        }
        self.highlights.append_after_notes = None;
        self.after_notes = Some(command);
        if self.notes_dirty && !self.notes_saving {
            self.save_notes();
        }
        self.drain_after_notes();
    }

    fn prepare_question_submission(
        &self,
        session_id: Option<String>,
        prompt: String,
        draft: Option<String>,
    ) -> QuestionSubmission {
        QuestionSubmission {
            draft,
            prompt,
            session_id,
            previous_requests: self
                .state
                .as_ref()
                .into_iter()
                .flat_map(|state| state.selected.iter().chain(state.sessions.iter()))
                .flat_map(|session| session.requests.iter().map(|request| request.id.clone()))
                .collect(),
            previous_generation: self
                .runtime
                .generation
                .as_ref()
                .map(|generation| generation.request_id.clone()),
            acknowledged: false,
            window: self.runtime.screen_enabled && self.runtime.screen_input == ScreenInput::Window,
        }
    }

    fn ask(&mut self) {
        if self.question.trim().is_empty()
            || !self.answer_ready()
            || self.selected_chat_requires_local()
        {
            return;
        }
        let prompt = self.question.trim().to_owned();
        let session = self.selected().map(|session| session.id.clone());
        let mut submission = self.prepare_question_submission(
            session.clone(),
            prompt.clone(),
            Some(self.question.clone()),
        );
        if self.selected().is_some_and(|session| session.history_chat) {
            submission.window = false;
        }
        if self.send(Command::Ask {
            session,
            prompt,
            new_chat_mode: self.selected_mode.clone(),
        }) {
            self.question_submission = Some(submission);
            self.overlay_panel = OverlayPanel::Chat;
            self.chat_tab = ChatTab::Notes;
        }
    }

    fn main_window(&mut self, ui: &mut Ui) {
        if self.backup_restore_blocks_controls() {
            ui.disable();
        }
        let available = ui.max_rect();
        ui.painter().rect_filled(available, 0, BG);
        let sidebar = Rect::from_min_size(
            available.min + vec2(6.0, 6.0),
            vec2(204.0, available.height() - 12.0),
        );
        ui.painter().rect_filled(sidebar, 13, SIDEBAR);
        let mut side = ui.new_child(UiBuilder::new().max_rect(sidebar.shrink2(vec2(12.0, 0.0))));
        side.add_space(44.0);
        side.add(
            egui::TextEdit::singleline(&mut self.settings_search)
                .hint_text("Search settings")
                .desired_width(180.0)
                .margin(vec2(10.0, 6.0)),
        );
        side.add_space(18.0);
        side.horizontal(|ui| {
            mark(ui, 26.0, TEAL);
            ui.vertical(|ui| {
                ui.label(RichText::new("Cueglint").size(13.0).strong());
                ui.label(
                    RichText::new(if self.fixture.is_some() {
                        "[Fixture] Isolated preview"
                    } else {
                        "Local workspace"
                    })
                    .size(10.0)
                    .color(MUTED),
                );
            });
        });
        let mut navigation = side.new_child(UiBuilder::new().max_rect(Rect::from_min_max(
            pos2(sidebar.left() + 12.0, available.top() + 154.0),
            sidebar.max - vec2(12.0, 0.0),
        )));
        navigation.spacing_mut().item_spacing.y = 2.0;
        let pages = [
            Page::General,
            Page::Calendar,
            Page::Modes,
            Page::Knowledge,
            Page::Shortcuts,
            Page::Security,
            Page::Billing,
            Page::About,
        ];
        for page in pages {
            if !page
                .label()
                .to_lowercase()
                .contains(&self.settings_search.to_lowercase())
            {
                continue;
            }
            let response = nav_row(
                &mut navigation,
                page.icon(),
                page.label(),
                self.page == page,
            );
            if response.clicked() {
                self.visit_settings_page(page);
                self.notice = None;
            }
        }
        let active_session = self.active_session().map(|session| session.id.clone());
        let bottom = Rect::from_min_max(
            pos2(
                sidebar.left() + 12.0,
                sidebar.bottom()
                    - if active_session.is_some() {
                        125.0
                    } else {
                        89.0
                    },
            ),
            sidebar.max - vec2(12.0, 12.0),
        );
        let mut bottom = ui.new_child(UiBuilder::new().max_rect(bottom));
        if button(&mut bottom, "Open assistant", false, vec2(180.0, 32.0)).clicked() {
            self.show_assistant();
        }
        if let Some(id) = active_session {
            bottom.add_space(5.0);
            if button(&mut bottom, "Stop session", false, vec2(180.0, 28.0)).clicked() {
                self.after_saving_notes(Command::Stop(id));
            }
        }
        bottom.add_space(12.0);
        bottom.horizontal(|ui| {
            let (dot, _) = ui.allocate_exact_size(vec2(6.0, 8.0), Sense::hover());
            ui.painter().circle_filled(dot.center(), 2.0, TEAL);
            ui.label(RichText::new("Native · v0.1.0").size(11.0).color(MUTED));
        });
        let mut content = Rect::from_min_max(
            pos2(
                available.left() + 230.0,
                available.top()
                    + if self.page == Page::Shortcuts {
                        6.0
                    } else {
                        23.0
                    },
            ),
            available.max - vec2(30.0, 18.0),
        );
        let now = SystemTime::now();
        let reminder = (self.settings_visible
            && !self.overlay_open
            && self.has_calendar_reminder(now))
        .then(|| {
            let mut banner = content;
            banner.min.y = (banner.max.y - reminder_ui::BANNER_HEIGHT).max(banner.min.y);
            content.max.y = (banner.min.y - 6.0).max(content.min.y);
            banner
        });
        let mut content_ui = ui.new_child(UiBuilder::new().max_rect(content));
        if self.page == Page::Modes {
            self.modes(&mut content_ui);
        } else {
            egui::ScrollArea::vertical().id_salt("settings-content").show(&mut content_ui, |ui| {
                match self.page {
                    Page::General => self.general(ui),
                    Page::Modes => {},
                    Page::Knowledge => self.knowledge(ui),
                    Page::Shortcuts => self.shortcuts(ui),
                    Page::Security => self.security(ui),
                    Page::About => self.about(ui),
                    Page::Calendar => self.calendar_settings(ui),
                    Page::Billing => self.unavailable(ui, Icon::Card, "Billing", "Cueglint runs locally. There is no account, subscription, or billing service in this build."),
                }
            });
        }
        if let Some((message, error)) = self.notice.clone() {
            let rect = Rect::from_min_max(
                pos2(content.left(), content.bottom() - 57.0),
                pos2(content.right(), content.bottom() - 2.0),
            );
            let mut notice_ui = ui.new_child(UiBuilder::new().max_rect(rect));
            Frame::new()
                .fill(if error {
                    Color32::from_rgb(81, 45, 44)
                } else {
                    Color32::from_rgb(42, 63, 60)
                })
                .corner_radius(10)
                .inner_margin(10)
                .show(&mut notice_ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(RichText::new(message).size(12.0))
                                .wrap()
                                .selectable(true),
                        );
                        if ui.small_button("×").clicked() {
                            self.notice = None;
                        }
                    });
                });
        }
        if let Some(rect) = reminder {
            let mut banner = ui.new_child(UiBuilder::new().max_rect(rect));
            self.calendar_reminder_banner(&mut banner, now);
        }
        self.settings_history_buttons(ui, available.min + vec2(230.0, 6.0));
        self.backup_restore_dialog(ui.ctx());
    }

    fn settings_history_buttons(&mut self, ui: &mut Ui, origin: Pos2) {
        // The header stays clickable above scrolled page content.
        ui.painter()
            .rect_filled(Rect::from_min_size(origin, vec2(81.0, 36.0)), 0, BG);
        let enabled = ui.is_enabled() && !self.backup_restore_blocks_controls();
        for (forward, x, glyph, label, has_history) in [
            (
                false,
                0.0,
                Icon::Back,
                "Back in Settings",
                !self.settings_back.is_empty(),
            ),
            (
                true,
                41.0,
                Icon::Forward,
                "Forward in Settings",
                !self.settings_forward.is_empty(),
            ),
        ] {
            let rect = Rect::from_min_size(origin + vec2(x, 0.0), vec2(40.0, 36.0));
            if enabled && has_history {
                let button = icon_button_at(ui, rect, glyph, label, Color32::TRANSPARENT);
                if button.has_focus() {
                    ui.painter().rect_stroke(
                        rect.shrink(1.0),
                        8,
                        Stroke::new(1.0, TEAL),
                        StrokeKind::Inside,
                    );
                }
                if button.clicked() {
                    self.step_settings_history(forward);
                    ui.ctx().request_repaint();
                }
            } else {
                disabled_icon_at(ui, rect, glyph, label);
            }
        }
    }

    fn calendar_ready(&self) -> bool {
        self.fixture.is_none()
            && self.state.is_some()
            && !self.quitting
            && !self.backup_restore_blocks_controls()
            && !self
                .acknowledgement_kinds
                .contains(&Acknowledgement::Calendar)
    }

    fn calendar_preview(&self) -> bool {
        matches!(
            self.fixture.as_deref(),
            Some(crate::calendar_fixture::SETTINGS | crate::calendar_fixture::HISTORY)
        )
    }

    fn calendar_settings(&mut self, ui: &mut Ui) {
        settings_heading(ui, "Calendar");
        ui.add_space(23.0);
        hero(
            ui,
            Icon::Calendar,
            "Calendar",
            "See upcoming events in History.\nCueglint reads your primary Google Calendar.",
        );
        ui.add_space(24.0);
        section(ui, "Connection");
        let preview = self.calendar_preview();
        if preview {
            ui.label(
                RichText::new("Sample calendar · requests disabled")
                    .size(11.0)
                    .color(TEAL),
            );
        }
        let calendar = self.runtime.calendar.clone();
        let ready = self.calendar_ready();
        let remote_failed = calendar
            .disconnect
            .as_ref()
            .is_some_and(|report| matches!(&report.remote, CalendarRevocation::Failed(_)));
        let local_failed = calendar
            .disconnect
            .as_ref()
            .is_some_and(|report| matches!(&report.local, Some(Err(_))));
        let (status, main_action) = if self.fixture.is_some() && !preview {
            ("Calendar unavailable in isolated previews", None)
        } else {
            match (&calendar.connection, &calendar.operation) {
                (_, Some(CalendarOperation::Inspecting)) => {
                    ("Checking the saved connection…", None)
                }
                (_, Some(CalendarOperation::Connecting)) => (
                    "Finish authorizing access in your browser.",
                    Some(("Cancel", CalendarCommand::Cancel)),
                ),
                (_, Some(CalendarOperation::Refreshing)) => (
                    "Refreshing upcoming events…",
                    Some(("Cancel", CalendarCommand::Cancel)),
                ),
                (_, Some(CalendarOperation::Disconnecting)) => ("Disconnecting…", None),
                (_, Some(CalendarOperation::Cancelling)) => ("Cancelling…", None),
                (CalendarConnection::Uninspected, None) => (
                    "Saved connection status is unknown.",
                    Some(("Check connection", CalendarCommand::Inspect)),
                ),
                (CalendarConnection::Unavailable, None) => ("Calendar unavailable", None),
                (CalendarConnection::Disconnected, None) if !calendar.configured => {
                    ("Not connected", None)
                }
                (CalendarConnection::Connected, None) => (
                    "Connected to your primary calendar",
                    Some(("Refresh", CalendarCommand::Refresh)),
                ),
                (
                    CalendarConnection::Disconnected | CalendarConnection::ReconnectRequired,
                    None,
                ) => (
                    "Authorization required",
                    Some(("Reconnect", CalendarCommand::Reconnect)),
                ),
            }
        };
        let mut action = None;
        card(ui, |ui| {
            row(ui, "Google Calendar", status, |ui| {
                if let Some((label, command)) = main_action
                    && calendar_button(ui, label, ready, true)
                {
                    action = Some(command);
                }
            });
            if self.fixture.is_some() && !preview {
                return;
            }
            if matches!(calendar.connection, CalendarConnection::Disconnected)
                && calendar.operation.is_none()
            {
                rule(ui);
                ui.add_space(12.0);
                let label = ui.label("Google Desktop OAuth client ID");
                ui.add_enabled(
                    ready,
                    egui::TextEdit::singleline(&mut self.calendar_client_id)
                        .id(Id::new("calendar-client-id"))
                        .hint_text("Enter your desktop client ID")
                        .char_limit(257)
                        .desired_width(f32::INFINITY),
                )
                .labelled_by(label.id);
                let client_id = self.calendar_client_id.trim();
                let valid = !client_id.is_empty()
                    && client_id.len() <= 256
                    && client_id
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte));
                if !client_id.is_empty() && !valid {
                    ui.label(
                        RichText::new("Enter a valid desktop client ID of at most 256 characters.")
                            .size(11.0)
                            .color(MUTED),
                    );
                }
                ui.label(
                    RichText::new("Connect opens Google in your browser for read-only access.")
                        .size(11.0)
                        .color(MUTED),
                );
                if calendar_button(ui, "Connect", ready && valid, true) {
                    action = Some(CalendarCommand::Connect {
                        client_id: client_id.to_owned(),
                    });
                }
                ui.add_space(12.0);
            }
            if let Some(disconnect) = &calendar.disconnect {
                rule(ui);
                ui.add_space(12.0);
                ui.label(
                    RichText::new(match &disconnect.remote {
                        CalendarRevocation::Revoked => "Google access revoked.".to_owned(),
                        CalendarRevocation::NoGrant => {
                            "No saved Google grant to revoke.".to_owned()
                        }
                        CalendarRevocation::Skipped => "Google revocation skipped.".to_owned(),
                        CalendarRevocation::Failed(error) => {
                            format!("Google revocation failed. {error}")
                        }
                    })
                    .size(12.0),
                );
                if let Some(local) = &disconnect.local {
                    ui.label(
                        RichText::new(match local {
                            Ok(()) => "Saved authorization removed from this device.".to_owned(),
                            Err(error) => format!("Local removal failed. {error}"),
                        })
                        .size(12.0),
                    );
                }
                ui.add_space(12.0);
            }
            if calendar.configured
                && calendar.operation.is_none()
                && (matches!(
                    calendar.connection,
                    CalendarConnection::Connected | CalendarConnection::ReconnectRequired
                ) || remote_failed
                    || local_failed)
            {
                rule(ui);
                ui.add_space(12.0);
                if !remote_failed && !local_failed {
                    ui.label(RichText::new("Disconnect requests Google revocation and removes the saved authorization.").size(11.0).color(MUTED));
                } else if remote_failed {
                    ui.label(
                        RichText::new(
                            "Access may remain in Google. You can revoke it in your Google account.",
                        )
                        .size(11.0)
                        .color(MUTED),
                    );
                }
                ui.horizontal_wrapped(|ui| {
                    if !local_failed
                        && calendar_button(
                            ui,
                            if remote_failed {
                                "Retry disconnect"
                            } else {
                                "Disconnect"
                            },
                            ready,
                            false,
                        )
                    {
                        action = Some(CalendarCommand::Disconnect { local_only: false });
                    }
                    if (remote_failed || local_failed)
                        && calendar_button(ui, "Remove from this device", ready, false)
                    {
                        action = Some(CalendarCommand::Disconnect { local_only: true });
                    }
                });
                ui.add_space(12.0);
            }
            if let Some(error) = &calendar.error
                && !remote_failed
                && !local_failed
            {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(error.to_string())
                        .size(12.0)
                        .color(Color32::from_rgb(255, 197, 187)),
                );
                ui.add_space(12.0);
            }
        });
        if let Some(command) = action {
            self.send(Command::Calendar(command));
        }
        self.calendar_reminder_settings(ui);
    }

    fn general(&mut self, ui: &mut Ui) {
        // This measured band must not move the hero when font metrics change.
        let (heading_rect, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), 31.0), Sense::hover());
        let mut heading = ui.new_child(UiBuilder::new().max_rect(heading_rect));
        settings_heading(&mut heading, "General");
        hero(
            ui,
            Icon::Gear,
            "General",
            "Make Cueglint feel at home.\nYour sessions and knowledge are stored on this device.",
        );
        ui.add_space(13.0);
        section(ui, "Software update");
        card(ui, |ui| {
            self.release_check
                .show(ui, self.fixture.is_none(), cueglint_local::releases::check);
        });
        ui.add_space(18.0);
        section(ui, "Preferences");
        card(ui, |ui| {
            row(
                ui,
                "Assistant",
                "A compact window, always within reach",
                |ui| {
                    if button(ui, "Open assistant", false, vec2(118.0, 28.0)).clicked() {
                        self.show_assistant();
                    }
                },
            );
            rule(ui);
            row(ui, "Appearance", "Warm dark theme", |ui| {
                ui.label(RichText::new("Dark").size(12.0).color(MUTED));
            });
            rule(ui);
            let status = self
                .runtime
                .capture
                .as_ref()
                .map(|capture| capture.state.as_str())
                .unwrap_or("Not listening")
                .to_owned();
            row(
                ui,
                "Audio capture",
                "Local microphone and system audio transcription",
                |ui| {
                    ui.label(RichText::new(status).size(12.0).color(MUTED));
                },
            );
        });
        ui.add_space(18.0);
        section(ui, "Audio & screen context");
        let window_picker_supported = cueglint_local::screen::scoped::is_supported();
        if self.reveal_permissions {
            ui.scroll_to_cursor(Some(Align::Min));
            self.reveal_permissions = false;
        }
        if let Some(state) = &self.state {
            let mut settings = state.settings.clone();
            let changed = card(ui, |ui| {
                ui.add_space(12.0);
                let mut changed = ui
                    .checkbox(&mut settings.microphone_enabled, "Microphone capture")
                    .changed();
                changed |= ui
                    .checkbox(&mut settings.system_audio_enabled, "System audio capture")
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut settings.screen_enabled,
                        "Include local screen text when asking",
                    )
                    .changed();
                if cfg!(target_os = "macos")
                    && settings.screen_enabled
                    && (window_picker_supported || settings.screen_input == ScreenInput::Window)
                {
                    ui.horizontal(|ui| {
                        ui.label("Screen source");
                        changed |= ui
                            .radio_value(
                                &mut settings.screen_input,
                                ScreenInput::Display,
                                "Display",
                            )
                            .changed();
                        changed |= ui
                            .add_enabled_ui(window_picker_supported, |ui| {
                                ui.radio_value(
                                    &mut settings.screen_input,
                                    ScreenInput::Window,
                                    "Single window",
                                )
                                .changed()
                            })
                            .inner;
                    });
                }
                ui.label(RichText::new("Audio starts when you start or resume listening. Stop listening before changing inputs. Enabled screen text is sampled locally for each question or preview on macOS.").size(11.0).color(MUTED));
                ui.add_space(12.0);
                changed
            });
            if changed {
                self.send(Command::Settings(settings));
            }
        }
        ui.add_space(8.0);
        self.capture_input_settings(ui);
        let (microphone, system_audio, display_screen, window_screen) = self
            .state
            .as_ref()
            .map(|state| {
                let settings = &state.settings;
                (
                    settings.microphone_enabled,
                    settings.system_audio_enabled,
                    settings.screen_enabled && settings.screen_input.uses_display(),
                    settings.screen_enabled && settings.screen_input == ScreenInput::Window,
                )
            })
            .unwrap_or_default();
        if microphone {
            ui.add_space(10.0);
            self.permission_card(ui, "Microphone", self.runtime.permissions.microphone, false);
        }
        if system_audio || display_screen {
            ui.add_space(10.0);
            self.permission_card(
                ui,
                "Screen & system audio",
                self.runtime.permissions.screen,
                display_screen,
            );
        }
        if window_screen && cfg!(target_os = "macos") {
            ui.add_space(10.0);
            let picker = self.runtime.window_picker.as_ref();
            let phase = picker.map(|picker| picker.phase);
            let question = picker.is_some_and(|picker| picker.question);
            card(ui, |ui| {
                row(
                    ui,
                    "Single window",
                    match phase {
                        Some(PickerPhase::Choosing) => "Choose a window in the macOS picker.",
                        Some(PickerPhase::Capturing) => "Reading the selected window locally.",
                        Some(PickerPhase::Retiring) => "Closing the window picker.",
                        None if !window_picker_supported => {
                            "Window selection requires macOS 15.2. Choose Display for screen context."
                        }
                        None if self.runtime.window_cleanup_unconfirmed => {
                            "Window stream stop could not be confirmed. Restart Cueglint before capturing again."
                        }
                        None => {
                            "Choose a window for each preview or question on macOS 15.2 or later."
                        }
                    },
                    |ui| {
                        if phase == Some(PickerPhase::Retiring)
                            || question
                            || !window_picker_supported
                            || self.runtime.window_cleanup_unconfirmed
                        {
                            return;
                        }
                        let (label, command) = if phase.is_some() {
                            ("Cancel capture", Command::CancelWindowPicker)
                        } else {
                            ("Capture screen now", Command::CaptureScreen)
                        };
                        if button(ui, label, false, vec2(145.0, 28.0)).clicked() {
                            self.notice = None;
                            self.send(command);
                        }
                    },
                );
            });
        }
        if let Some(screen) = &self.runtime.screen {
            let source = match screen.source {
                ScreenSource::Display(id) => format!("display {id}"),
                ScreenSource::Window(id) => format!("window {id}"),
            };
            ui.label(
                RichText::new(format!(
                    "Screen context: {source} · {} × {} · {} text characters · {} UTC",
                    screen.width,
                    screen.height,
                    screen.characters,
                    clock_label(screen.captured_at)
                ))
                .size(11.0)
                .color(MUTED),
            )
            .on_hover_text(format!("Context {}", screen.id));
        }
        if let Some(capture) = &self.runtime.capture {
            if capture.dropped_packets > 0 || capture.device_errors > 0 {
                ui.label(
                    RichText::new(format!(
                        "Capture health: {} dropped packets · {} device errors",
                        capture.dropped_packets, capture.device_errors
                    ))
                    .size(11.0)
                    .color(Color32::from_rgb(245, 190, 134)),
                );
            }
        }
        ui.add_space(18.0);
        // The font loader can change the height of the preceding capture controls.
        if self.reveal_provider && self.fonts.ready() {
            ui.scroll_to_cursor(Some(Align::Min));
            self.reveal_provider = false;
        }
        self.provider_settings(ui);
        ui.add_space(18.0);
        section(ui, "Local models");
        ui.label(RichText::new("Download once, run on this device. Downloads connect to Hugging Face. Local answers and all audio transcription stay on this device.").size(11.0).color(MUTED));
        ui.add_space(8.0);
        let backend = self.runtime.transcription_backend;
        card(ui, |ui| {
            ui.add_space(12.0);
            ui.label(RichText::new("Transcription processor").size(12.0));
            ui.horizontal(|ui| {
                for (choice, label) in [
                    (TranscriptionBackend::Cpu, "CPU transcription"),
                    (TranscriptionBackend::Metal, "Metal transcription"),
                ] {
                    if ui
                        .add_enabled(
                            self.state.is_some()
                                && self.pending == 0
                                && !self.capture_busy()
                                && asr::backend_compiled(choice),
                            egui::RadioButton::new(backend == choice, label),
                        )
                        .on_disabled_hover_text(
                            "Stop audio capture and wait for settings to save. Metal also requires a build with Metal support.",
                        )
                        .clicked()
                        && let Some(state) = &self.state
                    {
                        let mut settings = state.settings.clone();
                        settings.transcription_backend = choice;
                        self.send(Command::Settings(settings));
                    }
                }
            });
            if backend == TranscriptionBackend::Metal {
                ui.label(
                    RichText::new(if asr::backend_compiled(backend) {
                        "Metal device availability is checked when listening starts. Startup may take several seconds; there is no automatic fallback."
                    } else {
                        "Metal transcription is unavailable in this build. Choose CPU to listen. Your saved choice is retained."
                    })
                    .size(11.0)
                    .color(MUTED),
                );
            }
            ui.add_space(12.0);
            rule(ui);
            self.transcription_language_settings(ui);
        });
        ui.add_space(8.0);
        let source_label = ui.label(RichText::new("Import from a local folder").size(12.0));
        ui.add(
            egui::TextEdit::singleline(&mut self.model_import_path)
                .hint_text("Full path to the folder containing this model's required files")
                .desired_width(f32::INFINITY),
        )
        .labelled_by(source_label.id);
        ui.label(RichText::new("Imports stay local and verify the files below. Remove deletes only the local model copy; source folders, meetings and notes remain. Your model selection stays unchanged.").size(11.0).color(MUTED));
        ui.add_space(8.0);
        let model_job_active = self.model_job_busy();
        let file_actions_allowed = self.fixture.is_none()
            && self.state.is_some()
            && self.pending == 0
            && !self.capture_busy()
            && !self.generating()
            && !model_job_active;
        let mut profiles = self.model_profiles.clone();
        profiles.sort_by_key(|profile| {
            (
                profile.purpose != "generation",
                profile.id != models::GENERATION_PROFILE
                    && profile.id != models::TRANSCRIPTION_PROFILE,
            )
        });
        for profile in profiles {
            let installed = self.runtime.installed_profiles.contains(&profile.id);
            let incomplete = self.runtime.incomplete_profiles.contains(&profile.id);
            let active_job = self
                .runtime
                .model_job
                .as_ref()
                .filter(|job| job.profile_id == profile.id && model_job_active)
                .cloned();
            let latest_job = self
                .runtime
                .model_job
                .as_ref()
                .filter(|job| job.profile_id == profile.id)
                .cloned();
            let bytes: u64 = profile.files.iter().map(|file| file.bytes).sum();
            card(ui, |ui| {
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&profile.name).size(13.0));
                        ui.label(
                            RichText::new(if profile.purpose == "generation" {
                                "Answers"
                            } else {
                                "Transcription"
                            })
                            .size(11.0)
                            .color(MUTED),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(job) = &active_job {
                            ui.label(
                                RichText::new(match job.state.as_str() {
                                    "importing" => "Importing…",
                                    "removing" => "Removing…",
                                    _ => "Downloading…",
                                })
                                .size(11.0)
                                .color(MUTED),
                            );
                        } else if installed {
                            ui.label(RichText::new("Verified locally").size(11.0).color(TEAL));
                        } else if incomplete {
                            ui.label(
                                RichText::new("Incomplete local files")
                                    .size(11.0)
                                    .color(MUTED),
                            );
                        }
                    });
                });
                ui.add_space(8.0);
                if let Some(job) = &active_job {
                    if job.state != "removing" {
                        let cancel = ui.add_enabled(
                            self.fixture.is_none() && self.pending == 0,
                            egui::Button::new("Cancel").min_size(vec2(70.0, 28.0)),
                        );
                        cancel.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::Button,
                                cancel.enabled(),
                                format!("Cancel model operation for {}", profile.name),
                            )
                        });
                        if cancel.clicked() {
                            self.notice = None;
                            self.send(Command::CancelInstall);
                        }
                    } else {
                        ui.spinner();
                    }
                } else {
                    ui.horizontal_wrapped(|ui| {
                        if !installed {
                            let (label, action) = if incomplete {
                                ("Retry download".into(), "Retry download for")
                            } else {
                                (format!("Download {} MiB", bytes.div_ceil(1024 * 1024)), "Download")
                            };
                            let download = ui.add_enabled(file_actions_allowed,
                                egui::Button::new(label).fill(BLUE).min_size(vec2(128.0, 28.0)))
                                .on_disabled_hover_text("Model file changes require idle capture and answers, and are disabled in fixtures.");
                            download.widget_info(|| egui::WidgetInfo::labeled(
                                egui::WidgetType::Button, download.enabled(), format!("{action} {}", profile.name),
                            ));
                            if download.clicked() {
                                self.notice = None;
                                self.send(Command::InstallModel(profile.id.clone()));
                            }
                        }
                        let import = ui.add_enabled(file_actions_allowed && !self.model_import_path.trim().is_empty(),
                            egui::Button::new("Import").min_size(vec2(74.0, 28.0)))
                            .on_disabled_hover_text("Enter a full source-folder path and wait for capture, answers and model operations to finish.");
                        import.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, import.enabled(), format!("Import {}", profile.name)));
                        if import.clicked() {
                            self.notice = None;
                            self.send(Command::ImportModel(profile.id.clone(), PathBuf::from(self.model_import_path.trim())));
                        }
                        if installed || incomplete {
                            let (label, preposition) = if incomplete {
                                ("Remove local files", "for")
                            } else {
                                ("Remove local copy", "of")
                            };
                            let remove = ui.add_enabled(file_actions_allowed,
                                egui::Button::new(label).min_size(vec2(130.0, 28.0)))
                                .on_hover_text("Deletes all local files for this model, including completed artifacts and partial downloads.");
                            remove.widget_info(|| egui::WidgetInfo::labeled(
                                egui::WidgetType::Button, remove.enabled(), format!("{label} {preposition} {}", profile.name),
                            ));
                            if remove.clicked() {
                                self.notice = None;
                                self.send(Command::RemoveModel(profile.id.clone()));
                            }
                        }
                    });
                }
                ui.add_space(8.0);
                let generation = profile.purpose == "generation";
                let selected = if generation {
                    self.generation_profile()
                } else {
                    self.transcription_profile()
                } == profile.id
                    && (!generation || self.runtime.answer_provider.provider.is_local());
                let label = format!(
                    "Use {} for {}",
                    profile.name,
                    if generation {
                        "answers"
                    } else {
                        "transcription"
                    }
                );
                if ui
                    .add_enabled(
                        self.state.is_some() && self.pending == 0 && !self.capture_busy(),
                        egui::RadioButton::new(selected, label),
                    )
                    .on_disabled_hover_text(
                        "Wait for settings to save and stop audio capture before changing models.",
                    )
                    .clicked()
                {
                    if let Some(state) = &self.state {
                        let mut settings = state.settings.clone();
                        if generation {
                            settings.generation_profile = Some(profile.id.clone());
                            settings.answer_provider = ProviderConfig::default();
                        } else {
                            settings.transcription_profile = Some(profile.id.clone());
                        }
                        self.send(Command::Settings(settings));
                    }
                }
                let note = if profile.id == models::GENERATION_PROFILE {
                    "Recommended default"
                } else if profile.id == models::TRANSCRIPTION_PROFILE {
                    "Experimental English transcription; review the transcript."
                } else if profile.id == models::SMALL_GENERATION_PROFILE {
                    "Smaller download; lower accuracy in our context and follow-up checks."
                } else if profile.id == models::SMALL_TRANSCRIPTION_PROFILE {
                    "Smaller English transcription model; quality validation is incomplete."
                } else if profile.id == models::MULTILINGUAL_TRANSCRIPTION_PROFILE {
                    "Explicit language tokens; multilingual quality has not been tested."
                } else {
                    "Local model"
                };
                ui.label(RichText::new(note).size(11.0).color(MUTED));
                if selected && !installed && active_job.is_none() {
                    ui.label(
                        RichText::new(if incomplete {
                            "Retry the download or import the selected model to finish setup."
                        } else {
                            "Selected model is not installed. Download or import it to use it."
                        })
                        .size(11.0)
                        .color(MUTED),
                    );
                }
                if let Some(job) = &active_job {
                    if job.state != "removing" {
                        ui.add_space(7.0);
                        let fraction = if job.total_bytes == 0 {
                            0.0
                        } else {
                            (job.downloaded_bytes as f64 / job.total_bytes as f64).clamp(0.0, 1.0)
                                as f32
                        };
                        ui.add(egui::ProgressBar::new(fraction).show_percentage());
                        if !job.file.is_empty() {
                            ui.label(RichText::new(&job.file).size(10.0).color(MUTED));
                        }
                    }
                } else if let Some(job) = &latest_job {
                    let message = match job.state.as_str() {
                        "removed" => Some(
                            "Local model files removed. Meetings and notes remain; download or import this model before using it again.",
                        ),
                        "cancelled" => Some("Model operation cancelled. You can retry when ready."),
                        "error" => Some(
                            "Model operation failed. Check the error message. For import, use every required file for this exact model version, then retry.",
                        ),
                        _ => None,
                    };
                    if let Some(message) = message {
                        ui.label(RichText::new(message).size(11.0).color(MUTED));
                    }
                }
                egui::CollapsingHeader::new("Required import files").id_salt((&profile.id, "import-files")).show(ui, |ui| {
                    for file in &profile.files { ui.label(RichText::new(&file.name).size(11.0).color(MUTED)); }
                    ui.label(RichText::new("Files must match this exact model version; sizes and SHA-256 hashes are checked before installation.").size(10.0).color(MUTED));
                });
                ui.add_space(5.0);
                ui.hyperlink_to(
                    RichText::new(format!("{} license", profile.license))
                        .size(10.0)
                        .color(MUTED),
                    &profile.license_url,
                );
                ui.add_space(12.0);
            });
            ui.add_space(8.0);
        }
        ui.add_space(10.0);
        section(ui, "Model storage");
        card(ui, |ui| {
            ui.add_space(12.0);
            ui.label(
                RichText::new(self.runtime.model_root.display().to_string())
                    .size(11.0)
                    .color(MUTED),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.model_path)
                        .hint_text("Optional model directory")
                        .desired_width((ui.available_width() - 74.0).max(100.0)),
                );
                if ui.add_enabled(file_actions_allowed, egui::Button::new("Save").min_size(vec2(58.0, 28.0))).on_disabled_hover_text("Model storage can change only while capture, answers and model operations are idle.").clicked() {
                    if let Some(state) = &self.state {
                        let mut settings = state.settings.clone();
                        settings.model_directory = if self.model_path.trim().is_empty() {
                            None
                        } else {
                            Some(self.model_path.trim().into())
                        };
                        self.send(Command::Settings(settings));
                    }
                }
            });
            ui.add_space(12.0);
        });
        ui.add_space(20.0);
    }

    fn provider_busy(&self) -> bool {
        self.runtime.provider.as_ref().is_some_and(|status| {
            matches!(
                status.state.as_str(),
                "checking key"
                    | "saving key"
                    | "deleting key"
                    | "listing models"
                    | "testing connection"
            )
        })
    }

    fn provider_settings(&mut self, ui: &mut Ui) {
        let provider_top = ui.cursor().top();
        section(ui, "Answer provider");
        if let Some(caption) = &self.provider_fixture_caption {
            ui.label(RichText::new(caption).size(11.0).color(TEAL));
        }
        let mut config = self
            .provider_draft
            .take()
            .unwrap_or_else(|| self.runtime.answer_provider.clone());
        let previous_scope = config.credential_ref().ok();
        let busy = self.provider_busy();
        let controls = self.state.is_some() && self.pending == 0 && !self.capture_busy();
        let mut action = None;
        let mut use_provider = false;
        let mut save_key = false;
        let mut key_visible = false;
        let mut charges_visible = false;
        card(ui, |ui| {
            ui.add_space(12.0);
            ui.label(RichText::new(format!("Active: {}", self.answer_destination())).size(12.0));
            ui.label(RichText::new("Saving a key does not select a provider or send context. Use provider applies your choice to future questions.").size(11.0).color(MUTED));
            ui.add_space(8.0);
            let label = ui.label("Provider");
            egui::ComboBox::from_id_salt("answer-provider")
                .selected_text(config.provider.label())
                .show_ui(ui, |ui| {
                    for provider in ProviderId::ALL {
                        let response =
                            ui.selectable_value(&mut config.provider, provider, provider.label());
                        if response.clicked() {
                            ui.close();
                        }
                        if response.changed() {
                            config.model_id.clear();
                            config.base_url = None;
                        }
                    }
                })
                .response
                .labelled_by(label.id);
            if config.provider == ProviderId::OpenAiCompatible {
                let label = ui.label("API base URL");
                ui.add(
                    egui::TextEdit::singleline(config.base_url.get_or_insert_with(String::new))
                        .id_salt("provider-base-url")
                        .hint_text("http://localhost:11434/v1 or https://your-server.example/v1")
                        .desired_width(f32::INFINITY),
                )
                .labelled_by(label.id);
                ui.label(RichText::new("HTTPS or loopback HTTP. This endpoint receives your intentional questions and selected context.").size(11.0).color(MUTED));
            }
            let status = self
                .runtime
                .provider
                .as_ref()
                .filter(|status| {
                    status.config.credential_ref().ok() == config.credential_ref().ok()
                })
                .cloned();
            if !config.provider.is_local() {
                let label = ui.label("Model ID");
                ui.add(
                    egui::TextEdit::singleline(&mut config.model_id)
                        .id_salt("provider-model-id")
                        .hint_text("Enter the provider's model ID")
                        .desired_width(f32::INFINITY),
                )
                .labelled_by(label.id);
                if let Some(status) = &status
                    && !status.models.is_empty()
                {
                    egui::ComboBox::from_id_salt("provider-model-catalog")
                        .selected_text("Choose a listed model")
                        .show_ui(ui, |ui| {
                            for model in &status.models {
                                if ui
                                    .selectable_value(&mut config.model_id, model.clone(), model)
                                    .clicked()
                                {
                                    ui.close();
                                }
                            }
                        });
                }
                ui.label(RichText::new("Manual model IDs remain available. Account access and supported models depend on the provider.").size(11.0).color(MUTED));
            }
            ui.add_space(8.0);
            if ui
                .add_enabled(
                    controls && config.validate().is_ok(),
                    egui::Button::new("Use provider").fill(BLUE),
                )
                .clicked()
            {
                use_provider = true;
            }
            if config.provider.is_local() {
                ui.label(RichText::new("Local answers use the selected verified model below. No API key or hosted inference is needed.").size(11.0).color(MUTED));
            } else {
                ui.add_space(10.0);
                let key_label = ui.label("API key");
                let mut password = egui::TextEdit::singleline(&mut self.provider_key)
                    .id(Id::new("provider-api-key"))
                    .password(true)
                    .interactive(self.provider_key_submission.is_none())
                    .hint_text("Stored only in the OS credential store")
                    .desired_width(f32::INFINITY)
                    .show(ui);
                key_visible = ui.clip_rect().contains_rect(password.response.rect);
                password.response.response.labelled_by(key_label.id);
                // Password edit history must not retain earlier copies of the secret.
                password.state.clear_undoer();
                password.state.store(ui.ctx(), Id::new("provider-api-key"));
                let valid = config.validate().is_ok();
                let operations = self.fixture.is_none()
                    && self.pending == 0
                    && self.provider_key_submission.is_none()
                    && !busy
                    && valid;
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(
                            operations && !self.provider_key.is_empty(),
                            egui::Button::new("Save key"),
                        )
                        .clicked()
                    {
                        save_key = true;
                    }
                    if ui
                        .add_enabled(operations, egui::Button::new("Delete key"))
                        .clicked()
                    {
                        action = Some(ProviderAction::Delete);
                    }
                    if ui
                        .add_enabled(operations, egui::Button::new("Check key status"))
                        .clicked()
                    {
                        action = Some(ProviderAction::Status);
                    }
                });
                let key_status = match status
                    .as_ref()
                    .and_then(|status| status.credential.as_ref())
                {
                    Some(CredentialStatus::Stored) => "Key stored in the OS credential store.",
                    Some(CredentialStatus::Missing) => {
                        "No key stored for this provider and endpoint."
                    }
                    Some(CredentialStatus::Unavailable { reason }) => reason,
                    None => "Key status not checked for this provider and endpoint.",
                };
                ui.label(RichText::new(key_status).size(11.0).color(MUTED));
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(operations, egui::Button::new("Refresh models")).on_hover_text("Explicitly contacts this provider's model endpoint. No meeting context is sent.").clicked() { action = Some(ProviderAction::Models); }
                    if ui.add_enabled(operations && config.validate_for_generation().is_ok(), egui::Button::new("Test connection")).on_hover_text("Sends only a tiny synthetic instruction, with no meeting context.").clicked() { action = Some(ProviderAction::Test); }
                    if busy && self.runtime.provider.as_ref().is_some_and(|status| matches!(status.state.as_str(), "listing models" | "testing connection"))
                        && ui.button("Cancel provider operation").clicked() { self.send(Command::CancelProvider); }
                });
                if let Some(status) = &status {
                    if status.config == config {
                        if busy {
                            ui.label(
                                RichText::new(format!("{}…", status.state))
                                    .size(11.0)
                                    .color(MUTED),
                            );
                        }
                        if let Some(message) = &status.message {
                            ui.label(RichText::new(message).size(11.0).color(MUTED));
                        }
                    }
                    if let Some(url) = &status.documentation_url {
                        ui.hyperlink_to("Provider model documentation", url);
                    }
                }
                ui.label(RichText::new("Remote questions send your prompt and selected transcript, chat, knowledge and enabled screen text to this destination. Audio transcription stays local. There is no automatic fallback.").size(11.0).color(MUTED));
                let charges = ui.label(
                    RichText::new("Provider charges may apply to questions and connection tests.")
                        .size(11.0)
                        .color(MUTED),
                );
                charges_visible = ui.clip_rect().contains_rect(charges.rect);
            }
            ui.add_space(12.0);
        });
        if previous_scope != config.credential_ref().ok() {
            self.provider_key.zeroize();
            self.provider_key_submission = None;
            ui.ctx().data_mut(|data| {
                data.remove::<egui::text_edit::TextEditState>(Id::new("provider-api-key"))
            });
            save_key = false;
            action = None;
        }
        if use_provider && let Some(state) = &self.state {
            let mut settings = state.settings.clone();
            settings.answer_provider = config.clone();
            self.send(Command::Settings(settings));
        }
        if save_key {
            match Secret::new(self.provider_key.clone()) {
                Ok(secret) => {
                    action = Some(ProviderAction::Save(secret));
                }
                Err(error) => self.notice = Some((error.to_string(), true)),
            }
        }
        if let Some(action) = action {
            let id = uuid::Uuid::new_v4().to_string();
            let key_save = matches!(&action, ProviderAction::Save(_));
            if self.send(Command::Provider(id.clone(), config.clone(), action)) && key_save {
                self.provider_key_submission = config
                    .credential_ref()
                    .ok()
                    .map(|reference| (reference, id));
            }
        }
        self.provider_draft = Some(config);
        if self.provider_fixture_caption.is_some() {
            self.provider_fixture_ready = key_visible
                && charges_visible
                && ui
                    .clip_rect()
                    .contains(pos2(ui.max_rect().left(), provider_top));
        }
    }

    fn reconcile_provider_key(&mut self, ctx: &egui::Context) {
        let Some((reference, id)) = &self.provider_key_submission else {
            return;
        };
        let scope_changed = self
            .provider_draft
            .as_ref()
            .and_then(|config| config.credential_ref().ok())
            .as_ref()
            != Some(reference);
        let terminal = self
            .runtime
            .provider
            .as_ref()
            .filter(|status| {
                &status.id == id && status.config.credential_ref().ok().as_ref() == Some(reference)
            })
            .map(|status| status.state.as_str());
        if scope_changed || terminal == Some("ready") {
            self.provider_key.zeroize();
            self.provider_key_submission = None;
            ctx.data_mut(|data| {
                data.remove::<egui::text_edit::TextEditState>(Id::new("provider-api-key"))
            });
        } else if terminal.is_some_and(|state| matches!(state, "error" | "cancelled")) {
            self.provider_key_submission = None;
        }
    }

    fn answer_destination(&self) -> String {
        let config = &self.runtime.answer_provider;
        if config.provider.is_local() {
            format!("Local · {} · on this device", self.generation_profile())
        } else {
            format!(
                "{} · {} · {}",
                config.provider.label(),
                if config.model_id.is_empty() {
                    "model not selected"
                } else {
                    &config.model_id
                },
                config
                    .endpoint()
                    .unwrap_or_else(|_| "endpoint not configured".into())
            )
        }
    }

    fn generation_profile(&self) -> &str {
        self.state
            .as_ref()
            .and_then(|state| state.settings.generation_profile.as_deref())
            .unwrap_or(models::GENERATION_PROFILE)
    }

    fn transcription_language_settings(&mut self, ui: &mut Ui) {
        let Some(state) = &self.state else {
            return;
        };
        let mut settings = state.settings.clone();
        let before = settings.transcription_language.clone();
        let editable = self.pending == 0 && !self.capture_busy();
        row(
            ui,
            "Transcription language",
            "Input speech only; review transcripts",
            |ui| {
                ui.add_enabled_ui(editable, |ui| {
                    let selected = cueglint_core::transcription_language_name(
                        &settings.transcription_language,
                    )
                    .unwrap_or(&settings.transcription_language)
                    .to_owned();
                    // ComboBox hashes the salt before building its persistent button ID.
                    let combo_id =
                        ui.make_persistent_id(egui::IdSalt::new("transcription-language"));
                    let was_open = egui::ComboBox::is_open(ui.ctx(), combo_id);
                    let escape = was_open && ui.input(|input| input.key_pressed(egui::Key::Escape));
                    // A new popup Area is disabled during its first sizing pass.
                    let initial_focus_id = combo_id.with("initial-focus");
                    let mut initial_focus = !was_open
                        || ui
                            .ctx()
                            .data(|data| data.get_temp::<bool>(initial_focus_id))
                            .unwrap_or(false);
                    let restore_id = combo_id.with("restore-focus");
                    let previous_focus_id = combo_id.with("previous-option-focus");
                    let previous_focus = ui
                        .ctx()
                        .data(|data| data.get_temp::<egui::Id>(previous_focus_id));
                    let mut activated = false;
                    let response = egui::ComboBox::from_id_salt("transcription-language")
                        .width(118.0)
                        .selected_text(selected.as_str())
                        .show_ui(ui, |ui| {
                            for (code, name) in cueglint_core::TRANSCRIPTION_LANGUAGES {
                                let was_selected = settings.transcription_language == code;
                                let option = ui.selectable_value(
                                    &mut settings.transcription_language,
                                    code.into(),
                                    format!("{name} ({code})"),
                                );
                                if initial_focus && was_selected && option.enabled() {
                                    initial_focus = false;
                                    option.request_focus();
                                    option.scroll_to_me(Some(Align::Center));
                                } else if option.has_focus() && previous_focus != Some(option.id) {
                                    option.scroll_to_me(None);
                                }
                                if option.clicked() {
                                    activated = true;
                                    ui.close();
                                }
                            }
                        })
                        .response;
                    let restore_pending = ui
                        .ctx()
                        .data(|data| data.get_temp::<bool>(restore_id))
                        .unwrap_or(false);
                    if response.enabled() && (activated || escape) {
                        response.request_focus();
                        // Saving disables the combo until its existing worker acknowledgement.
                        ui.ctx().data_mut(|data| data.insert_temp(restore_id, true));
                    } else if restore_pending {
                        let another_widget = ui
                            .memory(|memory| memory.focused())
                            .is_some_and(|id| id != response.id);
                        let moved_away = ui.input(|input| {
                            (input.pointer.any_pressed() && !response.contains_pointer())
                                || input.key_pressed(egui::Key::Tab)
                        });
                        if another_widget || moved_away || (!editable && self.pending == 0) {
                            ui.ctx().data_mut(|data| data.remove::<bool>(restore_id));
                        } else if editable {
                            response.request_focus();
                            ui.ctx().data_mut(|data| data.remove::<bool>(restore_id));
                        }
                    }
                    if egui::ComboBox::is_open(ui.ctx(), response.id) {
                        ui.ctx()
                            .data_mut(|data| data.insert_temp(initial_focus_id, initial_focus));
                        if let Some(focused) = ui.memory(|memory| memory.focused()) {
                            ui.ctx()
                                .data_mut(|data| data.insert_temp(previous_focus_id, focused));
                        }
                    } else {
                        ui.ctx()
                            .data_mut(|data| data.remove::<bool>(initial_focus_id));
                        ui.ctx()
                            .data_mut(|data| data.remove::<egui::Id>(previous_focus_id));
                    }
                    response.widget_info(|| {
                        let mut info = egui::WidgetInfo::labeled(
                            egui::WidgetType::ComboBox,
                            response.enabled(),
                            "Transcription language",
                        );
                        info.current_text_value = Some(selected.clone());
                        info
                    });
                });
            },
        );
        let language_error = asr::validate_language(
            settings
                .transcription_profile
                .as_deref()
                .unwrap_or(models::TRANSCRIPTION_PROFILE),
            &settings.transcription_language,
        )
        .err();
        if settings.transcription_language != before {
            self.send(Command::Settings(settings));
        }
        if let Some(error) = language_error {
            ui.label(RichText::new(error.to_string()).size(11.0).color(MUTED));
        }
    }

    fn transcription_profile(&self) -> &str {
        self.state
            .as_ref()
            .and_then(|state| state.settings.transcription_profile.as_deref())
            .unwrap_or(models::TRANSCRIPTION_PROFILE)
    }

    fn permission_card(
        &mut self,
        ui: &mut Ui,
        title: &str,
        status: PermissionStatus,
        screen_retry: bool,
    ) {
        let (label, message) = match status {
            PermissionStatus::Granted => ("Access granted", "Capture starts only when requested."),
            PermissionStatus::Unknown => (
                "Not checked",
                "Permission status is unavailable. Starting capture will check access.",
            ),
            PermissionStatus::NotDetermined => (
                "Permission not requested",
                "When you start listening, macOS asks for microphone access. No audio is captured while waiting.",
            ),
            PermissionStatus::Required | PermissionStatus::Denied => {
                if title == "Microphone" {
                    (
                        "Access required",
                        "Allow Cueglint in System Settings → Privacy & Security → Microphone, then resume listening.",
                    )
                } else {
                    (
                        "Access required",
                        "Allow Cueglint in System Settings → Privacy & Security → Screen & System Audio Recording. Restart Cueglint if macOS asks, then retry.",
                    )
                }
            }
            PermissionStatus::Restricted => (
                "Access restricted",
                "This device restricts capture. Review its privacy settings or contact the device administrator.",
            ),
            PermissionStatus::Unsupported => (
                "Unavailable",
                "This capture source is not available on this platform.",
            ),
        };
        card(ui, |ui| {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).size(12.0).strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(label).size(11.0).color(
                        if status == PermissionStatus::Granted {
                            TEAL
                        } else {
                            MUTED
                        },
                    ));
                });
            });
            ui.label(RichText::new(message).size(11.0).color(MUTED));
            if status != PermissionStatus::Unsupported {
                ui.horizontal(|ui| {
                    if cfg!(target_os = "macos")
                        && matches!(
                            status,
                            PermissionStatus::Required
                                | PermissionStatus::Denied
                                | PermissionStatus::Restricted
                        )
                        && button(ui, "Open System Settings", false, vec2(145.0, 28.0)).clicked()
                    {
                        self.send(Command::OpenPrivacySettings);
                    }
                    if button(
                        ui,
                        if screen_retry {
                            if status == PermissionStatus::Granted {
                                "Capture screen now"
                            } else {
                                "Retry screen capture"
                            }
                        } else {
                            "Refresh permission"
                        },
                        false,
                        vec2(145.0, 28.0),
                    )
                    .clicked()
                    {
                        self.notice = None;
                        self.send(if screen_retry {
                            Command::CaptureScreen
                        } else {
                            Command::RefreshPermissions
                        });
                    }
                });
            }
            ui.add_space(12.0);
        });
    }

    fn modes(&mut self, ui: &mut Ui) {
        settings_heading(ui, "Modes");
        ui.add_space(21.0);
        ui.label(
            RichText::new("Choose how Cueglint should help in your next conversation.")
                .size(12.0)
                .color(MUTED),
        );
        ui.add_space(21.0);
        let modes = self
            .state
            .as_ref()
            .map(|s| s.modes.clone())
            .unwrap_or_default();
        if self.mode_draft.is_none() {
            self.mode_draft = modes.first().cloned();
        }
        let rect = ui.available_rect_before_wrap();
        let left = Rect::from_min_size(rect.min, vec2(168.0, rect.height() - 82.0));
        let mut list = ui.new_child(UiBuilder::new().max_rect(left));
        section(&mut list, "Your modes");
        for mode in modes {
            if nav_row(
                &mut list,
                Icon::Grid,
                &mode.name,
                self.mode_draft.as_ref().is_some_and(|m| m.id == mode.id),
            )
            .clicked()
            {
                self.mode_draft = Some(mode);
            }
        }
        list.add_space(10.0);
        if button(&mut list, "+  New mode", false, vec2(166.0, 32.0)).clicked() {
            self.mode_draft = Some(Mode::new("New mode", ""));
        }
        let right = Rect::from_min_max(
            rect.min + vec2(194.0, 0.0),
            pos2(rect.right(), rect.bottom() - 90.0),
        );
        let mut details = ui.new_child(UiBuilder::new().max_rect(right));
        details.set_clip_rect(right.intersect(ui.clip_rect()));
        if let Some(mut mode) = self.mode_draft.clone() {
            let stored_mode = self
                .state
                .as_ref()
                .is_some_and(|state| state.modes.iter().any(|stored| stored.id == mode.id));
            let mut duplicate = false;
            egui::ScrollArea::vertical()
                .id_salt(("mode-editor", &mode.id))
                .max_height(right.height().max(0.0))
                .show(&mut details, |details| {
                    if mode.builtin {
                        details.label(RichText::new(&mode.name).size(17.0).strong());
                    } else {
                        details.add(
                            egui::TextEdit::singleline(&mut mode.name)
                                .font(FontId::proportional(16.0))
                                .desired_width(f32::INFINITY),
                        );
                    }
                    details.add_space(20.0);
                    section(details, "Meeting context");
                    if mode.builtin {
                        details.add(
                            egui::Label::new(
                                RichText::new(&mode.instructions).size(13.0).color(MUTED),
                            )
                            .wrap()
                            .selectable(true),
                        );
                    } else {
                        details.add_sized(
                            vec2(details.available_width(), 200.0),
                            egui::TextEdit::multiline(&mut mode.instructions)
                                .hint_text(
                                    "Describe your context, goals, and preferred response style…",
                                )
                                .margin(10.0),
                        );
                    }
                    details.add_space(22.0);
                    details.separator();
                    details.add_space(14.0);
                    section(details, "Reference files");
                    let mut all = mode.knowledge_document_ids.is_none();
                    if details
                        .checkbox(&mut all, "Search all library files")
                        .changed()
                    {
                        mode.knowledge_document_ids = (!all).then(Vec::new);
                    }
                    if let (Some(ids), Some(state)) =
                        (&mut mode.knowledge_document_ids, &self.state)
                    {
                        let available: std::collections::HashSet<_> =
                            state.knowledge.iter().map(|doc| doc.id.as_str()).collect();
                        ids.retain(|id| available.contains(id.as_str()));
                        if state.knowledge.is_empty() {
                            details.label(
                                RichText::new("Your library has no documents yet.")
                                    .size(12.0)
                                    .color(MUTED),
                            );
                        }
                        for document in &state.knowledge {
                            let index = ids.iter().position(|id| id == &document.id);
                            let mut attached = index.is_some();
                            let changed = details
                                .push_id(&document.id, |ui| {
                                    ui.checkbox(&mut attached, &document.title)
                                        .on_hover_text(&document.source)
                                        .changed()
                                })
                                .inner;
                            if changed {
                                if let Some(index) = index {
                                    ids.remove(index);
                                } else {
                                    ids.push(document.id.clone());
                                }
                            }
                        }
                    }
                    if button(details, "Open knowledge", false, vec2(138.0, 30.0)).clicked() {
                        self.visit_settings_page(Page::Knowledge);
                    }
                    details.add_space(20.0);
                    let label = details.label("Meeting note template");
                    details
                        .add(
                            egui::TextEdit::multiline(&mut mode.note_template)
                                .hint_text("Starting text for new meetings in this mode")
                                .desired_width(f32::INFINITY)
                                .desired_rows(3)
                                .id(Id::new(("mode-note-template", &mode.id))),
                        )
                        .labelled_by(label.id);
                    details.small("Place {{highlights}} on its own line to fill it with reviewed transcript quotes.");
                    let template_bytes = mode.note_template.len();
                    if template_bytes > cueglint_core::MAX_NOTE_TEMPLATE_BYTES {
                        details.label(format!(
                            "Template is {template_bytes} bytes. Shorten it to {} bytes to save.",
                            cueglint_core::MAX_NOTE_TEMPLATE_BYTES
                        ));
                    }
                    if mode.note_template.is_empty()
                        && details.small_button("Use meeting outline").clicked()
                    {
                        mode.note_template = cueglint_core::MEETING_NOTE_OUTLINE.to_owned();
                    }
                    details.add_space(14.0);
                    details.horizontal(|ui| {
                        if ui
                            .add_enabled_ui(
                                template_bytes <= cueglint_core::MAX_NOTE_TEMPLATE_BYTES,
                                |ui| button(ui, "Save mode", false, vec2(100.0, 30.0)),
                            )
                            .inner
                            .clicked()
                        {
                            self.send(Command::Mode {
                                mode: mode.clone(),
                                activate: false,
                            });
                        }
                        if stored_mode {
                            duplicate = ui.small_button("Duplicate as draft").clicked();
                            if !mode.builtin && ui.small_button("Delete").clicked() {
                                self.send(Command::DeleteMode(mode.id.clone()));
                                self.mode_draft = None;
                            }
                        }
                    });
                });
            if duplicate {
                mode = mode.duplicate();
            }
            if self.mode_draft.is_some() {
                self.mode_draft = Some(mode.clone());
            }
            let bar = Rect::from_min_max(pos2(rect.left(), rect.bottom() - 73.0), rect.max);
            ui.painter()
                .hline(bar.x_range(), bar.top(), Stroke::new(1.0, LINE));
            let mut bar_ui = ui.new_child(UiBuilder::new().max_rect(bar.shrink2(vec2(0.0, 18.0))));
            let saved = self
                .state
                .as_ref()
                .is_some_and(|state| state.modes.contains(&mode));
            bar_ui.horizontal(|ui| {
                ui.label(
                    RichText::new(if !saved {
                        "Unsaved mode"
                    } else if self.selected_mode.as_ref() == Some(&mode.id) {
                        "Active for the next session"
                    } else {
                        "Ready for your next conversation"
                    })
                    .size(11.0)
                    .color(MUTED),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if mode.note_template.len() > cueglint_core::MAX_NOTE_TEMPLATE_BYTES {
                        ui.disable();
                    }
                    if button(ui, "Use this mode", true, vec2(128.0, 32.0)).clicked() {
                        if saved {
                            self.selected_mode = Some(mode.id);
                            self.notice =
                                Some(("Mode selected for your next session.".into(), false));
                        } else {
                            self.send(Command::Mode {
                                mode,
                                activate: true,
                            });
                        }
                    }
                });
            });
        }
    }

    fn import_knowledge(&mut self) {
        if self.knowledge_import.is_some() || self.import_path.trim().is_empty() {
            return;
        }
        let command = ImportCommand::new(PathBuf::from(self.import_path.trim()));
        if self.send(Command::Import(command.clone())) {
            self.knowledge_import = Some(command);
        }
    }

    fn knowledge(&mut self, ui: &mut Ui) {
        settings_heading(ui, "Knowledge");
        ui.add_space(23.0);
        hero(
            ui,
            Icon::File,
            "Your knowledge",
            "Keep useful context close.\nText, Markdown, DOCX and text-based PDF documents are stored and searched locally.",
        );
        ui.add_space(20.0);
        section(ui, "Add a document");
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.import_path)
                        .interactive(self.knowledge_import.is_none())
                        .hint_text("Paste a local file path, or drop a file here")
                        .desired_width((ui.available_width() - 80.0).max(100.0)),
                );
                if ui
                    .add_enabled_ui(
                        self.knowledge_import.is_none() && !self.import_path.trim().is_empty(),
                        |ui| button(ui, "Import", true, vec2(66.0, 28.0)),
                    )
                    .inner
                    .clicked()
                    && !self.import_path.trim().is_empty()
                {
                    self.import_knowledge();
                }
            });
            ui.add_space(8.0);
            ui.label(
                RichText::new("UTF-8 text, Markdown, DOCX or text-based PDF · up to 2 MiB")
                    .size(11.0)
                    .color(MUTED),
            );
            if let Some(import) = &self.knowledge_import {
                ui.horizontal(|ui| {
                    ui.label(if import.cancelled() {
                        "Cancelling document import…"
                    } else {
                        "Reading document locally…"
                    });
                    if ui
                        .add_enabled(!import.cancelled(), egui::Button::new("Cancel import"))
                        .clicked()
                    {
                        import.cancel();
                    }
                });
            }
        });
        ui.add_space(20.0);
        section(ui, "Documents");
        let documents = self
            .state
            .as_ref()
            .map(|s| s.knowledge.clone())
            .unwrap_or_default();
        egui::ScrollArea::vertical()
            .max_height(210.0)
            .show(ui, |ui| {
                if documents.is_empty() {
                    card(ui, |ui| {
                        ui.label(RichText::new("No documents yet").size(13.0));
                        ui.label(
                            RichText::new("Import a file to add searchable context.")
                                .size(12.0)
                                .color(MUTED),
                        );
                    });
                }
                for document in documents {
                    card(ui, |ui| {
                        ui.horizontal(|ui| {
                            icon(ui, Icon::File, 22.0, MUTED);
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&document.title).size(13.0));
                                ui.label(
                                    RichText::new(format!(
                                        "{} characters · local",
                                        document.text.chars().count()
                                    ))
                                    .size(11.0)
                                    .color(MUTED),
                                );
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui
                                    .small_button(
                                        if self.delete_document.as_ref() == Some(&document.id) {
                                            "Confirm delete"
                                        } else {
                                            "Delete"
                                        },
                                    )
                                    .clicked()
                                {
                                    if self.delete_document.as_ref() == Some(&document.id) {
                                        self.send(Command::DeleteKnowledge(document.id.clone()));
                                        self.delete_document = None;
                                    } else {
                                        self.delete_document = Some(document.id.clone());
                                    }
                                }
                            });
                        });
                    });
                    ui.add_space(6.0);
                }
            });
    }

    fn shortcuts(&mut self, ui: &mut Ui) {
        // Reviewed screenshot pixels, with capture DPI unknown; native visual verification is pending.
        let (surface, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), 646.0), Sense::hover());
        let origin = surface.min;
        let mut title = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(
            origin + vec2(93.0, 7.0),
            vec2(400.0, 28.0),
        )));
        title.label(RichText::new("Keyboard Shortcuts").size(14.0).strong());
        let mut columns = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(
            origin + vec2(16.0, 64.0),
            vec2(560.0, 24.0),
        )));
        columns.label(RichText::new("Name").size(11.0).color(MUTED));
        ui.painter().text(
            origin + vec2(514.0, 73.0),
            egui::Align2::LEFT_CENTER,
            "Hotkey",
            FontId::proportional(11.0),
            MUTED,
        );
        let bindings = self
            .state
            .as_ref()
            .map(|state| state.settings.shortcut_bindings.clone())
            .unwrap_or_default();
        let editable = self.pending == 0 && self.state.is_some() && !self.capture_busy();
        for (index, action) in Action::ALL.into_iter().enumerate() {
            let top = origin.y + 91.0 + index as f32 * 48.0;
            let center_y = top + 24.0;
            ui.painter()
                .hline(surface.x_range(), top, Stroke::new(1.0, LINE));
            ui.painter().text(
                pos2(origin.x + 44.0, center_y),
                egui::Align2::LEFT_CENTER,
                action.label(),
                FontId::proportional(13.0),
                TEXT,
            );
            draw_icon(
                ui.painter(),
                pos2(origin.x + 25.0, center_y),
                match action {
                    Action::Visibility => Icon::Eye,
                    Action::NewChat | Action::Ask => Icon::Glint,
                    Action::Settings => Icon::Gear,
                    Action::Session => Icon::Wave,
                    Action::ScrollUp | Action::ScrollDown => Icon::Chevron,
                    _ => Icon::Back,
                },
                16.0,
                TEXT,
            );
            let status = self.shortcuts.status(action);
            let recording = self.recording_shortcut == Some(action);
            let configured = shortcuts::binding(&bindings, action);
            let tokens: Vec<&str> = if recording {
                vec!["Press keys…"]
            } else {
                configured
                    .map(|binding| binding.split('+').collect())
                    .unwrap_or_else(|| vec!["Unset"])
            };
            let widths: Vec<f32> = tokens
                .iter()
                .map(|token| {
                    if matches!(*token, "Left" | "Right" | "Up" | "Down") {
                        20.0
                    } else {
                        (shortcut_token_label(token).chars().count() as f32 * 6.0 + 8.0).max(20.0)
                    }
                })
                .collect();
            let width = widths.iter().sum::<f32>() + tokens.len().saturating_sub(1) as f32 * 2.0;
            let left = origin.x + 549.0 - width;
            let record_rect = Rect::from_min_size(pos2(left, center_y - 12.0), vec2(width, 24.0));
            let response = ui
                .add_enabled_ui(editable, |ui| {
                    let response = ui.interact(
                        record_rect,
                        Id::new(("shortcut-record", action.id())),
                        Sense::click(),
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            response.enabled(),
                            format!(
                                "Record shortcut for {}; {}; {}",
                                action.label(),
                                configured.unwrap_or("Unset"),
                                status.label()
                            ),
                        )
                    });
                    let mut x = left;
                    for (token, width) in tokens.iter().zip(&widths) {
                        let cap = Rect::from_min_size(pos2(x, center_y - 10.0), vec2(*width, 20.0));
                        ui.painter().rect(
                            cap,
                            4,
                            Color32::from_rgb(61, 58, 60),
                            Stroke::new(1.0, Color32::from_rgb(77, 73, 76)),
                            StrokeKind::Inside,
                        );
                        let direction = match *token {
                            "Left" => Some(vec2(-1.0, 0.0)),
                            "Right" => Some(vec2(1.0, 0.0)),
                            "Up" => Some(vec2(0.0, -1.0)),
                            "Down" => Some(vec2(0.0, 1.0)),
                            _ => None,
                        };
                        if let Some(direction) = direction {
                            let side = vec2(-direction.y, direction.x);
                            let tip = cap.center() + direction * 4.0;
                            ui.painter().line_segment(
                                [cap.center() - direction * 4.0, tip],
                                Stroke::new(1.2, TEXT),
                            );
                            for sign in [-1.0, 1.0] {
                                ui.painter().line_segment(
                                    [tip, cap.center() + side * sign * 3.0],
                                    Stroke::new(1.2, TEXT),
                                );
                            }
                        } else {
                            ui.painter().text(
                                cap.center(),
                                egui::Align2::CENTER_CENTER,
                                shortcut_token_label(token),
                                FontId::proportional(11.0),
                                TEXT,
                            );
                        }
                        x += *width + 2.0;
                    }
                    response
                })
                .inner
                .on_hover_text(format!(
                    "{}: {}. {}{}",
                    action.label(),
                    configured.unwrap_or("Unset"),
                    status.label(),
                    if status.detail().is_empty() {
                        String::new()
                    } else {
                        format!(". {}", status.detail())
                    }
                ))
                .on_disabled_hover_text(
                    "Stop listening and wait for the current operation before editing shortcuts.",
                );
            if response.clicked() {
                self.recording_shortcut = Some(action);
                self.notice = None;
                ui.ctx().request_repaint();
            }
            if matches!(
                status,
                shortcuts::Status::Conflict(_)
                    | shortcuts::Status::Invalid(_)
                    | shortcuts::Status::Failed(_)
                    | shortcuts::Status::RestartRequired(_)
            ) {
                let marker = Rect::from_center_size(pos2(left - 12.0, center_y), vec2(12.0, 20.0));
                ui.painter()
                    .circle_filled(marker.center(), 2.5, Color32::from_rgb(245, 190, 134));
                ui.interact(
                    marker,
                    Id::new(("shortcut-status", action.id())),
                    Sense::hover(),
                )
                .on_hover_text(format!("{}: {}", status.label(), status.detail()));
            }
            if status.retryable() {
                let mut retry = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(
                    pos2(left - 76.0, center_y - 14.0),
                    vec2(48.0, 28.0),
                )));
                if retry
                    .add_enabled(
                        editable && self.recording_shortcut.is_none(),
                        egui::Button::new("Retry").min_size(vec2(45.0, 26.0)),
                    )
                    .on_hover_text(status.detail())
                    .clicked()
                {
                    self.retry_shortcut = Some(action);
                    ui.ctx().request_repaint();
                }
            }
            let reset_rect =
                Rect::from_center_size(pos2(origin.x + 575.0, center_y), vec2(24.0, 28.0));
            let reset = ui
                .add_enabled_ui(editable && bindings.contains_key(action.id()), |ui| {
                    let response = ui.interact(
                        reset_rect,
                        Id::new(("shortcut-reset", action.id())),
                        Sense::click(),
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            response.enabled(),
                            format!("Reset shortcut for {}", action.label()),
                        )
                    });
                    let color = if response.enabled() {
                        TEXT
                    } else {
                        Color32::from_gray(91)
                    };
                    let center = reset_rect.center();
                    let points: Vec<_> = (0..=15)
                        .map(|step| {
                            let angle = -1.2 + step as f32 / 15.0 * 5.0;
                            center + vec2(angle.cos(), angle.sin()) * 4.5
                        })
                        .collect();
                    ui.painter()
                        .add(egui::Shape::line(points, Stroke::new(1.1, color)));
                    ui.painter().line_segment(
                        [center + vec2(1.5, -4.2), center + vec2(1.5, -0.8)],
                        Stroke::new(1.1, color),
                    );
                    ui.painter().line_segment(
                        [center + vec2(1.5, -4.2), center + vec2(-1.7, -3.5)],
                        Stroke::new(1.1, color),
                    );
                    response
                })
                .inner
                .on_hover_text(format!("Restore the default for {}", action.label()));
            if reset.clicked() {
                self.save_shortcut(action, None, true);
            }
        }
        let helper = if self.recording_shortcut.is_some() {
            "Press Command or Control with a key. Escape cancels; Delete clears."
        } else if self.capture_busy() {
            "Stop the session before editing shortcuts. Existing global bindings stay active."
        } else if self.fixture.is_some() {
            "[Fixture] Global shortcuts are not registered. Keycaps edit only this isolated preview."
        } else if Action::ALL.into_iter().any(|action| {
            matches!(
                self.shortcuts.status(action),
                shortcuts::Status::RestartRequired(_)
            )
        }) {
            "Shortcut cleanup needs a restart. Stop your session, then quit and reopen Cueglint."
        } else if cfg!(target_os = "linux") {
            "Global shortcuts are unavailable on X11 and Wayland in this build; focused shortcuts work."
        } else {
            "Click a shortcut to change it. Escape cancels, Delete clears, and reset restores the default."
        };
        let mut help = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(
            origin + vec2(0.0, 631.0),
            vec2(surface.width(), 20.0),
        )));
        help.label(RichText::new(helper).size(11.0).color(MUTED));
    }

    fn security(&mut self, ui: &mut Ui) {
        settings_heading(ui, "Security");
        ui.add_space(23.0);
        hero(
            ui,
            Icon::Lock,
            "Local by default",
            "Your conversations are stored on this device.\nRemote providers receive selected context when you send a question.",
        );
        ui.add_space(24.0);
        card(ui, |ui| {
            row(
                ui,
                "Session storage",
                "SQLite in your per-user application data folder",
                |ui| {
                    ui.label(RichText::new("Local").size(12.0).color(TEAL));
                },
            );
            rule(ui);
            row(
                ui,
                "Screen capture protection",
                "The assistant may appear in screenshots and screen sharing",
                |ui| {
                    ui.label(RichText::new("Unavailable").size(11.0).color(MUTED));
                },
            );
            rule(ui);
            row(
                ui,
                "Encryption",
                "Database files are not encrypted by this application",
                |ui| {
                    ui.label(RichText::new("OS protection").size(11.0).color(MUTED));
                },
            );
        });
        ui.add_space(20.0);
        self.backup_settings(ui);
    }

    fn about(&mut self, ui: &mut Ui) {
        settings_heading(ui, "About");
        ui.add_space(23.0);
        hero(
            ui,
            Icon::Glint,
            "Cueglint",
            "A quiet place for the context you need.\nAn independent native Rust implementation.",
        );
        ui.add_space(24.0);
        card(ui, |ui| {
            row(ui, "Version", "Development build", |ui| {
                ui.label(
                    RichText::new(env!("CARGO_PKG_VERSION"))
                        .size(12.0)
                        .color(MUTED),
                );
            });
            rule(ui);
            row(ui, "UI", "Native windows · egui · wgpu", |ui| {
                ui.label(RichText::new("Rust").size(12.0).color(TEAL));
            });
            rule(ui);
            row(
                ui,
                "Availability",
                "Manual transcripts, notes, history, modes, and knowledge",
                |ui| {
                    ui.label(RichText::new("Local").size(12.0).color(MUTED));
                },
            );
        });
        ui.add_space(24.0);
        if button(ui, "Quit Cueglint", false, vec2(120.0, 32.0)).clicked() {
            self.request_quit(ui.ctx());
        }
    }

    fn unavailable(&mut self, ui: &mut Ui, glyph: Icon, title: &str, subtitle: &str) {
        settings_heading(ui, title);
        ui.add_space(23.0);
        hero(ui, glyph, title, subtitle);
        ui.add_space(24.0);
        card(ui, |ui| {
            ui.label(RichText::new("Not connected").size(13.0));
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "This section is a visible capability boundary, not a connected service.",
                )
                .size(12.0)
                .color(MUTED),
            );
        });
    }

    fn assistant_height(&self) -> f32 {
        match self.overlay_panel {
            OverlayPanel::Compact => {
                if self.mode_menu {
                    104.0
                        + self
                            .state
                            .as_ref()
                            .map(|state| state.modes.len() + 1)
                            .unwrap_or(1) as f32
                            * 32.0
                        + 16.0
                } else {
                    104.0
                }
            }
            OverlayPanel::Chat => 500.0,
            _ => 760.0,
        }
    }

    fn overlay(&mut self, ctx: &egui::Context) {
        if !self.overlay_open {
            return;
        }
        let height = self.assistant_height()
            + if self.has_calendar_reminder(self.calendar_reminder_now()) {
                reminder_ui::BANNER_HEIGHT
            } else {
                0.0
            };
        ctx.show_viewport_immediate(
            overlay_id(),
            egui::ViewportBuilder::default()
                .with_title("Cueglint Assistant")
                .with_inner_size([690.0, height])
                .with_min_inner_size([690.0, height])
                .with_max_inner_size([690.0, height])
                .with_position([440.0, 90.0])
                .with_decorations(false)
                .with_transparent(true)
                .with_always_on_top()
                .with_resizable(false),
            |ui, _class| {
                if ui.ctx().input(|i| i.viewport().close_requested()) {
                    self.show_settings(ui.ctx());
                }
                self.assistant_surface(ui);
            },
        );
        if self.focus_assistant && !self.backup_restore_blocks_controls() {
            if self.fixture.is_none() {
                ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Visible(false));
                self.settings_visible = false;
            }
            ctx.send_viewport_cmd_to(overlay_id(), ViewportCommand::Focus);
            self.focus_assistant = false;
        }
    }

    fn assistant_surface(&mut self, ui: &mut Ui) {
        if self.backup_restore_blocks_controls() {
            ui.disable();
        }
        self.keyboard(ui.ctx());
        self.capture_screenshot(ui.ctx());
        let now = self.calendar_reminder_now();
        let rect = ui.max_rect();
        let painted_reminder = if self.has_calendar_reminder(now)
            && rect.height() >= self.assistant_height() + reminder_ui::BANNER_HEIGHT - 1.0
        {
            let mut body_rect = rect;
            body_rect.max.y -= reminder_ui::BANNER_HEIGHT;
            let mut body = ui.new_child(UiBuilder::new().max_rect(body_rect));
            self.assistant_content(&mut body);
            let banner_rect =
                Rect::from_min_max(pos2(rect.left(), body_rect.bottom() + 4.0), rect.max);
            let mut banner = ui.new_child(UiBuilder::new().max_rect(banner_rect));
            self.calendar_reminder_banner(&mut banner, now)
        } else {
            self.assistant_content(ui);
            false
        };
        if self.calendar_reminder_preview() {
            self.calendar_reminder_fixture_ready = painted_reminder;
        }
    }

    fn assistant_content(&mut self, ui: &mut Ui) {
        let mut rect = ui.max_rect();
        if self.overlay_panel == OverlayPanel::Compact {
            rect.max.y = rect.min.y + 104.0;
            let input_rect = Rect::from_min_max(rect.min, pos2(rect.right(), rect.top() + 56.0));
            ui.painter().rect_filled(
                input_rect,
                CornerRadius {
                    nw: 20,
                    ne: 20,
                    sw: 0,
                    se: 0,
                },
                Color32::from_rgba_unmultiplied(119, 120, 122, 218),
            );
            self.question_input(ui, rect.top(), false);
            self.toolbar(ui, rect.top() + 56.0, false);
            if self.mode_menu {
                self.mode_popup(ui);
            }
        } else {
            if matches!(
                self.overlay_panel,
                OverlayPanel::History | OverlayPanel::Detail
            ) {
                ui.painter().rect_filled(rect, 20, Color32::BLACK);
            }
            match self.overlay_panel {
                OverlayPanel::History => self.history(ui),
                OverlayPanel::Chat => self.conversation(ui),
                OverlayPanel::Detail => self.detail(ui),
                OverlayPanel::Compact => {}
            }
            if self.overlay_panel == OverlayPanel::Chat && self.mode_menu {
                self.mode_popup(ui);
            }
        }
    }

    fn capture_screenshot(&mut self, ctx: &egui::Context) {
        if self.screenshot.is_none() {
            return;
        }
        let Some(request) = &self.screenshot_request else {
            return;
        };
        let request_id = request.id().to_string();
        let screenshot = ctx.input(|input| {
            input.events.iter().find_map(|event| {
                if let egui::Event::Screenshot {
                    image,
                    viewport_id,
                    user_data,
                } = event
                {
                    request
                        .matches(*viewport_id, user_data)
                        .then(|| image.clone())
                } else {
                    None
                }
            })
        });
        if let Some(image) = screenshot {
            self.trace_screenshot(ScreenshotStage::EventReceived, ctx);
            if let Some(path) = self.screenshot.take() {
                let job = screenshot::Job {
                    path,
                    image,
                    metadata: screenshot::Metadata {
                        request_id,
                        fixture: self.fixture.clone(),
                        pixels_per_point: ctx.pixels_per_point(),
                        zoom_factor: ctx.zoom_factor(),
                        adapter: self.screenshot_adapter.clone(),
                        fonts: self.fonts.receipt(),
                    },
                };
                match self.worker.screenshot.try_send(job) {
                    Ok(()) => self.trace_screenshot(ScreenshotStage::Queued, ctx),
                    Err(error) => {
                        self.trace_screenshot(ScreenshotStage::QueueError, ctx);
                        self.notice = Some((format!("Screenshot queue failed: {error}"), true));
                    }
                }
            }
        }
    }

    fn question_input(&mut self, ui: &mut Ui, y: f32, expanded: bool) {
        let rect = ui.max_rect();
        let input = Rect::from_min_max(
            pos2(
                rect.left()
                    + if self.overlay_panel == OverlayPanel::Chat {
                        55.0
                    } else {
                        15.0
                    },
                y + 21.0,
            ),
            pos2(rect.right() - 61.0, y + 49.0),
        );
        let mut field = ui.new_child(UiBuilder::new().max_rect(input));
        let response = field.add_sized(
            input.size(),
            egui::TextEdit::singleline(&mut self.question)
                .id(Id::new("question-input"))
                .interactive(self.question_submission.is_none())
                .hint_text(if self.overlay_panel == OverlayPanel::Chat {
                    "Ask follow-up…"
                } else {
                    "Ask anything…"
                })
                .font(FontId::proportional(15.0))
                .frame(Frame::NONE)
                .margin(vec2(0.0, 3.0)),
        );
        let submit = (response.has_focus() || response.lost_focus())
            && field.input(|i| i.key_pressed(egui::Key::Enter));
        let send_rect = Rect::from_min_size(pos2(rect.right() - 44.0, y + 12.0), vec2(32.0, 32.0));
        let generating = self.generating();
        let can_send = self.answer_ready()
            && !self.question.trim().is_empty()
            && !self.selected_chat_requires_local();
        let send = ui.add_enabled_ui(generating || can_send, |ui| {
            icon_button_at(
                ui,
                send_rect,
                if generating { Icon::Stop } else { Icon::Return },
                if generating { "Stop response" } else { "Send question" },
                Color32::from_rgb(151, 152, 154),
            )
        }).inner.on_disabled_hover_text(if self.question_submission.is_some() {
            "Preparing your question. The draft stays here until its context is accepted."
        } else if self.fixture.is_some() {
            "Local inference is disabled in this isolated fixture."
        } else if self.model_job_busy() {
            "Wait for the model file operation to finish. Your draft stays here."
        } else if self.pending > 0 || self.state.is_none() {
            "Wait for the current operation to finish. Your draft stays here."
        } else if self.question.trim().is_empty() {
            "Type a question to send."
        } else if self.selected_chat_requires_local() {
            "Select Local in Settings to continue this History chat."
        } else {
            if self.runtime.answer_provider.provider.is_local() {
                "Install the selected answer model in Settings before sending. Your draft stays here."
            } else {
                "Choose a model ID and valid endpoint in Settings before sending. Your draft stays here."
            }
        });
        if generating {
            if send.clicked() {
                self.send(Command::Cancel);
            }
        } else if can_send && (submit || send.clicked()) {
            self.ask();
        }
        if expanded {
            ui.painter()
                .hline(rect.x_range(), y, Stroke::new(1.0, Color32::from_gray(36)));
        }
        // shortcut: only the unused input gutter drags the native window; expand the drag area when native hit testing is added.
        let drag = ui.interact(
            Rect::from_min_max(pos2(rect.left(), y), pos2(rect.right() - 50.0, y + 10.0)),
            Id::new("overlay-drag"),
            Sense::drag(),
        );
        drag.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Drag assistant")
        });
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }
    }

    fn toolbar(&mut self, ui: &mut Ui, y: f32, expanded: bool) {
        let origin = pos2(ui.max_rect().left(), y);
        let toolbar = Rect::from_min_size(origin, vec2(690.0, 48.0));
        ui.painter().rect_filled(
            toolbar,
            CornerRadius {
                nw: 0,
                ne: 0,
                sw: 20,
                se: 20,
            },
            if expanded {
                Color32::from_rgb(17, 17, 17)
            } else {
                Color32::from_rgba_unmultiplied(78, 79, 81, 218)
            },
        );
        if icon_button_at(
            ui,
            Rect::from_min_size(origin + vec2(16.0, 8.0), vec2(32.0, 32.0)),
            Icon::Gear,
            "Open settings",
            Color32::TRANSPARENT,
        )
        .clicked()
        {
            self.visit_settings_page(Page::General);
            self.show_settings(ui.ctx());
        }
        if let Some(message) = self.blocked_capture_permission() {
            let message = if self.fixture.is_some() {
                format!("FIXTURE · {message}")
            } else {
                message.into()
            };
            let mut status = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(
                origin + vec2(57.0, 10.0),
                vec2(195.0, 30.0),
            )));
            if status
                .add(
                    egui::Button::new(
                        RichText::new(message)
                            .size(10.0)
                            .color(Color32::from_rgb(255, 215, 165)),
                    )
                    .fill(Color32::from_white_alpha(10))
                    .min_size(vec2(195.0, 28.0)),
                )
                .clicked()
            {
                self.visit_settings_page(Page::General);
                self.reveal_permissions = true;
                self.show_settings(ui.ctx());
            }
        } else if let Some((message, error)) = self.notice.clone() {
            let message = if self.fixture.is_some() {
                format!("FIXTURE · {message}")
            } else {
                message
            };
            let mut status = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(
                origin + vec2(58.0, 16.0),
                vec2(190.0, 20.0),
            )));
            status
                .add(
                    egui::Label::new(RichText::new(message.clone()).size(10.0).color(if error {
                        Color32::from_rgb(255, 197, 187)
                    } else {
                        Color32::from_gray(220)
                    }))
                    .truncate(),
                )
                .on_hover_text(message);
        } else if self.fixture.is_some() {
            let painter = ui.painter();
            painter.text(
                origin + vec2(58.0, 24.0),
                egui::Align2::LEFT_CENTER,
                "FIXTURE",
                FontId::proportional(9.0),
                Color32::from_gray(195),
            );
        }
        let screen_response = icon_button_at(
            ui,
            Rect::from_min_size(origin + vec2(265.0, 8.0), vec2(32.0, 32.0)),
            Icon::Screen,
            if self.runtime.screen_enabled {
                "Disable screen context"
            } else {
                "Enable screen context"
            },
            if self.runtime.screen_enabled {
                BLUE.gamma_multiply(0.5)
            } else {
                Color32::TRANSPARENT
            },
        );
        if screen_response.clicked() {
            let enabled = !self.runtime.screen_enabled;
            self.send(Command::ScreenEnabled(enabled));
            if enabled {
                self.send(Command::CaptureScreen);
            }
        }
        let history_chat = self.selected().is_some_and(|session| session.history_chat);
        screen_response.on_hover_text(if history_chat {
            "Screen context applies to ordinary assistant questions. History chat uses saved meeting text.".to_owned()
        } else {
            format!("Read text with local OCR when asking. A deliberate question sends enabled screen text to {}. Click to enable or disable.", self.answer_destination())
        });
        disabled_icon_at(
            ui,
            Rect::from_min_size(origin + vec2(307.0, 8.0), vec2(32.0, 32.0)),
            Icon::Eye,
            "Capture protection unavailable. This window may appear in screenshots and screen sharing.",
        );
        let mode_name = self
            .state
            .as_ref()
            .and_then(|state| {
                state
                    .modes
                    .iter()
                    .find(|mode| self.selected_mode.as_ref() == Some(&mode.id))
            })
            .map(|mode| mode.name.clone())
            .unwrap_or_else(|| "General".into());
        if icon_button_at(
            ui,
            Rect::from_min_size(origin + vec2(349.0, 8.0), vec2(32.0, 32.0)),
            Icon::Grid,
            &format!("Mode: {mode_name}"),
            Color32::TRANSPARENT,
        )
        .clicked()
        {
            self.mode_menu = !self.mode_menu;
        }
        let active = self
            .active_session()
            .map(|session| (session.id.clone(), session.status));
        let listening = self
            .runtime
            .capture
            .as_ref()
            .is_some_and(|capture| capture.state == "listening");
        let wave = icon_button_at(
            ui,
            Rect::from_min_size(origin + vec2(393.0, 8.0), vec2(32.0, 32.0)),
            Icon::Wave,
            if listening {
                "Pause listening"
            } else {
                "Start or pause listening"
            },
            if listening {
                TEAL.gamma_multiply(0.32)
            } else {
                Color32::TRANSPARENT
            },
        );
        if wave.clicked() {
            if let Some((id, status)) = active {
                match status {
                    SessionStatus::Recording => {
                        self.send(Command::Pause(id));
                    }
                    SessionStatus::Paused => {
                        self.send(Command::Resume(id));
                    }
                    SessionStatus::Stopped => self.start(),
                }
            } else {
                self.start();
            }
        }
        wave.on_hover_text("Start or pause a session. Listening requires the verified local Whisper model and OS permission.");
        let config = &self.runtime.answer_provider;
        let mut destination = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(
            origin + vec2(435.0, 8.0),
            vec2(140.0, 34.0),
        )));
        let selected = if config.provider.is_local() || history_chat {
            format!("Local · {}", self.generation_profile())
        } else {
            format!("Remote {} · {}", config.provider.label(), config.model_id)
        };
        destination
            .add(egui::Label::new(RichText::new(selected).size(9.0)).truncate())
            .on_hover_text(if history_chat {
                "History chat runs on this device".to_owned()
            } else {
                self.answer_destination()
            });
        let target = if history_chat && !config.provider.is_local() {
            "Select Local to continue".into()
        } else if config.provider.is_local() {
            "Context stays on device".into()
        } else {
            config
                .endpoint()
                .ok()
                .and_then(|endpoint| url::Url::parse(&endpoint).ok())
                .and_then(|url| url.host_str().map(|host| format!("To: {host}")))
                .unwrap_or_else(|| "Choose an endpoint".into())
        };
        destination.add(egui::Label::new(RichText::new(target).size(9.0).color(MUTED)).truncate())
            .on_hover_text(if history_chat {
                "History chat uses saved meeting excerpts and prior replies locally.".to_owned()
            } else {
                format!("{}\nSending a question shares its selected context here. Audio transcription stays local.", self.answer_destination())
            });
        let history = Rect::from_min_size(origin + vec2(580.0, 9.0), vec2(98.0, 30.0));
        let response = ui.interact(history, Id::new("overlay-history"), Sense::click());
        let new_chat = self.overlay_panel == OverlayPanel::Chat;
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                true,
                if new_chat { "New chat" } else { "Open history" },
            )
        });
        if response.hovered() {
            ui.painter()
                .rect_filled(history, 8, Color32::from_white_alpha(18));
        }
        let badge = Rect::from_min_size(origin + vec2(644.0, 9.0), vec2(32.0, 30.0));
        let label_right = badge.left() - 2.0;
        let mut label = egui::text::LayoutJob::simple_singleline(
            if new_chat { "New Chat" } else { "History" }.into(),
            FontId::proportional(13.0),
            TEXT,
        );
        label.wrap = egui::text::TextWrapping::truncate_at_width(label_right - history.left());
        let galley = ui.painter().layout_job(label);
        let label_x = (origin.x + 592.0).min(label_right - galley.size().x);
        let rect =
            egui::Align2::LEFT_CENTER.anchor_size(pos2(label_x, badge.center().y), galley.size());
        ui.painter().galley(rect.min, galley, TEXT);
        ui.painter()
            .rect_filled(badge, 8, Color32::from_white_alpha(13));
        if new_chat {
            let configured = self.state.as_ref().and_then(|state| {
                shortcuts::binding(&state.settings.shortcut_bindings, Action::NewChat)
            });
            response.clone().on_hover_text(
                configured
                    .map(|binding| format!("New chat · {binding}"))
                    .unwrap_or_else(|| "New chat · no shortcut assigned".into()),
            );
            ui.painter().text(
                origin + vec2(660.0, 24.0),
                egui::Align2::CENTER_CENTER,
                if configured == Action::NewChat.default_binding() {
                    if cfg!(target_os = "macos") {
                        "⌘R"
                    } else {
                        "CtrlR"
                    }
                } else {
                    "+"
                },
                FontId::proportional(10.0),
                TEXT,
            );
        } else {
            draw_icon(
                ui.painter(),
                origin + vec2(660.0, 24.0),
                Icon::Chevron,
                14.0,
                TEXT,
            );
        }
        if response.clicked() {
            if new_chat {
                self.new_chat();
            } else {
                self.overlay_panel = OverlayPanel::History;
            }
        }
    }

    fn mode_popup(&mut self, ui: &mut Ui) {
        let modes = self
            .state
            .as_ref()
            .map(|state| state.modes.clone())
            .unwrap_or_default();
        let height = (modes.len() + 1) as f32 * 32.0 + 12.0;
        let y = if self.overlay_panel == OverlayPanel::Compact {
            104.0
        } else {
            (ui.max_rect().height() - 48.0 - height).max(56.0)
        };
        let rect = Rect::from_min_size(ui.max_rect().min + vec2(345.0, y), vec2(168.0, height));
        ui.painter()
            .rect_filled(rect, 9, Color32::from_rgb(25, 25, 29));
        let mut menu = ui.new_child(UiBuilder::new().max_rect(rect.shrink2(vec2(5.0, 6.0))));
        menu.spacing_mut().item_spacing.y = 0.0;
        for mode in modes {
            let (row, response) = menu.allocate_exact_size(vec2(158.0, 32.0), Sense::click());
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::SelectableLabel,
                    true,
                    self.selected_mode.as_ref() == Some(&mode.id),
                    &mode.name,
                )
            });
            if response.hovered() {
                menu.painter().rect_filled(row, 5, Color32::from_gray(52));
            }
            menu.painter().text(
                row.left_center() + vec2(8.0, 0.0),
                egui::Align2::LEFT_CENTER,
                &mode.name,
                FontId::proportional(13.0),
                TEXT,
            );
            if self.selected_mode.as_ref() == Some(&mode.id) {
                menu.painter().text(
                    row.right_center() - vec2(8.0, 0.0),
                    egui::Align2::RIGHT_CENTER,
                    "✓",
                    FontId::proportional(13.0),
                    TEXT,
                );
            }
            if response.clicked() {
                self.selected_mode = Some(mode.id.clone());
                if let Some(id) = self.selected().map(|session| session.id.clone()) {
                    self.send(Command::SessionMode(id, Some(mode.id)));
                }
                self.mode_menu = false;
            }
        }
        menu.separator();
        if button(&mut menu, "Manage modes…", false, vec2(158.0, 29.0)).clicked() {
            self.mode_menu = false;
            self.visit_settings_page(Page::Modes);
            self.show_settings(ui.ctx());
        }
    }

    fn history_search_field(&mut self, ui: &mut Ui) -> egui::Response {
        let rect = ui.max_rect();
        let mut search = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_max(
            rect.min + vec2(55.0, 17.0),
            pos2(rect.right() - 52.0, rect.top() + 43.0),
        )));
        let response = search.add(
            egui::TextEdit::singleline(&mut self.history_search)
                .id(Id::new("history-search"))
                .hint_text("Search your sessions")
                .font(FontId::proportional(14.0))
                .frame(Frame::NONE)
                .desired_width(f32::INFINITY),
        );
        draw_icon(
            ui.painter(),
            pos2(rect.right() - 30.0, rect.top() + 28.0),
            Icon::Search,
            18.0,
            MUTED,
        );
        response
    }

    fn history(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        let header = Rect::from_min_size(rect.min, vec2(rect.width(), 56.0));
        ui.painter().rect_filled(
            header,
            CornerRadius {
                nw: 20,
                ne: 20,
                sw: 0,
                se: 0,
            },
            Color32::from_rgb(15, 15, 15),
        );
        if icon_button_at(
            ui,
            Rect::from_min_size(rect.min + vec2(8.0, 8.0), vec2(40.0, 40.0)),
            Icon::Back,
            "Collapse history",
            Color32::from_gray(28),
        )
        .clicked()
        {
            self.overlay_panel = OverlayPanel::Compact;
        }
        self.history_search_field(ui);
        let body = Rect::from_min_max(rect.min + vec2(12.0, 67.0), rect.max - vec2(12.0, 56.0));
        let mut body_ui = ui.new_child(UiBuilder::new().max_rect(body));
        let sessions = self
            .state
            .as_ref()
            .map(|s| s.sessions.clone())
            .unwrap_or_default();
        let query = self.history_search.trim().to_lowercase();
        egui::ScrollArea::vertical().show(&mut body_ui, |ui| {
            if query.is_empty() {
                self.history_calendar(ui);
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new("YOUR SESSIONS").size(11.0).color(MUTED));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if button(ui, "+ New chat", false, vec2(97.0, 28.0)).clicked() {
                        self.new_chat();
                    }
                });
            });
            ui.add_space(10.0);
            if self.fixture.is_some() && !self.calendar_preview() {
                ui.label(
                    RichText::new("Sample data · isolated in-memory fixture")
                        .size(11.0)
                        .color(TEAL),
                );
                ui.add_space(8.0);
            }
            self.history_question_action(ui);
            let mut group = String::new();
            let mut count = 0;
            for session in sessions.iter().filter(|s| s.matches_history_query(&query)) {
                let date = date_label(session.created_at);
                if group != date {
                    group = date;
                    ui.add_space(15.0);
                    ui.label(
                        RichText::new(&group)
                            .size(11.0)
                            .color(Color32::from_gray(150)),
                    );
                    ui.add_space(8.0);
                }
                if history_row(ui, session).clicked() {
                    self.after_saving_notes(Command::Open(session.id.clone()));
                    self.overlay_panel = if session.history_chat {
                        OverlayPanel::Chat
                    } else {
                        OverlayPanel::Detail
                    };
                    self.chat_tab = ChatTab::Notes;
                }
                count += 1;
            }
            if count == 0 {
                ui.add_space(90.0);
                ui.vertical_centered(|ui| {
                    mark(ui, 40.0, Color32::from_gray(135));
                    ui.add_space(18.0);
                    ui.label(
                        RichText::new(if query.is_empty() {
                            "Your conversations start here"
                        } else {
                            "No matching sessions"
                        })
                        .size(17.0),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(if query.is_empty() {
                            "Start a session to keep your transcript and notes together."
                        } else {
                            "Try a title, a date (YYYY-MM-DD), a note, or a transcript phrase."
                        })
                        .size(12.0)
                        .color(MUTED),
                    );
                });
            }
        });
        self.history_footer(ui, rect.bottom() - 48.0);
    }

    fn history_calendar(&mut self, ui: &mut Ui) {
        let preview = self.calendar_preview();
        if self.fixture.is_some() && !preview {
            return;
        }
        let calendar = self.runtime.calendar.clone();
        let Some(page) = calendar.upcoming else {
            return;
        };
        let ready = self.calendar_ready();
        ui.horizontal(|ui| {
            ui.label(RichText::new("UPCOMING").size(11.0).color(MUTED));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if matches!(calendar.operation, Some(CalendarOperation::Refreshing)) {
                    if calendar_button(ui, "Cancel refresh", ready, false) {
                        self.send(Command::Calendar(CalendarCommand::Cancel));
                    }
                    ui.spinner();
                } else if matches!(calendar.connection, CalendarConnection::ReconnectRequired) {
                    if calendar_button(ui, "Calendar settings", ready, false) {
                        self.visit_settings_page(Page::Calendar);
                        self.show_settings(ui.ctx());
                    }
                } else if calendar_button(
                    ui,
                    "Refresh calendar",
                    ready
                        && matches!(calendar.connection, CalendarConnection::Connected)
                        && calendar.operation.is_none(),
                    false,
                ) {
                    self.send(Command::Calendar(CalendarCommand::Refresh));
                }
            });
        });
        if preview {
            ui.label(
                RichText::new("Sample calendar · requests disabled")
                    .size(11.0)
                    .color(TEAL),
            );
        }
        if calendar.stale {
            let refreshed = calendar
                .refreshed_at
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok());
            let text = if let Some(millis) = refreshed {
                format!(
                    "Showing the last refresh from {} {} UTC",
                    utc_date(millis),
                    clock_label(millis)
                )
            } else {
                "Showing the last successful refresh".to_owned()
            };
            ui.label(RichText::new(text).size(11.0).color(MUTED));
        }
        if let Some(error) = &calendar.error {
            ui.label(
                RichText::new(error.to_string())
                    .size(12.0)
                    .color(Color32::from_rgb(255, 197, 187)),
            );
        }
        if page.events.is_empty() {
            ui.label(
                RichText::new("No upcoming events in the next 30 days")
                    .size(12.0)
                    .color(MUTED),
            );
        }
        const PREVIEW: usize = 3;
        let limit = if self.calendar_show_more {
            page.events.len()
        } else {
            PREVIEW
        };
        for event in page.events.iter().take(limit) {
            calendar_event_row(ui, event);
        }
        if page.events.len() > PREVIEW
            && calendar_button(
                ui,
                if self.calendar_show_more {
                    "Show fewer"
                } else {
                    "Show more"
                },
                true,
                false,
            )
        {
            self.calendar_show_more = !self.calendar_show_more;
        }
        if page.has_more && limit >= page.events.len() {
            ui.label(
                RichText::new("More events are available in Google Calendar.")
                    .size(11.0)
                    .color(MUTED),
            );
        }
        ui.add_space(14.0);
    }

    fn history_question_action(&mut self, ui: &mut Ui) {
        let saving_notes = matches!(
            self.after_notes,
            Some(Command::AskFromHistory { .. } | Command::AskSavedMeetings { .. })
        );
        let submitting = self
            .question_submission
            .as_ref()
            .is_some_and(|submission| submission.draft.is_none());
        let prompt = self.history_search.trim();
        let empty = prompt.is_empty();
        if empty && !saving_notes && !submitting {
            return;
        }
        let invalid_prompt = if empty {
            None
        } else {
            cueglint_core::validate_prompt(prompt).err()
        };
        let can_ask = !empty
            && invalid_prompt.is_none()
            && self.answer_ready()
            && self.after_notes.is_none()
            && !self.quitting
            && !self.backup_restore_blocks_controls();
        let (ask, stop) = ui
            .horizontal(|ui| {
                let ask = ui
                    .add_enabled_ui(can_ask, |ui| {
                        button(ui, "Ask the assistant", true, vec2(144.0, 32.0))
                    })
                    .inner
                    .on_disabled_hover_text(
                        "Wait for the current operation or check the answer model in Settings.",
                    );
                let stop = submitting
                    && self.generating()
                    && button(ui, "Stop response", false, vec2(110.0, 32.0)).clicked();
                let status = if saving_notes {
                    Some("Saving notes before asking…")
                } else if submitting {
                    match self
                        .runtime
                        .window_picker
                        .as_ref()
                        .map(|picker| picker.phase)
                    {
                        Some(PickerPhase::Choosing) => Some("Choose a window in the macOS picker."),
                        Some(PickerPhase::Capturing) => {
                            Some("Reading the selected window locally…")
                        }
                        _ => Some("Preparing your question…"),
                    }
                } else if self.runtime.screen_enabled {
                    Some(if self.runtime.screen_input.uses_display() {
                        "Includes current display text"
                    } else {
                        "Includes text from a window you choose"
                    })
                } else {
                    None
                };
                if let Some(status) = status {
                    ui.label(RichText::new(status).size(12.0).color(MUTED));
                }
                let provider = self.runtime.answer_provider.provider;
                if !provider.is_local() {
                    ui.label(
                        RichText::new(format!("Sends to {}", provider.label()))
                            .size(12.0)
                            .color(MUTED),
                    );
                }
                (ask.clicked(), stop)
            })
            .inner;
        ui.add_space(4.0);
        let local = self.runtime.answer_provider.provider.is_local();
        let saved = ui
            .add_enabled_ui(can_ask && local, |ui| {
                button(ui, "Ask saved meetings locally", false, vec2(192.0, 32.0))
            })
            .inner
            .on_hover_text("Use the local answer model with excerpts from saved notes, summaries, and transcripts.")
            .on_disabled_hover_text(if local {
                "Wait for the current operation or check the local answer model in Settings."
            } else {
                "Select Local in Settings to ask about saved meetings."
            })
            .clicked();
        if stop {
            self.send(Command::Cancel);
        } else if can_ask && (ask || saved) {
            let prompt = prompt.to_owned();
            let new_chat_mode = self.selected_mode.clone();
            self.after_saving_notes(if saved {
                Command::AskSavedMeetings {
                    prompt,
                    new_chat_mode,
                }
            } else {
                Command::AskFromHistory {
                    prompt,
                    new_chat_mode,
                }
            });
            if self.notes_dirty && !self.notes_saving {
                self.after_notes = None;
            }
        }
        let notice = invalid_prompt
            .map(|error| (error.to_string(), true))
            .or_else(|| self.notice.clone());
        if let Some((message, error)) = notice {
            ui.label(RichText::new(message).size(12.0).color(if error {
                Color32::from_rgb(255, 197, 187)
            } else {
                MUTED
            }));
        }
        ui.add_space(8.0);
    }

    fn history_footer(&mut self, ui: &mut Ui, y: f32) {
        let origin = pos2(ui.max_rect().left(), y);
        ui.painter().rect_filled(
            Rect::from_min_size(origin, vec2(690.0, 48.0)),
            CornerRadius {
                nw: 0,
                ne: 0,
                sw: 20,
                se: 20,
            },
            Color32::from_gray(17),
        );
        if icon_button_at(
            ui,
            Rect::from_min_size(origin + vec2(16.0, 8.0), vec2(32.0, 32.0)),
            Icon::Gear,
            "Open settings",
            Color32::TRANSPARENT,
        )
        .clicked()
        {
            self.show_settings(ui.ctx());
        }
        let mut footer = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_max(
            origin + vec2(485.0, 9.0),
            origin + vec2(676.0, 41.0),
        )));
        footer.horizontal(|ui| {
            let configured = self.state.as_ref().and_then(|state| {
                shortcuts::binding(&state.settings.shortcut_bindings, Action::NewChat)
            });
            if configured == Action::NewChat.default_binding() {
                keycap(
                    ui,
                    if cfg!(target_os = "macos") {
                        "⌘ R"
                    } else {
                        "Ctrl R"
                    },
                );
            }
            if button(ui, "+  New chat", false, vec2(115.0, 30.0)).clicked() {
                self.new_chat();
            }
        });
    }

    fn conversation(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        // Reference alpha and blur are unmeasured; these are Cueglint's translucency values.
        ui.painter().rect_filled(
            Rect::from_min_max(rect.min + vec2(0.0, 56.0), rect.max - vec2(0.0, 48.0)),
            0,
            Color32::from_rgba_unmultiplied(77, 78, 80, 210),
        );
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, vec2(690.0, 56.0)),
            CornerRadius {
                nw: 20,
                ne: 20,
                sw: 0,
                se: 0,
            },
            Color32::from_rgba_unmultiplied(119, 120, 122, 218),
        );
        if icon_button_at(
            ui,
            Rect::from_min_size(rect.min + vec2(8.0, 12.0), vec2(32.0, 32.0)),
            Icon::Back,
            "Back to history",
            Color32::TRANSPARENT,
        )
        .clicked()
        {
            self.overlay_panel = OverlayPanel::History;
        }
        self.question_input(ui, rect.top(), false);
        let body = Rect::from_min_max(rect.min + vec2(20.0, 74.0), rect.max - vec2(20.0, 60.0));
        let mut body_ui = ui.new_child(UiBuilder::new().max_rect(body));
        if let Some(picker) = self
            .runtime
            .window_picker
            .as_ref()
            .filter(|picker| picker.question)
        {
            let label = match picker.phase {
                PickerPhase::Choosing => Some("Choose a window in the macOS picker."),
                PickerPhase::Capturing => Some("Reading the selected window locally…"),
                PickerPhase::Retiring => None,
            };
            if let Some(label) = label {
                body_ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(label).size(13.0));
                });
                body_ui.add_space(12.0);
            }
        }
        let scroll = std::mem::take(&mut self.response_scroll);
        let session = self.selected();
        let range = self.reply_window.range();
        let mut navigation = None;
        let mut open_settings = false;
        let mut open_notes = false;
        if let Some(session) =
            session.filter(|session| session.requests.len() > crate::reply_window::LIMIT)
        {
            body_ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Replies {}–{} of {}", range.start + 1, range.end, session.requests.len())).size(11.0).color(MUTED));
                if ui.add_enabled(range.start > 0, egui::Button::new("Earlier replies").small()).clicked() { navigation = Some(ReplyNavigation::Earlier); }
                if ui.add_enabled(range.end < session.requests.len(), egui::Button::new("Later replies").small()).clicked() { navigation = Some(ReplyNavigation::Later); }
                if ui.add_enabled(!self.reply_window.latest(), egui::Button::new("Latest replies").small()).on_hover_text("Long conversations show up to 16 replies at a time. Earlier and Later keep every saved answer reachable.").clicked() { navigation = Some(ReplyNavigation::Latest); }
            });
        }
        egui::ScrollArea::vertical()
            .id_salt(("assistant-response", session.map(|session| session.id.as_str()), self.reply_window.anchor()))
            .stick_to_bottom(scroll == 0.0 && self.reply_window.latest())
            .show(&mut body_ui, |ui| {
                if scroll != 0.0 { ui.scroll_with_delta(vec2(0.0, scroll)); }
                if let Some(session) = session {
                    if session.history_chat {
                        let hint = if self.selected_chat_requires_local() {
                            "Select Local in Settings to continue this History chat."
                        } else {
                            "Local History chat"
                        };
                        ui.label(RichText::new(hint).size(12.0).color(MUTED));
                        ui.add_space(8.0);
                    }
                    // shortcut: variable-height long histories use an explicit reply window until measured virtualization is needed.
                    for reply in self.reply_window.replies() {
                        let request = &session.requests[reply.request];
                        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                            Frame::new()
                                .fill(Color32::from_white_alpha(24))
                                .corner_radius(18)
                                .inner_margin(Margin::symmetric(15, 11))
                                .show(ui, |ui| {
                                    ui.set_max_width(445.0);
                                    ui.add(
                                        egui::Label::new(RichText::new(&request.prompt).size(14.0))
                                            .wrap()
                                            .selectable(true),
                                    );
                                });
                        });
                        ui.add_space(18.0);
                        if let Some(context) = &request.context.past_meetings {
                            match context {
                                MeetingContext::Excerpts(excerpts) => {
                                    egui::CollapsingHeader::new("Saved meeting sources")
                                        .id_salt(("meeting-excerpts", &request.id))
                                        .show(ui, |ui| {
                                            for (index, excerpt) in excerpts.iter().enumerate() {
                                                ui.label(RichText::new(format!(
                                                    "[M{}] {} · {} · {}",
                                                    index + 1, excerpt.title,
                                                    utc_date(excerpt.created_at), excerpt.source.label()
                                                )).size(12.0));
                                                ui.add(egui::Label::new(RichText::new(&excerpt.text).size(12.0))
                                                    .wrap()
                                                    .selectable(true));
                                                ui.add_space(6.0);
                                            }
                                        });
                                }
                                MeetingContext::Withdrawn => {
                                    ui.label(RichText::new("A source changed or was deleted. Its excerpts and dependent answers were removed.")
                                        .size(12.0).color(MUTED));
                                    ui.add_space(24.0);
                                    continue;
                                }
                            }
                        }
                        if !request.provider.provider.is_local() {
                            ui.label(RichText::new(format!("{} · {}", request.provider.provider.label(), request.provider.model_id)).size(10.0).color(MUTED))
                                .on_hover_text(format!("Context destination: {}", request.provider.endpoint().unwrap_or_default()));
                        }
                        if let Some(answer) = reply.answer.and_then(|index| session.suggestions.get(index))
                        {
                            if !answer.sources.is_empty() {
                                ui.label(
                                    RichText::new("Viewed files")
                                        .size(12.0)
                                        .color(Color32::from_gray(203)),
                                );
                            }
                            self.answer_text(ui, session, request, &answer.text);
                            copy_response(ui, Id::new(("copy-response", &session.id, &request.id)), &answer.text);
                        } else if let Some(status) =
                            self.runtime.generation.as_ref().filter(|status| {
                                status.request_id == request.id
                                    && status.session_id == session.id
                                    && status.state == "generating"
                                    && self.answer_previews.allows_stream(status)
                            })
                        {
                            if status.text.is_empty() {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(RichText::new(if request.provider.provider.is_local() { "Thinking locally…" } else { "Waiting for selected provider…" }).size(13.0));
                                });
                            } else {
                                self.answer_text(ui, session, request, &status.text);
                                copy_response(ui, Id::new(("copy-response", &session.id, &request.id)), &status.text);
                            }
                        } else {
                            ui.label(
                                RichText::new("No response was saved for this question.")
                                    .size(12.0)
                                    .color(Color32::from_gray(205)),
                            );
                        }
                        ui.add_space(24.0);
                    }
                    if let Some(status) = self.runtime.generation.as_ref().filter(|status| {
                        status.session_id == session.id && status.state == "capturing screen" && self.reply_window.latest()
                    }) {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(RichText::new(&status.state).size(13.0));
                        });
                    }
                    if session.requests.is_empty() && !self.generating() {
                        ui.add_space(40.0);
                        ui.label(RichText::new("Ready for your next question").size(18.0));
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new(
                                "Your transcript, notes, and reference files are saved on this device. The selected answer provider receives context when you send a question.",
                            )
                            .size(12.0)
                            .color(Color32::from_gray(220)),
                        );
                        ui.add_space(25.0);
                    }
                    if self.runtime.answer_provider.provider.is_local() && !self
                        .runtime
                        .installed_profiles
                        .iter()
                        .any(|id| id == self.generation_profile())
                    {
                        ui.label(
                            RichText::new(
                                "Download or import the selected answer model in Settings to generate responses.",
                            )
                            .size(12.0)
                            .color(Color32::from_gray(222)),
                        );
                        if button(ui, "Set up local models", false, vec2(155.0, 30.0)).clicked() {
                            open_settings = true;
                        }
                        ui.add_space(15.0);
                    }
                    if button(ui, "Open transcript & notes", false, vec2(172.0, 30.0)).clicked() {
                        open_notes = true;
                    }
                } else {
                    ui.add_space(60.0);
                    ui.vertical_centered(|ui| {
                        mark(ui, 40.0, TEAL);
                        ui.add_space(20.0);
                        ui.label(RichText::new("Your context, close at hand").size(19.0));
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new("Ask a question using the selected answer provider.")
                                .size(12.0)
                                .color(Color32::from_gray(220)),
                        );
                    });
                }
            });
        if let Some(navigation) = navigation {
            if let Some(session) = self.selected() {
                self.reply_navigation = Some((session.id.clone(), navigation));
            }
            ui.ctx().request_repaint_of(ViewportId::ROOT);
            ui.ctx().request_repaint_of(overlay_id());
        }
        if open_settings {
            self.visit_settings_page(Page::General);
            self.show_settings(ui.ctx());
        }
        if open_notes {
            self.overlay_panel = OverlayPanel::Detail;
            self.chat_tab = ChatTab::Notes;
        }
        self.toolbar(ui, rect.bottom() - 48.0, false);
    }

    fn answer_text(
        &self,
        ui: &mut Ui,
        session: &Session,
        request: &GenerationRequest,
        source: &str,
    ) {
        let preview = self
            .answer_previews
            .get(&session.id, &request.id, request.context.revision);
        if let markdown::Preview::Formatted(document, text_revision) = preview {
            let output = markdown::show(
                ui,
                Id::new((
                    "assistant-answer",
                    &session.id,
                    &request.id,
                    request.context.revision,
                )),
                document,
                markdown::RenderOptions {
                    copy_source_button: false,
                    link_revision: text_revision,
                    ..Default::default()
                },
            );
            for action in output.actions {
                match action {
                    markdown::Action::Copy(text) => ui.ctx().copy_text(text.to_string()),
                    markdown::Action::Open(url) => {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(url.as_str()))
                    }
                }
            }
            return;
        }
        // Keep the current source visible while parsing; never substitute an older streamed revision.
        let (text, shortened) = markdown::source_preview(source);
        ui.add(
            egui::Label::new(RichText::new(text).size(14.0))
                .wrap()
                .selectable(true),
        );
        let notice = match preview {
            markdown::Preview::Pending => Some("Formatting response…".into()),
            markdown::Preview::Unformatted(error) => Some(format!(
                "Formatted preview unavailable: {error}. Showing source text."
            )),
            markdown::Preview::NotCached => {
                Some("Source text preview: the formatting cache limit was reached.".into())
            }
            markdown::Preview::Formatted(..) => None,
        };
        if let Some(notice) = notice {
            ui.label(RichText::new(notice).size(11.0).color(MUTED));
        }
        if shortened {
            ui.label(RichText::new("Preview shortened for a bounded view. Copy response includes the complete original text.").size(11.0).color(MUTED));
        }
    }

    fn detail(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        if self.acknowledgement_kinds.iter().any(|kind| {
            matches!(
                kind,
                Acknowledgement::Switch | Acknowledgement::NewChat | Acknowledgement::NewQuestion
            )
        }) {
            let mut loading = ui.new_child(UiBuilder::new().max_rect(rect.shrink(28.0)));
            loading.add_space(280.0);
            loading.vertical_centered(|ui| {
                ui.spinner();
                ui.label(RichText::new("Opening session…").size(13.0).color(MUTED));
            });
            return;
        }
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, vec2(690.0, 56.0)),
            CornerRadius {
                nw: 20,
                ne: 20,
                sw: 0,
                se: 0,
            },
            Color32::from_gray(15),
        );
        if icon_button_at(
            ui,
            Rect::from_min_size(rect.min + vec2(8.0, 8.0), vec2(40.0, 40.0)),
            Icon::Back,
            "Back to history",
            Color32::from_gray(28),
        )
        .clicked()
        {
            self.overlay_panel = OverlayPanel::History;
        }
        if self.history_search_field(ui).changed() {
            self.overlay_panel = OverlayPanel::History;
        }
        let session = self.selected().cloned();
        if let Some(session) = session {
            let mut heading = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_max(
                rect.min + vec2(20.0, 81.0),
                rect.min + vec2(670.0, 123.0),
            )));
            let name = heading.add(
                egui::TextEdit::singleline(&mut self.session_title)
                    .id(Id::new(("session-title", &session.id)))
                    .hint_text("Untitled session")
                    .font(FontId::new(
                        32.0,
                        egui::FontFamily::Name(DETAIL_TITLE_FONT.into()),
                    ))
                    .frame(Frame::NONE)
                    .desired_width(f32::INFINITY),
            );
            if name.lost_focus()
                && !self.session_title.trim().is_empty()
                && self.session_title != session.title
            {
                self.send(Command::Rename(
                    session.id.clone(),
                    self.session_title.trim().into(),
                ));
            }
            let mut metadata = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(
                rect.min + vec2(20.0, 130.0),
                vec2(650.0, 35.0),
            )));
            metadata.horizontal(|ui| {
                detail_chip(ui, &date_label(session.created_at));
                let speakers = session
                    .transcript
                    .iter()
                    .any(|segment| segment.speaker == Speaker::You)
                    as u8
                    + session
                        .transcript
                        .iter()
                        .any(|segment| segment.speaker == Speaker::Other)
                        as u8;
                let participants = if session
                    .transcript
                    .iter()
                    .any(|segment| segment.source != cueglint_core::TranscriptSource::Manual)
                {
                    "Audio transcript".into()
                } else {
                    format!("{speakers} speakers")
                };
                detail_chip(ui, &participants);
                if self.fixture.is_some() {
                    ui.with_layout(Layout::top_down(Align::Min), |ui| keycap(ui, "FIXTURE"));
                }
                if let Some(capture) = self
                    .runtime
                    .capture
                    .as_ref()
                    .filter(|capture| capture.session_id == session.id)
                {
                    ui.label(RichText::new(&capture.state).size(11.0).color(TEAL));
                }
            });
            let mut section_ui = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(
                rect.min + vec2(20.0, 185.0),
                vec2(650.0, 28.0),
            )));
            section_ui.horizontal(|ui| {
                ui.label(
                    RichText::new(if self.chat_tab == ChatTab::Transcript {
                        "TRANSCRIPT"
                    } else {
                        "NOTES"
                    })
                    .size(11.0)
                    .color(MUTED),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let text = if self.chat_tab == ChatTab::Transcript {
                        session
                            .transcript
                            .iter()
                            .map(|segment| format!("{}: {}", speaker_label(segment), segment.text))
                            .collect::<Vec<_>>()
                            .join("\n\n")
                    } else {
                        self.notes.clone()
                    };
                    if ui
                        .add_enabled(
                            !text.is_empty(),
                            egui::Button::new("Copy").min_size(vec2(60.0, 24.0)),
                        )
                        .clicked()
                    {
                        ui.ctx().copy_text(text);
                    }
                    if self.chat_tab == ChatTab::Notes {
                        ui.label(
                            RichText::new(if self.notes_failed {
                                "Save failed"
                            } else if self.notes_dirty {
                                "Saving…"
                            } else {
                                "Saved locally"
                            })
                            .size(11.0)
                            .color(MUTED),
                        );
                    }
                });
            });
            let body =
                Rect::from_min_max(rect.min + vec2(20.0, 226.0), rect.max - vec2(20.0, 67.0));
            let mut body_ui = ui.new_child(UiBuilder::new().max_rect(body));
            if self.chat_tab == ChatTab::Transcript {
                self.transcript_ui(&mut body_ui, &session);
            } else {
                self.notes_ui(&mut body_ui, &session);
            }
            let origin = pos2(rect.left(), rect.bottom() - 48.0);
            ui.painter().rect_filled(
                Rect::from_min_size(origin, vec2(690.0, 48.0)),
                CornerRadius {
                    nw: 0,
                    ne: 0,
                    sw: 20,
                    se: 20,
                },
                Color32::from_gray(17),
            );
            if icon_button_at(
                ui,
                Rect::from_min_size(origin + vec2(12.0, 8.0), vec2(32.0, 32.0)),
                Icon::Back,
                "Back to history",
                Color32::TRANSPARENT,
            )
            .clicked()
            {
                self.overlay_panel = OverlayPanel::History;
            }
            let mut conversation = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(
                origin + vec2(64.0, 8.0),
                vec2(172.0, 32.0),
            )));
            if button(
                &mut conversation,
                "Open conversation",
                false,
                vec2(172.0, 32.0),
            )
            .clicked()
            {
                self.overlay_panel = OverlayPanel::Chat;
            }
            let mut footer = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_max(
                origin + vec2(365.0, 8.0),
                origin + vec2(679.0, 42.0),
            )));
            footer.horizontal(|ui| {
                if button(
                    ui,
                    if self.chat_tab == ChatTab::Transcript {
                        "Show notes"
                    } else {
                        "Show transcript"
                    },
                    false,
                    vec2(124.0, 32.0),
                )
                .clicked()
                {
                    self.chat_tab = if self.chat_tab == ChatTab::Transcript {
                        ChatTab::Notes
                    } else {
                        ChatTab::Transcript
                    };
                }
                if session.status == SessionStatus::Recording {
                    if button(ui, "Pause", false, vec2(62.0, 32.0)).clicked() {
                        self.send(Command::Pause(session.id.clone()));
                    }
                    if button(ui, "End", true, vec2(70.0, 32.0)).clicked() {
                        self.after_saving_notes(Command::Stop(session.id.clone()));
                    }
                } else if !session.history_chat
                    && button(ui, "Resume session", true, vec2(136.0, 32.0)).clicked()
                {
                    self.after_saving_notes(Command::Resume(session.id.clone()));
                }
            });
        } else {
            let mut empty = ui.new_child(UiBuilder::new().max_rect(rect.shrink(40.0)));
            empty.add_space(260.0);
            empty.vertical_centered(|ui| {
                ui.label(RichText::new("No session selected").size(16.0).color(MUTED));
                if button(ui, "Open history", false, vec2(130.0, 32.0)).clicked() {
                    self.overlay_panel = OverlayPanel::History;
                }
            });
        }
    }

    fn transcript_ui(&mut self, ui: &mut Ui, session: &Session) {
        let active = session.status != SessionStatus::Stopped;
        let available = ui.available_height();
        egui::ScrollArea::vertical()
            .max_height(if active {
                (available - 130.0).max(80.0)
            } else {
                available
            })
            .show(ui, |ui| {
                if session.transcript.is_empty() {
                    ui.add_space(135.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new("No transcript has been recorded yet.")
                                .size(16.0)
                                .color(MUTED),
                        );
                    });
                }
                for segment in &session.transcript {
                    let width = ui.available_width();
                    let time = Duration::from_nanos(segment.session_offset_ns(session.created_at))
                        .as_secs();
                    ui.horizontal_top(|ui| {
                        let gutter = 64.0;
                        ui.vertical(|ui| {
                            ui.set_width(gutter - ui.spacing().item_spacing.x);
                            ui.label(
                                RichText::new(format!("{:02}:{:02}", time / 60, time % 60))
                                    .size(11.0)
                                    .color(MUTED),
                            );
                        });
                        ui.vertical(|ui| {
                            ui.set_width((width - gutter).max(0.0));
                            ui.label(
                                RichText::new(speaker_label(segment).to_uppercase())
                                    .size(10.0)
                                    .color(MUTED),
                            );
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&segment.text)
                                        .size(16.0)
                                        .line_height(Some(28.0))
                                        .color(if segment.finalized { TEXT } else { MUTED }),
                                )
                                .wrap()
                                .selectable(true),
                            );
                        });
                    });
                    ui.add_space(22.0);
                }
            });
        if active {
            ui.add_space(12.0);
            ui.add_sized(
                vec2(ui.available_width(), 58.0),
                egui::TextEdit::multiline(&mut self.transcript)
                    .hint_text("Add a manual transcript segment…")
                    .margin(10.0),
            );
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.speaker_you, true, "You");
                ui.selectable_value(&mut self.speaker_you, false, "Other speaker");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if button(ui, "Add segment", true, vec2(112.0, 28.0)).clicked()
                        && !self.transcript.trim().is_empty()
                    {
                        if self.send(Command::Transcript(
                            session.id.clone(),
                            self.transcript.trim().into(),
                            if self.speaker_you {
                                Speaker::You
                            } else {
                                Speaker::Other
                            },
                        )) {
                            self.transcript.clear();
                        }
                    }
                });
            });
        }
    }

    fn notes_ui(&mut self, ui: &mut Ui, session: &Session) {
        let height = (ui.available_height() - 95.0).max(100.0);
        let switching = self.acknowledgement_kinds.iter().any(|kind| {
            matches!(
                kind,
                Acknowledgement::Switch | Acknowledgement::NewChat | Acknowledgement::NewQuestion
            )
        });
        let editable = !switching && !self.highlights_appending();
        let mut layouter = notes_layouter;
        if ui
            .add_sized(
                vec2(ui.available_width(), height),
                egui::TextEdit::multiline(&mut self.notes)
                    .id(Id::new("session-notes"))
                    .interactive(editable)
                    .font(FontId::proportional(16.0))
                    .layouter(&mut layouter)
                    .hint_text("Write down decisions, next steps, and things worth remembering…")
                    .frame(Frame::NONE)
                    .margin(0.0),
            )
            .changed()
        {
            self.notes_dirty = true;
            self.notes_failed = false;
            self.notes_edited = Instant::now();
        }
        if self.notes_failed && button(ui, "Retry saving notes", false, vec2(135.0, 28.0)).clicked()
        {
            self.save_notes();
        }
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if ui.small_button("Highlights").clicked() {
                self.highlights.open = true;
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.export_path)
                    .id(Id::new("meeting-export-path"))
                    .hint_text("Export path (new .md or .json file)")
                    .desired_width((ui.available_width() - 134.0).max(100.0)),
            );
            let path = Path::new(self.export_path.trim());
            let format = if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
            {
                ExportFormat::Json
            } else {
                ExportFormat::Markdown
            };
            if ui
                .add_enabled_ui(!path.as_os_str().is_empty(), |ui| {
                    button(ui, &format!("Export {format}"), false, vec2(118.0, 28.0))
                })
                .inner
                .clicked()
            {
                self.after_saving_notes(Command::Export(
                    session.id.clone(),
                    path.to_owned(),
                    format,
                ));
            }
        });
        ui.horizontal(|ui| {
            if session.status != SessionStatus::Stopped
                && ui
                    .small_button(if session.muted {
                        "Unmute audio"
                    } else {
                        "Mute audio"
                    })
                    .clicked()
            {
                self.send(Command::Mute(session.id.clone(), !session.muted));
            }
            if session.status == SessionStatus::Stopped {
                if ui
                    .small_button(if session.saved_at.is_some() {
                        "Saved locally"
                    } else {
                        "Save session"
                    })
                    .clicked()
                {
                    self.send(Command::Save(session.id.clone()));
                }
                if ui
                    .small_button(if self.delete_session.as_ref() == Some(&session.id) {
                        "Confirm delete"
                    } else {
                        "Delete session"
                    })
                    .clicked()
                {
                    if self.delete_session.as_ref() == Some(&session.id) {
                        self.after_saving_notes(Command::Delete(session.id.clone()));
                        self.delete_session = None;
                        self.overlay_panel = OverlayPanel::History;
                    } else {
                        self.delete_session = Some(session.id.clone());
                    }
                }
            }
            if let Some((message, error)) = &self.notice {
                let color = if *error {
                    Color32::from_rgb(255, 197, 187)
                } else {
                    MUTED
                };
                ui.add(egui::Label::new(RichText::new(message).size(12.0).color(color)).truncate())
                    .on_hover_text(message);
            }
        });
        if !session.history_chat
            && session.saved_at.is_some()
            && self.delete_session.as_ref() == Some(&session.id)
        {
            ui.label(
                RichText::new("Deleting this meeting also removes dependent History chat answers.")
                    .size(12.0),
            );
        }
        if ui.is_enabled() {
            self.highlights_ui(ui.ctx(), session);
        }
    }
}

impl Drop for Cueglint {
    fn drop(&mut self) {
        if let Some(import) = &self.knowledge_import {
            import.cancel();
        }
        self.provider_key.zeroize();
    }
}

impl eframe::App for Cueglint {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.poll(ctx);
        self.poll_window_picker(ctx);
        if self.recording_shortcut.is_some()
            && (!self.settings_visible
                || ctx.input(|input| input.viewport().focused == Some(false)))
        {
            self.recording_shortcut = None;
        }
        if let Some(state) = &self.state {
            self.shortcuts.reconcile(
                &state.settings.shortcut_bindings,
                self.recording_shortcut.is_some(),
                self.retry_shortcut.take(),
            );
        }
        while let Some(action) = self.shortcuts.next() {
            self.dispatch_shortcut(action, ctx);
        }
        let dropped = self.shortcuts.dropped();
        if dropped > 0 {
            self.notice = Some((
                format!(
                    "Skipped {dropped} shortcut events while the bounded input queue was full. Press the combination again."
                ),
                true,
            ));
        }
        self.move_window(ctx);
        let reminder_changed = self.reconcile_reminders(self.calendar_reminder_now(), ctx);
        if self.focus_assistant
            || (reminder_changed && self.overlay_open)
            || ctx.input(|input| input.viewport().close_requested())
        {
            frame.request_ui_pass();
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if ctx.viewport_id() == ViewportId::ROOT {
            self.trace_screenshot(ScreenshotStage::RootUi, &ctx);
        }
        if ctx.input(|input| input.viewport().close_requested()) {
            if self.backup_restore_blocks_controls() {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            } else if !self.quitting && self.fixture.is_none() {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                self.show_assistant();
            } else {
                self.request_quit(&ctx);
            }
        }
        self.keyboard(&ctx);
        if !self.backup_restore_blocks_controls()
            && let Some(path) = ctx.input(|i| {
                i.raw
                    .dropped_files
                    .first()
                    .and_then(|file| file.path.clone())
            })
        {
            if let Some(import) = &self.knowledge_import {
                import.cancel();
            }
            self.import_path = path.to_string_lossy().into_owned();
            self.visit_settings_page(Page::Knowledge);
        }
        if self
            .fixture
            .as_deref()
            .and_then(crate::fixture_view)
            .is_some_and(|fixture| fixture != "dashboard")
        {
            self.assistant_surface(ui);
            return;
        }
        self.main_window(ui);
        self.overlay(&ctx);
    }

    fn on_exit(&mut self) {
        self.answer_previews.shutdown();
        self.shortcuts.shutdown();
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if self.fixture.is_none() {
            reminder_ui::save(&self.calendar_reminders, storage);
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn persist_egui_memory(&self) -> bool {
        self.fixture.is_none()
    }
}

fn configure(ctx: &egui::Context, native: bool) -> LegacyFonts {
    let mut style = egui::Style::default();
    style.visuals = egui::Visuals::dark();
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = CARD;
    style.visuals.extreme_bg_color = Color32::from_rgb(40, 37, 38);
    style.visuals.faint_bg_color = CARD;
    style.visuals.selection.bg_fill = BLUE.gamma_multiply(0.6);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(69, 66, 67);
    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(69, 66, 67);
    style.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
    style.visuals.widgets.inactive.corner_radius = CornerRadius::same(7);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(83, 79, 81);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(83, 79, 81);
    style.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
    style.visuals.widgets.hovered.corner_radius = CornerRadius::same(7);
    style.visuals.widgets.active.bg_fill = BLUE;
    style.visuals.widgets.active.corner_radius = CornerRadius::same(7);
    style.spacing.item_spacing = vec2(8.0, 6.0);
    style.spacing.button_padding = vec2(10.0, 5.0);
    style
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(12.0));
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_style_of(egui::Theme::Dark, style);
    LegacyFonts::start(ctx, native)
}

fn section(ui: &mut Ui, label: &str) {
    ui.label(RichText::new(label).size(12.0).color(MUTED));
    ui.add_space(4.0);
}

fn copy_response(ui: &mut Ui, id: Id, source: &str) {
    ui.scope_builder(UiBuilder::new().id(id), |ui| {
        if ui.small_button("Copy response").on_hover_text("Copy the complete original response. Selection covers visible text; scrolling an endpoint out of view or streaming changes can reset it.").clicked() {
            ui.ctx().copy_text(source.to_owned());
        }
    });
}

fn card<R>(ui: &mut Ui, content: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(CARD)
        .corner_radius(12)
        .inner_margin(Margin::symmetric(14, 0))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            content(ui)
        })
        .inner
}

fn rule(ui: &mut Ui) {
    let x = ui.available_rect_before_wrap().x_range();
    ui.painter()
        .hline(x, ui.cursor().top(), Stroke::new(1.0, LINE));
}

fn row(ui: &mut Ui, title: &str, subtitle: &str, trailing: impl FnOnce(&mut Ui)) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::hover());
    let mut text = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_max(
        rect.min + vec2(0.0, 14.0),
        pos2(rect.right() - 130.0, rect.bottom() - 8.0),
    )));
    text.label(RichText::new(title).size(13.0));
    text.label(RichText::new(subtitle).size(11.0).color(MUTED));
    let mut right = ui.new_child(
        UiBuilder::new()
            .max_rect(Rect::from_min_max(
                pos2(rect.right() - 126.0, rect.top() + 18.0),
                rect.max - vec2(0.0, 14.0),
            ))
            .layout(Layout::right_to_left(Align::Center)),
    );
    trailing(&mut right);
}

fn settings_heading(ui: &mut Ui, title: &str) {
    let mut rect = ui.available_rect_before_wrap();
    rect.min.x = (rect.min.x + 93.0).min(rect.max.x);
    ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
        ui.label(RichText::new(title).size(14.0).strong());
    });
}

fn hero(ui: &mut Ui, glyph: Icon, title: &str, subtitle: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 184.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 18, CARD);
    let tile = Rect::from_center_size(pos2(rect.center().x, rect.top() + 52.0), vec2(56.0, 56.0));
    painter.rect_filled(tile, 13, Color32::from_rgb(76, 72, 73));
    draw_icon(painter, tile.center(), glyph, 34.0, TEXT);
    painter.text(
        pos2(rect.center().x, rect.top() + 107.0),
        egui::Align2::CENTER_CENTER,
        title,
        FontId::proportional(22.0),
        TEXT,
    );
    painter.text(
        pos2(rect.center().x, rect.top() + 143.0),
        egui::Align2::CENTER_CENTER,
        subtitle,
        FontId::proportional(12.0),
        MUTED,
    );
}

fn button(ui: &mut Ui, title: &str, primary: bool, size: Vec2) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(title).size(12.0).color(TEXT))
            .fill(if primary { BLUE } else { SELECTED })
            .corner_radius(7)
            .min_size(size),
    )
}

fn calendar_button(ui: &mut Ui, title: &str, enabled: bool, primary: bool) -> bool {
    ui.add_enabled_ui(enabled, |ui| button(ui, title, primary, vec2(0.0, 28.0)))
        .inner
        .clicked()
}

fn shortcut_token_label(token: &str) -> &str {
    match token {
        "CommandOrControl" => {
            if cfg!(target_os = "macos") {
                "⌘"
            } else {
                "Ctrl"
            }
        }
        "Control" => {
            if cfg!(target_os = "macos") {
                "⌃"
            } else {
                "Ctrl"
            }
        }
        "Meta" => {
            if cfg!(target_os = "macos") {
                "⌘"
            } else {
                "Win"
            }
        }
        "Alt" => {
            if cfg!(target_os = "macos") {
                "⌥"
            } else {
                "Alt"
            }
        }
        "Shift" => {
            if cfg!(target_os = "macos") {
                "⇧"
            } else {
                "Shift"
            }
        }
        "Backslash" => "\\",
        "Comma" => ",",
        "Equals" => "=",
        "Minus" => "-",
        "Period" => ".",
        "Semicolon" => ";",
        "Slash" => "/",
        "OpenBracket" => "[",
        "CloseBracket" => "]",
        "Backtick" => "`",
        "Quote" => "'",
        token => token,
    }
}

fn keycap(ui: &mut Ui, key: &str) {
    Frame::new()
        .fill(Color32::from_gray(61))
        .corner_radius(5)
        .inner_margin(Margin::symmetric(7, 4))
        .show(ui, |ui| {
            ui.label(RichText::new(key).size(11.0).color(TEXT));
        });
}

fn detail_chip(ui: &mut Ui, text: &str) -> egui::Response {
    Frame::new()
        .fill(DETAIL_CHIP_FILL)
        .corner_radius(16)
        .inner_margin(Margin::symmetric(7, 0))
        .show(ui, |ui| {
            ui.set_height(32.0);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.label(RichText::new(text).size(11.0).color(TEXT));
            });
        })
        .response
}

fn notes_layouter(
    ui: &Ui,
    text: &dyn egui::TextBuffer,
    wrap_width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_format(
        text.as_str().to_owned(),
        egui::TextFormat {
            font_id: FontId::proportional(16.0),
            color: ui.visuals().text_color(),
            line_height: Some(28.0),
            ..Default::default()
        },
    );
    job.wrap.max_width = wrap_width;
    job.keep_trailing_whitespace = true;
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

fn nav_row(ui: &mut Ui, glyph: Icon, title: &str, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, title)
    });
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            9,
            if selected {
                SELECTED
            } else {
                Color32::from_white_alpha(8)
            },
        );
    }
    let tile = Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), vec2(23.0, 23.0));
    let color = match glyph {
        Icon::Gear => Color32::from_rgb(115, 113, 118),
        Icon::Calendar => Color32::from_rgb(198, 82, 81),
        Icon::Grid => Color32::from_rgb(145, 98, 192),
        Icon::File => Color32::from_rgb(70, 151, 143),
        Icon::Keyboard => Color32::from_rgb(120, 120, 126),
        Icon::Lock => Color32::from_rgb(85, 141, 195),
        Icon::Card => Color32::from_rgb(81, 158, 110),
        _ => Color32::from_rgb(107, 112, 119),
    };
    ui.painter().rect_filled(tile, 4, color);
    draw_icon(ui.painter(), tile.center(), glyph, 13.0, TEXT);
    ui.painter().text(
        pos2(rect.left() + 36.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        title,
        FontId::proportional(12.0),
        TEXT,
    );
    response
}

fn history_row(ui: &mut Ui, session: &Session) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &session.title));
    if response.hovered() {
        ui.painter().rect_filled(rect, 10, Color32::from_gray(28));
    }
    let tile = Rect::from_min_size(rect.min + vec2(12.0, 12.0), vec2(32.0, 32.0));
    ui.painter().rect_filled(tile, 8, Color32::from_gray(32));
    draw_icon(
        ui.painter(),
        tile.center(),
        Icon::File,
        17.0,
        Color32::from_gray(190),
    );
    let title = Rect::from_min_max(
        rect.min + vec2(64.0, 18.0),
        pos2(rect.right() - 154.0, rect.bottom() - 12.0),
    );
    let mut text = ui.new_child(UiBuilder::new().max_rect(title));
    // The row owns activation; selecting its title would intercept pointer clicks.
    text.add(
        egui::Label::new(RichText::new(&session.title).size(14.0))
            .truncate()
            .selectable(false),
    );
    // Activity timestamps do not measure recording duration.
    let status = match session.status {
        SessionStatus::Recording => "Active",
        SessionStatus::Paused => "Paused",
        SessionStatus::Stopped => "Stopped",
    };
    let pill = Rect::from_center_size(
        pos2(rect.right() - 100.0, rect.center().y),
        vec2(62.0, 26.0),
    );
    ui.painter().rect_filled(pill, 13, Color32::from_gray(28));
    ui.painter().text(
        pill.center(),
        egui::Align2::CENTER_CENTER,
        status,
        FontId::proportional(11.0),
        MUTED,
    );
    ui.painter().text(
        pos2(rect.right() - 14.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        clock_label(session.created_at),
        FontId::proportional(12.0),
        MUTED,
    );
    response
}

fn calendar_event_title(event: &CalendarEvent) -> String {
    let title: String = event
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or("Untitled event")
        .chars()
        .take(240)
        .map(|character| {
            if character.is_control() || character.is_whitespace() {
                ' '
            } else {
                character
            }
        })
        .collect();
    if title.trim().is_empty() {
        "Untitled event".into()
    } else {
        title
    }
}

fn calendar_event_row(ui: &mut Ui, event: &CalendarEvent) {
    let (date, time) = calendar_event_labels(&event.time);
    let title = calendar_event_title(event);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
    let tile = Rect::from_min_size(rect.min + vec2(12.0, 8.0), vec2(32.0, 32.0));
    ui.painter().rect_filled(tile, 8, Color32::from_gray(32));
    ui.painter().text(
        tile.center(),
        egui::Align2::CENTER_CENTER,
        date.rsplit('-').next().unwrap_or(&date),
        FontId::proportional(14.0),
        TEAL,
    );
    let mut content = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_max(
        rect.min + vec2(64.0, 4.0),
        rect.max - vec2(12.0, 4.0),
    )));
    content.spacing_mut().item_spacing.y = 2.0;
    content.add(
        egui::Label::new(RichText::new(title).size(14.0))
            .truncate()
            .selectable(false),
    );
    content.label(RichText::new(time).size(11.0).color(MUTED));
}

fn calendar_event_labels(time: &EventTime) -> (String, String) {
    match time {
        EventTime::AllDay {
            starts_on,
            ends_before,
        } => (
            starts_on.clone(),
            format!("All day · {starts_on} until {ends_before} (exclusive)"),
        ),
        EventTime::Timed { starts_at, ends_at } => {
            let (date, time, zone) = calendar_clock(starts_at);
            let label = if let Some(ends_at) = ends_at {
                let (end_date, end_time, end_zone) = calendar_clock(ends_at);
                if end_date == date && end_zone == zone {
                    format!("{date} · {time}–{end_time} {zone}")
                } else {
                    format!("{date} {time} {zone} – {end_date} {end_time} {end_zone}")
                }
            } else {
                format!("{date} · {time} {zone}")
            };
            (date, label)
        }
    }
}

fn calendar_clock(endpoint: &CalendarDateTime) -> (String, String, String) {
    let offset = endpoint.offset_minutes.unwrap_or(0);
    let millis = endpoint
        .utc
        .unix_seconds
        .saturating_add(i64::from(offset) * 60)
        .saturating_mul(1000);
    let zone = if offset == 0 {
        "UTC".to_owned()
    } else {
        let sign = if offset < 0 { '-' } else { '+' };
        let minutes = offset.unsigned_abs();
        format!("UTC{sign}{:02}:{:02}", minutes / 60, minutes % 60)
    };
    (utc_date(millis), clock_label(millis), zone)
}

fn speaker_label(segment: &cueglint_core::TranscriptSegment) -> &'static str {
    use cueglint_core::TranscriptSource;
    match segment.source {
        TranscriptSource::Microphone => "Microphone",
        TranscriptSource::SystemAudio => "System audio",
        TranscriptSource::Manual => {
            if segment.speaker == Speaker::You {
                "You"
            } else if segment.speaker == Speaker::Other {
                "Other speaker"
            } else {
                "Unknown speaker"
            }
        }
    }
}

fn clock_label(millis: i64) -> String {
    let minutes = millis.div_euclid(60_000).rem_euclid(1440);
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

fn date_label(millis: i64) -> String {
    format!("{} · UTC", utc_date(millis))
}

#[derive(Clone, Copy)]
enum Icon {
    Gear,
    Calendar,
    Grid,
    File,
    Keyboard,
    Lock,
    Card,
    Info,
    Glint,
    Return,
    Stop,
    Back,
    Forward,
    Screen,
    Eye,
    Wave,
    Chevron,
    Search,
}

fn icon(ui: &mut Ui, glyph: Icon, size: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    draw_icon(ui.painter(), rect.center(), glyph, size * 0.8, color);
}

fn mark(ui: &mut Ui, size: f32, color: Color32) {
    crate::brand::mark(ui, size, color);
}

fn icon_button_at(
    ui: &mut Ui,
    rect: Rect,
    glyph: Icon,
    label: &str,
    fill: Color32,
) -> egui::Response {
    let response = ui.interact(
        rect,
        ui.id()
            .with((label, rect.min.x.to_bits(), rect.min.y.to_bits())),
        Sense::click(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, response.enabled(), label)
    });
    ui.painter().rect_filled(
        rect,
        if rect.width() >= 40.0 { 20 } else { 8 },
        if response.hovered() {
            Color32::from_white_alpha(30)
        } else {
            fill
        },
    );
    draw_icon(
        ui.painter(),
        rect.center(),
        glyph,
        19.0,
        Color32::from_gray(228),
    );
    response.on_hover_text(label)
}

fn disabled_icon_at(ui: &mut Ui, rect: Rect, glyph: Icon, label: &str) {
    let response = ui.interact(rect, Id::new(label), Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, false, label));
    draw_icon(
        ui.painter(),
        rect.center(),
        glyph,
        19.0,
        Color32::from_gray(155),
    );
    response.on_hover_text(label);
}

fn draw_icon(painter: &egui::Painter, center: Pos2, glyph: Icon, size: f32, color: Color32) {
    let half = size * 0.5;
    let p = |x: f32, y: f32| center + vec2(x * half, y * half);
    let stroke = Stroke::new((size / 14.0).clamp(1.0, 2.0), color);
    let line = |a, b| {
        painter.line_segment([a, b], stroke);
    };
    let outline = |a: Pos2, b: Pos2, radius| {
        painter.rect_stroke(Rect::from_min_max(a, b), radius, stroke, StrokeKind::Inside);
    };
    match glyph {
        Icon::Gear => {
            painter.circle_stroke(center, half * 0.65, stroke);
            painter.circle_stroke(center, half * 0.25, stroke);
            for index in 0..8 {
                let angle = index as f32 * std::f32::consts::TAU / 8.0;
                let direction = vec2(angle.cos(), angle.sin());
                line(
                    center + direction * half * 0.63,
                    center + direction * half * 0.95,
                );
            }
        }
        Icon::Calendar => {
            outline(p(-0.82, -0.7), p(0.82, 0.85), 2);
            line(p(-0.8, -0.23), p(0.8, -0.23));
            line(p(-0.4, -1.0), p(-0.4, -0.5));
            line(p(0.4, -1.0), p(0.4, -0.5));
            line(p(-0.38, 0.2), p(-0.04, 0.2));
            line(p(0.27, 0.2), p(0.55, 0.2));
        }
        Icon::Grid => {
            for x in [-0.85, 0.15] {
                for y in [-0.85, 0.15] {
                    outline(p(x, y), p(x + 0.65, y + 0.65), 1);
                }
            }
        }
        Icon::File => {
            outline(p(-0.65, -0.9), p(0.65, 0.9), 2);
            line(p(-0.3, -0.3), p(0.3, -0.3));
            line(p(-0.3, 0.1), p(0.3, 0.1));
            line(p(-0.3, 0.5), p(0.12, 0.5));
        }
        Icon::Keyboard => {
            outline(p(-1.0, -0.65), p(1.0, 0.65), 2);
            for x in [-0.6, -0.2, 0.2, 0.6] {
                painter.circle_filled(p(x, -0.25), half * 0.06, color);
            }
            line(p(-0.5, 0.25), p(0.5, 0.25));
        }
        Icon::Lock => {
            outline(p(-0.7, -0.1), p(0.7, 0.9), 2);
            painter.add(egui::Shape::line(
                vec![
                    p(-0.45, -0.1),
                    p(-0.45, -0.6),
                    p(-0.25, -0.85),
                    p(0.25, -0.85),
                    p(0.45, -0.6),
                    p(0.45, -0.1),
                ],
                stroke,
            ));
            painter.circle_filled(p(0.0, 0.37), half * 0.12, color);
        }
        Icon::Card => {
            outline(p(-1.0, -0.65), p(1.0, 0.65), 2);
            line(p(-0.9, -0.2), p(0.9, -0.2));
            line(p(-0.6, 0.3), p(-0.2, 0.3));
        }
        Icon::Info => {
            painter.circle_stroke(center, half * 0.9, stroke);
            painter.circle_filled(p(0.0, -0.42), half * 0.1, color);
            line(p(0.0, -0.05), p(0.0, 0.5));
        }
        Icon::Glint => {
            crate::brand::paint(painter, center, size, color);
        }
        Icon::Stop => {
            painter.rect_filled(
                Rect::from_center_size(center, vec2(size * 0.6, size * 0.6)),
                2,
                color,
            );
        }
        Icon::Return => {
            line(p(0.7, -0.7), p(0.7, 0.1));
            line(p(0.7, 0.1), p(-0.7, 0.1));
            line(p(-0.7, 0.1), p(-0.15, -0.45));
            line(p(-0.7, 0.1), p(-0.15, 0.65));
        }
        Icon::Back | Icon::Forward => {
            let direction: f32 = if matches!(glyph, Icon::Back) {
                1.0
            } else {
                -1.0
            };
            let p = |x, y| p(direction * x, y);
            line(p(0.6, 0.0), p(-0.65, 0.0));
            line(p(-0.65, 0.0), p(-0.1, -0.55));
            line(p(-0.65, 0.0), p(-0.1, 0.55));
        }
        Icon::Screen => {
            outline(p(-0.9, -0.7), p(0.9, 0.45), 2);
            line(p(0.0, 0.45), p(0.0, 0.85));
            line(p(-0.45, 0.85), p(0.45, 0.85));
        }
        Icon::Eye => {
            painter.add(egui::Shape::line(
                vec![
                    p(-0.95, 0.0),
                    p(-0.45, -0.5),
                    p(0.0, -0.65),
                    p(0.45, -0.5),
                    p(0.95, 0.0),
                    p(0.45, 0.5),
                    p(0.0, 0.65),
                    p(-0.45, 0.5),
                    p(-0.95, 0.0),
                ],
                stroke,
            ));
            painter.circle_stroke(center, half * 0.25, stroke);
        }
        Icon::Wave => {
            for (x, height) in [
                (-0.8, 0.25),
                (-0.4, 0.7),
                (0.0, 1.0),
                (0.4, 0.6),
                (0.8, 0.3),
            ] {
                line(p(x, -height), p(x, height));
            }
        }
        Icon::Chevron => {
            line(p(-0.55, -0.2), p(0.0, 0.35));
            line(p(0.0, 0.35), p(0.55, -0.2));
        }
        Icon::Search => {
            painter.circle_stroke(p(-0.15, -0.15), half * 0.6, stroke);
            line(p(0.3, 0.3), p(0.85, 0.85));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_history_dates_handle_epoch_leap_day_and_negative_time() {
        assert_eq!(date_label(0), "1970-01-01 · UTC");
        assert_eq!(date_label(-1), "1969-12-31 · UTC");
        assert_eq!(date_label(1_709_164_800_000), "2024-02-29 · UTC");
        assert_eq!(clock_label(-1), "23:59");
    }

    #[test]
    fn headless_native_surfaces_render_without_a_window() {
        let context = egui::Context::default();
        let _fonts = configure(&context, false);
        let output = context.run_ui(egui::RawInput::default(), |ui| {
            Frame::new().show(ui, |ui| {
                hero(ui, Icon::Gear, "General", "Local workspace");
                nav_row(ui, Icon::Grid, "Modes", true);
            });
        });
        assert!(!output.shapes.is_empty());
    }

    fn calendar_frame(
        app: &mut Cueglint,
        context: &egui::Context,
        settings: bool,
        events: Vec<egui::Event>,
    ) -> egui::accesskit::TreeUpdate {
        let reminder = app.calendar_reminder_preview();
        let [width, height] = if reminder {
            crate::calendar_fixture::REMINDER_SIZE
        } else {
            [860.0, 760.0]
        };
        context
            .run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, height))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    if reminder {
                        app.assistant_surface(ui);
                    } else if settings {
                        app.main_window(ui);
                    } else {
                        app.history(ui);
                    }
                },
            )
            .platform_output
            .accesskit_update
            .unwrap()
    }

    fn click_calendar_button(tree: &egui::accesskit::TreeUpdate, label: &str) -> Vec<egui::Event> {
        let (id, _) = tree
            .nodes
            .iter()
            .find(|(_, node)| {
                node.role() == egui::accesskit::Role::Button
                    && node.label() == Some(label)
                    && !node.is_disabled()
            })
            .unwrap_or_else(|| panic!("missing enabled {label} button"));
        vec![egui::Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target_tree: egui::accesskit::TreeId::ROOT,
                target_node: *id,
                data: None,
            },
        )]
    }

    #[test]
    fn release_card_checks_only_on_click_and_can_retry_after_failure() {
        use crate::app::releases::ReleaseCheck;
        use cueglint_local::releases::PublishedRelease;
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        let context = egui::Context::default();
        context.enable_accesskit();
        let calls = Arc::new(AtomicUsize::new(0));
        let draw = |state: &mut ReleaseCheck, enabled, events| {
            context.run_ui(egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(600.0, 300.0))),
                events,
                ..Default::default()
            }, |ui| {
                let calls = calls.clone();
                state.show(ui, enabled, move || {
                    if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                        anyhow::bail!("Original fixture failure");
                    }
                    Ok(Some(PublishedRelease {
                        tag: "v1.0.0-dev.2".into(), prerelease: true,
                        url: "https://github.com/thematheusmello/cueglint/releases/tag/v1.0.0-dev.2".into(),
                    }))
                });
            }).platform_output
        };
        let mut state = ReleaseCheck::default();
        let disabled = draw(&mut state, false, vec![]).accesskit_update.unwrap();
        assert!(
            disabled
                .nodes
                .iter()
                .any(|(_, n)| n.label() == Some("Check now") && n.is_disabled())
        );
        for expected_calls in 1..=2 {
            let tree = draw(&mut state, true, vec![]).accesskit_update.unwrap();
            assert_eq!(calls.load(Ordering::SeqCst), expected_calls - 1);
            draw(&mut state, true, click_calendar_button(&tree, "Check now"));
            let deadline = Instant::now() + Duration::from_secs(3);
            while matches!(state, ReleaseCheck::Checking(_)) {
                assert!(Instant::now() < deadline);
                draw(&mut state, true, vec![]);
                std::thread::yield_now();
            }
            assert_eq!(calls.load(Ordering::SeqCst), expected_calls);
            if expected_calls == 1 {
                assert!(
                    matches!(&state, ReleaseCheck::Finished(Err(s)) if s == "Original fixture failure")
                );
            }
        }
        let tree = draw(&mut state, true, vec![]).accesskit_update.unwrap();
        assert!(
            tree.nodes
                .iter()
                .any(|(_, n)| n.value() == Some("Published prerelease"))
        );
        let output = draw(
            &mut state,
            true,
            click_calendar_button(&tree, "View release"),
        );
        assert!(
            output
                .commands
                .iter()
                .any(|cmd| matches!(cmd, egui::OutputCommand::OpenUrl(url)
            if url.url == "https://github.com/thematheusmello/cueglint/releases/tag/v1.0.0-dev.2"))
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn calendar_reminder_opt_in_and_start_follow_the_native_meeting_action() {
        use crate::runtime::{HighlightsBinding, HighlightsPhase, HighlightsStatus};
        use cueglint_providers::calendar::{UpcomingEvents, UtcInstant};
        use std::{collections::BTreeMap, sync::Arc};

        #[derive(Default)]
        struct MemoryStorage(BTreeMap<String, String>);
        impl eframe::Storage for MemoryStorage {
            fn get_string(&self, key: &str) -> Option<String> {
                self.0.get(key).cloned()
            }
            fn set_string(&mut self, key: &str, value: String) {
                self.0.insert(key.to_owned(), value);
            }
            fn remove_string(&mut self, key: &str) {
                self.0.remove(key);
            }
            fn flush(&mut self) {}
        }

        let context = egui::Context::default();
        context.enable_accesskit();
        let (mut app, store, _, commands) = question_app(&context);
        app.page = Page::Calendar;
        app.overlay_open = false;
        app.settings_visible = true;
        app.selected_mode = Some("meeting".into());
        let now = SystemTime::now();
        let start = (now + Duration::from_secs(120))
            .duration_since(UNIX_EPOCH)
            .unwrap();
        app.runtime.calendar.connection = CalendarConnection::Connected;
        // The last explicit refresh can be hours old while still current.
        app.runtime.calendar.refreshed_at = now.checked_sub(Duration::from_secs(8 * 3600));
        app.runtime.calendar.page_scope = Some([11; 32]);
        app.runtime.calendar.upcoming = Some(Arc::new(UpcomingEvents {
            events: vec![CalendarEvent {
                id: "original-private-event-id".into(),
                title: Some("  Original harbor\nmeeting  ".into()),
                time: EventTime::Timed {
                    starts_at: CalendarDateTime {
                        utc: UtcInstant {
                            unix_seconds: i64::try_from(start.as_secs()).unwrap(),
                            nanoseconds: start.subsec_nanos(),
                        },
                        offset_minutes: Some(-240),
                    },
                    ends_at: None,
                },
            }],
            calendar_time_zone: None,
            has_more: false,
        }));
        let title = "Original harbor meeting";
        let has = |tree: &egui::accesskit::TreeUpdate, label: &str| {
            tree.nodes
                .iter()
                .any(|(_, node)| node.value() == Some(label) || node.label() == Some(label))
        };
        app.reconcile_reminders(now, &context);
        let disabled = calendar_frame(&mut app, &context, true, vec![]);
        assert!(!has(&disabled, title) && !has(&disabled, "Start"));
        let (id, _) = disabled
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some("Remind me five minutes before meetings"))
            .expect("missing opt-in preference");
        let toggle = egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
            action: egui::accesskit::Action::Click,
            target_tree: egui::accesskit::TreeId::ROOT,
            target_node: *id,
            data: None,
        });
        calendar_frame(&mut app, &context, true, vec![toggle]);
        assert!(app.calendar_reminders.enabled());
        app.reconcile_reminders(now, &context);
        let due = calendar_frame(&mut app, &context, true, vec![]);
        assert!(has(&due, title));
        assert!(
            commands.try_recv().is_err(),
            "reminders cannot start capture or refresh by themselves"
        );

        app.runtime.calendar.operation = Some(CalendarOperation::Cancelling);
        app.reconcile_reminders(now, &context);
        assert!(!app.has_calendar_reminder(now));
        app.runtime.calendar.operation = None;
        app.reconcile_reminders(now, &context);
        assert!(app.has_calendar_reminder(now));

        app.acknowledgement_kinds
            .push_back(Acknowledgement::Restore);
        app.reconcile_reminders(now, &context);
        assert!(!app.has_calendar_reminder(now));
        app.acknowledgement_kinds.pop_back();
        app.reconcile_reminders(now, &context);
        assert!(app.has_calendar_reminder(now));

        let mut storage = MemoryStorage::default();
        eframe::App::save(&mut app, &mut storage);
        assert!(storage.0.values().all(|value| !value.contains(title)
            && !value.contains("original-private-event-id")));
        assert!(
            !store
                .backup_json()
                .unwrap()
                .contains("original-private-event-id")
        );
        app.calendar_reminders = reminder_ui::restore(Some(&storage));
        assert!(app.calendar_reminders.enabled());
        app.runtime.calendar.refreshed_at = Some(now);
        app.reconcile_reminders(now, &context);
        assert!(!has(
            &calendar_frame(&mut app, &context, true, vec![]),
            title
        ));
        assert!(commands.try_recv().is_err());

        // A new grant receives its own occurrence. An obsolete card cannot start a session.
        app.runtime.calendar.page_scope = Some([12; 32]);
        app.reconcile_reminders(now, &context);
        let new_grant = calendar_frame(&mut app, &context, true, vec![]);
        let start = click_calendar_button(&new_grant, "Start");
        app.runtime.calendar.stale = true;
        calendar_frame(&mut app, &context, true, start);
        assert!(commands.try_recv().is_err());
        app.reconcile_reminders(now, &context);
        assert!(!app.has_calendar_reminder(now));

        app.runtime.calendar.stale = false;
        app.runtime.calendar.page_scope = Some([13; 32]);
        app.reconcile_reminders(now, &context);
        app.runtime.highlights = Some(HighlightsStatus {
            binding: HighlightsBinding {
                operation_id: "reminder-fixture-highlights".into(),
                session_id: app.selected().unwrap().id.clone(),
                profile: models::GENERATION_PROFILE.into(),
                root: app.runtime.model_root.clone(),
                source_sha256: "a".repeat(64),
                job_id: Some("reminder-fixture-job".into()),
            },
            phase: HighlightsPhase::Generating,
            completed_chunks: 0,
            total_chunks: 1,
        });
        let busy = calendar_frame(&mut app, &context, true, vec![]);
        let (id, start) = busy
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some("Start"))
            .expect("missing Start during Highlights");
        assert!(start.is_disabled());
        let click = egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
            action: egui::accesskit::Action::Click,
            target_tree: egui::accesskit::TreeId::ROOT,
            target_node: *id,
            data: None,
        });
        calendar_frame(&mut app, &context, true, vec![click]);
        assert!(commands.try_recv().is_err());
        assert!(app.after_notes.is_none());
        assert!(app.has_calendar_reminder(now));

        app.runtime.highlights.as_mut().unwrap().phase = HighlightsPhase::Ready;
        let (blocked, _receiver) = std::sync::mpsc::sync_channel(0);
        let sender = std::mem::replace(&mut app.worker.commands, blocked);
        let ready = calendar_frame(&mut app, &context, true, vec![]);
        calendar_frame(
            &mut app,
            &context,
            true,
            click_calendar_button(&ready, "Start"),
        );
        assert!(app.has_calendar_reminder(now));
        assert!(app.after_notes.is_none());
        app.worker.commands = sender;
        app.overlay_open = false;

        let ready = calendar_frame(&mut app, &context, true, vec![]);
        calendar_frame(
            &mut app,
            &context,
            true,
            click_calendar_button(&ready, "Start"),
        );
        assert!(matches!(
            commands.try_recv().unwrap(),
            Command::Start { title: started, mode } if started == title && mode.as_deref() == Some("meeting")
        ));
        assert!(commands.try_recv().is_err());
        assert!(app.overlay_panel == OverlayPanel::Chat && app.overlay_open);
        assert!(!app.has_calendar_reminder(now));
    }

    #[test]
    fn calendar_fixtures_show_sample_states_and_disable_requests() {
        for (view, settings, expected, request_buttons) in [
            (
                crate::calendar_fixture::SETTINGS,
                true,
                "Connected to your primary calendar",
                &["Refresh", "Disconnect"][..],
            ),
            (
                crate::calendar_fixture::HISTORY,
                false,
                "Harbor exhibit walk-through",
                &["Refresh calendar"][..],
            ),
            (
                crate::calendar_fixture::REMINDER,
                false,
                "Harbor exhibit walk-through",
                &["Start", "Dismiss"][..],
            ),
        ] {
            let context = egui::Context::default();
            context.enable_accesskit();
            let creation = eframe::CreationContext::_new_kittest(context.clone());
            let mut app = Cueglint::new(
                &creation,
                Options {
                    fixture: Some(view.into()),
                    ..Default::default()
                },
            );
            let Response::State(state, _) = app
                .worker
                .responses
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
            else {
                panic!("Calendar fixture did not initialize");
            };
            app.apply_snapshot(state, None, false);
            app.pending = 0;
            app.acknowledgement_kinds.clear();
            let caption = if app.calendar_reminder_preview() {
                assert!(app.overlay_panel == OverlayPanel::Compact);
                assert!(!app.screenshot_content_ready());
                let now = app.calendar_reminder_now();
                assert!(app.reconcile_reminders(now, &context));
                assert!(app.has_calendar_reminder(now));
                assert!(!app.has_calendar_reminder(now + Duration::from_secs(120)));
                "Sample reminder · 2026-10-06 · 09:30 UTC"
            } else {
                assert!(app.screenshot_content_ready());
                assert!(if settings {
                    app.page == Page::Calendar
                } else {
                    app.overlay_panel == OverlayPanel::History
                });
                "Sample calendar · requests disabled"
            };
            let tree = calendar_frame(&mut app, &context, settings, vec![]);
            assert!(app.screenshot_content_ready());
            for expected in [expected, caption] {
                assert!(
                    tree.nodes
                        .iter()
                        .any(|(_, node)| node.value() == Some(expected)),
                    "missing {expected} in {view}"
                );
            }
            let clicks = request_buttons
                .iter()
                .map(|label| {
                    let (id, node) = tree
                        .nodes
                        .iter()
                        .find(|(_, node)| node.label() == Some(*label))
                        .unwrap_or_else(|| panic!("missing {label} button in {view}"));
                    assert!(node.is_disabled(), "{label} enabled in {view}");
                    egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
                        action: egui::accesskit::Action::Click,
                        target_tree: egui::accesskit::TreeId::ROOT,
                        target_node: *id,
                        data: None,
                    })
                })
                .collect();
            calendar_frame(&mut app, &context, settings, clicks);
            assert_eq!(app.pending, 0, "{view} queued a request");
        }
    }

    #[test]
    fn calendar_settings_routes_explicit_actions_and_local_removal_after_failure() {
        use cueglint_providers::calendar::{CalendarDisconnectReport, CalendarError};

        let context = egui::Context::default();
        context.enable_accesskit();
        let (mut app, _, _, commands) = question_app(&context);
        app.page = Page::Calendar;
        let action = |app: &mut Cueglint, label: &str| {
            let tree = calendar_frame(app, &context, true, vec![]);
            assert!(
                commands.try_recv().is_err(),
                "drawing Calendar must not issue a command"
            );
            calendar_frame(app, &context, true, click_calendar_button(&tree, label));
            let command = commands.try_recv().unwrap();
            assert_eq!(
                app.acknowledgement_kinds.pop_front(),
                Some(Acknowledgement::Calendar)
            );
            app.pending -= 1;
            command
        };
        assert!(matches!(
            action(&mut app, "Check connection"),
            Command::Calendar(CalendarCommand::Inspect)
        ));
        app.runtime.calendar.connection = CalendarConnection::Disconnected;
        let empty = calendar_frame(&mut app, &context, true, vec![]);
        assert!(
            empty
                .nodes
                .iter()
                .any(|(_, node)| node.label() == Some("Connect") && node.is_disabled())
        );
        app.calendar_client_id = "  original-calendar-client.apps.googleusercontent.com  ".into();
        assert!(
            matches!(action(&mut app, "Connect"), Command::Calendar(CalendarCommand::Connect { client_id })
            if client_id == "original-calendar-client.apps.googleusercontent.com")
        );
        app.runtime.calendar.operation = Some(CalendarOperation::Connecting);
        assert!(matches!(
            action(&mut app, "Cancel"),
            Command::Calendar(CalendarCommand::Cancel)
        ));
        app.runtime.calendar.operation = None;
        app.runtime.calendar.configured = true;
        app.runtime.calendar.connection = CalendarConnection::ReconnectRequired;
        assert!(matches!(
            action(&mut app, "Reconnect"),
            Command::Calendar(CalendarCommand::Reconnect)
        ));
        app.runtime.calendar.connection = CalendarConnection::Connected;
        assert!(matches!(
            action(&mut app, "Refresh"),
            Command::Calendar(CalendarCommand::Refresh)
        ));
        assert!(matches!(
            action(&mut app, "Disconnect"),
            Command::Calendar(CalendarCommand::Disconnect { local_only: false })
        ));
        app.runtime.calendar.disconnect = Some(CalendarDisconnectReport {
            remote: CalendarRevocation::Failed(CalendarError::Transport),
            local: None,
        });
        assert!(matches!(
            action(&mut app, "Retry disconnect"),
            Command::Calendar(CalendarCommand::Disconnect { local_only: false })
        ));
        assert!(matches!(
            action(&mut app, "Remove from this device"),
            Command::Calendar(CalendarCommand::Disconnect { local_only: true })
        ));
    }

    #[test]
    fn calendar_history_expands_only_the_loaded_page_and_preserves_date_boundaries() {
        use cueglint_providers::calendar::UpcomingEvents;

        let context = egui::Context::default();
        context.enable_accesskit();
        let (mut app, _, _, commands) = question_app(&context);
        let has = |tree: &egui::accesskit::TreeUpdate, text| {
            tree.nodes
                .iter()
                .any(|(_, node)| node.value() == Some(text))
        };
        let empty = calendar_frame(&mut app, &context, false, vec![]);
        assert!(!has(&empty, "UPCOMING"));
        app.runtime.calendar.connection = CalendarConnection::Connected;
        app.runtime.calendar.upcoming = Some(std::sync::Arc::new(UpcomingEvents {
            events: (1..=4)
                .map(|number| CalendarEvent {
                    id: number.to_string(),
                    title: Some(format!("Original calendar event {number}")),
                    time: EventTime::AllDay {
                        starts_on: "2024-02-29".into(),
                        ends_before: "2024-03-02".into(),
                    },
                })
                .collect(),
            calendar_time_zone: None,
            has_more: true,
        }));
        let preview = calendar_frame(&mut app, &context, false, vec![]);
        assert!(has(&preview, "Original calendar event 3"));
        assert!(!has(&preview, "Original calendar event 4"));
        assert!(has(
            &preview,
            "All day · 2024-02-29 until 2024-03-02 (exclusive)"
        ));
        calendar_frame(
            &mut app,
            &context,
            false,
            click_calendar_button(&preview, "Show more"),
        );
        let expanded = calendar_frame(&mut app, &context, false, vec![]);
        assert!(has(&expanded, "Original calendar event 4"));
        assert!(
            commands.try_recv().is_err(),
            "expanding the page must not enqueue a fetch"
        );
        app.history_search = "Original calendar event 4".into();
        let search = calendar_frame(&mut app, &context, false, vec![]);
        assert!(!has(&search, "UPCOMING"));
        assert!(has(&search, "No matching sessions"));
        assert!(commands.try_recv().is_err());
    }

    #[test]
    fn calendar_timed_labels_follow_each_offset_and_fall_back_to_utc() {
        use cueglint_providers::calendar::UtcInstant;

        let at = |unix_seconds, offset_minutes| CalendarDateTime {
            utc: UtcInstant {
                unix_seconds,
                nanoseconds: 0,
            },
            offset_minutes,
        };
        for (starts_at, ends_at, date, label) in [
            (
                at(-60, None),
                Some(at(60, None)),
                "1969-12-31",
                "1969-12-31 23:59 UTC – 1970-01-01 00:01 UTC",
            ),
            (
                at(-19_860, Some(330)),
                Some(at(-19_740, Some(330))),
                "1969-12-31",
                "1969-12-31 23:59 UTC+05:30 – 1970-01-01 00:01 UTC+05:30",
            ),
            (
                at(-19_740, Some(330)),
                Some(at(-19_500, Some(330))),
                "1970-01-01",
                "1970-01-01 · 00:01–00:05 UTC+05:30",
            ),
            (
                at(1_730_613_000, Some(-240)),
                Some(at(1_730_614_200, Some(-300))),
                "2024-11-03",
                "2024-11-03 01:50 UTC-04:00 – 2024-11-03 01:10 UTC-05:00",
            ),
            (at(60, None), None, "1970-01-01", "1970-01-01 · 00:01 UTC"),
        ] {
            assert_eq!(
                calendar_event_labels(&EventTime::Timed { starts_at, ends_at }),
                (date.to_owned(), label.to_owned()),
            );
        }
    }

    #[test]
    fn settings_backup_exports_latest_notes_and_preserves_an_existing_file() {
        use std::fs;

        let context = egui::Context::default();
        context.enable_accesskit();
        let creation = eframe::CreationContext::_new_kittest(context.clone());
        let mut app = Cueglint::new(
            &creation,
            Options {
                fixture: Some("native-v2-session".into()),
                ..Default::default()
            },
        );
        let finish = |app: &mut Cueglint| {
            let deadline = Instant::now() + Duration::from_secs(5);
            while app.pending > 0 && Instant::now() < deadline {
                app.poll(&context);
                std::thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(app.pending, 0, "store worker did not reply");
        };
        finish(&mut app);
        let session = app.selected().unwrap().id.clone();
        app.page = Page::Security;
        let draw = |app: &mut Cueglint, events| {
            context
                .run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(860.0, 670.0))),
                        events,
                        ..Default::default()
                    },
                    |ui| app.main_window(ui),
                )
                .platform_output
                .accesskit_update
                .unwrap()
        };
        let tree = draw(&mut app, vec![]);
        let (target, button) = tree
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some("Export backup"))
            .unwrap();
        assert!(button.is_disabled());

        let directory =
            std::env::temp_dir().join(format!("cueglint-backup-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("original backup.json");
        context.memory_mut(|memory| memory.request_focus(Id::new("backup-file-path")));
        let _ = draw(
            &mut app,
            vec![egui::Event::Paste(path.to_str().unwrap().into())],
        );
        assert_eq!(app.backup_path, path.to_str().unwrap());
        let click = || {
            vec![egui::Event::AccessKitActionRequest(
                egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Click,
                    target_tree: egui::accesskit::TreeId::ROOT,
                    target_node: *target,
                    data: None,
                },
            )]
        };
        let before_backup = "Original text typed immediately before backup.";
        app.notes = before_backup.into();
        app.notes_dirty = true;
        let _ = draw(&mut app, click());
        assert!(app.notes_saving && !path.exists());
        finish(&mut app);
        assert_eq!(
            app.notice,
            Some((format!("Backup exported to {}", path.display()), false))
        );
        let backup = fs::read_to_string(&path).unwrap();
        let mut restored = cueglint_core::Store::open_in_memory().unwrap();
        restored.restore_backup(&backup, false).unwrap();
        assert_eq!(restored.session(&session).unwrap().notes, before_backup);

        let after_backup = "Original text edited after the backup.";
        app.notes = after_backup.into();
        app.notes_dirty = true;
        let _ = draw(&mut app, click());
        finish(&mut app);
        let (error, failed) = app.notice.as_ref().unwrap();
        assert!(*failed && error.starts_with("Could not save backup to "));
        assert!(error.contains("without overwriting an existing file"));
        assert_eq!(fs::read_to_string(&path).unwrap(), backup);
        assert_eq!(app.selected().unwrap().notes, after_backup);
        assert!(!app.notes_dirty);
        fs::remove_file(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn versioned_fixtures_initialize_and_render_their_primary_views_without_a_window() {
        for view in crate::FIXTURE_VIEWS {
            let context = egui::Context::default();
            let creation = eframe::CreationContext::_new_kittest(context.clone());
            let options = Options {
                fixture: Some(format!("native-v2-{view}")),
                ..Default::default()
            };
            let size = options.fixture_viewport_size().unwrap_or([860.0, 670.0]);
            let mut app = Cueglint::new(&creation, options);
            let Response::State(initial, _) = app
                .worker
                .responses
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
            else {
                panic!("v2 fixture did not initialize: {view}")
            };
            app.pending = 0;
            app.acknowledgement_kinds.clear();
            app.apply_snapshot(initial, None, false);
            app.synchronize_markdown();
            assert_eq!(
                app.selected().is_some(),
                !matches!(view, "dashboard" | "history")
            );
            if let Some(session) = app.selected() {
                assert!(session.id.starts_with("native-v2-"));
            }
            let expected = match view {
                "session" => OverlayPanel::Detail,
                "history" => OverlayPanel::History,
                "markdown" | "markdown-long" => OverlayPanel::Chat,
                _ => OverlayPanel::Compact,
            };
            assert!(app.overlay_panel == expected);
            let mut frame = eframe::Frame::_new_kittest();
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(size[0], size[1]))),
                    ..Default::default()
                },
                |ui| eframe::App::ui(&mut app, ui, &mut frame),
            );
            assert!(!output.shapes.is_empty());
            assert_eq!(
                output.viewport_output.len(),
                1,
                "v2 views use only the primary viewport"
            );
            if matches!(view, "markdown" | "markdown-long") {
                let deadline = Instant::now() + Duration::from_secs(5);
                while !app.screenshot_content_ready() && Instant::now() < deadline {
                    app.poll(&context);
                    std::thread::sleep(Duration::from_millis(5));
                }
                assert!(
                    app.screenshot_content_ready(),
                    "v2 Markdown capture waits for the selected formatted source"
                );
            }
        }
    }

    #[test]
    fn provider_fixture_waits_for_fonts_and_reflow_before_exposing_the_empty_key_draft() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let creation = eframe::CreationContext::_new_kittest(context.clone());
        let mut app = Cueglint::new(
            &creation,
            Options {
                fixture: Some("provider-v2".into()),
                ..Default::default()
            },
        );
        let Response::State(initial, _) = app
            .worker
            .responses
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        else {
            panic!("provider fixture did not initialize")
        };
        assert!(initial.settings.answer_provider.provider.is_local());
        app.pending = 0;
        app.acknowledgement_kinds.clear();
        app.apply_snapshot(initial, None, false);
        assert!(!app.screenshot_content_ready());
        let mut frame = eframe::Frame::_new_kittest();
        let _ = context.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(860.0, 670.0))),
                time: Some(0.0),
                ..Default::default()
            },
            |ui| eframe::App::ui(&mut app, ui, &mut frame),
        );
        assert!(!app.fonts.ready());
        assert!(
            app.reveal_provider,
            "initial fallback metrics must not consume the provider scroll target"
        );
        app.runtime.capture_inputs.microphone_error = Some(
            "Original fixture input catalog notice.\nThe previous input list is unavailable.\nReconnect the fixture device before refreshing."
                .into(),
        );
        let mut ready = None;
        for index in 1..90 {
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(860.0, 670.0))),
                    time: Some(index as f64 / 60.0),
                    ..Default::default()
                },
                |ui| {
                    eframe::App::logic(&mut app, ui.ctx(), &mut frame);
                    eframe::App::ui(&mut app, ui, &mut frame);
                },
            );
            if app.screenshot_content_ready() {
                ready = Some(output);
                break;
            }
        }
        let output = ready.expect(
            "the existing General scroll area must expose the provider panel before capture",
        );
        assert!(app.fonts.ready() && !app.reveal_provider);
        assert!(app.page == Page::General && app.provider_key.is_empty());
        assert_eq!(output.viewport_output.len(), 1);
        let visible = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text)
                    if shape
                        .clip_rect
                        .intersects(text.galley.mesh_bounds.translate(text.pos.to_vec2())) =>
                {
                    Some(text.galley.job.text.as_str())
                }
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");
        for expected in [
            "Provider settings preview",
            "original-fixture-model",
            "https://models.example.test/v1",
            "Provider charges may apply",
        ] {
            assert!(
                visible.contains(expected),
                "missing visible provider fixture field: {expected}"
            );
        }
        let tree = output.platform_output.accesskit_update.unwrap();
        assert!(
            tree.nodes
                .iter()
                .any(|(_, node)| node.role() == egui::accesskit::Role::PasswordInput)
        );
        for label in [
            "Save key",
            "Delete key",
            "Check key status",
            "Refresh models",
            "Test connection",
        ] {
            assert!(
                tree.nodes
                    .iter()
                    .any(|(_, node)| node.label() == Some(label) && node.is_disabled()),
                "fixture I/O action must be disabled: {label}"
            );
        }
        let config = app.provider_draft.clone().unwrap();
        for action in [
            ProviderAction::Test,
            ProviderAction::Save(Secret::new("cueglint-fixture-never-valid-key".into()).unwrap()),
        ] {
            app.worker
                .commands
                .send(Command::Provider(
                    uuid::Uuid::new_v4().to_string(),
                    config.clone(),
                    action,
                ))
                .unwrap();
            let Response::Error(error) = app
                .worker
                .responses
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
            else {
                panic!("fixture actor must reject provider I/O")
            };
            assert!(error.contains("provider operations are disabled in deterministic fixtures"));
            let Response::Update(snapshot, _) = app
                .worker
                .responses
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
            else {
                panic!("fixture rejection must preserve state")
            };
            assert!(snapshot.settings.answer_provider.provider.is_local());
            assert!(snapshot.runtime.provider.is_none() && snapshot.runtime.generation.is_none());
        }
    }

    #[test]
    fn settings_navigation_keeps_measured_rows_pitch_origin_and_tiles() {
        let context = egui::Context::default();
        let (mut app, _, _, _) = question_app(&context);
        let mut origin = Pos2::ZERO;
        let output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(860.0, 670.0))),
                ..Default::default()
            },
            |ui| {
                origin = ui.max_rect().min;
                app.main_window(ui);
            },
        );
        let selected = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if rect.fill == SELECTED && rect.rect.size() == vec2(180.0, 36.0) =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
            .expect("selected General navigation row");
        assert_eq!(selected.top(), origin.y + 154.0);
        let tiles = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.rect.size() == vec2(23.0, 23.0) => Some(rect.rect),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(tiles.len(), 8, "Knowledge remains in the native navigation");
        for pair in tiles.windows(2) {
            assert_eq!(pair[1].top() - pair[0].top(), 38.0);
        }
        let hero = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.rect.size() == vec2(600.0, 184.0) => {
                    Some(rect.rect)
                }
                _ => None,
            })
            .expect("General hero");
        assert_eq!(hero.min, origin + vec2(230.0, 60.0));
    }

    #[test]
    fn settings_history_follows_category_links_and_branches_without_losing_drafts() {
        use egui::accesskit::{Action, ActionRequest, Role, TreeId, TreeUpdate};

        let context = egui::Context::default();
        context.enable_accesskit();
        let (mut app, _, _, commands) = question_app(&context);
        let draft = Mode::new(
            "Original unsaved mode",
            "Keep this draft while browsing settings.",
        );
        app.mode_draft = Some(draft.clone());
        let provider_draft = ProviderConfig {
            provider: ProviderId::OpenAiCompatible,
            model_id: "original-draft-model".into(),
            base_url: Some("http://127.0.0.1:5799/v1/".into()),
        };
        app.provider_draft = Some(provider_draft.clone());
        app.backup_path = "original draft backup.json".into();
        let frame = |app: &mut Cueglint, events| {
            context
                .run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(860.0, 670.0))),
                        events,
                        ..Default::default()
                    },
                    |ui| app.main_window(ui),
                )
                .platform_output
                .accesskit_update
                .unwrap()
        };
        let node = |tree: &TreeUpdate, label| {
            tree.nodes
                .iter()
                .find(|(_, n)| n.role() == Role::Button && n.label() == Some(label))
                .map(|(id, n)| (*id, n.is_disabled(), n.bounds().unwrap()))
                .unwrap_or_else(|| panic!("Missing Settings control {label}"))
        };
        let activate = |target_node, action| {
            vec![egui::Event::AccessKitActionRequest(ActionRequest {
                action,
                target_tree: TreeId::ROOT,
                target_node,
                data: None,
            })]
        };
        let click = |app: &mut Cueglint, label| {
            let (id, disabled, _) = node(&frame(app, vec![]), label);
            assert!(!disabled, "{label}");
            let _ = frame(app, activate(id, Action::Click));
        };
        let initial = frame(&mut app, vec![]);
        for (label, left) in [("Back in Settings", 230.0), ("Forward in Settings", 271.0)] {
            let (_, disabled, bounds) = node(&initial, label);
            assert!(disabled);
            assert_eq!(
                (bounds.x0, bounds.y0, bounds.x1, bounds.y1),
                (left, 6.0, left + 40.0, 42.0)
            );
        }
        click(&mut app, "Modes");
        click(&mut app, "Open knowledge");
        assert!(app.page == Page::Knowledge);
        assert!(node(&frame(&mut app, vec![]), "Forward in Settings").1);

        app.settings_search = "SEC".into();
        click(&mut app, "Back in Settings");
        assert!(app.page == Page::Modes);
        let (forward, disabled, _) = node(&frame(&mut app, vec![]), "Forward in Settings");
        assert!(!disabled);
        let focused = frame(&mut app, activate(forward, Action::Focus));
        assert_eq!(focused.focus, forward);
        let _ = frame(&mut app, vec![enter_event()]);
        assert!(app.page == Page::Knowledge);

        click(&mut app, "Back in Settings");
        click(&mut app, "Security");
        click(&mut app, "Security");
        assert!(app.page == Page::Security);
        assert!(node(&frame(&mut app, vec![]), "Forward in Settings").1);
        click(&mut app, "Back in Settings");
        assert!(app.page == Page::Modes);
        click(&mut app, "Back in Settings");
        assert!(app.page == Page::General);
        assert!(node(&frame(&mut app, vec![]), "Back in Settings").1);
        assert_eq!(app.settings_search, "SEC");
        assert_eq!(app.mode_draft.as_ref(), Some(&draft));
        assert_eq!(app.provider_draft.as_ref(), Some(&provider_draft));
        assert_eq!(app.backup_path, "original draft backup.json");
        assert!(commands.try_recv().is_err());
    }

    #[test]
    fn general_hero_keeps_its_measured_origin_across_embedded_font_metrics() {
        let mut heights = Vec::new();
        for scale in [0.8, 1.35] {
            let context = egui::Context::default();
            context.enable_accesskit();
            let (mut app, _, _, _) = question_app(&context);
            let mut fonts = egui::FontDefinitions::default();
            for font in fonts.font_data.values_mut() {
                std::sync::Arc::make_mut(font).tweak.scale = scale;
            }
            context.set_fonts(fonts);
            let mut origin = Pos2::ZERO;
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(860.0, 670.0))),
                    ..Default::default()
                },
                |ui| {
                    origin = ui.max_rect().min;
                    app.main_window(ui);
                },
            );
            let heading = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text)
                        if text.galley.job.text == "General"
                            && text
                                .galley
                                .job
                                .sections
                                .iter()
                                .all(|section| section.format.font_id.size == 14.0) =>
                    {
                        Some(text)
                    }
                    _ => None,
                })
                .expect("real General heading label");
            heights.push(heading.galley.size().y);
            let hero = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect) if rect.rect.size() == vec2(600.0, 184.0) => {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .expect("General hero");
            assert_eq!(hero.min, origin + vec2(230.0, 60.0));
            assert!(heading.pos.y + heading.galley.size().y <= hero.top());
            assert!(
                output
                    .platform_output
                    .accesskit_update
                    .unwrap()
                    .nodes
                    .iter()
                    .any(|(_, node)| node.role() == egui::accesskit::Role::Label
                        && node.value() == Some("General")),
                "heading keeps real label semantics"
            );
        }
        assert!(
            (heights[1] - heights[0]).abs() >= 4.0,
            "the production geometry must be exercised with materially different metrics"
        );
    }

    #[test]
    fn centered_question_keeps_fifteen_point_text_send_geometry_ime_and_horizontal_scroll() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let (mut app, _, _, _) = question_app(&context);
        app.fixture = Some("native-v2-overlay".into());
        let input = || egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(690.0, 104.0))),
            ..Default::default()
        };
        let output = context.run_ui(input(), |ui| app.question_input(ui, 0.0, false));
        let hint = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == "Ask anything…" => Some(text),
                _ => None,
            })
            .unwrap();
        assert!(
            hint.galley
                .job
                .sections
                .iter()
                .all(|section| section.format.font_id.size == 13.0),
            "hint atoms retain their existing Body font independently of editable text"
        );
        let ink = hint.galley.mesh_bounds.translate(hint.pos.to_vec2());
        assert!(
            (25.0..=32.0).contains(&ink.center().y),
            "logical glyph bounds must sit within the header center: {ink:?}"
        );
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Rect(rect) if rect.rect == Rect::from_min_size(pos2(646.0, 12.0), vec2(32.0, 32.0)))));
        assert!(
            output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .any(|(_, node)| node.label() == Some("Send question") && node.is_disabled())
        );
        let id = Id::new("question-input");
        context.memory_mut(|memory| memory.request_focus(id));
        let _ = context.run_ui(input(), |ui| app.question_input(ui, 0.0, false));
        let mut edit = input();
        edit.events = vec![
            egui::Event::Text("Original café ".into()),
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "仮".into(),
                active_range_chars: Some(0..1),
            }),
            egui::Event::Ime(egui::ImeEvent::Commit("完成".into())),
        ];
        let _ = context.run_ui(edit, |ui| app.question_input(ui, 0.0, false));
        assert_eq!(app.question, "Original café 完成");
        app.fixture = None;
        let typed = context.run_ui(input(), |ui| app.question_input(ui, 0.0, false));
        assert!(typed.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.job.text == app.question && text.galley.job.sections.iter().all(|section| section.format.font_id.size == 15.0))));
        let text = typed
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == app.question => Some(text),
                _ => None,
            })
            .unwrap();
        assert!(
            (25.0..=32.0).contains(
                &text
                    .galley
                    .mesh_bounds
                    .translate(text.pos.to_vec2())
                    .center()
                    .y
            )
        );
        assert!(typed.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Rect(rect) if rect.fill == Color32::from_rgb(151, 152, 154) && rect.rect == Rect::from_min_size(pos2(646.0, 12.0), vec2(32.0, 32.0)))));
        let long = format!("{} END", "original horizontal text ".repeat(40));
        app.question = long.clone();
        let mut state = egui::text_edit::TextEditState::load(&context, id).unwrap();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(long.chars().count()),
            )));
        state.store(&context, id);
        let output = context.run_ui(input(), |ui| app.question_input(ui, 0.0, false));
        let line = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == long => {
                    Some((text, shape.clip_rect))
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(line.0.galley.rows.len(), 1);
        assert!(
            line.0.pos.x < 15.0 && line.1.shrink(1.0).right() <= 629.0,
            "long prompts scroll within the original input width"
        );
        assert_eq!(app.question, long);
    }

    #[test]
    fn detail_title_editing_and_long_scroll_preserve_the_fixed_chip_row() {
        let context = egui::Context::default();
        let (mut app, store, _, commands) = question_app(&context);
        let session = store.session(&app.selected().unwrap().id).unwrap();
        let id = Id::new(("session-title", &session.id));
        context.memory_mut(|memory| memory.request_focus(id));
        let input = || egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(690.0, 760.0))),
            ..Default::default()
        };
        let _ = context.run_ui(input(), |ui| app.detail(ui));
        let mut state = egui::text_edit::TextEditState::load(&context, id).unwrap();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(app.session_title.chars().count()),
            )));
        state.store(&context, id);
        let title = "Original long title café with words to inspect without changing the single-line editing contract";
        let mut edit = input();
        edit.events.push(egui::Event::Text(title.into()));
        let output = context.run_ui(edit, |ui| app.detail(ui));
        assert_eq!(app.session_title, title);
        let line = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == title => {
                    Some((text, shape.clip_rect))
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(line.0.galley.rows.len(), 1);
        assert_eq!(line.0.galley.job.sections[0].format.font_id.size, 32.0);
        // egui expands the inner text clip by one point so its caret remains visible.
        let text_clip = line.1.shrink(1.0);
        assert!(
            text_clip.right() <= 670.0 && text_clip.bottom() <= 130.0,
            "inner title clip: {text_clip:?}"
        );
        let chips = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.fill == DETAIL_CHIP_FILL => Some(rect.rect),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(chips.len(), 2);
        assert!(
            chips
                .iter()
                .all(|chip| chip.top() == 130.0 && chip.height() == 32.0)
        );
        context.memory_mut(|memory| memory.surrender_focus(id));
        let _ = context.run_ui(input(), |ui| app.detail(ui));
        assert!(
            matches!(commands.try_recv().unwrap(), Command::Rename(saved, value) if saved == session.id && value == title)
        );
        assert_eq!(app.notes, session.notes);
    }

    #[test]
    fn detail_notes_use_twenty_eight_point_rows_and_only_detail_chips_grow() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let (mut app, store, _replies, _commands) = question_app(&context);
        let session = store.session(&app.selected().unwrap().id).unwrap();
        app.notes = format!(
            "First line  \n\nCafé 日本語\n{}",
            "wrapped words ".repeat(45)
        );
        let mut chip_heights = Vec::new();
        let output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(690.0, 760.0))),
                ..Default::default()
            },
            |ui| {
                ui.horizontal(|ui| {
                    chip_heights.push(detail_chip(ui, "1970-01-01 · UTC").rect.height());
                    chip_heights.push(detail_chip(ui, "2 speakers").rect.height());
                    ui.with_layout(Layout::top_down(Align::Min), |ui| keycap(ui, "FIXTURE"));
                });
                app.notes_ui(ui, &session);
            },
        );
        assert_eq!(chip_heights, [32.0, 32.0]);
        let painted_chips: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if rect.fill == DETAIL_CHIP_FILL || rect.fill == Color32::from_gray(61) =>
                {
                    Some(rect.rect.height())
                }
                _ => None,
            })
            .collect();
        assert_eq!(painted_chips.len(), 3);
        assert_eq!(painted_chips[..2], [32.0, 32.0]);
        let chip_shapes = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if rect.fill == DETAIL_CHIP_FILL || rect.fill == Color32::from_gray(61) =>
                {
                    Some(rect)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(chip_shapes[0].corner_radius, CornerRadius::same(16));
        assert_eq!(chip_shapes[1].corner_radius, CornerRadius::same(16));
        assert_eq!(chip_shapes[2].corner_radius, CornerRadius::same(5));
        assert!(
            (19.0..=23.0).contains(&painted_chips[2]),
            "painted chips: {painted_chips:?}"
        );
        let galley = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == app.notes => Some(&text.galley),
                _ => None,
            })
            .expect("native notes TextEdit must paint its unchanged source");
        assert!(
            galley.rows.len() > 5,
            "hard, blank, and wrapped lines must be laid out"
        );
        for pair in galley.rows.windows(2) {
            assert!((pair[1].pos.y - pair[0].pos.y - 28.0).abs() < 0.01);
        }
        assert!(galley.job.keep_trailing_whitespace);
        assert!(
            galley
                .job
                .sections
                .iter()
                .all(|section| section.format.font_id.size == 16.0)
        );
        let tree = output.platform_output.accesskit_update.unwrap();
        for label in ["1970-01-01 · UTC", "2 speakers"] {
            assert!(
                tree.nodes
                    .iter()
                    .any(|(_, node)| node.role() == egui::accesskit::Role::Label
                        && node.value() == Some(label))
            );
        }
    }

    #[test]
    fn notes_native_ime_selection_and_debounced_saves_preserve_exact_edits() {
        let context = egui::Context::default();
        let (mut app, mut store, replies, commands) = question_app(&context);
        let session = store.session(&app.selected().unwrap().id).unwrap();
        let notes_id = Id::new("session-notes");
        context.memory_mut(|memory| memory.request_focus(notes_id));
        let _ = context.run_ui(egui::RawInput::default(), |ui| app.notes_ui(ui, &session));
        let mut state = egui::text_edit::TextEditState::load(&context, notes_id).unwrap();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(0),
            )));
        state.store(&context, notes_id);
        let prefix = "Draft café  \n\n";
        for event in [
            egui::Event::Text(prefix.into()),
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "仮".into(),
                active_range_chars: Some(0..1),
            }),
            egui::Event::Ime(egui::ImeEvent::Commit("完成".into())),
        ] {
            let _ = context.run_ui(
                egui::RawInput {
                    events: vec![event],
                    ..Default::default()
                },
                |ui| app.notes_ui(ui, &session),
            );
        }
        let first = format!("{prefix}完成");
        assert_eq!(app.notes, first);
        assert!(app.notes_dirty);
        let mut state = egui::text_edit::TextEditState::load(&context, notes_id).unwrap();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(app.notes.chars().count()),
            )));
        state.store(&context, notes_id);
        let copied = context.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Copy],
                ..Default::default()
            },
            |ui| app.notes_ui(ui, &session),
        );
        assert!(copied.platform_output.commands.iter().any(|command| {
            matches!(command, egui::OutputCommand::CopyText(text) if text == &first)
        }));
        app.notes_edited = Instant::now();
        app.poll(&context);
        assert!(matches!(
            commands.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        app.notes_edited = Instant::now() - Duration::from_millis(501);
        app.poll(&context);
        let Command::Notes(id, text) = commands.try_recv().unwrap() else {
            panic!("the debounced edit must queue a notes save")
        };
        assert_eq!(id, session.id);
        assert_eq!(text, first);
        store.update_notes(&id, &text).unwrap();
        let saved = question_snapshot(&app, &store);
        // A new native edit must survive the acknowledgement of the older buffer.
        let _ = context.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Text(
                    "A newer selection replacement  \n".into(),
                )],
                ..Default::default()
            },
            |ui| app.notes_ui(ui, &session),
        );
        let latest = app.notes.clone();
        assert_eq!(latest, "A newer selection replacement  \n");
        replies.send(Response::State(saved, None)).unwrap();
        app.poll(&context);
        assert_eq!(app.notes, latest);
        assert!(app.notes_dirty && !app.notes_saving);
        app.notes_edited = Instant::now() - Duration::from_millis(501);
        app.poll(&context);
        let Command::Notes(id, text) = commands.try_recv().unwrap() else {
            panic!("the latest edit must receive its own save")
        };
        assert_eq!(text, latest);
        store.update_notes(&id, &text).unwrap();
        replies
            .send(Response::State(question_snapshot(&app, &store), None))
            .unwrap();
        app.poll(&context);
        assert!(!app.notes_dirty && !app.notes_saving);
        assert_eq!(app.selected().unwrap().notes, latest);
    }

    #[test]
    fn assistant_emits_semantic_accessibility_controls() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let creation = eframe::CreationContext::_new_kittest(context.clone());
        let mut app = Cueglint::new(
            &creation,
            Options {
                fixture: Some("overlay".into()),
                ..Default::default()
            },
        );
        let output = context.run_ui(egui::RawInput::default(), |ui| app.assistant_surface(ui));
        let tree = output
            .platform_output
            .accesskit_update
            .expect("assistant accessibility tree");
        assert!(
            tree.nodes
                .iter()
                .any(|(_, node)| node.role() == egui::accesskit::Role::TextInput)
        );
        for label in [
            "Send question",
            "Open settings",
            "Open history",
            "Mode: General",
        ] {
            assert!(
                tree.nodes
                    .iter()
                    .any(|(_, node)| node.label() == Some(label)),
                "missing accessible control: {label}"
            );
        }
    }

    #[test]
    fn assistant_regions_composite_translucently_once_with_rounded_corners_and_stable_hitboxes() {
        // Sample the actual filled native shapes over transparent clear color. Probes avoid text/icons.
        fn composite_alpha(output: &egui::FullOutput, point: Pos2) -> f32 {
            fn visit(shape: &egui::Shape, point: Pos2, alpha: &mut f32) {
                match shape {
                    egui::Shape::Vec(shapes) => {
                        for shape in shapes {
                            visit(shape, point, alpha);
                        }
                    }
                    egui::Shape::Rect(shape) if shape.rect.contains(point) => {
                        let rect = shape.rect;
                        let corners = [
                            (
                                pos2(rect.left(), rect.top()),
                                shape.corner_radius.nw,
                                vec2(1.0, 1.0),
                            ),
                            (
                                pos2(rect.right(), rect.top()),
                                shape.corner_radius.ne,
                                vec2(-1.0, 1.0),
                            ),
                            (
                                pos2(rect.left(), rect.bottom()),
                                shape.corner_radius.sw,
                                vec2(1.0, -1.0),
                            ),
                            (
                                pos2(rect.right(), rect.bottom()),
                                shape.corner_radius.se,
                                vec2(-1.0, -1.0),
                            ),
                        ];
                        for (corner, radius, direction) in corners {
                            let radius = f32::from(radius);
                            let center = corner + direction * radius;
                            if (point.x - corner.x).abs() < radius
                                && (point.y - corner.y).abs() < radius
                                && point.distance(center) > radius
                            {
                                return;
                            }
                        }
                        let source = f32::from(shape.fill.a()) / 255.0;
                        *alpha = source + *alpha * (1.0 - source);
                    }
                    _ => {}
                }
            }
            let mut alpha = 0.0;
            for shape in &output.shapes {
                if shape.clip_rect.contains(point) {
                    visit(&shape.shape, point, &mut alpha);
                }
            }
            alpha * 255.0
        }

        for (panel, height) in [
            (OverlayPanel::Compact, 104.0),
            (OverlayPanel::Chat, 500.0),
            (OverlayPanel::History, 760.0),
            (OverlayPanel::Detail, 760.0),
        ] {
            let context = egui::Context::default();
            context.enable_accesskit();
            let (mut app, _store, _replies, _commands) = question_app(&context);
            app.overlay_panel = panel;
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(690.0, height))),
                    ..Default::default()
                },
                |ui| app.assistant_surface(ui),
            );
            assert_eq!(composite_alpha(&output, pos2(1.0, 1.0)), 0.0);
            assert_eq!(composite_alpha(&output, pos2(1.0, height - 1.0)), 0.0);
            assert!(composite_alpha(&output, pos2(10.0, 10.0)) > 0.0);
            assert!(composite_alpha(&output, pos2(10.0, height - 10.0)) > 0.0);
            if matches!(panel, OverlayPanel::Compact | OverlayPanel::Chat) {
                for point in [pos2(500.0, 28.0), pos2(500.0, height - 24.0)] {
                    assert!((composite_alpha(&output, point) - 218.0).abs() < 0.01);
                }
                if panel == OverlayPanel::Chat {
                    assert!((composite_alpha(&output, pos2(5.0, 250.0)) - 210.0).abs() < 0.01);
                }
                let rectangles: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Rect(rect) => Some(rect),
                        _ => None,
                    })
                    .collect();
                let header = rectangles
                    .iter()
                    .find(|shape| shape.rect == Rect::from_min_size(Pos2::ZERO, vec2(690.0, 56.0)))
                    .unwrap();
                assert_eq!(
                    header.fill,
                    Color32::from_rgba_unmultiplied(119, 120, 122, 218)
                );
                assert_eq!(
                    header.corner_radius,
                    CornerRadius {
                        nw: 20,
                        ne: 20,
                        sw: 0,
                        se: 0
                    }
                );
                let toolbar = rectangles
                    .iter()
                    .find(|shape| {
                        shape.rect
                            == Rect::from_min_size(pos2(0.0, height - 48.0), vec2(690.0, 48.0))
                    })
                    .unwrap();
                assert_eq!(
                    toolbar.fill,
                    Color32::from_rgba_unmultiplied(78, 79, 81, 218)
                );
                assert_eq!(
                    toolbar.corner_radius,
                    CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: 20,
                        se: 20
                    }
                );
                let tree = output.platform_output.accesskit_update.unwrap();
                let send = tree
                    .nodes
                    .iter()
                    .find(|(_, node)| node.label() == Some("Send question"))
                    .unwrap()
                    .1
                    .bounds()
                    .unwrap();
                assert_eq!(
                    [send.x0, send.y0, send.x1, send.y1],
                    [646.0, 12.0, 678.0, 44.0]
                );
                let gear = tree
                    .nodes
                    .iter()
                    .find(|(_, node)| node.label() == Some("Open settings"))
                    .unwrap()
                    .1
                    .bounds()
                    .unwrap();
                assert_eq!(
                    [gear.x0, gear.y0, gear.x1, gear.y1],
                    [
                        16.0,
                        f64::from(height - 40.0),
                        48.0,
                        f64::from(height - 8.0)
                    ]
                );
            } else {
                assert_eq!(composite_alpha(&output, pos2(5.0, 300.0)), 255.0);
            }
        }
    }

    #[test]
    fn markdown_screenshot_fixtures_start_in_chat_and_wait_for_actual_formatting() {
        for (fixture, items) in [("markdown", 1), ("markdown-long", 80)] {
            let context = egui::Context::default();
            context.enable_accesskit();
            let creation = eframe::CreationContext::_new_kittest(context.clone());
            let mut app = Cueglint::new(
                &creation,
                Options {
                    fixture: Some(fixture.into()),
                    ..Default::default()
                },
            );
            assert!(app.overlay_panel == OverlayPanel::Chat);
            assert!(!app.screenshot_content_ready());
            let Response::State(initial, _) = app
                .worker
                .responses
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
            else {
                panic!("fixture did not initialize")
            };
            assert!(
                initial.runtime.capture.is_none()
                    && initial.runtime.generation.is_none()
                    && initial.runtime.model_job.is_none()
            );
            app.apply_snapshot(initial, None, false);
            assert!(
                !app.screenshot_content_ready(),
                "snapshot loading must not imply Markdown parsing is ready"
            );
            app.synchronize_markdown();
            let deadline = Instant::now() + Duration::from_secs(5);
            while !app.screenshot_content_ready() {
                app.answer_previews.poll();
                assert!(
                    Instant::now() < deadline,
                    "fixture formatting did not finish"
                );
                std::thread::sleep(Duration::from_millis(2));
            }
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(690.0, 500.0))),
                    ..Default::default()
                },
                |ui| app.assistant_surface(ui),
            );
            let tree = output.platform_output.accesskit_update.unwrap();
            assert_eq!(
                tree.nodes
                    .iter()
                    .filter(|(_, node)| node.role() == egui::accesskit::Role::ListItem)
                    .count(),
                items
            );
            assert!(
                tree.nodes
                    .iter()
                    .any(|(_, node)| node.label() == Some("Copy response"))
            );
            assert!(output.platform_output.commands.is_empty());
        }
    }

    #[test]
    fn screenshot_event_keeps_the_native_image_and_current_context_metrics_without_ui_file_io() {
        let context = egui::Context::default();
        let creation = eframe::CreationContext::_new_kittest(context.clone());
        let mut app = Cueglint::new(
            &creation,
            Options {
                fixture: Some("markdown".into()),
                screenshot: Some(std::env::temp_dir().join(format!(
                    "cueglint-native-event-{}.png",
                    uuid::Uuid::new_v4()
                ))),
                ..Default::default()
            },
        );
        let (jobs, received) = std::sync::mpsc::sync_channel(1);
        app.worker.screenshot = jobs;
        for _ in 0..3 {
            let _ = context.run_ui(Default::default(), |ui| app.fonts.poll(ui.ctx()));
        }
        assert!(app.fonts.ready());
        context.set_zoom_factor(1.25);
        let image = std::sync::Arc::new(egui::ColorImage::filled([3, 2], Color32::BLACK));
        let request_id = app.screenshot_request.as_ref().unwrap().id().to_string();
        let token = app.screenshot_request.as_ref().unwrap().token();
        let mut raw = egui::RawInput::default();
        raw.viewports
            .get_mut(&ViewportId::ROOT)
            .unwrap()
            .native_pixels_per_point = Some(2.0);
        let mut mismatched = raw.clone();
        mismatched.events.push(egui::Event::Screenshot {
            viewport_id: ViewportId::ROOT,
            user_data: screenshot::Request::new(Some("old-request".into())).token(),
            image: image.clone(),
        });
        mismatched.events.push(egui::Event::Screenshot {
            viewport_id: overlay_id(),
            user_data: token.clone(),
            image: image.clone(),
        });
        let _ = context.run_ui(mismatched, |ui| app.capture_screenshot(ui.ctx()));
        assert!(
            received.try_recv().is_err(),
            "wrong request or viewport must not enter the save worker"
        );
        assert!(app.screenshot.is_some());
        raw.events.push(egui::Event::Screenshot {
            viewport_id: ViewportId::ROOT,
            user_data: token,
            image: image.clone(),
        });
        let _ = context.run_ui(raw, |ui| app.capture_screenshot(ui.ctx()));
        let job = received.try_recv().unwrap();
        assert!(
            std::sync::Arc::ptr_eq(&job.image, &image),
            "pixel conversion/encoding belongs on the worker"
        );
        assert_eq!(job.image.size, [3, 2]);
        assert_eq!(job.metadata.request_id, request_id);
        assert_eq!(job.metadata.fonts["state"], "ready");
        assert_eq!(
            job.metadata.fonts["identity"]["source"],
            "embedded-fallback"
        );
        assert_eq!(job.metadata.fixture.as_deref(), Some("markdown"));
        assert_eq!(job.metadata.pixels_per_point, 2.5);
        assert_eq!(job.metadata.zoom_factor, 1.25);
        assert!(
            job.metadata.adapter.is_none(),
            "headless adapter identity must remain unknown"
        );
        assert!(app.screenshot.is_none());
        assert!(!job.path.exists());
    }

    #[test]
    fn dashboard_root_logic_requests_and_consumes_one_screenshot_without_hiding_settings() {
        let context = egui::Context::default();
        let creation = eframe::CreationContext::_new_kittest(context.clone());
        let mut app = Cueglint::new(
            &creation,
            Options {
                fixture: Some("dashboard".into()),
                screenshot: Some(std::env::temp_dir().join(format!(
                    "cueglint-dashboard-route-{}.png",
                    uuid::Uuid::new_v4()
                ))),
                ..Default::default()
            },
        );
        let Response::State(initial, _) = app
            .worker
            .responses
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        else {
            panic!("fixture did not initialize")
        };
        app.apply_snapshot(initial, None, false);
        app.pending = 0;
        app.acknowledgement_kinds.clear();
        app.ready_at = Some(Instant::now() - Duration::from_secs(1));
        let (jobs, received) = std::sync::mpsc::sync_channel(1);
        app.worker.screenshot = jobs;
        let input = || {
            let mut raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(860.0, 670.0))),
                ..Default::default()
            };
            let viewport = raw.viewports.get_mut(&ViewportId::ROOT).unwrap();
            viewport.minimized = Some(false);
            viewport.occluded = Some(false);
            raw
        };
        let mut frame = eframe::Frame::_new_kittest();
        let mut captured = None;
        let mut blocked = false;
        for _ in 0..8 {
            let output = context.run_ui(input(), |ui| {
                eframe::App::logic(&mut app, ui.ctx(), &mut frame);
                if !app.fonts.ready() {
                    blocked = true;
                    assert!(
                        !app.screenshot_requested,
                        "elapsed fixture time must not bypass font application readiness"
                    );
                }
                eframe::App::ui(&mut app, ui, &mut frame);
            });
            if output.viewport_output[&ViewportId::ROOT]
                .commands
                .iter()
                .any(|command| matches!(command, ViewportCommand::Screenshot(_)))
            {
                assert!(app.fonts.ready());
                captured = Some(output);
                break;
            }
        }
        assert!(blocked, "the test must exercise the font application gate");
        let output =
            captured.expect("the settled embedded font set must release one screenshot request");
        assert!(app.settings_visible && !app.overlay_open);
        let commands = &output.viewport_output[&ViewportId::ROOT].commands;
        assert_eq!(
            commands
                .iter()
                .filter(|command| matches!(command, ViewportCommand::Screenshot(_)))
                .count(),
            1
        );
        assert!(!commands.contains(&ViewportCommand::Visible(false)));
        assert!(!commands.contains(&ViewportCommand::Close));
        assert!(app.screenshot_requested);
        assert!(received.try_recv().is_err());
        for tick in 0..6 {
            let mut waiting = input();
            waiting.time = Some(10.0 + f64::from(tick));
            let output = context.run_ui(waiting, |ui| {
                eframe::App::logic(&mut app, ui.ctx(), &mut frame);
            });
            let root = &output.viewport_output[&ViewportId::ROOT];
            assert!(
                root.repaint_delay <= Duration::from_millis(100),
                "pending GPU readback must keep making progress without input"
            );
            assert!(
                !root
                    .commands
                    .iter()
                    .any(|command| { matches!(command, ViewportCommand::Screenshot(_)) })
            );
        }
        let image = std::sync::Arc::new(egui::ColorImage::filled([4, 3], Color32::BLACK));
        let mut readback = input();
        readback.events.push(egui::Event::Screenshot {
            viewport_id: ViewportId::ROOT,
            user_data: app.screenshot_request.as_ref().unwrap().token(),
            image: image.clone(),
        });
        let output = context.run_ui(readback, |ui| {
            // ROOT logic consumes readback without relying on an assistant surface.
            eframe::App::logic(&mut app, ui.ctx(), &mut frame);
            eframe::App::ui(&mut app, ui, &mut frame);
        });
        let job = received.try_recv().unwrap();
        assert!(std::sync::Arc::ptr_eq(&job.image, &image));
        assert_eq!(job.metadata.fixture.as_deref(), Some("dashboard"));
        assert!(!job.path.exists());
        assert!(app.screenshot.is_none());
        assert!(app.settings_visible && !app.overlay_open);
        assert!(
            !output.viewport_output[&ViewportId::ROOT]
                .commands
                .iter()
                .any(|command| matches!(command, ViewportCommand::Screenshot(_)))
        );
        let expected_trace = [
            ScreenshotStage::Initialized,
            ScreenshotStage::StateReady,
            ScreenshotStage::RootUi,
            ScreenshotStage::FontsReady,
            ScreenshotStage::ContentReady,
            ScreenshotStage::Requested,
            ScreenshotStage::EventReceived,
            ScreenshotStage::Queued,
        ]
        .into_iter()
        .fold(0u16, |bits, stage| bits | (1u16 << stage as u16));
        assert_eq!(app.screenshot_trace, Some(expected_trace));
        let trace = app.screenshot_trace;
        let _ = context.run_ui(input(), |ui| {
            eframe::App::logic(&mut app, ui.ctx(), &mut frame);
            eframe::App::ui(&mut app, ui, &mut frame);
        });
        assert_eq!(app.screenshot_trace, trace, "diagnostics must be finite");
        assert!(received.try_recv().is_err());
    }

    #[test]
    fn two_hundred_replies_draw_and_parse_only_the_selected_window_and_copy_older_source_exactly() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let creation = eframe::CreationContext::_new_kittest(context.clone());
        let mut app = Cueglint::new(
            &creation,
            Options {
                fixture: Some("overlay".into()),
                ..Default::default()
            },
        );
        let Response::State(mut initial, _) = app
            .worker
            .responses
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        else {
            panic!("fixture did not initialize")
        };
        let mut session = crate::reply_window::history(200);
        for answer in &mut session.suggestions {
            answer.text.push_str(&" original".repeat(7000));
        }
        initial.selected = Some(session);
        initial.sessions.clear();
        app.apply_snapshot(initial, None, false);
        app.synchronize_markdown();
        let (_sender, responses) = std::sync::mpsc::channel();
        app.worker.responses = responses;
        let input = || egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(690.0, 500.0))),
            ..Default::default()
        };
        let click = |target| {
            egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target_tree: egui::accesskit::TreeId::ROOT,
                target_node: target,
                data: None,
            })
        };
        let first = context.run_ui(input(), |ui| app.conversation(ui));
        let tree = first.platform_output.accesskit_update.unwrap();
        assert_eq!(
            tree.nodes
                .iter()
                .filter(|(_, node)| node.label() == Some("Copy response"))
                .count(),
            crate::reply_window::LIMIT,
            "layout may only visit the current reply window"
        );
        let target = tree
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some("Earlier replies"))
            .unwrap()
            .0;
        let mut earlier = input();
        earlier.events.push(click(target));
        let _ = context.run_ui(earlier, |ui| app.conversation(ui));
        assert!(app.reply_navigation.is_some());
        app.poll(&context); // There is deliberately no Store/Runtime response to trigger scheduling.
        assert_eq!(app.reply_window.range(), 168..184);
        let session = app.selected().unwrap();
        let request = &session.requests[168];
        assert!(
            !matches!(
                app.answer_previews
                    .get(&session.id, &request.id, request.context.revision),
                markdown::Preview::NotCached
            ),
            "page-only navigation must schedule its own formatting"
        );
        let newest = &session.requests[199];
        assert!(matches!(
            app.answer_previews
                .get(&session.id, &newest.id, newest.context.revision),
            markdown::Preview::NotCached
        ));
        let expected = session.suggestions[168].text.clone();
        let older = context.run_ui(input(), |ui| app.conversation(ui));
        let tree = older.platform_output.accesskit_update.unwrap();
        assert_eq!(
            tree.nodes
                .iter()
                .filter(|(_, node)| node.label() == Some("Copy response"))
                .count(),
            crate::reply_window::LIMIT
        );
        let copy = tree
            .nodes
            .iter()
            .filter(|(_, node)| node.label() == Some("Copy response"))
            .min_by(|left, right| {
                left.1
                    .bounds()
                    .unwrap()
                    .y0
                    .total_cmp(&right.1.bounds().unwrap().y0)
            })
            .unwrap()
            .0;
        let mut raw = input();
        raw.events.push(click(copy));
        let copied = context.run_ui(raw, |ui| app.conversation(ui));
        assert!(copied.platform_output.commands.iter().any(
            |command| matches!(command, egui::OutputCommand::CopyText(text) if text == &expected)
        ));
        let before = app.reply_window.range();
        let session = app.state.as_mut().unwrap().selected.as_mut().unwrap();
        let mut request = session.requests.last().unwrap().clone();
        request.id = "new-original-stream".into();
        request.status = cueglint_core::RequestStatus::Pending;
        request.context.revision += 1;
        session.active_request_id = Some(request.id.clone());
        session.requests.push(request.clone());
        app.runtime.generation = Some(crate::runtime::GenerationStatus {
            session_id: session.id.clone(),
            request_id: request.id.clone(),
            state: "generating".into(),
            text: "**Newest original source**".into(),
        });
        app.synchronize_markdown();
        assert_eq!(app.reply_window.range(), before);
        let session_id = app.selected().unwrap().id.clone();
        assert!(matches!(
            app.answer_previews
                .get(&session_id, &request.id, request.context.revision),
            markdown::Preview::NotCached
        ));
        app.reply_navigation = Some((session_id.clone(), ReplyNavigation::Latest));
        app.poll(&context);
        assert_eq!(app.reply_window.range(), 185..201);
        assert!(!matches!(
            app.answer_previews
                .get(&session_id, &request.id, request.context.revision),
            markdown::Preview::NotCached
        ));
        let old_session = session_id;
        let other = crate::reply_window::history(3);
        app.state.as_mut().unwrap().selected = Some(other);
        app.reply_navigation = Some((old_session, ReplyNavigation::Earlier));
        app.poll(&context);
        app.synchronize_markdown();
        assert_eq!(app.reply_window.range(), 0..3);
        let ordinary = context.run_ui(input(), |ui| app.conversation(ui));
        assert!(
            !ordinary
                .platform_output
                .accesskit_update
                .unwrap()
                .nodes
                .iter()
                .any(|(_, node)| node.label() == Some("Earlier replies"))
        );
    }

    #[test]
    fn conversation_formats_native_lists_code_and_links_while_copying_the_exact_original_response()
    {
        let context = egui::Context::default();
        context.enable_accesskit();
        let creation = eframe::CreationContext::_new_kittest(context.clone());
        let mut app = Cueglint::new(
            &creation,
            Options {
                fixture: Some("overlay".into()),
                ..Default::default()
            },
        );
        let Response::State(mut initial, _) = app
            .worker
            .responses
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        else {
            panic!("fixture did not initialize")
        };
        let source = "Original **answer** with [**safe** link](https://example.test/original).\n\n1. First original item\n2. Second original item\n\n```rust\nlet original = 7;\n```\n\n![original alt](https://never-fetch.invalid/image.png)\n\n<img src=\"file:///tmp/never-read\">";
        let mut store = cueglint_core::Store::open_in_memory().unwrap();
        let session = store
            .start_session("[Test] Original Markdown", None)
            .unwrap();
        let request = store
            .begin_request(&session.id, "Original scripted question")
            .unwrap();
        assert!(
            store
                .complete_request(&session.id, &request.id, source)
                .unwrap()
        );
        let session = store.session(&session.id).unwrap();
        initial.selected = Some(session.clone());
        initial.sessions = vec![session.clone()];
        app.apply_snapshot(initial, None, false);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            app.synchronize_markdown();
            if matches!(
                app.answer_previews
                    .get(&session.id, &request.id, request.context.revision),
                markdown::Preview::Formatted(..)
            ) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "answer formatting did not finish"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        let input = || egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(690.0, 1000.0))),
            ..Default::default()
        };
        let first = context.run_ui(input(), |ui| app.conversation(ui));
        assert!(
            first.platform_output.commands.is_empty(),
            "rendering cannot fetch/open/copy resources"
        );
        let tree = first.platform_output.accesskit_update.unwrap();
        assert_eq!(
            tree.nodes
                .iter()
                .filter(|(_, node)| node.role() == egui::accesskit::Role::ListItem)
                .count(),
            2
        );
        let link = tree
            .nodes
            .iter()
            .find(|(_, node)| node.role() == egui::accesskit::Role::Link)
            .expect("safe native link");
        assert_eq!(link.1.label(), Some("safe link"));
        let target = |label: &str| {
            tree.nodes
                .iter()
                .find(|(_, node)| node.label() == Some(label))
                .unwrap()
                .0
        };
        let click = |target| {
            egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target_tree: egui::accesskit::TreeId::ROOT,
                target_node: target,
                data: None,
            })
        };
        let mut copy = input();
        copy.events.push(click(target("Copy response")));
        let copied = context.run_ui(copy, |ui| app.conversation(ui));
        assert!(copied.platform_output.commands.iter().any(
            |command| matches!(command, egui::OutputCommand::CopyText(text) if text == source)
        ));
        let mut code = input();
        code.events.push(click(target("Copy code")));
        let copied_code = context.run_ui(code, |ui| app.conversation(ui));
        assert!(copied_code.platform_output.commands.iter().any(|command| matches!(command, egui::OutputCommand::CopyText(text) if text == "let original = 7;\n")));
        let mut open = input();
        open.events.push(click(link.0));
        let opened = context.run_ui(open, |ui| app.conversation(ui));
        assert!(opened.platform_output.commands.iter().any(|command| matches!(command, egui::OutputCommand::OpenUrl(url) if url.url == "https://example.test/original")));
        assert_eq!(app.selected().unwrap().suggestions[0].text, source);
    }

    #[test]
    fn streaming_response_copy_uses_the_latest_source_before_formatting_and_cancel_retires_it() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let creation = eframe::CreationContext::_new_kittest(context.clone());
        let mut app = Cueglint::new(
            &creation,
            Options {
                fixture: Some("overlay".into()),
                ..Default::default()
            },
        );
        let Response::State(mut initial, _) = app
            .worker
            .responses
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        else {
            panic!("fixture did not initialize")
        };
        let mut store = cueglint_core::Store::open_in_memory().unwrap();
        let session = store
            .start_session("[Test] Streaming Markdown", None)
            .unwrap();
        let request = store
            .begin_request(&session.id, "Original streaming question")
            .unwrap();
        initial.selected = Some(store.session(&session.id).unwrap());
        app.apply_snapshot(initial, None, false);
        app.runtime.generation = Some(crate::runtime::GenerationStatus {
            session_id: session.id.clone(),
            request_id: request.id.clone(),
            state: "generating".into(),
            text: "**Newest original streaming source**".into(),
        });
        app.synchronize_markdown();
        let input = || egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(690.0, 500.0))),
            ..Default::default()
        };
        let first = context.run_ui(input(), |ui| app.conversation(ui));
        let tree = first.platform_output.accesskit_update.unwrap();
        let target = tree
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some("Copy response"))
            .unwrap()
            .0;
        let deadline = Instant::now() + Duration::from_secs(5);
        while !matches!(
            app.answer_previews
                .get(&session.id, &request.id, request.context.revision),
            markdown::Preview::Formatted(..)
        ) {
            app.answer_previews.poll();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        let formatted = context.run_ui(input(), |ui| app.conversation(ui));
        assert!(
            formatted
                .platform_output
                .accesskit_update
                .unwrap()
                .nodes
                .iter()
                .any(|(id, node)| *id == target && node.label() == Some("Copy response")),
            "whole-response copy identity must survive pending-to-formatted layout changes"
        );
        let mut copy = input();
        copy.events.push(egui::Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target_tree: egui::accesskit::TreeId::ROOT,
                target_node: target,
                data: None,
            },
        ));
        let copied = context.run_ui(copy, |ui| app.conversation(ui));
        assert!(copied.platform_output.commands.iter().any(|command| matches!(command, egui::OutputCommand::CopyText(text) if text == "**Newest original streaming source**")));
        let (commands, _jobs) = std::sync::mpsc::sync_channel(1);
        app.worker.commands = commands;
        assert!(app.send(Command::Cancel));
        app.synchronize_markdown();
        let cancelled = context.run_ui(input(), |ui| app.conversation(ui));
        assert!(
            !cancelled
                .platform_output
                .accesskit_update
                .unwrap()
                .nodes
                .iter()
                .any(|(_, node)| node.label() == Some("Copy response")),
            "a queued cancellation cannot resurrect the retired stream before acknowledgement"
        );
    }

    #[test]
    fn same_request_same_length_link_rewrite_rejects_old_accessibility_click_but_copy_uses_latest_source()
     {
        let context = egui::Context::default();
        context.enable_accesskit();
        let creation = eframe::CreationContext::_new_kittest(context.clone());
        let mut app = Cueglint::new(
            &creation,
            Options {
                fixture: Some("overlay".into()),
                ..Default::default()
            },
        );
        let Response::State(mut initial, _) = app
            .worker
            .responses
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        else {
            panic!("fixture did not initialize")
        };
        let mut store = cueglint_core::Store::open_in_memory().unwrap();
        let session = store
            .start_session("[Test] Original link rewrite", None)
            .unwrap();
        let request = store
            .begin_request(&session.id, "Original link question")
            .unwrap();
        initial.selected = Some(store.session(&session.id).unwrap());
        app.apply_snapshot(initial, None, false);
        let old = "[unchanged label](https://example.test/old)";
        let new = "[unchanged label](https://example.test/new)";
        assert_eq!(old.len(), new.len());
        app.runtime.generation = Some(crate::runtime::GenerationStatus {
            session_id: session.id.clone(),
            request_id: request.id.clone(),
            state: "generating".into(),
            text: old.into(),
        });
        let ready = |app: &mut Cueglint| {
            app.synchronize_markdown();
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if let markdown::Preview::Formatted(_, revision) =
                    app.answer_previews
                        .get(&session.id, &request.id, request.context.revision)
                {
                    break revision;
                }
                app.answer_previews.poll();
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(2));
            }
        };
        let old_revision = ready(&mut app);
        let input = || egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(690.0, 500.0))),
            ..Default::default()
        };
        let click = |target| {
            egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target_tree: egui::accesskit::TreeId::ROOT,
                target_node: target,
                data: None,
            })
        };
        let first = context.run_ui(input(), |ui| app.conversation(ui));
        let tree = first.platform_output.accesskit_update.unwrap();
        let old_link = tree
            .nodes
            .iter()
            .find(|(_, node)| {
                node.role() == egui::accesskit::Role::Link
                    && node.label() == Some("unchanged label")
            })
            .unwrap()
            .0;
        let copy = tree
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some("Copy response"))
            .unwrap()
            .0;
        app.runtime.generation.as_mut().unwrap().text = new.into();
        let new_revision = ready(&mut app);
        assert!(new_revision > old_revision);
        let mut stale = input();
        stale.events.push(click(old_link));
        let rejected = context.run_ui(stale, |ui| app.conversation(ui));
        assert!(
            !rejected
                .platform_output
                .commands
                .iter()
                .any(|command| matches!(command, egui::OutputCommand::OpenUrl(_))),
            "an old link action cannot open either the old or rewritten target"
        );
        let tree = rejected.platform_output.accesskit_update.unwrap();
        let new_link = tree
            .nodes
            .iter()
            .find(|(_, node)| {
                node.role() == egui::accesskit::Role::Link
                    && node.label() == Some("unchanged label")
            })
            .unwrap()
            .0;
        assert_ne!(new_link, old_link);
        assert!(
            tree.nodes
                .iter()
                .any(|(id, node)| *id == copy && node.label() == Some("Copy response")),
            "whole-response copy intentionally keeps its latest-source identity"
        );
        let mut current = input();
        current.events.push(click(new_link));
        let opened = context.run_ui(current, |ui| app.conversation(ui));
        let urls: Vec<_> = opened
            .platform_output
            .commands
            .iter()
            .filter_map(|command| {
                if let egui::OutputCommand::OpenUrl(url) = command {
                    Some(url.url.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(urls, ["https://example.test/new"]);
        let mut current_copy = input();
        current_copy.events.push(click(copy));
        let copied = context.run_ui(current_copy, |ui| app.conversation(ui));
        assert!(
            copied.platform_output.commands.iter().any(
                |command| matches!(command, egui::OutputCommand::CopyText(text) if text == new)
            )
        );
    }

    #[test]
    fn blocked_screen_permission_exposes_the_recovery_path() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let creation = eframe::CreationContext::_new_kittest(context.clone());
        let mut app = Cueglint::new(
            &creation,
            Options {
                fixture: Some("overlay".into()),
                ..Default::default()
            },
        );
        app.runtime.screen_enabled = true;
        app.runtime.permissions.screen = PermissionStatus::Required;
        assert!(app.blocked_capture_permission().is_some());
        let output = context.run_ui(egui::RawInput::default(), |ui| {
            app.permission_card(
                ui,
                "Screen & system audio",
                PermissionStatus::Required,
                true,
            )
        });
        let tree = output.platform_output.accesskit_update.unwrap();
        let text = tree
            .nodes
            .iter()
            .filter_map(|(_, node)| node.label().or_else(|| node.value()))
            .collect::<Vec<_>>()
            .join(" ");
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            text.contains("System Settings → Privacy & Security → Screen & System Audio Recording"),
            "missing accessible recovery path: {text}"
        );
        assert!(
            tree.nodes
                .iter()
                .any(|(_, node)| node.label() == Some("Retry screen capture"))
        );
        app.runtime.permissions.screen = PermissionStatus::Granted;
        assert!(
            app.blocked_capture_permission().is_none(),
            "requested context and granted permission are distinct states"
        );
    }

    #[test]
    fn provider_selectors_close_on_accessibility_and_keyboard_activation_including_reselection() {
        for keyboard in [false, true] {
            for catalog in [false, true] {
                for reselect in [false, true] {
                    let context = egui::Context::default();
                    context.enable_accesskit();
                    let (mut app, _, _replies, commands) = question_app(&context);
                    let config = ProviderConfig {
                        provider: ProviderId::OpenAiCompatible,
                        model_id: "original-model-a".into(),
                        base_url: Some("http://localhost:11434/v1".into()),
                    };
                    app.provider_draft = Some(config.clone());
                    app.provider_key = "cueglint-synthetic-unsaved-key".into();
                    app.runtime.provider = Some(crate::runtime::ProviderStatus {
                        id: "original-provider-status".into(),
                        config: config.clone(),
                        credential: Some(CredentialStatus::Stored),
                        models: vec!["original-model-a".into(), "original-model-b".into()],
                        documentation_url: None,
                        state: "ready".into(),
                        message: None,
                    });
                    let render = |app: &mut Cueglint, events| {
                        context
                            .run_ui(
                                egui::RawInput {
                                    screen_rect: Some(Rect::from_min_size(
                                        Pos2::ZERO,
                                        vec2(860.0, 1200.0),
                                    )),
                                    events,
                                    ..Default::default()
                                },
                                |ui| app.provider_settings(ui),
                            )
                            .platform_output
                            .accesskit_update
                            .unwrap()
                    };
                    let action = |target_node, action| {
                        egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
                            action,
                            target_tree: egui::accesskit::TreeId::ROOT,
                            target_node,
                            data: None,
                        })
                    };
                    let tree = render(&mut app, vec![]);
                    let selector_value = if catalog {
                        "Choose a listed model"
                    } else {
                        config.provider.label()
                    };
                    let selector = tree
                        .nodes
                        .iter()
                        .find(|(_, node)| {
                            node.role() == egui::accesskit::Role::ComboBox
                                && node.value() == Some(selector_value)
                        })
                        .unwrap()
                        .0;
                    let _ = render(
                        &mut app,
                        vec![action(selector, egui::accesskit::Action::Click)],
                    );
                    assert!(egui::Popup::is_any_open(&context));
                    let tree = render(&mut app, vec![]);
                    let selection = match (catalog, reselect) {
                        (true, true) => "original-model-a",
                        (true, false) => "original-model-b",
                        (false, true) => ProviderId::OpenAiCompatible.label(),
                        (false, false) => ProviderId::OpenAi.label(),
                    };
                    let item = tree
                        .nodes
                        .iter()
                        .find(|(_, node)| {
                            node.role() == egui::accesskit::Role::Button
                                && node.label() == Some(selection)
                        })
                        .unwrap()
                        .0;
                    if keyboard {
                        let _ =
                            render(&mut app, vec![action(item, egui::accesskit::Action::Focus)]);
                        let _ = render(&mut app, vec![enter_event()]);
                    } else {
                        let _ =
                            render(&mut app, vec![action(item, egui::accesskit::Action::Click)]);
                    }
                    assert!(
                        !egui::Popup::is_any_open(&context),
                        "selector stayed open: keyboard={keyboard}, catalog={catalog}, reselect={reselect}"
                    );
                    let mut expected = config;
                    if catalog {
                        expected.model_id = selection.into();
                    } else if !reselect {
                        expected.provider = ProviderId::OpenAi;
                        expected.model_id.clear();
                        expected.base_url = None;
                    }
                    assert_eq!(app.provider_draft, Some(expected));
                    assert_eq!(app.provider_key.is_empty(), !catalog && !reselect);
                    assert!(commands.try_recv().is_err());
                    assert!(app.runtime.answer_provider.provider.is_local());
                }
            }
        }
    }

    #[test]
    fn provider_password_masks_accessibility_and_copy_then_clears_only_on_storage_acknowledgement()
    {
        const KEY: &str = "cueglint-synthetic-password-never-valid";
        let context = egui::Context::default();
        context.enable_accesskit();
        let (mut app, store, replies, commands) = question_app(&context);
        app.provider_draft = Some(ProviderConfig {
            provider: ProviderId::OpenAi,
            model_id: "original-model".into(),
            base_url: None,
        });
        let input = || egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(860.0, 1000.0))),
            ..Default::default()
        };
        let _ = context.run_ui(input(), |ui| app.provider_settings(ui));
        context.memory_mut(|memory| memory.request_focus(Id::new("provider-api-key")));
        let mut paste = input();
        paste.events = vec![egui::Event::Paste(KEY.into()), egui::Event::Copy];
        let output = context.run_ui(paste, |ui| app.provider_settings(ui));
        assert_eq!(app.provider_key, KEY);
        assert!(
            !format!("{:?}", output.platform_output.commands).contains(KEY),
            "password Copy must not put a key on the clipboard"
        );
        let tree = output.platform_output.accesskit_update.unwrap();
        assert!(
            tree.nodes
                .iter()
                .any(|(_, node)| node.role() == egui::accesskit::Role::PasswordInput)
        );
        for (_, node) in &tree.nodes {
            assert!(
                !format!("{node:?}").contains(KEY),
                "password text must be absent from all accessibility node properties"
            );
        }
        let target = tree
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some("Save key"))
            .map(|(id, _)| *id)
            .unwrap();
        let mut click = input();
        click.events.push(egui::Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target_tree: egui::accesskit::TreeId::ROOT,
                target_node: target,
                data: None,
            },
        ));
        let output = context.run_ui(click, |ui| app.provider_settings(ui));
        assert_eq!(
            app.provider_key, KEY,
            "retain the read-only masked draft until storage succeeds"
        );
        assert!(app.provider_key_submission.is_some());
        assert!(
            !format!(
                "{:?}{:?}",
                output.platform_output.commands, output.platform_output.events
            )
            .contains(KEY)
        );
        let Command::Provider(id, config, ProviderAction::Save(secret)) =
            commands.try_recv().unwrap()
        else {
            panic!("native Save key must queue its typed actor command")
        };
        assert_eq!(config.provider, ProviderId::OpenAi);
        assert_eq!(format!("{secret:?}"), "Secret([REDACTED])");
        assert!(
            store
                .settings()
                .unwrap()
                .answer_provider
                .provider
                .is_local()
        );
        assert!(
            store
                .session(&app.selected().unwrap().id)
                .unwrap()
                .requests
                .is_empty()
        );
        app.runtime.provider = Some(crate::runtime::ProviderStatus {
            id,
            config,
            credential: Some(CredentialStatus::Stored),
            models: Vec::new(),
            documentation_url: None,
            state: "ready".into(),
            message: None,
        });
        replies
            .send(Response::Runtime(app.runtime.clone()))
            .unwrap();
        app.poll(&context);
        assert!(app.provider_key.is_empty() && app.provider_key_submission.is_none());
        assert!(
            egui::text_edit::TextEditState::load(&context, Id::new("provider-api-key")).is_none()
        );
    }

    #[test]
    fn provider_probe_messages_stay_with_the_tested_model_when_a_draft_changes() {
        let context = egui::Context::default();
        let (mut app, _, replies, _) = question_app(&context);
        let config = ProviderConfig {
            provider: ProviderId::OpenAi,
            model_id: "original-model-a".into(),
            base_url: None,
        };
        const SUCCESS: &str = "Original model A connection succeeded";
        app.provider_draft = Some(config.clone());
        app.runtime.provider = Some(crate::runtime::ProviderStatus {
            id: uuid::Uuid::new_v4().to_string(),
            config,
            credential: Some(CredentialStatus::Stored),
            models: vec!["original-model-a".into(), "original-model-b".into()],
            documentation_url: None,
            state: "ready".into(),
            message: Some(SUCCESS.into()),
        });
        let render = |app: &mut Cueglint| {
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(860.0, 1200.0))),
                    ..Default::default()
                },
                |ui| app.provider_settings(ui),
            );
            output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" ")
        };
        assert!(render(&mut app).contains(SUCCESS));
        app.provider_draft.as_mut().unwrap().model_id = "original-model-b".into();
        let visible = render(&mut app);
        assert!(
            !visible.contains(SUCCESS),
            "a completed model A probe must not imply that model B was tested"
        );
        assert!(visible.contains("Key stored in the OS credential store."));
        assert!(
            visible.contains("Choose a listed model"),
            "catalogs remain shared by provider and endpoint"
        );
        assert!(visible.contains("Provider charges may apply to questions and connection tests."));
        app.runtime.provider.as_mut().unwrap().state = "testing connection".into();
        app.runtime.provider.as_mut().unwrap().message = None;
        assert!(!render(&mut app).contains("testing connection"));
        let mut late = app.runtime.clone();
        late.provider.as_mut().unwrap().state = "ready".into();
        late.provider.as_mut().unwrap().message = Some(SUCCESS.into());
        replies.send(Response::Runtime(late)).unwrap();
        app.poll(&context);
        assert!(
            !render(&mut app).contains(SUCCESS),
            "a late model A completion must not relabel the model B draft"
        );
        app.runtime.provider.as_mut().unwrap().state = "error".into();
        app.runtime.provider.as_mut().unwrap().message =
            Some("Original model A connection failed".into());
        assert!(!render(&mut app).contains("Original model A connection failed"));
        app.provider_draft.as_mut().unwrap().model_id = "original-model-a".into();
        assert!(render(&mut app).contains("Original model A connection failed"));
    }

    #[test]
    fn failed_key_save_keeps_the_draft_and_old_acknowledgements_cannot_clear_a_new_one() {
        let context = egui::Context::default();
        let (mut app, _, replies, _) = question_app(&context);
        let config = ProviderConfig {
            provider: ProviderId::OpenAi,
            model_id: "original-model".into(),
            base_url: None,
        };
        let id = uuid::Uuid::new_v4().to_string();
        app.provider_draft = Some(config.clone());
        app.provider_key = "cueglint-synthetic-key-retry".into();
        app.provider_key_submission = Some((config.credential_ref().unwrap(), id.clone()));
        app.runtime.provider = Some(crate::runtime::ProviderStatus {
            id: uuid::Uuid::new_v4().to_string(),
            config: config.clone(),
            credential: Some(CredentialStatus::Stored),
            models: Vec::new(),
            documentation_url: None,
            state: "ready".into(),
            message: None,
        });
        app.reconcile_provider_key(&context);
        assert!(!app.provider_key.is_empty() && app.provider_key_submission.is_some());
        let old_queued_id = app.runtime.provider.as_ref().unwrap().id.clone();
        app.pending = 1;
        app.acknowledgement_kinds
            .push_back(Acknowledgement::ProviderKey(
                config.credential_ref().unwrap(),
                old_queued_id,
            ));
        replies
            .send(Response::Error(
                "Original queued key operation failed".into(),
            ))
            .unwrap();
        app.poll(&context);
        assert_eq!(app.provider_key, "cueglint-synthetic-key-retry");
        assert!(
            app.provider_key_submission.is_some(),
            "an old queue rejection must not unlock or clear the new key submission"
        );
        let mut wrong_scope = config.clone();
        wrong_scope.provider = ProviderId::Anthropic;
        app.pending = 1;
        app.acknowledgement_kinds
            .push_back(Acknowledgement::ProviderKey(
                wrong_scope.credential_ref().unwrap(),
                id.clone(),
            ));
        replies
            .send(Response::Error(
                "Original wrong-scope acknowledgement".into(),
            ))
            .unwrap();
        app.poll(&context);
        assert!(
            app.provider_key_submission.is_some(),
            "a matching UUID still requires the matching credential scope"
        );
        app.runtime.provider.as_mut().unwrap().id = id.clone();
        app.runtime.provider.as_mut().unwrap().config = wrong_scope;
        app.reconcile_provider_key(&context);
        assert!(!app.provider_key.is_empty() && app.provider_key_submission.is_some());
        app.runtime.provider.as_mut().unwrap().config = config.clone();
        app.runtime.provider.as_mut().unwrap().id = id;
        app.runtime.provider.as_mut().unwrap().state = "error".into();
        app.reconcile_provider_key(&context);
        assert_eq!(app.provider_key, "cueglint-synthetic-key-retry");
        assert!(app.provider_key_submission.is_none());
        app.provider_key_submission = Some((
            config.credential_ref().unwrap(),
            uuid::Uuid::new_v4().to_string(),
        ));
        app.provider_draft.as_mut().unwrap().provider = ProviderId::Anthropic;
        app.reconcile_provider_key(&context);
        assert!(app.provider_key.is_empty() && app.provider_key_submission.is_none());
    }

    #[test]
    fn native_text_input_reaches_the_real_worker_provider_and_saved_conversation_without_local_weights()
     {
        use crate::runtime::provider_tests::{fixture_server, sse_answer};
        assert!(
            std::env::var_os("CUEGLINT_MODEL_DIR").is_none(),
            "BLOCKED: clear CUEGLINT_MODEL_DIR for this isolated empty-model test; user model caches must not be scanned"
        );
        let (endpoint, requests, server) =
            fixture_server(vec![(200, sse_answer("original-native-model"))]);
        let directory =
            std::env::temp_dir().join(format!("cueglint-native-provider-{}", uuid::Uuid::new_v4()));
        let database = directory.join("state.sqlite3");
        let session = {
            let mut store = cueglint_core::Store::open(&database).unwrap();
            let mut settings = store.settings().unwrap();
            settings.answer_provider = ProviderConfig {
                provider: ProviderId::OpenAiCompatible,
                model_id: "original-native-model".into(),
                base_url: Some(endpoint),
            };
            settings.microphone_enabled = false;
            settings.system_audio_enabled = false;
            settings.model_directory =
                Some(directory.join("empty-models").to_str().unwrap().into());
            store.update_settings(settings).unwrap();
            let session = store
                .start_session("[Fixture] Native provider path", None)
                .unwrap()
                .id;
            store
                .update_notes(&session, "Original notes remain editable")
                .unwrap();
            store.stop_session(&session).unwrap();
            session
        };
        let context = egui::Context::default();
        context.enable_accesskit();
        let creation = eframe::CreationContext::_new_kittest(context.clone());
        let mut app = Cueglint::new(
            &creation,
            Options {
                database: Some(database),
                ..Default::default()
            },
        );
        let Response::State(initial, _) = app
            .worker
            .responses
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        else {
            panic!("native worker did not initialize")
        };
        app.pending = 0;
        app.acknowledgement_kinds.clear();
        app.apply_snapshot(initial, None, false);
        assert!(requests.try_recv().is_err());
        assert!(app.send(Command::Open(session.clone())));
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.pending > 0 && Instant::now() < deadline {
            app.poll(&context);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(app.answer_ready() && app.runtime.installed_profiles.is_empty());
        let input = || egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(690.0, 500.0))),
            ..Default::default()
        };
        context.memory_mut(|memory| memory.request_focus(Id::new("question-input")));
        let _ = context.run_ui(input(), |ui| app.question_input(ui, 0.0, false));
        let mut typed = input();
        typed.events = vec![
            egui::Event::Text("Original native input question".into()),
            enter_event(),
        ];
        let _ = context.run_ui(typed, |ui| app.question_input(ui, 0.0, false));
        assert!(app.question_submission.is_some());
        let deadline = Instant::now() + Duration::from_secs(8);
        while app
            .selected()
            .is_none_or(|session| session.suggestions.is_empty())
            && Instant::now() < deadline
        {
            app.poll(&context);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            !app.notice.as_ref().is_some_and(
                |(notice, _)| notice.contains("native credential store is unavailable")
            ),
            "BLOCKED: the OS credential store could not read the unique test-only endpoint reference; no unrelated credential lookup or mutation was attempted"
        );
        let saved = app.selected().unwrap();
        assert_eq!(saved.requests.len(), 1);
        assert_eq!(saved.requests[0].prompt, "Original native input question");
        assert_eq!(
            saved.requests[0].status,
            cueglint_core::RequestStatus::Completed
        );
        assert_eq!(saved.suggestions[0].text, "Original fixture answer");
        assert_eq!(saved.notes, "Original notes remain editable");
        assert!(app.question.is_empty() && app.question_submission.is_none());
        let sent = requests.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(
            sent.contains("Original native input question")
                && !sent.to_ascii_lowercase().contains("authorization:")
        );
        let output = context.run_ui(input(), |ui| app.conversation(ui));
        let visible = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");
        assert!(visible.contains("Original fixture answer"));
        server.join().unwrap();
        drop(app);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn native_use_provider_persists_selection_without_contacting_a_service_or_needing_weights() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let creation = eframe::CreationContext::_new_kittest(context.clone());
        let mut app = Cueglint::new(
            &creation,
            Options {
                fixture: Some("dashboard".into()),
                ..Default::default()
            },
        );
        let Response::State(initial, _) = app
            .worker
            .responses
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        else {
            panic!("fixture did not initialize")
        };
        app.pending = 0;
        app.acknowledgement_kinds.clear();
        let initial_sessions = initial.sessions.clone();
        app.apply_snapshot(initial, None, false);
        assert!(app.runtime.answer_provider.provider.is_local());
        app.provider_draft = Some(ProviderConfig {
            provider: ProviderId::OpenAi,
            model_id: "original-model".into(),
            base_url: None,
        });
        let output = context.run_ui(egui::RawInput::default(), |ui| app.provider_settings(ui));
        let tree = output.platform_output.accesskit_update.unwrap();
        let target = tree
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some("Use provider"))
            .map(|(id, _)| *id)
            .unwrap();
        let input = egui::RawInput {
            events: vec![egui::Event::AccessKitActionRequest(
                egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Click,
                    target_tree: egui::accesskit::TreeId::ROOT,
                    target_node: target,
                    data: None,
                },
            )],
            ..Default::default()
        };
        let _ = context.run_ui(input, |ui| app.provider_settings(ui));
        let Response::State(saved, _) = app
            .worker
            .responses
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        else {
            panic!("provider selection did not persist")
        };
        assert_eq!(saved.settings.answer_provider.provider, ProviderId::OpenAi);
        assert_eq!(saved.settings.answer_provider.model_id, "original-model");
        assert_eq!(saved.sessions, initial_sessions);
        assert!(saved.runtime.generation.is_none());
        assert!(
            saved.runtime.provider.is_none(),
            "fixture settings changes must not touch native credentials or connect"
        );
        app.pending = 0;
        app.acknowledgement_kinds.clear();
        app.fixture = None;
        app.apply_snapshot(saved, None, false);
        assert!(app.runtime.installed_profiles.is_empty());
        assert!(
            app.answer_ready(),
            "a deliberately selected remote answer does not require local weights"
        );
    }

    #[test]
    fn accessible_transcription_language_picker_is_locked_during_capture_and_saves_explicit_codes()
    {
        let context = egui::Context::default();
        context.enable_accesskit();
        let (mut app, mut store, _, commands) = question_app(&context);
        let render = |app: &mut Cueglint, events| {
            context
                .run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(860.0, 1200.0))),
                        events,
                        ..Default::default()
                    },
                    |ui| app.transcription_language_settings(ui),
                )
                .platform_output
                .accesskit_update
                .unwrap()
        };
        let action = |target_node| {
            egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target_tree: egui::accesskit::TreeId::ROOT,
                target_node,
                data: None,
            })
        };
        for state in ["initializing", "listening", "finalizing"] {
            app.runtime.capture = Some(crate::runtime::CaptureStatus {
                session_id: "original-capture".into(),
                state: state.into(),
                dropped_packets: 0,
                device_errors: 0,
            });
            let tree = render(&mut app, vec![]);
            let (target, picker) = tree
                .nodes
                .iter()
                .find(|(_, node)| {
                    node.role() == egui::accesskit::Role::ComboBox
                        && node.label() == Some("Transcription language")
                })
                .expect("accessible transcription language picker");
            assert!(picker.is_disabled());
            let _ = render(&mut app, vec![action(*target)]);
            assert!(commands.try_recv().is_err());
            assert!(!egui::Popup::is_any_open(&context));
        }
        app.runtime.capture = None;
        let tree = render(&mut app, vec![]);
        let (target, picker) = tree
            .nodes
            .iter()
            .find(|(_, node)| {
                node.role() == egui::accesskit::Role::ComboBox
                    && node.label() == Some("Transcription language")
            })
            .unwrap();
        assert!(!picker.is_disabled());
        assert_eq!(picker.value(), Some("English"));
        let _ = render(&mut app, vec![action(*target)]);
        let tree = render(&mut app, vec![]);
        let target = tree
            .nodes
            .iter()
            .find(|(_, node)| {
                node.role() == egui::accesskit::Role::Button
                    && node.label() == Some("Portuguese (pt)")
            })
            .expect("Portuguese input-language choice")
            .0;
        let _ = render(&mut app, vec![action(target)]);
        assert!(!egui::Popup::is_any_open(&context));
        let Command::Settings(settings) = commands.try_recv().unwrap() else {
            panic!("language choice must use the existing settings worker")
        };
        assert_eq!(settings.transcription_language, "pt");
        assert_eq!(settings.transcription_profile, None);
        assert_eq!(settings.transcription_backend, TranscriptionBackend::Cpu);
        let previous = store.settings().unwrap();
        assert_eq!(settings.answer_provider, previous.answer_provider);
        let sessions = store.sessions("").unwrap();
        store.update_settings(settings).unwrap();
        assert_eq!(store.settings().unwrap().transcription_language, "pt");
        assert_eq!(store.sessions("").unwrap(), sessions);
    }

    #[test]
    fn transcription_language_popup_keyboard_entry_scroll_and_focus_return() {
        fn frame(
            app: &mut Cueglint,
            context: &egui::Context,
            outside: &mut String,
            events: Vec<egui::Event>,
        ) -> (egui::accesskit::TreeUpdate, egui::Response) {
            let mut field = None;
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(860.0, 670.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    app.keyboard(ui.ctx());
                    field = Some(ui.text_edit_singleline(outside));
                    ui.add_space(24.0);
                    app.transcription_language_settings(ui);
                },
            );
            (
                output.platform_output.accesskit_update.unwrap(),
                field.unwrap(),
            )
        }
        fn target(
            tree: &egui::accesskit::TreeUpdate,
            role: egui::accesskit::Role,
            label: &str,
        ) -> (egui::accesskit::NodeId, Pos2) {
            let (id, node) = tree
                .nodes
                .iter()
                .find(|(_, node)| node.role() == role && node.label() == Some(label))
                .unwrap_or_else(|| panic!("missing {label}"));
            let rect = node.bounds().unwrap();
            (
                *id,
                pos2(
                    ((rect.x0 + rect.x1) / 2.0) as f32,
                    ((rect.y0 + rect.y1) / 2.0) as f32,
                ),
            )
        }
        fn click(
            app: &mut Cueglint,
            context: &egui::Context,
            outside: &mut String,
            position: Pos2,
        ) -> (egui::accesskit::TreeUpdate, egui::Response) {
            for pressed in [true, false] {
                let _ = frame(
                    app,
                    context,
                    outside,
                    vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            frame(app, context, outside, vec![])
        }
        let key = |key| {
            [true, false]
                .into_iter()
                .map(|pressed| egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                })
                .collect()
        };
        let focused = |context: &egui::Context| {
            context
                .memory(|memory| memory.focused())
                .map(|id| id.accesskit_id())
        };
        let context = egui::Context::default();
        context.enable_accesskit();
        let (mut app, mut store, replies, commands) = question_app(&context);
        let sessions = store.sessions("").unwrap();
        let mut outside = String::new();
        let _ = frame(&mut app, &context, &mut outside, vec![]);
        let (tree, _) = frame(&mut app, &context, &mut outside, vec![]);
        let (combo, position) = target(
            &tree,
            egui::accesskit::Role::ComboBox,
            "Transcription language",
        );
        let (tree, _) = click(&mut app, &context, &mut outside, position);
        assert!(egui::Popup::is_any_open(&context));
        let english = target(&tree, egui::accesskit::Role::Button, "English (en)").0;
        assert_eq!(
            focused(&context),
            Some(english),
            "pointer-opening must give the selected option keyboard focus"
        );
        assert_eq!(
            tree.nodes
                .iter()
                .filter(|(_, node)| {
                    node.role() == egui::accesskit::Role::Button
                        && cueglint_core::TRANSCRIPTION_LANGUAGES
                            .iter()
                            .any(|(code, name)| {
                                node.label() == Some(format!("{name} ({code})").as_str())
                            })
                })
                .count(),
            99
        );
        let _ = frame(&mut app, &context, &mut outside, key(egui::Key::ArrowDown));
        let (tree, _) = frame(&mut app, &context, &mut outside, vec![]);
        assert_eq!(
            focused(&context),
            Some(target(&tree, egui::accesskit::Role::Button, "Chinese (zh)").0),
            "navigation must not be reset to the selected option on later frames"
        );
        assert!(
            commands.try_recv().is_err(),
            "arrows move focus without committing"
        );
        let _ = frame(&mut app, &context, &mut outside, key(egui::Key::Enter));
        assert!(!egui::Popup::is_any_open(&context));
        let Command::Settings(settings) = commands.try_recv().unwrap() else {
            panic!("keyboard selection must use the existing settings command")
        };
        assert_eq!(settings.transcription_language, "zh");
        let (tree, _) = frame(&mut app, &context, &mut outside, vec![]);
        assert!(
            tree.nodes
                .iter()
                .find(|(id, _)| *id == combo)
                .unwrap()
                .1
                .is_disabled()
        );
        store.update_settings(settings).unwrap();
        replies
            .send(Response::State(question_snapshot(&app, &store), None))
            .unwrap();
        app.poll(&context);
        let _ = frame(&mut app, &context, &mut outside, vec![]);
        assert_eq!(
            focused(&context),
            Some(combo),
            "commit returns focus after its acknowledgement"
        );
        let _ = frame(&mut app, &context, &mut outside, key(egui::Key::Enter));
        assert!(
            egui::Popup::is_any_open(&context),
            "the focused combo opens by keyboard"
        );
        let (tree, _) = frame(&mut app, &context, &mut outside, vec![]);
        assert_eq!(
            focused(&context),
            Some(target(&tree, egui::accesskit::Role::Button, "Chinese (zh)").0)
        );
        let _ = frame(&mut app, &context, &mut outside, key(egui::Key::Escape));
        assert!(!egui::Popup::is_any_open(&context));
        assert_eq!(
            focused(&context),
            Some(combo),
            "Escape returns focus without a setting change"
        );
        assert!(commands.try_recv().is_err());

        let languages = cueglint_core::TRANSCRIPTION_LANGUAGES;
        let (last_code, last_name) = languages[languages.len() - 1];
        let mut settings = store.settings().unwrap();
        settings.transcription_language = last_code.into();
        store.update_settings(settings).unwrap();
        replies
            .send(Response::State(question_snapshot(&app, &store), None))
            .unwrap();
        app.poll(&context);
        let _ = frame(&mut app, &context, &mut outside, vec![]);
        let _ = frame(&mut app, &context, &mut outside, key(egui::Key::Space));
        assert!(egui::Popup::is_any_open(&context));
        let mut settled = frame(&mut app, &context, &mut outside, vec![]);
        for _ in 0..20 {
            settled = frame(&mut app, &context, &mut outside, vec![]);
        }
        let selected = target(
            &settled.0,
            egui::accesskit::Role::Button,
            &format!("{last_name} ({last_code})"),
        )
        .0;
        assert_eq!(focused(&context), Some(selected));
        let response = context
            .read_response(context.memory(|m| m.focused()).unwrap())
            .unwrap();
        assert!(response.interact_rect.is_positive());
        assert!(
            (response.interact_rect.height() - response.rect.height()).abs() < 1.0,
            "the selected late option must scroll fully into view"
        );
        let outside_id = settled.1.id;
        let position = settled.1.rect.center();
        let _ = click(&mut app, &context, &mut outside, position);
        assert!(!egui::Popup::is_any_open(&context));
        assert_eq!(
            context.memory(|m| m.focused()),
            Some(outside_id),
            "closing by an outside click must preserve that field's focus"
        );
        assert!(commands.try_recv().is_err());

        let (tree, _) = frame(&mut app, &context, &mut outside, vec![]);
        let position = target(
            &tree,
            egui::accesskit::Role::ComboBox,
            "Transcription language",
        )
        .1;
        let _ = click(&mut app, &context, &mut outside, position);
        for _ in 0..20 {
            let _ = frame(&mut app, &context, &mut outside, vec![]);
        }
        let _ = frame(&mut app, &context, &mut outside, key(egui::Key::ArrowUp));
        let _ = frame(&mut app, &context, &mut outside, key(egui::Key::Enter));
        let Command::Settings(settings) = commands.try_recv().unwrap() else {
            panic!("late-option keyboard selection must send settings")
        };
        assert_eq!(
            settings.transcription_language,
            languages[languages.len() - 2].0
        );
        let (_, field) = frame(&mut app, &context, &mut outside, vec![]);
        let _ = click(&mut app, &context, &mut outside, field.rect.center());
        store.update_settings(settings).unwrap();
        replies
            .send(Response::State(question_snapshot(&app, &store), None))
            .unwrap();
        app.poll(&context);
        let _ = frame(&mut app, &context, &mut outside, vec![]);
        assert_eq!(
            context.memory(|m| m.focused()),
            Some(outside_id),
            "a delayed acknowledgement must not reclaim focus from another field"
        );
        assert_eq!(store.sessions("").unwrap(), sessions);
        assert!(commands.try_recv().is_err());
    }

    #[test]
    fn accessible_cpu_transcription_control_recovers_a_saved_metal_choice() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let (mut app, mut store, replies, commands) = question_app(&context);
        let mut settings = store.settings().unwrap();
        settings.transcription_backend = TranscriptionBackend::Metal;
        store.update_settings(settings).unwrap();
        app.runtime.transcription_backend = TranscriptionBackend::Metal;
        replies
            .send(Response::State(question_snapshot(&app, &store), None))
            .unwrap();
        app.poll(&context);
        let output = context.run_ui(egui::RawInput::default(), |ui| app.general(ui));
        let tree = output.platform_output.accesskit_update.unwrap();
        let (target, _) = tree
            .nodes
            .iter()
            .find(|(_, node)| {
                node.role() == egui::accesskit::Role::RadioButton
                    && node.label() == Some("CPU transcription")
            })
            .expect("accessible CPU transcription choice");
        let (_, metal) = tree
            .nodes
            .iter()
            .find(|(_, node)| {
                node.role() == egui::accesskit::Role::RadioButton
                    && node.label() == Some("Metal transcription")
            })
            .expect("saved Metal transcription choice remains visible");
        assert_eq!(
            metal.is_disabled(),
            !asr::backend_compiled(TranscriptionBackend::Metal)
        );
        let _ = context.run_ui(
            egui::RawInput {
                events: vec![egui::Event::AccessKitActionRequest(
                    egui::accesskit::ActionRequest {
                        action: egui::accesskit::Action::Click,
                        target_tree: egui::accesskit::TreeId::ROOT,
                        target_node: *target,
                        data: None,
                    },
                )],
                ..Default::default()
            },
            |ui| app.general(ui),
        );
        let Command::Settings(settings) = commands.try_recv().unwrap() else {
            panic!("CPU selection must use the settings worker command")
        };
        assert_eq!(settings.transcription_backend, TranscriptionBackend::Cpu);
        assert_eq!(settings.generation_profile, None);
        store.update_settings(settings).unwrap();
        assert_eq!(
            store.settings().unwrap().transcription_backend,
            TranscriptionBackend::Cpu
        );
    }

    #[test]
    fn accessible_model_selection_persists_the_chosen_profile() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let creation = eframe::CreationContext::_new_kittest(context.clone());
        let mut app = Cueglint::new(
            &creation,
            Options {
                fixture: Some("dashboard".into()),
                ..Default::default()
            },
        );
        let Response::State(initial, _) = app
            .worker
            .responses
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        else {
            panic!("fixture did not initialize")
        };
        app.acknowledgement_kinds.pop_front();
        app.pending = 0;
        app.apply_snapshot(initial, None, false);
        assert_eq!(app.generation_profile(), models::GENERATION_PROFILE);
        let output = context.run_ui(egui::RawInput::default(), |ui| app.general(ui));
        let tree = output.platform_output.accesskit_update.unwrap();
        let target = tree
            .nodes
            .iter()
            .find(|(_, node)| {
                node.role() == egui::accesskit::Role::RadioButton
                    && node.label().is_some_and(|label| label.contains("0.5B"))
            })
            .map(|(id, _)| *id)
            .expect("smaller answer-model radio");
        let request = egui::accesskit::ActionRequest {
            action: egui::accesskit::Action::Click,
            target_tree: egui::accesskit::TreeId::ROOT,
            target_node: target,
            data: None,
        };
        let input = egui::RawInput {
            events: vec![egui::Event::AccessKitActionRequest(request)],
            ..Default::default()
        };
        let _ = context.run_ui(input, |ui| app.general(ui));
        let Response::State(saved, _) = app
            .worker
            .responses
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        else {
            panic!("profile selection did not persist")
        };
        assert_eq!(
            saved.settings.generation_profile.as_deref(),
            Some(models::SMALL_GENERATION_PROFILE)
        );
        assert_eq!(saved.settings.transcription_profile, None);
    }

    #[test]
    fn model_file_controls_reject_actions_in_fixtures_and_during_active_work() {
        use crate::runtime::{CaptureStatus, GenerationStatus, ModelJob};
        let context = egui::Context::default();
        context.enable_accesskit();
        let creation = eframe::CreationContext::_new_kittest(context.clone());
        let mut app = Cueglint::new(
            &creation,
            Options {
                fixture: Some("dashboard".into()),
                ..Default::default()
            },
        );
        let Response::State(initial, _) = app
            .worker
            .responses
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        else {
            panic!("fixture did not initialize")
        };
        app.acknowledgement_kinds.pop_front();
        app.pending = 0;
        app.apply_snapshot(initial, None, false);
        app.model_import_path = "/tmp/cueglint-model-fixture-no-files-used".into();
        app.runtime.installed_profiles = vec![models::GENERATION_PROFILE.into()];
        app.runtime.incomplete_profiles = vec![models::SMALL_GENERATION_PROFILE.into()];
        for blocked_by in [
            "fixture",
            "capture",
            "generating",
            "capturing screen",
            "downloading",
            "importing",
            "removing",
            "pending",
        ] {
            // The worker stays fixture-backed even when testing a production UI state.
            app.fixture = (blocked_by == "fixture").then(|| "dashboard".into());
            app.pending = usize::from(blocked_by == "pending");
            app.runtime.capture = (blocked_by == "capture").then(|| CaptureStatus {
                session_id: "test-session".into(),
                state: "listening".into(),
                dropped_packets: 0,
                device_errors: 0,
            });
            app.runtime.generation =
                matches!(blocked_by, "generating" | "capturing screen").then(|| GenerationStatus {
                    session_id: "test-session".into(),
                    request_id: "test-request".into(),
                    state: blocked_by.into(),
                    text: String::new(),
                });
            app.runtime.model_job = matches!(blocked_by, "downloading" | "importing" | "removing")
                .then(|| ModelJob {
                    profile_id: models::TRANSCRIPTION_PROFILE.into(),
                    file: String::new(),
                    downloaded_bytes: 0,
                    total_bytes: 0,
                    state: blocked_by.into(),
                });
            let output = context.run_ui(egui::RawInput::default(), |ui| app.general(ui));
            let tree = output.platform_output.accesskit_update.unwrap();
            let actions: Vec<_> = tree
                .nodes
                .iter()
                .filter(|(_, node)| {
                    node.role() == egui::accesskit::Role::Button
                        && node.label().is_some_and(|label| {
                            label.starts_with("Import ")
                                || label.starts_with("Download ")
                                || label.starts_with("Remove local copy of ")
                                || label.starts_with("Remove local files for ")
                                || label.starts_with("Retry download for ")
                        })
                })
                .collect();
            assert!(
                actions.len() >= 3,
                "model actions must remain discoverable: {blocked_by}"
            );
            assert!(
                actions.iter().all(|(_, node)| node.is_disabled()),
                "unsafe enabled model action: {blocked_by}"
            );
            let target = actions
                .iter()
                .find(|(_, node)| node.label().unwrap().starts_with("Remove local copy of "))
                .map(|(id, _)| *id)
                .expect("installed model remove action");
            let pending = app.pending;
            let request = egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target_tree: egui::accesskit::TreeId::ROOT,
                target_node: target,
                data: None,
            };
            let _ = context.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::AccessKitActionRequest(request)],
                    ..Default::default()
                },
                |ui| app.general(ui),
            );
            assert_eq!(
                app.pending, pending,
                "disabled action queued a file operation: {blocked_by}"
            );
            assert_eq!(app.runtime.installed_profiles, [models::GENERATION_PROFILE]);
        }
        app.fixture = None;
        app.pending = 0;
        app.runtime.capture = None;
        app.runtime.generation = None;
        for state in ["ready", "removed", "error", "cancelled"] {
            app.runtime.model_job = Some(ModelJob {
                profile_id: models::TRANSCRIPTION_PROFILE.into(),
                file: String::new(),
                downloaded_bytes: 0,
                total_bytes: 0,
                state: state.into(),
            });
            let output = context.run_ui(egui::RawInput::default(), |ui| app.general(ui));
            let tree = output.platform_output.accesskit_update.unwrap();
            assert!(
                tree.nodes.iter().any(|(_, node)| node
                    .label()
                    .is_some_and(|label| label.starts_with("Import "))
                    && !node.is_disabled()),
                "terminal model job must release file controls: {state}"
            );
            for action in ["Retry download for ", "Remove local files for "] {
                assert_eq!(
                    tree.nodes
                        .iter()
                        .filter(|(_, node)| node
                            .label()
                            .is_some_and(|label| label.starts_with(action))
                            && !node.is_disabled())
                        .count(),
                    1,
                    "one incomplete profile should expose {action} after {state}",
                );
            }
        }
    }

    #[test]
    fn newer_notes_survive_an_in_flight_save_and_async_updates() {
        let context = egui::Context::default();
        let creation = eframe::CreationContext::_new_kittest(context);
        let mut app = Cueglint::new(
            &creation,
            Options {
                fixture: Some("session".into()),
                ..Default::default()
            },
        );
        let Response::State(initial, _) = app
            .worker
            .responses
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        else {
            panic!("fixture did not initialize")
        };
        app.apply_snapshot(initial, None, true);
        let state = app.state.as_ref().unwrap();
        let stale = Snapshot {
            draft_freshness: state.draft_freshness,
            accepted_freshness: state.accepted_freshness,
            sessions: state.sessions.clone(),
            selected: state.selected.clone(),
            modes: state.modes.clone(),
            knowledge: state.knowledge.clone(),
            settings: state.settings.clone(),
            runtime: state.runtime.clone(),
        };
        app.notes = "First edit".into();
        app.notes_dirty = true;
        app.save_notes();
        app.notes = "A newer edit while saving".into();
        app.apply_snapshot(stale, None, false);
        assert!(
            app.notes_saving,
            "background updates cannot acknowledge a save"
        );
        assert_eq!(app.notes, "A newer edit while saving");
        let Response::State(saved, _) = app
            .worker
            .responses
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        else {
            panic!("notes were not saved")
        };
        app.apply_snapshot(saved, None, true);
        assert!(
            app.notes_dirty,
            "the first save did not contain the newer edit"
        );
        app.save_notes();
        let Response::State(saved, _) = app
            .worker
            .responses
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        else {
            panic!("newer notes were not saved")
        };
        app.apply_snapshot(saved, None, true);
        assert!(!app.notes_dirty);
        assert_eq!(app.selected().unwrap().notes, "A newer edit while saving");
    }

    #[test]
    fn session_switch_prevents_typing_into_the_previous_notes() {
        let context = egui::Context::default();
        let creation = eframe::CreationContext::_new_kittest(context.clone());
        let mut app = Cueglint::new(
            &creation,
            Options {
                fixture: Some("session".into()),
                ..Default::default()
            },
        );
        let Response::State(initial, _) = app
            .worker
            .responses
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        else {
            panic!("fixture did not initialize")
        };
        app.acknowledgement_kinds.pop_front();
        app.apply_snapshot(initial, None, false);
        let previous = app.selected().unwrap().clone();
        let next = app
            .state
            .as_ref()
            .unwrap()
            .sessions
            .iter()
            .find(|session| session.id != previous.id)
            .unwrap()
            .id
            .clone();
        context.memory_mut(|memory| memory.request_focus(Id::new("session-notes")));
        let _ = context.run_ui(egui::RawInput::default(), |ui| app.notes_ui(ui, &previous));
        assert!(context.memory(|memory| memory.has_focus(Id::new("session-notes"))));
        app.send(Command::Open(next.clone()));
        let input = egui::RawInput {
            events: vec![egui::Event::Text("typing during navigation".into())],
            ..Default::default()
        };
        let _ = context.run_ui(input, |ui| app.notes_ui(ui, &previous));
        assert_eq!(
            app.notes, previous.notes,
            "the old editor must be read-only until navigation is acknowledged"
        );
        assert!(!app.notes_dirty);
        let Response::State(opened, _) = app
            .worker
            .responses
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        else {
            panic!("session did not open")
        };
        app.acknowledgement_kinds.pop_front();
        app.apply_snapshot(opened, None, false);
        assert_eq!(app.selected().unwrap().id, next);
        assert_eq!(app.notes, app.selected().unwrap().notes);
    }

    // The production UI uses a controlled protocol peer; no model files or inference are involved.
    pub(super) fn question_app(
        context: &egui::Context,
    ) -> (
        Cueglint,
        cueglint_core::Store,
        std::sync::mpsc::SyncSender<Response>,
        std::sync::mpsc::Receiver<Command>,
    ) {
        let creation = eframe::CreationContext::_new_kittest(context.clone());
        let mut app = Cueglint::new(
            &creation,
            Options {
                fixture: Some("session".into()),
                ..Default::default()
            },
        );
        let Response::State(_, _) = app
            .worker
            .responses
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        else {
            panic!("fixture did not initialize")
        };
        let mut store = cueglint_core::Store::open_in_memory().unwrap();
        let id = store
            .start_session("[Fixture] Question lifecycle", None)
            .unwrap()
            .id;
        store.stop_session(&id).unwrap();
        let (commands, command_rx) = std::sync::mpsc::sync_channel(16);
        let (replies, responses) = std::sync::mpsc::sync_channel(16);
        let (screenshot, _) = std::sync::mpsc::sync_channel(1);
        app.worker = Worker {
            commands,
            responses,
            screenshot,
        };
        app.fixture = None;
        app.pending = 0;
        app.acknowledgement_kinds.clear();
        app.runtime.installed_profiles = vec![models::GENERATION_PROFILE.into()];
        let mut state = question_snapshot(&app, &store);
        state.selected = Some(store.session(&id).unwrap());
        app.apply_snapshot(state, None, false);
        (app, store, replies, command_rx)
    }

    pub(super) fn question_snapshot(app: &Cueglint, store: &cueglint_core::Store) -> Snapshot {
        let sessions = store.sessions("").unwrap();
        Snapshot {
            draft_freshness: None,
            accepted_freshness: None,
            selected: sessions.first().cloned(),
            sessions,
            modes: store.modes().unwrap(),
            knowledge: store.knowledge().unwrap(),
            settings: store.settings().unwrap(),
            runtime: app.runtime.clone(),
        }
    }

    fn enter_event() -> egui::Event {
        egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn shortcut_event(key: egui::Key, modifiers: egui::Modifiers, repeat: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat,
            modifiers,
        }
    }

    #[test]
    fn escape_routing_preserves_popup_recording_and_global_overlay_behavior() {
        for (popup, recording, repeat, modifiers, forwarded, compact) in [
            (false, false, false, egui::Modifiers::NONE, false, true),
            (false, false, true, egui::Modifiers::NONE, false, false),
            (true, false, false, egui::Modifiers::NONE, true, false),
            (true, true, false, egui::Modifiers::NONE, false, false),
            (true, true, true, egui::Modifiers::NONE, false, false),
            (false, false, false, egui::Modifiers::COMMAND, true, false),
        ] {
            let context = egui::Context::default();
            let (mut app, _store, _replies, commands) = question_app(&context);
            if repeat {
                // egui derives repeat from a held key, not the supplied event flag.
                let _ = context.run_ui(
                    egui::RawInput {
                        events: vec![shortcut_event(egui::Key::Escape, modifiers, false)],
                        ..Default::default()
                    },
                    |_| {},
                );
            }
            app.overlay_panel = OverlayPanel::Chat;
            app.mode_menu = true;
            app.recording_shortcut = recording.then_some(Action::Ask);
            let mut escape_remaining = false;
            let _ = context.run_ui(
                egui::RawInput {
                    events: vec![shortcut_event(egui::Key::Escape, modifiers, repeat)],
                    ..Default::default()
                },
                |ui| {
                    if popup {
                        egui::Popup::open_id(ui.ctx(), Id::new("escape-routing-popup"));
                    }
                    app.keyboard(ui.ctx());
                    escape_remaining = ui.ctx().input(|input| input.key_pressed(egui::Key::Escape));
                },
            );
            assert_eq!(
                escape_remaining, forwarded,
                "popup={popup}, recording={recording}, repeat={repeat}, modifiers={modifiers:?}"
            );
            assert_eq!(app.overlay_panel == OverlayPanel::Compact, compact);
            assert_eq!(app.mode_menu, !compact);
            assert_eq!(
                app.recording_shortcut,
                (recording && repeat).then_some(Action::Ask),
                "shortcut recording must retain Escape priority, including repeat suppression"
            );
            assert!(commands.try_recv().is_err());
        }
    }

    #[test]
    fn session_shortcut_is_not_shadowed_and_holding_it_cannot_start_or_stop_twice() {
        let context = egui::Context::default();
        let (mut app, _store, _replies, commands) = question_app(&context);
        app.shortcuts.reconcile(&Default::default(), false, None);
        let _ = context.run_ui(
            egui::RawInput {
                events: vec![shortcut_event(
                    egui::Key::Backslash,
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    false,
                )],
                ..Default::default()
            },
            |ui| app.keyboard(ui.ctx()),
        );
        assert!(matches!(
            commands.try_recv().unwrap(),
            Command::Start { .. }
        ));
        assert!(
            app.overlay_open,
            "session start must show the assistant, not toggle its visibility"
        );
        app.pending = 0;
        let _ = context.run_ui(
            egui::RawInput {
                events: vec![shortcut_event(
                    egui::Key::Backslash,
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    true,
                )],
                ..Default::default()
            },
            |ui| app.keyboard(ui.ctx()),
        );
        assert!(
            commands.try_recv().is_err(),
            "a repeat cannot start another session"
        );
        let _ = context.run_ui(
            egui::RawInput {
                events: vec![shortcut_event(
                    egui::Key::Backslash,
                    egui::Modifiers::COMMAND | egui::Modifiers::ALT,
                    false,
                )],
                ..Default::default()
            },
            |ui| app.keyboard(ui.ctx()),
        );
        assert!(
            app.overlay_open,
            "extra modifiers must not match visibility"
        );
    }

    #[test]
    fn new_chat_waits_for_note_save_then_collapses_without_reusing_the_active_meeting() {
        let context = egui::Context::default();
        let (mut app, mut store, replies, commands) = question_app(&context);
        let meeting = store
            .start_session("Original meeting", Some("meeting"))
            .unwrap();
        let mut state = question_snapshot(&app, &store);
        state.selected = Some(meeting.clone());
        app.apply_snapshot(state, None, false);
        app.overlay_panel = OverlayPanel::Chat;
        app.notes = "Keep the original note.".into();
        app.notes_dirty = true;
        app.question = "Old draft".into();

        app.new_chat();
        let Command::Notes(id, notes) = commands.try_recv().unwrap() else {
            panic!("notes must save before switching conversations");
        };
        assert_eq!(id, meeting.id);
        assert!(commands.try_recv().is_err());
        assert_eq!(app.question, "Old draft");
        let mut saved = question_snapshot(&app, &store);
        saved.selected = Some(store.update_notes(&id, &notes).unwrap());
        saved.sessions = store.sessions("").unwrap();
        replies.send(Response::State(saved, None)).unwrap();
        app.poll(&context);
        assert!(matches!(commands.try_recv().unwrap(), Command::NewChat));

        let mut cleared = question_snapshot(&app, &store);
        cleared.selected = None;
        replies.send(Response::State(cleared, None)).unwrap();
        app.poll(&context);
        assert!(app.overlay_panel == OverlayPanel::Compact);
        assert!(app.selected().is_none());
        assert!(app.question.is_empty());
        assert_eq!(app.active_session().unwrap().id, meeting.id);
        assert_eq!(app.active_session().unwrap().notes, notes);

        app.question = "New independent question".into();
        app.ask();
        assert!(matches!(
            commands.try_recv().unwrap(),
            Command::Ask { session: None, new_chat_mode: Some(mode), .. } if mode == "meeting"
        ));
        assert!(
            app.question_submission
                .as_ref()
                .unwrap()
                .session_id
                .is_none()
        );
    }

    #[test]
    fn shortcut_record_clear_reset_and_duplicate_rejection_use_persisted_settings() {
        let context = egui::Context::default();
        let (mut app, mut store, replies, commands) = question_app(&context);
        app.shortcuts.reconcile(&Default::default(), true, None);
        app.recording_shortcut = Some(Action::Ask);
        let _ = context.run_ui(
            egui::RawInput {
                events: vec![shortcut_event(
                    egui::Key::R,
                    egui::Modifiers::COMMAND,
                    false,
                )],
                ..Default::default()
            },
            |ui| app.keyboard(ui.ctx()),
        );
        assert!(commands.try_recv().is_err());
        assert!(app.notice.as_ref().unwrap().0.contains("Already assigned"));
        assert_eq!(app.recording_shortcut, Some(Action::Ask));
        let _ = context.run_ui(
            egui::RawInput {
                events: vec![shortcut_event(
                    egui::Key::K,
                    egui::Modifiers::COMMAND | egui::Modifiers::ALT,
                    false,
                )],
                ..Default::default()
            },
            |ui| app.keyboard(ui.ctx()),
        );
        let Command::Settings(settings) = commands.try_recv().unwrap() else {
            panic!("recording must save settings")
        };
        assert_eq!(
            settings.shortcut_bindings["ask"].as_deref(),
            Some("CommandOrControl+Alt+K")
        );
        store.update_settings(settings).unwrap();
        replies
            .send(Response::State(question_snapshot(&app, &store), None))
            .unwrap();
        app.poll(&context);
        app.recording_shortcut = Some(Action::Ask);
        let _ = context.run_ui(
            egui::RawInput {
                events: vec![shortcut_event(
                    egui::Key::Backspace,
                    egui::Modifiers::COMMAND,
                    false,
                )],
                ..Default::default()
            },
            |ui| app.keyboard(ui.ctx()),
        );
        let Command::Settings(settings) = commands.try_recv().unwrap() else {
            panic!("modified Backspace must record a chord")
        };
        assert_eq!(
            settings.shortcut_bindings["ask"].as_deref(),
            Some("CommandOrControl+Backspace")
        );
        store.update_settings(settings).unwrap();
        replies
            .send(Response::State(question_snapshot(&app, &store), None))
            .unwrap();
        app.poll(&context);
        app.recording_shortcut = Some(Action::Ask);
        let _ = context.run_ui(
            egui::RawInput {
                events: vec![
                    egui::Event::Key {
                        key: egui::Key::Backspace,
                        physical_key: Some(egui::Key::Backspace),
                        pressed: false,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                    shortcut_event(
                        if cfg!(target_os = "macos") {
                            egui::Key::Backspace
                        } else {
                            egui::Key::Delete
                        },
                        egui::Modifiers::NONE,
                        false,
                    ),
                ],
                ..Default::default()
            },
            |ui| app.keyboard(ui.ctx()),
        );
        let Command::Settings(settings) = commands.try_recv().unwrap() else {
            panic!("Delete must clear the binding")
        };
        assert_eq!(settings.shortcut_bindings["ask"], None);
        store.update_settings(settings).unwrap();
        replies
            .send(Response::State(question_snapshot(&app, &store), None))
            .unwrap();
        app.poll(&context);
        app.save_shortcut(Action::Ask, None, true);
        let Command::Settings(settings) = commands.try_recv().unwrap() else {
            panic!("reset must restore the default")
        };
        assert!(!settings.shortcut_bindings.contains_key("ask"));
        assert_eq!(
            shortcuts::binding(&settings.shortcut_bindings, Action::Ask),
            None
        );
    }

    #[test]
    fn settings_writes_wait_for_acknowledgement_and_keep_saved_shortcuts() {
        let context = egui::Context::default();
        let (mut app, mut store, replies, commands) = question_app(&context);
        let mut stale = app.state.as_ref().unwrap().settings.clone();
        stale.microphone_enabled = !stale.microphone_enabled;
        app.save_shortcut(Action::Ask, Some("CommandOrControl+Alt+K".into()), false);
        let Command::Settings(binding) = commands.try_recv().unwrap() else {
            panic!("shortcut save")
        };
        assert!(!app.send(Command::Settings(stale)));
        assert!(
            commands.try_recv().is_err(),
            "a stale whole-settings write must not queue"
        );
        store.update_settings(binding).unwrap();
        replies
            .send(Response::State(question_snapshot(&app, &store), None))
            .unwrap();
        app.poll(&context);
        let mut current = app.state.as_ref().unwrap().settings.clone();
        current.microphone_enabled = !current.microphone_enabled;
        assert!(app.send(Command::Settings(current)));
        let Command::Settings(current) = commands.try_recv().unwrap() else {
            panic!("unrelated settings save")
        };
        store.update_settings(current).unwrap();
        assert_eq!(
            store.settings().unwrap().shortcut_bindings["ask"].as_deref(),
            Some("CommandOrControl+Alt+K")
        );
    }

    #[test]
    fn shortcut_edits_are_disabled_while_capture_settings_are_locked() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let (mut app, _store, _replies, commands) = question_app(&context);
        app.runtime.capture = Some(crate::runtime::CaptureStatus {
            session_id: app.selected().unwrap().id.clone(),
            state: "listening".into(),
            dropped_packets: 0,
            device_errors: 0,
        });
        let output = context.run_ui(egui::RawInput::default(), |ui| app.shortcuts(ui));
        let tree = output.platform_output.accesskit_update.unwrap();
        let recorders: Vec<_> = tree
            .nodes
            .iter()
            .filter(|(_, node)| {
                node.label()
                    .is_some_and(|label| label.starts_with("Record shortcut for "))
            })
            .collect();
        assert_eq!(recorders.len(), Action::ALL.len());
        assert!(recorders.iter().all(|(_, node)| node.is_disabled()));
        assert!(commands.try_recv().is_err());
    }

    #[test]
    fn visibility_recovery_keeps_settings_and_stop_reachable_when_global_registration_fails() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let (mut app, mut store, _replies, commands) = question_app(&context);
        let id = app.selected().unwrap().id.clone();
        store.resume_session(&id).unwrap();
        app.apply_snapshot(question_snapshot(&app, &store), None, false);
        for conflict in [false, true] {
            app.shortcuts = if conflict {
                shortcuts::tests::conflicted_manager(context.clone())
            } else {
                shortcuts::Manager::disabled(context.clone())
            };
            app.shortcuts.reconcile(&Default::default(), false, None);
            app.overlay_open = true;
            let output = context.run_ui(egui::RawInput::default(), |ui| {
                app.dispatch_shortcut(Action::Visibility, ui.ctx());
                app.main_window(ui);
            });
            assert!(!app.overlay_open && app.settings_visible);
            assert!(
                output.viewport_output[&ViewportId::ROOT]
                    .commands
                    .contains(&ViewportCommand::Visible(true))
            );
            assert!(
                commands.try_recv().is_err(),
                "hiding must preserve the active session"
            );
            let tree = output.platform_output.accesskit_update.unwrap();
            assert!(
                tree.nodes
                    .iter()
                    .any(|(_, node)| node.label() == Some("Stop session") && !node.is_disabled())
            );
        }
        let output = context.run_ui(egui::RawInput::default(), |ui| app.main_window(ui));
        let tree = output.platform_output.accesskit_update.unwrap();
        let (target, _) = tree
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some("Stop session"))
            .unwrap();
        let _ = context.run_ui(
            egui::RawInput {
                events: vec![egui::Event::AccessKitActionRequest(
                    egui::accesskit::ActionRequest {
                        action: egui::accesskit::Action::Click,
                        target_tree: egui::accesskit::TreeId::ROOT,
                        target_node: *target,
                        data: None,
                    },
                )],
                ..Default::default()
            },
            |ui| app.main_window(ui),
        );
        assert!(matches!(commands.try_recv().unwrap(), Command::Stop(stopped) if stopped == id));
    }

    #[test]
    fn registered_visibility_reopens_the_assistant_without_showing_settings_or_stopping_session() {
        let context = egui::Context::default();
        let (mut app, _store, _replies, commands) = question_app(&context);
        app.shortcuts = shortcuts::tests::registered_manager(context.clone());
        app.overlay_open = true;
        let output = context.run_ui(egui::RawInput::default(), |ui| {
            app.dispatch_shortcut(Action::Visibility, ui.ctx())
        });
        assert!(!app.overlay_open && !app.settings_visible);
        assert!(
            !output.viewport_output[&ViewportId::ROOT]
                .commands
                .contains(&ViewportCommand::Visible(true))
        );
        let mut frame = eframe::Frame::_new_kittest();
        let output = context.run_ui(egui::RawInput::default(), |ui| {
            app.dispatch_shortcut(Action::Visibility, ui.ctx());
            eframe::App::logic(&mut app, ui.ctx(), &mut frame);
            eframe::App::ui(&mut app, ui, &mut frame);
        });
        assert!(app.overlay_open);
        assert!(
            !output.viewport_output[&ViewportId::ROOT]
                .commands
                .contains(&ViewportCommand::Visible(true))
        );
        assert!(commands.try_recv().is_err());
    }

    #[test]
    fn hidden_root_close_keeps_the_close_guard_and_unsaved_notes() {
        let context = egui::Context::default();
        let (mut app, _store, _replies, commands) = question_app(&context);
        // Keep ROOT close events distinct from the child, as the native immediate renderer does.
        context.set_embed_viewports(false);
        egui::Context::set_immediate_viewport_renderer(|context, mut viewport| {
            let mut input = egui::RawInput {
                viewport_id: viewport.ids.this,
                ..Default::default()
            };
            input.viewports = context.input(|input| input.raw.viewports.clone());
            input.viewports.entry(viewport.ids.this).or_default();
            let _ = context.run_ui(input, |ui| (viewport.viewport_ui_cb)(ui));
        });
        app.overlay_open = false;
        app.settings_visible = false;
        app.focus_assistant = false;
        app.notes = "Unsaved notes must survive a hidden close".into();
        app.notes_dirty = true;
        app.notes_edited = Instant::now();
        let mut input = egui::RawInput::default();
        input
            .viewports
            .get_mut(&ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        let mut frame = eframe::Frame::_new_kittest();
        let output = context.run_ui(input, |ui| {
            eframe::App::logic(&mut app, ui.ctx(), &mut frame);
            eframe::App::ui(&mut app, ui, &mut frame);
        });
        assert!(
            output.viewport_output[&ViewportId::ROOT]
                .commands
                .contains(&ViewportCommand::CancelClose)
        );
        assert!(
            !output.viewport_output[&ViewportId::ROOT]
                .commands
                .contains(&ViewportCommand::Close)
        );
        assert!(app.overlay_open && app.notes_dirty);
        assert_eq!(app.notes, "Unsaved notes must survive a hidden close");
        assert!(commands.try_recv().is_err());
    }

    #[test]
    fn enter_submits_when_single_line_edit_releases_focus_without_discarding_draft() {
        let context = egui::Context::default();
        let (mut app, _store, _replies, commands) = question_app(&context);
        app.question = "A keyboard submission".into();
        context.memory_mut(|memory| memory.request_focus(Id::new("question-input")));
        let _ = context.run_ui(egui::RawInput::default(), |ui| {
            app.question_input(ui, 0.0, false)
        });
        let _ = context.run_ui(
            egui::RawInput {
                events: vec![enter_event()],
                ..Default::default()
            },
            |ui| app.question_input(ui, 0.0, false),
        );
        assert!(
            matches!(commands.try_recv().unwrap(), Command::Ask { prompt, .. } if prompt == "A keyboard submission")
        );
        assert_eq!(app.question, "A keyboard submission");
        assert!(app.question_submission.is_some());
        assert!(app.overlay_panel == OverlayPanel::Chat);
    }

    #[test]
    fn missing_models_and_active_model_jobs_disable_send_and_reject_enter() {
        use crate::runtime::ModelJob;
        let context = egui::Context::default();
        context.enable_accesskit();
        let (mut app, _store, _replies, commands) = question_app(&context);
        for blocked_by in ["missing", "downloading", "importing", "removing", "fixture"] {
            app.fixture = (blocked_by == "fixture").then(|| "session".into());
            app.runtime.installed_profiles = if blocked_by == "missing" {
                vec![]
            } else {
                vec![models::GENERATION_PROFILE.into()]
            };
            app.runtime.model_job = matches!(blocked_by, "downloading" | "importing" | "removing")
                .then(|| ModelJob {
                    profile_id: models::TRANSCRIPTION_PROFILE.into(),
                    file: String::new(),
                    downloaded_bytes: 0,
                    total_bytes: 0,
                    state: blocked_by.into(),
                });
            app.question = "Keep this question".into();
            context.memory_mut(|memory| memory.request_focus(Id::new("question-input")));
            let output = context.run_ui(egui::RawInput::default(), |ui| {
                app.question_input(ui, 0.0, false)
            });
            let tree = output.platform_output.accesskit_update.unwrap();
            let (id, send) = tree
                .nodes
                .iter()
                .find(|(_, node)| node.label() == Some("Send question"))
                .unwrap();
            assert!(send.is_disabled(), "unsafe Send button: {blocked_by}");
            let _ = context.run_ui(
                egui::RawInput {
                    events: vec![
                        enter_event(),
                        egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
                            action: egui::accesskit::Action::Click,
                            target_tree: egui::accesskit::TreeId::ROOT,
                            target_node: *id,
                            data: None,
                        }),
                    ],
                    ..Default::default()
                },
                |ui| app.question_input(ui, 0.0, false),
            );
            assert!(
                commands.try_recv().is_err(),
                "blocked Send/Enter queued work: {blocked_by}"
            );
            assert_eq!(app.question, "Keep this question");
            assert!(app.question_submission.is_none());
        }
    }

    #[test]
    fn mode_template_paste_preserves_every_byte_and_blocks_oversized_saves() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let (mut app, store, _, _) = question_app(&context);
        let limit = cueglint_core::MAX_NOTE_TEMPLATE_BYTES;
        let frame = |app: &mut Cueglint, events| {
            context
                .run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 1200.0))),
                        events,
                        ..Default::default()
                    },
                    |ui| app.modes(ui),
                )
                .platform_output
                .accesskit_update
                .unwrap()
        };
        for pasted in [
            "a".repeat(limit + 1),
            "é".repeat(limit / 2 + 1),
            "é".repeat(limit / 2),
        ] {
            let mode = store.mode("meeting").unwrap();
            let editor = Id::new(("mode-note-template", &mode.id));
            app.mode_draft = Some(mode);
            frame(&mut app, vec![]);
            context.memory_mut(|memory| memory.request_focus(editor));
            let tree = frame(&mut app, vec![egui::Event::Paste(pasted.clone())]);
            assert_eq!(app.mode_draft.as_ref().unwrap().note_template, pasted);
            for action in ["Save mode", "Use this mode"] {
                let (_, button) = tree
                    .nodes
                    .iter()
                    .find(|(_, node)| node.label() == Some(action))
                    .unwrap();
                assert_eq!(button.is_disabled(), pasted.len() > limit, "{action}");
            }
        }
    }

    #[test]
    fn mode_file_selection_is_saved_before_fresh_chat_activation() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let (mut app, mut store, replies, commands) = question_app(&context);
        let document = store
            .ingest_knowledge(
                "Original reference",
                "workshop.txt",
                "Workshop marker Orchid-731.",
            )
            .unwrap();
        let mode = store
            .upsert_mode(Mode::new("Workshop", "Use its reference files."))
            .unwrap();
        let mut snapshot = question_snapshot(&app, &store);
        snapshot.selected = None;
        app.apply_snapshot(snapshot, None, false);
        app.mode_draft = Some(mode.clone());
        let previous_mode = app.selected_mode.clone();
        let frame = |app: &mut Cueglint, events| {
            context
                .run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(630.0, 670.0))),
                        events,
                        ..Default::default()
                    },
                    |ui| app.modes(ui),
                )
                .platform_output
                .accesskit_update
                .unwrap()
        };
        let click = |app: &mut Cueglint, label| {
            let (id, _) = frame(app, vec![])
                .nodes
                .into_iter()
                .find(|(_, item)| item.label() == Some(label))
                .unwrap_or_else(|| panic!("missing mode control {label}"));
            let _ = frame(
                app,
                vec![egui::Event::AccessKitActionRequest(
                    egui::accesskit::ActionRequest {
                        action: egui::accesskit::Action::Click,
                        target_tree: egui::accesskit::TreeId::ROOT,
                        target_node: id,
                        data: None,
                    },
                )],
            );
        };
        click(&mut app, "Search all library files");
        assert_eq!(
            app.mode_draft
                .as_ref()
                .unwrap()
                .knowledge_document_ids
                .as_deref(),
            Some(&[] as &[String])
        );
        click(&mut app, "Original reference");
        click(&mut app, "Use this mode");
        let Command::Mode {
            mode: draft,
            activate: true,
        } = commands.try_recv().unwrap()
        else {
            panic!("Use this mode must save the attachment selection");
        };
        assert_eq!(
            draft.knowledge_document_ids,
            Some(vec![document.id.clone()])
        );
        assert_eq!(app.selected_mode, previous_mode);
        replies
            .send(Response::Error("Mode could not be saved".into()))
            .unwrap();
        app.poll(&context);
        assert_eq!(app.selected_mode, previous_mode);

        click(&mut app, "Use this mode");
        let Command::Mode {
            mode: draft,
            activate: true,
        } = commands.try_recv().unwrap()
        else {
            panic!("retry must retain the chosen attachment");
        };
        store.upsert_mode(draft).unwrap();
        let mut snapshot = question_snapshot(&app, &store);
        snapshot.selected = None;
        replies.send(Response::State(snapshot, None)).unwrap();
        app.poll(&context);
        app.question = "What is the marker?".into();
        app.ask();
        assert!(matches!(
            commands.try_recv().unwrap(),
            Command::Ask { session: None, new_chat_mode: Some(id), .. } if id == mode.id
        ));
    }

    #[test]
    fn queue_failure_and_error_acknowledgement_preserve_the_current_question() {
        let context = egui::Context::default();
        let (mut app, _store, replies, commands) = question_app(&context);
        for current_draft in ["Original question", "A newer draft"] {
            app.question = "Original question".into();
            app.ask();
            assert!(matches!(commands.try_recv().unwrap(), Command::Ask { .. }));
            app.question = current_draft.into();
            replies
                .send(Response::Error(
                    "Selected model disappeared before acceptance".into(),
                ))
                .unwrap();
            app.poll(&context);
            assert_eq!(app.question, current_draft);
            assert!(app.question_submission.is_none());
            assert_eq!(app.pending, 0);
        }
        drop(commands);
        app.question = "Queue is unavailable".into();
        app.ask();
        assert_eq!(app.question, "Queue is unavailable");
        assert!(app.question_submission.is_none());
        assert_eq!(app.pending, 0);
    }

    #[test]
    fn only_a_new_frozen_request_accepts_a_draft_and_screen_failure_keeps_it() {
        use crate::runtime::GenerationStatus;
        let context = egui::Context::default();
        let (mut app, mut store, replies, commands) = question_app(&context);
        let session = app.selected().unwrap().id.clone();
        let old = store.begin_request(&session, "Repeated question").unwrap();
        store.cancel_request(&session, &old.id).unwrap();
        app.apply_snapshot(question_snapshot(&app, &store), None, false);
        app.question = "Repeated question".into();
        app.ask();
        assert!(matches!(commands.try_recv().unwrap(), Command::Ask { .. }));
        app.runtime.generation = Some(GenerationStatus {
            session_id: session.clone(),
            request_id: old.id.clone(),
            state: "completed".into(),
            text: String::new(),
        });
        replies
            .send(Response::State(question_snapshot(&app, &store), None))
            .unwrap();
        app.poll(&context);
        assert_eq!(
            app.question, "Repeated question",
            "an identical earlier prompt is not acceptance"
        );
        assert!(app.question_submission.is_some());
        app.runtime.generation = Some(GenerationStatus {
            session_id: session.clone(),
            request_id: "screen-preflight".into(),
            state: "capturing screen".into(),
            text: String::new(),
        });
        replies
            .send(Response::Runtime(app.runtime.clone()))
            .unwrap();
        app.poll(&context);
        assert_eq!(app.question, "Repeated question");
        context.memory_mut(|memory| memory.request_focus(Id::new("question-input")));
        let _ = context.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Text("must not edit".into())],
                ..Default::default()
            },
            |ui| app.question_input(ui, 0.0, false),
        );
        assert_eq!(
            app.question, "Repeated question",
            "the editor is locked only while acceptance is pending"
        );
        app.runtime.generation.as_mut().unwrap().state = "error".into();
        app.question = "A newer draft after navigation".into();
        replies
            .send(Response::Update(
                question_snapshot(&app, &store),
                Some("Screen permission required".into()),
            ))
            .unwrap();
        app.poll(&context);
        assert_eq!(app.question, "A newer draft after navigation");
        assert!(app.question_submission.is_none());

        app.question = "Repeated question".into();
        app.ask();
        assert!(matches!(commands.try_recv().unwrap(), Command::Ask { .. }));
        let request = store.begin_request(&session, "Repeated question").unwrap();
        app.runtime.generation = Some(GenerationStatus {
            session_id: session,
            request_id: request.id.clone(),
            state: "generating".into(),
            text: String::new(),
        });
        replies
            .send(Response::State(question_snapshot(&app, &store), None))
            .unwrap();
        app.poll(&context);
        assert!(app.question.is_empty());
        assert!(app.question_submission.is_none());
        context.memory_mut(|memory| memory.request_focus(Id::new("question-input")));
        let _ = context.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Text("Repeated question".into())],
                ..Default::default()
            },
            |ui| app.question_input(ui, 0.0, false),
        );
        assert_eq!(
            app.question, "Repeated question",
            "a new draft is editable during streaming"
        );
        replies
            .send(Response::Update(question_snapshot(&app, &store), None))
            .unwrap();
        app.poll(&context);
        assert_eq!(
            app.question, "Repeated question",
            "a late snapshot must not clear the next identical draft"
        );
    }

    #[test]
    fn quit_waits_for_scoped_screen_worker_and_picker_retirement() {
        let context = egui::Context::default();
        let (mut app, _store, _replies, _commands) = question_app(&context);
        app.quit_started = true;
        for phase in [
            PickerPhase::Choosing,
            PickerPhase::Capturing,
            PickerPhase::Retiring,
        ] {
            app.runtime.window_picker = Some(crate::runtime::WindowPickerStatus {
                id: "scope-quit".into(),
                phase,
                question: false,
                deadline: Instant::now(),
                cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            });
            let output = context.run_ui(egui::RawInput::default(), |ui| app.request_quit(ui.ctx()));
            assert!(
                output.viewport_output[&ViewportId::ROOT]
                    .commands
                    .contains(&ViewportCommand::CancelClose)
            );
            let output = context.run_ui(egui::RawInput::default(), |ui| app.poll(ui.ctx()));
            assert!(
                !output.viewport_output[&ViewportId::ROOT]
                    .commands
                    .contains(&ViewportCommand::Close)
            );
        }
        app.runtime.window_picker = None;
        app.runtime.window_cleanup_unconfirmed = true;
        let output = context.run_ui(egui::RawInput::default(), |ui| app.poll(ui.ctx()));
        assert!(
            output.viewport_output[&ViewportId::ROOT]
                .commands
                .contains(&ViewportCommand::Close)
        );
        assert!(app.runtime.window_cleanup_unconfirmed);
    }

    #[test]
    fn quit_waits_for_document_import_cleanup_acknowledgement() {
        let context = egui::Context::default();
        let (mut app, store, replies, commands) = question_app(&context);
        let closes = |output: &egui::FullOutput| {
            output.viewport_output[&ViewportId::ROOT]
                .commands
                .contains(&ViewportCommand::Close)
        };
        app.import_path = "synthetic.pdf".into();
        app.import_knowledge();
        let Command::Import(import) = commands.try_recv().unwrap() else {
            panic!("document import was not queued")
        };
        replies
            .send(Response::State(question_snapshot(&app, &store), None))
            .unwrap();
        let _ = context.run_ui(egui::RawInput::default(), |ui| app.poll(ui.ctx()));
        assert_eq!(app.pending, 0);
        assert!(app.knowledge_import.is_some());

        let _ = context.run_ui(egui::RawInput::default(), |ui| app.request_quit(ui.ctx()));
        assert!(import.cancelled());
        assert!(matches!(commands.try_recv().unwrap(), Command::Cancel));
        assert!(matches!(
            commands.try_recv().unwrap(),
            Command::CancelInstall
        ));
        for _ in 0..2 {
            replies
                .send(Response::State(question_snapshot(&app, &store), None))
                .unwrap();
        }
        let output = context.run_ui(egui::RawInput::default(), |ui| app.poll(ui.ctx()));
        assert_eq!(app.pending, 0);
        assert!(app.knowledge_import.is_some());
        assert!(
            !closes(&output),
            "ordinary cancellation acknowledgements must not retire an import"
        );
        let output = context.run_ui(egui::RawInput::default(), |ui| app.request_quit(ui.ctx()));
        assert!(!closes(&output), "direct quit must await import cleanup");

        replies
            .send(Response::Import(
                "another-import".into(),
                Err("Document import cancelled".into()),
            ))
            .unwrap();
        let output = context.run_ui(egui::RawInput::default(), |ui| app.poll(ui.ctx()));
        assert!(app.knowledge_import.is_some());
        assert!(
            !closes(&output),
            "an unrelated import must not release quit"
        );
        replies
            .send(Response::Import(
                import.id,
                Err("Document import cancelled".into()),
            ))
            .unwrap();
        let output = context.run_ui(egui::RawInput::default(), |ui| app.poll(ui.ctx()));
        assert!(app.knowledge_import.is_none());
        assert!(closes(&output), "matching cleanup must release quit");
    }

    #[test]
    fn quit_waits_for_import_and_removal_cleanup_but_not_stalled_downloads() {
        use crate::runtime::ModelJob;
        for state in ["importing", "removing", "downloading"] {
            let context = egui::Context::default();
            let (mut app, store, replies, commands) = question_app(&context);
            app.runtime.model_job = Some(ModelJob {
                profile_id: models::GENERATION_PROFILE.into(),
                file: String::new(),
                downloaded_bytes: 0,
                total_bytes: 0,
                state: state.into(),
            });
            let _ = context.run_ui(egui::RawInput::default(), |ui| app.request_quit(ui.ctx()));
            assert!(matches!(commands.try_recv().unwrap(), Command::Cancel));
            assert!(matches!(
                commands.try_recv().unwrap(),
                Command::CancelInstall
            ));
            for _ in 0..2 {
                replies
                    .send(Response::State(question_snapshot(&app, &store), None))
                    .unwrap();
            }
            let output = context.run_ui(egui::RawInput::default(), |ui| app.poll(ui.ctx()));
            let closes = |output: &egui::FullOutput| {
                output.viewport_output[&ViewportId::ROOT]
                    .commands
                    .contains(&ViewportCommand::Close)
            };
            assert_eq!(
                closes(&output),
                state == "downloading",
                "quit policy: {state}"
            );
            app.runtime.model_job.as_mut().unwrap().state = "cancelled".into();
            replies
                .send(Response::Runtime(app.runtime.clone()))
                .unwrap();
            let output = context.run_ui(egui::RawInput::default(), |ui| app.poll(ui.ctx()));
            assert!(
                closes(&output),
                "cleanup completion must release quit: {state}"
            );
        }
    }
}
