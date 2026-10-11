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
| P2 | MUST | Legacy protocol `2025-11-25` (`initialize` handshake). Detect the protocol version of each server as the spec describes, and keep the result. | [compat] |
| P3 | SHOULD | Older legacy versions (`2025-06-18`, `2025-03-26`), only when a real server needs them. | [compat] |
| P4 | MUST | Transports: stdio and Streamable HTTP. | [stdio], [http] |
| P5 | SHOULD | HTTP+SSE transport (deprecated). Not in the MVP. Add it when a necessary server has only this transport. | [dep] |
| P6 | MUST | Tools: list and call. | [tools] |
| P7 | SHOULD | Resources: list, read, templates. Prompts: list, get. | [res], [prompts] |
| P8 | MUST | Process `input_required` results. At minimum, stop with a clear error. | [mrtr] |
| P9 | SHOULD | Elicitation, form mode and URL mode, on a TTY, and the error that asks the user to open a page (`URLElicitationRequiredError`). A URL opens only after the user agrees. With no TTY, stop with a defined exit code, and give the pages in the error. | [elicit] |
| P10 | MUST | Send a cancellation when the user interrupts a request (Ctrl-C). | [cancel] |
| P11 | SHOULD | Show progress notifications. | [progress] |
| P12 | SHOULD | Cache list results with the `ttlMs` and `cacheScope` hints. | [chg] |
| P13 | MAY | Tasks extension. `subscriptions/listen`. | [ext], [chg] |
| P14 | SHOULD NOT | Roots, Sampling, Logging (deprecated in `2026-07-28`). | [dep] |
| P15 | MUST | No request waits forever. Each request has a time limit (default 300 s, configurable). The whole connect (server start and handshake) has a limit of 30 s. A request timeout sends a cancel request; a connect timeout stops the server, because a client MUST NOT cancel `initialize`. Reason: an agent must not hang. | [timeouts] |

Protocol version cache (P2). The spec permits a client to keep the protocol
version across restarts of the same server configuration. `ncp` keeps the
version for each server name with the transport target (the stdio command,
arguments and directory, or the URL). A changed target makes the cached
version invalid. `ncp` probes with `server/discover` in its own timeout, and
connects again with `initialize` on a new transport when the server does not
know `server/discover` or does not answer in time. The cached version stays
until the target changes, also after a timeout. Thus a server that does not
answer `server/discover` costs the timeout once, not at each run. When the
handshake with the cached version fails, `ncp` probes again, because the
server can have changed.

### 3.2 Authorization (HTTP servers)

The user logs in as themself. `ncp` is an OAuth 2.1 client as the MCP spec
describes.

| ID | Level | Requirement | Ref |
|---|---|---|---|
| A1 | MUST | Authorization code flow with PKCE. | [auth] |
| A2 | MUST | Discovery: protected resource metadata (RFC 9728), and both RFC 8414 and OpenID Connect Discovery. | [auth] |
| A3 | MUST | `resource` parameter (RFC 8707) in each authorization request and token request. | [auth] |
| A4 | MUST | Validate `iss` in the authorization response (RFC 9207). | [auth] |
| A5 | MUST | Keep client credentials for each issuer. Register again when the authorization server changes. A client secret goes only to the issuer that the configuration names, so that a changed or hostile MCP server cannot get it. | [reg] |
| A13 | MUST | Each stored token set is bound to the server that it is for: the canonical URL of the MCP server (the RFC 8707 resource) and the issuer. Before `ncp` sends a stored token, and before a refresh, it compares the binding with the current entry and metadata. A different binding, or no binding, stops the command: `ncp` does not send the token, and the message says that the stop is on purpose. The user logs in again. Reason: a changed or hostile entry must not get a token that was issued for another server. | [auth] |
| A6 | SHOULD | Client registration in the spec order: pre-registered client, then Client ID Metadata Document (CIMD), then Dynamic Client Registration (DCR). | [reg] |
| A7 | SHOULD | `ncp` has a CIMD metadata document at a stable HTTPS URL, on the GitHub Pages site of the repository. The URL comes with the first release; until then, `ncp` uses no CIMD. A URL for each user comes only when a real user needs it. | [reg] |
| A8 | MUST | Refresh tokens with no user action. | [auth] |
| A9 | SHOULD | Step-up after `403 insufficient_scope`: request the old and the new scopes, with a retry limit. | [auth] |
| A10 | MUST | Login with no local browser: `ncp` shows the authorization URL and accepts the pasted redirect URL. | [auth] |
| A11 | MUST | Static header credentials (bearer token, API key) for servers with no OAuth. | — |
| A12 | SHOULD | Commands for login status and logout. | — |

Not in scope: [Enterprise-Managed Authorization][extauth], Client Credentials
extension.

### 3.3 Credentials and local security

`ncp` prevents accidental leaks of secrets. It does not protect against a
hostile process of the same user: such a process can read the same files and
ask the same credential store.

Accepted risks of project files (S5): an agent can run `ncp trust`, or
answer the question in a pseudo-terminal, so the guard is the permission
rules of the agent; after the trust, a project entry can send an
environment variable to its host, and the shown entries name each
`${NAME}` with its host; a network file system can give a false owner.

| ID | Level | Requirement | Ref |
|---|---|---|---|
| S1 | MUST | Keep each secret that `ncp` stores in the OS credential store: Keychain on macOS, Secret Service on Linux. Secrets are access tokens, refresh tokens, client secrets and static tokens. A value that the user wrote in the configuration file stays there, because no reliable test can tell a secret from another value. | [auth] |
| S2 | MUST | `ncp` writes no secret to a plain file by default. A `0600` file is permitted only after an explicit opt-in. With no credential store (for example Linux with no Secret Service), stop with an error that names this opt-in. | — |
| S3 | MUST | No secret in process arguments or in logs. Verbose output hides each value that `ncp` knows is a secret: values from `${NAME}` and the credential stores, credential headers of HTTP, and the tokens and credentials of OAuth. A literal value in the configuration shows as it is, because no reliable test can tell a secret from another value. | [sec] |
| S4 | MUST | stdio servers get credentials through environment variables. `ncp` reads these values from the credential store. | [auth] |
| S5 | MUST | `ncp` reads one project `.mcp.json`, found from the current folder up to the git root (not in the home folder), only if the user owns it. Before the user trusts it, the file has no effect: no process starts, no request goes out, and no store is read for its entries. The trust is one record for each path with the hash of the bytes, as the paranoid mode of mise; each change asks again. A command that names a server of an untrusted file asks with a TTY, and stops with exit 2 and the entries with no TTY. No flag and no environment variable give trust. After the trust, a project entry wins over a global entry of the same name. The rules are in docs/design/project-config.md. | — |
| S6 | MUST | If a daemon exists, it listens only on a Unix socket that only the user can open. No network listener. | — |
| S7 | MUST | On macOS, use the login Keychain through `/usr/bin/security`. Reason: `ncp` has no paid Developer ID; an ad-hoc signed binary changes identity at each update, and the native Keychain API then asks again for each item, which an agent cannot answer. `ncp` gives each command to `security -i` on standard input, because a value in the process arguments breaks S3. One input line holds at most 4095 bytes, and some token sets are longer (for example from Entra ID). Thus a value that does not fit one line is kept in several items. A save that fails before its end keeps the old value. Put the credential store behind one internal interface, so that a change of storage model or signing method changes only that module. | — |
| S8 | MUST | A credential store call that gives no answer in 60 s stops with an error. A locked store waits for its unlock prompt with no limit, and on a machine with no display nobody can answer the prompt. | — |
| S9 | MUST | `ncp` uses the destination of a stored static secret as part of its key. A header value goes only to the origin (scheme, host and port) of its entry, with no redirects (N6). A client secret goes only to the token endpoint of the configured issuer, and its key has the origin of this issuer. A value for a stdio server has no destination. When no item exists for the destination, `ncp` stops before the send, and sends nothing. Reason: two entries can name the same `${NAME}` by accident, or the user can change the URL of an entry. The value must not follow the name to another host. Values from the environment have no binding. | — |
| S10 | MUST | `ncp` treats a value as a secret only by fact: a value from `${NAME}` or a credential store, the value of a header field that RFC 9110 defines as credentials (`Authorization`, §11.6.2; `Proxy-Authorization`, §11.7.2), and OAuth tokens and client credentials. `ncp` never guesses from a value, a pattern or a list of names. Reason: a guess hides some secrets and not others, and the user cannot know which. A rule from the RFC is a fact that the user can check. | [sec] |

### 3.4 Network

| ID | Level | Requirement | Ref |
|---|---|---|---|
| N1 | MUST | Network access only to the configured MCP servers and their authorization servers. No telemetry, no download, no self-update. | — |
| N2 | MUST | Proxy variables `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY`, `NO_PROXY` (upper and lower case), with credentials in the proxy URL. When no variable is set, the manual HTTP and HTTPS proxy of the macOS system settings, as uv does. No exception for loopback hosts: `NO_PROXY` is the only exception list, as in curl and uv. This applies to MCP and OAuth requests. | — |
| N3 | MUST | System root certificates, including roots that the company adds, with no extra setting. | — |
| N4 | SHOULD | Optional CA file setting (`SSL_CERT_FILE`). When it is set, `ncp` trusts only the roots in the file, on macOS and on Linux, as uv does. The file must hold each root that the user needs, for example the public roots and the root of the company. A file that `ncp` cannot read, that has no certificate, or an empty value, stops with an error. Reason: one rule on each OS. On Linux the system store reads this variable in place of the system roots, and `ncp` cannot change its own environment. | — |
| N6 | MUST | The HTTP client of the MCP connection does not follow redirects. A `3xx` answer stops with an error that names the new location. Reason: the headers of an entry can hold credentials, and they go only to the URL of the entry. rmcp has the same default. | — |
| N5 | MUST | stdio servers receive the proxy variables. | — |

Not in scope: PAC files, and NTLM or Kerberos proxy authentication. Such
networks need a local helper proxy.

### 3.5 Interface

The model is modern Unix tools (for example `rg`, `fd`, `jq`, `gh`): good
defaults, fast, and usable in scripts.

| ID | Level | Requirement | Ref |
|---|---|---|---|
| I1 | MUST | Output for people on a TTY. JSON output when stdout is not a TTY. | — |
| I2 | MUST | The JSON output has a stable, documented schema. Results go to stdout, diagnostics go to stderr. | — |
| I3 | MUST | Defined exit codes: success, tool error (`isError`), protocol or transport error, authorization error, usage error, input required. | [tools] |
| I4 | MUST | Tool arguments as JSON, as `key=value` pairs, or from stdin. | — |
| I5 | SHOULD | Show the input schema of a tool. Validate arguments against the schema before the call. | [tools] |
| I6 | MUST | Verbose output (`-vv`) shows JSON-RPC and HTTP traffic, with the secrets of S3 hidden. | — |
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
  tools give a different meaning to a `url` with no `type`. An entry with
  `command` and no `type` is `stdio`, as in all tools that read this shape.
  `ncp` reads an `sse` entry, but it does not connect to it in the MVP (P5).
- Server name: 1 to 64 ASCII letters, digits, `-` and `_`. Reason: the name
  is a word in commands, and a part of credential keys. Without `.`, `:` and
  `/`, `ncp` can find a joined server and tool name (`github.search`) and show
  the correct command.
- Server URL: `https`, or `http` to a loopback host only (`localhost`,
  `127.0.0.0/8`, `::1`). No user name or password in the URL. Reason: tokens
  and headers must not go over the network in plain text, and the file must
  not hold a secret.
- `ncp` writes no secret to the file. A secret is a named value, and the
  file names it with `${NAME}`. `ncp` accepts a literal value that the user
  wrote, because it cannot tell a secret from another value, and the file
  can come from another tool.
- `${VAR}` and `${env:VAR}` in a value expand. No other variable syntax, and
  no command expansion. Expansion applies to `command`, `args`, `env` values,
  `cwd`, `url`, `headers` values and `oauth.clientSecret`, not to keys. The value comes from the
  environment, else from the credential store, else from the opt-in
  credential file. A value from the store or the file expands only in `env`
  values, `headers` values and `oauth.clientSecret`, because a value in
  `args` shows in the process list. A stored value goes only to its
  destination (S9). When an entry uses a stored value, its `url` and its
  `oauth.issuer` have no `${NAME}` in the scheme, host or port, because an
  environment value must not change the destination.
  A variable with no value is an error for that server. A `$` that does not
  start `${` stays as text.
- `oauth` is a known key of an `http` entry. The MVP reads `oauth.clientId`
  (a pre-registered client), `oauth.clientSecret` (its secret, a literal
  value or `${NAME}`, as in `headers`) and `oauth.issuer` (the issuer of the
  authorization server of the client). `clientSecret` needs `clientId` and
  `issuer`. When `issuer` is absent, `ncp` reads `authorizationServer` for
  compatibility with other tools. Other keys in `oauth` are unknown keys.
- `ncp` ignores unknown keys, with a warning, so files from other tools load.
  A key of the other transport (for example `headers` in a `stdio` entry) is
  also an unknown key.
- An invalid entry does not stop the other entries. `ncp` shows its error
  when the user selects that server.
- A `stdio` server gets the environment of `ncp` (with the proxy variables),
  then the expanded `env` of its entry. A later value replaces an earlier
  value with the same name.
- A `stdio` server starts in its own process group, so that Ctrl-C reaches
  only `ncp`, and `ncp` can send the cancel request. Thus `ncp` stops the
  server as the spec describes: close stdin, wait, SIGTERM, wait, SIGKILL.

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
3. Closed: macOS signing. With S7 (the Keychain through `/usr/bin/security`),
   an update gives no Keychain prompt, so `ncp` needs no paid Developer ID.

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
[timeouts]: https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle#timeouts
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
