//! Interop tests of `ncp` against the official MCP everything server
//! (TypeScript SDK). See `docs/design/interop.md`.
//!
//! The tests need Node and the pinned server. Thus they have the `ignore`
//! attribute. Run them with `mise run interop` after `mise run
//! interop:install`. They need no network.

// Test code can use unwrap: a panic is a test failure.
#![allow(clippy::unwrap_used)]

mod common;

use std::{
    net::{Ipv4Addr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use common::{Home, Run};
use serde_json::{Value, json};

const INSTALL_HINT: &str = "run `mise run interop:install`";

/// The servers of the TypeScript SDK v1 speak this version only.
const LEGACY_VERSION: &str = "2025-11-25";

/// The `node` program on the `PATH` of the test. `ncp` gives a stdio
/// server no `PATH`, so the entry needs an absolute path.
fn node() -> PathBuf {
    std::env::var_os("PATH")
        .iter()
        .flat_map(std::env::split_paths)
        .map(|dir| dir.join("node"))
        .find(|path| path.is_file())
        .unwrap_or_else(|| panic!("no node on PATH: {INSTALL_HINT}"))
}

/// The entry point of the server, from the `bin` field of its package.
fn server_entry() -> PathBuf {
    let package = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/interop/node_modules/@modelcontextprotocol/server-everything");
    let manifest = std::fs::read_to_string(package.join("package.json"))
        .unwrap_or_else(|_| panic!("the everything server is not installed: {INSTALL_HINT}"));
    let manifest: Value = serde_json::from_str(&manifest).unwrap();
    let bin = manifest["bin"]["mcp-server-everything"].as_str().unwrap();
    package.join(bin)
}

/// The entry of the server in the configuration, and the process that
/// serves HTTP when there is one.
struct Server {
    /// The transport, so that each test of each transport has its own home.
    transport: &'static str,
    entry: Value,
    process: Option<Child>,
}

impl Drop for Server {
    /// Stops the HTTP server also when the test fails.
    fn drop(&mut self) {
        if let Some(process) = self.process.as_mut() {
            let _ = process.kill();
            let _ = process.wait();
        }
    }
}

fn stdio_server() -> Server {
    let entry = server_entry().display().to_string();
    Server {
        transport: "stdio",
        entry: json!({"command": node(), "args": [entry, "stdio"]}),
        process: None,
    }
}

/// Starts the server over Streamable HTTP on a free port. The server
/// has no host option, so it listens on all interfaces. `ncp` connects
/// over loopback, the only host where `ncp` accepts plain HTTP.
fn http_server() -> Server {
    let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let process = Command::new(node())
        .arg(server_entry())
        .arg("streamableHttp")
        .env_clear()
        .env("PORT", port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut server = Server {
        transport: "http",
        entry: json!({"type": "http", "url": format!("http://127.0.0.1:{port}/mcp")}),
        process: Some(process),
    };
    wait_for_port(&mut server, port);
    server
}

fn wait_for_port(server: &mut Server, port: u16) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let process = server.process.as_mut().unwrap();
    while TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_err() {
        if let Some(status) = process.try_wait().unwrap() {
            panic!("the HTTP server stopped with {status}");
        }
        assert!(Instant::now() < deadline, "the HTTP server did not start");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A home with the server under the name `every`.
fn home_with(test: &str, server: &Server) -> Home {
    let home = Home::new(&format!("interop-{}-{test}", server.transport));
    home.configure(&json!({"every": server.entry}));
    home
}

fn ok(run: &Run) -> Value {
    assert_eq!(run.code(), 0, "{}", run.describe());
    run.stdout()
}

fn names(list: &Value, key: &str) -> Vec<String> {
    list[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap().to_owned())
        .collect()
}

fn text_of(result: &Value) -> &str {
    result["content"][0]["text"].as_str().unwrap()
}

/// The first contact falls back to the legacy version without a wait for
/// the probe timeout, and the second contact uses the cached protocol version.
fn falls_back_to_the_legacy_version_and_caches_it(server: &Server) {
    let home = home_with("version", server);
    let first = home.run(&["server", "show", "every", "--timings"]);
    assert_eq!(ok(&first)["protocolVersion"], LEGACY_VERSION);
    let total = first
        .stderr()
        .into_iter()
        .find_map(|line| line["timings"]["totalMs"].as_f64())
        .unwrap();
    let limit = ncp::client::PROBE_TIMEOUT.as_secs_f64() * 1000.0 / 2.0;
    assert!(total < limit, "first contact took {total} ms");

    let second = home.run(&["server", "show", "every"]);
    assert_eq!(ok(&second)["protocolVersion"], LEGACY_VERSION);
}

fn lists_tools_resources_and_prompts(server: &Server) {
    let home = home_with("lists", server);
    let tools = ok(&home.run(&["tool", "list", "every"]));
    assert!(names(&tools, "tools").contains(&"echo".to_owned()));
    let resources = ok(&home.run(&["resource", "list", "every"]));
    assert_ne!(names(&resources, "resources"), Vec::<String>::new());
    let prompts = ok(&home.run(&["prompt", "list", "every"]));
    assert!(names(&prompts, "prompts").contains(&"args-prompt".to_owned()));
}

/// The server registers some tools only for a client capability. With no
/// TTY, `ncp` declares no elicitation, and it never declares sampling or
/// roots.
fn declares_no_client_capability_without_a_tty(server: &Server) {
    let home = home_with("capabilities", server);
    let tools = names(&ok(&home.run(&["tool", "list", "every"])), "tools");
    for name in [
        "trigger-elicitation-request",
        "trigger-sampling-request",
        "get-roots-list",
    ] {
        assert!(!tools.contains(&name.to_owned()), "{name} in {tools:?}");
    }
}

fn calls_tools_with_typed_arguments(server: &Server) {
    let home = home_with("calls", server);
    let echo = ok(&home.run(&["tool", "call", "every", "echo", "message=hi"]));
    assert_eq!(text_of(&echo), "Echo: hi");
    let sum = ok(&home.run(&["tool", "call", "every", "get-sum", "a=2", "b=3"]));
    assert_eq!(text_of(&sum), "The sum of 2 and 3 is 5.");
}

fn gives_structured_image_and_resource_link_results(server: &Server) {
    let home = home_with("shapes", server);
    let structured = ok(&home.run(&[
        "tool",
        "call",
        "every",
        "get-structured-content",
        "location=Chicago",
    ]));
    assert!(structured["structuredContent"]["temperature"].is_number());

    let image = ok(&home.run(&["tool", "call", "every", "get-tiny-image"]));
    let block = image["content"]
        .as_array()
        .unwrap()
        .iter()
        .find(|block| block["type"] == "image")
        .unwrap();
    assert_eq!(block["mimeType"], "image/png");
    assert_ne!(block["data"].as_str().unwrap(), "");

    let links = ok(&home.run(&["tool", "call", "every", "get-resource-links", "count=2"]));
    let count = links["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|block| block["type"] == "resource_link" && block["uri"].is_string())
        .count();
    assert_eq!(count, 2);
}

fn reads_text_and_blob_resources_and_gets_a_prompt(server: &Server) {
    let home = home_with("read", server);
    let list = ok(&home.run(&["resource", "list", "every"]));
    let text_uri = list["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|resource| resource["mimeType"] == "text/markdown")
        .unwrap()["uri"]
        .as_str()
        .unwrap()
        .to_owned();
    let blob_uri = list["resourceTemplates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|template| template["mimeType"] == "application/octet-stream")
        .unwrap()["uriTemplate"]
        .as_str()
        .unwrap()
        .replace("{resourceId}", "1");

    let read =
        |uri: &str| ok(&home.run(&["resource", "read", "every", uri, "--output-format", "json"]));
    assert!(read(&text_uri)["contents"][0]["text"].is_string());
    assert!(read(&blob_uri)["contents"][0]["blob"].is_string());

    let prompt = ok(&home.run(&[
        "prompt",
        "get",
        "every",
        "args-prompt",
        "city=Paris",
        "--output-format",
        "json",
    ]));
    let text = prompt["messages"][0]["content"]["text"].as_str().unwrap();
    assert!(text.contains("Paris"), "{text}");
}

fn shows_progress_of_a_long_operation(server: &Server) {
    let home = home_with("progress", server);
    let run = home.run(&[
        "tool",
        "call",
        "every",
        "trigger-long-running-operation",
        "duration=1",
        "steps=2",
    ]);
    ok(&run);
    let steps = run
        .stderr()
        .into_iter()
        .filter(|line| line.get("progress").is_some())
        .count();
    assert_eq!(steps, 2, "{}", run.describe());
}

#[test]
#[ignore = "needs Node and the pinned server: mise run interop"]
fn stdio_falls_back_to_the_legacy_version_and_caches_it() {
    falls_back_to_the_legacy_version_and_caches_it(&stdio_server());
}

#[test]
#[ignore = "needs Node and the pinned server: mise run interop"]
fn stdio_lists_tools_resources_and_prompts() {
    lists_tools_resources_and_prompts(&stdio_server());
}

#[test]
#[ignore = "needs Node and the pinned server: mise run interop"]
fn stdio_declares_no_client_capability_without_a_tty() {
    declares_no_client_capability_without_a_tty(&stdio_server());
}

#[test]
#[ignore = "needs Node and the pinned server: mise run interop"]
fn stdio_calls_tools_with_typed_arguments() {
    calls_tools_with_typed_arguments(&stdio_server());
}

#[test]
#[ignore = "needs Node and the pinned server: mise run interop"]
fn stdio_gives_structured_image_and_resource_link_results() {
    gives_structured_image_and_resource_link_results(&stdio_server());
}

#[test]
#[ignore = "needs Node and the pinned server: mise run interop"]
fn stdio_reads_text_and_blob_resources_and_gets_a_prompt() {
    reads_text_and_blob_resources_and_gets_a_prompt(&stdio_server());
}

#[test]
#[ignore = "needs Node and the pinned server: mise run interop"]
fn stdio_shows_progress_of_a_long_operation() {
    shows_progress_of_a_long_operation(&stdio_server());
}

#[test]
#[ignore = "needs Node and the pinned server: mise run interop"]
fn http_falls_back_to_the_legacy_version_and_caches_it() {
    falls_back_to_the_legacy_version_and_caches_it(&http_server());
}

#[test]
#[ignore = "needs Node and the pinned server: mise run interop"]
fn http_lists_tools_resources_and_prompts() {
    lists_tools_resources_and_prompts(&http_server());
}

#[test]
#[ignore = "needs Node and the pinned server: mise run interop"]
fn http_declares_no_client_capability_without_a_tty() {
    declares_no_client_capability_without_a_tty(&http_server());
}

#[test]
#[ignore = "needs Node and the pinned server: mise run interop"]
fn http_calls_tools_with_typed_arguments() {
    calls_tools_with_typed_arguments(&http_server());
}

#[test]
#[ignore = "needs Node and the pinned server: mise run interop"]
fn http_gives_structured_image_and_resource_link_results() {
    gives_structured_image_and_resource_link_results(&http_server());
}

#[test]
#[ignore = "needs Node and the pinned server: mise run interop"]
fn http_reads_text_and_blob_resources_and_gets_a_prompt() {
    reads_text_and_blob_resources_and_gets_a_prompt(&http_server());
}
