//! Command transcripts and selected files for project trust.

#![allow(clippy::unwrap_used)]

mod common;

use std::os::unix::fs::PermissionsExt;

use common::{Home, ncp_snapshot};
use serde_json::json;

const FIXTURE: &str = env!("CARGO_BIN_EXE_ncp-fixture");

fn project(home: Home) -> Home {
    home.project(
        "app",
        &json!({"fx": {"command": FIXTURE, "args": ["modern"]}}),
    )
}

fn trusted_record(home: &Home) -> String {
    let name = ncp::project::record_name(&home.project_file());
    let path = home.root.join("state/ncp/trusted-configs").join(name);
    let record = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        record,
        ncp::project::digest(home.read("app/.mcp.json").as_bytes())
    );
    assert_eq!(record.len(), 64);
    assert!(!record.ends_with('\n'));
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    record
}

#[test]
fn trust_shows_the_entries_and_trusts_the_file() {
    let home = project(Home::new("trust-snapshot-success"));
    let run = ncp_snapshot!(home.filters(), home.ncp(["trust"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    Project file: [HOME]/app/.mcp.json (trusted)
      fx  [FIXTURE] modern
    ");
    assert!(run.0.stdout.ends_with(b"\n"));
    assert_eq!(run.0.stderr, [] as [u8; 0]);
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "trust"]),
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"file":"[HOME]/app/.mcp.json","invalid":[],"servers":[{"cwd":null,"env":[],"file":"[HOME]/app/.mcp.json","name":"fx","scope":"project","sends":[],"target":"[FIXTURE] modern","transport":"stdio","trust":"trusted"}],"trust":"trusted"}
    "#
    );
    assert!(run.0.stdout.ends_with(b"\n"));
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
    let record = trusted_record(&home);
    let digest_pattern = regex::escape(&record);
    let mut filters = home.filters();
    filters.push((&digest_pattern, "[DIGEST]"));
    insta::with_settings!({filters => filters}, {
        insta::assert_snapshot!(record, @"[DIGEST]");
        insta::assert_snapshot!(home.read("app/.mcp.json"), @r#"{"mcpServers":{"fx":{"args":["modern"],"command":"[FIXTURE]"}}}"#);
    });
}

#[test]
fn trust_with_no_project_file_names_where_the_search_stopped() {
    let home = Home::new("trust-snapshot-none");
    let run = ncp_snapshot!(home.filters(), home.ncp(["trust"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: ncp found no `.mcp.json` from the current directory up to `[HOME]`
    ");
    assert!(run.0.stderr.ends_with(b"\n"));
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "trust"]),
        @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    {"error":{"causes":[],"code":"usage","data":[],"exit":2,"hints":[],"message":"ncp found no `.mcp.json` from the current directory up to `[HOME]`"}}
    "#
    );
    run.error();
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
    assert!(run.0.stderr.ends_with(b"\n"));
    assert!(!home.root.join("state/ncp/trusted-configs").exists());
}

#[test]
fn trust_question_accepts_yes_and_runs_the_command_in_text() {
    let home = project(Home::new("trust-snapshot-question-text")).set("NCP_TEST_TTY", "1");
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["tool", "call", "fx", "echo", "text=hi"]),
        input = "y\n",
        @"
    exit_code: 0 (success)
    ----- stdout -----
    hi

    ----- stderr -----
    [HOME]/app/.mcp.json is not trusted. It has these servers:
      fx  [FIXTURE] modern

    Trust this file? [y/N] 
    "
    );
    assert_eq!(run.0.stdout, b"hi\n");
    assert!(run.0.stderr.ends_with(b"\n\nTrust this file? [y/N] "));
    assert!(!run.0.stderr.ends_with(b"\n"));
    let record = trusted_record(&home);
    let digest_pattern = regex::escape(&record);
    let mut filters = home.filters();
    filters.push((&digest_pattern, "[DIGEST]"));
    insta::with_settings!({filters => filters}, {
        insta::assert_snapshot!(record, @"[DIGEST]");
        insta::assert_snapshot!(home.read("app/.mcp.json"), @r#"{"mcpServers":{"fx":{"args":["modern"],"command":"[FIXTURE]"}}}"#);
    });
}

#[test]
fn trust_question_stays_plain_text_and_accepts_yes_in_json() {
    let home = project(Home::new("trust-snapshot-question-json")).set("NCP_TEST_TTY", "1");
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "tool", "call", "fx", "echo", "text=hi"]),
        input = "y\n",
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"content":[{"text":"hi","type":"text"}],"isError":false,"resultType":"complete"}

    ----- stderr -----
    [HOME]/app/.mcp.json is not trusted. It has these servers:
      fx  [FIXTURE] modern

    Trust this file? [y/N] 
    "#
    );
    assert_eq!(run.stdout()["content"][0]["text"], "hi");
    assert!(run.0.stdout.ends_with(b"\n"));
    assert!(run.0.stderr.ends_with(b"\n\nTrust this file? [y/N] "));
    assert!(!run.0.stderr.ends_with(b"\n"));
    let record = trusted_record(&home);
    let digest_pattern = regex::escape(&record);
    let mut filters = home.filters();
    filters.push((&digest_pattern, "[DIGEST]"));
    insta::with_settings!({filters => filters}, {
        insta::assert_snapshot!(record, @"[DIGEST]");
        insta::assert_snapshot!(home.read("app/.mcp.json"), @r#"{"mcpServers":{"fx":{"args":["modern"],"command":"[FIXTURE]"}}}"#);
    });
}
