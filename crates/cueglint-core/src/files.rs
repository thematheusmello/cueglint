//! Bounded file reads and atomic private exports shared by desktop and CLI.
use crate::MAX_BACKUP_BYTES;
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

/// Read a bounded UTF-8 backup from a regular file.
pub fn read_backup_file(path: &Path) -> Result<String> {
    let name = path.display();
    ensure!(
        fs::metadata(path)
            .with_context(|| format!("inspect {name}"))?
            .is_file(),
        "{name} must be a regular file"
    );
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // A FIFO could replace the path between metadata and open.
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(path).with_context(|| format!("open {name}"))?;
    let metadata = file.metadata()?;
    ensure!(metadata.is_file(), "{name} must be a regular file");
    ensure!(
        metadata.len() <= MAX_BACKUP_BYTES as u64,
        "{name} exceeds {MAX_BACKUP_BYTES} bytes"
    );
    read_utf8(file, path, MAX_BACKUP_BYTES)
}

/// Read bounded UTF-8 text from a path, including stream paths used by the CLI.
pub fn read_bounded_text(path: &Path, max_bytes: usize) -> Result<String> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    read_utf8(file, path, max_bytes)
}

fn read_utf8(file: File, path: &Path, max_bytes: usize) -> Result<String> {
    let mut bytes = Vec::new();
    file.take((max_bytes as u64).saturating_add(1))
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= max_bytes,
        "{} exceeds {max_bytes} bytes",
        path.display()
    );
    String::from_utf8(bytes).with_context(|| format!("{} must be UTF-8 text", path.display()))
}

pub fn write_private_export(path: &Path, text: &str, replace: bool) -> Result<()> {
    write_private_export_bytes(path, text.as_bytes(), replace)
}

pub fn write_private_export_bytes(path: &Path, bytes: &[u8], replace: bool) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let temp = parent.join(format!(".cueglint-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temp)
            .context("create private temporary export")?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        if replace {
            fs::rename(&temp, path).context("replace export atomically")?;
        } else {
            fs::hard_link(&temp, path)
                .context("create export without overwriting an existing file")?;
        }
        Ok(())
    })();
    let _ = fs::remove_file(temp);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_reader_requires_a_regular_file_with_bounded_utf8() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("cueglint-backup-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&dir)?;
        let path = dir.join("backup.json");
        fs::write(&path, r#"{"notes":"café"}"#)?;
        assert_eq!(read_backup_file(&path)?, r#"{"notes":"café"}"#);
        assert!(read_backup_file(&dir).is_err());
        fs::write(&path, [0xff])?;
        assert!(read_backup_file(&path).is_err());
        OpenOptions::new()
            .write(true)
            .open(&path)?
            .set_len(MAX_BACKUP_BYTES as u64 + 1)?;
        assert!(read_backup_file(&path).is_err());
        fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[test]
    fn exports_are_private_atomic_and_do_not_leave_partial_targets() {
        let dir = std::env::temp_dir().join(format!("cueglint-export-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("notes.md");
        write_private_export(&path, "private meeting", false).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert!(write_private_export(&path, "replacement", false).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "private meeting");
        assert!(write_private_export(&dir, "cannot replace directory", false).is_err());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        let binary = dir.join("frame.png");
        write_private_export_bytes(&binary, &[0, 255, 128, 10], false).unwrap();
        assert!(write_private_export_bytes(&binary, b"replacement", false).is_err());
        assert_eq!(fs::read(&binary).unwrap(), [0, 255, 128, 10]);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
        fs::remove_dir_all(dir).unwrap();
    }
}
