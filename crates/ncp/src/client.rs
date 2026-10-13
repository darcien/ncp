//! The client core: connect to a server, find its protocol version, list
//! and call tools.
//!
//! Version detection: `ncp` probes with `server/discover` in its own
//! timeout. When the server does not know `server/discover`, or the probe
//! times out, `ncp` connects again on a new transport with `initialize`.
//! `ncp` does not use the rmcp `Auto` mode. With that mode, a late answer
//! of a slow server arrives on the same transport, and the handshake
//! fails (see docs/research/early-tests.md).
//!
//! Time limits: a client SHOULD set a timeout for each request (MCP
//! 2025-11-25, basic/lifecycle, "Timeouts"). The whole connect has one
//! limit, and each later request has its own limit, because an agent must
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
    expand::Env,
    protocol_version::{self, ProtocolVersion, Startup},
    server::ServerName,
};

/// The time that `server/discover` gets before the fallback to
/// `initialize`. It is the value of rmcp.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// The limit of the whole connect: the server start, the probe, and
/// `initialize` after it.
pub const CONNECT_LIMIT: Duration = Duration::from_secs(30);

/// The default limit of one request after the connect.
pub const REQUEST_LIMIT: Duration = Duration::from_secs(300);

/// The limit to send a cancel request, and to close a session. A server
/// that does not read them must not keep `ncp` from its exit.
const CANCEL_LIMIT: Duration = Duration::from_secs(2);

pub struct Launch<'a> {
    pub server: &'a Server,
    pub cached_version: Option<&'a ProtocolVersion>,
    /// The environment for a stdio server, normally the environment of `ncp`.
    pub base_env: &'a Env,
    /// The `env` or `headers` values of the entry, after expansion.
    pub values: &'a Values,
    pub http: Option<&'a reqwest::Client>,
    /// Normally [`PROBE_TIMEOUT`].
    pub probe_timeout: Duration,
    /// Normally [`CONNECT_LIMIT`].
    pub connect_limit: Duration,
    pub server_log: &'a stdio::ServerLog,
    /// On Ctrl-C, the connect stops with no cancel request.
    pub cancel: &'a CancellationToken,
    pub observer: &'a observe::Observer,
    /// Only for an http server that needs a login.
    pub authorized: Option<&'a http::Authorized>,
    pub progress: &'a ProgressSink,
    /// A person at a TTY. With no elicitor, `ncp` tells the server that it
    /// cannot answer an elicitation.
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

/// One progress notification of a request (MCP 2025-11-25,
/// basic/utilities/progress).
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub progress: f64,
    pub total: Option<f64>,
    pub message: Option<String>,
}

/// rmcp adds a progress token to each request.
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
    /// request. A client SHOULD track active progress tokens (MCP
    /// 2025-11-25, basic/utilities/progress).
    active: Arc<AtomicBool>,
    /// True after `ncp` refused a question of a server, because it has no
    /// one to ask.
    refused: Arc<AtomicBool>,
}

/// Marks a request as active until the drop of the value, at each exit.
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

impl Handler {
    fn new(launch: &Launch<'_>, info: ClientConfig) -> Self {
        Self {
            info,
            progress: launch.progress.clone(),
            elicitor: launch.elicitor.cloned(),
            active: Arc::default(),
            refused: Arc::default(),
        }
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
        let Some(elicitor) = &self.elicitor else {
            // MCP 2025-11-25, client/elicitation: a request for a mode that
            // the client did not declare gets "Invalid params".
            self.refused.store(true, Ordering::SeqCst);
            return Err(rmcp::ErrorData::invalid_params(
                "the client declared no elicitation: it has no terminal to ask",
                None,
            ));
        };
        let params = serde_json::to_value(&request)
            .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))?;
        serde_json::from_value(elicitor(params).await)
            .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))
    }
}

pub struct Connection {
    service: Service,
    version: ProtocolVersion,
    process: Option<stdio::Process>,
    authorized: bool,
    refused: Arc<AtomicBool>,
}

/// Connects to the server and finds its protocol version.
///
/// # Errors
///
/// Returns an error when the server cannot start, when no handshake
/// succeeds, or when the connect passes its limit.
pub async fn connect(launch: &Launch<'_>) -> Result<Connection, ConnectError> {
    let deadline = Instant::now() + launch.connect_limit;
    match protocol_version::startup(launch.cached_version) {
        Startup::Probe => probe(launch, ProtocolVersion::first_with_discover(), deadline).await,
        Startup::Discover(version) => probe(launch, version, deadline).await,
        Startup::Initialize(version) => {
            match initialize(launch, version, Phase::Initialize, deadline).await {
                // The cached version is old if the server now knows
                // `server/discover`. Over HTTP, the rejection of
                // `initialize` is not always a JSON-RPC error, so each
                // handshake failure probes again.
                Err(ConnectError::Handshake { .. }) => {
                    probe(launch, ProtocolVersion::first_with_discover(), deadline).await
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
    let first_with_discover = to_rmcp(&ProtocolVersion::first_with_discover());
    if !preferred_versions.contains(&first_with_discover) {
        preferred_versions.push(first_with_discover);
    }
    let lifecycle = ClientLifecycleMode::Discover { preferred_versions };
    // Boxed: the handshake future is large, and the caller holds two.
    let attempt = Box::pin(handshake(
        launch,
        lifecycle,
        client_info(launch.elicitor.is_some()),
        Phase::Discover,
        deadline,
    ));
    // On a probe timeout, `ncp` drops the future. Its stdio process stops by
    // kill-on-drop, so a late answer cannot reach the next connection. When the
    // connect limit comes first, the handshake stops itself.
    let probe_deadline = Instant::now() + launch.probe_timeout;
    let answer = if probe_deadline < deadline {
        tokio::time::timeout_at(probe_deadline, attempt).await.ok()
    } else {
        Some(attempt.await)
    };
    let latest_with_initialize = ProtocolVersion::latest_with_initialize();
    match answer {
        Some(Err(ConnectError::Handshake { source, .. })) if is_legacy_answer(&source) => {
            initialize(launch, latest_with_initialize, Phase::Initialize, deadline).await
        }
        Some(result) => result,
        None => {
            let phase = Phase::InitializeAfterTimeout(launch.probe_timeout);
            initialize(launch, latest_with_initialize, phase, deadline).await
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
    Box::pin(handshake(
        launch,
        ClientLifecycleMode::Initialize,
        info,
        phase,
        deadline,
    ))
    .await
}

/// After the deadline, a stdio server gets the stop steps and no cancel
/// request, because a client MUST NOT cancel `initialize` (MCP 2025-11-25,
/// basic/utilities/cancellation).
async fn handshake(
    launch: &Launch<'_>,
    lifecycle: ClientLifecycleMode,
    info: ClientConfig,
    phase: Phase,
    deadline: Instant,
) -> Result<Connection, ConnectError> {
    let info = Handler::new(launch, info);
    let refused = info.refused.clone();
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
            // The handshake future owns stdin. It ends with the race, so stdin
            // closes before the stop steps.
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
    let version = version_of(&service)?;
    Ok(Connection {
        service,
        version,
        process,
        authorized: launch.authorized.is_some(),
        refused,
    })
}

/// The command as the file has it, because an expanded value must not show.
fn configured_command(server: &Server) -> String {
    match &server.target {
        Target::Stdio { command, .. } => command.clone(),
        Target::Remote { .. } => String::new(),
    }
}

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

/// Only a client with a person at a TTY declares elicitation in form and
/// URL mode. A server must not ask a client that has no one to answer.
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

fn version_of(service: &Service) -> Result<ProtocolVersion, ConnectError> {
    let text = service
        .peer_info()
        .map(|info| info.protocol_version.as_str().to_string())
        .unwrap_or_default();
    ProtocolVersion::parse(&text).map_err(|_| ConnectError::BadVersion(text))
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
    /// A client must not share a private list across authorization contexts. A
    /// list with no scope counts as private: that is the safe reading.
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
    pub fn protocol_version(&self) -> &ProtocolVersion {
        &self.version
    }

    #[must_use]
    pub fn is_authorized(&self) -> bool {
        self.authorized
    }

    /// `ncp` refuses a question of a server when it has no one to ask.
    #[must_use]
    pub fn refused_a_question(&self) -> bool {
        self.refused.load(Ordering::SeqCst)
    }

    #[must_use]
    pub fn server_info(&self) -> Option<std::sync::Arc<ServerPeerInfo>> {
        self.service.peer_info()
    }

    /// # Errors
    ///
    /// Returns an error when a `tools/list` request fails, gets a cancel, or
    /// passes its time limit.
    pub async fn list_tools(&self, limits: RequestLimits<'_>) -> Result<Vec<Tool>, RequestError> {
        Ok(self.list_tools_fresh_for(limits).await?.0)
    }

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

    /// An `input_required` result goes back to the caller, as for a tool
    /// call.
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

    /// Each page is one request with its own limits, because the spec
    /// applies the time limit to each request.
    async fn pages<T>(
        &self,
        limits: RequestLimits<'_>,
        request: impl Fn(PaginatedRequestParams) -> ClientRequest,
        page: impl Fn(ServerResult) -> Option<(Vec<T>, Option<Cursor>, CacheHints)>,
    ) -> Result<(Vec<T>, CacheHints), RequestError> {
        let mut items = Vec::new();
        let mut hints: Option<CacheHints> = None;
        let mut cursor = None;
        // A server that gives a cursor again makes the loop endless.
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

    /// The result is the first response, with no follow-up rounds: an
    /// `input_required` result goes back to the caller.
    ///
    /// # Errors
    ///
    /// Returns an error when the request fails, gets a cancel, or passes its
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

    /// On Ctrl-C or at the time limit, sends `notifications/cancelled` for
    /// the request.
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
        // The request ended for the user. A failed or slow cancel request does
        // not change that result, and its progress no longer shows.
        drop(active);
        let _ = tokio::time::timeout(CANCEL_LIMIT, handle.cancel(Some(reason.into()))).await;
        Err(error)
    }

    /// The process ID of a stdio server.
    #[must_use]
    pub fn process_id(&self) -> Option<u32> {
        self.process.as_ref().and_then(stdio::Process::id)
    }

    /// A stdio server gets the stop steps of the spec. The close of the
    /// session has a time limit, so that a server that does not answer
    /// cannot keep `ncp` from its exit.
    pub async fn close(self) {
        let _ = tokio::time::timeout(CANCEL_LIMIT, self.service.cancel()).await;
        if let Some(process) = self.process {
            process.stop().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_of_pages_take_the_shorter_time_and_private_from_any_page() {
        let public = |ttl| CacheHints {
            ttl_ms: ttl,
            private: false,
        };
        assert_eq!(public(Some(5)).and(public(Some(3))), public(Some(3)));
        assert_eq!(public(Some(5)).and(public(None)), public(None));
        assert!(public(Some(5)).and(CacheHints::of(Some(5), None)).private);
        assert!(
            !CacheHints::of(Some(5), Some(&rmcp::model::CacheScope::Public))
                .and(public(Some(5)))
                .private
        );
    }
}
