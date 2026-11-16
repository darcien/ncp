//! Transcripts of the server commands and the files that they change.

#![allow(clippy::unwrap_used)]

mod common;

use common::{Home, ncp_snapshot};
use serde_json::json;

const FIXTURE: &str = env!("CARGO_BIN_EXE_ncp-fixture");

#[test]
fn list_shows_the_configured_servers() {
    let home = Home::new("snap-server-list");
    home.configure(&json!({
        "fx": {"command": FIXTURE, "args": ["modern"]},
        "web": {"type": "http", "url": "https://web.example.com/mcp"}
    }));
    let run = ncp_snapshot!(home.filters(), home.ncp(["server", "list"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    fx   stdio  [FIXTURE] modern
    web  http   https://web.example.com/mcp
    ");
    assert!(run.0.stdout.ends_with(b"\n"));
    let run = ncp_snapshot!(home.filters(), home.ncp(["--output-format", "json", "server", "list"]), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"invalid":[],"servers":[{"file":"[HOME]/config/ncp/mcp.json","name":"fx","scope":"global","target":"[FIXTURE] modern","transport":"stdio"},{"file":"[HOME]/config/ncp/mcp.json","name":"web","scope":"global","target":"https://web.example.com/mcp","transport":"http"}]}
    "#);
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
}

#[test]
fn list_reports_a_configuration_file_that_does_not_parse() {
    let home = Home::new("snap-server-list-error");
    home.configure(&json!({}));
    std::fs::write(home.config_path(), "{").unwrap();
    let run = ncp_snapshot!(home.filters(), home.ncp(["server", "list"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The configuration file `[HOME]/config/ncp/mcp.json` is not valid
      cause: The file is not valid JSONC
      cause: Unterminated object on line 1 column 2
    ");
    assert!(run.0.stderr.ends_with(b"\n"));
    let run = ncp_snapshot!(home.filters(), home.ncp(["--output-format", "json", "server", "list"]), @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    {"error":{"causes":["The file is not valid JSONC","Unterminated object on line 1 column 2"],"code":"usage","data":[],"exit":2,"hints":[],"message":"The configuration file `[HOME]/config/ncp/mcp.json` is not valid"}}
    "#);
    run.error();
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
}

#[test]
fn show_gives_the_handshake_and_instructions() {
    let home = Home::new("snap-server-show");
    home.configure(&json!({"fx": {"command": FIXTURE, "args": ["modern"]}}));
    let run = ncp_snapshot!(home.filters(), home.ncp(["server", "show", "fx"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    Server: fx
    Target: [FIXTURE] modern
    Protocol version: 2026-07-28
    Server info: fixture-server 1.0.0
    Capabilities: prompts, resources, tools

    Instructions:
      Fixture server for the tests of ncp.
    ");
    assert!(run.0.stdout.ends_with(b"\n"));
    let run = ncp_snapshot!(home.filters(), home.ncp(["--output-format", "json", "server", "show", "fx"]), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"capabilities":{"prompts":{},"resources":{},"tools":{}},"instructions":"Fixture server for the tests of ncp.","protocolVersion":"2026-07-28","server":"fx","serverInfo":{"name":"fixture-server","version":"1.0.0"},"target":"[FIXTURE] modern"}
    "#);
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
}

#[test]
fn show_names_an_unknown_server() {
    let home = Home::new("snap-server-show-error");
    home.configure(&json!({"fx": {"command": FIXTURE, "args": ["modern"]}}));
    let run = ncp_snapshot!(home.filters(), home.ncp(["server", "show", "missing"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No server `missing` is configured in `[HOME]/config/ncp/mcp.json`

    hint: To see the configured servers, run `ncp server list`
    ");
    assert!(run.0.stderr.ends_with(b"\n"));
    let run = ncp_snapshot!(home.filters(), home.ncp(["--output-format", "json", "server", "show", "missing"]), @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    {"error":{"causes":[],"code":"usage","data":[],"exit":2,"hints":[{"command":"ncp server list","message":"To see the configured servers, run `ncp server list`"}],"message":"No server `missing` is configured in `[HOME]/config/ncp/mcp.json`"}}
    "#);
    run.error();
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
}

#[test]
fn add_writes_the_entry_and_next_commands() {
    let home = Home::new("snap-server-add");
    let run = ncp_snapshot!(home.filters(), home.ncp([
        "server", "add", "web", "https://web.example.com/mcp"
    ]), @"
    exit_code: 0 (success)
    ----- stdout -----
    Created [HOME]/config/ncp/mcp.json.
    Added web to [HOME]/config/ncp/mcp.json.
    Next:
      ncp auth login web   (if the server needs a login)
      ncp server show web
    ");
    assert!(run.0.stdout.ends_with(b"\n"));
    assert!(home.read("config/ncp/mcp.json").ends_with("}\n"));
    insta::with_settings!({filters => home.filters()}, {
        insta::assert_snapshot!(home.read("config/ncp/mcp.json"), @r#"
        {
          "mcpServers": {
            "web": {
              "type": "http",
              "url": "https://web.example.com/mcp"
            }
          }
        }
        "#);
    });
}

#[test]
fn add_in_json_reports_the_entry() {
    let home = Home::new("snap-server-add-json");
    let run = ncp_snapshot!(home.filters(), home.ncp([
        "--output-format", "json", "server", "add", "web", "https://web.example.com/mcp"
    ]), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"changed":true,"created":true,"file":"[HOME]/config/ncp/mcp.json","nextSteps":[{"command":"ncp auth login web","when":"the server needs a login"},{"command":"ncp server show web"}],"scope":"global","secrets":[],"server":"web","target":"https://web.example.com/mcp","transport":"http"}
    "#);
    assert!(run.0.stdout.ends_with(b"\n"));
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
    assert!(home.read("config/ncp/mcp.json").ends_with("}\n"));
    insta::with_settings!({filters => home.filters()}, {
        insta::assert_snapshot!(home.read("config/ncp/mcp.json"), @r#"
        {
          "mcpServers": {
            "web": {
              "type": "http",
              "url": "https://web.example.com/mcp"
            }
          }
        }
        "#);
    });
}

#[test]
fn add_rejects_an_existing_name_and_keeps_the_file() {
    let home = Home::new("snap-server-add-error");
    home.configure(&json!({"web": {"type": "http", "url": "https://web.example.com/mcp"}}));
    let before = home.read("config/ncp/mcp.json");
    let run = ncp_snapshot!(home.filters(), home.ncp([
        "server", "add", "web", "https://other.example.com/mcp"
    ]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: ncp cannot change `[HOME]/config/ncp/mcp.json`
      cause: The file already has an entry of the server `web`

    hint: To remove the entry, run `ncp server remove web --global`
    ");
    assert!(run.0.stderr.ends_with(b"\n"));
    let run = ncp_snapshot!(home.filters(), home.ncp([
        "--output-format", "json", "server", "add", "web", "https://other.example.com/mcp"
    ]), @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    {"error":{"causes":["The file already has an entry of the server `web`"],"code":"usage","data":[],"exit":2,"hints":[{"message":"To remove the entry, run `ncp server remove web --global`"}],"message":"ncp cannot change `[HOME]/config/ncp/mcp.json`"}}
    "#);
    run.error();
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
    assert_eq!(home.read("config/ncp/mcp.json"), before);
    insta::with_settings!({filters => home.filters()}, {
        insta::assert_snapshot!(home.read("config/ncp/mcp.json"), @r#"{"mcpServers":{"web":{"type":"http","url":"https://web.example.com/mcp"}}}"#);
    });
}

#[test]
fn remove_writes_the_file_without_the_entry() {
    let home = Home::new("snap-server-remove").with_fake_keychain();
    home.configure(&json!({
        "fx": {"command": FIXTURE, "args": ["modern"]},
        "other": {"command": "other-server"}
    }));
    let run = ncp_snapshot!(home.filters(), home.ncp(["server", "remove", "fx"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    Removed fx from [HOME]/config/ncp/mcp.json.
    ");
    assert!(run.0.stdout.ends_with(b"\n"));
    insta::with_settings!({filters => home.filters()}, {
        insta::assert_snapshot!(home.read("config/ncp/mcp.json"), @r#"{"mcpServers":{"other":{"command":"other-server"}}}"#);
    });
}

#[test]
fn remove_in_json_reports_the_removed_entry() {
    let home = Home::new("snap-server-remove-json").with_fake_keychain();
    home.configure(&json!({
        "fx": {"command": FIXTURE, "args": ["modern"]},
        "other": {"command": "other-server"}
    }));
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "server", "remove", "fx"]),
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"credentialStores":[],"file":"[HOME]/config/ncp/mcp.json","scope":"global","server":"fx","tokensStay":false}
    "#
    );
    assert!(run.0.stdout.ends_with(b"\n"));
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
    insta::with_settings!({filters => home.filters()}, {
        insta::assert_snapshot!(home.read("config/ncp/mcp.json"), @r#"{"mcpServers":{"other":{"command":"other-server"}}}"#);
    });
}

#[test]
fn remove_names_an_unknown_server_and_keeps_the_file() {
    let home = Home::new("snap-server-remove-error").with_fake_keychain();
    home.configure(&json!({"fx": {"command": FIXTURE, "args": ["modern"]}}));
    let before = home.read("config/ncp/mcp.json");
    let run = ncp_snapshot!(home.filters(), home.ncp(["server", "remove", "missing"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No server `missing` is configured in `[HOME]/config/ncp/mcp.json`

    hint: To see the configured servers, run `ncp server list`
    ");
    assert!(run.0.stderr.ends_with(b"\n"));
    let run = ncp_snapshot!(home.filters(), home.ncp(["--output-format", "json", "server", "remove", "missing"]), @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    {"error":{"causes":[],"code":"usage","data":[],"exit":2,"hints":[{"command":"ncp server list","message":"To see the configured servers, run `ncp server list`"}],"message":"No server `missing` is configured in `[HOME]/config/ncp/mcp.json`"}}
    "#);
    run.error();
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
    assert_eq!(home.read("config/ncp/mcp.json"), before);
    insta::with_settings!({filters => home.filters()}, {
        insta::assert_snapshot!(home.read("config/ncp/mcp.json"), @r#"{"mcpServers":{"fx":{"args":["modern"],"command":"[FIXTURE]"}}}"#);
    });
}

const PROMPT: &[u8] = b"Trust this file? [y/N] ";

#[test]
fn add_in_an_untrusted_project_stops_at_a_no() {
    let home = Home::new("snap-server-add-untrusted-no")
        .project("app", &json!({"fs": {"command": "x"}}))
        .set("NCP_TEST_TTY", "1");
    let before = std::fs::read(home.root.join("app/.mcp.json")).unwrap();
    let run = ncp_snapshot!(home.filters(), home.ncp([
        "server", "add", "web", "https://web.example.com/mcp"
    ]), input = "n\n", @"
    exit_code: 2 (failure)
    ----- stderr -----
    [HOME]/app/.mcp.json is not trusted. It has these servers:
      fs  x

    Trust this file? [y/N] error: You did not trust `[HOME]/app/.mcp.json`, so ncp did not change it

    hint: To change the global file, run the command again with `--global`
    ");
    assert!(run.0.stderr.windows(PROMPT.len()).any(|w| w == PROMPT));
    assert_eq!(
        std::fs::read(home.root.join("app/.mcp.json")).unwrap(),
        before
    );
}

#[test]
fn add_in_json_in_an_untrusted_project_stops_at_a_no() {
    let home = Home::new("snap-server-add-untrusted-no-json")
        .project("app", &json!({"fs": {"command": "x"}}))
        .set("NCP_TEST_TTY", "1");
    let before = std::fs::read(home.root.join("app/.mcp.json")).unwrap();
    let run = ncp_snapshot!(home.filters(), home.ncp([
        "--output-format", "json", "server", "add", "web", "https://web.example.com/mcp"
    ]), input = "n\n", @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    [HOME]/app/.mcp.json is not trusted. It has these servers:
      fs  x

    Trust this file? [y/N] {"error":{"causes":[],"code":"usage","data":[],"exit":2,"hints":[{"message":"To change the global file, run the command again with `--global`"}],"message":"You did not trust `[HOME]/app/.mcp.json`, so ncp did not change it"}}
    "#);
    let at = run
        .0
        .stderr
        .windows(PROMPT.len())
        .position(|w| w == PROMPT)
        .unwrap();
    let rest = &run.0.stderr[at + PROMPT.len()..];
    common::Run(std::process::Output {
        stderr: rest.to_vec(),
        ..run.0.clone()
    })
    .error();
    assert_eq!(
        std::fs::read(home.root.join("app/.mcp.json")).unwrap(),
        before
    );
}

#[test]
fn add_in_an_untrusted_project_trusts_and_writes_at_a_yes() {
    let home = Home::new("snap-server-add-untrusted-yes")
        .project("app", &json!({"fs": {"command": "x"}}))
        .set("NCP_TEST_TTY", "1");
    let run = ncp_snapshot!(home.filters(), home.ncp([
        "server", "add", "web", "https://web.example.com/mcp"
    ]), input = "y\n", @"
    exit_code: 0 (success)
    ----- stdout -----
    Added web to [HOME]/app/.mcp.json.
    Next:
      ncp auth login web   (if the server needs a login)
      ncp server show web

    ----- stderr -----
    [HOME]/app/.mcp.json is not trusted. It has these servers:
      fs  x

    Trust this file? [y/N]
    ");
    assert!(run.0.stderr.ends_with(PROMPT));
    let text = home.read("app/.mcp.json");
    // The edit keeps the original layout, which has no final newline.
    assert!(text.ends_with("}}"), "{text:?}");
    insta::with_settings!({filters => home.filters()}, {
        insta::assert_snapshot!(home.read("app/.mcp.json"), @r#"
        {"mcpServers":{
            "fs":{"command":"x"},
            "web": {
              "type": "http",
              "url": "https://web.example.com/mcp"
            }
          }}
        "#);
    });
}
