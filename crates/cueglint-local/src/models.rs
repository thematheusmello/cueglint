//! Pinned, hash-verified models. Only an explicit call to `install` performs network I/O.

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub const GENERATION_PROFILE: &str = "qwen2.5-1.5b-instruct";
pub const TRANSCRIPTION_PROFILE: &str = "whisper-base-en";
pub const SMALL_GENERATION_PROFILE: &str = "qwen2.5-0.5b-instruct";
pub const SMALL_TRANSCRIPTION_PROFILE: &str = "whisper-tiny-en";
pub const QUALITY_GENERATION_PROFILE: &str = GENERATION_PROFILE;
pub const QUALITY_TRANSCRIPTION_PROFILE: &str = TRANSCRIPTION_PROFILE;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelFile {
    pub name: String,
    pub repository: String,
    pub revision: String,
    pub sha256: String,
    pub bytes: u64,
}

impl ModelFile {
    fn url(&self) -> String {
        format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            self.repository, self.revision, self.name
        )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelProfile {
    pub id: String,
    pub name: String,
    pub purpose: String,
    pub license: String,
    pub license_url: String,
    pub files: Vec<ModelFile>,
}

#[derive(Clone, Debug, Serialize)]
pub struct InstalledProfile {
    pub profile: ModelProfile,
    pub directory: PathBuf,
}

impl InstalledProfile {
    pub fn file(&self, name: &str) -> Result<PathBuf> {
        ensure!(
            self.profile.files.iter().any(|file| file.name == name),
            "file is not part of the verified profile"
        );
        Ok(self.directory.join(name))
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct DownloadProgress {
    pub profile_id: String,
    pub file: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    profiles: Vec<ModelProfile>,
}

pub fn profiles() -> Result<Vec<ModelProfile>> {
    let manifest: Manifest =
        serde_json::from_str(include_str!("../../../models/local-defaults.json"))?;
    ensure!(manifest.version == 1, "unsupported model manifest version");
    let mut ids = BTreeSet::new();
    for profile in &manifest.profiles {
        validate_profile(profile)?;
        ensure!(ids.insert(&profile.id), "duplicate model profile");
    }
    Ok(manifest.profiles)
}

pub fn profile(profile_id: &str) -> Result<ModelProfile> {
    profiles()?
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .context("unknown model profile")
}

/// Read and hash every artifact. A matching filename alone never counts as an installed model.
pub fn verify(root: &Path, profile_id: &str) -> Result<InstalledProfile> {
    verify_with_cancel(root, profile_id, &AtomicBool::new(false))
}

pub fn verify_with_cancel(
    root: &Path,
    profile_id: &str,
    cancel: &AtomicBool,
) -> Result<InstalledProfile> {
    let profile = profile(profile_id)?;
    let directory = root.join(&profile.id);
    verify_profile(&directory, &profile, cancel)
}

fn verify_profile(
    directory: &Path,
    profile: &ModelProfile,
    cancel: &AtomicBool,
) -> Result<InstalledProfile> {
    ensure_plain_directory(directory)?;
    for file in &profile.files {
        verify_file(&directory.join(&file.name), file, cancel).with_context(|| {
            format!(
                "verify {}; install the {} model profile first",
                file.name, profile.id
            )
        })?;
    }
    Ok(InstalledProfile {
        profile: profile.clone(),
        directory: directory.to_owned(),
    })
}

/// Resume partial downloads, verify SHA-256, then atomically publish each completed file.
pub fn install(
    root: &Path,
    profile_id: &str,
    cancel: &AtomicBool,
    mut progress: impl FnMut(DownloadProgress),
) -> Result<InstalledProfile> {
    let profile = profile(profile_id)?;
    let (directory, _lock) = lock_profile(root, &profile, true)?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .https_only(true)
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .timeout_recv_body(Some(Duration::from_secs(30 * 60)))
        .build()
        .into();
    for file in &profile.files {
        ensure!(
            !cancel.load(Ordering::Relaxed),
            "model installation cancelled"
        );
        if verify_file(&directory.join(&file.name), file, cancel).is_ok() {
            progress(DownloadProgress {
                profile_id: profile.id.clone(),
                file: file.name.clone(),
                downloaded_bytes: file.bytes,
                total_bytes: file.bytes,
            });
            continue;
        }
        download_file(
            &agent,
            &file.url(),
            &directory,
            &profile.id,
            file,
            cancel,
            &mut progress,
        )?;
    }
    verify_profile(&directory, &profile, cancel)
}

/// Import only catalog-pinned artifacts from a local directory. No network is opened.
/// Cancellation before publication leaves existing files unchanged; verified publication completes once started.
pub fn import(
    root: &Path,
    profile_id: &str,
    source: &Path,
    cancel: &AtomicBool,
    progress: impl FnMut(DownloadProgress),
) -> Result<InstalledProfile> {
    import_profile(root, &profile(profile_id)?, source, cancel, progress)
}

fn import_profile(
    root: &Path,
    profile: &ModelProfile,
    source: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(DownloadProgress),
) -> Result<InstalledProfile> {
    validate_profile(profile)?;
    ensure!(!cancel.load(Ordering::Relaxed), "model import cancelled");
    ensure_plain_directory(source)?;
    let source = source
        .canonicalize()
        .context("resolve model import directory")?;
    if let Ok(destination) = root.join(&profile.id).canonicalize() {
        ensure!(
            source != destination,
            "model import source and destination are the same directory"
        );
    }
    let (directory, _lock) = lock_profile(root, profile, true)?;
    ensure!(
        source != directory,
        "model import source and destination are the same directory"
    );
    for file in &profile.files {
        ensure_regular_destination(&directory.join(&file.name))?;
    }
    let stage = directory.join(format!(".import-{}", uuid::Uuid::new_v4()));
    create_private_directory(&stage, false)?;
    let result = (|| -> Result<InstalledProfile> {
        let mut staged = Vec::new();
        for file in &profile.files {
            ensure!(!cancel.load(Ordering::Relaxed), "model import cancelled");
            let already_valid = verify_file(&directory.join(&file.name), file, cancel).is_ok();
            ensure!(!cancel.load(Ordering::Relaxed), "model import cancelled");
            if already_valid {
                verify_file(&source.join(&file.name), file, cancel)
                    .with_context(|| format!("verify source artifact {}", file.name))?;
                progress(DownloadProgress {
                    profile_id: profile.id.clone(),
                    file: file.name.clone(),
                    downloaded_bytes: file.bytes,
                    total_bytes: file.bytes,
                });
            } else {
                copy_verified_file(
                    &source.join(&file.name),
                    &stage.join(&file.name),
                    &profile.id,
                    file,
                    cancel,
                    &mut progress,
                )?;
                staged.push(file);
            }
        }
        ensure!(
            !cancel.load(Ordering::Relaxed),
            "model import cancelled before publication"
        );
        for file in staged {
            ensure_regular_destination(&directory.join(&file.name))?;
            std::fs::rename(stage.join(&file.name), directory.join(&file.name))
                .context("publish verified imported model")?;
        }
        verify_profile(&directory, profile, &AtomicBool::new(false))
    })();
    let cleanup = std::fs::remove_dir_all(stage);
    if result.is_ok() {
        cleanup.context("remove model import staging directory")?;
    }
    result
}

/// Remove known model artifacts and partial downloads, retaining the stable coordination lock.
/// Unknown files, links or devices cause refusal before any artifact is deleted.
pub fn remove(root: &Path, profile_id: &str) -> Result<()> {
    remove_profile(root, &profile(profile_id)?)
}

fn remove_profile(root: &Path, profile: &ModelProfile) -> Result<()> {
    validate_profile(profile)?;
    let (directory, _lock) = lock_profile(root, profile, false)?;
    let mut allowed = BTreeSet::new();
    allowed.insert(std::ffi::OsString::from(".install.lock"));
    for file in &profile.files {
        allowed.insert(file.name.clone().into());
        allowed.insert(format!("{}.part", file.name).into());
    }
    let mut artifacts = Vec::new();
    for entry in std::fs::read_dir(&directory)? {
        let entry = entry?;
        let name = entry.file_name();
        ensure!(
            allowed.contains(&name),
            "profile contains unknown entry {name:?}; no files were removed"
        );
        ensure_regular_destination(&entry.path())?;
        if name != ".install.lock" {
            artifacts.push(entry.path());
        }
    }
    for path in artifacts {
        std::fs::remove_file(path).context("remove model artifact")?;
    }
    Ok(())
}

fn lock_profile(root: &Path, profile: &ModelProfile, create: bool) -> Result<(PathBuf, File)> {
    safe_name(&profile.id)?;
    if create {
        create_private_directory(root, true)?;
    }
    let directory = root
        .canonicalize()
        .context("resolve model root")?
        .join(&profile.id);
    if create && !directory.exists() {
        create_private_directory(&directory, false)?;
    }
    ensure_plain_directory(&directory)?;
    let directory = directory.canonicalize()?;
    let path = directory.join(".install.lock");
    ensure_regular_destination(&path)?;
    let lock = private_options().create(true).open(path)?;
    lock.try_lock()
        .context("this model is already being changed by another process")?;
    Ok((directory, lock))
}

fn create_private_directory(path: &Path, recursive: bool) -> Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(recursive);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(path)
        .context("create private model directory")
}

fn ensure_plain_directory(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path).context("open model directory")?;
    ensure!(
        metadata.is_dir() && !is_link(&metadata),
        "model directory must be a regular directory, not a link"
    );
    Ok(())
}

fn copy_verified_file(
    source: &Path,
    destination: &Path,
    profile_id: &str,
    expected: &ModelFile,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(DownloadProgress),
) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "model import cancelled");
    ensure_regular_destination(source)?;
    let mut input = File::open(source).context("open source model artifact")?;
    let metadata = input.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() == expected.bytes,
        "source model size or file type does not match manifest"
    );
    let mut output = private_options().create_new(true).open(destination)?;
    let mut hasher = Sha256::new();
    let mut copied = 0;
    let mut buffer = [0u8; 128 * 1024];
    loop {
        ensure!(!cancel.load(Ordering::Relaxed), "model import cancelled");
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        copied += count as u64;
        ensure!(copied <= expected.bytes, "source model exceeds pinned size");
        output.write_all(&buffer[..count])?;
        hasher.update(&buffer[..count]);
        progress(DownloadProgress {
            profile_id: profile_id.to_owned(),
            file: expected.name.clone(),
            downloaded_bytes: copied,
            total_bytes: expected.bytes,
        });
    }
    ensure!(
        copied == expected.bytes && format!("{:x}", hasher.finalize()) == expected.sha256,
        "source model SHA-256 or size does not match manifest"
    );
    output.sync_all()?;
    Ok(())
}

fn validate_profile(profile: &ModelProfile) -> Result<()> {
    safe_name(&profile.id)?;
    ensure!(
        !profile.files.is_empty() && profile.files.len() <= 10,
        "invalid model file count"
    );
    let mut files = BTreeSet::new();
    for file in &profile.files {
        safe_name(&file.name)?;
        ensure!(files.insert(&file.name), "duplicate model file");
        ensure!(
            file.repository.split('/').count() == 2
                && file.repository.split('/').all(|s| !s.is_empty()
                    && s.chars()
                        .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))),
            "invalid model repository"
        );
        ensure!(
            file.revision.len() == 40 && file.revision.chars().all(|c| c.is_ascii_hexdigit()),
            "model revision must be a pinned commit"
        );
        ensure!(
            file.sha256.len() == 64 && file.sha256.chars().all(|c| c.is_ascii_hexdigit()),
            "invalid model checksum"
        );
        ensure!(
            file.bytes > 0 && file.bytes <= 8 * 1024 * 1024 * 1024,
            "invalid model file size"
        );
    }
    Ok(())
}

fn safe_name(name: &str) -> Result<()> {
    let mut components = Path::new(name).components();
    ensure!(
        matches!(components.next(), Some(Component::Normal(_)))
            && components.next().is_none()
            && !name.contains(['/', '\\', '\0'])
            && !name.starts_with('.'),
        "model paths must be plain filenames"
    );
    Ok(())
}

fn private_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}

fn verify_file(path: &Path, expected: &ModelFile, cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "model verification cancelled"
    );
    ensure_regular_destination(path)?;
    let metadata = std::fs::metadata(path)?;
    ensure!(
        metadata.is_file() && metadata.len() == expected.bytes,
        "model size or file type does not match manifest"
    );
    let mut file = File::open(path)?;
    ensure!(
        file.metadata()?.len() == expected.bytes,
        "model size does not match manifest"
    );
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 128 * 1024];
    loop {
        ensure!(
            !cancel.load(Ordering::Relaxed),
            "model verification cancelled"
        );
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    ensure!(
        format!("{:x}", hasher.finalize()) == expected.sha256,
        "model SHA-256 does not match manifest"
    );
    Ok(())
}

fn download_file(
    agent: &ureq::Agent,
    url: &str,
    directory: &Path,
    profile_id: &str,
    expected: &ModelFile,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(DownloadProgress),
) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "model installation cancelled"
    );
    let part_path = directory.join(format!("{}.part", expected.name));
    ensure_regular_destination(&part_path)?;
    let mut offset = std::fs::metadata(&part_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    ensure!(
        offset <= expected.bytes,
        "partial model exceeds expected size; remove {} and retry",
        part_path.display()
    );
    if offset < expected.bytes {
        let mut request = agent.get(url);
        if offset > 0 {
            request = request.header("Range", format!("bytes={offset}-"));
        }
        let mut response = request.call().context("download pinned model file")?;
        if offset > 0 && response.status().as_u16() == 206 {
            let expected_range = format!("bytes {offset}-");
            ensure!(
                response
                    .headers()
                    .get("content-range")
                    .and_then(|value| value.to_str().ok())
                    .is_some_and(|value| value.starts_with(&expected_range)),
                "server returned a different download range"
            );
        } else {
            ensure!(
                response.status().as_u16() == 200,
                "unexpected model download response"
            );
            offset = 0;
        }
        let mut file = private_options()
            .create(true)
            .append(offset > 0)
            .truncate(offset == 0)
            .open(&part_path)?;
        let mut reader = response.body_mut().as_reader();
        let mut buffer = [0u8; 128 * 1024];
        loop {
            ensure!(
                !cancel.load(Ordering::Relaxed),
                "model installation cancelled; partial download retained"
            );
            let count = reader.read(&mut buffer).context("read model download")?;
            if count == 0 {
                break;
            }
            ensure!(
                offset + count as u64 <= expected.bytes,
                "model download exceeds pinned size"
            );
            file.write_all(&buffer[..count])?;
            offset += count as u64;
            progress(DownloadProgress {
                profile_id: profile_id.to_owned(),
                file: expected.name.clone(),
                downloaded_bytes: offset,
                total_bytes: expected.bytes,
            });
        }
        file.sync_all()?;
    }
    ensure!(
        offset == expected.bytes,
        "model download was incomplete; retry to resume"
    );
    if let Err(error) = verify_file(&part_path, expected, cancel) {
        if !cancel.load(Ordering::Relaxed) {
            let _ = std::fs::remove_file(&part_path);
        }
        return Err(
            error.context("download integrity failed; existing installed model was preserved")
        );
    }
    std::fs::rename(part_path, directory.join(&expected.name)).context("publish verified model")?;
    Ok(())
}

fn ensure_regular_destination(path: &Path) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => ensure!(
            metadata.is_file() && !is_link(&metadata),
            "model installer destination must be a regular file, not a link or device"
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn is_link(metadata: &std::fs::Metadata) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Result<(PathBuf, PathBuf, PathBuf, ModelProfile)> {
        let base =
            std::env::temp_dir().join(format!("cueglint-model-files-{}", uuid::Uuid::new_v4()));
        let source = base.join("source");
        let root = base.join("models");
        std::fs::create_dir_all(&source)?;
        let artifacts: [(&str, &[u8]); 2] = [
            ("weights.bin", b"Original synthetic model fixture bytes."),
            (
                "LICENSE",
                b"Original test fixture; no external model content.",
            ),
        ];
        let mut files = Vec::new();
        for (name, bytes) in artifacts {
            std::fs::write(source.join(name), bytes)?;
            files.push(ModelFile {
                name: name.to_owned(),
                repository: "fixture/model".to_owned(),
                revision: "1".repeat(40),
                sha256: format!("{:x}", Sha256::digest(bytes)),
                bytes: bytes.len() as u64,
            });
        }
        let profile = ModelProfile {
            id: "original-fixture".to_owned(),
            name: "Original test fixture".to_owned(),
            purpose: "generation".to_owned(),
            license: "Original fixture license".to_owned(),
            license_url: "https://example.invalid/license".to_owned(),
            files,
        };
        Ok((base, source, root, profile))
    }

    #[test]
    fn local_import_verifies_pinned_files_and_preserves_licenses_and_source_extras() -> Result<()> {
        let (base, source, root, profile) = fixture()?;
        std::fs::write(
            source.join("notes.txt"),
            "Source notes are not model artifacts",
        )?;
        let mut progress = Vec::new();
        let imported =
            import_profile(&root, &profile, &source, &AtomicBool::new(false), |value| {
                progress.push(value)
            })?;
        assert_eq!(imported.profile.license, profile.license);
        assert_eq!(imported.profile.files[0].sha256, profile.files[0].sha256);
        for file in &profile.files {
            assert_eq!(
                std::fs::read(imported.directory.join(&file.name))?,
                std::fs::read(source.join(&file.name))?
            );
        }
        assert_eq!(
            std::fs::read_to_string(source.join("notes.txt"))?,
            "Source notes are not model artifacts"
        );
        assert!(!imported.directory.join("notes.txt").exists());
        assert_eq!(
            progress.last().unwrap().downloaded_bytes,
            profile.files.last().unwrap().bytes
        );
        assert_eq!(
            std::fs::read_dir(&imported.directory)?.count(),
            profile.files.len() + 1
        );
        import_profile(&root, &profile, &source, &AtomicBool::new(false), |_| {})?;
        std::fs::remove_dir_all(base)?;
        Ok(())
    }

    #[test]
    fn a_bad_source_preserves_valid_destination_and_no_partial_profile_is_published() -> Result<()>
    {
        let (base, source, root, profile) = fixture()?;
        let installed = import_profile(&root, &profile, &source, &AtomicBool::new(false), |_| {})?;
        let before = std::fs::read(installed.directory.join("LICENSE"))?;
        std::fs::write(
            source.join("LICENSE"),
            vec![b'x'; profile.files[1].bytes as usize],
        )?;
        assert!(import_profile(&root, &profile, &source, &AtomicBool::new(false), |_| {}).is_err());
        assert_eq!(std::fs::read(installed.directory.join("LICENSE"))?, before);
        verify_profile(&installed.directory, &profile, &AtomicBool::new(false))?;
        std::fs::write(
            installed.directory.join("weights.bin"),
            "Old invalid destination is still preserved on failed import",
        )?;
        let old = std::fs::read(installed.directory.join("weights.bin"))?;
        assert!(import_profile(&root, &profile, &source, &AtomicBool::new(false), |_| {}).is_err());
        assert_eq!(std::fs::read(installed.directory.join("weights.bin"))?, old);
        assert_eq!(std::fs::read(installed.directory.join("LICENSE"))?, before);
        assert_eq!(
            std::fs::read_dir(&installed.directory)?.count(),
            profile.files.len() + 1
        );
        std::fs::remove_dir_all(base)?;
        Ok(())
    }

    #[test]
    fn cancelled_copy_cleans_only_its_staging_files_and_preserves_old_data() -> Result<()> {
        let (base, source, root, mut profile) = fixture()?;
        let bytes = vec![b'o'; 300 * 1024];
        std::fs::write(source.join("weights.bin"), &bytes)?;
        profile.files[0].bytes = bytes.len() as u64;
        profile.files[0].sha256 = format!("{:x}", Sha256::digest(&bytes));
        let (directory, lock) = lock_profile(&root, &profile, true)?;
        std::fs::write(directory.join("weights.bin"), "Existing data")?;
        drop(lock);
        let cancel = AtomicBool::new(false);
        assert!(
            import_profile(&root, &profile, &source, &cancel, |_| cancel
                .store(true, Ordering::Relaxed))
            .is_err()
        );
        assert_eq!(
            std::fs::read_to_string(directory.join("weights.bin"))?,
            "Existing data"
        );
        assert!(!directory.join("LICENSE").exists());
        assert_eq!(std::fs::read_dir(&directory)?.count(), 2);
        assert!(import_profile(&root, &profile, &source, &AtomicBool::new(true), |_| {}).is_err());
        std::fs::remove_dir_all(base)?;
        Ok(())
    }

    #[test]
    fn source_equals_destination_and_locked_profiles_are_refused_without_writes() -> Result<()> {
        let (base, source, root, profile) = fixture()?;
        let directory = root.join(&profile.id);
        std::fs::create_dir_all(&directory)?;
        for file in &profile.files {
            std::fs::copy(source.join(&file.name), directory.join(&file.name))?;
        }
        assert!(
            import_profile(
                &root,
                &profile,
                &directory.join("."),
                &AtomicBool::new(false),
                |_| {}
            )
            .is_err()
        );
        assert_eq!(std::fs::read_dir(&directory)?.count(), profile.files.len());
        let (_, lock) = lock_profile(&root, &profile, false)?;
        assert!(import_profile(&root, &profile, &source, &AtomicBool::new(false), |_| {}).is_err());
        assert!(remove_profile(&root, &profile).is_err());
        verify_profile(&directory, &profile, &AtomicBool::new(false))?;
        drop(lock);
        std::fs::remove_dir_all(base)?;
        Ok(())
    }

    #[test]
    fn removal_preflights_unknown_files_and_preserves_the_lock_inode() -> Result<()> {
        let (base, source, root, profile) = fixture()?;
        let installed = import_profile(&root, &profile, &source, &AtomicBool::new(false), |_| {})?;
        let directory = &installed.directory;
        std::fs::write(directory.join("weights.bin.part"), "Partial fixture bytes")?;
        std::fs::write(directory.join("human-notes.txt"), "Never delete this")?;
        assert!(remove_profile(&root, &profile).is_err());
        assert_eq!(
            std::fs::read_to_string(directory.join("human-notes.txt"))?,
            "Never delete this"
        );
        assert!(
            directory.join("weights.bin").exists()
                && directory.join("LICENSE").exists()
                && directory.join("weights.bin.part").exists()
        );
        std::fs::remove_file(directory.join("human-notes.txt"))?;
        let old_lock_handle = private_options().open(directory.join(".install.lock"))?;
        remove_profile(&root, &profile)?;
        assert_eq!(std::fs::read_dir(directory)?.count(), 1);
        assert!(directory.join(".install.lock").is_file());
        old_lock_handle.try_lock()?;
        assert!(
            lock_profile(&root, &profile, false).is_err(),
            "removal must not replace the coordination inode"
        );
        drop(old_lock_handle);
        import_profile(&root, &profile, &source, &AtomicBool::new(false), |_| {})?;
        std::fs::remove_dir_all(base)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn source_files_profile_directories_and_partial_symlinks_are_rejected() -> Result<()> {
        use std::os::unix::fs::symlink;
        let (base, source, root, profile) = fixture()?;
        let weights = source.join("weights.bin");
        std::fs::rename(&weights, source.join("real-weights"))?;
        symlink(source.join("real-weights"), &weights)?;
        assert!(import_profile(&root, &profile, &source, &AtomicBool::new(false), |_| {}).is_err());
        assert!(!root.join(&profile.id).join("weights.bin").exists());
        std::fs::remove_file(&weights)?;
        std::fs::rename(source.join("real-weights"), &weights)?;
        let alias = base.join("source-alias");
        symlink(&source, &alias)?;
        assert!(import_profile(&root, &profile, &alias, &AtomicBool::new(false), |_| {}).is_err());
        let directory = root.join(&profile.id);
        std::fs::remove_dir_all(&directory)?;
        symlink(&source, &directory)?;
        assert!(import_profile(&root, &profile, &source, &AtomicBool::new(false), |_| {}).is_err());
        assert!(remove_profile(&root, &profile).is_err());
        assert_eq!(
            std::fs::read(&weights)?,
            b"Original synthetic model fixture bytes."
        );
        std::fs::remove_file(&directory)?;
        let installed = import_profile(&root, &profile, &source, &AtomicBool::new(false), |_| {})?;
        symlink(&weights, installed.directory.join("weights.bin.part"))?;
        assert!(remove_profile(&root, &profile).is_err());
        verify_profile(&installed.directory, &profile, &AtomicBool::new(false))?;
        std::fs::remove_dir_all(base)?;
        Ok(())
    }

    #[test]
    fn malformed_fixture_paths_are_rejected_before_touching_the_destination() -> Result<()> {
        let (base, source, root, mut profile) = fixture()?;
        profile.files[0].name = "../outside".to_owned();
        assert!(import_profile(&root, &profile, &source, &AtomicBool::new(false), |_| {}).is_err());
        assert!(remove_profile(&root, &profile).is_err());
        assert!(!root.exists());
        std::fs::remove_dir_all(base)?;
        Ok(())
    }

    #[test]
    fn shipped_manifest_is_pinned_and_rejects_path_traversal() -> Result<()> {
        assert_eq!(profiles()?.len(), 4);
        assert_eq!(
            profile(SMALL_GENERATION_PROFILE)?.files[0].bytes,
            491_400_032
        );
        assert_eq!(profile(GENERATION_PROFILE)?.files[0].bytes, 1_117_320_736);
        for path in ["../model", "/model", "x/y", "x\\y", "..", ".hidden"] {
            assert!(safe_name(path).is_err());
        }
        assert!(profile("unknown").is_err());
        Ok(())
    }

    #[test]
    fn verified_files_are_size_and_content_checked() -> Result<()> {
        let path = std::env::temp_dir().join(format!("cueglint-model-{}", uuid::Uuid::new_v4()));
        let mut expected = ModelFile {
            name: "sample".to_owned(),
            repository: "test/model".to_owned(),
            revision: "0".repeat(40),
            sha256: format!("{:x}", Sha256::digest(b"valid")),
            bytes: 5,
        };
        std::fs::write(&path, b"valid")?;
        verify_file(&path, &expected, &AtomicBool::new(false))?;
        assert!(verify_file(&path, &expected, &AtomicBool::new(true)).is_err());
        std::fs::write(&path, b"wrong")?;
        assert!(verify_file(&path, &expected, &AtomicBool::new(false)).is_err());
        expected.bytes = 6;
        assert!(verify_file(&path, &expected, &AtomicBool::new(false)).is_err());
        std::fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn downloads_resume_a_range_and_publish_only_verified_bytes() -> Result<()> {
        use std::net::TcpListener;
        let directory =
            std::env::temp_dir().join(format!("cueglint-download-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory)?;
        let expected = ModelFile {
            name: "sample.bin".to_owned(),
            repository: "test/model".to_owned(),
            revision: "0".repeat(40),
            sha256: format!("{:x}", Sha256::digest(b"abcdef")),
            bytes: 6,
        };
        std::fs::write(directory.join("sample.bin.part"), b"abc")?;
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = [0u8; 4096];
            let length = stream.read(&mut bytes).unwrap();
            let request = String::from_utf8_lossy(&bytes[..length]).to_lowercase();
            assert!(request.contains("range: bytes=3-"));
            stream.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Length: 3\r\nContent-Range: bytes 3-5/6\r\nConnection: close\r\n\r\ndef").unwrap();
        });
        download_file(
            &ureq::Agent::new_with_defaults(),
            &format!("http://{address}/model"),
            &directory,
            "test",
            &expected,
            &AtomicBool::new(false),
            &mut |_| {},
        )?;
        server.join().unwrap();
        assert_eq!(std::fs::read(directory.join("sample.bin"))?, b"abcdef");
        assert!(!directory.join("sample.bin.part").exists());
        std::fs::remove_dir_all(directory)?;
        Ok(())
    }
}
