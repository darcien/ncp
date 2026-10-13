//! The project file: a `.mcp.json` that a repository gives. The user did
//! not write it, and a commit can change it. Thus it has no effect until
//! the user trusts its exact bytes at its path, as in the paranoid mode of
//! mise.
//!
//! Pure functions:
//! - the search over the folder entries that the caller gives
//! - the trust state from the records
//! - the gate of a server name
//! - the entries to show before a trust
//!
//! The I/O is in `cli::project_file`.

use std::{
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use sha2::{Digest as _, Sha256};

use crate::config::UnexpandedEntry;

/// The name of the project file. Claude Code and VS Code read it.
pub const FILE_NAME: &str = ".mcp.json";

/// The largest project file that `ncp` reads. An `mcpServers` map is small;
/// the limit stops a read of a large file that a folder can hold.
pub const MAX_BYTES: u64 = 1 << 20;

/// Where the search of the project file ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Search {
    /// The project root: the folder that holds `.mcp.json`.
    Found(PathBuf),
    /// No project file. The folder where the search stopped.
    Stopped(PathBuf),
}

/// Searches from `start` up for the folder with `.mcp.json`. The search
/// stops after a folder with `.git`, at the home folder, and at the root
/// folder. It never reads `~/.mcp.json`: a file there would apply to each
/// folder below it. `has` tells if a folder has an entry of that name. It
/// reads only the folder entries, never the content of a `.git` file,
/// which a downloaded folder can forge.
pub fn search(start: &Path, home: Option<&Path>, has: impl Fn(&Path, &str) -> bool) -> Search {
    let mut last = start;
    for folder in start.ancestors() {
        if Some(folder) == home {
            return Search::Stopped(folder.to_path_buf());
        }
        if has(folder, FILE_NAME) {
            return Search::Found(folder.to_path_buf());
        }
        if has(folder, ".git") {
            return Search::Stopped(folder.to_path_buf());
        }
        last = folder;
    }
    Search::Stopped(last.to_path_buf())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trust {
    Trusted,
    /// `changed`: a record exists for other bytes at the same path.
    Untrusted {
        changed: bool,
    },
    /// The user said no. `ncp` does not ask again until `ncp trust`.
    Ignored,
}

impl Trust {
    /// The state from the records of the path. The ignored record wins,
    /// so that a change of the file does not ask again.
    #[must_use]
    pub fn from_records(digest: &str, trusted: Option<&str>, ignored: bool) -> Self {
        match (ignored, trusted) {
            (true, _) => Self::Ignored,
            (false, Some(record)) if record.trim() == digest => Self::Trusted,
            (false, record) => Self::Untrusted {
                changed: record.is_some(),
            },
        }
    }

    /// The word of the output, with the words of `ncp trust`.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Trusted => "trusted",
            Self::Untrusted { .. } => "untrusted",
            Self::Ignored => "ignored",
        }
    }
}

/// The SHA-256 of the bytes, in lowercase hex.
#[must_use]
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// The file name of the records of a project file: the digest of its path,
/// so that the name holds no part of the path.
#[must_use]
pub fn record_name(file: &Path) -> String {
    digest(file.as_os_str().as_bytes())
}

/// A project file that `ncp` read, with the state of its trust.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// The path where the search found the file. `ncp` does not resolve a
    /// symlink, so that a file linked into two folders asks in each folder.
    pub file: PathBuf,
    /// The text that `ncp` read one time. It shows, hashes and runs this
    /// one copy.
    pub text: String,
    pub digest: String,
    pub trust: Trust,
    /// The entries with no expansion, for the user to check.
    pub entries: Vec<UnexpandedEntry>,
}

impl Project {
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.entries.iter().any(|entry| entry.name == name)
    }
}

/// The gate stops a name that is a key of a project file that is not
/// trusted. An ignored file stops only a name that the global file does
/// not have, because then the global entry runs. A name that the project
/// file does not have passes.
#[must_use]
pub fn stops(project: &Project, name: &str, in_global: bool) -> bool {
    project.has(name)
        && match project.trust {
            Trust::Trusted => false,
            Trust::Untrusted { .. } => true,
            Trust::Ignored => !in_global,
        }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    /// A fake tree: the paths that exist.
    fn tree(paths: &[&str]) -> impl Fn(&Path, &str) -> bool {
        let set: HashSet<PathBuf> = paths.iter().map(PathBuf::from).collect();
        move |folder, name| set.contains(&folder.join(name))
    }

    #[test]
    fn finds_the_file_in_the_current_folder_or_a_parent() {
        let has = tree(&["/w/app/.mcp.json", "/w/app/.git"]);
        let home = Some(Path::new("/home/u"));
        assert_eq!(
            search(Path::new("/w/app"), home, &has),
            Search::Found("/w/app".into())
        );
        assert_eq!(
            search(Path::new("/w/app/src/lib"), home, &has),
            Search::Found("/w/app".into())
        );
    }

    #[test]
    fn stops_after_git() {
        let has = tree(&["/w/.mcp.json", "/w/app/.git"]);
        assert_eq!(
            search(Path::new("/w/app/src"), None, &has),
            Search::Stopped("/w/app".into())
        );
    }

    #[test]
    fn stops_at_the_home_folder_and_does_not_read_its_file() {
        let has = tree(&["/home/u/.mcp.json", "/home/.mcp.json"]);
        let home = Some(Path::new("/home/u"));
        assert_eq!(
            search(Path::new("/home/u/p"), home, &has),
            Search::Stopped("/home/u".into())
        );
    }

    #[test]
    fn stops_at_the_root_folder() {
        assert_eq!(
            search(Path::new("/a/b"), None, tree(&[])),
            Search::Stopped("/".into())
        );
    }

    #[test]
    fn trust_comes_from_the_records_and_the_ignored_record_wins() {
        assert_eq!(
            Trust::from_records("d1", Some("d1\n"), false),
            Trust::Trusted
        );
        assert_eq!(
            Trust::from_records("d1", Some("d0"), false),
            Trust::Untrusted { changed: true }
        );
        assert_eq!(
            Trust::from_records("d1", None, false),
            Trust::Untrusted { changed: false }
        );
        assert_eq!(Trust::from_records("d1", Some("d1"), true), Trust::Ignored);
    }

    #[test]
    fn record_names_differ_by_path_and_hold_no_part_of_it() {
        let one = record_name(Path::new("/w/app/.mcp.json"));
        assert_eq!(one.len(), 64);
        assert!(!one.contains("app"));
        assert_ne!(one, record_name(Path::new("/w/other/.mcp.json")));
    }

    fn project(trust: Trust) -> Project {
        let text = r#"{"mcpServers": {"fs": {"command": "x"}}}"#;
        Project {
            file: "/w/.mcp.json".into(),
            text: text.into(),
            digest: String::new(),
            trust,
            entries: crate::config::unexpanded_entries(text).unwrap(),
        }
    }

    #[test]
    fn gate_stops_only_a_name_of_a_project_file_that_is_not_trusted() {
        let untrusted = project(Trust::Untrusted { changed: false });
        assert!(!stops(&untrusted, "other", true));
        assert!(stops(&untrusted, "fs", true));
        assert!(stops(&untrusted, "fs", false));
        assert!(!stops(&project(Trust::Trusted), "fs", false));
    }

    #[test]
    fn an_ignored_file_stops_a_name_only_when_the_global_file_lacks_it() {
        let ignored = project(Trust::Ignored);
        assert!(stops(&ignored, "fs", false));
        assert!(!stops(&ignored, "fs", true));
    }
}
