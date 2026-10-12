//! Tests of `ncp server add` and `ncp server remove`: the entry, the
//! target file and its trust, the write, the secret, and the tokens.

// The whole file is test code, where the agent rules permit unwrap.
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

/// The JSON value of a file with comments: the tests read it with the
/// parser of `ncp`.
fn jsonc_value(text: &str) -> Value {
    Value::Object(ncp::config::parse_root(text).unwrap())
}

/// A folder in the home folder with a `.git` folder and no project file.
fn in_repository(mut home: Home) -> Home {
    let dir = home.root.join("app");
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    home.cwd = dir;
    home
}

fn new_project_file(home: &Home) -> PathBuf {
    std::fs::canonicalize(&home.cwd).unwrap().join(".mcp.json")
}

#[test]
fn add_creates_a_project_file_in_the_current_folder_and_trusts_it() {
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
fn add_in_the_home_folder_and_with_global_writes_the_global_file() {
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
fn the_same_entry_again_changes_nothing_and_another_entry_is_an_error() {
    let home = Home::new("a-again");
    let add = |url: &str| home.run(&["server", "add", "web", url, "--global"]);
    assert_eq!(add("https://web.example.com/mcp").code(), 0);
    let run = add("https://web.example.com/mcp");
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(run.stdout()["changed"], false);
    let run = add("https://other.example.com/mcp");
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert_eq!(run.error()["hint"], "ncp server remove web --global");
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
        run.error()["hint"],
        "--header 'Authorization: Bearer ${NAME}' --secret NAME"
    );
    assert!(!home.config_path().exists());
}

#[test]
fn usage_errors_come_before_any_read_or_write() {
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
fn an_untrusted_project_file_is_not_changed_without_a_trust() {
    let home = Home::new("a-untrusted").project("app", &json!({"fs": {"command": "x"}}));
    let file = home.project_file();
    let before = std::fs::read_to_string(&file).unwrap();
    let run = home.run(&["server", "add", "web", "https://w.example.com/mcp"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert_eq!(run.error()["hint"], "ncp trust --show");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
    // A No at the question ignores the file.
    let tty = home.set("NCP_TEST_TTY", "1");
    let run = tty.run_with_stdin(
        &["server", "add", "web", "https://w.example.com/mcp"],
        "n\n",
    );
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
    let home = tty.unset("NCP_TEST_TTY");
    let run = home.run(&["server", "add", "web", "https://w.example.com/mcp"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert_eq!(run.error()["hint"], "--global");
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
    assert_eq!(run.error()["hint"], "--global");
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
    // The same value again passes; another value is an error.
    let run = home.run_with_stdin(&args, "t0ken");
    assert_eq!(run.code(), 0, "{}", run.describe());
    let other = [
        "server",
        "add",
        "web2",
        &fixture.url,
        "--global",
        "--header",
        "Authorization: Bearer ${WEB_TOKEN}",
        "--secret",
        "WEB_TOKEN",
        "--credential-store",
        "file",
    ];
    let run = home.run_with_stdin(&other, "other");
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert!(servers_of(&home.config_path()).get("web2").is_none());
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
    let warning = run
        .stderr()
        .iter()
        .find_map(|line| line["warning"]["message"].as_str().map(str::to_string))
        .unwrap();
    assert!(
        warning.ends_with("also has web; its entry takes effect in this folder"),
        "{warning}"
    );
}

#[test]
fn remove_takes_the_entry_that_runs() {
    let home = Home::new("r-effective");
    home.configure(&json!({"web": {"command": "global"}, "g": {"command": "x"}}));
    let home = home.project(
        "app",
        &json!({"web": {"command": "project"}, "p": {"command": "x"}}),
    );
    assert_eq!(home.run(&["trust"]).code(), 0);
    let run = home.run(&["server", "remove", "web"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(run.stdout()["scope"], "project");
    assert_eq!(run.stdout()["file"], json!(home.project_file()));
    assert!(servers_of(&home.project_file()).get("web").is_none());
    assert!(servers_of(&home.config_path())["web"].is_object());
    // The project file stays trusted after the write.
    assert_eq!(home.run(&["trust", "--show"]).stdout()["trust"], "trusted");
    let run = home.run(&["server", "remove", "web"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(run.stdout()["scope"], "global");
    let run = home.run(&["server", "remove", "gg"]);
    assert_eq!(run.code(), 2, "{}", run.describe());
    assert!(
        run.error()["message"]
            .as_str()
            .unwrap()
            .contains("did you mean \"g\"")
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
    assert_eq!(run.error()["hint"], "ncp trust");
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
    let run = home.run(&["server", "remove", "other"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    assert_eq!(run.stdout()["credentialStores"], json!([]));
    assert_eq!(home.file_accounts(), ["oauth/other/tokens"]);
}
