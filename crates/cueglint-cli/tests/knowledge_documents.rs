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
