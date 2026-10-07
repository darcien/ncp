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
pub mod stdio;

use std::{collections::BTreeMap, fmt, time::Duration};

use rmcp::{
    ClientLifecycleMode, ClientServiceExt, RoleClient,
    model::{
        CallToolRequest, CallToolRequestParams, CallToolResponse, ClientCapabilities, ClientConfig,
        ClientRequest, Implementation, JsonObject, ListToolsRequest, PaginatedRequestParams,
        ServerResult, Tool,
    },
    service::{ClientInitializeError, PeerRequestOptions, RunningService, ServiceError},
};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::{
    config::{Server, Transport},
    credential::{CredentialKey, EnvName, HeaderName, Secret, SecretKind},
    era::{self, Era, ProtocolVersion, Startup},
    expand::Env,
    server::ServerName,
    store::{CredentialStore, StoreError},
};

/// The time that `server/discover` gets before the fallback to
/// `initialize`. It is the rmcp value.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// The limit of the whole connect: the server start, the probe, and
/// `initialize` after it.
pub const CONNECT_LIMIT: Duration = Duration::from_secs(30);

/// The default limit of one request after the connect.
pub const REQUEST_LIMIT: Duration = Duration::from_secs(300);

/// The limit to send a cancel request. A server that does not read it must
/// not keep `ncp` from its exit.
const CANCEL_LIMIT: Duration = Duration::from_secs(2);

/// The secrets of one server, read from the credential store.
#[derive(Debug, Default)]
pub struct Secrets {
    pub env: BTreeMap<EnvName, Secret>,
    pub headers: BTreeMap<HeaderName, Secret>,
}

impl Secrets {
    /// Reads the named secrets. A name with no stored value is left out.
    ///
    /// # Errors
    ///
    /// Returns an error when the store cannot be read.
    pub fn load(
        store: &dyn CredentialStore,
        server: &ServerName,
        env: &[EnvName],
        headers: &[HeaderName],
    ) -> Result<Self, StoreError> {
        let read = |kind: SecretKind| store.load(&CredentialKey::new(server.clone(), kind));
        let env = load_each(env, |name| read(SecretKind::Env(name)))?;
        let headers = load_each(headers, |name| read(SecretKind::Header(name)))?;
        Ok(Self { env, headers })
    }
}

fn load_each<N: Clone + Ord>(
    names: &[N],
    read: impl Fn(N) -> Result<Option<Secret>, StoreError>,
) -> Result<BTreeMap<N, Secret>, StoreError> {
    names
        .iter()
        .filter_map(|name| {
            read(name.clone())
                .map(|secret| secret.map(|secret| (name.clone(), secret)))
                .transpose()
        })
        .collect()
}

/// All inputs to connect to one server.
pub struct Launch<'a> {
    pub server: &'a Server,
    pub cached_era: Option<&'a Era>,
    /// The environment for a stdio server, normally the environment of `ncp`.
    pub base_env: &'a Env,
    pub secrets: &'a Secrets,
    pub http: &'a reqwest::Client,
    /// Normally [`PROBE_TIMEOUT`].
    pub probe_timeout: Duration,
    /// Normally [`CONNECT_LIMIT`].
    pub connect_limit: Duration,
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
    #[error("server {0}: the sse transport is not supported")]
    SseNotSupported(ServerName),
    #[error("cannot start {command:?}: {source}")]
    Spawn {
        command: String,
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
            let env = stdio::child_env(launch.base_env, &server.env, &launch.secrets.env);
            let (process, stdout, stdin) =
                stdio::spawn(server, &env).map_err(|source| ConnectError::Spawn {
                    command: server.command.clone(),
                    source,
                })?;
            let serve = info.serve_with_lifecycle((stdout, stdin), lifecycle);
            // The handshake future owns stdin. When it ends, stdin closes.
            let Ok(service) = tokio::time::timeout_at(deadline, serve).await else {
                process.stop().await;
                return Err(timeout);
            };
            (service.map_err(failed)?, Some(process))
        }
        Transport::Http(server) => {
            let transport = http::transport(launch.http, server, &launch.secrets.headers)?;
            let serve = info.serve_with_lifecycle(transport, lifecycle);
            let service = tokio::time::timeout_at(deadline, serve)
                .await
                .map_err(|_| timeout)?;
            (service.map_err(failed)?, None)
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
    pub async fn close(self) {
        let _ = self.service.cancel().await;
        if let Some(process) = self.process {
            process.stop().await;
        }
    }
}
