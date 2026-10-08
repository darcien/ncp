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

use std::{fmt, time::Duration};

use rmcp::{
    ClientLifecycleMode, ClientServiceExt, RoleClient,
    model::{
        CallToolRequest, CallToolRequestParams, CallToolResponse, ClientCapabilities, ClientConfig,
        ClientRequest, Implementation, JsonObject, ListToolsRequest, PaginatedRequestParams,
        ServerPeerInfo, ServerResult, Tool,
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
}

impl ConnectError {
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

    /// The scopes of a `403 insufficient_scope` answer.
    #[must_use]
    pub fn required_scopes(&self) -> Option<Vec<String>> {
        match self {
            Self::Service(ServiceError::TransportSend(error)) => http::required_scopes(error),
            _ => None,
        }
    }
}

type Service = RunningService<RoleClient, ClientConfig>;

pub struct Connection {
    service: Service,
    era: Era,
    process: Option<stdio::Process>,
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
    let attempt = handshake(launch, lifecycle, client_info(), Phase::Discover, deadline);
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
        Some(Err(ConnectError::Handshake { source, .. })) if is_json_rpc_error(&source) => {
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
    let info = client_info().with_protocol_version(to_rmcp(&version));
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
                        command: match &launch.server.target {
                            Target::Stdio { command, .. } => command.clone(),
                            Target::Remote { .. } => String::new(),
                        },
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
                Ended::Done(service) => (service.map_err(failed)?, Some(process)),
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
    })
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

fn client_info() -> ClientConfig {
    ClientConfig::new(
        ClientCapabilities::default(),
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

/// A correlated JSON-RPC error to the probe means a legacy server.
fn is_json_rpc_error(error: &ClientInitializeError) -> bool {
    matches!(error, ClientInitializeError::JsonRpcError(_))
}

fn to_rmcp(version: &ProtocolVersion) -> rmcp::model::ProtocolVersion {
    // rmcp has no public constructor for a version. Its deserializer accepts
    // any string, so this conversion cannot fail.
    serde_json::from_value(serde_json::Value::String(version.as_str().to_string()))
        .unwrap_or(rmcp::model::ProtocolVersion::LATEST_WITH_INITIALIZE)
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
        let mut tools = Vec::new();
        let mut cursor = None;
        loop {
            let params = PaginatedRequestParams::default().with_cursor(cursor);
            let request = ClientRequest::ListToolsRequest(ListToolsRequest::with_param(params));
            let ServerResult::ListToolsResult(page) = self.request(request, limits).await? else {
                return Err(RequestError::UnexpectedResult);
            };
            tools.extend(page.tools);
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(tools);
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
        limits: RequestLimits<'_>,
    ) -> Result<CallToolResponse, RequestError> {
        let params = CallToolRequestParams::new(name.to_string()).with_arguments(arguments);
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
        // request does not change that result.
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
