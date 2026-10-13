//! The project file at the edge: the search on the disk, the one read with
//! the owner check, and the trust records. The rules are in
//! `crate::project` and in docs/design/project-config.md.

use std::{
    fs::OpenOptions,
    io::{ErrorKind, Read as _},
    os::unix::fs::{MetadataExt as _, OpenOptionsExt as _},
    path::{Path, PathBuf},
};

use nix::fcntl::OFlag;

use super::outcome::Failure;
use crate::{
    config,
    expand::Env,
    paths::Paths,
    private_file,
    project::{self, Project, Search, Trust},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectFile {
    /// `--config-file` names the file, so there is no search.
    NotSearched,
    /// No project file. The folder where the search stopped.
    Absent(PathBuf),
    /// A file that `ncp` does not use, with the reason. `ncp` does not
    /// search higher, as git does when it finds a repository of another
    /// user.
    Unusable {
        file: PathBuf,
        reason: String,
    },
    Read(Project),
}

impl ProjectFile {
    #[must_use]
    pub fn project(&self) -> Option<&Project> {
        match self {
            Self::Read(project) => Some(project),
            _ => None,
        }
    }
}

/// Searches from the real path of the current folder, and reads the file
/// that the search finds. With `search` false (`--config-file`), there is
/// no project file.
#[must_use]
pub fn find(paths: &Paths, env: &Env, search: bool) -> ProjectFile {
    if !search {
        return ProjectFile::NotSearched;
    }
    let Ok(start) = std::env::current_dir().and_then(std::fs::canonicalize) else {
        return ProjectFile::Absent(PathBuf::from("."));
    };
    let home = env
        .get("HOME")
        .map(|home| std::fs::canonicalize(home).unwrap_or_else(|_| PathBuf::from(home)));
    let has = |folder: &Path, name: &str| folder.join(name).symlink_metadata().is_ok();
    match project::search(&start, home.as_deref(), has) {
        Search::Stopped(folder) => ProjectFile::Absent(folder),
        Search::Found(root) => {
            let file = root.join(project::FILE_NAME);
            let read = read_owned(&file).and_then(|text| {
                config::unexpanded_entries(&text)
                    .map(|entries| (entries, text))
                    .map_err(|error| error.to_string())
            });
            match read {
                Ok((entries, text)) => {
                    let digest = project::digest(text.as_bytes());
                    ProjectFile::Read(Project {
                        trust: trust_state(paths, &file, &digest),
                        file,
                        text,
                        digest,
                        entries,
                    })
                }
                Err(reason) => ProjectFile::Unusable { file, reason },
            }
        }
    }
}

// Reads the file one time, and checks the owner on the same open file.
// For a symlink, the check is on the target. Another user cannot make a
// file that the user owns (git CVE-2022-24765).
fn read_owned(file: &Path) -> Result<String, String> {
    // With no O_NONBLOCK, the open of a FIFO waits for a writer with no
    // limit, also in shell completion. The check of the open file follows.
    let handle = OpenOptions::new()
        .read(true)
        .custom_flags(OFlag::O_NONBLOCK.bits())
        .open(file)
        .map_err(|error| error.to_string())?;
    let metadata = handle.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("it is not a regular file".into());
    }
    if metadata.uid() != nix::unistd::geteuid().as_raw() {
        return Err("another user owns it".into());
    }
    let mut bytes = Vec::new();
    handle
        .take(project::MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > project::MAX_BYTES {
        return Err("it is larger than 1 MiB".into());
    }
    String::from_utf8(bytes).map_err(|_| "it is not UTF-8".into())
}

#[must_use]
pub fn trust_state(paths: &Paths, file: &Path, digest: &str) -> Trust {
    let trusted = std::fs::read_to_string(record(&paths.trusted_configs, file)).ok();
    let ignored = record(&paths.ignored_configs, file).exists();
    Trust::from_records(digest, trusted.as_deref(), ignored)
}

fn record(folder: &Path, file: &Path) -> PathBuf {
    folder.join(project::record_name(file))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Trust the bytes that `ncp` read, and remove an ignored record.
    Trust,
    /// `ncp` does not ask again until `ncp trust`.
    Ignore,
    Untrust,
}

/// Writes the change, and gives the new state.
///
/// # Errors
///
/// Returns a failure when `ncp` cannot write a record.
pub fn change(paths: &Paths, project: &Project, change: Change) -> Result<Trust, Failure> {
    let trusted = record(&paths.trusted_configs, &project.file);
    let ignored = record(&paths.ignored_configs, &project.file);
    match change {
        Change::Trust => {
            private_file::write(&trusted, project.digest.as_bytes()).and_then(|()| remove(&ignored))
        }
        Change::Ignore => private_file::write(&ignored, b""),
        Change::Untrust => remove(&trusted),
    }
    .map_err(|error| record_failure(&error))?;
    Ok(trust_state(paths, &project.file, &project.digest))
}

/// Trusts the bytes that `ncp` wrote, and removes an ignored record of an
/// old file at the path. The user saw the old bytes, or there were none,
/// and gave the change (project-config rule 9).
///
/// # Errors
///
/// Returns a failure when `ncp` cannot write a record.
pub fn trust_written(paths: &Paths, file: &Path, text: &str) -> Result<(), Failure> {
    private_file::write(
        &record(&paths.trusted_configs, file),
        project::digest(text.as_bytes()).as_bytes(),
    )
    .and_then(|()| remove(&record(&paths.ignored_configs, file)))
    .map_err(|error| record_failure(&error))
}

fn record_failure(error: &std::io::Error) -> Failure {
    Failure::protocol(format!("cannot write the trust record: {error}"))
}

fn remove(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}
