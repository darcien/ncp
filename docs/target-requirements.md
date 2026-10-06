# ncp: requirements

Status: baseline, 2026-10-07. Spec: MCP [2026-07-28][ver]. License:
`MIT OR Apache-2.0`. No release yet: during development, names, paths and
stored credentials can change and reset.

## 1. Purpose

`ncp` is a command-line MCP client for one user on macOS and Linux. People and
AI agents use the same CLI.

## 2. Stack

| ID | Level | Requirement | Ref |
|---|---|---|---|
| F1 | MUST | Rust, with the official SDK `rmcp`. No JavaScript or Python runtime. | [rmcp] |

Levels use [BCP 14][bcp14] key words, as the MCP spec does.

## 3. Requirements

### 3.1 Protocol

| ID | Level | Requirement | Ref |
|---|---|---|---|
| P1 | MUST | Modern protocol `2026-07-28` (stateless, version in each request). | [ver], [chg] |
| P2 | MUST | Legacy protocol `2025-11-25` (`initialize` handshake). Detect the era of each server as the spec describes, and keep the result. | [compat] |
| P3 | SHOULD | Older legacy versions (`2025-06-18`, `2025-03-26`), only when a real server needs them. | [compat] |
| P4 | MUST | Transports: stdio and Streamable HTTP. | [stdio], [http] |
| P5 | SHOULD | HTTP+SSE transport (deprecated). Not in the MVP. Add it when a necessary server has only this transport. | [dep] |
| P6 | MUST | Tools: list and call. | [tools] |
| P7 | SHOULD | Resources: list, read, templates. Prompts: list, get. | [res], [prompts] |
| P8 | MUST | Process `input_required` results. At minimum, stop with a clear error. | [mrtr] |
| P9 | SHOULD | Elicitation, form mode and URL mode, on a TTY. With no TTY, stop with a defined exit code. | [elicit] |
| P10 | MUST | Send a cancellation when the user interrupts a request (Ctrl-C). | [cancel] |
| P11 | SHOULD | Show progress notifications. | [progress] |
| P12 | SHOULD | Cache list results with the `ttlMs` and `cacheScope` hints. | [chg] |
| P13 | MAY | Tasks extension. `subscriptions/listen`. | [ext], [chg] |
| P14 | SHOULD NOT | Roots, Sampling, Logging (deprecated in `2026-07-28`). | [dep] |

### 3.2 Authorization (HTTP servers)

The user logs in as themself. `ncp` is an OAuth 2.1 client as the MCP spec
describes.

| ID | Level | Requirement | Ref |
|---|---|---|---|
| A1 | MUST | Authorization code flow with PKCE. | [auth] |
| A2 | MUST | Discovery: protected resource metadata (RFC 9728), and both RFC 8414 and OpenID Connect Discovery. | [auth] |
| A3 | MUST | `resource` parameter (RFC 8707) in each authorization request and token request. | [auth] |
| A4 | MUST | Validate `iss` in the authorization response (RFC 9207). | [auth] |
| A5 | MUST | Keep client credentials for each issuer. Register again when the authorization server changes. | [reg] |
| A6 | SHOULD | Client registration in the spec order: pre-registered client, then Client ID Metadata Document (CIMD), then Dynamic Client Registration (DCR). | [reg] |
| A7 | SHOULD | The CIMD `client_id` URL is configurable. CIMD needs a metadata document at a stable HTTPS URL. | [reg] |
| A8 | MUST | Refresh tokens with no user action. | [auth] |
| A9 | SHOULD | Step-up after `403 insufficient_scope`: request the old and the new scopes, with a retry limit. | [auth] |
| A10 | MUST | Login with no local browser: `ncp` shows the authorization URL and accepts the pasted redirect URL. | [auth] |
| A11 | MUST | Static header credentials (bearer token, API key) for servers with no OAuth. | — |
| A12 | SHOULD | Commands for login status and logout. | — |

Not in scope: [Enterprise-Managed Authorization][extauth], Client Credentials
extension.

### 3.3 Credentials and local security

| ID | Level | Requirement | Ref |
|---|---|---|---|
| S1 | MUST | Keep all secrets in the OS credential store: Keychain on macOS, Secret Service on Linux. Secrets are access tokens, refresh tokens, client secrets and static tokens. | [auth] |
| S2 | MUST | No secret in a plain file by default. A `0600` file is permitted only after an explicit opt-in. With no credential store (for example Linux with no Secret Service), stop with an error that names this opt-in. | — |
| S3 | MUST | No secret in process arguments or in logs. Debug output hides secrets. | [sec] |
| S4 | MUST | stdio servers get credentials through environment variables. `ncp` reads these values from the credential store. | [auth] |
| S5 | SHOULD | A configuration file in a project directory starts stdio commands only after user approval. | — |
| S6 | MUST | If a daemon exists, it listens only on a Unix socket that only the user can open. No network listener. | — |
| S7 | MUST | On macOS, use the native Keychain API. Put the credential store behind one internal interface, so that a change of storage model or signing method changes only that module. | — |

### 3.4 Network

| ID | Level | Requirement | Ref |
|---|---|---|---|
| N1 | MUST | Network access only to the configured MCP servers and their authorization servers. No telemetry, no download, no self-update. | — |
| N2 | MUST | Proxy variables `HTTPS_PROXY`, `HTTP_PROXY`, `NO_PROXY` (upper and lower case), with credentials in the proxy URL. This applies to MCP and OAuth requests. | — |
| N3 | MUST | System root certificates, including roots that the company adds, with no extra setting. | — |
| N4 | SHOULD | Optional CA file setting (`SSL_CERT_FILE`) for systems with no usable system store. | — |
| N5 | MUST | stdio servers receive the proxy variables. | — |

Not in scope: PAC files, and NTLM or Kerberos proxy authentication. Such
networks need a local helper proxy.

### 3.5 Interface

The model is modern Unix tools (for example `rg`, `fd`, `jq`, `gh`): good
defaults, fast, and usable in scripts.

| ID | Level | Requirement | Ref |
|---|---|---|---|
| I1 | MUST | Output for people on a TTY. JSON output when stdout is not a TTY, or with `--json`. | — |
| I2 | MUST | The JSON output has a stable, documented schema. Results go to stdout, diagnostics go to stderr. | — |
| I3 | MUST | Defined exit codes: success, tool error (`isError`), protocol or transport error, authorization error, usage error, input required. | [tools] |
| I4 | MUST | Tool arguments as JSON, as `key=value` pairs, or from stdin. | — |
| I5 | SHOULD | Show the input schema of a tool. Validate arguments against the schema before the call. | [tools] |
| I6 | MUST | Debug mode shows JSON-RPC and HTTP traffic, with secrets hidden. | — |
| I7 | SHOULD | `NO_COLOR` support. Shell completions. | — |
| I8 | MUST | MVP configuration: the common `mcpServers` JSON core (see below). | [cfg] |

The MVP configuration file uses only the field set that most agent tools
share. Thus other tools can read it, and `ncp` can change format later with
no loss.

```json
{
  "mcpServers": {
    "local": { "type": "stdio", "command": "srv", "args": [], "env": {}, "cwd": "." },
    "remote": { "type": "http", "url": "https://example.com/mcp", "headers": {} }
  }
}
```

- Path: `$XDG_CONFIG_HOME/ncp/mcp.json` (default `~/.config/ncp/mcp.json`)
  on macOS and Linux. User scope only in the MVP. Project scope comes with S5.
- `type` values: `stdio`, `http`, `sse`. A `url` entry needs `type`, because
  tools give a different meaning to a `url` with no `type`.
- No secret in the file. `ncp` keeps secret `env` values and secret `headers`
  in the credential store, for each server and name.
- `${VAR}` in a value expands from the environment. No other variable syntax,
  and no command expansion.
- `ncp` ignores unknown keys, with a warning, so files from other tools load.

### 3.6 Performance

| ID | Level | Requirement | Ref |
|---|---|---|---|
| F2 | MUST | Built-in timings that show CPU time and network time separately. | — |
| F5 | SHOULD | Timings for each phase: process start, configuration load, credential read, stdio server start, DNS, TCP, proxy `CONNECT`, TLS, server response. | — |
| F3 | SHOULD | `ncp` process start in a few milliseconds, with no network access. | — |
| F4 | MAY | Local daemon that keeps stdio servers, connections and tokens between calls. | — |

### 3.7 Distribution

| ID | Level | Requirement | Ref |
|---|---|---|---|
| D1 | MUST | macOS and Linux, x86_64 and arm64. One file to install, with almost no dependencies. | — |
| D2 | MUST | Updates come only from a separate distribution channel. No self-update in the MVP. | — |
| D3 | SHOULD | Releases have checksums or signatures. | — |

### 3.8 Not in scope

Windows. Registry search. Proxy or server mode for other users. Multi-user
access control. OAuth authorization server. Audit log. Kubernetes or Docker
deployment.

## 4. Test method

- Clean environment (`env -i`) and an empty, separate `HOME`.
- Test servers are local only: stubs, rmcp fixtures, and official MCP test
  parts (conformance suite, reference servers). No call to a third-party
  server until the local tests pass. See [test harness][harness].
- A sandbox blocks all outbound connections except to localhost:
  `sandbox-exec` on macOS, a network namespace on Linux. A local proxy logs
  each `CONNECT`. A direct connection fails and is visible.

## 5. Open items

None of these items blocks development.

1. Do the target networks use PAC files, or NTLM or Kerberos proxy
   authentication?
2. Tests: company root certificate on macOS (needs approval), and Linux
   (`SSL_CERT_FILE`, system store, Secret Service).
3. macOS release signing. The goal is free and secure, with no Keychain
   prompt after an update. Decide before the first public macOS release.
   Until then, local builds accept the prompt, or use the opt-in file store
   (S2).
   - Self-signed certificate: free. A test on a real Mac must show that the
     Keychain partition list accepts a rebuild with no prompt.
   - Ad-hoc signature: free. A prompt comes once for each version, and the
     prompt blocks agents until a person accepts it.
   - Developer ID: US$99 per year. Known to work. Not preferred: use it only
     if the two free options fail.

Research and history: `research/`. The candidate evaluation is in
`research/candidate-evaluation.md`.

[bcp14]: https://datatracker.ietf.org/doc/html/bcp14
[ver]: https://modelcontextprotocol.io/specification/versioning
[compat]: https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning
[chg]: https://modelcontextprotocol.io/specification/2026-07-28/changelog
[dep]: https://modelcontextprotocol.io/specification/2026-07-28/deprecated
[stdio]: https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio
[http]: https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http
[tools]: https://modelcontextprotocol.io/specification/2026-07-28/server/tools
[res]: https://modelcontextprotocol.io/specification/2026-07-28/server/resources
[prompts]: https://modelcontextprotocol.io/specification/2026-07-28/server/prompts
[mrtr]: https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/mrtr
[elicit]: https://modelcontextprotocol.io/specification/2026-07-28/client/elicitation
[cancel]: https://modelcontextprotocol.io/specification/2026-07-28/basic/utilities/cancellation
[progress]: https://modelcontextprotocol.io/specification/2026-07-28/basic/utilities/progress
[ext]: https://modelcontextprotocol.io/extensions/overview
[auth]: https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization
[reg]: https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/client-registration
[sec]: https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations
[extauth]: https://github.com/modelcontextprotocol/ext-auth
[rmcp]: https://github.com/modelcontextprotocol/rust-sdk
[cfg]: research/agent-config-formats.md
[harness]: research/test-harness.md
