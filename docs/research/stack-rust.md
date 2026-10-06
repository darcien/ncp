# Research: Rust stack for a new MCP CLI

Status: 2026-10-06. Baseline: [target-requirements.md](../target-requirements.md).
Prior work: [rust-clients.md](rust-clients.md). This report does not repeat it.
Stack under review: [`rmcp`][rmcp] 3.5.1 (client side), `tokio`, `reqwest` +
`rustls`, `keyring-core` stores, a CLI parser, a JSON Schema validator, std
`IsTerminal`.

Method: I read rmcp at tag `rmcp-v3.5.1` (equal to `main` on 2026-10-06)
through `gh api`. Files: 13 client-side source files, `Cargo.toml`, project
documents, CI workflows. I read reqwest 0.13.5, rustls-platform-verifier, rustls-native-certs,
hyper-util and keyring store sources. Data came from the GitHub API (releases, commits,
issues, PRs, advisories), the crates.io API and sparse index, and RustSec. I
also did 4 web searches and 4 web fetches. I did not clone,
build, install or run any project code. I did not run `cargo`. Crate counts
come from my own approximate resolver over the crates.io sparse index (see
section 5). "Unknown" means not verified.

Legend: **Has** = meets the requirement (code read). **CLI** = the SDK gives
the API; the new CLI code does the work. **Fork fix** = a small SDK change
adds it. **Gap** = structural gap.

## Summary

| Item | Result |
|---|---|
| Fit | All listed MUST items map to rmcp APIs. No structural gap. |
| Main gaps | Era-probe timeout fixed at 10 s. No `discover` skip for a cached modern server. DCR `client_secret` not persisted. Cross-origin `resource_metadata` rejected (open P1 bug). |
| Health | 47 rmcp releases and 380 human commits in 12 months. One person (Dale Seo) makes 39% of human commits and most merges. |
| Stability | 12 breaking releases in 12 months (0.9 to 0.17, 1.0, 2.0, 3.0). No LTS line. |
| Spec latency | `2026-07-28`: 0 days. `2025-11-25`: 51 days (first features) to 219 days (conformance fixes). |
| Dependencies | About 200 crates (Linux) and 140 crates (macOS) for a minimal CLI. `reqwest` is mandatory with the `auth` feature. |
| C code | TLS crypto needs a C compiler: `ring` or `aws-lc-sys`. The Linux keyring store can be pure Rust (`zbus`). |
| Size | About 4–5 MB for a similar rmcp CLI (published release assets). |

## 1. Philosophy and stance

| Topic | Finding | Source |
|---|---|---|
| Stated goal | "An official Rust Model Context Protocol SDK implementation with tokio async runtime." It implements `2026-07-28` and stays compatible with `2025-11-25` and earlier. | [README][readme] |
| API style | Trait-based and async. `ServiceExt::serve(transport)` returns a `RunningService`; `Peer` methods send requests. Transports are traits (`Transport`, `IntoTransport`). Public structs are `#[non_exhaustive]`. | [service.rs][svc], [VERSIONING.md][ver] |
| Runtime | tokio only. A `local` feature allows `!Send` futures (`spawn_local`). | [Cargo.toml][cargo], [service.rs L1319][svc-spawn] |
| Feature gating | "Heavily feature-gated" to keep builds lean. Default features are server-side (`base64`, `macros`, `server`). | [DEPENDENCY_POLICY.md][deppol] |
| Refuses | Legacy HTTP+SSE transport (`2024-11-05`): "deliberate non-goal", "will not add the legacy transport back". | [README Transports][readme] |
| Spec changes | Tracks spec revisions through milestones and conformance epics (#977). Tier 1 under SEP-1730. Breaking spec changes ship as a new major version (3.0.0 for `2026-07-28`). | [ROADMAP.md][road] |
| Compatibility promise | SemVer 2.0 for `rmcp` ≥ 1.0. `cargo-semver-checks` and `cargo-public-api` run in CI. MSRV bump counts as breaking (MSRV 1.88). Checks cover default features and all features except `local`. | [VERSIONING.md][ver], [ci.yml][ci] |
| Major history | 0.1.0 2025-03-16 (yanked). 0.2 to 0.17: 16 breaking minors, 2025-07-02 to 2026-02-27. 1.0.0 2026-03-03. 2.0.0 2026-06-29. 3.0.0 2026-07-28. | crates.io API |
| Breaking frequency | 12 breaking releases from 2025-10-06 to 2026-10-06. Majors 1.0 → 2.0: 118 days. 2.0 → 3.0: 29 days. No release on the 2.x line after 3.0.0. | crates.io API, [releases][rel] |
| License | Cargo: `Apache-2.0`. `LICENSE`: transition from MIT to Apache-2.0; old contributions without consent stay MIT. Both permit company use (C3). | [LICENSE][lic] |
| Governance | MCP project (LF Projects). SDK maintainers decide for their SDK; Core Maintainers can veto by majority vote; Lead Maintainers have final authority. | [governance][gov] |
| Named maintainers | MAINTAINERS.md (updated 2026-10-01), Rust SDK: Alex Hancock, Michael Bolin, Bradley Axen, Jack Amadeo, Michael Neale. CODEOWNERS: `@modelcontextprotocol/rust-sdk`. | [MAINTAINERS.md][maint], [CODEOWNERS][codeowners] |
| De facto maintainer | Dale Seo (Apollo GraphQL) is the top committer and merged 7 of 9 sampled PRs. He is not in MAINTAINERS.md. Original author: `4t145`. | `gh api` commits, PRs |
| Decision process | PRs with Conventional Commits. release-plz makes versions and notes. Dependabot weekly (Cargo). Issue triage SLA tracked (95.2%, 20/21 within 2 business days). | [ROADMAP.md][road], [DEPENDENCY_POLICY.md][deppol] |

## 2. Project health

Data: `gh api`, 2026-10-06. Window: 2025-10-06 to 2026-10-06.

| Metric | Value |
|---|---|
| Stars, forks | 3979, 653 |
| rmcp releases (12 months) | 47 tags: 41 stable, 5 beta, 1 alpha |
| Release gap (stable) | Median 7 days (41 gaps). Longest gap: 2026-05-13 to 2026-06-23 (41 days). |
| Commits on `main` (12 months) | 481 total: 380 human, 55 Dependabot, 46 release bot |
| Commits per month | 2025-10: 19, 11: 17, 12: 24, 2026-01: 28, 02: 49, 03: 49, 04: 29, 05: 19, 06: 36, 07: 108, 08: 47, 09: 48 |
| Authors (12 months) | 122 logins (including 2 bots) |
| Top committers (share of 380 human) | DaleSeo 147 (38.7%), alexhancock 31 (8.2%), jamadeo 23 (6.1%), tanish111 7, glicht 7. Top 3 = 53%. |
| Bus factor | Low. One person makes most commits and most merges. Merges sampled: DaleSeo 7, alexhancock 2. |
| Open items | 21 issues, 33 PRs |
| Issue response (13 issues, 2026-09-14 to 10-05) | A bot adds labels in about 1 min. Maintainer triage: 19 min (#1298), 2.2 h (#1301). 6 of 13 closed. External issues closed in 25 h (#1299), 40 h (#1317), 11.4 d (#1268). |
| PR merge time (15 latest non-bot merges) | Median 6.6 days. Range 4.4 h to 57.5 days. External PRs: 25 h, 9.1 d, 10.1 d, 28.4 d. |
| SECURITY.md | Yes. Private GitHub Security Advisory process. No response time stated. |
| CI | `ci.yml`: fmt, clippy (all features), semver checks, public API diff, MSRV, tests, coverage, examples, docs. All jobs on `ubuntu-latest`. No macOS job. CodeQL and OpenSSF Scorecard workflows exist. |
| Conformance | `conformance.yml` runs the official suite (`@modelcontextprotocol/conformance`) for client and server, `2025-11-25` and `2026-07-28`, on each push and PR. ROADMAP: client 100% on both. |

Security advisories (repository and GitHub Advisory Database):

| ID | Side | Severity | Affected | Fixed |
|---|---|---|---|---|
| GHSA-89vp-x53w-74fx / CVE-2026-42559 / RUSTSEC-2026-0189 | Server (DNS rebinding) | High | < 1.4.0 | 1.4.0 |
| GHSA-9pj6-vhgr-3mwh / CVE-2026-63128 | Server (session-table leak, DoS) | High | < 2.0.0 | 2.0.0 |
| GHSA-33f5-2c5q-wgwj / CVE-2026-63127 | Client OAuth (PRM `resource` not validated) | High | < 2.0.0 | 2.0.0 |
| GHSA-c9xm-49cp-xcr9 | Client OAuth (fetches server-controlled `resource_metadata` URL, SSRF) | Medium | < 2.0.0 | 2.0.0 |
| GHSA-9g45-5xwm-f3wc / CVE-2026-64684 | Client HTTP (custom headers sent to cross-origin redirect target) | Medium | < 2.1.0 | 2.1.0 |

No open advisory for 3.x on 2026-10-06. Related crates: `rustls` RUSTSEC-2026-0285
(fixed in 0.23.45, current). `aws-lc-sys` has 5 entries in 2026
(RUSTSEC-2026-0044 to 0048, fixed in 0.38/0.39); they concern AWS-LC X.509,
PKCS7 and AES-CCM code. rustls uses `webpki` for X.509, not AWS-LC.

Spec adoption latency:

| Spec | Final date | First rmcp support | Full support | Latency |
|---|---|---|---|---|
| `2026-07-28` (RC 2026-05-21) | 2026-07-28 | 3.0.0-beta.1, 2026-07-23 | 3.0.0, 2026-07-28 22:52 UTC; conformance 100% | 0 days |
| `2025-11-25` | 2025-11-25 | 0.13.0, 2026-01-15: OIDC discovery (#598), tasks SEP-1686, elicitation SEP-1330 | Version constant: 1.5.0, 2026-04-16 (#802). Model types: 2.0.0, 2026-06-29 (#927). Audit fixes: 2.1.0, 2026-07-02 (#951) | 51 to 219 days |

## 3. Requirement fit

Links point to tag `rmcp-v3.5.1`.

| ID | rmcp API or gap | Result |
|---|---|---|
| P1 | `ClientLifecycleMode::Discover` / `Auto` sends `server/discover`, then per-request `_meta` with version, client info and capabilities ([client.rs L642][cl-mode]). Stateless HTTP is the default (`allow_stateless: true`). | Has |
| P2 | `ClientLifecycleMode::Auto { preferred_versions, legacy_version }` ([L799][cl-auto]). See the era table below. | Has; CLI keeps the result |
| P4 | stdio: `TokioChildProcess` ([child_process.rs][child]). HTTP: `StreamableHttpClientTransport` ([streamable_http_client.rs][shc]). | Has |
| P6 | `Peer::list_tools`, `list_all_tools`, `call_tool`, `call_tool_once` ([client.rs L1935][cl-once]). | Has |
| P8 | `call_tool_once` returns `CallToolResponse::InputRequired`. `call_tool` drives MRTR through `ClientHandler`, max 10 rounds ([README][readme]). | Has |
| P10 | `Peer::send_cancellable_request` returns `RequestHandle` with public `id`, `rx`, `peer` ([service.rs L542][svc-rh], [L853][svc-scr]). `RequestHandle::cancel` sends `notifications/cancelled` ([L666][svc-cancel]). Request timeouts also send it. `call_tool` (auto MRTR) has no handle; dropping its future sends nothing. | CLI (wire Ctrl-C to the handle) |
| A1 | `get_authorization_url`: `PkceCodeChallenge::new_random_sha256()` ([auth.rs L1846][a-url]). | Has |
| A2 | `resolve_metadata`: RFC 9728 first, then RFC 8414 and OIDC well-known paths ([L2423][a-wk], [L2566][a-prm]). Cross-origin `resource_metadata` is rejected ([L1300][a-rrm]; open P1 [#1298][i1298]). AS metadata at IP-literal private or loopback hosts is rejected unless the MCP server is also loopback ([L1281][a-asurl]). | Has, with 2 limits |
| A3 | `resource` in the authorization URL ([L1860][a-res1]), code exchange ([L2149][a-res2]), refresh ([L2308][a-res3]). | Has |
| A4 | `validate_authorization_response_issuer` ([L2065][a-iss]). Strict when AS metadata has `authorization_response_iss_parameter_supported`. `set_allow_missing_issuer` changes it. | Has |
| A5 | `StoredCredentials.issuer`. `initialize_from_store` clears credentials when the issuer changes; it keeps a CIMD client ID ([L1445][a-init]). One store per `AuthorizationManager` (one server). DCR `client_secret` is not in `StoredCredentials` ([L225][a-sc]); DCR asks for a public client (`"none"`, [L1728][a-reg]). Open issue [#784][i784]. | Has (per server); Fork fix for confidential DCR clients |
| A8 | `get_access_token` refreshes 30 s before expiry ([L2215][a-buf]). `AuthClient` refreshes once after a `401` and retries ([auth/streamable_http_client.rs][a-react]). `acquire_refresh_guard` serializes refreshes. | Has |
| A10 | `get_authorization_url` gives the URL. `AuthorizationCallback::from_redirect_url` parses `code`, `state`, `iss` ([L3527][a-cb]). PKCE state lives in `StateStore` (trait; default in memory, [L469][a-ss]). | Has |
| A11 | `StreamableHttpClientTransportConfig.auth_header` (sent as `Bearer`) and `custom_headers` ([L2035][shc-cfg]). Only `accept`, `Mcp-Session-Id`, `Last-Event-Id` are reserved ([http_header.rs][hh]). | Has |
| N2 | reqwest 0.13 reads `ALL_PROXY`, `HTTPS_PROXY`, `HTTP_PROXY`, `NO_PROXY`, upper case first, then lower case ([hyper-util matcher.rs L228][matcher]). Userinfo in the proxy URL becomes proxy auth. rmcp's default transport client and default OAuth client both use reqwest defaults. Ignored when `REQUEST_METHOD` is set (CGI rule). | Has (not tested) |
| N3 | reqwest feature `rustls`/`rustls-no-provider` uses `rustls-platform-verifier` ([reqwest client.rs L759][rq-pv]). macOS: Security.framework trust evaluation. Linux: `rustls-native-certs` loads `SSL_CERT_FILE`/`SSL_CERT_DIR`, else the `openssl-probe` file and directory ([others.rs][pv-others]). | Has (not tested) |
| N5 | `TokioChildProcess::new(impl Into<CommandWrap>)` takes a `tokio::process::Command`. The child inherits the parent environment by default. The CLI controls `env`, `env_clear`. | Has |
| S4 | Same API: the CLI reads secrets from the store and calls `Command::env`. | CLI |

Era detection (P2) details:

| Question | Answer | Source |
|---|---|---|
| How | Sends `server/discover`. A correlated JSON-RPC error that is not a modern code (`MISSING_REQUIRED_CLIENT_CAPABILITY`, `HEADER_MISMATCH`) means legacy. `UnsupportedProtocolVersionError` (-32022) gives a retry with a supported version. | [client.rs L950][cl-disc] |
| HTTP | A 4xx response to `server/discover` (not 401/403, no session) becomes a JSON-RPC error and then a legacy result. Fallback is immediate, with no wait. | [streamable_http_client.rs L375][shc-legacy] |
| Timeout fallback | After 10 s with no answer, `Auto` sends `initialize`. Matches the spec for stdio. | [L657][cl-const] |
| Configurable | No. `DEFAULT_AUTO_DISCOVER_TIMEOUT` is a private `const`; the public entry points pass it ([L738][cl-entry]). Workaround: wrap `Discover` in `tokio::time::timeout`, then start a new transport with `Initialize`. Fork fix: one parameter. | Fork fix |
| Caching | No persistence. The result lives in the `Peer` for one connection. The negotiated version is in `peer_info()`. The CLI saves it per server and next time uses `Initialize` or `Discover` directly. | CLI |
| Skip `discover` | `Discover` mode always sends `server/discover` first. The per-request metadata setter is `pub(crate)` ([service.rs L1038][svc-meta]). Each modern connection costs 1 extra round trip. | Fork fix |

## 4. Extension points

| Need | Mechanism | Result |
|---|---|---|
| Own HTTP client (proxy, roots) | `StreamableHttpClient` trait (`post_message`, `get_stream`, `delete_session`) ([L418][shc-trait]); impl for `reqwest::Client` ([reqwest impl][rq-impl]); `StreamableHttpClientTransport::with_client(client, config)`. `AuthClient<C>` wraps any `C`. | Has |
| Own OAuth HTTP client | `OAuthHttpClient` trait with a redirect policy per request ([auth.rs L100][a-ohc]); `AuthorizationManager::new_with_oauth_http_client`. `with_client(reqwest::Client)` also works but loses the no-redirect client for token requests ([L1507][a-with]). | Has |
| Default HTTP client caveat | rmcp's default transport client sets `pool_max_idle_per_host(0)` and no redirects ([L400][rq-default]). Each POST opens a new TCP+TLS connection. Open issue [#1203][i1203] (redirects). Pass an own client to get pooling. | CLI |
| Own credential store | `CredentialStore` (async): `load`, `save`, `clear`, `on_refresh_token_rejected`, `acquire_refresh_guard` ([L301][a-cs]). `StateStore` for PKCE state. `StoredCredentials` derives `Serialize`; its `Debug` hides tokens. | Has |
| Own stdio spawn | `TokioChildProcess::new` / `builder` take any `tokio::process::Command` (env, cwd, `process_group`) and allow piped stderr ([child_process.rs][child]). Or spawn the process yourself and pass `(stdout, stdin)` as a transport ([transport.rs L35][tr]). | Has |
| Cancellation from Ctrl-C | `tokio::signal::ctrl_c()` plus `RequestHandle` (P10 row). Shutdown: `serve_with_ct(CancellationToken)`, `RunningService::cancel`. Important: Ctrl-C sends SIGINT to the whole foreground process group. Start the stdio server in its own group (`Command::process_group(0)`), or it gets SIGINT before the cancellation message. | CLI |
| F2 timing: DNS | reqwest `ClientBuilder::dns_resolver` takes a custom resolver; wrap the default and time it ([client.rs L2327][rq-dns]). | Has |
| F2 timing: TCP, proxy CONNECT, TLS | reqwest has no per-phase hook. `connector_layer` (tower `Layer`) wraps the full connect: DNS + TCP + CONNECT + TLS ([L2475][rq-layer]). Open reqwest request since 2021-10-12 ([#1352][rq1352]). | Gap in reqwest |
| F2 full split | Own client on hyper + hyper-util: `HttpConnector` with a timed resolver, `proxy::Tunnel` for CONNECT ([docs][hu-tunnel]), `tokio-rustls` for TLS, `proxy::Matcher::from_env` for env rules. Implement `StreamableHttpClient` and `OAuthHttpClient` on it. rmcp's Unix-socket client is a hyper-based model ([unix_socket.rs][unix]). reqwest stays in the build because `auth` needs it. | CLI (medium effort) |
| F2 timing: server time | Wrap the HTTP client: time from request start to response headers. For stdio: time from send to response at the `Peer`. | CLI |
| F2 timing: credential read, server start | Wrap `CredentialStore::load`; time `TokioChildProcess` spawn to first response. | CLI |

Runtime: a current-thread tokio runtime works for the client paths that I read.
rmcp uses `tokio::spawn` (allowed on current-thread) and has no `block_in_place`
in client code. Caveats:

- `TokioChildProcess` drop calls `tokio::spawn` to kill the child ([L47][child-drop]). After runtime shutdown, the kill task does not run. Call `close()` or set `kill_on_drop(true)`.
- Use one runtime for each `reqwest::Client`. A client used across two current-thread runtimes stalls ([reqwest#2501][rq2501]).
- macOS trust evaluation and sync keyring calls block the single thread for their duration. Duration Unknown.
- tokio features: `rt`, `macros`, `time`, `sync`, `process`, `io-util`, `net`, `signal`.

Minimal rmcp feature set for the client:

```toml
rmcp = { version = "3.5", default-features = false, features = [
  "client",
  "transport-child-process",
  "transport-streamable-http-client-reqwest",
  "reqwest-tls-no-provider",   # "reqwest" instead selects aws-lc-rs
  "auth",
] }
reqwest = { version = "0.13", default-features = false, features = ["rustls-no-provider"] }
rustls = { version = "0.23", default-features = false, features = ["ring", "std", "tls12"] }
```

Add `elicitation` for P9 (SHOULD). Do not enable `server`, `macros`,
`schemars`, `which-command`. With `rustls-no-provider`, install the provider
before the first `Client` build, or reqwest panics ([client.rs L2499][rq-prov]).

## 5. Dependencies

Method for counts: my Python script reads the crates.io sparse index
(2026-10-06). It picks the highest matching version and unifies features. It
applies target `cfg` and drops dev-dependencies. Targets: `x86_64-unknown-linux-musl`,
`aarch64-apple-darwin`. Accuracy: approximate, not `cargo tree`. Check: `tokio`
(`rt`, `macros`) gives 7 crates and `clap` (`derive`) gives 17, which match
known graphs.

| Dependency set | Linux | macOS |
|---|---|---|
| rmcp minimal (section 4), TLS without provider | 130 | 132 |
| Same, `reqwest` feature (aws-lc-rs) | 140 | 142 |
| rmcp without `auth` | 109 | 111 |
| reqwest + rustls (`ring`) only | 83 | 85 |
| CLI set A: rmcp minimal + reqwest + rustls `ring` + tokio + serde + `keyring-core` + store + `lexopt` | 202 (zbus store) | 139 (Apple store) |
| Set A with `dbus-secret-service-keyring-store` (`vendored`) | 156 | — |
| Set A with no Linux store | 136 | — |
| Set A with `clap` (`derive`) in place of `lexopt` | 214 | 151 |
| Set A + `jsonschema` 0.58 (`default-features = false`) | 238 | 181 |
| Set A + `boon` 0.6 | 214 | — |

Notes:

- `jsonschema` default features add `resolve-http` (reqwest + aws-lc-rs): 149 crates alone. Turn defaults off (65 crates alone).
- `zbus-secret-service-keyring-store` alone: 104 crates. It pulls `async-io`, `polling`, `rustix`, `nix`, RustCrypto (`aes`, `hkdf`, `num-bigint`).
- Apple store alone: 9 crates (`security-framework`, `core-foundation`).
- `oauth2` 5.0.0 (2025-01-21) brings `rand` 0.8, `sha2`, `base64` 0.22. Its repository had its last commit on 2026-02-22.
- `auth` enables `__reqwest`. reqwest stays in the build even with a custom `OAuthHttpClient`.

C code, `unsafe`, build scripts:

| Item | Finding |
|---|---|
| rmcp `unsafe` | None found in the 13 client-side files that I read. No `forbid(unsafe_code)`. |
| TLS crypto | `ring` 0.17.14: C and assembly through `cc`. `aws-lc-sys` 0.45: C through `cc`. Both need a C compiler for musl cross builds. aws-lc non-FIPS needs no CMake, Go or bindgen ([aws-lc-rs docs][awslc-req]); the `cmake` crate is in the graph, but the tool is not needed. FIPS needs CMake. |
| Pure-Rust TLS provider | `rustls-rustcrypto` 0.0.2-alpha, last update 2024-04-24. Not usable. |
| Linux credential store | `zbus` path: pure Rust, no libdbus. `dbus-secret-service` path: `libdbus-sys` (C). With `vendored` it builds libdbus from source (static); without it, it links `libdbus-1.so` dynamically. |
| macOS | Links `Security.framework` and `CoreFoundation` (system frameworks, dynamic). This is normal for macOS. |
| Proc macros | `serde_derive`, `tokio-macros`, `tracing-attributes`, `thiserror-impl`, `async-trait`, `futures-macro`, `zerovec-derive`, `yoke-derive`. They run at compile time. |
| Build scripts | Not counted. Known C build scripts: `ring` or `aws-lc-sys`, and `libdbus-sys` (dbus path only). |

Licenses (crates.io API): rmcp Apache-2.0; reqwest, rustls-platform-verifier,
keyring-core, stores, security-framework, oauth2, clap, process-wrap: MIT OR
Apache-2.0; rustls: Apache-2.0 OR ISC OR MIT; ring: Apache-2.0 AND ISC;
aws-lc-sys: ISC AND Apache-2.0 AND MIT AND BSD-3-Clause (and others); tokio,
zbus, jsonschema, lexopt: MIT; rustls-webpki: ISC; unicode-ident adds
Unicode-3.0. All permit company use. Full-graph license check: Unknown.

Static build:

- Linux: `x86_64`/`aarch64-unknown-linux-musl` with rustls + `ring` and the zbus store needs only a musl C compiler (for `ring`). No system library at run time.
- macOS: no static build; the binary links system frameworks and `libSystem`.

Keyring crate versions and features:

| Version | Features | Linux backend |
|---|---|---|
| keyring 3.6.3 (2025-07-27) | No default backend. `apple-native`, `windows-native`, `linux-native`, `sync-secret-service`, `async-secret-service`, `crypto-rust`, `crypto-openssl`, `tokio`, `async-io`, `vendored`, `linux-native-sync-persistent`, `linux-native-async-persistent` | `sync-secret-service` = libdbus (C). `async-secret-service` = zbus (Rust). |
| keyring 4.2.0 (2026-08-29) | Wrapper over `keyring-core` 1.0. `v1` (default): Apple `keychain` + Windows + `zbus-secret-service-keyring-store` (`crypto-rust`). `cli`: all stores (sample code). | zbus (Rust) by default |
| Recommended for this CLI | `keyring-core` 1 + `apple-native-keyring-store` (`keychain`) + `zbus-secret-service-keyring-store` (`crypto-rust` or `rt-tokio-crypto-rust`). The keyring README gives this advice for applications. | zbus (Rust) |

## 6. Credential store details

| Topic | Finding | Source |
|---|---|---|
| macOS API | `apple-native-keyring-store` `keychain` module: generic password in the login keychain, `service` and `account` attributes, through the legacy `SecKeychain` API of `security-framework`. The `protected` module (Data Protection keychain) needs a provisioning profile; unsigned apps get `-34018`. | [keychain.rs][apple-kc], [README][apple-readme] |
| macOS ACL, unsigned or ad-hoc | The creating binary goes into the item ACL. A changed binary (each rebuild or update) gets a dialog: "wants to use your confidential information". The maintainer confirms this for keyring (#272). Ad-hoc identity is the cdhash, which changes with each build. A Developer ID signature gives a stable designated requirement. | [keyring#272][kr272], [Apple forum][apple-acl], [HackTricks][ht-kc] |
| macOS for agents | A dialog blocks a non-interactive agent. In an SSH session the login keychain can be locked. Exact errors: Unknown (not tested). | Reasoning |
| macOS ACL control | The store has no API to set an ACL. `security-framework` ACL support: Unknown. | — |
| Linux no Secret Service | `SecretService::connect` needs a D-Bus session bus. With no bus (SSH, container, server), connect fails and the store returns an error. A locked collection needs an unlock prompt. Exact error variant: Unknown (not tested). No automatic fallback in `keyring-core`. | [service.rs][zbus-svc] |
| Linux options | `linux-keyutils-keyring-store` (kernel keyring, no daemon; lifetime rules: Unknown), or a `0600` file after opt-in (baseline S2). | [keyring Cargo.toml][kr-cargo] |
| Item naming | Apple: `service`, `account`. Secret Service: attributes `service`, `username`, optional `target`; default label `keyring:{user}@{service}`. Proposal: service `<cli-name>`, account `<server-url>#oauth` and `<server-url>#header`. | [cred.rs][zbus-cred] |
| Item size | One JSON `StoredCredentials` per server (client ID, tokens, scopes, issuer). Size limits: Unknown. | — |
| Concurrency | rmcp `acquire_refresh_guard` keeps a lock over load, refresh and save. The store needs a cross-process lock (file lock) for parallel CLI runs with rotating refresh tokens. | [auth.rs L301][a-cs] |
| Sync vs async | `keyring-core` API is sync. Call it inside the async `CredentialStore` methods; a short block is acceptable on a current-thread runtime. zbus blocking API with the `rt-tokio` feature inside a tokio runtime: Unknown. | — |
| Maintenance | keyring-rs: 768 stars, push 2026-10-03, 7 open issues; Dan Brotsky 462 of top commits. keyring-core: 39 stars. apple store: 7 stars, 38 of 40 commits by Brotsky. zbus store: 6 stars, 37 of 39 by Brotsky. `security-framework` (kornelski): 306 stars, push 2026-08-14. Bus factor: 1 for the stores. | `gh api` 2026-10-06 |
| Advisories | None for keyring, keyring stores, zbus, secret-service. `security-framework`: RUSTSEC-2017-0003 (fixed 2017). | RustSec |
| Alternative | `oo7` 0.6.0 (linux-credentials, 398 stars): pure-Rust Secret Service client with a file backend. Not a `keyring-core` store. | crates.io |

## 7. Startup and size

No build. Numbers come from published assets or reasoning.

| Item | Value | Basis |
|---|---|---|
| Binary size, reference | [lloydzhou/mcpc][mcpc] 0.2.0: rmcp 3.3 (client, child process, HTTP, `reqwest` with aws-lc, default features on), tokio multi-thread; `opt-level = "s"`, LTO, strip. 4.1 MB macOS arm64, 4.9 MB Linux amd64 (glibc). | Release assets, 2026-09-12 |
| Other references | mcpcall 0.4.1 (rmcp 1.7, not stripped): 14–17 MB. mcpal 0.4.3 (rmcp 1.6 + auth + keyring 3 + axum): 4.7–5.3 MB `.tar.xz`. avelino/mcp 0.6.2: 20–24 MB. | Release assets |
| Expected for this CLI | About 5–7 MB stripped, with auth, keyring and a parser. Estimate, not measured. `ring` vs aws-lc size difference: Unknown. | Reasoning |
| Process start | avelino/mcp (Rust) `--version`: 2.7 ms (baseline section 6). | Baseline |
| tokio runtime | Current-thread: no worker threads. Multi-thread: one thread per CPU. Cost in µs: Unknown. uv saw a 1.08x speedup after a switch to current-thread. Handle `--version` and `--help` before the runtime starts. | [search][tokio-cost] |
| TLS roots, Linux | `Verifier::new` loads system roots at each `reqwest::Client` build: the bundle file plus each file in the cert directory ([others.rs L92][pv-others], [native-certs][nc-lib]). The crate docs warn: "can be expensive", "~300KB disk file". rmcp defaults build 3 clients (transport, OAuth follow, OAuth no-redirect), so 3 loads. Fix: build one verifier or `rustls::ClientConfig` and share it (`tls_backend_preconfigured`). Time in ms: Unknown. | Source read |
| TLS roots, macOS | No root load at build. Trust evaluation runs at handshake through Security.framework (IPC to `trustd`). The OS can fetch revocation data; the verifier does not limit it ([apple.rs L186][pv-apple]). Cost: Unknown. | Source read |
| Crypto provider | rustls provider install: a static struct. Cost: negligible (reasoning). | Reasoning |
| Network round trips, HTTP | Modern server: `server/discover` + call = 2 POSTs. Legacy server: `discover` (4xx) + `initialize` + `initialized` + call = 4 POSTs. With rmcp's default client, each POST has a new TCP+TLS handshake. OAuth: `initialize_from_store` runs metadata discovery when metadata is not set ([L1445][a-init]); cache `AuthorizationMetadata` (it derives `Serialize`) and call `set_metadata`. | Source read |
| Credential read | macOS: IPC to `securityd`. Linux: D-Bus connect plus a DH session key exchange. Cost: Unknown. | Reasoning |

## 8. Risks and unknowns

1. Bus factor: one person not in MAINTAINERS.md (Dale Seo) makes 39% of human commits and most merges. Of the 5 listed maintainers, Alex Hancock (31 commits) and Jack Amadeo (23) are active committers.
2. Breaking churn: 12 breaking releases in 12 months, and 3.0 came 29 days after 2.0. No LTS line. Plan for one major upgrade each 1–4 months, or pin.
3. `2025-11-25` support took 51–219 days. The `2026-07-28` support took 0 days. The next spec revision latency: Unknown.
4. Cross-origin `resource_metadata` rejection breaks centralized PRM servers (open P1 [#1298][i1298]; the report names `api.anthropic.com` → `claude.ai`). PR #1302 is open.
5. AS metadata at IP-literal private hosts is rejected. Company AS at `10.x.x.x` URLs fails. Host names that resolve to private IPs pass.
6. Era probe timeout is fixed at 10 s. A silent legacy stdio server costs 10 s on the first run. No API to skip `discover` for a cached modern server.
7. DCR `client_secret` is not persisted ([#784][i784]). Refresh fails for an AS that issues a secret to a DCR client.
8. Default HTTP client: no pooling, no redirects ([#1203][i1203]). Use an own client; then keep the GHSA-9g45 fix in mind (no custom headers on cross-origin redirects).
9. F2 per-phase timing needs an own hyper-based HTTP client. reqwest has no hook for TCP, CONNECT and TLS phases.
10. macOS Keychain dialogs after each update of an unsigned or ad-hoc signed binary. Agents cannot answer dialogs. Developer ID signing changes this; cost and process: outside this report.
11. Linux with no Secret Service fails. The store crates have one maintainer each and few stars.
12. CI runs on Linux only. macOS child-process and signal paths have no CI in rmcp.
13. Unbounded line buffer in `AsyncRwTransport::receive` (open [#1030][i1030]). A bad stdio server can use much memory in the CLI.
14. Client accepts a JSON-RPC response with both `result` and `error` (open [#1283][i1283]).
15. Ctrl-C reaches the stdio server first unless the CLI starts it in its own process group.
16. Dependency count: about 200 crates on Linux. The zbus store alone adds about 66 crates to set A.
17. Unknown: startup cost in ms (tokio, Linux roots, keychain) and the binary size of this exact set. Also Unknown: Keychain errors in SSH sessions, zbus `rt-tokio` blocking behavior, full-graph licenses.

## 9. Verdict

- Strength: rmcp covers every listed MUST through public APIs. It has era detection, MRTR, cancellation, the full OAuth client (PKCE, RFC 9728/8414/OIDC, RFC 8707, RFC 9207, CIMD, DCR, step-up) and passes both conformance suites.
- Strength: good injection points: `StreamableHttpClient`, `OAuthHttpClient`, `CredentialStore`, `StateStore`, and a `tokio::process::Command`-based spawn. A current-thread runtime works.
- Weakness: high API churn (12 breaking releases in 12 months) and a bus factor near 1 in daily work.
- Weakness: F2 timing needs an own hyper client; reqwest and C-based TLS crypto stay in the build; about 200 crates on Linux.
- Open question: fork or wait? Four small SDK changes help the CLI: probe timeout, `discover` skip, DCR secret, cross-origin PRM. A fork must also follow a major rmcp release every 1–4 months.

## Sources

- rust-sdk: [repository][rmcp], [README][readme], [ROADMAP.md][road], [VERSIONING.md][ver], [DEPENDENCY_POLICY.md][deppol], [SECURITY.md][sec], [Cargo.toml][cargo], [LICENSE][lic], [CODEOWNERS][codeowners], [ci.yml][ci], [conformance.yml][conf], [releases][rel], [security advisories][adv]
- rmcp source (tag `rmcp-v3.5.1`): [service/client.rs][cl], [service.rs][svc], [transport.rs][tr], [child_process.rs][child], [streamable_http_client.rs][shc], [reqwest/streamable_http_client.rs][rq-impl], [auth.rs][auth], [auth/streamable_http_client.rs][a-react], [http_header.rs][hh], [unix_socket.rs][unix]
- rust-sdk issues and PRs: [#1298][i1298], [#1203][i1203], [#784][i784], [#1030][i1030], [#1283][i1283], [#802][p802], [#927][p927], [#951][p951], [#598][p598], [#815][i815]
- MCP: [governance][gov], [MAINTAINERS.md][maint], [versioning and compatibility][spec-ver], [2026-07-28 announcement (AAIF)][aaif]
- RustSec: [RUSTSEC-2026-0189][rs189], [rustls RUSTSEC-2026-0285][rs285], [aws-lc-sys advisories][rs-awslc]
- reqwest: [v0.13.5 Cargo.toml][rq-cargo], [v0.13.5 client.rs][rq-client], [#1352][rq1352], [#2501][rq2501]
- TLS: [rustls-platform-verifier others.rs][pv-others], [apple.rs][pv-apple], [rustls-native-certs lib.rs][nc-lib], [aws-lc-rs Linux requirements][awslc-req], [hyper-util proxy matcher][matcher], [hyper-util proxy tunnel][hu-tunnel]
- Keyring: [keyring-rs][kr], [keyring Cargo.toml][kr-cargo], [keyring#272][kr272], [apple-native-keyring-store][apple-readme], [keychain.rs][apple-kc], [zbus store cred.rs][zbus-cred], [zbus store service.rs][zbus-svc], [secret-service-rs Cargo.toml][ss-cargo]
- macOS ACL: [Apple developer forum thread 691160][apple-acl], [HackTricks macOS Keychain][ht-kc]
- Size and startup references: [lloydzhou/mcpc][mcpc], [loonghao/mcpcall][mcpcall], [pawelb0/mcpal][mcpal], [tokio runtime search result (uv, sccache)][tokio-cost]
- Data APIs: crates.io API (`/api/v1/crates/...`), crates.io sparse index (`index.crates.io`), GitHub REST API

[rmcp]: https://github.com/modelcontextprotocol/rust-sdk
[readme]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/README.md
[road]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/ROADMAP.md
[ver]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/VERSIONING.md
[deppol]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/DEPENDENCY_POLICY.md
[sec]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/SECURITY.md
[cargo]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/Cargo.toml
[lic]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/LICENSE
[codeowners]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/.github/CODEOWNERS
[ci]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/.github/workflows/ci.yml
[conf]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/.github/workflows/conformance.yml
[rel]: https://github.com/modelcontextprotocol/rust-sdk/releases
[adv]: https://github.com/modelcontextprotocol/rust-sdk/security/advisories
[cl]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/service/client.rs
[cl-mode]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/service/client.rs#L642
[cl-const]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/service/client.rs#L657
[cl-entry]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/service/client.rs#L738
[cl-auto]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/service/client.rs#L799
[cl-disc]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/service/client.rs#L950
[cl-once]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/service/client.rs#L1935
[svc]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/service.rs
[svc-rh]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/service.rs#L542
[svc-cancel]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/service.rs#L666
[svc-scr]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/service.rs#L853
[svc-meta]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/service.rs#L1038
[svc-spawn]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/service.rs#L1319
[tr]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport.rs#L35
[child]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/child_process.rs
[child-drop]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/child_process.rs#L47
[shc]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/streamable_http_client.rs
[shc-legacy]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/streamable_http_client.rs#L375
[shc-trait]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/streamable_http_client.rs#L418
[shc-cfg]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/streamable_http_client.rs#L2035
[rq-impl]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/common/reqwest/streamable_http_client.rs#L49
[rq-default]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/common/reqwest/streamable_http_client.rs#L400
[hh]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/common/http_header.rs#L20
[unix]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/common/unix_socket.rs
[auth]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs
[a-ohc]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L100
[a-sc]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L225
[a-cs]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L301
[a-ss]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L469
[a-asurl]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L1281
[a-rrm]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L1300
[a-init]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L1445
[a-with]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L1507
[a-reg]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L1728
[a-url]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L1846
[a-res1]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L1860
[a-iss]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L2065
[a-res2]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L2149
[a-buf]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L2215
[a-res3]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L2308
[a-wk]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L2423
[a-prm]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L2566
[a-cb]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L3527
[a-react]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/common/auth/streamable_http_client.rs
[i1298]: https://github.com/modelcontextprotocol/rust-sdk/issues/1298
[i1203]: https://github.com/modelcontextprotocol/rust-sdk/issues/1203
[i784]: https://github.com/modelcontextprotocol/rust-sdk/issues/784
[i1030]: https://github.com/modelcontextprotocol/rust-sdk/issues/1030
[i1283]: https://github.com/modelcontextprotocol/rust-sdk/issues/1283
[i815]: https://github.com/modelcontextprotocol/rust-sdk/issues/815
[p802]: https://github.com/modelcontextprotocol/rust-sdk/pull/802
[p927]: https://github.com/modelcontextprotocol/rust-sdk/pull/927
[p951]: https://github.com/modelcontextprotocol/rust-sdk/pull/951
[p598]: https://github.com/modelcontextprotocol/rust-sdk/pull/598
[gov]: https://modelcontextprotocol.io/community/governance
[maint]: https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/MAINTAINERS.md
[spec-ver]: https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning
[aaif]: https://aaif.io/blog/mcp-2026-07-28-whats-changing-and-how-to-migrate
[rs189]: https://github.com/rustsec/advisory-db/blob/main/crates/rmcp/RUSTSEC-2026-0189.md
[rs285]: https://github.com/rustsec/advisory-db/blob/main/crates/rustls/RUSTSEC-2026-0285.md
[rs-awslc]: https://github.com/rustsec/advisory-db/tree/main/crates/aws-lc-sys
[rq-cargo]: https://github.com/seanmonstar/reqwest/blob/v0.13.5/Cargo.toml
[rq-client]: https://github.com/seanmonstar/reqwest/blob/v0.13.5/src/async_impl/client.rs
[rq-pv]: https://github.com/seanmonstar/reqwest/blob/v0.13.5/src/async_impl/client.rs#L759
[rq-dns]: https://github.com/seanmonstar/reqwest/blob/v0.13.5/src/async_impl/client.rs#L2327
[rq-layer]: https://github.com/seanmonstar/reqwest/blob/v0.13.5/src/async_impl/client.rs#L2475
[rq-prov]: https://github.com/seanmonstar/reqwest/blob/v0.13.5/src/async_impl/client.rs#L2499
[rq1352]: https://github.com/seanmonstar/reqwest/issues/1352
[rq2501]: https://github.com/seanmonstar/reqwest/issues/2501
[pv-others]: https://github.com/rustls/rustls-platform-verifier/blob/main/rustls-platform-verifier/src/verification/others.rs
[pv-apple]: https://github.com/rustls/rustls-platform-verifier/blob/main/rustls-platform-verifier/src/verification/apple.rs
[nc-lib]: https://github.com/rustls/rustls-native-certs/blob/main/src/lib.rs
[awslc-req]: https://aws.github.io/aws-lc-rs/requirements/linux.html
[matcher]: https://github.com/hyperium/hyper-util/blob/master/src/client/proxy/matcher.rs
[hu-tunnel]: https://docs.rs/hyper-util/latest/hyper_util/client/legacy/connect/proxy/index.html
[kr]: https://github.com/open-source-cooperative/keyring-rs
[kr-cargo]: https://github.com/open-source-cooperative/keyring-rs/blob/main/Cargo.toml
[kr272]: https://github.com/open-source-cooperative/keyring-rs/issues/272
[apple-readme]: https://github.com/open-source-cooperative/apple-native-keyring-store
[apple-kc]: https://github.com/open-source-cooperative/apple-native-keyring-store/blob/main/src/keychain.rs
[zbus-cred]: https://github.com/open-source-cooperative/zbus-secret-service-keyring-store/blob/main/src/cred.rs
[zbus-svc]: https://github.com/open-source-cooperative/zbus-secret-service-keyring-store/blob/main/src/service.rs
[ss-cargo]: https://github.com/hwchen/secret-service-rs/blob/main/Cargo.toml
[apple-acl]: https://developer.apple.com/forums/thread/691160
[ht-kc]: https://book.hacktricks.xyz/macos-hardening/macos-red-teaming/macos-keychain
[mcpc]: https://github.com/lloydzhou/mcpc
[mcpcall]: https://github.com/loonghao/mcpcall
[mcpal]: https://github.com/pawelb0/mcpal
[tokio-cost]: https://users.rust-lang.org/t/choosing-rt-multi-thread-versus-rt-option-in-tokio/106125
