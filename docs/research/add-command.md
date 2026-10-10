# Study: add a server from the command line, and edit the configuration file

Date: 2026-10-10. Input for the `server add` proposal (backlog) and the
secret binding proposal ([secret-binding.md](../design/secret-binding.md)).

**Method.** The main session and research agents read official documents
and source code on 2026-10-10 (default branches). One agent ran a small test
program for `jsonc-parser` in a scratch directory. Nothing was installed in
the project. "Unknown" means that the source that was read does not state
the fact. add-mcp (Neon) is context only, not evidence (AGENTS.md).

## 1. Add commands of MCP clients

| Tool | Form | Same name | Secrets | Login | Source |
|---|---|---|---|---|---|
| Claude Code | `claude mcp add --transport http <name> <url>` (default transport stdio); `<name> -- <cmd> [args]`; `-e K=V`, `-H "K: V"`, `-s local\|project\|user`; `--client-secret` asks with masked input | Error: "MCP server sentry already exists in local config" | Header values are literals in the file | Not at add: `/mcp` or `claude mcp login`. `remove` deletes the stored tokens and client registration. | <https://code.claude.com/docs/en/mcp> |
| Codex | `codex mcp add <name> (--url <URL> \| -- <cmd>...)`; `--env K=V`; `--bearer-token-env-var NAME` | Overwrites, no message | Bearer token by variable name only | After the write, one OAuth discovery request for a URL entry; if supported, it starts the browser login | `openai/codex` `codex-rs/cli/src/mcp_cmd.rs` |
| Gemini CLI | `gemini mcp add [opts] <name> <commandOrUrl> [args]`; `-t` (default stdio), `-e`, `-H`, `-s user\|project` | Overwrites, with the messages "is already configured" and "updated" | Literals in the file | None | `google-gemini/gemini-cli` `packages/cli/src/commands/mcp/add.ts`, `docs/tools/mcp-server.md` |
| gh (not MCP) | `gh secret set NAME` | — | Interactive prompt; stdin from a pipe | — | <https://cli.github.com/manual/gh_secret_set> |
| add-mcp 2.4.1 (context) | `npx add-mcp <url\|package>`; name from the URL; writes the files of about 25 agents | — | Literals in the files; asks for `${VAR}` | None | npm registry, `neon-solutions/add-mcp` README |

Common parts: a name first, `-- <command>` for stdio, repeatable env and
header flags. The transport flag differs: Claude Code and Gemini CLI need
`--transport http`; Codex uses `--url`.

## 2. Contact with the server at add

- Claude Code: "The `claude mcp add` command saves the configuration
  without validating credentials". An `Added ...` line means that the
  configuration was written. The user checks with `claude mcp get <name>`.
- Gemini CLI: the add code builds the entry and saves it. It makes no
  network call.
- Codex: writes first. Then, for a URL entry, one OAuth discovery request
  decides if it starts a login. When discovery fails, it prints that the
  server "may or may not require login" and keeps the entry.

No checked client refuses to write when the server cannot be reached.

## 3. How CLIs edit a configuration file that people also edit

| Tool | File | Method | Source |
|---|---|---|---|
| `deno add` / `remove` | `deno.json`, `package.json` | `jsonc_parser::cst`: parse, insert or remove one property, write `cst.to_string()` | `denoland/deno` `cli/tools/pm/mod.rs` |
| Codex | `config.toml` | `toml_edit` | `openai/codex` `codex-rs/core/Cargo.toml` |
| uv | `pyproject.toml` | `toml_edit` ("Edit project and script metadata while preserving TOML formatting") | `astral-sh/uv` 0.13.0 `crates/uv-project-edit/src/lib.rs` |
| `cargo add` | `Cargo.toml` | `toml_edit::DocumentMut` | `rust-lang/cargo` 0.99.0 `src/cargo/util/toml_mut/manifest.rs` |
| VS Code | workspace `.mcp.json` | `jsonEdit.setProperty` and `applyEdits`; keeps indent, line ends and comments | `microsoft/vscode` `mcpResourceScannerService.ts` (`editJsonProperty`) |
| VS Code | user `mcp.json`, `.vscode/mcp.json` | `JSON.stringify(..., '\t')`; comments are lost | same file (`writeScannedMcpServers*`) |
| Gemini CLI | `settings.json` | `comment-json` parse and stringify; keeps comments; no write when the file does not parse | `packages/cli/src/utils/commentJson.ts` |
| Zed | `settings.json` | tree-sitter edits of the changed keys only; keeps comments | `zed-industries/zed` `crates/settings_json/src/settings_json.rs` |
| npm | `package.json` | round trip; keeps the indent and line-end style that it found | `npm/json-parse-even-better-errors` `lib/index.js` |

Conclusion: tools that edit a file that people also edit change only the
changed part (a syntax tree or text edits). A full rewrite loses comments
(VS Code user `mcp.json`).

## 4. JSON dialect of MCP configuration files

| Tool | File | Read | Source |
|---|---|---|---|
| VS Code | `mcp.json`, `.mcp.json` | Comments and trailing commas (`allowTrailingComma: true`). Duplicate key: last value wins. | `src/vs/base/common/json.ts`, `mcpResourceScannerService.ts` |
| VS Code | `~/.copilot/mcp-config.json` | Strict JSON | `mcpCopilotGlobalConfiguration.ts` |
| Zed | `settings.json` | Comments and trailing commas (`serde_json_lenient`) | `crates/settings_json/src/settings_json.rs` |
| Gemini CLI | `settings.json` | Comments; trailing commas rejected (strip-json-comments 3.1.1). Duplicate key: last value wins. | `packages/cli/src/config/settings.ts` |
| Claude Code | `settings.json` | A `//` comment gives "Invalid or malformed JSON". `.mcp.json`: unknown. | issues #17968 (open), #29370, #83190 |
| Cursor | `mcp.json` | Unknown | <https://cursor.com/docs/context/mcp> |

No checked tool reads JSON5 forms (single quotes, unquoted keys).

## 5. Test of jsonc-parser 0.33.2

Version 0.33.2 (published 2026-09-12), features `cst` and `serde`.

| Check | Result |
|---|---|
| Default options | Accept comments, trailing commas, single quotes, unquoted keys, hex numbers, unary plus and missing commas (`src/parse_to_ast.rs:68-80`) |
| Only `allow_comments` and `allow_trailing_commas` | JSON5 forms give errors, for example "Single-quoted strings are not allowed on line 1 column 2" |
| Duplicate keys | No error with any option; the last value wins, also in `mcpServers` (`src/serde.rs:347`) |
| Error text | "Unexpected close bracket on line 3 column 8", with byte range, line and column (`src/errors.rs:148-200`) |
| CST round trip | Parse, then `to_string()`: same bytes |
| CST insert into `mcpServers` | Every other byte stays: comments, unknown keys, the 4-space indent, the trailing-comma style |
| CST insert into `{}` or an empty file | Full object, indent of 2 spaces (`src/cst/mod.rs:3792`) |
| CST remove | Commas stay correct; a comment on the line above the removed entry stays (`src/cst/mod.rs:3285-3291`) |
| CST `to_serde_value` | Needs the feature `serde_json` (`Cargo.toml:37`) |

## 6. Consequences for `ncp`

- `server add` does not contact the server (section 2).
- `ncp` edits the configuration file through a concrete syntax tree, as
  Deno does (section 3). Every byte that `ncp` does not change stays.
- The loader reads comments and trailing commas, and no JSON5 forms
  (section 4). It writes strict JSON.
- Duplicate keys: the last value wins, as `serde_json` does today.

## 7. Credential keying in CLIs

Question: how does a CLI key a stored credential, and does it send the
credential only to the matching host?

| Tool | Key | Port in key | Sent only to the match | Source |
|---|---|---|---|---|
| git | protocol, host, path (path only with `credential.useHttpPath`) | Yes: `host` "includes the port number if one was specified" | Yes | <https://git-scm.com/docs/git-credential>, <https://git-scm.com/docs/gitcredentials> |
| netrc (curl) | `machine` (host name) | No: "independent of which protocol and port number" | Yes; a `default` entry matches all hosts | `curl/curl` 8.22.0 `docs/cmdline-opts/netrc.md` |
| docker | `auths` key: registry host | Yes (`host:port`) | By registry host. Token auth can also send it to the realm of the registry challenge (not checked in the source). | `docker/cli` v29.9.0 `cli/config/credentials/file_store.go`; <https://docs.docker.com/reference/cli/docker/login/> |
| npm | `//host/path/:_authToken` | Yes, for a port that is not the default | Yes: scoping exists so that "npm will never send credentials to the wrong host" | `npm/cli` `workspaces/config/lib/nerf-dart.js`; <https://docs.npmjs.com/cli/v10/configuring-npm/npmrc> |
| gh | `hosts.<hostname>`; keyring service `gh:<hostname>` | Unknown | To the host and its API host | `cli/cli` v2.102.0 `internal/config/config.go` |
| cargo | registry name; some credential providers key by index URL | Not applicable | Each token to its registry | <https://doc.rust-lang.org/cargo/reference/registry-authentication.html> |
| uv | service URL and user name; match needs the same realm (scheme, host, port), then the longest path prefix | Yes; a default port counts as no port | Yes (RFC 7235 §2.2 realm) | `astral-sh/uv` 0.13.0 `crates/uv-auth/src/realm.rs`, `store.rs`; <https://docs.astral.sh/uv/concepts/authentication/http/> |

Conclusion: each checked credential tool keys a credential by its
destination host and sends it only there. Most keep the port in the key
(git, docker, npm, uv); netrc does not. MCP clients key secrets by a
variable name, with no destination (section 1).
