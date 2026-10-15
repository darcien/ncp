//! Tests of the Linux Secret Service store.
//!
//! These tests run only in the Linux harness container, through
//! `harness/linux/secret-service.sh`. The script starts a new D-Bus session and
//! keyring daemon for each case, and runs one test in it. A normal test run
//! skips them, because a test must not use the Secret Service of the host.

#![cfg(target_os = "linux")]
// Test code can use unwrap: a panic is a test failure.
#![allow(clippy::unwrap_used)]

use std::time::{Duration, Instant};

use ncp::{
    credential::{CredentialKey, Destination, Secret, SecretName},
    server::ServerName,
    store::{self, CredentialStore, NativeStore, StoreError},
};

/// A short limit, so that the locked case does not wait 60 s.
const LIMIT: Duration = Duration::from_secs(3);

fn secret_key() -> CredentialKey {
    CredentialKey::Secret {
        destination: Destination::Stdio,
        name: SecretName::parse("NCP_TEST_TOKEN").unwrap(),
    }
}

async fn open() -> Result<std::sync::Arc<NativeStore>, StoreError> {
    store::run_blocking(LIMIT, || NativeStore::open(LIMIT))
        .await
        .map(std::sync::Arc::new)
}

async fn load(store: &std::sync::Arc<NativeStore>) -> Result<Option<Secret>, StoreError> {
    let store = store.clone();
    store::run_blocking(LIMIT, move || store.load(&secret_key())).await
}

#[tokio::test]
#[ignore = "runs only in harness/linux/secret-service.sh"]
async fn no_session_bus_gives_an_unavailable_store() {
    let result = match open().await {
        Ok(store) => load(&store).await.map(|_| ()),
        Err(error) => Err(error),
    };
    assert!(
        matches!(result, Err(StoreError::Unavailable(_))),
        "{result:?}"
    );
}

#[tokio::test]
#[ignore = "runs only in harness/linux/secret-service.sh"]
async fn bus_with_no_secret_service_gives_an_unavailable_store() {
    let result = match open().await {
        Ok(store) => load(&store).await.map(|_| ()),
        Err(error) => Err(error),
    };
    assert!(
        matches!(result, Err(StoreError::Unavailable(_))),
        "{result:?}"
    );
}

#[tokio::test]
#[ignore = "runs only in harness/linux/secret-service.sh"]
async fn unlocked_keyring_keeps_and_lists_secrets() {
    let store = open().await.unwrap();
    let tokens = CredentialKey::OAuthTokens(ServerName::parse("gh").unwrap());
    let value = Secret::new("v \"q\"\nline".into());
    let saved = value.clone();
    let writer = store.clone();
    store::run_blocking(LIMIT, move || {
        writer.save(&secret_key(), &saved)?;
        writer.save(&tokens, &Secret::new("t".into()))
    })
    .await
    .unwrap();
    assert_eq!(load(&store).await.unwrap(), Some(value));
    let lister = store.clone();
    let keys = store::run_blocking(LIMIT, move || lister.keys())
        .await
        .unwrap();
    let accounts: Vec<String> = keys.iter().map(CredentialKey::account).collect();
    assert_eq!(accounts, ["secret/NCP_TEST_TOKEN", "oauth/gh/tokens"]);
    let deleter = store.clone();
    store::run_blocking(LIMIT, move || {
        deleter.delete(&secret_key())?;
        deleter.delete(&secret_key())
    })
    .await
    .unwrap();
    assert_eq!(load(&store).await.unwrap(), None);
}

// A runtime that does not wait for a blocked store call at its end. A
// test runtime waits for its blocking threads with no limit.
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn lock_the_login_keyring() {
    let status = std::process::Command::new("dbus-send")
        .args([
            "--session",
            "--print-reply",
            "--dest=org.freedesktop.secrets",
            "/org/freedesktop/secrets",
            "org.freedesktop.Secret.Service.Lock",
            "array:objpath:/org/freedesktop/secrets/collection/login",
        ])
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
#[ignore = "runs only in harness/linux/secret-service.sh"]
fn locked_keyring_gives_an_error_within_the_time_limit() {
    let runtime = runtime();
    let start = Instant::now();
    let (keys, saved) = runtime.block_on(async {
        let store = open().await.unwrap();
        let writer = store.clone();
        store::run_blocking(LIMIT, move || {
            writer.save(&secret_key(), &Secret::new("before".into()))
        })
        .await
        .unwrap();
        lock_the_login_keyring();
        let lister = store.clone();
        let keys = store::run_blocking(LIMIT, move || lister.keys()).await;
        let writer = store.clone();
        let saved = store::run_blocking(LIMIT, move || {
            writer.save(&secret_key(), &Secret::new("v".into()))
        })
        .await;
        (keys, saved)
    });
    runtime.shutdown_timeout(Duration::from_millis(100));
    // The list shows no prompt: it fails at once for a locked item.
    assert!(matches!(keys, Err(StoreError::Locked(_))), "{keys:?}");
    // A save needs an unlock. With no prompt program, it fails at once. With
    // one, it stops at the time limit.
    assert!(saved.is_err(), "a locked keyring must not take the value");
    assert!(
        start.elapsed() < 2 * LIMIT + Duration::from_secs(1),
        "{:?}",
        start.elapsed()
    );
}
