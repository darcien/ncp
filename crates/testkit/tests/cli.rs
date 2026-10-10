//! Tests of the `ncp` command line against local fixture servers.
//!
//! Each test runs the binary with a clean environment and its own `HOME`
//! and XDG directories. stdout is a pipe, so the output is JSON.

// The whole file is test code, where the agent rules permit unwrap.
#![allow(clippy::unwrap_used)]

use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

mod common;

use common::{Home, NCP, Run};
use ncp_testkit::{
    Mode,
    auth_server::{AuthSettings, Registration},
    serve_http, serve_http_with,
};
use serde_json::{Value, json};

const FIXTURE: &str = env!("CARGO_BIN_EXE_ncp-fixture");

fn stdio(mode: &str) -> Value {
    json!({"command": FIXTURE, "args": [mode]})
}

fn text_of(result: &Value) -> &str {
    result["content"][0]["text"].as_str().unwrap()
}

#[test]
fn server_list_without_configuration_file_says_so_and_creates_no_file() {
    let home = Home::new("no-config");
    let run = home.run(&["server", "list"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(run.stdout(), json!({"servers": [], "invalid": []}));
    let hint = &run.stderr()[0]["hint"]["message"];
    assert!(hint.as_str().unwrap().contains("does not exist"), "{hint}");
    assert!(!home.config_path().exists());
}

#[test]
fn server_list_shows_servers_invalid_entries_and_unknown_keys() {
    let home = Home::new("list");
    home.configure(&json!({
        "local": {"command": "srv", "args": ["-v", "a b"], "disabled": true},
        "remote": {"type": "http", "url": "https://example.com/mcp"},
        "broken": {"url": "https://example.com/mcp"}
    }));
    let run = home.run(&["server", "list"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(
        run.stdout(),
        json!({
            "servers": [
                {"name": "local", "transport": "stdio", "target": "srv -v 'a b'"},
                {"name": "remote", "transport": "http", "target": "https://example.com/mcp"}
            ],
            "invalid": [{"name": "broken", "error":
                "entry has a \"url\" but no \"type\"; add \"type\": \"http\" or \"type\": \"sse\""}]
        })
    );
    assert_eq!(
        run.stderr(),
        [json!({"warning": {"server": "local", "message": "unknown key \"disabled\" ignored"}})]
    );
}

#[test]
fn configuration_file_that_does_not_parse_is_a_usage_error() {
    let home = Home::new("bad-config");
    let path = home.config_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "{").unwrap();
    let run = home.run(&["server", "list"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert!(run.stdout_is_empty());
    assert_eq!(run.error()["code"], "usage");
}

#[test]
fn tool_list_gives_mcp_tool_objects_and_names_server_instructions() {
    let home = Home::new("tool-list");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = home.run(&["tool", "list", "fx"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let output = run.stdout();
    assert_eq!(output["server"], "fx");
    assert_eq!(output["tools"][0]["name"], "echo");
    assert_eq!(
        output["tools"][0]["inputSchema"]["properties"]["text"]["type"],
        "string"
    );
    let hint = run.stderr()[0]["hint"]["message"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(hint.contains("ncp server show fx"), "{hint}");
}

#[test]
fn server_show_gives_the_handshake_data_with_instructions() {
    let home = Home::new("server-show");
    home.configure(&json!({"fx": stdio("legacy")}));
    let run = home.run(&["server", "show", "fx"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let output = run.stdout();
    assert_eq!(output["server"], "fx");
    assert_eq!(output["protocolVersion"], "2025-11-25");
    assert_eq!(
        output["instructions"],
        "Fixture server for the tests of ncp."
    );
    assert!(output["capabilities"]["tools"].is_object());
    assert!(
        output["target"]
            .as_str()
            .unwrap()
            .ends_with("ncp-fixture legacy")
    );
}

#[test]
fn tool_show_gives_the_tool_and_tool_call_help_gives_the_same() {
    let home = Home::new("tool-show");
    home.configure(&json!({"fx": stdio("modern")}));
    let show = home.run(&["tool", "show", "fx", "wait"]);
    assert_eq!(show.code(), 0, "{}", show.describe());
    assert_eq!(show.stdout()["tool"]["name"], "wait");
    let help = home.run(&["tool", "call", "fx", "wait", "--help"]);
    assert_eq!(help.code(), 0, "{}", help.describe());
    assert_eq!(help.stdout(), show.stdout());
}

#[test]
fn tool_call_reads_key_value_arguments_with_the_schema_types() {
    let home = Home::new("call-pairs");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = home.run(&["tool", "call", "fx", "wait", "ms=10"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(text_of(&run.stdout()), "done");
    let run = home.run(&["tool", "call", "fx", "echo", "text=123"]);
    assert_eq!(text_of(&run.stdout()), "123");
}

#[test]
fn tool_call_reads_a_json_object_from_an_argument_or_stdin() {
    let home = Home::new("call-json");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = home.run(&["tool", "call", "fx", "echo", r#"{"text": "from json"}"#]);
    assert_eq!(text_of(&run.stdout()), "from json");
    let run = home.run_with_stdin(&["tool", "call", "fx", "echo", "-"], r#"{"text": "piped"}"#);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(text_of(&run.stdout()), "piped");
}

#[test]
fn tool_call_checks_arguments_before_the_call_unless_told_not_to() {
    let home = Home::new("call-check");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = home.run(&["tool", "call", "fx", "wait", "ms=soon"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert!(run.stdout_is_empty());
    let message = run.error()["message"].as_str().unwrap().to_string();
    assert!(message.contains("/ms:"), "{message}");
    assert!(message.contains("ms (integer, optional)"), "{message}");
    let run = home.run(&[
        "tool",
        "call",
        "fx",
        "echo",
        "--no-validate",
        r#"{"text": 5}"#,
    ]);
    assert_eq!(run.code(), 0, "{}", run.describe());
}

#[test]
fn tool_error_result_goes_to_stdout_with_exit_1() {
    let home = Home::new("call-tool-error");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = home.run(&["tool", "call", "fx", "fail", "text=bad"]);
    assert_eq!(run.code(), 1, "{}", run.describe());
    assert_eq!(run.stdout()["isError"], true);
    assert_eq!(run.error()["code"], "tool");
}

#[test]
fn input_required_result_goes_to_stdout_with_exit_5() {
    let home = Home::new("call-input-required");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = home.run(&["tool", "call", "fx", "ask"]);
    assert_eq!(run.code(), 5, "{}", run.describe());
    assert_eq!(run.stdout()["requestState"], "state-1");
    assert_eq!(run.error()["code"], "input_required");
}

#[test]
fn joined_server_and_tool_name_gives_the_two_word_command() {
    let home = Home::new("joined");
    let run = home.run(&["tool", "call", "github.search_issues", "q=x"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert_eq!(
        run.error()["hint"],
        "ncp tool call github search_issues q=x"
    );
}

#[test]
fn argument_in_place_of_the_tool_gives_missing_tool_name() {
    let home = Home::new("missing-tool");
    let run = home.run(&["tool", "call", "github", "q=x"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    let error = run.error();
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .starts_with("missing tool name")
    );
    assert_eq!(error["hint"], "ncp tool list github");
}

#[test]
fn unknown_server_and_unknown_tool_name_the_command_that_lists_them() {
    let home = Home::new("unknown");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = home.run(&["tool", "list", "nope"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert_eq!(run.error()["hint"], "ncp server list");
    let run = home.run(&["tool", "call", "fx", "nope"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert_eq!(run.error()["hint"], "ncp tool list fx");
}

#[test]
fn unset_variable_in_args_gives_a_usage_error_with_the_fix() {
    let home = Home::new("unset-args");
    home.configure(&json!({"fx": {"command": FIXTURE, "args": ["${env:FIXTURE_MODE}"]}}));
    let run = home.run(&["tool", "list", "fx"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert_eq!(run.error()["hint"], "export FIXTURE_MODE=<value>");
    let home = home.set("FIXTURE_MODE", "modern");
    assert_eq!(home.run(&["tool", "list", "fx"]).code(), 0);
}

fn env_server() -> Value {
    json!({"fx": {"command": FIXTURE, "args": ["modern"],
        "env": {"TOKEN": "${env:FIXTURE_TOKEN}"}}})
}

fn env_value(home: &Home) -> Run {
    home.run(&["tool", "call", "fx", "env", "name=TOKEN"])
}

#[test]
fn name_that_no_place_has_names_the_secret_set_command() {
    let home = Home::new("unset-env").with_fake_keychain();
    home.configure(&env_server());
    let run = env_value(&home);
    assert_eq!(run.code(), 2, "{}", run.describe());
    let error = run.error();
    assert_eq!(error["message"], "FIXTURE_TOKEN is not set and not stored");
    assert_eq!(error["hint"], "ncp secret set FIXTURE_TOKEN");
}

#[test]
fn name_with_no_native_store_names_the_file_option() {
    let home = Home::new("no-native");
    home.configure(&env_server());
    let run = env_value(&home);
    assert_eq!(run.code(), 4, "{}", run.describe());
    assert_eq!(
        run.error()["hint"],
        "ncp secret set FIXTURE_TOKEN --credential-store file"
    );
    let run = home.run_with_stdin(&["secret", "set", "FIXTURE_TOKEN"], "v\n");
    assert_eq!(run.code(), 4, "{}", run.describe());
    assert_eq!(
        run.error()["hint"],
        "ncp secret set FIXTURE_TOKEN --credential-store file"
    );
}

#[test]
fn value_from_the_credential_file_expands_in_env_and_the_environment_wins() {
    let home = Home::new("file-secret");
    home.configure(&env_server());
    let run = home.run_with_stdin(
        &[
            "secret",
            "set",
            "FIXTURE_TOKEN",
            "--credential-store",
            "file",
        ],
        "from-file\n",
    );
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(
        run.stdout(),
        json!({"name": "FIXTURE_TOKEN", "place": "file"})
    );
    let path = home.root.join("state/ncp/credentials.json");
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    assert_eq!(text_of(&env_value(&home).stdout()), "from-file");
    let home = home.set("FIXTURE_TOKEN", "from-env");
    assert_eq!(text_of(&env_value(&home).stdout()), "from-env");
}

#[test]
fn native_store_comes_before_the_file_and_list_shows_both_places() {
    let home = Home::new("native-secret").with_fake_keychain();
    home.configure(&env_server());
    let set = |args: &[&str], value: &str| {
        let run = home.run_with_stdin(args, value);
        assert_eq!(run.code(), 0, "{}", run.describe());
        run
    };
    set(
        &[
            "secret",
            "set",
            "FIXTURE_TOKEN",
            "--credential-store",
            "file",
        ],
        "file\n",
    );
    let run = set(&["secret", "set", "FIXTURE_TOKEN"], "native\n");
    assert!(
        run.stderr()
            .iter()
            .any(|line| line.get("warning").is_some()),
        "{}",
        run.describe()
    );
    set(
        &["secret", "set", "UNUSED", "--credential-store", "file"],
        "x",
    );
    assert_eq!(text_of(&env_value(&home).stdout()), "native");
    let list = home.run(&["secret", "list"]);
    assert_eq!(list.code(), 0, "{}", list.describe());
    assert_eq!(
        list.stdout(),
        json!({"secrets": [
            {"name": "FIXTURE_TOKEN", "places": ["native", "file"], "used": true},
            {"name": "UNUSED", "places": ["file"], "used": false}
        ]})
    );
    let calls = std::fs::read_to_string(home.root.join("keychain/calls.log")).unwrap();
    assert!(
        !calls.contains("native"),
        "a value is in an argument: {calls}"
    );
    let run = home.run(&["secret", "delete", "FIXTURE_TOKEN"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(
        run.stdout(),
        json!({"name": "FIXTURE_TOKEN", "places": ["native", "file"]})
    );
    let run = home.run(&["secret", "delete", "FIXTURE_TOKEN"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert_eq!(run.error()["hint"], "ncp secret list");
}

#[test]
fn secret_set_rejects_a_bad_name_and_an_empty_value() {
    let home = Home::new("secret-bad");
    let run = home.run_with_stdin(&["secret", "set", "A-B", "--credential-store", "file"], "x");
    assert_eq!(run.code(), 2, "{}", run.describe());
    let run = home.run_with_stdin(&["secret", "set", "A", "--credential-store", "file"], "\n");
    assert_eq!(run.code(), 2, "{}", run.describe());
}

#[tokio::test(flavor = "multi_thread")]
async fn stored_bearer_token_goes_in_a_static_header() {
    let fixture = serve_http_with(Mode::Modern, Some("t0ken")).await.unwrap();
    let home = Home::new("static-header");
    home.configure(&json!({"web": {"type": "http", "url": fixture.url,
        "headers": {"Authorization": "Bearer ${WEB_TOKEN}"}}}));
    let run = home.run_with_stdin(
        &["secret", "set", "WEB_TOKEN", "--credential-store", "file"],
        "t0ken",
    );
    assert_eq!(run.code(), 0, "{}", run.describe());
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
}

#[test]
fn server_list_reads_no_store() {
    let home = Home::new("list-no-store").with_fake_keychain();
    home.configure(&env_server());
    let run = home.run(&["server", "list"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert!(!home.root.join("keychain/calls.log").exists());
}

#[test]
fn keeps_the_era_in_a_private_cache_file() {
    let home = Home::new("era-cache");
    home.configure(&json!({"fx": stdio("legacy")}));
    assert_eq!(home.run(&["tool", "list", "fx"]).code(), 0);
    let path = home.root.join("cache/ncp/eras.json");
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let cache: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        cache["servers"]["fx"]["era"],
        json!({"era": "legacy", "version": "2025-11-25"})
    );
    assert_eq!(home.run(&["tool", "list", "fx"]).code(), 0);
}

#[test]
fn ignores_an_era_cache_file_that_does_not_load() {
    let home = Home::new("era-cache-broken");
    home.configure(&json!({"fx": stdio("modern")}));
    let path = home.root.join("cache/ncp/eras.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "not json").unwrap();
    let run = home.run(&["tool", "list", "fx"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert!(std::fs::read_to_string(&path).unwrap().contains("modern"));
}

#[test]
fn request_timeout_exits_with_3_and_names_the_option() {
    let home = Home::new("timeout");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = home.run(&["tool", "call", "fx", "wait", "ms=30000", "--timeout", "1"]);
    assert_eq!(run.code(), 3, "{}", run.describe());
    assert!(run.stdout_is_empty());
    let error = run.error();
    assert_eq!(error["code"], "protocol");
    assert!(error["hint"].as_str().unwrap().contains("--timeout"));
}

#[test]
fn server_that_cannot_start_exits_with_3() {
    let home = Home::new("no-start");
    home.configure(&json!({"fx": {"command": "/nonexistent/ncp-server"}}));
    let run = home.run(&["tool", "list", "fx"]);
    assert_eq!(run.code(), 3, "{}", run.describe());
    assert_eq!(run.error()["server"], "fx");
}

#[test]
fn usage_error_from_the_parser_is_a_json_error_in_a_pipe() {
    let home = Home::new("parse-error");
    let run = home.run(&["tool", "frob"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert_eq!(run.error()["code"], "usage");
    let run = home.run(&["--version"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
}

#[tokio::test(flavor = "multi_thread")]
async fn calls_a_tool_of_an_http_server() {
    let fixture = serve_http(Mode::Modern).await.unwrap();
    let home = Home::new("http");
    home.configure(&json!({"web": {"type": "http", "url": fixture.url,
        "headers": {"X-Org": "acme"}}}));
    let run = home.run(&["tool", "call", "web", "header", "name=x-org"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(text_of(&run.stdout()), "acme");
}

#[tokio::test(flavor = "multi_thread")]
async fn http_server_that_needs_a_login_exits_with_4() {
    let fixture = serve_http_with(Mode::Modern, Some("t0ken")).await.unwrap();
    let home = Home::new("http-401");
    home.configure(&json!({"web": {"type": "http", "url": fixture.url}}));
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 4, "{}", run.describe());
    assert_eq!(run.error()["code"], "auth");
    home.configure(&json!({"web": {"type": "http", "url": fixture.url,
        "headers": {"Authorization": "Bearer t0ken"}}}));
    assert_eq!(home.run(&["tool", "list", "web"]).code(), 0);
}

#[test]
fn needs_home_or_xdg_directories() {
    let run = Run(Command::new(NCP)
        .args(["server", "list"])
        .env_clear()
        .output()
        .unwrap());
    assert_eq!(run.code(), 3, "{}", run.describe());
    assert!(Path::new(NCP).exists());
}

#[test]
fn expanded_values_never_go_to_output_or_to_the_era_cache() {
    let home = Home::new("no-leak").set("TOKEN", "supersecret");
    home.configure(&json!({"fx": {"command": FIXTURE, "args": ["modern", "${TOKEN}"]}}));
    let list = home.run(&["server", "list"]);
    assert_eq!(
        list.stdout()["servers"][0]["target"],
        format!("{FIXTURE} modern '${{TOKEN}}'")
    );
    assert_eq!(home.run(&["server", "show", "fx"]).code(), 0);
    let cache = std::fs::read_to_string(home.root.join("cache/ncp/eras.json")).unwrap();
    assert!(cache.contains("${TOKEN}"), "{cache}");
    assert!(!cache.contains("supersecret"), "{cache}");
}

#[test]
fn server_stderr_lines_become_json_lines_in_a_pipe() {
    let home = Home::new("server-log");
    home.configure(&json!({"fx": {"command": "/bin/sh",
        "args": ["-c", format!("echo 'starting up' >&2; exec {FIXTURE} modern")]}}));
    let run = home.run(&["tool", "call", "fx", "echo", "text=x"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert!(
        run.stderr()
            .contains(&json!({"serverLog": {"server": "fx", "message": "starting up"}})),
        "{}",
        run.describe()
    );
}

#[test]
fn unknown_tool_with_no_check_is_a_usage_error() {
    let home = Home::new("unknown-no-check");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = home.run(&["tool", "call", "fx", "nope", "--no-validate"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert_eq!(run.error()["hint"], "ncp tool list fx");
}

#[test]
fn sse_entry_is_a_usage_error_that_names_the_file() {
    let home = Home::new("sse");
    home.configure(&json!({"old": {"type": "sse", "url": "https://example.com/sse"}}));
    let run = home.run(&["tool", "list", "old"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    let message = run.error()["message"].as_str().unwrap().to_string();
    assert!(message.contains("mcp.json"), "{message}");
}

#[test]
fn era_cache_that_cannot_be_written_gives_a_warning_only() {
    let home = Home::new("cache-read-only");
    home.configure(&json!({"fx": stdio("modern")}));
    let cache = home.root.join("cache");
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::set_permissions(&cache, std::fs::Permissions::from_mode(0o500)).unwrap();
    let run = home.run(&["tool", "call", "fx", "echo", "text=x"]);
    std::fs::set_permissions(&cache, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert!(
        run.stderr()
            .iter()
            .any(|line| line.get("warning").is_some()),
        "{}",
        run.describe()
    );
}

#[test]
fn no_command_says_missing_command() {
    let home = Home::new("no-command");
    let run = home.run(&[]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert_eq!(run.error()["message"], "missing command");
}

#[test]
fn server_stderr_that_is_not_utf8_does_not_stop_the_server() {
    let home = Home::new("server-log-bytes");
    home.configure(&json!({"fx": {"command": "/bin/sh", "args": ["-c",
        format!("printf 'x\\377y\\n' >&2; sleep 0.2; echo second >&2; exec {FIXTURE} modern")]}}));
    let run = home.run(&["tool", "call", "fx", "echo", "text=x"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let logs: Vec<Value> = run
        .stderr()
        .into_iter()
        .filter_map(|line| line.get("serverLog").map(|log| log["message"].clone()))
        .collect();
    assert_eq!(logs, [json!("x\u{fffd}y"), json!("second")]);
}

#[tokio::test(flavor = "multi_thread")]
async fn errors_show_the_configured_command_and_url_and_not_expanded_values() {
    let home = Home::new("no-leak-errors")
        .set("MYCMD", "/nonexistent/secret-cmd")
        .set("APIKEY", "SECRETKEY123");
    home.configure(&json!({
        "cmd": {"command": "${MYCMD}"},
        "web": {"type": "http", "url": "http://127.0.0.1:9/mcp?key=${APIKEY}"}
    }));
    for server in ["cmd", "web"] {
        let run = home.run(&["tool", "list", server]);
        assert_eq!(run.code(), 3, "{}", run.describe());
        let text = String::from_utf8_lossy(&run.0.stderr).into_owned();
        assert!(
            !text.contains("secret-cmd") && !text.contains("SECRETKEY123"),
            "{text}"
        );
    }
}

#[test]
fn long_server_log_lines_are_cut_between_characters() {
    let home = Home::new("server-log-long");
    home.configure(&json!({"fx": {"command": "/bin/sh", "args": ["-c", format!(
        "{{ head -c 65535 /dev/zero | tr '\\0' a; printf '\\303\\251\\n'; \
           head -c 65536 /dev/zero | tr '\\0' b; printf '\\n'; \
           printf 'del \\177 c1 \\302\\233\\n'; }} >&2; exec {FIXTURE} modern")]}}));
    let run = home.run(&["tool", "call", "fx", "echo", "text=x"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let logs: Vec<String> = run
        .stderr()
        .into_iter()
        .filter_map(|line| line["serverLog"]["message"].as_str().map(str::to_string))
        .collect();
    assert_eq!(logs.len(), 4, "{logs:?}");
    assert_eq!(logs[0], "a".repeat(65535));
    assert_eq!(logs[1], "é");
    assert_eq!(logs[2], "b".repeat(65536));
    assert_eq!(logs[3], "del \u{7f} c1 \u{9b}");
    let raw = String::from_utf8_lossy(&run.0.stderr).into_owned();
    assert!(raw.contains("del \\u007f c1 \\u009b"), "{raw}");
}

fn warnings(run: &Run) -> Vec<String> {
    run.stderr()
        .into_iter()
        .filter_map(|line| line["warning"]["message"].as_str().map(str::to_string))
        .collect()
}

#[test]
fn secret_set_warns_when_another_place_wins() {
    let home = Home::new("copy-warnings")
        .with_fake_keychain()
        .set("SHADOWED", "env");
    let run = home.run_with_stdin(&["secret", "set", "A"], "native");
    assert_eq!(warnings(&run), Vec::<String>::new(), "{}", run.describe());
    let run = home.run_with_stdin(
        &["secret", "set", "A", "--credential-store", "file"],
        "file",
    );
    assert_eq!(run.code(), 0, "{}", run.describe());
    let messages = warnings(&run);
    assert!(
        messages[0].contains("the new value is not used"),
        "{messages:?}"
    );
    assert!(
        run.stderr()
            .contains(&json!({"hint": {"message": "ncp secret delete A"}})),
        "{}",
        run.describe()
    );
    let run = home.run_with_stdin(
        &["secret", "set", "SHADOWED", "--credential-store", "file"],
        "x",
    );
    assert!(
        warnings(&run)[0].contains("environment"),
        "{}",
        run.describe()
    );
}

#[test]
fn secret_commands_work_with_no_native_store_and_no_noise() {
    let home = Home::new("file-only");
    let run = home.run_with_stdin(&["secret", "set", "A", "--credential-store", "file"], "x");
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(warnings(&run), Vec::<String>::new());
    let list = home.run(&["secret", "list"]);
    assert_eq!(warnings(&list), Vec::<String>::new());
    assert_eq!(list.stdout()["secrets"][0]["places"], json!(["file"]));
    let run = home.run(&["secret", "delete", "TYPO"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert_eq!(run.error()["message"], "TYPO is not stored");
}

#[test]
fn secret_list_works_with_a_configuration_that_does_not_load() {
    let home = Home::new("list-bad-config");
    let path = home.config_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "{").unwrap();
    home.run_with_stdin(&["secret", "set", "A", "--credential-store", "file"], "x");
    let run = home.run(&["secret", "list"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(run.stdout()["secrets"][0]["used"], false);
    assert_eq!(warnings(&run).len(), 1, "{}", run.describe());
}

#[test]
fn name_used_only_in_args_is_not_used_and_names_the_rule() {
    let home = Home::new("args-only");
    home.configure(&json!({"fx": {"command": FIXTURE, "args": ["${ARG_ONLY}"]}}));
    home.run_with_stdin(
        &["secret", "set", "ARG_ONLY", "--credential-store", "file"],
        "modern",
    );
    let list = home.run(&["secret", "list"]);
    assert_eq!(list.stdout()["secrets"][0]["used"], false);
    let run = home.run(&["tool", "list", "fx"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    let message = run.error()["message"].as_str().unwrap().to_string();
    assert!(message.contains("only in env and headers"), "{message}");
}

#[test]
fn copy_check_of_a_slow_native_store_stops_at_its_limit_and_leaves_no_program() {
    let home = Home::new("slow-copy-check").with_fake_keychain();
    std::fs::write(home.root.join("keychain/delay"), "20000").unwrap();
    let start = std::time::Instant::now();
    let run = home.run_with_stdin(&["secret", "set", "A", "--credential-store", "file"], "x");
    let elapsed = start.elapsed();
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert!(elapsed < std::time::Duration::from_secs(8), "{elapsed:?}");
    assert!(
        warnings(&run)[0].contains("cannot check"),
        "{}",
        run.describe()
    );
    assert!(
        !run.stderr().iter().any(|line| line.get("hint").is_some()),
        "{}",
        run.describe()
    );
    let program = home.root.join("keychain/security").display().to_string();
    let found = Command::new("pgrep")
        .args(["-f", &program])
        .output()
        .unwrap();
    assert!(found.stdout.is_empty(), "a security program still runs");
}

#[tokio::test(flavor = "multi_thread")]
async fn loopback_server_through_a_remote_proxy_gives_the_no_proxy_hint() {
    let fixture = serve_http(Mode::Modern).await.unwrap();
    let home = Home::new("remote-proxy").set("HTTP_PROXY", "http://proxy.invalid:3128");
    home.configure(&json!({"web": {"type": "http", "url": fixture.url}}));
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 3, "{}", run.describe());
    let error = run.error();
    assert_eq!(error["hint"], "export NO_PROXY=localhost,127.0.0.1,::1");
    let message = error["message"].as_str().unwrap();
    assert!(
        message.contains("proxy.invalid") && message.contains("HTTP_PROXY"),
        "{message}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn requests_go_through_the_proxy_unless_no_proxy_names_the_host() {
    let fixture = serve_http(Mode::Modern).await.unwrap();
    let proxy = ncp_testkit::serve_fake_proxy().await.unwrap();
    let home = Home::new("local-proxy").set("HTTP_PROXY", &proxy.url);
    home.configure(&json!({"web": {"type": "http", "url": fixture.url}}));
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 3, "{}", run.describe());
    let lines = proxy.request_lines.lock().unwrap().clone();
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with(&format!("POST {}", fixture.url))),
        "{lines:?}"
    );
    let home = home.set("NO_PROXY", "127.0.0.1");
    assert_eq!(home.run(&["tool", "list", "web"]).code(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn https_server_works_with_the_roots_in_ssl_cert_file() {
    let fixture = ncp_testkit::serve_https(Mode::Modern).await.unwrap();
    let home = Home::new("cert-file").set("SSL_CERT_FILE", ncp_testkit::TEST_CA);
    home.configure(&json!({"web": {"type": "http", "url": fixture.url}}));
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
}

#[tokio::test(flavor = "multi_thread")]
async fn https_server_with_an_unknown_ca_fails_without_ssl_cert_file() {
    let fixture = ncp_testkit::serve_https(Mode::Modern).await.unwrap();
    let home = Home::new("no-cert-file");
    home.configure(&json!({"web": {"type": "http", "url": fixture.url}}));
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 3, "{}", run.describe());
}

#[tokio::test(flavor = "multi_thread")]
async fn ssl_cert_file_without_the_root_of_the_server_fails_with_a_hint() {
    let fixture = ncp_testkit::serve_https(Mode::Modern).await.unwrap();
    let home = Home::new("other-cert-file").set("SSL_CERT_FILE", ncp_testkit::OTHER_CA);
    home.configure(&json!({"web": {"type": "http", "url": fixture.url}}));
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 3, "{}", run.describe());
    let hint = run.error()["hint"].as_str().unwrap_or_default().to_string();
    assert!(
        hint.contains("SSL_CERT_FILE") && hint.contains("each root"),
        "{}",
        run.describe()
    );
}

/// On Linux the system store reads `SSL_CERT_DIR`, so a root there is a
/// system root. The file of `SSL_CERT_FILE` replaces it.
#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread")]
async fn ssl_cert_file_replaces_the_system_roots_on_linux() {
    let fixture = ncp_testkit::serve_https(Mode::Modern).await.unwrap();
    let home = Home::new("cert-file-replaces");
    let system = home.root.join("system-roots");
    std::fs::create_dir_all(&system).unwrap();
    std::fs::copy(ncp_testkit::TEST_CA, system.join("ca.pem")).unwrap();
    let home = home.set("SSL_CERT_DIR", &system.display().to_string());
    home.configure(&json!({"web": {"type": "http", "url": fixture.url}}));
    let trusted = home.run(&["tool", "list", "web"]);
    assert_eq!(trusted.code(), 0, "{}", trusted.describe());
    let home = home.set("SSL_CERT_FILE", ncp_testkit::OTHER_CA);
    let replaced = home.run(&["tool", "list", "web"]);
    assert_eq!(replaced.code(), 3, "{}", replaced.describe());
}

#[test]
fn unreadable_ssl_cert_file_gives_a_local_error_that_names_the_variable() {
    let home = Home::new("bad-cert-file");
    let missing = home.root.join("missing.pem").display().to_string();
    let home = home.set("SSL_CERT_FILE", &missing);
    home.configure(&json!({"web": {"type": "http", "url": "https://127.0.0.1:9/mcp"}}));
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 3, "{}", run.describe());
    let message = run.error()["message"].as_str().unwrap().to_string();
    assert!(
        message.contains("SSL_CERT_FILE") && message.contains(&missing),
        "{message}"
    );
}

/// The JSON Lines on stderr with the kind as their key, for example
/// `debug`.
fn notes(run: &Run, kind: &str) -> Vec<Value> {
    run.stderr()
        .into_iter()
        .filter_map(|line| line.get(kind).cloned())
        .collect()
}

#[test]
fn verbose_output_names_the_source_of_each_value_and_never_the_value() {
    let home = Home::new("verbose-sources").set("FIXTURE_TOKEN", "s3cret-value");
    home.configure(&env_server());
    let run = home.run(&["-v", "tool", "call", "fx", "env", "name=TOKEN"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let messages: Vec<String> = notes(&run, "debug")
        .iter()
        .map(|note| note["message"].as_str().unwrap().to_string())
        .collect();
    assert!(
        messages.contains(&"FIXTURE_TOKEN from the environment".to_string()),
        "{messages:?}"
    );
    assert!(
        messages.contains(&"no cached era: probe with server/discover".to_string()),
        "{messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.starts_with("saved the era: modern")),
        "{messages:?}"
    );
    assert!(!String::from_utf8_lossy(&run.0.stderr).contains("s3cret-value"));
    let run = home.run(&["tool", "list", "fx"]);
    assert!(notes(&run, "debug").is_empty(), "{}", run.describe());
}

#[tokio::test(flavor = "multi_thread")]
async fn verbose_output_names_the_proxy_route_and_a_stored_value_source() {
    let fixture = serve_http_with(Mode::Modern, Some("t0ken")).await.unwrap();
    let home = Home::new("verbose-route")
        .set("HTTP_PROXY", "http://proxy.invalid:3128")
        .set("NO_PROXY", "127.0.0.1");
    home.configure(&json!({"web": {"type": "http", "url": fixture.url,
        "headers": {"Authorization": "Bearer ${WEB_TOKEN}"}}}));
    let set = &["secret", "set", "WEB_TOKEN", "--credential-store", "file"];
    assert_eq!(home.run_with_stdin(set, "t0ken").code(), 0);
    let run = home.run(&["-v", "tool", "list", "web"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let messages: Vec<String> = notes(&run, "debug")
        .iter()
        .map(|note| note["message"].as_str().unwrap().to_string())
        .collect();
    assert!(
        messages.contains(&"WEB_TOKEN from the credential file".to_string()),
        "{messages:?}"
    );
    let route = format!(
        "route to {}: direct, because NO_PROXY matches the host",
        fixture.url
    );
    assert!(messages.contains(&route), "{messages:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn trace_output_shows_messages_and_requests_with_secrets_hidden() {
    let fixture = serve_http_with(Mode::Modern, Some("t0ken")).await.unwrap();
    let home = Home::new("trace")
        .set("WEB_TOKEN", "t0ken")
        .set("API_KEY", "k3y");
    // A value from ${NAME} is a secret; a literal value shows as it is.
    let url = format!("{}?api_key=${{API_KEY}}&page=2", fixture.url);
    home.configure(&json!({"web": {"type": "http", "url": url,
        "headers": {"Authorization": "Bearer ${WEB_TOKEN}", "X-Org": "acme"}}}));
    let run = home.run(&["-vv", "tool", "list", "web"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let stderr = String::from_utf8_lossy(&run.0.stderr).to_string();
    assert!(
        !stderr.contains("t0ken") && !stderr.contains("k3y"),
        "{stderr}"
    );
    let traces = notes(&run, "trace");
    assert!(
        traces
            .iter()
            .any(|trace| trace["sent"]["method"] == "tools/list"),
        "{stderr}"
    );
    assert!(
        traces
            .iter()
            .any(|trace| trace["received"]["result"]["tools"].is_array()),
        "{stderr}"
    );
    let request = traces
        .iter()
        .find_map(|trace| trace.get("http"))
        .unwrap_or_else(|| panic!("no HTTP trace: {stderr}"));
    assert_eq!(request["method"], "POST");
    assert!(
        request["url"]
            .as_str()
            .unwrap()
            .ends_with("?api_key=${API_KEY}&page=2"),
        "{request}"
    );
    assert_eq!(request["headers"]["authorization"], "****");
    assert_eq!(request["headers"]["x-org"], "acme");
}

#[tokio::test(flavor = "multi_thread")]
async fn timings_show_cpu_and_network_steps_after_the_run() {
    let fixture = serve_http(Mode::Modern).await.unwrap();
    let home = Home::new("timings");
    home.configure(&json!({"web": {"type": "http", "url": fixture.url}}));
    let run = home.run(&["--timings", "tool", "list", "web"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let lines = run.stderr();
    let timings = &lines.last().unwrap()["timings"];
    for step in ["totalMs", "cpuMs", "configMs", "connectMs"] {
        assert!(timings[step].is_number(), "{step}: {timings}");
    }
    assert!(
        timings.get("dnsMs").is_none(),
        "an IP address needs no DNS: {timings}"
    );
    assert!(
        timings["responseMs"].as_array().unwrap().len() >= 2,
        "the handshake and tools/list: {timings}"
    );
    let failed = home.run(&["--timings", "tool", "list", "nothing"]);
    assert_eq!(failed.code(), 2);
    assert!(failed.stderr().last().unwrap()["timings"]["totalMs"].is_number());
}

async fn oauth_fixture(
    settings: AuthSettings,
) -> (
    ncp_testkit::auth_server::AuthServer,
    ncp_testkit::HttpFixture,
) {
    let auth = ncp_testkit::auth_server::serve_auth(settings)
        .await
        .unwrap();
    let fixture = ncp_testkit::serve_http_with_oauth(Mode::Modern, &auth, None)
        .await
        .unwrap();
    (auth, fixture)
}

fn oauth_home(test: &str, url: &str) -> Home {
    let home = Home::new(test).set("NCP_TEST_BROWSER", "follow");
    home.configure(&json!({"web": {"type": "http", "url": url}}));
    home
}

const LOGIN_TO_FILE: [&str; 5] = ["auth", "login", "web", "--credential-store", "file"];

#[tokio::test(flavor = "multi_thread")]
async fn login_with_browser_registers_a_client_and_keeps_tokens_and_client() {
    let (auth, fixture) = oauth_fixture(AuthSettings::default()).await;
    let home = oauth_home("login", &fixture.url);
    let run = home.run(&LOGIN_TO_FILE);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let out = run.stdout();
    assert_eq!(out["server"], "web");
    assert_eq!(out["issuer"], auth.issuer());
    assert_eq!(out["place"], "file");
    let record = auth.record();
    assert_eq!(
        (record.register_calls, record.code_grants),
        (1, 1),
        "{record:?}"
    );
    assert_eq!(
        record.authorize_resource.as_deref(),
        Some(fixture.url.as_str())
    );
    assert_eq!(record.token_resource.as_deref(), Some(fixture.url.as_str()));
    let login = run
        .stderr()
        .into_iter()
        .find_map(|line| line.get("login").cloned())
        .unwrap();
    assert!(
        login["url"]
            .as_str()
            .unwrap()
            .contains("code_challenge_method=S256")
    );
    let file = std::fs::read_to_string(home.root.join("state/ncp/credentials.json")).unwrap();
    assert!(
        file.contains("oauth/web/tokens") && file.contains("oauth/web/client"),
        "{file}"
    );
    let again = home.run(&LOGIN_TO_FILE);
    assert_eq!(again.code(), 0, "{}", again.describe());
    assert_eq!(
        auth.record().register_calls,
        1,
        "the stored client is used again"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn login_uses_the_configured_client_id_and_registers_nothing() {
    let (auth, fixture) = oauth_fixture(AuthSettings::default()).await;
    auth.add_client("pre-1", None);
    let home = oauth_home("login-pre", &fixture.url);
    home.configure(&json!({"web": {"type": "http", "url": fixture.url,
        "oauth": {"clientId": "pre-1"}}}));
    let run = home.run(&LOGIN_TO_FILE);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let record = auth.record();
    assert_eq!(record.register_calls, 0);
    assert_eq!(record.token_client_id.as_deref(), Some("pre-1"));
}

/// A server with no Dynamic Client Registration and a client with a
/// secret that the user registered there.
async fn pre_registered(
    test: &str,
    oauth: impl FnOnce(&str) -> Value,
) -> (
    ncp_testkit::auth_server::AuthServer,
    ncp_testkit::HttpFixture,
    Home,
) {
    let (auth, fixture) = oauth_fixture(AuthSettings {
        registration: Registration::Closed,
        expires_in: 5,
        service_documentation: Some("https://as.test/register-a-client".into()),
        ..AuthSettings::default()
    })
    .await;
    auth.add_client("pre-2", Some("s3cr3t-value"));
    let home = oauth_home(test, &fixture.url).set("PRE_SECRET", "s3cr3t-value");
    home.configure(&json!({"web": {"type": "http", "url": fixture.url,
        "oauth": oauth(&auth.issuer())}}));
    (auth, fixture, home)
}

#[tokio::test(flavor = "multi_thread")]
async fn login_and_refresh_send_the_configured_client_secret_to_its_issuer() {
    let (auth, fixture) = oauth_fixture(AuthSettings {
        registration: Registration::Closed,
        expires_in: 5,
        ..AuthSettings::default()
    })
    .await;
    auth.add_client("pre-2", Some("s3cr3t-value"));
    let home = oauth_home("login-secret", &fixture.url).set("PRE_SECRET", "s3cr3t-value");
    home.configure(&json!({"web": {"type": "http", "url": fixture.url,
        "oauth": {"clientId": "pre-2", "clientSecret": "${PRE_SECRET}", "issuer": auth.issuer()}}}));
    let run = home.run(&["-vv", "auth", "login", "web", "--credential-store", "file"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert!(!run.describe().contains("s3cr3t-value"));
    let record = auth.record();
    assert_eq!(record.register_calls, 0);
    assert_eq!(record.token_client_id.as_deref(), Some("pre-2"));
    assert!(record.token_basic, "HTTP Basic is the default method");
    // rmcp refreshes a token that ends in less than 30 s.
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert!(auth.record().refresh_grants >= 1, "{:?}", auth.record());
    assert!(auth.record().token_basic);
}

#[tokio::test(flavor = "multi_thread")]
async fn login_stops_before_the_browser_when_the_server_names_another_issuer() {
    let (auth, _fixture, home) = pre_registered("login-other-issuer", |_| {
        json!({"clientId": "pre-2", "clientSecret": "${PRE_SECRET}",
            "issuer": "https://as.example"})
    })
    .await;
    let run = home.run(&LOGIN_TO_FILE);
    assert_eq!(run.code(), 4, "{}", run.describe());
    let error = run.error();
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("on purpose"), "{message}");
    assert!(message.contains(&auth.issuer()) && message.contains("https://as.example"));
    assert!(
        message.contains("did not send the client secret"),
        "{message}"
    );
    let hint = error["hint"].as_str().unwrap();
    assert!(hint.contains("https://as.test/register-a-client"), "{hint}");
    assert!(hint.contains("tell the owner of the server"), "{hint}");
    let record = auth.record();
    assert_eq!((record.authorize_calls, record.code_grants), (0, 0));
}

#[tokio::test(flavor = "multi_thread")]
async fn login_with_no_registration_and_no_client_names_the_registration_page() {
    let (_auth, _fixture, home) = pre_registered("login-no-dcr", |_| json!({})).await;
    let run = home.run(&LOGIN_TO_FILE);
    assert_eq!(run.code(), 4, "{}", run.describe());
    let hint = run.error()["hint"].as_str().unwrap().to_string();
    assert!(hint.contains("https://as.test/register-a-client"), "{hint}");
    assert!(hint.contains("\"clientSecret\""), "{hint}");
}

#[tokio::test(flavor = "multi_thread")]
async fn login_with_a_client_secret_and_no_issuer_names_the_issuer_of_the_server() {
    let (auth, _fixture, home) = pre_registered(
        "login-no-issuer",
        |_| json!({"clientId": "pre-2", "clientSecret": "s3cr3t-value"}),
    )
    .await;
    let run = home.run(&LOGIN_TO_FILE);
    assert_eq!(run.code(), 2, "{}", run.describe());
    let hint = run.error()["hint"].as_str().unwrap().to_string();
    assert!(hint.contains(&auth.issuer()), "{hint}");
    assert!(!run.describe().contains("s3cr3t-value"));
    assert_eq!(auth.record().authorize_calls, 0);
    let other = home.run(&["tool", "list", "web"]);
    assert_eq!(other.code(), 2, "{}", other.describe());
}

#[tokio::test(flavor = "multi_thread")]
async fn refresh_stops_and_keeps_the_tokens_when_the_issuer_changes() {
    let (auth, _fixture, home) = pre_registered(
        "refresh-other-issuer",
        |issuer| json!({"clientId": "pre-2", "clientSecret": "${PRE_SECRET}", "issuer": issuer}),
    )
    .await;
    assert_eq!(home.run(&LOGIN_TO_FILE).code(), 0);
    auth.settings(|settings| settings.issuer_path = "/b".into());
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 4, "{}", run.describe());
    assert!(
        run.error()["message"]
            .as_str()
            .unwrap()
            .contains("did not send the client secret")
    );
    assert_eq!(auth.record().refresh_grants, 0);
    let file = std::fs::read_to_string(home.root.join("state/ncp/credentials.json")).unwrap();
    assert!(file.contains("oauth/web/tokens"), "the tokens stay");
}

#[tokio::test(flavor = "multi_thread")]
async fn login_registers_again_when_the_issuer_changes() {
    let (auth, fixture) = oauth_fixture(AuthSettings::default()).await;
    let home = oauth_home("login-issuer", &fixture.url);
    assert_eq!(home.run(&LOGIN_TO_FILE).code(), 0);
    auth.settings(|settings| settings.issuer_path = "/b".into());
    let run = home.run(&LOGIN_TO_FILE);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(auth.record().register_calls, 2);
    assert_eq!(run.stdout()["issuer"], auth.issuer());
}

#[tokio::test(flavor = "multi_thread")]
async fn login_rejects_a_redirect_with_a_wrong_issuer() {
    let (auth, fixture) = oauth_fixture(AuthSettings {
        callback_iss: ncp_testkit::auth_server::CallbackIss::Wrong,
        ..AuthSettings::default()
    })
    .await;
    let home = oauth_home("login-iss", &fixture.url);
    let run = home.run(&LOGIN_TO_FILE);
    assert_eq!(run.code(), 4, "{}", run.describe());
    assert_eq!(auth.record().code_grants, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn login_reports_a_refusal_of_the_authorization_server() {
    let (_auth, fixture) = oauth_fixture(AuthSettings {
        decision: ncp_testkit::auth_server::Decision::Deny,
        ..AuthSettings::default()
    })
    .await;
    let home = oauth_home("login-deny", &fixture.url);
    let run = home.run(&LOGIN_TO_FILE);
    assert_eq!(run.code(), 4, "{}", run.describe());
    assert!(
        run.error()["message"]
            .as_str()
            .unwrap()
            .contains("access_denied"),
        "{}",
        run.describe()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn login_with_no_browser_reads_the_pasted_redirect_url() {
    use std::io::{BufRead as _, Write as _};
    let (auth, fixture) = oauth_fixture(AuthSettings::default()).await;
    let home = Home::new("login-paste");
    home.configure(&json!({"web": {"type": "http", "url": fixture.url}}));
    let mut child = Command::new(NCP)
        .args([
            "auth",
            "login",
            "web",
            "--no-browser",
            "--credential-store",
            "file",
        ])
        .env_clear()
        .envs(home.env.iter().cloned())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stderr = std::io::BufReader::new(child.stderr.take().unwrap());
    let url = loop {
        let mut line = String::new();
        assert_ne!(stderr.read_line(&mut line).unwrap(), 0, "no login URL");
        let value: Value = serde_json::from_str(&line).unwrap();
        if let Some(url) = value["login"]["url"].as_str() {
            break url.to_string();
        }
    };
    let browser = ncp::client::http::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let answer = browser.get(&url).send().await.unwrap();
    let location = answer.headers()["location"].to_str().unwrap().to_string();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{location}").unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(auth.record().code_grants, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn status_shows_the_login_with_no_token_and_logout_removes_the_tokens() {
    let (auth, fixture) = oauth_fixture(AuthSettings::default()).await;
    let home = oauth_home("logout", &fixture.url);
    assert_eq!(home.run(&LOGIN_TO_FILE).code(), 0);
    let run = home.run(&["auth", "status"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let row = &run.stdout()["servers"][0];
    assert_eq!(row["name"], "web");
    assert_eq!(row["loggedIn"], true);
    assert_eq!(row["issuer"], auth.issuer());
    assert!(row["expiresAt"].as_str().unwrap().ends_with('Z'), "{row}");
    assert!(
        !run.0.stdout.windows(3).any(|w| w == b"at-"),
        "a token shows"
    );
    let run = home.run(&["auth", "logout", "web"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(run.stdout(), json!({"server": "web", "places": ["file"]}));
    let row = &home.run(&["auth", "status", "web"]).stdout()["servers"][0];
    assert_eq!(row["loggedIn"], false);
    let file = std::fs::read_to_string(home.root.join("state/ncp/credentials.json")).unwrap();
    assert!(
        file.contains("oauth/web/client"),
        "logout keeps the client: {file}"
    );
}

#[test]
fn trace_of_a_failed_request_hides_values_in_the_url() {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let home = Home::new("trace-error").set("PATH_KEY", "s3cret-path");
    home.configure(&json!({"web": {"type": "http",
        "url": format!("http://127.0.0.1:{port}/mcp/${{PATH_KEY}}?api_key=${{PATH_KEY}}")}}));
    let run = home.run(&["-vv", "tool", "list", "web"]);
    assert_eq!(run.code(), 3, "{}", run.describe());
    let stderr = String::from_utf8_lossy(&run.0.stderr).to_string();
    assert!(
        notes(&run, "trace")
            .iter()
            .any(|trace| trace.get("http").is_some()),
        "{stderr}"
    );
    assert!(!stderr.contains("s3cret-path"), "{stderr}");
}

#[test]
fn stdio_server_works_with_a_bad_ssl_cert_file() {
    let home = Home::new("stdio-cert").set("SSL_CERT_FILE", "/nonexistent/ca.pem");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = home.run(&["tool", "list", "fx"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
}

#[tokio::test(flavor = "multi_thread")]
async fn tool_list_sends_the_stored_token_after_a_login() {
    let (_auth, fixture) = oauth_fixture(AuthSettings::default()).await;
    let home = oauth_home("token-use", &fixture.url);
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 4, "{}", run.describe());
    // The test has no native store, so a login can go only to the file.
    assert_eq!(
        run.error()["hint"],
        "ncp auth login web --credential-store file"
    );
    assert_eq!(home.run(&LOGIN_TO_FILE).code(), 0);
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert!(run.stdout()["tools"].is_array());
}

#[tokio::test(flavor = "multi_thread")]
async fn expired_token_is_refreshed_with_no_user_action_and_kept() {
    let (auth, fixture) = oauth_fixture(AuthSettings {
        expires_in: 5,
        ..AuthSettings::default()
    })
    .await;
    let home = oauth_home("refresh", &fixture.url);
    assert_eq!(home.run(&LOGIN_TO_FILE).code(), 0);
    let before = std::fs::read_to_string(home.root.join("state/ncp/credentials.json")).unwrap();
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert!(auth.record().refresh_grants >= 1, "{:?}", auth.record());
    let after = std::fs::read_to_string(home.root.join("state/ncp/credentials.json")).unwrap();
    assert_ne!(before, after, "the new tokens go back to the file");
}

/// Changes the stored tokens item of `web` in the credential file.
fn change_stored_tokens(home: &Home, change: impl FnOnce(&mut Value)) {
    fn find(value: &mut Value) -> Option<&mut Value> {
        let object = value.as_object_mut()?;
        if object.contains_key("oauth/web/tokens") {
            return object.get_mut("oauth/web/tokens");
        }
        object.values_mut().find_map(find)
    }
    let path = home.root.join("state/ncp/credentials.json");
    let mut file: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let item = find(&mut file).unwrap();
    let mut tokens: Value = serde_json::from_str(item.as_str().unwrap()).unwrap();
    change(&mut tokens);
    *item = json!(tokens.to_string());
    std::fs::write(&path, file.to_string()).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn stored_token_for_another_server_url_is_not_sent() {
    let (_auth, fixture) = oauth_fixture(AuthSettings::default()).await;
    let home = oauth_home("binding-url", &fixture.url);
    assert_eq!(home.run(&LOGIN_TO_FILE).code(), 0);
    change_stored_tokens(&home, |tokens| {
        tokens["ncp_binding"]["resource"] = json!("https://other.example/mcp");
    });
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 4, "{}", run.describe());
    let error = run.error();
    let message = error["message"].as_str().unwrap();
    assert!(
        message.contains("did not send the stored token"),
        "{message}"
    );
    // A stored URL can hold an old secret, so it does not show.
    assert!(!message.contains("other.example"), "{message}");
    assert_eq!(error["hint"], "ncp auth login web");
}

#[tokio::test(flavor = "multi_thread")]
async fn stored_token_with_no_binding_is_not_sent_until_a_new_login() {
    let (_auth, fixture) = oauth_fixture(AuthSettings::default()).await;
    let home = oauth_home("binding-missing", &fixture.url);
    assert_eq!(home.run(&LOGIN_TO_FILE).code(), 0);
    change_stored_tokens(&home, |tokens| {
        tokens.as_object_mut().unwrap().remove("ncp_binding");
    });
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 4, "{}", run.describe());
    assert!(
        run.error()["message"]
            .as_str()
            .unwrap()
            .contains("on purpose")
    );
    assert_eq!(home.run(&LOGIN_TO_FILE).code(), 0);
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
}

#[tokio::test(flavor = "multi_thread")]
async fn rejected_refresh_asks_for_a_new_login() {
    let (_auth, fixture) = oauth_fixture(AuthSettings {
        expires_in: 5,
        reject_refresh: true,
        ..AuthSettings::default()
    })
    .await;
    let home = oauth_home("refresh-rejected", &fixture.url);
    assert_eq!(home.run(&LOGIN_TO_FILE).code(), 0);
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 4, "{}", run.describe());
    assert_eq!(run.error()["hint"], "ncp auth login web");
}

#[tokio::test(flavor = "multi_thread")]
async fn login_after_insufficient_scope_asks_for_the_old_and_the_new_scopes() {
    step_up_keeps_the_scopes("step-up", false).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_scopes_of_a_403_stay_with_tokens_in_the_native_store() {
    step_up_keeps_the_scopes("step-up-native", true).await;
}

/// A call that gets a 403 for a new scope, then a login, then the call
/// again, with the tokens in the file or in the native store.
async fn step_up_keeps_the_scopes(name: &str, native: bool) {
    let auth = ncp_testkit::auth_server::serve_auth(AuthSettings {
        scopes_supported: vec!["a".into()],
        ..AuthSettings::default()
    })
    .await
    .unwrap();
    let step_up = ncp_testkit::StepUp {
        tool: "echo".into(),
        scope: "b".into(),
    };
    let fixture = ncp_testkit::serve_http_with_oauth(Mode::Modern, &auth, Some(step_up))
        .await
        .unwrap();
    let home = oauth_home(name, &fixture.url);
    let (home, login): (Home, &[&str]) = if native {
        (home.with_fake_keychain(), &["auth", "login", "web"])
    } else {
        (home, &LOGIN_TO_FILE)
    };
    assert_eq!(home.run(login).code(), 0);
    let call = ["tool", "call", "web", "echo", "text=hi"];
    let run = home.run(&call);
    assert_eq!(run.code(), 4, "{}", run.describe());
    let error = run.error();
    assert!(
        error["message"].as_str().unwrap().contains("scopes b"),
        "{error}"
    );
    assert_eq!(error["hint"], "ncp auth login web");
    let run = home.run(login);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let scope = auth.record().authorize_scope.unwrap();
    let scopes: Vec<&str> = scope.split_whitespace().collect();
    assert!(scopes.contains(&"a") && scopes.contains(&"b"), "{scope}");
    let run = home.run(&call);
    assert_eq!(run.code(), 0, "{}", run.describe());
}

#[tokio::test(flavor = "multi_thread")]
async fn trace_of_a_login_and_a_call_shows_no_token_code_or_verifier() {
    let (_auth, fixture) = oauth_fixture(AuthSettings::default()).await;
    let home = oauth_home("login-trace", &fixture.url);
    let login = home.run(&["-vv", "auth", "login", "web", "--credential-store", "file"]);
    assert_eq!(login.code(), 0, "{}", login.describe());
    let call = home.run(&["-vv", "tool", "list", "web"]);
    assert_eq!(call.code(), 0, "{}", call.describe());
    for run in [&login, &call] {
        let stderr = String::from_utf8_lossy(&run.0.stderr).to_string();
        assert!(
            stderr.contains("/token"),
            "the token request shows: {stderr}"
        );
        for secret in [
            "\"at-",
            "\"rt-",
            "code=code-",
            "code_verifier=",
            "Bearer at-",
        ] {
            let shown = stderr
                .lines()
                .filter(|line| line.contains(secret) && !line.contains("code_verifier=****"))
                .count();
            assert_eq!(shown, 0, "{secret} shows: {stderr}");
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn oauth_discovery_goes_through_the_proxy() {
    let (_auth, fixture) = oauth_fixture(AuthSettings::default()).await;
    let proxy = ncp_testkit::serve_fake_proxy().await.unwrap();
    let home = oauth_home("login-proxy", &fixture.url).set("HTTP_PROXY", &proxy.url);
    let run = home.run(&LOGIN_TO_FILE);
    assert_ne!(run.code(), 0, "{}", run.describe());
    let lines = proxy.request_lines.lock().unwrap().clone();
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with(&format!("GET {}", fixture.url))),
        "{lines:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn request_that_needs_a_login_after_an_open_handshake_uses_the_stored_login() {
    let auth = ncp_testkit::auth_server::serve_auth(AuthSettings::default())
        .await
        .unwrap();
    let fixture = ncp_testkit::serve_http_with_oauth_after_handshake(Mode::Legacy, &auth)
        .await
        .unwrap();
    // With a native store, the login can go there: the hint names no place.
    let home = oauth_home("open-handshake", &fixture.url).with_fake_keychain();
    let call = ["tool", "call", "web", "echo", "text=hi"];
    let run = home.run(&call);
    assert_eq!(run.code(), 4, "{}", run.describe());
    assert_eq!(run.error()["hint"], "ncp auth login web");
    assert_eq!(home.run(&LOGIN_TO_FILE).code(), 0);
    let run = home.run(&call);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(text_of(&run.stdout()), "hi");
}

#[tokio::test(flavor = "multi_thread")]
async fn login_ignores_idle_connections_and_redirects_of_another_login() {
    use std::io::{BufRead as _, Write as _};
    let (auth, fixture) = oauth_fixture(AuthSettings::default()).await;
    let home = Home::new("login-forged");
    home.configure(&json!({"web": {"type": "http", "url": fixture.url}}));
    let mut child = Command::new(NCP)
        .args(LOGIN_TO_FILE)
        .env_clear()
        .envs(home.env.iter().cloned())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stderr = std::io::BufReader::new(child.stderr.take().unwrap());
    let url = loop {
        let mut line = String::new();
        assert_ne!(stderr.read_line(&mut line).unwrap(), 0, "no login URL");
        let value: Value = serde_json::from_str(&line).unwrap();
        if let Some(url) = value["login"]["url"].as_str() {
            break url.to_string();
        }
    };
    let browser = ncp::client::http::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let answer = browser.get(&url).send().await.unwrap();
    let location = answer.headers()["location"].to_str().unwrap().to_string();
    let callback = url::Url::parse(&location).unwrap();
    let origin = format!("127.0.0.1:{}", callback.port().unwrap());
    // A connection that sends nothing, and a redirect of another login.
    let _idle = std::net::TcpStream::connect(&origin).unwrap();
    let mut forged = std::net::TcpStream::connect(&origin).unwrap();
    write!(
        forged,
        "GET /callback?error=access_denied&state=other HTTP/1.1\r\n\r\n"
    )
    .unwrap();
    let mut answer = String::new();
    std::io::BufReader::new(forged)
        .read_line(&mut answer)
        .unwrap();
    assert!(answer.contains("400"), "{answer}");
    let real = browser.get(&location).send().await.unwrap();
    assert_eq!(real.status(), 200);
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(auth.record().code_grants, 1);
}

fn keychain_calls(home: &Home) -> String {
    std::fs::read_to_string(home.root.join("keychain/calls.log")).unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
async fn server_with_no_login_never_opens_the_native_store() {
    let fixture = serve_http(Mode::Modern).await.unwrap();
    let home = Home::new("no-store-open").with_fake_keychain();
    home.configure(&json!({"web": {"type": "http", "url": fixture.url}}));
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(keychain_calls(&home), "");
}

#[tokio::test(flavor = "multi_thread")]
async fn login_to_the_native_store_and_refresh_write_back_there() {
    let (auth, fixture) = oauth_fixture(AuthSettings {
        expires_in: 5,
        ..AuthSettings::default()
    })
    .await;
    let home = oauth_home("native-login", &fixture.url).with_fake_keychain();
    let run = home.run(&["auth", "login", "web"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(run.stdout()["place"], "native");
    let items = || std::fs::read_to_string(home.root.join("keychain/items.json")).unwrap();
    let before = items();
    assert!(before.contains("oauth/web/tokens") && before.contains("oauth/web/client"));
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert!(auth.record().refresh_grants >= 1);
    assert_ne!(items(), before, "the refresh writes the native item");
    assert!(!home.root.join("state/ncp/credentials.json").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn native_store_that_cannot_answer_gives_its_reason_and_not_a_login_hint() {
    let (_auth, fixture) = oauth_fixture(AuthSettings::default()).await;
    let home = oauth_home("native-locked", &fixture.url).with_fake_keychain();
    assert_eq!(home.run(&["auth", "login", "web"]).code(), 0);
    std::fs::write(home.root.join("keychain/fail"), "36").unwrap();
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 4, "{}", run.describe());
    let error = run.error();
    assert_ne!(error["hint"], "ncp auth login web", "{error}");
    assert!(
        !error["message"].as_str().unwrap().contains("needs a login"),
        "{error}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn login_to_the_file_removes_old_native_tokens_and_calls_use_the_file() {
    let (_auth, fixture) = oauth_fixture(AuthSettings::default()).await;
    let home = oauth_home("login-move", &fixture.url).with_fake_keychain();
    assert_eq!(home.run(&["auth", "login", "web"]).code(), 0);
    let run = home.run(&LOGIN_TO_FILE);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let items = std::fs::read_to_string(home.root.join("keychain/items.json")).unwrap();
    assert!(!items.contains("oauth/web/tokens"), "{items}");
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
}

#[tokio::test(flavor = "multi_thread")]
async fn tokens_in_the_file_work_when_the_native_store_cannot_answer() {
    let (_auth, fixture) = oauth_fixture(AuthSettings::default()).await;
    let home = oauth_home("native-down-file", &fixture.url).with_fake_keychain();
    assert_eq!(home.run(&LOGIN_TO_FILE).code(), 0);
    std::fs::write(home.root.join("keychain/fail"), "36").unwrap();
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let run = home.run(&["auth", "status", "web"]);
    assert_eq!(
        run.stdout()["servers"][0]["loggedIn"],
        true,
        "{}",
        run.describe()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn login_that_gets_no_redirect_stops_at_the_time_limit() {
    let (_auth, fixture) = oauth_fixture(AuthSettings::default()).await;
    let home = Home::new("login-timeout");
    home.configure(&json!({"web": {"type": "http", "url": fixture.url}}));
    let started = std::time::Instant::now();
    let run = home.run(&[
        "--timeout",
        "1",
        "auth",
        "login",
        "web",
        "--credential-store",
        "file",
    ]);
    assert_eq!(run.code(), 3, "{}", run.describe());
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
    assert_eq!(
        run.error()["hint"],
        "give a longer limit with --timeout <SECONDS>"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn login_to_a_server_that_names_no_authorization_server_is_an_error() {
    let fixture = serve_http(Mode::Modern).await.unwrap();
    let home = oauth_home("login-no-as", &fixture.url);
    let run = home.run(&LOGIN_TO_FILE);
    assert_eq!(run.code(), 3, "{}", run.describe());
    let message = run.error()["message"].as_str().unwrap().to_string();
    assert!(
        message.contains("names no authorization server"),
        "{message}"
    );
    assert!(!home.root.join("state/ncp/credentials.json").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn parallel_calls_refresh_the_tokens_one_at_a_time() {
    let (auth, fixture) = oauth_fixture(AuthSettings {
        expires_in: 5,
        ..AuthSettings::default()
    })
    .await;
    let home = oauth_home("parallel-refresh", &fixture.url);
    assert_eq!(home.run(&LOGIN_TO_FILE).code(), 0);
    let runs: Vec<Run> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|_| scope.spawn(|| home.run(&["tool", "list", "web"])))
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect()
    });
    for run in &runs {
        assert_eq!(run.code(), 0, "{}", run.describe());
    }
    assert!(auth.record().refresh_grants >= 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn refresh_that_cannot_write_the_credential_file_is_a_local_error() {
    let (_auth, fixture) = oauth_fixture(AuthSettings {
        expires_in: 5,
        ..AuthSettings::default()
    })
    .await;
    let home = oauth_home("refresh-file-fails", &fixture.url);
    assert_eq!(home.run(&LOGIN_TO_FILE).code(), 0);
    let dir = home.root.join("state/ncp");
    let mode =
        |bits| std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(bits)).unwrap();
    mode(0o500);
    let run = home.run(&["tool", "list", "web"]);
    mode(0o700);
    assert_eq!(run.code(), 3, "{}", run.describe());
    let error = run.error();
    assert!(error.get("hint").is_none(), "{error}");
    assert!(
        !error["message"].as_str().unwrap().contains("local:"),
        "{error}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn trace_of_a_login_hides_a_value_in_the_server_url() {
    let (_auth, fixture) = oauth_fixture(AuthSettings::default()).await;
    let home = Home::new("login-url-value")
        .set("NCP_TEST_BROWSER", "follow")
        .set("URL_KEY", "s3cret-key");
    home.configure(&json!({"web": {"type": "http",
        "url": format!("{}?k=${{URL_KEY}}", fixture.url)}}));
    let run = home.run(&["-vv", "auth", "login", "web", "--credential-store", "file"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let stderr = String::from_utf8_lossy(&run.0.stderr).to_string();
    assert!(!stderr.contains("s3cret-key"), "{stderr}");
}

#[test]
fn output_format_option_chooses_text_or_json() {
    let home = Home::new("output-format");
    home.configure(&json!({"fx": stdio("modern")}));
    let text = home.run(&["--output-format", "text", "server", "list"]);
    assert_eq!(text.code(), 0, "{}", text.describe());
    let stdout = String::from_utf8_lossy(&text.0.stdout).to_string();
    assert!(stdout.starts_with("fx"), "{stdout}");
    let json = home.run(&["server", "list", "--output-format", "json"]);
    assert_eq!(json.stdout()["servers"][0]["name"], "fx");
}

#[test]
fn quiet_hides_warnings_and_hints_but_not_errors() {
    let home = Home::new("quiet");
    home.configure(&json!({"fx": {"command": FIXTURE, "args": ["modern"], "disabled": true}}));
    let loud = home.run(&["server", "list"]);
    assert!(!loud.stderr().is_empty(), "{}", loud.describe());
    let quiet = home.run(&["-q", "server", "list"]);
    assert_eq!(quiet.code(), 0);
    assert!(quiet.stderr().is_empty(), "{}", quiet.describe());
    let failed = home.run(&["-q", "tool", "list", "nothing"]);
    assert_eq!(failed.code(), 2);
    assert_eq!(failed.error()["code"], "usage");
    assert_eq!(home.run(&["-q", "-v", "server", "list"]).code(), 2);
}

#[test]
fn config_file_option_reads_that_file_and_a_missing_one_is_a_usage_error() {
    let home = Home::new("config-file");
    let path = home.root.join("other.json");
    std::fs::write(
        &path,
        json!({"mcpServers": {"other": stdio("modern")}}).to_string(),
    )
    .unwrap();
    let run = home.run(&["--config-file", path.to_str().unwrap(), "server", "list"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(run.stdout()["servers"][0]["name"], "other");
    let missing = home.root.join("missing.json");
    let run = home.run(&["--config-file", missing.to_str().unwrap(), "server", "list"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert!(
        run.error()["message"]
            .as_str()
            .unwrap()
            .contains("does not exist")
    );
}

#[test]
fn tool_call_sends_a_value_with_an_at_sign_as_text_and_reads_no_file() {
    let home = Home::new("at-sign-args");
    home.configure(&json!({"fx": stdio("modern")}));
    let file = home.root.join("text.txt");
    std::fs::write(&file, "from a file").unwrap();
    let arg = format!("text=@{}", file.display());
    let run = home.run(&["tool", "call", "fx", "echo", &arg]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(text_of(&run.stdout()), format!("@{}", file.display()));
    let run = home.run(&["tool", "call", "fx", "echo", "text=@acme/pkg"]);
    assert_eq!(text_of(&run.stdout()), "@acme/pkg");
}

#[test]
fn unknown_names_get_the_nearest_known_name() {
    let home = Home::new("near-names");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = home.run(&["tool", "list", "fz"]);
    assert_eq!(run.code(), 2);
    let message = run.error()["message"].as_str().unwrap().to_string();
    assert!(message.ends_with("did you mean \"fx\"?"), "{message}");
    let run = home.run(&["tool", "show", "fx", "ech"]);
    assert_eq!(run.code(), 2);
    let message = run.error()["message"].as_str().unwrap().to_string();
    assert!(message.ends_with("did you mean \"echo\"?"), "{message}");
}

#[test]
fn resource_list_gives_resources_and_templates() {
    let home = Home::new("resource-list");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = home.run(&["resource", "list", "fx"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let out = run.stdout();
    assert_eq!(out["server"], "fx");
    assert_eq!(out["resources"][0]["uri"], "fixture://greeting");
    assert_eq!(
        out["resourceTemplates"][0]["uriTemplate"],
        "fixture://echo/{text}"
    );
}

#[test]
fn resource_read_gives_json_in_a_pipe_and_raw_contents_on_request() {
    let home = Home::new("resource-read");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = home.run(&["resource", "read", "fx", "fixture://echo/x"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(run.stdout()["contents"][0]["text"], "x");
    let raw = ["--output-format", "text", "resource", "read", "fx"];
    let run = home.run(&[&raw[..], &["fixture://greeting"]].concat());
    assert_eq!(run.0.stdout, b"hello\n");
    let run = home.run(&[&raw[..], &["fixture://logo"]].concat());
    assert_eq!(run.0.stdout, b"\x89PNG\r\n\x1a\n");
    let run = home.run(&["resource", "read", "fx", "fixture://none"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert_eq!(run.error()["hint"], "ncp resource list fx");
}

#[tokio::test(flavor = "multi_thread")]
async fn refresh_of_native_tokens_works_when_the_state_folder_is_read_only() {
    let (auth, fixture) = oauth_fixture(AuthSettings {
        expires_in: 5,
        ..AuthSettings::default()
    })
    .await;
    let home = oauth_home("native-read-only-state", &fixture.url).with_fake_keychain();
    assert_eq!(home.run(&["auth", "login", "web"]).code(), 0);
    let dir = home.root.join("state/ncp");
    let _ = std::fs::remove_file(dir.join("oauth-web.lock"));
    let mode =
        |bits| std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(bits)).unwrap();
    mode(0o500);
    let run = home.run(&["tool", "list", "web"]);
    mode(0o700);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert!(auth.record().refresh_grants >= 1);
}

#[test]
fn prompt_list_and_get_use_the_prompt_arguments() {
    let home = Home::new("prompts");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = home.run(&["prompt", "list", "fx"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(run.stdout()["prompts"][0]["name"], "greet");
    let run = home.run(&["prompt", "get", "fx", "greet", "name=Ada"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(
        run.stdout()["messages"][0]["content"]["text"],
        "Hello, Ada!"
    );
    let run = home.run(&["prompt", "get", "fx", "greet"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert_eq!(run.error()["hint"], "ncp prompt list fx");
    let run = home.run(&["prompt", "get", "fx", "greet", "name:=1"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
}

#[test]
fn server_that_stops_at_its_start_gives_its_exit_status_and_its_log() {
    let home = Home::new("stops-at-start");
    home.configure(&json!({"fx": {"command": "/bin/sh", "args": ["-c", "echo boom >&2; exit 7"]}}));
    let run = home.run(&["tool", "list", "fx"]);
    assert_eq!(run.code(), 3, "{}", run.describe());
    let error = run.error();
    let message = error["message"].as_str().unwrap();
    assert!(
        message.contains("stopped before the handshake") && message.contains('7'),
        "{message}"
    );
    assert!(
        error["hint"].as_str().unwrap().contains("/bin/sh"),
        "{error}"
    );
    let logs = notes(&run, "serverLog");
    assert!(
        logs.iter().any(|log| log["message"] == "boom"),
        "{}",
        run.describe()
    );
}

/// A configuration whose server leaves a mark file when it starts.
fn marking_server(home: &Home) -> PathBuf {
    let mark = home.root.join("started");
    home.configure(&json!({
        "beta": {"command": "/bin/sh", "args": ["-c", format!("touch {}", mark.display())]},
        "alpha": {"type": "http", "url": "https://example.com/mcp"}
    }));
    mark
}

#[test]
fn completion_scripts_name_ncp_and_read_server_names_at_completion_time() {
    let home = Home::new("completion-scripts");
    for shell in ["bash", "zsh", "fish"] {
        let run = home.run(&["generate-shell-completion", shell]);
        assert_eq!(run.code(), 0, "{}", run.describe());
        let script = String::from_utf8_lossy(&run.0.stdout).to_string();
        assert!(script.contains("ncp __complete-servers"), "{shell}");
        // zsh runs the command in one function, and names the function at
        // each place.
        let places = match shell {
            "zsh" => script.matches(":_ncp_servers'").count(),
            _ => script.matches("ncp __complete-servers").count(),
        };
        assert_eq!(
            places, 11,
            "{shell}: one place for each command with a server"
        );
    }
    assert_eq!(home.run(&["generate-shell-completion", "elvish"]).code(), 2);
}

#[test]
fn server_names_complete_from_the_configuration_and_start_no_server() {
    let home = Home::new("completion-names");
    let mark = marking_server(&home);
    let run = home.run(&["__complete-servers"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(String::from_utf8_lossy(&run.0.stdout), "alpha\nbeta\n");
    assert_eq!(String::from_utf8_lossy(&run.0.stderr), "");
    assert!(!mark.exists(), "a server started");
}

#[test]
fn bash_completion_offers_the_configured_server_names() {
    let home = Home::new("completion-bash");
    let mark = marking_server(&home);
    let bin = home.root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::os::unix::fs::symlink(NCP, bin.join("ncp")).unwrap();
    let script = home.root.join("ncp.bash");
    let run = home.run(&["generate-shell-completion", "bash"]);
    std::fs::write(&script, &run.0.stdout).unwrap();
    let probe = format!(
        "source {}; COMP_WORDS=(ncp tool list ''); COMP_CWORD=3; _ncp ncp '' list; printf '%s\\n' \"${{COMPREPLY[@]}}\"",
        script.display()
    );
    let home = home.set("PATH", &format!("{}:/usr/bin:/bin", bin.display()));
    let output = Command::new("bash")
        .args(["-c", &probe])
        .env_clear()
        .envs(home.env.iter().cloned())
        .output()
        .unwrap();
    let words = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(words.lines().any(|word| word == "alpha"), "{words}");
    assert!(words.lines().any(|word| word == "beta"), "{words}");
    assert!(!mark.exists(), "a server started");
}

#[test]
fn configuration_that_does_not_parse_completes_no_names_and_says_nothing() {
    let home = Home::new("completion-broken");
    let path = home.config_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "{ not json").unwrap();
    let run = home.run(&["__complete-servers"]);
    assert_eq!(run.code(), 0);
    assert!(
        run.0.stdout.is_empty() && run.0.stderr.is_empty(),
        "{}",
        run.describe()
    );
}

#[test]
fn lists_read_all_pages_of_resources_and_prompts() {
    let home = Home::new("pages");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = home.run(&["resource", "list", "fx"]);
    assert_eq!(
        run.stdout()["resources"].as_array().unwrap().len(),
        4,
        "{}",
        run.describe()
    );
    let run = home.run(&["prompt", "list", "fx"]);
    assert_eq!(
        run.stdout()["prompts"].as_array().unwrap().len(),
        2,
        "{}",
        run.describe()
    );
}

#[test]
fn list_with_a_repeated_cursor_stops_with_exit_3() {
    let home = Home::new("repeat-cursor");
    home.configure(&json!({"fx": stdio("repeat-cursor")}));
    let run = home.run(&["prompt", "list", "fx"]);
    assert_eq!(run.code(), 3, "{}", run.describe());
    assert!(run.error()["message"].as_str().unwrap().contains("cursor"));
}

#[test]
fn server_with_no_resources_or_prompts_gives_empty_lists_and_a_hint() {
    let home = Home::new("tools-only");
    home.configure(&json!({"fx": stdio("tools-only")}));
    for (command, field) in [("resource", "resources"), ("prompt", "prompts")] {
        let run = home.run(&[command, "list", "fx"]);
        assert_eq!(run.code(), 0, "{}", run.describe());
        assert_eq!(run.stdout()[field], json!([]));
        assert!(!notes(&run, "hint").is_empty(), "{}", run.describe());
    }
}

#[test]
fn input_required_from_a_resource_or_a_prompt_gives_exit_5() {
    let home = Home::new("input-required-read");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = home.run(&["resource", "read", "fx", "fixture://ask"]);
    assert_eq!(run.code(), 5, "{}", run.describe());
    assert_eq!(run.stdout()["resultType"], "input_required");
    let run = home.run(&["prompt", "get", "fx", "ask"]);
    assert_eq!(run.code(), 5, "{}", run.describe());
}

#[test]
fn raw_read_of_several_contents_is_a_usage_error_with_empty_stdout() {
    let home = Home::new("pair-raw");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = home.run(&[
        "--output-format",
        "text",
        "resource",
        "read",
        "fx",
        "fixture://pair",
    ]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert!(run.stdout_is_empty());
    // --output-format text gives text on stderr too.
    let stderr = String::from_utf8_lossy(&run.0.stderr).to_string();
    assert!(
        stderr.contains("hint: use --output-format json"),
        "{stderr}"
    );
}

#[test]
fn progress_of_a_call_goes_to_stderr_unless_quiet() {
    let home = Home::new("progress");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = home.run(&["tool", "call", "fx", "progress", "text=x"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let steps = notes(&run, "progress");
    // The steps come fast: the rate limit keeps the first and the last. On
    // a busy host the steps can come slower, so the middle one can show.
    assert!((2..=3).contains(&steps.len()), "{}", run.describe());
    assert_eq!(steps[0]["progress"], 1.0);
    assert_eq!(
        steps[steps.len() - 1],
        json!({"server": "fx", "progress": 3.0, "total": 3.0, "message": "step 3"})
    );
    let quiet = home.run(&["-q", "tool", "call", "fx", "progress", "text=x"]);
    assert!(notes(&quiet, "progress").is_empty(), "{}", quiet.describe());
}

#[test]
fn fresh_tool_list_comes_from_the_cache_with_no_server_start() {
    let home = Home::new("list-cache");
    let started = home.root.join("server.pid");
    home.configure(
        &json!({"fx": {"command": FIXTURE, "args": ["cached-public"],
        "env": {"NCP_FIXTURE_PID_FILE": started.display().to_string()}}}),
    );
    let first = home.run(&["tool", "list", "fx"]);
    assert_eq!(first.code(), 0, "{}", first.describe());
    assert!(started.exists());
    let cache = home.root.join("cache/ncp/lists/fx.json");
    let mode = std::fs::metadata(&cache).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    std::fs::remove_file(&started).unwrap();
    let second = home.run(&["tool", "list", "fx"]);
    assert_eq!(second.stdout(), first.stdout());
    assert_eq!(
        notes(&second, "hint"),
        notes(&first, "hint"),
        "the same hints"
    );
    let show = home.run(&["tool", "show", "fx", "echo"]);
    assert_eq!(show.code(), 0, "{}", show.describe());
    assert!(!started.exists(), "a server started for a fresh list");
    let run = home.run(&["--no-cache", "tool", "list", "fx"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert!(started.exists(), "--no-cache must start the server");
}

#[test]
fn tool_list_with_no_time_to_live_is_not_cached() {
    let home = Home::new("no-list-cache");
    home.configure(&json!({"fx": stdio("modern")}));
    assert_eq!(home.run(&["tool", "list", "fx"]).code(), 0);
    assert!(!home.root.join("cache/ncp/lists/fx.json").exists());
}

#[test]
fn completion_never_gives_a_name_with_shell_characters() {
    let home = Home::new("complete-unsafe");
    home.configure(&json!({
        "$(touch${IFS}PWNED)": {"command": "x"},
        "ok": {"command": "x"},
        "bad_entry": {"url": "https://example.com/mcp"}
    }));
    let run = home.run(&["__complete-servers"]);
    assert_eq!(String::from_utf8_lossy(&run.0.stdout), "bad_entry\nok\n");
}

fn cached_server(home: &Home, env: &Value) {
    home.configure(&json!({"fx": {"command": FIXTURE, "args": ["cached-public"], "env": env}}));
}

#[test]
fn a_changed_entry_makes_the_cached_list_invalid() {
    let home = Home::new("cache-entry");
    cached_server(&home, &json!({"A": "1"}));
    assert_eq!(home.run(&["tool", "list", "fx"]).code(), 0);
    let from_cache = |run: &Run| {
        notes(run, "debug").iter().any(|note| {
            note["message"]
                .as_str()
                .unwrap_or("")
                .contains("list cache")
        })
    };
    assert!(from_cache(&home.run(&["-v", "tool", "list", "fx"])));
    cached_server(&home, &json!({"A": "2"}));
    assert!(!from_cache(&home.run(&["-v", "tool", "list", "fx"])));
}

#[test]
fn tool_call_lists_again_when_the_cached_list_does_not_have_the_tool() {
    let home = Home::new("cache-stale");
    cached_server(&home, &json!({}));
    assert_eq!(home.run(&["tool", "list", "fx"]).code(), 0);
    let path = home.root.join("cache/ncp/lists/fx.json");
    let mut cached: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    cached["tools"]
        .as_array_mut()
        .unwrap()
        .retain(|tool| tool["name"] != "echo");
    std::fs::write(&path, cached.to_string()).unwrap();
    let run = home.run(&["tool", "call", "fx", "echo", "text=hi"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(text_of(&run.stdout()), "hi");
    let run = home.run(&["tool", "show", "fx", "echo"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
}

#[test]
fn refresh_gets_the_list_again_and_keeps_it() {
    let home = Home::new("cache-refresh");
    let started = home.root.join("server.pid");
    home.configure(
        &json!({"fx": {"command": FIXTURE, "args": ["cached-public"],
        "env": {"NCP_FIXTURE_PID_FILE": started.display().to_string()}}}),
    );
    assert_eq!(home.run(&["tool", "list", "fx"]).code(), 0);
    let cache = home.root.join("cache/ncp/lists/fx.json");
    let before = std::fs::read_to_string(&cache).unwrap();
    std::fs::remove_file(&started).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    assert_eq!(home.run(&["--refresh", "tool", "list", "fx"]).code(), 0);
    assert!(started.exists(), "--refresh must start the server");
    assert_ne!(std::fs::read_to_string(&cache).unwrap(), before);
    // As in uv, --no-cache wins over --refresh: nothing is written.
    let before = std::fs::read_to_string(&cache).unwrap();
    assert_eq!(
        home.run(&["--refresh", "--no-cache", "tool", "list", "fx"])
            .code(),
        0
    );
    assert_eq!(std::fs::read_to_string(&cache).unwrap(), before);
}

#[test]
fn bash_completion_offers_the_cached_tool_names_and_no_unsafe_name() {
    let home = Home::new("completion-tools");
    cached_server(&home, &json!({}));
    assert_eq!(home.run(&["tool", "list", "fx"]).code(), 0);
    let path = home.root.join("cache/ncp/lists/fx.json");
    let mut cached: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let mut unsafe_tool = cached["tools"][0].clone();
    unsafe_tool["name"] = json!("$(touch PWNED)");
    cached["tools"].as_array_mut().unwrap().push(unsafe_tool);
    std::fs::write(&path, cached.to_string()).unwrap();
    let names = home.run(&["__complete-tools", "fx"]);
    let names = String::from_utf8_lossy(&names.0.stdout).to_string();
    assert!(names.lines().any(|name| name == "echo"), "{names}");
    assert!(!names.contains("PWNED"), "{names}");
    let bin = home.root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::os::unix::fs::symlink(NCP, bin.join("ncp")).unwrap();
    let script = home.root.join("ncp.bash");
    std::fs::write(
        &script,
        home.run(&["generate-shell-completion", "bash"]).0.stdout,
    )
    .unwrap();
    let home = home.set("PATH", &format!("{}:/usr/bin:/bin", bin.display()));
    let complete = |words: &str, cword: usize| {
        let probe = format!(
            "source {}; COMP_WORDS=({words}); COMP_CWORD={cword}; _ncp ncp \"${{COMP_WORDS[$COMP_CWORD]}}\" \"${{COMP_WORDS[$COMP_CWORD-1]}}\"; printf '%s\\n' \"${{COMPREPLY[@]}}\"",
            script.display()
        );
        let output = Command::new("bash")
            .args(["-c", &probe])
            .env_clear()
            .envs(home.env.iter().cloned())
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).to_string()
    };
    let has = |words: &str, word: &str| words.lines().any(|line| line == word);
    let words = complete("ncp tool call fx ''", 4);
    assert!(has(&words, "echo"), "{words}");
    let words = complete("ncp -v tool call fx ''", 5);
    assert!(has(&words, "echo"), "an option first: {words}");
    let words = complete("ncp tool call --output-format ''", 4);
    assert!(
        has(&words, "json") && !has(&words, "echo"),
        "an option value: {words}"
    );
    let words = complete("ncp -v tool call ''", 4);
    assert!(has(&words, "fx"), "the server after an option: {words}");
    assert!(!home.root.join("PWNED").exists());
}

#[test]
fn elicitation_of_a_modern_server_asks_on_the_tty_and_sends_the_answers() {
    let home = Home::new("elicit-modern").set("NCP_TEST_TTY", "1");
    home.configure(&json!({"fx": stdio("modern")}));
    // The fields come in name order: an empty age takes the default, then
    // a name, and yes to send.
    let run = home.run_with_stdin(&["tool", "call", "fx", "elicit", "text=x"], "\nAda\ny\n");
    assert_eq!(run.code(), 0, "{}", run.describe());
    let answer: Value = serde_json::from_str(text_of(&run.stdout())).unwrap();
    assert_eq!(
        answer,
        json!({"action": "accept", "content": {"name": "Ada", "age": 30}})
    );
    let stderr = String::from_utf8_lossy(&run.0.stderr).to_string();
    assert!(
        stderr.contains("The server fx asks: Who are you?"),
        "{stderr}"
    );
}

#[test]
fn elicitation_can_be_declined_and_with_no_tty_gives_exit_5() {
    let home = Home::new("elicit-decline").set("NCP_TEST_TTY", "1");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = home.run_with_stdin(&["tool", "call", "fx", "elicit", "text=x"], "\nAda\nn\n");
    assert_eq!(run.code(), 0, "{}", run.describe());
    let answer: Value = serde_json::from_str(text_of(&run.stdout())).unwrap();
    assert_eq!(answer, json!({"action": "decline"}));
    let home = Home::new("elicit-no-tty");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = home.run(&["tool", "call", "fx", "elicit", "text=x"]);
    assert_eq!(run.code(), 5, "{}", run.describe());
}

#[test]
fn elicitation_request_of_a_legacy_server_asks_on_the_tty() {
    let home = Home::new("elicit-legacy").set("NCP_TEST_TTY", "1");
    home.configure(&json!({"fx": stdio("legacy")}));
    let run = home.run_with_stdin(
        &["tool", "call", "fx", "elicit-legacy", "text=x"],
        "Ada\ny\n",
    );
    assert_eq!(run.code(), 0, "{}", run.describe());
    let answer: Value = serde_json::from_str(text_of(&run.stdout())).unwrap();
    assert_eq!(answer["action"], "accept");
    assert_eq!(answer["content"], json!({"name": "Ada"}));
}

#[test]
fn elicitation_lets_the_user_change_a_field_before_the_answer_goes_out() {
    let home = Home::new("elicit-change").set("NCP_TEST_TTY", "1");
    home.configure(&json!({"fx": stdio("modern")}));
    // age takes the default, name is Ada; then change field 2 to Grace.
    let run = home.run_with_stdin(
        &["tool", "call", "fx", "elicit", "text=x"],
        // Whole words count: "change" is not "cancel".
        "\nAda\nchange\n9\n2\nGrace\nsend\n",
    );
    assert_eq!(run.code(), 0, "{}", run.describe());
    let answer: Value = serde_json::from_str(text_of(&run.stdout())).unwrap();
    assert_eq!(
        answer,
        json!({"action": "accept", "content": {"name": "Grace", "age": 30}})
    );
}

#[test]
fn elicitation_form_can_be_cancelled() {
    let home = Home::new("elicit-cancel").set("NCP_TEST_TTY", "1");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = home.run_with_stdin(&["tool", "call", "fx", "elicit", "text=x"], "\nAda\nc\n");
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(text_of(&run.stdout()), r#"{"action":"cancel"}"#);
}

#[test]
fn legacy_question_with_no_tty_gets_invalid_params_and_exit_5() {
    let home = Home::new("elicit-legacy-no-tty");
    home.configure(&json!({"fx": stdio("legacy")}));
    let run = home.run(&["tool", "call", "fx", "elicit-legacy", "text=x"]);
    assert_eq!(run.code(), 5, "{}", run.describe());
    assert!(
        run.error()["message"]
            .as_str()
            .unwrap()
            .contains("cannot ask with no terminal")
    );
}

#[test]
fn wrong_answer_shows_server_text_with_escapes_also_with_json_output() {
    let home = Home::new("elicit-escapes").set("NCP_TEST_TTY", "1");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = home.run_with_stdin(
        &[
            "--output-format",
            "json",
            "tool",
            "call",
            "fx",
            "elicit-choice",
            "text=x",
        ],
        "blue\nr\ns\n",
    );
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert!(!run.0.stderr.contains(&0x1b), "{}", run.describe());
    assert!(!run.0.stderr.contains(&0x07), "{}", run.describe());
    let answer: Value = serde_json::from_str(text_of(&run.stdout())).unwrap();
    assert_eq!(answer["content"], json!({"color": "r"}));
}

#[test]
fn url_question_warns_about_a_punycode_host() {
    let home = Home::new("elicit-punycode").set("NCP_TEST_TTY", "1");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = elicit_url(&home, "https://xn--pple-43d.test/login", "n\n");
    let stderr = String::from_utf8_lossy(&run.0.stderr).to_string();
    assert_eq!(run.code(), 0, "{stderr}");
    assert!(stderr.contains("Punycode label"), "{stderr}");
}

fn elicit_url(home: &Home, url: &str, stdin: &str) -> Run {
    home.run_with_stdin(
        &["tool", "call", "fx", "elicit-url", &format!("text={url}")],
        stdin,
    )
}

#[test]
fn url_elicitation_opens_only_https_and_only_after_a_yes() {
    let home = Home::new("elicit-url").set("NCP_TEST_TTY", "1");
    home.configure(&json!({"fx": stdio("modern")}));
    let no = elicit_url(&home, "https://login.example.test/x", "n\n");
    assert_eq!(no.code(), 0, "{}", no.describe());
    assert_eq!(text_of(&no.stdout()), r#"{"action":"decline"}"#);
    let stderr = String::from_utf8_lossy(&no.0.stderr).to_string();
    assert!(stderr.contains("a page of login.example.test"), "{stderr}");
    // The test has no browser: a warning, and the answer is still yes.
    let yes = elicit_url(&home, "https://login.example.test/x", "y\n");
    assert_eq!(text_of(&yes.stdout()), r#"{"action":"accept"}"#);
    let file = elicit_url(&home, "file:///Applications/Calculator.app", "y\n");
    assert_eq!(text_of(&file.stdout()), r#"{"action":"decline"}"#);
    assert!(!notes(&file, "warning").is_empty(), "{}", file.describe());
}

#[test]
fn input_that_ncp_cannot_give_or_no_answer_ends_the_rounds() {
    let home = Home::new("elicit-ends").set("NCP_TEST_TTY", "1");
    home.configure(&json!({"fx": stdio("modern")}));
    let sampling = elicit_url(&home, "sampling", "");
    assert_eq!(sampling.code(), 5, "{}", sampling.describe());
    assert_eq!(sampling.stdout()["resultType"], "input_required");
    // The end of stdin cancels the question.
    let ended = home.run_with_stdin(&["tool", "call", "fx", "elicit", "text=x"], "");
    assert_eq!(ended.code(), 0, "{}", ended.describe());
    assert_eq!(text_of(&ended.stdout()), r#"{"action":"cancel"}"#);
}

#[test]
fn tool_completion_offers_no_names_of_a_changed_or_removed_entry() {
    let home = Home::new("completion-entry");
    cached_server(&home, &json!({}));
    assert_eq!(home.run(&["tool", "list", "fx"]).code(), 0);
    let names = |home: &Home| {
        String::from_utf8_lossy(&home.run(&["__complete-tools", "fx"]).0.stdout).to_string()
    };
    assert!(names(&home).contains("echo"));
    // The same name for another server.
    home.configure(&json!({"fx": stdio("modern")}));
    assert_eq!(names(&home), "");
    home.configure(&json!({}));
    assert_eq!(names(&home), "");
}

#[test]
fn text_diagnostics_of_a_stdio_server_show_debug_trace_and_server_start() {
    let home = Home::new("text-diagnostics");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = home.run(&[
        "--output-format",
        "text",
        "-vv",
        "--timings",
        "tool",
        "list",
        "fx",
    ]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let stderr = String::from_utf8_lossy(&run.0.stderr).to_string();
    assert!(
        stderr
            .lines()
            .any(|line| line.starts_with("debug: server fx: ")),
        "{stderr}"
    );
    assert!(
        stderr
            .lines()
            .any(|line| line.starts_with("trace: server fx: sent {")),
        "the trace of -vv: {stderr}"
    );
    let timings = stderr
        .lines()
        .find(|line| line.starts_with("timings: "))
        .unwrap_or_else(|| panic!("{stderr}"));
    assert!(timings.contains("server start "), "{timings}");
    assert!(timings.contains("responses "), "{timings}");
    assert_eq!(
        stderr.lines().last(),
        Some(timings),
        "the timings come last"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn login_to_the_file_waits_at_most_the_cleanup_limit_for_a_native_store_with_no_answer() {
    let (_auth, fixture) = oauth_fixture(AuthSettings::default()).await;
    let home = oauth_home("cleanup-limit", &fixture.url).with_fake_keychain();
    // The native store gives no answer, as at an unlock dialog.
    std::fs::write(home.root.join("keychain/delay"), "60000").unwrap();
    let started = std::time::Instant::now();
    let run = home.run(&LOGIN_TO_FILE);
    let elapsed = started.elapsed();
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert!(elapsed < std::time::Duration::from_secs(30), "{elapsed:?}");
    assert!(!notes(&run, "warning").is_empty(), "{}", run.describe());
}

#[test]
fn a_request_that_needs_a_page_gives_exit_5_with_the_pages() {
    let home = Home::new("needs-page");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = home.run(&["tool", "call", "fx", "needs-page", "text=x"]);
    assert_eq!(run.code(), 5, "{}", run.describe());
    let error = run.error();
    assert_eq!(
        error["pages"],
        json!([{"url": "https://login.example.test/start", "message": "Connect your account"}])
    );
    // On a TTY, ncp asks to open the page and does not run the call again.
    let home = home.set("NCP_TEST_TTY", "1");
    let run = home.run_with_stdin(&["tool", "call", "fx", "needs-page", "text=x"], "n\n");
    assert_eq!(run.code(), 5, "{}", run.describe());
    let stderr = String::from_utf8_lossy(&run.0.stderr).to_string();
    assert!(stderr.contains("a page of login.example.test"), "{stderr}");
    // A real TTY echoes the new line of the answer; a pipe does not.
    let last: Value = serde_json::from_str(&stderr[stderr.rfind("{\"error\"").unwrap()..]).unwrap();
    assert_eq!(
        last["error"]["hint"],
        "open the page, then run the command again"
    );
}

#[test]
fn a_page_that_is_not_safe_is_refused_and_no_page_is_a_protocol_error() {
    let home = Home::new("needs-page-unsafe");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = home.run(&["tool", "call", "fx", "needs-page", "text=unsafe"]);
    assert_eq!(run.code(), 5, "{}", run.describe());
    let error = run.error();
    assert!(error.get("pages").is_none(), "{error}");
    let message = error["message"].as_str().unwrap();
    assert!(
        message.contains("refused a page") && !message.contains("alert"),
        "{message}"
    );
    let run = home.run(&["tool", "call", "fx", "needs-page", "text=none"]);
    assert_eq!(run.code(), 3, "{}", run.describe());
}

/// A server that answers each request with the status line and headers,
/// and counts its connections.
fn raw_server(answer: String) -> (String, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
    use std::io::{Read as _, Write as _};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = count.clone();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let mut buffer = [0_u8; 8192];
            let _ = stream.read(&mut buffer);
            let _ = stream.write_all(answer.as_bytes());
        }
    });
    (url, count)
}

#[test]
fn http_server_that_redirects_stops_and_sends_no_header_to_the_new_location() {
    let ok = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string();
    let (target, target_hits) = raw_server(ok);
    // A trailing-slash redirect that keeps the query, with the value of
    // ${KEY} in it.
    let location = format!("{target}/?api_key=k1-secret&page=2");
    let (url, _) = raw_server(format!(
        "HTTP/1.1 308 Permanent Redirect\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    ));
    let home = Home::new("redirect").set("KEY", "k1-secret");
    home.configure(
        &json!({"web": {"type": "http", "url": format!("{url}?api_key=${{KEY}}"),
        "headers": {"X-Api-Key": "secret-value"}}}),
    );
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 3, "{}", run.describe());
    let error = run.error();
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("308"), "{message}");
    assert!(
        message.contains(&format!("{target}/?api_key=****&page=2")),
        "{message}"
    );
    assert!(!run.describe().contains("k1-secret"), "{message}");
    assert!(error["hint"].as_str().unwrap().contains("new URL"));
    assert_eq!(target_hits.load(std::sync::atomic::Ordering::SeqCst), 0);
}

fn list_file(home: &Home, server: &str) -> PathBuf {
    home.root.join(format!("cache/ncp/lists/{server}.json"))
}

#[test]
fn private_list_of_a_stdio_server_is_never_kept() {
    let home = Home::new("cache-private-stdio");
    home.configure(&json!({"fx": stdio("cached-public")}));
    assert_eq!(home.run(&["tool", "list", "fx"]).code(), 0);
    assert!(list_file(&home, "fx").exists());
    // The same server now says that the list is private: the old file goes.
    home.configure(&json!({"fx": stdio("cached")}));
    assert_eq!(home.run(&["tool", "list", "fx"]).code(), 0);
    assert!(!list_file(&home, "fx").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn private_list_of_an_http_server_is_kept_only_with_no_credential() {
    let open = serve_http(Mode::Cached).await.unwrap();
    let locked = serve_http_with(Mode::Cached, Some("t0ken")).await.unwrap();
    let home = Home::new("cache-private-http").set("WEB_TOKEN", "t0ken");
    home.configure(&json!({
        "open": {"type": "http", "url": open.url},
        "locked": {"type": "http", "url": locked.url,
            "headers": {"Authorization": "Bearer ${WEB_TOKEN}"}}
    }));
    assert_eq!(home.run(&["tool", "list", "open"]).code(), 0);
    assert_eq!(home.run(&["tool", "list", "locked"]).code(), 0);
    assert!(list_file(&home, "open").exists());
    assert!(!list_file(&home, "locked").exists());
}

async fn oauth_fixture_in(
    mode: Mode,
) -> (
    ncp_testkit::auth_server::AuthServer,
    ncp_testkit::HttpFixture,
) {
    let auth = ncp_testkit::auth_server::serve_auth(AuthSettings::default())
        .await
        .unwrap();
    let fixture = ncp_testkit::serve_http_with_oauth(mode, &auth, None)
        .await
        .unwrap();
    (auth, fixture)
}

#[tokio::test(flavor = "multi_thread")]
async fn private_list_after_a_login_is_not_kept_and_a_public_one_is() {
    let (_auth, private) = oauth_fixture_in(Mode::Cached).await;
    let home = oauth_home("cache-oauth-private", &private.url);
    assert_eq!(home.run(&LOGIN_TO_FILE).code(), 0);
    assert_eq!(home.run(&["tool", "list", "web"]).code(), 0);
    assert!(!list_file(&home, "web").exists());

    let (_auth, public) = oauth_fixture_in(Mode::CachedPublic).await;
    let home = oauth_home("cache-oauth-public", &public.url);
    assert_eq!(home.run(&LOGIN_TO_FILE).code(), 0);
    assert_eq!(home.run(&["tool", "list", "web"]).code(), 0);
    assert!(list_file(&home, "web").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn login_logout_and_a_secret_change_remove_the_cached_lists() {
    let (_auth, fixture) = oauth_fixture_in(Mode::CachedPublic).await;
    let home = oauth_home("cache-removal", &fixture.url);
    let cached = |home: &Home| {
        assert_eq!(home.run(&["tool", "list", "web"]).code(), 0);
        assert!(list_file(home, "web").exists());
    };
    assert_eq!(home.run(&LOGIN_TO_FILE).code(), 0);
    cached(&home);
    assert_eq!(home.run(&LOGIN_TO_FILE).code(), 0);
    assert!(
        !list_file(&home, "web").exists(),
        "a login removes the list"
    );
    cached(&home);
    assert_eq!(home.run(&["auth", "logout", "web"]).code(), 0);
    assert!(
        !list_file(&home, "web").exists(),
        "a logout removes the list"
    );
    assert_eq!(home.run(&LOGIN_TO_FILE).code(), 0);
    cached(&home);
    let run = home.run_with_stdin(
        &["secret", "set", "ANY", "--credential-store", "file"],
        "v\n",
    );
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert!(
        !list_file(&home, "web").exists(),
        "a change of a secret removes the lists"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn login_to_an_authorization_server_with_an_unknown_issuer_gives_the_ssl_cert_file_hint() {
    let auth = ncp_testkit::auth_server::serve_auth_tls(AuthSettings::default())
        .await
        .unwrap();
    let fixture = ncp_testkit::serve_http_with_oauth(Mode::Modern, &auth, None)
        .await
        .unwrap();
    let home = oauth_home("login-unknown-issuer", &fixture.url)
        .set("SSL_CERT_FILE", ncp_testkit::OTHER_CA);
    let run = home.run(&LOGIN_TO_FILE);
    assert_ne!(run.code(), 0, "{}", run.describe());
    let hint = run.error()["hint"].as_str().unwrap_or_default().to_string();
    assert!(hint.contains("SSL_CERT_FILE"), "{}", run.describe());
}

#[tokio::test(flavor = "multi_thread")]
async fn oauth_trace_hides_a_value_in_the_url_path() {
    let (_auth, fixture) = oauth_fixture(AuthSettings::default()).await;
    // The fixture serves only its own path, so the value is that path.
    let base = fixture.url.trim_end_matches("/mcp").to_string();
    let home = Home::new("oauth-path-value")
        .set("NCP_TEST_BROWSER", "follow")
        .set("TENANT_KEY", "mcp");
    home.configure(&json!({"web": {"type": "http", "url": format!("{base}/${{TENANT_KEY}}")}}));
    let run = home.run(&["-vv", "auth", "login", "web", "--credential-store", "file"]);
    let stderr = String::from_utf8_lossy(&run.0.stderr).to_string();
    assert_eq!(run.code(), 0, "{stderr}");
    // The URL to open must be whole, so only the traces count. They show
    // the resource in the metadata and in the token request.
    let traces: Vec<&str> = stderr
        .lines()
        .filter(|line| line.starts_with("{\"trace\""))
        .collect();
    assert!(
        traces.iter().any(|line| line.contains("resource")),
        "{stderr}"
    );
    assert!(traces.iter().all(|line| !line.contains("/mcp")), "{stderr}");
}

#[test]
fn issuer_lookup_of_a_slow_server_keeps_the_usage_error() {
    // A server that takes connections and never answers.
    let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/mcp", silent.local_addr().unwrap());
    let home = Home::new("issuer-lookup-slow");
    home.configure(&json!({"web": {"type": "http", "url": url,
        "oauth": {"clientId": "c", "clientSecret": "s3cr3t-value"}}}));
    let started = std::time::Instant::now();
    let run = home.run(&[
        "--timeout",
        "4",
        "auth",
        "login",
        "web",
        "--credential-store",
        "file",
    ]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert!(started.elapsed() < std::time::Duration::from_secs(4));
    drop(silent);
}

#[test]
fn failed_login_hides_a_value_in_the_url_path() {
    // Each request gets 401 with no metadata, so discovery fails and names
    // the URLs that it made from the server URL.
    let (url, _) = raw_server(
        "HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Bearer\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            .to_string(),
    );
    let base = url.trim_end_matches("/mcp").to_string();
    let home = Home::new("login-error-value").set("KEY", "t3nant-k3y");
    home.configure(&json!({"web": {"type": "http", "url": format!("{base}/${{KEY}}/mcp")}}));
    let run = home.run(&["-vv", "auth", "login", "web", "--credential-store", "file"]);
    assert_ne!(run.code(), 0, "{}", run.describe());
    let stderr = String::from_utf8_lossy(&run.0.stderr).to_string();
    assert!(!stderr.contains("t3nant-k3y"), "{stderr}");
}
