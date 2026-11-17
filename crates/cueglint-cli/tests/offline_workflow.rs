use serde_json::Value;
use std::{path::Path, process::Command};
use uuid::Uuid;

fn run(database: &Path, args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_cueglint-cli"))
        .arg("--db")
        .arg(database)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn cli_processes_share_a_complete_offline_workflow() {
    let directory = std::env::temp_dir().join(format!("cueglint-cli-{}", Uuid::new_v4()));
    let database = directory.join("original.sqlite3");
    let session = run(
        &database,
        &["session", "start", "CLI call", "--mode", "meeting"],
    );
    let id = session["id"].as_str().unwrap();
    run(
        &database,
        &[
            "transcript",
            id,
            "We should deliver on Friday",
            "--segment",
            "first",
            "--partial",
        ],
    );
    let finalized = run(
        &database,
        &[
            "transcript",
            id,
            "We should deliver on Friday.",
            "--segment",
            "first",
        ],
    );
    assert_eq!(finalized["transcript"].as_array().unwrap().len(), 1);
    run(
        &database,
        &["session", "notes", id, "Confirm with the team"],
    );
    run(&database, &["session", "stop", id]);
    let frozen = run(&database, &["request", "freeze", id, "What was agreed?"]);
    assert_eq!(frozen["inference_executed"], false);
    assert_eq!(frozen["request"]["status"], "cancelled");
    let backup = directory.join("backup.json");
    run(&database, &["session", "save", id]);
    let output = Command::new(env!("CARGO_BIN_EXE_cueglint-cli"))
        .arg("--db")
        .arg(&database)
        .args(["backup", "--output"])
        .arg(&backup)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let restored = directory.join("restored.sqlite3");
    run(&restored, &["restore", backup.to_str().unwrap()]);
    let history = run(&restored, &["session", "list", "Friday"]);
    assert_eq!(history.as_array().unwrap().len(), 1);
    assert_eq!(history[0]["notes"], "Confirm with the team");
    let missing = run(&restored, &["model-status"]);
    assert_eq!(missing["model"]["state"], "missing");
    assert_eq!(missing["inference_available"], false);
    run(&restored, &["session", "delete", id]);
    assert!(
        run(&restored, &["session", "list"])
            .as_array()
            .unwrap()
            .is_empty()
    );
    std::fs::remove_dir_all(directory).unwrap();
}
