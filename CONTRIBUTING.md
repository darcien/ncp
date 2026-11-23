# Contributing to ncp

`ncp` is a command-line MCP client for one user on macOS and Linux. It is
written in Rust on the official SDK `rmcp`. It targets the MCP spec
`2026-07-28` at modelcontextprotocol.io, with fallback to `2025-11-25`.

Read the documents of the task before a change:

| Task | Read |
|---|---|
| Each change | This document and [STYLE.md](STYLE.md), the rules for all text |
| A command, an option, an output or a secret | [docs/command-development.md](docs/command-development.md) |
| A test of command output | [docs/command-tests.md](docs/command-tests.md) |
| A dependency or a Renovate pull request | [docs/dependencies.md](docs/dependencies.md) |
| A release | [docs/release.md](docs/release.md) |
| The live tests | [docs/live-tests.md](docs/live-tests.md) |
| A research report or a test report | [docs/research/README.md](docs/research/README.md) |
| An RFC | [docs/rfcs/README.md](docs/rfcs/README.md) |

## Set up

1. Install the tools: `mise install`.
2. Install the git hook: `hk install --mise`.
3. To make sure that the setup works, run `mise run check` and
   `mise run test`.

## Before you start

These changes need the maintainer to agree first:

- A change to a MUST rule of `docs/requirements.md` or to the order of
  the work.
- A new dependency that `deny.toml` does not accept.
- A change to the scope of secrets, or to how `ncp` stores or handles
  secrets (`docs/command-development.md`, "Secrets").
- A new network destination, or code from a new source for `ncp` binary itself.

## Documents

- `docs/requirements.md`, `docs/plan.md` and `docs/design/` are the MVP
  documents. They describe v0.1.0. Do not take a task, an order or a
  design from them.
- Exception: each MUST rule of `docs/requirements.md` stays in force until
  a rule in another document replaces it. The output formats in
  `docs/command-development.md`, "Streams and formats", replace the rule
  I1. When the code does not obey a MUST rule, the code has a defect. For
  each other difference between the code and an MVP document, the code is
  correct.
- When you need prior art, take it from the official MCP specification,
  SDKs and test parts, and from tools of established vendors (frontier
  AI labs and major editors). Use a small project of one person only as
  an example.

### Defaults

Write Rust like a clean, professional Elixir codebase: simple, functional
and explicit.

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
- **Small modules and functions** with one purpose each. Names describe the
  domain, not the mechanism.
- **Few dependencies.** Prefer `std` and the dependencies that are already in
  the workspace.

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

- The git pre-commit hook runs the checks. Do not skip the hook.
- The hook runs only the checks that the staged files can affect. For
  example, a change to the docs does not start the Rust builds. When a new
  kind of file can change the build or the tests, add it to the globs in
  `hk.pkl`.
- After a change to `hk.pkl` or `mise.toml`, run `mise run precommit` to
  run all checks on all files.
- The advisory check gives only a warning. It does not stop a commit, CI
  or a release. The RustSec database can change when the repository does
  not.

## Commits

- Use [Conventional Commits](https://www.conventionalcommits.org/):
  `type(scope): subject`. Types: `feat`, `fix`, `refactor`, `test`, `docs`,
  `build`, `chore`.
- Mark a breaking change (`docs/command-development.md`, "Version rule")
  with a `BREAKING CHANGE` footer that gives the steps for the user.
- Prefer commit in small working chunks.
- Give a body when the reason is not clear from the subject.

Example:

```
feat(config): load servers from the global file

ncp reads the mcpServers map from ~/.config/ncp/mcp.json. A remote
server without a type field causes an error, because agent tools
give different meanings to a url with no type.
```
