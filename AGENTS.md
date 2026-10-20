# AGENTS.md

`ncp` is a command-line MCP client for one user on macOS and Linux. It is
written in Rust on the official SDK `rmcp`.

## MCP spec and sources

- **MCP spec**: the official, versioned specification at
  modelcontextprotocol.io. `ncp` targets `2026-07-28`, with fallback to
  `2025-11-25`.
- Cite an external fact with its permanent source: a versioned MCP spec
  URL, an RFC section (for example "RFC 9207 §2.4"), or the name of an
  official conformance scenario. Open the source before you cite it. Do not
  cite from memory.
- For a rule that is specific to `ncp`, state the rule and its reason in
  words.
- Name tests by behavior, for example
  `falls_back_to_initialize_when_discover_times_out`.
- `docs/research/` holds dated reports. Each report is standalone and can
  be stale. No permanent document depends on a report. Delete a report
  when it has no use. A broken link in an MVP document is permitted.
- Write a test report only for a defect, or for a fact that changes a
  decision. Do not commit a result that only passes.
- Take prior art from the official MCP specification, SDKs and test parts,
  and from tools of established vendors (frontier AI labs and major editors).
- For command-line behavior (names, flags, files, trust), use uv and mise
  as the first references. Use another tool (for example direnv) only when
  uv and mise have no answer, and say so.
- Use a small project of one person only as an example.

## Secrets: no detection

`ncp` knows that a value is a secret only by fact:

- The value comes from `${NAME}` or from a credential store.
- The value is in a header field that RFC 9110 defines as credentials:
  `Authorization` (§11.6.2) or `Proxy-Authorization` (§11.7.2).
- The value is an OAuth token or client credential.

Every other value is not a secret, also when it looks like one. Do not
propose a detection of secrets by value, by pattern, or by a list of
names. Do not ask the user about it again. This rule is final.

## Command-line principles

- Choose in this order: the MCP spec, secure by default, good ergonomics,
  the practice of uv and mise, the simplest design.
- One CLI for people and agents: text on a TTY, JSON in a pipe. Results go
  to stdout. Diagnostics go to stderr.
- Names follow uv: full words, one name for each action, no aliases. Short
  forms: only `-h`, `-q`, `-v` and `-V`. No `NCP_*` variables.
- No command waits for input that it did not ask for, and each wait has a
  time limit, so that an agent cannot hang.
- Only `auth login` starts OAuth.
- MCP objects pass through with no change.

## Documents

- Permanent documents: `AGENTS.md`, `CHANGELOG.md`, `docs/release.md`
  and `docs/live-tests.md`. Code and other documents can cite them.
- The code and its tests are the reference for the current behavior.
- `docs/requirements.md`, `docs/plan.md` and `docs/design/` are the MVP
  documents. They describe v0.1.0. Do not take a task, an order or a
  design from them.
- Exception: each MUST rule of `docs/requirements.md` stays in force until
  a permanent document replaces it. When the code does not obey a MUST
  rule, the code has a defect. For each other difference between the code
  and an MVP document, the code is correct.
- In code, tests and comments, do not cite an MVP document, an `ncp` RFC
  (`docs/rfcs/`) or a research report, and do not use their IDs (for
  example `P2` or `B67`). These documents can go away or change form. An
  IETF RFC section is a permanent source (see "MCP spec and sources").
- A commit body can give the path of the `ncp` RFC that it builds.
- Write documents and comments for the current behavior. Do not write the
  history of a change in them: the commit has it.

## RFCs

An `ncp` RFC records a design that the user must accept: a new or removed
command, a new configuration key, a new format of stored data, a breaking
change, or a change to a MUST rule of `docs/requirements.md`. A smaller
choice needs only the reason in the commit body.

- Path: `docs/rfcs/NNNN-slug.md`. Use the next free number. Do not use a
  number again.
- Front matter: `state` (`proposed`, `accepted`, `committed` or
  `abandoned`) and `supersedes` (a list of numbers).
- Write a new RFC with `state: proposed`. Only the user accepts or
  abandons an RFC. Set `state: accepted` or `state: abandoned` only after
  the user says so.
- Only an `accepted` RFC gives direction. A `proposed` RFC is a draft.
- The commit that completes the build also sets `state: committed`, and
  changes the permanent documents that the build affects. After that
  commit, the RFC is frozen.
- Do not change a `committed` or `abandoned` RFC. A new design is a new
  RFC with `supersedes`.

## Workflow

- The backlog is in `.scratch/backlog/` (not tracked by git). Do the item
  that the user names. Do not choose the next item yourself.
- Commit in small working chunks.
- Before you commit a change to a command, an option, an output field or
  a message, get an adversarial review from a subagent. Do 3 rounds at
  most. Put the findings that remain in the backlog, and tell the user
  about each high finding.
- Obey the version rule of `docs/release.md`. A breaking change needs an
  `accepted` RFC, and its `BREAKING CHANGE` footer gives the steps for the
  user.
- Keep notes in `.scratch/` (not tracked by git). Update
  `.scratch/progress.md` at the end of each backlog item, and before a long
  pause: done, next, blocked.
- At the start of a session, read `.scratch/progress.md` if it exists.

## Stop and ask

Stop and ask the user before you:

- Change a MUST rule of `docs/requirements.md`, or the order of the work.
- Add a network destination other than crates.io, the Rust toolchain, and
  pinned packages from the MCP organization. A pinned package includes the
  dependencies that its lock file pins.
- Run code that does not come from these sources.
- Change how secrets are stored or handled.
- Add a dependency that `deny.toml` does not accept.
- Start to build a change of the list in "RFCs" before its RFC has
  `state: accepted`. Write the RFC first. If you are not sure that a
  change is on the list, ask.

The rules for network destinations and code apply to `ncp` and to the
development on this machine. The settings of the CI platform control the
network of the CI.

## Credential store in tests

Automated tests do not use the macOS Keychain or the Secret Service of the
host. The Keychain shows a prompt, and an agent cannot accept it. Use the
in-memory store or the opt-in file store in tests.

The live tests (`mise run live`, `docs/live-tests.md`) are the exception.
They use the login Keychain, a browser, the system settings and a real
server. Only the user runs them. An agent never runs them.

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
  parts. Do not call third-party servers. The live tests are the exception
  (see above).

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
