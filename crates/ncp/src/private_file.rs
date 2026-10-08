//! Files that only the user can open: mode `0600`, in a directory with mode
//! `0700` when `ncp` makes the directory.
//!
//! A write goes to a new file that replaces the old file, so a failed write
//! cannot leave a partial file.

use std::{
    fs::{self, DirBuilder, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::Path,
};

/// Writes the file. Makes the missing parent directories with mode `0700`.
///
/// # Errors
///
/// Returns an error when a directory or the file cannot be written.
pub fn write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .and_then(|mut file| {
            file.write_all(bytes)?;
            file.sync_all()
        })
        .and_then(|()| fs::rename(&temporary, path))
        .inspect_err(|_| {
            let _ = fs::remove_file(&temporary);
        })
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn writes_file_with_mode_0600_in_new_directory_with_mode_0700() {
        let dir = std::env::temp_dir().join(format!("ncp-private-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("sub").join("file.json");
        write(&path, b"one").unwrap();
        write(&path, b"two").unwrap();
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(fs::read_to_string(&path).unwrap(), "two");
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(path.parent().unwrap()), 0o700);
        let names: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["file.json"]);
        fs::remove_dir_all(dir).unwrap();
    }
}
