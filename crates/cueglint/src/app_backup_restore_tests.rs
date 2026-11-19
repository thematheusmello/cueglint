use super::*;
use cueglint_core::{RequestStatus, Settings, Store};
use std::fs;

#[test]
fn settings_restore_preserves_current_notes_then_reopens_the_replacement_locally() {
    let mut archive_store = Store::open_in_memory().unwrap();
    let hosted = ProviderConfig {
        provider: ProviderId::OpenAi,
        model_id: "fixture-model".into(),
        base_url: None,
    };
    archive_store
        .update_settings(Settings {
            answer_provider: hosted.clone(),
            transcription_language: "de".into(),
            ..Settings::default()
        })
        .unwrap();
    let archived = archive_store
        .start_session("Original archived meeting", None)
        .unwrap();
    let archived_notes = "Original archived notes.\n\nKeep the cobalt ledger.";
    archive_store
        .update_notes(&archived.id, archived_notes)
        .unwrap();
    let request = archive_store
        .begin_request(&archived.id, "What did we keep?")
        .unwrap();
    assert_eq!(request.provider, hosted);
    let archive = archive_store.backup_json().unwrap();
    let directory = std::env::temp_dir().join(format!("cueglint-restore-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("original archive.json");
    cueglint_core::files::write_private_export(&path, &archive, false).unwrap();

    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let creation = eframe::CreationContext::_new_kittest(ctx.clone());
    let mut app = Cueglint::new(
        &creation,
        Options {
            fixture: Some("native-v2-session".into()),
            ..Default::default()
        },
    );
    let finish = |app: &mut Cueglint| {
        let deadline = Instant::now() + Duration::from_secs(5);
        while (app.pending > 0 || app.after_notes.is_some()) && Instant::now() < deadline {
            app.poll(&ctx);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(app.pending, 0, "store worker did not reply");
        assert!(
            app.after_notes.is_none(),
            "note-dependent command did not run"
        );
    };
    finish(&mut app);
    let previous_id = app.selected().unwrap().id.clone();
    assert!(app.send(Command::Stop(previous_id.clone())));
    finish(&mut app);
    app.page = Page::Security;

    let draw = |app: &mut Cueglint, events| {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(860.0, 670.0))),
                events,
                ..Default::default()
            },
            |ui| app.main_window(ui),
        )
        .platform_output
        .accesskit_update
        .unwrap()
    };
    let click = |tree: &egui::accesskit::TreeUpdate, label| {
        let (target_node, _) = tree
            .nodes
            .iter()
            .find(|(_, node)| {
                node.label() == Some(label) && node.supports_action(egui::accesskit::Action::Click)
            })
            .unwrap_or_else(|| panic!("Missing accessible control {label}"));
        vec![egui::Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target_tree: egui::accesskit::TreeId::ROOT,
                target_node: *target_node,
                data: None,
            },
        )]
    };
    let _ = draw(&mut app, vec![]);
    ctx.memory_mut(|memory| memory.request_focus(Id::new("backup-file-path")));
    let settings = draw(
        &mut app,
        vec![egui::Event::Paste(path.to_str().unwrap().into())],
    );
    assert_eq!(app.backup_path, path.to_str().unwrap());
    let review = click(&settings, "Review restore");

    let _ = draw(&mut app, review.clone());
    let modal = draw(&mut app, vec![]);
    let current_edit = "Original unsaved edit before the declined replacement.";
    app.notes = current_edit.into();
    app.notes_dirty = true;
    let _ = draw(&mut app, click(&modal, "Restore backup"));
    assert!(app.notes_saving);
    finish(&mut app);
    assert!(app.notice.as_ref().is_some_and(
        |(message, failed)| *failed && message.contains("explicit replacement is required")
    ));
    assert_eq!(app.selected().unwrap().id, previous_id);
    assert_eq!(app.selected().unwrap().notes, current_edit);
    assert!(!app.notes_dirty);

    let _ = draw(&mut app, vec![]);
    let _ = draw(&mut app, review);
    let modal = draw(&mut app, vec![]);
    let _ = draw(
        &mut app,
        click(
            &modal,
            "Replace current sessions, modes, knowledge, and settings",
        ),
    );
    let confirmed = draw(&mut app, vec![]);
    let _ = draw(&mut app, click(&confirmed, "Replace and restore"));
    finish(&mut app);
    assert_eq!(
        app.notice,
        Some(("Backup restored. Answers use Local mode.".into(), false))
    );
    let restored = app.state.as_ref().unwrap();
    assert_eq!(restored.sessions.len(), 1);
    assert_eq!(restored.sessions[0].id, archived.id);
    assert_eq!(restored.settings.answer_provider, ProviderConfig::default());
    assert_eq!(app.runtime.answer_provider, ProviderConfig::default());
    assert_eq!(app.runtime.transcription_language, "de");
    assert!(app.selected().is_none());
    assert!(app.send(Command::Open(archived.id)));
    finish(&mut app);
    let reopened = app.selected().unwrap();
    assert_eq!(reopened.title, archived.title);
    assert_eq!(reopened.notes, archived_notes);
    assert_eq!(reopened.status, SessionStatus::Paused);
    assert_eq!(reopened.requests[0].status, RequestStatus::Cancelled);
    assert_eq!(reopened.requests[0].provider, hosted);
    assert_eq!(fs::read_to_string(&path).unwrap(), archive);
    fs::remove_file(path).unwrap();
    fs::remove_dir(directory).unwrap();
}
