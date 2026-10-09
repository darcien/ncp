//! The client core: connect to a server, find its era, list and call tools.
//!
//! Era detection: `ncp` probes with `server/discover` in its own timeout. On
//! a legacy error or a timeout, it connects again on a new transport with
//! `initialize`. rmcp `Auto` mode is not used: when a slow server answers
//! the probe late, the late answer arrives on the same transport and the
//! handshake fails (see docs/research/early-tests.md).
//!
//! Time limits: the spec says that a client SHOULD set a timeout for each
//! request (MCP 2025-11-25, basic/lifecycle, "Timeouts"). The whole connect
//! has one limit, and each later request has its own limit. An agent must
//! not wait forever.

pub mod http;
pub mod observe;
pub mod stdio;

use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use rmcp::{
    ClientLifecycleMode, ClientServiceExt, RoleClient,
    model::{
        CallToolRequest, CallToolRequestParams, CallToolResponse, ClientCapabilities, ClientConfig,
        ClientRequest, Cursor, GetPromptRequest, GetPromptRequestParams, GetPromptResult,
        Implementation, InputRequiredResult, JsonObject, ListPromptsRequest,
        ListResourceTemplatesRequest, ListResourcesRequest, ListToolsRequest,
        PaginatedRequestParams, Prompt, ReadResourceRequest, ReadResourceRequestParams,
        ReadResourceResult, Resource, ResourceTemplate, ServerPeerInfo, ServerResult, Tool,
    },
    service::{ClientInitializeError, PeerRequestOptions, RunningService, ServiceError},
    transport::async_rw::AsyncRwTransport,
};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::{
    config::{Server, Target, Transport, Values},
    era::{self, Era, ProtocolVersion, Startup},
    expand::Env,
    server::ServerName,
};

/// The time that `server/discover` gets before the fallback to
/// `initialize`. It is the rmcp value.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// The limit of the whole connect: the server start, the probe, and
/// `initialize` after it.
pub const CONNECT_LIMIT: Duration = Duration::from_secs(30);

/// The default limit of one request after the connect.
pub const REQUEST_LIMIT: Duration = Duration::from_secs(300);

/// The limit to send a cancel request, and to close a session. A server
/// that does not read them must not keep `ncp` from its exit.
const CANCEL_LIMIT: Duration = Duration::from_secs(2);

/// All inputs to connect to one server.
pub struct Launch<'a> {
    pub server: &'a Server,
    pub cached_era: Option<&'a Era>,
    /// The environment for a stdio server, normally the environment of `ncp`.
    pub base_env: &'a Env,
    /// The `env` or `headers` values of the entry, after expansion.
    pub values: &'a Values,
    /// The HTTP client. An http server needs it; a stdio server does not.
    pub http: Option<&'a reqwest::Client>,
    /// Normally [`PROBE_TIMEOUT`].
    pub probe_timeout: Duration,
    /// Normally [`CONNECT_LIMIT`].
    pub connect_limit: Duration,
    /// Where the stderr of a stdio server goes.
    pub server_log: &'a stdio::ServerLog,
    /// Fires on Ctrl-C. The connect then stops with no cancel request.
    pub cancel: &'a CancellationToken,
    /// Gets the messages and the times of the connection.
    pub observer: &'a observe::Observer,
    /// The OAuth client with the stored tokens, for an http server that
    /// needs a login.
    pub authorized: Option<&'a http::Authorized>,
    /// Gets the progress notifications of the requests.
    pub progress: &'a ProgressSink,
    /// Answers an elicitation of the server: a person at a TTY. With no
    /// elicitor, `ncp` says that it cannot answer one.
    pub elicitor: Option<&'a Elicitor>,
}

/// The handshake step that failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Discover,
    Initialize,
    /// `initialize` after `server/discover` got no answer in time. A slow
    /// modern-only server fails here.
    InitializeAfterTimeout(Duration),
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Discover => f.write_str("server/discover"),
            Self::Initialize => f.write_str("initialize"),
            Self::InitializeAfterTimeout(timeout) => write!(
                f,
                "initialize (server/discover got no answer in {} ms)",
                timeout.as_millis()
            ),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error("the sse transport is not supported")]
    SseNotSupported(ServerName),
    #[error(
        "cannot start {command:?}{}: {source}",
        cwd.as_ref().map(|cwd| format!(" in {cwd:?}")).unwrap_or_default()
    )]
    Spawn {
        command: String,
        cwd: Option<String>,
        source: std::io::Error,
    },
    #[error(transparent)]
    Http(#[from] http::HttpError),
    #[error("{phase} failed: {source}")]
    Handshake {
        phase: Phase,
        source: Box<ClientInitializeError>,
    },
    #[error("server gave the protocol version {0:?}, which is not a date")]
    BadVersion(String),
    #[error("the server did not complete the connection in {} s", .0.as_secs())]
    Timeout(Duration),
    #[error("the connect was interrupted")]
    Cancelled,
    #[error("no HTTP client was given for an http server")]
    NoHttpClient,
    #[error("the server {command:?} stopped before the handshake completed ({status})")]
    Exited { command: String, status: String },
}

#[derive(Debug, thiserror::Error)]
pub enum RequestError {
    #[error(transparent)]
    Service(#[from] ServiceError),
    #[error("request cancelled")]
    Cancelled,
    #[error("the server gave no answer in {} s; ncp sent a cancel request", .0.as_secs())]
    Timeout(Duration),
    #[error("server sent a result of the wrong type")]
    UnexpectedResult,
    #[error("server sent a page cursor again, so the list has no end")]
    RepeatedCursor,
}

impl ConnectError {
    /// The error of the store of the OAuth tokens.
    #[must_use]
    pub fn token_store_error(&self) -> Option<String> {
        match self {
            Self::Handshake { source, .. } => match source.as_ref() {
                ClientInitializeError::TransportError { error, .. } => {
                    http::token_store_error(error)
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// The `WWW-Authenticate` challenge of a server that needs a login.
    #[must_use]
    pub fn challenge(&self) -> Option<String> {
        match self {
            Self::Handshake { source, .. } => match source.as_ref() {
                ClientInitializeError::TransportError { error, .. } => http::challenge(error),
                _ => None,
            },
            _ => None,
        }
    }

    /// True when the server needs a login or more scopes.
    #[must_use]
    pub fn needs_authorization(&self) -> bool {
        match self {
            Self::Handshake { source, .. } => match source.as_ref() {
                ClientInitializeError::TransportError { error, .. } => {
                    http::needs_authorization(error)
                }
                _ => false,
            },
            _ => false,
        }
    }
}

impl RequestError {
    /// True when the server needs a login or more scopes.
    #[must_use]
    pub fn needs_authorization(&self) -> bool {
        matches!(self, Self::Service(ServiceError::TransportSend(error)) if http::needs_authorization(error))
    }

    /// The `WWW-Authenticate` challenge of a `401` or `403` answer.
    #[must_use]
    pub fn challenge(&self) -> Option<String> {
        match self {
            Self::Service(ServiceError::TransportSend(error)) => http::challenge(error),
            _ => None,
        }
    }

    /// The error of the store of the OAuth tokens.
    #[must_use]
    pub fn token_store_error(&self) -> Option<String> {
        match self {
            Self::Service(ServiceError::TransportSend(error)) => http::token_store_error(error),
            _ => None,
        }
    }

    /// The challenge of a `401` answer.
    #[must_use]
    pub fn login_challenge(&self) -> Option<String> {
        match self {
            Self::Service(ServiceError::TransportSend(error)) => http::login_challenge(error),
            _ => None,
        }
    }

    /// The scopes of a `403 insufficient_scope` answer.
    #[must_use]
    pub fn required_scopes(&self) -> Option<Vec<String>> {
        match self {
            Self::Service(ServiceError::TransportSend(error)) => http::required_scopes(error),
            _ => None,
        }
    }
}

type Service = RunningService<RoleClient, Handler>;

/// One progress notification of a request (MCP basic/utilities/progress).
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub progress: f64,
    pub total: Option<f64>,
    pub message: Option<String>,
}

/// The function that gets the progress of each request. rmcp adds a
/// progress token to each request.
#[derive(Clone, Default)]
pub struct ProgressSink(Option<Arc<ProgressFunction>>);

type ProgressFunction = dyn Fn(&Progress) + Send + Sync;

impl ProgressSink {
    #[must_use]
    pub fn new(function: impl Fn(&Progress) + Send + Sync + 'static) -> Self {
        Self(Some(Arc::new(function)))
    }
}

impl std::fmt::Debug for ProgressSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ProgressSink")
    }
}

/// Asks the user for an elicitation: the parameters of
/// `elicitation/create` in, the `ElicitResult` out, both as JSON.
pub type Elicitor = Arc<
    dyn Fn(serde_json::Value) -> std::pin::Pin<Box<dyn Future<Output = serde_json::Value> + Send>>
        + Send
        + Sync,
>;

/// The answers to the `input_required` result of a request, for the next
/// round of the same request.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Continuation {
    pub responses: rmcp::model::InputResponses,
    pub state: Option<String>,
}

/// The client side of the session: the client information, the progress
/// of each request, and the answer to an elicitation of a legacy server.
#[derive(Clone)]
struct Handler {
    info: ClientConfig,
    progress: ProgressSink,
    elicitor: Option<Elicitor>,
    /// True while a request waits for its answer. `ncp` sends one request
    /// at a time, so a notification out of that time is for no active
    /// request (the spec: a client SHOULD track active progress tokens).
    active: Arc<AtomicBool>,
}

/// Marks a request as active until the value is dropped, at each exit.
struct ActiveRequest(Arc<AtomicBool>);

impl ActiveRequest {
    fn start(flag: &Arc<AtomicBool>) -> Self {
        flag.store(true, Ordering::SeqCst);
        Self(flag.clone())
    }
}

impl Drop for ActiveRequest {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl rmcp::ClientHandler for Handler {
    fn get_info(&self) -> ClientConfig {
        self.info.clone()
    }

    fn on_progress(
        &self,
        params: rmcp::model::ProgressNotificationParam,
        _context: rmcp::service::NotificationContext<RoleClient>,
    ) -> impl Future<Output = ()> + rmcp::service::MaybeSendFuture + '_ {
        if let (Some(function), true) = (&self.progress.0, self.active.load(Ordering::SeqCst)) {
            function(&Progress {
                progress: params.progress,
                total: params.total,
                message: params.message,
            });
        }
        std::future::ready(())
    }

    /// A legacy server asks with a request of its own during a request of
    /// `ncp` (MCP 2025-11-25, client/elicitation).
    async fn create_elicitation(
        &self,
        request: rmcp::model::ElicitRequestParams,
        _context: rmcp::service::RequestContext<RoleClient>,
    ) -> Result<rmcp::model::ElicitResult, rmcp::ErrorData> {
        let decline = || serde_json::json!({"action": "decline"});
        let answer = match (&self.elicitor, serde_json::to_value(&request)) {
            (Some(elicitor), Ok(params)) => elicitor(params).await,
            _ => decline(),
        };
        serde_json::from_value(answer)
            .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))
    }
}

pub struct Connection {
    service: Service,
    era: Era,
    process: Option<stdio::Process>,
    /// True when the connection sends the OAuth token of a login.
    authorized: bool,
}

/// Connects to the server, and finds its era.
///
/// # Errors
///
/// Returns an error when the server cannot start, when no handshake
/// succeeds, or when the connect passes its limit.
pub async fn connect(launch: &Launch<'_>) -> Result<Connection, ConnectError> {
    let deadline = Instant::now() + launch.connect_limit;
    match era::startup(launch.cached_era) {
        Startup::Probe => probe(launch, ProtocolVersion::first_modern(), deadline).await,
        Startup::Discover(version) => probe(launch, version, deadline).await,
        Startup::Initialize(version) => {
            match initialize(launch, version, Phase::Initialize, deadline).await {
                // The server can be modern now: probe again. Over HTTP the
                // rejection of `initialize` is not always a JSON-RPC error.
                Err(ConnectError::Handshake { .. }) => {
                    probe(launch, ProtocolVersion::first_modern(), deadline).await
                }
                other => other,
            }
        }
    }
}

async fn probe(
    launch: &Launch<'_>,
    preferred: ProtocolVersion,
    deadline: Instant,
) -> Result<Connection, ConnectError> {
    let mut preferred_versions = vec![to_rmcp(&preferred)];
    let first_modern = to_rmcp(&ProtocolVersion::first_modern());
    if !preferred_versions.contains(&first_modern) {
        preferred_versions.push(first_modern);
    }
    let lifecycle = ClientLifecycleMode::Discover { preferred_versions };
    let attempt = handshake(
        launch,
        lifecycle,
        client_info(launch.elicitor.is_some()),
        Phase::Discover,
        deadline,
    );
    // On a probe timeout the future is dropped. Its stdio process stops by
    // kill-on-drop, so a late answer cannot reach the next connection. When
    // the connect limit comes first, the handshake stops itself.
    let probe_deadline = Instant::now() + launch.probe_timeout;
    let answer = if probe_deadline < deadline {
        tokio::time::timeout_at(probe_deadline, attempt).await.ok()
    } else {
        Some(attempt.await)
    };
    let latest_legacy = ProtocolVersion::latest_legacy();
    match answer {
        Some(Err(ConnectError::Handshake { source, .. })) if is_legacy_answer(&source) => {
            initialize(launch, latest_legacy, Phase::Initialize, deadline).await
        }
        Some(result) => result,
        None => {
            let phase = Phase::InitializeAfterTimeout(launch.probe_timeout);
            initialize(launch, latest_legacy, phase, deadline).await
        }
    }
}

async fn initialize(
    launch: &Launch<'_>,
    version: ProtocolVersion,
    phase: Phase,
    deadline: Instant,
) -> Result<Connection, ConnectError> {
    let info = client_info(launch.elicitor.is_some()).with_protocol_version(to_rmcp(&version));
    handshake(
        launch,
        ClientLifecycleMode::Initialize,
        info,
        phase,
        deadline,
    )
    .await
}

/// Starts a new transport and runs one handshake on it, until the deadline.
///
/// After the deadline a stdio server gets the stop steps, and no cancel
/// request: the spec says that a client MUST NOT cancel `initialize`
/// (MCP 2025-11-25, basic/utilities/cancellation).
async fn handshake(
    launch: &Launch<'_>,
    lifecycle: ClientLifecycleMode,
    info: ClientConfig,
    phase: Phase,
    deadline: Instant,
) -> Result<Connection, ConnectError> {
    let info = Handler {
        info,
        progress: launch.progress.clone(),
        elicitor: launch.elicitor.cloned(),
        active: Arc::default(),
    };
    let failed = |source| ConnectError::Handshake {
        phase,
        source: Box::new(source),
    };
    let timeout = ConnectError::Timeout(launch.connect_limit);
    let (service, process) = match &launch.server.transport {
        Transport::Stdio(server) => {
            let env = stdio::child_env(launch.base_env, &launch.values.env);
            let started = Instant::now();
            let (process, stdout, stdin) =
                stdio::spawn(server, &env, launch.server_log).map_err(|source| {
                    ConnectError::Spawn {
                        // The command as the file writes it: an expanded
                        // value must not show.
                        command: configured_command(launch.server),
                        cwd: match &launch.server.target {
                            Target::Stdio { cwd, .. } => cwd.clone(),
                            Target::Remote { .. } => None,
                        },
                        source,
                    }
                })?;
            launch.observer.emit(&observe::Event::ServerStart {
                elapsed: started.elapsed(),
            });
            let transport = observe::Observed::new(
                AsyncRwTransport::new_client(stdout, stdin),
                launch.observer.clone(),
            );
            let serve = info.serve_with_lifecycle(transport, lifecycle);
            // The handshake future owns stdin. It ends with the race, so
            // stdin is closed before the stop steps.
            match race(serve, deadline, launch.cancel).await {
                Ended::Done(Ok(service)) => (service, Some(process)),
                // A failed handshake can be a server that stopped: a closed
                // connection, or a broken pipe on Linux. Its exit status
                // says why. The answer of a legacy server stays as it is,
                // so that the probe can fall back.
                Ended::Done(Err(source)) if !is_legacy_answer(&source) => {
                    let mut process = process;
                    let exited = process.exit_status().await;
                    process.stop().await;
                    return Err(match exited {
                        Some(status) => ConnectError::Exited {
                            command: configured_command(launch.server),
                            status: status.to_string(),
                        },
                        None => failed(source),
                    });
                }
                Ended::Done(Err(source)) => {
                    process.stop().await;
                    return Err(failed(source));
                }
                ended => {
                    process.stop().await;
                    return Err(ended.into_error(timeout));
                }
            }
        }
        Transport::Http(server) => {
            let headers = &launch.values.headers;
            let ended = if let Some(authorized) = launch.authorized {
                let transport =
                    http::transport(authorized.clone(), server, headers, launch.observer)?;
                race(
                    info.serve_with_lifecycle(transport, lifecycle),
                    deadline,
                    launch.cancel,
                )
                .await
            } else {
                let client = http::observed(
                    launch.http.ok_or(ConnectError::NoHttpClient)?,
                    launch.observer,
                );
                let transport = http::transport(client, server, headers, launch.observer)?;
                race(
                    info.serve_with_lifecycle(transport, lifecycle),
                    deadline,
                    launch.cancel,
                )
                .await
            };
            match ended {
                Ended::Done(service) => (service.map_err(failed)?, None),
                ended => return Err(ended.into_error(timeout)),
            }
        }
        Transport::Sse(_) => return Err(ConnectError::SseNotSupported(launch.server.name.clone())),
    };
    let era = era_of(&service)?;
    Ok(Connection {
        service,
        era,
        process,
        authorized: launch.authorized.is_some(),
    })
}

/// The command as the file writes it: an expanded value must not show.
fn configured_command(server: &Server) -> String {
    match &server.target {
        Target::Stdio { command, .. } => command.clone(),
        Target::Remote { .. } => String::new(),
    }
}

/// How a handshake ended.
enum Ended<T> {
    Done(T),
    TimedOut,
    Cancelled,
}

impl<T> Ended<T> {
    fn into_error(self, timeout: ConnectError) -> ConnectError {
        match self {
            Self::Cancelled => ConnectError::Cancelled,
            Self::Done(_) | Self::TimedOut => timeout,
        }
    }
}

/// Runs the handshake until it ends, the deadline passes, or Ctrl-C.
async fn race<T>(
    handshake: impl Future<Output = T>,
    deadline: Instant,
    cancel: &CancellationToken,
) -> Ended<T> {
    tokio::select! {
        result = tokio::time::timeout_at(deadline, handshake) => {
            result.map_or(Ended::TimedOut, Ended::Done)
        }
        () = cancel.cancelled() => Ended::Cancelled,
    }
}

/// The client information. A client with a person at a TTY says that it
/// can answer elicitation in form and URL mode; else a server must not ask.
fn client_info(elicitation: bool) -> ClientConfig {
    let mut capabilities = ClientCapabilities::default();
    if elicitation {
        capabilities.elicitation =
            serde_json::from_value(serde_json::json!({"form": {}, "url": {}})).ok();
    }
    ClientConfig::new(
        capabilities,
        Implementation::new("ncp", env!("CARGO_PKG_VERSION")),
    )
}

fn era_of(service: &Service) -> Result<Era, ConnectError> {
    let text = service
        .peer_info()
        .map(|info| info.protocol_version.as_str().to_string())
        .unwrap_or_default();
    let version = ProtocolVersion::parse(&text).map_err(|_| ConnectError::BadVersion(text))?;
    Ok(if version.is_modern() {
        Era::Modern { version }
    } else {
        Era::Legacy { version }
    })
}

/// A correlated JSON-RPC error to the probe means a legacy server. So does
/// an answer that is not a discover result: a legacy server can answer an
/// unknown method with an empty result.
fn is_legacy_answer(error: &ClientInitializeError) -> bool {
    matches!(
        error,
        ClientInitializeError::JsonRpcError(_) | ClientInitializeError::ExpectedInitResult(_)
    )
}

fn to_rmcp(version: &ProtocolVersion) -> rmcp::model::ProtocolVersion {
    // rmcp has no public constructor for a version. Its deserializer accepts
    // any string, so this conversion cannot fail.
    serde_json::from_value(serde_json::Value::String(version.as_str().to_string()))
        .unwrap_or(rmcp::model::ProtocolVersion::LATEST_WITH_INITIALIZE)
}

/// The cache hints of a list (MCP 2026-07-28, server/utilities/caching):
/// the time that it stays fresh, and if it is for one user only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheHints {
    pub ttl_ms: Option<u64>,
    /// A private list must not be shared across authorization contexts.
    /// A list with no scope counts as private: that is the safe reading.
    pub private: bool,
}

impl CacheHints {
    /// No hints: not fresh, private.
    pub const NONE: Self = Self {
        ttl_ms: None,
        private: true,
    };

    #[must_use]
    pub fn of(ttl_ms: Option<u64>, scope: Option<&rmcp::model::CacheScope>) -> Self {
        Self {
            ttl_ms,
            private: !matches!(scope, Some(rmcp::model::CacheScope::Public)),
        }
    }

    /// The hints of two pages: the shorter time, and private when one page
    /// is private.
    #[must_use]
    pub fn and(self, other: Self) -> Self {
        Self {
            ttl_ms: self.ttl_ms.zip(other.ttl_ms).map(|(a, b)| a.min(b)),
            private: self.private || other.private,
        }
    }
}

/// The answer to a request that can ask for more input (multi-round
/// requests): the result, or the `input_required` result for the caller.
#[derive(Debug)]
pub enum Answer<T> {
    Complete(T),
    InputRequired(InputRequiredResult),
}

/// How a request can end early: the cancel token of the caller (Ctrl-C),
/// and the time limit.
#[derive(Debug, Clone, Copy)]
pub struct RequestLimits<'a> {
    pub cancel: &'a CancellationToken,
    /// Normally [`REQUEST_LIMIT`].
    pub timeout: Duration,
}

impl Connection {
    #[must_use]
    pub fn era(&self) -> &Era {
        &self.era
    }

    /// True when the connection sends the OAuth token of a login.
    #[must_use]
    pub fn is_authorized(&self) -> bool {
        self.authorized
    }

    /// What the server said about itself in the handshake.
    #[must_use]
    pub fn server_info(&self) -> Option<std::sync::Arc<ServerPeerInfo>> {
        self.service.peer_info()
    }

    /// Lists all tools. Each page is one request with its own limits.
    ///
    /// # Errors
    ///
    /// Returns an error when a `tools/list` request fails, is cancelled or
    /// passes its time limit.
    pub async fn list_tools(&self, limits: RequestLimits<'_>) -> Result<Vec<Tool>, RequestError> {
        Ok(self.list_tools_fresh_for(limits).await?.0)
    }

    /// Lists all tools, with the cache hints of the pages.
    ///
    /// # Errors
    ///
    /// Returns an error when a `tools/list` request fails.
    pub async fn list_tools_fresh_for(
        &self,
        limits: RequestLimits<'_>,
    ) -> Result<(Vec<Tool>, CacheHints), RequestError> {
        self.pages(
            limits,
            |params| ClientRequest::ListToolsRequest(ListToolsRequest::with_param(params)),
            |result| match result {
                ServerResult::ListToolsResult(page) => {
                    let hints = CacheHints::of(page.ttl_ms, page.cache_scope.as_ref());
                    Some((page.tools, page.next_cursor, hints))
                }
                _ => None,
            },
        )
        .await
    }

    /// Lists all resources.
    ///
    /// # Errors
    ///
    /// Returns an error when a `resources/list` request fails.
    pub async fn list_resources(
        &self,
        limits: RequestLimits<'_>,
    ) -> Result<Vec<Resource>, RequestError> {
        self.pages(
            limits,
            |params| ClientRequest::ListResourcesRequest(ListResourcesRequest::with_param(params)),
            |result| match result {
                ServerResult::ListResourcesResult(page) => {
                    Some((page.resources, page.next_cursor, CacheHints::NONE))
                }
                _ => None,
            },
        )
        .await
        .map(|(items, _)| items)
    }

    /// Lists all resource templates.
    ///
    /// # Errors
    ///
    /// Returns an error when a `resources/templates/list` request fails.
    pub async fn list_resource_templates(
        &self,
        limits: RequestLimits<'_>,
    ) -> Result<Vec<ResourceTemplate>, RequestError> {
        self.pages(
            limits,
            |params| {
                ClientRequest::ListResourceTemplatesRequest(
                    ListResourceTemplatesRequest::with_param(params),
                )
            },
            |result| match result {
                ServerResult::ListResourceTemplatesResult(page) => {
                    Some((page.resource_templates, page.next_cursor, CacheHints::NONE))
                }
                _ => None,
            },
        )
        .await
        .map(|(items, _)| items)
    }

    /// Lists all prompts.
    ///
    /// # Errors
    ///
    /// Returns an error when a `prompts/list` request fails.
    pub async fn list_prompts(
        &self,
        limits: RequestLimits<'_>,
    ) -> Result<Vec<Prompt>, RequestError> {
        self.pages(
            limits,
            |params| ClientRequest::ListPromptsRequest(ListPromptsRequest::with_param(params)),
            |result| match result {
                ServerResult::ListPromptsResult(page) => {
                    Some((page.prompts, page.next_cursor, CacheHints::NONE))
                }
                _ => None,
            },
        )
        .await
        .map(|(items, _)| items)
    }

    /// Reads one resource. An `input_required` result goes back to the
    /// caller, as for a tool call.
    ///
    /// # Errors
    ///
    /// Returns an error when the request fails.
    pub async fn read_resource(
        &self,
        uri: &str,
        continuation: Option<&Continuation>,
        limits: RequestLimits<'_>,
    ) -> Result<Answer<ReadResourceResult>, RequestError> {
        let mut params = ReadResourceRequestParams::new(uri);
        if let Some(continuation) = continuation {
            params.input_responses = Some(continuation.responses.clone());
            params.request_state.clone_from(&continuation.state);
        }
        let request = ClientRequest::ReadResourceRequest(ReadResourceRequest::new(params));
        match self.request(request, limits).await? {
            ServerResult::ReadResourceResult(result) => Ok(Answer::Complete(result)),
            ServerResult::InputRequiredResult(result) => Ok(Answer::InputRequired(result)),
            _ => Err(RequestError::UnexpectedResult),
        }
    }

    /// Gets one prompt with its arguments.
    ///
    /// # Errors
    ///
    /// Returns an error when the request fails.
    pub async fn get_prompt(
        &self,
        name: &str,
        arguments: JsonObject,
        continuation: Option<&Continuation>,
        limits: RequestLimits<'_>,
    ) -> Result<Answer<GetPromptResult>, RequestError> {
        let mut params = GetPromptRequestParams::new(name);
        if let Some(continuation) = continuation {
            params.input_responses = Some(continuation.responses.clone());
            params.request_state.clone_from(&continuation.state);
        }
        params.arguments = (!arguments.is_empty()).then_some(arguments);
        let request = ClientRequest::GetPromptRequest(GetPromptRequest::new(params));
        match self.request(request, limits).await? {
            ServerResult::GetPromptResult(result) => Ok(Answer::Complete(result)),
            ServerResult::InputRequiredResult(result) => Ok(Answer::InputRequired(result)),
            _ => Err(RequestError::UnexpectedResult),
        }
    }

    /// All pages of a list. Each page is one request with its own limits
    /// (the spec: the time limit applies to each request).
    async fn pages<T>(
        &self,
        limits: RequestLimits<'_>,
        request: impl Fn(PaginatedRequestParams) -> ClientRequest,
        page: impl Fn(ServerResult) -> Option<(Vec<T>, Option<Cursor>, CacheHints)>,
    ) -> Result<(Vec<T>, CacheHints), RequestError> {
        let mut items = Vec::new();
        let mut hints: Option<CacheHints> = None;
        let mut cursor = None;
        // A server that gives a cursor again would make the loop endless.
        let mut seen = std::collections::HashSet::new();
        loop {
            let params = PaginatedRequestParams::default().with_cursor(cursor);
            let result = self.request(request(params), limits).await?;
            let (found, next, page_hints) = page(result).ok_or(RequestError::UnexpectedResult)?;
            items.extend(found);
            hints = Some(hints.map_or(page_hints, |earlier| earlier.and(page_hints)));
            match next {
                None => return Ok((items, hints.unwrap_or(CacheHints::NONE))),
                Some(next) if !seen.insert(next.clone()) => {
                    return Err(RequestError::RepeatedCursor);
                }
                Some(next) => cursor = Some(next),
            }
        }
    }

    /// Calls a tool.
    ///
    /// The result is the first response, with no follow-up rounds: an
    /// `input_required` result goes back to the caller.
    ///
    /// # Errors
    ///
    /// Returns an error when the request fails, is cancelled or passes its
    /// time limit.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: JsonObject,
        continuation: Option<&Continuation>,
        limits: RequestLimits<'_>,
    ) -> Result<CallToolResponse, RequestError> {
        let mut params = CallToolRequestParams::new(name.to_string()).with_arguments(arguments);
        if let Some(continuation) = continuation {
            params.input_responses = Some(continuation.responses.clone());
            params.request_state.clone_from(&continuation.state);
        }
        let request = ClientRequest::CallToolRequest(CallToolRequest::new(params));
        match self.request(request, limits).await? {
            ServerResult::CallToolResult(result) => Ok(CallToolResponse::Complete(result)),
            ServerResult::InputRequiredResult(result) => {
                Ok(CallToolResponse::InputRequired(result))
            }
            ServerResult::CreateTaskResult(result) => Ok(CallToolResponse::Task(result)),
            _ => Err(RequestError::UnexpectedResult),
        }
    }

    /// Sends one request. When the cancel token fires or the time limit
    /// passes, sends `notifications/cancelled` for the request.
    async fn request(
        &self,
        request: ClientRequest,
        limits: RequestLimits<'_>,
    ) -> Result<ServerResult, RequestError> {
        // rmcp gives each notification to its own task. The runtime of
        // `ncp` has one thread, so a notification task runs before or after
        // this function, and the flag says which.
        let active = ActiveRequest::start(&self.service.service().active);
        let mut handle = self
            .service
            .peer()
            .send_cancellable_request(request, PeerRequestOptions::no_options())
            .await?;
        let (reason, error) = tokio::select! {
            received = &mut handle.rx => {
                return Ok(received.map_err(|_| ServiceError::TransportClosed)??);
            }
            () = limits.cancel.cancelled() => ("user interrupt", RequestError::Cancelled),
            () = tokio::time::sleep(limits.timeout) => {
                ("request timeout", RequestError::Timeout(limits.timeout))
            }
        };
        // The request has ended for the user. A failed or slow cancel
        // request does not change that result, and its progress no longer
        // shows.
        drop(active);
        let _ = tokio::time::timeout(CANCEL_LIMIT, handle.cancel(Some(reason.into()))).await;
        Err(error)
    }

    /// The process ID of a stdio server.
    #[must_use]
    pub fn process_id(&self) -> Option<u32> {
        self.process.as_ref().and_then(stdio::Process::id)
    }

    /// Closes the connection. A stdio server gets the stop steps of the spec.
    /// The close of the session has a time limit, so that a server that
    /// does not answer cannot keep `ncp` from its exit.
    pub async fn close(self) {
        let _ = tokio::time::timeout(CANCEL_LIMIT, self.service.cancel()).await;
        if let Some(process) = self.process {
            process.stop().await;
        }
    }
}
