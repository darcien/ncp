//! The shared steps of the commands: load the configuration, select a
//! server, connect, and map errors to failures with a fix.

use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use rmcp::service::ClientInitializeError;
use tokio_util::sync::CancellationToken;

use super::{
    diagnostics,
    outcome::{Exit, Failure},
    timings::Timings,
    ui::Ui,
};
use crate::{
    client::{self, ConnectError, Connection, Launch, RequestError, RequestLimits},
    config::{self, Config, EntryError, LookupError, Server, Target, Transport},
    era::{Era, EraCache},
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
    pub open_native: super::secrets::OpenNative,
    pub open_browser: super::auth::OpenBrowser,
    /// Present with `--timings`.
    pub timings: Option<Arc<Timings>>,
}

impl Context<'_> {
    /// Runs the step, and adds its time to the timings.
    pub fn timed<T>(
        &self,
        step: impl FnOnce(&mut super::timings::Record, Duration),
        work: impl FnOnce() -> T,
    ) -> T {
        let started = Instant::now();
        let result = work();
        if let Some(timings) = &self.timings {
            timings.update(|record| step(record, started.elapsed()));
        }
        result
    }

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
        // Only env and headers values can use a stored secret: a value in
        // the command or the arguments shows in the process list.
        EntryError::Variable {
            error: ExpandError::Unset(variable),
            ..
        } => Failure {
            message: format!(
                "{}; a stored secret can go only in env and headers values",
                failure.message
            ),
            ..failure
        }
        .hint(format!("export {variable}=<value>")),
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
    let ui = context.ui;
    let cache_path = &context.paths.era_cache;
    let cache = era_file::load(cache_path).unwrap_or_else(|error| {
        ui.warning(None, &error.to_string());
        EraCache::default()
    });
    let target = server.target.clone();
    let cached = cache.lookup(&server.name, &target).cloned();
    ui.debug(Some(name), &cached_era_text(cached.as_ref()));
    let started = Instant::now();
    let values = super::secrets::resolve(context, server).await?;
    if let Some(timings) = &context.timings {
        timings.update(|record| record.credentials = Some(started.elapsed()));
    }
    if let Transport::Http(remote) = &server.transport {
        ui.debug(Some(name), &route_text(context.env, server, remote));
    }
    let shown = server.clone();
    let observer = diagnostics::observer(ui, name, context.timings.clone(), move |url| {
        crate::redact::url(&hide_expanded_url(&shown, url.to_string()))
    });
    let http = client::http::client(context.env, &observer)
        .map_err(|error| Failure::protocol(error.to_string()).server(name))?;
    let launch = Launch {
        server,
        cached_era: cached.as_ref(),
        base_env: context.env,
        values: &values,
        http: &http,
        probe_timeout: client::PROBE_TIMEOUT,
        connect_limit: client::CONNECT_LIMIT,
        server_log: &ui.server_log(name),
        cancel: &context.cancel,
        observer: &observer,
    };
    // The connect future holds two transports with their observers. Boxed,
    // it does not make each command future large.
    let connection = Box::pin(client::connect(&launch))
        .await
        .map_err(|error| connect_failure(context.env, server, &error))?;
    if cached.as_ref() != Some(connection.era()) {
        ui.debug(
            Some(name),
            &format!("saved the era: {}", era_text(connection.era())),
        );
        let cache = cache.record(server.name.clone(), target, connection.era().clone());
        if let Err(error) = era_file::save(cache_path, &cache) {
            ui.warning(None, &error.to_string());
        }
    }
    Ok(connection)
}

fn era_text(era: &Era) -> String {
    match era {
        Era::Modern { version } => format!("modern, protocol version {}", version.as_str()),
        Era::Legacy { version } => format!("legacy, protocol version {}", version.as_str()),
    }
}

fn cached_era_text(cached: Option<&Era>) -> String {
    match cached {
        Some(era) => format!("cached era: {}", era_text(era)),
        None => "no cached era: probe with server/discover".to_string(),
    }
}

/// The proxy route of the server URL, with its source.
fn route_text(env: &Env, server: &Server, remote: &config::RemoteServer) -> String {
    let shown = crate::redact::url(&hide_expanded_url(server, remote.url.as_str().to_string()));
    match url::Url::parse(remote.url.as_str()) {
        Ok(target) => format!("route to {shown}: {}", crate::proxy::route(env, &target)),
        Err(_) => format!("route to {shown}: unknown"),
    }
}

fn connect_failure(env: &Env, server: &Server, error: &ConnectError) -> Failure {
    let exit = match error {
        ConnectError::Cancelled => return interrupted().server(server.name.as_str()),
        _ if error.needs_authorization() => Exit::Auth,
        _ => Exit::Protocol,
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
    let failure =
        Failure::new(exit, hide_expanded_url(server, message)).server(server.name.as_str());
    match (&server.transport, exit) {
        (Transport::Http(remote), Exit::Protocol) => through_proxy(env, &remote.url, failure),
        _ => failure,
    }
}

/// A request to a loopback host through a proxy on another host cannot
/// reach the server, because no exception for loopback exists (design C5).
/// The failure then names the route and the fix.
fn through_proxy(env: &Env, url: &crate::server::ServerUrl, failure: Failure) -> Failure {
    let Ok(target) = url::Url::parse(url.as_str()) else {
        return failure;
    };
    let route = crate::proxy::route(env, &target);
    if route.sends_loopback_away(&target) {
        Failure {
            message: format!("{}; the request went through the {route}", failure.message),
            ..failure
        }
        .hint("export NO_PROXY=localhost,127.0.0.1,::1")
    } else {
        failure
    }
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

/// Ctrl-C. `ncp` sent the cancel request or stopped the server first.
#[must_use]
pub fn interrupted() -> Failure {
    Failure::new(Exit::Interrupted, "interrupted")
}

/// The failure of a request after the connect.
#[must_use]
pub fn request_failure(server: &Server, method: &str, error: &RequestError) -> Failure {
    let failure = match error {
        RequestError::Cancelled => interrupted(),
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
                verbosity: super::super::ui::Verbosity::Normal,
            },
        )
        .unwrap_err();
        assert!(failure.message.contains("does not exist"), "{failure:?}");
        assert_eq!(failure.hint.as_deref(), Some("ncp server list"));
    }
}
