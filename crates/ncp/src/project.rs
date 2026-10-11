//! The project file: a `.mcp.json` that a repository gives. The user did
//! not write it, and a commit can change it, so it has no effect until the
//! user trusts its exact bytes at its path, as the paranoid mode of mise.
//!
//! Pure functions: the search over the folder entries that the caller
//! gives, the trust state from the records, the gate of a server name, and
//! the entries to show before a trust. The I/O is in `cli::trust`.

use std::{
    collections::BTreeSet,
    fmt::Write as _,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use serde_json::{Map, Value};
use sha2::{Digest as _, Sha256};

use crate::expand;

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

/// The trust state of a project file.
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
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
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
    /// The path where the search found the file. A symlink is not resolved,
    /// so that a file linked into two folders asks in each folder.
    pub file: PathBuf,
    /// The text that `ncp` read one time. It shows, hashes and runs this
    /// one copy.
    pub text: String,
    pub digest: String,
    pub trust: Trust,
    pub entries: Vec<Entry>,
}

impl Project {
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.entries.iter().any(|entry| entry.name == name)
    }
}

/// One entry of a project file, read from the JSON value with no
/// expansion, for the user to check before the trust.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub kind: EntryKind,
    /// The names of `${NAME}` in the values that can come from a store.
    pub variables: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
    Stdio {
        command: String,
        args: Vec<String>,
        cwd: Option<String>,
        /// The names of the `env` variables. Never their values: a value
        /// can be a literal secret.
        env: Vec<String>,
    },
    Remote {
        transport: &'static str,
        url: String,
        /// Each `${NAME}` of `headers` and `oauth`, with the host that gets
        /// its value.
        sends: Vec<(String, String)>,
    },
    /// Not an entry that `ncp` can start.
    Invalid,
}

impl EntryKind {
    #[must_use]
    pub fn transport(&self) -> &'static str {
        match self {
            Self::Stdio { .. } => "stdio",
            Self::Remote { transport, .. } => transport,
            Self::Invalid => "invalid",
        }
    }
}

/// The entries of the `mcpServers` map.
#[must_use]
pub fn entries(servers: &Map<String, Value>) -> Vec<Entry> {
    servers
        .iter()
        .map(|(name, value)| entry(name, value))
        .collect()
}

fn entry(name: &str, value: &Value) -> Entry {
    let Some(object) = value.as_object() else {
        return Entry {
            name: name.to_string(),
            kind: EntryKind::Invalid,
            variables: BTreeSet::new(),
        };
    };
    let text = |key: &str| object.get(key).and_then(Value::as_str).map(str::to_string);
    let kind = match (text("type").as_deref(), text("url"), text("command")) {
        (Some(transport @ ("http" | "sse")), Some(url), _) => {
            let host = url::Url::parse(&url)
                .ok()
                .and_then(|url| url.host_str().map(str::to_string))
                .unwrap_or_else(|| url.clone());
            let issuer_host = object
                .get("oauth")
                .and_then(|oauth| {
                    oauth
                        .get("issuer")
                        .or_else(|| oauth.get("authorizationServer"))
                })
                .and_then(|issuer| issuer.as_str().or_else(|| issuer.get(0)?.as_str()))
                .and_then(|issuer| url::Url::parse(issuer).ok()?.host_str().map(str::to_string))
                .unwrap_or_else(|| host.clone());
            let headers = names_in(object.get("headers"))
                .into_iter()
                .map(|name| (name, host.clone()));
            let secret = names_in(
                object
                    .get("oauth")
                    .and_then(|oauth| oauth.get("clientSecret")),
            )
            .into_iter()
            .map(|name| (name, issuer_host.clone()));
            EntryKind::Remote {
                transport: if transport == "sse" { "sse" } else { "http" },
                url,
                sends: headers.chain(secret).collect(),
            }
        }
        (Some("stdio") | None, None, Some(command)) => EntryKind::Stdio {
            command,
            args: object
                .get("args")
                .and_then(Value::as_array)
                .map(|args| {
                    args.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            cwd: text("cwd"),
            env: object
                .get("env")
                .and_then(Value::as_object)
                .map(|env| env.keys().cloned().collect())
                .unwrap_or_default(),
        },
        _ => EntryKind::Invalid,
    };
    let variables = match &kind {
        EntryKind::Stdio { .. } => names_in(object.get("env")),
        EntryKind::Remote { sends, .. } => sends.iter().map(|(name, _)| name.clone()).collect(),
        EntryKind::Invalid => BTreeSet::new(),
    };
    Entry {
        name: name.to_string(),
        kind,
        variables,
    }
}

/// The names of `${NAME}` in the string values of a JSON value.
fn names_in(value: Option<&Value>) -> BTreeSet<String> {
    match value {
        Some(Value::String(text)) => expand::names(text)
            .into_iter()
            .map(str::to_string)
            .collect(),
        Some(Value::Object(map)) => map
            .values()
            .flat_map(|value| names_in(Some(value)))
            .collect(),
        _ => BTreeSet::new(),
    }
}

/// Why the gate stops a server name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// The name is a key of a project file that is not trusted. With
    /// `in_global`, the global file has the name too: after a trust the
    /// project entry wins, so the global entry does not run either.
    Untrusted { changed: bool, in_global: bool },
    /// The name is only in a project file that the user ignored.
    Ignored,
}

/// The gate in the lookup of a server name. A name that the project file
/// does not have passes, also when the file is not trusted.
#[must_use]
pub fn gate(project: Option<&Project>, name: &str, in_global: bool) -> Option<Stop> {
    let project = project.filter(|project| project.has(name))?;
    match project.trust {
        Trust::Trusted => None,
        Trust::Untrusted { changed } => Some(Stop::Untrusted { changed, in_global }),
        Trust::Ignored if in_global => None,
        Trust::Ignored => Some(Stop::Ignored),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde_json::json;

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
    fn stops_after_a_git_folder_or_a_git_file() {
        let has = tree(&["/w/.mcp.json", "/w/app/.git"]);
        assert_eq!(
            search(Path::new("/w/app/src"), None, &has),
            Search::Stopped("/w/app".into())
        );
    }

    #[test]
    fn reads_the_file_at_the_git_root() {
        let has = tree(&["/w/app/.mcp.json", "/w/app/.git"]);
        assert_eq!(
            search(Path::new("/w/app"), None, &has),
            Search::Found("/w/app".into())
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

    fn servers(value: &Value) -> Vec<Entry> {
        entries(value.as_object().unwrap())
    }

    #[test]
    fn reads_entries_with_env_names_and_the_host_of_each_variable() {
        let entries = servers(&json!({
            "fs": {"command": "npx", "args": ["-y", "pkg", "."], "cwd": "sub",
                   "env": {"API_KEY": "literal", "TOKEN": "${TOKEN}"}},
            "sentry": {"type": "http", "url": "https://mcp.sentry.dev/mcp",
                       "headers": {"Authorization": "Bearer ${SENTRY_TOKEN}"},
                       "oauth": {"clientId": "c", "clientSecret": "${SECRET}",
                                 "issuer": "https://auth.example.com"}},
            "bad": {"type": "ws"},
            "text": "x"
        }));
        let kind = |name: &str| &entries.iter().find(|e| e.name == name).unwrap().kind;
        assert_eq!(
            *kind("fs"),
            EntryKind::Stdio {
                command: "npx".into(),
                args: vec!["-y".into(), "pkg".into(), ".".into()],
                cwd: Some("sub".into()),
                env: vec!["API_KEY".into(), "TOKEN".into()],
            }
        );
        assert_eq!(entries[1].variables, BTreeSet::from(["TOKEN".to_string()]));
        assert_eq!(
            *kind("sentry"),
            EntryKind::Remote {
                transport: "http",
                url: "https://mcp.sentry.dev/mcp".into(),
                sends: vec![
                    ("SENTRY_TOKEN".into(), "mcp.sentry.dev".into()),
                    ("SECRET".into(), "auth.example.com".into()),
                ],
            }
        );
        assert_eq!(*kind("bad"), EntryKind::Invalid);
        assert_eq!(*kind("text"), EntryKind::Invalid);
    }

    fn project(trust: Trust) -> Project {
        Project {
            file: "/w/.mcp.json".into(),
            text: String::new(),
            digest: String::new(),
            trust,
            entries: servers(&json!({"fs": {"command": "x"}})),
        }
    }

    #[test]
    fn gate_stops_only_a_name_of_a_project_file_that_is_not_trusted() {
        let untrusted = project(Trust::Untrusted { changed: false });
        assert_eq!(gate(Some(&untrusted), "other", true), None);
        assert_eq!(
            gate(Some(&untrusted), "fs", true),
            Some(Stop::Untrusted {
                changed: false,
                in_global: true
            })
        );
        assert_eq!(gate(Some(&project(Trust::Trusted)), "fs", false), None);
        assert_eq!(gate(None, "fs", false), None);
    }

    #[test]
    fn an_ignored_file_stops_a_name_only_when_the_global_file_lacks_it() {
        let ignored = project(Trust::Ignored);
        assert_eq!(gate(Some(&ignored), "fs", false), Some(Stop::Ignored));
        assert_eq!(gate(Some(&ignored), "fs", true), None);
    }
}
