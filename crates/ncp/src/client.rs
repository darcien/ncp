//! The client core: connect to a server, find its era, list and call tools.
//!
//! Era detection: `ncp` probes with `server/discover` in its own timeout. On
//! a legacy error or a timeout, it connects again on a new transport with
//! `initialize`. rmcp `Auto` mode is not used: when a slow server answers
//! the probe late, the late answer arrives on the same transport and the
//! handshake fails (see docs/research/early-tests.md).

pub mod http;
pub mod stdio;

use std::{collections::BTreeMap, fmt, time::Duration};

use rmcp::{
    ClientLifecycleMode, ClientServiceExt, RoleClient,
    model::{
        CallToolRequest, CallToolRequestParams, CallToolResponse, ClientCapabilities, ClientConfig,
        ClientRequest, Implementation, JsonObject, ServerResult, Tool,
    },
    service::{ClientInitializeError, PeerRequestOptions, RunningService, ServiceError},
};
use tokio_util::sync::CancellationToken;

use crate::{
    config::{Server, Transport},
    credential::{CredentialKey, EnvName, HeaderName, Secret, SecretKind},
    era::{self, Era, ProtocolVersion, Startup},
    expand::Env,
    server::ServerName,
    store::{CredentialStore, StoreError},
};

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
    pub probe_timeout: Duration,
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
}

#[derive(Debug, thiserror::Error)]
pub enum CallError {
    #[error(transparent)]
    Service(#[from] ServiceError),
    #[error("request cancelled")]
    Cancelled,
    #[error("server sent a result that is not a tool result")]
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
/// Returns an error when the server cannot start, or when no handshake
/// succeeds.
pub async fn connect(launch: &Launch<'_>) -> Result<Connection, ConnectError> {
    match era::startup(launch.cached_era) {
        Startup::Probe => probe(launch, ProtocolVersion::first_modern()).await,
        Startup::Discover(version) => probe(launch, version).await,
        Startup::Initialize(version) => {
            match initialize(launch, version, Phase::Initialize).await {
                // The server can be modern now: probe again. Over HTTP the
                // rejection of `initialize` is not always a JSON-RPC error.
                Err(ConnectError::Handshake { .. }) => {
                    probe(launch, ProtocolVersion::first_modern()).await
                }
                other => other,
            }
        }
    }
}

async fn probe(
    launch: &Launch<'_>,
    preferred: ProtocolVersion,
) -> Result<Connection, ConnectError> {
    let mut preferred_versions = vec![to_rmcp(&preferred)];
    let first_modern = to_rmcp(&ProtocolVersion::first_modern());
    if !preferred_versions.contains(&first_modern) {
        preferred_versions.push(first_modern);
    }
    let lifecycle = ClientLifecycleMode::Discover { preferred_versions };
    let latest_legacy = ProtocolVersion::latest_legacy();
    // On a timeout the future is dropped. Its stdio process stops by
    // kill-on-drop, so a late answer cannot reach the next connection.
    match tokio::time::timeout(launch.probe_timeout, handshake(launch, lifecycle)).await {
        Ok(Err(ConnectError::Handshake { source, .. })) if is_json_rpc_error(&source) => {
            initialize(launch, latest_legacy, Phase::Initialize).await
        }
        Ok(result) => result,
        Err(_) => {
            let phase = Phase::InitializeAfterTimeout(launch.probe_timeout);
            initialize(launch, latest_legacy, phase).await
        }
    }
}

async fn initialize(
    launch: &Launch<'_>,
    version: ProtocolVersion,
    phase: Phase,
) -> Result<Connection, ConnectError> {
    let info = client_info().with_protocol_version(to_rmcp(&version));
    handshake_with(launch, ClientLifecycleMode::Initialize, info, phase).await
}

async fn handshake(
    launch: &Launch<'_>,
    lifecycle: ClientLifecycleMode,
) -> Result<Connection, ConnectError> {
    handshake_with(launch, lifecycle, client_info(), Phase::Discover).await
}

/// Starts a new transport and runs one handshake on it.
async fn handshake_with(
    launch: &Launch<'_>,
    lifecycle: ClientLifecycleMode,
    info: ClientConfig,
    phase: Phase,
) -> Result<Connection, ConnectError> {
    let failed = |source| ConnectError::Handshake {
        phase,
        source: Box::new(source),
    };
    let (service, process) = match &launch.server.transport {
        Transport::Stdio(server) => {
            let env = stdio::child_env(launch.base_env, &server.env, &launch.secrets.env);
            let (process, stdout, stdin) =
                stdio::spawn(server, &env).map_err(|source| ConnectError::Spawn {
                    command: server.command.clone(),
                    source,
                })?;
            let service = info
                .serve_with_lifecycle((stdout, stdin), lifecycle)
                .await
                .map_err(failed)?;
            (service, Some(process))
        }
        Transport::Http(server) => {
            let transport = http::transport(launch.http, server, &launch.secrets.headers)?;
            let service = info
                .serve_with_lifecycle(transport, lifecycle)
                .await
                .map_err(failed)?;
            (service, None)
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

impl Connection {
    #[must_use]
    pub fn era(&self) -> &Era {
        &self.era
    }

    /// # Errors
    ///
    /// Returns an error when a `tools/list` request fails.
    pub async fn list_tools(&self) -> Result<Vec<Tool>, ServiceError> {
        self.service.list_all_tools().await
    }

    /// Calls a tool. When `cancel` fires, sends `notifications/cancelled`
    /// for the request and returns `CallError::Cancelled`.
    ///
    /// The result is the first response, with no follow-up rounds: an
    /// `input_required` result goes back to the caller.
    ///
    /// # Errors
    ///
    /// Returns an error when the request fails or is cancelled.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: JsonObject,
        cancel: &CancellationToken,
    ) -> Result<CallToolResponse, CallError> {
        let params = CallToolRequestParams::new(name.to_string()).with_arguments(arguments);
        let request = ClientRequest::CallToolRequest(CallToolRequest::new(params));
        let mut handle = self
            .service
            .peer()
            .send_cancellable_request(request, PeerRequestOptions::no_options())
            .await?;
        let received = tokio::select! {
            received = &mut handle.rx => received,
            () = cancel.cancelled() => {
                handle.cancel(Some("user interrupt".into())).await?;
                return Err(CallError::Cancelled);
            }
        };
        match received.map_err(|_| ServiceError::TransportClosed)?? {
            ServerResult::CallToolResult(result) => Ok(CallToolResponse::Complete(result)),
            ServerResult::InputRequiredResult(result) => {
                Ok(CallToolResponse::InputRequired(result))
            }
            ServerResult::CreateTaskResult(result) => Ok(CallToolResponse::Task(result)),
            _ => Err(CallError::UnexpectedResult),
        }
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
