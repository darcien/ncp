# Design: command line

Status: decisions made with the user on 2026-10-08 (section 7). Waits for the
final review and acceptance (plan milestone 5, gate).
Baseline: [target-requirements.md](../target-requirements.md). Input:
[interface-and-config.md](../research/interface-and-config.md),
[early-tests.md](../research/early-tests.md),
[credential-store.md](../research/credential-store.md).

This design covers the items of the gate: commands, names, exit codes, JSON
output, secrets, the OAuth redirect, and file locations. Section 7 records
the decisions and their reasons.

## 1. Principles

- One CLI for people and agents. The same command gives text on a TTY and
  JSON in a pipe (I1).
- Results go to stdout. Diagnostics, warnings, hints and timings go to stderr
  (I2).
- Secure and correct by default. When `ncp` cannot do the right thing, it
  stops with an error that says what failed and gives the exact command that
  fixes it.
- Names follow uv: full words, one name for each action, no aliases. Short
  forms: only `-h`, `-q`, `-v` and `-V` (an `ncp` rule; uv has some more). A
  choice is a flag named after the setting,
  with the values as words (`--output-format <text|json>`). A flag name does
  not state a risk; the help text and the output state it. No `NCP_*`
  environment variables for now.
- Prefer the MCP spec, then common practice, over our own inventions. The
  network commands use the verbs of the MCP methods (`list`, `call`, `read`,
  `get`).
- No command waits for input that it did not ask for. A command reads stdin
  only when an argument says so (`-`), when `secret set` reads the value, or
  when `auth login` reads a pasted redirect URL.
  Thus an agent cannot hang on stdin.
- No command opens a browser or starts OAuth, except `auth login`.
- MCP objects pass through with no change: `Tool`, `CallToolResult`,
  `InputRequiredResult`. `ncp` adds only a small envelope where it must.

## 2. Commands

```
ncp [OPTIONS] server list
ncp [OPTIONS] server show <SERVER>
ncp [OPTIONS] tool list <SERVER>
ncp [OPTIONS] tool show <SERVER> <TOOL>
ncp [OPTIONS] tool call <SERVER> <TOOL> [ARGUMENT...]
ncp [OPTIONS] auth login <SERVER> [--no-browser] [--credential-store <native|file>]
ncp [OPTIONS] auth logout <SERVER>
ncp [OPTIONS] auth status [<SERVER>]
ncp [OPTIONS] secret set <NAME> [--credential-store <native|file>]
ncp [OPTIONS] secret list
ncp [OPTIONS] secret delete <NAME>
ncp [OPTIONS] cache clean
ncp generate-shell-completion <bash|zsh|fish>
```

| Command | Network | Result |
|---|---|---|
| `server list` | No | Each configured server from the configuration file: name, transport, target. Invalid entries with their errors. It does not start a server and does not read the native store or the file, so it never shows a prompt. |
| `server show` | Yes | What the server says about itself in its handshake (`server/discover` or `initialize`): `serverInfo`, protocol version, capabilities and `instructions`, with the configuration target. No other request. |
| `tool list` | Yes | The tools of the server. Text: one line for each tool, with the name, the title or the first line of the description, and a mark for the read-only or destructive hint. The mark says "hint": the spec tells clients to treat annotations from a server as untrusted. JSON: the full MCP `Tool` objects. When the server has `instructions`, a hint on stderr names `server show`. |
| `tool show` | Yes | One tool for people to read: title, description, arguments (name, type, required, description, allowed values, default), output, annotations, and an example call. JSON: the MCP `Tool` object. |
| `tool call` | Yes | The tool result. `tool call <SERVER> <TOOL> --help` prints the same as `tool show`. |
| `auth login` | Yes | Runs the OAuth flow and keeps the tokens (section 5). |
| `auth logout` | No | Removes the OAuth tokens of the server from all places (section 2.4). The client credentials stay, so the next login needs no new registration. |
| `auth status` | No | For each server: OAuth tokens present, expiry, scopes, issuer. No token value. |
| `secret set` | No | Reads one value from stdin and keeps it under the name (section 2.4). On a TTY the input is not shown. The value never goes in an argument (S3). |
| `secret list` | No | The names of the stored secrets and the place of each one (native store, file, or both). Names that no `${NAME}` in the configuration uses are marked. Never a value. |
| `secret delete` | No | Removes the secret from all places and says from which. |
| `cache clean` | No | Removes all caches (section 6). `ncp` works the same with no cache. |
| `generate-shell-completion` | No | The completion script for the shell. Server names come from the configuration. Tool names come only from a cache, or are not completed. Completion never starts a server. |

`ncp --help` shows the normal order of work: `server list`, `server show`,
`tool list`, `tool show`, `tool call`.

Resources and prompts (P7) come after the MVP, in the same shape. Each
command uses the MCP methods on its line:

```
ncp resource list <SERVER>                     resources/list, resources/templates/list
ncp resource read <SERVER> <URI>               resources/read
ncp prompt list <SERVER>                       prompts/list
ncp prompt get <SERVER> <PROMPT> [ARGUMENT...] prompts/get
```

`resource list` shows the resources and the templates in two sections,
because the templates tell the user which URIs exist. `resource read` writes
text contents to stdout with no change, so that the output can go into a
pipe.

### 2.1 Names

- The server and the tool are two words. No joined form: `github.search`,
  `github:search` and `github/search` are usage errors. A server name cannot
  contain `.`, `:` or `/`, so `ncp` finds a joined form and shows the correct
  command, for example "use `ncp tool call github search_issues`".
- No short form `ncp <SERVER> <TOOL>`. A server with the name of a command
  (`tool`, `auth`) would collide with it.
- The MCP spec says that a tool name SHOULD use only `A-Z`, `a-z`, `0-9`,
  `_`, `-` and `.` ([tool names][spec-tool-names]). It is not a MUST, so `ncp` accepts any tool name. A name
  with spaces needs shell quotes. A name that starts with `-` needs `--`
  before it.
- An unknown server or tool name gives a usage error with the near names, for
  example "no tool `search_issue`; did you mean `search_issues`?".
- `tool list` prints names that the user can copy into `tool call`.

### 2.2 Tool arguments (I4)

| Form | Meaning |
|---|---|
| `key=value` | The value is text. When the input schema gives the property type `number`, `integer`, `boolean`, `array` or `object`, `ncp` reads the value as JSON. A schema type `string` keeps the text, also for `123`. |
| `key:=json` | The value is JSON, with no schema rule. |
| `key=@path` | The value is the text of the file. `@@` at the start is a literal `@`. |
| `{...}` as the only argument | The arguments object as JSON. |
| `-` as the only argument | Read the arguments object as JSON from stdin. |

A repeated key is a usage error. With an input schema, `ncp` checks the
arguments before the call (I5). An error shows the property path and the
expected arguments, as `tool show` shows them. `--no-validate` turns the
check off. When the word after the server contains `=`, the tool is missing,
and the error says "missing tool name".

### 2.3 Options

| Option | Meaning |
|---|---|
| `--output-format <text\|json>` | Output format. Default: `text` on a TTY, else `json`. |
| `--color <auto\|always\|never>` | Color in text output. Default `auto`. `NO_COLOR` also turns color off (I7). |
| `-q`, `--quiet` | No hints and no warnings on stderr. Errors stay. |
| `-v`, `--verbose` | Diagnostics on stderr: the source of each variable (environment, native store, file; never the value), the proxy route and its source, the cached era. `-vv` adds the JSON-RPC messages and the HTTP requests, with secrets hidden (I6). |
| `--timings` | Timings on stderr (F2): process start, configuration, credential read, server start, DNS, connect, server response, CPU time. |
| `--config-file <path>` | Configuration file, as uv names the flag. Default in section 6. |
| `--timeout <seconds>` | Wait for the result of one request, in whole seconds. A timeout sends a cancel request. The spec says implementations SHOULD set a timeout for each request and SHOULD enforce a maximum ([timeouts][spec-timeouts]). Default: open (section 7). |
| `--credential-store <native\|file>` | Only on `secret set` and `auth login`: the place for the new value. Default `native` (decision C2). |
| `--no-browser` | Only on `auth login`: print the URL and read the redirect URL from stdin (A10). |
| `--no-validate` | Only on `tool call`: do not check the arguments against the input schema. |

### 2.4 Secrets

A secret is a named value, the same as an environment variable (decision
C1). The configuration names the secrets that a server needs with `${NAME}`,
as other tools do:

```json
"github": { "command": "github-mcp", "env": { "GITHUB_TOKEN": "${GITHUB_TOKEN}" } },
"linear": { "type": "http", "url": "https://mcp.linear.app/mcp",
            "headers": { "Authorization": "Bearer ${LINEAR_KEY}" } }
```

`ncp` gets the value of `${NAME}` (or `${env:NAME}`) from the first place
that has it:

1. The environment of `ncp`.
2. The native store: the macOS Keychain or the Linux Secret Service.
3. The credential file, if it exists (opt-in, mode 0600).

When no place has it, `ncp` stops with `GITHUB_TOKEN is not set and not
stored` and the hint `ncp secret set GITHUB_TOKEN`. When the native store is
not available, the error says why (for example "the login keyring is
locked") and gives the full command `ncp secret set GITHUB_TOKEN
--credential-store file`.

- A value from the native store or the file expands only in `env` values and
  `headers` values. In `args` it would show in the process list (S3). In
  `command`, `args`, `cwd` or `url`, a stored value is a usage error (exit 2)
  with the hint to set the variable in the environment.
- `ncp` reads the native store only for names that the configuration uses
  and the environment does not set. A server with no `${NAME}` never opens
  the store.
- A write goes to one place. When the other place still has a copy, `ncp`
  shows a warning and the hint `ncp secret delete NAME`, which removes all
  copies. `secret list` shows the place of each secret.
- OAuth tokens and client credentials are separate items for each server.
  A refresh writes the tokens back to the place that they came from.

## 3. Exit codes (I3)

| Code | Class | When |
|---|---|---|
| 0 | Success | The command completed. For `tool call`: `isError` is absent or false. |
| 1 | Tool error | `tool call` got a result with `isError: true`. The result is on stdout. |
| 2 | Usage error | Bad arguments, unknown server or tool, a joined server and tool name, invalid configuration file or entry, arguments that fail the input schema, an unset `${NAME}`. |
| 3 | Protocol, transport or local error | The server cannot start, the connection fails, a handshake fails, a JSON-RPC error, a timeout, a local file (cache, credential file) that `ncp` cannot read or write. |
| 4 | Authorization error | The server needs a login (`401`), a scope is missing after the retry limit (`403`), the native store is not available, is locked, or gives no answer in 60 s (S8). |
| 5 | Input required | `tool call` got an `input_required` result (P8). The result is on stdout, so that a later version or the caller can continue. |
| 130 | Interrupted | Ctrl-C. `ncp` sent the cancel request first (P10). 130 is 128 + SIGINT, as shells use. |

Elicitation on a TTY (P9) can come later. With no TTY, an elicitation request
gives code 5.

## 4. JSON output (I2)

Rules:

- One JSON value on stdout for each command. Pretty on a TTY, compact in a
  pipe.
- MCP objects are copied with no change. Their schema is the MCP schema of
  the negotiated protocol version.
- `ncp` objects have stable names. A new field can come in a minor version.
  The removal or change of a field needs a major version.
- stderr is JSON Lines: one JSON object on each line. The objects are the
  error, warnings and hints, `-v` diagnostics, and timings. stdout is empty
  after an error, except for codes 1 and 5.
- All `ncp` field names use camelCase, as the MCP objects do.

| Command | stdout |
|---|---|
| `server list` | `{"servers": [Server], "invalid": [{"name", "error"}]}` |
| `server show` | `{"server", "target", "protocolVersion", "serverInfo", "capabilities", "instructions"}`. The last four use the names of the MCP spec. The set can become smaller before the first release. |
| `tool list` | `{"server": "<name>", "tools": [MCP Tool]}` |
| `tool show` | `{"server": "<name>", "tool": MCP Tool}` |
| `tool call` | MCP `CallToolResult`, or MCP `InputRequiredResult` (code 5) |
| `auth status` | `{"servers": [{"name", "loggedIn", "issuer", "scopes", "expiresAt"}]}` |
| `secret list` | `{"secrets": [{"name", "places": ["native", "file"], "used": true}]}` |

```json
{"name": "github", "transport": "http", "target": "https://api.example.com/mcp"}
```

Error object on stderr:

```json
{"error": {"code": "auth", "exit": 4, "server": "github",
  "message": "the server needs a login",
  "hint": "ncp auth login github"}}
```

`code` values: `tool`, `usage`, `protocol`, `auth`, `input_required`,
`interrupted`. Each value has one exit code (section 3).

Timings (`--timings`) on stderr, in JSON mode:

```json
{"timings": {"totalMs": 412, "cpuMs": 18, "configMs": 1, "credentialsMs": 3,
  "serverStartMs": 0, "dnsMs": 2, "connectMs": 41, "responseMs": [180, 95]}}
```

`connectMs` includes TCP, the proxy `CONNECT` and TLS. reqwest has no hook
for each part (early tests, section 5). The separate parts (F5, SHOULD) need
an own connector on hyper-util, after the MVP.

## 5. OAuth

`ncp` follows the authorization part of the MCP spec: discovery, a
pre-registered client, a Client ID Metadata Document (CIMD), then Dynamic
Client Registration, and the scope selection of the spec (decision C4).

- Redirect URI: `http://127.0.0.1:<port>/callback` with a random port.
  RFC 8252 §7.3: the authorization server "MUST allow any port to be
  specified at the time of the request for loopback IP redirect URIs".
  RFC 8252 §8.3: use the loopback IP literal, not `localhost`. A random port
  cannot conflict with another program, and needs no setting. Limit: an
  authorization server that breaks RFC 8252 and compares the port exactly
  does not work in the MVP. `callbackPort` is the planned answer.
- A CIMD lists the redirect URI `http://127.0.0.1/callback`. The port does
  not count for a loopback URI (RFC 8252 §7.3). The place of the default
  document is a release decision.
- Settings for one server go in an `oauth` object in its configuration
  entry. The MVP reads only `clientId`, for a pre-registered client. Names
  for later, from the most common shape in other tools: `clientSecret`,
  `callbackPort`, `scopes`, `authServerMetadataUrl`. They come when a real
  server needs them and a test can cover them. Until then, they are unknown
  keys with a warning.
- With no local browser (A10), `auth login --no-browser` prints the URL and
  reads the redirect URL from stdin. No listener is necessary: the browser
  shows a connection error, and the user copies the URL from the address
  bar. On a TTY with a browser, `auth login` opens the browser and also
  accepts a pasted URL, whichever comes first.
- `ncp` checks `state` and `iss` (RFC 9207, A4) on both paths.

## 6. File locations

`ncp` uses the XDG base directories on macOS and on Linux. The configuration
path in the requirements already does this.

| File | Path | Content | Mode |
|---|---|---|---|
| Configuration | `$XDG_CONFIG_HOME/ncp/mcp.json` (`~/.config/ncp/mcp.json`) | `mcpServers` | user choice |
| Era cache | `$XDG_CACHE_HOME/ncp/eras.json` (`~/.cache/ncp/eras.json`) | Era for each server and target | 0600 |
| List cache (P12, later) | `$XDG_CACHE_HOME/ncp/lists/<server>.json` | Tool lists with `ttlMs` | 0600 |
| Opt-in credential file (C2) | `$XDG_STATE_HOME/ncp/credentials.json` (`~/.local/state/ncp/credentials.json`) | Secrets and OAuth tokens | 0600, in a 0700 directory |
| Daemon socket (F4, later) | `$XDG_RUNTIME_DIR/ncp/daemon.sock`, else `$TMPDIR/ncp-<uid>/daemon.sock` | — | 0600, in a 0700 directory (S6) |

The era cache and the list cache are caches: `ncp` works the same when they
are removed. A cache file that does not load is ignored and written again.

## 7. Decisions

| ID | Decision | Reason |
|---|---|---|
| C1 | A secret is a named value. The configuration names it with `${NAME}`; `ncp` reads the environment, then the native store, then the opt-in file. No secret index file. One store item for each name. | `${VAR}` is the common syntax in the `mcpServers` files of other tools, so the file stays portable. No index file means no file that can disagree with the store. A server with no `${NAME}` never opens the store. |
| C1a | The environment comes before the store. | Common practice. A CI job or `op run` can give a value with no change to the store. `-v` shows the source of each name. |
| C1b | On macOS, `ncp` uses the login Keychain through `/usr/bin/security`. | `ncp` has no paid Developer ID. An ad-hoc signed binary changes identity at each update, and the native API then asks again for each item. An agent cannot answer the dialog. The cost: a process of the same user can read the items through `security`, as with gh and with the Linux Secret Service. |
| C2 | `--credential-store <native\|file>` on the commands that write. Default `native`. No environment variable. | Secure by default. The opt-in is at the moment of need, and the error names it. The same behavior in each shell, because reads check all places. |
| C3 | Probe timeout 10 s, the rmcp value, with no option. The era cache keeps any result for the server configuration, and probes again only when the cached path fails. `cache clean` removes all caches. This replaces the advice of early-tests.md to keep the cause of a legacy result. | The MCP spec permits a client to keep the result for the same server configuration ([versioning][spec-versioning]). A server that does not answer `server/discover` then costs the timeout once, not at each call. A changed server repairs itself, because a failure causes a new probe. Consequence: a server for both eras that is slow at its first start (for example an `npx` download) stays legacy until `cache clean`. It works, with the legacy protocol. |
| C4 | No settings file. Settings for one server go in the `oauth` object of the entry. Only the methods of the MCP spec; only `clientId` in the MVP. | Other tools already use an `oauth` object, so no new key. A field without a test target is not built. |
| C5 | Proxy as uv: the proxy variables first, then the manual HTTP and HTTPS proxy of the macOS system settings (reqwest `system-proxy`). No exception for loopback. | Common behavior of curl, Node, Bun, Deno, Python and uv (tested on 2026-10-07; Go is the exception). When a request to a loopback host through a proxy on another host fails, the error shows the route and the hint `export NO_PROXY=localhost,127.0.0.1,::1`. The macOS settings do not give the automatic proxy configuration or the bypass list. |
| C6 | Server and tool as two words. Commands in noun groups with the verbs of the MCP methods. `server show` shows the handshake data, with `instructions`. `tool call ... --help` shows the tool. | No split rule to learn, and shell completion works for each word. The MCP spec names `instructions` as guidance for the client ([lifecycle][spec-lifecycle]); an agent that uses a shell has no other way to read it. |

## 8. Order after the gate

1. Command line: `server list`, `server show`, `tool list`, `tool show`,
   `tool call`, exit codes, JSON and text output, argument parsing and schema
   check.
2. Ctrl-C with the cancel request, and `--timeout`.
3. Secrets: `${NAME}` from the store and the file, `secret` commands, static
   headers (A11), the macOS Keychain through `/usr/bin/security`.
4. `--verbose` and `--timings`, the proxy route and the loopback hint.
5. OAuth: `auth login`, `auth logout`, `auth status`, refresh, step-up, the
   rmcp workarounds of [rmcp-open-points.md](../research/rmcp-open-points.md).
6. Conformance adapter for the official suite (test harness report).
7. Release build: musl targets, checksums.

[spec-tool-names]: https://modelcontextprotocol.io/specification/2025-11-25/server/tools#tool-names
[spec-timeouts]: https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle#timeouts
[spec-lifecycle]: https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle#initialization
[spec-versioning]: https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning#backward-compatibility-with-initialization-based-versions
