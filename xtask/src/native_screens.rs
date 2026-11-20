//! Actual executable fixture capture. This does not compare against the commercial app.
use super::{cargo_executable, fingerprint, hash_file, prepare_report, source_hash, write_report};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
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
const CALENDAR_REMINDER: &str = "native-v2-calendar-reminder";
const CALENDAR_FIXTURES: [&str; 3] = [
    "native-v2-calendar-settings",
    "native-v2-calendar-history",
    CALENDAR_REMINDER,
];
const LINUX_FONT: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf";

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum FixtureSet {
    Legacy,
    NativeV2,
    CalendarV2,
    TranscriptV1,
    LinuxDashboard,
}

impl FixtureSet {
    fn label(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::NativeV2 => "native-v2",
            Self::CalendarV2 => "calendar-v2",
            Self::TranscriptV1 => "transcript-v1",
            Self::LinuxDashboard => "linux-dashboard",
        }
    }
    fn views(self) -> &'static [&'static str] {
        match self {
            Self::Legacy => &FIXTURES,
            Self::NativeV2 => &V2_FIXTURES,
            Self::CalendarV2 => &CALENDAR_FIXTURES,
            Self::TranscriptV1 => &["transcript-v1", "transcript-empty-v1"],
            Self::LinuxDashboard => &["native-v2-dashboard"],
        }
    }
    fn data_path(self) -> Option<&'static str> {
        match self {
            Self::Legacy => None,
            Self::TranscriptV1 => Some("ui/fixtures/transcript-v1.json"),
            Self::NativeV2 | Self::CalendarV2 | Self::LinuxDashboard => {
                Some("ui/fixtures/native-v2.json")
            }
        }
    }
    fn view_state_path(self) -> Option<&'static str> {
        match self {
            Self::Legacy | Self::TranscriptV1 | Self::LinuxDashboard => None,
            Self::NativeV2 => Some("ui/fixtures/provider-v2.json"),
            Self::CalendarV2 => Some("crates/cueglint/src/calendar_fixture.rs"),
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

fn receipt(
    stdout: &[u8],
    stderr: &[u8],
    fixture: &str,
    png: &Path,
    linux_font_sha256: Option<&str>,
) -> Result<Value> {
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
    if let Some(font_sha256) = linux_font_sha256 {
        verify_linux_dashboard(&event, &decoded, font_sha256)?;
    }
    if matches!(
        fixture,
        CALENDAR_REMINDER | "transcript-v1" | "transcript-empty-v1"
    ) {
        ensure!(
            trace.iter().any(|line| line["request_id"] == request_id
                && line["event"] == "native_screenshot_stage"
                && line["stage"] == "requested"
                && line["content_ready"] == true),
            "{fixture} content was not ready when the native screenshot was requested"
        );
    }
    if matches!(fixture, "transcript-v1" | "transcript-empty-v1") {
        let scale = event["pixels_per_point"]
            .as_f64()
            .context("transcript scale")?;
        ensure!(
            (f64::from(decoded.width()) / scale - 690.0).abs() <= 2.0
                && (f64::from(decoded.height()) / scale - 760.0).abs() <= 2.0,
            "transcript requires the 690×760 logical meeting detail viewport"
        );
        if fixture == "transcript-empty-v1" {
            event["message_ink_center"] = json!(verify_empty_transcript(&decoded, scale)?);
        }
    }
    if fixture == CALENDAR_REMINDER {
        verify_calendar_reminder(&event, &decoded)?;
    }
    event["verified_png_sha256"] = json!(format!("{:x}", Sha256::digest(&bytes)));
    Ok(event)
}

fn verify_empty_transcript(image: &image::DynamicImage, scale: f64) -> Result<[f64; 2]> {
    let rgba = image
        .as_rgba8()
        .context("empty transcript requires RGBA8")?;
    let (mut left, mut top, mut right, mut bottom) = (rgba.width(), rgba.height(), 0, 0);
    // The blank message is the only ink in the recorded transcript body.
    let edge = |point: f64| (point * scale).round() as u32;
    for y in edge(220.0)..edge(712.0).min(rgba.height()) {
        for x in edge(12.0)..edge(678.0).min(rgba.width()) {
            let [r, g, b, a] = rgba[(x, y)].0.map(u32::from);
            let luma = (19595 * r + 38470 * g + 7471 * b + 32768) >> 16;
            if a > 0 && luma > 60 {
                left = left.min(x);
                top = top.min(y);
                right = right.max(x + 1);
                bottom = bottom.max(y + 1);
            }
        }
    }
    ensure!(
        right > left && bottom > top,
        "missing empty transcript message"
    );
    let center = [f64::from(left + right), f64::from(top + bottom)].map(|p| p / (2.0 * scale));
    ensure!(
        (center[0] - 344.5).abs() <= 2.0 && (center[1] - 407.5).abs() <= 2.0,
        "empty transcript ink center {center:?} outside the two-unit tolerance"
    );
    Ok(center)
}

fn verify_calendar_reminder(event: &Value, image: &image::DynamicImage) -> Result<()> {
    let scale = event["pixels_per_point"]
        .as_f64()
        .context("reminder scale")?;
    ensure!(
        (f64::from(image.width()) / scale - 690.0).abs() <= 2.0
            && (f64::from(image.height()) / scale - 164.0).abs() <= 2.0,
        "reminder requires the expanded 690×164 logical primary viewport"
    );
    let rgba = image
        .as_rgba8()
        .context("reminder screenshot requires RGBA8")?;
    let mut banner = rgba.enumerate_pixels().filter_map(|(x, y, pixel)| {
        let y = f64::from(y) / scale;
        ((116.0..150.0).contains(&y)
            && (16.0..674.0).contains(&(f64::from(x) / scale))
            && pixel[3] > 0)
            .then_some(pixel)
    });
    let first = banner.next().context("reminder banner is transparent")?;
    ensure!(
        banner.any(|pixel| pixel.0[..3] != first.0[..3]),
        "reminder banner contains no visible text or controls"
    );
    Ok(())
}

fn verify_linux_dashboard(
    event: &Value,
    image: &image::DynamicImage,
    font_sha256: &str,
) -> Result<()> {
    let adapter = &event["adapter"];
    ensure!(
        adapter["backend"] == "Vulkan"
            && adapter["device_type"] == "Cpu"
            && adapter["name"]
                .as_str()
                .is_some_and(|name| name.to_ascii_lowercase().contains("llvmpipe")),
        "Linux dashboard did not use the Lavapipe CPU Vulkan adapter"
    );
    ensure!(
        (image.width(), image.height()) == (860, 670)
            && event["pixels_per_point"].as_f64() == Some(1.0)
            && event["zoom_factor"].as_f64() == Some(1.0),
        "Linux dashboard requires an 860x670 viewport at scale and zoom 1"
    );
    let rgba = image.as_rgba8().context("Linux dashboard requires RGBA8")?;
    let mut visible = rgba.pixels().filter(|pixel| pixel[3] > 0);
    let first = visible.next().context("Linux dashboard is blank")?;
    ensure!(
        visible.any(|pixel| pixel.0[..3] != first.0[..3]),
        "Linux dashboard is blank"
    );
    let font = &event["fonts"]["identity"];
    let mut faces = font["faces"]
        .as_array()
        .context("Linux dashboard has no font faces")?
        .iter()
        .filter(|face| face["name"] == "platform-sans");
    ensure!(
        font["source"] == "legacy-platform-file"
            && font["file_name"] == "DejaVuSans.ttf"
            && faces
                .next()
                .is_some_and(|face| face["sha256"] == font_sha256)
            && faces.next().is_none(),
        "Linux dashboard did not use the identified DejaVu Sans"
    );
    Ok(())
}

#[derive(PartialEq, serde::Serialize)]
struct LinuxInputs {
    lock_sha256: String,
    platform_font_sha256: String,
    vulkan_icd: PathBuf,
    vulkan_icd_sha256: String,
}

impl LinuxInputs {
    fn read() -> Result<Self> {
        let vulkan_icd = PathBuf::from(
            std::env::var_os("VK_DRIVER_FILES")
                .context("set one Lavapipe ICD in VK_DRIVER_FILES")?,
        );
        ensure!(
            vulkan_icd.is_absolute() && fs::symlink_metadata(&vulkan_icd)?.is_file(),
            "expected one absolute, regular Lavapipe ICD file"
        );
        let icd = bounded_read(&vulkan_icd, 64 * 1024).context("read Lavapipe ICD")?;
        ensure!(!icd.is_empty(), "Lavapipe ICD is empty");
        Ok(Self {
            lock_sha256: hash_file(Path::new("Cargo.lock")).context("hash Cargo.lock")?,
            platform_font_sha256: hash_file(Path::new(LINUX_FONT)).context("hash DejaVu Sans")?,
            vulkan_icd_sha256: format!("{:x}", Sha256::digest(icd)),
            vulkan_icd,
        })
    }
}

fn verify_linux_executable(executable: &Path) -> Result<()> {
    let target = PathBuf::from(
        std::env::var_os("CARGO_TARGET_DIR").context("set an isolated CARGO_TARGET_DIR")?,
    );
    ensure!(
        fs::symlink_metadata(executable)?.is_file()
            && executable.canonicalize()? == target.canonicalize()?.join("debug/cueglint"),
        "Cargo did not report the Cueglint executable from the isolated target directory"
    );
    let mut header = [0; 20];
    File::open(executable)?.read_exact(&mut header)?;
    ensure!(
        &header[..7] == b"\x7fELF\x02\x01\x01" && header[18..20] == [0x3e, 0],
        "Cargo did not report an x86-64 ELF executable"
    );
    Ok(())
}

pub fn check(output_dir: &Path, report: &Path, fixture_set: FixtureSet) -> Result<()> {
    let linux_dashboard = matches!(fixture_set, FixtureSet::LinuxDashboard);
    if linux_dashboard {
        ensure!(
            cfg!(all(target_os = "linux", target_arch = "x86_64")),
            "the linux-dashboard fixture set requires x86-64 Linux"
        );
    } else {
        ensure!(
            cfg!(target_os = "macos"),
            "the {} fixture set requires macOS",
            fixture_set.label()
        );
    }
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
    let mut linux_before = None;
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
        if linux_dashboard {
            linux_before = Some(LinuxInputs::read()?);
        }
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
        if linux_dashboard {
            verify_linux_executable(&executable)?;
        }
        binary_before = Some(hash_file(&executable)?);
        binary = Some(executable.clone());
        for &fixture in fixture_set.views() {
            let png = output_dir.join(format!("{fixture}.png"));
            let stdout_path = output_dir.join(format!("{fixture}.stdout"));
            let stderr_path = output_dir.join(format!("{fixture}.stderr"));
            let started = Instant::now();
            let view_hash =
                if fixture == "provider-v2" || matches!(fixture_set, FixtureSet::CalendarV2) {
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
                let event = receipt(
                    &stdout,
                    &stderr,
                    fixture,
                    &png,
                    linux_before
                        .as_ref()
                        .map(|inputs| inputs.platform_font_sha256.as_str()),
                );
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
    let linux_after = if linux_before.is_some() {
        match LinuxInputs::read() {
            Ok(inputs) => Some(inputs),
            Err(error) => {
                errors.push(format!("final Linux inputs: {error:#}"));
                None
            }
        }
    } else {
        None
    };
    let identities_match = before.is_some()
        && before == after
        && binary_before.is_some()
        && binary_before == binary_after
        && (fixture_set.data_path().is_none()
            || (fixture_before.is_some() && fixture_before == fixture_after))
        && (fixture_set.view_state_path().is_none()
            || (view_before.is_some() && view_before == view_after))
        && (!linux_dashboard || (linux_before.is_some() && linux_before == linux_after));
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
    let mut value = json!({
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
    if linux_dashboard {
        value["linux_inputs"] = json!({"before": linux_before, "after": linux_after});
    }
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
    fn empty_message_center_distinguishes_the_measured_frame_and_the_failed_native_offset() {
        let image = |top: u32, bottom: u32| {
            image::DynamicImage::ImageRgba8(image::RgbaImage::from_fn(690, 760, |x, y| {
                let value = if (226..465).contains(&x) && (top..bottom).contains(&y) {
                    90
                } else {
                    12
                };
                image::Rgba([value, value, value, 255])
            }))
        };
        assert_eq!(
            verify_empty_transcript(&image(400, 415), 1.0).unwrap(),
            [345.5, 407.5]
        );
        assert!(verify_empty_transcript(&image(403, 417), 1.0).is_err());
        assert!(verify_empty_transcript(&image(0, 0), 1.0).is_err());
    }

    #[test]
    fn versioned_scenarios_have_separate_names_and_a_hashable_authored_backup() {
        assert_eq!(FixtureSet::Legacy.views(), FIXTURES);
        assert_eq!(FixtureSet::NativeV2.views().len(), FIXTURES.len() + 1);
        for (versioned, legacy) in FixtureSet::NativeV2.views().iter().zip(FIXTURES) {
            assert_eq!(versioned.strip_prefix("native-v2-"), Some(legacy));
        }
        assert_eq!(FixtureSet::NativeV2.views().last(), Some(&"provider-v2"));
        assert_eq!(FixtureSet::Legacy.view_state_path(), None);
        assert!(FixtureSet::CalendarV2.views().iter().all(|fixture| {
            !FixtureSet::Legacy.views().contains(fixture)
                && !FixtureSet::NativeV2.views().contains(fixture)
        }));
        assert_eq!(
            FixtureSet::CalendarV2.views().last(),
            Some(&CALENDAR_REMINDER)
        );
        assert_eq!(
            FixtureSet::CalendarV2.data_path(),
            FixtureSet::NativeV2.data_path()
        );
        assert_eq!(FixtureSet::LinuxDashboard.views(), &V2_FIXTURES[..1]);
        assert_eq!(
            FixtureSet::LinuxDashboard.data_path(),
            FixtureSet::NativeV2.data_path()
        );
        assert_eq!(FixtureSet::LinuxDashboard.view_state_path(), None);
        let root = crate::workspace_root().unwrap();
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
    fn reminder_png_requires_size_and_distinct_visible_banner_pixels() {
        let event = json!({"pixels_per_point": 2.0});
        let mut image = image::DynamicImage::new_rgba8(1380, 328);
        assert!(verify_calendar_reminder(&event, &image).is_err());
        let rgba = image.as_mut_rgba8().unwrap();
        rgba.put_pixel(32, 232, image::Rgba([18, 18, 18, 255]));
        assert!(verify_calendar_reminder(&event, &image).is_err());
        image
            .as_mut_rgba8()
            .unwrap()
            .put_pixel(34, 234, image::Rgba([200, 200, 200, 255]));
        verify_calendar_reminder(&event, &image).unwrap();
        assert!(verify_calendar_reminder(&json!({"pixels_per_point": 1.0}), &image).is_err());
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
                    None,
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
    fn linux_dashboard_requires_visible_lavapipe_frame_and_its_dejavu_face() -> Result<()> {
        let font_sha256 = "a".repeat(64);
        let event = json!({
            "adapter": {"backend":"Vulkan", "device_type":"Cpu", "name":"llvmpipe"},
            "pixels_per_point": 1.0, "zoom_factor": 1.0,
            "fonts": {"identity":{
                "source":"legacy-platform-file", "file_name":"DejaVuSans.ttf",
                "faces":[{"name":"platform-sans", "sha256":font_sha256}]
            }}
        });
        let mut pixels = image::RgbaImage::from_pixel(860, 670, image::Rgba([28, 28, 28, 255]));
        let blank = image::DynamicImage::from(pixels.clone());
        assert!(verify_linux_dashboard(&event, &blank, &font_sha256).is_err());
        pixels.put_pixel(0, 0, image::Rgba([40, 30, 20, 0]));
        let still_blank = image::DynamicImage::from(pixels.clone());
        assert!(verify_linux_dashboard(&event, &still_blank, &font_sha256).is_err());
        pixels.put_pixel(0, 0, image::Rgba([40, 30, 20, 255]));
        let frame = image::DynamicImage::from(pixels);
        verify_linux_dashboard(&event, &frame, &font_sha256)?;
        assert!(
            verify_linux_dashboard(&event, &frame.crop_imm(0, 0, 20, 20), &font_sha256).is_err()
        );
        for (field, wrong) in [
            ("/adapter/backend", json!("Gl")),
            ("/adapter/device_type", json!("IntegratedGpu")),
            ("/adapter/name", json!("other CPU")),
            ("/pixels_per_point", json!(2.0)),
            ("/zoom_factor", json!(1.25)),
            ("/fonts/identity/source", json!("embedded-fallback")),
            ("/fonts/identity/faces/0/sha256", json!("b".repeat(64))),
        ] {
            let mut mismatched = event.clone();
            *mismatched.pointer_mut(field).unwrap() = wrong;
            assert!(
                verify_linux_dashboard(&mismatched, &frame, &font_sha256).is_err(),
                "accepted mismatched {field}"
            );
        }
        Ok(())
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
