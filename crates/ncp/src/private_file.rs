//! Files that only the user can open: mode `0600`, in a directory with mode
//! `0700` when `ncp` makes the directory.
//!
//! A write goes to a new file that replaces the old file, so a failed write
//! cannot leave a partial file.

use std::{
    fs::{self, DirBuilder, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::Path,
};

/// Makes the missing parent directories with mode `0700`.
///
/// # Errors
///
/// Returns an error when `ncp` cannot write a directory or the file.
pub fn write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;
    replace(path, bytes, 0o600, || Ok(()))
}

/// Replaces the file with a new file of the mode, in the same folder, so
/// that a failed write cannot leave a partial file. `before_rename` runs
/// last, for example to check that nobody changed the old file; its error
/// stops the rename.
///
/// # Errors
///
/// Returns the error of the write, of the rename, or of `before_rename`.
pub fn replace<E: From<std::io::Error>>(
    path: &Path,
    bytes: &[u8],
    mode: u32,
    before_rename: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temporary = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let result = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&temporary)
        .and_then(|mut file| {
            file.write_all(bytes)?;
            // The mode exactly, also with a umask.
            file.set_permissions(fs::Permissions::from_mode(mode))?;
            file.sync_all()
        })
        .map_err(E::from)
        .and_then(|()| before_rename())
        .and_then(|()| fs::rename(&temporary, path).map_err(E::from));
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
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

/// An exclusive lock on a lock file. The lock ends at the drop of the value,
/// also when the process stops.
#[derive(Debug)]
pub struct Lock {
    _file: fs::File,
}

/// The time between two tries to get a lock.
const LOCK_STEP: std::time::Duration = std::time::Duration::from_millis(20);

/// Gets an exclusive lock on the lock file, and waits for it at most the
/// limit. The lock file is empty, with mode `0600`, in a directory with mode
/// `0700`. The call blocks: an async caller runs it on a blocking thread.
///
/// # Errors
///
/// Returns an error when `ncp` cannot open the file, or when another process
/// holds the lock for longer than the limit.
pub fn lock(path: &Path, limit: std::time::Duration) -> std::io::Result<Lock> {
    let parent = path.parent().unwrap_or(Path::new("."));
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)?;
    let deadline = std::time::Instant::now() + limit;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(Lock { _file: file }),
            Err(fs::TryLockError::WouldBlock) if std::time::Instant::now() < deadline => {
                std::thread::sleep(LOCK_STEP);
            }
            Err(fs::TryLockError::WouldBlock) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!(
                        "another ncp process holds {} for more than {} s",
                        path.display(),
                        limit.as_secs()
                    ),
                ));
            }
            Err(fs::TryLockError::Error(error)) => return Err(error),
        }
    }
}

#[cfg(test)]
mod lock_tests {
    use super::*;

    #[test]
    fn a_second_lock_waits_and_then_fails_at_its_limit() {
        let dir = std::env::temp_dir().join(format!("ncp-lock-{}", std::process::id()));
        let path = dir.join("test.lock");
        let held = lock(&path, std::time::Duration::from_secs(1)).unwrap();
        let error = lock(&path, std::time::Duration::from_millis(100)).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        drop(held);
        assert!(lock(&path, std::time::Duration::from_millis(100)).is_ok());
        let _ = fs::remove_dir_all(dir);
    }
}
