use serde_json::Value;
use std::{fs, path::Path, process::Command};

fn command(database: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_cueglint-cli"))
        .arg("--db")
        .arg(database)
        .args(args)
        .output()
        .unwrap()
}

fn json(database: &Path, args: &[&str]) -> Value {
    let output = command(database, args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn docx_cli_import_quotes_extracted_text_and_delete_invalidates_search() {
    let directory =
        std::env::temp_dir().join(format!("cueglint-cli-docx-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&directory).unwrap();
    let database = directory.join("data.sqlite3");
    let path = directory.join("original.docx");
    fs::write(
        &path,
        include_bytes!("../../cueglint-core/tests/fixtures/knowledge/original-workshop.docx"),
    )
    .unwrap();
    let document = json(
        &database,
        &[
            "knowledge",
            "add",
            path.to_str().unwrap(),
            "--title",
            "Original workshop",
        ],
    );
    assert!(document["text"].as_str().unwrap().contains("SAFFRON-929"));
    let found = json(&database, &["knowledge", "search", "safety lanterns"]);
    assert_eq!(found[0]["document_id"], document["id"]);
    assert_eq!(found[0]["source"], path.to_str().unwrap());
    let text = document["text"].as_str().unwrap();
    let start = found[0]["start_byte"].as_u64().unwrap() as usize;
    let end = found[0]["end_byte"].as_u64().unwrap() as usize;
    assert_eq!(found[0]["text"], &text[start..end]);
    for (name, contents) in [
        ("bad.docx", "malformed archive"),
        ("malformed.pdf", "%PDF-1.7\n"),
    ] {
        let path = directory.join(name);
        fs::write(&path, contents).unwrap();
        let output = command(&database, &["knowledge", "add", path.to_str().unwrap()]);
        assert!(!output.status.success());
        if name.ends_with("pdf") {
            assert!(String::from_utf8_lossy(&output.stderr).contains("PDF is malformed"));
        }
        assert_eq!(
            json(&database, &["knowledge", "list"])
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    json(
        &database,
        &["knowledge", "delete", document["id"].as_str().unwrap()],
    );
    assert!(
        json(&database, &["knowledge", "search", "SAFFRON-929"])
            .as_array()
            .unwrap()
            .is_empty()
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn mode_file_settings_apply_to_cli_requests_across_reopen() {
    let directory =
        std::env::temp_dir().join(format!("cueglint-cli-mode-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&directory).unwrap();
    let database = directory.join("data.sqlite3");
    let one = directory.join("orchid.txt");
    let two = directory.join("terrace.txt");
    fs::write(&one, "Workshop marker Orchid-731.").unwrap();
    fs::write(&two, "Workshop marker Terrace-993.").unwrap();
    let attached = json(&database, &["knowledge", "add", one.to_str().unwrap()]);
    let attached_id = attached["id"].as_str().unwrap();
    let other = json(&database, &["knowledge", "add", two.to_str().unwrap()]);
    let mode = json(
        &database,
        &[
            "mode",
            "create",
            "Workshop",
            "Use the selected reference files.",
        ],
    );
    let mode_id = mode["id"].as_str().unwrap();
    assert!(mode.get("knowledge_document_ids").is_none());
    let session = json(
        &database,
        &["session", "start", "Workshop", "--mode", mode_id],
    );
    let session_id = session["id"].as_str().unwrap();
    let freeze = || {
        let mut result = json(
            &database,
            &["request", "freeze", session_id, "Workshop marker"],
        );
        assert_eq!(result["inference_executed"], false);
        result.get_mut("request").unwrap().take()
    };
    assert_eq!(
        freeze()["context"]["knowledge"].as_array().unwrap().len(),
        2
    );

    let mode = json(&database, &["mode", "set-files", mode_id, attached_id]);
    assert_eq!(
        mode["knowledge_document_ids"],
        serde_json::json!([attached_id])
    );
    let mode = json(
        &database,
        &[
            "mode",
            "update",
            mode_id,
            "Renamed workshop",
            "Use workshop facts.",
        ],
    );
    assert_eq!(
        mode["knowledge_document_ids"],
        serde_json::json!([attached_id])
    );
    let frozen = freeze();
    let snippets = frozen["context"]["knowledge"].as_array().unwrap();
    assert_eq!(snippets.len(), 1);
    assert_eq!(snippets[0]["document_id"], attached["id"]);
    assert_eq!(snippets[0]["text"], attached["text"]);

    json(&database, &["knowledge", "delete", attached_id]);
    let mode = json(&database, &["mode", "show", mode_id]);
    assert_eq!(mode["knowledge_document_ids"], serde_json::json!([]));
    assert_eq!(freeze()["context"]["knowledge"], serde_json::json!([]));
    let mode = json(&database, &["mode", "set-files", mode_id, "--all"]);
    assert!(mode.get("knowledge_document_ids").is_none());
    assert_eq!(
        freeze()["context"]["knowledge"][0]["document_id"],
        other["id"]
    );
    assert!(
        !command(
            &database,
            &[
                "mode",
                "set-files",
                mode_id,
                "--all",
                other["id"].as_str().unwrap()
            ]
        )
        .status
        .success()
    );
    let mode = json(&database, &["mode", "set-files", mode_id]);
    assert_eq!(mode["knowledge_document_ids"], serde_json::json!([]));
    assert_eq!(freeze()["context"]["knowledge"], serde_json::json!([]));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn duplicate_mode_saves_an_editable_copy_with_the_exact_file_scope() {
    use cueglint_core::{Mode, Store};

    let directory = std::env::temp_dir().join(format!(
        "cueglint-cli-duplicate-mode-{}",
        uuid::Uuid::new_v4()
    ));
    let database = directory.join("data.sqlite3");
    let name = "🪁".repeat(50);
    let (sources, archived, active) = {
        let mut store = Store::open(&database).unwrap();
        let library = store.mode("general").unwrap();
        let mut empty = Mode::new("Workshop", "Use conversation context.");
        empty.knowledge_document_ids = Some(Vec::new());
        let empty = store.upsert_mode(empty).unwrap();
        let attached = store
            .ingest_knowledge("Original reference", "original.txt", "Kite marker 731.")
            .unwrap();
        let mut selected = Mode::new(&name, "Use the selected file.\nKeep the original wording.");
        selected.knowledge_document_ids = Some(vec![attached.id]);
        let selected = store.upsert_mode(selected).unwrap();
        let archived = store
            .start_session("Archived", Some(&selected.id))
            .unwrap()
            .id;
        store.stop_session(&archived).unwrap();
        store.save_session(&archived).unwrap();
        let active = store
            .start_session("Active", Some(&selected.id))
            .unwrap()
            .id;
        ([library, empty, selected], archived, active)
    };
    assert_eq!(name.len(), 200);
    let expected_titles = [
        "General (copy)".to_owned(),
        "Workshop (copy)".to_owned(),
        format!("{} (copy)", "🪁".repeat(48)),
    ];
    for (source, title) in sources.iter().zip(expected_titles) {
        let saved = json(&database, &["mode", "duplicate", &source.id]);
        let copied: Mode = serde_json::from_value(saved.clone()).unwrap();
        assert_ne!(copied.id, source.id);
        assert!(!copied.builtin);
        assert_eq!(copied.name, title);
        assert_eq!(copied.instructions, source.instructions);
        assert_eq!(copied.knowledge_document_ids, source.knowledge_document_ids);
        assert_eq!(json(&database, &["mode", "show", &copied.id]), saved);
        assert_eq!(
            json(&database, &["mode", "show", &source.id]),
            serde_json::to_value(source).unwrap()
        );
    }
    for id in [archived, active] {
        assert_eq!(
            json(&database, &["session", "show", &id])["mode_id"],
            sources[2].id
        );
    }
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn editable_mode_outline_seeds_only_new_meetings_and_round_trips_through_backup() {
    let directory =
        std::env::temp_dir().join(format!("cueglint-note-outline-{}", uuid::Uuid::new_v4()));
    let db = directory.join("source.db");
    let restored = directory.join("restored.db");
    let archive = directory.join("backup.json");
    let starter = json(&db, &["mode", "set-note-template", "meeting"]);
    assert_eq!(
        starter["note_template"],
        cueglint_core::MEETING_NOTE_OUTLINE
    );
    let copy = json(&db, &["mode", "duplicate", "meeting"]);
    let mode = copy["id"].as_str().unwrap();
    assert_eq!(copy["note_template"], starter["note_template"]);
    let outline = "## Tide table\r\n\r\n## Harbour café\n";
    json(&db, &["mode", "set-note-template", mode, outline]);
    let updated = json(
        &db,
        &["mode", "update", mode, "Harbour", "Cite the measurements."],
    );
    assert_eq!(updated["note_template"], outline);
    let meeting = json(&db, &["session", "start", "Pier survey", "--mode", mode]);
    let id = meeting["id"].as_str().unwrap();
    assert_eq!(meeting["notes"], outline);
    assert_eq!(meeting["notes_revision"], 1);
    let edited = "## Survey\nThe pier rose by 3 cm.";
    json(&db, &["session", "notes", id, edited]);
    json(&db, &["session", "set-mode", id, "meeting"]);
    let resumed = json(&db, &["session", "pause", id]);
    assert_eq!(resumed["notes"], edited);
    assert_eq!(json(&db, &["session", "resume", id])["notes_revision"], 2);
    json(&db, &["session", "stop", id]);
    let backup = json(&db, &["backup"]);
    assert_eq!(backup["version"], 5);
    fs::write(&archive, serde_json::to_vec(&backup).unwrap()).unwrap();
    json(&restored, &["restore", archive.to_str().unwrap()]);
    assert_eq!(json(&restored, &["session", "show", id])["notes"], edited);
    assert_eq!(
        json(
            &restored,
            &["session", "start", "Second survey", "--mode", mode]
        )["notes"],
        outline
    );
    json(&db, &["mode", "set-note-template", mode, ""]);
    assert_eq!(
        json(&db, &["session", "start", "Third survey", "--mode", mode])["notes"],
        ""
    );
    fs::remove_dir_all(directory).unwrap();
}
