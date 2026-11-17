use cueglint_core::{
    CONTEXT_TEXT_BYTES, ExportFormat, InferenceMetadata, Mode, Speaker, Store, TranscriptSource,
    TranscriptUpdate,
};

fn segment(id: String, text: String) -> TranscriptUpdate {
    TranscriptUpdate {
        id,
        speaker: Speaker::Unknown,
        text,
        finalized: true,
        source: TranscriptSource::Manual,
    }
}

#[test]
fn a_thousand_segment_meeting_keeps_a_fixed_snapshot_bound_across_a_hundred_requests()
-> anyhow::Result<()> {
    let mut store = Store::open_in_memory()?;
    let session = store.start_session("Original synthetic long-meeting fixture", None)?;
    store.update_notes(
        &session.id,
        "Human notes must survive every context selection.",
    )?;
    for index in 0..1_000 {
        let prefix = format!("Segment {index:04}: ");
        let text = format!("{prefix}{}", "é".repeat((256 - prefix.len()) / 2));
        assert_eq!(text.len(), 256);
        store.upsert_transcript(&session.id, segment(format!("segment-{index:04}"), text))?;
    }
    let original = store.session(&session.id)?;
    let mut max_snapshot_bytes = 0;
    for _ in 0..100 {
        let request =
            store.begin_request(&session.id, "Summarize the newest available discussion.")?;
        assert_eq!(request.context.selection_version, 1);
        assert_eq!(request.context.transcript.len(), 64);
        assert_eq!(request.context.omitted_transcript_segments, 936);
        assert_eq!(
            request
                .context
                .transcript
                .iter()
                .map(|s| s.text.len())
                .sum::<usize>(),
            CONTEXT_TEXT_BYTES
        );
        assert_eq!(request.context.transcript, original.transcript[936..]);
        assert!(request.context.diagnostics.is_empty());
        max_snapshot_bytes = max_snapshot_bytes.max(serde_json::to_vec(&request.context)?.len());
    }
    let stored = store.session(&session.id)?;
    assert_eq!(stored.transcript, original.transcript);
    assert_eq!(stored.notes, original.notes);
    assert_eq!(stored.requests.len(), 100);
    let copied_text: usize = stored
        .requests
        .iter()
        .flat_map(|r| &r.context.transcript)
        .map(|s| s.text.len())
        .sum();
    assert_eq!(copied_text, 100 * CONTEXT_TEXT_BYTES);
    assert!(max_snapshot_bytes < 64 * 1024);
    let backup = store.backup_json()?;
    let mut restored = Store::open_in_memory()?;
    restored.restore_backup(&backup, false)?;
    assert_eq!(
        restored.session(&session.id)?.transcript,
        original.transcript
    );
    assert_eq!(restored.session(&session.id)?.notes, original.notes);
    println!(
        "BOUNDED_CONTEXT_EVIDENCE {}",
        serde_json::json!({
            "fixture": "original synthetic manual UTF-8 transcript; not ASR or reference parity",
            "source_segments": 1000, "requests": 100, "source_text_bytes": 256000,
            "max_selected_segments": 64, "selected_text_bytes_per_request": CONTEXT_TEXT_BYTES,
            "total_copied_transcript_text_bytes": copied_text, "max_serialized_snapshot_bytes": max_snapshot_bytes,
            "full_transcript_and_notes_preserved": true, "backup_round_trip_passed": true,
        })
    );
    Ok(())
}

#[test]
fn whole_utf8_segments_form_a_contiguous_newest_suffix_and_oversize_is_explicit()
-> anyhow::Result<()> {
    let mut store = Store::open_in_memory()?;
    let session = store.start_session("Whole UTF-8 utterances", None)?;
    for index in 0..3 {
        store.upsert_transcript(
            &session.id,
            segment(format!("segment-{index}"), "é".repeat(4_096)),
        )?;
    }
    let request = store.begin_request(&session.id, "What was said?")?;
    assert_eq!(request.context.omitted_transcript_segments, 1);
    assert_eq!(request.context.transcript.len(), 2);
    assert!(
        request
            .context
            .transcript
            .iter()
            .all(|s| s.text == "é".repeat(4_096))
    );
    let oversized = "é".repeat(8_193);
    store.upsert_transcript(
        &session.id,
        segment("segment-3".to_owned(), oversized.clone()),
    )?;
    let request = store.begin_request(&session.id, "What was said most recently?")?;
    assert!(
        request.context.transcript.is_empty(),
        "older text must not replace an oversized newest utterance"
    );
    assert_eq!(request.context.omitted_transcript_segments, 4);
    assert!(request.context.diagnostics[0].contains("Newest transcript segment"));
    assert_eq!(
        store.session(&session.id)?.transcript.last().unwrap().text,
        oversized
    );
    Ok(())
}

#[test]
fn chat_budget_counts_both_whole_prompt_and_response_and_reports_an_oversized_newest_turn()
-> anyhow::Result<()> {
    let mut store = Store::open_in_memory()?;
    let session = store.start_session("Bounded completed chat", None)?;
    for _ in 0..3 {
        let request = store.begin_request(&session.id, &"q".repeat(4_096))?;
        store.complete_request(&session.id, &request.id, &"a".repeat(4_096))?;
    }
    let request = store.begin_request(&session.id, "Follow up")?;
    assert_eq!(request.context.chat.len(), 2);
    assert_eq!(request.context.omitted_chat_turns, 1);
    assert_eq!(
        request
            .context
            .chat
            .iter()
            .map(|t| t.prompt.len() + t.response.len())
            .sum::<usize>(),
        CONTEXT_TEXT_BYTES
    );
    store.complete_request(&session.id, &request.id, &"a".repeat(CONTEXT_TEXT_BYTES))?;
    let request = store.begin_request(&session.id, "One more question")?;
    assert!(request.context.chat.is_empty());
    assert_eq!(request.context.omitted_chat_turns, 4);
    assert!(request.context.diagnostics[0].contains("Newest completed chat turn"));
    let mut restored = Store::open_in_memory()?;
    restored.restore_backup(&store.backup_json()?, false)?;
    assert_eq!(restored.session(&session.id)?.suggestions.len(), 4);
    Ok(())
}

#[test]
fn old_unbounded_snapshots_restore_unchanged_but_new_requests_are_bounded() -> anyhow::Result<()> {
    let mut source = Store::open_in_memory()?;
    let session = source.start_session("Legacy snapshot", None)?;
    for index in 0..70 {
        source.upsert_transcript(
            &session.id,
            segment(format!("segment-{index:03}"), format!("Statement {index}")),
        )?;
    }
    source.update_notes(&session.id, "Original notes")?;
    source.begin_request(&session.id, "An old question")?;
    let mut backup: serde_json::Value = serde_json::from_str(&source.backup_json()?)?;
    let original = backup["sessions"][0]["transcript"].clone();
    let context = &mut backup["sessions"][0]["requests"][0]["context"];
    context["transcript"] = original;
    for key in [
        "selection_version",
        "omitted_transcript_segments",
        "omitted_chat_turns",
        "diagnostics",
    ] {
        context.as_object_mut().unwrap().remove(key);
    }
    let mut target = Store::open_in_memory()?;
    target.restore_backup(&backup.to_string(), false)?;
    let old = &target.session(&session.id)?.requests[0].context;
    assert_eq!(old.selection_version, 0);
    assert_eq!(old.transcript.len(), 70);
    let new = target.begin_request(&session.id, "A current question")?;
    assert_eq!(new.context.selection_version, 1);
    assert_eq!(new.context.transcript.len(), 64);
    assert_eq!(new.context.omitted_transcript_segments, 6);
    assert_eq!(target.session(&session.id)?.notes, "Original notes");
    Ok(())
}

#[test]
fn impossible_omissions_or_partial_prompt_identity_are_rejected_without_overwriting_history()
-> anyhow::Result<()> {
    let mut store = Store::open_in_memory()?;
    let session = store.start_session("Metadata validation", None)?;
    store.upsert_transcript(
        &session.id,
        segment("oversized".to_owned(), "x".repeat(CONTEXT_TEXT_BYTES + 1)),
    )?;
    let request = store.begin_request(&session.id, "Explain available context")?;
    let mut metadata = InferenceMetadata {
        model_profile: "fixture-worker".to_owned(),
        model_revision: "a".repeat(40),
        model_sha256: "b".repeat(64),
        tokenizer_sha256: "c".repeat(64),
        backend: "unit-test-worker".to_owned(),
        prompt_tokens: 100,
        generated_tokens: 5,
        max_new_tokens: 32,
        elapsed_ms: 10,
        finish_reason: "end_of_turn".to_owned(),
        omitted_transcript_segments: 0,
        omitted_chat_turns: 0,
        screen_truncated: false,
        prompt_sha256: Some("d".repeat(64)),
        template_revision: Some("test-template-v1".to_owned()),
    };
    assert!(
        store
            .complete_request_with_metadata(
                &session.id,
                &request.id,
                "A fixture response",
                metadata.clone()
            )
            .is_err()
    );
    metadata.omitted_transcript_segments = 1;
    metadata.template_revision = None;
    assert!(
        store
            .complete_request_with_metadata(
                &session.id,
                &request.id,
                "A fixture response",
                metadata.clone()
            )
            .is_err()
    );
    metadata.template_revision = Some("test-template-v1".to_owned());
    assert!(store.complete_request_with_metadata(
        &session.id,
        &request.id,
        "A fixture response",
        metadata
    )?);
    let mut invalid: serde_json::Value = serde_json::from_str(&store.backup_json()?)?;
    invalid["sessions"][0]["requests"][0]["context"]["omitted_transcript_segments"] = 9999.into();
    assert!(store.restore_backup(&invalid.to_string(), true).is_err());
    assert_eq!(store.session(&session.id)?.transcript.len(), 1);
    assert_eq!(store.session(&session.id)?.suggestions.len(), 1);
    Ok(())
}

#[test]
fn legacy_completed_requests_without_an_answer_do_not_break_future_backup_round_trips()
-> anyhow::Result<()> {
    let mut source = Store::open_in_memory()?;
    let session = source.start_session("Legacy missing response", None)?;
    source.update_notes(&session.id, "Keep the existing human note")?;
    let request = source.begin_request(&session.id, "An old question")?;
    source.complete_request(
        &session.id,
        &request.id,
        "An answer absent from the legacy archive",
    )?;
    let mut backup: serde_json::Value = serde_json::from_str(&source.backup_json()?)?;
    backup["sessions"][0]["suggestions"] = serde_json::json!([]);
    let context = &mut backup["sessions"][0]["requests"][0]["context"];
    for key in [
        "selection_version",
        "omitted_transcript_segments",
        "omitted_chat_turns",
        "diagnostics",
    ] {
        context.as_object_mut().unwrap().remove(key);
    }
    let mut target = Store::open_in_memory()?;
    target.restore_backup(&backup.to_string(), false)?;
    let current = target.begin_request(&session.id, "A current question")?;
    assert!(current.context.chat.is_empty());
    assert_eq!(current.context.omitted_chat_turns, 0);
    let mut round_trip = Store::open_in_memory()?;
    round_trip.restore_backup(&target.backup_json()?, false)?;
    assert_eq!(
        round_trip.session(&session.id)?.notes,
        "Keep the existing human note"
    );
    assert_eq!(round_trip.session(&session.id)?.requests.len(), 2);
    Ok(())
}

#[test]
fn exhausted_revisions_return_errors_and_roll_back_every_context_mutation() -> anyhow::Result<()> {
    let mut source = Store::open_in_memory()?;
    let mode = source.upsert_mode(Mode::new(
        "Fixture mode",
        "Use the fictional reference text.",
    ))?;
    let session = source.start_session("Exhausted revision fixture", Some(&mode.id))?;
    source.update_notes(&session.id, "Original human note")?;
    source.upsert_transcript(
        &session.id,
        segment("original".to_owned(), "An existing statement".to_owned()),
    )?;
    let document = source.ingest_knowledge(
        "Fixture policy",
        "fixture.txt",
        "The fictional policy color is blue.",
    )?;
    source.begin_request(&session.id, "What is the policy color?")?;
    let mut backup: serde_json::Value = serde_json::from_str(&source.backup_json()?)?;
    backup["sessions"][0]["context_revision"] = u64::MAX.into();
    let mut target = Store::open_in_memory()?;
    target.restore_backup(&backup.to_string(), false)?;
    target.resume_session(&session.id)?;
    let before = target.session(&session.id)?;
    let error = target
        .begin_request(&session.id, "A new question")
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("exhausted its context revisions")
    );
    assert_eq!(target.session(&session.id)?, before);
    assert!(
        target
            .upsert_transcript(
                &session.id,
                segment("new".to_owned(), "Must not replace history".to_owned())
            )
            .is_err()
    );
    assert_eq!(target.session(&session.id)?, before);
    assert!(
        target
            .set_session_mode(&session.id, Some("general"))
            .is_err()
    );
    assert_eq!(target.session(&session.id)?, before);
    assert!(target.delete_mode(&mode.id).is_err());
    assert_eq!(target.mode(&mode.id)?, mode);
    assert_eq!(target.session(&session.id)?, before);
    assert!(target.delete_knowledge(&document.id).is_err());
    assert_eq!(target.knowledge()?, vec![document]);
    assert_eq!(target.session(&session.id)?, before);
    target.update_notes(&session.id, "Notes remain editable after a revision error")?;
    assert!(
        target
            .export_session(&session.id, ExportFormat::Json)?
            .contains("Notes remain editable")
    );
    let mut round_trip = Store::open_in_memory()?;
    round_trip.restore_backup(&target.backup_json()?, false)?;
    assert_eq!(round_trip.session(&session.id)?.context_revision, u64::MAX);
    Ok(())
}
