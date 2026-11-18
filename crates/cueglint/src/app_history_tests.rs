use super::*;

fn pointer(position: Pos2, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(position),
        egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

fn frame(
    app: &mut Cueglint,
    context: &egui::Context,
    panel: OverlayPanel,
    events: Vec<egui::Event>,
) -> egui::accesskit::TreeUpdate {
    context
        .run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(690.0, 760.0))),
                events,
                ..Default::default()
            },
            |ui| match panel {
                OverlayPanel::History => app.history(ui),
                OverlayPanel::Detail => app.detail(ui),
                OverlayPanel::Chat => app.conversation(ui),
                _ => unreachable!(),
            },
        )
        .platform_output
        .accesskit_update
        .unwrap()
}

fn target(
    tree: &egui::accesskit::TreeUpdate,
    label: &str,
    index: usize,
) -> (egui::accesskit::NodeId, Rect) {
    let mut buttons: Vec<_> = tree
        .nodes
        .iter()
        .filter(|(_, node)| {
            node.role() == egui::accesskit::Role::Button && node.label() == Some(label)
        })
        .map(|(id, node)| {
            let bounds = node.bounds().unwrap();
            (
                *id,
                Rect::from_min_max(
                    pos2(bounds.x0 as f32, bounds.y0 as f32),
                    pos2(bounds.x1 as f32, bounds.y1 as f32),
                ),
            )
        })
        .collect();
    buttons.sort_by(|a, b| a.1.top().total_cmp(&b.1.top()));
    *buttons
        .get(index)
        .unwrap_or_else(|| panic!("missing {label} button {index}"))
}

#[test]
fn history_row_title_icon_and_metadata_click_once_with_one_accessible_action() {
    let mut store = cueglint_core::Store::open_in_memory().unwrap();
    let session = store.start_chat(None).unwrap();
    for position in [
        pos2(128.0, 244.0),
        pos2(40.0, 244.0),
        pos2(578.0, 244.0),
        pos2(664.0, 244.0),
    ] {
        let context = egui::Context::default();
        context.enable_accesskit();
        let clicks = std::cell::Cell::new(0);
        let draw = |events| {
            context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(690.0, 760.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    let mut row = ui.new_child(
                        UiBuilder::new()
                            .max_rect(Rect::from_min_size(pos2(12.0, 216.0), vec2(666.0, 56.0))),
                    );
                    if history_row(&mut row, &session).clicked() {
                        clicks.set(clicks.get() + 1);
                    }
                },
            )
        };
        let tree = draw(vec![]).platform_output.accesskit_update.unwrap();
        let (id, bounds) = target(&tree, "New chat", 0);
        assert_eq!(
            bounds,
            Rect::from_min_size(pos2(12.0, 216.0), vec2(666.0, 56.0))
        );
        assert_eq!(
            tree.nodes
                .iter()
                .filter(|(_, node)| node.role() == egui::accesskit::Role::Button
                    && node.label() == Some("New chat"))
                .count(),
            1
        );
        let _ = draw(pointer(position, true));
        assert_eq!(clicks.get(), 0);
        let _ = draw(pointer(position, false));
        assert_eq!(
            clicks.get(),
            1,
            "the full row must activate at {position:?}"
        );
        let _ = draw(vec![]);
        assert_eq!(clicks.get(), 1);
        let _ = draw(vec![egui::Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target_tree: egui::accesskit::TreeId::ROOT,
                target_node: id,
                data: None,
            },
        )]);
        let _ = draw(vec![]);
        assert_eq!(
            clicks.get(),
            2,
            "one pointer activation and one accessible activation at {position:?}"
        );
    }
}

#[test]
fn history_title_open_preserves_notes_until_save_and_selects_the_clicked_duplicate_title() {
    let context = egui::Context::default();
    context.enable_accesskit();
    let (mut app, mut store, replies, commands) = tests::question_app(&context);
    let previous = app.selected().unwrap().id.clone();
    store.start_chat(None).unwrap();
    store.start_chat(None).unwrap();
    let mut snapshot = tests::question_snapshot(&app, &store);
    let selected = snapshot
        .sessions
        .iter()
        .filter(|s| s.title == "New chat")
        .nth(1)
        .unwrap()
        .id
        .clone();
    snapshot.selected = Some(store.session(&previous).unwrap());
    app.apply_snapshot(snapshot, None, false);
    app.history_search = "New chat".into();
    app.overlay_panel = OverlayPanel::History;
    app.notes = "Keep the unsaved original note.".into();
    app.notes_dirty = true;

    for failed in [true, false] {
        let tree = frame(&mut app, &context, OverlayPanel::History, vec![]);
        let (_, bounds) = target(&tree, "New chat", 1);
        let position = pos2(bounds.left() + 116.0, bounds.center().y);
        let _ = frame(
            &mut app,
            &context,
            OverlayPanel::History,
            pointer(position, true),
        );
        assert!(commands.try_recv().is_err());
        let _ = frame(
            &mut app,
            &context,
            OverlayPanel::History,
            pointer(position, false),
        );
        let Command::Notes(id, notes) = commands.try_recv().unwrap() else {
            panic!("opening a row must first save the previous session's notes");
        };
        assert_eq!(id, previous);
        assert_eq!(notes, "Keep the unsaved original note.");
        assert_eq!(app.selected().unwrap().id, previous);
        assert!(commands.try_recv().is_err());
        if failed {
            replies
                .send(Response::Error("Original save failure".into()))
                .unwrap();
            app.poll(&context);
            assert!(commands.try_recv().is_err());
            assert!(app.notes_dirty);
            assert_eq!(app.notes, notes);
            assert_eq!(app.selected().unwrap().id, previous);
        } else {
            let mut saved = tests::question_snapshot(&app, &store);
            saved.selected = Some(store.update_notes(&id, &notes).unwrap());
            saved.sessions = store.sessions("").unwrap();
            replies.send(Response::State(saved, None)).unwrap();
            app.poll(&context);
            assert!(matches!(commands.try_recv().unwrap(), Command::Open(id) if id == selected));
            assert!(commands.try_recv().is_err());
        }
    }
    let mut opened = tests::question_snapshot(&app, &store);
    opened.selected = Some(store.session(&selected).unwrap());
    replies.send(Response::State(opened, None)).unwrap();
    app.poll(&context);
    assert_eq!(app.selected().unwrap().id, selected);
    assert_eq!(
        store.session(&previous).unwrap().notes,
        "Keep the unsaved original note."
    );
    assert!(app.active_session().is_none());
}

#[test]
fn detail_conversation_keeps_selected_answers_notes_and_question_identity_without_recording() {
    let context = egui::Context::default();
    context.enable_accesskit();
    let (mut app, mut store, _replies, commands) = tests::question_app(&context);
    let selected = app.selected().unwrap().id.clone();
    let request = store
        .begin_request(&selected, "Original history question")
        .unwrap();
    assert!(
        store
            .complete_request(&selected, &request.id, "Original history answer")
            .unwrap()
    );
    let meeting = store
        .start_session("Separate active meeting", None)
        .unwrap();
    let mut snapshot = tests::question_snapshot(&app, &store);
    snapshot.selected = Some(store.session(&selected).unwrap());
    app.apply_snapshot(snapshot, None, false);
    app.synchronize_markdown();
    app.overlay_panel = OverlayPanel::Detail;
    app.notes = "Original unsaved notes remain editable.".into();
    app.notes_dirty = true;
    let sessions = store.sessions("").unwrap();
    let tree = frame(&mut app, &context, OverlayPanel::Detail, vec![]);
    let (_, bounds) = target(&tree, "Open conversation", 0);
    for pressed in [true, false] {
        let _ = frame(
            &mut app,
            &context,
            OverlayPanel::Detail,
            pointer(bounds.center(), pressed),
        );
    }
    assert!(app.overlay_panel == OverlayPanel::Chat);
    assert_eq!(app.selected().unwrap().id, selected);
    assert_eq!(app.active_session().unwrap().id, meeting.id);
    assert_eq!(app.notes, "Original unsaved notes remain editable.");
    assert!(app.notes_dirty && app.runtime.capture.is_none());
    assert!(
        commands.try_recv().is_err(),
        "opening the panel must not start or resume recording"
    );
    let tree = frame(&mut app, &context, OverlayPanel::Chat, vec![]);
    assert!(tree.nodes.iter().any(|(_, node)| {
        node.value()
            .is_some_and(|label| label.contains("Original history answer"))
    }));
    let (_, input) = tree
        .nodes
        .iter()
        .find(|(id, _)| *id == Id::new("question-input").accesskit_id())
        .unwrap();
    let bounds = input.bounds().unwrap();
    let position = pos2(
        ((bounds.x0 + bounds.x1) / 2.0) as f32,
        ((bounds.y0 + bounds.y1) / 2.0) as f32,
    );
    for pressed in [true, false] {
        let _ = frame(
            &mut app,
            &context,
            OverlayPanel::Chat,
            pointer(position, pressed),
        );
    }
    let _ = frame(
        &mut app,
        &context,
        OverlayPanel::Chat,
        vec![egui::Event::Text(
            "A fresh question for the selected conversation".into(),
        )],
    );
    assert_eq!(
        app.question,
        "A fresh question for the selected conversation"
    );
    let _ = frame(
        &mut app,
        &context,
        OverlayPanel::Chat,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: Some(egui::Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(
        matches!(commands.try_recv().unwrap(), Command::Ask { session: Some(id), prompt, .. } if id == selected && prompt == "A fresh question for the selected conversation")
    );
    assert!(commands.try_recv().is_err());
    assert_eq!(store.sessions("").unwrap(), sessions);
}
