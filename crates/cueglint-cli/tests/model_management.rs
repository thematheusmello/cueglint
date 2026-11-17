use cueglint_local::models;
use std::{
    path::Path,
    process::{Command, Output},
};
use uuid::Uuid;

fn run(base: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cueglint-cli"))
        .arg("--db")
        .arg(base.join("state.sqlite3"))
        .arg("--models-dir")
        .arg(base.join("models"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn cli_import_rejects_unpinned_local_bytes_without_replacing_existing_files() -> anyhow::Result<()>
{
    let base = std::env::temp_dir().join(format!("cueglint-cli-import-{}", Uuid::new_v4()));
    let source = base.join("source");
    let profile = models::profile(models::GENERATION_PROFILE)?;
    let destination = base.join("models").join(&profile.id);
    std::fs::create_dir_all(&source)?;
    std::fs::create_dir_all(&destination)?;
    for file in &profile.files {
        std::fs::write(
            source.join(&file.name),
            "Original tiny invalid source fixture",
        )?;
        std::fs::write(
            destination.join(&file.name),
            "Existing fixture must remain intact",
        )?;
    }
    let output = run(
        &base,
        &["models", "import", &profile.id, source.to_str().unwrap()],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("source model size"));
    for file in &profile.files {
        assert_eq!(
            std::fs::read_to_string(destination.join(&file.name))?,
            "Existing fixture must remain intact"
        );
        assert_eq!(
            std::fs::read_to_string(source.join(&file.name))?,
            "Original tiny invalid source fixture"
        );
    }
    assert_eq!(
        std::fs::read_dir(&destination)?.count(),
        profile.files.len() + 1
    );
    std::fs::remove_dir_all(base)?;
    Ok(())
}

#[test]
fn cli_remove_refuses_unknown_files_then_removes_only_catalog_names_from_a_synthetic_folder()
-> anyhow::Result<()> {
    let base = std::env::temp_dir().join(format!("cueglint-cli-remove-{}", Uuid::new_v4()));
    let profile = models::profile(models::TRANSCRIPTION_PROFILE)?;
    let destination = base.join("models").join(&profile.id);
    std::fs::create_dir_all(&destination)?;
    for file in &profile.files {
        std::fs::write(
            destination.join(&file.name),
            "Original disposable fixture bytes",
        )?;
    }
    std::fs::write(
        destination.join("human-notes.txt"),
        "Preserve this unknown file",
    )?;
    let refused = run(&base, &["models", "remove", &profile.id]);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("unknown entry"));
    for file in &profile.files {
        assert!(destination.join(&file.name).is_file());
    }
    assert_eq!(
        std::fs::read_to_string(destination.join("human-notes.txt"))?,
        "Preserve this unknown file"
    );
    std::fs::remove_file(destination.join("human-notes.txt"))?;
    let removed = run(&base, &["models", "remove", &profile.id]);
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    let output: serde_json::Value = serde_json::from_slice(&removed.stdout)?;
    assert_eq!(output["removed"], profile.id);
    assert_eq!(std::fs::read_dir(&destination)?.count(), 1);
    assert!(destination.join(".install.lock").is_file());
    std::fs::remove_dir_all(base)?;
    Ok(())
}
