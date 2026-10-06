# Test harness: official MCP test parts for `ncp`

Status: research, 2026-10-07. Baseline: `target-requirements.md` (2026-10-06).

Method: read-only `gh api` reads of `modelcontextprotocol/conformance` (main
`c37eec888e`, 2026-10-01), `modelcontextprotocol/rust-sdk` (tag
`rmcp-v3.5.1`, same content as main `79437f2`), `modelcontextprotocol/servers`
(main `5abed86c53`) and `typescript-sdk` (branch `v1.x`). npm registry
metadata through WebFetch. One spec page through WebFetch. Nothing was cloned,
built, installed or run. "Unknown" means not verified.

## 1. Summary

| Item | Finding |
|---|---|
| Main official part | `@modelcontextprotocol/conformance`. In client mode, the suite starts a local mock MCP server (and a mock authorization server for `auth/*`), then starts the client under test. |
| Client interface | Command line plus environment. The suite appends the server URL as the last argument. It sets `MCP_CONFORMANCE_SCENARIO`, `MCP_CONFORMANCE_PROTOCOL_VERSION` and, for some scenarios, `MCP_CONFORMANCE_CONTEXT` (JSON). |
| Transport coverage | Streamable HTTP only. No stdio scenario exists. |
| Version to pin | `0.2.0-alpha.12` (npm `alpha` tag, 2026-10-01). The npm `latest` tag is `0.1.16` (2026-03-27). It predates `2026-07-28`. |
| rust-sdk pattern | One test binary `conformance-client` (`conformance/src/bin/client.rs`). It routes on `MCP_CONFORMANCE_SCENARIO`. CI runs `npx -y @modelcontextprotocol/conformance@0.2.0-alpha.10 client --command <bin> --suite all --spec-version <v>`. |
| Reuse for `ncp` | Reuse the pattern, not the binary. `ncp` needs a small adapter binary that maps each scenario to `ncp` commands (black-box test of the CLI). |
| Local server fixtures | rmcp `conformance-server` (HTTP, both eras). `server-everything` (TypeScript, legacy only, stdio and HTTP). rmcp example servers (stdio and HTTP). |
| Main gaps | stdio (all), cancellation (P10), token refresh (A8), static headers (A11), large results, era cache (P2), all S* and N* items. |

## 2. Conformance suite

### 2.1 How a client is tested

| Item | Finding | Source |
|---|---|---|
| Role | The suite acts as the server. It starts an Express server on `listen(0)` and uses `http://localhost:<port>`. | `src/scenarios/client/auth/helpers/serverLifecycle.ts` |
| Command | `conformance client --command "<cmd>" (--scenario <name> \| --suite <s> \| --requirements <rev>) [--spec-version <v>] [--expected-failures <yml>] [--timeout <ms>] [-o <dir>]` | README, `src/index.ts` |
| Process start | `spawn(executable, args, {shell: true, env})`. The command string is split on spaces. The URL is the last argument. | `src/runner/client.ts` L25–L62 |
| Environment | The child gets the full environment of the suite, plus the `MCP_CONFORMANCE_*` variables. | `src/runner/client.ts` L47–L55 |
| Timeout | 30 000 ms for each scenario (default). The suite kills the client after the timeout. | `src/runner/client.ts` L65–L68 |
| Era rule | "Clients under test must derive the lifecycle from the protocol version they are asked to run." `2025-11-25` and older use `initialize`. `2026-07-28` uses `server/discover` and `_meta` in each request. | README |
| Checks | Scenario checks, plus `wire-schema-valid` for each JSON-RPC message (since `0.2.0-alpha.11`). JSON schemas are in the package (`src/spec-types/*.schema.json`). | README, `package.json` history |
| Results | `results/<scenario>-<timestamp>/checks.json`, `stdout.txt`, `stderr.txt`, or the `-o` directory. | README |
| Baseline file | YAML `client:` list. Per-check entries `<scenario>:<check-id>`. A stale entry fails the run. | README |
| Scored sets | `requirements/2025-11-25.yaml`, `requirements/2026-07-28.yaml` (frozen). `--requirements` exists since `0.2.0-alpha.11` (commit `232a901445`, 2026-08-07). | repo |
| OAuth login | The mock authorization server approves at once. `GET /authorize` returns a `302` to `redirect_uri` with `code`, `state` and (by scenario) `iss`. The client fetches the URL with no redirect and reads `Location`. | `createAuthServer.ts` L409–L494, `examples/clients/typescript/helpers/ConformanceOAuthProvider.ts` |
| CIMD | The client must use `client_id = https://conformance-test.local/client-metadata.json`. The mock server does not fetch it. | `src/scenarios/client/auth/basic-cimd.ts` |
| Pre-registration data | `MCP_CONFORMANCE_CONTEXT` = `{name, client_id, client_secret, issuer}`. | `src/schemas/context.ts` |

### 2.2 Client scenarios by version

`--suite all --spec-version <v>` selects the scenarios whose range contains
`<v>`. The "Required" column comes from the frozen requirement sets.

| Scenario | 2025-11-25 | 2026-07-28 | Required | Baseline items |
|---|---|---|---|---|
| `initialize` | Yes | No (removed) | 2025-11-25 | P2 |
| `tools_call` | Yes | Yes | both | P6, P1/P2 |
| `elicitation-sep1034-client-defaults` | Yes | No (removed) | 2025-11-25 | P9 |
| `sse-retry` | Yes | No (removed) | 2025-11-25 | P4 |
| `request-metadata` | No | Yes | 2026-07-28 | P1 |
| `sep-2322-client-request-state` (MRTR) | No | Yes | 2026-07-28 | P8, P9 |
| `http-standard-headers` | No | Yes | 2026-07-28 | P1, P4 |
| `http-custom-headers`, `http-invalid-tool-headers` | No | Yes | 2026-07-28 | P1, P6 |
| `json-schema-ref-no-deref` | No | Yes | 2026-07-28 | N1, I5 |
| `json-schema-2020-12-preservation` | Yes | Yes | not scored | I5 |
| `auth/2025-03-26-oauth-metadata-backcompat`, `auth/2025-03-26-oauth-endpoint-fallback` | Only `2025-03-26` | No | no | P3 |

### 2.3 Auth scenarios

All auth scenarios use a local mock authorization server. It checks PKCE
(`pkce-code-challenge-sent`, `pkce-s256-method-used`,
`pkce-verifier-matches-challenge`), DCR (`client-registration`) and the
`resource` parameter (`resourceParameterChecks.ts`).

| Scenario | 2025-11-25 | 2026-07-28 | Tests | Baseline items |
|---|---|---|---|---|
| `auth/metadata-default` | Yes | Yes | PRM path `/.well-known/oauth-protected-resource/mcp` in `WWW-Authenticate`; RFC 8414 root; DCR | A1, A2, A3, A6 |
| `auth/metadata-var1` | Yes | Yes | PRM not in header; OIDC discovery | A2 |
| `auth/metadata-var2` | Yes | Yes | Root PRM; RFC 8414 with path `/tenant1` | A2, A3 |
| `auth/metadata-var3` | Yes | Yes | Custom PRM URL; OIDC with path prefix | A2 |
| `auth/basic-cimd` | Yes | Yes | CIMD in place of DCR | A6, A7 |
| `auth/pre-registration` | Yes | Yes | Pre-registered client from context | A6 |
| `auth/scope-from-www-authenticate`, `-from-scopes-supported`, `-omitted-when-undefined` | Yes | Yes | Scope selection | A9 |
| `auth/scope-step-up` | Yes | Yes | `403 insufficient_scope`, request old and new scopes | A9 |
| `auth/scope-retry-limit` | Yes | Yes | Retry limit | A9 |
| `auth/token-endpoint-auth-basic`, `-post`, `-none` | Yes | Yes | Token endpoint client authentication | A5 |
| `auth/resource-mismatch` | No | Yes | Reject PRM `resource` that does not match | A3 |
| `auth/offline-access-scope`, `auth/offline-access-not-supported` | No | Yes | `offline_access` and `refresh_token` grant in metadata. No refresh grant is sent. | A8 (part) |
| `auth/authorization-server-migration` | No | Yes | Register again at a new AS; no reuse of old credentials | A5 |
| `auth/iss-supported`, `-not-advertised`, `-supported-missing`, `-wrong-issuer`, `-unexpected`, `-normalized` | No | Yes | RFC 9207 `iss` | A4 |
| `auth/metadata-issuer-mismatch` | No | Yes | RFC 8414 issuer echo | A2, A4 |
| `auth/client-credentials-*`, `auth/enterprise-managed-authorization`, `auth/dpop*`, `auth/wif-jwt-bearer` | extension | extension | Not in scope (baseline 3.2) | — |

No scenario sends a `refresh_token` grant. A search of the repository for
`refresh_token` found only metadata checks and DPoP code.

### 2.4 Runtime, license, pinning

| Item | Finding |
|---|---|
| Runtime | Node.js. Build target `node20` (`package.json` build script). No `engines` field in `package.json`. The GitHub Action uses Node 22 by default. |
| Dependencies | `@modelcontextprotocol/sdk ^1.29.0`, `express`, `ajv`, `jose`, `undici`, `@octokit/rest`, others (`0.2.0-alpha.12`). |
| Network at run time | Client mode: no fetch found in `src/runner/client.ts` or in the client scenarios read. Servers bind on port 0. `@octokit/rest` is for `tier-check` (GitHub API). `sdk` clones repositories. Do not use these two subcommands. Full code base: Unknown. The sandbox in section 6 detects any fetch. |
| Network at install | `npx -y` downloads from the npm registry. Install once, outside the sandbox, from a lock file. |
| License | `package.json`: `MIT`. `LICENSE`: MIT to Apache-2.0 transition text. GitHub: `NOASSERTION`. |
| Versions | npm `latest` = `0.1.16`. npm `alpha` = `0.2.0-alpha.12` (2026-10-01). `0.2.0-alpha.11` (2026-08-07). `0.2.0-alpha.10` (2026-07-27, the anchor of the `2026-07-28` set). |
| Git tags | Tags stop at `v0.1.16`. No tag exists for `0.2.0-alpha.*`. |
| GitHub Action | `modelcontextprotocol/conformance@v0.1.x`. It has no `spec-version` or `requirements` input. Do not use it. |
| Pin method | Exact npm version in a `package.json` plus `package-lock.json` (integrity hashes) in `tests/conformance/`. Run `node node_modules/@modelcontextprotocol/conformance/dist/index.js`. Alternative: git commit SHA. |

## 3. How rust-sdk runs the suite

| Item | Finding | Source |
|---|---|---|
| Crate | `mcp-conformance`, `publish = false`, two binaries: `conformance-client`, `conformance-server`. | `conformance/Cargo.toml` |
| rmcp features | `client`, `server`, `elicitation`, `auth`, `auth-client-credentials-jwt`, `auth-enterprise-managed`, `request-state`, streamable HTTP client (reqwest) and server. | `conformance/Cargo.toml` |
| Client entry | Reads `MCP_CONFORMANCE_SCENARIO` (default `initialize`), URL from `argv[1]`, context from `MCP_CONFORMANCE_CONTEXT`. Own timeout `MCP_CONFORMANCE_TIMEOUT_SECS` (25 s) so the suite never kills it. | `client.rs` L1068–L1101 |
| Era choice | `MCP_CONFORMANCE_PROTOCOL_VERSION` ≥ `2026-07-28` gives `ClientLifecycleMode::Discover`, else `Initialize`. No auto detection in conformance runs. | `client.rs` L996–L1024 |
| Scenario map | One `match` on the scenario name. Auth scenarios share `run_auth_client`. Step-up, retry limit, migration and pre-registration have own functions. | `client.rs` L1104–L1206 |
| Headless login | `reqwest` with `redirect::Policy::none()`, read `Location`, then `AuthorizationCallback::from_redirect_url`. | `client.rs` L289–L300, L855–L866 |
| CIMD | `.with_client_metadata_url("https://conformance-test.local/client-metadata.json")` | `client.rs` L212, L281 |
| Migration | Uses raw `reqwest` and `AuthorizationManager` with `InMemoryCredentialStore`, three rounds in one process. | `client.rs` L568–L630 |
| CI | `ubuntu-latest` only. `CONFORMANCE_VERSION: 0.2.0-alpha.10`. Client legs: `--suite all --spec-version 2025-11-25`, the same for `2026-07-28`, and `--suite extensions` with a baseline. | `.github/workflows/conformance.yml` |
| Client baseline | `auth/enterprise-managed-authorization`, `auth/dpop`, `auth/dpop-nonce`, `auth/wif-jwt-bearer` (extensions only). | `conformance/expected-failures-extensions.yaml` |
| Server | `conformance-server` on `127.0.0.1:$PORT/mcp` (default 8001). One process serves both eras. | `server.rs` L1733–L1761, `known-sdks.ts` comment |
| License | Workspace `license = "Apache-2.0"`; `LICENSE` has the MIT to Apache-2.0 transition text. | `Cargo.toml`, `LICENSE` |

Reuse for `ncp`:

| Part | Reuse | Reason |
|---|---|---|
| CI shape (two legs, `--spec-version`, `-o`, baseline) | Copy | Same suite, same needs. Add a macOS job. Pin `0.2.0-alpha.12` or later to get wire-schema checks and `--requirements`. |
| `client.rs` scenario router | Copy the idea, not the code | `client.rs` tests rmcp APIs. `ncp` must test the CLI. The adapter calls the `ncp` binary. |
| Headless login code | Copy the idea | In `ncp`, the adapter does the `GET` and gives the `Location` value to `ncp login --no-browser` on stdin. This also tests A10. |
| `conformance-server` | Vendor `server.rs` at a pinned tag (Apache-2.0 notice) | It is the best local HTTP server for both eras. It is not on crates.io. |

Adapter contract for `ncp` (proposal):

| Scenario group | Adapter action |
|---|---|
| All | Read the URL from the last argument. Map `MCP_CONFORMANCE_PROTOCOL_VERSION` to an `ncp` option that pins the era (needed by the era rule in 2.1). Use a temporary `HOME` and the opt-in file credential store (S2). |
| `tools_call`, `http-*`, `request-metadata`, `json-schema-*` | `ncp tools list` then `ncp tools call` with arguments from the schema or from context `toolCalls`. |
| `sep-2322-client-request-state`, `elicitation-sep1034-client-defaults` | `ncp tools call` with a non-interactive answer source (for example an answers file). Without it, `ncp` stops with the "input required" exit code (P8) and the scenario fails. |
| `auth/*` | `ncp login --no-browser`; the adapter fetches the printed URL with no redirect and writes `Location` to stdin. Then `ncp tools list`. |
| `auth/pre-registration` | Pass `client_id`, `client_secret`, `issuer` from context as `ncp` configuration. |
| `auth/basic-cimd` | Set the CIMD URL setting (A7) to the fixed URL. |
| `auth/authorization-server-migration` | Run `ncp` several times with the same `HOME`. This tests A5 across processes. |

## 4. Reference test servers

| Server | Origin | Language | Transports | Eras | Gives | Notes |
|---|---|---|---|---|---|---|
| `conformance-server` | rust-sdk `conformance/src/bin/server.rs` | Rust | Streamable HTTP | Both | `test_tool_with_progress`, `test_elicitation`, `test_input_required_result_*` (MRTR), `test_sampling`, `test_tool_with_logging`, resources, prompts, `test_custom_header` | Binds `127.0.0.1`. Not published; vendor it. |
| Conformance mock servers | conformance `src/scenarios/client/*` | Node | Streamable HTTP | By scenario | Section 2 | Only inside a suite run. |
| `server-everything` | servers `src/everything`, npm `@modelcontextprotocol/server-everything@2026.8.31` | Node | stdio, SSE, Streamable HTTP (`PORT`, default 3001) | Legacy only (TS SDK v1; `LATEST_PROTOCOL_VERSION = 2025-11-25`) | `trigger-long-running-operation` (progress), `trigger-elicitation-request` (form), `trigger-url-elicitation` (URL mode), `get-env`, resources, prompts, Tasks (old SEP-1686) | Good for the era fallback path. `gzip-file-as-resource` fetches a URL: set `GZIP_ALLOWED_DOMAINS` or do not call it. The long-running tool ignores cancellation. |
| conformance `examples/servers/typescript/everything-server.ts` | conformance repo | Node | Streamable HTTP | Legacy (TS SDK `^1.29.0`) | Same tool names as `conformance-server` | Not on npm. |
| rmcp `counter_stdio`, `memory_stdio`, `prompt_stdio`, `elicitation_stdio`, `task_stdio`, `progress_demo` (stdio or HTTP) | rust-sdk `examples/servers` (`publish = false`) | Rust | stdio, HTTP | Both (rmcp test `high_level_server_accepts_discover_startup_without_initialize`; not run) | Tools, prompts, elicitation (legacy push), Tasks, progress | Vendor or rebuild from rmcp APIs. |
| rmcp `mrtr.rs` | rust-sdk examples | Rust | In-memory | `2026-07-28` | MRTR with `RequestStateCodec` | Pattern for an own MRTR fixture. |
| rmcp `simple_auth_streamhttp` | rust-sdk examples | Rust | HTTP | Both | Static bearer token | Pattern for A11. |
| rmcp `complex_auth_streamhttp` | rust-sdk examples | Rust | HTTP on `127.0.0.1:3000` | Both | AS with DCR, metadata, PRM | Human approval form. Rejects `refresh_token` grant. No PKCE check found. Not a good mock. |
| rmcp `scripts/keycloak-oauth-fixture.sh` | rust-sdk | Docker | HTTP | — | Real refresh tokens (Keycloak 26.0) | Pulls `quay.io/keycloak/keycloak:26.0`. Used by `#[ignore]` test `test_live_oauth_refresh.rs`. Optional tier only. |

Fit to the needs listed in the task:

| Need | Official part | Status |
|---|---|---|
| Modern stdio | rmcp example stdio servers | Partial. Not tested as such by rmcp CI. |
| Legacy stdio | `server-everything` stdio | Yes |
| Modern Streamable HTTP | `conformance-server`, conformance suite | Yes |
| Legacy Streamable HTTP | `conformance-server`, `server-everything`, conformance suite | Yes |
| OAuth HTTP with local mock AS | Conformance auth scenarios | Yes, only inside a suite run |
| `input_required` / elicitation | `sep-2322-client-request-state`, `conformance-server` MRTR tools, `server-everything` (legacy form and URL) | Yes |
| Progress | `conformance-server` `test_tool_with_progress`, `server-everything`, rmcp `progress_demo` | Yes (no check of `ncp` output) |
| Cancellation | None that records the signal | Gap |
| Large results | None | Gap |

## 5. rmcp test utilities for a downstream crate

| Item | Finding |
|---|---|
| `testing` feature | None. Features: `client`, `server`, `macros`, `elicitation`, `request-state`, transports, `auth`, others. |
| Public parts useful in tests | Server API with `transport-io` / `transport-async-rw` (in-process duplex), `transport-streamable-http-server` (axum), `transport-child-process`. `InMemoryCredentialStore`. `AuthorizationCallback::from_redirect_url`. `RequestStateCodec`. |
| `crates/rmcp/tests/common/` | `calculator.rs`, `handlers.rs`. Not exported. Copy if needed. |
| Test patterns to copy | `test_streamable_http_disconnect_cancel.rs`, `test_cancelled_response.rs` (P10), `test_progress_subscriber.rs` (P11), `test_mrtr_behavior.rs` (P8), `test_cache_hints.rs` (P12), `test_client_lifecycle_modes.rs` (P2 fallback), `test_streamable_http_standard_headers.rs` (P1). |
| Mock OAuth server | None reusable outside the conformance suite. |

## 6. Gaps: baseline MUST items with no official test part

| ID | Official part | Gap | Own fixture |
|---|---|---|---|
| P1 | Conformance `2026-07-28` leg (HTTP) | stdio | rmcp stdio server, modern era |
| P2 | Conformance `2025-11-25` leg (pinned era); `server-everything` | Auto detection and the cache of the result | Test with `server-everything` (fallback) and `conformance-server`; second run reads the cache |
| P4 | Conformance (HTTP only) | stdio: start, stderr, exit, framing | rmcp stdio fixture binary |
| P6 | `tools_call` | stdio | Same stdio fixture |
| P8 | `sep-2322-client-request-state` | "Stop with a clear error" and the exit code (I3) | MRTR fixture, non-TTY run |
| P10 | None | All | rmcp tool that waits and records cancellation: `notifications/cancelled` (stdio, legacy HTTP), disconnect (modern HTTP); test sends SIGINT |
| A1–A4 | Auth scenarios | None | — |
| A5 | `auth/authorization-server-migration`, token endpoint auth | Storage in the OS store across processes | Covered by S1 tests |
| A8 | `auth/offline-access-*` (metadata only) | Refresh grant | Own mock AS (rmcp axum) with short `expires_in` and a refresh grant |
| A10 | Indirect, through the adapter | Prompt text and paste parsing | Unit tests plus the adapter path |
| A11 | None | All | HTTP fixture that requires a fixed bearer token or API key header |
| S1, S7 | None | All | Keychain tests on macOS; Secret Service on Linux (D-Bus session) |
| S2 | None | All | File store mode `0600`, opt-in only |
| S3 | None | All | Read `ps` arguments during a run; scan debug logs for known test secrets |
| S4 | `server-everything` `get-env` (shows the environment of a stdio server) | Credential store source | Fixture secret in store, check `get-env` output |
| S6 | None | All (only if a daemon exists) | Socket mode and owner check |
| N1 | None | All | Sandbox plus proxy log (section 7) |
| N2 | None | All | Local `CONNECT` proxy with basic auth, upper and lower case variables, `NO_PROXY` |
| N3 | None | All | Local TLS server with a test CA. macOS system store needs approval (open item 4). Linux: `SSL_CERT_FILE` and system store. |
| N5 | `server-everything` `get-env` | None | — |

Also own: large results (multi-MB `tools/call` result, stdio and HTTP), and all
I* items.

## 7. Proposed harness layout

| Tier | Content | Runs on | Network | Sandbox |
|---|---|---|---|---|
| T0 unit | `cargo test`: config, argument parse, redaction, exit code map, credential store behind a mock (S7 interface), era cache logic | macOS, Linux CI | None | No |
| T1 local fixtures | `cargo test` with `ncp` binary black-box tests. Fixtures in a workspace crate `ncp-fixtures`: vendored `conformance-server`, stdio fixture (both eras), cancel fixture, large-result fixture, static-header fixture, mock AS with refresh, `CONNECT` proxy, TLS server with test CA. Ports from `127.0.0.1:0`. | macOS, Linux CI | localhost only | Yes |
| T2 official Node parts | Pinned conformance suite (`0.2.0-alpha.12`, lock file) with the `ncp-conformance` adapter: legs `--requirements 2025-11-25`, `--requirements 2026-07-28`, then `--suite all` for each version (informational), `--spec-version 2025-03-26` for P3. Pinned `server-everything` for legacy stdio and HTTP, `get-env`, URL elicitation. | macOS, Linux CI | localhost only at run time; npm install in a separate step before the sandbox | Yes |
| T3 OS integration | Real Keychain (temporary keychain on macOS), Secret Service (`dbus-run-session` with `gnome-keyring`), company root CA | macOS and Linux, manual or a separate CI job | localhost only | Yes |
| T4 third-party (later) | Keycloak fixture (Docker), real public servers | Manual, opt-in, outside CI | Allowed list only | Proxy log on |

Sandbox:

| Platform | Method | Notes |
|---|---|---|
| macOS | `sandbox-exec` profile: deny `network-outbound`, allow outbound to `localhost` and Unix sockets, allow `network-bind` and `network-inbound` on localhost | `sandbox-exec` is deprecated but present. The conformance servers bind all interfaces (`listen(0)`); the profile must permit this bind. Exact profile behavior: Unknown, test first. |
| Linux | New network namespace with only `lo` up (`unshare -rn` or `ip netns`) | GitHub Ubuntu 24.04 runners restrict unprivileged user namespaces through AppArmor. A `sudo` namespace or a sysctl change is then necessary. Status on the runner: Unknown. |
| Both | `env -i` with a temporary `HOME`; the T2 suite passes its environment to the adapter, so start the suite itself under `env -i`. A local proxy logs each `CONNECT`. | Matches baseline section 5. |

CI matrix: T0, T1 and T2 on `ubuntu-latest` and `macos-latest`. rust-sdk runs
conformance on Linux only, so a macOS run is new coverage. Keep an
`expected-failures` file for `ncp` from the first T2 run. Upload the `-o`
result directories as CI artifacts.

## Sources

- conformance (main `c37eec888e`): [README](https://github.com/modelcontextprotocol/conformance/blob/main/README.md), [SDK_INTEGRATION.md](https://github.com/modelcontextprotocol/conformance/blob/main/SDK_INTEGRATION.md), [package.json](https://github.com/modelcontextprotocol/conformance/blob/main/package.json), [LICENSE](https://github.com/modelcontextprotocol/conformance/blob/main/LICENSE), [action.yml](https://github.com/modelcontextprotocol/conformance/blob/main/action.yml), [requirements/2025-11-25.yaml](https://github.com/modelcontextprotocol/conformance/blob/main/requirements/2025-11-25.yaml), [requirements/2026-07-28.yaml](https://github.com/modelcontextprotocol/conformance/blob/main/requirements/2026-07-28.yaml), [src/scenarios/index.ts](https://github.com/modelcontextprotocol/conformance/blob/main/src/scenarios/index.ts), [src/scenarios/client/auth/index.ts](https://github.com/modelcontextprotocol/conformance/blob/main/src/scenarios/client/auth/index.ts), [discovery-metadata.ts](https://github.com/modelcontextprotocol/conformance/blob/main/src/scenarios/client/auth/discovery-metadata.ts), [basic-cimd.ts](https://github.com/modelcontextprotocol/conformance/blob/main/src/scenarios/client/auth/basic-cimd.ts), [createAuthServer.ts](https://github.com/modelcontextprotocol/conformance/blob/main/src/scenarios/client/auth/helpers/createAuthServer.ts), [serverLifecycle.ts](https://github.com/modelcontextprotocol/conformance/blob/main/src/scenarios/client/auth/helpers/serverLifecycle.ts), [mrtr-client.ts](https://github.com/modelcontextprotocol/conformance/blob/main/src/scenarios/client/mrtr-client.ts), [src/runner/client.ts](https://github.com/modelcontextprotocol/conformance/blob/main/src/runner/client.ts), [src/schemas/context.ts](https://github.com/modelcontextprotocol/conformance/blob/main/src/schemas/context.ts), [src/types.ts](https://github.com/modelcontextprotocol/conformance/blob/main/src/types.ts), [known-sdks.ts](https://github.com/modelcontextprotocol/conformance/blob/main/src/sdk-runner/known-sdks.ts), [ConformanceOAuthProvider.ts](https://github.com/modelcontextprotocol/conformance/blob/main/examples/clients/typescript/helpers/ConformanceOAuthProvider.ts), [examples/servers/typescript](https://github.com/modelcontextprotocol/conformance/tree/main/examples/servers/typescript), [releases](https://github.com/modelcontextprotocol/conformance/releases)
- npm: [@modelcontextprotocol/conformance](https://registry.npmjs.org/@modelcontextprotocol/conformance), [@modelcontextprotocol/server-everything](https://registry.npmjs.org/@modelcontextprotocol/server-everything)
- rust-sdk (tag `rmcp-v3.5.1`): [conformance.yml](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/.github/workflows/conformance.yml), [conformance/Cargo.toml](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/conformance/Cargo.toml), [conformance/src/bin/client.rs](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/conformance/src/bin/client.rs), [conformance/src/bin/server.rs](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/conformance/src/bin/server.rs), [expected-failures-extensions.yaml](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/conformance/expected-failures-extensions.yaml), [README](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/README.md), [crates/rmcp/Cargo.toml](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/Cargo.toml), [crates/rmcp/tests](https://github.com/modelcontextprotocol/rust-sdk/tree/rmcp-v3.5.1/crates/rmcp/tests), [test_live_oauth_refresh.rs](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/tests/test_live_oauth_refresh.rs), [examples/servers/README.md](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/examples/servers/README.md), [complex_auth_streamhttp.rs](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/examples/servers/src/complex_auth_streamhttp.rs), [keycloak-oauth-fixture.sh](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/scripts/keycloak-oauth-fixture.sh)
- servers (main `5abed86c53`): [src/everything](https://github.com/modelcontextprotocol/servers/tree/main/src/everything), [docs/features.md](https://github.com/modelcontextprotocol/servers/blob/main/src/everything/docs/features.md), [streamableHttp.ts](https://github.com/modelcontextprotocol/servers/blob/main/src/everything/transports/streamableHttp.ts), [LICENSE](https://github.com/modelcontextprotocol/servers/blob/main/LICENSE)
- typescript-sdk: [src/types.ts (v1.x)](https://github.com/modelcontextprotocol/typescript-sdk/blob/v1.x/src/types.ts)
- Spec: [Cancellation 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/basic/utilities/cancellation)
- Prior research: `stack-rust.md`, `vendor-clients.md`, `stack-go.md`
