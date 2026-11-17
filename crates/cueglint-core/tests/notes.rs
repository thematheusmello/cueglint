use cueglint_core::*;
use serde_json::{Value, json};

fn prepared(texts: &[&str]) -> (Store, String, NotesPlan) {
    let mut store = Store::open_in_memory().unwrap();
    let session = store
        .start_session("Original human title", Some("meeting"))
        .unwrap();
    store
        .update_notes(&session.id, "Human notes\r\nKeep café 🦀 exactly.")
        .unwrap();
    store
        .update_summary(&session.id, "Legacy human summary stays unchanged.")
        .unwrap();
    for (index, text) in texts.iter().enumerate() {
        store
            .upsert_transcript(
                &session.id,
                TranscriptUpdate {
                    id: format!("utterance-{index:03}"),
                    speaker: Speaker::Unknown,
                    text: text.to_string(),
                    finalized: true,
                    source: TranscriptSource::Manual,
                },
            )
            .unwrap();
    }
    store.stop_session(&session.id).unwrap();
    let input = store.notes_input(&session.id).unwrap();
    let chunks = (0..texts.len())
        .collect::<Vec<_>>()
        .chunks(2)
        .map(|chunk| chunk.to_vec())
        .collect();
    (
        store,
        session.id,
        NotesPlan {
            source: input.source,
            chunks,
        },
    )
}

fn supplied(selections: Vec<Vec<usize>>) -> NotesCompletion {
    NotesCompletion {
        selections,
        provenance: NotesProvenance::Supplied {},
    }
}

fn accept(store: &mut Store, session: &str, plan: NotesPlan) -> String {
    let selections = plan.chunks.iter().map(|chunk| vec![chunk[0]]).collect();
    let job = store.begin_notes(session, plan).unwrap();
    assert!(
        store
            .complete_notes(session, &job.id, supplied(selections))
            .unwrap()
    );
    store.accept_notes(session, &job.id).unwrap();
    job.id
}

#[test]
fn review_never_replaces_user_text_and_repeated_accept_cannot_reset_apply_once() {
    let (mut store, id, plan) = prepared(&[
        "Maybe Ana could send this, if approved.",
        "Nobody committed to a deadline.",
        "Keep **literal** text and é\r\nnewlines.",
    ]);
    let original = store.session(&id).unwrap();
    let job = store.begin_notes(&id, plan.clone()).unwrap();
    assert!(store.begin_notes(&id, plan.clone()).is_err());
    let edited = "New human edit while a worker runs.\r\nDo not replace this.";
    let revision = store.update_notes(&id, edited).unwrap().notes_revision;
    assert!(
        store
            .complete_notes(&id, &job.id, supplied(vec![vec![1, 0], vec![2]]))
            .unwrap()
    );
    let ready = store.session(&id).unwrap();
    assert_eq!(ready.notes, edited);
    assert_eq!(ready.summary, original.summary);
    assert_eq!(ready.title, original.title);
    assert_eq!(
        ready
            .notes_job
            .as_ref()
            .unwrap()
            .candidate
            .as_ref()
            .unwrap()
            .highlights
            .iter()
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        [
            "Maybe Ana could send this, if approved.",
            "Nobody committed to a deadline.",
            "Keep **literal** text and é\r\nnewlines."
        ]
    );
    assert!(
        store.begin_notes(&id, plan.clone()).is_err(),
        "unreviewed drafts cannot be overwritten"
    );
    let accepted = store.accept_notes(&id, &job.id).unwrap();
    assert_eq!(accepted.notes, edited);
    assert_eq!(accepted.notes_revision, revision);
    let NotesAppendOutcome::Applied { session: appended } = store
        .append_generated_notes(&id, &job.id, revision)
        .unwrap()
    else {
        panic!("expected append")
    };
    assert!(appended.notes.starts_with(&format!("{edited}\n\n")));
    assert_eq!(appended.notes_revision, revision + 1);
    assert_eq!(
        appended
            .generated_notes
            .as_ref()
            .unwrap()
            .applied_notes_revision,
        Some(revision + 1)
    );
    let again = store.accept_notes(&id, &job.id).unwrap();
    assert_eq!(
        again, appended,
        "idempotent acceptance must preserve applied-once metadata and timestamps"
    );
    let NotesAppendOutcome::AlreadyApplied { session } =
        store.append_generated_notes(&id, &job.id, 0).unwrap()
    else {
        panic!("retry must be a no-op even with an old revision")
    };
    assert_eq!(session, appended);
    let next = store.begin_notes(&id, plan).unwrap();
    assert!(
        store
            .complete_notes(&id, &next.id, supplied(vec![vec![0], vec![]]))
            .unwrap()
    );
    let discarded = store.discard_notes(&id, &next.id).unwrap();
    assert_eq!(store.discard_notes(&id, &next.id).unwrap(), discarded);
    assert_eq!(discarded.generated_notes, appended.generated_notes);
    assert_eq!(discarded.notes, appended.notes);
    assert!(store.discard_notes(&id, &job.id).is_err());
    assert!(store.accept_notes(&id, &next.id).is_err());
}

#[test]
fn append_revision_conflicts_wrong_ids_and_size_errors_preserve_all_user_data() {
    let (mut store, id, plan) = prepared(&["An original whole utterance."]);
    let generated = accept(&mut store, &id, plan);
    let before = store.session(&id).unwrap();
    store
        .update_notes(&id, "An intervening human edit.")
        .unwrap();
    let edited = store.session(&id).unwrap();
    let NotesAppendOutcome::Conflict { session } = store
        .append_generated_notes(&id, &generated, before.notes_revision)
        .unwrap()
    else {
        panic!("expected revision conflict")
    };
    assert_eq!(session, edited);
    assert_eq!(store.session(&id).unwrap(), edited);
    assert!(
        store
            .append_generated_notes(&id, "wrong-record", edited.notes_revision)
            .is_err()
    );
    store
        .update_notes(&id, &"x".repeat(MAX_DOCUMENT_BYTES))
        .unwrap();
    let full = store.session(&id).unwrap();
    assert!(
        store
            .append_generated_notes(&id, &generated, full.notes_revision)
            .is_err()
    );
    assert_eq!(store.session(&id).unwrap(), full);
    let mut backup: Value = serde_json::from_str(&store.backup_json().unwrap()).unwrap();
    backup["sessions"][0]["notes_revision"] = u64::MAX.into();
    store.restore_backup(&backup.to_string(), true).unwrap();
    let exhausted = store.session(&id).unwrap();
    assert!(store.update_notes(&id, "A revision cannot wrap.").is_err());
    assert!(
        store
            .append_generated_notes(&id, &generated, u64::MAX)
            .is_err()
    );
    assert_eq!(store.session(&id).unwrap(), exhausted);
}

#[test]
fn coverage_is_full_ordered_whole_input_and_has_no_token_capacity_claim() {
    let (mut store, id, plan) = prepared(&["First", "Second", "Third"]);
    for chunks in [
        vec![],
        vec![vec![]],
        vec![vec![0, 1]],
        vec![vec![0, 1], vec![1, 2]],
        vec![vec![1, 0, 2]],
        vec![vec![0, 1, 2, 3]],
    ] {
        let mut bad = plan.clone();
        bad.chunks = chunks;
        let before = store.session(&id).unwrap();
        assert!(store.begin_notes(&id, bad).is_err());
        assert_eq!(store.session(&id).unwrap(), before);
    }
    let mut bad = plan.clone();
    bad.source.session_id = "another-session".into();
    assert!(store.begin_notes(&id, bad).is_err());
    let mut bad = plan.clone();
    bad.source.sha256 = "0".repeat(64);
    assert!(store.begin_notes(&id, bad).is_err());
    let mut bad = plan.clone();
    bad.source.segment_ids.swap(0, 1);
    assert!(store.begin_notes(&id, bad).is_err());
    let job = store.begin_notes(&id, plan).unwrap();
    for selections in [
        vec![],
        vec![vec![0]],
        vec![vec![2], vec![0]],
        vec![vec![0, 0], vec![2]],
        vec![vec![99], vec![2]],
        vec![vec![0], vec![2], vec![]],
    ] {
        assert!(
            store
                .complete_notes(&id, &job.id, supplied(selections))
                .is_err()
        );
        assert_eq!(
            store.session(&id).unwrap().notes_job.unwrap().status,
            NotesJobStatus::Pending
        );
    }
    assert!(
        store
            .complete_notes(&id, &job.id, supplied(vec![vec![0], vec![2]]))
            .unwrap()
    );

    let large = "x".repeat(65_536);
    let (mut store, id, mut plan) = prepared(&[&large, &large]);
    assert_eq!(plan.source.text_bytes, NOTES_MAX_SOURCE_BYTES);
    plan.chunks = vec![vec![0, 1]];
    assert!(
        store.begin_notes(&id, plan).is_ok(),
        "structural coverage is not a claim that a model tokenizer can fit this chunk"
    );
    let mut store = Store::open_in_memory().unwrap();
    let session = store.start_session("Too large", None).unwrap();
    for index in 0..3 {
        store
            .upsert_transcript(
                &session.id,
                TranscriptUpdate {
                    id: format!("large-{index}"),
                    speaker: Speaker::Unknown,
                    text: large.clone(),
                    finalized: true,
                    source: TranscriptSource::Manual,
                },
            )
            .unwrap();
    }
    store.stop_session(&session.id).unwrap();
    assert!(store.notes_input(&session.id).is_err());
    assert!(store.session(&session.id).unwrap().notes_job.is_none());

    let texts = ["utterance"; 17];
    let (mut store, id, mut plan) = prepared(&texts);
    plan.chunks = (0..17).map(|index| vec![index]).collect();
    assert!(store.begin_notes(&id, plan).is_err());

    let texts = ["utterance"; 9];
    let (mut store, id, mut plan) = prepared(&texts);
    plan.chunks = vec![(0..9).collect()];
    let job = store.begin_notes(&id, plan).unwrap();
    assert!(
        store
            .complete_notes(&id, &job.id, supplied(vec![(0..9).collect()]))
            .is_err()
    );
    assert!(
        store
            .complete_notes(&id, &job.id, supplied(vec![(0..8).collect()]))
            .unwrap()
    );
    let ready = store.session(&id).unwrap().notes_job.unwrap();
    assert_eq!(
        ready.plan.source.segment_ids.len(),
        9,
        "coverage describes all input, not only selected highlights"
    );
    assert_eq!(
        ready.candidate.unwrap().highlights.len(),
        NOTES_MAX_SELECTIONS_PER_CHUNK
    );

    let mut empty = Store::open_in_memory().unwrap();
    let session = empty.start_session("No transcript", None).unwrap();
    assert!(empty.notes_input(&session.id).is_err());
    empty.stop_session(&session.id).unwrap();
    assert!(empty.notes_input(&session.id).is_err());
    assert!(empty.session(&session.id).unwrap().notes_job.is_none());
}

#[test]
fn cancelled_resumed_cross_session_and_deleted_jobs_cannot_publish_late_results() {
    let (mut store, id, plan) = prepared(&["Before resume.", "Tentative wording stays tentative."]);
    let job = store.begin_notes(&id, plan.clone()).unwrap();
    store.resume_session(&id).unwrap();
    assert_eq!(
        store.session(&id).unwrap().notes_job.unwrap().cancel_reason,
        Some(NotesCancelReason::Resumed)
    );
    assert!(
        !store
            .complete_notes(&id, &job.id, supplied(vec![vec![0]]))
            .unwrap()
    );
    store
        .upsert_transcript(
            &id,
            TranscriptUpdate {
                id: "new-final".into(),
                speaker: Speaker::Unknown,
                text: "New source after resume.".into(),
                finalized: true,
                source: TranscriptSource::Manual,
            },
        )
        .unwrap();
    store.stop_session(&id).unwrap();
    assert_eq!(
        store.notes_freshness(&id, &plan.source).unwrap(),
        NotesFreshness::Historical
    );
    assert!(store.begin_notes(&id, plan).is_err());
    let input = store.notes_input(&id).unwrap();
    let next = store
        .begin_notes(
            &id,
            NotesPlan {
                source: input.source,
                chunks: vec![vec![0, 1, 2]],
            },
        )
        .unwrap();
    let before = store.session(&id).unwrap();
    assert_eq!(
        store
            .cancel_notes(&id, &job.id, NotesCancelReason::User)
            .unwrap(),
        before
    );
    assert!(
        !store
            .complete_notes("missing-session", &next.id, supplied(vec![vec![0]]))
            .unwrap()
    );
    let other = store.start_session("Another session", None).unwrap();
    store.stop_session(&other.id).unwrap();
    assert!(
        !store
            .complete_notes(&other.id, &next.id, supplied(vec![vec![0]]))
            .unwrap()
    );
    store
        .cancel_notes(&id, &next.id, NotesCancelReason::User)
        .unwrap();
    assert!(
        !store
            .complete_notes(&id, &next.id, supplied(vec![vec![0]]))
            .unwrap()
    );
    let input = store.notes_input(&id).unwrap();
    let last = store
        .begin_notes(
            &id,
            NotesPlan {
                source: input.source,
                chunks: vec![vec![0, 1, 2]],
            },
        )
        .unwrap();
    store.delete_session(&id).unwrap();
    assert!(
        !store
            .complete_notes(&id, &last.id, supplied(vec![vec![0]]))
            .unwrap()
    );
}

#[test]
fn ready_and_accepted_drafts_survive_resume_as_historical_manual_snapshots() {
    let (mut store, id, plan) = prepared(&["Maybe someone can help; nothing was promised."]);
    let job = store.begin_notes(&id, plan.clone()).unwrap();
    store
        .complete_notes(&id, &job.id, supplied(vec![vec![0]]))
        .unwrap();
    let ready = store
        .session(&id)
        .unwrap()
        .notes_job
        .unwrap()
        .candidate
        .unwrap();
    store.resume_session(&id).unwrap();
    assert_eq!(
        store.notes_freshness(&id, &plan.source).unwrap(),
        NotesFreshness::Historical
    );
    assert_eq!(
        store
            .session(&id)
            .unwrap()
            .notes_job
            .unwrap()
            .candidate
            .unwrap(),
        ready
    );
    store.accept_notes(&id, &job.id).unwrap();
    let current = store.session(&id).unwrap();
    assert!(matches!(
        store
            .append_generated_notes(&id, &job.id, current.notes_revision)
            .unwrap(),
        NotesAppendOutcome::Applied { .. }
    ));
    assert!(
        store
            .export_session(&id, ExportFormat::Markdown)
            .unwrap()
            .contains("historical transcript snapshot")
    );
    store.stop_session(&id).unwrap();
    assert_eq!(
        store.notes_freshness(&id, &plan.source).unwrap(),
        NotesFreshness::Current
    );
}

fn metadata() -> InferenceMetadata {
    InferenceMetadata {
        model_profile: "declared-test-model".into(),
        model_revision: "a".repeat(40),
        model_sha256: "b".repeat(64),
        tokenizer_sha256: "c".repeat(64),
        backend: "declared-test-backend".into(),
        prompt_tokens: 64,
        generated_tokens: 8,
        max_new_tokens: 128,
        elapsed_ms: 1,
        finish_reason: "end_of_turn".into(),
        omitted_transcript_segments: 0,
        omitted_chat_turns: 0,
        screen_truncated: false,
        prompt_sha256: Some("d".repeat(64)),
        template_revision: Some("test-notes-selection-v1".into()),
        decoder_policy_revision: None,
    }
}

#[test]
fn decoder_policy_is_optional_for_legacy_records_and_survives_v2_backup_and_export() {
    for policy in [None, Some("original-test-decoder-v1".to_owned())] {
        let mut run = metadata();
        run.decoder_policy_revision = policy.clone();
        let serialized = serde_json::to_value(&run).unwrap();
        assert_eq!(
            serialized.get("decoder_policy_revision").is_some(),
            policy.is_some()
        );
        assert_eq!(
            serde_json::from_value::<InferenceMetadata>(serialized).unwrap(),
            run
        );

        let (mut store, id, plan) = prepared(&["Remember the earlier fact.", "A later fact."]);
        let job = store.begin_notes(&id, plan).unwrap();
        assert!(
            store
                .complete_notes(
                    &id,
                    &job.id,
                    NotesCompletion {
                        selections: vec![vec![0]],
                        provenance: NotesProvenance::Local { runs: vec![run] },
                    },
                )
                .unwrap()
        );
        let original = store.session(&id).unwrap();
        let export: Value =
            serde_json::from_str(&store.export_session(&id, ExportFormat::Json).unwrap()).unwrap();
        let exported_run = &export["notes_job"]["candidate"]["provenance"]["runs"][0];
        assert_eq!(
            exported_run.get("decoder_policy_revision").is_some(),
            policy.is_some()
        );
        if let Some(policy) = &policy {
            assert_eq!(exported_run["decoder_policy_revision"], policy.as_str());
        }
        let backup = store.backup_json().unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&backup).unwrap()["version"],
            2
        );
        let mut restored = Store::open_in_memory().unwrap();
        restored.restore_backup(&backup, false).unwrap();
        assert_eq!(restored.session(&id).unwrap(), original);
    }
}

#[test]
fn invalid_raw_decoder_policies_and_mixed_chunk_policies_cannot_replace_user_data() {
    let (mut source, id, plan) = prepared(&["Earlier.", "Middle.", "Later."]);
    let job = source.begin_notes(&id, plan).unwrap();
    let mut run = metadata();
    run.decoder_policy_revision = Some("original-test-decoder-v1".into());
    source
        .complete_notes(
            &id,
            &job.id,
            NotesCompletion {
                selections: vec![vec![0], vec![2]],
                provenance: NotesProvenance::Local {
                    runs: vec![run.clone(), run],
                },
            },
        )
        .unwrap();
    let backup: Value = serde_json::from_str(&source.backup_json().unwrap()).unwrap();
    let mut target = Store::open_in_memory().unwrap();
    target.restore_backup(&backup.to_string(), false).unwrap();
    let before = target.session(&id).unwrap();
    for invalid in [
        json!(""),
        json!("invalid policy"),
        json!("x".repeat(101)),
        json!(true),
        json!(null),
        json!("different-valid-policy"),
    ] {
        let mut bad = backup.clone();
        bad["sessions"][0]["notes_job"]["candidate"]["provenance"]["runs"][1]["decoder_policy_revision"] =
            invalid;
        assert!(target.restore_backup(&bad.to_string(), true).is_err());
        assert_eq!(target.session(&id).unwrap(), before);
    }
}

#[test]
fn local_provenance_requires_finished_unomitted_identified_runs_for_every_chunk() {
    let (mut store, id, plan) = prepared(&["One", "Two", "Three"]);
    let job = store.begin_notes(&id, plan).unwrap();
    let mut variants = vec![vec![], vec![metadata()]];
    for mutate in 0..6 {
        let mut bad = metadata();
        match mutate {
            0 => bad.finish_reason = "token_limit".into(),
            1 => bad.omitted_transcript_segments = 1,
            2 => bad.omitted_chat_turns = 1,
            3 => bad.screen_truncated = true,
            4 => {
                bad.prompt_sha256 = None;
                bad.template_revision = None;
            }
            _ => bad.model_sha256 = "e".repeat(64),
        }
        variants.push(vec![metadata(), bad]);
    }
    for runs in variants {
        assert!(
            store
                .complete_notes(
                    &id,
                    &job.id,
                    NotesCompletion {
                        selections: vec![vec![0], vec![2]],
                        provenance: NotesProvenance::Local { runs }
                    }
                )
                .is_err()
        );
    }
    assert!(
        store
            .complete_notes(
                &id,
                &job.id,
                NotesCompletion {
                    selections: vec![vec![0], vec![2]],
                    provenance: NotesProvenance::Local {
                        runs: vec![metadata(), metadata()]
                    }
                }
            )
            .unwrap()
    );
    assert!(
        store
            .export_session(&id, ExportFormat::Markdown)
            .unwrap()
            .contains("not an abstractive summary or a quality guarantee")
    );
}

#[test]
fn malformed_v2_notes_restore_is_rejected_before_replacing_existing_data() {
    let (mut source, id, plan) = prepared(&[
        "Maybe this can happen; no commitment.",
        "Keep Unicode é 🦀 exactly.",
    ]);
    let job = source.begin_notes(&id, plan).unwrap();
    source
        .complete_notes(&id, &job.id, supplied(vec![vec![0, 1]]))
        .unwrap();
    let backup: Value = serde_json::from_str(&source.backup_json().unwrap()).unwrap();
    let mut target = Store::open_in_memory().unwrap();
    target.restore_backup(&backup.to_string(), false).unwrap();
    let before = target.session(&id).unwrap();
    for case in 0..10 {
        let mut bad = backup.clone();
        let job = &mut bad["sessions"][0]["notes_job"];
        match case {
            0 => job["candidate"]["highlights"][0]["text"] = "A fabricated promise.".into(),
            1 => job["candidate"]["highlights"][0]["start_byte"] = 1.into(),
            2 => job["plan"]["source"]["digest_version"] = 999.into(),
            3 => job["plan"]["source"]["sha256"] = "0".repeat(64).into(),
            4 => job["plan"]["chunks"] = json!([[0]]),
            5 => job["candidate"]["provenance"]["quality_passed"] = true.into(),
            6 => job["plan"]["unexpected"] = true.into(),
            7 => job["candidate"]["highlights"][0]["segment_id"] = "missing-segment".into(),
            8 => job["candidate"]["highlights"][1] = job["candidate"]["highlights"][0].clone(),
            _ => job["status"] = "pending".into(),
        }
        assert!(
            target.restore_backup(&bad.to_string(), true).is_err(),
            "malformed case {case}"
        );
        assert_eq!(target.session(&id).unwrap(), before);
    }
    source
        .accept_notes(&id, &before.notes_job.as_ref().unwrap().id)
        .unwrap();
    let mut bad: Value = serde_json::from_str(&source.backup_json().unwrap()).unwrap();
    bad["sessions"][0]["generated_notes"]["applied_notes_revision"] = 999.into();
    assert!(target.restore_backup(&bad.to_string(), true).is_err());
}

#[test]
fn restore_checks_notes_sources_against_canonical_order_before_replacing_any_data() {
    let (mut source, id, plan) = prepared(&["Earlier source.", "Later source."]);
    let job = source.begin_notes(&id, plan).unwrap();
    source
        .complete_notes(&id, &job.id, supplied(vec![vec![0, 1]]))
        .unwrap();
    let backup = source.backup_json().unwrap();
    let mut target = Store::open_in_memory().unwrap();
    target.restore_backup(&backup, false).unwrap();
    let keep = target
        .start_session("Keep this target record", None)
        .unwrap()
        .id;
    target
        .update_notes(&keep, "Unrelated user notes stay exact.\r\nCafé 🦀")
        .unwrap();
    target.stop_session(&keep).unwrap();
    let before = target.sessions("").unwrap();

    let mut reordered: Value = serde_json::from_str(&backup).unwrap();
    let session = &mut reordered["sessions"][0];
    let started = session["created_at"].as_i64().unwrap();
    // Timestamps are outside digest v1, but dictate the canonical transcript order.
    for (index, offset) in [(0, 2_000), (1, 1_000)] {
        session["transcript"][index]["created_at"] = (started + offset).into();
        session["transcript"][index]["updated_at"] = (started + offset).into();
    }
    assert!(target.restore_backup(&reordered.to_string(), true).is_err());
    assert_eq!(target.sessions("").unwrap(), before);
    assert_eq!(
        target.session(&keep).unwrap().notes,
        "Unrelated user notes stay exact.\r\nCafé 🦀"
    );
    assert!(target.export_session(&id, ExportFormat::Markdown).is_ok());
}

#[test]
fn restore_and_restart_retire_pending_jobs_without_losing_previous_accepted_or_user_notes() {
    let (mut source, id, plan) = prepared(&["An original source."]);
    let accepted = accept(&mut source, &id, plan.clone());
    let pending = source.begin_notes(&id, plan).unwrap();
    let backup = source.backup_json().unwrap();
    let mut target = Store::open_in_memory().unwrap();
    target.restore_backup(&backup, false).unwrap();
    let restored = target.session(&id).unwrap();
    assert_eq!(
        restored.notes_job.as_ref().unwrap().cancel_reason,
        Some(NotesCancelReason::Restored)
    );
    assert_eq!(restored.generated_notes.as_ref().unwrap().id, accepted);
    assert!(
        !target
            .complete_notes(&id, &pending.id, supplied(vec![vec![0]]))
            .unwrap()
    );
    let directory =
        std::env::temp_dir().join(format!("cueglint-notes-restart-{}", uuid::Uuid::new_v4()));
    let path = directory.join("notes.sqlite3");
    let next;
    {
        let mut disk = Store::open(&path).unwrap();
        disk.restore_backup(&backup, false).unwrap();
        let input = disk.notes_input(&id).unwrap();
        next = disk
            .begin_notes(
                &id,
                NotesPlan {
                    source: input.source,
                    chunks: vec![vec![0]],
                },
            )
            .unwrap()
            .id;
    }
    {
        let mut disk = Store::open(&path).unwrap();
        let restored = disk.session(&id).unwrap();
        assert_eq!(
            restored.notes_job.as_ref().unwrap().cancel_reason,
            Some(NotesCancelReason::Interrupted)
        );
        assert_eq!(restored.notes, source.session(&id).unwrap().notes);
        assert_eq!(restored.generated_notes.as_ref().unwrap().id, accepted);
        assert!(
            !disk
                .complete_notes(&id, &next, supplied(vec![vec![0]]))
                .unwrap()
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn v1_backup_and_database_migrate_exact_user_text_and_export_portable_v2() {
    let (source, id, _) = prepared(&["Original final transcript."]);
    let original = source.session(&id).unwrap();
    let mut legacy: Value = serde_json::from_str(&source.backup_json().unwrap()).unwrap();
    legacy["version"] = 1.into();
    let row = legacy["sessions"][0].as_object_mut().unwrap();
    for field in ["notes_revision", "notes_job", "generated_notes"] {
        row.remove(field);
    }
    let mut target = Store::open_in_memory().unwrap();
    target.restore_backup(&legacy.to_string(), false).unwrap();
    let restored = target.session(&id).unwrap();
    assert_eq!(restored.notes, original.notes);
    assert_eq!(restored.summary, original.summary);
    assert_eq!(restored.title, original.title);
    assert_eq!(restored.notes_revision, 0);
    assert!(restored.notes_job.is_none() && restored.generated_notes.is_none());
    let export: Value =
        serde_json::from_str(&target.export_session(&id, ExportFormat::Json).unwrap()).unwrap();
    assert_eq!(export["version"], 2);
    assert_eq!(export["notes"], original.notes);
    assert_eq!(
        serde_json::from_str::<Value>(&target.backup_json().unwrap()).unwrap()["version"],
        2
    );
    let directory =
        std::env::temp_dir().join(format!("cueglint-notes-v1-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("legacy.sqlite3");
    {
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TABLE sessions(id TEXT PRIMARY KEY,data TEXT NOT NULL); PRAGMA user_version=1;",
        )
        .unwrap();
        db.execute(
            "INSERT INTO sessions(id,data) VALUES(?1,?2)",
            rusqlite::params![id, legacy["sessions"][0].to_string()],
        )
        .unwrap();
    }
    {
        let migrated = Store::open(&path).unwrap();
        assert_eq!(migrated.session(&id).unwrap().notes, original.notes);
        assert_eq!(migrated.session(&id).unwrap().summary, original.summary);
    }
    {
        let db = rusqlite::Connection::open(&path).unwrap();
        assert_eq!(
            db.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                .unwrap(),
            2
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn supplied_export_is_literal_and_preserves_source_spans_and_user_fields_exactly() {
    let text = "Maybe **someone** could help, if approved.\r\n<script>literal</script> 🦀";
    let (mut store, id, plan) = prepared(&[text]);
    let job = store.begin_notes(&id, plan).unwrap();
    store
        .complete_notes(&id, &job.id, supplied(vec![vec![0]]))
        .unwrap();
    let before = store.session(&id).unwrap();
    let markdown = store.export_session(&id, ExportFormat::Markdown).unwrap();
    assert!(markdown.contains(&before.notes));
    assert!(markdown.contains(before.summary.as_ref().unwrap()));
    assert!(markdown.contains("Unreviewed extractive highlights draft"));
    assert!(markdown.contains("Supplied utterance selection; no model execution is claimed."));
    let json: Value =
        serde_json::from_str(&store.export_session(&id, ExportFormat::Json).unwrap()).unwrap();
    let highlight = &json["notes_job"]["candidate"]["highlights"][0];
    assert_eq!(highlight["text"], text);
    assert_eq!(highlight["start_byte"], 0);
    assert_eq!(highlight["end_byte"], text.len());
    assert_eq!(
        store.export_session(&id, ExportFormat::Markdown).unwrap(),
        markdown
    );
    assert_eq!(store.session(&id).unwrap(), before, "export is read-only");
    let backup = store.backup_json().unwrap();
    let mut restored = Store::open_in_memory().unwrap();
    restored.restore_backup(&backup, false).unwrap();
    assert_eq!(restored.session(&id).unwrap(), before);
}

#[test]
fn source_identity_distinguishes_unicode_normalization_and_line_endings_without_changing_ids() {
    let (source, id, plan) = prepared(&["é\r\nA tentative statement."]);
    let backup: Value = serde_json::from_str(&source.backup_json().unwrap()).unwrap();
    for text in [
        "e\u{301}\r\nA tentative statement.",
        "é\nA tentative statement.",
    ] {
        let mut changed = backup.clone();
        changed["sessions"][0]["transcript"][0]["text"] = text.into();
        let mut target = Store::open_in_memory().unwrap();
        target.restore_backup(&changed.to_string(), false).unwrap();
        let input = target.notes_input(&id).unwrap();
        assert_eq!(input.source.session_id, plan.source.session_id);
        assert_eq!(input.source.segment_ids, plan.source.segment_ids);
        assert_eq!(input.transcript[0].text, text);
        assert_ne!(input.source.sha256, plan.source.sha256);
    }
}
