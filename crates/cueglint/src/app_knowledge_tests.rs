use super::*;

#[test]
fn import_queue_identity_cancellation_and_late_errors_preserve_selection_and_notes() {
    let context = egui::Context::default();
    let (mut app, store, replies, commands) = tests::question_app(&context);
    let selected = app.selected().unwrap().id.clone();
    app.notes = "Unsaved original human notes".into();
    app.notes_dirty = true;
    app.notes_edited = Instant::now();
    app.import_path = "/original/first.docx".into();
    app.import_knowledge();
    let Command::Import(first) = commands.try_recv().unwrap() else {
        panic!("missing typed import")
    };
    assert_eq!(first.path, PathBuf::from("/original/first.docx"));
    assert_eq!(app.knowledge_import.as_ref().unwrap().id, first.id);
    replies
        .send(Response::State(
            tests::question_snapshot(&app, &store),
            None,
        ))
        .unwrap();
    app.poll(&context);
    assert_eq!(app.pending, 0);
    app.import_knowledge();
    assert!(
        commands.try_recv().is_err(),
        "a second import cannot occupy another slot"
    );
    first.cancel();
    assert!(app.knowledge_import.as_ref().unwrap().cancelled());
    replies
        .send(Response::Import(
            "obsolete-original-request".into(),
            Err("stale failure".into()),
        ))
        .unwrap();
    app.poll(&context);
    assert_eq!(app.knowledge_import.as_ref().unwrap().id, first.id);
    replies
        .send(Response::Import(
            first.id,
            Err("Document import cancelled".into()),
        ))
        .unwrap();
    app.poll(&context);
    assert!(app.knowledge_import.is_none());
    assert_eq!(app.selected().unwrap().id, selected);
    assert_eq!(app.notes, "Unsaved original human notes");
    assert!(app.notes_dirty);
}
