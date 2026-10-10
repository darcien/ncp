# Vendor MCP command-line clients

Status: research, 2026-10-06. Baseline: [requirements.md](../requirements.md).

Method: read-only. GitHub REST API (repo metadata, releases, contributors,
file contents), WebSearch, and WebFetch of READMEs and docs. No code was
cloned, installed, built or run. "Unknown" means not verified in docs or
source.

## 1. Result

No vendor CLI is acceptable as a fork base. Each candidate has a MUST
structural gap. All mature candidates are Node or Python (F1, D1). The Go
candidate (Docker) is a gateway front-end, not a direct client (P4, D1).

Two projects are strong references for design: the MCP Inspector CLI and
Apify `mcpc`. For the build path, the official Go SDK (v1.7.0+) and Rust SDK
(`rmcp` 3.0+) support `2026-07-28`. The official conformance suite tests
clients against `2026-07-28`, including auth.

## 2. Shortlist

| Project | Owner | Stars | Last commit | Latest release | License | Runtime | Lists/calls tools | Decision |
|---|---|---|---|---|---|---|---|---|
| [modelcontextprotocol/inspector](https://github.com/modelcontextprotocol/inspector) (`--cli`) | MCP project | 11,027 | 2026-09-30 | 2.9.0 (2026-09-30) | MIT, moving to Apache-2.0 | Node ≥22.19 | Yes | Deep read |
| [apify/mcpc](https://github.com/apify/mcpc) | Apify | 976 | 2026-10-05 | v0.7.0 (2026-09-23) | Apache-2.0 | Node ≥22.12 or Bun | Yes | Deep read |
| [docker/mcp-gateway](https://github.com/docker/mcp-gateway) (`docker mcp`) | Docker | 1,589 | 2026-09-16 | v0.44.1 pre-release (2026-09-23) | MIT | Go 1.25 | Yes, through the gateway | Deep read |
| [PrefectHQ/fastmcp](https://github.com/PrefectHQ/fastmcp) (`fastmcp list/call`) | Prefect | 27,980 | 2026-10-05 | v4.0.11 (2026-10-04) | Apache-2.0 | Python | Yes | Medium read |
| [openclaw/mcporter](https://github.com/openclaw/mcporter) | OpenClaw Foundation | 5,049 | 2026-10-01 | v0.14.2 (2026-10-01) | MIT | Node ≥24, Bun binary | Yes | Medium read |
| [IBM/mcp-cli](https://github.com/IBM/mcp-cli) | IBM (moved from chrishayuk) | 2,025 | 2026-07-30 | v0.20.1 (2026-07-22) | Apache-2.0 | Python | Through an LLM chat | Excluded: LLM chat app, single-author `chuk-*` dependencies |
| [modelcontextprotocol/conformance](https://github.com/modelcontextprotocol/conformance) | MCP project | 129 | 2026-10-01 | v0.1.16 (2026-03-27) | Unknown (GitHub: NOASSERTION) | Node | No (test harness) | Test tool, see §6 |
| [modelcontextprotocol/go-sdk](https://github.com/modelcontextprotocol/go-sdk) | MCP project, with Google | 5,187 | 2026-10-06 | v1.8.0 (2026-09-14) | Unknown (GitHub: NOASSERTION) | Go | Examples only | Build base, see §6 |
| [modelcontextprotocol/rust-sdk](https://github.com/modelcontextprotocol/rust-sdk) | MCP project | 3,979 | 2026-10-05 | rmcp-v3.5.1 (2026-10-05) | Unknown (GitHub: NOASSERTION) | Rust | Examples only | Build base, see §6 |
| [google-gemini/gemini-cli](https://github.com/google-gemini/gemini-cli) | Google | 107,246 | 2026-10-06 | — | Apache-2.0 | Node | No | Excluded: `gemini mcp` has only add, list, remove, enable/disable |
| [aaif-goose/goose](https://github.com/aaif-goose/goose) | Agentic AI Foundation (was Block) | 54,981 | 2026-10-06 | — | Apache-2.0 | Rust | No | Excluded: LLM agent, no direct tool-call command, has `update` command |
| Vercel `vercel mcp` | Vercel | — | — | — | — | Node | No | Excluded: writes client config for Vercel MCP |
| Cloudflare, GitHub, Microsoft | — | — | — | — | — | — | — | No CLI client found. They publish servers and SDKs. `cloudflare/mcp-remote` does not exist; `geelen/mcp-remote` redirects to `punkpeye/mcp-remote` (user account) |

Official docs: modelcontextprotocol.io calls the Inspector "the reference
developer tool". It ships web, CLI and TUI clients in one package. The docs
list no other official CLI client.

## 3. Facts per candidate

| Item | Inspector CLI | mcpc | docker mcp | fastmcp CLI | mcporter |
|---|---|---|---|---|---|
| Spec versions | `2026-07-28` and legacy. Per-server `protocolEra`: `legacy` (default), `auto`, `modern` | `2026-07-28` auto-negotiated, falls back to `2025-11-25` or older. `--protocol-version` pins | Legacy only. Uses go-sdk v1.4.1; `2026-07-28` came in go-sdk v1.7.0 | `2026-07-28` and legacy. `mode="auto"` probes `server/discover` | `2026-07-28` and legacy, per server |
| Transports | stdio, Streamable HTTP, SSE | stdio, Streamable HTTP | stdio to the gateway. Servers run as containers or catalog remotes | stdio, HTTP, SSE | stdio, Streamable HTTP, SSE |
| OAuth | Auth code + PKCE, PRM and AS discovery, DCR, CIMD (`--client-metadata-url`), static client, step-up, RFC 7009 revoke. Loopback callback only. No paste flow; remote use needs port forward or `--print-handoff` | Auth code + PKCE, PRM and AS discovery, RFC 8707, RFC 9207 `iss` (SDK checks it when present), pre-registered → CIMD (Apify-hosted default) → DCR. Manual URL paste fallback when no browser. Client credentials, ID-JAG | PKCE (docs). DCR through Docker Desktop, or "CE mode" with a loopback callback on port 5000 | PKCE, DCR, CIMD (docs). Opens browser. Paste flow: Unknown | `mcporter auth`. Refresh on 401. Other details: Unknown |
| Credential storage | OS keychain (`@napi-rs/keyring`). Auto fallback to plaintext `~/.mcp-inspector/secrets.json` (0600) with no keychain. `headers` stay in plain `mcp.json` | OS keychain. Auto fallback to `~/.mcpc/credentials.json` (0600) on headless systems | Docker Desktop secrets, or Docker credential helpers (`osxkeychain`, `secretservice`, `pass`) | Library default: memory. CLI: Unknown | Plain `~/.mcporter/credentials.json` |
| Proxy | `HTTPS_PROXY`, `HTTP_PROXY`, `NO_PROXY`, both cases, through undici `EnvHttpProxyAgent`. Covers OAuth | Same variables, undici `EnvHttpProxyAgent` | Unknown | Unknown | Unknown |
| Root certificates | Node bundled roots. `NODE_EXTRA_CA_CERTS` adds roots | Node bundled roots. `--insecure` flag exists | Unknown (Go uses the system store by default) | Unknown | Unknown |
| stdio env | Unknown | Minimal whitelist. Proxy and CA variables are not passed unless set in `env` | Container env | Unknown | Unknown |
| Output | `--format text` (default) or `json`. One envelope `{result}`. Errors: one JSON line on stderr. Not TTY-aware | `--json` or `MCPC_JSON=1`. Not TTY-aware | `tools ls --format=json`. `tools call` prints text and "Tool call took:" to stdout | `--json` | Unknown |
| Exit codes | 0 ok, 1 usage/other, 3 auth, 4 unreachable, 5 tool error, 2/6–9 feature checks | 0 ok, 1 client, 2 server/tool, 3 network, 4 auth | Unknown | Unknown | Unknown |
| Daemon | No. Connect, run one method, disconnect | Yes. One bridge process per session on a Unix socket in `~/.mcpc/bridges/`. Optional TCP proxy (`--proxy`, 127.0.0.1 default) | No for the CLI path. The gateway can serve TCP | No | Yes, optional keep-alive daemon |
| Config | `~/.mcp-inspector/mcp.json` (`mcpServers` shape plus Inspector fields), `client.json`, `oauth.json` | `~/.mcpc/` (`sessions.json`, `profiles.json`). Reads `mcpServers` files. Auto-discovery ignores `${VAR}` in cwd configs | Local database plus `~/.docker/config.json` | Reads Claude, Cursor, Gemini, Goose configs and `./mcp.json` | `config/mcporter.json` or XDG path, JSONC. Imports editor configs |
| Distribution | npm (`npx`), Docker image | npm, Bun, Homebrew (bundles Node) | Docker Desktop, or build the CLI plugin | PyPI | npm, Homebrew, Bun-compiled macOS binaries with checksums and notarization |
| Other network calls | No telemetry dependency in root `package.json`. `npx` downloads on each run | No telemetry module in `src/` (by file name). x402 wallet code: Unknown | OpenTelemetry metrics to the Docker Desktop analytics socket, exported to Docker | Unknown | OpenClaw extension relay discovery for Chrome DevTools only |

## 4. MUST requirements

Has / Fork fix / Structural gap / Unknown. One result for each candidate.

| ID | Inspector CLI | mcpc | docker mcp | fastmcp CLI | mcporter |
|---|---|---|---|---|---|
| P1 modern | Has | Has | Fork fix (SDK bump) | Has | Has |
| P2 legacy, detect, keep | Fork fix (`auto` not default, result not kept) | Has | Fork fix | Unknown | Unknown |
| P4 stdio + HTTP | Has | Has | Structural gap | Has | Has |
| P6 tools | Has | Has | Has | Has | Has |
| P8 `input_required` | Unknown (CLI) | Unknown (elicitation "planned") | Fork fix | Unknown | Has (README) |
| P10 cancel on Ctrl-C | Unknown | Unknown | Unknown | Unknown | Unknown |
| A1 PKCE | Has | Has | Has | Has | Unknown |
| A2 discovery | Has (OIDC: Unknown) | Has (OIDC: Unknown) | Unknown | Unknown | Unknown |
| A3 RFC 8707 | Unknown | Has | Unknown | Unknown | Unknown |
| A4 RFC 9207 `iss` | Unknown | Has | Unknown | Unknown | Unknown |
| A5 credentials per issuer | Unknown | Unknown | Unknown | Unknown | Unknown |
| A8 refresh | Has | Has | Unknown | Unknown | Has |
| A10 no-browser login | Fork fix | Has | Unknown | Unknown | Unknown |
| A11 static headers | Has | Has | Has | Has | Has |
| S1 OS keychain | Fork fix (headers in file) | Has | Has | Unknown | Fork fix |
| S2 no plain file by default | Fork fix | Fork fix | Unknown | Unknown | Fork fix |
| S3 no secret in args | Fork fix (`--client-secret`, `--header`) | Fork fix (`--client-secret`, `--header`) | Unknown | Unknown | Unknown |
| S4 stdio env from store | Has | Has | Has | Unknown | Unknown |
| S6 daemon on Unix socket only | Has (no daemon) | Fork fix (remove `--proxy`; socket mode Unknown) | Has (CLI path) | Has (no daemon) | Unknown |
| N1 no other network | Unknown | Unknown | Fork fix (telemetry) | Unknown | Unknown |
| N2 proxy variables | Has | Has | Unknown | Unknown | Unknown |
| N3 system roots | Fork fix (Node system CA option) | Fork fix (same) | Unknown | Unknown | Unknown |
| N5 proxy to stdio servers | Unknown | Fork fix (whitelist) | Unknown | Unknown | Unknown |
| I1 TTY / JSON | Fork fix | Fork fix | Fork fix | Fork fix | Unknown |
| I2 stable JSON, stdout/stderr | Has | Has | Fork fix | Unknown | Unknown |
| I3 exit codes | Fork fix (no input-required code) | Fork fix (tool and server error share code 2) | Unknown | Unknown | Unknown |
| I4 JSON, `key=value`, stdin | Fork fix (stdin: Unknown) | Has | Fork fix | Fork fix | Unknown |
| I6 debug traffic, secrets hidden | Unknown | Unknown | Unknown | Unknown | Unknown |
| F1 Rust or Go | Structural gap | Structural gap | Has | Structural gap | Structural gap |
| F2 timings | Fork fix | Fork fix | Fork fix | Fork fix | Fork fix |
| D1 one file, macOS + Linux | Structural gap | Structural gap | Structural gap (needs Docker) | Structural gap | Fork fix (no Linux binary) |
| D2 no self-update | Unknown | Unknown | Has | Unknown | Has |
| C1 established | Has | Has (main author `jancurn`, 713 commits) | Has | Has | Partial (one author has 864 commits) |
| C2 follows spec | Has | Has | Fork fix (SDK 1.4.1) | Has | Has |
| C3 license | Has | Has | Has | Has | Has |

Notes:

- F1 alone removes every Node and Python candidate. mcporter's Bun binary
  embeds a JavaScript runtime, so F1 also applies.
- Docker `tools call` starts `docker mcp gateway run` and talks to it over
  stdio. Servers run in containers. The tool is not a direct client for a
  local command or a remote URL with user OAuth.
- Inspector and mcpc both write secrets to a 0600 file with no opt-in when
  no keychain is available. Both conflict with S2.

## 5. Ideas worth taking

- **Error envelope (Inspector).** On each non-zero exit, write one JSON line to
  stderr: `{"error":{"code","message","status","url"}}`. The `code` is
  stable. Agents branch on it.
- **Agent-safe auth (Inspector).** `--stored-auth-only` never starts a login.
  With no TTY on stdin or stderr, fail at once with `auth_required`. Do not
  wait for a callback.
- **Era per server (Inspector).** Config field `protocolEra`: `legacy`,
  `auto`, `modern`. Note: Inspector defaults to `legacy` because a
  `server/discover` probe stalls on silent legacy stdio servers. Our P2 design
  needs a timeout for that probe and must keep the result.
- **Logout revokes (Inspector).** Send RFC 7009 revocation, best effort, then
  delete local state.
- **Argument syntax (mcpc).** `key:=json` for typed values, `key=value` for
  strings, inline JSON when the first argument starts with `{`, JSON from
  stdin when piped.
- **Paste fallback (mcpc).** Loopback callback first. If no browser, accept the
  pasted redirect URL. This matches A10.
- **Registration order (mcpc).** Pre-registered, then CIMD, then DCR, with
  `--client-metadata-url` to override and a flag to force DCR. This matches
  A6 and A7.
- **Untrusted project config (mcpc).** Auto-discovered config in the current
  directory does not expand `${VAR}` into headers. This supports S5.
- **Schema lint (Inspector).** `--strict` reports tool schemas that clients
  reject. It is a cheap add-on to I5.
- **Root certificates.** Node clients miss company roots unless the user sets
  `NODE_EXTRA_CA_CERTS`. A Rust build needs a platform verifier (for example
  `rustls-platform-verifier`) to meet N3 with no setting.

## 6. Build path and test tools

- **Go SDK.** v1.7.0 (2026-07-28) added full `2026-07-28` support: stateless
  model, `server/discover`, MRTR. v1.8.0 is current. Examples:
  `examples/client/listfeatures`, `loadtest`, `middleware`. No CLI.
- **Rust SDK.** `rmcp` 3.0.0 (2026-07-28) added `2026-07-28`: sessionless
  Streamable HTTP, SEP-2549 TTL cache, Tasks extension, `subscriptions/listen`.
  3.5.1 is current. 3.0.0-beta.3 rejects a missing AS `issuer` by default.
  Examples in `examples/clients/src/`. A conformance client is at
  `conformance/src/bin/client.rs`.
- **Conformance suite.** `@modelcontextprotocol/conformance client --command
  "<cli>" --suite all|core|auth|backcompat --spec-version 2026-07-28`. It
  starts a test server, runs the client against it, and checks each JSON-RPC
  message against the spec schema. The reference client is
  `examples/clients/typescript/everything-client.ts`. Use it as an
  acceptance test for a new CLI (run it only in the sandbox from §5 of the
  baseline).

## Sources

- Inspector: [README](https://github.com/modelcontextprotocol/inspector/blob/main/README.md), [CLI README](https://github.com/modelcontextprotocol/inspector/blob/main/clients/cli/README.md), [secret storage](https://github.com/modelcontextprotocol/inspector/blob/main/docs/secret-storage.md), [environment variables](https://github.com/modelcontextprotocol/inspector/blob/main/docs/environment-variables.md), [server configuration](https://github.com/modelcontextprotocol/inspector/blob/main/docs/mcp-server-configuration.md), [roadmap 2026 H2](https://github.com/modelcontextprotocol/inspector/blob/main/docs/inspector-roadmap-2026-h2.md), [package.json](https://github.com/modelcontextprotocol/inspector/blob/main/package.json)
- MCP docs: [Inspector](https://modelcontextprotocol.io/docs/tools/inspector), [protocol eras](https://modelcontextprotocol.io/docs/2026-07-28/tools/inspector/protocol-eras), [authorization](https://modelcontextprotocol.io/docs/2026-07-28/tools/inspector/authorization)
- mcpc: [README](https://github.com/apify/mcpc/blob/main/README.md), [src/lib/auth/oauth-flow.ts](https://github.com/apify/mcpc/blob/main/src/lib/auth/oauth-flow.ts), [src/lib/proxy.ts](https://github.com/apify/mcpc/blob/main/src/lib/proxy.ts), [package.json](https://github.com/apify/mcpc/blob/main/package.json)
- Docker: [README](https://github.com/docker/mcp-gateway/blob/main/README.md), [cmd/docker-mcp/tools/call.go](https://github.com/docker/mcp-gateway/blob/main/cmd/docker-mcp/tools/call.go), [cmd/docker-mcp/tools/start.go](https://github.com/docker/mcp-gateway/blob/main/cmd/docker-mcp/tools/start.go), [telemetry](https://github.com/docker/mcp-gateway/blob/main/docs/telemetry/README.md), [OAuth CE mode](https://github.com/docker/mcp-gateway/blob/main/docs/oauth-ce-mode.md), [OAuth flows](https://github.com/docker/mcp-gateway/blob/main/docs/oauth-flows.md), [go.mod](https://github.com/docker/mcp-gateway/blob/main/go.mod)
- FastMCP: [CLI client](https://gofastmcp.com/cli/client.md), [client OAuth](https://gofastmcp.com/clients/auth/oauth.md), [v4 notes](https://gofastmcp.com/development/v4-notes)
- mcporter: [README](https://github.com/openclaw/mcporter/blob/main/README.md), [install](https://github.com/openclaw/mcporter/blob/main/docs/install.md), [config](https://github.com/openclaw/mcporter/blob/main/docs/config.md)
- IBM mcp-cli: [README](https://github.com/IBM/mcp-cli/blob/main/README.md)
- SDKs: [go-sdk releases](https://github.com/modelcontextprotocol/go-sdk/releases), [rust-sdk releases](https://github.com/modelcontextprotocol/rust-sdk/releases)
- Conformance: [README](https://github.com/modelcontextprotocol/conformance/blob/main/README.md)
- Excluded: [gemini-cli mcp commands](https://github.com/google-gemini/gemini-cli/tree/main/packages/cli/src/commands/mcp), [goose CLI commands](https://github.com/aaif-goose/goose/tree/main/crates/goose-cli/src/commands), [vercel mcp](https://vercel.com/docs/cli/mcp)
