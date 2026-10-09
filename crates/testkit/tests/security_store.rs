//! The `security` store with the fake `security` program of the test kit.
//! Tests never use the real Keychain.

// The whole file is test code, where the agent rules permit unwrap.
#![allow(clippy::unwrap_used)]

use std::{fs, path::PathBuf};

use serde_json::json;

use ncp::{
    credential::{CredentialKey, Secret, SecretName},
    server::ServerName,
    store::{CredentialStore, SecurityStore, StoreError},
};

const FAKE: &str = env!("CARGO_BIN_EXE_fake-security");

/// A value that is not safe in a command line or in the output of the real
/// program.
const HARD_VALUE: &str =
    "he said \"hi\" and 'bye' \\ $HOME `x` #\n  tab\t\r\n\u{fc}n\u{ef}c\u{f8}d\u{e9} \u{1f511}";

/// A new directory with a link to the fake program. The fake keeps its
/// files next to the link.
fn keychain(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ncp-security-{}-{test}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    std::os::unix::fs::symlink(FAKE, dir.join("security")).unwrap();
    dir
}

fn store(dir: &std::path::Path) -> SecurityStore {
    SecurityStore::with_program(dir.join("security"))
}

fn server(name: &str) -> ServerName {
    ServerName::parse(name).unwrap()
}

fn tokens() -> CredentialKey {
    CredentialKey::OAuthTokens(server("a"))
}

fn secret(text: &str) -> Secret {
    Secret::new(text.to_string())
}

fn calls(dir: &std::path::Path) -> String {
    fs::read_to_string(dir.join("calls.log")).unwrap_or_default()
}

#[test]
fn keeps_secrets_by_key() {
    let dir = keychain("contract");
    let store = store(&dir);
    let env = CredentialKey::Secret(SecretName::parse("API_KEY").unwrap());
    let other_server = CredentialKey::OAuthTokens(server("b"));

    assert_eq!(store.load(&tokens()).unwrap(), None);
    store.save(&tokens(), &secret("t1")).unwrap();
    store.save(&env, &secret("e1")).unwrap();
    assert_eq!(store.load(&tokens()).unwrap(), Some(secret("t1")));
    assert_eq!(store.load(&env).unwrap(), Some(secret("e1")));
    assert_eq!(store.load(&other_server).unwrap(), None);

    store.save(&tokens(), &secret("t2")).unwrap();
    assert_eq!(store.load(&tokens()).unwrap(), Some(secret("t2")));

    store.delete(&tokens()).unwrap();
    assert_eq!(store.load(&tokens()).unwrap(), None);
    assert_eq!(store.load(&env).unwrap(), Some(secret("e1")));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn deleting_a_missing_item_is_not_an_error() {
    let dir = keychain("delete-missing");
    store(&dir).delete(&tokens()).unwrap();
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn keeps_a_value_with_quotes_spaces_new_lines_and_non_ascii_text() {
    let dir = keychain("hard-value");
    let store = store(&dir);
    store.save(&tokens(), &secret(HARD_VALUE)).unwrap();
    assert_eq!(store.load(&tokens()).unwrap(), Some(secret(HARD_VALUE)));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn never_puts_the_value_in_a_program_argument() {
    let dir = keychain("arguments");
    let store = store(&dir);
    store.save(&tokens(), &secret(HARD_VALUE)).unwrap();
    store.load(&tokens()).unwrap();
    store.delete(&tokens()).unwrap();
    let log = calls(&dir);
    // The save and the delete also list the accounts, to find old parts.
    assert_eq!(log.lines().count(), 5);
    assert_eq!(log.lines().next(), Some(r#"["-i"]"#));
    assert!(!log.contains("he said"));
    assert!(!log.contains("ncp-base64:"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn stores_the_value_in_encoded_form() {
    let dir = keychain("encoded");
    store(&dir).save(&tokens(), &secret(HARD_VALUE)).unwrap();
    let items = fs::read_to_string(dir.join("items.json")).unwrap();
    assert!(items.contains("\"oauth/a/tokens\": \"ncp-base64:"));
    assert!(!items.contains("he said"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn reads_a_value_with_no_mark_as_plain_text() {
    let dir = keychain("plain");
    fs::write(
        dir.join("items.json"),
        r#"{"ncp": {"oauth/a/tokens": "added by hand"}}"#,
    )
    .unwrap();
    assert_eq!(
        store(&dir).load(&tokens()).unwrap(),
        Some(secret("added by hand"))
    );
    fs::remove_dir_all(dir).unwrap();
}

fn items(dir: &std::path::Path) -> serde_json::Map<String, serde_json::Value> {
    let text = fs::read_to_string(dir.join("items.json")).unwrap();
    let mut all: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&text).unwrap();
    match all.remove("ncp") {
        Some(serde_json::Value::Object(items)) => items,
        _ => serde_json::Map::new(),
    }
}

fn long_value() -> String {
    HARD_VALUE.repeat(200)
}

#[test]
fn keeps_a_long_value_in_parts() {
    let dir = keychain("parts");
    let store = store(&dir);
    store.save(&tokens(), &secret(&long_value())).unwrap();
    assert_eq!(store.load(&tokens()).unwrap(), Some(secret(&long_value())));
    let items = items(&dir);
    let head = items["oauth/a/tokens"].as_str().unwrap();
    assert!(head.starts_with("ncp-parts:"), "{head}");
    assert!(items.len() > 2);
    assert!(!calls(&dir).contains("he said"));
    // A part is not a key of its own.
    assert_eq!(store.keys().unwrap(), [tokens()]);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn removes_the_old_parts_after_each_save() {
    let dir = keychain("old-parts");
    // Parts that are older than the limit of a save belong to no save that
    // still runs.
    let limit = std::time::Duration::from_secs(1);
    let store = store(&dir).with_limit(limit);
    let older_than_the_limit = || std::thread::sleep(limit + std::time::Duration::from_millis(100));
    store.save(&tokens(), &secret(&long_value())).unwrap();
    let first: Vec<String> = items(&dir).keys().cloned().collect();
    older_than_the_limit();
    store.save(&tokens(), &secret(&long_value())).unwrap();
    let second: Vec<String> = items(&dir).keys().cloned().collect();
    assert_eq!(first.len(), second.len());
    assert_eq!(
        first
            .iter()
            .filter(|account| second.contains(account))
            .count(),
        1,
        "only the head stays: {first:?} {second:?}"
    );
    older_than_the_limit();
    store.save(&tokens(), &secret("short")).unwrap();
    assert_eq!(items(&dir).keys().collect::<Vec<_>>(), ["oauth/a/tokens"]);
    assert_eq!(store.load(&tokens()).unwrap(), Some(secret("short")));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn keeps_the_parts_of_a_save_that_can_still_run() {
    let dir = keychain("running-save");
    let store = store(&dir);
    // Another process has written the parts of its save, and not yet its
    // head. Its generation is young: the time now, in hexadecimal.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let running = format!("oauth/a/tokens/{now:x}-1/0");
    let stopped = "oauth/a/tokens/1-1/0";
    fs::write(
        dir.join("items.json"),
        json!({"ncp": {running.clone(): "ncp-base64:", stopped: "ncp-base64:"}}).to_string(),
    )
    .unwrap();
    store.save(&tokens(), &secret("t1")).unwrap();
    let items = items(&dir);
    assert!(items.contains_key(&running), "{items:?}");
    assert!(!items.contains_key(stopped), "{items:?}");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn deletes_the_head_and_the_parts() {
    let dir = keychain("delete-parts");
    let store = store(&dir);
    let other = CredentialKey::Secret(SecretName::parse("A").unwrap());
    store.save(&other, &secret("keep")).unwrap();
    store.save(&tokens(), &secret(&long_value())).unwrap();
    assert!(store.delete(&tokens()).unwrap());
    assert_eq!(items(&dir).keys().collect::<Vec<_>>(), ["secret/A"]);
    assert_eq!(store.load(&tokens()).unwrap(), None);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn reads_the_head_again_then_names_the_account_when_a_part_is_missing() {
    let dir = keychain("missing-part");
    fs::write(
        dir.join("items.json"),
        r#"{"ncp": {"oauth/a/tokens": "ncp-parts:g1:2", "oauth/a/tokens/g1/0": "ncp-base64:"}}"#,
    )
    .unwrap();
    let Err(StoreError::Failure(message)) = store(&dir).load(&tokens()) else {
        panic!("a missing part must fail");
    };
    assert!(message.contains("oauth/a/tokens"), "{message}");
    let head_reads = calls(&dir)
        .lines()
        .filter(|line| line.contains(r#""oauth/a/tokens","-w""#))
        .count();
    assert_eq!(head_reads, 2);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn keeps_the_old_value_when_a_save_in_parts_runs_out_of_time() {
    let dir = keychain("parts-deadline");
    let fast = store(&dir);
    fast.save(&tokens(), &secret("old")).unwrap();
    // Each run takes 300 ms, and the parts need more runs than the limit
    // permits.
    fs::write(dir.join("delay"), "300").unwrap();
    let limit = std::time::Duration::from_millis(1000);
    let start = std::time::Instant::now();
    let result = fast
        .clone()
        .with_limit(limit)
        .save(&tokens(), &secret(&long_value()));
    assert!(matches!(result, Err(StoreError::NoAnswer(_))), "{result:?}");
    assert!(start.elapsed() < limit + std::time::Duration::from_millis(500));
    fs::remove_file(dir.join("delay")).unwrap();
    assert_eq!(fast.load(&tokens()).unwrap(), Some(secret("old")));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn reports_a_failing_program_without_the_value() {
    let dir = keychain("failing");
    // The fake writes the command that it received to stderr, then fails.
    fs::write(dir.join("fail"), "1").unwrap();
    let Err(StoreError::Failure(message)) = store(&dir).save(&tokens(), &secret(HARD_VALUE)) else {
        panic!("a failing program must give a failure");
    };
    assert!(message.contains("exited with code 1"), "{message}");
    assert!(!message.contains("he said"));
    assert!(!message.contains("ncp-base64:"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn reports_a_refused_access_as_denied() {
    let dir = keychain("denied");
    // errSecAuthFailed (-25293) gives exit code 51.
    fs::write(dir.join("fail"), "51").unwrap();
    assert!(matches!(
        store(&dir).load(&tokens()),
        Err(StoreError::Denied(_))
    ));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn reports_a_missing_program_as_unavailable() {
    let dir = keychain("missing-program");
    let store = SecurityStore::with_program(dir.join("no-such-program"));
    assert!(matches!(
        store.load(&tokens()),
        Err(StoreError::Unavailable(_))
    ));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn stops_a_program_that_gives_no_answer_at_the_limit() {
    let dir = keychain("no-answer");
    fs::write(dir.join("delay"), "30000").unwrap();
    let start = std::time::Instant::now();
    let limit = std::time::Duration::from_millis(300);
    let result = store(&dir).with_limit(limit).load(&tokens());
    assert!(
        matches!(result, Err(StoreError::NoAnswer(d)) if d == limit),
        "{result:?}"
    );
    assert!(start.elapsed() < std::time::Duration::from_secs(3));
    // The program is gone: no process has the fake of this test as its path.
    let program = dir.join("security").display().to_string();
    let found = std::process::Command::new("pgrep")
        .args(["-f", &program])
        .output()
        .unwrap();
    assert!(found.stdout.is_empty(), "the program still runs");
    fs::remove_dir_all(dir).unwrap();
}
