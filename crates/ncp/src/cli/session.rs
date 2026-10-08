//! The shared steps of the commands: load the configuration, select a
//! server, connect, and map errors to failures with a fix.

use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
    time::Duration,
};

use rmcp::service::ClientInitializeError;
use tokio_util::sync::CancellationToken;

use super::{
    outcome::{Exit, Failure},
    ui::Ui,
};
use crate::{
    client::{self, ConnectError, Connection, Launch, RequestError, RequestLimits, Secrets},
    config::{self, Config, EntryError, LookupError, Server, Target, Transport},
    era::EraCache,
    era_file,
    expand::{Env, ExpandError},
    paths::Paths,
};

/// The inputs that each command shares.
pub struct Context<'a> {
    pub env: &'a Env,
    pub ui: Ui,
    pub paths: Paths,
    pub request_limit: Duration,
    /// Fires on Ctrl-C.
    pub cancel: CancellationToken,
}

impl Context<'_> {
    #[must_use]
    pub fn limits(&self) -> RequestLimits<'_> {
        RequestLimits {
            cancel: &self.cancel,
            timeout: self.request_limit,
        }
    }
}

/// The configuration and where it comes from.
pub struct Loaded {
    pub config: Config,
    pub path: PathBuf,
    pub exists: bool,
}

/// Reads the configuration file. A missing file gives an empty
/// configuration: `ncp` never makes the file.
///
/// # Errors
///
/// Returns a usage failure when the file does not parse, and a local
/// failure when it cannot be read.
pub fn load_config(path: &Path, env: &Env) -> Result<Loaded, Failure> {
    let (text, exists) = match std::fs::read_to_string(path) {
        Ok(text) => (text, true),
        Err(error) if error.kind() == ErrorKind::NotFound => ("{}".to_string(), false),
        Err(error) => {
            return Err(Failure::protocol(format!(
                "cannot read {}: {error}",
                path.display()
            )));
        }
    };
    let config = config::parse(&text, env)
        .map_err(|error| Failure::usage(format!("{}: {error}", path.display())))?;
    Ok(Loaded {
        config,
        path: path.to_path_buf(),
        exists,
    })
}

/// A word that joins a server name and a tool name, for example
/// `github.search`. A server name cannot contain `.`, `:` or `/`, so the
/// first of them splits the word.
#[must_use]
pub fn split_joined(word: &str) -> Option<(&str, &str)> {
    let index = word.find(['.', ':', '/'])?;
    let (server, tool) = (&word[..index], &word[index + 1..]);
    (!server.is_empty() && !tool.is_empty()).then_some((server, tool))
}

/// The failure for a joined word, with the command to use instead.
#[must_use]
pub fn joined_failure(word: &str, fix: &str) -> Failure {
    Failure::usage(format!(
        "{word:?} joins a server name and a tool name; give them as two words"
    ))
    .hint(fix)
}

/// Selects a server and shows the warnings of its entry.
///
/// # Errors
///
/// Returns a usage failure for an unknown server or an invalid entry.
pub fn select<'a>(loaded: &'a Loaded, name: &str, ui: Ui) -> Result<&'a Server, Failure> {
    let server = loaded.config.server(name).map_err(|error| match error {
        LookupError::NotFound(_) => {
            let place = if loaded.exists {
                format!("in {}", loaded.path.display())
            } else {
                format!("because {} does not exist", loaded.path.display())
            };
            Failure::usage(format!("no server {name:?} is configured {place}"))
                .hint("ncp server list")
        }
        LookupError::Invalid { error, .. } => entry_failure(name, &error, &loaded.path),
    })?;
    if let Transport::Sse(_) = server.transport {
        return Err(Failure::usage(format!(
            "the entry in {} has type \"sse\", and ncp connects only to stdio and http servers",
            loaded.path.display()
        ))
        .server(name));
    }
    loaded
        .config
        .warnings()
        .iter()
        .filter(|warning| warning.server.as_deref() == Some(name))
        .for_each(|warning| ui.warning(Some(name), &unknown_key(&warning.key)));
    Ok(server)
}

#[must_use]
pub fn unknown_key(key: &str) -> String {
    format!("unknown key {key:?} ignored")
}

fn entry_failure(name: &str, error: &EntryError, path: &Path) -> Failure {
    let failure = Failure::usage(format!(
        "the entry in {} is not valid: {error}",
        path.display()
    ))
    .server(name);
    match error {
        EntryError::Variable {
            error: ExpandError::Unset(variable),
            ..
        } => failure.hint(format!("export {variable}=<value>")),
        _ => failure,
    }
}

/// Connects to the server, with the cached era. Saves the era when it
/// changed.
///
/// # Errors
///
/// Returns a failure when the connect fails. An era cache that cannot be
/// read or written gives a warning: `ncp` works the same with no cache.
pub async fn open(context: &Context<'_>, server: &Server) -> Result<Connection, Failure> {
    let name = server.name.as_str();
    let cache_path = &context.paths.era_cache;
    let cache = era_file::load(cache_path).unwrap_or_else(|error| {
        context.ui.warning(None, &error.to_string());
        EraCache::default()
    });
    let target = server.target.clone();
    let cached = cache.lookup(&server.name, &target).cloned();
    let http = client::http::client()
        .map_err(|error| Failure::protocol(error.to_string()).server(name))?;
    let launch = Launch {
        server,
        cached_era: cached.as_ref(),
        base_env: context.env,
        secrets: &Secrets::default(),
        http: &http,
        probe_timeout: client::PROBE_TIMEOUT,
        connect_limit: client::CONNECT_LIMIT,
        server_log: &context.ui.server_log(name),
    };
    let connection = client::connect(&launch)
        .await
        .map_err(|error| connect_failure(server, &error))?;
    if cached.as_ref() != Some(connection.era()) {
        let cache = cache.record(server.name.clone(), target, connection.era().clone());
        if let Err(error) = era_file::save(cache_path, &cache) {
            context.ui.warning(None, &error.to_string());
        }
    }
    Ok(connection)
}

fn connect_failure(server: &Server, error: &ConnectError) -> Failure {
    let exit = if error.needs_authorization() {
        Exit::Auth
    } else {
        Exit::Protocol
    };
    let message = match error {
        _ if error.needs_authorization() => "the server needs a login or more scopes".to_string(),
        ConnectError::Handshake { phase, source } => match source.as_ref() {
            ClientInitializeError::TransportError { error, .. } => {
                format!("{phase} failed: {}", causes(error.error.as_ref()))
            }
            other => format!("{phase} failed: {other}"),
        },
        other => other.to_string(),
    };
    Failure::new(exit, hide_expanded_url(server, message)).server(server.name.as_str())
}

/// The message with the expanded URL replaced by the URL as the file writes
/// it. HTTP errors hold the URL, and a query parameter can hold a key.
fn hide_expanded_url(server: &Server, message: String) -> String {
    match (&server.transport, &server.target) {
        (Transport::Http(remote) | Transport::Sse(remote), Target::Remote { url })
            if remote.url.as_str() != url =>
        {
            message.replace(remote.url.as_str(), url)
        }
        _ => message,
    }
}

/// The error and its causes, with no repeated text. Transport errors wrap
/// each other, and the outer text often holds the inner text.
fn causes(error: &(dyn std::error::Error + 'static)) -> String {
    std::iter::successors(Some(error), |error| error.source())
        .map(ToString::to_string)
        .fold(Vec::<String>::new(), |mut parts, text| {
            if parts.last().is_none_or(|last| !last.contains(&text)) {
                parts.push(text);
            }
            parts
        })
        .join(": ")
}

/// The failure of a request after the connect.
#[must_use]
pub fn request_failure(server: &Server, method: &str, error: &RequestError) -> Failure {
    let failure = match error {
        RequestError::Cancelled => Failure::new(Exit::Interrupted, "interrupted"),
        RequestError::Timeout(_) => Failure::protocol(format!("{method}: {error}"))
            .hint("give a longer limit with --timeout <SECONDS>"),
        _ if error.needs_authorization() => Failure::new(
            Exit::Auth,
            format!("{method}: the server needs a login or more scopes"),
        ),
        RequestError::Service(rmcp::ServiceError::McpError(data)) => Failure::protocol(format!(
            "{method}: the server returned error {}: {}",
            data.code.0, data.message
        )),
        RequestError::Service(rmcp::ServiceError::TransportSend(error)) => {
            Failure::protocol(format!("{method}: {}", causes(error.error.as_ref())))
        }
        _ => Failure::protocol(format!("{method}: {error}")),
    };
    let failure = failure.server(server.name.as_str());
    Failure {
        message: hide_expanded_url(server, failure.message.clone()),
        ..failure
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_joined_server_and_tool_names() {
        assert_eq!(split_joined("github.search"), Some(("github", "search")));
        assert_eq!(split_joined("github:a.b"), Some(("github", "a.b")));
        assert_eq!(split_joined("github/search"), Some(("github", "search")));
        assert_eq!(split_joined("github"), None);
        assert_eq!(split_joined(".x"), None);
        assert_eq!(split_joined("x."), None);
    }

    #[test]
    fn unset_variable_gives_a_hint_to_set_it() {
        let failure = entry_failure(
            "gh",
            &EntryError::Variable {
                field: "env.T".into(),
                error: ExpandError::Unset("TOKEN".into()),
            },
            Path::new("/c/mcp.json"),
        );
        assert_eq!(failure.exit, Exit::Usage);
        assert!(failure.message.contains("/c/mcp.json"), "{failure:?}");
        assert_eq!(failure.hint.as_deref(), Some("export TOKEN=<value>"));
    }

    #[test]
    fn missing_file_gives_an_empty_configuration() {
        let loaded = load_config(Path::new("/nonexistent/ncp/mcp.json"), &Env::new()).unwrap();
        assert!(!loaded.exists);
        assert_eq!(loaded.config.servers().count(), 0);
        let failure = select(
            &loaded,
            "gh",
            Ui {
                format: super::super::ui::Format::Json,
            },
        )
        .unwrap_err();
        assert!(failure.message.contains("does not exist"), "{failure:?}");
        assert_eq!(failure.hint.as_deref(), Some("ncp server list"));
    }
}
