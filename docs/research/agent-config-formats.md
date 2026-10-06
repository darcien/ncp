# Study: MCP server config formats of agent TUIs, CLIs and harnesses

Date: 2026-10-07. Input for the provisional `ncp` config format.

**Method.** Read official docs (WebFetch), public repo files (`gh api`) and
web search results. One `curl` GET read Amp's public settings JSON schema.
Nothing was cloned, installed or run. This study extends
[interface-and-config.md part B](interface-and-config.md#part-b-configuration-formats-i8).
Rows marked "B.2" come from that study and were not checked again.
"Unknown" means the fetched official source does not state the fact.

## 0. Scope status

| Tool | Status 2026-10-07 | MCP config file |
|---|---|---|
| OpenCode (`anomalyco/opencode`, was `sst/opencode`) | Active | Yes |
| Crush (`charmbracelet/crush`) | Active | Yes. `crushrc` (Bash). `crush.json` is deprecated but read. |
| GitHub Copilot CLI | Active (1.0.92, 2026-10-05) | Yes |
| Amp | Active | Yes (inside `settings.json`) |
| Kiro (IDE and CLI) | Active | Yes |
| Cline (IDE, CLI) | Active | Yes |
| Roo Code | Repo archived (last push 2026-05-15) | Yes |
| Continue | Active | Yes (YAML blocks) |
| Warp | Active | Yes |
| Qwen Code | Active (Gemini CLI fork) | Yes |
| Kimi CLI | Archived. Replaced by Kimi Code CLI. | Kimi Code CLI: yes |
| Factory droid | Active | Yes |
| Cursor CLI | Active | Same files as the Cursor editor |
| Augment (Auggie CLI) | Active | Yes |
| Junie (IDE and CLI) | Active | Yes |
| JetBrains AI Assistant | Active | IDE settings dialog. Accepts `mcpServers` JSON snippets. File path Unknown. |
| Docker MCP Toolkit | Active | No hand-edited per-server file. Local database plus profiles. Out of format scope. |
| LibreChat | Active (server app) | Yes (`librechat.yaml`) |
| Hermes Agent (NousResearch), added | Active, 251k stars | Yes |
| Mistral Vibe, added | Active | Yes |
| OpenHands CLI, added | Active | Yes |
| LM Studio, added | Active | Yes ("follows Cursor's `mcp.json` notation") |

## 1. Formats by tool (new rows)

### OpenCode

User `~/.config/opencode/opencode.json`. Project `opencode.json` (also `.jsonc`).
`OPENCODE_CONFIG` adds a custom file. All files merge; they do not replace.

```jsonc
{ "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "local":  { "type": "local", "command": ["npx", "-y", "my-mcp"], "cwd": "dir",
                "environment": { "K": "{env:MY_KEY}" }, "enabled": true, "timeout": 5000 },
    "remote": { "type": "remote", "url": "https://mcp.example.com",
                "headers": { "Authorization": "Bearer {env:MY_API_KEY}" },
                "oauth": { "clientId": "{env:ID}", "scope": "a b" } } },
  "tools": { "my-mcp*": false } }
```

- `command` is one array (program plus arguments). No `args`.
- `{env:VAR}` (unset gives empty string). `{file:path}` (relative to the config file).
- `oauth: false` disables OAuth detection. Tokens: `~/.local/share/opencode/mcp-auth.json`.
- `timeout` (ms, default 5000) applies to the tool list fetch.

### Crush

Precedence: `./.crushrc`, `./crushrc`, `$XDG_CONFIG_HOME/crush/crushrc`.
Legacy JSON: `.crush.json` or `crush.json` in the same directories. All files merge.

```bash
mcp add github --type http --url https://api.githubcopilot.com/mcp/ \
  --timeout 10 --header Authorization "Bearer $GH_PAT" --disabled-tools create_issue
```

```json
{ "mcp": { "github": { "type": "http", "url": "https://api.github.com/mcp/",
  "headers": { "Authorization": "Bearer $GH_PAT" }, "oauth": true,
  "oauth_client_id": "Iv1.abc", "oauth_client_secret": "$GITHUB_MCP_SECRET",
  "oauth_callback_port": 40704 } } }
```

- Schema fields: `type` (stdio, sse, http; required in JSON), `command`, `args`, `env`, `url`, `headers`, `disabled`, `disabled_tools`, `enabled_tools`, `timeout` (s, default 10), `sessionless`, `oauth*`. No `cwd`.
- `$VAR` and `$(command)` expand at load time. Crush calls both files "trusted code".

### GitHub Copilot CLI

User `~/.copilot/mcp-config.json` (`COPILOT_HOME` moves it). Project
`.mcp.json` or `.github/mcp.json`, searched from cwd up to the repo root.
`.mcp.json` wins in one directory. Copilot CLI no longer reads `.vscode/mcp.json`.

```json
{ "mcpServers": {
    "playwright": { "type": "local", "command": "npx", "args": ["@playwright/mcp@latest"],
                    "env": { "TOKEN": "${GITHUB_TOKEN}" }, "tools": ["*"] },
    "context7":   { "type": "http", "url": "https://mcp.context7.com/mcp",
                    "headers": { "CONTEXT7_API_KEY": "KEY" }, "tools": ["resolve-library-id"] } } }
```

- `type`: `local`, `stdio`, `http`, `sse`. Other fields: `cwd` (with `~`), `timeout` (ms), `deferTools`, `oauthClientId`, `oauthScopes`, `auth.redirectPort` (changelog).
- `env` values are literal. `${VAR}` reads the parent environment (changelog 0.0.340).
- Project files accept `mcpServers` or a bare map of servers. Project servers load only after folder trust.
- Disable: `copilot mcp disable NAME`. The state persists. Its storage location is Unknown.

### Amp

User `~/.config/amp/settings.json` or `.jsonc`. Workspace: nearest
`.amp/settings.json` upward from cwd. Root key `amp.mcpServers` (a flat, dotted key).

```json
{ "amp.mcpServers": {
    "playwright":  { "command": "npx", "args": ["-y", "@playwright/mcp@latest"] },
    "sourcegraph": { "url": "${SRC_ENDPOINT}/.api/mcp/v1",
                     "headers": { "Authorization": "token ${SRC_ACCESS_TOKEN}" } } },
  "amp.tools.disable": ["mcp_server_tool*"] }
```

- Schema stdio: `command`, `args`, `env`. Remote: `url`, `headers`, `transport` (`http`, `sse`, `http-first`), `oauth` (`clientId`, `clientSecret`, `authUrl`, `tokenUrl`, `scopes`, `redirectUrl`).
- `${VAR}` and `${amp:id-token}`. Workspace servers need `amp mcp approve NAME`.
- `amp.mcpPermissions`: ordered allow or reject rules that match `command`, `args` or `url`.
- `cwd`, timeouts, per-server `disabled` in the file: Unknown.

### Kiro (IDE and CLI)

User `~/.kiro/settings/mcp.json`. Workspace `.kiro/settings/mcp.json` wins.
An agent config `mcpServers` field wins over both. Root `mcpServers`.

- Local: `command`, `args`, `env`. Remote: `url` (HTTPS, or HTTP on localhost), `headers`, `oauth`, `oauthScopes`.
- Both: `disabled`, `autoApprove` (`"*"` allowed), `disabledTools`.
- `${VAR}` expands only after the user approves that variable.
- `oauth`: `clientId`, `clientSecret` (CLI only), `redirectUri`, `clientMetadataUrl`.
- `type`, `cwd`, timeouts: Unknown.

### Cline

CLI and shared file `~/.cline/data/settings/cline_mcp_settings.json`
(`CLINE_MCP_SETTINGS_PATH` overrides). The IDE opens its own copy from the UI.
No project MCP file. Root `mcpServers`.

```json
{ "mcpServers": {
    "local":  { "command": "node", "args": ["s.js"], "cwd": "/srv", "env": { "K": "${env:K}" },
                "disabled": false, "autoApprove": [], "timeout": 60 },
    "remote": { "type": "streamableHttp", "url": "https://x/mcp", "headers": {} } } }
```

- `type`: `stdio`, `sse`, `streamableHttp`. A `url` entry without `type` is SSE.
- The CLI writes a nested form: `{ "transport": { "type": ..., "url": ... }, ... }`.
- `${env:VAR}` only. An unset variable stays as literal text.
- `timeout` in seconds, default 60, range 1 to 3600.

### Roo Code (archived)

Global `mcp_settings.json` (extension storage). Project `.roo/mcp.json` wins.
Root `mcpServers`. Fields: `command`, `args`, `cwd`, `env`, `type`
(`streamable-http`, `sse`), `url`, `headers`, `alwaysAllow`, `disabled`,
`disabledTools`, `timeout` (s, 1 to 3600, default 60), `watchPaths`.
`${env:VAR}` works in `args`.

### Continue

User `~/.continue/config.yaml`. Workspace: one file per block in
`.continue/mcpServers/` (YAML, or JSON copied from Claude Desktop, Cursor, Cline).

```yaml
mcpServers:            # a list, not a map
  - name: GitHub
    type: stdio        # stdio | sse | streamable-http
    command: npx
    args: ["-y", "@modelcontextprotocol/server-github"]
    cwd: /path
    env:
      GITHUB_PERSONAL_ACCESS_TOKEN: ${{ secrets.GITHUB_PERSONAL_ACCESS_TOKEN }}
```

- Remote: `url`, `requestOptions` (headers). `connectionTimeout`.
- Secrets: `${{ secrets.X }}`, `${{ inputs.X }}`.

### Warp

Files: `~/.warp/.mcp.json`, `.warp/.mcp.json`, `~/.agents/.mcp.json`,
`.agents/.mcp.json`. Warp also reads Claude Code (`~/.claude.json`, `.mcp.json`)
and Codex (`~/.codex/config.toml`). Root `mcpServers`.

- stdio: `command`, `args`, `env`, `working_directory`. Remote: `url`, `headers`.
- OAuth by one-click login. Credentials stay on the device.
- Variable expansion by Warp: Unknown. The doc example puts `${X_CHROMA_TOKEN}` in `mcp-remote` args; `mcp-remote` can expand it.

### Qwen Code

Gemini CLI shape. User `~/.qwen/settings.json`, project `.qwen/settings.json`.

- `command` = stdio, `httpUrl` = Streamable HTTP, `url` = SSE.
- Fields: `args`, `env` (`$VAR`, `${VAR}`), `cwd`, `headers`, `timeout` (ms, default 600000, tool call), `discoveryTimeoutMs`, `trust`, `includeTools`, `excludeTools` (exclude wins).
- `oauth`: `enabled`, `clientId`, `clientSecret`, `authorizationUrl`, `tokenUrl`, `scopes`, `redirectUri`, `audiences`.
- Tokens: `~/.qwen/mcp-oauth-tokens.json`, plaintext 0600 by default.
- Server filters: `mcp.allowed`, `mcp.excluded` (globs).

### Kimi Code CLI (replaces Kimi CLI)

User `~/.kimi-code/mcp.json` (or `$KIMI_CODE_HOME/mcp.json`). Project
`.kimi-code/mcp.json` wins. Project servers need workspace trust.

```json
{ "mcpServers": {
    "fs":     { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"] },
    "linear": { "url": "https://mcp.linear.app/mcp", "bearerTokenEnvVar": "LINEAR_TOKEN" },
    "old":    { "transport": "sse", "url": "https://mcp.example.com/sse" } } }
```

- Fields: `env`, `cwd`, `headers`, `bearerTokenEnvVar`, `enabled`, `deferred`, `startupTimeoutMs` (default 30000), `toolTimeoutMs`, `enabledTools`, `disabledTools`.
- OAuth: `/mcp-config login NAME`. No OAuth fields in the file.
- Interpolation syntax: Unknown (not in the doc).

### Factory droid

User `~/.factory/mcp.json`. Folder and project `.factory/mcp.json`.
Organization-managed servers win. Root `mcpServers`.

```json
{ "mcpServers": {
    "linear":   { "type": "http", "url": "https://mcp.linear.app/mcp", "disabled": false },
    "airtable": { "type": "stdio", "command": "npx", "args": ["-y", "airtable-mcp-server"],
                  "env": { "AIRTABLE_API_KEY": "${AIRTABLE_API_KEY}" } } } }
```

- `type`: `stdio` (default), `http`, `sse`. Fields: `disabled`, `disabledTools`, `timeout` (ms, tool call), `connectTimeout` (ms).
- `${VAR}` expands only in `env`, `headers`, `oauth.clientId`, `oauth.clientSecret`.
- `oauth`: `scopes`, `resource`, `authorizationServerIssuer`, `clientId`, `clientSecret`, `clientMetadataUrl`, `tokenEndpointAuthMethod`, `callbackPort`. Tokens: system keyring, else a fallback file.
- `cwd`: Unknown.

### Cursor CLI

"MCP in the CLI uses the same configuration as the editor": `~/.cursor/mcp.json`
and `.cursor/mcp.json` (see B.2). `agent mcp enable|disable|login` change an
approved list. Its storage location is Unknown.

### Augment (Auggie CLI)

`~/.augment/settings.json`, root `mcpServers`. `--mcp-config` adds servers for
one run in the same shape. `type`: `http`, `sse`, or none for stdio.
Fields: `command`, `args`, `env`, `url`, `headers`. Only `${workspaceFolder}`
expands, in `command`, `args`, `url`. Project file, `cwd`, timeouts, OAuth: Unknown.

### Junie (IDE and CLI)

Project `.junie/mcp/mcp.json`, user `~/.junie/mcp/mcp.json`. Root `mcpServers`.
The doc example shows `command`, `args`, `env`, `url`, `headers`. Remote
OAuth servers show "Authorization required" in `/mcp`. Other fields: Unknown.

### Hermes Agent

`~/.hermes/config.yaml`, key `mcp_servers`. Secrets in `~/.hermes/.env`.
`hermes import-agent claude-code` converts `mcpServers` from `~/.claude.json`.

```yaml
mcp_servers:
  github:
    command: npx
    args: ["-y", "@modelcontextprotocol/server-github"]
    env: { GITHUB_PERSONAL_ACCESS_TOKEN: "${env:GITHUB_TOKEN}" }  # same as ${GITHUB_TOKEN}
    enabled: true
    timeout: 120          # s, tool call, default 300
    connect_timeout: 60   # s
    tools: { include: [create_issue], exclude: [], resources: true, prompts: true }
  linear:
    url: https://mcp.linear.app/mcp
    auth: oauth
```

- Accepts `${VAR}` and `${env:VAR}`, plus `${userHome}`, `${workspaceFolder}`, `${/}`.
- An unset variable in remote `url` or `headers` fails the connect. Elsewhere the text stays literal.
- Other fields: `cwd`, `transport: sse`, `trust` (`full`, `untrusted`), `lazy`, `ssl_verify`, `client_cert`, `keepalive_interval`.
- Tokens: `~/.hermes/mcp-tokens/<server>.json`. Project scope: Unknown.

### Mistral Vibe

`./.vibe/config.toml`, else `~/.vibe/config.toml`. An array of tables.

```toml
[[mcp_servers]]
name = "fetch_server"
transport = "stdio"          # stdio | http | streamable-http
command = "uvx"
args = ["mcp-server-fetch"]
env = { "DEBUG" = "1" }

[[mcp_servers]]
name = "my_http_server"
transport = "http"
url = "http://localhost:8000"
startup_timeout_sec = 10
tool_timeout_sec = 60
[mcp_servers.auth]
type = "static"              # or "oauth" with scopes = []
headers = { "X-Client" = "vibe" }
api_key_env = "MY_API_KEY_ENV_VAR"
api_key_header = "Authorization"
api_key_format = "Bearer {token}"
```

Disable a tool with top-level `disabled_tools` globs.

### OpenHands CLI

`~/.openhands/mcp.json`, root `mcpServers` (JSON since 1.0.0; TOML before).
The doc shows stdio fields `command`, `args`, `env`. `openhands mcp add`
takes `--transport http|sse|stdio`, `--header`, `--env`, `--auth oauth`,
`--disabled`. JSON field names for remote servers: Unknown.

### LM Studio

`mcp.json` from the app UI, root `mcpServers`, Cursor notation. Example shows
`url`, `headers`. File path: Unknown.

### LibreChat

`librechat.yaml`, key `mcpServers` (server-side, multi-user).

- `type`: `stdio`, `sse`, `websocket`, `streamable-http`. Without `type`: `http(s)` URL gives SSE, `ws(s)` gives WebSocket.
- Fields: `command`, `args`, `env`, `url`, `headers`, `timeout`, `initTimeout`, `apiKey`, `oauth`, `requiresOAuth`, `customUserVars`, `startup`, `chatMenu`, `serverInstructions`.
- `oauth`: `authorization_url`, `token_url`, `client_id`, `client_secret`, `redirect_uri`, `scope`, and more.
- Variables: `${ENV_VAR}`, `{{LIBRECHAT_USER_ID}}`, `{{CUSTOM_VAR}}` from `customUserVars`.

### Docker MCP Toolkit

No per-server file to hand-edit. Servers live in profiles in a local
database. A client gets one stdio entry:
`{"type": "stdio", "command": "docker", "args": ["mcp", "gateway", "run", "--profile", "p"]}`.
Profile settings use `docker mcp profile config --set github.timeout=30`.
Secrets use `docker mcp secret`.

### Unknowns from B.2, now filled

| B.2 Unknown | Finding |
|---|---|
| OpenCode user file, fields | `~/.config/opencode/opencode.json`; fields above |
| Copilot CLI project file | `.mcp.json`, `.github/mcp.json` |
| Claude Code per-server timeout | `timeout` (ms) in a `.mcp.json` entry, tool call limit |
| Claude Code `cwd` | Not documented on the MCP page. Still Unknown. |
| Zed project scope, variables | Not documented on the MCP page. Still Unknown. |
| IntelliJ `.idea/mcp.json` | Not confirmed. Junie uses `.junie/mcp/mcp.json`; AI Assistant uses an IDE dialog. |
| Codex extra fields | `enabled`, `required`, `disabled_tools`, `default_tools_approval_mode`, `tools.<t>.approval_mode` |

## 2. Comparison table

Abbreviations: "infer" = no transport field; `command` means stdio and the URL
field means remote. "ms"/"s" = timeout unit. "-" = not supported per the doc.

| Tool | User file | Project file | Fmt | Root key | Transport selector | stdio fields | Remote URL / headers | Variables / secrets | Enable flag | OAuth in file | Tool filter | Timeouts |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Claude Code (B.2) | `~/.claude.json` | `.mcp.json` | JSON | `mcpServers` | `type`: stdio, http, sse, ws | command, args, env | url, headers, headersHelper | `${VAR}`, `${VAR:-d}` | settings lists | `oauth` | permission rules | `timeout` ms |
| VS Code (B.2) | profile `mcp.json` | `.vscode/mcp.json` | JSON | `servers` | `type`: stdio, http, sse | command, args, env, envFile, cwd | url, headers | `${input:id}`, `${env:VAR}` | Unknown | `oauth` | Unknown | Unknown |
| Cursor + Cursor CLI (B.2) | `~/.cursor/mcp.json` | `.cursor/mcp.json` | JSON | `mcpServers` | infer | command, args, env, envFile | url, headers | `${env:VAR}`, `${workspaceFolder}` | CLI approved list | `auth` | Unknown | Unknown |
| Windsurf (B.2) | `~/.config/devin/mcp_config.json` | Unknown | JSON | `mcpServers` | infer | command, args, env | **serverUrl**, headers | `${env:VAR}`, `${file:p}` | Unknown | Unknown | Unknown | Unknown |
| Zed (B.2) | settings.json | Unknown | JSON | `context_servers` | infer | command, args, env | url, headers | Unknown | Unknown | auto | `mcp:srv:tool` perms | Unknown |
| Codex (B.2+) | `~/.codex/config.toml` | `.codex/config.toml` | TOML | `mcp_servers` | infer | command, args, env, env_vars, cwd | url, http_headers, env_http_headers | env var names | `enabled` | CLI login | enabled_tools, disabled_tools | startup/tool `_sec` |
| Gemini CLI (B.2) | `~/.gemini/settings.json` | `.gemini/settings.json` | JSON | `mcpServers` | infer: `httpUrl` / `url`=SSE | command, args, env, cwd | **httpUrl**, url, headers | `$VAR`, `${VAR}` | Unknown | `oauth` | includeTools, excludeTools | `timeout` ms |
| goose (B.2) | `~/.config/goose/config.yaml` | Unknown | YAML | `extensions` | `type` | **cmd**, args, **envs**, env_keys | **uri**, headers | keyring names | `enabled` | Unknown | Unknown | `timeout` |
| OpenCode | `~/.config/opencode/opencode.json` | `opencode.json` | JSONC | `mcp` | `type`: local, remote | **command[]**, cwd, **environment** | url, headers | `{env:V}`, `{file:p}` | `enabled` | `oauth` obj or false | top-level `tools` globs | `timeout` ms |
| Crush | `~/.config/crush/crushrc` (+ JSON) | `.crushrc`, `crush.json` | Bash / JSON | `mcp` | `type`: stdio, http, sse | command, args, env | url, headers | `$VAR`, `$(cmd)` | `disabled` | `oauth` bool + `oauth_*` | enabled_tools, disabled_tools | `timeout` s |
| Copilot CLI | `~/.copilot/mcp-config.json` | `.mcp.json`, `.github/mcp.json` | JSON | `mcpServers` | `type`: local, stdio, http, sse | command, args, env, cwd | url, headers | `${VAR}` | CLI state | oauthClientId, oauthScopes, auth.redirectPort | `tools` allow list | `timeout` ms |
| Amp | `~/.config/amp/settings.json(c)` | `.amp/settings.json(c)` | JSONC | `amp.mcpServers` | `transport`: http, sse, http-first | command, args, env | url, headers | `${VAR}`, `${amp:id-token}` | Unknown | `oauth` obj | `amp.tools.disable` globs | Unknown |
| Kiro | `~/.kiro/settings/mcp.json` | `.kiro/settings/mcp.json` | JSON | `mcpServers` | infer | command, args, env | url, headers | `${VAR}` after approval | `disabled` | `oauth`, oauthScopes | disabledTools, autoApprove | Unknown |
| Cline | `~/.cline/data/settings/cline_mcp_settings.json` | - | JSON | `mcpServers` | `type`: stdio, sse, streamableHttp | command, args, env, cwd | url, headers | `${env:VAR}` | `disabled` | written by CLI | autoApprove | `timeout` s |
| Roo Code (archived) | `mcp_settings.json` | `.roo/mcp.json` | JSON | `mcpServers` | `type`: streamable-http, sse | command, args, env, cwd | url, headers | `${env:VAR}` | `disabled` | Unknown | disabledTools, alwaysAllow | `timeout` s |
| Continue | `~/.continue/config.yaml` | `.continue/mcpServers/*` | YAML | `mcpServers` **list** | `type`: stdio, sse, streamable-http | command, args, env, cwd | url, requestOptions | `${{ secrets.X }}` | Unknown | Unknown | Unknown | connectionTimeout |
| Warp | `~/.warp/.mcp.json`, `~/.agents/.mcp.json` | `.warp/.mcp.json`, `.agents/.mcp.json` | JSON | `mcpServers` | infer | command, args, env, **working_directory** | url, headers | Unknown | UI | UI login | Unknown | Unknown |
| Qwen Code | `~/.qwen/settings.json` | `.qwen/settings.json` | JSON | `mcpServers` | infer: `httpUrl` / `url`=SSE | command, args, env, cwd | **httpUrl**, url, headers | `$VAR`, `${VAR}` | `mcp.excluded` | `oauth` obj | includeTools, excludeTools | timeout, discoveryTimeoutMs (ms) |
| Kimi Code CLI | `~/.kimi-code/mcp.json` | `.kimi-code/mcp.json` | JSON | `mcpServers` | infer; `transport: sse` | command, args, env, cwd | url, headers, bearerTokenEnvVar | env var name | `enabled` | CLI login | enabledTools, disabledTools | startupTimeoutMs, toolTimeoutMs |
| Factory droid | `~/.factory/mcp.json` | `.factory/mcp.json` | JSON | `mcpServers` | `type`: stdio, http, sse | command, args, env | url, headers | `${VAR}` (env, headers, oauth) | `disabled` | `oauth` obj | disabledTools | timeout, connectTimeout (ms) |
| Augment | `~/.augment/settings.json` | Unknown | JSON | `mcpServers` | `type`: http, sse | command, args, env | url, headers | `${workspaceFolder}` only | Unknown | Unknown | Unknown | Unknown |
| Junie | `~/.junie/mcp/mcp.json` | `.junie/mcp/mcp.json` | JSON | `mcpServers` | infer | command, args, env | url, headers | Unknown | Unknown | Unknown | Unknown | Unknown |
| Hermes Agent | `~/.hermes/config.yaml` | Unknown | YAML | `mcp_servers` | infer; `transport: sse` | command, args, env, cwd | url, headers | `${VAR}`, `${env:VAR}`, `.env` | `enabled` | `auth: oauth` + `oauth` | tools.include, tools.exclude | timeout, connect_timeout (s) |
| Mistral Vibe | `~/.vibe/config.toml` | `.vibe/config.toml` | TOML | `[[mcp_servers]]` **list** | `transport`: stdio, http, streamable-http | command, args, env | url, auth.headers, auth.api_key_env | env var name | Unknown | `auth.type = "oauth"` | global disabled_tools | startup/tool `_sec` |
| OpenHands CLI | `~/.openhands/mcp.json` | Unknown | JSON | `mcpServers` | Unknown | command, args, env | Unknown | Unknown | CLI `--disabled` | CLI `--auth` | Unknown | Unknown |
| LibreChat | `librechat.yaml` (server) | - | YAML | `mcpServers` | `type`: stdio, sse, websocket, streamable-http | command, args, env | url, headers | `${VAR}`, `{{USER_VAR}}` | Unknown | `oauth` obj | Unknown | timeout, initTimeout |

## 3. Common core

18 of 28 tools use `mcpServers` as a map of servers. The shared core is the
Claude Desktop shape plus a few fields:

```json
{
  "mcpServers": {
    "<name>": {
      "command": "string",
      "args": ["string"],
      "env": { "KEY": "value" },
      "cwd": "path",
      "url": "https://...",
      "headers": { "Name": "value" }
    }
  }
}
```

| Field | Shared by | Notes |
|---|---|---|
| map from name to server | all except Continue, Vibe (lists) | Name is the key |
| `command` (string) | all except OpenCode (array), goose (`cmd`) | |
| `args` (string array) | all except OpenCode | |
| `env` (string map) | all except OpenCode (`environment`), goose (`envs`) | |
| `url` | all except Windsurf (`serverUrl`), goose (`uri`); Gemini/Qwen use `httpUrl` for Streamable HTTP | |
| `headers` (string map) | most; Codex `http_headers`; Continue `requestOptions`; Vibe `auth.headers` | |
| `cwd` | 11 tools; Warp uses `working_directory` | Base of a relative path differs or is Unknown |
| `type` | 12 tools | Values differ (section 4) |
| `disabled: true` or `enabled: false` | 10 tools | Two opposite names |
| deny list of tools | 9 tools per server; OpenCode, Amp, Vibe globally | Many names |
| `timeout` | 12 tools | Unit and meaning differ |
| `oauth` object with `clientId`, `clientSecret`, scopes | 10 tools | Sub-field names differ |

## 4. Differences that matter for import

**Root key.**

| Root key | Tools |
|---|---|
| `mcpServers` (map) | Claude Desktop, Claude Code, Cursor, Windsurf, Gemini, Qwen, Copilot CLI, Kiro, Cline, Roo, Factory, Augment, Junie, Warp, Kimi Code, OpenHands, LM Studio, LibreChat (YAML) |
| `mcpServers` (list with `name`) | Continue |
| `servers` | VS Code |
| `amp.mcpServers` | Amp (dotted key, not nested) |
| `mcp` | OpenCode, Crush |
| `mcp_servers` | Codex (TOML table), Hermes (YAML map) |
| `[[mcp_servers]]` with `name` | Mistral Vibe |
| `context_servers` | Zed |
| `extensions` | goose |
| none (bare map) | Copilot CLI project files (also accepted) |

**Remote URL field.** `url` in most tools. Exceptions: `serverUrl` (Windsurf),
`httpUrl` (Gemini, Qwen), `uri` (goose).

**Meaning of `url` without a type.** This is the largest trap.

| `url` alone means | Tools |
|---|---|
| Streamable HTTP | Cursor, Codex, Kimi Code, Hermes |
| SSE | Gemini, Qwen, Cline (no `type`), LibreChat (`http(s)`), Warp (doc says "Server-Sent Events") |
| try HTTP, then SSE | Amp `http-first` (default value Unknown) |
| Unknown | Kiro, Junie, Zed, Windsurf, OpenHands, LM Studio |

**Transport selector field and values.**

| Field | Values |
|---|---|
| `type` | `stdio`, `http`, `sse`, `ws` (Claude Code); `local`, `stdio`, `http`, `sse` (Copilot CLI); `local`, `remote` (OpenCode); `streamableHttp` (Cline); `streamable-http` (Roo, Continue, LibreChat); `streamable_http` (goose); `websocket` (LibreChat) |
| `transport` | `http`, `sse`, `http-first` (Amp); `sse` only (Kimi Code, Hermes); `stdio`, `http`, `streamable-http` (Vibe); nested object (Cline CLI) |
| field name | `httpUrl` vs `url` (Gemini, Qwen) |

Normalize: `http` = `streamable-http` = `streamableHttp` = `streamable_http` =
`remote`. `local` = `stdio`.

**Variable syntax.**

| Syntax | Tools | Import note |
|---|---|---|
| `${VAR}` | Claude Code, Copilot CLI, Kiro, Factory, Amp, Hermes, LibreChat, Gemini, Qwen | Fields that expand differ per tool |
| `${VAR:-default}` | Claude Code | |
| `${env:VAR}` | VS Code, Cursor, Windsurf, Cline, Roo, Hermes | Cline expands only this form |
| `$VAR` | Gemini, Qwen (in `env`), Crush | |
| `$(command)` | Crush | Runs a command at load. Import must never evaluate it. |
| `{env:VAR}`, `{file:path}` | OpenCode | No `$` |
| `${{ secrets.X }}` | Continue | Secret store reference |
| `${input:id}` | VS Code | Prompted secret |
| `${file:path}` | Windsurf | |
| env var name in a field | Codex (`bearer_token_env_var`, `env_http_headers`, `env_vars`), Kimi (`bearerTokenEnvVar`), Vibe (`api_key_env`) | No interpolation needed |
| product tokens | `${amp:id-token}`, `{{LIBRECHAT_USER_ID}}`, `${workspaceFolder}` | Cannot map; keep or reject |

Unset-variable behavior differs: empty string (OpenCode), literal text kept
(Cline, Hermes outside remote fields), connect fails (Hermes remote fields),
approval prompt (Kiro).

**Enable flag.** `disabled: true` (Crush, Kiro, Cline, Roo, Factory) versus
`enabled: false` (OpenCode, Codex, Kimi Code, Hermes, goose). Copilot CLI,
Cursor CLI and Claude Code keep the state outside the server entry.

**Tool filters.** Allow: `enabled_tools`, `enabledTools`, `includeTools`,
`tools.include`, `tools` (Copilot). Deny: `disabled_tools`, `disabledTools`,
`excludeTools`, `tools.exclude`. Auto-approve (not a filter): `autoApprove`,
`alwaysAllow`, `trust`. Precedence differs: deny wins in Codex, Gemini, Qwen;
include wins in Hermes.

**Timeouts.** Units: ms (Claude Code, OpenCode, Copilot, Factory, Gemini,
Qwen, Kimi, LibreChat), s (Crush, Cline, Roo, Hermes, Codex `_sec`, Vibe
`_sec`). Meaning: startup, tool list fetch, or tool call. Copy only with the
unit and meaning known.

**Project file names.** `.mcp.json` is shared by Claude Code, Copilot CLI,
Warp (reads), and VS Code (portable read). Each other tool uses its own
directory: `.cursor/`, `.vscode/`, `.gemini/`, `.qwen/`, `.kiro/settings/`,
`.roo/`, `.factory/`, `.junie/mcp/`, `.kimi-code/`, `.amp/`, `.warp/`,
`.agents/`, `.codex/`, `.vibe/`, `.continue/mcpServers/`.

**Project trust.** Claude Code, Copilot CLI, Amp, Kimi Code, Codex and Kiro
(variables) gate project files. Crush runs project config as code.

## 5. Provisional format options for `ncp`

No decision here. All options assume: secrets live in the OS credential
store; the file holds references only; `ncp` never writes other tools' files.

### Option A: `mcpServers` JSONC superset (Claude Code / Copilot / Factory shape)

```jsonc
// ~/.config/ncp/mcp.json
{
  "mcpServers": {
    "github": {
      "type": "http",                                   // stdio | http | sse
      "url": "https://api.githubcopilot.com/mcp/",
      "headers": { "Authorization": "Bearer ${secret:github}" },
      "disabled": false,
      "disabledTools": ["delete_repo"],
      "timeout": 60000
    },
    "fs": { "type": "stdio", "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "${HOME}/src"] }
  },
  "x-ncp": { "cacheTtl": "30s" }
}
```

| For | Against |
|---|---|
| Paste-compatible with 18 tools and most server READMEs | JSON with comments; strict JSON readers reject it |
| Project file `.mcp.json` is already shared by Claude Code, Copilot CLI, Warp | `ncp` does not own `.mcp.json` semantics |
| Explicit `type` removes the `url` = SSE or HTTP ambiguity | `${secret:name}` is a new syntax that other tools leave as literal text |
| One parser for the native format and most imports | `disabled` vs `enabled`, `disabledTools` vs `excludeTools`: one name must be chosen |

### Option B: own TOML, Codex-like, with env-name and secret indirection

```toml
# ~/.config/ncp/config.toml
[servers.github]
url = "https://api.githubcopilot.com/mcp/"
transport = "http"
headers = { "X-Region" = "us" }
bearer = { secret = "github" }          # or { env = "GH_TOKEN" }
disabled_tools = ["delete_repo"]
startup_timeout = "10s"
tool_timeout = "60s"

[servers.fs]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "~/src"]
env = { LOG = "info" }
env_from = ["GITHUB_TOKEN"]             # forward by name
```

| For | Against |
|---|---|
| Comments and typed values; units in durations remove the ms/s problem | Not paste-compatible; needs `ncp import` |
| No string interpolation; secret and env are structured fields (Codex, Kimi, Vibe pattern) | Third TOML shape (Codex table, Vibe array, `ncp`) |
| `ncp` owns the schema and the project file name | Project file is a new name that no other tool reads |

### Option C: canonical model plus read-only adapters

`ncp` defines an internal server model (name, transport, command, args, env,
cwd, url, headers, auth, filters, timeouts). Its own file holds only `ncp`
settings and explicit references to foreign files. Adapters read the formats
in section 4. Hermes (`${VAR}` and `${env:VAR}`) and Warp (reads Claude Code,
Codex, `~/.agents/.mcp.json`) use parts of this pattern.

```toml
# ~/.config/ncp/config.toml
[[sources]]
path = "~/.claude.json"            # adapter chosen by path or by `format = "claude"`
[[sources]]
path = "~/.codex/config.toml"
only = ["context7"]

[servers.local-db]                 # native entries use the canonical model
command = "db-mcp"
```

| For | Against |
|---|---|
| No duplicate definitions; the user keeps one source of truth | About 10 adapters, each with its own variable rules and defaults |
| The user names each foreign file; this is the trust step | `url` default (SSE vs HTTP) and timeout units differ per source |
| `ncp` never writes foreign files | Precedence across sources is harder to explain and test |
| Native format choice (A or B) stays small | A foreign tool can change its format (Crush moved to Bash; Copilot changed `env` meaning) |

Open points for all options:

- Secret reference syntax: `${secret:name}`, `{ secret = "name" }`, or env var names only.
- Expansion of `${env:VAR}` as an alias of `${VAR}` (Hermes does both).
- Unset variable: empty, literal, or error. Hermes fails remote fields; that rule prevents a false "bad credential" result.
- Name for the enable flag and tool lists.
- `cwd` base: config file directory or process cwd.
- Read of `.mcp.json` and `~/.agents/.mcp.json` as project or shared sources.

## Sources

Official docs:

- OpenCode: [MCP servers](https://opencode.ai/docs/mcp-servers/), [Config](https://opencode.ai/docs/config/)
- Crush: [README](https://github.com/charmbracelet/crush), [docs/config/README.md](https://github.com/charmbracelet/crush/blob/main/docs/config/README.md), [schema.json](https://github.com/charmbracelet/crush/blob/main/schema.json)
- Copilot CLI: [Adding MCP servers](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers), [Using Copilot CLI](https://docs.github.com/en/copilot/how-tos/use-copilot-agents/use-copilot-cli), [changelog.md](https://github.com/github/copilot-cli/blob/main/changelog.md)
- Amp: [MCP](https://ampcode.com/docs/customize/mcp), [CLI settings](https://ampcode.com/docs/cli/settings), [settings schema](https://ampcode.com/cli-settings.schema.json)
- Kiro: [MCP configuration](https://kiro.dev/docs/mcp/configuration/)
- Cline: [Configuring MCP servers](https://docs.cline.bot/mcp/configuring-mcp-servers), [docs/mcp/mcp-overview.mdx](https://github.com/cline/cline/blob/main/docs/mcp/mcp-overview.mdx), [docs/getting-started/config.mdx](https://github.com/cline/cline/blob/main/docs/getting-started/config.mdx), [schemas.ts](https://github.com/cline/cline/blob/main/apps/vscode/src/services/mcp/schemas.ts), [envExpansion.ts](https://github.com/cline/cline/blob/main/apps/vscode/src/utils/envExpansion.ts)
- Roo Code: [Using MCP in Roo](https://roocodeinc.github.io/Roo-Code/features/mcp/using-mcp-in-roo), [repo (archived)](https://github.com/RooCodeInc/Roo-Code)
- Continue: [MCP deep dive](https://docs.continue.dev/customize/deep-dives/mcp), [config.yaml reference](https://docs.continue.dev/reference), [configuration](https://docs.continue.dev/advanced/deep-dives/configuration)
- Warp: [MCP](https://docs.warp.dev/agents/capabilities/mcp.md)
- Qwen Code: [docs/users/features/mcp.md](https://github.com/QwenLM/qwen-code/blob/main/docs/users/features/mcp.md)
- Kimi: [kimi-cli README (archived)](https://github.com/MoonshotAI/kimi-cli), [Kimi Code MCP](https://github.com/MoonshotAI/kimi-code/blob/main/docs/en/customization/mcp.md)
- Factory: [MCP](https://docs.factory.com/cli/configuration/mcp)
- Cursor CLI: [MCP](https://cursor.com/docs/cli/mcp)
- Augment: [Auggie integrations](https://docs.augmentcode.com/cli/integrations)
- Junie: [Junie CLI MCP configuration](https://junie.jetbrains.com/docs/junie-cli-mcp-configuration.html)
- JetBrains AI Assistant: [MCP](https://www.jetbrains.com/help/ai-assistant/mcp.html) (search summary only)
- Docker: [MCP Toolkit](https://docs.docker.com/ai/mcp-catalog-and-toolkit/toolkit/), [docker/mcp-gateway README](https://github.com/docker/mcp-gateway)
- LibreChat: [mcpServers object](https://www.librechat.ai/docs/configuration/librechat_yaml/object_structure/mcp_servers)
- Hermes Agent: [MCP config reference](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/reference/mcp-config-reference.md), [MCP guide](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/features/mcp.md)
- Mistral Vibe: [README](https://github.com/mistralai/mistral-vibe)
- OpenHands: [CLI MCP servers](https://docs.openhands.dev/openhands/usage/cli/mcp-servers.md)
- LM Studio: [MCP](https://lmstudio.ai/docs/app/mcp)
- Codex: [MCP](https://learn.chatgpt.com/docs/extend/mcp?surface=cli)
- Claude Code: [MCP](https://code.claude.com/docs/en/mcp)
- Zed: [MCP](https://zed.dev/docs/ai/mcp)
