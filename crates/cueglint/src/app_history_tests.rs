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

fn activate(id: egui::accesskit::NodeId) -> Vec<egui::Event> {
    vec![egui::Event::AccessKitActionRequest(
        egui::accesskit::ActionRequest {
            action: egui::accesskit::Action::Click,
            target_tree: egui::accesskit::TreeId::ROOT,
            target_node: id,
            data: None,
        },
    )]
}

#[test]
fn history_ask_is_leading_and_only_an_explicit_action_sends_the_exact_query() {
    let context = egui::Context::default();
    context.enable_accesskit();
    let (mut app, mut store, replies, commands) = tests::question_app(&context);
    let previous = app.selected().unwrap().clone();
    app.overlay_panel = OverlayPanel::History;
    app.question = "Unsent follow-up from the assistant.".into();
    app.selected_mode = Some("interview".into());

    let tree = frame(&mut app, &context, OverlayPanel::History, vec![]);
    assert!(
        !tree
            .nodes
            .iter()
            .any(|(_, node)| node.label() == Some("Ask the assistant"))
    );
    context.memory_mut(|memory| memory.request_focus(Id::new("history-search")));
    let _ = frame(&mut app, &context, OverlayPanel::History, vec![]);
    let matching = frame(
        &mut app,
        &context,
        OverlayPanel::History,
        vec![egui::Event::Text("  QuEsTiOn lifecycle  ".into())],
    );
    let (_, action_bounds) = target(&matching, "Ask the assistant", 0);
    let (_, result_bounds) = target(&matching, &previous.title, 0);
    assert!(action_bounds.bottom() <= result_bounds.top());
    assert_eq!(app.history_search, "  QuEsTiOn lifecycle  ");
    assert!(commands.try_recv().is_err());

    let query = "  Which  Río trail opens on Friday?  ";
    app.history_search = query.into();
    let unmatched = frame(
        &mut app,
        &context,
        OverlayPanel::History,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: Some(egui::Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(
        unmatched
            .nodes
            .iter()
            .any(|(_, node)| node.value() == Some("No matching sessions"))
    );
    assert!(commands.try_recv().is_err());
    let (action, _) = target(&unmatched, "Ask the assistant", 0);
    let _ = frame(&mut app, &context, OverlayPanel::History, activate(action));
    let Command::AskFromHistory {
        prompt,
        new_chat_mode,
    } = commands.try_recv().unwrap()
    else {
        panic!("History must explicitly request a fresh conversation");
    };
    assert_eq!(prompt, query.trim());
    assert_eq!(new_chat_mode.as_deref(), Some("interview"));
    assert!(commands.try_recv().is_err());
    assert!(app.overlay_panel == OverlayPanel::History);
    assert_eq!(app.selected().unwrap().id, previous.id);
    assert!(app.question_submission.is_some());

    let chat = store.start_chat(new_chat_mode.as_deref()).unwrap();
    let request = store.begin_request(&chat.id, &prompt).unwrap();
    app.runtime.generation = Some(crate::runtime::GenerationStatus {
        session_id: chat.id.clone(),
        request_id: request.id,
        state: "generating".into(),
        text: String::new(),
    });
    let mut opened = tests::question_snapshot(&app, &store);
    opened.selected = Some(store.session(&chat.id).unwrap());
    replies.send(Response::State(opened, None)).unwrap();
    app.poll(&context);

    assert!(app.overlay_panel == OverlayPanel::Chat);
    assert_eq!(app.selected().unwrap().id, chat.id);
    assert!(app.question_submission.is_none());
    assert_eq!(app.history_search, query);
    assert_eq!(app.question, "Unsent follow-up from the assistant.");
    assert_eq!(store.session(&previous.id).unwrap(), previous);
}

#[test]
fn saved_meeting_action_opts_in_without_deferring_to_a_screen_picker() {
    let context = egui::Context::default();
    context.enable_accesskit();
    let (mut app, _store, _replies, commands) = tests::question_app(&context);
    app.overlay_panel = OverlayPanel::History;
    app.history_search = "  Original cobalt decision  ".into();
    app.runtime.screen_enabled = true;
    app.runtime.screen_input = cueglint_core::ScreenInput::Window;
    app.selected_mode = Some("meeting".into());
    let tree = frame(&mut app, &context, OverlayPanel::History, vec![]);
    let (action, _) = target(&tree, "Ask saved meetings locally", 0);
    assert!(commands.try_recv().is_err());
    let _ = frame(&mut app, &context, OverlayPanel::History, activate(action));
    assert!(matches!(
        commands.try_recv().unwrap(),
        Command::AskSavedMeetings { prompt, new_chat_mode }
            if prompt == "Original cobalt decision" && new_chat_mode.as_deref() == Some("meeting")
    ));
    assert!(!app.question_submission.as_ref().unwrap().window);
}

#[test]
fn saved_meeting_history_action_reopens_locally_and_withdraws_deleted_sources() -> anyhow::Result<()>
{
    use crate::{
        runtime::{Event, Runtime},
        worker::execute,
    };
    use cueglint_core::{MeetingExcerptSource, RequestStatus, Store};
    use cueglint_local::generation::{FinishReason, GenerationOutput, ModelIdentity};

    const PROMPT: &str = "When are the Cobalt route test and review?";
    const ANSWER: &str = "The Cobalt route test is Friday and the review is Monday.";
    const WITHDRAWN: &str =
        "A source changed or was deleted. Its excerpts and dependent answers were removed.";
    let context = egui::Context::default();
    context.enable_accesskit();
    let (mut app, mut store, replies, commands) = tests::question_app(&context);
    let mut sources = Vec::new();
    for (title, note) in [
        (
            "Cobalt route planning",
            "The Cobalt route test starts Friday at Pier 6.",
        ),
        (
            "Cobalt route sign-off",
            "The Cobalt route review is Monday at Warehouse 3.",
        ),
    ] {
        let meeting = store.start_session(title, Some("meeting"))?;
        store.update_notes(&meeting.id, note)?;
        sources.push(store.finish_session(&meeting.id)?);
    }
    let mut selected = Some(sources[0].id.clone());
    let mut hosted = store.settings()?;
    hosted.answer_provider = ProviderConfig {
        provider: ProviderId::OpenAi,
        model_id: "original-dummy-model".into(),
        base_url: None,
    };
    store.update_settings(hosted.clone())?;
    let (mut runtime, queued_answer) =
        Runtime::with_scripted_answer_queue(&store, context.clone())?;
    assert!(!runtime.snapshot.screen_enabled);

    let snapshot = |app: &Cueglint, store: &Store, selected: &Option<String>, runtime: &Runtime| {
        let mut state = tests::question_snapshot(app, store);
        state.selected = selected.as_deref().map(|id| store.session(id).unwrap());
        state.runtime = runtime.snapshot.clone();
        state
    };
    let dispatch = |app: &mut Cueglint,
                    store: &mut Store,
                    selected: &mut Option<String>,
                    runtime: &mut Runtime|
     -> anyhow::Result<()> {
        let notice = execute(store, selected, runtime, commands.try_recv()?)?;
        replies
            .send(Response::State(
                snapshot(app, store, selected, runtime),
                notice,
            ))
            .map_err(|_| anyhow::anyhow!("headless UI dropped its worker response"))?;
        app.poll(&context);
        Ok(())
    };
    let visible = |tree: &egui::accesskit::TreeUpdate, text| {
        tree.nodes
            .iter()
            .any(|(_, node)| node.label() == Some(text) || node.value() == Some(text))
    };
    let enabled = |tree: &egui::accesskit::TreeUpdate, id| {
        tree.nodes
            .iter()
            .any(|(node_id, node)| *node_id == id && !node.is_disabled())
    };
    app.apply_snapshot(snapshot(&app, &store, &selected, &runtime), None, false);
    app.overlay_panel = OverlayPanel::History;
    app.history_search = format!("  {PROMPT}  ");
    let tree = frame(&mut app, &context, OverlayPanel::History, vec![]);
    let (saved, _) = target(&tree, "Ask saved meetings locally", 0);
    assert!(!enabled(&tree, saved));
    assert!(enabled(&tree, target(&tree, "Ask the assistant", 0).0));
    frame(&mut app, &context, OverlayPanel::History, activate(saved));
    assert!(commands.try_recv().is_err() && queued_answer().is_none());

    let mut local = hosted.clone();
    local.answer_provider = ProviderConfig::default();
    assert!(app.send(Command::Settings(local)));
    dispatch(&mut app, &mut store, &mut selected, &mut runtime)?;
    let tree = frame(&mut app, &context, OverlayPanel::History, vec![]);
    let (saved, _) = target(&tree, "Ask saved meetings locally", 0);
    assert!(enabled(&tree, saved));
    frame(&mut app, &context, OverlayPanel::History, activate(saved));
    dispatch(&mut app, &mut store, &mut selected, &mut runtime)?;

    let request = queued_answer().expect("the History action must enqueue a real worker request");
    let chat = &request.session_id;
    assert_eq!(request.prompt, PROMPT);
    assert_eq!(request.provider, ProviderConfig::default());
    assert_eq!(request.context.mode, Some(store.mode("meeting")?));
    assert!(request.context.screen.is_none() && runtime.snapshot.window_picker.is_none());
    let Some(MeetingContext::Excerpts(excerpts)) = &request.context.past_meetings else {
        panic!("the worker must freeze meeting excerpts");
    };
    assert_eq!(excerpts.len(), sources.len());
    for source in &sources {
        let excerpt = excerpts
            .iter()
            .find(|excerpt| excerpt.meeting_id == source.id)
            .unwrap();
        assert_eq!(excerpt.title, source.title);
        assert_eq!(excerpt.source, MeetingExcerptSource::Notes);
        assert_eq!(excerpt.text, source.notes);
    }
    let selected_chat = app.selected().unwrap();
    assert!(selected_chat.history_chat && selected_chat.saved_at.is_some());
    assert_eq!(&selected_chat.id, chat);
    assert_eq!(selected_chat.requests, [request.clone()]);
    assert!(app.overlay_panel == OverlayPanel::Chat && app.question_submission.is_none());

    let answer = GenerationOutput {
        text: ANSWER.into(),
        prompt_tokens: 64,
        generated_tokens: 14,
        max_new_tokens: 64,
        elapsed_ms: 0,
        finish_reason: FinishReason::EndOfTurn,
        model: ModelIdentity {
            profile_id: "original-fixture".into(),
            revision: "0".repeat(40),
            sha256: "0".repeat(64),
            tokenizer_sha256: "0".repeat(64),
            backend: "fixture".into(),
        },
        omitted_transcript_segments: 0,
        omitted_chat_turns: 0,
        screen_truncated: false,
        sources: Vec::new(),
        prompt_sha256: "0".repeat(64),
        template_revision: "original-fixture".into(),
        decoder_policy_revision: None,
    };
    let (updated, notice) = runtime.apply(
        &mut store,
        Event::GenerationDone(request.id.clone(), Ok(answer)),
    )?;
    assert!(updated);
    assert!(
        replies
            .send(Response::Update(
                snapshot(&app, &store, &selected, &runtime),
                notice
            ))
            .is_ok()
    );
    app.poll(&context);
    let tree = frame(&mut app, &context, OverlayPanel::Chat, vec![]);
    assert!(visible(&tree, "Saved meeting sources"));
    assert!(visible(&tree, "Copy response"));
    assert_eq!(app.selected().unwrap().suggestions[0].text, ANSWER);

    assert!(app.send(Command::Settings(hosted)));
    dispatch(&mut app, &mut store, &mut selected, &mut runtime)?;
    assert_eq!(app.selected().unwrap().suggestions[0].text, ANSWER);
    app.question = "Keep this original follow-up draft.".into();
    let tree = frame(&mut app, &context, OverlayPanel::Chat, vec![]);
    let (send, _) = target(&tree, "Send question", 0);
    assert!(!enabled(&tree, send));
    frame(&mut app, &context, OverlayPanel::Chat, activate(send));
    assert!(commands.try_recv().is_err() && queued_answer().is_none());

    assert!(app.send(Command::Delete(sources[0].id.clone())));
    dispatch(&mut app, &mut store, &mut selected, &mut runtime)?;
    assert!(store.session(&sources[0].id).is_err());
    let withdrawn = app.selected().unwrap();
    assert_eq!(&withdrawn.id, chat);
    assert!(withdrawn.suggestions.is_empty());
    assert_eq!(withdrawn.requests[0].prompt, PROMPT);
    assert_eq!(withdrawn.requests[0].status, RequestStatus::Cancelled);
    assert_eq!(
        withdrawn.requests[0].context.past_meetings,
        Some(MeetingContext::Withdrawn)
    );
    let tree = frame(&mut app, &context, OverlayPanel::Chat, vec![]);
    assert!(visible(&tree, WITHDRAWN));
    assert!(!visible(&tree, "Saved meeting sources") && !visible(&tree, ANSWER));
    assert!(!visible(&tree, "Copy response"));

    app.new_chat();
    dispatch(&mut app, &mut store, &mut selected, &mut runtime)?;
    assert!(app.selected().is_none() && app.question.is_empty());
    assert!(app.overlay_panel == OverlayPanel::Compact);
    app.overlay_panel = OverlayPanel::History;
    app.history_search = "History chat".into();
    let tree = frame(&mut app, &context, OverlayPanel::History, vec![]);
    let (saved_chat, _) = target(&tree, "History chat", 0);
    frame(
        &mut app,
        &context,
        OverlayPanel::History,
        activate(saved_chat),
    );
    dispatch(&mut app, &mut store, &mut selected, &mut runtime)?;
    assert_eq!(&app.selected().unwrap().id, chat);
    assert!(app.overlay_panel == OverlayPanel::Chat && app.selected_chat_requires_local());
    assert!(visible(
        &frame(&mut app, &context, OverlayPanel::Chat, vec![]),
        WITHDRAWN
    ));
    assert!(queued_answer().is_none());
    Ok(())
}

#[test]
fn history_ask_waits_for_saved_notes_and_keeps_queries_after_refusal() {
    let context = egui::Context::default();
    context.enable_accesskit();
    let (mut app, mut store, replies, commands) = tests::question_app(&context);
    let previous = app.selected().unwrap().id.clone();
    let query = "  Where is the original field notebook?  ";
    let notes = "Keep the original handwritten sketch.";
    app.overlay_panel = OverlayPanel::History;
    app.history_search = query.into();
    app.question = "Keep this other unfinished draft.".into();
    app.notes = notes.into();
    app.notes_dirty = true;

    for saved in [false, true] {
        let tree = frame(&mut app, &context, OverlayPanel::History, vec![]);
        let (action, _) = target(&tree, "Ask the assistant", 0);
        let _ = frame(&mut app, &context, OverlayPanel::History, activate(action));
        assert!(
            matches!(commands.try_recv().unwrap(), Command::Notes(id, text) if id == previous && text == notes)
        );
        assert!(commands.try_recv().is_err());

        let error = if saved {
            store.update_notes(&previous, notes).unwrap();
            replies
                .send(Response::State(
                    tests::question_snapshot(&app, &store),
                    None,
                ))
                .unwrap();
            app.poll(&context);
            assert!(
                matches!(commands.try_recv().unwrap(), Command::AskFromHistory { prompt, .. } if prompt == query.trim())
            );
            "The selected answer model became unavailable."
        } else {
            "The original note could not be saved."
        };
        replies.send(Response::Error(error.into())).unwrap();
        app.poll(&context);

        assert_eq!(app.notes_dirty, !saved);
        assert_eq!(app.notes, notes);
        assert_eq!(app.selected().unwrap().id, previous);
        assert_eq!(app.history_search, query);
        assert_eq!(app.question, "Keep this other unfinished draft.");
        assert!(app.question_submission.is_none());
        assert!(app.overlay_panel == OverlayPanel::History);
        assert!(commands.try_recv().is_err());
        let tree = frame(&mut app, &context, OverlayPanel::History, vec![]);
        assert!(
            tree.nodes
                .iter()
                .any(|(_, node)| node.value() == Some(error))
        );
    }
    assert_eq!(store.sessions("").unwrap().len(), 1);
    assert_eq!(store.session(&previous).unwrap().notes, notes);
}

#[test]
fn history_ask_refuses_oversized_unready_and_active_submissions() {
    let context = egui::Context::default();
    context.enable_accesskit();
    let (mut app, _store, _replies, commands) = tests::question_app(&context);
    let previous = app.selected().unwrap().id.clone();
    app.overlay_panel = OverlayPanel::History;
    let valid = "Original survey question";
    for (query, model_ready, generating) in [
        ("é".repeat(32_769), true, false),
        (valid.into(), false, false),
        (valid.into(), true, true),
    ] {
        app.history_search = query.clone();
        app.runtime.installed_profiles = if model_ready {
            vec![models::GENERATION_PROFILE.into()]
        } else {
            vec![]
        };
        app.runtime.generation = generating.then(|| crate::runtime::GenerationStatus {
            session_id: previous.clone(),
            request_id: "existing-response".into(),
            state: "generating".into(),
            text: "An independent partial response".into(),
        });
        let tree = frame(&mut app, &context, OverlayPanel::History, vec![]);
        let (action, _) = target(&tree, "Ask the assistant", 0);
        assert!(
            tree.nodes
                .iter()
                .any(|(id, node)| *id == action && node.is_disabled())
        );
        let _ = frame(&mut app, &context, OverlayPanel::History, activate(action));
        assert_eq!(app.history_search, query);
        assert!(app.question_submission.is_none());
        assert!(commands.try_recv().is_err());
        assert_eq!(app.generating(), generating);
    }
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
fn history_date_prefix_keeps_the_created_date_group_and_reopens_the_match() {
    let context = egui::Context::default();
    context.enable_accesskit();
    let (mut app, mut store, replies, commands) = tests::question_app(&context);
    let other = app.selected().unwrap().clone();
    let title = "Greenhouse layout";
    let matching = store.start_session(title, None).unwrap().id;
    let notes = "Sketch the second seed bed.";
    store.update_notes(&matching, notes).unwrap();
    store.finish_session(&matching).unwrap();
    let march_first = 1_709_251_200_000_i64;
    let mut backup: serde_json::Value =
        serde_json::from_str(&store.backup_json().unwrap()).unwrap();
    for row in backup["sessions"].as_array_mut().unwrap() {
        let (created, updated) = if row["id"] == matching {
            (march_first - 1, march_first + 86_400_000)
        } else {
            (march_first, march_first)
        };
        row["created_at"] = created.into();
        row["updated_at"] = updated.into();
    }
    store.restore_backup(&backup.to_string(), true).unwrap();
    let mut snapshot = tests::question_snapshot(&app, &store);
    snapshot.selected = Some(store.session(&other.id).unwrap());
    app.apply_snapshot(snapshot, None, false);
    app.overlay_panel = OverlayPanel::History;
    app.history_search = " 2024-02 ".into();

    let date = "2024-02-29 · UTC";
    let tree = frame(&mut app, &context, OverlayPanel::History, vec![]);
    let (_, bounds) = target(&tree, title, 0);
    assert!(
        tree.nodes
            .iter()
            .any(|(_, node)| node.value() == Some(date))
    );
    assert!(!tree.nodes.iter().any(|(_, node)| {
        node.value() == Some("2024-03-01 · UTC") || node.label() == Some(other.title.as_str())
    }));
    for pressed in [true, false] {
        let _ = frame(
            &mut app,
            &context,
            OverlayPanel::History,
            pointer(bounds.center(), pressed),
        );
    }
    assert!(matches!(commands.try_recv().unwrap(), Command::Open(id) if id == matching));
    assert!(commands.try_recv().is_err());
    assert!(app.overlay_panel == OverlayPanel::Detail);

    let mut opened = tests::question_snapshot(&app, &store);
    opened.selected = Some(store.session(&matching).unwrap());
    replies.send(Response::State(opened, None)).unwrap();
    app.poll(&context);
    assert_eq!(app.selected().unwrap().id, matching);
    assert_eq!(app.notes, notes);
    let tree = frame(&mut app, &context, OverlayPanel::Detail, vec![]);
    assert!(
        tree.nodes
            .iter()
            .any(|(_, node)| node.value() == Some(date))
    );
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

#[test]
fn stopped_history_badge_ignores_notes_and_followup_activity() {
    let context = egui::Context::default();
    context.enable_accesskit();
    let (mut app, mut store, _replies, commands) = tests::question_app(&context);
    let chat = store.start_chat(None).unwrap();
    let prior = store
        .begin_request(&chat.id, "Original prior question")
        .unwrap();
    assert!(
        store
            .complete_request(&chat.id, &prior.id, "Original prior answer")
            .unwrap()
    );
    let created_at = chat.created_at - 53 * 60_000;
    let mut backup: serde_json::Value =
        serde_json::from_str(&store.backup_json().unwrap()).unwrap();
    for row in backup["sessions"].as_array_mut().unwrap() {
        row["created_at"] = created_at.into();
        if row["id"] == chat.id {
            row["updated_at"] = (created_at + 1_616).into();
            row["requests"][0]["created_at"] = (created_at + 100).into();
            row["suggestions"][0]["created_at"] = (created_at + 1_616).into();
        } else {
            row["updated_at"] = (created_at + 2_000).into();
        }
    }
    store.restore_backup(&backup.to_string(), true).unwrap();
    assert_ne!(store.sessions("").unwrap()[0].id, chat.id);
    app.history_search = "New chat".into();
    let render = |app: &mut Cueglint, store: &cueglint_core::Store| {
        let mut snapshot = tests::question_snapshot(app, store);
        snapshot.selected = Some(store.session(&chat.id).unwrap());
        app.apply_snapshot(snapshot, None, false);
        context
            .run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(690.0, 760.0))),
                    ..Default::default()
                },
                |ui| app.history(ui),
            )
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text)
                    if matches!(text.galley.text(), "Active" | "Paused" | "Stopped")
                        || text.galley.text().ends_with(" min") =>
                {
                    Some(text.galley.text().to_owned())
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let mut badges = vec![render(&mut app, &store)];
    store
        .update_notes(&chat.id, "Original edited note stays intact.")
        .unwrap();
    assert_eq!(store.sessions("").unwrap()[0].id, chat.id);
    badges.push(render(&mut app, &store));
    let followup = store
        .begin_request(&chat.id, "Original follow-up question")
        .unwrap();
    assert_eq!(followup.context.chat.len(), 1);
    assert_eq!(followup.context.chat[0].response, "Original prior answer");
    assert!(
        store
            .complete_request(&chat.id, &followup.id, "Original follow-up answer")
            .unwrap()
    );
    badges.push(render(&mut app, &store));
    let saved = store.session(&chat.id).unwrap();
    assert_eq!(saved.status, SessionStatus::Stopped);
    assert_eq!(saved.created_at, created_at);
    assert!(saved.updated_at >= created_at + 53 * 60_000);
    assert!(saved.saved_at.is_none() && saved.transcript.is_empty());
    assert_eq!(saved.notes, "Original edited note stays intact.");
    assert_eq!(saved.requests.len(), 2);
    assert_eq!(saved.suggestions.len(), 2);
    assert!(app.active_session().is_none() && app.runtime.capture.is_none());
    assert!(commands.try_recv().is_err());
    assert_eq!(badges, vec![vec!["Stopped".to_owned()]; 3]);
}

#[test]
fn typing_in_meeting_detail_keeps_the_search_caret_in_history() {
    let context = egui::Context::default();
    context.enable_accesskit();
    let (mut app, _store, _replies, commands) = tests::question_app(&context);
    app.overlay_panel = OverlayPanel::Detail;
    app.history_search = "cycle".into();
    let selected = app.selected().unwrap().clone();
    let search = pos2(110.0, 25.0);
    for events in [vec![], pointer(search, true), pointer(search, false)] {
        frame(&mut app, &context, OverlayPanel::Detail, events);
    }
    frame(
        &mut app,
        &context,
        OverlayPanel::Detail,
        vec![
            egui::Event::Key {
                key: egui::Key::Home,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Text("Question ".into()),
        ],
    );
    assert!(app.overlay_panel == OverlayPanel::History);
    assert_eq!(app.history_search, "Question cycle");

    let tree = frame(
        &mut app,
        &context,
        OverlayPanel::History,
        vec![egui::Event::Text("life".into())],
    );
    assert_eq!(app.history_search, "Question lifecycle");
    target(&tree, &selected.title, 0);
    assert_eq!(app.selected().unwrap().id, selected.id);
    assert!(commands.try_recv().is_err());
}

#[test]
fn saved_transcript_shows_audio_position_even_if_transcription_finished_later() {
    use cueglint_core::{AudioMetadata, TranscriptSegment, TranscriptSource};

    let context = egui::Context::default();
    context.enable_accesskit();
    let (mut app, _store, _replies, _commands) = tests::question_app(&context);
    let session = app.state.as_mut().unwrap().selected.as_mut().unwrap();
    let manual = TranscriptSegment {
        id: "typed".into(),
        speaker: Speaker::You,
        text: "Manual entry".into(),
        finalized: true,
        source: TranscriptSource::Manual,
        created_at: session.created_at + 92_000,
        updated_at: session.created_at + 92_000,
        audio: None,
    };
    let recorded = TranscriptSegment {
        id: "recorded".into(),
        speaker: Speaker::Unknown,
        text: "Recorded entry".into(),
        source: TranscriptSource::SystemAudio,
        audio: Some(AudioMetadata {
            start_ns: 64_999_999_999,
            end_ns: 66_000_000_000,
            sample_rate: 16_000,
            model_profile: "original-fixture".into(),
            model_revision: "a".repeat(40),
            model_sha256: "b".repeat(64),
            language: "en".into(),
            backend: None,
            frontend_revision: None,
            frontend_config_sha256: None,
        }),
        ..manual.clone()
    };
    session.transcript = vec![recorded, manual];
    app.chat_tab = ChatTab::Transcript;
    let tree = frame(&mut app, &context, OverlayPanel::Detail, vec![]);
    let times: Vec<_> = tree
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == egui::accesskit::Role::Label)
        .filter_map(|(_, node)| node.value())
        .filter(|value| *value == "01:04" || *value == "01:32")
        .collect();
    assert_eq!(times, ["01:04", "01:32"]);
}

#[test]
fn saved_transcript_wraps_at_the_measured_pitch_and_aligns_with_its_speaker() {
    use egui::accesskit::Role;

    let context = egui::Context::default();
    context.enable_accesskit();
    let (mut app, _store, _replies, _commands) = tests::question_app(&context);
    let session = app.state.as_mut().unwrap().selected.as_mut().unwrap();
    let utterance = "The original paper lanterns will be ready on Thursday. ".repeat(6);
    session.transcript = vec![cueglint_core::TranscriptSegment {
        id: "layout-original".into(),
        speaker: Speaker::You,
        text: utterance.clone(),
        finalized: true,
        source: cueglint_core::TranscriptSource::Manual,
        created_at: session.created_at + 72_000,
        updated_at: session.created_at + 72_000,
        audio: None,
    }];
    app.chat_tab = ChatTab::Transcript;
    let tree = frame(&mut app, &context, OverlayPanel::Detail, vec![]);
    let label = |text: &str| {
        tree.nodes
            .iter()
            .map(|(_, node)| node)
            .find(|node| node.role() == Role::Label && node.value() == Some(text))
            .unwrap()
    };
    let paragraph = label(&utterance);
    let mut rows: Vec<_> = tree
        .nodes
        .iter()
        .filter(|(id, node)| paragraph.children().contains(id) && node.role() == Role::TextRun)
        .map(|(_, node)| node.bounds().unwrap())
        .collect();
    rows.sort_by(|a, b| a.y0.total_cmp(&b.y0));
    assert!(
        rows.len() >= 3,
        "expected an automatically wrapped utterance"
    );
    assert_eq!(label("01:12").bounds().unwrap().x0, 20.0);
    assert_eq!(label("YOU").bounds().unwrap().x0, 84.0);
    for row in &rows {
        assert_eq!(row.x0, 84.0);
        assert!(row.x1 <= 670.0);
    }
    for adjacent in rows.windows(2) {
        assert_eq!(adjacent[1].y0 - adjacent[0].y0, 28.0);
    }
}
