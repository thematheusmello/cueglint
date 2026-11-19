use crate::{cargo_executable, notices};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsStr,
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) const TARGET: &str = "x86_64-pc-windows-msvc";
const BUILD_DIR: &str = "target/windows-package";
const PACKAGE: &str = "cueglint-windows-x86_64-msvc";
const NOTICE_ROOTS: [&str; 2] = ["cueglint.NOTICES", "cueglint-cli.NOTICES"];

pub(super) fn package(root: &Path, build: impl FnOnce(&Path) -> Result<Vec<u8>>) -> Result<()> {
    let bundle = notices::load_windows(root)?;
    bundle.require_ready(TARGET)?;
    available_destination(root)?;
    for name in ["cueglint", "cueglint-cli"] {
        executable_path(root, name)?;
    }
    let artifacts = build(&root.join(BUILD_DIR))?;
    bundle.revalidate(root)?;
    let gui = open_artifact(root, &artifacts, "cueglint")?;
    let cli = open_artifact(root, &artifacts, "cueglint-cli")?;
    let destination = available_destination(root)?;
    let dist = destination.parent().context("Windows package parent")?;
    fs::create_dir_all(dist)?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let stage = dist.join(format!(".{PACKAGE}-{}-{stamp}.tmp", std::process::id()));
    fs::create_dir(&stage).context("create Windows package staging directory")?;
    let result = (|| {
        let mut sums = copy_artifact(gui, &stage, "cueglint.exe")?;
        sums.push_str(&copy_artifact(cli, &stage, "cueglint-cli.exe")?);
        for notice_root in NOTICE_ROOTS {
            bundle.install(&stage, notice_root, TARGET)?;
        }
        for notice_root in NOTICE_ROOTS {
            sums.push_str(&bundle.checksums(&stage, notice_root)?);
        }
        File::create_new(stage.join("SHA256SUMS"))?.write_all(sums.as_bytes())?;
        bundle.revalidate(root)?;
        publish(&stage, &available_destination(root)?)
    })();
    if let Err(error) = result {
        fs::remove_dir_all(&stage)
            .with_context(|| format!("remove failed stage {} after {error:#}", stage.display()))?;
        return Err(error);
    }
    println!("Created {}", destination.display());
    Ok(())
}

fn available_destination(root: &Path) -> Result<PathBuf> {
    let dist = notices::output_path(root, "dist")?;
    for entry in fs::read_dir(root)? {
        let name = entry?.file_name();
        ensure!(
            !name.eq_ignore_ascii_case(OsStr::new("dist")) || name == "dist",
            "Windows package output parent has a case collision"
        );
    }
    match fs::read_dir(&dist) {
        Ok(entries) => {
            for entry in entries {
                ensure!(
                    !entry?.file_name().eq_ignore_ascii_case(OsStr::new(PACKAGE)),
                    "Windows package already exists at {}",
                    dist.join(PACKAGE).display()
                );
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("inspect Windows package output parent"),
    }
    Ok(dist.join(PACKAGE))
}

fn executable_path(root: &Path, name: &str) -> Result<PathBuf> {
    notices::output_path(root, &format!("{BUILD_DIR}/{TARGET}/release/{name}.exe"))
}

fn open_artifact(root: &Path, artifacts: &[u8], name: &str) -> Result<File> {
    let reported = cargo_executable(artifacts, name)?;
    let expected = executable_path(root, name)?;
    let unexpected = || {
        format!(
            "Cargo reported an unexpected path for {name}: {}",
            reported.display()
        )
    };
    ensure!(
        reported.is_absolute()
            && reported.ends_with(expected.strip_prefix(root)?)
            && reported.canonicalize().with_context(unexpected)?
                == expected.canonicalize().with_context(unexpected)?,
        "{}",
        unexpected()
    );
    let mut file = notices::open_regular(&expected)?;
    let mut dos = [0; 64];
    file.read_exact(&mut dos).context("read DOS header")?;
    let offset = u32::from_le_bytes(dos[60..64].try_into()?) as u64;
    ensure!(
        &dos[..2] == b"MZ" && offset >= 64 && offset + 26 <= file.metadata()?.len(),
        "invalid PE header offset for {name}"
    );
    file.seek(SeekFrom::Start(offset))?;
    let mut pe = [0; 26];
    file.read_exact(&mut pe)?;
    ensure!(
        &pe[..6] == b"PE\0\0\x64\x86" && pe[24..26] == [0x0b, 0x02],
        "expected AMD64 PE32+ for {name}"
    );
    file.rewind()?;
    Ok(file)
}

fn copy_artifact(mut source: File, stage: &Path, name: &str) -> Result<String> {
    let expected = checksum(&mut source)?;
    source.rewind()?;
    // Cargo can hard-link its outputs. Copy just the data into a new regular file.
    let mut file = File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(stage.join(name))?;
    io::copy(&mut source, &mut file)?;
    file.rewind()?;
    let actual = checksum(&mut file)?;
    ensure!(actual == expected, "Windows package copy changed {name}");
    Ok(format!("{actual}  {name}\n"))
}

fn checksum(file: &mut File) -> Result<String> {
    let mut hash = Sha256::new();
    io::copy(file, &mut hash)?;
    Ok(format!("{:x}", hash.finalize()))
}

#[cfg(windows)]
fn publish(stage: &Path, destination: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileW(existing: *const u16, new: *const u16) -> i32;
    }
    let parent = destination
        .parent()
        .context("Windows package parent")?
        .canonicalize()?;
    let stage = parent.join(stage.file_name().context("Windows package stage name")?);
    let destination = parent.join(
        destination
            .file_name()
            .context("Windows package directory")?,
    );
    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain([0])
            .collect::<Vec<_>>()
    };
    let stage = wide(&stage);
    let destination = wide(&destination);
    // MoveFileW keeps the same-volume directory rename atomic and refuses an existing name.
    if unsafe { MoveFileW(stage.as_ptr(), destination.as_ptr()) } == 0 {
        return Err(io::Error::last_os_error()).context("publish Windows package");
    }
    Ok(())
}

#[cfg(not(windows))]
fn publish(stage: &Path, destination: &Path) -> Result<()> {
    fs::rename(stage, destination).context("publish synthetic Windows package")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const NOTICE: &str = "third-party/runtime/windows-x86_64-msvc/texts/synthetic/original.txt";
    const MANIFEST: &str = "third-party/runtime/windows-x86_64-msvc/manifest.json";

    fn fixture() -> (PathBuf, Value) {
        let (root, mut manifest) = notices::tests::fixture();
        let original = root.join(manifest["files"][0]["source_path"].as_str().unwrap());
        let notice = root.join(NOTICE);
        fs::create_dir_all(notice.parent().unwrap()).unwrap();
        fs::copy(original, notice).unwrap();
        manifest["files"][0]["source_path"] = NOTICE.into();
        manifest["packages"][0]["notices"][0]["file"] = NOTICE.into();
        manifest["source_snapshot"]["target"] = TARGET.into();
        manifest["source_snapshot"]["roots"] = json!([
            concat!("cueglint@", env!("CARGO_PKG_VERSION")),
            concat!("cueglint-cli@", env!("CARGO_PKG_VERSION"))
        ]);
        let shared = "third-party/runtime/manifest.json";
        manifest["reused_source_inventory"] = json!({
            "source_path": shared,
            "current_checkout_sha256": crate::hash_file(&root.join(shared)).unwrap()
        });
        manifest["readiness_decision"] = json!({"id":"synthetic-source-adoption","target":TARGET});
        save(&root, &manifest);
        (root, manifest)
    }

    fn save(root: &Path, manifest: &Value) {
        fs::write(root.join(MANIFEST), serde_json::to_vec(manifest).unwrap()).unwrap();
    }

    fn fake_artifacts(target_dir: &Path) -> Result<Vec<u8>> {
        let release = target_dir.join(TARGET).join("release");
        fs::create_dir_all(&release)?;
        let mut artifacts = Vec::new();
        for (index, name) in ["cueglint", "cueglint-cli"].into_iter().enumerate() {
            let path = release.join(format!("{name}.exe"));
            let mut bytes = [0; 91];
            bytes[..2].copy_from_slice(b"MZ");
            bytes[60] = 64;
            bytes[64..70].copy_from_slice(b"PE\0\0\x64\x86");
            bytes[88..90].copy_from_slice(&[0x0b, 0x02]);
            bytes[90] = index as u8;
            fs::write(&path, bytes)?;
            serde_json::to_writer(
                &mut artifacts,
                &json!({"reason":"compiler-artifact","target":{"name":name,"kind":["bin"]},"executable":path}),
            )?;
            artifacts.push(b'\n');
        }
        Ok(artifacts)
    }

    #[test]
    fn windows_notice_gate_precedes_build_and_output() {
        let (root, mut manifest) = fixture();
        manifest["bundle_ready"] = false.into();
        save(&root, &manifest);
        let error = package(&root, |_| panic!("unapproved bundle reached build")).unwrap_err();
        assert!(error.to_string().contains("not approved for distribution"));
        manifest["bundle_ready"] = true.into();
        manifest["source_snapshot"]["target"] = "aarch64-apple-darwin".into();
        save(&root, &manifest);
        let error = package(&root, |_| panic!("wrong target reached build")).unwrap_err();
        assert!(error.to_string().contains("does not cover target"));
        manifest["source_snapshot"]["target"] = TARGET.into();
        let adopted = manifest["readiness_decision"].take();
        save(&root, &manifest);
        let error = package(&root, |_| panic!("missing adoption reached build")).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("requires a source adoption decision")
        );

        manifest["readiness_decision"] = adopted;
        let expected = manifest["reused_source_inventory"]["current_checkout_sha256"].take();
        manifest["reused_source_inventory"]["current_checkout_sha256"] = "0".repeat(64).into();
        save(&root, &manifest);
        let error = package(&root, |_| panic!("wrong shared source reached build")).unwrap_err();
        assert!(error.to_string().contains("notice input changed"));

        manifest["reused_source_inventory"]["current_checkout_sha256"] = expected;
        manifest["files"][0]["source_path"] = NOTICE.replace("original.txt", "NUL.txt").into();
        save(&root, &manifest);
        let error = package(&root, |_| panic!("unsafe path reached build")).unwrap_err();
        assert!(error.to_string().contains("unsafe on Windows"));
        assert!(!root.join("dist").exists());
        assert!(!root.join(BUILD_DIR).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn windows_package_copies_exact_artifacts_and_notices_and_preserves_existing_output() {
        let (root, _) = fixture();
        package(&root, fake_artifacts).unwrap();
        let dist = root.join("dist");
        let destination = dist.join(PACKAGE);
        assert_eq!(fs::read_dir(&dist).unwrap().count(), 1);
        assert_eq!(fs::read_dir(&destination).unwrap().count(), 5);
        let mut expected_sums = String::new();
        for name in ["cueglint.exe", "cueglint-cli.exe"] {
            let target = root.join(BUILD_DIR).join(TARGET).join("release").join(name);
            let packaged = destination.join(name);
            let bytes = fs::read(&target).unwrap();
            assert_eq!(fs::read(&packaged).unwrap(), bytes);
            let hard_link = root.join(name);
            fs::hard_link(target, &hard_link).unwrap();
            fs::write(hard_link, b"changed Cargo output").unwrap();
            assert_eq!(fs::read(&packaged).unwrap(), bytes);
            expected_sums.push_str(&format!(
                "{}  {name}\n",
                crate::hash_file(&packaged).unwrap()
            ));
        }
        for notice_root in NOTICE_ROOTS {
            for (output, source) in [("manifest.json", MANIFEST), (NOTICE, NOTICE)] {
                let relative = format!("{notice_root}/{output}");
                let packaged = destination.join(&relative);
                assert_eq!(
                    fs::read(&packaged).unwrap(),
                    fs::read(root.join(source)).unwrap()
                );
                expected_sums.push_str(&format!(
                    "{}  {relative}\n",
                    crate::hash_file(&packaged).unwrap()
                ));
            }
        }
        let error = package(&root, |_| panic!("existing destination reached build")).unwrap_err();
        assert!(error.to_string().contains("already exists"));
        assert_eq!(
            fs::read_to_string(destination.join("SHA256SUMS")).unwrap(),
            expected_sums
        );
        assert_eq!(fs::read_dir(dist).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn windows_package_revalidates_locked_inputs_and_artifact_locations() {
        let (root, _) = fixture();
        let error = package(&root, |target| {
            let artifacts = fake_artifacts(target)?;
            fs::write(root.join("Cargo.lock"), "changed during build")?;
            Ok(artifacts)
        })
        .unwrap_err();
        assert!(error.to_string().contains("notice input changed"));
        assert!(!root.join("dist").exists());
        fs::remove_dir_all(root).unwrap();

        let (root, _) = fixture();
        let error = package(&root, |_| fake_artifacts(&root.join("outside-target"))).unwrap_err();
        assert!(error.to_string().contains("unexpected path"));
        assert!(!root.join("dist").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retained_windows_inventory_inputs_and_originals_validate() {
        notices::load_windows(&crate::workspace_root().unwrap()).unwrap();
    }
}
