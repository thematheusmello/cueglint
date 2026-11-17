use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Output},
};

fn settings(database: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cueglint-cli"))
        .arg("--db")
        .arg(database)
        .arg("settings")
        .args(arguments)
        .output()
        .unwrap()
}

fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn cli_sets_disables_and_restores_shortcuts_without_accepting_invalid_keys() {
    let directory =
        std::env::temp_dir().join(format!("cueglint-cli-shortcuts-{}", uuid::Uuid::new_v4()));
    let database = directory.join("state.sqlite3");
    let changed = success(settings(
        &database,
        &["set-shortcut", "new_chat", "Control+Alt+R"],
    ));
    assert_eq!(changed["shortcut_bindings"]["new_chat"], "Control+Alt+R");
    for arguments in [
        vec!["set-shortcut", "new_chat", "Shift+R"],
        vec!["disable-shortcut", "not_an_action"],
        vec!["reset-shortcut", "not_an_action"],
    ] {
        assert!(!settings(&database, &arguments).status.success());
        assert_eq!(success(settings(&database, &["show"])), changed);
    }
    let disabled = success(settings(&database, &["disable-shortcut", "new_chat"]));
    assert!(
        disabled["shortcut_bindings"]
            .as_object()
            .unwrap()
            .contains_key("new_chat")
    );
    assert!(disabled["shortcut_bindings"]["new_chat"].is_null());
    let reset = success(settings(&database, &["reset-shortcut", "new_chat"]));
    assert!(
        !reset["shortcut_bindings"]
            .as_object()
            .unwrap()
            .contains_key("new_chat")
    );
    std::fs::remove_dir_all(directory).unwrap();
}
