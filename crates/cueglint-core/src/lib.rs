//! Offline session storage and request coordination. This crate never opens a network connection.
pub mod files;
mod history;
pub use history::{MeetingContext, MeetingExcerpt, MeetingExcerptSource};
pub mod knowledge;
mod language;
pub use language::{
    TRANSCRIPTION_LANGUAGES, transcription_language_name, validate_transcription_language,
};
pub mod provider;
pub use provider::{
    Citation, CredentialRef, CredentialStatus, HostedInferenceMetadata, MessageRole,
    ProviderConfig, ProviderId, ProviderMessage, TokenUsage,
};
mod notes;
pub use notes::*;

use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs::{File, OpenOptions},
    num::NonZeroU32,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const NOTES_SCHEMA_VERSION: u32 = 2;
const MODE_FILES_SCHEMA_VERSION: u32 = 3;
const HISTORY_CHAT_SCHEMA_VERSION: u32 = 4;
pub const SCHEMA_VERSION: u32 = HISTORY_CHAT_SCHEMA_VERSION;
pub const MAX_DOCUMENT_BYTES: usize = 2 * 1024 * 1024;
// shortcut: logical backups are bounded to 64 MiB, add streaming archives before larger histories.
pub const MAX_BACKUP_BYTES: usize = 64 * 1024 * 1024;
const MAX_TRANSCRIPT_BYTES: usize = 65_536;
const MAX_MODE_NAME_BYTES: usize = 200;
pub const CONTEXT_TEXT_BYTES: usize = 16 * 1024;
pub const CONTEXT_TRANSCRIPT_SEGMENTS: usize = 64;
pub const CONTEXT_CHAT_TURNS: usize = 8;
pub const SHORTCUT_ACTIONS: [&str; 11] = [
    "assistant_visibility",
    "new_chat",
    "settings",
    "session_toggle",
    "move_left",
    "move_right",
    "move_up",
    "move_down",
    "scroll_up",
    "scroll_down",
    "ask",
];

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Format a Unix millisecond timestamp as a UTC calendar date.
pub fn utc_date(millis: i64) -> String {
    let days = millis.div_euclid(86_400_000) + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = month_part + if month_part < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    format!("{year:04}-{month:02}-{day:02}")
}

fn id() -> String {
    Uuid::new_v4().to_string()
}

fn check_text(value: &str, label: &str, max: usize, allow_empty: bool) -> Result<()> {
    ensure!(
        allow_empty || !value.trim().is_empty(),
        "{label} cannot be empty"
    );
    ensure!(value.len() <= max, "{label} exceeds {max} bytes");
    ensure!(!value.contains('\0'), "{label} contains a NUL character");
    Ok(())
}

pub fn validate_prompt(prompt: &str) -> Result<()> {
    check_text(prompt, "prompt", MAX_TRANSCRIPT_BYTES, false)
}

/// Validate the portable serialized CPAL identity without resolving a native device.
pub fn validate_microphone_device_id(device: &str) -> Result<()> {
    check_text(device, "microphone device identifier", 4_096, false)?;
    let (host, identity) = device
        .split_once(':')
        .context("microphone device identifier must contain a host and device ID")?;
    ensure!(
        !host.is_empty()
            && host
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
            && !identity.trim().is_empty(),
        "microphone device identifier must contain a host and device ID"
    );
    Ok(())
}

/// Validate portable ordinary-key shortcuts without loading a platform input API.
pub fn validate_shortcut_binding(binding: &str) -> Result<()> {
    check_text(binding, "shortcut binding", 96, false)?;
    ensure!(
        binding
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'+'),
        "shortcut requires canonical ASCII key tokens"
    );
    let parts: Vec<_> = binding.split('+').collect();
    ensure!(
        parts.len() >= 2,
        "shortcut requires a command or control modifier and a key"
    );
    let key = parts[parts.len() - 1];
    let ordinary = key.len() == 1
        && key
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    let function = key
        .strip_prefix('F')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=20).contains(&n) && key == format!("F{n}"));
    ensure!(
        ordinary
            || function
            || [
                "Left",
                "Right",
                "Up",
                "Down",
                "Space",
                "Tab",
                "Enter",
                "Backspace",
                "Insert",
                "Home",
                "End",
                "PageUp",
                "PageDown",
                "Comma",
                "Minus",
                "Period",
                "Equals",
                "Semicolon",
                "Backslash",
                "Slash",
                "OpenBracket",
                "CloseBracket",
                "Backtick",
                "Quote",
            ]
            .contains(&key),
        "unsupported shortcut key: {key}"
    );
    let modifiers = &parts[..parts.len() - 1];
    let mut previous = None;
    for modifier in modifiers {
        let index = ["CommandOrControl", "Control", "Meta", "Alt", "Shift"]
            .iter()
            .position(|allowed| allowed == modifier)
            .context("unsupported shortcut modifier")?;
        ensure!(
            previous.is_none_or(|last| index > last),
            "shortcut modifiers must be unique and in canonical order"
        );
        previous = Some(index);
    }
    ensure!(
        modifiers
            .iter()
            .any(|m| ["CommandOrControl", "Control", "Meta"].contains(m)),
        "shortcut requires a command or control modifier"
    );
    ensure!(
        !modifiers.contains(&"CommandOrControl")
            || (!modifiers.contains(&"Control") && !modifiers.contains(&"Meta")),
        "CommandOrControl cannot be combined with Control or Meta"
    );
    Ok(())
}

fn check_id(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 100
            && value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)),
        "invalid identifier"
    );
    Ok(())
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Recording,
    Paused,
    Stopped,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Speaker {
    You,
    Other,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptSource {
    Microphone,
    SystemAudio,
    Manual,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TranscriptUpdate {
    pub id: String,
    pub speaker: Speaker,
    pub text: String,
    pub finalized: bool,
    pub source: TranscriptSource,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TranscriptSegment {
    pub id: String,
    pub speaker: Speaker,
    pub text: String,
    pub finalized: bool,
    pub source: TranscriptSource,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub audio: Option<AudioMetadata>,
}

impl TranscriptSegment {
    /// Position on the recording timeline, using arrival time for entries without audio metadata.
    pub fn session_offset_ns(&self, session_created_ms: i64) -> u64 {
        self.audio.as_ref().map_or_else(
            || {
                (self.created_at.saturating_sub(session_created_ms).max(0) as u64)
                    .saturating_mul(1_000_000)
            },
            |audio| audio.start_ns,
        )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AudioMetadata {
    pub start_ns: u64,
    pub end_ns: u64,
    pub sample_rate: u32,
    pub model_profile: String,
    pub model_revision: String,
    pub model_sha256: String,
    pub language: String,
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub frontend_revision: Option<String>,
    #[serde(default)]
    pub frontend_config_sha256: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Mode {
    pub id: String,
    pub name: String,
    pub instructions: String,
    pub builtin: bool,
    /// An absent list searches the library. An explicit empty list excludes all files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge_document_ids: Option<Vec<String>>,
}

impl Mode {
    pub fn new(name: impl Into<String>, instructions: impl Into<String>) -> Self {
        Self {
            id: id(),
            name: name.into(),
            instructions: instructions.into(),
            builtin: false,
            knowledge_document_ids: None,
        }
    }

    pub fn duplicate(&self) -> Self {
        const SUFFIX: &str = " (copy)";
        let name = self.name.trim();
        let prefix = &name[..name.floor_char_boundary(MAX_MODE_NAME_BYTES - SUFFIX.len())];
        Self {
            knowledge_document_ids: self.knowledge_document_ids.clone(),
            ..Self::new(
                format!("{}{SUFFIX}", prefix.trim_end()),
                self.instructions.clone(),
            )
        }
    }

    fn detach_document(&mut self, document_id: &str) -> bool {
        self.knowledge_document_ids.as_mut().is_some_and(|ids| {
            let count = ids.len();
            ids.retain(|id| id != document_id);
            ids.len() != count
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct KnowledgeDocument {
    pub id: String,
    pub title: String,
    pub source: String,
    pub text: String,
    pub created_at: i64,
}

/// A quotation from the stored text, extracted for DOCX. Byte offsets refer to that exact text.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct GroundingSnippet {
    pub document_id: String,
    pub title: String,
    pub source: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ContextSnapshot {
    pub revision: u64,
    pub mode: Option<Mode>,
    pub transcript: Vec<TranscriptSegment>,
    pub knowledge: Vec<GroundingSnippet>,
    #[serde(default)]
    pub screen: Option<ScreenContext>,
    #[serde(default)]
    pub chat: Vec<ChatTurn>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub past_meetings: Option<MeetingContext>,
    /// Zero identifies legacy snapshots whose original selection counts were not recorded.
    #[serde(default)]
    pub selection_version: u32,
    #[serde(default)]
    pub omitted_transcript_segments: usize,
    #[serde(default)]
    pub omitted_chat_turns: usize,
    #[serde(default)]
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ChatTurn {
    pub request_id: String,
    pub prompt: String,
    pub response: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ScreenContext {
    pub id: String,
    #[serde(flatten)]
    pub source: ScreenSource,
    pub captured_at: i64,
    pub width: u32,
    pub height: u32,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pixel_sha256: Option<String>,
    pub text: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(try_from = "ScreenSourceFields", into = "ScreenSourceFields")]
pub enum ScreenSource {
    Display(u32),
    Window(NonZeroU32),
}

#[derive(Deserialize, Serialize)]
struct ScreenSourceFields {
    #[serde(skip_serializing_if = "Option::is_none")]
    display_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    window_id: Option<NonZeroU32>,
}

impl TryFrom<ScreenSourceFields> for ScreenSource {
    type Error = &'static str;

    fn try_from(fields: ScreenSourceFields) -> std::result::Result<Self, Self::Error> {
        match (fields.display_id, fields.window_id) {
            (Some(id), None) => Ok(Self::Display(id)),
            (None, Some(id)) => Ok(Self::Window(id)),
            _ => Err("screen context requires exactly one display or window ID"),
        }
    }
}

impl From<ScreenSource> for ScreenSourceFields {
    fn from(source: ScreenSource) -> Self {
        match source {
            ScreenSource::Display(id) => Self {
                display_id: Some(id),
                window_id: None,
            },
            ScreenSource::Window(id) => Self {
                display_id: None,
                window_id: Some(id),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RequestStatus {
    Pending,
    Completed,
    Cancelled,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct GenerationRequest {
    pub id: String,
    pub session_id: String,
    pub prompt: String,
    pub context: ContextSnapshot,
    pub status: RequestStatus,
    pub created_at: i64,
    #[serde(default)]
    pub provider: ProviderConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Suggestion {
    pub id: String,
    pub request_id: String,
    pub context_revision: u64,
    pub text: String,
    pub sources: Vec<GroundingSnippet>,
    pub created_at: i64,
    #[serde(default)]
    pub inference: Option<InferenceMetadata>,
    #[serde(default)]
    pub hosted_inference: Option<HostedInferenceMetadata>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InferenceMetadata {
    pub model_profile: String,
    pub model_revision: String,
    pub model_sha256: String,
    pub tokenizer_sha256: String,
    pub backend: String,
    pub prompt_tokens: usize,
    pub generated_tokens: usize,
    pub max_new_tokens: usize,
    pub elapsed_ms: u64,
    pub finish_reason: String,
    pub omitted_transcript_segments: usize,
    #[serde(default)]
    pub omitted_chat_turns: usize,
    pub screen_truncated: bool,
    #[serde(default)]
    pub prompt_sha256: Option<String>,
    #[serde(default)]
    pub template_revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decoder_policy_revision: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub status: SessionStatus,
    pub muted: bool,
    pub mode_id: Option<String>,
    pub transcript: Vec<TranscriptSegment>,
    pub notes: String,
    #[serde(default)]
    pub notes_revision: u64,
    #[serde(default)]
    pub notes_job: Option<NotesJob>,
    #[serde(default)]
    pub generated_notes: Option<GeneratedNotes>,
    pub summary: Option<String>,
    pub suggestions: Vec<Suggestion>,
    pub requests: Vec<GenerationRequest>,
    pub context_revision: u64,
    pub active_request_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub saved_at: Option<i64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub history_chat: bool,
}

impl Session {
    /// Match a lowercase history query against saved text or a prefix of the UTC creation date.
    pub fn matches_history_query(&self, query: &str) -> bool {
        query.is_empty()
            || self.title.to_lowercase().contains(query)
            || self.notes.to_lowercase().contains(query)
            || self
                .summary
                .as_ref()
                .is_some_and(|summary| summary.to_lowercase().contains(query))
            || self
                .transcript
                .iter()
                .any(|segment| segment.text.to_lowercase().contains(query))
            || utc_date(self.created_at).starts_with(query)
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionBackend {
    #[default]
    Cpu,
    Metal,
}

impl std::str::FromStr for TranscriptionBackend {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "cpu" => Ok(Self::Cpu),
            "metal" => Ok(Self::Metal),
            _ => anyhow::bail!("transcription backend must be cpu or metal"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScreenInput {
    #[default]
    Display,
    Window,
}

impl ScreenInput {
    pub fn uses_display(&self) -> bool {
        *self == Self::Display
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Settings {
    pub microphone_enabled: bool,
    pub system_audio_enabled: bool,
    pub model_path: Option<String>,
    #[serde(default)]
    pub model_directory: Option<String>,
    #[serde(default)]
    pub screen_enabled: bool,
    #[serde(default, skip_serializing_if = "ScreenInput::uses_display")]
    pub screen_input: ScreenInput,
    #[serde(default)]
    pub display_id: Option<u32>,
    #[serde(default)]
    pub microphone_device_id: Option<String>,
    #[serde(default)]
    pub generation_profile: Option<String>,
    #[serde(default)]
    pub answer_provider: ProviderConfig,
    /// Unset keeps the original limit of the requesting desktop or CLI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hosted_answer_token_limit: Option<u32>,
    #[serde(default)]
    pub transcription_profile: Option<String>,
    #[serde(default)]
    pub transcription_backend: TranscriptionBackend,
    #[serde(default = "language::default_transcription_language")]
    pub transcription_language: String,
    /// Missing actions use their platform default; an explicit null disables the action.
    #[serde(default)]
    pub shortcut_bindings: BTreeMap<String, Option<String>>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            microphone_enabled: true,
            system_audio_enabled: false,
            model_path: None,
            model_directory: None,
            screen_enabled: false,
            screen_input: ScreenInput::default(),
            display_id: None,
            microphone_device_id: None,
            generation_profile: None,
            answer_provider: ProviderConfig::default(),
            hosted_answer_token_limit: None,
            transcription_profile: None,
            transcription_backend: TranscriptionBackend::default(),
            transcription_language: language::default_transcription_language(),
            shortcut_bindings: BTreeMap::new(),
        }
    }
}

/// Configured means a readable file is selected; it does not claim an inference engine exists.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ModelStatus {
    Missing,
    Configured { path: String },
    Unavailable { path: String },
}

#[derive(Clone, Copy, Debug)]
pub enum ExportFormat {
    Json,
    Markdown,
}

impl std::fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Json => "JSON",
            Self::Markdown => "Markdown",
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Backup {
    format: String,
    version: u32,
    exported_at: i64,
    sessions: Vec<Session>,
    modes: Vec<Mode>,
    knowledge: Vec<KnowledgeDocument>,
    settings: Settings,
}

impl Backup {
    fn data_version(&self) -> u32 {
        if self.sessions.iter().any(|session| session.history_chat) {
            HISTORY_CHAT_SCHEMA_VERSION
        } else if self
            .modes
            .iter()
            .any(|mode| mode.knowledge_document_ids.is_some())
            || self.sessions.iter().any(session_has_mode_file_scope)
        {
            MODE_FILES_SCHEMA_VERSION
        } else {
            NOTES_SCHEMA_VERSION
        }
    }
}

fn session_has_mode_file_scope(session: &Session) -> bool {
    session.requests.iter().any(|request| {
        request
            .context
            .mode
            .as_ref()
            .is_some_and(|mode| mode.knowledge_document_ids.is_some())
    })
}

struct DatabaseLock(File);

impl Drop for DatabaseLock {
    fn drop(&mut self) {
        // A duplicated or inherited handle must not extend this Store's ownership.
        let _ = self.0.unlock();
    }
}

pub struct Store {
    connection: Connection,
    _lock: Option<DatabaseLock>,
    partials: HashMap<(String, String), TranscriptSegment>,
}

#[cfg(test)]
mod lock_tests;

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            let mut builder = std::fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(parent).context("create data directory")?;
        }
        let canonical_path = if path.exists() {
            path.canonicalize()?
        } else {
            path.parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."))
                .canonicalize()?
                .join(path.file_name().context("database path needs a filename")?)
        };
        let path = canonical_path.as_path();
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut lock_path = path.as_os_str().to_os_string();
        lock_path.push(".lock");
        let lock = options
            .open(Path::new(&lock_path))
            .context("open database lock")?;
        lock.try_lock()
            .context("database is already open in another Cueglint process")?;
        let lock = DatabaseLock(lock);
        options.open(path).context("create private database file")?;
        let mut store = Self::initialize(Connection::open(path).context("open session database")?)?;
        store._lock = Some(lock);
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::initialize(Connection::open_in_memory()?)
    }

    pub fn default_path() -> Result<PathBuf> {
        if let Some(path) = std::env::var_os("CUEGLINT_DATA_DIR").filter(|path| !path.is_empty()) {
            return Ok(PathBuf::from(path).join("cueglint.sqlite3"));
        }
        #[cfg(target_os = "windows")]
        let base = PathBuf::from(
            std::env::var_os("LOCALAPPDATA")
                .context("LOCALAPPDATA is not set; use --db or CUEGLINT_DATA_DIR")?,
        )
        .join("Cueglint");
        #[cfg(target_os = "macos")]
        let base = PathBuf::from(
            std::env::var_os("HOME").context("HOME is not set; use --db or CUEGLINT_DATA_DIR")?,
        )
        .join("Library/Application Support/Cueglint");
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let base = match std::env::var_os("XDG_DATA_HOME").filter(|path| !path.is_empty()) {
            Some(path) => PathBuf::from(path).join("cueglint"),
            None => PathBuf::from(
                std::env::var_os("HOME")
                    .context("HOME is not set; use --db or CUEGLINT_DATA_DIR")?,
            )
            .join(".local/share/cueglint"),
        };
        Ok(base.join("cueglint.sqlite3"))
    }

    fn initialize(mut connection: Connection) -> Result<Self> {
        connection.busy_timeout(Duration::from_secs(5))?;
        let version: u32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        ensure!(
            version <= SCHEMA_VERSION,
            "database schema {version} is newer than supported schema {SCHEMA_VERSION}"
        );
        connection.execute_batch("PRAGMA secure_delete=ON;")?;
        let tx = connection.transaction()?;
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (id TEXT PRIMARY KEY, data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS modes (id TEXT PRIMARY KEY, data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS knowledge (id TEXT PRIMARY KEY, data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS settings (id TEXT PRIMARY KEY, data TEXT NOT NULL);",
        )?;
        if version < NOTES_SCHEMA_VERSION {
            tx.pragma_update(None, "user_version", NOTES_SCHEMA_VERSION)?;
        }
        for mode in default_modes() {
            tx.execute(
                "INSERT OR IGNORE INTO modes (id,data) VALUES (?1,?2)",
                params![mode.id, serde_json::to_string(&mode)?],
            )?;
        }
        // The process lock proves that no previous worker can own a persisted request.
        let stored: Vec<String> = {
            let mut stmt = tx.prepare("SELECT data FROM sessions")?;
            stmt.query_map([], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        for json in stored {
            let mut session: Session = serde_json::from_str(&json)?;
            notes::validate_session_notes(&session)?;
            let had_partials = session.transcript.iter().any(|segment| !segment.finalized);
            let notes_interrupted =
                notes::cancel_pending(&mut session, NotesCancelReason::Interrupted);
            if session.active_request_id.is_some() || had_partials || notes_interrupted {
                cancel_active(&mut session);
                session.transcript.retain(|segment| segment.finalized);
                tx.execute(
                    "UPDATE sessions SET data=?1 WHERE id=?2",
                    params![serde_json::to_string(&session)?, session.id],
                )?;
            }
        }
        tx.commit()?;
        Ok(Self {
            connection,
            _lock: None,
            partials: HashMap::new(),
        })
    }

    /// A capture host calls this at startup: persisted recording state cannot resume hardware.
    /// CLI editing keeps workflow state across commands and does not call this method.
    pub fn recover_interrupted(&mut self) -> Result<()> {
        self.partials.clear();
        let tx = self.connection.transaction()?;
        let sessions: Vec<String> = {
            let mut stmt = tx.prepare("SELECT data FROM sessions")?;
            stmt.query_map([], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        for json in sessions {
            let mut session: Session =
                serde_json::from_str(&json).context("read stored session")?;
            let notes_interrupted =
                notes::cancel_pending(&mut session, NotesCancelReason::Interrupted);
            if session.status == SessionStatus::Recording
                || session.active_request_id.is_some()
                || notes_interrupted
            {
                if session.status == SessionStatus::Recording {
                    session.status = SessionStatus::Paused;
                }
                cancel_active(&mut session);
                tx.execute(
                    "UPDATE sessions SET data=?1 WHERE id=?2",
                    params![serde_json::to_string(&session)?, session.id],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn session(&self, session_id: &str) -> Result<Session> {
        Ok(self.with_partials(self.stored_session(session_id)?))
    }

    fn stored_session(&self, session_id: &str) -> Result<Session> {
        let mut session: Session = self
            .get("sessions", session_id)?
            .with_context(|| format!("session {session_id} does not exist"))?;
        sort_transcript(&mut session);
        Ok(session)
    }

    fn with_partials(&self, mut session: Session) -> Session {
        for ((session_id, segment_id), partial) in &self.partials {
            if session_id == &session.id
                && !session
                    .transcript
                    .iter()
                    .any(|segment| &segment.id == segment_id)
            {
                session.transcript.push(partial.clone());
            }
        }
        sort_transcript(&mut session);
        session
    }

    /// Search by UTC creation date, title, transcript, summary or notes, most recently updated first.
    pub fn sessions(&self, query: &str) -> Result<Vec<Session>> {
        check_text(query, "search", 1_000, true)?;
        let query = query.to_lowercase();
        let mut sessions: Vec<Session> = self.all("sessions")?;
        sessions.retain(|session| session.matches_history_query(&query));
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(a.id.cmp(&b.id)));
        Ok(sessions)
    }

    /// Set the update time of a session in an isolated in-memory fixture.
    #[doc(hidden)]
    pub fn set_fixture_session_updated_at(
        &mut self,
        session_id: &str,
        updated_at: i64,
    ) -> Result<()> {
        ensure!(
            self._lock.is_none(),
            "fixture timestamps require an in-memory store"
        );
        let mut session = self.stored_session(session_id)?;
        ensure!(
            updated_at >= session.created_at,
            "fixture update precedes creation"
        );
        session.updated_at = updated_at;
        self.put("sessions", session_id, &session)
    }

    pub fn start_session(&mut self, title: &str, mode_id: Option<&str>) -> Result<Session> {
        self.create_session(title, mode_id, SessionStatus::Recording)
    }

    /// Store a conversation without reserving the recording slot.
    pub fn start_chat(&mut self, mode_id: Option<&str>) -> Result<Session> {
        self.create_session("New chat", mode_id, SessionStatus::Stopped)
    }

    fn create_session(
        &mut self,
        title: &str,
        mode_id: Option<&str>,
        status: SessionStatus,
    ) -> Result<Session> {
        let session = self.new_session(title, mode_id, status)?;
        self.put("sessions", &session.id, &session)?;
        Ok(session)
    }

    fn new_session(
        &self,
        title: &str,
        mode_id: Option<&str>,
        status: SessionStatus,
    ) -> Result<Session> {
        check_text(title, "title", 200, false)?;
        if let Some(mode_id) = mode_id {
            self.mode(mode_id)?;
        }
        let count: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?;
        ensure!(
            count < 10_000,
            "session history limit reached; export and delete older entries"
        );
        if status == SessionStatus::Recording {
            self.ensure_idle(None)?;
        }
        let timestamp = now();
        Ok(Session {
            id: id(),
            title: title.to_owned(),
            status,
            muted: false,
            mode_id: mode_id.map(str::to_owned),
            transcript: Vec::new(),
            notes: String::new(),
            notes_revision: 0,
            notes_job: None,
            generated_notes: None,
            summary: None,
            suggestions: Vec::new(),
            requests: Vec::new(),
            context_revision: 0,
            active_request_id: None,
            created_at: timestamp,
            updated_at: timestamp,
            saved_at: None,
            history_chat: false,
        })
    }

    pub fn pause_session(&mut self, session_id: &str) -> Result<Session> {
        self.change_session(session_id, |session| {
            ensure!(
                session.status == SessionStatus::Recording,
                "only a recording session can be paused"
            );
            session.status = SessionStatus::Paused;
            Ok(())
        })
    }

    pub fn resume_session(&mut self, session_id: &str) -> Result<Session> {
        self.ensure_idle(Some(session_id))?;
        self.change_session(session_id, |session| {
            ensure!(
                !session.history_chat,
                "start a new session to record a meeting during a History chat"
            );
            ensure!(
                session.status != SessionStatus::Recording,
                "the session is already recording"
            );
            session.status = SessionStatus::Recording;
            notes::cancel_pending(session, NotesCancelReason::Resumed);
            Ok(())
        })
    }

    pub fn mute_session(&mut self, session_id: &str, muted: bool) -> Result<Session> {
        self.change_session(session_id, |session| {
            ensure!(
                session.status != SessionStatus::Stopped,
                "the session is stopped"
            );
            session.muted = muted;
            Ok(())
        })
    }

    pub fn stop_session(&mut self, session_id: &str) -> Result<Session> {
        self.change_session(session_id, |session| {
            session.status = SessionStatus::Stopped;
            cancel_active(session);
            Ok(())
        })
    }

    /// Commit the capture stop and save marker together.
    pub fn finish_session(&mut self, session_id: &str) -> Result<Session> {
        self.change_session(session_id, |session| {
            session.status = SessionStatus::Stopped;
            session.saved_at = Some(now());
            cancel_active(session);
            Ok(())
        })
    }

    /// Every mutation is already durable. Save additionally marks this entry for history.
    pub fn save_session(&mut self, session_id: &str) -> Result<Session> {
        self.change_session(session_id, |session| {
            session.saved_at = Some(now());
            Ok(())
        })
    }

    pub fn rename_session(&mut self, session_id: &str, title: &str) -> Result<Session> {
        check_text(title, "title", 200, false)?;
        self.change_session(session_id, |session| {
            session.title = title.to_owned();
            Ok(())
        })
    }

    pub fn set_session_mode(&mut self, session_id: &str, mode_id: Option<&str>) -> Result<Session> {
        if let Some(mode_id) = mode_id {
            self.mode(mode_id)?;
        }
        self.change_session(session_id, |session| {
            session.mode_id = mode_id.map(str::to_owned);
            advance_context_revision(session)?;
            cancel_active(session);
            Ok(())
        })
    }

    pub fn update_notes(&mut self, session_id: &str, notes: &str) -> Result<Session> {
        check_text(notes, "notes", MAX_DOCUMENT_BYTES, true)?;
        self.change_session(session_id, |session| {
            notes::write_notes(session, notes)?;
            Ok(())
        })
    }

    pub fn update_summary(&mut self, session_id: &str, summary: &str) -> Result<Session> {
        check_text(summary, "summary", MAX_DOCUMENT_BYTES, true)?;
        self.change_session(session_id, |session| {
            session.summary = Some(summary.to_owned());
            Ok(())
        })
    }

    pub fn delete_session(&mut self, session_id: &str) -> Result<()> {
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        let source = self.stored_session(session_id)?;
        if source.saved_at.is_some() && !source.history_chat {
            history::withdraw_affected(&tx, session_id, None)?;
        }
        tx.execute("DELETE FROM sessions WHERE id=?1", [session_id])?;
        tx.commit()?;
        self.partials.retain(|(id, _), _| id != session_id);
        Ok(())
    }

    pub fn upsert_transcript(
        &mut self,
        session_id: &str,
        update: TranscriptUpdate,
    ) -> Result<Session> {
        self.upsert_transcript_inner(session_id, update, None)
    }

    pub fn upsert_transcript_with_audio(
        &mut self,
        session_id: &str,
        update: TranscriptUpdate,
        audio: AudioMetadata,
    ) -> Result<Session> {
        validate_audio(&audio)?;
        ensure!(
            update.source != TranscriptSource::Manual,
            "audio provenance requires a captured audio source"
        );
        self.upsert_transcript_inner(session_id, update, Some(audio))
    }

    fn upsert_transcript_inner(
        &mut self,
        session_id: &str,
        update: TranscriptUpdate,
        audio: Option<AudioMetadata>,
    ) -> Result<Session> {
        check_id(&update.id)?;
        check_text(
            &update.text,
            "transcript segment",
            MAX_TRANSCRIPT_BYTES,
            false,
        )?;
        let session = self.stored_session(session_id)?;
        ensure!(
            session.status == SessionStatus::Recording,
            "transcript capture requires a recording session"
        );
        ensure!(
            !(session.muted && update.source == TranscriptSource::Microphone),
            "microphone is muted"
        );
        if let Some(segment) = session
            .transcript
            .iter()
            .find(|segment| segment.id == update.id)
        {
            ensure!(
                segment.source == update.source && segment.speaker == update.speaker,
                "segment identity cannot change"
            );
            ensure!(
                update.finalized
                    && segment.text == update.text
                    && (audio.is_none() || segment.audio == audio),
                "a finalized transcript segment is immutable"
            );
            return self.session(session_id);
        }
        let key = (session_id.to_owned(), update.id.clone());
        let previous = self.partials.get(&key);
        if let Some(previous) = previous {
            ensure!(
                previous.source == update.source && previous.speaker == update.speaker,
                "segment identity cannot change"
            );
            ensure!(
                previous.audio.is_none() || audio.is_some(),
                "captured hypotheses require updated audio provenance"
            );
            if let (Some(before), Some(after)) = (&previous.audio, &audio) {
                let mut expected = before.clone();
                expected.end_ns = after.end_ns;
                ensure!(
                    after.end_ns >= before.end_ns && expected == *after,
                    "audio provenance cannot change between hypotheses"
                );
            }
        }
        ensure!(
            previous.is_some()
                || session.transcript.len()
                    + self
                        .partials
                        .keys()
                        .filter(|(id, _)| id == session_id)
                        .count()
                    < 10_000,
            "session transcript segment limit reached"
        );
        let timestamp = now();
        let segment = TranscriptSegment {
            id: update.id,
            speaker: update.speaker,
            text: update.text,
            finalized: update.finalized,
            source: update.source,
            created_at: previous.map_or(timestamp, |s| s.created_at),
            updated_at: timestamp,
            audio: audio.or_else(|| previous.and_then(|s| s.audio.clone())),
        };
        if !segment.finalized {
            self.partials.insert(key, segment);
            return self.session(session_id);
        }
        self.change_session(session_id, |session| {
            session.transcript.push(segment);
            sort_transcript(session);
            advance_context_revision(session)?;
            Ok(())
        })?;
        self.partials.remove(&key);
        self.session(session_id)
    }

    pub fn modes(&self) -> Result<Vec<Mode>> {
        let mut modes: Vec<Mode> = self.all("modes")?;
        modes.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(modes)
    }

    pub fn mode(&self, mode_id: &str) -> Result<Mode> {
        self.get("modes", mode_id)?
            .with_context(|| format!("mode {mode_id} does not exist"))
    }

    pub fn upsert_mode(&mut self, mut mode: Mode) -> Result<Mode> {
        validate_mode(&mode)?;
        let existing = self.get::<Mode>("modes", &mode.id)?;
        ensure!(
            existing.is_some() || self.modes()?.len() < 1_000,
            "mode limit reached"
        );
        if let Some(ids) = &mode.knowledge_document_ids {
            let mut exists = self
                .connection
                .prepare("SELECT EXISTS(SELECT 1 FROM knowledge WHERE id=?1)")?;
            for id in ids {
                ensure!(
                    exists.query_row([id], |row| row.get::<_, bool>(0))?,
                    "mode references a missing document"
                );
            }
        }
        mode.builtin = existing.is_some_and(|mode| mode.builtin);
        let tx = self.connection.transaction()?;
        tx.execute(
            "INSERT INTO modes(id,data) VALUES (?1,?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
            params![mode.id, serde_json::to_string(&mode)?],
        )?;
        if mode.knowledge_document_ids.is_some() {
            // Previous readers would treat a scoped mode as library-wide.
            raise_schema_version(&tx, MODE_FILES_SCHEMA_VERSION)?;
        }
        tx.commit()?;
        Ok(mode)
    }

    pub fn update_mode_text(
        &mut self,
        id: String,
        name: String,
        instructions: String,
    ) -> Result<Mode> {
        let knowledge_document_ids = self
            .get::<Mode>("modes", &id)?
            .and_then(|mode| mode.knowledge_document_ids);
        self.upsert_mode(Mode {
            id,
            name,
            instructions,
            builtin: false,
            knowledge_document_ids,
        })
    }

    pub fn delete_mode(&mut self, mode_id: &str) -> Result<()> {
        ensure!(
            !self.mode(mode_id)?.builtin,
            "built-in modes cannot be deleted"
        );
        let tx = self.connection.transaction()?;
        // Keep historical sessions readable without dangling live mode selections.
        let sessions: Vec<Session> = {
            let mut stmt = tx.prepare("SELECT data FROM sessions")?;
            let json = stmt
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            json.iter()
                .map(|s| serde_json::from_str(s).map_err(Into::into))
                .collect::<Result<_>>()?
        };
        for mut session in sessions
            .into_iter()
            .filter(|s| s.mode_id.as_deref() == Some(mode_id))
        {
            session.mode_id = None;
            advance_context_revision(&mut session)?;
            cancel_active(&mut session);
            tx.execute(
                "UPDATE sessions SET data=?1 WHERE id=?2",
                params![serde_json::to_string(&session)?, session.id],
            )?;
        }
        tx.execute("DELETE FROM modes WHERE id=?1", [mode_id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn knowledge(&self) -> Result<Vec<KnowledgeDocument>> {
        let mut docs: Vec<KnowledgeDocument> = self.all("knowledge")?;
        docs.sort_by(|a, b| a.title.cmp(&b.title));
        Ok(docs)
    }

    pub fn ingest_knowledge(
        &mut self,
        title: &str,
        source: &str,
        text: &str,
    ) -> Result<KnowledgeDocument> {
        let document = KnowledgeDocument {
            id: id(),
            title: title.to_owned(),
            source: source.to_owned(),
            text: text.to_owned(),
            created_at: now(),
        };
        validate_document(&document)?;
        let docs = self.knowledge()?;
        ensure!(
            docs.len() < 1_000
                && docs.iter().map(|d| d.text.len()).sum::<usize>() + text.len()
                    <= 20 * 1024 * 1024,
            "knowledge library limit reached"
        );
        self.put("knowledge", &document.id, &document)?;
        Ok(document)
    }

    pub fn delete_knowledge(&mut self, document_id: &str) -> Result<()> {
        let tx = self.connection.transaction()?;
        ensure!(
            tx.execute("DELETE FROM knowledge WHERE id=?1", [document_id])? > 0,
            "document does not exist"
        );
        let modes: Vec<String> = {
            let mut stmt = tx.prepare("SELECT data FROM modes")?;
            stmt.query_map([], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        for json in modes {
            let mut mode: Mode = serde_json::from_str(&json)?;
            if mode.detach_document(document_id) {
                tx.execute(
                    "UPDATE modes SET data=?1 WHERE id=?2",
                    params![serde_json::to_string(&mode)?, mode.id],
                )?;
            }
        }
        let stored: Vec<String> = {
            let mut stmt = tx.prepare("SELECT data FROM sessions")?;
            stmt.query_map([], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        for json in stored {
            let mut session: Session = serde_json::from_str(&json)?;
            let mut changed = false;
            for request in &mut session.requests {
                changed |= request
                    .context
                    .mode
                    .as_mut()
                    .is_some_and(|mode| mode.detach_document(document_id));
                let before = request.context.knowledge.len();
                request
                    .context
                    .knowledge
                    .retain(|source| source.document_id != document_id);
                if before != request.context.knowledge.len() {
                    changed = true;
                    if request.status == RequestStatus::Pending {
                        request.status = RequestStatus::Cancelled;
                        if session.active_request_id.as_deref() == Some(&request.id) {
                            session.active_request_id = None;
                        }
                    }
                }
            }
            for suggestion in &mut session.suggestions {
                let before = suggestion.sources.len();
                suggestion
                    .sources
                    .retain(|source| source.document_id != document_id);
                changed |= before != suggestion.sources.len();
            }
            if changed {
                advance_context_revision(&mut session)?;
                session.updated_at = now();
                tx.execute(
                    "UPDATE sessions SET data=?1 WHERE id=?2",
                    params![serde_json::to_string(&session)?, session.id],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Bounded lexical retrieval; results quote the stored original and never invent citations.
    pub fn search_knowledge(&self, query: &str, limit: usize) -> Result<Vec<GroundingSnippet>> {
        self.search_knowledge_files(query, limit, None)
    }

    fn search_knowledge_files(
        &self,
        query: &str,
        limit: usize,
        document_ids: Option<&[String]>,
    ) -> Result<Vec<GroundingSnippet>> {
        check_text(query, "knowledge search", MAX_TRANSCRIPT_BYTES, true)?;
        ensure!(
            limit <= 20,
            "at most 20 grounding snippets may be requested"
        );
        let words: BTreeSet<String> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.chars().count() >= 2)
            .map(str::to_lowercase)
            .collect();
        if words.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let documents = if let Some(ids) = document_ids {
            ids.iter()
                .map(|id| {
                    self.get::<KnowledgeDocument>("knowledge", id)?
                        .context("mode references a missing document")
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            self.knowledge()?
        };
        let mut matches = Vec::new();
        for document in documents {
            for (start_byte, end_byte) in text_ranges(&document.text, 1_200) {
                let text = &document.text[start_byte..end_byte];
                let lower = text.to_lowercase();
                let score = words
                    .iter()
                    .filter(|word| lower.contains(word.as_str()))
                    .count();
                if score > 0 {
                    matches.push((
                        score,
                        GroundingSnippet {
                            document_id: document.id.clone(),
                            title: document.title.clone(),
                            source: document.source.clone(),
                            start_byte,
                            end_byte,
                            text: text.to_owned(),
                        },
                    ));
                }
            }
        }
        matches.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then(a.1.document_id.cmp(&b.1.document_id))
                .then(a.1.start_byte.cmp(&b.1.start_byte))
        });
        Ok(matches
            .into_iter()
            .take(limit)
            .map(|(_, snippet)| snippet)
            .collect())
    }

    /// Freeze a request for a local worker. This method performs no inference.
    pub fn begin_request(&mut self, session_id: &str, prompt: &str) -> Result<GenerationRequest> {
        self.begin_request_with_screen(session_id, prompt, None)
    }

    pub fn begin_request_with_screen(
        &mut self,
        session_id: &str,
        prompt: &str,
        screen: Option<ScreenContext>,
    ) -> Result<GenerationRequest> {
        validate_prompt(prompt)?;
        if let Some(screen) = &screen {
            validate_screen(screen)?;
        }
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        let session = self.stored_session(session_id)?;
        let request = self.freeze_request(&tx, session, prompt, screen)?;
        tx.commit()?;
        Ok(request)
    }

    /// Insert a newly created conversation only once its first screen-backed request is valid.
    pub fn start_chat_with_screen(
        &mut self,
        mode_id: Option<&str>,
        prompt: &str,
        screen: ScreenContext,
    ) -> Result<GenerationRequest> {
        validate_prompt(prompt)?;
        validate_screen(&screen)?;
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        let session = self.new_session("New chat", mode_id, SessionStatus::Stopped)?;
        let request = self.freeze_request(&tx, session, prompt, Some(screen))?;
        tx.commit()?;
        Ok(request)
    }

    /// Atomically open a durable local-only chat with a real match from saved meetings.
    pub fn start_history_chat(
        &mut self,
        mode_id: Option<&str>,
        prompt: &str,
    ) -> Result<GenerationRequest> {
        validate_prompt(prompt)?;
        // Lock the source reads and request insert in one transaction. A withdrawal can
        // neither pass between retrieval and persistence nor resurrect older chat turns.
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        let mut session = self.new_session("History chat", mode_id, SessionStatus::Stopped)?;
        session.history_chat = true;
        session.saved_at = Some(now());
        let request = self.freeze_request(&tx, session, prompt, None)?;
        tx.commit()?;
        Ok(request)
    }

    fn freeze_request(
        &self,
        tx: &Transaction<'_>,
        mut session: Session,
        prompt: &str,
        screen: Option<ScreenContext>,
    ) -> Result<GenerationRequest> {
        ensure!(
            session.requests.len() < 1_000,
            "session request limit reached"
        );
        let provider = self.settings()?.answer_provider;
        let past_meetings = if session.history_chat {
            ensure!(
                provider.provider.is_local(),
                "select Local in Settings to ask about saved meetings"
            );
            ensure!(
                screen.is_none(),
                "History chat uses saved meeting text only"
            );
            let mut excerpts = history::retrieve(self, prompt)?;
            if excerpts.is_empty()
                && let Some(MeetingContext::Excerpts(previous)) = session
                    .requests
                    .iter()
                    .rev()
                    .find(|request| request.status == RequestStatus::Completed)
                    .and_then(|request| request.context.past_meetings.as_ref())
            {
                history::ensure_sources_current(self, previous)?;
                excerpts.clone_from(previous);
            }
            ensure!(
                !excerpts.is_empty(),
                "No saved meeting excerpts matched. Try a distinctive project name or term."
            );
            Some(MeetingContext::Excerpts(excerpts))
        } else {
            None
        };
        let mode = session
            .mode_id
            .as_deref()
            .map(|key| self.mode(key))
            .transpose()?;
        let knowledge = if session.history_chat {
            Vec::new()
        } else {
            self.search_knowledge_files(
                prompt,
                5,
                mode.as_ref()
                    .and_then(|mode| mode.knowledge_document_ids.as_deref()),
            )?
        };
        cancel_active(&mut session);
        advance_context_revision(&mut session)?;
        let transcript_start = suffix_start(
            &session.transcript,
            CONTEXT_TRANSCRIPT_SEGMENTS,
            |segment| segment.text.len(),
        );
        let responses: HashMap<&str, &Suggestion> = session
            .suggestions
            .iter()
            .map(|suggestion| (suggestion.request_id.as_str(), suggestion))
            .collect();
        let completed: Vec<(&GenerationRequest, &Suggestion)> = session
            .requests
            .iter()
            .filter(|prior| prior.status == RequestStatus::Completed)
            .filter_map(|prior| {
                responses
                    .get(prior.id.as_str())
                    .map(|&suggestion| (prior, suggestion))
            })
            .collect();
        let chat_start = suffix_start(&completed, CONTEXT_CHAT_TURNS, |(prior, suggestion)| {
            prior.prompt.len() + suggestion.text.len()
        });
        let chat = completed[chat_start..]
            .iter()
            .map(|(prior, suggestion)| ChatTurn {
                request_id: prior.id.clone(),
                prompt: prior.prompt.clone(),
                response: suggestion.text.clone(),
            })
            .collect();
        let mut diagnostics = Vec::new();
        if session
            .transcript
            .last()
            .is_some_and(|segment| segment.text.len() > CONTEXT_TEXT_BYTES)
        {
            diagnostics.push("Newest transcript segment exceeds the 16 KiB context budget; transcript context omitted.".to_owned());
        }
        if completed.last().is_some_and(|(prior, suggestion)| {
            prior.prompt.len() + suggestion.text.len() > CONTEXT_TEXT_BYTES
        }) {
            diagnostics.push("Newest completed chat turn exceeds the 16 KiB context budget; chat context omitted.".to_owned());
        }
        let request = GenerationRequest {
            id: id(),
            session_id: session.id.clone(),
            prompt: prompt.to_owned(),
            context: ContextSnapshot {
                revision: session.context_revision,
                mode,
                transcript: session.transcript[transcript_start..].to_vec(),
                knowledge,
                screen,
                chat,
                past_meetings,
                selection_version: 1,
                omitted_transcript_segments: transcript_start,
                omitted_chat_turns: chat_start,
                diagnostics,
            },
            status: RequestStatus::Pending,
            created_at: now(),
            provider,
        };
        session.active_request_id = Some(request.id.clone());
        session.requests.push(request.clone());
        session.updated_at = now();
        if session.history_chat {
            raise_schema_version(tx, HISTORY_CHAT_SCHEMA_VERSION)?;
        }
        tx.execute(
            "INSERT INTO sessions(id,data) VALUES (?1,?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
            params![session.id, serde_json::to_string(&session)?],
        )?;
        Ok(request)
    }

    pub fn cancel_request(&mut self, session_id: &str, request_id: &str) -> Result<Session> {
        self.change_session(session_id, |session| {
            let request = session
                .requests
                .iter_mut()
                .find(|r| r.id == request_id)
                .context("request does not belong to this session")?;
            if request.status == RequestStatus::Pending {
                request.status = RequestStatus::Cancelled;
            }
            if session.active_request_id.as_deref() == Some(request_id) {
                session.active_request_id = None;
            }
            Ok(())
        })
    }

    /// Returns false for cancelled, superseded, duplicate, or cross-session completions.
    pub fn complete_request(
        &mut self,
        session_id: &str,
        request_id: &str,
        text: &str,
    ) -> Result<bool> {
        self.finish_request(session_id, request_id, text, None, None)
    }

    pub fn complete_request_with_metadata(
        &mut self,
        session_id: &str,
        request_id: &str,
        text: &str,
        metadata: InferenceMetadata,
    ) -> Result<bool> {
        validate_inference(&metadata)?;
        ensure!(
            metadata.finish_reason != "cancelled",
            "cancelled inference cannot be committed"
        );
        self.finish_request(session_id, request_id, text, Some(metadata), None)
    }

    pub fn complete_request_with_hosted_metadata(
        &mut self,
        session_id: &str,
        request_id: &str,
        text: &str,
        metadata: HostedInferenceMetadata,
    ) -> Result<bool> {
        self.finish_request(session_id, request_id, text, None, Some(metadata))
    }

    fn finish_request(
        &mut self,
        session_id: &str,
        request_id: &str,
        text: &str,
        inference: Option<InferenceMetadata>,
        hosted_inference: Option<HostedInferenceMetadata>,
    ) -> Result<bool> {
        check_text(text, "generated response", MAX_DOCUMENT_BYTES, false)?;
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        let Some(mut session) = self.get::<Session>("sessions", session_id)? else {
            return Ok(false);
        };
        if session.active_request_id.as_deref() != Some(request_id) {
            return Ok(false);
        }
        let Some(request) = session.requests.iter_mut().find(|r| {
            r.id == request_id && r.session_id == session_id && r.status == RequestStatus::Pending
        }) else {
            return Ok(false);
        };
        if let Some(metadata) = &inference {
            ensure!(
                request.provider.provider.is_local(),
                "local provenance requires a frozen local provider"
            );
            validate_inference_omissions(metadata, &request.context)?;
        }
        if let Some(metadata) = &hosted_inference {
            ensure!(
                request.context.past_meetings.is_none(),
                "saved meeting requests cannot use hosted inference"
            );
            metadata.validate_for(&request.provider)?;
        }
        request.status = RequestStatus::Completed;
        session.suggestions.push(Suggestion {
            id: id(),
            request_id: request.id.clone(),
            context_revision: request.context.revision,
            text: text.to_owned(),
            sources: request.context.knowledge.clone(),
            created_at: now(),
            inference,
            hosted_inference,
        });
        session.active_request_id = None;
        session.updated_at = now();
        self.put("sessions", &session.id, &session)?;
        tx.commit()?;
        Ok(true)
    }

    pub fn settings(&self) -> Result<Settings> {
        Ok(self.get("settings", "main")?.unwrap_or_default())
    }

    pub fn update_settings(&mut self, settings: Settings) -> Result<Settings> {
        validate_settings(&settings)?;
        let previous = self.settings()?;
        if previous.answer_provider != settings.answer_provider
            || previous.generation_profile != settings.generation_profile
        {
            let tx = self.connection.transaction()?;
            let sessions: Vec<String> = {
                let mut statement = tx.prepare("SELECT data FROM sessions")?;
                statement
                    .query_map([], |row| row.get(0))?
                    .collect::<rusqlite::Result<_>>()?
            };
            for json in sessions {
                let mut session: Session = serde_json::from_str(&json)?;
                if session.active_request_id.is_some() {
                    cancel_active(&mut session);
                    tx.execute(
                        "UPDATE sessions SET data=?1 WHERE id=?2",
                        params![serde_json::to_string(&session)?, session.id],
                    )?;
                }
            }
            tx.execute("INSERT INTO settings(id,data) VALUES ('main',?1) ON CONFLICT(id) DO UPDATE SET data=excluded.data", [serde_json::to_string(&settings)?])?;
            tx.commit()?;
        } else {
            self.put("settings", "main", &settings)?;
        }
        Ok(settings)
    }

    pub fn model_status(&self) -> Result<ModelStatus> {
        Ok(match self.settings()?.model_path {
            None => ModelStatus::Missing,
            Some(path) if Path::new(&path).is_file() && std::fs::File::open(&path).is_ok() => {
                ModelStatus::Configured { path }
            }
            Some(path) => ModelStatus::Unavailable { path },
        })
    }

    pub fn export_session(&self, session_id: &str, format: ExportFormat) -> Result<String> {
        let session = self.stored_session(session_id)?;
        match format {
            ExportFormat::Json => {
                let mut export = serde_json::to_value(&session)?;
                export["format"] = "cueglint-session".into();
                export["version"] = if session.history_chat {
                    HISTORY_CHAT_SCHEMA_VERSION
                } else if session_has_mode_file_scope(&session) {
                    MODE_FILES_SCHEMA_VERSION
                } else {
                    NOTES_SCHEMA_VERSION
                }
                .into();
                Ok(serde_json::to_string_pretty(&export)?)
            }
            ExportFormat::Markdown => {
                let mut output = format!(
                    "# {}\n\nCueglint session export v2\n\nSession: {}\n\n## Transcript\n\n",
                    session.title, session.id
                );
                for segment in &session.transcript {
                    output.push_str(&format!(
                        "**{}{}:** {}\n\n",
                        match segment.speaker {
                            Speaker::You => "You",
                            Speaker::Other => "Other",
                            Speaker::Unknown => "Unknown speaker",
                        },
                        if segment.finalized { "" } else { " (partial)" },
                        segment.text
                    ));
                }
                if let Some(summary) = &session.summary {
                    output.push_str(&format!("## Summary\n\n{summary}\n\n"));
                }
                output.push_str(&format!("## Notes\n\n{}\n", session.notes));
                notes::append_export(&session, &mut output)?;
                if !session.suggestions.is_empty() {
                    output.push_str("\n## Suggestions\n\n");
                    for suggestion in &session.suggestions {
                        output.push_str(&format!("{}\n\n", suggestion.text));
                        if let Some(MeetingContext::Excerpts(excerpts)) = session
                            .requests
                            .iter()
                            .find(|request| request.id == suggestion.request_id)
                            .and_then(|request| request.context.past_meetings.as_ref())
                        {
                            for (index, excerpt) in excerpts.iter().enumerate() {
                                output.push_str(&format!(
                                    "- M{} · {} · {} · {}\n",
                                    index + 1,
                                    excerpt.title.replace(['\r', '\n'], " "),
                                    utc_date(excerpt.created_at),
                                    excerpt.source.label()
                                ));
                            }
                            output.push('\n');
                        }
                    }
                }
                Ok(output)
            }
        }
    }

    pub fn backup_json(&self) -> Result<String> {
        let mut backup = Backup {
            format: "cueglint-backup".to_owned(),
            version: NOTES_SCHEMA_VERSION,
            exported_at: now(),
            sessions: self.sessions("")?,
            modes: self.modes()?,
            knowledge: self.knowledge()?,
            settings: self.settings()?,
        };
        backup.version = backup.data_version();
        let json = serde_json::to_string_pretty(&backup)?;
        ensure!(
            json.len() <= MAX_BACKUP_BYTES,
            "backup exceeds supported size"
        );
        Ok(json)
    }

    /// Validate the whole backup before an atomic replace. `replace` is required for existing data.
    pub fn restore_backup(&mut self, json: &str, replace: bool) -> Result<()> {
        self.restore_backup_with(json, replace, |_| Ok(()))
    }

    /// Restore with Local selected. Prepare the runtime from those settings before replacing data.
    pub fn restore_local_backup<T>(
        &mut self,
        json: &str,
        replace: bool,
        prepare: impl FnOnce(&Settings) -> Result<T>,
    ) -> Result<T> {
        self.restore_backup_with(json, replace, |settings| {
            settings.answer_provider = ProviderConfig::default();
            prepare(settings)
        })
    }

    fn restore_backup_with<T>(
        &mut self,
        json: &str,
        replace: bool,
        prepare: impl FnOnce(&mut Settings) -> Result<T>,
    ) -> Result<T> {
        ensure!(
            json.len() <= MAX_BACKUP_BYTES,
            "backup exceeds supported size"
        );
        let mut backup: Backup = serde_json::from_str(json).context("invalid backup JSON")?;
        for session in &mut backup.sessions {
            sort_transcript(session);
        }
        validate_backup(&backup)?;
        let existing: i64 = self.connection.query_row("SELECT (SELECT COUNT(*) FROM sessions) + (SELECT COUNT(*) FROM knowledge) + (SELECT COUNT(*) FROM settings)", [], |row| row.get(0))?;
        let modes = self.modes()?;
        let defaults = default_modes();
        let pristine_modes =
            modes.len() == defaults.len() && modes.iter().all(|mode| defaults.contains(mode));
        ensure!(
            replace || (existing == 0 && pristine_modes),
            "destination contains data; explicit replacement is required"
        );
        let prepared = prepare(&mut backup.settings)?;
        for session in &mut backup.sessions {
            if session.status == SessionStatus::Recording {
                session.status = SessionStatus::Paused;
            }
            cancel_active(session);
            notes::cancel_pending(session, NotesCancelReason::Restored);
        }
        let tx = self.connection.transaction()?;
        raise_schema_version(&tx, backup.data_version())?;
        tx.execute_batch(
            "DELETE FROM sessions; DELETE FROM modes; DELETE FROM knowledge; DELETE FROM settings;",
        )?;
        for session in backup.sessions {
            tx.execute(
                "INSERT INTO sessions(id,data) VALUES (?1,?2)",
                params![session.id, serde_json::to_string(&session)?],
            )?;
        }
        for mode in backup.modes {
            tx.execute(
                "INSERT INTO modes(id,data) VALUES (?1,?2)",
                params![mode.id, serde_json::to_string(&mode)?],
            )?;
        }
        for document in backup.knowledge {
            tx.execute(
                "INSERT INTO knowledge(id,data) VALUES (?1,?2)",
                params![document.id, serde_json::to_string(&document)?],
            )?;
        }
        tx.execute(
            "INSERT INTO settings(id,data) VALUES ('main',?1)",
            [serde_json::to_string(&backup.settings)?],
        )?;
        tx.commit()?;
        self.partials.clear();
        Ok(prepared)
    }

    fn change_session(
        &mut self,
        session_id: &str,
        change: impl FnOnce(&mut Session) -> Result<()>,
    ) -> Result<Session> {
        self.change_session_if(session_id, |session| {
            change(session)?;
            Ok(((), true))
        })
        .map(|(_, session)| session)
    }

    fn change_session_if<T>(
        &mut self,
        session_id: &str,
        change: impl FnOnce(&mut Session) -> Result<(T, bool)>,
    ) -> Result<(T, Session)> {
        let tx = self.connection.transaction()?;
        let json: String = tx
            .query_row(
                "SELECT data FROM sessions WHERE id=?1",
                [session_id],
                |row| row.get(0),
            )
            .optional()?
            .context("session does not exist")?;
        let mut session: Session = serde_json::from_str(&json)?;
        let (result, changed) = change(&mut session)?;
        if changed {
            if session.saved_at.is_some()
                && !session.history_chat
                && tx.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?
                    >= HISTORY_CHAT_SCHEMA_VERSION
            {
                history::withdraw_affected(&tx, session_id, Some(&session))?;
            }
            session.updated_at = now();
            tx.execute(
                "UPDATE sessions SET data=?1 WHERE id=?2",
                params![serde_json::to_string(&session)?, session_id],
            )?;
        }
        tx.commit()?;
        self.partials.retain(|(id, _), segment| {
            id != session_id
                || (session.status == SessionStatus::Recording
                    && !(session.muted && segment.source == TranscriptSource::Microphone))
        });
        Ok((result, self.with_partials(session)))
    }

    fn ensure_idle(&self, except: Option<&str>) -> Result<()> {
        ensure!(
            !self
                .sessions("")?
                .iter()
                .any(|session| session.status != SessionStatus::Stopped
                    && Some(session.id.as_str()) != except),
            "stop the current session before starting or resuming another"
        );
        Ok(())
    }

    fn get<T: for<'de> Deserialize<'de>>(&self, table: &str, key: &str) -> Result<Option<T>> {
        let json: Option<String> = self
            .connection
            .query_row(
                &format!("SELECT data FROM {table} WHERE id=?1"),
                [key],
                |row| row.get(0),
            )
            .optional()?;
        json.map(|json| serde_json::from_str(&json).context("read stored record"))
            .transpose()
    }

    fn all<T: for<'de> Deserialize<'de>>(&self, table: &str) -> Result<Vec<T>> {
        let mut stmt = self
            .connection
            .prepare(&format!("SELECT data FROM {table}"))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.iter()
            .map(|json| serde_json::from_str(json).context("read stored record"))
            .collect()
    }

    fn put<T: Serialize>(&self, table: &str, key: &str, value: &T) -> Result<()> {
        self.connection.execute(&format!("INSERT INTO {table}(id,data) VALUES (?1,?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data"), params![key, serde_json::to_string(value)?])?;
        Ok(())
    }
}

fn raise_schema_version(tx: &rusqlite::Transaction<'_>, required: u32) -> Result<()> {
    let current = tx.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?;
    if current < required {
        tx.pragma_update(None, "user_version", required)?;
    }
    Ok(())
}

fn cancel_active(session: &mut Session) {
    if let Some(active_id) = session.active_request_id.take()
        && let Some(request) = session
            .requests
            .iter_mut()
            .find(|request| request.id == active_id)
        && request.status == RequestStatus::Pending
    {
        request.status = RequestStatus::Cancelled;
    }
}

fn advance_context_revision(session: &mut Session) -> Result<()> {
    session.context_revision = session.context_revision.checked_add(1).with_context(|| {
        format!(
            "Session {} has exhausted its context revisions; export it and start a new session",
            session.id
        )
    })?;
    Ok(())
}

fn sort_transcript(session: &mut Session) {
    let started = session.created_at;
    session.transcript.sort_by(|a, b| {
        a.session_offset_ns(started)
            .cmp(&b.session_offset_ns(started))
            .then(a.id.cmp(&b.id))
    });
}

fn suffix_start<T>(items: &[T], count_limit: usize, text_bytes: impl Fn(&T) -> usize) -> usize {
    let mut start = items.len();
    let mut used = 0;
    for item in items.iter().rev().take(count_limit) {
        let bytes = text_bytes(item);
        if bytes > CONTEXT_TEXT_BYTES - used {
            break;
        }
        used += bytes;
        start -= 1;
    }
    start
}

fn default_modes() -> Vec<Mode> {
    [
        (
            "general",
            "General",
            "Give concise, relevant suggestions grounded in the conversation and supplied sources.",
        ),
        (
            "interview",
            "Interview",
            "Help prepare thoughtful interview answers. Never invent experience or credentials.",
        ),
        (
            "sales",
            "Sales",
            "Help clarify needs, answer objections honestly, and propose practical next steps.",
        ),
        (
            "meeting",
            "Meeting",
            "Help identify decisions, open questions, and action items from the conversation.",
        ),
    ]
    .into_iter()
    .map(|(id, name, instructions)| Mode {
        id: id.to_owned(),
        name: name.to_owned(),
        instructions: instructions.to_owned(),
        builtin: true,
        knowledge_document_ids: None,
    })
    .collect()
}

fn text_ranges(text: &str, max_chars: usize) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut count = 0;
    for (offset, _) in text.char_indices() {
        if count == max_chars {
            ranges.push((start, offset));
            start = offset;
            count = 0;
        }
        count += 1;
    }
    if start < text.len() {
        ranges.push((start, text.len()));
    }
    ranges
}

fn validate_mode(mode: &Mode) -> Result<()> {
    check_id(&mode.id)?;
    check_text(&mode.name, "mode name", MAX_MODE_NAME_BYTES, false)?;
    check_text(
        &mode.instructions,
        "mode instructions",
        MAX_TRANSCRIPT_BYTES,
        false,
    )?;
    if let Some(ids) = &mode.knowledge_document_ids {
        ensure!(ids.len() <= 1_000, "mode reference file limit exceeded");
        unique_ids(ids.iter().map(String::as_str), "mode document")?;
    }
    Ok(())
}

fn validate_document(document: &KnowledgeDocument) -> Result<()> {
    check_id(&document.id)?;
    check_text(&document.title, "document title", 200, false)?;
    check_text(&document.source, "document source", 4_096, false)?;
    check_text(&document.text, "document text", MAX_DOCUMENT_BYTES, false)
}

fn validate_settings(settings: &Settings) -> Result<()> {
    settings.answer_provider.validate()?;
    if let Some(limit) = settings.hosted_answer_token_limit {
        ensure!(
            (1..=1024).contains(&limit),
            "hosted answer token limit must be between 1 and 1024"
        );
    }
    validate_transcription_language(&settings.transcription_language)?;
    if let Some(path) = &settings.model_path {
        check_text(path, "model path", 4_096, false)?;
    }
    if let Some(path) = &settings.model_directory {
        check_text(path, "model directory", 4_096, false)?;
    }
    if let Some(device) = &settings.microphone_device_id {
        validate_microphone_device_id(device)?;
    }
    ensure!(
        settings.display_id != Some(0),
        "capture display ID must be nonzero; use the explicit default choice"
    );
    for profile in [
        &settings.generation_profile,
        &settings.transcription_profile,
    ]
    .into_iter()
    .flatten()
    {
        check_id(profile)?;
    }
    for (action, binding) in &settings.shortcut_bindings {
        ensure!(
            SHORTCUT_ACTIONS.contains(&action.as_str()),
            "unknown shortcut action: {action}"
        );
        if let Some(binding) = binding {
            validate_shortcut_binding(binding)?;
        }
    }
    Ok(())
}

fn unique_ids<'a>(ids: impl Iterator<Item = &'a str>, label: &str) -> Result<()> {
    let mut seen = BTreeSet::new();
    for id in ids {
        check_id(id)?;
        ensure!(seen.insert(id), "duplicate {label} identifier");
    }
    Ok(())
}

fn validate_backup(backup: &Backup) -> Result<()> {
    ensure!(
        backup.format == "cueglint-backup" && (1..=SCHEMA_VERSION).contains(&backup.version),
        "unsupported backup format or version"
    );
    ensure!(
        backup.sessions.len() <= 10_000
            && backup.modes.len() <= 1_000
            && backup.knowledge.len() <= 1_000,
        "backup record limit exceeded"
    );
    unique_ids(backup.sessions.iter().map(|s| s.id.as_str()), "session")?;
    unique_ids(backup.modes.iter().map(|m| m.id.as_str()), "mode")?;
    unique_ids(backup.knowledge.iter().map(|d| d.id.as_str()), "document")?;
    let meeting_sources: HashMap<_, _> = backup
        .sessions
        .iter()
        .map(|session| (session.id.as_str(), session))
        .collect();
    validate_settings(&backup.settings)?;
    let builtins = default_modes();
    let document_ids: BTreeSet<_> = backup.knowledge.iter().map(|doc| doc.id.as_str()).collect();
    let validate_mode_files = |mode: &Mode| -> Result<()> {
        validate_mode(mode)?;
        if let Some(ids) = &mode.knowledge_document_ids {
            ensure!(
                backup.version >= MODE_FILES_SCHEMA_VERSION,
                "mode file scopes require backup version 3"
            );
            ensure!(
                ids.iter().all(|id| document_ids.contains(id.as_str())),
                "mode references a missing document"
            );
        }
        Ok(())
    };
    for mode in &backup.modes {
        validate_mode_files(mode)?;
        ensure!(
            mode.builtin == builtins.iter().any(|builtin| builtin.id == mode.id),
            "invalid built-in mode identity"
        );
    }
    ensure!(
        builtins
            .iter()
            .all(|builtin| backup.modes.iter().any(|mode| mode.id == builtin.id)),
        "backup is missing a built-in mode"
    );
    for document in &backup.knowledge {
        validate_document(document)?;
    }
    ensure!(
        backup.knowledge.iter().map(|d| d.text.len()).sum::<usize>() <= 20 * 1024 * 1024,
        "knowledge library limit exceeded"
    );
    for session in &backup.sessions {
        if session.history_chat {
            ensure!(
                backup.version >= HISTORY_CHAT_SCHEMA_VERSION
                    && session.saved_at.is_some()
                    && session.status == SessionStatus::Stopped
                    && session.transcript.is_empty(),
                "saved meeting conversations require a version 4, saved text-only session"
            );
        }
        if backup.version == 1 {
            ensure!(
                session.notes_revision == 0
                    && session.notes_job.is_none()
                    && session.generated_notes.is_none(),
                "version1 backup cannot contain version2 notes records"
            );
        }
        notes::validate_session_notes(session)?;
        check_text(&session.title, "session title", 200, false)?;
        check_text(&session.notes, "notes", MAX_DOCUMENT_BYTES, true)?;
        if let Some(summary) = &session.summary {
            check_text(summary, "summary", MAX_DOCUMENT_BYTES, true)?;
        }
        ensure!(
            session.transcript.len() <= 10_000
                && session.requests.len() <= 1_000
                && session.suggestions.len() <= 1_000,
            "session record limit exceeded"
        );
        unique_ids(
            session.transcript.iter().map(|s| s.id.as_str()),
            "transcript",
        )?;
        unique_ids(session.requests.iter().map(|r| r.id.as_str()), "request")?;
        unique_ids(
            session.suggestions.iter().map(|s| s.id.as_str()),
            "suggestion",
        )?;
        unique_ids(
            session
                .suggestions
                .iter()
                .map(|suggestion| suggestion.request_id.as_str()),
            "suggestion request",
        )?;
        if let Some(mode_id) = &session.mode_id {
            ensure!(
                backup.modes.iter().any(|m| &m.id == mode_id),
                "session refers to a missing mode"
            );
        }
        for segment in &session.transcript {
            ensure!(
                segment.finalized,
                "durable history cannot contain partial hypotheses"
            );
            check_text(&segment.text, "transcript", MAX_TRANSCRIPT_BYTES, false)?;
            if let Some(audio) = &segment.audio {
                validate_audio(audio)?;
                ensure!(
                    segment.source != TranscriptSource::Manual,
                    "audio provenance requires a captured source"
                );
            }
        }
        let response_by_request: HashMap<&str, &Suggestion> = session
            .suggestions
            .iter()
            .map(|suggestion| (suggestion.request_id.as_str(), suggestion))
            .collect();
        for (request_index, request) in session.requests.iter().enumerate() {
            request.provider.validate()?;
            history::validate_request(session, request, &meeting_sources)?;
            ensure!(
                request.session_id == session.id,
                "request belongs to another session"
            );
            ensure!(
                request.context.revision <= session.context_revision,
                "request context revision is in the future"
            );
            check_text(&request.prompt, "prompt", MAX_TRANSCRIPT_BYTES, false)?;
            ensure!(
                request.context.transcript.len() <= 10_000 && request.context.knowledge.len() <= 20,
                "request context limit exceeded"
            );
            if let Some(mode) = &request.context.mode {
                validate_mode_files(mode)?;
                if let Some(ids) = &mode.knowledge_document_ids {
                    ensure!(
                        request
                            .context
                            .knowledge
                            .iter()
                            .all(|snippet| ids.contains(&snippet.document_id)),
                        "request contains knowledge outside its mode scope"
                    );
                }
            }
            ensure!(
                request.context.selection_version <= 1,
                "unsupported context selection version"
            );
            if request.context.selection_version == 1 {
                ensure!(
                    request.context.transcript.len() <= CONTEXT_TRANSCRIPT_SEGMENTS
                        && request
                            .context
                            .transcript
                            .iter()
                            .map(|segment| segment.text.len())
                            .sum::<usize>()
                            <= CONTEXT_TEXT_BYTES
                        && request
                            .context
                            .chat
                            .iter()
                            .map(|turn| turn.prompt.len() + turn.response.len())
                            .sum::<usize>()
                            <= CONTEXT_TEXT_BYTES,
                    "bounded context text limit exceeded"
                );
            } else {
                ensure!(
                    request.context.omitted_transcript_segments == 0
                        && request.context.omitted_chat_turns == 0
                        && request.context.diagnostics.is_empty(),
                    "legacy snapshot has unsupported selection metadata"
                );
            }
            ensure!(
                request.context.omitted_transcript_segments
                    <= session
                        .transcript
                        .len()
                        .saturating_sub(request.context.transcript.len()),
                "invalid transcript omission count"
            );
            let completed_count = session.requests[..request_index]
                .iter()
                .filter(|prior| {
                    prior.status == RequestStatus::Completed
                        && response_by_request.contains_key(prior.id.as_str())
                })
                .count();
            ensure!(
                request.context.omitted_chat_turns
                    <= completed_count.saturating_sub(request.context.chat.len()),
                "invalid chat omission count"
            );
            if request.context.selection_version == 1 {
                ensure!(
                    request.context.omitted_chat_turns + request.context.chat.len()
                        == completed_count,
                    "bounded chat omission count does not match completed history"
                );
            }
            ensure!(
                request.context.diagnostics.len() <= 2,
                "too many context diagnostics"
            );
            for diagnostic in &request.context.diagnostics {
                check_text(diagnostic, "context diagnostic", 256, false)?;
            }
            unique_ids(
                request.context.transcript.iter().map(|s| s.id.as_str()),
                "context transcript",
            )?;
            for segment in &request.context.transcript {
                ensure!(
                    segment.finalized && session.transcript.contains(segment),
                    "request context contains an unknown or partial transcript"
                );
                check_text(
                    &segment.text,
                    "context transcript",
                    MAX_TRANSCRIPT_BYTES,
                    false,
                )?;
            }
            for snippet in &request.context.knowledge {
                validate_snippet(snippet, &backup.knowledge)?;
            }
            if let Some(screen) = &request.context.screen {
                validate_screen(screen)?;
            }
            ensure!(
                request.context.chat.len() <= 8,
                "chat context exceeds eight turns"
            );
            unique_ids(
                request
                    .context
                    .chat
                    .iter()
                    .map(|turn| turn.request_id.as_str()),
                "chat turn",
            )?;
            for turn in &request.context.chat {
                check_text(&turn.prompt, "previous prompt", MAX_TRANSCRIPT_BYTES, false)?;
                check_text(
                    &turn.response,
                    "previous response",
                    MAX_DOCUMENT_BYTES,
                    false,
                )?;
                ensure!(
                    session.requests[..request_index]
                        .iter()
                        .any(|prior| prior.id == turn.request_id
                            && prior.status == RequestStatus::Completed
                            && prior.prompt == turn.prompt)
                        && response_by_request
                            .get(turn.request_id.as_str())
                            .is_some_and(|suggestion| suggestion.text == turn.response),
                    "chat context does not match completed history"
                );
            }
        }
        if let Some(active) = &session.active_request_id {
            ensure!(
                session
                    .requests
                    .iter()
                    .any(|r| &r.id == active && r.status == RequestStatus::Pending),
                "invalid active request"
            );
        }
        ensure!(
            session
                .requests
                .iter()
                .filter(|request| request.status == RequestStatus::Pending)
                .count()
                == usize::from(session.active_request_id.is_some()),
            "orphaned pending request"
        );
        for suggestion in &session.suggestions {
            check_text(&suggestion.text, "suggestion", MAX_DOCUMENT_BYTES, false)?;
            ensure!(
                session
                    .requests
                    .iter()
                    .any(|r| r.id == suggestion.request_id
                        && r.status == RequestStatus::Completed
                        && r.context.revision == suggestion.context_revision
                        && r.context.knowledge == suggestion.sources),
                "suggestion refers to an invalid request"
            );
            ensure!(
                suggestion.sources.len() <= 20,
                "too many suggestion sources"
            );
            for snippet in &suggestion.sources {
                validate_snippet(snippet, &backup.knowledge)?;
            }
            if let Some(metadata) = &suggestion.inference {
                validate_inference(metadata)?;
                let request = session
                    .requests
                    .iter()
                    .find(|request| request.id == suggestion.request_id)
                    .context("suggestion request is missing")?;
                ensure!(
                    request.provider.provider.is_local(),
                    "local provenance requires a frozen local provider"
                );
                validate_inference_omissions(metadata, &request.context)?;
                ensure!(
                    metadata.finish_reason != "cancelled",
                    "suggestion has cancelled inference metadata"
                );
            }
            if let Some(metadata) = &suggestion.hosted_inference {
                ensure!(
                    suggestion.inference.is_none(),
                    "suggestion cannot have both local and hosted provenance"
                );
                let request = session
                    .requests
                    .iter()
                    .find(|request| request.id == suggestion.request_id)
                    .context("suggestion request is missing")?;
                metadata.validate_for(&request.provider)?;
            }
        }
    }
    Ok(())
}

fn validate_snippet(snippet: &GroundingSnippet, documents: &[KnowledgeDocument]) -> Result<()> {
    check_id(&snippet.document_id)?;
    check_text(&snippet.title, "source title", 200, false)?;
    check_text(&snippet.source, "source", 4_096, false)?;
    check_text(&snippet.text, "source quotation", MAX_DOCUMENT_BYTES, false)?;
    ensure!(
        snippet.start_byte < snippet.end_byte
            && snippet.end_byte - snippet.start_byte == snippet.text.len(),
        "invalid source byte offsets"
    );
    let document = documents
        .iter()
        .find(|document| document.id == snippet.document_id)
        .context("source document is missing")?;
    ensure!(
        document.title == snippet.title
            && document.source == snippet.source
            && document.text.get(snippet.start_byte..snippet.end_byte)
                == Some(snippet.text.as_str()),
        "source quotation does not match original document"
    );
    Ok(())
}

fn validate_screen(screen: &ScreenContext) -> Result<()> {
    check_id(&screen.id)?;
    ensure!(
        screen.width > 0 && screen.width <= 32_768 && screen.height > 0 && screen.height <= 32_768,
        "invalid screen dimensions"
    );
    ensure!(screen.captured_at >= 0, "invalid screen capture timestamp");
    let is_sha256 = |hash: &str| hash.len() == 64 && hash.bytes().all(|c| c.is_ascii_hexdigit());
    ensure!(is_sha256(&screen.sha256), "invalid screen hash");
    let pixels = screen.pixel_sha256.as_deref();
    ensure!(pixels.is_none_or(is_sha256), "invalid screen pixel hash");
    ensure!(
        !matches!(screen.source, ScreenSource::Window(_)) || pixels.is_some(),
        "a window capture requires its pixel hash"
    );
    check_text(&screen.text, "screen text", MAX_TRANSCRIPT_BYTES, true)
}

fn validate_inference(metadata: &InferenceMetadata) -> Result<()> {
    check_id(&metadata.model_profile)?;
    check_text(&metadata.backend, "inference backend", 200, false)?;
    ensure!(
        metadata.model_revision.len() == 40
            && metadata
                .model_revision
                .chars()
                .all(|c| c.is_ascii_hexdigit()),
        "invalid model revision"
    );
    for hash in [&metadata.model_sha256, &metadata.tokenizer_sha256] {
        ensure!(
            hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()),
            "invalid inference artifact hash"
        );
    }
    ensure!(
        metadata.prompt_sha256.is_some() == metadata.template_revision.is_some(),
        "prompt hash and template revision must be recorded together"
    );
    if let Some(hash) = &metadata.prompt_sha256 {
        ensure!(
            hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()),
            "invalid prompt SHA-256"
        );
    }
    if let Some(revision) = &metadata.template_revision {
        check_id(revision)?;
    }
    if let Some(revision) = &metadata.decoder_policy_revision {
        check_id(revision)?;
        ensure!(
            metadata.template_revision.is_some(),
            "decoder policy requires prompt/template identities"
        );
    }
    ensure!(
        (1..=1_024).contains(&metadata.max_new_tokens)
            && metadata.generated_tokens <= metadata.max_new_tokens
            && metadata.prompt_tokens <= 32_768,
        "invalid inference token counts"
    );
    ensure!(
        ["end_of_turn", "token_limit", "cancelled"].contains(&metadata.finish_reason.as_str()),
        "invalid inference finish reason"
    );
    Ok(())
}

fn validate_inference_omissions(
    metadata: &InferenceMetadata,
    context: &ContextSnapshot,
) -> Result<()> {
    let transcript_total = context
        .omitted_transcript_segments
        .checked_add(context.transcript.len())
        .context("transcript omission count overflow")?;
    let chat_total = context
        .omitted_chat_turns
        .checked_add(context.chat.len())
        .context("chat omission count overflow")?;
    ensure!(
        (context.omitted_transcript_segments..=transcript_total)
            .contains(&metadata.omitted_transcript_segments),
        "inference transcript omissions do not match the frozen selection"
    );
    ensure!(
        (context.omitted_chat_turns..=chat_total).contains(&metadata.omitted_chat_turns),
        "inference chat omissions do not match the frozen selection"
    );
    Ok(())
}

fn validate_audio(audio: &AudioMetadata) -> Result<()> {
    ensure!(
        audio.end_ns > audio.start_ns && audio.end_ns - audio.start_ns <= 30_000_000_000,
        "invalid audio sample times"
    );
    ensure!(
        audio.sample_rate == 16_000,
        "audio provenance requires mono 16kHz inference samples"
    );
    check_id(&audio.model_profile)?;
    ensure!(
        audio.model_revision.len() == 40
            && audio.model_revision.chars().all(|c| c.is_ascii_hexdigit()),
        "invalid ASR model revision"
    );
    ensure!(
        audio.model_sha256.len() == 64 && audio.model_sha256.chars().all(|c| c.is_ascii_hexdigit()),
        "invalid ASR model hash"
    );
    ensure!(
        !audio.language.is_empty()
            && audio.language.len() <= 35
            && audio
                .language
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-'),
        "invalid transcript language"
    );
    match (
        &audio.backend,
        &audio.frontend_revision,
        &audio.frontend_config_sha256,
    ) {
        (None, None, None) => {}
        (Some(backend), Some(revision), Some(hash)) => {
            check_id(backend)?;
            check_id(revision)?;
            ensure!(
                hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()),
                "invalid ASR frontend configuration SHA-256"
            );
        }
        _ => anyhow::bail!(
            "ASR backend, frontend revision and configuration hash must be recorded together"
        ),
    }
    Ok(())
}
