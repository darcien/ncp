//! Command transcripts for `trust --show`, `--untrust`, `--ignore` and a declined trust question.

#![allow(clippy::unwrap_used)]

mod common;

use common::{Home, ncp_snapshot};
use serde_json::json;

const FIXTURE: &str = env!("CARGO_BIN_EXE_ncp-fixture");

fn project(home: Home) -> Home {
    home.project(
        "app",
        &json!({"fx": {"command": FIXTURE, "args": ["modern"]}}),
    )
}

fn record_in(home: &Home, dir: &str) -> std::path::PathBuf {
    let name = ncp::project::record_name(&home.project_file());
    home.root.join("state/ncp").join(dir).join(name)
}

fn record_path(home: &Home) -> std::path::PathBuf {
    record_in(home, "trusted-configs")
}

fn ignore_path(home: &Home) -> std::path::PathBuf {
    record_in(home, "ignored-configs")
}

#[test]
fn trust_show_reports_untrusted_then_trusted_and_writes_nothing() {
    let home = project(Home::new("trust-option-show"));
    let run = ncp_snapshot!(home.filters(), home.ncp(["trust", "--show"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    Project file: [HOME]/app/.mcp.json (untrusted)
      fx  [FIXTURE] modern
    ");
    assert!(run.0.stdout.ends_with(b"\n"));
    assert_eq!(run.0.stderr, [] as [u8; 0]);
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "trust", "--show"]),
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"file":"[HOME]/app/.mcp.json","invalid":[],"servers":[{"cwd":null,"env":[],"file":"[HOME]/app/.mcp.json","name":"fx","scope":"project","sends":[],"target":"[FIXTURE] modern","transport":"stdio","trust":"untrusted"}],"trust":"untrusted"}
    "#
    );
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
    assert!(!record_path(&home).exists());
    assert!(!ignore_path(&home).exists());
    assert_eq!(home.ncp(["trust"]).output().unwrap().status.code(), Some(0));
    let run = ncp_snapshot!(home.filters(), home.ncp(["trust", "--show"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    Project file: [HOME]/app/.mcp.json (trusted)
      fx  [FIXTURE] modern
    ");
    assert!(run.0.stdout.ends_with(b"\n"));
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "trust", "--show"]),
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"file":"[HOME]/app/.mcp.json","invalid":[],"servers":[{"cwd":null,"env":[],"file":"[HOME]/app/.mcp.json","name":"fx","scope":"project","sends":[],"target":"[FIXTURE] modern","transport":"stdio","trust":"trusted"}],"trust":"trusted"}
    "#
    );
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
    assert!(!ignore_path(&home).exists());
}

#[test]
fn trust_untrust_removes_the_trust_record_in_text() {
    let home = project(Home::new("trust-option-untrust-text"));
    assert_eq!(home.ncp(["trust"]).output().unwrap().status.code(), Some(0));
    assert!(record_path(&home).exists());
    let run = ncp_snapshot!(home.filters(), home.ncp(["trust", "--untrust"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    Project file: [HOME]/app/.mcp.json (untrusted)
      fx  [FIXTURE] modern
    ");
    assert!(run.0.stdout.ends_with(b"\n"));
    assert!(!record_path(&home).exists());
}

#[test]
fn trust_untrust_removes_the_trust_record_in_json() {
    let home = project(Home::new("trust-option-untrust-json"));
    assert_eq!(home.ncp(["trust"]).output().unwrap().status.code(), Some(0));
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "trust", "--untrust"]),
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"file":"[HOME]/app/.mcp.json","invalid":[],"servers":[{"cwd":null,"env":[],"file":"[HOME]/app/.mcp.json","name":"fx","scope":"project","sends":[],"target":"[FIXTURE] modern","transport":"stdio","trust":"untrusted"}],"trust":"untrusted"}
    "#
    );
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
    assert!(!record_path(&home).exists());
}

#[test]
fn trust_ignore_records_the_file_as_ignored_in_text() {
    let home = project(Home::new("trust-option-ignore-text"));
    let run = ncp_snapshot!(home.filters(), home.ncp(["trust", "--ignore"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    Project file: [HOME]/app/.mcp.json (ignored)
      fx  [FIXTURE] modern
    ");
    assert!(run.0.stdout.ends_with(b"\n"));
    let record = ignore_path(&home);
    assert!(record.exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode =
            |path: &std::path::Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&record), 0o600);
        assert_eq!(mode(record.parent().unwrap()), 0o700);
    }
    insta::with_settings!({filters => home.filters()}, {
        insta::assert_snapshot!(std::fs::read_to_string(&record).unwrap(), @"");
    });
    assert!(!record_path(&home).exists());
}

#[test]
fn trust_ignore_records_the_file_as_ignored_in_json() {
    let home = project(Home::new("trust-option-ignore-json"));
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "trust", "--ignore"]),
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"file":"[HOME]/app/.mcp.json","invalid":[],"servers":[{"cwd":null,"env":[],"file":"[HOME]/app/.mcp.json","name":"fx","scope":"project","sends":[],"target":"[FIXTURE] modern","transport":"stdio","trust":"ignored"}],"trust":"ignored"}
    "#
    );
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
    assert!(ignore_path(&home).exists());
    assert!(!record_path(&home).exists());
}

#[test]
fn trust_question_declined_stops_without_a_record_in_text() {
    let home = project(Home::new("trust-option-decline-text")).set("NCP_TEST_TTY", "1");
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["tool", "list", "fx"]),
        input = "n\n",
        @"
    exit_code: 2 (failure)
    ----- stderr -----
    [HOME]/app/.mcp.json is not trusted. It has these servers:
      fx  [FIXTURE] modern

    Trust this file? [y/N] error: The server `fx` is in `[HOME]/app/.mcp.json`, which you ignored

    hint: To trust the file, run `ncp trust`
    "
    );
    let prompt = format!(
        "{} is not trusted. It has these servers:\n  fx  {FIXTURE} modern\n\nTrust this file? [y/N] ",
        home.project_file().display()
    );
    assert!(run.0.stderr.starts_with(prompt.as_bytes()));
    assert_eq!(run.0.stdout, [] as [u8; 0]);
    assert!(ignore_path(&home).exists());
    assert!(!record_path(&home).exists());
    assert!(!home.root.join("state/ncp/trusted-configs").exists());
}

#[test]
fn trust_question_declined_stops_without_a_record_in_json() {
    let home = project(Home::new("trust-option-decline-json")).set("NCP_TEST_TTY", "1");
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "tool", "list", "fx"]),
        input = "n\n",
        @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    [HOME]/app/.mcp.json is not trusted. It has these servers:
      fx  [FIXTURE] modern

    Trust this file? [y/N] {"error":{"causes":[],"code":"usage","data":[],"exit":2,"hints":[{"message":"To trust the file, run `ncp trust`"}],"message":"The server `fx` is in `[HOME]/app/.mcp.json`, which you ignored","server":"fx"}}
    "#
    );
    let prompt = format!(
        "{} is not trusted. It has these servers:\n  fx  {FIXTURE} modern\n\nTrust this file? [y/N] ",
        home.project_file().display()
    );
    let error = run.0.stderr.strip_prefix(prompt.as_bytes()).unwrap();
    assert!(error.ends_with(b"}\n"));
    let error = common::Run(std::process::Output {
        stderr: error.to_vec(),
        ..run.0.clone()
    })
    .error();
    assert_eq!(error["code"], "usage");
    assert_eq!(run.0.stdout, [] as [u8; 0]);
    assert!(ignore_path(&home).exists());
    assert!(!home.root.join("state/ncp/trusted-configs").exists());
}
