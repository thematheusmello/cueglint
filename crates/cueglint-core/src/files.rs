//! Atomic private exports shared by desktop and CLI.
use anyhow::{Context, Result};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

pub fn write_private_export(path: &Path, text: &str, replace: bool) -> Result<()> {
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
        file.write_all(text.as_bytes())?;
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
        fs::remove_dir_all(dir).unwrap();
    }
}
