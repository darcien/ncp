# Design: command line

Status: accepted by the user on 2026-10-08 (plan milestone 5, gate).
Baseline: [requirements.md](../requirements.md). Input:
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
- Choose in this order:
  1. The MCP spec.
  2. Secure by default. A common practice that is not secure is not a
     reference: many tools keep secrets in a plain file; `ncp` does not.
  3. Good ergonomics: the first try works, an error gives the exact fix,
     nothing hangs.
  4. Common practice of well-known tools: uv and mise first.
  5. The simplest design.
- The network commands use the verbs of the MCP methods (`list`, `call`,
  `read`, `get`).
- No command waits for input that it did not ask for. A command reads stdin
  only when an argument says so (`-`), when `secret set` or `server add
  --secret` reads the value, when `auth login` reads a pasted redirect URL,
  when `ncp` asks the trust question of a project file (B76), or when a
  server asks a question (elicitation) and stdin and stderr are a TTY
  (B63). Each wait
  has a time limit. Thus an agent cannot hang on stdin.
- No command starts OAuth, except `auth login`. A browser opens only for
  `auth login`, and for an elicitation in URL mode after the person says
  yes (B63).
- MCP objects pass through with no change: `Tool`, `CallToolResult`,
  `InputRequiredResult`. `ncp` adds only a small envelope where it must.

## 2. Commands

```
ncp [OPTIONS] server add <SERVER> <URL> [--global] [--header 'NAME: VALUE']... [--secret <NAME> [--credential-store <native|file>]]
ncp [OPTIONS] server add <SERVER> [--global] [--env NAME | --env NAME=VALUE]... [--secret <NAME> [--credential-store <native|file>]] -- <COMMAND> [ARG...]
ncp [OPTIONS] server remove <SERVER> [--global]
ncp [OPTIONS] server list
ncp [OPTIONS] server show <SERVER>
ncp [OPTIONS] tool list <SERVER>
ncp [OPTIONS] tool show <SERVER> <TOOL>
ncp [OPTIONS] tool call <SERVER> <TOOL> [ARGUMENT...]
ncp [OPTIONS] auth login <SERVER> [--no-browser] [--credential-store <native|file>]
ncp [OPTIONS] auth logout <SERVER>
ncp [OPTIONS] auth status [<SERVER>]
ncp [OPTIONS] secret set <NAME> [--server <SERVER>] [--credential-store <native|file>]
ncp [OPTIONS] secret list
ncp [OPTIONS] secret delete <NAME> [--server <SERVER> | --unused | --all]
ncp [OPTIONS] trust [--show | --untrust | --ignore]
ncp generate-shell-completion <bash|zsh|fish>
```

| Command | Network | Result |
|---|---|---|
| `server add` | No | Adds an entry to the project file, or with `--global` to the global file, and keeps every other byte of the file. With `--secret NAME`, also reads and stores the value of `${NAME}`. The result gives the steps that are left ([server-add.md](server-add.md)). |
| `server remove` | No | Removes the entry that runs for the name, or with `--global` the global entry, and the OAuth tokens of the server. `${NAME}` secrets stay ([server-add.md](server-add.md) section 6). |
| `server list` | No | Each server of the global file and of the project file (section 2.5): name, transport, target, scope, file, and the trust state of a project entry. Invalid entries with their errors. It does not start a server and does not read the native store or the file, so it never shows a prompt. |
| `server show` | Yes | What the server says about itself in its handshake (`server/discover` or `initialize`): `serverInfo`, protocol version, capabilities and `instructions`, with the configuration target. No other request. |
| `tool list` | Yes | The tools of the server. Text: one line for each tool, with the name, the title or the first line of the description, and a mark for the read-only or destructive hint. The mark says "hint": the spec tells clients to treat annotations from a server as untrusted. JSON: the full MCP `Tool` objects. When the server has `instructions`, a hint on stderr names `server show`. |
| `tool show` | Yes | One tool for people to read: title, description, arguments (name, type, required, description, allowed values, default), output, the annotation hints with the spec defaults (B64), and an example call. JSON: the MCP `Tool` object. |
| `tool call` | Yes | The tool result. `tool call <SERVER> <TOOL> --help` prints the same as `tool show`, so it connects to the server (or reads the list cache, B62); the help says so. |
| `auth login` | Yes | Runs the OAuth flow and keeps the tokens (section 5). |
| `auth logout` | No | Removes the OAuth tokens of the server from all places (section 2.4). The client credentials stay, so the next login needs no new registration. |
| `auth status` | No | For each server: OAuth tokens present, expiry, scopes, issuer. No token value. |
| `secret set` | No | Reads one value from stdin and keeps it under the name and its destination (section 2.4, B72). On a TTY the question shows the destination, and the input is not shown. The value never goes in an argument (S3). |
| `secret list` | No | One row for each name and destination, with the place (native store, file, or both) and the servers that use it. Rows that no `${NAME}` in the configuration uses are marked. Never a value. |
| `secret delete` | No | Deletes the item of one destination, the unused items, or all items of the name, from all places (B72). It says which destinations and places. |
| `trust` | No | Trusts the project file of the current folder, and shows its entries, as `mise trust`. `--show` shows them and changes nothing; `--untrust` removes the trust; `--ignore` refuses the file (section 2.5). |
| `generate-shell-completion` | No | The completion script for the shell. Server names come from the configuration. Tool names come only from a cache, or are not completed. Completion never starts a server. |

`ncp --help` shows the normal order of work: `server add`, `server list`,
`server show`, `tool list`, `tool show`, `tool call`.

Resources and prompts (P7) have the same shape. Each command uses the MCP
methods on its line:

```
ncp resource list <SERVER>                     resources/list, resources/templates/list
ncp resource read <SERVER> <URI>               resources/read
ncp prompt list <SERVER>                       prompts/list
ncp prompt get <SERVER> <PROMPT> [ARGUMENT...] prompts/get
```

`resource list` shows the resources and the templates in two sections,
because the templates tell the user which URIs exist. `resource read` with
`--output-format text` writes the contents to a pipe with no change, so
that the output can go into a file or another program (B58).

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
- An unknown server or tool name gives a usage error that points to
  `ncp server list` or `ncp tool list SERVER`, and names the nearest known
  name, for example "no tool `search_issue`; did you mean
  `search_issues`?". A name is near with at most one edit for each three
  characters (at least one), or with a case difference only.
- `tool list` prints names that the user can copy into `tool call`.

### 2.2 Tool arguments (I4)

| Form | Meaning |
|---|---|
| `key=value` | The value is text. When the input schema gives the property type `number`, `integer`, `boolean`, `array` or `object`, `ncp` reads the value as JSON. A schema type `string` keeps the text, also for `123`. |
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
| `-q`, `--quiet` | No hints and no warnings on stderr. Errors stay. |
| `-v`, `--verbose` | Diagnostics on stderr: the source of each variable (environment, native store, file; never the value), the proxy route and its source, the cached protocol version. `-vv` adds the JSON-RPC messages and the HTTP requests, with secrets hidden (I6). |
| `--timings` | Timings on stderr (F2): process start, configuration, credential read, server start, DNS, connect, server response, CPU time. |
| `--config-file <path>` | Configuration file, as uv names the flag. A missing file named here is a usage error. Default in section 6. |
| `--no-cache` | Read and write no cache: no cached protocol version and no cached tool list (B62), as uv names the flag. |
| `--refresh` | Read no cache, but write it: probe the protocol version and get the tool list again (B62), as uv names the flag. |
| `--timeout <seconds>` | The time limit for one request, in whole seconds. Default 300 s (decision C7). A timeout sends a cancel request and exits with code 3. Progress notifications do not reset the clock. |
| `--credential-store <native\|file>` | Only on `secret set`, `auth login` and `server add --secret`: the place for the new value. Default `native` (decision C2). |
| `--global` | Only on `server add` and `server remove`: the global file in place of the project file, as `mise use -g`. |
| `--header 'NAME: VALUE'`, `--env NAME[=VALUE]` | Only on `server add`: a header of an http entry, an `env` value of a stdio entry. `--env NAME` writes `${NAME}`. |
| `--secret <NAME>` | Only on `server add`: read the value of `${NAME}` (hidden on a TTY, else from stdin) and store it. |
| `--server <SERVER>` | Only on `secret set` and `secret delete`: use only the destination of this entry (B72). |
| `--unused`, `--all` | Only on `secret delete`: delete the items of the name that no entry uses, or all items of the name (B72). |
| `--no-browser` | Only on `auth login`: print the URL and read the redirect URL from stdin (A10). |
| `--no-validate` | Only on `tool call`: do not check the arguments against the input schema. |

### 2.4 Secrets

A secret is a named value, the same as an environment variable (decision
C1). The configuration names the secrets that a server needs with `${NAME}`,
as other tools do. The store keeps a value for one destination (S9,
[secret-binding.md](secret-binding.md)):

```json
"github": { "command": "github-mcp", "env": { "GITHUB_TOKEN": "${GITHUB_TOKEN}" } },
"linear": { "type": "http", "url": "https://mcp.linear.app/mcp",
            "headers": { "Authorization": "Bearer ${LINEAR_KEY}" } }
```

| Value | Destination | Account |
|---|---|---|
| `headers` value of an `http` entry | The origin of the entry URL: scheme, host and port | `secret/https://mcp.linear.app/LINEAR_KEY` |
| `oauth.clientSecret` | The origin of `oauth.issuer` | `secret/https://login.acme.dev/ACME_SECRET` |
| `env` value of a `stdio` entry | None | `secret/GITHUB_TOKEN` |

Add the entry first, then store the value:

```
ncp secret set LINEAR_KEY
```

The question shows the destination: "Value of LINEAR_KEY for
https://mcp.linear.app (server linear):". When two entries with different
destinations name `${NAME}`, `ncp secret set` gives a usage error that lists
them, with the hint `--server`.

`ncp` gets the value of `${NAME}` (or `${env:NAME}`) from the first place
that has it:

1. The environment of `ncp`. A value from the environment has no binding.
2. The native store, the macOS Keychain or the Linux Secret Service: only
   the item of the destination.
3. The credential file, if it exists (opt-in, mode 0600): only the item of
   the destination.

No item of another destination is a fallback. When no place has the item,
`ncp` stops before the send (exit 2):

```
error: LINEAR_KEY is not set, and not stored for https://mcp.linear.app.
ncp sent nothing to server linear. If this URL is correct, store the
value for it.
hint: ncp secret set LINEAR_KEY --server linear
hint: ncp secret list
```

When the native store is not available, the error says why (for example
"the login keyring is locked") and gives the full command `ncp secret set
LINEAR_KEY --server linear --credential-store file`.

- A value from the native store or the file expands only in `env` values,
  `headers` values and `oauth.clientSecret`. In `args` it would show in the
  process list (S3). In `command`, `args`, `cwd` or `url`, a stored value is
  a usage error (exit 2) with the hint to set the variable in the
  environment (B32).
- An entry that uses a stored value has no `${NAME}` in the scheme, host or
  port of its `url` and its `oauth.issuer` (I8). Else the stored value
  gives a usage error, and the message tells the user to write them as
  literal text.
- `ncp` reads the native store only for names that the configuration uses
  and the environment does not set. A server with no `${NAME}` never opens
  the store.
- A write goes to one place. When the other place still has a copy for the
  same destination, `ncp` shows a warning that names the destination, and
  the hint `ncp secret delete NAME --server SERVER`. `secret list` shows
  the place of each item.
- OAuth tokens and client credentials are separate items for each server.
  A refresh writes the tokens back to the place that they came from.

### 2.5 Project file

`ncp` also reads a project `.mcp.json` (requirement S5). The rules are in
[project-config.md](project-config.md); this section gives the short form.

- Search: from the current folder up to the git root. Not `~/.mcp.json`.
  The user must own the file.
- Scopes: global (`~/.config/ncp/mcp.json`) and project. For the same name,
  the trusted project entry wins.
- Trust: as the paranoid mode of mise. One record for each path with the
  hash of the bytes. Each change asks again. `ncp trust` trusts, as `mise
  trust`.
- Gate: a command that names a server of an untrusted file asks "Trust?
  [y/N]" with a TTY, and stops with exit 2 and the entries with no TTY.
  Other commands work as before.
- `${NAME}` in a trusted project entry expands as in the global file.
- `server add` and `server remove` write the project file; `--global`
  writes the global file ([server-add.md](server-add.md)).

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

Elicitation (P9) asks a person when stdin and stderr are a TTY (B63). With no
TTY, an elicitation request gives code 5.

## 4. JSON output (I2)

Rules:

- One JSON value on stdout for each command. Pretty on a TTY, compact in a
  pipe.
- MCP objects are copied with no change. Their schema is the MCP schema of
  the negotiated protocol version.
- `ncp` objects have stable names. A new field can come in a minor version.
  The removal or change of a field needs a major version.
- stderr is JSON Lines: one JSON object on each line. The key of each
  object is its kind: `error`, `warning`, `hint`, `debug` (`-v`), `trace`
  (`-vv`), `progress`, `serverLog` (a stderr line of a stdio server),
  `login` (the URL of `auth login`) and `timings`. stdout is empty
  after an error, except for codes 1 and 5.
- All `ncp` field names use camelCase. The only reasons are the MCP spec
  and consistency: `ncp` copies MCP objects with no change, and their
  names are camelCase (`serverInfo`, `inputSchema`), so one output must
  not mix two styles. Without the spec, `ncp` would use snake_case, as uv
  (`version_parts`) and mise (`requested_version`) do. A field that holds
  the value of a flag has the name of the flag in camelCase, for example
  `--credential-store` and `credentialStore`.

| Command | stdout |
|---|---|
| `server list` | `{"servers": [Server], "invalid": [{"name", "error"}]}` |
| `server show` | `{"server", "target", "protocolVersion", "serverInfo", "capabilities", "instructions"}`. The last four use the names of the MCP spec. The set can become smaller before the first release. |
| `tool list` | `{"server": "<name>", "tools": [MCP Tool]}` |
| `tool show` | `{"server": "<name>", "tool": MCP Tool}` |
| `tool call` | MCP `CallToolResult`, or MCP `InputRequiredResult` (code 5) |
| `auth status` | `{"servers": [{"name", "loggedIn", "issuer", "scopes", "expiresAt"}]}` |
| `secret list` | `{"secrets": [{"name", "destination", "credentialStores": ["native", "file"], "used": true, "servers": ["linear"]}]}`. `destination` is `null` for a stdio value. |
| `resource list` | `{"server": "<name>", "resources": [MCP Resource], "resourceTemplates": [MCP ResourceTemplate]}` |
| `resource read` | MCP `ReadResourceResult`; with `--output-format text` in a pipe, the contents as they are (B58) |
| `prompt list` | `{"server": "<name>", "prompts": [MCP Prompt]}` |
| `prompt get` | MCP `GetPromptResult`, or MCP `InputRequiredResult` (code 5) |

```json
{"name": "github", "transport": "http", "target": "https://api.example.com/mcp",
 "scope": "project", "file": "/work/app/.mcp.json", "trust": "trusted"}
```

Error object on stderr:

```json
{"error": {"code": "auth", "exit": 4, "server": "github",
  "message": "the server needs a login",
  "hint": "ncp auth login github"}}
```

`code` values: `tool`, `usage`, `protocol`, `auth`, `input_required`,
`interrupted`. Each value has one exit code (section 3).

`server` and `hint` are optional. An `input_required` error for pages that
the user must open has `pages`: `[{"url": "...", "message": "..."}]` (B65).

Timings (`--timings`) on stderr, in JSON mode:

```json
{"timings": {"totalMs": 412, "cpuMs": 18, "configMs": 1, "credentialsMs": 3,
  "serverStartMs": 0, "dnsMs": 2, "connectMs": 41, "responseMs": [180, 95]}}
```

`connectMs` includes TCP, the proxy `CONNECT` and TLS, but not DNS: reqwest
looks up the name inside its connector, so `ncp` takes `dnsMs` away from
the connect time. reqwest has no hook
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
  not count for a loopback URI (RFC 8252 §7.3). The document is on the
  GitHub Pages site of the repository, at a fixed path. The URL comes with
  the first release (A7).
- A CIMD URL must not move. A server does not follow a redirect (CIMD
  draft-02 §5), and a new URL is a new client (§8.3). Before a change, the
  old URL must keep the same document for as long as old versions or old
  tokens can use it.
- Settings for one server go in an `oauth` object in its configuration
  entry. The MVP reads `clientId`, `clientSecret` and `issuer`, for a
  pre-registered client (B68). Names for later, from the most common shape in other tools:
  `callbackPort`, `scopes`, `authServerMetadataUrl`. They come when a real
  server or an official conformance scenario needs them, and a test can
  cover them. Until then, they are unknown keys with a warning.
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
| Global file | `$XDG_CONFIG_HOME/ncp/mcp.json` (`~/.config/ncp/mcp.json`) | `mcpServers` | user choice |
| Project file (S5) | `.mcp.json` of the current folder or a parent, up to the git root | `mcpServers` | user choice |
| Protocol version cache | `$XDG_CACHE_HOME/ncp/protocol-versions.json` | Protocol version for each server and target | 0600 |
| List cache (P12) | `$XDG_CACHE_HOME/ncp/lists/<server>.json` | Tool lists with `ttlMs` | 0600 |
| Trust records (S5) | `$XDG_STATE_HOME/ncp/trusted-configs/<hash of path>` and `ignored-configs/<hash of path>` | The hash of the trusted bytes; an ignored record is empty | 0600, in a 0700 directory |
| Opt-in credential file (C2) | `$XDG_STATE_HOME/ncp/credentials.json` (`~/.local/state/ncp/credentials.json`) | Secrets and OAuth tokens | 0600, in a 0700 directory |
| Daemon socket (F4, later) | `$XDG_RUNTIME_DIR/ncp/daemon.sock`, else `$TMPDIR/ncp-<uid>/daemon.sock` | — | 0600, in a 0700 directory (S6) |

The protocol version cache and the list cache are caches: `ncp` works the same when they
are removed. A cache file that does not load is ignored and written again.
The file of a server that the configuration no longer has stays until the
user removes `~/.cache/ncp`: a cache file holds no credential, and the
target and the digest of the entry stop its use for another server
(B62).

## 7. Decisions

| ID | Decision | Reason |
|---|---|---|
| C1 | A secret is a named value. The configuration names it with `${NAME}`; `ncp` reads the environment, then the native store, then the opt-in file. No secret index file. One store item for each name and destination (S9). Changed on 2026-10-10 by [secret-binding.md](secret-binding.md): before, one item for each name. | `${VAR}` is the common syntax in the `mcpServers` files of other tools, so the file stays portable. No index file means no file that can disagree with the store. A server with no `${NAME}` never opens the store. |
| C1a | The environment comes before the store. | Common practice. A CI job or `op run` can give a value with no change to the store. `-v` shows the source of each name. |
| C1b | On macOS, `ncp` uses the login Keychain through `/usr/bin/security`. | `ncp` has no paid Developer ID. An ad-hoc signed binary changes identity at each update, and the native API then asks again for each item. An agent cannot answer the dialog. The cost: a process of the same user can read the items through `security`, as with gh and with the Linux Secret Service. |
| C2 | `--credential-store <native\|file>` on the commands that write. Default `native`. No environment variable. | Secure by default. The opt-in is at the moment of need, and the error names it. The same behavior in each shell, because reads check all places. |
| C3 | Probe timeout 10 s, the rmcp value, with no option. The protocol version cache keeps any result for the server configuration, and probes again only when the cached path fails. No command clears the cache in the MVP; the user can remove `~/.cache/ncp`. This replaces the advice of early-tests.md to keep the cause of a legacy result. | The MCP spec permits a client to keep the result for the same server configuration ([versioning][spec-versioning]). A server that does not answer `server/discover` then costs the timeout once, not at each call. A changed server repairs itself, because a failure causes a new probe. Consequence: a server for both handshakes that is slow at its first start (for example an `npx` download) keeps `initialize` until the cache is removed. It works, with the `2025-11-25` protocol. |
| C4 | No settings file. Settings for one server go in the `oauth` object of the entry. Only the methods of the MCP spec; only `clientId`, `clientSecret` and `issuer` in the MVP. | Other tools already use an `oauth` object, so no new key. A field without a test target is not built. A real server or an official conformance scenario is a test target. |
| C5 | Proxy as uv: the proxy variables first, then the manual HTTP and HTTPS proxy of the macOS system settings (reqwest `system-proxy`). No exception for loopback. | Common behavior of curl, Node, Bun, Deno, Python and uv (tested on 2026-10-07; Go is the exception). When a request to a loopback host through a proxy on another host fails, the error shows the route and the hint `export NO_PROXY=localhost,127.0.0.1,::1`. The macOS settings do not give the automatic proxy configuration or the bypass list. |
| C6 | Server and tool as two words. Commands in noun groups with the verbs of the MCP methods. `server show` shows the handshake data, with `instructions`. `tool call ... --help` shows the tool. | No split rule to learn, and shell completion works for each word. The MCP spec names `instructions` as guidance for the client ([lifecycle][spec-lifecycle]); an agent that uses a shell has no other way to read it. |
| C7 | No request waits forever. Each request has a limit of 300 s, and `--timeout <seconds>` changes it. The whole connect (server start, the 10 s probe, `initialize`) has a fixed limit of 30 s. A request timeout sends a cancel request. A connect timeout stops the server and sends no cancel request, because the spec says a client MUST NOT cancel `initialize`. | The spec says implementations SHOULD set a timeout for all sent requests and SHOULD enforce a maximum ([timeouts][spec-timeouts]). An agent must not hang. 300 s and 30 s are the defaults of OpenAI Codex (`DEFAULT_TOOL_TIMEOUT`, `DEFAULT_STARTUP_TIMEOUT`); Goose also uses 300 s. Both use rmcp. |

## 8. Order of work

The order is in [plan.md](../plan.md), milestones 6 to 13.

## 9. Decisions during build

Small decisions that an agent makes during the build, with the reason. The
user reviews them.

| ID | Decision | Reason |
|---|---|---|
| B1 | After a request timeout or a Ctrl-C, `ncp` waits at most 2 s to send the cancel request. Then it continues with the result of the request. | Good ergonomics: a server that does not read the cancel request must not stop the exit. 2 s is also the wait of each stop step of a stdio server. |
| B2 | `tool list` reads all pages of `tools/list`. Each page is one request with its own time limit. | The spec: the time limit applies to each request. One limit for all pages would need a second option. |
| B3 | A warning or a hint on stderr in JSON mode: `{"warning": {"message", "server"}}` and `{"hint": {"message", "server"}}`. `server` is absent when the note is not about one server. Text mode: `warning: ...` and `hint: ...`, as uv writes them. | The same names as the error object, so one parser reads all lines. |
| B4 | `server list` shows the unknown-key warnings of all entries. Other commands show only the warnings of the selected server. | Good ergonomics: a call to one server does not repeat the warnings of other servers at each run. |
| B5 | `tool call` reads the tool list first only when it needs the input schema: for `key=value` arguments, for the schema check, and for `--help`. With `--no-validate` and a JSON object, it sends only `tools/call`, except to an http server with a protocol version from `2026-07-28`: there it reads the list too. | One request less, and a tool that the list does not show stays callable. From `2026-07-28` an HTTP client mirrors each parameter with `x-mcp-header` into an `Mcp-Param-*` header (SEP-2243), and rmcp knows these parameters only from a listed tool. |
| B6 | When `ncp` cannot use an input schema (for example a `$ref` to another document), it shows a warning and sends the call with no check. | Secure by default: the check resolves no reference over the network or from a file (N1). A bad schema from a server must not block a correct call. |
| B7 | When no server is configured, `server list` writes the message as a hint on stderr, with the path. stdout has the empty list in JSON mode and nothing in text mode. | stdout has only the result, so a script reads the same shape in all cases. |
| B8 | Text output lists object keys in name order, for example the arguments in `tool show`. | serde_json keeps keys in name order; the JSON output is the same MCP object. |
| B9 | An environment variable that is not UTF-8 is not a `${NAME}` value, and a stdio server does not get it. | A JSON configuration value is UTF-8 text. |
| B10 | A `401` or `403` from an HTTP server gives exit 4 with the message "the server needs a login or more scopes" and the hint `ncp auth login SERVER`. | The fix of the error. |
| B11 | `tool call` writes the `tool` and `input_required` error objects on stderr too, after the result on stdout. | Each exit code has one `code` value (section 4); a caller that reads only stderr sees why the code is not 0. |
| B12 | A result with a task (`CreateTaskResult`) gives exit 3. | `ncp` does not ask for tasks, so the server breaks the protocol. |
| B13 | Output and the protocol version cache use the target as the file writes it, with `${NAME}` not expanded. | Secure by default: an expanded value can be a secret, and must not go to a plain file or to the screen (S2, S3). A changed variable value can make the cached protocol version wrong; then the handshake fails and `ncp` probes again. |
| B14 | In JSON mode, each line that a stdio server writes on stderr becomes `{"serverLog": {"server", "message"}}` on the stderr of `ncp`. In text mode the line goes through, with control characters as escapes (B15). A line that is not UTF-8 shows with replacement characters, and a line longer than 64 KiB comes in parts. | stderr stays JSON Lines (section 4), and the server log stays visible for debugging. |
| B15 | Text output shows each control character, except new line and tab, as an escape such as `\u{1b}`. The Unicode characters that change the direction of text or have no width (for example U+202E and U+200B) also show as escapes. The same characters, and also tag characters, Hangul fillers and line separators, are `\uXXXX` escapes in JSON output. The variation selectors U+FE00 to U+FE0F stay, because many emoji have one. A table cell shows a new line as `\n`, and the columns are measured in characters after the escapes; a wide character (CJK, emoji) can still move a column. | Secure by default: a server or a configuration value can hold terminal escape sequences that change the title, write the clipboard, or hide lines, and direction marks that put the parts of a URL in a different order on the screen. |
| B16 | For `key=value`, `ncp` finds the property types also in `anyOf`, `oneOf` and `allOf`, in a `$ref` into the same schema, and in `enum` and `const`. `tool show` shows the same types. | Good ergonomics: tool SDKs write an optional number as `anyOf` with `null`, so the first try must work. |
| B17 | A `tools/call` error "invalid params" (-32602) is a usage error (exit 2). The hint is `ncp tool show SERVER TOOL` when `ncp` found the tool in the list, else `ncp tool list SERVER`. | The spec uses this code for an unknown tool and for a request that does not satisfy the `CallToolRequest` schema (MCP 2025-11-25, server/tools, "Error Handling"). Both are usage errors (section 3). An input validation error of the tool is a tool execution error with `isError` (exit 1). With no list, the tool can be unknown. |
| B18 | A protocol version cache that cannot be read or written gives a warning, and the command continues. | Section 6: `ncp` works the same with no cache. A read-only cache directory must not stop a call that works. |
| B19 | An entry with type `sse` is a usage error (exit 2) that names the configuration file. An invalid entry also names the file. | The fix is in the file. |
| B20 | `tool call --help` with no tool gives the help of the command. With a tool, it gives the output of `tool show`. | The design gives `tool call SERVER TOOL --help`; the other forms keep the normal help. |
| B21 | A parse error of the command line gives the JSON error object in a pipe, with the hint `ncp --help`. No command gives the message "missing command". | stderr is JSON Lines in a pipe. |
| B22 | No `HOME` and no XDG variables, or a configuration file that cannot be read, gives exit 3. A file that does not parse gives exit 2. | Section 3: a local file that `ncp` cannot read is a local error; a bad file is a usage error. |
| B23 | A word in the place of the tool that contains `=`, starts with `{`, or is `-`, gives "missing tool name". | These are the argument forms of section 2.2. |
| B24 | `tool list` shows an annotation mark only for a hint that the server sets to true. | A mark for each default value would mark almost all tools. |
| B25 | The JSON of `tool list` and `tool show` is the `Tool` object as rmcp reads it. A field that rmcp does not know is not in the output. | rmcp gives typed objects only. Known limit; see the backlog. |
| B26 | When `-` reads the arguments from a TTY, a hint says to end the input with Ctrl-D. | Good ergonomics: nothing seems to hang. |
| B27 | `secret set` writes `{"name", "credentialStore", "destination", "servers"}`, and `secret delete` writes `{"name", "file", "deleted": [{"destination", "credentialStores"}]}`. `destination` is `null` for a stdio value. Text: one sentence, for example "Stored LINEAR_KEY in the native store for https://mcp.linear.app (server linear)." | Section 4 gives no shape for these commands; the names follow `secret list`. |
| B28 | `secret delete` of a name that no place has is a usage error (exit 2) with the hint `ncp secret list`. | The user can have a typing error in the name; a silent success hides it. |
| B29 | `secret set` does not keep one new line at the end of the input (LF or CRLF). An empty value is a usage error. | `echo` and a pasted line add a new line, which is never part of a token. |
| B30 | When a store cannot answer, `secret list` shows a warning and lists the other place. `secret delete` removes the copies that it can reach, and fails only when it removed none. With no flag, a place that cannot list stops the delete before it removes anything, because an item there could be a second destination (B72); the message says so and gives the hint `--server`. | Good ergonomics: one broken place does not hide the other. |
| B31 | A native store that cannot answer does not stop the search for `${NAME}`: the file can have the name. When no place has it, the error gives the store error (exit 4) and the hint `ncp secret set NAME --credential-store file`. | Section 2.4, with the order of the places kept. |
| B32 | A `${NAME}` in `command`, `args`, `cwd` or `url` comes only from the environment. `ncp` does not read a store for it. The error is "variable NAME is not set" with the hint `export NAME=<value>`. | Section 2.4: a stored value must not go into the process list or a URL. Not reading the store also shows no prompt. |
| B33 | A credential file that `ncp` cannot read or write gives exit 3. A native store that is not available, is locked, or gives no answer gives exit 4. | Section 3. |
| B34 | Items of the earlier layout (`<server>/env/<NAME>`) are ignored. | No release has used them (requirements: stored credentials can reset before the first release). |
| B35 | After `secret set`, `ncp` checks the other place for a copy, with a limit of 5 s, and warns when the environment sets the name. Each warning names the place that wins. The check uses the same key, so it looks only at the same destination. When a copy is found, the warning names the destination, and the hint is `ncp secret delete NAME --server SERVER` (section 2.4): it removes the copies of this destination, and the user then stores the value in one place. When the check fails, the warning says so and gives no hint. | The check only gives a warning, so it must not wait for a locked store. |
| B36 | A host with no native store (for example Linux with no Secret Service) gives no warning in `secret list` and `secret delete`: the native store counts as empty. | A user who keeps all values in the file must not see the same warning at each run. |
| B37 | `secret list` with a configuration file that does not load gives a warning, and no secret shows as used. | The names come from the stores, not from the file. |
| B38 | On Linux, the list of names reads no value and unlocks nothing. A locked item gives the warning "unlock the keyring to list them". `secret delete` removes the item in one call, which can show the unlock prompt, as `secret set` does. | keyring-core unlocks each item that a search finds, which shows a prompt. The design says that the list reads no value. |
| B39 | `secret set` checks the store before it reads the value. On a TTY, it turns off the echo and the line mode with `/bin/stty`, and does the backspace itself, so that a value longer than 1024 bytes works on macOS. It restores the earlier settings after the line and after Ctrl-C (exit 130). When `stty` fails, `secret set` fails and reads no value. | The user does not type a secret twice, and the shell of the user works after Ctrl-C. The value never shows. |
| B40 | Each run of `/usr/bin/security` stops 1 s before the limit of its store call. `secret delete` removes and reports in one run. Ctrl-C stops the program too, because it stays in the process group of `ncp`, and gives exit 130. | A program that stays can show a dialog after the exit of `ncp`, or write or remove an item late. |
| B41 | `-v` writes `debug: server NAME: message` in text and `{"debug": {"message", "server"}}` in JSON. It shows the configuration file, the source of each `${NAME}` of `env` and `headers` ("from the environment", "from the native store", "from the credential file"), the proxy route of an HTTP server with its source, the cached protocol version, and the protocol version that `ncp` saves. | The same shape as warnings and hints (B3). uv also writes `DEBUG` lines for `-v`. |
| B42 | `-vv` writes each JSON-RPC message as `{"trace": {"server", "sent": message}}` or `{"trace": {"server", "received": message}}`, and each HTTP request as `{"trace": {"server", "http": {"method", "url", "headers", "outcome", "ms"}}}`. Text: `trace: server NAME: sent {...}`. The headers are the ones that `ncp` gives; reqwest adds `accept`, `content-type` and `content-length`. The outcome is the kind of response (`202 accepted`, `JSON response`, `event stream`) or the error, because the rmcp HTTP client gives no status code for a success. The message bodies are not changed. | The user gave the arguments and gets the results on stdout. The secrets of the connection are in the headers and the URL, and these are hidden (B43). |
| B43 | `-vv` hides the values of requirement S3 and no other value: values from `${NAME}` and the credential stores, the credential headers of HTTP, and the tokens and credentials of OAuth. It does not guess from a name: a URL query value or a header with a literal value shows as it is. A trace body or error is not a URL: it shows with each `${NAME}` value of the server URL hidden, also in its encoded forms. | A guess from a name hides some secrets and not others, and the user cannot know which. The rule of S3 is a fact that the user can check. |
| B44 | `--timings` writes one line at the end of the run, also after an error. Times are in milliseconds with one decimal, because a local step takes less than 1 ms. A step that did not occur is absent. `dnsMs`, `connectMs` and `serverStartMs` add up all occurrences, for example a second start after a probe timeout. `serverStartMs` is the time to start the process; the handshake time is in `responseMs`. `totalMs` starts when `main` starts: `std` gives no start time of the process. `cpuMs` is the CPU time of `ncp` only, not of a stdio server or `/usr/bin/security`. `connectMs` has no DNS time (section 4), and the first `responseMs` includes the connect. `credentialsMs` shows only when a store is read. It adds up each read and write of a store in the run: the `${NAME}` values, the OAuth tokens and client, and the read and write of the tokens at a refresh. | F2 asks for CPU time and network time apart; the run with the error is often the one to measure. |
| B45 | `auth login` writes `{"server", "issuer", "scopes", "credentialStore"}`, and in text "Logged in to SERVER at ISSUER. The tokens are in PLACE.". The URL to open goes to stderr: text lines for a person, `{"login": {"server", "url"}}` in JSON. `auth logout` writes `{"server", "credentialStores"}`. `auth status` text is a table: name, state, issuer, scopes, expiry. | The same names as `secret set`, `secret delete` (B27) and the `auth status` shape of section 4. An agent can give the URL to a person. |
| B46 | `auth login` always listens on `127.0.0.1` with a random port. It also reads a pasted redirect URL from stdin when stdin is a TTY or with `--no-browser`. The first redirect wins. The wait has the limit of `--timeout` (default 300 s), and Ctrl-C gives 130. On macOS the browser is `/usr/bin/open`; on Linux it is `xdg-open`. When the browser cannot open, a warning shows, and the login waits for the paste. | Section 5: whichever comes first. A listener with `--no-browser` costs nothing and helps when the browser is on the same host. Nothing waits forever (C7). |
| B47 | The client item goes to the place of the tokens (`--credential-store`). A login reads the client item only from that place, and uses it only when its issuer is the current issuer. With no usable item, `ncp` registers the client itself (DCR) and keeps the item before the login continues. With `oauth.clientId`, `ncp` keeps no client item (B68). | Requirement A5. rmcp keeps a registered client secret only in memory (rmcp issue #784). A registration that a later step loses is not done again. |
| B48 | `ncp` refuses an authorization server endpoint that does not use HTTPS, except on a loopback host. A missing `issuer` in the metadata falls back to the origin of the authorization endpoint. | The spec: all authorization server endpoints MUST use HTTPS (MCP 2025-11-25, basic/authorization, "Communication Security"). rmcp does not check it. Local tests need loopback. |
| B49 | `auth status` reads the native store and then the file for each http server. `auth logout` removes the tokens from both places and keeps the client item; a native store that gives no answer gives a warning, and the file copy still goes away. | Design 2.4 and the `auth logout` row of section 2. |
| B51 | `ncp` reads the stored tokens of an http server only after the server answers `401`, and then connects again with the token. With no tokens, or tokens of another issuer, the error is exit 4 with the hint `ncp auth login SERVER`. A refresh writes the new tokens to the place that they came from. When the native store cannot keep the tokens at login, the error is exit 4 with the hint `--credential-store file`. When the request sent a stored header value and the server answers `401`, the first hint is `ncp secret set NAME --server SERVER`, and the second hint is `ncp auth login SERVER`: after a change of the key at one origin, the key at another origin can be old (S9). | Design C1: a server with no login never opens the native store. The cost is one more request for each run of an OAuth server. A long token set goes into several items (B66). |
| B52 | After `403 insufficient_scope`, a call gives exit 4 that names the scopes, with the hint `ncp auth login SERVER`. `ncp` keeps the scopes in the tokens item (field `ncp_required_scopes`, which rmcp ignores) until the next login. `auth login` asks for the scopes of B79. There is no automatic retry. | Requirement A9: the old and the new scopes. A new authorization needs the browser, and a call never waits for input that it did not ask for. Each step-up is one login by the user, so it cannot loop. |
| B53 | After a login, `ncp` removes the tokens item of the other place, with a limit of 5 s; a failure gives a warning. The search for tokens reads the native store and then the file, as for `${NAME}`: a native store that cannot answer does not stop the search, and its error (exit 4, with the reason) is the result only when the file has no tokens. The OAuth steps of a login have the limit of `--timeout`, with exit 3 at the limit; the steps after a `401` (discovery, refresh) have the connect limit of 30 s, after the store read with its own limit. Ctrl-C gives 130. The listener answers each connection in its own task with a limit of 5 s; a redirect with another state gets `400`, and the wait goes on. Discovery that finds no metadata is an error. | Design 2.4: calls read the native store first, so an old copy there would hide new tokens in the file, and a locked keyring must not hide tokens in the file or look like a missing login. Section 3: a timeout is exit 3. C7: nothing waits forever. Any local program or web page can send a request to the listener. The fallback endpoints of rmcp are for 2025-03-26 only. |
| B54 | When the answer to `server/discover` is a result that is not a discover result (for example an empty result), `ncp` treats the server as legacy and sends `initialize`, as for a JSON-RPC error. | Good ergonomics: the official conformance server answers an unknown method so, and the server works with `initialize`. A modern server gives a discover result. |
| B55 | Lock files: `$XDG_STATE_HOME/ncp/credentials.lock` for each change of the credential file, and `$XDG_STATE_HOME/ncp/oauth-<server>.lock` for each refresh of the tokens of a server, in the native store or the file. They are empty, mode 0600. A refresh, the save of a login and a logout hold the lock of the server; a login or a logout that cannot get it in 45 s goes on with a warning. A refresh waits at most 45 s for the lock. With the lock, it reads the tokens again, because another process can have refreshed them. A refresh that does not get the lock in 45 s fails with exit 3. When the lock file cannot be made (for example a read-only home), a refresh of tokens in the native store goes on with no lock. | Two processes that refresh at the same time use one refresh token twice; with rotation the second refresh fails, and the user must log in again. Agents start calls in parallel. The file lock of `std` needs no new dependency. |
| B56 | `-q` and `-v` together are a usage error; `-qq` is the same as `-q`. In uv, `-qq` also hides stdout; `ncp` keeps stdout, because the result is the output. `--output-format json` on a TTY gives pretty JSON (section 4). `-q` keeps the server log of a stdio server and the result. `--config-file` takes a path that is relative to the working directory, and the file must exist (exit 2). `--output-format` changes only the format, not the TTY checks (for example the hidden input of `secret set`). | A quiet run with diagnostics has no meaning. The server log is the only view of a server failure. A typing error in a named path must not look like an empty configuration. |
| B57 | Removed on 2026-10-10: `ncp` has no `key:=json` and no `key=@path`. JSON arguments come with `{...}` or `-`. The text of a file comes from the shell: `key="$(cat FILE)"`. | Not in the spec, and a second form for each job. A value that starts with `@`, for example an npm scope or a user name, must stay text and never read a file. |
| B58 | `resource read` follows I1: JSON (`ReadResourceResult`) in a pipe, text on a TTY. On a TTY, text gets the escapes of B15, and a blob shows as one summary line. With `--output-format text` in a pipe, it writes the one content as it is: the text, or the bytes of the blob. A resource with more than one content then is a usage error with the hint `--output-format json`, and `ncp` decodes before it writes, so that stdout stays empty after an error. A server with no `resources` or `prompts` capability gives an empty list and a hint. A server with no `resources/templates/list` gives no templates. "Invalid params" and "resource not found" are usage errors with the hint `ncp resource list SERVER` or `ncp prompt list SERVER`. Prompt arguments are `key=value`: the spec gives them as text. `input_required` gives exit 5, as for `tool call`. A list whose server gives a page cursor again stops with exit 3. | I1 is a MUST. The raw form lets an image go into a file. Binary data on a terminal can change the terminal. Several contents have no raw form: their borders and types would be lost. A repeated cursor would make the list endless. |
| B59 | clap writes the static part of each completion script (commands, options, value choices). A server name comes from `ncp __complete-servers` at completion time, and only a name that passes the name rule goes out, because a shell expands each word again: a word that is not in the grammar, so that the scripts do not offer it. It reads only the configuration file, writes one name on each line, and writes nothing (exit 0) when the configuration cannot be read. The scripts run it at the server argument of each command. A tool name comes from `ncp __complete-tools SERVER`, which reads the list cache of the server, also an old one, when the list is of the configured entry (same target and digest), and gives only names of the characters `A-Z a-z 0-9 _ - .`: a tool name comes from a server, and a shell expands the words again. With no cache, no tool name completes. The hidden words read the default configuration file: `--config-file` on the command line does not change the names, because each of the three scripts would need to parse it. | The dynamic engine of clap_complete needs unstable features of two crates. The hidden word starts no server and opens no store, so a tab is fast and shows no prompt. Tool names need a running server or a cache. |
| B60 | No color (removed on 2026-10-10, with `--color`). Each line of a message after the first is indented, so that a server message cannot start a line that looks like a note of `ncp`. | Not in the spec. With no color, `NO_COLOR` (I7) is met. The indent is the part that stops a false note. |
| B61 | Each progress notification of a request goes to stderr: `progress: server NAME: 3/10 message` in text, `{"progress": {"server", "progress", "total", "message"}}` in JSON, with the fields that the server sends. `-q` hides it. A whole number shows with no decimals in text. A new line in the message shows as `\\n`. A notification when no request waits is dropped, and at most one line goes out in 200 ms, except the last step (progress equal to total). Progress does not change the time limit (C7). | Requirement P11. stderr is JSON Lines in a pipe (section 4). rmcp adds a progress token to each request, so each server can report progress. |
| B62 | A tool list with `ttlMs` goes to the list cache with the target, a digest of the entry as the file writes it (`env`, `headers`, `oauth.clientId`) and the time, for at most 24 h. A list that is private, or has no `cacheScope`, is kept only when the connection used no credential (no OAuth token, no `${NAME}` value): the cache keeps no copy of a credential, so it cannot see a new one. Such a list of a stdio server is never kept: the server gets the environment of `ncp`, which can hold a credential that the entry does not show. A login, a logout, and a change of a secret remove the cached lists. While a list is fresh for the same target and entry, `tool list` and `tool show` use it and start no server, and `tool call` uses it for the schema; `-v` says so. A cached list that does not have the tool, or whose schema rejects the arguments, gets one live list on the connection. On an http server with a protocol version from `2026-07-28`, `tool call` lists again, because rmcp mirrors the `x-mcp-header` parameters only for a tool that it listed in the session (B5). A list that `ncp` must not keep removes the old file. A cache that cannot be written gives a warning. `--no-cache` turns off the list cache and the protocol version cache. `--refresh` reads neither, but writes both; with `--no-cache`, `--no-cache` wins, as in uv. | Requirement P12. The spec: a private result MUST NOT be shared across authorization contexts (MCP 2026-07-28, server/utilities/caching), and a client MAY fetch again when it has reason to believe that the data changed. A wrong `ttlMs` must not keep an old list for days. |
| B63 | When stdin and stderr are a TTY, `ncp` says in its capabilities that it can answer elicitation in form and URL mode, and it asks on stderr: the message of the server, then each field with its type, its default and a check of the answer, then "send, change a field, decline or cancel", so that the user can correct an answer before it goes out. The text of the server shows with the escapes of B15, each block line after the first indented, and each label, title and URL on one line, so that it cannot look like a question or a note of `ncp`. In URL mode `ncp` opens only an `https` URL (or `http` to a loopback host) and only after a yes; it shows the host and the URL, with a warning when the host has a Punycode label (`xn--`), because such a name can look like another name. Another URL is declined with a warning: a `file:` or a custom scheme starts a program, not a page. A multi-round request (`input_required`, 2026-07-28) gets the answers and runs again, with the request state as it came; at most 8 rounds, then the last `input_required` result goes to stdout with exit 5. A request for sampling or roots also gives that result with exit 5, before any question. A request of a legacy server (`elicitation/create`) gets the same questions. Each answer has the limit of `--timeout`; no answer in time gives exit 5, the end of stdin cancels the question, and Ctrl-C gives 130. With no TTY, `ncp` declares no elicitation, `input_required` gives exit 5 as before, and a legacy server that asks anyway gets the error `-32602` (Invalid params), as MCP 2025-11-25 requires for a mode that the client did not declare; the command then exits with 5. The fields come in name order, because the JSON library of rmcp keeps no key order. Each answer gets the checks of the schema: length, range, `minItems` and `maxItems`, and a light check of the `email`, `uri`, `date` and `date-time` formats; a required multi-select needs one value. A `oneOf` list with an item that has no `const` is refused, with a decline. | Requirement P9. The spec: a client shows which server asks, lets the user decline or cancel, and opens a URL only with consent. An agent with no TTY must not hang on a question, and an agent in a pseudo-terminal gets the time limit. |
| B64 | `tool show` gives each annotation hint as yes or no, and marks a value that the server did not set as "(default)", with the default of the spec: not read-only, destructive, not idempotent, open-world. For a read-only tool it leaves out the destructive and idempotent hints. `tool list` keeps the marks of B24. | An absent hint must not look safe: the spec defaults are the careful values. The spec gives the destructive and idempotent hints a meaning only for a tool that is not read-only. |
| B65 | A server error `-32042` (`URLElicitationRequiredError`, MCP 2025-11-25) gives exit 5. The text error lists each page with its URL and message; the JSON error has them in `pages` (`url`, `message`). Only a page that is safe to open (B63: `https`, or `http` to a loopback host) is in the list, with its URL in normal form; for another page the error gives only the reason that `ncp` refuses it. An error with no page is a protocol error (exit 3). On a TTY, `tool call`, `resource read` and `prompt get` first ask to open each page, only after a yes; a list command only gives the error. `ncp` does not run the request again: after a page opens, the hint says to run the command again when the page is done. | Requirement P9. The server needs the user in a browser before it can answer, which is input that `ncp` cannot give, so exit 5 fits. An automatic retry would need `notifications/elicitation/complete`, and the person knows when the page is done. An agent gets the URLs as data to show to its user. |
| B66 | The macOS store keeps a value in parts when its `security -i` command line does not fit 4095 bytes. It cuts the encoded value into slices, and each slice is an item under `ACCOUNT/GEN/`. No item account is another item account followed by `/`, also with an origin in the account (S9, secret-binding.md rule 4), so the cleanup of the parts of one item never removes another item. The item `ACCOUNT` (the head) names GEN and the number of parts. GEN is new for each save. The head and the parts are written with no second encoding, so a head cannot look like a stored value. A save writes each part in its own run and checks each exit code, then writes the head. After the head, it removes each other account under `ACCOUNT/` whose GEN is older than the limit of a save, also when the new value fits one line. A younger GEN can belong to a save of another process that has not written its head yet, so it stays until a later save. GEN starts with the time of the save. The whole save has one deadline, and it stops before the head when too little time is left. A failure before the head keeps the old value. A read that does not find a part reads the head again one time. Then the error is exit 4 and names the account. A delete removes the head and each account under `ACCOUNT/`. The token response stays as rmcp gives it, with `id_token`. | Requirement S7. One `add-generic-password -U` changes one item atomically, so the head is the commit point, and GEN keeps the parts of two saves apart. Readers do not hold the refresh lock, so the second read covers a save by another process. The deadline keeps the rule of the program limit: no item is written after `ncp` reports a failure. The cleanup after each save leaves no old token data behind, except for a short time the parts of a save that can still run. No spec asks a client to remove `id_token`. |
| B67 | The client of a login, in the order of the spec: `oauth.clientId` (B68), then the client item of the same issuer, then a new registration (DCR). When the metadata has no `registration_endpoint`, rmcp refuses to register, and `ncp` gives exit 4 with the hint to register a client and set `oauth.clientId`, `oauth.clientSecret` and `oauth.issuer`. The hint gives the `service_documentation` URL of the metadata when it passes the safe page rule of B63. CIMD comes with the first release (A7). rmcp selects the token authentication: HTTP Basic, or the request body when the metadata lists `client_secret_post` and not `client_secret_basic`. | MCP 2026-07-28 client registration: the order of the mechanisms. RFC 8414 §2: `service_documentation` tells how to register a client when there is no DCR, and with no list of methods the default is `client_secret_basic`. RFC 6749 §2.3.1: the body is NOT RECOMMENDED. |
| B68 | `oauth.clientSecret` is a literal value or a template with `${NAME}`, as a `headers` value. It needs `oauth.clientId` and `oauth.issuer`, else the entry is invalid (exit 2). `oauth.issuer` is the issuer of the authorization server where the user registered the client. It is optional with `oauth.clientId` alone. When `issuer` is absent, `ncp` reads `authorizationServer` (a string or an array of strings). When both are present and do not agree, the entry is invalid. `ncp` compares the issuer of the metadata with the configured value as an exact string: after discovery and before the browser opens, and before each token request that carries the secret. A mismatch stops with exit 4. The message says that `ncp` stopped on purpose, names both issuers and, with a secret, says that the secret was not sent. The hints have two branches. For an expected move: when the new server has DCR, remove the three keys and run `ncp auth login SERVER`; else register a client there, with the `service_documentation` URL when it passes B63. For an unexpected change: keep the configuration and tell the owner of the server. For a missing issuer, `auth login` adds the issuer that the server names. The store key of a stored client secret has the origin of `oauth.issuer` (S9), so a header at another origin cannot read it. `ncp` gets the secret at `auth login`, and at a refresh when the tokens have the client ID of `oauth.clientId`. No output shows the secret, also not an error of the expansion. `ncp` records no issuer at the first login. | MCP 2026-07-28 client registration, Authorization Server Binding: pre-registered credentials MUST be bound to their issuer, and a mismatch SHOULD show an error. The clients that take a secret bind it to an issuer that the user names, and no surveyed client records one at the first use ([mcp-client-oauth.md](../research/mcp-client-oauth.md)). `issuer` is the name of RFC 8414 §2; `authorizationServer` is the key of Claude Desktop. RFC 6749 §6: a confidential client authenticates at a refresh too. The official conformance scenario `auth/pre-registration` is the test target (C4). |
| B50 | When `SSL_CERT_FILE` is set, `ncp` trusts only the PEM roots in the file, on macOS and on Linux. A file that `ncp` cannot read, or that has no certificate, gives exit 3 with the path and the variable name, and so does an empty value. Only an http server reads the file, so a stdio server works with a bad value. When a TLS connection fails with an unknown issuer and the variable is set, the hint says that the file replaces the system roots and must hold each root that the user needs. | N4. uv 0.12 does the same on each OS: the variables "override the default certificate source entirely", and uv rejected a merge with the default roots (`docs/research/custom-ca-certificates.md`). On Linux the system store reads the variable in place of the system roots, so an addition there needs a change of the environment of `ncp` or its own search for the system roots. uv ignores an empty value; `ncp` cannot, because the Linux system store then loads no root. A file with only the root of a company is the common error, and uv gives no hint for it. |
| B69 | The HTTP client of the MCP connection follows no redirect. A `3xx` answer gives exit 3 with the status and the new location, and the hint to put the new URL in the entry when the user trusts it. | Requirement N6. The headers of an entry can hold credentials, and reqwest removes only the standard credential headers at a redirect to another host. rmcp also follows no redirect by default, because custom headers must not go to the target of a redirect. |
| B70 | Each stored token set has a binding: the canonical URL of the MCP server and the issuer, as an extra field next to the required scopes. A pure check compares the binding with the entry and the metadata before `ncp` sends a stored token and before a refresh. A different URL or issuer, or no binding, gives exit 4: "ncp did not send the stored token", says that the stop is on purpose, and gives the hint `ncp auth login SERVER`. The message names only the current value, never a stored URL or issuer: a stored value can hold an old `${NAME}` value that `ncp` cannot hide (S3). The server name is only the storage key. | Requirement A13. A changed entry or a hostile server must not get a token of another server. A token from before this change has no binding, and one new login is safer than a guess. |
| B71 | Small rules of the `oauth` object. `oauth.issuer` or `oauth.clientSecret` with no `oauth.clientId` makes the entry invalid. `authorizationServer` is a string or an array with one string. A `clientSecret` that has `${` but is not a valid `${NAME}` makes the entry invalid, and the error does not show the value. For a `clientSecret` with no issuer, `auth login` finds the metadata of the server, sends nothing to it, and gives exit 2 with the hint to add the issuer that the server names. Other commands give exit 2 with no hint. `ncp` gets the secret each time it uses the stored tokens of the configured client, and not only at a refresh. | An issuer with no client binds nothing, and the user can think that it does. A client is registered at one authorization server, so one issuer is enough; more issuers need a test target. An error with a part of a literal secret is a leak (S3). rmcp decides about a refresh after `ncp` gives it the client, so `ncp` cannot know in advance if a refresh comes. The cost is one store read for each use of the tokens, after a `401` answer. |
| B72 | `secret set NAME [--server SERVER]` finds the entries that name `${NAME}` in `env`, `headers` or `oauth.clientSecret` (with `--server`, only that entry). The targets are the different store keys of these entries. With no entry, `ncp` gives a usage error, and the message tells the user to add the entry first. A `${NAME}` only in `command`, `args`, `cwd` or `url` gives the hint of B32. With two or more targets, `ncp` gives a usage error that lists each server and its destination, with the hint `--server`. A pipe gives the value with no question. `ncp` never copies a stored value to a new destination. `secret delete NAME` with `--server` removes the item of that destination. With `--unused`, it removes the items that no entry of the loaded file uses. It gives a usage error when the file does not load or an entry is invalid. With `--all`, it removes all items. With no flag, it removes the one item of the name. When there are two or more items, it gives a usage error that lists the destinations. | [secret-binding.md](secret-binding.md) rules 6 and 7. One target for each write stops a name collision (P1). A copy makes the leak of a typo in a URL one key press. The user cannot get a value back through `ncp`, so a delete of more than one destination needs a flag. A script that changes a key gives `--server`, so a second entry with the same name does not break it. |
| B73 | `secret set` asks with hidden input only when stdin and stderr are a TTY, as for other questions. A TTY on stdin with no TTY on stderr is a usage error, with the hint to give the value on stdin from a file. Each read of the value has the limit of `--timeout`; with no value in time, the exit is 3. | With no TTY on stderr, the question does not show, and a read of the TTY with the echo on would show the value. An agent must not hang on a pipe that gives nothing (P15). |
| B74 | An error can have more than one hint, in the order to try them. Text: one `hint:` line for each. JSON: the error object keeps one `hint`, the first; each other hint follows as a hint note of B3, with the server of the error. `-q` keeps them, because they are part of the error. | Section 2.4 and B51 give two hints. The error object of section 4 keeps its shape, and a reader of B3 notes reads the other hints. |
| B75 | Small rules of the secret binding. (1) `secret set` gives a usage error when one of the entries that name `${NAME}` has no literal origin, also when other entries have one: the user then sees the entry that cannot use the value. (2) The miss error for a stdio value says "ncp did not start server NAME." (3) `secret delete --unused` with no unused item is a usage error with the hint `ncp secret list`, as B28. (4) The `file` field of `secret delete` is `null` when `ncp` read no configuration (`--all` and no flag), and only the JSON names it. (5) `secret list` marks an item as used only by a valid entry: `ncp` cannot know the destination of an invalid entry. (6) The text of a destination with a client secret is "the issuer of server NAME". | (1) Secure by default: a value that goes to only some of the entries is a surprise. (2) A stdio value goes to no network destination. (3) A silent success hides a typing error. (4) Section 4: the text result is one sentence for each destination. (5) The same rule as the lookup. (6) The question names the real destination of the value. |
| B76 | Small rules of the project file (milestone 18). (1) The question text is "FILE is not trusted. It has these servers:" (or "changed since you trusted it"), the shown entries, then "Trust? [y/N] ". Only `y` or `yes` trusts. The end of stdin or the time limit of `--timeout` is no answer: no record, and the error of the case with no TTY. (2) `ncp trust` text: "FILE (STATE)" and the shown entries. Its JSON is `{"file", "trust", "servers", "invalid"}`: an invalid entry goes in `invalid` with its error, as in `server list`. The shown entries use the rules of the loader with no expansion, so the entry that shows is the entry that runs; each word of the file (argument, `cwd`, `env` name, host) is a shell word, and in the text a URL shows in the form that the client sends (the JSON keeps the URL of the file, with its escapes). The checks of the loader run with no expansion: a `${NAME}` in the path or the query of a URL becomes a literal, and a `${NAME}` in the scheme or the host leaves only the URL rules out. An entry that fails a check shows its target and the error; an entry that the loader cannot read at all (name, shape, type) shows only the error. In JSON, an entry of a project file that the user checks also has `cwd`, `env` (the names) and `sends` (`[{"name", "host"}]`), because an agent reads the JSON of `ncp trust --show` (decision D). (3) `server list` text: a mark column, `project` for a trusted entry and `project, untrusted` or `project, ignored` for the others. The rows of a file that is not trusted come after the rows that run. (4) `ncp trust --untrust` keeps an ignored record; `--ignore` keeps a trusted record, because the ignored record wins. (5) A project file that `ncp` does not use (owner, not a regular file, larger than 1 MiB, not UTF-8, not the `mcpServers` shape) gives the warning "ncp does not use FILE: REASON" at each command that loads the configuration. (6) `secret delete --unused` stops with exit 2 when an entry of a project file that is not trusted names the secret or is invalid, and when `ncp` does not use the project file. (7) `secret list` with an unused row and `secret delete --unused` give the hint that a project in another folder can still use the item. | (1) mise asks the same question with No as the default; no answer must not write a record (design rule 4). (2) and (3): the state of the file shows in each place that shows its entries. (4) KISS: each option changes one record. (5) The design says "a warning"; one text for each reason. (6) Secure by default: `--unused` already stops for an invalid entry, because `ncp` cannot know which items it uses. (7) Design rule 9. |
| B77 | Small rules of `server add` and `server remove` (milestone 19). (1) The base of the edit of a project file is the bytes that the user trusted; the base of the global file, which needs no trust, is the bytes under the lock, so that two adds of agents at the same time both pass. Rule 3 runs again in the edit. (2) Rule 13 looks for the item in both stores; when the chosen store has the same value, `ncp` does not save it again. (3) The client item goes only with the tokens: with no tokens, it stays. The binding check compares the URL with no query, and the issuer only when the entry names one, because `server remove` contacts no server. (4) An http entry with an `Authorization` or `Proxy-Authorization` header gets no login step. (5) Text: "Added NAME to FILE." (first "Created FILE." for a new file), one line for each value, then "Next:" and one indented line for each step; a `setEnv` step shows as `export NAME=<value>`. (6) The temporary file is `.NAME.PID.tmp` in the folder of the target. (7) Shell completion offers server names for `server remove`, and not for `server add`, which takes a new name. (8) `--credential-store` with no `--secret` is a usage error of the parser. (9) An entry with no `${NAME}` gives no list of the stores, so `server add` shows no prompt of the native store. (10) With `--secret`, the steps of the value also run when the file has the same entry already, so that a retry with `--secret` stores the value. (11) The trust record of the new bytes is written inside the lock, and it removes an ignored record of an old file at the path. (12) An error about a `--header` or `--env` flag with no valid name gives the position of the flag, not its text. A header value with a control character and an empty command are usage errors. (13) A symlink to a missing file at the target gives exit 3, so the link stays. (14) Ctrl-C stops the wait for the lock with exit 130. (15) `server remove NAME --global` for a name that only the project file has names that file, with the hint `ncp server remove NAME`. The result says when OAuth tokens of the name stay because they are for another URL or issuer, or `ncp` cannot compare them, and its JSON has `tokensStay`. (16) A tab in a header value is permitted (RFC 9110 §5.5); other control characters are not. A word after `--secret` that is not a valid name gives an error that does not show the word (rule 12). (17) `server remove` tells if the global file has the name with the rule of the loader (the last copy of a key); a duplicate key then stops only an edit of the global file. (18) `--header`, `--env` and `--secret` take a value that starts with `-`, so that the parser does not show it in an error; a `${...}` that is not a valid reference gives an error with the field only. | (1) Design 4.7 protects the trusted bytes; a global file has no trusted bytes, and an exit 3 for a parallel add would make an agent retry. (2) Rule 13 says "a credential store". (3) and (4) KISS and the design (section 6, 4.6). (5) to (8) Good ergonomics and uv conventions. (9) Agents run `server add` with no person at the Keychain. (10) to (15) The adversarial review of milestone 19: rule 7 (a piped value is not lost), design 4.7 steps 6 to 9, rule 5 (never the value), section 5 (the link stays), C7 (nothing hangs) and section 6 ("say so"). |
| B78 | An answer `404` to the handshake of an http server gives the hint "check the path of the URL in the entry: the server has no MCP endpoint there, and many servers use /mcp". rmcp keeps only the text of the status, so the check reads the text `HTTP 404` of the pinned rmcp version; a test with a local server that answers `404` keeps it true. | Design server-add.md section 10: a URL with no `/mcp` must give the error and a hint at the first connect, because `server add` does not contact the server. Found in the interop test of milestone 19 with `https://docs.mcp.cloudflare.com`. |
| B79 | The scopes of `auth login` (changes B52, 2026-10-12). The initial selection: the `scope` of the `401` challenge of a probe with no token; else the `scopes_supported` of the protected resource metadata; else no `scope`. When the authorization server metadata lists `offline_access`, it is added to a selection that is not empty. The request is the union of the initial selection, the scopes that the last login requested, and the kept `403` scopes. The tokens item keeps the requested scopes (field `ncp_requested_scopes`, which rmcp ignores). The kept scopes count only when the binding of the stored tokens is the server and the issuer of the login. The granted scopes of the old tokens never go into the request. rmcp falls back to the `scopes_supported` of the authorization server, so `ncp` hides that list from the selection of rmcp. | MCP 2026-07-28, basic/authorization: "Scope Selection Strategy" (the priority order, and the earlier scopes in a re-authorization), "Step-Up Authorization Flow" step 2 (the union with the previously requested scope set), and "Refresh Tokens" (`offline_access`). The live test of 2026-10-12: a second login at Cloudflare asked for about 200 granted scopes in place of 3. A login that drops the scopes of a step-up makes the user log in again. |

[spec-tool-names]: https://modelcontextprotocol.io/specification/2025-11-25/server/tools#tool-names
[spec-timeouts]: https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle#timeouts
[spec-lifecycle]: https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle#initialization
[spec-versioning]: https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning#backward-compatibility-with-initialization-based-versions
