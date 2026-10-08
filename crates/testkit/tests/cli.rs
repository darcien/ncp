//! Tests of the `ncp` command line against local fixture servers.
//!
//! Each test runs the binary with a clean environment and its own `HOME`
//! and XDG directories. stdout is a pipe, so the output is JSON.

// The whole file is test code, where the agent rules permit unwrap.
#![allow(clippy::unwrap_used)]

use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

use ncp_testkit::{Mode, serve_http, serve_http_with};
use serde_json::{Value, json};

const NCP: &str = env!("CARGO_BIN_EXE_ncp-cli");
const FIXTURE: &str = env!("CARGO_BIN_EXE_ncp-fixture");

/// A temporary home for one test.
struct Home {
    root: PathBuf,
    env: Vec<(String, String)>,
}

impl Home {
    fn new(test: &str) -> Self {
        let root = std::env::temp_dir().join(format!("ncp-cli-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let env = vec![
            ("HOME".into(), root.display().to_string()),
            (
                "XDG_CONFIG_HOME".into(),
                root.join("config").display().to_string(),
            ),
            (
                "XDG_CACHE_HOME".into(),
                root.join("cache").display().to_string(),
            ),
            (
                "XDG_STATE_HOME".into(),
                root.join("state").display().to_string(),
            ),
        ];
        Self { root, env }
    }

    fn config_path(&self) -> PathBuf {
        self.root.join("config/ncp/mcp.json")
    }

    fn configure(&self, servers: &Value) -> &Self {
        let path = self.config_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, json!({ "mcpServers": servers }).to_string()).unwrap();
        self
    }

    fn set(mut self, name: &str, value: &str) -> Self {
        self.env.push((name.into(), value.into()));
        self
    }

    fn run(&self, args: &[&str]) -> Run {
        self.run_with_stdin(args, "")
    }

    fn run_with_stdin(&self, args: &[&str], stdin: &str) -> Run {
        use std::io::Write as _;
        let mut child = Command::new(NCP)
            .args(args)
            .env_clear()
            .envs(self.env.iter().cloned())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        Run(child.wait_with_output().unwrap())
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct Run(Output);

impl Run {
    fn code(&self) -> i32 {
        self.0.status.code().unwrap()
    }

    fn stdout(&self) -> Value {
        serde_json::from_slice(&self.0.stdout)
            .unwrap_or_else(|_| panic!("stdout is not JSON: {}", self.describe()))
    }

    fn stdout_is_empty(&self) -> bool {
        self.0.stdout.is_empty()
    }

    /// The JSON Lines on stderr.
    fn stderr(&self) -> Vec<Value> {
        String::from_utf8_lossy(&self.0.stderr)
            .lines()
            .map(|line| serde_json::from_str(line).unwrap_or_else(|_| panic!("not JSON: {line}")))
            .collect()
    }

    fn error(&self) -> Value {
        self.stderr()
            .into_iter()
            .find_map(|line| line.get("error").cloned())
            .unwrap_or_else(|| panic!("no error: {}", self.describe()))
    }

    fn describe(&self) -> String {
        format!(
            "exit {:?}\nstdout: {}\nstderr: {}",
            self.0.status.code(),
            String::from_utf8_lossy(&self.0.stdout),
            String::from_utf8_lossy(&self.0.stderr)
        )
    }
}

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
fn unset_variable_gives_a_usage_error_with_the_fix() {
    let home = Home::new("unset");
    home.configure(&json!({"fx": {"command": FIXTURE, "args": ["modern"],
        "env": {"TOKEN": "${env:FIXTURE_TOKEN}"}}}));
    let run = home.run(&["tool", "list", "fx"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert_eq!(run.error()["hint"], "export FIXTURE_TOKEN=<value>");
    let home = home.set("FIXTURE_TOKEN", "t");
    let run = home.run(&["tool", "call", "fx", "env", "name=TOKEN"]);
    assert_eq!(text_of(&run.stdout()), "t");
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
    assert_eq!(run.error()["hint"], "ncp tool show fx nope");
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
