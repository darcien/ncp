# A daemon for stateful MCP servers

Date: 2026-11-07. Requirements F4 and S6.

## Question

Must `ncp` get a daemon, so that a server keeps its state between runs?
Does the removal of protocol sessions in `2026-07-28` remove the need?

Terms:

- Handle: an ID that a server makes and returns in a tool result. The
  model sends it back as a tool argument.
- Holder: a daemon that keeps stdio servers alive between runs, and
  nothing more.

## Determinations

These determinations are proposals. The maintainer decides.

| Option | Fixes | Verdict |
|---|---|---|
| A daemon for speed | About 440 ms of a run of 470 ms ([connection-latency.md][latency]) | Do not |
| A daemon for state | Stateful stdio servers, and HTTP servers of `2025-11-25` that keep state in a session | Do not, for now |
| An opt-in holder for each stdio server | Stateful stdio servers | Defer. Needs an RFC. |
| Keep the session ID of an HTTP server between runs | HTTP servers of `2025-11-25` that keep state in a session | Defer. Not tested. Needs an RFC. |

- The spec removes the problem only for HTTP servers of `2026-07-28` with
  handles. The server process lives on, so the state lives on.
- The spec does not remove the problem for stdio servers. A handle names
  state in the server process, and `ncp` stops the process after each run.
- The demand is small, and the vendors give these users a CLI that keeps
  state.
- Write the RFC for a holder when a stateful stdio server meets all these
  conditions:
  - Users of `ncp` need it.
  - It has no vendor CLI.
  - It keeps its state in its process after it adopts `2026-07-28`.

## Evidence

### The spec

- `2026-07-28` removes protocol sessions. "Servers that need cross-call
  state use explicit, server-minted handles passed as ordinary tool
  arguments" ([changelog][chg], item 1).
- "The server owns the state, the client holds a name for it"
  ([SEP-2567][sep2567-139]).
- Stateful stdio servers "continue to function with a stdio client that
  spawns one process", but "SHOULD NOT rely on process-lifetime state"
  ([SEP-2567][sep2567-237]).
- A server that keeps state in a session stays on `2025-11-25` until it
  changes. There is no deprecation window ([SEP-2567][sep2567-249]).
- A new session for each tool call is a known client behavior: "ChatGPT
  creates a fresh session for every individual tool call"
  ([SEP-2567][sep2567-47]).
- The spec gives no rule for how long a client keeps a stdio server alive
  ([stdio][stdio]).
- Task polling (`tasks/get`) and `subscriptions/listen` need a server that
  stays alive ([changelog][chg], items 4 and 6). For `ncp`, both are MAY
  (P13).

### What a run of `ncp` keeps

At the end of each run, `ncp` stops each stdio server
(`crates/ncp/src/client/stdio.rs`), and rmcp 3.5.1 deletes the HTTP
session. Only the cache on disk stays.

### Clients

| Client | Keeps servers between runs | Stated reason |
|---|---|---|
| `mcpdo` (official, experimental) | Yes, until 60 s after the last connection | "multi-step flows over one session" ([mcpdo][mcpdo]) |
| mcp2cli | Opt-in named session | Startup cost ([SKILL.md][mcp2cli]) |
| philschmid/mcp-cli | Yes by default, 60 s idle | Startup latency ([README][philschmid]) |
| avelino/mcp | No | — ([README][avelino]) |
| mcporter | Opt-in for each server | Keep-alive servers, Chrome DevTools ([daemon.md][mcporter]) |
| playwright-cli (vendor, not MCP) | Yes, a named browser session | State between runs ([README][pwcli]) |

Only `mcpdo` gives state as the reason. Playwright tells coding agents to
use its CLI: "If you are using **coding agents**, that is the best fit"
([README][pwcli]).

### Stateful servers in use

From [connection-latency.md][latency], "Browser servers":

- Browser servers get about 45 % of the MCP calls, in chains of 7 to 10
  calls. This report did not get the 45 % again from the table there.
- The use is narrow: 45 of 4,852 sessions, from about 12 of 173 users.
- Playwright MCP 0.0.83 failed a chain across three runs of `ncp`, over
  stdio and over HTTP.
- The release notes of Playwright MCP [v0.0.84][pw084] do not name
  `2026-07-28` or handles.

### Cost of a daemon

| Area | Cost |
|---|---|
| Secrets (S1–S3) | Tokens and `env` values stay in memory. `mcpdo` has no maximum lifetime while a connection is open ([security][security]). |
| Credential store (S2) | With no keychain, `mcpdo` uses a plaintext `secrets.json` that two processes share ([security][security]). |
| Environment (N2, N5) | The daemon keeps the environment of its start. `ncp` takes `${NAME}` from the environment of the caller first. |
| Trust (S5) | The daemon must check the trust of the project file at each call. |
| Upgrade | Old code runs until a person stops the daemon ([mcpdo][mcpdo]). |
| Cancel and timeouts (P10, P15) | The cancel must cross a socket. |
| Shared state | "Selecting a page in one caller can change the target of another caller's later operation" ([daemon.md][mcporter]). |

### Smallest holder

If the trigger occurs:

- Opt-in for each server. With no holder, `ncp` works as it does now, as
  `ssh` does with no ControlMaster.
- Stdio servers only. No HTTP connection and no token in the holder.
- S6: a socket with mode `0600` in a `0700` directory and a check of the
  peer user ID. `docs/design/cli.md` reserves the path.
- The run of `ncp` reads the credential store, checks the trust and keeps
  its timers. A digest of the command and the environment selects the
  holder.

## Open questions

- Test 1: Playwright MCP over stdio with `--cdp-endpoint` to a Chrome that
  lives longer than the server. If a chain works across three runs, the
  state can live outside the server process.
- Test 2: Playwright MCP over HTTP, started by the user, with the session
  ID kept between runs and no `DELETE`. If a chain works, a stored session
  ID fixes HTTP servers of `2025-11-25` with no daemon.
- Do the next releases of Playwright MCP and Chrome DevTools MCP adopt
  handles, with the state outside the process?

## Method

The links give the commit of each source. Two reviewers (agents) made the
case for and against a daemon from the same sources. No test ran.

[latency]: connection-latency.md
[chg]: https://modelcontextprotocol.io/specification/2026-07-28/changelog
[stdio]: https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio
[sep2567-47]: https://github.com/modelcontextprotocol/modelcontextprotocol/blob/c9b51b3a02/seps/2567-sessionless-mcp.md#L47
[sep2567-139]: https://github.com/modelcontextprotocol/modelcontextprotocol/blob/c9b51b3a02/seps/2567-sessionless-mcp.md#L139
[sep2567-237]: https://github.com/modelcontextprotocol/modelcontextprotocol/blob/c9b51b3a02/seps/2567-sessionless-mcp.md#L237
[sep2567-249]: https://github.com/modelcontextprotocol/modelcontextprotocol/blob/c9b51b3a02/seps/2567-sessionless-mcp.md#L249
[mcpdo]: https://github.com/modelcontextprotocol/modelcontextprotocol/blob/c9b51b3a02/docs/docs/2026-07-28/tools/inspector/mcpdo.mdx
[security]: https://github.com/modelcontextprotocol/modelcontextprotocol/blob/c1c8107/docs/docs/2026-07-28/tools/inspector/security.mdx
[mcp2cli]: https://github.com/knowsuchagency/mcp2cli/blob/cf81e493b2/skills/mcp2cli/SKILL.md
[philschmid]: https://github.com/philschmid/mcp-cli/blob/d77672a1ce/README.md
[avelino]: https://github.com/avelino/mcp/blob/f909f8b08f/README.md
[mcporter]: https://github.com/openclaw/mcporter/blob/21624e8cc8/docs/daemon.md
[pwcli]: https://github.com/microsoft/playwright-cli/blob/4bc00fb7fa/README.md
[pw084]: https://github.com/microsoft/playwright-mcp/releases/tag/v0.0.84
