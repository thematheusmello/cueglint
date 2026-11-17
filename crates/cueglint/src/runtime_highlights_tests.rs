//! Weight-free actor protocol tests inject typed preparation/completion events.
//! They do not claim model execution or highlight quality.
use super::*;
use cueglint_core::{
    NotesCancelReason, NotesCompletion, NotesJobStatus, NotesPlan, NotesProvenance,
};

fn setup(capacity: usize) -> (Store, String, Runtime, Receiver<InferenceCommand>) {
    let mut store = Store::open_in_memory().unwrap();
    let id = store
        .start_session("Original highlights actor fixture", None)
        .unwrap()
        .id;
    for (index, text) in ["Maybe Ana can review this.", "No deadline was committed."]
        .iter()
        .enumerate()
    {
        store
            .upsert_transcript(
                &id,
                TranscriptUpdate {
                    id: format!("segment-{index}"),
                    speaker: Speaker::You,
                    text: (*text).into(),
                    finalized: true,
                    source: TranscriptSource::Manual,
                },
            )
            .unwrap();
    }
    store.update_notes(&id, "Human notes stay intact.").unwrap();
    store.stop_session(&id).unwrap();
    let mut runtime = Runtime::new(&store, true, egui::Context::default()).unwrap();
    let (sender, receiver) = mpsc::sync_channel(capacity);
    runtime.generator = sender;
    runtime.fixture = false;
    runtime
        .snapshot
        .installed_profiles
        .push(runtime.snapshot.generation_profile.clone());
    (store, id, runtime, receiver)
}

fn prepare(
    runtime: &mut Runtime,
    store: &mut Store,
    id: &str,
    jobs: &Receiver<InferenceCommand>,
) -> HighlightsBinding {
    runtime.start_highlights(store, uuid(), id).unwrap();
    let InferenceCommand::PrepareHighlights(job) = jobs.try_recv().unwrap() else {
        panic!("wrong actor job")
    };
    let plan = NotesPlan {
        source: job.input.source,
        chunks: vec![vec![0], vec![1]],
    };
    assert!(
        store.session(id).unwrap().notes_job.is_none(),
        "preparation cannot create a portable pending job"
    );
    runtime
        .apply(store, Event::HighlightsPrepared(job.binding, Ok(plan)))
        .unwrap();
    let InferenceCommand::RunHighlights(run) = jobs.try_recv().unwrap() else {
        panic!("wrong actor job")
    };
    assert!(!run.cancel.load(Ordering::Relaxed));
    run.binding
}

fn supplied() -> NotesCompletion {
    NotesCompletion {
        selections: vec![vec![0], vec![1]],
        provenance: NotesProvenance::Supplied {},
    }
}

#[test]
fn highlights_are_explicit_local_and_publish_only_complete_frozen_results() {
    let (mut store, id, mut runtime, jobs) = setup(2);
    runtime.snapshot.answer_provider = ProviderConfig {
        provider: cueglint_core::ProviderId::OpenAi,
        model_id: "explicit-hosted-answer-fixture".into(),
        base_url: None,
    };
    let binding = prepare(&mut runtime, &mut store, &id, &jobs);
    assert_eq!(binding.profile, runtime.snapshot.generation_profile);
    assert!(runtime.ensure_answer_ready("Question").is_err());
    assert!(runtime.start_capture(&store, &id).is_err());
    runtime
        .apply(
            &mut store,
            Event::HighlightsProgress(
                binding.clone(),
                cueglint_local::notes::NotesProgress {
                    completed_chunks: 1,
                    total_chunks: 2,
                },
            ),
        )
        .unwrap();
    runtime
        .apply(
            &mut store,
            Event::HighlightsProgress(
                binding.clone(),
                cueglint_local::notes::NotesProgress {
                    completed_chunks: 100,
                    total_chunks: 2,
                },
            ),
        )
        .unwrap();
    assert_eq!(
        runtime
            .snapshot
            .highlights
            .as_ref()
            .unwrap()
            .completed_chunks,
        1
    );
    assert!(
        store
            .session(&id)
            .unwrap()
            .notes_job
            .unwrap()
            .candidate
            .is_none()
    );
    runtime
        .apply(
            &mut store,
            Event::HighlightsDone(binding.clone(), Ok(supplied())),
        )
        .unwrap();
    let ready = store.session(&id).unwrap();
    assert_eq!(ready.notes, "Human notes stay intact.");
    assert!(ready.generated_notes.is_none());
    assert_eq!(ready.notes_job.unwrap().status, NotesJobStatus::Ready);
    assert!(!runtime.highlights_busy());
    runtime
        .apply(
            &mut store,
            Event::HighlightsDone(binding, Err("late error".into())),
        )
        .unwrap();
    assert_eq!(
        runtime.snapshot.highlights.as_ref().unwrap().phase,
        HighlightsPhase::Ready
    );
}

#[test]
fn cancellation_waits_for_actor_terminal_and_rejects_stale_completion_after_new_job() {
    let (mut store, id, mut runtime, jobs) = setup(2);
    let old = prepare(&mut runtime, &mut store, &id, &jobs);
    runtime
        .cancel_highlights(&mut store, NotesCancelReason::User)
        .unwrap();
    assert!(runtime.highlights_busy());
    assert_eq!(
        store.session(&id).unwrap().notes_job.unwrap().status,
        NotesJobStatus::Cancelled
    );
    assert!(runtime.start_highlights(&store, uuid(), &id).is_err());
    runtime
        .apply(
            &mut store,
            Event::HighlightsDone(old.clone(), Ok(supplied())),
        )
        .unwrap();
    assert!(!runtime.highlights_busy());
    let new = prepare_after_cancel(&mut runtime, &mut store, &id, &jobs);
    runtime
        .apply(
            &mut store,
            Event::HighlightsDone(old, Err("old error".into())),
        )
        .unwrap();
    assert_eq!(runtime.snapshot.highlights.as_ref().unwrap().binding, new);
    runtime
        .apply(&mut store, Event::HighlightsDone(new, Ok(supplied())))
        .unwrap();
    assert_eq!(
        store.session(&id).unwrap().notes_job.unwrap().status,
        NotesJobStatus::Ready
    );
}

fn prepare_after_cancel(
    runtime: &mut Runtime,
    store: &mut Store,
    id: &str,
    jobs: &Receiver<InferenceCommand>,
) -> HighlightsBinding {
    runtime.start_highlights(store, uuid(), id).unwrap();
    let InferenceCommand::PrepareHighlights(job) = jobs.try_recv().unwrap() else {
        panic!("wrong job")
    };
    runtime
        .apply(
            store,
            Event::HighlightsPrepared(
                job.binding,
                Ok(NotesPlan {
                    source: job.input.source,
                    chunks: vec![vec![0], vec![1]],
                }),
            ),
        )
        .unwrap();
    let InferenceCommand::RunHighlights(run) = jobs.try_recv().unwrap() else {
        panic!("wrong job")
    };
    run.binding
}

#[test]
fn preparation_source_change_and_queue_saturation_leave_no_publishable_job() {
    let (mut store, id, mut runtime, jobs) = setup(1);
    assert!(runtime.generator.try_send(InferenceCommand::Reset).is_ok());
    assert!(runtime.start_highlights(&store, uuid(), &id).is_err());
    assert!(!runtime.highlights_busy());
    assert!(store.session(&id).unwrap().notes_job.is_none());
    jobs.try_recv().unwrap();
    runtime.start_highlights(&store, uuid(), &id).unwrap();
    let InferenceCommand::PrepareHighlights(job) = jobs.try_recv().unwrap() else {
        panic!("wrong job")
    };
    store.resume_session(&id).unwrap();
    store
        .upsert_transcript(
            &id,
            TranscriptUpdate {
                id: "segment-2".into(),
                speaker: Speaker::You,
                text: "A newly finalized utterance changes the complete source.".into(),
                finalized: true,
                source: TranscriptSource::Manual,
            },
        )
        .unwrap();
    store.stop_session(&id).unwrap();
    runtime
        .apply(
            &mut store,
            Event::HighlightsPrepared(
                job.binding,
                Ok(NotesPlan {
                    source: job.input.source,
                    chunks: vec![vec![0], vec![1]],
                }),
            ),
        )
        .unwrap();
    assert!(!runtime.highlights_busy());
    assert!(store.session(&id).unwrap().notes_job.is_none());
    runtime.start_highlights(&store, uuid(), &id).unwrap();
    let InferenceCommand::PrepareHighlights(job) = jobs.try_recv().unwrap() else {
        panic!("wrong job")
    };
    assert!(runtime.generator.try_send(InferenceCommand::Reset).is_ok());
    runtime
        .apply(
            &mut store,
            Event::HighlightsPrepared(
                job.binding,
                Ok(NotesPlan {
                    source: job.input.source,
                    chunks: vec![vec![0], vec![1, 2]],
                }),
            ),
        )
        .unwrap();
    assert!(!runtime.highlights_busy());
    assert_eq!(
        store.session(&id).unwrap().notes_job.unwrap().cancel_reason,
        Some(NotesCancelReason::Failed)
    );
}

#[test]
fn cancellation_while_planning_and_root_profile_or_source_changes_cannot_publish() {
    let (mut store, id, mut runtime, jobs) = setup(2);
    runtime.start_highlights(&store, uuid(), &id).unwrap();
    let InferenceCommand::PrepareHighlights(job) = jobs.try_recv().unwrap() else {
        panic!("wrong job")
    };
    runtime
        .cancel_highlights(&mut store, NotesCancelReason::User)
        .unwrap();
    assert!(job.cancel.load(Ordering::Relaxed));
    assert!(runtime.highlights_busy());
    runtime
        .apply(
            &mut store,
            Event::HighlightsPrepared(job.binding, Err("cancelled".into())),
        )
        .unwrap();
    assert!(!runtime.highlights_busy());
    assert!(store.session(&id).unwrap().notes_job.is_none());
    let binding = prepare(&mut runtime, &mut store, &id, &jobs);
    runtime.snapshot.generation_profile = "different-profile".into();
    runtime
        .apply(&mut store, Event::HighlightsDone(binding, Ok(supplied())))
        .unwrap();
    assert!(!runtime.highlights_busy());
    assert_eq!(
        store.session(&id).unwrap().notes_job.unwrap().cancel_reason,
        Some(NotesCancelReason::Failed)
    );
    assert_eq!(
        store.session(&id).unwrap().notes,
        "Human notes stay intact."
    );
}
