# Design proposal: command line

Status: proposal, 2026-10-07. Waits for acceptance (plan milestone 5, gate).
Baseline: [target-requirements.md](../target-requirements.md). Input:
[interface-and-config.md](../research/interface-and-config.md),
[early-tests.md](../research/early-tests.md).

This proposal covers the items of the gate: commands, names, exit codes, JSON
output, the OAuth redirect, and file locations. Section 7 lists the decisions
that need the user. The client core (milestone 4) already supports the
behavior below, except OAuth, timings and argument parsing.

## 1. Principles

- One CLI for people and agents. The same command gives text on a TTY and
  JSON in a pipe (I1).
- Results go to stdout. Diagnostics, warnings and timings go to stderr (I2).
- No command waits for input that it did not ask for. A command reads stdin
  only when an argument says so (`-`). Thus an agent cannot hang on stdin.
- No command opens a browser or starts OAuth, except `login`.
- MCP objects pass through with no change: `Tool`, `CallToolResult`,
  `InputRequiredResult`. `ncp` adds only a small envelope where it must.

## 2. Commands

```
ncp [OPTIONS] servers
ncp [OPTIONS] tools <server>
ncp [OPTIONS] tool <server>.<tool>
ncp [OPTIONS] call <server>.<tool> [ARGUMENT...]
ncp [OPTIONS] login <server> [--no-browser]
ncp [OPTIONS] logout <server>
ncp [OPTIONS] auth status [<server>]
ncp [OPTIONS] secret set <server> (env|header) <NAME>
ncp [OPTIONS] secret rm <server> (env|header) <NAME>
ncp [OPTIONS] secret list <server>
ncp completions <bash|zsh|fish>
```

| Command | Network | Result |
|---|---|---|
| `servers` | No | Each configured server: name, transport, target. Invalid entries with their errors. Configuration warnings on stderr. |
| `tools <server>` | Yes | The tools of the server: name, title, description and annotations. Text output is one line for each tool. |
| `tool <server>.<tool>` | Yes | One tool with its input schema and output schema. |
| `call <server>.<tool>` | Yes | The tool result. |
| `login <server>` | Yes | Runs the OAuth flow, keeps the tokens (section 5). |
| `logout <server>` | No | Removes the OAuth tokens of the server. The client credentials stay, so the next login needs no new registration. `--all` also removes them. |
| `auth status` | No | For each server: OAuth tokens present, expiry, scopes, issuer. No token value. |
| `secret set` | No | Reads one value from stdin and keeps it in the credential store. On a TTY the input is not shown. The value never goes in an argument (S3). |
| `secret rm`, `secret list` | No | `list` shows names only. |

Resources and prompts (P7) come after the MVP as `resources`, `resource`,
`prompts` and `prompt`, in the same shape.

### 2.1 Names

- A server name has only ASCII letters, digits, `-` and `_` (already in the
  requirements).
- `<server>.<tool>` splits at the first `.`. A tool name can contain `.` and
  `/` (SEP-986), and a server name cannot. Thus `github.admin.tools.list`
  gives the server `github` and the tool `admin.tools.list`.
- An unknown server or tool name gives a usage error with the near names, for
  example "no tool `search_issue`; did you mean `search_issues`?".

### 2.2 Tool arguments (I4)

| Form | Meaning |
|---|---|
| `key=value` | The value is text. When the input schema gives the property type `number`, `integer`, `boolean`, `array` or `object`, `ncp` reads the value as JSON. A schema type `string` keeps the text, also for `123`. |
| `key:=json` | The value is JSON, with no schema rule. |
| `key=@path` | The value is the text of the file. `@@` at the start is a literal `@`. |
| `{...}` as the only argument | The arguments object as JSON. |
| `-` as the only argument | Read the arguments object as JSON from stdin. |

A repeated key is a usage error. With an input schema, `ncp` checks the
arguments before the call and gives a usage error with the property path
(I5). `--no-validate` turns the check off.

### 2.3 Global options

| Option | Environment | Meaning |
|---|---|---|
| `--json` / `--text` | — | Force JSON or text output. Default: text on a TTY, else JSON. |
| `--config <path>` | `NCP_CONFIG` | Configuration file. Default in section 6. |
| `--debug` | `NCP_DEBUG=1` | JSON-RPC messages and HTTP requests on stderr. Secrets are hidden (I6). |
| `--timings` | — | Timings on stderr (F2): process start, configuration, credential read, server start, DNS, connect, server response, CPU time. |
| `--probe-timeout <ms>` | `NCP_PROBE_TIMEOUT` | Wait for the answer to `server/discover`. Default 10 000 ms (decision D3). |
| `--timeout <ms>` | `NCP_TIMEOUT` | Wait for the result of one request. Default none. A timeout sends a cancel request. |
| — | `NO_COLOR` | No color (I7). |
| — | `NCP_CREDENTIAL_STORE=file` | The opt-in file store (decision D2). |

## 3. Exit codes (I3)

| Code | Class | When |
|---|---|---|
| 0 | Success | The command completed. For `call`: `isError` is absent or false. |
| 1 | Tool error | `call` got a result with `isError: true`. The result is on stdout. |
| 2 | Usage error | Bad arguments, unknown server or tool, invalid configuration file or entry, arguments that fail the input schema. |
| 3 | Protocol or transport error | The server cannot start, the connection fails, a handshake fails, a JSON-RPC error, a timeout. |
| 4 | Authorization error | The server needs a login (`401`), a scope is missing after the retry limit (`403`), the credential store is not available or is locked. |
| 5 | Input required | `call` got an `input_required` result (P8). The result is on stdout, so that a later version or the caller can continue. |
| 130 | Interrupted | Ctrl-C. `ncp` sent the cancel request first (P10). 130 is 128 + SIGINT, as shells use. |

Elicitation on a TTY (P9) can come later. With no TTY, an elicitation request
gives code 5.

## 4. JSON output (I2)

Rules:

- One JSON value on stdout for each command. Pretty on a TTY with `--json`,
  compact in a pipe.
- MCP objects are copied with no change. Their schema is the MCP schema of
  the negotiated protocol version.
- `ncp` objects have stable names. A new field can come in a minor version.
  The removal or change of a field needs a major version.
- In JSON mode, an error is one JSON object on stderr. stdout is then empty,
  except for codes 1 and 5.

| Command | stdout |
|---|---|
| `servers` | `{"servers": [Server], "invalid": [{"name", "error"}]}` |
| `tools` | `{"server": "<name>", "era": Era, "tools": [MCP Tool]}` |
| `tool` | `{"server": "<name>", "tool": MCP Tool}` |
| `call` | MCP `CallToolResult`, or MCP `InputRequiredResult` (code 5) |
| `auth status` | `{"servers": [{"name", "logged_in", "issuer", "scopes", "expires_at"}]}` |
| `secret list` | `{"server": "<name>", "env": ["NAME"], "headers": ["name"]}` |

```json
{"name": "github", "transport": "http", "target": "https://api.example.com/mcp"}
{"era": "modern", "version": "2026-07-28"}
{"era": "legacy", "version": "2025-11-25", "cause": "server_error"}
```

Error object on stderr:

```json
{"error": {"code": "auth", "exit": 4, "server": "github",
  "message": "the server needs a login",
  "hint": "ncp login github"}}
```

`code` values: `tool`, `usage`, `protocol`, `auth`, `input_required`,
`interrupted`. Each value has one exit code (section 3).

Timings (`--timings`) on stderr, in JSON mode:

```json
{"timings": {"total_ms": 412, "cpu_ms": 18, "config_ms": 1, "credentials_ms": 3,
  "server_start_ms": 0, "dns_ms": 2, "connect_ms": 41, "response_ms": [180, 95]}}
```

`connect_ms` includes TCP, the proxy `CONNECT` and TLS. reqwest has no hook
for each part (early tests, section 5). The separate parts (F5, SHOULD) need
an own connector on hyper-util, after the MVP.

## 5. OAuth redirect

Proposal: a **random loopback port**, with an optional fixed port for each
server.

- Redirect URI: `http://127.0.0.1:<port>/callback`. RFC 8252 §7.3: the
  authorization server "MUST allow any port to be specified at the time of
  the request for loopback IP redirect URIs". RFC 8252 §8.3: use the loopback
  IP literal, not `localhost`.
- A random port cannot conflict with another program, and needs no setting.
- Some authorization servers compare the port exactly, mostly with a
  pre-registered client. For them, a fixed port is a setting of the server
  (decision D4 gives the place).
- A Client ID Metadata Document lists the redirect URIs. It lists
  `http://127.0.0.1/callback`. The port does not count for a loopback URI
  (RFC 8252 §7.3).
- With no local browser (A10), `login --no-browser` prints the URL and reads
  the redirect URL from stdin. No listener is necessary: the browser shows a
  connection error, and the user copies the URL from the address bar. On a
  TTY with a browser, `login` opens the browser and also accepts a pasted
  URL, whichever comes first.
- `ncp` checks `state` and `iss` (RFC 9207, A4) on both paths.

## 6. File locations

`ncp` uses the XDG base directories on macOS and on Linux. The configuration
path in the requirements already does this.

| File | Path | Content | Mode |
|---|---|---|---|
| Configuration | `$XDG_CONFIG_HOME/ncp/mcp.json` (`~/.config/ncp/mcp.json`) | `mcpServers` | user choice |
| Era cache | `$XDG_CACHE_HOME/ncp/eras.json` (`~/.cache/ncp/eras.json`) | Era for each server and target | 0600 |
| List cache (P12, later) | `$XDG_CACHE_HOME/ncp/lists/<server>.json` | Tool lists with `ttlMs` | 0600 |
| Secret index (D1) | `$XDG_STATE_HOME/ncp/secrets.json` (`~/.local/state/ncp/secrets.json`) | Names of stored secrets for each server. No value. | 0600 |
| Opt-in credential file (D2) | `$XDG_STATE_HOME/ncp/credentials.json` | Secrets | 0600, in a 0700 directory |
| Daemon socket (F4, later) | `$XDG_RUNTIME_DIR/ncp/daemon.sock`, else `$TMPDIR/ncp-<uid>/daemon.sock` | — | 0600, in a 0700 directory (S6) |

The era cache and the list cache are caches: `ncp` works the same when they
are removed. A cache file that does not load is ignored and written again.

## 7. Decisions for the user

| ID | Decision | Proposal | Alternative |
|---|---|---|---|
| D1 | How `ncp` knows the names of stored secrets. `keyring-core` can search both stores, but a Secret Service search asks to unlock locked items, and a Keychain search prompt is not known (docs/research/credential-store.md). | A secret index file with names only (section 6). `secret set` and `secret rm` change it. `mcp.json` stays the same as in other tools. | A reference in `mcp.json`, for example `"env": {"API_KEY": "${secret:API_KEY}"}`. Other tools then pass the literal text to the server. |
| D2 | The opt-in for the file store (S2). | Environment variable `NCP_CREDENTIAL_STORE=file`. The error with no native store names it. | A key in `mcp.json`. Other tools can reject unknown keys. |
| D3 | Default probe timeout. | 10 s, the rmcp value. The first start of a stdio server (for example a download by `npx`) counts against it. A timeout costs a second start. | 3 s, as go-sdk PR #1334. Faster for a legacy server that does not answer, but a slow first start falls back to `initialize`. |
| D4 | The place of settings that `mcp.json` cannot hold (fixed OAuth port, CIMD URL (A7), probe timeout for one server). | A second file, `$XDG_CONFIG_HOME/ncp/settings.json`, with a map for each server name. `mcp.json` stays portable. | An extension key in `mcp.json`, for example `"x-ncp": {...}`. |
| D5 | The proxy for a loopback URL. reqwest sends a loopback request to the proxy when `NO_PROXY` does not name loopback (early tests, section 4). | Never use a proxy for `localhost`, `127.0.0.0/8` and `::1`, as Go `net/http` does. A proxy cannot reach the loopback of the user. | Keep the reqwest behavior and document `NO_PROXY`. |
| D6 | The selector form. | `<server>.<tool>` only. | Also accept two words, `<server> <tool>`. |

## 8. Order after the gate

1. Command line: `servers`, `tools`, `tool`, `call`, exit codes, JSON and text
   output, argument parsing and schema check.
2. Ctrl-C with the cancel request, and `--timeout`.
3. `secret` commands and static headers (A11).
4. `--debug` and `--timings`.
5. OAuth: `login`, `logout`, `auth status`, refresh, step-up, the rmcp
   workarounds of [rmcp-open-points.md](../research/rmcp-open-points.md).
6. Conformance adapter for the official suite (test harness report).
7. Release build: musl targets (a musl C compiler for `ring`), checksums.
