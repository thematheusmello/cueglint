use cueglint_core::{
    AudioMetadata, ExportFormat, InferenceMetadata, Mode, ModelStatus, RequestStatus,
    ScreenContext, SessionStatus, Speaker, Store, TranscriptSource, TranscriptUpdate,
};
use serde_json::Value;
use std::path::PathBuf;
use uuid::Uuid;

fn update(id: &str, text: &str, finalized: bool) -> TranscriptUpdate {
    TranscriptUpdate {
        id: id.to_owned(),
        speaker: Speaker::You,
        text: text.to_owned(),
        finalized,
        source: TranscriptSource::Manual,
    }
}

fn temporary_db() -> PathBuf {
    std::env::temp_dir()
        .join(format!("cueglint-test-{}", Uuid::new_v4()))
        .join("state.sqlite3")
}

#[test]
fn session_lifecycle_is_durable_and_restart_cancels_lost_workers() -> anyhow::Result<()> {
    let path = temporary_db();
    let session_id;
    let request_id;
    {
        let mut store = Store::open(&path)?;
        assert!(
            Store::open(&path).is_err(),
            "a second process must not pause a live session"
        );
        let session = store.start_session("Research call", Some("meeting"))?;
        session_id = session.id;
        assert!(store.start_session("Second call", None).is_err());
        store.pause_session(&session_id)?;
        assert!(
            store
                .upsert_transcript(&session_id, update("one", "Paused capture", true))
                .is_err()
        );
        store.resume_session(&session_id)?;
        store.mute_session(&session_id, true)?;
        let mut microphone = update("mic", "Must not be captured", true);
        microphone.source = TranscriptSource::Microphone;
        assert!(store.upsert_transcript(&session_id, microphone).is_err());
        store.upsert_transcript(&session_id, update("one", "Meeting notes", true))?;
        store.update_notes(&session_id, "Do not lose my notes")?;
        store.save_session(&session_id)?;
        request_id = store.begin_request(&session_id, "Next action?")?.id;
    }
    {
        let mut store = Store::open(&path)?;
        store.recover_interrupted()?;
        let session = store.session(&session_id)?;
        assert_eq!(session.status, SessionStatus::Paused);
        assert_eq!(session.notes, "Do not lose my notes");
        assert!(session.saved_at.is_some());
        assert!(session.active_request_id.is_none());
        assert_eq!(session.requests[0].status, RequestStatus::Cancelled);
        assert!(!store.complete_request(&session_id, &request_id, "Late response")?);
        store.resume_session(&session_id)?;
        store.stop_session(&session_id)?;
        assert_eq!(store.session(&session_id)?.status, SessionStatus::Stopped);
        store.start_session("Next call", None)?;
    }
    std::fs::remove_dir_all(path.parent().unwrap())?;
    Ok(())
}

#[test]
fn failed_finish_keeps_the_recording_and_retry_saves_the_same_meeting() -> anyhow::Result<()> {
    let path = temporary_db();
    let mut store = Store::open(&path)?;
    let id = store.start_session("Original retryable meeting", None)?.id;
    store.upsert_transcript(&id, update("final", "The checkpoint is Friday.", true))?;
    store.update_notes(&id, "Keep my original notes.")?;
    let request = store.begin_request(&id, "What is the checkpoint?")?;
    let before = store.session(&id)?;
    let fault = rusqlite::Connection::open(&path)?;
    fault.execute_batch(
        "CREATE TRIGGER fail_session_save BEFORE UPDATE ON sessions
         WHEN json_extract(NEW.data, '$.saved_at') IS NOT NULL
         BEGIN SELECT RAISE(ABORT, 'injected save failure'); END;",
    )?;

    let error = store.finish_session(&id).unwrap_err();
    assert!(format!("{error:#}").contains("injected save failure"));
    assert_eq!(
        store.session(&id)?,
        before,
        "failed final save changes no session data"
    );

    fault.execute_batch("DROP TRIGGER fail_session_save")?;
    let saved = store.finish_session(&id)?;
    assert_eq!(saved.status, SessionStatus::Stopped);
    assert!(saved.saved_at.is_some());
    assert_eq!(saved.transcript, before.transcript);
    assert_eq!(saved.notes, before.notes);
    assert_eq!(saved.requests[0].status, RequestStatus::Cancelled);
    assert!(!store.complete_request(&id, &request.id, "A stale answer")?);
    drop(fault);
    drop(store);
    assert_eq!(Store::open(&path)?.session(&id)?, saved);
    std::fs::remove_dir_all(path.parent().unwrap())?;
    Ok(())
}

#[test]
fn partial_transcripts_revise_one_identity_and_finalize_exactly_once() -> anyhow::Result<()> {
    let mut store = Store::open_in_memory()?;
    let id = store.start_session("Transcript", None)?.id;
    store.upsert_transcript(&id, update("stable-segment", "hel", false))?;
    store.upsert_transcript(&id, update("stable-segment", "hello", false))?;
    let session = store.upsert_transcript(&id, update("stable-segment", "hello world", true))?;
    assert_eq!(session.transcript.len(), 1);
    assert_eq!(session.context_revision, 1);
    assert_eq!(session.transcript[0].text, "hello world");
    assert!(session.transcript[0].finalized);
    assert_eq!(
        store
            .upsert_transcript(&id, update("stable-segment", "hello world", true))?
            .context_revision,
        1
    );
    assert!(
        store
            .upsert_transcript(&id, update("stable-segment", "rewritten", true))
            .is_err()
    );
    assert!(
        store
            .upsert_transcript(&id, update("stable-segment", "hello world", false))
            .is_err()
    );
    Ok(())
}

#[test]
fn frozen_requests_exclude_partials_and_reject_cancelled_superseded_results() -> anyhow::Result<()>
{
    let mut store = Store::open_in_memory()?;
    let id = store
        .start_session("Request ownership", Some("interview"))?
        .id;
    store.upsert_transcript(&id, update("final", "A complete statement", true))?;
    store.upsert_transcript(&id, update("partial", "Still speaking", false))?;
    let first = store.begin_request(&id, "How should I respond?")?;
    assert_eq!(first.context.transcript.len(), 1);
    assert_eq!(first.context.revision, 2);
    assert_eq!(first.context.mode.as_ref().unwrap().id, "interview");
    store.upsert_transcript(&id, update("partial", "Now finalized", true))?;
    assert_eq!(
        first.context.transcript.len(),
        1,
        "the request owns an immutable snapshot"
    );
    let second = store.begin_request(&id, "A different question")?;
    assert!(!store.complete_request(&id, &first.id, "A stale answer")?);
    assert!(!store.complete_request("another-session", &second.id, "Wrong session")?);
    assert!(store.complete_request(&id, &second.id, "A response supplied by a test worker")?);
    assert!(!store.complete_request(&id, &second.id, "Duplicate response")?);
    let third = store.begin_request(&id, "One more")?;
    store.cancel_request(&id, &third.id)?;
    assert!(!store.complete_request(&id, &third.id, "Cancelled answer")?);
    let session = store.session(&id)?;
    assert_eq!(session.suggestions.len(), 1);
    assert_eq!(session.suggestions[0].request_id, second.id);
    assert_eq!(session.suggestions[0].context_revision, 4);
    Ok(())
}

#[test]
fn knowledge_quotes_preserve_original_utf8_and_grounding_metadata() -> anyhow::Result<()> {
    let mut store = Store::open_in_memory()?;
    let original = format!(
        "{}\nThe coffee budget is €240 per month.\n",
        "Ação. ".repeat(300)
    );
    let doc = store.ingest_knowledge("Office budget", "budget-original.txt", &original)?;
    assert_eq!(store.knowledge()?[0].text, original);
    let snippets = store.search_knowledge("coffee budget", 5)?;
    assert!(!snippets.is_empty());
    for snippet in &snippets {
        assert_eq!(snippet.document_id, doc.id);
        assert_eq!(snippet.text, original[snippet.start_byte..snippet.end_byte]);
        assert_eq!(snippet.source, "budget-original.txt");
    }
    assert!(store.search_knowledge("unmatchedword", 5)?.is_empty());
    assert!(store.ingest_knowledge("Bad", "bad.txt", "\0").is_err());
    let session = store.start_session("Office meeting", None)?;
    let request = store.begin_request(&session.id, "What is the coffee budget?")?;
    let completed = store.begin_request(&session.id, "Confirm the coffee budget")?;
    store.complete_request(
        &session.id,
        &completed.id,
        "An authored response remains editable history",
    )?;
    let pending = store.begin_request(&session.id, "What is the coffee budget?")?;
    store.delete_knowledge(&doc.id)?;
    assert!(
        !request.context.knowledge.is_empty(),
        "the external worker owns its original immutable snapshot"
    );
    assert!(!store.complete_request(
        &session.id,
        &pending.id,
        "A stale answer using deleted knowledge"
    )?);
    let stored = store.session(&session.id)?;
    assert!(
        stored
            .requests
            .iter()
            .all(|r| r.context.knowledge.is_empty())
    );
    assert!(stored.suggestions[0].sources.is_empty());
    assert_eq!(
        stored.suggestions[0].text,
        "An authored response remains editable history"
    );
    let backup = store.backup_json()?;
    let mut restored = Store::open_in_memory()?;
    restored.restore_backup(&backup, false)?;
    assert!(
        restored.session(&session.id)?.suggestions[0]
            .sources
            .is_empty()
    );
    assert!(
        !backup.contains("€240"),
        "deletion must remove the source's stored derived quotations too"
    );
    Ok(())
}

#[test]
fn mode_files_filter_frozen_context_and_empty_scope_survives_deletion_and_restore()
-> anyhow::Result<()> {
    let mut store = Store::open_in_memory()?;
    let attached = store.ingest_knowledge("Attached", "orchid.txt", "The orchid marker is 731.")?;
    for number in 0..5 {
        store.ingest_knowledge(
            &format!("Other {number}"),
            "elsewhere.txt",
            "The orchid safety marker is 900.",
        )?;
    }
    let mut mode = store.upsert_mode(Mode::new("Workshop", "Use the chosen reference files."))?;
    let query = "orchid safety";
    let session = store.start_session("Workshop session", Some(&mode.id))?;
    store.update_notes(&session.id, "Keep these original notes")?;
    let legacy = store.begin_request(&session.id, query)?;
    assert_eq!(legacy.context.knowledge.len(), 5);
    assert!(
        legacy
            .context
            .knowledge
            .iter()
            .all(|item| item.document_id != attached.id)
    );
    assert!(
        legacy
            .context
            .mode
            .unwrap()
            .knowledge_document_ids
            .is_none()
    );

    mode.knowledge_document_ids = Some(vec![attached.id.clone()]);
    store.upsert_mode(mode.clone())?;
    for ids in [
        vec!["missing".into()],
        vec![attached.id.clone(); 2],
        vec![attached.id.clone(); 1_001],
    ] {
        let invalid = Mode {
            knowledge_document_ids: Some(ids),
            ..mode.clone()
        };
        assert!(store.upsert_mode(invalid).is_err());
        assert_eq!(store.mode(&mode.id)?, mode);
    }
    let pending = store.begin_request(&session.id, query)?;
    assert_eq!(pending.context.mode.as_ref(), Some(&mode));
    assert_eq!(pending.context.knowledge.len(), 1);
    assert_eq!(pending.context.knowledge[0].document_id, attached.id);
    assert_eq!(pending.context.knowledge[0].text, attached.text);
    let scoped_backup = store.backup_json()?;
    let mut restored = Store::open_in_memory()?;
    restored.restore_backup(&scoped_backup, false)?;
    assert_eq!(restored.mode(&mode.id)?, mode);
    let mut invalid: Value = serde_json::from_str(&scoped_backup)?;
    invalid["sessions"][0]["requests"][1]["context"]["mode"]["knowledge_document_ids"] =
        serde_json::json!([]);
    assert!(restored.restore_backup(&invalid.to_string(), true).is_err());
    assert_eq!(restored.mode(&mode.id)?, mode);
    assert_eq!(
        restored.session(&session.id)?.notes,
        "Keep these original notes"
    );

    store.delete_knowledge(&attached.id)?;
    assert!(!store.complete_request(&session.id, &pending.id, "Stale answer")?);
    let stored = store.session(&session.id)?;
    assert_eq!(stored.requests[1].status, RequestStatus::Cancelled);
    assert!(stored.requests[1].context.knowledge.is_empty());
    assert_eq!(
        stored.requests[1]
            .context
            .mode
            .as_ref()
            .unwrap()
            .knowledge_document_ids
            .as_deref(),
        Some(&[] as &[String])
    );
    assert_eq!(pending.context.knowledge[0].document_id, attached.id);
    assert_eq!(
        store.mode(&mode.id)?.knowledge_document_ids,
        Some(Vec::<String>::new())
    );
    restored.restore_backup(&store.backup_json()?, true)?;
    let new = restored.begin_request(&session.id, query)?;
    assert!(new.context.knowledge.is_empty());
    assert_eq!(restored.search_knowledge(query, 5)?.len(), 5);
    assert_eq!(
        restored.session(&session.id)?.notes,
        "Keep these original notes"
    );
    Ok(())
}

#[test]
fn scoped_mode_format_blocks_rollback_without_upgrading_unscoped_backups() -> anyhow::Result<()> {
    use rusqlite::{Connection, OpenFlags};
    use std::path::Path;
    fn sqlite_version(path: &Path) -> rusqlite::Result<u32> {
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?.query_row(
            "PRAGMA user_version",
            [],
            |row| row.get(0),
        )
    }

    let path = temporary_db();
    let restore_path = path.with_file_name("restored.sqlite3");
    let mut store = Store::open(&path)?;
    let mut mode = store.upsert_mode(Mode::new("Workshop", "Use the chosen files."))?;
    let unscoped_backup = store.backup_json()?;
    assert_eq!(
        serde_json::from_str::<Value>(&unscoped_backup)?["version"],
        2
    );
    assert_eq!(sqlite_version(&path)?, 2);
    mode.knowledge_document_ids = Some(vec!["missing".into()]);
    assert!(store.upsert_mode(mode.clone()).is_err());
    assert_eq!(sqlite_version(&path)?, 2);

    mode.knowledge_document_ids = Some(vec![]);
    store.upsert_mode(mode.clone())?;
    assert_eq!(sqlite_version(&path)?, 3);
    let session = store.start_session("Workshop session", Some(&mode.id))?;
    let export = || store.export_session(&session.id, ExportFormat::Json);
    assert_eq!(serde_json::from_str::<Value>(&export()?)?["version"], 2);
    let request = store.begin_request(&session.id, "Workshop marker")?;
    store.cancel_request(&session.id, &request.id)?;
    let scoped_backup = store.backup_json()?;
    let scoped: Value = serde_json::from_str(&scoped_backup)?;
    assert_eq!(scoped["version"], 3);
    let export = store.export_session(&session.id, ExportFormat::Json)?;
    assert_eq!(serde_json::from_str::<Value>(&export)?["version"], 3);
    assert!(
        store
            .export_session(&session.id, ExportFormat::Markdown)?
            .contains("export v2")
    );

    let mut restored = Store::open(&restore_path)?;
    restored.restore_backup(&unscoped_backup, false)?;
    assert_eq!(sqlite_version(&restore_path)?, 2);
    for version in [1, 2] {
        let mut invalid = scoped.clone();
        invalid["version"] = version.into();
        assert_eq!(
            restored
                .restore_backup(&invalid.to_string(), true)
                .unwrap_err()
                .to_string(),
            "mode file scopes require backup version 3"
        );
        for mode in invalid["modes"].as_array_mut().unwrap() {
            mode.as_object_mut()
                .unwrap()
                .remove("knowledge_document_ids");
        }
        assert_eq!(
            restored
                .restore_backup(&invalid.to_string(), true)
                .unwrap_err()
                .to_string(),
            "mode file scopes require backup version 3"
        );
        assert_eq!(sqlite_version(&restore_path)?, 2);
        assert!(restored.mode(&mode.id)?.knowledge_document_ids.is_none());
    }
    restored.restore_backup(&scoped_backup, true)?;
    assert_eq!(sqlite_version(&restore_path)?, 3);
    drop(restored);
    let mut restored = Store::open(&restore_path)?;
    assert_eq!(restored.mode(&mode.id)?, mode);
    assert_eq!(sqlite_version(&restore_path)?, 3);
    restored.restore_backup(&unscoped_backup, true)?;
    assert_eq!(sqlite_version(&restore_path)?, 3);
    assert_eq!(
        serde_json::from_str::<Value>(&restored.backup_json()?)?["version"],
        2
    );
    drop(restored);
    drop(store);
    std::fs::remove_dir_all(path.parent().unwrap())?;
    Ok(())
}

#[test]
fn saved_session_search_matches_creation_date_after_reopen() -> anyhow::Result<()> {
    let path = temporary_db();
    let mut store = Store::open(&path)?;
    let earlier = store.start_session("Tidal generator workshop", None)?.id;
    store.update_notes(&earlier, "Original rotor schedule")?;
    store.update_summary(&earlier, "Inspect the bearing")?;
    store.upsert_transcript(&earlier, update("final", "Anchor the frame", true))?;
    store.upsert_transcript(&earlier, update("partial", "Tentative reading", false))?;
    assert!(store.sessions("tentative")?.is_empty());
    store.finish_session(&earlier)?;
    let later = store.start_session("Spring shadow study", None)?.id;
    store.finish_session(&later)?;

    let march_first = 1_709_251_200_000_i64;
    let mut backup: Value = serde_json::from_str(&store.backup_json()?)?;
    for row in backup["sessions"].as_array_mut().unwrap() {
        let (created, updated) = if row["id"] == earlier {
            (march_first - 1, march_first + 86_400_000)
        } else {
            (march_first, march_first)
        };
        row["created_at"] = created.into();
        row["updated_at"] = updated.into();
    }
    store.restore_backup(&backup.to_string(), true)?;
    drop(store);

    let store = Store::open(&path)?;
    let expected = [store.session(&earlier)?, store.session(&later)?];
    assert_eq!(store.sessions("")?, expected);
    assert_eq!(store.sessions("2024")?, expected);
    for query in [
        "2024-02-29",
        "2024-02",
        "TIDAL",
        "ROTOR",
        "BEARING",
        "ANCHOR",
    ] {
        assert_eq!(store.sessions(query)?.as_slice(), &expected[..1], "{query}");
    }
    assert_eq!(store.sessions("2024-03")?.as_slice(), &expected[1..]);
    for query in ["2024-03-02", " 2024-02", "UTC", "tentative"] {
        assert!(store.sessions(query)?.is_empty(), "{query}");
    }
    assert!(store.sessions(&"a".repeat(1_001)).is_err());
    assert!(store.sessions("\0").is_err());
    drop(store);
    std::fs::remove_dir_all(path.parent().unwrap())?;
    Ok(())
}

#[test]
fn modes_notes_history_export_delete_and_atomic_backup_round_trip() -> anyhow::Result<()> {
    let mut store = Store::open_in_memory()?;
    let mode = store.upsert_mode(Mode::new("Support", "Use supplied support documentation."))?;
    let session = store.start_session("Unusual customer issue", Some(&mode.id))?;
    store.update_notes(&session.id, "Remember the revised delivery date")?;
    store.update_summary(&session.id, "A practical next step")?;
    store.stop_session(&session.id)?;
    store.save_session(&session.id)?;
    assert_eq!(store.sessions("REVISED")?.len(), 1);
    assert!(
        store
            .export_session(&session.id, ExportFormat::Markdown)?
            .contains("Remember the revised delivery date")
    );
    let json: Value =
        serde_json::from_str(&store.export_session(&session.id, ExportFormat::Json)?)?;
    assert_eq!(json["summary"], "A practical next step");
    assert_eq!(json["format"], "cueglint-session");
    assert_eq!(json["version"], 2);
    let backup = store.backup_json()?;
    let mut target = Store::open_in_memory()?;
    target.restore_backup(&backup, false)?;
    assert_eq!(target.session(&session.id)?, store.session(&session.id)?);
    assert!(target.restore_backup(&backup, false).is_err());
    let before = target.backup_json()?;
    let mut invalid: Value = serde_json::from_str(&backup)?;
    invalid["sessions"][0]["notes"] = Value::String("\0".to_owned());
    assert!(target.restore_backup(&invalid.to_string(), true).is_err());
    let before: Value = serde_json::from_str(&before)?;
    let after: Value = serde_json::from_str(&target.backup_json()?)?;
    assert_eq!(
        before["sessions"], after["sessions"],
        "invalid restore must preserve all existing data"
    );
    invalid["version"] = 999.into();
    assert!(target.restore_backup(&invalid.to_string(), true).is_err());
    target.delete_mode(&mode.id)?;
    assert_eq!(target.session(&session.id)?.mode_id, None);
    assert!(target.delete_mode("general").is_err());
    target.delete_session(&session.id)?;
    assert!(target.sessions("")?.is_empty());
    assert!(target.session(&session.id).is_err());
    Ok(())
}

#[test]
fn a_configured_path_is_never_reported_as_running_inference() -> anyhow::Result<()> {
    let mut store = Store::open_in_memory()?;
    assert_eq!(store.model_status()?, ModelStatus::Missing);
    let mut settings = store.settings()?;
    settings.model_path = Some("/not/a/model.gguf".to_owned());
    store.update_settings(settings)?;
    assert!(matches!(
        store.model_status()?,
        ModelStatus::Unavailable { .. }
    ));
    Ok(())
}

#[test]
fn edited_builtins_and_forged_grounding_cannot_be_overwritten_by_an_unconfirmed_restore()
-> anyhow::Result<()> {
    let mut source = Store::open_in_memory()?;
    source.ingest_knowledge("Manual", "manual.txt", "The safe setting is blue.")?;
    let session = source.start_session("Grounding", None)?;
    source.begin_request(&session.id, "What setting is safe?")?;
    let backup = source.backup_json()?;
    let mut destination = Store::open_in_memory()?;
    let mut mode = destination.mode("general")?;
    mode.instructions = "My own carefully edited instructions".to_owned();
    destination.upsert_mode(mode)?;
    assert!(destination.restore_backup(&backup, false).is_err());
    assert_eq!(
        destination.mode("general")?.instructions,
        "My own carefully edited instructions"
    );
    let mut forged: Value = serde_json::from_str(&backup)?;
    forged["sessions"][0]["requests"][0]["context"]["knowledge"][0]["text"] =
        "The safe setting is evil.".into();
    assert!(
        destination
            .restore_backup(&forged.to_string(), true)
            .is_err()
    );
    assert_eq!(
        destination.mode("general")?.instructions,
        "My own carefully edited instructions"
    );
    destination.restore_backup(&backup, true)?;
    assert!(
        destination
            .session(&session.id)?
            .active_request_id
            .is_none()
    );
    Ok(())
}

#[test]
fn screen_context_keeps_legacy_display_json_and_rejects_ambiguous_window_identity() {
    let mut context = serde_json::json!({
        "id": "original-screen",
        "display_id": 2,
        "captured_at": 100,
        "width": 1920,
        "height": 1080,
        "sha256": "a".repeat(64),
        "text": "Original screen text"
    });
    let display: ScreenContext = serde_json::from_value(context.clone()).unwrap();
    assert_eq!(display.source, cueglint_core::ScreenSource::Display(2));
    assert_eq!(serde_json::to_value(display).unwrap(), context);

    context["window_id"] = 31.into();
    context["pixel_sha256"] = "b".repeat(64).into();
    assert!(serde_json::from_value::<ScreenContext>(context.clone()).is_err());
    context.as_object_mut().unwrap().remove("display_id");
    let window: ScreenContext = serde_json::from_value(context.clone()).unwrap();
    assert_eq!(
        window.source,
        cueglint_core::ScreenSource::Window(std::num::NonZeroU32::new(31).unwrap())
    );
    assert_eq!(serde_json::to_value(window).unwrap(), context);

    context["window_id"] = 0.into();
    assert!(serde_json::from_value::<ScreenContext>(context.clone()).is_err());
    context.as_object_mut().unwrap().remove("window_id");
    assert!(serde_json::from_value::<ScreenContext>(context).is_err());
}

#[test]
fn new_screen_chat_is_persisted_with_its_first_frozen_request() -> anyhow::Result<()> {
    let mut store = Store::open_in_memory()?;
    let meeting = store.start_session("Independent meeting", None)?;
    let mut screen = ScreenContext {
        id: "original-window".into(),
        source: cueglint_core::ScreenSource::Window(std::num::NonZeroU32::new(31).unwrap()),
        captured_at: 100,
        width: 538,
        height: 288,
        sha256: "invalid".into(),
        pixel_sha256: None,
        text: "Original window marker".into(),
    };
    assert!(
        store
            .start_chat_with_screen(None, "What is the marker?", screen.clone())
            .is_err()
    );
    assert_eq!(store.sessions("")?, [meeting.clone()]);
    screen.sha256 = "a".repeat(64);
    assert!(
        store
            .start_chat_with_screen(None, "What is the marker?", screen.clone())
            .is_err()
    );
    assert_eq!(store.sessions("")?, [meeting.clone()]);
    screen.pixel_sha256 = Some("b".repeat(64));
    let request = store.start_chat_with_screen(None, "What is the marker?", screen.clone())?;
    let chat = store.session(&request.session_id)?;
    assert_eq!(chat.status, cueglint_core::SessionStatus::Stopped);
    assert_eq!(chat.active_request_id.as_deref(), Some(request.id.as_str()));
    assert_eq!(chat.requests, [request.clone()]);
    assert_eq!(request.context.screen, Some(screen));
    assert_eq!(store.session(&meeting.id)?, meeting);
    Ok(())
}

#[test]
fn screen_identity_and_actual_inference_metadata_are_preserved() -> anyhow::Result<()> {
    let mut store = Store::open_in_memory()?;
    let session = store.start_session("Screen question", None)?;
    let screen = ScreenContext {
        id: "capture-1".to_owned(),
        source: cueglint_core::ScreenSource::Display(2),
        captured_at: 100,
        width: 1920,
        height: 1080,
        sha256: "a".repeat(64),
        pixel_sha256: None,
        text: "A visible error message".to_owned(),
    };
    let request = store.begin_request_with_screen(
        &session.id,
        "What should I check?",
        Some(screen.clone()),
    )?;
    assert_eq!(request.context.screen, Some(screen));
    let mut metadata = InferenceMetadata {
        model_profile: "test-model".to_owned(),
        model_revision: "a".repeat(40),
        model_sha256: "b".repeat(64),
        tokenizer_sha256: "c".repeat(64),
        backend: "unit-test-worker".to_owned(),
        prompt_tokens: 128,
        generated_tokens: 20,
        max_new_tokens: 32,
        elapsed_ms: 25,
        finish_reason: "cancelled".to_owned(),
        omitted_transcript_segments: 0,
        screen_truncated: false,
        omitted_chat_turns: 0,
        prompt_sha256: None,
        template_revision: None,
        decoder_policy_revision: None,
    };
    assert!(
        store
            .complete_request_with_metadata(
                &session.id,
                &request.id,
                "Partial output",
                metadata.clone()
            )
            .is_err()
    );
    metadata.finish_reason = "end_of_turn".to_owned();
    assert!(store.complete_request_with_metadata(
        &session.id,
        &request.id,
        "A test result",
        metadata.clone()
    )?);
    let backup = store.backup_json()?;
    let mut restored = Store::open_in_memory()?;
    restored.restore_backup(&backup, false)?;
    assert_eq!(
        restored.session(&session.id)?.suggestions[0].inference,
        Some(metadata)
    );
    Ok(())
}

#[test]
fn a_saved_session_resumes_its_identity_without_replacing_another_active_session()
-> anyhow::Result<()> {
    let mut store = Store::open_in_memory()?;
    let session = store.start_session("Original call", Some("meeting"))?;
    store.upsert_transcript(&session.id, update("first", "Original transcript", true))?;
    store.update_notes(&session.id, "Keep this note")?;
    store.stop_session(&session.id)?;
    store.save_session(&session.id)?;
    let resumed = store.resume_session(&session.id)?;
    assert_eq!(resumed.id, session.id);
    assert_eq!(resumed.notes, "Keep this note");
    assert_eq!(resumed.transcript.len(), 1);
    assert!(resumed.saved_at.is_some());
    store.stop_session(&session.id)?;
    let another = store.start_session("Another call", None)?;
    assert!(store.resume_session(&session.id).is_err());
    assert_eq!(store.session(&another.id)?.status, SessionStatus::Recording);
    assert_eq!(store.session(&session.id)?.status, SessionStatus::Stopped);
    Ok(())
}

#[test]
fn partial_hypotheses_are_transient_and_final_audio_provenance_is_durable() -> anyhow::Result<()> {
    let path = temporary_db();
    let session_id;
    {
        let mut store = Store::open(&path)?;
        session_id = store.start_session("Transient hypotheses", None)?.id;
        let mut partial = update("audio-segment", "An evolving hypothesis", false);
        partial.source = TranscriptSource::Microphone;
        let mut audio = AudioMetadata {
            start_ns: 1_000,
            end_ns: 1_000_001_000,
            sample_rate: 16_000,
            model_profile: "whisper-tiny-en".to_owned(),
            model_revision: "a".repeat(40),
            model_sha256: "b".repeat(64),
            language: "en".to_owned(),
            backend: None,
            frontend_revision: None,
            frontend_config_sha256: None,
        };
        store.upsert_transcript_with_audio(&session_id, partial.clone(), audio.clone())?;
        assert_eq!(store.session(&session_id)?.transcript.len(), 1);
        assert!(store.sessions("evolving")?.is_empty());
        assert!(!store.backup_json()?.contains("evolving"));
        assert!(
            !store
                .export_session(&session_id, ExportFormat::Json)?
                .contains("evolving")
        );
        store.update_notes(&session_id, "Save notes while a hypothesis is visible")?;
        assert!(!store.backup_json()?.contains("evolving"));
        partial.text = "A finalized utterance".to_owned();
        partial.finalized = true;
        audio.end_ns = 2_000_001_000;
        let finalized = store.upsert_transcript_with_audio(&session_id, partial, audio.clone())?;
        assert_eq!(finalized.transcript.len(), 1);
        assert_eq!(finalized.transcript[0].audio, Some(audio));
        store.upsert_transcript(
            &session_id,
            update("unsaved", "Discard this hypothesis", false),
        )?;
    }
    {
        let store = Store::open(&path)?;
        let session = store.session(&session_id)?;
        assert_eq!(session.transcript.len(), 1);
        assert_eq!(session.transcript[0].text, "A finalized utterance");
        assert!(session.transcript[0].audio.is_some());
        assert!(session.notes.contains("Save notes"));
    }
    std::fs::remove_dir_all(path.parent().unwrap())?;
    Ok(())
}

#[test]
fn captured_turns_order_by_capture_time_and_do_not_claim_speaker_identity() -> anyhow::Result<()> {
    let mut store = Store::open_in_memory()?;
    let session = store.start_session("Overlapping sources", None)?;
    let mut audio = AudioMetadata {
        start_ns: 2_000_000_000,
        end_ns: 3_000_000_000,
        sample_rate: 16_000,
        model_profile: "whisper-tiny-en".to_owned(),
        model_revision: "a".repeat(40),
        model_sha256: "b".repeat(64),
        language: "en".to_owned(),
        backend: None,
        frontend_revision: None,
        frontend_config_sha256: None,
    };
    let mut later = update("later-completed-first", "Later captured turn", true);
    later.speaker = Speaker::Unknown;
    later.source = TranscriptSource::Microphone;
    store.upsert_transcript_with_audio(&session.id, later, audio.clone())?;
    audio.start_ns = 0;
    audio.end_ns = 1_000_000_000;
    let mut earlier = update("earlier-completed-last", "Earlier captured turn", true);
    earlier.speaker = Speaker::Unknown;
    earlier.source = TranscriptSource::SystemAudio;
    let saved = store.upsert_transcript_with_audio(&session.id, earlier, audio)?;
    assert_eq!(saved.transcript[0].id, "earlier-completed-last");
    assert_eq!(
        store
            .begin_request(&session.id, "What was said?")?
            .context
            .transcript[0]
            .id,
        "earlier-completed-last"
    );
    assert!(
        store
            .export_session(&session.id, ExportFormat::Markdown)?
            .contains("Unknown speaker")
    );
    Ok(())
}

#[test]
fn followups_freeze_only_the_latest_eight_completed_chat_turns() -> anyhow::Result<()> {
    let mut store = Store::open_in_memory()?;
    let session = store.start_session("Follow-up history", None)?;
    let mut previous_revision = 0;
    for index in 0..10 {
        let request = store.begin_request(&session.id, &format!("Question {index}"))?;
        assert!(request.context.revision > previous_revision);
        previous_revision = request.context.revision;
        store.complete_request(&session.id, &request.id, &format!("Test response {index}"))?;
    }
    let pending = store.begin_request(&session.id, "A cancelled question")?;
    store.cancel_request(&session.id, &pending.id)?;
    let followup = store.begin_request(&session.id, "What did I just ask?")?;
    assert_eq!(followup.context.chat.len(), 8);
    assert_eq!(followup.context.chat[0].prompt, "Question 2");
    assert_eq!(followup.context.chat[7].prompt, "Question 9");
    assert_eq!(followup.context.chat[7].response, "Test response 9");
    let backup = store.backup_json()?;
    let mut target = Store::open_in_memory()?;
    target.restore_backup(&backup, false)?;
    assert_eq!(
        target
            .session(&session.id)?
            .requests
            .last()
            .unwrap()
            .context
            .chat,
        followup.context.chat
    );
    Ok(())
}
