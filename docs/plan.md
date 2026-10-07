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

Done. The user accepted `docs/design/cli.md` on 2026-10-08, with the
decisions C1 to C7 in its section 7.

## Rules for the milestones after the gate

The milestones below are written so that an agent can do them with no
further design discussion. `docs/design/cli.md` is the source of truth for
names, output and behavior. When the design and this plan disagree on
behavior, the design wins. The order of work is this plan; it replaces
section 8 of the design.

When the design does not answer a small question, choose the option that
best fits section 1 of the design, in its order: the spec, secure by
default, good ergonomics, common practice, KISS. Record the choice and its reason in the design, section
9 "Decisions during build", in the same commit, and list it in
`.scratch/progress.md` for the user to review. Continue the work. Stop only
for the points below.

For each milestone:

1. Read `.scratch/progress.md` and the backlog in `.scratch/backlog/`.
2. Change `docs/target-requirements.md` first when behavior changes, in the
   same commit as the code or an earlier one.
3. Write tests named by behavior. Tests use only local servers and never the
   macOS Keychain or the Linux Secret Service of the host.
4. Before each commit: `mise run check`, `mise run test`,
   `mise run linux:check` and `mise run linux:test` pass.
5. When a milestone adds or changes a command, a flag, an output field or a
   message, spawn an adversarial reviewer. It checks the change against
   section 1 of the design: the spec, secure by default, good ergonomics,
   common practice and uv conventions, KISS. Fix each finding that conflicts with the design or the spec, then
   review again. Stop after 3 rounds. Other findings go to
   `.scratch/backlog/features.md`.
6. Update `.scratch/progress.md`: done, next, blocked.

Stop and ask the user (from AGENTS.md, and the points below):

- A new dependency that `deny.toml` does not accept, a new network
  destination, or a change to how secrets are stored.
- A test that needs a real Keychain, a real Secret Service, a real Mac
  dialog, or a third-party server. Write the steps for the user instead.
- A change to a decision of the design (C1 to C7) or to a MUST requirement.
- Code from outside the allowed sources, for example Node or npm packages
  that do not come from the MCP organization.

## Milestone 6: client core changes from the design

Library changes with no command line.

1. Era cache (C3): remove the cause of a legacy era. A cached legacy era
   skips the probe and sends `initialize`. When the handshake with the
   cached era fails, probe again. Update the tests: a server that ignores
   `server/discover` costs the probe timeout only once (count the discover
   calls of the fixture).
2. Time limits (C7, P15): 30 s for the whole connect: server start, the
   10 s probe, and `initialize` after it. A connect timeout stops a stdio
   server with the stop steps and sends no cancel request, because the spec
   says that a client MUST NOT cancel `initialize`. A request limit (default
   300 s) for each later request, given by the caller. A request timeout
   sends a cancel request, as Ctrl-C does.
3. Fix the flaky test (`.scratch/backlog/flaky-probe-timeout-test.md`): a
   fixture mode that is slow on `server/discover` and rejects `initialize`.
   Done when 60 runs in `mise run linux:test` pass.
4. `${env:NAME}` as a second form of `${NAME}` in the configuration.

## Milestone 7: command line

Design sections 1 to 4. Parser: `clap` with `derive` (uv uses clap). Schema check: `jsonschema`
with `default-features = false` (no network fetch of schemas).

1. Commands: `server list`, `server show`, `tool list`, `tool show`,
   `tool call` (with `--help`). Shell completion waits in the backlog: an
   agent does not need it.
2. `${NAME}` comes from the environment only in this milestone. The native
   store and the file come in milestone 9. A hint names only commands that
   exist; hints for `secret set` and `auth login` come with those commands.
3. Names (section 2.1): two words, the joined-form error with the correct
   command, the near-name suggestion, `--` for a tool name that starts with
   `-`.
4. Arguments (section 2.2): all forms, schema coercion, the check before the
   call, `--no-validate`.
5. Options (section 2.3): `--output-format`, `--color` and `NO_COLOR`,
   `-q`, `--config-file`, `--timeout`.
6. Output (section 4): text and JSON, JSON Lines on stderr, the error object,
   exit codes (section 3), the hint on `tool list` when the server has
   `instructions`, and the order of work in `ncp --help`. Text output of
   `tool call`: text blocks as they are; one summary line for other content
   (for example `[image image/png, 2048 bytes]`); `structuredContent` as JSON
   only when there is no text block. A `CreateTaskResult` gives exit 3 with
   the message that tasks are not supported.
7. A missing configuration file is not an error, and `ncp` never creates
   it. `server list` says in one line that no server is configured, with
   the path of the file, and exits 0. A command that names a server that
   does not exist gives exit 2, because the user asked for something that
   `ncp` cannot do.
8. The era cache file at the XDG path (section 6), mode 0600. A file that
   does not load is ignored and written again.
9. Tests run the `ncp` binary against the fixture servers (stdio and HTTP)
   and check stdout, stderr and the exit code. Each test runs the binary
   with a clean environment and its own temporary `HOME` and `XDG_*`
   folders.
10. Sandbox (requirements section 4): `mise run linux:test` runs the tests
    in a container with no network (`--network none`), after an online step
    that fetches the crates. Thus a test that calls out fails and is
    visible.

## Milestone 8: Ctrl-C

1. Ctrl-C during a request sends the cancel request, stops a stdio server
   with the stop steps, and exits with 130 (P10).
2. Ctrl-C during the server start or the handshake stops the server and
   exits with 130.
3. The runtime shuts down with a time limit of 2 s, because a native store
   call that passed its limit can stay on its thread. Test: a blocked store
   call does not delay the exit by more than 3 s.

## Milestone 9: secrets

Design section 2.4, decisions C1 and C2.

1. macOS: the Keychain through `/usr/bin/security`
   (`.scratch/backlog/macos-keychain.md`), before any command uses the
   native store. Tests use a fake `security` program. The check on a real
   Mac is a step for the user: write the steps and report them.
2. New layout of the stored items, approved by decision C1. Service `ncp`.
   Accounts: `secret/<NAME>` for a named value, `oauth/<server>/tokens` and
   `oauth/<server>/client` for OAuth. The file store uses the same accounts.
   Remove the per-server `env` and `header` keys.
3. Resolve `${NAME}`: the environment, then the native store, then the opt-in
   file. Read the native store only for names that the configuration uses
   and the environment does not set. Call the native store only through
   `store::run_blocking`.
4. A value from a store expands only in `env` and `headers` values; in other
   fields it is a usage error with a hint.
5. `secret set`, `secret list`, `secret delete`, and
   `--credential-store <native|file>`. The warning when the other place has
   a copy. The errors and hints when the native store is not available.
   `secret list` gets the names from the native store with a search that
   reads no value: `keyring-core` search on Linux, and
   `security dump-keychain` with no `-d` on macOS, filtered by the service
   `ncp`. The real-Mac steps of item 1 also check that this shows no prompt.
6. Static headers for HTTP servers (A11).
7. Linux harness step 2 (a private D-Bus and keyring daemon in the container)
   needs a change to the credential rule in AGENTS.md. Ask the user first.
   Until then, the Secret Service store has no automated test.

## Milestone 10: diagnostics and proxy

Design sections 2.3 and 4, decision C5.

1. `-v`: the source of each variable (never the value), the proxy route and
   its source, the cached era. `-vv`: JSON-RPC messages and HTTP requests,
   with secrets hidden (I6, S3). Hidden: each `env` and header value that
   came from `${...}`, the `Authorization`, `Proxy-Authorization` and
   `Cookie` headers, the user information in proxy and server URLs, URL
   query parameters with a secret name (for example `token`, `api_key`),
   and all OAuth token and client secret fields. As uv does, a hidden value
   shows as `****`, always with the same length. An empty value shows as
   empty, and a value that is not set does not show. Thus the user can see
   if a value is set, empty, or missing, but never its length.
2. `--timings` (F2), in text and JSON.
3. Turn on reqwest `system-proxy`. Get the route from the same matcher as
   reqwest (`hyper_util::client::proxy::Matcher::from_system`). When a
   request to a loopback host through a proxy on another host fails, show
   the route and the `NO_PROXY` hint. Test with a local fake proxy. The
   macOS system proxy cannot be tested by automation: write steps for the
   user.
4. `SSL_CERT_FILE` (N4): when it is set, trust the roots in that file in
   addition to the system store. Test with a local TLS server and a test CA.

## Milestone 11: OAuth

Design section 5, decision C4, requirements A1 to A12.

1. Discovery (A2), PKCE (A1), the `resource` parameter (A3), the `iss`
   check (A4), the random loopback port, and `--no-browser` with a pasted
   redirect URL (A10).
2. Client registration in the spec order (A6): `oauth.clientId`, then CIMD,
   then Dynamic Client Registration. The place of the CIMD document is a
   release decision: ask the user before CIMD is turned on. Until then, use
   the pre-registered client and DCR.
3. Tokens and client credentials as separate items for each server. The
   client item records its issuer; a new issuer causes a new registration
   (A5). Refresh with no user action (A8), step-up after `403` (A9).
4. `auth login`, `auth logout`, `auth status` (A12).
5. The workarounds in `docs/research/rmcp-open-points.md`.
6. Tests: a local authorization server fixture. No third-party server.

## Milestone 12: conformance

Design and method in `docs/research/test-harness.md`.

1. A small adapter binary that maps each conformance scenario to `ncp`
   commands.
2. **Stop and ask** before you install Node or npm packages that do not
   come from the MCP organization.
3. Run the official suite (`@modelcontextprotocol/conformance`, pinned) for
   both protocol versions. Done when each failure is fixed or listed in the
   expected failures with a reason.

## Milestone 13: release build (gate)

1. Static musl builds for `x86_64` and `aarch64` in the Linux harness, macOS
   builds for both architectures, and checksums (D3).
2. Linux harness steps: smoke tests of the release binary on minimal images,
   and `linux/amd64`.
3. Start time (F3): measure `ncp --version` and `ncp server list`, and
   record the result in a research report.
4. **Stop and ask** before the distribution channel (D2), CI, and any push or
   upload. They add network destinations and accounts.
