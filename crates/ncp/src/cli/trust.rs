//! The project file at the edge: the search on the disk, the one read with
//! the owner check, the trust records, and `ncp trust`. The rules are in
//! `crate::project` and in docs/design/project-config.md.

use std::{
    fs::File,
    io::{ErrorKind, Read as _},
    os::unix::fs::MetadataExt as _,
    path::{Path, PathBuf},
};

use serde_json::json;

use super::{
    args::TrustArgs,
    outcome::Failure,
    render,
    session::{self, Context},
};
use crate::{
    config,
    expand::Env,
    paths::Paths,
    private_file,
    project::{self, Project, Search, Trust},
};

/// What the search found.
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
/// that the search finds.
#[must_use]
pub fn find(paths: &Paths, env: &Env) -> ProjectFile {
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
            match read_owned(&file).and_then(|text| {
                config::parse_servers(&text)
                    .map(|servers| (project::entries(&servers), text))
                    .map_err(|error| error.to_string())
            }) {
                Ok((entries, text)) => {
                    let digest = project::digest(text.as_bytes());
                    let trust = trust_state(paths, &file, &digest);
                    ProjectFile::Read(Project {
                        file,
                        text,
                        digest,
                        trust,
                        entries,
                    })
                }
                Err(reason) => ProjectFile::Unusable { file, reason },
            }
        }
    }
}

/// Reads the file one time, and checks the owner on the same open file.
/// For a symlink, the check is on the target. Another user cannot make a
/// file that the user owns (git CVE-2022-24765).
fn read_owned(file: &Path) -> Result<String, String> {
    // A check before the open: an open of a FIFO waits for a writer.
    if !std::fs::metadata(file).is_ok_and(|metadata| metadata.is_file()) {
        return Err("it is not a regular file".into());
    }
    let handle = File::open(file).map_err(|error| error.to_string())?;
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

fn trust_state(paths: &Paths, file: &Path, digest: &str) -> Trust {
    let name = project::record_name(file);
    let trusted = std::fs::read_to_string(paths.trusted_configs.join(&name)).ok();
    let ignored = paths.ignored_configs.join(&name).exists();
    Trust::from_records(digest, trusted.as_deref(), ignored)
}

/// Trusts the bytes that `ncp` read, and removes an ignored record.
///
/// # Errors
///
/// Returns a failure when a record cannot be written.
pub fn trust(paths: &Paths, project: &Project) -> Result<(), Failure> {
    let name = project::record_name(&project.file);
    private_file::write(
        &paths.trusted_configs.join(&name),
        project.digest.as_bytes(),
    )
    .and_then(|()| remove(&paths.ignored_configs.join(&name)))
    .map_err(|error| record_failure(&error))
}

/// Writes the ignored record: `ncp` does not ask again until `ncp trust`.
///
/// # Errors
///
/// Returns a failure when the record cannot be written.
pub fn ignore(paths: &Paths, project: &Project) -> Result<(), Failure> {
    private_file::write(
        &paths
            .ignored_configs
            .join(project::record_name(&project.file)),
        b"",
    )
    .map_err(|error| record_failure(&error))
}

fn untrust(paths: &Paths, project: &Project) -> Result<(), Failure> {
    remove(
        &paths
            .trusted_configs
            .join(project::record_name(&project.file)),
    )
    .map_err(|error| record_failure(&error))
}

fn remove(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

fn record_failure(error: &std::io::Error) -> Failure {
    Failure::protocol(format!("cannot write the trust record: {error}"))
}

/// `ncp trust`: no question and no TTY, as `mise trust`. The user typed
/// the command, so the user means it.
///
/// # Errors
///
/// Returns a usage failure with `--config-file`, with no project file, or
/// with a file that `ncp` does not use; a failure when a record cannot be
/// written.
pub fn command(context: &Context<'_>, args: &TrustArgs) -> Result<(), Failure> {
    let found = if context.named_config {
        ProjectFile::NotSearched
    } else {
        find(&context.paths, context.env)
    };
    let project = match found {
        ProjectFile::Read(project) => project,
        ProjectFile::NotSearched => {
            return Err(Failure::usage(
                "--config-file names the file, so there is no project file to trust",
            ));
        }
        ProjectFile::Absent(folder) => {
            return Err(Failure::usage(format!(
                "no {} from the current folder up to {}",
                project::FILE_NAME,
                folder.display()
            )));
        }
        ProjectFile::Unusable { file, reason } => {
            return Err(Failure::usage(format!(
                "ncp does not use {}: {reason}",
                file.display()
            )));
        }
    };
    let trust = if args.show {
        project.trust
    } else if args.untrust {
        untrust(&context.paths, &project)?;
        match project.trust {
            Trust::Ignored => Trust::Ignored,
            _ => Trust::Untrusted { changed: false },
        }
    } else if args.ignore {
        ignore(&context.paths, &project)?;
        Trust::Ignored
    } else {
        trust(&context.paths, &project)?;
        Trust::Trusted
    };
    let project = Project { trust, ..project };
    context.ui.result(
        || render::project_file(&project),
        || {
            json!({
                "file": project.file,
                "trust": trust.word(),
                "servers": project.entries.iter()
                    .map(|entry| session::project_row(&project, entry))
                    .collect::<Vec<_>>(),
            })
        },
    )
}
