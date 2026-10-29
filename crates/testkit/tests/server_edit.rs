//! Tests of `ncp server add` and `ncp server remove`: the entry, the
//! target file and its trust, the write, the secret, and the tokens.

// Test code can use unwrap: a panic is a test failure.
#![allow(clippy::unwrap_used)]

mod common;

use std::{os::unix::fs::PermissionsExt, path::PathBuf};

use common::Home;
use ncp_testkit::{Mode, serve_http_with};
use serde_json::{Value, json};

const FIXTURE: &str = env!("CARGO_BIN_EXE_ncp-fixture");

fn servers_of(path: &std::path::Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap();
    let root: Value = jsonc_value(&text);
    root["mcpServers"].clone()
}

// The JSON value of a file with comments: the tests read it with the
// parser of `ncp`.
fn jsonc_value(text: &str) -> Value {
    Value::Object(ncp::config::parse_root(text).unwrap())
}

// A directory in the home directory with a `.git` directory and no project file.
fn in_repository(mut home: Home) -> Home {
    let dir = home.root.join("app");
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    home.cwd = dir;
    home
}

fn new_project_file(home: &Home) -> PathBuf {
    std::fs::canonicalize(&home.cwd).unwrap().join(".mcp.json")
}

// `ncp server add NAME URL --global` with more arguments.
fn add_global(home: &Home, name: &str, url: &str, more: &[&str]) -> common::Run {
    let args: Vec<&str> = ["server", "add", name, url, "--global"]
        .into_iter()
        .chain(more.iter().copied())
        .collect();
    home.run(&args)
}

fn add_global_with_stdin(
    home: &Home,
    name: &str,
    url: &str,
    more: &[&str],
    stdin: &str,
) -> common::Run {
    let args: Vec<&str> = ["server", "add", name, url, "--global"]
        .into_iter()
        .chain(more.iter().copied())
        .collect();
    home.run_with_stdin(&args, stdin)
}

const WEB: &str = "https://web.example.com/mcp";

#[test]
fn add_creates_a_project_file_in_the_current_dir_and_trusts_it() {
    let home = in_repository(Home::new("a-new"));
    let run = home.run(&["server", "add", "docs", "https://docs.example.com/mcp"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let file = new_project_file(&home);
    let out = run.stdout();
    assert_eq!(out["file"], json!(file));
    assert_eq!(out["scope"], "project");
    assert_eq!(out["created"], true);
    assert_eq!(out["changed"], true);
    assert_eq!(out["transport"], "http");
    assert_eq!(
        out["nextSteps"],
        json!([
            {"command": "ncp auth login docs", "when": "the server needs a login"},
            {"command": "ncp server show docs"}
        ])
    );
    assert_eq!(
        servers_of(&file),
        json!({"docs": {"type": "http", "url": "https://docs.example.com/mcp"}})
    );
    let list = home.run(&["server", "list"]).stdout();
    assert_eq!(list["servers"][0]["trust"], "trusted");
}

#[test]
fn add_in_the_home_dir_and_with_global_writes_the_global_file() {
    let home = Home::new("a-home");
    let run = home.run(&["server", "add", "docs", "https://docs.example.com/mcp"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(run.stdout()["scope"], "global");
    assert!(servers_of(&home.config_path())["docs"].is_object());
    let mode = std::fs::metadata(home.config_path())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
    let home = in_repository(home);
    let run = home.run(&[
        "server",
        "add",
        "two",
        "https://two.example.com/mcp",
        "--global",
    ]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert!(servers_of(&home.config_path())["two"].is_object());
    assert!(!new_project_file(&home).exists());
}

#[test]
fn add_keeps_every_other_byte_and_the_mode_of_the_file() {
    let home = Home::new("a-bytes");
    let path = home.config_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let old = "{\n    // mine\n    \"mcpServers\": {\n        \"a\": {\"command\": \"x\", \"odd\": 1},\n    },\n    \"theme\": \"dark\"\n}\n";
    std::fs::write(&path, old).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let run = home.run(&[
        "server",
        "add",
        "web",
        "https://web.example.com/mcp",
        "--global",
    ]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let new = std::fs::read_to_string(&path).unwrap();
    assert!(new.starts_with("{\n    // mine\n    \"mcpServers\": {\n        \"a\": {\"command\": \"x\", \"odd\": 1},\n"), "{new}");
    assert!(new.ends_with("    \"theme\": \"dark\"\n}\n"), "{new}");
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o644);
}

#[test]
fn add_writes_through_a_symlink_and_keeps_the_link() {
    let home = Home::new("a-link");
    let real = home.root.join("dotfiles/mcp.json");
    std::fs::create_dir_all(real.parent().unwrap()).unwrap();
    std::fs::write(&real, "{}").unwrap();
    let path = home.config_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&real, &path).unwrap();
    let run = home.run(&[
        "server",
        "add",
        "web",
        "https://web.example.com/mcp",
        "--global",
    ]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert!(
        std::fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(servers_of(&real)["web"].is_object());
}

#[test]
fn the_same_entry_again_changes_nothing() {
    let home = Home::new("a-again");
    assert_eq!(add_global(&home, "web", WEB, &[]).code(), 0);
    let run = add_global(&home, "web", WEB, &[]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(run.stdout()["changed"], false);
}

#[test]
fn another_entry_of_the_name_is_an_error() {
    let home = Home::new("a-other");
    assert_eq!(add_global(&home, "web", WEB, &[]).code(), 0);
    let run = add_global(&home, "web", "https://other.example.com/mcp", &[]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert_eq!(
        run.error()["hints"],
        json!([{"message": "To remove the entry, run `ncp server remove web --global`"}])
    );
}

#[test]
fn a_duplicate_key_of_the_name_is_an_error() {
    let home = Home::new("a-duplicate");
    let path = home.config_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let entry = format!(r#"{{"type": "http", "url": "{WEB}"}}"#);
    std::fs::write(
        &path,
        format!(r#"{{"mcpServers": {{"web": {entry}, "web": {entry}}}}}"#),
    )
    .unwrap();
    let run = add_global(&home, "web", WEB, &[]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    let error = run.error();
    assert!(
        error["causes"][0]
            .as_str()
            .unwrap()
            .contains("more than one time"),
        "{error}"
    );
    assert_eq!(
        run.hints(),
        ["Remove the copies of the key `web` from the file. Then run the command again."]
    );
}

#[test]
fn a_literal_credential_header_is_an_error_that_shows_no_value() {
    let home = Home::new("a-literal");
    let run = home.run(&[
        "server",
        "add",
        "web",
        "https://web.example.com/mcp",
        "--global",
        "--header",
        "Authorization: Bearer s3cr3t-value",
    ]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert!(
        !run.stderr_text().contains("s3cr3t-value"),
        "{}",
        run.describe()
    );
    assert_eq!(
        run.error()["hints"],
        json!([{"message": "Give the header as `--header 'Authorization: Bearer ${NAME}' --secret NAME`"}])
    );
    assert!(!home.config_path().exists());
}

#[test]
fn a_usage_error_writes_no_file() {
    let home = Home::new("a-usage");
    for args in [
        &["server", "add", "web"][..],
        &[
            "server",
            "add",
            "web",
            "https://w.example.com/mcp",
            "--",
            "srv",
        ],
        &[
            "server",
            "add",
            "web",
            "https://w.example.com/mcp",
            "--credential-store",
            "file",
        ],
        &[
            "server",
            "add",
            "web",
            "https://w.example.com/mcp",
            "--secret",
            "KEY",
        ],
        &[
            "server",
            "add",
            "web",
            "https://w.example.com/mcp",
            "--env",
            "K",
        ],
    ] {
        let run = home.run_with_stdin(args, "value");
        assert_eq!(run.code(), 2, "{args:?}: {}", run.describe());
        assert!(!home.config_path().exists(), "{args:?}");
    }
    let config = home.config_path().display().to_string();
    let run = home.run(&[
        "--config-file",
        &config,
        "server",
        "add",
        "w",
        "https://w.example.com/mcp",
        "--global",
    ]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert!(!home.config_path().exists());
}

#[test]
fn an_untrusted_project_file_is_not_changed_with_no_tty() {
    let home = Home::new("a-untrusted").project("app", &json!({"fs": {"command": "x"}}));
    let file = home.project_file();
    let before = std::fs::read_to_string(&file).unwrap();
    let run = home.run(&["server", "add", "web", WEB]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    let error = run.error();
    assert_eq!(
        error["message"],
        format!(
            "ncp changes only a trusted project file, and you did not trust `{}`:",
            file.display()
        )
    );
    assert!(
        error["data"][0].as_str().unwrap().starts_with("fs  x"),
        "{error}"
    );
    assert_eq!(error["hints"][0]["command"], "ncp trust --show");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
}

#[test]
fn a_no_at_the_question_ignores_the_file_and_changes_nothing() {
    let home = Home::new("a-no")
        .project("app", &json!({"fs": {"command": "x"}}))
        .set("NCP_TEST_TTY", "1");
    let file = home.project_file();
    let before = std::fs::read_to_string(&file).unwrap();
    let run = home.run_with_stdin(&["server", "add", "web", WEB], "n\n");
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
    let home = home.unset("NCP_TEST_TTY");
    let run = home.run(&["server", "add", "web", WEB]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert!(
        run.error()["message"]
            .as_str()
            .unwrap()
            .starts_with("You ignored `"),
        "{}",
        run.describe()
    );
    assert_eq!(
        run.hints(),
        ["To change the global file, run the command again with `--global`"]
    );
}

#[test]
fn a_new_file_at_the_path_of_an_ignored_file_is_trusted() {
    let home = Home::new("a-reborn").project("app", &json!({"fs": {"command": "x"}}));
    assert_eq!(home.run(&["trust", "--ignore"]).code(), 0);
    std::fs::remove_file(home.project_file()).unwrap();
    let run = home.run(&["server", "add", "web", WEB]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(run.stdout()["created"], true);
    assert_eq!(home.run(&["trust", "--show"]).stdout()["trust"], "trusted");
}

#[test]
fn a_change_of_a_project_file_after_the_trust_stops_the_write() {
    let home = Home::new("a-race-project")
        .project("app", &json!({"fs": {"command": "x"}}))
        .set("NCP_TEST_CHANGE_BEFORE_RENAME", "1");
    assert_eq!(home.run(&["trust"]).code(), 0);
    let run = home.run(&["server", "add", "web", WEB]);
    assert_eq!(run.code(), 3, "{}", run.describe());
    let text = std::fs::read_to_string(home.project_file()).unwrap();
    assert!(!text.contains("web"), "{text}");
    // `ncp` does not trust the changed bytes.
    let home = home.unset("NCP_TEST_CHANGE_BEFORE_RENAME");
    assert_eq!(
        home.run(&["trust", "--show"]).stdout()["trust"],
        "untrusted"
    );
}

#[test]
fn a_yes_at_the_question_trusts_and_adds() {
    let home = Home::new("a-yes")
        .project("app", &json!({"fs": {"command": "x"}}))
        .set("NCP_TEST_TTY", "1");
    let run = home.run_with_stdin(
        &["server", "add", "web", "https://w.example.com/mcp"],
        "y\n",
    );
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert!(servers_of(&home.project_file())["web"].is_object());
    let home = home.unset("NCP_TEST_TTY");
    assert_eq!(home.run(&["trust", "--show"]).stdout()["trust"], "trusted");
}

#[test]
fn a_project_file_that_ncp_does_not_use_is_not_changed() {
    let mut home = Home::new("a-unusable");
    let dir = home.root.join("app");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(".mcp.json"), "{ not json").unwrap();
    home.cwd = dir.clone();
    let run = home.run(&["server", "add", "web", "https://w.example.com/mcp"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    let causes = run.error()["causes"].clone();
    assert_eq!(causes[0], "The file is not valid JSONC", "{causes}");
    assert_eq!(causes.as_array().unwrap().len(), 2, "{causes}");
    assert_eq!(
        run.hints(),
        ["To change the global file, run the command again with `--global`"]
    );
    assert_eq!(
        std::fs::read_to_string(dir.join(".mcp.json")).unwrap(),
        "{ not json"
    );
}

#[test]
fn a_change_of_the_file_before_the_rename_stops_the_write() {
    let home = Home::new("a-race").set("NCP_TEST_CHANGE_BEFORE_RENAME", "1");
    let path = home.config_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "{}").unwrap();
    let run = home.run(&[
        "server",
        "add",
        "web",
        "https://w.example.com/mcp",
        "--global",
    ]);
    assert_eq!(run.code(), 3, "{}", run.describe());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{} ");
    let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn a_read_only_file_is_not_changed() {
    let home = Home::new("a-readonly");
    let path = home.config_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "{}").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
    let run = home.run(&[
        "server",
        "add",
        "web",
        "https://w.example.com/mcp",
        "--global",
    ]);
    assert_eq!(run.code(), 3, "{}", run.describe());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}");
}

#[test]
fn two_adds_at_the_same_time_keep_both_entries() {
    let home = Home::new("a-parallel");
    std::thread::scope(|scope| {
        let runs: Vec<_> = (0..4)
            .map(|index| {
                let home = &home;
                scope.spawn(move || {
                    let name = format!("s{index}");
                    let url = format!("https://s{index}.example.com/mcp");
                    home.run(&["server", "add", &name, &url, "--global"]).code()
                })
            })
            .collect();
        for run in runs {
            assert_eq!(run.join().unwrap(), 0);
        }
    });
    let servers = servers_of(&home.config_path());
    assert_eq!(servers.as_object().unwrap().len(), 4);
}

#[test]
fn the_same_add_at_the_same_time_passes_each_time() {
    let home = Home::new("a-parallel-same");
    std::thread::scope(|scope| {
        let runs: Vec<_> = (0..4)
            .map(|_| scope.spawn(|| add_global(&home, "web", WEB, &[]).code()))
            .collect();
        for run in runs {
            assert_eq!(run.join().unwrap(), 0);
        }
    });
}

#[test]
fn a_symlink_to_a_missing_file_stays() {
    let home = Home::new("a-dangling");
    let path = home.config_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(home.root.join("missing.json"), &path).unwrap();
    let run = add_global(&home, "web", WEB, &[]);
    assert_eq!(run.code(), 3, "{}", run.describe());
    assert!(
        std::fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn an_invalid_flag_name_is_not_in_the_error() {
    let home = Home::new("a-echo");
    let run = home.run(&[
        "server",
        "add",
        "gh",
        "--global",
        "--env",
        "sk-live-abc123",
        "--",
        "srv",
    ]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert!(!run.stderr_text().contains("sk-live"), "{}", run.describe());
    let run = add_global(
        &home,
        "web",
        WEB,
        &["--header", "Authorization Bearer user:pa55"],
    );
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert!(!run.stderr_text().contains("pa55"), "{}", run.describe());
}

#[test]
fn the_text_result_names_the_file_and_the_steps() {
    let home = Home::new("a-text");
    let run = add_global(&home, "web", WEB, &["--output-format", "text"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let file = home.config_path();
    assert_eq!(
        run.stdout_text(),
        format!(
            "Created {0}.\nAdded web to {0}.\nNext:\n  ncp auth login web   (if the server needs a login)\n  ncp server show web\n",
            file.display()
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn secret_stores_the_value_and_the_file_keeps_the_reference() {
    let fixture = serve_http_with(Mode::Modern, Some("t0ken")).await.unwrap();
    let home = Home::new("a-secret");
    let args = [
        "server",
        "add",
        "web",
        &fixture.url,
        "--global",
        "--header",
        "Authorization: Bearer ${WEB_TOKEN}",
        "--secret",
        "WEB_TOKEN",
        "--credential-store",
        "file",
    ];
    let run = home.run_with_stdin(&args, "t0ken");
    assert_eq!(run.code(), 0, "{}", run.describe());
    let out = run.stdout();
    assert_eq!(out["secrets"][0]["name"], "WEB_TOKEN");
    assert_eq!(out["secrets"][0]["credentialStores"], json!(["file"]));
    assert_eq!(out["secrets"][0]["servers"], json!(["web"]));
    // A credential header needs no login.
    assert_eq!(
        out["nextSteps"],
        json!([{"command": "ncp server show web"}])
    );
    let text = std::fs::read_to_string(home.config_path()).unwrap();
    assert!(
        text.contains("Bearer ${WEB_TOKEN}") && !text.contains("t0ken"),
        "{text}"
    );
    assert_eq!(home.run(&["tool", "list", "web"]).code(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn secret_with_another_stored_value_is_an_error_and_writes_no_entry() {
    let fixture = serve_http_with(Mode::Modern, Some("t0ken")).await.unwrap();
    let home = Home::new("a-secret-other");
    let header = ["--header", "Authorization: Bearer ${WEB_TOKEN}"];
    let secret = ["--secret", "WEB_TOKEN", "--credential-store", "file"];
    let more: Vec<&str> = header.iter().chain(secret.iter()).copied().collect();
    assert_eq!(
        add_global_with_stdin(&home, "web", &fixture.url, &more, "t0ken").code(),
        0
    );
    // The same value again passes.
    assert_eq!(
        add_global_with_stdin(&home, "web", &fixture.url, &more, "t0ken").code(),
        0
    );
    let run = add_global_with_stdin(&home, "web2", &fixture.url, &more, "other");
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert!(servers_of(&home.config_path()).get("web2").is_none());
}

#[test]
fn secret_on_an_entry_that_is_there_still_stores_the_value() {
    let home = Home::new("a-secret-retry");
    let header = ["--header", "Authorization: Bearer ${KEY}"];
    assert_eq!(add_global(&home, "web", WEB, &header).code(), 0);
    let more = [
        header[0],
        header[1],
        "--secret",
        "KEY",
        "--credential-store",
        "file",
    ];
    let run = add_global_with_stdin(&home, "web", WEB, &more, "value");
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(run.stdout()["changed"], false);
    assert_eq!(home.file_accounts(), ["secret/https://web.example.com/KEY"]);
}

#[test]
fn a_stdio_entry_with_env_gives_the_steps_that_are_left() {
    let home = Home::new("a-stdio");
    let run = home.run(&[
        "server",
        "add",
        "gh",
        "--global",
        "--env",
        "GH_TOKEN",
        "--env",
        "MODE=fast",
        "--",
        FIXTURE,
        "${FIXTURE_MODE}",
    ]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(
        servers_of(&home.config_path())["gh"],
        json!({"type": "stdio", "command": FIXTURE, "args": ["${FIXTURE_MODE}"],
               "env": {"GH_TOKEN": "${GH_TOKEN}", "MODE": "fast"}})
    );
    assert_eq!(
        run.stdout()["nextSteps"],
        json!([
            {"command": "ncp secret set GH_TOKEN --server gh"},
            {"setEnv": "FIXTURE_MODE"},
            {"command": "ncp server show gh"}
        ])
    );
}

#[test]
fn add_global_warns_when_a_trusted_project_entry_has_the_name() {
    let home = Home::new("a-warn").project("app", &json!({"web": {"command": "x"}}));
    assert_eq!(home.run(&["trust"]).code(), 0);
    let run = home.run(&[
        "server",
        "add",
        "web",
        "https://w.example.com/mcp",
        "--global",
    ]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let warning = run.warnings()[0]["message"].as_str().unwrap().to_string();
    assert!(
        warning.ends_with(
            "also has an entry of the server `web`. Its entry takes effect in this directory."
        ),
        "{warning}"
    );
}

fn both_files(test: &str) -> Home {
    let home = Home::new(test);
    home.configure(&json!({"web": {"command": "global"}, "g": {"command": "x"}}));
    home.project(
        "app",
        &json!({"web": {"command": "project"}, "p": {"command": "x"}}),
    )
}

#[test]
fn remove_takes_the_project_entry_that_runs_and_keeps_the_trust() {
    let home = both_files("r-project");
    assert_eq!(home.run(&["trust"]).code(), 0);
    let run = home.run(&["server", "remove", "web"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(run.stdout()["scope"], "project");
    assert_eq!(run.stdout()["file"], json!(home.project_file()));
    assert!(servers_of(&home.project_file()).get("web").is_none());
    assert!(servers_of(&home.config_path())["web"].is_object());
    assert_eq!(home.run(&["trust", "--show"]).stdout()["trust"], "trusted");
    // Then the global entry runs, and a second remove takes it.
    let run = home.run(&["server", "remove", "web"]);
    assert_eq!(run.stdout()["scope"], "global");
}

#[test]
fn remove_from_the_project_works_with_no_global_file() {
    let home = Home::new("r-no-global").project("app", &json!({"web": {"command": "x"}}));
    assert_eq!(home.run(&["trust"]).code(), 0);
    let run = home.run(&["server", "remove", "web"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert!(servers_of(&home.project_file()).get("web").is_none());
    assert!(!home.config_path().exists());
}

#[test]
fn remove_of_an_unknown_name_gives_the_nearest_name() {
    let home = both_files("r-unknown");
    let run = home.run(&["server", "remove", "gg"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert_eq!(run.error()["hints"][0]["message"], "Did you mean `g`?");
}

#[test]
fn remove_global_of_a_project_name_names_the_project_file() {
    let home = both_files("r-global-only");
    assert_eq!(home.run(&["trust"]).code(), 0);
    let run = home.run(&["server", "remove", "p", "--global"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert_eq!(
        run.error()["hints"],
        json!([{"message": "To remove the entry of the project file, run `ncp server remove p`"}])
    );
}

#[test]
fn remove_from_an_untrusted_project_file_needs_the_trust() {
    let home = both_files("r-untrusted");
    let run = home.run(&["server", "remove", "p"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert_eq!(run.error()["hints"][0]["command"], "ncp trust --show");
    let home = home.set("NCP_TEST_TTY", "1");
    let run = home.run_with_stdin(&["server", "remove", "p"], "y\n");
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert!(servers_of(&home.project_file()).get("p").is_none());
}

#[test]
fn remove_of_an_invalid_entry_by_its_raw_key() {
    let home = Home::new("r-invalid");
    home.configure(&json!({"bad.name": {"command": "x"}, "ok": {"command": "y"}}));
    let run = home.run(&["server", "remove", "bad.name", "--global"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(
        servers_of(&home.config_path()),
        json!({"ok": {"command": "y"}})
    );
}

#[test]
fn remove_from_an_ignored_project_file_is_an_error_unless_the_global_file_has_it() {
    let home = Home::new("r-ignored");
    home.configure(&json!({"both": {"command": "x"}}));
    let home = home.project(
        "app",
        &json!({"both": {"command": "y"}, "only": {"command": "z"}}),
    );
    assert_eq!(home.run(&["trust", "--ignore"]).code(), 0);
    let run = home.run(&["server", "remove", "only"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert_eq!(
        run.error()["hints"],
        json!([{"message": "To trust the file, run `ncp trust`"}])
    );
    let run = home.run(&["server", "remove", "both"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(run.stdout()["scope"], "global");
}

#[test]
fn remove_deletes_only_the_tokens_bound_to_the_entry() {
    let home = Home::new("r-tokens");
    home.configure(&json!({
        "web": {"type": "http", "url": "https://web.example.com/mcp"},
        "other": {"type": "http", "url": "https://other.example.com/mcp"}
    }));
    let tokens = |resource: &str| {
        json!({"client_id": "c", "token_response": null,
               "ncp_binding": {"resource": resource, "issuer": "https://as.example.com"}})
        .to_string()
    };
    home.store_in_file("oauth/web/tokens", &tokens("https://web.example.com/mcp"));
    home.store_in_file(
        "oauth/web/client",
        &json!({"issuer": "https://as.example.com", "clientId": "c"}).to_string(),
    );
    home.store_in_file(
        "oauth/other/tokens",
        &tokens("https://elsewhere.example.com/mcp"),
    );
    let run = home.run(&["server", "remove", "web"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(run.stdout()["credentialStores"], json!(["file"]));
    let run = home.run(&["server", "remove", "other", "--output-format", "text"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert!(
        run.stdout_text().contains("The OAuth tokens of other stay"),
        "{}",
        run.describe()
    );
    assert_eq!(home.file_accounts(), ["oauth/other/tokens"]);
}

#[test]
fn remove_with_a_store_that_fails_gives_a_warning_and_removes_the_entry() {
    let home = Home::new("r-store");
    home.configure(&json!({"web": {"type": "http", "url": WEB}}));
    // A directory in the place of the credential file: each read fails.
    std::fs::create_dir_all(home.root.join("state/ncp/credentials.json")).unwrap();
    let run = home.run(&["server", "remove", "web"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert!(
        run.stderr()
            .iter()
            .any(|line| line.get("warning").is_some()),
        "{}",
        run.describe()
    );
    assert_eq!(servers_of(&home.config_path()), json!({}));
}

#[test]
fn the_tokens_stay_when_the_other_file_still_has_the_name() {
    let home = both_files("r-still");
    assert_eq!(home.run(&["trust"]).code(), 0);
    let run = home.run(&["server", "remove", "web", "--output-format", "text"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert!(
        run.stdout_text()
            .contains("OAuth tokens of web, if any, stay: the entry of web in"),
        "{}",
        run.describe()
    );
}

// A local HTTP server that answers each request with `404`, as a server
// does for a path with no MCP endpoint.
fn not_found_server() -> String {
    use std::io::{Read as _, Write as _};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for mut stream in listener.incoming().filter_map(Result::ok) {
            let mut buffer = [0; 8192];
            let _ = stream.read(&mut buffer);
            let _ = stream.write_all(
                b"HTTP/1.1 404 Not Found\r\ncontent-length: 9\r\nconnection: close\r\n\r\nNot Found",
            );
        }
    });
    url
}

#[test]
fn a_url_with_a_wrong_path_gives_a_hint_at_the_first_connect() {
    let url = not_found_server();
    let home = Home::new("a-wrong-path");
    assert_eq!(
        home.run(&["server", "add", "web", &url, "--global"]).code(),
        0
    );
    let run = home.run(&["server", "show", "web"]);
    assert_eq!(run.code(), 3, "{}", run.describe());
    let error = run.error();
    assert!(
        error["causes"][0].as_str().unwrap().contains("HTTP 404"),
        "{error}"
    );
    assert_eq!(
        run.hints(),
        [
            "The server has no MCP endpoint at the path of the URL. Many servers use `/mcp`. \
             Examine the path in the entry."
        ]
    );
}

#[test]
fn ctrl_c_stops_the_wait_for_the_lock() {
    let home = Home::new("a-lock-interrupt");
    let lock = home.root.join("state/ncp/config.lock");
    std::fs::create_dir_all(lock.parent().unwrap()).unwrap();
    let held = std::fs::File::create(&lock).unwrap();
    held.lock().unwrap();
    let child = std::process::Command::new(common::NCP)
        .args(["server", "add", "web", WEB, "--global"])
        .env_clear()
        .envs(home.env.iter().cloned())
        .current_dir(&home.cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(700));
    let sent = std::process::Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(sent.success());
    let started = std::time::Instant::now();
    let run = common::Run(child.wait_with_output().unwrap());
    assert_eq!(run.code(), 130, "{}", run.describe());
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    assert!(!home.config_path().exists());
    drop(held);
}

#[test]
fn a_secret_word_that_is_not_a_name_is_not_in_the_error() {
    let home = Home::new("a-secret-echo");
    let run = add_global(
        &home,
        "web",
        WEB,
        &[
            "--header",
            "Authorization: Bearer ${K}",
            "--secret",
            "sk-live-ABC123",
        ],
    );
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert!(!run.stderr_text().contains("sk-live"), "{}", run.describe());
}

#[test]
fn a_duplicate_key_in_the_global_file_does_not_stop_a_remove_from_the_project() {
    let home = Home::new("r-global-duplicate");
    let path = home.config_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, r#"{"mcpServers": {}, "mcpServers": {}}"#).unwrap();
    let home = home.project("app", &json!({"p": {"command": "x"}}));
    assert_eq!(home.run(&["trust"]).code(), 0);
    let run = home.run(&["server", "remove", "p"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(run.stdout()["tokensStay"], false);
}

#[test]
fn a_global_duplicate_key_with_the_name_keeps_the_tokens_of_the_name() {
    let home = Home::new("r-global-duplicate-name");
    let path = home.config_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let entry = format!(r#"{{"type": "http", "url": "{WEB}"}}"#);
    std::fs::write(
        &path,
        format!(r#"{{"mcpServers": {{}}, "mcpServers": {{"p": {entry}}}}}"#),
    )
    .unwrap();
    let home = home.project("app", &json!({"p": {"type": "http", "url": WEB}}));
    assert_eq!(home.run(&["trust"]).code(), 0);
    let run = home.run(&["server", "remove", "p"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(run.stdout()["tokensStay"], true);
    // With the project file ignored, the global entry runs, and its file
    // has a duplicate key.
    let home = Home::new("r-global-duplicate-ignored");
    let path = home.config_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        format!(r#"{{"mcpServers": {{}}, "mcpServers": {{"p": {entry}}}}}"#),
    )
    .unwrap();
    let home = home.project("app", &json!({"p": {"type": "http", "url": WEB}}));
    assert_eq!(home.run(&["trust", "--ignore"]).code(), 0);
    let run = home.run(&["server", "remove", "p"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert!(run.error_text().contains("more than one time"));
}

#[test]
fn a_value_that_starts_with_two_hyphens_is_not_in_the_error() {
    let home = Home::new("a-hyphen");
    let run = home.run(&[
        "server",
        "add",
        "gh",
        "--global",
        "--env",
        "--sk-live-abc",
        "--",
        "srv",
    ]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert!(!run.stderr_text().contains("sk-live"), "{}", run.describe());
    let run = add_global(&home, "web", WEB, &["--secret", "--sk-live-abc"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert!(!run.stderr_text().contains("sk-live"), "{}", run.describe());
}

#[test]
fn add_with_a_url_that_is_not_valid_names_the_reason() {
    let home = Home::new("e-add-bad-url");
    let run = add_global(&home, "web", "http://example.com/mcp", &[]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    let text = run.error_text();
    assert!(text.contains("loopback"), "{text}");
    assert_eq!(run.hints(), ["Use an `https` URL"]);
    assert!(!home.config_path().exists());
    let run = add_global(&home, "web", "https://u:p@example.com/mcp", &[]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert!(
        run.hints()[0].contains("credential store"),
        "{}",
        run.describe()
    );
}

#[test]
fn the_remove_hint_of_a_named_file_keeps_the_config_file_option() {
    let home = Home::new("a-named-exists");
    let path = home.root.join("named.json");
    std::fs::write(
        &path,
        json!({"mcpServers": {"web": {"command": "x"}}}).to_string(),
    )
    .unwrap();
    let file = path.to_str().unwrap();
    let run = home.run(&["--config-file", file, "server", "add", "web", WEB]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert_eq!(
        run.hints(),
        [format!(
            "To remove the entry, run `ncp --config-file {file} server remove web`"
        )]
    );
}
