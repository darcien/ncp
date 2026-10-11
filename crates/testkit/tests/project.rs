//! Tests of the project file: the search, the trust, the gate in the
//! lookup of a server name, and `ncp trust`.
//!
//! A stdio entry writes a marker file when it starts, so that a test can
//! check that no process started before the trust.

// The whole file is test code, where the agent rules permit unwrap.
#![allow(clippy::unwrap_used)]

mod common;

use std::path::PathBuf;

use common::Home;
use ncp_testkit::{Mode, serve_http_with};
use serde_json::{Value, json};

const FIXTURE: &str = env!("CARGO_BIN_EXE_ncp-fixture");

/// A stdio entry that writes the marker file when it starts.
fn marked(home: &Home) -> (Value, PathBuf) {
    let marker = home.root.join("started");
    let entry = json!({"command": FIXTURE, "args": ["modern"],
        "env": {"NCP_FIXTURE_PID_FILE": marker.display().to_string()}});
    (entry, marker)
}

fn fixture() -> Value {
    json!({"command": FIXTURE, "args": ["modern"]})
}

fn messages(run: &common::Run, kind: &str) -> Vec<String> {
    run.stderr()
        .iter()
        .filter_map(|line| line[kind]["message"].as_str().map(str::to_string))
        .collect()
}

#[test]
fn untrusted_project_entry_stops_with_its_entries_and_starts_nothing() {
    let home = Home::new("p-untrusted");
    let (entry, marker) = marked(&home);
    let home = home.project("app", &json!({"fs": entry}));
    let run = home.run(&["tool", "list", "fs"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    let error = run.error();
    let message = error["message"].as_str().unwrap();
    let file = home.project_file();
    assert!(
        message.starts_with(&format!(
            "fs is in {}, which is not trusted:\n",
            file.display()
        )),
        "{message}"
    );
    assert!(
        message.contains(&format!("fs  {FIXTURE} modern")),
        "{message}"
    );
    assert!(message.contains("env: NCP_FIXTURE_PID_FILE"), "{message}");
    assert_eq!(error["hint"], "ncp trust");
    assert!(!marker.exists());
}

#[test]
fn global_entry_runs_with_no_message_beside_an_untrusted_project() {
    let home = Home::new("p-global");
    home.configure(&json!({"fx": fixture()}));
    let home = home.project("app", &json!({"fs": {"command": "false"}}));
    let run = home.run(&["tool", "list", "fx"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let stderr = String::from_utf8_lossy(&run.0.stderr).to_string();
    assert!(!stderr.contains(".mcp.json"), "{stderr}");
    assert_eq!(messages(&run, "warning"), Vec::<String>::new(), "{stderr}");
}

#[test]
fn name_in_both_files_stops_and_names_the_ignore_option() {
    let home = Home::new("p-both");
    home.configure(&json!({"fx": fixture()}));
    let home = home.project("app", &json!({"fx": {"command": "false"}}));
    let run = home.run(&["tool", "list", "fx"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert_eq!(run.error()["hint"], "ncp trust");
    assert_eq!(messages(&run, "hint"), ["ncp trust --ignore"]);
    // With the file ignored, the global entry runs.
    assert_eq!(home.run(&["trust", "--ignore"]).code(), 0);
    let run = home.run(&["tool", "list", "fx"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
}

#[test]
fn trust_works_with_no_tty_and_then_the_entry_runs() {
    let home = Home::new("p-trust");
    let (entry, marker) = marked(&home);
    let home = home.project("app", &json!({"fs": entry}));
    let run = home.run(&["trust"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let file = home.project_file();
    let out = run.stdout();
    assert_eq!(out["file"], json!(file));
    assert_eq!(out["trust"], "trusted");
    assert_eq!(out["servers"][0]["name"], "fs");
    assert_eq!(out["servers"][0]["scope"], "project");
    assert_eq!(out["servers"][0]["trust"], "trusted");
    // A second trust exits 0 too.
    assert_eq!(home.run(&["trust"]).code(), 0);
    let run = home.run(&["tool", "list", "fs"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert!(marker.exists());
}

#[test]
fn a_changed_byte_asks_again_and_says_that_the_file_changed() {
    let home = Home::new("p-changed").project("app", &json!({"fx": fixture()}));
    assert_eq!(home.run(&["trust"]).code(), 0);
    let file = home.project_file();
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, format!("{text} ")).unwrap();
    let run = home.run(&["tool", "list", "fx"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    let message = run.error()["message"].as_str().unwrap().to_string();
    assert!(
        message.contains("which changed since you trusted it"),
        "{message}"
    );
    // The old bytes are trusted again after a revert.
    std::fs::write(&file, text).unwrap();
    assert_eq!(home.run(&["tool", "list", "fx"]).code(), 0);
}

#[test]
fn yes_at_the_question_trusts_the_file_and_runs_the_entry() {
    let home = Home::new("p-yes")
        .project("app", &json!({"fx": fixture()}))
        .set("NCP_TEST_TTY", "1");
    let run = home.run_with_stdin(&["tool", "list", "fx"], "y\n");
    assert_eq!(run.code(), 0, "{}", run.describe());
    let stderr = String::from_utf8_lossy(&run.0.stderr).to_string();
    assert!(
        stderr.contains("is not trusted. It has these servers:"),
        "{stderr}"
    );
    assert!(stderr.contains("Trust? [y/N]"), "{stderr}");
    let run = home.unset("NCP_TEST_TTY").run(&["trust", "--show"]);
    assert_eq!(run.stdout()["trust"], "trusted");
}

#[test]
fn no_at_the_question_ignores_the_file_until_ncp_trust() {
    let home = Home::new("p-no")
        .project("app", &json!({"fx": fixture()}))
        .set("NCP_TEST_TTY", "1");
    let run = home.run_with_stdin(&["tool", "list", "fx"], "n\n");
    assert_eq!(run.code(), 2, "{}", run.describe());
    let file = home.project_file();
    // The question is text on stderr, before the JSON error. A pipe does
    // not echo the answer, so the error follows the question on its line.
    let stderr = String::from_utf8_lossy(&run.0.stderr).to_string();
    let last = stderr.lines().last().unwrap();
    let error: Value = serde_json::from_str(&last[last.find('{').unwrap()..]).unwrap();
    assert_eq!(
        error["error"]["message"],
        format!("fx is in {}, which you ignored", file.display())
    );
    assert_eq!(error["error"]["hint"], "ncp trust");
    // No question after a change of the file.
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, format!("{text}\n")).unwrap();
    let run = home.run_with_stdin(&["tool", "list", "fx"], "y\n");
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert!(!String::from_utf8_lossy(&run.0.stderr).contains("Trust?"));
    // ncp trust removes the ignored record.
    assert_eq!(home.run(&["trust"]).code(), 0);
    assert_eq!(home.run(&["tool", "list", "fx"]).code(), 0);
}

#[test]
fn no_answer_at_the_question_writes_no_record() {
    let home = Home::new("p-end")
        .project("app", &json!({"fx": fixture()}))
        .set("NCP_TEST_TTY", "1");
    let run = home.run_with_stdin(&["tool", "list", "fx"], "");
    assert_eq!(run.code(), 2, "{}", run.describe());
    let home = home.unset("NCP_TEST_TTY");
    assert_eq!(
        home.run(&["trust", "--show"]).stdout()["trust"],
        "untrusted"
    );
}

#[test]
fn show_changes_nothing_and_untrust_and_ignore_write_their_records() {
    let home = Home::new("p-options").project("app", &json!({"fx": fixture()}));
    let state = |home: &Home| home.run(&["trust", "--show"]).stdout()["trust"].clone();
    assert_eq!(state(&home), "untrusted");
    assert_eq!(state(&home), "untrusted");
    assert_eq!(home.run(&["trust"]).stdout()["trust"], "trusted");
    assert_eq!(
        home.run(&["trust", "--untrust"]).stdout()["trust"],
        "untrusted"
    );
    assert_eq!(state(&home), "untrusted");
    assert_eq!(
        home.run(&["trust", "--ignore"]).stdout()["trust"],
        "ignored"
    );
    assert_eq!(state(&home), "ignored");
    let run = home.run(&["trust", "--show", "--ignore"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
}

#[test]
fn the_same_bytes_at_another_path_ask_again() {
    let home = Home::new("p-paths").project("one", &json!({"fx": fixture()}));
    assert_eq!(home.run(&["trust"]).code(), 0);
    let home = home.project("two", &json!({"fx": fixture()}));
    assert_eq!(home.run(&["tool", "list", "fx"]).code(), 2);
}

#[test]
fn one_file_linked_into_two_folders_asks_in_each_folder() {
    let mut home = Home::new("p-link").project("shared", &json!({"fx": fixture()}));
    let target = home.cwd.join(".mcp.json");
    for folder in ["a", "b"] {
        let dir = home.root.join(folder);
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::os::unix::fs::symlink(&target, dir.join(".mcp.json")).unwrap();
    }
    home.cwd = home.root.join("a");
    assert_eq!(home.run(&["tool", "list", "fx"]).code(), 2);
    assert_eq!(home.run(&["trust"]).code(), 0);
    assert_eq!(home.run(&["tool", "list", "fx"]).code(), 0);
    home.cwd = home.root.join("b");
    assert_eq!(home.run(&["tool", "list", "fx"]).code(), 2);
}

#[test]
fn search_finds_the_file_in_a_parent_and_stops_at_the_git_root() {
    let mut home = Home::new("p-search").project("app", &json!({"fx": fixture()}));
    assert_eq!(home.run(&["trust"]).code(), 0);
    let sub = home.cwd.join("src/deep");
    std::fs::create_dir_all(&sub).unwrap();
    home.cwd.clone_from(&sub);
    assert_eq!(home.run(&["tool", "list", "fx"]).code(), 0);
    // A repository below the project has its own root.
    let inner = home.root.join("app/vendor/lib");
    std::fs::create_dir_all(inner.join(".git")).unwrap();
    home.cwd = inner;
    let run = home.run(&["tool", "list", "fx"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert!(
        run.error()["message"]
            .as_str()
            .unwrap()
            .starts_with("no server")
    );
}

#[test]
fn a_file_that_does_not_parse_gives_a_warning_and_the_global_file_works() {
    let home = Home::new("p-bad");
    home.configure(&json!({"fx": fixture()}));
    let home = home.project("app", &json!({}));
    std::fs::write(home.project_file(), "{ not json").unwrap();
    let run = home.run(&["tool", "list", "fx"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let warnings = messages(&run, "warning");
    assert_eq!(warnings.len(), 1, "{}", run.describe());
    assert!(warnings[0].starts_with("ncp does not use "), "{warnings:?}");
    assert_eq!(home.run(&["trust"]).code(), 2);
}

#[test]
fn a_project_file_that_is_not_a_regular_file_is_not_used() {
    let mut home = Home::new("p-dir");
    let dir = home.root.join("app");
    std::fs::create_dir_all(dir.join(".mcp.json")).unwrap();
    home.cwd = dir;
    let run = home.run(&["server", "list"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let warnings = messages(&run, "warning");
    assert!(
        warnings[0].ends_with("it is not a regular file"),
        "{warnings:?}"
    );
}

#[test]
fn config_file_option_means_no_search() {
    let home = Home::new("p-named");
    home.configure(&json!({"fx": fixture()}));
    let config = home.config_path().display().to_string();
    let home = home.project("app", &json!({"fs": {"command": "false"}}));
    let run = home.run(&["--config-file", &config, "server", "list"]);
    assert_eq!(run.stdout()["servers"].as_array().unwrap().len(), 1);
    let run = home.run(&["--config-file", &config, "trust"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
}

#[test]
fn a_trusted_project_entry_replaces_the_global_entry() {
    let home = Home::new("p-merge");
    home.configure(&json!({"fx": {"command": "false"}, "g": {"command": "false"}}));
    let home = home.project("app", &json!({"fx": fixture()}));
    let list = home.run(&["server", "list"]).stdout();
    let file = home.project_file();
    assert_eq!(
        list["servers"],
        json!([
            {"name": "fx", "transport": "stdio", "target": "false", "scope": "global",
             "file": home.config_path()},
            {"name": "g", "transport": "stdio", "target": "false", "scope": "global",
             "file": home.config_path()},
            {"name": "fx", "transport": "stdio", "target": format!("{FIXTURE} modern"),
             "scope": "project", "file": file, "trust": "untrusted"},
        ])
    );
    assert_eq!(home.run(&["trust"]).code(), 0);
    let list = home.run(&["server", "list"]).stdout();
    assert_eq!(list["servers"][0]["scope"], "project");
    assert_eq!(list["servers"][0]["trust"], "trusted");
    assert_eq!(list["servers"][1]["scope"], "global");
    assert_eq!(home.run(&["tool", "list", "fx"]).code(), 0);
}

#[test]
fn listing_commands_and_completion_never_ask_and_skip_an_untrusted_file() {
    let home = Home::new("p-quiet");
    home.configure(&json!({"g": fixture()}));
    let home = home
        .project(
            "app",
            &json!({"fs": {"type": "http", "url": "https://x.test/mcp",
            "headers": {"Authorization": "Bearer ${TOKEN}"}}}),
        )
        .set("NCP_TEST_TTY", "1");
    for args in [
        &["server", "list"][..],
        &["auth", "status"],
        &["secret", "list"],
    ] {
        let run = home.run_with_stdin(args, "y\n");
        assert_eq!(run.code(), 0, "{args:?}: {}", run.describe());
        assert!(!String::from_utf8_lossy(&run.0.stderr).contains("Trust?"));
    }
    assert_eq!(home.run(&["auth", "status"]).stdout()["servers"], json!([]));
    let run = home.run(&["__complete-servers"]);
    assert_eq!(String::from_utf8_lossy(&run.0.stdout), "g\n");
    assert_eq!(String::from_utf8_lossy(&run.0.stderr), "");
    // Did-you-mean does not offer the name of the untrusted file.
    let run = home.unset("NCP_TEST_TTY").run(&["tool", "list", "f"]);
    assert!(!run.error()["message"].as_str().unwrap().contains("fs"));
}

#[test]
fn shown_entries_escape_and_give_no_values() {
    let home = Home::new("p-shown").project(
        "app",
        &json!({
            "fs": {"command": "srv", "args": ["a\u{1b}[2Jb", "c\u{202e}d"],
                   "env": {"SECRET_ENV": "literal-value"}},
            "web": {"type": "http", "url": "https://mcp.example.com/mcp",
                    "headers": {"Authorization": "Bearer ${WEB_TOKEN}", "X-Key": "lit-header"}}
        }),
    );
    let run = home.run(&["trust", "--show", "--output-format", "text"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let text = String::from_utf8_lossy(&run.0.stdout).to_string();
    assert!(text.contains("\\u{1b}"), "{text}");
    assert!(text.contains("\\u{202e}"), "{text}");
    assert!(!text.contains('\u{1b}'), "{text}");
    assert!(text.contains("env: SECRET_ENV"), "{text}");
    assert!(!text.contains("literal-value"), "{text}");
    assert!(!text.contains("lit-header"), "{text}");
    assert!(text.contains("WEB_TOKEN -> mcp.example.com"), "{text}");
}

#[test]
fn secret_set_names_the_untrusted_file_that_uses_the_name() {
    let home = Home::new("p-secret").project(
        "app",
        &json!({"web": {"type": "http", "url": "https://mcp.example.com/mcp",
            "headers": {"Authorization": "Bearer ${WEB_TOKEN}"}}}),
    );
    let run = home.run_with_stdin(
        &["secret", "set", "WEB_TOKEN", "--credential-store", "file"],
        "v\n",
    );
    assert_eq!(run.code(), 2, "{}", run.describe());
    let file = home.project_file();
    assert_eq!(
        run.error()["message"],
        format!(
            "server web names ${{WEB_TOKEN}} in {}, which is not trusted",
            file.display()
        )
    );
    assert_eq!(run.error()["hint"], "ncp trust");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_trusted_project_header_gets_its_value_from_the_environment_and_the_store() {
    let fixture = serve_http_with(Mode::Modern, Some("t0ken")).await.unwrap();
    let servers = json!({"web": {"type": "http", "url": fixture.url,
        "headers": {"Authorization": "Bearer ${WEB_TOKEN}"}}});
    let home = Home::new("p-expand-env")
        .project("app", &servers)
        .set("WEB_TOKEN", "t0ken");
    // Before the trust, nothing goes out.
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert_eq!(home.run(&["trust"]).code(), 0);
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 0, "{}", run.describe());

    let home = Home::new("p-expand-store").project("app", &servers);
    assert_eq!(home.run(&["trust"]).code(), 0);
    let run = home.run_with_stdin(
        &["secret", "set", "WEB_TOKEN", "--credential-store", "file"],
        "t0ken",
    );
    assert_eq!(run.code(), 0, "{}", run.describe());
    let run = home.run(&["tool", "list", "web"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
}
