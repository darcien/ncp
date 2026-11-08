//! The word in the place of a server name. It can also be a joined
//! `server.tool` word, a URL, or the key of an entry from another client.
//! These pure functions decide what a command does with a word that names
//! no known server.

use super::{
    near,
    outcome::{Failure, Hint},
    project_file::ProjectFile,
    render::shell_word,
    session::Loaded,
    tool_args,
};
use crate::{
    project::Trust,
    text::{code, path_code},
};

/// The word in the place of a server name, and the command that reads it.
#[derive(Debug, Clone, Copy)]
pub struct Word<'a> {
    pub text: &'a str,
    pub kind: Kind<'a>,
}

impl<'a> Word<'a> {
    /// The word of a command that takes no name after the server.
    #[must_use]
    pub fn server(text: &'a str) -> Self {
        Self {
            text,
            kind: Kind::Other,
        }
    }
}

/// The command that reads the word. A command that takes a name after the
/// server carries the next word, because that word decides if a joined
/// word runs.
#[derive(Debug, Clone, Copy)]
pub enum Kind<'a> {
    /// `tool show`, with the tool word.
    ToolShow(Option<&'a str>),
    /// `tool call`, with the word after the server word.
    ToolCall(Option<&'a str>),
    /// `prompt get`, with the prompt word.
    PromptGet(Option<&'a str>),
    Other,
}

/// What a command does with a word that names no known server.
#[derive(Debug)]
pub enum Resolution<'w> {
    /// Run the two parts of a joined word as two words.
    Joined {
        server: &'w str,
        name: &'w str,
    },
    Failed(Failure),
}

/// A word that starts with `http://` or `https://`, in any case.
#[must_use]
pub fn is_url(word: &str) -> bool {
    let lower = word.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// A word that joins a server name and a second name, for example
/// `github.search`. A server name cannot contain `.`, `:` or `/`, so the
/// first of them splits the word. A URL is never a joined word.
#[must_use]
pub fn split_joined(word: &str) -> Option<(&str, &str)> {
    if is_url(word) {
        return None;
    }
    let index = word.find(['.', ':', '/'])?;
    let (server, name) = (&word[..index], &word[index + 1..]);
    (!server.is_empty() && !name.is_empty()).then_some((server, name))
}

/// Decides on a word that the lookup did not find.
#[must_use]
pub fn resolve<'w>(word: Word<'w>, loaded: &Loaded) -> Resolution<'w> {
    // A server name near a URL is not the server of that URL, so a URL gets
    // no nearest name.
    if is_url(word.text) {
        return Resolution::Failed(not_configured(word.text, loaded, None));
    }
    match split_joined(word.text).filter(|(server, _)| loaded.knows(server)) {
        Some((server, name)) if runs_joined(word.kind) => Resolution::Joined { server, name },
        Some((server, _)) => Resolution::Failed(match second_name(word.kind) {
            Some(what) => joined_failure(word.text, server, what),
            None => not_configured(word.text, loaded, did_you_mean(Some(server))),
        }),
        None => {
            let near = near::nearest(word.text, loaded.known_names());
            Resolution::Failed(not_configured(word.text, loaded, did_you_mean(near)))
        }
    }
}

// The next word tells that the second part of the word is the tool name
// or the prompt name. With another next word, ncp cannot know which name
// the user wants, because that word can also be a name.
fn runs_joined(kind: Kind<'_>) -> bool {
    match kind {
        Kind::ToolShow(next) => next.is_none(),
        Kind::ToolCall(next) => next.is_none_or(tool_args::is_argument),
        Kind::PromptGet(next) => next.is_none_or(|next| next.contains('=')),
        Kind::Other => false,
    }
}

// The kind of name that a command takes after the server.
fn second_name(kind: Kind<'_>) -> Option<&'static str> {
    match kind {
        Kind::ToolShow(_) | Kind::ToolCall(_) => Some("tool"),
        Kind::PromptGet(_) => Some("prompt"),
        Kind::Other => None,
    }
}

fn joined_failure(word: &str, server: &str, what: &str) -> Failure {
    Failure::usage(format!(
        "{} joins a server name and a {what} name",
        code(word)
    ))
    .hint(list_hint(what, server))
}

/// The hint to list the names of one kind on a server, for example the tools.
#[must_use]
pub fn list_hint(what: &str, server: &str) -> Hint {
    Hint::run(
        &format!("see the {what}s of the server"),
        format!("ncp {what} list {}", shell_word(server)),
    )
}

// The failure for a name that no file has. The first hint replaces the
// nearest name.
fn not_configured(word: &str, loaded: &Loaded, first: Option<Hint>) -> Failure {
    let place = match (&loaded.project, loaded.exists) {
        (ProjectFile::Read(project), _) if project.trust == Trust::Trusted => format!(
            " in {} or {}",
            path_code(&loaded.path),
            path_code(&project.file)
        ),
        (_, true) => format!(" in {}", path_code(&loaded.path)),
        (_, false) => format!(", because {} does not exist", path_code(&loaded.path)),
    };
    Failure::usage(format!("No server {} is configured{place}", code(word)))
        .with_hints(first)
        .hint(Hint::run("see the configured servers", "ncp server list"))
}

/// The hint "Did you mean ...?" for a near name, or `None` with no name.
#[must_use]
pub fn did_you_mean(near: Option<&str>) -> Option<Hint> {
    near::did_you_mean(near).map(Hint::text)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::{config, expand::Env};

    fn global() -> Loaded {
        let config = config::parse(
            r#"{"mcpServers": {
                "loc": {"type": "http", "url": "http://127.0.0.1:9/mcp"},
                "io.github.github/github-mcp-server": {"command": "gh"},
                "fx": {"command": "fx"}
            }}"#,
            &Env::new(),
        )
        .unwrap();
        Loaded {
            config,
            path: PathBuf::from("/c/mcp.json"),
            exists: true,
            project: ProjectFile::Absent(PathBuf::from("/p")),
        }
    }

    fn word(text: &'static str, kind: Kind<'static>) -> Word<'static> {
        Word { text, kind }
    }

    fn failed(resolution: Resolution<'_>) -> Failure {
        match resolution {
            Resolution::Failed(failure) => failure,
            Resolution::Joined { server, name } => panic!("joined: {server} {name}"),
        }
    }

    fn hints(failure: &Failure) -> Vec<&str> {
        failure.hints().iter().map(Hint::message).collect()
    }

    #[test]
    fn split_joined_splits_at_the_first_separator_and_never_a_url() {
        assert!(is_url("HTTPS://x.example/mcp"));
        assert!(is_url("http://127.0.0.1:9/mcp"));
        assert!(!is_url("ftp://x"));
        assert_eq!(split_joined("https://x.example/mcp"), None);
        assert_eq!(split_joined("HTTP://x.example/mcp"), None);
        assert_eq!(split_joined("github.search"), Some(("github", "search")));
        assert_eq!(split_joined("github:a.b"), Some(("github", "a.b")));
        assert_eq!(split_joined("github/search"), Some(("github", "search")));
        assert_eq!(split_joined("github"), None);
        assert_eq!(split_joined(".x"), None);
        assert_eq!(split_joined("x."), None);
    }

    #[test]
    fn a_joined_word_runs_only_when_the_next_word_is_missing_or_an_argument() {
        let runs = [
            Kind::ToolShow(None),
            Kind::ToolCall(None),
            Kind::ToolCall(Some("q=x")),
            Kind::ToolCall(Some("{}")),
            Kind::ToolCall(Some("-")),
            Kind::PromptGet(None),
            Kind::PromptGet(Some("x=1")),
        ];
        for kind in runs {
            assert!(
                matches!(
                    resolve(word("fx.search", kind), &global()),
                    Resolution::Joined {
                        server: "fx",
                        name: "search"
                    }
                ),
                "{kind:?}"
            );
        }
        for (kind, what) in [
            (Kind::ToolShow(Some("extra")), "tool"),
            (Kind::ToolCall(Some("query")), "tool"),
            (Kind::PromptGet(Some("x")), "prompt"),
            (Kind::PromptGet(Some("-")), "prompt"),
            (Kind::PromptGet(Some(r#"{"a":1}"#)), "prompt"),
        ] {
            let failure = failed(resolve(word("fx.search", kind), &global()));
            assert_eq!(
                failure.message(),
                format!("`fx.search` joins a server name and a {what} name")
            );
            assert_eq!(
                failure
                    .hints()
                    .iter()
                    .map(Hint::command)
                    .collect::<Vec<_>>(),
                [Some(format!("ncp {what} list fx").as_str())]
            );
        }
    }

    #[test]
    fn a_joined_word_of_another_command_names_the_server_part() {
        let failure = failed(resolve(word("fx.search", Kind::Other), &global()));
        assert_eq!(
            failure.message(),
            "No server `fx.search` is configured in `/c/mcp.json`"
        );
        assert_eq!(
            hints(&failure),
            [
                "Did you mean `fx`?",
                "To see the configured servers, run `ncp server list`"
            ]
        );
    }

    #[test]
    fn an_unknown_server_part_is_not_a_joined_word() {
        let failure = failed(resolve(
            word("nope.search", Kind::ToolShow(None)),
            &global(),
        ));
        assert_eq!(
            failure.message(),
            "No server `nope.search` is configured in `/c/mcp.json`"
        );
        assert_eq!(
            hints(&failure),
            ["To see the configured servers, run `ncp server list`"]
        );
    }

    #[test]
    fn a_url_gets_only_the_hint_to_see_the_configured_servers() {
        for text in [
            "http://127.0.0.1:9/mcp",
            "https://x.example/mcp",
            "HTTPS://x.example/mcp",
        ] {
            for kind in [Kind::Other, Kind::ToolCall(None), Kind::PromptGet(None)] {
                let failure = failed(resolve(word(text, kind), &global()));
                assert_eq!(
                    failure.message(),
                    format!("No server `{text}` is configured in `/c/mcp.json`")
                );
                assert_eq!(
                    hints(&failure),
                    ["To see the configured servers, run `ncp server list`"],
                    "{text} {kind:?}"
                );
            }
        }
    }

    #[test]
    fn another_word_gets_the_nearest_known_name() {
        let failure = failed(resolve(word("fy", Kind::Other), &global()));
        assert_eq!(hints(&failure)[0], "Did you mean `fx`?");
    }
}
