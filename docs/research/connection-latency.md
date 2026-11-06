# Time of the server start, the stop and the HTTPS handshake

Date: 2026-11-06.

## Question

In real sessions, do the start of a stdio server and the HTTPS handshake
cost enough time to optimize?

Terms:

- RTT: the time of one network round trip.
- Legacy server: a server that speaks `2025-11-25` and answers
  `server/discover` with "Method not found".
- Cached run: a run with the protocol version of the server in the cache
  of `ncp`. A first contact is a run with no cached version.
- Call cycle: the time from an MCP call to the next step of the agent.
- Turn wait: the time from a prompt to the last step of the agent.

## Determinations

These determinations are proposals. The maintainer decides.

### Cost of a run

An agent runs `ncp` one time for each MCP tool call. This table
multiplies the cost of a run by the counts of "SWE-chat analysis". The
rows at 100 ms RTT are estimates (see "Round trips at a distance"). The
call cycle and the turn wait are the denominators, because a session
includes the time that the person thinks.

| Cost of `ncp` for each call | For each call | Part of a call cycle (7.2 s) | p90 turn (9 calls) | p90 session (26 calls) |
|---|---|---|---|---|
| Spawn | under 2 ms | under 0.03 % | under 0.02 s | under 0.05 s |
| TCP and TLS, Cloudflare (near) | about 23 ms | 0.3 % | 0.2 s | 0.6 s |
| Boot of a Node stdio server | about 80 ms | 1.1 % | 0.7 s | 2.1 s |
| `server/discover` at 100 ms RTT | about 100 ms | 1.4 % | 0.9 s | 2.6 s |
| `npx` with no download | 111–182 ms | 1.5–2.5 % | 1.0–1.6 s | 2.9–4.7 s |
| TCP and TLS at 100 ms RTT | about 200 ms | 2.8 % | 1.8 s | 5.2 s |
| Stop wait of the everything server | about 360 ms | 5.0 % | 3.2 s | 9.4 s |
| Stop wait of a server that does not exit at EOF | about 2 s | 28 % | 18 s | 52 s |

- The stop wait is the largest cost of a run. For a server that does not
  exit at EOF, the 18 s is about 14 % of the wait of the p90 MCP turns.
  The statistics review computed this share from those turns.
- A turn with the median number of MCP calls (2) loses less than 1 s to
  `ncp` in each row, except the last row (4 s).
- Nobody has measured which common servers do not exit at EOF. That
  number decides the size of the largest cost.

### Verdicts

| Change | Saves | Verdict | Reason |
|---|---|---|---|
| A step for the stop and the `DELETE` in `--timings` | No time. It shows up to 6.5 s that now looks like a slow `ncp`. | Do | It is the purpose of `--timings`. One field and one timer, with no change of behavior. |
| A shorter wait before SIGTERM | Up to about 1.8 s for each call to a server that does not exit at EOF: about 16 s in a p90 turn | Do not, for now | 2 s is the lowest wait of the official SDK clients. The spec puts the duty on the server. SIGTERM can stop the cleanup of a server. Decide again if a measurement shows that common servers do not exit at EOF. |
| The result before the stop, for each command | Time to the first line on a TTY only | Defer | A pipe, a shell and an agent wait for the exit of `ncp`. |
| Skip `server/discover` with a cached `2026-07-28` version | 1 RTT on each run | Defer | The spec allows it. rmcp 3.5.1 cannot do it. |
| Skip `tools/list` before `tools/call` | 1 RTT on each run | Defer | Only a narrow form is safe: a cached tool with no `x-mcp-header`, and a new list and one retry after a header mismatch. |
| No GET stream, no `DELETE`, no wait for the 202 | 1–2 RTT, legacy servers only | Do not | `2026-07-28` removes all three. Each can break a legacy server, and rmcp has no option for them. |
| TLS sessions on disk | 0 RTT, some CPU | Do not | rustls cannot do it, and a session file is a new stored format. |
| webpki roots in place of the platform verifier | About 2 ms, not significant | Do not | `SSL_CERT_FILE` already gives this choice. See `custom-ca-certificates.md`. |
| A daemon or a keep-alive in `ncp` | The boot, the handshake, `server/discover` and the stop wait. For the everything server: about 440 ms of a run of 470 ms, 6 % of a call cycle, 4 s in a p90 turn. For a server that does not exit at EOF: about 2 s. | Do not | The saving is real, but MCP calls are rare: the median session has none. A daemon touches many MUST rules of `docs/requirements.md`, keeps secrets in memory, and adds a second program with its own lifetime. |
| A documented gateway for a stdio server | Same as a daemon | Defer | No official gateway exists, and a local port is not an authentication boundary. |

### Possible defects

- A cached `server show` of the everything server shows a Node stack
  trace from the server. `ncp` causes it, because it closes stdout before
  the server writes its notification (see "Stop of a stdio server").
- `ncp` has a handler only for SIGINT. If a caller sends SIGTERM or
  SIGHUP to `ncp`, a stdio server that does not exit at EOF can stay
  alive with no parent. This comes from a code read, and nobody tested
  it.
- A first contact with a legacy server that does not exit at EOF takes
  about 4 s, because each of the two processes gets the full stop wait.

## Method

- `ncp`: commit `bf8abbb`, release build of the test binary `ncp-cli`
  (fake Keychain). `rmcp` 3.5.1, `reqwest` 0.13.5, `hyper` 1.11.1,
  `rustls` 0.23.46 (`ring`), `rustls-platform-verifier` 0.7.1, `tokio`
  1.54.1.
- Host: macOS 15.7.4, arm64. Node 22.23.3. `hyperfine` 2.0.0.
- Each first contact used a new, empty home directory.
- Local servers: the rmcp fixture of the test kit (`ncp-fixture`), and
  `@modelcontextprotocol/server-everything` `2026.8.31` from
  `harness/interop/pnpm-lock.yaml`, on stdio and on Streamable HTTP.
- One remote server: `https://docs.mcp.cloudflare.com/mcp`. The
  maintainer approved this host. No other MCP server got a request.
- Tools of the test:
  - `--timings` and `-vv` of `ncp`.
  - A wrapper that writes the start time and the exit time of the server
    process.
  - A TCP proxy in Python that counts the connections and can add a delay.
  - A driver that sends JSON-RPC lines to a stdio server with no `ncp`.
- A second person (an agent) did an adversarial review. It ran the main
  measurements again and read the code again. This report includes its
  corrections. The reviewers opened each source of the prior art, and
  made the strongest case against each change.
- The test code is in `.scratch/research/connection-latency/` (not in
  git).

## Evidence

### What `--timings` measures

- `server start` is the time of `spawn()` only. The boot of the server
  is in the first value of `responses`.
- `connect` is DNS, TCP, the proxy `CONNECT` and TLS together.
  `--timings` takes the DNS time out.
- The response time of the first request includes the new connection.
- `total` includes the stop of a stdio server and the `DELETE` of an HTTP
  session, because `ncp` writes the timings after the runtime stops
  (`crates/ncp/src/cli.rs`). No step of its own shows them.
- On macOS, the first exec of a new binary file costs about 290 ms. The
  first value of `responses` includes it.

### Stdio servers

All rows are cached runs, unless the row says otherwise.

| Server and command | Total | Boot and handshake | Stop |
|---|---|---|---|
| Fixture (Rust), `server show` | 4.4–5.6 ms | 3–7 ms | 0 ms |
| everything (Node), `server show` | 77–123 ms | 74–115 ms | 3–4 ms (crash, see "Stop of a stdio server") |
| everything, `tool list` | 440–475 ms | 78–109 ms | about 360 ms |
| everything, `tool call` | 471–477 ms | 108–110 ms | about 360 ms |
| everything through `npx --offline --no`, `tool list` | 607 ms | about 200–250 ms | about 360 ms |
| everything, first contact, `server show` | 525–535 ms | 2 × 74–91 ms | about 360 ms |
| Fixture with `linger` (exits only on SIGTERM) | 2009–2013 ms | 3–7 ms | about 2000 ms |
| Fixture with `linger`, first contact | 4028 ms | 3–7 ms | 2 × about 2000 ms |

- `ncp server list` (no connection): 2.8 ms ± 0.4 ms, 30 runs. The CPU
  time of a run with a connection is 5–14 ms. The spawn costs 0.2–1.7 ms.
- In a `-vv` trace, Node needs 75–79 ms from the spawn to the first
  stderr line of the server. The handshake after that takes about 8 ms.
- `npx` adds 111–182 ms with no network. An entry that starts the
  installed server with `node` does not pay this cost.
- A first contact with a legacy server starts the server two times: one
  time for `server/discover`, and one time for `initialize`. The first
  process gets all the stop steps before the second start
  (`crates/ncp/src/client.rs`).

### Stop of a stdio server

- `Connection::close()` waits up to `CANCEL_LIMIT` (2 s) for rmcp. Then
  it closes stdin and waits up to `STOP_WAIT` (2 s,
  `crates/ncp/src/client/stdio.rs`) for the exit. Then it sends SIGTERM,
  waits 2 s more, and kills the process. Last, it waits up to `LOG_WAIT`
  (0.5 s) for the stderr of the server. The upper limit is thus about
  6.5 s.
- `tool call` writes its result before `close()`. `server show`,
  `tool list`, `tool show`, `resource list`, `resource read`,
  `prompt list` and `prompt get` write their result after `close()`
  (`crates/ncp/src/cli/commands.rs`). For each command, `ncp` exits
  after the stop.
- The driver sent messages to the everything server with no `ncp`, then
  closed stdin:

  | Messages before the close | Exit after the close |
  |---|---|
  | `initialize` and `notifications/initialized` | 358.8–359.2 ms |
  | `initialize` only | 2.2–2.5 ms |
  | `server/discover` only | 2.3–2.9 ms |

- The cause of the 360 ms is in the server:
  - After `notifications/initialized`, the everything server starts a
    timer of 350 ms (`dist/server/index.js`, line 78).
  - The stdio transport of the TypeScript SDK 1.32.1 does not stop when
    stdin ends. Thus Node lives until the timer ends.
- The cause of the fast stop of a cached `server show`:
  - `close()` closes stdin and the read end of stdout together.
  - A cached `server show` sends no request after the handshake. Thus
    `ncp` closes stdout before the server writes
    `notifications/tools/list_changed`.
  - The write fails with EPIPE. The server has no handler for it, so it
    stops with exit code 1 and writes a Node stack trace to stderr.
    `ncp` shows these lines as lines of the server.
  - The reviewer tested this. `--no-cache server show` (no cache write)
    crashed in 3 of 3 runs. `--refresh server show` (with a cache write
    before the close) crashed in 0 of 3 runs.
- Playwright MCP 0.0.83 exits when stdin closes. The stop after a
  snapshot took about 190 ms. The stop after a navigate took about
  1.4 s, because Chrome closed with a page open.

Steps of other clients after the close of stdin:

| Client | Steps |
|---|---|
| TypeScript SDK [1.32.1](https://github.com/modelcontextprotocol/typescript-sdk/blob/1.32.1/src/client/stdio.ts) and [v2.4.0](https://github.com/modelcontextprotocol/typescript-sdk/blob/v2.4.0/packages/client/src/client/stdio.ts) | 2 s, SIGTERM, 2 s, SIGKILL |
| Python SDK [v2.3.0](https://github.com/modelcontextprotocol/python-sdk/blob/v2.3.0/src/mcp/client/stdio.py) | 2 s, terminate, 2 s, kill |
| Go SDK [v1.8.0](https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/mcp/cmd.go) | 5 s, SIGTERM, 5 s, kill |
| VS Code [1.145.1](https://github.com/microsoft/vscode/blob/1.145.1/src/vs/workbench/contrib/mcp/node/mcpStdioStateHandler.ts) | 10 s, then 10 s |
| Codex CLI [rust-v0.167.0](https://github.com/openai/codex/blob/rust-v0.167.0/codex-rs/rmcp-client/src/stdio_server_launcher.rs) | SIGTERM at once, no close of stdin |
| MCP Inspector CLI [2.14.0](https://github.com/modelcontextprotocol/inspector/blob/2.14.0/clients/cli/src/cli.ts) | Writes the result, then disconnects with the TypeScript SDK steps, then exits |

- The `2026-07-28` stdio transport puts the duty on the server: "Servers
  SHOULD exit promptly when their standard input is closed or reads
  return end-of-file"
  ([stdio](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio)).
  The spec gives no number for the wait of the client.
- The TypeScript SDK v2 server transport now stops at the end of stdin
  ([PR 2494](https://github.com/modelcontextprotocol/typescript-sdk/pull/2494)).
  The fix for v1 was not merged on the date of this report.

### HTTP servers

#### Loopback: the everything server (HTTP/1.1, `2025-11-25`)

- `server show`: 6–17 ms. `tool call`: 9–21 ms. `connect`: 0.2–0.5 ms.
- A `tool call` sends 6 requests in this order:
  1. POST `initialize`.
  2. POST `notifications/initialized`. `ncp` waits for the `202`.
  3. GET for the event stream, at the same time as step 4.
  4. POST `tools/list`.
  5. POST `tools/call`. `ncp` writes the result here.
  6. `DELETE` of the session.
- Each run opens 2 TCP connections. The GET stream holds the first
  connection. Thus the next POST, or the `DELETE` of `server show`, opens
  the second connection on the critical path.
- The protocol version does not cause HTTP/1.1. The server used HTTP/1.1
  because plain HTTP on loopback has no ALPN. reqwest offers `h2` and
  `http/1.1` over TLS.
- With a delay of 25 ms in each direction (50 ms RTT) in the proxy:

  | Command | Total |
  |---|---|
  | `server show`, cached | 172–181 ms |
  | `tool list` | 226–230 ms |
  | `tool call` | 288–289 ms |

  One POST through the proxy took 55–60 ms, because the proxy adds its
  own time. Thus a `tool call` is 5 round trips. The proxy accepts each
  TCP connection on loopback, so these totals have no TCP or TLS round
  trip.

#### Remote: `docs.mcp.cloudflare.com` (`2026-07-28`, TLS 1.3, HTTP/2)

| Part | Time |
|---|---|
| DNS, first lookup | 50.2 ms |
| DNS, cached by the system | 1.2–2.6 ms |
| TCP (curl) | 8.7–10.0 ms |
| TLS 1.3 (curl) | 12.3–17.7 ms |
| `connect` of `ncp` (TCP and TLS) | 20.0–32.0 ms |
| Request on an open connection | 14.1–20.9 ms |
| `server show`, cached, 30 runs | mean 61.2 ms, range 46.1–203.9 ms. One outlier raises the mean. |
| `server show`, cached, 4 other runs | 41.1–42.4 ms |
| `tool list`, cached | 57.4–78.8 ms |

- `tool list` sends 2 POST requests on 1 connection: `server/discover`
  and `tools/list`. There is no GET, no `notifications/initialized` and no
  `DELETE`.
- `ncp` sends `server/discover` on each run, also with a cached
  `2026-07-28` version (`crates/ncp/src/client.rs`).
- For a `tool call` to a `2026-07-28` HTTP server, `ncp` always sends
  `tools/list` first. rmcp builds the `Mcp-Param-*` headers only from the
  tools that it listed in the session (`mirrors_parameters`,
  `crates/ncp/src/cli/commands.rs`).
- The handshake (`connect`) is about 23 ms of a `tool list` run of
  57.4–78.8 ms.
- Verification of the certificate: 5 + 5 alternated runs with the
  platform verifier and with `SSL_CERT_FILE=/etc/ssl/cert.pem` gave
  `connect` medians of 24.9 ms and 22.6 ms. The difference is about 2 ms,
  inside the spread. An earlier test with runs that were not paired gave
  ranges that overlap. `SSL_CERT_FILE` also changes the roots, so the
  test does not isolate the verifier.

#### Round trips at a distance

These counts are estimates from the measurements and the code. The cached
DNS time is small.

- `2026-07-28` server over HTTP/2: 2 RTT for TCP and TLS, then 1 RTT for
  each request. A `tool call` sends `server/discover`, `tools/list` and
  `tools/call`. Thus the handshake is 2 of 5 round trips. At 100 ms RTT,
  that is about 200 ms of about 500 ms.
- Legacy server over HTTPS with only HTTP/1.1: 2 connections, each with
  TCP and TLS, and 5 requests. A `tool call` is thus about 9 round trips
  to the exit, and 4 of them are TCP and TLS.

### MCP calls in real sessions

No published source gives the number of MCP calls in a real session.
These sources give the nearest numbers. I opened each source on
2026-11-06.

| Source | Data | Number |
|---|---|---|
| Anthropic, [Agentic coding and persistent returns to expertise](https://www.anthropic.com/research/claude-code-expertise) (2026-06-16) | 400,000 Claude Code sessions, October 2025 to April 2026 | About 4 user turns in a typical session, and about 10 actions (tool calls of any kind) for each turn |
| Anthropic, [Measuring AI agent autonomy in practice](https://www.anthropic.com/research/measuring-agent-autonomy) (2026-02-18) | Claude Code sessions, late 2025 to early 2026 | The median turn takes about 45 s (40–55 s). |
| SWE-chat, Baumann et al., [arXiv 2604.20779](https://arxiv.org/abs/2604.20779) | Real sessions of open-source developers | Version 1, about 6,000 sessions: 6,001 MCP calls of 355,000 tool calls (1.9 %). Version 2, almost 18,000 sessions: 27,327 of 1,996,558 (1.4 %, Table 5). No number for each session. |
| Liu et al. (UIUC, Microsoft), [arXiv 2608.00101](https://arxiv.org/abs/2608.00101) | 13.5 million GitHub Copilot sessions | Tool calls for each session: median 13, mean 43.6, p90 111.4 (Table 4). Tool execution: median 166 ms. The paper does not name MCP. |
| qaby.ai, [1.42M MCP tool calls compared](https://qaby.ai/blog/mcp-client-comparison-claude-code-vs-cursor) | One Playwright MCP server | Median of 8 calls for each `distinct_id`, which the post calls "a session identity" |

#### SWE-chat analysis

- The data is the version 1 snapshot of SWE-chat (5,851 sessions), from
  the mirror `cfahlgren1/SWE-chat` on Hugging Face (ODC-BY). The card of
  the mirror says that the original files are unchanged. The original
  `SALT-NLP/SWE-chat` needs a login. The maintainer approved the download.
- A small Rust reader (crates `parquet` and `arrow` 60.1.0) took the tool
  calls and the user prompts from `conversations.parquet`. No code from
  the dataset ran.
- The analysis counts Claude Code sessions only. Other agents name MCP
  tools in other forms, and they have about 28 MCP calls in all.
- It does not count the servers that the client app supplies: `ide`,
  Chrome, `conductor`, `acp`, `happy` and `ccd_session`. It removes 17
  sessions that are copies of other sessions.
- A second person (an agent) computed the numbers again with its own
  code. The two results agree. The confidence intervals (95 %) come from
  a bootstrap over users, because the sessions of one user are not
  independent.
- For all agents and before the filters, the data has 5,584 `mcp__`
  calls (1.57 %), not the 6,001 of the paper.
  The cause of the difference is not known.
- The sample is open-source developers who log through the Entire.io
  CLI. It is not a sample of all users of agents.

| Number | Value |
|---|---|
| Claude Code sessions with a tool call | 4,773, from 173 users |
| Sessions with an MCP call | 405 (8.5 %, CI 5.3–13.4 %) |
| MCP calls for each session, all sessions | median 0, mean 0.87 (CI 0.46–1.12), p99 22 |
| MCP calls for each session that has MCP calls | median 3 (CI 3–4), mean 10.3, p90 26 (CI 13–37), p99 94 |
| MCP calls for each turn that has MCP calls | median 2, p90 9 |
| MCP share of all tool calls | 1.29 % (CI 0.75–1.71 %) |
| Call cycle | median 7.2 s. The call itself takes 0.66 s, and the model step after it 5.5 s. |
| Turn wait | median 160 s |
| Time between two calls to the same server | median 8.6 s. 72 % of the calls come within 60 s of the last call to the server. |

- 58 of 173 users have an MCP session, and 5 users make 73 % of the MCP
  calls. Without these 5 users, the p90 for each session goes from 26
  to 14. The median stays at 3.

#### Browser servers

45 % of the MCP calls go to browser servers (Playwright, Chrome
DevTools). These servers keep application state in the server process:
the open browser, its pages, and the element references of the last
snapshot. `2026-07-28` removes protocol sessions, but it keeps this kind
of state: "Servers that need cross-call state use explicit,
server-minted handles passed as ordinary tool arguments"
([changelog](https://modelcontextprotocol.io/specification/2026-07-28/changelog),
item 1).

| Server | Calls | Sessions | Users | Calls within 300 s of an earlier call to the same server |
|---|---|---|---|---|
| Playwright | 947 | 27 | 6 (one user makes 73 % of the calls) | 93 % |
| Chrome DevTools | 274 | 12 | 4 | 92 % |
| Other browser servers | 315 | 6 | 2 | 97 % |
| Claude in Chrome (supplied by the app) | 895 | 27 | 14 | 94 % |

- User-configured browser servers are in 45 of 4,852 Claude Code
  sessions (copies not removed), from
  about 12 of 173 users.
- Almost each call depends on an earlier call: a click needs the page of
  the navigate and the element reference of the snapshot. The median
  chain is 7 calls for Playwright and 10 for Chrome DevTools.

Test with `@playwright/mcp` 0.0.83 (published 2026-09-28), headless,
`--isolated`, with the installed Chrome and a local page. The maintainer
approved this test. Each step was a separate run of `ncp`:
`browser_navigate`, `browser_snapshot`, `browser_click`.

- The result was the same in 3 modes: stdio, HTTP with `--port`, and
  HTTP with `--port` and `--shared-browser-context`. The navigate opened
  the page. The snapshot showed `about:blank`. The click failed with "Ref
  e3 not found in the current page snapshot".
- The server speaks `2025-11-25`. In HTTP mode, it binds the pages and
  tabs to the MCP session (`Mcp-Session-Id`). `ncp` starts a new session
  in each run and ends it with `DELETE`.
- In a test with raw HTTP requests, a second session also saw
  `about:blank`, while the first session was still open. `browser_tabs`
  in the second session listed only its own blank tab. Thus no mode of
  this version lets a later session use the page of an earlier session.
- Only a client that keeps one MCP session open across calls can chain
  these calls. An agent app does this. For `ncp`, this needs a daemon
  that keeps the session.
- The vendors give agents with a shell another tool:
  - The [Playwright MCP README](https://github.com/microsoft/playwright-mcp)
    says that a coding agent "might benefit from using the CLI+SKILLS".
    It keeps MCP for "specialized agentic loops that benefit from
    persistent state".
  - [`playwright-cli`](https://github.com/microsoft/playwright-cli) keeps
    a browser session between separate runs, selected with `--session`
    or `PLAYWRIGHT_CLI_SESSION`.
  - The package of
    [Chrome DevTools MCP](https://github.com/ChromeDevTools/chrome-devtools-mcp)
    includes a `chrome-devtools` CLI "to control the browser from your
    terminal without MCP".

### Prior art: reuse of a server or a connection across runs

- The MCP Inspector has an experimental daemon client, `mcpdo`
  ([docs](https://modelcontextprotocol.io/docs/2026-07-28/tools/inspector/mcpdo.md)).
  Its reason is state across calls, not speed. Its docs list these
  costs:
  - It keeps secrets and tokens in memory.
  - It keeps the environment of its start, and it runs old code after an
    upgrade.
  - It has no maximum lifetime while a connection is open.
- The Inspector keeps its one-shot CLI, with no state between runs, for
  CI and scripts.
- The local-server guide of MCP says that a local port "is not an
  authentication boundary"
  ([local server security](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/local-server-security.md)).
- OpenSSH ControlMaster is the nearest Unix match. It is off by default,
  and `ssh` connects directly when no master listens
  ([ssh_config](https://man.openbsd.org/ssh_config)).
- curl reuses a connection only inside one run
  ([manpage](https://curl.se/docs/manpage.html)). uv uses a file cache
  ([cache](https://docs.astral.sh/uv/concepts/cache/)), and mise reads
  its configuration on each run. Neither keeps a daemon for its own speed.

### Prior art: HTTP and TLS

- `2026-07-28` removes the GET stream, the protocol sessions with their
  `DELETE`, and the `initialize` handshake
  ([Streamable HTTP](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http),
  [changelog](https://modelcontextprotocol.io/specification/2026-07-28/changelog)).
- `2026-07-28` makes `server/discover` optional between two peers that
  speak it
  ([versioning](https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning)).
  rmcp 3.5.1 has no lifecycle mode that skips it.
- A resumed TLS 1.3 handshake still costs 1 RTT (RFC 8446 §2.2). An HTTP
  client "MUST NOT send unsafe methods" in early data (RFC 8470 §4), and
  each MCP message is a POST.
- rustls 0.23.46 resumes a session only with the same verifier object.
  Thus a new process cannot resume a session from an earlier process.
- The curl option `--ssl-sessions` (8.12.0) stores sessions in a file. Its
  docs call it experimental ([manpage](https://curl.se/docs/manpage.html)).

## Not measured

- Servers that are not in the pinned MCP packages, for example a Python
  server through `uvx`, or an `npx -y` that downloads.
- A remote server at a long distance. "Round trips at a distance" gives
  the estimate.
- How many remote MCP servers offer only HTTP/1.1.
- Which common servers do not exit when stdin closes, for example
  `chrome-devtools` and `context7`.
- MCP calls in the turns of subagents. SWE-chat version 1 has none, and
  53 % of its sessions start a subagent. Thus the counts can be too low,
  by an amount that nobody knows.
- Node 24.22.2, the version that `mise.toml` pins.
- Linux.
