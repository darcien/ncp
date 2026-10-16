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
    outcome::{Exit, Failure, Hint, Warning},
    project_file::{self, Change, ProjectFile},
    timings::Timings,
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
    text::code,
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
    pub cache: CacheUse,
    /// A person can answer questions: stdin and stderr are a TTY.
    pub interactive: bool,
    /// `--config-file` names the file: no search for a project file.
    pub named_config: bool,
    /// Runs before the rename of a changed configuration file. Tests give
    /// a hook that changes the file, as another tool can.
    pub before_rename: fn(&Path),
}

impl Context<'_> {
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
    /// The view that runs: the global entries, and the entries of a trusted
    /// project file in place of global entries of the same names. An entry of
    /// an untrusted project file is not in it. Thus no command can start it,
    /// send to it, or read the store for it.
    pub config: Config,
    /// The global file.
    pub path: PathBuf,
    pub exists: bool,
    pub project: ProjectFile,
}

impl Loaded {
    /// The project file that stops the name in the gate, if one does.
    /// Before a trust, `config` is the global view, so it tells if the
    /// global file has the name.
    #[must_use]
    pub fn blocked_by(&self, name: &str) -> Option<&Project> {
        self.project
            .project()
            .filter(|project| project::stops(project, name, self.config.has(name)))
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
/// global file gives an empty configuration: only `server add` makes it.
/// With `search` false (`--config-file`), there is no project file. Only a
/// trusted project file joins the view, and only then its `${NAME}` values
/// expand. Before the trust, no value of the environment goes into it.
///
/// # Errors
///
/// Returns a usage failure when a file does not parse, and a local failure when
/// `ncp` cannot read the global file.
pub fn load_config(paths: &Paths, env: &Env, search: bool) -> Result<Loaded, Failure> {
    let path = &paths.config_file;
    let (text, exists) = match std::fs::read_to_string(path) {
        Ok(text) => (text, true),
        Err(error) if error.kind() == ErrorKind::NotFound => ("{}".to_string(), false),
        Err(error) => {
            return Err(Failure::protocol(format!(
                "ncp cannot read the configuration file {}",
                code(&path.display().to_string())
            ))
            .caused_by(&error));
        }
    };
    let global = config::parse(&text, env).map_err(|error| {
        Failure::usage(format!(
            "The configuration file {} is not valid",
            code(&path.display().to_string())
        ))
        .caused_by(&error)
    })?;
    let loaded = Loaded {
        config: global,
        path: path.clone(),
        exists,
        project: project_file::find(paths, env, search),
    };
    match loaded.project.project().map(|project| project.trust) {
        Some(Trust::Trusted) => trusted(loaded, env),
        _ => Ok(loaded),
    }
}

// The view after the trust: the project entries join it, from the same
// bytes that the user checked.
fn trusted(loaded: Loaded, env: &Env) -> Result<Loaded, Failure> {
    let ProjectFile::Read(project) = loaded.project else {
        return Ok(loaded);
    };
    let entries = config::parse(&project.text, env).map_err(|error| {
        Failure::usage(format!(
            "The project file {} is not valid",
            code(&project.file.display().to_string())
        ))
        .caused_by(&error)
    })?;
    Ok(Loaded {
        config: loaded.config.with_project(entries),
        project: ProjectFile::Read(Project {
            trust: Trust::Trusted,
            ..project
        }),
        ..loaded
    })
}

/// The command can name a server of an untrusted project file. When a person
/// can answer, `ncp` then shows the entries and asks, as `mise` asks. A Yes
/// trusts the bytes that `ncp` read. A No writes the ignored record. With no
/// answer, `select` stops with the entries.
///
/// # Errors
///
/// Returns the failures of `load_config`, a failure when `ncp` cannot write a
/// record, and exit 130 at Ctrl-C.
pub async fn load(context: &Context<'_>, named: Option<&str>) -> Result<Loaded, Failure> {
    let loaded = context.timed(
        |record, elapsed| record.config = Some(elapsed),
        || load_config(&context.paths, context.env, !context.named_config),
    )?;
    report_files(context, &loaded);
    let Some(project) = named.and_then(|name| loaded.blocked_by(name)) else {
        return Ok(loaded);
    };
    if !context.interactive || project.trust == Trust::Ignored {
        return Ok(loaded);
    }
    context
        .ui
        .diagnostic(&format!("{}\n", super::render::project_question(project)));
    let change = match super::ask::confirm(context, "Trust? [y/N] ").await? {
        Some(true) => Change::Trust,
        Some(false) => Change::Ignore,
        None => return Ok(loaded),
    };
    let trust = project_file::change(&context.paths, project, change)?;
    let loaded = Loaded {
        project: match loaded.project {
            ProjectFile::Read(project) => ProjectFile::Read(Project { trust, ..project }),
            other => other,
        },
        ..loaded
    };
    match trust {
        Trust::Trusted => trusted(loaded, context.env),
        _ => Ok(loaded),
    }
}

// `-v` names the files, and a file that `ncp` does not use gives a
// warning.
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
pub fn joined_failure(word: &str, fix: Hint) -> Failure {
    Failure::usage(format!(
        "{} joins a server name and a tool name",
        code(word)
    ))
    .hint(fix)
}

/// Selects a server and shows the warnings of its entry.
///
/// # Errors
///
/// Returns a usage failure for an unknown server or an invalid entry.
pub fn select<'a>(loaded: &'a Loaded, name: &str, ui: Ui) -> Result<&'a Server, Failure> {
    if let Some(project) = loaded.blocked_by(name) {
        return Err(blocked(name, project, loaded.config.has(name)));
    }
    let server = loaded.config.server(name).map_err(|error| match error {
        LookupError::NotFound(_) => {
            let shown = |path: &Path| code(&path.display().to_string());
            let place = match (&loaded.project, loaded.exists) {
                (ProjectFile::Read(project), _) if project.trust == Trust::Trusted => {
                    format!(" in {} or {}", shown(&loaded.path), shown(&project.file))
                }
                (_, true) => format!(" in {}", shown(&loaded.path)),
                (_, false) => format!(", because {} does not exist", shown(&loaded.path)),
            };
            let names = loaded
                .config
                .servers()
                .map(|server| server.name.as_str())
                .chain(loaded.config.invalid().map(|(name, _)| name));
            let failure = Failure::usage(format!("No server {} is configured{place}", code(name)));
            let failure = match super::near::nearest(name, names) {
                Some(near) => failure.hint(format!("Did you mean {}?", code(near))),
                None => failure,
            };
            failure.hint(Hint::run("see the configured servers", "ncp server list"))
        }
        LookupError::Invalid { error, .. } => entry_failure(name, &error, loaded.file_of(name)),
    })?;
    if let Transport::Sse(_) = server.transport {
        return Err(Failure::usage(format!(
            "The server {} has the type `sse` in {}. ncp connects only to `stdio` and `http` servers.",
            code(name),
            code(&loaded.file_of(name).display().to_string())
        ))
        .about(name));
    }
    loaded
        .config
        .warnings()
        .iter()
        .filter(|warning| warning.server.as_deref() == Some(name))
        .for_each(|warning| ui.warn(&unknown_key(warning, loaded)));
    Ok(server)
}

// The failure shows the entries of the file, so that the user or an agent
// can check them before `ncp trust`.
fn blocked(name: &str, project: &Project, in_global: bool) -> Failure {
    let head = format!(
        "The server {} is in {}",
        code(name),
        code(&project.file.display().to_string())
    );
    let trust = Hint::run_text("trust the file", "ncp trust");
    let state = match project.trust {
        Trust::Ignored => {
            return Failure::usage(format!("{head}, which you ignored"))
                .about(name)
                .hint(trust);
        }
        Trust::Untrusted { changed: true } => "which changed after you trusted it",
        // `blocked_by` gives no trusted file.
        Trust::Untrusted { changed: false } | Trust::Trusted => "which you did not trust",
    };
    let failure = super::render::project_entries(&project.entries)
        .into_iter()
        .fold(
            Failure::usage(format!("{head}, {state}:")).about(name),
            Failure::data,
        )
        .hint(trust);
    if in_global {
        failure.hint(Hint::run_text(
            "ignore the file and use the entry of the global file",
            "ncp trust --ignore",
        ))
    } else {
        failure
    }
}

/// The warning for a key that `ncp` does not know. An entry names its
/// file, because a project file and the global file can have the same
/// server.
#[must_use]
pub fn unknown_key(warning: &config::Warning, loaded: &Loaded) -> Warning {
    let key = code(&warning.key);
    match &warning.server {
        Some(server) => Warning::new(format!(
            "The entry of the server {} in {} has the unknown key {key}. ncp ignores it.",
            code(server),
            code(&loaded.file_of(server).display().to_string())
        ))
        .about(server.as_str()),
        None => Warning::new(format!(
            "The configuration has the unknown key {key}. ncp ignores it."
        )),
    }
}

fn entry_failure(name: &str, error: &EntryError, path: &Path) -> Failure {
    let failure = Failure::usage(format!(
        "The entry of the server {} in {} is not valid",
        code(name),
        code(&path.display().to_string())
    ))
    .about(name)
    .caused_by(error);
    let failure = super::fixes::with_entry_hints(failure, error);
    match error {
        // Only env and headers values can use a stored secret: a value in
        // the command or the arguments shows in the process list.
        EntryError::Variable {
            error: ExpandError::Unset(variable),
            ..
        } => failure.hint(format!(
            "ncp puts a stored secret only into `env` and `headers` values. To set the variable, run `export {variable}=<value>`. Then run the command again."
        )),
        _ => failure,
    }
}

/// Connects to the server, with the cached protocol version. Saves the
/// version when it changed.
///
/// # Errors
///
/// Returns a failure when the connect fails. When `ncp` cannot read or write
/// the version cache, it gives a warning: `ncp` works the same with no cache.
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

// The store read has its own limit of 60 s, so it comes first. The OAuth
// steps after it (discovery, a refresh) have the limit of the connect and
// Ctrl-C (design C7).
async fn stored_login(
    context: &Context<'_>,
    login: &super::auth::StoredLogin<'_>,
) -> Result<Option<client::http::Authorized>, Failure> {
    let Some(found) = super::auth::find_tokens(context, login.server).await? else {
        return Ok(None);
    };
    let limit = client::CONNECT_LIMIT;
    let name = login.server.name.as_str();
    let timeout = Failure::protocol(format!(
        "The OAuth discovery for the server {} did not complete in {} s",
        code(name),
        limit.as_secs()
    ))
    .about(name)
    .hint(wait_longer());
    super::auth::limited(
        context,
        limit,
        timeout,
        Box::pin(super::auth::stored_login(context, login, found)),
    )
    .await
    .map_err(|failure| {
        let failure = failure.map_text(|text| hide_expanded_url(login.server, text));
        with_tls_hint(context.env, &login.notes.get(), failure)
    })
}

/// Connects with the stored login of an http server. A server that let a
/// connect with no token in, and then answered `401` to a request, needs
/// this.
///
/// # Errors
///
/// Returns a failure when the store has no login, or when the connect fails.
pub async fn open_with_login(
    context: &Context<'_>,
    server: &Server,
    challenge: Option<&str>,
) -> Result<Connection, Failure> {
    let prepared = prepare(context, server).await?;
    let needs_login = || {
        Failure::new(Exit::Auth, needs_login_text(server))
            .about(server.name.as_str())
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
/// `tools/list` with no token. `auth login` uses it for discovery and for
/// the scopes (MCP 2025-11-25, basic/authorization, "Authorization Server
/// Discovery"). `None` when the server needs no login or gives no
/// challenge.
///
/// # Errors
///
/// Returns a failure when `ncp` cannot read a value of the entry.
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
            ui.warn(&Warning::new(error.to_string()).caused_by(&error));
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
                    .map_err(|error| http_failure(name, &error))?,
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
        let failure = connect_failure(context.env, server, error, &self.notes.get())
            .map_text(|text| hide_expanded_url(server, text));
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

    // When `ncp` cannot write the cache, it gives only a warning, because
    // `ncp` works the same with no cache.
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
            ui.warn(&Warning::new(error.to_string()).caused_by(&error));
        }
    }
}

fn cached_version_text(cached: Option<&ProtocolVersion>) -> String {
    match cached {
        Some(version) => format!("cached protocol version: {}", version.describe()),
        None => "no cached protocol version: probe with server/discover".to_string(),
    }
}

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
    let name = server.name.as_str();
    if let Some(redirected) = notes
        .redirect
        .as_ref()
        .filter(|_| !error.needs_authorization())
    {
        return Failure::protocol(format!(
            "The server {} answered {} with a redirect to {}. ncp follows no redirect.",
            code(name),
            redirected.status,
            code(&redirected.location)
        ))
        .about(name)
        .hint("If you trust the new URL, put it in the entry");
    }
    let exit = if error.needs_authorization() {
        Exit::Auth
    } else {
        Exit::Protocol
    };
    let failure = match error {
        ConnectError::Cancelled => return interrupted(Some(name)),
        _ if error.needs_authorization() => Failure::new(exit, needs_login_text(server)),
        ConnectError::Handshake { phase, source } => with_handshake_causes(
            Failure::new(
                exit,
                format!(
                    "The handshake with the server {} failed in {phase}",
                    code(name)
                ),
            ),
            source,
        ),
        ConnectError::Http(error) => return http_failure(name, error),
        ConnectError::Spawn { .. } => {
            Failure::new(exit, format!("ncp cannot start the server {}", code(name)))
                .caused_by(error)
        }
        ConnectError::Exited { .. } => Failure::new(
            exit,
            format!(
                "The server {} stopped before the handshake completed",
                code(name)
            ),
        )
        .caused_by(error),
        ConnectError::BadVersion(version) => Failure::new(
            exit,
            format!(
                "The server {} gave the protocol version {}, which is not a date",
                code(name),
                code(version)
            ),
        ),
        ConnectError::Timeout(limit) => Failure::new(
            exit,
            format!(
                "The server {} did not complete the connection in {} s",
                code(name),
                limit.as_secs()
            ),
        ),
        ConnectError::SseNotSupported(_) => Failure::new(
            exit,
            format!("ncp does not connect to the `sse` server {}", code(name)),
        ),
        ConnectError::NoHttpClient => Failure::new(
            exit,
            format!("ncp has no HTTP client for the server {}", code(name)),
        ),
    }
    .about(name);
    let failure = if notes.unknown_issuer {
        with_tls_hint(env, notes, failure.cause(UNKNOWN_ISSUER))
    } else {
        failure
    };
    // The log of the server is on stderr already. The command gives the
    // rest of the reason.
    let failure = match error {
        ConnectError::Exited { .. } => failure.hint(format!(
            "To see why the server stopped, run {} in a shell",
            code(&super::render::target(server))
        )),
        _ if exit == Exit::Auth => failure.hint(login_hint(server)),
        // B78.
        _ if error.is_not_found() => failure.hint(
            "The server has no MCP endpoint at the path of the URL. Many servers use `/mcp`. \
             Examine the path in the entry.",
        ),
        _ => failure,
    };
    // A proxy on another host can also answer 401 or 403.
    match &server.transport {
        Transport::Http(remote) => through_proxy(env, &remote.url, failure),
        _ => failure,
    }
}

// The causes of a failed handshake. rmcp gives no `source()` for a
// transport error, so the client unwraps it (design section 6.1).
fn with_handshake_causes(
    failure: Failure,
    error: &rmcp::service::ClientInitializeError,
) -> Failure {
    match error {
        ClientInitializeError::TransportError { error, .. } => {
            with_transport_causes(failure, error)
        }
        other => failure.caused_by(other),
    }
}

fn with_transport_causes(
    failure: Failure,
    error: &rmcp::transport::DynamicTransportError,
) -> Failure {
    client::transport_errors(error)
        .into_iter()
        .fold(failure, Failure::caused_by)
}

// A failure of the HTTP client names the variable and the path of a bad
// `SSL_CERT_FILE` in the message (requirement N4).
pub(super) fn http_failure(name: &str, error: &client::http::HttpError) -> Failure {
    let failure = Failure::protocol(format!("{error} for the server {}", code(name)))
        .about(name)
        .caused_by(error);
    match error {
        client::http::HttpError::CertFile { path, .. } if path.is_empty() => {
            failure.hint("Remove `SSL_CERT_FILE`, or set it to a file of PEM certificates")
        }
        _ => failure,
    }
}

/// The text for a certificate of an unknown issuer. rmcp keeps no cause of
/// a transport error, so `ncp` adds it.
pub const UNKNOWN_ISSUER: &str =
    "The certificate of the server has an issuer that no trusted root names";

/// The hint for a certificate of an unknown issuer when `SSL_CERT_FILE` has a
/// value: the file replaces the system roots (design B50).
#[must_use]
pub fn with_tls_hint(env: &Env, notes: &client::http::Notes, failure: Failure) -> Failure {
    if notes.unknown_issuer && env.contains_key("SSL_CERT_FILE") {
        failure.hint(
            "`SSL_CERT_FILE` replaces the system roots. Put each root that you need in the \
             file. Include the public roots.",
        )
    } else {
        failure
    }
}

// A request to a loopback host through a proxy on another host cannot
// reach the server, because no exception for loopback exists (design C5).
// The hint then names the route and the fix.
fn through_proxy(env: &Env, url: &crate::server::ServerUrl, failure: Failure) -> Failure {
    let Ok(target) = url::Url::parse(url.as_str()) else {
        return failure;
    };
    let route = crate::proxy::route(env, &target);
    match &route {
        crate::proxy::Route::Proxy { url, source } if route.sends_loopback_away(&target) => {
            let source = match source {
                crate::proxy::Source::Variable(name) => code(name),
                crate::proxy::Source::SystemSettings => "the system settings".to_string(),
            };
            failure.hint(format!(
                "ncp sent the request to the proxy {} from {source}. To send requests to a \
                 loopback host directly, run `export NO_PROXY=localhost,127.0.0.1,::1`.",
                code(url)
            ))
        }
        _ => failure,
    }
}

/// In `-vv`, the server URL shows as the file writes it. Each other URL,
/// for example of OAuth, shows with its secrets hidden. An expanded value
/// of the server URL can also be in an OAuth URL, for example in
/// `resource`.
///
/// A text that is not a URL, for example a body or an error of a trace,
/// gets only the hidden `${NAME}` values. rmcp puts the server URL into
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
            // Replace the whole URL first, so that the URL as the file
            // writes it shows. Then hide each value in other places, for
            // example in a URL that rmcp makes from it.
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

// The hints of a `401` or `403` after a request with a stored header
// value. A new value comes first: after the user changes the key at one
// origin, the key at another origin can be old (B51).
fn with_secret_hints(failure: Failure, server: &Server, stored: &[SecretName]) -> Failure {
    if !matches!(server.transport, Transport::Http(_)) || stored.is_empty() {
        return failure;
    }
    let word = super::render::shell_word(server.name.as_str());
    let hints: Vec<Hint> = stored
        .iter()
        .map(|name| {
            Hint::text(format!(
                "If the value of {} changed, run `ncp secret set {name} --server {word}` to store the new value",
                code(name.as_str())
            ))
        })
        .chain(failure.hints().iter().cloned())
        .collect();
    failure.with_hints(hints)
}

fn needs_login_text(server: &Server) -> String {
    format!(
        "The server {} needs a login or more scopes",
        code(server.name.as_str())
    )
}

pub(super) fn login_hint(server: &Server) -> Hint {
    Hint::run(
        "log in",
        format!(
            "ncp auth login {}",
            super::render::shell_word(server.name.as_str())
        ),
    )
}

/// The hint for a time limit that `--timeout` sets.
#[must_use]
pub fn wait_longer() -> Hint {
    Hint::text("To wait longer, run the command again with `--timeout <SECONDS>`")
}

/// Ctrl-C. `ncp` sent the cancel request or stopped the server first.
#[must_use]
pub fn interrupted(server: Option<&str>) -> Failure {
    match server {
        Some(server) => Failure::new(
            Exit::Interrupted,
            format!(
                "ncp stopped the work with the server {} because of an interrupt",
                code(server)
            ),
        )
        .about(server),
        None => Failure::new(Exit::Interrupted, "ncp stopped because of an interrupt"),
    }
}

// The store can fail at the save of a refresh. A read after the refresh
// lock can also find tokens of another server. A file that fails is a
// local error. A native store that fails, for example a locked keyring,
// is an authorization error, because the file can keep the tokens
// (design B51).
fn token_store_failure(server: &Server, text: &str) -> Failure {
    if let Some(mismatch) = crate::oauth::store::binding_mismatch(text) {
        return super::auth::binding_failure(server, &mismatch);
    }
    let name = server.name.as_str();
    let keep = format!(
        "ncp cannot keep the OAuth tokens of the server {}",
        code(name)
    );
    // rmcp keeps the store error as one text, so it is one cause (design
    // rule 7).
    match crate::oauth::store::file_failure(text) {
        Some(text) => Failure::protocol(keep).about(name).cause(text),
        None => Failure::new(Exit::Auth, keep)
            .about(name)
            .cause(text)
            .hint(super::auth::file_login_hint(name)),
    }
}

// The text of the server error is free text, so it is a cause.
fn server_error(server: &str, method: &str, data: &rmcp::ErrorData) -> Failure {
    Failure::protocol(format!(
        "The server {} returned the error {} for {}",
        code(server),
        data.code.0,
        code(method)
    ))
    .cause(data.message.as_ref())
}

// The failure of a request that needs the user to open pages first, or `None`
// when the error names no page. Only a page that is safe to open goes into the
// failure. The others show the reason that `ncp` refuses them, and not their
// URL.
fn page_failure(server: &str, method: &str, data: &rmcp::ErrorData) -> Option<Failure> {
    let found = crate::elicit::pages(data.data.as_ref());
    if found.is_empty() {
        return None;
    }
    let failure = found.iter().fold(
        Failure::new(
            Exit::InputRequired,
            format!(
                "The server {} asks you to open a page before it answers {}:",
                code(server),
                code(method)
            ),
        )
        .cause(data.message.as_ref()),
        |failure, page| match page {
            Ok(page) => failure.data(format!("{} ({})", page.url, page.message)),
            Err(reason) => failure.data(format!("ncp refused a page: {reason}")),
        },
    );
    let pages: Box<[crate::elicit::Page]> = found.into_iter().filter_map(Result::ok).collect();
    Some(if pages.is_empty() {
        failure
    } else {
        failure
            .hint("After you complete the steps on the page, run the command again")
            .with_pages(pages)
    })
}

pub fn request_failure(server: &Server, method: &str, error: &RequestError) -> Failure {
    if let Some(text) = error.token_store_error() {
        return token_store_failure(server, &text);
    }
    let name = server.name.as_str();
    let failure = match error {
        RequestError::Cancelled => interrupted(Some(name)),
        RequestError::Timeout(limit) => Failure::protocol(format!(
            "The server {} gave no answer to {} in {} s. ncp sent a cancel request.",
            code(name),
            code(method),
            limit.as_secs()
        ))
        .hint(wait_longer()),
        _ if error.needs_authorization() => {
            let need = match error.required_scopes() {
                Some(scopes) if !scopes.is_empty() => {
                    format!("the scopes {}", code(&scopes.join(" ")))
                }
                _ => "a login or more scopes".to_string(),
            };
            Failure::new(
                Exit::Auth,
                format!(
                    "The server {} needs {need} for {}",
                    code(name),
                    code(method)
                ),
            )
            .hint(login_hint(server))
            .with_login_challenge(error.login_challenge())
        }
        RequestError::Service(rmcp::ServiceError::McpError(data))
            if data.code.0 == crate::elicit::URL_REQUIRED =>
        {
            page_failure(name, method, data).unwrap_or_else(|| server_error(name, method, data))
        }
        RequestError::Service(rmcp::ServiceError::McpError(data)) => {
            server_error(name, method, data)
        }
        RequestError::Service(rmcp::ServiceError::TransportSend(error)) => {
            with_transport_causes(request_failed(name, method), error)
        }
        _ => request_failed(name, method).caused_by(error),
    };
    failure
        .about(name)
        .map_text(|text| hide_expanded_url(server, text))
}

fn request_failed(server: &str, method: &str) -> Failure {
    Failure::protocol(format!(
        "The request {} to the server {} failed",
        code(method),
        code(server)
    ))
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
        assert_eq!(with_tls_hint(&set, &noted, failure()).hints().len(), 1);
        assert_eq!(with_tls_hint(&env(&[]), &noted, failure()).hints(), []);
        assert_eq!(
            with_tls_hint(&set, &client::http::Notes::default(), failure())
                .hints()
                .len(),
            0
        );
    }

    #[test]
    fn redaction_hides_a_url_value_in_each_part_of_a_failure() {
        let env = Env::from([("K".to_string(), "s3cr3t".to_string())]);
        let config = config::parse(
            r#"{"mcpServers": {"web": {"type": "http", "url": "https://h/mcp?key=${K}"}}}"#,
            &env,
        )
        .unwrap();
        let server = config.server("web").unwrap();
        let url = "https://h/mcp?key=s3cr3t";
        let failure = Failure::protocol(format!("m {url}"))
            .data(format!("d {url}"))
            .cause(format!("error sending request for url ({url})"))
            .hint(format!("h {url}"))
            .map_text(|text| hide_expanded_url(server, text));
        let shown = format!("{failure:?}");
        assert!(!shown.contains("s3cr3t"), "{shown}");
        assert_eq!(failure.diagnostic().causes().len(), 1);
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
        assert_eq!(failure.exit(), Exit::Usage);
        assert!(failure.message().contains("/c/mcp.json"), "{failure:?}");
        assert_eq!(
            failure.hints().first().map(Hint::message),
            Some(
                "ncp puts a stored secret only into `env` and `headers` values. To set the \
                 variable, run `export TOKEN=<value>`. Then run the command again."
            )
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
        assert!(failure.message().contains("does not exist"), "{failure:?}");
        assert_eq!(
            failure.hints().first().and_then(Hint::command),
            Some("ncp server list")
        );
    }
}
