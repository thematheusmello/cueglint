use super::*;

#[test]
fn import_accessibility_is_disabled_for_blank_or_busy_sources_and_queues_only_ready_input() {
    for (path, busy, disabled) in [
        ("", false, true),
        (" \t ", false, true),
        ("/original/workshop.docx", true, true),
        ("/original/workshop.docx", false, false),
    ] {
        let context = egui::Context::default();
        context.enable_accesskit();
        let (mut app, _store, _replies, commands) = tests::question_app(&context);
        app.import_path = path.into();
        if busy {
            app.knowledge_import = Some(ImportCommand::new(PathBuf::from(path)));
        }
        let input = || egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(860.0, 1000.0))),
            ..Default::default()
        };
        let output = context.run_ui(input(), |ui| app.knowledge(ui));
        let tree = output.platform_output.accesskit_update.unwrap();
        let (id, node) = tree
            .nodes
            .iter()
            .find(|(_, node)| {
                node.role() == egui::accesskit::Role::Button && node.label() == Some("Import")
            })
            .unwrap();
        assert_eq!(node.is_disabled(), disabled, "path={path:?}, busy={busy}");
        let mut click = input();
        click.events.push(egui::Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target_tree: egui::accesskit::TreeId::ROOT,
                target_node: *id,
                data: None,
            },
        ));
        let _ = context.run_ui(click, |ui| app.knowledge(ui));
        if disabled {
            assert!(
                commands.try_recv().is_err(),
                "disabled Import cannot queue work"
            );
        } else {
            let Command::Import(command) = commands.try_recv().unwrap() else {
                panic!("ready Import did not queue a typed command")
            };
            assert_eq!(command.path, PathBuf::from(path));
        }
    }
}

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
