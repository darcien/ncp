# Implementation plan

Temporary working document for the MVP, like `target-requirements.md`.
Do the milestones in order. Each milestone ends with `mise run check` and
`mise run test` green, and one or more commits.

Record progress in `.scratch/progress.md` (not tracked by git): what is done,
what is next, what is blocked, and why.

## Milestone 1: early tests of risks

Prove the parts that come only from reading code. Each test uses only local
servers. Put test code in `.scratch/`. Write the results in a new report,
`docs/research/early-tests.md`. A failed or blocked test is a valid result:
record it and continue.

1. rmcp client against a local rmcp server, on stdio and on Streamable HTTP,
   in modern mode and in legacy mode.
2. Era detection with our own timeout. If the server is legacy, restart with
   `initialize`. A slow modern server that answers late must not cause a
   failure.
3. Cancellation: a stdio server in its own process group, and a cancel
   request after Ctrl-C.
4. reqwest proxy behavior through a local proxy: `HTTPS_PROXY`,
   `https_proxy`, `NO_PROXY`, and credentials in the proxy URL.
5. Timings through the `reqwest::Client` that we give to rmcp: DNS, connect,
   and response time.
6. `keyring-core` 1 with the zbus Secret Service store: does it build for
   `x86_64-unknown-linux-musl`? A build check only. If the toolchain for this
   target is not available, record the step as blocked.

## Milestone 2: domain core

Pure functions and types. No I/O, no command line.

1. Configuration model: parse the `mcpServers` JSON core. A `url` entry needs
   `type`. Unknown keys give warnings. `${VAR}` expands from an environment
   map that the caller gives.
2. Value types with rules: server name, server URL.
3. Protocol era (unknown, modern, legacy with version) and the cache of the
   era for each server.
4. Credential model: the secret kinds, and the key for each server and name.

## Milestone 3: credential store

1. One internal interface for the credential store.
2. An in-memory store for tests.
3. The opt-in `0600` file store.
4. The native stores (macOS Keychain, Linux Secret Service). Compile them,
   but do not use them in automated tests.

## Milestone 4: client core

A library API with no command line.

1. Connect to a server from the configuration: stdio and Streamable HTTP.
2. Era detection with our own timeout and the era cache.
3. List tools and call a tool.
4. Proxy variables for stdio servers. Secrets from the credential store as
   environment variables for stdio servers.
5. Integration tests against local fixture servers built with rmcp.

## Milestone 5: command-line design (gate)

Write a design proposal in `docs/design/cli.md`:

- Commands and arguments, and how to name a server and a tool.
- Exit codes and their numbers.
- JSON output schema.
- OAuth redirect: fixed or random loopback port.
- Location of state and cache files.

**Stop after the proposal.** Do not start the next milestone before the user
accepts the design.

## After the gate

Command line, output and exit codes, OAuth, timings, conformance adapter for
the official suite, release build. The order is set after the gate.
