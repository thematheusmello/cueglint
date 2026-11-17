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
const V2_FIXTURES: [&str; 7] = [
    "native-v2-dashboard",
    "native-v2-overlay",
    "native-v2-history",
    "native-v2-session",
    "native-v2-markdown",
    "native-v2-markdown-long",
    "provider-v2",
];

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum FixtureSet {
    Legacy,
    NativeV2,
}

impl FixtureSet {
    fn label(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::NativeV2 => "native-v2",
        }
    }
    fn views(self) -> &'static [&'static str] {
        match self {
            Self::Legacy => &FIXTURES,
            Self::NativeV2 => &V2_FIXTURES,
        }
    }
    fn data_path(self) -> Option<&'static str> {
        match self {
            Self::Legacy => None,
            Self::NativeV2 => Some("ui/fixtures/native-v2.json"),
        }
    }
    fn view_state_path(self) -> Option<&'static str> {
        match self {
            Self::Legacy => None,
            Self::NativeV2 => Some("ui/fixtures/provider-v2.json"),
        }
    }
}
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

fn receipt(stdout: &[u8], stderr: &[u8], fixture: &str, png: &Path) -> Result<Value> {
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
    let request_id = event["request_id"]
        .as_str()
        .context("receipt has no screenshot request identity")?;
    ensure!(
        request_id.len() == 36
            && request_id.bytes().enumerate().all(|(i, c)| {
                if matches!(i, 8 | 13 | 18 | 23) {
                    c == b'-'
                } else {
                    c.is_ascii_hexdigit()
                }
            }),
        "invalid screenshot request identity"
    );
    let font_set = event["fonts"]["identity"]["font_set_id"]
        .as_str()
        .context("receipt has no font-set identity")?;
    ensure!(
        event["fonts"]["state"] == "ready"
            && font_set.len() == 64
            && font_set.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "screenshot fonts were not settled and identified"
    );
    let trace: Vec<Value> = String::from_utf8_lossy(stderr)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|line| line["fixture"] == fixture && line["viewport"] == "root")
        .collect();
    for (kind, stage) in [
        ("native_screenshot_stage", "requested"),
        ("native_screenshot_transport", "copy_submitted"),
        ("native_screenshot_transport", "readback_ready"),
    ] {
        ensure!(
            trace
                .iter()
                .filter(|line| line["request_id"] == request_id
                    && line["event"] == kind
                    && line["stage"] == stage)
                .count()
                == 1,
            "screenshot receipt missing unique {stage} trace for its request"
        );
    }
    ensure!(
        trace.iter().any(|line| line["request_id"] == request_id
            && line["event"] == "native_screenshot_stage"
            && line["stage"] == "requested"
            && line["font_state"]["state"] == "ready"
            && line["font_state"]["identity"]["font_set_id"] == font_set),
        "screenshot requested and saved font sets differ"
    );
    ensure!(
        !trace
            .iter()
            .any(|line| line["event"] == "native_screenshot_transport"
                && matches!(
                    line["stage"].as_str(),
                    Some(
                        "map_failed"
                            | "format_unsupported"
                            | "delivery_failed"
                            | "queue_overflow"
                            | "viewport_dropped"
                    )
                )),
        "screenshot request reported a terminal transport failure"
    );
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

pub fn check(output_dir: &Path, report: &Path, fixture_set: FixtureSet) -> Result<()> {
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
    let fixture_before = fixture_set.data_path().and_then(|path| {
        fingerprint(
            hash_file(Path::new(path)),
            "initial fixture data",
            &mut errors,
        )
    });
    let view_before = fixture_set.view_state_path().and_then(|path| {
        fingerprint(
            hash_file(Path::new(path)),
            "initial fixture view state",
            &mut errors,
        )
    });
    let mut binary = None;
    let mut binary_before = None;
    let mut runs = Vec::new();
    let mut build = Value::Null;
    let outcome = (|| -> Result<()> {
        ensure!(
            before.is_some(),
            "source identity unavailable before native run"
        );
        ensure!(
            fixture_set.data_path().is_none() || fixture_before.is_some(),
            "fixture data identity unavailable before native run"
        );
        ensure!(
            fixture_set.view_state_path().is_none() || view_before.is_some(),
            "fixture view identity unavailable before native run"
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
        for &fixture in fixture_set.views() {
            let png = output_dir.join(format!("{fixture}.png"));
            let stdout_path = output_dir.join(format!("{fixture}.stdout"));
            let stderr_path = output_dir.join(format!("{fixture}.stderr"));
            let started = Instant::now();
            let view_hash = if fixture == "provider-v2" {
                view_before.clone()
            } else {
                None
            };
            let mut partial = json!({"fixture":fixture,"fixture_data_sha256":fixture_before,"view_state_sha256":view_hash,"passed":false});
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
                let event = receipt(&stdout, &stderr, fixture, &png);
                let passed = status.success() && stopped.is_none() && event.is_ok();
                let png_hash = event
                    .as_ref()
                    .ok()
                    .and_then(|value| value.get("verified_png_sha256"))
                    .cloned();
                Ok(
                    json!({"fixture":fixture,"fixture_data_sha256":fixture_before,"view_state_sha256":view_hash,"passed":passed,"exit_code":status.code(),"stop_reason":stopped,"elapsed_ms":started.elapsed().as_millis(),"receipt":event.as_ref().ok(),"receipt_error":event.err().map(|error|format!("{error:#}")),"stdout":String::from_utf8_lossy(&stdout),"stderr":String::from_utf8_lossy(&stderr),"png":png.file_name().unwrap().to_string_lossy(),"png_sha256":png_hash}),
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
    let fixture_after = fixture_set.data_path().and_then(|path| {
        fingerprint(
            hash_file(Path::new(path)),
            "final fixture data",
            &mut errors,
        )
    });
    let view_after = fixture_set.view_state_path().and_then(|path| {
        fingerprint(
            hash_file(Path::new(path)),
            "final fixture view state",
            &mut errors,
        )
    });
    let binary_after = binary
        .as_ref()
        .and_then(|path| fingerprint(hash_file(path), "final binary", &mut errors));
    let identities_match = before.is_some()
        && before == after
        && binary_before.is_some()
        && binary_before == binary_after
        && (fixture_set.data_path().is_none()
            || (fixture_before.is_some() && fixture_before == fixture_after))
        && (fixture_set.view_state_path().is_none()
            || (view_before.is_some() && view_before == view_after));
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
        "fixture_data_sha256": fixture_before,
        "fixture_data_sha256_after": fixture_after,
        "view_state_sha256": view_before,
        "view_state_sha256_after": view_after,
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
            "fixture_set": fixture_set.label(),
            "fixtures": fixture_set.views(),
            "fixture_data": fixture_set.data_path(),
            "fixture_view_state": fixture_set.view_state_path(),
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
    fn versioned_scenarios_have_separate_names_and_a_hashable_authored_backup() {
        assert_eq!(FixtureSet::Legacy.views(), FIXTURES);
        assert_eq!(FixtureSet::NativeV2.views().len(), FIXTURES.len() + 1);
        for (versioned, legacy) in FixtureSet::NativeV2.views().iter().zip(FIXTURES) {
            assert_eq!(versioned.strip_prefix("native-v2-"), Some(legacy));
        }
        assert_eq!(FixtureSet::NativeV2.views().last(), Some(&"provider-v2"));
        assert_eq!(FixtureSet::Legacy.view_state_path(), None);
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let path = root.join(FixtureSet::NativeV2.data_path().unwrap());
        let bytes = fs::read(&path).unwrap();
        assert_eq!(
            hash_file(&path).unwrap(),
            format!("{:x}", Sha256::digest(&bytes))
        );
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["format"], "cueglint-backup");
        assert_eq!(value["version"], 1);
        let view: Value = serde_json::from_slice(
            &fs::read(root.join(FixtureSet::NativeV2.view_state_path().unwrap())).unwrap(),
        )
        .unwrap();
        assert_eq!(view["scenario"], "provider-v2");
        assert_eq!(view["api_key"], "");
        assert_eq!(view["network_enabled"], false);
    }
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
            let request = "00000000-0000-0000-0000-000000000123";
            let mut event = json!({"event":"native_screenshot_saved","request_id":request,"fixture":"markdown","path":png,"image":{"width":3,"height":2},"pixels_per_point":2.0,"zoom_factor":1.0,"adapter":null});
            let font_state = json!({"state":"ready","identity":{"source":"synthetic-fixture","font_set_id":"a".repeat(64)}});
            event["fonts"] = font_state.clone();
            let trace = [("native_screenshot_stage", "requested"), ("native_screenshot_transport", "copy_submitted"), ("native_screenshot_transport", "readback_ready")]
                .into_iter().map(|(kind, stage)| {
                    let mut value = json!({"event":kind,"request_id":request,"fixture":"markdown","viewport":"root","stage":stage});
                    if stage == "requested" { value["font_state"] = font_state.clone(); }
                    value.to_string()
                })
                .collect::<Vec<_>>().join("\n");
            let check = |event: &Value, trace: &str, fixture: &str| {
                receipt(
                    event.to_string().as_bytes(),
                    trace.as_bytes(),
                    fixture,
                    &png,
                )
            };
            assert!(check(&event, &trace, "markdown").is_ok());
            event["fonts"]["state"] = json!("waiting_application");
            assert!(check(&event, &trace, "markdown").is_err());
            event["fonts"] = font_state.clone();
            event["fonts"]["identity"]["font_set_id"] = json!("b".repeat(64));
            assert!(check(&event, &trace, "markdown").is_err());
            event["fonts"] = font_state;
            let skipped = format!(
                "{}\n{trace}",
                json!({"event":"native_screenshot_transport","request_id":request,"fixture":"markdown","viewport":"root","stage":"surface_timeout"})
            );
            assert!(check(&event, &skipped, "markdown").is_ok());
            assert!(
                check(
                    &event,
                    &trace.replace("copy_submitted", "surface_timeout"),
                    "markdown"
                )
                .is_err()
            );
            assert!(check(&event, "", "markdown").is_err());
            assert!(
                check(
                    &event,
                    &trace.replace(request, "00000000-0000-0000-0000-000000000456"),
                    "markdown"
                )
                .is_err()
            );
            assert!(check(&event, &trace.replace("root", "other"), "markdown").is_err());
            let failed = format!(
                "{trace}\n{}",
                json!({"event":"native_screenshot_transport","request_id":request,"fixture":"markdown","viewport":"root","stage":"map_failed"})
            );
            assert!(check(&event, &failed, "markdown").is_err());
            let foreign_failure = format!(
                "{trace}\n{}",
                json!({"event":"native_screenshot_transport","request_id":"00000000-0000-0000-0000-000000000456","fixture":"markdown","viewport":"root","stage":"queue_overflow"})
            );
            assert!(check(&event, &foreign_failure, "markdown").is_err());
            assert!(check(&event, &trace, "overlay").is_err());
            event["image"]["width"] = json!(4);
            assert!(check(&event, &trace, "markdown").is_err());
            event["image"]["width"] = json!(3);
            event["pixels_per_point"] = json!(0);
            assert!(check(&event, &trace, "markdown").is_err());
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
