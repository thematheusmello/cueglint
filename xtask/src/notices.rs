//! Source-owned notice inputs; this does not decide license applicability or linked coverage.
#[cfg(test)]
use super::hash_file;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::Path,
};

const MANIFEST: &str = "third-party/runtime/manifest.json";
const WINDOWS_MANIFEST: &str = "third-party/runtime/windows-x86_64-msvc/manifest.json";
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_BUNDLE_BYTES: usize = 16 * 1024 * 1024;
const INPUTS: &[&str] = &[
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

#[derive(Deserialize)]
struct Manifest {
    schema_version: u32,
    bundle_ready: bool,
    source_snapshot: Scope,
    inputs: Vec<Input>,
    files: Vec<Notice>,
    packages: Vec<Package>,
    embedded_assets: Vec<Associations>,
    toolchains: Vec<Associations>,
    additional_retained_envelopes: Vec<Associations>,
    gaps: Vec<Gap>,
    reused_source_inventory: Option<ReusedSourceInventory>,
    readiness_decision: Option<NoticeDecision>,
}

#[derive(Deserialize)]
struct ReusedSourceInventory {
    source_path: String,
    current_checkout_sha256: String,
}

#[derive(Deserialize)]
struct NoticeDecision {
    id: String,
    target: String,
}

#[derive(Deserialize)]
struct Scope {
    target: String,
    candidate_packages: usize,
    #[serde(default)]
    roots: Vec<String>,
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

#[derive(Deserialize)]
struct Package {
    id: String,
    notices: Vec<Reference>,
}

#[derive(Deserialize)]
struct Reference {
    file: String,
}

#[derive(Deserialize)]
struct Associations {
    notice_files: Vec<String>,
}

fn allowed_notice(relative: &str, windows: bool) -> bool {
    [
        "LICENSE-APACHE",
        "LICENSE-MIT",
        "THIRD_PARTY.md",
        "ui/brand/LICENSE",
        "third-party/runtime/README.md",
        "third-party/runtime/omissions.json",
        "vendor/eframe/LICENSE-APACHE",
        "vendor/eframe/LICENSE-MIT",
        "vendor/eframe/CUEGLINT-PATCH.json",
        "vendor/egui-wgpu/LICENSE-APACHE",
        "vendor/egui-wgpu/LICENSE-MIT",
        "vendor/egui-wgpu/CUEGLINT-PATCH.json",
    ]
    .contains(&relative)
        || [
            "third-party/runtime/texts/",
            "crates/cueglint/third-party/",
            "crates/cueglint-providers/third-party/",
            "crates/cueglint-core/third-party/knowledge/",
            "crates/cueglint-local/assets/licenses/asr-frontend/",
        ]
        .iter()
        .any(|prefix| relative.starts_with(prefix))
        || (windows
            && (matches!(
                relative,
                "third-party/runtime/windows-x86_64-msvc/README.md"
                    | "third-party/runtime/windows-x86_64-msvc/graph.json"
            ) || relative.starts_with("third-party/runtime/windows-x86_64-msvc/texts/")))
}

fn windows_name(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let device = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || matches!(
            stem.as_bytes(),
            [b'C', b'O', b'M', b'0'..=b'9'] | [b'L', b'P', b'T', b'0'..=b'9']
        );
    !device
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-+".contains(&byte))
}

pub(super) struct Bundle {
    manifest_path: &'static str,
    manifest: Manifest,
    payload: BTreeMap<String, Vec<u8>>,
}

fn is_link(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    false
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
                !is_link(&metadata) && (metadata.is_file() || metadata.is_dir()),
                "notice path refuses links or special files: {}",
                path.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("inspect notice path"),
        }
    }
    Ok(path)
}

pub(super) fn open_regular(path: &Path) -> Result<fs::File> {
    let mut options = fs::File::options();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 1;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && !is_link(&metadata),
        "expected a regular file without reparse points: {}",
        path.display()
    );
    Ok(file)
}

fn read(root: &Path, relative: &str) -> Result<Vec<u8>> {
    let path = output_path(root, relative)?;
    let file = open_regular(&path).with_context(|| format!("open notice input {relative}"))?;
    ensure!(
        file.metadata()?.len() <= MAX_FILE_BYTES,
        "notice input too large: {relative}"
    );
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
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
    load_manifest(root, MANIFEST)
}

#[cfg(any(test, all(windows, target_arch = "x86_64", target_env = "msvc")))]
pub(super) fn load_windows(root: &Path) -> Result<Bundle> {
    load_manifest(root, WINDOWS_MANIFEST)
}

fn load_manifest(root: &Path, manifest_path: &'static str) -> Result<Bundle> {
    let windows = manifest_path == WINDOWS_MANIFEST;
    let bytes = read(root, manifest_path)?;
    let manifest: Manifest = serde_json::from_slice(&bytes).context("read notice manifest")?;
    ensure!(manifest.schema_version == 1, "unsupported notice schema");
    if windows {
        ensure!(
            manifest.source_snapshot.roots
                == [
                    concat!("cueglint@", env!("CARGO_PKG_VERSION")),
                    concat!("cueglint-cli@", env!("CARGO_PKG_VERSION"))
                ],
            "Windows notice inventory does not cover the selected GUI and CLI roots"
        );
        let shared = manifest
            .reused_source_inventory
            .as_ref()
            .context("Windows notice inventory is missing its shared source identity")?;
        ensure!(
            shared.source_path == MANIFEST,
            "Windows notice inventory has an unexpected shared source path"
        );
        verify(
            &read(root, MANIFEST)?,
            &shared.current_checkout_sha256,
            MANIFEST,
        )?;
    }
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
            INPUTS.contains(&input.source_path.as_str()),
            "unreviewed notice input identity: {}",
            input.source_path
        );
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
    for &required in INPUTS {
        ensure!(
            inputs.contains(required),
            "missing required notice identity: {required}"
        );
    }
    let mut payload = BTreeMap::from([("manifest.json".to_owned(), bytes)]);
    let mut total = payload["manifest.json"].len();
    for file in &manifest.files {
        ensure!(
            allowed_notice(&file.source_path, windows),
            "notice source is outside reviewed public notice roots: {}",
            file.source_path
        );
        ensure!(
            !windows || file.source_path.split('/').all(windows_name),
            "notice path is unsafe on Windows: {}",
            file.source_path
        );
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
    if windows {
        let mut directories = BTreeMap::new();
        for path in payload.keys() {
            for (index, _) in path.match_indices('/') {
                let name = &path[..index];
                ensure!(
                    *directories.entry(name.to_ascii_lowercase()).or_insert(name) == name,
                    "notice output has a directory case collision: {name}"
                );
            }
        }
    }
    ensure!(
        manifest.packages.len() == manifest.source_snapshot.candidate_packages
            && !manifest.packages.is_empty(),
        "notice package inventory count changed"
    );
    let mut packages = BTreeSet::new();
    for package in &manifest.packages {
        ensure!(
            !package.id.is_empty() && packages.insert(&package.id),
            "empty or duplicate notice package identity"
        );
        ensure!(
            !windows || !package.notices.is_empty(),
            "Windows package notice association is missing: {}",
            package.id
        );
        for reference in &package.notices {
            ensure!(
                reference.file != "manifest.json" && payload.contains_key(&reference.file),
                "package notice reference is missing: {}",
                reference.file
            );
        }
    }
    for association in manifest
        .embedded_assets
        .iter()
        .chain(&manifest.toolchains)
        .chain(&manifest.additional_retained_envelopes)
    {
        for name in &association.notice_files {
            ensure!(
                name != "manifest.json" && payload.contains_key(name),
                "asset/toolchain/envelope notice reference is missing: {name}"
            );
        }
    }
    Ok(Bundle {
        manifest_path,
        manifest,
        payload,
    })
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
        if self.manifest_path == WINDOWS_MANIFEST {
            ensure!(
                self.manifest
                    .readiness_decision
                    .as_ref()
                    .is_some_and(|decision| {
                        !decision.id.trim().is_empty() && decision.target == target
                    }),
                "Windows notice readiness requires a source adoption decision for {target}"
            );
        }
        Ok(())
    }

    pub(super) fn revalidate(&self, root: &Path) -> Result<()> {
        let current = load_manifest(root, self.manifest_path)?;
        ensure!(
            self.payload == current.payload,
            "notice inventory changed during build"
        );
        Ok(())
    }

    pub(super) fn install(&self, root: &Path, relative: &str, target: &str) -> Result<String> {
        self.require_ready(target)?;
        self.members(root, relative)?;
        for (name, bytes) in &self.payload {
            let path = output_path(root, &format!("{relative}/{name}"))?;
            fs::create_dir_all(path.parent().context("notice parent directory")?)?;
            if self.manifest_path == WINDOWS_MANIFEST {
                fs::File::create_new(path)?.write_all(bytes)?;
            } else {
                fs::write(path, bytes)?;
            }
        }
        self.checksums(root, relative)
    }

    pub(super) fn checksums(&self, root: &Path, relative: &str) -> Result<String> {
        ensure!(
            self.members(root, relative)?.len() == self.payload.len(),
            "notice output tree is incomplete"
        );
        let mut checksums = String::new();
        for (name, bytes) in &self.payload {
            let actual_bytes = read(root, &format!("{relative}/{name}"))?;
            let actual = format!("{:x}", Sha256::digest(actual_bytes));
            verify(bytes, &actual, name)?;
            checksums.push_str(&format!("{actual}  {relative}/{name}\n"));
        }
        Ok(checksums)
    }

    fn members(&self, root: &Path, relative: &str) -> Result<BTreeSet<String>> {
        let destination = output_path(root, relative)?;
        if !destination.exists() {
            return Ok(BTreeSet::new());
        }
        let mut directories = BTreeSet::from([String::new()]);
        for name in self.payload.keys() {
            for (index, _) in name.match_indices('/') {
                directories.insert(name[..index].to_owned());
            }
        }
        let mut pending = vec![String::new()];
        let mut visited = BTreeSet::new();
        let mut files = BTreeSet::new();
        while let Some(prefix) = pending.pop() {
            ensure!(
                visited.insert(prefix.clone()) && visited.len() <= directories.len(),
                "notice directory traversal exceeded its declared tree"
            );
            for entry in fs::read_dir(destination.join(&prefix))? {
                let entry = entry?;
                let basename = entry.file_name();
                let basename = basename
                    .to_str()
                    .context("notice output has a non-UTF8 name")?;
                let name = if prefix.is_empty() {
                    basename.to_owned()
                } else {
                    format!("{prefix}/{basename}")
                };
                let metadata = entry.metadata()?;
                if metadata.is_dir() && !is_link(&metadata) {
                    ensure!(
                        directories.contains(&name),
                        "unexpected notice output directory: {name}"
                    );
                    pending.push(name);
                } else {
                    ensure!(
                        metadata.is_file()
                            && !is_link(&metadata)
                            && self.payload.contains_key(&name),
                        "unexpected notice output file or link: {name}"
                    );
                    ensure!(
                        files.insert(name) && files.len() <= self.payload.len(),
                        "notice output exceeds its declared files"
                    );
                }
            }
        }
        Ok(files)
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::sync::atomic::{AtomicUsize, Ordering};

    pub(crate) fn fixture() -> (std::path::PathBuf, Value) {
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
        let root = crate::workspace_root().unwrap();
        load(&root).unwrap();
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
    fn unreviewed_config_inputs_are_rejected_before_reading_them() {
        let (root, mut manifest) = fixture();
        manifest["inputs"]
            .as_array_mut()
            .unwrap()
            .push(json!({"source_path":".private/nonexistent-config.toml","sha256":"not-read"}));
        save(&root, &manifest);
        let error = load(&root).err().unwrap().to_string();
        assert!(
            error.contains("unreviewed notice input identity"),
            "{error}"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn undeclared_output_directories_are_rejected_before_traversal() {
        let (root, _) = fixture();
        let bundle = load(&root).unwrap();
        bundle
            .install(&root, "dist/notices", "aarch64-apple-darwin")
            .unwrap();
        let unexpected = root.join("dist/notices/unlisted");
        fs::create_dir(&unexpected).unwrap();
        fs::write(unexpected.join("not-traversed.txt"), "unexpected subtree").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&unexpected, unexpected.join("not-traversed-cycle")).unwrap();
        let error = bundle
            .checksums(&root, "dist/notices")
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("unexpected notice output directory: unlisted"),
            "{error}"
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
                    hash_file(&root.join(&path)).unwrap()
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
