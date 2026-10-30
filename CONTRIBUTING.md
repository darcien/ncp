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

An `ncp` RFC records a design that the maintainer must accept: a new or
removed command, a new configuration key, a new format of stored data, a
breaking change, or a change to a MUST rule of `docs/requirements.md`. A
smaller choice needs only the reason in the commit body.

- Path: `docs/rfcs/NNNN-slug.md`. Use the next free number. Do not use a
  number again.
- Front matter: `state` (`proposed`, `accepted`, `committed` or
  `abandoned`) and `supersedes` (a list of numbers).
- Write a new RFC with `state: proposed`. Only the maintainer accepts or
  abandons an RFC. Set `state: accepted` or `state: abandoned` only after
  the maintainer says so.
- Only an `accepted` RFC gives direction. A `proposed` RFC is a draft.
- The commit that completes the build also sets `state: committed`, and
  changes the documents that the build affects. After that
  commit, the RFC is frozen.
- Do not change a `committed` or `abandoned` RFC. A new design is a new
  RFC with `supersedes`.
- Do not start to build a change of this list before its RFC has
  `state: accepted`. Write the RFC first. If you are not sure that a
  change is on the list, ask the maintainer.
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

## Checks

- The git pre-commit hook runs the checks. All must pass. Do not skip the
  hook.
- The hook runs only the checks that the staged files can affect. For
  example, a change to the docs does not start the Rust builds. When a new
  kind of file can change the build or the tests, add it to the globs in
  `hk.pkl`.
- Run `mise run precommit` to run all checks on all files, for example after
  a change to `hk.pkl` or `mise.toml`.

## Commits

- Commit in small working chunks.
- Obey the version rule of `docs/release.md`. A breaking change needs an
  `accepted` RFC, and its `BREAKING CHANGE` footer gives the steps for the
  user.
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
