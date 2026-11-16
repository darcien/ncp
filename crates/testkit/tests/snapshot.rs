//! Tests of the transcript format and of the snapshot filters.

#![allow(clippy::unwrap_used)]

use std::{
    os::unix::process::ExitStatusExt as _,
    path::Path,
    process::{Command, ExitStatus, Output},
};

mod common;

use common::{Home, apply_filters, ncp_snapshot};
use ncp_testkit::{Mode, serve_http};
use serde_json::{Value, json};

const FIXTURE: &str = env!("CARGO_BIN_EXE_ncp-fixture");
const FAKE_SECURITY: &str = env!("CARGO_BIN_EXE_fake-security");
const BLOCKED_STORE: &str = env!("CARGO_BIN_EXE_ncp-blocked-store");

fn stdio(mode: &str) -> Value {
    json!({"command": FIXTURE, "args": [mode]})
}

fn shell(script: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", script]);
    command
}

#[test]
fn empty_streams_have_no_sections() {
    let run = ncp_snapshot!([], shell("exit 7"), @"exit_code: 7 (failure)");
    assert_eq!(run.code(), 7);
    assert_eq!(run.0.stdout, [] as [u8; 0]);
    assert_eq!(run.0.stderr, [] as [u8; 0]);
    assert_eq!(
        common::transcript(&run.0).as_bytes(),
        b"exit_code: 7 (failure)\n"
    );
}

#[test]
fn signal_status_remains_visible() {
    let run = ncp_snapshot!([], shell("kill -TERM $$"), @"
    exit_code: -1 (failure)
    exit_status: signal: 15 (SIGTERM)
    ");
    assert_eq!(run.0.status.signal(), Some(15));
    assert_eq!(run.0.status.code(), None);
}

#[test]
fn streams_have_an_empty_line_between_them() {
    let run = ncp_snapshot!([], shell("printf 'out\n'; printf 'err\n' >&2"), @"
    exit_code: 0 (success)
    ----- stdout -----
    out

    ----- stderr -----
    err
    ");
    assert_eq!(run.0.stdout, b"out\n");
    assert_eq!(run.0.stderr, b"err\n");
    assert_eq!(
        common::transcript(&run.0),
        "exit_code: 0 (success)\n----- stdout -----\nout\n\n----- stderr -----\nerr\n"
    );
}

#[test]
fn streams_without_final_newlines_still_have_an_empty_line_between_them() {
    let run = ncp_snapshot!([], shell("printf out; printf err >&2"), @"
    exit_code: 0 (success)
    ----- stdout -----
    out

    ----- stderr -----
    err
    ");
    assert_eq!(run.0.stdout, b"out");
    assert_eq!(run.0.stderr, b"err");
    assert!(!common::transcript(&run.0).ends_with('\n'));
}

#[test]
fn stdin_and_the_returned_output_come_from_one_run() {
    let home = Home::new("snapshot-same-run");
    let mut command = home.command(Path::new("/bin/sh"));
    command.args(["-c", "printf x >> count; cat; printf 'done\n' >&2"]);
    let run = ncp_snapshot!(home.filters(), command, input = "abc\n", @"
    exit_code: 0 (success)
    ----- stdout -----
    abc

    ----- stderr -----
    done
    ");
    assert_eq!(home.read("count"), "x");
    assert_eq!(run.0.stdout, b"abc\n");
    assert_eq!(run.0.stderr, b"done\n");
}

#[test]
fn an_empty_stdin_is_closed() {
    let run = ncp_snapshot!([], shell("cat; printf eof"), @"
    exit_code: 0 (success)
    ----- stdout -----
    eof
    ");
    assert_eq!(run.0.stdout, b"eof");
}

#[test]
fn transcript_conversion_does_not_replace_raw_byte_assertions() {
    let output = Output {
        status: ExitStatus::from_raw(0),
        stdout: vec![b'a', 0xff, b' ', b'\r', b'\n'],
        stderr: Vec::new(),
    };
    assert_eq!(
        common::transcript(&output),
        "exit_code: 0 (success)\n----- stdout -----\na\u{fffd} \r\n"
    );
    assert_eq!(output.stdout, [b'a', 0xff, b' ', b'\r', b'\n']);
}

#[test]
fn usage_names_ncp_and_commands_remain_adjustable() {
    let home = Home::new("snapshot-usage");
    let mut command = home.ncp(["tool", "list"]);
    command.env("EXTRA_TEST_VALUE", "present");
    assert_eq!(command.get_current_dir(), Some(home.cwd.as_path()));
    assert!(
        command
            .get_envs()
            .any(|(key, value)| key == "EXTRA_TEST_VALUE" && value == Some("present".as_ref()))
    );
    let run = ncp_snapshot!(home.filters(), command, @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: the following required arguments were not provided:
      <SERVER>

    Usage: ncp tool list <SERVER>

    For more information, try '--help'.
    ");
    assert_eq!(run.0.stdout, [] as [u8; 0]);
    assert!(run.stderr_text().contains("Usage: ncp tool list"));
}

#[test]
fn cli_version_filters_leave_the_real_version_available() {
    let home = Home::new("snapshot-version");
    let run = ncp_snapshot!(home.filters(), home.ncp(["--version"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    ncp [VERSION]
    ");
    assert_eq!(
        run.stdout_text(),
        format!("ncp {}\n", env!("CARGO_PKG_VERSION"))
    );
    let run = ncp_snapshot!(home.filters(), home.ncp(["server", "--version"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    ncp-server [VERSION]
    ");
    assert_eq!(
        run.stdout_text(),
        format!("ncp-server {}\n", env!("CARGO_PKG_VERSION"))
    );
    let run = ncp_snapshot!(home.filters(), home.ncp(["tool", "list", "--version"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    ncp-tool-list [VERSION]
    ");
    assert_eq!(
        run.stdout_text(),
        format!("ncp-tool-list {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn text_and_json_keep_the_consumer_layout() {
    let home = Home::new("snapshot-formats");
    home.configure(&json!({"fx": stdio("modern")}));
    let text = ncp_snapshot!(home.filters(), home.ncp(["server", "list"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    fx  stdio  [FIXTURE] modern
    ");
    assert_eq!(text.0.stderr, [] as [u8; 0]);
    let json = ncp_snapshot!(home.filters(), home.ncp(["--output-format", "json", "server", "list"]), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"invalid":[],"servers":[{"file":"[HOME]/config/ncp/mcp.json","name":"fx","scope":"global","target":"[FIXTURE] modern","transport":"stdio"}]}
    "#);
    assert_eq!(json.stdout_text().lines().count(), 1);
    assert!(json.0.stdout.ends_with(b"\n"));
    assert_eq!(
        json.stdout()["servers"][0]["target"],
        format!("{FIXTURE} modern")
    );
}

#[test]
fn existing_helpers_keep_their_json_default() {
    let home = Home::new("snapshot-old-helper");
    assert_eq!(
        home.run(&["server", "list"]).stdout(),
        json!({"invalid": [], "servers": []})
    );
    let text = home.run_with_default_format(&["server", "list"]);
    assert_eq!(text.code(), 0);
    assert!(!text.stdout_text().starts_with('{'));
}

#[test]
fn executable_paths_have_distinct_placeholders_in_transcripts_and_files() {
    let home = Home::new("snapshot-executables");
    let mut command = shell("printf '%s\n' \"$@\"");
    command.args([
        "paths",
        common::NCP,
        FIXTURE,
        FAKE_SECURITY,
        BLOCKED_STORE,
        "--serve",
        "modern",
    ]);
    let run = ncp_snapshot!(home.filters(), command, @"
    exit_code: 0 (success)
    ----- stdout -----
    [NCP]
    [FIXTURE]
    [FAKE_SECURITY]
    [BLOCKED_STORE]
    --serve
    modern
    ");
    assert!(run.stdout_text().contains(FIXTURE));
    let contents = json!({
        "programs": [common::NCP, FIXTURE, FAKE_SECURITY, BLOCKED_STORE],
        "args": ["--serve", "modern"],
    })
    .to_string();
    std::fs::write(home.root.join("executables.json"), &contents).unwrap();
    insta::with_settings!({filters => home.filters()}, {
        insta::assert_snapshot!(home.read("executables.json"), @r#"{"args":["--serve","modern"],"programs":["[NCP]","[FIXTURE]","[FAKE_SECURITY]","[BLOCKED_STORE]"]}"#);
    });
    assert_eq!(home.read("executables.json"), contents);
}

#[test]
fn executable_filters_do_not_replace_sibling_names_arguments_or_build_directories() {
    let executables = [
        (common::NCP, "NCP"),
        (FIXTURE, "FIXTURE"),
        (FAKE_SECURITY, "FAKE_SECURITY"),
        (BLOCKED_STORE, "BLOCKED_STORE"),
    ];
    for (program, name) in executables {
        let filters = common::path_filters(Path::new(program), name, false);
        let filters: Vec<_> = filters
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        let parent = Path::new(program).parent().unwrap().display();
        let unchanged = format!(
            "{program}-other {program}.bak {program}/child /outside{program} {parent}/unrelated --program={program}-other ncp-fixture --version 7.8.9"
        );
        assert_eq!(apply_filters(unchanged.clone(), &filters), unchanged);
        assert_eq!(
            apply_filters(format!("\"{program}\" `{program}` ({program})"), &filters),
            format!("\"[{name}]\" `[{name}]` ([{name}])")
        );
        assert_eq!(
            apply_filters(format!("{program} {program} {program}"), &filters),
            format!("[{name}] [{name}] [{name}]")
        );
    }
}

#[test]
fn standard_filters_keep_sibling_executable_names_in_selected_files() {
    let home = Home::new("snapshot-sibling-file");
    for program in [common::NCP, FIXTURE, FAKE_SECURITY, BLOCKED_STORE] {
        let path = Path::new(program);
        let parent = path.parent().unwrap();
        let filename = path.file_name().unwrap().to_str().unwrap();
        let filtered_parent = apply_filters(parent.display().to_string(), &home.filters());
        let contents = json!({
            "command": format!("{program}-other"),
            "args": [format!("{program}.bak"), parent.join("unrelated").display().to_string(), "--serve".to_string()],
        }).to_string();
        std::fs::write(home.root.join("siblings.json"), contents).unwrap();
        let filtered = apply_filters(home.read("siblings.json"), &home.filters());
        assert_eq!(
            serde_json::from_str::<Value>(&filtered).unwrap(),
            json!({
                "command": format!("{filtered_parent}/{filename}-other"),
                "args": [format!("{filtered_parent}/{filename}.bak"), format!("{filtered_parent}/unrelated"), "--serve".to_string()],
            })
        );
        assert!(!filtered.contains("[TARGET]"));
    }
}

#[test]
fn path_aliases_escape_regex_characters_and_preserve_separators() {
    let home = Home::new("snapshot-path-aliases");
    let real = home.root.join("real.[a]+");
    let alias = home.root.join("alias.(b)");
    std::fs::create_dir_all(&real).unwrap();
    std::os::unix::fs::symlink(&real, &alias).unwrap();
    let filters = common::path_filters(&alias, "PATH", true);
    let filters: Vec<_> = filters
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let canonical = std::fs::canonicalize(&real).unwrap();
    let text = format!(
        "{}\n{}/\n{}/child\n{}\n{}/child\n{}-sibling\n{}-sibling",
        canonical.display(),
        canonical.display(),
        canonical.display(),
        alias.display(),
        alias.display(),
        canonical.display(),
        alias.display()
    );
    let filtered = apply_filters(text, &filters);
    assert_eq!(
        filtered,
        format!(
            "[PATH]\n[PATH]/\n[PATH]/child\n[PATH]\n[PATH]/child\n{}-sibling\n{}-sibling",
            canonical.display(),
            alias.display()
        )
    );
    assert!(!filtered.contains("/private[PATH]"));
}

#[test]
fn executable_aliases_cover_both_real_and_supplied_paths() {
    let home = Home::new("snapshot-executable-alias");
    let alias = home.root.join("alias.[exe]+");
    std::os::unix::fs::symlink(common::NCP, &alias).unwrap();
    let real = std::fs::canonicalize(&alias).unwrap();
    let filters = common::path_filters(&alias, "PROGRAM", false);
    let filters: Vec<_> = filters
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    assert_eq!(
        apply_filters(format!("{} {}", real.display(), alias.display()), &filters),
        "[PROGRAM] [PROGRAM]"
    );
}

#[test]
fn more_specific_paths_precede_enclosing_paths() {
    let home = Home::new("snapshot-enclosing");
    let text = format!(
        "{}\n{}/\n{}/child\n{}-sibling",
        home.root.display(),
        home.root.display(),
        home.root.display(),
        home.root.display()
    );
    assert_eq!(
        apply_filters(text, &home.filters()),
        format!(
            "[HOME]\n[HOME]/\n[HOME]/child\n{}-sibling",
            home.root.display()
        )
    );
    let canonical = std::fs::canonicalize(&home.root).unwrap();
    assert_eq!(
        apply_filters(canonical.display().to_string(), &home.filters()),
        "[HOME]"
    );
    assert_eq!(apply_filters(common::NCP.into(), &home.filters()), "[NCP]");
}

#[test]
fn filters_before_and_after_standard_filters_keep_two_hosts_distinct() {
    let home = Home::new("snapshot-two-hosts");
    let mut filters = vec![(r"127\.0\.0\.1:54322\b", "[SECOND]")];
    filters.extend(home.filters());
    filters.push((r"\[SECOND\]/mcp", "[SECOND]/alternate"));
    let text = "http://127.0.0.1:54321/mcp http://127.0.0.1:54322/mcp\n{\"first\":\"http://127.0.0.1:54321/mcp\",\"second\":\"http://127.0.0.1:54322/mcp\"}";
    insta::assert_snapshot!(apply_filters(text.into(), &filters), @r#"
    http://[LOCALHOST]/mcp http://[SECOND]/alternate
    {"first":"http://[LOCALHOST]/mcp","second":"http://[SECOND]/alternate"}
    "#);
    let unchanged = "127.0.0.1:port 127.0.0.1: 1127.0.0.1:1234 192.0.2.1:54321";
    assert_eq!(apply_filters(unchanged.into(), &home.filters()), unchanged);
}

#[test]
fn missing_file_filters_are_opt_in_and_preserve_adjacent_causes() {
    let home = Home::new("snapshot-missing-file");
    let text = "error: The file could not be read\n  cause: outer cause\n  cause: No such file or directory (os error 2)\n  cause: next cause\n  cause: The system cannot find the path specified. (os error 3)\n  cause: Permission denied (os error 13)\n";
    assert_eq!(apply_filters(text.into(), &home.filters()), text);
    insta::assert_snapshot!(apply_filters(text.into(), &common::missing_file_filters()), @"
    error: The file could not be read
      cause: outer cause
      cause: [OS ERROR 2]
      cause: next cause
      cause: [OS ERROR 2]
      cause: Permission denied (os error 13)
    ");
    let json = r#"{"error":{"causes":["outer \"quoted\" cause","No such file or directory (os error 2)","next cause","Das System kann den angegebenen Pfad nicht finden. (os error 3)","Permission denied (os error 13)"]}}"#;
    let filtered = apply_filters(json.into(), &common::missing_file_filters());
    insta::assert_snapshot!(filtered, @r#"{"error":{"causes":["outer \"quoted\" cause","[OS ERROR 2]","next cause","[OS ERROR 2]","Permission denied (os error 13)"]}}"#);
    assert_eq!(
        serde_json::from_str::<Value>(&filtered).unwrap()["error"]["causes"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    let with_path = "  cause: /tmp/missing.json: No such file or directory (os error 2)\n";
    assert_eq!(
        apply_filters(with_path.into(), &common::missing_file_filters()),
        "  cause: /tmp/missing.json: [OS ERROR 2]\n"
    );
    let json = r#"{"causes":["/tmp/missing.json: No such file or directory (os error 2)"]}"#;
    assert_eq!(
        apply_filters(json.into(), &common::missing_file_filters()),
        r#"{"causes":["/tmp/missing.json: [OS ERROR 2]"]}"#
    );
}

#[test]
fn refused_connection_filters_match_only_known_native_variants() {
    let home = Home::new("snapshot-refused");
    let variants = [
        "Connection refused (os error 61)",
        "Connection refused (os error 111)",
        "No connection could be made because the target machine actively refused it. (os error 10061)",
    ];
    for cause in variants {
        let text = format!(
            "error: The handshake failed\n  cause: outer\n  cause: {cause}\n  cause: next\n"
        );
        assert_eq!(apply_filters(text.clone(), &home.filters()), text);
        assert_eq!(
            apply_filters(text, &common::connection_refused_filters()),
            "error: The handshake failed\n  cause: outer\n  cause: [CONNECTION REFUSED]\n  cause: next\n"
        );
        let json = json!({"error": {"causes": ["outer", cause, "next"]}}).to_string();
        assert_eq!(
            apply_filters(json, &common::connection_refused_filters()),
            r#"{"error":{"causes":["outer","[CONNECTION REFUSED]","next"]}}"#
        );
    }
    let unchanged = [
        "Connection timed out (os error 110)",
        "Operation timed out (os error 60)",
        "Permission denied (os error 13)",
        "failed to lookup address information: nodename nor servname provided, or not known",
        "Name or service not known (os error -2)",
        "Connection refused (os error 13)",
        "Connection refused by policy",
    ]
    .join("\n");
    assert_eq!(
        apply_filters(unchanged.clone(), &common::connection_refused_filters()),
        unchanged
    );
    let json = json!({"causes": unchanged.lines().collect::<Vec<_>>()}).to_string();
    assert_eq!(
        apply_filters(json.clone(), &common::connection_refused_filters()),
        json
    );
}

#[test]
fn measured_times_use_diagnostic_contexts_and_fixed_values_stay_exact() {
    let home = Home::new("snapshot-measured-times");
    let text = [
        "timings: total 41 ms, cpu 8.0 ms, config 0.4 ms, credentials 1 ms, server start 2.5 ms, dns 2 ms, connect 3.2 ms, responses 18.0 ms 9 ms",
        r#"{"timings":{"configMs":0.4,"connectMs":3,"cpuMs":8.0,"credentialsMs":1,"dnsMs":2.1,"responseMs":[18.0,9],"serverStartMs":2.5,"totalMs":41}}"#,
        "trace: server web: POST http://127.0.0.1:53817/mcp [mcp-method: tools/list]: event stream in 12 ms",
        "trace: server web: POST http://127.0.0.1:53817/mcp [mcp-method: tools/list]: JSON response in 3.4 ms",
        r#"{"trace":{"http":{"headers":{"mcp-method":"tools/list"},"method":"POST","ms":12,"outcome":"event stream"},"server":"web"}}"#,
        r#"{"trace":{"http":{"headers":{},"method":"GET","ms":3.4,"outcome":"JSON response"},"server":"web"}}"#,
        "debug: server fx: tools from the list cache, 12 s old",
        r#"{"debug":{"message":"tools from the list cache, 3 s old","server":"fx"}}"#,
        "  cause: `initialize`, after `server/discover` gave no answer in 500 ms",
        r#"{"trace":{"received":{"result":{"ttlMs":60000,"totalMs":41,"responseMs":[18,9],"http":{"method":"POST","ms":12}}},"server":"web"}}"#,
        "payload: total 12.4 ms, response in 3.4 ms, 12 s old",
    ].join("\n");
    let filtered = apply_filters(common::normalize_timings(&text), &home.filters());
    insta::assert_snapshot!(filtered, @r#"
    timings: total [TIME], cpu [TIME], config [TIME], credentials [TIME], server start [TIME], dns [TIME], connect [TIME], responses [TIME] [TIME]
    {"timings":{"configMs":[TIME],"connectMs":[TIME],"cpuMs":[TIME],"credentialsMs":[TIME],"dnsMs":[TIME],"responseMs":[[TIME],[TIME]],"serverStartMs":[TIME],"totalMs":[TIME]}}
    trace: server web: POST http://[LOCALHOST]/mcp [mcp-method: tools/list]: event stream in [TIME]
    trace: server web: POST http://[LOCALHOST]/mcp [mcp-method: tools/list]: JSON response in [TIME]
    {"trace":{"http":{"headers":{"mcp-method":"tools/list"},"method":"POST","ms":[TIME],"outcome":"event stream"},"server":"web"}}
    {"trace":{"http":{"headers":{},"method":"GET","ms":[TIME],"outcome":"JSON response"},"server":"web"}}
    debug: server fx: tools from the list cache, [N] s old
    {"debug":{"message":"tools from the list cache, [N] s old","server":"fx"}}
      cause: `initialize`, after `server/discover` gave no answer in 500 ms
    {"trace":{"received":{"result":{"ttlMs":60000,"totalMs":41,"responseMs":[18,9],"http":{"method":"POST","ms":12}}},"server":"web"}}
    payload: total 12.4 ms, response in 3.4 ms, 12 s old
    "#);
}

#[test]
fn every_response_time_keeps_its_own_placeholder() {
    let times = (1..=40).map(|n| format!("{n}.5 ms")).collect::<Vec<_>>();
    let text = format!(
        "timings: total 41 ms, responses {}\n{{\"timings\":{{\"responseMs\":[{}],\"totalMs\":41}}}}",
        times.join(" "),
        (1..=40)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(","),
    );
    let filtered = common::normalize_timings(&text);
    let placeholders = vec!["[TIME]"; 40];
    assert_eq!(
        filtered,
        format!(
            "timings: total [TIME], responses {}\n{{\"timings\":{{\"responseMs\":[{}],\"totalMs\":[TIME]}}}}",
            placeholders.join(" "),
            placeholders.join(","),
        )
    );
}

#[test]
fn versions_in_server_metadata_and_unrelated_payloads_stay_exact() {
    let home = Home::new("snapshot-version-scope");
    for name in ["ncp", "rmcp", "ncp-fixture", "fixture-server"] {
        let text = [
            format!("{name} 7.8.9"),
            format!("Server info: {name} 7.8.9"),
            json!({"name": name, "version": "7.8.9"}).to_string(),
            json!({"serverInfo": {"name": name, "version": "7.8.9"}}).to_string(),
            json!({"trace": {"received": {"result": {"serverInfo": {"name": name, "version": "7.8.9"}, "_meta": {"io.modelcontextprotocol/serverInfo": {"name": name, "version": "7.8.9"}}}}}}).to_string(),
            json!({"trace": {"sent": {"jsonrpc": "2.0", "method": "tools/call", "params": {"arguments": {"clientInfo": {"name": name, "version": "7.8.9"}}}}}}).to_string(),
            json!({"trace": {"sent": {"jsonrpc": "2.0", "method": "tools/call", "params": {"arguments": {"request": {"jsonrpc": "2.0", "method": "initialize", "params": {"capabilities": {}, "clientInfo": {"name": name, "version": "7.8.9"}}}}}}}}).to_string(),
        ].join("\n");
        assert_eq!(
            apply_filters(text.clone(), &home.filters()),
            text,
            "name: {name}"
        );
    }
    let text = "trace: server fx: received {\"clientInfo\":{\"name\":\"ncp\",\"version\":\"7.8.9\"}}\n{\"payload\":\"ncp-server 7.8.9\"}\nncp-unrelated 7.8.9\n";
    assert_eq!(apply_filters(text.into(), &home.filters()), text);
}

#[test]
fn echoed_version_text_is_not_a_cli_version_result() {
    let home = Home::new("snapshot-echoed-version");
    home.configure(&json!({"fx": stdio("modern")}));
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["tool", "call", "fx", "echo", "text=ncp 7.8.9"]),
        @"
    exit_code: 0 (success)
    ----- stdout -----
    ncp 7.8.9
    "
    );
    assert_eq!(run.0.stdout, b"ncp 7.8.9\n");
}

#[test]
fn outgoing_client_versions_are_filtered_only_in_trace_metadata() {
    let home = Home::new("snapshot-client-version");
    let text = [
        r#"trace: server fx: sent {"id":0,"jsonrpc":"2.0","method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/clientCapabilities":{},"io.modelcontextprotocol/clientInfo":{"name":"ncp","version":"9.8.7"},"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}}"#,
        r#"{"trace":{"sent":{"id":0,"jsonrpc":"2.0","method":"initialize","params":{"capabilities":{},"clientInfo":{"name":"ncp","version":"9.8.7"},"protocolVersion":"2025-11-25"}},"server":"fx"}}"#,
        r#"{"trace":{"sent":{"id":1,"jsonrpc":"2.0","method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/clientInfo":{"name":"ncp","version":"9.8.7"},"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}},"server":"fx"}}"#,
    ].join("\n");
    insta::assert_snapshot!(apply_filters(text, &home.filters()), @r#"
    trace: server fx: sent {"id":0,"jsonrpc":"2.0","method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/clientCapabilities":{},"io.modelcontextprotocol/clientInfo":{"name":"ncp","version":"[VERSION]"},"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}}
    {"trace":{"sent":{"id":0,"jsonrpc":"2.0","method":"initialize","params":{"capabilities":{},"clientInfo":{"name":"ncp","version":"[VERSION]"},"protocolVersion":"2025-11-25"}},"server":"fx"}}
    {"trace":{"sent":{"id":1,"jsonrpc":"2.0","method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/clientInfo":{"name":"ncp","version":"[VERSION]"},"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}},"server":"fx"}}
    "#);
}

#[test]
fn help_keeps_whitespace_only_lines_and_final_newlines() {
    let home = Home::new("snapshot-help-whitespace");
    let run = ncp_snapshot!(home.filters(), home.ncp(["tool", "--help"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    Tools of a server

    Usage: ncp tool [OPTIONS] <COMMAND>

    Commands:
      list  List the tools of a server
      show  Show one tool: its arguments, output and an example call
      call  Call a tool

    Options:
          --timeout <SECONDS>
              The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection
              
              [default: 300]

      -v, --verbose...
              Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change

          --timings
              Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response

      -q, --quiet...
              Show no hints, no warnings and no progress on stderr. ncp still shows errors

          --no-cache
              Do not read or write the cache: the protocol version and the tool list of each server

          --refresh
              Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh

          --output-format <FORMAT>
              The output format. Default: text

              Possible values:
              - text: Text for people
              - json: One JSON value on stdout, and JSON Lines on stderr

          --config-file <PATH>
              Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question

      -h, --help
              Print help (see a summary with '-h')

      -V, --version
              Print version
    ");
    let bytes = &run.0.stdout;
    assert!(bytes.ends_with(b"\n"));
    assert!(!bytes.ends_with(b"\n\n"));
    assert!(
        bytes
            .split(|byte| *byte == b'\n')
            .any(|line| !line.is_empty() && line.iter().all(|byte| *byte == b' '))
    );
    assert_eq!(
        apply_filters(run.stdout_text(), &home.filters()).as_bytes(),
        bytes
    );
    let text = "Help\n  \n  option  \n\t\n";
    insta::assert_snapshot!(apply_filters(text.into(), &home.filters()), @"
    Help
      
      option  
    	
    ");
    assert_eq!(
        apply_filters(text.into(), &home.filters()).as_bytes(),
        b"Help\n  \n  option  \n\t\n"
    );
}

#[test]
fn legacy_json_rpc_error_and_stdio_timings_have_full_transcripts() {
    let home = Home::new("snapshot-legacy-trace");
    home.configure(&json!({"fx": stdio("legacy")}));
    let run = ncp_snapshot!(home.filters(), home.ncp(["-vv", "--output-format", "json", "server", "show", "fx"]), @r#"
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
    {"debug":{"message":"saved the protocol version: 2025-11-25 (initialize)","server":"fx"}}
    "#);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert!(
        run.stderr()
            .iter()
            .any(|line| line["trace"]["received"]["error"]["code"] == -32601)
    );
    assert_eq!(
        run.stdout()["serverInfo"],
        json!({"name": "fixture-server", "version": "1.0.0"})
    );
    let run = ncp_snapshot!(home.filters(), home.ncp(["--timings", "-q", "tool", "call", "fx", "echo", "text=hi"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    hi

    ----- stderr -----
    timings: total [TIME], cpu [TIME], config [TIME], server start [TIME], responses [TIME] [TIME] [TIME]
    ");
    assert!(run.stderr_text().contains("server start "));
    assert!(run.stderr_text().contains("responses "));
    let run = ncp_snapshot!(home.filters(), home.ncp(["--timings", "-q", "--output-format", "json", "tool", "call", "fx", "echo", "text=hi"]), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"content":[{"text":"hi","type":"text"}],"isError":false}

    ----- stderr -----
    {"timings":{"configMs":[TIME],"cpuMs":[TIME],"responseMs":[[TIME],[TIME],[TIME]],"serverStartMs":[TIME],"totalMs":[TIME]}}
    "#);
    let timings = run
        .stderr()
        .into_iter()
        .find_map(|line| line.get("timings").cloned())
        .unwrap();
    assert!(timings["serverStartMs"].is_number());
    assert_eq!(timings["responseMs"].as_array().unwrap().len(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn http_trace_timings_work_in_both_formats() {
    let fixture = serve_http(Mode::Modern).await.unwrap();
    let home = Home::new("snapshot-http-trace");
    home.configure(&json!({"web": {"type": "http", "url": fixture.url}}));
    let text = ncp_snapshot!(home.filters(), home.ncp(["-vv", "server", "show", "web"]), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    Server: web
    Target: http://[LOCALHOST]/mcp
    Protocol version: 2026-07-28
    Server info: fixture-server 1.0.0
    Capabilities: prompts, resources, tools

    Instructions:
      Fixture server for the tests of ncp.

    ----- stderr -----
    debug: configuration file: [HOME]/config/ncp/mcp.json
    debug: no project file: the search stopped at [HOME]
    debug: server web: no cached protocol version: probe with server/discover
    debug: server web: route to http://[LOCALHOST]/mcp: direct
    trace: server web: sent {"id":0,"jsonrpc":"2.0","method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/clientCapabilities":{},"io.modelcontextprotocol/clientInfo":{"name":"ncp","version":"[VERSION]"},"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}}
    trace: server web: POST http://[LOCALHOST]/mcp [mcp-method: server/discover, mcp-protocol-version: 2026-07-28]: event stream in [TIME]
    trace: server web: received {"id":0,"jsonrpc":"2.0","result":{"_meta":{"io.modelcontextprotocol/serverInfo":{"name":"fixture-server","version":"1.0.0"}},"cacheScope":"private","capabilities":{"prompts":{},"resources":{},"tools":{}},"instructions":"Fixture server for the tests of ncp.","resultType":"complete","supportedVersions":["2024-11-05","2025-03-26","2025-06-18","2025-11-25","2026-07-28"],"ttlMs":0}}
    debug: server web: saved the protocol version: 2026-07-28 (server/discover)
    "#);
    assert!(text.stderr_text().contains("event stream in "));
    let json = ncp_snapshot!(home.filters(), home.ncp(["-vv", "--output-format", "json", "server", "show", "web"]), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"capabilities":{"prompts":{},"resources":{},"tools":{}},"instructions":"Fixture server for the tests of ncp.","protocolVersion":"2026-07-28","server":"web","serverInfo":{"name":"fixture-server","version":"1.0.0"},"target":"http://[LOCALHOST]/mcp"}

    ----- stderr -----
    {"debug":{"message":"configuration file: [HOME]/config/ncp/mcp.json"}}
    {"debug":{"message":"no project file: the search stopped at [HOME]"}}
    {"debug":{"message":"cached protocol version: 2026-07-28 (server/discover)","server":"web"}}
    {"debug":{"message":"route to http://[LOCALHOST]/mcp: direct","server":"web"}}
    {"trace":{"sent":{"id":0,"jsonrpc":"2.0","method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/clientCapabilities":{},"io.modelcontextprotocol/clientInfo":{"name":"ncp","version":"[VERSION]"},"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}},"server":"web"}}
    {"trace":{"http":{"headers":{"mcp-method":"server/discover","mcp-protocol-version":"2026-07-28"},"method":"POST","ms":[TIME],"outcome":"event stream","url":"http://[LOCALHOST]/mcp"},"server":"web"}}
    {"trace":{"received":{"id":0,"jsonrpc":"2.0","result":{"_meta":{"io.modelcontextprotocol/serverInfo":{"name":"fixture-server","version":"1.0.0"}},"cacheScope":"private","capabilities":{"prompts":{},"resources":{},"tools":{}},"instructions":"Fixture server for the tests of ncp.","resultType":"complete","supportedVersions":["2024-11-05","2025-03-26","2025-06-18","2025-11-25","2026-07-28"],"ttlMs":0}},"server":"web"}}
    "#);
    assert!(
        json.stderr()
            .iter()
            .any(|line| line["trace"]["http"]["ms"].is_number())
    );
    assert_eq!(
        json.stdout()["serverInfo"],
        json!({"name": "fixture-server", "version": "1.0.0"})
    );
}

#[test]
fn cache_age_is_filtered_in_real_text_and_json_diagnostics() {
    let home = Home::new("snapshot-cache-age");
    home.configure(&json!({"fx": stdio("cached-public")}));
    assert_eq!(home.run(&["tool", "list", "fx"]).code(), 0);
    let text = ncp_snapshot!(home.filters(), home.ncp(["-v", "tool", "show", "fx", "echo"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    Tool: echo

    Return the text

    Arguments:
      text (string, optional)

    Output:
      not described

    Annotations (hints from the server, not checked):
      read-only: no (default)
      destructive: yes (default)
      idempotent: no (default)
      open-world: yes (default)

    Example:
      ncp tool call fx echo

    ----- stderr -----
    debug: configuration file: [HOME]/config/ncp/mcp.json
    debug: no project file: the search stopped at [HOME]
    debug: server fx: tools from the list cache, [N] s old
    ");
    assert!(text.stderr_text().contains("tools from the list cache, "));
    let json = ncp_snapshot!(home.filters(), home.ncp(["-v", "--output-format", "json", "tool", "show", "fx", "echo"]), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"server":"fx","tool":{"description":"Return the text","inputSchema":{"properties":{"text":{"type":"string"}},"type":"object"},"name":"echo"}}

    ----- stderr -----
    {"debug":{"message":"configuration file: [HOME]/config/ncp/mcp.json"}}
    {"debug":{"message":"no project file: the search stopped at [HOME]"}}
    {"debug":{"message":"tools from the list cache, [N] s old","server":"fx"}}
    "#);
    assert!(json.stderr().iter().any(|line| {
        line["debug"]["message"]
            .as_str()
            .is_some_and(|message| message.starts_with("tools from the list cache, "))
    }));
}

#[test]
fn refused_connection_transcripts_keep_the_native_cause_available() {
    let home = Home::new("snapshot-refused-transcript");
    home.configure(&json!({"dead": {"type": "http", "url": "http://127.0.0.1:9/mcp"}}));
    let filters = [home.filters(), common::connection_refused_filters()].concat();
    let text = ncp_snapshot!(filters, home.ncp(["tool", "list", "dead"]), @"
    exit_code: 3 (failure)
    ----- stderr -----
    error: The handshake with the server `dead` failed in `server/discover`
      cause: Client error: error sending request for url (http://[LOCALHOST]/mcp)
      cause: client error (Connect)
      cause: tcp connect error
      cause: [CONNECTION REFUSED]
    ");
    assert!(text.stderr_text().contains("Connection refused (os error "));
    let json = ncp_snapshot!(
        filters,
        home.ncp(["--output-format", "json", "tool", "list", "dead"]),
        @r#"
    exit_code: 3 (failure)
    ----- stderr -----
    {"error":{"causes":["Client error: error sending request for url (http://[LOCALHOST]/mcp)","client error (Connect)","tcp connect error","[CONNECTION REFUSED]"],"code":"protocol","data":[],"exit":3,"hints":[],"message":"The handshake with the server `dead` failed in `server/discover`","server":"dead"}}
    "#
    );
    assert!(
        json.error()["causes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|cause| {
                cause
                    .as_str()
                    .is_some_and(|text| text.starts_with("Connection refused (os error "))
            })
    );
}
