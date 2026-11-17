//! A development check for the CLI boundary; live capture and GUI acceptance are separate.
use super::{fingerprint, hash_file, prepare_report, source_hash};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    fs,
    net::TcpListener,
    path::Path,
    process::Command,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const POLICY: &str = "(version 1)(allow default)(deny network*)";

pub fn check(models: &Path, report: &Path) -> Result<()> {
    prepare_report(report)?;
    ensure!(
        cfg!(target_os = "macos"),
        "This process-network-denial runner currently supports macOS only"
    );
    let build = Command::new("cargo")
        .args([
            "build",
            "--locked",
            "-p",
            "cueglint-cli",
            "--message-format=json",
        ])
        .output()
        .context("build CLI")?;
    eprint!("{}", String::from_utf8_lossy(&build.stderr));
    ensure!(build.status.success(), "CLI build failed");
    let binary = build_executable(&build.stdout)?;
    let models = models.canonicalize().context("installed model directory")?;
    let workspace = std::env::current_dir()?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let private =
        std::env::temp_dir().join(format!("cueglint-offline-{}-{stamp}", std::process::id()));
    fs::create_dir(&private)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&private, fs::Permissions::from_mode(0o700))?;
    }
    let before = source_hash()?;
    let binary_hash = hash_file(&binary)?;
    let db = private.join("workflow.sqlite3");
    let mut steps = Vec::new();
    let mut checks = json!({});
    let outcome = (|| -> Result<()> {
        // A reachable control rules out a failed listener as the reason the denied probe fails.
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port().to_string();
        let args = ["-z", "-w", "1", "127.0.0.1", &port];
        let control = Command::new("/usr/bin/nc").args(args).output()?;
        let denied = Command::new("/usr/bin/sandbox-exec")
            .args(["-p", POLICY, "/usr/bin/nc"])
            .args(args)
            .output()?;
        checks["network_canary"] = json!({"control_exit":control.status.code(),"denied_exit":denied.status.code(),"passed":control.status.success() && !denied.status.success(),"route":"loopback TCP; identical listener and command"});
        ensure!(
            control.status.success() && !denied.status.success(),
            "process network denial was not established"
        );
        let mut execute = |database: &Path, args: &[&str]| -> Result<Value> {
            let start = Instant::now();
            let output = Command::new("/usr/bin/sandbox-exec")
                .args(["-p", POLICY])
                .arg(&binary)
                .arg("--db")
                .arg(database)
                .arg("--models-dir")
                .arg(&models)
                .args(args)
                .output()?;
            let value = if output.stdout.is_empty() {
                Value::Null
            } else {
                serde_json::from_slice(&output.stdout).unwrap_or_else(
                    |_| json!({"unparsed_stdout":String::from_utf8_lossy(&output.stdout)}),
                )
            };
            steps.push(json!({"args":args,"elapsed_ms":start.elapsed().as_millis(),"exit":output.status.code(),"stdout":value,"stderr":String::from_utf8_lossy(&output.stderr)}));
            ensure!(
                output.status.success(),
                "offline CLI operation failed: {}",
                args.join(" ")
            );
            Ok(value)
        };
        let status = execute(&db, &["model-status"])?;
        ensure!(
            status["inference_available"] == true && status["transcription_available"] == true,
            "default models are not installed"
        );
        let wav = workspace.join("parity/fixtures/clean-english-v2.wav");
        let asr = execute(&db, &["transcribe", wav.to_str().context("fixture path")?])?;
        let text = asr["text"].as_str().context("ASR produced no text")?;
        ensure!(!text.trim().is_empty(), "ASR produced empty text");
        let started = execute(&db, &["session", "start", "Original offline CLI check"])?;
        let session = started["id"].as_str().context("session identity missing")?;
        execute(&db, &["transcript", session, text])?;
        let grounding = workspace.join("parity/fixtures/grounding.txt");
        execute(
            &db,
            &[
                "knowledge",
                "add",
                grounding.to_str().context("fixture path")?,
                "--title",
                "Original Meridian facts",
            ],
        )?;
        let answer = execute(
            &db,
            &[
                "ask",
                session,
                "According to the context, what is the monthly coffee budget?",
                "--stream",
            ],
        )?;
        ensure!(
            answer["applied"] == true
                && answer["output"]["text"]
                    .as_str()
                    .is_some_and(|t| t.contains("240") && !t.contains("300") && !t.contains("500")),
            "offline grounded answer failed the original fact check"
        );
        execute(
            &db,
            &[
                "session",
                "notes",
                session,
                "Original offline check: preserve these human notes.",
            ],
        )?;
        execute(&db, &["session", "stop", session])?;
        execute(&db, &["session", "save", session])?;
        let reopened = execute(&db, &["session", "show", session])?;
        ensure!(
            reopened["transcript"][0]["text"] == text && reopened["saved_at"].is_number(),
            "saved transcript did not reopen"
        );
        let export = execute(&db, &["export", session, "--format", "json"])?;
        ensure!(
            export["format"] == "cueglint-session"
                && export["version"] == 1
                && export["notes"] == reopened["notes"],
            "versioned export did not preserve human notes"
        );
        let export_path = private.join("session.json");
        fs::write(&export_path, serde_json::to_vec_pretty(&export)?)?;
        let backup = private.join("backup.json");
        execute(
            &db,
            &[
                "backup",
                "--output",
                backup.to_str().context("backup path")?,
            ],
        )?;
        let restored_db = private.join("restored.sqlite3");
        execute(
            &restored_db,
            &["restore", backup.to_str().context("backup path")?],
        )?;
        let restored = execute(&restored_db, &["session", "show", session])?;
        ensure!(
            restored == reopened,
            "backup/restore changed the saved session"
        );
        execute(&db, &["session", "delete", session])?;
        ensure!(
            execute(&db, &["session", "list"])?
                .as_array()
                .is_some_and(Vec::is_empty),
            "session deletion left a library entry"
        );
        checks["workflow"] = json!({"passed":true,"file_asr":true,"local_generation":true,"saved_reopened":true,"export_format":"cueglint-session-v1","backup_roundtrip":true,"session_deleted":true,"audio_capture":false,"screen_capture":false,"export_sha256":hash_file(&export_path)?});
        Ok(())
    })();
    let mut identity_errors = Vec::new();
    let after = fingerprint(source_hash(), "final source", &mut identity_errors);
    let binary_after = fingerprint(hash_file(&binary), "final CLI binary", &mut identity_errors);
    let manifest_hash = fingerprint(
        hash_file(Path::new("models/local-defaults.json")),
        "model manifest",
        &mut identity_errors,
    );
    let audio_hash = fingerprint(
        hash_file(Path::new("parity/fixtures/clean-english-v2.wav")),
        "audio fixture",
        &mut identity_errors,
    );
    let grounding_hash = fingerprint(
        hash_file(Path::new("parity/fixtures/grounding.txt")),
        "grounding fixture",
        &mut identity_errors,
    );
    let endpoint_match = after.as_deref() == Some(before.as_str())
        && binary_after.as_deref() == Some(binary_hash.as_str());
    let passed = outcome.is_ok() && endpoint_match && identity_errors.is_empty();
    let value = json!({"schema_version":1,"evidence_type":"implementation-only-real-offline-CLI","passed":passed,"status":if passed {"passed"} else {"failed"},"source_commit":Command::new("git").args(["rev-parse","HEAD"]).output().ok().and_then(|o|String::from_utf8(o.stdout).ok()).map(|s|s.trim().to_owned()),"source_hash":before,"source_hash_after":after,"source_and_binary_endpoint_match":endpoint_match,"cli_binary_sha256":binary_hash,"cli_binary_sha256_after":binary_after,"manifest_sha256":manifest_hash,"audio_fixture_sha256":audio_hash,"grounding_fixture_sha256":grounding_hash,"identity_errors":identity_errors,"platform":std::env::consts::OS,"architecture":std::env::consts::ARCH,"time_unix_ms":stamp,"network_policy":POLICY,"checks":checks,"steps":steps,"failure":outcome.as_ref().err().map(ToString::to_string),"limitations":["CLI and file ASR only; no live microphone/system audio, screen context, GUI, clean install or reference parity.","One original grounding case; ASR output is persisted and retained here, not a new accuracy benchmark.","Network-denial canary is checked; no packet audit or denied-syscall trace is claimed.","Endpoint fingerprints identify a working tree; there is no continuous filesystem watch or clean-build attestation."]});
    let public = serde_json::to_string_pretty(&value)?
        .replace(&*workspace.to_string_lossy(), "${WORKSPACE}")
        .replace(&*private.to_string_lossy(), "${RUN_DIR}")
        .replace(&*models.to_string_lossy(), "${MODEL_DIR}");
    cueglint_core::files::write_private_export(report, &format!("{public}\n"), false)?;
    ensure!(
        passed,
        "offline CLI check failed; report preserved at {}",
        report.display()
    );
    println!(
        "Offline CLI check passed; report {}. Private original data: {}",
        report.display(),
        private.display()
    );
    Ok(())
}

fn build_executable(stdout: &[u8]) -> Result<std::path::PathBuf> {
    let mut executable = None;
    for message in serde_json::Deserializer::from_slice(stdout).into_iter::<Value>() {
        let message = message.context("read Cargo artifact output")?;
        if message["reason"] == "compiler-artifact" && message["target"]["name"] == "cueglint-cli" {
            if let Some(path) = message["executable"].as_str() {
                executable = Some(path.into());
            }
        }
    }
    executable.context("Cargo reported no cueglint-cli executable")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cargo_artifact_identity_supports_a_configured_target_directory() {
        let messages = br#"{"reason":"compiler-artifact","target":{"name":"library"},"executable":null}
{"reason":"compiler-artifact","target":{"name":"cueglint-cli"},"executable":"/custom-target/debug/cueglint-cli"}
{"reason":"build-finished","success":true}"#;
        assert_eq!(
            build_executable(messages).unwrap(),
            Path::new("/custom-target/debug/cueglint-cli")
        );
        assert!(build_executable(b"invalid Cargo JSON").is_err());
        assert!(build_executable(br#"{"reason":"build-finished","success":true}"#).is_err());
    }
}
