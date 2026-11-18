use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Output},
};

fn command(database: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cueglint-cli"))
        .arg("--db")
        .arg(database)
        .args(args)
        .output()
        .unwrap()
}

fn run(database: &Path, args: &[&str]) -> Value {
    let output = command(database, args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn saved_language_and_one_file_overrides_never_substitute_models_or_touch_notes() {
    let directory =
        std::env::temp_dir().join(format!("cueglint-language-cli-{}", uuid::Uuid::new_v4()));
    let database = directory.join("state.sqlite3");
    let session = run(
        &database,
        &["session", "start", "Original language selection"],
    );
    let session = session["id"].as_str().unwrap();
    run(
        &database,
        &["session", "notes", session, "Keep exact notes. Café 🦀"],
    );
    assert_eq!(
        run(&database, &["settings", "show"])["transcription_language"],
        "en"
    );
    run(&database, &["settings", "set-transcription-language", "pt"]);
    let settings = run(&database, &["settings", "show"]);
    assert_eq!(settings["transcription_language"], "pt");
    assert!(settings["transcription_profile"].is_null());
    assert_eq!(settings["transcription_backend"], "cpu");
    let status = run(&database, &["model-status"]);
    assert_eq!(status["transcription_language"], "pt");
    assert_eq!(status["transcription_available"], false);
    assert!(
        status["transcription"]["error"]
            .as_str()
            .unwrap()
            .contains("English-only")
    );
    let wav = directory.join("authored-invalid.wav");
    std::fs::write(&wav, b"Original non-WAV fixture bytes").unwrap();
    let incompatible = command(&database, &["transcribe", wav.to_str().unwrap()]);
    assert!(!incompatible.status.success());
    assert!(String::from_utf8_lossy(&incompatible.stderr).contains("English-only"));
    for arguments in [
        vec!["transcribe", wav.to_str().unwrap(), "--language", "en"],
        vec![
            "transcribe",
            wav.to_str().unwrap(),
            "--profile",
            "whisper-base",
            "--language",
            "pt",
        ],
    ] {
        let overridden = command(&database, &arguments);
        assert!(!overridden.status.success());
        let error = String::from_utf8_lossy(&overridden.stderr);
        assert!(!error.contains("English-only"), "{error}");
        assert!(
            !error.contains("install the"),
            "invalid WAV must fail before weights: {error}"
        );
        assert_eq!(run(&database, &["settings", "show"]), settings);
    }
    let invalid = command(
        &database,
        &["settings", "set-transcription-language", "auto"],
    );
    assert!(!invalid.status.success());
    assert_eq!(run(&database, &["settings", "show"]), settings);
    run(
        &database,
        &["settings", "set-transcription-profile", "whisper-base"],
    );
    let selected = run(&database, &["model-status"]);
    assert_eq!(selected["transcription"]["profile_id"], "whisper-base");
    assert_eq!(selected["transcription_language"], "pt");
    assert_eq!(selected["transcription_available"], false);
    assert!(!directory.join("models").exists());
    assert_eq!(
        run(&database, &["session", "show", session])["notes"],
        "Keep exact notes. Café 🦀"
    );
    std::fs::remove_dir_all(directory).unwrap();
}
