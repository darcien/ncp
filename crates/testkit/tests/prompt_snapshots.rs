//! Transcripts of the prompt commands.

#![allow(clippy::unwrap_used)]

mod common;

use common::{Home, ncp_snapshot};
use serde_json::json;

const FIXTURE: &str = env!("CARGO_BIN_EXE_ncp-fixture");

fn home(name: &str, mode: &str) -> Home {
    let home = Home::new(name);
    home.configure(&json!({"fx": {"command": FIXTURE, "args": [mode]}}));
    home
}

#[test]
fn prompt_list_shows_the_server_prompts() {
    let home = home("prompt-list-success", "modern");
    let run = ncp_snapshot!(home.filters(), home.ncp(["prompt", "list", "fx"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    greet  Greet a person  name=
    ask    Asks for input
    ");
    assert_eq!(
        run.0.stdout,
        b"greet  Greet a person  name=\nask    Asks for input\n"
    );
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "prompt", "list", "fx"]),
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"prompts":[{"arguments":[{"description":"Who to greet","name":"name","required":true}],"description":"Greet a person","name":"greet"},{"description":"Asks for input","name":"ask"}],"server":"fx"}
    "#
    );
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
}

#[test]
fn prompt_list_stops_when_the_server_repeats_a_cursor() {
    let home = home("prompt-list-repeat-cursor", "repeat-cursor");
    ncp_snapshot!(home.filters(), home.ncp(["prompt", "list", "fx"]), @"
    exit_code: 3 (failure)
    ----- stderr -----
    error: The request `prompts/list` to the server `fx` failed
      cause: The server sent a page cursor that it sent before, so the list does not end
    ");
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "prompt", "list", "fx"]),
        @r#"
    exit_code: 3 (failure)
    ----- stderr -----
    {"error":{"causes":["The server sent a page cursor that it sent before, so the list does not end"],"code":"protocol","data":[],"exit":3,"hints":[],"message":"The request `prompts/list` to the server `fx` failed","server":"fx"}}
    "#
    );
    run.error();
}

#[test]
fn prompt_get_renders_the_prompt_with_arguments() {
    let home = home("prompt-get-success", "modern");
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["prompt", "get", "fx", "greet", "name=Ada"]),
        @"
    exit_code: 0 (success)
    ----- stdout -----
    user: Hello, Ada!
    "
    );
    assert_eq!(run.0.stdout, b"user: Hello, Ada!\n");
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "prompt", "get", "fx", "greet", "name=Ada"]),
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"description":"A greeting","messages":[{"content":{"text":"Hello, Ada!","type":"text"},"role":"user"}],"resultType":"complete"}
    "#
    );
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
}

#[test]
fn prompt_get_rejects_an_unknown_prompt() {
    let home = home("prompt-get-unknown", "modern");
    ncp_snapshot!(home.filters(), home.ncp(["prompt", "get", "fx", "nope"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The server `fx` refused `prompts/get`
      cause: no prompt nope

    hint: To see the prompts of the server, run `ncp prompt list fx`
    ");
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "prompt", "get", "fx", "nope"]),
        @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    {"error":{"causes":["no prompt nope"],"code":"usage","data":[],"exit":2,"hints":[{"command":"ncp prompt list fx","message":"To see the prompts of the server, run `ncp prompt list fx`"}],"message":"The server `fx` refused `prompts/get`","server":"fx"}}
    "#
    );
    run.error();
}
