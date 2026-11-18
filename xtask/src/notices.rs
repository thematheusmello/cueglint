//! Source-owned notice inputs; this does not decide license applicability or linked coverage.
use super::{collect_source_files, hash_file};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::Read, path::Path};

const MANIFEST: &str = "third-party/runtime/manifest.json";
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_BUNDLE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Deserialize)]
struct Manifest {
    schema_version: u32,
    bundle_ready: bool,
    source_snapshot: Scope,
    inputs: Vec<Input>,
    files: Vec<Notice>,
    gaps: Vec<Gap>,
}

#[derive(Deserialize)]
struct Scope {
    target: String,
}

#[derive(Deserialize)]
struct Input {
    source_path: String,
    sha256: String,
}

#[derive(Deserialize)]
struct Notice {
    source_path: String,
    bytes: u64,
    sha256: String,
}

#[derive(Deserialize)]
struct Gap {
    id: String,
    blocks_bundle: bool,
}

pub(super) struct Bundle {
    manifest: Manifest,
    payload: BTreeMap<String, Vec<u8>>,
}

pub(super) fn output_path(root: &Path, relative: &str) -> Result<std::path::PathBuf> {
    ensure!(
        !relative.is_empty()
            && !relative.contains('\\')
            && !relative.contains(':')
            && !relative.chars().any(char::is_control)
            && relative.split('/').all(|part| !part.is_empty()
                && part != "."
                && part != ".."
                && !part.ends_with('.')
                && !part.ends_with(' ')),
        "notice path must be a safe repository-relative path: {relative:?}"
    );
    let mut path = root.to_owned();
    for part in relative.split('/') {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => ensure!(
                !metadata.file_type().is_symlink() && (metadata.is_file() || metadata.is_dir()),
                "notice path refuses links or special files: {}",
                path.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("inspect notice path"),
        }
    }
    Ok(path)
}

fn read(root: &Path, relative: &str) -> Result<Vec<u8>> {
    let path = output_path(root, relative)?;
    let metadata =
        fs::metadata(&path).with_context(|| format!("missing notice input {relative}"))?;
    ensure!(metadata.is_file(), "notice input is not a file: {relative}");
    ensure!(
        metadata.len() <= MAX_FILE_BYTES,
        "notice input too large: {relative}"
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_FILE_BYTES,
        "notice input grew too large"
    );
    Ok(bytes)
}

fn verify(bytes: &[u8], expected: &str, relative: &str) -> Result<()> {
    ensure!(
        format!("{:x}", Sha256::digest(bytes)) == expected,
        "notice input changed; review and refresh the inventory: {relative}"
    );
    Ok(())
}

pub(super) fn load(root: &Path) -> Result<Bundle> {
    let bytes = read(root, MANIFEST)?;
    let manifest: Manifest = serde_json::from_slice(&bytes).context("read notice manifest")?;
    ensure!(manifest.schema_version == 1, "unsupported notice schema");
    ensure!(
        !manifest.inputs.is_empty()
            && manifest.inputs.len() <= 256
            && !manifest.files.is_empty()
            && manifest.files.len() <= 1024,
        "notice inventory is empty or exceeds its input limits"
    );
    let mut inputs = std::collections::BTreeSet::new();
    for input in &manifest.inputs {
        ensure!(
            inputs.insert(input.source_path.clone()),
            "duplicate notice input"
        );
        verify(
            &read(root, &input.source_path)?,
            &input.sha256,
            &input.source_path,
        )?;
    }
    for required in [
        "Cargo.lock",
        "Cargo.toml",
        "rust-toolchain.toml",
        ".cargo/config.toml",
    ] {
        ensure!(
            inputs.contains(required),
            "missing required notice identity: {required}"
        );
    }
    let mut payload = BTreeMap::from([("manifest.json".to_owned(), bytes)]);
    let mut total = payload["manifest.json"].len();
    for file in &manifest.files {
        let bytes = read(root, &file.source_path)?;
        ensure!(
            bytes.len() as u64 == file.bytes,
            "notice size changed: {}",
            file.source_path
        );
        verify(&bytes, &file.sha256, &file.source_path)?;
        total = total
            .checked_add(bytes.len())
            .context("notice size overflow")?;
        ensure!(
            total <= MAX_BUNDLE_BYTES,
            "notice bundle exceeds size limit"
        );
        ensure!(
            payload.insert(file.source_path.clone(), bytes).is_none(),
            "duplicate notice output"
        );
    }
    let portable: std::collections::BTreeSet<_> = payload
        .keys()
        .map(|path| path.to_ascii_lowercase())
        .collect();
    ensure!(
        portable.len() == payload.len(),
        "notice output has a case collision"
    );
    for path in &portable {
        for (index, _) in path.match_indices('/') {
            ensure!(
                !portable.contains(&path[..index]),
                "notice output has a file/directory collision"
            );
        }
    }
    Ok(Bundle { manifest, payload })
}

impl Bundle {
    pub(super) fn require_ready(&self, target: &str) -> Result<()> {
        let blockers: Vec<_> = self
            .manifest
            .gaps
            .iter()
            .filter(|gap| gap.blocks_bundle)
            .map(|gap| gap.id.as_str())
            .collect();
        ensure!(
            self.manifest.bundle_ready && blockers.is_empty(),
            "notice inventory is not approved for distribution; remaining blockers: {}",
            blockers.join(", ")
        );
        ensure!(
            self.manifest.source_snapshot.target == target,
            "notice inventory does not cover target {target}"
        );
        Ok(())
    }

    pub(super) fn revalidate(&self, root: &Path) -> Result<()> {
        let current = load(root)?;
        ensure!(
            self.payload == current.payload,
            "notice inventory changed during build"
        );
        Ok(())
    }

    pub(super) fn install(&self, root: &Path, relative: &str, target: &str) -> Result<String> {
        self.require_ready(target)?;
        let destination = output_path(root, relative)?;
        let mut previous = Vec::new();
        collect_source_files(&destination, &mut previous)?;
        for path in previous {
            let name = path
                .strip_prefix(&destination)?
                .to_string_lossy()
                .replace('\\', "/");
            ensure!(
                self.payload.contains_key(&name),
                "stale notice output; remove the old generated notice tree: {}",
                destination.display()
            );
        }
        for (name, bytes) in &self.payload {
            let path = output_path(root, &format!("{relative}/{name}"))?;
            fs::create_dir_all(path.parent().context("notice parent directory")?)?;
            fs::write(&path, bytes)?;
        }
        self.checksums(root, relative)
    }

    pub(super) fn checksums(&self, root: &Path, relative: &str) -> Result<String> {
        let mut checksums = String::new();
        for (name, bytes) in &self.payload {
            let path = output_path(root, &format!("{relative}/{name}"))?;
            let actual = hash_file(&path)?;
            verify(bytes, &actual, name)?;
            checksums.push_str(&format!("{actual}  {relative}/{name}\n"));
        }
        Ok(checksums)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn fixture() -> (std::path::PathBuf, Value) {
        static ID: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "cueglint-notices-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let input_names = [
            "Cargo.lock",
            "Cargo.toml",
            "rust-toolchain.toml",
            ".cargo/config.toml",
            "crates/cueglint/Cargo.toml",
            "crates/cueglint-cli/Cargo.toml",
            "crates/cueglint-core/Cargo.toml",
            "crates/cueglint-local/Cargo.toml",
            "crates/cueglint-providers/Cargo.toml",
            "xtask/Cargo.toml",
            "vendor/eframe/Cargo.toml",
            "vendor/egui-wgpu/Cargo.toml",
        ];
        for name in input_names
            .into_iter()
            .chain(["third-party/runtime/texts/synthetic/original.txt"])
        {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, format!("Original synthetic notice/input: {name}\n")).unwrap();
        }
        let inputs: Vec<_> = input_names.into_iter().map(|source_path| json!({"source_path":source_path,"sha256":hash_file(&root.join(source_path)).unwrap()})).collect();
        let notice = root.join("third-party/runtime/texts/synthetic/original.txt");
        let manifest = json!({"schema_version":1,"bundle_ready":true,"source_snapshot":{"target":"aarch64-apple-darwin","candidate_packages":1},"inputs":inputs,"files":[{"source_path":"third-party/runtime/texts/synthetic/original.txt","bytes":fs::metadata(&notice).unwrap().len(),"sha256":hash_file(&notice).unwrap()}],"packages":[{"id":"synthetic@0.0.0","notices":[{"file":"third-party/runtime/texts/synthetic/original.txt"}]}],"embedded_assets":[],"toolchains":[],"additional_retained_envelopes":[],"gaps":[]});
        save(&root, &manifest);
        (root, manifest)
    }

    fn save(root: &Path, manifest: &Value) {
        let path = root.join(MANIFEST);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, serde_json::to_vec_pretty(manifest).unwrap()).unwrap();
    }

    #[test]
    fn retained_inventory_inputs_and_originals_validate_without_claiming_readiness() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        load(root).unwrap();
    }

    #[test]
    fn regression_private_notice_sources_are_rejected() {
        let (root, mut manifest) = fixture();
        let relative = ".private/synthetic-notice.txt";
        fs::create_dir(root.join(".private")).unwrap();
        fs::write(
            root.join(relative),
            b"Original synthetic private placeholder; no user data",
        )
        .unwrap();
        manifest["files"].as_array_mut().unwrap().push(json!({"source_path":relative,"bytes":fs::metadata(root.join(relative)).unwrap().len(),"sha256":hash_file(&root.join(relative)).unwrap()}));
        save(&root, &manifest);
        assert!(
            load(&root).is_err(),
            "accepted a private source into the public notice payload"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn regression_every_runtime_manifest_identity_is_required() {
        let (root, mut manifest) = fixture();
        manifest["inputs"]
            .as_array_mut()
            .unwrap()
            .retain(|input| input["source_path"] != "crates/cueglint/Cargo.toml");
        fs::write(
            root.join("crates/cueglint/Cargo.toml"),
            "changed untracked feature configuration",
        )
        .unwrap();
        save(&root, &manifest);
        assert!(
            load(&root).is_err(),
            "accepted an unbound runtime feature manifest"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn regression_association_references_must_exist_in_payload() {
        for association in [
            "packages",
            "embedded_assets",
            "toolchains",
            "additional_retained_envelopes",
        ] {
            let (root, mut manifest) = fixture();
            let missing = "third-party/runtime/texts/synthetic/omitted.txt";
            if association == "packages" {
                manifest["packages"][0]["notices"] = json!([{"file":missing}]);
            } else {
                manifest[association] = json!([{"notice_files":[missing]}]);
            }
            save(&root, &manifest);
            assert!(
                load(&root).is_err(),
                "accepted a missing {association} notice"
            );
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn regression_extra_copied_files_are_not_an_exact_notice_tree() {
        let (root, _) = fixture();
        let bundle = load(&root).unwrap();
        bundle
            .install(&root, "dist/notices", "aarch64-apple-darwin")
            .unwrap();
        fs::write(root.join("dist/notices/unlisted.txt"), "unlisted output").unwrap();
        assert!(
            bundle.checksums(&root, "dist/notices").is_err(),
            "accepted an extra file in the notice tree"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn regression_copied_file_validation_retains_its_read_bound() {
        let (root, _) = fixture();
        let bundle = load(&root).unwrap();
        bundle
            .install(&root, "dist/notices", "aarch64-apple-darwin")
            .unwrap();
        fs::File::options()
            .write(true)
            .open(root.join("dist/notices/third-party/runtime/texts/synthetic/original.txt"))
            .unwrap()
            .set_len(MAX_FILE_BYTES + 1)
            .unwrap();
        let error = bundle
            .checksums(&root, "dist/notices")
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("too large"),
            "copied-file check did not enforce its size bound: {error}"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn both_distributions_retain_exact_bytes_and_checksums_and_refuse_stale_outputs() {
        let (root, _) = fixture();
        let bundle = load(&root).unwrap();
        for destination in [
            "dist/Cueglint.app/Contents/Resources/THIRD_PARTY",
            "dist/cueglint-cli.NOTICES",
        ] {
            let sums = bundle
                .install(&root, destination, "aarch64-apple-darwin")
                .unwrap();
            for (name, bytes) in &bundle.payload {
                let path = format!("{destination}/{name}");
                assert_eq!(fs::read(root.join(&path)).unwrap(), *bytes);
                assert!(sums.contains(&format!(
                    "{}  {path}\n",
                    hash_file(&root.join(path)).unwrap()
                )));
            }
            assert_eq!(
                bundle
                    .install(&root, destination, "aarch64-apple-darwin")
                    .unwrap(),
                sums
            );
            fs::write(
                root.join(destination)
                    .join("third-party/runtime/texts/synthetic/original.txt"),
                "altered copied notice",
            )
            .unwrap();
            assert!(bundle.checksums(&root, destination).is_err());
            bundle
                .install(&root, destination, "aarch64-apple-darwin")
                .unwrap();
        }
        fs::write(
            root.join("dist/cueglint-cli.NOTICES/removed-notice.txt"),
            b"stale",
        )
        .unwrap();
        assert!(
            bundle
                .install(&root, "dist/cueglint-cli.NOTICES", "aarch64-apple-darwin")
                .is_err()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_altered_duplicate_or_stale_notice_inputs_are_rejected() {
        let (root, _) = fixture();
        let bundle = load(&root).unwrap();
        fs::write(root.join("Cargo.lock"), "changed dependency resolution").unwrap();
        assert!(bundle.revalidate(&root).is_err());
        fs::remove_dir_all(root).unwrap();
        let (root, mut manifest) = fixture();
        let bundle = load(&root).unwrap();
        manifest["collection_state"] = "changed during build".into();
        save(&root, &manifest);
        assert!(bundle.revalidate(&root).is_err());
        fs::remove_dir_all(root).unwrap();
        for change in [
            "missing",
            "altered",
            "size",
            "duplicate",
            "input-identity",
            "schema",
        ] {
            let (root, mut manifest) = fixture();
            match change {
                "missing" => {
                    fs::remove_file(root.join("third-party/runtime/texts/synthetic/original.txt"))
                        .unwrap()
                }
                "altered" => fs::write(
                    root.join("third-party/runtime/texts/synthetic/original.txt"),
                    "changed notice",
                )
                .unwrap(),
                "size" => manifest["files"][0]["bytes"] = 0.into(),
                "duplicate" => {
                    let duplicate = manifest["files"][0].clone();
                    manifest["files"].as_array_mut().unwrap().push(duplicate);
                }
                "input-identity" => {
                    manifest["inputs"].as_array_mut().unwrap().remove(0);
                }
                "schema" => manifest["schema_version"] = 2.into(),
                _ => unreachable!(),
            }
            save(&root, &manifest);
            assert!(load(&root).is_err(), "accepted {change}");
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn unapproved_or_wrong_target_inventory_never_creates_distribution_outputs() {
        for change in ["not-ready", "blocker", "target"] {
            let (root, mut manifest) = fixture();
            match change {
                "not-ready" => manifest["bundle_ready"] = false.into(),
                "blocker" => {
                    manifest["gaps"] = json!([{"id":"missing-original","blocks_bundle":true}])
                }
                "target" => manifest["source_snapshot"]["target"] = "unreviewed-target".into(),
                _ => unreachable!(),
            }
            save(&root, &manifest);
            let bundle = load(&root).unwrap();
            assert!(
                bundle
                    .install(&root, "dist/notices", "aarch64-apple-darwin")
                    .is_err()
            );
            assert!(!root.join("dist").exists());
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn oversized_inputs_and_portable_output_collisions_are_rejected() {
        for change in ["oversized", "case-collision", "reserved-manifest-directory"] {
            let (root, mut manifest) = fixture();
            let relative = match change {
                "oversized" => "third-party/runtime/texts/synthetic/original.txt",
                "case-collision" => "third-party/runtime/texts/synthetic/ORIGINAL.txt",
                "reserved-manifest-directory" => "manifest.json/original.txt",
                _ => unreachable!(),
            };
            let path = root.join(relative);
            if change == "oversized" {
                fs::File::options()
                    .write(true)
                    .open(path)
                    .unwrap()
                    .set_len(MAX_FILE_BYTES + 1)
                    .unwrap();
            } else {
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(
                    &path,
                    fs::read(root.join("third-party/runtime/texts/synthetic/original.txt"))
                        .unwrap(),
                )
                .unwrap();
                manifest["files"].as_array_mut().unwrap().push(json!({"source_path":relative,"bytes":fs::metadata(&path).unwrap().len(),"sha256":hash_file(&path).unwrap()}));
            }
            save(&root, &manifest);
            assert!(load(&root).is_err(), "accepted {change}");
            assert!(!root.join("dist").exists());
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn unsafe_paths_and_parent_links_cannot_escape_the_notice_roots() {
        let (root, _) = fixture();
        for path in [
            "",
            "/absolute",
            "../outside",
            "a/../outside",
            "a//b",
            "./notice",
            "C:/outside",
            "a\\outside",
            "line\nbreak",
            "trailing./file",
            "trailing /file",
        ] {
            assert!(output_path(&root, path).is_err(), "accepted {path:?}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                root.join("third-party/runtime/texts/synthetic"),
                root.join("linked"),
            )
            .unwrap();
            assert!(output_path(&root, "linked/original.txt").is_err());
            std::os::unix::fs::symlink(
                root.join("third-party/runtime/texts/synthetic/original.txt"),
                root.join("linked-file"),
            )
            .unwrap();
            assert!(output_path(&root, "linked-file").is_err());
            let original =
                fs::read(root.join("third-party/runtime/texts/synthetic/original.txt")).unwrap();
            assert!(
                load(&root)
                    .unwrap()
                    .install(&root, "linked", "aarch64-apple-darwin")
                    .is_err()
            );
            assert_eq!(
                fs::read(root.join("third-party/runtime/texts/synthetic/original.txt")).unwrap(),
                original
            );
        }
        fs::remove_dir_all(root).unwrap();
    }
}
