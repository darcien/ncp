# Contributing to ncp

`ncp` is a command-line MCP client for one user on macOS and Linux. It is
written in Rust on the official SDK `rmcp`.

Read the documents of the task before a change:

| Task | Read |
|---|---|
| Each change | This document and [STYLE.md](STYLE.md), the rules for all text |
| A command, an option, an output or a secret | [docs/command-development.md](docs/command-development.md) |
| A release | [docs/release.md](docs/release.md) |
| The live tests | [docs/live-tests.md](docs/live-tests.md) |

AI agents also read [AGENTS.md](AGENTS.md).

## Set up

1. Install the tools: `mise install`.
2. Install the git hook: `hk install --mise`.
3. To make sure that the setup works, run `mise run check` and
   `mise run test`.

## MCP spec and sources

- **MCP spec**: the official, versioned specification at
  modelcontextprotocol.io. `ncp` targets `2026-07-28`, with fallback to
  `2025-11-25`.
- Cite an external fact with a source that does not change: a versioned
  MCP spec URL, an RFC section (for example "RFC 9207 §2.4"), or the name
  of an official conformance scenario. Open the source before you cite it. Do not
  cite from memory.
- For a rule that is specific to `ncp`, state the rule and its reason in
  words.
- `docs/research/` holds dated reports. Each report is standalone and can
  be stale. No other document depends on a report. Delete a report
  when it has no use. A broken link in an MVP document is permitted.
- Write a test report only for a defect, or for a fact that changes a
  decision. Do not commit a result that only passes.
- Take prior art from the official MCP specification, SDKs and test parts,
  and from tools of established vendors (frontier AI labs and major editors).
- Use a small project of one person only as an example.

## Documents

- The code and its tests are the reference for the current behavior.
- `docs/requirements.md`, `docs/plan.md` and `docs/design/` are the MVP
  documents. They describe v0.1.0. Do not take a task, an order or a
  design from them.
- Exception: each MUST rule of `docs/requirements.md` stays in force until
  a rule in another document replaces it. The output formats in
  `docs/command-development.md`, "Streams and formats", replace the rule
  I1. When the code does not obey a MUST rule, the code has a defect. For
  each other difference between the code and an MVP document, the code is
  correct.
- In code, tests and comments, do not cite an MVP document, an `ncp` RFC
  (`docs/rfcs/`) or a research report, and do not use their IDs (for
  example `P2` or `B67`). These documents can go away or change form. An
  IETF RFC section is not an `ncp` RFC (see "MCP spec and sources").
- A commit body can give the path of the `ncp` RFC that it builds.
- Write documents and comments for the current behavior. Do not write the
  history of a change in them: the commit has it.

## RFCs

An `ncp` RFC records the reasoning and the design of a change before the
build. Write one for a medium or large change that needs deeper
reasoning to reach a whole solution. A small change needs only the
reason in the commit body.

- Path: `docs/rfcs/NNNN-slug.md`. Use the next free number. Do not use a
  number again.
- Front matter: `state` (`proposed`, `accepted`, `committed` or
  `abandoned`) and `supersedes` (a list of numbers).
- Sections, in the style of an Oxide RFD: Background, Problems,
  Determinations, Alternatives, Tests and Compatibility. Alternatives
  gives each design that the RFC does not choose, with the reason from
  the order of choice (`docs/command-development.md`). A draft can also
  have Open questions. The maintainer answers them before acceptance.
- Write a new RFC with `state: proposed`. Only the maintainer accepts or
  abandons an RFC. Set `state: accepted` or `state: abandoned` only after
  the maintainer says so.
- Only an `accepted` RFC gives direction. A `proposed` RFC is a draft.
- An RFC on a separate branch stays editable until its completed
  implementation lands on `main`. A local commit does not freeze it.
  Keep its state `proposed` or `accepted`, as the maintainer decides.
- When the completed implementation lands on `main`, set
  `state: committed`. Update the affected documents in the same change.
  The RFC is then frozen.
- Do not change a frozen or `abandoned` RFC. A new design after that
  needs a new RFC with `supersedes`.
- Do not start to build a change that needs an RFC before its RFC has
  `state: accepted`. If you are not sure that a change needs one, ask
  the maintainer.
- A change to a MUST rule of `docs/requirements.md`, or to the order of the
  work, needs the maintainer to agree first.

## Code style

Write Rust like a clean, professional Elixir codebase: simple, functional and
explicit.

- **KISS.** Use the simplest design that meets the spec. Do not add
  abstractions for future needs.
- **Functional core, imperative shell.** Keep domain logic in pure functions:
  data in, data out. Keep I/O (network, files, processes, credential store)
  at the edges.
- **Data flows through transformations.** Use iterator chains, `map`,
  `and_then` and `?`, as Elixir uses pipelines and `with`. Use loops only
  when they are clearer.
- **Immutable by default.** Use `mut` only in a small scope. Do not share
  mutable state.
- **Model the domain with types.** Use enums for states and alternatives,
  and use exhaustive `match`. Use newtypes for values with rules (for example
  a server name). Make illegal states impossible to represent.
- **Errors are values.** Return `Result` with a typed error. Do not use
  `unwrap` or `expect` outside tests.
- **Small modules and functions** with one purpose each. Names describe the
  domain, not the mechanism.
- **Few dependencies.** Prefer `std` and the dependencies that are already in
  the workspace.
- `unsafe` is forbidden.

## Dependencies

Each new dependency needs a reason, and `deny.toml` must accept it. To add
a dependency that `deny.toml` does not accept, ask the maintainer.

## Tests

- Tests use only local servers: stubs, rmcp fixtures, and official MCP test
  parts. Do not call third-party servers.
- Automated tests do not use the macOS Keychain or the Secret Service of the
  host. The Keychain shows a prompt, and an automated run cannot accept it.
  Use the in-memory store or the credential file in tests.
- The live tests (`mise run live`, `docs/live-tests.md`) are the exception
  to the two rules above. They use the login
  Keychain, a browser, the system settings and a real server. Only the
  maintainer runs them.
- Tests of the Linux Secret Service store run only in the Linux harness
  container. Each test run starts its own D-Bus session and keyring daemon in
  the container.
- Name tests by behavior, for example
  `falls_back_to_initialize_when_discover_times_out`.

### Command snapshots

- A new test of command output snapshots the full transcript: the exit
  code, stdout and stderr. Use `common::ncp_snapshot!` with
  `home.filters()` and `home.ncp(args)`. Text is the default.
  JSON tests pass `--output-format json`.
- For each command, keep a text snapshot of its main success and main
  error. A command with no error path snapshots its warning instead.
- When a change edits a current test of command output, move that test
  to snapshots. For its main success and main error, snapshot both output
  formats. An explicit change can instead move one command group.
  Other current tests keep their part checks.
- Shared helpers and loops can cross command groups. When you move one to
  snapshots, keep each case that it covers.
- These rules apply to tests that `mise run test` runs. Ignored tests
  keep their part checks.
- Runs with open stdin, signals, concurrent stream reads or output streamed
  to a file keep their part checks.
- A test that changes a selected file snapshots the whole file with
  `insta::assert_snapshot!` inside
  `insta::with_settings!({filters => home.filters()}, { ... })`.
  It does not inventory every changed file. Keep file-mode assertions.
- The macro returns the same run for raw assertions. Do not run the
  command again to obtain its output. Keep `run.error()` and
  `run.warnings()`, which check the rules of JSON diagnostics.
- Keep raw assertions for secrets, exact bytes, final newlines and
  line-end spaces. Snapshots cannot protect these properties alone.
- Keep assertions for requests, file modes and other data outside the
  transcript. If a filtered value is the subject of the test, check its
  unfiltered value too.
- Output with variable order or line count keeps its required property
  checks. Examples include server logs, progress and origin-sorted values.
- Each inline assertion has its own call site. Do not run it repeatedly
  through a loop, closure or helper. Use `insta::allow_duplicates!` only
  when all runs must produce the same text.
- Per-test filters can precede or follow `home.filters()`. Later filters
  see earlier replacements. With multiple local servers, name all but
  one before the standard loopback filter.
- Standard filters cover paths, exact build executables, local ports,
  measured times in traces and the cache, and outgoing client-version
  metadata. `ncp_snapshot!` itself replaces each time of a `--timings`
  line, so the number of responses stays visible. It also filters CLI
  version results when the `ncp` binary runs with `--version` or `-V`
  before `--`. Other changing values need per-test filters, also when
  repeated runs produce the same value.
- Table padding can depend on a path. Use a per-test filter that keeps
  the indentation.
- Missing-file and connection-refused normalization is opt-in. Do not
  filter arbitrary OS codes, fixed time limits or server identities.
- A controlled server failure is good when it tests the intended
  behavior. It does not replace the connection-refused case.
- Fixture payloads avoid accidental filter matches unless the test checks
  filter scope.
- Inline snapshots retain intentional whitespace. The trailing-whitespace
  step in `hk.pkl` lists narrow source-file exclusions.
  Keep raw assertions for whitespace and final newlines.
- To update snapshots, run `cargo insta test --workspace --accept`.
  Review every changed transcript. `cargo insta review` is interactive.
  An agent uses `cargo insta accept` for pending snapshots.
  The hook and CI never accept snapshots automatically.
- Host runs write ignored `*.pending-snap` files beside test sources.
  Linux runs set `INSTA_PENDING_DIR=/tmp/insta-pending`, so pending files
  stay inside the container, also with stale host files.
  Container pending files disappear after the run.
- If a container snapshot fails, correct its filter or update it on the
  host. Workspace discovery uses offline `cargo metadata` in the container.

## Checks

- The git pre-commit hook runs the checks. Do not skip the hook.
- The advisory check gives only a warning. It does not stop a commit, CI or
  a release. The RustSec database can change when the repository does not.
- The hook runs only the checks that the staged files can affect. For
  example, a change to the docs does not start the Rust builds. When a new
  kind of file can change the build or the tests, add it to the globs in
  `hk.pkl`.
- Run `mise run precommit` to run all checks on all files, for example after
  a change to `hk.pkl` or `mise.toml`.

## Commits

- Commit in small working chunks.
- Mark a breaking change (`docs/command-development.md`, "Version rule")
  with a `BREAKING CHANGE` footer that gives the steps for the user.
- Before a push, fold a fix of an unpushed commit into that commit, so that
  the history has no fix-up commits.
- Use [Conventional Commits](https://www.conventionalcommits.org/):
  `type(scope): subject`. Types: `feat`, `fix`, `refactor`, `test`, `docs`,
  `build`, `chore`.
- Write the subject and the body as [STYLE.md](STYLE.md) says. Give a body
  when the reason is not clear from the subject.

Example:

```
feat(config): load servers from the global file

ncp reads the mcpServers map from ~/.config/ncp/mcp.json. A remote
server without a type field causes an error, because agent tools
give different meanings to a url with no type.
```
