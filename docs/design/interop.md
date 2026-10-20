# Design: interop tests

> Archived at v0.1.0. This MVP document describes v0.1.0 and gives no
> direction. When the code and this document do not agree, the code is
> correct.

Status: accepted by the user on 2026-10-10. Versions 1 and 2 each had one
adversarial review and one simplicity review. Plan: milestone 15.

## 1. Goal

Find out if `ncp` works with MCP servers that other SDKs made.

Our own tests use rmcp fixtures, and `ncp` uses the rmcp client. When both
sides share a wrong assumption, our tests cannot see it. The conformance
suite checks the wire against the spec, but only over HTTP and only with
small mock servers. Thus the largest gap is a server on a different SDK.

## 2. Place among the tests

| Layer | Question | Servers | When |
|---|---|---|---|
| Unit and integration tests | Does `ncp` do what the design says, also with faults? | rmcp fixtures | Git hook |
| Conformance | Does `ncp` follow the spec? | Mock servers of the official suite | By hand |
| Interop | Does `ncp` work with the official servers of other SDKs? | MCP organization reference servers | By hand, optional |
| Manual | Do the Keychain, the browser, the system proxy and real deployments work? | Real | Before a release |

A test of `ncp` behavior goes in the integration tests, with an rmcp
fixture. An example is the display of each content type: the fixture can
send each type, and the result does not change between runs. Interop checks
only what a different SDK can change: the start of the session, the
version, the framing, the requests from the server, and the shape of real
results.

## 3. Server

`@modelcontextprotocol/server-everything` (TypeScript SDK v1, npm, MCP
organization), over stdio and Streamable HTTP. Its README calls it "a test
server for builders of MCP clients".

`requirements.md` section 4 accepts official MCP test parts as local
test servers. `AGENTS.md` accepts a pinned package of the MCP organization,
with the dependencies that its lock file pins. The tests need no network.

## 4. Setup

pnpm installs the server. Follow the pnpm guide "Mitigating supply chain
attacks" (pnpm.io/supply-chain-security) and the mise Node.js cookbook.

All files are in `harness/interop/`:

| File | Content |
|---|---|
| `package.json` | `@modelcontextprotocol/server-everything` at `2026.8.31`, the version that `docs/research/test-harness.md` checked. |
| `pnpm-workspace.yaml` | The settings below. |
| `pnpm-lock.yaml` | Committed. |
| `.gitignore` | `/node_modules`, as in `harness/conformance/`. |

- **pnpm version.** `mise.toml` pins pnpm at an exact version, as it pins
  all other tools. This is the mise setup with no Corepack. `package.json`
  has no pnpm version.
- **Settings.** Set only what differs from the pnpm defaults:

  ```yaml
  minimumReleaseAgeStrict: true
  trustPolicy: no-downgrade
  ```

  Do not add `allowBuilds` entries. With the defaults, an install then
  fails when a dependency has a build script.
- **Lock and install.** `pnpm install --lockfile-only` makes or updates
  the lock. `mise run interop:install` runs `pnpm install
  --frozen-lockfile`. This is the only online step.
- On a pin change, review the diff of the direct package and the diff of the
  lock file.

## 5. Tests

### Form

- The file is `crates/testkit/tests/interop.rs`. Each test has `#[ignore]`.
  The git hook compiles the tests, but does not run them, because the hook
  has no Node.
- `mise run interop` runs `cargo test -p ncp-testkit --test interop --
  --ignored`.
- When the install is missing, a test fails with a hint to run
  `mise run interop:install`. It does not skip.
- The tests use the helpers of `tests/cli.rs`: a clean environment, an
  empty home, the file credential store, `--output-format json`. Move these
  helpers to a module that both test files use. Each test asserts on the
  parsed JSON output.

### Server command

The tests start `ncp` with a clean environment that has no `PATH`, and
`ncp` gives a stdio server its own environment. Thus a shell shim in
`node_modules/.bin` cannot find `node`, and the test gives `ncp` absolute
paths:

- `command`: the `node` that the test process finds on its own `PATH`. mise
  puts the pinned Node there.
- `args`: the entry point of the server, from the `bin` field of
  `node_modules/@modelcontextprotocol/server-everything/package.json`, and
  the transport (`stdio` or `streamableHttp`).

### Over stdio

1. **Version fallback, first contact.** With an empty home, `server show`
   gives `protocolVersion` `2025-11-25`. With `--timings`, `totalMs` is less
   than half of `ncp::client::PROBE_TIMEOUT`. Thus a server that ignores the
   Discover request, so that `ncp` waits for the timeout, fails the test.
   Only this test has a time limit.
2. **Version fallback, cached protocol version.** A second call with the same home gives
   the same version. This is the normal path for a user.
3. **Lists.** `tool list`, `resource list` and `prompt list` exit 0.
4. **Capabilities.** The server registers some tools only for a client
   capability. With no TTY, `ncp` declares no elicitation, so the tool list
   does not show `trigger-elicitation-request`. Thus this test checks the
   capabilities that `ncp` sends.
5. **Calls.** `echo` and `get-sum` give the expected values.
6. **Result shapes.** `get-structured-content`, `get-tiny-image` and
   `get-resource-links` exit 0, and the expected fields are in the output.
7. **Resources and prompts.** `resource read` of a text resource and of a
   blob resource. Take the URIs from `resource list` and its templates.
   `prompt get args-prompt` with arguments.
8. **Progress.** `trigger-long-running-operation` with a short duration
   and few steps exits 0, and stderr has progress lines.

Do not use `gzip-file-as-resource`, because it fetches a URL. Do not test
cancellation, because `trigger-long-running-operation` ignores it.

### Over Streamable HTTP

Tests 1 to 7 again:

- The test gets a free port from the operating system, and starts the
  server with `streamableHttp` and that `PORT`. Then it waits until the port
  accepts a connection.
- `ncp` connects to `http://127.0.0.1:<port>/mcp`. `ncp` accepts plain
  HTTP only to a loopback host.
- The test stops the server at the end, also when the test fails.
- If the server listens on all interfaces, the macOS firewall can show a
  dialog. Then use a host option of the server if it has one, or record the
  dialog in the report of the first run.

### Environment of a stdio server: an integration test

`ncp` clears the environment before it starts a stdio server. Then it gives
the server its own environment and the `env` of the entry. The SDK of the
server cannot change this, so the test goes in the integration tests, not
in interop:

- Add a tool to the rmcp fixture that gives the names of its environment.
- Put a value in the credential store that no entry references.
- Assert that the server gets exactly the expected names, and that no value
  of the store is in its environment.

## 6. When interop runs

Interop is optional. The user or an agent runs `mise run interop` by hand.
It is not in the git hook, and it is not a condition to end a milestone.
The project is too early to fix a schedule.

## 7. Later work

Each item needs a stop and ask before it starts.

- **A schedule for conformance and interop**, for example at the end of
  each milestone or before a release.
- **The conformance suite on pnpm**, with the settings of section 4, so
  that both harnesses use one package manager.
- **`mcp-server-time`** (Python SDK, PyPI, MCP organization). It is the
  best next server, because many small servers use the Python SDK. It needs
  uv, PyPI and a lock form for Python. Use `required-environments` for
  `aarch64` and `x86_64`, so that the lock has wheels for both.
- **A Docker runner**, when the first server that is not from the MCP
  organization comes. Notes from the reviews:
  - hardened flags: read-only root, no capabilities, non-root user, limits
  - a pinned multi-arch index digest for each image
  - copy only the ncp binary, not the whole target volume
  - unique container names with `--rm`; exit codes 125, 126, 127 and 137
    are infra failures
  - `UV_COMPILE_BYTECODE=1` for Python, because a read-only root gives no
    `.pyc` files
  - `ca-certificates` in the image for a remote server
- **A live tier** with remote servers that need no authentication, from
  established vendors (for example AWS Knowledge). A failure there is a
  lead, not a defect. Until then, add such a server to the user
  configuration and run `ncp` by hand.
- **Third-party stdio servers**, for example `duckduckgo-mcp-server`.
  `AGENTS.md` takes a small project of one person only as an example. With
  full network, such a server can reach the local network, and a tool that
  fetches any URL makes the list of destinations open. An egress proxy that
  allows only the hosts of each server must come first.
- **Remote servers that need OAuth** (for example GitHub). A login needs a
  person at a browser, so they belong in the manual steps.
- **A server that speaks `2026-07-28`**, when an official SDK other than
  rmcp supports that version. Until then, the conformance suite and the
  rmcp fixtures test that version.
