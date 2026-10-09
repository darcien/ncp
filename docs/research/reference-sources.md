# Note: which sources count as evidence

Date: 2026-10-10. This note does not change the older reports. It sets how
to read them.

**Rule** (AGENTS.md): prior art for a decision comes from the official MCP
specification, SDKs and test parts, and from tools of established vendors
(frontier AI labs and major editors). A small project of one person is
context, not evidence.

**Evidence.** The MCP specification and its official extensions, the official
SDKs (TypeScript, Python, Go, Rust `rmcp`), the MCP Inspector and the
conformance suite; Claude Code and Claude Desktop, OpenAI Codex CLI, Gemini
CLI, VS Code, Cursor, Zed and Goose.

**Context only.** These tools appear in older reports
(`interface-and-config.md`, `agent-config-formats.md`, `go-clients.md`,
`rust-clients.md`, `mcp-client-oauth.md` and others). Their behavior is no
reason for a decision:

- `openclaw/mcporter`, `apify/mcpc`, `lydakis/mcpx`, `philschmid/mcp-cli`,
  `knowsuchagency/mcp2cli`, `f/mcptools`, `IBM/mcp-cli`
- Factory droid

A decision that took an idea from one of these tools keeps its own reason,
if it has one. When such a decision changes, check it against the evidence
above.
