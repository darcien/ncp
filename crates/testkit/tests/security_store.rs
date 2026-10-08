//! The `security` store with the fake `security` program of the test kit.
//! Tests never use the real Keychain.

// The whole file is test code, where the agent rules permit unwrap.
#![allow(clippy::unwrap_used)]

use std::{fs, path::PathBuf};

use ncp::{
    credential::{CredentialKey, EnvName, Secret, SecretKind},
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

fn key(server: &str, kind: SecretKind) -> CredentialKey {
    CredentialKey::new(ServerName::parse(server).unwrap(), kind)
}

fn tokens() -> CredentialKey {
    key("a", SecretKind::OAuthTokens)
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
    let env = key("a", SecretKind::Env(EnvName::parse("API_KEY").unwrap()));
    let other_server = key("b", SecretKind::OAuthTokens);

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
    assert_eq!(log.lines().count(), 3);
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
    assert!(items.contains("\"a/oauth/tokens\": \"ncp-base64:"));
    assert!(!items.contains("he said"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn reads_a_value_with_no_mark_as_plain_text() {
    let dir = keychain("plain");
    fs::write(
        dir.join("items.json"),
        r#"{"ncp": {"a/oauth/tokens": "added by hand"}}"#,
    )
    .unwrap();
    assert_eq!(
        store(&dir).load(&tokens()).unwrap(),
        Some(secret("added by hand"))
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn keeps_the_longest_value_that_fits_in_one_command_line() {
    let dir = keychain("longest");
    let store = store(&dir);
    let fits = |len: usize| store.save(&tokens(), &secret(&"x".repeat(len))).is_ok();
    // Find the longest value that the store accepts.
    let (mut short, mut long) = (0, 4096);
    while long - short > 1 {
        let middle = usize::midpoint(short, long);
        if fits(middle) {
            short = middle;
        } else {
            long = middle;
        }
    }
    store.save(&tokens(), &secret(&"x".repeat(short))).unwrap();
    // The fake cuts a longer line as the real program does, so a limit that
    // is too large gives a different value here.
    assert_eq!(
        store.load(&tokens()).unwrap(),
        Some(secret(&"x".repeat(short)))
    );
    assert!(short > 2900, "the longest value has {short} bytes");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn refuses_a_value_that_is_too_long_without_running_the_program() {
    let dir = keychain("too-long");
    let value = format!("{}tail", "x".repeat(4000));
    let Err(StoreError::Failure(message)) = store(&dir).save(&tokens(), &secret(&value)) else {
        panic!("a long value must fail");
    };
    assert!(!message.contains("xxxx"));
    assert_eq!(calls(&dir), "");
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
