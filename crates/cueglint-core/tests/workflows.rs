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
    assert_eq!(json["version"], 1);
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
fn screen_identity_and_actual_inference_metadata_are_preserved() -> anyhow::Result<()> {
    let mut store = Store::open_in_memory()?;
    let session = store.start_session("Screen question", None)?;
    let screen = ScreenContext {
        id: "capture-1".to_owned(),
        display_id: 2,
        captured_at: 100,
        width: 1920,
        height: 1080,
        sha256: "a".repeat(64),
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
