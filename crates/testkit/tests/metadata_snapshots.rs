//! Command transcripts of the server identity and of the client version.

#![allow(clippy::unwrap_used)]

mod common;

use common::{Home, Run};
use ncp_testkit::{Mode, serve_http};
use serde_json::json;

const FIXTURE: &str = env!("CARGO_BIN_EXE_ncp-fixture");

fn stdio_home(test: &str, mode: &str) -> Home {
    let home = Home::new(test);
    home.configure(&json!({"fx": {"command": FIXTURE, "args": [mode]}}));
    home
}

fn assert_identity(run: &Run, protocol: &str) {
    assert_eq!(run.code(), 0, "{}", run.describe());
    let output = run.stdout();
    assert_eq!(output["protocolVersion"], protocol);
    assert_eq!(
        output["serverInfo"],
        json!({"name": "fixture-server", "version": "1.0.0"})
    );
    assert_eq!(
        output["instructions"],
        "Fixture server for the tests of ncp."
    );
    assert!(output["capabilities"]["tools"].is_object());
    assert!(output["capabilities"]["resources"].is_object());
    assert!(output["capabilities"]["prompts"].is_object());
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
}

#[test]
fn stdio_discovery_identity_reaches_command_output() {
    let home = stdio_home("metadata-stdio-discovery", "modern");
    let text = common::ncp_snapshot!(
        home.filters(),
        home.ncp(["--no-cache", "server", "show", "fx"]),
        @"
    exit_code: 0 (success)
    ----- stdout -----
    Server: fx
    Target: [FIXTURE] modern
    Protocol version: 2026-07-28
    Server info: fixture-server 1.0.0
    Capabilities: prompts, resources, tools

    Instructions:
      Fixture server for the tests of ncp.
    "
    );
    assert_eq!(text.code(), 0, "{}", text.describe());
    let json = common::ncp_snapshot!(
        home.filters(),
        home.ncp(["--no-cache", "--output-format", "json", "-vv", "server", "show", "fx"]),
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"capabilities":{"prompts":{},"resources":{},"tools":{}},"instructions":"Fixture server for the tests of ncp.","protocolVersion":"2026-07-28","server":"fx","serverInfo":{"name":"fixture-server","version":"1.0.0"},"target":"[FIXTURE] modern"}

    ----- stderr -----
    {"debug":{"message":"configuration file: [HOME]/config/ncp/mcp.json"}}
    {"debug":{"message":"no project file: the search stopped at [HOME]"}}
    {"debug":{"message":"no cached protocol version: probe with server/discover","server":"fx"}}
    {"trace":{"sent":{"id":0,"jsonrpc":"2.0","method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/clientCapabilities":{},"io.modelcontextprotocol/clientInfo":{"name":"ncp","version":"[VERSION]"},"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}},"server":"fx"}}
    {"trace":{"received":{"id":0,"jsonrpc":"2.0","result":{"_meta":{"io.modelcontextprotocol/serverInfo":{"name":"fixture-server","version":"1.0.0"}},"cacheScope":"private","capabilities":{"prompts":{},"resources":{},"tools":{}},"instructions":"Fixture server for the tests of ncp.","resultType":"complete","supportedVersions":["2024-11-05","2025-03-26","2025-06-18","2025-11-25","2026-07-28"],"ttlMs":0}},"server":"fx"}}
    "#
    );
    assert_identity(&json, "2026-07-28");
}

#[test]
fn stdio_initialization_identity_reaches_command_output() {
    let home = stdio_home("metadata-stdio-initialize", "legacy");
    let text = common::ncp_snapshot!(
        home.filters(),
        home.ncp(["--no-cache", "server", "show", "fx"]),
        @"
    exit_code: 0 (success)
    ----- stdout -----
    Server: fx
    Target: [FIXTURE] legacy
    Protocol version: 2025-11-25
    Server info: fixture-server 1.0.0
    Capabilities: prompts, resources, tools

    Instructions:
      Fixture server for the tests of ncp.
    "
    );
    assert_eq!(text.code(), 0, "{}", text.describe());
    let json = common::ncp_snapshot!(
        home.filters(),
        home.ncp(["--no-cache", "--output-format", "json", "-vv", "server", "show", "fx"]),
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"capabilities":{"prompts":{},"resources":{},"tools":{}},"instructions":"Fixture server for the tests of ncp.","protocolVersion":"2025-11-25","server":"fx","serverInfo":{"name":"fixture-server","version":"1.0.0"},"target":"[FIXTURE] legacy"}

    ----- stderr -----
    {"debug":{"message":"configuration file: [HOME]/config/ncp/mcp.json"}}
    {"debug":{"message":"no project file: the search stopped at [HOME]"}}
    {"debug":{"message":"no cached protocol version: probe with server/discover","server":"fx"}}
    {"trace":{"sent":{"id":0,"jsonrpc":"2.0","method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/clientCapabilities":{},"io.modelcontextprotocol/clientInfo":{"name":"ncp","version":"[VERSION]"},"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}},"server":"fx"}}
    {"trace":{"received":{"error":{"code":-32601,"message":"Method not found"},"id":0,"jsonrpc":"2.0"},"server":"fx"}}
    {"trace":{"sent":{"id":0,"jsonrpc":"2.0","method":"initialize","params":{"capabilities":{},"clientInfo":{"name":"ncp","version":"[VERSION]"},"protocolVersion":"2025-11-25"}},"server":"fx"}}
    {"trace":{"received":{"id":0,"jsonrpc":"2.0","result":{"capabilities":{"prompts":{},"resources":{},"tools":{}},"instructions":"Fixture server for the tests of ncp.","protocolVersion":"2025-11-25","serverInfo":{"name":"fixture-server","version":"1.0.0"}}},"server":"fx"}}
    {"trace":{"sent":{"jsonrpc":"2.0","method":"notifications/initialized"},"server":"fx"}}
    "#
    );
    assert_identity(&json, "2025-11-25");
    let error = json
        .stderr()
        .into_iter()
        .find_map(|line| line["trace"]["received"].get("error").cloned())
        .unwrap();
    assert_eq!(
        error,
        json!({"code": -32601, "message": "Method not found"})
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn http_discovery_identity_reaches_command_output() {
    let fixture = serve_http(Mode::Modern).await.unwrap();
    let home = Home::new("metadata-http-discovery");
    home.configure(&json!({"fx": {"type": "http", "url": fixture.url}}));
    let text = common::ncp_snapshot!(
        home.filters(),
        home.ncp(["--no-cache", "server", "show", "fx"]),
        @"
    exit_code: 0 (success)
    ----- stdout -----
    Server: fx
    Target: http://[LOCALHOST]/mcp
    Protocol version: 2026-07-28
    Server info: fixture-server 1.0.0
    Capabilities: prompts, resources, tools

    Instructions:
      Fixture server for the tests of ncp.
    "
    );
    assert_eq!(text.code(), 0, "{}", text.describe());
    let json = common::ncp_snapshot!(
        home.filters(),
        home.ncp(["--no-cache", "--output-format", "json", "-vv", "server", "show", "fx"]),
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"capabilities":{"prompts":{},"resources":{},"tools":{}},"instructions":"Fixture server for the tests of ncp.","protocolVersion":"2026-07-28","server":"fx","serverInfo":{"name":"fixture-server","version":"1.0.0"},"target":"http://[LOCALHOST]/mcp"}

    ----- stderr -----
    {"debug":{"message":"configuration file: [HOME]/config/ncp/mcp.json"}}
    {"debug":{"message":"no project file: the search stopped at [HOME]"}}
    {"debug":{"message":"no cached protocol version: probe with server/discover","server":"fx"}}
    {"debug":{"message":"route to http://[LOCALHOST]/mcp: direct","server":"fx"}}
    {"trace":{"sent":{"id":0,"jsonrpc":"2.0","method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/clientCapabilities":{},"io.modelcontextprotocol/clientInfo":{"name":"ncp","version":"[VERSION]"},"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}},"server":"fx"}}
    {"trace":{"http":{"headers":{"mcp-method":"server/discover","mcp-protocol-version":"2026-07-28"},"method":"POST","ms":[TIME],"outcome":"event stream","url":"http://[LOCALHOST]/mcp"},"server":"fx"}}
    {"trace":{"received":{"id":0,"jsonrpc":"2.0","result":{"_meta":{"io.modelcontextprotocol/serverInfo":{"name":"fixture-server","version":"1.0.0"}},"cacheScope":"private","capabilities":{"prompts":{},"resources":{},"tools":{}},"instructions":"Fixture server for the tests of ncp.","resultType":"complete","supportedVersions":["2024-11-05","2025-03-26","2025-06-18","2025-11-25","2026-07-28"],"ttlMs":0}},"server":"fx"}}
    "#
    );
    assert_identity(&json, "2026-07-28");
}

#[tokio::test(flavor = "multi_thread")]
async fn http_initialization_identity_reaches_command_output() {
    let fixture = serve_http(Mode::Legacy).await.unwrap();
    let home = Home::new("metadata-http-initialize");
    home.configure(&json!({"fx": {"type": "http", "url": fixture.url}}));
    let text = common::ncp_snapshot!(
        home.filters(),
        home.ncp(["--no-cache", "server", "show", "fx"]),
        @"
    exit_code: 0 (success)
    ----- stdout -----
    Server: fx
    Target: http://[LOCALHOST]/mcp
    Protocol version: 2025-11-25
    Server info: fixture-server 1.0.0
    Capabilities: prompts, resources, tools

    Instructions:
      Fixture server for the tests of ncp.
    "
    );
    assert_eq!(text.code(), 0, "{}", text.describe());
    let json = common::ncp_snapshot!(
        home.filters(),
        home.ncp(["--no-cache", "--output-format", "json", "server", "show", "fx"]),
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"capabilities":{"prompts":{},"resources":{},"tools":{}},"instructions":"Fixture server for the tests of ncp.","protocolVersion":"2025-11-25","server":"fx","serverInfo":{"name":"fixture-server","version":"1.0.0"},"target":"http://[LOCALHOST]/mcp"}
    "#
    );
    assert_identity(&json, "2025-11-25");
}

#[test]
fn version_output_reports_the_built_package_version() {
    let home = Home::new("metadata-client-version");
    let root = common::ncp_snapshot!(home.filters(), home.ncp(["--version"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    ncp [VERSION]
    ");
    assert_eq!(root.code(), 0, "{}", root.describe());
    assert_eq!(
        root.0.stdout,
        format!("ncp {}\n", env!("CARGO_PKG_VERSION")).as_bytes()
    );
    assert_eq!(root.0.stderr, [] as [u8; 0]);
    let server = common::ncp_snapshot!(home.filters(), home.ncp(["server", "--version"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    ncp-server [VERSION]
    ");
    assert_eq!(server.code(), 0, "{}", server.describe());
    assert_eq!(
        server.0.stdout,
        format!("ncp-server {}\n", env!("CARGO_PKG_VERSION")).as_bytes()
    );
    assert_eq!(server.0.stderr, [] as [u8; 0]);
}
