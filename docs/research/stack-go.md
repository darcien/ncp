# Research: Go stack for a new MCP CLI client

Status: 2026-10-06. Baseline: [requirements.md](../requirements.md).
Prior work: [go-clients.md](go-clients.md). This report does not repeat it.
Stack: [`modelcontextprotocol/go-sdk`][gosdk] v1.8.0 (client side), a
credential store, a CLI parser, JSON Schema validation, and TTY detection.

Method: I read repository metadata, releases, commits, contributors, issues,
PRs, security advisories, workflows, and source files through `gh api`. I read
the go-sdk source at tag `v1.8.0` and at `main` (`172aebf`, 43 commits after
`v1.8.0`). I read module data from the deps.dev API, OSV, the OpenSSF
Scorecard API, and Go release notes and package docs on go.dev and pkg.go.dev.
I did not clone, build, install, or run any code. "Unknown" means not
verified. "Not tested" means that I read the code path but did not run it.

Legend for section 3:

- **Has**: the SDK meets the requirement through its public API.
- **Caller code**: the CLI meets it with its own code on public SDK APIs. No
  SDK change.
- **SDK gap**: an SDK change is necessary (upstream fix or fork fix).
- **Structural gap**: the SDK design prevents it.

## 1. Philosophy and stance

| Item | Finding | Source |
|---|---|---|
| Stated goals | Complete, idiomatic, robust, future-proof, extensible. Minimal core, with extension through interfaces, middleware, and hooks. | [design.md][gs-design] |
| API style | One main package `mcp` (like `net/http`). `Client` → `Connect(ctx, Transport)` → `ClientSession` with typed methods (`ListTools`, `CallTool`). `context.Context` on each call. Iterators for pagination. Middleware on send and receive. | [design.md][gs-design], [client.go][gs-client] |
| Scope: what it does not do | No token persistence (caller hooks only). No browser start, no redirect listener. No config file. Not implemented on the client: OAuth client credentials, DPoP (SEP-1932), Workload Identity JWT bearer (SEP-1933). | [baseline.yml][gs-baseline], [protocol.md][gs-protocol] |
| Dependency policy | "As few dependencies as possible." Each new module dependency needs a proposal. Rule: no dependency types in the SDK API. Note: the `auth` API exposes `oauth2.TokenSource` and `*oauth2.Config`. | [CONTRIBUTING.md][gs-contrib] |
| Spec changes | Support for a new spec revision comes in a minor release. Removal of a revision is a breaking change (v2). Spec-deprecated features (roots, sampling, logging) stay at least 12 months. | [CONTRIBUTING.md][gs-contrib], [README][gs-readme] |
| Compatibility promise | Semver through the Go module path. Breaking change = new `/v2` path. Covers `mcp`, `jsonrpc`, `auth`, `auth/extauth`, `oauthex`. `internal/` has no promise. | [CONTRIBUTING.md][gs-contrib] |
| Deprecation policy | `// Deprecated:` comment; the API stays until the next major version. Behavior changes for bug or security fixes get a temporary `MCPGODEBUG` opt-out, kept for about two minor releases. | [CONTRIBUTING.md][gs-contrib], [mcpgodebug.md][gs-godebug] |
| Known v1 API mistakes | Listed in `rough_edges.md`. Example: a nil `ClientOptions.Capabilities` advertises `roots` with `listChanged: true`. | [rough_edges.md][gs-rough] |
| Go version policy | Targets only supported Go versions. `go.mod`: `go 1.25.0`. CI: Go 1.25 and 1.26. | [go.mod][gs-gomod], [test.yml][gs-testyml] |
| Governance (text) | "Administered by the Go team and Anthropic." Approvers merge PRs. Proposals: label `proposal`, open at least one week, a maintainer approves. Unclear proposals go to a Working Group meeting. MCP antitrust policy applies. | [CONTRIBUTING.md][gs-contrib] |
| Maintainers (named) | Maciej Kisiel, Yaroslav Shevchuk, Guglielmo Colombo, Jonathan Amsterdam. All four show Google in their GitHub profiles. Robert Findley (Go tools) was the top committer before 2026-03. | [MCP MAINTAINERS.md][mcp-maint], GitHub profiles |
| Decision speed (sample) | Proposal #901 (OAuth session persistence) opened 2026-04-21. PR #1058 merged 2026-07-13: 83 days. | [#901][gs-901], [#1058][gs-1058] |

Notes:

- The README marks client OAuth as "experimental" for v1.4.0+. The v1.5.0
  notes (2026-04-07) say the OAuth APIs are stable and the build tag
  `mcp_go_client_oauth` is gone. The README footnote is out of date.
- The project calls itself a Tier 1 SDK target (SEP-1730, "Completed" in
  [ROADMAP.md][gs-roadmap]).

## 2. Project health

### 2.1 Activity (2025-10-06 to 2026-10-06, GitHub API)

| Metric | Value |
|---|---|
| Stars / forks / open issues+PRs | 5187 / 569 / 119 (2026-10-06) |
| Stable releases | 11: v1.1.0 (2025-10-30), v1.2.0 (2025-12-22), v1.3.0 (2026-02-09), v1.3.1, v1.4.0 (2026-02-27), v1.4.1, v1.5.0 (2026-04-07), v1.6.0, v1.6.1, v1.7.0 (2026-07-28), v1.8.0 (2026-09-14) |
| Pre-releases | 12 (`-pre.N` before most minor releases) |
| Commits on `main` | 353 (32 by dependabot, 321 by people) |
| Commits per month | 25, 8, 34, 16, 36, 28, 25, 25, 29, 35, 37, 47 (2025-10 to 2026-09) |
| Distinct commit authors | 104 (includes bots) |
| Top committers (12 months, 321 human commits) | guglielmo-san 65 (20%), maciej-kisiel 47 (15%), findleyr 45 (14%). Top 3 = 49%. The 5 Google people above = 171 (53%). |
| Top committers (since 2026-06-01, 140 human commits) | guglielmo-san 58 (41%); next yarolegovich 9 |
| Last commit (person) | guglielmo-san 2026-09-30, maciej-kisiel 2026-05-15, jba 2026-03-27, findleyr 2026-03-03 |
| PR merges (last 100 merged, 2026-07-15 to 2026-10-06) | guglielmo-san 90, yarolegovich 10 |

Bus factor: low. One maintainer merged 90 of the last 100 PRs. Two of four
listed maintainers have no commit since 2026-05.

### 2.2 Response time (sample, GitHub API, 2026-10-06)

| Sample | Maintainer comment or close | Median | 75th percentile | No action yet |
|---|---|---|---|---|
| 30 newest issues (created 2026-09-15 to 2026-10-04) | 22 of 30 | 24.2 h | 60.5 h | 8 |
| 30 newest non-dependabot PRs (created 2026-09-24 to 2026-10-05) | 22 of 30 (20 merged) | 15.7 h | 66.3 h | 8 |

"Maintainer" = guglielmo-san, yarolegovich, maciej-kisiel, jba, findleyr.

### 2.3 Security

| Item | Finding |
|---|---|
| SECURITY.md | Yes. Private report through GitHub Security Advisories. No response-time promise. ([SECURITY.md][gs-security]) |
| Past advisories | 4, all "high", all fixed in ≤ v1.4.1: [GHSA-wvj2-96wp-fq3f][ghsa1] (CVE-2026-27896, case-insensitive JSON, fixed v1.3.1); [GHSA-q382-vc8q-7jhj][ghsa2] (null Unicode in JSON, v1.4.1); [GHSA-89xv-2j6f-qhc8][ghsa3] (CVE-2026-33252, cross-site tool execution, server, v1.4.1); [GHSA-xw59-hvm2-8pj6][ghsa4] (CVE-2026-34742, DNS rebinding, server, v1.4.0) |
| Open advisories for v1.8.0 | None (deps.dev `advisoryKeys: []`, 2026-10-06) |
| OpenSSF Scorecard | 9.4 (2026-10-06). Fuzzing 10, SAST 10, Code-Review 10, Pinned-Dependencies 8, Signed-Releases n/a |
| Dependency scan | Nightly `govulncheck` ([nightly.yml][gs-nightly]). CodeQL workflow. |

### 2.4 CI and conformance

| Item | Finding |
|---|---|
| Unit tests | `go test ./...` on Go 1.25 and 1.26, plus `-race` on 1.26. Ubuntu only. No macOS runner. ([test.yml][gs-testyml]) |
| Conformance | Official `@modelcontextprotocol/conformance` `0.2.0-alpha.11` on each PR and push: server for `2025-11-25` (stateful) and `2026-07-28` (stateless); client full suite. ([conformance.yml][gs-conf]) |
| Client expected failures | Only out-of-scope auth: client credentials (2), Enterprise-Managed Authorization, DPoP (2), WIF JWT bearer. ([baseline.yml][gs-baseline]) |

### 2.5 Spec adoption latency

| Spec | RC date | Final date | First SDK support | Full SDK support | Latency from final |
|---|---|---|---|---|---|
| 2026-07-28 | 2026-05-29 | 2026-07-28 | v1.7.0-pre.1, 2026-06-24 | v1.7.0, 2026-07-28 | 0 days |
| 2025-11-25 | 2025-11-15 | 2025-11-25 | v1.2.0 (partial), 2025-12-22 | v1.4.0 (with experimental client OAuth), 2026-02-27; stable client OAuth v1.5.0, 2026-04-07 | 27 / 94 / 133 days |

Sources: [spec releases][mcp-rel], [go-sdk releases][gs-rel].

## 3. Requirement fit

Links point to tag `v1.8.0` unless noted.

| ID | SDK API | Result | Notes |
|---|---|---|---|
| P1 | `Client.Connect` sends `server/discover`; `injectRequestMeta` adds `_meta` protocolVersion, clientInfo, clientCapabilities to each request; standard headers `Mcp-Protocol-Version`, `Mcp-Method`, `Mcp-Name` ([client.go#L308][c-connect], [client.go#L418][c-discover], [streamable.go#L2428][s-headers]) | Has | Default since v1.7.0. Conformance client suite passes in CI. |
| P2 | Same `Connect`. `ClientSessionOptions.ProtocolVersion` pins a version ([client.go#L249][c-opts]) | Has, plus SDK gap (stdio timeout), plus Caller code (cache) | See 3.1. |
| P4 | `CommandTransport` (stdio, [cmd.go#L20][cmd]); `StreamableClientTransport` ([streamable.go#L1966][s-transport]) | Has | `SSEClientTransport` covers P5. |
| P6 | `ListTools`, `Tools` iterator, `CallTool` ([client.go#L1263][c-list], [client.go#L1284][c-call]) | Has | `ListTools` drops tools with invalid `x-mcp-header` annotations and logs an error ([streamable_headers.go][s-filter], main). |
| P8 | MRTR middleware on by default; `MultiRoundTripOptions{Disabled: true}` returns the result with `InputRequests` and `RequestState` (`CallToolResult.NeedsInput`) ([mrtr.go#L24][mrtr-opts], [mrtr.go#L73][mrtr-mw]) | Has | Default mode calls `ElicitationHandler` and retries, max 10 rounds. For agents: disable it, print the result, exit with the input-required code. |
| P10 | Context cancel → `notifications/cancelled` with the request ID ([transport.go#L294][t-call], [transport.go#L343][t-cancel]) | Has, plus Caller code | v1.8.0 sends the notification in a goroutine after `CallTool` returns. The CLI must wait for it before exit. See 3.2. |
| A1 | `auth.AuthorizationCodeHandler`: PKCE S256, random `state`, state check ([authorization_code.go#L600][a-getcode]) | Has | Rejects an AS with no `S256` (prior report). |
| A2 | PRM from `WWW-Authenticate`, path, root; RFC 8414 and OIDC URLs ([authorization_code.go#L286][a-authorize], [shared.go][a-shared]) | Has | Uses only `authorization_servers[0]`. Falls back to `/authorize`, `/token`, `/register` when no metadata exists. |
| A3 | `resource` in the authorization request ([L600][a-getcode]) and code exchange ([L655][a-exchange]) | Has for these two. **Caller code** for refresh | Refresh uses `x/oauth2` `tokenRefresher`, which sends only `grant_type` and `refresh_token` ([oauth2.go#L274][oauth2-refresh]). Fix: a `NewTokenSource` hook with an own refresher, or a `RoundTripper` on `Config.Client` that adds `resource` to `refresh_token` grants. |
| A4 | `validateIssuerResponse` ([authorization_code.go#L631][a-iss]) | Has | Default rejects a matching `iss` when the AS does not advertise `authorization_response_iss_parameter_supported`. Set `AcceptUnadvertisedIss: true` for interop. |
| A5 | Registration order CIMD → `PreregisteredClient` → DCR ([authorization_code.go#L546][a-reg]) | Caller code | The SDK keeps no client registration. `PreregisteredClient.Issuer` mismatch returns an error; it does not register again. DCR runs on each `Authorize`. The DCR result reaches the caller only through `NewTokenSource(*oauth2.Config)`, with no issuer. CLI: discover the issuer first with `auth.GetAuthServerMetadata` and `oauthex.GetProtectedResourceMetadata`, load the client for that issuer, and on a mismatch register again. |
| A8 | In process: `oauth2` token source refresh in `setMCPHeaders` ([streamable.go#L2428][s-headers]). Across processes: `InitialTokenSource`, `NewTokenSource` ([authorization_code.go#L75][a-config]) | Has (in process), Caller code (persistence) | `invalid_grant` on refresh → request without token → 401 → `Authorize` (interactive login). The refresh runs on `context.Background()` ([L674][a-refreshctx]); Ctrl-C does not stop it. Set a timeout on the HTTP client. |
| A10 | `AuthorizationCodeFetcher(ctx, {URL})` returns `{Code, State, Iss}` ([authorization_code.go#L72][a-config]) | Caller code | The CLI prints the URL and reads the pasted redirect URL. The SDK checks `state` and `iss`. |
| A11 | No header option on `StreamableClientTransport` | Caller code | `HTTPClient` with a `RoundTripper` that adds the header. A static `OAuthHandler` covers bearer tokens only. |
| N2 | Default `http.DefaultClient` for MCP and OAuth ([streamable.go#L2047][s-connect]; `Config.Client` default in `NewAuthorizationCodeHandler`) | Has | Go `ProxyFromEnvironment`: upper and lower case, `NO_PROXY`, user:pass in the proxy URL. Go never proxies `localhost` or loopback targets. A custom client must keep `Proxy: http.ProxyFromEnvironment`. |
| N3 | Go `crypto/x509` | Has | macOS: platform verifier with Keychain roots, including admin-added roots ([Go 1.18 notes][go118]). Linux: system bundle paths. Caveat: from Go 1.27, `SSL_CERT_FILE` or `SSL_CERT_DIR` on macOS turns off the platform verifier ([crypto/x509][x509]). |
| N5 | `CommandTransport.Command` is a caller `*exec.Cmd`; nil `Env` inherits `os.Environ()` ([cmd.go#L20][cmd]) | Has | Not tested. |
| S4 | Same `*exec.Cmd` | Caller code | `cmd.Env = append(os.Environ(), "NAME="+secret)` after the keyring read. The SDK does not touch `Env`. |

### 3.1 P2 detail: era detection

| Question | Finding |
|---|---|
| How | `Connect` with the default version (`2026-07-28`) sends `server/discover`. On `-32022` with `supported` data, it retries discover with a mutual version (max 2 tries). On any other error, it sends `initialize` with `2025-11-25` ([client.go#L308][c-connect]). Since v1.7.0 (#1014): "fall back ... on any error". |
| Timeouts | No internal bound. Discover and `initialize` use the caller `ctx`. A stdio server that ignores unknown requests before `initialize` makes `Connect` block until the deadline; the fallback then fails with the same expired `ctx` ([#1332][gs-1332], open). [PR #1334][gs-1334] (open, 2026-10-06) adds a 3 s bound on stdio only. HTTP: legacy servers answer discover with an error, so fallback is fast (not tested). |
| Caching | None across processes. No API to inject a cached discover result. The CLI can pin `ProtocolVersion: "2025-11-25"` for a known legacy server; this skips discover. A modern server costs one discover round trip per process. |
| Per-session caches | `ttlMs` caches for list results are in memory, per `ClientSession` ([cache.go][g-cache], main). `Mcp-Param-*` headers for `tools/call` need the tool definition from a `tools/list` in the same session (`lookupTool`, [client.go#L599][c-lookup]). A one-shot call with no list sends no param headers. Fix: list first, or add the headers in a `RoundTripper`. |

### 3.2 P10 detail: Ctrl-C

| Step | Finding |
|---|---|
| Signal | `signal.NotifyContext(ctx, os.Interrupt)` cancels the call context (stdlib). |
| Notification | `call()` retires the call and sends `notifications/cancelled` in a goroutine with a 5 s timeout ([transport.go#L294][t-call]). `CallTool` returns `ctx.Err()` at once. If `main` exits first, the notification is lost. |
| Sync option | `MCPGODEBUG=blockingcancelnotify=1` restores a synchronous send. The flag is "removed in 1.9.0" ([mcpgodebug.md][gs-godebug]). Do not depend on it. |
| CLI fix (HTTP) | Watch for the `notifications/cancelled` POST in the `RoundTripper`. Wait for it with a short deadline. |
| CLI fix (stdio) | Wrap the stdio `Connection` to see the write. Start the child with `SysProcAttr{Setpgid: true}`, so terminal Ctrl-C does not kill the server before it gets the notification. |
| Wrapper limit | A custom `Connection` wrapper loses the unexported `sessionUpdated` hook. For a `2025-11-25` HTTP session, requests after `initialize` then have no `Mcp-Protocol-Version` header and no standalone SSE stream ([#1328][gs-1328], open). Wrap HTTP at the `RoundTripper`, not at the `Connection`. |
| Shutdown cost | `CommandTransport` close waits `TerminateDuration` (default 5 s) before SIGTERM ([cmd.go#L69][cmd-close]). HTTP close sends `DELETE` for a session, bounded at 5 s. Set short values for a one-shot CLI. |

## 4. Extension points

| Point | API | Result | Notes |
|---|---|---|---|
| Own HTTP client (proxy, roots) | `StreamableClientTransport.HTTPClient`; `AuthorizationCodeHandlerConfig.Client` (PRM, AS metadata, DCR, code exchange, refresh); `httpClient` argument on `oauthex` and `auth.GetAuthServerMetadata` | Has | One `*http.Client` covers all MCP and OAuth traffic. |
| Own credential or token store | `InitialTokenSource` (load), `NewTokenSource` (wrap; sees each refresh and the `*oauth2.Config`) — PR [#1058][gs-1058], 2026-07-13 | Has | Client registration and issuer storage is caller work (A5). |
| Own stdio process spawn | `CommandTransport{Command: *exec.Cmd, TerminateDuration}`; `IOTransport` for own pipes | Has | Caller sets `Env`, `Dir`, `SysProcAttr`, `Stderr`. A nil `Stderr` sends server stderr to `/dev/null` (`os/exec` rule). |
| Per-phase timing (F2) | No SDK hook. `net/http/httptrace` in a `RoundTripper`: `DNSStart/Done`, `ConnectStart/Done`, `TLSHandshakeStart/Done`, `WroteRequest`, `GotFirstResponseByte`. `http.Transport.OnProxyConnectResponse` (Go 1.20+) marks the end of proxy `CONNECT` | Caller code | POST requests use the call `ctx` ([streamable.go#L2266][s-write]), so a trace on that `ctx` also works. The refresh request uses a background `ctx`; a `RoundTripper` trace covers it. With a proxy, DNS is for the proxy host only. Stdio phases: time `Transport.Connect` and the first response in a wrapper. |
| Cancellation from Ctrl-C | `context.Context` on each call | Has, plus Caller code | See 3.2. |
| Middleware | `AddSendingMiddleware`, `AddReceivingMiddleware` ([client.go][gs-client]) | Has | Sees MCP methods, not wire bytes. `LoggingTransport` logs JSON-RPC but has the #1328 defect. |

## 5. Dependencies

### 5.1 go-sdk v1.8.0 module graph

From [go.mod][gs-gomod] and deps.dev (`:requirements`, 2026-10-06). "Linked" = imported by the 52 non-test files under `mcp/`, `auth/`, `oauthex/`, `jsonrpc/`, `internal/` that I read on `main`.

| Module | Version | Type | License | Linked into a client | cgo |
|---|---|---|---|---|---|
| github.com/google/jsonschema-go | v0.4.3 | direct | MIT | Yes (`mcp`) | No |
| github.com/segmentio/encoding | v0.5.4 | direct | MIT, Apache-2.0 | Yes (`internal/json`) | No |
| github.com/segmentio/asm | v1.1.3 | indirect | MIT | Yes (Go assembly, 19 `.s` files) | No |
| github.com/yosida95/uritemplate/v3 | v3.0.2 | direct | BSD-3-Clause | Yes (resource templates) | No |
| golang.org/x/oauth2 | v0.35.0 | direct | BSD-3-Clause | Yes (`auth`) | No |
| golang.org/x/time | v0.15.0 | direct | BSD-3-Clause | Yes (`rate`, logging) | No |
| golang.org/x/sync | v0.20.0 | indirect | BSD-3-Clause | Yes (`errgroup`, MRTR) | No |
| golang.org/x/sys | v0.41.0 | indirect | BSD-3-Clause | Unknown which packages import it | No |
| github.com/golang-jwt/jwt/v5 | v5.3.1 | direct | MIT | No (examples, tests) | No |
| github.com/google/go-cmp | v0.7.0 | direct | BSD-3-Clause | No (tests) | No |
| golang.org/x/tools | v0.42.0 | direct | BSD-3-Clause | No (not in the files I read) | No |

- go-sdk module licenses (deps.dev): Apache-2.0, MIT, CC-BY-4.0.
- Advisory on the graph: `x/sys` v0.41.0 has GO-2026-5024 (CVE-2026-39824). It
  is in `x/sys/windows` only. No effect on macOS or Linux.
- No cgo in the graph. `CGO_ENABLED=0` builds of go-sdk work on macOS and
  Linux. Evidence: mcpx v1.2.1 ships darwin and linux builds with
  `CGO_ENABLED=0`, go-sdk v1.7.0 and go-keyring ([.goreleaser.yml][mx-gr]).
- On macOS, Go has not used cgo for `net` and `os/user` since Go 1.20
  ([Go 1.20 notes][go120]). Every darwin binary links `libSystem` and system
  frameworks; no third-party dylib is necessary.

### 5.2 Credential store libraries

| Library | Version (date) | License | macOS Keychain method | Secret in argv (S3) | Linux | `CGO_ENABLED=0` on macOS | Health |
|---|---|---|---|---|---|---|---|
| [zalando/go-keyring][zk] | v0.2.8 (2026-03-23) | MIT | exec `/usr/bin/security` ([keyring_darwin.go][zk-darwin]) | No. `Set` writes `add-generic-password ... -w <secret>` to stdin of `security -i`; argv is `-i`. `Get` reads the secret from stdout | Secret Service through pure-Go `godbus/dbus/v5` v5.2.2 (BSD-2-Clause) ([keyring_unix.go][zk-unix]) | Yes | 1340 stars. 33 commits in 12 months (8 dependabot, 5 Copilot). 2 Zalando maintainers. Scorecard 7.5 (2026-09-28). Used by GitHub CLI (`cli/cli` go.mod) |
| [99designs/keyring][99k] | v1.2.2 (2022-12-19) | MIT | cgo, `99designs/go-keychain` (`//go:build darwin && cgo`) | No | Secret Service, KWallet, keyctl, pass, encrypted file | No: Keychain backend is not compiled | Last push 2024-05-07. Old `godbus/dbus` pseudo-version from 2019. Not maintained |
| [keybase/go-keychain][kbk] | v0.0.1 (2025-02-27), only tag | MIT | cgo, Security and CoreFoundation frameworks | No | — | No: package needs `import "C"` | 670 stars, push 2026-09-09 |
| [byteness/keyring][bk] | v1.13.2 (2026-09-15) | MIT | cgo (`byteness/go-keychain`, a 2019 fork) | No | Secret Service, more | No (Unknown for exact tags) | 21 stars. Pulls 1Password SDKs and Touch ID modules. Not lean |
| Own binding on [ebitengine/purego][purego] | purego v0.9.1 | Apache-2.0 | Calls Security framework at run time, no cgo | No | — | Yes | purego: 4002 stars. Ready Keychain wrappers are small: `lstoll/keychain` (5 stars, "early development"). go-keyring PR [#136][zk-136] (purego) closed unmerged 2026-05-06 |

Result: only zalando/go-keyring gives a `CGO_ENABLED=0` build with a maintained
library. Linux needs no cgo with go-keyring.

### 5.3 CLI parser, TTY, JSON Schema

| Need | Option | Version (date) | License | Runtime deps | Dead-code elimination (DCE) | Note |
|---|---|---|---|---|---|---|
| CLI | stdlib `flag` | Go | BSD-3 | none | OK | No subcommands; stops at the first non-flag argument. mcpx uses no CLI library ([go.mod][mx-gomod]) |
| CLI | [spf13/cobra][cobra] | v1.10.2 (2025-12-04) | Apache-2.0 | `spf13/pflag` (BSD-3). `go-md2man`, `yaml` only in `doc`; `mousetrap` only on Windows | OK unless custom templates are set ([PR #1956][cobra-1956], 2025-01-27) | Shell completions built in (I7). 44684 stars. Used by GitHub CLI |
| CLI | [alecthomas/kong][kong] | v1.16.1 (2026-08-09) | MIT | none | **Off**: `MethodByName(name)` with a variable name ([callbacks.go#L122][kong-cb]) | Struct-tag model |
| CLI | [urfave/cli/v3][urfave] | v3.14.0 (2026-10-02) | MIT | none at run time | Off with `text/template`; build tag `urfave_cli_no_template` fixes it ([PR #2441][urfave-2441], merged 2026-10-01) | PR data: example CLI 5.1 MB → 3.2 MB with the tag |
| TTY | `golang.org/x/term` `IsTerminal` | v0.37.0+ | BSD-3 | `x/sys` (already in the graph) | OK | Stdlib `os.ModeCharDevice` also matches `/dev/null`; not exact |
| JSON Schema | [google/jsonschema-go][jsg] | v0.4.3 (2026-04-17) | MIT | none (already in the graph) | No `MethodByName` found (code search) | Draft 2020-12 and draft-07 only. `format` is an annotation, not checked. Remote `$ref` fails unless a `Loader` is set ([resolve.go][jsg-resolve]): good for N1. `pattern` uses Go RE2: ECMA-262 lookahead or backreferences fail at `Resolve`. Last commit 2026-05-22 |
| JSON Schema | [santhosh-tekuri/jsonschema/v6][stj] | v6 | Apache-2.0 | `golang.org/x/text` | Unknown | 1278 stars. More drafts. Adds one module |

## 6. Credential store details

### 6.1 macOS Keychain

| Item | zalando/go-keyring (exec `security`) | Direct Security framework (cgo or purego) |
|---|---|---|
| Item type and naming | Generic password in the default keychain. `-s <service>` (service), `-a <user>` (account). Value stored as `go-keyring-base64:<base64>` | Caller chooses attributes |
| ACL owner | `/usr/bin/security` (Apple-signed) creates the item, so it is the trusted app | The calling binary, bound to its code signature |
| Unsigned or ad-hoc signed CLI | No prompt. Updates of the CLI do not change access | Ad-hoc signature changes with each build. macOS shows an access prompt after each update ([#136 comment][zk-136], 2026-05-09). Not tested |
| Developer ID signed CLI | No difference | Stable designated requirement; no prompt after an update. Not tested |
| Isolation from other processes of the same user | None. Any process of the user can run `security find-generic-password -w` with no prompt ([#110][zk-110], open since 2024-09-02) | Other apps get a prompt |
| Agents with no GUI | No prompt, so no hang (login keychain unlocked) | A prompt blocks a headless agent |
| Size limit | Command line max 4096 bytes, including base64 (≈ 4/3 growth). Practical secret limit ≈ 2.9 KB minus names. `ErrSetDataTooBig` above it | Unknown limit; large |
| Open defects | [#153][zk-153]: a failed `Set` at exactly 4096 bytes returns `nil`. [#154][zk-154]: a backslash in service or account names is dropped; a single quote fails. Both open, no reply (2026-09-03) | — |
| Data protection keychain | Not available | Needs a keychain-access-group entitlement; plain CLI tools do not have it ([TN3137][tn3137]). Not verified here |
| Cost per read | One `fork`+`exec` of `security`. Time: Unknown | In-process call. Time: Unknown |

Rules for the CLI with go-keyring:

1. Use ASCII names with no backslash and no quote.
2. Store each secret as one item, and keep each item under about 2.5 KB.
   Split access token, refresh token, and client secret.
3. Do not store a large JSON session in one item.
4. Add a timeout around each call. go-keyring has no `context` (issue #131).
   GitHub CLI wraps it with a 60 s timeout ([cli/cli keyring.go][gh-keyring]).

### 6.2 Linux Secret Service

| Item | Finding (go-keyring v0.2.8, godbus v5.2.2) |
|---|---|
| Transport | Session D-Bus, pure Go. `OpenSession` with algorithm `plain`: the secret is not encrypted on the bus socket ([secret_service.go][zk-ss]) |
| Item naming | Collection `login` (else alias `default`). Attributes `service`, `username`. Label `Password for '<user>' on '<service>'` |
| Locked collection | `Unlock` starts a Secret Service prompt and waits for it. With no GUI, the wait time is Unknown. Use a timeout |
| No `DBUS_SESSION_BUS_ADDRESS` | godbus checks `/run/user/<uid>/bus` and `/run/user/<uid>/dbus-session`. If neither exists, it runs `dbus-launch` and calls `os.Setenv` with the new address ([conn_other.go][godbus-other]). Effects: a new bus daemon can start; the changed environment goes to stdio servers (N5, S4). Check the bus before the keyring call |
| Bus present, no secrets provider | Error from D-Bus (name not provided). No fallback |
| File fallback | None. Request open ([#130][zk-130]). The CLI must add the `0600` opt-in file itself (S2) |
| keyctl | Not supported (PR #129 closed unmerged) |

## 7. Startup and size

| Item | Value | Source or basis |
|---|---|---|
| Published size, go-sdk based CLI | mcpx v1.2.1 archives: darwin/arm64 3.85 MB, darwin/amd64 4.26 MB, linux/arm64 3.84 MB, linux/amd64 4.26 MB (`.tar.gz` with README and man page). Build: `CGO_ENABLED=0`, `-s -w`, go-sdk v1.7.0, go-keyring, TOML, daemon | [mcpx release][mx-rel], [.goreleaser.yml][mx-gr] |
| Uncompressed binary | Unknown. Estimate 8–12 MB, if Go binaries compress to 35–50% (assumption, not measured) | Reasoning |
| DCE blockers in go-sdk | None found: no `MethodByName`, no `text/template` in the 52 non-test library files | grep on `main` |
| DCE blockers in other choices | kong (yes); urfave/cli (yes, unless build tag); cobra (no, with default templates); godbus `export.go` calls `reflect.Type.Method(i)` — reachability Unknown | Sections 5.3, [export.go][godbus-export] |
| Size effect of DCE | Cobra PR: 40% of a small example and 12% of Delve were dead code. urfave PR: 5.1 → 3.2 MB | [cobra #1956][cobra-1956], [urfave #2441][urfave-2441] |
| Package `init` work in go-sdk | `mcpgodebug` parses `MCPGODEBUG`; `mcp/logging.go` fills an 8-entry map; `mcp/resource.go` reads one flag; `mcp/streamable.go` stores one int | grep on `main` |
| `init` work in jsonschema-go | Reflects over the `Schema` struct fields and sorts them; one small `regexp.MustCompile`; a 5-entry type map | [schema.go][jsg-schema], [infer.go][jsg-infer] |
| Schema cost at run time | `pattern` regexps compile at `Resolve`, per schema, not at start | [resolve.go][jsg-resolve] |
| Process start time | Unknown. No published number for this stack. No network and no file access at `init` in go-sdk | Reasoning |
| Main per-run costs after start | macOS: one `security` process per secret read. Linux: D-Bus connect, `OpenSession`, `Unlock`, search, get. Modern server: one `server/discover` round trip. Times: Unknown | Sections 3.1, 6 |
| Startup risk | A malformed `MCPGODEBUG` value makes `init` panic before `main` ([mcpgodebug.go][gs-godebug-go], main) | Code |

## 8. Risks and unknowns

| # | Risk or unknown | Effect | Mitigation |
|---|---|---|---|
| 1 | One active merger: 90 of the last 100 merged PRs by one maintainer. Two of four maintainers have no commit since 2026-05 | Slow fixes if that person leaves | Pin versions; keep the CLI on public APIs only |
| 2 | Stdio era detection hangs on a silent legacy server ([#1332][gs-1332]); fix PR open | P2 failure until the deadline | Own deadline; on timeout, restart the server and pin `2025-11-25` |
| 3 | Async `notifications/cancelled`; sync flag removed in v1.9.0 | P10 notification lost at exit | Wait for the write (section 3.2) |
| 4 | `Connection` wrappers break legacy HTTP sessions ([#1328][gs-1328]) | Missing headers, no SSE stream | Wrap at `RoundTripper` for HTTP |
| 5 | Refresh sends no `resource` (x/oauth2) | A3 not met for refresh | Own refresher or `RoundTripper` |
| 6 | Refresh uses a background context | Ctrl-C does not stop a slow refresh | `http.Client.Timeout` on the OAuth client |
| 7 | go-keyring on macOS: no isolation from same-user processes ([#110][zk-110]) | Weaker S1 than a per-app ACL | Accept and document, or own purego binding with Developer ID signing |
| 8 | go-keyring size limit ≈ 2.9 KB per secret and silent failure at the edge ([#153][zk-153]) | Lost tokens with large JWTs | Split items; check size before `Set`; read back after `Set` |
| 9 | godbus runs `dbus-launch` and changes the process environment when no bus exists | Stray daemon; env change reaches stdio servers | Check the bus first; snapshot `os.Environ()` before keyring use |
| 10 | Go 1.27: `SSL_CERT_FILE` on macOS turns off the platform verifier | N3 company roots lost when the variable is set | Document; warn when it is set on macOS |
| 11 | jsonschema-go: RE2 `pattern`, two drafts only, no commit since 2026-05-22 | I5 validation fails for some schemas | Treat a `Resolve` error as "not validated", do not block the call |
| 12 | No macOS CI in go-sdk | Darwin stdio and signal defects found late | Own macOS CI |
| 13 | Default client capabilities advertise `roots` | P14 (roots not wanted) | Set `Capabilities: &mcp.ClientCapabilities{}` |
| 14 | Notification handlers can run after `CallTool` returns ([#1337][gs-1337], open) | Progress output (P11) after the result | Drain or ignore late progress |
| 15 | No cross-process era or list cache API; `Mcp-Param-*` headers need a list in the same session | Extra round trips per run | Own cache plus own headers in `RoundTripper`, or a daemon (F4) |
| 16 | `main` after v1.8.0 changes `StructuredContent` numbers to `json.Number` (v1.9.0 entry) | Output schema of the CLI changes on upgrade | Pin and test on upgrade |
| 17 | Unknown: process start time, keyring read time on both OSes, binary size of this exact stack | F3 and D1 numbers missing | Measure in the prototype |
| 18 | Unknown: macOS `security` behavior with a locked login keychain over SSH | Headless use on macOS | Test |

## 9. Verdict

- **Strength:** go-sdk is the official SDK with fast spec adoption (2026-07-28
  support on release day), conformance tests in CI, semver with a written
  compatibility and deprecation policy, and no cgo. A `CGO_ENABLED=0` binary
  for macOS and Linux is proven by mcpx.
- **Strength:** the extension points cover the CLI needs: own `*http.Client`
  for MCP and OAuth (N2, N3, F2 through `httptrace`), own `*exec.Cmd` (N5,
  S4), token load and save hooks (A8), and a caller fetcher for paste login
  (A10). No MUST requirement is a structural gap.
- **Weakness:** several MUST items need careful caller code: A3 on refresh,
  A5 per-issuer registration, A11 headers, P10 wait-before-exit, and the P2
  stdio timeout (#1332, fix still open).
- **Weakness:** the Go credential store options are poor. go-keyring is the
  only maintained no-cgo choice. On macOS it exposes items to every
  same-user process, limits secrets to about 2.9 KB, and has open silent
  failure defects. Project health also depends on one active merger.
- **Main open question:** is go-keyring's `/usr/bin/security` model
  acceptable for S1, or does the CLI need an own purego Keychain binding
  with Developer ID signing to get a per-app ACL without update prompts?

## Sources

- go-sdk: [repository][gosdk], [releases][gs-rel] (v1.2.0, v1.4.0, v1.5.0, v1.7.0, v1.8.0 notes), [README][gs-readme], [CONTRIBUTING.md][gs-contrib], [design.md][gs-design], [ROADMAP.md][gs-roadmap], [rough_edges.md][gs-rough], [mcpgodebug.md][gs-godebug], [protocol.md][gs-protocol], [SECURITY.md][gs-security], [go.mod][gs-gomod], [test.yml][gs-testyml], [conformance.yml][gs-conf], [nightly.yml][gs-nightly], [baseline.yml][gs-baseline]
- go-sdk source (v1.8.0): [client.go][gs-client], [cmd.go][cmd], [transport.go][t-call], [streamable.go][s-transport], [mrtr.go][mrtr-mw], [authorization_code.go][a-config], [shared.go][a-shared]; (main `172aebf`): [cache.go][g-cache], [streamable_headers.go][s-filter], [mcpgodebug.go][gs-godebug-go], `internal/jsonrpc2/conn.go`
- go-sdk issues and PRs: [#901][gs-901], [#1058][gs-1058], [#1313][gs-1313], [#1328][gs-1328], [#1332][gs-1332], [#1334][gs-1334], [#1337][gs-1337]
- Security: [GHSA-wvj2-96wp-fq3f][ghsa1], [GHSA-q382-vc8q-7jhj][ghsa2], [GHSA-89xv-2j6f-qhc8][ghsa3], [GHSA-xw59-hvm2-8pj6][ghsa4], [OSV query API](https://api.osv.dev/v1/query), [GO-2026-5024](https://osv.dev/vulnerability/GO-2026-5024), [Scorecard go-sdk](https://scorecard.dev/viewer/?uri=github.com/modelcontextprotocol/go-sdk), [Scorecard go-keyring](https://scorecard.dev/viewer/?uri=github.com/zalando/go-keyring)
- deps.dev: [go-sdk v1.8.0](https://deps.dev/go/github.com%2Fmodelcontextprotocol%2Fgo-sdk/v1.8.0), version records for each module in section 5
- MCP: [spec releases][mcp-rel], [MAINTAINERS.md][mcp-maint]
- x/oauth2: [oauth2.go v0.35.0][oauth2-refresh]
- Go: [Go 1.18 notes][go118], [Go 1.20 notes][go120], [crypto/x509][x509], [net/http/httptrace](https://pkg.go.dev/net/http/httptrace), [http.Transport.OnProxyConnectResponse](https://pkg.go.dev/net/http#Transport)
- Apple: [TN3137: On Mac keychain APIs and implementations][tn3137]
- zalando/go-keyring: [repository][zk], [keyring_darwin.go][zk-darwin], [keyring_unix.go][zk-unix], [secret_service.go][zk-ss], [MAINTAINERS](https://github.com/zalando/go-keyring/blob/master/MAINTAINERS), issues [#110][zk-110], [#130][zk-130], [#136][zk-136], [#153][zk-153], [#154][zk-154]
- godbus/dbus v5.2.2: [conn_other.go][godbus-other], [export.go][godbus-export]
- Other keyrings: [99designs/keyring][99k] (`keychain.go`, `go.mod`), [keybase/go-keychain][kbk] (`keychain.go`), [byteness/keyring][bk] (`go.mod`), [ebitengine/purego][purego], [lstoll/keychain](https://github.com/lstoll/keychain)
- GitHub CLI: [go.mod](https://github.com/cli/cli/blob/trunk/go.mod), [.goreleaser.yml](https://github.com/cli/cli/blob/trunk/.goreleaser.yml), [internal/keyring/keyring.go][gh-keyring]
- mcpx: [v1.2.1 release][mx-rel], [.goreleaser.yml][mx-gr], [go.mod][mx-gomod], `docs/release-checklist.md`, README
- CLI and schema libraries: [cobra][cobra] (go.mod, `command.go`, [PR #1956][cobra-1956]), [kong][kong] ([callbacks.go][kong-cb], go.mod), [urfave/cli][urfave] (go.mod, [PR #2441][urfave-2441]), [peterbourgon/ff](https://github.com/peterbourgon/ff) (go.mod), [golang/term](https://github.com/golang/term) (go.mod), [jsonschema-go][jsg] (README, `doc.go`, [resolve.go][jsg-resolve], [schema.go][jsg-schema], [infer.go][jsg-infer]), [santhosh-tekuri/jsonschema][stj] (go.mod)

[gosdk]: https://github.com/modelcontextprotocol/go-sdk
[gs-rel]: https://github.com/modelcontextprotocol/go-sdk/releases
[gs-readme]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/README.md
[gs-contrib]: https://github.com/modelcontextprotocol/go-sdk/blob/main/CONTRIBUTING.md
[gs-design]: https://github.com/modelcontextprotocol/go-sdk/blob/main/design/design.md
[gs-roadmap]: https://github.com/modelcontextprotocol/go-sdk/blob/main/ROADMAP.md
[gs-rough]: https://github.com/modelcontextprotocol/go-sdk/blob/main/docs/rough_edges.md
[gs-godebug]: https://github.com/modelcontextprotocol/go-sdk/blob/main/docs/mcpgodebug.md
[gs-godebug-go]: https://github.com/modelcontextprotocol/go-sdk/blob/172aebf1bf779694d1fcda355292040108758ade/internal/mcpgodebug/mcpgodebug.go#L24
[gs-protocol]: https://github.com/modelcontextprotocol/go-sdk/blob/main/docs/protocol.md
[gs-security]: https://github.com/modelcontextprotocol/go-sdk/blob/main/SECURITY.md
[gs-gomod]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/go.mod
[gs-testyml]: https://github.com/modelcontextprotocol/go-sdk/blob/main/.github/workflows/test.yml
[gs-conf]: https://github.com/modelcontextprotocol/go-sdk/blob/main/.github/workflows/conformance.yml
[gs-nightly]: https://github.com/modelcontextprotocol/go-sdk/blob/main/.github/workflows/nightly.yml
[gs-baseline]: https://github.com/modelcontextprotocol/go-sdk/blob/main/conformance/baseline.yml
[gs-client]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/mcp/client.go
[c-connect]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/mcp/client.go#L308
[c-discover]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/mcp/client.go#L418
[c-opts]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/mcp/client.go#L249
[c-lookup]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/mcp/client.go#L599
[c-list]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/mcp/client.go#L1263
[c-call]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/mcp/client.go#L1284
[cmd]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/mcp/cmd.go#L20
[cmd-close]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/mcp/cmd.go#L69
[t-call]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/mcp/transport.go#L294
[t-cancel]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/mcp/transport.go#L343
[s-transport]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/mcp/streamable.go#L1966
[s-connect]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/mcp/streamable.go#L2047
[s-write]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/mcp/streamable.go#L2266
[s-headers]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/mcp/streamable.go#L2428
[s-filter]: https://github.com/modelcontextprotocol/go-sdk/blob/172aebf1bf779694d1fcda355292040108758ade/mcp/streamable_headers.go#L266
[g-cache]: https://github.com/modelcontextprotocol/go-sdk/blob/172aebf1bf779694d1fcda355292040108758ade/mcp/cache.go
[mrtr-opts]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/mcp/mrtr.go#L24
[mrtr-mw]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/mcp/mrtr.go#L73
[a-config]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/auth/authorization_code.go#L75
[a-authorize]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/auth/authorization_code.go#L286
[a-reg]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/auth/authorization_code.go#L546
[a-getcode]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/auth/authorization_code.go#L600
[a-iss]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/auth/authorization_code.go#L631
[a-exchange]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/auth/authorization_code.go#L655
[a-refreshctx]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/auth/authorization_code.go#L674
[a-shared]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/auth/shared.go
[gs-901]: https://github.com/modelcontextprotocol/go-sdk/issues/901
[gs-1058]: https://github.com/modelcontextprotocol/go-sdk/pull/1058
[gs-1313]: https://github.com/modelcontextprotocol/go-sdk/issues/1313
[gs-1328]: https://github.com/modelcontextprotocol/go-sdk/issues/1328
[gs-1332]: https://github.com/modelcontextprotocol/go-sdk/issues/1332
[gs-1334]: https://github.com/modelcontextprotocol/go-sdk/pull/1334
[gs-1337]: https://github.com/modelcontextprotocol/go-sdk/issues/1337
[ghsa1]: https://github.com/modelcontextprotocol/go-sdk/security/advisories/GHSA-wvj2-96wp-fq3f
[ghsa2]: https://github.com/modelcontextprotocol/go-sdk/security/advisories/GHSA-q382-vc8q-7jhj
[ghsa3]: https://github.com/modelcontextprotocol/go-sdk/security/advisories/GHSA-89xv-2j6f-qhc8
[ghsa4]: https://github.com/modelcontextprotocol/go-sdk/security/advisories/GHSA-xw59-hvm2-8pj6
[mcp-rel]: https://github.com/modelcontextprotocol/modelcontextprotocol/releases
[mcp-maint]: https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/MAINTAINERS.md
[oauth2-refresh]: https://github.com/golang/oauth2/blob/v0.35.0/oauth2.go#L274
[go118]: https://go.dev/doc/go1.18
[go120]: https://go.dev/doc/go1.20
[x509]: https://pkg.go.dev/crypto/x509#SystemCertPool
[tn3137]: https://developer.apple.com/documentation/technotes/tn3137-on-mac-keychains
[zk]: https://github.com/zalando/go-keyring
[zk-darwin]: https://github.com/zalando/go-keyring/blob/66b55cc0c51cd3d8dabc0ac70da30f058b2fe54a/keyring_darwin.go
[zk-unix]: https://github.com/zalando/go-keyring/blob/66b55cc0c51cd3d8dabc0ac70da30f058b2fe54a/keyring_unix.go
[zk-ss]: https://github.com/zalando/go-keyring/blob/66b55cc0c51cd3d8dabc0ac70da30f058b2fe54a/secret_service/secret_service.go
[zk-110]: https://github.com/zalando/go-keyring/issues/110
[zk-130]: https://github.com/zalando/go-keyring/issues/130
[zk-136]: https://github.com/zalando/go-keyring/pull/136
[zk-153]: https://github.com/zalando/go-keyring/issues/153
[zk-154]: https://github.com/zalando/go-keyring/issues/154
[godbus-other]: https://github.com/godbus/dbus/blob/v5.2.2/conn_other.go
[godbus-export]: https://github.com/godbus/dbus/blob/v5.2.2/export.go#L77
[99k]: https://github.com/99designs/keyring
[kbk]: https://github.com/keybase/go-keychain
[bk]: https://github.com/byteness/keyring
[purego]: https://github.com/ebitengine/purego
[gh-keyring]: https://github.com/cli/cli/blob/trunk/internal/keyring/keyring.go
[mx-rel]: https://github.com/lydakis/mcpx/releases/tag/v1.2.1
[mx-gr]: https://github.com/lydakis/mcpx/blob/main/.goreleaser.yml
[mx-gomod]: https://github.com/lydakis/mcpx/blob/main/go.mod
[cobra]: https://github.com/spf13/cobra
[cobra-1956]: https://github.com/spf13/cobra/pull/1956
[kong]: https://github.com/alecthomas/kong
[kong-cb]: https://github.com/alecthomas/kong/blob/master/callbacks.go#L122
[urfave]: https://github.com/urfave/cli
[urfave-2441]: https://github.com/urfave/cli/pull/2441
[jsg]: https://github.com/google/jsonschema-go
[jsg-resolve]: https://github.com/google/jsonschema-go/blob/main/jsonschema/resolve.go
[jsg-schema]: https://github.com/google/jsonschema-go/blob/main/jsonschema/schema.go#L613
[jsg-infer]: https://github.com/google/jsonschema-go/blob/main/jsonschema/infer.go#L385
[stj]: https://github.com/santhosh-tekuri/jsonschema
