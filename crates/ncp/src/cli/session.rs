//! The shared steps of the commands: load the configuration, select a
//! server, connect, and map errors to failures with a fix.

use std::{
    fmt::Write as _,
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
    trust::ProjectFile,
    ui::{Ui, Verbosity},
};
use crate::{
    client::{self, ConnectError, Connection, Launch, RequestError, RequestLimits},
    config::{self, Config, EntryError, LookupError, Server, Target, Transport},
    credential::SecretName,
    expand::{Env, ExpandError},
    paths::Paths,
    project::{self, Project, Trust},
    protocol_version::{ProtocolVersion, VersionCache},
    protocol_version_file,
};

/// What `ncp` does with its caches (the protocol version cache and the list cache).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheUse {
    ReadWrite,
    /// `--refresh`: get fresh data, and keep it.
    WriteOnly,
    /// `--no-cache`.
    Off,
}

impl CacheUse {
    #[must_use]
    pub fn reads(self) -> bool {
        self == Self::ReadWrite
    }

    #[must_use]
    pub fn writes(self) -> bool {
        self != Self::Off
    }
}

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
    /// What `ncp` does with its caches.
    pub cache: CacheUse,
    /// A person can answer questions: stdin and stderr are a TTY.
    pub interactive: bool,
    /// `--config-file` names the file: no search for a project file.
    pub named_config: bool,
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
    /// The view that runs: the global entries, and the entries of a
    /// trusted project file in place of global entries of the same names.
    /// An entry of a project file that is not trusted is not in it, so no
    /// command can start it, send to it, or read the store for it.
    pub config: Config,
    /// The global file.
    pub path: PathBuf,
    pub exists: bool,
    pub project: ProjectFile,
    global: Config,
}

impl Loaded {
    /// The view after a change of the trust, from the same bytes.
    ///
    /// # Errors
    ///
    /// Returns a usage failure when a trusted project file does not parse.
    pub fn with_trust(self, trust: Trust, env: &Env) -> Result<Self, Failure> {
        let project = match self.project {
            ProjectFile::Read(project) => ProjectFile::Read(Project { trust, ..project }),
            other => other,
        };
        assemble(self.global, self.path, self.exists, project, env)
    }

    /// Why the gate stops the server name, if it does.
    #[must_use]
    pub fn stop(&self, name: &str) -> Option<project::Stop> {
        project::gate(self.project.project(), name, self.global.has(name))
    }

    /// The trusted project file, when it gives the entry of the name.
    #[must_use]
    pub fn project_of(&self, name: &str) -> Option<&Project> {
        self.project
            .project()
            .filter(|project| project.trust == Trust::Trusted && project.has(name))
    }

    /// The file of the entry that runs for the name.
    #[must_use]
    pub fn file_of(&self, name: &str) -> &Path {
        self.project_of(name)
            .map_or(self.path.as_path(), |project| project.file.as_path())
    }
}

/// Reads the global file and searches for the project file. A missing
/// global file gives an empty configuration: `ncp` never makes the file.
/// With `search` false (`--config-file`), there is no project file.
///
/// # Errors
///
/// Returns a usage failure when a file does not parse, and a local
/// failure when the global file cannot be read.
pub fn load_config(paths: &Paths, env: &Env, search: bool) -> Result<Loaded, Failure> {
    let path = &paths.config_file;
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
    let global = config::parse(&text, env)
        .map_err(|error| Failure::usage(format!("{}: {error}", path.display())))?;
    let project = if search {
        super::trust::find(paths, env)
    } else {
        ProjectFile::NotSearched
    };
    assemble(global, path.clone(), exists, project, env)
}

/// Only a trusted project file joins the view. Its `${NAME}` values expand
/// only then: before the trust, no value of the environment goes into it.
fn assemble(
    global: Config,
    path: PathBuf,
    exists: bool,
    project: ProjectFile,
    env: &Env,
) -> Result<Loaded, Failure> {
    let config = match project.project() {
        Some(project) if project.trust == Trust::Trusted => global.clone().with_project(
            config::parse(&project.text, env)
                .map_err(|error| Failure::usage(format!("{}: {error}", project.file.display())))?,
        ),
        _ => global.clone(),
    };
    Ok(Loaded {
        config,
        path,
        exists,
        project,
        global,
    })
}

/// Loads the configuration for a command. When the command names a server
/// of a project file that is not trusted, and a person can answer, shows
/// the entries and asks, as `mise` asks. A Yes trusts the bytes that `ncp`
/// read; a No writes the ignored record. With no answer, `select` stops
/// with the entries.
///
/// # Errors
///
/// Returns the failures of `load_config`, a failure when a record cannot
/// be written, and exit 130 at Ctrl-C.
pub async fn load(context: &Context<'_>, named: Option<&str>) -> Result<Loaded, Failure> {
    let loaded = context.timed(
        |record, elapsed| record.config = Some(elapsed),
        || load_config(&context.paths, context.env, !context.named_config),
    )?;
    report_files(context, &loaded);
    let asks = context.interactive
        && named
            .and_then(|name| loaded.stop(name))
            .is_some_and(|stop| matches!(stop, project::Stop::Untrusted { .. }));
    let Some(project) = loaded.project.project().filter(|_| asks) else {
        return Ok(loaded);
    };
    context
        .ui
        .diagnostic(&format!("{}\n", super::render::project_question(project)));
    match super::ask::confirm(context, "Trust? [y/N] ").await? {
        Some(true) => {
            super::trust::trust(&context.paths, project)?;
            loaded.with_trust(Trust::Trusted, context.env)
        }
        Some(false) => {
            super::trust::ignore(&context.paths, project)?;
            loaded.with_trust(Trust::Ignored, context.env)
        }
        None => Ok(loaded),
    }
}

/// `-v` names the files, and a file that `ncp` does not use gives a
/// warning.
fn report_files(context: &Context<'_>, loaded: &Loaded) {
    let state = if loaded.exists {
        ""
    } else {
        " (does not exist)"
    };
    context.ui.debug(
        None,
        &format!("configuration file: {}{state}", loaded.path.display()),
    );
    match &loaded.project {
        ProjectFile::NotSearched => {}
        ProjectFile::Absent(folder) => context.ui.debug(
            None,
            &format!(
                "no project file: the search stopped at {}",
                folder.display()
            ),
        ),
        ProjectFile::Unusable { file, reason } => context.ui.warning(
            None,
            &format!("ncp does not use {}: {reason}", file.display()),
        ),
        ProjectFile::Read(project) => context.ui.debug(
            None,
            &format!(
                "project file: {} ({})",
                project.file.display(),
                project.trust.word()
            ),
        ),
    }
}

/// One row of `server list` and `ncp trust`: an entry with its file.
pub struct Row {
    pub name: String,
    /// The transport and the target, or the error of an invalid entry.
    pub entry: Result<(&'static str, String), String>,
    pub file: PathBuf,
    /// `None` for an entry of the global file.
    pub trust: Option<Trust>,
}

impl Row {
    /// The object of `server list`: `{"name", "transport", "target"}` or
    /// `{"name", "error"}`, with `scope`, `file`, and `trust` for a
    /// project entry.
    #[must_use]
    pub fn json(&self) -> serde_json::Value {
        let mut row = match &self.entry {
            Ok((transport, target)) => {
                serde_json::json!({"name": self.name, "transport": transport, "target": target})
            }
            Err(error) => serde_json::json!({"name": self.name, "error": error}),
        };
        row["scope"] = serde_json::json!(if self.trust.is_some() {
            "project"
        } else {
            "global"
        });
        row["file"] = serde_json::json!(self.file);
        if let Some(trust) = self.trust {
            row["trust"] = serde_json::json!(trust.word());
        }
        row
    }
}

/// The rows of the entries of a project file.
#[must_use]
pub fn project_rows(project: &Project) -> Vec<Row> {
    project
        .entries
        .iter()
        .map(|entry| Row {
            name: entry.name.clone(),
            entry: entry
                .shape
                .as_ref()
                .map(|shape| (shape.transport, super::render::target_text(&shape.target)))
                .map_err(ToString::to_string),
            file: project.file.clone(),
            trust: Some(project.trust),
        })
        .collect()
}

/// The rows of `server list`: the entries that run, then the entries of a
/// project file that is not trusted, with their state. Nothing else
/// happens for them (design rule 8).
#[must_use]
pub fn list_rows(loaded: &Loaded) -> Vec<Row> {
    let place = |name: &str| match loaded.project_of(name) {
        Some(project) => (project.file.clone(), Some(Trust::Trusted)),
        None => (loaded.path.clone(), None),
    };
    let servers = loaded.config.servers().map(|server| {
        let name = server.name.to_string();
        let (file, trust) = place(&name);
        Row {
            entry: Ok((
                super::render::transport_name(server),
                super::render::target(server),
            )),
            name,
            file,
            trust,
        }
    });
    let invalid = loaded.config.invalid().map(|(name, error)| {
        let (file, trust) = place(name);
        Row {
            name: name.to_string(),
            entry: Err(error.to_string()),
            file,
            trust,
        }
    });
    let waiting = loaded
        .project
        .project()
        .filter(|project| project.trust != Trust::Trusted)
        .map(project_rows)
        .unwrap_or_default();
    servers.chain(invalid).chain(waiting).collect()
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
    if let (Some(stop), Some(project)) = (loaded.stop(name), loaded.project.project()) {
        return Err(stop_failure(name, project, stop));
    }
    let server = loaded.config.server(name).map_err(|error| match error {
        LookupError::NotFound(_) => {
            let place = match (&loaded.project, loaded.exists) {
                (ProjectFile::Read(project), _) if project.trust == Trust::Trusted => {
                    format!("in {} or {}", loaded.path.display(), project.file.display())
                }
                (_, true) => format!("in {}", loaded.path.display()),
                (_, false) => format!("because {} does not exist", loaded.path.display()),
            };
            let names = loaded
                .config
                .servers()
                .map(|server| server.name.as_str())
                .chain(loaded.config.invalid().map(|(name, _)| name));
            let near = super::near::nearest(name, names)
                .map(|near| format!("; did you mean {near:?}?"))
                .unwrap_or_default();
            Failure::usage(format!("no server {name:?} is configured {place}{near}"))
                .hint("ncp server list")
        }
        LookupError::Invalid { error, .. } => entry_failure(name, &error, loaded.file_of(name)),
    })?;
    if let Transport::Sse(_) = server.transport {
        return Err(Failure::usage(format!(
            "the entry in {} has type \"sse\", and ncp connects only to stdio and http servers",
            loaded.file_of(name).display()
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

/// The failure of a name that the gate stops, with the entries of the
/// file, so that the user or an agent can check them before `ncp trust`.
#[must_use]
pub fn stop_failure(name: &str, project: &Project, stop: project::Stop) -> Failure {
    let file = project.file.display();
    match stop {
        project::Stop::Untrusted { changed, in_global } => {
            let state = if changed {
                "which changed since you trusted it"
            } else {
                "which is not trusted"
            };
            let failure = Failure::usage(format!(
                "{name} is in {file}, {state}:\n{}",
                super::render::project_entries(&project.entries).trim_end()
            ))
            .server(name)
            .hint("ncp trust");
            if in_global {
                failure.hint("ncp trust --ignore")
            } else {
                failure
            }
        }
        project::Stop::Ignored => Failure::usage(format!("{name} is in {file}, which you ignored"))
            .server(name)
            .hint("ncp trust"),
    }
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

/// Connects to the server, with the cached protocol version. Saves the
/// version when it changed.
///
/// # Errors
///
/// Returns a failure when the connect fails. A version cache that cannot be
/// read or written gives a warning: `ncp` works the same with no cache.
pub async fn open(context: &Context<'_>, server: &Server) -> Result<Connection, Failure> {
    let prepared = prepare(context, server).await?;
    let launch = prepared.launch(context, server);
    // The connect future holds two transports with their observers. Boxed,
    // it does not make each command future large.
    let connection = match (
        Box::pin(client::connect(&launch)).await,
        &server.transport,
        &prepared.http,
    ) {
        (Err(error), Transport::Http(remote), Some(http)) if error.needs_authorization() => {
            let challenge = error.challenge();
            let login = super::auth::StoredLogin {
                server,
                remote,
                http,
                observer: &prepared.observer,
                notes: &prepared.notes,
                challenge: challenge.as_deref(),
            };
            let Some(authorized) = stored_login(context, &login).await? else {
                return Err(prepared.connect_failure(context, server, &error));
            };
            let launch = Launch {
                authorized: Some(&authorized),
                ..launch
            };
            Box::pin(client::connect(&launch)).await
        }
        (result, ..) => result,
    }
    .map_err(|error| prepared.connect_failure(context, server, &error))?;
    prepared.save_version(context, server, connection.protocol_version());
    Ok(connection)
}

/// The stored login. The store read has its own limit of 60 s, so
/// it comes first. The OAuth steps after it (discovery, a refresh) have
/// the limit of the connect and Ctrl-C (design C7).
async fn stored_login(
    context: &Context<'_>,
    login: &super::auth::StoredLogin<'_>,
) -> Result<Option<client::http::Authorized>, Failure> {
    let Some(found) = super::auth::find_tokens(context, login.server).await? else {
        return Ok(None);
    };
    let limit = client::CONNECT_LIMIT;
    let timeout = Failure::protocol(format!(
        "the OAuth discovery did not complete in {} s",
        limit.as_secs()
    ))
    .server(login.server.name.as_str());
    super::auth::limited(
        context,
        limit,
        timeout,
        Box::pin(super::auth::stored_login(context, login, found)),
    )
    .await
    .map_err(|failure| {
        let failure = Failure {
            message: hide_expanded_url(login.server, failure.message.clone()),
            ..failure
        };
        with_tls_hint(context.env, &login.notes.get(), failure)
    })
}

/// Connects with the stored login of an http server. A server that let a
/// connect with no token in, and then answered `401` to a request, needs
/// this.
///
/// # Errors
///
/// Returns a failure when no login is stored, or when the connect fails.
pub async fn open_with_login(
    context: &Context<'_>,
    server: &Server,
    challenge: Option<&str>,
) -> Result<Connection, Failure> {
    let prepared = prepare(context, server).await?;
    let needs_login = || {
        Failure::new(Exit::Auth, "the server needs a login or more scopes")
            .server(server.name.as_str())
            .hint(login_hint(server))
    };
    let needs_login = || with_secret_hints(needs_login(), server, &prepared.stored);
    let (Transport::Http(remote), Some(http)) = (&server.transport, &prepared.http) else {
        return Err(needs_login());
    };
    let login = super::auth::StoredLogin {
        server,
        remote,
        http,
        observer: &prepared.observer,
        notes: &prepared.notes,
        challenge,
    };
    let Some(authorized) = stored_login(context, &login).await? else {
        return Err(needs_login());
    };
    let launch = Launch {
        authorized: Some(&authorized),
        ..prepared.launch(context, server)
    };
    let connection = Box::pin(client::connect(&launch))
        .await
        .map_err(|error| prepared.connect_failure(context, server, &error))?;
    prepared.save_version(context, server, connection.protocol_version());
    Ok(connection)
}

/// The `WWW-Authenticate` challenge of the server, from a connect and a
/// `tools/list` with no token. `auth login` uses it for discovery and for the scopes, as the
/// spec describes (MCP 2025-11-25, basic/authorization, "Authorization
/// Server Discovery"). `None` when the server needs no login or gives no
/// challenge.
///
/// # Errors
///
/// Returns a failure when a value of the entry cannot be read.
pub async fn challenge(context: &Context<'_>, server: &Server) -> Result<Option<String>, Failure> {
    let prepared = prepare(context, server).await?;
    let launch = prepared.launch(context, server);
    Ok(match Box::pin(client::connect(&launch)).await {
        // A server can let the handshake in and ask for a login at the
        // first request.
        Ok(connection) => {
            let listed = connection.list_tools(context.limits()).await;
            connection.close().await;
            listed.err().and_then(|error| error.challenge())
        }
        Err(error) => error.challenge(),
    })
}

/// The parts of a connect that do not depend on a login.
struct Prepared {
    cache: VersionCache,
    cached: Option<ProtocolVersion>,
    values: crate::config::Values,
    /// The names of the values that came from a store.
    stored: Vec<SecretName>,
    observer: client::observe::Observer,
    http: Option<reqwest::Client>,
    /// What the HTTP client saw that a transport error does not say.
    notes: client::http::ConnectNotes,
    server_log: client::stdio::ServerLog,
    progress: client::ProgressSink,
    elicitor: Option<client::Elicitor>,
}

async fn prepare(context: &Context<'_>, server: &Server) -> Result<Prepared, Failure> {
    let name = server.name.as_str();
    let ui = context.ui;
    let cache = if context.cache.writes() {
        protocol_version_file::load(&context.paths.version_cache).unwrap_or_else(|error| {
            ui.warning(None, &error.to_string());
            VersionCache::default()
        })
    } else {
        VersionCache::default()
    };
    let cached = context
        .cache
        .reads()
        .then(|| cache.lookup(&server.name, &server.target).cloned())
        .flatten();
    ui.debug(Some(name), &cached_version_text(cached.as_ref()));
    let super::secrets::Resolved { values, stored } =
        super::secrets::resolve(context, server).await?;
    let shown = server.clone();
    let observer = diagnostics::observer(ui, name, context.timings.clone(), shown_url(&shown));
    // Only an http server needs the HTTP client. Thus a bad SSL_CERT_FILE
    // does not stop a stdio server.
    let notes = client::http::ConnectNotes::default();
    let http = match &server.transport {
        Transport::Http(remote) => {
            // The route costs a read of the system settings on macOS.
            if ui.verbosity >= Verbosity::Verbose {
                ui.debug(Some(name), &route_text(context.env, server, remote));
            }
            Some(
                client::http::client(context.env, &observer, &notes)
                    .map_err(|error| Failure::protocol(error.to_string()).server(name))?,
            )
        }
        _ => None,
    };
    Ok(Prepared {
        cache,
        cached,
        values,
        stored,
        observer,
        http,
        notes,
        server_log: ui.server_log(name),
        progress: ui.progress(name),
        elicitor: context
            .interactive
            .then(|| super::ask::elicitor(context, name)),
    })
}

impl Prepared {
    fn connect_failure(
        &self,
        context: &Context<'_>,
        server: &Server,
        error: &ConnectError,
    ) -> Failure {
        let failure = connect_failure(context.env, server, error, &self.notes.get());
        if error.needs_authorization() {
            with_secret_hints(failure, server, &self.stored)
        } else {
            failure
        }
    }

    fn launch<'a>(&'a self, context: &'a Context<'_>, server: &'a Server) -> Launch<'a> {
        Launch {
            server,
            cached_version: self.cached.as_ref(),
            base_env: context.env,
            values: &self.values,
            http: self.http.as_ref(),
            probe_timeout: client::PROBE_TIMEOUT,
            connect_limit: client::CONNECT_LIMIT,
            server_log: &self.server_log,
            cancel: &context.cancel,
            observer: &self.observer,
            authorized: None,
            progress: &self.progress,
            elicitor: self.elicitor.as_ref(),
        }
    }

    /// Saves the protocol version when it changed. A cache that cannot be
    /// written gives a warning: `ncp` works the same with no cache.
    fn save_version(self, context: &Context<'_>, server: &Server, version: &ProtocolVersion) {
        if !context.cache.writes() || self.cached.as_ref() == Some(version) {
            return;
        }
        let ui = context.ui;
        ui.debug(
            Some(server.name.as_str()),
            &format!("saved the protocol version: {}", version.describe()),
        );
        let cache = self
            .cache
            .record(server.name.clone(), server.target.clone(), version.clone());
        if let Err(error) = protocol_version_file::save(&context.paths.version_cache, &cache) {
            ui.warning(None, &error.to_string());
        }
    }
}

fn cached_version_text(cached: Option<&ProtocolVersion>) -> String {
    match cached {
        Some(version) => format!("cached protocol version: {}", version.describe()),
        None => "no cached protocol version: probe with server/discover".to_string(),
    }
}

/// The proxy route of the server URL, with its source.
fn route_text(env: &Env, server: &Server, remote: &config::RemoteServer) -> String {
    let shown = hide_expanded_url(server, remote.url.as_str().to_string());
    match url::Url::parse(remote.url.as_str()) {
        Ok(target) => format!("route to {shown}: {}", crate::proxy::route(env, &target)),
        Err(_) => format!("route to {shown}: unknown"),
    }
}

fn connect_failure(
    env: &Env,
    server: &Server,
    error: &ConnectError,
    notes: &client::http::Notes,
) -> Failure {
    if let Some(text) = error.token_store_error() {
        return token_store_failure(server, &text);
    }
    if let Some(redirected) = notes
        .redirect
        .as_ref()
        .filter(|_| !error.needs_authorization())
    {
        return Failure::protocol(hide_expanded_url(server, redirected.to_string()))
            .hint("put the new URL in the entry when you trust it")
            .server(server.name.as_str());
    }
    if notes.unknown_issuer {
        let failure = Failure::protocol(format!(
            "{}: {UNKNOWN_ISSUER}",
            hide_expanded_url(server, error.to_string())
        ))
        .server(server.name.as_str());
        return with_tls_hint(env, notes, failure);
    }
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
    // The log of the server is on stderr already; the command gives the
    // rest of the reason.
    let failure = if let ConnectError::Exited { .. } = error {
        failure.hint(format!(
            "run the command in a shell to see why: {}",
            super::render::target(server)
        ))
    } else {
        failure
    };
    let failure = if exit == Exit::Auth {
        failure.hint(login_hint(server))
    } else {
        failure
    };
    // A proxy on another host can also answer 401 or 403.
    match &server.transport {
        Transport::Http(remote) => through_proxy(env, &remote.url, failure),
        _ => failure,
    }
}

/// The text for a certificate of an unknown issuer. rmcp keeps no cause of
/// a transport error, so `ncp` adds it.
pub const UNKNOWN_ISSUER: &str =
    "the certificate of the server has an issuer that no trusted root names";

/// The hint for a certificate of an unknown issuer when `SSL_CERT_FILE` is
/// set: the file replaces the system roots (design B50).
#[must_use]
pub fn with_tls_hint(env: &Env, notes: &client::http::Notes, failure: Failure) -> Failure {
    if notes.unknown_issuer && env.contains_key("SSL_CERT_FILE") {
        failure.hint("SSL_CERT_FILE replaces the system roots: the file must hold each root that you need, also the public roots")
    } else {
        failure
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

/// How a URL of the server shows in `-vv`: the server URL as the file
/// writes it; each other URL, for example of OAuth, with its secrets
/// hidden. An expanded value of the server URL can also be in an OAuth
/// URL, for example in `resource`.
///
/// A text that is not a URL, for example a body or an error of a trace,
/// gets only the hidden `${NAME}` values: rmcp puts the server URL into
/// OAuth bodies and errors, also percent-encoded.
pub fn shown_url(server: &Server) -> impl Fn(&str) -> String + Send + Sync + 'static {
    let server = server.clone();
    move |text| {
        let hidden = hide_expanded_url(&server, text.to_string());
        // "error: ..." is a valid URL with the scheme "error", so only a
        // web URL counts.
        let is_url = (text.starts_with("http://") || text.starts_with("https://"))
            && !text.contains(char::is_whitespace);
        if is_url {
            crate::redact::url(&hidden)
        } else {
            hidden
        }
    }
}

/// The message with the expanded URL replaced by the URL as the file writes
/// it, with its secret query values and user information hidden. HTTP
/// errors hold the URL, and a query parameter can hold a key.
pub(super) fn hide_expanded_url(server: &Server, message: String) -> String {
    match (&server.transport, &server.target) {
        (Transport::Http(remote) | Transport::Sse(remote), Target::Remote { url }) => {
            let shown = crate::redact::url(url);
            // The whole URL first, so that the URL as the file writes it
            // shows; then each value in other places, for example in a URL
            // that rmcp makes from it.
            let parts: Vec<&str> = message.split(remote.url.as_str()).collect();
            parts
                .iter()
                .map(|part| match url::Url::parse(remote.url.as_str()) {
                    // reqwest names the URL in its parsed form.
                    Ok(parsed) => part
                        .split(parsed.as_str())
                        .map(|piece| crate::redact::values(piece, &remote.url_values))
                        .collect::<Vec<_>>()
                        .join(&shown),
                    Err(_) => crate::redact::values(part, &remote.url_values),
                })
                .collect::<Vec<_>>()
                .join(&shown)
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

/// The hints of a `401` or `403` after a request with a stored header
/// value. A new value comes first: after the user changes the key at one
/// origin, the key at another origin can be old (B51).
fn with_secret_hints(failure: Failure, server: &Server, stored: &[SecretName]) -> Failure {
    if !matches!(server.transport, Transport::Http(_)) || stored.is_empty() {
        return failure;
    }
    let word = super::render::shell_word(server.name.as_str());
    let hints = stored
        .iter()
        .map(|name| format!("ncp secret set {name} --server {word}"))
        .chain(failure.hints.iter().cloned())
        .collect();
    Failure { hints, ..failure }
}

/// The command that logs in to the server.
fn login_hint(server: &Server) -> String {
    format!(
        "ncp auth login {}",
        super::render::shell_word(server.name.as_str())
    )
}

/// Ctrl-C. `ncp` sent the cancel request or stopped the server first.
#[must_use]
pub fn interrupted() -> Failure {
    Failure::new(Exit::Interrupted, "interrupted")
}

/// The store of the OAuth tokens failed, for example at the save of a
/// refresh, or a read after the refresh lock found tokens of another
/// server. A file that fails is a local error. A native store that fails
/// (for example a locked keyring, design B51) is an authorization error:
/// the file can keep the tokens.
fn token_store_failure(server: &Server, text: &str) -> Failure {
    if let Some(mismatch) = crate::oauth::store::binding_mismatch(text) {
        return super::auth::binding_failure(server, &mismatch);
    }
    if let Some(text) = crate::oauth::store::file_failure(text) {
        return Failure::protocol(format!("cannot keep the OAuth tokens: {text}"))
            .server(server.name.as_str());
    }
    Failure::new(Exit::Auth, format!("cannot keep the OAuth tokens: {text}"))
        .server(server.name.as_str())
        .hint(format!("{} --credential-store file", login_hint(server)))
}

/// The failure of a request after the connect.
#[must_use]
fn server_error(method: &str, data: &rmcp::ErrorData) -> Failure {
    Failure::protocol(format!(
        "{method}: the server returned error {}: {}",
        data.code.0, data.message
    ))
}

/// The failure of a request that needs the user to open pages first, or
/// `None` when the error names no page. Only a page that is safe to open
/// goes into the failure; the others show the reason that `ncp` refuses
/// them, and not their URL.
fn page_failure(method: &str, data: &rmcp::ErrorData) -> Option<Failure> {
    let found = crate::elicit::pages(data.data.as_ref());
    if found.is_empty() {
        return None;
    }
    let list = found.iter().fold(String::new(), |mut list, page| {
        let _ = match page {
            Ok(page) => write!(list, "\n{} ({})", page.url, page.message),
            Err(reason) => write!(list, "\nrefused a page: {reason}"),
        };
        list
    });
    let pages: Box<[crate::elicit::Page]> = found.into_iter().filter_map(Result::ok).collect();
    let failure = Failure::new(
        Exit::InputRequired,
        format!(
            "{method}: the server asks you to open a page first ({}):{list}",
            data.message
        ),
    );
    Some(if pages.is_empty() {
        failure
    } else {
        Failure {
            pages,
            ..failure.hint("open the page, then run the command again")
        }
    })
}

pub fn request_failure(server: &Server, method: &str, error: &RequestError) -> Failure {
    if let Some(text) = error.token_store_error() {
        return token_store_failure(server, &text);
    }
    let failure = match error {
        RequestError::Cancelled => interrupted(),
        RequestError::Timeout(_) => Failure::protocol(format!("{method}: {error}"))
            .hint("give a longer limit with --timeout <SECONDS>"),
        _ if error.needs_authorization() => {
            let need = match error.required_scopes() {
                Some(scopes) if !scopes.is_empty() => {
                    format!("the server needs the scopes {}", scopes.join(" "))
                }
                _ => "the server needs a login or more scopes".to_string(),
            };
            Failure {
                login_challenge: error.login_challenge(),
                ..Failure::new(Exit::Auth, format!("{method}: {need}")).hint(login_hint(server))
            }
        }
        RequestError::Service(rmcp::ServiceError::McpError(data))
            if data.code.0 == crate::elicit::URL_REQUIRED =>
        {
            page_failure(method, data).unwrap_or_else(|| server_error(method, data))
        }
        RequestError::Service(rmcp::ServiceError::McpError(data)) => server_error(method, data),
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
    fn tls_hint_comes_only_for_an_unknown_issuer_with_ssl_cert_file() {
        let env = |pairs: &[(&str, &str)]| -> Env {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        let noted = client::http::Notes {
            unknown_issuer: true,
            ..client::http::Notes::default()
        };
        let set = env(&[("SSL_CERT_FILE", "/roots.pem")]);
        let failure = || Failure::protocol("x");
        assert_eq!(with_tls_hint(&set, &noted, failure()).hints.len(), 1);
        assert!(with_tls_hint(&env(&[]), &noted, failure()).hints.is_empty());
        assert_eq!(
            with_tls_hint(&set, &client::http::Notes::default(), failure())
                .hints
                .len(),
            0
        );
    }

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
        assert_eq!(
            failure.hints.first().map(String::as_str),
            Some("export TOKEN=<value>")
        );
    }

    #[test]
    fn missing_file_gives_an_empty_configuration() {
        let paths = Paths {
            config_file: "/nonexistent/ncp/mcp.json".into(),
            ..Paths::from_env(&Env::from([(
                "HOME".to_string(),
                "/nonexistent".to_string(),
            )]))
            .unwrap()
        };
        let loaded = load_config(&paths, &Env::new(), false).unwrap();
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
        assert_eq!(
            failure.hints.first().map(String::as_str),
            Some("ncp server list")
        );
    }
}
