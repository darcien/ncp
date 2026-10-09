# AGENTS.md

`ncp` is a command-line MCP client for one user on macOS and Linux. It is
written in Rust on the official SDK `rmcp`.

## MCP spec and requirements

- **MCP spec**: the official, versioned specification at
  modelcontextprotocol.io. `ncp` targets `2026-07-28`, with fallback to
  `2025-11-25`.
- **Requirements**: `docs/target-requirements.md`. A temporary working
  document for the MVP. It goes away when `ncp` conforms to the MCP spec.
- When a change adds or changes behavior, update the requirements first. Put
  the requirements change in the same commit as the code, or in an earlier
  commit.
- Requirement IDs (for example `P2`) are for the requirements document only.
  Do not use them in code, tests, comments or commits.
- Cite permanent sources: a versioned MCP spec URL, an RFC section (for
  example "RFC 9207 §2.4"), or the name of an official conformance scenario.
- For a rule that is specific to `ncp`, state the rule and its reason in
  words.
- Name tests by behavior, for example
  `falls_back_to_initialize_when_discover_times_out`.
- Research and history are in `docs/research/`. Do not change old reports.
  Add a new report instead.

## Workflow

- Follow `docs/plan.md`. Do the milestones in order.
- You can break down the work for a milestone, and commit in small working chunk.
- Keep notes in `.scratch/` (not tracked by git). Update
  `.scratch/progress.md` at the end of each milestone, and before a long
  pause: done, next, blocked.
- At the start of a session, read `.scratch/progress.md` if it exists.

## Stop and ask

Stop and ask the user before you:

- Change a MUST requirement, or the plan order.
- Add a network destination other than crates.io, the Rust toolchain, and
  pinned packages from the MCP organization.
- Run code that does not come from these sources.
- Change how secrets are stored or handled.
- Add a dependency that `deny.toml` does not accept.
- Pass a milestone that the plan marks as a gate.

## Credential store in tests

Automated tests do not use the macOS Keychain or the Secret Service of the
host. The Keychain shows a prompt, and an agent cannot accept it. Use the
in-memory store or the opt-in file store in tests.

Tests of the Linux Secret Service store run only in the Linux harness
container. Each test run starts its own D-Bus session and keyring daemon in
the container.

## Checks

- The git pre-commit hook runs the checks. All must pass. Do not skip the
  hook. Install it one time with `hk install --mise`.
- The hook runs only the checks that the staged files can affect. For
  example, a change to the docs does not start the Rust builds. When a new
  kind of file can change the build or the tests, add it to the globs in
  `hk.pkl`.
- Run `mise run precommit` to run all checks on all files, for example after
  a change to `hk.pkl` or `mise.toml`.
- Tests use only local servers: stubs, rmcp fixtures, and official MCP test
  parts. Do not call third-party servers.

## Commits

- You can commit. Use the git identity of the user. Do not add a
  `Co-Authored-By` line or any other AI attribution.
- Use [Conventional Commits](https://www.conventionalcommits.org/):
  `type(scope): subject`. Types: `feat`, `fix`, `refactor`, `test`, `docs`,
  `build`, `chore`.
- Write the subject and the body in ASD-STE100 Simplified Technical English:
  - Use short sentences, active voice and simple tenses.
  - Describe the behavior and the reason for the change.
  - Do not repeat the code. Do not use code names as the only description.
  - Give a body when the reason is not clear from the subject.

Example:

```
feat(config): load servers from the user configuration file

ncp reads the mcpServers map from ~/.config/ncp/mcp.json. A remote
server without a type field causes an error, because agent tools
give different meanings to a url with no type.
```

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
- **Few dependencies.** Each new dependency needs a reason. Prefer `std` and
  the dependencies that are already in the workspace.
- `unsafe` is forbidden.

## Documentation

Write docs and comments in ASD-STE100 Simplified Technical English. Comments
explain why, not what.
