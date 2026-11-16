//! Transcripts of the resource commands.

#![allow(clippy::unwrap_used)]

mod common;

use common::{Home, ncp_snapshot};
use serde_json::json;

const FIXTURE: &str = env!("CARGO_BIN_EXE_ncp-fixture");

fn home(name: &str) -> Home {
    let home = Home::new(name);
    home.configure(&json!({"fx": {"command": FIXTURE, "args": ["modern"]}}));
    home
}

#[test]
fn resource_list_shows_the_server_resources() {
    let home = home("resource-list-success");
    let run = ncp_snapshot!(home.filters(), home.ncp(["resource", "list", "fx"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    Resources:
    fixture://greeting  text/plain  A greeting
    fixture://logo      image/png
    fixture://pair                  Two contents: a text and a blob
    fixture://ask                   Asks for input

    Templates:
    fixture://echo/{text}    Returns the text
    ");
    assert_eq!(run.0.stderr, [] as [u8; 0]);
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "resource", "list", "fx"]),
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"resourceTemplates":[{"description":"Returns the text","name":"echo","uriTemplate":"fixture://echo/{text}"}],"resources":[{"description":"A greeting","mimeType":"text/plain","name":"greeting","uri":"fixture://greeting"},{"mimeType":"image/png","name":"logo","uri":"fixture://logo"},{"description":"Two contents: a text and a blob","name":"pair","uri":"fixture://pair"},{"description":"Asks for input","name":"ask","uri":"fixture://ask"}],"server":"fx"}
    "#
    );
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
}

#[test]
fn resource_list_with_an_unknown_server_fails() {
    let home = home("resource-list-unknown");
    ncp_snapshot!(home.filters(), home.ncp(["resource", "list", "nope"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No server `nope` is configured in `[HOME]/config/ncp/mcp.json`

    hint: To see the configured servers, run `ncp server list`
    ");
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "resource", "list", "nope"]),
        @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    {"error":{"causes":[],"code":"usage","data":[],"exit":2,"hints":[{"command":"ncp server list","message":"To see the configured servers, run `ncp server list`"}],"message":"No server `nope` is configured in `[HOME]/config/ncp/mcp.json`"}}
    "#
    );
    run.error();
}

#[test]
fn resource_read_writes_the_contents_unchanged() {
    let home = home("resource-read-success");
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["resource", "read", "fx", "fixture://greeting"]),
        @"
    exit_code: 0 (success)
    ----- stdout -----
    hello
    "
    );
    assert_eq!(run.0.stderr, [] as [u8; 0]);
    assert_eq!(run.0.stdout, b"hello\n");
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "resource", "read", "fx", "fixture://greeting"]),
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"cacheScope":"private","contents":[{"mimeType":"text/plain","text":"hello\n","uri":"fixture://greeting"}],"resultType":"complete","ttlMs":0}
    "#
    );
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
    assert!(run.0.stdout.ends_with(b"}\n"));
}

#[test]
fn resource_read_with_an_unknown_resource_fails() {
    let home = home("resource-read-unknown");
    ncp_snapshot!(
        home.filters(),
        home.ncp(["resource", "read", "fx", "fixture://none"]),
        @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The server `fx` refused `resources/read`
      cause: no resource fixture://none

    hint: To see the resources of the server, run `ncp resource list fx`
    "
    );
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "resource", "read", "fx", "fixture://none"]),
        @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    {"error":{"causes":["no resource fixture://none"],"code":"usage","data":[],"exit":2,"hints":[{"command":"ncp resource list fx","message":"To see the resources of the server, run `ncp resource list fx`"}],"message":"The server `fx` refused `resources/read`","server":"fx"}}
    "#
    );
    run.error();
}
