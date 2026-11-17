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
        safe_name(&profile.id)?;
        ensure!(ids.insert(&profile.id), "duplicate model profile");
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
    for file in &profile.files {
        verify_file(&directory.join(&file.name), file, cancel).with_context(|| {
            format!(
                "verify {}; install the {} model profile first",
                file.name, profile.id
            )
        })?;
    }
    Ok(InstalledProfile { profile, directory })
}

/// Resume partial downloads, verify SHA-256, then atomically publish each completed file.
pub fn install(
    root: &Path,
    profile_id: &str,
    cancel: &AtomicBool,
    mut progress: impl FnMut(DownloadProgress),
) -> Result<InstalledProfile> {
    let profile = profile(profile_id)?;
    let directory = root.join(&profile.id);
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(&directory)
        .context("create model directory")?;
    ensure_regular_destination(&directory.join(".install.lock"))?;
    let lock = private_options()
        .create(true)
        .open(directory.join(".install.lock"))?;
    lock.try_lock()
        .context("this model is already being installed by another process")?;
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
    drop(lock);
    verify_with_cancel(root, profile_id, cancel)
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
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "model installer destination must be a regular file, not a link or device"
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
