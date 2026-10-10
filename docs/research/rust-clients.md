# Research: Rust MCP CLI clients and SDKs

Status: 2026-10-06. Baseline: [requirements.md](../requirements.md).

Method: `gh search repos` (14 queries), `gh api` (repo metadata, releases,
commits, contributors, file contents), WebSearch (4 queries), WebFetch (reqwest
changelog and docs). I read READMEs, `Cargo.toml` files, and these source files:
rmcp `auth.rs` and `service.rs`; rust-mcp-sdk `client_auth/*` and
`client_runtime.rs`; turbomcp-cli `cli.rs`, `main.rs`, `executor.rs`; mcplug
`oauth/*`, `daemon/manager.rs`, `transports/*`, `main.rs`, `error.rs`; reqwest
0.13.2 `Cargo.toml`, `client.rs`, `proxy.rs`. I did not clone, build, install or
run any code. "Unknown" means that I did not verify the item.

## 1. Result

- No Rust MCP CLI client meets C1. All CLI projects are small or come from one
  person. The largest CLI ([mcp-probe][probe], 139 stars) has no commit since
  2025-07-08.
- The official SDK [rmcp][rmcp] is the best base for a new build. It has
  `2026-07-28` and `2025-11-25` with automatic era detection, Streamable HTTP,
  and the full OAuth client (PKCE, RFC 9728, RFC 8414 + OIDC, RFC 8707,
  RFC 9207, CIMD, DCR, step-up). It has a pluggable `CredentialStore`. The
  default store is in memory only.
- [rust-mcp-sdk][rms] 2.x supports only `2026-07-28`. The `2025-11-25` client is
  on a separate 1.x line. Its HTTP stack uses bundled Mozilla roots (reqwest
  0.12 `rustls-tls`), not the system store.
- [TurboMCP][tm] has a CLI (`turbomcp-cli`) only on the 3.x line
  (`2025-11-25`). The 4.0 alpha (`2026-07-28`) has no CLI crate. The CLI has no
  OAuth and no credential store.
- [codex-rs/rmcp-client][codex] (OpenAI Codex) is not an MCP CLI. It is a
  reference design: rmcp plus the `keyring` crate for OAuth tokens.

## 2. CLI clients: shortlist

| Project | Owner | Stars | Last commit | Latest release | License |
|---|---|---|---|---|---|
| [Epistates/turbomcp][tm] (`turbomcp-cli`, branch `v3.x`) | Org; main author Nicholas Paterno (1030 of ~1040 commits). Not widely known. | 98 | 2026-10-03 | v3.6.0, 2026-09-30 (crate); v4.0.0-alpha.5, 2026-09-14 | MIT |
| [hydai/mcplug][mcplug] | Person: Hung-Ying Tai, WasmEdge maintainer (known) | 0 | 2026-05-14 (dependabot); last code 2026-03 | v0.1.2, 2026-03-03 | MIT |
| [conikeec/mcp-probe][probe] | Person, not known | 139 | 2025-07-08 | v0.3.0, 2025-06-30 | MIT |
| [camiloazula/coco-mcp][coco] (reference only) | Person, not known; repo created 2026-09-12 | 9 | 2026-09-25 | v0.4.0, 2026-09-25 | Apache-2.0 |

## 3. CLI clients: facts

| Item | turbomcp-cli 3.6 | mcplug 0.1.2 | mcp-probe 0.3 | coco-mcp 0.4 |
|---|---|---|---|---|
| Spec versions | `2025-11-25`, adapter for `2025-06-18` ([lib.rs][tm-proto]) | `2025-03-26` hard-coded ([http_sse.rs][mcplug-http]) | Pre-`2025-11-25` (last commit 2025-07) | Both eras, per server: `initialize` (2024-11-05 to 2025-11-25) and `2026-07-28` |
| Transports | stdio, HTTP ("HTTP/SSE"), WebSocket, TCP, Unix socket | stdio, HTTP | stdio, HTTP/SSE, WebSocket, TCP | stdio, Streamable HTTP |
| OAuth client | None. README roadmap only. | Browser login, PKCE S256, RFC 8414 at base URL. No RFC 9728, no `resource`, no paste flow ([flow.rs][mcplug-flow]). | Unknown | PKCE, DCR (README). Rest Unknown. |
| Credential storage | None. Token in `--auth` argument or `MCP_AUTH` env. | Plain `~/.mcplug/<server>/tokens.json`, no mode set ([cache.rs][mcplug-cache]) | Unknown | OS keyring (README) |
| Proxy, roots | Unknown | reqwest 0.13 defaults: env proxy, platform verifier (not tested) | Unknown | Unknown |
| Output | `-f human\|json\|yaml\|table\|compact`. No TTY detection found. `--no-color`. | Human default, `--json`. Error JSON with code strings. | TUI | GUI; CLI via `--cli` |
| Exit codes | `1` for all errors ([main.rs][tm-main]) | `1` for all errors | Unknown | `1` on breaking diff; rest Unknown |
| Daemon | No | Yes. Unix socket `~/.mcplug/daemon.sock`. Socket mode Unknown. | No | No (GUI process) |
| Config | `~/.turbomcp/config.yaml` (saved connections). `install` writes Claude Desktop and Cursor config. | `mcpServers` JSON: `./config/mcplug.json`, `~/.mcplug/mcplug.json`. Imports Cursor, Claude Desktop, Claude Code, VS Code, Windsurf, Codex, OpenCode. | `--session-file` JSON | SQLite. Imports and exports `mcpServers`. |
| Distribution | `cargo install` only. No release assets. | Release tarballs: macOS and Linux, x86_64 and arm64. No checksums seen. | Tarballs with `.sha256`, Homebrew tap, `curl \| bash` installer | Homebrew tap; macOS arm64/x86_64, Linux x86_64 |
| Other network | `deploy` (cloud platforms) and `new` on user command. Downloads Unknown. | None found. Opens a browser for OAuth. | Unknown | Unknown |

## 4. CLI clients: MUST results

Has = H. Fork fix = FF. Structural gap = SG. Unknown = U.

| ID | turbomcp-cli | mcplug | mcp-probe |
|---|---|---|---|
| P1 `2026-07-28` | SG (v3 line; v4 has no CLI) | SG (own protocol code, `2025-03-26`) | SG (inactive) |
| P2 `2025-11-25` + detect | FF (has `2025-11-25`; no era detection) | FF | U |
| P4 stdio + Streamable HTTP | U (HTTP mode Unknown) | U | U |
| P6 tools list/call | H | H | H |
| P8 `input_required` | SG (needs `2026-07-28`) | SG | SG |
| P10 cancel on Ctrl-C | U | U | U |
| A1 PKCE | SG (no OAuth) | H | U |
| A2 RFC 9728 + 8414 + OIDC | SG | FF | U |
| A3 RFC 8707 `resource` | SG | FF | U |
| A4 RFC 9207 `iss` | SG | FF | U |
| A5 per-issuer client | SG | FF | U |
| A8 refresh | SG | U | U |
| A10 paste redirect URL | SG | FF | U |
| A11 static header | H (`--auth`) | H (`headers` in config) | U |
| S1 OS credential store | FF | FF | U |
| S2 no plain file | H (stores nothing) | FF | U |
| S3 no secret in argv | FF (env var exists) | U | U |
| S4 stdio env from store | FF | FF | U |
| S6 daemon socket user-only | H (no daemon) | U | H (no daemon) |
| N1 no other network | U (`deploy`, `new`) | H (none found) | U |
| N2 proxy env | U | H (reqwest default, not tested) | U |
| N3 system roots | U | H (platform verifier, not tested) | U |
| N5 proxy env to stdio | U | U | U |
| I1 TTY-aware JSON | FF | FF | SG (TUI) |
| I2 stable JSON schema | U | U | U |
| I3 defined exit codes | FF | FF | U |
| I4 JSON, `key=value`, stdin | U (JSON only seen) | H (`key:value`, JSON, call syntax); stdin U | U |
| I6 debug traffic, secrets hidden | U | U | H (protocol view); secrets U |
| F1 native | H | H | H |
| F2 phase timings | FF | FF | U |
| D1 one file, 4 targets | FF (no release assets) | H | H |
| D2 no self-update | U | H (none found) | U (`curl \| bash`) |
| C1 established, known maintainer | SG (one main author, 98 stars) | SG (0 stars; known person) | SG |
| C2 follows spec | U (v4 does; CLI dropped) | SG (no update since 2026-03) | SG |
| C3 license | H | H | H |

Verdict: no CLI candidate is acceptable. Each has a structural gap on C1 or P1.

## 5. SDKs: client-side facts

| Item | [rmcp][rmcp] (official) | [rust-mcp-sdk][rms] | [TurboMCP v4][tm] |
|---|---|---|---|
| Owner | `modelcontextprotocol` org | `rust-mcp-stack` org; main author `hashemix` (301 commits) | Epistates; one main author |
| Stars | 3979 | 196 | 98 |
| Last commit, release | 2026-10-05, rmcp-v3.5.1 2026-10-05 | 2026-09-19, v2.0.0 2026-08-27 | 2026-10-03, v4.0.0-alpha.5 2026-09-14 |
| License | MIT to Apache-2.0 transition ([LICENSE][rmcp-lic]) | MIT | MIT |
| Spec versions | `2026-07-28` and `2025-11-25` (and older known versions). Tier 1; client conformance 100% on both ([ROADMAP.md][rmcp-road]). | 2.x: `2026-07-28` only. 1.x LTS: `2025-11-25`. | `2025-06-18`, `2025-11-25`, `2026-07-28` (alpha) |
| Era detection | `ClientLifecycleMode::Auto`: try `server/discover`, fall back to `initialize` on "not implemented" or after 10 s ([README][rmcp]) | None in 2.x client runtime (no `initialize` path found) | "Negotiates the revision" (README). Details Unknown. |
| Transports (client) | stdio, child process, Streamable HTTP, Streamable HTTP over Unix socket. No HTTP+SSE (non-goal). | stdio, Streamable HTTP, legacy SSE | stdio, Streamable HTTP, WebSocket |
| MRTR / `input_required` | Yes. `call_tool` drives MRTR, max 10 rounds. | Yes (README) | Unknown |
| Cancellation | `RequestHandle::cancel` sends `notifications/cancelled` ([service.rs][rmcp-svc]) | Unknown | Unknown |
| List cache (`ttlMs`, `cacheScope`) | Yes, in memory | Yes, in memory (`response_cache.rs`) | "Response caching" (README) |
| PKCE | S256 only | Yes | Yes (README) |
| Discovery | RFC 9728, RFC 8414, OIDC | RFC 9728, RFC 8414; issuer match check | RFC 9728 (README) |
| RFC 8707 `resource` | Yes, authorization and token requests | Yes (`McpAuthConfig::resource`) | Unknown |
| RFC 9207 `iss` | Yes. Strict when AS advertises support; tolerant otherwise ([auth.rs][rmcp-auth] L2065) | Yes, strict (SEP-2468) | Unknown |
| Registration order | Pre-registered, then CIMD, then DCR | Pre-registered, then CIMD, then DCR | Unknown |
| Per-issuer credentials | `StoredCredentials.issuer` field | `TokenStore` keyed by issuer (SEP-2352) | Unknown |
| Step-up (`insufficient_scope`) | Yes, scope union, attempt counter | `union_scopes` helper | Unknown |
| Paste redirect URL | `AuthorizationCallback::from_redirect_url` parses `code`, `state`, `iss` | Unknown | Unknown |
| Credential store | Trait `CredentialStore`; default in memory. No keyring backend. | Trait `TokenStore`; default in memory. No keyring backend. | Unknown |
| TLS roots | reqwest 0.13 `rustls`: platform verifier (system store) | reqwest 0.12 `rustls-tls`: bundled Mozilla roots | Unknown |
| Proxy | reqwest reads `HTTP(S)_PROXY`/`ALL_PROXY`/`NO_PROXY`, upper and lower case. macOS system settings need the `system-proxy` feature (rmcp turns default features off). | reqwest 0.12 env proxy | Unknown |
| Custom OAuth HTTP | `OAuthHttpClient` trait; separate no-redirect client for token requests | Unknown | Unknown |

## 6. SDKs: MUST results for a new build

Only rows that the SDK controls. CLI rows (I*, F2, D*, S3, S5) belong to the new code.

| ID | rmcp | rust-mcp-sdk 2.x | TurboMCP v4 |
|---|---|---|---|
| P1 | H | H | H (alpha) |
| P2 | H (`Auto`; the CLI keeps the result) | SG (needs 1.x line too) | U |
| P4 | H | H | H |
| P6 | H | H | H |
| P8 | H | H | U |
| P10 | H | U | U |
| A1 | H | H | H |
| A2 | H | FF (OIDC Unknown) | U |
| A3 | H | H | U |
| A4 | H | H | U |
| A5 | H | H | U |
| A8 | H | H | U |
| A10 | H | U | U |
| A11 | H (static headers) | H | U |
| S1 | FF (implement `CredentialStore` with `keyring`) | FF | U |
| S2 | H (memory default) | H | U |
| N1 | H (no telemetry dependency in `Cargo.toml`) | H (no telemetry dependency in `Cargo.toml`) | U (crate `turbomcp-telemetry` exists) |
| N2 | H (env vars; not tested) | H (not tested) | U |
| N3 | H (platform verifier) | FF (switch to native roots) | U |
| F1 | H | H | H |
| C1 | H | FF (org, one main author) | SG (one main author, alpha) |
| C2 | H (Tier 1, conformance CI) | H | H |
| C3 | H | H | H |

## 7. Other projects seen (not shortlisted)

| Project | Stars | Why not |
|---|---|---|
| [lloydzhou/mcpc][mcpc] | 1 | Single author, created 2026-09-10. Daemon, `@session` syntax, `--protocol-version`. |
| [loonghao/mcpcall][mcpcall] | 1 | Single author. Uses rmcp. Clean crate split; `batch`, `doctor`, `--bearer-env`. |
| [pawelb0/mcpal][mcpal] | 2 | Single author. OAuth PKCE + DCR, OS keyring. |
| [rust-mcp-stack/mcp-discovery][disc] | 86 | Documentation generator, not a call client. Uses rust-mcp-sdk 1.x. |
| [openai/codex][codex] `rmcp-client` | — | Agent, not an MCP CLI. Reference for keyring storage. |
| [aws/amazon-q-developer-cli][q] | 1983 | Agent chat, not an MCP CLI. |

avelino/mcp: no new information on S1 or N1. Commits through 2026-09-27
(v0.6.2-beta) add proxy identity forwarding and Docker fixes. No commit message
names auth storage or ChronoDB.

## 8. Ideas worth taking

- rmcp `ClientLifecycleMode::Auto`: probe `server/discover`, then fall back to
  `initialize`. Keep the detected era per server, because the fallback can take
  10 s.
- Codex `rmcp-client`: rmcp plus `keyring` with a store mode setting
  (`auto`, `keyring`, `file`). Use the same split, but make `file` an explicit
  opt-in only (S2). Codex `auto` falls back to a file with no prompt.
- rmcp `OAuthHttpClient`: one HTTP client for discovery (follow redirects) and
  one for token requests (no redirects). Put timings (F2) in this layer.
- mcplug: import `mcpServers` from Claude Desktop, Claude Code, Cursor and VS
  Code paths. Tool call syntax `server.tool key:value` and `server.tool(...)`.
  Error JSON with a stable `code` string.
- coco-mcp: `diff` against a saved tool snapshot, classify changes as
  breaking, compatible or cosmetic, exit `1` on breaking. Copy a request as
  JSON-RPC or `curl` with tokens hidden. Move tokens from an imported
  `mcpServers` file into the keyring.
- mcpcall: `--bearer-env NAME` keeps tokens out of argv (S3). `batch` reads a
  JSON array from stdin and runs all calls in one session. `doctor` checks an
  endpoint. Crate split: CLI, core contracts, SDK adapter.
- turbomcp-cli: `tools schema` and `tools export` print input schemas (I5).

## Sources

- [modelcontextprotocol/rust-sdk][rmcp], [ROADMAP.md][rmcp-road], [OAUTH_SUPPORT.md][rmcp-oauth], [auth.rs][rmcp-auth], [service.rs][rmcp-svc], [Cargo.toml][rmcp-cargo], [LICENSE][rmcp-lic]
- [rust-mcp-stack/rust-mcp-sdk][rms], [client_auth/client.rs][rms-client], [client_auth/store.rs][rms-store], [workspace Cargo.toml][rms-cargo]
- [Epistates/turbomcp][tm], [turbomcp-cli README (v3.x)][tm-cli], [turbomcp-cli main.rs][tm-main], [turbomcp-protocol lib.rs (v3.x)][tm-proto]
- [hydai/mcplug][mcplug], [oauth/cache.rs][mcplug-cache], [oauth/flow.rs][mcplug-flow], [transports/http_sse.rs][mcplug-http], [daemon/manager.rs][mcplug-daemon]
- [conikeec/mcp-probe][probe], [camiloazula/coco-mcp][coco], [lloydzhou/mcpc][mcpc], [loonghao/mcpcall][mcpcall], [pawelb0/mcpal][mcpal], [rust-mcp-stack/mcp-discovery][disc]
- [openai/codex rmcp-client oauth.rs][codex], [aws/amazon-q-developer-cli][q], [avelino/mcp commits][avelino]
- [reqwest CHANGELOG][reqwest-chg], [reqwest 0.13 docs][reqwest-docs], [reqwest v0.13.2 Cargo.toml][reqwest-cargo]

[rmcp]: https://github.com/modelcontextprotocol/rust-sdk
[rmcp-road]: https://github.com/modelcontextprotocol/rust-sdk/blob/main/ROADMAP.md
[rmcp-oauth]: https://github.com/modelcontextprotocol/rust-sdk/blob/main/docs/OAUTH_SUPPORT.md
[rmcp-auth]: https://github.com/modelcontextprotocol/rust-sdk/blob/main/crates/rmcp/src/transport/auth.rs
[rmcp-svc]: https://github.com/modelcontextprotocol/rust-sdk/blob/main/crates/rmcp/src/service.rs
[rmcp-cargo]: https://github.com/modelcontextprotocol/rust-sdk/blob/main/crates/rmcp/Cargo.toml
[rmcp-lic]: https://github.com/modelcontextprotocol/rust-sdk/blob/main/LICENSE
[rms]: https://github.com/rust-mcp-stack/rust-mcp-sdk
[rms-client]: https://github.com/rust-mcp-stack/rust-mcp-sdk/blob/main/crates/rust-mcp-sdk/src/auth/client_auth/client.rs
[rms-store]: https://github.com/rust-mcp-stack/rust-mcp-sdk/blob/main/crates/rust-mcp-sdk/src/auth/client_auth/store.rs
[rms-cargo]: https://github.com/rust-mcp-stack/rust-mcp-sdk/blob/main/Cargo.toml
[tm]: https://github.com/Epistates/turbomcp
[tm-cli]: https://github.com/Epistates/turbomcp/blob/v3.x/crates/turbomcp-cli/README.md
[tm-main]: https://github.com/Epistates/turbomcp/blob/v3.x/crates/turbomcp-cli/src/main.rs
[tm-proto]: https://github.com/Epistates/turbomcp/blob/v3.x/crates/turbomcp-protocol/src/lib.rs
[mcplug]: https://github.com/hydai/mcplug
[mcplug-cache]: https://github.com/hydai/mcplug/blob/master/src/oauth/cache.rs
[mcplug-flow]: https://github.com/hydai/mcplug/blob/master/src/oauth/flow.rs
[mcplug-http]: https://github.com/hydai/mcplug/blob/master/src/transports/http_sse.rs
[mcplug-daemon]: https://github.com/hydai/mcplug/blob/master/src/daemon/manager.rs
[probe]: https://github.com/conikeec/mcp-probe
[coco]: https://github.com/camiloazula/coco-mcp
[mcpc]: https://github.com/lloydzhou/mcpc
[mcpcall]: https://github.com/loonghao/mcpcall
[mcpal]: https://github.com/pawelb0/mcpal
[disc]: https://github.com/rust-mcp-stack/mcp-discovery
[codex]: https://github.com/openai/codex/blob/main/codex-rs/rmcp-client/src/oauth.rs
[q]: https://github.com/aws/amazon-q-developer-cli
[avelino]: https://github.com/avelino/mcp/commits
[reqwest-chg]: https://github.com/seanmonstar/reqwest/blob/master/CHANGELOG.md
[reqwest-docs]: https://docs.rs/reqwest/0.13/reqwest/
[reqwest-cargo]: https://github.com/seanmonstar/reqwest/blob/v0.13.2/Cargo.toml
