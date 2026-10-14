# Implementation plan

Temporary working document for the MVP, like `requirements.md`.
Do the milestones in order. Each milestone ends with `mise run check` and
`mise run test` green, and one or more commits.

Record progress in `.scratch/progress.md` (not tracked by git): what is done,
what is next, what is blocked, and why.

The distribution milestone has no number and is always the last. Put a new
milestone before it, with the next number.

## Milestone 1: early tests of risks

Status: done.

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

Status: done.

Pure functions and types. No I/O, no command line.

1. Configuration model: parse the `mcpServers` JSON core. A `url` entry needs
   `type`. Unknown keys give warnings. `${VAR}` expands from an environment
   map that the caller gives.
2. Value types with rules: server name, server URL.
3. Protocol era (unknown, modern, legacy with version) and the cache of the
   era for each server.
4. Credential model: the secret kinds, and the key for each server and name.

## Milestone 3: credential store

Status: done.

1. One internal interface for the credential store.
2. An in-memory store for tests.
3. The opt-in `0600` file store.
4. The native stores (macOS Keychain, Linux Secret Service). Compile them,
   but do not use them in automated tests.

## Milestone 4: client core

Status: done.

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
2. Change `docs/requirements.md` first when behavior changes, in the
   same commit as the code or an earlier one.
3. Write tests named by behavior. Tests use only local servers and never the
   macOS Keychain or the Linux Secret Service of the host.
4. Before each commit: the git pre-commit hook passes.
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
- Code from outside the allowed sources of `AGENTS.md`, for example an npm
  package that is not from the MCP organization and is not in the lock file
  of such a package.

## Milestone 6: client core changes from the design

Status: done.

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

Status: done.

The minimum for a person or an agent to list and call tools. Design
sections 1 to 4 give the details. Items marked "later" in the design wait
in `.scratch/backlog/features.md`. Parser: `clap` with `derive`. Schema
check: `jsonschema` with `default-features = false`.

1. Commands: `server list`, `server show`, `tool list`, `tool show`,
   `tool call` (with `--help`).
2. Arguments: `key=value` with schema coercion, `{...}`, `-` (stdin). The
   schema check before the call, and `--no-validate`.
3. Option: `--timeout`.
4. Output: text on a TTY, JSON in a pipe, JSON Lines on stderr, the exit
   codes. No color.
5. Errors give the fix: a joined server and tool name, a missing tool name,
   an unknown server or tool (with the path of the configuration file or
   the hint `ncp tool list SERVER`).
6. `${NAME}` comes from the environment only. Hints name only commands that
   exist.
7. Configuration file at the default path only. A missing file is not an
   error: `server list` says that no server is configured, with the path,
   and exits 0. `ncp` never creates the file. A file that does not parse
   gives exit 2.
8. The era cache file (section 6), mode 0600.
9. Tests run the binary against the fixtures, with a clean environment and
   temporary `HOME` and `XDG_*` folders. `mise run linux:test` runs them
   with `--network none`, after an online step that fetches the crates.

## Milestone 8: Ctrl-C

Status: done.

1. Ctrl-C during a request sends the cancel request, stops a stdio server
   with the stop steps, and exits with 130 (P10).
2. Ctrl-C during the server start or the handshake stops the server and
   exits with 130.
3. The runtime shuts down with a time limit of 2 s, because a native store
   call that passed its limit can stay on its thread. Test: a blocked store
   call does not delay the exit by more than 3 s.

## Milestone 9: secrets

Status: done.

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
7. Linux harness step 2: a container stage with `dbus` and `gnome-keyring`
   from deb.debian.org. Each test run starts its own D-Bus session and
   keyring daemon (AGENTS.md permits this only in the container). Test the
   cases: no bus, no Secret Service, unlocked, locked (the time limit).

## Milestone 10: diagnostics and proxy

Status: done.

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

Status: done.

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

Status: done.

Design and method in `docs/research/test-harness.md`.

1. A small adapter binary that maps each conformance scenario to `ncp`
   commands.
2. The user approved Node and the suite on 2026-10-08, with pinned versions
   only. Before you install:
   - Pin Node to an exact version in `mise.toml`.
   - Find the exact version of `@modelcontextprotocol/conformance` on the
     npm registry. Check that the MCP organization publishes it. Do not use
     `latest`, a version range, or `npx` with no version.
   - Install with `npm ci` from a committed `package-lock.json`, so that each
     dependency has an exact version and an integrity hash.
   - Record the versions and the hash of the suite in the test report.
   Stop and ask if a check fails.
3. Run the official suite (`@modelcontextprotocol/conformance`, pinned) for
   both protocol versions. Done when each failure is fixed or listed in the
   expected failures with a reason.

## Milestone 13: decisions of the reviews of 2026-10-09 and 2026-10-10

Status: done. The user approved the build on 2026-10-10. Three review
rounds ran; the open points are in the backlog.

The user accepted these decisions. The requirements and the design have the
behavior. Do the items in order. Each item is one or more small commits.

1. macOS store: long values in parts (S7, B66).
2. Client selection, `oauth.clientSecret` and `oauth.issuer` (A5, A6, B67,
   B68). Remove the pre-registration scenario from the expected failures of
   the conformance suite.
3. Token binding (A13, B70).
4. No redirects on the MCP connection (N6, B69).
5. Verbose output hides only the secrets of S3 (I6, B43). Remove the guess
   from the name of a URL query parameter.
6. `SSL_CERT_FILE` replaces the system roots on macOS too (N4, B50). Add the
   hint for an unknown issuer, and a Linux test that proves the rule.
7. Remove `key:=json`, `key=@path` (also for prompt arguments) and
   `--color` (B57, B58, B60). Keep the indent of later message lines.
8. List cache: never keep a private list of a stdio server (B62). Add tests:
   a private list after OAuth or `${NAME}` is not written; a login, a
   logout and a change of a secret remove the lists; the merge of pages;
   a public list with a credential is kept.
9. Elicitation (B63):
   - With no TTY, a legacy request gets `-32602`, then exit 5. Correct the
     comment in `harness/conformance/expected-failures-agent.yaml`.
   - The user can change a field before the answer goes out.
   - A warning for a host with a Punycode label.
   - The error after a wrong answer escapes server text also with JSON
     output on a TTY.
   - Tests for these, and for form cancel and Ctrl-C during a question.
10. Fixes with no new decision:
    - The OAuth callback shows `error_description` only after the `iss`
      check (MCP 2026-07-28, authorization).
    - The token search treats an unavailable native store as the other
      store searches do, not as empty.
    - One function for the rule "HTTPS, or HTTP to a loopback host".
    - U+200D (zero width joiner) is not escaped, so that emoji stay whole.
11. Check the claims in `.scratch/backlog/review-claims.md`. Fix each claim
    that is true. Remove the file when done.

## Milestone 14: release build

Status: items 1 to 3 done on 2026-10-10 at commit 3066c47 (report
`docs/research/release-build-2026-10-10.md`). The release step also makes
the archives and `SHA256SUMS` of the decided release form, with no upload.
Item 4 moved to the distribution milestone: the user decided on 2026-10-10 to do the
interop tests before the distribution, and to do the distribution last.

1. Static musl builds for `x86_64` and `aarch64` in the Linux harness, macOS
   builds for both architectures, and checksums (D3).
2. Linux harness steps: smoke tests of the release binary on minimal images,
   and `linux/amd64`.
3. Start time (F3): measure `ncp --version` and `ncp server list`, and
   record the result in a research report.
4. Moved to the distribution milestone.

## Milestone 15: interop tests

Status: done on 2026-10-10. All 13 interop tests passed (report
`docs/research/interop.md`).

Test `ncp` against the official server of a different SDK, with no
network. Design: `docs/design/interop.md` (accepted on 2026-10-10). Interop
is optional: it is not in the git hook, and later milestones do not need
it.

1. `harness/interop/` (design section 4): pnpm pinned at an exact version in
   `mise.toml` only, `server-everything` at `2026.8.31`,
   `pnpm-workspace.yaml`, a committed `pnpm-lock.yaml`, and
   `mise run interop:install`. If `typos` reports words in
   `pnpm-lock.yaml`, exclude the file.
2. Move the shared helpers of `crates/testkit/tests/cli.rs` to a module
   that more test files can use.
3. The interop tests over stdio (design section 5), and
   `mise run interop`.
4. The same tests over Streamable HTTP on `127.0.0.1`.
5. The integration test for the environment of a stdio server (design
   section 5, last part). This test is in the git hook.
6. Run `mise run interop` one time. Record the result in
   `docs/research/interop.md`: the protocol version, the times of the
   first and the second contact, each failure and its cause, and any macOS
   dialog.

Done when the git hook passes and `mise run interop` passes one time.

## Milestone 16: fixes from the reviews of 2026-10-10

Status: done on 2026-10-10. Item 5 was done already in milestone 13. One
review round found only small items, and they are fixed.

These fixes need no new decision. Each one is a small commit.

1. The rule "HTTPS, or HTTP to a loopback host" for a server URL is an
   `ncp` rule. The MCP 2025-11-25 authorization text "Communication
   Security" covers only the authorization server endpoints and the
   redirect URIs. Remove the citation from `server.rs` and state the rule
   and its reason in words (AGENTS.md).
2. After the refresh lock, a token read that finds tokens of another
   binding gives the B70 message, not "the server needs a login". Use a
   marked store error that maps to the binding failure.
3. When the login fails, the error path uses the `url_values` of the first
   load of the configuration. It does not read the file again, because the
   file can change during a long wait for the browser.
4. Each wait has a time limit. `secret set` reads a piped value with the
   `--timeout` limit. Its TTY test checks stdin and stderr, as
   `cli/ask.rs` does.
5. Clean-up after B66: remove the size comment in `oauth/items.rs`
   ("about 2900 bytes"). The long-line refusal in `store/security.rs` uses
   the split limit.

## Milestone 17: bind stored secrets to origins

Status: done on 2026-10-10. Three review rounds; the findings are fixed.
Not tested: the question on a TTY (it needs a terminal).

Design: `docs/design/secret-binding.md`. Research:
`docs/research/pnpm-credentials.md`, `docs/research/add-command.md`.

1. Done: the user accepted the design on 2026-10-10.
2. Done: requirements S9 and I8, and the changes of section 5 of the
   design in `cli.md` (B72).
3. Domain types, with no I/O:
   - `Origin`: the scheme, host and port of the URL as the file writes it.
     The text before the first `${` must reach the end of the host and
     port. The url crate gives the normal form.
   - The scope of a value: an origin, or no destination for stdio.
   - The secret key holds the scope and the name. Remove the key that has
     only a name, so that no code can read a stored value for an entry
     without its origin.
   - The account `secret/ORIGIN/NAME` or `secret/NAME`, and the parse of
     an account back to a key.
4. Lookup: a pure plan of the keys that an entry needs, then the store
   reads, then the expansion. The miss error names only the current
   origin. The key of `oauth.clientSecret` has the origin of
   `oauth.issuer`, and the B68 issuer check stays.
5. `secret set NAME [--server SERVER]`: the target rule as a pure function
   of the configuration. The question and the result show the destination
   and the servers that use it.
6. `secret delete NAME [--server SERVER | --unused | --all]`. With no
   flag, it removes nothing when NAME has two or more destinations.
   `secret list` shows
   one row for each name and store key. The copy check of B35 uses the
   same key.
7. A `401` after a stored header value gives the hint `ncp secret set
   NAME --server SERVER` first, then `ncp auth login SERVER` (B51).
8. Tests: the list in section 5 of the design, and this property: for each
   URL with a literal origin, the literal origin is the origin of the
   expanded URL.

Items from before this milestone need no migration, because there is no
release. An old `secret/NAME` item is then a value for stdio servers.

## Milestone 18: project configuration (gate)

Status: design `docs/design/project-config.md` revision 4, accepted by the
user on 2026-10-11. The design is the source of truth; this list only
orders the work.

1. Read JSONC in the global file and the project file (design rule 1).
2. Search and owner check (rule 2), merge (rule 3), trust records (rule 4).
3. The trust gate in the lookup of a server name, the question, and
   `ncp trust` with `--show`, `--untrust` and `--ignore` (rule 5).
4. Expansion (rule 6), what a project file can set (rule 7), and one gate
   before any action (rule 8).
5. Output (section 4) and tests (section 6).
6. The changes to the requirements and to `cli.md` in design section 5
   are done (2026-10-11).

## Milestone 19: `server add` and `server remove` (gate)

Design: `docs/design/server-add.md` (version 7), accepted by the user on
2026-10-12, with option A for a new project file (the current folder).
Its target files, `--global` and the trust check before a write come from
`docs/design/project-config.md` rule 9 (milestone 18).
The user wants one flow: the entry, the secret and the login hint in one
step.

1. Move the proposal to `docs/design/server-add.md`. Version 5 already
   follows milestone 17 (the entry before `secret set`, an item for each
   origin, the row shape of `secret list` in the result, rule 13) and
   project-config rule 9. Still open: `--env NAME` shows the other stdio
   entries that use NAME. Review it again, then **stop and ask** the user
   to accept it.
2. The settled decisions of the proposal (section 8): no question without
   `--secret NAME`; literal values accepted with no warning; the exact
   template for a credential header; no contact with the server at add;
   no login at add, and the result gives `ncp auth login NAME`;
   `--config-file` is an `ncp` file; the configuration file is the source
   of truth.
3. The dependency `jsonc-parser` (feature `cst`) for the edit of the file.
   Every byte that `ncp` does not change stays. Check `deny.toml` first,
   and stop and ask if it does not accept the crate. Pin a version that is
   older than two weeks.
4. The file write: strict JSON for the new entry, no clobber, a self-check
   before the rename, an atomic rename, a lock, a new read before the
   rename, and the modes of the proposal (section 5).
5. `server remove`: remove the entry by its raw key, then the OAuth items
   of the server. A store failure gives a warning and exit 0. The `${NAME}`
   secrets stay.
6. Manual interop with the Cloudflare servers of the proposal (section
   10). The hosts are `docs.mcp.cloudflare.com`, `blog.mcp.cloudflare.com`
   and `mcp.cloudflare.com`. The user approved them on 2026-10-11
   (AGENTS.md). Only this manual test uses them; the automated tests do
   not. The user does the OAuth step of `mcp.cloudflare.com` in a
   browser, with their own account. Record the result in a new report in
   `docs/research/`.

## Milestone 20: errors, warnings and hints

Design: `docs/design/output-messages.md` (revision 5). The design is the
source of truth; this list only orders the work. Research:
`docs/research/uv-output-messages.md`.

1. **Stop and ask** the user to accept the design.
2. Do steps 2 to 10 of the migration plan (design section 8), in order.
   Each step changes the types, the call sites, the tests and the design
   documents of one area.

## Last milestone: distribution (gate)

The workflow needs action pins from github.com. The
push, the tag and the CIMD URL need the user.

Parked by the user on 2026-10-12. Start only after the user does the tests
of `mise run live` (`docs/live-tests.md`), and reviews the build decisions
in section 9 of `docs/design/cli.md`.

1. **Stop and ask** before the distribution channel (D2), CI, and any push or
   upload. They add network destinations and accounts.
2. The decisions of 2026-10-09 (`.scratch/backlog/release.md`):
   - Source: a public GitHub repository.
   - CI: a GitHub Actions release workflow on a tag `vX.Y.Z`. Its first
     step checks that the tag is the version in `Cargo.toml`. It uses
     `harness/release.sh`.
   - Artifacts: `ncp-X.Y.Z-<target>.tar.gz` (one top folder with `ncp` and
     the licenses), one `SHA256SUMS`, and a GitHub artifact attestation.
   - Channel: mise `github:OWNER/ncp` only. No Homebrew tap.
   - Version: SemVer `0.y`.
   - CIMD URL: `https://OWNER.github.io/ncp/oauth/client-metadata.json` on
     GitHub Pages. Set the URL in the code at this release. Never move the
     URL unless the old one still serves `200`, because CIMD permits no
     redirects. Before the URL goes in, ask the user about the CIMD risks
     in the release backlog (exact redirect match against a random port,
     an authorization server that limits redirect URIs to the origin of
     the document, company networks that cannot reach `github.io`).
3. Checks at the first release: `mise install` works on macOS and Linux;
   the binary has no quarantine attribute; the tag push starts the
   workflow.
4. Later, not in the MVP: a release with no manual tag and no hand-written
   changelog (for example release-plz), and a version rule that follows
   the MCP spec version.
