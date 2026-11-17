//! Native egui tests use a controlled typed worker peer, with no model or capture.
use super::tests::{question_app, question_snapshot};
use super::*;
use crate::runtime::{HighlightsBinding, HighlightsPhase, HighlightsStatus};
use crate::worker::{HighlightsAck, HighlightsAction};
use cueglint_core::{
    NotesCompletion, NotesPlan, NotesProvenance, Store, TranscriptSource, TranscriptUpdate,
};

fn accepted(store: &mut Store, session: &str) -> String {
    store.resume_session(session).unwrap();
    store
        .upsert_transcript(
            session,
            TranscriptUpdate {
                id: "original-quote".into(),
                speaker: Speaker::You,
                text: "Maybe Ana can review this, if approved.".into(),
                finalized: true,
                source: TranscriptSource::Manual,
            },
        )
        .unwrap();
    store.stop_session(session).unwrap();
    let job = store
        .begin_notes(
            session,
            NotesPlan {
                source: store.notes_input(session).unwrap().source,
                chunks: vec![vec![0]],
            },
        )
        .unwrap();
    store
        .complete_notes(
            session,
            &job.id,
            NotesCompletion {
                selections: vec![vec![0]],
                provenance: NotesProvenance::Supplied {},
            },
        )
        .unwrap();
    store.accept_notes(session, &job.id).unwrap();
    job.id
}

#[test]
fn append_drains_every_notes_ack_then_freezes_editor_until_exact_typed_ack() {
    let context = egui::Context::default();
    let (mut app, mut store, replies, commands) = question_app(&context);
    let session = app.selected().unwrap().id.clone();
    let generated = accepted(&mut store, &session);
    app.apply_snapshot(question_snapshot(&app, &store), None, false);
    app.notes = "Human café notes before append.".into();
    app.notes_dirty = true;
    app.request_highlights_append(&session, &generated);
    let Command::Notes(id, first) = commands.try_recv().unwrap() else {
        panic!("manual notes must drain first")
    };
    assert!(app.notes_saving);
    store.update_notes(&id, &first).unwrap();
    let saved = question_snapshot(&app, &store);
    app.notes = "A newer human edit while the first save runs.".into();
    replies
        .send(Response::Update(question_snapshot(&app, &store), None))
        .unwrap();
    app.poll(&context);
    assert!(
        app.notes_saving,
        "derived updates cannot acknowledge a manual save"
    );
    assert!(commands.try_recv().is_err());
    replies.send(Response::State(saved, None)).unwrap();
    app.poll(&context);
    let Command::Notes(id, latest) = commands.try_recv().unwrap() else {
        panic!("latest manual edit needs its own acknowledgement")
    };
    store.update_notes(&id, &latest).unwrap();
    replies
        .send(Response::State(question_snapshot(&app, &store), None))
        .unwrap();
    app.poll(&context);
    let Command::Highlights(command) = commands.try_recv().unwrap() else {
        panic!("append must follow the final notes ack")
    };
    let HighlightsAction::Append {
        generated: actual,
        expected_notes_revision,
    } = &command.action
    else {
        panic!("wrong highlights action")
    };
    assert_eq!(actual, &generated);
    assert_eq!(
        *expected_notes_revision,
        store.session(&session).unwrap().notes_revision
    );
    assert!(app.highlights_appending());
    let before = app.notes.clone();
    context.memory_mut(|memory| memory.request_focus(Id::new("session-notes")));
    let view = app.selected().unwrap().clone();
    let _ = context.run_ui(
        egui::RawInput {
            events: vec![egui::Event::Text("must not type".into())],
            ..Default::default()
        },
        |ui| app.notes_ui(ui, &view),
    );
    assert_eq!(app.notes, before);
    let mut stale = command.clone();
    stale.id = "old-operation".into();
    replies
        .send(Response::Highlights(HighlightsAck {
            command: stale,
            result: Err("stale error".into()),
        }))
        .unwrap();
    app.poll(&context);
    assert!(
        app.highlights_appending(),
        "an unrelated ACK cannot unlock the editor"
    );
    let outcome = store
        .append_generated_notes(&session, &generated, *expected_notes_revision)
        .unwrap();
    replies
        .send(Response::Highlights(HighlightsAck {
            command,
            result: Ok((question_snapshot(&app, &store), Some(outcome))),
        }))
        .unwrap();
    app.poll(&context);
    assert!(!app.highlights_appending());
    assert_eq!(app.notes, store.session(&session).unwrap().notes);
    assert!(app.notes.starts_with(&latest));
    assert_eq!(app.pending, 0);
}

#[test]
fn conflicts_require_review_and_failed_manual_save_never_queues_append() {
    let context = egui::Context::default();
    let (mut app, mut store, replies, commands) = question_app(&context);
    let session = app.selected().unwrap().id.clone();
    let generated = accepted(&mut store, &session);
    app.apply_snapshot(question_snapshot(&app, &store), None, false);
    app.request_highlights_append(&session, &generated);
    let Command::Highlights(command) = commands.try_recv().unwrap() else {
        panic!("expected append")
    };
    let HighlightsAction::Append {
        expected_notes_revision,
        ..
    } = command.action
    else {
        panic!("wrong action")
    };
    store
        .update_notes(&session, "A concurrent human edit must win.")
        .unwrap();
    let conflict = store
        .append_generated_notes(&session, &generated, expected_notes_revision)
        .unwrap();
    replies
        .send(Response::Highlights(HighlightsAck {
            command,
            result: Ok((question_snapshot(&app, &store), Some(conflict))),
        }))
        .unwrap();
    app.poll(&context);
    assert!(app.highlights.conflict);
    assert_eq!(app.notes, "A concurrent human edit must win.");
    app.request_highlights_append(&session, &generated);
    assert!(commands.try_recv().is_err());
    app.send_highlights(&session, HighlightsAction::Review);
    let Command::Highlights(review) = commands.try_recv().unwrap() else {
        panic!("expected explicit review")
    };
    replies
        .send(Response::Highlights(HighlightsAck {
            command: review,
            result: Ok((question_snapshot(&app, &store), None)),
        }))
        .unwrap();
    app.poll(&context);
    assert!(!app.highlights.conflict);
    app.notes = "Unsaved exact edit.".into();
    app.notes_dirty = true;
    app.request_highlights_append(&session, &generated);
    assert!(matches!(commands.try_recv().unwrap(), Command::Notes(..)));
    replies
        .send(Response::Error("Simulated disk failure".into()))
        .unwrap();
    app.poll(&context);
    assert!(app.notes_dirty && app.notes_failed);
    assert!(app.highlights.append_after_notes.is_none());
    assert!(commands.try_recv().is_err());
    assert_eq!(app.notes, "Unsaved exact edit.");
}

#[test]
fn session_navigation_and_quit_wait_for_highlights_terminal_not_cancel_command_ack() {
    let context = egui::Context::default();
    let (mut app, store, replies, commands) = question_app(&context);
    let session = app.selected().unwrap().id.clone();
    app.runtime.highlights = Some(HighlightsStatus {
        binding: HighlightsBinding {
            operation_id: "held-operation".into(),
            session_id: session.clone(),
            profile: models::GENERATION_PROFILE.into(),
            root: app.runtime.model_root.clone(),
            source_sha256: "a".repeat(64),
            job_id: Some("held-job".into()),
        },
        phase: HighlightsPhase::Generating,
        completed_chunks: 0,
        total_chunks: 1,
    });
    app.after_saving_notes(Command::Open(session.clone()));
    assert!(matches!(commands.try_recv().unwrap(), Command::Cancel));
    app.runtime.highlights.as_mut().unwrap().phase = HighlightsPhase::Cancelling;
    replies
        .send(Response::State(question_snapshot(&app, &store), None))
        .unwrap();
    app.poll(&context);
    assert!(
        commands.try_recv().is_err(),
        "cancel command ACK is not an actor completion"
    );
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
    let held = context.run_ui(egui::RawInput::default(), |ui| app.poll(ui.ctx()));
    assert!(
        !held.viewport_output[&ViewportId::ROOT]
            .commands
            .contains(&ViewportCommand::Close)
    );
    app.runtime.highlights.as_mut().unwrap().phase = HighlightsPhase::Cancelled;
    replies
        .send(Response::Runtime(app.runtime.clone()))
        .unwrap();
    app.poll(&context);
    assert!(
        commands.try_recv().is_err(),
        "quit retires a deferred session change"
    );
    let done = context.run_ui(egui::RawInput::default(), |ui| app.poll(ui.ctx()));
    assert!(
        done.viewport_output[&ViewportId::ROOT]
            .commands
            .contains(&ViewportCommand::Close)
    );
}

#[test]
fn navigation_requested_before_generate_ack_cancels_the_newly_acknowledged_job() {
    let context = egui::Context::default();
    let (mut app, store, replies, commands) = question_app(&context);
    let session = app.selected().unwrap().id.clone();
    app.send_highlights(&session, HighlightsAction::Generate);
    let Command::Highlights(command) = commands.try_recv().unwrap() else {
        panic!("expected generate")
    };
    app.after_saving_notes(Command::Resume(session.clone()));
    assert!(commands.try_recv().is_err());
    app.runtime.highlights = Some(HighlightsStatus {
        binding: HighlightsBinding {
            operation_id: command.id.clone(),
            session_id: session,
            profile: models::GENERATION_PROFILE.into(),
            root: app.runtime.model_root.clone(),
            source_sha256: "a".repeat(64),
            job_id: None,
        },
        phase: HighlightsPhase::Preparing,
        completed_chunks: 0,
        total_chunks: 0,
    });
    replies
        .send(Response::Highlights(HighlightsAck {
            command,
            result: Ok((question_snapshot(&app, &store), None)),
        }))
        .unwrap();
    app.poll(&context);
    assert!(matches!(commands.try_recv().unwrap(), Command::Cancel));
    assert!(commands.try_recv().is_err());
    app.runtime.highlights.as_mut().unwrap().phase = HighlightsPhase::Cancelling;
    replies
        .send(Response::State(question_snapshot(&app, &store), None))
        .unwrap();
    app.poll(&context);
    assert!(
        commands.try_recv().is_err(),
        "resumption waits for the actor terminal event"
    );
    app.runtime.highlights.as_mut().unwrap().phase = HighlightsPhase::Cancelled;
    replies
        .send(Response::Runtime(app.runtime.clone()))
        .unwrap();
    app.poll(&context);
    assert!(matches!(commands.try_recv().unwrap(), Command::Resume(_)));
}

#[test]
fn typed_highlights_ack_cannot_clear_notes_saving_or_cross_session_and_review_is_native() {
    let context = egui::Context::default();
    let (mut app, mut store, replies, commands) = question_app(&context);
    let session = app.selected().unwrap().id.clone();
    accepted(&mut store, &session);
    app.apply_snapshot(question_snapshot(&app, &store), None, false);
    app.notes = "Exact dirty manual notes.".into();
    app.notes_dirty = true;
    app.save_notes();
    assert!(matches!(commands.try_recv().unwrap(), Command::Notes(..)));
    app.send_highlights(&session, HighlightsAction::Review);
    let Command::Highlights(review) = commands.try_recv().unwrap() else {
        panic!("wrong command")
    };
    let mut wrong = review.clone();
    wrong.session = "different-session".into();
    replies
        .send(Response::Highlights(HighlightsAck {
            command: wrong,
            result: Ok((question_snapshot(&app, &store), None)),
        }))
        .unwrap();
    app.poll(&context);
    assert_eq!(app.highlights.pending, Some(review.clone()));
    replies
        .send(Response::Highlights(HighlightsAck {
            command: review,
            result: Ok((question_snapshot(&app, &store), None)),
        }))
        .unwrap();
    app.poll(&context);
    assert!(app.notes_saving);
    assert_eq!(app.notes, "Exact dirty manual notes.");
    assert_eq!(
        app.acknowledgement_kinds.front(),
        Some(&Acknowledgement::Notes)
    );
    app.highlights.open = true;
    let view = app.selected().unwrap().clone();
    // A new egui Window completes an invisible sizing pass before painting.
    let _ = context.run_ui(egui::RawInput::default(), |ui| {
        app.highlights_ui(ui.ctx(), &view)
    });
    let output = context.run_ui(egui::RawInput::default(), |ui| {
        app.highlights_ui(ui.ctx(), &view)
    });
    let texts: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.text()),
            _ => None,
        })
        .collect();
    assert!(
        texts
            .iter()
            .any(|text| text.contains("Accepted highlights"))
    );
    assert!(texts.iter().any(|text| text.contains("Maybe Ana")));
}
