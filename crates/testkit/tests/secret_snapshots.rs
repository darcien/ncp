//! Transcripts of the secret commands and the files that they change.

#![allow(clippy::unwrap_used)]

mod common;

use std::os::unix::fs::PermissionsExt as _;

use common::{Home, Run, ncp_snapshot};
use serde_json::json;

const SECRET: &str = "snapshot-private-value";

fn secret_home(name: &str) -> Home {
    let home = Home::new(name).with_fake_keychain();
    home.configure(&json!({"fx": {
        "command": "unused", "env": {"TOKEN": "${TOKEN}"}
    }}));
    home
}

fn assert_no_secret(home: &Home, run: &Run) {
    for bytes in [&run.0.stdout, &run.0.stderr] {
        assert!(
            !bytes
                .windows(SECRET.len())
                .any(|value| value == SECRET.as_bytes())
        );
    }
    let calls = home.root.join("keychain/calls.log");
    if calls.exists() {
        let calls = std::fs::read_to_string(calls).unwrap();
        assert!(
            !calls.contains(SECRET),
            "a value is in a store argument: {calls}"
        );
    }
}

fn assert_file_mode(home: &Home) {
    let path = home.root.join("state/ncp/credentials.json");
    assert_eq!(
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn secret_set_stores_piped_input_in_the_credential_file() {
    let home = secret_home("secret-snapshot-set-text");
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["secret", "set", "TOKEN", "--credential-store", "file"]),
        input = &format!("{SECRET}\n"),
        @"
    exit_code: 0 (success)
    ----- stdout -----
    Stored TOKEN in the credential file for stdio servers (server fx).
    "
    );
    assert_no_secret(&home, &run);
    assert_file_mode(&home);
    assert_eq!(home.file_accounts(), ["secret/TOKEN"]);
    let mut filters = vec![(SECRET, "[SECRET]")];
    filters.extend(home.filters());
    insta::with_settings!({filters => filters}, {
        insta::assert_snapshot!(home.read("state/ncp/credentials.json"), @r#"
        {
          "version": 1,
          "secrets": {
            "secret/TOKEN": "[SECRET]"
          }
        }
        "#);
    });
}

#[test]
fn secret_set_reports_the_stored_item_in_json() {
    let home = secret_home("secret-snapshot-set-json");
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "secret", "set", "TOKEN", "--credential-store", "file"]),
        input = &format!("{SECRET}\n"),
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"credentialStore":"file","destination":null,"name":"TOKEN","servers":["fx"]}
    "#
    );
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
    assert!(run.0.stdout.ends_with(b"\n"));
    assert_no_secret(&home, &run);
    assert_file_mode(&home);
    assert_eq!(home.file_accounts(), ["secret/TOKEN"]);
}

#[test]
fn secret_set_needs_an_entry_that_names_the_secret() {
    let home = Home::new("secret-snapshot-set-unused").with_fake_keychain();
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["secret", "set", "TOKEN", "--credential-store", "file"]),
        input = SECRET,
        @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No entry names `${TOKEN}` in `env`, `headers` or `oauth.clientSecret`

    hint: To store the value, add an entry that names `${TOKEN}` first. Then run the command again.
    "
    );
    assert_no_secret(&home, &run);
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "secret", "set", "TOKEN", "--credential-store", "file"]),
        input = SECRET,
        @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    {"error":{"causes":[],"code":"usage","data":[],"exit":2,"hints":[{"message":"To store the value, add an entry that names `${TOKEN}` first. Then run the command again."}],"message":"No entry names `${TOKEN}` in `env`, `headers` or `oauth.clientSecret`"}}
    "#
    );
    run.error();
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
    assert_no_secret(&home, &run);
    assert!(!home.root.join("state/ncp/credentials.json").exists());
}

#[test]
fn secret_list_shows_the_stored_items() {
    let home = secret_home("secret-snapshot-list");
    home.store_in_file("secret/TOKEN", SECRET);
    let run = ncp_snapshot!(home.filters(), home.ncp(["secret", "list"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    TOKEN  stdio servers  file  fx
    ");
    assert_no_secret(&home, &run);
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "secret", "list"]),
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"secrets":[{"credentialStores":["file"],"destination":null,"name":"TOKEN","servers":["fx"],"used":true}]}
    "#
    );
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
    assert!(run.0.stdout.ends_with(b"\n"));
    assert_no_secret(&home, &run);
}

#[test]
fn secret_list_warns_when_the_credential_file_is_corrupt() {
    let home = secret_home("secret-snapshot-list-corrupt");
    home.store_in_file("secret/TOKEN", SECRET);
    std::fs::write(
        home.root.join("state/ncp/credentials.json"),
        format!("{{{SECRET}"),
    )
    .unwrap();
    let run = ncp_snapshot!(home.filters(), home.ncp(["secret", "list"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: ncp cannot list the items in the credential file
      cause: The credential file `[HOME]/state/ncp/credentials.json` is not valid
    ");
    assert_no_secret(&home, &run);
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "secret", "list"]),
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"secrets":[]}

    ----- stderr -----
    {"warning":{"causes":["The credential file `[HOME]/state/ncp/credentials.json` is not valid"],"data":[],"hints":[],"message":"ncp cannot list the items in the credential file"}}
    "#
    );
    assert_eq!(run.warnings().len(), 1);
    assert_no_secret(&home, &run);
    let mut filters = vec![(SECRET, "[SECRET]")];
    filters.extend(home.filters());
    insta::with_settings!({filters => filters}, {
        insta::assert_snapshot!(home.read("state/ncp/credentials.json"), @"{[SECRET]");
    });
}

#[test]
fn secret_delete_removes_the_stored_item() {
    let home = secret_home("secret-snapshot-delete-text");
    home.store_in_file("secret/TOKEN", SECRET);
    let run = ncp_snapshot!(home.filters(), home.ncp(["secret", "delete", "TOKEN"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    Deleted TOKEN for stdio servers from the credential file.
    ");
    assert_no_secret(&home, &run);
    assert_eq!(home.file_accounts(), [] as [String; 0]);
    insta::with_settings!({filters => home.filters()}, {
        insta::assert_snapshot!(home.read("state/ncp/credentials.json"), @r#"
        {
          "version": 1,
          "secrets": {}
        }
        "#);
    });
}

#[test]
fn secret_delete_reports_the_deleted_item_in_json() {
    let home = secret_home("secret-snapshot-delete-json");
    home.store_in_file("secret/TOKEN", SECRET);
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "secret", "delete", "TOKEN"]),
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"deleted":[{"credentialStores":["file"],"destination":null}],"file":null,"name":"TOKEN"}
    "#
    );
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
    assert_no_secret(&home, &run);
    assert_eq!(home.file_accounts(), [] as [String; 0]);
}

#[test]
fn secret_delete_reports_a_name_with_no_stored_item() {
    let home = secret_home("secret-snapshot-delete-missing");
    ncp_snapshot!(home.filters(), home.ncp(["secret", "delete", "TOKEN"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No credential store has an item of `TOKEN`

    hint: To see the stored items and their destinations, run `ncp secret list`
    ");
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "secret", "delete", "TOKEN"]),
        @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    {"error":{"causes":[],"code":"usage","data":[],"exit":2,"hints":[{"command":"ncp secret list","message":"To see the stored items and their destinations, run `ncp secret list`"}],"message":"No credential store has an item of `TOKEN`"}}
    "#
    );
    run.error();
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
    assert!(!home.root.join("state/ncp/credentials.json").exists());
}
