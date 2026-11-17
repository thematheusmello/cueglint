use anyhow::{Context, Result, bail, ensure};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
mod offline;

#[derive(Parser)]
#[command(about = "Cueglint reproducible development and evidence tasks")]
struct Args {
    #[command(subcommand)]
    command: Task,
}

#[derive(Subcommand)]
enum Task {
    /// Generate the honest parity report and static public dashboard.
    Progress {
        #[arg(long)]
        check: bool,
    },
    /// Validate registry identities, states, dependencies and evidence claims.
    VerifyRegistry,
    /// Compare matched PNGs without silently resizing or masking panels.
    Visual {
        reference: PathBuf,
        actual: PathBuf,
        #[arg(long)]
        report: PathBuf,
    },
    /// Run real core transcript/cancellation tests; this is not a live ASR benchmark.
    AudioFixtures,
    /// Run installed local model checks without downloading; preserve failures.
    InferenceBench {
        #[arg(long)]
        models_dir: PathBuf,
        #[arg(long)]
        report: PathBuf,
    },
    /// Exercise file ASR, local answers and portable data with macOS process networking denied.
    OfflineCliCheck {
        #[arg(long)]
        models_dir: PathBuf,
        #[arg(long)]
        report: PathBuf,
    },
    /// Build a macOS app bundle and CLI. Does not imply notarization or clean-install verification.
    Package,
}

#[derive(Deserialize)]
struct Registry {
    schema_version: u32,
    baseline: String,
    features: Vec<Feature>,
}

#[derive(Deserialize)]
struct Feature {
    id: String,
    category: String,
    scope: String,
    reference_build: String,
    contract: String,
    visual_contract: Option<String>,
    observation: State,
    implementation: State,
    verification: Verification,
    evidence_ids: Vec<String>,
    sources: Vec<String>,
    fixtures: Vec<String>,
    scenarios: Vec<String>,
    acceptance: Vec<String>,
    tolerances: serde_json::Value,
    dependencies: Vec<String>,
    locations: Vec<String>,
    issue: Option<String>,
    pull_requests: Vec<String>,
    gaps: Vec<String>,
    blockers: Vec<String>,
    freshness: String,
}

#[derive(Deserialize)]
struct State {
    state: String,
}

#[derive(Deserialize)]
struct Verification {
    state: String,
    run_id: Option<String>,
    source_commit: Option<String>,
    source_hash: Option<String>,
    model_hash: Option<String>,
    configuration_hash: Option<String>,
    fixture_hash: Option<String>,
    platform: Option<String>,
    time: Option<String>,
    artifacts: Vec<String>,
}

fn main() -> Result<()> {
    std::env::set_current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap())?;
    match Args::parse().command {
        Task::Progress { check } => progress(check),
        Task::VerifyRegistry => {
            registry()?;
            println!("Registry is structurally valid; this does not establish parity.");
            Ok(())
        }
        Task::Visual {
            reference,
            actual,
            report,
        } => visual(&reference, &actual, &report),
        Task::AudioFixtures => {
            run(Command::new("cargo").args(["test", "-p", "cueglint-core", "-p", "cueglint-local"]))
        }
        Task::InferenceBench { models_dir, report } => inference_bench(&models_dir, &report),
        Task::OfflineCliCheck { models_dir, report } => offline::check(&models_dir, &report),
        Task::Package => package(),
    }
}

fn registry() -> Result<Registry> {
    // JSON is a strict YAML 1.2 subset; no second parser or hand-edited derived counters.
    let registry: Registry = serde_json::from_slice(&fs::read("parity/features.yaml")?)?;
    ensure!(registry.schema_version == 1, "unsupported registry schema");
    let ids: BTreeSet<_> = registry.features.iter().map(|f| &f.id).collect();
    ensure!(ids.len() == registry.features.len(), "duplicate feature ID");
    let current_hash = source_hash()?;
    for f in &registry.features {
        ensure!(
            !f.id.is_empty() && !f.contract.is_empty() && !f.reference_build.is_empty(),
            "incomplete contract {}",
            f.id
        );
        ensure!(
            ["desktop", "mobile", "browser", "sharing", "sync"].contains(&f.scope.as_str()),
            "bad scope {}",
            f.id
        );
        ensure!(
            ["unobserved", "documented", "reference-executed", "blocked"]
                .contains(&f.observation.state.as_str()),
            "bad observation {}",
            f.id
        );
        ensure!(
            ["planned", "in progress", "partial", "implemented"]
                .contains(&f.implementation.state.as_str()),
            "bad implementation {}",
            f.id
        );
        ensure!(
            ["not run", "passed", "failed", "blocked", "stale"]
                .contains(&f.verification.state.as_str()),
            "bad verification {}",
            f.id
        );
        ensure!(
            !f.acceptance.is_empty() && !f.scenarios.is_empty(),
            "missing acceptance/scenario {}",
            f.id
        );
        ensure!(
            !f.tolerances.is_null(),
            "missing predefined tolerances {}",
            f.id
        );
        for dep in &f.dependencies {
            ensure!(ids.contains(dep), "{} missing dependency {}", f.id, dep);
        }
        for fixture in &f.fixtures {
            ensure!(
                Path::new(fixture).is_file(),
                "{} missing fixture {}",
                f.id,
                fixture
            );
        }
        if f.observation.state == "reference-executed" {
            ensure!(
                !f.evidence_ids.is_empty(),
                "reference claim without evidence {}",
                f.id
            );
        }
        if f.verification.state == "passed" {
            let v = &f.verification;
            ensure!(
                f.observation.state == "reference-executed",
                "parity pass lacks executed reference {}",
                f.id
            );
            ensure!(
                f.implementation.state == "implemented",
                "pass lacks implementation {}",
                f.id
            );
            ensure!(
                v.source_hash.as_deref() == Some(current_hash.as_str()),
                "stale source evidence {}",
                f.id
            );
            for field in [
                &v.run_id,
                &v.source_commit,
                &v.model_hash,
                &v.configuration_hash,
                &v.fixture_hash,
                &v.platform,
                &v.time,
            ] {
                ensure!(
                    field.as_ref().is_some_and(|s| !s.is_empty()),
                    "incomplete run identity {}",
                    f.id
                );
            }
            ensure!(!v.artifacts.is_empty(), "missing report artifacts {}", f.id);
            for artifact in &v.artifacts {
                ensure!(Path::new(artifact).is_file(), "missing report {}", artifact);
            }
        }
        let _ = (
            &f.visual_contract,
            &f.sources,
            &f.locations,
            &f.issue,
            &f.pull_requests,
            &f.freshness,
        );
    }
    Ok(registry)
}

fn source_hash() -> Result<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("workspace root")?;
    fn collect(path: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
        if !path.exists() {
            return Ok(());
        }
        if path.is_dir() {
            for e in fs::read_dir(path)? {
                collect(&e?.path(), out)?;
            }
        } else {
            out.push(path.to_owned());
        }
        Ok(())
    }
    let mut files = vec![root.join("Cargo.toml")];
    if root.join("Cargo.lock").exists() {
        files.push(root.join("Cargo.lock"));
    }
    for p in [
        "crates",
        "xtask/src",
        "ui/design-tokens.json",
        "ui/reference-manifest.json",
        "parity/reference.json",
        "parity/scenarios",
        "parity/fixtures",
        "spec/features",
        "models",
        "vendor/eframe",
        ".cargo",
        "rust-toolchain.toml",
    ] {
        collect(&root.join(p), &mut files)?;
    }
    files.sort();
    let mut hash = Sha256::new();
    for p in files {
        hash.update(
            p.strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/")
                .as_bytes(),
        );
        hash.update([0]);
        hash.update(fs::read(p)?);
        hash.update([0]);
    }
    hash.update(b"parity/contracts\0");
    let registry: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("parity/features.yaml"))?)?;
    hash.update(serde_json::to_vec(&contract_identity(&registry)?)?);
    Ok(format!("{:x}", hash.finalize()))
}

fn contract_identity(registry: &serde_json::Value) -> Result<Vec<serde_json::Value>> {
    registry["features"]
        .as_array()
        .context("missing feature contracts")?
        .iter()
        .map(|feature| {
            let mut fields = serde_json::Map::new();
            for key in [
                "id",
                "category",
                "scope",
                "reference_build",
                "contract",
                "visual_contract",
                "acceptance",
                "tolerances",
                "dependencies",
                "fixtures",
                "scenarios",
                "sources",
                "evidence_ids",
            ] {
                fields.insert(
                    key.to_owned(),
                    feature.get(key).cloned().unwrap_or(serde_json::Value::Null),
                );
            }
            Ok(serde_json::Value::Object(fields))
        })
        .collect()
}

fn progress(check: bool) -> Result<()> {
    let r = registry()?;
    let n = r.features.len();
    let count =
        |predicate: fn(&Feature) -> bool| r.features.iter().filter(|f| predicate(f)).count();
    let observed = count(|f| f.observation.state == "reference-executed");
    let discovered = count(|f| {
        matches!(
            f.observation.state.as_str(),
            "documented" | "reference-executed"
        )
    });
    let implemented = count(|f| f.implementation.state == "implemented");
    let passed = count(|f| f.verification.state == "passed");
    let visual_total = count(|f| f.visual_contract.is_some());
    let visual_passed = count(|f| f.visual_contract.is_some() && f.verification.state == "passed");
    let ai_total = count(|f| f.category == "ai" || f.category == "audio");
    let ai_passed = count(|f| {
        (f.category == "ai" || f.category == "audio") && f.verification.state == "passed"
    });
    let mut md = format!(
        "# Cueglint progress\n\nGenerated by `cargo xtask progress` from `parity/features.yaml`. No whole-product parity claim.\n\nBaseline: **{}**. Hosted service versions remain unknown unless recorded.\n\n| Measure | Count / denominator |\n|---|---:|\n| Discovered contracts (documented or reference-executed) | {discovered} / {n} inventoried |\n| Contracts with reference execution (may be partial) | {observed} / {n} inventoried |\n| Implemented contracts | {implemented} / {n} inventoried |\n| Verified matched workflows/contracts | {passed} / {n} inventoried |\n| Passed visual contracts | {visual_passed} / {visual_total} visual contracts |\n| Passed AI/audio contracts | {ai_passed} / {ai_total} AI/audio contracts |\n| Clean-install verified release platforms | 0 / 3 (macOS, Windows, Linux) |\n\nCompilation and implementation-only unit tests are reported separately and never count as matched parity. Desktop is the first release gate; mobile, browser, sharing and sync remain inventoried.\n\n| Feature | Scope | Observation | Implementation | Verification | Remaining gaps |\n|---|---|---|---|---|---|\n",
        r.baseline
    );
    for f in &r.features {
        md.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            f.id,
            f.scope,
            f.observation.state,
            f.implementation.state,
            f.verification.state,
            f.gaps
                .iter()
                .chain(&f.blockers)
                .map(|s| s.replace('|', "/").replace('\n', " "))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    md.push_str("\nRecorded test reports (historical runs; inspect source/model identities for freshness). These do not promote a feature to matched parity.\n\n| Report | Recorded status | Source |\n|---|---|---|\n");
    let reports = report_rows(Path::new("parity/reports"))?;
    for (name, status, source) in &reports {
        md.push_str(&format!(
            "| [{name}](../parity/reports/{name}) | {} | {} |\n",
            status.replace('|', "/"),
            source.replace('|', "/")
        ));
    }
    md.push_str("\nSee [verification](verification.md), [platform matrix](platform-support.md), [roadmap](../ROADMAP.md), and [continuation record](../STATUS.md). Failures and stale evidence remain visible.\n");
    let mut html = String::from(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>Cueglint progress</title><style>body{font:16px system-ui;background:#f8f9fb;color:#1b2533;max-width:1400px;margin:40px auto;padding:0 24px}h1{letter-spacing:-.04em}table{border-collapse:collapse;width:100%;font-size:13px}td,th{padding:10px;text-align:left;border-bottom:1px solid #dde1e8}th{background:#eef1f6}a{color:#2366bb}.counts{display:flex;gap:16px;flex-wrap:wrap}.card{padding:20px;background:white;border:1px solid #dde1e8;border-radius:14px}small{color:#626b7a}</style><h1>Cueglint</h1><p>Your context. Your assistant. Your machine.</p><p>Evidence-backed progress. Desktop parity is not yet established.</p>",
    );
    html.push_str(&format!(
        "<p>Baseline: {}</p><div class=\"counts\">",
        escape(&r.baseline)
    ));
    for (label, value, denominator) in [
        ("Discovered", discovered, n),
        ("Observed", observed, n),
        ("Implemented", implemented, n),
        ("Verified", passed, n),
        ("Visual", visual_passed, visual_total),
        ("AI / audio", ai_passed, ai_total),
    ] {
        html.push_str(&format!(
            "<div class=\"card\"><strong>{value} / {denominator}</strong><br>{label}</div>"
        ));
    }
    html.push_str("</div><p><small>Counts describe inventoried contracts, not a completion percentage. Zero release platforms verified out of three. Implementation-only tests do not establish reference parity.</small></p><table><tr><th>Contract<th>Scope<th>Observed<th>Implemented<th>Verified<th>Gaps</tr>");
    for f in &r.features {
        html.push_str(&format!(
            "<tr><td>{}<td>{}<td>{}<td>{}<td>{}<td>{}</tr>",
            escape(&f.id),
            escape(&f.scope),
            escape(&f.observation.state),
            escape(&f.implementation.state),
            escape(&f.verification.state),
            escape(
                &f.gaps
                    .iter()
                    .chain(&f.blockers)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        ));
    }
    html.push_str("</table><p>Recorded historical test reports; source and model identities determine freshness. No report below establishes whole-product parity.</p><table><tr><th>Report<th>Recorded status<th>Source</tr>");
    for (name, status, source) in &reports {
        html.push_str(&format!("<tr><td><a href=\"https://github.com/thematheusmello/cueglint/blob/main/parity/reports/{}\">{}</a><td>{}<td>{}</tr>",escape(name),escape(name),escape(status),escape(source)));
    }
    html.push_str("</table><p><a href=\"https://github.com/thematheusmello/cueglint\">Source, roadmap and evidence</a></p></html>\n");
    for (path, contents) in [
        ("docs/progress.md", md),
        ("docs/dashboard/index.html", html),
    ] {
        if check {
            ensure!(
                fs::read_to_string(path).ok().as_deref() == Some(&contents),
                "{path} is stale; run cargo xtask progress"
            );
        } else {
            fs::create_dir_all(Path::new(path).parent().unwrap())?;
            fs::write(path, contents)?;
        }
    }
    println!(
        "Progress {}: {observed}/{n} observed, {implemented}/{n} implemented, {passed}/{n} verified",
        if check { "consistent" } else { "generated" }
    );
    Ok(())
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn report_rows(directory: &Path) -> Result<Vec<(String, String, String)>> {
    let mut paths = fs::read_dir(directory)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.retain(|p| p.extension().is_some_and(|e| e == "json"));
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .context("report name is not UTF-8")?;
            ensure!(
                name.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)),
                "unsafe report name: {name}"
            );
            let report: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)
                .with_context(|| format!("invalid report {}", path.display()))?;
            ensure!(report.is_object(), "report {name} must be a JSON object");
            let status = report["status"]
                .as_str()
                .unwrap_or(match report["passed"].as_bool() {
                    Some(true) => "passed",
                    Some(false) => "failed",
                    None => "see individual checks",
                })
                .replace(['\r', '\n'], " ")
                .replace('_', " ");
            let source = report["source_commit"]
                .as_str()
                .unwrap_or("fingerprints in report")
                .replace(['\r', '\n'], " ");
            Ok((name.to_owned(), status, source))
        })
        .collect()
}

#[derive(Serialize)]
struct VisualReport {
    schema_version: u32,
    evidence_type: &'static str,
    reference_sha256: String,
    actual_sha256: String,
    source_hash: String,
    width: u32,
    height: u32,
    mean_absolute_channel_error: f64,
    changed_pixel_fraction: f64,
    tolerance_mean_error: f64,
    tolerance_changed_fraction: f64,
    passed: bool,
    masks: Vec<String>,
    limitations: &'static str,
}

fn visual(reference: &Path, actual: &Path, report: &Path) -> Result<()> {
    let a = image::open(reference)?.to_rgba8();
    let b = image::open(actual)?.to_rgba8();
    ensure!(
        a.dimensions() == b.dimensions(),
        "viewport mismatch {:?} vs {:?}; no automatic resizing",
        a.dimensions(),
        b.dimensions()
    );
    ensure!(a.width() > 0 && a.height() > 0, "empty screenshot");
    let mut total = 0u64;
    let mut changed = 0u64;
    for (left, right) in a.pixels().zip(b.pixels()) {
        let delta = left
            .0
            .iter()
            .zip(right.0)
            .map(|(x, y)| x.abs_diff(y) as u64)
            .collect::<Vec<_>>();
        total += delta.iter().sum::<u64>();
        if delta.iter().any(|d| *d > 16) {
            changed += 1;
        }
    }
    let pixels = a.width() as f64 * a.height() as f64;
    let mean = total as f64 / (pixels * 4.0);
    let fraction = changed as f64 / pixels;
    // These thresholds are declared before comparison; changing them invalidates prior runs.
    let result = VisualReport {
        schema_version: 1,
        evidence_type: "image-comparison-only",
        reference_sha256: hash_file(reference)?,
        actual_sha256: hash_file(actual)?,
        source_hash: source_hash()?,
        width: a.width(),
        height: a.height(),
        mean_absolute_channel_error: mean,
        changed_pixel_fraction: fraction,
        tolerance_mean_error: 3.0,
        tolerance_changed_fraction: 0.02,
        passed: mean <= 3.0 && fraction <= 0.02,
        masks: vec![],
        limitations: "No masks or rescaling. Pixel similarity does not establish interaction, motion, typography metrics, or model quality. Reference authenticity must be checked separately.",
    };
    if let Some(parent) = report.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::write(report, serde_json::to_vec_pretty(&result)?)?;
    println!(
        "Visual comparison: mean {mean:.3}, changed {fraction:.4}; passed={}",
        result.passed
    );
    ensure!(
        result.passed,
        "visual tolerances exceeded; failure report preserved"
    );
    Ok(())
}

fn hash_file(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}

fn run(command: &mut Command) -> Result<()> {
    let status = command.status().context("starting command")?;
    ensure!(status.success(), "command failed: {status}");
    Ok(())
}

fn package() -> Result<()> {
    if !cfg!(target_os = "macos") {
        bail!("Packaging is currently implemented for macOS only; Windows/Linux remain open");
    }
    run(Command::new("cargo").args([
        "build",
        "--release",
        "-p",
        "cueglint",
        "-p",
        "cueglint-cli",
    ]))?;
    let bundle = Path::new("dist/Cueglint.app/Contents");
    fs::create_dir_all(bundle.join("MacOS"))?;
    fs::copy("target/release/cueglint", bundle.join("MacOS/cueglint"))?;
    fs::create_dir_all("dist")?;
    fs::copy("target/release/cueglint-cli", "dist/cueglint-cli")?;
    fs::write(
        bundle.join("Info.plist"),
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\"><plist version=\"1.0\"><dict><key>CFBundleExecutable</key><string>cueglint</string><key>CFBundleIdentifier</key><string>org.cueglint.desktop</string><key>CFBundleName</key><string>Cueglint</string><key>CFBundleDisplayName</key><string>Cueglint</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleShortVersionString</key><string>0.1.0</string><key>CFBundleVersion</key><string>1</string><key>NSMicrophoneUsageDescription</key><string>Cueglint transcribes microphone audio locally when you start a session.</string><key>NSScreenCaptureUsageDescription</key><string>Cueglint uses the selected screen as local assistant context.</string><key>NSHighResolutionCapable</key><true/></dict></plist>",
    )?;
    run(Command::new("codesign").args(["--force", "--deep", "--sign", "-", "dist/Cueglint.app"]))?;
    fs::write(
        "dist/SHA256SUMS",
        format!(
            "{}  Cueglint.app/Contents/MacOS/cueglint\n{}  cueglint-cli\n",
            hash_file(&bundle.join("MacOS/cueglint"))?,
            hash_file(Path::new("dist/cueglint-cli"))?
        ),
    )?;
    println!("Created dist/Cueglint.app and dist/cueglint-cli (ad-hoc signature; not notarized)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn html_escaping_does_not_render_context_as_markup() {
        assert_eq!(escape("<script>\"&"), "&lt;script&gt;&quot;&amp;");
    }
    #[test]
    fn acceptance_changes_invalidate_evidence_without_a_verification_hash_cycle() {
        let mut registry = serde_json::json!({"features":[{"id":"original","tolerances":{"pixels":2},"verification":{"state":"not run"}}]});
        let original = contract_identity(&registry).unwrap();
        registry["features"][0]["verification"] =
            serde_json::json!({"state":"passed","source_hash":"recorded later"});
        registry["features"][0]["pull_requests"] = serde_json::json!(["review link"]);
        assert_eq!(original, contract_identity(&registry).unwrap());
        registry["features"][0]["tolerances"]["pixels"] = 3.into();
        assert_ne!(original, contract_identity(&registry).unwrap());
    }
    #[test]
    fn report_index_preserves_failures_and_rejects_invalid_input() {
        let dir =
            std::env::temp_dir().join(format!("cueglint-report-index-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("failed-run.json");
        fs::write(&path, r#"{"passed":false,"source_commit":"original"}"#).unwrap();
        assert_eq!(
            report_rows(&dir).unwrap(),
            vec![("failed-run.json".into(), "failed".into(), "original".into())]
        );
        fs::write(&path, "incomplete JSON").unwrap();
        assert!(report_rows(&dir).is_err());
        fs::remove_file(path).unwrap();
        fs::write(dir.join("[unsafe].json"), "{}").unwrap();
        assert!(report_rows(&dir).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn visual_comparison_preserves_failure() {
        let dir = std::env::temp_dir().join(format!("cueglint-visual-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.png");
        let b = dir.join("b.png");
        let report = dir.join("report.json");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 0, 255]))
            .save(&a)
            .unwrap();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 255, 255, 255]))
            .save(&b)
            .unwrap();
        assert!(visual(&a, &b, &report).is_err());
        assert!(report.exists());
        fs::remove_dir_all(dir).unwrap();
    }
}

fn inference_bench(models_dir: &Path, report: &Path) -> Result<()> {
    let output = Command::new("cargo")
        .args([
            "test",
            "-p",
            "cueglint-local",
            "--test",
            "generation_smoke",
            "--test",
            "asr_smoke",
            "--",
            "--ignored",
            "--nocapture",
        ])
        .env("CUEGLINT_TEST_MODELS_DIR", models_dir)
        .output()
        .context("running installed local models")?;
    let result = serde_json::json!({"schema_version":1,"evidence_type":"implementation-only-real-inference","passed":output.status.success(),"source_hash":source_hash()?,"model_manifest_sha256":hash_file(Path::new("models/local-defaults.json"))?,"platform":std::env::consts::OS,"architecture":std::env::consts::ARCH,"time_unix_ms":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis(),"stdout":String::from_utf8_lossy(&output.stdout),"stderr":String::from_utf8_lossy(&output.stderr),"limitations":"Development smoke cases only, not reference parity, live dual-source audio, blocked-network traffic evidence, a quality distribution or a sustained-session benchmark."});
    if let Some(parent) = report.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::write(report, serde_json::to_vec_pretty(&result)?)?;
    ensure!(
        output.status.success(),
        "local inference checks failed; report preserved at {}",
        report.display()
    );
    println!("Real local model smoke checks passed; {}", report.display());
    Ok(())
}
