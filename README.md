# ncp

`ncp` is a command-line client for the Model Context Protocol (MCP). It
lists and calls the tools, resources and prompts of MCP servers. It is one
static binary for macOS and Linux. People and agents use the same commands:
the output is text on a terminal and JSON in a pipe.

`ncp` speaks the protocol versions `2026-07-28` and `2025-11-25`. It finds
the version of each server itself. It connects to stdio servers and to
Streamable HTTP servers.

Status: before the first release. The behavior can change.

## Build

You need Rust (the version is in `rust-toolchain.toml`).

```
cargo build --release -p ncp
```

The binary is `target/release/ncp`. `mise run release` builds the binaries
for macOS and Linux, x86_64 and arm64, into `dist/`, with `SHA256SUMS`.

## Configure

`ncp` reads the `mcpServers` map from `~/.config/ncp/mcp.json`
(`$XDG_CONFIG_HOME/ncp/mcp.json`). Other agent tools use the same shape.

```json
{
  "mcpServers": {
    "files": {
      "command": "mcp-server-files",
      "args": ["--root", "/srv/data"],
      "env": { "API_TOKEN": "${FILES_TOKEN}" }
    },
    "docs": {
      "type": "http",
      "url": "https://mcp.example.com/mcp",
      "headers": { "Authorization": "Bearer ${DOCS_KEY}" }
    }
  }
}
```

- An HTTP entry must have `"type": "http"`.
- `${NAME}` comes from the environment. In `env` and `headers`, it can
  also come from the credential store (see Secrets).
- An unknown key gives a warning, not an error.
- `--config-file PATH` reads another file.

## Use

```
ncp server list
ncp server show SERVER
ncp tool list SERVER
ncp tool show SERVER TOOL
ncp tool call SERVER TOOL key=value ...
ncp resource list SERVER
ncp resource read SERVER URI
ncp prompt list SERVER
ncp prompt get SERVER PROMPT key=value ...
```

Tool arguments:

| Form | Value |
|---|---|
| `key=value` | Text. When the input schema gives another type, `ncp` reads the value as JSON. |
| `key:=json` | JSON, with no schema rule. |
| `key=@path` | The text of the file. `@@` at the start is a literal `@`. |
| `{...}` | The whole arguments object as JSON. |
| `-` | The arguments object as JSON from stdin. |

`ncp` checks the arguments against the input schema before the call.
`--no-validate` turns the check off.

### Output and exit codes

Results go to stdout, and errors, warnings and hints go to stderr. On a
terminal the output is text. In a pipe stdout is one JSON value, and
stderr is JSON Lines. `--output-format text|json` chooses the format.

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | The tool returned an error (`isError`). The result is on stdout. |
| 2 | Usage error: bad arguments, unknown server or tool, bad configuration |
| 3 | Protocol, transport or local error, or a timeout |
| 4 | The server needs a login or more scopes, or the credential store cannot answer |
| 5 | The server asks for more input (`input_required`) |
| 130 | Ctrl-C. `ncp` sent the cancel request first. |

## Secrets

A secret is a named value for `${NAME}`. `ncp` reads it from the first
place that has it: the environment, the native store (the macOS Keychain
or the Linux Secret Service), then the opt-in credential file.

```
ncp secret set FILES_TOKEN          # reads the value from stdin
ncp secret list                     # names and places, never values
ncp secret delete FILES_TOKEN
```

With no native store, add `--credential-store file`. The file has mode
0600. A value never goes into a process argument.

## OAuth

For an HTTP server that needs a login:

```
ncp auth login SERVER               # opens the browser
ncp auth login SERVER --no-browser  # prints the URL; paste the redirect URL
ncp auth status
ncp auth logout SERVER
```

`ncp` refreshes the tokens with no user action. When a server asks for
more scopes, the error says so, and the next login asks for them.

## Diagnostics

| Option | Effect |
|---|---|
| `-v` | Where each value comes from (never the value), the proxy route, the era. |
| `-vv` | Also each JSON-RPC message and each HTTP request, with credentials hidden. |
| `--timings` | The time of each step, and the CPU time. |
| `-q` | No hints and no warnings. |
| `--timeout SECONDS` | The limit for one request (default 300). |
| `--no-cache`, `--refresh` | Go around the caches of the era and of the tool lists. |

Proxy: `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY` and `NO_PROXY`, then the
macOS system proxy. A local server through a remote proxy gives a hint to
set `NO_PROXY`. `SSL_CERT_FILE` adds root certificates.

## Shell completion

```
ncp generate-shell-completion bash > ~/.local/share/bash-completion/completions/ncp
ncp generate-shell-completion zsh  > "${fpath[1]}/_ncp"
ncp generate-shell-completion fish > ~/.config/fish/completions/ncp.fish
```

Server names complete from the configuration. Completion starts no server.

## Development

The rules for contributors and agents are in `AGENTS.md`. The design is in
`docs/design/cli.md`, the requirements in `docs/target-requirements.md`,
and the research in `docs/research/`.

```
hk install --mise      # install the git pre-commit hook (one time)
mise run precommit     # format, checks and tests on the host and in Linux
mise run conformance   # the official MCP conformance suite
```

## License

MIT or Apache-2.0, at your choice.
