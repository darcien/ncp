# Design: command line

Status: accepted by the user on 2026-10-08 (plan milestone 5, gate).
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
- Choose in this order:
  1. The MCP spec.
  2. Secure by default. A common practice that is not secure is not a
     reference: many tools keep secrets in a plain file; `ncp` does not.
  3. Good ergonomics: the first try works, an error gives the exact fix,
     nothing hangs.
  4. Common practice of well-known tools, uv first.
  5. The simplest design.
- The network commands use the verbs of the MCP methods (`list`, `call`,
  `read`, `get`).
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
ncp generate-shell-completion <bash|zsh|fish>      (after the MVP)
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
| `generate-shell-completion` | No | After the MVP. The completion script for the shell. Server names come from the configuration. Tool names come only from a cache, or are not completed. Completion never starts a server. |

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
- An unknown server or tool name gives a usage error. The MVP points to
  `ncp server list` or `ncp tool list SERVER`. Later: the near names, for
  example "no tool `search_issue`; did you mean `search_issues`?".
- `tool list` prints names that the user can copy into `tool call`.

### 2.2 Tool arguments (I4)

| Form | Meaning |
|---|---|
| `key=value` | The value is text. When the input schema gives the property type `number`, `integer`, `boolean`, `array` or `object`, `ncp` reads the value as JSON. A schema type `string` keeps the text, also for `123`. |
| `key:=json` | Later. The value is JSON, with no schema rule. |
| `key=@path` | Later. The value is the text of the file. `@@` at the start is a literal `@`. |
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
| `--output-format <text\|json>` | Later. Output format. Default: `text` on a TTY, else `json`. |
| `--color <auto\|always\|never>` | Later, with color. Color in text output. Default `auto`. `NO_COLOR` also turns color off (I7). The MVP has no color. |
| `-q`, `--quiet` | Later. No hints and no warnings on stderr. Errors stay. |
| `-v`, `--verbose` | Diagnostics on stderr: the source of each variable (environment, native store, file; never the value), the proxy route and its source, the cached era. `-vv` adds the JSON-RPC messages and the HTTP requests, with secrets hidden (I6). |
| `--timings` | Timings on stderr (F2): process start, configuration, credential read, server start, DNS, connect, server response, CPU time. |
| `--config-file <path>` | Later. Configuration file, as uv names the flag. A missing file named here is a usage error. Default in section 6. |
| `--timeout <seconds>` | The time limit for one request, in whole seconds. Default 300 s (decision C7). A timeout sends a cancel request and exits with code 3. Progress notifications do not reset the clock. |
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
| C3 | Probe timeout 10 s, the rmcp value, with no option. The era cache keeps any result for the server configuration, and probes again only when the cached path fails. No command clears the cache in the MVP; the user can remove `~/.cache/ncp`. This replaces the advice of early-tests.md to keep the cause of a legacy result. | The MCP spec permits a client to keep the result for the same server configuration ([versioning][spec-versioning]). A server that does not answer `server/discover` then costs the timeout once, not at each call. A changed server repairs itself, because a failure causes a new probe. Consequence: a server for both eras that is slow at its first start (for example an `npx` download) stays legacy until the cache is removed. It works, with the legacy protocol. |
| C4 | No settings file. Settings for one server go in the `oauth` object of the entry. Only the methods of the MCP spec; only `clientId` in the MVP. | Other tools already use an `oauth` object, so no new key. A field without a test target is not built. |
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
| B5 | `tool call` reads the tool list first only when it needs the input schema: for `key=value` arguments, for the schema check, and for `--help`. With `--no-validate` and a JSON object, it sends only `tools/call`. | One request less, and a tool that the list does not show stays callable. |
| B6 | When `ncp` cannot use an input schema (for example a `$ref` to another document), it shows a warning and sends the call with no check. | Secure by default: the check resolves no reference over the network or from a file (N1). A bad schema from a server must not block a correct call. |
| B7 | When no server is configured, `server list` writes the message as a hint on stderr, with the path. stdout has the empty list in JSON mode and nothing in text mode. | stdout has only the result, so a script reads the same shape in all cases. |
| B8 | Text output lists object keys in name order, for example the arguments in `tool show`. | serde_json keeps keys in name order; the JSON output is the same MCP object. |
| B9 | An environment variable that is not UTF-8 is not a `${NAME}` value, and a stdio server does not get it. | A JSON configuration value is UTF-8 text. |
| B10 | A `401` or `403` from an HTTP server gives exit 4 with the message "the server needs a login or more scopes". The hint `ncp auth login` comes with that command. | Hints name only commands that exist. |
| B11 | `tool call` writes the `tool` and `input_required` error objects on stderr too, after the result on stdout. | Each exit code has one `code` value (section 4); a caller that reads only stderr sees why the code is not 0. |
| B12 | A result with a task (`CreateTaskResult`) gives exit 3. | `ncp` does not ask for tasks, so the server breaks the protocol. |
| B13 | Output and the era cache use the target as the file writes it, with `${NAME}` not expanded. | Secure by default: an expanded value can be a secret, and must not go to a plain file or to the screen (S2, S3). A changed variable value can make the cached era wrong; then the handshake fails and `ncp` probes again. |
| B14 | In JSON mode, each line that a stdio server writes on stderr becomes `{"serverLog": {"server", "message"}}` on the stderr of `ncp`. In text mode the line goes through, with control characters as escapes (B15). A line that is not UTF-8 shows with replacement characters, and a line longer than 64 KiB comes in parts. | stderr stays JSON Lines (section 4), and the server log stays visible for debugging. |
| B15 | Text output shows each control character, except new line and tab, as an escape such as `\u{1b}`. | Secure by default: a server or a configuration value can hold terminal escape sequences that change the title, write the clipboard, or hide lines. |
| B16 | For `key=value`, `ncp` finds the property types also in `anyOf`, `oneOf` and `allOf`, in a `$ref` into the same schema, and in `enum` and `const`. `tool show` shows the same types. | Good ergonomics: tool SDKs write an optional number as `anyOf` with `null`, so the first try must work. |
| B17 | A `tools/call` error "invalid params" (-32602) is a usage error (exit 2) with the hint `ncp tool show SERVER TOOL`. | The spec uses this code for an unknown tool and for bad arguments. Both are usage errors (section 3). |
| B18 | An era cache that cannot be read or written gives a warning, and the command continues. | Section 6: `ncp` works the same with no cache. A read-only cache directory must not stop a call that works. |
| B19 | An entry with type `sse` is a usage error (exit 2) that names the configuration file. An invalid entry also names the file. | The fix is in the file. |
| B20 | `tool call --help` with no tool gives the help of the command. With a tool, it gives the output of `tool show`. | The design gives `tool call SERVER TOOL --help`; the other forms keep the normal help. |
| B21 | A parse error of the command line gives the JSON error object in a pipe, with the hint `ncp --help`. No command gives the message "missing command". | stderr is JSON Lines in a pipe. |
| B22 | No `HOME` and no XDG variables, or a configuration file that cannot be read, gives exit 3. A file that does not parse gives exit 2. | Section 3: a local file that `ncp` cannot read is a local error; a bad file is a usage error. |
| B23 | A word in the place of the tool that contains `=`, starts with `{`, or is `-`, gives "missing tool name". | These are the argument forms of section 2.2. |
| B24 | `tool list` shows an annotation mark only for a hint that the server sets to true. | A mark for each default value would mark almost all tools. |
| B25 | The JSON of `tool list` and `tool show` is the `Tool` object as rmcp reads it. A field that rmcp does not know is not in the output. | rmcp gives typed objects only. Known limit; see the backlog. |
| B26 | When `-` reads the arguments from a TTY, a hint says to end the input with Ctrl-D. | Good ergonomics: nothing seems to hang. |
| B27 | `secret set` writes `{"name", "place"}`, and `secret delete` writes `{"name", "places"}`. Text: one sentence, for example "Stored GITHUB_TOKEN in the native store." | Section 4 gives no shape for these commands; the names follow `secret list`. |
| B28 | `secret delete` of a name that no place has is a usage error (exit 2) with the hint `ncp secret list`. | The user can have a typing error in the name; a silent success hides it. |
| B29 | `secret set` does not keep one new line at the end of the input (LF or CRLF). An empty value is a usage error. | `echo` and a pasted line add a new line, which is never part of a token. |
| B30 | When a store cannot answer, `secret list` shows a warning and lists the other place. `secret delete` removes the copies that it can reach, and fails only when it removed none. | Good ergonomics: one broken place does not hide the other. |
| B31 | A native store that cannot answer does not stop the search for `${NAME}`: the file can have the name. When no place has it, the error gives the store error (exit 4) and the hint `ncp secret set NAME --credential-store file`. | Section 2.4, with the order of the places kept. |
| B32 | A `${NAME}` in `command`, `args`, `cwd` or `url` comes only from the environment. `ncp` does not read a store for it. The error is "variable NAME is not set" with the hint `export NAME=<value>`. | Section 2.4: a stored value must not go into the process list or a URL. Not reading the store also shows no prompt. |
| B33 | A credential file that `ncp` cannot read or write gives exit 3. A native store that is not available, is locked, or gives no answer gives exit 4. | Section 3. |
| B34 | Items of the earlier layout (`<server>/env/<NAME>`) are ignored. | No release has used them (requirements: stored credentials can reset before the first release). |
| B35 | After `secret set`, `ncp` checks the other place for a copy, with a limit of 5 s, and warns when the environment sets the name. Each warning names the place that wins. When a copy is found, the hint is `ncp secret delete NAME` (section 2.4): it removes all copies, and the user then stores the value in one place. When the check fails, the warning says so and gives no hint. | The check only gives a warning, so it must not wait for a locked store. |
| B36 | A host with no native store (for example Linux with no Secret Service) gives no warning in `secret list` and `secret delete`: the native store counts as empty. | A user who keeps all values in the file must not see the same warning at each run. |
| B37 | `secret list` with a configuration file that does not load gives a warning, and no secret shows as used. | The names come from the stores, not from the file. |
| B38 | On Linux, the list of names reads no value and unlocks nothing. A locked item gives the warning "unlock the keyring to list them". `secret delete` removes the item in one call, which can show the unlock prompt, as `secret set` does. | keyring-core unlocks each item that a search finds, which shows a prompt. The design says that the list reads no value. |
| B39 | `secret set` checks the store before it reads the value. On a TTY, it turns off the echo and the line mode with `/bin/stty`, and does the backspace itself, so that a value longer than 1024 bytes works on macOS. It restores the earlier settings after the line and after Ctrl-C (exit 130). When `stty` fails, `secret set` fails and reads no value. | The user does not type a secret twice, and the shell of the user works after Ctrl-C. The value never shows. |
| B40 | Each run of `/usr/bin/security` stops 1 s before the limit of its store call. `secret delete` removes and reports in one run. Ctrl-C stops the program too, because it stays in the process group of `ncp`, and gives exit 130. | A program that stays can show a dialog after the exit of `ncp`, or write or remove an item late. |
| B41 | `-v` writes `debug: server NAME: message` in text and `{"debug": {"message", "server"}}` in JSON. It shows the configuration file, the source of each `${NAME}` of `env` and `headers` ("from the environment", "from the native store", "from the credential file"), the proxy route of an HTTP server with its source, the cached era, and the era that `ncp` saves. | The same shape as warnings and hints (B3). uv also writes `DEBUG` lines for `-v`. |
| B42 | `-vv` writes each JSON-RPC message as `{"trace": {"server", "sent": message}}` or `{"trace": {"server", "received": message}}`, and each HTTP request as `{"trace": {"server", "http": {"method", "url", "headers", "outcome", "ms"}}}`. Text: `trace: server NAME: sent {...}`. The headers are the ones that `ncp` gives; reqwest adds `accept`, `content-type` and `content-length`. The outcome is the kind of response (`202 accepted`, `JSON response`, `event stream`) or the error, because the rmcp HTTP client gives no status code for a success. The message bodies are not changed. | The user gave the arguments and gets the results on stdout. The secrets of the connection are in the headers and the URL, and these are hidden (B43). |
| B43 | `-vv` hides a URL query value when the parameter name contains `token`, `secret`, `password`, `passwd`, `key`, `signature` or `credential`, or is `code`, `sig`, `pwd` or `auth`. A text that is not a URL is hidden as a whole. | Secure by default: to hide too much is safe, and to show a key is not. |
| B44 | `--timings` writes one line at the end of the run, also after an error. Times are in milliseconds with one decimal, because a local step takes less than 1 ms. A step that did not occur is absent. `dnsMs`, `connectMs` and `serverStartMs` add up all occurrences, for example a second start after a probe timeout. `serverStartMs` is the time to start the process; the handshake time is in `responseMs`. `totalMs` starts when `main` starts: `std` gives no start time of the process. `cpuMs` is the CPU time of `ncp` only, not of a stdio server or `/usr/bin/security`. `connectMs` includes the DNS time of a new connection, and the first `responseMs` includes the connect. `credentialsMs` shows only when a store is read. | F2 asks for CPU time and network time apart; the run with the error is often the one to measure. |
| B45 | `auth login` writes `{"server", "issuer", "scopes", "place"}`, and in text "Logged in to SERVER at ISSUER. The tokens are in PLACE.". The URL to open goes to stderr: text lines for a person, `{"login": {"server", "url"}}` in JSON. `auth logout` writes `{"server", "places"}`. `auth status` text is a table: name, state, issuer, scopes, expiry. | The same names as `secret set`, `secret delete` (B27) and the `auth status` shape of section 4. An agent can give the URL to a person. |
| B46 | `auth login` always listens on `127.0.0.1` with a random port. It also reads a pasted redirect URL from stdin when stdin is a TTY or with `--no-browser`. The first redirect wins. The wait has the limit of `--timeout` (default 300 s), and Ctrl-C gives 130. On macOS the browser is `/usr/bin/open`; on Linux it is `xdg-open`. When the browser cannot open, a warning shows, and the login waits for the paste. | Section 5: whichever comes first. A listener with `--no-browser` costs nothing and helps when the browser is on the same host. Nothing waits forever (C7). |
| B47 | The client item goes to the place of the tokens (`--credential-store`). A login reads the client item only from that place, and uses it only when its issuer is the current issuer. With no usable item, `ncp` registers the client itself (DCR) and keeps the item before the login continues. With `oauth.clientId`, `ncp` keeps no client item. | Requirement A5. rmcp keeps a registered client secret only in memory (rmcp issue #784). A registration that a later step loses is not done again. |
| B48 | `ncp` refuses an authorization server endpoint that does not use HTTPS, except on a loopback host. A missing `issuer` in the metadata falls back to the origin of the authorization endpoint. | The spec: all authorization server endpoints MUST use HTTPS (MCP 2025-11-25, basic/authorization, "Communication Security"). rmcp does not check it. Local tests need loopback. |
| B49 | `auth status` reads the native store and then the file for each http server. `auth logout` removes the tokens from both places and keeps the client item; a native store that gives no answer gives a warning, and the file copy still goes away. | Design 2.4 and the `auth logout` row of section 2. |
| B50 | When `SSL_CERT_FILE` is set, `ncp` trusts the PEM roots in the file in addition to the system store. A file that `ncp` cannot read, or that has no certificate, gives exit 3 with the path and the variable name, and so does an empty value. Only an http server reads the file, so a stdio server works with a bad value. On Linux the system store reads `SSL_CERT_FILE` (and `SSL_CERT_DIR`) in place of the system roots, so there the file replaces the system roots, as in OpenSSL; an empty value there loads no root. `ncp` cannot change this, because it does not change its own environment. | N4, with the variable name of curl, OpenSSL and uv. A silent fallback gives a TLS error that does not name the cause. |

[spec-tool-names]: https://modelcontextprotocol.io/specification/2025-11-25/server/tools#tool-names
[spec-timeouts]: https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle#timeouts
[spec-lifecycle]: https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle#initialization
[spec-versioning]: https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning#backward-compatibility-with-initialization-based-versions
