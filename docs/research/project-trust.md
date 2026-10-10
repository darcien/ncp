# Study: trust of project configuration files in MCP clients

Date: 2026-10-10. Input for [project-config.md](../design/project-config.md).

**Method.** Research agents read the source and the documentation of
established clients on 2026-10-10: Claude Code, VS Code, Cursor, Codex,
Gemini CLI, Goose, Zed and OpenCode. The main session checked the facts
in the column "Checked". Small one-person CLIs are not evidence (user rule
of 2026-10-10). The session notes keep only a summary, so this report
gives the facts and their source names, not long quotes.

## 1. Facts

| Tool | Fact | Source | Checked |
|---|---|---|---|
| VS Code | The discovery of MCP servers in the files of other applications is off by default. A user had found that VS Code started a server from the configuration of Cursor. | Pull request #260209, which closes issue #258823 | Yes |
| VS Code | A workspace `mcp.json` reads comments and trailing commas. With duplicate keys, the last value wins. | `src/vs/base/common/json.ts` ([add-command.md](add-command.md) §4) | Yes |
| Codex | The trust of a project comes only from configuration layers outside the project. A project layer cannot make itself trusted. | Configuration loader README | Yes |
| Codex | `/import` copies servers from another tool one time, with a review. | Codex documentation | Yes |
| Codex | A project file could change settings that only the user should set. | CVE-2025-61260 | Plan reference |
| Gemini CLI | An untrusted folder in a headless run is a fatal error. An environment variable (`GEMINI_CLI_TRUST_WORKSPACE`) and a flag (`--skip-trust`) trust the folder for one session only. | Gemini CLI source | Yes |
| Cursor | An approved `mcp.json` entry could change after the approval and run with no new question. The fix asks again after a change. | CVE-2025-54136 | Plan reference |
| Claude Code | "Add from Claude Desktop" copies the servers one time, after a selection. | Claude Code documentation | Yes |
| Zed | A run of `git` in an untrusted folder could start a program of the project. | CVE-2026-44465 | Plan reference |
| MCP | The security best practices of the specification (2025-11-25, "Local MCP Server Compromise") tell a client to show the exact command of a local server, with no truncation, before it runs. | modelcontextprotocol.io, specification 2025-11-25, Security Best Practices | No (named by the review of 2026-10-10) |
| git | A repository in a folder of another user is not used unless the user marks it safe (`safe.directory`). | CVE-2022-24765 | Plan reference |

## 2. Common design

The clients that handle this risk use the same parts:

1. The trust record is outside the project, keyed by the real path of the
   project root.
2. An untrusted file is inert: it starts nothing and sends nothing.
3. A run with no person to ask fails closed. An explicit flag or variable
   overrides this for one run, and is not saved.
4. A change of an approved entry needs a new approval (Cursor after its
   CVE, VS Code with a nonce, Codex with the hash of a hook).

## 3. Consequences for `ncp`

- Keep the approvals in the state folder of the user, keyed by the real
  path of the project root, with a hash of each entry.
- Ask at the first use on a TTY. Refuse with no TTY. Give a flag for one
  run that `ncp` does not save.
- Find `.git` with no run of `git`, and stop the search at a folder of
  another user.
- Do not read files of other applications (VS Code #260209).
