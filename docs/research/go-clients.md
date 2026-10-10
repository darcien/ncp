# Research: Go MCP CLI clients

Status: 2026-10-06. Baseline: [requirements.md](../requirements.md).
Spec: MCP `2026-07-28` (modern), `2025-11-25` (legacy).

Method: I read GitHub search results (14 `gh search repos` queries, sorted by
stars), repository metadata, releases, contributors, READMEs, project docs, and
selected source files through the contents API. I did one web search. I did not
clone, build, install, or run any code. "Not tested" means that I read the code
path but did not run it.

## 1. Result

- No Go CLI client is an acceptable fork base. Each candidate has a MUST
  structural gap, or needs large changes in its purpose.
- `lydakis/mcpx` is the best technical match. It fails C1: one author,
  53 stars, created 2026-02. Use it as a design reference.
- `f/mcptools` has the known maintainer and the most stars. It fails C2: no
  commit since 2025-12-18, and it sends `initialize` with `2024-11-05`.
- For a new Go build, the official `modelcontextprotocol/go-sdk` (v1.8.0) has
  all protocol and OAuth parts that the baseline needs (section 5).

Legend for the tables:

- **Has**: meets the requirement (from code that I read, unless noted).
- **Has (doc)**: the project docs state it. I did not read the code.
- **Fork fix**: a small change adds it. **Fork fix (L)**: a fork can add it,
  but the change is large.
- **Structural gap**: the design or the project state prevents it.
- **Unknown**: not verified.

## 2. Long list

| Repo | Stars | Last push | Latest release | License | Owner | Type | Decision |
|---|---|---|---|---|---|---|---|
| [f/mcptools][mcptools] | 1626 | 2025-12-18 | v0.7.1 (2025-05-05) | MIT | Fatih Kadir Akın (known, GitHub Star) | list/call CLI, shell, web UI | Shortlist |
| [lydakis/mcpx][mcpx] | 53 | 2026-09-29 | v1.2.1 (2026-09-29) | MIT | George Lydakis (person, 97 of 98 human commits) | list/call CLI with daemon | Shortlist |
| [giantswarm/mcp-debug][mcpdebug] | 37 | 2026-10-02 | v0.1.18 (2026-09-29) | Apache-2.0 | Giant Swarm (company) | debugger, REPL, HTTP only | Shortlist |
| [standardbeagle/mcp-tui][mcptui] | 11 | 2026-10-04 | v0.10.0 (2026-09-27) | MIT | small org; most commits by a `claude` account | TUI and CLI | Shortlist (README only) |
| [stacklok/toolhive][toolhive] (`thv mcp`) | 2235 | 2026-10-06 | v0.51.4 (2026-09-27) | Apache-2.0 | Stacklok (company) | server platform with `thv mcp list/call` | Not fit: `thv mcp` has SSE and Streamable HTTP only; update checker with instance ID ([checker.go][thv-upd]) |
| [docker/mcp-gateway][docker] (`docker mcp`) | 1589 | 2026-09-23 | none on GitHub | MIT | Docker (company) | Docker CLI plugin and gateway | Not fit: needs Docker; servers run in containers; OTEL telemetry through Docker Desktop ([telemetry doc][docker-tel]) |
| [thellimist/clihub][clihub] | 666 | 2026-03-12 | none | MIT | Kan Yilmaz (person) | generator: compiles one CLI for each server | Not fit: needs Go toolchain; tokens in `~/.clihub/credentials.json` |
| [mark3labs/mcphost][mcphost] | 1595 | 2026-04-13 | archived | MIT | mark3labs | LLM host | Out of scope; archived |
| [mcpshim/mcpshim][mcpshim] | 66 | 2026-08-13 | v0.0.3 | MIT | one-person org | daemon and CLI, remote servers | Not fit: C1; mcp-go v0.44 |
| [rkonfj/mcpurl][mcpurl] | 20 | 2025-07-19 | v0.2.1 | GPL-3.0 | person | curl-like client | Not fit: stale |
| [juanibiapina/mcpli][mcpli] | 11 | archived | — | none | person | CLI | Not fit: archived, no license |
| [smart-mcp-proxy/mcpproxy-go][mcpproxy] | 385 | 2026-10-06 | v0.70.0 | MIT | org | aggregating proxy | Out of scope; not read |

## 3. Shortlist facts

| Item | mcptools | mcpx | mcp-debug | mcp-tui |
|---|---|---|---|---|
| SDK | mcp-go v0.34.0 | go-sdk v1.7.0 | mcp-go v1.1.1, own OAuth code | go-sdk v1.8.0 |
| Spec versions | `initialize` with `2024-11-05`, hard-coded ([utils.go][mt-utils]) | `2026-07-28` with `server/discover`; fallback to `2025-11-25` `initialize` ([resolve.go][mx-resolve], [pool.go][mx-pool]) | `initialize` with `2024-11-05`, hard-coded ([client.go][md-client]) | `2026-07-28` and earlier; `--protocol-version` pin (doc) |
| Transports | stdio, Streamable HTTP, HTTP+SSE | stdio, Streamable HTTP; SSE Unknown | Streamable HTTP only ([root.go][md-root]) | stdio, Streamable HTTP, SSE (doc) |
| OAuth client | None. Static: `--auth-header`, `--auth-user`, `user:pass@` in URL | go-sdk handler: PKCE, RFC 9728, RFC 8414 + OIDC, RFC 8707, RFC 9207 `iss`, CIMD opt-in (`--oauth-client-metadata-url`), DCR fallback, refresh with rotation ([handler.go][mx-handler]) | PKCE, RFC 9728, RFC 8414, OIDC option, RFC 8707, CIMD by default, then DCR; step-up with retry limit ([root.go][md-root]). RFC 9207: Unknown | PKCE, DCR, CIMD, client credentials, SEP-990 (doc) |
| Login without browser | n/a | Prints URL, never opens a browser. Loopback `127.0.0.1:<random>` only, no paste ([loopback.go][mx-loop]) | Fixed redirect `localhost:8765` | Unknown |
| Credential storage | None. Secrets go in process arguments | OAuth session JSON (tokens, client secret, issuer) in OS store through `zalando/go-keyring`, service `mcpx`, key `oauth:sha256(serverURL)` ([store.go][mx-store]). Static headers in `config.toml` with `${ENV}` expansion | Memory only (`NewMemoryTokenStore`). Login on each run | JSON file cache, mode `0600`, dir `0700` ([cache.go][mt2-cache]) |
| Proxy | Go default: mcp-go uses `&http.Client{}`, so `HTTPS_PROXY` applies. Not tested | Disabled on purpose: `Transport.Proxy = nil` for MCP HTTP and OAuth (SSRF guard with pinned dial) ([client.go][mx-safehttp]) | Unknown | Unknown |
| Root certificates | Go system roots, no override found | Clone of `http.DefaultTransport`, Go system roots. Not tested | Unknown | Unknown |
| Output | `--format table\|json\|pretty`, default `table`, not TTY-aware | Tool output passes through raw. `--json` for mcpx lists and help. `--catalog` gives full descriptors | Logs and REPL. `--json-rpc` traffic log, `--no-color` | Unknown |
| Exit codes | `0`; `1` for all errors ([main.go][mt-main]) | `0` ok, `1` `isError` and `input_required`, `2` usage, `3` internal/transport | Unknown | Unknown |
| Daemon | No. `mcp web` listens on all interfaces (`":"+port`, default 41999) | Yes, auto-start. Unix socket `0600` in `0700` dir, peer UID check, nonce file `0600` ([server.go][mx-ipc]). Keeps stdio servers and sessions | No. `--mcp-server` mode can listen on `:8899` | Unknown |
| Config | `~/.mcpt/aliases.json`, `~/.mcpt/configs.json` | `~/.config/mcpx/config.toml` (TOML) | Flags only | Unknown |
| Reads client configs | `mcp configs scan/set/sync`: VS Code, Cursor, Claude Desktop, Claude Code, Windsurf (`mcpServers` JSON); also writes them | Fallback read when no managed servers: Claude (`~/.claude.json`, `.mcp.json`), Cursor, Cline, Codex, Kiro. `mcpx import` with redacted preview ([fallback.go][mx-fallback]) | No | Claude Desktop, VS Code (doc) |
| Distribution | Homebrew tap (macOS), `go install`. No binaries on GitHub releases | GoReleaser: darwin and linux, amd64 and arm64, `checksums.txt`; Homebrew cask; npm and PyPI wrappers | Binaries for darwin, linux, windows (amd64, arm64) with cosign bundles | `go install`, npm |
| Other network calls | None found (code search for `http.Get`, `http.NewRequest`) | None found for update or telemetry. Codex Apps: if Codex Apps is on, mcpx reads `~/.codex/auth.json` and adds the `chatgpt.com` backend as a server | `self-update` command downloads from GitHub releases (only on command) ([selfupdate.go][md-self]) | Unknown |

Notes:

- mcpx daemon: stdio servers get the environment of the daemon. The daemon
  gets the environment of the CLI call that started it
  ([spawn.go][mx-spawn]). A changed `HTTPS_PROXY` needs a daemon restart.
- mcpx cancellation: when the CLI process stops, the socket closes. The daemon
  then cancels the request context ([server.go][mx-ipc]). go-sdk sends
  `notifications/cancelled` on context cancel (v1.8.0 release notes).
- mcpx uses project-scoped fallback files (`.mcp.json`) without approval when
  it has no managed servers. This is against S5 (SHOULD).

## 4. MUST requirements

| ID | mcptools | mcpx | mcp-debug | mcp-tui |
|---|---|---|---|---|
| P1 | Fork fix (L): upgrade to mcp-go v1, remove hard-coded version | Has | Fork fix: remove hard-coded `initialize` | Has (doc) |
| P2 | Fork fix (L) | Has | Fork fix | Has (doc) |
| P4 | Has | Has | Fork fix: no stdio client | Has (doc) |
| P6 | Has | Has | Has (REPL only) | Has (doc) |
| P8 | Fork fix | Has: JSON result, `--request-state` resume, `--interactive` on TTY ([mrtr.go][mx-mrtr]) | Fork fix | Has (doc) |
| P10 | Unknown | Has (not tested) | Unknown | Unknown |
| A1 | Fork fix (L): no OAuth | Has | Has (doc) | Has (doc) |
| A2 | Fork fix (L) | Has | Has (doc) | Unknown |
| A3 | Fork fix (L) | Has for authorization and code exchange; refresh Unknown | Has (doc) | Unknown |
| A4 | Fork fix (L) | Has | Unknown | Unknown |
| A5 | Fork fix (L) | Unknown: session bound to issuer, key per server URL | Unknown | Unknown |
| A8 | Fork fix (L) | Has | Fork fix: memory only | Unknown |
| A10 | Fork fix (L) | Fork fix: add paste in the fetcher | Unknown | Unknown |
| A11 | Has (arguments only) | Has | Fork fix: no header flag found | Unknown |
| S1 | Fork fix (L) | Fork fix: static headers not in OS store | Fork fix: no persistence | Fork fix: `0600` file |
| S2 | Has | Has | Has | Fork fix: file by default |
| S3 | Fork fix: secrets only through arguments | Unknown | Fork fix: secret flag (env option exists) | Has (doc): masked |
| S4 | Fork fix | Fork fix: env from config, no OS store read | Fork fix: no stdio | Unknown |
| S6 | Has (no daemon) | Has | Has (no daemon) | Unknown |
| N1 | Has | Has (see Codex Apps note) | Fork fix: remove `self-update` | Unknown |
| N2 | Has (not tested) | Fork fix: proxy disabled by design; fork must move SSRF checks to request time | Unknown | Unknown |
| N3 | Has (not tested) | Has (not tested) | Unknown | Unknown |
| N5 | Has: inherits `os.Environ()` | Has (daemon environment, see note) | Fork fix: no stdio | Unknown |
| I1 | Fork fix | Fork fix | Fork fix (L) | Unknown |
| I2 | Fork fix | Fork fix | Fork fix (L) | Unknown |
| I3 | Fork fix | Fork fix: `input_required` shares code `1`; no auth code | Unknown | Unknown |
| I4 | Fork fix: JSON in `-p` only | Has: schema flags, positional JSON, stdin JSON | Fork fix (L) | Unknown |
| I6 | Fork fix: server stderr only | Fork fix | Has; redaction Unknown | Has (doc) |
| F1 | Has | Has | Has | Has |
| F2 | Fork fix (L) | Fork fix (L) | Fork fix (L) | Fork fix: HTTP timings exist (doc) |
| D1 | Fork fix: no Linux binaries | Has | Has | Unknown |
| D2 | Has | Has | Fork fix: remove `self-update` | Unknown |
| C1 | Has | **Structural gap**: single author | Has | **Structural gap**: small, unknown |
| C2 | **Structural gap**: inactive since 2025-12; `2024-11-05` | Has | Unknown | Has |
| C3 | Has | Has | Has | Has |

Verdict:

- mcptools: not acceptable (C2).
- mcpx: not acceptable (C1). Technical gaps are small: N2, A10, S1, I3, F2.
- mcp-debug: no MUST structural gap, but it is a debugger. A fork must add
  stdio, a one-shot CLI, JSON output, and token persistence. A new build on
  go-sdk is less work.
- mcp-tui: not acceptable (C1).

## 5. Go SDKs for a new build (client side)

| Item | [go-sdk][gosdk] | [mcp-go][mcpgo] | [go-mcp][gomcp] | [mcp-golang][mcpgolang] |
|---|---|---|---|---|
| Owner | `modelcontextprotocol` org, with Google | mark3labs (org) | ThinkInAIXYZ (org) | metoro-io (org) |
| Stars / release | 5187 / v1.8.0 (2026-09-14) | 9153 / v1.1.1 (2026-09-23) | 675 / v0.2.29 (2026-06-29) | 1229 / v0.16.1 (2026-02-25) |
| License | Apache-2.0 (new code), MIT (old code) | MIT | MIT | MIT |
| Spec versions | `2026-07-28` since v1.7.0; legacy `2025-11-25` and earlier. `server/discover` with `initialize` fallback | `2026-07-28` since v1.0.0 ([PR #951][mcpgo-951]); legacy list Unknown | `2024-11-05`, `2025-03-26` only | Unknown; server focus |
| Client transports | stdio (`CommandTransport`), Streamable HTTP, SSE | stdio, Streamable HTTP, SSE | Unknown | Unknown |
| MRTR, `ttlMs`, `subscriptions/listen` | Has (v1.7.0 notes) | Unknown | No | No |
| Cancellation | Sends `notifications/cancelled` on context cancel | Unknown | Unknown | Unknown |
| PKCE | Has; rejects an AS with no `S256` ([auth_meta.go][gs-authmeta]) | Has | No OAuth client found | Unknown |
| Discovery | RFC 9728 (header, path, root); RFC 8414 and OIDC with path insertion and appending ([shared.go][gs-shared]) | RFC 9728; RFC 8414 and OIDC | — | — |
| RFC 8707 `resource` | Authorization and token requests ([authorization_code.go][gs-authcode]) | Token, refresh, and registration requests ([oauth.go][mcpgo-oauth]) | — | — |
| RFC 9207 `iss` | Has; option `AcceptUnadvertisedIss` | Not found (`ProcessAuthorizationResponse` takes no `iss`) | — | — |
| Registration | CIMD, then pre-registered, then DCR (DCR body max 1 MB) | Pre-registered, DCR. CIMD not found | — | — |
| Step-up | Has (`insufficient_scope`) | Unknown | — | — |
| Token storage | Caller hooks (`InitialTokenSource`, `NewTokenSource`) | `TokenStore` interface; memory default | — | — |
| Default HTTP client | `http.DefaultClient` (proxy variables apply) | `&http.Client{Timeout: 30s}` (proxy variables apply) | — | — |
| No-browser login | Caller supplies `AuthorizationCodeFetcher`; paste is possible | Caller builds URL and calls `ProcessAuthorizationResponse`; paste is possible | — | — |

Notes:

- go-sdk tries CIMD before a pre-registered client. Baseline A6 puts the
  pre-registered client first. A caller fixes this: set CIMD only when no
  pre-registered client exists. mcpx does this ([handler.go][mx-handler]).
- The proxy and root-certificate results depend on the HTTP client that the
  caller gives. The Go standard library reads `HTTPS_PROXY`, `HTTP_PROXY`,
  `NO_PROXY` in both cases, and credentials in the proxy URL. On macOS, Go
  uses the system verifier. On Linux, Go reads the system bundle and
  `SSL_CERT_FILE`.

## 6. Ideas worth taking

- **Command shape (mcpx):** `mcpx <server> <tool> --arg=value`. Flags come
  from the input schema. `--help` prints a man-page view of the schema.
  `--catalog` prints full descriptors. Arguments also come as positional JSON
  or stdin JSON.
- **Daemon security (mcpx):** Unix socket `0600` in a `0700` runtime dir, peer
  UID check, a nonce in a `0600` state file. A closed socket cancels the
  request.
- **MRTR for agents (mcpx):** print the `input_required` result as JSON with a
  defined exit code. Resume with `--request-state` and `--input-responses`.
  Prompt only with `--interactive` on a TTY.
- **OAuth login (mcpx):** print the URL and do not open a browser. Bind the
  saved session to the issuer. Stop credential writes during logout. Key the
  OS store entry by a hash of the server URL.
- **Config import (mcpx):** explicit `import <client>` with a redacted preview
  and source provenance, instead of a silent read of other client files.
- **CIMD hosting (mcp-debug):** host the client metadata document on GitHub
  Pages ([client.json][md-cimd]). Flags to set or turn off the CIMD URL.
- **Test switches (mcp-debug):** `--oauth-skip-resource-param`,
  `--oauth-step-up-max-retries`, `--oauth-preferred-auth-server`. These help
  tests against servers that do not follow the spec.
- **Debug view (mcp-tui, doc):** JSON-RPC in both directions, HTTP timings,
  masked credentials, `--protocol-version` pin, export of a session as a shell
  script.
- **Exit control (`thv mcp call`):** `--ignore-tool-error`, and
  `--args-file -` for stdin ([thv_mcp_call.md][thv-call]).
- **Policy (mcptools):** `guard --allow/--deny` patterns for tools, prompts and
  resources.
- **Quality gates (mcpx):** a pinned official conformance scenario for
  `2026-07-28` (`make conformance-core`) and manual performance benchmarks.

## 7. Open items

1. mcpx: does the refresh request send `resource` (A3)? It uses
   `oauth2.Config.TokenSource`, which adds no extra parameters. Read the go-sdk
   refresh path.
2. mcpx: does go-sdk register again when the authorization server changes (A5)?
3. mcp-tui: verify the README claims in code before using it as a reference.

## Sources

- [f/mcptools][mcptools]: README, [utils.go][mt-utils], [main.go][mt-main], `call.go`, `go.mod`
- [lydakis/mcpx][mcpx]: README, `docs/design.md`, `docs/usage.md`, [handler.go][mx-handler], [store.go][mx-store], [loopback.go][mx-loop], [safehttp/client.go][mx-safehttp], [ipc/server.go][mx-ipc], [daemon/spawn.go][mx-spawn], [mrtr.go][mx-mrtr], [pool.go][mx-pool], [resolve.go][mx-resolve], [fallback.go][mx-fallback], `internal/cli/auth.go`, `internal/mcppool/http.go`, `internal/mcppool/stdio.go`, `internal/processenv/env.go`
- [giantswarm/mcp-debug][mcpdebug]: README, `docs/usage.md`, [cmd/root.go][md-root], [cmd/selfupdate.go][md-self], [internal/agent/client.go][md-client]
- [standardbeagle/mcp-tui][mcptui]: README, [internal/mcp/oauth/cache.go][mt2-cache]
- [stacklok/toolhive][toolhive]: [thv_mcp_call.md][thv-call], `docs/cli/thv_mcp_list_tools.md`, [pkg/updates/checker.go][thv-upd]
- [docker/mcp-gateway][docker]: README, [docs/telemetry/README.md][docker-tel]
- [thellimist/clihub][clihub]: README
- [modelcontextprotocol/go-sdk][gosdk]: [v1.7.0 notes][gs-170], [v1.8.0 notes][gs-180], [authorization_code.go][gs-authcode], [shared.go][gs-shared], [auth_meta.go][gs-authmeta], `docs/protocol.md`, `mcp/streamable.go`, `LICENSE`
- [mark3labs/mcp-go][mcpgo]: [v1.0.0 notes][mcpgo-100], [client/transport/oauth.go][mcpgo-oauth], `client/transport/stdio.go` and `streamable_http.go` (v0.34.0)
- [ThinkInAIXYZ/go-mcp][gomcp]: `protocol/initialize.go`
- Web search: "Go command-line MCP client call tools stdio streamable HTTP OAuth github 2026"

[mcptools]: https://github.com/f/mcptools
[mt-utils]: https://github.com/f/mcptools/blob/master/cmd/mcptools/commands/utils.go
[mt-main]: https://github.com/f/mcptools/blob/master/cmd/mcptools/main.go
[mcpx]: https://github.com/lydakis/mcpx
[mx-handler]: https://github.com/lydakis/mcpx/blob/main/internal/oauthclient/handler.go
[mx-store]: https://github.com/lydakis/mcpx/blob/main/internal/oauthclient/store.go
[mx-loop]: https://github.com/lydakis/mcpx/blob/main/internal/oauthclient/loopback.go
[mx-safehttp]: https://github.com/lydakis/mcpx/blob/main/internal/safehttp/client.go
[mx-ipc]: https://github.com/lydakis/mcpx/blob/main/internal/ipc/server.go
[mx-spawn]: https://github.com/lydakis/mcpx/blob/main/internal/daemon/spawn.go
[mx-mrtr]: https://github.com/lydakis/mcpx/blob/main/internal/cli/mrtr.go
[mx-pool]: https://github.com/lydakis/mcpx/blob/main/internal/mcppool/pool.go
[mx-resolve]: https://github.com/lydakis/mcpx/blob/main/internal/bootstrap/resolve.go
[mx-fallback]: https://github.com/lydakis/mcpx/blob/main/internal/config/fallback.go
[mcpdebug]: https://github.com/giantswarm/mcp-debug
[md-root]: https://github.com/giantswarm/mcp-debug/blob/main/cmd/root.go
[md-self]: https://github.com/giantswarm/mcp-debug/blob/main/cmd/selfupdate.go
[md-client]: https://github.com/giantswarm/mcp-debug/blob/main/internal/agent/client.go
[md-cimd]: https://giantswarm.github.io/mcp-debug/client.json
[mcptui]: https://github.com/standardbeagle/mcp-tui
[mt2-cache]: https://github.com/standardbeagle/mcp-tui/blob/main/internal/mcp/oauth/cache.go
[toolhive]: https://github.com/stacklok/toolhive
[thv-call]: https://github.com/stacklok/toolhive/blob/main/docs/cli/thv_mcp_call.md
[thv-upd]: https://github.com/stacklok/toolhive/blob/main/pkg/updates/checker.go
[docker]: https://github.com/docker/mcp-gateway
[docker-tel]: https://github.com/docker/mcp-gateway/blob/main/docs/telemetry/README.md
[clihub]: https://github.com/thellimist/clihub
[mcphost]: https://github.com/mark3labs/mcphost
[mcpshim]: https://github.com/mcpshim/mcpshim
[mcpurl]: https://github.com/rkonfj/mcpurl
[mcpli]: https://github.com/juanibiapina/mcpli
[mcpproxy]: https://github.com/smart-mcp-proxy/mcpproxy-go
[gosdk]: https://github.com/modelcontextprotocol/go-sdk
[gs-170]: https://github.com/modelcontextprotocol/go-sdk/releases/tag/v1.7.0
[gs-180]: https://github.com/modelcontextprotocol/go-sdk/releases/tag/v1.8.0
[gs-authcode]: https://github.com/modelcontextprotocol/go-sdk/blob/main/auth/authorization_code.go
[gs-shared]: https://github.com/modelcontextprotocol/go-sdk/blob/main/auth/shared.go
[gs-authmeta]: https://github.com/modelcontextprotocol/go-sdk/blob/main/oauthex/auth_meta.go
[mcpgo]: https://github.com/mark3labs/mcp-go
[mcpgo-100]: https://github.com/mark3labs/mcp-go/releases/tag/v1.0.0
[mcpgo-951]: https://github.com/mark3labs/mcp-go/pull/951
[mcpgo-oauth]: https://github.com/mark3labs/mcp-go/blob/main/client/transport/oauth.go
[gomcp]: https://github.com/ThinkInAIXYZ/go-mcp
[mcpgolang]: https://github.com/metoro-io/mcp-golang
