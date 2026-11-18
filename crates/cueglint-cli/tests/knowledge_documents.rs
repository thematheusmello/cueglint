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
