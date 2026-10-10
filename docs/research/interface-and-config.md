# Study: interface and configuration of MCP CLI clients

Status: research, 2026-10-06. Input for [requirements.md](../requirements.md)
section 3.5 (I1 to I8) and open item 1 (I8).

Method: read-only study of READMEs and docs through `gh api` (contents API,
GraphQL), WebSearch and WebFetch. No code was cloned, installed or run. All
behavior below comes from documentation, not from tests. "Unknown" means the
docs do not state it.

The tools in Part A use JavaScript, TypeScript, Python or Bun. They fail F1.
This study uses them for ideas only.

## Part A: CLI interface patterns

### A.1 Shortlist

Stars and push dates come from the GitHub API on 2026-10-06. The "MCP" column
comes from the comparison table in the apify/mcpc README (a third-party claim,
2026-09-23).

| Tool | Lang | Stars | Last push | MCP | Deep read | Kind |
|---|---|---:|---|---|---|---|
| [openclaw/mcporter][mcporter] | TS | 5049 | 2026-10-01 | 2026-07-28 | Yes | Client + CLI generator + daemon |
| [knowsuchagency/mcp2cli][mcp2cli] | Python | 2413 | 2026-09-12 | 2025-11-25 | Yes | MCP/OpenAPI/GraphQL to CLI, at runtime |
| [IBM/mcp-cli][ibm] (was chrishayuk/mcp-cli) | Python | 2025 | 2026-08-05 | 2025-06-18 | Partial (config, tokens) | LLM chat client with a command mode |
| [f/mcptools][mcptools] | Go | 1626 | 2025-12-18 | 2024-11-05 | Partial | Client, shell, proxy, guard |
| [philschmid/mcp-cli][phil] | TS (Bun) | 1276 | 2026-02-05 | 2025-11-25 | Yes | Agent-first client |
| [apify/mcpc][mcpc] | TS | 976 | 2026-10-06 | 2026-07-28 | Yes | Full-spec client with sessions |
| [wong2/mcp-cli][wong2] | JS | 446 | 2026-06-08 | 2025-11-25 | Partial | Inspector |
| [lydakis/mcpx][mcpx] | Go | 53 | 2026-09-29 | 2026-07-28 | Yes | Unix-composable client (small project) |
| [thellimist/clihub][clihub] | Go | 666 | 2026-03-12 | 2025-11-25 | No | Code generator: MCP tools to a CLI binary |

Note: lydakis/mcpx is Go but small and single-author. It fails C1. It is in
the study because its interface is close to the target.

### A.2 Command structure and tool naming

| Tool | Shape | Server and tool selector |
|---|---|---|
| mcpc | `mcpc @session tools-call <tool> ...` | Server = named session `@name`, made first with `mcpc connect <url \| file:entry> @name`. Verb-noun subcommands: `tools-list`, `tools-get`, `tools-call`, `resources-read`, `prompts-get`. |
| mcporter | `mcporter call server.tool ...` | `server.tool`. Also a URL selector: `mcporter call 'https://host/mcp.tool'`. Also `--server` and `--tool`. |
| mcp2cli | `mcp2cli --mcp <url> <tool> --flag v` | Source flag (`--mcp`, `--mcp-stdio "cmd"`) or a saved "baked" name: `mcp2cli @name <tool>`. |
| philschmid/mcp-cli | `mcp-cli call <server> <tool> [json]` | Two words, or `server/tool`. Subcommands: `info`, `grep`, `call`. |
| lydakis/mcpx | `mcpx <server> <tool> --flag=v` | Two positional words. `mcpx` lists servers. `mcpx <server>` lists tools. A URL works in place of a server name. |
| wong2/mcp-cli | `mcp-cli call-tool server:tool --args '{}'` | `server:tool`. |
| f/mcptools | `mcp call <tool> --params '{}' <server command...>` | The server command or an alias comes last. |

Naming hazard: [SEP-986][sep986] (Final) permits `.` and `/` in tool names, for
example `admin.tools.list`. A `server.tool` selector must split at the first
`.` only, and server names must not contain `.`.

### A.3 Tool arguments

| Tool | JSON | key=value | Flags from inputSchema | stdin | File |
|---|---|---|---|---|---|
| mcpc | Positional, when the first argument starts with `{` or `[` | `key:=value`. Valid JSON keeps its type, else string. `id:='"123"'` forces a string. | No | Yes, when no arguments and stdin is a pipe | Unknown |
| mcporter | `--args '{...}'`, `--json '{...}'`; also function-call syntax `'srv.tool(a: 1, b: "x")'` | `key=value`, `key:value`, `key:=value` | Yes, `--issue-id X` maps to `issueId`. Unknown flags fail before the call. | `--args -` | `key=@path` reads a file as a string; `@@` escapes |
| mcp2cli | Unknown for MCP; `--stdin` for OpenAPI bodies | No | Yes, each tool is a subcommand with generated `--flags` and `--help` | OpenAPI only | `env:` and `file:` prefixes for secret options |
| philschmid | Positional JSON | No | No | Yes, automatic | No |
| lydakis/mcpx | Positional or stdin JSON for complex schemas | No | Yes, `--key=value`, with `--help` per tool | Yes | Unknown |

Coercion rules in mcporter, worth a note:

- Values coerce to the schema type (numbers, booleans, enums).
- A numeric-looking value stays a string when the schema field is a string (for example a Slack `thread_ts`).
- `--raw-strings` stops numeric coercion. `--no-coerce` stops all coercion.
- A bare string for an array field becomes a one-item array.

### A.4 Output, errors and exit codes

| Tool | Default output | Machine output | Errors | Exit codes |
|---|---|---|---|---|
| mcpc | Markdown-like text with colors; `--max-chars N` | `--json` (or `MCPC_JSON=1`): one JSON object or array. MCP commands return MCP-spec objects. | stdout on success, stderr on error | 0 ok, 1 client, 2 server (tool failed), 3 network, 4 auth. `grep`: 0 match, 1 no match. |
| mcporter | Text | `--output text\|markdown\|json\|raw` | JSON mode prints `{ server, tool, issue }` for auth, offline and HTTP failures | `list --exit-code`: 1 when a server is unhealthy. Others: Unknown. |
| mcp2cli | Human text | `--json`: valid JSON for every command; tool calls give the full `CallToolResult` (`content`, `structuredContent`, `isError`). Pretty on a TTY, compact in a pipe. `--toon`, `--head N`, `--raw`. | Unknown | Unknown |
| philschmid | Text for lists; JSON for `call` | `call` output is always JSON | Typed codes with a suggestion, for example `Error [TOOL_NOT_FOUND]` plus available tools | 1 client, 2 server, 3 network, 4 auth (source `src/errors.ts`) |
| lydakis/mcpx | Tool output passes through unchanged | `--json` for mcpx-owned output only; `--catalog` for full descriptors | Unknown | 0 ok, 1 tool error (`isError`), 2 usage, 3 internal. `input_required`: JSON result and exit 1. |
| f/mcptools | Table | `--format json\|pretty` | Unknown | Unknown |

Example of an actionable error (philschmid/mcp-cli):

```
Error [SERVER_NOT_FOUND]: Server "github" not found in config
  Details: Available servers: filesystem, sqlite
  Suggestion: Use one of: mcp-cli info filesystem, mcp-cli info sqlite
```

`input_required` (P8) in lydakis/mcpx: the call returns the JSON result with
exit 1. The user resumes with `--request-state` and `--input-responses`.
`--interactive` turns on TTY elicitation.

### A.5 Tool discovery and help for agents

| Pattern | Tools |
|---|---|
| Built-in agent guide that matches the installed version: `mcpc help --skill` prints a valid `SKILL.md` | mcpc |
| `skill install` writes a general or per-server skill file | lydakis/mcpx, mcp2cli |
| Search across all servers: `grep <pattern>` (tools, instructions; `--resources`, `--prompts`; `-E` regex) | mcpc, philschmid |
| `--search PATTERN`, `--top N`, `--compact` (names only), ranking by local use count | mcp2cli |
| Compact list first, full schema on request: `tools-list` then `tools-get`; `list --brief` then `--schema` | mcpc, mcporter |
| TypeScript-like signatures with `@param` lines; optional parameters folded to `// optional (8): ...` | mcporter |
| Tool annotations next to each tool: `read-only`, `destructive`, `idempotent`, `open-world` | mcpc |
| Schema snapshot check: `tools-call --schema expected.json` fails when the schema changed | mcpc |
| Near-miss names auto-corrected, with suggestions | mcporter |
| `doctor` command: config, protocol, transport, redacted auth | lydakis/mcpx, mcporter (`config doctor`) |
| Shell completions: `mcpx completion bash\|zsh\|fish` | lydakis/mcpx |

### A.6 Authorization UX

| Tool | Login | No local browser | Token store | Other |
|---|---|---|---|---|
| mcpc | `mcpc login <server> [--profile work]`; `logout`. OAuth starts only from `login`. | Unknown; `--callback-host`, `--callback-port` exist | OS keychain. Headless Linux: `~/.mcpc/credentials.json` (0600). | Named profiles per server. CIMD (hosted document), then DCR. `--client-id` for pre-registered clients. Client-credentials grant. Static header with `--header`, kept in the keychain per session. |
| mcporter | `mcporter auth <name>` or `config login`; `logout` | `--no-browser` prints the URL. With `--json`: `{ authorizationUrl, redirectUrl }`. | `~/.mcporter/credentials.json` (plain file vault) | A changed server URL clears its tokens. `vault set --stdin` seeds tokens. `auth: refreshable_bearer` injects a fresh token into a stdio env var. |
| mcp2cli | `--oauth` per call | `--oauth-manual-callback`: prints the URL, reads the pasted redirect URL from stdin | `~/.cache/mcp2cli/oauth/` (files) | `env:VAR` and `file:path` prefixes keep secrets out of `ps`. |
| lydakis/mcpx | `mcpx auth login\|status\|logout <server>` | Login always prints the URL. It never opens remote URLs. | OS credential store | `--oauth-client-metadata-url` for CIMD |
| IBM/mcp-cli | `mcp-cli token set <name> --type bearer` (hidden input) | n/a | Keychain, Secret Service, encrypted file or Vault (`--token-backend`) | Config refers to a stored token with `${TOKEN:bearer:name}`. Logs redact secrets. |

mcp2cli `--oauth-manual-callback` and mcpx "print, never open" match A10.

### A.7 Daemon, connection reuse and caching

| Tool | Model | Socket and lifetime | Cache |
|---|---|---|---|
| mcpc | One bridge process per named session. Explicit `connect`, `restart`, `close`. | Unix sockets in `~/.mcpc/bridges/`. Keepalive pings, auto-reconnect. Session states: `live`, `disconnected`, `unauthorized`, `expired` and more. | Tool lists cached in the bridge. `list_changed` refreshes them. |
| mcporter | One single-user daemon. Opt-in per server: `"lifecycle": "keep-alive"`. | `~/.mcporter/daemon/user.sock`, mode 0600 in a 0700 directory. HMAC challenge before the config is sent. Idle timeout per server. | Schema cache in `$XDG_CACHE_HOME/mcporter/<server>/schema.json` |
| philschmid | Lazy spawn: the first call forks one daemon per server. | `/tmp/mcp-cli-{uid}/<server>.sock`. 60 s idle timeout. A config change starts a new daemon. After a 5 s spawn timeout, it connects directly. `MCP_NO_DAEMON=1` turns it off. | None documented |
| mcp2cli | Per call; a "session daemon" exists (`--session`), details Unknown | Unknown | Tool lists in `~/.cache/mcp2cli/`, 1 h TTL, `--refresh`, `--cache-ttl` |
| lydakis/mcpx | Daemon holds ephemeral sources | Unknown | Tool-response cache: `--cache=5m`, `--no-cache`, `default_cache_ttl` per server |

### A.8 MCP-to-CLI tools for agents

These tools turn MCP tools into shell commands. The goal is fewer tokens in the
agent context: the agent reads `--help` only when it needs a tool.

| Tool | Method |
|---|---|
| mcp2cli | Runtime. Each tool becomes a subcommand with generated flags. `bake create` saves a source; `bake install` writes a wrapper to `~/.local/bin/<name>`. Claims 96 to 99 % token saving (author claim, not verified). |
| mcporter `generate-cli` | Writes a TypeScript CLI for one server; can compile with Bun. `--include-tools`, `--exclude-tools`. `emit-ts` writes typed clients. |
| lydakis/mcpx `shim install <server>` | Writes a passthrough so `github search-repositories --query=mcp` works. It fails when the name already exists in `PATH`. |
| thellimist/clihub | Generates standalone CLI binaries from MCP tools (not deep read). |
| mcporter `serve` | The reverse: exposes daemon-held servers as one MCP server, tools named `server__tool`. |

### A.9 Config files of these CLIs

| Tool | Own file | Reads other clients |
|---|---|---|
| mcpc | No own server file. `~/.mcpc/sessions.json`, `profiles.json`. | Any `mcpServers` file by `path:entry`. `mcpc connect` with no argument finds `.mcp.json`, `.cursor/mcp.json`, `.vscode/mcp.json`, `~/.claude.json`, Claude Desktop, Windsurf, Kiro. It does not expand `${VAR}` in project-directory files until the user names the file. |
| mcporter | `config/mcporter.json` (project) and `$XDG_CONFIG_HOME/mcporter/mcporter.json[c]` or `~/.mcporter/mcporter.json` (user). JSONC, `$schema`. | Imports Cursor, Claude Code, Claude Desktop, Codex (TOML), Windsurf, OpenCode, VS Code. Imports are read-only; local entries win. |
| philschmid | `mcp_servers.json`: `MCP_CONFIG_PATH`, `-c`, `./`, `~/.mcp_servers.json`, `~/.config/mcp/` | No |
| lydakis/mcpx | `~/.config/mcpx/config.toml`, `[servers.<name>]` | `mcpx import claude\|cursor\|codex\|cline\|kiro`: redacted preview first, then explicit import with provenance. |
| IBM/mcp-cli | `server_config.json` (`mcpServers`) | Unknown |

lydakis/mcpx TOML shape:

```toml
[servers.github]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
env = { GITHUB_TOKEN = "${GITHUB_TOKEN}" }
default_cache_ttl = "30s"
```

## Part B: configuration formats (I8)

### B.1 Formats by client

**Claude Desktop** ([doc][cd]). macOS:
`~/Library/Application Support/Claude/claude_desktop_config.json`. Windows:
`%APPDATA%\Claude\claude_desktop_config.json`. The doc lists macOS and Windows
only; a Linux path is Unknown.

```json
{ "mcpServers": { "filesystem": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "/Users/username/Desktop"] } } }
```

Remote servers in this file, variable expansion: Unknown (not in the doc).

**Claude Code** ([doc][cc]). Scopes: user = `~/.claude.json` top-level
`mcpServers`; local = `~/.claude.json` under `projects["<path>"].mcpServers`;
project = `.mcp.json` in the project root.

```json
{
  "mcpServers": {
    "api": {
      "type": "http",
      "url": "https://mcp.example.com/mcp",
      "headers": { "Authorization": "Bearer ${API_KEY:-default-token}" },
      "headersHelper": "/path/to/script.sh",
      "oauth": { "clientId": "id", "callbackPort": 8080, "scopes": "a b" }
    },
    "db": {
      "type": "stdio",
      "command": "/path/to/server",
      "args": ["--config", "${CLAUDE_PROJECT_DIR}/config.json"],
      "env": { "DB_URL": "${DB_URL:-postgresql://localhost}" }
    }
  }
}
```

- `type`: `stdio`, `http`, `sse`, `ws`.
- `${VAR}` and `${VAR:-default}` work in `command`, `args`, `env`, `url`, `headers`, `headersHelper`.
- In remote `url` and `headers`, variables with names that contain `TOKEN`, `SECRET`, `PASSWORD`, `KEY` or `AUTH` read as empty. The user copies them to a custom name.
- `headersHelper` runs a command that prints a JSON object of headers (10 s limit).
- Interactive sessions ask for approval of `.mcp.json` servers. `claude -p` does not ask.
- OAuth tokens: macOS Keychain, else a credentials file.
- Precedence: managed, local, project, user, plugin, claude.ai connectors. The first match wins; no field merge.

**VS Code** ([doc][vsc]). Workspace `.vscode/mcp.json`; user profile
`mcp.json` (for example `~/Library/Application Support/Code/User/mcp.json`,
per the [mcporter import doc][mcporter-import]). Root key `servers`, plus
`inputs` and `sandbox`.

```json
{
  "inputs": [
    { "type": "promptString", "id": "perplexity-key", "description": "Perplexity API Key", "password": true }
  ],
  "servers": {
    "perplexity": { "type": "stdio", "command": "<command>",
                    "env": { "PERPLEXITY_API_KEY": "${input:perplexity-key}" } },
    "slack": { "type": "http", "url": "https://mcp.slack.com/mcp", "oauth": { "clientId": "example-client-id" } }
  }
}
```

- stdio fields: `command`, `args`, `cwd`, `env`, `envFile`, `dev`, `sandboxEnabled`.
- Remote fields: `url`, `headers`, `oauth` (`clientId`, `enterpriseManaged`).
- Variables: `${input:id}` (prompted once, then stored in secret storage), `${env:VAR}`, `${workspaceFolder}`.
- VS Code also reads a "portable" format: `.mcp.json` in the workspace root and `~/.copilot/mcp-config.json`, root key `mcpServers`.

**Cursor** ([doc][cursor]). `~/.cursor/mcp.json` (user), `.cursor/mcp.json`
(project). Root `mcpServers`. No `type`: `command` means stdio, `url` means
remote.

```json
{ "mcpServers": {
    "local": { "command": "npx", "args": ["-y", "mcp-server"], "env": { "API_KEY": "${env:API_KEY}" } },
    "remote": { "url": "http://localhost:3000/mcp", "headers": { "API_KEY": "value" },
                "auth": { "CLIENT_ID": "id", "CLIENT_SECRET": "secret", "scopes": ["read"] } } } }
```

Variables in `command`, `args`, `env`, `url`, `headers`: `${env:NAME}`,
`${userHome}`, `${workspaceFolder}`, `${workspaceFolderBasename}`, `${/}`.
stdio also has `envFile`.

**Windsurf, now Devin Desktop** ([doc][windsurf]). `~/.config/devin/mcp_config.json`
(or `$XDG_CONFIG_HOME/devin/`). The older path is
`~/.codeium/windsurf/mcp_config.json` (per mcporter). Root `mcpServers`. Remote
servers use `serverUrl`, not `url`.

```json
{ "mcpServers": { "remote": { "serverUrl": "https://example.com/mcp",
                              "headers": { "API_KEY": "Bearer ${env:AUTH_TOKEN}" } } } }
```

Variables: `${env:VAR}` and `${file:/path}` (trimmed file content). Project
scope: Unknown.

**Zed** ([doc][zed]). Key `context_servers` in `settings.json`. Local:
`command`, `args`, `env`. Remote: `url`, `headers`. With no `Authorization`
header, Zed starts the MCP OAuth flow. User path `~/.config/zed/settings.json`
and project `.zed/settings.json`: not confirmed in the fetched doc. Project
scope for `context_servers`: Unknown.

```json
{ "context_servers": { "remote-mcp-server": { "url": "https://example.com/mcp", "headers": { "Authorization": "Bearer <token>" } } } }
```

**OpenAI Codex CLI** ([doc][codex]). TOML. `~/.codex/config.toml` (user),
`.codex/config.toml` (project, trusted projects only).

```toml
[mcp_servers.context7]
command = "npx"
args = ["-y", "@upstash/context7-mcp"]
env_vars = ["LOCAL_TOKEN"]          # forward these from the parent env

[mcp_servers.context7.env]
MY_ENV_VAR = "MY_ENV_VALUE"

[mcp_servers.figma]
url = "https://mcp.figma.com/mcp"
bearer_token_env_var = "FIGMA_OAUTH_TOKEN"
http_headers = { "X-Figma-Region" = "us-east-1" }
# env_http_headers = { "X-Key" = "ENV_VAR_NAME" }
startup_timeout_sec = 20
tool_timeout_sec = 45
enabled_tools = ["open", "screenshot"]
```

- No `type`: `command` or `url`.
- Secrets by environment variable name, not by inline interpolation.
- OAuth: `codex mcp login <name>`. `mcp_oauth_credentials_store = "auto" | "keyring" | "file"`; `auto` falls back to `CODEX_HOME/.credentials.json` ([source][codex-store]).
- `mcp_oauth_callback_port`, `mcp_oauth_callback_url` for remote hosts.

**Gemini CLI** ([doc][gemini]). `~/.gemini/settings.json` (user),
`.gemini/settings.json` (project). Root `mcpServers`.

```json
{ "mcpServers": {
    "py": { "command": "python", "args": ["-m", "my_mcp_server"],
            "env": { "DATABASE_URL": "$DB_CONNECTION_STRING", "API_KEY": "${EXTERNAL_API_KEY}" } },
    "http": { "httpUrl": "http://localhost:3000/mcp", "headers": { "Authorization": "Bearer t" } },
    "sse":  { "url": "https://svc.run.app/sse", "authProviderType": "service_account_impersonation" } } }
```

- Transport by field name: `command` = stdio, `httpUrl` = Streamable HTTP, `url` = SSE.
- Other fields: `cwd`, `timeout` (ms), `trust` (default `false`), `includeTools`, `excludeTools`, `oauth`.
- `$VAR` and `${VAR}` expand in `env`.
- Sensitive variables in the inherited environment are hidden from stdio servers. Variables set in `env` pass through.
- OAuth tokens: `~/.gemini/mcp-oauth-tokens.json`.

**goose** ([doc][goose]). YAML. `~/.config/goose/config.yaml`.

```yaml
extensions:
  filesystem:
    type: stdio
    name: filesystem
    enabled: true
    cmd: npx
    args: ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
    env_keys: []
    envs: {}
    timeout: 300
  remote-tools:
    type: streamable_http
    uri: "https://example.com/mcp"
    headers: {}
```

Secrets: environment first, then the system keyring, then
`~/.config/goose/secrets.yaml` when `GOOSE_DISABLE_KEYRING` is set.
`env_keys` names values from that secret chain (secondary source:
[goose keyring summary][goose-sec]). Project scope: Unknown.

**Other clients, from [discussion #2218][d2218]** (not checked against their
own docs): OpenCode `opencode.json`, key `mcp`, `type: local|remote`, field
`environment`. Copilot CLI `~/.copilot/mcp-config.json`, key `mcpServers`.
IntelliJ `.idea/mcp.json`, key `mcpServers`.

### B.2 Comparison

| Client | User file | Project file | Fmt | Root key | Transport selector | stdio fields | Remote fields | Variables / secrets |
|---|---|---|---|---|---|---|---|---|
| Claude Desktop | `~/Library/Application Support/Claude/claude_desktop_config.json` | none | JSON | `mcpServers` | none (`command`) | command, args, env | Unknown | Unknown |
| Claude Code | `~/.claude.json` | `.mcp.json` | JSON | `mcpServers` | `type`: stdio, http, sse, ws | command, args, env | url, headers, headersHelper, oauth | `${VAR}`, `${VAR:-d}`; keychain for OAuth |
| VS Code | profile `mcp.json` | `.vscode/mcp.json` | JSON | `servers` (+`inputs`) | `type`: stdio, http, sse | command, args, env, envFile, cwd | url, headers, oauth | `${input:id}` (secret store), `${env:VAR}` |
| Cursor | `~/.cursor/mcp.json` | `.cursor/mcp.json` | JSON | `mcpServers` | none (`command` / `url`) | command, args, env, envFile | url, headers, auth | `${env:VAR}`, `${userHome}` |
| Windsurf / Devin | `~/.config/devin/mcp_config.json` | Unknown | JSON | `mcpServers` | none (`command` / `serverUrl`) | command, args, env | serverUrl, headers | `${env:VAR}`, `${file:path}` |
| Zed | settings.json | Unknown | JSON | `context_servers` | none (`command` / `url`) | command, args, env | url, headers | Unknown |
| Codex CLI | `~/.codex/config.toml` | `.codex/config.toml` (trusted) | TOML | `mcp_servers` | none (`command` / `url`) | command, args, env, env_vars, cwd | url, http_headers, env_http_headers, bearer_token_env_var | env var names; keyring or file for OAuth |
| Gemini CLI | `~/.gemini/settings.json` | `.gemini/settings.json` | JSON | `mcpServers` | field name: `command` / `httpUrl` / `url` (SSE) | command, args, env, cwd | httpUrl, url, headers, oauth | `$VAR`, `${VAR}` in env; token file |
| goose | `~/.config/goose/config.yaml` | Unknown | YAML | `extensions` | `type`: stdio, streamable_http | cmd, args, envs, env_keys | uri, headers | keyring, else `secrets.yaml` |
| OpenCode (2218) | Unknown | `opencode.json` | JSON | `mcp` | `type`: local, remote | Unknown | Unknown | Unknown |
| mcporter | `~/.mcporter/mcporter.json` | `config/mcporter.json` | JSONC | `mcpServers` | none; `baseUrl`/`url` | command, args, env, cwd | url, headers, auth, oauth* | `${VAR}`, `${VAR:-d}`, `$env:VAR` |
| lydakis/mcpx | `~/.config/mcpx/config.toml` | imported manifests | TOML | `servers` | Unknown | command, args, env | Unknown | `${VAR}`; OAuth in OS store |
| IBM/mcp-cli | `server_config.json` | Unknown | JSON | `mcpServers` | none | command, args, env | url, headers | `${TOKEN:bearer:name}` (OS store) |

Common ground:

- A map from server name to definition.
- stdio: `command` (string), `args` (array), `env` (map). Remote: `url` and `headers` (map).
- `mcpServers` in JSON is the most common shape. Server READMEs show it.

Differences:

- Root key: `mcpServers`, `servers`, `context_servers`, `mcp_servers`, `extensions`, `mcp`.
- Transport selector: explicit `type` (Claude Code, VS Code, goose, OpenCode) or field inference.
- Remote URL field name: `url`, `serverUrl`, `httpUrl`, `uri`, `baseUrl`.
- Variable syntax: `${VAR}`, `${VAR:-d}`, `${env:VAR}`, `$env:VAR`, `$VAR`, `${input:id}`, `${file:path}`, `${TOKEN:type:name}`. mcporter rejects `${env:VAR}` with an error.
- Secrets: inline values (most), env var names (Codex), prompted secret store (VS Code), keychain references (IBM), header helper command (Claude Code).
- `cwd` support: VS Code, Codex, Gemini, mcporter. Claude Code, Cursor: Unknown.
- Project trust: Claude Code asks for approval; Codex needs a trusted project; mcpc does not expand variables from project files.

### B.3 Standard status

- No standard client config format exists. Issue [#292][i292] "Define a Standard MCP Configuration Schema" is open since 2025-04-08 (22 comments, no SEP). [#2219][i2219] closed as a duplicate. Discussion [#2218][d2218] closed on 2026-09-25 when Discussions closed.
- A 2026-07-28 comment on #292 proposes a portable server-definition schema plus host adapters, not one shared file.
- No SEP in `modelcontextprotocol/seps` covers client config (checked 2026-10-06).
- The registry [`server.json`][serverjson] (schema `2025-12-11`) describes how to install a server, not a client entry. It has useful secret metadata: `environmentVariables[].isSecret`, `remotes[].headers[].isSecret`, `isRequired`, `default`, and `{var}` placeholders with `variables`.

```json
{ "name": "io.example/brave-search", "version": "1.0.2",
  "packages": [ { "registryType": "npm", "identifier": "@modelcontextprotocol/server-brave-search",
                  "transport": { "type": "stdio" },
                  "environmentVariables": [ { "name": "BRAVE_API_KEY", "isRequired": true, "isSecret": true } ] } ],
  "remotes": [ { "type": "streamable-http", "url": "https://mcp.example.com/http",
                 "headers": [ { "name": "X-API-Key", "isRequired": true, "isSecret": true } ] } ] }
```

- [SEP-1024][sep1024] (Final): clients with one-click local install must show the full command and get consent. This supports S5.
- [SEP-2127][sep2127] (open): Server Cards for HTTP discovery (`.well-known`).

## Ideas worth taking

Interface (requirement IDs in brackets):

1. **Selector.** Accept `server.tool` (mcporter) and `server tool` (mcpx). Split at the first `.`. Forbid `.` in server names (SEP-986).
2. **Arguments** [I4]. Accept a JSON object as one argument when it starts with `{`. Accept JSON on stdin when stdin is a pipe and no arguments exist. Accept `key=value` with schema-aware coercion. Keep a string when the schema says string. Add `key:=<json>` for typed values (mcpc, httpie). Add `key=@file`.
3. **Schema before call** [I5]. Generate `<server> <tool> --help` from `inputSchema` (mcpx, mcp2cli). Reject unknown keys before the call (mcporter).
4. **JSON output** [I1, I2]. Print the MCP result object unchanged, with `structuredContent` and `isError` (mcpc, mcp2cli). On error in JSON mode, print one error object on stderr with a stable `code`, for example `{ "code": "auth", "server": "...", "message": "..." }` (mcporter, philschmid).
5. **Exit codes** [I3]. Prior art: 1 client, 2 server, 3 network, 4 auth (mcpc, philschmid); 1 `isError`, 2 usage (mcpx). The target needs six classes, including `input_required`. For `input_required`, print the JSON result and give resume flags (mcpx) [P8].
6. **Agent help**. Add `help --skill` that prints a `SKILL.md` for the installed version (mcpc). Add `grep` across servers with exit 0/1 like grep(1) (mcpc). Add a compact list (`--brief`), with the full schema on request. Show tool annotations (`read-only`, `destructive`).
7. **Actionable errors**. Each error has a code, details and a suggested command (philschmid). Suggest near-miss names (mcporter).
8. **Login** [A10, A12]. OAuth starts only from `login`. Never open a browser in an agent flow (mcpc, mcpx). Print the URL and read the pasted redirect URL from stdin (mcp2cli `--oauth-manual-callback`). `login --json` prints `authorizationUrl` (mcporter). Add `auth status` (mcpx). Add named profiles per server (mcpc `--profile`).
9. **Project trust** [S5]. Do not start commands or expand variables from a project-directory file until the user approves it or names it (mcpc, Claude Code, SEP-1024).
10. **Secrets in config** [S1, S4]. Refer to a stored secret by name in the config (IBM `${TOKEN:bearer:name}`). Never store the value in the file. Claude Code `headersHelper` is a fallback for dynamic tokens.
11. **Daemon** [F4, S6]. Lazy spawn on first call, idle exit after 60 s, new daemon on config change, direct connection after a spawn timeout (philschmid). Socket 0600 in a 0700 directory, peer check (mcporter). Opt-in keep-alive per server (mcporter `lifecycle`).
12. **Cache** [P12]. Cache tool lists with a TTL and `--refresh` (mcp2cli). Clear the cache on `list_changed` (mcpc). Use server `ttlMs` when present.
13. **Diagnostics** [I6, F2]. A `doctor` command with redacted auth (mcpx). Redact secrets in all logs (IBM).
14. **Schema pin**. `--schema expected.json` fails a call when the tool schema changed in an incompatible way (mcpc).
15. **Shims**. `shim install <server>` writes `~/.local/bin/<server>`. It fails when the name already exists in `PATH` (mcpx).
16. **Tool filters**. Per-server `allowedTools` / `blockedTools` (mcporter, Codex, Gemini, philschmid).

## Config format options

No decision here. Each option assumes the S1 rule: secrets live in the OS
credential store, and the file holds only references.

### Option 1: own TOML file, import for other formats

`~/.config/<cli>/config.toml` (XDG). Shape like Codex or lydakis/mcpx:
`[servers.<name>]` with `command`, `args`, `env`, `cwd`, `url`, `headers`.
Secret references by name, for example `headers.Authorization = { secret = "github" }`.
An `import` command reads `mcpServers` JSON, VS Code and Codex files, with a
redacted preview.

| For | Against |
|---|---|
| Comments, typed values, native parsers in Rust and Go | Not paste-compatible with server READMEs (they show `mcpServers` JSON) |
| The CLI owns the schema; no conflict with other tools | Two formats to support: native and import |
| Clear place for CLI fields (timeouts, cache, profiles, tool filters) | Project-scope file is a new name that no other tool reads |

### Option 2: `mcpServers` JSON superset

User file `~/.config/<cli>/mcp.json` and project `.mcp.json`, in the Claude Code
shape with explicit `type`. `${VAR}` and `${VAR:-default}`. CLI fields under one
extension key, for example `"x-<cli>": { ... }`.

| For | Against |
|---|---|
| Paste-compatible; mcpc and mcporter read the same file | No comments, unless JSONC (other readers can reject JSONC) |
| Shares project `.mcp.json` with Claude Code | Claude Code owns the semantics of `.mcp.json`; its changes break the CLI |
| One format to support | Inline values in shared files invite plain-text secrets (S2 risk); variable rules differ between clients |

### Option 3: own file plus read-only references to client files

Own user file (TOML or JSON) for CLI settings and servers. A server entry can
point at an entry in another file, for example
`ref = "~/.claude.json:github"` or `.vscode/mcp.json:fs` (mcpc `file:entry`).
The CLI never writes other clients' files.

| For | Against |
|---|---|
| No duplicate server definitions | Parsers for several formats (`mcpServers`, `servers`, TOML, `serverUrl`, `httpUrl`) |
| Explicit trust step: the user names each external file | Variable syntax differs per source file; one rule per source |
| No conflict on ownership | Precedence rules are harder to explain |

Open points for all options: the secret reference syntax (`${secret:name}`,
`{ secret = "name" }` or `env:` names like Codex); whether `cwd` is relative
to the config file (mcporter) or to the current directory; and the project-file
approval store.

## Sources

Part A:

- [apify/mcpc README][mcpc]
- [openclaw/mcporter README][mcporter]; docs [call-syntax], [cli-reference], [config][mcporter-config], [daemon][mcporter-daemon], [agent-skills][mcporter-skills], [import][mcporter-import]
- [knowsuchagency/mcp2cli README][mcp2cli]
- [philschmid/mcp-cli README][phil], [src/errors.ts][phil-errors]
- [lydakis/mcpx README][mcpx]
- [IBM/mcp-cli README][ibm]
- [f/mcptools README][mcptools]
- [wong2/mcp-cli README][wong2]
- [thellimist/clihub][clihub]

Part B:

- [Claude Desktop: connect local servers][cd]
- [Claude Code: MCP][cc]
- [VS Code: MCP configuration reference][vsc]
- [Cursor: MCP][cursor]
- [Devin Desktop (Windsurf) Cascade MCP][windsurf]
- [Zed: MCP][zed]
- [Codex: MCP][codex]; [Codex sample config][codex-store]
- [Gemini CLI: MCP servers][gemini]
- [goose: config files][goose]; [goose keyring summary][goose-sec]
- [MCP registry server.json format][serverjson]
- [Issue #292][i292], [Issue #2219][i2219], [Discussion #2218][d2218]
- [SEP-986][sep986], [SEP-1024][sep1024], [SEP-2127][sep2127]

[mcpc]: https://github.com/apify/mcpc
[mcporter]: https://github.com/openclaw/mcporter
[call-syntax]: https://github.com/openclaw/mcporter/blob/main/docs/call-syntax.md
[cli-reference]: https://github.com/openclaw/mcporter/blob/main/docs/cli-reference.md
[mcporter-config]: https://github.com/openclaw/mcporter/blob/main/docs/config.md
[mcporter-daemon]: https://github.com/openclaw/mcporter/blob/main/docs/daemon.md
[mcporter-skills]: https://github.com/openclaw/mcporter/blob/main/docs/agent-skills.md
[mcporter-import]: https://github.com/openclaw/mcporter/blob/main/docs/config.md#mcporter-config-import-kind
[mcp2cli]: https://github.com/knowsuchagency/mcp2cli
[phil]: https://github.com/philschmid/mcp-cli
[phil-errors]: https://github.com/philschmid/mcp-cli/blob/main/src/errors.ts
[mcpx]: https://github.com/lydakis/mcpx
[ibm]: https://github.com/IBM/mcp-cli
[mcptools]: https://github.com/f/mcptools
[wong2]: https://github.com/wong2/mcp-cli
[clihub]: https://github.com/thellimist/clihub
[cd]: https://modelcontextprotocol.io/docs/develop/connect-local-servers
[cc]: https://code.claude.com/docs/en/mcp
[vsc]: https://code.visualstudio.com/docs/copilot/reference/mcp-configuration
[cursor]: https://cursor.com/docs/context/mcp
[windsurf]: https://docs.devin.ai/desktop/cascade/mcp
[zed]: https://zed.dev/docs/ai/mcp
[codex]: https://learn.chatgpt.com/docs/extend/mcp?surface=cli
[codex-store]: https://developers.openai.com/codex/config-sample.md
[gemini]: https://geminicli.com/docs/tools/mcp-server/
[goose]: https://github.com/block/goose/blob/main/documentation/docs/guides/config-files.md
[goose-sec]: https://instagit.com/block/goose/goose-api-key-security-keyring-integration/
[serverjson]: https://github.com/modelcontextprotocol/registry/blob/main/docs/reference/server-json/generic-server-json.md
[i292]: https://github.com/modelcontextprotocol/modelcontextprotocol/issues/292
[i2219]: https://github.com/modelcontextprotocol/modelcontextprotocol/issues/2219
[d2218]: https://github.com/modelcontextprotocol/modelcontextprotocol/discussions/2218
[sep986]: https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/seps/986-specify-format-for-tool-names.md
[sep1024]: https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/seps/1024-mcp-client-security-requirements-for-local-server-.md
[sep2127]: https://github.com/modelcontextprotocol/modelcontextprotocol/issues/2127
