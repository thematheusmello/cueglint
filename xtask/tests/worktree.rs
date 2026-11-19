use std::{fs, process::Command, time::SystemTime};

#[test]
fn registry_check_reads_the_invoking_cargo_workspace() {
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("cueglint-xtask-{}-{nonce}", std::process::id()));
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join("parity")).unwrap();
    fs::create_dir(root.join("xtask")).unwrap();
    fs::write(
        root.join("parity/features.yaml"),
        r#"{"schema_version":0,"baseline":"original fixture","features":[]}"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .env("CARGO_MANIFEST_DIR", root.join("xtask"))
        .arg("verify-registry")
        .output()
        .unwrap();
    fs::remove_dir_all(root).unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported registry schema"));
}
