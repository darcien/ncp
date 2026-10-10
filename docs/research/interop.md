# Interop tests: first run

Date: 2026-10-10. Design: [interop.md](../design/interop.md). Plan:
milestone 15.

## Setup

| Item | Value |
|---|---|
| `ncp` | commit `69ff0c9`, debug build |
| Server | `@modelcontextprotocol/server-everything` `2026.8.31` (TypeScript SDK v1), from `harness/interop/pnpm-lock.yaml` |
| Node | `22.23.3` (mise) |
| pnpm | `12.10.1` (mise) |
| Host | macOS 15.7.4, arm64 |
| Command | `mise run interop:install`, then `mise run interop` |

The install passed the supply-chain policy of pnpm ("Lockfile passes
supply-chain policies"). No dependency has a build script, so the install
did not stop on the default `allowBuilds` rule.

## Result

All 13 tests passed: 7 over stdio, 6 over Streamable HTTP. The run took
2.7 s. Five more runs after the fix of failure 1 also passed.

| Check | stdio | Streamable HTTP |
|---|---|---|
| Version fallback, first contact | Pass | Pass |
| Version fallback, cached era | Pass | Pass |
| Lists of tools, resources and prompts | Pass | Pass |
| Client capabilities (no elicitation, sampling or roots tools) | Pass | Pass |
| `echo` and `get-sum` | Pass | Pass |
| Structured content, image, resource links | Pass | Pass |
| Text and blob resources, `args-prompt` | Pass | Pass |
| Progress of `trigger-long-running-operation` | Pass | Not in the design |

The integration test of the environment of a stdio server is in the git
hook. It passed on macOS and in the Linux harness.

## Protocol version

The server speaks `2025-11-25`. It answers `server/discover` with
`-32601` "Method not found". `ncp` then starts a new transport and sends
`initialize`, and saves the legacy era in its cache.

Over stdio, the first contact thus starts the server two times. The
server writes "Starting default (STDIO) server..." to stderr for each
start, and `ncp` shows each line as a `serverLog` line.

## Times

`ncp server show --timings`, three runs each. Times in ms.

| Transport | Contact | `totalMs` | `responseMs` |
|---|---|---|---|
| stdio | first | 516 to 524 | two, 75 to 82 each |
| stdio | second | 89 to 97 | one, 84 to 91 |
| Streamable HTTP | first | 11 to 27 | two, 1.4 to 14 each |
| Streamable HTTP | second | 7 to 9 | one, 2.5 to 2.9 |

- The first stdio contact is less than one tenth of the probe timeout
  (10 s). Thus `ncp` does not wait for the timeout: the server answers the
  probe at once.
- The time of a stdio response is mostly the start of Node. The first
  contact has two starts and the stop of the first process.
- The second contact uses the cached era. It sends no probe and starts the
  server one time.
- The slow first HTTP contact of run 1 (27 ms) was the first request to a
  new server process.

## Failures

The last run had no failures. Failures during the build, each with its
cause:

1. **Shared test home.** The first run with both transports had two to
   four failures in each run. The stdio and the HTTP form of a test used
   the same home directory, so one test removed the configuration of the
   other. Cause: a defect in the test. Fix: the home name has the
   transport.
2. **`__CF_USER_TEXT_ENCODING`.** The environment test on macOS found
   this name in the environment of the stdio server. macOS CoreFoundation
   adds it to each process that loads CoreFoundation. It holds the text
   encoding of the user, not a value of `ncp`. The test ignores this one
   name.
3. **Tool count.** A client test counts the tools of the fixture. The new
   tool `env-names` changed the count. Fix: the count in the test.

No failure came from `ncp` or from the server.

## macOS dialog

None. The server has no host option, and it listens on all interfaces.
The macOS firewall of this Mac is off (`socketfilterfw --getglobalstate`
gives "disabled"). On a Mac with the firewall on, macOS can ask if `node`
can accept incoming connections. This run did not show the dialog, so its
effect on the tests is not known.

## Not tested

As the design says: `gzip-file-as-resource` (it fetches a URL),
cancellation (the long-running tool ignores it), and `2026-07-28` (no
official server of another SDK speaks it yet).
