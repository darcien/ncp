# Native credential store: crates and threading

Date: 2026-10-07. This report replaces the `keyring` 3.6 parts of
`early-tests.md` (test 6) and the keyring 3.6.3 setup of milestone 3.

## Question

Which crates give the native credential store, and how does an async
program call them correctly? `ncp` is for general use: a company Mac, a
personal computer, or a remote Linux server. Linux releases are static musl
binaries.

## Result

| Topic | Result | Source |
|---|---|---|
| Crates for applications | The `keyring` 4 README: applications "should take dependencies on the keyring-core crate and the specific credential stores they want to use". `keyring` 4 is a wrapper for samples and a `v1` shim. | [keyring 4.2.0 README][kr] |
| `keyring` 3.6.3 | No release after 2025-07-27. It uses zbus 4 and secret-service 4, and opens a new D-Bus connection for each call (`keyring-3.6.3/src/secret_service.rs:144,337`). | crate source |
| `v1` shim of `keyring` 4 | It keeps a global default store. A failed start makes all later calls fail. | [keyring-rs#343][kr343] |
| macOS store | `apple-native-keyring-store` 1.0.2, module `keychain`: generic password in the login keychain, attributes `service` and `account`. The `protected` module needs a provisioning profile. | `apple-native-keyring-store-1.0.2/src/lib.rs`, `keychain.rs` |
| Linux store | `zbus-secret-service-keyring-store` 1.0.1: secret-service 5 and zbus 5, pure Rust. One D-Bus connection for each store, behind a mutex. `Store::new` connects, so it blocks. | `service.rs:19-28`, `store.rs:47-60` |
| Runtime feature | The Linux store needs exactly one of four `rt-*` features. `rt-async-io-crypto-rust` enables `zbus/async-io`. | `zbus-secret-service-keyring-store-1.0.1/src/lib.rs` (Features) |
| zbus blocking call | Without the zbus `tokio` feature, `block_on` is `async_io::block_on`. With it, zbus starts a private tokio runtime (`zbus-5.19.0/src/utils.rs:31-50`). | crate source |
| tokio feature panic | With the tokio runtime feature, a call on a tokio thread panics: "Cannot start a runtime from within a runtime". The keyring 3.6.3 README calls this a deadlock. It advises the runtime of the app and a separate thread; the separate thread is what prevents the panic. | [keyring-rs#132][kr132] |
| Test of the four contexts | zbus with `async-io`: a blocking round trip works on a tokio worker, on a current-thread runtime, in `spawn_blocking`, and on a plain thread. zbus with `tokio`: panic on the worker and on the current-thread runtime. Local D-Bus over a socket pair, no Secret Service. | scratch test, 2026-10-07 |
| Unlock prompt | A locked item causes an unlock prompt. The blocking prompt call waits for the `Completed` signal with no time limit. On a server with no display, nobody can answer the prompt. | `secret-service-5.2.0/src/util.rs:121-135` |
| Runtime shutdown | tokio waits with no limit for started `spawn_blocking` tasks at shutdown, unless the runtime shuts down with a timeout. | `tokio-1.53.2/src/task/blocking.rs:28,115` |
| Static musl | `keyring-core` 1.0.0 and the zbus store with `rt-async-io-crypto-rust` build for `x86_64-unknown-linux-musl` with `rust-lld`: static-pie ELF, no `cc` and no `-sys` crate other than `linux-raw-sys`. | scratch build, 2026-10-07 |
| No session bus | secret-service returns `Unavailable`; the store returns `PlatformFailure`. The failure is fast. | `secret-service-5.2.0/src/util.rs:151-158` |
| Search | `keyring-core` has `CredentialStore::search`. Both stores implement it. A Secret Service search with locked items asks to unlock them. A Keychain search prompt: Unknown. | `zbus-secret-service-keyring-store-1.0.1/src/store.rs`, `apple-native-keyring-store-1.0.2/src/keychain.rs` |
| Other tools | gh: 60 s limit for each keyring call, then plain text. Codex CLI: keyring, then a file. uv and cargo: plain text by default. | [gh keyring.go][gh] |

## Decision

- Use `keyring-core` 1.0.0, `apple-native-keyring-store` 1.0.2 with
  `keychain`, and `zbus-secret-service-keyring-store` 1.0.1 with
  `rt-async-io-crypto-rust`. Pin them with `=`.
- Build entries from a store value. Do not use the global default store.
- Run each store call on a blocking thread with a 60 s limit. Shut down the
  runtime with a time limit, because a call that passes the limit can stay
  on its thread.
- `deny.toml` refuses the zbus `tokio` feature, so that no other crate can
  enable it.
- Keep the explicit opt-in for the file store. Other tools change to plain
  text without a request; `ncp` does not.

## Open points

- Does a Keychain search show a prompt? This decides if the secret index
  file of the command-line design is necessary.
- Behavior on a real Linux host with and without a Secret Service. Automated
  tests do not use a native store.

[kr]: https://docs.rs/crate/keyring/4.2.0
[kr343]: https://github.com/open-source-cooperative/keyring-rs/issues/343
[kr132]: https://github.com/hwchen/keyring-rs/issues/132
[gh]: https://github.com/cli/cli/blob/trunk/internal/keyring/keyring.go
