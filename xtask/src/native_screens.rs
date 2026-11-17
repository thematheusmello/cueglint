//! Actual executable fixture capture. This does not compare against the commercial app.
use super::{cargo_executable, fingerprint, hash_file, prepare_report, source_hash, write_report};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const FIXTURES: [&str; 6] = [
    "dashboard",
    "overlay",
    "history",
    "session",
    "markdown",
    "markdown-long",
];
const MAX_LOG_BYTES: u64 = 1024 * 1024;
const MAX_BUILD_LOG_BYTES: u64 = 32 * 1024 * 1024;
const MAX_PNG_BYTES: u64 = 16 * 1024 * 1024;
const BUILD_SECONDS: u64 = 15 * 60;
const FIXTURE_SECONDS: u64 = 30;
const LOG_POLL_MS: u64 = 25;

fn new_log(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}

fn bounded_read(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "artifact exceeds its declared byte limit"
    );
    Ok(bytes)
}

fn own_process_group(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(not(unix))]
    let _ = command;
}

fn stop(child: &mut Child) -> Result<ExitStatus> {
    #[cfg(unix)]
    {
        // Cargo build scripts belong to the same owned group as the process we reap.
        let killed = Command::new("/bin/kill")
            .args(["-KILL", "--", &format!("-{}", child.id())])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        ensure!(
            killed.success() || child.try_wait()?.is_some(),
            "stop owned process group"
        );
    }
    #[cfg(not(unix))]
    child.kill()?;
    Ok(child.wait()?)
}

fn wait(
    child: &mut Child,
    deadline: Duration,
    logs: &[&Path],
    max_log_bytes: u64,
) -> Result<(ExitStatus, Option<&'static str>)> {
    let started = Instant::now();
    let result = (|| loop {
        let over_limit = logs
            .iter()
            .map(|path| fs::metadata(path).map(|m| m.len()))
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .any(|size| size > max_log_bytes);
        if over_limit {
            return Ok((stop(child)?, Some("log byte limit exceeded")));
        }
        if let Some(status) = child.try_wait()? {
            return Ok((status, None));
        }
        if started.elapsed() >= deadline {
            return Ok((stop(child)?, Some("deadline exceeded")));
        }
        thread::sleep(Duration::from_millis(LOG_POLL_MS));
    })();
    if result.is_err() {
        let _ = stop(child);
    }
    result
}

fn receipt(stdout: &[u8], fixture: &str, png: &Path) -> Result<Value> {
    let mut events = String::from_utf8_lossy(stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|event| event["event"] == "native_screenshot_saved")
        .collect::<Vec<_>>();
    ensure!(
        events.len() == 1,
        "expected exactly one native screenshot receipt"
    );
    let mut event = events.remove(0);
    ensure!(event["fixture"] == fixture, "receipt fixture mismatch");
    let reported = event["path"]
        .as_str()
        .context("receipt has no output path")?;
    ensure!(
        Path::new(reported).canonicalize()? == png.canonicalize()?,
        "receipt output path mismatch"
    );
    for field in ["pixels_per_point", "zoom_factor"] {
        ensure!(
            event[field]
                .as_f64()
                .is_some_and(|v| v.is_finite() && v > 0.0),
            "invalid native scale receipt"
        );
    }
    ensure!(
        fs::symlink_metadata(png)?.is_file(),
        "native PNG must be a regular file"
    );
    let bytes = bounded_read(png, MAX_PNG_BYTES)?;
    ensure!(
        bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "fixture did not produce PNG bytes"
    );
    let dimensions = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()?
        .into_dimensions()?;
    ensure!(
        dimensions.0 > 0 && dimensions.1 > 0 && dimensions.0 <= 4096 && dimensions.1 <= 4096,
        "fixture image dimensions exceed bounds"
    );
    let decoded = image::load_from_memory(&bytes)?;
    ensure!(
        decoded.width() == dimensions.0 && decoded.height() == dimensions.1,
        "inconsistent PNG dimensions"
    );
    ensure!(
        event["image"]["width"] == dimensions.0 && event["image"]["height"] == dimensions.1,
        "receipt image dimensions mismatch"
    );
    event["verified_png_sha256"] = json!(format!("{:x}", Sha256::digest(&bytes)));
    Ok(event)
}

pub fn check(output_dir: &Path, report: &Path) -> Result<()> {
    ensure!(
        cfg!(target_os = "macos"),
        "native fixture smoke currently runs on macOS only"
    );
    ensure!(
        !output_dir.try_exists()?,
        "choose a fresh native artifact directory"
    );
    fs::create_dir_all(output_dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(output_dir, fs::Permissions::from_mode(0o700))?;
    }
    prepare_report(report)?;
    let output_dir = output_dir.canonicalize()?;
    let workspace = std::env::current_dir()?;
    let started_at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let mut errors = Vec::new();
    let before = fingerprint(source_hash(), "initial source", &mut errors);
    let mut binary = None;
    let mut binary_before = None;
    let mut runs = Vec::new();
    let mut build = Value::Null;
    let outcome = (|| -> Result<()> {
        ensure!(
            before.is_some(),
            "source identity unavailable before native run"
        );
        let build_stdout = output_dir.join("build.jsonl");
        let build_stderr = output_dir.join("build.stderr");
        let mut command = Command::new("cargo");
        command
            .args([
                "build",
                "--locked",
                "-p",
                "cueglint",
                "--message-format=json",
            ])
            .stdout(Stdio::from(new_log(&build_stdout)?))
            .stderr(Stdio::from(new_log(&build_stderr)?));
        own_process_group(&mut command);
        let mut child = command.spawn()?;
        let (status, stopped) = wait(
            &mut child,
            Duration::from_secs(BUILD_SECONDS),
            &[&build_stdout, &build_stderr],
            MAX_BUILD_LOG_BYTES,
        )?;
        build = json!({"exit_code":status.code(),"stop_reason":stopped,"deadline_seconds":BUILD_SECONDS,"sampled_log_limit_bytes":MAX_BUILD_LOG_BYTES,"stdout":"build.jsonl","stderr":"build.stderr"});
        ensure!(
            status.success() && stopped.is_none(),
            "native executable build failed or exceeded limits"
        );
        let executable = cargo_executable(
            &bounded_read(&build_stdout, MAX_BUILD_LOG_BYTES)?,
            "cueglint",
        )?;
        binary_before = Some(hash_file(&executable)?);
        binary = Some(executable.clone());
        for fixture in FIXTURES {
            let png = output_dir.join(format!("{fixture}.png"));
            let stdout_path = output_dir.join(format!("{fixture}.stdout"));
            let stderr_path = output_dir.join(format!("{fixture}.stderr"));
            let started = Instant::now();
            let mut partial = json!({"fixture":fixture,"passed":false});
            let result = (|| -> Result<Value> {
                let mut command = Command::new(&executable);
                command
                    .args(["--fixture", fixture, "--screenshot"])
                    .arg(&png)
                    .stdout(Stdio::from(new_log(&stdout_path)?))
                    .stderr(Stdio::from(new_log(&stderr_path)?));
                own_process_group(&mut command);
                let mut child = command.spawn()?;
                let (status, stopped) = wait(
                    &mut child,
                    Duration::from_secs(FIXTURE_SECONDS),
                    &[&stdout_path, &stderr_path],
                    MAX_LOG_BYTES,
                )?;
                partial["exit_code"] = json!(status.code());
                partial["stop_reason"] = json!(stopped);
                let stdout = bounded_read(&stdout_path, MAX_LOG_BYTES)?;
                let stderr = bounded_read(&stderr_path, MAX_LOG_BYTES)?;
                let event = receipt(&stdout, fixture, &png);
                let passed = status.success() && stopped.is_none() && event.is_ok();
                let png_hash = event
                    .as_ref()
                    .ok()
                    .and_then(|value| value.get("verified_png_sha256"))
                    .cloned();
                Ok(
                    json!({"fixture":fixture,"passed":passed,"exit_code":status.code(),"stop_reason":stopped,"elapsed_ms":started.elapsed().as_millis(),"receipt":event.as_ref().ok(),"receipt_error":event.err().map(|error|format!("{error:#}")),"stdout":String::from_utf8_lossy(&stdout),"stderr":String::from_utf8_lossy(&stderr),"png":png.file_name().unwrap().to_string_lossy(),"png_sha256":png_hash}),
                )
            })();
            runs.push(result.unwrap_or_else(|error| {
                partial["failure"] = json!(format!("{error:#}"));
                partial["elapsed_ms"] = json!(started.elapsed().as_millis());
                partial
            }));
        }
        ensure!(
            runs.iter().all(|run| run["passed"] == true),
            "one or more native fixtures failed"
        );
        Ok(())
    })();
    let after = fingerprint(source_hash(), "final source", &mut errors);
    let binary_after = binary
        .as_ref()
        .and_then(|path| fingerprint(hash_file(path), "final binary", &mut errors));
    let identities_match = before.is_some()
        && before == after
        && binary_before.is_some()
        && binary_before == binary_after;
    let passed = outcome.is_ok() && identities_match && errors.is_empty();
    let command_text = |program: &str, arg: &str| {
        Command::new(program)
            .arg(arg)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    };
    let source_commit = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
    let value = json!({
        "schema_version": 1,
        "evidence_type": "implementation-only-native-GPU-fixture-smoke",
        "status": if passed {"passed"} else {"failed"},
        "passed": passed,
        "source_commit": source_commit,
        "source_hash": before,
        "source_hash_after": after,
        "binary_sha256": binary_before,
        "binary_sha256_after": binary_after,
        "source_binary_endpoints_match": identities_match,
        "identity_errors": errors,
        "platform": std::env::consts::OS,
        "architecture": std::env::consts::ARCH,
        "os_version": command_text("sw_vers", "-productVersion"),
        "os_build": command_text("sw_vers", "-buildVersion"),
        "interface_locale": "English fixture strings",
        "theme": "Cueglint dark fixture style",
        "started_at_unix_ms": started_at,
        "time_unix_ms": SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
        "configuration": {
            "fixtures": FIXTURES,
            "build_command": ["cargo", "build", "--locked", "-p", "cueglint", "--message-format=json"],
            "fixture_arguments": ["--fixture", "FIXTURE", "--screenshot", "FRESH_PNG_PATH"],
            "build_deadline_seconds": BUILD_SECONDS,
            "fixture_deadline_seconds": FIXTURE_SECONDS,
            "log_poll_ms": LOG_POLL_MS,
            "fixture_log_limit_bytes": MAX_LOG_BYTES,
            "build_log_limit_bytes": MAX_BUILD_LOG_BYTES,
            "png_limit_bytes": MAX_PNG_BYTES,
            "model_configuration": "not-applicable-no-inference"
        },
        "build": build,
        "runs": runs,
        "failure": outcome.err().map(|e| format!("{e:#}")),
        "limitations": [
            "Original labelled fixtures only; no models, live capture, reference account or proprietary artifacts.",
            "Primary viewport GPU readback: OS backdrop/blur/shadow/compositing, real child windows, focus/shortcuts, capture exclusion and AX are not verified.",
            "No reference visual comparison, clean installation, inference quality or platform-support extrapolation.",
            "Endpoint identities do not provide continuous filesystem monitoring.",
            "Log size is sampled every25ms; a producer can exceed the limit between polls. Stopped process groups are killed and their leader reaped."
        ]
    });
    let text = serde_json::to_string_pretty(&value)?
        .replace(&*workspace.to_string_lossy(), "${WORKSPACE}")
        .replace(&*output_dir.to_string_lossy(), "${ARTIFACT_DIR}");
    write_report(report, &serde_json::from_str::<Value>(&text)?)?;
    ensure!(
        passed,
        "native fixture smoke failed; report and raw artifacts preserved"
    );
    println!("Native fixture smoke passed; report {}", report.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cargo_selects_the_reported_gui_binary_and_rejects_ambiguous_artifacts() {
        let output = br#"{"reason":"compiler-artifact","target":{"name":"cueglint"},"executable":"/custom/debug/cueglint"}"#;
        assert_eq!(
            cargo_executable(output, "cueglint").unwrap(),
            Path::new("/custom/debug/cueglint")
        );
        assert!(cargo_executable(output, "cueglint-cli").is_err());
        assert!(
            cargo_executable(
                &[output.as_slice(), b"\n", output.as_slice()].concat(),
                "cueglint"
            )
            .is_err()
        );
    }
    #[test]
    fn output_receipt_binds_png_fixture_and_scale_without_running_a_window() -> Result<()> {
        let directory = std::env::temp_dir().join(format!(
            "cueglint-native-check-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        fs::create_dir(&directory)?;
        let png = directory.join("fixture.png");
        let outcome = (|| -> Result<()> {
            image::RgbaImage::from_pixel(3, 2, image::Rgba([255, 0, 0, 128])).save(&png)?;
            let mut event = json!({"event":"native_screenshot_saved","fixture":"markdown","path":png,"image":{"width":3,"height":2},"pixels_per_point":2.0,"zoom_factor":1.0,"adapter":null});
            assert!(receipt(event.to_string().as_bytes(), "markdown", &png).is_ok());
            assert!(receipt(event.to_string().as_bytes(), "overlay", &png).is_err());
            event["image"]["width"] = json!(4);
            assert!(receipt(event.to_string().as_bytes(), "markdown", &png).is_err());
            event["image"]["width"] = json!(3);
            event["pixels_per_point"] = json!(0);
            assert!(receipt(event.to_string().as_bytes(), "markdown", &png).is_err());
            Ok(())
        })();
        fs::remove_dir_all(directory)?;
        outcome
    }

    #[test]
    fn child_wait_probe() {
        if std::env::var_os("CUEGLINT_XTASK_WAIT_PROBE").is_some() {
            thread::sleep(Duration::from_secs(2));
        }
    }

    #[test]
    fn deadline_reaps_its_child_without_launching_a_window() -> Result<()> {
        let mut command = Command::new(std::env::current_exe()?);
        command
            .args(["--exact", "native_screens::tests::child_wait_probe"])
            .env("CUEGLINT_XTASK_WAIT_PROBE", "1")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        own_process_group(&mut command);
        let mut child = command.spawn()?;
        let (_, stopped) = wait(&mut child, Duration::from_millis(10), &[], MAX_LOG_BYTES)?;
        ensure!(stopped == Some("deadline exceeded"));
        ensure!(child.try_wait()?.is_some());
        Ok(())
    }

    #[test]
    fn oversized_logs_stop_and_reap_the_child_and_keep_the_log() -> Result<()> {
        let log = std::env::temp_dir().join(format!("cueglint-log-limit-{}", temporary_suffix()));
        let mut command = Command::new(std::env::current_exe()?);
        command
            .args(["--exact", "native_screens::tests::child_wait_probe"])
            .env("CUEGLINT_XTASK_WAIT_PROBE", "1")
            .stdout(Stdio::from(new_log(&log)?))
            .stderr(Stdio::null());
        own_process_group(&mut command);
        let mut child = command.spawn()?;
        let outcome = (|| -> Result<()> {
            let (_, stopped) = wait(&mut child, Duration::from_secs(1), &[&log], 1)?;
            ensure!(stopped == Some("log byte limit exceeded"));
            ensure!(child.try_wait()?.is_some());
            ensure!(fs::metadata(&log)?.len() > 1);
            Ok(())
        })();
        fs::remove_file(log)?;
        outcome
    }

    fn temporary_suffix() -> String {
        format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }
}
