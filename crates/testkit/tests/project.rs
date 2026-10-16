//! Tests of the project file: the search, the trust, the gate in the
//! lookup of a server name, and `ncp trust`.
//!
//! A stdio entry writes a marker file when it starts, so that a test can
//! check that no process started before the trust.

// Test code can use unwrap: a panic is a test failure.
#![allow(clippy::unwrap_used)]

mod common;

use std::path::PathBuf;

use common::Home;
use ncp_testkit::{Mode, serve_http_with};
use serde_json::{Value, json};

const FIXTURE: &str = env!("CARGO_BIN_EXE_ncp-fixture");

// A stdio entry that writes the marker file when it starts.
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
    let file = home.project_file();
    assert_eq!(
        error["message"],
        format!(
            "The server `fs` is in `{}`, which you did not trust:",
            file.display()
        )
    );
    let data = error["data"][0].as_str().unwrap();
    assert!(data.starts_with(&format!("fs  {FIXTURE} modern")), "{data}");
    assert!(data.contains("env: NCP_FIXTURE_PID_FILE"), "{data}");
    assert_eq!(run.hints(), ["To trust the file, run `ncp trust`"]);
    assert!(!marker.exists());
}

#[test]
fn global_entry_runs_with_no_message_beside_an_untrusted_project() {
    let home = Home::new("p-global");
    home.configure(&json!({"fx": fixture()}));
    let home = home.project("app", &json!({"fs": {"command": "false"}}));
    let run = home.run(&["tool", "list", "fx"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let stderr = run.stderr_text();
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
    assert_eq!(
        run.hints(),
        [
            "To trust the file, run `ncp trust`",
            "To ignore the file and use the entry of the global file, run `ncp trust --ignore`"
        ]
    );
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
        message.ends_with("which changed after you trusted it:"),
        "{message}"
    );
    // After a revert, the trust of the old bytes applies again.
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
    let stderr = run.stderr_text();
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
    let stderr = run.stderr_text();
    let last = stderr.lines().last().unwrap();
    let error: Value = serde_json::from_str(&last[last.find('{').unwrap()..]).unwrap();
    assert_eq!(
        error["error"]["message"],
        format!(
            "The server `fx` is in `{}`, which you ignored",
            file.display()
        )
    );
    assert_eq!(
        common::hints(&error["error"]),
        ["To trust the file, run `ncp trust`"]
    );
    // No question after a change of the file.
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, format!("{text}\n")).unwrap();
    let run = home.run_with_stdin(&["tool", "list", "fx"], "y\n");
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert!(!run.stderr_text().contains("Trust?"));
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
            .starts_with("No server `fx` is configured")
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
    let warning = &run.warnings()[0];
    assert!(
        warning["message"]
            .as_str()
            .unwrap()
            .starts_with("ncp does not use `"),
        "{warning}"
    );
    assert_eq!(warning["causes"], json!(["The file is not a regular file"]));
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
             "scope": "project", "file": file, "trust": "untrusted",
             "cwd": null, "env": [], "sends": []},
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
        assert!(!run.stderr_text().contains("Trust?"));
    }
    assert_eq!(home.run(&["auth", "status"]).stdout()["servers"], json!([]));
    let run = home.run(&["__complete-servers"]);
    assert_eq!(run.stdout_text(), "g\n");
    assert_eq!(run.stderr_text(), "");
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
    let text = run.stdout_text();
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
            "The server `web` names `${{WEB_TOKEN}}` in `{}`, which you did not trust",
            file.display()
        )
    );
    assert_eq!(run.hints(), ["To trust the file, run `ncp trust`"]);
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

// A local port that counts no request: a test checks that nothing
// connects to it.
fn silent_port() -> (std::net::TcpListener, String) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!(
        "http://127.0.0.1:{}/mcp",
        listener.local_addr().unwrap().port()
    );
    (listener, url)
}

fn no_connection(listener: &std::net::TcpListener) -> bool {
    matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
}

#[test]
fn an_untrusted_remote_entry_gets_no_request_from_any_command() {
    let (listener, url) = silent_port();
    let home = Home::new("p-silent")
        .project(
            "app",
            &json!({"web": {"type": "http", "url": url,
                "headers": {"X-Key": "${WEB_TOKEN}"}}}),
        )
        .set("WEB_TOKEN", "t0ken");
    for args in [
        &["tool", "list", "web"][..],
        &["auth", "login", "web", "--no-browser"],
        &["auth", "logout", "web"],
        &["auth", "status", "web"],
        &["server", "show", "web"],
    ] {
        let run = home.run(args);
        assert_eq!(run.code(), 2, "{args:?}: {}", run.describe());
    }
    assert!(no_connection(&listener));
}

#[test]
fn the_shown_entry_is_the_entry_that_runs() {
    let home = Home::new("p-hidden").project(
        "app",
        &json!({"fs": {"type": "stdio", "command": "sh", "args": ["-c", "touch x"],
            "url": "https://x.test/mcp"}}),
    );
    let run = home.run(&["tool", "list", "fs"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    let data = run.error()["data"].to_string();
    assert!(data.contains("fs  sh -c 'touch x'"), "{data}");
    let out = home.run(&["trust", "--show"]).stdout();
    assert_eq!(out["servers"][0]["target"], "sh -c 'touch x'");
}

#[test]
fn an_invalid_entry_shows_its_error() {
    let home = Home::new("p-invalid").project("app", &json!({"bad": {"type": "ws"}}));
    let out = home.run(&["trust", "--show"]).stdout();
    assert_eq!(out["servers"], json!([]));
    assert_eq!(out["invalid"][0]["name"], "bad");
    assert_eq!(out["invalid"][0]["trust"], "untrusted");
    let list = home.run(&["server", "list"]).stdout();
    assert_eq!(
        list["invalid"][0]["error"],
        "The type `ws` is not `stdio`, `http` or `sse`"
    );
}

#[test]
fn a_word_of_the_file_cannot_look_like_two_parts_of_a_line() {
    let home = Home::new("p-spoof").project(
        "app",
        &json!({"fs": {"command": "srv", "cwd": "x   env: SAFE",
            "env": {"A   B -> safe.example": "v"}}}),
    );
    let run = home.run(&["trust", "--show", "--output-format", "text"]);
    let text = run.stdout_text();
    assert!(text.contains("cwd: 'x   env: SAFE'"), "{text}");
    assert!(text.contains("env: 'A   B -> safe.example'"), "{text}");
}

#[test]
fn a_file_of_another_user_is_not_used_and_the_search_stops() {
    let mut home = Home::new("p-owner").project("w", &json!({"fx": fixture()}));
    std::fs::remove_dir_all(home.cwd.join(".git")).unwrap();
    let app = home.cwd.join("app");
    std::fs::create_dir_all(&app).unwrap();
    // A regular file that root owns.
    std::os::unix::fs::symlink("/etc/hosts", app.join(".mcp.json")).unwrap();
    home.cwd = app;
    let run = home.run(&["server", "list"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(
        run.warnings()[0]["causes"],
        json!(["Another user owns the file"])
    );
    assert_eq!(run.stdout()["servers"], json!([]));
}

#[test]
fn a_git_file_stops_the_search() {
    let mut home = Home::new("p-gitfile").project("w", &json!({"fx": fixture()}));
    let app = home.cwd.join("app");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(app.join(".git"), "gitdir: /elsewhere\n").unwrap();
    home.cwd = app;
    assert_eq!(home.run(&["server", "list"]).stdout()["servers"], json!([]));
}

#[test]
fn server_list_text_names_the_project_file_and_marks_its_entries() {
    let home = Home::new("p-text");
    home.configure(&json!({"g": fixture()}));
    let home = home.project("app", &json!({"fx": fixture()}));
    let run = home.run(&["server", "list", "--output-format", "text"]);
    let text = run.stdout_text();
    let file = home.project_file();
    assert!(
        text.starts_with(&format!("Project file: {} (untrusted)\n", file.display())),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|line| line.starts_with("fx ") && line.ends_with("project, untrusted")),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|line| line.starts_with("g ") && line.ends_with(" modern")),
        "{text}"
    );
}

#[test]
fn verbose_output_names_the_project_file_or_where_the_search_stopped() {
    let home = Home::new("p-verbose").project("app", &json!({"fx": fixture()}));
    let file = home.project_file();
    let run = home.run(&["-v", "server", "list"]);
    assert!(
        messages(&run, "debug").contains(&format!("project file: {} (untrusted)", file.display())),
        "{}",
        run.describe()
    );
    let mut home = home;
    home.cwd = home.root.clone();
    let run = home.run(&["-v", "server", "list"]);
    let root = std::fs::canonicalize(&home.root).unwrap();
    assert!(
        messages(&run, "debug").contains(&format!(
            "no project file: the search stopped at {}",
            root.display()
        )),
        "{}",
        run.describe()
    );
}

#[test]
fn untrust_keeps_an_ignored_record() {
    let home = Home::new("p-untrust").project("app", &json!({"fx": fixture()}));
    assert_eq!(home.run(&["trust", "--ignore"]).code(), 0);
    assert_eq!(
        home.run(&["trust", "--untrust"]).stdout()["trust"],
        "ignored"
    );
}

#[test]
fn delete_unused_stops_when_an_untrusted_entry_names_the_secret() {
    let home = Home::new("p-unused").project(
        "app",
        &json!({"web": {"type": "http", "url": "https://mcp.example.com/mcp",
            "headers": {"Authorization": "Bearer ${WEB_TOKEN}"}}}),
    );
    home.store_in_file("secret/https://mcp.example.com/WEB_TOKEN", "v");
    let run = home.run(&["secret", "delete", "WEB_TOKEN", "--unused"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    let message = run.error()["message"].as_str().unwrap().to_string();
    assert!(
        message.ends_with("which you did not trust. ncp cannot know which items it uses."),
        "{message}"
    );
    let run = home.run(&["secret", "list"]);
    assert!(
        messages(&run, "hint")
            .iter()
            .any(|hint| hint.contains("A project in another folder can still use the item")),
        "{}",
        run.describe()
    );
}

#[test]
fn a_fifo_as_project_file_does_not_stop_the_command() {
    let mut home = Home::new("p-fifo");
    let dir = home.root.join("app");
    std::fs::create_dir_all(&dir).unwrap();
    let status = std::process::Command::new("mkfifo")
        .arg(dir.join(".mcp.json"))
        .status()
        .unwrap();
    assert!(status.success());
    home.cwd = dir;
    let run = home.run(&["server", "list"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(
        run.warnings()[0]["causes"],
        json!(["The file is not a regular file"])
    );
}

#[test]
fn an_entry_that_the_loader_refuses_shows_its_target_and_the_problem() {
    let home = Home::new("p-problem").project(
        "app",
        &json!({"fs": {"command": "sh", "args": [1, "-c", "evil"]}}),
    );
    let out = home.run(&["trust", "--show"]).stdout();
    assert_eq!(out["servers"], json!([]));
    assert_eq!(out["invalid"][0]["target"], "sh -c evil");
    assert_eq!(
        out["invalid"][0]["error"],
        "The field `args[0]` must be a string"
    );
    assert_eq!(out["invalid"][0]["causes"], json!([]));
}

#[test]
fn trust_show_gives_cwd_env_names_and_each_variable_with_its_host() {
    let home = Home::new("p-json").project(
        "app",
        &json!({
            "fs": {"command": "node", "cwd": "x", "env": {"LD_PRELOAD": "lib.so"}},
            "web": {"type": "http", "url": "https://mcp.example.com/mcp",
                    "headers": {"Authorization": "Bearer ${TOKEN}"}}
        }),
    );
    let out = home.run(&["trust", "--show"]).stdout();
    assert_eq!(out["servers"][0]["cwd"], "x");
    assert_eq!(out["servers"][0]["env"], json!(["LD_PRELOAD"]));
    assert_eq!(
        out["servers"][1]["sends"],
        json!([{"name": "TOKEN", "host": "mcp.example.com"}])
    );
}

#[test]
fn a_url_shows_as_the_client_sends_it() {
    let home = Home::new("p-url").project(
        "app",
        &json!({"web": {"type": "http",
            "url": "https://mcp.sentry.dev\t\t.evil.example/x   TOKEN -> mcp.sentry.dev"}}),
    );
    let run = home.run(&["trust", "--show", "--output-format", "text"]);
    let text = run.stdout_text();
    assert!(
        text.contains(
            "https://mcp.sentry.dev.evil.example/x%20%20%20TOKEN%20-%3E%20mcp.sentry.dev"
        ),
        "{text}"
    );
}

#[test]
fn delete_unused_stops_when_an_untrusted_entry_is_invalid() {
    let home = Home::new("p-unused-invalid").project(
        "app",
        &json!({"bad.name": {"command": "x", "env": {"Z": "${ZZ}"}}}),
    );
    home.store_in_file("secret/ZZ", "v");
    let run = home.run(&["secret", "delete", "ZZ", "--unused"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
}
