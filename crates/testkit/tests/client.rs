//! Integration tests of the client core against local fixture servers.

// The whole file is test code, where the agent rules permit unwrap.
#![allow(clippy::unwrap_used)]

use std::{
    collections::BTreeMap,
    process::Command,
    time::{Duration, Instant},
};

use ncp::{
    client::{self, CallError, ConnectError, Connection, Launch, Phase, Secrets},
    config::{RemoteServer, Server, StdioServer, Transport},
    credential::{CredentialKey, EnvName, HeaderName, Secret, SecretKind},
    era::{Era, LegacyCause, ProtocolVersion},
    expand::Env,
    server::{ServerName, ServerUrl},
    store::{CredentialStore, MemoryStore},
};
use ncp_testkit::{Mode, serve_http};
use rmcp::model::{CallToolResponse, JsonObject};
use tokio_util::sync::CancellationToken;

const FIXTURE: &str = env!("CARGO_BIN_EXE_ncp-fixture");

fn name() -> ServerName {
    ServerName::parse("fixture").unwrap()
}

fn stdio(args: &[&str], env: &[(&str, &str)]) -> Server {
    Server {
        name: name(),
        transport: Transport::Stdio(StdioServer {
            command: FIXTURE.into(),
            args: args.iter().map(ToString::to_string).collect(),
            env: env
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            cwd: None,
        }),
    }
}

fn http_server(url: &str, headers: &[(&str, &str)]) -> Server {
    Server {
        name: name(),
        transport: Transport::Http(RemoteServer {
            url: ServerUrl::parse(url).unwrap(),
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }),
    }
}

/// Tests use only local servers, and reqwest sends loopback requests to a
/// proxy when the environment names one. Thus the test client has no proxy.
fn http() -> reqwest::Client {
    client::http::builder().no_proxy().build().unwrap()
}

struct Setup {
    server: Server,
    cached: Option<Era>,
    base_env: Env,
    secrets: Secrets,
    probe_timeout: Duration,
}

impl Setup {
    fn new(server: Server) -> Self {
        Self {
            server,
            cached: None,
            base_env: Env::new(),
            secrets: Secrets::default(),
            probe_timeout: Duration::from_secs(5),
        }
    }

    async fn connect(&self) -> Result<Connection, ConnectError> {
        let http = http();
        client::connect(&Launch {
            server: &self.server,
            cached_era: self.cached.as_ref(),
            base_env: &self.base_env,
            secrets: &self.secrets,
            http: &http,
            probe_timeout: self.probe_timeout,
        })
        .await
    }
}

fn version(text: &str) -> ProtocolVersion {
    ProtocolVersion::parse(text).unwrap()
}

fn modern() -> Era {
    Era::Modern {
        version: version("2026-07-28"),
    }
}

fn legacy(cause: LegacyCause) -> Era {
    Era::Legacy {
        version: version("2025-11-25"),
        cause,
    }
}

fn arguments(value: serde_json::Value) -> JsonObject {
    match value {
        serde_json::Value::Object(map) => map,
        other => panic!("not an object: {other}"),
    }
}

/// Calls a tool and returns the first text content.
async fn call_text(connection: &Connection, tool: &str, args: serde_json::Value) -> String {
    let response = connection
        .call_tool(tool, arguments(args), &CancellationToken::new())
        .await
        .unwrap();
    let CallToolResponse::Complete(result) = response else {
        panic!("not complete: {response:?}")
    };
    serde_json::to_value(&result).unwrap()["content"][0]["text"]
        .as_str()
        .unwrap()
        .to_string()
}

fn process_is_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
}

#[tokio::test]
async fn finds_modern_era_of_stdio_server_with_discover() {
    let connection = Setup::new(stdio(&["modern"], &[])).connect().await.unwrap();
    assert_eq!(connection.era(), &modern());
    assert_eq!(
        call_text(&connection, "echo", serde_json::json!({"text": "hi"})).await,
        "hi"
    );
    connection.close().await;
}

#[tokio::test]
async fn falls_back_to_initialize_when_stdio_server_rejects_discover() {
    let connection = Setup::new(stdio(&["legacy"], &[])).connect().await.unwrap();
    assert_eq!(connection.era(), &legacy(LegacyCause::ServerError));
    assert_eq!(
        call_text(&connection, "echo", serde_json::json!({"text": "hi"})).await,
        "hi"
    );
    connection.close().await;
}

#[tokio::test]
async fn falls_back_to_initialize_when_discover_times_out() {
    let mut setup = Setup::new(stdio(&["slow-3000"], &[]));
    setup.probe_timeout = Duration::from_millis(300);
    let connection = setup.connect().await.unwrap();
    assert_eq!(connection.era(), &legacy(LegacyCause::ProbeTimeout));
    assert_eq!(
        call_text(&connection, "echo", serde_json::json!({"text": "hi"})).await,
        "hi"
    );
    connection.close().await;
}

#[tokio::test]
async fn connects_to_modern_only_stdio_server() {
    let connection = Setup::new(stdio(&["modern-only"], &[]))
        .connect()
        .await
        .unwrap();
    assert_eq!(connection.era(), &modern());
    connection.close().await;
}

#[tokio::test]
async fn finds_modern_era_of_http_server() {
    let fixture = serve_http(Mode::Modern).await.unwrap();
    let connection = Setup::new(http_server(&fixture.url, &[]))
        .connect()
        .await
        .unwrap();
    assert_eq!(connection.era(), &modern());
    assert_eq!(connection.list_tools().await.unwrap().len(), 6);
    connection.close().await;
}

#[tokio::test]
async fn falls_back_to_initialize_when_http_server_rejects_discover() {
    let fixture = serve_http(Mode::Legacy).await.unwrap();
    let connection = Setup::new(http_server(&fixture.url, &[]))
        .connect()
        .await
        .unwrap();
    assert_eq!(connection.era(), &legacy(LegacyCause::ServerError));
    assert_eq!(
        call_text(&connection, "echo", serde_json::json!({"text": "hi"})).await,
        "hi"
    );
    connection.close().await;
}

#[tokio::test]
async fn falls_back_to_initialize_when_http_discover_times_out() {
    let fixture = serve_http(Mode::Slow(Duration::from_secs(3)))
        .await
        .unwrap();
    let mut setup = Setup::new(http_server(&fixture.url, &[]));
    setup.probe_timeout = Duration::from_millis(300);
    let connection = setup.connect().await.unwrap();
    assert_eq!(connection.era(), &legacy(LegacyCause::ProbeTimeout));
    assert_eq!(
        call_text(&connection, "echo", serde_json::json!({"text": "hi"})).await,
        "hi"
    );
    connection.close().await;
}

#[tokio::test]
async fn skips_probe_for_cached_legacy_era_from_server_error() {
    let fixture = serve_http(Mode::Legacy).await.unwrap();
    let mut setup = Setup::new(http_server(&fixture.url, &[]));
    setup.cached = Some(legacy(LegacyCause::ServerError));
    let connection = setup.connect().await.unwrap();
    assert_eq!(connection.era(), &legacy(LegacyCause::ServerError));
    assert_eq!(
        fixture
            .discover_calls
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    connection.close().await;
}

#[tokio::test]
async fn probes_again_for_cached_legacy_era_from_timeout() {
    let fixture = serve_http(Mode::Modern).await.unwrap();
    let mut setup = Setup::new(http_server(&fixture.url, &[]));
    setup.cached = Some(legacy(LegacyCause::ProbeTimeout));
    let connection = setup.connect().await.unwrap();
    assert_eq!(connection.era(), &modern());
    connection.close().await;
}

#[tokio::test]
async fn probes_when_cached_legacy_server_now_rejects_initialize() {
    let fixture = serve_http(Mode::ModernOnly).await.unwrap();
    let mut setup = Setup::new(http_server(&fixture.url, &[]));
    setup.cached = Some(legacy(LegacyCause::ServerError));
    let connection = setup.connect().await.unwrap();
    assert_eq!(connection.era(), &modern());
    connection.close().await;
}

#[tokio::test]
async fn names_probe_timeout_when_initialize_fails_after_it() {
    let fixture = serve_http(Mode::ModernOnly).await.unwrap();
    let mut setup = Setup::new(http_server(&fixture.url, &[]));
    // No round trip is faster than zero: the probe always times out.
    setup.probe_timeout = Duration::ZERO;
    let error = setup.connect().await.err().unwrap();
    assert!(
        matches!(
            error,
            ConnectError::Handshake {
                phase: Phase::InitializeAfterTimeout(_),
                ..
            }
        ),
        "{error}"
    );
    assert!(error.to_string().contains("server/discover got no answer"));
}

#[tokio::test]
async fn returns_tool_error_result_as_complete_result() {
    let connection = Setup::new(stdio(&["modern"], &[])).connect().await.unwrap();
    let response = connection
        .call_tool(
            "fail",
            arguments(serde_json::json!({"text": "bad"})),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    let CallToolResponse::Complete(result) = response else {
        panic!("not complete")
    };
    assert_eq!(result.is_error, Some(true));
    connection.close().await;
}

#[tokio::test]
async fn returns_input_required_result_to_the_caller() {
    let connection = Setup::new(stdio(&["modern"], &[])).connect().await.unwrap();
    let response = connection
        .call_tool("ask", JsonObject::new(), &CancellationToken::new())
        .await
        .unwrap();
    let CallToolResponse::InputRequired(result) = response else {
        panic!("not input_required: {response:?}")
    };
    assert_eq!(result.request_state.as_deref(), Some("state-1"));
    connection.close().await;
}

fn marker(test: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("ncp-marker-{}-{test}", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}

async fn call_and_cancel(
    connection: &Connection,
    marker: &std::path::Path,
) -> Result<CallToolResponse, CallError> {
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        trigger.cancel();
    });
    let args = serde_json::json!({"ms": 30_000, "marker": marker.to_str().unwrap()});
    connection.call_tool("wait", arguments(args), &cancel).await
}

async fn wait_for_file(path: &std::path::Path) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

#[tokio::test]
async fn sends_cancel_request_to_stdio_server_when_cancelled() {
    let connection = Setup::new(stdio(&["modern"], &[])).connect().await.unwrap();
    let marker = marker("stdio-cancel");
    let result = call_and_cancel(&connection, &marker).await;
    assert!(matches!(result, Err(CallError::Cancelled)), "{result:?}");
    assert!(
        wait_for_file(&marker).await,
        "server did not see the cancel request"
    );
    connection.close().await;
    let _ = std::fs::remove_file(marker);
}

#[tokio::test]
async fn sends_cancel_request_to_http_server_when_cancelled() {
    let fixture = serve_http(Mode::Modern).await.unwrap();
    let connection = Setup::new(http_server(&fixture.url, &[]))
        .connect()
        .await
        .unwrap();
    let marker = marker("http-cancel");
    let result = call_and_cancel(&connection, &marker).await;
    assert!(matches!(result, Err(CallError::Cancelled)), "{result:?}");
    assert!(
        wait_for_file(&marker).await,
        "server did not see the cancel request"
    );
    connection.close().await;
    let _ = std::fs::remove_file(marker);
}

#[tokio::test]
async fn gives_stdio_server_proxy_variables_entry_env_and_stored_secrets() {
    let store = MemoryStore::default();
    let api_key = EnvName::parse("API_KEY").unwrap();
    store
        .save(
            &CredentialKey::new(name(), SecretKind::Env(api_key.clone())),
            &Secret::new("from-store".into()),
        )
        .unwrap();
    let mut setup = Setup::new(stdio(&["modern"], &[("MODE", "from-entry")]));
    setup.base_env = Env::from([
        (
            "HTTPS_PROXY".to_string(),
            "http://proxy.test:3128".to_string(),
        ),
        ("no_proxy".to_string(), "localhost".to_string()),
    ]);
    setup.secrets = Secrets::load(&store, &name(), &[api_key], &[]).unwrap();
    let connection = setup.connect().await.unwrap();
    for (variable, expected) in [
        ("HTTPS_PROXY", "http://proxy.test:3128"),
        ("no_proxy", "localhost"),
        ("MODE", "from-entry"),
        ("API_KEY", "from-store"),
        ("HOME", "<unset>"),
    ] {
        let value = call_text(&connection, "env", serde_json::json!({"name": variable})).await;
        assert_eq!(value, expected, "{variable}");
    }
    connection.close().await;
}

#[tokio::test]
async fn sends_entry_headers_and_stored_header_secrets_to_http_server() {
    let fixture = serve_http(Mode::Modern).await.unwrap();
    let store = MemoryStore::default();
    let authorization = HeaderName::parse("Authorization").unwrap();
    store
        .save(
            &CredentialKey::new(name(), SecretKind::Header(authorization.clone())),
            &Secret::new("Bearer from-store".into()),
        )
        .unwrap();
    let mut setup = Setup::new(http_server(&fixture.url, &[("X-Org", "acme")]));
    setup.secrets = Secrets::load(&store, &name(), &[], &[authorization]).unwrap();
    let connection = setup.connect().await.unwrap();
    let header =
        |name: &'static str| call_text(&connection, "header", serde_json::json!({"name": name}));
    assert_eq!(header("authorization").await, "Bearer from-store");
    assert_eq!(header("x-org").await, "acme");
    connection.close().await;
}

#[tokio::test]
async fn leaves_out_secret_names_with_no_stored_value() {
    let store = MemoryStore::default();
    let secrets = Secrets::load(&store, &name(), &[EnvName::parse("NONE").unwrap()], &[]).unwrap();
    assert!(secrets.env.is_empty());
}

#[tokio::test]
async fn stops_stdio_server_that_exits_when_stdin_closes() {
    let connection = Setup::new(stdio(&["modern"], &[])).connect().await.unwrap();
    let pid = connection.process_id().unwrap();
    let start = Instant::now();
    connection.close().await;
    assert!(!process_is_alive(pid));
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "{:?}",
        start.elapsed()
    );
}

#[tokio::test]
async fn stops_stdio_server_that_ignores_closed_stdin_with_sigterm() {
    let connection = Setup::new(stdio(&["modern", "linger"], &[]))
        .connect()
        .await
        .unwrap();
    let pid = connection.process_id().unwrap();
    connection.close().await;
    assert!(!process_is_alive(pid));
}

#[tokio::test]
async fn stops_stdio_server_that_ignores_sigterm_with_sigkill() {
    let connection = Setup::new(stdio(&["modern", "ignore-term"], &[]))
        .connect()
        .await
        .unwrap();
    let pid = connection.process_id().unwrap();
    connection.close().await;
    assert!(!process_is_alive(pid));
}

#[tokio::test]
async fn rejects_sse_server() {
    let server = Server {
        name: name(),
        transport: Transport::Sse(RemoteServer {
            url: ServerUrl::parse("http://localhost:1/sse").unwrap(),
            headers: BTreeMap::new(),
        }),
    };
    let error = Setup::new(server).connect().await.err().unwrap();
    assert!(matches!(error, ConnectError::SseNotSupported(_)));
}

#[tokio::test]
async fn reports_command_that_cannot_start() {
    let mut server = stdio(&[], &[]);
    if let Transport::Stdio(stdio) = &mut server.transport {
        stdio.command = "/nonexistent/ncp-test-server".into();
    }
    let error = Setup::new(server).connect().await.err().unwrap();
    assert!(matches!(error, ConnectError::Spawn { .. }), "{error}");
}
